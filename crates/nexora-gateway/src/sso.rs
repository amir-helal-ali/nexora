//! مسارات SSO (الدخول الموحد) — OIDC + SAML 2.0.
//!
//! هذه المسارات تتيح للمستخدمين تسجيل الدخول عبر مزودي هوية خارجيين
//! (Google، Microsoft Entra، Okta، ADFS، إلخ).
//!
//! # مسارات OIDC
//!
//! - `GET /api/auth/sso/oidc/:provider/login` — يبدأ تدفق OIDC
//! - `GET /api/auth/sso/oidc/:provider/callback` — يستقبل الرمز من IdP
//!
//! # مسارات SAML
//!
//! - `GET /api/auth/sso/saml/:provider/login` — يبدأ تدفق SAML
//! - `POST /api/auth/sso/saml/:provider/acs` — يستقبل استجابة SAML
//!
//! # الأمان
//!
//! - كل تدفق يولّد `state` عشوائي يُتحقَّق منه عند الاستدعاء (CSRF)
//! - OIDC يستخدم PKCE (S256) لمنع اعتراض الرمز
//! - SAML يتحقق من نافذة الصلاحية والجمهور

use crate::middleware::AuthContext;
use crate::routes::{error_response, GatewayState};
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Redirect};
use axum::response::Response as AxumResponse;
use axum::Json;
use nexora_auth::sso::{
    OidcClient, SsoConfig, SsoProviderConfig, SsoProviderKind, SsoSessionManager,
};
use serde::Deserialize;
use serde_json::json;
use std::collections::HashMap;
use std::sync::Arc;

/// حالة SSO المشتركة عبر المسارات.
#[derive(Clone)]
pub struct SsoState {
    /// مدير جلسات SSO.
    pub sessions: Arc<SsoSessionManager>,
    /// إعدادات SSO (محمية بـ RwLock للتحديث الديناميكي).
    pub config: Arc<parking_lot::RwLock<SsoConfig>>,
    /// عملاء OIDC مُهيّأون (يُبنون عند الطلب الأول).
    pub oidc_clients: Arc<parking_lot::RwLock<HashMap<String, Arc<OidcClient>>>>,
}

impl SsoState {
    /// إنشاء حالة SSO جديدة.
    pub fn new(config: SsoConfig) -> Self {
        Self {
            sessions: Arc::new(SsoSessionManager::default()),
            config: Arc::new(parking_lot::RwLock::new(config)),
            oidc_clients: Arc::new(parking_lot::RwLock::new(HashMap::new())),
        }
    }

    /// إنشاء حالة فارغة (لا مزودين).
    pub fn empty() -> Self {
        Self::new(SsoConfig::default())
    }

    /// إضافة مزود SSO.
    pub fn add_provider(&self, provider: SsoProviderConfig) {
        self.config.write().upsert(provider);
    }

    /// البحث عن مزود بالمعرّف.
    pub fn find_provider(&self, id: &str) -> Option<SsoProviderConfig> {
        self.config.read().find(id).cloned()
    }

    /// عدد المزودين.
    pub fn provider_count(&self) -> usize {
        self.config.read().providers.len()
    }
}

impl std::fmt::Debug for SsoState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SsoState")
            .field("providers", &self.config.read().providers.len())
            .field("pending_flows", &self.sessions.flow_count())
            .field("active_sessions", &self.sessions.session_count())
            .finish()
    }
}

// ==================================================================
// OIDC Routes
// ==================================================================

