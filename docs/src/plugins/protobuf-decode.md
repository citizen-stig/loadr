# protobuf-decode plugin

> **Status:** planned — this page documents the intended contract; the plugin is
> not yet shipped in the plugin index.

`loadr-plugin-protobuf-decode` adds a **protobuf extractor**. It is a native
`extractor` plugin: given a response body carrying a length-delimited protobuf
message, it decodes that message against a compiled `FileDescriptorSet` and
returns a single field by path. Decoding is pure Rust — it uses
[`prost-reflect`], in line with loadr's *protox-not-protoc* choice, so there is
no `protoc` toolchain, no C dependency, and no code generation: the descriptor
set is loaded at runtime and the message is decoded dynamically.

It exists to pull values out of **binary protobuf responses** the same way the
built-in `jsonpath` extractor pulls them out of JSON: an order `id`, a session
token, a cursor for the next page. The extracted value is stringified and flows
into a `${var}` you can reuse in later requests, exactly like the built-in
extractors.

The plugin implements the native `PluginExtractor` trait; the contract is
documented in [Developing a plugin](developing.md).

[`prost-reflect`]: https://docs.rs/prost-reflect

## Build and install

```bash
cargo build -p loadr-plugin-protobuf-decode --release

# `loadr plugin install` copies a directory that holds plugin.toml next to the
# artifact named by its `entry`. Stage the built cdylib beside the manifest:
mkdir -p dist
cp plugins/loadr-plugin-protobuf-decode/plugin.toml dist/
cp target/release/libloadr_plugin_protobuf_decode.so dist/   # .dylib on macOS, .dll on Windows
loadr plugin install dist
loadr plugin info protobuf-decode
```

Once the plugin is published to the index, the one-liner install is:

```bash
loadr plugin install protobuf-decode
```

Installing copies `plugin.toml` and the artifact into
`~/.loadr/plugins/protobuf-decode/`. The manifest declares it as a native
extractor:

```toml
[plugin]
name = "protobuf-decode"
kind = "extractor"
type = "native"
entry = "libloadr_plugin_protobuf_decode.so"
description = "Decode a protobuf response against a FileDescriptorSet and return a field by path"
```

## Preparing the descriptor set

The plugin decodes messages *dynamically*, so it needs the message schema as a
`FileDescriptorSet` — the same `.pb` blob `protoc --descriptor_set_out` emits.
In keeping with loadr's [`protox`] approach you can produce it in pure Rust
without a `protoc` install:

```bash
# via the protox CLI (pure Rust) …
protox -o api.pb order.proto

# … or the classic protoc, if you already have it
protoc --include_imports --descriptor_set_out=api.pb order.proto
```

Point the plugin's `descriptor` config at the resulting file
(`./api.pb`). Include imports so every transitively referenced type is present.

[`protox`]: https://docs.rs/protox

## Use it in a test

List the plugin under `plugins:` with its `config`, then reference it from an
`extract:` step by `type: plugin`. Plugin extractors are addressed by plugin
**name**; the `config` from the `plugins:` entry is passed to every call, and
the `name` on the `extract:` step is the variable the result is stored into.

```yaml
plugins:
  - name: protobuf-decode
    config:
      descriptor: ./api.pb     # compiled FileDescriptorSet
      message: Order           # message type to decode the body as
      field: id                # field path to return

scenarios:
  orders:
    executor: constant-vus
    vus: 5
    duration: 1m
    flow:
      - request:
          name: create order
          method: POST
          url: /v1/orders
          headers:
            Accept: application/x-protobuf
          body:
            json: { sku: "widget-1", qty: 2 }
          extract:
            # Decode the protobuf Order and pull out its `id` field into ${order_id}
            - { type: plugin, name: order_id, plugin: protobuf-decode }
      - request:
          name: fetch order
          url: /v1/orders/${order_id}
          assert:
            - { type: status, equals: 200 }
```

The `config` is JSON-shaped and handed to the plugin as-is, e.g.
`{"descriptor":"./api.pb","message":"Order","field":"id"}`.

## Config reference

| Key          | Type   | Required | Meaning |
|--------------|--------|----------|---------|
| `descriptor` | string | yes      | Path to a compiled `FileDescriptorSet` (`.pb`). Loaded once and cached; a relative path is resolved from the working directory the run was launched in. |
| `message`    | string | yes      | The message type to decode the body as. Give either the short name (`Order`) or its fully-qualified name (`api.v1.Order`) when the short name is ambiguous. |
| `field`      | string | yes      | Field path to return. A dotted path walks into nested messages (e.g. `payment.card.last4`); a numeric segment indexes a repeated field (e.g. `items.0.sku`). |

The body is read as a **length-delimited** protobuf message (the `varint` length
prefix followed by the encoded bytes, as written by `write_length_delimited` /
Go's `EncodeDelimited`). The selected field is stringified for the `${var}`:
scalars use their natural text form (numbers, `true`/`false`, `enum` value
names), `bytes` are base64, and a message- or `repeated`-typed leaf is rendered
as JSON.

If the descriptor can't be loaded, the `message` type isn't found, or the body
isn't a valid protobuf message, the plugin surfaces a configuration/decoding
error on first call. If the `field` path simply doesn't resolve to a set value,
the extraction yields **no value** — the same as any other extractor that fails
to match.

## Metrics

**n/a.** Extractor plugins run inside an existing request's lifecycle and do not
emit their own metric family; the surrounding HTTP request is still measured by
the core `http_*` metrics.

## Notes

- **Why prost-reflect.** Decoding is done dynamically against the descriptor set
  rather than from generated Rust structs, so one plugin handles any schema
  without a rebuild. This mirrors loadr's core stance of using [`protox`] /
  `prost-reflect` (pure Rust) instead of shelling out to `protoc`.
- **Length-delimited vs. raw.** gRPC and Twirp-protobuf responses frame each
  message with a length prefix; that is what this plugin expects. A body that is
  a single *raw* (non-delimited) message won't decode — re-frame it or strip the
  gRPC 5-byte frame header upstream.
- **Relationship to Twirp/gRPC over HTTP.** For **JSON**-mode Twirp or any
  JSON API, reach for the built-in `type: jsonpath` extractor instead
  (`examples/43-twirp.yaml`) — no descriptor needed. This plugin is for the
  **binary** protobuf path where there is no JSON to select against.
- **Fixed per entry.** The plugin's `config` is fixed per `plugins:` entry. To
  extract several different fields (or from several message types) in one plan,
  list the plugin more than once under distinct names, each with its own
  `descriptor` / `message` / `field`.
