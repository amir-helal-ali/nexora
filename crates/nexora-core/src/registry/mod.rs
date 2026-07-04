//! Service Registry — logical-name → service-instance lookup.
//!
//! See Nexora Engineering Specification, Part 4 (SERVICE DISCOVERY).
//! Every service automatically registers with the Core. Services never
//! communicate through hardcoded addresses; everything uses logical
//! identities.

use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fmt;
use std::net::SocketAddr;
use time::OffsetDateTime;

/// Logical service name (e.g. `auth`, `billing.invoice`).
pub type ServiceName = String;

/// Unique instance ID for a service (typically a UUID).
pub type InstanceId = String;

/// A registered service instance.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ServiceInstance {
    /// Logical service name.
    pub name: ServiceName,
    /// Unique instance ID.
    pub instance_id: InstanceId,
    /// NXP endpoint address.
    pub addr: SocketAddr,
    /// Capabilities advertised by this instance.
    pub capabilities: Vec<String>,
    /// Region (e.g. `eu-west-1`, `us-east-1`).
    pub region: String,
    /// Last heartbeat timestamp (unix nanos).
    pub last_heartbeat: i64,
    /// Whether the instance is currently healthy.
    pub healthy: bool,
    /// Optional weighted priority for load balancing (higher = preferred).
    pub priority: u32,
}

/// Error from registry operations.
#[derive(Debug, thiserror::Error)]
pub enum RegistryError {
    /// No instance found for the given name.
    #[error("no instance found for service: {0}")]
    NotFound(ServiceName),
    /// Instance already registered.
    #[error("instance already registered: {0}")]
    AlreadyExists(InstanceId),
}

/// Service Registry. Thread-safe.
pub struct ServiceRegistry {
    instances: RwLock<HashMap<InstanceId, ServiceInstance>>,
    by_name: RwLock<HashMap<ServiceName, Vec<InstanceId>>>,
}

impl fmt::Debug for ServiceRegistry {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let count = self.instances.read().len();
        let names = self.by_name.read().len();
        f.debug_struct("ServiceRegistry")
            .field("instance_count", &count)
            .field("service_names", &names)
            .finish()
    }
}

impl Default for ServiceRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl ServiceRegistry {
    /// Construct an empty registry.
    pub fn new() -> Self {
        Self {
            instances: RwLock::new(HashMap::new()),
            by_name: RwLock::new(HashMap::new()),
        }
    }

    /// Number of registered instances.
    pub fn service_count(&self) -> usize {
        self.instances.read().len()
    }

    /// Register a new service instance.
    pub fn register(&self, instance: ServiceInstance) -> Result<(), RegistryError> {
        let mut instances = self.instances.write();
        if instances.contains_key(&instance.instance_id) {
            return Err(RegistryError::AlreadyExists(instance.instance_id.clone()));
        }
        let name = instance.name.clone();
        let id = instance.instance_id.clone();
        instances.insert(id.clone(), instance);
        drop(instances);
        let mut by_name = self.by_name.write();
        by_name.entry(name).or_default().push(id);
        Ok(())
    }

    /// Deregister a service instance.
    pub fn deregister(&self, instance_id: &str) -> bool {
        let mut instances = self.instances.write();
        let removed = instances.remove(instance_id);
        if let Some(inst) = &removed {
            let mut by_name = self.by_name.write();
            if let Some(ids) = by_name.get_mut(&inst.name) {
                ids.retain(|x| x != instance_id);
                if ids.is_empty() {
                    by_name.remove(&inst.name);
                }
            }
        }
        removed.is_some()
    }

    /// Mark a heartbeat from an instance.
    pub fn heartbeat(&self, instance_id: &str) -> bool {
        let mut instances = self.instances.write();
        if let Some(inst) = instances.get_mut(instance_id) {
            inst.last_heartbeat = OffsetDateTime::now_utc().unix_timestamp_nanos() as i64;
            inst.healthy = true;
            true
        } else {
            false
        }
    }

    /// Mark an instance as unhealthy.
    pub fn mark_unhealthy(&self, instance_id: &str) -> bool {
        let mut instances = self.instances.write();
        if let Some(inst) = instances.get_mut(instance_id) {
            inst.healthy = false;
            true
        } else {
            false
        }
    }

    /// Lookup all instances for a logical name.
    pub fn lookup(&self, name: &str) -> Vec<ServiceInstance> {
        let by_name = self.by_name.read();
        let instances = self.instances.read();
        by_name
            .get(name)
            .cloned()
            .unwrap_or_default()
            .iter()
            .filter_map(|id| instances.get(id).cloned())
            .collect()
    }

    /// Lookup only healthy instances for a logical name.
    pub fn lookup_healthy(&self, name: &str) -> Vec<ServiceInstance> {
        self.lookup(name).into_iter().filter(|i| i.healthy).collect()
    }

    /// Pick the highest-priority healthy instance for a name. Returns
    /// `NotFound` if no healthy instance exists.
    pub fn pick_one(&self, name: &str) -> Result<ServiceInstance, RegistryError> {
        let mut healthy = self.lookup_healthy(name);
        healthy.sort_by(|a, b| b.priority.cmp(&a.priority));
        healthy
            .into_iter()
            .next()
            .ok_or_else(|| RegistryError::NotFound(name.to_string()))
    }

    /// Snapshot of all instances.
    pub fn list(&self) -> Vec<ServiceInstance> {
        self.instances.read().values().cloned().collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_instance(name: &str, id: &str, priority: u32) -> ServiceInstance {
        ServiceInstance {
            name: name.to_string(),
            instance_id: id.to_string(),
            addr: "127.0.0.1:4433".parse().unwrap(),
            capabilities: vec!["nxp/1".to_string()],
            region: "eu-west-1".to_string(),
            last_heartbeat: 0,
            healthy: true,
            priority,
        }
    }

    #[test]
    fn register_lookup_deregister() {
        let reg = ServiceRegistry::new();
        reg.register(sample_instance("auth", "auth-1", 10)).unwrap();
        reg.register(sample_instance("auth", "auth-2", 20)).unwrap();
        reg.register(sample_instance("billing", "bill-1", 10)).unwrap();

        assert_eq!(reg.lookup("auth").len(), 2);
        assert_eq!(reg.lookup("billing").len(), 1);
        assert_eq!(reg.lookup("nope").len(), 0);

        // pick_one returns the highest priority
        let picked = reg.pick_one("auth").unwrap();
        assert_eq!(picked.instance_id, "auth-2");

        reg.deregister("auth-2");
        assert_eq!(reg.lookup("auth").len(), 1);
    }

    #[test]
    fn unhealthy_filtered() {
        let reg = ServiceRegistry::new();
        reg.register(sample_instance("auth", "auth-1", 10)).unwrap();
        reg.register(sample_instance("auth", "auth-2", 20)).unwrap();
        reg.mark_unhealthy("auth-2");
        let picked = reg.pick_one("auth").unwrap();
        assert_eq!(picked.instance_id, "auth-1");
    }

    #[test]
    fn duplicate_instance_rejected() {
        let reg = ServiceRegistry::new();
        reg.register(sample_instance("auth", "auth-1", 10)).unwrap();
        assert!(matches!(
            reg.register(sample_instance("auth", "auth-1", 10)),
            Err(RegistryError::AlreadyExists(_))
        ));
    }
}
