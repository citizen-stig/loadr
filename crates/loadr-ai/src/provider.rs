//! The provider seam: any LLM that can turn a system prompt + messages into text.

use async_trait::async_trait;

/// One chat message.
#[derive(Debug, Clone)]
pub struct Msg {
    pub role: String,
    pub content: String,
}

impl Msg {
    pub fn user(c: impl Into<String>) -> Self {
        Self {
            role: "user".into(),
            content: c.into(),
        }
    }
    pub fn assistant(c: impl Into<String>) -> Self {
        Self {
            role: "assistant".into(),
            content: c.into(),
        }
    }
}

/// A chat completion backend (Anthropic, Bedrock, an OpenAI-compatible endpoint,
/// or a test mock). The one seam the generation flow depends on.
#[async_trait]
pub trait LlmProvider: Send + Sync {
    async fn chat(&self, system: &str, messages: &[Msg]) -> Result<String, crate::AiError>;
}
