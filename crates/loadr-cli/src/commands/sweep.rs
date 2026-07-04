//! `loadr sweep` — run one plan across a parameter matrix and tabulate the
//! results side by side.
//!
//! Each `--var name=v1,v2,…` axis multiplies into a cartesian matrix; every
//! combination is executed sequentially by re-invoking the current `loadr`
//! binary (`run --summary-export …`) with the variable exported as
//! `LOADR_SWEEP_<NAME>` and, for `vus`/`duration`, applied as the matching CLI
//! override. The collected summaries render as a combo × latency/error/RPS
//! matrix on the terminal and optionally as GitHub-flavoured markdown.

use std::path::{Path, PathBuf};

use clap::Args;
use loadr_core::summary::MetricSummary;
use loadr_core::{MetricKind, Summary};
use owo_colors::OwoColorize;

use crate::commands::compare::{
    fmt_latency, fmt_pct, fmt_per_second, render_markdown_table, render_text_table,
};

/// Columns of the result matrix (rows are combos).
const MATRIX_HEADERS: [&str; 6] = ["combo", "p50", "p95", "p99", "error rate", "rps"];

#[derive(Args)]
pub struct SweepArgs {
    /// Test definition file
    pub plan: PathBuf,
    /// Sweep axis, `name=v1,v2,...` (e.g. `vus=10,50,100`). Repeatable;
    /// multiple axes form a cartesian product.
    #[arg(long = "var", value_name = "NAME=V1,V2,..", value_parser = parse_var)]
    pub vars: Vec<SweepVar>,
    /// Directory for the per-combo summary exports (default: `loadr-sweep/`)
    #[arg(long, value_name = "DIR")]
    pub out_dir: Option<PathBuf>,
    /// Write the result matrix as a GitHub-flavoured markdown table
    #[arg(long, value_name = "PATH")]
    pub markdown: Option<PathBuf>,
    /// Duration override for combos that don't sweep `duration`, e.g. `30s`
    #[arg(long)]
    pub duration: Option<String>,
}

/// One `--var` axis: a variable name and the values to sweep over.
#[derive(Debug, Clone, PartialEq)]
pub struct SweepVar {
    pub name: String,
    pub values: Vec<String>,
}

/// A combo's parsed summary and the exit code of the process that ran it.
pub(crate) type RunOutcome = (Summary, i32);

/// Seam for executing one sweep combination — the real implementation spawns
/// the current `loadr` binary; tests substitute a fake returning canned
/// summaries (repo convention: no subprocesses in unit tests).
pub(crate) trait ComboRunner {
    /// Run the plan with `combo` applied, exporting the summary to `export`.
    fn run(&mut self, combo: &[(String, String)], export: &Path) -> anyhow::Result<RunOutcome>;
}

/// One combination's outcome.
#[derive(Debug)]
pub(crate) struct SweepResult {
    pub label: String,
    pub export: PathBuf,
    pub exit_code: i32,
    /// `None` when the combo failed before producing a summary.
    pub summary: Option<Summary>,
}

pub fn execute(args: SweepArgs) -> anyhow::Result<i32> {
    if args.vars.is_empty() {
        anyhow::bail!("at least one --var is required (e.g. --var vus=10,50,100)");
    }
    let combos = expand_matrix(&args.vars);
    let out_dir = args.out_dir.unwrap_or_else(|| PathBuf::from("loadr-sweep"));
    std::fs::create_dir_all(&out_dir)?;
    let axes: Vec<&str> = args.vars.iter().map(|v| v.name.as_str()).collect();
    eprintln!(
        "{} sweeping {} combination(s) of {}",
        "→".cyan(),
        combos.len(),
        axes.join(" × ")
    );

    let mut runner = SubprocessRunner {
        plan: args.plan,
        duration: args.duration,
    };
    let results = run_sweep(&mut runner, &combos, &out_dir);

    println!();
    for line in render_text_table(&MATRIX_HEADERS, &matrix_rows(&results)) {
        println!("{line}");
    }
    println!();

    if let Some(path) = &args.markdown {
        std::fs::write(path, render_markdown(&results))?;
        eprintln!(
            "{} markdown matrix written to {}",
            "✓".green(),
            path.display()
        );
    }

    let failed: Vec<&SweepResult> = results.iter().filter(|r| r.exit_code != 0).collect();
    if !failed.is_empty() {
        for r in &failed {
            eprintln!(
                "{} {} failed (exit {}) — {}",
                "✗".red(),
                r.label,
                r.exit_code,
                r.export.display()
            );
        }
        eprintln!(
            "{} {} of {} combo(s) failed",
            "✗".red(),
            failed.len(),
            results.len()
        );
        return Ok(loadr_core::EXIT_THRESHOLD_FAILED);
    }
    Ok(0)
}

