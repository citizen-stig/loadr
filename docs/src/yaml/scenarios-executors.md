# Scenarios & executors

A test has one or more **scenarios**, all running concurrently (offset with
`start_time`). Each scenario picks an **executor** — the algorithm that
schedules iterations. loadr implements all seven k6 executors with identical
semantics.

```yaml
scenarios:
  my_scenario:
    executor: ramping-vus        # which scheduling model
    # ... executor-specific knobs ...
    start_time: 30s              # delay after test start (default 0)
    graceful_stop: 30s           # time for in-flight iterations to finish (default 30s)
    exec: myJsFunction           # JS function to run per iteration (optional)
    flow: [ ... ]                # declarative steps per iteration (optional; needs flow and/or exec)
    pacing: { iterations_per_second: 10 }   # constant-throughput governor
    think_time: { type: constant, duration: 1s }  # default pause after each request
    tags: { kind: api }          # tags on all samples from this scenario
```

## Closed-model executors

New iterations start only when a VU finishes its previous one — throughput
depends on response times (a coordinated-omission-prone model; use open
models to control *offered* load).

### `constant-vus`

```yaml
executor: constant-vus
vus: 50
duration: 5m
```

### `ramping-vus`

VU count follows linear ramps between stage targets.

```yaml
executor: ramping-vus
start_vus: 0
stages:
  - { duration: 2m, target: 100 }   # ramp 0 → 100
  - { duration: 5m, target: 100 }   # hold
  - { duration: 1m, target: 0 }     # ramp down
graceful_ramp_down: 30s             # grace for iterations on de-allocated VUs
```

### `per-vu-iterations`

Each VU runs exactly N iterations.

```yaml
executor: per-vu-iterations
vus: 10
iterations: 100        # per VU → 1000 total
max_duration: 10m      # safety cap (default 10m)
```

### `shared-iterations`

A pool of N iterations split dynamically among VUs (fast VUs do more).

```yaml
executor: shared-iterations
vus: 10
iterations: 1000       # total
max_duration: 10m
```

## Open-model executors

Iterations start **on schedule regardless of completion** — the offered load
is what you configured, and saturation shows up as `dropped_iterations`
instead of silently lower request rates.

### `constant-arrival-rate`

```yaml
executor: constant-arrival-rate
rate: 100              # iteration starts per time_unit
time_unit: 1s          # default 1s (rate: 6000 + time_unit: 1m ≡ 100/s)
duration: 10m
pre_allocated_vus: 50  # workers created up front
max_vus: 200           # pool may grow to this before dropping iterations
```

### `ramping-arrival-rate`

```yaml
executor: ramping-arrival-rate
start_rate: 10
time_unit: 1s
pre_allocated_vus: 50
max_vus: 500
stages:
  - { duration: 2m, target: 100 }   # linear rate ramp
  - { duration: 5m, target: 100 }
```

## `externally-controlled`

VU count is set at runtime — from the web UI's run page, the controller API,
or programmatically. Great for exploratory "turn the dial" testing.

```yaml
executor: externally-controlled
max_vus: 500
duration: 30m          # optional; omit to run until stopped
```

## Graceful stop semantics

When a scenario's schedule ends (or the run is stopped), no new iterations
start; in-flight iterations get `graceful_stop` (default 30s) to finish before
being cancelled. `ramping-vus` additionally applies `graceful_ramp_down` to
VUs being de-allocated mid-iteration during a downward ramp.
