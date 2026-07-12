//! QuickJS-backed implementation of loadr's script engine traits.
//!
//! [`JsEngine`] loads an ES module (inline source or file), validates it and
//! records its exports. [`ScriptEngine::instantiate`] creates one fully
//! isolated QuickJS runtime + context per VU, with a memory limit and a
//! wall-clock deadline enforced through the engine's interrupt handler.
//!
//! Scripts get loadr's built-in standard library (`http`, `check`, `sleep`,
//! `group`, metric classes, `crypto`, `encoding`, `__ENV`, `open`,
//! `console`, `session`) as globals and as `loadr`, `loadr/http`,
//! `loadr/metrics`, `loadr/crypto` and `loadr/encoding` modules. The `k6/*`
//! names resolve to the same modules as a migration-compatibility alias. All
//! side effects are routed through the [`ScriptHost`] passed to each call via
//! the host bridge.

mod convert;
mod globals;
mod host_bridge;
mod modules;

use std::collections::HashSet;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use loadr_config::JsConfig;
use loadr_core::error::ScriptError;
use loadr_core::script::{ScriptEngine, ScriptHost, VuScript};
use parking_lot::Mutex;
use rquickjs::function::Args;
use rquickjs::promise::PromiseState;
use rquickjs::{
    Coerced, Context, Ctx, Exception, FromJs, Function, Module, Object, Promise, Runtime, Value,
};

use host_bridge::HostCell;

/// JS prelude evaluated in every context before the user module.
const PRELUDE: &str = include_str!("prelude.js");

/// Global key under which the user module's namespace object is stored.
const EXPORTS_KEY: &str = "__loadr_exports";

/// Default per-call wall-clock limit.
const DEFAULT_TIMEOUT: Duration = Duration::from_secs(10);

/// Default JS heap limit per VU runtime, in MiB.
const DEFAULT_MEMORY_LIMIT_MB: u64 = 64;

// ---------------------------------------------------------------------------
// JsEngine
// ---------------------------------------------------------------------------

/// QuickJS script engine: factory for per-VU [`JsVuScript`] instances.
#[derive(Debug)]
pub struct JsEngine {
    source: String,
    module_name: String,
    timeout: Duration,
    memory_limit: usize,
    functions: HashSet<String>,
}

impl JsEngine {
    /// Load the module source from `config` (inline `script` wins over
    /// `file`, which is resolved relative to `base_dir`), validate that it
    /// compiles and evaluates, and record its exported functions.
    pub fn new(config: &JsConfig, base_dir: &Path) -> Result<JsEngine, ScriptError> {
        let (source, module_name) = load_source(config, base_dir)?;
        let timeout = config
            .timeout
            .map(|d| d.as_duration())
            .filter(|d| !d.is_zero())
            .unwrap_or(DEFAULT_TIMEOUT);
        let memory_limit_mb = match config.memory_limit_mb {
            Some(0) | None => DEFAULT_MEMORY_LIMIT_MB,
            Some(mb) => mb,
        };
        let memory_limit =
            usize::try_from(memory_limit_mb.saturating_mul(1024 * 1024)).unwrap_or(usize::MAX);

        let mut engine = JsEngine {
            source,
            module_name,
            timeout,
            memory_limit,
            functions: HashSet::new(),
        };
        // Validate the module by instantiating once; this compiles the source,
        // runs top-level code and collects the exported function names.
        let probe = engine.build_vu()?;
        engine.functions = probe.functions;
        Ok(engine)
    }

    /// Build a fresh, isolated runtime + context and evaluate the module in it.
    fn build_vu(&self) -> Result<JsVuScript, ScriptError> {
        let runtime = Runtime::new()
            .map_err(|e| ScriptError::Runtime(format!("failed to create JS runtime: {e}")))?;
        runtime.set_memory_limit(self.memory_limit);
        runtime.set_loader(modules::ModuleResolver, modules::ModuleLoader);

        let deadline: Arc<Mutex<Option<Instant>>> = Arc::new(Mutex::new(None));
        let timed_out = Arc::new(AtomicBool::new(false));
        {
            let deadline = Arc::clone(&deadline);
            let timed_out = Arc::clone(&timed_out);
            runtime.set_interrupt_handler(Some(Box::new(move || {
                let limit = *deadline.lock();
                if let Some(limit) = limit {
                    if Instant::now() > limit {
                        timed_out.store(true, Ordering::SeqCst);
                        return true;
                    }
                }
                false
            })));
        }

        let context = Context::full(&runtime)
            .map_err(|e| ScriptError::Runtime(format!("failed to create JS context: {e}")))?;

        let mut vu = JsVuScript {
            _runtime: runtime,
            context,
            host: HostCell::new(),
            deadline,
            timed_out,
            timeout: self.timeout,
            functions: HashSet::new(),
        };
        vu.functions = vu.init_module(&self.source, &self.module_name)?;
        Ok(vu)
    }
}

