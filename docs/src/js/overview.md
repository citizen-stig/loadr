# Embedded JavaScript overview

loadr embeds a JavaScript engine (QuickJS) so dynamic logic lives next to the
declarative YAML. JS is usable three ways:

## 1. Inline expressions

Anywhere `${...}` works, `${js: <expr>}` evaluates in the VU's runtime:

```yaml
headers:
  X-Request-Id: "${js: crypto.uuidv4()}"
params:
  page: "${js: Math.ceil(Math.random() * 10)}"
```

## 2. Inline script steps

```yaml
flow:
  - js: "session.counterAdd('pages_viewed', 1)"
  - js:
      script: |
        const row = session.data('users');
        session.vars.greeting = `hello ${row.username}`;
  - js:
      call: warmCache        # an exported function from the module
```

## 3. A module (inline or file)

```yaml
js:
  file: ./script.js          # or  script: |  (inline source)
  timeout: 10s               # per-call wall-clock limit (default 10s)
  memory_limit_mb: 64        # per-VU heap limit (default 64)
```

The module is an ES module with k6-compatible imports:

```js
import http from 'k6/http';
import { check, sleep, group } from 'k6';
import { Counter, Trend } from 'k6/metrics';

export function setup() { /* once, before VUs start */ return {...}; }
export default function (data) { /* per iteration when exec/default used */ }
export function teardown(data) { /* once, after the run */ }
export function beforeRequest(req) { /* around every YAML request */ return req; }
export function afterRequest(res) { /* ... */ }
```

## Isolation & limits

Every VU gets its **own** JS runtime and context — no shared mutable state
between VUs (matching k6). Each runtime enforces:

- a heap limit (`memory_limit_mb`) — exceeding it throws;
- a wall-clock interrupt per call (`timeout`) — infinite loops are killed;
- no filesystem or network access except through the provided APIs
  (`open()` is restricted to the test's directory).

## Values flow both ways

- Extracted YAML values appear in JS as `session.vars.<name>`.
- Values set from JS (`session.vars.x = ...`) are usable in YAML as `${x}`.
- `setup()`'s return value is passed to every scenario function and is
  readable in hooks.
