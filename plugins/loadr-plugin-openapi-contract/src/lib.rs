//! WASM assertion plugin: asserts a response conforms to an OpenAPI operation
//! schema.
//!
//! Config: `{"schema": { ... }, "status": 200}`.
//!
//! `schema` is an OpenAPI 3.0 / JSON-Schema *subset* describing the expected
//! response body. `status` (optional) is the expected HTTP status code. The
//! response passes when the status matches (if given) and the JSON body
//! validates against the schema. On failure the verdict `detail` lists every
//! contract violation found so the report is actionable.
//!
//! Supported schema keywords (the pragmatic subset — the same subset used by
//! the `json-schema` assertion plugin, plus OpenAPI's `nullable`):
//!   - `type`        object | array | string | integer | number | boolean | null
//!                   (also accepts a JSON-Schema array of type names)
//!   - `nullable`    OpenAPI 3.0 flag: allows an explicit JSON `null`
//!   - `enum`        instance must equal one of the listed values
//!   - `required`    (objects) listed property keys must be present
//!   - `properties`  (objects) per-key sub-schemas
//!   - `additionalProperties` (objects) when `false`, reject unknown keys
//!   - `items`       (arrays) sub-schema applied to every element
//!   - `minItems` / `maxItems`         (arrays)
//!   - `minLength` / `maxLength`       (strings, counted in chars)
//!   - `minimum` / `maximum`           (numbers, inclusive)
//!   - `exclusiveMinimum` / `exclusiveMaximum` (numbers)

wit_bindgen::generate!({
    path: "../../crates/loadr-plugin-api/wit",
    world: "loadr-assertion-plugin",
});

use exports::loadr::plugin::assertion::{Guest as Assertion, Verdict};
use exports::loadr::plugin::meta::{Guest as Meta, Info};

use serde_json::Value;

#[derive(serde::Deserialize)]
struct Config {
    /// OpenAPI/JSON-Schema subset for the response body.
    schema: Value,
    /// Expected HTTP status code (optional — omit to skip the status check).
    #[serde(default)]
    status: Option<i64>,
}

// ---------------------------------------------------------------------------
// Pure logic (no wit types) — unit-tested on the host below.
// ---------------------------------------------------------------------------

/// The JSON type name of an instance, for human-readable messages.
fn json_type(v: &Value) -> &'static str {
    match v {
        Value::Null => "null",
        Value::Bool(_) => "boolean",
        Value::Number(_) => "number",
        Value::String(_) => "string",
        Value::Array(_) => "array",
        Value::Object(_) => "object",
    }
}

/// Does an instance satisfy a single OpenAPI/JSON-Schema type name?
fn type_name_matches(name: &str, instance: &Value) -> bool {
    match name {
        "object" => instance.is_object(),
        "array" => instance.is_array(),
        "string" => instance.is_string(),
        "boolean" => instance.is_boolean(),
        "null" => instance.is_null(),
        "number" => instance.is_number(),
        // JSON Schema integers accept whole-valued floats (e.g. `1.0`).
        "integer" => {
            instance.is_i64()
                || instance.is_u64()
                || instance.as_f64().map(|f| f.fract() == 0.0).unwrap_or(false)
        }
        _ => true, // unknown/unsupported type name: don't fail the contract on it
    }
}

/// Collect the declared type names from a `type` keyword, which may be a single
/// string or (JSON Schema) an array of strings.
fn type_names(type_val: &Value) -> Vec<&str> {
    match type_val {
        Value::String(s) => vec![s.as_str()],
        Value::Array(arr) => arr.iter().filter_map(|v| v.as_str()).collect(),
        _ => Vec::new(),
    }
}

/// Does the schema (via `type` and/or `nullable`) permit an explicit null?
fn allows_null(schema: &serde_json::Map<String, Value>) -> bool {
    if schema
        .get("nullable")
        .and_then(Value::as_bool)
        .unwrap_or(false)
    {
        return true;
    }
    match schema.get("type") {
        Some(t) => type_names(t).iter().any(|n| *n == "null"),
        None => false, // no type constraint at all: handled by caller
    }
}

/// Push a violation message anchored at `path`.
fn violation(errors: &mut Vec<String>, path: &str, msg: impl std::fmt::Display) {
    errors.push(format!("{path}: {msg}"));
}

