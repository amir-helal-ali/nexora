//! مسارات MFA + Audit + Rules في البوابة.
//!
//! هذه المسارات تتيح إدارة:
//! - المصادقة متعددة العوامل (MFA/TOTP)
//! - سجل التدقيق (الاستعلام والإحصائيات)
//! - محرك القواعد (CRUD + التنفيذ)

use crate::middleware::AuthContext;
use crate::routes::{error_response, GatewayState};
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;
use nexora_audit::{AuditCategory, AuditEntry, AuditFilter, AuditSort};
use nexora_auth::mfa::{MfaManager, MfaVerifyResult};
use nexora_rules::{Action, Condition, Rule, RuleEngine, RuleStatus};
use serde::Deserialize;
use serde_json::json;
use std::sync::Arc;

type AxumResponse = Response;

// ==================================================================
// MFA Routes
// ==================================================================

/// `POST /api/auth/mfa/enroll/begin` — بدء تفعيل MFA.
pub async fn mfa_enroll_begin(
    State(state): State<GatewayState>,
    ctx: axum::Extension<AuthContext>,
) -> AxumResponse {
    let enrollment = state.mfa.begin_enrollment(&ctx.user_id);

    // سجّل في سجل التدقيق.
    state.audit.log(
        AuditEntry::new(&ctx.user_id, "mfa.enroll.begin", &ctx.user_id)
            .with_category(AuditCategory::Auth),
    );

    Json(json!({
        "secret": enrollment.secret.to_base32(),
        "otpauth_url": enrollment.otpauth_url,
        "backup_codes": enrollment.backup_codes,
    }))
    .into_response()
}

/// `POST /api/auth/mfa/enroll/complete` — إكمال تفعيل MFA.
#[derive(Deserialize)]
pub struct MfaEnrollCompleteBody {
    pub code: String,
    pub secret: String,
    pub backup_codes: Vec<String>,
}

pub async fn mfa_enroll_complete(
    State(state): State<GatewayState>,
    ctx: axum::Extension<AuthContext>,
    Json(body): Json<MfaEnrollCompleteBody>,
) -> AxumResponse {
    let secret = match nexora_auth::mfa::totp::TotpSecret::from_base32(&body.secret) {
        Ok(s) => s,
        Err(e) => return error_response(StatusCode::BAD_REQUEST, &format!("سر غير صالح: {e}")),
    };

    let enrollment = nexora_auth::mfa::MfaEnrollment {
        secret,
        otpauth_url: String::new(),
        backup_codes: body.backup_codes,
    };

    match state.mfa.complete_enrollment(&ctx.user_id, &enrollment, &body.code) {
        Ok(()) => {
            state.audit.log(
                AuditEntry::new(&ctx.user_id, "mfa.enroll.complete", &ctx.user_id)
                    .with_category(AuditCategory::Auth),
            );
            Json(json!({"ok": true, "enabled": true})).into_response()
        }
        Err(e) => error_response(StatusCode::BAD_REQUEST, &e),
    }
}

/// `POST /api/auth/mfa/verify` — التحقق من رمز MFA.
#[derive(Deserialize)]
pub struct MfaVerifyBody {
    pub code: String,
}

pub async fn mfa_verify(
    State(state): State<GatewayState>,
    ctx: axum::Extension<AuthContext>,
    Json(body): Json<MfaVerifyBody>,
) -> AxumResponse {
    let result = state.mfa.verify(&ctx.user_id, &body.code);
    let (ok, message): (bool, String) = match result {
        MfaVerifyResult::Valid => (true, "صالح".into()),
        MfaVerifyResult::Invalid => (false, "رمز غير صالح".into()),
        MfaVerifyResult::NotEnrolled => (false, "MFA غير مُفعّل".into()),
        MfaVerifyResult::AlreadyUsed => (false, "كود الاسترداد مستخدم بالفعل".into()),
        MfaVerifyResult::Error(e) => (false, e),
    };

    state.audit.log(
        AuditEntry::new(&ctx.user_id, "mfa.verify", &ctx.user_id)
            .with_category(AuditCategory::Auth)
            .with_success(ok),
    );

    Json(json!({"valid": ok, "message": message})).into_response()
}

/// `POST /api/auth/mfa/disable` — تعطيل MFA.
pub async fn mfa_disable(
    State(state): State<GatewayState>,
    ctx: axum::Extension<AuthContext>,
) -> AxumResponse {
    let was_enabled = state.mfa.disable(&ctx.user_id);
    state.audit.log(
        AuditEntry::new(&ctx.user_id, "mfa.disable", &ctx.user_id)
            .with_category(AuditCategory::Auth)
            .with_success(was_enabled),
    );
    Json(json!({"ok": true, "was_enabled": was_enabled})).into_response()
}

