# OTLP metrics plugin

`loadr-plugin-otlp-metrics` is a **native output plugin**: it encodes a run's
snapshot series as [OpenTelemetry](https://opentelemetry.io/) **OTLP metrics**
and posts them to any OTLP collector so a live test shows up alongside the rest
of your telemetry. It is not built into loadr core — install it, then add it to a
plan's `outputs:` list.

The plugin serialises each one-second snapshot into an
`ExportMetricsServiceRequest` using **prost-generated OTLP types**, then ships
the protobuf body over [`hyper`](https://hyper.rs/) with a
`Content-Type: application/x-protobuf` POST to the collector's
`/v1/metrics` endpoint (**OTLP/HTTP**, the 4318 port). There is **no OpenTelemetry
SDK and no `protoc`** in the build — the OTLP `.proto` files are compiled with
[`protox`](https://crates.io/crates/protox) at build time, matching loadr's
`protox`-not-`protoc` stance, so it is **fully buildable today** with a pure-Rust
toolchain.

It follows the same `start` / `on_snapshot` / `finish` lifecycle as the shipped
[`native-output` example](native.md), so if you have read that plugin this one
will feel familiar.

> **Status:** planned. The design is fixed and the transport is pure `hyper` over
> prost-generated OTLP types, but the plugin is not part of a published release
> yet. Track it before depending on it in CI.

## Build and install

```bash
cargo build -p loadr-plugin-otlp-metrics --release

# `loadr plugin install` copies a directory that holds plugin.toml next to the
# artifact named by its `entry`. Stage the built cdylib beside the manifest:
mkdir -p dist
cp plugins/loadr-plugin-otlp-metrics/plugin.toml dist/
cp target/release/libloadr_plugin_otlp_metrics.so dist/   # .dylib on macOS, .dll on Windows
loadr plugin install dist
loadr plugin info otlp-metrics
```

Installing copies `plugin.toml` and the artifact into
`~/.loadr/plugins/otlp-metrics/` (override with `LOADR_PLUGINS_DIR` or
`--plugins-dir`). The manifest declares an output plugin:

```toml
[plugin]
name = "otlp-metrics"
kind = "output"
type = "native"
entry = "libloadr_plugin_otlp_metrics.so"
description = "Encodes snapshot series as OTLP metrics (protobuf/HTTP) and posts them to a collector"
```

## Use it in a test

An output plugin is wired in through the plan's `outputs:` list as a
`type: plugin` entry, naming the installed plugin and passing its `config`
straight through to `start`:

```yaml
name: checkout-load

outputs:
  - type: plugin
    name: otlp-metrics
    config:
      endpoint: http://collector:4318

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
or a `json` archive next to the OTLP export, for example. The plugin's simple
form is also reachable from the CLI with `--out otlp-metrics` when the `config`
lives in the plan.

## Config reference

The object under `config:` is handed to the plugin's `start` as JSON.

| Key | Type | Default | Meaning |
|---|---|---|---|
| `endpoint` | string | — (**required**) | Base URL of the OTLP/HTTP collector, e.g. `http://collector:4318`. The plugin appends `/v1/metrics` unless the URL already ends in that path. A missing or malformed endpoint fails `start`. |
| `headers` | map of string→string | `{}` | Extra HTTP headers sent on every export — typically an auth header for a hosted collector, e.g. `{ Authorization: "Bearer ${env.OTLP_TOKEN}" }`. |
| `service_name` | string | `loadr` | Value of the `service.name` resource attribute on every exported metric, so the run is identifiable in the backend. |
| `resource_attributes` | map of string→string | `{}` | Extra OTLP resource attributes attached to every metric (e.g. `{ deployment.environment: staging }`), merged with the run's own tags. |

`${env.…}` and other interpolation resolve before the config reaches the plugin,
so tokens for a hosted collector stay out of the plan file.

## What gets sent

Each one-second **snapshot** (`on_snapshot`) is converted into an
`ExportMetricsServiceRequest`, encoded as protobuf, and POSTed in a single
request to `<endpoint>/v1/metrics`:

- **counters** (e.g. `http_reqs`) become OTLP monotonic `Sum` data points;
- **gauges** (e.g. active VUs) become OTLP `Gauge` data points;
- **trends** (e.g. `http_req_duration`) are emitted as gauge data points per
  published quantile — `…​.p95`, `…​.p99`, `…​.avg`, `…​.max` — matching how the
  `prometheus` output shapes trends.

`start` opens the `hyper` client and validates the config; `finish` flushes any
buffered final snapshot and the end-of-run summary so no trailing second is lost.
Every metric carries the run's `run_id` as a data-point attribute so concurrent
runs stay separable in a shared backend.

## Metrics

The plugin reports its own health back into the run so an export problem is
visible in loadr's summary rather than silently dropping data:

| Metric | Kind | Meaning |
|---|---|---|
| `otlp_datapoints_sent` | counter | Total OTLP data points accepted by the collector |
| `otlp_export_errors` | counter | Snapshot exports that failed (HTTP error, timeout, or a non-2xx from the collector) |

A healthy run shows `otlp_datapoints_sent` climbing once per second and
`otlp_export_errors` at zero; a persistently rising `otlp_export_errors` usually
means a wrong `endpoint`, a missing auth header, or blocked egress to the
collector.

```yaml
thresholds:
  otlp_export_errors: [ "count==0" ]
```

## Notes

- **Point at the OTLP/HTTP port.** OTLP/HTTP listens on `4318` by default; the
  `4317` gRPC port will not accept these protobuf POSTs. Give `endpoint` the base
  URL and let the plugin append `/v1/metrics`.
- **protobuf, not JSON.** The body is binary protobuf with
  `Content-Type: application/x-protobuf`; the collector must have its OTLP/HTTP
  receiver enabled (the default in the OpenTelemetry Collector).
- **Auth via `headers`.** Hosted collectors usually want a bearer token or API
  key — set it in `config.headers` from the environment, never hard-coded.
- **Fire-and-batch, not blocking.** An export failure is counted in
  `otlp_export_errors` and the run continues; the plugin does not stall the load
  generator waiting on the collector, and a transient error does not fail the
  test.
- For end-of-run gating in CI, prefer `--summary-export results.json` and loadr's
  own `thresholds`; use the OTLP export for the live and historical view, not the
  exit code.
</content>
</invoke>
