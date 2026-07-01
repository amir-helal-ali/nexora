//! Tenant manager — organizations, memberships, teams.

use crate::types::{Membership, Organization, OrganizationId, OrgRole, OrgTier, Team, TeamId};
use parking_lot::RwLock;
use std::collections::HashMap;
use std::fmt;
use std::sync::Arc;
use time::OffsetDateTime;

/// Error from tenancy operations.
#[derive(Debug, thiserror::Error)]
pub enum TenancyError {
    /// Org not found.
    #[error("organization not found: {0}")]
    OrgNotFound(OrganizationId),
    /// Team not found.
    #[error("team not found: {0}")]
    TeamNotFound(TeamId),
    /// Org slug already taken.
    #[error("slug already taken: {0}")]
    SlugTaken(String),
    /// User is not a member of the org.
    #[error("user {user_id} is not a member of {org_id}")]
    NotMember { org_id: OrganizationId, user_id: String },
    /// User is already a member.
    #[error("user {user_id} is already a member of {org_id}")]
    AlreadyMember { org_id: OrganizationId, user_id: String },
    /// Max members exceeded.
    #[error("organization {0} has reached max members")]
    MaxMembers(OrganizationId),
    /// Insufficient permissions.
    #[error("user {user_id} lacks required role {required} in {org_id}")]
    InsufficientRole {
        org_id: OrganizationId,
        user_id: String,
        required: OrgRole,
    },
}

/// The tenant manager. Thread-safe.
pub struct TenantManager {
    orgs: RwLock<HashMap<OrganizationId, Organization>>,
    memberships: RwLock<Vec<Membership>>,
    teams: RwLock<HashMap<TeamId, Team>>,
    /// Map: user_id → list of org_ids.
    orgs_by_user: RwLock<HashMap<String, Vec<OrganizationId>>>,
    event_bus: Option<Arc<nexora_core::EventBus>>,
}

impl fmt::Debug for TenantManager {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("TenantManager")
            .field("orgs", &self.orgs.read().len())
            .field("teams", &self.teams.read().len())
            .finish()
    }
}

impl Default for TenantManager {
    fn default() -> Self {
        Self::new()
    }
}

impl TenantManager {
    /// Construct an empty manager.
    pub fn new() -> Self {
        Self {
            orgs: RwLock::new(HashMap::new()),
            memberships: RwLock::new(Vec::new()),
            teams: RwLock::new(HashMap::new()),
            orgs_by_user: RwLock::new(HashMap::new()),
            event_bus: None,
        }
    }

    /// Attach an EventBus.
    pub fn with_event_bus(mut self, bus: Arc<nexora_core::EventBus>) -> Self {
        self.event_bus = Some(bus);
        self
    }

    /// Number of organizations.
    pub fn org_count(&self) -> usize {
        self.orgs.read().len()
    }

    /// Number of teams.
    pub fn team_count(&self) -> usize {
        self.teams.read().len()
    }

    /// Create a new organization. The creator becomes the Owner.
    pub fn create_org(&self, name: &str, slug: &str, tier: OrgTier, owner_id: &str) -> Result<Organization, TenancyError> {
        let mut orgs = self.orgs.write();
        if orgs.values().any(|o| o.slug == slug) {
            return Err(TenancyError::SlugTaken(slug.to_string()));
        }
        let org = Organization::new(name, slug, tier, owner_id);
        let org_id = org.id.clone();
        let owner = owner_id.to_string();
        orgs.insert(org_id.clone(), org.clone());
        drop(orgs);

        // Add owner as a member with Owner role.
        let membership = Membership {
            org_id: org_id.clone(),
            user_id: owner.clone(),
            role: OrgRole::Owner,
            joined_at: OffsetDateTime::now_utc().unix_timestamp_nanos() as i64,
        };
        self.memberships.write().push(membership);
        self.orgs_by_user
            .write()
            .entry(owner)
            .or_default()
            .push(org_id.clone());

        self.emit("org.created", &org_id);
        Ok(org)
    }

    /// Get an organization by ID.
    pub fn get_org(&self, id: &str) -> Option<Organization> {
        self.orgs.read().get(id).cloned()
    }

    /// Get an organization by slug.
    pub fn get_org_by_slug(&self, slug: &str) -> Option<Organization> {
        self.orgs.read().values().find(|o| o.slug == slug).cloned()
    }

    /// List all organizations.
    pub fn list_orgs(&self) -> Vec<Organization> {
        self.orgs.read().values().cloned().collect()
    }

    /// List organizations for a user.
    pub fn list_orgs_for_user(&self, user_id: &str) -> Vec<Organization> {
        let by_user = self.orgs_by_user.read();
        let orgs = self.orgs.read();
        by_user
            .get(user_id)
            .cloned()
            .unwrap_or_default()
            .iter()
            .filter_map(|id| orgs.get(id).cloned())
            .collect()
    }

