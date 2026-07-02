//! Permission Engine — hierarchical RBAC + ABAC.
//!
//! See Nexora Engineering Specification, Part 4 (PERMISSION ENGINE) and
//! Part 9 (AUTHORIZATION ENGINE). Permissions are hierarchical and
//! context-aware. Support: Users, Groups, Teams, Organizations, Projects,
//! Modules, Plugins, Resources, Commands, Streams.
//!
//! # Model
//!
//! A `Principal` (user, service, plugin, AI agent) holds a set of `Role`s.
//! Each `Role` grants a set of `Permission`s on a `Resource` pattern.
//! Permission checks evaluate: principal → roles → permissions → resource
//! pattern match → allow/deny.

use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::fmt;

/// Principal identifier (user ID, service ID, plugin ID, etc.).
pub type PrincipalId = String;

/// Role identifier (e.g. `admin`, `developer`, `viewer`).
pub type RoleId = String;

/// Resource identifier pattern (e.g. `project:123`, `module:*`).
pub type ResourcePattern = String;

/// A single permission check.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Permission {
    /// Read access.
    Read,
    /// Write / modify access.
    Write,
    /// Create new resource.
    Create,
    /// Delete resource.
    Delete,
    /// Execute a command.
    Execute,
    /// Administrative operations.
    Admin,
}

impl fmt::Display for Permission {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Read => f.write_str("read"),
            Self::Write => f.write_str("write"),
            Self::Create => f.write_str("create"),
            Self::Delete => f.write_str("delete"),
            Self::Execute => f.write_str("execute"),
            Self::Admin => f.write_str("admin"),
        }
    }
}

/// A principal — anything that can be granted permissions.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Principal {
    /// Unique principal ID.
    pub id: PrincipalId,
    /// Display name.
    pub name: String,
    /// Principal kind (user, service, plugin, ai_agent).
    pub kind: PrincipalKind,
    /// Roles assigned to this principal.
    pub roles: HashSet<RoleId>,
}

/// What kind of principal this is.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PrincipalKind {
    /// Human user.
    User,
    /// Backend service.
    Service,
    /// Plugin.
    Plugin,
    /// AI agent (deferred — Part 11).
    AiAgent,
}

/// A role grants a set of permissions on a resource pattern.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Role {
    /// Role ID.
    pub id: RoleId,
    /// Human-readable description.
    pub description: String,
    /// Permissions granted by this role.
    pub grants: Vec<Grant>,
}

/// A single grant: permission on a resource pattern.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Grant {
    /// Permission granted.
    pub permission: Permission,
    /// Resource pattern (supports `*` wildcard suffix).
    pub resource: ResourcePattern,
}

/// Result of an authorization check.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Decision {
    /// Allowed.
    Allow,
    /// Denied.
    Deny,
}

/// Error from the permission engine.
#[derive(Debug, thiserror::Error)]
pub enum PermissionError {
    /// Principal not found.
    #[error("principal not found: {0}")]
    PrincipalNotFound(PrincipalId),
    /// Role not found.
    #[error("role not found: {0}")]
    RoleNotFound(RoleId),
}

/// The Permission Engine. Thread-safe.
pub struct PermissionEngine {
    principals: RwLock<HashMap<PrincipalId, Principal>>,
    roles: RwLock<HashMap<RoleId, Role>>,
}

impl fmt::Debug for PermissionEngine {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let p = self.principals.read().len();
        let r = self.roles.read().len();
        f.debug_struct("PermissionEngine")
            .field("principals", &p)
            .field("roles", &r)
            .finish()
    }
}

impl Default for PermissionEngine {
    fn default() -> Self {
        Self::new()
    }
}

impl PermissionEngine {
    /// Construct an empty permission engine.
    pub fn new() -> Self {
        Self {
            principals: RwLock::new(HashMap::new()),
            roles: RwLock::new(HashMap::new()),
        }
    }

    /// Number of registered principals.
    pub fn principal_count(&self) -> usize {
        self.principals.read().len()
    }

    /// Register a role.
    pub fn register_role(&self, role: Role) {
        self.roles.write().insert(role.id.clone(), role);
    }

    /// Register a principal.
    pub fn register_principal(&self, principal: Principal) {
        self.principals
            .write()
            .insert(principal.id.clone(), principal);
    }

    /// Assign a role to a principal.
    pub fn assign_role(&self, principal_id: &str, role_id: &str) -> Result<(), PermissionError> {
        let mut principals = self.principals.write();
        let principal = principals
            .get_mut(principal_id)
            .ok_or_else(|| PermissionError::PrincipalNotFound(principal_id.to_string()))?;
        // Verify role exists.
        if !self.roles.read().contains_key(role_id) {
            return Err(PermissionError::RoleNotFound(role_id.to_string()));
        }
        principal.roles.insert(role_id.to_string());
        Ok(())
    }

    /// Revoke a role from a principal.
    pub fn revoke_role(&self, principal_id: &str, role_id: &str) -> Result<(), PermissionError> {
        let mut principals = self.principals.write();
        let principal = principals
            .get_mut(principal_id)
            .ok_or_else(|| PermissionError::PrincipalNotFound(principal_id.to_string()))?;
        principal.roles.remove(role_id);
        Ok(())
    }

