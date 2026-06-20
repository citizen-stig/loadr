# Tuning notes — can we make k6 go faster?

A common, fair question about any "tool X vs tool Y" benchmark: *is the slower
tool just mis-configured?* So we tried to tune **k6** for maximum throughput on
the **same target** (the Go `/json` endpoint, 50 VUs unless noted), and measured
each change. Short answer: a few best-practice settings buy ~**+10%**; beyond
that k6 is **CPU-bound in its own runtime**, not by the target or its config.

All numbers below are `requests/sec` on the same 6-core box, generator and
target **co-located** (Docker host networking). loadr is shown as the anchor —
the box demonstrably sustains ~68–70k, so anything below that is the *generator*
leaving throughput on the table, not the server.

| Variant | req/s | vs stock k6 | Notes |
|---------|------:|------------:|-------|
| **loadr** (reference) | **~69,000** | — | compiled engine, anchor for the box's ceiling |
| k6 — stock | ~41,000 | — | default config |
| k6 — `discardResponseBodies` + minimal `systemTags` + `--compatibility-mode=base` | ~45,000 | **+10%** | the committed `k6-tuned` variant |
| k6 — `http.batch([10× GET])` per iteration | ~19,000 | **−54%** | floods the metric/JS path; latency 25ms |
| k6 — 100 VUs | ~44,000 | ~0% | no extra throughput, p95 doubles |
| k6 — 200 VUs | ~42,000 | ~0% | no extra throughput, p95 ~13ms |
| k6 — 400 VUs | ~40,000 | slightly worse | pure queuing |
| k6 — **2 processes** (50 VUs each) | ~49,000 combined | **+11%** | each process tops out ~24k when sharing cores |

## What this tells us

- **Body/tag/compat tunings help a little (~+10%)** and are best-practice, so the
  committed `k6-tuned` scenario uses them. On a ~60-byte response they're minor;
  on large payloads `discardResponseBodies` matters much more.
- **Adding VUs or batching does *not* raise throughput** — k6 already saturates
  its CPU at 50 VUs here, so more concurrency only adds queuing latency. Batching
  is actively counter-productive for this workload.
- **k6 is CPU-bound in its JS runtime + metric pipeline.** Every request runs the
  goja JS VM and emits ~13 metric samples (duration, blocked, connecting, tls,
  sending, waiting, receiving, reqs, data_*, iteration…). That per-request CPU
  cost — not the target, not connection reuse, not config — is the ceiling. loadr
  (compiled, no per-request scripting VM) is more CPU-efficient per request, so it
  reaches ~69k before the box saturates while k6 tops out ~45k.

## The real levers (beyond config)

1. **Separate the generator from the target.** Co-located here, k6's heavier
   footprint competes with the server for the same 6 cores. On a dedicated
   generator host k6 would climb further — the single most impactful change.
2. **Run multiple k6 instances** (or k6's distributed/cloud mode). A single k6
   process has a single-goroutine metrics-engine ceiling; two processes gave
   +11%. This is the supported way to scale one k6 box.

## Reproduce

```bash
cd benchmarks
TOOLS="loadr k6 k6-tuned" ./run.sh         # stock vs tuned side by side
```

The committed tuned scenario is `scenarios/k6/script-tuned.js.tmpl`, run via the
`k6-tuned` tool in `run.sh` (`--compatibility-mode=base --no-usage-report`).
