//! Nexora Multi-Tenancy — organizations, teams, memberships.
//!
//! See Nexora Engineering Specification, Part 2 Law 23 (MULTI TENANCY):
//! "The platform must support: Individuals, Teams, Organizations, Enterprises,
//! Managed service providers."

#![forbid(unsafe_code)]
#![warn(missing_docs, rust_2018_idioms)]

pub mod handler;
pub mod manager;
pub mod types;

pub use handler::TenancyHandler;
pub use manager::{TenancyError, TenantManager};
pub use types::{Membership, Organization, OrganizationId, OrgRole, Team, TeamId};

use nexora_core::NexoraCore;
use std::sync::Arc;

/// The Tenancy service.
pub struct TenancyService {
    /// The tenant manager.
    pub manager: TenantManager,
    /// Reference to the Core.
    pub core: Arc<NexoraCore>,
}

impl std::fmt::Debug for TenancyService {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TenancyService")
            .field("orgs", &self.manager.org_count())
            .field("teams", &self.manager.team_count())
            .finish()
    }
}

impl TenancyService {
    /// Construct a new Tenancy service.
    pub fn new(core: Arc<NexoraCore>) -> Self {
        let manager = TenantManager::new().with_event_bus(core.events_inner());
        Self { manager, core }
    }

    /// Returns a handler.
    pub fn handler(self: Arc<Self>) -> TenancyHandler {
        TenancyHandler::new(self)
    }
}
