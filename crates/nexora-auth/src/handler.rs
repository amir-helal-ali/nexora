//! Auth NXP Handler — dispatches AUTH_LOGIN, AUTH_LOGOUT, AUTH_REFRESH opcodes.
//!
//! See Nexora Engineering Specification, Part 9 (AUTHENTICATION SYSTEM) and
//! Part 4 (NXP INTEGRATION LAYER). This is the Auth service's NXP handler —
//! it bridges the protocol to the service's business logic.

use crate::token::{SessionToken, TokenError, DEFAULT_REFRESH_TTL, DEFAULT_TOKEN_TTL};
use crate::users::UserError;
use crate::AuthService;
use nxp_core::{NxpError, Opcode, error::protocol_codes, error::auth_codes};
use nxp_payload::{decode, encode, Encoding};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

/// The Auth handler. Owns a reference to the AuthService.
#[derive(Clone)]
pub struct AuthHandler {
    service: Arc<AuthService>,
}

impl std::fmt::Debug for AuthHandler {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AuthHandler")
            .field("service", &self.service)
            .finish()
    }
}

impl AuthHandler {
    /// Construct a new handler.
    pub fn new(service: Arc<AuthService>) -> Self {
        Self { service }
    }

    /// Returns a reference to the underlying AuthService.
    pub fn service(&self) -> &Arc<AuthService> {
        &self.service
    }

    /// Dispatch an auth opcode. Returns the MessagePack-encoded response.
    pub async fn dispatch(
        &self,
        opcode: Opcode,
        payload: &[u8],
        encoding: Encoding,
    ) -> Result<Vec<u8>, NxpError> {
        match opcode {
            Opcode::AuthLogin => self.handle_login(payload, encoding),
            Opcode::AuthLogout => self.handle_logout(payload, encoding),
            Opcode::AuthRefresh => self.handle_refresh(payload, encoding),
            _ => Err(NxpError::protocol(
                protocol_codes::UNKNOWN_OPCODE,
                format!("Auth handler does not implement opcode {:?}", opcode),
            )),
        }
    }

    fn handle_login(&self, payload: &[u8], encoding: Encoding) -> Result<Vec<u8>, NxpError> {
        let req: LoginRequest = decode(encoding, payload)
            .map_err(|e| NxpError::protocol(protocol_codes::DECODE_FAILED, e.to_string()))?;

        // Verify credentials.
        let user = self
            .service
            .users
            .verify(&req.username, &req.password)
            .map_err(map_user_error)?;

        // Issue token.
        let token = self
            .service
            .tokens
            .issue(&user.id, DEFAULT_TOKEN_TTL);

        // Create session.
        let session = self
            .service
            .sessions
            .create(&user.id, req.client.clone());

        // Record login (updates last_login, emits user.logged_in event).
        self.service.users.record_login(&user.id);

        let resp = LoginResponse {
            token: token.to_string(),
            token_expires_at_ns: token.claims.exp,
            session_id: session.id,
            user_id: user.id,
            username: user.username,
        };
        encode(encoding, &resp)
            .map_err(|e| NxpError::protocol(protocol_codes::ENCODE_FAILED, e.to_string()))
    }

    fn handle_logout(&self, payload: &[u8], encoding: Encoding) -> Result<Vec<u8>, NxpError> {
        let req: LogoutRequest = decode(encoding, payload)
            .map_err(|e| NxpError::protocol(protocol_codes::DECODE_FAILED, e.to_string()))?;

        // Parse + verify token (without checking expiry, so users can logout
        // expired tokens too).
        let token = SessionToken::from_str(&req.token)
            .map_err(|e| NxpError::auth(auth_codes::TOKEN_EXPIRED, e.to_string()))?;

        // Revoke the token version + the session.
        self.service.tokens.revoke(&token.claims.sub);
        if let Some(session_id) = req.session_id {
            self.service.sessions.revoke(&session_id);
        }

        // Emit user.logged_out event.
        self.service
            .core
            .events
            .publish("user.logged_out", token.claims.sub.clone());

        let resp = LogoutResponse { ok: true };
        encode(encoding, &resp)
            .map_err(|e| NxpError::protocol(protocol_codes::ENCODE_FAILED, e.to_string()))
    }