/// Validate `instance` against `schema`, appending any violations to `errors`.
///
/// `path` is a JSONPath-ish breadcrumb (`$`, `$.name`, `$.tags[0]`) used only
/// for messages. A non-object schema (e.g. `true`) imposes no constraints.
fn validate_value(schema: &Value, instance: &Value, path: &str, errors: &mut Vec<String>) {
    let obj = match schema.as_object() {
        Some(o) => o,
        None => return,
    };

    // An explicit null: allowed only when the schema opts in (nullable / type
    // null). If it opts in we're done; otherwise let the type check below
    // produce a precise "expected X, got null" message.
    if instance.is_null() && allows_null(obj) {
        return;
    }

    // enum: instance must equal one of the allowed values exactly.
    if let Some(Value::Array(allowed)) = obj.get("enum") {
        if !allowed.iter().any(|a| a == instance) {
            let rendered = serde_json::to_string(instance).unwrap_or_default();
            violation(
                errors,
                path,
                format!("value {rendered} is not one of the allowed enum values"),
            );
        }
    }

    // type: if it doesn't match, report and stop (dependent keyword checks
    // below would only produce noise on a mistyped value).
    if let Some(type_val) = obj.get("type") {
        let names = type_names(type_val);
        if !names.is_empty() && !names.iter().any(|n| type_name_matches(n, instance)) {
            violation(
                errors,
                path,
                format!(
                    "expected type {}, got {}",
                    names.join("|"),
                    json_type(instance)
                ),
            );
            return;
        }
    }

    match instance {
        Value::Object(map) => validate_object(obj, map, path, errors),
        Value::Array(arr) => validate_array(obj, arr, path, errors),
        Value::String(s) => validate_string(obj, s, path, errors),
        Value::Number(_) => validate_number(obj, instance, path, errors),
        _ => {}
    }
}

fn validate_object(
    schema: &serde_json::Map<String, Value>,
    map: &serde_json::Map<String, Value>,
    path: &str,
    errors: &mut Vec<String>,
) {
    // required
    if let Some(Value::Array(required)) = schema.get("required") {
        for key in required.iter().filter_map(Value::as_str) {
            if !map.contains_key(key) {
                violation(errors, path, format!("missing required property \"{key}\""));
            }
        }
    }

    // properties: validate each present property against its sub-schema.
    let properties = schema.get("properties").and_then(Value::as_object);
    if let Some(props) = properties {
        for (key, sub_schema) in props {
            if let Some(child) = map.get(key) {
                validate_value(sub_schema, child, &child_path(path, key), errors);
            }
        }
    }

    // additionalProperties: false rejects keys absent from `properties`.
    if schema.get("additionalProperties") == Some(&Value::Bool(false)) {
        for key in map.keys() {
            let declared = properties.map(|p| p.contains_key(key)).unwrap_or(false);
            if !declared {
                violation(
                    errors,
                    path,
                    format!("unexpected additional property \"{key}\""),
                );
            }
        }
    }
}

fn validate_array(
    schema: &serde_json::Map<String, Value>,
    arr: &[Value],
    path: &str,
    errors: &mut Vec<String>,
) {
    if let Some(min) = schema.get("minItems").and_then(Value::as_u64) {
        if (arr.len() as u64) < min {
            violation(
                errors,
                path,
                format!("array has {} items, minimum is {min}", arr.len()),
            );
        }
    }
    if let Some(max) = schema.get("maxItems").and_then(Value::as_u64) {
        if (arr.len() as u64) > max {
            violation(
                errors,
                path,
                format!("array has {} items, maximum is {max}", arr.len()),
            );
        }
    }
    if let Some(items) = schema.get("items") {
        for (i, element) in arr.iter().enumerate() {
            validate_value(items, element, &index_path(path, i), errors);
        }
    }
}

fn validate_string(
    schema: &serde_json::Map<String, Value>,
    s: &str,
    path: &str,
    errors: &mut Vec<String>,
) {
    let len = s.chars().count() as u64;
    if let Some(min) = schema.get("minLength").and_then(Value::as_u64) {
        if len < min {
            violation(
                errors,
                path,
                format!("string length {len} is below minLength {min}"),
            );
        }
    }
    if let Some(max) = schema.get("maxLength").and_then(Value::as_u64) {
        if len > max {
            violation(
                errors,
                path,
                format!("string length {len} exceeds maxLength {max}"),
            );
        }
    }
}

fn validate_number(
    schema: &serde_json::Map<String, Value>,
    instance: &Value,
    path: &str,
    errors: &mut Vec<String>,
) {
    let n = match instance.as_f64() {
        Some(n) => n,
        None => return,
    };
    if let Some(min) = schema.get("minimum").and_then(Value::as_f64) {
        if n < min {
            violation(errors, path, format!("value {n} is below minimum {min}"));
        }
    }
    if let Some(max) = schema.get("maximum").and_then(Value::as_f64) {
        if n > max {
            violation(errors, path, format!("value {n} exceeds maximum {max}"));
        }
    }
    if let Some(min) = schema.get("exclusiveMinimum").and_then(Value::as_f64) {
        if n <= min {
            violation(
                errors,
                path,
                format!("value {n} must be greater than exclusiveMinimum {min}"),
            );
        }
    }
    if let Some(max) = schema.get("exclusiveMaximum").and_then(Value::as_f64) {
        if n >= max {
            violation(
                errors,
                path,
                format!("value {n} must be less than exclusiveMaximum {max}"),
            );
        }
    }
}

