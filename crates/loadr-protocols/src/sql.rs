//! SQL protocol handler for PostgreSQL and MySQL (`postgres://` / `mysql://`).
//!
//! Each configured query is executed as one "request". The handler records
//! query latency (as `waiting_ms`/`duration_ms`), the number of rows returned
//! (SELECT) or affected (INSERT/UPDATE/DELETE) in `extras.rows`, and any
//! database error in `ProtocolResponse::error` so a request is marked failed.
//!
//! Connections honor the engine's per-VU model: each VU keeps a small `sqlx`
//! pool (max 1 live connection) per database URL in `ctx.extensions`, reused
//! across requests and transparently re-established by `sqlx` on failure.
//!
//! The query and its positional bind parameters are taken from
//! `request.options.plugin` as `{ "query": "SELECT ...", "params": ["a", 1] }`
//! (populated from the YAML `sql:` block), falling back to the request `body`
//! as the query text when no plugin options are present.

use std::collections::HashMap;
use std::time::Instant;

use async_trait::async_trait;
use bytes::Bytes;
use loadr_core::error::ProtocolError;
use loadr_core::protocol::{PreparedRequest, ProtocolHandler, ProtocolResponse, Timings};
use loadr_core::vu::VuContext;
use sqlx::mysql::MySqlPoolOptions;
use sqlx::postgres::PgPoolOptions;
use sqlx::{MySqlPool, PgPool};
use url::Url;

use crate::net::ms_since;

/// Which SQL backend a URL targets.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Backend {
    Postgres,
    MySql,
}

impl Backend {
    fn from_scheme(scheme: &str) -> Option<Backend> {
        match scheme {
            "postgres" | "postgresql" => Some(Backend::Postgres),
            "mysql" => Some(Backend::MySql),
            _ => None,
        }
    }

    fn name(self) -> &'static str {
        match self {
            Backend::Postgres => "postgres",
            Backend::MySql => "mysql",
        }
    }
}

/// Per-VU cache of `sqlx` pools keyed by the connection URL.
#[derive(Default)]
struct SqlPools {
    postgres: HashMap<String, PgPool>,
    mysql: HashMap<String, MySqlPool>,
}

/// The resolved query for one request.
struct SqlQuery {
    query: String,
    params: Vec<String>,
}

impl SqlQuery {
    /// Resolve the query + params from plugin options, falling back to `body`.
    fn from_request(request: &PreparedRequest) -> Result<SqlQuery, ProtocolError> {
        if let Some(plugin) = &request.options.plugin {
            if let Some(query) = plugin.get("query").and_then(serde_json::Value::as_str) {
                let params = plugin
                    .get("params")
                    .and_then(serde_json::Value::as_array)
                    .map(|arr| {
                        arr.iter()
                            .map(|v| match v {
                                serde_json::Value::String(s) => s.clone(),
                                serde_json::Value::Null => String::new(),
                                other => other.to_string(),
                            })
                            .collect()
                    })
                    .unwrap_or_default();
                if query.trim().is_empty() {
                    return Err(ProtocolError::InvalidRequest(
                        "sql query is empty".to_string(),
                    ));
                }
                return Ok(SqlQuery {
                    query: query.to_string(),
                    params,
                });
            }
        }
        let query = String::from_utf8_lossy(&request.body).trim().to_string();
        if query.is_empty() {
            return Err(ProtocolError::InvalidRequest(
                "no sql query provided (set the `sql.query` option or a request body)".to_string(),
            ));
        }
        Ok(SqlQuery {
            query,
            params: Vec::new(),
        })
    }
}

/// Validate the URL scheme and return the backend it targets.
fn parse_backend(raw: &str) -> Result<Backend, ProtocolError> {
    let url = Url::parse(raw)
        .map_err(|e| ProtocolError::InvalidRequest(format!("invalid url `{raw}`: {e}")))?;
    Backend::from_scheme(url.scheme()).ok_or_else(|| {
        ProtocolError::InvalidRequest(format!(
            "sql handler cannot handle scheme `{}`",
            url.scheme()
        ))
    })
}

/// True when a statement is expected to return a result set (SELECT / WITH /
/// SHOW / VALUES / EXPLAIN / postgres `RETURNING`). Used to pick between the
/// row-returning and the affected-rows execution paths.
fn returns_rows(query: &str) -> bool {
    let head: String = query
        .trim_start()
        .chars()
        .take_while(|c| c.is_alphabetic())
        .flat_map(char::to_lowercase)
        .collect();
    matches!(
        head.as_str(),
        "select" | "with" | "show" | "values" | "explain" | "table" | "describe" | "desc"
    ) || query.to_lowercase().contains(" returning ")
}

