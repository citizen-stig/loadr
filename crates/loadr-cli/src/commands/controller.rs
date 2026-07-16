//! `loadr controller` — the distributed-mode control plane: coordination gRPC
//! server + management web UI + optional Prometheus endpoint.

use std::path::PathBuf;
use std::sync::Arc;

use clap::Args;
use loadr_core::{AggValues, MetricKind, SeriesSnapshot, Snapshot, Tags};
use owo_colors::OwoColorize;

#[derive(Args)]
pub struct ControllerArgs {
    /// Coordination gRPC bind address
    #[arg(long, default_value = "127.0.0.1:7625")]
    pub bind: std::net::SocketAddr,
    /// Web UI / REST API bind address
    #[arg(long, default_value = "127.0.0.1:6464")]
    pub ui_bind: std::net::SocketAddr,
    /// Web UI basic-auth username
    #[arg(long)]
    pub ui_user: Option<String>,
    /// Web UI basic-auth password
    #[arg(long, env = "LOADR_UI_PASSWORD", hide_env_values = true)]
    pub ui_password: Option<String>,
    /// Web UI bearer token (repeatable)
    #[arg(long)]
    pub ui_token: Vec<String>,
    /// Serve a Prometheus scrape endpoint with fleet-wide metrics
    #[arg(long)]
    pub prometheus_listen: Option<String>,
    /// TLS certificate for the coordination listener (PEM)
    #[arg(long, requires = "tls_key")]
    pub tls_cert: Option<PathBuf>,
    /// TLS private key (PEM)
    #[arg(long, requires = "tls_cert")]
    pub tls_key: Option<PathBuf>,
    /// Require agent client certificates signed by this CA (mTLS)
    #[arg(long, requires = "tls_cert")]
    pub tls_client_ca: Option<PathBuf>,
    /// Directory for the test library and run history
    #[arg(long, default_value_os_t = default_storage_dir())]
    pub storage_dir: PathBuf,
    /// Seconds without traffic before an agent is considered lost
    #[arg(long, default_value = "6")]
    pub agent_liveness: u64,
}

fn default_storage_dir() -> PathBuf {
    dirs_home().join(".loadr").join("controller")
}

pub fn dirs_home() -> PathBuf {
    std::env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."))
}

pub fn execute(args: ControllerArgs) -> anyhow::Result<i32> {
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()?;
    runtime.block_on(async move {
        let tls = match (&args.tls_cert, &args.tls_key) {
            (Some(cert), Some(key)) => Some(loadr_agent::ControllerTls {
                cert_pem: cert.clone(),
                key_pem: key.clone(),
                client_ca_pem: args.tls_client_ca.clone(),
            }),
            _ => None,
        };
        let controller = loadr_agent::Controller::start(loadr_agent::ControllerConfig {
            bind: args.bind,
            tls,
            agent_liveness: std::time::Duration::from_secs(args.agent_liveness),
        })
        .await?;
        eprintln!(
            "{} controller listening on {} (agents join with: loadr agent --join {})",
            "→".cyan(),
            controller.addr(),
            controller.addr()
        );

        std::fs::create_dir_all(&args.storage_dir)?;
        let backend = Arc::new(ControllerBackend {
            controller: controller.clone(),
            storage_dir: args.storage_dir.clone(),
        });

        let mut auth = loadr_plugin_webui::AuthConfig::default();
        if let (Some(user), Some(pass)) = (&args.ui_user, &args.ui_password) {
            auth.basic = Some((user.clone(), pass.clone()));
        }
        auth.tokens = args.ui_token.clone();
        let ui = loadr_plugin_webui::WebUi::serve(loadr_plugin_webui::WebUiConfig {
            bind: args.ui_bind,
            auth,
            backend: backend.clone(),
        })
        .await?;
        eprintln!("{} web UI at http://{}/", "→".cyan(), ui.addr);

        // Optional fleet-wide Prometheus endpoint fed from run snapshots.
        let mut prom_task = None;
        if let Some(listen) = &args.prometheus_listen {
            let mut output = loadr_outputs::PrometheusOutput::new(
                Some(listen.clone()),
                None,
                std::time::Duration::from_secs(5),
            );
            use loadr_core::Output as _;
            output
                .start()
                .await
                .map_err(|e| anyhow::anyhow!("prometheus endpoint: {e}"))?;
            if let Some(addr) = output.bound_addr() {
                eprintln!("{} prometheus metrics at http://{addr}/metrics", "→".cyan());
            }
            let controller = controller.clone();
            prom_task = Some(tokio::spawn(async move {
                let mut output = output;
                let mut ticker = tokio::time::interval(std::time::Duration::from_secs(1));
                loop {
                    ticker.tick().await;
                    let runs = controller.runs();
                    let views: Vec<_> = select_prometheus_runs(&runs)
                        .into_iter()
                        .filter_map(|run| controller.run_metrics_view(&run.run_id))
                        .collect();
                    let snapshot = prometheus_snapshot(&views);
                    output.on_snapshot(&snapshot).await;
                }
            }));
        }

        eprintln!("  Ctrl-C to shut down");
        let _ = tokio::signal::ctrl_c().await;
        eprintln!("\nshutting down...");
        if let Some(t) = prom_task {
            t.abort();
        }
        ui.shutdown().await;
        controller.shutdown();
        Ok(0)
    })
}

