//! Loader and adapters for native dynamic-library plugins.
//!
//! [`NativePlugin::load`] uses `abi_stable`'s library header machinery, which
//! validates the plugin's type layout against the host's before any call is
//! made, then checks our own [`LOADR_PLUGIN_ABI_VERSION`]. Loaded libraries
//! are intentionally never unloaded (abi_stable leaks them), so handed-out
//! trait objects stay valid for the process lifetime.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use abi_stable::library::lib_header_from_path;
use abi_stable::std_types::{ROption, RResult, RString};
use async_trait::async_trait;
use base64::Engine as _;
use bytes::Bytes;
use serde::{Deserialize, Serialize};

use loadr_core::error::{EngineError, ProtocolError};
use loadr_core::metrics::Sample;
use loadr_core::{
    Output, PreparedRequest, ProtocolHandler, ProtocolResponse, Snapshot, Summary, VuContext,
};

use crate::abi::{
    FfiOutputBox, FfiProtocolBox, FfiServiceBox, PluginModRef, LOADR_PLUGIN_ABI_VERSION,
};
use crate::error::PluginError;
use crate::traits::ServicePlugin;
use crate::PluginInfo;

/// JSON request payload handed to [`crate::abi::FfiProtocol::execute`].
#[derive(Debug, Serialize, Deserialize)]
pub struct FfiRequest {
    pub name: String,
    pub method: String,
    pub url: String,
    pub headers: Vec<(String, String)>,
    /// Request body, base64 encoded.
    pub body_b64: String,
    pub timeout_ms: u64,
    /// `options.plugin` from the prepared request, passed through verbatim.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub options: Option<serde_json::Value>,
    /// Plugin-level configuration (manifest defaults + per-run overrides).
    #[serde(default, skip_serializing_if = "serde_json::Value::is_null")]
    pub config: serde_json::Value,
}

/// JSON response payload returned by [`crate::abi::FfiProtocol::execute`].
#[derive(Debug, Default, Serialize, Deserialize)]
pub struct FfiResponse {
    #[serde(default)]
    pub status: i64,
    #[serde(default)]
    pub status_text: String,
    #[serde(default)]
    pub headers: Vec<(String, String)>,
    /// Response body, base64 encoded.
    #[serde(default)]
    pub body_b64: String,
    #[serde(default)]
    pub duration_ms: f64,
    #[serde(default)]
    pub error: Option<String>,
    #[serde(default)]
    pub extras: serde_json::Value,
}

/// A loaded native plugin library.
pub struct NativePlugin {
    module: PluginModRef,
    info: PluginInfo,
    path: PathBuf,
}

impl std::fmt::Debug for NativePlugin {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("NativePlugin")
            .field("info", &self.info)
            .field("path", &self.path)
            .finish()
    }
}

impl NativePlugin {
    /// Load a plugin dynamic library and validate its ABI.
    pub fn load(path: &Path) -> Result<NativePlugin, PluginError> {
        // Deliberately NOT `RootModule::load_from_file`: that caches the root
        // module in a per-*type* static, so a second, different plugin
        // library would silently resolve to the first one loaded. The header
        // path validates version + layout the same way, per library.
        let header = lib_header_from_path(path).map_err(|e| PluginError::Load {
            path: path.display().to_string(),
            message: e.to_string(),
        })?;
        let module: PluginModRef =
            header
                .init_root_module::<PluginModRef>()
                .map_err(|e| PluginError::Load {
                    path: path.display().to_string(),
                    message: e.to_string(),
                })?;
        let version = module.abi_version();
        if version != LOADR_PLUGIN_ABI_VERSION {
            return Err(PluginError::AbiVersion {
                host: LOADR_PLUGIN_ABI_VERSION,
                plugin: version,
            });
        }
        let info_json = module.info()();
        let info: PluginInfo =
            serde_json::from_str(info_json.as_str()).map_err(|e| PluginError::Load {
                path: path.display().to_string(),
                message: format!("invalid plugin info JSON: {e}"),
            })?;
        tracing::debug!(name = %info.name, kind = %info.kind, path = %path.display(), "loaded native plugin");
        Ok(NativePlugin {
            module,
            info,
            path: path.to_path_buf(),
        })
    }

