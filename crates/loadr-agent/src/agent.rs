//! The load agent: connects to a controller, registers, receives assignments,
//! runs partitioned test plans and streams metric deltas back.

use std::collections::HashMap;
use std::path::{Component, Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use loadr_core::{
    Engine, EngineOptions, MetricsDelta, Output, ProtocolRegistry, RunHandle, RunStatus,
    ScriptEngine, Tags,
};
use parking_lot::Mutex;
use tokio::sync::{mpsc, oneshot};
use tokio_stream::wrappers::ReceiverStream;
use tokio_util::sync::CancellationToken;
use tonic::transport::{Certificate, Channel, ClientTlsConfig, Endpoint, Identity};

use crate::error::AgentError;
use crate::pb;
use crate::pb::agent_message::Msg as AgentMsg;
use crate::pb::controller_message::Msg as CtrlMsg;
use crate::pb::coordination_client::CoordinationClient;
use crate::{now_unix_ms, PROTOCOL_VERSION};

/// Builds a [`ProtocolRegistry`] for one run from the parsed test plan (HTTP
/// defaults, `plugins:` declarations) and the run's base directory (where
/// data files were materialized).
pub type ProtocolFactory = Arc<
    dyn Fn(&loadr_config::TestPlan, &std::path::Path) -> Result<ProtocolRegistry, String>
        + Send
        + Sync,
>;

/// Builds a script engine for one run from the plan's `js:` block and the
/// run's base directory.
pub type ScriptFactory = Arc<
    dyn Fn(&loadr_config::JsConfig, &std::path::Path) -> Result<Arc<dyn ScriptEngine>, String>
        + Send
        + Sync,
>;

/// Builds the plugin-backed data sources for one run from the plan's
/// `plugins:` declarations and the run's base directory (where data files
/// were materialized). The returned map is keyed by the plan's `plugins:`
/// entry name — that is what `data.<name>.source` refers to.
pub type DataSourceFactory = Arc<
    dyn Fn(
            &[loadr_config::PluginRef],
            &std::path::Path,
        ) -> Result<HashMap<String, Box<dyn loadr_core::DataSourcePlugin>>, String>
        + Send
        + Sync,
>;

/// Injected runtime dependencies (keeps `loadr-agent` decoupled from the
/// protocol and JS crates).
#[derive(Clone)]
pub struct RunnerDeps {
    pub protocols: ProtocolFactory,
    pub script: Option<ScriptFactory>,
    /// `None` means this agent build has no data-source plugin support: plans
    /// that declare `data.*: { type: plugin }` sources fail engine setup.
    pub data_sources: Option<DataSourceFactory>,
}

/// TLS settings for the agent → controller channel.
#[derive(Debug, Clone, Default)]
pub struct AgentTls {
    /// CA bundle to verify the controller certificate.
    pub ca_pem: Option<PathBuf>,
    /// Client certificate for mTLS.
    pub cert_pem: Option<PathBuf>,
    /// Client private key for mTLS.
    pub key_pem: Option<PathBuf>,
    /// Override the TLS server name (when connecting by IP).
    pub domain: Option<String>,
}

/// Agent configuration.
#[derive(Clone)]
pub struct AgentConfig {
    /// Controller endpoint, e.g. `http://10.0.0.1:7777` or `https://...`.
    pub controller_addr: String,
    /// Stable agent identity. Defaults to a fresh UUID; set it explicitly to
    /// resume the same identity across restarts.
    pub agent_id: Option<String>,
    pub agent_name: String,
    pub labels: HashMap<String, String>,
    pub tls: Option<AgentTls>,
    /// Where assignment data files are materialized (`<work_dir>/<run_id>/`).
    pub work_dir: PathBuf,
    pub deps: RunnerDeps,
}

/// The run currently executing (or armed and waiting for `Start`).
struct ActiveRun {
    run_id: String,
    handle: RunHandle,
    start_tx: Option<oneshot::Sender<i64>>,
}

type SharedRun = Arc<Mutex<Option<ActiveRun>>>;

enum SessionEnd {
    Shutdown,
    Disconnected,
}

/// The load agent. [`Agent::run`] connects, registers and serves assignments
/// until the shutdown token fires, reconnecting with backoff on stream errors.
pub struct Agent;

impl Agent {
    /// Run the agent loop to completion (until `shutdown` is cancelled).
    pub async fn run(config: AgentConfig, shutdown: CancellationToken) -> Result<(), AgentError> {
        let agent_id = config
            .agent_id
            .clone()
            .unwrap_or_else(|| uuid::Uuid::new_v4().to_string());
        let endpoint = build_endpoint(&config)?;
        let current: SharedRun = Arc::new(Mutex::new(None));
        // The uplink outlives individual connections so run events and metric
        // batches queued during a reconnect are delivered afterwards.
        let (uplink_tx, mut uplink_rx) = mpsc::channel::<pb::AgentMessage>(256);
        let mut backoff = Duration::from_millis(500);

        while !shutdown.is_cancelled() {
            let outcome = match endpoint.connect().await {
                Ok(channel) => {
                    run_session(
                        channel,
                        &config,
                        &agent_id,
                        &current,
                        &uplink_tx,
                        &mut uplink_rx,
                        &shutdown,
                    )
                    .await
                }
                Err(e) => Err(AgentError::Transport(format!("connect failed: {e}"))),
            };
            match outcome {
                Ok(SessionEnd::Shutdown) => break,
                Ok(SessionEnd::Disconnected) => {
                    backoff = Duration::from_millis(500);
                    tracing::warn!(agent_id = %agent_id, "controller connection lost; reconnecting");
                }
                Err(e) => {
                    tracing::warn!(agent_id = %agent_id, error = %e, "controller connection failed");
                }
            }
            if shutdown.is_cancelled() {
                break;
            }
            let jitter = Duration::from_millis(u64::from(now_unix_ms() as u32 % 250));
            tokio::select! {
                _ = tokio::time::sleep(backoff + jitter) => {}
                _ = shutdown.cancelled() => break,
            }
            backoff = (backoff * 2).min(Duration::from_secs(15));
        }

        if let Some(run) = current.lock().take() {
            run.handle.kill("agent shutting down");
        }
        Ok(())
    }
}

fn build_endpoint(config: &AgentConfig) -> Result<Endpoint, AgentError> {
    let mut endpoint = Channel::from_shared(config.controller_addr.clone())
        .map_err(|e| AgentError::Config(format!("invalid controller address: {e}")))?
        .connect_timeout(Duration::from_secs(5));
    if let Some(tls) = &config.tls {
        let mut tls_cfg = ClientTlsConfig::new();
        if let Some(ca) = &tls.ca_pem {
            tls_cfg = tls_cfg.ca_certificate(Certificate::from_pem(read_file(ca)?));
        }
        if let (Some(cert), Some(key)) = (&tls.cert_pem, &tls.key_pem) {
            tls_cfg = tls_cfg.identity(Identity::from_pem(read_file(cert)?, read_file(key)?));
        }
        if let Some(domain) = &tls.domain {
            tls_cfg = tls_cfg.domain_name(domain.clone());
        }
        endpoint = endpoint
            .tls_config(tls_cfg)
            .map_err(|e| AgentError::Tls(e.to_string()))?;
    }
    Ok(endpoint)
}

fn read_file(path: &Path) -> Result<Vec<u8>, AgentError> {
    std::fs::read(path).map_err(|e| AgentError::Io {
        path: path.display().to_string(),
        source: e,
    })
}

#[allow(clippy::too_many_arguments)]
async fn run_session(
    channel: Channel,
    config: &AgentConfig,
    agent_id: &str,
    current: &SharedRun,
    uplink_tx: &mpsc::Sender<pb::AgentMessage>,
    uplink_rx: &mut mpsc::Receiver<pb::AgentMessage>,
    shutdown: &CancellationToken,
) -> Result<SessionEnd, AgentError> {
    let mut client = CoordinationClient::new(channel);
    let (tx, rx) = mpsc::channel::<pb::AgentMessage>(64);

    // Queue Register before opening the stream: the controller only answers
    // the Session call once it has read the registration.
    let resume_run_id = current
        .lock()
        .as_ref()
        .map(|r| r.run_id.clone())
        .unwrap_or_default();
    let register = pb::AgentMessage {
        msg: Some(AgentMsg::Register(pb::Register {
            agent_id: agent_id.to_string(),
            agent_name: config.agent_name.clone(),
            protocol_version: PROTOCOL_VERSION,
            loadr_version: loadr_core::build_info::VERSION.to_string(),
            cpu_cores: std::thread::available_parallelism()
                .map(|n| n.get() as u32)
                .unwrap_or(1),
            labels: config.labels.clone(),
            resume_run_id,
            build_revision: loadr_core::build_info::GIT_REVISION.to_string(),
        })),
    };
    tx.try_send(register)
        .map_err(|_| AgentError::Transport("could not queue register message".into()))?;

    let mut inbound = client
        .session(ReceiverStream::new(rx))
        .await
        .map_err(|e| AgentError::Transport(format!("session open failed: {e}")))?
        .into_inner();

    let mut registered = false;
    let mut heartbeat = tokio::time::interval(Duration::from_secs(2));
    heartbeat.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);

    let end = |registered: bool| -> Result<SessionEnd, AgentError> {
        if registered {
            Ok(SessionEnd::Disconnected)
        } else {
            Err(AgentError::Transport(
                "disconnected before registration completed".into(),
            ))
        }
    };

    loop {
        tokio::select! {
            _ = shutdown.cancelled() => return Ok(SessionEnd::Shutdown),
            msg = uplink_rx.recv() => {
                if let Some(m) = msg {
                    if tx.send(m).await.is_err() {
                        return end(registered);
                    }
                }
            }
            res = inbound.message() => {
                match res {
                    Ok(Some(cm)) => {
                        handle_controller_message(cm, config, current, uplink_tx, &mut registered)
                            .await;
                    }
                    Ok(None) => return end(registered),
                    Err(status) => {
                        tracing::warn!(error = %status, "controller stream error");
                        return end(registered);
                    }
                }
            }
            _ = heartbeat.tick() => {
                if tx.send(make_heartbeat(current)).await.is_err() {
                    return end(registered);
                }
            }
        }
    }
}

