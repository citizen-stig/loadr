//! Durable run-history store + statistical regression detection.
//!
//! `loadr history record` writes a run's metrics to a local SQLite database;
//! `loadr history check` compares the newest run against its recorded history
//! with a robust modified z-score, flagging regressions the way `compare` does
//! for two runs — but against a window, so a single noisy CI run can't false-
//! alarm.

pub mod stats;
pub mod store;

pub use stats::{detect, Verdict};
pub use store::{fields_of, higher_is_worse, HistoryError, RunRow, Store};

use loadr_core::Summary;

/// One field's regression check.
pub struct CheckRow {
    pub metric: String,
    pub field: String,
    pub verdict: Verdict,
}

/// A stable plan id derived from a summary's scenario set + name — groups "the
/// same test" across runs when the full `TestPlan` isn't at hand. Override with
/// an explicit `--plan`.
pub fn plan_id_from_summary(s: &Summary) -> String {
    let mut scen = s.scenarios.clone();
    scen.sort();
    format!("{}|{}", s.name.clone().unwrap_or_default(), scen.join(","))
}

/// Check a run's metrics against its recorded history (excluding the run
/// itself). Returns one row per trended field that has prior history.
pub fn check(
    store: &Store,
    s: &Summary,
    plan_id: &str,
    window: usize,
) -> Result<Vec<CheckRow>, HistoryError> {
    let mut rows = Vec::new();
    for m in &s.metrics {
        for (field, value) in store::fields_of(m) {
            let hist = store.history(plan_id, &m.metric, field, Some(&s.run_id), window)?;
            if hist.is_empty() {
                continue;
            }
            let verdict = stats::detect(value, &hist, store::higher_is_worse(&m.metric, field));
            rows.push(CheckRow {
                metric: m.metric.clone(),
                field: field.to_string(),
                verdict,
            });
        }
    }
    Ok(rows)
}
