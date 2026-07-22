//! `loadr history` — durable run history + statistical regression detection.

use std::path::PathBuf;

use clap::{Args, Subcommand};
use owo_colors::OwoColorize;

use loadr_core::Summary;
use loadr_history::{check, plan_id_from_summary, Store};

#[derive(Subcommand)]
pub enum HistoryCommand {
    /// Record a run summary into the history database
    Record(RecordArgs),
    /// List recorded runs
    List(ListArgs),
    /// Flag statistical regressions of a run against its recorded history
    Check(CheckArgs),
}

#[derive(Args)]
pub struct RecordArgs {
    /// A `loadr run --summary-export` JSON file
    pub summary: PathBuf,
    /// History database path
    #[arg(long, default_value = ".loadr/history.db")]
    pub db: PathBuf,
    /// Plan id to group under (default: derived from the summary)
    #[arg(long)]
    pub plan: Option<String>,
    #[arg(long)]
    pub git_sha: Option<String>,
    #[arg(long)]
    pub git_ref: Option<String>,
}

#[derive(Args)]
pub struct ListArgs {
    #[arg(long, default_value = ".loadr/history.db")]
    pub db: PathBuf,
    /// Only runs for this plan id
    #[arg(long)]
    pub plan: Option<String>,
}

#[derive(Args)]
pub struct CheckArgs {
    /// A `loadr run --summary-export` JSON file (the run under test)
    pub summary: PathBuf,
    #[arg(long, default_value = ".loadr/history.db")]
    pub db: PathBuf,
    #[arg(long)]
    pub plan: Option<String>,
    /// How many prior runs to compare against
    #[arg(long, default_value_t = 20)]
    pub window: usize,
    /// Exit 99 if a regression is found (default on)
    #[arg(long, default_value_t = true)]
    pub assert: bool,
}

pub fn execute(cmd: HistoryCommand) -> anyhow::Result<i32> {
    match cmd {
        HistoryCommand::Record(a) => record(a),
        HistoryCommand::List(a) => list(a),
        HistoryCommand::Check(a) => check_cmd(a),
    }
}

fn read_summary(path: &std::path::Path) -> anyhow::Result<Summary> {
    let raw = std::fs::read_to_string(path)
        .map_err(|e| anyhow::anyhow!("cannot read {}: {e}", path.display()))?;
    serde_json::from_str(&raw).map_err(|e| anyhow::anyhow!("not a loadr summary JSON: {e}"))
}

fn record(a: RecordArgs) -> anyhow::Result<i32> {
    let s = read_summary(&a.summary)?;
    let plan = a.plan.clone().unwrap_or_else(|| plan_id_from_summary(&s));
    let store = Store::open(&a.db)?;
    let n = store.record(&s, &plan, a.git_sha.as_deref(), a.git_ref.as_deref())?;
    eprintln!(
        "{} recorded run {} ({n} metric value(s)) under plan {}",
        "history:".green().bold(),
        s.run_id.dimmed(),
        plan.dimmed()
    );
    Ok(0)
}

fn list(a: ListArgs) -> anyhow::Result<i32> {
    let store = Store::open(&a.db)?;
    let runs = store.list(a.plan.as_deref())?;
    if runs.is_empty() {
        eprintln!("{} no recorded runs", "history:".yellow().bold());
        return Ok(0);
    }
    println!("{:<24} {:<20} {:<8} when(ms)", "run", "plan", "slo");
    for r in runs {
        let slo = if r.thresholds_passed {
            "pass".green().to_string()
        } else {
            "FAIL".red().to_string()
        };
        let plan = if r.plan_id.len() > 18 {
            format!("{}…", &r.plan_id[..17])
        } else {
            r.plan_id.clone()
        };
        println!(
            "{:<24} {:<20} {:<8} {}",
            r.name.as_deref().unwrap_or(&r.run_id),
            plan,
            slo,
            r.ts
        );
    }
    Ok(0)
}

fn check_cmd(a: CheckArgs) -> anyhow::Result<i32> {
    let s = read_summary(&a.summary)?;
    let plan = a.plan.clone().unwrap_or_else(|| plan_id_from_summary(&s));
    let store = Store::open(&a.db)?;
    let rows = check(&store, &s, &plan, a.window)?;

    if rows.is_empty() {
        eprintln!(
            "{} no prior history for plan {} — record some runs first",
            "history:".yellow().bold(),
            plan.dimmed()
        );
        return Ok(0);
    }

    println!(
        "{:<22} {:>10} {:>10} {:>7} {:>6}  verdict",
        "metric.field", "value", "median", "z", "n"
    );
    let mut regressions = 0;
    for r in &rows {
        let v = &r.verdict;
        let verdict = if v.regression {
            regressions += 1;
            "✗ REGRESSION".red().bold().to_string()
        } else if v.low_confidence {
            "~ low-conf".yellow().to_string()
        } else {
            "✓ ok".green().to_string()
        };
        println!(
            "{:<22} {:>10.1} {:>10.1} {:>7.1} {:>6}  {verdict}",
            format!("{}.{}", r.metric, r.field),
            v.value,
            v.median,
            v.z,
            v.sample_n
        );
    }

    if regressions > 0 {
        eprintln!(
            "{} {regressions} regression(s) against {} prior run(s)",
            "history:".red().bold(),
            rows.first().map(|r| r.verdict.sample_n).unwrap_or(0)
        );
        if a.assert {
            return Ok(99);
        }
    } else {
        eprintln!("{} no regressions", "history:".green().bold());
    }
    Ok(0)
}
