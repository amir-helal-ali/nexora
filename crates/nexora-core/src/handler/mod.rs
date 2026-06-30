//! Core NXP Handler — dispatches incoming NXP commands to Core subsystems.
//!
//! See Nexora Engineering Specification, Part 4 (NXP INTEGRATION LAYER).
//! Each service must implement an NXP Handler, Command Processor, Event
//! Publisher, Stream Handler, and Capability Registry. This module is the
//! Core's own handler — it dispatches the protocol-control and core-system
//! opcodes defined in `nxp-core::opcode`.

use crate::events::EventPayload;
use crate::modules::{Module, ModuleManager, ModuleState, ResourceBudget};
use crate::permissions::{Permission, Principal, PrincipalKind};
use crate::plugins::{PluginManager, PluginManifest, PluginResourceLimits};
use crate::registry::ServiceInstance;
use crate::NexoraCore;
use nxp_core::{NxpError, Opcode, error::protocol_codes};
use nxp_payload::{decode, encode, Encoding};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use time::OffsetDateTime;

/// The Core handler. Owns a reference to the Core.
pub struct CoreHandler {
    core: Arc<NexoraCore>,
}

impl std::fmt::Debug for CoreHandler {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CoreHandler")
            .field("core", &self.core)
            .finish()
    }
}

impl CoreHandler {
    /// Construct a new handler around the given Core.
    pub fn new(core: Arc<NexoraCore>) -> Self {
        Self { core }
    }

    /// Returns a reference to the underlying Core.
    pub fn core(&self) -> &NexoraCore {
        &self.core
    }

    /// Dispatch a single NXP command. Returns the response payload bytes
    /// (MessagePack-encoded) on success.
    ///
    /// The caller (transport layer) is responsible for wrapping the response
    /// in an NXP frame.
    pub async fn dispatch(
        &self,
        opcode: Opcode,
        payload: &[u8],
        encoding: Encoding,
    ) -> Result<Vec<u8>, NxpError> {
        match opcode {
            Opcode::Ping => self.handle_ping(payload, encoding),
            Opcode::Pong => self.handle_pong(),
            Opcode::Bye => self.handle_bye(),

            Opcode::RegisterService => self.handle_register_service(payload, encoding),
            Opcode::DiscoverService => self.handle_discover_service(payload, encoding),

            Opcode::SubscribeEvent => self.handle_subscribe_event(payload, encoding),
            Opcode::PublishEvent => self.handle_publish_event(payload, encoding),
            Opcode::ReplayEvents => self.handle_replay_events(payload, encoding),

            Opcode::ExecuteCommand => self.handle_execute_command(payload, encoding),

            // AI opcodes are reserved per Part 11 — never implemented.
            op if op.is_ai_reserved() => Err(NxpError::protocol(
                protocol_codes::UNKNOWN_OPCODE,
                format!("AI opcode {:?} is reserved (Part 11 deferred)", op),
            )),

            // Opcodes not yet wired up.
            _ => Err(NxpError::protocol(
                protocol_codes::UNKNOWN_OPCODE,
                format!("opcode {:?} not yet implemented in Core handler", opcode),
            )),
        }
    }

    // ------------------------------------------------------------------
    // Protocol control opcodes
    // ------------------------------------------------------------------

    fn handle_ping(&self, _payload: &[u8], encoding: Encoding) -> Result<Vec<u8>, NxpError> {
        let resp = PingResponse { pong: true };
        encode(encoding, &resp)
            .map_err(|e| NxpError::protocol(protocol_codes::ENCODE_FAILED, e.to_string()))
    }

    fn handle_pong(&self) -> Result<Vec<u8>, NxpError> {
        Ok(Vec::new())
    }

    fn handle_bye(&self) -> Result<Vec<u8>, NxpError> {
        Ok(Vec::new())
    }

    // ------------------------------------------------------------------
    // Service registry opcodes
    // ------------------------------------------------------------------

