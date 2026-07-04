//! Cluster manager — node registry, discovery, failover.

use crate::types::{ClusterNode, ClusterStats, NodeId, NodeRegion, NodeRole, NodeStatus};
use parking_lot::RwLock;
use std::collections::HashMap;
use std::fmt;
use std::sync::Arc;
use time::OffsetDateTime;

/// Heartbeat timeout: 3 missed heartbeats (15s at 5s intervals).
const HEARTBEAT_TIMEOUT_SECONDS: u64 = 15;

/// Error from cluster operations.
#[derive(Debug, thiserror::Error)]
pub enum ClusterError {
    /// Node not found.
    #[error("node not found: {0}")]
    NotFound(NodeId),
    /// Node already registered.
    #[error("node already registered: {0}")]
    AlreadyExists(NodeId),
    /// No healthy nodes available.
    #[error("no healthy nodes available")]
    NoHealthyNodes,
}

/// The cluster manager. Thread-safe.
pub struct ClusterManager {
    nodes: RwLock<HashMap<NodeId, ClusterNode>>,
    local_node_id: RwLock<Option<NodeId>>,
    event_bus: Option<Arc<nexora_core::EventBus>>,
}

impl fmt::Debug for ClusterManager {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let count = self.nodes.read().len();
        let healthy = self.nodes.read().values().filter(|n| n.status == NodeStatus::Healthy).count();
        f.debug_struct("ClusterManager")
            .field("nodes", &count)
            .field("healthy", &healthy)
            .finish()
    }
}

impl Default for ClusterManager {
    fn default() -> Self {
        Self::new()
    }
}

impl ClusterManager {
    /// Construct an empty cluster manager.
    pub fn new() -> Self {
        Self {
            nodes: RwLock::new(HashMap::new()),
            local_node_id: RwLock::new(None),
            event_bus: None,
        }
    }

    /// Attach an EventBus.
    pub fn with_event_bus(mut self, bus: Arc<nexora_core::EventBus>) -> Self {
        self.event_bus = Some(bus);
        self
    }

    /// Number of registered nodes.
    pub fn node_count(&self) -> usize {
        self.nodes.read().len()
    }

    /// Number of healthy nodes.
    pub fn healthy_count(&self) -> usize {
        self.nodes
            .read()
            .values()
            .filter(|n| n.status == NodeStatus::Healthy)
            .count()
    }

    /// Register a new node.
    pub fn register(&self, mut node: ClusterNode) -> Result<ClusterNode, ClusterError> {
        let mut nodes = self.nodes.write();
        if nodes.contains_key(&node.id) {
            return Err(ClusterError::AlreadyExists(node.id.clone()));
        }
        node.registered_at = OffsetDateTime::now_utc().unix_timestamp_nanos() as i64;
        node.last_heartbeat = node.registered_at;
        node.heartbeat_count = 0;
        let id = node.id.clone();
        nodes.insert(id.clone(), node.clone());
        drop(nodes);
        self.emit("cluster.node_registered", &id);
        Ok(node)
    }

    /// Deregister a node.
    pub fn deregister(&self, id: &str) -> Result<(), ClusterError> {
        let mut nodes = self.nodes.write();
        nodes
            .remove(id)
            .ok_or_else(|| ClusterError::NotFound(id.to_string()))?;
        drop(nodes);
        // Clear local node ID if it was this node.
        let mut local = self.local_node_id.write();
        if local.as_deref() == Some(id) {
            *local = None;
        }
        drop(local);
        self.emit("cluster.node_deregistered", id);
        Ok(())
    }

    /// Record a heartbeat from a node.
    pub fn heartbeat(&self, id: &str) -> Result<ClusterNode, ClusterError> {
        let mut nodes = self.nodes.write();
        let node = nodes
            .get_mut(id)
            .ok_or_else(|| ClusterError::NotFound(id.to_string()))?;
        node.last_heartbeat = OffsetDateTime::now_utc().unix_timestamp_nanos() as i64;
        node.heartbeat_count += 1;
        // If node was offline/unhealthy, restore to healthy.
        if node.status == NodeStatus::Offline || node.status == NodeStatus::Unhealthy {
            node.status = NodeStatus::Healthy;
        }
        let node = node.clone();
        drop(nodes);
        Ok(node)
    }

