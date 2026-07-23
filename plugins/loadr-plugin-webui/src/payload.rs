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
        weighted_request_latency(snap, None, pick)
            .map(|v| json!(v))
            .unwrap_or(Value::Null)
    };
    let (check_passes, check_fails) = check_counts(snap);

    json!({
        "ts": snap.timestamp_ms,
        "elapsed": snap.elapsed_secs,
        "interval_secs": snap.interval_secs,
        "state": state,
        "rps": interval_request_rps(snap, None, interval),
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
        "reqs_total": request_counter_total(snap, None),
        "http_reqs_total": counter_total(snap, "http_reqs"),
        "failures": failures_breakdown(snap),
    })
}

/// A single failure-cause bucket: a label, its count, and share of all failures
/// in that category.
fn bucket(key: String, count: u64, category_total: u64) -> Value {
    let share = if category_total > 0 {
        count as f64 / category_total as f64
    } else {
        0.0
    };
    json!({ "key": key, "count": count, "share": share })
}

/// Sort buckets descending by count, cap to `limit`, folding the rest into an
/// "other" row so the panel never grows unbounded under high-cardinality tags.
fn top_buckets(mut counts: Vec<(String, u64)>, limit: usize) -> Vec<Value> {
    counts.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
    let total: u64 = counts.iter().map(|(_, c)| c).sum();
    let mut out = Vec::new();
    if counts.len() > limit {
        let (head, tail) = counts.split_at(limit.saturating_sub(1));
        for (k, c) in head {
            out.push(bucket(k.clone(), *c, total));
        }
        let other: u64 = tail.iter().map(|(_, c)| c).sum();
        if other > 0 {
            out.push(bucket("other".to_string(), other, total));
        }
    } else {
        for (k, c) in &counts {
            out.push(bucket(k.clone(), *c, total));
        }
    }
    out
}

/// Group failed requests, failed checks, and script exceptions by cause.
///
/// Sources, all from data the engine already tracks:
/// - Failed requests: the `http_req_failed` rate, which every protocol records
///   (sum = number of failing samples in a Rate series). Series carrying an
///   `error_kind` (transport failures) or `error` (prepare/protocol/extraction)
///   tag are bucketed by that kind; everything else failed on status and is
///   bucketed by its `status` tag — HTTP 4xx/5xx codes as-is, gRPC codes by
///   name (`UNAVAILABLE (14)`) via the `proto` tag.
/// - Failed checks: the failing fraction of each `checks` series, by `check` tag.
/// - Script exceptions: the `vu_exceptions` counter, by `exception` tag.
pub(crate) fn failures_breakdown(snap: &Snapshot) -> Value {
    use std::collections::BTreeMap;
    const LIMIT: usize = 12;

    let mut by_status: BTreeMap<String, u64> = BTreeMap::new();
    let mut by_error_kind: BTreeMap<String, u64> = BTreeMap::new();
    for s in snap.series.iter().filter(|s| s.metric == "http_req_failed") {
        let fails = s.agg.sum.max(0.0) as u64;
        if fails == 0 {
            continue;
        }
        if let Some(kind) = s.tags.get("error_kind").or_else(|| s.tags.get("error")) {
            *by_error_kind.entry(kind.clone()).or_default() += fails;
        } else {
            let key = match s.tags.get("status") {
                Some(status) => status_key(status, s.tags.get("proto").map(String::as_str)),
                None => "unknown".to_string(),
            };
            *by_status.entry(key).or_default() += fails;
        }
    }

    // Failed checks: count = total evaluations, sum = passes, so fails = count - sum.
    let mut by_check: BTreeMap<String, u64> = BTreeMap::new();
    for s in snap.series.iter().filter(|s| s.metric == "checks") {
        let Some(name) = s.tags.get("check") else {
            continue;
        };
        let fails = s.agg.count.saturating_sub(s.agg.sum.max(0.0) as u64);
        if fails > 0 {
            *by_check.entry(name.clone()).or_default() += fails;
        }
    }

    // Script exceptions from the vu_exceptions counter's `exception` tag.
    let mut by_exception: BTreeMap<String, u64> = BTreeMap::new();
    for s in snap.series.iter().filter(|s| s.metric == "vu_exceptions") {
        let key = s
            .tags
            .get("exception")
            .cloned()
            .unwrap_or_else(|| "exception".to_string());
        *by_exception.entry(key).or_default() += s.agg.sum.max(0.0) as u64;
    }

    let sum_counts = |m: &BTreeMap<String, u64>| -> u64 { m.values().sum() };
    let status_total = sum_counts(&by_status);
    let error_total = sum_counts(&by_error_kind);
    let check_total = sum_counts(&by_check);
    let exception_total = sum_counts(&by_exception);

    json!({
        "total": status_total + error_total + check_total + exception_total,
        "failed_requests": status_total + error_total,
        "failed_checks": check_total,
        "exceptions": exception_total,
        "by_status": top_buckets(by_status.into_iter().collect(), LIMIT),
        "by_error_kind": top_buckets(by_error_kind.into_iter().collect(), LIMIT),
        "by_check": top_buckets(by_check.into_iter().collect(), LIMIT),
        "by_exception": top_buckets(by_exception.into_iter().collect(), LIMIT),
    })
}

