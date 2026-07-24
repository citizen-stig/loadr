//! The capturing proxy.
//!
//! Two paths:
//!  * **plain HTTP** — the client sends an absolute-form request
//!    (`GET http://host/path`); we forward it upstream and capture.
//!  * **HTTPS** — the client sends `CONNECT host:443`; we answer `200`, take
//!    over the tunnel, terminate TLS with a per-host leaf cert minted by our
//!    CA (MITM), then serve plain HTTP over it, forwarding each request to the
//!    real host and capturing the plaintext.

use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Instant;

use bytes::Bytes;
use http_body_util::{BodyExt, Full};
use hyper::body::Incoming;
use hyper::server::conn::http1;
use hyper::service::service_fn;
use hyper::{Method, Request, Response, StatusCode};
use hyper_util::client::legacy::Client;
use hyper_util::rt::{TokioExecutor, TokioIo};
use tokio::net::TcpListener;
use tokio_rustls::TlsAcceptor;

use crate::ca::Ca;
use crate::har::{Captured, Recording};

type UpstreamClient = Client<
    hyper_rustls::HttpsConnector<hyper_util::client::legacy::connect::HttpConnector>,
    Full<Bytes>,
>;

/// Runtime configuration for a recording session.
pub struct RecordConfig {
    pub listen: SocketAddr,
    pub recording: Recording,
    pub ca: Arc<Ca>,
}

/// Bind the proxy and serve until the returned future is dropped (the CLI
/// races this against Ctrl-C). Each accepted connection is handled on its own
/// task, so a slow client never blocks the accept loop.
pub async fn run(cfg: RecordConfig) -> anyhow::Result<()> {
    let listener = TcpListener::bind(cfg.listen).await?;
    let client = build_client();

    loop {
        let (stream, _peer) = match listener.accept().await {
            Ok(v) => v,
            Err(e) => {
                tracing::warn!("accept error: {e}");
                continue;
            }
        };
        let ctx = ConnCtx {
            recording: cfg.recording.clone(),
            ca: cfg.ca.clone(),
            client: client.clone(),
        };
        tokio::spawn(async move {
            if let Err(e) = serve_conn(stream, ctx).await {
                tracing::debug!("connection ended: {e}");
            }
        });
    }
}

#[derive(Clone)]
struct ConnCtx {
    recording: Recording,
    ca: Arc<Ca>,
    client: UpstreamClient,
}

fn build_client() -> UpstreamClient {
    let https = hyper_rustls::HttpsConnectorBuilder::new()
        .with_webpki_roots()
        .https_or_http()
        .enable_http1()
        .build();
    Client::builder(TokioExecutor::new()).build(https)
}

async fn serve_conn(stream: tokio::net::TcpStream, ctx: ConnCtx) -> anyhow::Result<()> {
    let io = TokioIo::new(stream);
    let ctx2 = ctx.clone();
    http1::Builder::new()
        .serve_connection(
            io,
            service_fn(move |req| {
                let ctx = ctx2.clone();
                async move { handle(req, ctx).await }
            }),
        )
        .with_upgrades()
        .await?;
    Ok(())
}

async fn handle(
    req: Request<Incoming>,
    ctx: ConnCtx,
) -> Result<Response<Full<Bytes>>, hyper::Error> {
    if req.method() == Method::CONNECT {
        // HTTPS: establish the tunnel, then MITM it on an upgraded task.
        let authority = req
            .uri()
            .authority()
            .map(|a| a.to_string())
            .unwrap_or_default();
        let host = authority.split(':').next().unwrap_or("").to_string();
        tokio::spawn(async move {
            match hyper::upgrade::on(req).await {
                Ok(upgraded) => {
                    if let Err(e) = mitm(upgraded, host, ctx).await {
                        tracing::debug!("mitm ended: {e}");
                    }
                }
                Err(e) => tracing::debug!("upgrade failed: {e}"),
            }
        });
        Ok(Response::new(Full::new(Bytes::new())))
    } else {
        // Plain HTTP forward-proxy request (absolute-form URI).
        forward(req, ctx, "http").await
    }
}

/// MITM a CONNECT tunnel: TLS-accept with a per-host cert, then serve inner
/// HTTP requests, forwarding each to `https://host/...`.
async fn mitm(
    upgraded: hyper::upgrade::Upgraded,
    host: String,
    ctx: ConnCtx,
) -> anyhow::Result<()> {
    let server_cfg = ctx.ca.server_config_for(&host)?;
    let acceptor = TlsAcceptor::from(server_cfg);
    let tls = acceptor.accept(TokioIo::new(upgraded)).await?;

    let host_for_svc = host.clone();
    http1::Builder::new()
        .serve_connection(
            TokioIo::new(tls),
            service_fn(move |req| {
                let ctx = ctx.clone();
                let host = host_for_svc.clone();
                async move { forward_https(req, ctx, host).await }
            }),
        )
        .await?;
    Ok(())
}

