//! Sandbox error types.

use thiserror::Error;

pub type SandboxResult<T> = Result<T, SandboxError>;

#[derive(Debug, Error)]
pub enum SandboxError {
    #[error("plugin manifest invalid: {0}")]
    InvalidManifest(String),

    #[error("wasm compilation failed: {0}")]
    CompilationFailed(String),

    #[error("wasm instantiation failed: {0}")]
    InstantiationFailed(String),

    #[error("plugin exceeded fuel budget ({fuel} fuel units consumed)")]
    FuelExhausted { fuel: u64 },

    #[error("plugin exceeded memory limit ({requested} bytes > {limit} bytes)")]
    MemoryLimitExceeded { requested: usize, limit: usize },

    #[error("plugin timed out after {0} ms")]
    Timeout(u64),

    #[error("plugin invoked a capability it was not granted: {0:?}")]
    UnauthorizedCapability(crate::capabilities::Capability),

    #[error("wasm trap: {0}")]
    Trap(String),

    #[error("entry point '{0}' not found in module")]
    EntryPointMissing(String),

    #[error("engine error: {0}")]
    Engine(String),
}
