# MongoDB plugin

`loadr-plugin-mongo` adds MongoDB as a load-test target. It is a **native
protocol plugin**: MongoDB support is not built into loadr core — the heavy
`mongodb` Rust driver ships only inside this plugin's dynamic library. Once the
plugin is installed, a request to a `mongodb://` (or `mongo://`) URL routes
straight to it.

It is the first plugin built on loadr's runtime protocol-plugin path; the
contract it uses is documented in
[Developing a plugin](developing.md#native-protocol-plugins).

## Build and install

```bash
cargo build -p loadr-plugin-mongo --release

# `loadr plugin install` copies a directory that holds plugin.toml next to the
# artifact named by its `entry`. Stage the built cdylib beside the manifest:
mkdir -p dist
cp plugins/loadr-plugin-mongo/plugin.toml dist/
cp target/release/libloadr_plugin_mongo.so dist/   # .dylib on macOS, .dll on Windows
loadr plugin install dist
loadr plugin info mongo
```

Installing copies `plugin.toml` and the artifact into `~/.loadr/plugins/mongo/`.
The manifest declares the URL schemes the plugin serves:

```toml
[plugin]
name = "mongo"
kind = "protocol"
type = "native"
entry = "libloadr_plugin_mongo.so"
schemes = ["mongodb", "mongo"]
```

## Use it in a test

List the plugin under `plugins:` and target a `mongodb://` URL. The operation
is described by the request's `plugin:` block:

```yaml
plugins:
  - name: mongo            # or: { name: mongo, path: target/release/libloadr_plugin_mongo.so }

scenarios:
  main:
    executor: constant-vus
    vus: 5
    duration: 15s
    flow:
      - request:
          name: insert product
          url: mongodb://user:pass@host:27017/loadr
          plugin:
            operation: insert
            collection: products
            document: { name: "vu-${vu}-item", price: 12.5, stock: 3 }
          assert:
            - { type: status, equals: 1 }      # 1 = ok, 0 = driver error

      - request:
          name: find cheap products
          url: mongodb://user:pass@host:27017/loadr
          plugin:
            operation: find
            collection: products
            filter: { price: { $lt: 50 } }
            limit: 100

      - request:
          name: stock by tag
          url: mongodb://user:pass@host:27017/loadr
          plugin:
            operation: aggregate
            collection: products
            pipeline:
              - { $unwind: "$tags" }
              - { $group: { _id: "$tags", total: { $sum: "$stock" } } }
```

A complete runnable plan is in `examples/28-mongo.yaml`.

## Request options (`plugin:` block)

| Key          | Type      | Used by                         | Notes |
|--------------|-----------|---------------------------------|-------|
| `operation`  | string    | all                             | `insert`, `find`, `update`, `delete`, `aggregate`, `command` |
| `database`   | string    | all (optional)                  | Defaults to the database in the URI path |
| `collection` | string    | all except `command`            | Required |
| `document`   | object    | `insert`                        | Insert one document |
| `documents`  | array     | `insert`                        | Insert many documents |
| `filter`     | object    | `find`, `update`, `delete`      | Defaults to `{}` (match all) |
| `update`     | object    | `update`                        | e.g. `{ "$set": { ... } }` |
| `pipeline`   | array     | `aggregate`                     | Aggregation stages |
| `command`    | object    | `command`                       | Raw database command |
| `limit`      | integer   | `find`                          | Optional |
| `multi`      | bool      | `update`, `delete`              | Operate on many docs (default `false`) |

`${...}` placeholders inside any string leaf are interpolated by loadr before
the plugin runs, so values can reference VU state, variables, and data feeds.

## Metrics

loadr turns the plugin's response into a dedicated metric family named after the
protocol (`mongo`):

| Metric              | Kind    | Meaning |
|---------------------|---------|---------|
| `mongo_reqs`        | counter | One per operation |
| `mongo_req_duration`| trend   | Operation latency (ms) |
| `mongo_docs`        | counter | Documents inserted / matched+modified / deleted / returned |

A request is marked **failed** when the operation errors (response `status` 0).
`http_req_failed` therefore tracks the Mongo failure rate too, and `checks` /
`assert` entries can gate on `status` (1 = ok).

## Connection pooling

The plugin keeps an internal pool of `mongodb::Client` handles keyed by the full
connection URI, shared across every VU. A `Client` is itself an internally
pooled, cheaply-cloned handle, so one per distinct URI is the correct model
under load — the first request for a URI establishes it, and all subsequent
requests (any VU) reuse it. The plugin owns a single Tokio runtime and
`block_on`s the async driver, because the protocol ABI is synchronous and
carries no per-VU context across the FFI boundary.

## Testing against a real server

The example harness brings up `mongo:7` with seed data:

```bash
docker compose -f examples/harness/docker-compose.yml up -d mongo

LOADR_TEST_MONGO_URL=mongodb://loadr:loadr@127.0.0.1:27017/loadr \
  cargo test -p loadr-plugin-mongo
```

The integration tests no-op when `LOADR_TEST_MONGO_URL` is unset.
