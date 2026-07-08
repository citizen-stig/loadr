# Design Spec: Spec-Driven Generation & API Fuzzing

## 1. Goal & user story

Today loadr can *import* recorded traffic (`loadr convert`, `loadr record`). It cannot start from an
API *contract*. A team with an OpenAPI/GraphQL/gRPC/Postman definition but no traffic yet has nothing
to load-test.

`loadr gen` closes that gap: point it at a contract and it emits a runnable loadr YAML plan with one
request per operation, every parameter and body filled from schema-derived example data.

> **User story.** "I have `openapi.yaml`. I run `loadr gen openapi openapi.yaml -o plan.yaml`, then
> `loadr run plan.yaml`, and every endpoint gets exercised with valid example data — no hand-writing."

The same schema knowledge powers **fuzzing**: `--fuzz` additionally emits boundary and spec-invalid
variants of every operation and asserts the server answers 2xx–4xx but **never 5xx**, turning a
contract into a robustness suite. This marries the existing adversarial payloads in `loadr-payload`
(nested JSON, billion-laughs, ReDoS) with schema-aware structural mutations.

## 2. CLI / API surface

Mirrors `loadr convert` (`crates/loadr-cli/src/commands/convert.rs`): read a file, produce a
`Conversion`, print warnings to stderr, write YAML to `-o`/stdout with a provenance header.

```
loadr gen <source> <input> [-o out.yaml] [--fuzz] [--base-url URL] [--server N] [--include GLOB] [--exclude GLOB]

  <source>   openapi | graphql | grpc | postman
  <input>    contract file (.yaml/.json/.graphql/.proto)
  -o, --output      output YAML path (default: stdout)
  --base-url        override the base URL (default: derived from spec `servers`/host)
  --server          index into OpenAPI `servers[]` (default 0)
  --include/--exclude  operationId/path globs to select a subset
  --fuzz            also emit boundary + spec-invalid variants with a "no 5xx" gate
  --fuzz-payloads   comma list of loadr-payload kinds to inject (default: nested-json,long-string)
```

`<source>` is an explicit subcommand-like positional (not inferred from extension) because `.json`
is ambiguous between OpenAPI, Postman, and a GraphQL introspection dump. Output matches `convert`:

```
warning: [GET /pets/{petId}] no example for path param `petId`; used schema default
✓ wrote plan.yaml (34 request(s), 3 warning(s))
```

Wiring: add `Command::Gen(commands::gen::GenArgs)` to the enum in `crates/loadr-cli/src/main.rs`
(alongside `Convert`, `Compare`, `Record`) and dispatch to `commands::gen::execute`.

## 3. Architecture

New crate **`crates/loadr-gen`**, structured exactly like `loadr-convert`:

- `lib.rs` — re-exports `gen_openapi`, `gen_graphql`, `gen_grpc`, `gen_postman`; defines `GenError`
  (thiserror, one variant per bad-input class, mirroring `ConvertError`).
- `openapi.rs`, `graphql.rs`, `grpc.rs`, `postman.rs` — one generator per source.
- `example.rs` — the shared **schema → `serde_json::Value` example** engine (the heart of the crate).
- `fuzz.rs` — turns one example into valid/boundary/invalid variants.

