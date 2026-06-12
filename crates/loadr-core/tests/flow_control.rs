//! Integration tests for the Locust/Gatling-inspired flow-control steps,
//! feeder strategies and throttling — driven through the real engine with a
//! mock protocol handler. (JS-condition while/if coverage lives in the CLI
//! e2e suite, which wires the real QuickJS engine.)

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use async_trait::async_trait;
use loadr_core::{
    Engine, EngineOptions, PreparedRequest, ProtocolError, ProtocolHandler, ProtocolRegistry,
    ProtocolResponse, VuContext,
};

/// A protocol handler that records every request URL and returns 200.
#[derive(Default)]
struct RecordingHandler {
    count: AtomicU64,
    urls: parking_lot::Mutex<Vec<String>>,
}

#[async_trait]
impl ProtocolHandler for RecordingHandler {
    fn name(&self) -> &str {
        "http"
    }
    async fn execute(
        &self,
        _ctx: &mut VuContext,
        request: &PreparedRequest,
    ) -> Result<ProtocolResponse, ProtocolError> {
        self.count.fetch_add(1, Ordering::Relaxed);
        self.urls.lock().push(request.url.clone());
        Ok(ProtocolResponse {
            status: 200,
            protocol_version: "HTTP/1.1".into(),
            url: request.url.clone(),
            ..Default::default()
        })
    }
}

fn registry(handler: Arc<RecordingHandler>) -> ProtocolRegistry {
    let mut reg = ProtocolRegistry::new();
    reg.register(handler);
    reg.register_alias("https", "http");
    reg
}

async fn run(yaml: &str, handler: Arc<RecordingHandler>) -> loadr_core::RunResult {
    let loaded = loadr_config::load_str(yaml, &loadr_config::LoadOptions::new()).expect("parse");
    let engine = Engine::new(
        loaded.plan,
        std::path::PathBuf::from("."),
        EngineOptions {
            protocols: registry(handler),
            ..Default::default()
        },
    )
    .expect("engine");
    engine.run().await.expect("run")
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn repeat_runs_steps_n_times() {
    let handler = Arc::new(RecordingHandler::default());
    run(
        r#"
scenarios:
  s:
    executor: shared-iterations
    vus: 1
    iterations: 3
    flow:
      - repeat:
          times: 4
          steps:
            - request: { url: "http://x/hit" }
"#,
        handler.clone(),
    )
    .await;
    // 3 iterations × 4 repeats = 12 requests.
    assert_eq!(handler.count.load(Ordering::Relaxed), 12);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn random_weighted_favours_heavy_branch() {
    let handler = Arc::new(RecordingHandler::default());
    run(
        r#"
scenarios:
  s:
    executor: shared-iterations
    vus: 4
    iterations: 400
    flow:
      - random:
          strategy: weighted
          choices:
            - { weight: 9, steps: [ { request: { url: "http://x/common" } } ] }
            - { weight: 1, steps: [ { request: { url: "http://x/rare" } } ] }
"#,
        handler.clone(),
    )
    .await;
    let urls = handler.urls.lock();
    let common = urls.iter().filter(|u| u.ends_with("/common")).count();
    let rare = urls.iter().filter(|u| u.ends_with("/rare")).count();
    assert_eq!(common + rare, 400);
    // ~90/10 split; allow generous slack but the heavy branch must dominate.
    assert!(common > rare * 3, "common={common} rare={rare}");
    assert!(rare > 0, "rare branch should still fire sometimes");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn round_robin_alternates() {
    let handler = Arc::new(RecordingHandler::default());
    run(
        r#"
scenarios:
  s:
    executor: shared-iterations
    vus: 1
    iterations: 6
    flow:
      - random:
          strategy: round_robin
          choices:
            - { steps: [ { request: { url: "http://x/a" } } ] }
            - { steps: [ { request: { url: "http://x/b" } } ] }
            - { steps: [ { request: { url: "http://x/c" } } ] }
"#,
        handler.clone(),
    )
    .await;
    let urls = handler.urls.lock();
    assert_eq!(urls.iter().filter(|u| u.ends_with("/a")).count(), 2);
    assert_eq!(urls.iter().filter(|u| u.ends_with("/b")).count(), 2);
    assert_eq!(urls.iter().filter(|u| u.ends_with("/c")).count(), 2);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn throttle_caps_request_rate() {
    let handler = Arc::new(RecordingHandler::default());
    let start = std::time::Instant::now();
    run(
        r#"
scenarios:
  s:
    executor: constant-vus
    vus: 10
    duration: 2s
    throttle: { requests_per_second: 20 }
    flow:
      - request: { url: "http://x/throttled" }
"#,
        handler.clone(),
    )
    .await;
    let elapsed = start.elapsed().as_secs_f64();
    let count = handler.count.load(Ordering::Relaxed);
    // At 20 rps for ~2s, expect roughly 40 requests — never wildly more.
    assert!(
        count <= 55,
        "throttle exceeded: {count} requests in {elapsed:.1}s"
    );
    assert!(
        count >= 20,
        "throttle too aggressive: only {count} requests"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn weighted_data_pick_random() {
    // `random` feeder never exhausts even with on_eof: stop.
    let handler = Arc::new(RecordingHandler::default());
    run(
        r#"
data:
  ids:
    type: inline
    pick: random
    on_eof: stop
    rows:
      - { id: a }
      - { id: b }
      - { id: c }
scenarios:
  s:
    executor: shared-iterations
    vus: 2
    iterations: 50
    flow:
      - request: { url: "http://x/item/${data.ids.id}" }
"#,
        handler.clone(),
    )
    .await;
    // 50 iterations all completed (no early stop from the feeder).
    assert_eq!(handler.count.load(Ordering::Relaxed), 50);
}