async fn handle_controller_message(
    cm: pb::ControllerMessage,
    config: &AgentConfig,
    current: &SharedRun,
    uplink_tx: &mpsc::Sender<pb::AgentMessage>,
    registered: &mut bool,
) {
    match cm.msg {
        Some(CtrlMsg::Registered(r)) => {
            if r.protocol_version != PROTOCOL_VERSION {
                tracing::warn!(
                    controller = r.protocol_version,
                    agent = PROTOCOL_VERSION,
                    "coordination protocol version mismatch"
                );
            }
            *registered = true;
            tracing::info!(controller_id = %r.controller_id, "registered with controller");
        }
        Some(CtrlMsg::Assignment(a)) => {
            let run_id = a.run_id.clone();
            if let Err(detail) = handle_assignment(a, config, current, uplink_tx) {
                tracing::warn!(run_id = %run_id, error = %detail, "assignment failed");
                let _ = uplink_tx
                    .send(run_event(&run_id, "failed", detail, Vec::new()))
                    .await;
            }
        }
        Some(CtrlMsg::Start(s)) => {
            let mut cur = current.lock();
            if let Some(run) = cur.as_mut() {
                if run.run_id == s.run_id {
                    if let Some(start_tx) = run.start_tx.take() {
                        let _ = start_tx.send(s.start_unix_ms);
                    }
                }
            }
        }
        Some(CtrlMsg::Control(c)) => {
            let handle = current
                .lock()
                .as_ref()
                .filter(|r| r.run_id == c.run_id)
                .map(|r| r.handle.clone());
            let Some(handle) = handle else {
                let _ = uplink_tx
                    .send(control_ack(
                        &c,
                        Err("run is not active on this agent".to_string()),
                    ))
                    .await;
                return;
            };
            let result = match c.action.as_str() {
                "stop" => {
                    handle.stop("controller requested stop");
                    Ok(())
                }
                "kill" => {
                    handle.kill("controller requested kill");
                    Ok(())
                }
                "pause" => {
                    handle.pause(true);
                    Ok(())
                }
                "resume" => {
                    handle.pause(false);
                    Ok(())
                }
                "scale" => handle.scale(&c.scenario, c.value),
                other => Err(format!("unknown control action `{other}`")),
            };
            if let Err(error) = &result {
                tracing::warn!(action = %c.action, scenario = %c.scenario, %error, "control failed");
            }
            let _ = uplink_tx.send(control_ack(&c, result)).await;
        }
        None => {}
    }
}

