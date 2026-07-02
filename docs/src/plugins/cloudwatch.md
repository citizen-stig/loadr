# CloudWatch plugin

`loadr-plugin-cloudwatch` is a **native output plugin**: it streams a run's
metrics into [Amazon CloudWatch](https://aws.amazon.com/cloudwatch/) so a live
test shows up on your existing dashboards and alarms. It is not built into loadr
core — install it, then add it to a plan's `outputs:` list.

The plugin calls the **CloudWatch `PutMetricData` API** directly over HTTPS with
[`hyper`](https://hyper.rs/), authenticating each request with **SigV4** from a
pure-Rust signer. There is **no AWS SDK, no CloudWatch agent, and no StatsD
hop** — the plugin batches each one-second snapshot into a `PutMetricData`
payload, signs it, and ships it straight to the regional monitoring endpoint.
Because the whole path is plain signed HTTP, it is fully buildable today with no
native AWS SDK.

It follows the same `start` / `on_snapshot` / `finish` lifecycle as the shipped
[`native-output` example](native.md), so if you have read that plugin this one
will feel familiar.

> **Status:** planned. The design is fixed and the transport is pure `hyper` plus
> a pure-Rust SigV4 signer, but the plugin is not part of a published release
> yet. Track it before depending on it in CI.

## Build and install

```bash
cargo build -p loadr-plugin-cloudwatch --release

# `loadr plugin install` copies a directory that holds plugin.toml next to the
# artifact named by its `entry`. Stage the built cdylib beside the manifest:
mkdir -p dist
cp plugins/loadr-plugin-cloudwatch/plugin.toml dist/
cp target/release/libloadr_plugin_cloudwatch.so dist/   # .dylib on macOS, .dll on Windows
loadr plugin install dist
loadr plugin info cloudwatch
```

Installing copies `plugin.toml` and the artifact into
`~/.loadr/plugins/cloudwatch/` (override with `LOADR_PLUGINS_DIR` or
`--plugins-dir`). The manifest declares an output plugin:

```toml
[plugin]
name = "cloudwatch"
kind = "output"
type = "native"
entry = "libloadr_plugin_cloudwatch.so"
description = "Batches snapshot series into CloudWatch PutMetricData over signed HTTPS"
```

## Use it in a test

An output plugin is wired in through the plan's `outputs:` list as a
`type: plugin` entry, naming the installed plugin and passing its `config`
straight through to `start`:

```yaml
name: checkout-load

outputs:
  - type: plugin
    name: cloudwatch
    config:
      namespace: loadr
      region: eu-west-2

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
or a `json` archive next to the CloudWatch export, for example. The plugin's
simple form is also reachable from the CLI with `--output cloudwatch` when the
`config` lives in the plan.

## Credentials

The plugin signs with SigV4 and resolves credentials from the **standard AWS
environment chain** — `AWS_ACCESS_KEY_ID` / `AWS_SECRET_ACCESS_KEY` (plus
`AWS_SESSION_TOKEN` for temporary/STS credentials), and `AWS_REGION` as a
fallback when `config.region` is omitted. Nothing AWS-specific lives in the plan
file; grant the credentials `cloudwatch:PutMetricData` and keep them out of
source control.

## Config reference

The object under `config:` is handed to the plugin's `start` as JSON.

| Key | Type | Default | Meaning |
|---|---|---|---|
| `namespace` | string | `loadr` | CloudWatch namespace every metric is published under. Groups the run's metrics on the console and in alarms — e.g. `loadr`, or `loadr/checkout` for a per-service split. |
| `region` | string | `${AWS_REGION}` | AWS region whose endpoint receives the data — selects the host, e.g. `eu-west-2` posts to `https://monitoring.eu-west-2.amazonaws.com/`. Required when `AWS_REGION` is unset. |
| `dimensions` | map of string→string | `{}` | Extra CloudWatch dimensions attached to every metric (e.g. `{ env: staging, service: checkout }`), merged with the run's own tags. |

`${env.…}` and other interpolation resolve before the config reaches the plugin,
so anything you pull from the environment stays out of the plan file.

## What gets sent

Each one-second **snapshot** (`on_snapshot`) is converted into a `PutMetricData`
batch, signed with SigV4, and POSTed in a single request:

- **counters** (e.g. `http_reqs`) become CloudWatch metrics with `Count` units;
- **gauges** (e.g. active VUs) become plain value metrics;
- **trends** (e.g. `http_req_duration`) are emitted per published quantile —
  `…​.p95`, `…​.p99`, `…​.avg`, `…​.max` — matching how the `prometheus` output
  shapes trends.

`start` initialises the `hyper` client, resolves credentials, and validates the
config; `finish` flushes any buffered final snapshot and the end-of-run summary
so no trailing second is lost. Every metric carries the run's `run_id` as a
dimension so concurrent runs stay separable on a shared dashboard. Batches
respect CloudWatch's `PutMetricData` limit and are split across requests when a
snapshot carries more series than one call allows.

## Metrics

The plugin reports its own health back into the run so an export problem is
visible in loadr's summary rather than silently dropping data:

| Metric | Kind | Meaning |
|---|---|---|
| `cloudwatch_metrics_sent` | counter | Total metric data points accepted by `PutMetricData` |
| `cloudwatch_throttles` | counter | Requests rejected with `Throttling` / `429` (rate-limited by CloudWatch) |

A healthy run shows `cloudwatch_metrics_sent` climbing once per second and
`cloudwatch_throttles` at zero; a persistently rising `cloudwatch_throttles`
means you are pushing more series per second than the account's `PutMetricData`
limit allows — trim the metric set or coarsen the dimensions.

## Notes

- **Credentials come from the environment.** Use an IAM role, `aws-vault`, or the
  standard `AWS_*` variables; never commit access keys in a plan.
- **Least privilege.** The plugin only needs `cloudwatch:PutMetricData` — scope
  the policy to that action.
- **Match the region to the intake.** `config.region` (or `AWS_REGION`) selects
  the endpoint; data lands in that region's CloudWatch and nowhere else.
- **Fire-and-batch, not blocking.** A flush failure is counted and the run
  continues; the plugin does not stall the load generator waiting on CloudWatch,
  and a transient error or throttle does not fail the test.
- **Ingestion cost.** Every snapshot second is a billable `PutMetricData`
  submission and custom-metric charge — keep `namespace`/`dimensions` tight and
  lean on `thresholds` for pass/fail rather than querying CloudWatch in CI.
- For end-of-run gating in CI, prefer `--summary-export results.json` and
  loadr's own `thresholds`; use the CloudWatch export for the live and historical
  view, not the exit code.
```
