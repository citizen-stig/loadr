# loadr GitHub Actions

First-party actions that make loadr **CI-native**: install the CLI on a runner,
run a test plan, and turn thresholds + checks into a **JUnit XML** report that
GitHub (and every other CI) renders in its test panel. A breached threshold
fails the job.

## `levantar-ai/loadr@v1`

The Marketplace action — install loadr, run a plan, write a JUnit report and JSON
summary, and fail the job on a threshold breach.

```yaml
- name: Load test
  uses: levantar-ai/loadr@v1
  with:
    plan: tests/checkout.yaml
    version: latest          # or a tag like v1.21.5
    junit: loadr-junit.xml   # default
    summary: loadr-summary.json
    # args: "--vus 50 --duration 2m"   # extra `loadr run` flags
    # fail-on-threshold: 'false'        # record without failing the job
```

`@v1` floats to the latest `v1.x` release; pin a full tag like `@v1.22.2` for a
fixed version. The subdirectory form
`levantar-ai/loadr/.github/actions/run@v1` is identical and still supported.

Surface the results in the PR's checks tab with any JUnit reporter, e.g.
[`dorny/test-reporter`](https://github.com/dorny/test-reporter):

```yaml
- uses: dorny/test-reporter@v1
  if: always()
  with:
    name: loadr thresholds
    path: loadr-junit.xml
    reporter: java-junit
```

Outputs: `passed` (`true`/`false`), `exit-code`, `junit`, `summary`.

## `levantar-ai/loadr/.github/actions/setup-loadr`

Just install the CLI and add it to `PATH` — then script the rest yourself.

```yaml
- uses: levantar-ai/loadr/.github/actions/setup-loadr@v1
  with:
    version: latest
- run: loadr run tests/api.yaml --junit loadr-junit.xml
```

Outputs: `version`, `path`.

Both actions run on Linux, macOS and Windows runners. `setup-loadr` resolves the
release asset for the runner's OS/arch and downloads it from the loadr GitHub
releases.
