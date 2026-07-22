//! Tests for the recorder that don't need a live network: the HAR→scenario
//! render (with auto-correlation) and the CA minting per-host TLS configs.

use loadr_record::ca::Ca;
use loadr_record::har::{Captured, Recording};
use loadr_record::{render, Emit};

fn cap(method: &str, url: &str, req_body: Option<&str>, resp_body: &str) -> Captured {
    Captured {
        method: method.into(),
        url: url.into(),
        http_version: "HTTP/1.1".into(),
        req_headers: vec![("accept".into(), "*/*".into())],
        req_body: req_body.map(|b| ("application/json".into(), b.into())),
        status: 200,
        status_text: "OK".into(),
        resp_headers: vec![("content-type".into(), "application/json".into())],
        resp_mime: "application/json".into(),
        resp_body: Some(resp_body.into()),
        wait_ms: 5.0,
    }
}

#[test]
fn render_scenario_auto_correlates_a_token() {
    let rec = Recording::new();
    // Login returns a token...
    rec.push(cap(
        "POST",
        "http://api.test/login",
        Some("{}"),
        r#"{"token":"3f2504e0-4f89-41d3-9a0c-0305e82c3301"}"#,
    ));
    // ...which a later request reuses in its body.
    rec.push(cap(
        "POST",
        "http://api.test/order",
        Some(r#"{"token":"3f2504e0-4f89-41d3-9a0c-0305e82c3301","qty":1}"#),
        r#"{"ok":true}"#,
    ));
    assert_eq!(rec.len(), 2);

    let (yaml, warnings) = render(&rec, Emit::Scenario).expect("render scenario");
    // The producing request grew an extract, and the consumer a substitution.
    assert!(yaml.contains("extract"), "expected an extract:\n{yaml}");
    assert!(
        yaml.contains("${token}"),
        "expected a token substitution:\n{yaml}"
    );
    assert!(
        warnings.iter().any(|w| w.contains("auto-correlated")),
        "expected an auto-correlation note, got: {warnings:?}"
    );
}

#[test]
fn render_har_is_valid_json_har() {
    let rec = Recording::new();
    rec.push(cap("GET", "http://api.test/health", None, r#"{"ok":true}"#));
    let (har, _) = render(&rec, Emit::Har).expect("render har");
    let v: serde_json::Value = serde_json::from_str(&har).expect("valid json");
    assert_eq!(v["log"]["entries"][0]["request"]["method"], "GET");
    assert_eq!(
        v["log"]["entries"][0]["request"]["url"],
        "http://api.test/health"
    );
}

#[test]
fn ca_mints_per_host_server_configs() {
    let _ = rustls::crypto::ring::default_provider().install_default();
    let dir = std::env::temp_dir().join(format!("loadr-rec-test-{}", std::process::id()));
    let ca = Ca::load_or_create(&dir).expect("create ca");
    assert!(ca.cert_pem().contains("BEGIN CERTIFICATE"));
    // Minting succeeds and is cached (same Arc on second call).
    let a = ca.server_config_for("example.com").expect("mint");
    let b = ca.server_config_for("example.com").expect("cached");
    assert!(std::sync::Arc::ptr_eq(&a, &b));
    ca.server_config_for("other.test").expect("mint another");
    let _ = std::fs::remove_dir_all(&dir);
}
