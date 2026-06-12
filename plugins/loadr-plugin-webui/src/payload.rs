//! Builders for the trimmed live payloads sent to the SPA: the per-run SSE
//! "snapshot" event and the aggregate overview document.
//!
//! Percentiles merged across tagged series are count-weighted approximations
//! (the snapshot carries no histograms); the end-of-run summary is exact.

use std::collections::BTreeSet;

use loadr_core::aggregate::AggValues;
use loadr_core::{Snapshot, ThresholdStatus};
use serde_json::{json, Value};

use crate::UiBackend;

const LIVE_STATES: [&str; 3] = ["pending", "running", "stopping"];

/// The trimmed once-per-second payload for live dashboards.
pub(crate) fn live_payload(snap: &Snapshot, thresholds: &[ThresholdStatus], state: &str) -> Value {
    let interval = if snap.interval_secs > 0.0 {
        snap.interval_secs
    } else {
        1.0
    };
    let latency = |pick: fn(&AggValues) -> Option<f64>| -> Value {
        weighted(snap, "http_req_duration", None, pick)
            .map(|v| json!(v))
            .unwrap_or(Value::Null)
    };
    let (check_passes, check_fails) = check_counts(snap);

    json!({
        "ts": snap.timestamp_ms,
        "elapsed": snap.elapsed_secs,
        "interval_secs": snap.interval_secs,
        "state": state,
        "rps": interval_rps(snap, "http_reqs", None, interval),
        "iterations_ps": interval_rps(snap, "iterations", None, interval),
        "error_rate": merged_rate(snap, "http_req_failed", None),
        "active_vus": gauge_sum(snap, "vus"),
        "max_vus": gauge_sum(snap, "vus_max"),
        "latency": {
            "avg": latency(|a| a.avg),
            "p50": latency(|a| a.med),
            "p90": latency(|a| a.p90),
            "p95": latency(|a| a.p95),
            "p99": latency(|a| a.p99),
        },
        "per_scenario": per_scenario(snap, interval),
        "thresholds": thresholds,
        "checks": { "passes": check_passes, "fails": check_fails },
        "data_sent_ps": interval_bytes_per_sec(snap, "data_sent", interval),
        "data_received_ps": interval_bytes_per_sec(snap, "data_received", interval),
        "http_reqs_total": counter_total(snap, "http_reqs"),
    })
}

/// The aggregate overview: the most relevant run (live preferred, else most
/// recent) plus fleet counters.
pub(crate) fn overview_json(backend: &dyn UiBackend) -> Value {
    let runs = backend.runs();
    let live_runs = runs
        .iter()
        .filter(|r| LIVE_STATES.contains(&r.state.as_str()))
        .count();
    let target = runs
        .iter()
        .find(|r| LIVE_STATES.contains(&r.state.as_str()))
        .or_else(|| runs.first());

    let (run, metrics) = match target {
        Some(r) => {
            let thresholds = backend.run_thresholds(&r.run_id);
            let metrics = backend
                .run_snapshot(&r.run_id)
                .map(|s| live_payload(&s, &thresholds, &r.state))
                .unwrap_or(Value::Null);
            (serde_json::to_value(r).unwrap_or(Value::Null), metrics)
        }
        None => (Value::Null, Value::Null),
    };

    json!({
        "run": run,
        "metrics": metrics,
        "live_runs": live_runs,
        "total_runs": runs.len(),
        "agents": backend.agents().len(),
    })
}

fn series_matches(s: &loadr_core::SeriesSnapshot, metric: &str, scenario: Option<&str>) -> bool {
    if s.metric != metric {
        return false;
    }
    match scenario {
        Some(name) => s.tags.get("scenario").map(String::as_str) == Some(name),
        None => true,
    }
}

/// Events recorded since the previous snapshot, per second.
fn interval_rps(snap: &Snapshot, metric: &str, scenario: Option<&str>, interval: f64) -> f64 {
    let count: u64 = snap
        .series
        .iter()
        .filter(|s| series_matches(s, metric, scenario))
        .map(|s| s.interval_count)
        .sum();
    count as f64 / interval
}

/// Pass fraction merged exactly across tag sets (sum of passes / sum of total).
fn merged_rate(snap: &Snapshot, metric: &str, scenario: Option<&str>) -> Option<f64> {
    let (passes, total) = snap
        .series
        .iter()
        .filter(|s| series_matches(s, metric, scenario))
        .fold((0.0_f64, 0_u64), |(p, t), s| {
            (p + s.agg.sum, t + s.agg.count)
        });
    if total > 0 {
        Some(passes / total as f64)
    } else {
        None
    }
}

