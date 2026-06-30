//! Nexora Marketplace — full software economy layer.
//!
//! See Nexora Engineering Specification, Part 5. The Marketplace allows
//! developers to build, publish, distribute, monetize, and manage packages
//! (Modules, Plugins, AI Agents, Templates, Services, Automations).
//!
//! # Subsystems
//!
//! - [`version`]: SemVer parsing + comparison + version ranges
//! - [`package`]: Package model, types, manifest
//! - [`signature`]: Ed25519 signature verification + SHA-256 integrity
//! - [`dependency`]: Dependency graph with acyclic validation
//! - [`store`]: PackageStore (CRUD + search + rating)
//! - [`install`]: 13-step installation pipeline
//! - [`handler`]: Marketplace NXP handler

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
