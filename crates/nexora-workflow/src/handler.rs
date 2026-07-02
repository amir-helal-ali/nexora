//! Workflow handler — dispatches workflow commands via the Gateway.

use crate::engine::{ExecutionStatus, WorkflowExecution};
use crate::types::{Workflow, WorkflowTrigger};
use crate::WorkflowService;
use nxp_core::NxpError;
use nxp_core::error::protocol_codes;
use serde_json::Value;
use std::sync::Arc;

/// The Workflow handler.
#[derive(Clone)]
pub struct WorkflowHandler {
    service: Arc<WorkflowService>,
}

impl std::fmt::Debug for WorkflowHandler {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("WorkflowHandler")
            .field("service", &self.service)
            .finish()
    }
}

impl WorkflowHandler {
    /// Construct a new handler.
    pub fn new(service: Arc<WorkflowService>) -> Self {
        Self { service }
    }

    /// Execute a workflow command.
    pub async fn execute(&self, command: &str, args: &Value) -> Result<Value, NxpError> {
        match command {
            "workflow.register" => self.cmd_register(args),
            "workflow.unregister" => self.cmd_unregister(args),
            "workflow.list" => self.cmd_list(),
            "workflow.get" => self.cmd_get(args),
            "workflow.enable" => self.cmd_enable(args),
            "workflow.disable" => self.cmd_disable(args),
            "workflow.trigger" => self.cmd_trigger(args),
            "workflow.trigger_event" => self.cmd_trigger_event(args),
            "workflow.list_executions" => self.cmd_list_executions(),
            "workflow.stats" => self.cmd_stats(),
            _ => Err(NxpError::protocol(
                protocol_codes::UNKNOWN_OPCODE,
                format!("unknown workflow command: {}", command),
            )),
        }
    }

    fn cmd_register(&self, args: &Value) -> Result<Value, NxpError> {
        let wf: Workflow = serde_json::from_value(args.clone())
            .map_err(|e| NxpError::protocol(protocol_codes::DECODE_FAILED, e.to_string()))?;
        let wf = self
            .service
            .engine
            .register(wf)
            .map_err(|e| NxpError::protocol(protocol_codes::DECODE_FAILED, e.to_string()))?;
        Ok(serde_json::json!({ "ok": true, "workflow": wf }))
    }

    fn cmd_unregister(&self, args: &Value) -> Result<Value, NxpError> {
        let id = args.get("id").and_then(|v| v.as_str())
            .ok_or_else(|| NxpError::protocol(protocol_codes::DECODE_FAILED, "missing id"))?;
        self.service.engine.unregister(id)
            .map_err(|e| NxpError::protocol(protocol_codes::DECODE_FAILED, e.to_string()))?;
        Ok(serde_json::json!({ "ok": true }))
    }

    fn cmd_list(&self) -> Result<Value, NxpError> {
        let wfs = self.service.engine.list();
        Ok(serde_json::json!({ "ok": true, "count": wfs.len(), "workflows": wfs }))
    }

    fn cmd_get(&self, args: &Value) -> Result<Value, NxpError> {
        let id = args.get("id").and_then(|v| v.as_str())
            .ok_or_else(|| NxpError::protocol(protocol_codes::DECODE_FAILED, "missing id"))?;
        let wf = self.service.engine.get(id)
            .ok_or_else(|| NxpError::protocol(protocol_codes::DECODE_FAILED, format!("workflow {} not found", id)))?;
        Ok(serde_json::json!({ "ok": true, "workflow": wf }))
    }

    fn cmd_enable(&self, args: &Value) -> Result<Value, NxpError> {
        let id = args.get("id").and_then(|v| v.as_str())
            .ok_or_else(|| NxpError::protocol(protocol_codes::DECODE_FAILED, "missing id"))?;
        self.service.engine.enable(id)
            .map_err(|e| NxpError::protocol(protocol_codes::DECODE_FAILED, e.to_string()))?;
        Ok(serde_json::json!({ "ok": true }))
    }

    fn cmd_disable(&self, args: &Value) -> Result<Value, NxpError> {
        let id = args.get("id").and_then(|v| v.as_str())
            .ok_or_else(|| NxpError::protocol(protocol_codes::DECODE_FAILED, "missing id"))?;
        self.service.engine.disable(id)
            .map_err(|e| NxpError::protocol(protocol_codes::DECODE_FAILED, e.to_string()))?;
        Ok(serde_json::json!({ "ok": true }))
    }

