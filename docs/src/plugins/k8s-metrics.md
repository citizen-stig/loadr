# k8s-metrics plugin

> **Status:** planned — this plugin is not in the signed [plugin index](installing.md)
> yet, and `observe:` plugin sources are a later phase of the
> [`observe` RFC](https://github.com/levantar-ai/loadr). The shape below describes
> the intended collector contract; the config keys and metric names may still
> change before the first release.

`loadr-plugin-k8s-metrics` is a **service plugin** (`kind = "service"`, role:
*observability collectors*). During a run it polls the Kubernetes
**`metrics.k8s.io`** aggregated API (falling back to the **kubelet `/stats/summary`**
endpoint) over HTTPS, reads pod CPU and memory usage for a namespace/label
selection, and emits **system-metric samples aligned to loadr's run timeline** —
so container resource usage overlays the load metrics on one chart.

It is **pure HTTP over [hyper](https://github.com/hyperium/hyper)** — loadr's own
HTTP stack — with **bearer-token auth** from the in-cluster service-account token,
exactly matching the built-in **Prometheus system-metric collector** already in
the tree. There is no `kubectl`, no Kubernetes client SDK, and no extra C
dependency: it authenticates with the mounted service-account credentials and
speaks the metrics API directly.

Like every collector, it pulls from the controller for the run's time window
only, resamples onto loadr's snapshot grid, and **never fails the load test** if a
scrape is slow or the API is briefly unreachable — a missed scrape leaves a gap in
the series and is counted, not fatal. The canonical sample model and the
correlation story are described in the
[`observe` design note](https://github.com/levantar-ai/loadr).

The service lifecycle it uses is the native `FfiService` contract
(`start(config_json) → stop()`) documented in
[Native plugins](native.md#the-interface).

## Install

Once published, `k8s-metrics` will resolve from the signed
[plugin index](installing.md) by name — no build toolchain required:

```bash
loadr plugin install k8s-metrics
loadr plugin info k8s-metrics
```

This picks the artifact for your host target, checks it against the plugin ABI
your `loadr` build provides, verifies its sha256 and unpacks it into your plugins
directory (`~/.loadr/plugins/k8s-metrics/`, or `$LOADR_PLUGINS_DIR`).

The installed manifest declares a native service plugin:

```toml
[plugin]
name = "k8s-metrics"
version = "0.1.0"
kind = "service"
type = "native"
entry = "libloadr_plugin_k8s_metrics.so"
description = "Polls metrics.k8s.io for pod CPU/memory and emits system-metric samples"
```

To run straight from a build tree instead, point the plan's `plugins:` entry at
the built artifact (`path: target/release/libloadr_plugin_k8s_metrics.so`) rather
than resolving it by name.

## Use it in a test

List the plugin under `plugins:`, then wire it into the plan's `observe:` block as
a collector. `type: plugin` routes the collector step to a service plugin and
`service:` names it; the `config:` block selects which pods to scrape and how
often. Collection happens once, on the controller, for the run's window — so the
series merges straight onto the timeline.

```yaml
plugins:
  - name: k8s-metrics            # or: { name: k8s-metrics, path: target/release/libloadr_plugin_k8s_metrics.so }

defaults:
  http:
    base_url: https://api.example.com

observe:
  - name: api pods
    type: plugin
    service: k8s-metrics
    config:
      namespace: app            # namespace to scrape
      selector: app=api         # label selector for the pods
      interval_ms: 5000         # poll every 5s

scenarios:
  load:
    executor: constant-vus
    vus: 25
    duration: 10m
    flow:
      - request: { name: list,   url: /api/items,   checks: [ { type: status, equals: 200 } ] }
      - request: { name: detail, url: /api/items/1, checks: [ { type: status, equals: 200 } ] }

thresholds:
  http_req_duration: [ "p(95)<400" ]
  # gate the run on the target staying healthy, not just the client SLO:
  "k8s_pod_cpu_cores{namespace:app}": [ "value<3.5" ]
```

The emitted series (`k8s_pod_cpu_cores`, `k8s_pod_mem_bytes`) show up in the
report's **Infrastructure** chart group and can be overlaid on a load chart (e.g.
p99 latency + pod CPU on a dual axis), so a throughput plateau and a CPU ceiling
line up on one x-axis.

## Config reference

Passed as the collector's `config:` map (JSON at the ABI boundary, e.g.
`{"namespace":"app","selector":"app=api","interval_ms":5000}`).

| Key | Type | Default | Meaning |
|---|---|---|---|
| `namespace` | string | `default` | Kubernetes namespace to scrape pods from. |
| `selector` | string | *(all pods in namespace)* | Label selector (`app=api`, `tier=backend,app=api`) narrowing which pods are collected. Passed through as the metrics API `labelSelector`. |
| `interval_ms` | integer (ms) | snapshot interval | Poll period. Each tick is one scrape of the metrics API. Bounds query volume against the API server. |
| `api_url` | string | in-cluster (`https://kubernetes.default.svc`) | Override the API server base URL (e.g. to hit the kubelet summary endpoint directly). |
| `token` | string | mounted SA token | Bearer token. Defaults to the projected service-account token at `/var/run/secrets/kubernetes.io/serviceaccount/token`; supply via `${env.K8S_TOKEN}` to run off-cluster. |
| `ca_cert` | string | mounted SA CA | Path to the cluster CA bundle for TLS verification. Defaults to the mounted service-account CA. |
| `insecure` | bool | `false` | Skip TLS verification of the API server (off-cluster testing only). |

Credentials are read from the environment / mounted secrets and are **redacted
from logs and exports**; never inline a token in a committed plan.

## Metrics

The collector normalizes each scrape into loadr's metric model — indistinguishable
from a native sample downstream, so overlays, thresholds, and re-export all apply
for free:

| Metric | Kind | Unit | Meaning |
|---|---|---|---|
| `k8s_pod_cpu_cores` | gauge | cores | Pod CPU usage in cores, tagged `{ namespace, pod }`. |
| `k8s_pod_mem_bytes` | gauge | bytes | Pod working-set memory in bytes, tagged `{ namespace, pod }`. |
| `k8s_scrapes` | counter | count | One per successful poll of the metrics API; a gap in this series flags failed scrapes. |

Each pod matched by the selector produces its own `(metric, tags)` series, so a
Deployment's pods appear as separate lines (or aggregate, via the report's tag
rollups). Because the gauges support the `value` aggregation and tag selectors,
you can threshold on them directly:

```yaml
thresholds:
  "k8s_pod_cpu_cores{namespace:app}": [ "value<3.5" ]     # don't redline the pods
  "k8s_pod_mem_bytes{namespace:app}": [ "max<2147483648" ] # stay under 2 GiB
```

With `abort_on_fail`, a breaching threshold stops the run the moment the target
pods saturate — not just when the client-side SLO breaks.

## Notes

- **RBAC.** The service account the run uses needs read access to the metrics API:
  `get`/`list` on `pods` in the target namespace and on
  `pods.metrics.k8s.io`. Without the metrics-server aggregated API installed, the
  collector falls back to the kubelet `/stats/summary` endpoint, which requires the
  `nodes/stats` permission instead.
- **Metrics-server latency.** `metrics.k8s.io` values are themselves sampled on the
  metrics-server's own interval (typically ~15s), so an `interval_ms` far below that
  re-reads the same value. Match `interval_ms` to your snapshot interval and expect
  metrics-server-grained resolution, not per-request granularity.
- **On-cluster placement.** For the in-cluster defaults to work, the loadr
  controller must run inside the cluster (e.g. a Job in the same cluster as the
  target) with the projected service-account token mounted. To collect from outside
  the cluster, set `api_url`, `token`, and `ca_cert` explicitly.
- **Failure isolation.** An unreachable API server, a slow scrape, or a garbage
  response is logged and counted (`k8s_scrapes` stops advancing) but **never fails
  the load test**; the affected series simply shows a gap. Set the collector's
  `required: true` only if you want a source outage to hard-fail the run.
- **Distributed runs.** Collection happens once, on the controller — not per agent —
  so there is a single coherent view of pod resource usage aligned to the merged
  load timeline, with no N× duplicate scrapes against the API server.
