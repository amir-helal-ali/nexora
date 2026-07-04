//! NXP frame format.
//!
//! See RFC §2.3. Every NXP message on the wire is a `Frame`. This module
//! implements zero-copy-friendly encode/decode over `bytes::BytesMut`.
//!
//! Frame layout (48-byte fixed header):
//!
//! ```text
//!  0                   1                   2                   3
//!  0 1 2 3 4 5 6 7 8 9 0 1 2 3 4 5 6 7 8 9 0 1 2 3 4 5 6 7 8 9 0 1
//! +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
//! | Magic (2B) | Ver (1B) | Flags (2B)   | Opcode (2B)           |
//! +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
//! |                       Stream ID (4B)                         |
//! +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
//! |                    Request ID    (8B)                        |
//! +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
//! |                      Timestamp (8B, μs)                      |
//! +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
//! |                        Nonce (12B)                           |
//! +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
//! |                    Payload Length (4B)                       |
//! +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
//! |                  Payload (variable, encrypted)               |
//! +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
//! |              Auth Tag (16B, ChaCha20-Poly1305)               |
//! +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
//! |              Signature (64B, Ed25519, optional)              |
//! +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
//! ```

use crate::error::{protocol_codes, NxpError, Result};
use crate::flags::FrameFlags;
use crate::opcode::Opcode;
use crate::version::VERSION;
use bytes::{Buf, BufMut, Bytes, BytesMut};
use serde::{Deserialize, Serialize};
use std::fmt;

/// Magic bytes identifying an NXP frame: ASCII `'N' 'X'` = `0x4E 0x58`.
pub const MAGIC: [u8; 2] = [0x4E, 0x58];

/// Maximum payload length (16 MiB). Larger payloads must be split across
/// multiple frames on a stream.
pub const MAX_PAYLOAD_LEN: usize = 16 * 1024 * 1024;

/// Fixed header size in bytes (everything up to and including the payload
/// length field). Does NOT include payload, auth tag, or signature.
pub const HEADER_LEN: usize = 2 + 1 + 2 + 2 + 4 + 8 + 8 + 12 + 4; // = 43

/// ChaCha20-Poly1305 auth tag length.
pub const AUTH_TAG_LEN: usize = 16;

/// Ed25519 signature length.
pub const SIGNATURE_LEN: usize = 64;

/// Per-frame AEAD nonce length.
pub const NONCE_LEN: usize = 12;

/// Frame header. The non-payload, non-auth-tag, non-signature portion of a
/// frame. Carries all routing, control, and integrity metadata.
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FrameHeader {
    /// Wire-format version. Must equal [`VERSION`].
    pub version: u8,
    /// Frame flag bitfield.
    pub flags: FrameFlags,
    /// Opcode identifying the command.
    pub opcode: Opcode,
    /// Multiplexing identifier (maps to QUIC stream).
    pub stream_id: u32,
    /// Unique per-request correlation ID.
    pub request_id: u64,
    /// Microseconds since UNIX epoch (UTC).
    pub timestamp_us: u64,
    /// Per-frame AEAD nonce. Must never be reused within a session.
    pub nonce: [u8; NONCE_LEN],
    /// Length in bytes of the ciphertext payload that follows the header.
    pub payload_len: u32,
}

impl fmt::Debug for FrameHeader {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("FrameHeader")
            .field("version", &self.version)
            .field("flags", &self.flags)
            .field("opcode", &self.opcode)
            .field("stream_id", &self.stream_id)
            .field("request_id", &self.request_id)
            .field("timestamp_us", &self.timestamp_us)
            .field("nonce", &hex::encode(self.nonce))
            .field("payload_len", &self.payload_len)
            .finish()
    }
}

/// A complete NXP frame: header + payload + auth tag + optional signature.
#[derive(Clone, PartialEq, Eq)]
pub struct Frame {
    /// Frame header.
    pub header: FrameHeader,
    /// Ciphertext payload.
    pub payload: Bytes,
    /// ChaCha20-Poly1305 auth tag (16 bytes).
    pub auth_tag: [u8; AUTH_TAG_LEN],
    /// Optional Ed25519 signature (64 bytes). Present iff
    /// `header.flags.contains(FrameFlags::SIGNED)`.
    pub signature: Option<[u8; SIGNATURE_LEN]>,
}

