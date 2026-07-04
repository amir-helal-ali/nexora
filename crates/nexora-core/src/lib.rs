//! Nexora Core — نواة نظام التشغيل السحابي.
//!
//! انظر مواصفة Nexora الهندسية، الجزء 4. النواة مسؤولة عن تحميل، إدارة،
//! تأمين، مراقبة، وتنسيق كل قدرة في المنصة. لا شيء داخل المنصة يعمل
//! بشكل مستقل عن النواة.
//!
//! # الأنظمة الفرعية
//!
//! - [`modules`]: مدير الوحدات — دورة حياة تثبيت/تفعيل/تعطيل/ترقية
//! - [`registry`]: سجل الخدمات — بحث اسم منطقي ← خدمة
//! - [`events`]: ناقل الأحداث — أحداث دائمة، قابلة لإعادة التشغيل، مرتبة
//! - [`permissions`]: محرك الصلاحيات — RBAC + ABAC هرمي
//! - [`plugins`]: نظام المكونات — موقّع، في صندوق حماية، بموارد محدودة
//! - [`config`]: مدير التكوين — إعادة تحميل ديناميكية
//! - [`secrets`]: مدير الأسرار — مشفّر، موسوم بالنسخ، مدقّق
//! - [`health`]: مراقب الصحة — حياة، جاهزية، إحصائيات
//! - [`handler`]: معالج NXP للنواة — يوجّه أوامر NXP إلى الأنظمة الفرعية

#![forbid(unsafe_code)]
#![warn(missing_docs, rust_2018_idioms)]

pub mod config;
pub mod events;
pub mod handler;
pub mod health;
pub mod modules;
pub mod permissions;
pub mod plugins;
pub mod registry;
pub mod secrets;

pub use events::{Event, EventBus, EventId, EventSubscriber};
pub use handler::CoreHandler;
pub use health::{HealthMonitor, HealthStatus};
pub use modules::{Module, ModuleId, ModuleManager, ModuleState};
pub use permissions::{Permission, PermissionEngine, Principal};
pub use plugins::{Plugin, PluginId, PluginManager, PluginManifest, PluginState};
pub use registry::{ServiceInstance, ServiceRegistry, ServiceName};
pub use secrets::{SecretId, SecretManager, SecretVersion};

/// The Core itself — the top-level orchestrator that owns all subsystems.
pub struct NexoraCore {
    /// Module manager.
    pub modules: modules::ModuleManager,
    /// Service registry.
    pub registry: registry::ServiceRegistry,
    /// Event bus — shared with module/plugin managers so lifecycle changes
    /// are recorded as events automatically.
    pub events: std::sync::Arc<events::EventBus>,
    /// Permission engine — shared with services (e.g. Auth) that need to
    /// register principals.
    pub permissions: std::sync::Arc<permissions::PermissionEngine>,
    /// Plugin manager.
    pub plugins: plugins::PluginManager,
    /// Configuration manager.
    pub config: config::ConfigManager,
    /// Secret manager.
    pub secrets: secrets::SecretManager,
    /// Health monitor.
    pub health: health::HealthMonitor,
}

impl NexoraCore {
    /// Returns a clone of the inner `Arc<PermissionEngine>`. Convenience
    /// method for services that need their own strong reference.
    pub fn permissions_inner(&self) -> std::sync::Arc<permissions::PermissionEngine> {
        self.permissions.clone()
    }

    /// Returns a clone of the inner `Arc<EventBus>`.
    pub fn events_inner(&self) -> std::sync::Arc<events::EventBus> {
        self.events.clone()
    }
}

impl std::fmt::Debug for NexoraCore {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("NexoraCore")
            .field("modules", &self.modules.module_count())
            .field("services", &self.registry.service_count())
            .field("events_published", &self.events.published_count())
            .field("principals", &self.permissions.principal_count())
            .field("plugins", &self.plugins.plugin_count())
            .field("secrets", &self.secrets.secret_count())
            .finish()
    }
}

impl NexoraCore {
    /// Construct a fresh Core with all subsystems empty and wired together.
    /// The Module and Plugin managers share the EventBus so lifecycle changes
    /// are recorded as events automatically.
    pub fn new() -> Self {
        let event_bus = std::sync::Arc::new(events::EventBus::new());
        let permissions = std::sync::Arc::new(permissions::PermissionEngine::new());
        Self {
            modules: modules::ModuleManager::new().with_event_bus(event_bus.clone()),
            registry: registry::ServiceRegistry::new(),
            events: event_bus.clone(),
            permissions: permissions.clone(),
            plugins: plugins::PluginManager::new().with_event_bus(event_bus),
            config: config::ConfigManager::new(),
            secrets: secrets::SecretManager::new(),
            health: health::HealthMonitor::new(),
        }
    }

    /// Returns `true` if all subsystems report healthy.
    pub fn is_healthy(&self) -> bool {
        self.health.status() == health::HealthStatus::Healthy
    }
}

impl Default for NexoraCore {
    fn default() -> Self {
        Self::new()
    }
}
