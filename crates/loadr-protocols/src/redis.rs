//! Redis (RESP) protocol handler (`redis://host:port[/db]`).
//!
//! Speaks the REdis Serialization Protocol over a raw TCP connection, with a
//! per-VU connection pool keyed by `host:port` stored in `ctx.extensions`.
//! Connections are reused across requests and transparently re-established
//! when a previous command left the socket in an error state.
//!
//! The command to run is taken (in priority order) from
//! `request.options.plugin` as `{ "command": ["GET", "key"] }`, then from
//! `request.options.socket.payload` (a space-separated command), then from the
//! request `body`. For sockets and Redis alike `status` is `0` on success and
//! non-zero on a RESP error reply.

use std::collections::HashMap;
use std::time::Instant;

use async_trait::async_trait;
use bytes::Bytes;
use loadr_core::error::ProtocolError;
use loadr_core::protocol::{PreparedRequest, ProtocolHandler, ProtocolResponse, Timings};
use loadr_core::vu::VuContext;
use tokio::io::{AsyncReadExt, AsyncWriteExt as _, BufReader};
use tokio::net::TcpStream;
use url::Url;

use crate::net::ms_since;

/// Per-VU pool of live Redis connections keyed by `host:port`.
#[derive(Default)]
struct RedisPool {
    conns: HashMap<String, BufReader<TcpStream>>,
}

/// A parsed RESP reply.
#[derive(Debug, Clone, PartialEq)]
enum RespValue {
    /// `+OK`
    Simple(String),
    /// `-ERR ...`
    Error(String),
    /// `:123`
    Integer(i64),
    /// `$...` bulk string.
    Bulk(Bytes),
    /// `*...` array.
    Array(Vec<RespValue>),
    /// `$-1` / `*-1` null.
    Nil,
}

impl RespValue {
    /// Wire-format type tag used in `extras.reply_type`.
    fn type_name(&self) -> &'static str {
        match self {
            RespValue::Simple(_) => "string",
            RespValue::Error(_) => "error",
            RespValue::Integer(_) => "integer",
            RespValue::Bulk(_) => "bulk",
            RespValue::Array(_) => "array",
            RespValue::Nil => "nil",
        }
    }

    /// JSON rendering for `extras.value`.
    fn to_json(&self) -> serde_json::Value {
        match self {
            RespValue::Simple(s) | RespValue::Error(s) => serde_json::Value::String(s.clone()),
            RespValue::Integer(n) => serde_json::Value::Number((*n).into()),
            RespValue::Bulk(b) => {
                serde_json::Value::String(String::from_utf8_lossy(b).into_owned())
            }
            RespValue::Array(items) => {
                serde_json::Value::Array(items.iter().map(RespValue::to_json).collect())
            }
            RespValue::Nil => serde_json::Value::Null,
        }
    }

    /// Plain-text/body rendering of the reply.
    fn to_body(&self) -> Bytes {
        match self {
            RespValue::Simple(s) | RespValue::Error(s) => Bytes::from(s.clone().into_bytes()),
            RespValue::Integer(n) => Bytes::from(n.to_string().into_bytes()),
            RespValue::Bulk(b) => b.clone(),
            RespValue::Nil => Bytes::new(),
            RespValue::Array(_) => Bytes::from(self.to_json().to_string().into_bytes()),
        }
    }
}

/// Redis protocol handler.
#[derive(Default)]
pub struct RedisHandler;

impl RedisHandler {
    pub fn new() -> Self {
        RedisHandler
    }
}

/// Parse and validate the target URL, returning `(host:port key, db)`.
fn parse_target(raw: &str) -> Result<(String, String, Option<u32>), ProtocolError> {
    let url = Url::parse(raw)
        .map_err(|e| ProtocolError::InvalidRequest(format!("invalid url `{raw}`: {e}")))?;
    if url.scheme() != "redis" {
        return Err(ProtocolError::InvalidRequest(format!(
            "redis handler cannot handle scheme `{}`",
            url.scheme()
        )));
    }
    let host = url
        .host_str()
        .ok_or_else(|| ProtocolError::InvalidRequest(format!("`{raw}` has no host")))?
        .to_string();
    let port = url.port().unwrap_or(6379);
    let key = format!("{host}:{port}");
    let db = match url.path().trim_start_matches('/') {
        "" => None,
        digits => Some(
            digits
                .parse::<u32>()
                .map_err(|_| ProtocolError::InvalidRequest(format!("invalid db `{digits}`")))?,
        ),
    };
    Ok((key, host, db))
}