/// `GET /api/auth/mfa/status` — حالة MFA للمستخدم.
pub async fn mfa_status(
    State(state): State<GatewayState>,
    ctx: axum::Extension<AuthContext>,
) -> AxumResponse {
    let enrolled = state.mfa.is_enrolled(&ctx.user_id);
    Json(json!({
        "enrolled": enrolled,
        "enabled": enrolled,
    }))
    .into_response()
}

/// `POST /api/auth/mfa/backup-codes/regenerate` — توليد أكواد استرداد جديدة.
pub async fn mfa_regenerate_backup_codes(
    State(state): State<GatewayState>,
    ctx: axum::Extension<AuthContext>,
) -> AxumResponse {
    match state.mfa.regenerate_backup_codes(&ctx.user_id) {
        Ok(codes) => {
            state.audit.log(
                AuditEntry::new(&ctx.user_id, "mfa.backup_codes.regenerate", &ctx.user_id)
                    .with_category(AuditCategory::Auth),
            );
            Json(json!({"backup_codes": codes})).into_response()
        }
        Err(e) => error_response(StatusCode::BAD_REQUEST, &e),
    }
}

/// `GET /api/auth/mfa/stats` — إحصائيات MFA (للمشرفين).
pub async fn mfa_stats(State(state): State<GatewayState>) -> AxumResponse {
    Json(json!({
        "enrolled_users": state.mfa.enrolled_count(),
    }))
    .into_response()
}

// ==================================================================
// Audit Routes
// ==================================================================

/// `GET /api/audit/entries` — استعلام عن مدخلات التدقيق.
pub async fn audit_list(
    State(state): State<GatewayState>,
    _ctx: axum::Extension<AuthContext>,
    Query(params): Query<AuditQueryParams>,
) -> AxumResponse {
    let mut filter = AuditFilter::new();
    if let Some(actor) = params.actor {
        filter = filter.with_actor(actor);
    }
    if let Some(action) = params.action {
        filter = filter.with_action(action);
    }
    if let Some(category) = params.category {
        if let Ok(cat) = category.parse::<AuditCategory>() {
            filter = filter.with_category(cat);
        }
    }
    if let Some(limit) = params.limit {
        filter = filter.with_limit(limit);
    }
    if let Some(offset) = params.offset {
        filter = filter.with_offset(offset);
    }
    if let Some(success) = params.success {
        if success {
            filter = filter.success_only();
        } else {
            filter = filter.failures_only();
        }
    }
    if params.oldest_first.unwrap_or(false) {
        filter = filter.with_sort(AuditSort::OldestFirst);
    }

    let result = state.audit.query(&filter);
    Json(json!({
        "entries": result.entries,
        "total": result.total,
        "limit": result.limit,
        "offset": result.offset,
    }))
    .into_response()
}

#[derive(Deserialize)]
pub struct AuditQueryParams {
    pub actor: Option<String>,
    pub action: Option<String>,
    pub category: Option<String>,
    pub success: Option<bool>,
    pub limit: Option<usize>,
    pub offset: Option<usize>,
    pub oldest_first: Option<bool>,
}

/// `GET /api/audit/stats` — إحصائيات سجل التدقيق.
pub async fn audit_stats(
    State(state): State<GatewayState>,
    _ctx: axum::Extension<AuthContext>,
) -> AxumResponse {
    let stats = state.audit.stats();
    Json(json!({
        "total": stats.total,
        "success": stats.success,
        "failure": stats.failure,
        "by_category": stats.categories.iter().map(|(c, n)| {
            json!({"category": c.as_str(), "count": n})
        }).collect::<Vec<_>>(),
    }))
    .into_response()
}

/// `GET /api/audit/:id` — مدخل تدقيق محدد.
pub async fn audit_get(
    State(state): State<GatewayState>,
    _ctx: axum::Extension<AuthContext>,
    Path(id): Path<String>,
) -> AxumResponse {
    match state.audit.get(&id) {
        Some(entry) => Json(json!({"entry": entry})).into_response(),
        None => error_response(StatusCode::NOT_FOUND, "مدخل غير موجود"),
    }
}

// ==================================================================
// Rules Routes
// ==================================================================

/// `GET /api/rules` — قائمة القواعد.
pub async fn rules_list(
    State(state): State<GatewayState>,
    _ctx: axum::Extension<AuthContext>,
) -> AxumResponse {
    let engine = match &state.rules {
        Some(e) => e,
        None => return error_response(StatusCode::SERVICE_UNAVAILABLE, "محرك القواعد غير مفعّل"),
    };
    let rules = engine.list();
    Json(json!({
        "rules": rules,
        "count": rules.len(),
    }))
    .into_response()
}

