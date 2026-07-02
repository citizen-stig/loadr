# openapi-contract plugin

> **Status:** planned — this page documents the intended contract; the plugin is
> not yet shipped in the plugin index.

`loadr-plugin-openapi-contract` adds an **OpenAPI contract assertion**. It is a
native `assertion` plugin: it loads an [OpenAPI 3] document **once** (at first
use), resolves the operation being tested by its **method + path** (or
`operationId`), and validates each response — status code, headers, and body —
against the schema the spec declares for that operation. Validation is pure Rust
(the [`openapiv3`] crate parses the document, [`jsonschema`] validates the body),
so there is no external validator process and no C dependency; the parsed spec
and the compiled response schemas are reused across VUs and requests.

It exists to gate a response on its *contract* rather than a single field: that
the endpoint returned a status the spec actually documents, that the declared
response headers are present, and that the body matches the schema for that
status code. When the response conforms the check passes; when it does not, the
request is marked failed and the error names what broke — an undocumented status,
a missing header, or the JSON path that violated the body schema.

The plugin implements the native `PluginAssertion` trait; the contract is
documented in [Developing a plugin](developing.md).

[OpenAPI 3]: https://spec.openapis.org/oas/v3.1.0.html
[`openapiv3`]: https://docs.rs/openapiv3
[`jsonschema`]: https://docs.rs/jsonschema

## Build and install

```bash
cargo build -p loadr-plugin-openapi-contract --release

# `loadr plugin install` copies a directory that holds plugin.toml next to the
# artifact named by its `entry`. Stage the built cdylib beside the manifest:
mkdir -p dist
cp plugins/loadr-plugin-openapi-contract/plugin.toml dist/
cp target/release/libloadr_plugin_openapi_contract.so dist/   # .dylib on macOS, .dll on Windows
loadr plugin install dist
loadr plugin info openapi-contract
```

Installing copies `plugin.toml` and the artifact into
`~/.loadr/plugins/openapi-contract/`. The manifest declares it as a native
assertion:

```toml
[plugin]
name = "openapi-contract"
kind = "assertion"
type = "native"
entry = "libloadr_plugin_openapi_contract.so"
description = "Validate a response against an OpenAPI 3 operation (status/headers/body)"
```

## Use it in a test

List the plugin under `plugins:` with its `config`, then reference it from an
`assert:` step by `type: plugin`. Plugin assertions are addressed by plugin
**name**; the `config` from the `plugins:` entry is passed to every call.

```yaml
plugins:
  - name: openapi-contract
    config: { spec: "./openapi.yaml", operation: "getOrder" }

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
            # Fail the request unless the response matches the getOrder contract
            - { type: plugin, name: order matches contract, plugin: openapi-contract }
```

The `config` is JSON-shaped and handed to the plugin as-is, e.g.
`{"spec":"./openapi.yaml","operation":"getOrder"}`.

## Config reference

| Key         | Type   | Required | Meaning |
|-------------|--------|----------|---------|
| `spec`      | string | yes      | Path to an OpenAPI 3 document (`.yaml` or `.json`), resolved relative to the plan file. Parsed once, then reused for every validation. |
| `operation` | string | yes\*    | The `operationId` to validate against. The plugin looks it up in the spec and resolves its method + path. |
| `method`    | string | yes\*    | HTTP method (e.g. `GET`, `POST`) — used with `path` to select the operation when you address it by route rather than `operationId`. |
| `path`      | string | yes\*    | Templated path from the spec (e.g. `/orders/{id}`) — paired with `method`. |
| `validate`  | array  | no       | Which parts to check: any of `status`, `headers`, `body`. Defaults to all three. |

\* Identify the operation **either** by `operation` (an `operationId`) **or** by
the `method` + `path` pair. An unknown `operationId`, a `method`/`path` that the
spec does not define, or an unparseable spec document is a **configuration
error** surfaced when the plugin is first called — the run stops rather than
silently passing.

```yaml
# Address the operation by method + path instead of operationId:
plugins:
  - name: openapi-contract
    config:
      spec: "./openapi.yaml"
      method: GET
      path: /orders/{id}
      validate: [status, body]   # skip header validation
```

The check **passes** when the response status is one the operation documents and
the body (and, unless skipped, the declared headers) conform to that status's
schema. It **fails** when the status is not in the operation's `responses`, when
a required response header is missing, or when the body does not match the
schema — the check message carries the specific reason and, for body errors, the
first violation and its instance path (e.g.
`/items/0/price: "12.00" is not of type "number"`), so a failing run tells you
exactly which part of the contract broke.

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

- **Parsed once.** The spec is read and parsed, and each operation's response
  schemas compiled, on first use and cached for the lifetime of the run — so
  per-response validation is just a schema walk; the parse cost is paid a single
  time, not per request.
- **Status selects the schema.** The plugin validates the body against the
  schema declared for the **actual** response status (falling back to the
  `default` response when the spec provides one). A status the operation does
  not document is itself a contract failure, before any body check runs.
- **`$ref` resolution.** Local `$ref`s into `components/schemas` are resolved
  when the spec is loaded, so shared component schemas validate the same way
  they read in the document. External-file `$ref`s are not fetched.
- **First body error wins.** Body validation stops at and reports the first
  violation with its JSON path; it is not an exhaustive list of every problem in
  the body. It applies to JSON response bodies — non-JSON media types are
  validated at the status/header level only.
- **Relationship to built-in checks.** loadr core already ships field-level
  assertions (`type: jsonpath`, `body_contains`, `status`) and the
  [json-schema](json-schema.md) plugin validates a body against a standalone
  schema. This plugin goes further by validating the **whole response against
  the API's own contract** — reach for it to catch drift between a service and
  the OpenAPI document it publishes.
- **Fixed per entry.** The `config` (and therefore the operation) is fixed per
  `plugins:` entry. To validate several endpoints against their own operations
  in one plan, list the plugin more than once under distinct names, each with
  its own `operation` (or `method` + `path`).