impl ScriptEngine for JsEngine {
    fn setup(&self, host: &mut dyn ScriptHost) -> Result<serde_json::Value, ScriptError> {
        if !self.functions.contains("setup") {
            return Ok(serde_json::Value::Null);
        }
        let mut vu = self.build_vu()?;
        vu.call_function(host, "setup", &[])
    }

    fn teardown(
        &self,
        host: &mut dyn ScriptHost,
        setup_data: serde_json::Value,
    ) -> Result<(), ScriptError> {
        if !self.functions.contains("teardown") {
            return Ok(());
        }
        let mut vu = self.build_vu()?;
        vu.call_function(host, "teardown", std::slice::from_ref(&setup_data))
            .map(|_| ())
    }

    fn instantiate(&self) -> Result<Box<dyn VuScript>, ScriptError> {
        Ok(Box::new(self.build_vu()?))
    }

    fn has_function(&self, name: &str) -> bool {
        self.functions.contains(name)
    }
}

fn load_source(config: &JsConfig, base_dir: &Path) -> Result<(String, String), ScriptError> {
    if let Some(script) = &config.script {
        return Ok((script.clone(), "test.js".to_string()));
    }
    if let Some(file) = &config.file {
        let path = if file.is_absolute() {
            file.clone()
        } else {
            base_dir.join(file)
        };
        let source = std::fs::read_to_string(&path).map_err(|e| {
            ScriptError::Runtime(format!(
                "failed to read script file {}: {e}",
                path.display()
            ))
        })?;
        let name = file
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| "test.js".to_string());
        return Ok((source, name));
    }
    Err(ScriptError::Runtime(
        "js config requires either `script` (inline source) or `file`".to_string(),
    ))
}

// ---------------------------------------------------------------------------
// JsVuScript
// ---------------------------------------------------------------------------

/// One VU's isolated QuickJS runtime.
pub struct JsVuScript {
    /// Kept alive for the lifetime of the context (owns the interrupt
    /// handler, memory limit and module loader).
    _runtime: Runtime,
    context: Context,
    host: HostCell,
    deadline: Arc<Mutex<Option<Instant>>>,
    timed_out: Arc<AtomicBool>,
    timeout: Duration,
    functions: HashSet<String>,
}

impl JsVuScript {
    /// Register natives, run the prelude, evaluate the user module and stash
    /// its namespace; returns the names of function-valued exports.
    fn init_module(&self, source: &str, name: &str) -> Result<HashSet<String>, ScriptError> {
        self.run_guarded(|ctx| {
            globals::register(&ctx, &self.host).map_err(|e| script_err(&ctx, e))?;
            ctx.eval::<(), _>(PRELUDE)
                .map_err(|e| script_err(&ctx, e))?;

            let declared =
                Module::declare(ctx.clone(), name, source).map_err(|e| script_err(&ctx, e))?;
            let (module, promise) = declared.eval().map_err(|e| script_err(&ctx, e))?;
            match promise.finish::<()>() {
                Ok(()) => {}
                Err(rquickjs::Error::WouldBlock) => {
                    return Err(ScriptError::Runtime(
                        "module evaluation did not complete; top-level await on values that \
                         never resolve is not supported"
                            .to_string(),
                    ));
                }
                Err(e) => return Err(script_err(&ctx, e)),
            }

            let namespace = module.namespace().map_err(|e| script_err(&ctx, e))?;
            ctx.globals()
                .set(EXPORTS_KEY, namespace.clone())
                .map_err(|e| script_err(&ctx, e))?;

            let mut functions = HashSet::new();
            for prop in namespace.props::<String, Value>() {
                let (key, value) = prop.map_err(|e| script_err(&ctx, e))?;
                if value.is_function() {
                    functions.insert(key);
                }
            }
            Ok(functions)
        })
    }

