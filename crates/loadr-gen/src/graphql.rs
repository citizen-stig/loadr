//! GraphQL introspection JSON → loadr plan.
//!
//! For each field on the `Query`/`Mutation` root types, generate an operation:
//! a document with the field's args lifted to variables and a selection set
//! expanded to a bounded depth (scalars as leaves, objects recursed with a
//! cycle guard). Emitted as `RequestStep.graphql`.

use std::collections::{HashMap, HashSet};

use indexmap::IndexMap;
use serde_json::{json, Map, Value};

use loadr_config::{
    Defaults, Dur, ExecutorKind, GraphqlOptions, HttpDefaults, RequestStep, Scenario, Step,
    TestPlan,
};

use crate::{parse_contract, Conversion, ConversionWarning, GenError, GenOptions};

const MAX_DEPTH: usize = 3;
const MAX_FIELDS: usize = 20;

type TypeMap<'a> = HashMap<String, &'a Value>;

/// Generate a plan from a GraphQL introspection result (JSON).
pub fn gen_graphql(source: &str, opts: &GenOptions) -> Result<Conversion, GenError> {
    let root = parse_contract(source)?;
    let schema = root
        .pointer("/data/__schema")
        .or_else(|| root.get("__schema"))
        .ok_or_else(|| {
            GenError::GraphQl("missing `__schema` (need an introspection result JSON)".into())
        })?;

    let types = schema
        .get("types")
        .and_then(|t| t.as_array())
        .ok_or_else(|| GenError::GraphQl("introspection has no `types`".into()))?;

    let mut typemap: TypeMap = HashMap::new();
    for t in types {
        if let Some(n) = t.get("name").and_then(|n| n.as_str()) {
            typemap.insert(n.to_string(), t);
        }
    }

    let endpoint = opts
        .base_url
        .clone()
        .unwrap_or_else(|| "/graphql".to_string());
    let mut flow: Vec<Step> = Vec::new();

    for (op_kind, root_ptr) in [
        ("query", "/queryType/name"),
        ("mutation", "/mutationType/name"),
    ] {
        let root_name = match schema.pointer(root_ptr).and_then(|n| n.as_str()) {
            Some(n) => n,
            None => continue,
        };
        let fields = match typemap
            .get(root_name)
            .and_then(|t| t.get("fields"))
            .and_then(|f| f.as_array())
        {
            Some(f) => f,
            None => continue,
        };
        for field in fields {
            let fname = field.get("name").and_then(|n| n.as_str()).unwrap_or("");
            if fname.is_empty() || fname.starts_with("__") {
                continue;
            }
            let mut step = build_field(op_kind, fname, field, &typemap);
            step.url = endpoint.clone();
            flow.push(Step::Request(Box::new(step)));
        }
    }

    if flow.is_empty() {
        return Err(GenError::GraphQl("no query/mutation fields found".into()));
    }

    let n = flow.len();
    let mut scenarios = IndexMap::new();
    scenarios.insert(
        "api".to_string(),
        Scenario {
            executor: ExecutorKind::ConstantVus,
            vus: Some(1),
            duration: Some(Dur::from_millis(60_000)),
            flow,
            ..Default::default()
        },
    );

    let plan = TestPlan {
        name: Some("graphql".to_string()),
        description: Some(
            "Generated from a GraphQL introspection by `loadr gen`. Review and set real load."
                .into(),
        ),
        defaults: Defaults {
            http: HttpDefaults {
                base_url: opts.base_url.clone(),
                ..Default::default()
            },
            ..Default::default()
        },
        scenarios,
        ..Default::default()
    };

    let warnings = vec![ConversionWarning {
        element: "scenario `api`".into(),
        message: format!(
            "generated {n} operation(s); pass the GraphQL endpoint with --base-url and set real load"
        ),
    }];

    Ok(Conversion { plan, warnings })
}

fn build_field(op_kind: &str, fname: &str, field: &Value, typemap: &TypeMap) -> RequestStep {
    let mut var_defs: Vec<String> = Vec::new();
    let mut arg_uses: Vec<String> = Vec::new();
    let mut variables = Map::new();

    if let Some(args) = field.get("args").and_then(|a| a.as_array()) {
        for arg in args {
            let an = arg.get("name").and_then(|n| n.as_str()).unwrap_or("");
            if an.is_empty() {
                continue;
            }
            let tref = arg.get("type").unwrap_or(&Value::Null);
            var_defs.push(format!("${an}: {}", render_type_ref(tref)));
            arg_uses.push(format!("{an}: ${an}"));
            variables.insert(an.to_string(), example_input(tref, typemap, 0));
        }
    }

    let (ret_name, ret_kind) = unwrap(field.get("type").unwrap_or(&Value::Null));
    let selection = if matches!(ret_kind.as_str(), "OBJECT" | "INTERFACE" | "UNION") {
        let mut visited = HashSet::new();
        format!(" {}", selection_set(&ret_name, typemap, 1, &mut visited))
    } else {
        String::new()
    };

    let sig = if var_defs.is_empty() {
        String::new()
    } else {
        format!("({})", var_defs.join(", "))
    };
    let call = if arg_uses.is_empty() {
        String::new()
    } else {
        format!("({})", arg_uses.join(", "))
    };
    let query = format!("{op_kind} {fname}{sig} {{\n  {fname}{call}{selection}\n}}\n");

    RequestStep {
        name: Some(format!("{op_kind} {fname}")),
        method: Some("POST".into()),
        graphql: Some(GraphqlOptions {
            query,
            variables: if variables.is_empty() {
                None
            } else {
                Some(Value::Object(variables))
            },
            operation_name: Some(fname.to_string()),
        }),
        ..Default::default()
    }
}

