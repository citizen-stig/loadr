# MQTT plugin

> **Status:** planned — this page describes the intended design; the `mqtt`
> plugin is not yet in the signed [plugin index](installing.md).

`loadr-plugin-mqtt` adds **MQTT** as a load-test target. It is a **native
protocol plugin**: MQTT support is not built into loadr core — the
[`rumqttc`](https://github.com/bytebeamio/rumqtt) client ships only inside this
plugin's dynamic library. `rumqttc` is pure Rust with no native broker library
to link against (and TLS wired to `rustls`, never OpenSSL), so the cdylib
cross-compiles to every loadr release target, exactly like the
[`rabbitmq`](rabbitmq.md) plugin. Each request runs exactly **one operation** —
publish one message, or subscribe and receive one message — at a configurable
QoS, timed end to end. Once the plugin is installed, a request to an `mqtt://`
(or `mqtts://`) URL routes straight to it.

The contract it uses is documented in
[Developing a plugin](developing.md#native-protocol-plugins).

## When to use

Reach for this when the thing under test *is* the broker: sizing a broker under
publish pressure, measuring end-to-end publish/subscribe latency on a topic, or
finding the ingest rate at which subscribers fall behind. For an application
that merely *uses* MQTT behind an HTTP API, test the API with the built-in
`http` handler.

## Install

Once published, `mqtt` will ship in the signed [plugin index](installing.md), so
install it by name — no build toolchain required:

```bash
loadr plugin install mqtt
loadr plugin info mqtt
```

This resolves `mqtt` in the index, picks the artifact for your host target,
checks it against the plugin ABI your `loadr` build provides, downloads it,
verifies its sha256 and unpacks it into your plugins directory
(`~/.loadr/plugins/mqtt/`, or `$LOADR_PLUGINS_DIR`).

The installed manifest declares the URL schemes the plugin serves:

```toml
[plugin]
name = "mqtt"
kind = "protocol"
type = "native"
entry = "libloadr_plugin_mqtt.so"
schemes = ["mqtt", "mqtts"]
```

To run straight from a build tree instead, point the plan's `plugins:` entry at
the built artifact (`path: target/release/libloadr_plugin_mqtt.so`) rather than
resolving it by name.

## The target URL

```
mqtt://[user[:password]@]host[:port]
```

- **scheme** is `mqtt` (TLS variant `mqtts`).
- **port** defaults to the MQTT standard port (`1883`, or `8883` for `mqtts`).
- **user / password** — optional credentials passed to the broker on `CONNECT`.

## Use it in a test

List the plugin under `plugins:` and target an `mqtt://` URL. The operation is
described by the request's `plugin:` block:

```yaml
plugins:
  - name: mqtt        # or: { name: mqtt, path: target/release/libloadr_plugin_mqtt.so }

scenarios:
  publish:
    executor: constant-vus
    vus: 10
    duration: 15s
    flow:
      - request:
          name: publish reading
          url: mqtt://broker.example.com:1883
          plugin:
            operation: publish
            topic: sensors/${vu}/temperature
            qos: 1                                   # 0 | 1 | 2
            body: '{"vu": ${vu}, "iteration": ${iteration}}'
          assert:
            - { type: status, equals: 1 }            # 1 = ok, 0 = broker error

  subscribe:
    executor: constant-arrival-rate
    rate: 60
    duration: 15s
    pre_allocated_vus: 10
    max_vus: 40
    flow:
      - request:
          name: receive reading
          url: mqtt://broker.example.com:1883
          plugin:
            operation: subscribe
            topic: sensors/+/temperature
            qos: 1
          checks:
            - { type: status, equals: 1 }
            - { type: duration, name: delivery is fast, max: 250ms }

thresholds:
  checks: [ "rate>0.99" ]
  mqtt_req_duration: [ "p(95)<300ms" ]
  mqtt_reqs: [ "count>0" ]
```

A complete runnable plan will ship in `examples/44-mqtt.yaml`.

## Config reference (`plugin:` block)

| Key         | Type   | Used by     | Notes |
|-------------|--------|-------------|-------|
| `operation` | string | all         | `publish` or `subscribe` (required) |
| `topic`     | string | all         | Topic to publish to, or a topic filter to subscribe on (required) |
| `qos`       | int    | all         | Quality of Service: `0` at-most-once, `1` at-least-once, `2` exactly-once (default `0`) |
| `body`      | string | `publish`   | Message payload; a JSON object/array is serialised compactly |
| `retain`    | bool   | `publish`   | Set the MQTT retain flag on the published message (default `false`) |
| `timeout`   | string | `subscribe` | How long to wait for a message before failing (default: the request timeout) |

`${...}` placeholders inside any string leaf are interpolated by loadr before
the plugin runs, so topics, payloads and credentials can reference VU state,
variables and data feeds.

A `publish` at QoS 1 or 2 waits on the broker's **acknowledgement**
(`PUBACK` / `PUBCOMP`), so a message the broker never confirms surfaces as a
failed request rather than a silently-dropped one. At QoS 0 the request
succeeds once the packet is written to the socket. A `subscribe` returns the
first message delivered on the topic filter; the received payload is exposed on
the response body for assertions and extraction.

## Metrics

loadr turns the plugin's response into a dedicated metric family named after the
protocol (`mqtt`):

| Metric              | Kind    | Meaning |
|---------------------|---------|---------|
| `mqtt_reqs`         | counter | One per operation |
| `mqtt_req_duration` | trend   | Operation round-trip latency (ms) |

A request is marked **failed** when the operation errors (response `status` 0 —
a broker error, an unconfirmed publish, a subscribe that times out with no
message, a connection failure, or a timeout). `http_req_failed` therefore tracks
the MQTT failure rate too, and `checks` / `assert` entries can gate on `status`
(1 = ok).

```yaml
thresholds:
  checks: [ "rate>0.99" ]
  mqtt_req_duration: [ "p(95)<300ms" ]
```

## Notes

- **Connection pooling.** The plugin keeps an internal pool of `rumqttc`
  connection handles keyed by the full connection URI, shared across every VU.
  The first request for a URI opens one TCP connection and issues `CONNECT`; all
  subsequent requests (any VU) reuse it.
- **One message per request.** Each request is exactly one `publish` or one
  `subscribe` that receives a single message — there is no batching or
  long-lived subscription held across requests; steady load is expressed with an
  arrival-rate executor.
- **QoS is per request.** The `qos` key selects the delivery guarantee for that
  one operation, so a plan can mix at-most-once publishes and exactly-once
  subscribes side by side.
- **No native broker library.** `rumqttc` is pure Rust, so the plugin needs no C
  MQTT library and cross-compiles for all loadr release targets like the
  `rabbitmq` plugin.
- **Synchronous ABI.** The plugin owns a single Tokio runtime and `block_on`s
  the async MQTT client, because the protocol ABI is synchronous and carries no
  per-VU context across the FFI boundary.

## Testing against a real server

The example harness will bring up an MQTT broker (e.g. `eclipse-mosquitto`):

```bash
docker compose -f examples/harness/docker-compose.yml up -d mqtt

LOADR_TEST_MQTT_URL=mqtt://127.0.0.1:1883 \
  cargo test -p loadr-plugin-mqtt
```

The integration tests no-op when `LOADR_TEST_MQTT_URL` is unset.
