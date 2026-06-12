//! Data parameterization: CSV and inline data sources with shared or per-VU
//! cursors and recycle/stop-at-EOF semantics.

use std::collections::HashMap;
use std::path::Path;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

use indexmap::IndexMap;
use loadr_config::{DataMode, DataSource, OnEof, PickStrategy};
use rand::seq::SliceRandom;
use rand::RngExt;

use crate::error::EngineError;

/// One data row: column name → string value.
pub type Row = IndexMap<String, String>;

#[derive(Debug)]
struct Feed {
    rows: Vec<Arc<Row>>,
    mode: DataMode,
    on_eof: OnEof,
    pick: PickStrategy,
    shared_cursor: AtomicUsize,
}

/// Per-VU feeder state: sequential cursors and per-VU shuffle orders.
#[derive(Debug, Default)]
pub struct VuFeedState {
    cursors: HashMap<String, usize>,
    shuffles: HashMap<String, Vec<usize>>,
}

impl VuFeedState {
    pub fn new() -> Self {
        Self::default()
    }
}

/// All loaded data sources for a test.
#[derive(Debug, Default)]
pub struct DataFeeds {
    feeds: HashMap<String, Feed>,
}

/// Signalled when a `stop`-mode source is exhausted: the VU should retire.
#[derive(Debug, thiserror::Error)]
#[error("data source `{0}` is exhausted")]
pub struct EndOfData(pub String);

impl DataFeeds {
    /// Load every source declared in the plan. CSV paths resolve against `base_dir`.
    pub fn load(
        sources: &IndexMap<String, DataSource>,
        base_dir: &Path,
    ) -> Result<DataFeeds, EngineError> {
        let mut feeds = HashMap::new();
        for (name, source) in sources {
            let feed = match source {
                DataSource::Csv {
                    path,
                    mode,
                    on_eof,
                    pick,
                    delimiter,
                    has_header,
                } => {
                    let resolved = if path.is_absolute() {
                        path.clone()
                    } else {
                        base_dir.join(path)
                    };
                    let raw = std::fs::read(&resolved).map_err(|e| EngineError::Data {
                        source_name: name.clone(),
                        message: format!("cannot read {}: {e}", resolved.display()),
                    })?;
                    let mut builder = csv::ReaderBuilder::new();
                    builder.has_headers(*has_header);
                    if let Some(d) = delimiter {
                        builder.delimiter(*d as u8);
                    }
                    let mut reader = builder.from_reader(raw.as_slice());
                    let headers: Vec<String> = if *has_header {
                        reader
                            .headers()
                            .map_err(|e| EngineError::Data {
                                source_name: name.clone(),
                                message: e.to_string(),
                            })?
                            .iter()
                            .map(str::to_string)
                            .collect()
                    } else {
                        Vec::new()
                    };
                    let mut rows = Vec::new();
                    for record in reader.records() {
                        let record = record.map_err(|e| EngineError::Data {
                            source_name: name.clone(),
                            message: e.to_string(),
                        })?;
                        let mut row = Row::new();
                        for (i, field) in record.iter().enumerate() {
                            let key = headers.get(i).cloned().unwrap_or_else(|| format!("col{i}"));
                            row.insert(key, field.to_string());
                        }
                        rows.push(Arc::new(row));
                    }
                    if rows.is_empty() {
                        return Err(EngineError::Data {
                            source_name: name.clone(),
                            message: "CSV has no data rows".into(),
                        });
                    }
                    make_feed(rows, *mode, *on_eof, *pick)
                }
                DataSource::Json {
                    path,
                    mode,
                    on_eof,
                    pick,
                } => {
                    let resolved = if path.is_absolute() {
                        path.clone()
                    } else {
                        base_dir.join(path)
                    };
                    let raw = std::fs::read(&resolved).map_err(|e| EngineError::Data {
                        source_name: name.clone(),
                        message: format!("cannot read {}: {e}", resolved.display()),
                    })?;
                    let parsed: serde_json::Value =
                        serde_json::from_slice(&raw).map_err(|e| EngineError::Data {
                            source_name: name.clone(),
                            message: format!("invalid JSON: {e}"),
                        })?;
                    let array = parsed.as_array().ok_or_else(|| EngineError::Data {
                        source_name: name.clone(),
                        message: "JSON data source must be an array of objects".into(),
                    })?;
                    let mut rows = Vec::with_capacity(array.len());
                    for item in array {
                        let obj = item.as_object().ok_or_else(|| EngineError::Data {
                            source_name: name.clone(),
                            message: "each JSON data row must be an object".into(),
                        })?;
                        let row: Row = obj
                            .iter()
                            .map(|(k, v)| (k.clone(), json_to_string(v)))
                            .collect();
                        rows.push(Arc::new(row));
                    }
                    if rows.is_empty() {
                        return Err(EngineError::Data {
                            source_name: name.clone(),
                            message: "JSON data source is empty".into(),
                        });
                    }
                    make_feed(rows, *mode, *on_eof, *pick)
                }
                DataSource::Inline {
                    rows,
                    mode,
                    on_eof,
                    pick,
                } => {
                    let converted: Vec<Arc<Row>> = rows
                        .iter()
                        .map(|r| {
                            Arc::new(
                                r.iter()
                                    .map(|(k, v)| (k.clone(), json_to_string(v)))
                                    .collect::<Row>(),
                            )
                        })
                        .collect();
                    if converted.is_empty() {
                        return Err(EngineError::Data {
                            source_name: name.clone(),
                            message: "inline data has no rows".into(),
                        });
                    }
                    make_feed(converted, *mode, *on_eof, *pick)
                }
            };
            feeds.insert(name.clone(), feed);
        }
        Ok(DataFeeds { feeds })
    }

