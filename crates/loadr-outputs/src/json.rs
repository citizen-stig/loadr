//! Newline-delimited JSON output.
//!
//! Wire format (one JSON object per line):
//! - `{"type":"point","metric":...,"kind":...,"value":...,"tags":{...},"time_ms":...}`
//!   for every sample,
//! - `{"type":"snapshot","time_ms":...,"elapsed_secs":...,"interval_secs":...,"series":[...]}`
//!   once per snapshot (aggregates with `null` fields trimmed),
//! - `{"type":"summary",...}` once at the end of the run.

use std::fs::File;
use std::io::{BufWriter, Write as _};
use std::path::PathBuf;

use async_trait::async_trait;
use loadr_core::aggregate::{AggValues, Snapshot};
use loadr_core::error::EngineError;
use loadr_core::metrics::Sample;
use loadr_core::output::Output;
use loadr_core::summary::Summary;
use serde_json::{json, Value};

/// Writes newline-delimited JSON to a file.
pub struct JsonOutput {
    path: PathBuf,
    writer: Option<BufWriter<File>>,
}

impl JsonOutput {
    /// Create a JSON output writing to `path`. Relative paths resolve against
    /// the current working directory.
    pub fn new(path: impl Into<PathBuf>) -> Self {
        JsonOutput {
            path: path.into(),
            writer: None,
        }
    }

    fn write_line(&mut self, value: &Value) {
        if let Some(w) = &mut self.writer {
            if let Err(err) = writeln!(w, "{value}") {
                tracing::warn!(path = %self.path.display(), error = %err, "json output write failed");
            }
        }
    }
}

/// Serialize aggregates, dropping `null` (absent) fields.
fn trim_agg(agg: &AggValues) -> Value {
    match serde_json::to_value(agg) {
        Ok(Value::Object(map)) => {
            Value::Object(map.into_iter().filter(|(_, v)| !v.is_null()).collect())
        }
        Ok(other) => other,
        Err(_) => Value::Null,
    }
}

#[async_trait]
impl Output for JsonOutput {
    fn name(&self) -> &str {
        "json"
    }

    async fn start(&mut self) -> Result<(), EngineError> {
        let file = File::create(&self.path).map_err(|source| EngineError::Io {
            path: self.path.display().to_string(),
            source,
        })?;
        self.writer = Some(BufWriter::new(file));
        Ok(())
    }

    async fn on_samples(&mut self, samples: &[Sample]) {
        for s in samples {
            let line = json!({
                "type": "point",
                "metric": &*s.metric,
                "kind": s.kind.as_str(),
                "value": s.value,
                "tags": &*s.tags,
                "time_ms": s.timestamp_ms,
            });
            self.write_line(&line);
        }
    }

    async fn on_snapshot(&mut self, snapshot: &Snapshot) {
        let series: Vec<Value> = snapshot
            .series
            .iter()
            .map(|s| {
                json!({
                    "metric": s.metric,
                    "kind": s.kind.as_str(),
                    "tags": s.tags,
                    "agg": trim_agg(&s.agg),
                    "interval_count": s.interval_count,
                    "interval_sum": s.interval_sum,
                })
            })
            .collect();
        let line = json!({
            "type": "snapshot",
            "time_ms": snapshot.timestamp_ms,
            "elapsed_secs": snapshot.elapsed_secs,
            "interval_secs": snapshot.interval_secs,
            "series": series,
        });
        self.write_line(&line);
    }

    async fn finish(&mut self, summary: &Summary) {
        let mut value = serde_json::to_value(summary).unwrap_or(Value::Null);
        if let Value::Object(map) = &mut value {
            map.insert("type".to_string(), Value::String("summary".to_string()));
            let line = Value::Object(std::mem::take(map));
            self.write_line(&line);
        } else {
            self.write_line(&json!({ "type": "summary", "summary": value }));
        }
        if let Some(w) = &mut self.writer {
            if let Err(err) = w.flush() {
                tracing::warn!(path = %self.path.display(), error = %err, "json output flush failed");
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::{fixture_samples, fixture_snapshot, fixture_summary};

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn writes_points_snapshot_and_summary() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("out.jsonl");
        let mut out = JsonOutput::new(path.clone());
        out.start().await.expect("start");

        out.on_samples(&fixture_samples()).await;
        out.on_snapshot(&fixture_snapshot()).await;
        out.finish(&fixture_summary()).await;

        let text = std::fs::read_to_string(&path).expect("read");
        let lines: Vec<Value> = text
            .lines()
            .map(|l| serde_json::from_str(l).expect("valid json line"))
            .collect();
        assert!(lines.len() >= 3);

        let points: Vec<&Value> = lines.iter().filter(|l| l["type"] == "point").collect();
        assert_eq!(points.len(), fixture_samples().len());
        let p = points
            .iter()
            .find(|p| p["metric"] == "http_req_duration")
            .expect("trend point");
        assert_eq!(p["kind"], "trend");
        assert_eq!(p["tags"]["method"], "GET");
        assert!(p["time_ms"].as_u64().is_some());

        let snap = lines
            .iter()
            .find(|l| l["type"] == "snapshot")
            .expect("snapshot line");
        let series = snap["series"].as_array().expect("series");
        let trend = series
            .iter()
            .find(|s| s["metric"] == "http_req_duration")
            .expect("trend series");
        // Trimmed aggregates: trends have p95 but no `rate`/`last`.
        assert!(trend["agg"]["p95"].is_number());
        assert!(trend["agg"].get("rate").is_none());
        assert!(trend["agg"].get("last").is_none());

        let summary = lines
            .iter()
            .find(|l| l["type"] == "summary")
            .expect("summary line");
        assert_eq!(summary["run_id"], "run-1");
        assert!(summary["metrics"].as_array().is_some());
    }
}
