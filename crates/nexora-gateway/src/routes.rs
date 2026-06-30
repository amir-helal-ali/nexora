//! HTTP routes — JSON endpoints that translate to NXP commands.
//!
//! Each route is an axum handler that:
//! 1. Parses the JSON body
//! 2. Encodes it as MessagePack (the NXP payload format)
//! 3. Dispatches via the appropriate handler (Auth or Core)
//! 4. Decodes the MessagePack response back to JSON
//! 5. Returns the JSON response

use crate::middleware::AuthContext;
use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::{
        sse::{Event as SseEvent, KeepAlive, Sse},
        IntoResponse, Json, Response,
    },
};
use nexora_auth::AuthHandler;
use nexora_billing::BillingHandler;
use nexora_core::CoreHandler;
use nexora_marketplace::MarketplaceHandler;
use nexora_workflow::WorkflowHandler;
use nxp_core::Opcode;
use nxp_payload::Encoding;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::convert::Infallible;
use std::sync::Arc;
use std::time::Duration;
use tokio_stream::{wrappers::BroadcastStream, StreamExt as _};

/// Shared gateway state: handlers + core reference.
#[derive(Clone)]
pub struct GatewayState {
    /// Auth handler (in-process; no NXP round-trip needed for v0.1).
    pub auth: Arc<AuthHandler>,
    /// Core handler (in-process).
    pub core: Arc<CoreHandler>,
    /// Marketplace handler (in-process).
    pub marketplace: Arc<MarketplaceHandler>,
    /// Billing handler (in-process).
    pub billing: Arc<BillingHandler>,
    /// Workflow handler (in-process).
    pub workflow: Arc<WorkflowHandler>,
    /// Whether the gateway is ready to serve traffic.
    pub ready: bool,
}

impl std::fmt::Debug for GatewayState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("GatewayState")
            .field("ready", &self.ready)
            .finish_non_exhaustive()
    }
}

// ==================================================================
// Health & OpenAPI
// ==================================================================

/// `GET /api/health` — gateway liveness probe.
pub async fn health() -> impl IntoResponse {
    Json(json!({
        "status": "ok",
        "service": "nexora-gateway",
        "version": env!("CARGO_PKG_VERSION"),
    }))
}

/// `GET /api/openapi.json` — OpenAPI 3.0 specification.
pub async fn openapi() -> impl IntoResponse {
    Json(crate::spec::openapi_spec())
}

// ==================================================================
// Auth routes (public — no token required)
// ==================================================================

/// Request body for `POST /api/auth/login`.
#[derive(Debug, Deserialize, Serialize)]
pub struct LoginBody {
    /// Username.
    pub username: String,
    /// Password.
    pub password: String,
    /// Optional client identifier (e.g. browser fingerprint).
    #[serde(default)]
    pub client: Option<String>,
}

/// `POST /api/auth/login` — exchange credentials for a session token.
pub async fn auth_login(
    State(state): State<GatewayState>,
    Json(body): Json<LoginBody>,
) -> Response {
    let req = serde_json::json!({
        "username": body.username,
        "password": body.password,
        "client": body.client,
    });
    let payload = match serde_json::to_vec(&req) {
        // We use JSON internally for the handler request shape because the
        // handler does its own MessagePack decode. To keep this MVP simple,
        // we encode the request as MessagePack directly here.
        Ok(_) => match rmp_serde::to_vec_named(&req) {
            Ok(b) => b,
            Err(e) => return error_response(StatusCode::INTERNAL_SERVER_ERROR, &e.to_string()),
        },
        Err(e) => return error_response(StatusCode::INTERNAL_SERVER_ERROR, &e.to_string()),
    };

    dispatch_auth(state, Opcode::AuthLogin, &payload).await
}

/// Request body for `POST /api/auth/logout`.
#[derive(Debug, Deserialize)]
pub struct LogoutBody {
    /// The token to revoke.
    pub token: String,
    /// Optional session ID.
    #[serde(default)]
    pub session_id: Option<String>,
}

