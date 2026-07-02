//! Nexora Gateway demo server.
//!
//! Boots a complete HTTP gateway on top of Nexora Core + Auth. Pre-creates
//! a demo user so you can immediately test the full flow with curl:
//!
//! ```bash
//! # 1. Start the gateway (this binary)
//! cargo run --bin gateway-demo -- 127.0.0.1:8080
//!
//! # 2. Login
//! curl -X POST http://127.0.0.1:8080/api/auth/login \
//!   -H "Content-Type: application/json" \
//!   -d '{"username":"admin","password":"admin123"}'
//! # -> { "token": "...", "session_id": "...", ... }
//!
//! # 3. Use the token to call a protected route
//! curl -X POST http://127.0.0.1:8080/api/core/ping \
//!   -H "Authorization: Bearer <token>"
//! # -> { "pong": true }
//!
//! # 4. Publish an event
//! curl -X POST http://127.0.0.1:8080/api/core/events \
//!   -H "Authorization: Bearer <token>" \
//!   -H "Content-Type: application/json" \
//!   -d '{"name":"test.event","payload":"hello from curl"}'
//!
//! # 5. Replay events
//! curl "http://127.0.0.1:8080/api/core/events?from_id=0" \
//!   -H "Authorization: Bearer <token>"
//! ```

use nexora_auth::AuthService;
use nexora_core::permissions::{Grant, Permission, Role};
use nexora_core::NexoraCore;
use nexora_gateway::GatewayServer;
use nexora_marketplace::MarketplaceService;
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
        .unwrap_or_else(|| "127.0.0.1:8080".to_string())
        .parse()?;

    // Bootstrap Core + Auth.
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
    core.permissions.register_role(Role {
        id: "viewer".into(),
        description: "Read-only".into(),
        grants: vec![Grant {
            permission: Permission::Read,
            resource: "project:*".into(),
        }],
    });

    let auth = Arc::new(AuthService::new(core.clone()));
    auth.users
        .create("admin", "admin123", Some("admin@nexora.io".into()), vec!["admin".into()])
        .expect("failed to create admin user");
    auth.users
        .create("viewer", "viewer123", None, vec!["viewer".into()])
        .expect("failed to create viewer user");

    // Bootstrap Marketplace + pre-publish a demo package.
    let marketplace = Arc::new(MarketplaceService::new(core.clone()));
    // Bootstrap Billing.
    let billing = Arc::new(nexora_billing::BillingService::new(core.clone()));
    // Bootstrap Workflow Engine.
    let workflow = Arc::new(nexora_workflow::WorkflowService::new(core.clone()));
    let cluster = Arc::new(nexora_cluster::ClusterService::new(core.clone()));
    {
        use nexora_marketplace::package::{PackageBilling, PackageManifest, PackageType, ResourceLimits, Visibility};
        use nexora_marketplace::version::{Version, VersionRange};
        let mut manifest = PackageManifest {
            id: "com.nexora.demo.auth".into(),
            name: "Demo Auth Module".into(),
            version: Version::new(1, 0, 0),
            package_type: PackageType::Module,
            owner_public_key: "00".repeat(32),
            owner_name: "Nexora".into(),
            capabilities: vec!["nxp.command.execute".into()],
            resource_limits: ResourceLimits::default(),
            dependencies: vec![],
            nxp_capabilities: vec!["quic".into()],
            core_compatibility: VersionRange::Caret(Version::new(0, 1, 0)),
            billing: PackageBilling::Free,
            visibility: Visibility::Public,
            signature: "ff".repeat(64), // invalid signature (demo)
            description: "A demo Auth module for testing the marketplace.".into(),
            readme: "# Demo Auth Module\n\nThis is a demo package.".into(),
            tags: vec!["auth".into(), "demo".into()],
        };
        // Sign it so installation can succeed.
        let signing = ed25519_dalek::SigningKey::generate(&mut rand::rngs::OsRng);
        let verifying = signing.verifying_key();
        manifest.owner_public_key = hex::encode(verifying.to_bytes());
        nexora_marketplace::signature::sign_manifest(&mut manifest, &signing);
        marketplace
            .store
            .publish(manifest)
            .expect("failed to publish demo package");
    }

    tracing::info!("Gateway bootstrapped:");
    tracing::info!("  users: admin (admin123), viewer (viewer123)");
    tracing::info!("  marketplace: 1 demo package (com.nexora.demo.auth@1.0.0)");
    tracing::info!("  billing: ready");
    tracing::info!("  try: curl -X POST http://{}/api/auth/login \\", addr);
    tracing::info!("           -H 'Content-Type: application/json' \\");
    tracing::info!("           -d '{{\"username\":\"admin\",\"password\":\"admin123\"}}'");

    let notifications = std::sync::Arc::new(nexora_notifications::NotificationService::new());
    let server = GatewayServer::new(core, auth, marketplace, billing, workflow, cluster, notifications);
    server.serve(addr).await
}