    fn cmd_trigger(&self, args: &Value) -> Result<Value, NxpError> {
        let id = args.get("id").and_then(|v| v.as_str())
            .ok_or_else(|| NxpError::protocol(protocol_codes::DECODE_FAILED, "missing id"))?;
        let payload = args.get("payload").and_then(|v| v.as_str());
        let exec = self.service.engine.trigger_manual(id, payload)
            .map_err(|e| NxpError::protocol(protocol_codes::DECODE_FAILED, e.to_string()))?;
        Ok(serde_json::json!({ "ok": true, "execution": exec }))
    }

    fn cmd_trigger_event(&self, args: &Value) -> Result<Value, NxpError> {
        let event_name = args.get("event_name").and_then(|v| v.as_str())
            .ok_or_else(|| NxpError::protocol(protocol_codes::DECODE_FAILED, "missing event_name"))?;
        let payload = args.get("payload").and_then(|v| v.as_str()).unwrap_or("");
        let execs = self.service.engine.trigger_event(event_name, payload);
        Ok(serde_json::json!({
            "ok": true,
            "triggered": execs.len(),
            "executions": execs,
        }))
    }

    fn cmd_list_executions(&self) -> Result<Value, NxpError> {
        let execs = self.service.engine.list_executions();
        Ok(serde_json::json!({ "ok": true, "count": execs.len(), "executions": execs }))
    }

    fn cmd_stats(&self) -> Result<Value, NxpError> {
        let execs = self.service.engine.list_executions();
        let succeeded = execs.iter().filter(|e| e.status == ExecutionStatus::Succeeded).count();
        let failed = execs.iter().filter(|e| e.status == ExecutionStatus::Failed).count();
        let stopped = execs.iter().filter(|e| e.status == ExecutionStatus::Stopped).count();
        Ok(serde_json::json!({
            "ok": true,
            "stats": {
                "workflow_count": self.service.engine.workflow_count(),
                "execution_count": execs.len(),
                "succeeded": succeeded,
                "failed": failed,
                "stopped": stopped,
            }
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use nexora_core::NexoraCore;

    fn setup() -> WorkflowHandler {
        let core = Arc::new(NexoraCore::new());
        let svc = Arc::new(WorkflowService::new(core));
        WorkflowHandler::new(svc)
    }

    #[tokio::test]
    async fn register_and_list() {
        let h = setup();
        let wf = serde_json::json!({
            "id": "wf1",
            "name": "Test",
            "description": "",
            "trigger": { "kind": "manual" },
            "steps": [
                { "name": "log", "action": { "kind": "log", "message": "hello" } }
            ],
            "enabled": true,
            "created_at": 0,
            "execution_count": 0,
        });
        h.execute("workflow.register", &wf).await.unwrap();
        let resp = h.execute("workflow.list", &serde_json::json!({})).await.unwrap();
        assert_eq!(resp["count"], 1);
    }

    #[tokio::test]
    async fn trigger_manual_works() {
        let h = setup();
        h.execute("workflow.register", &serde_json::json!({
            "id": "wf1", "name": "Test", "description": "",
            "trigger": { "kind": "manual" },
            "steps": [{ "name": "log", "action": { "kind": "log", "message": "hi" } }],
            "enabled": true, "created_at": 0, "execution_count": 0,
        })).await.unwrap();
        let resp = h.execute("workflow.trigger", &serde_json::json!({"id":"wf1"})).await.unwrap();
        assert_eq!(resp["execution"]["status"], "succeeded");
    }

    #[tokio::test]
    async fn stats_work() {
        let h = setup();
        h.execute("workflow.register", &serde_json::json!({
            "id": "wf1", "name": "Test", "description": "",
            "trigger": { "kind": "manual" },
            "steps": [{ "name": "log", "action": { "kind": "log", "message": "hi" } }],
            "enabled": true, "created_at": 0, "execution_count": 0,
        })).await.unwrap();
        h.execute("workflow.trigger", &serde_json::json!({"id":"wf1"})).await.unwrap();
        let resp = h.execute("workflow.stats", &serde_json::json!({})).await.unwrap();
        assert_eq!(resp["stats"]["workflow_count"], 1);
        assert_eq!(resp["stats"]["execution_count"], 1);
        assert_eq!(resp["stats"]["succeeded"], 1);
    }

    #[tokio::test]
    async fn unknown_command_rejected() {
        let h = setup();
        let err = h.execute("workflow.nope", &serde_json::json!({})).await.unwrap_err();
        assert_eq!(err.code, protocol_codes::UNKNOWN_OPCODE);
    }
}
