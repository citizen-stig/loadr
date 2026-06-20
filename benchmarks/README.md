# loadr benchmarks

A simple, **fair** head-to-head of loadr against the major load-testing tools —
**k6, JMeter, Gatling and Locust** — all driving the *same* Dockerized target,
one tool at a time. This is the **first, deliberately simple** benchmark; it
establishes the harness we'll grow.

## The benchmark

Every tool runs the identical **closed-model** scenario:

> **N concurrent users** (default 50) hammer `GET /json` as fast as they can for
> a **fixed duration** (default 30s), with no think time.

We record, per tool: total requests, error rate, **throughput (req/s)** and
**latency p50 / p95 / p99 (ms)** — each parsed from that tool's own native
output, then normalised into one table.

### The target

`target/` is a tiny dependency-free Go HTTP API (`/`, `/json`, `/users/{id}`,
`/echo`, `/healthz`) on a distroless image. It uses all cores and does almost no
work, so it is **not** the bottleneck — the load generator is the variable under
test. It runs with host networking on `:18080`, and every tool reaches it over
the same path (`http://localhost:18080`) with no docker-proxy/NAT in between.

### The tools

| Tool | How it runs | Scenario |
|------|-------------|----------|
| **loadr** | the release binary (under test), on the host | `scenarios/loadr/plan.yaml.tmpl` |
| **k6** | `grafana/k6` container | `scenarios/k6/script.js.tmpl` |
| **JMeter** | `justb4/jmeter` container, non-GUI | `scenarios/jmeter/plan.jmx` |
| **Gatling** | `maven` container, Java DSL sim | `scenarios/gatling/` |
| **Locust** | `locustio/locust` container, headless | `scenarios/locust/locustfile.py` |

Scenarios with `.tmpl` are rendered with the shared `BENCH_*` values at run time,
so all five use the same URL / VUs / duration.

## Run it locally

Requires Docker (host networking → **Linux**) and a loadr binary (`$LOADR_BIN`,
else `../target/{release,debug}/loadr`, else it `cargo build`s one).

```bash
cd benchmarks
./run.sh                                   # all 5 tools, 50 VUs, 30s
TOOLS="loadr k6" ./run.sh                  # a subset
BENCH_VUS=100 BENCH_DURATION=60s BENCH_DURATION_S=60 ./run.sh
```

It brings up the target, warms it, runs each tool sequentially, writes
`results/<tool>/…` (raw native output) plus `results/summary.json` and
`results/summary.md`, and prints the comparison table. The target is torn down on
exit.

### Example output

```
| Tool    | Requests | Error % | Throughput (req/s) | p50 (ms) | p95 (ms) | p99 (ms) |
|---------|---------:|--------:|-------------------:|---------:|---------:|---------:|
| loadr   |   680271 |    0.00 |            67987.0 |     0.57 |     1.55 |     2.51 |
| k6      |   404686 |    0.00 |            40460.2 |     0.75 |     3.28 |     6.38 |
| jmeter  |   374201 |    0.00 |            37729.5 |     1.00 |     3.00 |     5.00 |
| gatling |   353346 |    0.00 |            35334.6 |     1.00 |     5.00 |     9.00 |
| locust  |    14785 |    0.00 |             1630.0 |    21.00 |    29.00 |    43.00 |
```
*(50 VUs · 10s · GET /json, on one developer machine — absolute numbers vary by
host; compare tools within a single run, not across machines.)*

## Run it in CI

`.github/workflows/benchmark.yml` — a **manually-dispatched** workflow (it runs
five tools back-to-back, so it isn't on every push). Trigger it from the Actions
tab with optional `vus` / `duration` / `tools` inputs. It builds loadr release,
runs `run.sh`, posts the table to the run summary, and uploads `results/` as an
artifact.

## Methodology notes & caveats

- **Out-of-the-box, not tuned.** Each tool runs with stock settings — the point
  is a like-for-like default comparison, not each tool's theoretical maximum. We
  also researched and committed **tuned** variants (`k6-tuned`, `jmeter-tuned`,
  `locust-tuned`, Gatling heap) — Locust gains **~14×**, the rest are near their
  ceiling here. Per-tool config + measured impact: [`CONFIGURATION.md`](CONFIGURATION.md);
  the k6 deep-dive: [`TUNING.md`](TUNING.md).
- **Locust** is single-process here; its throughput ceiling (and the latency that
  implies) reflects one Python worker, which is how most people first run it.
  Distributed Locust workers would change that — a future benchmark.
- Compare tools **within one run** on one machine. Absolute throughput depends on
  the host; the *relative* ordering is the signal.
- Latency is wall-clock response time as each tool measures it; loadr/k6 report
  sub-ms precision, JMeter/Gatling report integer milliseconds.

## Layout

```
benchmarks/
├── target/             # the Go API under test + Dockerfile
├── docker-compose.yml  # brings up the target (host networking, :18080)
├── scenarios/          # one equivalent scenario per tool
├── lib/report.py       # parse each tool's output → normalised table
├── run.sh              # orchestrator (build target, run tools 1-by-1, report)
└── results/            # per-run output (gitignored)
```
