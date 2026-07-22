# Design Spec: Resilience & Game-Day Suite

## 1. Goal & user story

loadr already ships the *primitives* of resilience testing, but they are unrelated islands. `faults:` (`loadr_config::plan::FaultSpec` in `crates/loadr-config/src/plan.rs`, guide `docs/src/guides/chaos.md`, example `examples/48-chaos.yaml`) injects latency jitter and drop-rate into a scenario. `${payload:…}` + `loadr payload` (`crates/loadr-payload`, `docs/src/guides/payload.md`, `examples/49-payload-complexity.yaml`) build adversarial bodies. `loadr sweep` (`crates/loadr-cli/src/commands/sweep.rs`, `examples/45-sweep.yaml`) walks a matrix and can fit a complexity exponent. Thresholds with `abort_on_fail`/`delay_abort_eval` (`ThresholdEntry::Detailed`, `docs/src/yaml/thresholds.md`) are the circuit breaker. What's missing is *orchestration*: a way to say "hold steady-state, systematically fail each dependency, and tell me — with an SLO guardrail that auto-aborts — whether the system stayed within its objectives, then score it."

**Resilience & Game-Day** adds a thin experiment layer on top of these primitives — it composes them, it does not reinvent them.

One-command experience:

```bash
loadr gameday run resilience/checkout.yaml         # runs the experiment matrix, prints a resilience score
loadr gameday run resilience/checkout.yaml --md report.md --json out.json --assert
```

Exit `99` (`loadr_core::EXIT_THRESHOLD_FAILED`) when the steady-state hypothesis is violated under a fault the plan declared as "must survive", so CI gates on it exactly like `loadr run` and `loadr compare --assert` do.

## 2. CLI / API surface

Mirrors `loadr sweep`/`loadr compare` styling: clap `Args`, `✓`/`✗`/`→` via `owo_colors::OwoColorize`, `anyhow::Result<i32>` return, `eprintln!` for progress and `println!` for the result table.

**`loadr gameday run <plan.yaml>`** — the experiment runner.

| Flag | Meaning |
|---|---|
| `--only <name,…>` | run a subset of the declared experiments |
| `--out-dir <DIR>` | per-arm summary exports (default `loadr-gameday/`), same convention as sweep's `--out-dir` |
| `--md <PATH>` | GitHub-flavoured markdown runbook + scorecard (for PR comments) |
| `--json <PATH>` | machine-readable `GamedayReport` |
| `--assert` | exit `99` if any `must_survive` arm breaks its hypothesis (default on when `blocking:` arms exist) |
| `--dry-run` | print the resolved arm matrix and derived per-arm plans, run nothing |
| `--seed <N>` | override the experiment's fault seed for reproducibility |

**`loadr gameday score <report.json>`** — recompute/print the resilience score from a prior report (like `loadr report` reading a summary), for aggregating several game-days.

Terminal output is a **scorecard**: one row per arm (baseline + each fault), columns `arm │ p95 │ error rate │ hypothesis │ verdict`, reusing sweep/compare's `render_text_table` and the `fmt_latency`/`fmt_pct` helpers re-exported from `crate::commands::compare`. A final line prints the aggregate **resilience score 0–100** and letter grade.

`gameday` is registered in `crates/loadr-cli/src/main.rs` exactly like the others: a `Gameday(commands::gameday::GamedayCommand)` subcommand-group variant in `enum Command` dispatched to `commands::gameday::execute`, matching the `Plugin` subcommand-group pattern already there.

## 3. Architecture

Deliberately thin. **One net-new crate** and **one new CLI command module**; everything heavy is reused.

**New crate `crates/loadr-gameday`** (workspace member; add to root `Cargo.toml` `members`). Pure orchestration + scoring, no engine/network of its own:
- `experiment.rs` — the DSL types (`Experiment`, `Arm`, `Hypothesis`, `Guardrail`) deserialized from a `resilience:` block.
- `expand.rs` — expands the dependency-failure matrix into concrete **arms**, and for each arm *derives a `loadr_config::TestPlan`* from the base plan by injecting the arm's `FaultSpec` and guardrail thresholds. This is the same relationship `crates/loadr-convert/src/har.rs` has: it takes source input and emits a validated `loadr_config::TestPlan` (`loadr_convert::Conversion { plan, warnings }`), then hands it to the normal run path.
- `score.rs` — the resilience score maths over collected `loadr_core::Summary` per arm.
- `report.rs` — `GamedayReport` (serde) + markdown runbook renderer.

