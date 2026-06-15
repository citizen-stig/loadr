# The CLI

```text
loadr <COMMAND>

Commands:
  run          Run a test (standalone, or submit to a controller)
  validate     Validate test files and print diagnostics
  convert      Convert JMeter .jmx or k6 .js files to loadr YAML
  controller   Run the distributed-mode controller
  agent        Run a load-generating agent
  plugin       List, install, enable, disable and inspect plugins
  report       Render an HTML report from a summary JSON file
  schema       Print the JSON Schema for test definitions
  completions  Generate shell completions
  version      Print version information
```

Global flags: `-q/--quiet` (errors only), `-v/--verbose` (repeat for more),
`--no-color`.

## `loadr run`

```bash
loadr run test.yaml                         # run locally
loadr run -e staging test.yaml              # apply the env.staging overrides
loadr run --vus 50 --duration 2m test.yaml  # override single-scenario load
loadr run --ui test.yaml                    # serve the live web UI during the run
loadr run --summary-export out.json test.yaml
loadr run --output json=samples.jsonl test.yaml   # ad-hoc output (repeatable)
loadr run --quiet test.yaml                 # summary only, no live progress
loadr run --controller host:6464 test.yaml  # submit via the controller's API port
```

| Exit code | Meaning |
|---|---|
| 0 | run finished, all thresholds passed |
| 1 | error (invalid test, I/O, ...) |
| 99 | run finished but thresholds failed (k6-compatible) |
| 130 | interrupted (Ctrl-C twice; first Ctrl-C stops gracefully) |

### Selecting scenarios by tag

Tag scenarios in YAML with the scenario-level `tags` map, then run only the
ones you want with `--tags` / `--exclude-tags`. (These same tags are also
attached to the scenario's metric samples.)

```yaml
scenarios:
  smoke_read:
    executor: shared-iterations
    vus: 2
    iterations: 10
    tags: { suite: smoke, kind: read }     # name → value pairs
    flow: [ { request: { url: /api/v1/items } } ]
  full_write:
    executor: ramping-vus
    stages: [ { duration: 1m, target: 30 } ]
    tags: { suite: full, kind: write }
    flow: [ ... ]
```

The filter matches against tag **values**, not the tag names:

- `--tags a,b` — keep a scenario if it carries **at least one** of these values
  (any-match / OR). Omit `--tags` to start from every scenario.
- `--exclude-tags a,b` — drop a scenario if it carries **any** of these values.
- Exclude always wins: a scenario matched by both `--tags` and `--exclude-tags`
  is dropped.
- Both flags take a comma-separated list and may be repeated.
- If the filter leaves no scenarios, the run fails with an error rather than
  running nothing.

```bash
loadr run --tags smoke test.yaml                 # only scenarios tagged `smoke`
loadr run --tags read,write test.yaml            # tagged `read` OR `write`
loadr run --tags full --exclude-tags write test.yaml  # full load, reads only
loadr run --exclude-tags smoke test.yaml         # everything except smoke
```

After filtering, loadr prints how many of the original scenarios remain
(unless `--quiet`).

## `loadr validate`

```console
$ loadr validate broken.yaml
error at line 12, column 5 (scenarios.api.executor): `constant-arrival-rate` requires `pre_allocated_vus`
error at line 18, column 9 (scenarios.api.flow[0].request.url): `${vars.api_kye}` is not defined under `variables:` — did you mean `api_key`?
2 error(s), 0 warning(s)
```

`--format json` emits diagnostics as JSON for editor/CI integration.

## `loadr convert`

```bash
loadr convert plan.jmx -o converted.yaml
loadr convert k6-script.js -o converted.yaml
```

Conversion warnings (unsupported constructs, things to review) print to
stderr; the output always passes `loadr validate`.

## `loadr plugin`

```bash
loadr plugin list                      # discovered plugins + enabled state
loadr plugin install ./my-plugin-dir  # copy into the plugins directory
loadr plugin info my-extractor
loadr plugin disable my-extractor
loadr plugin enable my-extractor
```

The plugins directory is `~/.loadr/plugins` (override with
`LOADR_PLUGINS_DIR` or `--plugins-dir`).

## `loadr report`

```bash
loadr run --summary-export results.json test.yaml
loadr report results.json -o report.html
```

Produces a self-contained HTML file: interactive time-series charts
(throughput, latency p50/p95/p99, active VUs, error rate) plus the aggregate
metric tables, latency percentiles, and check and threshold outcomes —
shareable with people who don't run loadr. No network assets; the charts are
inline SVG drawn by a small inline script. See [HTML reports](../reporting.md)
for the chart details and the `timeline` schema.
