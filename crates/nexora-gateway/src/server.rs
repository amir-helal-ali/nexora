//! Gateway HTTP server.
//!
//! Boots an `axum` server that exposes the Nexora platform over HTTP. The
//! server translates every HTTP request into an NXP-style dispatch (in v0.1
//! this is in-process; in v0.2+ it will be a real NXP frame over QUIC to a
//! remote Core/Auth service).

use crate::middleware::{require_token, AuthMiddleware};
use crate::routes::{
    auth_login, auth_logout, auth_refresh, billing_cancel_subscription, billing_create_invoice,
    billing_create_payment, billing_create_subscription, billing_get_invoice,
    billing_list_invoices, billing_list_payments, billing_list_subscriptions,
    billing_succeed_payment, billing_stats, cluster_heartbeat, cluster_list, cluster_pick,
    cluster_register, cluster_stats, core_event_stream, core_get_module, core_health,
    core_list_modules, core_list_sessions, core_ping, core_publish_event, core_replay_events,
    graphql_handler, graphql_handler_playground, health, marketplace_check_updates,
    marketplace_get, marketplace_install, marketplace_list, marketplace_list_installed,
    marketplace_process_auto_updates, marketplace_publish, marketplace_rollback_package,
    marketplace_search, marketplace_uninstall, marketplace_update_package, notifications_delete,
    notifications_list, notifications_mark_all_read, notifications_mark_read, notifications_send,
    notifications_stats, notifications_unread_count, openapi, workflow_get, workflow_list,
    workflow_list_executions, workflow_register, workflow_stats, workflow_trigger, ws_handler,
    GatewayState,
};
use axum::{
    middleware::from_fn_with_state,
    routing::{get, post},
    Router,
};
use nexora_auth::AuthService;
use nexora_core::NexoraCore;
use std::net::SocketAddr;
use std::sync::Arc;
use tower_http::{cors::CorsLayer, limit::RequestBodyLimitLayer, trace::TraceLayer};

/// The gateway HTTP server. Owns the listeners + dispatches to handlers.
pub struct GatewayServer {
    /// Shared state (handlers + flags).
    pub state: GatewayState,
}

impl std::fmt::Debug for GatewayServer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("GatewayServer")
            .field("state", &self.state)
            .finish()
    }
}

impl GatewayServer {
    /// Construct a new gateway wrapping all services.
    pub fn new(
        core: Arc<NexoraCore>,
        auth: Arc<AuthService>,
        marketplace: Arc<nexora_marketplace::MarketplaceService>,
        billing: Arc<nexora_billing::BillingService>,
        workflow: Arc<nexora_workflow::WorkflowService>,
        cluster: Arc<nexora_cluster::ClusterService>,
        notifications: Arc<nexora_notifications::NotificationService>,
    ) -> Self {
        let auth_handler = Arc::new(nexora_auth::AuthHandler::new(auth.clone()));
        let core_handler = Arc::new(nexora_core::CoreHandler::new(core.clone()));
        let marketplace_handler = Arc::new(marketplace.handler());
        let billing_handler = Arc::new(billing.handler());
        let workflow_handler = Arc::new(workflow.handler());
        let cluster_handler = Arc::new(cluster.handler());
        // Build the GraphQL schema.
        let graphql_schema = nexora_graphql::build_schema(core.clone());
        Self {
            state: GatewayState {
                auth: auth_handler,
                core: core_handler,
                marketplace: marketplace_handler,
                billing: billing_handler,
                workflow: workflow_handler,
                cluster: cluster_handler,
                notifications,
                graphql: Some(Arc::new(graphql_schema)),
                ready: true,
            },
        }
    }