Depends only on `loadr-config` (`TestPlan`, `Scenario`, `FaultSpec`, `ThresholdList`/`ThresholdEntry`, `Dur`) and `loadr-core` (`Summary`, `MetricSummary`, `MetricKind`, `EXIT_THRESHOLD_FAILED`). No `tokio`, no `hyper` — so it unit-tests fully offline.

**Execution seam.** Each arm is *one `loadr run`* of a derived plan. Like `sweep.rs`'s `ComboRunner` trait (real impl spawns the current binary with `run --summary-export …`; tests substitute a fake returning canned `Summary`s — the repo's "no subprocesses in unit tests" convention), `loadr-gameday` defines an `ArmRunner` trait:

```rust
pub trait ArmRunner {
    /// Run a derived plan to completion, exporting the summary to `export`.
    fn run(&mut self, plan: &TestPlan, export: &Path) -> anyhow::Result<(Summary, i32)>;
}
```

The CLI's real `ArmRunner` writes the derived plan to a temp YAML and invokes `loadr run --summary-export`, reusing sweep's exact subprocess+summary-parse machinery (factor sweep's `SubprocessRunner` summary-read helper into a shared `crate::commands::proc` used by both). Because each arm is a real `loadr run`, **the guardrail auto-abort is free**: it's an ordinary `abort_on_fail` threshold in the derived plan, evaluated by the existing threshold engine — the RFC schedule sketch in `examples/48-chaos.yaml`'s header comment (a coordinated `nemesis:` with a seed) is what this DSL formalises, but grounded in today's `FaultSpec` rather than the not-yet-built infra-level nemesis.

## 4. Key data structures & algorithms

The `resilience:` block (new optional field on the experiment file; the base scenarios/thresholds/`defaults` are ordinary `TestPlan` YAML):

```yaml
resilience:
  seed: 1234
  steady_state:                       # the hypothesis: what "healthy" means
    scenario: steady_traffic          # judged scenario (tag-scoped, like example 48)
    hypothesis:
      - "http_req_failed{scenario:steady_traffic}: rate<0.05"
      - "http_req_duration{scenario:steady_traffic}: p(95)<800ms"
      - "checks{scenario:steady_traffic}: rate>0.95"
  guardrail:                          # auto-abort circuit breaker (blast-radius cap)
    "http_req_failed{scenario:steady_traffic}": "rate<0.25"
    delay_abort_eval: 20s
  experiments:
    - name: dependency-latency
      target: steady_traffic
      matrix:                         # cartesian, like sweep --var
        latency: [100ms, 500ms, 2s]
        drop_rate: [0.0, 0.1]
      must_survive: true              # arms here gate CI (blocking)
    - name: payload-complexity
      composes: sweep                 # delegate to the sweep complexity primitive
      var: depth=4000,16000,64000
      complexity: depth
      max_exponent: 1.2
```

Core types (serde, `#[serde(deny_unknown_fields, rename_all = "snake_case")]`, `JsonSchema` derive so `loadr schema` covers them, matching `plan.rs` conventions):

```rust
pub struct Experiment { pub name: String, pub target: String,
    pub matrix: IndexMap<String, Vec<String>>, pub must_survive: bool,
    pub composes: Option<Composition> }          // Fault | Sweep(SweepDelegate)
pub struct Arm { pub label: String, pub faults: FaultSpec,        // loadr_config type, reused verbatim
    pub derived: TestPlan, pub must_survive: bool }
pub struct GamedayReport { pub arms: Vec<ArmOutcome>, pub score: ResilienceScore, pub seed: u64 }
pub struct ArmOutcome { pub label: String, pub summary: Option<Summary>,
    pub hypothesis: Vec<HypothesisResult>, pub aborted: bool, pub exit_code: i32 }
```

**Matrix → arms.** `expand.rs` reuses sweep's cartesian expansion logic (`expand_matrix` in `sweep.rs`, factored to a shared helper). `latency`/`drop_rate` axes map straight onto `FaultSpec.latency.jitter` / `FaultSpec.drop_rate`; arbitrary axes export as `LOADR_SWEEP_<NAME>` so `${payload:…}` / `${env.*}` bodies keep working, identical to sweep. A leading **baseline arm** (empty `FaultSpec`) is always synthesised so the score has a healthy reference.

**Derive plan.** For each arm, clone the base `TestPlan`, set `scenario.faults = Some(arm.faults)` on the target scenario, and *merge* the `hypothesis` + `guardrail` expressions into `plan.thresholds` as `ThresholdEntry::Detailed { abort_on_fail: true, delay_abort_eval }` for the guardrail and plain `Expr` for the hypothesis. Run `loadr_config::validate` on the result (reject early, like `Conversion`).

**Hypothesis evaluation.** Post-run, each hypothesis expression is checked against the arm's `Summary` reusing loadr's existing threshold-expression evaluator in `loadr-core` (the same code that produces exit 99) rather than a fork — parse `metric{tags}: expr`, look up the `MetricSummary`, apply the aggregation (`p(95)`, `rate`, `slo(N%)`). An arm **survives** iff every hypothesis holds *and* it did not hit the guardrail abort.

**Resilience score (0–100).** Per arm, `arm_score = 100 · Π survived(hypothesis_i)` softened by *degradation*, not pure pass/fail: for each hypothesis measure headroom `h = (bound − observed)/bound` clamped to `[-1,1]` and score `50·(1+h)` so an arm that passes with margin scores higher than one that barely passes. Aggregate = weighted mean over arms, weight `2×` for `must_survive` arms. Baseline degradation (`arm vs baseline` p95 ratio, computed with compare.rs's direction-aware delta) is a tie-breaker printed alongside. Grade bands A≥90 … F<60. Deterministic given `seed`.

## 5. Reuse map

| Concern | Reused (exists today) | Net-new |
|---|---|---|
| Fault injection | `loadr_config::plan::FaultSpec`, `LatencyFault`, `DropMode`, `faults_injected` counter | — |
| Adversarial bodies | `crates/loadr-payload`, `${payload:…}`, `loadr payload` | — |
| Matrix expansion / complexity fit | `sweep.rs` `expand_matrix`, `fit_exponent`, `--complexity`/`--max-exponent` | wiring `composes: sweep` |
| Guardrail auto-abort | `ThresholdEntry::Detailed{abort_on_fail, delay_abort_eval}` + threshold engine | merge into derived plan |
| Hypothesis eval | `loadr-core` threshold-expression evaluator, `slo(N%)`, tag selectors | expr→arm-summary adapter |
| Run each arm | `loadr run --summary-export`, `Summary` parse (sweep `SubprocessRunner`) | `ArmRunner` trait |
| Plan derivation | `loadr_config::TestPlan`, `validate` (`har.rs` pattern) | `expand.rs` |
| Tables / markdown | `compare.rs` `render_text_table`, `render_markdown_table`, `fmt_latency`, `fmt_pct` | scorecard columns |
| CLI wiring | `main.rs` subcommand-group pattern (`Plugin`) | `commands/gameday.rs` |
| Score / runbook | — | `score.rs`, `report.rs`, `GamedayReport` |

Net-new is only the orchestration crate + one command module + scoring maths. Everything load-bearing is existing, tested code.

## 6. Testing plan

Mirrors the repo's `#[cfg(test)]`-in-module style and the 70% coverage gate; no real network in unit tests.

- **`expand.rs`**: matrix → arm labels/`FaultSpec` (table tests), baseline arm always present, arbitrary axis → `LOADR_SWEEP_*`, derived plan has the injected `faults` on the *target* scenario only and the guardrail threshold with `abort_on_fail:true`. Assert `loadr_config::validate` passes on derived plans.
- **`score.rs`**: golden `Summary` fixtures → expected score/grade; monotonic (more headroom ⇒ higher score); `must_survive` weighting; determinism under fixed seed.
- **Hypothesis eval**: reuse `loadr-core` evaluator tests as the source of truth; add adapter tests for `metric{tags}: expr` parsing and no-samples-passes semantics (documented in `thresholds.md`).
- **CLI (`commands/gameday.rs`)**: a fake `ArmRunner` returning canned `(Summary, exit)` (exactly sweep's `ComboRunner` test double), asserting `--assert` returns `EXIT_THRESHOLD_FAILED` when a `must_survive` arm fails, `0` otherwise, and that `--only` filters.
- **Integration** (repo `tests/` + `examples/harness/docker-compose.yml`, the go-httpbin stack example 48 already uses): a new `examples/50-gameday.yaml` runs a 2-arm matrix against `/status/500` + `/delay`, gated to a short duration, asserting a scorecard and non-zero exit under forced faults. Runs only in the network-enabled integration job, never in unit tests.

## 7. Docs / desktop UI / demo

- **Book**: new `docs/src/guides/gameday.md` (sits beside `chaos.md`/`payload.md`/`sweep.md`), framed as "chaos + payload + sweep, orchestrated + scored". Add the `resilience:` block to the YAML reference and a Field Card. Cross-link from `chaos.md` ("to orchestrate several faults with a score, see Game-Day").
- **Schema**: `Experiment`/`Arm`/`Hypothesis` derive `JsonSchema`, so `loadr schema` and editor completion pick them up for free.
- **Desktop** (`desktop/`, the Electron cockpit over the bundled CLI): a "Game-Day" tab that shells `loadr gameday run --json` and renders the `GamedayReport` scorecard + per-arm timeline (reuses the existing summary-timeline view; the score is one new gauge). Ships after the CLI is stable.
- **Demo**: a `site/videos` vhs `.tape` running `examples/50-gameday.yaml` against the harness, showing the matrix streaming and the final score — same recipe as the other demos (re-record before deploy, per repo memory).

## 8. Milestones

- **M1 — smallest shippable (~3–4d).** `loadr-gameday` crate + `commands/gameday.rs`. `resilience.experiments` with a `latency`/`drop_rate` matrix only (no `composes`, no score). Derives plans, runs arms via `ArmRunner`, evaluates the hypothesis, prints the scorecard, `--assert` gates CI. This alone replaces the hand-rolled nemesis scenario in `examples/48-chaos.yaml` with a declarative matrix. Ships the guide + `examples/50-gameday.yaml`.
- **M2 — resilience score (~2d).** `score.rs`, headroom-based scoring, grade, `--json`/`--md` `GamedayReport`, `loadr gameday score`.
- **M3 — compose sweep/payload (~2d).** `composes: sweep` delegating to `sweep.rs`'s complexity fit so a payload-complexity probe is one arm of a game-day; fold `max_exponent` failures into the score as a failed hypothesis.
- **M4 — reporting polish + desktop (~3d).** Markdown runbook (owner/steps/rollback prose per experiment), HTML scorecard via the `loadr report` renderer, desktop Game-Day tab, demo tape.

## 9. Risk & hard parts

- **Cost/time blowup.** Arms run *sequentially* (like sweep) and each is a full `loadr run`; a 3×2 matrix + baseline = 7 runs. Mitigate: default short per-arm durations, `--only`, and document the arm count in `--dry-run` before committing minutes of load.
- **Score is a judgement call.** Any single 0–100 number invites bikeshedding and false confidence. Keep it *explainable* — always print the per-hypothesis headroom that produced it, never just the number; treat the score as a summary of the scorecard, not a replacement.
- **Guardrail vs. hypothesis confusion.** Both are threshold expressions; users will conflate "abort the whole game-day" (guardrail, blast-radius cap) with "this arm failed" (hypothesis). The DSL keeps them in separate blocks and the docs must hammer the distinction.
- **Fault fidelity ceiling.** `FaultSpec` is *client-side* injection (latency + drop) — it cannot partition a real cluster or kill a node. This is honest and cheap (the `chaos.md` selling point) but must not be oversold as infra chaos; the `nemesis:` infra layer sketched in `examples/48-chaos.yaml` / `docs/design/concurrency-consistency-testing.md` remains a separate future RFC that this DSL is forward-compatible with (an arm could later target a `nemesis` instead of a `FaultSpec`).
- **Reusing the threshold evaluator out of run context.** The evaluator is built to run inside the engine; exposing a pure `evaluate(expr, &Summary)` entry point in `loadr-core` without dragging in engine state is the main refactor risk — scope it as a small, separately-tested public fn before M1 depends on it.