    pub fn has_source(&self, name: &str) -> bool {
        self.feeds.contains_key(name)
    }

    pub fn source_names(&self) -> Vec<&str> {
        self.feeds.keys().map(String::as_str).collect()
    }

    /// Fetch the next row for `source`, honoring its mode, pick strategy and
    /// EOF behaviour. `state` holds per-VU cursors/shuffles; `rng` drives random
    /// and shuffle selection.
    pub fn next_row(
        &self,
        source: &str,
        state: &mut VuFeedState,
        rng: &mut impl RngExt,
    ) -> Result<Arc<Row>, NextRowError> {
        let feed = self
            .feeds
            .get(source)
            .ok_or_else(|| NextRowError::UnknownSource(source.to_string()))?;
        let len = feed.rows.len();

        // Random: independent uniform pick every call; never exhausts.
        if feed.pick == PickStrategy::Random {
            let i = rng.random_range(0..len);
            return Ok(feed.rows[i].clone());
        }

        // Sequential / shuffle both walk an order with a cursor.
        let cursor = match feed.mode {
            DataMode::Shared => feed.shared_cursor.fetch_add(1, Ordering::Relaxed),
            DataMode::PerVu => {
                let c = state.cursors.entry(source.to_string()).or_insert(0);
                let idx = *c;
                *c += 1;
                idx
            }
        };

        // EOF policy applies to the cursor before resolving through the order.
        if cursor >= len && feed.on_eof == OnEof::Stop {
            return Err(NextRowError::Exhausted(EndOfData(source.to_string())));
        }

        let row_index = match feed.pick {
            PickStrategy::Shuffle => {
                // Shuffle = a per-VU permutation of row indices, walked in order.
                let order = state.shuffles.entry(source.to_string()).or_insert_with(|| {
                    let mut order: Vec<usize> = (0..len).collect();
                    order.shuffle(rng);
                    order
                });
                order[cursor % len]
            }
            _ => cursor % len,
        };
        Ok(feed.rows[row_index].clone())
    }
}

fn make_feed(rows: Vec<Arc<Row>>, mode: DataMode, on_eof: OnEof, pick: PickStrategy) -> Feed {
    Feed {
        rows,
        mode,
        on_eof,
        pick,
        shared_cursor: AtomicUsize::new(0),
    }
}