    fn handle_register_service(
        &self,
        payload: &[u8],
        encoding: Encoding,
    ) -> Result<Vec<u8>, NxpError> {
        let req: RegisterServiceRequest = decode(encoding, payload)
            .map_err(|e| NxpError::protocol(protocol_codes::DECODE_FAILED, e.to_string()))?;
        let instance = ServiceInstance {
            name: req.name,
            instance_id: req.instance_id,
            addr: req.addr.parse().unwrap_or_else(|_| "0.0.0.0:0".parse().unwrap()),
            capabilities: req.capabilities.unwrap_or_default(),
            region: req.region.unwrap_or_else(|| "unknown".to_string()),
            last_heartbeat: OffsetDateTime::now_utc().unix_timestamp_nanos() as i64,
            healthy: true,
            priority: req.priority.unwrap_or(0),
        };
        self.core
            .registry
            .register(instance)
            .map_err(|e| NxpError::protocol(protocol_codes::DECODE_FAILED, e.to_string()))?;
        let resp = RegisterServiceResponse { ok: true };
        encode(encoding, &resp)
            .map_err(|e| NxpError::protocol(protocol_codes::ENCODE_FAILED, e.to_string()))
    }

    fn handle_discover_service(
        &self,
        payload: &[u8],
        encoding: Encoding,
    ) -> Result<Vec<u8>, NxpError> {
        let req: DiscoverServiceRequest = decode(encoding, payload)
            .map_err(|e| NxpError::protocol(protocol_codes::DECODE_FAILED, e.to_string()))?;
        let instances = self.core.registry.lookup(&req.name);
        let resp = DiscoverServiceResponse { instances };
        encode(encoding, &resp)
            .map_err(|e| NxpError::protocol(protocol_codes::ENCODE_FAILED, e.to_string()))
    }

    // ------------------------------------------------------------------
    // Event bus opcodes
    // ------------------------------------------------------------------

    fn handle_subscribe_event(
        &self,
        _payload: &[u8],
        _encoding: Encoding,
    ) -> Result<Vec<u8>, NxpError> {
        // In the MVP, subscription is handled at the transport layer (the
        // caller holds the long-lived NxpConnection and reads from it).
        // Here we just acknowledge the subscription request.
        let resp = SubscribeEventResponse { ok: true };
        encode(_encoding, &resp)
            .map_err(|e| NxpError::protocol(protocol_codes::ENCODE_FAILED, e.to_string()))
    }

    fn handle_publish_event(
        &self,
        payload: &[u8],
        encoding: Encoding,
    ) -> Result<Vec<u8>, NxpError> {
        let req: PublishEventRequest = decode(encoding, payload)
            .map_err(|e| NxpError::protocol(protocol_codes::DECODE_FAILED, e.to_string()))?;
        let id = self
            .core
            .events
            .publish_event(req.name, EventPayload::Text(req.payload));
        let resp = PublishEventResponse { event_id: id };
        encode(encoding, &resp)
            .map_err(|e| NxpError::protocol(protocol_codes::ENCODE_FAILED, e.to_string()))
    }

    fn handle_replay_events(
        &self,
        payload: &[u8],
        encoding: Encoding,
    ) -> Result<Vec<u8>, NxpError> {
        let req: ReplayEventsRequest = decode(encoding, payload)
            .map_err(|e| NxpError::protocol(protocol_codes::DECODE_FAILED, e.to_string()))?;
        let events = if req.filter.is_empty() {
            self.core.events.replay(req.from_id)
        } else {
            self.core.events.replay_filtered(req.from_id, &req.filter)
        };
        let resp = ReplayEventsResponse { events };
        encode(encoding, &resp)
            .map_err(|e| NxpError::protocol(protocol_codes::ENCODE_FAILED, e.to_string()))
    }

    // ------------------------------------------------------------------
    // Generic command dispatch
    // ------------------------------------------------------------------