/// `POST /api/rules` — إنشاء قاعدة جديدة.
pub async fn rules_create(
    State(state): State<GatewayState>,
    ctx: axum::Extension<AuthContext>,
    Json(body): Json<CreateRuleBody>,
) -> AxumResponse {
    let engine = match &state.rules {
        Some(e) => e,
        None => return error_response(StatusCode::SERVICE_UNAVAILABLE, "محرك القواعد غير مفعّل"),
    };

    let rule = Rule::new(body.name, body.condition, body.actions);
    let rule_id = rule.id.clone();

    state.audit.log(
        AuditEntry::new(&ctx.user_id, "rule.create", &rule_id)
            .with_category(AuditCategory::Rule),
    );

    match engine.register(rule) {
        Ok(id) => Json(json!({"id": id, "ok": true})).into_response(),
        Err(e) => error_response(StatusCode::CONFLICT, &e.to_string()),
    }
}

#[derive(Deserialize)]
pub struct CreateRuleBody {
    pub name: String,
    pub condition: Condition,
    pub actions: Vec<Action>,
}

/// `GET /api/rules/:id` — قاعدة محددة.
pub async fn rules_get(
    State(state): State<GatewayState>,
    _ctx: axum::Extension<AuthContext>,
    Path(id): Path<String>,
) -> AxumResponse {
    let engine = match &state.rules {
        Some(e) => e,
        None => return error_response(StatusCode::SERVICE_UNAVAILABLE, "محرك القواعد غير مفعّل"),
    };
    match engine.get(&id) {
        Some(rule) => Json(json!({"rule": rule})).into_response(),
        None => error_response(StatusCode::NOT_FOUND, "قاعدة غير موجودة"),
    }
}

/// `DELETE /api/rules/:id` — حذف قاعدة.
pub async fn rules_delete(
    State(state): State<GatewayState>,
    ctx: axum::Extension<AuthContext>,
    Path(id): Path<String>,
) -> AxumResponse {
    let engine = match &state.rules {
        Some(e) => e,
        None => return error_response(StatusCode::SERVICE_UNAVAILABLE, "محرك القواعد غير مفعّل"),
    };

    state.audit.log(
        AuditEntry::new(&ctx.user_id, "rule.delete", &id)
            .with_category(AuditCategory::Rule),
    );

    match engine.delete(&id) {
        Ok(()) => Json(json!({"ok": true})).into_response(),
        Err(e) => error_response(StatusCode::NOT_FOUND, &e.to_string()),
    }
}

/// `POST /api/rules/:id/enable` — تفعيل قاعدة.
pub async fn rules_enable(
    State(state): State<GatewayState>,
    ctx: axum::Extension<AuthContext>,
    Path(id): Path<String>,
) -> AxumResponse {
    let engine = match &state.rules {
        Some(e) => e,
        None => return error_response(StatusCode::SERVICE_UNAVAILABLE, "محرك القواعد غير مفعّل"),
    };
    match engine.enable(&id) {
        Ok(()) => {
            state.audit.log(
                AuditEntry::new(&ctx.user_id, "rule.enable", &id)
                    .with_category(AuditCategory::Rule),
            );
            Json(json!({"ok": true})).into_response()
        }
        Err(e) => error_response(StatusCode::NOT_FOUND, &e.to_string()),
    }
}

/// `POST /api/rules/:id/disable` — تعطيل قاعدة.
pub async fn rules_disable(
    State(state): State<GatewayState>,
    ctx: axum::Extension<AuthContext>,
    Path(id): Path<String>,
) -> AxumResponse {
    let engine = match &state.rules {
        Some(e) => e,
        None => return error_response(StatusCode::SERVICE_UNAVAILABLE, "محرك القواعد غير مفعّل"),
    };
    match engine.disable(&id) {
        Ok(()) => {
            state.audit.log(
                AuditEntry::new(&ctx.user_id, "rule.disable", &id)
                    .with_category(AuditCategory::Rule),
            );
            Json(json!({"ok": true})).into_response()
        }
        Err(e) => error_response(StatusCode::NOT_FOUND, &e.to_string()),
    }
}

/// `GET /api/rules/stats` — إحصائيات محرك القواعد.
pub async fn rules_stats(
    State(state): State<GatewayState>,
    _ctx: axum::Extension<AuthContext>,
) -> AxumResponse {
    let engine = match &state.rules {
        Some(e) => e,
        None => return error_response(StatusCode::SERVICE_UNAVAILABLE, "محرك القواعد غير مفعّل"),
    };
    let stats = engine.stats();
    Json(json!({
        "total_rules": stats.total_rules,
        "enabled_rules": stats.enabled_rules,
        "disabled_rules": stats.disabled_rules,
        "total_executions": stats.total_executions,
        "total_successes": stats.total_successes,
        "total_failures": stats.total_failures,
    }))
    .into_response()
}
