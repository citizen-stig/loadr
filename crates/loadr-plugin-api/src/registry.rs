//! Plugin discovery, loading, enable/disable, and installation.
//!
//! The plugins directory layout is one subdirectory per plugin:
//!
//! ```text
//! ~/.loadr/plugins/
//! ├── upper-boundary/
//! │   ├── plugin.toml
//! │   └── plugin.wasm
//! └── echo-proto/
//!     ├── plugin.toml
//!     ├── disabled          # optional marker: skip this plugin
//!     └── libnative_protocol.so
//! ```
//!
//! Installation is just copying such a directory into the plugins dir
//! ([`PluginRegistry::install_from_dir`]).

use std::path::{Path, PathBuf};
use std::sync::Arc;

use loadr_core::ProtocolHandler;

use crate::error::PluginError;
use crate::manifest::{PluginKind, PluginManifest, PluginType};
use crate::native::NativePlugin;
use crate::traits::{PluginAssertion, PluginExtractor, ServicePlugin};
use crate::wasm::{WasmAssertion, WasmExtractor};

/// Register the URL scheme(s) a loaded protocol plugin serves with the
/// host-global scheme router, so `infer` can route e.g. `mongodb://` to the
/// plugin. A no-op when the plugin declares no schemes.
fn register_protocol_schemes(protocol: &str, schemes: &[String]) {
    if !schemes.is_empty() {
        loadr_core::protocol::register_plugin_schemes(protocol, schemes);
    }
}

/// Marker file that disables a plugin without uninstalling it.
pub const DISABLED_MARKER: &str = "disabled";

/// The default plugins directory: `$LOADR_PLUGINS_DIR` or `~/.loadr/plugins`.
pub fn default_plugins_dir() -> PathBuf {
    if let Some(dir) = std::env::var_os("LOADR_PLUGINS_DIR") {
        return PathBuf::from(dir);
    }
    let home = std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .unwrap_or_else(|| ".".into());
    Path::new(&home).join(".loadr").join("plugins")
}

/// A plugin loaded and bridged to the engine-facing abstraction for its kind.
pub enum LoadedPlugin {
    Extractor(Box<dyn PluginExtractor>),
    Assertion(Box<dyn PluginAssertion>),
    Output(Box<dyn loadr_core::Output>),
    Protocol(Arc<dyn loadr_core::ProtocolHandler>),
    Service(Box<dyn ServicePlugin>),
}

impl LoadedPlugin {
    pub fn kind(&self) -> PluginKind {
        match self {
            LoadedPlugin::Extractor(_) => PluginKind::Extractor,
            LoadedPlugin::Assertion(_) => PluginKind::Assertion,
            LoadedPlugin::Output(_) => PluginKind::Output,
            LoadedPlugin::Protocol(_) => PluginKind::Protocol,
            LoadedPlugin::Service(_) => PluginKind::Service,
        }
    }
}

impl std::fmt::Debug for LoadedPlugin {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "LoadedPlugin({})", self.kind())
    }
}

/// Discovery and loading entry points.
pub struct PluginRegistry;

impl PluginRegistry {
    /// Scan `dir` for plugin installations (subdirectories with a
    /// `plugin.toml`). Returns manifests sorted by name, including disabled
    /// ones (with `enabled = false`). A missing directory yields an empty list.
    pub fn discover(dir: &Path) -> Result<Vec<PluginManifest>, PluginError> {
        let mut out = Vec::new();
        let entries = match std::fs::read_dir(dir) {
            Ok(entries) => entries,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(out),
            Err(e) => return Err(PluginError::io(dir, e)),
        };
        for entry in entries {
            let entry = entry.map_err(|e| PluginError::io(dir, e))?;
            let sub = entry.path();
            if !sub.is_dir() || !sub.join("plugin.toml").is_file() {
                continue;
            }
            match PluginManifest::load(&sub) {
                Ok(manifest) => out.push(manifest),
                Err(e) => {
                    tracing::warn!(dir = %sub.display(), "skipping invalid plugin: {e}");
                }
            }
        }
        out.sort_by(|a, b| a.name.cmp(&b.name));
        Ok(out)
    }