/// `POST /api/auth/logout` — revoke a session token.
pub async fn auth_logout(
    State(state): State<GatewayState>,
    Json(body): Json<LogoutBody>,
) -> Response {
    let req = serde_json::json!({
        "token": body.token,
        "session_id": body.session_id,
    });
    let payload = match rmp_serde::to_vec_named(&req) {
        Ok(b) => b,
        Err(e) => return error_response(StatusCode::INTERNAL_SERVER_ERROR, &e.to_string()),
    };
    dispatch_auth(state, Opcode::AuthLogout, &payload).await
}

/// Request body for `POST /api/auth/refresh`.
#[derive(Debug, Deserialize)]
pub struct RefreshBody {
    /// The (still-valid) token to refresh.
    pub token: String,
}

/// `POST /api/auth/refresh` — exchange a valid token for a new one.
pub async fn auth_refresh(
    State(state): State<GatewayState>,
    Json(body): Json<RefreshBody>,
) -> Response {
    let req = serde_json::json!({ "token": body.token });
    let payload = match rmp_serde::to_vec_named(&req) {
        Ok(b) => b,
        Err(e) => return error_response(StatusCode::INTERNAL_SERVER_ERROR, &e.to_string()),
    };
    dispatch_auth(state, Opcode::AuthRefresh, &payload).await
}

// ==================================================================
// Core routes (protected — require Bearer token)
// ==================================================================

/// `POST /api/core/ping` — send a PING through the Core.
pub async fn core_ping(
    State(state): State<GatewayState>,
    _ctx: axum::Extension<AuthContext>,
) -> Response {
    dispatch_core(state, Opcode::Ping, &[]).await
}

/// Request body for `POST /api/core/events`.
#[derive(Debug, Deserialize)]
pub struct PublishEventBody {
    /// Event name (e.g. `project.created`).
    pub name: String,
    /// Event payload (string).
    pub payload: String,
}

/// `POST /api/core/events` — publish an event on the EventBus.
pub async fn core_publish_event(
    State(state): State<GatewayState>,
    _ctx: axum::Extension<AuthContext>,
    Json(body): Json<PublishEventBody>,
) -> Response {
    let req = serde_json::json!({
        "name": body.name,
        "payload": body.payload,
    });
    let payload = match rmp_serde::to_vec_named(&req) {
        Ok(b) => b,
        Err(e) => return error_response(StatusCode::INTERNAL_SERVER_ERROR, &e.to_string()),
    };
    dispatch_core(state, Opcode::PublishEvent, &payload).await
}

/// Query params for `GET /api/core/events`.
#[derive(Debug, Deserialize)]
pub struct ReplayEventsQuery {
    /// Replay events from this ID (inclusive). Defaults to 0.
    #[serde(default)]
    pub from_id: u64,
    /// Optional name-prefix filter (e.g. `user.`).
    #[serde(default)]
    pub filter: Option<String>,
}

/// `GET /api/core/events` — replay events from the EventBus.
pub async fn core_replay_events(
    State(state): State<GatewayState>,
    _ctx: axum::Extension<AuthContext>,
    Query(q): Query<ReplayEventsQuery>,
) -> Response {
    let req = serde_json::json!({
        "from_id": q.from_id,
        "filter": q.filter.unwrap_or_default(),
    });
    let payload = match rmp_serde::to_vec_named(&req) {
        Ok(b) => b,
        Err(e) => return error_response(StatusCode::INTERNAL_SERVER_ERROR, &e.to_string()),
    };
    dispatch_core(state, Opcode::ReplayEvents, &payload).await
}

/// Query params for `GET /api/core/events/stream` (SSE).
#[derive(Debug, Deserialize)]
pub struct EventStreamQuery {
    /// Optional name-prefix filter (e.g. `user.`).
    #[serde(default)]
    pub filter: Option<String>,
}

