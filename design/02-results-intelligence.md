# Design Spec: Results Intelligence & Regression History

## 1. Goal & user story

Today `loadr compare baseline.json current.json` (`crates/loadr-cli/src/commands/compare.rs`) is *pairwise*: two summary files, direction-aware deltas, a fixed 5%/explicit tolerance gate, exit `99` (`loadr_core::EXIT_THRESHOLD_FAILED`) on regression. It has no memory. A run that is 8% slower than yesterday may be 8% slower than pure noise, and there is no way to see "p95 over the last 30 builds".

**Results Intelligence** adds a persistent results store, statistical regression detection (does this run fall outside the *noise band* of its history, not just outside a hand-picked %), and `loadr trends` — a local web view of metric history per plan.

One-command experience:

```bash
loadr run perf/api.yaml --record-history      # every run is saved, tagged with git sha/ref
loadr trends --serve                          # open http://127.0.0.1:6465 — pick a plan, see p95 over time
loadr history check perf/api.yaml             # exit 99 if the latest run is a statistically significant regression
```

CI drops the fragile "pick a baseline file" step: history *is* the baseline.

## 2. CLI / API surface

Mirrors `loadr convert`/`loadr compare` styling (clap `Args`, `✓`/`✗` `owo_colors`, `anyhow::Result<i32>` exit codes).

**`loadr run … --record-history`** — a new flag on `RunArgs` (`crates/loadr-cli/src/commands/run.rs`). After the `Summary` is built it is inserted into the store. `--git-sha <SHA>` / `--git-ref <REF>` override the auto-detected values (`git rev-parse HEAD` / `--abbrev-ref`, falling back to `GITHUB_SHA`/`GITHUB_REF_NAME`). `--history-db <PATH>` overrides the default `./.loadr/history.db`.

**`loadr history` (subcommand group, like `loadr plugin`)**
- `list [--plan <id>] [--ref <ref>] [--limit N]` — table of recent runs (run id, git sha, started, p95, error rate, verdict).
- `baseline set <run_id>` / `baseline show [--plan <id>]` — pin/inspect the baseline for a plan+ref.
- `check <plan.yaml|--plan <id>>` — run statistical regression detection of the newest recorded run against its history; prints the same table style as `compare`; `--assert` (default on for `check`) exits `99`. `--alert-webhook <url>` / `--alert-slack <url>` POSTs a regression report.
- `prune [--keep N] [--older-than 90d]` — bound store growth.

