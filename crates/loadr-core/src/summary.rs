//! End-of-run summary: structured data plus console/JSON renderers.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::aggregate::{AggValues, Aggregator, Snapshot};
use crate::metrics::MetricKind;
use crate::thresholds::ThresholdStatus;

/// Per-check pass/fail counts.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CheckSummary {
    pub name: String,
    pub passes: u64,
    pub fails: u64,
}

/// One metric in the summary, merged across all tag combinations.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MetricSummary {
    pub metric: String,
    pub kind: MetricKind,
    /// Trend values are milliseconds.
    pub agg: AggValues,
}

/// A single time bucket in the run timeline, derived from one live snapshot.
///
/// One point is emitted per snapshot interval. All values describe the run at
/// `elapsed_secs`; throughput and `error_rate` cover the interval window, while
/// latency percentiles and `active_vus` are point-in-time. This is the data the
/// HTML report charts and is exact enough for visual analysis (latency
/// percentiles come from the live, count-weighted merge — the aggregate table
/// remains the source of exact end-of-run figures).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TimelinePoint {
    /// Seconds since the run started.
    pub elapsed_secs: f64,
    /// Successful + failed requests per second over the interval.
    pub rps: f64,
    /// Completed iterations per second over the interval.
    pub iterations_ps: f64,
    /// Active virtual users at this instant.
    pub active_vus: f64,
    /// Fraction of requests that failed over the interval, in `[0, 1]`.
    pub error_rate: f64,
    /// `http_req_duration` average in milliseconds (null if no samples yet).
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub latency_avg: Option<f64>,
    /// `http_req_duration` p50 in milliseconds.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub latency_p50: Option<f64>,
    /// `http_req_duration` p95 in milliseconds.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub latency_p95: Option<f64>,
    /// `http_req_duration` p99 in milliseconds.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub latency_p99: Option<f64>,
}

impl TimelinePoint {
    /// Derive a timeline point from a live snapshot.
    pub fn from_snapshot(snap: &crate::aggregate::Snapshot) -> TimelinePoint {
        let interval = if snap.interval_secs > 0.0 {
            snap.interval_secs
        } else {
            1.0
        };

        // Throughput over the interval (summed across all tag sets).
        let interval_count = |metric: &str| -> f64 {
            snap.series
                .iter()
                .filter(|s| s.metric == metric)
                .map(|s| s.interval_count)
                .sum::<u64>() as f64
        };

        // Pass/fail merged exactly across tag sets: a "failed" rate metric.
        let error_rate = {
            let (passes, total) = snap
                .series
                .iter()
                .filter(|s| s.metric == "http_req_failed")
                .fold((0.0_f64, 0_u64), |(p, t), s| {
                    (p + s.agg.sum, t + s.agg.count)
                });
            if total > 0 {
                passes / total as f64
            } else {
                0.0
            }
        };

        // Latency: count-weighted merge of the trend statistic across tag sets.
        let latency = |pick: fn(&AggValues) -> Option<f64>| -> Option<f64> {
            let mut acc = 0.0_f64;
            let mut total = 0_u64;
            for s in snap
                .series
                .iter()
                .filter(|s| s.metric == "http_req_duration")
            {
                if s.agg.count == 0 {
                    continue;
                }
                if let Some(v) = pick(&s.agg) {
                    acc += v * s.agg.count as f64;
                    total += s.agg.count;
                }
            }
            (total > 0).then_some(acc / total as f64)
        };

        let active_vus = snap
            .series
            .iter()
            .filter(|s| s.metric == "vus")
            .filter_map(|s| s.agg.last)
            .sum();

        TimelinePoint {
            elapsed_secs: snap.elapsed_secs,
            rps: interval_count("http_reqs") / interval,
            iterations_ps: interval_count("iterations") / interval,
            active_vus,
            error_rate,
            latency_avg: latency(|a| a.avg),
            latency_p50: latency(|a| a.med),
            latency_p95: latency(|a| a.p95),
            latency_p99: latency(|a| a.p99),
        }
    }
}

