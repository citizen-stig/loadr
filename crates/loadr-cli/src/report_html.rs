//! Self-contained HTML report rendering from a `Summary`.

use loadr_core::{MetricKind, Summary};

/// Render a standalone HTML report (no external assets).
pub fn render(summary: &Summary) -> String {
    let title = summary.name.as_deref().unwrap_or("loadr test");
    let status_pill = if summary.thresholds_passed && summary.aborted.is_none() {
        r#"<span class="pill pass">PASSED</span>"#.to_string()
    } else if summary.aborted.is_some() {
        format!(
            r#"<span class="pill fail">ABORTED</span> <span class="muted">{}</span>"#,
            esc(summary.aborted.as_deref().unwrap_or(""))
        )
    } else {
        r#"<span class="pill fail">THRESHOLDS FAILED</span>"#.to_string()
    };

    let mut threshold_rows = String::new();
    for t in &summary.thresholds {
        let mark = if t.passed {
            r#"<td class="ok">✓</td>"#
        } else {
            r#"<td class="bad">✗</td>"#
        };
        threshold_rows.push_str(&format!(
            "<tr>{mark}<td><code>{}</code></td><td><code>{}</code></td><td>{}</td></tr>",
            esc(&t.metric),
            esc(&t.expression),
            t.observed
                .map(|v| format!("{v:.2}"))
                .unwrap_or_else(|| "no samples".into()),
        ));
    }

    let mut check_rows = String::new();
    for c in &summary.checks {
        let total = c.passes + c.fails;
        let pct = if total > 0 {
            100.0 * c.passes as f64 / total as f64
        } else {
            100.0
        };
        let class = if c.fails == 0 { "ok" } else { "bad" };
        check_rows.push_str(&format!(
            r#"<tr><td class="{class}">{}</td><td>{}</td><td>{}</td><td>{}</td><td><div class="bar"><div style="width:{pct:.1}%"></div></div> {pct:.2}%</td></tr>"#,
            if c.fails == 0 { "✓" } else { "✗" },
            esc(&c.name),
            c.passes,
            c.fails,
        ));
    }

    let mut trend_rows = String::new();
    let mut other_rows = String::new();
    for m in &summary.metrics {
        match m.kind {
            MetricKind::Trend => {
                trend_rows.push_str(&format!(
                    "<tr><td><code>{}</code></td><td>{}</td>{}{}{}{}{}{}{}</tr>",
                    esc(&m.metric),
                    m.agg.count,
                    td_ms(m.agg.avg),
                    td_ms(m.agg.min),
                    td_ms(m.agg.med),
                    td_ms(m.agg.p90),
                    td_ms(m.agg.p95),
                    td_ms(m.agg.p99),
                    td_ms(m.agg.max),
                ));
            }
            MetricKind::Counter => {
                other_rows.push_str(&format!(
                    "<tr><td><code>{}</code></td><td>counter</td><td>{}</td><td>{}/s</td></tr>",
                    esc(&m.metric),
                    fmt_num(m.agg.sum),
                    fmt_num(m.agg.per_second.unwrap_or(0.0)),
                ));
            }
            MetricKind::Rate => {
                other_rows.push_str(&format!(
                    "<tr><td><code>{}</code></td><td>rate</td><td>{:.2}%</td><td>✓ {} ✗ {}</td></tr>",
                    esc(&m.metric),
                    m.agg.rate.unwrap_or(0.0) * 100.0,
                    m.agg.sum as u64,
                    m.agg.count - m.agg.sum as u64,
                ));
            }
            MetricKind::Gauge => {
                other_rows.push_str(&format!(
                    "<tr><td><code>{}</code></td><td>gauge</td><td>{}</td><td>min {} / max {}</td></tr>",
                    esc(&m.metric),
                    fmt_num(m.agg.last.unwrap_or(0.0)),
                    fmt_num(m.agg.min.unwrap_or(0.0)),
                    fmt_num(m.agg.max.unwrap_or(0.0)),
                ));
            }
        }
    }

    format!(
        r##"<!doctype html>
<html lang="en"><head><meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>loadr report — {title}</title>
<style>
:root {{ color-scheme: dark; --bg:#0e1117; --panel:#161b24; --border:#262d3a; --fg:#dbe2ee; --muted:#8794a8; --green:#3fb968; --red:#e5534b; --accent:#7aa2f7; }}
body {{ margin:0; background:var(--bg); color:var(--fg); font:15px/1.5 -apple-system,"Segoe UI",Roboto,sans-serif; }}
.wrap {{ max-width: 1080px; margin: 0 auto; padding: 32px 20px 64px; }}
h1 {{ font-size: 26px; margin: 0 0 4px; }} h2 {{ font-size: 17px; margin: 36px 0 10px; color: var(--accent); }}
.muted {{ color: var(--muted); }}
.pill {{ display:inline-block; padding:2px 12px; border-radius:999px; font-weight:600; font-size:13px; }}
.pill.pass {{ background:rgba(63,185,104,.15); color:var(--green); border:1px solid var(--green); }}
.pill.fail {{ background:rgba(229,83,75,.15); color:var(--red); border:1px solid var(--red); }}
table {{ width:100%; border-collapse:collapse; background:var(--panel); border:1px solid var(--border); border-radius:8px; overflow:hidden; }}
th,td {{ text-align:left; padding:8px 12px; border-bottom:1px solid var(--border); font-variant-numeric: tabular-nums; }}
th {{ color:var(--muted); font-weight:600; font-size:12px; text-transform:uppercase; letter-spacing:.04em; }}
tr:last-child td {{ border-bottom:none; }}
.ok {{ color:var(--green); }} .bad {{ color:var(--red); }}
code {{ color:var(--accent); }}
.bar {{ display:inline-block; width:120px; height:8px; background:var(--border); border-radius:4px; vertical-align:middle; margin-right:8px; }}
.bar div {{ height:8px; background:var(--green); border-radius:4px; }}
.meta {{ display:flex; gap:24px; flex-wrap:wrap; margin:18px 0 0; }}
.meta div {{ background:var(--panel); border:1px solid var(--border); border-radius:8px; padding:10px 16px; }}
.meta b {{ display:block; font-size:20px; }}
footer {{ margin-top:48px; color:var(--muted); font-size:13px; }}
</style></head><body><div class="wrap">
<h1>{title}</h1>
<p>{status_pill}</p>
<div class="meta">
  <div><b>{duration:.1}s</b><span class="muted">duration</span></div>
  <div><b>{scenarios}</b><span class="muted">scenario(s)</span></div>
  <div><b>{run_id}</b><span class="muted">run id</span></div>
</div>
<h2>Thresholds</h2>
<table><thead><tr><th></th><th>Metric</th><th>Expression</th><th>Observed</th></tr></thead>
<tbody>{threshold_rows}</tbody></table>
<h2>Checks</h2>
<table><thead><tr><th></th><th>Check</th><th>Passes</th><th>Fails</th><th>Rate</th></tr></thead>
<tbody>{check_rows}</tbody></table>
<h2>Latency (trends)</h2>
<table><thead><tr><th>Metric</th><th>Count</th><th>Avg</th><th>Min</th><th>Med</th><th>p90</th><th>p95</th><th>p99</th><th>Max</th></tr></thead>
<tbody>{trend_rows}</tbody></table>
<h2>Counters, rates &amp; gauges</h2>
<table><thead><tr><th>Metric</th><th>Kind</th><th>Value</th><th>Detail</th></tr></thead>
<tbody>{other_rows}</tbody></table>
<footer>Generated by loadr {version} — https://loadr.io</footer>
</div></body></html>"##,
        title = esc(title),
        duration = summary.duration_secs,
        scenarios = summary.scenarios.len(),
        run_id = esc(&summary.run_id),
        version = env!("CARGO_PKG_VERSION"),
    )
}

fn td_ms(v: Option<f64>) -> String {
    match v {
        None => "<td>-</td>".to_string(),
        Some(ms) if ms >= 1000.0 => format!("<td>{:.2}s</td>", ms / 1000.0),
        Some(ms) => format!("<td>{ms:.1}ms</td>"),
    }
}

fn fmt_num(v: f64) -> String {
    if v.fract() == 0.0 && v.abs() < 1e15 {
        format!("{}", v as i64)
    } else {
        format!("{v:.2}")
    }
}

fn esc(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn renders_report() {
        let json = serde_json::json!({
            "name": "demo", "run_id": "r-1", "started_ms": 0u64, "ended_ms": 1000u64,
            "duration_secs": 1.0, "scenarios": ["s"],
            "metrics": [
                {"metric": "http_req_duration", "kind": "trend",
                 "agg": {"count": 10u64, "sum": 100.0, "avg": 10.0, "min": 1.0, "max": 20.0,
                          "med": 9.0, "p90": 18.0, "p95": 19.0, "p99": 20.0, "p999": 20.0,
                          "rate": null, "last": null, "per_second": 10.0}}
            ],
            "checks": [{"name": "ok", "passes": 9u64, "fails": 1u64}],
            "thresholds": [{"metric": "http_req_duration", "expression": "p(95)<400",
                             "observed": 19.0, "passed": true, "abort_on_fail": false}],
            "thresholds_passed": true, "aborted": null,
            "snapshot": {"timestamp_ms": 0u64, "elapsed_secs": 1.0, "interval_secs": 1.0, "series": []}
        });
        let summary: Summary = serde_json::from_value(json).expect("summary");
        let html = render(&summary);
        assert!(html.contains("PASSED"));
        assert!(html.contains("http_req_duration"));
        assert!(html.contains("p(95)&lt;400"));
        assert!(html.contains("<title>loadr report — demo</title>"));
    }
}
