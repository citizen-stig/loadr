# GitHub Actions & JUnit reports

loadr is **CI-native**: it ships first-party GitHub Actions and emits a **JUnit
XML** report, so a load test drops straight into a pipeline and shows up in the
PR's test panel. A breached [threshold](../yaml/thresholds.md) fails the job
(exit `99` — see [exit codes](../reference/exit-codes.md)).

## Quick start

```yaml
name: Performance
on: [pull_request]

jobs:
  load-test:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4

      - name: Load test
        uses: levantar-ai/loadr@v1
        with:
          plan: perf/checkout.yaml
          version: latest

      - name: Publish results
        uses: dorny/test-reporter@v1
        if: always()
        with:
          name: loadr thresholds
          path: loadr-junit.xml
          reporter: java-junit
```

The action installs loadr, runs the plan, writes `loadr-junit.xml` and
`loadr-summary.json`, and fails the step when a threshold is breached. Any JUnit
reporter then renders the thresholds and checks as a test report.

`@v1` floats to the latest `v1.x` release; pin a full tag (e.g. `@v1.22.2`) to
freeze the version. The subdirectory form
`levantar-ai/loadr/.github/actions/run@v1` is equivalent.

### `run` inputs

| Input | Default | Description |
|---|---|---|
| `plan` | *(required)* | Path to the test plan YAML. |
| `version` | `latest` | loadr version to install (tag like `v1.21.5`, or `latest`). |
| `junit` | `loadr-junit.xml` | Where to write the JUnit report. |
| `summary` | `loadr-summary.json` | Where to write the JSON summary. |
| `args` | `''` | Extra flags passed verbatim to `loadr run` (e.g. `--vus 50 --duration 2m`). |
| `fail-on-threshold` | `true` | Set `false` to record results without failing the job. |

Outputs: `passed` (`true`/`false`), `exit-code`, `junit`, `summary`.

### Just install the CLI

If you'd rather script the run yourself, use `setup-loadr`:

```yaml
- uses: levantar-ai/loadr/.github/actions/setup-loadr@v1
  with:
    version: latest
- run: loadr run perf/api.yaml --junit loadr-junit.xml --summary-export summary.json
```

It resolves the release asset for the runner's OS/arch (Linux, macOS, Windows)
and adds `loadr` to `PATH`. Outputs: `version`, `path`.

## The JUnit report

`loadr run --junit <path>` (and `loadr report summary.json --format junit`)
render the run as JUnit XML. Each [threshold](../yaml/thresholds.md) and each
named [check](../yaml/assertions-checks.md) becomes a `<testcase>`, grouped into
`thresholds`, `checks` and `run` test suites. A failed threshold, a check with
any failing samples, or an aborted run emits a `<failure>`:

```xml
<?xml version="1.0" encoding="UTF-8"?>
<testsuites name="loadr: checkout" tests="6" failures="1" time="30.004">
  <testsuite name="thresholds" tests="3" failures="1" time="30.004">
    <testcase name="http_req_duration: p(95)&lt;500" classname="threshold"/>
    <testcase name="http_req_failed: rate&lt;0.01" classname="threshold">
      <failure message="threshold rate&lt;0.01 failed (observed: 0.04)"/>
    </testcase>
    <testcase name="checks: rate&gt;0.95" classname="threshold"/>
  </testsuite>
  <testsuite name="checks" tests="2" failures="0" time="30.004"> ... </testsuite>
  <testsuite name="run" tests="1" failures="0" time="30.004"> ... </testsuite>
</testsuites>
```

This is the shape every CI test reporter understands — GitHub Actions, GitLab,
Jenkins, CircleCI, Bamboo and Azure DevOps all ingest it directly.

## Other CI systems

You don't need the GitHub Action — `loadr run` is the whole integration. The exit
code gates the pipeline and `--junit` feeds the test panel:

```bash
# GitLab CI, Jenkins, CircleCI, ...
loadr run perf/checkout.yaml --junit loadr-junit.xml --summary-export summary.json
# non-zero exit (99) fails the stage on a breached threshold
```

```yaml
# GitLab CI: collect the report
load_test:
  script:
    - loadr run perf/checkout.yaml --junit loadr-junit.xml
  artifacts:
    when: always
    reports:
      junit: loadr-junit.xml
```

Convert an already-exported summary to JUnit after the fact (e.g. from a
distributed run) with:

```bash
loadr report summary.json --format junit --output loadr-junit.xml
```
