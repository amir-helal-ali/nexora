//! Workflow engine — executes workflows when triggered by events.

use crate::types::{Workflow, WorkflowAction, WorkflowId, WorkflowStep, WorkflowTrigger};
use nexora_core::events::EventPayload;
use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fmt;
use std::sync::Arc;
use time::OffsetDateTime;

/// Status of a workflow execution.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ExecutionStatus {
    /// Execution started.
    Running,
    /// All steps completed successfully.
    Succeeded,
    /// A step failed.
    Failed,
    /// A condition step stopped execution.
    Stopped,
}

impl fmt::Display for ExecutionStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Running => f.write_str("running"),
            Self::Succeeded => f.write_str("succeeded"),
            Self::Failed => f.write_str("failed"),
            Self::Stopped => f.write_str("stopped"),
        }
    }
}

/// A single step execution result.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct StepResult {
    /// Step name.
    pub step_name: String,
    /// Action that was performed.
    pub action: String,
    /// Whether the step succeeded.
    pub success: bool,
    /// Optional message.
    pub message: Option<String>,
}

/// A full workflow execution record.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct WorkflowExecution {
    /// Unique execution ID (UUID).
    pub id: String,
    /// Workflow ID that was executed.
    pub workflow_id: WorkflowId,
    /// Event that triggered this execution (if any).
    pub trigger_event: Option<String>,
    /// Payload from the triggering event.
    pub trigger_payload: Option<String>,
    /// Execution status.
    pub status: ExecutionStatus,
    /// Per-step results.
    pub step_results: Vec<StepResult>,
    /// When execution started (unix nanos).
    pub started_at: i64,
    /// When execution finished (unix nanos).
    pub finished_at: Option<i64>,
    /// Error message (if failed).
    pub error: Option<String>,
}

/// Error from workflow operations.
#[derive(Debug, thiserror::Error)]
pub enum WorkflowError {
    /// Workflow not found.
    #[error("workflow not found: {0}")]
    NotFound(WorkflowId),
    /// Workflow is disabled.
    #[error("workflow disabled: {0}")]
    Disabled(WorkflowId),
    /// Duplicate workflow ID.
    #[error("workflow already exists: {0}")]
    AlreadyExists(WorkflowId),
}

/// The workflow engine. Thread-safe.
pub struct WorkflowEngine {
    workflows: RwLock<HashMap<WorkflowId, Workflow>>,
    executions: RwLock<Vec<WorkflowExecution>>,
    event_bus: Option<Arc<nexora_core::EventBus>>,
}

impl fmt::Debug for WorkflowEngine {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let wf_count = self.workflows.read().len();
        let ex_count = self.executions.read().len();
        f.debug_struct("WorkflowEngine")
            .field("workflows", &wf_count)
            .field("executions", &ex_count)
            .finish()
    }
}

impl Default for WorkflowEngine {
    fn default() -> Self {
        Self::new()
    }
}

impl WorkflowEngine {
    /// Construct an empty engine.
    pub fn new() -> Self {
        Self {
            workflows: RwLock::new(HashMap::new()),
            executions: RwLock::new(Vec::new()),
            event_bus: None,
        }
    }

    /// Attach an EventBus for publishing events from workflow actions.
    pub fn with_event_bus(mut self, bus: Arc<nexora_core::EventBus>) -> Self {
        self.event_bus = Some(bus);
        self
    }

    /// Number of registered workflows.
    pub fn workflow_count(&self) -> usize {
        self.workflows.read().len()
    }

    /// Number of total executions.
    pub fn execution_count(&self) -> usize {
        self.executions.read().len()
    }

    /// Register a workflow.
    pub fn register(&self, mut workflow: Workflow) -> Result<Workflow, WorkflowError> {
        let mut wfs = self.workflows.write();
        if wfs.contains_key(&workflow.id) {
            return Err(WorkflowError::AlreadyExists(workflow.id.clone()));
        }
        workflow.created_at = OffsetDateTime::now_utc().unix_timestamp_nanos() as i64;
        wfs.insert(workflow.id.clone(), workflow.clone());
        drop(wfs);
        if let Some(bus) = &self.event_bus {
            bus.publish("workflow.registered", workflow.id.clone());
        }
        Ok(workflow)
    }

