# Developing a plugin

A practical walkthrough — we'll build, test and ship the `uppercase-extractor`
WASM plugin (the same one in `plugins/examples/wasm-extractor`).

## 1. Scaffold

```bash
cargo new --lib uppercase-extractor && cd uppercase-extractor
mkdir wit && cp <loadr repo>/crates/loadr-plugin-api/wit/loadr.wit wit/
```

```toml
[package]
name = "uppercase-extractor"
version = "0.1.0"
edition = "2021"

[lib]
crate-type = ["cdylib"]

[dependencies]
wit-bindgen = "0.58"
serde_json = "1"
```

## 2. Implement

```rust
wit_bindgen::generate!({ path: "wit", world: "loadr-plugin" });

struct Plugin;

impl exports::loadr::plugin::meta::Guest for Plugin {
    fn describe() -> exports::loadr::plugin::meta::Info {
        exports::loadr::plugin::meta::Info {
            name: "uppercase-extractor".into(),
            version: env!("CARGO_PKG_VERSION").into(),
            kind: "extractor".into(),
            description: "boundary extractor that upper-cases the match".into(),
        }
    }
}

impl exports::loadr::plugin::extractor::Guest for Plugin {
    fn extract(body: Vec<u8>, _headers: Vec<(String, String)>, config: String) -> Option<String> {
        let cfg: serde_json::Value = serde_json::from_str(&config).ok()?;
        let (left, right) = (cfg["left"].as_str()?, cfg["right"].as_str()?);
        let text = String::from_utf8_lossy(&body);
        let start = text.find(left)? + left.len();
        let end = text[start..].find(right)? + start;
        Some(text[start..end].to_uppercase())
    }
}

export!(Plugin);
```

## 3. Build & package

```bash
rustup target add wasm32-wasip2
cargo build --release --target wasm32-wasip2

mkdir dist
cp target/wasm32-wasip2/release/uppercase_extractor.wasm dist/
cat > dist/plugin.toml <<'EOF'
[plugin]
name = "uppercase-extractor"
version = "0.1.0"
kind = "extractor"
type = "wasm"
entry = "uppercase_extractor.wasm"
description = "Boundary extractor that upper-cases the match"
EOF
```

## 4. Install & use

```bash
loadr plugin install ./dist
loadr plugin info uppercase-extractor
```

```yaml
plugins: [ { name: uppercase-extractor, config: { left: "token=", right: ";" } } ]
```

## 5. Publish to the index

A locally-installed directory is enough for development, but to make your
plugin installable by name (`loadr plugin install <name>`) it has to appear in
the **plugin index** — the catalogue described in
[Installing plugins](installing.md).

For each supported host target, package the `plugin.toml` plus the built
dynamic library into an archive (`.tar.gz` on Linux/macOS, `.zip` on Windows),
name it `<name>-<target>.<ext>`, and add an entry to `plugins/index.json`:

```json
{
  "schema": 1,
  "plugins": {
    "myproto": {
      "kind": "protocol",
      "description": "…",
      "latest": "0.1.0",
      "versions": {
        "0.1.0": {
          "min_loadr_abi": "1.0",
          "artifacts": {
            "x86_64-unknown-linux-gnu": {
              "url": "https://…/myproto-x86_64-unknown-linux-gnu.tar.gz",
              "sha256": "<sha256 of the archive>",
              "entry": "libloadr_plugin_myproto.so"
            }
          }
        }
      }
    }
  }
}
```

The release CI fills in the real `url`/`sha256` per target; bump
`min_loadr_abi` to the host ABI your build requires (the
`LOADR_PLUGIN_ABI_VERSION` you compiled against). The `entry` is the
per-platform artifact filename (`libloadr_plugin_<name>.so` /
`.dylib` / `loadr_plugin_<name>.dll`) and must match the `entry` inside the
archive's `plugin.toml`.

Until the index goes live you can hand a tester an archive directly:

```bash
loadr plugin install ./myproto-x86_64-unknown-linux-gnu.tar.gz --allow-untrusted
```

## Testing tips

- Drive the component directly in a Rust test with
  `loadr_plugin_api::WasmExtractor::load(path)` — exactly what loadr's own
  test suite does for the examples.
- For native plugins: build with `cargo build`, then
  `NativePlugin::load("target/debug/libmy_plugin.so")` in a test.
- Keep configs JSON-serializable and document them in your README; loadr
  passes the `config:` value through verbatim.

## Versioning rules

- WASM: the WIT package version (`loadr:plugin@0.1.0`) is the contract.
- Native: `abi_stable` layout checking is the contract; additionally the
  root module carries `abi_version` — bump on breaking changes and loadr
  will refuse mismatches with a clean message.

## Native protocol plugins

A **protocol plugin** adds a new load-test target (a database, a queue, a
bespoke wire protocol). It must be a *native* plugin — WASM plugins can only be
extractors/assertions. `loadr-plugin-mongo` is the reference implementation; see
[the MongoDB plugin](mongo.md) for an end-to-end example.

### The ABI

A protocol plugin implements the synchronous `FfiProtocol` trait and exports it
via `make_protocol`:

