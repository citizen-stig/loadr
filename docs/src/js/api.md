# JS API reference

The `loadr` module re-exports the whole standard library, so a single specifier works: `import { http, check, sleep, Trend } from 'loadr'`. Or import from the focused sub-modules:
`import http from 'loadr/http'`, `import { check, sleep, group } from 'loadr'`,
`import { Counter, Gauge, Rate, Trend } from 'loadr/metrics'`.

## `http`

```js
http.get(url, params?)
http.post(url, body?, params?)        // also put, patch, del, head, options
http.request(method, url, body?, params?)
```

- `body`: string, or object (serialized as JSON with `Content-Type: application/json`).
- `params`: `{ headers: {}, timeout: 5000 /* ms */, tags: {}, name: 'metric name' }`.
- Relative URLs join `defaults.http.base_url`. Requests use the VU's cookie
  jar, connection pool and TLS settings, and emit the full `http_*` metric
  family.

Response object:

```js
{
  status: 200, status_text: 'OK',
  body: '...',            // string
  json(),                 // parsed body (or null)
  headers: { 'content-type': '...' },   // lower-cased keys
  duration_ms: 87.2,
  timings: { dns_ms, connect_ms, tls_ms, sending_ms, waiting_ms, receiving_ms, duration_ms, blocked_ms },
  error: null,            // transport error string, if any
  url: 'https://...',     // final URL after redirects
  protocol: 'HTTP/2'
}
```

## `check(value, conditions, tags?)`

```js
check(res, {
  'status 200': (r) => r.status === 200,
  'fast': (r) => r.duration_ms < 200,
  'flag set': someBoolean,
});
```

Each key records a pass/fail sample into the `checks` metric (tag
`check=<key>`). Returns `true` when all passed. Never throws.

## `sleep(seconds)` and `group(name, fn)`

```js
sleep(1.5);
group('checkout', () => { http.post('/cart', ...); });
```

Groups nest; samples inside carry `group="::checkout"` tags.

## Metrics

```js
const errors = new Counter('business_errors');
const queue = new Gauge('queue_depth');
const hits = new Rate('cache_hits');
const renderTime = new Trend('render_time');

errors.add(1);
queue.add(42);
hits.add(true);                       // or 1/0
renderTime.add(16.6, { page: 'home' });   // value + extra tags
```

Metrics are registered on first use (or declare them in YAML `metrics:` to
use them in thresholds with validation).

## `session` — the VU bridge

```js
session.vu              // VU id (number)
session.iteration       // current iteration (0-based)
session.scenario        // scenario name
session.vars.foo        // shared variable store: ${foo} in YAML sees this
session.vars.foo = 'x'
session.data('users')   // current data row for a source: {col: value}
session.cookieGet(url, name)
session.cookieSet(url, name, value)
session.cookiesClear()
// conveniences for YAML one-liners:
session.counterAdd(name, value, tags?)
session.gaugeSet(name, value, tags?)
session.rateAdd(name, pass, tags?)
session.trendAdd(name, value, tags?)
```

## `crypto`

```js
crypto.sha256('data', 'hex')       // or 'base64'
crypto.sha1('data', 'hex')
crypto.md5('data', 'hex')
crypto.hmac('sha256', 'secret', 'data', 'hex')
crypto.randomBytes(16)             // array of bytes
crypto.uuidv4()                    // string
```

## `encoding`

```js
encoding.b64encode('hello')        // 'aGVsbG8='
encoding.b64decode('aGVsbG8=')     // 'hello'
```

## Environment & files

```js
__ENV.MY_VAR                       // process environment (string | undefined)
open('./payload.json')             // file contents as string
open('./blob.bin', 'b')            // as bytes
```

`open()` resolves relative to the test file's directory and refuses to read
outside it.

## `console`

`console.log/info/warn/error/debug` route into loadr's structured logging
(visible with `-v`, in the web UI log view, and in agent logs).