impl fmt::Debug for Frame {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Frame")
            .field("header", &self.header)
            .field("payload_len", &self.payload.len())
            .field("auth_tag", &hex::encode(self.auth_tag))
            .field(
                "signature",
                &self.signature.as_ref().map(hex::encode),
            )
            .finish()
    }
}

impl FrameHeader {
    /// Encode into the given `BytesMut` at its current write position.
    /// Writes exactly [`HEADER_LEN`] bytes.
    pub fn encode(&self, dst: &mut BytesMut) {
        dst.put_slice(&MAGIC);
        dst.put_u8(self.version);
        dst.put_u16(self.flags.bits());
        dst.put_u16(self.opcode.as_u16());
        dst.put_u32(self.stream_id);
        dst.put_u64(self.request_id);
        dst.put_u64(self.timestamp_us);
        dst.put_slice(&self.nonce);
        dst.put_u32(self.payload_len);
    }

    /// Decode a header from the given slice. Returns the parsed header plus
    /// the number of bytes consumed (always [`HEADER_LEN`]).
    ///
    /// Errors:
    /// - `BAD_MAGIC` — slice does not start with `0x4E58`
    /// - `BAD_VERSION` — wire-format version mismatch
    /// - `TRUNCATED_HEADER` — slice shorter than [`HEADER_LEN`]
    /// - `UNKNOWN_OPCODE` — opcode is not a known builtin and not in the
    ///   application namespace (0xC000–0xFFFF)
    pub fn decode(src: &[u8]) -> Result<(Self, usize)> {
        if src.len() < HEADER_LEN {
            return Err(NxpError::protocol(
                protocol_codes::TRUNCATED_HEADER,
                format!("got {} bytes, need {}", src.len(), HEADER_LEN),
            ));
        }
        let mut buf = &src[..HEADER_LEN];
        let mut magic = [0u8; 2];
        magic.copy_from_slice(&buf[..2]);
        buf.advance(2);
        if magic != MAGIC {
            return Err(NxpError::protocol(
                protocol_codes::BAD_MAGIC,
                format!("expected {:?}, got {:?}", MAGIC, magic),
            ));
        }
        let version = buf.get_u8();
        if version != VERSION {
            return Err(NxpError::protocol(
                protocol_codes::BAD_VERSION,
                format!("expected version {}, got {}", VERSION, version),
            ));
        }
        let flags = FrameFlags::from_bits(buf.get_u16());
        let opcode_raw = buf.get_u16();
        let opcode = Opcode::from_u16(opcode_raw).ok_or_else(|| {
            // Application namespace (0xC000–0xFFFF) is reserved for
            // marketplace-published packages and is not decoded by the
            // builtin enum. Such opcodes must be dispatched by the
            // Capability Registry, not by `FrameHeader::decode`.
            if opcode_raw >= 0xC000 {
                NxpError::protocol(
                    protocol_codes::UNKNOWN_OPCODE,
                    format!("application opcode 0x{:04X} requires Capability Registry", opcode_raw),
                )
            } else {
                NxpError::protocol(
                    protocol_codes::UNKNOWN_OPCODE,
                    format!("unknown builtin opcode 0x{:04X}", opcode_raw),
                )
            }
        })?;
        let stream_id = buf.get_u32();
        let request_id = buf.get_u64();
        let timestamp_us = buf.get_u64();
        let mut nonce = [0u8; NONCE_LEN];
        nonce.copy_from_slice(&buf[..NONCE_LEN]);
        buf.advance(NONCE_LEN);
        let payload_len = buf.get_u32();

        Ok((
            Self {
                version,
                flags,
                opcode,
                stream_id,
                request_id,
                timestamp_us,
                nonce,
                payload_len,
            },
            HEADER_LEN,
        ))
    }
}