/// `GET /api/core/events/stream` — Server-Sent Events stream of live events.
///
/// Opens a long-lived SSE connection. Every event published to the EventBus
/// after the connection opens is pushed to the client as an SSE `event`
/// frame. Optional `filter` query param filters by event name prefix.
///
/// Per Part 7: "Real-time system: All updates are streaming-based."
pub async fn core_event_stream(
    State(state): State<GatewayState>,
    _ctx: axum::Extension<AuthContext>,
    Query(q): Query<EventStreamQuery>,
) -> Sse<impl tokio_stream::Stream<Item = Result<SseEvent, Infallible>>> {
    let filter = q.filter.unwrap_or_default();
    // Subscribe to the EventBus. The filter is applied per-event below so
    // we can support multiple subscribers with different filters on a single
    // broadcast channel.
    let subscriber = state.core.core().events.subscribe(filter.clone());

    // Convert the broadcast receiver into a stream.
    let stream = BroadcastStream::new(subscriber.rx).filter_map(move |result| {
        let evt = match result {
            Ok(e) => e,
            Err(_) => return None, // lagging subscriber — skip
        };
        // Apply the filter (empty filter = all events).
        if !filter.is_empty() && !evt.name.starts_with(&filter) {
            return None;
        }
        // Build an SSE event with the event data as JSON.
        let data = json!({
            "id": evt.id,
            "name": evt.name,
            "payload": match &evt.payload {
                nexora_core::events::EventPayload::Text(s) => s.clone(),
                nexora_core::events::EventPayload::Bytes(b) => hex::encode(b),
                nexora_core::events::EventPayload::Empty => String::new(),
            },
            "timestamp": evt.timestamp,
        });
        Some(Ok(SseEvent::default()
            .event(&evt.name)
            .data(data.to_string())))
    });

    Sse::new(stream)
        .keep_alive(KeepAlive::new()
            .interval(Duration::from_secs(5))
            .text("keep-alive"))
}

/// `GET /api/core/modules` — list installed modules.
pub async fn core_list_modules(
    State(state): State<GatewayState>,
    _ctx: axum::Extension<AuthContext>,
) -> Response {
    let modules = state.core.core().modules.list();
    Json(json!({
        "ok": true,
        "count": modules.len(),
        "modules": modules,
    }))
    .into_response()
}

/// `GET /api/core/modules/:id` — get a single module by ID.
pub async fn core_get_module(
    State(state): State<GatewayState>,
    _ctx: axum::Extension<AuthContext>,
    Path(id): Path<String>,
) -> Response {
    match state.core.core().modules.get(&id) {
        Some(m) => Json(json!({ "ok": true, "module": m })).into_response(),
        None => error_response(StatusCode::NOT_FOUND, &format!("module {} not found", id)),
    }
}

/// `GET /api/core/sessions` — list active sessions (admin/debug only).
pub async fn core_list_sessions(
    State(state): State<GatewayState>,
    _ctx: axum::Extension<AuthContext>,
) -> Response {
    let sessions = state.core.core().events.snapshot();
    Json(json!({
        "ok": true,
        "event_count": sessions.len(),
    }))
    .into_response()
}

/// `GET /api/core/health` — aggregate Core health snapshot.
pub async fn core_health(
    State(state): State<GatewayState>,
    _ctx: axum::Extension<AuthContext>,
) -> Response {
    let health = state.core.core().health.snapshot();
    Json(json!({
        "ok": true,
        "overall": format!("{}", state.core.core().health.status()),
        "subsystems": health,
    }))
    .into_response()
}

// ==================================================================
// Internal dispatch helpers
// ==================================================================

async fn dispatch_auth(state: GatewayState, opcode: Opcode, payload: &[u8]) -> Response {
    match state.auth.dispatch(opcode, payload, Encoding::MessagePack).await {
        Ok(resp_bytes) => {
            // Decode MessagePack response back to a JSON Value.
            let value: Value = match rmp_serde::from_slice(&resp_bytes) {
                Ok(v) => v,
                Err(e) => {
                    return error_response(
                        StatusCode::INTERNAL_SERVER_ERROR,
                        &format!("response decode: {}", e),
                    );
                }
            };
            Json(value).into_response()
        }
        Err(e) => {
            let status = match e.scope {
                nxp_core::ErrorScope::Auth => StatusCode::UNAUTHORIZED,
                nxp_core::ErrorScope::Authz => StatusCode::FORBIDDEN,
                nxp_core::ErrorScope::Protocol => StatusCode::BAD_REQUEST,
                _ => StatusCode::INTERNAL_SERVER_ERROR,
            };
            error_response(status, &format!("{}", e))
        }
    }
}