    /// Unregister a workflow.
    pub fn unregister(&self, id: &str) -> Result<(), WorkflowError> {
        self.workflows
            .write()
            .remove(id)
            .ok_or_else(|| WorkflowError::NotFound(id.to_string()))?;
        if let Some(bus) = &self.event_bus {
            bus.publish("workflow.unregistered", id.to_string());
        }
        Ok(())
    }

    /// Enable a workflow.
    pub fn enable(&self, id: &str) -> Result<(), WorkflowError> {
        let mut wfs = self.workflows.write();
        let wf = wfs.get_mut(id).ok_or_else(|| WorkflowError::NotFound(id.to_string()))?;
        wf.enabled = true;
        Ok(())
    }

    /// Disable a workflow.
    pub fn disable(&self, id: &str) -> Result<(), WorkflowError> {
        let mut wfs = self.workflows.write();
        let wf = wfs.get_mut(id).ok_or_else(|| WorkflowError::NotFound(id.to_string()))?;
        wf.enabled = false;
        Ok(())
    }

    /// Get a workflow by ID.
    pub fn get(&self, id: &str) -> Option<Workflow> {
        self.workflows.read().get(id).cloned()
    }

    /// List all workflows.
    pub fn list(&self) -> Vec<Workflow> {
        self.workflows.read().values().cloned().collect()
    }

    /// List all executions.
    pub fn list_executions(&self) -> Vec<WorkflowExecution> {
        self.executions.read().clone()
    }

    /// Trigger workflows matching the given event name. Returns the executions.
    pub fn trigger_event(&self, event_name: &str, payload: &str) -> Vec<WorkflowExecution> {
        let matching: Vec<Workflow> = {
            let wfs = self.workflows.read();
            wfs.values()
                .filter(|wf| wf.matches_event(event_name))
                .cloned()
                .collect()
        };

        let mut results = Vec::new();
        for wf in matching {
            if let Ok(exec) = self.execute(&wf.id, Some(event_name), Some(payload)) {
                results.push(exec);
            }
        }
        results
    }

    /// Manually trigger a workflow by ID.
    pub fn trigger_manual(&self, id: &str, payload: Option<&str>) -> Result<WorkflowExecution, WorkflowError> {
        self.execute(id, None, payload)
    }

    fn execute(
        &self,
        workflow_id: &str,
        trigger_event: Option<&str>,
        trigger_payload: Option<&str>,
    ) -> Result<WorkflowExecution, WorkflowError> {
        // Get the workflow (clone to avoid holding lock during execution).
        let wf = {
            let wfs = self.workflows.read();
            wfs.get(workflow_id)
                .cloned()
                .ok_or_else(|| WorkflowError::NotFound(workflow_id.to_string()))?
        };

        if !wf.enabled {
            return Err(WorkflowError::Disabled(workflow_id.to_string()));
        }

        let mut exec = WorkflowExecution {
            id: uuid::Uuid::new_v4().to_string(),
            workflow_id: workflow_id.to_string(),
            trigger_event: trigger_event.map(|s| s.to_string()),
            trigger_payload: trigger_payload.map(|s| s.to_string()),
            status: ExecutionStatus::Running,
            step_results: vec![],
            started_at: OffsetDateTime::now_utc().unix_timestamp_nanos() as i64,
            finished_at: None,
            error: None,
        };

        // Execute each step sequentially.
        for step in &wf.steps {
            let result = self.execute_step(step, trigger_payload.unwrap_or(""));
            exec.step_results.push(result.clone());

            // If step failed, stop.
            if !result.success {
                exec.status = ExecutionStatus::Failed;
                exec.error = result.message.clone();
                break;
            }

            // If it was a condition that returned false, stop.
            if let WorkflowAction::Condition { .. } = &step.action {
                if result.message.as_deref() == Some("condition false") {
                    exec.status = ExecutionStatus::Stopped;
                    break;
                }
            }
        }

        if exec.status == ExecutionStatus::Running {
            exec.status = ExecutionStatus::Succeeded;
        }
        exec.finished_at = Some(OffsetDateTime::now_utc().unix_timestamp_nanos() as i64);

        // Increment execution count on the workflow.
        {
            let mut wfs = self.workflows.write();
            if let Some(w) = wfs.get_mut(workflow_id) {
                w.execution_count += 1;
            }
        }

        // Store execution.
        self.executions.write().push(exec.clone());

        // Emit event.
        if let Some(bus) = &self.event_bus {
            bus.publish(
                "workflow.executed",
                format!("{}:{}", workflow_id, exec.status),
            );
        }

        Ok(exec)
    }

