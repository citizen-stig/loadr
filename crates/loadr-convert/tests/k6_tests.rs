//! Integration tests for the k6 script converter.

use loadr_config::{
    Body, Condition, ExecutorKind, MetricKindSpec, Severity, Step, TestPlan, ThinkTimeSpec,
    ThresholdEntry, ThresholdList,
};
use loadr_convert::{convert_k6, ConvertError};

const SIMPLE: &str = include_str!("data/simple_k6.js");
const SCENARIOS: &str = include_str!("data/scenarios_k6.js");
const FULL: &str = include_str!("data/full_k6.js");

fn assert_no_validation_errors(plan: &TestPlan) {
    let diags = loadr_config::validate(plan, None, &Default::default());
    let errors: Vec<_> = diags
        .iter()
        .filter(|d| d.severity == Severity::Error)
        .collect();
    assert!(errors.is_empty(), "validation errors: {errors:#?}");
}

fn assert_round_trips(plan: &TestPlan) {
    let yaml = serde_yaml::to_string(plan).expect("serialize plan");
    let loaded = loadr_config::load_str(&yaml, &loadr_config::LoadOptions::new())
        .unwrap_or_else(|e| panic!("round-trip failed: {e}\n---\n{yaml}"));
    assert_eq!(loaded.plan.scenarios.len(), plan.scenarios.len());
}

#[test]
fn simple_script_converts() {
    let conv = convert_k6(SIMPLE).expect("convert");
    let plan = &conv.plan;

    let scenario = plan.scenarios.get("default").expect("default scenario");
    assert_eq!(scenario.executor, ExecutorKind::ConstantVus);
    assert_eq!(scenario.vus, Some(10));
    assert_eq!(scenario.duration, Some(loadr_config::Dur::from_secs(30)));

    assert_eq!(scenario.flow.len(), 2, "flow: {:?}", scenario.flow);
    let req = match &scenario.flow[0] {
        Step::Request(r) => r,
        other => panic!("expected request, got {other:?}"),
    };
    assert_eq!(req.url, "https://test.example.com/api/users");
    assert_eq!(req.method.as_deref(), Some("GET"));
    assert_eq!(req.name.as_deref(), Some("/api/users"));

    // check() entries become checks on the preceding request.
    assert_eq!(req.checks.len(), 3);
    match &req.checks[0] {
        Condition::Status { name, equals, .. } => {
            assert_eq!(name.as_deref(), Some("status is 200"));
            assert_eq!(*equals, Some(200));
        }
        other => panic!("expected status check, got {other:?}"),
    }
    match &req.checks[1] {
        Condition::BodyContains { value, .. } => assert_eq!(value, "users"),
        other => panic!("expected body_contains check, got {other:?}"),
    }
    match &req.checks[2] {
        Condition::Duration { max, .. } => {
            assert_eq!(*max, loadr_config::Dur::from_millis(500));
        }
        other => panic!("expected duration check, got {other:?}"),
    }

    match &scenario.flow[1] {
        Step::ThinkTime(ThinkTimeSpec::Constant { duration }) => {
            assert_eq!(*duration, loadr_config::Dur::from_secs(1));
        }
        other => panic!("expected think_time, got {other:?}"),
    }

    // Thresholds pass through.
    assert_eq!(plan.thresholds.len(), 2);
    assert!(plan.thresholds.contains_key("http_req_duration"));
    assert!(plan.thresholds.contains_key("http_req_failed"));

    // Everything converted -> no JS module needed.
    assert!(plan.js.is_none(), "js: {:?}", plan.js);

    assert_no_validation_errors(plan);
    assert_round_trips(plan);
}