async fn dispatch_core(state: GatewayState, opcode: Opcode, payload: &[u8]) -> Response {
    match state.core.dispatch(opcode, payload, Encoding::MessagePack).await {
        Ok(resp_bytes) => {
            let value: Value = match rmp_serde::from_slice(&resp_bytes) {
                Ok(v) => v,
                Err(_) => {
                    // Empty or non-JSON response — return as raw bytes.
                    return Json(json!({ "ok": true })).into_response();
                }
            };
            Json(value).into_response()
        }
        Err(e) => {
            let status = match e.scope {
                nxp_core::ErrorScope::Auth => StatusCode::UNAUTHORIZED,
                nxp_core::ErrorScope::Authz => StatusCode::FORBIDDEN,
                nxp_core::ErrorScope::Protocol => StatusCode::BAD_REQUEST,
                _ => StatusCode::INTERNAL_SERVER_ERROR,
            };
            error_response(status, &format!("{}", e))
        }
    }
}

// ==================================================================
// Marketplace routes (protected — require Bearer token)
// ==================================================================

/// `GET /api/marketplace/packages` — list all packages (latest version each).
pub async fn marketplace_list(
    State(state): State<GatewayState>,
    _ctx: axum::Extension<AuthContext>,
) -> Response {
    match state.marketplace.execute("marketplace.list", &json!({})).await {
        Ok(v) => Json(v).into_response(),
        Err(e) => error_response(StatusCode::INTERNAL_SERVER_ERROR, &e.to_string()),
    }
}

/// `GET /api/marketplace/packages/search?q=...` — search packages.
pub async fn marketplace_search(
    State(state): State<GatewayState>,
    _ctx: axum::Extension<AuthContext>,
    Query(q): Query<MarketplaceSearchQuery>,
) -> Response {
    match state.marketplace.execute("marketplace.search", &json!({ "query": q.q })).await {
        Ok(v) => Json(v).into_response(),
        Err(e) => error_response(StatusCode::INTERNAL_SERVER_ERROR, &e.to_string()),
    }
}

/// Query params for marketplace search.
#[derive(Debug, Deserialize)]
pub struct MarketplaceSearchQuery {
    /// Search query.
    #[serde(default)]
    pub q: String,
}

/// `GET /api/marketplace/packages/:id` — get package details.
pub async fn marketplace_get(
    State(state): State<GatewayState>,
    _ctx: axum::Extension<AuthContext>,
    Path(id): Path<String>,
) -> Response {
    match state.marketplace.execute("marketplace.get", &json!({ "id": id })).await {
        Ok(v) => Json(v).into_response(),
        Err(e) => error_response(StatusCode::NOT_FOUND, &e.to_string()),
    }
}

/// `GET /api/marketplace/installed` — list installed packages.
pub async fn marketplace_list_installed(
    State(state): State<GatewayState>,
    _ctx: axum::Extension<AuthContext>,
) -> Response {
    match state.marketplace.execute("marketplace.list_installed", &json!({})).await {
        Ok(v) => Json(v).into_response(),
        Err(e) => error_response(StatusCode::INTERNAL_SERVER_ERROR, &e.to_string()),
    }
}

/// Request body for `POST /api/marketplace/packages/:id/install`.
#[derive(Debug, Deserialize)]
pub struct InstallBody {
    /// Version to install (e.g. "1.0.0").
    pub version: String,
}

/// `POST /api/marketplace/packages/:id/install` — install a package.
pub async fn marketplace_install(
    State(state): State<GatewayState>,
    _ctx: axum::Extension<AuthContext>,
    Path(id): Path<String>,
    Json(body): Json<InstallBody>,
) -> Response {
    match state
        .marketplace
        .execute("marketplace.install", &json!({ "id": id, "version": body.version }))
        .await
    {
        Ok(v) => Json(v).into_response(),
        Err(e) => error_response(StatusCode::BAD_REQUEST, &e.to_string()),
    }
}

/// `POST /api/marketplace/packages/:id/uninstall` — uninstall a package.
pub async fn marketplace_uninstall(
    State(state): State<GatewayState>,
    _ctx: axum::Extension<AuthContext>,
    Path(id): Path<String>,
) -> Response {
    match state.marketplace.execute("marketplace.uninstall", &json!({ "id": id })).await {
        Ok(v) => Json(v).into_response(),
        Err(e) => error_response(StatusCode::BAD_REQUEST, &e.to_string()),
    }
}

