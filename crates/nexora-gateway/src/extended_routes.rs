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

// ==================================================================
// Security Routes
// ==================================================================

/// `GET /api/security/alerts` — قائمة التنبيهات الأمنية.
pub async fn security_alerts_list(
    State(state): State<GatewayState>,
    _ctx: axum::Extension<AuthContext>,
) -> AxumResponse {
    let alerts = state.security.list_alerts();
    let active = state.security.list_active_alerts().len();
    Json(json!({
        "alerts": alerts,
        "total": alerts.len(),
        "active": active,
    }))
    .into_response()
}

/// `GET /api/security/alerts/active` — التنبيهات النشطة فقط.
pub async fn security_alerts_active(
    State(state): State<GatewayState>,
    _ctx: axum::Extension<AuthContext>,
) -> AxumResponse {
    let alerts = state.security.list_active_alerts();
    Json(json!({
        "alerts": alerts,
        "count": alerts.len(),
    }))
    .into_response()
}

/// `GET /api/security/stats` — إحصائيات الأمان.
pub async fn security_stats(
    State(state): State<GatewayState>,
    _ctx: axum::Extension<AuthContext>,
) -> AxumResponse {
    let stats = state.security.stats();
    Json(json!({
        "total_alerts": stats.total_alerts,
        "active_alerts": stats.active_alerts,
        "resolved_alerts": stats.resolved_alerts,
        "critical_alerts": stats.critical_alerts,
        "high_alerts": stats.high_alerts,
        "last_alert_at": stats.last_alert_at,
    }))
    .into_response()
}

/// `POST /api/security/alerts/:id/resolve` — حل تنبيه.
pub async fn security_alert_resolve(
    State(state): State<GatewayState>,
    ctx: axum::Extension<AuthContext>,
    Path(id): Path<String>,
) -> AxumResponse {
    if state.security.resolve_alert(&id) {
        state.audit.log(
            AuditEntry::new(&ctx.user_id, "security.alert.resolve", &id)
                .with_category(AuditCategory::Auth),
        );
        Json(json!({"ok": true})).into_response()
    } else {
        error_response(StatusCode::NOT_FOUND, "تنبيه غير موجود")
    }
}

/// `POST /api/security/alerts/:id/dismiss` — تجاهل تنبيه.
pub async fn security_alert_dismiss(
    State(state): State<GatewayState>,
    ctx: axum::Extension<AuthContext>,
    Path(id): Path<String>,
) -> AxumResponse {
    if state.security.dismiss_alert(&id) {
        state.audit.log(
            AuditEntry::new(&ctx.user_id, "security.alert.dismiss", &id)
                .with_category(AuditCategory::Auth),
        );
        Json(json!({"ok": true})).into_response()
    } else {
        error_response(StatusCode::NOT_FOUND, "تنبيه غير موجود")
    }
}

/// `GET /api/security/alerts/:id` — تنبيه محدد.
pub async fn security_alert_get(
    State(state): State<GatewayState>,
    _ctx: axum::Extension<AuthContext>,
    Path(id): Path<String>,
) -> AxumResponse {
    match state.security.get_alert(&id) {
        Some(alert) => Json(json!({"alert": alert})).into_response(),
        None => error_response(StatusCode::NOT_FOUND, "تنبيه غير موجود"),
    }
}

// ==================================================================
// Audit Export Routes (CSV + JSON)
// ==================================================================

/// `GET /api/audit/export?format=json` — تصدير سجل التدقيق.
pub async fn audit_export(
    State(state): State<GatewayState>,
    _ctx: axum::Extension<AuthContext>,
    Query(params): Query<ExportParams>,
) -> AxumResponse {
    let filter = AuditFilter::new().with_limit(params.limit.unwrap_or(10000));
    let result = state.audit.query(&filter);

    if params.format.as_deref() == Some("csv") {
        // تصدير CSV.
        let mut csv = String::from("id,actor,action,target,category,success,timestamp\n");
        for e in &result.entries {
            csv.push_str(&format!(
                "{},{},{},{},{},{},{}\n",
                e.id,
                csv_escape(&e.actor),
                csv_escape(&e.action),
                csv_escape(&e.target),
                e.category.as_str(),
                e.success,
                e.timestamp,
            ));
        }
        (
            StatusCode::OK,
            [
                (axum::http::header::CONTENT_TYPE, "text/csv; charset=utf-8".to_string()),
                (axum::http::header::CONTENT_DISPOSITION, "attachment; filename=\"audit_log.csv\"".to_string()),
            ],
            csv,
        )
            .into_response()
    } else {
        // تصدير JSON.
        (
            StatusCode::OK,
            [
                (axum::http::header::CONTENT_TYPE, "application/json".to_string()),
                (axum::http::header::CONTENT_DISPOSITION, "attachment; filename=\"audit_log.json\"".to_string()),
            ],
            Json(json!({
                "entries": result.entries,
                "total": result.total,
                "exported_at": time::OffsetDateTime::now_utc().unix_timestamp_nanos() as i64,
            })),
        )
            .into_response()
    }
}