/// Forward a plaintext HTTPS request captured inside the MITM tunnel.
async fn forward_https(
    req: Request<Incoming>,
    ctx: ConnCtx,
    host: String,
) -> Result<Response<Full<Bytes>>, hyper::Error> {
    let pq = req
        .uri()
        .path_and_query()
        .map(|p| p.as_str())
        .unwrap_or("/");
    let abs = format!("https://{host}{pq}");
    forward_to(req, ctx, &abs).await
}

/// Forward a plain-HTTP proxy request whose URI is already absolute.
async fn forward(
    req: Request<Incoming>,
    ctx: ConnCtx,
    _scheme: &str,
) -> Result<Response<Full<Bytes>>, hyper::Error> {
    let abs = req.uri().to_string();
    forward_to(req, ctx, &abs).await
}

/// Common forward + capture path.
async fn forward_to(
    req: Request<Incoming>,
    ctx: ConnCtx,
    absolute_url: &str,
) -> Result<Response<Full<Bytes>>, hyper::Error> {
    let method = req.method().clone();
    let version = req.version();
    let req_headers: Vec<(String, String)> = req
        .headers()
        .iter()
        .filter(|(n, _)| !n.as_str().to_ascii_lowercase().starts_with("proxy-"))
        .map(|(n, v)| (n.to_string(), v.to_str().unwrap_or("").to_string()))
        .collect();
    let req_ct = header_value(&req_headers, "content-type");

    // Buffer the request body so we can both forward and record it.
    let (parts, body) = req.into_parts();
    let req_body = body.collect().await?.to_bytes();
    let req_body_text = decode_text(&req_body);

    // Rebuild the upstream request.
    let mut up = Request::builder().method(&method).uri(absolute_url);
    for (n, v) in &req_headers {
        // Host is re-derived by the connector; skip proxy hop headers.
        if n.eq_ignore_ascii_case("proxy-connection") {
            continue;
        }
        up = up.header(n.as_str(), v.as_str());
    }
    let upstream_req = match up.body(Full::new(req_body.clone())) {
        Ok(r) => r,
        Err(_) => return Ok(bad_gateway()),
    };
    let _ = parts; // parts.uri already captured via absolute_url

    let started = Instant::now();
    let resp = match ctx.client.request(upstream_req).await {
        Ok(r) => r,
        Err(e) => {
            tracing::debug!("upstream error for {absolute_url}: {e}");
            return Ok(bad_gateway());
        }
    };
    let wait_ms = started.elapsed().as_secs_f64() * 1000.0;

    let status = resp.status();
    let resp_headers: Vec<(String, String)> = resp
        .headers()
        .iter()
        .map(|(n, v)| (n.to_string(), v.to_str().unwrap_or("").to_string()))
        .collect();
    let resp_ct = header_value(&resp_headers, "content-type");
    let (rparts, rbody) = resp.into_parts();
    let resp_body = rbody.collect().await?.to_bytes();
    let resp_text = decode_text(&resp_body);

    // Record.
    ctx.recording.push(Captured {
        method: method.to_string(),
        url: absolute_url.to_string(),
        http_version: format!("{version:?}"),
        req_headers,
        req_body: req_body_text.map(|t| (req_ct.unwrap_or_else(|| "text/plain".into()), t)),
        status: status.as_u16(),
        status_text: status.canonical_reason().unwrap_or("").to_string(),
        resp_headers,
        resp_mime: resp_ct.unwrap_or_default(),
        resp_body: resp_text,
        wait_ms,
    });

    // Return the captured response to the client unchanged.
    let mut out = Response::builder().status(rparts.status);
    for (n, v) in &rparts.headers {
        out = out.header(n, v);
    }
    Ok(out
        .body(Full::new(resp_body))
        .unwrap_or_else(|_| bad_gateway()))
}

fn bad_gateway() -> Response<Full<Bytes>> {
    Response::builder()
        .status(StatusCode::BAD_GATEWAY)
        .body(Full::new(Bytes::from_static(
            b"loadr record: upstream error",
        )))
        .unwrap()
}

fn header_value(headers: &[(String, String)], name: &str) -> Option<String> {
    headers
        .iter()
        .find(|(n, _)| n.eq_ignore_ascii_case(name))
        .map(|(_, v)| v.clone())
}

/// Keep only bodies that are valid UTF-8 and not obviously binary; the
/// correlator works on text (JSON/form/HTML) anyway.
fn decode_text(body: &Bytes) -> Option<String> {
    if body.is_empty() {
        return None;
    }
    match std::str::from_utf8(body) {
        Ok(s) if !s.contains('\u{0}') => Some(s.to_string()),
        _ => None,
    }
}
