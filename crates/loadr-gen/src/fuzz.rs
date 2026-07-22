//! Schema-aware fuzz variants for an operation.
//!
//! For each operation with a JSON body, `--fuzz` appends variant requests
//! beside the valid one: structural mutations (drop a required key, confuse a
//! type) and adversarial payloads from [`loadr_payload`]. Every variant carries
//! a single assertion — the status must be 2xx–4xx, so **any 5xx fails**. A
//! contract that promises to reject bad input should never crash on it.

use serde_json::Value;

use loadr_config::{Body, BodySpec, Condition, RequestStep};

use crate::example::{example_for, Ctx, Resolver};

/// Default adversarial payload kinds when `--fuzz-payloads` is not given.
pub const DEFAULT_PAYLOADS: [&str; 2] = ["nested-json", "long-string"];

/// Build fuzz-variant steps for one operation.
pub fn variants_for(
    op: &Value,
    base: &RequestStep,
    r: &Resolver,
    payload_kinds: &[String],
) -> Vec<RequestStep> {
    // Only fuzz operations with a JSON request body.
    let schema = match op
        .get("requestBody")
        .and_then(|b| b.get("content"))
        .and_then(|c| c.get("application/json"))
        .and_then(|j| j.get("schema"))
    {
        Some(s) => s,
        None => return Vec::new(),
    };

    let mut ctx = Ctx::default();
    let example = example_for(schema, r, &mut ctx);
    let mut out = Vec::new();

    // 1. Structural — drop each required key.
    if let (Some(required), Value::Object(obj)) =
        (schema.get("required").and_then(|r| r.as_array()), &example)
    {
        for key in required.iter().filter_map(|k| k.as_str()) {
            let mut mutated = obj.clone();
            if mutated.remove(key).is_some() {
                out.push(variant(
                    base,
                    &format!("missing required `{key}`"),
                    json_body(Value::Object(mutated)),
                ));
            }
        }
    }

    // 2. Type confusion — swap the first scalar to the wrong type.
    if let Value::Object(obj) = &example {
        if let Some((k, v)) = obj.iter().find(|(_, v)| v.is_string() || v.is_number()) {
            let swapped = if v.is_string() {
                Value::from(999_999_999_i64)
            } else {
                Value::from("not-a-number")
            };
            let mut mutated = obj.clone();
            mutated.insert(k.clone(), swapped);
            out.push(variant(
                base,
                &format!("wrong type for `{k}`"),
                json_body(Value::Object(mutated)),
            ));
        }
    }

    // 3. Adversarial payloads — replace the body with a loadr-payload catalog entry.
    for kind in payload_kinds {
        if let Ok(bytes) = loadr_payload::generate_str(kind) {
            let text = String::from_utf8_lossy(&bytes).into_owned();
            out.push(variant(
                base,
                &format!("adversarial `{kind}`"),
                Body::Text(text),
            ));
        }
    }

    out
}

fn json_body(v: Value) -> Body {
    Body::Spec(BodySpec {
        json: Some(v),
        ..Default::default()
    })
}

fn variant(base: &RequestStep, desc: &str, body: Body) -> RequestStep {
    let mut s = base.clone();
    s.name = Some(format!(
        "{} [fuzz: {desc}]",
        base.name.as_deref().unwrap_or("")
    ));
    s.body = Some(body);
    // The gate: 2xx–4xx passes; any 5xx fails.
    s.assert = vec![Condition::Status {
        name: Some("no 5xx".into()),
        equals: None,
        one_of: None,
        matches: Some("^[234]..$".into()),
        on_failure: None,
    }];
    s
}