#[derive(Deserialize)]
pub struct ExportParams {
    pub format: Option<String>,
    pub limit: Option<usize>,
}

fn csv_escape(s: &str) -> String {
    if s.contains(',') || s.contains('"') || s.contains('\n') {
        format!("\"{}\"", s.replace('"', "\"\""))
    } else {
        s.to_string()
    }
}

// ==================================================================
// Security Policy Routes
// ==================================================================

/// `GET /api/security/policies` — قائمة السياسات.
pub async fn security_policies_list(
    State(state): State<GatewayState>,
    _ctx: axum::Extension<AuthContext>,
) -> AxumResponse {
    let policies = state.policies.list();
    Json(json!({
        "policies": policies,
        "total": policies.len(),
        "enabled": state.policies.enabled_count(),
    }))
    .into_response()
}

/// `POST /api/security/policies` — إنشاء سياسة.
pub async fn security_policies_create(
    State(state): State<GatewayState>,
    ctx: axum::Extension<AuthContext>,
    Json(body): Json<serde_json::Value>,
) -> AxumResponse {
    let name = body.get("name").and_then(|v| v.as_str()).unwrap_or("");
    let policy_type_str = body.get("policy_type").and_then(|v| v.as_str()).unwrap_or("custom");
    let action_str = body.get("action").and_then(|v| v.as_str()).unwrap_or("allow");

    if name.is_empty() {
        return error_response(StatusCode::BAD_REQUEST, "اسم السياسة مطلوب");
    }

    let policy_type = match policy_type_str {
        "require_mfa" => nexora_security::PolicyType::RequireMfa,
        "account_lockout" => nexora_security::PolicyType::AccountLockout,
        "max_sessions" => nexora_security::PolicyType::MaxSessions,
        "time_restriction" => nexora_security::PolicyType::TimeRestriction,
        "ip_restriction" => nexora_security::PolicyType::IpRestriction,
        "rate_limit" => nexora_security::PolicyType::RateLimit,
        "password_policy" => nexora_security::PolicyType::PasswordPolicy,
        "password_expiry" => nexora_security::PolicyType::PasswordExpiry,
        "session_policy" => nexora_security::PolicyType::SessionPolicy,
        _ => nexora_security::PolicyType::Custom,
    };

    let action = match action_str {
        "deny" => nexora_security::PolicyAction::Deny,
        "warn" => nexora_security::PolicyAction::Warn,
        "require_step_up" => nexora_security::PolicyAction::RequireStepUp,
        _ => nexora_security::PolicyAction::Allow,
    };

    let mut policy = nexora_security::SecurityPolicy::new(name, policy_type, action);
    if let Some(desc) = body.get("description").and_then(|v| v.as_str()) {
        policy = policy.with_description(desc);
    }
    if let Some(resources) = body.get("resources").and_then(|v| v.as_array()) {
        for r in resources {
            if let Some(s) = r.as_str() {
                policy = policy.with_resource(s);
            }
        }
    }

    let id = state.policies.register(policy);

    state.audit.log(
        AuditEntry::new(&ctx.user_id, "security.policy.create", &id)
            .with_category(AuditCategory::Auth),
    );

    Json(json!({"id": id, "ok": true})).into_response()
}

/// `DELETE /api/security/policies/:id` — حذف سياسة.
pub async fn security_policies_delete(
    State(state): State<GatewayState>,
    ctx: axum::Extension<AuthContext>,
    Path(id): Path<String>,
) -> AxumResponse {
    if state.policies.remove(&id) {
        state.audit.log(
            AuditEntry::new(&ctx.user_id, "security.policy.delete", &id)
                .with_category(AuditCategory::Auth),
        );
        Json(json!({"ok": true})).into_response()
    } else {
        error_response(StatusCode::NOT_FOUND, "سياسة غير موجودة")
    }
}

/// `POST /api/security/policies/:id/toggle` — تفعيل/تعطيل سياسة.
#[derive(Deserialize)]
pub struct PolicyToggleBody {
    pub enabled: bool,
}

pub async fn security_policies_toggle(
    State(state): State<GatewayState>,
    ctx: axum::Extension<AuthContext>,
    Path(id): Path<String>,
    Json(body): Json<PolicyToggleBody>,
) -> AxumResponse {
    if state.policies.set_enabled(&id, body.enabled) {
        state.audit.log(
            AuditEntry::new(&ctx.user_id, "security.policy.toggle", &id)
                .with_category(AuditCategory::Auth)
                .with_metadata("enabled", &body.enabled.to_string()),
        );
        Json(json!({"ok": true, "enabled": body.enabled})).into_response()
    } else {
        error_response(StatusCode::NOT_FOUND, "سياسة غير موجودة")
    }
}

