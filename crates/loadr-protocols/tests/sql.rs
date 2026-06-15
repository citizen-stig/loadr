//! Integration tests for the SQL protocol handler against real PostgreSQL and
//! MySQL servers.
//!
//! These tests connect to live databases, so they only run when the relevant
//! connection URL is provided in the environment (they no-op otherwise). The
//! example harness brings the servers up via Docker:
//!
//! ```sh
//! docker compose -f examples/harness/docker-compose.yml up -d postgres mysql
//! LOADR_TEST_POSTGRES_URL=postgres://loadr:loadr@127.0.0.1:5432/loadr \
//! LOADR_TEST_MYSQL_URL=mysql://loadr:loadr@127.0.0.1:3306/loadr \
//!   cargo test -p loadr-protocols --test sql
//! ```
//!
//! The seed schema matches `examples/harness/sql/{postgres,mysql}-init.sql`.

use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use bytes::Bytes;
use loadr_core::data::DataFeeds;
use loadr_core::metrics::{MetricRegistry, MetricsBus, Tags};
use loadr_core::protocol::{PreparedRequest, ProtocolHandler, RequestOptions};
use loadr_core::vu::{RunContext, VuContext};
use loadr_protocols::SqlHandler;

fn vu() -> VuContext {
    let data = DataFeeds::load(&Default::default(), Path::new(".")).expect("data feeds");
    let run = Arc::new(RunContext {
        variables: serde_json::Map::new(),
        secrets: Default::default(),
        env: Default::default(),
        data,
        registry: Arc::new(MetricRegistry::with_builtins()),
        base_dir: ".".into(),
        setup_data: parking_lot::RwLock::new(serde_json::Value::Null),
    });
    let (bus, _rx) = MetricsBus::new();
    VuContext::new(1, Arc::from("t"), Arc::new(Tags::new()), bus, run, true)
}

fn query(url: &str, sql: &str, params: &[&str]) -> PreparedRequest {
    let mut request = PreparedRequest {
        name: url.to_string(),
        protocol: "sql".to_string(),
        method: "GET".to_string(),
        url: url.to_string(),
        headers: Vec::new(),
        body: Bytes::new(),
        timeout: Duration::from_secs(10),
        follow_redirects: false,
        max_redirects: 0,
        options: RequestOptions::default(),
    };
    let p: Vec<serde_json::Value> = params.iter().map(|s| serde_json::json!(s)).collect();
    request.options.plugin = Some(serde_json::json!({ "query": sql, "params": p }));
    request
}

// ---------------------------------------------------------------------------
// PostgreSQL
// ---------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn postgres_select_and_insert() {
    let Ok(url) = std::env::var("LOADR_TEST_POSTGRES_URL") else {
        eprintln!("skipping: LOADR_TEST_POSTGRES_URL not set");
        return;
    };
    let handler = SqlHandler::new();
    let mut vu = vu();

    // A SELECT returns rows; first call must connect, second must reuse the pool.
    let select = query(
        &url,
        "SELECT id, name FROM products WHERE price < $1",
        &["50"],
    );
    let resp = handler.execute(&mut vu, &select).await.expect("select");
    assert!(resp.error.is_none(), "select error: {:?}", resp.error);
    assert_eq!(resp.status, 0);
    assert_eq!(resp.protocol_version, "sql");
    assert_eq!(resp.extras["backend"], "postgres");
    let rows = resp.extras["rows"].as_u64().unwrap();
    assert!(rows >= 1, "expected seeded rows, got {rows}");
    assert!(resp.timings.connect_ms > 0.0, "first call must connect");
    assert!(resp.timings.waiting_ms >= 0.0);

    // Reuse the pooled connection.
    let again = query(&url, "SELECT COUNT(*) FROM products", &[]);
    let resp2 = handler.execute(&mut vu, &again).await.expect("select2");
    assert!(resp2.error.is_none());
    assert_eq!(
        resp2.timings.connect_ms, 0.0,
        "second call must reuse the pool"
    );

    // A parameterised INSERT reports affected rows.
    let insert = query(
        &url,
        "INSERT INTO products (name, price, stock) VALUES ($1, $2, $3)",
        &["test-item", "1.50", "3"],
    );
    let resp3 = handler.execute(&mut vu, &insert).await.expect("insert");
    assert!(resp3.error.is_none(), "insert error: {:?}", resp3.error);
    assert_eq!(resp3.extras["rows"].as_u64().unwrap(), 1, "1 row affected");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn postgres_error_is_reported() {
    let Ok(url) = std::env::var("LOADR_TEST_POSTGRES_URL") else {
        eprintln!("skipping: LOADR_TEST_POSTGRES_URL not set");
        return;
    };
    let handler = SqlHandler::new();
    let mut vu = vu();

    let bad = query(&url, "SELECT * FROM table_that_does_not_exist", &[]);
    let resp = handler.execute(&mut vu, &bad).await.expect("execute");
    assert_ne!(resp.status, 0, "db error must yield non-zero status");
    assert!(resp.error.is_some(), "db error must be reported");
    assert!(resp.failed(), "errored response must be failed");
}