    /// Mark a node as unhealthy.
    pub fn mark_unhealthy(&self, id: &str) -> Result<ClusterNode, ClusterError> {
        let mut nodes = self.nodes.write();
        let node = nodes
            .get_mut(id)
            .ok_or_else(|| ClusterError::NotFound(id.to_string()))?;
        node.status = NodeStatus::Unhealthy;
        let node = node.clone();
        drop(nodes);
        self.emit("cluster.node_unhealthy", id);
        Ok(node)
    }

    /// Mark a node as degraded.
    pub fn mark_degraded(&self, id: &str) -> Result<ClusterNode, ClusterError> {
        let mut nodes = self.nodes.write();
        let node = nodes
            .get_mut(id)
            .ok_or_else(|| ClusterError::NotFound(id.to_string()))?;
        node.status = NodeStatus::Degraded;
        Ok(nodes.get(id).cloned().unwrap())
    }

    /// Get a node by ID.
    pub fn get(&self, id: &str) -> Option<ClusterNode> {
        self.nodes.read().get(id).cloned()
    }

    /// List all nodes.
    pub fn list(&self) -> Vec<ClusterNode> {
        self.nodes.read().values().cloned().collect()
    }

    /// List nodes by role.
    pub fn list_by_role(&self, role: NodeRole) -> Vec<ClusterNode> {
        self.nodes
            .read()
            .values()
            .filter(|n| n.role == role)
            .cloned()
            .collect()
    }

    /// List nodes by region.
    pub fn list_by_region(&self, region: &str) -> Vec<ClusterNode> {
        self.nodes
            .read()
            .values()
            .filter(|n| n.region.0 == region)
            .cloned()
            .collect()
    }

    /// List healthy nodes.
    pub fn list_healthy(&self) -> Vec<ClusterNode> {
        self.nodes
            .read()
            .values()
            .filter(|n| n.status == NodeStatus::Healthy)
            .cloned()
            .collect()
    }

    /// Pick the best node for serving a request (highest priority healthy node).
    pub fn pick_node(&self) -> Result<ClusterNode, ClusterError> {
        let mut healthy: Vec<ClusterNode> = self.list_healthy();
        if healthy.is_empty() {
            return Err(ClusterError::NoHealthyNodes);
        }
        healthy.sort_by(|a, b| b.priority.cmp(&a.priority));
        Ok(healthy.into_iter().next().unwrap())
    }

    /// Pick the best node in a specific region.
    pub fn pick_node_in_region(&self, region: &str) -> Result<ClusterNode, ClusterError> {
        let mut nodes: Vec<ClusterNode> = self
            .list_healthy()
            .into_iter()
            .filter(|n| n.region.0 == region)
            .collect();
        if nodes.is_empty() {
            return Err(ClusterError::NoHealthyNodes);
        }
        nodes.sort_by(|a, b| b.priority.cmp(&a.priority));
        Ok(nodes.into_iter().next().unwrap())
    }

    /// Set the local node ID.
    pub fn set_local(&self, id: &str) {
        *self.local_node_id.write() = Some(id.to_string());
    }

    /// Get the local node.
    pub fn get_local(&self) -> Option<ClusterNode> {
        let local_id = self.local_node_id.read().clone()?;
        self.get(&local_id)
    }

    /// Check all nodes for stale heartbeats and mark them offline.
    /// Returns the list of nodes that were marked offline.
    pub fn reap_stale_nodes(&self) -> Vec<ClusterNode> {
        let mut reaped = Vec::new();
        let mut nodes = self.nodes.write();
        for node in nodes.values_mut() {
            if node.status != NodeStatus::Offline && node.is_heartbeat_stale(HEARTBEAT_TIMEOUT_SECONDS) {
                node.status = NodeStatus::Offline;
                reaped.push(node.clone());
            }
        }
        drop(nodes);
        for node in &reaped {
            self.emit("cluster.node_offline", &node.id);
        }
        reaped
    }

