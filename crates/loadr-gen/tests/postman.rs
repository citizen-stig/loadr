//! Tests for Postman collection generation. Pure (no network).

use loadr_gen::{gen_postman, GenOptions};

const COLLECTION: &str = r##"
{
  "info": { "name": "Shop API", "schema": "https://schema.getpostman.com/json/collection/v2.1.0/collection.json" },
  "item": [
    {
      "name": "auth",
      "item": [
        {
          "name": "login",
          "request": {
            "method": "POST",
            "url": { "raw": "{{baseUrl}}/login" },
            "header": [{ "key": "Content-Type", "value": "application/json" }],
            "body": { "mode": "raw", "raw": "{\"user\":\"alice\"}" }
          }
        }
      ]
    },
    {
      "name": "getProfile",
      "request": {
        "method": "GET",
        "url": "{{baseUrl}}/profile/{{userId}}",
        "header": [{ "key": "Authorization", "value": "Bearer {{token}}" }]
      }
    }
  ]
}
"##;

#[test]
fn folders_become_groups_and_vars_are_rewritten() {
    let c = gen_postman(COLLECTION, &GenOptions::default()).expect("generate");
    assert_eq!(c.plan.name.as_deref(), Some("Shop API"));
    let s = c
        .plan
        .scenarios
        .get("collection")
        .expect("collection scenario");
    // Top-level flow: one Group (auth) + one request (getProfile).
    assert_eq!(s.flow.len(), 2);

    let yaml = serde_yaml::to_string(&c.plan).unwrap();
    // Postman {{var}} → loadr ${var}
    assert!(yaml.contains("${baseUrl}/login"), "var rewrite:\n{yaml}");
    assert!(yaml.contains("${token}"), "header var:\n{yaml}");
    assert!(!yaml.contains("{{"), "no postman vars left:\n{yaml}");
    // group nesting present
    assert!(
        yaml.contains("group:") || yaml.contains("steps:"),
        "group:\n{yaml}"
    );
}

#[test]
fn postman_plan_is_valid() {
    let c = gen_postman(COLLECTION, &GenOptions::default()).unwrap();
    let yaml = serde_yaml::to_string(&c.plan).unwrap();
    loadr_config::load_str(&yaml, &loadr_config::LoadOptions::new())
        .unwrap_or_else(|e| panic!("postman plan failed validation: {e}\n---\n{yaml}"));
}

#[test]
fn not_a_collection_is_an_error() {
    assert!(gen_postman("{\"foo\":1}", &GenOptions::default()).is_err());
}