/// Count-weighted merge of a trend statistic across tag sets (approximate).
fn weighted<F>(snap: &Snapshot, metric: &str, scenario: Option<&str>, pick: F) -> Option<f64>
where
    F: Fn(&AggValues) -> Option<f64>,
{
    let mut acc = 0.0_f64;
    let mut total = 0_u64;
    for s in snap
        .series
        .iter()
        .filter(|s| series_matches(s, metric, scenario))
    {
        if s.agg.count == 0 {
            continue;
        }
        if let Some(v) = pick(&s.agg) {
            acc += v * s.agg.count as f64;
            total += s.agg.count;
        }
    }
    if total > 0 {
        Some(acc / total as f64)
    } else {
        None
    }
}

/// Sum of gauge `last` values across series of a metric.
fn gauge_sum(snap: &Snapshot, metric: &str) -> f64 {
    snap.series
        .iter()
        .filter(|s| s.metric == metric)
        .filter_map(|s| s.agg.last)
        .sum()
}

fn counter_total(snap: &Snapshot, metric: &str) -> f64 {
    snap.series
        .iter()
        .filter(|s| s.metric == metric)
        .map(|s| s.agg.sum)
        .sum()
}

fn interval_bytes_per_sec(snap: &Snapshot, metric: &str, interval: f64) -> f64 {
    let sum: f64 = snap
        .series
        .iter()
        .filter(|s| s.metric == metric)
        .map(|s| s.interval_sum)
        .sum();
    (sum / interval).max(0.0)
}

fn check_counts(snap: &Snapshot) -> (u64, u64) {
    let mut passes = 0_u64;
    let mut total = 0_u64;
    for s in snap.series.iter().filter(|s| s.metric == "checks") {
        passes += s.agg.sum.max(0.0) as u64;
        total += s.agg.count;
    }
    (passes, total.saturating_sub(passes))
}

fn per_scenario(snap: &Snapshot, interval: f64) -> Vec<Value> {
    let mut names: BTreeSet<&str> = snap
        .series
        .iter()
        .filter_map(|s| s.tags.get("scenario").map(String::as_str))
        .collect();
    names.remove("setup");
    names.remove("teardown");
    names
        .into_iter()
        .map(|name| {
            json!({
                "scenario": name,
                "rps": interval_rps(snap, "http_reqs", Some(name), interval),
                "iterations_ps": interval_rps(snap, "iterations", Some(name), interval),
                "p95": weighted(snap, "http_req_duration", Some(name), |a| a.p95),
                "avg": weighted(snap, "http_req_duration", Some(name), |a| a.avg),
                "error_rate": merged_rate(snap, "http_req_failed", Some(name)),
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use loadr_core::aggregate::Aggregator;
    use loadr_core::metrics::{now_millis, MetricKind, Sample, Tags};
    use std::sync::Arc;

    fn sample(metric: &str, kind: MetricKind, value: f64, tags: &[(&str, &str)]) -> Sample {
        let tags: Tags = tags
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect();
        Sample {
            metric: Arc::from(metric),
            kind,
            value,
            tags: Arc::new(tags),
            timestamp_ms: now_millis(),
        }
    }

    #[test]
    fn live_payload_shape() {
        let mut agg = Aggregator::new();
        for i in 0..50 {
            agg.record(&sample(
                "http_reqs",
                MetricKind::Counter,
                1.0,
                &[("scenario", "browse")],
            ));
            agg.record(&sample(
                "http_req_duration",
                MetricKind::Trend,
                10.0 + i as f64,
                &[("scenario", "browse")],
            ));
            agg.record(&sample(
                "http_req_failed",
                MetricKind::Rate,
                if i % 10 == 0 { 1.0 } else { 0.0 },
                &[("scenario", "browse")],
            ));
        }
        agg.record(&sample("vus", MetricKind::Gauge, 7.0, &[]));
        let snap = agg.snapshot();
        let payload = live_payload(&snap, &[], "running");
        assert_eq!(payload["state"], "running");
        assert!(payload["rps"].as_f64().expect("rps") > 0.0);
        assert_eq!(payload["active_vus"], 7.0);
        let err = payload["error_rate"].as_f64().expect("error rate");
        assert!((err - 0.1).abs() < 1e-9);
        assert!(payload["latency"]["p95"].as_f64().expect("p95") > 10.0);
        let scenarios = payload["per_scenario"].as_array().expect("scenarios");
        assert_eq!(scenarios.len(), 1);
        assert_eq!(scenarios[0]["scenario"], "browse");
    }
}