/// Resolve the command argv from plugin options, socket payload, or body.
fn command_args(request: &PreparedRequest) -> Result<Vec<Vec<u8>>, ProtocolError> {
    if let Some(plugin) = &request.options.plugin {
        if let Some(arr) = plugin.get("command").and_then(serde_json::Value::as_array) {
            let mut args = Vec::with_capacity(arr.len());
            for item in arr {
                match item {
                    serde_json::Value::String(s) => args.push(s.clone().into_bytes()),
                    serde_json::Value::Number(n) => args.push(n.to_string().into_bytes()),
                    other => {
                        return Err(ProtocolError::InvalidRequest(format!(
                            "redis command args must be strings/numbers, got `{other}`"
                        )))
                    }
                }
            }
            if !args.is_empty() {
                return Ok(args);
            }
        }
    }
    let raw = if let Some(socket) = &request.options.socket {
        if !socket.payload.is_empty() {
            socket.payload.clone()
        } else {
            request.body.clone()
        }
    } else {
        request.body.clone()
    };
    let text = String::from_utf8_lossy(&raw);
    let args: Vec<Vec<u8>> = text
        .split_whitespace()
        .map(|s| s.as_bytes().to_vec())
        .collect();
    if args.is_empty() {
        return Err(ProtocolError::InvalidRequest(
            "no redis command provided (set options.plugin.command or a body)".to_string(),
        ));
    }
    Ok(args)
}

/// Encode an argv as a RESP array of bulk strings.
fn encode_command(args: &[Vec<u8>]) -> Vec<u8> {
    let mut out = Vec::new();
    out.extend_from_slice(format!("*{}\r\n", args.len()).as_bytes());
    for arg in args {
        out.extend_from_slice(format!("${}\r\n", arg.len()).as_bytes());
        out.extend_from_slice(arg);
        out.extend_from_slice(b"\r\n");
    }
    out
}

/// Read one CRLF-terminated line (without the trailing CRLF).
async fn read_line<R: AsyncReadExt + Unpin>(reader: &mut R) -> Result<Vec<u8>, String> {
    let mut line = Vec::new();
    loop {
        let byte = reader
            .read_u8()
            .await
            .map_err(|e| format!("read failed: {e}"))?;
        if byte == b'\r' {
            let next = reader
                .read_u8()
                .await
                .map_err(|e| format!("read failed: {e}"))?;
            if next == b'\n' {
                break;
            }
            line.push(b'\r');
            line.push(next);
        } else {
            line.push(byte);
        }
    }
    Ok(line)
}

/// Read a single RESP reply from `reader`.
async fn read_reply<R: AsyncReadExt + Unpin>(reader: &mut R) -> Result<RespValue, String> {
    let prefix = reader
        .read_u8()
        .await
        .map_err(|e| format!("read failed: {e}"))?;
    match prefix {
        b'+' => {
            let line = read_line(reader).await?;
            Ok(RespValue::Simple(
                String::from_utf8_lossy(&line).into_owned(),
            ))
        }
        b'-' => {
            let line = read_line(reader).await?;
            Ok(RespValue::Error(
                String::from_utf8_lossy(&line).into_owned(),
            ))
        }
        b':' => {
            let line = read_line(reader).await?;
            let n = String::from_utf8_lossy(&line)
                .trim()
                .parse::<i64>()
                .map_err(|e| format!("invalid integer reply: {e}"))?;
            Ok(RespValue::Integer(n))
        }
        b'$' => {
            let line = read_line(reader).await?;
            let len = String::from_utf8_lossy(&line)
                .trim()
                .parse::<i64>()
                .map_err(|e| format!("invalid bulk length: {e}"))?;
            if len < 0 {
                return Ok(RespValue::Nil);
            }
            let mut buf = vec![0u8; len as usize];
            reader
                .read_exact(&mut buf)
                .await
                .map_err(|e| format!("read failed: {e}"))?;
            // Consume the trailing CRLF.
            let mut crlf = [0u8; 2];
            reader
                .read_exact(&mut crlf)
                .await
                .map_err(|e| format!("read failed: {e}"))?;
            Ok(RespValue::Bulk(Bytes::from(buf)))
        }
        b'*' => {
            let line = read_line(reader).await?;
            let len = String::from_utf8_lossy(&line)
                .trim()
                .parse::<i64>()
                .map_err(|e| format!("invalid array length: {e}"))?;
            if len < 0 {
                return Ok(RespValue::Nil);
            }
            let mut items = Vec::with_capacity(len as usize);
            for _ in 0..len {
                items.push(Box::pin(read_reply(reader)).await?);
            }
            Ok(RespValue::Array(items))
        }
        other => Err(format!("unexpected RESP prefix byte: {other:#x}")),
    }
}

