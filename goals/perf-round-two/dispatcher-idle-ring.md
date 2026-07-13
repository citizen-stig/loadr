# Goal - arrival dispatcher without a wake per iteration

The open-model dispatcher still performs one queue pop + one `Notify` wake per
scheduled iteration, all on a single task — the last one-core choke point on
the request hot path now that metrics recording is sharded. This goal inverts
the hand-off so workers claim work themselves and the dispatcher only touches
a counter once per tick.

Paste this whole block into a fresh coding-agent session:

```text
/goal Replace the arrival dispatcher's per-iteration wake hand-off with a shared claim counter so dispatch cost is O(ticks), not O(iterations)

CONTEXT
- Base branch: risotto. File: crates/loadr-core/src/executor.rs —
  `run_arrival_rate` (:630), `dispatch_tick` (:615, env
  LOADR_DISPATCH_TICK_US, default 5000us, clamped 250..=1_000_000).
- Current design: workers park on a per-worker `Arc<Notify>` and register it
  through `mpsc::unbounded_channel::<Arc<Notify>>()` (:652) every idle cycle;
  the dispatcher accumulates fractional owed iterations from
  `schedule.rate_at(elapsed) * dt` each tick, then per whole iteration pops
  one idle worker and calls `notify_one()`, spawning a new worker when none
  are idle and `allocated < max_vus`, else incrementing the
  `dropped_iterations` counter.
- Why it matters: at 150k+/s the dispatcher does 150k+ pops/wakes per second
  on one task; measurement history (docs/loadr-s3-tower-bypass.md, "Related"
  section) identifies the single dispatcher as a remaining reason N processes
  outperform one runtime. This was the disposition's deferred
  "bounded-MPMC / indexed idle-worker ring" item.

IMPLEMENTATION
- Replace the per-iteration hand-off with a shared budget the workers claim:
  e.g. `Arc<AtomicI64>` owed-iterations counter plus one `Arc<Notify>`
  (notify_waiters) or a small ring of Notifys to avoid thundering herd —
  design freedom here, but the dispatcher's per-tick work must become O(1)
  amortized (add owed count, wake up to `min(owed, parked)` workers), not
  O(iterations).
- Worker loop: try to claim (`fetch_sub` while positive); on failure park on
  the Notify and re-check after wake (spurious wakes are fine; lost wakes are
  not — keep the permit-buffered pattern the current code documents).
- MUST preserve, exactly: `max_vus` cap with spawn-on-demand growth;
  `dropped_iterations` accounting (an owed iteration that cannot run because
  the pool is saturated at max_vus within the tick window counts as dropped —
  do not let the budget silently absorb drops); cancellation via the existing
  token; `LOADR_DISPATCH_TICK_US` semantics; ramping-arrival-rate sharing the
  same loop via `RateSchedule`.
- Keep the diff inside `run_arrival_rate` + small helpers. Do not touch other
  executors.

OUT OF SCOPE
- Multi-dispatcher / per-core sharded dispatchers (a later stage if this is
  not enough).
- Changing tick default or CLI/env surface.
- The metrics pipeline, gRPC handler, or worker body (`ExecEnv::run_one`).

TESTS
- Existing e2e `arrival_rate_keeps_schedule_without_drops`
  (crates/loadr-cli/tests/e2e.rs) must stay green — it pins throughput band
  and zero drops under --worker-threads 2.
- Add a saturation test: max_vus deliberately too small for the rate ->
  `dropped_iterations > 0` and the run still completes (engine-level test in
  loadr-core, or e2e asserting the summary counter; follow the existing e2e
  style with the in-process HttpTestServer).
- Add a burst-behavior assertion if practical: with a 1ms tick the schedule
  is met (reuses the e2e helper with LOADR_DISPATCH_TICK_US=1000).

QUALITY BAR
Focused regression tests as above; perf claims (if stated) measured in
release mode; no unrelated refactors; conventional commit, no Claude-Session
trailer. Run cargo fmt --all and cargo clippy --workspace --all-targets --
-D warnings, then cargo test -p loadr-core -p loadr-cli --locked (workspace
suite before the PR: --exclude loadr-browser locally).

DONE when: the dispatcher performs no per-iteration channel operation or
targeted wake (verified by code inspection), and both the zero-drop and
saturation tests pass.
```
