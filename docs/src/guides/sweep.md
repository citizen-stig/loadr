# Parameter sweeps (loadr sweep)

Where does the system break — at 50 VUs, or 500? `loadr sweep` runs **one plan
across a parameter matrix** and tabulates the results side by side, so the
knee in the latency curve is one table read away.

```bash
loadr sweep perf/api.yaml --var vus=10,50,100,200
```

```text
→ sweeping 4 combination(s) of vus
→ [1/4] vus=10
  ✓ exit 0 — loadr-sweep/sweep-vus-10.json
→ [2/4] vus=50
  ✓ exit 0 — loadr-sweep/sweep-vus-50.json
...

  combo    p50      p95       p99       error rate  rps
  -------  -------  --------  --------  ----------  --------
  vus=10   18.20ms  24.90ms   31.00ms   0.00%       55.1/s
  vus=50   19.10ms  28.40ms   40.20ms   0.00%       270.8/s
  vus=100  24.70ms  61.30ms   112.5ms   0.02%       509.2/s
  vus=200  71.40ms  412.80ms  1.28s     2.31%       541.0/s
```

Each combination runs **sequentially** as a full `loadr run --quiet
--summary-export`, so combos don't contend with each other for load-generator
resources.

## Axes: `--var`

`--var name=v1,v2,...` defines one axis; repeat it and the axes multiply into
a cartesian matrix:

```bash
loadr sweep perf/api.yaml --var vus=25,50 --var duration=1m,5m   # 4 combos
```

Two variable names are special — they map onto `loadr run`'s load overrides:

| Variable | Effect |
|---|---|
| `vus` | passed as `--vus` |
| `duration` | passed as `--duration` |

Every swept variable (special or not) is also exported to the child run as an
environment variable, `LOADR_SWEEP_<NAME>` (uppercased), so a plan can consume
arbitrary axes via [`${env.*}` interpolation](../yaml/variables.md):

```yaml
variables:
  page_size: "${env.LOADR_SWEEP_PAGE_SIZE}"
scenarios:
  browse:
    executor: constant-vus
    vus: 20
    duration: 1m
    flow:
      - request: { url: "/api/items?limit=${vars.page_size}" }
```

```bash
loadr sweep perf/browse.yaml --var page_size=10,100,1000
```

`--duration 30s` on the sweep itself overrides the plan's duration for every
combo that doesn't sweep `duration` — handy for shortening a plan while you
explore.

## Outputs

- Every combo's summary lands in `--out-dir` (default `loadr-sweep/`) as
  `sweep-<combo-slug>.json` — ordinary
  [summary exports](../getting-started/cli.md#loadr-report), so
  `loadr report` and [`loadr compare`](compare.md) work on them directly.
- `--markdown sweep.md` writes the matrix as a GitHub-flavoured table.

The matrix reads `http_req_duration` / `http_req_failed` / `http_reqs`, and
falls back to any `<family>_req*` metric family — so a sweep over a
[plugin protocol](../plugins/overview.md) (e.g. `mongo_req_duration`)
tabulates the same way.

## Failures

A failing combo never aborts the sweep: it is reported, its row shows `-` (or
its numbers, if it produced a summary — e.g. a threshold failure), and the
remaining combos still run. If **any** combo exits non-zero, `loadr sweep`
exits **99** at the end; otherwise `0`.

## The overnight matrix

Sweeps are long by construction — 6 combos × 10 minutes is an hour of load.
Run the big matrix on a schedule, not on every PR:

```yaml
# .github/workflows/perf-nightly.yml
name: Nightly perf matrix
on:
  schedule: [{ cron: "0 2 * * *" }]

jobs:
  sweep:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: levantar-ai/loadr/.github/actions/setup-loadr@v1
        with: { version: latest }

      - name: Sweep
        run: |
          loadr sweep perf/api.yaml \
            --var vus=25,50,100,200 --duration 10m \
            --out-dir sweeps --markdown sweep.md

      - name: Publish matrix
        if: always()
        run: cat sweep.md >> "$GITHUB_STEP_SUMMARY"

      - name: Keep the summaries
        if: always()
        uses: actions/upload-artifact@v4
        with: { name: perf-sweep-${{ github.run_id }}, path: sweeps/ }
```

The matrix shows up in the workflow's summary page each morning, and the
per-combo JSON exports are archived — so when a knee moves, you can
`loadr compare` last week's `sweep-vus-100.json` against today's and see
exactly which percentile shifted.