fn select_prometheus_runs(
    runs: &[loadr_agent::RunSummaryInfo],
) -> Vec<&loadr_agent::RunSummaryInfo> {
    let active: Vec<_> = runs
        .iter()
        .filter(|run| run.state == "pending" || run.state == "running")
        .collect();
    if active.is_empty() {
        // `runs()` is oldest-first, so retain only the newest completed run
        // when the fleet is idle.
        runs.last().into_iter().collect()
    } else {
        active
    }
}

fn prometheus_snapshot(views: &[loadr_agent::RunMetricsView]) -> Snapshot {
    let mut snapshot = Snapshot {
        timestamp_ms: views
            .iter()
            .map(|view| view.detailed.timestamp_ms)
            .max()
            .unwrap_or_default(),
        elapsed_secs: views
            .iter()
            .map(|view| view.detailed.elapsed_secs)
            .fold(0.0, f64::max),
        ..Snapshot::default()
    };

    for view in views {
        let run_tags = run_tags(view);
        for series in &view.detailed.series {
            let mut series = series.clone();
            // `instance` was historically used internally for thresholds. It
            // would collide with Prometheus' scrape target label, while the
            // trusted loadr_agent/loadr_agent_id labels preserve identity.
            series.tags.remove("instance");
            add_run_tags(&mut series.tags, view);
            snapshot.series.push(series);
        }
        for fleet in &view.fleet {
            snapshot.series.push(SeriesSnapshot {
                metric: format!("fleet_{}", fleet.metric),
                kind: fleet.kind,
                tags: run_tags.clone(),
                agg: fleet.agg.clone(),
                interval_count: 0,
                interval_sum: 0.0,
            });
        }
        let mut info_tags = run_tags.clone();
        info_tags.insert("state".to_string(), view.state.clone());
        snapshot
            .series
            .push(gauge_series("fleet_run_info", info_tags, 1.0));
        snapshot.series.push(gauge_series(
            "fleet_run_started_timestamp_seconds",
            run_tags,
            view.started_ms as f64 / 1000.0,
        ));
    }
    snapshot
        .series
        .sort_by(|a, b| a.metric.cmp(&b.metric).then_with(|| a.tags.cmp(&b.tags)));
    snapshot
}

fn run_tags(view: &loadr_agent::RunMetricsView) -> Tags {
    let mut tags = Tags::new();
    add_run_tags(&mut tags, view);
    tags
}

fn add_run_tags(tags: &mut Tags, view: &loadr_agent::RunMetricsView) {
    tags.insert("loadr_run_id".to_string(), view.run_id.clone());
    tags.insert(
        "loadr_run_name".to_string(),
        view.name.clone().unwrap_or_default(),
    );
}

