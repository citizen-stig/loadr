//! Host-side loader for WASM component plugins (extractors and assertions).
//!
//! Components get a WASI context with **no preopened directories, no network,
//! no inherited stdio/env** — they are pure sandboxed functions over bytes.
//! The wasmtime [`Engine`] is process-global and lazily created because
//! engine/compiler construction is expensive; component compilation itself is
//! per-plugin and happens once at load time.

use std::path::Path;

use once_cell::sync::Lazy;
use parking_lot::Mutex;
use wasmtime::component::{Component, Linker};
use wasmtime::{Engine, Store};
use wasmtime_wasi::{ResourceTable, WasiCtx, WasiCtxBuilder, WasiCtxView, WasiView};

use loadr_core::ProtocolResponse;

use crate::error::PluginError;
use crate::traits::{PluginAssertion, PluginExtractor};
use crate::PluginInfo;

mod gen_extractor {
    wasmtime::component::bindgen!({
        path: "wit",
        world: "loadr-plugin",
    });
}

mod gen_assertion {
    wasmtime::component::bindgen!({
        path: "wit",
        world: "loadr-assertion-plugin",
    });
}

mod gen_meta {
    wasmtime::component::bindgen!({
        path: "wit",
        world: "loadr-meta-probe",
    });
}

/// Store data: a locked-down WASI context.
struct WasiHost {
    ctx: WasiCtx,
    table: ResourceTable,
}

impl WasiView for WasiHost {
    fn ctx(&mut self) -> WasiCtxView<'_> {
        WasiCtxView {
            ctx: &mut self.ctx,
            table: &mut self.table,
        }
    }
}

fn engine() -> Result<&'static Engine, PluginError> {
    static ENGINE: Lazy<Result<Engine, String>> = Lazy::new(|| {
        let mut config = wasmtime::Config::new();
        config.wasm_component_model(true);
        Engine::new(&config).map_err(|e| e.to_string())
    });
    ENGINE
        .as_ref()
        .map_err(|e| PluginError::Wasm(format!("cannot create wasmtime engine: {e}")))
}

/// A sandboxed store: WASI present (wasip2 components import it) but with no
/// preopens, no network, no env and no inherited stdio.
fn sandboxed_store(engine: &Engine) -> Store<WasiHost> {
    let ctx = WasiCtxBuilder::new().build();
    Store::new(
        engine,
        WasiHost {
            ctx,
            table: ResourceTable::new(),
        },
    )
}

fn wasi_linker(engine: &Engine) -> Result<Linker<WasiHost>, PluginError> {
    let mut linker: Linker<WasiHost> = Linker::new(engine);
    wasmtime_wasi::p2::add_to_linker_sync(&mut linker)
        .map_err(|e| PluginError::Wasm(format!("cannot link WASI: {e}")))?;
    Ok(linker)
}

fn load_component(engine: &Engine, path: &Path) -> Result<Component, PluginError> {
    Component::from_file(engine, path).map_err(|e| PluginError::Load {
        path: path.display().to_string(),
        message: e.to_string(),
    })
}

/// Probe any loadr plugin component for its `meta.describe()` info without
/// knowing its kind up front.
pub fn probe_info(path: &Path) -> Result<PluginInfo, PluginError> {
    let engine = engine()?;
    let component = load_component(engine, path)?;
    let linker = wasi_linker(engine)?;
    let mut store = sandboxed_store(engine);
    let probe = gen_meta::LoadrMetaProbe::instantiate(&mut store, &component, &linker)
        .map_err(|e| PluginError::Wasm(format!("cannot instantiate {}: {e}", path.display())))?;
    let info = probe
        .loadr_plugin_meta()
        .call_describe(&mut store)
        .map_err(|e| PluginError::Wasm(format!("describe() failed: {e}")))?;
    Ok(PluginInfo {
        name: info.name,
        version: info.version,
        kind: info.kind,
        description: info.description,
        schemes: Vec::new(),
    })
}

struct ExtractorInstance {
    store: Store<WasiHost>,
    bindings: gen_extractor::LoadrPlugin,
}

/// A loaded WASM extractor plugin.
///
/// The component instance lives in a `Store`, which is `Send` but not `Sync`;
/// calls are short, so a mutex around the instance is the simple, safe choice.
pub struct WasmExtractor {
    info: PluginInfo,
    inner: Mutex<ExtractorInstance>,
}

impl std::fmt::Debug for WasmExtractor {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("WasmExtractor")
            .field("info", &self.info)
            .finish()
    }
}

