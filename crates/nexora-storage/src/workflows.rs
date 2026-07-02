//! SQLite-backed workflow store — persists workflows + executions.
//!
//! Wraps the in-memory WorkflowEngine's stores and writes through to SQLite
//! on every mutation. On startup, call `load_into()` to restore state.

use crate::{Database, StorageError};
use nexora_workflow::engine::{ExecutionStatus, WorkflowExecution};
use nexora_workflow::types::{Workflow, WorkflowStep, WorkflowTrigger};
use std::sync::Arc;

/// SQLite-backed workflow store.
pub struct SqliteWorkflowStore {
    db: Database,
    event_bus: Option<Arc<nexora_core::EventBus>>,
}

impl std::fmt::Debug for SqliteWorkflowStore {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SqliteWorkflowStore")
            .field("db", &self.db)
            .finish()
    }
}

impl SqliteWorkflowStore {
    /// Construct a new store.
    pub fn new(db: Database) -> Self {
        Self { db, event_bus: None }
    }

    /// Attach an EventBus.
    pub fn with_event_bus(mut self, bus: Arc<nexora_core::EventBus>) -> Self {
        self.event_bus = Some(bus);
        self
    }

    /// Save a workflow (insert or replace).
    pub fn save_workflow(&self, wf: &Workflow) -> Result<(), StorageError> {
        let trigger_json = serde_json::to_string(&wf.trigger)?;
        let steps_json = serde_json::to_string(&wf.steps)?;
        self.db.with_conn(|conn| {
            conn.execute(
                "INSERT OR REPLACE INTO workflows
                 (id, name, description, trigger_json, steps_json, enabled, created_at, execution_count)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
                rusqlite::params![
                    wf.id,
                    wf.name,
                    wf.description,
                    trigger_json,
                    steps_json,
                    if wf.enabled { 1 } else { 0 },
                    wf.created_at,
                    wf.execution_count,
                ],
            )?;
            Ok(())
        })
    }

    /// Update execution count.
    pub fn update_execution_count(&self, id: &str, count: u64) -> Result<(), StorageError> {
        self.db.with_conn(|conn| {
            conn.execute(
                "UPDATE workflows SET execution_count = ?1 WHERE id = ?2",
                rusqlite::params![count, id],
            )?;
            Ok(())
        })
    }

    /// Update enabled status.
    pub fn update_enabled(&self, id: &str, enabled: bool) -> Result<(), StorageError> {
        self.db.with_conn(|conn| {
            conn.execute(
                "UPDATE workflows SET enabled = ?1 WHERE id = ?2",
                rusqlite::params![if enabled { 1 } else { 0 }, id],
            )?;
            Ok(())
        })
    }

    /// Delete a workflow.
    pub fn delete_workflow(&self, id: &str) -> Result<(), StorageError> {
        self.db.with_conn(|conn| {
            conn.execute("DELETE FROM workflows WHERE id = ?1", rusqlite::params![id])?;
            conn.execute("DELETE FROM workflow_executions WHERE workflow_id = ?1", rusqlite::params![id])?;
            Ok(())
        })
    }

    /// Save an execution.
    pub fn save_execution(&self, exec: &WorkflowExecution) -> Result<(), StorageError> {
        let step_results_json = serde_json::to_string(&exec.step_results)?;
        self.db.with_conn(|conn| {
            conn.execute(
                "INSERT OR REPLACE INTO workflow_executions
                 (id, workflow_id, trigger_event, trigger_payload, status,
                  step_results_json, started_at, finished_at, error)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
                rusqlite::params![
                    exec.id,
                    exec.workflow_id,
                    exec.trigger_event,
                    exec.trigger_payload,
                    exec.status.to_string(),
                    step_results_json,
                    exec.started_at,
                    exec.finished_at,
                    exec.error,
                ],
            )?;
            Ok(())
        })
    }

    /// Count workflows.
    pub fn workflow_count(&self) -> Result<i64, StorageError> {
        self.db.with_conn(|conn| {
            Ok(conn.query_row("SELECT COUNT(*) FROM workflows", [], |row| row.get(0))?)
        })
    }