/// Parse one `--var` spec (`vus=10,50,100`).
fn parse_var(spec: &str) -> Result<SweepVar, String> {
    let (name, values) = spec
        .split_once('=')
        .ok_or_else(|| format!("expected `name=v1,v2,..` (e.g. `vus=10,50`), got `{spec}`"))?;
    let name = name.trim();
    if name.is_empty() || !name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') {
        return Err(format!("bad variable name `{name}` in `{spec}`"));
    }
    let values: Vec<String> = values.split(',').map(|v| v.trim().to_string()).collect();
    if values.iter().any(|v| v.is_empty()) {
        return Err(format!("empty value in `{spec}`"));
    }
    Ok(SweepVar {
        name: name.to_string(),
        values,
    })
}

/// Cartesian product of all axes, preserving `--var` order. No axes yields a
/// single empty combination (callers reject that upfront).
pub(crate) fn expand_matrix(vars: &[SweepVar]) -> Vec<Vec<(String, String)>> {
    let mut combos: Vec<Vec<(String, String)>> = vec![Vec::new()];
    for var in vars {
        let mut next = Vec::with_capacity(combos.len() * var.values.len());
        for combo in &combos {
            for value in &var.values {
                let mut widened = combo.clone();
                widened.push((var.name.clone(), value.clone()));
                next.push(widened);
            }
        }
        combos = next;
    }
    combos
}

/// Human label for a combo (`vus=10 duration=30s`).
pub(crate) fn combo_label(combo: &[(String, String)]) -> String {
    let parts: Vec<String> = combo.iter().map(|(n, v)| format!("{n}={v}")).collect();
    parts.join(" ")
}

/// Filename-safe slug for a combo (`vus-10_duration-30s`).
pub(crate) fn combo_slug(combo: &[(String, String)]) -> String {
    let sanitize = |s: &str| s.chars().map(slug_char).collect::<String>();
    let parts: Vec<String> = combo
        .iter()
        .map(|(n, v)| format!("{}-{}", sanitize(n), sanitize(v)))
        .collect();
    parts.join("_")
}

fn slug_char(c: char) -> char {
    if c.is_ascii_alphanumeric() || c == '.' {
        c
    } else {
        '-'
    }
}

/// Environment variable a sweep value is exported under.
fn sweep_env_name(var: &str) -> String {
    format!("LOADR_SWEEP_{}", var.to_ascii_uppercase())
}

/// Run every combination sequentially, streaming a one-line status per combo.
/// A failing combo never aborts the sweep — it lands in the matrix as `-`.
pub(crate) fn run_sweep(
    runner: &mut dyn ComboRunner,
    combos: &[Vec<(String, String)>],
    out_dir: &Path,
) -> Vec<SweepResult> {
    let total = combos.len();
    let mut results = Vec::with_capacity(total);
    for (i, combo) in combos.iter().enumerate() {
        let label = combo_label(combo);
        let export = out_dir.join(format!("sweep-{}.json", combo_slug(combo)));
        eprintln!("{} [{}/{total}] {label}", "→".cyan(), i + 1);
        let (summary, exit_code) = match runner.run(combo, &export) {
            Ok((summary, code)) => {
                if code == 0 {
                    eprintln!("  {} exit {code} — {}", "✓".green(), export.display());
                } else {
                    eprintln!("  {} exit {code} — {}", "✗".red(), export.display());
                }
                (Some(summary), code)
            }
            Err(e) => {
                eprintln!("  {} {label}: {e:#}", "✗".red());
                (None, -1)
            }
        };
        results.push(SweepResult {
            label,
            export,
            exit_code,
            summary,
        });
    }
    results
}

