//! Marketplace NXP handler — dispatches marketplace opcodes.
//!
//! Per Part 5 §"MARKETPLACE API", the marketplace exposes official APIs:
//! search_packages, install_package, uninstall_package, update_package,
//! validate_package, publish_package, list_installed, get_metrics,
//! manage_subscription.
//!
//! In v0.1 we expose these via the EXECUTE_COMMAND opcode (the Core's
//! generic command dispatch) with `command: "marketplace.*"`.

use crate::install::InstallReport;
use crate::package::PackageManifest;
use crate::store::TrustScore;
use crate::MarketplaceService;
use nxp_core::NxpError;
use nxp_core::error::protocol_codes;
use std::sync::Arc;

/// The Marketplace handler. Owns a reference to the service.
#[derive(Clone)]
pub struct MarketplaceHandler {
    service: Arc<MarketplaceService>,
}

impl std::fmt::Debug for MarketplaceHandler {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("MarketplaceHandler")
            .field("service", &self.service)
            .finish()
    }
}

impl MarketplaceHandler {
    /// Construct a new handler.
    pub fn new(service: Arc<MarketplaceService>) -> Self {
        Self { service }
    }

    /// Returns a reference to the underlying service.
    pub fn service(&self) -> &Arc<MarketplaceService> {
        &self.service
    }

    /// Execute a marketplace command. Returns a JSON-serializable response.
    pub async fn execute(&self, command: &str, args: &serde_json::Value) -> Result<serde_json::Value, NxpError> {
        match command {
            "marketplace.publish" => self.cmd_publish(args),
            "marketplace.search" => self.cmd_search(args),
            "marketplace.list" => self.cmd_list(),
            "marketplace.list_installed" => self.cmd_list_installed(),
            "marketplace.get" => self.cmd_get(args),
            "marketplace.install" => self.cmd_install(args),
            "marketplace.uninstall" => self.cmd_uninstall(args),
            "marketplace.update_trust" => self.cmd_update_trust(args),
            "marketplace.check_updates" => self.cmd_check_updates(),
            "marketplace.update_package" => self.cmd_update_package(args),
            "marketplace.rollback_package" => self.cmd_rollback_package(args),
            "marketplace.set_update_policy" => self.cmd_set_update_policy(args),
            "marketplace.process_auto_updates" => self.cmd_process_auto_updates(),
            _ => Err(NxpError::protocol(
                protocol_codes::UNKNOWN_OPCODE,
                format!("unknown marketplace command: {}", command),
            )),
        }
    }

    fn cmd_publish(&self, args: &serde_json::Value) -> Result<serde_json::Value, NxpError> {
        let manifest: PackageManifest = serde_json::from_value(args.clone())
            .map_err(|e| NxpError::protocol(protocol_codes::DECODE_FAILED, e.to_string()))?;
        let pkg = self
            .service
            .store
            .publish(manifest)
            .map_err(|e| NxpError::protocol(protocol_codes::DECODE_FAILED, e.to_string()))?;
        Ok(serde_json::json!({
            "ok": true,
            "package_id": pkg.manifest.id,
            "version": pkg.manifest.version.to_string(),
            "integrity_hash": pkg.integrity_hash,
        }))
    }

    fn cmd_search(&self, args: &serde_json::Value) -> Result<serde_json::Value, NxpError> {
        let query = args.get("query").and_then(|v| v.as_str()).unwrap_or("");
        let results = self.service.store.search(query);
        Ok(serde_json::json!({
            "ok": true,
            "count": results.len(),
            "packages": results,
        }))
    }

    fn cmd_list(&self) -> Result<serde_json::Value, NxpError> {
        let packages = self.service.store.list();
        Ok(serde_json::json!({
            "ok": true,
            "count": packages.len(),
            "packages": packages,
        }))
    }

    fn cmd_list_installed(&self) -> Result<serde_json::Value, NxpError> {
        let packages = self.service.store.list_installed();
        Ok(serde_json::json!({
            "ok": true,
            "count": packages.len(),
            "packages": packages,
        }))
    }

    fn cmd_get(&self, args: &serde_json::Value) -> Result<serde_json::Value, NxpError> {
        let id = args
            .get("id")
            .and_then(|v| v.as_str())
            .ok_or_else(|| NxpError::protocol(protocol_codes::DECODE_FAILED, "missing id"))?;
        let version = args.get("version").and_then(|v| v.as_str());
        let pkg = if let Some(v) = version {
            let v: crate::version::Version = v
                .parse()
                .map_err(|e: crate::version::ParseVersionError| NxpError::protocol(protocol_codes::DECODE_FAILED, e.to_string()))?;
            self.service.store.get_version(id, &v)
        } else {
            self.service.store.get_latest(id)
        }
        .ok_or_else(|| NxpError::protocol(protocol_codes::DECODE_FAILED, format!("package {} not found", id)))?;
        Ok(serde_json::json!({ "ok": true, "package": pkg }))
    }

