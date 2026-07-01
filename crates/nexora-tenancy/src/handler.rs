//! Tenancy handler — dispatches tenancy commands via the Gateway.

use crate::types::{OrgRole, OrgTier};
use crate::TenancyService;
use nxp_core::NxpError;
use nxp_core::error::protocol_codes;
use serde_json::Value;
use std::sync::Arc;

/// The Tenancy handler.
#[derive(Clone)]
pub struct TenancyHandler {
    service: Arc<TenancyService>,
}

impl std::fmt::Debug for TenancyHandler {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TenancyHandler")
            .field("service", &self.service)
            .finish()
    }
}

impl TenancyHandler {
    /// Construct a new handler.
    pub fn new(service: Arc<TenancyService>) -> Self {
        Self { service }
    }

    /// Execute a tenancy command.
    pub async fn execute(&self, command: &str, args: &Value) -> Result<Value, NxpError> {
        match command {
            "tenancy.create_org" => self.cmd_create_org(args),
            "tenancy.get_org" => self.cmd_get_org(args),
            "tenancy.list_orgs" => self.cmd_list_orgs(),
            "tenancy.list_my_orgs" => self.cmd_list_my_orgs(args),
            "tenancy.add_member" => self.cmd_add_member(args),
            "tenancy.remove_member" => self.cmd_remove_member(args),
            "tenancy.list_members" => self.cmd_list_members(args),
            "tenancy.create_team" => self.cmd_create_team(args),
            "tenancy.list_teams" => self.cmd_list_teams(args),
            "tenancy.add_team_member" => self.cmd_add_team_member(args),
            "tenancy.remove_team_member" => self.cmd_remove_team_member(args),
            "tenancy.stats" => self.cmd_stats(),
            _ => Err(NxpError::protocol(
                protocol_codes::UNKNOWN_OPCODE,
                format!("unknown tenancy command: {}", command),
            )),
        }
    }

    fn cmd_create_org(&self, args: &Value) -> Result<Value, NxpError> {
        let name = args.get("name").and_then(|v| v.as_str())
            .ok_or_else(|| NxpError::protocol(protocol_codes::DECODE_FAILED, "missing name"))?;
        let slug = args.get("slug").and_then(|v| v.as_str())
            .ok_or_else(|| NxpError::protocol(protocol_codes::DECODE_FAILED, "missing slug"))?;
        let tier_str = args.get("tier").and_then(|v| v.as_str()).unwrap_or("team");
        let owner_id = args.get("owner_id").and_then(|v| v.as_str())
            .ok_or_else(|| NxpError::protocol(protocol_codes::DECODE_FAILED, "missing owner_id"))?;
        let tier = parse_tier(tier_str)
            .ok_or_else(|| NxpError::protocol(protocol_codes::DECODE_FAILED, format!("unknown tier: {}", tier_str)))?;
        let org = self.service.manager.create_org(name, slug, tier, owner_id)
            .map_err(|e| NxpError::protocol(protocol_codes::DECODE_FAILED, e.to_string()))?;
        Ok(serde_json::json!({ "ok": true, "organization": org }))
    }

    fn cmd_get_org(&self, args: &Value) -> Result<Value, NxpError> {
        let id = args.get("id").and_then(|v| v.as_str())
            .ok_or_else(|| NxpError::protocol(protocol_codes::DECODE_FAILED, "missing id"))?;
        let org = self.service.manager.get_org(id)
            .ok_or_else(|| NxpError::protocol(protocol_codes::DECODE_FAILED, "org not found"))?;
        Ok(serde_json::json!({ "ok": true, "organization": org }))
    }

    fn cmd_list_orgs(&self) -> Result<Value, NxpError> {
        let orgs = self.service.manager.list_orgs();
        Ok(serde_json::json!({ "ok": true, "count": orgs.len(), "organizations": orgs }))
    }