/// Bind a single string parameter with an inferred type so that comparisons
/// against numeric columns work (PostgreSQL in particular will not implicitly
/// cast `text` to `numeric`). Integers bind as `i64`, decimals as `f64`,
/// everything else as text.
fn bind_pg<'q>(
    query: sqlx::query::Query<'q, sqlx::Postgres, sqlx::postgres::PgArguments>,
    p: &'q str,
) -> sqlx::query::Query<'q, sqlx::Postgres, sqlx::postgres::PgArguments> {
    if let Ok(i) = p.parse::<i64>() {
        query.bind(i)
    } else if let Ok(f) = p.parse::<f64>() {
        query.bind(f)
    } else {
        query.bind(p)
    }
}

fn bind_my<'q>(
    query: sqlx::query::Query<'q, sqlx::MySql, sqlx::mysql::MySqlArguments>,
    p: &'q str,
) -> sqlx::query::Query<'q, sqlx::MySql, sqlx::mysql::MySqlArguments> {
    if let Ok(i) = p.parse::<i64>() {
        query.bind(i)
    } else if let Ok(f) = p.parse::<f64>() {
        query.bind(f)
    } else {
        query.bind(p)
    }
}

/// Run the query against postgres. Returns the number of rows returned by a
/// row-producing statement, or rows affected by a DML statement.
async fn run_postgres(
    pool: &PgPool,
    q: &SqlQuery,
    timings: &mut Timings,
) -> Result<u64, sqlx::Error> {
    let exec_start = Instant::now();
    let mut query = sqlx::query(&q.query);
    for p in &q.params {
        query = bind_pg(query, p);
    }
    let count = if returns_rows(&q.query) {
        query.fetch_all(pool).await?.len() as u64
    } else {
        query.execute(pool).await?.rows_affected()
    };
    timings.waiting_ms = ms_since(exec_start);
    Ok(count)
}

/// Run the query against mysql. Returns rows returned (SELECT) or affected (DML).
async fn run_mysql(
    pool: &MySqlPool,
    q: &SqlQuery,
    timings: &mut Timings,
) -> Result<u64, sqlx::Error> {
    let exec_start = Instant::now();
    let mut query = sqlx::query(&q.query);
    for p in &q.params {
        query = bind_my(query, p);
    }
    let count = if returns_rows(&q.query) {
        query.fetch_all(pool).await?.len() as u64
    } else {
        query.execute(pool).await?.rows_affected()
    };
    timings.waiting_ms = ms_since(exec_start);
    Ok(count)
}

/// SQL protocol handler.
#[derive(Default)]
pub struct SqlHandler;

impl SqlHandler {
    pub fn new() -> Self {
        SqlHandler
    }

    /// Get-or-create the per-VU postgres pool for `url`. The bool is `true` when
    /// a fresh pool (and connection) had to be established.
    async fn pg_pool(ctx: &mut VuContext, url: &str) -> Result<(PgPool, bool), sqlx::Error> {
        if let Some(p) = ctx
            .extensions
            .get_or_insert_with(SqlPools::default)
            .postgres
            .get(url)
        {
            return Ok((p.clone(), false));
        }
        let pool = PgPoolOptions::new().max_connections(1).connect(url).await?;
        ctx.extensions
            .get_or_insert_with(SqlPools::default)
            .postgres
            .insert(url.to_string(), pool.clone());
        Ok((pool, true))
    }

    /// Get-or-create the per-VU mysql pool for `url`. The bool is `true` when a
    /// fresh pool (and connection) had to be established.
    async fn my_pool(ctx: &mut VuContext, url: &str) -> Result<(MySqlPool, bool), sqlx::Error> {
        if let Some(p) = ctx
            .extensions
            .get_or_insert_with(SqlPools::default)
            .mysql
            .get(url)
        {
            return Ok((p.clone(), false));
        }
        let pool = MySqlPoolOptions::new()
            .max_connections(1)
            .connect(url)
            .await?;
        ctx.extensions
            .get_or_insert_with(SqlPools::default)
            .mysql
            .insert(url.to_string(), pool.clone());
        Ok((pool, true))
    }
}

#[async_trait]
impl ProtocolHandler for SqlHandler {
    fn name(&self) -> &str {
        "sql"
    }