/// Spawns the current `loadr` binary for one combo.
struct SubprocessRunner {
    plan: PathBuf,
    duration: Option<String>,
}

impl ComboRunner for SubprocessRunner {
    fn run(&mut self, combo: &[(String, String)], export: &Path) -> anyhow::Result<RunOutcome> {
        let exe = std::env::current_exe()?;
        let mut cmd = std::process::Command::new(exe);
        cmd.arg("run")
            .arg(&self.plan)
            .arg("--quiet")
            .arg("--summary-export")
            .arg(export)
            .stdout(std::process::Stdio::null());
        let mut sweeps_duration = false;
        for (name, value) in combo {
            cmd.env(sweep_env_name(name), value);
            // vus/duration map onto `loadr run`'s load overrides.
            match name.as_str() {
                "vus" => {
                    cmd.arg("--vus").arg(value);
                }
                "duration" => {
                    cmd.arg("--duration").arg(value);
                    sweeps_duration = true;
                }
                _ => {}
            }
        }
        if let Some(duration) = &self.duration {
            if !sweeps_duration {
                cmd.arg("--duration").arg(duration);
            }
        }
        let status = cmd.status()?;
        let code = status.code().unwrap_or(-1);
        let raw = std::fs::read_to_string(export).map_err(|e| {
            anyhow::anyhow!("combo produced no summary at {}: {e}", export.display())
        })?;
        let summary = serde_json::from_str(&raw).map_err(|e| {
            anyhow::anyhow!("{} is not a loadr summary export: {e}", export.display())
        })?;
        Ok((summary, code))
    }
}

/// One matrix cell, `-` when the value is missing.
fn cell(v: Option<f64>, fmt: impl Fn(f64) -> String) -> String {
    v.map(fmt).unwrap_or_else(|| "-".to_string())
}

/// Build the matrix rows (combo, p50, p95, p99, error rate, rps). Falls back
/// from the `http_` metrics to any protocol family following the
/// `<family>_req*` convention, so plugin-protocol sweeps tabulate too.
pub(crate) fn matrix_rows(results: &[SweepResult]) -> Vec<Vec<String>> {
    results
        .iter()
        .map(|r| {
            let Some(s) = &r.summary else {
                let mut row = vec![r.label.clone()];
                row.resize(MATRIX_HEADERS.len(), "-".to_string());
                return row;
            };
            let trend = find_metric(s, MetricKind::Trend, "http_req_duration", "_req_duration");
            let error = find_metric(s, MetricKind::Rate, "http_req_failed", "_req_failed")
                .and_then(|m| m.agg.rate)
                .map(|rate| rate * 100.0);
            let rps = find_metric(s, MetricKind::Counter, "http_reqs", "_reqs")
                .and_then(|m| m.agg.per_second);
            vec![
                r.label.clone(),
                cell(trend.and_then(|t| t.agg.med), fmt_latency),
                cell(trend.and_then(|t| t.agg.p95), fmt_latency),
                cell(trend.and_then(|t| t.agg.p99), fmt_latency),
                cell(error, fmt_pct),
                cell(rps, fmt_per_second),
            ]
        })
        .collect()
}

/// Exact metric name first, then the `<family>` suffix convention.
fn find_metric<'a>(
    summary: &'a Summary,
    kind: MetricKind,
    exact: &str,
    suffix: &str,
) -> Option<&'a MetricSummary> {
    let metrics = &summary.metrics;
    if let Some(m) = metrics.iter().find(|m| m.metric == exact) {
        return Some(m);
    }
    metrics
        .iter()
        .find(|m| m.kind == kind && m.metric.ends_with(suffix))
}

