//! WASM assertion plugin: validates the JSON response body against a JSON
//! Schema supplied in config.
//!
//! Config: `{"schema": { ... }}`.
//!
//! This is a *pragmatic subset* of JSON Schema (draft-07-ish), hand-rolled so
//! it compiles cleanly to `wasm32-wasip2` with no C or `std::net` deps. The
//! supported keywords are:
//!
//! - `type`            — one of `null|boolean|integer|number|string|array|object`,
//!                       or an array of such names (any match passes).
//! - `required`        — array of property names that must be present on an object.
//! - `properties`      — map of property name -> subschema (recursively validated
//!                       when the property is present).
//! - `items`           — subschema applied to every element of an array.
//! - `enum`            — array of allowed values (deep equality).
//! - `minimum`/`maximum`       — inclusive numeric bounds.
//! - `minLength`/`maxLength`   — inclusive string length bounds (Unicode chars).
//! - `minItems`/`maxItems`     — inclusive array length bounds.
//! - `minProperties`/`maxProperties` — inclusive object property-count bounds.
//!
//! Unknown keywords are ignored. The verdict passes when there are zero
//! violations; otherwise `detail` lists every violation found.

wit_bindgen::generate!({
    path: "../../crates/loadr-plugin-api/wit",
    world: "loadr-assertion-plugin",
});

use exports::loadr::plugin::assertion::{Guest as Assertion, Verdict};
use exports::loadr::plugin::meta::{Guest as Meta, Info};

use serde_json::Value;

// ---------------------------------------------------------------------------
// Pure logic (no wit types) — unit-tested on the host below.
// ---------------------------------------------------------------------------

/// The JSON Schema `type` name for an instance value. `integer` is reported for
/// numbers with no fractional part; everything else numeric is `number`.
fn type_name(value: &Value) -> &'static str {
    match value {
        Value::Null => "null",
        Value::Bool(_) => "boolean",
        Value::String(_) => "string",
        Value::Array(_) => "array",
        Value::Object(_) => "object",
        Value::Number(n) => {
            if n.is_i64() || n.is_u64() {
                "integer"
            } else {
                // Could still be an integral f64 (e.g. 2.0). Treat those as integers.
                match n.as_f64() {
                    Some(f) if f.fract() == 0.0 => "integer",
                    _ => "number",
                }
            }
        }
    }
}

/// Does `value` satisfy the JSON Schema `type` keyword `expected`?
/// `integer` satisfies a `number` requirement, but not vice versa.
fn type_matches(value: &Value, expected: &str) -> bool {
    let actual = type_name(value);
    if actual == expected {
        return true;
    }
    // An integer is also a valid number.
    expected == "number" && actual == "integer"
}

/// Numeric value of an instance as f64, if it is a JSON number.
fn as_f64(value: &Value) -> Option<f64> {
    value.as_f64()
}

/// A human-friendly rendering of an instance path (empty path -> `<root>`).
fn show_path(path: &str) -> String {
    if path.is_empty() {
        "<root>".to_string()
    } else {
        path.to_string()
    }
}

/// Join a parent path with a child key using dotted / bracketed notation.
fn join_key(parent: &str, key: &str) -> String {
    if parent.is_empty() {
        key.to_string()
    } else {
        format!("{parent}.{key}")
    }
}

/// Join a parent path with an array index.
fn join_index(parent: &str, idx: usize) -> String {
    format!("{parent}[{idx}]")
}

