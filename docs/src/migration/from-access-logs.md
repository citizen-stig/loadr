# Replaying access logs

Your production nginx/apache access log already *is* a load profile: real
endpoints, real mix, real arrival rate. `loadr convert` turns it into a
runnable plan:

```bash
loadr convert access.log -o replay.yaml     # .log infers the kind
loadr convert traffic.txt --from accesslog  # force it for other extensions
```

It parses **COMBINED**-format lines (and plain **COMMON** — no
referer/user-agent), tolerating custom `log_format`s with extra trailing
fields:

```text
203.0.113.7 - alice [10/Oct/2025:13:55:36 +0000] "GET /api/users/42 HTTP/1.1" 200 512 "-" "curl/8.0"
```

Malformed lines are skipped and counted in a warning.

## What it builds

One scenario, `replayed_traffic`, that reproduces the observed traffic shape:

- **`constant-arrival-rate`** at the log's *average* request rate, for the
  log's observed duration; `pre_allocated_vus` sized to the observed
  *per-second peak* (`max_vus` at twice that).
- A single **weighted `random` block** over the top 20 endpoints, weighted by
  how often each appeared — the same shape as
  `examples/40-scenario-weights.yaml`.
- Endpoints are grouped by method + normalised path: numeric ids, UUIDs and
  long hex segments become `${vars.id}`, and query strings are dropped for
  grouping.

```yaml
# loadr convert access.log (abridged)
name: access log replay
description: 'Imported from an access log by `loadr convert`: 18240 requests
  over 600s (avg 30.40 req/s, peak 77 req/s).'
variables:
  id: "1"                        # placeholder — see warnings
scenarios:
  replayed_traffic:
    executor: constant-arrival-rate
    rate: 30.4
    duration: 600s
    pre_allocated_vus: 77
    max_vus: 154
    flow:
      - random:
          strategy: weighted
          choices:
            - weight: 9120
              name: GET /api/items
              steps: [ { request: { name: GET /api/items, method: GET, url: /api/items } } ]
            - weight: 4560
              name: GET /api/users/${vars.id}
              steps: [ { request: { method: GET, url: "/api/users/${vars.id}" } } ]
            - weight: 1824
              name: POST /api/orders
              steps: [ { request: { method: POST, url: /api/orders } } ]
```

The output always passes `loadr validate`.

## Read the warnings

Everything the converter approximated is reported to stderr. Fix these before
running load:

| Warning | What to do |
|---|---|
| `base_url` | Logs record neither scheme nor host — set `defaults.http.base_url`. |
| average vs peak rate | The rate is the observed *average*; for worst-case load, switch to `ramping-arrival-rate` up to the reported peak. |
| id segments normalised | Replace the placeholder `id` variable with a [`data:` feeder](../yaml/data.md) of real identifiers. |
| query strings dropped | Re-add parameters that matter via `params:`. |
| request bodies | Logs don't record bodies — add realistic `body:` payloads to POST/PUT/PATCH requests. |
| long tail dropped | Only the top 20 endpoints are kept; the warning says what share of traffic that covers. |

As with [HAR](from-har.md) and [k6](from-k6.md) imports, treat the output as a
strong first draft: set the base URL, feed real ids, add
[checks](../yaml/assertions-checks.md) and
[thresholds](../yaml/thresholds.md), then run.
