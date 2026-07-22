//! Tests for OpenAPI generation + the schema→example engine. All pure (no network).

use loadr_gen::{gen_openapi, GenOptions};

const PETSTORE: &str = r##"
openapi: 3.0.0
info: { title: Petstore, version: "1.0" }
servers:
  - url: https://api.petstore.test/v1
paths:
  /pets:
    get:
      operationId: listPets
      parameters:
        - { name: limit, in: query, schema: { type: integer, minimum: 10 } }
        - { name: X-Trace, in: header, schema: { type: string } }
      responses: { "200": { description: ok } }
    post:
      operationId: createPet
      requestBody:
        content:
          application/json:
            schema:
              type: object
              required: [name]
              properties:
                name: { type: string }
                tag:  { type: string }
                owner: { $ref: "#/components/schemas/Owner" }
      responses: { "201": { description: created } }
  /pets/{petId}:
    get:
      operationId: getPet
      parameters:
        - { name: petId, in: path, required: true, schema: { type: string } }
      responses: { "200": { description: ok }, "404": { description: missing } }
components:
  schemas:
    Owner:
      type: object
      properties:
        id: { type: string, format: uuid }
        friend: { $ref: "#/components/schemas/Owner" }
"##;

fn gen() -> loadr_gen::Conversion {
    gen_openapi(PETSTORE, &GenOptions::default()).expect("generate")
}

#[test]
fn one_request_per_operation_with_base_url() {
    let c = gen();
    let s = c.plan.scenarios.get("api").expect("api scenario");
    assert_eq!(s.flow.len(), 3, "3 operations => 3 requests");
    assert_eq!(
        c.plan.defaults.http.base_url.as_deref(),
        Some("https://api.petstore.test/v1")
    );
}

#[test]
fn params_and_path_templating_land_in_the_right_slots() {
    let yaml = serde_yaml::to_string(&gen().plan).unwrap();
    // query + header params captured
    assert!(yaml.contains("limit:"), "query param:\n{yaml}");
    assert!(yaml.contains("X-Trace:"), "header param:\n{yaml}");
    // path param example substituted into the url (no {petId} left)
    assert!(!yaml.contains("{petId}"), "path templated:\n{yaml}");
    // JSON body populated with the required key; the self-referential Owner terminated
    assert!(yaml.contains("name:"), "body populated:\n{yaml}");
    // declared 2xx codes become a status assertion
    assert!(
        yaml.contains("one_of") || yaml.contains("status"),
        "status assert:\n{yaml}"
    );
}

#[test]
fn generated_plan_passes_validation() {
    // The load-bearing invariant: what we emit is a valid loadr plan.
    let yaml = serde_yaml::to_string(&gen().plan).unwrap();
    loadr_config::load_str(&yaml, &loadr_config::LoadOptions::new())
        .unwrap_or_else(|e| panic!("generated plan failed validation: {e}\n---\n{yaml}"));
}

#[test]
fn include_exclude_select_a_subset() {
    let only_get = gen_openapi(
        PETSTORE,
        &GenOptions {
            include: vec!["get*".into()],
            ..Default::default()
        },
    )
    .unwrap();
    let s = only_get.plan.scenarios.get("api").unwrap();
    assert_eq!(
        s.flow.len(),
        1,
        "only operationId starting with 'get' => getPet"
    );
}

#[test]
fn missing_paths_is_an_error_not_a_panic() {
    let err = gen_openapi(
        "openapi: 3.0.0\ninfo: { title: x, version: '1' }\n",
        &GenOptions::default(),
    );
    assert!(err.is_err());
}

#[test]
fn fuzz_adds_gated_variants_and_stays_valid() {
    let c = gen_openapi(
        PETSTORE,
        &GenOptions {
            fuzz: true,
            ..Default::default()
        },
    )
    .unwrap();
    let s = c.plan.scenarios.get("api").unwrap();
    // createPet has a JSON body → fuzz variants appended beside the 3 base ops.
    assert!(
        s.flow.len() > 3,
        "fuzz should add variants, got {}",
        s.flow.len()
    );

    let yaml = serde_yaml::to_string(&c.plan).unwrap();
    assert!(yaml.contains("[fuzz:"), "variant names present:\n{yaml}");
    assert!(yaml.contains("^[234]..$"), "no-5xx gate present:\n{yaml}");
    // The fuzz plan is still a valid, runnable loadr plan.
    loadr_config::load_str(&yaml, &loadr_config::LoadOptions::new())
        .unwrap_or_else(|e| panic!("fuzz plan failed validation: {e}"));
}
