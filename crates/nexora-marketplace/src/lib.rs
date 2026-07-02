//! متجر Nexora — طبقة اقتصاد برمجي كاملة.
//!
//! انظر مواصفة Nexora الهندسية، الجزء 5. يسمح المتجر للمطورين ببناء،
//! نشر، توزيع، تحقيق دخل، وإدارة الحزم (وحدات، مكونات، وكلاء AI،
//! قوالب، خدمات، أتمتة).
//!
//! # الأنظمة الفرعية
//!
//! - [`version`]: تحليل SemVer + مقارنة + نطاقات النسخ
//! - [`package`]: نموذج الحزمة، الأنواع، البيان
//! - [`signature`]: التحقق من توقيع Ed25519 + نزاهة SHA-256
//! - [`dependency`]: رسم بياني للتبعيات مع تحقق غير دوري
//! - [`store`]: مخزن الحزم (CRUD + بحث + تقييم)
//! - [`install`]: خط أنابيب تثبيت 13 خطوة
//! - [`handler`]: معالج NXP للمتجر

#![forbid(unsafe_code)]
#![warn(missing_docs, rust_2018_idioms)]

pub mod dependency;
pub mod handler;
pub mod install;
pub mod package;
pub mod signature;
pub mod store;
pub mod update;
pub mod version;

pub use dependency::{Dependency, DependencyError, DependencyGraph};
pub use handler::MarketplaceHandler;
pub use install::{InstallError, InstallPipeline, InstallReport};
pub use package::{Package, PackageBilling, PackageId, PackageManifest, PackageType, Visibility};
pub use signature::{verify_package_signature, PackageSignatureError};
pub use store::{PackageStore, PackageStoreError, TrustScore};
pub use update::{AvailableUpdate, RollbackResult, UpdateCheckResult, UpdateError, UpdateManager, UpdatePolicy};
pub use version::{ParseVersionError, Version, VersionRange};

use nexora_core::NexoraCore;
use std::sync::Arc;

/// The Marketplace service. Owns the package store + install pipeline + update manager.
pub struct MarketplaceService {
    /// Package store.
    pub store: PackageStore,
    /// Install pipeline.
    pub pipeline: InstallPipeline,
    /// Auto-update manager.
    pub updates: UpdateManager,
    /// Reference to the Core (for events + permissions + modules).
    pub core: Arc<NexoraCore>,
}

impl std::fmt::Debug for MarketplaceService {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("MarketplaceService")
            .field("packages", &self.store.package_count())
            .field("installed", &self.store.installed_count())
            .field("core", &self.core)
            .finish()
    }
}

impl MarketplaceService {
    /// Construct a new Marketplace service.
    pub fn new(core: Arc<NexoraCore>) -> Self {
        let bus = core.events_inner();
        let store = PackageStore::new().with_event_bus(bus.clone());
        let pipeline = InstallPipeline::new();
        let updates = UpdateManager::new().with_event_bus(bus);
        Self {
            store,
            pipeline,
            updates,
            core,
        }
    }

    /// Returns a handler for dispatching NXP marketplace opcodes.
    pub fn handler(self: Arc<Self>) -> MarketplaceHandler {
        MarketplaceHandler::new(self)
    }
}
