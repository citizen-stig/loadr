# Lifecycle hooks

```text
            ┌──────────┐
            │ setup()  │  once, before any VU; may make requests;
            └────┬─────┘  return value shared (read-only) with all VUs
                 │
   ┌─────────────┴──────────────┐
   │ per iteration, per VU:     │
   │   flow steps               │   beforeRequest(req) ─▶ request ─▶ afterRequest(res)
   │   then exec function       │   (around every YAML request step)
   └─────────────┬──────────────┘
                 │
            ┌────┴──────┐
            │ teardown()│  once, after the run (even on abort)
            └───────────┘
```

## `setup()` / `teardown(data)`

```js
export function setup() {
  const res = http.post('/auth/token', JSON.stringify({ id: __ENV.CLIENT_ID }));
  return { token: res.json().token };          // must be JSON-serializable
}
export function teardown(data) {
  http.post('/auth/revoke', JSON.stringify({ token: data.token }));
}
```

## Scenario functions

A scenario runs its YAML `flow` first (if any), then its `exec` function
(default export when `exec: default`):

```yaml
scenarios:
  scripted: { executor: constant-vus, vus: 10, duration: 5m, exec: buyFlow }
```

```js
export function buyFlow(data, ctx) {
  // data = setup() result; ctx = { vu, iteration, scenario }
  const res = http.get('/items', { headers: { Authorization: `Bearer ${data.token}` } });
  check(res, { 'ok': (r) => r.status === 200 });
  sleep(1);
}
```

## `beforeRequest(req)` / `afterRequest(res)`

Fire around **every YAML `request:` step** (not around `http.*` calls made
from JS). `beforeRequest` may mutate and return the request:

```js
export function beforeRequest(req) {
  req.headers['X-Signature'] = crypto.hmac('sha256', __ENV.SIGNING_KEY, req.body || '', 'hex');
  return req;       // returning nothing keeps the request unchanged
}

export function afterRequest(res) {
  if (res.status === 429) console.warn(`rate limited on ${res.url}`);
}
```

The `req` object: `{name, method, url, headers, body}` — `url`, `method`,
`headers` and `body` may be overridden by the returned object.

## Per-VU `on_start` / `on_stop`

A scenario can name an exported function to run **once per VU**, around that
VU's stream of iterations (Locust's `on_start` / `on_stop`):

- `on_start` runs once, just before the VU's **first** iteration.
- `on_stop` runs once, when the VU **retires** (after its last iteration).
  It is skipped for a VU that never ran an iteration.

Use them for per-user setup and cleanup that should happen once per virtual
user rather than once per iteration — e.g. log in on start, log out on stop.
Both receive the `setup()` result as their single argument:

```yaml
scenarios:
  users:
    executor: constant-vus
    vus: 50
    duration: 5m
    on_start: login        # exported from the JS module
    on_stop: logout
    exec: browse
```

```js
export function login(data) {
  const res = http.post('/auth/login', JSON.stringify({ pw: __ENV.PW }));
  // Stash per-VU state on the VU's session for later iterations.
  session.vars.token = res.json().token;
}

export function browse(data) {
  http.get('/feed', { headers: { Authorization: `Bearer ${session.vars.token}` } });
}

export function logout(data) {
  http.post('/auth/logout', JSON.stringify({ token: session.vars.token }));
}
```

`on_start` runs per VU (so once per simulated user), whereas `setup()` runs
once for the whole test. A failing `on_start` / `on_stop` is logged as a
warning and does not abort the run.

## `handleSummary(data)`

Export `handleSummary` to produce a **custom end-of-run report**. It runs once,
after `teardown()`, with the run summary as its single argument. If it returns
a string, that string replaces the default console summary; returning nothing
(or `null`) leaves the default summary in place. This matches k6's
`handleSummary`.

```js
export function handleSummary(data) {
  const reqs = data.metrics.find((m) => m.metric === 'http_reqs');
  const dur  = data.metrics.find((m) => m.metric === 'http_req_duration');
  return [
    `run ${data.run_id} — ${data.duration_secs.toFixed(1)}s`,
    `requests: ${reqs ? reqs.agg.sum : 0}`,
    `p95 latency: ${dur ? dur.agg.p95.toFixed(1) : 0} ms`,
    `thresholds passed: ${data.thresholds_passed}`,
  ].join('\n');
}
```

`data` is the run summary (the same object written by the JSON output):

```js
{
  name, run_id,
  started_ms, ended_ms, duration_secs,
  scenarios: ['users', ...],            // scenario names
  metrics: [ { metric, kind, agg: { avg, min, med, max, p90, p95, p99,
                                    sum, count, rate, per_second, last } }, ... ],
  checks:  [ { name, passes, fails }, ... ],
  thresholds: [ ... ],
  thresholds_passed: true,
  aborted: null,                        // abort reason, if any
}
```

Non-string return values are pretty-printed as JSON and used as the report.