    /// Add a member to an organization.
    pub fn add_member(&self, org_id: &str, user_id: &str, role: OrgRole) -> Result<Membership, TenancyError> {
        let mut orgs = self.orgs.write();
        let org = orgs.get_mut(org_id).ok_or_else(|| TenancyError::OrgNotFound(org_id.to_string()))?;

        // Check max members.
        let current_count = self.memberships.read().iter().filter(|m| m.org_id == org_id).count();
        if current_count >= org.max_members as usize {
            return Err(TenancyError::MaxMembers(org_id.to_string()));
        }

        // Check if already a member.
        if self.memberships.read().iter().any(|m| m.org_id == org_id && m.user_id == user_id) {
            return Err(TenancyError::AlreadyMember {
                org_id: org_id.to_string(),
                user_id: user_id.to_string(),
            });
        }

        let membership = Membership {
            org_id: org_id.to_string(),
            user_id: user_id.to_string(),
            role,
            joined_at: OffsetDateTime::now_utc().unix_timestamp_nanos() as i64,
        };
        self.memberships.write().push(membership.clone());
        self.orgs_by_user
            .write()
            .entry(user_id.to_string())
            .or_default()
            .push(org_id.to_string());

        self.emit("org.member_added", &format!("{}:{}", org_id, user_id));
        Ok(membership)
    }

    /// Remove a member from an organization.
    pub fn remove_member(&self, org_id: &str, user_id: &str) -> Result<(), TenancyError> {
        let mut memberships = self.memberships.write();
        let pos = memberships
            .iter()
            .position(|m| m.org_id == org_id && m.user_id == user_id)
            .ok_or_else(|| TenancyError::NotMember {
                org_id: org_id.to_string(),
                user_id: user_id.to_string(),
            })?;
        memberships.remove(pos);
        drop(memberships);

        self.orgs_by_user
            .write()
            .entry(user_id.to_string())
            .or_default()
            .retain(|id| id != org_id);

        self.emit("org.member_removed", &format!("{}:{}", org_id, user_id));
        Ok(())
    }

    /// List members of an organization.
    pub fn list_members(&self, org_id: &str) -> Vec<Membership> {
        self.memberships
            .read()
            .iter()
            .filter(|m| m.org_id == org_id)
            .cloned()
            .collect()
    }

    /// Get a user's role in an org.
    pub fn get_role(&self, org_id: &str, user_id: &str) -> Option<OrgRole> {
        self.memberships
            .read()
            .iter()
            .find(|m| m.org_id == org_id && m.user_id == user_id)
            .map(|m| m.role)
    }

    /// Check if a user has at least the given role in an org.
    pub fn has_role(&self, org_id: &str, user_id: &str, required: OrgRole) -> bool {
        match self.get_role(org_id, user_id) {
            Some(role) => role_level(role) >= role_level(required),
            None => false,
        }
    }

    /// Create a team within an org.
    pub fn create_team(&self, org_id: &str, name: &str) -> Result<Team, TenancyError> {
        if !self.orgs.read().contains_key(org_id) {
            return Err(TenancyError::OrgNotFound(org_id.to_string()));
        }
        let team = Team::new(org_id, name);
        let team_id = team.id.clone();
        self.teams.write().insert(team_id.clone(), team.clone());
        self.emit("team.created", &team_id);
        Ok(team)
    }

    /// List teams in an org.
    pub fn list_teams(&self, org_id: &str) -> Vec<Team> {
        self.teams
            .read()
            .values()
            .filter(|t| t.org_id == org_id)
            .cloned()
            .collect()
    }

    /// Add a user to a team.
    pub fn add_team_member(&self, team_id: &str, user_id: &str) -> Result<Team, TenancyError> {
        let mut teams = self.teams.write();
        let team = teams.get_mut(team_id).ok_or_else(|| TenancyError::TeamNotFound(team_id.to_string()))?;
        if !team.member_ids.contains(&user_id.to_string()) {
            team.member_ids.push(user_id.to_string());
        }
        Ok(team.clone())
    }

    /// Remove a user from a team.
    pub fn remove_team_member(&self, team_id: &str, user_id: &str) -> Result<Team, TenancyError> {
        let mut teams = self.teams.write();
        let team = teams.get_mut(team_id).ok_or_else(|| TenancyError::TeamNotFound(team_id.to_string()))?;
        team.member_ids.retain(|id| id != user_id);
        Ok(team.clone())
    }

    fn emit(&self, name: &str, payload: &str) {
        if let Some(bus) = &self.event_bus {
            bus.publish(name, payload.to_string());
        }
    }
}

