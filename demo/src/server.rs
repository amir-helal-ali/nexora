//! NXP demo server.
//!
//! Listens for incoming QUIC connections, accepts a `PING` frame, and
//! responds with a `PONG` frame. This is the simplest end-to-end test
//! of the NXP wire format over QUIC.
//!
//! Run with: `cargo run --bin nxp-server -- 127.0.0.1:4433`

use nxp_core::{Frame, FrameFlags, FrameHeader, Opcode, VERSION, NONCE_LEN};
use nxp_session::time::now_us;
use nxp_transport::NxpServer;
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

    let server = NxpServer::bind(addr).await?;
    let local = server.local_addr()?;
    tracing::info!("NXP server listening on {}", local);

    // Set up Ctrl-C handler.
    let shutdown = tokio::signal::ctrl_c();
    tokio::pin!(shutdown);

    loop {
        tokio::select! {
            _ = &mut shutdown => {
                tracing::info!("Ctrl-C received; shutting down");
                server.close();
                break;
            }
            conn = server.accept() => {
                let Some(mut conn) = conn else {
                    tracing::debug!("accept returned None; continuing");
                    continue;
                };
                tracing::info!("accepted connection");
                tokio::spawn(async move {
                    loop {
                        match conn.recv_frame().await {
                            Ok(frame) => {
                                tracing::info!(
                                    "received frame: opcode={:?} stream={} req={} payload={}B",
                                    frame.header.opcode,
                                    frame.header.stream_id,
                                    frame.header.request_id,
                                    frame.payload.len()
                                );
                                if frame.header.opcode == Opcode::Ping {
                                    let pong = Frame {
                                        header: FrameHeader {
                                            version: VERSION,
                                            flags: FrameFlags::NONE,
                                            opcode: Opcode::Pong,
                                            stream_id: frame.header.stream_id,
                                            request_id: frame.header.request_id,
                                            timestamp_us: now_us(),
                                            nonce: [0u8; NONCE_LEN],
                                            payload_len: frame.payload.len() as u32,
                                        },
                                        payload: frame.payload.clone(),
                                        auth_tag: [0u8; nxp_core::AUTH_TAG_LEN],
                                        signature: None,
                                    };
                                    if let Err(e) = conn.send_frame(&pong).await {
                                        tracing::warn!("send_frame failed: {}", e);
                                        break;
                                    }
                                    tracing::info!("sent PONG");
                                } else if frame.header.opcode == Opcode::Bye {
                                    tracing::info!("client sent BYE; closing");
                                    break;
                                } else {
                                    tracing::warn!(
                                        "unexpected opcode: {:?}",
                                        frame.header.opcode
                                    );
                                }
                            }
                            Err(e) => {
                                tracing::info!("connection closed: {}", e);
                                break;
                            }
                        }
                    }
                });
            }
        }
    }
    Ok(())
}