/// Label a failing `status` tag, naming gRPC codes so non-HTTP failures stay
/// readable next to plain HTTP statuses.
fn status_key(status: &str, proto: Option<&str>) -> String {
    if proto == Some("grpc") {
        if let Some(name) = status.parse().ok().and_then(grpc_code_name) {
            return format!("{name} ({status})");
        }
    }
    status.to_string()
}

/// tonic isn't a dependency of this crate; the 16 canonical gRPC codes are
/// stable, so a local table is fine.
fn grpc_code_name(code: i64) -> Option<&'static str> {
    Some(match code {
        1 => "CANCELLED",
        2 => "UNKNOWN",
        3 => "INVALID_ARGUMENT",
        4 => "DEADLINE_EXCEEDED",
        5 => "NOT_FOUND",
        6 => "ALREADY_EXISTS",
        7 => "PERMISSION_DENIED",
        8 => "RESOURCE_EXHAUSTED",
        9 => "FAILED_PRECONDITION",
        10 => "ABORTED",
        11 => "OUT_OF_RANGE",
        12 => "UNIMPLEMENTED",
        13 => "INTERNAL",
        14 => "UNAVAILABLE",
        15 => "DATA_LOSS",
        16 => "UNAUTHENTICATED",
        _ => return None,
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
    scenario_matches(s, scenario)
}

fn scenario_matches(s: &loadr_core::SeriesSnapshot, scenario: Option<&str>) -> bool {
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

fn is_request_counter(metric: &str) -> bool {
    metric.ends_with("_reqs")
}

fn is_request_duration(metric: &str) -> bool {
    metric.ends_with("_req_duration")
}

// Request metrics are protocol-family specific (`http_reqs`, `grpc_reqs`,
// `<plugin>_reqs`). The live dashboard headline is protocol-agnostic, so it
// rolls those families up instead of treating HTTP as the only request source.

/// Events recorded since the previous snapshot across all request families,
/// per second.
fn interval_request_rps(snap: &Snapshot, scenario: Option<&str>, interval: f64) -> f64 {
    let count: u64 = snap
        .series
        .iter()
        .filter(|s| is_request_counter(&s.metric) && scenario_matches(s, scenario))
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

fn weighted_request_latency<F>(snap: &Snapshot, scenario: Option<&str>, pick: F) -> Option<f64>
where
    F: Fn(&AggValues) -> Option<f64>,
{
    weighted_matching(
        snap,
        |s| is_request_duration(&s.metric) && scenario_matches(s, scenario),
        pick,
    )
}

fn weighted_matching<F, M>(snap: &Snapshot, matches: M, pick: F) -> Option<f64>
where
    F: Fn(&AggValues) -> Option<f64>,
    M: Fn(&loadr_core::SeriesSnapshot) -> bool,
{
    let mut acc = 0.0_f64;
    let mut total = 0_u64;
    for s in snap.series.iter().filter(|s| matches(s)) {
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

fn request_counter_total(snap: &Snapshot, scenario: Option<&str>) -> f64 {
    snap.series
        .iter()
        .filter(|s| is_request_counter(&s.metric) && scenario_matches(s, scenario))
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
                "rps": interval_request_rps(snap, Some(name), interval),
                "iterations_ps": interval_rps(snap, "iterations", Some(name), interval),
                "p95": weighted_request_latency(snap, Some(name), |a| a.p95),
                "avg": weighted_request_latency(snap, Some(name), |a| a.avg),
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

    #[test]
    fn live_payload_counts_grpc_request_family() {
        let mut agg = Aggregator::new();
        for i in 0..20 {
            agg.record(&sample(
                "grpc_reqs",
                MetricKind::Counter,
                1.0,
                &[("scenario", "rpc")],
            ));
            agg.record(&sample(
                "grpc_req_duration",
                MetricKind::Trend,
                5.0 + i as f64,
                &[("scenario", "rpc")],
            ));
        }
        let snap = agg.snapshot();
        let payload = live_payload(&snap, &[], "running");

        assert!(payload["rps"].as_f64().expect("rps") > 0.0);
        assert_eq!(payload["reqs_total"], 20.0);
        assert_eq!(payload["http_reqs_total"], 0.0);
        assert!(payload["latency"]["p95"].as_f64().expect("p95") > 0.0);
        let scenarios = payload["per_scenario"].as_array().expect("scenarios");
        assert_eq!(scenarios[0]["scenario"], "rpc");
        assert!(scenarios[0]["rps"].as_f64().expect("scenario rps") > 0.0);
        assert!(scenarios[0]["p95"].as_f64().expect("scenario p95") > 0.0);
    }

    #[test]
    fn live_payload_counts_plugin_request_family() {
        let mut agg = Aggregator::new();
        for i in 0..10 {
            agg.record(&sample(
                "mongo_reqs",
                MetricKind::Counter,
                1.0,
                &[("scenario", "transfers")],
            ));
            agg.record(&sample(
                "mongo_req_duration",
                MetricKind::Trend,
                1.0 + i as f64,
                &[("scenario", "transfers")],
            ));
        }
        let snap = agg.snapshot();
        let payload = live_payload(&snap, &[], "running");

        assert!(payload["rps"].as_f64().expect("rps") > 0.0);
        assert_eq!(payload["reqs_total"], 10.0);
        assert_eq!(payload["http_reqs_total"], 0.0);
        assert!(payload["latency"]["p95"].as_f64().expect("p95") > 0.0);
        let scenarios = payload["per_scenario"].as_array().expect("scenarios");
        assert_eq!(scenarios[0]["scenario"], "transfers");
        assert!(scenarios[0]["rps"].as_f64().expect("scenario rps") > 0.0);
        assert!(scenarios[0]["p95"].as_f64().expect("scenario p95") > 0.0);
    }

    #[test]
    fn live_payload_sums_mixed_request_families() {
        let mut agg = Aggregator::new();
        for _ in 0..7 {
            agg.record(&sample(
                "http_reqs",
                MetricKind::Counter,
                1.0,
                &[("scenario", "mixed")],
            ));
            agg.record(&sample(
                "http_req_duration",
                MetricKind::Trend,
                10.0,
                &[("scenario", "mixed")],
            ));
        }
        for _ in 0..3 {
            agg.record(&sample(
                "grpc_reqs",
                MetricKind::Counter,
                1.0,
                &[("scenario", "mixed")],
            ));
            agg.record(&sample(
                "grpc_req_duration",
                MetricKind::Trend,
                30.0,
                &[("scenario", "mixed")],
            ));
        }
        let snap = agg.snapshot();
        let payload = live_payload(&snap, &[], "running");

        assert_eq!(payload["reqs_total"], 10.0);
        assert_eq!(payload["http_reqs_total"], 7.0);
        assert!(payload["rps"].as_f64().expect("rps") > 0.0);
        assert_eq!(payload["latency"]["avg"], 16.0);
        let scenarios = payload["per_scenario"].as_array().expect("scenarios");
        assert_eq!(scenarios[0]["avg"], 16.0);
    }

    /// Build a snapshot exercising every failure source, then assert the
    /// breakdown groups by cause with correct counts and shares.
    #[test]
    fn failures_breakdown_groups_by_cause() {
        let mut agg = Aggregator::new();
        // 10 OK 200s + 3 500s + 2 404s (http_req_failed rate carries status,
        // recorded per request as the engine does).
        for _ in 0..10 {
            agg.record(&sample(
                "http_req_failed",
                MetricKind::Rate,
                0.0,
                &[("status", "200"), ("proto", "http")],
            ));
        }
        for _ in 0..3 {
            agg.record(&sample(
                "http_req_failed",
                MetricKind::Rate,
                1.0,
                &[("status", "500"), ("proto", "http")],
            ));
        }
        for _ in 0..2 {
            agg.record(&sample(
                "http_req_failed",
                MetricKind::Rate,
                1.0,
                &[("status", "404"), ("proto", "http")],
            ));
        }
        // 4 transport timeouts via http_req_failed with an error_kind tag.
        for _ in 0..4 {
            agg.record(&sample(
                "http_req_failed",
                MetricKind::Rate,
                1.0,
                &[("error_kind", "timeout")],
            ));
        }
        // A check "status is 200": 7 pass, 5 fail.
        for i in 0..12 {
            agg.record(&sample(
                "checks",
                MetricKind::Rate,
                if i < 7 { 1.0 } else { 0.0 },
                &[("check", "status is 200")],
            ));
        }
        // 6 script exceptions of the same normalised message.
        for _ in 0..6 {
            agg.record(&sample(
                "vu_exceptions",
                MetricKind::Counter,
                1.0,
                &[("exception", "TypeError: x is undefined")],
            ));
        }
        let snap = agg.snapshot();
        let f = failures_breakdown(&snap);

        // 5 status (3+2) + 4 error_kind + 5 check + 6 exception = 20.
        assert_eq!(f["total"], 20);
        assert_eq!(f["failed_requests"], 9); // 5 status + 4 error_kind
        assert_eq!(f["failed_checks"], 5);
        assert_eq!(f["exceptions"], 6);

        let by_status = f["by_status"].as_array().expect("by_status");
        assert_eq!(by_status.len(), 2);
        // Highest count first: 500 with 3.
        assert_eq!(by_status[0]["key"], "500");
        assert_eq!(by_status[0]["count"], 3);
        let share = by_status[0]["share"].as_f64().expect("share");
        assert!((share - 3.0 / 5.0).abs() < 1e-9);

        let by_kind = f["by_error_kind"].as_array().expect("by_error_kind");
        assert_eq!(by_kind.len(), 1);
        assert_eq!(by_kind[0]["key"], "timeout");
        assert_eq!(by_kind[0]["count"], 4);

        let by_check = f["by_check"].as_array().expect("by_check");
        assert_eq!(by_check.len(), 1);
        assert_eq!(by_check[0]["key"], "status is 200");
        assert_eq!(by_check[0]["count"], 5);

        let by_exc = f["by_exception"].as_array().expect("by_exception");
        assert_eq!(by_exc.len(), 1);
        assert_eq!(by_exc[0]["count"], 6);
    }

    /// gRPC failures carry small numeric codes (1..16) and no error tag; they
    /// must land in by_status with a readable name, not vanish (issue: the
    /// CSV/Report buttons stayed disabled on gRPC runs with failures).
    #[test]
    fn failures_breakdown_grpc_status_failures() {
        let mut agg = Aggregator::new();
        for _ in 0..5 {
            agg.record(&sample(
                "http_req_failed",
                MetricKind::Rate,
                0.0,
                &[("status", "0"), ("proto", "grpc")],
            ));
        }
        for _ in 0..3 {
            agg.record(&sample(
                "http_req_failed",
                MetricKind::Rate,
                1.0,
                &[("status", "14"), ("proto", "grpc")],
            ));
        }
        for _ in 0..2 {
            agg.record(&sample(
                "http_req_failed",
                MetricKind::Rate,
                1.0,
                &[("status", "7"), ("proto", "grpc")],
            ));
        }
        let snap = agg.snapshot();
        let f = failures_breakdown(&snap);
        assert_eq!(f["total"], 5);
        assert_eq!(f["failed_requests"], 5);
        let by_status = f["by_status"].as_array().expect("by_status");
        assert_eq!(by_status.len(), 2);
        assert_eq!(by_status[0]["key"], "UNAVAILABLE (14)");
        assert_eq!(by_status[0]["count"], 3);
        assert_eq!(by_status[1]["key"], "PERMISSION_DENIED (7)");
        assert_eq!(by_status[1]["count"], 2);
    }

    #[test]
    fn failures_breakdown_empty_when_all_ok() {
        let mut agg = Aggregator::new();
        for _ in 0..5 {
            agg.record(&sample(
                "http_req_failed",
                MetricKind::Rate,
                0.0,
                &[("status", "200"), ("proto", "http")],
            ));
        }
        let snap = agg.snapshot();
        let f = failures_breakdown(&snap);
        assert_eq!(f["total"], 0);
        assert!(f["by_status"].as_array().expect("arr").is_empty());
    }

    #[test]
    fn failures_breakdown_caps_high_cardinality() {
        let mut agg = Aggregator::new();
        // 20 distinct failing statuses -> capped to 12 with an "other" row.
        for code in 400..420 {
            agg.record(&sample(
                "http_req_failed",
                MetricKind::Rate,
                1.0,
                &[("status", &code.to_string())],
            ));
        }
        let snap = agg.snapshot();
        let f = failures_breakdown(&snap);
        let by_status = f["by_status"].as_array().expect("by_status");
        assert_eq!(by_status.len(), 12);
        assert_eq!(by_status.last().unwrap()["key"], "other");
        // All 20 failures still accounted for across the 12 rows.
        let summed: u64 = by_status.iter().map(|b| b["count"].as_u64().unwrap()).sum();
        assert_eq!(summed, 20);
    }
}