    /// Load a plugin from its manifest using its default config.
    pub fn load(manifest: &PluginManifest) -> Result<LoadedPlugin, PluginError> {
        Self::load_with_config(manifest, &serde_json::Value::Null)
    }

    /// Load a plugin from its manifest with `overrides` merged on top of the
    /// manifest's `[config]` defaults.
    pub fn load_with_config(
        manifest: &PluginManifest,
        overrides: &serde_json::Value,
    ) -> Result<LoadedPlugin, PluginError> {
        let config = manifest.merged_config(overrides);
        match (manifest.plugin_type, manifest.kind) {
            (PluginType::Wasm, PluginKind::Extractor) => Ok(LoadedPlugin::Extractor(Box::new(
                WasmExtractor::load(&manifest.entry)?,
            ))),
            (PluginType::Wasm, PluginKind::Assertion) => Ok(LoadedPlugin::Assertion(Box::new(
                WasmAssertion::load(&manifest.entry)?,
            ))),
            (PluginType::Wasm, kind) => Err(PluginError::Other(format!(
                "wasm plugins cannot provide `{kind}` (only extractor/assertion); \
                 use a native plugin for `{}`",
                manifest.name
            ))),
            (PluginType::Native, kind) => {
                let plugin = NativePlugin::load(&manifest.entry)?;
                match kind {
                    PluginKind::Output => {
                        Ok(LoadedPlugin::Output(Box::new(plugin.make_output(config)?)))
                    }
                    PluginKind::Protocol => {
                        // Manifest `[plugin].schemes` win; fall back to the
                        // plugin's own `info().schemes` when none are declared.
                        let schemes = if manifest.schemes.is_empty() {
                            plugin.info().schemes.clone()
                        } else {
                            manifest.schemes.clone()
                        };
                        let handler = plugin.make_protocol(config)?;
                        register_protocol_schemes(handler.name(), &schemes);
                        Ok(LoadedPlugin::Protocol(Arc::new(handler)))
                    }
                    PluginKind::Service => {
                        Ok(LoadedPlugin::Service(Box::new(plugin.make_service()?)))
                    }
                    other => Err(PluginError::Other(format!(
                        "native plugins cannot provide `{other}` (only output/protocol/service); \
                         use a wasm plugin for `{}`",
                        manifest.name
                    ))),
                }
            }
        }
    }

    /// Resolve and load a `plugins:` entry from a test plan.
    ///
    /// - With `path`: load that artifact directly. If a `plugin.toml` sits
    ///   next to it, it supplies kind/type/defaults; otherwise the kind is
    ///   taken from a `kind` key in `PluginRef.config` (wasm) or from the
    ///   plugin's own `info()` (native).
    /// - Without `path`: resolve by name inside `plugins_dir`. Disabled
    ///   plugins are reported as not found.
    pub fn load_ref(
        plugin_ref: &loadr_config::plan::PluginRef,
        plugins_dir: &Path,
    ) -> Result<LoadedPlugin, PluginError> {
        if let Some(path) = &plugin_ref.path {
            return Self::load_path(plugin_ref, path);
        }
        let manifest = Self::discover(plugins_dir)?
            .into_iter()
            .find(|m| m.name == plugin_ref.name && m.enabled)
            .ok_or_else(|| PluginError::NotFound {
                name: plugin_ref.name.clone(),
                dir: plugins_dir.display().to_string(),
            })?;
        Self::load_with_config(&manifest, &plugin_ref.config)
    }