/// The end-of-run summary.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Summary {
    pub name: Option<String>,
    pub run_id: String,
    pub started_ms: u64,
    pub ended_ms: u64,
    pub duration_secs: f64,
    pub scenarios: Vec<String>,
    pub metrics: Vec<MetricSummary>,
    pub checks: Vec<CheckSummary>,
    pub thresholds: Vec<ThresholdStatus>,
    /// All thresholds passed (true when there are none).
    pub thresholds_passed: bool,
    /// Set when the run was aborted (reason).
    pub aborted: Option<String>,
    /// The final full snapshot (per-tag series detail).
    pub snapshot: Snapshot,
    /// Per-interval time series for charting (throughput, latency percentiles,
    /// active VUs, error rate). One point per snapshot interval. Empty for
    /// summaries produced before timeline capture existed.
    #[serde(default)]
    pub timeline: Vec<TimelinePoint>,
}

impl Summary {
    /// Build the summary from the final aggregator state.
    #[allow(clippy::too_many_arguments)]
    pub fn build(
        name: Option<String>,
        run_id: String,
        started_ms: u64,
        scenarios: Vec<String>,
        agg: &mut Aggregator,
        thresholds: Vec<ThresholdStatus>,
        aborted: Option<String>,
        timeline: Vec<TimelinePoint>,
    ) -> Summary {
        let snapshot = agg.snapshot();
        // Merge each metric across all tag sets.
        let metric_names: Vec<(String, MetricKind)> = {
            let mut seen = BTreeMap::new();
            for s in &snapshot.series {
                seen.entry(s.metric.clone()).or_insert(s.kind);
            }
            seen.into_iter().collect()
        };
        let metrics: Vec<MetricSummary> = metric_names
            .iter()
            .filter_map(|(m, _)| {
                agg.aggregate_selector(m, &[])
                    .map(|(kind, values)| MetricSummary {
                        metric: m.clone(),
                        kind,
                        agg: values,
                    })
            })
            .collect();

        // Check summaries from `checks` series tagged with `check`.
        let mut checks: BTreeMap<String, (u64, u64)> = BTreeMap::new();
        for s in snapshot.series.iter().filter(|s| s.metric == "checks") {
            let name = s
                .tags
                .get("check")
                .cloned()
                .unwrap_or_else(|| "unnamed".to_string());
            let entry = checks.entry(name).or_insert((0, 0));
            let passes = s.agg.sum as u64;
            entry.0 += passes;
            entry.1 += s.agg.count - passes;
        }
        let checks: Vec<CheckSummary> = checks
            .into_iter()
            .map(|(name, (passes, fails))| CheckSummary {
                name,
                passes,
                fails,
            })
            .collect();

        let thresholds_passed = thresholds.iter().all(|t| t.passed);
        let ended_ms = crate::metrics::now_millis();
        Summary {
            name,
            run_id,
            started_ms,
            ended_ms,
            duration_secs: snapshot.elapsed_secs,
            scenarios,
            metrics,
            checks,
            thresholds,
            thresholds_passed,
            aborted,
            snapshot,
            timeline,
        }
    }

    pub fn to_json(&self) -> serde_json::Value {
        serde_json::to_value(self).unwrap_or(serde_json::Value::Null)
    }

