//! Nexora Cluster Manager — multi-node coordination.
//!
//! See Nexora Engineering Specification, Part 4 (CLUSTER MANAGER) and
//! Part 14 (GLOBAL DEPLOYMENT & EDGE NETWORK ARCHITECTURE).
//!
//! The Cluster Manager tracks all nodes in a Nexora cluster:
//! - Node registration + heartbeat
//! - Node discovery (by region, capability, health)
//! - Automatic failover detection (missed heartbeats → marked unhealthy)
//! - Load balancing (priority-weighted node selection)
//! - Cluster-wide health aggregation

#![forbid(unsafe_code)]
#![warn(missing_docs, rust_2018_idioms)]

pub mod handler;
pub mod manager;
pub mod types;

pub use handler::ClusterHandler;
pub use manager::{ClusterManager, ClusterError};
pub use types::{ClusterNode, ClusterStats, NodeId, NodeRegion, NodeRole, NodeStatus};

use nexora_core::NexoraCore;
use std::sync::Arc;

/// The Cluster service.
pub struct ClusterService {
    /// The cluster manager.
    pub manager: ClusterManager,
    /// Reference to the Core.
    pub core: Arc<NexoraCore>,
}

impl std::fmt::Debug for ClusterService {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ClusterService")
            .field("nodes", &self.manager.node_count())
            .field("healthy", &self.manager.healthy_count())
            .finish()
    }
}

impl ClusterService {
    /// Construct a new Cluster service.
    pub fn new(core: Arc<NexoraCore>) -> Self {
        let manager = ClusterManager::new().with_event_bus(core.events_inner());
        Self { manager, core }
    }

    /// Returns a handler for dispatching cluster commands.
    pub fn handler(self: Arc<Self>) -> ClusterHandler {
        ClusterHandler::new(self)
    }
}
