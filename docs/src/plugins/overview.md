# Plugin system overview

loadr extends through five plugin types over two mechanisms — without
rebuilding the binary and without a JVM.

| Plugin type | Extends | Typical examples |
|---|---|---|
| `protocol` | new request kinds in `flow:` | MQTT, Kafka, Redis, database drivers |
| `output` | metric exporters | proprietary APMs, custom data lakes |
| `extractor` | new `extract:` types | HTML tables, protobuf bodies, JWT claims |
| `assertion` | new condition types | schema validation, image diffing |
| `service` | long-running components | the web UI itself, webhook notifiers |

## Two mechanisms

- **WASM components** (wasmtime, [WIT-defined interface](wasm.md)) — for
  extractors and assertions: portable (one `.wasm` runs on every platform),
  fully sandboxed (no filesystem/network unless granted), written in any
  language with component tooling.
- **Native libraries** ([`abi_stable`](native.md)) — for protocols, outputs
  and services where raw performance or arbitrary system access matters.
  Layout-checked at load time: an ABI-incompatible plugin fails loudly with a
  useful error, not undefined behaviour. Native plugins are normally written
  in Rust.

Native plugins do **not** have to be Rust. A small, frozen
[plain C ABI](c-abi.md) lets you write a `protocol` plugin in C, Go, Zig, or
any language that emits a C shared library — loadr auto-detects which ABI a
library exports at load time, so both kinds coexist transparently.

## Installing & using

```bash
loadr plugin list
loadr plugin install ./uppercase-extractor/   # dir with plugin.toml + artifact
loadr plugin info uppercase-extractor
loadr plugin disable uppercase-extractor
```

Plugins live in `~/.loadr/plugins/<name>/` (override:
`LOADR_PLUGINS_DIR` or `--plugins-dir`), each with a manifest:

```toml
# plugin.toml
[plugin]
name = "uppercase-extractor"
version = "0.1.0"
kind = "extractor"            # protocol | output | extractor | assertion | service
type = "wasm"                 # wasm | native
entry = "uppercase.wasm"
description = "Boundary extractor that upper-cases the match"
```

Reference plugins from a test:

```yaml
plugins:
  - { name: uppercase-extractor, config: { left: "id=", right: ";" } }
  - { name: kafka-protocol, path: ./libkafka_protocol.so }   # explicit path

scenarios:
  s:
    flow:
      - request:
          protocol: kafka-protocol          # protocol plugins by name
          url: kafka://broker:9092/topic
```

Working examples of **every type** ship in
`plugins/examples/` —
start there, then read [Developing a plugin](developing.md).
