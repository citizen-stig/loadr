use std::collections::HashMap;
use std::net::SocketAddr;

use tokio::io::{AsyncReadExt as _, AsyncWriteExt as _, BufReader};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::oneshot;

use crate::TestServerError;

/// Minimal in-process Redis (RESP) test server.
///
/// Speaks just enough RESP for integration tests:
/// - `PING` → `+PONG`
/// - `SET key val` → `+OK` (stores `val` under `key`)
/// - `GET key` → bulk string of the last `SET` value, or nil
/// - `SELECT n` → `+OK`
/// - anything else → `-ERR unknown command`
///
/// Each connection has its own in-memory keyspace. Shuts down on drop.
pub struct RedisTestServer {
    /// Bound address (always `127.0.0.1` with an ephemeral port).
    pub addr: SocketAddr,
    shutdown: Option<oneshot::Sender<()>>,
}

impl RedisTestServer {
    /// Spawns the server on `127.0.0.1` with an ephemeral port.
    pub async fn spawn() -> Result<Self, TestServerError> {
        let listener = TcpListener::bind("127.0.0.1:0").await?;
        let addr = listener.local_addr()?;
        let (tx, mut rx) = oneshot::channel::<()>();
        tokio::spawn(async move {
            loop {
                tokio::select! {
                    _ = &mut rx => break,
                    accepted = listener.accept() => {
                        match accepted {
                            Ok((stream, peer)) => {
                                tokio::spawn(handle_connection(stream, peer));
                            }
                            Err(e) => tracing::warn!(error = %e, "redis test server accept failed"),
                        }
                    }
                }
            }
            tracing::debug!("redis test server stopped");
        });
        tracing::debug!(%addr, "redis test server listening");
        Ok(Self {
            addr,
            shutdown: Some(tx),
        })
    }

    /// Redis URL, e.g. `redis://127.0.0.1:54321`.
    pub fn url(&self) -> String {
        format!("redis://{}", self.addr)
    }

    /// Stops the server. Also happens automatically on drop.
    pub fn shutdown(&mut self) {
        if let Some(tx) = self.shutdown.take() {
            let _ = tx.send(());
        }
    }
}

impl Drop for RedisTestServer {
    fn drop(&mut self) {
        self.shutdown();
    }
}

async fn handle_connection(stream: TcpStream, peer: SocketAddr) {
    let mut reader = BufReader::new(stream);
    let mut store: HashMap<String, Vec<u8>> = HashMap::new();
    loop {
        let args = match read_command(&mut reader).await {
            Ok(Some(args)) => args,
            Ok(None) => break, // peer closed
            Err(e) => {
                tracing::debug!(error = %e, %peer, "redis read error");
                break;
            }
        };
        let reply = handle_command(&args, &mut store);
        if reader.get_mut().write_all(&reply).await.is_err() {
            break;
        }
    }
}

/// Read one RESP array-of-bulk-strings command. Returns `None` on EOF.
async fn read_command(
    reader: &mut BufReader<TcpStream>,
) -> Result<Option<Vec<Vec<u8>>>, std::io::Error> {
    let prefix = match reader.read_u8().await {
        Ok(b) => b,
        Err(e) if e.kind() == std::io::ErrorKind::UnexpectedEof => return Ok(None),
        Err(e) => return Err(e),
    };
    if prefix != b'*' {
        // Inline command (rare); read the rest of the line and ignore.
        let _ = read_line(reader).await?;
        return Ok(Some(Vec::new()));
    }
    let count: i64 = parse_int(&read_line(reader).await?);
    if count < 0 {
        return Ok(Some(Vec::new()));
    }
    let mut args = Vec::with_capacity(count as usize);
    for _ in 0..count {
        let bulk_prefix = reader.read_u8().await?;
        if bulk_prefix != b'$' {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "expected bulk string",
            ));
        }
        let len: i64 = parse_int(&read_line(reader).await?);
        if len < 0 {
            args.push(Vec::new());
            continue;
        }
        let mut buf = vec![0u8; len as usize];
        reader.read_exact(&mut buf).await?;
        let mut crlf = [0u8; 2];
        reader.read_exact(&mut crlf).await?;
        args.push(buf);
    }
    Ok(Some(args))
}

fn handle_command(args: &[Vec<u8>], store: &mut HashMap<String, Vec<u8>>) -> Vec<u8> {
    if args.is_empty() {
        return b"-ERR empty command\r\n".to_vec();
    }
    let command = String::from_utf8_lossy(&args[0]).to_ascii_uppercase();
    match command.as_str() {
        "PING" => b"+PONG\r\n".to_vec(),
        "SELECT" => b"+OK\r\n".to_vec(),
        "SET" if args.len() >= 3 => {
            let key = String::from_utf8_lossy(&args[1]).into_owned();
            store.insert(key, args[2].clone());
            b"+OK\r\n".to_vec()
        }
        "GET" if args.len() >= 2 => {
            let key = String::from_utf8_lossy(&args[1]).into_owned();
            match store.get(&key) {
                Some(value) => {
                    let mut out = format!("${}\r\n", value.len()).into_bytes();
                    out.extend_from_slice(value);
                    out.extend_from_slice(b"\r\n");
                    out
                }
                None => b"$-1\r\n".to_vec(),
            }
        }
        other => format!("-ERR unknown command '{other}'\r\n").into_bytes(),
    }
}

async fn read_line(reader: &mut BufReader<TcpStream>) -> Result<Vec<u8>, std::io::Error> {
    let mut line = Vec::new();
    loop {
        let byte = reader.read_u8().await?;
        if byte == b'\r' {
            let next = reader.read_u8().await?;
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

fn parse_int(bytes: &[u8]) -> i64 {
    String::from_utf8_lossy(bytes).trim().parse().unwrap_or(-1)
}
