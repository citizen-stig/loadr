//! Integration tests for the JMeter `.jmx` converter.

use loadr_config::{
    Body, Condition, DataSource, ExecutorKind, Extractor, MatchIndex, Severity, Step, TestPlan,
    ThinkTimeSpec,
};
use loadr_convert::{convert_jmx, ConvertError};

const SIMPLE: &str = include_str!("data/simple.jmx");
const COMPLEX: &str = include_str!("data/complex.jmx");

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
fn simple_plan_converts() {
    let conv = convert_jmx(SIMPLE).expect("convert");
    let plan = &conv.plan;

    assert_eq!(plan.name.as_deref(), Some("Simple API Test"));
    assert_eq!(
        plan.variables.get("page_size"),
        Some(&serde_json::Value::String("25".into()))
    );

    // CSV data set with explicit variable names -> headerless CSV + warning.
    match plan.data.get("users-csv") {
        Some(DataSource::Csv {
            path, has_header, ..
        }) => {
            assert_eq!(path.to_string_lossy(), "users.csv");
            assert!(!has_header);
        }
        other => panic!("expected csv data source, got {other:?}"),
    }
    assert!(conv
        .warnings
        .iter()
        .any(|w| w.message.contains("no header row")));

    // The single sampler's scheme://host was hoisted into defaults.
    assert_eq!(
        plan.defaults.http.base_url.as_deref(),
        Some("https://api.example.com")
    );

    let scenario = plan.scenarios.get("users").expect("users scenario");
    assert_eq!(scenario.executor, ExecutorKind::PerVuIterations);
    assert_eq!(scenario.vus, Some(5));
    assert_eq!(scenario.iterations, Some(10));

    // Sampler-scoped constant timer becomes a think-time step before the request.
    assert_eq!(scenario.flow.len(), 2);
    match &scenario.flow[0] {
        Step::ThinkTime(ThinkTimeSpec::Constant { duration }) => {
            assert_eq!(*duration, loadr_config::Dur::from_millis(1000));
        }
        other => panic!("expected think_time, got {other:?}"),
    }
    let req = match &scenario.flow[1] {
        Step::Request(r) => r,
        other => panic!("expected request, got {other:?}"),
    };
    assert_eq!(req.name.as_deref(), Some("Get User"));
    assert_eq!(req.method.as_deref(), Some("GET"));
    assert_eq!(req.url, "/users");
    // ${username} -> data source ref, ${page_size} -> vars ref.
    assert_eq!(
        req.params.get("name").map(String::as_str),
        Some("${data.users-csv.username}")
    );
    assert_eq!(
        req.params.get("limit").map(String::as_str),
        Some("${vars.page_size}")
    );

    assert_eq!(req.assert.len(), 2);
    match &req.assert[0] {
        Condition::Status { equals, .. } => assert_eq!(*equals, Some(200)),
        other => panic!("expected status assertion, got {other:?}"),
    }
    match &req.assert[1] {
        Condition::Duration { max, .. } => {
            assert_eq!(*max, loadr_config::Dur::from_millis(500));
        }
        other => panic!("expected duration assertion, got {other:?}"),
    }

    // The listener is unsupported -> warning, not an error.
    assert!(conv
        .warnings
        .iter()
        .any(|w| w.element.contains("ResultCollector")));

    assert_no_validation_errors(plan);
    assert_round_trips(plan);
}

