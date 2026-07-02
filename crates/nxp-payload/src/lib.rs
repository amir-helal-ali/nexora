//! NXP payload layer.
//!
//! See RFC §2.4. Payloads are binary-serialized; MessagePack is the default,
//! CBOR is selectable per-frame via [`FrameFlags::COMPACT`].
//! JSON is forbidden for internal NXP communication.

#![forbid(unsafe_code)]
#![warn(missing_docs, rust_2018_idioms)]

use bytes::Bytes;
use nxp_core::FrameFlags;
use serde::de::DeserializeOwned;
use serde::Serialize;
use thiserror::Error;

/// Payload encoding format.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Encoding {
    /// MessagePack (default).
    MessagePack,
    /// CBOR (selected via `FrameFlags::COMPACT`).
    Cbor,
}

/// Encode/decode error.
#[derive(Debug, Error)]
pub enum PayloadError {
    /// MessagePack encode failed.
    #[error("msgpack encode: {0}")]
    MsgpackEncode(#[from] rmp_serde::encode::Error),
    /// MessagePack decode failed.
    #[error("msgpack decode: {0}")]
    MsgpackDecode(#[from] rmp_serde::decode::Error),
    /// CBOR encode failed.
    #[error("cbor encode: {0}")]
    CborEncode(#[from] cbor::ser::Error<std::io::Error>),
    /// CBOR decode failed.
    #[error("cbor decode: {0}")]
    CborDecode(#[from] cbor::de::Error<std::io::Error>),
}

/// Serialize `value` into payload bytes using the requested encoding.
pub fn encode<T: Serialize>(encoding: Encoding, value: &T) -> Result<Vec<u8>, PayloadError> {
    match encoding {
        Encoding::MessagePack => Ok(rmp_serde::to_vec_named(value)?),
        Encoding::Cbor => {
            let mut buf = Vec::with_capacity(64);
            cbor::into_writer(value, &mut buf)?;
            Ok(buf)
        }
    }
}

/// Deserialize a payload of the given encoding.
pub fn decode<T: DeserializeOwned>(encoding: Encoding, bytes: &[u8]) -> Result<T, PayloadError> {
    match encoding {
        Encoding::MessagePack => Ok(rmp_serde::from_slice(bytes)?),
        Encoding::Cbor => Ok(cbor::from_reader(bytes)?),
    }
}

/// Convert NXP frame flags into the encoding they imply.
pub fn encoding_for_flags(flags: FrameFlags) -> Encoding {
    if flags.contains(FrameFlags::COMPACT) {
        Encoding::Cbor
    } else {
        Encoding::MessagePack
    }
}

/// Convenience: encode directly into a `Bytes` for use as a frame payload.
pub fn encode_bytes<T: Serialize>(encoding: Encoding, value: &T) -> Result<Bytes, PayloadError> {
    encode(encoding, value).map(Bytes::from)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde::{Deserialize, Serialize};

    #[derive(Debug, Serialize, Deserialize, PartialEq)]
    struct Hello {
        version: u8,
        capabilities: Vec<String>,
    }

    #[test]
    fn msgpack_roundtrip() {
        let h = Hello {
            version: 1,
            capabilities: vec!["quic".into(), "zstd".into()],
        };
        let bytes = encode(Encoding::MessagePack, &h).unwrap();
        let h2: Hello = decode(Encoding::MessagePack, &bytes).unwrap();
        assert_eq!(h, h2);
    }

    #[test]
    fn cbor_roundtrip() {
        let h = Hello {
            version: 1,
            capabilities: vec!["quic".into(), "zstd".into()],
        };
        let bytes = encode(Encoding::Cbor, &h).unwrap();
        let h2: Hello = decode(Encoding::Cbor, &bytes).unwrap();
        assert_eq!(h, h2);
    }

    #[test]
    fn flags_select_encoding() {
        assert_eq!(
            encoding_for_flags(FrameFlags::NONE),
            Encoding::MessagePack
        );
        assert_eq!(
            encoding_for_flags(FrameFlags::COMPACT),
            Encoding::Cbor
        );
        assert_eq!(
            encoding_for_flags(FrameFlags::ENCRYPTED | FrameFlags::COMPACT),
            Encoding::Cbor
        );
    }

    #[test]
    fn msgpack_compact_vs_json() {
        let h = Hello {
            version: 1,
            capabilities: vec!["quic".into()],
        };
        let mp = encode(Encoding::MessagePack, &h).unwrap();
        let cbor = encode(Encoding::Cbor, &h).unwrap();
        let json_len = serde_json::to_vec(&h).unwrap().len();
        // Both binary formats must be smaller than JSON.
        assert!(mp.len() < json_len, "msgpack {} vs json {}", mp.len(), json_len);
        assert!(cbor.len() < json_len, "cbor {} vs json {}", cbor.len(), json_len);
    }
}
