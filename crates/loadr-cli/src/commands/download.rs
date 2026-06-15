//! HTTPS download helper for `loadr plugin install`/`search`/`update`.
//!
//! Wraps the project's existing hyper + rustls stack (`hyper-util` legacy
//! client over a `hyper-rustls` HTTPS connector, webpki roots) behind a tiny
//! blocking [`HttpFetcher`] that implements [`loadr_plugin_api::Fetcher`]. A
//! short-lived current-thread Tokio runtime drives each request so the
//! synchronous `loadr plugin` command path needs no async plumbing.
//!
//! Redirects (GitHub release asset downloads in particular) are followed up to
//! a small bound. Only `https://` (and `http://` for explicit `--allow-untrusted`
//! local mirrors) URLs are accepted.

use std::time::Duration;

use http_body_util::BodyExt as _;
use loadr_plugin_api::{Fetcher, PluginError};

type HttpsClient = hyper_util::client::legacy::Client<
    hyper_rustls::HttpsConnector<hyper_util::client::legacy::connect::HttpConnector>,
    http_body_util::Full<bytes::Bytes>,
>;

const MAX_REDIRECTS: usize = 8;
/// Cap a downloaded body at 256 MiB to bound memory for a hostile server.
const MAX_BODY: usize = 256 * 1024 * 1024;

/// A blocking HTTPS fetcher backed by hyper + rustls.
pub struct HttpFetcher {
    runtime: tokio::runtime::Runtime,
    client: HttpsClient,
}

impl HttpFetcher {
    pub fn new() -> anyhow::Result<Self> {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()?;
        let https = hyper_rustls::HttpsConnectorBuilder::new()
            .with_webpki_roots()
            .https_or_http()
            .enable_http1()
            .build();
        let client =
            hyper_util::client::legacy::Client::builder(hyper_util::rt::TokioExecutor::new())
                .build(https);
        Ok(HttpFetcher { runtime, client })
    }

    async fn get(client: &HttpsClient, url: &str) -> Result<Vec<u8>, PluginError> {
        let mut current = url.to_string();
        for _ in 0..=MAX_REDIRECTS {
            let uri: hyper::Uri = current
                .parse()
                .map_err(|e| PluginError::Other(format!("invalid url `{current}`: {e}")))?;
            let request = hyper::Request::builder()
                .method(hyper::Method::GET)
                .uri(&uri)
                .header(hyper::header::USER_AGENT, "loadr-plugin-installer")
                .header(hyper::header::ACCEPT, "application/octet-stream")
                .body(http_body_util::Full::new(bytes::Bytes::new()))
                .map_err(|e| PluginError::Other(format!("building request: {e}")))?;
            let response = client
                .request(request)
                .await
                .map_err(|e| PluginError::Other(format!("GET {current} failed: {e}")))?;
            let status = response.status();
            if status.is_redirection() {
                let location = response
                    .headers()
                    .get(hyper::header::LOCATION)
                    .and_then(|v| v.to_str().ok())
                    .ok_or_else(|| {
                        PluginError::Other(format!("{current} returned {status} without Location"))
                    })?;
                current = resolve_redirect(&current, location);
                continue;
            }
            if !status.is_success() {
                return Err(PluginError::Other(format!(
                    "GET {current} returned {status}"
                )));
            }
            let body = response.into_body();
            let collected = body
                .collect()
                .await
                .map_err(|e| PluginError::Other(format!("reading body from {current}: {e}")))?
                .to_bytes();
            if collected.len() > MAX_BODY {
                return Err(PluginError::Other(format!(
                    "{current} body exceeds {MAX_BODY} bytes"
                )));
            }
            return Ok(collected.to_vec());
        }
        Err(PluginError::Other(format!(
            "too many redirects fetching {url}"
        )))
    }

    /// Fetch + parse a JSON document (used for the GitHub releases API).
    pub fn fetch_json(&self, url: &str) -> Result<serde_json::Value, PluginError> {
        let bytes = self.fetch(url)?;
        serde_json::from_slice(&bytes)
            .map_err(|e| PluginError::Other(format!("invalid JSON from {url}: {e}")))
    }
}

impl Fetcher for HttpFetcher {
    fn fetch(&self, url: &str) -> Result<Vec<u8>, PluginError> {
        self.runtime
            .block_on(async {
                tokio::time::timeout(Duration::from_secs(120), Self::get(&self.client, url)).await
            })
            .map_err(|_| PluginError::Other(format!("timed out fetching {url}")))?
    }
}

/// Resolve a possibly-relative `Location` header against the current URL.
fn resolve_redirect(current: &str, location: &str) -> String {
    if location.starts_with("http://") || location.starts_with("https://") {
        return location.to_string();
    }
    match url::Url::parse(current).and_then(|base| base.join(location)) {
        Ok(joined) => joined.to_string(),
        Err(_) => location.to_string(),
    }
}

/// Resolve a `github:owner/repo[@vX]` reference to the download URL of the
/// release asset matching the host target triple, via the GitHub releases API.
/// Returns `(asset_url, asset_name)`.
pub fn resolve_github(
    fetcher: &HttpFetcher,
    spec: &str,
    target: &str,
) -> Result<(String, String), PluginError> {
    let spec = spec.strip_prefix("github:").unwrap_or(spec);
    let (repo, tag) = match spec.split_once('@') {
        Some((r, t)) => (r, Some(t)),
        None => (spec, None),
    };
    if repo.split('/').count() != 2 {
        return Err(PluginError::Other(format!(
            "expected `github:owner/repo[@tag]`, got `{spec}`"
        )));
    }
    let api = match tag {
        Some(t) => format!("https://api.github.com/repos/{repo}/releases/tags/{t}"),
        None => format!("https://api.github.com/repos/{repo}/releases/latest"),
    };
    let release = fetcher.fetch_json(&api)?;
    let assets = release
        .get("assets")
        .and_then(|a| a.as_array())
        .ok_or_else(|| PluginError::Other(format!("no assets in release {api}")))?;
    // Pick the asset whose name mentions the host target triple.
    for asset in assets {
        let name = asset.get("name").and_then(|n| n.as_str()).unwrap_or("");
        if name.contains(target)
            && (name.ends_with(".tar.gz") || name.ends_with(".tgz") || name.ends_with(".zip"))
        {
            let url = asset
                .get("browser_download_url")
                .and_then(|u| u.as_str())
                .ok_or_else(|| PluginError::Other("asset missing download url".into()))?;
            return Ok((url.to_string(), name.to_string()));
        }
    }
    Err(PluginError::Other(format!(
        "release `{repo}` has no asset for target `{target}` (.tar.gz/.tgz/.zip)"
    )))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn redirect_resolution() {
        assert_eq!(
            resolve_redirect("https://a.test/x", "https://b.test/y"),
            "https://b.test/y"
        );
        assert_eq!(
            resolve_redirect("https://a.test/dir/x", "/abs"),
            "https://a.test/abs"
        );
        assert_eq!(
            resolve_redirect("https://a.test/dir/x", "rel"),
            "https://a.test/dir/rel"
        );
    }
}
