//! `loadr explain <summary.json>` — a plain-language root-cause read of a run.
//!
//! A deterministic analyzer over a `loadr run --summary-export` file: it reports
//! the threshold verdict, error rate, latency tail, and a heuristic "likely
//! cause". This is the offline path of the AI copilot — no model, no network —
//! and the same digest an LLM provider will later narrate.

use std::path::PathBuf;

use clap::Args;
use owo_colors::OwoColorize;

use loadr_core::summary::MetricSummary;
use loadr_core::thresholds::ThresholdStatus;
use loadr_core::Summary;

#[derive(Args)]
pub struct ExplainArgs {
    /// A `loadr run --summary-export` JSON file
    pub summary: PathBuf,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Level {
    Ok,
    Warn,
    Bad,
}

pub struct Finding {
    pub level: Level,
    pub text: String,
}

pub fn execute(args: ExplainArgs) -> anyhow::Result<i32> {
    let raw = std::fs::read_to_string(&args.summary)
        .map_err(|e| anyhow::anyhow!("cannot read {}: {e}", args.summary.display()))?;
    let s: Summary =
        serde_json::from_str(&raw).map_err(|e| anyhow::anyhow!("not a loadr summary JSON: {e}"))?;

    println!(
        "{} {}",
        "loadr explain".red().bold(),
        s.name.as_deref().unwrap_or("run").dimmed()
    );
    for f in analyze(&s.metrics, &s.thresholds, s.thresholds_passed) {
        let mark = match f.level {
            Level::Ok => "•".green().to_string(),
            Level::Warn => "!".yellow().bold().to_string(),
            Level::Bad => "✗".red().bold().to_string(),
        };
        println!("{mark} {}", f.text);
    }
    Ok(0)
}

/// Derive plain-language findings from a run's metrics and thresholds.
pub fn analyze(
    metrics: &[MetricSummary],
    thresholds: &[ThresholdStatus],
    thresholds_passed: bool,
) -> Vec<Finding> {
    let mut out = Vec::new();

    // Threshold verdict.
    let failed: Vec<&ThresholdStatus> = thresholds.iter().filter(|t| !t.passed).collect();
    if thresholds_passed {
        out.push(ok("All thresholds passed — the run met its SLOs."));
    } else {
        out.push(bad(format!(
            "{} threshold(s) failed — the run did not meet its SLOs.",
            failed.len()
        )));
        for t in &failed {
            let obs = t
                .observed
                .map(|o| format!(" (observed {o:.1})"))
                .unwrap_or_default();
            out.push(bad(format!("  ✗ {}{obs}", t.expression)));
        }
    }

    let find = |name: &str| metrics.iter().find(|m| m.metric == name);
    let err_rate = find("http_req_failed")
        .and_then(|m| m.agg.rate)
        .unwrap_or(0.0);
    let dur = find("http_req_duration").map(|m| &m.agg);
    let high_tail = dur
        .and_then(|a| a.med.zip(a.p99))
        .map(|(m, p)| m > 0.0 && p / m >= 5.0)
        .unwrap_or(false);

    // Error rate.
    if err_rate >= 0.05 {
        out.push(bad(format!(
            "Error rate is {:.1}% — a large fraction of requests failed; check the status/timeout breakdown.",
            err_rate * 100.0
        )));
    } else if err_rate >= 0.005 {
        out.push(warn(format!(
            "Error rate is {:.2}% — above a typical 0.1% budget; worth investigating.",
            err_rate * 100.0
        )));
    }

    // Latency, with a tail read.
    if let Some(a) = dur {
        if let (Some(med), Some(p99)) = (a.med, a.p99) {
            out.push(ok(format!(
                "Latency: p50 {med:.0}ms · p95 {:.0}ms · p99 {p99:.0}ms.",
                a.p95.unwrap_or(0.0)
            )));
            if high_tail {
                out.push(warn(format!(
                    "Heavy tail: p99 {p99:.0}ms is {:.1}× the median {med:.0}ms — a slow minority, not average slowness (suspect coordinated omission, GC pauses, lock contention, or a cold path).",
                    p99 / med
                )));
            }
        }
    }

    if let Some(rps) = find("http_reqs").and_then(|m| m.agg.per_second) {
        out.push(ok(format!("Throughput: {rps:.0} req/s.")));
    }

    // Heuristic likely-cause.
    if err_rate >= 0.05 && high_tail {
        out.push(bad(
            "Likely cause: past the knee — latency and errors climbed together, the signature of saturation. Reduce load or add capacity, then re-test.",
        ));
    } else if high_tail && err_rate < 0.005 {
        out.push(warn(
            "Likely cause: tail latency without errors points at a slow code path (cache miss, GC, lock contention), not raw capacity. Profile the p99 requests.",
        ));
    } else if thresholds_passed && err_rate < 0.005 && !high_tail {
        out.push(ok("Overall: a healthy run with no red flags."));
    }

    out
}

fn ok(t: impl Into<String>) -> Finding {
    Finding {
        level: Level::Ok,
        text: t.into(),
    }
}
fn warn(t: impl Into<String>) -> Finding {
    Finding {
        level: Level::Warn,
        text: t.into(),
    }
}
fn bad(t: impl Into<String>) -> Finding {
    Finding {
        level: Level::Bad,
        text: t.into(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use loadr_core::aggregate::AggValues;
    use loadr_core::summary::MetricSummary;
    use loadr_core::MetricKind;

    fn metric(name: &str, kind: MetricKind, agg: AggValues) -> MetricSummary {
        MetricSummary {
            metric: name.into(),
            kind,
            agg,
        }
    }

    fn threshold(expr: &str, passed: bool, observed: f64) -> ThresholdStatus {
        ThresholdStatus {
            metric: "http_req_duration".into(),
            expression: expr.into(),
            observed: Some(observed),
            passed,
            abort_on_fail: false,
        }
    }

    #[test]
    fn healthy_run_reads_clean() {
        let dur = AggValues {
            med: Some(40.0),
            p95: Some(80.0),
            p99: Some(120.0),
            ..Default::default()
        };
        let metrics = vec![
            metric("http_req_duration", MetricKind::Trend, dur),
            metric(
                "http_req_failed",
                MetricKind::Rate,
                AggValues {
                    rate: Some(0.0),
                    ..Default::default()
                },
            ),
        ];
        let f = analyze(&metrics, &[], true);
        assert!(f
            .iter()
            .any(|f| f.level == Level::Ok && f.text.contains("healthy")));
        assert!(f.iter().all(|f| f.level != Level::Bad));
    }

    #[test]
    fn saturation_is_flagged_when_latency_and_errors_climb() {
        let dur = AggValues {
            med: Some(50.0),
            p95: Some(800.0),
            p99: Some(3000.0),
            ..Default::default()
        };
        let metrics = vec![
            metric("http_req_duration", MetricKind::Trend, dur),
            metric(
                "http_req_failed",
                MetricKind::Rate,
                AggValues {
                    rate: Some(0.12),
                    ..Default::default()
                },
            ),
        ];
        let f = analyze(&metrics, &[threshold("p99 < 300", false, 3000.0)], false);
        assert!(f.iter().any(|f| f.text.contains("past the knee")));
        assert!(f.iter().any(|f| f.text.contains("threshold(s) failed")));
        assert!(f
            .iter()
            .any(|f| f.text.to_lowercase().contains("error rate is 12")));
    }

    #[test]
    fn slow_path_flagged_when_tail_high_but_no_errors() {
        let dur = AggValues {
            med: Some(20.0),
            p95: Some(300.0),
            p99: Some(1500.0),
            ..Default::default()
        };
        let metrics = vec![
            metric("http_req_duration", MetricKind::Trend, dur),
            metric(
                "http_req_failed",
                MetricKind::Rate,
                AggValues {
                    rate: Some(0.0),
                    ..Default::default()
                },
            ),
        ];
        let f = analyze(&metrics, &[], true);
        assert!(f.iter().any(|f| f.text.contains("slow code path")));
    }
}
