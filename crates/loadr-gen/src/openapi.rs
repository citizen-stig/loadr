//! OpenAPI 3.x → loadr plan.
//!
//! Walks `paths.{path}.{method}` and builds one [`RequestStep`] per operation,
//! filling path/query/header params and the JSON request body from
//! schema-derived examples. The emitted plan mirrors `convert_har`'s shape
//! (one `constant-vus` scenario) and passes `loadr_config` validation.

use indexmap::IndexMap;
use serde_json::Value;

use loadr_config::{
    Body, BodySpec, Condition, Defaults, Dur, ExecutorKind, HttpDefaults, RequestStep, Scenario,
    Step, TestPlan,
};

use crate::example::{example_for, scalar_to_string, Ctx, Resolver};
use crate::{parse_contract, Conversion, ConversionWarning, GenError, GenOptions};

const HTTP_METHODS: [&str; 7] = ["get", "put", "post", "delete", "patch", "head", "options"];

/// Generate a plan from an OpenAPI document (JSON or YAML).
pub fn gen_openapi(source: &str, opts: &GenOptions) -> Result<Conversion, GenError> {
    let root = parse_contract(source)?;
    let resolver = Resolver::new(&root);
    let mut warnings: Vec<ConversionWarning> = Vec::new();

    let base_url = opts.base_url.clone().or_else(|| {
        root.get("servers")
            .and_then(|s| s.as_array())
            .and_then(|a| a.get(opts.server))
            .and_then(|s| s.get("url"))
            .and_then(|u| u.as_str())
            .map(String::from)
    });

    let title = root
        .pointer("/info/title")
        .and_then(|t| t.as_str())
        .unwrap_or("api")
        .to_string();

    let paths = root
        .get("paths")
        .and_then(|p| p.as_object())
        .ok_or_else(|| GenError::OpenApi("missing `paths` (not an OpenAPI document?)".into()))?;

    let mut flow: Vec<Step> = Vec::new();
    for (path, item) in paths {
        for m in HTTP_METHODS {
            let op = match item.get(m) {
                Some(o) if o.is_object() => o,
                _ => continue,
            };
            let selector = op
                .get("operationId")
                .and_then(|o| o.as_str())
                .map(String::from)
                .unwrap_or_else(|| format!("{m} {path}"));
            if !selected(&selector, path, opts) {
                continue;
            }
            flow.push(Step::Request(Box::new(build_op(
                path,
                m,
                op,
                &resolver,
                &mut warnings,
            ))));
        }
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
        name: Some(title),
        description: Some(
            "Generated from an OpenAPI contract by `loadr gen`. Review and set real load.".into(),
        ),
        defaults: Defaults {
            http: HttpDefaults {
                base_url,
                ..Default::default()
            },
            ..Default::default()
        },
        scenarios,
        ..Default::default()
    };

    warnings.push(ConversionWarning {
        element: "scenario `api`".into(),
        message: format!(
            "generated {n} request(s); defaulted to constant-vus 1 VU for 60s — set real vus/duration/executor before load testing"
        ),
    });

    Ok(Conversion { plan, warnings })
}

fn selected(selector: &str, path: &str, opts: &GenOptions) -> bool {
    let hits = |globs: &[String]| {
        globs
            .iter()
            .any(|g| glob_match(g, selector) || glob_match(g, path))
    };
    if !opts.include.is_empty() && !hits(&opts.include) {
        return false;
    }
    if !opts.exclude.is_empty() && hits(&opts.exclude) {
        return false;
    }
    true
}

/// Minimal `*` glob (no regex dep): exact, prefix `foo*`, suffix `*foo`,
/// or contains `*foo*`.
fn glob_match(pat: &str, s: &str) -> bool {
    match (pat.strip_prefix('*'), pat.strip_suffix('*')) {
        (Some(_), Some(_)) => s.contains(pat.trim_matches('*')),
        (Some(suffix), None) => s.ends_with(suffix),
        (None, Some(prefix)) => s.starts_with(prefix),
        (None, None) => pat == s,
    }
}

fn build_op(
    path: &str,
    method: &str,
    op: &Value,
    r: &Resolver,
    warnings: &mut Vec<ConversionWarning>,
) -> RequestStep {
    let mut ctx = Ctx::default();
    let mut url = path.to_string();
    let mut headers: IndexMap<String, String> = IndexMap::new();
    let mut params: IndexMap<String, String> = IndexMap::new();

    if let Some(ps) = op.get("parameters").and_then(|p| p.as_array()) {
        for p in ps {
            let name = p.get("name").and_then(|n| n.as_str()).unwrap_or("");
            if name.is_empty() {
                continue;
            }
            let schema = p.get("schema").cloned().unwrap_or(Value::Null);
            let ex = scalar_to_string(&example_for(&schema, r, &mut ctx));
            match p.get("in").and_then(|i| i.as_str()).unwrap_or("") {
                "path" => url = url.replace(&format!("{{{name}}}"), &ex),
                "query" => {
                    params.insert(name.to_string(), ex);
                }
                "header" => {
                    headers.insert(name.to_string(), ex);
                }
                _ => {}
            }
        }
    }

    let mut body: Option<Body> = None;
    if let Some(content) = op.get("requestBody").and_then(|b| b.get("content")) {
        if let Some(schema) = content
            .get("application/json")
            .and_then(|c| c.get("schema"))
        {
            body = Some(Body::Spec(BodySpec {
                json: Some(example_for(schema, r, &mut ctx)),
                ..Default::default()
            }));
        } else if let Some(schema) = content
            .get("application/x-www-form-urlencoded")
            .and_then(|c| c.get("schema"))
        {
            if let Value::Object(o) = example_for(schema, r, &mut ctx) {
                let form: IndexMap<String, String> = o
                    .into_iter()
                    .map(|(k, v)| (k, scalar_to_string(&v)))
                    .collect();
                body = Some(Body::Spec(BodySpec {
                    form: Some(form),
                    ..Default::default()
                }));
            }
        }
    }

    let name = op
        .get("operationId")
        .and_then(|o| o.as_str())
        .map(String::from)
        .unwrap_or_else(|| format!("{} {}", method.to_uppercase(), path));

    let ok_codes: Vec<i64> = op
        .get("responses")
        .and_then(|r| r.as_object())
        .map(|m| {
            m.keys()
                .filter_map(|k| k.parse::<i64>().ok())
                .filter(|c| (200..300).contains(c))
                .collect()
        })
        .unwrap_or_default();
    let assert = if ok_codes.is_empty() {
        Vec::new()
    } else {
        vec![Condition::Status {
            name: None,
            equals: None,
            one_of: Some(ok_codes),
            matches: None,
            on_failure: None,
        }]
    };

    for w in ctx.warnings {
        warnings.push(ConversionWarning {
            element: name.clone(),
            message: w,
        });
    }

    RequestStep {
        name: Some(name),
        method: Some(method.to_uppercase()),
        url,
        headers,
        params,
        body,
        assert,
        ..Default::default()
    }
}