It **reuses `loadr_convert::{Conversion, ConversionWarning}`** as its return type rather than inventing
a parallel type — `loadr-gen` depends on `loadr-convert` for those two structs, exactly as `har.rs`
already builds a `Conversion` around a `loadr_config::TestPlan`. Everything is best-effort: anything
unrepresentable becomes a `ConversionWarning`, and the emitted plan passes `loadr_config::load_str`
with `deny_errors: true` (the invariant `har.rs`'s `plan_passes_validation` test enforces).

Like `convert_har`, OpenAPI/Postman/GraphQL-introspection inputs are **JSON or YAML parsed straight
to `serde_json::Value`** — no schema-parser dependency. Cargo deps: `loadr-config`, `loadr-convert`,
`serde_json`, `serde_yaml` (promote from dev-dep), `regex`, `thiserror`, `indexmap`, plus
`loadr-payload` for `--fuzz`. gRPC is the exception (see risks).

`commands/gen.rs` is a thin adapter (~60 lines, cloned from `convert.rs`): match `<source>` to the
generator, print warnings via `owo_colors`, write the header + `serde_yaml::to_string(&plan)`.

## 4. Key data structures & algorithms

**Plan shape** (identical to `convert_har`'s output). One `Scenario` named after the spec title,
`executor: ExecutorKind::ConstantVus`, `vus: Some(1)`, `duration: Some(Dur::from_secs(60))`,
`defaults.http.base_url` from `servers[0].url`, `flow: Vec<Step>` of `Step::Request(Box<RequestStep>)`.

**OpenAPI operation → `RequestStep`.** Walk `paths.{path}.{method}`:
- `method` ← the HTTP verb; `url` ← `path` with `{param}` path-params rewritten to `${param}` and a
  matching `variables` entry seeded with the example.
- `parameters[]` by `in`: `path` → `${param}` in url + a seed var; `query` → `RequestStep.params`
  (`IndexMap<String,String>`); `header` → `RequestStep.headers`.
- `requestBody.content["application/json"].schema` → `Body::Spec(BodySpec { json: Some(example), .. })`;
  `application/x-www-form-urlencoded` → `BodySpec.form`; `multipart/form-data` → `BodySpec.multipart`.
- `name` ← `operationId` (fallback `"{METHOD} {path}"`), matching `har.rs`'s naming.
- A `Condition::Status { one_of: Some(<declared 2xx codes>), .. }` assertion so the baseline plan is
  self-checking.

**`example.rs` — `example_for(schema, resolver, &mut Ctx) -> Value`.** Precedence, highest first:
`example` / `examples[0]` → `default` → `enum[0]` → by `type`. Type defaults: `string` →
format-aware (`date-time`→RFC3339, `uuid`→a fixed UUID, `email`→`user@example.com`, else `minLength`
'a's or `"string"`); `integer`/`number` → `minimum` (respecting `exclusiveMinimum`) else `0`;
`boolean` → `false`; `array` → one element from `items` (honouring `minItems`); `object` → recurse
over `properties`, always emitting `required` keys and up to N optional ones. `allOf` deep-merges
member schemas; `oneOf`/`anyOf` take `[0]` with a warning.

**`$ref` resolution.** A `Resolver` holds the root `Value`; `resolve("#/components/schemas/Pet")`
splits on `/` and indexes into the document (JSON-Pointer, no external-file refs in M1 — those emit a
warning). A `Ctx` carries a `HashSet<String>` of ref pointers currently on the stack; re-entering a
ref returns a minimal stub (`{}`/`null`) instead of recursing forever — the standard self-referential
schema (a tree node) must terminate.

**GraphQL.** Prefer an introspection JSON (`__schema`, parsed as `Value` like OpenAPI); accept SDL as
a fallback (lightweight source scan, no full parser — the `convert_k6` philosophy). For each field on
`Query`/`Mutation`, build a `GraphqlOptions { query, variables, operation_name }` where `query` is a
generated selection set expanded to a bounded depth (scalars only past the limit, cycle-guarded) and
`variables` are examples for the field's args. Emit as `RequestStep.graphql`.

**gRPC.** Enumerate services/methods and build `RequestStep.grpc = GrpcOptions { proto_files, service,
method, message }` with `message` an example of the input type. Method enumeration reuses loadr's
existing in-process proto compiler (`loadr-protocols`, protox — no `protoc`) rather than a new parser.

**Fuzzing (`fuzz.rs`).** For each operation, `--fuzz` appends variant `RequestStep`s beside the valid
one, each carrying `assert: vec![Condition::Status { matches: Some("^[234]..$"), on_failure, .. }]` —
**any 5xx fails the assertion, which is the gate**. Variant families:
1. **Boundary** — from the schema: `maximum+1`, `minimum-1`, empty string, `maxLength+1` string,
   `[]` for a `minItems:1` array, `null` into non-nullable.
2. **Spec-invalid structural** — drop a `required` key; swap a value's type (string↔int); add an
   unexpected key when `additionalProperties:false`.
3. **Adversarial payload** — replace the JSON body with `loadr_payload::generate_str(kind)` output
   (default `nested-json`, `long-string`) as `Body::Text`, reusing the existing catalog verbatim.

## 5. Reuse map

| Capability | Reuse (exists) | Net-new |
|---|---|---|
| Return type / warnings | `loadr_convert::{Conversion, ConversionWarning}` | — |
| Plan/step/body/assert types | `loadr_config::{TestPlan, Scenario, Step, RequestStep, Body, BodySpec, Condition, GraphqlOptions, GrpcOptions, ExecutorKind, Dur}` | — |
| Contract parsing | `serde_json`/`serde_yaml` → `Value` (as `convert_har`) | JSON-Pointer `$ref` resolver |
| Validation invariant | `loadr_config::load_str(deny_errors)` | — |
| Adversarial bodies | `loadr_payload::{CATALOG, generate_str}` | schema-boundary + structural mutators |
| Response-schema semantics | subset in `plugins/loadr-plugin-openapi-contract` (types/enum/nullable) | invert it: schema → **example** |
| Proto method enumeration | `loadr-protocols` (protox) | gRPC message→example |
| CLI adapter | `commands/convert.rs` pattern | `commands/gen.rs` |

Net-new is essentially one crate: the example engine, the four thin walkers, and the fuzz mutators.

## 6. Testing plan

Mirror `har.rs`'s in-module `#[cfg(test)]` style; unit tests are pure (no network), satisfying the
70% gate. Each generator ships an embedded sample spec (Petstore-lite) as a `const &str`.

- **`example.rs`**: precedence (`example` > `default` > `enum` > type); every JSON type; `format`
  handling; `required`-only objects; `minItems`/`minimum` boundaries.
- **`$ref`**: local ref resolves; self-referential schema terminates (bounded output); missing ref →
  warning not panic.
- **openapi**: path/query/header params land in `url`/`params`/`headers`; path templating →
  `${param}` + seeded `variables`; JSON body populated; one step per operation; `--include`/`--exclude`.
- **fuzz**: boundary variant violates the schema bound; each fuzz step carries the `^[234]..$` gate;
  payload kinds resolve through `loadr_payload`.
- **Invariant test per generator** (the load-bearing one): `serde_yaml::to_string(plan)` →
  `loadr_config::load_str` with `check_files:false, deny_errors:true` succeeds — copied from
  `har.rs::plan_passes_validation`.
- **CLI integration** (`assert_cmd`, matching other commands): `loadr gen openapi fixture.yaml`
  exits 0, stdout parses as a plan, `--fuzz` adds steps.

## 7. Docs / desktop UI / demo

- **Book**: a "Generate from a contract" chapter + per-source Field Cards, beside the existing
  `convert`/`record` docs; document the 5xx gate semantics and `--fuzz-payloads`.
- **Desktop (`desktop/`)**: a "Generate" panel over the bundled CLI (file picker + source dropdown +
  fuzz toggle), sibling to Convert/Record; it shells out to `loadr gen` like the other panels.
- **Demo** (`site/`, vhs tape + Playwright): `loadr gen openapi petstore.yaml -o plan.yaml && loadr run
  plan.yaml`, then a `--fuzz` run surfacing a seeded 5xx. Tape lives in `vhs/`; `site/videos/out/` is
  gitignored and re-recorded before deploy.
- `plugins/index.json` / release trains: unaffected — this is in-binary, not a plugin.

## 8. Milestones

- **M1 — OpenAPI valid-path (smallest shippable).** `loadr gen openapi`: `example.rs` + local `$ref` +
  path/query/header/JSON-body mapping + `servers` base URL + validation invariant test. Ships the
  headline value alone. **~4–5 d.**
- **M2 — `--fuzz` for OpenAPI.** Boundary + structural mutators + `loadr-payload` injection + the
  `^[234]..$` gate; `--fuzz-payloads`. **~3 d.**
- **M3 — Postman.** Collection JSON → steps (folders → groups via `Step::Group`); reuses the example
  engine only for `{{var}}` seeding. **~2 d.**
- **M4 — GraphQL.** Introspection JSON first, SDL fallback; bounded/cycle-guarded selection sets. **~3–4 d.**
- **M5 — gRPC.** Method enumeration via `loadr-protocols`, message→example. **~3–4 d** (highest risk).

### Risks & hard parts

- **Recursion/cycles** in `$ref`, GraphQL selections, and self-referential messages — the single most
  likely source of hangs/OOM. Mitigation: mandatory depth cap + visited-set stub, tested explicitly.
- **`oneOf`/`anyOf`/`discriminator`** have no single "right" example — take `[0]` + warn; don't over-engineer.
- **gRPC** needs a proto front-end; wiring `loadr-protocols`' compiler for *enumeration* (not just
  execution) is unproven and the reason M5 is last and independently deferrable.
- **Scale**: a 500-operation spec yields a huge plan. Default `vus:1/60s` (like `convert_har`) plus a
  warning to set real load; `--include` keeps output tractable.
- **External-file `$ref`** and OpenAPI 3.1 JSON-Schema divergences are out of scope for M1 (warn).