    /// Get cluster-wide statistics.
    pub fn stats(&self) -> ClusterStats {
        let nodes = self.nodes.read();
        let mut by_role: HashMap<String, usize> = HashMap::new();
        let mut by_region: HashMap<String, usize> = HashMap::new();
        let mut healthy = 0;
        let mut degraded = 0;
        let mut unhealthy = 0;
        let mut offline = 0;

        for node in nodes.values() {
            *by_role.entry(node.role.to_string()).or_default() += 1;
            *by_region.entry(node.region.0.clone()).or_default() += 1;
            match node.status {
                NodeStatus::Healthy => healthy += 1,
                NodeStatus::Degraded => degraded += 1,
                NodeStatus::Unhealthy => unhealthy += 1,
                NodeStatus::Offline => offline += 1,
            }
        }

        ClusterStats {
            total_nodes: nodes.len(),
            healthy_nodes: healthy,
            degraded_nodes: degraded,
            unhealthy_nodes: unhealthy,
            offline_nodes: offline,
            by_role,
            by_region,
        }
    }

    fn emit(&self, name: &str, id: &str) {
        if let Some(bus) = &self.event_bus {
            bus.publish(name, id.to_string());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn setup() -> ClusterManager {
        let bus = Arc::new(nexora_core::EventBus::new());
        ClusterManager::new().with_event_bus(bus)
    }

    fn sample_node(id: &str, role: NodeRole, region: NodeRegion, priority: u32) -> ClusterNode {
        let mut n = ClusterNode::new(id, format!("{} node", id), role, region, "0.0.0.0:4433");
        n.priority = priority;
        n
    }

    #[test]
    fn register_and_get() {
        let mgr = setup();
        let node = mgr.register(sample_node("n1", NodeRole::Regional, NodeRegion::eu_west_1(), 10)).unwrap();
        assert_eq!(mgr.node_count(), 1);
        assert_eq!(mgr.get("n1").unwrap().id, "n1");
    }

    #[test]
    fn duplicate_register_rejected() {
        let mgr = setup();
        mgr.register(sample_node("n1", NodeRole::Regional, NodeRegion::eu_west_1(), 10)).unwrap();
        assert!(mgr.register(sample_node("n1", NodeRole::Regional, NodeRegion::eu_west_1(), 10)).is_err());
    }

    #[test]
    fn heartbeat_updates_timestamp() {
        let mgr = setup();
        mgr.register(sample_node("n1", NodeRole::Regional, NodeRegion::eu_west_1(), 10)).unwrap();
        std::thread::sleep(std::time::Duration::from_millis(10));
        let node = mgr.heartbeat("n1").unwrap();
        assert_eq!(node.heartbeat_count, 1);
        assert!(node.last_heartbeat > node.registered_at);
    }

    #[test]
    fn heartbeat_restores_offline_node() {
        let mgr = setup();
        let mut node = sample_node("n1", NodeRole::Regional, NodeRegion::eu_west_1(), 10);
        node.status = NodeStatus::Offline;
        mgr.register(node).unwrap();
        let restored = mgr.heartbeat("n1").unwrap();
        assert_eq!(restored.status, NodeStatus::Healthy);
    }

    #[test]
    fn deregister_removes_node() {
        let mgr = setup();
        mgr.register(sample_node("n1", NodeRole::Regional, NodeRegion::eu_west_1(), 10)).unwrap();
        mgr.deregister("n1").unwrap();
        assert_eq!(mgr.node_count(), 0);
    }

    #[test]
    fn pick_node_returns_highest_priority() {
        let mgr = setup();
        mgr.register(sample_node("n1", NodeRole::Regional, NodeRegion::eu_west_1(), 10)).unwrap();
        mgr.register(sample_node("n2", NodeRole::Regional, NodeRegion::eu_west_1(), 30)).unwrap();
        mgr.register(sample_node("n3", NodeRole::Regional, NodeRegion::eu_west_1(), 20)).unwrap();
        let picked = mgr.pick_node().unwrap();
        assert_eq!(picked.id, "n2"); // highest priority
    }

    #[test]
    fn pick_node_in_region() {
        let mgr = setup();
        mgr.register(sample_node("n1", NodeRole::Regional, NodeRegion::eu_west_1(), 10)).unwrap();
        mgr.register(sample_node("n2", NodeRole::Regional, NodeRegion::us_east_1(), 30)).unwrap();
        let picked = mgr.pick_node_in_region("eu-west-1").unwrap();
        assert_eq!(picked.id, "n1");
    }

    #[test]
    fn pick_node_no_healthy_returns_error() {
        let mgr = setup();
        let mut node = sample_node("n1", NodeRole::Regional, NodeRegion::eu_west_1(), 10);
        node.status = NodeStatus::Unhealthy;
        mgr.register(node).unwrap();
        assert!(mgr.pick_node().is_err());
    }

    #[test]
    fn mark_unhealthy_excludes_from_pick() {
        let mgr = setup();
        mgr.register(sample_node("n1", NodeRole::Regional, NodeRegion::eu_west_1(), 10)).unwrap();
        mgr.register(sample_node("n2", NodeRole::Regional, NodeRegion::eu_west_1(), 20)).unwrap();
        mgr.mark_unhealthy("n2").unwrap();
        let picked = mgr.pick_node().unwrap();
        assert_eq!(picked.id, "n1"); // n2 is unhealthy
    }

    #[test]
    fn reap_stale_nodes_marks_offline() {
        let mgr = setup();
        let mut node = sample_node("n1", NodeRole::Edge, NodeRegion::edge(), 10);
        node.last_heartbeat = 0; // very old
        mgr.register(node).unwrap();
        // register resets last_heartbeat, so we need to set it after.
        {
            let mut nodes = mgr.nodes.write();
            nodes.get_mut("n1").unwrap().last_heartbeat = 0;
        }
        let reaped = mgr.reap_stale_nodes();
        assert_eq!(reaped.len(), 1);
        assert_eq!(reaped[0].status, NodeStatus::Offline);
        assert_eq!(mgr.get("n1").unwrap().status, NodeStatus::Offline);
    }

    #[test]
    fn list_by_role_and_region() {
        let mgr = setup();
        mgr.register(sample_node("n1", NodeRole::Regional, NodeRegion::eu_west_1(), 10)).unwrap();
        mgr.register(sample_node("n2", NodeRole::Edge, NodeRegion::eu_west_1(), 5)).unwrap();
        mgr.register(sample_node("n3", NodeRole::Regional, NodeRegion::us_east_1(), 10)).unwrap();
        assert_eq!(mgr.list_by_role(NodeRole::Regional).len(), 2);
        assert_eq!(mgr.list_by_role(NodeRole::Edge).len(), 1);
        assert_eq!(mgr.list_by_region("eu-west-1").len(), 2);
        assert_eq!(mgr.list_by_region("us-east-1").len(), 1);
    }

    #[test]
    fn stats_aggregate_correctly() {
        let mgr = setup();
        mgr.register(sample_node("n1", NodeRole::Regional, NodeRegion::eu_west_1(), 10)).unwrap();
        mgr.register(sample_node("n2", NodeRole::Edge, NodeRegion::edge(), 5)).unwrap();
        mgr.mark_unhealthy("n2").unwrap();
        let stats = mgr.stats();
        assert_eq!(stats.total_nodes, 2);
        assert_eq!(stats.healthy_nodes, 1);
        assert_eq!(stats.unhealthy_nodes, 1);
        assert_eq!(stats.by_role.get("regional"), Some(&1));
        assert_eq!(stats.by_role.get("edge"), Some(&1));
    }

    #[test]
    fn local_node_tracking() {
        let mgr = setup();
        mgr.register(sample_node("n1", NodeRole::Global, NodeRegion::eu_west_1(), 100)).unwrap();
        mgr.set_local("n1");
        let local = mgr.get_local().unwrap();
        assert_eq!(local.id, "n1");
    }

    #[test]
    fn events_emitted() {
        let bus = Arc::new(nexora_core::EventBus::new());
        let mgr = ClusterManager::new().with_event_bus(bus.clone());
        mgr.register(sample_node("n1", NodeRole::Regional, NodeRegion::eu_west_1(), 10)).unwrap();
        mgr.heartbeat("n1").unwrap();
        mgr.mark_unhealthy("n1").unwrap();
        mgr.deregister("n1").unwrap();
        let events = bus.replay_filtered(0, "cluster.");
        let names: Vec<&str> = events.iter().map(|e| e.name.as_str()).collect();
        assert!(names.contains(&"cluster.node_registered"));
        assert!(names.contains(&"cluster.node_unhealthy"));
        assert!(names.contains(&"cluster.node_deregistered"));
    }
}
