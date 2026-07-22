# Run history & regression detection (loadr history)

`loadr history` keeps a durable record of your runs in a local SQLite database
and flags **statistical** regressions — not "is this run 10% slower than one
baseline", but "is this run an outlier against the last 20 runs", so a single
noisy CI run can't false-alarm.

## Record every run

```console
$ loadr run plan.yaml --summary-export summary.json
$ loadr history record summary.json --plan checkout
history: recorded run 3c1bf16c… (46 metric value(s)) under plan checkout
```

Runs are grouped by `--plan` (or a stable id derived from the summary). Point
`--db` anywhere; the default is `./.loadr/history.db`.

```console
$ loadr history list --plan checkout
run                      plan       slo    when(ms)
2caaa6c1…                checkout   pass   1783582431919
96e3e525…                checkout   pass   1783582429806
```

## Check for regressions

```console
$ loadr history check summary.json --plan checkout
metric.field                value     median       z      n  verdict
http_req_duration.p99       256.0        0.8  2550.2      6  ✗ REGRESSION
http_req_duration.p50       243.6        0.4 65602.9      6  ✗ REGRESSION
http_reqs.rps                16.2     6652.5  -176.6      6  ✗ REGRESSION
http_req_receiving.p95        0.0        0.1  -103.9      6  ✓ ok
history: 34 regression(s) against 6 prior run(s)   # exit 99
```

`check` exits **99** when it finds a regression, so it gates CI out of the box
(disable with `--assert=false`).

## How the detection works

- **Median + MAD** (median absolute deviation) describe the history — both
  resistant to a one-off outlier that would blow up a mean/stddev.
- **Modified z-score** `z = 0.6745·(x − median)/MAD`; a regression is `|z| > 3.5`
  in the worse direction (higher latency/error, lower throughput).
- **Guardrails**: with `MAD == 0` (identical history) it falls back to a ±10%
  check; with `< 5` prior runs it marks the verdict *low-confidence* rather than
  false-alarming an early-life plan.

Wire it into CI: record on every green build, `check` on each PR run — a slow
merge trips the gate before it reaches `main`, and you can pipe the same summary
into [`loadr explain`](explain.md) for the plain-language "why".
