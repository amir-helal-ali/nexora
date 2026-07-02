//! Cluster handler — dispatches cluster commands via the Gateway.

use crate::manager::ClusterError;
use crate::types::{ClusterNode, NodeRegion, NodeRole, NodeStatus};
use crate::ClusterService;
use nxp_core::NxpError;
use nxp_core::error::protocol_codes;
use serde_json::Value;
use std::sync::Arc;

/// The Cluster handler.
#[derive(Clone)]
pub struct ClusterHandler {
    service: Arc<ClusterService>,
}

impl std::fmt::Debug for ClusterHandler {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ClusterHandler")
            .field("service", &self.service)
            .finish()
    }
}

impl ClusterHandler {
    /// Construct a new handler.
    pub fn new(service: Arc<ClusterService>) -> Self {
        Self { service }
    }

    /// Execute a cluster command.
    pub async fn execute(&self, command: &str, args: &Value) -> Result<Value, NxpError> {
        match command {
            "cluster.register" => self.cmd_register(args),
            "cluster.deregister" => self.cmd_deregister(args),
            "cluster.heartbeat" => self.cmd_heartbeat(args),
            "cluster.mark_unhealthy" => self.cmd_mark_unhealthy(args),
            "cluster.list" => self.cmd_list(),
            "cluster.get" => self.cmd_get(args),
            "cluster.list_healthy" => self.cmd_list_healthy(),
            "cluster.list_by_role" => self.cmd_list_by_role(args),
            "cluster.list_by_region" => self.cmd_list_by_region(args),
            "cluster.pick_node" => self.cmd_pick_node(),
            "cluster.pick_node_in_region" => self.cmd_pick_in_region(args),
            "cluster.reap_stale" => self.cmd_reap_stale(),
            "cluster.stats" => self.cmd_stats(),
            _ => Err(NxpError::protocol(
                protocol_codes::UNKNOWN_OPCODE,
                format!("unknown cluster command: {}", command),
            )),
        }
    }

    fn cmd_register(&self, args: &Value) -> Result<Value, NxpError> {
        let node: ClusterNode = serde_json::from_value(args.clone())
            .map_err(|e| NxpError::protocol(protocol_codes::DECODE_FAILED, e.to_string()))?;
        let node = self
            .service
            .manager
            .register(node)
            .map_err(map_cluster_error)?;
        Ok(serde_json::json!({ "ok": true, "node": node }))
    }

    fn cmd_deregister(&self, args: &Value) -> Result<Value, NxpError> {
        let id = args.get("id").and_then(|v| v.as_str())
            .ok_or_else(|| NxpError::protocol(protocol_codes::DECODE_FAILED, "missing id"))?;
        self.service.manager.deregister(id).map_err(map_cluster_error)?;
        Ok(serde_json::json!({ "ok": true }))
    }

    fn cmd_heartbeat(&self, args: &Value) -> Result<Value, NxpError> {
        let id = args.get("id").and_then(|v| v.as_str())
            .ok_or_else(|| NxpError::protocol(protocol_codes::DECODE_FAILED, "missing id"))?;
        let node = self.service.manager.heartbeat(id).map_err(map_cluster_error)?;
        Ok(serde_json::json!({ "ok": true, "node": node }))
    }

    fn cmd_mark_unhealthy(&self, args: &Value) -> Result<Value, NxpError> {
        let id = args.get("id").and_then(|v| v.as_str())
            .ok_or_else(|| NxpError::protocol(protocol_codes::DECODE_FAILED, "missing id"))?;
        let node = self.service.manager.mark_unhealthy(id).map_err(map_cluster_error)?;
        Ok(serde_json::json!({ "ok": true, "node": node }))
    }

    fn cmd_list(&self) -> Result<Value, NxpError> {
        let nodes = self.service.manager.list();
        Ok(serde_json::json!({ "ok": true, "count": nodes.len(), "nodes": nodes }))
    }

    fn cmd_get(&self, args: &Value) -> Result<Value, NxpError> {
        let id = args.get("id").and_then(|v| v.as_str())
            .ok_or_else(|| NxpError::protocol(protocol_codes::DECODE_FAILED, "missing id"))?;
        let node = self.service.manager.get(id)
            .ok_or_else(|| NxpError::protocol(protocol_codes::DECODE_FAILED, format!("node {} not found", id)))?;
        Ok(serde_json::json!({ "ok": true, "node": node }))
    }

    fn cmd_list_healthy(&self) -> Result<Value, NxpError> {
        let nodes = self.service.manager.list_healthy();
        Ok(serde_json::json!({ "ok": true, "count": nodes.len(), "nodes": nodes }))
    }

    fn cmd_list_by_role(&self, args: &Value) -> Result<Value, NxpError> {
        let role_str = args.get("role").and_then(|v| v.as_str())
            .ok_or_else(|| NxpError::protocol(protocol_codes::DECODE_FAILED, "missing role"))?;
        let role = parse_role(role_str)
            .ok_or_else(|| NxpError::protocol(protocol_codes::DECODE_FAILED, format!("unknown role: {}", role_str)))?;
        let nodes = self.service.manager.list_by_role(role);
        Ok(serde_json::json!({ "ok": true, "count": nodes.len(), "nodes": nodes }))
    }