    fn handle_execute_command(
        &self,
        payload: &[u8],
        encoding: Encoding,
    ) -> Result<Vec<u8>, NxpError> {
        let req: ExecuteCommandRequest = decode(encoding, payload)
            .map_err(|e| NxpError::protocol(protocol_codes::DECODE_FAILED, e.to_string()))?;

        // Permission check before dispatching.
        if !self.core.permissions.is_allowed(
            &req.principal_id,
            req.permission,
            &req.resource,
        ) {
            return Err(NxpError::authz(
                nxp_core::error::authz_codes::FORBIDDEN,
                format!(
                    "principal {} denied {} on {}",
                    req.principal_id, req.permission, req.resource
                ),
            ));
        }

        // Dispatch based on the command target.
        let resp = match req.command.as_str() {
            "module.install" => self.cmd_module_install(req.args)?,
            "module.enable" => self.cmd_simple_module(req.args, |m, id| m.enable(id))?,
            "module.pause" => self.cmd_simple_module(req.args, |m, id| m.pause(id))?,
            "module.resume" => self.cmd_simple_module(req.args, |m, id| m.resume(id))?,
            "module.uninstall" => self.cmd_simple_module(req.args, |m, id| m.uninstall(id))?,
            "module.list" => self.cmd_module_list()?,
            "plugin.register" => self.cmd_plugin_register(req.args)?,
            "plugin.activate" => self.cmd_simple_plugin(req.args, |p, id| p.activate(id))?,
            "plugin.stop" => self.cmd_simple_plugin(req.args, |p, id| p.stop(id))?,
            "plugin.list" => self.cmd_plugin_list()?,
            "principal.register" => self.cmd_principal_register(req.args)?,
            "principal.list" => self.cmd_principal_list()?,
            _ => {
                return Err(NxpError::protocol(
                    protocol_codes::UNKNOWN_OPCODE,
                    format!("unknown command: {}", req.command),
                ));
            }
        };

        encode(encoding, &resp)
            .map_err(|e| NxpError::protocol(protocol_codes::ENCODE_FAILED, e.to_string()))
    }

    // ---- Command implementations ----

    fn cmd_module_install(&self, args: ExecuteCommandArgs) -> Result<ExecuteCommandResponse, NxpError> {
        let module = Module {
            id: args.get_string("id").ok_or_else(|| {
                NxpError::protocol(protocol_codes::DECODE_FAILED, "missing id")
            })?,
            name: args.get_string("name").unwrap_or_default(),
            version: args.get_string("version").unwrap_or_else(|| "0.1.0".to_string()),
            state: ModuleState::Installed,
            owner: args.get_string("owner").unwrap_or_default(),
            capabilities: args.get_string_list("capabilities").unwrap_or_default(),
            resource_budget: ResourceBudget::default(),
            installed_at: 0,
            last_transition: 0,
            transition_count: 0,
        };
        self.core
            .modules
            .install(module)
            .map_err(|e| NxpError::protocol(protocol_codes::DECODE_FAILED, e.to_string()))?;
        Ok(ExecuteCommandResponse::new(true, "module installed"))
    }

    fn cmd_simple_module(
        &self,
        args: ExecuteCommandArgs,
        op: fn(&ModuleManager, &str) -> Result<(), crate::modules::ModuleError>,
    ) -> Result<ExecuteCommandResponse, NxpError> {
        let id = args.get_string("id").ok_or_else(|| {
            NxpError::protocol(protocol_codes::DECODE_FAILED, "missing id")
        })?;
        op(&self.core.modules, &id)
            .map_err(|e| NxpError::protocol(protocol_codes::DECODE_FAILED, e.to_string()))?;
        Ok(ExecuteCommandResponse::new(true, format!("module {} transition complete", id)))
    }

