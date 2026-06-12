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
