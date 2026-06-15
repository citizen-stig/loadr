//! Registry tests: discovery over an assembled plugins dir, enable/disable
//! round trips, install, and `load_ref` resolution by name and by path.

mod common;

use std::path::Path;

use loadr_core::ProtocolResponse;
use loadr_plugin_api::{LoadedPlugin, PluginKind, PluginRegistry, PluginType};

/// Assemble a plugins dir containing the built example plugins:
/// the wasm extractor (`upper-boundary`) and the native protocol
/// (`echo-proto`), each installed as `<dir>/<name>/{plugin.toml, artifact}`.
fn assemble_plugins_dir(dir: &Path) {
    let root = common::workspace_root();

    let wasm = common::build_wasm_guest("wasm-extractor", "loadr_wasm_extractor.wasm");
    let extractor_dir = dir.join("upper-boundary");
    std::fs::create_dir_all(&extractor_dir).expect("mkdir");
    std::fs::copy(
        root.join("plugins/examples/wasm-extractor/plugin.toml"),
        extractor_dir.join("plugin.toml"),
    )
    .expect("copy manifest");
    std::fs::copy(&wasm, extractor_dir.join("loadr_wasm_extractor.wasm")).expect("copy wasm");

    let so =
        common::build_native_example("loadr-plugin-example-native-protocol", "native_protocol");
    let proto_dir = dir.join("echo-proto");
    std::fs::create_dir_all(&proto_dir).expect("mkdir");
    std::fs::copy(
        root.join("plugins/examples/native-protocol/plugin.toml"),
        proto_dir.join("plugin.toml"),
    )
    .expect("copy manifest");
    std::fs::copy(&so, proto_dir.join("libnative_protocol.so")).expect("copy so");
}

fn plugin_ref(name: &str, config: serde_json::Value) -> loadr_config::PluginRef {
    serde_json::from_value(serde_json::json!({ "name": name, "config": config }))
        .expect("valid plugin ref")
}

#[test]
fn discover_enable_disable_and_load_ref() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let plugins_dir = tmp.path().join("plugins");
    assemble_plugins_dir(&plugins_dir);

    // Discovery: both plugins, sorted by name, enabled.
    let manifests = PluginRegistry::discover(&plugins_dir).expect("discover");
    assert_eq!(manifests.len(), 2);
    assert_eq!(manifests[0].name, "echo-proto");
    assert_eq!(manifests[0].kind, PluginKind::Protocol);
    assert_eq!(manifests[0].plugin_type, PluginType::Native);
    assert_eq!(manifests[1].name, "upper-boundary");
    assert_eq!(manifests[1].kind, PluginKind::Extractor);
    assert_eq!(manifests[1].plugin_type, PluginType::Wasm);
    assert!(manifests.iter().all(|m| m.enabled));
    assert_eq!(manifests[1].default_config["left"], "<<");

    // Disable / re-enable round trip.
    PluginRegistry::set_enabled(&plugins_dir, "echo-proto", false).expect("disable");
    let manifests = PluginRegistry::discover(&plugins_dir).expect("discover");
    assert!(!manifests[0].enabled, "echo-proto disabled");
    assert!(manifests[1].enabled, "upper-boundary still enabled");

    // Disabled plugins don't resolve by name.
    let err = PluginRegistry::load_ref(
        &plugin_ref("echo-proto", serde_json::Value::Null),
        &plugins_dir,
    )
    .expect_err("disabled plugin is not loadable by name");
    assert!(
        matches!(err, loadr_plugin_api::PluginError::NotFound { .. }),
        "{err}"
    );

    PluginRegistry::set_enabled(&plugins_dir, "echo-proto", true).expect("enable");
    assert!(PluginRegistry::discover(&plugins_dir)
        .expect("discover")
        .iter()
        .all(|m| m.enabled));

    // load_ref by name: wasm extractor, exercising manifest config defaults.
    let loaded = PluginRegistry::load_ref(
        &plugin_ref("upper-boundary", serde_json::Value::Null),
        &plugins_dir,
    )
    .expect("load upper-boundary");
    let LoadedPlugin::Extractor(extractor) = loaded else {
        panic!("expected extractor, got {loaded:?}");
    };
    let response = ProtocolResponse {
        body: bytes::Bytes::from_static(b"x <<hello>> y"),
        ..Default::default()
    };
    // Config comes from the manifest's [config] defaults here.
    let manifest = PluginRegistry::discover(&plugins_dir)
        .expect("discover")
        .into_iter()
        .find(|m| m.name == "upper-boundary")
        .expect("manifest");
    let value = extractor
        .extract(&response, &manifest.merged_config(&serde_json::Value::Null))
        .expect("extract");
    assert_eq!(value.as_deref(), Some("HELLO"));

    // load_ref by name: native protocol.
    let loaded = PluginRegistry::load_ref(
        &plugin_ref("echo-proto", serde_json::Value::Null),
        &plugins_dir,
    )
    .expect("load echo-proto");
    assert!(matches!(loaded, LoadedPlugin::Protocol(_)), "{loaded:?}");

    // Unknown names error cleanly.
    let err = PluginRegistry::load_ref(&plugin_ref("nope", serde_json::Value::Null), &plugins_dir)
        .expect_err("unknown plugin");
    assert!(
        matches!(err, loadr_plugin_api::PluginError::NotFound { .. }),
        "{err}"
    );
}

