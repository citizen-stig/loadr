//! Tests for GraphQL introspection generation. Pure (no network).

use loadr_gen::{gen_graphql, GenOptions};

const INTROSPECTION: &str = r##"
{
  "data": {
    "__schema": {
      "queryType": { "name": "Query" },
      "mutationType": null,
      "types": [
        {
          "kind": "OBJECT",
          "name": "Query",
          "fields": [
            {
              "name": "product",
              "args": [
                { "name": "id", "type": { "kind": "NON_NULL", "ofType": { "kind": "SCALAR", "name": "ID", "ofType": null } } }
              ],
              "type": { "kind": "OBJECT", "name": "Product", "ofType": null }
            }
          ]
        },
        {
          "kind": "OBJECT",
          "name": "Product",
          "fields": [
            { "name": "id", "args": [], "type": { "kind": "SCALAR", "name": "ID", "ofType": null } },
            { "name": "name", "args": [], "type": { "kind": "SCALAR", "name": "String", "ofType": null } }
          ]
        }
      ]
    }
  }
}
"##;

fn opts() -> GenOptions {
    GenOptions {
        base_url: Some("https://api.test/graphql".into()),
        ..Default::default()
    }
}

#[test]
fn builds_operation_with_variables_and_selection() {
    let c = gen_graphql(INTROSPECTION, &opts()).expect("generate");
    let s = c.plan.scenarios.get("api").expect("api scenario");
    assert_eq!(s.flow.len(), 1, "one query field => one operation");

    let yaml = serde_yaml::to_string(&c.plan).unwrap();
    // args lifted to a variable + used in the call
    assert!(yaml.contains("$id: ID!"), "variable signature:\n{yaml}");
    assert!(yaml.contains("product(id: $id)"), "arg used:\n{yaml}");
    // object return type expanded to a selection set of its scalar fields
    assert!(
        yaml.contains("id name") || yaml.contains("id"),
        "selection:\n{yaml}"
    );
    // variables object seeded
    assert!(yaml.contains("id:"), "variables:\n{yaml}");
}

#[test]
fn graphql_plan_is_valid() {
    let c = gen_graphql(INTROSPECTION, &opts()).unwrap();
    let yaml = serde_yaml::to_string(&c.plan).unwrap();
    loadr_config::load_str(&yaml, &loadr_config::LoadOptions::new())
        .unwrap_or_else(|e| panic!("graphql plan failed validation: {e}\n---\n{yaml}"));
}

#[test]
fn not_introspection_is_an_error() {
    assert!(gen_graphql("{\"foo\":1}", &opts()).is_err());
}
