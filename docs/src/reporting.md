# HTML reports & time-series charts

`loadr report` turns a summary JSON export into a single, self-contained HTML
file you can share with people who don't run loadr. It contains:

- **Time-series charts** — throughput, response-time percentiles, active VUs
  and error rate plotted against elapsed run time.
- The **aggregate tables** — thresholds, checks, latency trends, and
  counters/rates/gauges, exactly as the console summary reports them.

The file references **no external assets**: the charts are inline SVG drawn by
a small inline script, and all styling is inline CSS. It opens offline and is
safe to attach to an email or commit to a repo.

## Generating a report

```bash
loadr run --summary-export results.json test.yaml
loadr report results.json -o report.html
```

`loadr report` accepts any summary JSON produced by `--summary-export`,
including one fetched from a controller's `/api/runs/{id}/summary` endpoint in
distributed mode.

## The charts

Four charts are rendered from the run timeline:

| Chart | Series | Source |
|-------|--------|--------|
| **Throughput** | requests/s, iterations/s | per-interval `http_reqs` / `iterations` counts |
| **Response time** | p50, p95, p99, avg (ms) | `http_req_duration` percentiles |
| **Active VUs** | virtual users | `vus` gauge |
| **Error rate** | failed % | `http_req_failed` rate |

Each chart shares a hover crosshair: move the pointer over any chart and a
dashed line tracks the nearest interval in **all four** charts at once, with the
exact values for that instant printed beneath. This makes it easy to correlate,
say, a latency spike with the moment VUs ramped up.

A ramping or spike profile produces the most interesting shape — see
[`examples/24-timeseries-report.yaml`](https://github.com/levantar-ai/loadr/blob/main/examples/24-timeseries-report.yaml),
which warms up, spikes to ~5x load, then recovers.

## How the timeline is captured

During a run the engine snapshots the aggregator once per snapshot interval
(1 s by default; `--snapshot-interval` to change it). Each snapshot is reduced
to one compact **timeline point** and appended to the summary. In distributed
mode the controller samples the centrally merged snapshot at the same cadence,
so the timeline reflects the whole fleet.

Timeline latency percentiles are the live, count-weighted merge across tag sets
— accurate enough for visual analysis. The **aggregate tables remain the exact
end-of-run figures** (merged from HDR histograms), so a threshold and its chart
may differ by a hair; trust the table for pass/fail.

## `timeline` in the results JSON

The summary export gains a top-level `timeline` array. It is **additive** —
existing fields are unchanged, and reports from before this feature (no
`timeline`) still render, just without charts.

```json
{
  "name": "timeseries-report",
  "run_id": "...",
  "duration_secs": 50.0,
  "metrics": [ "..." ],
  "thresholds": [ "..." ],
  "snapshot": { "...": "final per-tag snapshot" },
  "timeline": [
    {
      "elapsed_secs": 1.0,
      "rps": 48.0,
      "iterations_ps": 24.0,
      "active_vus": 5.0,
      "error_rate": 0.0,
      "latency_avg": 12.4,
      "latency_p50": 9.0,
      "latency_p95": 31.0,
      "latency_p99": 58.0
    }
  ]
}
```

| Field | Meaning |
|-------|---------|
| `elapsed_secs` | seconds since the run started |
| `rps` | requests/s over the interval |
| `iterations_ps` | completed iterations/s over the interval |
| `active_vus` | active virtual users at that instant |
| `error_rate` | failed-request fraction over the interval, `0`-`1` |
| `latency_avg` / `latency_p50` / `latency_p95` / `latency_p99` | `http_req_duration` in ms; omitted when no requests had completed yet |

One point is emitted per snapshot interval, plus a trailing point covering the
residual window so even sub-interval runs produce a timeline. The latency
fields are omitted (rather than `null`) when there is no sample yet, so charts
simply start once traffic begins.
