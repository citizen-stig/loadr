# Payload generation & complexity testing

Real systems rarely fall over on *typical* input. They fall over on input
**crafted** to hit a super-linear code path: a deeply-nested document that sends
a parser quadratic, an expansion bomb that turns a kilobyte into a gigabyte, a
string that drives a validator's regex into catastrophic backtracking. A load
test that only replays realistic traffic will never find these.

`loadr payload` generates those adversarial inputs, each parameterised by a
single **magnitude** (a depth, count, byte length or level count) so you can
*scale* one input and watch what the response time does. Feed that scaling axis
to `loadr sweep --complexity` and it fits the response-time-vs-size curve, tells
you the algorithmic order (`O(n^k)`), and fails CI when the exponent crosses a
bound you set. Linear scaling passes; a parser that goes quadratic on depth is a
denial-of-service waiting to happen, and the gate catches it.

> **Responsible use.** These payloads are designed to exhaust CPU and memory on
> the receiving end — that is the whole point. Only ever aim them at systems you
> own or are explicitly authorised to test. Pointed at a shared or production
> target they are indistinguishable from a DoS attack. Start with small
> magnitudes against a disposable environment.

## `loadr payload`

```bash
loadr payload <kind>[:<magnitude>]
```

Writes the raw payload **bytes to stdout** so it pipes cleanly into `curl`,
`xargs` or a file. Omit `:<magnitude>` to use the kind's default; exceed the
kind's safety cap and it refuses rather than trying to allocate the machine to
death.

```bash
# A JSON object nested 10,000 levels deep, straight to a request body.
loadr payload nested-json:10000 | curl -X POST http://localhost:3001/api/parse \
  -H 'Content-Type: application/json' --data-binary @-

# Write a 64k-deep markdown blockquote bomb to a file.
loadr payload nested-markdown-blockquote:64000 -o bomb.md
```

| Flag | Meaning |
|---|---|
| `-o`, `--output <file>` | Write to a file instead of stdout (prints a one-line summary: kind, magnitude, content-type, byte count). |
| `--list` | Print the whole catalog — category, parameter, default and cap — then exit. |

### `loadr payload --list`

```bash
loadr payload --list
```

lists every kind grouped by category. Each kind advertises the `param` its
magnitude controls, a `default` used when you omit `:n`, and a hard `max`
safety cap.

## The catalog

Eighteen kinds across seven categories. The **param** column is what the
magnitude means for that kind, and **stresses** is the code path it targets.

### nesting — deep structure → super-linear parsers

| Kind | param | default | max | Content-Type | Stresses |
|---|---|---|---|---|---|
| `nested-json` | depth | 10000 | 5000000 | `application/json` | Deeply nested JSON object `{"a":{"a":…}}` — recursive-descent / stack-depth parsing. |
| `nested-array` | depth | 10000 | 5000000 | `application/json` | Deeply nested JSON array `[[[…]]]` — same parser stress via array nesting. |
| `nested-markdown-blockquote` | depth | 50000 | 5000000 | `text/markdown` | One line of N blockquote markers (`>>>…`) — the goldmark-class super-quadratic blowup. |
| `nested-markdown-bracket` | depth | 50000 | 5000000 | `text/markdown` | Unmatched nested link brackets `[[[…]]]` — inline link/reference backtracking. |
| `nested-xml` | depth | 20000 | 5000000 | `application/xml` | Deeply nested XML elements `<a><a>…</a></a>` — stack/tree-depth parser stress. |
| `nested-html` | depth | 20000 | 5000000 | `text/html` | Deeply nested `<div>` tags — HTML parsers and sanitizers walking a deep tree. |
| `nested-parens` | depth | 50000 | 5000000 | `text/plain` | Balanced nested parentheses `((((…))))` — expression/formula/filter grammars. |
| `nested-graphql` | depth | 2000 | 200000 | `application/json` | Deeply nested GraphQL selection `{a{a{…}}}` — query validation / depth limiting. |

### amplification — small in → huge out