#[test]
fn load_ref_by_explicit_path() {
    // A bare .so path (no manifest next to it): kind comes from info().
    let so =
        common::build_native_example("loadr-plugin-example-native-protocol", "native_protocol");
    let mut pref = plugin_ref("echo-proto", serde_json::Value::Null);
    pref.path = Some(so);
    let loaded = PluginRegistry::load_ref(&pref, Path::new("/nonexistent")).expect("load by path");
    assert!(matches!(loaded, LoadedPlugin::Protocol(_)));

    // A bare .wasm path with the kind supplied in the ref config.
    let wasm = common::build_wasm_guest("wasm-extractor", "loadr_wasm_extractor.wasm");
    let mut pref = plugin_ref("upper-boundary", serde_json::json!({"kind": "extractor"}));
    pref.path = Some(wasm.clone());
    let loaded =
        PluginRegistry::load_ref(&pref, Path::new("/nonexistent")).expect("load wasm by path");
    assert!(matches!(loaded, LoadedPlugin::Extractor(_)));

    // A bare .wasm path with no kind hint: probed via meta.describe().
    let mut pref = plugin_ref("upper-boundary", serde_json::Value::Null);
    pref.path = Some(wasm);
    let loaded =
        PluginRegistry::load_ref(&pref, Path::new("/nonexistent")).expect("load via probe");
    assert!(matches!(loaded, LoadedPlugin::Extractor(_)));
}

#[test]
fn protocol_plugin_registers_declared_schemes() {
    // Build the real mongo plugin and install it with its `plugin.toml`
    // (which declares `schemes = ["mongodb", "mongo"]`), then assert the host
    // scheme router routes those URL schemes to the plugin handler name.
    let root = common::workspace_root();
    let so = common::build_native_example("loadr-plugin-mongo", "loadr_plugin_mongo");

    let tmp = tempfile::tempdir().expect("tempdir");
    let plugins_dir = tmp.path().join("plugins");
    let mongo_dir = plugins_dir.join("mongo");
    std::fs::create_dir_all(&mongo_dir).expect("mkdir");
    std::fs::copy(
        root.join("plugins/loadr-plugin-mongo/plugin.toml"),
        mongo_dir.join("plugin.toml"),
    )
    .expect("copy manifest");
    // Copy to the filename the manifest's `entry` points at, regardless of the
    // host's dylib extension (abi_stable loads by path, not by extension).
    let entry = PluginRegistry::discover(&plugins_dir)
        .expect("discover")
        .into_iter()
        .find(|m| m.name == "mongo")
        .expect("mongo manifest")
        .entry;
    std::fs::copy(&so, &entry).expect("copy artifact");

    loadr_core::protocol::clear_plugin_schemes();
    let loaded =
        PluginRegistry::load_ref(&plugin_ref("mongo", serde_json::Value::Null), &plugins_dir)
            .expect("load mongo");
    assert!(matches!(loaded, LoadedPlugin::Protocol(_)), "{loaded:?}");

    // The declared schemes now route to the handler whose `name()` is `mongo`.
    assert_eq!(
        loadr_core::ProtocolRegistry::infer(None, "mongodb://h:27017/db"),
        "mongo"
    );
    assert_eq!(
        loadr_core::ProtocolRegistry::infer(None, "mongo://h/db"),
        "mongo"
    );
    loadr_core::protocol::clear_plugin_schemes();
}

#[test]
fn install_from_dir_copies_plugin() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let staged = tmp.path().join("staged");
    assemble_plugins_dir(&staged);

    let plugins_dir = tmp.path().join("installed");
    let manifest = PluginRegistry::install_from_dir(&staged.join("upper-boundary"), &plugins_dir)
        .expect("install");
    assert_eq!(manifest.name, "upper-boundary");
    assert!(manifest.entry.is_file(), "artifact copied");
    assert_eq!(manifest.dir, plugins_dir.join("upper-boundary"));

    let discovered = PluginRegistry::discover(&plugins_dir).expect("discover");
    assert_eq!(discovered.len(), 1);
    assert_eq!(discovered[0].name, "upper-boundary");
}

#[test]
fn discover_missing_dir_is_empty() {
    let list = PluginRegistry::discover(Path::new("/nonexistent/loadr-plugins"))
        .expect("missing dir is fine");
    assert!(list.is_empty());
}