```rust
use loadr_plugin_api::abi::{FfiProtocol, FfiProtocolBox, FfiProtocol_TO, PluginMod, LOADR_PLUGIN_ABI_VERSION};
use loadr_plugin_api::{FfiRequest, FfiResponse};
use abi_stable::std_types::{RString, ROption::{RNone, RSome}};

struct MyProto;

impl FfiProtocol for MyProto {
    fn name(&self) -> RString { RString::from("myproto") }
    fn execute(&self, request_json: RString) -> RString {
        // parse FfiRequest JSON, run the op, return FfiResponse JSON.
        // MUST NOT panic — report failures via the response `error` field.
    }
}

extern "C" fn make_protocol() -> FfiProtocolBox {
    FfiProtocol_TO::from_value(MyProto, abi_stable::erased_types::TD_Opaque)
}

extern "C" fn plugin_info() -> RString { /* PluginInfo JSON, incl. "schemes" */ }

loadr_plugin_api::export_loadr_plugin! {
    PluginMod {
        abi_version: LOADR_PLUGIN_ABI_VERSION,
        info: plugin_info,
        make_output: RNone,
        make_protocol: RSome(make_protocol),
        make_service: RNone,
    }
}
```

Key facts that shape the design:

- `execute` is **synchronous**, takes `&self`, and runs on **one shared
  instance** (`Send + Sync`) created once via `make_protocol()`. There is no
  per-VU context across the FFI boundary.
- A plugin that drives an async client (most do) must therefore **own its async
  machinery**: create its own Tokio runtime inside the cdylib and `block_on`,
  and keep an **internal connection pool** keyed by the connection target
  (e.g. `OnceCell<Mutex<HashMap<String, Client>>>`), reused across every call
  and VU. Do not connect per request.
- Build the crate as `crate-type = ["cdylib"]`, `publish = false`, a member of
  the workspace under `plugins/`.

### Request / response JSON

The host serializes a `loadr_plugin_api::FfiRequest` to JSON and hands it to
`execute`; the plugin returns a `FfiResponse` as JSON:

```jsonc
// FfiRequest (host -> plugin)
{
  "name": "find users",          // metric `name` tag
  "method": "POST",
  "url": "mongodb://h:27017/db",  // the connection target / URL
  "headers": [["k", "v"]],
  "body_b64": "",                 // base64 request body
  "timeout_ms": 30000,
  "options": { ... },             // the request's `plugin:` block, ${...}-interpolated
  "config": { ... }               // merged plugin config (manifest [config] + PluginRef.config)
}

// FfiResponse (plugin -> host)
{
  "status": 1,                    // your convention; non-failed by default
  "status_text": "OK",
  "headers": [],
  "body_b64": "",
  "duration_ms": 1.7,
  "error": null,                  // Some(msg) => request is marked failed
  "extras": { "docs": 3 }         // free-form; the host can read fields out (see below)
}
```

The host already interpolates `${...}` in the request's `plugin:` block before
the plugin sees it, so `options` arrives fully rendered.

### Declaring the URL scheme(s) — routing contract

A runtime-loaded plugin cannot edit core, so it **declares the URL scheme(s) it
serves** and the host wires up routing automatically. Declare schemes in two
places (the manifest wins; `info()` is the fallback when a plugin is loaded by
bare path):

```toml
# plugin.toml
[plugin]
name = "myproto"
kind = "protocol"
type = "native"
entry = "libmyproto.so"
schemes = ["myproto", "myp"]      # URL schemes this plugin claims
```

```rust
// plugin_info() JSON
{ "name": "myproto", "kind": "protocol", "schemes": ["myproto", "myp"], ... }
```

When the host loads the plugin it registers those schemes with a process-global
scheme router (`loadr_core::protocol::register_plugin_schemes`). After that,
`ProtocolRegistry::infer` resolves a URL like `myproto://host/...` to the
handler whose `name()` is `myproto`. **Built-in schemes always win** over plugin
aliases, and an explicit `protocol: myproto` in YAML also resolves (it must
match the plugin handler's `name()`, which the validator accepts because it is
listed under `plugins:`).

So a test can target the plugin either way:

```yaml
plugins: [ { name: myproto } ]
flow:
  - request: { url: "myproto://host/...", plugin: { ... } }   # routed by scheme
  - request: { url: "host/...", protocol: myproto, plugin: { ... } }  # routed by name
```

### Metrics

The host derives a metric **family** from the handler `name()` for plugin
protocols, emitting `<name>_reqs` (counter), `<name>_req_duration` (trend), and
— when the response includes `extras.docs` — `<name>_docs` (counter). A response
with a non-null `error` increments `http_req_failed`. So `loadr-plugin-mongo`
(name `mongo`) produces `mongo_reqs` / `mongo_req_duration` / `mongo_docs`
without any core changes per plugin.

### Testing

- Unit-test the `execute`/`handle` logic by building `FfiRequest` JSON and
  asserting on the `FfiResponse` — no host needed.
- Integration-test against a real backend behind an env-var gate (e.g.
  `LOADR_TEST_MONGO_URL`) so CI skips it when the service is absent; bring the
  service up via `examples/harness/docker-compose.yml`.
- End-to-end, load the built artifact with
  `loadr_plugin_api::NativePlugin::load("target/debug/libmyproto.so")`.