/// Validate `instance` against `schema`, appending any violations (as human
/// readable strings, each prefixed by the offending instance path) to `out`.
///
/// A non-object schema is itself a violation (schemas must be JSON objects).
fn validate(instance: &Value, schema: &Value, path: &str, out: &mut Vec<String>) {
    let obj = match schema.as_object() {
        Some(o) => o,
        None => {
            out.push(format!("{}: schema is not a JSON object", show_path(path)));
            return;
        }
    };

    // type
    if let Some(t) = obj.get("type") {
        let ok = match t {
            Value::String(s) => type_matches(instance, s),
            Value::Array(types) => types
                .iter()
                .filter_map(|v| v.as_str())
                .any(|s| type_matches(instance, s)),
            _ => true, // malformed type keyword: skip rather than reject.
        };
        if !ok {
            let want = match t {
                Value::String(s) => s.clone(),
                Value::Array(types) => types
                    .iter()
                    .filter_map(|v| v.as_str())
                    .collect::<Vec<_>>()
                    .join("|"),
                _ => String::new(),
            };
            out.push(format!(
                "{}: expected type {} but found {}",
                show_path(path),
                want,
                type_name(instance)
            ));
        }
    }

    // enum
    if let Some(Value::Array(allowed)) = obj.get("enum") {
        if !allowed.iter().any(|v| v == instance) {
            out.push(format!(
                "{}: value {} is not one of the allowed enum values",
                show_path(path),
                instance
            ));
        }
    }

    // Numeric bounds.
    if let Some(inst_n) = as_f64(instance) {
        if let Some(min) = obj.get("minimum").and_then(as_f64) {
            if inst_n < min {
                out.push(format!(
                    "{}: value {} is less than minimum {}",
                    show_path(path),
                    trim_num(inst_n),
                    trim_num(min)
                ));
            }
        }
        if let Some(max) = obj.get("maximum").and_then(as_f64) {
            if inst_n > max {
                out.push(format!(
                    "{}: value {} is greater than maximum {}",
                    show_path(path),
                    trim_num(inst_n),
                    trim_num(max)
                ));
            }
        }
    }

    // String length bounds (in Unicode scalar values).
    if let Value::String(s) = instance {
        let len = s.chars().count() as u64;
        if let Some(min) = obj.get("minLength").and_then(Value::as_u64) {
            if len < min {
                out.push(format!(
                    "{}: string length {} is less than minLength {}",
                    show_path(path),
                    len,
                    min
                ));
            }
        }
        if let Some(max) = obj.get("maxLength").and_then(Value::as_u64) {
            if len > max {
                out.push(format!(
                    "{}: string length {} is greater than maxLength {}",
                    show_path(path),
                    len,
                    max
                ));
            }
        }
    }

    // Array constraints.
    if let Value::Array(items) = instance {
        let len = items.len() as u64;
        if let Some(min) = obj.get("minItems").and_then(Value::as_u64) {
            if len < min {
                out.push(format!(
                    "{}: array length {} is less than minItems {}",
                    show_path(path),
                    len,
                    min
                ));
            }
        }
        if let Some(max) = obj.get("maxItems").and_then(Value::as_u64) {
            if len > max {
                out.push(format!(
                    "{}: array length {} is greater than maxItems {}",
                    show_path(path),
                    len,
                    max
                ));
            }
        }
        if let Some(items_schema) = obj.get("items") {
            for (idx, elem) in items.iter().enumerate() {
                validate(elem, items_schema, &join_index(path, idx), out);
            }
        }
    }

    // Object constraints.
    if let Value::Object(map) = instance {
        let count = map.len() as u64;
        if let Some(min) = obj.get("minProperties").and_then(Value::as_u64) {
            if count < min {
                out.push(format!(
                    "{}: object has {} properties, fewer than minProperties {}",
                    show_path(path),
                    count,
                    min
                ));
            }
        }
        if let Some(max) = obj.get("maxProperties").and_then(Value::as_u64) {
            if count > max {
                out.push(format!(
                    "{}: object has {} properties, more than maxProperties {}",
                    show_path(path),
                    count,
                    max
                ));
            }
        }

        // required
        if let Some(Value::Array(required)) = obj.get("required") {
            for req in required.iter().filter_map(|v| v.as_str()) {
                if !map.contains_key(req) {
                    out.push(format!(
                        "{}: missing required property \"{}\"",
                        show_path(path),
                        req
                    ));
                }
            }
        }

        // properties (recurse into present properties only)
        if let Some(Value::Object(props)) = obj.get("properties") {
            for (name, subschema) in props {
                if let Some(child) = map.get(name) {
                    validate(child, subschema, &join_key(path, name), out);
                }
            }
        }
    }
}

/// Format an f64 without a trailing `.0` for integral values, for tidier
/// violation messages.
fn trim_num(f: f64) -> String {
    if f.fract() == 0.0 && f.is_finite() {
        format!("{}", f as i64)
    } else {
        format!("{f}")
    }
}

