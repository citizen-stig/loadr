//! Postman collection (v2.x) → loadr plan.
//!
//! Walks the collection's `item` tree: folders become `Step::Group`, requests
//! become `RequestStep`. Postman `{{var}}` placeholders are rewritten to loadr
//! `${var}` interpolation.

use indexmap::IndexMap;
use serde_json::Value;

use loadr_config::{
    Body, BodySpec, Defaults, Dur, ExecutorKind, GroupStep, RequestStep, Scenario, Step, TestPlan,
};

use crate::{parse_contract, Conversion, ConversionWarning, GenError, GenOptions};

/// Generate a plan from a Postman collection (JSON).
pub fn gen_postman(source: &str, _opts: &GenOptions) -> Result<Conversion, GenError> {
    let root = parse_contract(source)?;
    let name = root
        .pointer("/info/name")
        .and_then(|n| n.as_str())
        .unwrap_or("postman collection")
        .to_string();

    let items = root
        .get("item")
        .and_then(|i| i.as_array())
        .ok_or_else(|| GenError::Postman("missing `item` (not a Postman collection?)".into()))?;

    let flow = walk_items(items);
    let n = count_requests(&flow);

    let mut scenarios = IndexMap::new();
    scenarios.insert(
        "collection".to_string(),
        Scenario {
            executor: ExecutorKind::ConstantVus,
            vus: Some(1),
            duration: Some(Dur::from_millis(60_000)),
            flow,
            ..Default::default()
        },
    );

    let plan = TestPlan {
        name: Some(name),
        description: Some(
            "Generated from a Postman collection by `loadr gen`. Review and set real load.".into(),
        ),
        defaults: Defaults::default(),
        scenarios,
        ..Default::default()
    };

    let warnings = vec![ConversionWarning {
        element: "scenario `collection`".into(),
        message: format!(
            "generated {n} request(s); Postman `{{{{var}}}}` became loadr `${{var}}` — set those via env/--var; defaulted to constant-vus 1 VU for 60s"
        ),
    }];

    Ok(Conversion { plan, warnings })
}

fn walk_items(items: &[Value]) -> Vec<Step> {
    let mut flow = Vec::new();
    for item in items {
        if let Some(sub) = item.get("item").and_then(|i| i.as_array()) {
            // A folder.
            flow.push(Step::Group(GroupStep {
                name: item
                    .get("name")
                    .and_then(|n| n.as_str())
                    .unwrap_or("group")
                    .to_string(),
                steps: walk_items(sub),
            }));
        } else if let Some(req) = item.get("request") {
            let name = item.get("name").and_then(|n| n.as_str()).map(String::from);
            flow.push(Step::Request(Box::new(build_request(name, req))));
        }
    }
    flow
}

fn build_request(name: Option<String>, req: &Value) -> RequestStep {
    let method = req
        .get("method")
        .and_then(|m| m.as_str())
        .unwrap_or("GET")
        .to_uppercase();
    let url = postman_url(req.get("url"));

    let mut headers: IndexMap<String, String> = IndexMap::new();
    if let Some(hs) = req.get("header").and_then(|h| h.as_array()) {
        for h in hs {
            if h.get("disabled").and_then(|d| d.as_bool()).unwrap_or(false) {
                continue;
            }
            let k = h.get("key").and_then(|k| k.as_str()).unwrap_or("");
            if !k.is_empty() {
                let v = h.get("value").and_then(|v| v.as_str()).unwrap_or("");
                headers.insert(k.to_string(), subst_vars(v));
            }
        }
    }

    let body = req.get("body").and_then(postman_body);

    RequestStep {
        name: name.or_else(|| Some(format!("{method} {url}"))),
        method: Some(method),
        url,
        headers,
        body,
        ..Default::default()
    }
}

fn postman_url(u: Option<&Value>) -> String {
    match u {
        Some(Value::String(s)) => subst_vars(s),
        Some(Value::Object(o)) => {
            if let Some(raw) = o.get("raw").and_then(|r| r.as_str()) {
                return subst_vars(raw);
            }
            let host = join_array(o.get("host"), ".");
            let path = join_array(o.get("path"), "/");
            subst_vars(&format!("{host}/{path}"))
        }
        _ => "/".to_string(),
    }
}

fn join_array(v: Option<&Value>, sep: &str) -> String {
    v.and_then(|x| x.as_array())
        .map(|a| {
            a.iter()
                .filter_map(|e| e.as_str())
                .collect::<Vec<_>>()
                .join(sep)
        })
        .unwrap_or_default()
}

fn postman_body(body: &Value) -> Option<Body> {
    match body.get("mode").and_then(|m| m.as_str())? {
        "raw" => body
            .get("raw")
            .and_then(|r| r.as_str())
            .map(|s| Body::Text(subst_vars(s))),
        "urlencoded" => {
            let form: IndexMap<String, String> = body
                .get("urlencoded")
                .and_then(|u| u.as_array())?
                .iter()
                .filter_map(|kv| {
                    Some((
                        kv.get("key")?.as_str()?.to_string(),
                        subst_vars(kv.get("value").and_then(|v| v.as_str()).unwrap_or("")),
                    ))
                })
                .collect();
            Some(Body::Spec(BodySpec {
                form: Some(form),
                ..Default::default()
            }))
        }
        _ => None,
    }
}

/// Postman `{{var}}` → loadr `${var}`.
fn subst_vars(s: &str) -> String {
    s.replace("{{", "${").replace("}}", "}")
}

fn count_requests(flow: &[Step]) -> usize {
    flow.iter()
        .map(|s| match s {
            Step::Request(_) => 1,
            Step::Group(g) => count_requests(&g.steps),
            _ => 0,
        })
        .sum()
}
