//! GraphQL handler: delegates the HTTP transport to [`HttpHandler`] and
//! post-processes the response body for GraphQL `errors`.
//!
//! The engine has already rendered the `{query, variables, operationName}`
//! JSON envelope into the request body and set method/headers.

use std::sync::Arc;

use async_trait::async_trait;
use loadr_core::error::ProtocolError;
use loadr_core::protocol::{PreparedRequest, ProtocolHandler, ProtocolResponse};
use loadr_core::vu::VuContext;

use crate::http::HttpHandler;

/// GraphQL-over-HTTP handler.
pub struct GraphqlHandler {
    inner: Arc<HttpHandler>,
}

impl GraphqlHandler {
    /// Wrap an existing HTTP handler (shares its connection pool semantics).
    pub fn new(inner: Arc<HttpHandler>) -> Self {
        GraphqlHandler { inner }
    }
}

#[async_trait]
impl ProtocolHandler for GraphqlHandler {
    fn name(&self) -> &str {
        "graphql"
    }

    async fn execute(
        &self,
        ctx: &mut VuContext,
        request: &PreparedRequest,
    ) -> Result<ProtocolResponse, ProtocolError> {
        let mut response = self.inner.execute(ctx, request).await?;
        post_process(&mut response);
        Ok(response)
    }
}

/// Inspect a 200 response body for a GraphQL `errors` array.
///
/// When errors are present, `extras` carries the count and the raw errors.
/// The request is only marked failed (`error` set) when there is no `data`
/// field at all — partial errors alongside `data` are not failures.
pub(crate) fn post_process(response: &mut ProtocolResponse) {
    if response.status != 200 || response.error.is_some() {
        return;
    }
    let Ok(value) = serde_json::from_slice::<serde_json::Value>(&response.body) else {
        return;
    };
    let Some(errors) = value.get("errors").and_then(|e| e.as_array()) else {
        return;
    };
    if errors.is_empty() {
        return;
    }
    let count = errors.len();
    response.extras = serde_json::json!({
        "graphql_errors": count,
        "errors": errors,
    });
    if value.get("data").is_none() {
        response.error = Some(format!("graphql: {count} error(s)"));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bytes::Bytes;

    fn resp(status: i64, body: &str) -> ProtocolResponse {
        ProtocolResponse {
            status,
            body: Bytes::copy_from_slice(body.as_bytes()),
            protocol_version: "HTTP/1.1".to_string(),
            ..ProtocolResponse::default()
        }
    }

    #[test]
    fn errors_without_data_fail() {
        let mut r = resp(200, r#"{"errors":[{"message":"boom"},{"message":"bad"}]}"#);
        post_process(&mut r);
        assert_eq!(r.error.as_deref(), Some("graphql: 2 error(s)"));
        assert_eq!(r.extras["graphql_errors"], 2);
        assert_eq!(r.extras["errors"][0]["message"], "boom");
        assert!(r.failed());
    }

    #[test]
    fn partial_errors_with_data_do_not_fail() {
        let mut r = resp(200, r#"{"data":{"x":1},"errors":[{"message":"partial"}]}"#);
        post_process(&mut r);
        assert!(r.error.is_none());
        assert_eq!(r.extras["graphql_errors"], 1);
        assert!(!r.failed());
    }

    #[test]
    fn clean_response_untouched() {
        let mut r = resp(200, r#"{"data":{"x":1}}"#);
        post_process(&mut r);
        assert!(r.error.is_none());
        assert!(r.extras.is_null());
    }

    #[test]
    fn empty_errors_array_ignored() {
        let mut r = resp(200, r#"{"data":null,"errors":[]}"#);
        post_process(&mut r);
        assert!(r.error.is_none());
        assert!(r.extras.is_null());
    }

    #[test]
    fn non_200_skipped() {
        let mut r = resp(500, r#"{"errors":[{"message":"boom"}]}"#);
        post_process(&mut r);
        assert!(r.error.is_none());
        assert!(r.extras.is_null());
    }

    #[test]
    fn non_json_body_skipped() {
        let mut r = resp(200, "not json");
        post_process(&mut r);
        assert!(r.error.is_none());
    }
}