/// GitHub-flavoured markdown matrix suitable for a PR comment.
pub(crate) fn render_markdown(results: &[SweepResult]) -> String {
    let mut out = String::from("## loadr sweep\n\n");
    out.push_str(&render_markdown_table(
        &MATRIX_HEADERS,
        &matrix_rows(results),
    ));
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use loadr_core::{AggValues, Snapshot};

    fn var(name: &str, values: &[&str]) -> SweepVar {
        SweepVar {
            name: name.to_string(),
            values: values.iter().map(|v| v.to_string()).collect(),
        }
    }

    fn combo(pairs: &[(&str, &str)]) -> Vec<(String, String)> {
        pairs
            .iter()
            .map(|(n, v)| (n.to_string(), v.to_string()))
            .collect()
    }

    fn summary_with(p50: f64, p95: f64, p99: f64, error_rate: f64, rps: f64) -> Summary {
        let trend = MetricSummary {
            metric: "http_req_duration".to_string(),
            kind: MetricKind::Trend,
            agg: AggValues {
                count: 100,
                sum: p50 * 100.0,
                avg: Some(p50),
                med: Some(p50),
                p95: Some(p95),
                p99: Some(p99),
                ..Default::default()
            },
        };
        let failed = MetricSummary {
            metric: "http_req_failed".to_string(),
            kind: MetricKind::Rate,
            agg: AggValues {
                count: 100,
                sum: error_rate * 100.0,
                rate: Some(error_rate),
                ..Default::default()
            },
        };
        let reqs = MetricSummary {
            metric: "http_reqs".to_string(),
            kind: MetricKind::Counter,
            agg: AggValues {
                count: 1000,
                sum: 1000.0,
                per_second: Some(rps),
                ..Default::default()
            },
        };
        Summary {
            name: None,
            run_id: "r".to_string(),
            started_ms: 0,
            ended_ms: 0,
            duration_secs: 10.0,
            scenarios: Vec::new(),
            metrics: vec![trend, failed, reqs],
            checks: Vec::new(),
            thresholds: Vec::new(),
            thresholds_passed: true,
            aborted: None,
            snapshot: Snapshot::default(),
            timeline: Vec::new(),
        }
    }

    /// Fake runner: pops canned outcomes and records the combos it saw.
    struct FakeRunner {
        outcomes: Vec<anyhow::Result<RunOutcome>>,
        seen: Vec<Vec<(String, String)>>,
    }

    impl ComboRunner for FakeRunner {
        fn run(
            &mut self,
            combo: &[(String, String)],
            _export: &Path,
        ) -> anyhow::Result<RunOutcome> {
            self.seen.push(combo.to_vec());
            self.outcomes.remove(0)
        }
    }

    #[test]
    fn var_parses_name_and_values() {
        let v = parse_var("vus=10,50,100").expect("valid var");
        assert_eq!(v, var("vus", &["10", "50", "100"]));
        // Whitespace around values is trimmed.
        assert_eq!(
            parse_var("rate= 50 ,100").expect("valid"),
            var("rate", &["50", "100"])
        );
    }

    #[test]
    fn var_rejects_bad_specs() {
        assert!(parse_var("vus").unwrap_err().contains("name=v1,v2"));
        assert!(parse_var("=10").unwrap_err().contains("bad variable name"));
        assert!(parse_var("v us=10")
            .unwrap_err()
            .contains("bad variable name"));
        assert!(parse_var("vus=10,,50").unwrap_err().contains("empty value"));
    }

    #[test]
    fn matrix_expands_the_cartesian_product() {
        let combos = expand_matrix(&[var("vus", &["10", "50"]), var("rate", &["1", "2", "3"])]);
        assert_eq!(combos.len(), 6);
        assert_eq!(combos[0], combo(&[("vus", "10"), ("rate", "1")]));
        assert_eq!(combos[1], combo(&[("vus", "10"), ("rate", "2")]));
        assert_eq!(combos[5], combo(&[("vus", "50"), ("rate", "3")]));
    }

    #[test]
    fn matrix_of_one_axis_is_its_values() {
        let combos = expand_matrix(&[var("vus", &["10", "50", "100"])]);
        assert_eq!(combos.len(), 3);
        assert_eq!(combos[2], combo(&[("vus", "100")]));
        // Degenerate case, rejected by `execute` before it gets here.
        assert_eq!(expand_matrix(&[]), vec![Vec::new()]);
    }

    #[test]
    fn labels_slugs_and_env_names() {
        let c = combo(&[("vus", "10"), ("duration", "1m30s")]);
        assert_eq!(combo_label(&c), "vus=10 duration=1m30s");
        assert_eq!(combo_slug(&c), "vus-10_duration-1m30s");
        // Slugs stay filename-safe.
        assert_eq!(combo_slug(&combo(&[("rate", "50/s")])), "rate-50-s");
        assert_eq!(sweep_env_name("vus"), "LOADR_SWEEP_VUS");
    }

    #[test]
    fn run_sweep_drives_the_runner_in_order() {
        let combos = expand_matrix(&[var("vus", &["10", "50"])]);
        let mut runner = FakeRunner {
            outcomes: vec![
                Ok((summary_with(10.0, 20.0, 30.0, 0.0, 100.0), 0)),
                Ok((summary_with(15.0, 40.0, 80.0, 0.01, 90.0), 99)),
            ],
            seen: Vec::new(),
        };
        let results = run_sweep(&mut runner, &combos, Path::new("out"));
        assert_eq!(runner.seen, combos);
        assert_eq!(results.len(), 2);
        assert_eq!(results[0].label, "vus=10");
        assert_eq!(results[0].exit_code, 0);
        assert_eq!(
            results[0].export,
            Path::new("out").join("sweep-vus-10.json")
        );
        assert_eq!(results[1].exit_code, 99);
        assert!(results[1].summary.is_some());
    }

    #[test]
    fn run_sweep_keeps_going_past_failures() {
        let combos = expand_matrix(&[var("vus", &["10", "50"])]);
        let mut runner = FakeRunner {
            outcomes: vec![
                Err(anyhow::anyhow!("boom")),
                Ok((summary_with(10.0, 20.0, 30.0, 0.0, 100.0), 0)),
            ],
            seen: Vec::new(),
        };
        let results = run_sweep(&mut runner, &combos, Path::new("out"));
        assert_eq!(results.len(), 2);
        assert_eq!(results[0].exit_code, -1);
        assert!(results[0].summary.is_none());
        assert_eq!(results[1].exit_code, 0);
        assert!(results[1].summary.is_some());
    }

    #[test]
    fn matrix_rows_format_cells() {
        let results = vec![
            SweepResult {
                label: "vus=10".to_string(),
                export: PathBuf::from("a.json"),
                exit_code: 0,
                summary: Some(summary_with(20.0, 45.5, 90.0, 0.01, 250.0)),
            },
            SweepResult {
                label: "vus=50".to_string(),
                export: PathBuf::from("b.json"),
                exit_code: -1,
                summary: None,
            },
        ];
        let rows = matrix_rows(&results);
        assert_eq!(
            rows[0],
            ["vus=10", "20.00ms", "45.50ms", "90.00ms", "1.00%", "250.0/s"]
        );
        assert_eq!(rows[1], ["vus=50", "-", "-", "-", "-", "-"]);
    }

    #[test]
    fn matrix_rows_fall_back_to_plugin_metric_families() {
        let mut summary = summary_with(20.0, 45.0, 90.0, 0.0, 100.0);
        for m in &mut summary.metrics {
            m.metric = m.metric.replace("http_", "mongo_");
        }
        let results = vec![SweepResult {
            label: "vus=10".to_string(),
            export: PathBuf::from("a.json"),
            exit_code: 0,
            summary: Some(summary),
        }];
        let rows = matrix_rows(&results);
        assert_eq!(rows[0][2], "45.00ms");
        assert_eq!(rows[0][5], "100.0/s");
    }

    #[test]
    fn markdown_matrix_renders_a_table() {
        let results = vec![SweepResult {
            label: "vus=10".to_string(),
            export: PathBuf::from("a.json"),
            exit_code: 0,
            summary: Some(summary_with(20.0, 45.0, 90.0, 0.01, 250.0)),
        }];
        let md = render_markdown(&results);
        assert!(md.starts_with("## loadr sweep\n\n"));
        assert!(md.contains("| combo | p50 | p95 | p99 | error rate | rps |"));
        assert!(md.contains("| vus=10 | 20.00ms | 45.00ms | 90.00ms | 1.00% | 250.0/s |"));
    }
}