    fn cmd_module_list(&self) -> Result<ExecuteCommandResponse, NxpError> {
        let modules = self.core.modules.list();
        let serialized = serde_json::to_value(&modules)
            .map_err(|e| NxpError::protocol(protocol_codes::ENCODE_FAILED, e.to_string()))?;
        Ok(ExecuteCommandResponse::new(true, format!("{} modules", modules.len()))
            .with_data(serialized))
    }

    fn cmd_plugin_register(&self, args: ExecuteCommandArgs) -> Result<ExecuteCommandResponse, NxpError> {
        let manifest = PluginManifest {
            id: args.get_string("id").ok_or_else(|| {
                NxpError::protocol(protocol_codes::DECODE_FAILED, "missing id")
            })?,
            name: args.get_string("name").unwrap_or_default(),
            version: args.get_string("version").unwrap_or_else(|| "0.1.0".to_string()),
            owner: args.get_string("owner").unwrap_or_default(),
            signer_public_key: args.get_string("signer").unwrap_or_else(|| "00".repeat(32)),
            capabilities: args.get_string_list("capabilities").unwrap_or_default(),
            resource_limits: PluginResourceLimits::default(),
            extends_module: args.get_string("extends").unwrap_or_default(),
            signature: args.get_string("signature").unwrap_or_else(|| "00".repeat(64)),
        };
        self.core
            .plugins
            .register(manifest)
            .map_err(|e| NxpError::protocol(protocol_codes::DECODE_FAILED, e.to_string()))?;
        Ok(ExecuteCommandResponse::new(true, "plugin registered"))
    }

    fn cmd_simple_plugin(
        &self,
        args: ExecuteCommandArgs,
        op: fn(&PluginManager, &str) -> Result<(), crate::plugins::PluginError>,
    ) -> Result<ExecuteCommandResponse, NxpError> {
        let id = args.get_string("id").ok_or_else(|| {
            NxpError::protocol(protocol_codes::DECODE_FAILED, "missing id")
        })?;
        op(&self.core.plugins, &id)
            .map_err(|e| NxpError::protocol(protocol_codes::DECODE_FAILED, e.to_string()))?;
        Ok(ExecuteCommandResponse::new(true, format!("plugin {} transition complete", id)))
    }

    fn cmd_plugin_list(&self) -> Result<ExecuteCommandResponse, NxpError> {
        let plugins = self.core.plugins.list();
        Ok(ExecuteCommandResponse::new(true, format!("{} plugins", plugins.len())))
    }

    fn cmd_principal_register(&self, args: ExecuteCommandArgs) -> Result<ExecuteCommandResponse, NxpError> {
        let id = args.get_string("id").ok_or_else(|| {
            NxpError::protocol(protocol_codes::DECODE_FAILED, "missing id")
        })?;
        let kind = match args.get_string("kind").as_deref() {
            Some("service") => PrincipalKind::Service,
            Some("plugin") => PrincipalKind::Plugin,
            Some("ai_agent") => PrincipalKind::AiAgent,
            _ => PrincipalKind::User,
        };
        let principal = Principal {
            id: id.clone(),
            name: args.get_string("name").unwrap_or_else(|| id.clone()),
            kind,
            roles: args
                .get_string_list("roles")
                .unwrap_or_default()
                .into_iter()
                .collect(),
        };
        self.core.permissions.register_principal(principal);
        Ok(ExecuteCommandResponse::new(true, format!("principal {} registered", id)))
    }

    fn cmd_principal_list(&self) -> Result<ExecuteCommandResponse, NxpError> {
        let principals = self.core.permissions.list_principals();
        Ok(ExecuteCommandResponse::new(true, format!("{} principals", principals.len())))
    }
}

// ----------------------------------------------------------------------
// Request / response types
// ----------------------------------------------------------------------

#[derive(Debug, Serialize, Deserialize)]
struct PingResponse {
    pong: bool,
}

