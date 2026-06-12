# Extraction & correlation

Extractors pull values out of a response into named variables, available to
every later step in the iteration as `${name}` and to JS as
`session.vars.name`.

```yaml
- request:
    url: /checkout/start
    extract:
      - { type: jsonpath, name: order_id, expression: "$.order.id" }
      - { type: regex,    name: csrf,     expression: 'csrf" value="([^"]+)', group: 1 }
      - { type: xpath,    name: total,    expression: "//order/total" }
      - { type: css,      name: token,    expression: "input[name=token]", attribute: value }
      - { type: boundary, name: trace,    left: 'trace="', right: '"' }
      - { type: header,   name: location, header: Location }
- request:
    url: /orders/${order_id}
    headers: { X-Trace: "${trace}" }
```

| Type | Source | Notes |
|---|---|---|
| `jsonpath` | JSON body | full JSONPath; result keeps its JSON type |
| `regex` | body text | `group` selects the capture group (default 1, 0 = whole match) |
| `xpath` | XML body | XPath 1.0 |
| `css` | HTML body | CSS selector; `attribute:` reads an attribute, otherwise element text |
| `boundary` | body text | JMeter-style left/right boundary |
| `header` | response headers | case-insensitive |

Common options:

- `default: value` — used when nothing matches. **Without a default, a failed
  extraction marks the request failed** (`http_req_failed`) and the variable
  stays unset.
- `index: first | last | random | all` — which match to take (`all` produces a
  JSON array). Supported by jsonpath, regex, css and boundary.

Extracted values are per-VU and per-iteration scoped state — they persist
across steps within the iteration and across iterations of the same VU until
overwritten.
