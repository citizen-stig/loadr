# WASM plugins

WASM plugins are [component-model](https://component-model.bytecodealliance.org/)
components against the WIT world in
[`crates/loadr-plugin-api/wit/loadr.wit`](https://github.com/reaandrew/loadr.io/blob/main/crates/loadr-plugin-api/wit/loadr.wit).
The host runs them in wasmtime with **no filesystem and no network** — a
malicious or buggy extractor can waste CPU, nothing else.

The interface (abridged):

```wit
package loadr:plugin;

interface meta {
  record info { name: string, version: string, kind: string, description: string }
  describe: func() -> info;
}

interface extractor {
  /// body + headers + the plugin's JSON config -> extracted value (or none)
  extract: func(body: list<u8>, headers: list<tuple<string,string>>, config: string) -> option<string>;
}

interface assertion {
  record verdict { pass: bool, detail: string }
  check: func(status: s64, body: list<u8>, headers: list<tuple<string,string>>,
              duration-ms: f64, config: string) -> verdict;
}
```

## Writing one in Rust

```bash
cargo new --lib my-extractor && cd my-extractor
rustup target add wasm32-wasip2
```

```toml
# Cargo.toml
[lib]
crate-type = ["cdylib"]
[dependencies]
wit-bindgen = "0.58"
```

```rust
wit_bindgen::generate!({ path: "wit", world: "loadr-plugin" });

struct Plugin;

impl exports::loadr::plugin::meta::Guest for Plugin {
    fn describe() -> exports::loadr::plugin::meta::Info { /* ... */ }
}

impl exports::loadr::plugin::extractor::Guest for Plugin {
    fn extract(body: Vec<u8>, _headers: Vec<(String, String)>, config: String) -> Option<String> {
        let cfg: serde_json::Value = serde_json::from_str(&config).ok()?;
        // ... your logic ...
    }
}

export!(Plugin);
```

```bash
cargo build --release --target wasm32-wasip2
# target/wasm32-wasip2/release/my_extractor.wasm is the component
```

Package it with a `plugin.toml` (`type = "wasm"`) and `loadr plugin install`.
Any language with component tooling (Go via TinyGo, Python via componentize-py,
JS via jco) works the same way.

## Using it

```yaml
plugins: [ { name: my-extractor, config: { left: "id=", right: ";" } } ]
scenarios:
  s:
    flow:
      - request:
          url: /page
          extract:
            - { type: plugin, name: order_id, plugin: my-extractor }
```

(Plugin extractors/assertions are addressed by plugin name; their `config`
from the `plugins:` entry is passed to every call.)
