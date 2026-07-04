//! Token validation middleware.
//!
//! Extracts the `Authorization: Bearer <token>` header, verifies it against
//! the AuthHandler's TokenVerifier, and injects the verified claims into the
//! request extension. Unauthenticated requests receive a 401 response.

use axum::{
    extract::Request,
    http::{header, StatusCode},
    middleware::Next,
    response::{IntoResponse, Response},
};
use nexora_auth::{token::SessionToken, TokenError};
use std::sync::Arc;

/// The verified token claims, injected into the request extension by the
/// middleware. Handlers can extract this to know who is calling.
#[derive(Clone, Debug)]
pub struct AuthContext {
    /// Subject (user ID).
    pub user_id: String,
    /// Token version.
    pub version: u64,
    /// Issued-at (unix nanos).
    pub issued_at: i64,
    /// Expiry (unix nanos).
    pub expires_at: i64,
}

/// Middleware state: holds the AuthHandler so we can verify tokens.
#[derive(Clone)]
pub struct AuthMiddleware {
    auth: Arc<nexora_auth::AuthService>,
}

impl AuthMiddleware {
    /// Construct a new middleware wrapping the given AuthService.
    pub fn new(auth: Arc<nexora_auth::AuthService>) -> Self {
        Self { auth }
    }
}

/// The actual middleware function. Apply via
/// `axum::middleware::from_fn_with_state(state, auth_middleware)`.
pub async fn require_token(
    state: axum::extract::State<AuthMiddleware>,
    mut req: Request,
    next: Next,
) -> Response {
    // Extract Authorization header.
    let auth_header = req
        .headers()
        .get(header::AUTHORIZATION)
        .and_then(|h| h.to_str().ok())
        .unwrap_or("");

    // Try Authorization: Bearer <token> first.
    let token_str = if let Some(t) = auth_header.strip_prefix("Bearer ") {
        t.trim().to_string()
    } else {
        // Fallback: ?token=<token> query param (for SSE/EventSource which
        // cannot set custom headers). This is safe because the token is
        // still Ed25519-signed and verified normally.
        let query = req.uri().query().unwrap_or("");
        let mut token: Option<String> = None;
        for pair in query.split('&') {
            let mut parts = pair.splitn(2, '=');
            if parts.next() == Some("token") {
                if let Some(v) = parts.next() {
                    token = Some(v.to_string());
                }
            }
        }
        match token {
            Some(t) => t,
            None => {
                return (StatusCode::UNAUTHORIZED, "missing Bearer token").into_response();
            }
        }
    };

    // Parse + verify the token.
    let token = match SessionToken::from_str(&token_str) {
        Ok(t) => t,
        Err(e) => {
            return (StatusCode::UNAUTHORIZED, format!("invalid token: {}", e)).into_response();
        }
    };

    let claims = match state.auth.tokens.verify(&token) {
        Ok(c) => c,
        Err(e) => {
            let msg = match e {
                TokenError::Expired => "token expired",
                TokenError::Revoked => "token revoked",
                TokenError::VersionMismatch { .. } => "token version mismatch (rotated)",
                TokenError::InvalidSignature => "invalid token signature",
                TokenError::Malformed(_) => "malformed token",
            };
            return (StatusCode::UNAUTHORIZED, msg).into_response();
        }
    };

    // Inject claims into request extension and continue.
    let ctx = AuthContext {
        user_id: claims.sub.clone(),
        version: claims.ver,
        issued_at: claims.iat,
        expires_at: claims.exp,
    };
    req.extensions_mut().insert(ctx);
    next.run(req).await
}
