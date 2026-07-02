# Datadog plugin

`loadr-plugin-datadog` is a **native output plugin**: it streams a run's
metrics into [Datadog](https://www.datadoghq.com/) so a live test shows up on
your existing dashboards and monitors. It is not built into loadr core — install
it, then add it to a plan's `outputs:` list.

The plugin talks to the **Datadog v2 series HTTP API**
(`POST /api/v2/series`) directly over [`hyper`](https://hyper.rs/), authenticated
with an API key in the `DD-API-KEY` header. There is **no `dd-trace`, no Datadog
Agent, and no StatsD hop** — the plugin batches each one-second snapshot into a
series payload and ships it straight to Datadog's intake. Because the whole path
is plain HTTP, it is fully buildable today with no native Datadog SDK.

It follows the same `start` / `on_snapshot` / `finish` lifecycle as the shipped
[`native-output` example](native.md), so if you have read that plugin this one
will feel familiar.

> **Status:** planned. The design is fixed and the transport is pure `hyper`, but
> the plugin is not part of a published release yet. Track it before depending on
> it in CI.

## Build and install

```bash
cargo build -p loadr-plugin-datadog --release

# `loadr plugin install` copies a directory that holds plugin.toml next to the
# artifact named by its `entry`. Stage the built cdylib beside the manifest:
mkdir -p dist
cp plugins/loadr-plugin-datadog/plugin.toml dist/
cp target/release/libloadr_plugin_datadog.so dist/   # .dylib on macOS, .dll on Windows
loadr plugin install dist
loadr plugin info datadog
```

Installing copies `plugin.toml` and the artifact into `~/.loadr/plugins/datadog/`
(override with `LOADR_PLUGINS_DIR` or `--plugins-dir`). The manifest declares an
output plugin:

```toml
[plugin]
name = "datadog"
kind = "output"
type = "native"
entry = "libloadr_plugin_datadog.so"
description = "Batches snapshot series into the Datadog v2 series HTTP API"
```

## Use it in a test

An output plugin is wired in through the plan's `outputs:` list as a
`type: plugin` entry, naming the installed plugin and passing its `config`
straight through to `start`:

```yaml
name: checkout-load

outputs:
  - type: plugin
    name: datadog
    config:
      api_key: "${env.DD_API_KEY}"   # never hard-code the key
      site: datadoghq.eu

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
or a `json` archive next to the Datadog export, for example. The plugin's simple
form is also reachable from the CLI with `--output datadog` when the `config`
lives in the plan.

## Config reference

The object under `config:` is handed to the plugin's `start` as JSON.

| Key | Type | Default | Meaning |
|---|---|---|---|
| `api_key` | string | — (**required**) | Datadog API key, sent as the `DD-API-KEY` header. Read it from the environment (`${env.DD_API_KEY}`); a missing or empty key fails `start`. |
| `site` | string | `datadoghq.com` | Datadog site the intake lives on. Selects the host — e.g. `datadoghq.eu` posts to `https://api.datadoghq.eu/api/v2/series`. Use `us3.datadoghq.com`, `us5.datadoghq.com`, `ap1.datadoghq.com`, or `ddog-gov.com` for the other regions. |
| `prefix` | string | `loadr.` | Prepended to every metric name, so a trend such as `http_req_duration` arrives as `loadr.http_req_duration`. |
| `tags` | list of strings | `[]` | Extra Datadog tags (`key:value`) attached to every point, merged with the run's own tags. Handy for `env:staging` or `service:checkout`. |

`${env.…}` and other interpolation resolve before the config reaches the plugin,
so secrets stay out of the plan file.

## What gets sent

Each one-second **snapshot** (`on_snapshot`) is converted into a Datadog v2
series batch and POSTed in a single request:

- **counters** (e.g. `http_reqs`) become Datadog `count` points;
- **gauges** (e.g. active VUs) become `gauge` points;
- **trends** (e.g. `http_req_duration`) are emitted as gauge points per
  published quantile — `…​.p95`, `…​.p99`, `…​.avg`, `…​.max` — matching how the
  `prometheus` output shapes trends.

`start` opens the `hyper` client and validates the config; `finish` flushes any
buffered final snapshot and the end-of-run summary so no trailing second is lost.
Points carry the run's `run_id` as a tag so concurrent runs stay separable on a
shared dashboard.

## Metrics

The plugin reports its own health back into the run so an export problem is
visible in loadr's summary rather than silently dropping data:

| Metric | Kind | Meaning |
|---|---|---|
| `datadog_points_sent` | counter | Total series points accepted by the intake |
| `datadog_flush_errors` | counter | Snapshot flushes that failed (HTTP error, timeout, or a non-2xx from the API) |

A healthy run shows `datadog_points_sent` climbing once per second and
`datadog_flush_errors` at zero; a persistently rising `datadog_flush_errors`
usually means a bad `api_key`, the wrong `site`, or blocked egress to Datadog.

## Notes

- **The key is a secret.** Pull it from the environment (`${env.DD_API_KEY}`) or
  a secret store; never commit it in a plan.
- **Match the site to the key.** A key issued for the EU org will be rejected by
  the US intake (and vice versa) — a `403` shows up as `datadog_flush_errors`.
- **Fire-and-batch, not blocking.** A flush failure is counted and the run
  continues; the plugin does not stall the load generator waiting on Datadog, and
  a transient error does not fail the test.
- **Ingestion cost.** Every snapshot second is a billable series submission —
  keep `prefix`/`tags` tight and lean on `thresholds` for pass/fail rather than
  querying Datadog in CI.
- For end-of-run gating in CI, prefer `--summary-export results.json` and
  loadr's own `thresholds`; use the Datadog export for the live and historical
  view, not the exit code.
