//! Metric primitives: kinds, samples, the metric registry and the sample bus.

use std::collections::{BTreeMap, HashMap};
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use parking_lot::RwLock;
use serde::{Deserialize, Serialize};

/// Sorted tag set attached to samples.
pub type Tags = BTreeMap<String, String>;

/// The four metric kinds, matching k6.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MetricKind {
    /// Monotonically accumulating sum.
    Counter,
    /// Last value (also tracks min/max).
    Gauge,
    /// Fraction of non-zero samples.
    Rate,
    /// Distribution (HDR histogram): percentiles, avg, min, max.
    Trend,
}

impl MetricKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            MetricKind::Counter => "counter",
            MetricKind::Gauge => "gauge",
            MetricKind::Rate => "rate",
            MetricKind::Trend => "trend",
        }
    }
}

impl From<loadr_config::MetricKindSpec> for MetricKind {
    fn from(spec: loadr_config::MetricKindSpec) -> Self {
        match spec {
            loadr_config::MetricKindSpec::Counter => MetricKind::Counter,
            loadr_config::MetricKindSpec::Gauge => MetricKind::Gauge,
            loadr_config::MetricKindSpec::Rate => MetricKind::Rate,
            loadr_config::MetricKindSpec::Trend => MetricKind::Trend,
        }
    }
}

/// Definition of a metric.
#[derive(Debug, Clone)]
pub struct MetricDef {
    pub name: Arc<str>,
    pub kind: MetricKind,
    /// Trend values are durations in milliseconds.
    pub time: bool,
    pub description: Option<String>,
}

/// Built-in metrics (name, kind, is-time).
pub const BUILTIN_METRIC_DEFS: &[(&str, MetricKind, bool)] = &[
    ("http_reqs", MetricKind::Counter, false),
    ("http_req_duration", MetricKind::Trend, true),
    ("http_req_blocked", MetricKind::Trend, true),
    ("http_req_connecting", MetricKind::Trend, true),
    ("http_req_tls_handshaking", MetricKind::Trend, true),
    ("http_req_sending", MetricKind::Trend, true),
    ("http_req_waiting", MetricKind::Trend, true),
    ("http_req_receiving", MetricKind::Trend, true),
    ("http_req_failed", MetricKind::Rate, false),
    ("iterations", MetricKind::Counter, false),
    ("iteration_duration", MetricKind::Trend, true),
    ("dropped_iterations", MetricKind::Counter, false),
    ("vus", MetricKind::Gauge, false),
    ("vus_max", MetricKind::Gauge, false),
    ("checks", MetricKind::Rate, false),
    // Script (JS) exceptions raised in hooks, exec functions, and js steps.
    // Tagged with `exception` (a normalised message) and `scenario`.
    ("vu_exceptions", MetricKind::Counter, false),
    ("data_sent", MetricKind::Counter, false),
    ("data_received", MetricKind::Counter, false),
    ("ws_connecting", MetricKind::Trend, true),
    ("ws_session_duration", MetricKind::Trend, true),
    ("ws_msgs_sent", MetricKind::Counter, false),
    ("ws_msgs_received", MetricKind::Counter, false),
    ("grpc_reqs", MetricKind::Counter, false),
    ("grpc_req_duration", MetricKind::Trend, true),
    ("tcp_reqs", MetricKind::Counter, false),
    ("tcp_req_duration", MetricKind::Trend, true),
    ("udp_reqs", MetricKind::Counter, false),
    ("udp_req_duration", MetricKind::Trend, true),
    ("graphql_reqs", MetricKind::Counter, false),
    ("graphql_req_duration", MetricKind::Trend, true),
    ("sql_reqs", MetricKind::Counter, false),
    ("sql_req_duration", MetricKind::Trend, true),
    ("sql_rows", MetricKind::Counter, false),
];

/// Registry of known metrics: built-ins, YAML custom metrics, and metrics
/// created at runtime from JS.
#[derive(Debug, Default)]
pub struct MetricRegistry {
    defs: RwLock<HashMap<Arc<str>, MetricDef>>,
}

impl MetricRegistry {
    pub fn with_builtins() -> Self {
        let reg = MetricRegistry::default();
        {
            let mut defs = reg.defs.write();
            for (name, kind, time) in BUILTIN_METRIC_DEFS {
                let name: Arc<str> = Arc::from(*name);
                defs.insert(
                    name.clone(),
                    MetricDef {
                        name,
                        kind: *kind,
                        time: *time,
                        description: None,
                    },
                );
            }
        }
        reg
    }

    /// Register a metric; returns an error when re-registering with a different kind.
    pub fn register(
        &self,
        name: &str,
        kind: MetricKind,
        time: bool,
        description: Option<String>,
    ) -> Result<Arc<str>, String> {
        let mut defs = self.defs.write();
        if let Some(existing) = defs.get(name) {
            if existing.kind != kind {
                return Err(format!(
                    "metric `{name}` already registered as {}, cannot redefine as {}",
                    existing.kind.as_str(),
                    kind.as_str()
                ));
            }
            return Ok(existing.name.clone());
        }
        let arc: Arc<str> = Arc::from(name);
        defs.insert(
            arc.clone(),
            MetricDef {
                name: arc.clone(),
                kind,
                time,
                description,
            },
        );
        Ok(arc)
    }

    pub fn get(&self, name: &str) -> Option<MetricDef> {
        self.defs.read().get(name).cloned()
    }

    pub fn all(&self) -> Vec<MetricDef> {
        self.defs.read().values().cloned().collect()
    }
}

