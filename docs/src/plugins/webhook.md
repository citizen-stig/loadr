# Webhook plugin

`loadr-plugin-webhook` is a **native output plugin** and the simplest possible
custom sink: it serialises each one-second snapshot and the end-of-run summary
as JSON and **POSTs** them to a URL you configure. It is not built into loadr
core — install it, then add it to a plan's `outputs:` list (or reach it from the
CLI with `--out webhook`).

The transport is nothing but plain HTTP over [`hyper`](https://hyper.rs/) — no
SDK, no client library, no message broker. Each POST carries a JSON body, your
optional static `headers`, and an optional HMAC signature so the receiver can
verify the payload came from your run. If you can stand up an HTTP endpoint, you
have a metrics sink; it is the smallest thing that satisfies the `output`
contract, and a good starting point for building your own.

It follows the same `start` / `on_snapshot` / `finish` lifecycle as the shipped
[`native-output` example](native.md), so if you have read that plugin this one
will feel familiar.

> **Status:** planned. The design is fixed and the transport is pure `hyper`, but
> the plugin is not part of a published release yet. Track it before depending on
> it in CI.

## Build and install

```bash
cargo build -p loadr-plugin-webhook --release

# `loadr plugin install` copies a directory that holds plugin.toml next to the
# artifact named by its `entry`. Stage the built cdylib beside the manifest:
mkdir -p dist
cp plugins/loadr-plugin-webhook/plugin.toml dist/
cp target/release/libloadr_plugin_webhook.so dist/   # .dylib on macOS, .dll on Windows
loadr plugin install dist
loadr plugin info webhook
```

Installing copies `plugin.toml` and the artifact into `~/.loadr/plugins/webhook/`
(override with `LOADR_PLUGINS_DIR` or `--plugins-dir`). The manifest declares an
output plugin:

```toml
[plugin]
name = "webhook"
kind = "output"
type = "native"
entry = "libloadr_plugin_webhook.so"
description = "POSTs each snapshot and the summary as JSON to a configured URL"
```

## Use it in a test

An output plugin is wired in through the plan's `outputs:` list as a
`type: plugin` entry, naming the installed plugin and passing its `config`
straight through to `start`:

```yaml
name: checkout-load

outputs:
  - type: plugin
    name: webhook
    config:
      url: https://hooks.example.com/loadr
      headers:
        X-Source: loadr
        Authorization: "Bearer ${env.WEBHOOK_TOKEN}"   # never hard-code secrets
      hmac_secret: "${env.WEBHOOK_HMAC}"                # optional payload signing

scenarios:
  main:
    executor: constant-vus
    vus: 50
    duration: 10m
    flow:
      - request: { name: list, url: https://api.example.com/items }
      - request:
          name: checkout
          url: https://api.example.com/checkout
          method: POST
          checks: [ { type: status, equals: 200 } ]
```

You can run any number of outputs alongside it — a `prometheus` scrape endpoint
or a `json` archive next to the webhook export, for example. The plugin's simple
form is also reachable from the CLI with `--out webhook` when the `config` lives
in the plan.

## Config reference

The object under `config:` is handed to the plugin's `start` as JSON.

| Key | Type | Default | Meaning |
|---|---|---|---|
| `url` | string | — (**required**) | The endpoint each snapshot and the summary are POSTed to. Must be `http(s)://…`; a missing or malformed URL fails `start` so a typo is caught before the run rather than dropped silently. |
| `headers` | object (string → string) | `{}` | Static headers added to every request — e.g. `Authorization`, an API token, or a routing tag. Values interpolate (`${env.…}`), so pull secrets from the environment. `Content-Type: application/json` is always set. |
| `hmac_secret` | string | — | When set, each request is signed: the plugin computes `HMAC-SHA256(secret, body)` over the exact JSON bytes and sends it as the `X-Loadr-Signature` header (hex). Leave unset to POST unsigned. |
| `timeout` | duration | `5s` | Per-request timeout. A request that exceeds it is abandoned and counted as a delivery error; it never stalls the load generator. |

`${env.…}` and other interpolation resolve before the config reaches the plugin,
so secrets stay out of the plan file.

## What gets sent

Every request is a single POST with a JSON body and an `event` field naming the
payload kind:

- **`snapshot`** — one per second (`on_snapshot`): the run's live metric
  snapshot, the same one-second rollup the `prometheus` and `json` outputs see,
  carrying counters (`http_reqs`), gauges (active VUs) and trend quantiles
  (`http_req_duration` `p95`/`p99`/`avg`/`max`).
- **`summary`** — one at the end (`finish`): the full end-of-run summary,
  including threshold pass/fail, check rates and the aggregated trends.

Each body also carries the run's `run_id`, so a receiver aggregating several
concurrent runs can keep them separate. `start` opens the `hyper` client and
validates the config; `finish` flushes the final snapshot and the summary so no
trailing second is lost.

## Metrics

The plugin reports its own health back into the run so a delivery problem is
visible in loadr's summary rather than silently dropping data:

| Metric | Kind | Meaning |
|---|---|---|
| `webhook_deliveries` | counter | Requests the endpoint accepted (a 2xx response) |
| `webhook_delivery_errors` | counter | Requests that failed — a connection error, a timeout, or a non-2xx status |

A healthy run shows `webhook_deliveries` climbing once per second and
`webhook_delivery_errors` at zero; a persistently rising `webhook_delivery_errors`
usually means a bad `url`, a rejected auth header, or blocked egress to the
endpoint.

## Notes

- **Fire-and-forget, not blocking.** A delivery failure is counted and the run
  continues; the plugin does not stall the load generator waiting on your
  endpoint, and a transient error does not fail the test.
- **Keep secrets in the environment.** Pull tokens and the `hmac_secret` from
  `${env.…}` or a secret store; never commit them in a plan.
- **Verify with the HMAC.** When `hmac_secret` is set, recompute
  `HMAC-SHA256(secret, raw_body)` on the receiving side and compare it to
  `X-Loadr-Signature` before trusting a payload — that is what stops a spoofed
  POST from polluting your dashboards.
- **Your endpoint must be fast.** A snapshot arrives every second; a receiver
  slower than the `timeout` will show up as `webhook_delivery_errors`. Accept
  the POST and process it asynchronously rather than doing heavy work inline.
- For end-of-run gating in CI, prefer `--summary-export results.json` and
  loadr's own `thresholds`; use the webhook export for the live and historical
  view, not the exit code.
```
