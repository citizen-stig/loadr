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