    async fn execute(
        &self,
        ctx: &mut VuContext,
        request: &PreparedRequest,
    ) -> Result<ProtocolResponse, ProtocolError> {
        let backend = parse_backend(&request.url)?;
        let q = SqlQuery::from_request(request)?;
        let bytes_sent = q.query.len() as u64;

        let start = Instant::now();
        let mut timings = Timings::default();

        let result: Result<u64, String> = tokio::time::timeout(request.timeout, async {
            match backend {
                Backend::Postgres => {
                    let connect_start = Instant::now();
                    let (pool, fresh) = SqlHandler::pg_pool(ctx, &request.url)
                        .await
                        .map_err(|e| format!("connect failed: {e}"))?;
                    if fresh {
                        timings.connect_ms = ms_since(connect_start);
                        timings.blocked_ms = timings.connect_ms;
                    }
                    run_postgres(&pool, &q, &mut timings)
                        .await
                        .map_err(|e| e.to_string())
                }
                Backend::MySql => {
                    let connect_start = Instant::now();
                    let (pool, fresh) = SqlHandler::my_pool(ctx, &request.url)
                        .await
                        .map_err(|e| format!("connect failed: {e}"))?;
                    if fresh {
                        timings.connect_ms = ms_since(connect_start);
                        timings.blocked_ms = timings.connect_ms;
                    }
                    run_mysql(&pool, &q, &mut timings)
                        .await
                        .map_err(|e| e.to_string())
                }
            }
        })
        .await
        .unwrap_or_else(|_| Err(format!("query timed out after {:?}", request.timeout)));

        timings.duration_ms = ms_since(start);

        match result {
            Ok(rows) => {
                let body = Bytes::from(rows.to_string().into_bytes());
                tracing::debug!(url = %request.url, backend = backend.name(), rows, "sql query finished");
                Ok(ProtocolResponse {
                    status: 0,
                    status_text: String::new(),
                    headers: Vec::new(),
                    bytes_received: body.len() as u64,
                    body,
                    timings,
                    bytes_sent,
                    protocol_version: "sql".to_string(),
                    error: None,
                    url: request.url.clone(),
                    extras: serde_json::json!({
                        "backend": backend.name(),
                        "rows": rows,
                    }),
                })
            }
            Err(message) => Ok(ProtocolResponse {
                status: 1,
                error: Some({
                    tracing::debug!(url = %request.url, backend = backend.name(), error = %message, "sql query failed");
                    message
                }),
                bytes_sent,
                timings,
                protocol_version: "sql".to_string(),
                url: request.url.clone(),
                extras: serde_json::json!({ "backend": backend.name() }),
                ..ProtocolResponse::default()
            }),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use loadr_core::protocol::RequestOptions;
    use std::time::Duration;

    fn base_request(url: &str) -> PreparedRequest {
        PreparedRequest {
            name: "q".into(),
            protocol: "sql".into(),
            method: "GET".into(),
            url: url.into(),
            headers: Vec::new(),
            body: Bytes::new(),
            timeout: Duration::from_secs(5),
            follow_redirects: false,
            max_redirects: 0,
            options: RequestOptions::default(),
        }
    }

    #[test]
    fn parses_backends() {
        assert_eq!(
            parse_backend("postgres://u:p@h:5432/db").unwrap(),
            Backend::Postgres
        );
        assert_eq!(
            parse_backend("postgresql://h/db").unwrap(),
            Backend::Postgres
        );
        assert_eq!(parse_backend("mysql://h:3306/db").unwrap(), Backend::MySql);
        assert!(parse_backend("http://h/db").is_err());
        assert!(parse_backend("not a url").is_err());
    }

    #[test]
    fn query_from_plugin_options() {
        let mut req = base_request("postgres://h/db");
        req.options.plugin = Some(serde_json::json!({
            "query": "SELECT * FROM t WHERE id = $1",
            "params": ["42", 7, null],
        }));
        let q = SqlQuery::from_request(&req).unwrap();
        assert_eq!(q.query, "SELECT * FROM t WHERE id = $1");
        assert_eq!(
            q.params,
            vec!["42".to_string(), "7".to_string(), String::new()]
        );
    }

    #[test]
    fn query_from_body_fallback() {
        let mut req = base_request("mysql://h/db");
        req.body = Bytes::from_static(b"  SELECT 1  ");
        let q = SqlQuery::from_request(&req).unwrap();
        assert_eq!(q.query, "SELECT 1");
        assert!(q.params.is_empty());
    }

    #[test]
    fn empty_query_rejected() {
        let req = base_request("postgres://h/db");
        assert!(SqlQuery::from_request(&req).is_err());

        let mut req = base_request("postgres://h/db");
        req.options.plugin = Some(serde_json::json!({ "query": "   " }));
        assert!(SqlQuery::from_request(&req).is_err());
    }

    #[test]
    fn backend_name_roundtrip() {
        assert_eq!(Backend::Postgres.name(), "postgres");
        assert_eq!(Backend::MySql.name(), "mysql");
        assert_eq!(Backend::from_scheme("mysql"), Some(Backend::MySql));
        assert_eq!(Backend::from_scheme("oracle"), None);
    }
}
