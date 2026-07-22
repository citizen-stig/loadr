//! The SQLite-backed run-history store (bundled SQLite — no server needed).

use std::path::Path;

use loadr_core::summary::MetricSummary;
use loadr_core::Summary;
use rusqlite::{params, Connection};

#[derive(Debug, thiserror::Error)]
pub enum HistoryError {
    #[error("history db: {0}")]
    Db(#[from] rusqlite::Error),
}

/// One recorded run (header row).
#[derive(Debug, Clone)]
pub struct RunRow {
    pub run_id: String,
    pub plan_id: String,
    pub name: Option<String>,
    pub git_sha: Option<String>,
    pub ts: i64,
    pub thresholds_passed: bool,
}

pub struct Store {
    conn: Connection,
}

impl Store {
    pub fn open(path: &Path) -> Result<Self, HistoryError> {
        if let Some(dir) = path.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        let conn = Connection::open(path)?;
        Self::init(&conn)?;
        Ok(Self { conn })
    }

    /// In-memory store (tests).
    pub fn open_memory() -> Result<Self, HistoryError> {
        let conn = Connection::open_in_memory()?;
        Self::init(&conn)?;
        Ok(Self { conn })
    }

    fn init(conn: &Connection) -> Result<(), HistoryError> {
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS runs (
                run_id TEXT PRIMARY KEY,
                plan_id TEXT NOT NULL,
                name TEXT,
                git_sha TEXT,
                git_ref TEXT,
                ts INTEGER NOT NULL,
                thresholds_passed INTEGER NOT NULL
            );
            CREATE TABLE IF NOT EXISTS metric_values (
                run_id TEXT NOT NULL,
                plan_id TEXT NOT NULL,
                metric TEXT NOT NULL,
                field TEXT NOT NULL,
                value REAL NOT NULL,
                ts INTEGER NOT NULL
            );
            CREATE INDEX IF NOT EXISTS ix_runs_plan ON runs(plan_id, ts);
            CREATE INDEX IF NOT EXISTS ix_mv ON metric_values(plan_id, metric, field, ts);",
        )?;
        Ok(())
    }

    /// Record a run under `plan_id`. Idempotent per `run_id`.
    pub fn record(
        &self,
        s: &Summary,
        plan_id: &str,
        git_sha: Option<&str>,
        git_ref: Option<&str>,
    ) -> Result<usize, HistoryError> {
        let ts = s.ended_ms as i64;
        self.conn.execute(
            "INSERT OR REPLACE INTO runs (run_id, plan_id, name, git_sha, git_ref, ts, thresholds_passed)
             VALUES (?1,?2,?3,?4,?5,?6,?7)",
            params![s.run_id, plan_id, s.name, git_sha, git_ref, ts, s.thresholds_passed as i64],
        )?;
        self.conn.execute(
            "DELETE FROM metric_values WHERE run_id=?1",
            params![s.run_id],
        )?;

        let mut n = 0usize;
        for m in &s.metrics {
            for (field, value) in fields_of(m) {
                self.conn.execute(
                    "INSERT INTO metric_values (run_id, plan_id, metric, field, value, ts)
                     VALUES (?1,?2,?3,?4,?5,?6)",
                    params![s.run_id, plan_id, m.metric, field, value, ts],
                )?;
                n += 1;
            }
        }
        Ok(n)
    }

    /// List recorded runs (optionally for one plan), newest first.
    pub fn list(&self, plan_id: Option<&str>) -> Result<Vec<RunRow>, HistoryError> {
        let sql = "SELECT run_id, plan_id, name, git_sha, ts, thresholds_passed FROM runs
                   WHERE (?1 IS NULL OR plan_id = ?1) ORDER BY ts DESC";
        let mut stmt = self.conn.prepare(sql)?;
        let rows = stmt
            .query_map(params![plan_id], |r| {
                Ok(RunRow {
                    run_id: r.get(0)?,
                    plan_id: r.get(1)?,
                    name: r.get(2)?,
                    git_sha: r.get(3)?,
                    ts: r.get(4)?,
                    thresholds_passed: r.get::<_, i64>(5)? != 0,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(rows)
    }

    /// Prior values of one metric field for a plan, oldest→newest, excluding
    /// `exclude_run` (the run under test).
    pub fn history(
        &self,
        plan_id: &str,
        metric: &str,
        field: &str,
        exclude_run: Option<&str>,
        limit: usize,
    ) -> Result<Vec<f64>, HistoryError> {
        let sql = "SELECT value FROM metric_values
                   WHERE plan_id=?1 AND metric=?2 AND field=?3 AND (?4 IS NULL OR run_id != ?4)
                   ORDER BY ts DESC LIMIT ?5";
        let mut stmt = self.conn.prepare(sql)?;
        let mut vals = stmt
            .query_map(
                params![plan_id, metric, field, exclude_run, limit as i64],
                |r| r.get::<_, f64>(0),
            )?
            .collect::<Result<Vec<_>, _>>()?;
        vals.reverse(); // oldest→newest
        Ok(vals)
    }
}

/// Metric fields worth trending, with the sort direction (`higher_is_worse`).
pub fn fields_of(m: &MetricSummary) -> Vec<(&'static str, f64)> {
    let a = &m.agg;
    let mut v = Vec::new();
    if let Some(x) = a.med {
        v.push(("p50", x));
    }
    if let Some(x) = a.p95 {
        v.push(("p95", x));
    }
    if let Some(x) = a.p99 {
        v.push(("p99", x));
    }
    if let Some(x) = a.avg {
        v.push(("avg", x));
    }
    if let Some(x) = a.rate {
        v.push(("rate", x));
    }
    if let Some(x) = a.per_second {
        v.push(("rps", x));
    }
    v
}

/// Whether a higher value of `(metric, field)` is worse (latency, error rate)
/// or better (throughput).
pub fn higher_is_worse(metric: &str, field: &str) -> bool {
    !(field == "rps" || field == "per_second" || metric == "http_reqs" || metric == "iterations")
}
