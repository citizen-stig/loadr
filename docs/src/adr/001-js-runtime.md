# ADR-001: JavaScript runtime — QuickJS (rquickjs)

**Status**: accepted

## Context

Tests embed JavaScript three ways (inline `${js:}`, script steps, full
modules). Candidates: QuickJS (via `rquickjs`), embedded V8 (via
`deno_core`/`rusty_v8`), JavaScriptCore, Bun, and pure-Rust engines (Boa).

## Decision

QuickJS via `rquickjs`, behind the `ScriptEngine`/`VuScript` traits in
`loadr-core` so the choice stays reversible.

## Rationale

**Per-VU isolation cost dominates.** k6 semantics require an isolated JS
context per VU — no shared mutable state. A load test may run thousands of
VUs. QuickJS runtimes cost kilobytes and microseconds to create; V8 isolates
cost megabytes and milliseconds. At 5 000 VUs that's the difference between
"fine" and gigabytes of heap before the first request.

**Execution model.** Load scripts are straight-line blocking code
(`http.get()`, `sleep()`, `check()`). QuickJS lets host calls block
synchronously and bridge into Tokio (`block_in_place` + `block_on`).
`deno_core` imposes its async ops/event-loop model, which fights
deterministic pacing and per-request hooks.

**Distribution.** `rusty_v8` means huge prebuilt artifacts, long CI builds
and friction on musl/distroless/cross targets — directly against the
single-small-binary goal. QuickJS is plain C, compiled by cargo everywhere.

**The JIT doesn't pay here.** Iterations are network-bound; script time is a
few percent of iteration time. This mirrors k6's own choice of goja (a non-JIT
Go interpreter) over embedding V8. Hot paths (crypto, encoding, HTTP, JSON)
are native Rust functions exposed to JS.

**Why not the others:**

- **Bun** is a runtime/toolkit on JavaScriptCore written in Zig — there is no
  supported way to embed it in a Rust process. Using it would mean shipping a
  separate `bun` executable and IPC, destroying the single-binary story,
  per-VU isolation and the platform matrix. JavaScriptCore itself has only
  immature Rust bindings with painful static linking on Linux.
- **Boa** (pure Rust): attractive supply-chain-wise, but slower than QuickJS
  with less complete ES support at decision time.

## Consequences

- CPU-heavy *user* script code runs ~10–30× slower than V8 — documented;
  native stdlib mitigates the common cases.
- Sandboxing is straightforward: per-runtime memory limits + interrupt
  handler for wall-clock budgets (`js.timeout`, `js.memory_limit_mb`).
- A V8-backed `ScriptEngine` implementation can slot in later behind the same
  trait if benchmarks ever justify it.