    fn execute_step(&self, step: &WorkflowStep, trigger_payload: &str) -> StepResult {
        match &step.action {
            WorkflowAction::PublishEvent { name, payload } => {
                let resolved = payload.replace("{{trigger}}", trigger_payload);
                if let Some(bus) = &self.event_bus {
                    bus.publish(name, resolved.clone());
                }
                StepResult {
                    step_name: step.name.clone(),
                    action: format!("publish:{}", name),
                    success: true,
                    message: Some(format!("published {} with payload: {}", name, resolved)),
                }
            }
            WorkflowAction::Log { message } => {
                let resolved = message.replace("{{trigger}}", trigger_payload);
                tracing::info!("[workflow] {}", resolved);
                StepResult {
                    step_name: step.name.clone(),
                    action: "log".into(),
                    success: true,
                    message: Some(resolved),
                }
            }
            WorkflowAction::Wait { seconds } => {
                // In v0.1, we don't actually block (async execution would require
                // a more complex runtime). We just record the intended wait.
                StepResult {
                    step_name: step.name.clone(),
                    action: format!("wait:{}s", seconds),
                    success: true,
                    message: Some(format!("would wait {}s (skipped in sync mode)", seconds)),
                }
            }
            WorkflowAction::Condition { condition } => {
                let resolved = condition.replace("{{trigger}}", trigger_payload);
                let passed = resolved == "true" || resolved == "1";
                StepResult {
                    step_name: step.name.clone(),
                    action: format!("if:{}", resolved),
                    success: true,
                    message: Some(if passed {
                        "condition true".into()
                    } else {
                        "condition false".into()
                    }),
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn setup() -> WorkflowEngine {
        let bus = Arc::new(nexora_core::EventBus::new());
        WorkflowEngine::new().with_event_bus(bus)
    }

    fn sample_workflow() -> Workflow {
        Workflow::new("wf1", "Test Workflow", WorkflowTrigger::Event { event_name: "user.".into() })
            .add_step(WorkflowStep {
                name: "log".into(),
                action: WorkflowAction::Log { message: "User event: {{trigger}}".into() },
            })
            .add_step(WorkflowStep {
                name: "publish".into(),
                action: WorkflowAction::PublishEvent {
                    name: "workflow.completed".into(),
                    payload: "done".into(),
                },
            })
    }

    #[test]
    fn register_and_list() {
        let engine = setup();
        engine.register(sample_workflow()).unwrap();
        assert_eq!(engine.workflow_count(), 1);
        assert!(engine.get("wf1").is_some());
        assert_eq!(engine.list().len(), 1);
    }

    #[test]
    fn duplicate_register_rejected() {
        let engine = setup();
        engine.register(sample_workflow()).unwrap();
        let err = engine.register(sample_workflow()).unwrap_err();
        assert!(matches!(err, WorkflowError::AlreadyExists(_)));
    }

    #[test]
    fn trigger_event_executes_matching_workflows() {
        let engine = setup();
        engine.register(sample_workflow()).unwrap();

        let execs = engine.trigger_event("user.created", "alice");
        assert_eq!(execs.len(), 1);
        assert_eq!(execs[0].status, ExecutionStatus::Succeeded);
        assert_eq!(execs[0].step_results.len(), 2);
    }

    #[test]
    fn trigger_event_does_not_match_non_matching() {
        let engine = setup();
        engine.register(sample_workflow()).unwrap();

        let execs = engine.trigger_event("module.installed", "auth");
        assert_eq!(execs.len(), 0);
    }

    #[test]
    fn manual_trigger_works() {
        let engine = setup();
        let mut wf = Workflow::new("wf2", "Manual", WorkflowTrigger::Manual);
        wf.steps.push(WorkflowStep {
            name: "log".into(),
            action: WorkflowAction::Log { message: "manual run".into() },
        });
        engine.register(wf).unwrap();

        let exec = engine.trigger_manual("wf2", Some("test")).unwrap();
        assert_eq!(exec.status, ExecutionStatus::Succeeded);
        assert!(exec.trigger_event.is_none());
    }

    #[test]
    fn disabled_workflow_not_triggered() {
        let engine = setup();
        engine.register(sample_workflow()).unwrap();
        engine.disable("wf1").unwrap();

        let execs = engine.trigger_event("user.created", "alice");
        assert_eq!(execs.len(), 0); // disabled workflows don't match
    }

    #[test]
    fn enable_disable_cycle() {
        let engine = setup();
        engine.register(sample_workflow()).unwrap();
        engine.disable("wf1").unwrap();
        assert!(!engine.get("wf1").unwrap().enabled);
        engine.enable("wf1").unwrap();
        assert!(engine.get("wf1").unwrap().enabled);
    }

    #[test]
    fn condition_stops_execution() {
        let engine = setup();
        let wf = Workflow::new("wf3", "Conditional", WorkflowTrigger::Manual)
            .add_step(WorkflowStep {
                name: "check".into(),
                action: WorkflowAction::Condition { condition: "false".into() },
            })
            .add_step(WorkflowStep {
                name: "should_not_run".into(),
                action: WorkflowAction::Log { message: "should not see this".into() },
            });
        engine.register(wf).unwrap();

        let exec = engine.trigger_manual("wf3", None).unwrap();
        assert_eq!(exec.status, ExecutionStatus::Stopped);
        assert_eq!(exec.step_results.len(), 1); // only the condition step ran
    }

    #[test]
    fn condition_with_trigger_substitution() {
        let engine = setup();
        let wf = Workflow::new("wf4", "Conditional", WorkflowTrigger::Manual)
            .add_step(WorkflowStep {
                name: "check".into(),
                action: WorkflowAction::Condition { condition: "{{trigger}}".into() },
            })
            .add_step(WorkflowStep {
                name: "after".into(),
                action: WorkflowAction::Log { message: "passed".into() },
            });
        engine.register(wf).unwrap();

        // Trigger with "true" → condition passes.
        let exec = engine.trigger_manual("wf4", Some("true")).unwrap();
        assert_eq!(exec.status, ExecutionStatus::Succeeded);
        assert_eq!(exec.step_results.len(), 2);

        // Trigger with "false" → condition fails.
        let exec = engine.trigger_manual("wf4", Some("false")).unwrap();
        assert_eq!(exec.status, ExecutionStatus::Stopped);
        assert_eq!(exec.step_results.len(), 1);
    }

    #[test]
    fn publish_event_action_publishes() {
        let bus = Arc::new(nexora_core::EventBus::new());
        let engine = WorkflowEngine::new().with_event_bus(bus.clone());
        let wf = Workflow::new("wf5", "Publisher", WorkflowTrigger::Manual)
            .add_step(WorkflowStep {
                name: "pub".into(),
                action: WorkflowAction::PublishEvent {
                    name: "test.from_workflow".into(),
                    payload: "hello".into(),
                },
            });
        engine.register(wf).unwrap();
        engine.trigger_manual("wf5", None).unwrap();

        let events = bus.replay_filtered(0, "test.from_workflow");
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].name, "test.from_workflow");
    }

    #[test]
    fn trigger_payload_substitution() {
        let bus = Arc::new(nexora_core::EventBus::new());
        let engine = WorkflowEngine::new().with_event_bus(bus.clone());
        let wf = Workflow::new("wf6", "Sub", WorkflowTrigger::Event { event_name: "user.".into() })
            .add_step(WorkflowStep {
                name: "pub".into(),
                action: WorkflowAction::PublishEvent {
                    name: "workflow.echo".into(),
                    payload: "got: {{trigger}}".into(),
                },
            });
        engine.register(wf).unwrap();
        engine.trigger_event("user.created", "alice");

        let events = bus.replay_filtered(0, "workflow.echo");
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].payload, EventPayload::Text("got: alice".into()));
    }

    #[test]
    fn unregister_works() {
        let engine = setup();
        engine.register(sample_workflow()).unwrap();
        engine.unregister("wf1").unwrap();
        assert_eq!(engine.workflow_count(), 0);
    }

    #[test]
    fn execution_count_increments() {
        let engine = setup();
        engine.register(sample_workflow()).unwrap();
        engine.trigger_event("user.created", "a");
        engine.trigger_event("user.created", "b");
        assert_eq!(engine.execution_count(), 2);
        assert_eq!(engine.get("wf1").unwrap().execution_count, 2);
    }
}