    /// k6-style console rendering (plain text; the CLI adds color).
    pub fn render_console(&self) -> String {
        let mut out = String::new();
        let title = self.name.as_deref().unwrap_or("loadr test");
        out.push_str(&format!(
            "\n  {} — {} scenario(s), {:.1}s\n\n",
            title,
            self.scenarios.len(),
            self.duration_secs
        ));
        if let Some(reason) = &self.aborted {
            out.push_str(&format!("  ! run aborted: {reason}\n\n"));
        }

        // Checks first.
        if !self.checks.is_empty() {
            let total_pass: u64 = self.checks.iter().map(|c| c.passes).sum();
            let total_fail: u64 = self.checks.iter().map(|c| c.fails).sum();
            let pct = if total_pass + total_fail > 0 {
                100.0 * total_pass as f64 / (total_pass + total_fail) as f64
            } else {
                100.0
            };
            out.push_str(&format!(
                "  checks{} {:>6.2}% — ✓ {} ✗ {}\n",
                dots("checks"),
                pct,
                total_pass,
                total_fail
            ));
            for c in &self.checks {
                let mark = if c.fails == 0 { "✓" } else { "✗" };
                out.push_str(&format!(
                    "    {mark} {} ({} / {})\n",
                    c.name,
                    c.passes,
                    c.passes + c.fails
                ));
            }
            out.push('\n');
        }

        for m in &self.metrics {
            if m.metric == "checks" {
                continue;
            }
            let line = match m.kind {
                MetricKind::Trend => format!(
                    "avg={} min={} med={} max={} p(90)={} p(95)={} p(99)={}",
                    fmt_ms(m.agg.avg),
                    fmt_ms(m.agg.min),
                    fmt_ms(m.agg.med),
                    fmt_ms(m.agg.max),
                    fmt_ms(m.agg.p90),
                    fmt_ms(m.agg.p95),
                    fmt_ms(m.agg.p99),
                ),
                MetricKind::Counter => format!(
                    "{} ({}/s)",
                    fmt_num(m.agg.sum),
                    fmt_num(m.agg.per_second.unwrap_or(0.0))
                ),
                MetricKind::Rate => format!(
                    "{:.2}% — ✓ {} ✗ {}",
                    m.agg.rate.unwrap_or(0.0) * 100.0,
                    m.agg.sum as u64,
                    m.agg.count - m.agg.sum as u64
                ),
                MetricKind::Gauge => format!(
                    "value={} min={} max={}",
                    fmt_num(m.agg.last.unwrap_or(0.0)),
                    fmt_num(m.agg.min.unwrap_or(0.0)),
                    fmt_num(m.agg.max.unwrap_or(0.0))
                ),
            };
            out.push_str(&format!("  {}{} {}\n", m.metric, dots(&m.metric), line));
        }

        if !self.thresholds.is_empty() {
            out.push_str("\n  thresholds:\n");
            for t in &self.thresholds {
                let mark = if t.passed { "✓" } else { "✗" };
                let observed = t
                    .observed
                    .map(|v| format!("{v:.2}"))
                    .unwrap_or_else(|| "no samples".to_string());
                out.push_str(&format!(
                    "    {mark} {}: {} (observed: {})\n",
                    t.metric, t.expression, observed
                ));
            }
        }
        out.push('\n');
        out
    }
}

fn dots(name: &str) -> String {
    let width = 30usize.saturating_sub(name.len());
    format!("{}:", ".".repeat(width.max(2)))
}

fn fmt_ms(v: Option<f64>) -> String {
    match v {
        None => "-".to_string(),
        Some(ms) if ms >= 1000.0 => format!("{:.2}s", ms / 1000.0),
        Some(ms) if ms >= 1.0 => format!("{ms:.2}ms"),
        Some(ms) => format!("{:.0}µs", ms * 1000.0),
    }
}

