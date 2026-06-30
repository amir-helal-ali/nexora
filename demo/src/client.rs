//! NXP demo client.
//!
//! Connects to an NXP server, sends a `PING` frame, and prints the `PONG`
//! response. Demonstrates the basic client-side wire path.
//!
//! Run with: `cargo run --bin nxp-client -- 127.0.0.1:4433`

use bytes::Bytes;
use nxp_core::{Frame, FrameFlags, FrameHeader, Opcode, VERSION, NONCE_LEN};
use nxp_session::time::now_us;
use nxp_transport::NxpClient;
use std::net::SocketAddr;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();

    let addr: SocketAddr = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "127.0.0.1:4433".to_string())
        .parse()?;

    let client = NxpClient::new()?;
    tracing::info!("connecting to {}", addr);
    let mut conn = client.connect(addr, "nexora.internal").await?;
    tracing::info!("connected");

    // Send PING with a small payload (the current time as ASCII).
    let payload = format!("ping@{}", now_us()).into_bytes();
    let payload_len = payload.len() as u32;
    let frame = Frame {
        header: FrameHeader {
            version: VERSION,
            flags: FrameFlags::NONE,
            opcode: Opcode::Ping,
            stream_id: 1,
            request_id: 1,
            timestamp_us: now_us(),
            nonce: [0u8; NONCE_LEN],
            payload_len,
        },
        payload: Bytes::from(payload),
        auth_tag: [0u8; nxp_core::AUTH_TAG_LEN],
        signature: None,
    };

    conn.send_frame(&frame).await?;
    tracing::info!("PING sent ({}B payload)", frame.payload.len());

    let resp = conn.recv_frame().await?;
    tracing::info!(
        "PONG received: opcode={:?} stream={} req={} payload={}B",
        resp.header.opcode,
        resp.header.stream_id,
        resp.header.request_id,
        resp.payload.len()
    );
    if !resp.payload.is_empty() {
        let body = String::from_utf8_lossy(&resp.payload);
        println!("server echoed: {}", body);
    }
    Ok(())
}