/// Milliseconds since the UNIX epoch.
pub fn now_millis() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

/// One metric sample.
#[derive(Debug, Clone, Serialize)]
pub struct Sample {
    pub metric: Arc<str>,
    pub kind: MetricKind,
    pub value: f64,
    pub tags: Arc<Tags>,
    /// Milliseconds since the UNIX epoch.
    pub timestamp_ms: u64,
}

/// Cloneable fan-in handle that VUs use to emit samples.
#[derive(Debug, Clone)]
pub struct MetricsBus {
    tx: tokio::sync::mpsc::UnboundedSender<Sample>,
}

impl MetricsBus {
    pub fn new() -> (Self, tokio::sync::mpsc::UnboundedReceiver<Sample>) {
        let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
        (MetricsBus { tx }, rx)
    }

    pub fn emit(&self, sample: Sample) {
        // The receiver only closes at the very end of a run; late samples
        // from draining VUs are intentionally dropped.
        let _ = self.tx.send(sample);
    }

    pub fn emit_value(&self, metric: &Arc<str>, kind: MetricKind, value: f64, tags: &Arc<Tags>) {
        self.emit(Sample {
            metric: metric.clone(),
            kind,
            value,
            tags: tags.clone(),
            timestamp_ms: now_millis(),
        });
    }

    pub fn counter(&self, metric: &Arc<str>, value: f64, tags: &Arc<Tags>) {
        self.emit_value(metric, MetricKind::Counter, value, tags);
    }

    pub fn gauge(&self, metric: &Arc<str>, value: f64, tags: &Arc<Tags>) {
        self.emit_value(metric, MetricKind::Gauge, value, tags);
    }

    pub fn rate(&self, metric: &Arc<str>, pass: bool, tags: &Arc<Tags>) {
        self.emit_value(metric, MetricKind::Rate, if pass { 1.0 } else { 0.0 }, tags);
    }

    pub fn trend(&self, metric: &Arc<str>, value: f64, tags: &Arc<Tags>) {
        self.emit_value(metric, MetricKind::Trend, value, tags);
    }
}

/// Interned built-in metric names, resolved once per engine.
#[derive(Debug, Clone)]
pub struct BuiltinMetrics {
    pub http_reqs: Arc<str>,
    pub http_req_duration: Arc<str>,
    pub http_req_blocked: Arc<str>,
    pub http_req_connecting: Arc<str>,
    pub http_req_tls_handshaking: Arc<str>,
    pub http_req_sending: Arc<str>,
    pub http_req_waiting: Arc<str>,
    pub http_req_receiving: Arc<str>,
    pub http_req_failed: Arc<str>,
    pub iterations: Arc<str>,
    pub iteration_duration: Arc<str>,
    pub dropped_iterations: Arc<str>,
    pub vus: Arc<str>,
    pub vus_max: Arc<str>,
    pub checks: Arc<str>,
    pub vu_exceptions: Arc<str>,
    pub data_sent: Arc<str>,
    pub data_received: Arc<str>,
}

impl BuiltinMetrics {
    pub fn resolve(registry: &MetricRegistry) -> Self {
        let name = |n: &str| {
            registry
                .get(n)
                .map(|d| d.name)
                .unwrap_or_else(|| Arc::from(n))
        };
        BuiltinMetrics {
            http_reqs: name("http_reqs"),
            http_req_duration: name("http_req_duration"),
            http_req_blocked: name("http_req_blocked"),
            http_req_connecting: name("http_req_connecting"),
            http_req_tls_handshaking: name("http_req_tls_handshaking"),
            http_req_sending: name("http_req_sending"),
            http_req_waiting: name("http_req_waiting"),
            http_req_receiving: name("http_req_receiving"),
            http_req_failed: name("http_req_failed"),
            iterations: name("iterations"),
            iteration_duration: name("iteration_duration"),
            dropped_iterations: name("dropped_iterations"),
            vus: name("vus"),
            vus_max: name("vus_max"),
            checks: name("checks"),
            vu_exceptions: name("vu_exceptions"),
            data_sent: name("data_sent"),
            data_received: name("data_received"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn registry_has_builtins() {
        let reg = MetricRegistry::with_builtins();
        let def = reg.get("http_req_duration").expect("builtin");
        assert_eq!(def.kind, MetricKind::Trend);
        assert!(def.time);
        assert_eq!(reg.get("checks").map(|d| d.kind), Some(MetricKind::Rate));
    }

    #[test]
    fn register_custom_and_conflict() {
        let reg = MetricRegistry::with_builtins();
        reg.register("my_counter", MetricKind::Counter, false, None)
            .expect("register");
        // Same kind is idempotent.
        reg.register("my_counter", MetricKind::Counter, false, None)
            .expect("idempotent");
        // Different kind is an error.
        assert!(reg
            .register("my_counter", MetricKind::Trend, false, None)
            .is_err());
    }

    #[tokio::test]
    async fn bus_delivers_samples() {
        let (bus, mut rx) = MetricsBus::new();
        let metric: Arc<str> = Arc::from("checks");
        let tags = Arc::new(Tags::new());
        bus.rate(&metric, true, &tags);
        bus.counter(&metric, 2.0, &tags);
        let s1 = rx.recv().await.expect("sample");
        assert_eq!(s1.value, 1.0);
        assert_eq!(s1.kind, MetricKind::Rate);
        let s2 = rx.recv().await.expect("sample");
        assert_eq!(s2.value, 2.0);
    }
}