/// `GET /api/auth/sso/oidc/:provider/login` — يبدأ تدفق OIDC.
///
/// يُعيد توجيه المتصفح إلى صفحة تسجيل الدخول الخاصة بـ IdP.
pub async fn sso_oidc_login(
    State(state): State<GatewayState>,
    Path(provider_id): Path<String>,
) -> AxumResponse {
    let sso_state = match &state.sso {
        Some(s) => s.clone(),
        None => return error_response(StatusCode::SERVICE_UNAVAILABLE, "SSO غير مفعّل"),
    };

    // ابحث عن المزود.
    let provider = match sso_state.find_provider(&provider_id) {
        Some(p) => p.clone(),
        None => {
            return error_response(
                StatusCode::NOT_FOUND,
                &format!("مزود SSO غير موجود: {provider_id}"),
            )
        }
    };

    if provider.kind != SsoProviderKind::Oidc {
        return error_response(
            StatusCode::BAD_REQUEST,
            &format!("المزود {provider_id} ليس OIDC"),
        );
    }

    // ابني عميل OIDC (أو استخدم النسخة المخبّأة).
    // هام: يجب إفلات read guard قبل أي .await لضمان Send.
    let cached_client = sso_state.oidc_clients.read().get(&provider_id).cloned();
    let client = if let Some(c) = cached_client {
        c
    } else {
        let client = match OidcClient::new(provider.clone()).await {
            Ok(c) => Arc::new(c),
            Err(e) => {
                return error_response(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    &format!("فشل تهيئة OIDC: {e}"),
                )
            }
        };
        sso_state
            .oidc_clients
            .write()
            .insert(provider_id.clone(), client.clone());
        client
    };

    // ابني URL التفويض.
    let redirect_uri = format!("/api/auth/sso/oidc/{provider_id}/callback");
    let (auth_url, state_token, nonce) = match client.build_authorization_url(&redirect_uri) {
        Ok(parts) => parts,
        Err(e) => {
            return error_response(
                StatusCode::INTERNAL_SERVER_ERROR,
                &format!("فشل بناء URL التفويض: {e}"),
            )
        }
    };

    // سجّل التدفق المعلّق.
    sso_state
        .sessions
        .start_flow(&provider_id, &redirect_uri, Some(nonce), None);

    // أعد التوجيه إلى IdP.
    Redirect::to(&auth_url).into_response()
}

/// معاملات الاستعلام لاستدعاء OIDC.
#[derive(Deserialize)]
pub struct OidcCallbackQuery {
    /// الرمز من IdP.
    pub code: Option<String>,
    /// الحالة (للتحقق من CSRF).
    pub state: String,
    /// خطأ من IdP (إن وُجد).
    pub error: Option<String>,
    pub error_description: Option<String>,
}

/// `GET /api/auth/sso/oidc/:provider/callback` — يستقبل الرمز من IdP.
pub async fn sso_oidc_callback(
    State(state): State<GatewayState>,
    Path(provider_id): Path<String>,
    Query(query): Query<OidcCallbackQuery>,
) -> AxumResponse {
    let sso_state = match &state.sso {
        Some(s) => s.clone(),
        None => return error_response(StatusCode::SERVICE_UNAVAILABLE, "SSO غير مفعّل"),
    };

    // تحقق من وجود خطأ من IdP.
    if let Some(err) = &query.error {
        let desc = query.error_description.unwrap_or_default();
        return error_response(
            StatusCode::BAD_REQUEST,
            &format!("خطأ IdP: {err} — {desc}"),
        );
    }

    // تحقق من وجود الرمز.
    let code = match &query.code {
        Some(c) => c.clone(),
        None => return error_response(StatusCode::BAD_REQUEST, "رمز التفويض مفقود"),
    };

    // استهلِك التدفق (يتحقق من state).
    let flow = match sso_state.sessions.consume_flow(&query.state) {
        Ok(f) => f,
        Err(e) => {
            return error_response(
                StatusCode::BAD_REQUEST,
                &format!("فشل التحقق من state: {e}"),
            )
        }
    };

    // ابحث عن المزود.
    let provider = match sso_state.find_provider(&provider_id) {
        Some(p) => p.clone(),
        None => {
            return error_response(
                StatusCode::NOT_FOUND,
                &format!("مزود SSO غير موجود: {provider_id}"),
            )
        }
    };

    // ابني عميل OIDC.
    let client = {
        let cache = sso_state.oidc_clients.read();
        cache.get(&provider_id).cloned()
    };
    let client = match client {
        Some(c) => c,
        None => {
            // أعِد بناء العميل.
            match OidcClient::new(provider.clone()).await {
                Ok(c) => Arc::new(c),
                Err(e) => {
                    return error_response(
                        StatusCode::INTERNAL_SERVER_ERROR,
                        &format!("فشل تهيئة OIDC: {e}"),
                    )
                }
            }
        }
    };

    // بادل الرمز بالرمز المميز.
    let claims = match client.exchange_code(&code, &flow.redirect_uri).await {
        Ok(c) => c,
        Err(e) => {
            return error_response(
                StatusCode::UNAUTHORIZED,
                &format!("فشل تبادل الرمز: {e}"),
            )
        }
    };

    // تحقق من الـ nonce (إن وُجد).
    if let Some(expected_nonce) = &flow.nonce {
        if let Some(actual_nonce) = &claims.nonce {
            if actual_nonce != expected_nonce {
                return error_response(StatusCode::UNAUTHORIZED, "عدم تطابق nonce");
            }
        }
    }

    // تحقق من انتهاء الصلاحية.
    if nexora_auth::sso::oidc::is_id_token_expired(&claims) {
        return error_response(StatusCode::UNAUTHORIZED, "انتهت صلاحية الرمز");
    }

    // أنشئ جلسة SSO.
    let user_id = claims.email.clone().unwrap_or_else(|| claims.sub.clone());
    let session = sso_state
        .sessions
        .create_session(&user_id, &provider_id, &claims.sub);

    // في الإنتاج، هنا سنصدر رمز Nexora (Ed25519) ونضعه في كوكي.
    // للتنفيذ المرجعي، نُرجع معرّف الجلسة مباشرة.
    Json(json!({
        "ok": true,
        "session_id": session.id,
        "user_id": user_id,
        "provider": provider_id,
        "idp_subject": claims.sub,
        "email": claims.email,
        "name": claims.name,
        "redirect_to": provider.redirect_after_login,
    }))
    .into_response()
}