fn child_path(path: &str, key: &str) -> String {
    format!("{path}.{key}")
}

fn index_path(path: &str, i: usize) -> String {
    format!("{path}[{i}]")
}

/// Evaluate the full contract: optional status check + schema validation of the
/// JSON body. Returns `(pass, detail)`.
fn evaluate(
    status: i64,
    body: &[u8],
    schema: &Value,
    expected_status: Option<i64>,
) -> (bool, String) {
    let mut problems: Vec<String> = Vec::new();

    if let Some(expected) = expected_status {
        if status != expected {
            problems.push(format!(
                "status {status} does not match expected {expected}"
            ));
        }
    }

    match serde_json::from_slice::<Value>(body) {
        Ok(instance) => validate_value(schema, &instance, "$", &mut problems),
        Err(e) => problems.push(format!("response body is not valid JSON: {e}")),
    }

    if problems.is_empty() {
        let status_note = match expected_status {
            Some(_) => format!("status {status} and body "),
            None => "body ".to_string(),
        };
        (
            true,
            format!("response conforms to OpenAPI schema ({status_note}validated)"),
        )
    } else {
        (
            false,
            format!(
                "{} contract violation(s): {}",
                problems.len(),
                problems.join("; ")
            ),
        )
    }
}

// ---------------------------------------------------------------------------
// WASM plugin exports.
// ---------------------------------------------------------------------------

struct Plugin;

impl Meta for Plugin {
    fn describe() -> Info {
        Info {
            name: "openapi-contract".to_string(),
            version: env!("CARGO_PKG_VERSION").to_string(),
            kind: "assertion".to_string(),
            description:
                "Asserts the response status and body conform to an OpenAPI operation schema"
                    .to_string(),
        }
    }
}

impl Assertion for Plugin {
    fn check(
        status: i64,
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
                    detail: format!("invalid config: {e}"),
                }
            }
        };
        let (pass, detail) = evaluate(status, &body, &config.schema, config.status);
        Verdict { pass, detail }
    }
}

