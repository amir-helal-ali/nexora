//! Tenancy types — organizations, teams, memberships.

use serde::{Deserialize, Serialize};
use std::fmt;
use time::OffsetDateTime;

/// Unique organization ID.
pub type OrganizationId = String;
/// Unique team ID.
pub type TeamId = String;

/// Organization tier (Part 2 Law 23).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OrgTier {
    /// Individual user (personal workspace).
    Individual,
    /// Small team.
    Team,
    /// Organization.
    Organization,
    /// Enterprise (large org with SSO, audit, etc.).
    Enterprise,
    /// Managed Service Provider (manages multiple orgs).
    Msp,
}

impl fmt::Display for OrgTier {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Individual => f.write_str("individual"),
            Self::Team => f.write_str("team"),
            Self::Organization => f.write_str("organization"),
            Self::Enterprise => f.write_str("enterprise"),
            Self::Msp => f.write_str("msp"),
        }
    }
}

/// Role within an organization.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OrgRole {
    /// Full control (owner).
    Owner,
    /// Admin-level access.
    Admin,
    /// Regular member.
    Member,
    /// Read-only viewer.
    Viewer,
    /// Billing-only access.
    Billing,
}

impl fmt::Display for OrgRole {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Owner => f.write_str("owner"),
            Self::Admin => f.write_str("admin"),
            Self::Member => f.write_str("member"),
            Self::Viewer => f.write_str("viewer"),
            Self::Billing => f.write_str("billing"),
        }
    }
}

/// An organization (tenant).
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Organization {
    /// Unique org ID.
    pub id: OrganizationId,
    /// Display name.
    pub name: String,
    /// Slug (URL-friendly, unique).
    pub slug: String,
    /// Tier.
    pub tier: OrgTier,
    /// Owner user ID.
    pub owner_id: String,
    /// Description.
    pub description: String,
    /// Whether the org is active.
    pub active: bool,
    /// When the org was created (unix nanos).
    pub created_at: i64,
    /// Max number of members (tier-dependent).
    pub max_members: u32,
}

impl Organization {
    /// Construct a new organization.
    pub fn new(name: &str, slug: &str, tier: OrgTier, owner_id: &str) -> Self {
        let max = match tier {
            OrgTier::Individual => 1,
            OrgTier::Team => 25,
            OrgTier::Organization => 200,
            OrgTier::Enterprise => 10_000,
            OrgTier::Msp => 100_000,
        };
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            name: name.to_string(),
            slug: slug.to_string(),
            tier,
            owner_id: owner_id.to_string(),
            description: String::new(),
            active: true,
            created_at: OffsetDateTime::now_utc().unix_timestamp_nanos() as i64,
            max_members: max,
        }
    }
}

/// Membership of a user in an organization.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Membership {
    /// Organization ID.
    pub org_id: OrganizationId,
    /// User ID.
    pub user_id: String,
    /// Role in the org.
    pub role: OrgRole,
    /// When the membership was created (unix nanos).
    pub joined_at: i64,
}

/// A team within an organization.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Team {
    /// Unique team ID.
    pub id: TeamId,
    /// Organization ID this team belongs to.
    pub org_id: OrganizationId,
    /// Team name.
    pub name: String,
    /// Team description.
    pub description: String,
    /// Member user IDs.
    pub member_ids: Vec<String>,
    /// When the team was created (unix nanos).
    pub created_at: i64,
}

impl Team {
    /// Construct a new team.
    pub fn new(org_id: &str, name: &str) -> Self {
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            org_id: org_id.to_string(),
            name: name.to_string(),
            description: String::new(),
            member_ids: vec![],
            created_at: OffsetDateTime::now_utc().unix_timestamp_nanos() as i64,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn org_construction() {
        let org = Organization::new("Acme Inc", "acme", OrgTier::Organization, "u1");
        assert_eq!(org.name, "Acme Inc");
        assert_eq!(org.slug, "acme");
        assert_eq!(org.tier, OrgTier::Organization);
        assert_eq!(org.owner_id, "u1");
        assert!(org.active);
        assert_eq!(org.max_members, 200);
    }

    #[test]
    fn tier_max_members() {
        assert_eq!(Organization::new("a", "a", OrgTier::Individual, "u").max_members, 1);
        assert_eq!(Organization::new("a", "a", OrgTier::Team, "u").max_members, 25);
        assert_eq!(Organization::new("a", "a", OrgTier::Enterprise, "u").max_members, 10_000);
        assert_eq!(Organization::new("a", "a", OrgTier::Msp, "u").max_members, 100_000);
    }

    #[test]
    fn team_construction() {
        let team = Team::new("org-1", "Dev Team");
        assert_eq!(team.org_id, "org-1");
        assert_eq!(team.name, "Dev Team");
        assert!(team.member_ids.is_empty());
    }

    #[test]
    fn role_display() {
        assert_eq!(OrgRole::Owner.to_string(), "owner");
        assert_eq!(OrgRole::Viewer.to_string(), "viewer");
    }

    #[test]
    fn tier_display() {
        assert_eq!(OrgTier::Enterprise.to_string(), "enterprise");
        assert_eq!(OrgTier::Msp.to_string(), "msp");
    }
}
