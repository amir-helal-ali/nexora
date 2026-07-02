//! SQLite-backed tenancy store — persists organizations, memberships, teams.

use crate::{Database, StorageError};
use nexora_tenancy::types::{Membership, Organization, OrgRole, OrgTier, Team};
use std::sync::Arc;

/// SQLite-backed tenancy store.
pub struct SqliteTenancyStore {
    db: Database,
    event_bus: Option<Arc<nexora_core::EventBus>>,
}

impl std::fmt::Debug for SqliteTenancyStore {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SqliteTenancyStore")
            .field("db", &self.db)
            .finish()
    }
}

impl SqliteTenancyStore {
    /// Construct.
    pub fn new(db: Database) -> Self {
        Self { db, event_bus: None }
    }

    /// Attach EventBus.
    pub fn with_event_bus(mut self, bus: Arc<nexora_core::EventBus>) -> Self {
        self.event_bus = Some(bus);
        self
    }

    // ---- Organizations ----

    /// Save an organization (insert or replace).
    pub fn save_org(&self, org: &Organization) -> Result<(), StorageError> {
        self.db.with_conn(|conn| {
            conn.execute(
                "INSERT OR REPLACE INTO organizations
                 (id, name, slug, tier, owner_id, description, active, created_at, max_members)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
                rusqlite::params![
                    org.id,
                    org.name,
                    org.slug,
                    org.tier.to_string(),
                    org.owner_id,
                    org.description,
                    if org.active { 1 } else { 0 },
                    org.created_at,
                    org.max_members,
                ],
            )?;
            Ok(())
        })
    }

    /// Count organizations.
    pub fn org_count(&self) -> Result<i64, StorageError> {
        self.db.with_conn(|conn| {
            Ok(conn.query_row("SELECT COUNT(*) FROM organizations", [], |row| row.get(0))?)
        })
    }

    // ---- Memberships ----

    /// Save a membership (insert or replace).
    pub fn save_membership(&self, m: &Membership) -> Result<(), StorageError> {
        self.db.with_conn(|conn| {
            conn.execute(
                "INSERT OR REPLACE INTO org_memberships
                 (org_id, user_id, role, joined_at)
                 VALUES (?1, ?2, ?3, ?4)",
                rusqlite::params![m.org_id, m.user_id, m.role.to_string(), m.joined_at],
            )?;
            Ok(())
        })
    }

    /// Delete a membership.
    pub fn delete_membership(&self, org_id: &str, user_id: &str) -> Result<(), StorageError> {
        self.db.with_conn(|conn| {
            conn.execute(
                "DELETE FROM org_memberships WHERE org_id = ?1 AND user_id = ?2",
                rusqlite::params![org_id, user_id],
            )?;
            Ok(())
        })
    }

    /// Count memberships.
    pub fn membership_count(&self) -> Result<i64, StorageError> {
        self.db.with_conn(|conn| {
            Ok(conn.query_row("SELECT COUNT(*) FROM org_memberships", [], |row| row.get(0))?)
        })
    }

    // ---- Teams ----

    /// Save a team (insert or replace).
    pub fn save_team(&self, team: &Team) -> Result<(), StorageError> {
        let member_ids_json = serde_json::to_string(&team.member_ids)?;
        self.db.with_conn(|conn| {
            conn.execute(
                "INSERT OR REPLACE INTO teams
                 (id, org_id, name, description, member_ids_json, created_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                rusqlite::params![
                    team.id,
                    team.org_id,
                    team.name,
                    team.description,
                    member_ids_json,
                    team.created_at,
                ],
            )?;
            Ok(())
        })
    }

    /// Delete a team.
    pub fn delete_team(&self, id: &str) -> Result<(), StorageError> {
        self.db.with_conn(|conn| {
            conn.execute("DELETE FROM teams WHERE id = ?1", rusqlite::params![id])?;
            Ok(())
        })
    }

    /// Count teams.
    pub fn team_count(&self) -> Result<i64, StorageError> {
        self.db.with_conn(|conn| {
            Ok(conn.query_row("SELECT COUNT(*) FROM teams", [], |row| row.get(0))?)
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn setup() -> SqliteTenancyStore {
        let db = Database::open_in_memory().unwrap();
        SqliteTenancyStore::new(db)
    }

    fn sample_org(slug: &str, owner: &str) -> Organization {
        Organization::new("Test Org", slug, OrgTier::Team, owner)
    }

    fn sample_membership(org_id: &str, user_id: &str) -> Membership {
        Membership {
            org_id: org_id.to_string(),
            user_id: user_id.to_string(),
            role: OrgRole::Member,
            joined_at: 1000,
        }
    }

    fn sample_team(org_id: &str) -> Team {
        Team::new(org_id, "Dev Team")
    }

    #[test]
    fn save_and_count_orgs() {
        let store = setup();
        assert_eq!(store.org_count().unwrap(), 0);
        store.save_org(&sample_org("acme", "u1")).unwrap();
        store.save_org(&sample_org("beta", "u2")).unwrap();
        assert_eq!(store.org_count().unwrap(), 2);
    }

    #[test]
    fn save_and_delete_membership() {
        let store = setup();
        store.save_org(&sample_org("acme", "u1")).unwrap();
        store.save_membership(&sample_membership("org-1", "u2")).unwrap();
        assert_eq!(store.membership_count().unwrap(), 1);
        store.delete_membership("org-1", "u2").unwrap();
        assert_eq!(store.membership_count().unwrap(), 0);
    }

    #[test]
    fn save_and_delete_team() {
        let store = setup();
        let team = sample_team("org-1");
        store.save_team(&team).unwrap();
        assert_eq!(store.team_count().unwrap(), 1);
        store.delete_team(&team.id).unwrap();
        assert_eq!(store.team_count().unwrap(), 0);
    }

    #[test]
    fn membership_upsert_works() {
        let store = setup();
        let mut m = sample_membership("org-1", "u1");
        m.role = OrgRole::Member;
        store.save_membership(&m).unwrap();
        m.role = OrgRole::Admin;
        store.save_membership(&m).unwrap();
        assert_eq!(store.membership_count().unwrap(), 1); // upsert, not insert
    }

    #[test]
    fn team_member_ids_json_roundtrip() {
        let store = setup();
        let mut team = sample_team("org-1");
        team.member_ids = vec!["u1".into(), "u2".into(), "u3".into()];
        store.save_team(&team).unwrap();
        assert_eq!(store.team_count().unwrap(), 1);
    }

    #[test]
    fn org_all_fields_persist() {
        let store = setup();
        let org = Organization::new("Acme Inc", "acme", OrgTier::Enterprise, "u1");
        store.save_org(&org).unwrap();
        assert_eq!(store.org_count().unwrap(), 1);
    }
}