/// `POST /api/marketplace/publish` — publish a new package manifest.
pub async fn marketplace_publish(
    State(state): State<GatewayState>,
    _ctx: axum::Extension<AuthContext>,
    Json(manifest): Json<Value>,
) -> Response {
    match state.marketplace.execute("marketplace.publish", &manifest).await {
        Ok(v) => Json(v).into_response(),
        Err(e) => error_response(StatusCode::BAD_REQUEST, &e.to_string()),
    }
}

/// `GET /api/marketplace/updates/check` — check all installed packages for updates.
pub async fn marketplace_check_updates(
    State(state): State<GatewayState>,
    _ctx: axum::Extension<AuthContext>,
) -> Response {
    match state.marketplace.execute("marketplace.check_updates", &json!({})).await {
        Ok(v) => Json(v).into_response(),
        Err(e) => error_response(StatusCode::INTERNAL_SERVER_ERROR, &e.to_string()),
    }
}

/// `POST /api/marketplace/updates/process` — process all auto-update packages.
pub async fn marketplace_process_auto_updates(
    State(state): State<GatewayState>,
    _ctx: axum::Extension<AuthContext>,
) -> Response {
    match state.marketplace.execute("marketplace.process_auto_updates", &json!({})).await {
        Ok(v) => Json(v).into_response(),
        Err(e) => error_response(StatusCode::INTERNAL_SERVER_ERROR, &e.to_string()),
    }
}

/// `POST /api/marketplace/packages/:id/update` — update a package to latest.
pub async fn marketplace_update_package(
    State(state): State<GatewayState>,
    _ctx: axum::Extension<AuthContext>,
    Path(id): Path<String>,
) -> Response {
    match state.marketplace.execute("marketplace.update_package", &json!({ "id": id })).await {
        Ok(v) => Json(v).into_response(),
        Err(e) => error_response(StatusCode::BAD_REQUEST, &e.to_string()),
    }
}

/// Request body for rollback.
#[derive(Debug, Deserialize)]
pub struct RollbackBody {
    /// Version to roll back to.
    pub version: String,
}

/// `POST /api/marketplace/packages/:id/rollback` — roll back to a previous version.
pub async fn marketplace_rollback_package(
    State(state): State<GatewayState>,
    _ctx: axum::Extension<AuthContext>,
    Path(id): Path<String>,
    Json(body): Json<RollbackBody>,
) -> Response {
    match state
        .marketplace
        .execute("marketplace.rollback_package", &json!({ "id": id, "version": body.version }))
        .await
    {
        Ok(v) => Json(v).into_response(),
        Err(e) => error_response(StatusCode::BAD_REQUEST, &e.to_string()),
    }
}

// ==================================================================
// Billing routes (protected — require Bearer token)
// ==================================================================

/// `GET /api/billing/invoices` — list all invoices.
pub async fn billing_list_invoices(
    State(state): State<GatewayState>,
    _ctx: axum::Extension<AuthContext>,
) -> Response {
    match state.billing.execute("billing.list_invoices", &json!({})).await {
        Ok(v) => Json(v).into_response(),
        Err(e) => error_response(StatusCode::INTERNAL_SERVER_ERROR, &e.to_string()),
    }
}

/// `GET /api/billing/invoices/:id` — get a single invoice.
pub async fn billing_get_invoice(
    State(state): State<GatewayState>,
    _ctx: axum::Extension<AuthContext>,
    Path(id): Path<String>,
) -> Response {
    match state.billing.execute("billing.get_invoice", &json!({ "id": id })).await {
        Ok(v) => Json(v).into_response(),
        Err(e) => error_response(StatusCode::NOT_FOUND, &e.to_string()),
    }
}

/// `POST /api/billing/invoices` — create a new invoice.
pub async fn billing_create_invoice(
    State(state): State<GatewayState>,
    _ctx: axum::Extension<AuthContext>,
    Json(body): Json<Value>,
) -> Response {
    match state.billing.execute("billing.create_invoice", &body).await {
        Ok(v) => Json(v).into_response(),
        Err(e) => error_response(StatusCode::BAD_REQUEST, &e.to_string()),
    }
}

