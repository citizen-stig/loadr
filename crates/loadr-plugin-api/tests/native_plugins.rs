//! End-to-end tests for native dynamic-library plugins: build the example
//! cdylibs, load them via abi_stable, and drive the core-facing adapters.

mod common;

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use loadr_core::metrics::{now_millis, MetricKind, MetricRegistry, MetricsBus, Sample, Tags};
use loadr_core::vu::RunContext;
use loadr_core::{
    Aggregator, Output, PreparedRequest, ProtocolHandler, RequestOptions, Summary, VuContext,
};
use loadr_plugin_api::NativePlugin;

fn output_so() -> std::path::PathBuf {
    common::build_native_example("loadr-plugin-example-native-output", "native_output")
}

fn protocol_so() -> std::path::PathBuf {
    common::build_native_example("loadr-plugin-example-native-protocol", "native_protocol")
}

fn sample(metric: &str, kind: MetricKind, value: f64) -> Sample {
    Sample {
        metric: Arc::from(metric),
        kind,
        value,
        tags: Arc::new(Tags::new()),
        timestamp_ms: now_millis(),
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

#[tokio::test]
async fn output_plugin_writes_report_file() {
    let plugin = NativePlugin::load(&output_so()).expect("load output plugin");
    assert_eq!(plugin.info().name, "file-report");
    assert_eq!(plugin.info().kind, "output");

    let dir = tempfile::tempdir().expect("tempdir");
    let report = dir.path().join("report.txt");
    let mut output = plugin
        .make_output(serde_json::json!({"path": report}))
        .expect("make output");
    assert_eq!(output.name(), "file-report");

    // Build real snapshots/summary through the aggregator.
    let mut agg = Aggregator::new();
    agg.record(&sample("http_reqs", MetricKind::Counter, 1.0));
    agg.record(&sample("http_req_duration", MetricKind::Trend, 42.0));

    output.start().await.expect("start");
    let samples = [sample("http_reqs", MetricKind::Counter, 1.0)];
    output.on_samples(&samples).await;
    let snapshot = agg.snapshot();
    assert_eq!(snapshot.series.len(), 2);
    output.on_snapshot(&snapshot).await;
    let summary = Summary::build(
        Some("native-output-test".into()),
        "run-42".into(),
        now_millis(),
        vec!["default".into()],
        &mut agg,
        Vec::new(),
        None,
        Vec::new(),
    );
    output.finish(&summary).await;

    let text = std::fs::read_to_string(&report).expect("report written");
    assert!(
        text.contains("snapshot 1: series=2"),
        "snapshot line present:\n{text}"
    );
    assert!(
        text.contains("summary: run_id=run-42"),
        "summary line present:\n{text}"
    );
    assert!(text.contains("snapshots=1"), "{text}");
}

#[tokio::test]
async fn output_plugin_rejects_bad_config() {
    let plugin = NativePlugin::load(&output_so()).expect("load output plugin");
    let mut output = plugin
        .make_output(serde_json::json!({}))
        .expect("make output");
    let err = output.start().await.expect_err("missing path must fail");
    assert!(err.to_string().contains("path"), "{err}");
}

#[tokio::test]
async fn protocol_plugin_reverses_body_with_prefix() {
    let plugin = NativePlugin::load(&protocol_so()).expect("load protocol plugin");
    assert_eq!(plugin.info().kind, "protocol");
    let handler = plugin
        .make_protocol(serde_json::Value::Null)
        .expect("make protocol");
    assert_eq!(ProtocolHandler::name(&handler), "echo-proto");

    let mut vu = minimal_vu();
    let request = PreparedRequest {
        name: "echo".into(),
        protocol: "echo-proto".into(),
        method: "SEND".into(),
        url: "echo://local".into(),
        headers: vec![("x-test".into(), "1".into())],
        body: bytes::Bytes::from_static(b"abcdef"),
        timeout: Duration::from_secs(5),
        follow_redirects: false,
        max_redirects: 0,
        options: RequestOptions {
            plugin: Some(serde_json::json!({"prefix": "PFX:"})),
            ..Default::default()
        },
    };
    let response = handler.execute(&mut vu, &request).await.expect("execute");
    assert_eq!(response.status, 200);
    assert_eq!(response.status_text, "OK");
    assert_eq!(&response.body[..], b"PFX:fedcba");
    assert!(response.timings.duration_ms >= 0.0);
    assert_eq!(response.header("x-echo-proto"), Some("1"));
    assert_eq!(response.extras["prefix_applied"], true);
    assert!(response.error.is_none());
    assert_eq!(response.bytes_sent, 6);
    assert_eq!(response.bytes_received, 10);
}

#[tokio::test]
async fn protocol_plugin_config_prefix_fallback() {
    let plugin = NativePlugin::load(&protocol_so()).expect("load protocol plugin");
    let handler = plugin
        .make_protocol(serde_json::json!({"prefix": "CFG:"}))
        .expect("make protocol");
    let mut vu = minimal_vu();
    let request = PreparedRequest {
        name: "echo".into(),
        protocol: "echo-proto".into(),
        method: "SEND".into(),
        url: "echo://local".into(),
        headers: Vec::new(),
        body: bytes::Bytes::from_static(b"xyz"),
        timeout: Duration::from_secs(5),
        follow_redirects: false,
        max_redirects: 0,
        options: RequestOptions::default(),
    };
    let response = handler.execute(&mut vu, &request).await.expect("execute");
    assert_eq!(&response.body[..], b"CFG:zyx");
}

#[test]
fn kind_mismatch_constructors_error() {
    let plugin = NativePlugin::load(&output_so()).expect("load output plugin");
    let err = plugin
        .make_protocol(serde_json::Value::Null)
        .expect_err("output plugin has no protocol");
    assert!(
        matches!(err, loadr_plugin_api::PluginError::KindMismatch { .. }),
        "{err}"
    );
    let err = plugin.make_service().expect_err("no service either");
    assert!(
        matches!(err, loadr_plugin_api::PluginError::KindMismatch { .. }),
        "{err}"
    );
}

#[test]
fn missing_library_errors_cleanly() {
    let err = NativePlugin::load(std::path::Path::new("/nonexistent/libplugin.so"))
        .expect_err("missing file");
    assert!(
        matches!(err, loadr_plugin_api::PluginError::Load { .. }),
        "{err}"
    );
}