export!(Plugin);

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn errs(schema: &Value, instance: &Value) -> Vec<String> {
        let mut e = Vec::new();
        validate_value(schema, instance, "$", &mut e);
        e
    }

    #[test]
    fn accepts_conforming_object() {
        let schema = json!({
            "type": "object",
            "required": ["id", "name"],
            "properties": {
                "id": { "type": "integer", "minimum": 1 },
                "name": { "type": "string", "minLength": 1 }
            }
        });
        assert!(errs(&schema, &json!({ "id": 5, "name": "widget" })).is_empty());
    }

    #[test]
    fn flags_missing_required_property() {
        let schema = json!({ "type": "object", "required": ["id", "name"] });
        let e = errs(&schema, &json!({ "id": 1 }));
        assert_eq!(e.len(), 1);
        assert!(e[0].contains("missing required property \"name\""), "{e:?}");
    }

    #[test]
    fn flags_wrong_type_and_stops_dependent_checks() {
        let schema = json!({ "type": "string", "minLength": 3 });
        let e = errs(&schema, &json!(42));
        // Only the type error — no cascading minLength noise on a non-string.
        assert_eq!(e.len(), 1);
        assert!(e[0].contains("expected type string, got number"), "{e:?}");
    }

    #[test]
    fn integer_accepts_whole_float_rejects_fractional() {
        let schema = json!({ "type": "integer" });
        assert!(errs(&schema, &json!(3.0)).is_empty());
        assert_eq!(errs(&schema, &json!(3.5)).len(), 1);
    }

    #[test]
    fn nullable_allows_null_but_bare_type_does_not() {
        let nullable = json!({ "type": "string", "nullable": true });
        assert!(errs(&nullable, &json!(null)).is_empty());

        let strict = json!({ "type": "string" });
        let e = errs(&strict, &json!(null));
        assert_eq!(e.len(), 1);
        assert!(e[0].contains("got null"), "{e:?}");
    }

    #[test]
    fn enum_membership_is_enforced() {
        let schema = json!({ "enum": ["active", "inactive"] });
        assert!(errs(&schema, &json!("active")).is_empty());
        let e = errs(&schema, &json!("pending"));
        assert_eq!(e.len(), 1);
        assert!(e[0].contains("enum"), "{e:?}");
    }

    #[test]
    fn string_length_bounds() {
        let schema = json!({ "type": "string", "minLength": 2, "maxLength": 4 });
        assert!(errs(&schema, &json!("abc")).is_empty());
        assert!(errs(&schema, &json!("a"))[0].contains("below minLength"));
        assert!(errs(&schema, &json!("abcde"))[0].contains("exceeds maxLength"));
        // char count, not byte count — a 3-char multibyte string passes.
        assert!(errs(&schema, &json!("é€ß")).is_empty());
    }

    #[test]
    fn number_bounds_inclusive_and_exclusive() {
        let inclusive = json!({ "type": "number", "minimum": 0, "maximum": 10 });
        assert!(errs(&inclusive, &json!(0)).is_empty());
        assert!(errs(&inclusive, &json!(10)).is_empty());
        assert!(errs(&inclusive, &json!(-1))[0].contains("below minimum"));
        assert!(errs(&inclusive, &json!(11))[0].contains("exceeds maximum"));

        let exclusive = json!({ "type": "number", "exclusiveMinimum": 0, "exclusiveMaximum": 10 });
        assert!(errs(&exclusive, &json!(0))[0].contains("exclusiveMinimum"));
        assert!(errs(&exclusive, &json!(10))[0].contains("exclusiveMaximum"));
        assert!(errs(&exclusive, &json!(5)).is_empty());
    }

    #[test]
    fn array_items_and_bounds() {
        let schema = json!({
            "type": "array",
            "items": { "type": "string" },
            "minItems": 1,
            "maxItems": 2
        });
        assert!(errs(&schema, &json!(["a", "b"])).is_empty());
        assert!(errs(&schema, &json!([]))[0].contains("minimum is 1"));
        assert!(errs(&schema, &json!(["a", "b", "c"]))[0].contains("maximum is 2"));

        // Wrong element type is reported with an indexed path.
        let e = errs(&schema, &json!(["ok", 5]));
        assert_eq!(e.len(), 1);
        assert!(e[0].starts_with("$[1]:"), "{e:?}");
    }

    #[test]
    fn additional_properties_false_rejects_unknown_keys() {
        let schema = json!({
            "type": "object",
            "properties": { "id": { "type": "integer" } },
            "additionalProperties": false
        });
        assert!(errs(&schema, &json!({ "id": 1 })).is_empty());
        let e = errs(&schema, &json!({ "id": 1, "extra": true }));
        assert_eq!(e.len(), 1);
        assert!(
            e[0].contains("unexpected additional property \"extra\""),
            "{e:?}"
        );
    }

    #[test]
    fn nested_property_path_is_reported() {
        let schema = json!({
            "type": "object",
            "properties": {
                "user": {
                    "type": "object",
                    "properties": { "age": { "type": "integer" } }
                }
            }
        });
        let e = errs(&schema, &json!({ "user": { "age": "old" } }));
        assert_eq!(e.len(), 1);
        assert!(e[0].starts_with("$.user.age:"), "{e:?}");
    }

    #[test]
    fn evaluate_passes_on_matching_status_and_body() {
        let schema = json!({ "type": "object", "required": ["id"] });
        let (pass, detail) = evaluate(200, br#"{"id":1}"#, &schema, Some(200));
        assert!(pass, "{detail}");
        assert!(detail.contains("conforms"), "{detail}");
    }

    #[test]
    fn evaluate_fails_on_status_mismatch() {
        let schema = json!({ "type": "object" });
        let (pass, detail) = evaluate(500, b"{}", &schema, Some(200));
        assert!(!pass);
        assert!(
            detail.contains("status 500 does not match expected 200"),
            "{detail}"
        );
    }

    #[test]
    fn evaluate_skips_status_when_not_configured() {
        let schema = json!({ "type": "object" });
        let (pass, _) = evaluate(418, b"{}", &schema, None);
        assert!(pass);
    }

    #[test]
    fn evaluate_reports_invalid_json_body() {
        let schema = json!({ "type": "object" });
        let (pass, detail) = evaluate(200, b"not json", &schema, Some(200));
        assert!(!pass);
        assert!(detail.contains("not valid JSON"), "{detail}");
    }

    #[test]
    fn evaluate_aggregates_multiple_violations() {
        let schema = json!({
            "type": "object",
            "required": ["id", "name"],
            "properties": {
                "id": { "type": "integer" },
                "name": { "type": "string" }
            }
        });
        // Wrong status, missing "name", wrong type for "id".
        let (pass, detail) = evaluate(404, br#"{"id":"x"}"#, &schema, Some(200));
        assert!(!pass);
        assert!(detail.contains("status 404"), "{detail}");
        assert!(
            detail.contains("missing required property \"name\""),
            "{detail}"
        );
        assert!(detail.contains("$.id:"), "{detail}");
    }
}
