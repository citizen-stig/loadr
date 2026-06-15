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

## Fused check-chains

A **chain** does extract → coerce type → transform → validate → save in one
declarative step (Gatling-style), so you do not have to spread a single value's
handling across an `extract:` entry, a JS hook and a `checks:` entry. Chains
appear in the same `extract:` list as the classic extractors and can be mixed
with them freely.

```yaml
- request:
    url: /inventory
    extract:
      # The `chain:` key is the variable name to save under.
      - chain: cheapest_name
        jmespath: "items | sort_by(@, &price)[0].name"   # one source
        as: string                                        # optional: coerce
        transform: [trim, uppercase]                      # optional: pipeline
        check:                                            # optional: validate
          not_empty: true
          matches: "^[A-Z]+$"
        default: NONE                                     # optional: fallback
- request:
    url: /items/${cheapest_name}
```

### Source — pick exactly one

A chain reads from one source; the field name *is* the source type:

| Field | Source | Notes |
|---|---|---|
| `jmespath` | JSON body | [JMESPath] query/transform language (filters, projections, functions) |
| `jsonpath` | JSON body | full JSONPath; result keeps its JSON type |
| `regex` | body text | `group:` selects the capture group (default 1) |
| `header` | response headers | case-insensitive header name |
| `css` | HTML body | CSS selector; `attribute:` reads an attribute, else element text |
| `xpath` | XML body | XPath 1.0 |
| `left` + `right` | body text | JMeter-style boundary extractor |

`index: first | last | random | all` chooses which match to take when the
source yields several (`all` produces a JSON array). For `jmespath`, `index`
applies when the query itself returns an array.

[JMESPath]: https://jmespath.org/

### `as:` — coerce the type

Coerce the raw value before transforming/validating it: `int`, `float`, `bool`
or `string`. Numeric and boolean strings are parsed (`"7"` → `7`, `"yes"` →
`true`); `bool` accepts `true/false`, `1/0`, `yes/no`, `on/off`. JSONPath and
JMESPath already keep native JSON types, so `as:` is mainly for the text-based
sources (regex, header, css, …) or to normalise a stringly value.

### `transform:` — an ordered pipeline

Each transform runs in order and yields a string. String forms take no
argument; object forms carry one:

| Transform | Effect |
|---|---|
| `trim` | strip surrounding whitespace |
| `lowercase` / `uppercase` | change case |
| `url_encode` / `url_decode` | percent-encoding |
| `base64_encode` / `base64_decode` | standard base64 |
| `{ append: "..." }` / `{ prepend: "Bearer " }` | concatenate a literal |
| `{ replace: [from, to] }` | replace all occurrences |
| `{ substring: [start, len] }` | character-offset substring (`len` optional) |

```yaml
- chain: auth
  header: X-Token
  transform: [trim, { prepend: "Bearer " }]   # "  abc " -> "Bearer abc"
```

### `check:` — validate before saving

Every set constraint must hold or the chain fails. A failing chain check is
recorded to the `checks` metric (just like a standalone `checks:` entry) **and**
marks the request failed; `on_failure:` controls flow exactly like an assertion.

| Key | Meaning |
|---|---|
| `equals` | value must equal this (compared after coerce/transform) |
| `matches` | value (as text) must match this regex |
| `one_of` | value must be one of these |
| `min` / `max` | numeric bounds (inclusive) |
| `not_empty` | value (as text) must be non-empty |
| `on_failure` | `continue` (default) · `abort_iteration` · `abort_scenario` · `abort_test` |

```yaml
- chain: order_status
  jsonpath: "$.status"
  transform: [lowercase]
  check:
    one_of: [pending, paid, shipped, delivered]
    on_failure: abort_iteration
  default: pending
```

### `default:`

As with the classic extractors, `default:` supplies a value when the source
matches nothing. Without one, a no-match marks the request failed and leaves the
variable unset. The default is still coerced, transformed and validated.

A complete runnable example lives in
[`examples/25-check-chains.yaml`](https://github.com/levantar-ai/loadr/blob/main/examples/25-check-chains.yaml).