    pub fn info(&self) -> &PluginInfo {
        &self.info
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Instantiate the plugin's output, wrapped as a `loadr_core::Output`.
    pub fn make_output(
        &self,
        config: serde_json::Value,
    ) -> Result<NativeOutputAdapter, PluginError> {
        match self.module.make_output() {
            ROption::RSome(ctor) => Ok(NativeOutputAdapter::new(ctor(), config)),
            ROption::RNone => Err(self.missing("output")),
        }
    }

    /// Instantiate the plugin's protocol handler.
    pub fn make_protocol(
        &self,
        config: serde_json::Value,
    ) -> Result<NativeProtocolAdapter, PluginError> {
        match self.module.make_protocol() {
            ROption::RSome(ctor) => Ok(NativeProtocolAdapter::new(ctor(), config)),
            ROption::RNone => Err(self.missing("protocol")),
        }
    }

    /// Instantiate the plugin's service.
    pub fn make_service(&self) -> Result<NativeServiceAdapter, PluginError> {
        match self.module.make_service() {
            ROption::RSome(ctor) => Ok(NativeServiceAdapter::new(ctor())),
            ROption::RNone => Err(self.missing("service")),
        }
    }

    fn missing(&self, expected: &str) -> PluginError {
        PluginError::KindMismatch {
            name: self.info.name.clone(),
            expected: expected.to_string(),
            actual: self.info.kind.clone(),
        }
    }
}

/// Bridges an FFI output plugin to `loadr_core::Output`.
pub struct NativeOutputAdapter {
    name: String,
    config: serde_json::Value,
    inner: FfiOutputBox,
}

impl std::fmt::Debug for NativeOutputAdapter {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("NativeOutputAdapter")
            .field("name", &self.name)
            .finish()
    }
}

impl NativeOutputAdapter {
    fn new(inner: FfiOutputBox, config: serde_json::Value) -> Self {
        let name = inner.name().into_string();
        NativeOutputAdapter {
            name,
            config,
            inner,
        }
    }
}

#[async_trait]
impl Output for NativeOutputAdapter {
    fn name(&self) -> &str {
        &self.name
    }

    async fn start(&mut self) -> Result<(), EngineError> {
        match self.inner.start(RString::from(self.config.to_string())) {
            RResult::ROk(()) => Ok(()),
            RResult::RErr(e) => Err(EngineError::Other(format!(
                "plugin output `{}` failed to start: {e}",
                self.name
            ))),
        }
    }

    async fn on_samples(&mut self, samples: &[Sample]) {
        match serde_json::to_string(samples) {
            Ok(json) => self.inner.on_samples(RString::from(json)),
            Err(e) => tracing::warn!(output = %self.name, "cannot serialize samples: {e}"),
        }
    }

    async fn on_snapshot(&mut self, snapshot: &Snapshot) {
        match serde_json::to_string(snapshot) {
            Ok(json) => self.inner.on_snapshot(RString::from(json)),
            Err(e) => tracing::warn!(output = %self.name, "cannot serialize snapshot: {e}"),
        }
    }

    async fn finish(&mut self, summary: &Summary) {
        match serde_json::to_string(summary) {
            Ok(json) => self.inner.finish(RString::from(json)),
            Err(e) => tracing::warn!(output = %self.name, "cannot serialize summary: {e}"),
        }
    }
}

/// Bridges an FFI protocol plugin to `loadr_core::ProtocolHandler`.
pub struct NativeProtocolAdapter {
    name: String,
    config: serde_json::Value,
    inner: Arc<FfiProtocolBox>,
}

