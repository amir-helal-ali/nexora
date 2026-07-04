//! The sandbox runtime itself — wraps wasmtime with strict resource limits
//! and capability-gated host functions.

use crate::capabilities::{Capability, CapabilitySet};
use crate::error::{SandboxError, SandboxResult};
use crate::manifest::{PluginManifest, PluginOutput, MAX_FUEL};
use parking_lot::Mutex;
use std::sync::Arc;
use wasmtime::{Engine, Instance, Linker, Module, Store, StoreLimits, StoreLimitsBuilder};

/// A reusable sandbox. Holds one `wasmtime::Engine` (cheap to clone, expensive
/// to construct, so we build it once and reuse across plugin executions).
#[derive(Clone)]
pub struct Sandbox {
    engine: Engine,
    /// Tracks fuel consumed per plugin invocation. Used for diagnostics.
    last_fuel_used: Arc<Mutex<u64>>,
}

impl Sandbox {
    /// Create a new sandbox with default engine configuration:
    /// - fuel consumption enabled
    /// - cranelift compiler (no JIT caching across instances — security)
    /// - no threading inside the guest
    pub fn new() -> SandboxResult<Self> {
        let mut config = wasmtime::Config::new();
        config.consume_fuel(true);
        config.cranelift_opt_level(wasmtime::OptLevel::Speed);
        config.parallel_compilation(true);

        let engine = Engine::new(&config)
            .map_err(|e| SandboxError::Engine(e.to_string()))?;

        Ok(Self {
            engine,
            last_fuel_used: Arc::new(Mutex::new(0)),
        })
    }

    /// Returns the fuel consumed by the most recent `execute` call.
    pub fn last_fuel_used(&self) -> u64 {
        *self.last_fuel_used.lock()
    }

    /// Compile a manifest's WASM bytes into a `Module`. The compiled module
    /// can be cached and reused across executions of the same plugin.
    pub fn compile(&self, manifest: &PluginManifest) -> SandboxResult<Module> {
        manifest.validate()?;
        Module::from_binary(&self.engine, &manifest.wasm_bytes)
            .map_err(|e| SandboxError::CompilationFailed(e.to_string()))
    }

    /// Execute a plugin. The `entry_point` is the name of a WASM export
    /// (function) that takes no parameters and returns either nothing or
    /// an i32 status code.
    pub fn execute(
        &self,
        manifest: &PluginManifest,
        entry_point: &str,
        _args: &[u8],
    ) -> SandboxResult<PluginOutput> {
        manifest.validate()?;
        if manifest.fuel > MAX_FUEL {
            return Err(SandboxError::InvalidManifest(format!(
                "fuel {0} exceeds hard cap {MAX_FUEL}",
                manifest.fuel
            )));
        }

        let module = self.compile(manifest)?;
        let caps = CapabilitySet::from_iter(manifest.capabilities.iter().copied());

        // Store limits enforce memory cap.
        let limiter = StoreLimitsBuilder::new()
            .memory_size(manifest.memory_bytes)
            .build();
        let mut store: Store<StoreLimits> = Store::new(&self.engine, limiter);
        store.limiter(|s| s as &mut dyn wasmtime::ResourceLimiter);

        // Add initial fuel. Anything beyond this triggers a trap.
        store
            .set_fuel(manifest.fuel)
            .map_err(|e| SandboxError::Engine(e.to_string()))?;

        let linker = self.build_linker(&caps);

        let instance = linker
            .instantiate(&mut store, &module)
            .map_err(|e| SandboxError::InstantiationFailed(e.to_string()))?;

        // Run the entry point.
        let output = self.invoke_entry(&mut store, &instance, entry_point)?;

        // Record fuel consumed.
        let remaining = store.get_fuel().unwrap_or(0);
        let consumed = manifest.fuel.saturating_sub(remaining);
        *self.last_fuel_used.lock() = consumed;

        Ok(output)
    }

    /// Build the host function linker, gated by capabilities.
    fn build_linker(&self, caps: &CapabilitySet) -> Linker<StoreLimits> {
        let mut linker: Linker<StoreLimits> = Linker::new(&self.engine);

        // nexus_log(level: i32, msg_ptr: i32, msg_len: i32) -> i32
        if caps.contains(Capability::Log) {
            let _ = linker.func_wrap("nexus", "log", |_: wasmtime::Caller<'_, StoreLimits>,
             _level: i32,
             _msg_ptr: i32,
             _msg_len: i32| {
                0i32
            });
        }

        // nexus_clock_now() -> i64 (epoch millis)
        if caps.contains(Capability::Clock) {
            let _ = linker.func_wrap("nexus", "clock_now", |_: wasmtime::Caller<'_, StoreLimits>| -> i64 {
                use std::time::{SystemTime, UNIX_EPOCH};
                SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .map(|d| d.as_millis() as i64)
                    .unwrap_or(0)
            });
        }

        // nexus_random_fill(buf_ptr: i32, buf_len: i32) -> i32
        if caps.contains(Capability::Random) {
            let _ = linker.func_wrap("nexus", "random_fill",
             |_: wasmtime::Caller<'_, StoreLimits>,
              _buf_ptr: i32,
              _buf_len: i32| {
                0i32
            });
        }

