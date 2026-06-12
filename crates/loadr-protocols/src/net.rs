//! Small networking helpers shared by the protocol handlers.

use std::net::SocketAddr;
use std::time::Instant;

use loadr_core::protocol::Timings;
use tokio::io::{AsyncRead, AsyncWrite};

/// Milliseconds elapsed since `start` as `f64`.
pub(crate) fn ms_since(start: Instant) -> f64 {
    start.elapsed().as_secs_f64() * 1000.0
}

/// Object-safe async byte stream (TCP or TLS) used behind `Box<dyn _>`.
pub(crate) trait IoStream: AsyncRead + AsyncWrite + Send + Unpin {}
impl<T: AsyncRead + AsyncWrite + Send + Unpin> IoStream for T {}

/// Resolve `host:port`, recording DNS time into `timings.dns_ms` when an
/// actual lookup happens (IP literals resolve instantly with `dns_ms = 0`).
pub(crate) async fn resolve(
    host: &url::Host<&str>,
    port: u16,
    timings: &mut Timings,
) -> Result<SocketAddr, String> {
    match host {
        url::Host::Ipv4(ip) => Ok(SocketAddr::new((*ip).into(), port)),
        url::Host::Ipv6(ip) => Ok(SocketAddr::new((*ip).into(), port)),
        url::Host::Domain(domain) => {
            let start = Instant::now();
            let mut addrs = tokio::net::lookup_host((domain.to_string(), port))
                .await
                .map_err(|e| format!("dns lookup for `{domain}` failed: {e}"))?;
            timings.dns_ms = ms_since(start);
            addrs
                .next()
                .ok_or_else(|| format!("dns lookup for `{domain}` returned no addresses"))
        }
    }
}

/// Extract `(host, port)` from a URL, failing when either is missing.
pub(crate) fn host_port(url: &url::Url) -> Result<(String, u16), String> {
    let host = url
        .host_str()
        .ok_or_else(|| format!("url `{url}` has no host"))?
        .to_string();
    let port = url
        .port_or_known_default()
        .ok_or_else(|| format!("url `{url}` has no port"))?;
    Ok((host, port))
}
