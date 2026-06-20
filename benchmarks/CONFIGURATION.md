# Configuring each tool for throughput — research + measured impact

A fair benchmark should give every tool its best realistic shot. This documents
the **high-throughput configuration** for each load tool (researched from the
official docs, linked below), the **tuned variant we committed**, and — honestly —
**how much it actually moved the needle** on *this* benchmark (50 VUs · `GET
/json` · ~60-byte body · generator and target co-located on one 6-core Linux box).

Run the tuned variants alongside the stock ones:

```bash
cd benchmarks
TOOLS="loadr k6 k6-tuned jmeter jmeter-tuned gatling locust locust-tuned" ./run.sh
```

## TL;DR — measured

| Tool | Stock | Tuned | Δ | Why |
|------|------:|------:|---|-----|
| **Locust** | ~1,500 req/s | **~21,000 req/s** | **~14×** | was genuinely mis-configured (see below) |
| JMeter | ~47,000 | ~48,500 | ~+3% | already near its efficient ceiling here |
| k6 | ~44,000 | ~44,000 | ~flat | CPU-bound in its JS+metrics runtime |
| Gatling | ~48,000 | ~48,000 | ~flat | already efficient; heap added defensively |
| loadr | ~58–69k | — | — | the compiled baseline; nothing to tune |

**The big lesson:** the researched tunings are all *correct*, but their **impact
depends on where the bottleneck is**. On a localhost, sub-millisecond target with
keep-alive already on, the connection/heap/GC levers have almost nothing to bite
on — k6/JMeter/Gatling are already near their efficient ceiling and the shared
6-core box (generator **and** target) is the limit. The one tool leaving massive
throughput on the table was **Locust**, because its defaults are genuinely slow
(wrong HTTP client + single process). On a **remote** target, **larger payloads**,
or a **dedicated generator host**, the other tunings matter much more.

---

## Locust — the big win (~14×)

Stock Locust uses the `requests`-based `HttpUser` in a **single GIL-bound
process** → ~1,500 req/s. Two changes fix it:

1. **`FastHttpUser`** (geventhttpclient instead of `requests`) — ~4× less CPU per
   request. The docs quote ~16k vs ~4k req/s per core.
2. **`--processes -1`** — one worker per core; near-linear scaling past the
   single-process GIL ceiling.

Committed as `scenarios/locust/locustfile-tuned.py` + the `locust-tuned` runner.
Measured here: **1,478 → 20,783 req/s (14×)**, p95 28ms → 4ms.

- https://docs.locust.io/en/stable/increase-performance.html
- https://docs.locust.io/en/stable/running-distributed.html

## JMeter — ~+3% here, bigger on remote targets

Stock JMeter was already ~47k. The committed `jmeter-tuned`
(`scenarios/jmeter/user.properties` + a 2G heap via `JVM_XMS/JVM_XMX`) sets:

- **`httpclient.reset_state_on_thread_group_iteration=false`** — persist
  connections across loop iterations instead of re-handshaking each one. PFLB
  measured ~3.9× **on a remote target**; here, with keep-alive already on and a
  localhost sub-ms target, handshakes aren't the bottleneck → only ~+3%.
- **Trimmed JTL** (`subresults/assertions/response_data/url=false`) and a fixed
  heap — keep GC/IO off the hot path.
- Already good in the stock plan: non-GUI `-n`, no listeners, HttpClient4,
  Use-KeepAlive on.
- **Distributed mode does *not* help on one host** (RMI overhead) — skip it.
- justb4 image gotcha: set heap via `JVM_XMS`/`JVM_XMX` (MB), **not** `JVM_ARGS`
  (the entrypoint overwrites it).

- https://jmeter.apache.org/usermanual/best-practices.html
- https://jmeter.apache.org/usermanual/properties_reference.html
- https://pflb.us/blog/how-to-speed-up-jmeter-part-1/

## k6 — ~flat here (CPU-bound, as measured earlier)

Committed `k6-tuned` (`scenarios/k6/script-tuned.js.tmpl` + the `k6-tuned` runner):
`discardResponseBodies`, minimal `systemTags`, `--no-thresholds`,
`--no-usage-report`. All best-practice, all free — but on a 60-byte response with
k6 already CPU-bound in its goja runtime + ~13-metric-sample pipeline, the gain is
within noise (see [`TUNING.md`](TUNING.md) for the full k6 experiment ladder).

> Note: the table reads k6's `http_reqs.rate`, which divides by the whole run
> window (including ramp-down), so it slightly *understates* k6's steady-state
> throughput vs the count-based tools (by request count k6-tuned is flat-to-
> marginally-ahead of stock, not behind). It's consistent k6-vs-k6, and the
> cross-tool ordering doesn't change.

Research notes worth recording:
- **`--compatibility-mode=base` is a no-op on k6 ≥0.53** (we dropped it). It only
  helped on much older k6.
- **Do NOT disable connection reuse** (`noConnectionReuse`/`noVUConnectionReuse`
  *reduce* throughput).
- **`http.batch`** raises requests-per-iteration but changes test semantics; it
  was counter-productive here.
- Real scaling: multiple k6 processes split by `--execution-segment` (pin
  `GOMAXPROCS` per process), or distributed/cloud.

- https://grafana.com/docs/k6/latest/testing-guides/running-large-tests/
- https://grafana.com/docs/k6/latest/using-k6/javascript-typescript-compatibility-mode/

## Gatling — already efficient; heap added defensively

Gatling was already ~48k with `shareConnections()`. Applied to the committed
scenario:

- **Fixed G1 heap via the plugin `<jvmArgs>`** (`-Xms2G -Xmx2G -XX:+UseG1GC
  -XX:+AlwaysPreTouch`). Important: the gatling-maven-plugin **forks** the test
  JVM, so `MAVEN_OPTS`/`JAVA_OPTS` do **not** reach it — args must go in
  `<jvmArgs>`. Default was only `-Xmx1G`.
- **`shareConnections()`** (already used) — one shared pool, fewer sockets.
- **`.disableCaching()`** is important *if* the endpoint sets cache headers
  (Gatling would otherwise serve cache hits without touching the server) — our
  target sets none, so it's not needed here, but it's the right call generally.
- The **~30s JIT warm-up** ramp we observed is HotSpot tiered compilation; the
  fair treatment is to discard the first ~30–60s and measure steady state.
- Closed model (`constantConcurrentUsers`) self-throttles to
  `concurrency / response-time`; to find the true *ceiling* use the open model
  (`rampUsersPerSec(...).to(...)`).

- https://gatling.io/blog/scaling-load-tests
- https://docs.gatling.io/reference/script/http/protocol/

## Cross-cutting: the host (matters most at high RPS)

These apply to **every** generator and are usually the real wall before the tool's
own config:

- **File descriptors**: `--ulimit nofile=1048576:1048576` (each socket = 1 FD).
- **Ephemeral ports / TIME_WAIT** (host, since we use `--network host`):
  `net.ipv4.ip_local_port_range="1024 65535"`, `net.ipv4.tcp_tw_reuse=1`. Keep-
  alive / connection-sharing is the script-side mitigation.
- **CPU**: pin cores (`--cpuset-cpus`) rather than quota (`--cpus`) to avoid CFS
  throttling; keep the generator ≤~80% CPU so its own scheduling doesn't inflate
  *reported* latency.
- **Biggest lever of all: put the generator on a host SEPARATE from the target.**
  Co-located here, the heavier tools compete with the server for the same cores —
  which is exactly why the more CPU-efficient engine (loadr) reaches a higher
  number on this box.
