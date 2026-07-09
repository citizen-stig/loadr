# Implementation Plan: Plugin-Backed On-Demand Data Sources

This document is a self-contained implementation plan for the design in
`docs/custom-grpc-plugin-feeder.md`. It is written so an engineer (or coding
agent) can execute it without additional context. All file/line references
were verified against `main` at commit `31412fc`; treat line numbers as
anchors that may have drifted slightly, but the named types/functions are the
source of truth.

## 1. Goal

Add a `data_source` capability to native plugins and a `type: plugin` data
source variant, so a plugin can generate rows on demand (per request) that
flow into the existing `${data.<source>.<column>}` interpolation. Motivating
use case: gRPC load tests where each request needs freshly generated,
Ed25519-signed transaction bytes inside a protobuf `bytes` field:

```yaml
plugins:
  - name: tx-signer
    path: ./target/release/libtx_signer.dylib
    config: { key_env: TX_SIGNING_KEY }

data:
  signed_tx:
    type: plugin
    source: tx-signer            # names the plugins: entry
    config: { chain_id: testnet-1 }

scenarios:
  submit:
    executor: constant-vus
    vus: 100
    duration: 5m
    flow:
      - request:
          name: submit tx
          protocol: grpc
          url: grpc://node:50051
          grpc:
            proto_files: [submit.proto]
            service: mempool.Submitter
            method: Submit
            message:
              tx: "${data.signed_tx.tx_b64}"   # bytes field <- base64 string
          checks:
            - { type: status, equals: 0 }
```

The gRPC handler stays **100% unchanged**: `prost-reflect`'s
`DynamicMessage::deserialize` (in `crates/loadr-protocols/src/grpc.rs`,
`GrpcHandler::execute`, ~line 394) already maps base64 JSON strings into
protobuf `bytes` fields. The whole feature is a narrow extension: one new FFI
trait, one new core trait, one new config enum variant, and a dispatch inside
`DataFeeds`.

## 2. Why this design (summary of the design doc)

- Payloads are time-sensitive and signed — they cannot be pre-generated into
  CSV/JSON, and the signature must land inside the protobuf message, not in
  gRPC metadata.
- A full native gRPC *protocol* plugin was rejected: it would reimplement
  transport, descriptor resolution, channel reuse, streaming, timeouts, and
  metrics — and the sync native ABI would force a plugin-owned runtime with
  `block_on`.
- JS-based signing is too slow for the target load; an external signer
  sidecar adds a network hop.
- Therefore: the plugin's only job is producing a signed byte field, exposed
  through the existing feeder/interpolation machinery.

## 3. Fixed design decisions (constraints — do not relitigate)

1. **Per-request freshness.** A plugin-backed source yields a fresh row for
   each *request preparation*. All `${data.x.*}` fields rendered during one
   request's preparation come from the same row. CSV/JSON/inline feeds keep
   their existing per-iteration caching. Rationale: a flow with two submits
   per iteration must not send the same signed tx twice.
