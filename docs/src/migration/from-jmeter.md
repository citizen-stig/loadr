# Migrating from JMeter

```bash
loadr convert test-plan.jmx -o converted.yaml
loadr validate converted.yaml
loadr run converted.yaml
```

The converter parses JMeter 5.x plans and emits clean YAML, with a warning
for every element it couldn't translate (disabled elements, plugins,
`${__functions}`, complex controllers).

## Concept map

| JMeter | loadr |
|---|---|
| Thread Group (threads, ramp-up, duration) | scenario: `constant-vus` / `ramping-vus` |
| Thread Group with loop count | `per-vu-iterations` |
| Multiple Thread Groups | multiple scenarios (run concurrently) |
| HTTP Request sampler | `request:` step |
| HTTP Header Manager | `headers:` (request- or defaults-level by scope) |
| HTTP Cookie Manager | `defaults.http.cookies: true` (default) |
| CSV Data Set Config | `data:` block |
| User Defined Variables | `variables:` |
| Constant / Uniform / Gaussian Random Timer | `think_time:` (same three types) |
| Constant Throughput Timer | `pacing:` (per-minute → per-second) |
| Response Assertion | `assert:` `status` / `body_contains` / `body_matches` |
| Duration / Size Assertion | `assert:` `duration` / `size` |
| JSON / XPath Assertion | `assert:` `jsonpath` / `xpath` |
| Regular Expression Extractor | `extract:` `regex` (incl. match no. → `index`) |
| JSON / XPath / Boundary Extractor | `extract:` `jsonpath` / `xpath` / `boundary` |
| CSS Selector Extractor | `extract:` `css` |
| Transaction Controller | `group:` step |
| Loop Controller | steps replicated (≤10) or warning |
| Backend Listener (InfluxDB/Graphite) | `outputs:` influxdb / prometheus / statsd |
| Aggregate Report / HTML dashboard | console summary + `loadr report` + web UI |
| Distributed testing (RMI, jmeter-server) | `loadr controller` / `loadr agent` (gRPC, mTLS) |
| BeanShell / JSR223 / Groovy | embedded JavaScript |

## What changes for the better

- **Percentiles are exact** (HDR histograms), including across the fleet —
  JMeter's distributed mode ships raw samples or averages, loadr merges
  histograms.
- **Open-model load**: JMeter's thread-based model can't hold a target
  request rate when the system slows down; `constant-arrival-rate` can.
- **Code review-able tests**: YAML diffs instead of 4000-line XML.
- No JVM tuning, no plugin manager, one binary.

## What needs hand-porting

- JMeter **plugins** (custom samplers etc.) → loadr protocol plugins.
- `${__time()}`, `${__Random()}`, `${__UUID()}` and friends → `${js: ...}`
  one-liners (`Date.now()`, `Math.random()`, `crypto.uuidv4()`). The
  converter flags each occurrence.
- **If/While/Switch controllers** → JS scenario functions (`exec:`), where
  real control flow is natural.
- **Module/Include controllers** → split scenarios across files and compose
  with environments or separate tests.
