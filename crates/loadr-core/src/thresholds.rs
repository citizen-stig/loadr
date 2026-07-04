//! Compiled thresholds and their continuous evaluation.

use std::time::Duration;

use loadr_config::{Agg, MetricSelector, Op, ThresholdExpr, ThresholdList};

use crate::aggregate::Aggregator;
use crate::metrics::MetricKind;

/// One compiled threshold.
#[derive(Debug)]
pub struct CompiledThreshold {
    pub selector: MetricSelector,
    pub expr: ThresholdExpr,
    pub abort_on_fail: bool,
    pub delay_abort_eval: Option<Duration>,
    /// Original expression text for display.
    pub source: String,
}

/// Result of evaluating one threshold.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ThresholdStatus {
    pub metric: String,
    pub expression: String,
    /// The observed aggregate value, when samples exist.
    pub observed: Option<f64>,
    pub passed: bool,
    pub abort_on_fail: bool,
}

/// Compile the `thresholds:` block.
pub fn compile_thresholds(
    thresholds: &indexmap::IndexMap<String, ThresholdList>,
) -> Result<Vec<CompiledThreshold>, String> {
    let mut out = Vec::new();
    for (selector_str, list) in thresholds {
        let selector = MetricSelector::parse(selector_str).map_err(|e| e.to_string())?;
        for entry in list.entries() {
            let expr = ThresholdExpr::parse(entry.expression())
                .map_err(|e| format!("threshold `{selector_str}`: {e}"))?;
            out.push(CompiledThreshold {
                selector: selector.clone(),
                expr,
                abort_on_fail: entry.abort_on_fail(),
                delay_abort_eval: entry.delay_abort_eval().map(|d| d.as_duration()),
                source: entry.expression().to_string(),
            });
        }
    }
    Ok(out)
}

/// Map an SLO objective onto the aggregation the histogram summary actually
/// carries: `slo(50%)` reads the median, the other supported points read the
/// fixed percentiles (p90/p95/p99/p99.9). Every other aggregation is passed
/// through unchanged.
fn effective_agg(agg: &Agg) -> Agg {
    match agg {
        Agg::Slo(n) if (*n - 50.0).abs() < 1e-9 => Agg::Med,
        Agg::Slo(n) => Agg::Percentile(*n),
        other => *other,
    }
}

/// Evaluate one threshold against the aggregator.
/// A threshold with no samples yet passes (matching k6).
pub fn evaluate(threshold: &CompiledThreshold, agg: &Aggregator) -> ThresholdStatus {
    let metric = &threshold.selector.metric;
    // SLO objectives are evaluated as the percentile-at-N of the trend.
    let lookup = effective_agg(&threshold.expr.agg);
    let observed = agg
        .aggregate_selector(metric, &threshold.selector.tags)
        .and_then(|(kind, values)| {
            match lookup {
                // Arbitrary percentiles fall back to a merged-histogram query.
                Agg::Percentile(p) => values.value_for(&lookup, kind).or_else(|| {
                    agg.aggregate_selector_percentile(metric, &threshold.selector.tags, p)
                }),
                _ => values.value_for(&lookup, kind),
            }
        });
    let passed = match observed {
        Some(v) => threshold.expr.op.eval(v, threshold.expr.bound),
        None => true,
    };
    ThresholdStatus {
        metric: threshold.selector.to_string(),
        expression: threshold.source.clone(),
        observed,
        passed,
        abort_on_fail: threshold.abort_on_fail,
    }
}

/// Evaluate all thresholds; second element is true when an abort-on-fail
/// threshold (past its delay) is failing.
pub fn evaluate_all(
    thresholds: &[CompiledThreshold],
    agg: &Aggregator,
    elapsed: Duration,
) -> (Vec<ThresholdStatus>, bool) {
    let mut abort = false;
    let statuses: Vec<ThresholdStatus> = thresholds
        .iter()
        .map(|t| {
            let status = evaluate(t, agg);
            if !status.passed && t.abort_on_fail {
                let delay_ok = t.delay_abort_eval.map(|d| elapsed >= d).unwrap_or(true);
                if delay_ok {
                    abort = true;
                }
            }
            status
        })
        .collect();
    (statuses, abort)
}

/// Helper for summary display: the metric kind a threshold applies to.
pub fn threshold_kind(agg: &Aggregator, t: &CompiledThreshold) -> Option<MetricKind> {
    agg.aggregate_selector(&t.selector.metric, &t.selector.tags)
        .map(|(k, _)| k)
}

