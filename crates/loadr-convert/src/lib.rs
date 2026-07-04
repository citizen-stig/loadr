//! # loadr-convert
//!
//! Converters that turn existing load-test assets into loadr YAML test plans:
//!
//! - [`convert_jmx`] — JMeter 5.x `.jmx` test plans (the common 90% of elements).
//! - [`convert_k6`] — k6 JavaScript scripts (options, default/scenario functions,
//!   checks, groups, sleeps) using lightweight source analysis, no JS engine.
//! - [`convert_har`] — HAR (HTTP Archive) recordings, with heuristic
//!   auto-correlation of dynamic values (tokens/ids) reused across requests.
//! - [`convert_accesslog`] — nginx/apache COMBINED access logs (`.log`),
//!   reconstructing the observed traffic shape (arrival rate, duration,
//!   weighted endpoint mix).
//!
//! All converters are best-effort: anything they cannot represent faithfully
//! becomes a [`ConversionWarning`] instead of a hard error, and the resulting
//! [`loadr_config::TestPlan`] is designed to pass `loadr_config::validate`
//! without errors.

mod accesslog;
mod har;
mod jmx;
mod k6;

use thiserror::Error;

pub use accesslog::convert_accesslog;
pub use har::convert_har;
pub use jmx::convert_jmx;
pub use k6::convert_k6;

/// The result of a conversion: a loadr test plan plus best-effort warnings.
#[derive(Debug)]
pub struct Conversion {
    /// The converted plan; serialize with `serde_yaml::to_string`.
    pub plan: loadr_config::TestPlan,
    /// Elements/constructs that were skipped or approximated.
    pub warnings: Vec<ConversionWarning>,
}

/// A non-fatal conversion note about one source element.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConversionWarning {
    /// The source element (JMX tag/test name, or k6 construct).
    pub element: String,
    /// What happened and what to review.
    pub message: String,
}

impl std::fmt::Display for ConversionWarning {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {}", self.element, self.message)
    }
}

/// Errors that abort a conversion entirely.
#[derive(Debug, Error)]
pub enum ConvertError {
    /// The input is not well-formed XML.
    #[error("XML parse error: {0}")]
    Xml(String),
    /// The XML parsed but is not a JMeter test plan.
    #[error("not a JMeter test plan: {0}")]
    NotJmx(String),
    /// The k6 script could not be analyzed at all.
    #[error("JavaScript parse error: {0}")]
    Js(String),
    /// `export const options = {...}` could not be interpreted.
    #[error("invalid k6 options: {0}")]
    Options(String),
    /// The input is not a usable HAR (HTTP Archive) document.
    #[error("invalid HAR: {0}")]
    Har(String),
    /// The access log contained no parseable request lines.
    #[error("invalid access log: {0}")]
    AccessLog(String),
}