    /// Count executions.
    pub fn execution_count(&self) -> Result<i64, StorageError> {
        self.db.with_conn(|conn| {
            Ok(conn.query_row("SELECT COUNT(*) FROM workflow_executions", [], |row| row.get(0))?)
        })
    }

    /// Load all workflows from SQLite.
    pub fn load_workflows(&self) -> Result<Vec<Workflow>, StorageError> {
        self.db.with_conn(|conn| {
            let mut stmt = conn.prepare(
                "SELECT id, name, description, trigger_json, steps_json, enabled, created_at, execution_count
                 FROM workflows",
            )?;
            let rows = stmt.query_map([], |row| {
                let id: String = row.get(0)?;
                let name: String = row.get(1)?;
                let description: String = row.get(2)?;
                let trigger_json: String = row.get(3)?;
                let steps_json: String = row.get(4)?;
                let enabled: i64 = row.get(5)?;
                let created_at: i64 = row.get(6)?;
                let execution_count: i64 = row.get(7)?;
                Ok((id, name, description, trigger_json, steps_json, enabled, created_at, execution_count))
            })?;
            let mut result = Vec::new();
            for row in rows {
                let (id, name, description, trigger_json, steps_json, enabled, created_at, execution_count) = row?;
                let trigger: WorkflowTrigger = serde_json::from_str(&trigger_json)?;
                let steps: Vec<WorkflowStep> = serde_json::from_str(&steps_json)?;
                result.push(Workflow {
                    id,
                    name,
                    description,
                    trigger,
                    steps,
                    enabled: enabled != 0,
                    created_at,
                    execution_count: execution_count as u64,
                });
            }
            Ok(result)
        })
    }