    fn cmd_install(&self, args: &serde_json::Value) -> Result<serde_json::Value, NxpError> {
        let id = args
            .get("id")
            .and_then(|v| v.as_str())
            .ok_or_else(|| NxpError::protocol(protocol_codes::DECODE_FAILED, "missing id"))?;
        let version_str = args
            .get("version")
            .and_then(|v| v.as_str())
            .ok_or_else(|| NxpError::protocol(protocol_codes::DECODE_FAILED, "missing version"))?;
        let version: crate::version::Version = version_str
            .parse()
            .map_err(|e: crate::version::ParseVersionError| NxpError::protocol(protocol_codes::DECODE_FAILED, e.to_string()))?;
        let report: InstallReport = self
            .service
            .pipeline
            .run(&self.service.store, id, &version)
            .map_err(|e| NxpError::protocol(protocol_codes::DECODE_FAILED, e.to_string()))?;
        Ok(serde_json::json!({
            "ok": report.success,
            "report": report,
        }))
    }

    fn cmd_uninstall(&self, args: &serde_json::Value) -> Result<serde_json::Value, NxpError> {
        let id = args
            .get("id")
            .and_then(|v| v.as_str())
            .ok_or_else(|| NxpError::protocol(protocol_codes::DECODE_FAILED, "missing id"))?;
        self.service
            .store
            .mark_uninstalled(id)
            .map_err(|e| NxpError::protocol(protocol_codes::DECODE_FAILED, e.to_string()))?;
        Ok(serde_json::json!({ "ok": true }))
    }

    fn cmd_update_trust(&self, args: &serde_json::Value) -> Result<serde_json::Value, NxpError> {
        let id = args
            .get("id")
            .and_then(|v| v.as_str())
            .ok_or_else(|| NxpError::protocol(protocol_codes::DECODE_FAILED, "missing id"))?;
        let version_str = args
            .get("version")
            .and_then(|v| v.as_str())
            .ok_or_else(|| NxpError::protocol(protocol_codes::DECODE_FAILED, "missing version"))?;
        let version: crate::version::Version = version_str
            .parse()
            .map_err(|e: crate::version::ParseVersionError| NxpError::protocol(protocol_codes::DECODE_FAILED, e.to_string()))?;
        let trust: TrustScore = args
            .get("trust")
            .ok_or_else(|| NxpError::protocol(protocol_codes::DECODE_FAILED, "missing trust"))
            .and_then(|v| {
                serde_json::from_value::<TrustScore>(v.clone())
                    .map_err(|e| NxpError::protocol(protocol_codes::DECODE_FAILED, e.to_string()))
            })?;
        self.service
            .store
            .update_trust(id, &version, trust)
            .map_err(|e| NxpError::protocol(protocol_codes::DECODE_FAILED, e.to_string()))?;
        Ok(serde_json::json!({ "ok": true }))
    }

    fn cmd_check_updates(&self) -> Result<serde_json::Value, NxpError> {
        let result = self.service.updates.check_updates(&self.service.store);
        Ok(serde_json::json!({
            "ok": true,
            "updates_available": result.updates.len(),
            "result": result,
        }))
    }

    fn cmd_update_package(&self, args: &serde_json::Value) -> Result<serde_json::Value, NxpError> {
        let id = args
            .get("id")
            .and_then(|v| v.as_str())
            .ok_or_else(|| NxpError::protocol(protocol_codes::DECODE_FAILED, "missing id"))?;
        let report = self
            .service
            .updates
            .update_package(&self.service.store, id)
            .map_err(|e| NxpError::protocol(protocol_codes::DECODE_FAILED, e.to_string()))?;
        Ok(serde_json::json!({
            "ok": report.success,
            "report": report,
        }))
    }

    fn cmd_rollback_package(&self, args: &serde_json::Value) -> Result<serde_json::Value, NxpError> {
        let id = args
            .get("id")
            .and_then(|v| v.as_str())
            .ok_or_else(|| NxpError::protocol(protocol_codes::DECODE_FAILED, "missing id"))?;
        let version_str = args
            .get("version")
            .and_then(|v| v.as_str())
            .ok_or_else(|| NxpError::protocol(protocol_codes::DECODE_FAILED, "missing version"))?;
        let version: crate::version::Version = version_str
            .parse()
            .map_err(|e: crate::version::ParseVersionError| NxpError::protocol(protocol_codes::DECODE_FAILED, e.to_string()))?;
        let result = self
            .service
            .updates
            .rollback(&self.service.store, id, &version)
            .map_err(|e| NxpError::protocol(protocol_codes::DECODE_FAILED, e.to_string()))?;
        Ok(serde_json::json!({
            "ok": result.success,
            "result": result,
        }))
    }