    /// Run `f` against the context with the wall-clock deadline armed.
    /// A deadline hit is reported as [`ScriptError::Timeout`].
    fn run_guarded<R>(
        &self,
        f: impl for<'js> FnOnce(Ctx<'js>) -> Result<R, ScriptError>,
    ) -> Result<R, ScriptError> {
        self.timed_out.store(false, Ordering::SeqCst);
        *self.deadline.lock() = Some(Instant::now() + self.timeout);
        let result = self.context.with(f);
        *self.deadline.lock() = None;
        if result.is_err() && self.timed_out.load(Ordering::SeqCst) {
            return Err(ScriptError::Timeout(self.timeout));
        }
        result
    }
}

impl VuScript for JsVuScript {
    fn call_function(
        &mut self,
        host: &mut dyn ScriptHost,
        name: &str,
        args: &[serde_json::Value],
    ) -> Result<serde_json::Value, ScriptError> {
        if !self.functions.contains(name) {
            return Err(ScriptError::NoSuchFunction(name.to_string()));
        }
        // SAFETY: the guard lives until this function returns, covering the
        // whole script execution; the host borrow outlives it. Script code
        // only runs on this thread while we hold `&mut self`.
        let _guard = unsafe { self.host.attach(host) };
        self.run_guarded(|ctx| {
            let exports: Object = ctx
                .globals()
                .get(EXPORTS_KEY)
                .map_err(|e| script_err(&ctx, e))?;
            let function: Function = exports.get(name).map_err(|e| script_err(&ctx, e))?;

            let mut call_args = Args::new(ctx.clone(), args.len());
            for arg in args {
                let value = convert::json_to_js(&ctx, arg).map_err(|e| script_err(&ctx, e))?;
                call_args.push_arg(value).map_err(|e| script_err(&ctx, e))?;
            }

            let returned: Value = function
                .call_arg(call_args)
                .map_err(|e| script_err(&ctx, e))?;
            let resolved = resolve_value(&ctx, returned)?;
            convert::js_to_json(&ctx, &resolved).map_err(|e| script_err(&ctx, e))
        })
    }

    fn eval(
        &mut self,
        host: &mut dyn ScriptHost,
        code: &str,
    ) -> Result<serde_json::Value, ScriptError> {
        // SAFETY: as in `call_function` — guard scoped to this call, single
        // threaded execution under `&mut self`.
        let _guard = unsafe { self.host.attach(host) };
        self.run_guarded(|ctx| {
            // First try the snippet as a single expression so its value is
            // returned; fall back to statement form on a syntax error.
            let expression = format!(
                "(function(response){{ return (\n{code}\n); }})(globalThis.__loadr_eval_response())"
            );
            let value = match ctx.eval::<Value, _>(expression) {
                Ok(value) => value,
                Err(rquickjs::Error::Exception) => {
                    let caught = ctx.catch();
                    if exception_name(&caught).as_deref() == Some("SyntaxError") {
                        let statements = format!(
                            "(function(response){{\n{code}\n}})(globalThis.__loadr_eval_response())"
                        );
                        ctx.eval::<Value, _>(statements)
                            .map_err(|e| script_err(&ctx, e))?
                    } else {
                        return Err(caught_to_script_err(&ctx, caught));
                    }
                }
                Err(e) => return Err(script_err(&ctx, e)),
            };
            let resolved = resolve_value(&ctx, value)?;
            convert::js_to_json(&ctx, &resolved).map_err(|e| script_err(&ctx, e))
        })
    }

    fn has_function(&self, name: &str) -> bool {
        self.functions.contains(name)
    }
}

// ---------------------------------------------------------------------------
// error mapping & promise resolution
// ---------------------------------------------------------------------------

/// If `value` is a promise, drive the job queue until it settles.
fn resolve_value<'js>(ctx: &Ctx<'js>, value: Value<'js>) -> Result<Value<'js>, ScriptError> {
    if !value.is_promise() {
        return Ok(value);
    }
    let promise = Promise::from_js(ctx, value).map_err(|e| script_err(ctx, e))?;
    while promise.state() == PromiseState::Pending {
        if !ctx.execute_pending_job() {
            return Err(ScriptError::Runtime(
                "async result is still pending: the returned promise never resolved \
                 (only promise chains that complete without external I/O are supported)"
                    .to_string(),
            ));
        }
    }
    match promise.result::<Value>() {
        Some(Ok(value)) => Ok(value),
        Some(Err(e)) => Err(script_err(ctx, e)),
        None => Err(ScriptError::Runtime(
            "promise settled but produced no result".to_string(),
        )),
    }
}

