# Elasticsearch plugin

`loadr-plugin-elasticsearch` adds Elasticsearch as a load-test target. It is a
**native protocol plugin**: Elasticsearch support is not built into loadr core.
Elasticsearch's API is plain HTTP/JSON, so the plugin talks to it directly over
loadr's own **hyper + hyper-rustls** stack (pure-Rust TLS via `ring` + webpki
roots — no system OpenSSL) rather than dragging in the heavy official
`elasticsearch` crate. That keeps the cdylib light and cross-compilable for
every release target. Once the plugin is installed, a request to an
`elasticsearch://` (or `es://`) URL routes straight to it.

The contract it uses is documented in
[Developing a plugin](developing.md#native-protocol-plugins).

## Build and install

```bash
cargo build -p loadr-plugin-elasticsearch --release

# `loadr plugin install` copies a directory that holds plugin.toml next to the
# artifact named by its `entry`. Stage the built cdylib beside the manifest:
mkdir -p dist
cp plugins/loadr-plugin-elasticsearch/plugin.toml dist/
cp target/release/libloadr_plugin_elasticsearch.so dist/   # .dylib on macOS, .dll on Windows
loadr plugin install dist
loadr plugin info elasticsearch
```

Installing copies `plugin.toml` and the artifact into
`~/.loadr/plugins/elasticsearch/`. The manifest declares the URL schemes the
plugin serves:

```toml
[plugin]
name = "elasticsearch"
kind = "protocol"
type = "native"
entry = "libloadr_plugin_elasticsearch.so"
schemes = ["elasticsearch", "es"]
```

## Use it in a test

List the plugin under `plugins:` and target an `elasticsearch://` URL (both
`elasticsearch://` and `es://` are mapped onto plain `http://` internally; a
`http(s)://` URL is also served). Basic-auth credentials in the URL —
`elasticsearch://user:pass@host:9200` — become an HTTP `Authorization: Basic`
header. The operation is described by the request's `plugin:` block:

```yaml
plugins:
  - name: elasticsearch    # or: { name: elasticsearch, path: target/release/libloadr_plugin_elasticsearch.so }

scenarios:
  main:
    executor: constant-vus
    vus: 5
    duration: 15s
    flow:
      - request:
          name: index product
          url: elasticsearch://host:9200
          plugin:
            operation: index
            index: products
            document: { name: "vu-${vu}-item", price: 12.5, stock: 3 }
          assert:
            - { type: status, equals: 1 }      # 1 = ok, 0 = error

      - request:
          name: bulk index
          url: elasticsearch://host:9200
          plugin:
            operation: bulk
            index: products
            operations:
              - { index: {} }
              - { name: "a", price: 1.0 }
              - { index: {} }
              - { name: "b", price: 2.0 }

      - request:
          name: search cheap products
          url: elasticsearch://host:9200
          plugin:
            operation: search
            index: products
            query: { size: 20, query: { range: { price: { lt: 50 } } } }
```

A complete runnable plan is in `examples/33-elasticsearch.yaml`.

## Request options (`plugin:` block)

| Key          | Type    | Used by  | Notes |
|--------------|---------|----------|-------|
| `operation`  | string  | all      | `index`, `get`, `search`, `bulk` |
| `index`      | string  | all\*    | Target index / alias. Required for index/get/search; optional for bulk |
| `id`         | string  | index/get| Document id. Optional for `index` (server generates one), required for `get` |
| `document`   | object  | `index`  | The document body |
| `query`      | object  | `search` | Elasticsearch query DSL body. Defaults to `match_all` when omitted |
| `operations` | array   | `bulk`   | NDJSON action/source objects — alternating action lines (`{ index: {} }`) and source documents |

`${...}` placeholders inside any string leaf are interpolated by loadr before
the plugin runs, so values can reference VU state, variables, and data feeds.

### Operation → REST mapping

| Operation | HTTP request |
|-----------|--------------|
| `index` (with `id`) | `PUT /{index}/_doc/{id}` |
| `index` (no `id`)   | `POST /{index}/_doc` |
| `get`     | `GET /{index}/_doc/{id}` |
| `search`  | `POST /{index}/_search` |
| `bulk`    | `POST /{index}/_bulk` (or `POST /_bulk`) with `application/x-ndjson` |

## Metrics

loadr turns the plugin's response into a dedicated metric family named after the
protocol (`elasticsearch`):

| Metric                      | Kind    | Meaning |
|-----------------------------|---------|---------|
| `elasticsearch_reqs`        | counter | One per operation |
| `elasticsearch_req_duration`| trend   | Operation latency (ms) |
| `elasticsearch_docs`        | counter | Documents written (index = 1, bulk = items succeeded) |

Search hits are also reported in the response `extras.hits`. A request is marked
**failed** when the operation errors — a non-2xx HTTP status, a transport error,
or a `_bulk` response with per-item errors (response `status` 0).
`http_req_failed` therefore tracks the Elasticsearch failure rate too, and
`checks` / `assert` entries can gate on `status` (1 = ok).

## Connection pooling

The plugin keeps an internal pool of hyper clients keyed by the full request
URL, shared across every VU. A hyper-util legacy `Client` is itself an
internally-pooled, cheaply-cloned handle, so one per distinct base URL is the
correct model under load — the first request for a URL establishes it, and all
subsequent requests (any VU) reuse the pooled connections. The plugin owns a
single Tokio runtime and `block_on`s the async HTTP request, because the
protocol ABI is synchronous and carries no per-VU context across the FFI
boundary.

## Testing against a real server

The example harness brings up `elasticsearch:8.x` as a single node with security
disabled (heap capped at 512 MB so CI doesn't OOM):

```bash
docker compose -f examples/harness/docker-compose.yml up -d elasticsearch

# ES is slow to start; wait for the cluster to report healthy first:
until curl -fsS http://127.0.0.1:9200/_cluster/health; do sleep 2; done

LOADR_TEST_ES_URL=http://127.0.0.1:9200 \
  cargo test -p loadr-plugin-elasticsearch
```

The integration tests no-op when `LOADR_TEST_ES_URL` is unset.
