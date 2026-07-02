# NATS plugin

> **Status:** planned — the design below is settled but `loadr-plugin-nats` is
> not yet in the [plugin index](installing.md). The URL scheme, `plugin:` block
> and metric names are documented here so plans can be written against the final
> shape.

`loadr-plugin-nats` adds **NATS** as a load-test target. It is a **native
protocol plugin**: NATS support is not built into loadr core. Like the
[`redis`](redis.md) plugin, it speaks the wire protocol directly — it talks the
**NATS line protocol** over a raw TCP socket (no C client, no `async-nats`
crate) — so every request is one exchange, timed end to end. It keeps an
internal `host:port` connection pool shared across every VU. The driver is pure
Rust, so the cdylib cross-compiles to every loadr release target. Once the
plugin is installed, a request to a `nats://` URL routes straight to it.

The contract it uses is documented in
[Developing a plugin](developing.md#native-protocol-plugins).

## When to use

Reach for this when the thing under test *is* the NATS server or a subscriber
behind it: sizing a subject under publish pressure, measuring request/reply
round-trip latency to a responder, or finding the publish rate at which a
consumer falls behind. For an application that merely *uses* NATS behind an HTTP
API, test the API with the `http` handler.

## Install

Once published, `nats` will ship in the signed [plugin index](installing.md),
so you install it by name — no build toolchain required:

```bash
loadr plugin install nats
loadr plugin info nats
```

This resolves `nats` in the index, picks the artifact for your host target,
checks it against the plugin ABI your `loadr` build provides, downloads it,
verifies its sha256 and unpacks it into your plugins directory
(`~/.loadr/plugins/nats/`, or `$LOADR_PLUGINS_DIR`).

The installed manifest declares the URL scheme the plugin serves:

```toml
[plugin]
name = "nats"
kind = "protocol"
type = "native"
entry = "libloadr_plugin_nats.so"
schemes = ["nats"]
```

To run straight from a build tree instead, point the plan's `plugins:` entry at
the built artifact (`path: target/release/libloadr_plugin_nats.so`) rather than
resolving it by name.

## The target URL

```
nats://[user[:password]@]host[:port]
```

- **scheme** must be `nats`.
- **port** defaults to the NATS standard port (`4222`) when omitted.
- **credentials** in the userinfo are sent in the `CONNECT` handshake when the
  server requires them.

## Use it in a test

List the plugin under `plugins:` and target a `nats://` URL. The operation is
described by the request's `plugin:` block — a `subject` plus an `operation`
of `publish` or `request`:

```yaml
plugins:
  - name: nats            # or: { name: nats, path: target/release/libloadr_plugin_nats.so }

scenarios:
  # Fire-and-forget publishers push messages onto a subject.
  publish:
    executor: constant-vus
    vus: 20
    duration: 15s
    flow:
      - request:
          name: publish event
          url: nats://msg.example.com:4222
          plugin:
            operation: publish
            subject: events.ingest
            body: '{"vu": ${vu}, "iteration": ${iteration}}'
          assert:
            - { type: status, equals: 0 }        # 0 = ok, 1 = protocol error

  # Request/reply against a responder, at a fixed rate.
  request_reply:
    executor: constant-arrival-rate
    rate: 200
    duration: 15s
    pre_allocated_vus: 30
    max_vus: 100
    flow:
      - request:
          name: ask service
          url: nats://msg.example.com:4222
          plugin:
            operation: request
            subject: rpc.echo
            body: "ping ${vu}"
          checks:
            - { type: status, equals: 0 }
            - { type: body_contains, value: pong }
            - { type: duration, name: reply is fast, max: 50ms }
```

## Request options (`plugin:` block)

| Key         | Type   | Used by   | Notes |
|-------------|--------|-----------|-------|
| `operation` | string | all       | `publish` or `request` (default `publish`) |
| `subject`   | string | all       | Subject to publish/request on (required) |
| `body`      | string | all       | Message payload; a JSON object/array is serialised compactly |
| `reply_to`  | string | `publish` | Optional reply subject set on a bare `PUB` |

`${...}` placeholders inside any string leaf are interpolated by loadr before
the plugin runs, so `subject`, `body` and `reply_to` can reference VU state,
variables, and data feeds.

## Operations, status and body

| `operation` | What it does | `status` | Body |
|-------------|--------------|----------|------|
| `publish`   | Sends one `PUB` and confirms the server accepted it | `0` on ack, `1` on `-ERR` | empty |
| `request`   | Sends a request and waits for the reply message | `0` on reply, `1` on error/timeout | the reply payload |

A `publish` **succeeds at the transport level** as soon as the server accepts
the message; there is no delivery guarantee to subscribers (core NATS is
at-most-once). A `request` succeeds only when a responder answers before the
request deadline — no responder (or a timeout) is a *failure* with `error` set
and no body.

## Metrics

loadr turns the plugin's response into a dedicated metric family named after the
protocol (`nats`):

| Metric              | Kind    | Meaning |
|---------------------|---------|---------|
| `nats_reqs`         | counter | One per operation (publish or request) |
| `nats_req_duration` | trend   | Operation round-trip latency (ms) |

A request is marked **failed** when the operation errors (a `-ERR` from the
server, a request timeout, or a connection failure). `http_req_failed`
therefore tracks the NATS failure rate too, and `checks` / `assert` entries can
gate on `status` (0 = ok).

```yaml
thresholds:
  checks: [ "rate>0.99" ]
  nats_req_duration: [ "p(95)<50ms" ]
  nats_reqs: [ "count>0" ]
```

## Notes

- **Connection pooling.** The plugin keeps an internal pool of live connections
  keyed by `host:port`, shared across every VU — modelled on the `redis`
  plugin's raw-socket pool. A request checks out an idle connection (running the
  `CONNECT`/`INFO` handshake on a fresh socket only), reuses it for the
  exchange, and returns it for the next caller, so concurrent VUs reuse a small
  set of sockets rather than reconnecting on every message. A connection left in
  an error state is dropped instead of returned, so the next caller
  transparently re-establishes it.
- **One exchange per request.** Each request is exactly one `publish` or one
  `request`/reply; there is no long-lived subscription or streaming (JetStream)
  inside a single request.
- **Synchronous ABI.** The plugin owns a single Tokio runtime and `block_on`s
  the async socket I/O, because the protocol ABI is synchronous and carries no
  per-VU context across the FFI boundary.
