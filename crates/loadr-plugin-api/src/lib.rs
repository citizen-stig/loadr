//! # loadr-plugin-api
//!
//! The plugin system for [loadr](https://loadr.io). Two mechanisms:
//!
//! - **WASM component plugins** (wasmtime + WIT, [`wasm`]) for *extractors*
//!   and *assertions* — pure functions over response bytes, run fully
//!   sandboxed (no filesystem, no network).
//! - **Native dynamic-library plugins** (`abi_stable`, [`abi`]/[`native`])
//!   for *outputs*, *protocols*, and *services* — things that need real I/O.
//!   Payloads cross the FFI boundary as JSON, keeping the ABI small and
//!   stable.
//!
//! Discovery and lifecycle live in [`registry`]: a plugins directory holds
//! one subdirectory per plugin with a `plugin.toml` manifest ([`manifest`])
//! next to the artifact.

pub mod abi;
pub mod error;
pub mod manifest;
pub mod native;
pub mod registry;
pub mod traits;
pub mod wasm;

// Re-exported for the `export_loadr_plugin!` macro and for plugin authors.
pub use abi_stable;

pub use error::PluginError;
pub use manifest::{merge_config, PluginKind, PluginManifest, PluginType};
pub use native::{
    FfiRequest, FfiResponse, NativeOutputAdapter, NativePlugin, NativeProtocolAdapter,
    NativeServiceAdapter,
};
pub use registry::{default_plugins_dir, LoadedPlugin, PluginRegistry, DISABLED_MARKER};
pub use traits::{PluginAssertion, PluginExtractor, ServicePlugin};
pub use wasm::{probe_info, WasmAssertion, WasmExtractor};

/// Identity and kind of a plugin, as reported by the plugin itself
/// (WASM `meta.describe()` / native `info()`).
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct PluginInfo {
    pub name: String,
    pub version: String,
    /// One of `extractor`, `assertion`, `output`, `protocol`, `service`.
    pub kind: String,
    pub description: String,
    /// URL scheme(s) a protocol plugin serves, used when the plugin is loaded
    /// directly by path (no adjacent `plugin.toml`). When a manifest is
    /// present, the manifest's `[plugin].schemes` takes precedence.
    #[serde(default)]
    pub schemes: Vec<String>,
}