/// `GET /api/security/policies/evaluate?resource=...` — تقييم السياسات.
pub async fn security_policies_evaluate(
    State(state): State<GatewayState>,
    _ctx: axum::Extension<AuthContext>,
    Query(params): Query<EvaluateParams>,
) -> AxumResponse {
    let resource = params.resource.unwrap_or_default();
    let evaluation = state.policies.evaluate(&resource, None);
    Json(json!({
        "action": match evaluation.action {
            nexora_security::PolicyAction::Allow => "allow",
            nexora_security::PolicyAction::Deny => "deny",
            nexora_security::PolicyAction::Warn => "warn",
            nexora_security::PolicyAction::RequireStepUp => "require_step_up",
        },
        "allowed": evaluation.is_allowed(),
        "reason": evaluation.reason,
        "severity": evaluation.severity.as_str(),
        "policy_id": evaluation.policy_id,
    }))
    .into_response()
}

#[derive(Deserialize)]
pub struct EvaluateParams {
    pub resource: Option<String>,
}

// ==================================================================
// Security Report Routes
// ==================================================================

/// `GET /api/security/reports/:period` — توليد تقرير أمني.
pub async fn security_report(
    State(state): State<GatewayState>,
    _ctx: axum::Extension<AuthContext>,
    Path(period_str): Path<String>,
) -> AxumResponse {
    let period = match period_str.as_str() {
        "daily" => nexora_security::ReportPeriod::Daily,
        "weekly" => nexora_security::ReportPeriod::Weekly,
        "monthly" => nexora_security::ReportPeriod::Monthly,
        _ => return error_response(StatusCode::BAD_REQUEST, "فترة غير صالحة (daily/weekly/monthly)"),
    };

    let alerts = state.security.list_alerts();
    let audit_filter = nexora_audit::AuditFilter::new().with_limit(10000);
    let audit_result = state.audit.query(&audit_filter);

    let report = nexora_security::ReportGenerator::generate(
        period,
        &alerts,
        &audit_result.entries,
    );

    Json(json!({
        "report": report,
    }))
    .into_response()
}

// ==================================================================
// WebAuthn Routes
// ==================================================================

/// `POST /api/auth/webauthn/register/begin` — بدء تسجيل مفتاح أمني.
pub async fn webauthn_register_begin(
    State(state): State<GatewayState>,
    ctx: axum::Extension<AuthContext>,
) -> AxumResponse {
    let challenge = state.webauthn.begin_registration(&ctx.user_id);
    state.audit.log(
        AuditEntry::new(&ctx.user_id, "webauthn.register.begin", &ctx.user_id)
            .with_category(AuditCategory::Auth),
    );
    Json(json!({
        "challenge": challenge.challenge,
        "expires_in_seconds": 300,
    }))
    .into_response()
}

/// `POST /api/auth/webauthn/register/complete` — إكمال تسجيل مفتاح أمني.
#[derive(Deserialize)]
pub struct WebAuthnRegisterCompleteBody {
    pub credential_id: String,
    pub public_key: String,
    pub authenticator_type: String,
    pub label: String,
}

pub async fn webauthn_register_complete(
    State(state): State<GatewayState>,
    ctx: axum::Extension<AuthContext>,
    Json(body): Json<WebAuthnRegisterCompleteBody>,
) -> AxumResponse {
    match state.webauthn.complete_registration(
        &ctx.user_id,
        "response",
        &body.credential_id,
        &body.public_key,
        &body.authenticator_type,
        &body.label,
    ) {
        Ok(result) => {
            state.audit.log(
                AuditEntry::new(&ctx.user_id, "webauthn.register.complete", &result.credential_id)
                    .with_category(AuditCategory::Auth),
            );
            Json(json!({
                "ok": true,
                "credential_id": result.credential_id,
                "label": result.label,
            }))
            .into_response()
        }
        Err(e) => error_response(StatusCode::BAD_REQUEST, &e),
    }
}

/// `GET /api/auth/webauthn/credentials` — قائمة مفاتيح الأمان.
pub async fn webauthn_list_credentials(
    State(state): State<GatewayState>,
    ctx: axum::Extension<AuthContext>,
) -> AxumResponse {
    let creds = state.webauthn.list_credentials(&ctx.user_id);
    Json(json!({
        "credentials": creds,
        "count": creds.len(),
        "registered": state.webauthn.is_registered(&ctx.user_id),
    }))
    .into_response()
}

/// `DELETE /api/auth/webauthn/credentials/:id` — حذف مفتاح أمان.
pub async fn webauthn_delete_credential(
    State(state): State<GatewayState>,
    ctx: axum::Extension<AuthContext>,
    Path(cred_id): Path<String>,
) -> AxumResponse {
    if state.webauthn.remove_credential(&ctx.user_id, &cred_id) {
        state.audit.log(
            AuditEntry::new(&ctx.user_id, "webauthn.credential.delete", &cred_id)
                .with_category(AuditCategory::Auth),
        );
        Json(json!({"ok": true})).into_response()
    } else {
        error_response(StatusCode::NOT_FOUND, "مفتاح غير موجود")
    }
}

/// `GET /api/auth/webauthn/stats` — إحصائيات WebAuthn.
pub async fn webauthn_stats(State(state): State<GatewayState>) -> AxumResponse {
    Json(json!({
        "registered_users": state.webauthn.registered_count(),
    }))
    .into_response()
}
