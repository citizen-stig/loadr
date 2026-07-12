//! Anthropic Messages API provider (over hyper + rustls, per repo convention).

use async_trait::async_trait;
use bytes::Bytes;
use http_body_util::{BodyExt, Full};
use hyper::Request;
use hyper_util::client::legacy::Client;
use hyper_util::rt::TokioExecutor;
use serde_json::json;

use crate::provider::{LlmProvider, Msg};
use crate::AiError;

/// Default model — a current Claude id (override with `--model`).
pub const DEFAULT_MODEL: &str = "claude-sonnet-5";

pub struct Anthropic {
    api_key: String,
    model: String,
}

impl Anthropic {
    pub const KEY_ENV: &'static str = "ANTHROPIC_API_KEY";

    /// Build from `ANTHROPIC_API_KEY`; errors with setup guidance if unset.
    pub fn from_env(model: Option<String>) -> Result<Self, AiError> {
        let api_key = std::env::var(Self::KEY_ENV).map_err(|_| AiError::NoKey(Self::KEY_ENV))?;
        Ok(Self {
            api_key,
            model: model.unwrap_or_else(|| DEFAULT_MODEL.to_string()),
        })
    }
}

#[async_trait]
impl LlmProvider for Anthropic {
    async fn chat(&self, system: &str, messages: &[Msg]) -> Result<String, AiError> {
        let _ = rustls::crypto::ring::default_provider().install_default();
        let https = hyper_rustls::HttpsConnectorBuilder::new()
            .with_webpki_roots()
            .https_only()
            .enable_http1()
            .build();
        let client: Client<_, Full<Bytes>> = Client::builder(TokioExecutor::new()).build(https);

        let body = json!({
            "model": self.model,
            "max_tokens": 4096,
            "system": system,
            "messages": messages
                .iter()
                .map(|m| json!({ "role": m.role, "content": m.content }))
                .collect::<Vec<_>>(),
        });

        let req = Request::builder()
            .method("POST")
            .uri("https://api.anthropic.com/v1/messages")
            .header("content-type", "application/json")
            .header("x-api-key", &self.api_key)
            .header("anthropic-version", "2023-06-01")
            .body(Full::new(Bytes::from(body.to_string())))
            .map_err(|e| AiError::Provider(e.to_string()))?;

        let resp = client
            .request(req)
            .await
            .map_err(|e| AiError::Provider(e.to_string()))?;
        let status = resp.status();
        let bytes = resp
            .into_body()
            .collect()
            .await
            .map_err(|e| AiError::Provider(e.to_string()))?
            .to_bytes();

        if !status.is_success() {
            return Err(AiError::Provider(format!(
                "Anthropic API {status}: {}",
                String::from_utf8_lossy(&bytes)
            )));
        }

        let v: serde_json::Value =
            serde_json::from_slice(&bytes).map_err(|e| AiError::Provider(e.to_string()))?;
        let text = v
            .get("content")
            .and_then(|c| c.as_array())
            .map(|blocks| {
                blocks
                    .iter()
                    .filter(|b| b.get("type").and_then(|t| t.as_str()) == Some("text"))
                    .filter_map(|b| b.get("text").and_then(|t| t.as_str()))
                    .collect::<Vec<_>>()
                    .join("")
            })
            .unwrap_or_default();
        Ok(text)
    }
}