/// Budget-framed observed label for an SLO threshold, e.g.
/// `p99.9=412ms (budget: <=300ms)`. Returns `None` for non-SLO thresholds and
/// when nothing was observed yet, so callers can fall back to the plain
/// `observed:` rendering. Values are printed in milliseconds — the unit SLO
/// latency budgets (and trend bounds) are expressed in; `<`/`>` bounds are
/// framed as the inclusive budget (`<=`/`>=`).
pub fn slo_observed_label(threshold: &CompiledThreshold, observed: Option<f64>) -> Option<String> {
    let Agg::Slo(n) = threshold.expr.agg else {
        return None;
    };
    let v = observed?;
    let budget_op = match threshold.expr.op {
        Op::Lt | Op::Le => "<=".to_string(),
        Op::Gt | Op::Ge => ">=".to_string(),
        other => other.to_string(),
    };
    Some(format!(
        "p{}={}ms (budget: {budget_op}{}ms)",
        fmt_ms(n),
        fmt_ms(v),
        fmt_ms(threshold.expr.bound),
    ))
}

/// Format a millisecond value tersely: whole numbers drop the decimals,
/// everything else keeps at most two.
fn fmt_ms(v: f64) -> String {
    if (v - v.round()).abs() < 1e-9 {
        (v.round() as i64).to_string()
    } else {
        let s = format!("{v:.2}");
        s.trim_end_matches('0').trim_end_matches('.').to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::metrics::{now_millis, Sample, Tags};
    use std::sync::Arc;

    fn agg_with_durations(values: &[f64]) -> Aggregator {
        let mut agg = Aggregator::new();
        for v in values {
            agg.record(&Sample {
                metric: Arc::from("http_req_duration"),
                kind: MetricKind::Trend,
                value: *v,
                tags: Arc::new(Tags::new()),
                timestamp_ms: now_millis(),
            });
        }
        agg
    }

    fn compile(yaml: &str) -> Vec<CompiledThreshold> {
        let map: indexmap::IndexMap<String, ThresholdList> =
            serde_yaml::from_str(yaml).expect("yaml");
        compile_thresholds(&map).expect("compile")
    }

    #[test]
    fn passing_and_failing() {
        let agg = agg_with_durations(&(1..=100).map(|i| i as f64).collect::<Vec<_>>());
        let ts = compile(r#"{ http_req_duration: [ "p(95)<200", "avg<10" ] }"#);
        let (statuses, abort) = evaluate_all(&ts, &agg, Duration::from_secs(10));
        assert!(statuses[0].passed, "{:?}", statuses[0]);
        assert!(!statuses[1].passed, "{:?}", statuses[1]);
        assert!(!abort, "no abort_on_fail set");
    }

    #[test]
    fn abort_on_fail_with_delay() {
        let agg = agg_with_durations(&[100.0, 200.0]);
        let ts = compile(
            r#"{ http_req_duration: [ { threshold: "max<50", abort_on_fail: true, delay_abort_eval: 30s } ] }"#,
        );
        let (_, abort_early) = evaluate_all(&ts, &agg, Duration::from_secs(5));
        assert!(!abort_early, "within delay window");
        let (_, abort_late) = evaluate_all(&ts, &agg, Duration::from_secs(31));
        assert!(abort_late, "past delay window");
    }

    #[test]
    fn no_samples_passes() {
        let agg = Aggregator::new();
        let ts = compile(r#"{ http_req_duration: [ "p(95)<200" ] }"#);
        let (statuses, abort) = evaluate_all(&ts, &agg, Duration::ZERO);
        assert!(statuses[0].passed);
        assert!(statuses[0].observed.is_none());
        assert!(!abort);
    }

    #[test]
    fn arbitrary_percentile() {
        let agg = agg_with_durations(&(1..=1000).map(|i| i as f64).collect::<Vec<_>>());
        let ts = compile(r#"{ http_req_duration: [ "p(42)<500" ] }"#);
        let (statuses, _) = evaluate_all(&ts, &agg, Duration::ZERO);
        let observed = statuses[0].observed.expect("p42");
        assert!((observed - 420.0).abs() / 420.0 < 0.02, "p42={observed}");
        assert!(statuses[0].passed);
    }

    #[test]
    fn slo_thresholds_evaluate_at_mapped_percentile() {
        let agg = agg_with_durations(&(1..=1000).map(|i| i as f64).collect::<Vec<_>>());
        let ts = compile(
            r#"{ http_req_duration: [ "slo(95%)<1000", "slo(95%)<900", "slo(50%)<600", "slo(50%)<400" ] }"#,
        );
        let (statuses, _) = evaluate_all(&ts, &agg, Duration::ZERO);
        // slo(95%) reads p95 (~950 of 1..=1000).
        let p95 = statuses[0].observed.expect("p95");
        assert!((p95 - 950.0).abs() / 950.0 < 0.02, "p95={p95}");
        assert!(statuses[0].passed, "{:?}", statuses[0]);
        assert!(!statuses[1].passed, "{:?}", statuses[1]);
        // slo(50%) reads the median (~500).
        let med = statuses[2].observed.expect("med");
        assert!((med - 500.0).abs() / 500.0 < 0.02, "med={med}");
        assert!(statuses[2].passed, "{:?}", statuses[2]);
        assert!(!statuses[3].passed, "{:?}", statuses[3]);
    }

    #[test]
    fn slo_bound_accepts_duration_units() {
        let agg = agg_with_durations(&(1..=1000).map(|i| i as f64).collect::<Vec<_>>());
        // p99.9 of 1..=1000 is ~999ms; a 1.5s budget passes, 500ms does not.
        let ts =
            compile(r#"{ http_req_duration: [ "slo(99.9%) <= 1.5s", "slo(99.9%) < 500ms" ] }"#);
        let (statuses, _) = evaluate_all(&ts, &agg, Duration::ZERO);
        assert!(statuses[0].passed, "{:?}", statuses[0]);
        assert!(!statuses[1].passed, "{:?}", statuses[1]);
        // The status keeps the original expression text for display.
        assert_eq!(statuses[0].expression, "slo(99.9%) <= 1.5s");
    }

    #[test]
    fn slo_no_samples_passes() {
        let agg = Aggregator::new();
        let ts = compile(r#"{ http_req_duration: [ "slo(99%)<300ms" ] }"#);
        let (statuses, abort) = evaluate_all(&ts, &agg, Duration::ZERO);
        assert!(statuses[0].passed);
        assert!(statuses[0].observed.is_none());
        assert!(!abort);
    }

    #[test]
    fn unsupported_slo_point_fails_compile_with_diagnostic() {
        let map: indexmap::IndexMap<String, ThresholdList> =
            serde_yaml::from_str(r#"{ http_req_duration: [ "slo(99.5%) < 300ms" ] }"#)
                .expect("yaml");
        let err = compile_thresholds(&map).unwrap_err();
        let expected = "slo(99.5%) unsupported: histogram summary carries p50/p90/p95/p99/p99.9";
        assert!(err.contains("threshold `http_req_duration`"), "{err}");
        assert!(err.contains(expected), "{err}");
    }

    #[test]
    fn slo_observed_label_frames_budget() {
        let ts = compile(r#"{ http_req_duration: [ "slo(99.9%) < 300ms" ] }"#);
        assert_eq!(
            slo_observed_label(&ts[0], Some(412.0)).as_deref(),
            Some("p99.9=412ms (budget: <=300ms)")
        );
        assert_eq!(
            slo_observed_label(&ts[0], Some(12.5)).as_deref(),
            Some("p99.9=12.5ms (budget: <=300ms)")
        );
        assert!(slo_observed_label(&ts[0], None).is_none(), "no samples yet");

        // Inclusive framing for exclusive ops; other ops render verbatim.
        let ts = compile(r#"{ http_req_duration: [ "slo(90%)>10" ] }"#);
        assert_eq!(
            slo_observed_label(&ts[0], Some(15.0)).as_deref(),
            Some("p90=15ms (budget: >=10ms)")
        );

        // Non-SLO thresholds keep the plain observed rendering.
        let ts = compile(r#"{ http_req_duration: [ "p(95)<400" ] }"#);
        assert!(slo_observed_label(&ts[0], Some(100.0)).is_none());
    }

    #[test]
    fn rate_metric_threshold() {
        let mut agg = Aggregator::new();
        for i in 0..100 {
            agg.record(&Sample {
                metric: Arc::from("checks"),
                kind: MetricKind::Rate,
                value: if i < 97 { 1.0 } else { 0.0 },
                tags: Arc::new(Tags::new()),
                timestamp_ms: now_millis(),
            });
        }
        let ts = compile(r#"{ checks: [ "rate>0.95" ] }"#);
        let (statuses, _) = evaluate_all(&ts, &agg, Duration::ZERO);
        assert!(statuses[0].passed);
        let ts = compile(r#"{ checks: [ "rate>0.99" ] }"#);
        let (statuses, _) = evaluate_all(&ts, &agg, Duration::ZERO);
        assert!(!statuses[0].passed);
    }

    #[test]
    fn tag_selector_threshold() {
        let mut agg = Aggregator::new();
        let mut tags_a = Tags::new();
        tags_a.insert("scenario".into(), "a".into());
        let mut tags_b = Tags::new();
        tags_b.insert("scenario".into(), "b".into());
        for (tags, value) in [(tags_a, 10.0), (tags_b, 1000.0)] {
            let tags = Arc::new(tags);
            for _ in 0..10 {
                agg.record(&Sample {
                    metric: Arc::from("http_req_duration"),
                    kind: MetricKind::Trend,
                    value,
                    tags: tags.clone(),
                    timestamp_ms: now_millis(),
                });
            }
        }
        let ts = compile(r#"{ "http_req_duration{scenario:a}": [ "avg<100" ] }"#);
        let (statuses, _) = evaluate_all(&ts, &agg, Duration::ZERO);
        assert!(statuses[0].passed, "{statuses:?}");
        let ts = compile(r#"{ http_req_duration: [ "avg<100" ] }"#);
        let (statuses, _) = evaluate_all(&ts, &agg, Duration::ZERO);
        assert!(!statuses[0].passed, "merged includes slow scenario");
    }
}
