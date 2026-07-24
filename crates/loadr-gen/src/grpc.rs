//! gRPC `.proto` → loadr plan.
//!
//! Compiles the `.proto` in-process with `protox` (no `protoc` needed), then
//! enumerates every service method and emits a unary `RequestStep.grpc` with an
//! example request message derived from the method's input descriptor.

use std::path::{Path, PathBuf};

use indexmap::IndexMap;
use prost_reflect::{DescriptorPool, FieldDescriptor, Kind, MessageDescriptor};
use serde_json::{json, Map, Value};

use loadr_config::{
    Defaults, Dur, ExecutorKind, GrpcOptions, HttpDefaults, RequestStep, Scenario, Step, TestPlan,
};

use crate::{Conversion, ConversionWarning, GenError, GenOptions};

const MAX_DEPTH: usize = 6;

/// Generate a plan from a `.proto` file.
pub fn gen_grpc(proto: &Path, opts: &GenOptions) -> Result<Conversion, GenError> {
    let includes: Vec<PathBuf> = proto
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .map(|p| vec![p.to_path_buf()])
        .unwrap_or_default();

    let fds = protox::compile([proto], &includes)
        .map_err(|e| GenError::Grpc(format!("failed to compile {}: {e}", proto.display())))?;
    let pool = DescriptorPool::from_file_descriptor_set(fds)
        .map_err(|e| GenError::Grpc(format!("invalid descriptor set: {e}")))?;

    let proto_files = vec![proto.to_path_buf()];
    let endpoint = opts
        .base_url
        .clone()
        .unwrap_or_else(|| "grpc://localhost:50051".to_string());

    let mut flow: Vec<Step> = Vec::new();
    for service in pool.services() {
        for method in service.methods() {
            let message = example_message(&method.input(), 0);
            flow.push(Step::Request(Box::new(RequestStep {
                name: Some(format!("{}/{}", service.full_name(), method.name())),
                url: endpoint.clone(),
                grpc: Some(GrpcOptions {
                    proto_files: proto_files.clone(),
                    proto_includes: includes.clone(),
                    service: service.full_name().to_string(),
                    method: method.name().to_string(),
                    message: Some(message),
                    ..Default::default()
                }),
                ..Default::default()
            })));
        }
    }

    if flow.is_empty() {
        return Err(GenError::Grpc(
            "no services/methods found in the proto".into(),
        ));
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
        name: Some("grpc".to_string()),
        description: Some(
            "Generated from a .proto by `loadr gen`. Review and set real load.".into(),
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
            "generated {n} method(s); set the gRPC endpoint with --base-url (grpc://host:port) and real load"
        ),
    }];

    Ok(Conversion { plan, warnings })
}

fn example_message(desc: &MessageDescriptor, depth: usize) -> Value {
    if depth > MAX_DEPTH {
        return json!({});
    }
    let mut m = Map::new();
    for field in desc.fields() {
        let value = if field.is_list() {
            json!([field_example(&field, depth)])
        } else if field.is_map() {
            json!({})
        } else {
            field_example(&field, depth)
        };
        m.insert(field.name().to_string(), value);
    }
    Value::Object(m)
}

fn field_example(field: &FieldDescriptor, depth: usize) -> Value {
    match field.kind() {
        Kind::Double | Kind::Float => json!(0.0),
        Kind::Int32
        | Kind::Int64
        | Kind::Uint32
        | Kind::Uint64
        | Kind::Sint32
        | Kind::Sint64
        | Kind::Fixed32
        | Kind::Fixed64
        | Kind::Sfixed32
        | Kind::Sfixed64 => json!(0),
        Kind::Bool => json!(false),
        Kind::String => json!("string"),
        Kind::Bytes => json!(""),
        Kind::Message(d) => example_message(&d, depth + 1),
        Kind::Enum(e) => e
            .values()
            .next()
            .map(|v| json!(v.name()))
            .unwrap_or_else(|| json!(0)),
    }
}