fn gauge_series(metric: &str, tags: Tags, value: f64) -> SeriesSnapshot {
    SeriesSnapshot {
        metric: metric.to_string(),
        kind: MetricKind::Gauge,
        tags,
        agg: AggValues {
            count: 1,
            sum: value,
            min: Some(value),
            max: Some(value),
            last: Some(value),
            ..AggValues::default()
        },
        interval_count: 0,
        interval_sum: 0.0,
    }
}

/// `UiBackend` over a distributed `ControllerHandle`.
struct ControllerBackend {
    controller: loadr_agent::ControllerHandle,
    storage_dir: PathBuf,
}

impl ControllerBackend {
    fn tests_dir(&self) -> PathBuf {
        self.storage_dir.join("tests")
    }

    /// Gather files a plan references, reading from the controller's disk
    /// (tests dir first, then storage dir) so agents receive them.
    fn collect_files(&self, plan: &loadr_config::TestPlan) -> Vec<(String, Vec<u8>)> {
        let mut wanted: Vec<String> = Vec::new();
        for source in plan.data.values() {
            if let loadr_config::DataSource::Csv { path, .. } = source {
                wanted.push(path.display().to_string());
            }
        }
        if let Some(js) = &plan.js {
            if let Some(file) = &js.file {
                wanted.push(file.display().to_string());
            }
        }
        for scenario in plan.scenarios.values() {
            collect_step_files(&scenario.flow, &mut wanted);
        }
        let mut out = Vec::new();
        for rel in wanted {
            if rel.starts_with('/') || rel.contains("..") {
                continue;
            }
            for base in [self.tests_dir(), self.storage_dir.clone()] {
                let candidate = base.join(&rel);
                if let Ok(content) = std::fs::read(&candidate) {
                    out.push((rel.clone(), content));
                    break;
                }
            }
        }
        out
    }
}

fn collect_step_files(steps: &[loadr_config::Step], wanted: &mut Vec<String>) {
    for step in steps {
        match step {
            loadr_config::Step::Request(r) => {
                if let Some(loadr_config::Body::Spec(spec)) = &r.body {
                    if let Some(f) = &spec.file {
                        wanted.push(f.display().to_string());
                    }
                }
                if let Some(grpc) = &r.grpc {
                    for f in &grpc.proto_files {
                        wanted.push(f.display().to_string());
                    }
                }
            }
            loadr_config::Step::Group(g) => collect_step_files(&g.steps, wanted),
            _ => {}
        }
    }
}

#[async_trait::async_trait]
impl loadr_plugin_webui::UiBackend for ControllerBackend {
    async fn start_test(
        &self,
        name: Option<String>,
        yaml: String,
        env: Option<String>,
    ) -> Result<String, String> {
        let loaded = loadr_config::load_str(&yaml, &loadr_config::LoadOptions::new())
            .map_err(|e| e.to_string())?;
        let files = self.collect_files(&loaded.plan);
        self.controller
            .submit(
                yaml,
                loadr_agent::SubmitOptions {
                    env,
                    name,
                    files,
                    agent_filter: None,
                    on_agent_loss: Default::default(),
                    start_barrier: std::time::Duration::from_secs(2),
                },
            )
            .await
            .map_err(|e| e.to_string())
    }

    fn runs(&self) -> Vec<loadr_plugin_webui::RunInfo> {
        self.controller
            .runs()
            .into_iter()
            .map(|r| {
                let summary = self.controller.run_summary(&r.run_id);
                loadr_plugin_webui::RunInfo {
                    run_id: r.run_id.clone(),
                    name: r.name,
                    state: r.state.clone(),
                    passed: summary.as_ref().map(|s| s.thresholds_passed),
                    started_ms: r.started_ms,
                    ended_ms: summary.as_ref().map(|s| s.ended_ms),
                    scenarios: summary.map(|s| s.scenarios).unwrap_or_default(),
                    agents: r.agents,
                }
            })
            .collect()
    }

    fn run_handle(&self, _run_id: &str) -> Option<loadr_core::RunHandle> {
        None // distributed runs stream via backend polling
    }

    fn run_snapshot(&self, run_id: &str) -> Option<Arc<loadr_core::Snapshot>> {
        self.controller
            .watch_run(run_id)
            .map(|rx| rx.borrow().clone())
            .or_else(|| {
                self.controller
                    .run_summary(run_id)
                    .map(|s| Arc::new(s.snapshot))
            })
    }

