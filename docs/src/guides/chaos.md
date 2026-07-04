# Chaos testing (fault injection)

A load test that only runs on a healthy network tells you how the system
behaves on a good day. The `faults:` block degrades the traffic loadr
generates — **added latency jitter** and **dropped requests** — so you can
rehearse the bad day: do the retries fire, do the
[thresholds](../yaml/thresholds.md) catch it, do the dashboards and alerts
light up?

```yaml
faults:
  latency:
    jitter: 100ms            # extra client-side delay per request
    distribution: gaussian   # gaussian | uniform
  drop_rate: 0.05            # drop 5% of requests
  drop_mode: before_send     # dropped before they leave the client

scenarios:
  api:
    executor: constant-vus
    vus: 20
    duration: 2m
    flow:
      - request: { url: /api/orders }
```

Faults are injected **in the load generator**, not in your infrastructure —
nothing is installed on the target, and turning chaos off is deleting four
lines of YAML. That makes this the cheap, repeatable end of chaos engineering:
same plan, same CI job, plus faults.

## `latency` — jitter

Every request gets an extra delay before it completes, drawn per request from
the configured distribution and scaled by `jitter`:

| `distribution` | Shape of the added delay |
|---|---|
| `uniform` | spread evenly across the jitter range |
| `gaussian` | clustered around the middle with occasional outliers (never negative) |

The delay is indistinguishable from network latency to everything downstream:
it lands in `http_req_duration`, moves the percentiles, and trips latency
thresholds exactly as real slowness would.

## `drop_rate` / `drop_mode` — loss

`drop_rate` is the fraction (`0..1`) of requests to drop. `drop_mode:
before_send` (the only mode today) fails the request before it leaves the
client — it never reaches the target, and it counts as a failed request in
`http_req_failed`, checks and error-rate thresholds.

## The `faults_injected` counter

Every injected fault — a jittered request or a dropped one — increments the
**`faults_injected`** counter. It appears in the summary and on the run
[timeline](../reporting.md), so you can see chaos overlaid on the latency and
error charts, and you can threshold on it like any counter:

```yaml
thresholds:
  faults_injected: [ "count>0" ]      # fail the run if chaos silently no-oped
  http_req_failed: [ "rate<0.10" ]    # ...while proving errors stay bounded
```

## What to assert under chaos

A chaos run flips the intent of your gates: instead of "nothing goes wrong",
assert "when things go wrong, the system degrades the way we promised".

```yaml
faults:
  latency: { jitter: 200ms, distribution: uniform }
  drop_rate: 0.02
  drop_mode: before_send

thresholds:
  http_req_duration: [ "slo(95%) < 800ms" ]   # SLO holds despite jitter
  checks: [ "rate>0.90" ]                     # graceful degradation, not collapse
  faults_injected: [ "count>0" ]
```

Keep the chaos plan separate from your clean-baseline plan — and never feed a
chaos run to [`loadr compare`](compare.md) as a baseline.