// ==================================================================
// SAML Routes
// ==================================================================

/// `GET /api/auth/sso/saml/:provider/login` — يبدأ تدفق SAML.
pub async fn sso_saml_login(
    State(state): State<GatewayState>,
    Path(provider_id): Path<String>,
) -> AxumResponse {
    let sso_state = match &state.sso {
        Some(s) => s.clone(),
        None => return error_response(StatusCode::SERVICE_UNAVAILABLE, "SSO غير مفعّل"),
    };

    let provider = match sso_state.find_provider(&provider_id) {
        Some(p) => p.clone(),
        None => {
            return error_response(
                StatusCode::NOT_FOUND,
                &format!("مزود SSO غير موجود: {provider_id}"),
            )
        }
    };

    if provider.kind != SsoProviderKind::Saml {
        return error_response(
            StatusCode::BAD_REQUEST,
            &format!("المزود {provider_id} ليس SAML"),
        );
    }

    let client = match nexora_auth::sso::SamlClient::new(provider.clone()) {
        Ok(c) => c,
        Err(e) => {
            return error_response(
                StatusCode::INTERNAL_SERVER_ERROR,
                &format!("فشل تهيئة SAML: {e}"),
            )
        }
    };

    let authn_url = match client.build_authn_request_url() {
        Ok(u) => u,
        Err(e) => {
            return error_response(
                StatusCode::INTERNAL_SERVER_ERROR,
                &format!("فشل بناء طلب المصادقة: {e}"),
            )
        }
    };

    // سجّل تدفقاً معلّقاً (SAML لا يستخدم nonce).
    let redirect_uri = format!("/api/auth/sso/saml/{provider_id}/acs");
    sso_state
        .sessions
        .start_flow(&provider_id, &redirect_uri, None, None);

    Redirect::to(&authn_url).into_response()
}

/// معاملات استدعاء SAML ACS (POST binding).
#[derive(Deserialize)]
pub struct SamlAcsBody {
    /// استجابة SAML المشفّرة base64.
    pub samlresponse: String,
    /// RelayState (اختياري).
    #[serde(default)]
    pub relaystate: Option<String>,
}

/// `POST /api/auth/sso/saml/:provider/acs` — يستقبل استجابة SAML.
pub async fn sso_saml_acs(
    State(state): State<GatewayState>,
    Path(provider_id): Path<String>,
    Json(body): Json<SamlAcsBody>,
) -> AxumResponse {
    let sso_state = match &state.sso {
        Some(s) => s.clone(),
        None => return error_response(StatusCode::SERVICE_UNAVAILABLE, "SSO غير مفعّل"),
    };

    let provider = match sso_state.find_provider(&provider_id) {
        Some(p) => p.clone(),
        None => {
            return error_response(
                StatusCode::NOT_FOUND,
                &format!("مزود SSO غير موجود: {provider_id}"),
            )
        }
    };

    let client = match nexora_auth::sso::SamlClient::new(provider.clone()) {
        Ok(c) => c,
        Err(e) => {
            return error_response(
                StatusCode::INTERNAL_SERVER_ERROR,
                &format!("فشل تهيئة SAML: {e}"),
            )
        }
    };

    // حلّل استجابة SAML.
    let assertion = match client.parse_response(&body.samlresponse) {
        Ok(a) => a,
        Err(e) => {
            return error_response(
                StatusCode::UNAUTHORIZED,
                &format!("فشل تحليل استجابة SAML: {e}"),
            )
        }
    };

    // أنشئ جلسة SSO.
    let session = sso_state
        .sessions
        .create_session(&assertion.subject, &provider_id, &assertion.subject);

    Json(json!({
        "ok": true,
        "session_id": session.id,
        "user_id": assertion.subject,
        "provider": provider_id,
        "issuer": assertion.issuer,
        "session_index": assertion.session_index,
        "attributes": assertion.attributes,
        "redirect_to": provider.redirect_after_login,
    }))
    .into_response()
}