/// `GET /api/billing/payments` — list all payments.
pub async fn billing_list_payments(
    State(state): State<GatewayState>,
    _ctx: axum::Extension<AuthContext>,
) -> Response {
    match state.billing.execute("billing.list_payments", &json!({})).await {
        Ok(v) => Json(v).into_response(),
        Err(e) => error_response(StatusCode::INTERNAL_SERVER_ERROR, &e.to_string()),
    }
}

/// `POST /api/billing/payments` — create a payment for an invoice.
pub async fn billing_create_payment(
    State(state): State<GatewayState>,
    _ctx: axum::Extension<AuthContext>,
    Json(body): Json<Value>,
) -> Response {
    match state.billing.execute("billing.create_payment", &body).await {
        Ok(v) => Json(v).into_response(),
        Err(e) => error_response(StatusCode::BAD_REQUEST, &e.to_string()),
    }
}

/// `POST /api/billing/payments/:id/succeed` — mark a payment as succeeded.
pub async fn billing_succeed_payment(
    State(state): State<GatewayState>,
    _ctx: axum::Extension<AuthContext>,
    Path(id): Path<String>,
) -> Response {
    match state.billing.execute("billing.succeed_payment", &json!({ "id": id })).await {
        Ok(v) => Json(v).into_response(),
        Err(e) => error_response(StatusCode::BAD_REQUEST, &e.to_string()),
    }
}

/// `GET /api/billing/subscriptions` — list all subscriptions.
pub async fn billing_list_subscriptions(
    State(state): State<GatewayState>,
    _ctx: axum::Extension<AuthContext>,
) -> Response {
    match state.billing.execute("billing.list_subscriptions", &json!({})).await {
        Ok(v) => Json(v).into_response(),
        Err(e) => error_response(StatusCode::INTERNAL_SERVER_ERROR, &e.to_string()),
    }
}

/// `POST /api/billing/subscriptions` — create a subscription.
pub async fn billing_create_subscription(
    State(state): State<GatewayState>,
    _ctx: axum::Extension<AuthContext>,
    Json(body): Json<Value>,
) -> Response {
    match state.billing.execute("billing.create_subscription", &body).await {
        Ok(v) => Json(v).into_response(),
        Err(e) => error_response(StatusCode::BAD_REQUEST, &e.to_string()),
    }
}

/// `POST /api/billing/subscriptions/:id/cancel` — cancel a subscription.
pub async fn billing_cancel_subscription(
    State(state): State<GatewayState>,
    _ctx: axum::Extension<AuthContext>,
    Path(id): Path<String>,
) -> Response {
    match state.billing.execute("billing.cancel_subscription", &json!({ "id": id })).await {
        Ok(v) => Json(v).into_response(),
        Err(e) => error_response(StatusCode::BAD_REQUEST, &e.to_string()),
    }
}

/// `GET /api/billing/stats` — billing summary stats.
pub async fn billing_stats(
    State(state): State<GatewayState>,
    _ctx: axum::Extension<AuthContext>,
) -> Response {
    match state.billing.execute("billing.stats", &json!({})).await {
        Ok(v) => Json(v).into_response(),
        Err(e) => error_response(StatusCode::INTERNAL_SERVER_ERROR, &e.to_string()),
    }
}

// ==================================================================
// Workflow routes (protected — require Bearer token)
// ==================================================================

/// `GET /api/workflows` — list all workflows.
pub async fn workflow_list(
    State(state): State<GatewayState>,
    _ctx: axum::Extension<AuthContext>,
) -> Response {
    match state.workflow.execute("workflow.list", &json!({})).await {
        Ok(v) => Json(v).into_response(),
        Err(e) => error_response(StatusCode::INTERNAL_SERVER_ERROR, &e.to_string()),
    }
}

/// `POST /api/workflows` — register a new workflow.
pub async fn workflow_register(
    State(state): State<GatewayState>,
    _ctx: axum::Extension<AuthContext>,
    Json(body): Json<Value>,
) -> Response {
    match state.workflow.execute("workflow.register", &body).await {
        Ok(v) => Json(v).into_response(),
        Err(e) => error_response(StatusCode::BAD_REQUEST, &e.to_string()),
    }
}

