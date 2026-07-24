//! AI copilot for loadr: natural language → a validated scenario.
//!
//! Provider-agnostic (an [`LlmProvider`] seam) so the flow is unit-testable with
//! a mock and pluggable across Anthropic / Bedrock / OpenAI-compatible backends.
//! The generation flow mirrors the desktop app: one model call, extract the
//! YAML, validate it with `loadr`, and one repair round on failure.

pub mod anthropic;
pub mod generate;
pub mod prompt;
pub mod provider;

pub use anthropic::Anthropic;
pub use generate::generate_plan;
pub use provider::{LlmProvider, Msg};

#[derive(Debug, thiserror::Error)]
pub enum AiError {
    #[error("no API key — set {0} (e.g. `export {0}=sk-...`)")]
    NoKey(&'static str),
    #[error("the model did not return a YAML plan — try rephrasing the request")]
    NoYaml,
    #[error("the generated plan is still invalid after one repair round: {0}")]
    Invalid(String),
    #[error("provider error: {0}")]
    Provider(String),
}
