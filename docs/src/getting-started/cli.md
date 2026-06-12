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
loadr run --controller host:7625 test.yaml  # submit to a controller fleet
```

| Exit code | Meaning |
|---|---|
| 0 | run finished, all thresholds passed |
| 1 | error (invalid test, I/O, ...) |
| 99 | run finished but thresholds failed (k6-compatible) |
| 130 | interrupted (Ctrl-C twice; first Ctrl-C stops gracefully) |

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

Produces a self-contained HTML file: metric tables, latency percentiles,
check and threshold outcomes — shareable with people who don't run loadr.