    /// Load recent executions (limited count).
    pub fn load_executions(&self, limit: usize) -> Result<Vec<WorkflowExecution>, StorageError> {
        self.db.with_conn(|conn| {
            let mut stmt = conn.prepare(
                "SELECT id, workflow_id, trigger_event, trigger_payload, status,
                        step_results_json, started_at, finished_at, error
                 FROM workflow_executions
                 ORDER BY started_at DESC
                 LIMIT ?1",
            )?;
            let rows = stmt.query_map(rusqlite::params![limit as i64], |row| {
                let id: String = row.get(0)?;
                let workflow_id: String = row.get(1)?;
                let trigger_event: Option<String> = row.get(2)?;
                let trigger_payload: Option<String> = row.get(3)?;
                let status_str: String = row.get(4)?;
                let step_results_json: String = row.get(5)?;
                let started_at: i64 = row.get(6)?;
                let finished_at: Option<i64> = row.get(7)?;
                let error: Option<String> = row.get(8)?;

                let status = match status_str.as_str() {
                    "succeeded" => ExecutionStatus::Succeeded,
                    "failed" => ExecutionStatus::Failed,
                    "stopped" => ExecutionStatus::Stopped,
                    _ => ExecutionStatus::Running,
                };
                let step_results: Vec<nexora_workflow::engine::StepResult> =
                    serde_json::from_str(&step_results_json).unwrap_or_default();

                Ok(WorkflowExecution {
                    id,
                    workflow_id,
                    trigger_event,
                    trigger_payload,
                    status,
                    step_results,
                    started_at,
                    finished_at,
                    error,
                })
            })?;
            let mut result = Vec::new();
            for row in rows {
                result.push(row?);
            }
            Ok(result)
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use nexora_workflow::types::{Workflow, WorkflowAction, WorkflowStep, WorkflowTrigger};

    fn setup() -> SqliteWorkflowStore {
        let db = Database::open_in_memory().unwrap();
        SqliteWorkflowStore::new(db)
    }

    fn sample_workflow(id: &str) -> Workflow {
        Workflow::new(id, "Test Workflow", WorkflowTrigger::Manual)
            .add_step(WorkflowStep {
                name: "log".into(),
                action: WorkflowAction::Log { message: "hello".into() },
            })
    }

    #[test]
    fn save_and_count_workflows() {
        let store = setup();
        assert_eq!(store.workflow_count().unwrap(), 0);
        store.save_workflow(&sample_workflow("wf1")).unwrap();
        store.save_workflow(&sample_workflow("wf2")).unwrap();
        assert_eq!(store.workflow_count().unwrap(), 2);
    }

    #[test]
    fn load_workflows_roundtrip() {
        let store = setup();
        let wf = sample_workflow("wf1");
        store.save_workflow(&wf).unwrap();
        let loaded = store.load_workflows().unwrap();
        assert_eq!(loaded.len(), 1);
        assert_eq!(loaded[0].id, "wf1");
        assert_eq!(loaded[0].name, "Test Workflow");
        assert_eq!(loaded[0].steps.len(), 1);
        assert!(loaded[0].enabled);
    }

    #[test]
    fn delete_workflow() {
        let store = setup();
        store.save_workflow(&sample_workflow("wf1")).unwrap();
        assert_eq!(store.workflow_count().unwrap(), 1);
        store.delete_workflow("wf1").unwrap();
        assert_eq!(store.workflow_count().unwrap(), 0);
    }

    #[test]
    fn update_enabled() {
        let store = setup();
        store.save_workflow(&sample_workflow("wf1")).unwrap();
        store.update_enabled("wf1", false).unwrap();
        let loaded = store.load_workflows().unwrap();
        assert!(!loaded[0].enabled);
    }

    #[test]
    fn update_execution_count() {
        let store = setup();
        store.save_workflow(&sample_workflow("wf1")).unwrap();
        store.update_execution_count("wf1", 42).unwrap();
        let loaded = store.load_workflows().unwrap();
        assert_eq!(loaded[0].execution_count, 42);
    }

    #[test]
    fn save_and_load_execution() {
        let store = setup();
        let exec = WorkflowExecution {
            id: "exec-1".into(),
            workflow_id: "wf1".into(),
            trigger_event: Some("test.event".into()),
            trigger_payload: Some("hello".into()),
            status: ExecutionStatus::Succeeded,
            step_results: vec![],
            started_at: 1000,
            finished_at: Some(2000),
            error: None,
        };
        store.save_execution(&exec).unwrap();
        assert_eq!(store.execution_count().unwrap(), 1);

        let loaded = store.load_executions(10).unwrap();
        assert_eq!(loaded.len(), 1);
        assert_eq!(loaded[0].id, "exec-1");
        assert_eq!(loaded[0].status, ExecutionStatus::Succeeded);
        assert_eq!(loaded[0].trigger_event, Some("test.event".into()));
    }

    #[test]
    fn load_executions_respects_limit() {
        let store = setup();
        for i in 0..5 {
            let exec = WorkflowExecution {
                id: format!("exec-{}", i),
                workflow_id: "wf1".into(),
                trigger_event: None,
                trigger_payload: None,
                status: ExecutionStatus::Succeeded,
                step_results: vec![],
                started_at: i as i64,
                finished_at: None,
                error: None,
            };
            store.save_execution(&exec).unwrap();
        }
        let loaded = store.load_executions(3).unwrap();
        assert_eq!(loaded.len(), 3);
    }

    #[test]
    fn trigger_json_roundtrip() {
        let store = setup();
        let wf = Workflow::new(
            "wf1",
            "Event Trigger",
            WorkflowTrigger::Event { event_name: "user.".into() },
        );
        store.save_workflow(&wf).unwrap();
        let loaded = store.load_workflows().unwrap();
        assert_eq!(loaded[0].trigger, wf.trigger);
    }

    #[test]
    fn complex_steps_roundtrip() {
        let store = setup();
        let wf = Workflow::new("wf1", "Complex", WorkflowTrigger::Manual)
            .add_step(WorkflowStep {
                name: "log".into(),
                action: WorkflowAction::Log { message: "step 1".into() },
            })
            .add_step(WorkflowStep {
                name: "publish".into(),
                action: WorkflowAction::PublishEvent {
                    name: "test.event".into(),
                    payload: "hello".into(),
                },
            })
            .add_step(WorkflowStep {
                name: "check".into(),
                action: WorkflowAction::Condition { condition: "true".into() },
            });
        store.save_workflow(&wf).unwrap();
        let loaded = store.load_workflows().unwrap();
        assert_eq!(loaded[0].steps.len(), 3);
        assert_eq!(loaded[0].steps[1].action, wf.steps[1].action);
    }
}
