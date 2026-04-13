//! WASM-in-WASM plugin host — executes external command plugins via wasmi.
//!
//! Plugins are WASM modules that export an `execute` function following a
//! JSON-based protocol. They receive `{args, stdin, files}` and return
//! `{stdout, exit-code, writes}`.

use std::cell::RefCell;
use std::collections::BTreeMap;

// Global registry of plugin WASM bytes, keyed by command name.
// Safe because WASM is single-threaded.
thread_local! {
    static REGISTRY: RefCell<BTreeMap<String, Vec<u8>>> = const { RefCell::new(BTreeMap::new()) };
}

/// Register a plugin's WASM bytes under a command name.
pub fn register(name: &str, wasm_bytes: Vec<u8>) {
    REGISTRY.with(|r| {
        r.borrow_mut().insert(name.to_string(), wasm_bytes);
    });
}

/// Check if a plugin is registered for the given command name.
pub fn has_plugin(name: &str) -> bool {
    REGISTRY.with(|r| r.borrow().contains_key(name))
}

/// Result from running a plugin.
pub struct PluginResult {
    pub stdout: String,
    pub exit_code: i32,
    pub writes: BTreeMap<String, String>,
}

/// Run a registered plugin with the given args, stdin, and file contents.
pub fn run(
    name: &str,
    args: &[String],
    stdin: &str,
    files: &BTreeMap<String, String>,
) -> Result<PluginResult, String> {
    let wasm_bytes = REGISTRY.with(|r| r.borrow().get(name).cloned());
    let wasm_bytes = wasm_bytes.ok_or_else(|| format!("plugin not found: {}", name))?;
    run_wasm(&wasm_bytes, args, stdin, files)
}

/// Host state shared with the plugin during execution.
struct HostState {
    /// Input bytes to deliver to the plugin (set before calling execute).
    input: Vec<u8>,
    /// Result bytes received from the plugin (set by send_result).
    result: Vec<u8>,
}

/// Execute a WASM plugin module with wasmi.
pub(crate) fn run_wasm(
    wasm_bytes: &[u8],
    args: &[String],
    stdin: &str,
    files: &BTreeMap<String, String>,
) -> Result<PluginResult, String> {
    use wasmi::*;

    // Build input JSON
    let input_json = serde_json::json!({
        "args": args,
        "stdin": stdin,
        "files": files,
    });
    let input_bytes = serde_json::to_vec(&input_json).unwrap_or_default();

    // Create engine and module
    let engine = Engine::default();
    let module = Module::new(&engine, wasm_bytes).map_err(|e| format!("plugin load: {e}"))?;

    // Create store with host state
    let mut store = Store::new(
        &engine,
        HostState {
            input: input_bytes,
            result: Vec::new(),
        },
    );

    // Build linker with host functions (wasm-minimal-protocol compatible)
    let mut linker = <Linker<HostState>>::new(&engine);

    // Host function: write args to plugin's buffer
    linker
        .func_wrap(
            "typst_env",
            "wasm_minimal_protocol_write_args_to_buffer",
            |mut caller: Caller<'_, HostState>, ptr: i32| {
                let input = caller.data().input.clone();
                let Some(memory) = caller.get_export("memory").and_then(|e| e.into_memory()) else {
                    return;
                };
                let _ = memory.write(&mut caller, ptr as usize, &input);
            },
        )
        .map_err(|e| format!("linker: {e}"))?;

    // Host function: plugin sends result back
    linker
        .func_wrap(
            "typst_env",
            "wasm_minimal_protocol_send_result_to_host",
            |mut caller: Caller<'_, HostState>, ptr: i32, len: i32| {
                let Some(memory) = caller.get_export("memory").and_then(|e| e.into_memory()) else {
                    return;
                };
                let mut buf = vec![0u8; len as usize];
                if memory.read(&caller, ptr as usize, &mut buf).is_ok() {
                    caller.data_mut().result = buf;
                }
            },
        )
        .map_err(|e| format!("linker: {e}"))?;

    // Instantiate the module
    let instance = linker
        .instantiate_and_start(&mut store, &module)
        .map_err(|e| format!("plugin instantiate: {e}"))?;

    // Call the execute function: execute(input_len) -> result_len
    let execute_fn = instance
        .get_typed_func::<i32, i32>(&store, "execute")
        .map_err(|e| format!("plugin missing execute: {e}"))?;

    let input_len = store.data().input.len() as i32;
    let _result_len = execute_fn
        .call(&mut store, input_len)
        .map_err(|e| format!("plugin execute failed: {e}"))?;

    // Parse result JSON
    let result_bytes = std::mem::take(&mut store.data_mut().result);
    if result_bytes.is_empty() {
        return Err("plugin returned no result".to_string());
    }

    let result: serde_json::Value =
        serde_json::from_slice(&result_bytes).map_err(|e| format!("plugin result parse: {e}"))?;

    let stdout = result["stdout"].as_str().unwrap_or("").to_string();
    let exit_code = result["exit-code"].as_i64().unwrap_or(0) as i32;
    let mut writes = BTreeMap::new();
    if let Some(w) = result.get("writes").and_then(|v| v.as_object()) {
        for (k, v) in w {
            if let Some(s) = v.as_str() {
                writes.insert(k.clone(), s.to_string());
            }
        }
    }

    Ok(PluginResult {
        stdout,
        exit_code,
        writes,
    })
}