impl Frame {
    /// Total on-wire size of this frame: header + payload + auth tag +
    /// signature (if present).
    pub fn wire_size(&self) -> usize {
        let sig = if self.signature.is_some() {
            SIGNATURE_LEN
        } else {
            0
        };
        HEADER_LEN + self.payload.len() + AUTH_TAG_LEN + sig
    }

    /// Encode the full frame into a `BytesMut`. Allocates exactly once.
    pub fn encode(&self) -> BytesMut {
        let mut dst = BytesMut::with_capacity(self.wire_size());
        self.header.encode(&mut dst);
        dst.put_slice(&self.payload);
        dst.put_slice(&self.auth_tag);
        if let Some(sig) = &self.signature {
            dst.put_slice(sig);
        }
        dst
    }

    /// Encode the full frame into a writer (e.g. an async stream).
    pub fn encode_to(&self, dst: &mut BytesMut) {
        self.header.encode(dst);
        dst.put_slice(&self.payload);
        dst.put_slice(&self.auth_tag);
        if let Some(sig) = &self.signature {
            dst.put_slice(sig);
        }
    }

    /// Decode a frame from a complete byte slice. The caller must ensure
    /// the slice contains the entire frame (use [`Frame::peek_required_len`]
    /// to determine how many bytes are needed from a partial buffer).
    pub fn decode(src: &[u8]) -> Result<Self> {
        let (header, _) = FrameHeader::decode(src)?;
        if header.payload_len as usize > MAX_PAYLOAD_LEN {
            return Err(NxpError::protocol(
                protocol_codes::PAYLOAD_TOO_LARGE,
                format!(
                    "payload_len={} exceeds MAX_PAYLOAD_LEN={}",
                    header.payload_len, MAX_PAYLOAD_LEN
                ),
            ));
        }
        let needs_sig = header.flags.contains(FrameFlags::SIGNED);
        let sig_len = if needs_sig { SIGNATURE_LEN } else { 0 };
        let total = HEADER_LEN + header.payload_len as usize + AUTH_TAG_LEN + sig_len;
        if src.len() < total {
            return Err(NxpError::protocol(
                protocol_codes::TRUNCATED_HEADER,
                format!(
                    "frame truncated: got {} bytes, need {}",
                    src.len(),
                    total
                ),
            ));
        }

        let mut offset = HEADER_LEN;
        let payload_end = offset + header.payload_len as usize;
        let payload = Bytes::copy_from_slice(&src[offset..payload_end]);
        offset = payload_end;

        let mut auth_tag = [0u8; AUTH_TAG_LEN];
        auth_tag.copy_from_slice(&src[offset..offset + AUTH_TAG_LEN]);
        offset += AUTH_TAG_LEN;

        let signature = if needs_sig {
            let mut sig = [0u8; SIGNATURE_LEN];
            sig.copy_from_slice(&src[offset..offset + SIGNATURE_LEN]);
            Some(sig)
        } else {
            None
        };

        Ok(Self {
            header,
            payload,
            auth_tag,
            signature,
        })
    }