    fn run_thresholds(&self, run_id: &str) -> Vec<loadr_core::ThresholdStatus> {
        self.controller.run_thresholds(run_id)
    }

    fn run_summary(&self, run_id: &str) -> Option<loadr_core::Summary> {
        self.controller.run_summary(run_id)
    }

    async fn stop_run(&self, run_id: &str, kill: bool) -> Result<(), String> {
        if kill {
            self.controller
                .kill_run(run_id)
                .await
                .map_err(|e| e.to_string())
        } else {
            self.controller
                .stop_run(run_id)
                .await
                .map_err(|e| e.to_string())
        }
    }

    async fn pause_run(&self, run_id: &str, paused: bool) -> Result<(), String> {
        self.controller
            .pause_run(run_id, paused)
            .await
            .map_err(|e| e.to_string())
    }

    async fn scale_run(&self, run_id: &str, scenario: &str, vus: u64) -> Result<(), String> {
        self.controller
            .scale(run_id, scenario, vus)
            .await
            .map_err(|e| e.to_string())
    }

    fn agents(&self) -> Vec<loadr_plugin_webui::AgentView> {
        self.controller
            .agents()
            .into_iter()
            .map(|a| loadr_plugin_webui::AgentView {
                id: a.id,
                name: a.name,
                healthy: a.healthy,
                active_vus: a.active_vus,
                cores: a.cores,
                // The controller reports `last_heartbeat_ms` as a delta (ms since
                // the last beat); the UI expects an absolute epoch timestamp.
                last_heartbeat_ms: loadr_core::metrics::now_millis()
                    .saturating_sub(a.last_heartbeat_ms),
                labels: a.labels.into_iter().collect(),
            })
            .collect()
    }

    fn tests(&self) -> Vec<loadr_plugin_webui::StoredTest> {
        let dir = self.tests_dir();
        let mut out = Vec::new();
        if let Ok(entries) = std::fs::read_dir(&dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.extension().and_then(|e| e.to_str()) == Some("yaml") {
                    if let Ok(yaml) = std::fs::read_to_string(&path) {
                        let updated_ms = entry
                            .metadata()
                            .and_then(|m| m.modified())
                            .ok()
                            .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                            .map(|d| d.as_millis() as u64)
                            .unwrap_or(0);
                        out.push(loadr_plugin_webui::StoredTest {
                            name: path
                                .file_stem()
                                .map(|s| s.to_string_lossy().to_string())
                                .unwrap_or_default(),
                            yaml,
                            updated_ms,
                        });
                    }
                }
            }
        }
        out.sort_by(|a, b| a.name.cmp(&b.name));
        out
    }

    fn save_test(&self, name: String, yaml: String) -> Result<(), String> {
        if name.is_empty() || name.contains(['/', '\\']) || name.contains("..") {
            return Err("invalid test name".into());
        }
        let dir = self.tests_dir();
        std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
        std::fs::write(dir.join(format!("{name}.yaml")), yaml).map_err(|e| e.to_string())
    }

    fn delete_test(&self, name: &str) -> Result<(), String> {
        if name.contains(['/', '\\']) || name.contains("..") {
            return Err("invalid test name".into());
        }
        std::fs::remove_file(self.tests_dir().join(format!("{name}.yaml")))
            .map_err(|e| e.to_string())
    }

    fn recent_logs(&self) -> Vec<loadr_plugin_webui::LogLine> {
        Vec::new()
    }
}

// ---------------------------------------------------------------------------
// Small HTTP client helpers shared with `loadr run --controller`.
// ---------------------------------------------------------------------------

pub type HttpClient = hyper_util::client::legacy::Client<
    hyper_util::client::legacy::connect::HttpConnector,
    http_body_util::Full<bytes::Bytes>,
>;

pub fn http_client() -> HttpClient {
    hyper_util::client::legacy::Client::builder(hyper_util::rt::TokioExecutor::new()).build_http()
}

