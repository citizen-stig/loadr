# Migrating from k6

Two paths, freely mixed:

1. **Automatic**: `loadr convert script.js -o test.yaml` translates options,
   scenarios, stages, thresholds, plain `http.*` calls, `check`s, `sleep`s and
   `group`s into YAML, and preserves anything it can't translate as embedded
   JS with warnings.
2. **Keep your script**: loadr's JS API is deliberately k6-shaped — many
   scripts run nearly unchanged under a thin YAML wrapper:

```yaml
js: { file: ./your-k6-script.js }
scenarios:
  default: { executor: constant-vus, vus: 10, duration: 5m, exec: default }
```

## Concept map

| k6 | loadr |
|---|---|
| `export const options = { vus, duration }` | scenario with `constant-vus` |
| `options.stages` | `ramping-vus` + `stages:` |
| `options.scenarios.<name>` | `scenarios.<name>` (same executor names) |
| `options.thresholds` | `thresholds:` (same expression syntax) |
| `import http from 'k6/http'` | works as-is |
| `check(res, {...})` | works as-is; or YAML `checks:` |
| `sleep(n)` | works as-is; or YAML `think_time` |
| `group(name, fn)` | works as-is; or YAML `group:` step |
| `Trend/Counter/Rate/Gauge` | work as-is; or YAML `metrics:` |
| `__ENV.FOO` | works as-is; or `${env.FOO}` in YAML |
| `open('data.csv')` + papaparse | `data:` block (CSV native) |
| `setup()` / `teardown()` | identical lifecycle |
| `k6 run script.js` | `loadr run test.yaml` |
| exit code 99 on threshold failure | identical |
| k6 Cloud / dashboards | built-in web UI + Prometheus/Grafana outputs |
| xk6 extensions | WASM / native plugins (no rebuild) |

## What the converter handles

`loadr convert` covers the common 90%: `vus`/`duration`/`stages`/
`iterations`, the full `options.scenarios` matrix (camelCase →
snake_case), thresholds incl. `abortOnFail`/`delayAbortEval`, `http.get/post/
put/del/patch/head/options/request` with literal URLs/bodies/headers,
`JSON.stringify` bodies, `check` patterns (status equality, `body.includes`,
duration comparisons — others become `js` conditions), `sleep` (constant and
`Math.random()` uniform), `group`, custom metric declarations, and
recognized imports.

Anything else — loops, conditionals, custom logic — is preserved verbatim in
the `js:` block and listed as a warning, so the converted test **always runs**.

## Differences to know

- **Trend values**: loadr's `res.duration_ms` ≈ k6's `res.timings.duration`.
  The converter rewrites the common forms; review custom timing math.
- **Async**: k6 scripts using top-level `await`/`http.asyncRequest` need
  restructuring into synchronous calls (QuickJS resolves returned promises,
  but the blocking API is the model).
- **Cookies**: automatic jars per VU, same as k6; the `http.cookieJar()` API
  is replaced by `session.cookieGet/Set/Clear`.
- **`handleSummary()`**: replaced by `--summary-export` + `loadr report`.