2. **Minimal config surface.** `data.<name>: { type: plugin, source: <plugin
   name>, config: <object> }` — **no** `mode`/`on_eof`/`pick` fields (they
   describe stored-row iteration, which doesn't exist here). Plugin-signaled
   exhaustion behaves like `on_eof: stop`: the VU retires cleanly.
3. **Configs at init, not per call.** The design doc's conceptual ABI put
   plugin/source configs in every `next_row` request; this plan deviates
   deliberately: configs are delivered once via `init()`, and the per-call
   context is ~150 bytes. Required for the performance target.
4. **Performance target:** >50k rows/s per loadr instance for small signed
   payloads; MB-sized payloads must work (at whatever rate the network
   allows). Ed25519 signing (~25–50µs) runs inline synchronously on the
   tokio worker thread — precedent: JS interpolation already runs inline via
   `block_in_place`, and a 25–50µs CPU burst does not need offload. Do NOT
   use `spawn_blocking` per request (hand-off cost dominates).
5. **No semantic changes to `crates/loadr-agent` or
   `crates/loadr-cli/src/commands/agent.rs`.** Agent-side plugin loading is
   being added in a separate PR (#76 on the upstream repo, "fix(agent):
   Allow agent mode load custom local protocol plugins"); this feature
   composes with it later (see §9). One **mechanical** agent edit is
   required and permitted: `agent.rs` (~line 400) builds `EngineOptions` as
   a full struct literal without `..Default::default()`, so the new
   `data_sources` field needs a single added line
   `data_sources: Default::default(),` to keep the crate compiling. Nothing
   else in the agent changes (PR #76 touches the same file — expect a
   trivial merge).
6. **Never ship plugin binaries from controller to agents** (design-doc
   non-goal; the controller's `collect_files` correctly has no arm for the
   new variant since plugin sources reference no files).
7. Native (abi_stable) plugins only. No WASM/C-ABI data sources, no batch
   row API, no generic protobuf mutation hook, no async native ABI.

## 4. Verified codebase map (read this before coding)

### Config layer — `crates/loadr-config`
- `src/plan.rs` ~1908: `DataSource` enum, internally tagged
  `#[serde(tag = "type", rename_all = "snake_case")]`, variants
  `Csv`/`Json`/`Inline`, derives `Serialize, Deserialize, JsonSchema`.
  `TestPlan.data: IndexMap<String, DataSource>` (~line 49).
  `PluginRef { name, path: Option<PathBuf>, config: serde_json::Value,
  enabled: bool }` (~2118). Precedent for a plugin-provided variant:
  `OutputConfig::Plugin` (~2054).
- `src/validate.rs`: exhaustive `match` over `DataSource` at ~126–148 (this
  is a compile-forced touch point). Protocol-name-vs-plugins check precedent
  at ~590: `!self.plan.plugins.iter().any(|p| &p.name == protocol)` (note: it
  intentionally does not check `enabled`). `${data.<src>.*}` reference check
  at ~926 works for any variant (checks key existence in `plan.data` only).

### Runtime layer — `crates/loadr-core`
- `src/data.rs`: `pub type Row = IndexMap<String, String>` (all values
  stringified). Private `struct Feed { rows: Vec<Arc<Row>>, mode, on_eof,
  pick, shared_cursor: AtomicUsize }`. `DataFeeds::load(sources, base_dir)`
  (~52) eagerly loads all rows; exhaustive match over variants.
  `next_row(&self, source, state: &mut VuFeedState, rng: &mut impl RngExt)
  -> Result<Arc<Row>, NextRowError>` (~203).
  `NextRowError::{UnknownSource, Exhausted(EndOfData)}` (~263).
  `VuFeedState { cursors: HashMap<String, usize>, shuffles: ... }` is per-VU
  (no locks). `json_to_string` (~271): strings unquoted, other JSON values
  via `to_string()`.
- `src/vu.rs`: `VuContext` owns `data_state: VuFeedState` and
  `current_rows: HashMap<String, Arc<Row>>` (per-iteration row cache,
  cleared by `begin_iteration()` at ~125). `data_row(source)` (~131):
  cache-hit or `run.data.next_row(...)`. `resolve_expr` (~147) handles the
  `data.` prefix at ~158: splits `source.column` at the first `.`, calls
  `data_row`, returns `row.get(column).cloned()`. `pub fn json_to_string`
  (~185) is public. The `${iteration}` builtin exposes
  `iteration.saturating_sub(1)` (0-based; `begin_iteration` pre-increments).
- `src/flow.rs`: `run_request` (~989) → `self.prepare(req, vu, script)`
  (single call site, ~1011; `prepare` is fully synchronous). Error handling
  at ~1011–1019: `PrepareError::DataExhausted` → `RequestFlow::StopVu`
  (VU retires); `PrepareError::Other(e)` → log + `http_req_failed` rate with
  tags `("name", ...), ("error", "prepare")` + `RequestFlow::Continue`.
  In `render_template` (~1977), `vu.resolve_expr` errors map at ~2020–2023:
  `NextRowError::Exhausted → PrepareError::DataExhausted`; **any other
  `NextRowError` hits the catch-all → `PrepareError::Other`** — so a new
  error variant needs zero flow.rs mapping changes. `render_json` (~2042):
  a lone `${expr}` JSON leaf is re-parsed via `serde_json::from_str`
  (~2055), falling back to string — that's how typed values re-enter JSON.
- **Checks do not interpolate templates.** `checks:` are compiled once into
  `CompiledCondition` (`src/conditions.rs` ~91: `BodyContains { value:
  value.clone(), .. }` — a literal clone; no `Template` involvement anywhere
  in `conditions.rs`), and `eval_condition` (flow.rs ~1196) evaluates them
  against the response only. A `${data.*}` inside a check value is compared
  as the literal string — do not design tests or docs around interpolated
  check values.
- `src/engine.rs`: `EngineOptions { run_id, protocols, script, outputs,
  partition, extra_tags, snapshot_interval }` (~37, plus `Default` at ~49).
  `Engine::new` (~177) calls `DataFeeds::load(&plan.data, &base_dir)?` at
  ~238 and builds `Arc<RunContext>` (~240) — all before any VU starts, so
  errors here are the pre-VU fast-fail (and in agent mode they fail the
  assignment before the synchronized start barrier).
- `src/metrics.rs` has `now_millis()` (already used across the codebase).

### Plugin ABI — `crates/loadr-plugin-api` (abi_stable 0.11)
- `src/abi.rs`: design note (lines 1–9): all rich data crosses FFI as JSON in
  `RString`s; additive payload changes are never ABI breaks.
  `LOADR_PLUGIN_ABI_VERSION: u32 = 1` (~25). `#[sabi_trait]` traits:
  `FfiOutput: Send`, `FfiProtocol: Send + Sync`, `FfiService: Send`
  (`start(&mut self, config_json) -> RResult<RString, RString>` — proof that
  `&mut self` works in sabi traits). Root module:
  ```rust
  #[repr(C)]
  #[derive(StableAbi)]
  #[sabi(kind(Prefix(prefix_ref = PluginModRef)))]
  #[sabi(missing_field(panic))]
  pub struct PluginMod {
      pub abi_version: u32,
      pub info: extern "C" fn() -> RString,
      pub make_output: ROption<extern "C" fn() -> FfiOutputBox>,
      pub make_protocol: ROption<extern "C" fn() -> FfiProtocolBox>,
      #[sabi(last_prefix_field)]
      pub make_service: ROption<extern "C" fn() -> FfiServiceBox>,
  }
  ```
  `export_loadr_plugin!` macro (~121) wraps `export_root_module` +
  `leak_into_prefix()`.
- **Verified abi_stable 0.11.3 semantics** (from its own
  `src/docs/prefix_types.rs`): fields appended *after* the
  `#[sabi(last_prefix_field)]` marker are suffix fields — libraries compiled
  without them still load; the marker must NOT move (moving it is an ABI
  break); the struct-level `missing_field(panic)` can be overridden
  per-field, and `#[sabi(missing_field(default))]` makes the accessor return
  `Default::default()` (= `RNone` for `ROption`) when the field is absent.
  Therefore the extension below requires **no ABI version bump**.
- `src/native.rs`: `NativePlugin::load` via `lib_header_from_path` (layout +
  version validated per library; libraries intentionally leaked).
  `make_output(config)` / `make_protocol(config)` bake the merged config
  into the adapter at construction (`NativeOutputAdapter { name, config,
  inner }`) — mirror this. `make_service()` currently errors with
  `KindMismatch` when the ctor is `RNone`. `NativeServiceAdapter` bridges
  `FfiServiceBox` → `trait ServicePlugin`.
- `src/registry.rs`: `LoadedPlugin` enum (~101): `Extractor | Assertion |
  Output(Box<dyn loadr_core::Output>) | Protocol(Arc<dyn
  loadr_core::ProtocolHandler>) | Service(Box<dyn ServicePlugin>)`.
  Constructed in `load_with_config` (`(Native, PluginKind::Service)` arm,
  ~198) and `load_path` (~303, used for `plugins: [{path: ...}]` refs where
  kind comes from the plugin's `info()`). `load_ref` (~219) is the plan
  entry point. **`loadr-plugin-api` already depends on `loadr-core`** — the
  core-facing trait goes in loadr-core, the adapter in plugin-api.
- `src/manifest.rs`: `plugin.toml` — `ManifestPlugin { name, version, kind,
  type, entry, abi?, description, schemes }`; no `deny_unknown_fields`, no
  `capabilities` field yet. `PluginKind::{Extractor, Assertion, Output,
  Protocol, Service}`. `merged_config` = manifest `[config]` defaults with
  `PluginRef.config` shallow-merged on top.

### CLI wiring — `crates/loadr-cli/src/commands/run.rs`
- `build_engine` (~119): iterates `plan.plugins` (skipping `enabled: false`),
  `PluginRegistry::load_ref`, routes variants at ~151–163
  (`Service(service) => services.push(service)`), then constructs
  `Engine::new(plan, base_dir, EngineOptions { run_id, protocols, script,
  outputs, ..Default::default() })` (~186). Returns `(Engine,
  Vec<Box<dyn ServicePlugin>>)`.
- This is the **only** engine construction site in the CLI: `sweep`
  re-invokes the loadr binary as a subprocess; `compare`/`report`/`payload`
  don't build engines. (`agent.rs:397` builds one too, via a full
  `EngineOptions { ... }` literal without `..Default::default()` — it gets
  the one mechanical `data_sources: Default::default(),` line allowed by
  §3.5 and nothing else.)
- Known pre-existing gap, **not to be fixed in this PR**:
  `ServicePlugin::start()` is never called by the host (only `stop()` at
  teardown, ~426). Data-source plugins don't need it — the new `init()` is
  their setup hook. Mention in PR description as a known gap.

### Tests / docs / examples infrastructure
- `crates/loadr-plugin-api/tests/common/mod.rs` provides
  `build_native_example(crate_name, lib_stem)` (cargo-builds example plugin
  cdylibs, platform-correct artifact names) and is used by
  `tests/native_plugins.rs` ("build the example cdylibs, load them via
  abi_stable, drive the core-facing adapters").
- `testsupport/loadr-testserver` exports `GrpcEchoServer` (tonic echo:
  unary + streaming), `pb`, and `FILE_DESCRIPTOR_SET`. gRPC e2e tests live
  in `crates/loadr-cli/tests/e2e.rs`.
- Docs (mdBook, `docs/src/`): data feeders documented in
  `docs/src/yaml/data.md`; plugin authoring in `docs/src/plugins/native.md`
  and `developing.md`. **The docs already promise this feature's syntax**:
  `docs/src/plugins/faker-gen.md:73-74` shows `type: plugin` + `source:`
  (implement exactly this); `docs/src/plugins/sql-feeder.md:84` uses
  `service:` (reconcile to `source:`).
- Examples are numbered YAML plans in `examples/` (currently up to
  `49-payload-complexity.yaml`; there are also `examples/plugins/` and
  `examples/protos/` subdirs).
- No checked-in JSON schema artifact exists; nothing to regenerate.

## 5. Implementation steps

Keep each step compiling and testable; suggested commit boundaries are the
step boundaries (fold the small cross-crate match-site fixes into the step
that forces them so every commit builds).

### Step 1 — `loadr-config`: new variant + validation

`crates/loadr-config/src/plan.rs`, after the `Inline` variant:

```rust
/// Rows generated on demand by a `data_source`-capable plugin listed under
/// `plugins:`. Rows are per-request: each request preparation pulls a fresh
/// row; a plugin that reports exhaustion retires the VU (like `on_eof: stop`).
Plugin {
    /// Plugin name (a `plugins:` entry) that generates the rows.
    source: String,
    /// Source-level config passed to the plugin at init.
    #[serde(default, skip_serializing_if = "serde_json::Value::is_null")]
    config: serde_json::Value,
},
```

`crates/loadr-config/src/validate.rs`, new arm in the exhaustive match
(mirror the protocol check's style, including not filtering on `enabled`):

```rust
DataSource::Plugin { source, .. } => {
    if source.is_empty() {
        self.error(
            format!("data.{name}.source"),
            "plugin data source needs a `source` plugin name",
        );
    } else if !self.plan.plugins.iter().any(|p| &p.name == source) {
        self.push(
            Diagnostic::error(
                format!("data.{name}.source"),
                format!("data source references plugin `{source}` which is not listed under `plugins:`"),
            )
            .with_suggestion(
                "declare the plugin under `plugins:`; it must provide the data_source capability",
            ),
        );
    }
}
```

Tests in the existing test modules: `type: plugin` YAML round-trips to the
variant (config defaulting to `Null` when omitted); empty/unknown `source`
produces the diagnostics; a plan combining `type: plugin` with `${data.x.y}`
references passes the reference check.

### Step 2 — `loadr-core`: capability trait, plugin feeds, freshness

`crates/loadr-core/src/data.rs`:

```rust
/// Identity of the caller pulling a row (used only by plugin-backed feeds).
#[derive(Clone, Copy)]
pub struct RowIdentity<'a> {
    pub vu: u64,
    pub iteration: u64,          // 0-based, matches `${iteration}`
    pub scenario: &'a str,
    pub request: Option<&'a str>,
}

/// Full context for one on-demand row generation (hot path).
pub struct PluginRowCtx<'a> {
    pub source: &'a str,
    pub vu: u64,
    pub iteration: u64,
    pub seq: u64,                // monotonic per-VU, per-source counter
    pub scenario: &'a str,
    pub request: Option<&'a str>,
    pub ts_ms: u64,              // core-supplied wall clock
}

pub enum PluginRowResult {
    Row(Row),
    Exhausted,
}

/// Core-facing `data_source` plugin capability. `next_row` runs on the
/// request hot path, concurrently across VU worker threads.
pub trait DataSourcePlugin: Send + Sync {
    fn name(&self) -> &str;
    /// One-time setup before VUs start: `source_configs` maps each
    /// `data.<name>` backed by this plugin to its `config:` value.
    fn init(
        &mut self,
        source_configs: &IndexMap<String, serde_json::Value>,
    ) -> Result<(), String>;
    fn next_row(&self, ctx: &PluginRowCtx<'_>) -> Result<PluginRowResult, String>;
}
```

- Rename the private `Feed` struct to `MemoryFeed`; introduce
  `enum Feed { Memory(MemoryFeed), Plugin(Arc<dyn DataSourcePlugin>) }`
  (all private — no API leak).
- `MemoryFeed` keeps `#[derive(Debug)]`, but `Arc<dyn DataSourcePlugin>` is
  not `Debug`: give the new `Feed` enum a **manual** `Debug` impl (delegate
  for `Memory`, write the plugin name for `Plugin`) so `DataFeeds` keeps its
  existing `#[derive(Debug, Default)]`. Do not add a `Debug` bound to
  `DataSourcePlugin`.
- `DataFeeds` precomputes `has_on_demand: bool` and exposes
  `pub fn has_on_demand(&self) -> bool` + `pub fn is_on_demand(&self, name:
  &str) -> bool`.
- `load` gains a third parameter:
  `mut plugins: HashMap<String, Box<dyn DataSourcePlugin>>`.
  For plugin sources: first collect, per plugin name, an
  `IndexMap<String /* feed name */, serde_json::Value /* config */>`; then
  per plugin: `plugins.remove(name)` — `None` ⇒
  `EngineError::Data { source_name, message: "plugin `<name>` is not loaded
  or does not provide the data_source capability" }` (this is the pre-VU
  fast-fail); call `init(&group_configs)` (error ⇒ `EngineError::Data`);
  `Arc::from(boxed)`; insert one `Feed::Plugin(Arc::clone(..))` per feed
  name. Plugins present in the map that no `data.*` entry references are
  dropped without init.
- `next_row` gains `id: &RowIdentity<'_>`. Memory arm unchanged (ignores
  `id`). Plugin arm: derive `seq` from the existing `state.cursors` entry
  (read then post-increment — same mechanism the per-VU cursor uses), build
  `PluginRowCtx` with `ts_ms: crate::metrics::now_millis()`, then:
  `Ok(Row)` → `Ok(Arc::new(row))`; `Ok(Exhausted)` →
  `Err(NextRowError::Exhausted(EndOfData(source.into())))`; `Err(message)` →
  `Err(NextRowError::Plugin { source: source.into(), message })`.
- New error variant:
  ```rust
  #[error("data source `{source}`: plugin error: {message}")]
  Plugin { source: String, message: String },
  ```
  No flow.rs change needed: the catch-all at ~2023 already maps unknown
  `NextRowError`s to `PrepareError::Other` (failed request, VU continues),
  and `Exhausted` keeps meaning StopVu — exactly the decided semantics.
- Update the ~15 in-repo `load`/`next_row` call sites (loadr-core tests,
  `vu.rs` test helper, `crates/loadr-browser/tests/browser.rs:36`,
  `crates/loadr-protocols/tests/{integration.rs:47,sse.rs:20}`): pass
  `HashMap::new()` / a shared `RowIdentity` test-helper. Mechanical.

`crates/loadr-core/src/vu.rs`:

- `VuContext` gains `pub current_request: Option<String>`.
- New method:
  ```rust
  /// Begin preparing a request: plugin-backed rows are per-request, so evict
  /// them from the iteration cache and remember the request name for row ctx.
  pub fn begin_request(&mut self, name: &str) {
      if !self.run.data.has_on_demand() {
          return;
      }
      self.current_request = Some(name.to_string());
      let data = &self.run.data;
      self.current_rows.retain(|src, _| !data.is_on_demand(src));
  }
  ```
  (Disjoint field borrows — `&self.run` immutable vs `&mut
  self.current_rows` — compile fine inside one method body.)
- `data_row` builds
  `RowIdentity { vu: self.vu_id, iteration: self.iteration.saturating_sub(1),
  scenario: &self.scenario, request: self.current_request.as_deref() }`
  and passes it to `next_row`. Plugin rows still enter `current_rows`, so
  every `${data.x.*}` field rendered during one request's **preparation**
  sees the same row; eviction happens at the next `begin_request`. (Checks
  are compiled literals — see §4 — so same-row claims stop at preparation.)
- `current_request` is informational context for plugin authors, meaning
  "the request currently being prepared, if any": set by `begin_request`,
  cleared to `None` right after `prepare` returns, and reset to `None` in
  `begin_iteration`. A plugin row first fetched outside request preparation
  (e.g. from a JS step) then gets `request: None`, never a stale name.

`crates/loadr-core/src/flow.rs` — restructure the prepare call in
`run_request` (~1011) so the clear runs on **every** outcome. This matters:
the `PrepareError::Other` arm returns `RequestFlow::Continue`, so the same
iteration can proceed into e.g. a JS step, which must not observe a stale
request name. Bind the result, clear, then match:

```rust
vu.begin_request(&req.display_name);
let prepared_result = self.prepare(req, vu, script);
vu.current_request = None;

let mut prepared = match prepared_result {
    Ok(p) => p,
    Err(PrepareError::DataExhausted) => return RequestFlow::StopVu,
    Err(PrepareError::Other(e)) => { /* existing error handling, unchanged */ }
};
```

(Retries re-enter `run_request`, so a retried request gets a freshly signed
payload — correct for time-sensitive signatures; document this.)

`crates/loadr-core/src/engine.rs`: `EngineOptions` gains
`pub data_sources: HashMap<String, Box<dyn crate::data::DataSourcePlugin>>`
(+ `Default`); `Engine::new` forwards it into `DataFeeds::load`. Re-export
`DataSourcePlugin`, `PluginRowCtx`, `PluginRowResult`, `RowIdentity` from
`src/lib.rs` (same pattern as `Output`/`ProtocolHandler`).

Tests (in-crate, with a fake `DataSourcePlugin` using atomics): rows
returned and `seq` monotonic per VU/source; plugin error →
`NextRowError::Plugin`; exhausted → `Exhausted`; missing plugin →
`EngineError::Data` naming the source; `init` receives the grouped source
configs; concurrent `next_row` via threads over `Arc<DataFeeds>`; VU level —
same row within a request, fresh row after `begin_request`, memory feeds
unaffected by eviction, ctx carries request name and 0-based iteration.

### Step 3 — `loadr-plugin-api`: FFI trait, ABI extension, adapter, registry

`src/abi.rs`:

```rust
/// An on-demand data source (`data.<name>.type: plugin`). `next_row` is on
/// the request hot path and is called concurrently from VU threads.
#[sabi_trait]
pub trait FfiDataSource: Send + Sync {
    fn name(&self) -> RString;

    /// Called once before VUs start. `init_json`:
    /// `{"plugin_config": <merged [config] + PluginRef.config>,
    ///   "sources": {"<data name>": <data.<name>.config>, ...}}`
    fn init(&mut self, init_json: RString) -> RResult<(), RString>;

    /// `ctx_json`: `{"source","vu","iteration","seq","scenario","request"?,"ts_ms"}`.
    /// Returns `{"row": {"col": <scalar>, ...}}` or `{"exhausted": true}`.
    fn next_row(&self, ctx_json: RString) -> RResult<RString, RString>;
}

pub type FfiDataSourceBox = FfiDataSource_TO<'static, RBox<()>>;
```

Extend `PluginMod` — append **after** `make_service`; the
`#[sabi(last_prefix_field)]` marker MUST stay on `make_service`:

```rust
    #[sabi(last_prefix_field)]
    pub make_service: ROption<extern "C" fn() -> FfiServiceBox>,
    /// Suffix field: plugins compiled before it existed still load; the
    /// accessor then returns `RNone`. Not an ABI break — version stays 1.
    #[sabi(missing_field(default))]
    pub make_data_source: ROption<extern "C" fn() -> FfiDataSourceBox>,
```

Do **not** bump `LOADR_PLUGIN_ABI_VERSION`. Update the macro doc example.

⚠️ **Compile ripple:** every in-repo crate constructing a `PluginMod { ... }`
literal needs one added line `make_data_source: RNone,`. Find them with
`rg -l 'export_loadr_plugin'` across `plugins/` (~most of the 35 native
plugins + `plugins/examples/native-output`, `native-protocol`). Purely
mechanical; runtime compatibility for already-compiled dylibs is preserved,
which is the compatibility that matters.

`src/native.rs`:

- `NativeDataSourceAdapter { name: String, config: serde_json::Value, inner:
  FfiDataSourceBox }` implementing `loadr_core::DataSourcePlugin` (mirror
  `NativeOutputAdapter`): `init` serializes
  `{"plugin_config": &self.config, "sources": source_configs}`; `next_row`
  serializes the ctx (serde structs `FfiRowCtx<'a>` /
  `FfiRowResponse { #[serde(default)] row: Option<serde_json::Map<..>>,
  #[serde(default)] exhausted: bool }`), maps `RErr` → `Err(String)`, parse
  failure → `Err("invalid row JSON: ...")`, `exhausted: true` →
  `PluginRowResult::Exhausted`, and stringifies row values with
  `loadr_core::vu::json_to_string` (same typing semantics as JSON/inline
  feeds).
- `NativePlugin::make_data_source(&self, config: serde_json::Value) ->
  Option<NativeDataSourceAdapter>` (`RNone` ⇒ `None`; the suffix accessor
  returns `ROption` just like `make_service`'s). **Keep the existing
  `make_service() -> Result<NativeServiceAdapter, PluginError>` signature
  unchanged** — it is public API; add a sibling probe
  `maybe_service(&self) -> Option<NativeServiceAdapter>` and let the
  registry construct the "provides neither service nor data_source"
  `KindMismatch` error from the two `Option`s.

`src/registry.rs` — restructure the variant:

```rust
Service {
    service: Option<Box<dyn ServicePlugin>>,
    data_source: Option<Box<dyn loadr_core::data::DataSourcePlugin>>,
},
```

Both `(Native, PluginKind::Service)` arms (`load_with_config` ~198 and
`load_path` ~303) construct both capabilities from the loaded module and
error with `KindMismatch` only when **neither** is present — a signer plugin
may be data-source-only (`make_service: RNone`); existing service plugins are
untouched. `kind()` still returns `PluginKind::Service`. Update the
`LoadedPlugin` match sites (registry `kind()`, `run.rs` routing, plugin-api
tests asserting variants).

`src/manifest.rs` (small, optional-but-included): `#[serde(default)]
capabilities: Vec<String>` on `ManifestPlugin`, `pub capabilities:
Vec<String>` on `PluginManifest`. Informational only (the design doc's
manifest example declares `capabilities = ["data_source"]`); authoritative
capability detection remains the module's `make_data_source` presence. Add a
parse test.

### Step 4 — `loadr-cli`: wiring (~8 lines)

`run.rs build_engine`: collect
`let mut data_sources: HashMap<String, Box<dyn loadr_core::DataSourcePlugin>>`,
route:

```rust
loadr_plugin_api::LoadedPlugin::Service { service, data_source } => {
    if let Some(s) = service {
        services.push(s);
    }
    if let Some(ds) = data_source {
        data_sources.insert(plugin_ref.name.clone(), ds);
    }
}
```

Key by **`plugin_ref.name`** — the name that `data.<x>.source` and the
validator refer to (not the plugin's self-reported `info().name`). Pass
`data_sources` in `EngineOptions`.

### Step 5 — Example plugin + tests

**`plugins/examples/native-data-source/`** — mirror `native-protocol`'s
layout (`Cargo.toml` with `crate-type = ["cdylib"]`, `plugin.toml` with
`kind = "service"`, `type = "native"`, `capabilities = ["data_source"]`,
entry = platform dylib name). An Ed25519 signer using `ed25519-dalek`
(new dependency for this example crate only — check `deny.toml` passes):

- `init`: read `key_hex` (or derive a key deterministically from a `seed`
  integer) from `plugin_config`; store `SigningKey` and per-source settings
  (e.g. `chain_id`, optional `limit`). No per-request I/O afterwards.
- `next_row`: build a compact payload from `{chain_id, vu, seq, ts_ms}`,
  sign it, return
  `{"row": {"tx_b64": base64(payload ‖ signature), "nonce": "<vu>:<seq>"}}`.
  Uniqueness comes from `(vu, seq)` — no locks needed. When `limit` is set
  and reached (an `AtomicU64` counter), return `{"exhausted": true}`.

**Tests:**

- `crates/loadr-plugin-api/tests/native_plugins.rs` (using
  `common::build_native_example`): loading yields
  `LoadedPlugin::Service { data_source: Some(_), .. }`; adapter
  `init` + `next_row` roundtrip; **the signature verifies with
  `ed25519-dalek` in the test** (this is the design doc's "server verifies
  the signed bytes" acceptance, proven at the adapter boundary); `limit` →
  `Exhausted`; invalid config → init error; concurrent `next_row` from
  several threads yields unique nonces; a service plugin without the
  capability still loads (`data_source: None`).
- **`testsupport/loadr-testserver`** (small additive change): add
  `bytes payload = 3;` to both `EchoRequest` and `EchoResponse` in
  `proto/echo.proto`, and copy `req.payload` into responses in the `Echo`
  service impl (`src/grpc.rs`). The `build.rs` recompiles the proto with
  protox and regenerates the descriptor set + tonic code on change — no
  manual codegen step. Without this, the echo proto has only
  `string message`, and the e2e would prove interpolation through gRPC but
  not the base64-string → protobuf `bytes` mapping, which is the crux of the
  design. Additive proto3 field — existing tests unaffected.
- `crates/loadr-cli/tests/e2e.rs`: full binary run — plan with
  `plugins: [{name: tx-signer, path: <built dylib>}]`,
  `data.signed_tx: {type: plugin, source: tx-signer}`, a gRPC request
  against `GrpcEchoServer` sending
  `message: { message: "sig", payload: "${data.signed_tx.tx_b64}" }` (the
  new `bytes` field) with `checks: [{type: status, equals: 0}]`; assert the
  JSON summary shows `grpc_reqs > 0` and `http_req_failed == 0`. Check
  values are compiled literals (§4) — do **not** attempt
  `body_contains: "${data...}"`. Optional strengthening: give the example
  signer a fixed payload prefix (e.g. `chain_id` bytes first), making the
  leading base64 characters deterministic, and assert that **literal**
  prefix with `body_contains` on the echoed response. Add a second e2e: a
  plan whose `data.*.source` names a plugin without the capability exits
  non-zero **before** starting VUs, with the "not loaded or does not provide
  the data_source capability" error.
- Perf smoke: an `#[ignore]`d test in plugin-api hammering the adapter
  single-threaded for ~1s and printing rows/s (no CI assertion — flaky);
  record the number in the PR description. Expect ≥20–40k rows/s/core with
  real signing; adapter overhead alone should exceed 300k rows/s.

### Step 6 — Docs + example plan

- `docs/src/yaml/data.md`: document `type: plugin` — syntax; per-request
  freshness (contrast with per-iteration caching of file feeds; note retry
  behavior); exhaustion retires the VU; plugin errors count as failed
  requests (`error:prepare` tag) and the run continues; distributed note:
  the plugin must be installed on every agent, assignments fail pre-barrier
  otherwise; plugins are never shipped over the wire.
- `docs/src/plugins/native.md` (and/or `developing.md`): authoring section
  for the `data_source` capability — `FfiDataSource`, the init/ctx/row JSON
  contracts, `Send + Sync` requirement ("called concurrently from VU
  threads"), "load keys and static config at init, no per-request I/O",
  base64 for protobuf `bytes` fields, and the `make_data_source` field in
  `export_loadr_plugin!`.
- Reconcile promised syntax — the config surface is exactly
  `{type, source, config}`, so **remove `mode`/`on_eof`/`pick` from every
  plugin-feeder example**: `docs/src/plugins/faker-gen.md` (~73-80: keeps
  `type: plugin` + `source:`, drops the iteration knobs),
  `docs/src/plugins/sql-feeder.md` (~84-90: `service:` → `source:`, drop
  the knobs), and the plan example in `docs/custom-grpc-plugin-feeder.md`
  (~93: drop `mode: shared`). Note that the faker/sql plugins themselves
  adopt the capability in follow-ups.
- In `yaml/data.md`, state explicitly that `mode`/`pick`/`on_eof` do not
  apply to `type: plugin` (there are no stored rows or cursors) **and are
  ignored if present**: serde's internally-tagged enums cannot reject
  unknown fields, and adding reject-only fields would pollute the JSON
  schema. A raw-YAML unknown-key lint in `loadr validate` is a possible
  follow-up (§9).
- Update `docs/custom-grpc-plugin-feeder.md` with a short "Status:
  implemented" note linking to the two pages above.
- Add `examples/50-plugin-data-source.yaml` showing the signed-tx gRPC shape
  (reference the example plugin by `path:`).

## 6. Performance analysis (why this meets the target)

Per `next_row` call: ctx serialize (~150 B) ≈ 0.3µs + FFI hop (ns) +
plugin-side ctx parse ≈ 0.3µs + **Ed25519 sign 25–50µs (dominant)** + row
JSON build/parse/stringify ≈ 1µs + `Arc<Row>` alloc ≈ 0.2µs ⇒ ~2µs plumbing,
<10% of signing cost. Calls run concurrently on VU tokio workers with no
shared locks in the path (per-VU `seq`, plugin-side atomics), so throughput
scales with cores: 50k signs/s ≈ 1.5–2.5 cores of signing + ~0.1 core of
plumbing. Delivering configs at `init` keeps per-call payloads constant-size
regardless of config size. MB-scale base64 rows cost ~1–3 ms each in JSON
parse+copy on the host (plus the render-path clones every feed already
pays) — acceptable at the request rates MB payloads imply; a raw-bytes side
channel and `Arc<str>` row values are follow-ups only if profiling demands.
Batch row generation is deliberately excluded: time-sensitive signatures
must not be pre-generated, and signing dominates FFI overhead anyway.

## 7. Acceptance checklist (from the design doc, adjusted)

- [ ] Config parses `data.<name>.type: plugin` (with and without `config`).
- [ ] Validation fails when the referenced plugin is missing from `plugins:`.
- [ ] Run fails before VUs start when the plugin is missing the capability
      or `init` errors.
- [ ] Native plugin API test loads the example data source and pulls rows
      concurrently.
- [ ] `${data.signed_tx.tx_b64}` invokes the plugin per request; all fields
      rendered during one request's preparation share a row; consecutive
      requests get fresh rows.
- [ ] gRPC e2e sends the signed bytes through a real protobuf `bytes` field
      (extended echo proto) end-to-end; signature verified with the plugin's
      public key (adapter-level test).
- [ ] Plugin `next_row` errors become failed requests (`error:prepare`),
      never panics/aborts; exhaustion retires the VU cleanly.
- [ ] `cargo fmt --all` clean; `cargo clippy --workspace --all-targets`
      clean.

## 8. Verification commands

```sh
cargo fmt --all
cargo clippy --workspace --all-targets
cargo test -p loadr-config -p loadr-core -p loadr-plugin-api
cargo test -p loadr-cli --test e2e          # builds example dylib, runs gRPC echo
cargo test -p loadr-plugin-api -- --ignored # perf smoke, prints rows/s
```

Manual: build the release binary and run
`loadr run examples/50-plugin-data-source.yaml` (uses the example plugin +
a local gRPC target); `loadr validate` on a plan with an undeclared
`source:` must report the diagnostic.

## 9. Risks & follow-ups

- **abi_stable suffix-field behavior** is verified from its 0.11.3 docs, but
  the "old compiled dylib against new host" path can't be unit-tested
  in-tree. Fallback if anything surprises in practice: bump
  `LOADR_PLUGIN_ABI_VERSION` to 2 (cost: all native plugins rebuild — all
  are in-repo, so acceptable).
- **Distributed runs**: until agent-side plugin loading (PR #76) is extended
  to service/data-source plugins, agents build `EngineOptions` without
  `data_sources`, so plans with plugin feeds fail the assignment
  **pre-barrier** with the clear "not loaded / lacks capability" error —
  which is the design doc's required behavior. Follow-up: after #76 lands,
  populate `data_sources` in the agent's engine setup the same way
  `build_engine` does.
- **Cross-agent row semantics**: each agent generates independently
  (`vu`/`seq` are per-process). Fine for signing; if global uniqueness is
  needed, plugins can mix in an agent identifier from env. Documented, not
  solved here.
- **Unknown-field strictness**: serde internally-tagged enums silently
  ignore unknown fields, so `mode:`/`pick:`/`on_eof:` on a `type: plugin`
  source are ignored rather than rejected. Handled by making all docs and
  examples consistent (Step 6); a raw-YAML unknown-key lint in
  `loadr validate` is a possible follow-up.
- **Typed-leaf edge (pre-existing, all feeds)**: a lone `${expr}` JSON leaf
  that parses as a JSON scalar re-types (`render_json`); an all-digit short
  value would become a number and fail `bytes` deserialization. Real base64
  with padding never hits this; the authoring docs should tell plugin
  authors to emit genuine base64 strings.
- **Out of scope, do not do here**: wiring `ServicePlugin::start()`
  (pre-existing gap — separate fix); porting faker-gen/sql-feeder to the
  capability; WASM/C-ABI data sources; batch row API; per-message freshness
  inside WS/streaming flows; shipping plugin binaries to agents.

## 10. Style expectations

- Match surrounding code: comment density and doc-comment style of
  `data.rs`/`abi.rs` (short, contract-stating doc comments; no
  narrate-the-diff comments).
- Rust 2021, `cargo fmt --all` mandatory (design-doc acceptance item).
- Errors follow existing enums (`EngineError::Data`, `NextRowError`,
  `PluginError`) — no new error crates or ad-hoc `anyhow` in core.
- Keep the FFI JSON contracts documented on the traits themselves (the
  `abi.rs` module doc is the precedent).

