# JUnit report plugin

`loadr-plugin-junit-report` is a **native output plugin**: at the end of a run
it turns every `check` and `threshold` into a JUnit `<testcase>` and writes a
`junit.xml` file that CI systems (Jenkins, GitLab, GitHub Actions, CircleCI,
Azure Pipelines, …) can ingest into their native test panel. It is not built
into loadr core — install it, then wire it into a plan's `outputs:` list.

The plugin is **pure Rust**: it buffers the pass/fail outcome of each check and
threshold as the run progresses and, in `finish()`, renders them with a small
hand-rolled XML builder straight to disk. There is **no external test-reporter
binary and no XSLT** — just file writing. It is modelled directly on the shipped
[`native-output` example](native.md) (`file-report`), following the same
`start` / `on_snapshot` / `finish` lifecycle, so if you have read that plugin
this one will feel familiar.

> **Status:** planned. The design is fixed and the implementation is plain Rust
> file writing, but the plugin is not part of a published release yet. loadr
> already ships a built-in `--junit <path>` flag (and `loadr report … --format
> junit`) that covers the common case — see [CI: GitHub
> Actions](../ci/github-actions.md); this plugin is the same idea expressed
> through the output-plugin model. Track it before depending on it in CI.

## Build and install

```bash
cargo build -p loadr-plugin-junit-report --release

# `loadr plugin install` copies a directory that holds plugin.toml next to the
# artifact named by its `entry`. Stage the built cdylib beside the manifest:
mkdir -p dist
cp plugins/loadr-plugin-junit-report/plugin.toml dist/
cp target/release/libloadr_plugin_junit_report.so dist/   # .dylib on macOS, .dll on Windows
loadr plugin install dist
loadr plugin info junit-report
```

Installing copies `plugin.toml` and the artifact into
`~/.loadr/plugins/junit-report/` (override with `LOADR_PLUGINS_DIR` or
`--plugins-dir`). The manifest declares an output plugin with a default output
path:

```toml
[plugin]
name = "junit-report"
kind = "output"
type = "native"
entry = "libloadr_plugin_junit_report.so"
description = "Maps checks and thresholds to JUnit testcases and writes junit.xml"

[config]
path = "junit.xml"
```

## Use it in a test

An output plugin is wired in through the plan's `outputs:` list as a
`type: plugin` entry, naming the installed plugin and passing its `config`
straight through to `start`:

```yaml
name: checkout-load

outputs:
  - type: plugin
    name: junit-report
    config:
      path: junit.xml          # where to write the report (default: junit.xml)

scenarios:
  main:
    executor: constant-vus
    vus: 50
    duration: 10m
    flow:
      - request: { name: list, url: https://api.example.com/items }
      - request:
          name: checkout
          url: https://api.example.com/checkout
          method: POST
          checks:
            - { type: status, equals: 200 }
            - { type: body_contains, value: order_id }

thresholds:
  http_req_duration: [ "p(95)<500ms" ]
  http_req_failed:   [ "rate<0.01" ]
```

You can run any number of outputs alongside it — a `json` archive or a
`prometheus` scrape endpoint next to the JUnit export, for example. The plugin's
simple form is also reachable ad hoc from the CLI with
`--output plugin=junit-report` when the `config` (the `path`) lives in the plan.

## Config reference

The object under `config:` is handed to the plugin's `start` as JSON.

| Key | Type | Default | Meaning |
|---|---|---|---|
| `path` | string | `junit.xml` | Where the report is written. Created (and truncated) in `start`; the buffered `<testsuite>` is flushed to it in `finish`. An empty `path` fails `start`. |

`${env.…}` and other interpolation resolve before the config reaches the
plugin, so a per-branch filename can flow in from the environment.

## What gets written

Each **check** and each **threshold** becomes one `<testcase>` under a single
`<testsuite>`:

- a **passing** check/threshold is an empty `<testcase>` (a green test);
- a **failing** one carries a `<failure>` child whose message names the
  condition that broke (the check expression, or the threshold expression and
  the value it was compared against);
- the `<testsuite>` attributes (`tests`, `failures`, `time`) are filled from
  the end-of-run summary, and the run's `run_id` rides along as a property so
  concurrent runs stay distinguishable.

`start` opens (and truncates) the file and validates the config; `on_samples`
and `on_snapshot` are no-ops — nothing is written mid-run. Only `finish`
renders the XML, so the file appears complete-and-valid or not at all, never
half-written.

## Metrics

The plugin reports its own health back into the run so a reporting problem is
visible in loadr's summary rather than silently producing an empty file:

| Metric | Kind | Meaning |
|---|---|---|
| `junit_testcases` | counter | Total `<testcase>` entries written (checks + thresholds) |
| `junit_failures` | counter | Of those, how many carried a `<failure>` (failed checks/thresholds) |

A green CI run shows `junit_failures` at zero and `junit_testcases` equal to the
number of checks plus thresholds in the plan. A non-zero `junit_failures`
mirrors the run's own pass/fail state, so the JUnit panel and loadr's exit code
agree.

## Notes

- **The JUnit file is the report, not the gate.** loadr's own exit code (driven
  by `thresholds`) is what should fail the pipeline; the `junit.xml` feeds the
  test panel so humans can see *which* check or threshold broke. Pair it with
  `--summary-export results.json` for the machine-readable timeline.
- **Built-in first.** For the common case, loadr's built-in `loadr run --junit
  junit.xml` (and `loadr report summary.json --format junit`) writes the same
  shape without installing anything — see
  [CI: GitHub Actions](../ci/github-actions.md). Reach for this plugin when you
  want the report emitted through the output-plugin pipeline alongside other
  `outputs:` entries.
- **Overwrites, not appends.** Unlike the `file-report` example it is modelled
  on, `junit.xml` is truncated at `start`, so re-running in the same workspace
  replaces the previous report rather than appending to it.
- **One suite per run.** All checks and thresholds land in a single
  `<testsuite>`; there is no per-scenario or per-request nesting.
