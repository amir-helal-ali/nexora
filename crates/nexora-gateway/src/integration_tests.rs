//! اختبارات تكامل شاملة للبوابة.
//!
//! هذه الاختبارات تتحقق من المسارات الجديدة:
//! - الإشعارات (CRUD كامل)
//! - SSO (قائمة المزودين، الإحصائيات)
//! - GraphQL (استعلامات، طفرات، introspection)

#![cfg(test)]

use axum::body::Body;
use axum::http::{Request, StatusCode};
use nexora_auth::AuthService;
use nexora_core::permissions::{Grant, Permission, Role};
use nexora_core::NexoraCore;
use std::sync::Arc;
use tower::ServiceExt;

/// إنشاء بوابة كاملة للاختبارات.
fn setup_server() -> crate::server::GatewayServer {
    let core = Arc::new(NexoraCore::new());
    core.permissions.register_role(Role {
        id: "admin".into(),
        description: "admin".into(),
        grants: vec![Grant {
            permission: Permission::Admin,
            resource: "*".into(),
        }],
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
    crate::server::GatewayServer::new(
        core,
        auth,
        marketplace,
        billing,
        workflow,
        cluster,
        notifications,
    )
}

/// الحصول على رمز مصادقة صالح + user_id.
async fn get_token(server: &crate::server::GatewayServer) -> (String, String) {
    let app = server.router();
    let resp = app
        .oneshot(
            Request::builder()
                .uri("/api/auth/login")
                .method("POST")
                .header("Content-Type", "application/json")
                .body(Body::from(
                    r#"{"username":"alice","password":"hunter2"}"#,
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let body = axum::body::to_bytes(resp.into_body(), 1024 * 1024)
        .await
        .unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    (
        json["token"].as_str().unwrap().to_string(),
        json["user_id"].as_str().unwrap().to_string(),
    )
}

// ==================================================================
// اختبارات الإشعارات
// ==================================================================

#[tokio::test]
async fn notifications_list_returns_empty_initially() {
    let server = setup_server();
    let (token, user_id) = get_token(&server).await;
    let app = server.router();

    let resp = app
        .oneshot(
            Request::builder()
                .uri("/api/notifications")
                .header("Authorization", format!("Bearer {token}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let body = axum::body::to_bytes(resp.into_body(), 1024 * 1024)
        .await
        .unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["count"], 0);
}

#[tokio::test]
async fn notifications_create_and_list() {
    let server = setup_server();
    let (token, user_id) = get_token(&server).await;

    // أنشئ إشعاراً.
    {
        let app = server.router();
        let resp = app
            .oneshot(
                Request::builder()
                    .uri("/api/notifications")
                    .method("POST")
                    .header("Authorization", format!("Bearer {token}"))
                    .header("Content-Type", "application/json")
                    .body(Body::from(
                        format!(r#"{{"user_id":"{}","title":"اختبار","body":"مرحبا"}}"#, user_id),
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::CREATED);
    }

    // اعرض القائمة.
    {
        let app = server.router();
        let resp = app
            .oneshot(
                Request::builder()
                    .uri("/api/notifications")
                    .header("Authorization", format!("Bearer {token}"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let body = axum::body::to_bytes(resp.into_body(), 1024 * 1024)
            .await
            .unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json["count"], 1);
    }
}

#[tokio::test]
async fn notifications_unread_count() {
    let server = setup_server();
    let (token, user_id) = get_token(&server).await;

    // أنشئ إشعاراً.
    {
        let app = server.router();
        let resp = app
            .oneshot(
                Request::builder()
                    .uri("/api/notifications")
                    .method("POST")
                    .header("Authorization", format!("Bearer {token}"))
                    .header("Content-Type", "application/json")
                    .body(Body::from(
                        format!(r#"{{"user_id":"{}","title":"T","body":"B"}}"#, user_id),
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::CREATED);
    }

    // تحقق من العدد غير المقروء.
    {
        let app = server.router();
        let resp = app
            .oneshot(
                Request::builder()
                    .uri("/api/notifications/unread-count")
                    .header("Authorization", format!("Bearer {token}"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let body = axum::body::to_bytes(resp.into_body(), 1024 * 1024)
            .await
            .unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json["unread"], 1);
    }
}

#[tokio::test]
async fn notifications_stats() {
    let server = setup_server();
    let (token, user_id) = get_token(&server).await;

    let app = server.router();
    let resp = app
        .oneshot(
            Request::builder()
                .uri("/api/notifications/stats")
                .header("Authorization", format!("Bearer {token}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let body = axum::body::to_bytes(resp.into_body(), 1024 * 1024)
        .await
        .unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert!(json["total"].is_number());
    assert!(json["delivered"].is_number());
    assert!(json["failed"].is_number());
    assert!(json["channels"].is_array());
}

#[tokio::test]
async fn notifications_create_validates_required_fields() {
    let server = setup_server();
    let (token, user_id) = get_token(&server).await;

    let app = server.router();
    let resp = app
        .oneshot(
            Request::builder()
                .uri("/api/notifications")
                .method("POST")
                .header("Authorization", format!("Bearer {token}"))
                .header("Content-Type", "application/json")
                .body(Body::from(r#"{"user_id":"","title":"","body":""}"#))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn notifications_protected_without_token() {
    let server = setup_server();
    let app = server.router();

    let resp = app
        .oneshot(
            Request::builder()
                .uri("/api/notifications")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
}

// ==================================================================
// اختبارات SSO
// ==================================================================

#[tokio::test]
async fn sso_providers_list_empty() {
    let server = setup_server();
    let (token, user_id) = get_token(&server).await;
    let app = server.router();

    let resp = app
        .oneshot(
            Request::builder()
                .uri("/api/auth/sso/providers")
                .header("Authorization", format!("Bearer {token}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let body = axum::body::to_bytes(resp.into_body(), 1024 * 1024)
        .await
        .unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["count"], 0);
}

#[tokio::test]
async fn sso_stats() {
    let server = setup_server();
    let (token, user_id) = get_token(&server).await;
    let app = server.router();

    let resp = app
        .oneshot(
            Request::builder()
                .uri("/api/auth/sso/stats")
                .header("Authorization", format!("Bearer {token}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let body = axum::body::to_bytes(resp.into_body(), 1024 * 1024)
        .await
        .unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert!(json["providers"].is_number());
    assert!(json["pending_flows"].is_number());
    assert!(json["active_sessions"].is_number());
}

#[tokio::test]
async fn sso_oidc_login_unknown_provider() {
    let server = setup_server();
    let app = server.router();

    let resp = app
        .oneshot(
            Request::builder()
                .uri("/api/auth/sso/oidc/nonexistent/login")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn sso_saml_login_unknown_provider() {
    let server = setup_server();
    let app = server.router();

    let resp = app
        .oneshot(
            Request::builder()
                .uri("/api/auth/sso/saml/nonexistent/login")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn sso_management_protected() {
    let server = setup_server();
    let app = server.router();

    let resp = app
        .oneshot(
            Request::builder()
                .uri("/api/auth/sso/providers")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
}

// ==================================================================
// اختبارات GraphQL
// ==================================================================

#[tokio::test]
async fn graphql_health_query() {
    let server = setup_server();
    let app = server.router();

    let resp = app
        .oneshot(
            Request::builder()
                .uri("/api/graphql")
                .method("POST")
                .header("Content-Type", "application/json")
                .body(Body::from(
                    r#"{"query":"{ health { healthy eventsPublished } }"}"#,
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let body = axum::body::to_bytes(resp.into_body(), 1024 * 1024)
        .await
        .unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert!(json["data"]["health"]["healthy"].as_bool().unwrap_or(false));
}

#[tokio::test]
async fn graphql_introspection() {
    let server = setup_server();
    let app = server.router();

    let resp = app
        .oneshot(
            Request::builder()
                .uri("/api/graphql")
                .method("POST")
                .header("Content-Type", "application/json")
                .body(Body::from(
                    r#"{"query":"{ __schema { queryType { name } } }"}"#,
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let body = axum::body::to_bytes(resp.into_body(), 1024 * 1024)
        .await
        .unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["data"]["__schema"]["queryType"]["name"], "Query");
}

#[tokio::test]
async fn graphql_events_query() {
    let server = setup_server();
    let (token, user_id) = get_token(&server).await;

    // انشر حدثاً أولاً.
    {
        let app = server.router();
        let _ = app
            .oneshot(
                Request::builder()
                    .uri("/api/core/events")
                    .method("POST")
                    .header("Authorization", format!("Bearer {token}"))
                    .header("Content-Type", "application/json")
                    .body(Body::from(
                        r#"{"name":"test.event","payload":"hello"}"#,
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
    }

    // استعلم عن الأحداث.
    let app = server.router();
    let resp = app
        .oneshot(
            Request::builder()
                .uri("/api/graphql")
                .method("POST")
                .header("Content-Type", "application/json")
                .body(Body::from(
                    r#"{"query":"{ events(fromId: 0, limit: 10) { id name } }"}"#,
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let body = axum::body::to_bytes(resp.into_body(), 1024 * 1024)
        .await
        .unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    let events = json["data"]["events"].as_array().unwrap();
    assert!(!events.is_empty());
}

#[tokio::test]
async fn graphql_create_notification_mutation() {
    let server = setup_server();
    let app = server.router();

    let resp = app
        .oneshot(
            Request::builder()
                .uri("/api/graphql")
                .method("POST")
                .header("Content-Type", "application/json")
                .body(Body::from(
                    r#"{"query":"mutation { createNotification(input: { userId: \"u1\", title: \"T\", body: \"B\" }) { id userId title } }"}"#,
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let body = axum::body::to_bytes(resp.into_body(), 1024 * 1024)
        .await
        .unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["data"]["createNotification"]["userId"], "u1");
    assert_eq!(json["data"]["createNotification"]["title"], "T");
    assert!(!json["data"]["createNotification"]["id"]
        .as_str()
        .unwrap()
        .is_empty());
}

#[tokio::test]
async fn graphql_playground_html() {
    let server = setup_server();
    let app = server.router();

    let resp = app
        .oneshot(
            Request::builder()
                .uri("/api/graphql")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let body = axum::body::to_bytes(resp.into_body(), 1024 * 1024)
        .await
        .unwrap();
    let html = String::from_utf8(body.to_vec()).unwrap();
    assert!(html.contains("Nexora GraphQL"));
    assert!(html.contains("Playground"));
}

#[tokio::test]
async fn graphql_missing_query_returns_400() {
    let server = setup_server();
    let app = server.router();

    let resp = app
        .oneshot(
            Request::builder()
                .uri("/api/graphql")
                .method("POST")
                .header("Content-Type", "application/json")
                .body(Body::from(r#"{"variables":{}}"#))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
}

// ==================================================================
// اختبارات مسارات عامة
// ==================================================================

#[tokio::test]
async fn health_endpoint_works() {
    let server = setup_server();
    let app = server.router();

    let resp = app
        .oneshot(
            Request::builder()
                .uri("/api/health")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
}

#[tokio::test]
async fn openapi_spec_works() {
    let server = setup_server();
    let app = server.router();

    let resp = app
        .oneshot(
            Request::builder()
                .uri("/api/openapi.json")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
}

// ==================================================================
// اختبارات MFA
// ==================================================================

#[tokio::test]
async fn mfa_status_unenrolled() {
    let server = setup_server();
    let (token, _user_id) = get_token(&server).await;
    let app = server.router();

    let resp = app
        .oneshot(
            Request::builder()
                .uri("/api/auth/mfa/status")
                .header("Authorization", format!("Bearer {token}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let body = axum::body::to_bytes(resp.into_body(), 1024 * 1024)
        .await
        .unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["enrolled"], false);
}

#[tokio::test]
async fn mfa_enroll_begin_returns_secret() {
    let server = setup_server();
    let (token, _user_id) = get_token(&server).await;
    let app = server.router();

    let resp = app
        .oneshot(
            Request::builder()
                .uri("/api/auth/mfa/enroll/begin")
                .method("POST")
                .header("Authorization", format!("Bearer {token}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let body = axum::body::to_bytes(resp.into_body(), 1024 * 1024)
        .await
        .unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert!(json["secret"].as_str().unwrap().len() > 10);
    assert!(json["otpauth_url"].as_str().unwrap().starts_with("otpauth://"));
    assert!(json["backup_codes"].is_array());
}

#[tokio::test]
async fn mfa_verify_unenrolled_returns_not_enrolled() {
    let server = setup_server();
    let (token, _user_id) = get_token(&server).await;
    let app = server.router();

    let resp = app
        .oneshot(
            Request::builder()
                .uri("/api/auth/mfa/verify")
                .method("POST")
                .header("Authorization", format!("Bearer {token}"))
                .header("Content-Type", "application/json")
                .body(Body::from(r#"{"code":"123456"}"#))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let body = axum::body::to_bytes(resp.into_body(), 1024 * 1024)
        .await
        .unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["valid"], false);
    assert!(json["message"].as_str().unwrap().contains("غير مُفعّل"));
}

#[tokio::test]
async fn mfa_disable_without_enrollment() {
    let server = setup_server();
    let (token, _user_id) = get_token(&server).await;
    let app = server.router();

    let resp = app
        .oneshot(
            Request::builder()
                .uri("/api/auth/mfa/disable")
                .method("POST")
                .header("Authorization", format!("Bearer {token}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let body = axum::body::to_bytes(resp.into_body(), 1024 * 1024)
        .await
        .unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["was_enabled"], false);
}

#[tokio::test]
async fn mfa_stats() {
    let server = setup_server();
    let (token, _user_id) = get_token(&server).await;
    let app = server.router();

    let resp = app
        .oneshot(
            Request::builder()
                .uri("/api/auth/mfa/stats")
                .header("Authorization", format!("Bearer {token}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let body = axum::body::to_bytes(resp.into_body(), 1024 * 1024)
        .await
        .unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["enrolled_users"], 0);
}

#[tokio::test]
async fn mfa_routes_protected() {
    let server = setup_server();
    let app = server.router();

    let resp = app
        .oneshot(
            Request::builder()
                .uri("/api/auth/mfa/status")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
}

// ==================================================================
// اختبارات Audit
// ==================================================================

#[tokio::test]
async fn audit_stats_initially_empty() {
    let server = setup_server();
    let (token, _user_id) = get_token(&server).await;
    let app = server.router();

    let resp = app
        .oneshot(
            Request::builder()
                .uri("/api/audit/stats")
                .header("Authorization", format!("Bearer {token}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let body = axum::body::to_bytes(resp.into_body(), 1024 * 1024)
        .await
        .unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert!(json["total"].is_number());
}

#[tokio::test]
async fn audit_list_returns_entries() {
    let server = setup_server();
    let (token, _user_id) = get_token(&server).await;

    // أنشئ بعض مدخلات التدقيق عبر مسار MFA.
    {
        let app = server.router();
        let _ = app
            .oneshot(
                Request::builder()
                    .uri("/api/auth/mfa/enroll/begin")
                    .method("POST")
                    .header("Authorization", format!("Bearer {token}"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
    }

    // استعلم عن المدخلات.
    let app = server.router();
    let resp = app
        .oneshot(
            Request::builder()
                .uri("/api/audit/entries?limit=10")
                .header("Authorization", format!("Bearer {token}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let body = axum::body::to_bytes(resp.into_body(), 1024 * 1024)
        .await
        .unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert!(json["total"].as_u64().unwrap() > 0);
    assert!(json["entries"].is_array());
}

#[tokio::test]
async fn audit_list_filter_by_action() {
    let server = setup_server();
    let (token, _user_id) = get_token(&server).await;

    // أنشئ مدخلات.
    {
        let app = server.router();
        let _ = app
            .oneshot(
                Request::builder()
                    .uri("/api/auth/mfa/enroll/begin")
                    .method("POST")
                    .header("Authorization", format!("Bearer {token}"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
    }

    // فلتر بالإجراء.
    let app = server.router();
    let resp = app
        .oneshot(
            Request::builder()
                .uri("/api/audit/entries?action=mfa.enroll.begin")
                .header("Authorization", format!("Bearer {token}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let body = axum::body::to_bytes(resp.into_body(), 1024 * 1024)
        .await
        .unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert!(json["total"].as_u64().unwrap() > 0);
}

#[tokio::test]
async fn audit_get_nonexistent_returns_404() {
    let server = setup_server();
    let (token, _user_id) = get_token(&server).await;
    let app = server.router();

    let resp = app
        .oneshot(
            Request::builder()
                .uri("/api/audit/nonexistent-id")
                .header("Authorization", format!("Bearer {token}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn audit_routes_protected() {
    let server = setup_server();
    let app = server.router();

    let resp = app
        .oneshot(
            Request::builder()
                .uri("/api/audit/entries")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
}

// ==================================================================
// اختبارات Rules
// ==================================================================

#[tokio::test]
async fn rules_list_empty() {
    let server = setup_server();
    let (token, _user_id) = get_token(&server).await;
    let app = server.router();

    let resp = app
        .oneshot(
            Request::builder()
                .uri("/api/rules")
                .header("Authorization", format!("Bearer {token}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let body = axum::body::to_bytes(resp.into_body(), 1024 * 1024)
        .await
        .unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["count"], 0);
}

#[tokio::test]
async fn rules_create_and_list() {
    let server = setup_server();
    let (token, _user_id) = get_token(&server).await;

    // أنشئ قاعدة.
    {
        let app = server.router();
        let resp = app
            .oneshot(
                Request::builder()
                    .uri("/api/rules")
                    .method("POST")
                    .header("Authorization", format!("Bearer {token}"))
                    .header("Content-Type", "application/json")
                    .body(Body::from(
                        r#"{"name":"test-rule","condition":{"kind":{"type":"always"}},"actions":[]}"#,
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
    }

    // اعرض القائمة.
    {
        let app = server.router();
        let resp = app
            .oneshot(
                Request::builder()
                    .uri("/api/rules")
                    .header("Authorization", format!("Bearer {token}"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let body = axum::body::to_bytes(resp.into_body(), 1024 * 1024)
            .await
            .unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json["count"], 1);
    }
}

#[tokio::test]
async fn rules_stats() {
    let server = setup_server();
    let (token, _user_id) = get_token(&server).await;
    let app = server.router();

    let resp = app
        .oneshot(
            Request::builder()
                .uri("/api/rules/stats")
                .header("Authorization", format!("Bearer {token}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let body = axum::body::to_bytes(resp.into_body(), 1024 * 1024)
        .await
        .unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert!(json["total_rules"].is_number());
    assert!(json["enabled_rules"].is_number());
}

#[tokio::test]
async fn rules_get_nonexistent_returns_404() {
    let server = setup_server();
    let (token, _user_id) = get_token(&server).await;
    let app = server.router();

    let resp = app
        .oneshot(
            Request::builder()
                .uri("/api/rules/nonexistent-id")
                .header("Authorization", format!("Bearer {token}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn rules_delete_nonexistent_returns_404() {
    let server = setup_server();
    let (token, _user_id) = get_token(&server).await;
    let app = server.router();

    let resp = app
        .oneshot(
            Request::builder()
                .uri("/api/rules/nonexistent-id")
                .method("DELETE")
                .header("Authorization", format!("Bearer {token}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn rules_routes_protected() {
    let server = setup_server();
    let app = server.router();

    let resp = app
        .oneshot(
            Request::builder()
                .uri("/api/rules")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn rules_enable_nonexistent_returns_404() {
    let server = setup_server();
    let (token, _user_id) = get_token(&server).await;
    let app = server.router();

    let resp = app
        .oneshot(
            Request::builder()
                .uri("/api/rules/nonexistent/enable")
                .method("POST")
                .header("Authorization", format!("Bearer {token}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn rules_disable_nonexistent_returns_404() {
    let server = setup_server();
    let (token, _user_id) = get_token(&server).await;
    let app = server.router();

    let resp = app
        .oneshot(
            Request::builder()
                .uri("/api/rules/nonexistent/disable")
                .method("POST")
                .header("Authorization", format!("Bearer {token}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::NOT_FOUND);
}

// ==================================================================
// اختبارات Security
// ==================================================================

#[tokio::test]
async fn security_stats_initially_empty() {
    let server = setup_server();
    let (token, _user_id) = get_token(&server).await;
    let app = server.router();

    let resp = app
        .oneshot(
            Request::builder()
                .uri("/api/security/stats")
                .header("Authorization", format!("Bearer {token}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let body = axum::body::to_bytes(resp.into_body(), 1024 * 1024)
        .await
        .unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["total_alerts"], 0);
}

#[tokio::test]
async fn security_alerts_list_empty() {
    let server = setup_server();
    let (token, _user_id) = get_token(&server).await;
    let app = server.router();

    let resp = app
        .oneshot(
            Request::builder()
                .uri("/api/security/alerts")
                .header("Authorization", format!("Bearer {token}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let body = axum::body::to_bytes(resp.into_body(), 1024 * 1024)
        .await
        .unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["total"], 0);
}

#[tokio::test]
async fn security_alerts_active_empty() {
    let server = setup_server();
    let (token, _user_id) = get_token(&server).await;
    let app = server.router();

    let resp = app
        .oneshot(
            Request::builder()
                .uri("/api/security/alerts/active")
                .header("Authorization", format!("Bearer {token}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let body = axum::body::to_bytes(resp.into_body(), 1024 * 1024)
        .await
        .unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["count"], 0);
}

#[tokio::test]
async fn security_alert_get_nonexistent_returns_404() {
    let server = setup_server();
    let (token, _user_id) = get_token(&server).await;
    let app = server.router();

    let resp = app
        .oneshot(
            Request::builder()
                .uri("/api/security/alerts/nonexistent-id")
                .header("Authorization", format!("Bearer {token}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn security_alert_resolve_nonexistent_returns_404() {
    let server = setup_server();
    let (token, _user_id) = get_token(&server).await;
    let app = server.router();

    let resp = app
        .oneshot(
            Request::builder()
                .uri("/api/security/alerts/nonexistent-id/resolve")
                .method("POST")
                .header("Authorization", format!("Bearer {token}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn security_alert_dismiss_nonexistent_returns_404() {
    let server = setup_server();
    let (token, _user_id) = get_token(&server).await;
    let app = server.router();

    let resp = app
        .oneshot(
            Request::builder()
                .uri("/api/security/alerts/nonexistent-id/dismiss")
                .method("POST")
                .header("Authorization", format!("Bearer {token}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn security_routes_protected() {
    let server = setup_server();
    let app = server.router();

    let resp = app
        .oneshot(
            Request::builder()
                .uri("/api/security/stats")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
}

// ==================================================================
// اختبارات Audit Export
// ==================================================================

#[tokio::test]
async fn audit_export_json() {
    let server = setup_server();
    let (token, _user_id) = get_token(&server).await;
    let app = server.router();

    let resp = app
        .oneshot(
            Request::builder()
                .uri("/api/audit/export?format=json")
                .header("Authorization", format!("Bearer {token}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let headers = resp.headers();
    assert!(headers
        .get("content-disposition")
        .unwrap()
        .to_str()
        .unwrap()
        .contains("audit_log.json"));
}

#[tokio::test]
async fn audit_export_csv() {
    let server = setup_server();
    let (token, _user_id) = get_token(&server).await;
    let app = server.router();

    let resp = app
        .oneshot(
            Request::builder()
                .uri("/api/audit/export?format=csv")
                .header("Authorization", format!("Bearer {token}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let headers = resp.headers();
    assert!(headers
        .get("content-disposition")
        .unwrap()
        .to_str()
        .unwrap()
        .contains("audit_log.csv"));
    let body = axum::body::to_bytes(resp.into_body(), 1024 * 1024)
        .await
        .unwrap();
    let csv = String::from_utf8(body.to_vec()).unwrap();
    assert!(csv.starts_with("id,actor,action,target,category,success,timestamp"));
}

#[tokio::test]
async fn audit_export_default_is_json() {
    let server = setup_server();
    let (token, _user_id) = get_token(&server).await;
    let app = server.router();

    let resp = app
        .oneshot(
            Request::builder()
                .uri("/api/audit/export")
                .header("Authorization", format!("Bearer {token}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let headers = resp.headers();
    assert!(headers
        .get("content-type")
        .unwrap()
        .to_str()
        .unwrap()
        .contains("application/json"));
}

#[tokio::test]
async fn audit_export_protected() {
    let server = setup_server();
    let app = server.router();

    let resp = app
        .oneshot(
            Request::builder()
                .uri("/api/audit/export")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
}

// ==================================================================
// اختبارات Security Policies
// ==================================================================

#[tokio::test]
async fn security_policies_list_empty() {
    let server = setup_server();
    let (token, _) = get_token(&server).await;
    let app = server.router();

    let resp = app
        .oneshot(
            Request::builder()
                .uri("/api/security/policies")
                .header("Authorization", format!("Bearer {token}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let body = axum::body::to_bytes(resp.into_body(), 1024 * 1024)
        .await
        .unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["total"], 0);
}

#[tokio::test]
async fn security_policies_create_and_list() {
    let server = setup_server();
    let (token, _) = get_token(&server).await;

    // أنشئ سياسة.
    {
        let app = server.router();
        let resp = app
            .oneshot(
                Request::builder()
                    .uri("/api/security/policies")
                    .method("POST")
                    .header("Authorization", format!("Bearer {token}"))
                    .header("Content-Type", "application/json")
                    .body(Body::from(
                        r#"{"name":"test-policy","policy_type":"rate_limit","action":"deny","resources":["api/billing/*"]}"#,
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
    }

    // اعرض القائمة.
    {
        let app = server.router();
        let resp = app
            .oneshot(
                Request::builder()
                    .uri("/api/security/policies")
                    .header("Authorization", format!("Bearer {token}"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let body = axum::body::to_bytes(resp.into_body(), 1024 * 1024)
            .await
            .unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json["total"], 1);
        assert_eq!(json["enabled"], 1);
    }
}

#[tokio::test]
async fn security_policies_create_validates_name() {
    let server = setup_server();
    let (token, _) = get_token(&server).await;
    let app = server.router();

    let resp = app
        .oneshot(
            Request::builder()
                .uri("/api/security/policies")
                .method("POST")
                .header("Authorization", format!("Bearer {token}"))
                .header("Content-Type", "application/json")
                .body(Body::from(r#"{"name":"","policy_type":"custom","action":"allow"}"#))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn security_policies_evaluate() {
    let server = setup_server();
    let (token, _) = get_token(&server).await;
    let app = server.router();

    let resp = app
        .oneshot(
            Request::builder()
                .uri("/api/security/policies/evaluate?resource=api/test")
                .header("Authorization", format!("Bearer {token}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let body = axum::body::to_bytes(resp.into_body(), 1024 * 1024)
        .await
        .unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["allowed"], true);
}

#[tokio::test]
async fn security_policies_delete_nonexistent() {
    let server = setup_server();
    let (token, _) = get_token(&server).await;
    let app = server.router();

    let resp = app
        .oneshot(
            Request::builder()
                .uri("/api/security/policies/nonexistent-id")
                .method("DELETE")
                .header("Authorization", format!("Bearer {token}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn security_policies_toggle_nonexistent() {
    let server = setup_server();
    let (token, _) = get_token(&server).await;
    let app = server.router();

    let resp = app
        .oneshot(
            Request::builder()
                .uri("/api/security/policies/nonexistent-id/toggle")
                .method("POST")
                .header("Authorization", format!("Bearer {token}"))
                .header("Content-Type", "application/json")
                .body(Body::from(r#"{"enabled":false}"#))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn security_policies_routes_protected() {
    let server = setup_server();
    let app = server.router();

    let resp = app
        .oneshot(
            Request::builder()
                .uri("/api/security/policies")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
}

// ==================================================================
// اختبارات Security Reports
// ==================================================================

#[tokio::test]
async fn security_report_daily() {
    let server = setup_server();
    let (token, _) = get_token(&server).await;
    let app = server.router();

    let resp = app
        .oneshot(
            Request::builder()
                .uri("/api/security/reports/daily")
                .header("Authorization", format!("Bearer {token}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let body = axum::body::to_bytes(resp.into_body(), 1024 * 1024)
        .await
        .unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert!(json["report"]["total_alerts"].is_number());
    assert!(json["report"]["summary"].as_str().unwrap().contains("يومي"));
}

#[tokio::test]
async fn security_report_weekly() {
    let server = setup_server();
    let (token, _) = get_token(&server).await;
    let app = server.router();

    let resp = app
        .oneshot(
            Request::builder()
                .uri("/api/security/reports/weekly")
                .header("Authorization", format!("Bearer {token}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let body = axum::body::to_bytes(resp.into_body(), 1024 * 1024)
        .await
        .unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert!(json["report"]["summary"].as_str().unwrap().contains("أسبوعي"));
}

#[tokio::test]
async fn security_report_monthly() {
    let server = setup_server();
    let (token, _) = get_token(&server).await;
    let app = server.router();

    let resp = app
        .oneshot(
            Request::builder()
                .uri("/api/security/reports/monthly")
                .header("Authorization", format!("Bearer {token}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let body = axum::body::to_bytes(resp.into_body(), 1024 * 1024)
        .await
        .unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert!(json["report"]["summary"].as_str().unwrap().contains("شهري"));
}

#[tokio::test]
async fn security_report_invalid_period() {
    let server = setup_server();
    let (token, _) = get_token(&server).await;
    let app = server.router();

    let resp = app
        .oneshot(
            Request::builder()
                .uri("/api/security/reports/invalid")
                .header("Authorization", format!("Bearer {token}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn security_report_protected() {
    let server = setup_server();
    let app = server.router();

    let resp = app
        .oneshot(
            Request::builder()
                .uri("/api/security/reports/daily")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
}

// ==================================================================
// اختبارات WebAuthn
// ==================================================================

#[tokio::test]
async fn webauthn_register_begin() {
    let server = setup_server();
    let (token, _) = get_token(&server).await;
    let app = server.router();

    let resp = app
        .oneshot(
            Request::builder()
                .uri("/api/auth/webauthn/register/begin")
                .method("POST")
                .header("Authorization", format!("Bearer {token}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let body = axum::body::to_bytes(resp.into_body(), 1024 * 1024)
        .await
        .unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert!(!json["challenge"].as_str().unwrap().is_empty());
    assert_eq!(json["expires_in_seconds"], 300);
}

#[tokio::test]
async fn webauthn_list_credentials_empty() {
    let server = setup_server();
    let (token, _) = get_token(&server).await;
    let app = server.router();

    let resp = app
        .oneshot(
            Request::builder()
                .uri("/api/auth/webauthn/credentials")
                .header("Authorization", format!("Bearer {token}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let body = axum::body::to_bytes(resp.into_body(), 1024 * 1024)
        .await
        .unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["count"], 0);
    assert_eq!(json["registered"], false);
}

#[tokio::test]
async fn webauthn_stats() {
    let server = setup_server();
    let (token, _) = get_token(&server).await;
    let app = server.router();

    let resp = app
        .oneshot(
            Request::builder()
                .uri("/api/auth/webauthn/stats")
                .header("Authorization", format!("Bearer {token}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let body = axum::body::to_bytes(resp.into_body(), 1024 * 1024)
        .await
        .unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["registered_users"], 0);
}

#[tokio::test]
async fn webauthn_delete_nonexistent_credential() {
    let server = setup_server();
    let (token, _) = get_token(&server).await;
    let app = server.router();

    let resp = app
        .oneshot(
            Request::builder()
                .uri("/api/auth/webauthn/credentials/nonexistent")
                .method("DELETE")
                .header("Authorization", format!("Bearer {token}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn webauthn_routes_protected() {
    let server = setup_server();
    let app = server.router();

    let resp = app
        .oneshot(
            Request::builder()
                .uri("/api/auth/webauthn/credentials")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn webauthn_full_registration_flow() {
    let server = setup_server();
    let (token, _) = get_token(&server).await;

    // ابدأ التسجيل.
    {
        let app = server.router();
        let resp = app
            .oneshot(
                Request::builder()
                    .uri("/api/auth/webauthn/register/begin")
                    .method("POST")
                    .header("Authorization", format!("Bearer {token}"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
    }

    // أكمل التسجيل.
    {
        let app = server.router();
        let resp = app
            .oneshot(
                Request::builder()
                    .uri("/api/auth/webauthn/register/complete")
                    .method("POST")
                    .header("Authorization", format!("Bearer {token}"))
                    .header("Content-Type", "application/json")
                    .body(Body::from(
                        r#"{"credential_id":"cred-1","public_key":"pk-1","authenticator_type":"yubikey","label":"مفتاحي"}"#,
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
    }

    // تحقق من القائمة.
    {
        let app = server.router();
        let resp = app
            .oneshot(
                Request::builder()
                    .uri("/api/auth/webauthn/credentials")
                    .header("Authorization", format!("Bearer {token}"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let body = axum::body::to_bytes(resp.into_body(), 1024 * 1024)
            .await
            .unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json["count"], 1);
        assert_eq!(json["registered"], true);
    }
}

// ==================================================================
// اختبارات Monitoring
// ==================================================================

#[tokio::test]
async fn monitoring_snapshot_works() {
    let server = setup_server();
    let (token, _) = get_token(&server).await;
    let app = server.router();

    let resp = app
        .oneshot(
            Request::builder()
                .uri("/api/monitoring/snapshot")
                .header("Authorization", format!("Bearer {token}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let body = axum::body::to_bytes(resp.into_body(), 1024 * 1024)
        .await
        .unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["overall_health"], "healthy");
    assert_eq!(json["total_requests"], 0);
}

#[tokio::test]
async fn monitoring_metrics_works() {
    let server = setup_server();
    let (token, _) = get_token(&server).await;
    let app = server.router();

    let resp = app
        .oneshot(
            Request::builder()
                .uri("/api/monitoring/metrics")
                .header("Authorization", format!("Bearer {token}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let body = axum::body::to_bytes(resp.into_body(), 1024 * 1024)
        .await
        .unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert!(json["total_requests"].is_number());
    assert!(json["success_rate"].is_number());
}

#[tokio::test]
async fn monitoring_paths_empty() {
    let server = setup_server();
    let (token, _) = get_token(&server).await;
    let app = server.router();

    let resp = app
        .oneshot(
            Request::builder()
                .uri("/api/monitoring/paths")
                .header("Authorization", format!("Bearer {token}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let body = axum::body::to_bytes(resp.into_body(), 1024 * 1024)
        .await
        .unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["count"], 0);
}

#[tokio::test]
async fn monitoring_health_works() {
    let server = setup_server();
    let (token, _) = get_token(&server).await;
    let app = server.router();

    let resp = app
        .oneshot(
            Request::builder()
                .uri("/api/monitoring/health")
                .header("Authorization", format!("Bearer {token}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let body = axum::body::to_bytes(resp.into_body(), 1024 * 1024)
        .await
        .unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["overall_status"], "healthy");
}

#[tokio::test]
async fn monitoring_reset_works() {
    let server = setup_server();
    let (token, _) = get_token(&server).await;
    let app = server.router();

    let resp = app
        .oneshot(
            Request::builder()
                .uri("/api/monitoring/reset")
                .method("POST")
                .header("Authorization", format!("Bearer {token}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
}

#[tokio::test]
async fn monitoring_routes_protected() {
    let server = setup_server();
    let app = server.router();

    let resp = app
        .oneshot(
            Request::builder()
                .uri("/api/monitoring/snapshot")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
}

// ==================================================================
// اختبارات Security Presets
// ==================================================================

#[tokio::test]
async fn security_presets_list() {
    let server = setup_server();
    let (token, _) = get_token(&server).await;
    let app = server.router();

    let resp = app
        .oneshot(
            Request::builder()
                .uri("/api/security/presets")
                .header("Authorization", format!("Bearer {token}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let body = axum::body::to_bytes(resp.into_body(), 1024 * 1024)
        .await
        .unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["count"], 3);
}

#[tokio::test]
async fn security_presets_apply_basic() {
    let server = setup_server();
    let (token, _) = get_token(&server).await;
    let app = server.router();

    let resp = app
        .oneshot(
            Request::builder()
                .uri("/api/security/presets/basic/apply")
                .method("POST")
                .header("Authorization", format!("Bearer {token}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let body = axum::body::to_bytes(resp.into_body(), 1024 * 1024)
        .await
        .unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["ok"], true);
    assert_eq!(json["policies_created"], 3);
}

#[tokio::test]
async fn security_presets_apply_enterprise() {
    let server = setup_server();
    let (token, _) = get_token(&server).await;
    let app = server.router();

    let resp = app
        .oneshot(
            Request::builder()
                .uri("/api/security/presets/enterprise/apply")
                .method("POST")
                .header("Authorization", format!("Bearer {token}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let body = axum::body::to_bytes(resp.into_body(), 1024 * 1024)
        .await
        .unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["policies_created"], 5);
}

#[tokio::test]
async fn security_presets_apply_high_security() {
    let server = setup_server();
    let (token, _) = get_token(&server).await;
    let app = server.router();

    let resp = app
        .oneshot(
            Request::builder()
                .uri("/api/security/presets/high_security/apply")
                .method("POST")
                .header("Authorization", format!("Bearer {token}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let body = axum::body::to_bytes(resp.into_body(), 1024 * 1024)
        .await
        .unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["policies_created"], 6);
}

#[tokio::test]
async fn security_presets_apply_invalid() {
    let server = setup_server();
    let (token, _) = get_token(&server).await;
    let app = server.router();

    let resp = app
        .oneshot(
            Request::builder()
                .uri("/api/security/presets/invalid/apply")
                .method("POST")
                .header("Authorization", format!("Bearer {token}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn security_presets_routes_protected() {
    let server = setup_server();
    let app = server.router();

    let resp = app
        .oneshot(
            Request::builder()
                .uri("/api/security/presets")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
}