#[derive(Debug, Serialize, Deserialize)]
struct RegisterServiceRequest {
    name: String,
    instance_id: String,
    addr: String,
    capabilities: Option<Vec<String>>,
    region: Option<String>,
    priority: Option<u32>,
}

#[derive(Debug, Serialize, Deserialize)]
struct RegisterServiceResponse {
    ok: bool,
}

#[derive(Debug, Serialize, Deserialize)]
struct DiscoverServiceRequest {
    name: String,
}

#[derive(Debug, Serialize, Deserialize)]
struct DiscoverServiceResponse {
    instances: Vec<ServiceInstance>,
}

#[derive(Debug, Serialize, Deserialize)]
struct SubscribeEventResponse {
    ok: bool,
}

#[derive(Debug, Serialize, Deserialize)]
struct PublishEventRequest {
    name: String,
    payload: String,
}

#[derive(Debug, Serialize, Deserialize)]
struct PublishEventResponse {
    event_id: u64,
}

#[derive(Debug, Serialize, Deserialize)]
struct ReplayEventsRequest {
    from_id: u64,
    #[serde(default)]
    filter: String,
}

#[derive(Debug, Serialize, Deserialize)]
struct ReplayEventsResponse {
    events: Vec<crate::events::Event>,
}

#[derive(Debug, Serialize, Deserialize)]
struct ExecuteCommandRequest {
    principal_id: String,
    command: String,
    resource: String,
    permission: Permission,
    #[serde(default)]
    args: ExecuteCommandArgs,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
struct ExecuteCommandArgs {
    #[serde(default)]
    fields: HashMap<String, String>,
    #[serde(default)]
    lists: HashMap<String, Vec<String>>,
}

impl ExecuteCommandArgs {
    fn get_string(&self, key: &str) -> Option<String> {
        self.fields.get(key).cloned()
    }
    fn get_string_list(&self, key: &str) -> Option<Vec<String>> {
        self.lists.get(key).cloned()
    }
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct ExecuteCommandResponse {
    ok: bool,
    message: String,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    data: Option<serde_json::Value>,
}

impl ExecuteCommandResponse {
    fn new(ok: bool, message: impl Into<String>) -> Self {
        Self {
            ok,
            message: message.into(),
            data: None,
        }
    }
    fn with_data(mut self, data: serde_json::Value) -> Self {
        self.data = Some(data);
        self
    }
}

// We need serde_json here for the data field.
// It's a small dep that's already pulled in transitively by tracing-subscriber.
use std::collections::HashMap;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::permissions::{Grant, Role};
    use nxp_payload::Encoding;

    fn sample_handler() -> CoreHandler {
        let core = Arc::new(NexoraCore::new());
        // Register a viewer role + a principal with it.
        core.permissions.register_role(Role {
            id: "admin".into(),
            description: "admin".into(),
            grants: vec![Grant {
                permission: Permission::Execute,
                resource: "command:*".into(),
            }],
        });
        core.permissions.register_principal(Principal {
            id: "u1".into(),
            name: "Alice".into(),
            kind: PrincipalKind::User,
            roles: vec!["admin".to_string()].into_iter().collect(),
        });
        CoreHandler::new(core)
    }

    #[tokio::test]
    async fn ping_returns_pong() {
        let h = sample_handler();
        let resp = h.dispatch(Opcode::Ping, &[], Encoding::MessagePack).await.unwrap();
        let parsed: PingResponse = rmp_serde::from_slice(&resp).unwrap();
        assert!(parsed.pong);
    }

    #[tokio::test]
    async fn ai_opcodes_are_rejected() {
        let h = sample_handler();
        let err = h.dispatch(Opcode::AiRequest, &[], Encoding::MessagePack).await.unwrap_err();
        assert_eq!(err.code, protocol_codes::UNKNOWN_OPCODE);
    }

