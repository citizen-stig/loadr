# Credits & influences

loadr stands on the shoulders of the load-testing tools that came before it.
It is not a fork of any of them — it's a fresh implementation in Rust — but its
design borrows the best ideas from four projects, deliberately and gratefully.

## [k6](https://k6.io) — the model

loadr independently implements the modern load-testing execution model that k6
helped popularize: the seven executor types (`constant-vus`, `ramping-vus`,
`constant-arrival-rate`, `ramping-arrival-rate`, `per-vu-iterations`,
`shared-iterations`, `externally-controlled`), the open/closed load distinction,
four metric types (Counter, Gauge, Rate, Trend), thresholds as pass/fail gates
with `abortOnFail` and exit code 99, checks, groups and tags. It is a fresh Rust
implementation — not a fork or a port, and no k6 source is used. For teams moving
across, `loadr convert` imports existing k6 scripts and the JS runtime accepts
their imports so they run unchanged.

## [Apache JMeter](https://jmeter.apache.org) — the arsenal

JMeter's breadth of assertions, extractors and timers shaped loadr's request
toolkit: response/duration/size/JSONPath/XPath assertions, the regular
expression / boundary / CSS / XPath extractors, the constant / uniform /
gaussian timers and the constant-throughput timer (loadr's `pacing`), CSV data
sets with shared/per-thread cursors and recycle/stop-at-EOF, and cookie
management. `loadr convert` reads `.jmx` plans so you can bring decades of
existing tests with you.

## [Gatling](https://gatling.io) — the DSL

Gatling contributed the *flow control* and *injection* vocabulary: the
`repeat` / `while` / `if`-`else` loops and conditionals, the
`randomSwitch` / `uniformRandomSwitch` / `roundRobinSwitch` branch selection
(loadr's `random` step), the feeder *strategies* (sequential / random /
shuffle), JSON feeders, and the request-rate `throttle` (`reachRps`). Gatling's
rich, assertion-driven simulation reports also informed loadr's HTML report.

## [Locust](https://locust.io) — the behaviour model

Locust's weighted-task model — users that pick `@task(weight)` actions at
random rather than running a fixed script — is exactly what loadr's weighted
`random` step expresses. Locust's clean real-time web UI was a direct
inspiration for loadr's built-in management UI, and its straightforward
distributed master/worker model informed loadr's controller/agent design.

## What loadr adds

The combination is the point — everything you would reach for k6, JMeter,
Gatling or Locust to do — scriptable execution *and* a deep assertion arsenal
*and* a flow-control DSL *and* weighted-behaviour modelling — in one binary,
plus a few things none of them ship together: a single static binary with no runtime
(no JVM, no Python, no Go toolchain, no `protoc`, no OpenSSL); **mathematically
correct distributed percentiles** via HDR-histogram merging (not averaging); a
sandboxed WASM + native **plugin** system that needs no rebuild; six protocols
with per-phase timings; and a declarative, schema-validated YAML format you can
code-review.

> Trademarks and project names belong to their respective owners. loadr is an
> independent project and is not affiliated with or endorsed by k6/Grafana
> Labs, the Apache Software Foundation, Gatling Corp, or the Locust project.
