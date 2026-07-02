# json-schema plugin

> **Status:** planned — this page documents the intended contract; the plugin is
> not yet shipped in the plugin index.

`loadr-plugin-json-schema` adds a **JSON Schema assertion**. It is a native
`assertion` plugin: it compiles a [JSON Schema] document **once** (at first use)
and then validates every response body against it, failing the check with the
**first** validation error — including the JSON path that violated the schema.
Both **Draft 7** and **Draft 2020-12** are supported. Validation is pure Rust
(the [`jsonschema`] crate), so there is no external validator process and no C
dependency; the compiled schema is reused across VUs and requests.

It exists to gate a response on its *shape* rather than a single field: that an
order object carries every required property, that `total` is a number, that
`status` is one of a fixed enum, that no additional properties leaked in. When
the body conforms the check passes; when it does not, the request is marked
failed and the error names the offending instance path.

The plugin implements the native `PluginAssertion` trait; the contract is
documented in [Developing a plugin](developing.md).

[JSON Schema]: https://json-schema.org/
[`jsonschema`]: https://docs.rs/jsonschema

## Build and install

```bash
cargo build -p loadr-plugin-json-schema --release

# `loadr plugin install` copies a directory that holds plugin.toml next to the
# artifact named by its `entry`. Stage the built cdylib beside the manifest:
mkdir -p dist
cp plugins/loadr-plugin-json-schema/plugin.toml dist/
cp target/release/libloadr_plugin_json_schema.so dist/   # .dylib on macOS, .dll on Windows
loadr plugin install dist
loadr plugin info json-schema
```

Installing copies `plugin.toml` and the artifact into
`~/.loadr/plugins/json-schema/`. The manifest declares it as a native assertion:

```toml
[plugin]
name = "json-schema"
kind = "assertion"
type = "native"
entry = "libloadr_plugin_json_schema.so"
description = "Validate a response body against a JSON Schema (Draft 7 / 2020-12)"
```

## Use it in a test

List the plugin under `plugins:` with its `config`, then reference it from an
`assert:` step by `type: plugin`. Plugin assertions are addressed by plugin
**name**; the `config` from the `plugins:` entry is passed to every call.

```yaml
plugins:
  - name: json-schema
    config: { schema: "./order.schema.json" }

scenarios:
  orders:
    executor: constant-vus
    vus: 10
    duration: 30s
    flow:
      - request:
          name: fetch order
          url: /orders/${order_id}
          assert:
            - { type: status, equals: 200 }
            # Fail the request unless the body matches order.schema.json
            - { type: plugin, name: order matches schema, plugin: json-schema }
```

The `config` is JSON-shaped and handed to the plugin as-is, e.g.
`{"schema":"./order.schema.json"}`.

## Config reference

| Key      | Type   | Required | Meaning |
|----------|--------|----------|---------|
| `schema` | string | yes\*    | Path to a JSON Schema document, resolved relative to the plan file. Compiled once, then reused for every validation. |
| `inline` | object | yes\*    | An inline JSON Schema, given directly in the plan instead of a file path. |
| `draft`  | string | no       | Force the dialect: `"7"` or `"2020-12"`. When omitted the draft is inferred from the schema's `$schema` keyword, defaulting to Draft 2020-12. |

\* Provide exactly one of `schema` or `inline`. A missing/invalid schema, or an
unparseable schema document, is a **configuration error** surfaced when the
plugin is first called — the run stops rather than silently passing.

```yaml
# Inline schema instead of a file:
plugins:
  - name: json-schema
    config:
      inline:
        type: object
        required: [id, status, total]
        properties:
          status: { enum: [pending, paid, shipped] }
          total:  { type: number }
        additionalProperties: false
```

The check **passes** when the response body is valid JSON that conforms to the
schema. It **fails** when the body is not valid JSON, or when validation reports
one or more errors — the check message carries the first error and its instance
path (e.g. `/items/0/price: "12.00" is not of type "number"`), so a failing run
tells you exactly which field broke.

## Metrics

**n/a.** Assertion plugins run inside an existing request's lifecycle and do not
emit their own metric family. A failed validation marks the surrounding request
as failed, so it flows into the standard `checks` rate and `http_req_failed`
just like any built-in `assert:` entry — gate on those with `thresholds:`.

```yaml
thresholds:
  checks: [ "rate>0.99" ]
```

## Notes

- **Compiled once.** The schema is parsed and compiled on first use and cached
  for the lifetime of the run, so per-response validation is just a tree walk —
  the cost of compilation is paid a single time, not per request.
- **First error wins.** Validation stops at and reports the first violation with
  its JSON path; it is not an exhaustive list of every problem in the body.
  Narrow the schema (or the request) if you need to isolate a specific field.
- **JSON only.** The body must parse as JSON. For non-JSON bodies use the
  built-in `type: body_contains` / `type: body_matches` assertions, or an
  extractor plus a scalar check.
- **Relationship to built-in checks.** loadr core already ships field-level
  assertions (`type: jsonpath`, `body_contains`, `status`). This plugin
  complements them by validating the **whole document shape** in one step —
  reach for it when a per-field check list would be long or brittle, or as a
  worked reference for the `PluginAssertion` trait.
- **Fixed per entry.** The `config` (and therefore the schema) is fixed per
  `plugins:` entry. To validate different endpoints against different schemas in
  one plan, list the plugin more than once under distinct names, each with its
  own `schema`.
</content>
</invoke>