fn control_ack(control: &pb::Control, result: Result<(), String>) -> pb::AgentMessage {
    let (applied, detail) = match result {
        Ok(()) => (true, String::new()),
        Err(detail) => (false, detail),
    };
    pb::AgentMessage {
        msg: Some(AgentMsg::ControlAck(pb::ControlAck {
            run_id: control.run_id.clone(),
            command_id: control.command_id,
            action: control.action.clone(),
            scenario: control.scenario.clone(),
            value: control.value,
            applied,
            detail,
        })),
    }
}

/// Materialize an assignment, build the engine and arm it behind the start
/// barrier. Returns a human-readable failure reason on error.
fn handle_assignment(
    a: pb::Assignment,
    config: &AgentConfig,
    current: &SharedRun,
    uplink_tx: &mpsc::Sender<pb::AgentMessage>,
) -> Result<(), String> {
    {
        let cur = current.lock();
        if let Some(run) = cur.as_ref() {
            if run.run_id == a.run_id {
                // Duplicate assignment (e.g. after a resume): keep the run.
                return Ok(());
            }
            if !matches!(run.handle.status(), RunStatus::Finished { .. }) {
                return Err(format!("agent is busy with run {}", run.run_id));
            }
        }
    }
    if a.partition_count == 0 || a.partition_index >= a.partition_count {
        return Err(format!(
            "invalid partition {}/{}",
            a.partition_index, a.partition_count
        ));
    }
    validate_run_id(&a.run_id).map_err(|e| e.to_string())?;
    let run_dir = config.work_dir.join(&a.run_id);
    std::fs::create_dir_all(&run_dir)
        .map_err(|e| format!("cannot create {}: {e}", run_dir.display()))?;
    materialize_files(&run_dir, &a.files).map_err(|e| e.to_string())?;

    let yaml =
        std::str::from_utf8(&a.plan_yaml).map_err(|e| format!("plan is not valid UTF-8: {e}"))?;
    let mut load_opts = loadr_config::LoadOptions::new();
    if !a.env.is_empty() {
        load_opts.env = Some(a.env.clone());
    }
    let loaded = loadr_config::load_str_with_base(yaml, &run_dir, &load_opts)
        .map_err(|e| format!("invalid plan: {e}"))?;
    let plan = loaded.plan;

    let protocols = (config.deps.protocols)(&plan, &run_dir)?;
    let data_sources = match &config.deps.data_sources {
        Some(factory) => factory(&plan.plugins, &run_dir)?,
        None => HashMap::new(),
    };
    let script = match (&plan.js, &config.deps.script) {
        (Some(js), Some(factory)) => Some(factory(js, &run_dir)?),
        (Some(_), None) => {
            return Err("plan uses JavaScript but this agent has no script engine".into())
        }
        _ => None,
    };

    let mut extra_tags = Tags::new();
    extra_tags.insert("instance".to_string(), config.agent_name.clone());

    let output = DeltaOutput {
        run_id: a.run_id.clone(),
        uplink: uplink_tx.clone(),
    };
    let engine = Engine::new(
        plan,
        run_dir,
        EngineOptions {
            run_id: Some(a.run_id.clone()),
            protocols,
            script,
            outputs: vec![Box::new(output)],
            partition: Some((a.partition_index, a.partition_count)),
            extra_tags,
            snapshot_interval: Duration::from_millis(500),
            data_sources,
        },
    )
    .map_err(|e| format!("engine setup failed: {e}"))?;

    let handle = engine.handle();
    let (start_tx, start_rx) = oneshot::channel::<i64>();
    *current.lock() = Some(ActiveRun {
        run_id: a.run_id.clone(),
        handle,
        start_tx: Some(start_tx),
    });
    spawn_run(
        engine,
        start_rx,
        a.run_id,
        uplink_tx.clone(),
        current.clone(),
    );
    Ok(())
}

