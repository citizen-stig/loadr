# Thresholds

Thresholds are the pass/fail contract of a test, evaluated continuously
during the run and finally at the end. Any failing threshold makes
`loadr run` exit with code **99** (k6-compatible).

```yaml
thresholds:
  http_req_duration:
    - "p(95)<400"                    # plain expression
    - threshold: "p(99.9)<1500"      # object form
      abort_on_fail: true            # stop the test the moment it fails...
      delay_abort_eval: 30s          # ...but not in the first 30s (warm-up)
  http_req_failed: [ "rate<0.01" ]
  checks: [ "rate>0.99" ]
  my_custom_counter: [ "count>1000" ]
  "http_req_duration{scenario:api}": [ "p(99)<250" ]   # tag-filtered
```

## Expression syntax

`<aggregation> <op> <bound>` where `op` ∈ `<` `<=` `>` `>=` `==` `!=`.

| Aggregation | Applies to | Meaning |
|---|---|---|
| `avg`, `min`, `max`, `med` | trend | statistics in milliseconds |
| `p(N)` | trend | any percentile, e.g. `p(95)`, `p(99.9)` (HDR-exact) |
| `rate` | rate | pass fraction 0..1; on counters: events/second |
| `count` | counter | total |
| `value` | gauge | last value |
| `slo(N%)` | trend | SLO form: N% of samples within the bound (see [below](#slo-objectives)) |

Bounds accept durations for time metrics: `p(95)<400ms`, `avg<1.5s`.

## SLO objectives

`slo(N%) < bound` states a latency objective the way SLOs are written —
"N% of requests complete within *bound*":

```yaml
thresholds:
  http_req_duration:
    - "slo(99%) < 300ms"       # 99% of requests under 300ms
    - "slo(99.9%) < 1s"
```

It is exactly equivalent to the matching percentile check
(`slo(99%) < 300ms` ≡ `p(99)<300`) — the win is that the plan reads like the
SLO document it enforces.

- Supported objectives: **50, 90, 95, 99, 99.9** — the fixed percentiles the
  histogram summary carries. Anything else (`slo(99.5%)`) is rejected at
  parse time rather than silently approximated; use `p(99.5)` if you need an
  arbitrary percentile.
- The percent sign is optional: `slo(95)<400` works.
- Everything else about thresholds applies unchanged: duration bounds, tag
  selectors, `abort_on_fail`, exit code 99.

## Tag selectors

`metric{tag:value,tag2:value2}` aggregates only samples whose tags include
all listed pairs. Useful tags: `scenario`, `name` (request name), `method`,
`status`, `group`, `check`, plus anything from `tags:` blocks.

```yaml
thresholds:
  "http_req_duration{name:checkout}": [ "p(95)<800" ]
  "checks{scenario:browse}": [ "rate>0.95" ]
```

## Semantics worth knowing

- A threshold over a metric with **no samples passes** (matching k6) — but
  `loadr validate` warns when the metric name is unknown.
- `abort_on_fail` triggers a graceful stop (in-flight iterations finish,
  summary still produced, exit code 99).
- In distributed runs thresholds are evaluated **centrally** on merged
  histograms, so `p(99)` is the true fleet-wide percentile.
