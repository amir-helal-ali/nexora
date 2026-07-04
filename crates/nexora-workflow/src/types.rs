//! Workflow types — definitions, triggers, steps, actions.

use serde::{Deserialize, Serialize};
use std::fmt;
use time::OffsetDateTime;

/// Unique workflow ID.
pub type WorkflowId = String;

/// What triggers a workflow to start.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum WorkflowTrigger {
    /// Triggered when an event with this name prefix is published.
    Event {
        /// Event name prefix to match (e.g. "user.created").
        event_name: String,
    },
    /// Triggered manually (via API or command palette).
    Manual,
    /// Triggered on a schedule (cron-like, seconds interval for v0.1).
    Schedule {
        /// Interval in seconds.
        interval_seconds: u64,
    },
}

impl fmt::Display for WorkflowTrigger {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Event { event_name } => write!(f, "event:{}", event_name),
            Self::Manual => f.write_str("manual"),
            Self::Schedule { interval_seconds } => write!(f, "every:{}s", interval_seconds),
        }
    }
}

/// An action performed by a workflow step.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum WorkflowAction {
    /// Publish an event.
    PublishEvent {
        /// Event name to publish.
        name: String,
        /// Payload (string). Can reference trigger payload with "{{trigger}}".
        payload: String,
    },
    /// Log a message (for debugging).
    Log {
        /// Message to log.
        message: String,
    },
    /// Wait for a duration before continuing.
    Wait {
        /// Duration in seconds.
        seconds: u64,
    },
    /// Conditionally skip remaining steps if a condition is met.
    Condition {
        /// If this string equals "true", continue; otherwise stop.
        condition: String,
    },
}

impl fmt::Display for WorkflowAction {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::PublishEvent { name, .. } => write!(f, "publish:{}", name),
            Self::Log { message } => write!(f, "log:{}", &message[..message.len().min(30)]),
            Self::Wait { seconds } => write!(f, "wait:{}s", seconds),
            Self::Condition { condition } => write!(f, "if:{}", condition),
        }
    }
}

/// A single step in a workflow.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct WorkflowStep {
    /// Step name (for display).
    pub name: String,
    /// The action to perform.
    pub action: WorkflowAction,
}

/// A workflow definition.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Workflow {
    /// Unique workflow ID.
    pub id: WorkflowId,
    /// Human-readable name.
    pub name: String,
    /// Description.
    pub description: String,
    /// What triggers this workflow.
    pub trigger: WorkflowTrigger,
    /// Ordered list of steps to execute.
    pub steps: Vec<WorkflowStep>,
    /// Whether the workflow is enabled.
    pub enabled: bool,
    /// When the workflow was created (unix nanos).
    pub created_at: i64,
    /// Number of times this workflow has executed.
    pub execution_count: u64,
}

impl Workflow {
    /// Construct a new workflow.
    pub fn new(id: impl Into<String>, name: impl Into<String>, trigger: WorkflowTrigger) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
            description: String::new(),
            trigger,
            steps: vec![],
            enabled: true,
            created_at: OffsetDateTime::now_utc().unix_timestamp_nanos() as i64,
            execution_count: 0,
        }
    }

    /// Add a step.
    pub fn add_step(mut self, step: WorkflowStep) -> Self {
        self.steps.push(step);
        self
    }

    /// Check if this workflow should trigger for the given event name.
    pub fn matches_event(&self, event_name: &str) -> bool {
        match &self.trigger {
            WorkflowTrigger::Event { event_name: prefix } => {
                self.enabled && event_name.starts_with(prefix)
            }
            _ => false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn workflow_construction() {
        let wf = Workflow::new("wf1", "Test", WorkflowTrigger::Manual)
            .add_step(WorkflowStep {
                name: "step1".into(),
                action: WorkflowAction::Log { message: "hello".into() },
            });
        assert_eq!(wf.id, "wf1");
        assert_eq!(wf.steps.len(), 1);
        assert!(wf.enabled);
    }

    #[test]
    fn event_trigger_matching() {
        let wf = Workflow::new(
            "wf1",
            "Test",
            WorkflowTrigger::Event { event_name: "user.".into() },
        );
        assert!(wf.matches_event("user.created"));
        assert!(wf.matches_event("user.logged_in"));
        assert!(!wf.matches_event("module.installed"));
    }

    #[test]
    fn disabled_workflow_does_not_match() {
        let mut wf = Workflow::new(
            "wf1",
            "Test",
            WorkflowTrigger::Event { event_name: "user.".into() },
        );
        wf.enabled = false;
        assert!(!wf.matches_event("user.created"));
    }

    #[test]
    fn trigger_display() {
        assert_eq!(
            WorkflowTrigger::Event { event_name: "user.created".into() }.to_string(),
            "event:user.created"
        );
        assert_eq!(WorkflowTrigger::Manual.to_string(), "manual");
        assert_eq!(
            WorkflowTrigger::Schedule { interval_seconds: 60 }.to_string(),
            "every:60s"
        );
    }
}
