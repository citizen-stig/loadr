//! The generation flow: prompt → model → extract YAML → validate → one repair.

use loadr_config::LoadOptions;

use crate::prompt::{build_repair_message, build_user_message, extract_yaml, SYSTEM_PROMPT};
use crate::provider::{LlmProvider, Msg};
use crate::AiError;

/// Turn a natural-language request into a validated loadr plan YAML.
///
/// One model call; if the result fails `loadr` validation, one repair round is
/// attempted with the diagnostics fed back. Returns the validated YAML.
pub async fn generate_plan(
    provider: &dyn LlmProvider,
    prompt: &str,
    include_schema: bool,
) -> Result<String, AiError> {
    let schema = include_schema.then(loadr_config::json_schema);
    let mut messages = vec![Msg::user(build_user_message(prompt, schema.as_ref()))];

    let resp = provider.chat(SYSTEM_PROMPT, &messages).await?;
    let yaml = extract_yaml(&resp).ok_or(AiError::NoYaml)?;
    if let Err(errs) = validate(&yaml) {
        // One repair round with the diagnostics fed back.
        messages.push(Msg::assistant(resp));
        messages.push(Msg::user(build_repair_message(&yaml, &errs)));
        let resp2 = provider.chat(SYSTEM_PROMPT, &messages).await?;
        let yaml2 = extract_yaml(&resp2).ok_or(AiError::NoYaml)?;
        validate(&yaml2).map_err(|e| AiError::Invalid(e.join("; ")))?;
        return Ok(yaml2);
    }
    Ok(yaml)
}

fn validate(yaml: &str) -> Result<(), Vec<String>> {
    loadr_config::load_str(yaml, &LoadOptions::new())
        .map(|_| ())
        .map_err(|e| vec![e.to_string()])
}

#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;
    use std::sync::Mutex;

    /// A provider that returns canned responses in sequence.
    struct Mock {
        replies: Mutex<Vec<String>>,
    }
    #[async_trait]
    impl LlmProvider for Mock {
        async fn chat(&self, _s: &str, _m: &[Msg]) -> Result<String, AiError> {
            Ok(self.replies.lock().unwrap().remove(0))
        }
    }

    const GOOD: &str = "```yaml\nname: t\nscenarios:\n  s:\n    executor: constant-vus\n    vus: 1\n    duration: 5s\n    flow:\n    - request: { url: https://example.com/ }\n```";
    const BAD: &str = "```yaml\nname: t\nscenarios:\n  s:\n    executor: not-a-real-executor\n```";

    #[tokio::test]
    async fn valid_first_try_returns_yaml() {
        let m = Mock {
            replies: Mutex::new(vec![GOOD.into()]),
        };
        let yaml = generate_plan(&m, "hit example.com", false)
            .await
            .expect("plan");
        assert!(yaml.contains("scenarios:"));
        loadr_config::load_str(&yaml, &LoadOptions::new()).expect("valid");
    }

    #[tokio::test]
    async fn invalid_then_repaired() {
        // First reply is invalid; the repair round returns a good plan.
        let m = Mock {
            replies: Mutex::new(vec![BAD.into(), GOOD.into()]),
        };
        let yaml = generate_plan(&m, "hit example.com", false)
            .await
            .expect("repaired plan");
        loadr_config::load_str(&yaml, &LoadOptions::new()).expect("valid after repair");
    }

    #[tokio::test]
    async fn no_yaml_is_an_error() {
        let m = Mock {
            replies: Mutex::new(vec!["I can't help with that.".into()]),
        };
        assert!(matches!(
            generate_plan(&m, "x", false).await,
            Err(AiError::NoYaml)
        ));
    }
}