**`loadr trends`** — like `loadr report`, but historical and interactive.
- `--serve` (default) starts an axum server (default `127.0.0.1:6465`; `--bind`, `--token`, `--basic user:pass` mirroring the webui plugin's `AuthConfig`).
- `--export <dir>` writes a self-contained static HTML bundle (no server) for artifact upload.

Output of `history check` reuses `compare`'s terminal/markdown table verbatim (`render_text_table`, `render_markdown`) with one extra column: **z / band**.

## 3. Architecture

Two net-new pieces plus one refactor.

**New crate `crates/loadr-history`** (workspace member, added to `Cargo.toml` members list). Owns:
- the store (`store.rs`) — SQLite via `rusqlite` with the `bundled` feature;
- statistics (`stats.rs`) — robust regression detection;
- plan identity (`plan_id.rs`) — a stable hash of a `loadr_config::TestPlan`.

It depends on `loadr-core` (for `Summary`, `MetricSummary`, `MetricKind`, `AggValues`) and `loadr-config` (for `TestPlan`/`Scenario`/`RequestStep` in `plan.rs`). It has **no** network or engine deps — pure store + math, so it unit-tests offline.

**Why SQLite (bundled rusqlite), not append-only JSONL.** `history check` and `trends` need *indexed windowed queries*: "the last N runs for this plan_id on ref `main`, this metric+field, ordered by time". Append-only NDJSON forces a full scan and re-parse of every historical `Summary` on each query, and gives no concurrent-writer safety when parallel CI jobs record at once. SQLite gives indexes, `ORDER BY … LIMIT`, transactional inserts, and WAL concurrency in a single file. The usual objection — "a C dependency breaks cross-compilation" — does not apply here: loadr already statically compiles a C library (QuickJS via `rquickjs`, per the workspace `Cargo.toml`), so `rusqlite`'s bundled amalgamation compiles with the same `cc` path we already exercise on every release target. The store is a CLI/CI-runner artifact, never shipped inside the per-target plugin cdylibs where the pure-Rust/`hyper-rustls` discipline matters.

**`loadr trends` command (`crates/loadr-cli/src/commands/trends.rs`)** reuses the webui plugin's serving pattern directly: axum `Router`, the `rust_embed`-derived `Assets` approach in `plugins/loadr-plugin-webui/src/server.rs`, and its `AuthConfig` (Basic + bearer). It does **not** reuse `UiBackend` (that trait is about live run control); trends is read-only over the store, so it defines a small `HistoryBackend` returning JSON from `loadr-history`. Per-run drill-down reuses the existing `crate::report_html::render(&summary)` from `loadr report`.

**Refactor of `compare.rs`.** `Direction`, `direction()`, `is_throughput()`, `fields_of()`, `render_text_table`, `render_markdown_table`, `fmt_value` are today `pub(crate)`/private in `compare.rs`. Move the direction/field extraction into a shared `compare::model` module (still in `loadr-cli`) so `history check` reuses the exact same "which way is worse / which fields matter" logic instead of forking it. This is the same reuse relationship `har.rs` has with `loadr-convert`'s `Conversion` + `loadr-config` `TestPlan`.

**Alerting reuse.** No new notifier. `history check --alert-*` builds a `RegressionReport` and hands it to the existing `loadr-plugin-slack-notifier` / `loadr-plugin-webhook` message senders — those plugins already act in `finish` on a `Summary`-shaped JSON payload over the shared `hyper` + `hyper-rustls` stack. We factor their `WebhookSender` message-build seam so `history check` can post a regression-specific message through the identical transport.

## 4. Key data structures & algorithms

Schema (SQLite):

```sql
CREATE TABLE runs (
  run_id TEXT PRIMARY KEY,          -- Summary.run_id
  plan_id TEXT NOT NULL,            -- stable hash of the TestPlan
  plan_name TEXT,                   -- Summary.name
  git_sha TEXT, git_ref TEXT,
  started_ms INTEGER, ended_ms INTEGER, duration_secs REAL,
  thresholds_passed INTEGER, aborted TEXT,
  summary_json TEXT NOT NULL);      -- the whole Summary blob, for report/drill-down
CREATE TABLE metric_values (        -- denormalised for fast trend queries
  run_id TEXT, metric TEXT, field TEXT, kind INTEGER, value REAL);
CREATE INDEX ix_mv ON metric_values(plan_id_ref, metric, field);  -- via runs join
CREATE TABLE baselines (plan_id TEXT, git_ref TEXT, run_id TEXT,
  PRIMARY KEY(plan_id, git_ref));   -- absent row ⇒ rolling window
```

`metric_values` is populated by walking `Summary.metrics` with the *same* `fields_of()` used by compare, so the store and the gate agree on units (rates in percentage points, etc.).

**`plan_id`** — `blake3` over the canonicalised `TestPlan`: sorted scenario names, each step's method/URL-template/weight, ignoring volatile bits (data-file paths, env). Groups "the same test" across file renames.

**Statistical detection (`stats.rs`).** For each (metric, field) with a *worse* `Direction`:
1. Load the window: last `N` (default 20) prior runs for this `plan_id` on the baseline ref, newest-excluded is the run under test.
2. Robust center/spread: **median** and **MAD** (median absolute deviation) — resistant to the one-off CI outlier that would blow up a mean/stddev.
3. Modified z-score (Iglewicz–Hoaglin): `z = 0.6745 · (x − median) / MAD`. Flag a regression when `z > 3.5` **and** the deviation is in the worse direction. Report the band `median ± 3.5·MAD/0.6745` so the UI can draw it.
4. **Guardrails:** if `MAD == 0` (identical history) fall back to the current compare percent tolerance; if the window has `< 5` runs, fall back to pairwise vs the baseline run and mark the verdict *low-confidence*. This keeps early-life plans from false-alarming.

`RegressionReport { rows: Vec<RegressionRow> }` where `RegressionRow` extends compare's `MetricDelta` with `median`, `mad`, `z`, `sample_n`, and a reused `regression: Option<bool>`.

## 5. Reuse map

| Concern | Reuse (exists) | Net-new |
|---|---|---|
| Run summary shape | `loadr_core::Summary`, `MetricSummary`, `AggValues`, `MetricKind` | — |
| Field/direction logic | `compare.rs` `direction/is_throughput/fields_of` (extract to `compare::model`) | statistical gate replaces fixed tolerance |
| Table/markdown render | `compare.rs` `render_text_table`, `render_markdown_table`, `fmt_value` | one extra `z/band` column |
| Per-run HTML | `report_html::render` (from `loadr report`) | — |
| Web serving | webui plugin axum + `rust_embed` `Assets` + `AuthConfig` pattern | read-only `HistoryBackend`, trends SPA |
| Alerting | slack-notifier / webhook plugin `WebhookSender` + `hyper-rustls` | `RegressionReport` message body |
| Plan types | `loadr_config` `TestPlan`/`Scenario`/`RequestStep` | `plan_id` hash |
| Store | `rusqlite` (bundled) | `loadr-history` crate, schema |
| Exit codes | `loadr_core::EXIT_THRESHOLD_FAILED` (99) | — |

Net-new is one crate + one command + one small SPA; everything else is composition.

## 6. Testing plan

Follows the repo's `#[cfg(test)]` in-module style (as in `compare.rs`) and the 70% gate / no-real-network rule.
- **`loadr-history` unit tests** against an in-memory SQLite (`:memory:`): insert N synthetic `Summary`s, assert `metric_values` extraction matches `fields_of`, assert window queries order/limit correctly, assert `prune` bounds rows.
- **`stats.rs` unit tests** are the crux: a flat history + a large jump ⇒ regression; a jump *within* MAD ⇒ not; an improvement ⇒ never; `MAD==0` fallback; `<5` runs low-confidence; outlier-robustness (one wild historical value must not mask a real regression). Pure functions, table-driven, mirroring compare's `diff_*` tests.
- **`plan_id` tests**: rename/reorder invariance; content change ⇒ different id.
- **CLI integration** (`assert_cmd`, as other commands do): `run --record-history` then `history list`/`check` on a temp DB, asserting exit 99 on an injected regression and `0` otherwise. `trends --export` produces valid HTML.
- **Web layer**: axum router tested with `tower::ServiceExt::oneshot` (no bound socket), including auth rejection — matching the webui plugin's server tests. No live network anywhere.

## 7. Docs / desktop UI / demo

- **Docs**: new `docs/src/guides/trends.md` and a "Regression history" section extending `docs/src/guides/compare.md`, contrasting pairwise vs statistical gating and documenting the store location, git tagging, and the z-score band. Add a Field Card (per repo convention).
- **Desktop**: the Electron app (`desktop/`) gains a "Trends" tab that embeds the same trends SPA pointed at a locally-launched `loadr trends --serve`, reusing the existing bundled-CLI spawn path.
- **Demo**: a VHS `.tape` (per the demo-videos memory) showing three `run --record-history` builds, then `loadr trends` with a visible p95 regression band, then a red `history check` failing CI. Extend the `loadr-demos` GitHub Actions sample to record history across builds.

## 8. Milestones

- **M1 — store + record (smallest shippable, ~3–4 d).** `loadr-history` crate, SQLite schema, `run --record-history`, `loadr history list`. No stats, no web. Immediately useful: durable run history.
- **M2 — statistical check (~3 d).** `stats.rs` (MAD/z + guardrails), `compare::model` refactor, `loadr history check` with exit 99 and the extended table. This is the headline value.
- **M3 — baselines & alerting (~2 d).** `baseline set/show`, rolling-vs-pinned, `--alert-webhook/--alert-slack` via the existing senders, `prune`.
- **M4 — `loadr trends` web view (~4–5 d).** axum + embedded SPA, trend charts with regression bands, per-run drill-down via `report_html`, `--export` static bundle, desktop tab.

**Risk / hard parts.** (1) `plan_id` stability — too strict and every edit forks history; too loose and unrelated plans merge. Ship it configurable and log the id. (2) The `<5`-runs cold-start window will dominate real early usage — the fallback must be graceful, not silent. (3) MAD assumes roughly unimodal noise; bimodal infra (two CI runner sizes) will inflate the band and hide regressions — document tagging runs by ref/environment as the mitigation. (4) `rusqlite bundled` adds C-compile time to the CLI build (not the plugins) — acceptable given QuickJS precedent, but call it out in release timing.