    fn cmd_set_update_policy(&self, args: &serde_json::Value) -> Result<serde_json::Value, NxpError> {
        let id = args
            .get("id")
            .and_then(|v| v.as_str())
            .ok_or_else(|| NxpError::protocol(protocol_codes::DECODE_FAILED, "missing id"))?;
        let policy_str = args
            .get("policy")
            .and_then(|v| v.as_str())
            .ok_or_else(|| NxpError::protocol(protocol_codes::DECODE_FAILED, "missing policy"))?;
        let policy: crate::update::UpdatePolicy = match policy_str {
            "auto" => crate::update::UpdatePolicy::Auto,
            "manual" => crate::update::UpdatePolicy::Manual,
            "disabled" => crate::update::UpdatePolicy::Disabled,
            _ => return Err(NxpError::protocol(protocol_codes::DECODE_FAILED, format!("unknown policy: {}", policy_str))),
        };
        self.service.updates.set_policy(id, policy);
        Ok(serde_json::json!({ "ok": true, "policy": policy_str }))
    }

    fn cmd_process_auto_updates(&self) -> Result<serde_json::Value, NxpError> {
        let results = self.service.updates.process_auto_updates(&self.service.store);
        let summary: Vec<serde_json::Value> = results
            .iter()
            .map(|r| match r {
                Ok(report) => serde_json::json!({
                    "success": true,
                    "package_id": report.package_id,
                    "version": report.version,
                }),
                Err(e) => serde_json::json!({
                    "success": false,
                    "error": e.to_string(),
                }),
            })
            .collect();
        let succeeded = summary.iter().filter(|s| s["success"].as_bool().unwrap_or(false)).count();
        Ok(serde_json::json!({
            "ok": true,
            "processed": results.len(),
            "succeeded": succeeded,
            "results": summary,
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::version::Version;
    use nexora_core::NexoraCore;

    fn setup() -> MarketplaceHandler {
        let core = Arc::new(NexoraCore::new());
        let svc = Arc::new(MarketplaceService::new(core));
        MarketplaceHandler::new(svc)
    }

    fn sample_manifest_args(id: &str) -> serde_json::Value {
        serde_json::json!({
            "id": id,
            "name": format!("{} package", id),
            "version": "1.0.0",
            "package_type": "module",
            "owner_public_key": "00".repeat(32),
            "owner_name": "test",
            "capabilities": ["nxp.command.execute"],
            "resource_limits": { "max_cpu_percent": 25, "max_memory_mb": 256, "max_commands_per_sec": 100 },
            "dependencies": [],
            "nxp_capabilities": ["quic"],
            "core_compatibility": "^0.1.0",
            "billing": { "kind": "free" },
            "visibility": "public",
            "signature": "ff".repeat(64),
            "description": "test",
            "readme": "# test",
            "tags": ["test"],
        })
    }

    #[tokio::test]
    async fn publish_then_search() {
        let h = setup();
        h.execute("marketplace.publish", &sample_manifest_args("com.test.foo"))
            .await
            .unwrap();
        let resp = h.execute("marketplace.search", &serde_json::json!({"query": "foo"}))
            .await
            .unwrap();
        assert_eq!(resp["count"], 1);
    }

    #[tokio::test]
    async fn list_returns_all() {
        let h = setup();
        h.execute("marketplace.publish", &sample_manifest_args("com.test.a"))
            .await
            .unwrap();
        h.execute("marketplace.publish", &sample_manifest_args("com.test.b"))
            .await
            .unwrap();
        let resp = h.execute("marketplace.list", &serde_json::json!({})).await.unwrap();
        assert_eq!(resp["count"], 2);
    }

    #[tokio::test]
    async fn get_returns_package() {
        let h = setup();
        h.execute("marketplace.publish", &sample_manifest_args("com.test.foo"))
            .await
            .unwrap();
        let resp = h
            .execute("marketplace.get", &serde_json::json!({"id": "com.test.foo"}))
            .await
            .unwrap();
        assert_eq!(resp["package"]["manifest"]["id"], "com.test.foo");
    }

    #[tokio::test]
    async fn install_unsigned_fails() {
        let h = setup();
        h.execute("marketplace.publish", &sample_manifest_args("com.test.foo"))
            .await
            .unwrap();
        let err = h
            .execute(
                "marketplace.install",
                &serde_json::json!({"id": "com.test.foo", "version": "1.0.0"}),
            )
            .await
            .unwrap_err();
        assert_eq!(err.scope, nxp_core::ErrorScope::Protocol);
    }

    #[tokio::test]
    async fn unknown_command_rejected() {
        let h = setup();
        let err = h.execute("marketplace.nope", &serde_json::json!({})).await.unwrap_err();
        assert_eq!(err.code, protocol_codes::UNKNOWN_OPCODE);
    }
}
