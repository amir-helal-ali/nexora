//! Nexora Auth demo server.
//!
//! A complete end-to-end demonstration of the Auth/Identity Service:
//! starts an NXP server that accepts AUTH_LOGIN, AUTH_LOGOUT, AUTH_REFRESH
//! frames. User creation happens at boot via the AuthService API.
//!
//! Run with: `cargo run --bin auth-demo -- 127.0.0.1:4434`

use bytes::Bytes;
use nexora_auth::AuthService;
use nexora_core::{
    permissions::{Grant, Permission, Role},
    NexoraCore,
};
use nxp_core::{Frame, FrameFlags, FrameHeader, Opcode, VERSION, AUTH_TAG_LEN, NONCE_LEN};
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
        .unwrap_or_else(|| "127.0.0.1:4434".to_string())
        .parse()?;

    // ---- Bootstrap the Core ----
    let core = Arc::new(NexoraCore::new());

    // Register roles.
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
    core.permissions.register_role(Role {
        id: "viewer".into(),
        description: "Read-only".into(),
        grants: vec![Grant {
            permission: Permission::Read,
            resource: "project:*".into(),
        }],
    });

    // ---- Bootstrap the Auth service ----
    let auth = Arc::new(AuthService::new(core.clone()));

    // Pre-create a demo user.
    auth.users
        .create("admin", "admin123", Some("admin@nexora.io".into()), vec!["admin".into()])
        .expect("failed to create admin user");
    auth.users
        .create("viewer", "viewer123", None, vec!["viewer".into()])
        .expect("failed to create viewer user");

    tracing::info!("Auth service bootstrapped:");
    tracing::info!("  - users: {} (admin, viewer)", auth.users.user_count());
    tracing::info!("  - login with: username=admin password=admin123");

    let handler = auth.handler();

    // ---- Start the NXP server ----
    let server = NxpServer::bind(addr).await?;
    let local = server.local_addr()?;
    tracing::info!("Nexora Auth listening on {}", local);

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
                                let result = handler.dispatch(frame.header.opcode, &frame.payload, encoding).await;
                                let (resp_opcode, resp_bytes) = match result {
                                    Ok(b) => (Opcode::Ack, b),
                                    Err(e) => {
                                        tracing::warn!("dispatch error: {}", e);
                                        (Opcode::Error, e.encode_msgpack().unwrap_or_default())
                                    }
                                };
                                let resp = Frame {
                                    header: FrameHeader {
                                        version: VERSION,
                                        flags: if resp_opcode == Opcode::Error { FrameFlags::ERROR } else { FrameFlags::NONE },
                                        opcode: resp_opcode,
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
