# Assertions & checks

The same condition types power two blocks with different consequences:

- **`assert:`** — JMeter-style assertions. A failure marks the request failed
  (`http_req_failed`) and can change control flow via `on_failure`.
- **`checks:`** — k6-style checks. Results are recorded into the `checks`
  rate metric (per-check, via the `check` tag) and *never* fail the request.
  Gate the run with a threshold: `checks: ["rate>0.99"]`.

```yaml
- request:
    url: /orders
    assert:
      - { type: status, equals: 201 }
      - { type: jsonpath, expression: "$.order.id", exists: true, on_failure: abort_iteration }
    checks:
      - { type: duration, name: fast enough, max: 250ms }
      - { type: body_contains, value: '"status":"pending"' }
```

## Condition types

| Type | Fields | Passes when |
|---|---|---|
| `status` | `equals`, `one_of: [..]`, `matches: "2.."` | status code matches |
| `body_contains` | `value`, `negate` | body contains (or not) the substring |
| `body_matches` | `pattern`, `negate` | body matches the regex |
| `jsonpath` | `expression`, `equals`, `exists` | match exists (default) / equals the JSON value |
| `xpath` | `expression`, `equals`, `exists` | XPath 1.0 result |
| `duration` | `max` | response duration ≤ max |
| `size` | `min`, `max`, `equals` | body size in bounds |
| `header` | `header`, `equals`, `contains`, `exists` | header present/matching |
| `js` | `expression` | the JS expression is truthy (`response` is in scope) |

All take an optional `name` (used in reports; a sensible one is generated
otherwise) and, in `assert:` blocks, `on_failure`:

| `on_failure` | Effect |
|---|---|
| `continue` (default) | record the failure, keep going |
| `abort_iteration` | skip the rest of this iteration |
| `abort_scenario` | stop this scenario |
| `abort_test` | stop the whole run (exit code reflects failure) |

## JS conditions

```yaml
checks:
  - type: js
    name: balanced response
    expression: "response.json ? true : JSON.parse(response.body).items.length > 0"
```

The `response` object has `status`, `body`, `headers` (lower-cased),
`duration_ms`, `url`, `error`, `protocol`.