/// Parse a response body and validate it against `schema`. Returns
/// `Ok(violations)` (possibly empty) on success, or `Err(message)` when the
/// body is not valid JSON.
fn validate_body(body: &[u8], schema: &Value) -> Result<Vec<String>, String> {
    let instance: Value = serde_json::from_slice(body)
        .map_err(|e| format!("response body is not valid JSON: {e}"))?;
    let mut out = Vec::new();
    validate(&instance, schema, "", &mut out);
    Ok(out)
}

// ---------------------------------------------------------------------------
// WASM plugin exports.
// ---------------------------------------------------------------------------

#[derive(serde::Deserialize)]
struct Config {
    schema: Value,
}

struct Plugin;

impl Meta for Plugin {
    fn describe() -> Info {
        Info {
            name: "json-schema".to_string(),
            version: env!("CARGO_PKG_VERSION").to_string(),
            kind: "assertion".to_string(),
            description:
                "Validates the JSON response body against a JSON Schema (pragmatic subset)"
                    .to_string(),
        }
    }
}

impl Assertion for Plugin {
    fn check(
        _status: i64,
        body: Vec<u8>,
        _headers: Vec<(String, String)>,
        _duration_ms: f64,
        config: String,
    ) -> Verdict {
        let config: Config = match serde_json::from_str(&config) {
            Ok(c) => c,
            Err(e) => {
                return Verdict {
                    pass: false,
                    detail: format!("invalid config (expected {{\"schema\": {{...}}}}): {e}"),
                }
            }
        };

        match validate_body(&body, &config.schema) {
            Ok(violations) if violations.is_empty() => Verdict {
                pass: true,
                detail: "body conforms to schema".to_string(),
            },
            Ok(violations) => {
                let n = violations.len();
                Verdict {
                    pass: false,
                    detail: format!(
                        "{n} schema violation{}: {}",
                        if n == 1 { "" } else { "s" },
                        violations.join("; ")
                    ),
                }
            }
            Err(e) => Verdict {
                pass: false,
                detail: e,
            },
        }
    }
}