fn fmt_num(v: f64) -> String {
    if v.fract() == 0.0 && v.abs() < 1e15 {
        format!("{}", v as i64)
    } else {
        format!("{v:.2}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::metrics::{now_millis, Sample, Tags};
    use std::sync::Arc;

    fn build_summary() -> Summary {
        let mut agg = Aggregator::new();
        let mut tags = Tags::new();
        tags.insert("check".into(), "status is 200".into());
        let tags = Arc::new(tags);
        for i in 0..10 {
            agg.record(&Sample {
                metric: Arc::from("checks"),
                kind: MetricKind::Rate,
                value: if i < 9 { 1.0 } else { 0.0 },
                tags: tags.clone(),
                timestamp_ms: now_millis(),
            });
            agg.record(&Sample {
                metric: Arc::from("http_req_duration"),
                kind: MetricKind::Trend,
                value: 10.0 + i as f64,
                tags: Arc::new(Tags::new()),
                timestamp_ms: now_millis(),
            });
        }
        Summary::build(
            Some("demo".into()),
            "run-1".into(),
            now_millis(),
            vec!["default".into()],
            &mut agg,
            vec![ThresholdStatus {
                metric: "http_req_duration".into(),
                expression: "p(95)<400".into(),
                observed: Some(18.0),
                passed: true,
                abort_on_fail: false,
            }],
            None,
            Vec::new(),
        )
    }

    #[test]
    fn builds_checks_and_metrics() {
        let s = build_summary();
        assert_eq!(s.checks.len(), 1);
        assert_eq!(s.checks[0].passes, 9);
        assert_eq!(s.checks[0].fails, 1);
        assert!(s.thresholds_passed);
        assert!(s
            .metrics
            .iter()
            .any(|m| m.metric == "http_req_duration" && m.agg.count == 10));
    }

    #[test]
    fn console_render_contains_key_lines() {
        let s = build_summary();
        let text = s.render_console();
        assert!(text.contains("checks"));
        assert!(text.contains("✓ 9 ✗ 1"));
        assert!(text.contains("http_req_duration"));
        assert!(text.contains("p(95)<400"));
        assert!(text.contains("✓ http_req_duration"));
    }

    #[test]
    fn json_round_trip() {
        let s = build_summary();
        let json = s.to_json();
        let back: Summary = serde_json::from_value(json).expect("round trip");
        assert_eq!(back.run_id, "run-1");
        assert_eq!(back.checks.len(), 1);
    }

    #[test]
    fn timeline_point_from_snapshot() {
        let mut agg = Aggregator::new();
        for i in 0..20 {
            agg.record(&Sample {
                metric: Arc::from("http_reqs"),
                kind: MetricKind::Counter,
                value: 1.0,
                tags: Arc::new(Tags::new()),
                timestamp_ms: now_millis(),
            });
            agg.record(&Sample {
                metric: Arc::from("http_req_duration"),
                kind: MetricKind::Trend,
                value: 10.0 + i as f64,
                tags: Arc::new(Tags::new()),
                timestamp_ms: now_millis(),
            });
            agg.record(&Sample {
                metric: Arc::from("http_req_failed"),
                kind: MetricKind::Rate,
                value: if i % 4 == 0 { 1.0 } else { 0.0 },
                tags: Arc::new(Tags::new()),
                timestamp_ms: now_millis(),
            });
        }
        agg.record(&Sample {
            metric: Arc::from("vus"),
            kind: MetricKind::Gauge,
            value: 5.0,
            tags: Arc::new(Tags::new()),
            timestamp_ms: now_millis(),
        });
        let snap = agg.snapshot();
        let p = TimelinePoint::from_snapshot(&snap);
        assert!(p.rps > 0.0, "rps should be positive");
        assert_eq!(p.active_vus, 5.0);
        // 5 of 20 requests failed → 25%.
        assert!((p.error_rate - 0.25).abs() < 1e-9, "err={}", p.error_rate);
        assert!(p.latency_p95.unwrap() >= p.latency_p50.unwrap());
        assert!(p.latency_avg.unwrap() > 0.0);
    }

    #[test]
    fn timeline_survives_round_trip() {
        let mut s = build_summary();
        s.timeline = vec![TimelinePoint {
            elapsed_secs: 1.0,
            rps: 10.0,
            iterations_ps: 4.0,
            active_vus: 3.0,
            error_rate: 0.1,
            latency_avg: Some(12.0),
            latency_p50: Some(10.0),
            latency_p95: Some(40.0),
            latency_p99: None,
        }];
        let back: Summary = serde_json::from_value(s.to_json()).expect("round trip");
        assert_eq!(back.timeline.len(), 1);
        assert_eq!(back.timeline[0].rps, 10.0);
        assert_eq!(back.timeline[0].latency_p99, None);
    }
}