    fn cmd_list_by_region(&self, args: &Value) -> Result<Value, NxpError> {
        let region = args.get("region").and_then(|v| v.as_str())
            .ok_or_else(|| NxpError::protocol(protocol_codes::DECODE_FAILED, "missing region"))?;
        let nodes = self.service.manager.list_by_region(region);
        Ok(serde_json::json!({ "ok": true, "count": nodes.len(), "nodes": nodes }))
    }

    fn cmd_pick_node(&self) -> Result<Value, NxpError> {
        let node = self.service.manager.pick_node().map_err(map_cluster_error)?;
        Ok(serde_json::json!({ "ok": true, "node": node }))
    }

    fn cmd_pick_in_region(&self, args: &Value) -> Result<Value, NxpError> {
        let region = args.get("region").and_then(|v| v.as_str())
            .ok_or_else(|| NxpError::protocol(protocol_codes::DECODE_FAILED, "missing region"))?;
        let node = self.service.manager.pick_node_in_region(region).map_err(map_cluster_error)?;
        Ok(serde_json::json!({ "ok": true, "node": node }))
    }

    fn cmd_reap_stale(&self) -> Result<Value, NxpError> {
        let reaped = self.service.manager.reap_stale_nodes();
        Ok(serde_json::json!({
            "ok": true,
            "reaped_count": reaped.len(),
            "reaped": reaped,
        }))
    }

    fn cmd_stats(&self) -> Result<Value, NxpError> {
        let stats = self.service.manager.stats();
        Ok(serde_json::json!({ "ok": true, "stats": stats }))
    }
}

fn parse_role(s: &str) -> Option<NodeRole> {
    match s {
        "global" => Some(NodeRole::Global),
        "regional" => Some(NodeRole::Regional),
        "edge" => Some(NodeRole::Edge),
        "local" => Some(NodeRole::Local),
        _ => None,
    }
}

fn map_cluster_error(e: ClusterError) -> NxpError {
    NxpError::protocol(protocol_codes::DECODE_FAILED, e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use nexora_core::NexoraCore;

    fn setup() -> ClusterHandler {
        let core = Arc::new(NexoraCore::new());
        let svc = Arc::new(ClusterService::new(core));
        ClusterHandler::new(svc)
    }

    #[tokio::test]
    async fn register_and_list() {
        let h = setup();
        h.execute("cluster.register", &serde_json::json!({
            "id": "n1",
            "name": "Test Node",
            "role": "regional",
            "region": "eu-west-1",
            "addr": "10.0.0.1:4433",
            "capabilities": [],
            "priority": 10,
            "status": "healthy",
            "registered_at": 0,
            "last_heartbeat": 0,
            "heartbeat_count": 0,
            "is_local": false
        })).await.unwrap();
        let resp = h.execute("cluster.list", &serde_json::json!({})).await.unwrap();
        assert_eq!(resp["count"], 1);
    }

    #[tokio::test]
    async fn heartbeat_works() {
        let h = setup();
        h.execute("cluster.register", &serde_json::json!({
            "id": "n1", "name": "Test", "role": "regional", "region": "eu-west-1",
            "addr": "10.0.0.1:4433", "capabilities": [], "priority": 10,
            "status": "healthy", "registered_at": 0, "last_heartbeat": 0,
            "heartbeat_count": 0, "is_local": false
        })).await.unwrap();
        let resp = h.execute("cluster.heartbeat", &serde_json::json!({"id":"n1"})).await.unwrap();
        assert_eq!(resp["node"]["heartbeat_count"], 1);
    }

    #[tokio::test]
    async fn stats_work() {
        let h = setup();
        h.execute("cluster.register", &serde_json::json!({
            "id": "n1", "name": "Test", "role": "regional", "region": "eu-west-1",
            "addr": "10.0.0.1:4433", "capabilities": [], "priority": 10,
            "status": "healthy", "registered_at": 0, "last_heartbeat": 0,
            "heartbeat_count": 0, "is_local": false
        })).await.unwrap();
        let resp = h.execute("cluster.stats", &serde_json::json!({})).await.unwrap();
        assert_eq!(resp["stats"]["total_nodes"], 1);
        assert_eq!(resp["stats"]["healthy_nodes"], 1);
    }

    #[tokio::test]
    async fn pick_node_works() {
        let h = setup();
        h.execute("cluster.register", &serde_json::json!({
            "id": "n1", "name": "Test", "role": "regional", "region": "eu-west-1",
            "addr": "10.0.0.1:4433", "capabilities": [], "priority": 10,
            "status": "healthy", "registered_at": 0, "last_heartbeat": 0,
            "heartbeat_count": 0, "is_local": false
        })).await.unwrap();
        let resp = h.execute("cluster.pick_node", &serde_json::json!({})).await.unwrap();
        assert_eq!(resp["node"]["id"], "n1");
    }

    #[tokio::test]
    async fn unknown_command_rejected() {
        let h = setup();
        let err = h.execute("cluster.nope", &serde_json::json!({})).await.unwrap_err();
        assert_eq!(err.code, protocol_codes::UNKNOWN_OPCODE);
    }
}
