# Custom gRPC Plugin Feeder Design

> **Status: implemented.** The `data_source` plugin capability and the
> `type: plugin` data source variant described below have shipped — see
> [Plugin-backed (on-demand) sources](src/yaml/data.md#plugin-backed-on-demand-sources)
> for the user-facing syntax and
> [Native data-source plugins](src/plugins/developing.md#native-data-source-plugins)
> for how to write one. The example plugin at
> `plugins/examples/native-data-source` implements exactly the `tx-signer`
> use case this document describes. See
> `docs/plugin-on-demand-data-implementation-plan.md` for the implementation
> plan this was built from.

## Summary

This document captures a proposed core extension for high-throughput gRPC load
tests where a request payload must be generated and cryptographically signed at
request time.

The motivating use case is a gRPC method whose request looks like this:

```proto
message SubmitRequest {
  bytes tx = 1;
}
```

The target service expects `tx` to contain a complete, time-sensitive,
Ed25519-signed payload. The payload cannot be pre-generated, and the signature
must land inside the protobuf message, not in gRPC metadata.

The recommended design is to add service/plugin-backed on-demand data sources
to loadr core. A native service plugin generates signed transaction bytes as
feeder output, and the existing built-in gRPC handler continues to encode and
send the request.

## Original Problem

The built-in gRPC handler is the right place to keep protocol behavior:

- async tonic-based request send/receive;
- per-VU channel reuse;
- descriptor resolution from proto files or server reflection;
- unary and streaming request shapes;
- gRPC metrics, timings, failures, and response handling.

A full native gRPC protocol plugin can functionally solve the signing problem,
but it has the wrong ownership boundary. It would need to reimplement the
transport layer, descriptor handling, channel reuse, streaming shapes, timeout
behavior, and failure accounting. The current native protocol ABI is also
synchronous, so async gRPC inside such a plugin usually requires a plugin-owned
runtime and `block_on`, which is not ideal for a load generator.

A metadata/header signer is also insufficient. The signature must be part of
the protobuf request payload, specifically `SubmitRequest.tx`.

## Current Constraints

Current loadr behavior and limitations:

- `DataSource` supports CSV, JSON, and inline rows only.
- `ServicePlugin` is lifecycle-only: `start(config)` and `stop()`.
- The built-in gRPC handler builds `DynamicMessage` values from `grpc.message`
  JSON and then sends them.
- `beforeRequest` can mutate URL, method, headers, and body, but it does not
  expose or mutate `grpc.message`.
- Existing docs already describe planned service-backed feeders, such as
  `docs/src/plugins/faker-gen.md` and `docs/src/plugins/sql-feeder.md`, but that
  capability is not implemented as a general core facility yet.

Because of this, a native service plugin cannot currently generate or replace a
built-in gRPC payload field on the request hot path without core changes.

## Design Direction

Add an on-demand service/plugin-backed data source capability.

The plugin remains a native service plugin at the manifest level:

```toml
[plugin]
name = "tx-sourcer"
version = "0.1.0"
kind = "service"
type = "native"
entry = "libtx_sourcer.so"
description = "Generates signed transaction payloads for gRPC SubmitRequest"
capabilities = ["data_source"]
```

The test plan can then use it as a normal feeder:

```yaml
plugins:
  - name: tx-sourcer
    path: ./target/release/libtx_sourcer.dylib
    config:
      key_env: TX_SIGNING_KEY

data:
  signed_tx:
    type: plugin
    source: tx-sourcer
    config:
      chain_id: testnet-1

scenarios:
  submit:
    executor: constant-vus
    vus: 100
    duration: 5m
    flow:
      - request:
          name: submit tx
          protocol: grpc
          url: grpc://node:50051
          grpc:
            proto_files: [submit.proto]
            service: mempool.Submitter
            method: Submit
            message:
              tx: "${data.signed_tx.tx_b64}"
          checks:
            - { type: status, equals: 0 }
```

The plugin returns a row containing `tx_b64`. Protobuf JSON mapping represents
`bytes` fields as base64 strings, so the existing gRPC message serializer can set
`SubmitRequest.tx` normally.

## Core Changes Needed

### 1. Extend plan config

Add a plugin-backed data source variant to `loadr-config`:

```rust
Plugin {
    source: String,
    config: serde_json::Value,
    mode: DataMode,
    on_eof: OnEof,
    pick: PickStrategy,
}
```

`source` names a loaded service plugin with the `data_source` capability.

### 2. Add a native data-source capability

Keep `kind = "service"` but add an optional native capability interface,
separate from lifecycle `start/stop`.

Conceptual ABI:

```rust
pub trait FfiDataSource: Send + Sync {
    fn name(&self) -> RString;

    fn next_row(
        &self,
        request: FfiNextRowRequest,
    ) -> RResult<FfiNextRowResponse, RString>;
}
```

`FfiNextRowRequest` should include:

- data source name;
- plugin-level config;
- data-source-level config;
- VU id;
- iteration number;
- row index;
- scenario name;
- request name if available;
- wall-clock timestamp supplied by core.

`FfiNextRowResponse` should include:

- row fields as strings or JSON scalars;
- optional exhaustion marker for finite generators;
- optional plugin metrics counters.

### 3. Wire plugin-backed rows into `DataFeeds`

`DataFeeds::next_row` should support both in-memory feeds and plugin feeds. For
plugin feeds, it calls the native data-source capability on demand and returns
the row to interpolation.

The existing interpolation syntax remains unchanged:

```yaml
"${data.signed_tx.tx_b64}"
```

### 4. Keep gRPC transport unchanged

The built-in gRPC handler should continue to receive `grpc.message` as rendered
JSON. It should not need to know that `tx_b64` came from a native plugin.

This preserves:

- channel pooling;
- reflection/proto descriptor caching;
- request/response metrics;
- streaming behavior;
- timeout handling;
- in-flight request accounting.

## Performance and Stability Requirements

- The native data-source call is on the request hot path, so the capability must
  be `Send + Sync`.
- The plugin should load keys and static configuration during setup and avoid
  per-request disk or network I/O.
- Ed25519 signing should run inline as CPU work; no plugin-owned Tokio runtime is
  needed for the signing path.
- Core should treat plugin errors as request preparation failures, not panics.
- Missing plugin, wrong capability, or invalid config should fail before VUs
  start.
- Native plugins run in-process, so they must be trusted like other native
  dependencies.

## Agent and Distributed Runs

For agent mode, signer/data-source plugins should be installed locally on each
agent. A controller assignment that references a missing plugin or missing
capability should fail during assignment setup, before the synchronized start
barrier.

The controller should not ship native dynamic libraries inside assignments.

## Non-Goals

- Do not create a full native gRPC protocol plugin for this use case.
- Do not add a generic protobuf mutation hook in v1.
- Do not rely on JavaScript Ed25519 signing for the performance path.
- Do not require an external signer sidecar.
- Do not design a general async native plugin ABI as part of this feature.
- Do not transfer native plugin binaries from controller to agents.

## Acceptance Tests

- Config parses `data.<name>.type: plugin`.
- Validation fails when the referenced plugin is missing or lacks `data_source`.
- Native plugin API test loads a fake data-source service and pulls rows
  concurrently.
- Data feeder test confirms `${data.signed_tx.tx_b64}` invokes the plugin per
  row/request.
- gRPC integration test sends `SubmitRequest { tx }` where the server verifies
  the signed bytes.
- Plugin errors become request preparation failures and do not crash the run.
- Agent assignment fails cleanly when a referenced plugin data source is
  unavailable.
- Rust implementation runs `cargo fmt --all`.

## Design Default

Use plugin-backed data sources as the first core change because the plugin's
real job is to produce a signed byte field, not to own gRPC transport or mutate
protobuf internals. This keeps the extension point narrow and lets the existing
gRPC handler remain responsible for efficient async protocol behavior.

