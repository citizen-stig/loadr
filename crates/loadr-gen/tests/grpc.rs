//! Tests for gRPC .proto generation. Compiles a temp proto (no network).

use loadr_gen::{gen_grpc, GenOptions};

const PROTO: &str = r#"
syntax = "proto3";
package greet;
message HelloRequest { string name = 1; int32 count = 2; }
message HelloReply { string message = 1; }
service Greeter {
  rpc SayHello(HelloRequest) returns (HelloReply);
}
"#;

#[test]
fn compiles_proto_enumerates_methods_and_builds_message() {
    let dir = std::env::temp_dir().join(format!("loadr-gen-grpc-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("greet.proto");
    std::fs::write(&path, PROTO).unwrap();

    let c = gen_grpc(
        &path,
        &GenOptions {
            base_url: Some("grpc://localhost:50051".into()),
            ..Default::default()
        },
    )
    .expect("gen grpc");

    let s = c.plan.scenarios.get("api").expect("api scenario");
    assert_eq!(s.flow.len(), 1, "one rpc => one request");

    let yaml = serde_yaml::to_string(&c.plan).unwrap();
    assert!(yaml.contains("greet.Greeter"), "service:\n{yaml}");
    assert!(yaml.contains("SayHello"), "method:\n{yaml}");
    // example request message built from the input descriptor
    assert!(yaml.contains("name:"), "message field name:\n{yaml}");
    assert!(yaml.contains("count:"), "message field count:\n{yaml}");
    // still a valid, runnable plan
    loadr_config::load_str(&yaml, &loadr_config::LoadOptions::new())
        .unwrap_or_else(|e| panic!("grpc plan failed validation: {e}\n---\n{yaml}"));

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn bad_proto_is_an_error_not_a_panic() {
    let path = std::env::temp_dir().join(format!("loadr-gen-bad-{}.proto", std::process::id()));
    std::fs::write(&path, "this is not valid proto").unwrap();
    assert!(gen_grpc(&path, &GenOptions::default()).is_err());
    let _ = std::fs::remove_file(&path);
}