impl std::fmt::Debug for NativeProtocolAdapter {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("NativeProtocolAdapter")
            .field("name", &self.name)
            .finish()
    }
}

impl NativeProtocolAdapter {
    fn new(inner: FfiProtocolBox, config: serde_json::Value) -> Self {
        let name = inner.name().into_string();
        NativeProtocolAdapter {
            name,
            config,
            inner: Arc::new(inner),
        }
    }
}

pub(crate) struct OwnedFfiRequest {
    name: String,
    method: String,
    url: String,
    headers: Vec<(String, String)>,
    body: Bytes,
    timeout_ms: u64,
    options: Option<serde_json::Value>,
}

impl OwnedFfiRequest {
    pub(crate) fn from_prepared(request: &PreparedRequest) -> Self {
        OwnedFfiRequest {
            name: request.name.clone(),
            method: request.method.clone(),
            url: request.url.clone(),
            headers: request.headers.clone(),
            body: request.body.clone(),
            timeout_ms: request.timeout.as_millis() as u64,
            options: request.options.plugin.clone(),
        }
    }

    pub(crate) fn into_ffi_request(self, config: serde_json::Value) -> (FfiRequest, String, u64) {
        let url = self.url.clone();
        let bytes_sent = self.body.len() as u64;
        let request = FfiRequest {
            name: self.name,
            method: self.method,
            url: self.url,
            headers: self.headers,
            body_b64: base64::engine::general_purpose::STANDARD.encode(&self.body),
            timeout_ms: self.timeout_ms,
            options: self.options,
            config,
        };
        (request, url, bytes_sent)
    }
}

fn call_v1_protocol(
    plugin_name: &str,
    inner: &FfiProtocolBox,
    request: OwnedFfiRequest,
    config: serde_json::Value,
) -> Result<ProtocolResponse, ProtocolError> {
    let (ffi_request, url, bytes_sent) = request.into_ffi_request(config);
    let request_json = serde_json::to_string(&ffi_request)
        .map_err(|e| ProtocolError::InvalidRequest(format!("cannot encode request: {e}")))?;
    let response_json = inner.execute(RString::from(request_json));
    let ffi = serde_json::from_str(response_json.as_str()).map_err(|e| {
        ProtocolError::Transport(format!(
            "plugin `{plugin_name}` returned invalid response JSON: {e}"
        ))
    })?;
    response_from_ffi(plugin_name, ffi, url, bytes_sent)
}

pub(crate) fn response_from_ffi(
    plugin_name: &str,
    ffi: FfiResponse,
    url: String,
    bytes_sent: u64,
) -> Result<ProtocolResponse, ProtocolError> {
    let body = base64::engine::general_purpose::STANDARD
        .decode(&ffi.body_b64)
        .map_err(|e| {
            ProtocolError::Transport(format!(
                "plugin `{plugin_name}` returned invalid body base64: {e}"
            ))
        })?;
    let mut response = ProtocolResponse {
        status: ffi.status,
        status_text: ffi.status_text,
        headers: ffi.headers,
        bytes_sent,
        bytes_received: body.len() as u64,
        body: Bytes::from(body),
        protocol_version: plugin_name.to_string(),
        error: ffi.error,
        url,
        extras: ffi.extras,
        ..Default::default()
    };
    response.timings.duration_ms = ffi.duration_ms;
    response.timings.waiting_ms = ffi.duration_ms;
    Ok(response)
}

#[async_trait]
impl ProtocolHandler for NativeProtocolAdapter {
    fn name(&self) -> &str {
        &self.name
    }

    async fn execute(
        &self,
        _ctx: &mut VuContext,
        request: &PreparedRequest,
    ) -> Result<ProtocolResponse, ProtocolError> {
        let owned = OwnedFfiRequest::from_prepared(request);
        let plugin_name = self.name.clone();
        let inner = Arc::clone(&self.inner);
        let config = self.config.clone();
        let call_plugin_name = plugin_name.clone();
        tokio::task::spawn_blocking(move || {
            call_v1_protocol(&call_plugin_name, &inner, owned, config)
        })
        .await
        .map_err(|e| {
            ProtocolError::Transport(format!("plugin `{plugin_name}` blocking task failed: {e}"))
        })?
    }
}

