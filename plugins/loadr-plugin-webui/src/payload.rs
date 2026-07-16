//! Builders for the trimmed live payloads sent to the SPA: the per-run SSE
//! "snapshot" event and the aggregate overview document.
//!
//! Decision-facing cumulative percentiles come from exact aggregate snapshots:
//! histograms are merged by the engine/controller before this module sees them.

use std::collections::BTreeSet;

use loadr_core::aggregate::AggValues;
use loadr_core::{Snapshot, ThresholdStatus};
use serde_json::{json, Value};

use crate::{now_ms, RunInfo, UiBackend};

const LIVE_STATES: [&str; 3] = ["pending", "running", "stopping"];

/// The trimmed once-per-second payload for live dashboards.
pub(crate) fn live_payload(
    snap: &Snapshot,
    exact: Option<&Snapshot>,
    thresholds: &[ThresholdStatus],
    run: &RunInfo,
) -> Value {
    let interval = if snap.interval_secs > 0.0 {
        snap.interval_secs
    } else {
        1.0
    };
    let latency = |pick: fn(&AggValues) -> Option<f64>| -> Value {
        exact
            .and_then(|aggregates| aggregate_value(aggregates, "request_duration", None, pick))
            .or_else(|| weighted_request_latency(snap, None, pick))
            .map(|v| json!(v))
            .unwrap_or(Value::Null)
    };
    let (check_passes, check_fails) = check_counts(snap);
    let interval_error = interval_rate(snap, "http_req_failed", None);
    let (error_rate, error_window) = match interval_error {
        Some(rate) => (Some(rate), "interval"),
        None => (merged_rate(snap, "http_req_failed", None), "run_to_date"),
    };
    let latency_quality = if exact.is_some() {
        "exact"
    } else {
        "estimated"
    };

    json!({
        "run_id": run.run_id,
        "generated_ms": now_ms(),
        "ts": snap.timestamp_ms,
        "elapsed": snap.elapsed_secs,
        "interval_secs": snap.interval_secs,
        "state": run.state,
        "complete": run.complete,
        "assigned_agents": run.agents,
        "contributing_agents": run.contributing_agents,
        "lost_agents": run.lost_agents,
        "rps": interval_request_rps(snap, None, interval),
        "iterations_ps": interval_rps(snap, "iterations", None, interval),
        "error_rate": error_rate,
        "active_vus": gauge_sum(snap, "vus"),
        "max_vus": gauge_sum(snap, "vus_max"),
        "latency": {
            "avg": latency(|a| a.avg),
            "p50": latency(|a| a.med),
            "p90": latency(|a| a.p90),
            "p95": latency(|a| a.p95),
            "p99": latency(|a| a.p99),
        },
        "per_scenario": per_scenario(snap, exact, interval),
        "per_agent": exact.map(per_agent).unwrap_or_default(),
        "thresholds": thresholds,
        "checks": { "passes": check_passes, "fails": check_fails },
        "data_sent_ps": interval_bytes_per_sec(snap, "data_sent", interval),
        "data_received_ps": interval_bytes_per_sec(snap, "data_received", interval),
        "request_reqs_total": exact
            .and_then(|aggregates| aggregate_value(aggregates, "request_reqs", None, |a| Some(a.sum)))
            .unwrap_or_else(|| request_counter_total(snap)),
        "metric_contract": {
            "rps_window": "interval",
            "error_rate_window": error_window,
            "latency_window": "run_to_date",
            "latency_quality": latency_quality,
        },
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
/// - HTTP status codes: failing `http_req_failed` samples bucketed by their
///   non-zero `status` tag. This respects custom expected-response rules.
/// - Transport/error kinds: `http_req_failed` series carrying an `error_kind`
///   (transport failures) or `error` (prepare/protocol/extraction) tag.
/// - Failed checks: the failing fraction of each `checks` series, by `check` tag.
/// - Script exceptions: the `vu_exceptions` counter, by `exception` tag.
pub(crate) fn failures_breakdown(snap: &Snapshot) -> Value {
    use std::collections::BTreeMap;
    const LIMIT: usize = 12;

    // Status failures from the failure rate itself. Using `http_reqs` and a
    // hard-coded >=400 test would misclassify custom expected statuses.
    let mut by_status: BTreeMap<String, u64> = BTreeMap::new();
    for s in snap.series.iter().filter(|s| s.metric == "http_req_failed") {
        let Some(status) = s.tags.get("status") else {
            continue;
        };
        let code: i64 = status.parse().unwrap_or(0);
        if code > 0 && s.agg.sum > 0.0 {
            *by_status.entry(status.clone()).or_default() += s.agg.sum.max(0.0) as u64;
        }
    }

    // Transport / error-kind failures from http_req_failed series tags.
    let mut by_error_kind: BTreeMap<String, u64> = BTreeMap::new();
    for s in snap.series.iter().filter(|s| s.metric == "http_req_failed") {
        let kind = s
            .tags
            .get("error_kind")
            .or_else(|| s.tags.get("error"))
            .cloned();
        if let Some(kind) = kind {
            // sum = number of failing samples in a Rate series.
            *by_error_kind.entry(kind).or_default() += s.agg.sum.max(0.0) as u64;
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

    let failed_requests = snap
        .series
        .iter()
        .filter(|s| s.metric == "http_req_failed")
        .map(|s| s.agg.sum.max(0.0) as u64)
        .sum::<u64>();
    json!({
        "event_total": status_total + error_total + check_total + exception_total,
        "failed_requests": failed_requests,
        "failed_checks": check_total,
        "exceptions": exception_total,
        "by_status": top_buckets(by_status.into_iter().collect(), LIMIT),
        "by_error_kind": top_buckets(by_error_kind.into_iter().collect(), LIMIT),
        "by_check": top_buckets(by_check.into_iter().collect(), LIMIT),
        "by_exception": top_buckets(by_exception.into_iter().collect(), LIMIT),
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
            let exact = backend.run_aggregate_snapshot(&r.run_id);
            let metrics = backend
                .run_snapshot(&r.run_id)
                .map(|s| live_payload(&s, exact.as_deref(), &thresholds, r))
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

fn interval_request_rps(snap: &Snapshot, scenario: Option<&str>, interval: f64) -> f64 {
    let count: u64 = snap
        .series
        .iter()
        .filter(|s| loadr_core::metrics::is_request_counter_metric(&s.metric))
        .filter(|s| match scenario {
            Some(name) => s.tags.get("scenario").map(String::as_str) == Some(name),
            None => true,
        })
        .map(|s| s.interval_count)
        .sum();
    count as f64 / interval
}

fn interval_rate(snap: &Snapshot, metric: &str, scenario: Option<&str>) -> Option<f64> {
    let (passes, total) = snap
        .series
        .iter()
        .filter(|s| series_matches(s, metric, scenario))
        .fold((0.0, 0_u64), |(passes, total), series| {
            (passes + series.interval_sum, total + series.interval_count)
        });
    (total > 0).then_some(passes / total as f64)
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

fn aggregate_value<F>(
    aggregates: &Snapshot,
    metric: &str,
    scenario: Option<&str>,
    pick: F,
) -> Option<f64>
where
    F: Fn(&AggValues) -> Option<f64>,
{
    aggregates
        .series
        .iter()
        .find(|series| {
            series.metric == metric
                && match scenario {
                    Some(name) => {
                        series.tags.len() == 1
                            && series.tags.get("scenario").map(String::as_str) == Some(name)
                    }
                    None => series.tags.is_empty(),
                }
        })
        .and_then(|series| pick(&series.agg))
}

/// Compatibility fallback for backends that cannot yet provide exact
/// aggregate histograms. Its use is explicitly labelled `estimated` in the
/// payload and GraphQL's duplicate duration family is excluded.
fn weighted_request_latency<F>(snap: &Snapshot, scenario: Option<&str>, pick: F) -> Option<f64>
where
    F: Fn(&AggValues) -> Option<f64>,
{
    let mut weighted = 0.0;
    let mut count = 0_u64;
    for series in snap.series.iter().filter(|series| {
        loadr_core::metrics::is_request_duration_metric(&series.metric)
            && match scenario {
                Some(name) => series.tags.get("scenario").map(String::as_str) == Some(name),
                None => true,
            }
    }) {
        if let Some(value) = pick(&series.agg) {
            weighted += value * series.agg.count as f64;
            count += series.agg.count;
        }
    }
    (count > 0).then_some(weighted / count as f64)
}

/// Sum of gauge `last` values across series of a metric.
fn gauge_sum(snap: &Snapshot, metric: &str) -> f64 {
    snap.series
        .iter()
        .filter(|s| s.metric == metric)
        .filter_map(|s| s.agg.last)
        .sum()
}

fn request_counter_total(snap: &Snapshot) -> f64 {
    snap.series
        .iter()
        .filter(|s| loadr_core::metrics::is_request_counter_metric(&s.metric))
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

fn per_scenario(snap: &Snapshot, exact: Option<&Snapshot>, interval: f64) -> Vec<Value> {
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
                "p95": exact
                    .and_then(|aggregates| aggregate_value(aggregates, "request_duration", Some(name), |a| a.p95))
                    .or_else(|| weighted_request_latency(snap, Some(name), |a| a.p95)),
                "avg": exact
                    .and_then(|aggregates| aggregate_value(aggregates, "request_duration", Some(name), |a| a.avg))
                    .or_else(|| weighted_request_latency(snap, Some(name), |a| a.avg)),
                "error_rate": interval_rate(snap, "http_req_failed", Some(name))
                    .or_else(|| merged_rate(snap, "http_req_failed", Some(name))),
            })
        })
        .collect()
}

fn per_agent(aggregates: &Snapshot) -> Vec<Value> {
    let mut agents: BTreeSet<(String, String)> = BTreeSet::new();
    for series in &aggregates.series {
        if let (Some(name), Some(id)) = (
            series.tags.get("loadr_agent"),
            series.tags.get("loadr_agent_id"),
        ) {
            agents.insert((id.clone(), name.clone()));
        }
    }
    agents
        .into_iter()
        .map(|(id, name)| {
            let find = |metric: &str| {
                aggregates.series.iter().find(|series| {
                    series.metric == metric
                        && series.tags.get("loadr_agent_id") == Some(&id)
                        && series.tags.get("loadr_agent") == Some(&name)
                })
            };
            json!({
                "id": id,
                "name": name,
                "requests": find("request_reqs").map(|s| s.agg.sum).unwrap_or(0.0),
                "latency_avg": find("request_duration").and_then(|s| s.agg.avg),
                "latency_p95": find("request_duration").and_then(|s| s.agg.p95),
                "error_rate": find("http_req_failed").and_then(|s| s.agg.rate),
                "active_vus": find("vus").and_then(|s| s.agg.last).unwrap_or(0.0),
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

    fn run_info(state: &str) -> RunInfo {
        RunInfo {
            run_id: "run-test".to_string(),
            name: Some("test".to_string()),
            state: state.to_string(),
            passed: None,
            started_ms: now_ms(),
            ended_ms: None,
            observed_ms: now_ms(),
            scenarios: vec!["browse".to_string()],
            agents: Vec::new(),
            contributing_agents: Vec::new(),
            lost_agents: Vec::new(),
            complete: None,
            on_agent_loss: None,
        }
    }

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
        let exact = agg.aggregate_snapshot(&[&["scenario"]]);
        let snap = agg.snapshot();
        let payload = live_payload(&snap, Some(&exact), &[], &run_info("running"));
        assert_eq!(payload["state"], "running");
        assert!(payload["rps"].as_f64().expect("rps") > 0.0);
        assert_eq!(payload["active_vus"], 7.0);
        let err = payload["error_rate"].as_f64().expect("error rate");
        assert!((err - 0.1).abs() < 1e-9);
        assert!(payload["latency"]["p95"].as_f64().expect("p95") > 10.0);
        let scenarios = payload["per_scenario"].as_array().expect("scenarios");
        assert_eq!(scenarios.len(), 1);
        assert_eq!(scenarios[0]["scenario"], "browse");
        assert_eq!(payload["metric_contract"]["latency_quality"], "exact");
    }

    #[test]
    fn distributed_percentile_uses_merged_histogram_and_shows_agent_contributions() {
        let mut agg = Aggregator::new();
        for value in 1..=1000 {
            agg.record(&sample(
                "http_req_duration",
                MetricKind::Trend,
                value as f64,
                &[("loadr_agent", "a"), ("loadr_agent_id", "id-a")],
            ));
            agg.record(&sample(
                "http_reqs",
                MetricKind::Counter,
                1.0,
                &[("loadr_agent", "a"), ("loadr_agent_id", "id-a")],
            ));
        }
        for value in 1001..=2000 {
            agg.record(&sample(
                "http_req_duration",
                MetricKind::Trend,
                value as f64,
                &[("loadr_agent", "b"), ("loadr_agent_id", "id-b")],
            ));
            agg.record(&sample(
                "http_reqs",
                MetricKind::Counter,
                1.0,
                &[("loadr_agent", "b"), ("loadr_agent_id", "id-b")],
            ));
        }
        let exact = agg.aggregate_snapshot(&[&["loadr_agent", "loadr_agent_id"]]);
        let snap = agg.snapshot();
        let payload = live_payload(&snap, Some(&exact), &[], &run_info("running"));
        let p99 = payload["latency"]["p99"].as_f64().expect("p99");
        assert!((1970.0..=1990.0).contains(&p99), "true fleet p99: {p99}");
        assert_eq!(payload["per_agent"].as_array().map(Vec::len), Some(2));
        assert_eq!(payload["request_reqs_total"], 2000.0);
    }

    #[test]
    fn graphql_transport_is_counted_once_in_request_rollup() {
        let mut agg = Aggregator::new();
        agg.record(&sample("http_reqs", MetricKind::Counter, 1.0, &[]));
        agg.record(&sample("graphql_reqs", MetricKind::Counter, 1.0, &[]));
        agg.record(&sample("http_req_duration", MetricKind::Trend, 20.0, &[]));
        agg.record(&sample(
            "graphql_req_duration",
            MetricKind::Trend,
            20.0,
            &[],
        ));
        let exact = agg.aggregate_snapshot(&[]);
        let snap = agg.snapshot();
        let payload = live_payload(&snap, Some(&exact), &[], &run_info("running"));
        assert_eq!(payload["request_reqs_total"], 1.0);
        assert_eq!(payload["latency"]["avg"], 20.0);
    }

    /// Build a snapshot exercising every failure source, then assert the
    /// breakdown groups by cause with correct counts and shares.
    #[test]
    fn failures_breakdown_groups_by_cause() {
        let mut agg = Aggregator::new();
        // 10 OK 200s + 3 failing 500s + 2 failing 404s.
        for _ in 0..10 {
            agg.record(&sample(
                "http_reqs",
                MetricKind::Counter,
                1.0,
                &[("status", "200")],
            ));
        }
        for _ in 0..3 {
            agg.record(&sample(
                "http_reqs",
                MetricKind::Counter,
                1.0,
                &[("status", "500")],
            ));
            agg.record(&sample(
                "http_req_failed",
                MetricKind::Rate,
                1.0,
                &[("status", "500")],
            ));
        }
        for _ in 0..2 {
            agg.record(&sample(
                "http_reqs",
                MetricKind::Counter,
                1.0,
                &[("status", "404")],
            ));
            agg.record(&sample(
                "http_req_failed",
                MetricKind::Rate,
                1.0,
                &[("status", "404")],
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
        assert_eq!(f["event_total"], 20);
        assert_eq!(f["failed_requests"], 9);
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

    #[test]
    fn failures_breakdown_empty_when_all_ok() {
        let mut agg = Aggregator::new();
        for _ in 0..5 {
            agg.record(&sample(
                "http_reqs",
                MetricKind::Counter,
                1.0,
                &[("status", "200")],
            ));
        }
        let snap = agg.snapshot();
        let f = failures_breakdown(&snap);
        assert_eq!(f["event_total"], 0);
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