pub async fn http_json(
    client: &HttpClient,
    method: http::Method,
    url: &str,
    body: Option<&serde_json::Value>,
) -> anyhow::Result<serde_json::Value> {
    let mut builder = http::Request::builder().method(method).uri(url);
    if body.is_some() {
        builder = builder.header(http::header::CONTENT_TYPE, "application/json");
    }
    let request = builder.body(http_body_util::Full::new(bytes::Bytes::from(
        body.map(serde_json::to_vec)
            .transpose()?
            .unwrap_or_default(),
    )))?;
    let response = client.request(request).await?;
    let status = response.status();
    use http_body_util::BodyExt as _;
    let bytes = response.into_body().collect().await?.to_bytes();
    let value: serde_json::Value = serde_json::from_slice(&bytes)
        .unwrap_or_else(|_| serde_json::json!({"raw": String::from_utf8_lossy(&bytes)}));
    if !status.is_success() {
        anyhow::bail!("{url} returned {status}: {value}");
    }
    Ok(value)
}

#[cfg(test)]
mod prometheus_tests {
    use super::*;

    #[test]
    fn projection_separates_detailed_and_fleet_series() {
        let mut detailed_tags = Tags::new();
        detailed_tags.insert("instance".into(), "legacy-agent".into());
        detailed_tags.insert("loadr_agent".into(), "worker-a".into());
        detailed_tags.insert("loadr_agent_id".into(), "agent-a-id".into());
        let detailed = Snapshot {
            timestamp_ms: 1_700_000_000_000,
            elapsed_secs: 12.0,
            interval_secs: 0.0,
            series: vec![SeriesSnapshot {
                metric: "http_reqs".into(),
                kind: MetricKind::Counter,
                tags: detailed_tags,
                agg: AggValues {
                    count: 7,
                    sum: 7.0,
                    ..AggValues::default()
                },
                interval_count: 0,
                interval_sum: 0.0,
            }],
        };
        let views = vec![loadr_agent::RunMetricsView {
            run_id: "run-1".into(),
            name: Some("checkout".into()),
            state: "running".into(),
            started_ms: 1_700_000_000_000,
            detailed,
            fleet: vec![loadr_agent::FleetMetric {
                metric: "http_reqs".into(),
                kind: MetricKind::Counter,
                agg: AggValues {
                    count: 12,
                    sum: 12.0,
                    ..AggValues::default()
                },
            }],
        }];

        let projected = prometheus_snapshot(&views);
        let detailed = projected
            .series
            .iter()
            .find(|series| series.metric == "http_reqs")
            .expect("detailed series");
        assert!(!detailed.tags.contains_key("instance"));
        assert_eq!(detailed.tags["loadr_agent"], "worker-a");
        assert_eq!(detailed.tags["loadr_run_id"], "run-1");

        let fleet = projected
            .series
            .iter()
            .find(|series| series.metric == "fleet_http_reqs")
            .expect("fleet series");
        assert_eq!(fleet.agg.sum, 12.0);
        assert!(!fleet.tags.contains_key("loadr_agent"));
        assert_eq!(fleet.tags["loadr_run_name"], "checkout");

        let info = projected
            .series
            .iter()
            .find(|series| series.metric == "fleet_run_info")
            .expect("run info");
        assert_eq!(info.tags["state"], "running");
        assert_eq!(info.agg.last, Some(1.0));
    }

    #[test]
    fn selects_all_active_runs_or_newest_completed() {
        let run = |id: &str, state: &str, started_ms: u64| loadr_agent::RunSummaryInfo {
            run_id: id.into(),
            name: None,
            state: state.into(),
            started_ms,
            agents: Vec::new(),
        };
        let runs = vec![
            run("old", "finished", 1),
            run("active-a", "running", 2),
            run("active-b", "pending", 3),
        ];
        let selected: Vec<_> = select_prometheus_runs(&runs)
            .into_iter()
            .map(|run| run.run_id.as_str())
            .collect();
        assert_eq!(selected, ["active-a", "active-b"]);

        let completed = vec![run("old", "finished", 1), run("new", "failed", 2)];
        let selected = select_prometheus_runs(&completed);
        assert_eq!(selected.len(), 1);
        assert_eq!(selected[0].run_id, "new");
    }
}