    /// Check whether a principal has a permission on a resource.
    pub fn check(
        &self,
        principal_id: &str,
        permission: Permission,
        resource: &str,
    ) -> Decision {
        let principals = self.principals.read();
        let roles = self.roles.read();
        let principal = match principals.get(principal_id) {
            Some(p) => p,
            None => return Decision::Deny,
        };
        for role_id in &principal.roles {
            if let Some(role) = roles.get(role_id) {
                for grant in &role.grants {
                    if grant.permission == permission
                        && pattern_matches(&grant.resource, resource)
                    {
                        return Decision::Allow;
                    }
                }
            }
        }
        Decision::Deny
    }

    /// Convenience: returns `true` if allowed.
    pub fn is_allowed(
        &self,
        principal_id: &str,
        permission: Permission,
        resource: &str,
    ) -> bool {
        self.check(principal_id, permission, resource) == Decision::Allow
    }

    /// List all principals (snapshot).
    pub fn list_principals(&self) -> Vec<Principal> {
        self.principals.read().values().cloned().collect()
    }
}

/// Match a resource pattern against a concrete resource. Supports `*` as a
/// trailing wildcard.
fn pattern_matches(pattern: &str, resource: &str) -> bool {
    if pattern == "*" {
        return true;
    }
    if let Some(prefix) = pattern.strip_suffix("*") {
        return resource.starts_with(prefix);
    }
    pattern == resource
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_engine() -> PermissionEngine {
        let engine = PermissionEngine::new();
        // Admin role: full access (Admin permission) + wildcard grants for
        // every other permission. In production this would be expressed via
        // a single "owner" grant; for the MVP we enumerate.
        engine.register_role(Role {
            id: "admin".into(),
            description: "Full admin".into(),
            grants: vec![
                Grant { permission: Permission::Admin,   resource: "*".into() },
                Grant { permission: Permission::Read,    resource: "*".into() },
                Grant { permission: Permission::Write,   resource: "*".into() },
                Grant { permission: Permission::Create,  resource: "*".into() },
                Grant { permission: Permission::Delete,  resource: "*".into() },
                Grant { permission: Permission::Execute, resource: "*".into() },
            ],
        });
        engine.register_role(Role {
            id: "dev".into(),
            description: "Developer".into(),
            grants: vec![
                Grant {
                    permission: Permission::Read,
                    resource: "project:*".into(),
                },
                Grant {
                    permission: Permission::Write,
                    resource: "project:*".into(),
                },
                Grant {
                    permission: Permission::Execute,
                    resource: "command:*".into(),
                },
            ],
        });
        engine.register_role(Role {
            id: "viewer".into(),
            description: "Read-only".into(),
            grants: vec![Grant {
                permission: Permission::Read,
                resource: "project:*".into(),
            }],
        });
        engine
    }

    #[test]
    fn admin_can_do_anything() {
        let engine = sample_engine();
        engine.register_principal(Principal {
            id: "u1".into(),
            name: "Alice".into(),
            kind: PrincipalKind::User,
            roles: vec!["admin".to_string()].into_iter().collect(),
        });
        assert!(engine.is_allowed("u1", Permission::Admin, "anything"));
        assert!(engine.is_allowed("u1", Permission::Delete, "module:auth"));
    }

    #[test]
    fn dev_can_read_write_projects_but_not_admin() {
        let engine = sample_engine();
        engine.register_principal(Principal {
            id: "u2".into(),
            name: "Bob".into(),
            kind: PrincipalKind::User,
            roles: vec!["dev".to_string()].into_iter().collect(),
        });
        assert!(engine.is_allowed("u2", Permission::Read, "project:123"));
        assert!(engine.is_allowed("u2", Permission::Write, "project:123"));
        assert!(engine.is_allowed("u2", Permission::Execute, "command:deploy"));
        assert!(!engine.is_allowed("u2", Permission::Delete, "project:123"));
        assert!(!engine.is_allowed("u2", Permission::Admin, "project:123"));
        // Outside scope
        assert!(!engine.is_allowed("u2", Permission::Read, "billing:invoice"));
    }

    #[test]
    fn viewer_can_only_read() {
        let engine = sample_engine();
        engine.register_principal(Principal {
            id: "u3".into(),
            name: "Carol".into(),
            kind: PrincipalKind::User,
            roles: vec!["viewer".to_string()].into_iter().collect(),
        });
        assert!(engine.is_allowed("u3", Permission::Read, "project:123"));
        assert!(!engine.is_allowed("u3", Permission::Write, "project:123"));
    }

    #[test]
    fn unknown_principal_denied() {
        let engine = sample_engine();
        assert!(!engine.is_allowed("nobody", Permission::Read, "project:1"));
    }

    #[test]
    fn assign_and_revoke_role() {
        let engine = sample_engine();
        engine.register_principal(Principal {
            id: "u4".into(),
            name: "Dave".into(),
            kind: PrincipalKind::User,
            roles: HashSet::new(),
        });
        assert!(!engine.is_allowed("u4", Permission::Read, "project:1"));
        engine.assign_role("u4", "viewer").unwrap();
        assert!(engine.is_allowed("u4", Permission::Read, "project:1"));
        engine.revoke_role("u4", "viewer").unwrap();
        assert!(!engine.is_allowed("u4", Permission::Read, "project:1"));
    }

    #[test]
    fn pattern_matching() {
        assert!(pattern_matches("*", "anything"));
        assert!(pattern_matches("project:*", "project:123"));
        assert!(pattern_matches("project:*", "project:abc:def"));
        assert!(!pattern_matches("project:*", "module:auth"));
        assert!(pattern_matches("project:123", "project:123"));
        assert!(!pattern_matches("project:123", "project:456"));
    }
}