/// Hold the engine ready, wait for the synchronized start, run to completion
/// and report the outcome.
fn spawn_run(
    engine: Engine,
    start_rx: oneshot::Receiver<i64>,
    run_id: String,
    uplink: mpsc::Sender<pb::AgentMessage>,
    current: SharedRun,
) {
    tokio::spawn(async move {
        let clear = |current: &SharedRun, run_id: &str| {
            let mut cur = current.lock();
            if cur.as_ref().map(|r| r.run_id == run_id).unwrap_or(false) {
                *cur = None;
            }
        };
        let Ok(start_ms) = start_rx.await else {
            tracing::debug!(run_id = %run_id, "assignment dropped before start");
            clear(&current, &run_id);
            return;
        };
        let now = now_unix_ms() as i64;
        if start_ms > now {
            tokio::time::sleep(Duration::from_millis((start_ms - now) as u64)).await;
        }
        let _ = uplink
            .send(run_event(&run_id, "started", String::new(), Vec::new()))
            .await;
        match engine.run().await {
            Ok(result) => {
                let summary_json = serde_json::to_vec(&result.summary).unwrap_or_default();
                let (kind, detail) = match result.aborted {
                    Some(reason) => ("aborted", reason),
                    None => ("finished", String::new()),
                };
                let _ = uplink
                    .send(run_event(&run_id, kind, detail, summary_json))
                    .await;
            }
            Err(e) => {
                let _ = uplink
                    .send(run_event(&run_id, "failed", e.to_string(), Vec::new()))
                    .await;
            }
        }
        clear(&current, &run_id);
    });
}

