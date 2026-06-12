//! # loadr-plugin-webui
//!
//! The embedded management web UI for [loadr](https://loadr.io) — live
//! dashboards, run control (stop/kill/pause/scale), a YAML test library with
//! validation, agent status and backend logs, served from a single binary.
//!
//! The UI is decoupled from how runs are launched through the [`UiBackend`]
//! trait: the CLI implements it in standalone mode, the controller implements
//! it in distributed mode. A complete reference implementation,
//! [`LocalBackend`], lives in this crate and drives real
//! [`loadr_core::Engine`] runs through an injected [`EngineLauncher`].
//!
//! ```no_run
//! # async fn demo(backend: std::sync::Arc<dyn loadr_plugin_webui::UiBackend>) {
//! use loadr_plugin_webui::{AuthConfig, WebUi, WebUiConfig};
//!
//! let handle = WebUi::serve(WebUiConfig {
//!     bind: "127.0.0.1:6464".parse().expect("addr"),
//!     auth: AuthConfig::default(),
//!     backend,
//! })
//! .await
//! .expect("serve");
//! println!("web UI listening on http://{}", handle.addr);
//! # }
//! ```

pub mod backend;
pub mod server;

mod api;
mod payload;
mod stream;

use std::collections::BTreeMap;
use std::sync::Arc;

pub use backend::{EngineLauncher, LauncherResult, LocalBackend, WebUiLogLayer};
pub use server::{AuthConfig, WebUi, WebUiConfig, WebUiError, WebUiHandle};

/// One run as the UI sees it (live or historical).
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct RunInfo {
    pub run_id: String,
    pub name: Option<String>,
    /// `pending` | `running` | `stopping` | `finished` | `failed`.
    pub state: String,
    /// All thresholds passed (set once the run ended).
    pub passed: Option<bool>,
    pub started_ms: u64,
    pub ended_ms: Option<u64>,
    pub scenarios: Vec<String>,
    /// Agent ids participating in the run (empty in standalone mode).
    pub agents: Vec<String>,
}

/// One load-generation agent (distributed mode).
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct AgentView {
    pub id: String,
    pub name: String,
    pub healthy: bool,
    pub active_vus: u64,
    pub cores: u32,
    pub last_heartbeat_ms: u64,
    pub labels: BTreeMap<String, String>,
}

/// A saved test definition in the test library.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct StoredTest {
    pub name: String,
    pub yaml: String,
    pub updated_ms: u64,
}

/// One backend log line for the Logs page.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct LogLine {
    pub ts_ms: u64,
    /// `trace` | `debug` | `info` | `warn` | `error`.
    pub level: String,
    pub message: String,
}

/// Everything the web UI needs from whatever is orchestrating runs.
///
/// Implemented by the CLI (standalone) and the controller (distributed);
/// [`LocalBackend`] is the in-process reference implementation.
#[async_trait::async_trait]
pub trait UiBackend: Send + Sync + 'static {
    /// Validate and start a test from YAML; returns the new run id.
    async fn start_test(
        &self,
        name: Option<String>,
        yaml: String,
        env: Option<String>,
    ) -> Result<String, String>;

    /// All known runs, newest first.
    fn runs(&self) -> Vec<RunInfo>;

    /// Live control handle for a run (None once it finished).
    fn run_handle(&self, run_id: &str) -> Option<loadr_core::RunHandle>;

    /// Latest snapshot: live for running runs, last-known for finished ones.
    fn run_snapshot(&self, run_id: &str) -> Option<Arc<loadr_core::Snapshot>>;

    /// Current threshold statuses for a run.
    fn run_thresholds(&self, run_id: &str) -> Vec<loadr_core::ThresholdStatus>;

    /// End-of-run summary (None while the run is still live).
    fn run_summary(&self, run_id: &str) -> Option<loadr_core::Summary>;

    /// Stop a run: graceful by default, immediate when `kill` is set.
    async fn stop_run(&self, run_id: &str, kill: bool) -> Result<(), String>;

    /// Pause or resume a run.
    async fn pause_run(&self, run_id: &str, paused: bool) -> Result<(), String>;

    /// Scale an externally-controlled scenario to `vus`.
    async fn scale_run(&self, run_id: &str, scenario: &str, vus: u64) -> Result<(), String>;

    /// Connected agents (empty in standalone mode).
    fn agents(&self) -> Vec<AgentView>;

    /// The saved test library.
    fn tests(&self) -> Vec<StoredTest>;

    fn save_test(&self, name: String, yaml: String) -> Result<(), String>;

    fn delete_test(&self, name: &str) -> Result<(), String>;

    /// Recent backend log lines, oldest first.
    fn recent_logs(&self) -> Vec<LogLine>;
}

/// Milliseconds since the Unix epoch.
pub(crate) fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}