| Kind | param | default | max | Content-Type | Stresses |
|---|---|---|---|---|---|
| `billion-laughs` | levels | 9 | 12 | `application/xml` | Classic XML entity-expansion bomb — ~10^levels expansion from a tiny document. |
| `yaml-alias-bomb` | levels | 10 | 24 | `application/x-yaml` | Exponential YAML anchor/alias expansion — 2^levels blowup. |

### volume — allocation / O(n²) stress

| Kind | param | default | max | Content-Type | Stresses |
|---|---|---|---|---|---|
| `json-array` | count | 1000000 | 50000000 | `application/json` | A flat JSON array of N integers — allocation, GC and per-element processing. |
| `json-object-keys` | count | 1000000 | 20000000 | `application/json` | A JSON object with N distinct keys — hashmap-build and key-processing stress. |
| `long-string` | bytes | 10000000 | 200000000 | `application/json` | A single JSON string of N bytes — copy/scan/validation cost in one enormous field. |
| `csv-rows` | count | 1000000 | 50000000 | `text/csv` | A CSV with N rows — row-parsing throughput and streaming behaviour. |

### regex — catastrophic backtracking (ReDoS)

| Kind | param | default | max | Content-Type | Stresses |
|---|---|---|---|---|---|
| `redos` | bytes | 50000 | 10000000 | `text/plain` | `'aaaa…!'` — drives `(a+)+$`-style vulnerable validators into exponential backtracking. |

### unicode — normalization / grapheme cost

| Kind | param | default | max | Content-Type | Stresses |
|---|---|---|---|---|---|
| `zalgo` | count | 100000 | 20000000 | `text/plain` | A base char with N stacked combining marks — normalization / width / grapheme cost. |

### numeric — slow number parsing

| Kind | param | default | max | Content-Type | Stresses |
|---|---|---|---|---|---|
| `bignum` | count | 100000 | 50000000 | `application/json` | A bare integer with N digits — bignum / arbitrary-precision parse cost. |

### collision — worst-case hashmaps

| Kind | param | default | max | Content-Type | Stresses |
|---|---|---|---|---|---|
| `hash-collision` | count | 65536 | 1000000 | `application/json` | A JSON object whose N keys all collide in 31-based string hashing — O(n²) map inserts. |

## Payloads in a plan: `${payload:…}`

Instead of piping bytes at the shell, embed a payload directly in a request body
(or any templated field) with the `${payload:<kind>:<magnitude>}` template. The
body is generated at request time and never materialised into VU state, so even
a gigabyte payload costs nothing to hold.

```yaml
- request:
    method: POST
    url: /api/parse
    headers: { Content-Type: application/json }
    body: '${payload:nested-json:20000}'
```

The magnitude may be a literal, or a `$ENVVAR` reference that reads an
environment variable at request time:

```yaml
body: '${payload:nested-markdown-blockquote:$LOADR_SWEEP_DEPTH}'
```

That `$ENVVAR` form is the hook for scaling. `loadr sweep --var depth=…` exports
each value as `LOADR_SWEEP_DEPTH` (the `LOADR_SWEEP_<NAME>` convention — see
[Parameter sweeps](sweep.md)), so the same plan runs at every depth on the axis
with no edits. Only the magnitude after the last `:` is expanded; the kind name
is left untouched. An unset variable resolves to empty and the payload spec then
fails to parse — the honest signal that your sweep axis wasn't exported.

## Fitting complexity: `loadr sweep --complexity`

`loadr sweep` gained two flags for turning a size sweep into a complexity
verdict:

| Flag | Meaning |
|---|---|
| `--complexity <AXIS>` | Treat this swept axis as an **input size** and fit the exponent `k` in `response-time ≈ size^k`. The axis values must be numeric. |
| `--max-exponent <K>` | Fail (**exit 99**) if the fitted exponent exceeds `K` — e.g. `1.2` flags worse-than-quasilinear scaling. Implies `--complexity`. |

The fit is a log-log least-squares regression of p95 `http_req_duration`
against the size axis, done per group of combos that share every *other* axis.
The resulting `k` is labelled:

| Fitted `k` | Verdict |
|---|---|
| `< 0.5` | flat / sub-linear |
| `< 1.2` | ≈ linear |
| `< 1.6` | super-linear |
| `< 2.4` | ≈ quadratic ⚠ DoS risk |
| `≥ 2.4` | super-quadratic ⚠⚠ DoS |

## Worked example

`examples/49-payload-complexity.yaml` scales a nested-markdown blockquote bomb
against a markup-rendering endpoint. The body's depth is the swept `depth` axis,
read at request time from `LOADR_SWEEP_DEPTH`:

```yaml
name: payload-complexity-probe
description: scale a nested-markdown payload and fit the target's complexity exponent

defaults:
  http:
    base_url: http://localhost:3001

scenarios:
  render:
    executor: per-vu-iterations
    vus: 1
    iterations: 4
    flow:
      - request:
          name: render
          method: POST
          url: /api/v1/markup
          headers: { Content-Type: application/json }
          # ${payload:<kind>:$ENVVAR} — depth comes from the swept axis.
          body: '{"Text": "${payload:nested-markdown-blockquote:$LOADR_SWEEP_DEPTH}", "Mode": "markdown", "Context": "x/y"}'
          checks: [ { type: status, equals: 200 } ]
```

Sweep the depth axis and gate the exponent at 1.2:

```bash
loadr sweep examples/49-payload-complexity.yaml \
  --var depth=4000,8000,16000,32000,64000 \
  --complexity depth --max-exponent 1.2
```

Against a parser that walks the blockquote nesting quadratically, the run ends
like this:

```text
→ sweeping 5 combination(s) of depth
→ [1/5] depth=4000
  ✓ exit 0 — loadr-sweep/sweep-depth-4000.json
→ [2/5] depth=8000
  ✓ exit 0 — loadr-sweep/sweep-depth-8000.json
...

  combo        p50      p95       p99       error rate  rps
  -----------  -------  --------  --------  ----------  -------
  depth=4000   41.20ms  52.90ms   61.00ms   0.00%       18.9/s
  depth=8000   150.4ms  178.2ms   190.1ms   0.00%       5.4/s
  depth=16000  590.7ms  651.0ms   690.4ms   0.00%       1.5/s
  depth=32000  2.35s    2.61s     2.74s     0.00%       0.4/s
  depth=64000  9.41s    10.30s    10.80s    0.00%       0.1/s

complexity (response time vs depth)
  O(n^1.99)  ≈ quadratic ⚠ DoS risk
    4.0k→52.90ms  8.0k→178.20ms  16.0k→651.00ms  32.0k→2.61s  64.0k→10.30s
✗ fitted exponent O(n^1.99) exceeds the --max-exponent 1.20 bound
```

Every 2× in depth roughly 4×s the latency — quadratic — so the fit lands near
`k ≈ 2`, past the `1.2` bound, and `loadr sweep` **exits 99**. Wired into CI,
that turns "our markdown renderer is O(n²) on nesting depth" from a production
incident into a failed check on the PR that introduced it.

A healthy, linear-scaling endpoint reports the opposite:

```text
complexity (response time vs depth)
  O(n^1.03)  ≈ linear
    4.0k→41.10ms  8.0k→82.40ms  16.0k→165.20ms  32.0k→331.00ms  64.0k→660.10ms
✓ O(n^1.03) within the --max-exponent 1.20 bound
```

## In CI

Complexity probes are cheap — a single VU walking a handful of magnitudes — so
unlike a full load [sweep](sweep.md#the-overnight-matrix) they belong on every
PR:

```yaml
      - name: Complexity gate
        run: |
          loadr sweep examples/49-payload-complexity.yaml \
            --var depth=4000,8000,16000,32000,64000 \
            --complexity depth --max-exponent 1.2
```

The step fails on exit 99, blocking a merge that makes a parser scale worse than
quasilinear. Point the same pattern at a `nested-json`, `redos` or
`hash-collision` body to guard whichever code path you care about — and see the
[Exit codes reference](../reference/exit-codes.md) for wiring the gate into a
pipeline.
