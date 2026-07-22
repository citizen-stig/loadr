//! Schema → example JSON value engine.
//!
//! Given a JSON-Schema fragment (as found in an OpenAPI document) it returns a
//! representative `serde_json::Value`, resolving local `$ref`s and terminating
//! on self-referential schemas via a depth cap + visited-set stub.

use std::collections::HashSet;

use serde_json::{json, Map, Value};

const MAX_DEPTH: usize = 12;
const MAX_OPTIONAL: usize = 8;

/// Resolves local `#/...` JSON-Pointer `$ref`s against the root document.
pub struct Resolver<'a> {
    root: &'a Value,
}

impl<'a> Resolver<'a> {
    pub fn new(root: &'a Value) -> Self {
        Self { root }
    }

    fn resolve(&self, ptr: &str) -> Option<&'a Value> {
        let rest = ptr.strip_prefix("#/")?;
        let mut cur = self.root;
        for raw in rest.split('/') {
            let seg = raw.replace("~1", "/").replace("~0", "~");
            cur = cur.get(&seg)?;
        }
        Some(cur)
    }
}

/// Carries recursion state and collected warnings across one example build.
#[derive(Default)]
pub struct Ctx {
    stack: HashSet<String>,
    depth: usize,
    pub warnings: Vec<String>,
}

/// Build a representative example value for `schema`.
pub fn example_for(schema: &Value, r: &Resolver, ctx: &mut Ctx) -> Value {
    if ctx.depth > MAX_DEPTH {
        return Value::Null;
    }

    // $ref — resolve, guarding against cycles.
    if let Some(ptr) = schema.get("$ref").and_then(|v| v.as_str()) {
        if ctx.stack.contains(ptr) {
            return json!({}); // cycle: minimal stub
        }
        return match r.resolve(ptr) {
            Some(target) => {
                ctx.stack.insert(ptr.to_string());
                ctx.depth += 1;
                let v = example_for(target, r, ctx);
                ctx.depth -= 1;
                ctx.stack.remove(ptr);
                v
            }
            None => {
                ctx.warnings
                    .push(format!("unresolved $ref `{ptr}` — used null"));
                Value::Null
            }
        };
    }

    // Explicit example / default / enum, highest precedence first.
    if let Some(ex) = schema.get("example") {
        return ex.clone();
    }
    if let Some(ex) = schema
        .get("examples")
        .and_then(|e| e.as_array())
        .and_then(|a| a.first())
    {
        return ex.clone();
    }
    if let Some(d) = schema.get("default") {
        return d.clone();
    }
    if let Some(e) = schema
        .get("enum")
        .and_then(|e| e.as_array())
        .and_then(|a| a.first())
    {
        return e.clone();
    }

    // allOf — deep-merge object members.
    if let Some(all) = schema.get("allOf").and_then(|a| a.as_array()) {
        let mut merged = Map::new();
        for sub in all {
            if let Value::Object(o) = example_for(sub, r, ctx) {
                for (k, v) in o {
                    merged.insert(k, v);
                }
            }
        }
        return Value::Object(merged);
    }
    // oneOf / anyOf — take the first, with a note.
    for key in ["oneOf", "anyOf"] {
        if let Some(first) = schema
            .get(key)
            .and_then(|a| a.as_array())
            .and_then(|a| a.first())
        {
            ctx.warnings
                .push(format!("`{key}` schema — used the first variant"));
            return example_for(first, r, ctx);
        }
    }

    // By type (inferred as object when `properties` present, else string).
    let ty = schema.get("type").and_then(|t| t.as_str()).unwrap_or(
        if schema.get("properties").is_some() {
            "object"
        } else {
            "string"
        },
    );

    match ty {
        "object" => {
            let mut m = Map::new();
            let required: HashSet<String> = schema
                .get("required")
                .and_then(|r| r.as_array())
                .map(|a| {
                    a.iter()
                        .filter_map(|v| v.as_str().map(String::from))
                        .collect()
                })
                .unwrap_or_default();
            if let Some(props) = schema.get("properties").and_then(|p| p.as_object()) {
                let mut optional = 0;
                for (k, sub) in props {
                    if !required.contains(k) {
                        if optional >= MAX_OPTIONAL {
                            continue;
                        }
                        optional += 1;
                    }
                    ctx.depth += 1;
                    m.insert(k.clone(), example_for(sub, r, ctx));
                    ctx.depth -= 1;
                }
            }
            Value::Object(m)
        }
        "array" => match schema.get("items") {
            Some(items) => {
                ctx.depth += 1;
                let v = example_for(items, r, ctx);
                ctx.depth -= 1;
                json!([v])
            }
            None => json!([]),
        },
        "integer" => json!(schema.get("minimum").and_then(|m| m.as_i64()).unwrap_or(0)),
        "number" => json!(schema
            .get("minimum")
            .and_then(|m| m.as_f64())
            .unwrap_or(0.0)),
        "boolean" => json!(false),
        "null" => Value::Null,
        _ => Value::String(string_example(schema)),
    }
}

fn string_example(schema: &Value) -> String {
    match schema.get("format").and_then(|f| f.as_str()).unwrap_or("") {
        "date-time" => "2020-01-01T00:00:00Z".into(),
        "date" => "2020-01-01".into(),
        "uuid" => "3f2504e0-4f89-41d3-9a0c-0305e82c3301".into(),
        "email" => "user@example.com".into(),
        "uri" | "url" => "https://example.com".into(),
        "hostname" => "example.com".into(),
        "ipv4" => "192.0.2.1".into(),
        _ => {
            let min = schema
                .get("minLength")
                .and_then(|m| m.as_u64())
                .unwrap_or(0) as usize;
            if min > "string".len() {
                "a".repeat(min)
            } else {
                "string".into()
            }
        }
    }
}

/// Render a scalar example as a string for a URL path/query/header slot.
pub fn scalar_to_string(v: &Value) -> String {
    match v {
        Value::String(s) => s.clone(),
        Value::Null => String::new(),
        other => other.to_string(),
    }
}
