//! خدمة Nexora للمصادقة/الهوية.
//!
//! أول خدمة إنتاجية مبنية على Nexora Core. توفر:
//! - إدارة المستخدمين (إنشاء، جلب، قائمة، حذف) مع تجزئة Argon2 لكلمات المرور
//! - رموز الجلسة (Ed25519 موقّعة، انتهاء صلاحية، تحديث)
//! - معالج NXP للمصادقة يوجّه AUTH_LOGIN / AUTH_LOGOUT / AUTH_REFRESH
//! - SSO مؤسسي عبر OIDC + SAML 2.0 (خلف علم ميزة `sso`)
//!
//! # التكامل مع النواة
//!
//! - إنشاء المستخدم يسجّل تلقائياً كياناً في محرك الصلاحيات
//! - كل تغييرات الحالة تنبعث كأحداث على ناقل الأحداث (`user.created`،
//!   `user.deleted`، `user.logged_in`، `user.logged_out`)
//! - الجلسات موسومة بالنسخة: تحديث الرمز يبطل الرمز السابق
//! - الرموز موقّعة بمفتاح هوية Ed25519 طويل المدى للخدمة

#![forbid(unsafe_code)]
#![warn(missing_docs, rust_2018_idioms)]

pub mod handler;
pub mod mfa;
pub mod password;
pub mod store;
pub mod token;
pub mod users;
pub mod webauthn;

#[cfg(feature = "sso")]
pub mod sso;

pub use handler::AuthHandler;
pub use password::{hash_password, verify_password, PasswordError};
pub use store::SessionStore;
pub use token::{SessionToken, TokenError, TokenVerifier};
pub use users::{User, UserError, UserStore, UserId};

use nexora_core::NexoraCore;
use nxp_security::IdentityKey;
use std::sync::Arc;

/// The Auth service. Owns references to Core subsystems + its own stores.
pub struct AuthService {
    /// User store.
    pub users: UserStore,
    /// Session store.
    pub sessions: SessionStore,
    /// Token verifier (holds the signing key).
    pub tokens: TokenVerifier,
    /// Reference to the Core (for permissions + events).
    pub core: Arc<NexoraCore>,
}

impl std::fmt::Debug for AuthService {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AuthService")
            .field("users", &self.users.user_count())
            .field("sessions", &self.sessions.session_count())
            .field("core", &self.core)
            .finish()
    }
}

impl AuthService {
    /// Construct a new AuthService with a fresh Ed25519 identity key.
    /// Wires the UserStore to the Core's Permission Engine and Event Bus
    /// so user creation auto-registers principals and emits events.
    /// In production, the key is loaded from the Secret Manager.
    pub fn new(core: Arc<NexoraCore>) -> Self {
        let identity = IdentityKey::generate();
        let users = UserStore::new()
            .with_permission_engine(core.permissions_inner())
            .with_event_bus(core.events_inner());
        Self {
            users,
            sessions: SessionStore::new(),
            tokens: TokenVerifier::new(identity),
            core,
        }
    }

    /// Construct with an existing identity key (e.g. loaded from vault).
    pub fn with_identity(core: Arc<NexoraCore>, identity: IdentityKey) -> Self {
        let users = UserStore::new()
            .with_permission_engine(core.permissions_inner())
            .with_event_bus(core.events_inner());
        Self {
            users,
            sessions: SessionStore::new(),
            tokens: TokenVerifier::new(identity),
            core,
        }
    }

    /// Returns a handler for dispatching NXP auth opcodes.
    pub fn handler(self: Arc<Self>) -> AuthHandler {
        AuthHandler::new(self)
    }
}
