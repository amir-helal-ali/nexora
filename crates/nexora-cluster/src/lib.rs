//! مدير عنقود Nexora — تنسيق متعدد العقد.
//!
//! انظر مواصفة Nexora الهندسية، الجزء 4 (مدير العنقود) و
//! الجزء 14 (النشر العالمي ومعماريات شبكة الطرف).
//!
//! يتعقّب مدير العنقود كل العقد في عنقود Nexora:
//! - تسجيل العقد + نبض القلب
//! - اكتشاف العقد (حسب المنطقة، القدرة، الصحة)
//! - كشف الفشل التلقائي (نبضات ضائعة ← تُعلّم كغير صحية)
//! - موازنة الحمل (اختيار عقد مرجّح بالأولوية)
//! - تجميع صحة العنقود

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