#[test]
fn scenarios_script_converts() {
    let conv = convert_k6(SCENARIOS).expect("convert");
    let plan = &conv.plan;

    assert_eq!(plan.scenarios.len(), 2);

    let browse = plan.scenarios.get("browse").expect("browse");
    assert_eq!(browse.executor, ExecutorKind::RampingVus);
    assert_eq!(browse.start_vus, Some(0));
    assert_eq!(browse.stages.len(), 3);
    assert_eq!(browse.stages[0].duration, loadr_config::Dur::from_secs(60));
    assert_eq!(browse.stages[0].target, 20.0);
    assert_eq!(browse.stages[2].target, 0.0);
    assert_eq!(
        browse.graceful_ramp_down,
        Some(loadr_config::Dur::from_secs(10))
    );
    // exec function was converted to a flow; exec is cleared.
    assert_eq!(browse.exec, None);
    assert_eq!(browse.flow.len(), 2);
    let req = match &browse.flow[0] {
        Step::Request(r) => r,
        other => panic!("expected request, got {other:?}"),
    };
    // `${__ENV.BASE_URL}` template -> loadr `${env.BASE_URL}`.
    assert_eq!(req.url, "${env.BASE_URL}/products");
    match &browse.flow[1] {
        Step::ThinkTime(ThinkTimeSpec::Uniform { min, max }) => {
            assert_eq!(*min, loadr_config::Dur::from_secs(1));
            assert_eq!(*max, loadr_config::Dur::from_secs(3));
        }
        other => panic!("expected uniform think_time, got {other:?}"),
    }

    let api = plan.scenarios.get("api").expect("api");
    assert_eq!(api.executor, ExecutorKind::ConstantArrivalRate);
    assert_eq!(api.rate, Some(50.0));
    assert_eq!(api.duration, Some(loadr_config::Dur::from_secs(120)));
    assert_eq!(api.pre_allocated_vus, Some(10));
    assert_eq!(api.max_vus, Some(50));
    assert_eq!(api.start_time, Some(loadr_config::Dur::from_secs(10)));
    assert_eq!(api.time_unit, Some(loadr_config::Dur::from_secs(1)));
    let post = match &api.flow[0] {
        Step::Request(r) => r,
        other => panic!("expected request, got {other:?}"),
    };
    assert_eq!(post.method.as_deref(), Some("POST"));
    match post.body.as_ref() {
        Some(Body::Spec(spec)) => {
            assert_eq!(spec.json, Some(serde_json::json!({ "item": 42, "qty": 1 })));
        }
        other => panic!("expected json body, got {other:?}"),
    }
    assert_eq!(
        post.headers.get("Content-Type").map(String::as_str),
        Some("application/json")
    );

    // Thresholds: list with a detailed entry, plus a single-string entry.
    match plan.thresholds.get("http_req_duration") {
        Some(ThresholdList::Many(entries)) => {
            assert_eq!(entries.len(), 2);
            assert_eq!(entries[0].expression(), "p(95)<500");
            match &entries[1] {
                ThresholdEntry::Detailed {
                    threshold,
                    abort_on_fail,
                    delay_abort_eval,
                } => {
                    assert_eq!(threshold, "p(99)<1500");
                    assert!(abort_on_fail);
                    assert_eq!(*delay_abort_eval, Some(loadr_config::Dur::from_secs(30)));
                }
                other => panic!("expected detailed threshold, got {other:?}"),
            }
        }
        other => panic!("expected threshold list, got {other:?}"),
    }
    match plan.thresholds.get("checks") {
        Some(ThresholdList::Single(s)) => assert_eq!(s, "rate>0.95"),
        other => panic!("expected single threshold, got {other:?}"),
    }

    assert!(plan.js.is_none(), "js: {:?}", plan.js);
    assert_no_validation_errors(plan);
    assert_round_trips(plan);
}

#[test]
fn full_script_converts_with_js_fallback() {
    let conv = convert_k6(FULL).expect("convert");
    let plan = &conv.plan;

    // Custom metrics from k6/metrics constructors.
    assert_eq!(
        plan.metrics.get("login_time").map(|m| m.kind),
        Some(MetricKindSpec::Trend)
    );
    assert_eq!(
        plan.metrics.get("orders_placed").map(|m| m.kind),
        Some(MetricKindSpec::Counter)
    );

    // options.stages shorthand -> ramping-vus.
    let scenario = plan.scenarios.get("default").expect("default scenario");
    assert_eq!(scenario.executor, ExecutorKind::RampingVus);
    assert_eq!(scenario.stages.len(), 2);

    // Two groups from group() calls.
    assert_eq!(scenario.flow.len(), 2);
    let storefront = match &scenario.flow[0] {
        Step::Group(g) => g,
        other => panic!("expected group, got {other:?}"),
    };
    assert_eq!(storefront.name, "storefront");
    assert_eq!(storefront.steps.len(), 2);
    let home = match &storefront.steps[0] {
        Step::Request(r) => r,
        other => panic!("expected request, got {other:?}"),
    };
    // `r.status === 200 || r.status === 304` -> one_of.
    match &home.checks[0] {
        Condition::Status { one_of, .. } => {
            assert_eq!(one_of, &Some(vec![200, 304]));
        }
        other => panic!("expected status one_of check, got {other:?}"),
    }

    let order_group = match &scenario.flow[1] {
        Step::Group(g) => g,
        other => panic!("expected group, got {other:?}"),
    };
    assert_eq!(order_group.name, "order");
    let order = match &order_group.steps[0] {
        Step::Request(r) => r,
        other => panic!("expected request, got {other:?}"),
    };
    assert_eq!(order.checks.len(), 2);
    match &order.checks[0] {
        Condition::Status { equals, .. } => assert_eq!(*equals, Some(201)),
        other => panic!("expected status check, got {other:?}"),
    }
    // Unrecognized check body -> JS condition with the param renamed.
    match &order.checks[1] {
        Condition::Js { expression, .. } => {
            assert!(
                expression.contains("response.json('id')"),
                "expression: {expression}"
            );
        }
        other => panic!("expected js check, got {other:?}"),
    }

    // setup() + the unconverted `orders.add(1)` keep the original script around.
    let js = plan.js.as_ref().expect("js module preserved");
    let script = js.script.as_deref().expect("inline script");
    assert!(script.contains("export default function"));
    assert!(script.contains("export function setup()"));
    assert!(
        !script.contains("from 'k6"),
        "k6 imports should be stripped:\n{script}"
    );

    // Warnings: unconverted line, lifecycle copy, unrecognized jslib import.
    assert!(conv
        .warnings
        .iter()
        .any(|w| w.message.contains("orders.add(1)")));
    assert!(conv
        .warnings
        .iter()
        .any(|w| w.element.contains("setup()/teardown()")));
    assert!(conv
        .warnings
        .iter()
        .any(|w| w.element.contains("jslib.k6.io")));

    assert_no_validation_errors(plan);
    assert_round_trips(plan);
}

#[test]
fn script_without_default_function_fails() {
    let err = convert_k6("export const options = { vus: 1, duration: '10s' };")
        .expect_err("no default function");
    assert!(matches!(err, ConvertError::Js(_)), "got {err:?}");
}