impl WasmExtractor {
    /// Load a `.wasm` component implementing the `loadr-plugin` world.
    pub fn load(path: &Path) -> Result<WasmExtractor, PluginError> {
        let engine = engine()?;
        let component = load_component(engine, path)?;
        let linker = wasi_linker(engine)?;
        let mut store = sandboxed_store(engine);
        let bindings = gen_extractor::LoadrPlugin::instantiate(&mut store, &component, &linker)
            .map_err(|e| {
                PluginError::Wasm(format!("cannot instantiate {}: {e}", path.display()))
            })?;
        let raw = bindings
            .loadr_plugin_meta()
            .call_describe(&mut store)
            .map_err(|e| PluginError::Wasm(format!("describe() failed: {e}")))?;
        let info = PluginInfo {
            name: raw.name,
            version: raw.version,
            kind: raw.kind,
            description: raw.description,
            schemes: Vec::new(),
        };
        if info.kind != "extractor" {
            return Err(PluginError::KindMismatch {
                name: info.name,
                expected: "extractor".to_string(),
                actual: info.kind,
            });
        }
        tracing::debug!(name = %info.name, path = %path.display(), "loaded wasm extractor");
        Ok(WasmExtractor {
            info,
            inner: Mutex::new(ExtractorInstance { store, bindings }),
        })
    }

    pub fn info(&self) -> &PluginInfo {
        &self.info
    }

    /// Raw extraction over bytes/headers with a JSON config string.
    pub fn extract_raw(
        &self,
        body: &[u8],
        headers: &[(String, String)],
        config_json: &str,
    ) -> Result<Option<String>, PluginError> {
        let mut inner = self.inner.lock();
        let ExtractorInstance { store, bindings } = &mut *inner;
        bindings
            .loadr_plugin_extractor()
            .call_extract(store, body, headers, config_json)
            .map_err(|e| PluginError::Call(format!("extract() trapped: {e}")))
    }
}

impl PluginExtractor for WasmExtractor {
    fn name(&self) -> &str {
        &self.info.name
    }

    fn extract(
        &self,
        response: &ProtocolResponse,
        config: &serde_json::Value,
    ) -> Result<Option<String>, String> {
        self.extract_raw(&response.body, &response.headers, &config.to_string())
            .map_err(|e| e.to_string())
    }
}

struct AssertionInstance {
    store: Store<WasiHost>,
    bindings: gen_assertion::LoadrAssertionPlugin,
}

/// A loaded WASM assertion plugin.
pub struct WasmAssertion {
    info: PluginInfo,
    inner: Mutex<AssertionInstance>,
}

impl std::fmt::Debug for WasmAssertion {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("WasmAssertion")
            .field("info", &self.info)
            .finish()
    }
}

impl WasmAssertion {
    /// Load a `.wasm` component implementing the `loadr-assertion-plugin` world.
    pub fn load(path: &Path) -> Result<WasmAssertion, PluginError> {
        let engine = engine()?;
        let component = load_component(engine, path)?;
        let linker = wasi_linker(engine)?;
        let mut store = sandboxed_store(engine);
        let bindings =
            gen_assertion::LoadrAssertionPlugin::instantiate(&mut store, &component, &linker)
                .map_err(|e| {
                    PluginError::Wasm(format!("cannot instantiate {}: {e}", path.display()))
                })?;
        let raw = bindings
            .loadr_plugin_meta()
            .call_describe(&mut store)
            .map_err(|e| PluginError::Wasm(format!("describe() failed: {e}")))?;
        let info = PluginInfo {
            name: raw.name,
            version: raw.version,
            kind: raw.kind,
            description: raw.description,
            schemes: Vec::new(),
        };
        if info.kind != "assertion" {
            return Err(PluginError::KindMismatch {
                name: info.name,
                expected: "assertion".to_string(),
                actual: info.kind,
            });
        }
        tracing::debug!(name = %info.name, path = %path.display(), "loaded wasm assertion");
        Ok(WasmAssertion {
            info,
            inner: Mutex::new(AssertionInstance { store, bindings }),
        })
    }

    pub fn info(&self) -> &PluginInfo {
        &self.info
    }

    /// Raw check over status/bytes/headers with a JSON config string.
    pub fn check_raw(
        &self,
        status: i64,
        body: &[u8],
        headers: &[(String, String)],
        duration_ms: f64,
        config_json: &str,
    ) -> Result<(bool, String), PluginError> {
        let mut inner = self.inner.lock();
        let AssertionInstance { store, bindings } = &mut *inner;
        let verdict = bindings
            .loadr_plugin_assertion()
            .call_check(store, status, body, headers, duration_ms, config_json)
            .map_err(|e| PluginError::Call(format!("check() trapped: {e}")))?;
        Ok((verdict.pass, verdict.detail))
    }
}

impl PluginAssertion for WasmAssertion {
    fn name(&self) -> &str {
        &self.info.name
    }

    fn check(
        &self,
        response: &ProtocolResponse,
        config: &serde_json::Value,
    ) -> Result<(bool, String), String> {
        self.check_raw(
            response.status,
            &response.body,
            &response.headers,
            response.timings.duration_ms,
            &config.to_string(),
        )
        .map_err(|e| e.to_string())
    }
}