    /// Build the router with all routes + middleware.
    pub fn router(&self) -> Router {
        let state = self.state.clone();
        let auth_middleware = AuthMiddleware::new(self.state.auth.service().clone());

        // Public routes — no token required (WebSocket does its own auth via ?token=).
        let public_routes = Router::new()
            .route("/api/health", get(health))
            .route("/api/openapi.json", get(openapi))
            .route("/api/auth/login", post(auth_login))
            .route("/api/auth/refresh", post(auth_refresh))
            .route("/api/ws", get(ws_handler))
            // GraphQL endpoint (POST for queries/mutations, GET for Playground HTML)
            .route("/api/graphql", post(graphql_handler).get(graphql_handler_playground));

        // Protected routes — Bearer token required.
        let protected_routes = Router::new()
            .route("/api/auth/logout", post(auth_logout))
            .route("/api/core/ping", post(core_ping))
            .route("/api/core/events", get(core_replay_events).post(core_publish_event))
            .route("/api/core/events/stream", get(core_event_stream))
            .route("/api/core/modules", get(core_list_modules))
            .route("/api/core/modules/:id", get(core_get_module))
            .route("/api/core/sessions", get(core_list_sessions))
            .route("/api/core/health", get(core_health))
            // Marketplace routes
            .route("/api/marketplace/packages", get(marketplace_list).post(marketplace_publish))
            .route("/api/marketplace/packages/search", get(marketplace_search))
            .route("/api/marketplace/packages/:id", get(marketplace_get))
            .route("/api/marketplace/packages/:id/install", post(marketplace_install))
            .route("/api/marketplace/packages/:id/uninstall", post(marketplace_uninstall))
            .route("/api/marketplace/packages/:id/update", post(marketplace_update_package))
            .route("/api/marketplace/packages/:id/rollback", post(marketplace_rollback_package))
            .route("/api/marketplace/installed", get(marketplace_list_installed))
            .route("/api/marketplace/updates/check", get(marketplace_check_updates))
            .route("/api/marketplace/updates/process", post(marketplace_process_auto_updates))
            // Billing routes
            .route("/api/billing/invoices", get(billing_list_invoices).post(billing_create_invoice))
            .route("/api/billing/invoices/:id", get(billing_get_invoice))
            .route("/api/billing/payments", get(billing_list_payments).post(billing_create_payment))
            .route("/api/billing/payments/:id/succeed", post(billing_succeed_payment))
            .route("/api/billing/subscriptions", get(billing_list_subscriptions).post(billing_create_subscription))
            .route("/api/billing/subscriptions/:id/cancel", post(billing_cancel_subscription))
            .route("/api/billing/stats", get(billing_stats))
            // Workflow routes
            .route("/api/workflows", get(workflow_list).post(workflow_register))
            .route("/api/workflows/:id", get(workflow_get))
            .route("/api/workflows/:id/trigger", post(workflow_trigger))
            .route("/api/workflows/executions", get(workflow_list_executions))
            .route("/api/workflows/stats", get(workflow_stats))
            // Cluster routes
            .route("/api/cluster/nodes", get(cluster_list).post(cluster_register))
            .route("/api/cluster/nodes/:id/heartbeat", post(cluster_heartbeat))
            .route("/api/cluster/stats", get(cluster_stats))
            .route("/api/cluster/pick", get(cluster_pick))
            // Notification routes
            .route("/api/notifications", get(notifications_list).post(notifications_send))
            .route("/api/notifications/unread-count", get(notifications_unread_count))
            .route("/api/notifications/stats", get(notifications_stats))
            .route("/api/notifications/:id/read", post(notifications_mark_read))
            .route("/api/notifications/read-all", post(notifications_mark_all_read))
            .route("/api/notifications/:id", axum::routing::delete(notifications_delete))
            .layer(from_fn_with_state(auth_middleware, require_token));

        Router::new()
            .merge(public_routes)
            .merge(protected_routes)
            .layer(CorsLayer::permissive())
            .layer(TraceLayer::new_for_http())
            .layer(RequestBodyLimitLayer::new(16 * 1024 * 1024)) // 16 MiB max body
            .with_state(state)
    }