export!(Plugin);

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn violations(instance: Value, schema: Value) -> Vec<String> {
        let mut out = Vec::new();
        validate(&instance, &schema, "", &mut out);
        out
    }

    #[test]
    fn type_name_distinguishes_integer_and_number() {
        assert_eq!(type_name(&json!(5)), "integer");
        assert_eq!(type_name(&json!(5.0)), "integer");
        assert_eq!(type_name(&json!(5.5)), "number");
        assert_eq!(type_name(&json!("x")), "string");
        assert_eq!(type_name(&json!(true)), "boolean");
        assert_eq!(type_name(&json!(null)), "null");
        assert_eq!(type_name(&json!([1])), "array");
        assert_eq!(type_name(&json!({"a":1})), "object");
    }

    #[test]
    fn integer_satisfies_number_but_not_reverse() {
        assert!(type_matches(&json!(3), "number"));
        assert!(type_matches(&json!(3), "integer"));
        assert!(!type_matches(&json!(3.5), "integer"));
        assert!(type_matches(&json!(3.5), "number"));
    }

    #[test]
    fn valid_object_has_no_violations() {
        let schema = json!({
            "type": "object",
            "required": ["id", "name"],
            "properties": {
                "id": {"type": "integer", "minimum": 1},
                "name": {"type": "string", "minLength": 1}
            }
        });
        let instance = json!({"id": 7, "name": "loadr"});
        assert!(violations(instance, schema).is_empty());
    }

    #[test]
    fn missing_required_property_is_reported() {
        let schema = json!({"type": "object", "required": ["id", "name"]});
        let instance = json!({"id": 1});
        let v = violations(instance, schema);
        assert_eq!(v.len(), 1);
        assert!(v[0].contains("missing required property \"name\""));
    }

    #[test]
    fn wrong_type_is_reported_with_path() {
        let schema = json!({
            "type": "object",
            "properties": {"id": {"type": "integer"}}
        });
        let instance = json!({"id": "not-a-number"});
        let v = violations(instance, schema);
        assert_eq!(v.len(), 1);
        assert!(v[0].starts_with("id:"), "got: {}", v[0]);
        assert!(v[0].contains("expected type integer"));
        assert!(v[0].contains("found string"));
    }

    #[test]
    fn enum_mismatch_is_reported() {
        let schema = json!({"enum": ["red", "green", "blue"]});
        assert!(violations(json!("green"), schema.clone()).is_empty());
        let v = violations(json!("purple"), schema);
        assert_eq!(v.len(), 1);
        assert!(v[0].contains("not one of the allowed enum"));
    }

    #[test]
    fn numeric_bounds_inclusive() {
        let schema = json!({"minimum": 1, "maximum": 10});
        assert!(violations(json!(1), schema.clone()).is_empty());
        assert!(violations(json!(10), schema.clone()).is_empty());
        assert_eq!(violations(json!(0), schema.clone()).len(), 1);
        let v = violations(json!(11), schema);
        assert_eq!(v.len(), 1);
        assert!(v[0].contains("greater than maximum 10"));
    }

    #[test]
    fn string_length_bounds() {
        let schema = json!({"type": "string", "minLength": 2, "maxLength": 4});
        assert!(violations(json!("ab"), schema.clone()).is_empty());
        assert!(violations(json!("abcd"), schema.clone()).is_empty());
        assert!(violations(json!("a"), schema.clone())[0].contains("less than minLength"));
        assert!(violations(json!("abcde"), schema)[0].contains("greater than maxLength"));
    }

    #[test]
    fn array_items_and_length() {
        let schema = json!({
            "type": "array",
            "minItems": 1,
            "maxItems": 3,
            "items": {"type": "integer", "minimum": 0}
        });
        assert!(violations(json!([1, 2, 3]), schema.clone()).is_empty());
        assert!(violations(json!([]), schema.clone())[0].contains("less than minItems"));
        assert!(
            violations(json!([1, 2, 3, 4]), schema.clone())[0].contains("greater than maxItems")
        );
        // Bad element reports the indexed path.
        let v = violations(json!([1, -5, 2]), schema);
        assert_eq!(v.len(), 1);
        assert!(v[0].starts_with("[1]:"), "got: {}", v[0]);
    }

    #[test]
    fn object_property_count_bounds() {
        let schema = json!({"type": "object", "minProperties": 1, "maxProperties": 2});
        assert!(violations(json!({"a": 1}), schema.clone()).is_empty());
        assert!(violations(json!({}), schema.clone())[0].contains("fewer than minProperties"));
        assert!(
            violations(json!({"a":1,"b":2,"c":3}), schema)[0].contains("more than maxProperties")
        );
    }

    #[test]
    fn nested_properties_recurse() {
        let schema = json!({
            "type": "object",
            "properties": {
                "user": {
                    "type": "object",
                    "required": ["email"],
                    "properties": {"email": {"type": "string"}}
                }
            }
        });
        let instance = json!({"user": {"name": "x"}});
        let v = violations(instance, schema);
        assert_eq!(v.len(), 1);
        assert!(v[0].starts_with("user:"), "got: {}", v[0]);
        assert!(v[0].contains("missing required property \"email\""));
    }

    #[test]
    fn multiple_violations_all_collected() {
        let schema = json!({
            "type": "object",
            "required": ["id"],
            "properties": {"age": {"type": "integer", "minimum": 0}}
        });
        let instance = json!({"age": -3});
        let v = violations(instance, schema);
        // Missing "id" and age below minimum.
        assert_eq!(v.len(), 2);
    }

    #[test]
    fn type_array_any_match_passes() {
        let schema = json!({"type": ["string", "null"]});
        assert!(violations(json!("x"), schema.clone()).is_empty());
        assert!(violations(json!(null), schema.clone()).is_empty());
        assert!(!violations(json!(5), schema).is_empty());
    }

    #[test]
    fn validate_body_rejects_non_json() {
        let schema = json!({"type": "object"});
        let err = validate_body(b"not json", &schema).unwrap_err();
        assert!(err.contains("not valid JSON"));
    }

    #[test]
    fn validate_body_returns_violations() {
        let schema = json!({"type": "object", "required": ["id"]});
        let out = validate_body(br#"{"name":"x"}"#, &schema).unwrap();
        assert_eq!(out.len(), 1);
        assert!(out[0].contains("missing required property \"id\""));
    }

    #[test]
    fn root_path_is_labelled() {
        let schema = json!({"type": "object"});
        let v = violations(json!(42), schema);
        assert_eq!(v.len(), 1);
        assert!(v[0].starts_with("<root>:"), "got: {}", v[0]);
    }
}
