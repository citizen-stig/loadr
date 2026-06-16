# RabbitMQ plugin

`loadr-plugin-rabbitmq` adds **RabbitMQ** (AMQP 0.9.1) as a load-test target. It
is a **native protocol plugin**: RabbitMQ support is not built into loadr core —
the [`lapin`](https://github.com/amqp-rs/lapin) AMQP client ships only inside
this plugin's dynamic library. `lapin` is pure Rust (no C or system-library
dependencies), so the cdylib cross-compiles to every loadr release target; TLS
(`amqps://`) is wired to `rustls` only, never OpenSSL/`native-tls`. Once the
plugin is installed, a request to an `amqp://`, `amqps://` (or `rabbitmq://`)
URL routes straight to it.

The contract it uses is documented in
[Developing a plugin](developing.md#native-protocol-plugins).

## When to use

Reach for this when the thing under test *is* the broker: sizing a queue under
publish pressure, measuring end-to-end publish/consume latency, or finding the
ingest rate at which a consumer falls behind. For an application that merely
*uses* RabbitMQ behind an HTTP API, test the API with the `http` handler.

## Build and install

```bash
cargo build -p loadr-plugin-rabbitmq --release

# `loadr plugin install` copies a directory that holds plugin.toml next to the
# artifact named by its `entry`. Stage the built cdylib beside the manifest:
mkdir -p dist
cp plugins/loadr-plugin-rabbitmq/plugin.toml dist/
cp target/release/libloadr_plugin_rabbitmq.so dist/   # .dylib on macOS, .dll on Windows
loadr plugin install dist
loadr plugin info rabbitmq
```

Installing copies `plugin.toml` and the artifact into `~/.loadr/plugins/rabbitmq/`.
The manifest declares the URL schemes the plugin serves:

```toml
[plugin]
name = "rabbitmq"
kind = "protocol"
type = "native"
entry = "libloadr_plugin_rabbitmq.so"
schemes = ["amqp", "amqps", "rabbitmq"]
```

## The target URL

```
amqp://[user[:password]@]host[:port][/vhost]
```

- **scheme** is `amqp` (TLS variant `amqps`; alias `rabbitmq`).
- **port** defaults to the AMQP standard port (`5672`, or `5671` for `amqps`).
- **vhost** is URL-encoded in the path; the default vhost `/` is written `%2f`,
  e.g. `amqp://loadr:loadr@host:5672/%2f`.

## Use it in a test

List the plugin under `plugins:` and target an `amqp://` URL. The operation is
described by the request's `plugin:` block:

```yaml
plugins:
  - name: rabbitmq        # or: { name: rabbitmq, path: target/release/libloadr_plugin_rabbitmq.so }

scenarios:
  publish:
    executor: constant-vus
    vus: 5
    duration: 15s
    flow:
      - request:
          name: publish job
          url: amqp://loadr:loadr@host:5672/%2f
          plugin:
            operation: publish
            routing_key: loadr.work    # default exchange routes by queue name
            queue: loadr.work
            declare_queue: true
            body: '{"vu": ${vu}}'
          assert:
            - { type: status, equals: 1 }   # 1 = ok, 0 = broker error

  consume:
    executor: constant-arrival-rate
    rate: 60
    duration: 15s
    pre_allocated_vus: 10
    max_vus: 40
    flow:
      - request:
          name: get job
          url: amqp://loadr:loadr@host:5672/%2f
          plugin:
            operation: get
            queue: loadr.work
            ack: true
```

A complete runnable plan is in `examples/32-rabbitmq.yaml`.

## Request options (`plugin:` block)

| Key             | Type    | Used by   | Notes |
|-----------------|---------|-----------|-------|
| `operation`     | string  | all       | `publish` or `get` |
| `exchange`      | string  | `publish` | Target exchange (default `""`, the default exchange) |
| `routing_key`   | string  | `publish` | Routing key; on the default exchange this is the queue name |
| `queue`         | string  | `get`     | Queue to consume from (falls back to `routing_key`) |
| `body`          | string  | `publish` | Message body; a JSON object/array is serialised compactly |
| `declare_queue` | bool    | both      | Declare a durable queue first (default `false`) |
| `ack`           | bool    | `get`     | Acknowledge the consumed message (default `true`) |

`${...}` placeholders inside any string leaf are interpolated by loadr before
the plugin runs, so values can reference VU state, variables, and data feeds.

A `get` against an empty queue is **not** an error: the request succeeds and
reports zero messages.

## Metrics

loadr turns the plugin's response into a dedicated metric family named after the
protocol (`rabbitmq`):

| Metric                 | Kind    | Meaning |
|------------------------|---------|---------|
| `rabbitmq_reqs`        | counter | One per operation |
| `rabbitmq_req_duration`| trend   | Operation latency (ms) |
| `rabbitmq_msgs`        | counter | Messages published (1) or consumed (0 or 1) |

A request is marked **failed** when the operation errors (response `status` 0).
`http_req_failed` therefore tracks the RabbitMQ failure rate too, and `checks` /
`assert` entries can gate on `status` (1 = ok).

## Connection pooling

The plugin keeps an internal pool of `lapin` connection + channel handles keyed
by the full connection URI, shared across every VU. The first request for a URI
opens one TCP connection and a multiplexed channel; all subsequent requests (any
VU) reuse it. The plugin owns a single Tokio runtime and `block_on`s the async
client, because the protocol ABI is synchronous and carries no per-VU context
across the FFI boundary.

## Testing against a real server

The example harness brings up `rabbitmq:3.13-management` with the `loadr` user
and a `loadr.work` queue pre-declared from `definitions.json`:

```bash
docker compose -f examples/harness/docker-compose.yml up -d rabbitmq

LOADR_TEST_AMQP_URL=amqp://loadr:loadr@127.0.0.1:5672/%2f \
  cargo test -p loadr-plugin-rabbitmq
```

The integration tests no-op when `LOADR_TEST_AMQP_URL` is unset.