/// Bridges an FFI service plugin to the [`ServicePlugin`] trait.
pub struct NativeServiceAdapter {
    name: String,
    inner: FfiServiceBox,
}

impl std::fmt::Debug for NativeServiceAdapter {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("NativeServiceAdapter")
            .field("name", &self.name)
            .finish()
    }
}

impl NativeServiceAdapter {
    fn new(inner: FfiServiceBox) -> Self {
        let name = inner.name().into_string();
        NativeServiceAdapter { name, inner }
    }
}

impl ServicePlugin for NativeServiceAdapter {
    fn name(&self) -> &str {
        &self.name
    }

    fn start(&mut self, config: &serde_json::Value) -> Result<String, PluginError> {
        match self.inner.start(RString::from(config.to_string())) {
            RResult::ROk(addr) => Ok(addr.into_string()),
            RResult::RErr(e) => Err(PluginError::Call(format!(
                "service `{}` failed to start: {e}",
                self.name
            ))),
        }
    }

    fn stop(&mut self) {
        self.inner.stop();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;
    use std::time::Duration;

    use loadr_core::metrics::{MetricRegistry, MetricsBus, Tags};
    use loadr_core::vu::RunContext;
    use loadr_core::RequestOptions;

    use crate::abi::{FfiProtocol, FfiProtocol_TO};

    struct SlowProtocol;

    impl FfiProtocol for SlowProtocol {
        fn name(&self) -> RString {
            RString::from("slow")
        }

        fn execute(&self, _request_json: RString) -> RString {
            std::thread::sleep(Duration::from_millis(300));
            RString::from(r#"{"status":200,"status_text":"OK","body_b64":"","duration_ms":300}"#)
        }
    }

    fn minimal_vu() -> VuContext {
        let (bus, _rx) = MetricsBus::new();
        let run = Arc::new(RunContext {
            variables: serde_json::Map::new(),
            secrets: HashMap::new(),
            env: HashMap::new(),
            data: Default::default(),
            registry: Arc::new(MetricRegistry::with_builtins()),
            base_dir: ".".into(),
            setup_data: parking_lot::RwLock::new(serde_json::Value::Null),
        });
        VuContext::new(1, Arc::from("test"), Arc::new(Tags::new()), bus, run, true)
    }

    fn request() -> PreparedRequest {
        PreparedRequest {
            name: "slow".into(),
            protocol: "slow".into(),
            method: "GET".into(),
            url: "slow://local".into(),
            headers: Vec::new(),
            body: Bytes::new(),
            timeout: Duration::from_secs(5),
            follow_redirects: false,
            max_redirects: 0,
            options: RequestOptions::default(),
        }
    }

    #[tokio::test(flavor = "current_thread")]
    async fn v1_protocol_call_does_not_block_core_runtime_timer() {
        let inner = FfiProtocol_TO::from_value(SlowProtocol, abi_stable::erased_types::TD_Opaque);
        let adapter = NativeProtocolAdapter::new(inner, serde_json::Value::Null);
        let mut vu = minimal_vu();
        let request = request();
        let started = std::time::Instant::now();
        let execute = adapter.execute(&mut vu, &request);
        tokio::pin!(execute);

        tokio::select! {
            _ = tokio::time::sleep(Duration::from_millis(25)) => {
                assert!(
                    started.elapsed() < Duration::from_millis(200),
                    "blocking plugin call stalled the core runtime for {:?}",
                    started.elapsed(),
                );
            }
            result = &mut execute => {
                panic!("slow plugin finished before timer fired: {result:?}");
            }
        }

        let response = execute.await.expect("slow plugin response");
        assert_eq!(response.status, 200);
    }
}
