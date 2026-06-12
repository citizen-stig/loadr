# Think time & pacing

## Think time (JMeter-style timers)

A pause, either as an explicit step or as a default after every request:

```yaml
flow:
  - request: { url: / }
  - think_time: { type: constant, duration: 2s }
  - request: { url: /next }
```

| Type | Fields | Behaviour |
|---|---|---|
| `constant` | `duration` | fixed pause |
| `uniform` | `min`, `max` | uniformly random in [min, max] |
| `gaussian` | `mean`, `std_dev` | normal distribution, truncated at 0 |

Scenario- or test-wide default (applied after each `request` step):

```yaml
defaults:
  think_time: { type: uniform, min: 1s, max: 3s }
scenarios:
  fast_api:
    think_time: { type: constant, duration: 100ms }   # overrides the default
```

## Pacing (constant throughput)

The JMeter "constant throughput timer" equivalent: space iteration starts so
the scenario approaches a target rate, with VUs as the concurrency ceiling.

```yaml
scenarios:
  steady:
    executor: constant-vus
    vus: 20
    duration: 10m
    pacing: { iterations_per_second: 10 }   # ~10 iterations/s across all 20 VUs
    flow: [ { request: { url: / } } ]
```

Prefer the arrival-rate executors when you need precise *offered* load;
pacing is the right tool when porting JMeter plans or when you want a closed
model with an upper rate bound.
