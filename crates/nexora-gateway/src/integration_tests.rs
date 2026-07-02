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
