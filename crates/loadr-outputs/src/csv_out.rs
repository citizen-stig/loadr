//! CSV output: one row per sample.
//!
//! Wire format: header `timestamp_ms,metric,kind,value,tags`, then one row per
//! sample with tags rendered as `k=v;k2=v2`.

use std::fs::File;
use std::path::PathBuf;

use async_trait::async_trait;
use loadr_core::error::EngineError;
use loadr_core::metrics::{Sample, Tags};
use loadr_core::output::Output;
use loadr_core::summary::Summary;

/// Writes samples as CSV rows to a file.
pub struct CsvOutput {
    path: PathBuf,
    writer: Option<csv::Writer<File>>,
}

impl CsvOutput {
    /// Create a CSV output writing to `path`. Relative paths resolve against
    /// the current working directory.
    pub fn new(path: impl Into<PathBuf>) -> Self {
        CsvOutput {
            path: path.into(),
            writer: None,
        }
    }
}

/// Render tags as `k=v;k2=v2`.
fn format_tags(tags: &Tags) -> String {
    let mut out = String::new();
    for (i, (k, v)) in tags.iter().enumerate() {
        if i > 0 {
            out.push(';');
        }
        out.push_str(k);
        out.push('=');
        out.push_str(v);
    }
    out
}

#[async_trait]
impl Output for CsvOutput {
    fn name(&self) -> &str {
        "csv"
    }

    async fn start(&mut self) -> Result<(), EngineError> {
        let file = File::create(&self.path).map_err(|source| EngineError::Io {
            path: self.path.display().to_string(),
            source,
        })?;
        let mut writer = csv::Writer::from_writer(file);
        writer
            .write_record(["timestamp_ms", "metric", "kind", "value", "tags"])
            .map_err(|err| EngineError::Other(format!("csv output header: {err}")))?;
        self.writer = Some(writer);
        Ok(())
    }

    async fn on_samples(&mut self, samples: &[Sample]) {
        let Some(writer) = &mut self.writer else {
            return;
        };
        for s in samples {
            let record = [
                s.timestamp_ms.to_string(),
                s.metric.to_string(),
                s.kind.as_str().to_string(),
                s.value.to_string(),
                format_tags(&s.tags),
            ];
            if let Err(err) = writer.write_record(&record) {
                tracing::warn!(path = %self.path.display(), error = %err, "csv output write failed");
                return;
            }
        }
    }

    async fn finish(&mut self, _summary: &Summary) {
        if let Some(writer) = &mut self.writer {
            if let Err(err) = writer.flush() {
                tracing::warn!(path = %self.path.display(), error = %err, "csv output flush failed");
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::{fixture_samples, fixture_summary};

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn writes_header_and_rows() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("out.csv");
        let mut out = CsvOutput::new(path.clone());
        out.start().await.expect("start");

        let samples = fixture_samples();
        out.on_samples(&samples).await;
        out.finish(&fixture_summary()).await;

        let mut reader = csv::Reader::from_path(&path).expect("open csv");
        let headers = reader.headers().expect("headers").clone();
        assert_eq!(
            headers.iter().collect::<Vec<_>>(),
            vec!["timestamp_ms", "metric", "kind", "value", "tags"]
        );
        let rows: Vec<csv::StringRecord> = reader
            .records()
            .collect::<Result<_, _>>()
            .expect("rows parse");
        assert_eq!(rows.len(), samples.len());
        let trend = rows
            .iter()
            .find(|r| &r[1] == "http_req_duration")
            .expect("trend row");
        assert_eq!(&trend[2], "trend");
        assert_eq!(&trend[4], "method=GET;status=200");
        assert!(trend[0].parse::<u64>().is_ok());
        assert!(trend[3].parse::<f64>().is_ok());
    }
}
