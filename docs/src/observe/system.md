# System metrics (observe)

The `observe:` block is the inverse of [outputs](../yaml/outputs.md): instead
of pushing load metrics out, it **pulls foreign metrics in** for the run
window, overlays them on the run [timeline](../reporting.md), and lets you
threshold on them. The `system` source samples the local host's CPU, memory,
disk and network live during the run:

```yaml
observe:
  - type: system
    metrics: [cpu, memory, disk, network]   # default: all four
    interval: 1s                             # default 1s (floor 100ms)
```

That yields four series on the timeline, next to the load metrics in the HTML
report and the summary export:

| Series | Meaning | Unit |
|---|---|---|
| `system_cpu` | busy fraction across all cores | ratio `0..1` |
| `system_memory` | used / total (`MemAvailable`-based) | ratio `0..1` |
| `system_disk_io` | bytes read + written per second, physical devices only | bytes/s |
| `system_network` | bytes received + sent per second, loopback excluded | bytes/s |

`as_prefix: gen1` renames the series (`gen1_cpu`, …) — useful when you also
pull the *target's* metrics and want the legend unambiguous.

Failure never breaks the load test: an unreadable `/proc` file just skips a
point. The `system` source is **Linux-only** (`/proc`); on macOS/Windows it
logs a warning and produces no series.

## Why observe the generator?

The first question after a bad latency chart is "was that the target, or was
my load generator saturated?". `system_cpu` on the same timeline answers it —
if generator CPU pins at 1.0 exactly when p99 spikes, the numbers are lying to
you and you need more [agents](../distributed/overview.md), not a faster
backend.

## Thresholds on observed metrics

Observed series are thresholdable like any [metric](../yaml/thresholds.md),
evaluated as gauges (`avg`, `min`, `max`, `med`, percentiles, `value`):

```yaml
observe:
  - { type: system, metrics: [cpu, memory] }

thresholds:
  system_cpu: [ "max<0.9" ]        # fail the run if the generator saturates
  system_memory: [ "avg<0.8" ]
```

These gates are evaluated **at run end**, when the series are drained —
`abort_on_fail` doesn't fire live for observed metrics (yet).

## Pulling the target's metrics: `type: prometheus`

The same block also queries a Prometheus server with a PromQL range query
over the run window, collected post-run:

```yaml
observe:
  - type: system                                   # this host
  - type: prometheus                               # the target
    name: api cpu
    source: http://prometheus:9090
    query: sum(rate(container_cpu_usage_seconds_total{pod=~"api-.*"}[1m]))
    as: target_cpu
    unit: ratio
    token: ${env.PROM_TOKEN}                       # optional bearer token
```

| Field | Meaning |
|---|---|
| `source` | Prometheus base URL |
| `query` | PromQL expression, run as a range query over `[start, end]` |
| `as` | series name in the report (default: `name`, else derived from the query) |
| `unit` | axis hint: `ratio` \| `percent` \| `bytes` \| `count` \| `seconds` |
| `token` | optional bearer token |

A query returning several label sets produces suffixed series
(`target_cpu`, `target_cpu_1`, …). An unreachable source is logged and
skipped — it never fails the run. Prometheus series are thresholdable exactly
like `system` ones (`target_cpu: [ "max<0.9" ]`).

For Kubernetes pod metrics or CloudWatch, see the
[k8s-metrics](../plugins/k8s-metrics.md) and
[CloudWatch](../plugins/cloudwatch.md) collector plugins.
