//! Compiles the vendored Prometheus remote-write and OTLP proto subsets with
//! `protox` (a pure-Rust protobuf compiler — no system `protoc` required) and
//! generates Rust types plus the OTLP gRPC client/server with
//! `tonic-prost-build`.

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let protos = [
        "proto/prometheus/types.proto",
        "proto/prometheus/remote.proto",
        "proto/opentelemetry/proto/common/v1/common.proto",
        "proto/opentelemetry/proto/resource/v1/resource.proto",
        "proto/opentelemetry/proto/metrics/v1/metrics.proto",
        "proto/opentelemetry/proto/collector/metrics/v1/metrics_service.proto",
    ];
    for proto in &protos {
        println!("cargo:rerun-if-changed={proto}");
    }

    let fds = protox::compile(protos, ["proto"])?;
    tonic_prost_build::configure()
        .build_client(true)
        // The server is generated for tests (a capturing OTLP collector).
        .build_server(true)
        .compile_fds(fds)?;
    Ok(())
}