/// `GET /api/workflows/:id` — get a workflow.
pub async fn workflow_get(
    State(state): State<GatewayState>,
    _ctx: axum::Extension<AuthContext>,
    Path(id): Path<String>,
) -> Response {
    match state.workflow.execute("workflow.get", &json!({ "id": id })).await {
        Ok(v) => Json(v).into_response(),
        Err(e) => error_response(StatusCode::NOT_FOUND, &e.to_string()),
    }
}

/// `POST /api/workflows/:id/trigger` — manually trigger a workflow.
pub async fn workflow_trigger(
    State(state): State<GatewayState>,
    _ctx: axum::Extension<AuthContext>,
    Path(id): Path<String>,
    Json(body): Json<Value>,
) -> Response {
    let mut args = json!({ "id": id });
    if let Some(payload) = body.get("payload") {
        args["payload"] = payload.clone();
    }
    match state.workflow.execute("workflow.trigger", &args).await {
        Ok(v) => Json(v).into_response(),
        Err(e) => error_response(StatusCode::BAD_REQUEST, &e.to_string()),
    }
}

/// `GET /api/workflows/executions` — list all executions.
pub async fn workflow_list_executions(
    State(state): State<GatewayState>,
    _ctx: axum::Extension<AuthContext>,
) -> Response {
    match state.workflow.execute("workflow.list_executions", &json!({})).await {
        Ok(v) => Json(v).into_response(),
        Err(e) => error_response(StatusCode::INTERNAL_SERVER_ERROR, &e.to_string()),
    }
}

/// `GET /api/workflows/stats` — workflow summary stats.
pub async fn workflow_stats(
    State(state): State<GatewayState>,
    _ctx: axum::Extension<AuthContext>,
) -> Response {
    match state.workflow.execute("workflow.stats", &json!({})).await {
        Ok(v) => Json(v).into_response(),
        Err(e) => error_response(StatusCode::INTERNAL_SERVER_ERROR, &e.to_string()),
    }
}

fn error_response(status: StatusCode, message: &str) -> Response {
    (
        status,
        Json(json!({
            "ok": false,
            "error": message,
        })),
    )
        .into_response()
}

#[cfg(test)]
mod tests {
    use super::*;
    use nexora_auth::AuthService;
    use nexora_core::permissions::{Grant, Permission, Role};
    use nexora_core::NexoraCore;

    fn setup() -> GatewayState {
        let core = std::sync::Arc::new(NexoraCore::new());
        core.permissions.register_role(Role {
            id: "admin".into(),
            description: "admin".into(),
            grants: vec![Grant { permission: Permission::Admin, resource: "*".into() }],
        });
        let auth = std::sync::Arc::new(AuthService::new(core.clone()));
        // Pre-create a test user.
        auth.users
            .create("alice", "hunter2", None, vec!["admin".into()])
            .unwrap();
        let marketplace = std::sync::Arc::new(nexora_marketplace::MarketplaceService::new(core.clone()));
        let billing = std::sync::Arc::new(nexora_billing::BillingService::new(core.clone()));
        let workflow = std::sync::Arc::new(nexora_workflow::WorkflowService::new(core.clone()));
        GatewayState {
            auth: std::sync::Arc::new(AuthHandler::new(auth.clone())),
            core: std::sync::Arc::new(CoreHandler::new(core.clone())),
            marketplace: std::sync::Arc::new(marketplace.handler()),
            billing: std::sync::Arc::new(billing.handler()),
            workflow: std::sync::Arc::new(workflow.handler()),
            ready: true,
        }
    }

    #[tokio::test]
    async fn dispatch_auth_login_success() {
        let state = setup();
        let req = serde_json::json!({
            "username": "alice",
            "password": "hunter2",
        });
        let payload = rmp_serde::to_vec_named(&req).unwrap();
        let resp = dispatch_auth(state, Opcode::AuthLogin, &payload).await;
        assert_eq!(resp.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn dispatch_auth_login_wrong_password_returns_401() {
        let state = setup();
        let req = serde_json::json!({
            "username": "alice",
            "password": "WRONG",
        });
        let payload = rmp_serde::to_vec_named(&req).unwrap();
        let resp = dispatch_auth(state, Opcode::AuthLogin, &payload).await;
        assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn dispatch_core_ping_success() {
        let state = setup();
        let resp = dispatch_core(state, Opcode::Ping, &[]).await;
        assert_eq!(resp.status(), StatusCode::OK);
    }
}
