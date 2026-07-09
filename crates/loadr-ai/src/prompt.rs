//! The system prompt, message builders, and YAML extraction — ported from the
//! desktop AI flow (`desktop/src/shared/ai.ts`). Pure and headless-testable.

use serde_json::Value;

pub const SYSTEM_PROMPT: &str = r#"You are an expert performance engineer who writes load-test plans for "loadr".
Output a SINGLE loadr test plan as YAML, inside one ```yaml fenced block, and NOTHING else — no prose, no explanation.
loadr plan shape:
- top level: name, optional description, optional defaults.http.base_url, optional variables, optional data (CSV/JSON for data-driven), optional thresholds, and scenarios (a map).
- each scenario has an executor and a flow (ordered list of steps):
  - executors: constant-vus {vus,duration}, ramping-vus {stages:[{duration,target}]}, constant-arrival-rate {rate,duration,pre_allocated_vus}, per-vu-iterations {vus,iterations}, shared-iterations {vus,iterations}.
  - flow steps are single-key maps: request, think_time, js, group, repeat, while, if, foreach, switch, during, retry, parallel, rendezvous.
  - request: { method, url, name?, headers?, params?, body?, timeout?, assert?:[conditions], checks?:[conditions], extract?:[extractors] }.
    conditions: {type: status, equals|one_of|matches}, {type: jsonpath, expression, equals?|exists?}, {type: body_contains, value}, {type: duration, max}.
    extractors: {type: jsonpath|regex|header, name, expression|header}.
  - think_time: { type: constant|uniform|gaussian, duration | min,max | mean,std_dev }.
- thresholds gate pass/fail, e.g. { http_req_duration: ["p(95)<500"], http_req_failed: ["rate<0.01"] }.
- templates: ${var} interpolation; ${js: expr} for inline JS; session.vars for extracted values.
Rules:
- Prefer a closed model (constant-vus) with a realistic duration (e.g. 30s) and modest VUs unless the user asks otherwise.
- Use relative URLs with defaults.http.base_url when a base URL is known; otherwise full URLs.
- Add sensible assertions (status 2xx) and thresholds (p95 latency, error rate) by default.
- Only use the step kinds and fields listed above. Produce a plan that passes `loadr validate`."#;

/// The user message for a fresh generation.
pub fn build_user_message(prompt: &str, schema: Option<&Value>) -> String {
    let mut parts: Vec<String> = Vec::new();
    let p = prompt.trim();
    parts.push(if p.is_empty() {
        "Create a sensible HTTP load test.".to_string()
    } else {
        p.to_string()
    });
    if let Some(schema) = schema {
        parts.push(format!(
            "Authoritative loadr JSON Schema (the plan MUST validate against it):\n{}",
            serde_json::to_string(schema).unwrap_or_default()
        ));
    }
    parts.push("Return ONE ```yaml fenced block containing only the plan.".to_string());
    parts.join("\n\n")
}

/// The follow-up asking the model to fix validation errors.
pub fn build_repair_message(yaml: &str, errors: &[String]) -> String {
    let errs = errors
        .iter()
        .map(|e| format!("- {e}"))
        .collect::<Vec<_>>()
        .join("\n");
    format!(
        "That plan failed `loadr validate` with these errors:\n\n{errs}\n\nHere is the plan you produced:\n\n```yaml\n{yaml}\n```\n\nReturn a corrected plan as ONE ```yaml fenced block, nothing else."
    )
}

/// Pull a YAML plan out of a model response (a fenced block, else a bare plan).
pub fn extract_yaml(text: &str) -> Option<String> {
    // Fenced ```yaml … ``` (or ``` … ```).
    if let Some(start) = text.find("```") {
        let after = &text[start + 3..];
        // skip an optional language tag on the same line
        let body_start = after.find('\n').map(|i| i + 1).unwrap_or(0);
        let body = &after[body_start..];
        if let Some(end) = body.find("```") {
            let inner = body[..end].trim();
            if !inner.is_empty() {
                return Some(inner.to_string());
            }
        }
    }
    // Bare plan: looks like YAML with a `scenarios:` key.
    let t = text.trim();
    let starts_ok = ["name:", "scenarios:", "description:", "defaults:"]
        .iter()
        .any(|k| t.starts_with(k));
    if starts_ok && t.contains("scenarios:") {
        return Some(t.to_string());
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_fenced_yaml() {
        let resp = "Here you go:\n```yaml\nname: x\nscenarios: {}\n```\nHope that helps";
        assert_eq!(
            extract_yaml(resp).as_deref(),
            Some("name: x\nscenarios: {}")
        );
    }

    #[test]
    fn extracts_bare_plan() {
        let resp = "name: x\nscenarios:\n  s: {}\n";
        assert!(extract_yaml(resp).unwrap().contains("scenarios:"));
    }

    #[test]
    fn none_when_no_plan() {
        assert!(extract_yaml("I can't help with that.").is_none());
    }
}