/// Send `payload` on `conn` and read one reply. Tracks send/wait/receive times.
async fn exchange(
    conn: &mut BufReader<TcpStream>,
    payload: &[u8],
    timings: &mut Timings,
) -> Result<RespValue, String> {
    let send_start = Instant::now();
    conn.get_mut()
        .write_all(payload)
        .await
        .map_err(|e| format!("send failed: {e}"))?;
    conn.get_mut()
        .flush()
        .await
        .map_err(|e| format!("flush failed: {e}"))?;
    timings.sending_ms += ms_since(send_start);

    let wait_start = Instant::now();
    let reply = read_reply(conn).await?;
    timings.waiting_ms += ms_since(wait_start);
    Ok(reply)
}

impl RedisHandler {
    /// Run the command, establishing/reusing a pooled connection. Returns the
    /// reply plus measured timings, or a transport error string.
    async fn run(
        ctx: &mut VuContext,
        key: &str,
        host: &str,
        db: Option<u32>,
        payload: &[u8],
    ) -> Result<(RespValue, Timings, bool), String> {
        let mut timings = Timings::default();
        let mut reconnected = false;

        // Reuse an existing connection if present; on any failure fall through
        // to a fresh connect.
        let has_conn = ctx
            .extensions
            .get_or_insert_with(RedisPool::default)
            .conns
            .contains_key(key);

        if has_conn {
            let pool = ctx.extensions.get_or_insert_with(RedisPool::default);
            if let Some(conn) = pool.conns.get_mut(key) {
                match exchange(conn, payload, &mut timings).await {
                    Ok(reply) => return Ok((reply, timings, false)),
                    Err(e) => {
                        tracing::debug!(error = %e, key, "redis connection errored; reconnecting");
                        pool.conns.remove(key);
                        timings = Timings::default();
                        reconnected = true;
                    }
                }
            }
        }

        // Establish a fresh connection.
        let connect_start = Instant::now();
        let stream = TcpStream::connect(key)
            .await
            .map_err(|e| format!("connection to {key} failed: {e}"))?;
        let _ = stream.set_nodelay(true);
        timings.connect_ms = ms_since(connect_start);
        timings.blocked_ms = timings.connect_ms;
        let mut conn = BufReader::new(stream);

        // Optional SELECT db on a new connection.
        if let Some(db) = db {
            let select = encode_command(&[b"SELECT".to_vec(), db.to_string().into_bytes()]);
            let mut select_timings = Timings::default();
            match exchange(&mut conn, &select, &mut select_timings).await {
                Ok(RespValue::Error(e)) => return Err(format!("SELECT {db} failed: {e}")),
                Ok(_) => {}
                Err(e) => return Err(e),
            }
        }
        let _ = host; // host retained for diagnostics/symmetry with other handlers.

        let reply = exchange(&mut conn, payload, &mut timings).await?;
        let pool = ctx.extensions.get_or_insert_with(RedisPool::default);
        pool.conns.insert(key.to_string(), conn);
        Ok((reply, timings, reconnected))
    }
}

#[async_trait]
impl ProtocolHandler for RedisHandler {
    fn name(&self) -> &str {
        "redis"
    }

