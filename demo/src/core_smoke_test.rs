//! Nexora Core smoke test — exercises every Core subsystem end-to-end.
//!
//! Run with: `cargo run --bin core-smoke-test`

use nexora_core::{
    CoreHandler, NexoraCore,
    permissions::{Grant, Permission, Principal, PrincipalKind, Role},
};
use nxp_core::Opcode;
use nxp_payload::Encoding;
use serde::{Deserialize, Serialize};
use std::sync::Arc;

#[derive(Debug, Serialize, Deserialize)]
struct PublishEventRequest {
    name: String,
    payload: String,
}

#[derive(Debug, Serialize, Deserialize)]
struct PublishEventResponse {
    event_id: u64,
}

#[derive(Debug, Serialize, Deserialize)]
struct ReplayEventsRequest {
    from_id: u64,
    #[serde(default)]
    filter: String,
}

#[derive(Debug, Serialize, Deserialize)]
struct ReplayEventsResponse {
    events: Vec<nexora_core::events::Event>,
}

#[derive(Debug, Serialize, Deserialize)]
struct ExecuteCommandRequest {
    principal_id: String,
    command: String,
    resource: String,
    permission: Permission,
    #[serde(default)]
    args: Args,
}

#[derive(Default, Debug, Serialize, Deserialize)]
struct Args {
    #[serde(default)]
    fields: std::collections::HashMap<String, String>,
}

#[derive(Debug, Serialize, Deserialize)]
struct ExecuteCommandResponse {
    ok: bool,
    message: String,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    println!("=== Nexora Core Smoke Test ===\n");

    // Bootstrap the Core.
    let core = Arc::new(NexoraCore::new());
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
        id: "tester".into(),
        name: "Test User".into(),
        kind: PrincipalKind::User,
        roles: vec!["admin".to_string()].into_iter().collect(),
    });

    let handler = CoreHandler::new(core.clone());

    // ----- 1. PING -----
    println!("[1] Sending PING...");
    let resp = handler.dispatch(Opcode::Ping, &[], Encoding::MessagePack).await?;
    println!("    -> got {} bytes back", resp.len());
    assert!(!resp.is_empty());
    println!("    OK\n");

    // ----- 2. PUBLISH_EVENT -----
    println!("[2] Publishing event 'test.started'...");
    let req = PublishEventRequest {
        name: "test.started".into(),
        payload: "smoke-test".into(),
    };
    let payload = rmp_serde::to_vec_named(&req)?;
    let resp = handler.dispatch(Opcode::PublishEvent, &payload, Encoding::MessagePack).await?;
    let parsed: PublishEventResponse = rmp_serde::from_slice(&resp)?;
    println!("    -> event_id = {}", parsed.event_id);
    assert!(parsed.event_id > 0);
    println!("    OK\n");

    // ----- 3. EXECUTE_COMMAND: module.install -----
    println!("[3] Installing module 'auth' via ExecuteCommand...");
    let mut args = Args::default();
    args.fields.insert("id".into(), "auth".into());
    args.fields.insert("name".into(), "Auth Module".into());
    args.fields.insert("owner".into(), "core".into());
    let req = ExecuteCommandRequest {
        principal_id: "tester".into(),
        command: "module.install".into(),
        resource: "command:module.install".into(),
        permission: Permission::Execute,
        args,
    };
    let payload = rmp_serde::to_vec_named(&req)?;
    let resp = handler.dispatch(Opcode::ExecuteCommand, &payload, Encoding::MessagePack).await?;
    let parsed: ExecuteCommandResponse = rmp_serde::from_slice(&resp)?;
    println!("    -> ok={}, message=\"{}\"", parsed.ok, parsed.message);
    assert!(parsed.ok);
    assert_eq!(core.modules.module_count(), 1);
    println!("    OK (module_count={})\n", core.modules.module_count());

    // ----- 4. EXECUTE_COMMAND: module.enable -----
    println!("[4] Enabling module 'auth'...");
    let mut args = Args::default();
    args.fields.insert("id".into(), "auth".into());
    let req = ExecuteCommandRequest {
        principal_id: "tester".into(),
        command: "module.enable".into(),
        resource: "command:module.enable".into(),
        permission: Permission::Execute,
        args,
    };
    let payload = rmp_serde::to_vec_named(&req)?;
    let resp = handler.dispatch(Opcode::ExecuteCommand, &payload, Encoding::MessagePack).await?;
    let parsed: ExecuteCommandResponse = rmp_serde::from_slice(&resp)?;
    println!("    -> ok={}, message=\"{}\"", parsed.ok, parsed.message);
    assert!(parsed.ok);
    assert_eq!(core.modules.get("auth").unwrap().state, nexora_core::ModuleState::Enabled);
    println!("    OK (state=enabled)\n");

    // ----- 5. REPLAY_EVENTS -----
    println!("[5] Replaying events from id=0...");
    let req = ReplayEventsRequest { from_id: 0, filter: String::new() };
    let payload = rmp_serde::to_vec_named(&req)?;
    let resp = handler.dispatch(Opcode::ReplayEvents, &payload, Encoding::MessagePack).await?;
    let parsed: ReplayEventsResponse = rmp_serde::from_slice(&resp)?;
    println!("    -> {} events replayed", parsed.events.len());
    // Expect at least: test.started + module.installed + module.enabled
    assert!(parsed.events.len() >= 3, "expected >= 3 events, got {}", parsed.events.len());
    for evt in &parsed.events {
        println!("       - id={} name={}", evt.id, evt.name);
    }
    println!("    OK\n");

    // ----- 6. Permission denial -----
    println!("[6] Verifying permission denial for unprivileged principal...");
    core.permissions.register_principal(Principal {
        id: "unpriv".into(),
        name: "Unprivileged".into(),
        kind: PrincipalKind::User,
        roles: std::collections::HashSet::new(),
    });
    let req = ExecuteCommandRequest {
        principal_id: "unpriv".into(),
        command: "module.list".into(),
        resource: "command:module.list".into(),
        permission: Permission::Execute,
        args: Args::default(),
    };
    let payload = rmp_serde::to_vec_named(&req)?;
    let result = handler.dispatch(Opcode::ExecuteCommand, &payload, Encoding::MessagePack).await;
    assert!(result.is_err(), "expected permission denial");
    let err = result.unwrap_err();
    println!("    -> denied: scope={:?} code=0x{:04X}", err.scope, err.code);
    assert_eq!(err.scope, nxp_core::ErrorScope::Authz);
    println!("    OK\n");

    // ----- 7. AI opcodes rejected -----
    println!("[7] Verifying AI opcodes are rejected (Part 11 deferred)...");
    let result = handler.dispatch(Opcode::AiRequest, &[], Encoding::MessagePack).await;
    assert!(result.is_err());
    let err = result.unwrap_err();
    println!("    -> rejected: code=0x{:04X} msg=\"{}\"", err.code, err.message);
    println!("    OK\n");

    // ----- 8. Final summary -----
    println!("=== Final Core State ===");
    println!("{:#?}", core);
    println!("\nAll smoke tests passed.");
    Ok(())
}
