# Feeder strategies & throttling

Two more features borrowed from Gatling: feeder *strategies* (how rows are
chosen) and a *throttle* (a hard request-rate ceiling).

## Pick strategies

Any CSV, JSON or inline data source takes a `pick` strategy alongside its
`mode` (shared/per-VU) and `on_eof` (recycle/stop):

```yaml
data:
  users:
    type: csv
    path: data/users.csv
    mode: per_vu
    pick: shuffle       # sequential (default) | random | shuffle
    on_eof: recycle
```

| `pick` | Behaviour |
|---|---|
| `sequential` | rows in file order; the cursor advances by one (default) — Gatling circular |
| `random` | a uniformly random row every time; never exhausts (`on_eof` ignored) — Gatling random |
| `shuffle` | the full set shuffled once per VU, then read in that order — Gatling shuffle |

## JSON feeders

Besides CSV and inline rows, a data source can be a JSON file — an array of
objects, each object a row:

```yaml
data:
  skus:
    type: json
    path: data/skus.json    # [ { "sku": "W-1", "name": "Widget" }, ... ]
    pick: random
```

Reference fields the same way: `${data.skus.sku}`.

## Throttling (request-rate ceiling)

A scenario can cap its aggregate request rate regardless of how many VUs are
running or how fast the target responds — Gatling's `throttle` /
`reachRps(...)`. Iterations block before each request until a slot frees up
(a global token-bucket limiter shared across all the scenario's VUs).

```yaml
scenarios:
  steady:
    executor: constant-vus
    vus: 50
    duration: 10m
    throttle: { requests_per_second: 200 }   # never exceed 200 req/s in total
    flow:
      - request: { url: /api/items }
```

This is distinct from the arrival-rate executors (which control *iteration*
starts) and from `pacing` (which spaces iterations): `throttle` is a ceiling on
*requests* that applies on top of whatever executor you choose. Use it to stay
under a known rate limit, or to hold a flat load while a closed model would
otherwise overshoot.

See `examples/17-feeders-and-throttle.yaml`.
