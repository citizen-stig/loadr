# Comparing runs (loadr compare)

`loadr compare` diffs two summary exports and tells you what got worse — the
missing piece between "the thresholds passed" and "the PR made checkout 20%
slower". Feed it a baseline and a current run, get a direction-aware delta
table, and (in CI) gate the pipeline on regressions.

```bash
loadr run perf/api.yaml --summary-export baseline.json    # e.g. on main
loadr run perf/api.yaml --summary-export current.json     # e.g. on the PR
loadr compare baseline.json current.json
```

```text
  metric             field       baseline  current   delta     delta %
  -----------------  ----------  --------  --------  --------  --------  -
  http_req_duration  avg         52.10ms   55.80ms   +3.70ms   ▲ 7.1%
  http_req_duration  p50         41.00ms   42.30ms   +1.30ms   ▲ 3.2%
  http_req_duration  p95         98.40ms   131.20ms  +32.80ms  ▲ 33.3%   ✗
  http_req_duration  p99         180.00ms  184.10ms  +4.10ms   ▲ 2.3%    ✓
  http_reqs          count       30124     29891     -233      ▼ 0.8%
  http_reqs          per_second  1004.1/s  996.4/s   -7.8/s    ▼ 0.8%
  http_req_failed    error_rate  0.20%     0.21%     +0.01%    ▲ 5.0%    ✓
  checks             pass_rate   99.80%    99.75%    -0.05%    ▼ 0.1%    ✓
  thresholds         passed      pass      pass      -         -         ✓

✗ 1 regression(s) beyond tolerance
```

Both files come from `loadr run --summary-export` (local or distributed runs
alike). Metrics present in only one file are skipped.

## What is compared

| Metric kind | Fields | Worse direction |
|---|---|---|
| trend (latency) | `avg`, `p50`, `p95`, `p99` | up |
| counter | `count`, `per_second` | down for `iterations` / `*_reqs` (throughput); neutral otherwise |
| rate | `error_rate` (failure rates) / `rate`, as percent | error rate: up |
| checks | one merged `pass_rate` row across all checks | down |
| thresholds | one `passed` row | a newly failing threshold is **always** a regression |

Improvements are never regressions — the gate only fires on the worse
direction. Gauges (`vus`, …) describe configuration, not performance, and are
excluded.

## Tolerances

By default four fields gate at a **5% relative** tolerance: `p95`, `p99`,
`error_rate` and `pass_rate`. Everything else is informational until you gate
it explicitly with `--max-regression` (repeatable):

```bash
loadr compare baseline.json current.json \
  --max-regression p95=10% \
  --max-regression error_rate=0.5 \
  --max-regression http_req_duration.p99=25 \
  --max-regression rps=5%
```

- `field=limit` applies to every metric exposing the field;
  `metric.field=limit` scopes it to one metric — the scoped spec wins.
- `10%` is relative to the baseline. A bare number is **absolute** in the
  field's display unit: milliseconds for latency, percentage points for
  rates (`error_rate=0.5` allows +0.5pp).
- Aliases: `med` = `p50`, `rps` = `per_second`, `checks` = `pass_rate`.
- The sign is ignored — a tolerance is always a magnitude of allowed
  worsening, so `rps=-5%` and `rps=5%` both mean "at most 5% throughput drop".

An explicit spec also *enables* gating on fields that are informational by
default (`avg`, `count`, `per_second`, …).

## Outputs

```bash
loadr compare baseline.json current.json \
  --output compare.json \       # machine-readable rows
  --markdown compare.md         # GitHub-flavoured table for a PR comment
```

The markdown version bolds regressed cells and ends with a one-line verdict —
paste-ready for a PR comment:

| Metric | Field | Baseline | Current | Δ | Δ% | Verdict |
| --- | --- | --- | --- | --- | --- | --- |
| `http_req_duration` | p95 | 98.40ms | 131.20ms | **+32.80ms** | **▲ 33.3%** | **regression** |
| `http_req_duration` | p99 | 180.00ms | 184.10ms | +4.10ms | ▲ 2.3% | ok |

## Gating CI: `--assert`

Without `--assert`, `loadr compare` reports and exits `0`. With it, any
regression beyond tolerance exits **99** — the same
[threshold-failure code](../reference/exit-codes.md) as `loadr run`, so the
same job-failure wiring applies. (Exit `1` still means an error: unreadable
files, not a summary export, no shared metrics.)

```yaml
# .github/workflows/perf.yml (PR job)
- name: Run load test
  run: loadr run perf/api.yaml --summary-export current.json

- name: Fetch baseline            # produced by the nightly job on main
  uses: actions/download-artifact@v4
  with: { name: perf-baseline }

- name: Compare against main
  run: |
    loadr compare baseline.json current.json \
      --max-regression p95=10% --markdown compare.md --assert

- name: Comment on the PR
  if: always()
  uses: marocchino/sticky-pull-request-comment@v2
  with: { path: compare.md }
```

Where the baseline comes from is up to you: a nightly run on `main` uploaded
as an artifact, a blessed summary committed to the repo, or the previous
release's run. Just make sure baseline and current use the **same plan and
load shape** — comparing a 10-VU run against a 100-VU run tells you nothing.

Sweeping load levels instead of comparing two runs? See
[Parameter sweeps](sweep.md).