/// Render a type ref back to SDL (`String!`, `[Int!]`).
fn render_type_ref(t: &Value) -> String {
    match t.get("kind").and_then(|k| k.as_str()) {
        Some("NON_NULL") => format!("{}!", render_type_ref(child(t))),
        Some("LIST") => format!("[{}]", render_type_ref(child(t))),
        _ => t
            .get("name")
            .and_then(|n| n.as_str())
            .unwrap_or("String")
            .to_string(),
    }
}

/// Strip NON_NULL/LIST wrappers to the named type + its kind.
fn unwrap(t: &Value) -> (String, String) {
    let mut cur = t;
    while matches!(
        cur.get("kind").and_then(|k| k.as_str()),
        Some("NON_NULL") | Some("LIST")
    ) {
        cur = child(cur);
    }
    (
        cur.get("name")
            .and_then(|n| n.as_str())
            .unwrap_or("")
            .to_string(),
        cur.get("kind")
            .and_then(|k| k.as_str())
            .unwrap_or("")
            .to_string(),
    )
}

fn child(t: &Value) -> &Value {
    t.get("ofType").unwrap_or(&Value::Null)
}

fn selection_set(
    type_name: &str,
    typemap: &TypeMap,
    depth: usize,
    visited: &mut HashSet<String>,
) -> String {
    if depth > MAX_DEPTH || visited.contains(type_name) {
        return "{ __typename }".to_string();
    }
    let type_def = match typemap.get(type_name) {
        Some(t) => *t,
        None => return "{ __typename }".to_string(),
    };
    visited.insert(type_name.to_string());

    let mut sels: Vec<String> = Vec::new();
    if let Some(fields) = type_def.get("fields").and_then(|f| f.as_array()) {
        for f in fields.iter().take(MAX_FIELDS) {
            let fname = f.get("name").and_then(|n| n.as_str()).unwrap_or("");
            if fname.is_empty() || fname.starts_with("__") {
                continue;
            }
            // Skip nested fields that require args — we can't fill them here.
            if f.get("args")
                .and_then(|a| a.as_array())
                .is_some_and(|a| !a.is_empty())
            {
                continue;
            }
            let (child_name, child_kind) = unwrap(f.get("type").unwrap_or(&Value::Null));
            if matches!(child_kind.as_str(), "OBJECT" | "INTERFACE" | "UNION") {
                sels.push(format!(
                    "{fname} {}",
                    selection_set(&child_name, typemap, depth + 1, visited)
                ));
            } else {
                sels.push(fname.to_string());
            }
        }
    }
    visited.remove(type_name);

    if sels.is_empty() {
        sels.push("__typename".to_string());
    }
    format!("{{ {} }}", sels.join(" "))
}

fn example_input(t: &Value, typemap: &TypeMap, depth: usize) -> Value {
    match t.get("kind").and_then(|k| k.as_str()) {
        Some("NON_NULL") => example_input(child(t), typemap, depth),
        Some("LIST") => json!([example_input(child(t), typemap, depth)]),
        Some("ENUM") => {
            let name = t.get("name").and_then(|n| n.as_str()).unwrap_or("");
            typemap
                .get(name)
                .and_then(|td| td.get("enumValues"))
                .and_then(|e| e.as_array())
                .and_then(|a| a.first())
                .and_then(|v| v.get("name"))
                .cloned()
                .unwrap_or_else(|| Value::String("ENUM".into()))
        }
        Some("INPUT_OBJECT") if depth <= 4 => {
            let name = t.get("name").and_then(|n| n.as_str()).unwrap_or("");
            let mut m = Map::new();
            if let Some(fields) = typemap
                .get(name)
                .and_then(|td| td.get("inputFields"))
                .and_then(|f| f.as_array())
            {
                for f in fields {
                    if let Some(fn_) = f.get("name").and_then(|n| n.as_str()) {
                        m.insert(
                            fn_.to_string(),
                            example_input(
                                f.get("type").unwrap_or(&Value::Null),
                                typemap,
                                depth + 1,
                            ),
                        );
                    }
                }
            }
            Value::Object(m)
        }
        // SCALAR (or unresolved) — by conventional name.
        _ => match t.get("name").and_then(|n| n.as_str()).unwrap_or("String") {
            "Int" => json!(0),
            "Float" => json!(0.0),
            "Boolean" => json!(false),
            "ID" => json!("id"),
            _ => json!("string"),
        },
    }
}