    /// Bind + serve on the given address. Blocks until shutdown.
    pub async fn serve(self, addr: SocketAddr) -> anyhow::Result<()> {
        let app = self.router();
        let listener = tokio::net::TcpListener::bind(addr).await?;
        tracing::info!("Nexora API Gateway listening on http://{}", addr);
        axum::serve(listener, app)
            .with_graceful_shutdown(shutdown_signal())
            .await?;
        Ok(())
    }
}

async fn shutdown_signal() {
    let ctrl_c = async {
        tokio::signal::ctrl_c()
            .await
            .expect("failed to install Ctrl-C handler");
    };

    #[cfg(unix)]
    let terminate = async {
        tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
            .expect("failed to install signal handler")
            .recv()
            .await;
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        _ = ctrl_c => {},
        _ = terminate => {},
    }
    tracing::info!("Shutdown signal received");
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::routes::LoginBody;
    use axum::body::Body;
    use axum::http::{Request, StatusCode};
    use nexora_core::permissions::{Grant, Permission, Role};
    use tower::ServiceExt;

    fn setup_server() -> GatewayServer {
        let core = Arc::new(NexoraCore::new());
        core.permissions.register_role(Role {
            id: "admin".into(),
            description: "admin".into(),
            grants: vec![Grant { permission: Permission::Admin, resource: "*".into() }],
        });
        let auth = Arc::new(AuthService::new(core.clone()));
        auth.users
            .create("alice", "hunter2", None, vec!["admin".into()])
            .unwrap();
        let marketplace = Arc::new(nexora_marketplace::MarketplaceService::new(core.clone()));
        let billing = Arc::new(nexora_billing::BillingService::new(core.clone()));
        let workflow = Arc::new(nexora_workflow::WorkflowService::new(core.clone()));
        let cluster = Arc::new(nexora_cluster::ClusterService::new(core.clone()));
        let notifications = Arc::new(nexora_notifications::NotificationService::new());
        GatewayServer::new(core, auth, marketplace, billing, workflow, cluster, notifications)
    }

    #[tokio::test]
    async fn health_endpoint() {
        let server = setup_server();
        let app = server.router();
        let resp = app
            .oneshot(Request::builder().uri("/api/health").body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn openapi_endpoint() {
        let server = setup_server();
        let app = server.router();
        let resp = app
            .oneshot(Request::builder().uri("/api/openapi.json").body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn login_endpoint_success() {
        let server = setup_server();
        let app = server.router();
        let body = serde_json::to_string(&LoginBody {
            username: "alice".into(),
            password: "hunter2".into(),
            client: None,
        })
        .unwrap();
        let resp = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/auth/login")
                    .header("content-type", "application/json")
                    .body(Body::from(body))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn login_endpoint_wrong_password() {
        let server = setup_server();
        let app = server.router();
        let body = serde_json::to_string(&LoginBody {
            username: "alice".into(),
            password: "WRONG".into(),
            client: None,
        })
        .unwrap();
        let resp = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/auth/login")
                    .header("content-type", "application/json")
                    .body(Body::from(body))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn protected_route_without_token_returns_401() {
        let server = setup_server();
        let app = server.router();
        let resp = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/core/ping")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn protected_route_with_valid_token_succeeds() {
        let server = setup_server();
        // Login first to get a token.
        let app = server.router();
        let body = serde_json::to_string(&LoginBody {
            username: "alice".into(),
            password: "hunter2".into(),
            client: None,
        })
        .unwrap();
        let resp = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/auth/login")
                    .header("content-type", "application/json")
                    .body(Body::from(body))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let body_bytes = axum::body::to_bytes(resp.into_body(), 1024 * 1024).await.unwrap();
        let login_resp: serde_json::Value = serde_json::from_slice(&body_bytes).unwrap();
        let token = login_resp["token"].as_str().unwrap().to_string();

        // Now call protected /api/core/ping with the token.
        let resp = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/core/ping")
                    .header("authorization", format!("Bearer {}", token))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
    }
}