    #[tokio::test]
    async fn register_and_discover_service() {
        let h = sample_handler();
        let req = RegisterServiceRequest {
            name: "auth".into(),
            instance_id: "auth-1".into(),
            addr: "127.0.0.1:4433".into(),
            capabilities: Some(vec!["nxp/1".into()]),
            region: Some("eu".into()),
            priority: Some(10),
        };
        let payload = rmp_serde::to_vec_named(&req).unwrap();
        h.dispatch(Opcode::RegisterService, &payload, Encoding::MessagePack)
            .await
            .unwrap();

        let req = DiscoverServiceRequest { name: "auth".into() };
        let payload = rmp_serde::to_vec_named(&req).unwrap();
        let resp = h
            .dispatch(Opcode::DiscoverService, &payload, Encoding::MessagePack)
            .await
            .unwrap();
        let parsed: DiscoverServiceResponse = rmp_serde::from_slice(&resp).unwrap();
        assert_eq!(parsed.instances.len(), 1);
        assert_eq!(parsed.instances[0].instance_id, "auth-1");
    }

    #[tokio::test]
    async fn publish_and_replay_event() {
        let h = sample_handler();
        let req = PublishEventRequest {
            name: "test.event".into(),
            payload: "hello".into(),
        };
        let payload = rmp_serde::to_vec_named(&req).unwrap();
        let resp = h
            .dispatch(Opcode::PublishEvent, &payload, Encoding::MessagePack)
            .await
            .unwrap();
        let parsed: PublishEventResponse = rmp_serde::from_slice(&resp).unwrap();
        assert!(parsed.event_id > 0);

        let req = ReplayEventsRequest {
            from_id: 0,
            filter: "".into(),
        };
        let payload = rmp_serde::to_vec_named(&req).unwrap();
        let resp = h
            .dispatch(Opcode::ReplayEvents, &payload, Encoding::MessagePack)
            .await
            .unwrap();
        let parsed: ReplayEventsResponse = rmp_serde::from_slice(&resp).unwrap();
        assert_eq!(parsed.events.len(), 1);
        assert_eq!(parsed.events[0].name, "test.event");
    }

    #[tokio::test]
    async fn execute_command_module_install() {
        let h = sample_handler();
        let mut args = ExecuteCommandArgs::default();
        args.fields.insert("id".into(), "auth".into());
        args.fields.insert("name".into(), "Auth Module".into());
        args.fields.insert("owner".into(), "core".into());
        let req = ExecuteCommandRequest {
            principal_id: "u1".into(),
            command: "module.install".into(),
            resource: "command:module.install".into(),
            permission: Permission::Execute,
            args,
        };
        let payload = rmp_serde::to_vec_named(&req).unwrap();
        let resp = h
            .dispatch(Opcode::ExecuteCommand, &payload, Encoding::MessagePack)
            .await
            .unwrap();
        let parsed: ExecuteCommandResponse = rmp_serde::from_slice(&resp).unwrap();
        assert!(parsed.ok);
        assert_eq!(h.core().modules.module_count(), 1);
    }

    #[tokio::test]
    async fn execute_command_denied_without_permission() {
        let core = Arc::new(NexoraCore::new());
        // Register a principal with NO roles.
        core.permissions.register_principal(Principal {
            id: "u_unpriv".into(),
            name: "Unprivileged".into(),
            kind: PrincipalKind::User,
            roles: std::collections::HashSet::new(),
        });
        let h = CoreHandler::new(core);
        let req = ExecuteCommandRequest {
            principal_id: "u_unpriv".into(),
            command: "module.list".into(),
            resource: "command:module.list".into(),
            permission: Permission::Execute,
            args: ExecuteCommandArgs::default(),
        };
        let payload = rmp_serde::to_vec_named(&req).unwrap();
        let err = h
            .dispatch(Opcode::ExecuteCommand, &payload, Encoding::MessagePack)
            .await
            .unwrap_err();
        assert_eq!(err.scope, nxp_core::ErrorScope::Authz);
    }
}