fn run_event(run_id: &str, kind: &str, detail: String, summary_json: Vec<u8>) -> pb::AgentMessage {
    pb::AgentMessage {
        msg: Some(AgentMsg::Event(pb::RunEvent {
            run_id: run_id.to_string(),
            kind: kind.to_string(),
            detail,
            summary_json,
        })),
    }
}

fn make_heartbeat(current: &SharedRun) -> pb::AgentMessage {
    let (run_id, run_state, active_vus) = match current.lock().as_ref() {
        Some(run) => {
            let state = match run.handle.status() {
                RunStatus::Pending => "pending",
                RunStatus::Running => "running",
                RunStatus::Stopping => "stopping",
                RunStatus::Finished { .. } => "finished",
            };
            let vus = run
                .handle
                .snapshot()
                .find("vus")
                .and_then(|s| s.agg.last)
                .unwrap_or(0.0);
            (run.run_id.clone(), state.to_string(), vus.max(0.0) as u64)
        }
        None => (String::new(), "idle".to_string(), 0),
    };
    pb::AgentMessage {
        msg: Some(AgentMsg::Heartbeat(pb::Heartbeat {
            active_vus,
            cpu_load: 0.0,
            run_id,
            run_state,
        })),
    }
}

/// Validate a data-file relative path: it must be relative and contain only
/// normal components (no `..`, no `.`, no root or prefix).
pub fn validate_data_file_path(path: &str) -> Result<(), AgentError> {
    if path.is_empty() {
        return Err(AgentError::Security("empty data file path".into()));
    }
    let p = Path::new(path);
    if p.is_absolute() || path.starts_with('/') || path.starts_with('\\') {
        return Err(AgentError::Security(format!(
            "absolute data file path `{path}` rejected"
        )));
    }
    if !p.components().all(|c| matches!(c, Component::Normal(_))) {
        return Err(AgentError::Security(format!(
            "data file path `{path}` must not contain `..`, `.` or a root"
        )));
    }
    Ok(())
}

/// Run ids become directory names; require a single normal path component.
fn validate_run_id(run_id: &str) -> Result<(), AgentError> {
    if run_id.is_empty() || run_id.contains('/') || run_id.contains('\\') || run_id.contains("..") {
        return Err(AgentError::Security(format!("invalid run id `{run_id}`")));
    }
    Ok(())
}

