//! Cluster node types.

use serde::{Deserialize, Serialize};
use std::fmt;
use time::OffsetDateTime;

/// Unique node ID (UUID v4).
pub type NodeId = String;

/// Node role in the cluster.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NodeRole {
    /// Global control plane node.
    Global,
    /// Regional cluster node.
    Regional,
    /// Edge node (lightweight, low-resource).
    Edge,
    /// Local execution node.
    Local,
}

impl fmt::Display for NodeRole {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Global => f.write_str("global"),
            Self::Regional => f.write_str("regional"),
            Self::Edge => f.write_str("edge"),
            Self::Local => f.write_str("local"),
        }
    }
}

/// Geographic region.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct NodeRegion(pub String);

impl fmt::Display for NodeRegion {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl NodeRegion {
    /// Common regions.
    pub fn eu_west_1() -> Self { Self("eu-west-1".into()) }
    /// US East.
    pub fn us_east_1() -> Self { Self("us-east-1".into()) }
    /// Asia Pacific.
    pub fn ap_southeast_1() -> Self { Self("ap-southeast-1".into()) }
    /// Edge (any).
    pub fn edge() -> Self { Self("edge".into()) }
}

/// Node health status.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NodeStatus {
    /// Node is healthy and accepting traffic.
    Healthy,
    /// Node is degraded but operational.
    Degraded,
    /// Node is unhealthy — traffic should be rerouted.
    Unhealthy,
    /// Node has gone offline (missed heartbeats).
    Offline,
}

impl fmt::Display for NodeStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Healthy => f.write_str("healthy"),
            Self::Degraded => f.write_str("degraded"),
            Self::Unhealthy => f.write_str("unhealthy"),
            Self::Offline => f.write_str("offline"),
        }
    }
}

/// A cluster node.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ClusterNode {
    /// Unique node ID.
    pub id: NodeId,
    /// Human-readable name.
    pub name: String,
    /// Node role.
    pub role: NodeRole,
    /// Geographic region.
    pub region: NodeRegion,
    /// NXP endpoint address (e.g. "10.0.0.5:4433").
    pub addr: String,
    /// Capabilities advertised by this node.
    pub capabilities: Vec<String>,
    /// Priority for load balancing (higher = preferred).
    pub priority: u32,
    /// Current status.
    pub status: NodeStatus,
    /// When the node registered (unix nanos).
    pub registered_at: i64,
    /// Last heartbeat timestamp (unix nanos).
    pub last_heartbeat: i64,
    /// Number of heartbeats received.
    pub heartbeat_count: u64,
    /// Whether this is the local node.
    pub is_local: bool,
}

impl ClusterNode {
    /// Construct a new node.
    pub fn new(
        id: impl Into<String>,
        name: impl Into<String>,
        role: NodeRole,
        region: NodeRegion,
        addr: impl Into<String>,
    ) -> Self {
        let now = OffsetDateTime::now_utc().unix_timestamp_nanos() as i64;
        Self {
            id: id.into(),
            name: name.into(),
            role,
            region,
            addr: addr.into(),
            capabilities: vec![],
            priority: 0,
            status: NodeStatus::Healthy,
            registered_at: now,
            last_heartbeat: now,
            heartbeat_count: 0,
            is_local: false,
        }
    }

    /// Check if this node's heartbeat is stale (older than the given threshold in seconds).
    pub fn is_heartbeat_stale(&self, threshold_seconds: u64) -> bool {
        let now = OffsetDateTime::now_utc().unix_timestamp_nanos() as i64;
        let threshold_ns = threshold_seconds as i64 * 1_000_000_000;
        now - self.last_heartbeat > threshold_ns
    }
}

/// Cluster-wide statistics.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ClusterStats {
    /// Total nodes.
    pub total_nodes: usize,
    /// Healthy nodes.
    pub healthy_nodes: usize,
    /// Degraded nodes.
    pub degraded_nodes: usize,
    /// Unhealthy nodes.
    pub unhealthy_nodes: usize,
    /// Offline nodes.
    pub offline_nodes: usize,
    /// Nodes by role.
    pub by_role: std::collections::HashMap<String, usize>,
    /// Nodes by region.
    pub by_region: std::collections::HashMap<String, usize>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn node_construction() {
        let node = ClusterNode::new(
            "node-1",
            "EU-West Primary",
            NodeRole::Regional,
            NodeRegion::eu_west_1(),
            "10.0.0.5:4433",
        );
        assert_eq!(node.id, "node-1");
        assert_eq!(node.role, NodeRole::Regional);
        assert_eq!(node.status, NodeStatus::Healthy);
        assert!(!node.is_local);
    }

    #[test]
    fn heartbeat_staleness() {
        let mut node = ClusterNode::new(
            "node-1", "Test", NodeRole::Edge, NodeRegion::edge(), "0.0.0.0:4433",
        );
        // Backdate the heartbeat.
        node.last_heartbeat = 0;
        assert!(node.is_heartbeat_stale(60));
    }

    #[test]
    fn fresh_heartbeat_not_stale() {
        let node = ClusterNode::new(
            "node-1", "Test", NodeRole::Edge, NodeRegion::edge(), "0.0.0.0:4433",
        );
        assert!(!node.is_heartbeat_stale(60));
    }

    #[test]
    fn role_display() {
        assert_eq!(NodeRole::Global.to_string(), "global");
        assert_eq!(NodeRole::Edge.to_string(), "edge");
    }

    #[test]
    fn status_display() {
        assert_eq!(NodeStatus::Healthy.to_string(), "healthy");
        assert_eq!(NodeStatus::Offline.to_string(), "offline");
    }
}
