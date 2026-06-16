//! End-to-end tests for the plain C-ABI plugin path: build the `c-echo`
//! example with the system C compiler, then load and drive it through the same
//! registry / engine surfaces the abi_stable plugins use.

mod common;

use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use loadr_core::metrics::{MetricRegistry, MetricsBus, Tags};
use loadr_core::vu::RunContext;
use loadr_core::{PreparedRequest, ProtocolHandler, RequestOptions, VuContext};
use loadr_plugin_api::{is_c_abi_plugin, CAbiPlugin, LoadedPlugin, PluginRegistry};

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

fn plugin_ref(name: &str, config: serde_json::Value) -> loadr_config::PluginRef {
    serde_json::from_value(serde_json::json!({ "name": name, "config": config }))
        .expect("valid plugin ref")
}

/// The c-echo cdylib is detected as a C-ABI plugin, and an abi_stable plugin
/// is not.
#[test]
fn detection_distinguishes_c_from_abi_stable() {
    let c_so = common::build_c_echo_example();
    assert!(is_c_abi_plugin(&c_so), "c-echo exports the C entry symbol");

    let rust_so =
        common::build_native_example("loadr-plugin-example-native-protocol", "native_protocol");
    assert!(
        !is_c_abi_plugin(&rust_so),
        "abi_stable plugin has no C entry symbol and must fall back"
    );
}

/// Load the C plugin directly and drive its protocol handler exactly as the
/// engine would: a `PreparedRequest` in, a `ProtocolResponse` out.
#[tokio::test]
async fn c_plugin_execute_round_trip() {
    let so = common::build_c_echo_example();
    let plugin = CAbiPlugin::load(&so).expect("load c-echo");
    assert_eq!(plugin.info().name, "cecho");
    assert_eq!(plugin.info().kind, "protocol");
    assert_eq!(plugin.info().schemes, vec!["cecho".to_string()]);

    let handler = plugin
        .make_protocol(serde_json::Value::Null)
        .expect("make protocol");
    assert_eq!(ProtocolHandler::name(&handler), "cecho");

    let mut vu = minimal_vu();
    let request = PreparedRequest {
        name: "echo".into(),
        protocol: "cecho".into(),
        method: "SEND".into(),
        url: "cecho://local".into(),
        headers: Vec::new(),
        body: bytes::Bytes::from_static(b"hello-c-abi"),
        timeout: Duration::from_secs(5),
        follow_redirects: false,
        max_redirects: 0,
        options: RequestOptions::default(),
    };
    let response = handler.execute(&mut vu, &request).await.expect("execute");
    assert_eq!(response.status, 200);
    assert_eq!(response.status_text, "OK");
    // The C plugin echoes the request body verbatim.
    assert_eq!(&response.body[..], b"hello-c-abi");
    assert_eq!(response.bytes_sent, 11);
    assert_eq!(response.bytes_received, 11);
    assert_eq!(response.header("x-cecho"), Some("1"));
    assert_eq!(response.extras["echoed_by"], "c-echo");
    assert_eq!(response.extras["method"], "SEND");
    assert!(response.error.is_none());
}

/// Install c-echo with its `plugin.toml` (which declares `abi = "c"` and
/// `schemes = ["cecho"]`), load it through the registry, and assert scheme
/// routing works just like an abi_stable plugin.
#[test]
fn c_plugin_loads_via_registry_and_registers_scheme() {
    let root = common::workspace_root();
    let so = common::build_c_echo_example();

    let tmp = tempfile::tempdir().expect("tempdir");
    let plugins_dir = tmp.path().join("plugins");
    let dir = plugins_dir.join("cecho");
    std::fs::create_dir_all(&dir).expect("mkdir");
    std::fs::copy(
        root.join("examples/plugins/c-echo/plugin.toml"),
        dir.join("plugin.toml"),
    )
    .expect("copy manifest");
    // Copy to the filename the manifest's `entry` points at.
    let entry = PluginRegistry::discover(&plugins_dir)
        .expect("discover")
        .into_iter()
        .find(|m| m.name == "cecho")
        .expect("cecho manifest")
        .entry;
    std::fs::copy(&so, &entry).expect("copy artifact");

    loadr_core::protocol::clear_plugin_schemes();
    let loaded =
        PluginRegistry::load_ref(&plugin_ref("cecho", serde_json::Value::Null), &plugins_dir)
            .expect("load cecho");
    assert!(matches!(loaded, LoadedPlugin::Protocol(_)), "{loaded:?}");

    assert_eq!(
        loadr_core::ProtocolRegistry::infer(None, "cecho://host/path"),
        "cecho"
    );
    loadr_core::protocol::clear_plugin_schemes();
}

/// The same library loads via the registry with auto-detection when the
/// manifest omits the `abi` key entirely (backward-compatible default).
#[test]
fn c_plugin_auto_detected_without_abi_key() {
    let so = common::build_c_echo_example();

    let tmp = tempfile::tempdir().expect("tempdir");
    let plugins_dir = tmp.path().join("plugins");
    let dir = plugins_dir.join("cecho");
    std::fs::create_dir_all(&dir).expect("mkdir");
    // A manifest with NO `abi` key: the loader must probe and pick the C ABI.
    std::fs::write(
        dir.join("plugin.toml"),
        "[plugin]\n\
         name = \"cecho\"\n\
         version = \"0.1.0\"\n\
         kind = \"protocol\"\n\
         type = \"native\"\n\
         entry = \"libloadr_plugin_cecho.so\"\n\
         schemes = [\"cecho\"]\n",
    )
    .expect("write manifest");
    std::fs::copy(&so, dir.join("libloadr_plugin_cecho.so")).expect("copy artifact");

    loadr_core::protocol::clear_plugin_schemes();
    let loaded =
        PluginRegistry::load_ref(&plugin_ref("cecho", serde_json::Value::Null), &plugins_dir)
            .expect("load auto-detected cecho");
    assert!(matches!(loaded, LoadedPlugin::Protocol(_)), "{loaded:?}");
    loadr_core::protocol::clear_plugin_schemes();
}

/// A bare `.so` path with no manifest: kind/schemes come from `info()`.
#[test]
fn c_plugin_loads_by_explicit_path() {
    let so = common::build_c_echo_example();
    let mut pref = plugin_ref("cecho", serde_json::Value::Null);
    pref.path = Some(so);
    loadr_core::protocol::clear_plugin_schemes();
    let loaded = PluginRegistry::load_ref(&pref, Path::new("/nonexistent")).expect("load by path");
    assert!(matches!(loaded, LoadedPlugin::Protocol(_)));
    loadr_core::protocol::clear_plugin_schemes();
}