// ---------------------------------------------------------------------------
// MySQL
// ---------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn mysql_select_and_insert() {
    let Ok(url) = std::env::var("LOADR_TEST_MYSQL_URL") else {
        eprintln!("skipping: LOADR_TEST_MYSQL_URL not set");
        return;
    };
    let handler = SqlHandler::new();
    let mut vu = vu();

    let select = query(&url, "SELECT name FROM products WHERE stock > ?", &["0"]);
    let resp = handler.execute(&mut vu, &select).await.expect("select");
    assert!(resp.error.is_none(), "select error: {:?}", resp.error);
    assert_eq!(resp.status, 0);
    assert_eq!(resp.extras["backend"], "mysql");
    assert!(resp.extras["rows"].as_u64().unwrap() >= 1);
    assert!(resp.timings.connect_ms > 0.0);

    let reuse = query(&url, "SELECT 1", &[]);
    let resp2 = handler.execute(&mut vu, &reuse).await.expect("select2");
    assert!(resp2.error.is_none());
    assert_eq!(resp2.timings.connect_ms, 0.0, "pool reuse");

    let insert = query(
        &url,
        "INSERT INTO products (name, price, stock) VALUES (?, ?, ?)",
        &["my-item", "2.00", "9"],
    );
    let resp3 = handler.execute(&mut vu, &insert).await.expect("insert");
    assert!(resp3.error.is_none(), "insert error: {:?}", resp3.error);
    assert_eq!(resp3.extras["rows"].as_u64().unwrap(), 1);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn mysql_error_is_reported() {
    let Ok(url) = std::env::var("LOADR_TEST_MYSQL_URL") else {
        eprintln!("skipping: LOADR_TEST_MYSQL_URL not set");
        return;
    };
    let handler = SqlHandler::new();
    let mut vu = vu();

    let bad = query(&url, "SELECT * FROM nope_does_not_exist", &[]);
    let resp = handler.execute(&mut vu, &bad).await.expect("execute");
    assert_ne!(resp.status, 0);
    assert!(resp.error.is_some());
    assert!(resp.failed());
}

// ---------------------------------------------------------------------------
// No DB needed
// ---------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn connection_failure_is_reported() {
    let handler = SqlHandler::new();
    let mut vu = vu();
    // Port 1 is never listening.
    let req = query("postgres://u:p@127.0.0.1:1/db", "SELECT 1", &[]);
    let resp = handler.execute(&mut vu, &req).await.expect("execute");
    assert_ne!(resp.status, 0);
    assert!(resp.error.is_some());
    assert_eq!(resp.protocol_version, "sql");
    assert!(resp.failed());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn unknown_scheme_rejected() {
    let handler = SqlHandler::new();
    let mut vu = vu();
    let req = query("http://127.0.0.1/db", "SELECT 1", &[]);
    let err = handler.execute(&mut vu, &req).await;
    assert!(err.is_err(), "non-sql scheme must error");
}