    fn handle_refresh(&self, payload: &[u8], encoding: Encoding) -> Result<Vec<u8>, NxpError> {
        let req: RefreshRequest = decode(encoding, payload)
            .map_err(|e| NxpError::protocol(protocol_codes::DECODE_FAILED, e.to_string()))?;

        let old_token = SessionToken::from_str(&req.token)
            .map_err(|e| NxpError::auth(auth_codes::TOKEN_EXPIRED, e.to_string()))?;

        let new_token = self
            .service
            .tokens
            .refresh(&old_token, DEFAULT_REFRESH_TTL)
            .map_err(map_token_error)?;

        let resp = RefreshResponse {
            token: new_token.to_string(),
            token_expires_at_ns: new_token.claims.exp,
        };
        encode(encoding, &resp)
            .map_err(|e| NxpError::protocol(protocol_codes::ENCODE_FAILED, e.to_string()))
    }
}

// ------------------------------------------------------------------
// Request / response types
// ------------------------------------------------------------------

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
struct LogoutRequest {
    token: String,
    #[serde(default)]
    session_id: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
struct LogoutResponse {
    ok: bool,
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

// ------------------------------------------------------------------
// Error mapping
// ------------------------------------------------------------------

fn map_user_error(e: UserError) -> NxpError {
    match e {
        UserError::NotFound(_) | UserError::InvalidCredentials => {
            NxpError::auth(auth_codes::INVALID_CREDENTIALS, "invalid username or password")
        }
        UserError::Inactive(id) => NxpError::auth(auth_codes::INVALID_CREDENTIALS, format!("user {} inactive", id)),
        UserError::UsernameTaken(name) => NxpError::auth(auth_codes::INVALID_CREDENTIALS, format!("user {} exists", name)),
        UserError::Password(_) => NxpError::internal("password hashing failed"),
    }
}

fn map_token_error(e: TokenError) -> NxpError {
    match e {
        TokenError::InvalidSignature | TokenError::Malformed(_) => {
            NxpError::auth(auth_codes::INVALID_CREDENTIALS, "invalid token")
        }
        TokenError::Expired => NxpError::auth(auth_codes::TOKEN_EXPIRED, "token expired"),
        TokenError::Revoked | TokenError::VersionMismatch { .. } => {
            NxpError::auth(auth_codes::TOKEN_EXPIRED, "token revoked")
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::AuthService;
    use nexora_core::NexoraCore;
    use nxp_core::Opcode;
    use nxp_payload::Encoding;

    fn setup() -> (Arc<AuthService>, Arc<AuthHandler>) {
        let core = Arc::new(NexoraCore::new());
        let svc = Arc::new(AuthService::new(core));
        // Pre-create a test user.
        svc.users.create("alice", "hunter2", None, vec!["viewer".into()]).unwrap();
        let handler = Arc::new(AuthHandler::new(svc.clone()));
        (svc, handler)
    }

    #[tokio::test]
    async fn login_returns_token_and_session() {
        let (svc, handler) = setup();
        let req = LoginRequest {
            username: "alice".into(),
            password: "hunter2".into(),
            client: Some("test-client".into()),
        };
        let payload = rmp_serde::to_vec_named(&req).unwrap();
        let resp = handler
            .dispatch(Opcode::AuthLogin, &payload, Encoding::MessagePack)
            .await
            .unwrap();
        let parsed: LoginResponse = rmp_serde::from_slice(&resp).unwrap();
        assert!(!parsed.token.is_empty());
        assert_eq!(parsed.username, "alice");
        assert_eq!(svc.sessions.session_count(), 1);
    }

    #[tokio::test]
    async fn login_with_wrong_password_fails() {
        let (_svc, handler) = setup();
        let req = LoginRequest {
            username: "alice".into(),
            password: "WRONG".into(),
            client: None,
        };
        let payload = rmp_serde::to_vec_named(&req).unwrap();
        let err = handler
            .dispatch(Opcode::AuthLogin, &payload, Encoding::MessagePack)
            .await
            .unwrap_err();
        assert_eq!(err.scope, nxp_core::ErrorScope::Auth);
        assert_eq!(err.code, auth_codes::INVALID_CREDENTIALS);
    }

    #[tokio::test]
    async fn login_then_logout_invalidates_token() {
        let (svc, handler) = setup();

        // Login
        let req = LoginRequest {
            username: "alice".into(),
            password: "hunter2".into(),
            client: None,
        };
        let payload = rmp_serde::to_vec_named(&req).unwrap();
        let resp = handler
            .dispatch(Opcode::AuthLogin, &payload, Encoding::MessagePack)
            .await
            .unwrap();
        let login: LoginResponse = rmp_serde::from_slice(&resp).unwrap();

        // Verify token works.
        let token = SessionToken::from_str(&login.token).unwrap();
        let claims = svc.tokens.verify(&token).unwrap();
        assert_eq!(claims.sub, login.user_id);

        // Logout
        let req = LogoutRequest {
            token: login.token.clone(),
            session_id: Some(login.session_id.clone()),
        };
        let payload = rmp_serde::to_vec_named(&req).unwrap();
        let _resp = handler
            .dispatch(Opcode::AuthLogout, &payload, Encoding::MessagePack)
            .await
            .unwrap();

        // Token should now be invalid.
        let err = svc.tokens.verify(&token).unwrap_err();
        assert!(matches!(err, TokenError::VersionMismatch { .. }));
    }

    #[tokio::test]
    async fn refresh_issues_new_token_and_invalidates_old() {
        let (svc, handler) = setup();

        // Login.
        let req = LoginRequest {
            username: "alice".into(),
            password: "hunter2".into(),
            client: None,
        };
        let payload = rmp_serde::to_vec_named(&req).unwrap();
        let resp = handler
            .dispatch(Opcode::AuthLogin, &payload, Encoding::MessagePack)
            .await
            .unwrap();
        let login: LoginResponse = rmp_serde::from_slice(&resp).unwrap();
        let old_token = SessionToken::from_str(&login.token).unwrap();

        // Refresh.
        let req = RefreshRequest { token: login.token.clone() };
        let payload = rmp_serde::to_vec_named(&req).unwrap();
        let resp = handler
            .dispatch(Opcode::AuthRefresh, &payload, Encoding::MessagePack)
            .await
            .unwrap();
        let parsed: RefreshResponse = rmp_serde::from_slice(&resp).unwrap();
        let new_token = SessionToken::from_str(&parsed.token).unwrap();

        // New token is valid.
        let claims = svc.tokens.verify(&new_token).unwrap();
        assert_eq!(claims.sub, login.user_id);
        assert!(claims.ver > old_token.claims.ver);

        // Old token is invalid.
        let err = svc.tokens.verify(&old_token).unwrap_err();
        assert!(matches!(err, TokenError::VersionMismatch { .. }));
    }

    #[tokio::test]
    async fn login_emits_events() {
        let (_svc, handler) = setup();
        let req = LoginRequest {
            username: "alice".into(),
            password: "hunter2".into(),
            client: None,
        };
        let payload = rmp_serde::to_vec_named(&req).unwrap();
        let _ = handler
            .dispatch(Opcode::AuthLogin, &payload, Encoding::MessagePack)
            .await
            .unwrap();
        // The user was pre-created with no events emitted (since the UserStore
        // in setup() wasn't wired to the bus). After login, we expect at least
        // user.logged_in. But wait — setup() uses AuthService::new which doesn't
        // wire the bus either. Let's verify by attaching to the core's bus.
        // Actually AuthService uses self.core.events. So login should emit
        // user.logged_in.
        let events = handler.service.core.events.replay_filtered(0, "user.");
        assert!(events.iter().any(|e| e.name == "user.logged_in"));
    }
}