fn materialize_files(dir: &Path, files: &[pb::DataFile]) -> Result<(), AgentError> {
    for f in files {
        validate_data_file_path(&f.relative_path)?;
        let path = dir.join(&f.relative_path);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| AgentError::Io {
                path: parent.display().to_string(),
                source: e,
            })?;
        }
        std::fs::write(&path, &f.content).map_err(|e| AgentError::Io {
            path: path.display().to_string(),
            source: e,
        })?;
    }
    Ok(())
}

/// An [`Output`] that ships the engine aggregator's drained [`MetricsDelta`]s
/// to the controller: `wants_delta` opts in, so the engine takes the delta
/// directly from its own `Aggregator` instead of this output keeping a
/// second one just to re-record every sample.
struct DeltaOutput {
    run_id: String,
    uplink: mpsc::Sender<pb::AgentMessage>,
}

impl DeltaOutput {
    fn batch(&self, delta: &MetricsDelta) -> Option<pb::AgentMessage> {
        let delta_json = serde_json::to_vec(delta).ok()?;
        Some(pb::AgentMessage {
            msg: Some(AgentMsg::Metrics(pb::MetricsBatch {
                run_id: self.run_id.clone(),
                delta_json,
            })),
        })
    }
}

#[async_trait]
impl Output for DeltaOutput {
    fn name(&self) -> &str {
        "controller-delta"
    }

    fn wants_samples(&self) -> bool {
        false
    }

    fn wants_delta(&self) -> bool {
        true
    }

    async fn on_delta(&mut self, delta: &MetricsDelta, last: bool) -> bool {
        let Some(msg) = self.batch(delta) else {
            // Not retryable: the data can't be serialized, ever.
            return true;
        };
        if last {
            // The run is ending: block until the controller has it rather
            // than dropping the final delta.
            let _ = self.uplink.send(msg).await;
            true
        } else {
            // Never block the aggregator: when the uplink is congested (e.g.
            // a reconnect in progress) report rejection so the caller can
            // restore the delta and retry next tick.
            self.uplink.try_send(msg).is_ok()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use loadr_core::{Aggregator, MetricKind, Sample, Tags};

    /// A one-series delta, built the same way the engine's aggregator would.
    fn one_delta() -> MetricsDelta {
        let mut agg = Aggregator::new();
        agg.record(&Sample {
            metric: Arc::from("http_reqs"),
            kind: MetricKind::Counter,
            value: 1.0,
            tags: Arc::new(Tags::new()),
            timestamp_ms: 0,
        });
        agg.take_delta()
    }

    fn filler() -> pb::AgentMessage {
        pb::AgentMessage {
            msg: Some(AgentMsg::Heartbeat(pb::Heartbeat::default())),
        }
    }

    #[tokio::test]
    async fn delta_output_backpressure() {
        let (tx, mut rx) = mpsc::channel(1);
        let mut output = DeltaOutput {
            run_id: "run-1".to_string(),
            uplink: tx,
        };
        let delta = one_delta();

        // Congested: the channel's one slot is already occupied, so a
        // non-final delta must be rejected (never block the aggregator).
        output.uplink.try_send(filler()).expect("prefill");
        assert!(
            !output.on_delta(&delta, false).await,
            "on_delta should report rejection when the uplink is congested"
        );

        // Drained: capacity is free, so the same delta is now accepted and
        // decodes back to the same delta JSON.
        rx.recv().await.expect("filler");
        assert!(output.on_delta(&delta, false).await);
        let msg = rx.recv().await.expect("delta message");
        let Some(AgentMsg::Metrics(batch)) = msg.msg else {
            panic!("expected a Metrics message");
        };
        assert_eq!(batch.run_id, "run-1");
        assert_eq!(batch.delta_json, serde_json::to_vec(&delta).unwrap());

        // Final flush: congest the channel again, then confirm on_delta
        // blocks (rather than dropping the last delta) until room frees up.
        output.uplink.try_send(filler()).expect("prefill 2");
        assert!(
            tokio::time::timeout(Duration::from_millis(50), output.on_delta(&delta, true))
                .await
                .is_err(),
            "on_delta(last=true) should block while the channel is full"
        );
        rx.recv().await.expect("second filler");
        assert!(
            tokio::time::timeout(Duration::from_millis(50), output.on_delta(&delta, true))
                .await
                .expect("send should complete once room frees up"),
            "final delta should be accepted"
        );
    }
}
