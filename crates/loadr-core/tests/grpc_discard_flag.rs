//! Flow-level coverage for the gRPC lazy-decode gate: `prepare` computes
//! `GrpcRequest.discard_response_body` from whether the plan's
//! `extract`/`assert`/`checks` read the response body, ANDed with whether a
//! script `afterRequest` hook is present. Driven through the real engine
//! with a mock protocol handler (no real gRPC server — `reflection: true`
//! passes config validation without dialing) and a mock script engine (no
//! real JS runtime).

use std::collections::HashSet;
use std::sync::Arc;

use async_trait::async_trait;
use loadr_core::{
    Engine, EngineOptions, PreparedRequest, ProtocolError, ProtocolHandler, ProtocolRegistry,
    ProtocolResponse, ScriptEngine, ScriptError, ScriptHost, VuContext, VuScript,
};

/// Records the `discard_response_body` flag `prepare` computed for each gRPC
/// request; returns a canned response so `extract`/`assert`/`checks` in the
/// plan have something to match against.
#[derive(Default)]
struct RecordingGrpcHandler {
    flags: parking_lot::Mutex<Vec<bool>>,
}

#[async_trait]
impl ProtocolHandler for RecordingGrpcHandler {
    fn name(&self) -> &str {
        "grpc"
    }
    async fn execute(
        &self,
        _ctx: &mut VuContext,
        request: &PreparedRequest,
    ) -> Result<ProtocolResponse, ProtocolError> {
        let grpc = request.options.grpc.as_ref().expect("grpc options");
        self.flags.lock().push(grpc.discard_response_body);
        Ok(ProtocolResponse {
            status: 0,
            protocol_version: "grpc".to_string(),
            url: request.url.clone(),
            body: bytes::Bytes::from_static(br#"{"ok":true}"#),
            ..Default::default()
        })
    }
}

/// Minimal `ScriptEngine`/`VuScript`: reports a fixed, configurable set of
/// "exported" functions and no-ops everything else — enough to drive
/// `prepare`'s runtime `afterRequest` gate without a real JS runtime.
#[derive(Clone, Default)]
struct MockScriptEngine {
    functions: HashSet<String>,
}

impl MockScriptEngine {
    fn with_functions(names: &[&str]) -> Self {
        MockScriptEngine {
            functions: names.iter().map(|s| s.to_string()).collect(),
        }
    }
}

impl ScriptEngine for MockScriptEngine {
    fn setup(&self, _host: &mut dyn ScriptHost) -> Result<serde_json::Value, ScriptError> {
        Ok(serde_json::Value::Null)
    }

    fn teardown(
        &self,
        _host: &mut dyn ScriptHost,
        _setup_data: serde_json::Value,
    ) -> Result<(), ScriptError> {
        Ok(())
    }

    fn instantiate(&self) -> Result<Box<dyn VuScript>, ScriptError> {
        Ok(Box::new(MockVuScript {
            functions: self.functions.clone(),
        }))
    }

    fn has_function(&self, name: &str) -> bool {
        self.functions.contains(name)
    }
}

struct MockVuScript {
    functions: HashSet<String>,
}

impl VuScript for MockVuScript {
    fn call_function(
        &mut self,
        _host: &mut dyn ScriptHost,
        _name: &str,
        _args: &[serde_json::Value],
    ) -> Result<serde_json::Value, ScriptError> {
        Ok(serde_json::Value::Null)
    }

    fn eval(
        &mut self,
        _host: &mut dyn ScriptHost,
        _code: &str,
    ) -> Result<serde_json::Value, ScriptError> {
        Ok(serde_json::Value::Null)
    }