    /// Inspect the first bytes of a buffer and return how many bytes must be
    /// read in total to obtain one complete frame. Returns `None` if the
    /// header itself isn't fully present yet.
    pub fn peek_required_len(src: &[u8]) -> Result<Option<usize>> {
        if src.len() < HEADER_LEN {
            return Ok(None);
        }
        let (header, _) = FrameHeader::decode(src)?;
        let sig_len = if header.flags.contains(FrameFlags::SIGNED) {
            SIGNATURE_LEN
        } else {
            0
        };
        Ok(Some(
            HEADER_LEN + header.payload_len as usize + AUTH_TAG_LEN + sig_len,
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_header() -> FrameHeader {
        FrameHeader {
            version: VERSION,
            flags: FrameFlags::ENCRYPTED,
            opcode: Opcode::Ping,
            stream_id: 7,
            request_id: 42,
            timestamp_us: 1_700_000_000_000_000,
            nonce: [0xAA; NONCE_LEN],
            payload_len: 16,
        }
    }

    #[test]
    fn header_roundtrip() {
        let h = sample_header();
        let mut buf = BytesMut::with_capacity(HEADER_LEN);
        h.encode(&mut buf);
        assert_eq!(buf.len(), HEADER_LEN);
        let (h2, n) = FrameHeader::decode(&buf).unwrap();
        assert_eq!(n, HEADER_LEN);
        assert_eq!(h, h2);
    }

    #[test]
    fn frame_roundtrip_no_signature() {
        let frame = Frame {
            header: sample_header(),
            payload: Bytes::from(vec![0u8; 16]),
            auth_tag: [0xBB; AUTH_TAG_LEN],
            signature: None,
        };
        let buf = frame.encode();
        let frame2 = Frame::decode(&buf).unwrap();
        assert_eq!(frame, frame2);
    }

    #[test]
    fn frame_roundtrip_with_signature() {
        let mut h = sample_header();
        h.flags = FrameFlags::ENCRYPTED | FrameFlags::SIGNED;
        let frame = Frame {
            header: h,
            payload: Bytes::from(vec![0u8; 16]),
            auth_tag: [0xBB; AUTH_TAG_LEN],
            signature: Some([0xCC; SIGNATURE_LEN]),
        };
        let buf = frame.encode();
        let frame2 = Frame::decode(&buf).unwrap();
        assert_eq!(frame, frame2);
    }

    #[test]
    fn rejects_bad_magic() {
        let mut h = sample_header();
        // We can't easily corrupt magic through the public API; build manually.
        let mut buf = BytesMut::with_capacity(HEADER_LEN);
        buf.put_slice(&[0x00, 0x00]); // bad magic
        buf.put_u8(h.version);
        buf.put_u16(h.flags.bits());
        buf.put_u16(h.opcode.as_u16());
        buf.put_u32(h.stream_id);
        buf.put_u64(h.request_id);
        buf.put_u64(h.timestamp_us);
        buf.put_slice(&h.nonce);
        buf.put_u32(h.payload_len);
        let err = FrameHeader::decode(&buf).unwrap_err();
        assert_eq!(err.code, protocol_codes::BAD_MAGIC);
    }

    #[test]
    fn rejects_bad_version() {
        let h = sample_header();
        let mut buf = BytesMut::with_capacity(HEADER_LEN);
        h.encode(&mut buf);
        // Overwrite the version byte (offset 2).
        buf[2] = 99;
        let err = FrameHeader::decode(&buf).unwrap_err();
        assert_eq!(err.code, protocol_codes::BAD_VERSION);
    }

    #[test]
    fn rejects_truncated_header() {
        let short = [0u8; 5];
        let err = FrameHeader::decode(&short).unwrap_err();
        assert_eq!(err.code, protocol_codes::TRUNCATED_HEADER);
    }

    #[test]
    fn rejects_payload_too_large() {
        let mut h = sample_header();
        h.payload_len = MAX_PAYLOAD_LEN as u32 + 1;
        let mut buf = BytesMut::new();
        h.encode(&mut buf);
        buf.put_slice(&[0u8; 16]); // some payload
        buf.put_slice(&[0u8; AUTH_TAG_LEN]);
        // Also overwrite the version byte to be valid (since h.encode wrote it).
        buf[2] = VERSION;
        let err = Frame::decode(&buf).unwrap_err();
        assert_eq!(err.code, protocol_codes::PAYLOAD_TOO_LARGE);
    }

    #[test]
    fn peek_required_len_partial() {
        let partial = [0u8; 5];
        assert_eq!(Frame::peek_required_len(&partial).unwrap(), None);

        let frame = Frame {
            header: sample_header(),
            payload: Bytes::from(vec![0u8; 16]),
            auth_tag: [0xBB; AUTH_TAG_LEN],
            signature: None,
        };
        let full = frame.encode();
        let len = Frame::peek_required_len(&full).unwrap().unwrap();
        assert_eq!(len, full.len());
    }
}
