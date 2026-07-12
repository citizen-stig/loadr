# CI/CD performance gate

Run loadr in CI and **fail the build on a performance regression** — the
loadr equivalent of Artillery's `cicd` example.

## Files
- `perf-smoke.yaml` — a 30-second gate with latency/error/check thresholds.
- `github-actions.yml` — a workflow using the official `levantar-ai/loadr@v1`
  Action. Copy it to `.github/workflows/perf-gate.yml`.

## How the gate works
The thresholds in `perf-smoke.yaml` **are** the gate. If p95/p99 latency, the
error rate, or the check pass-rate regress past their limits, `loadr run` exits
non-zero, the Action's `fail-on-threshold` (default `true`) fails the job, and
the merge is blocked. The Action also writes a JUnit report (`loadr-junit.xml`)
and a JSON summary (`loadr-summary.json`), uploaded here as artifacts.

## Configure
- Set repo/Environment variable `STAGING_URL` (mapped to `TARGET_URL`), or
  point `base_url` at any reachable target.
- Pin `version:` to a specific tag (e.g. `v1.23.0`) for reproducible CI instead
  of `latest`.

## Other CI systems
The Action is convenience sugar over the CLI — any runner works:
```bash
curl -sSL https://github.com/levantar-ai/loadr/releases/latest/download/loadr-x86_64-unknown-linux-gnu.tar.gz | tar xz
TARGET_URL=https://staging.example.com ./loadr run examples/cicd/perf-smoke.yaml --junit out.xml
# exit code is non-zero (99) when a threshold fails → your job goes red
```