    fn cmd_list_my_orgs(&self, args: &Value) -> Result<Value, NxpError> {
        let user_id = args.get("user_id").and_then(|v| v.as_str())
            .ok_or_else(|| NxpError::protocol(protocol_codes::DECODE_FAILED, "missing user_id"))?;
        let orgs = self.service.manager.list_orgs_for_user(user_id);
        Ok(serde_json::json!({ "ok": true, "count": orgs.len(), "organizations": orgs }))
    }

    fn cmd_add_member(&self, args: &Value) -> Result<Value, NxpError> {
        let org_id = args.get("org_id").and_then(|v| v.as_str())
            .ok_or_else(|| NxpError::protocol(protocol_codes::DECODE_FAILED, "missing org_id"))?;
        let user_id = args.get("user_id").and_then(|v| v.as_str())
            .ok_or_else(|| NxpError::protocol(protocol_codes::DECODE_FAILED, "missing user_id"))?;
        let role_str = args.get("role").and_then(|v| v.as_str()).unwrap_or("member");
        let role = parse_role(role_str)
            .ok_or_else(|| NxpError::protocol(protocol_codes::DECODE_FAILED, format!("unknown role: {}", role_str)))?;
        let m = self.service.manager.add_member(org_id, user_id, role)
            .map_err(|e| NxpError::protocol(protocol_codes::DECODE_FAILED, e.to_string()))?;
        Ok(serde_json::json!({ "ok": true, "membership": m }))
    }

    fn cmd_remove_member(&self, args: &Value) -> Result<Value, NxpError> {
        let org_id = args.get("org_id").and_then(|v| v.as_str())
            .ok_or_else(|| NxpError::protocol(protocol_codes::DECODE_FAILED, "missing org_id"))?;
        let user_id = args.get("user_id").and_then(|v| v.as_str())
            .ok_or_else(|| NxpError::protocol(protocol_codes::DECODE_FAILED, "missing user_id"))?;
        self.service.manager.remove_member(org_id, user_id)
            .map_err(|e| NxpError::protocol(protocol_codes::DECODE_FAILED, e.to_string()))?;
        Ok(serde_json::json!({ "ok": true }))
    }

    fn cmd_list_members(&self, args: &Value) -> Result<Value, NxpError> {
        let org_id = args.get("org_id").and_then(|v| v.as_str())
            .ok_or_else(|| NxpError::protocol(protocol_codes::DECODE_FAILED, "missing org_id"))?;
        let members = self.service.manager.list_members(org_id);
        Ok(serde_json::json!({ "ok": true, "count": members.len(), "members": members }))
    }

    fn cmd_create_team(&self, args: &Value) -> Result<Value, NxpError> {
        let org_id = args.get("org_id").and_then(|v| v.as_str())
            .ok_or_else(|| NxpError::protocol(protocol_codes::DECODE_FAILED, "missing org_id"))?;
        let name = args.get("name").and_then(|v| v.as_str())
            .ok_or_else(|| NxpError::protocol(protocol_codes::DECODE_FAILED, "missing name"))?;
        let team = self.service.manager.create_team(org_id, name)
            .map_err(|e| NxpError::protocol(protocol_codes::DECODE_FAILED, e.to_string()))?;
        Ok(serde_json::json!({ "ok": true, "team": team }))
    }

    fn cmd_list_teams(&self, args: &Value) -> Result<Value, NxpError> {
        let org_id = args.get("org_id").and_then(|v| v.as_str())
            .ok_or_else(|| NxpError::protocol(protocol_codes::DECODE_FAILED, "missing org_id"))?;
        let teams = self.service.manager.list_teams(org_id);
        Ok(serde_json::json!({ "ok": true, "count": teams.len(), "teams": teams }))
    }

    fn cmd_add_team_member(&self, args: &Value) -> Result<Value, NxpError> {
        let team_id = args.get("team_id").and_then(|v| v.as_str())
            .ok_or_else(|| NxpError::protocol(protocol_codes::DECODE_FAILED, "missing team_id"))?;
        let user_id = args.get("user_id").and_then(|v| v.as_str())
            .ok_or_else(|| NxpError::protocol(protocol_codes::DECODE_FAILED, "missing user_id"))?;
        let team = self.service.manager.add_team_member(team_id, user_id)
            .map_err(|e| NxpError::protocol(protocol_codes::DECODE_FAILED, e.to_string()))?;
        Ok(serde_json::json!({ "ok": true, "team": team }))
    }

