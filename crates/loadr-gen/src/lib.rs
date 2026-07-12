//! Generate a runnable loadr scenario from an API contract.
//!
//! Where `loadr convert`/`loadr record` start from *traffic*, `loadr gen` starts
//! from a *contract*: point it at an OpenAPI document and it emits one request
//! per operation, every parameter and body filled from schema-derived example
//! data. It reuses [`loadr_convert::Conversion`] as its return type, exactly as
//! `har.rs` builds a `Conversion` around a `loadr_config::TestPlan`.

pub mod example;
pub mod fuzz;
pub mod graphql;
pub mod grpc;
pub mod openapi;
pub mod postman;

pub use graphql::gen_graphql;
pub use grpc::gen_grpc;
pub use loadr_convert::{Conversion, ConversionWarning};
pub use openapi::gen_openapi;
pub use postman::gen_postman;

use serde_json::Value;

#[derive(Debug, thiserror::Error)]
pub enum GenError {
    #[error("could not parse contract (not JSON or YAML): {0}")]
    Parse(String),
    #[error("openapi: {0}")]
    OpenApi(String),
    #[error("postman: {0}")]
    Postman(String),
    #[error("graphql: {0}")]
    GraphQl(String),
    #[error("grpc: {0}")]
    Grpc(String),
}

/// Options shared by the generators.
#[derive(Debug, Clone, Default)]
pub struct GenOptions {
    /// Index into an OpenAPI `servers[]` array.
    pub server: usize,
    /// Override the derived base URL.
    pub base_url: Option<String>,
    /// operationId/path globs to include (empty = all).
    pub include: Vec<String>,
    /// operationId/path globs to exclude.
    pub exclude: Vec<String>,
    /// Also emit boundary/structural/adversarial fuzz variants with a no-5xx gate.
    pub fuzz: bool,
    /// Adversarial payload kinds to inject when fuzzing (empty = defaults).
    pub fuzz_payloads: Vec<String>,
}

/// Parse a contract that may be JSON or YAML into a `serde_json::Value`.
pub(crate) fn parse_contract(source: &str) -> Result<Value, GenError> {
    if let Ok(v) = serde_json::from_str::<Value>(source) {
        return Ok(v);
    }
    serde_yaml::from_str::<Value>(source).map_err(|e| GenError::Parse(e.to_string()))
}