    fn load_path(
        plugin_ref: &loadr_config::plan::PluginRef,
        path: &Path,
    ) -> Result<LoadedPlugin, PluginError> {
        // Prefer a manifest next to the artifact: it carries kind + defaults.
        if let Some(dir) = path.parent() {
            if dir.join("plugin.toml").is_file() {
                let manifest = PluginManifest::load(dir)?;
                return Self::load_with_config(&manifest, &plugin_ref.config);
            }
        }
        let is_wasm = path
            .extension()
            .map(|e| e.eq_ignore_ascii_case("wasm"))
            .unwrap_or(false);
        if is_wasm {
            // Kind from PluginRef.config, falling back to the component's own
            // describe() metadata.
            let kind = match plugin_ref.config.get("kind").and_then(|v| v.as_str()) {
                Some(k) => k.to_string(),
                None => crate::wasm::probe_info(path)?.kind,
            };
            match PluginKind::parse(&kind) {
                Some(PluginKind::Extractor) => Ok(LoadedPlugin::Extractor(Box::new(
                    WasmExtractor::load(path)?,
                ))),
                Some(PluginKind::Assertion) => Ok(LoadedPlugin::Assertion(Box::new(
                    WasmAssertion::load(path)?,
                ))),
                _ => Err(PluginError::Other(format!(
                    "wasm plugin `{}` reports unsupported kind `{kind}`",
                    plugin_ref.name
                ))),
            }
        } else {
            let plugin = NativePlugin::load(path)?;
            let kind = plugin.info().kind.clone();
            let config = plugin_ref.config.clone();
            match PluginKind::parse(&kind) {
                Some(PluginKind::Output) => {
                    Ok(LoadedPlugin::Output(Box::new(plugin.make_output(config)?)))
                }
                Some(PluginKind::Protocol) => {
                    let schemes = plugin.info().schemes.clone();
                    let handler = plugin.make_protocol(config)?;
                    register_protocol_schemes(handler.name(), &schemes);
                    Ok(LoadedPlugin::Protocol(Arc::new(handler)))
                }
                Some(PluginKind::Service) => {
                    Ok(LoadedPlugin::Service(Box::new(plugin.make_service()?)))
                }
                _ => Err(PluginError::Other(format!(
                    "native plugin `{}` reports unsupported kind `{kind}`",
                    plugin_ref.name
                ))),
            }
        }
    }

    /// Enable or disable an installed plugin by toggling the `disabled`
    /// marker file in its directory.
    pub fn set_enabled(plugins_dir: &Path, name: &str, enabled: bool) -> Result<(), PluginError> {
        let manifest = Self::discover(plugins_dir)?
            .into_iter()
            .find(|m| m.name == name)
            .ok_or_else(|| PluginError::NotFound {
                name: name.to_string(),
                dir: plugins_dir.display().to_string(),
            })?;
        let marker = manifest.dir.join(DISABLED_MARKER);
        if enabled {
            match std::fs::remove_file(&marker) {
                Ok(()) => Ok(()),
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
                Err(e) => Err(PluginError::io(&marker, e)),
            }
        } else {
            std::fs::write(&marker, b"").map_err(|e| PluginError::io(&marker, e))
        }
    }

    /// Install a plugin by copying its directory (which must contain a valid
    /// `plugin.toml`) into `plugins_dir/<name>`. Returns the installed
    /// manifest. Overwrites an existing installation of the same name.
    pub fn install_from_dir(src: &Path, plugins_dir: &Path) -> Result<PluginManifest, PluginError> {
        let manifest = PluginManifest::load(src)?;
        let dest = plugins_dir.join(&manifest.name);
        std::fs::create_dir_all(&dest).map_err(|e| PluginError::io(&dest, e))?;
        for entry in std::fs::read_dir(src).map_err(|e| PluginError::io(src, e))? {
            let entry = entry.map_err(|e| PluginError::io(src, e))?;
            let from = entry.path();
            if from.is_file() {
                let to = dest.join(entry.file_name());
                std::fs::copy(&from, &to).map_err(|e| PluginError::io(&to, e))?;
            }
        }
        PluginManifest::load(&dest)
    }
}