/// Role hierarchy level (higher = more permissions).
fn role_level(role: OrgRole) -> u8 {
    match role {
        OrgRole::Viewer => 1,
        OrgRole::Billing => 2,
        OrgRole::Member => 3,
        OrgRole::Admin => 4,
        OrgRole::Owner => 5,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn setup() -> TenantManager {
        let bus = Arc::new(nexora_core::EventBus::new());
        TenantManager::new().with_event_bus(bus)
    }

    #[test]
    fn create_org_adds_owner() {
        let mgr = setup();
        let org = mgr.create_org("Acme", "acme", OrgTier::Organization, "u1").unwrap();
        assert_eq!(mgr.org_count(), 1);
        let members = mgr.list_members(&org.id);
        assert_eq!(members.len(), 1);
        assert_eq!(members[0].role, OrgRole::Owner);
        assert_eq!(members[0].user_id, "u1");
    }

    #[test]
    fn duplicate_slug_rejected() {
        let mgr = setup();
        mgr.create_org("Acme", "acme", OrgTier::Team, "u1").unwrap();
        assert!(mgr.create_org("Other", "acme", OrgTier::Team, "u2").is_err());
    }

    #[test]
    fn add_and_remove_member() {
        let mgr = setup();
        let org = mgr.create_org("Acme", "acme", OrgTier::Team, "u1").unwrap();
        mgr.add_member(&org.id, "u2", OrgRole::Member).unwrap();
        assert_eq!(mgr.list_members(&org.id).len(), 2);
        mgr.remove_member(&org.id, "u2").unwrap();
        assert_eq!(mgr.list_members(&org.id).len(), 1);
    }

    #[test]
    fn duplicate_member_rejected() {
        let mgr = setup();
        let org = mgr.create_org("Acme", "acme", OrgTier::Team, "u1").unwrap();
        mgr.add_member(&org.id, "u2", OrgRole::Member).unwrap();
        assert!(mgr.add_member(&org.id, "u2", OrgRole::Admin).is_err());
    }

    #[test]
    fn max_members_enforced() {
        let mgr = setup();
        let org = mgr.create_org("Solo", "solo", OrgTier::Individual, "u1").unwrap();
        // Individual = max 1 member (the owner). Adding another should fail.
        assert!(mgr.add_member(&org.id, "u2", OrgRole::Member).is_err());
    }

    #[test]
    fn list_orgs_for_user() {
        let mgr = setup();
        let org1 = mgr.create_org("Acme", "acme", OrgTier::Team, "u1").unwrap();
        let org2 = mgr.create_org("Beta", "beta", OrgTier::Team, "u1").unwrap();
        let orgs = mgr.list_orgs_for_user("u1");
        assert_eq!(orgs.len(), 2);
    }

    #[test]
    fn role_hierarchy() {
        assert!(role_level(OrgRole::Owner) > role_level(OrgRole::Admin));
        assert!(role_level(OrgRole::Admin) > role_level(OrgRole::Member));
        assert!(role_level(OrgRole::Member) > role_level(OrgRole::Viewer));
    }

    #[test]
    fn has_role_check() {
        let mgr = setup();
        let org = mgr.create_org("Acme", "acme", OrgTier::Team, "u1").unwrap();
        mgr.add_member(&org.id, "u2", OrgRole::Viewer).unwrap();
        assert!(mgr.has_role(&org.id, "u1", OrgRole::Owner));
        assert!(mgr.has_role(&org.id, "u2", OrgRole::Viewer));
        assert!(!mgr.has_role(&org.id, "u2", OrgRole::Admin));
        assert!(!mgr.has_role(&org.id, "u3", OrgRole::Viewer));
    }

    #[test]
    fn team_lifecycle() {
        let mgr = setup();
        let org = mgr.create_org("Acme", "acme", OrgTier::Team, "u1").unwrap();
        let team = mgr.create_team(&org.id, "Dev Team").unwrap();
        assert_eq!(mgr.team_count(), 1);
        mgr.add_team_member(&team.id, "u1").unwrap();
        mgr.add_team_member(&team.id, "u2").unwrap();
        let updated = mgr.list_teams(&org.id);
        assert_eq!(updated[0].member_ids.len(), 2);
        mgr.remove_team_member(&team.id, "u2").unwrap();
        let updated = mgr.list_teams(&org.id);
        assert_eq!(updated[0].member_ids.len(), 1);
    }

    #[test]
    fn get_org_by_slug() {
        let mgr = setup();
        mgr.create_org("Acme Inc", "acme-inc", OrgTier::Organization, "u1").unwrap();
        let found = mgr.get_org_by_slug("acme-inc").unwrap();
        assert_eq!(found.name, "Acme Inc");
        assert!(mgr.get_org_by_slug("nonexistent").is_none());
    }

    #[test]
    fn events_emitted() {
        let bus = Arc::new(nexora_core::EventBus::new());
        let mgr = TenantManager::new().with_event_bus(bus.clone());
        let org = mgr.create_org("Acme", "acme", OrgTier::Team, "u1").unwrap();
        mgr.add_member(&org.id, "u2", OrgRole::Member).unwrap();
        let events = bus.replay_filtered(0, "org.");
        let names: Vec<&str> = events.iter().map(|e| e.name.as_str()).collect();
        assert!(names.contains(&"org.created"));
        assert!(names.contains(&"org.member_added"));
    }
}