#[test]
fn complex_plan_converts() {
    let conv = convert_jmx(COMPLEX).expect("convert");
    let plan = &conv.plan;

    assert_eq!(plan.name.as_deref(), Some("Complex Shop Plan"));
    assert_eq!(
        plan.scenarios.len(),
        2,
        "scenarios: {:?}",
        plan.scenarios.keys()
    );

    // Plan-level header manager -> defaults.
    assert_eq!(
        plan.defaults.http.headers.get("Accept").map(String::as_str),
        Some("application/json")
    );
    assert_eq!(
        plan.defaults
            .http
            .headers
            .get("X-Source")
            .map(String::as_str),
        Some("jmeter")
    );

    // All samplers share http://shop.example.com (port 80 == default).
    assert_eq!(
        plan.defaults.http.base_url.as_deref(),
        Some("http://shop.example.com")
    );

    // Browse: loops forever + scheduler 300s + 60s ramp -> ramping-vus.
    let browse = plan.scenarios.get("browse-shop").expect("browse scenario");
    assert_eq!(browse.executor, ExecutorKind::RampingVus);
    assert_eq!(browse.stages.len(), 2);
    assert_eq!(browse.stages[0].duration, loadr_config::Dur::from_secs(60));
    assert_eq!(browse.stages[0].target, 20.0);
    assert_eq!(browse.stages[1].duration, loadr_config::Dur::from_secs(240));
    assert_eq!(browse.stages[1].target, 20.0);

    // Constant throughput timer: 120/min -> 2 iterations/s pacing.
    let pacing = browse.pacing.expect("pacing");
    assert!((pacing.iterations_per_second - 2.0).abs() < 1e-9);

    // Thread-group level uniform random timer -> scenario think time.
    match browse.think_time {
        Some(ThinkTimeSpec::Uniform { min, max }) => {
            assert_eq!(min, loadr_config::Dur::from_millis(1000));
            assert_eq!(max, loadr_config::Dur::from_millis(3000));
        }
        other => panic!("expected uniform think time, got {other:?}"),
    }

    // Transaction controller -> group; disabled sampler skipped.
    assert_eq!(browse.flow.len(), 1);
    let group = match &browse.flow[0] {
        Step::Group(g) => g,
        other => panic!("expected group, got {other:?}"),
    };
    assert_eq!(group.name, "Checkout");
    assert_eq!(group.steps.len(), 2);

    let cart = match &group.steps[0] {
        Step::Request(r) => r,
        other => panic!("expected request, got {other:?}"),
    };
    assert_eq!(cart.url, "/cart");
    assert_eq!(cart.headers.get("X-Cart").map(String::as_str), Some("1"));
    assert_eq!(cart.extract.len(), 4);
    match &cart.extract[0] {
        Extractor::Regex {
            name,
            group,
            default,
            ..
        } => {
            assert_eq!(name, "csrf");
            assert_eq!(*group, Some(1));
            assert_eq!(default.as_deref(), Some("NONE"));
        }
        other => panic!("expected regex extractor, got {other:?}"),
    }
    match &cart.extract[1] {
        Extractor::Jsonpath {
            name,
            expression,
            index,
            ..
        } => {
            assert_eq!(name, "item_id");
            assert_eq!(expression, "$.items[0].id");
            assert_eq!(*index, Some(MatchIndex::Random));
        }
        other => panic!("expected jsonpath extractor, got {other:?}"),
    }
    match &cart.extract[2] {
        Extractor::Boundary {
            name, left, right, ..
        } => {
            assert_eq!(name, "session");
            assert_eq!(left, "session=");
            assert_eq!(right, ";");
        }
        other => panic!("expected boundary extractor, got {other:?}"),
    }
    match &cart.extract[3] {
        Extractor::Xpath {
            name, expression, ..
        } => {
            assert_eq!(name, "page_title");
            assert_eq!(expression, "//title/text()");
        }
        other => panic!("expected xpath extractor, got {other:?}"),
    }

    // Substring assertion + NOT substring assertion.
    assert_eq!(cart.assert.len(), 2);
    match &cart.assert[0] {
        Condition::BodyContains { value, negate, .. } => {
            assert_eq!(value, "cart-items");
            assert!(!negate);
        }
        other => panic!("expected body_contains, got {other:?}"),
    }
    match &cart.assert[1] {
        Condition::BodyContains { value, negate, .. } => {
            assert_eq!(value, "error-banner");
            assert!(negate, "NOT flag should negate");
        }
        other => panic!("expected negated body_contains, got {other:?}"),
    }

    // Raw POST body with rewritten-free extractor references kept bare.
    let order = match &group.steps[1] {
        Step::Request(r) => r,
        other => panic!("expected request, got {other:?}"),
    };
    assert_eq!(order.url, "/orders");
    assert_eq!(order.method.as_deref(), Some("POST"));
    assert_eq!(order.follow_redirects, Some(false));
    match order.body.as_ref() {
        Some(Body::Text(body)) => {
            assert!(body.contains("${csrf}"), "body: {body}");
            assert!(body.contains("${item_id}"));
        }
        other => panic!("expected raw text body, got {other:?}"),
    }
    assert_eq!(order.assert.len(), 2);
    match &order.assert[0] {
        Condition::Jsonpath {
            expression, equals, ..
        } => {
            assert_eq!(expression, "$.status");
            assert_eq!(equals, &Some(serde_json::json!("confirmed")));
        }
        other => panic!("expected jsonpath assertion, got {other:?}"),
    }
    match &order.assert[1] {
        Condition::Xpath {
            expression, exists, ..
        } => {
            assert_eq!(expression, "//receipt");
            assert_eq!(*exists, Some(true));
        }
        other => panic!("expected xpath assertion, got {other:?}"),
    }

    // Second thread group: forever + scheduler, no ramp -> constant-vus.
    let health = plan
        .scenarios
        .get("health-checks")
        .expect("health scenario");
    assert_eq!(health.executor, ExecutorKind::ConstantVus);
    assert_eq!(health.vus, Some(10));
    assert_eq!(health.duration, Some(loadr_config::Dur::from_secs(120)));
    let health_req = match &health.flow[0] {
        Step::Request(r) => r,
        other => panic!("expected request, got {other:?}"),
    };
    assert_eq!(health_req.url, "/health");
    match &health_req.assert[0] {
        Condition::Size { max, .. } => assert_eq!(*max, Some(2048)),
        other => panic!("expected size assertion, got {other:?}"),
    }

    // Warnings for disabled + cookie clearing.
    assert!(conv
        .warnings
        .iter()
        .any(|w| w.message.contains("disabled") && w.element.contains("Old Endpoint")));
    assert!(conv
        .warnings
        .iter()
        .any(|w| w.message.contains("clear cookies")));

    assert_no_validation_errors(plan);
    assert_round_trips(plan);
}

#[test]
fn invalid_xml_is_an_error() {
    match convert_jmx("not xml at all <<<") {
        Err(ConvertError::Xml(_)) | Err(ConvertError::NotJmx(_)) => {}
        other => panic!("expected an XML/NotJmx error, got {other:?}"),
    }
}

#[test]
fn non_jmx_xml_is_rejected() {
    let err =
        convert_jmx("<?xml version=\"1.0\"?><other><thing/></other>").expect_err("should fail");
    assert!(matches!(err, ConvertError::NotJmx(_)), "got {err:?}");
}