    fn cmd_remove_team_member(&self, args: &Value) -> Result<Value, NxpError> {
        let team_id = args.get("team_id").and_then(|v| v.as_str())
            .ok_or_else(|| NxpError::protocol(protocol_codes::DECODE_FAILED, "missing team_id"))?;
        let user_id = args.get("user_id").and_then(|v| v.as_str())
            .ok_or_else(|| NxpError::protocol(protocol_codes::DECODE_FAILED, "missing user_id"))?;
        let team = self.service.manager.remove_team_member(team_id, user_id)
            .map_err(|e| NxpError::protocol(protocol_codes::DECODE_FAILED, e.to_string()))?;
        Ok(serde_json::json!({ "ok": true, "team": team }))
    }

    fn cmd_stats(&self) -> Result<Value, NxpError> {
        Ok(serde_json::json!({
            "ok": true,
            "stats": {
                "organizations": self.service.manager.org_count(),
                "teams": self.service.manager.team_count(),
            }
        }))
    }
}

fn parse_tier(s: &str) -> Option<OrgTier> {
    match s {
        "individual" => Some(OrgTier::Individual),
        "team" => Some(OrgTier::Team),
        "organization" => Some(OrgTier::Organization),
        "enterprise" => Some(OrgTier::Enterprise),
        "msp" => Some(OrgTier::Msp),
        _ => None,
    }
}

fn parse_role(s: &str) -> Option<OrgRole> {
    match s {
        "owner" => Some(OrgRole::Owner),
        "admin" => Some(OrgRole::Admin),
        "member" => Some(OrgRole::Member),
        "viewer" => Some(OrgRole::Viewer),
        "billing" => Some(OrgRole::Billing),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use nexora_core::NexoraCore;

    fn setup() -> TenancyHandler {
        let core = Arc::new(NexoraCore::new());
        let svc = Arc::new(TenancyService::new(core));
        TenancyHandler::new(svc)
    }

    #[tokio::test]
    async fn create_and_list() {
        let h = setup();
        h.execute("tenancy.create_org", &serde_json::json!({
            "name": "Acme", "slug": "acme", "tier": "team", "owner_id": "u1"
        })).await.unwrap();
        let resp = h.execute("tenancy.list_orgs", &serde_json::json!({})).await.unwrap();
        assert_eq!(resp["count"], 1);
    }

    #[tokio::test]
    async fn add_and_list_members() {
        let h = setup();
        let resp = h.execute("tenancy.create_org", &serde_json::json!({
            "name": "Acme", "slug": "acme", "tier": "team", "owner_id": "u1"
        })).await.unwrap();
        let org_id = resp["organization"]["id"].as_str().unwrap();
        h.execute("tenancy.add_member", &serde_json::json!({
            "org_id": org_id, "user_id": "u2", "role": "member"
        })).await.unwrap();
        let resp = h.execute("tenancy.list_members", &serde_json::json!({"org_id": org_id})).await.unwrap();
        assert_eq!(resp["count"], 2);
    }

    #[tokio::test]
    async fn stats_work() {
        let h = setup();
        h.execute("tenancy.create_org", &serde_json::json!({
            "name": "Acme", "slug": "acme", "tier": "team", "owner_id": "u1"
        })).await.unwrap();
        let resp = h.execute("tenancy.stats", &serde_json::json!({})).await.unwrap();
        assert_eq!(resp["stats"]["organizations"], 1);
    }

    #[tokio::test]
    async fn unknown_command_rejected() {
        let h = setup();
        let err = h.execute("tenancy.nope", &serde_json::json!({})).await.unwrap_err();
        assert_eq!(err.code, protocol_codes::UNKNOWN_OPCODE);
    }
}
