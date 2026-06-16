# Apache Kafka plugin

`loadr-plugin-kafka` adds Apache Kafka as a load-test target. It is a **native
protocol plugin**: Kafka support is not built into loadr core — the Kafka client
ships only inside this plugin's dynamic library. Once the plugin is installed, a
request to a `kafka://` URL routes straight to it.

The client is [`rskafka`](https://crates.io/crates/rskafka), a **pure-Rust**
Kafka client. It pulls in no `librdkafka` / C toolchain, so the plugin
cross-compiles cleanly to every loadr release target (Linux gnu x64/arm64, macOS
x64/arm64, Windows MSVC). The contract it uses is documented in
[Developing a plugin](developing.md#native-protocol-plugins).

## Build and install

```bash
cargo build -p loadr-plugin-kafka --release

# `loadr plugin install` copies a directory that holds plugin.toml next to the
# artifact named by its `entry`. Stage the built cdylib beside the manifest:
mkdir -p dist
cp plugins/loadr-plugin-kafka/plugin.toml dist/
cp target/release/libloadr_plugin_kafka.so dist/   # .dylib on macOS, .dll on Windows
loadr plugin install dist
loadr plugin info kafka
```

Installing copies `plugin.toml` and the artifact into `~/.loadr/plugins/kafka/`.
The manifest declares the URL scheme the plugin serves:

```toml
[plugin]
name = "kafka"
kind = "protocol"
type = "native"
entry = "libloadr_plugin_kafka.so"
schemes = ["kafka"]
```

## Use it in a test

List the plugin under `plugins:` and target a `kafka://` URL. The broker is the
URL authority and the topic is the URL path (`kafka://broker:9092/topic`). The
operation is described by the request's `plugin:` block:

```yaml
plugins:
  - name: kafka            # or: { name: kafka, path: target/release/libloadr_plugin_kafka.so }

scenarios:
  producers:
    executor: constant-vus
    vus: 5
    duration: 15s
    flow:
      - request:
          name: produce event
          url: kafka://broker:9092/loadr-demo
          plugin:
            operation: produce
            key: "vu-${vu}"
            value: "event from vu ${vu} iter ${iteration}"
          assert:
            - { type: status, equals: 1 }      # 1 = ok, 0 = client error

  consumers:
    executor: constant-arrival-rate
    rate: 40
    duration: 15s
    pre_allocated_vus: 5
    max_vus: 20
    flow:
      - request:
          name: fetch from head
          url: kafka://broker:9092/loadr-demo
          plugin:
            operation: fetch
            offset: 0
            max_wait_ms: 500
```

A complete runnable plan is in `examples/31-kafka.yaml`.

## Request options (`plugin:` block)

| Key           | Type    | Used by   | Notes |
|---------------|---------|-----------|-------|
| `operation`   | string  | all       | `produce` or `fetch` |
| `topic`       | string  | all       | Defaults to the topic in the URL path |
| `partition`   | integer | all       | Defaults to `0` |
| `key`         | scalar  | `produce` | Optional record key (string/number/bool) |
| `value`       | scalar  | `produce` | Record value (string/number/bool) |
| `offset`      | integer | `fetch`   | Start offset (default `0`) |
| `max_bytes`   | integer | `fetch`   | Max bytes to return (default `1000000`) |
| `max_wait_ms` | integer | `fetch`   | Broker max wait, ms (default `500`) |

`${...}` placeholders inside any string leaf are interpolated by loadr before
the plugin runs, so values can reference VU state, variables, and data feeds.

## Metrics

loadr turns the plugin's response into a dedicated metric family named after the
protocol (`kafka`):

| Metric               | Kind    | Meaning |
|----------------------|---------|---------|
| `kafka_reqs`         | counter | One per operation |
| `kafka_req_duration` | trend   | Operation latency (ms) |
| `kafka_msgs`         | counter | Messages produced (1) / fetched (N) |

A request is marked **failed** when the operation errors (response `status` 0).
`http_req_failed` therefore tracks the Kafka failure rate too, and `checks` /
`assert` entries can gate on `status` (1 = ok).

## Connection pooling

The plugin keeps an internal pool of `rskafka` `Client` handles keyed by the
broker authority parsed from the URL, plus a per-`(broker, topic, partition)`
`PartitionClient` cache layered on top, all shared across every VU. The first
request for a broker establishes the connection and subsequent requests (any VU)
reuse it. The plugin owns a single Tokio runtime and `block_on`s the async
client, because the protocol ABI is synchronous and carries no per-VU context
across the FFI boundary.

Records are produced and fetched **uncompressed** (`NoCompression`): the C-backed
compression codecs in `rskafka` are disabled so the dependency tree stays
pure-Rust and cross-compilable.

## Testing against a real broker

The example harness brings up a single-node KRaft `apache/kafka:3.8.0` (no
ZooKeeper) and creates the `loadr-demo` topic via a one-shot `kafka-init`
container:

```bash
docker compose -f examples/harness/docker-compose.yml up -d kafka kafka-init

LOADR_TEST_KAFKA_URL=kafka://127.0.0.1:9092/loadr-demo \
  cargo test -p loadr-plugin-kafka
```

The integration tests no-op when `LOADR_TEST_KAFKA_URL` is unset.