    fn has_function(&self, name: &str) -> bool {
        self.functions.contains(name)
    }
}

/// Run one gRPC request through the real engine; return the
/// `discard_response_body` flag(s) `prepare` computed for it.
async fn discard_flags(yaml: &str, script: Option<Arc<dyn ScriptEngine>>) -> Vec<bool> {
    let loaded = loadr_config::load_str(yaml, &loadr_config::LoadOptions::new()).expect("parse");
    let handler = Arc::new(RecordingGrpcHandler::default());
    let mut protocols = ProtocolRegistry::new();
    protocols.register(handler.clone());
    let engine = Engine::new(
        loaded.plan,
        std::path::PathBuf::from("."),
        EngineOptions {
            protocols,
            script,
            ..Default::default()
        },
    )
    .expect("engine");
    engine.run().await.expect("run");
    let flags = handler.flags.lock().clone();
    flags
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn bare_request_discards_body() {
    let flags = discard_flags(
        r#"
scenarios:
  s:
    executor: shared-iterations
    vus: 1
    iterations: 1
    flow:
      - request:
          url: grpc://example.invalid:1
          grpc:
            reflection: true
            service: loadr.test.Echo
            method: UnaryEcho
            message: { message: "hi" }
"#,
        None,
    )
    .await;
    assert_eq!(flags, vec![true]);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn status_assert_still_discards_body() {
    let flags = discard_flags(
        r#"
scenarios:
  s:
    executor: shared-iterations
    vus: 1
    iterations: 1
    flow:
      - request:
          url: grpc://example.invalid:1
          grpc:
            reflection: true
            service: loadr.test.Echo
            method: UnaryEcho
            message: { message: "hi" }
          assert:
            - { type: status, equals: 0 }
"#,
        None,
    )
    .await;
    assert_eq!(flags, vec![true]);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn jsonpath_assert_forces_decode() {
    let flags = discard_flags(
        r#"
scenarios:
  s:
    executor: shared-iterations
    vus: 1
    iterations: 1
    flow:
      - request:
          url: grpc://example.invalid:1
          grpc:
            reflection: true
            service: loadr.test.Echo
            method: UnaryEcho
            message: { message: "hi" }
          assert:
            - { type: jsonpath, expression: "$.ok", equals: true }
"#,
        None,
    )
    .await;
    assert_eq!(flags, vec![false]);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn extract_forces_decode() {
    let flags = discard_flags(
        r#"
scenarios:
  s:
    executor: shared-iterations
    vus: 1
    iterations: 1
    flow:
      - request:
          url: grpc://example.invalid:1
          grpc:
            reflection: true
            service: loadr.test.Echo
            method: UnaryEcho
            message: { message: "hi" }
          extract:
            - { type: jsonpath, name: ok, expression: "$.ok" }
"#,
        None,
    )
    .await;
    assert_eq!(flags, vec![false]);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn body_contains_check_forces_decode() {
    let flags = discard_flags(
        r#"
scenarios:
  s:
    executor: shared-iterations
    vus: 1
    iterations: 1
    flow:
      - request:
          url: grpc://example.invalid:1
          grpc:
            reflection: true
            service: loadr.test.Echo
            method: UnaryEcho
            message: { message: "hi" }
          checks:
            - { type: body_contains, value: "ok" }
"#,
        None,
    )
    .await;
    assert_eq!(flags, vec![false]);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn js_condition_forces_decode() {
    let flags = discard_flags(
        r#"
scenarios:
  s:
    executor: shared-iterations
    vus: 1
    iterations: 1
    flow:
      - request:
          url: grpc://example.invalid:1
          grpc:
            reflection: true
            service: loadr.test.Echo
            method: UnaryEcho
            message: { message: "hi" }
          assert:
            - { type: js, expression: "response.status === 0" }
"#,
        None,
    )
    .await;
    assert_eq!(flags, vec![false]);
}

const BARE_REQUEST_YAML: &str = r#"
scenarios:
  s:
    executor: shared-iterations
    vus: 1
    iterations: 1
    flow:
      - request:
          url: grpc://example.invalid:1
          grpc:
            reflection: true
            service: loadr.test.Echo
            method: UnaryEcho
            message: { message: "hi" }
"#;

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn after_request_hook_forces_decode() {
    let script: Arc<dyn ScriptEngine> =
        Arc::new(MockScriptEngine::with_functions(&["afterRequest"]));
    let flags = discard_flags(BARE_REQUEST_YAML, Some(script)).await;
    assert_eq!(flags, vec![false]);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn script_without_after_request_still_discards_body() {
    let script: Arc<dyn ScriptEngine> = Arc::new(MockScriptEngine::with_functions(&["setup"]));
    let flags = discard_flags(BARE_REQUEST_YAML, Some(script)).await;
    assert_eq!(flags, vec![true]);
}
