//! NXP opcodes.
//!
//! See RFC §4. Opcodes are 16-bit unsigned integers partitioned into
//! namespaces. Protocol control opcodes (0x0000–0x00FF) are defined here.
//! Application namespaces are populated by the Core's Capability Registry.

use serde::{Deserialize, Serialize};
use std::fmt;

/// NXP opcode. 16-bit unsigned integer.
///
/// ```rust
/// use nxp_core::Opcode;
/// assert_eq!(Opcode::Ping.as_u16(), 0x0003);
/// assert_eq!(Opcode::from_u16(0x0003), Some(Opcode::Ping));
/// ```
#[derive(Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[repr(u16)]
pub enum Opcode {
    // ---- Protocol control (0x0000–0x00FF) ----
    /// Initiate a session.
    Hello = 0x0001,
    /// Server accepts a session.
    HelloAck = 0x0002,
    /// Heartbeat probe.
    Ping = 0x0003,
    /// Heartbeat response.
    Pong = 0x0004,
    /// Graceful session close.
    Bye = 0x0005,
    /// Error response.
    Error = 0x0006,
    /// Explicit acknowledgment.
    Ack = 0x0007,
    /// Resume a previously-issued session token.
    Resume = 0x0008,
    /// Resume accepted.
    ResumeAck = 0x0009,

    // ---- Core system (0x0100–0x0FFF) ----
    /// Login with credentials / OAuth token.
    AuthLogin = 0x0100,
    /// End authenticated session.
    AuthLogout = 0x0101,
    /// Rotate session keys.
    AuthRefresh = 0x0102,
    /// Service self-registers with Core.
    RegisterService = 0x0110,
    /// Lookup service by logical name.
    DiscoverService = 0x0111,
    /// Subscribe to an event stream.
    SubscribeEvent = 0x0112,
    /// Publish an event.
    PublishEvent = 0x0113,
    /// Replay events from an offset.
    ReplayEvents = 0x0114,
    /// Generic command dispatch.
    ExecuteCommand = 0x0120,
    /// Open a long-lived stream.
    StreamOpen = 0x0121,
    /// Close a stream.
    StreamClose = 0x0122,

    // ---- Reserved AI opcodes (DEFERRED — Part 11) ----
    /// Reserved: AI request. NOT implemented.
    AiRequest = 0x8001,
    /// Reserved: AI stream. NOT implemented.
    AiStream = 0x8002,
    /// Reserved: AI context sync. NOT implemented.
    AiContextSync = 0x8003,
    /// Reserved: AI agent exec. NOT implemented.
    AiAgentExec = 0x8004,
    /// Reserved: AI model query. NOT implemented.
    AiModelQuery = 0x8005,
}

impl Opcode {
    /// Convert to raw `u16`.
    #[inline]
    pub const fn as_u16(self) -> u16 {
        self as u16
    }

    /// Convert from raw `u16`. Returns `None` for unknown built-in opcodes.
    /// Application-defined opcodes (0xC000–0xFFFF) are returned as `None`;
    /// callers in the application layer should handle them via the Capability
    /// Registry, not via this enum.
    #[inline]
    pub const fn from_u16(v: u16) -> Option<Self> {
        match v {
            0x0001 => Some(Self::Hello),
            0x0002 => Some(Self::HelloAck),
            0x0003 => Some(Self::Ping),
            0x0004 => Some(Self::Pong),
            0x0005 => Some(Self::Bye),
            0x0006 => Some(Self::Error),
            0x0007 => Some(Self::Ack),
            0x0008 => Some(Self::Resume),
            0x0009 => Some(Self::ResumeAck),

            0x0100 => Some(Self::AuthLogin),
            0x0101 => Some(Self::AuthLogout),
            0x0102 => Some(Self::AuthRefresh),
            0x0110 => Some(Self::RegisterService),
            0x0111 => Some(Self::DiscoverService),
            0x0112 => Some(Self::SubscribeEvent),
            0x0113 => Some(Self::PublishEvent),
            0x0114 => Some(Self::ReplayEvents),
            0x0120 => Some(Self::ExecuteCommand),
            0x0121 => Some(Self::StreamOpen),
            0x0122 => Some(Self::StreamClose),

            0x8001 => Some(Self::AiRequest),
            0x8002 => Some(Self::AiStream),
            0x8003 => Some(Self::AiContextSync),
            0x8004 => Some(Self::AiAgentExec),
            0x8005 => Some(Self::AiModelQuery),

            _ => None,
        }
    }

    /// Returns `true` if this opcode is reserved for the deferred AI layer.
    #[inline]
    pub const fn is_ai_reserved(self) -> bool {
        matches!(
            self,
            Self::AiRequest
                | Self::AiStream
                | Self::AiContextSync
                | Self::AiAgentExec
                | Self::AiModelQuery
        )
    }
}

impl fmt::Debug for Opcode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Opcode(0x{:04X}:{})", self.as_u16(), self.name())
    }
}

impl fmt::Display for Opcode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.name())
    }
}

impl Opcode {
    /// Stable lowercase name used in logs and metrics.
    pub const fn name(self) -> &'static str {
        match self {
            Self::Hello => "hello",
            Self::HelloAck => "hello_ack",
            Self::Ping => "ping",
            Self::Pong => "pong",
            Self::Bye => "bye",
            Self::Error => "error",
            Self::Ack => "ack",
            Self::Resume => "resume",
            Self::ResumeAck => "resume_ack",
            Self::AuthLogin => "auth_login",
            Self::AuthLogout => "auth_logout",
            Self::AuthRefresh => "auth_refresh",
            Self::RegisterService => "register_service",
            Self::DiscoverService => "discover_service",
            Self::SubscribeEvent => "subscribe_event",
            Self::PublishEvent => "publish_event",
            Self::ReplayEvents => "replay_events",
            Self::ExecuteCommand => "execute_command",
            Self::StreamOpen => "stream_open",
            Self::StreamClose => "stream_close",
            Self::AiRequest => "ai_request",
            Self::AiStream => "ai_stream",
            Self::AiContextSync => "ai_context_sync",
            Self::AiAgentExec => "ai_agent_exec",
            Self::AiModelQuery => "ai_model_query",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip_all_builtins() {
        for op in [
            Opcode::Hello,
            Opcode::HelloAck,
            Opcode::Ping,
            Opcode::Pong,
            Opcode::Bye,
            Opcode::Error,
            Opcode::Ack,
            Opcode::Resume,
            Opcode::ResumeAck,
            Opcode::AuthLogin,
            Opcode::AuthLogout,
            Opcode::AuthRefresh,
            Opcode::RegisterService,
            Opcode::DiscoverService,
            Opcode::SubscribeEvent,
            Opcode::PublishEvent,
            Opcode::ReplayEvents,
            Opcode::ExecuteCommand,
            Opcode::StreamOpen,
            Opcode::StreamClose,
            Opcode::AiRequest,
            Opcode::AiStream,
            Opcode::AiContextSync,
            Opcode::AiAgentExec,
            Opcode::AiModelQuery,
        ] {
            assert_eq!(Opcode::from_u16(op.as_u16()), Some(op));
        }
    }

    #[test]
    fn unknown_opcode_returns_none() {
        assert_eq!(Opcode::from_u16(0xFFFF), None);
        assert_eq!(Opcode::from_u16(0x00CC), None);
    }

    #[test]
    fn ai_opcodes_flagged_as_reserved() {
        assert!(Opcode::AiRequest.is_ai_reserved());
        assert!(!Opcode::Ping.is_ai_reserved());
    }
}
