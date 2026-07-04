//! Nexora Core demo server.
//!
//! A complete end-to-end demonstration of Nexora Core: starts an NXP server
//! that accepts connections, dispatches commands to Core subsystems, and
//! publishes events.
//!
//! Run with: `cargo run --bin nexora-core-demo -- 127.0.0.1:4433`

use bytes::Bytes;
use nexora_core::{CoreHandler, NexoraCore, permissions::{Permission, Principal, PrincipalKind, Role, Grant}};
use nxp_core::{Frame, FrameFlags, FrameHeader, Opcode, VERSION, NONCE_LEN, AUTH_TAG_LEN};
use nxp_payload::Encoding;
use nxp_session::time::now_us;
use nxp_transport::NxpServer;
use std::net::SocketAddr;
use std::sync::Arc;

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

    // ---- Bootstrap the Core with sample data ----
    let core = Arc::new(NexoraCore::new());

    // Register an admin role + a principal.
    core.permissions.register_role(Role {
        id: "admin".into(),
        description: "Full admin".into(),
        grants: vec![
            Grant { permission: Permission::Admin,   resource: "*".into() },
            Grant { permission: Permission::Read,    resource: "*".into() },
            Grant { permission: Permission::Write,   resource: "*".into() },
            Grant { permission: Permission::Create,  resource: "*".into() },
            Grant { permission: Permission::Delete,  resource: "*".into() },
            Grant { permission: Permission::Execute, resource: "*".into() },
        ],
    });
    core.permissions.register_principal(Principal {
        id: "demo-admin".into(),
        name: "Demo Admin".into(),
        kind: PrincipalKind::User,
        roles: vec!["admin".to_string()].into_iter().collect(),
    });
    tracing::info!("Core bootstrapped; registered principal 'demo-admin' with admin role");

    let handler = Arc::new(CoreHandler::new(core.clone()));

    // ---- Start the NXP server ----
    let server = NxpServer::bind(addr).await?;
    let local = server.local_addr()?;
    tracing::info!("Nexora Core listening on {}", local);
    tracing::info!("Try: nxp ping {}", local);

    // ---- Health monitor: report Core subsystem health ----
    {
        let health_core = core.clone();
        tokio::spawn(async move {
            loop {
                health_core.health.report("module_manager", nexora_core::HealthStatus::Healthy, None);
                health_core.health.report("event_bus", nexora_core::HealthStatus::Healthy, None);
                health_core.health.report("permission_engine", nexora_core::HealthStatus::Healthy, None);
                health_core.health.report("service_registry", nexora_core::HealthStatus::Healthy, None);
                tokio::time::sleep(std::time::Duration::from_secs(15)).await;
            }
        });
    }

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
                let Some(mut conn) = conn else { continue };
                let handler = handler.clone();
                tokio::spawn(async move {
                    loop {
                        match conn.recv_frame().await {
                            Ok(frame) => {
                                tracing::info!(
                                    "RX opcode={:?} stream={} req={} payload={}B",
                                    frame.header.opcode,
                                    frame.header.stream_id,
                                    frame.header.request_id,
                                    frame.payload.len()
                                );
                                let encoding = if frame.header.flags.contains(FrameFlags::COMPACT) {
                                    Encoding::Cbor
                                } else {
                                    Encoding::MessagePack
                                };
                                match handler.dispatch(frame.header.opcode, &frame.payload, encoding).await {
                                    Ok(resp_bytes) => {
                                        let resp = Frame {
                                            header: FrameHeader {
                                                version: VERSION,
                                                flags: FrameFlags::NONE,
                                                opcode: match frame.header.opcode {
                                                    Opcode::Ping => Opcode::Pong,
                                                    Opcode::RegisterService => Opcode::Ack,
                                                    Opcode::DiscoverService => Opcode::Ack,
                                                    Opcode::SubscribeEvent => Opcode::Ack,
                                                    Opcode::PublishEvent => Opcode::Ack,
                                                    Opcode::ReplayEvents => Opcode::Ack,
                                                    Opcode::ExecuteCommand => Opcode::Ack,
                                                    _ => Opcode::Ack,
                                                },
                                                stream_id: frame.header.stream_id,
                                                request_id: frame.header.request_id,
                                                timestamp_us: now_us(),
                                                nonce: [0u8; NONCE_LEN],
                                                payload_len: resp_bytes.len() as u32,
                                            },
                                            payload: Bytes::from(resp_bytes),
                                            auth_tag: [0u8; AUTH_TAG_LEN],
                                            signature: None,
                                        };
                                        if let Err(e) = conn.send_frame(&resp).await {
                                            tracing::warn!("TX failed: {}", e);
                                            break;
                                        }
                                    }
                                    Err(e) => {
                                        tracing::warn!("dispatch error: {}", e);
                                        // Send an ERROR frame back.
                                        let err_payload = e.encode_msgpack().unwrap_or_default();
                                        let resp = Frame {
                                            header: FrameHeader {
                                                version: VERSION,
                                                flags: FrameFlags::ERROR,
                                                opcode: Opcode::Error,
                                                stream_id: frame.header.stream_id,
                                                request_id: frame.header.request_id,
                                                timestamp_us: now_us(),
                                                nonce: [0u8; NONCE_LEN],
                                                payload_len: err_payload.len() as u32,
                                            },
                                            payload: Bytes::from(err_payload),
                                            auth_tag: [0u8; AUTH_TAG_LEN],
                                            signature: None,
                                        };
                                        let _ = conn.send_frame(&resp).await;
                                    }
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