// ==================================================================
// SSO Management Routes
// ==================================================================

/// `GET /api/auth/sso/providers` — قائمة مزودي SSO المُهيّأين.
pub async fn sso_list_providers(
    State(state): State<GatewayState>,
    _ctx: axum::Extension<AuthContext>,
) -> AxumResponse {
    let sso_state = match &state.sso {
        Some(s) => s.clone(),
        None => return error_response(StatusCode::SERVICE_UNAVAILABLE, "SSO غير مفعّل"),
    };

    let providers: Vec<_> = sso_state
        .config
        .read()
        .providers
        .iter()
        .map(|p| {
            json!({
                "id": p.id,
                "display_name": p.display_name,
                "kind": match p.kind {
                    SsoProviderKind::Oidc => "oidc",
                    SsoProviderKind::Saml => "saml",
                },
                "redirect_after_login": p.redirect_after_login,
            })
        })
        .collect();

    Json(json!({
        "providers": providers,
        "count": providers.len(),
    }))
    .into_response()
}

/// `GET /api/auth/sso/stats` — إحصائيات جلسات SSO.
pub async fn sso_stats(
    State(state): State<GatewayState>,
    _ctx: axum::Extension<AuthContext>,
) -> AxumResponse {
    let sso_state = match &state.sso {
        Some(s) => s.clone(),
        None => return error_response(StatusCode::SERVICE_UNAVAILABLE, "SSO غير مفعّل"),
    };

    // نظّف الجلسات منتهية الصلاحية.
    let (flows_purged, sessions_purged) = sso_state.sessions.purge_expired();

    Json(json!({
        "providers": sso_state.provider_count(),
        "pending_flows": sso_state.sessions.flow_count(),
        "active_sessions": sso_state.sessions.session_count(),
        "flows_purged": flows_purged,
        "sessions_purged": sessions_purged,
    }))
    .into_response()
}

/// `POST /api/auth/sso/logout` — تسجيل خروج من جلسة SSO.
#[derive(Deserialize)]
pub struct SsoLogoutBody {
    pub session_id: String,
}

pub async fn sso_logout(
    State(state): State<GatewayState>,
    Json(body): Json<SsoLogoutBody>,
) -> AxumResponse {
    let sso_state = match &state.sso {
        Some(s) => s.clone(),
        None => return error_response(StatusCode::SERVICE_UNAVAILABLE, "SSO غير مفعّل"),
    };

    if sso_state.sessions.revoke_session(&body.session_id) {
        Json(json!({"ok": true})).into_response()
    } else {
        error_response(StatusCode::NOT_FOUND, "جلسة SSO غير موجودة")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sso_state_empty_has_no_providers() {
        let state = SsoState::empty();
        assert_eq!(state.provider_count(), 0);
    }

    #[test]
    fn sso_state_add_provider() {
        let state = SsoState::empty();
        let provider = SsoProviderConfig {
            id: "test".into(),
            display_name: "Test".into(),
            kind: SsoProviderKind::Oidc,
            client_id: "x".into(),
            client_secret: "y".into(),
            oidc_discovery_url: Some("https://example.com".into()),
            oidc_scopes: vec!["openid".into()],
            saml_metadata_url: None,
            saml_sso_url: None,
            saml_idp_certificate: None,
            saml_sp_entity_id: None,
            saml_sp_acs_url: None,
            redirect_after_login: "/dashboard".into(),
            role_mapping: HashMap::new(),
            default_role: "viewer".into(),
        };
        state.add_provider(provider);
        assert_eq!(state.provider_count(), 1);
    }
}