#[derive(Debug, thiserror::Error)]
pub enum NextRowError {
    #[error("unknown data source `{0}`")]
    UnknownSource(String),
    #[error(transparent)]
    Exhausted(#[from] EndOfData),
}

fn json_to_string(v: &serde_json::Value) -> String {
    match v {
        serde_json::Value::String(s) => s.clone(),
        other => other.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use loadr_config::PickStrategy;
    use rand::SeedableRng;
    use std::io::Write;

    fn rng() -> rand::rngs::SmallRng {
        rand::rngs::SmallRng::seed_from_u64(42)
    }

    fn csv_feeds(
        dir: &Path,
        content: &str,
        mode: DataMode,
        on_eof: OnEof,
        pick: PickStrategy,
    ) -> DataFeeds {
        let path = dir.join("users.csv");
        let mut f = std::fs::File::create(&path).expect("create");
        f.write_all(content.as_bytes()).expect("write");
        let mut sources = IndexMap::new();
        sources.insert(
            "users".to_string(),
            DataSource::Csv {
                path: "users.csv".into(),
                mode,
                on_eof,
                pick,
                delimiter: None,
                has_header: true,
            },
        );
        DataFeeds::load(&sources, dir).expect("load")
    }

    #[test]
    fn shared_cursor_distributes_rows() {
        let dir = tempfile::tempdir().expect("tmp");
        let feeds = csv_feeds(
            dir.path(),
            "user,pass\nu1,p1\nu2,p2\nu3,p3\n",
            DataMode::Shared,
            OnEof::Recycle,
            PickStrategy::Sequential,
        );
        let mut st = VuFeedState::new();
        let mut r = rng();
        let r1 = feeds.next_row("users", &mut st, &mut r).expect("row");
        let r2 = feeds.next_row("users", &mut st, &mut r).expect("row");
        assert_eq!(r1["user"], "u1");
        assert_eq!(r2["user"], "u2");
    }

    #[test]
    fn recycle_wraps() {
        let dir = tempfile::tempdir().expect("tmp");
        let feeds = csv_feeds(
            dir.path(),
            "user\nu1\nu2\n",
            DataMode::Shared,
            OnEof::Recycle,
            PickStrategy::Sequential,
        );
        let mut st = VuFeedState::new();
        let mut r = rng();
        for _ in 0..2 {
            feeds.next_row("users", &mut st, &mut r).expect("row");
        }
        let wrapped = feeds.next_row("users", &mut st, &mut r).expect("row");
        assert_eq!(wrapped["user"], "u1");
    }

    #[test]
    fn stop_at_eof() {
        let dir = tempfile::tempdir().expect("tmp");
        let feeds = csv_feeds(
            dir.path(),
            "user\nu1\n",
            DataMode::Shared,
            OnEof::Stop,
            PickStrategy::Sequential,
        );
        let mut st = VuFeedState::new();
        let mut r = rng();
        feeds.next_row("users", &mut st, &mut r).expect("row");
        assert!(matches!(
            feeds.next_row("users", &mut st, &mut r),
            Err(NextRowError::Exhausted(_))
        ));
    }

    #[test]
    fn per_vu_cursors_are_independent() {
        let dir = tempfile::tempdir().expect("tmp");
        let feeds = csv_feeds(
            dir.path(),
            "user\nu1\nu2\n",
            DataMode::PerVu,
            OnEof::Recycle,
            PickStrategy::Sequential,
        );
        let (mut vu1, mut vu2) = (VuFeedState::new(), VuFeedState::new());
        let mut r = rng();
        assert_eq!(
            feeds.next_row("users", &mut vu1, &mut r).unwrap()["user"],
            "u1"
        );
        assert_eq!(
            feeds.next_row("users", &mut vu2, &mut r).unwrap()["user"],
            "u1"
        );
        assert_eq!(
            feeds.next_row("users", &mut vu1, &mut r).unwrap()["user"],
            "u2"
        );
    }

    #[test]
    fn random_pick_stays_in_range_and_never_exhausts() {
        let dir = tempfile::tempdir().expect("tmp");
        let feeds = csv_feeds(
            dir.path(),
            "user\nu1\nu2\nu3\n",
            DataMode::Shared,
            OnEof::Stop,
            PickStrategy::Random,
        );
        let mut st = VuFeedState::new();
        let mut r = rng();
        let mut seen = std::collections::BTreeSet::new();
        for _ in 0..200 {
            let row = feeds
                .next_row("users", &mut st, &mut r)
                .expect("random never stops");
            seen.insert(row["user"].clone());
        }
        assert!(seen.len() > 1, "random should visit multiple rows");
        assert!(seen
            .iter()
            .all(|u| ["u1", "u2", "u3"].contains(&u.as_str())));
    }

    #[test]
    fn shuffle_covers_all_rows_once_per_cycle() {
        let dir = tempfile::tempdir().expect("tmp");
        let feeds = csv_feeds(
            dir.path(),
            "user\nu1\nu2\nu3\nu4\n",
            DataMode::PerVu,
            OnEof::Recycle,
            PickStrategy::Shuffle,
        );
        let mut st = VuFeedState::new();
        let mut r = rng();
        let mut first_cycle = std::collections::BTreeSet::new();
        for _ in 0..4 {
            let row = feeds.next_row("users", &mut st, &mut r).expect("row");
            first_cycle.insert(row["user"].clone());
        }
        // A full cycle visits every row exactly once (just shuffled order).
        assert_eq!(first_cycle.len(), 4);
    }

    #[test]
    fn json_source_loads_objects() {
        let dir = tempfile::tempdir().expect("tmp");
        std::fs::write(
            dir.path().join("d.json"),
            r#"[{"id":1,"name":"alpha"},{"id":2,"name":"beta"}]"#,
        )
        .expect("write");
        let mut sources = IndexMap::new();
        sources.insert(
            "items".to_string(),
            DataSource::Json {
                path: "d.json".into(),
                mode: DataMode::Shared,
                on_eof: OnEof::Recycle,
                pick: PickStrategy::Sequential,
            },
        );
        let feeds = DataFeeds::load(&sources, dir.path()).expect("load");
        let mut st = VuFeedState::new();
        let mut r = rng();
        let row = feeds.next_row("items", &mut st, &mut r).expect("row");
        assert_eq!(row["id"], "1");
        assert_eq!(row["name"], "alpha");
    }

    #[test]
    fn inline_rows() {
        let mut sources = IndexMap::new();
        let mut row = IndexMap::new();
        row.insert("id".to_string(), serde_json::json!(7));
        row.insert("name".to_string(), serde_json::json!("alpha"));
        sources.insert(
            "items".to_string(),
            DataSource::Inline {
                rows: vec![row],
                mode: DataMode::Shared,
                on_eof: OnEof::Recycle,
                pick: PickStrategy::Sequential,
            },
        );
        let feeds = DataFeeds::load(&sources, Path::new(".")).expect("load");
        let mut st = VuFeedState::new();
        let mut r = rng();
        let row = feeds.next_row("items", &mut st, &mut r).expect("row");
        assert_eq!(row["id"], "7");
        assert_eq!(row["name"], "alpha");
    }

    #[test]
    fn headerless_csv_gets_positional_columns() {
        let dir = tempfile::tempdir().expect("tmp");
        let path = dir.path().join("data.csv");
        std::fs::write(&path, "a,b\nc,d\n").expect("write");
        let mut sources = IndexMap::new();
        sources.insert(
            "d".to_string(),
            DataSource::Csv {
                path: "data.csv".into(),
                mode: DataMode::Shared,
                on_eof: OnEof::Recycle,
                pick: PickStrategy::Sequential,
                delimiter: None,
                has_header: false,
            },
        );
        let feeds = DataFeeds::load(&sources, dir.path()).expect("load");
        let mut st = VuFeedState::new();
        let mut r = rng();
        let row = feeds.next_row("d", &mut st, &mut r).expect("row");
        assert_eq!(row["col0"], "a");
        assert_eq!(row["col1"], "b");
    }
}
