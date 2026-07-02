//! Nexora Auth smoke test — exercises the full auth flow end-to-end.

use nexora_auth::AuthService;
use nexora_core::{
    permissions::{Grant, Permission, Role},
    NexoraCore,
};
use nxp_core::Opcode;
use nxp_payload::Encoding;
use serde::{Deserialize, Serialize};
use std::sync::Arc;

#[derive(Debug, Serialize, Deserialize)]
struct LoginRequest {
    username: String,
    password: String,
    #[serde(default)]
    client: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
struct LoginResponse {
    token: String,
    token_expires_at_ns: i64,
    session_id: String,
    user_id: String,
    username: String,
}

#[derive(Debug, Serialize, Deserialize)]
struct RefreshRequest {
    token: String,
}

#[derive(Debug, Serialize, Deserialize)]
struct RefreshResponse {
    token: String,
    token_expires_at_ns: i64,
}

#[derive(Debug, Serialize, Deserialize)]
struct LogoutRequest {
    token: String,
    #[serde(default)]
    session_id: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
struct LogoutResponse {
    ok: bool,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    println!("=== Nexora Auth Smoke Test ===\n");

    // Bootstrap.
    let core = Arc::new(NexoraCore::new());
    core.permissions.register_role(Role {
        id: "admin".into(),
        description: "Full admin".into(),
        grants: vec![Grant { permission: Permission::Admin, resource: "*".into() }],
    });
    let auth = Arc::new(AuthService::new(core.clone()));
    let handler = auth.clone().handler();

    // ----- 1. Create a user -----
    println!("[1] Creating user 'alice'...");
    auth.users
        .create("alice", "hunter2", Some("alice@nexora.io".into()), vec!["admin".into()])?;
    println!("    -> user_count = {}", auth.users.user_count());
    println!("    -> principal auto-registered: {} principals", core.permissions.principal_count());
    println!("    OK\n");

    // ----- 2. LOGIN with correct password -----
    println!("[2] Login with correct password...");
    let req = LoginRequest {
        username: "alice".into(),
        password: "hunter2".into(),
        client: Some("smoke-test".into()),
    };
    let payload = rmp_serde::to_vec_named(&req)?;
    let resp = handler.dispatch(Opcode::AuthLogin, &payload, Encoding::MessagePack).await?;
    let login: LoginResponse = rmp_serde::from_slice(&resp)?;
    println!("    -> token length: {}", login.token.len());
    println!("    -> session_id: {}", login.session_id);
    println!("    -> username: {}", login.username);
    println!("    OK\n");

    // ----- 3. LOGIN with wrong password -----
    println!("[3] Login with WRONG password...");
    let req = LoginRequest {
        username: "alice".into(),
        password: "WRONG".into(),
        client: None,
    };
    let payload = rmp_serde::to_vec_named(&req)?;
    let err = handler.dispatch(Opcode::AuthLogin, &payload, Encoding::MessagePack).await.unwrap_err();
    println!("    -> denied: scope={:?} code=0x{:04X} msg=\"{}\"", err.scope, err.code, err.message);
    assert_eq!(err.scope, nxp_core::ErrorScope::Auth);
    println!("    OK\n");

    // ----- 4. REFRESH the token -----
    println!("[4] Refreshing token...");
    let req = RefreshRequest { token: login.token.clone() };
    let payload = rmp_serde::to_vec_named(&req)?;
    let resp = handler.dispatch(Opcode::AuthRefresh, &payload, Encoding::MessagePack).await?;
    let refreshed: RefreshResponse = rmp_serde::from_slice(&resp)?;
    println!("    -> new token length: {}", refreshed.token.len());
    assert_ne!(refreshed.token, login.token);
    println!("    OK (new token issued, old invalidated)\n");

    // ----- 5. Old token is now invalid -----
    println!("[5] Verifying old token is revoked...");
    let req = RefreshRequest { token: login.token.clone() };
    let payload = rmp_serde::to_vec_named(&req)?;
    let err = handler.dispatch(Opcode::AuthRefresh, &payload, Encoding::MessagePack).await.unwrap_err();
    println!("    -> denied: scope={:?} code=0x{:04X}", err.scope, err.code);
    assert_eq!(err.scope, nxp_core::ErrorScope::Auth);
    println!("    OK\n");

    // ----- 6. LOGOUT -----
    println!("[6] Logging out...");
    let req = LogoutRequest {
        token: refreshed.token.clone(),
        session_id: Some(login.session_id.clone()),
    };
    let payload = rmp_serde::to_vec_named(&req)?;
    let resp = handler.dispatch(Opcode::AuthLogout, &payload, Encoding::MessagePack).await?;
    let parsed: LogoutResponse = rmp_serde::from_slice(&resp)?;
    println!("    -> ok = {}", parsed.ok);
    assert!(parsed.ok);
    println!("    OK\n");

    // ----- 7. Token no longer works after logout -----
    println!("[7] Token revoked after logout...");
    let req = RefreshRequest { token: refreshed.token.clone() };
    let payload = rmp_serde::to_vec_named(&req)?;
    let err = handler.dispatch(Opcode::AuthRefresh, &payload, Encoding::MessagePack).await.unwrap_err();
    println!("    -> denied: code=0x{:04X}", err.code);
    println!("    OK\n");

    // ----- 8. Events emitted -----
    println!("[8] Verifying events were emitted...");
    let events = core.events.replay_filtered(0, "user.");
    println!("    -> {} user.* events:", events.len());
    for e in &events {
        println!("       - id={} name={}", e.id, e.name);
    }
    assert!(events.iter().any(|e| e.name == "user.created"));
    assert!(events.iter().any(|e| e.name == "user.logged_in"));
    assert!(events.iter().any(|e| e.name == "user.logged_out"));
    println!("    OK\n");

    // ----- 9. Final state -----
    println!("=== Final State ===");
    println!("  users:        {}", auth.users.user_count());
    println!("  sessions:     {}", auth.sessions.session_count());
    println!("  principals:   {}", core.permissions.principal_count());
    println!("  events:       {}", core.events.published_count());
    println!("\nAll auth smoke tests passed.");
    Ok(())
}
