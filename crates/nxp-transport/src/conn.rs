//! NXP connection — frame-level read/write over a QUIC stream.

use bytes::BytesMut;
use nxp_core::{Frame, FrameFlags, FrameHeader, NxpError};
use quinn::{RecvStream, SendStream};
use std::fmt;

/// Error reading a frame from a stream.
#[derive(Debug, thiserror::Error)]
pub enum ReadFrameError {
    /// Underlying QUIC stream closed.
    #[error("quic stream closed: {0}")]
    StreamClosed(#[from] quinn::ReadError),
    /// Underlying QUIC write closed.
    #[error("quic write closed: {0}")]
    WriteClosed(#[from] quinn::WriteError),
    /// Frame decode failed.
    #[error("decode: {0}")]
    Decode(#[from] NxpError),
    /// Stream ended before a full frame arrived.
    #[error("unexpected eof")]
    UnexpectedEof,
}

/// Error from `read_exact` mapping.
impl From<quinn::ReadExactError> for ReadFrameError {
    fn from(e: quinn::ReadExactError) -> Self {
        match e {
            quinn::ReadExactError::FinishedEarly(_) => ReadFrameError::UnexpectedEof,
            quinn::ReadExactError::ReadError(e) => ReadFrameError::StreamClosed(e),
        }
    }
}

/// A bidirectional NXP stream. Owns the QUIC send and receive halves.
pub struct NxpConnection {
    /// Outgoing half.
    pub send: SendStream,
    /// Incoming half.
    pub recv: RecvStream,
}

impl fmt::Debug for NxpConnection {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("NxpConnection").finish_non_exhaustive()
    }
}

impl NxpConnection {
    /// Construct from a `quinn::Connection`'s bidirectional stream.
    pub fn from_streams(send: SendStream, recv: RecvStream) -> Self {
        Self { send, recv }
    }

    /// Send a frame. Encodes the frame and writes it to the QUIC stream.
    pub async fn send_frame(&mut self, frame: &Frame) -> Result<(), ReadFrameError> {
        let buf = frame.encode();
        self.send.write_all(&buf).await?;
        Ok(())
    }

    /// Receive a frame. Reads the header first, then the rest of the body.
    pub async fn recv_frame(&mut self) -> Result<Frame, ReadFrameError> {
        // Read header.
        let mut header_buf = vec![0u8; nxp_core::HEADER_LEN];
        self.recv.read_exact(&mut header_buf).await?;
        let (header, _) = FrameHeader::decode(&header_buf)?;

        // Read payload + auth tag + optional signature.
        let payload_len = header.payload_len as usize;
        let sig_len = if header.flags.contains(FrameFlags::SIGNED) {
            nxp_core::SIGNATURE_LEN
        } else {
            0
        };
        let body_len = payload_len + nxp_core::AUTH_TAG_LEN + sig_len;
        let mut body = vec![0u8; body_len];
        if !body.is_empty() {
            self.recv.read_exact(&mut body).await?;
        }

        // Reassemble and decode.
        let mut full = BytesMut::with_capacity(header_buf.len() + body.len());
        full.extend_from_slice(&header_buf);
        full.extend_from_slice(&body);
        let frame = Frame::decode(&full)?;
        Ok(frame)
    }
}
