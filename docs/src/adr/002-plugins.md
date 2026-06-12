# ADR-002: Plugin system — WASM components + abi_stable natives

**Status**: accepted

## Context

Plugins must cover five shapes (protocol, output, extractor, assertion,
service) with very different risk/performance profiles. One mechanism can't
serve both "run untrusted-ish pure functions safely" and "pump millions of
samples with zero overhead".

## Decision

Two first-class mechanisms:

1. **WASM components** (wasmtime + a WIT-defined interface) for extractors
   and assertions — pure functions over response bytes.
2. **Native dynamic libraries** (`abi_stable`) for outputs, protocols and
   services.

## Rationale

- Extractors/assertions are called per-response with untrusted test-author
  logic; the component model gives capability-safe sandboxing (no FS/network),
  cross-platform portability of a single artifact, and polyglot authorship.
  `wasm32-wasip2` makes Rust guests produce components directly.
- Protocols and outputs need real sockets, threads and throughput; native
  libraries are the honest answer. `abi_stable` removes the classic cdylib
  footgun: type layouts are validated at load time, so version mismatches are
  clean errors, not UB. JSON-over-FFI keeps the ABI minimal and evolvable —
  marshalling cost is irrelevant at plugin-boundary frequencies (outputs see
  batched samples, protocols see one call per request).
- Rejected: dylib-only (no sandbox, no portability), WASM-only (WASI sockets
  are not mature enough for protocol throughput), subprocess plugins à la
  Terraform (operationally heavier; latency per extractor call).

## Consequences

- Two loaders to maintain; shared discovery/manifest/registry layer
  (`plugin.toml`, `~/.loadr/plugins`).
- The web UI ships as a service plugin (statically linked by default),
  proving the service interface with first-party code.
- Worked examples of every type live in `plugins/examples/` and are exercised
  by `cargo test` (including compiling the WASM guests).
