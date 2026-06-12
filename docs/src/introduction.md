# loadr

loadr is a load testing platform in a single binary. It combines the two
dominant traditions in load testing:

- **k6's model**: scriptable tests, open/closed load models with precise
  executors, a first-class metrics engine with thresholds as pass/fail
  criteria, and a great CLI experience.
- **JMeter's breadth**: rich assertions (response code, body, JSONPath, XPath,
  size, duration), extractors (regex, boundary, CSS, XPath), timers (constant,
  uniform, gaussian, constant-throughput), CSV data sets, cookie management,
  and broad protocol coverage.

…and adds what both lack: **declarative YAML test definitions** validated by a
JSON Schema, a **plugin system** (sandboxed WASM components and native
libraries), **built-in distributed execution** with mathematically correct
percentile aggregation, and a **built-in management web UI**.

## A taste

```yaml
name: smoke
defaults:
  http: { base_url: https://api.example.com }

scenarios:
  api:
    executor: constant-arrival-rate
    rate: 100
    duration: 5m
    pre_allocated_vus: 50
    flow:
      - request:
          url: /search?q=widgets
          extract: [ { type: jsonpath, name: first, expression: "$.results[0].id" } ]
          checks: [ { type: status, equals: 200 } ]
      - request: { url: "/items/${first}" }

thresholds:
  http_req_duration: [ "p(95)<300" ]
  http_req_failed: [ "rate<0.01" ]
```

```bash
loadr run smoke.yaml          # exit code 0 when thresholds pass, 99 when not
```

## How the pieces fit

| Component | What it does |
|---|---|
| `loadr run` | run a test locally (standalone mode) |
| `loadr controller` + `loadr agent` | distribute one test across a fleet |
| `loadr validate` | lint a test file with line/column diagnostics |
| `loadr convert` | import JMeter `.jmx` files and k6 scripts |
| `loadr report` | render an HTML report from saved results |
| Web UI | live dashboards, test editing, fleet management |
| Plugins | new protocols, outputs, extractors, assertions, services |

Continue with [Installation](getting-started/installation.md), or jump to the
[YAML reference](yaml/overview.md), the [JS API](js/api.md), or the
[migration guides](migration/from-k6.md).
