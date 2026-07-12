# loadr feature-family design specs

Implementation blueprints for the feature families on the roadmap
(see [the roadmap post](https://loadr.io/blog/six-new-families/)). Each spec is
grounded in the current codebase — real crates, types, and files — with a
reuse map (what already exists vs what's net-new) and a slice-able build order.

The **Session Recorder** (the sixth family) is already shipped —
see `crates/loadr-record` and [`loadr record`](../docs/src/guides/record.md).

| # | Family | One-command experience | New crate | Status |
|---|--------|------------------------|-----------|--------|
| — | Session Recorder | `loadr record` → auto-correlated scenario | `loadr-record` | ✅ shipped |
| 1 | [Spec-Driven Generation & Fuzzing](01-spec-driven-generation.md) | `loadr gen openapi api.yaml` | `loadr-gen` | 📐 designed |
| 2 | [Results Intelligence & Regression History](02-results-intelligence.md) | `loadr trends` | `loadr-history` | 📐 designed |
| 3 | [AI Copilot](03-ai-copilot.md) | `loadr explain run.json` · `loadr scenario "…"` | `loadr-ai` | 📐 designed |
| 4 | [Trace-Driven Root Cause](04-trace-driven-rca.md) | `observe: { traces: … }` + `--rca` | (extends `loadr-outputs`) | 📐 designed |
| 5 | [Resilience & Game-Day Suite](05-resilience-gameday.md) | `loadr gameday run` | `loadr-gameday` | 📐 designed |

## Common threads

- **Minimal net-new.** Every family composes existing machinery — the HAR
  correlator, `Conversion`/`TestPlan`, `loadr compare`, `loadr-payload`, the
  chaos fault model, the observe seam, the webui/report renderers — rather than
  reinventing it.
- **Offline-first.** Optional capabilities (AI, trace backends) degrade
  gracefully so the single binary still builds and runs with no keys/services.
- **Shippable slices.** Each spec's **M1** is the smallest independently useful
  increment, so the family can land incrementally behind CI.