    async fn execute(
        &self,
        ctx: &mut VuContext,
        request: &PreparedRequest,
    ) -> Result<ProtocolResponse, ProtocolError> {
        let (key, host, db) = parse_target(&request.url)?;
        let args = command_args(request)?;
        let payload = encode_command(&args);
        let bytes_sent = payload.len() as u64;

        let start = Instant::now();
        let outcome = tokio::time::timeout(
            request.timeout,
            RedisHandler::run(ctx, &key, &host, db, &payload),
        )
        .await;

        match outcome {
            Ok(Ok((reply, mut timings, _reconnected))) => {
                timings.duration_ms =
                    timings.sending_ms + timings.waiting_ms + timings.receiving_ms;
                let (status, status_text, error) = match &reply {
                    RespValue::Error(msg) => (1, msg.clone(), Some(msg.clone())),
                    _ => (0, String::new(), None),
                };
                let body = reply.to_body();
                tracing::debug!(url = %request.url, reply_type = reply.type_name(), "redis command finished");
                Ok(ProtocolResponse {
                    status,
                    status_text,
                    headers: Vec::new(),
                    bytes_received: body.len() as u64,
                    body,
                    timings,
                    bytes_sent,
                    protocol_version: "redis".to_string(),
                    error,
                    url: request.url.clone(),
                    extras: serde_json::json!({
                        "reply_type": reply.type_name(),
                        "value": reply.to_json(),
                    }),
                })
            }
            Ok(Err(message)) => Ok(ProtocolResponse {
                status: 0,
                error: Some(message),
                bytes_sent,
                protocol_version: "redis".to_string(),
                url: request.url.clone(),
                ..ProtocolResponse::default()
            }),
            Err(_) => {
                let elapsed = ms_since(start);
                Ok(ProtocolResponse {
                    status: 0,
                    error: Some(format!("request timed out after {:?}", request.timeout)),
                    bytes_sent,
                    protocol_version: "redis".to_string(),
                    url: request.url.clone(),
                    timings: Timings {
                        duration_ms: elapsed,
                        ..Timings::default()
                    },
                    ..ProtocolResponse::default()
                })
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bytes::Bytes;

    #[test]
    fn parses_redis_urls() {
        let (key, host, db) = parse_target("redis://127.0.0.1:6379").unwrap();
        assert_eq!(key, "127.0.0.1:6379");
        assert_eq!(host, "127.0.0.1");
        assert_eq!(db, None);
        let (key, _, db) = parse_target("redis://localhost/3").unwrap();
        assert_eq!(key, "localhost:6379");
        assert_eq!(db, Some(3));
        assert!(parse_target("http://x:1").is_err());
    }

    #[test]
    fn encodes_resp_command() {
        let cmd = encode_command(&[b"SET".to_vec(), b"k".to_vec(), b"v".to_vec()]);
        assert_eq!(cmd, b"*3\r\n$3\r\nSET\r\n$1\r\nk\r\n$1\r\nv\r\n");
    }

    #[test]
    fn command_args_from_plugin() {
        let mut req = base_request();
        req.options.plugin = Some(serde_json::json!({ "command": ["GET", "mykey"] }));
        let args = command_args(&req).unwrap();
        assert_eq!(args, vec![b"GET".to_vec(), b"mykey".to_vec()]);
    }

    #[test]
    fn command_args_from_body_fallback() {
        let mut req = base_request();
        req.body = Bytes::from_static(b"PING");
        let args = command_args(&req).unwrap();
        assert_eq!(args, vec![b"PING".to_vec()]);
    }

    #[test]
    fn empty_command_rejected() {
        let req = base_request();
        assert!(command_args(&req).is_err());
    }

    #[tokio::test]
    async fn reads_resp_values() {
        assert_eq!(
            read_reply(&mut &b"+OK\r\n"[..]).await.unwrap(),
            RespValue::Simple("OK".to_string())
        );
        assert_eq!(
            read_reply(&mut &b"-ERR bad\r\n"[..]).await.unwrap(),
            RespValue::Error("ERR bad".to_string())
        );
        assert_eq!(
            read_reply(&mut &b":42\r\n"[..]).await.unwrap(),
            RespValue::Integer(42)
        );
        assert_eq!(
            read_reply(&mut &b"$5\r\nhello\r\n"[..]).await.unwrap(),
            RespValue::Bulk(Bytes::from_static(b"hello"))
        );
        assert_eq!(
            read_reply(&mut &b"$-1\r\n"[..]).await.unwrap(),
            RespValue::Nil
        );
        let arr = read_reply(&mut &b"*2\r\n:1\r\n$1\r\nx\r\n"[..])
            .await
            .unwrap();
        assert_eq!(
            arr,
            RespValue::Array(vec![
                RespValue::Integer(1),
                RespValue::Bulk(Bytes::from_static(b"x"))
            ])
        );
    }

    fn base_request() -> PreparedRequest {
        use loadr_core::protocol::RequestOptions;
        use std::time::Duration;
        PreparedRequest {
            name: "r".into(),
            protocol: "redis".into(),
            method: "GET".into(),
            url: "redis://127.0.0.1:6379".into(),
            headers: Vec::new(),
            body: Bytes::new(),
            timeout: Duration::from_secs(5),
            follow_redirects: false,
            max_redirects: 0,
            options: RequestOptions::default(),
        }
    }
}