        linker
    }

    /// Invoke the entry-point function and translate the result.
    fn invoke_entry(
        &self,
        store: &mut Store<StoreLimits>,
        instance: &Instance,
        entry_point: &str,
    ) -> SandboxResult<PluginOutput> {
        let func = instance
            .get_func(&mut *store, entry_point)
            .ok_or_else(|| SandboxError::EntryPointMissing(entry_point.to_string()))?;

        // Check signature: () -> () or () -> i32
        let ty = func.ty(&*store);
        if ty.params().len() != 0 {
            return Err(SandboxError::EntryPointMissing(format!(
                "{entry_point} (must take 0 parameters)"
            )));
        }
        let results: Vec<_> = ty.results().collect();
        if results.is_empty() {
            // () -> ()
            func.call(&mut *store, &[], &mut []).map_err(|e| {
                let msg = e.to_string();
                if msg.contains("fuel") {
                    SandboxError::FuelExhausted { fuel: 0 }
                } else {
                    SandboxError::Trap(msg)
                }
            })?;
            return Ok(PluginOutput::Unit);
        }
        if results.len() == 1 && matches!(results[0], wasmtime::ValType::I32) {
            // () -> i32
            let mut out = [wasmtime::Val::I32(0)];
            func.call(&mut *store, &[], &mut out).map_err(|e| {
                let msg = e.to_string();
                if msg.contains("fuel") {
                    SandboxError::FuelExhausted { fuel: 0 }
                } else {
                    SandboxError::Trap(msg)
                }
            })?;
            let rc = out[0].unwrap_i32();
            if rc == 0 {
                return Ok(PluginOutput::Unit);
            }
            return Err(SandboxError::Trap(format!(
                "plugin returned non-zero status code: {rc}"
            )));
        }

        Err(SandboxError::EntryPointMissing(format!(
            "{entry_point} (signature must be () -> () or () -> i32)"
        )))
    }
}

impl Default for Sandbox {
    fn default() -> Self {
        Self::new().expect("Sandbox::new must succeed with default config")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::manifest::PluginManifest;

    /// Build a minimal valid WASM module exporting `run: () -> ()`.
    fn minimal_wasm_run() -> Vec<u8> {
        // wat:
        //   (module
        //     (func (export "run"))
        //   )
        vec![
            0x00, 0x61, 0x73, 0x6d, // magic: \0asm
            0x01, 0x00, 0x00, 0x00, // version: 1
            // Type section (id=1), 1 entry, func type with 0 params, 0 results
            0x01, 0x04, 0x01, 0x60, 0x00, 0x00,
            // Function section (id=3), 1 entry, type idx 0
            0x03, 0x02, 0x01, 0x00,
            // Export section (id=7), 1 entry
            // name "run" (3 chars), kind=func, idx=0
            0x07, 0x07, 0x01, 0x03, 0x72, 0x75, 0x6e, 0x00, 0x00,
            // Code section (id=10), 1 entry, body = end (0x0b)
            0x0a, 0x04, 0x01, 0x02, 0x00, 0x0b,
        ]
    }

    fn manifest(wasm: Vec<u8>, fuel: u64) -> PluginManifest {
        PluginManifest {
            id: "test".into(),
            version: "1.0.0".into(),
            wasm_bytes: wasm,
            capabilities: vec![Capability::Log, Capability::Clock],
            fuel,
            memory_bytes: 1 * 1024 * 1024,
            timeout_ms: 1_000,
        }
    }

    #[test]
    fn executes_minimal_module() {
        let sb = Sandbox::new().unwrap();
        let m = manifest(minimal_wasm_run(), 1_000);
        let out = sb.execute(&m, "run", &[]).unwrap();
        assert_eq!(out, PluginOutput::Unit);
        assert!(sb.last_fuel_used() < 100);
    }

    #[test]
    fn rejects_missing_entry_point() {
        let sb = Sandbox::new().unwrap();
        let m = manifest(minimal_wasm_run(), 1_000);
        let err = sb.execute(&m, "does_not_exist", &[]).unwrap_err();
        assert!(matches!(err, SandboxError::EntryPointMissing(_)));
    }

    #[test]
    fn rejects_invalid_wasm() {
        let sb = Sandbox::new().unwrap();
        let m = manifest(vec![0x00, 0x01, 0x02], 1_000);
        let err = sb.execute(&m, "run", &[]).unwrap_err();
        assert!(matches!(err, SandboxError::CompilationFailed(_)));
    }

    #[test]
    fn compile_caches_module() {
        let sb = Sandbox::new().unwrap();
        let m = manifest(minimal_wasm_run(), 1_000);
        let _mod1 = sb.compile(&m).unwrap();
        let _mod2 = sb.compile(&m).unwrap();
    }

    #[test]
    fn default_sandbox_works() {
        let sb = Sandbox::default();
        let m = manifest(minimal_wasm_run(), 1_000);
        let _out = sb.execute(&m, "run", &[]).unwrap();
    }

    #[test]
    fn fuel_accounting_records_consumption() {
        let sb = Sandbox::new().unwrap();
        let m = manifest(minimal_wasm_run(), 1_000);
        sb.execute(&m, "run", &[]).unwrap();
        assert!(sb.last_fuel_used() > 0);
    }
}