/// Map an rquickjs error (catching any pending exception) to a `ScriptError`.
fn script_err(ctx: &Ctx<'_>, error: rquickjs::Error) -> ScriptError {
    match error {
        rquickjs::Error::Allocation => ScriptError::OutOfMemory,
        rquickjs::Error::Exception => caught_to_script_err(ctx, ctx.catch()),
        e @ (rquickjs::Error::Resolving { .. } | rquickjs::Error::Loading { .. }) => {
            ScriptError::Runtime(e.to_string())
        }
        other => ScriptError::Runtime(other.to_string()),
    }
}

/// Convert an already-caught exception value to a `ScriptError`.
fn caught_to_script_err<'js>(ctx: &Ctx<'js>, caught: Value<'js>) -> ScriptError {
    if let Some(object) = caught.as_object() {
        if let Some(exception) = Exception::from_object(object.clone()) {
            let message = exception.message().unwrap_or_default();
            if message.contains("out of memory") {
                return ScriptError::OutOfMemory;
            }
            let name = object
                .get::<_, Coerced<String>>("name")
                .map(|c| c.0)
                .unwrap_or_else(|_| "Error".to_string());
            let mut text = if message.is_empty() {
                name
            } else {
                format!("{name}: {message}")
            };
            if let Some(stack) = exception.stack() {
                let stack = stack.trim_end();
                if !stack.is_empty() {
                    text.push('\n');
                    text.push_str(stack);
                }
            }
            return ScriptError::Exception(text);
        }
    }
    // A non-Error value was thrown.
    let description = ctx
        .json_stringify(caught.clone())
        .ok()
        .flatten()
        .and_then(|s| s.to_string().ok())
        .unwrap_or_else(|| format!("{:?}", caught.type_of()));
    ScriptError::Exception(format!("uncaught value: {description}"))
}

/// Read the `name` property of a caught exception value, if any.
fn exception_name(caught: &Value<'_>) -> Option<String> {
    caught
        .as_object()
        .and_then(|o| o.get::<_, Coerced<String>>("name").ok())
        .map(|c| c.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn assert_send<T: Send>() {}
    fn assert_send_sync<T: Send + Sync>() {}

    #[test]
    fn engine_is_send_sync_and_vu_is_send() {
        assert_send_sync::<JsEngine>();
        assert_send::<JsVuScript>();
    }

    #[test]
    fn load_source_requires_script_or_file() {
        let config = JsConfig {
            file: None,
            script: None,
            timeout: None,
            memory_limit_mb: None,
        };
        let err = load_source(&config, Path::new(".")).expect_err("must fail");
        assert!(matches!(err, ScriptError::Runtime(_)));
    }

    #[test]
    fn load_source_prefers_inline_script() {
        let config = JsConfig {
            file: Some("missing.js".into()),
            script: Some("export default function () {}".into()),
            timeout: None,
            memory_limit_mb: None,
        };
        let (source, name) = load_source(&config, Path::new(".")).expect("inline source");
        assert_eq!(name, "test.js");
        assert!(source.contains("export default"));
    }

    #[test]
    fn missing_file_is_a_clean_error() {
        let config = JsConfig {
            file: Some("definitely-not-here.js".into()),
            script: None,
            timeout: None,
            memory_limit_mb: None,
        };
        let err = JsEngine::new(&config, Path::new("/nonexistent-dir")).expect_err("must fail");
        let text = err.to_string();
        assert!(
            text.contains("definitely-not-here.js"),
            "error names the file: {text}"
        );
    }

    #[test]
    fn syntax_error_is_reported_at_engine_creation() {
        let config = JsConfig {
            file: None,
            script: Some("export default function ( {".into()),
            timeout: None,
            memory_limit_mb: None,
        };
        let err = JsEngine::new(&config, Path::new(".")).expect_err("syntax error");
        assert!(
            matches!(err, ScriptError::Exception(_) | ScriptError::Runtime(_)),
            "got: {err:?}"
        );
    }
}
