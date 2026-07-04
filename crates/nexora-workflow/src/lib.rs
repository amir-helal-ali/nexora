//! محرك سير عمل Nexora — خطوط أتمتة يحركها الأحداث.
//!
//! انظر مواصفة Nexora الهندسية، الجزء 4 (محرك سير العمل).
//! سير العمل هو تسلسل خطوات تُنفَّذ تلقائياً عند تشغيلها بأحداث.
//! كل خطوة يمكنها نشر أحداث، انتظار شروط، أو استدعاء إجراءات خارجية.
//!
//! # مثال
//!
//! ```text
//! سير عمل: "ترحيب بمستخدم جديد"
//!   المُشغِّل: user.created
//!   الخطوات:
//!     1. انشر "notification.welcome" بحمولة من المُشغِّل
//!     2. انشر "billing.create_trial_invoice"
//!     3. انشر "analytics.track" مع event="signup"
//! ```

#![forbid(unsafe_code)]
#![warn(missing_docs, rust_2018_idioms)]

pub mod engine;
pub mod handler;
pub mod types;

pub use engine::{WorkflowEngine, WorkflowExecution, ExecutionStatus};
pub use handler::WorkflowHandler;
pub use types::{Workflow, WorkflowAction, WorkflowId, WorkflowStep, WorkflowTrigger};

use nexora_core::NexoraCore;
use std::sync::Arc;

/// The Workflow service.
pub struct WorkflowService {
    /// The workflow engine.
    pub engine: WorkflowEngine,
    /// Reference to the Core.
    pub core: Arc<NexoraCore>,
}

impl std::fmt::Debug for WorkflowService {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("WorkflowService")
            .field("workflows", &self.engine.workflow_count())
            .field("executions", &self.engine.execution_count())
            .finish()
    }
}

impl WorkflowService {
    /// Construct a new Workflow service.
    pub fn new(core: Arc<NexoraCore>) -> Self {
        let engine = WorkflowEngine::new().with_event_bus(core.events_inner());
        Self { engine, core }
    }

    /// Returns a handler for dispatching workflow commands.
    pub fn handler(self: Arc<Self>) -> WorkflowHandler {
        WorkflowHandler::new(self)
    }
}
