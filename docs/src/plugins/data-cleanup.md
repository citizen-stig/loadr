# Data cleanup plugin

> **Status:** planned — not yet in the signed [plugin index](installing.md). This
> page documents the intended shape; the config keys and metric names may still
> change before the first release.

`loadr-plugin-data-cleanup` is a **service plugin** (`kind = "service"`, role:
*fixtures & lifecycle*). It solves a problem every test against a **shared,
long-lived environment** eventually hits: the run **creates data** — orders,
users, uploads, tenants — and, unless something tears it down, that data
**leaks**. After a few soak runs the staging database is full of `loadtest-*`
rows and the next run's assertions start tripping over them.

The plugin keeps a **registry of the resources a run created**. VUs **push the
ID or URL of each resource they create** — via a `js:` hook — as they go, and the
service records them. When the run ends, `stop()` walks that registry and
**issues one cleanup call per resource**: an HTTP `DELETE` against the resource
URL (the `http-delete` strategy) or a parameterised SQL `DELETE` (the `sql`
strategy). The environment is returned to the state it was in before the run,
without a hand-written teardown script.

It is **pure Rust, HTTP over [hyper](https://github.com/hyperium/hyper)** —
loadr's own HTTP stack — so there is no extra HTTP client and no C dependency.
The `sql` strategy reuses loadr's bundled database driver for the target engine.

The service lifecycle it uses is the native `ServicePlugin` contract
(`start(config_json) → … → stop()`) documented in
[Native plugins](native.md#the-interface): `start()` validates the strategy and
opens the registry, the running fleet appends created IDs to it, and `stop()`
drains the registry with cleanup calls once every VU has retired.

## Install

Once published, `data-cleanup` will resolve from the signed
[plugin index](installing.md) by name — no build toolchain required:

```bash
loadr plugin install data-cleanup
loadr plugin info data-cleanup
```

This picks the artifact for your host target, checks it against the plugin ABI
your `loadr` build provides, verifies its sha256 and unpacks it into your plugins
directory (`~/.loadr/plugins/data-cleanup/`, or `$LOADR_PLUGINS_DIR`).

Until then you can build and stage it from source like any native plugin:

```bash
cargo build -p loadr-plugin-data-cleanup --release

mkdir -p dist
cp plugins/loadr-plugin-data-cleanup/plugin.toml dist/
cp target/release/libloadr_plugin_data_cleanup.so dist/   # .dylib on macOS, .dll on Windows
loadr plugin install dist
```

Installing copies `plugin.toml` and the artifact into
`~/.loadr/plugins/data-cleanup/`. The manifest declares a native service plugin:

```toml
[plugin]
name = "data-cleanup"
version = "0.1.0"
kind = "service"
type = "native"
entry = "libloadr_plugin_data_cleanup.so"
description = "Tracks resources a run creates and deletes them at run end"
```

## Use it in a test

List the plugin under `plugins:`, then declare it under the plan's `services:`
block. `type: plugin` routes the step to a service plugin and `service:` names it;
the `config:` block carries the cleanup `strategy` and its target. The service
starts once at the beginning of the run and exposes a `track` binding to JS as
`session.services.<name>.track(...)` — call it from a hook whenever a request
creates something you will want to delete:

```yaml
plugins:
  - name: data-cleanup            # or: { name: data-cleanup, path: target/release/libloadr_plugin_data_cleanup.so }

services:
  cleanup:
    type: plugin
    service: data-cleanup
    config:
      strategy: http-delete
      base: https://api.staging.example.com/v1/orders   # DELETE <base>/<id>
      headers:
        Authorization: "Bearer ${env.API_TOKEN}"        # sent on every delete

defaults:
  http:
    base_url: https://api.staging.example.com

js:
  file: scripts/track-created.js

scenarios:
  create_orders:
    executor: constant-vus
    vus: 50
    duration: 5m
    exec: create_order

thresholds:
  http_req_failed:             [ "rate<0.01" ]
  cleanup_errors:              [ "count==0" ]   # every created resource was removed
```

The `js:` hook extracts the new resource's id from each create response and
registers it with the service:

```javascript
// scripts/track-created.js
export function create_order(session) {
  const res = session.request("POST", "/orders", {
    body: JSON.stringify({ sku: "widget", qty: 1 }),
    headers: { "Content-Type": "application/json" },
  });
  if (res.status === 201) {
    // push the created id (or full URL) into the cleanup registry
    session.services.cleanup.track(res.json().id);
  }
}
```

Every id the fleet pushes is buffered in the service. Nothing is deleted during
the run — the hot path only appends to an in-memory registry, so tracking adds no
per-request network cost. When the last VU retires and `stop()` runs, the service
walks the registry and fires a `DELETE https://api.staging.example.com/v1/orders/<id>`
for each entry, then reports how many it removed.

You can also register a **full URL** instead of a bare id — useful when the
create response returns a `Location` header:

```javascript
session.services.cleanup.track(res.headers["location"]);  // absolute URL, deleted as-is
```

### SQL strategy

To clean rows directly in the database instead of via the API, switch the
strategy to `sql`. The registry then holds primary-key values, and `stop()` runs
one parameterised `DELETE` per id (never string-interpolated — the id is bound as
a query parameter):

```yaml
services:
  cleanup:
    type: plugin
    service: data-cleanup
    config:
      strategy: sql
      url: postgres://loadtest@db.staging.example.com/app   # ${env.…} in practice
      table: orders
      key: id                       # DELETE FROM orders WHERE id = $1
```

## Config reference

Config is the JSON object under the service's `config:` key. It is handed to the
service's `start()` verbatim at run start.

| Key         | Type     | Default        | Meaning |
|-------------|----------|----------------|---------|
| `strategy`  | string   | *(required)*   | Cleanup mechanism: `http-delete` (issue HTTP `DELETE`s) or `sql` (issue SQL `DELETE`s). An unknown value fails `start()` so the plan is rejected before the run rather than mid-load. |
| `base`      | string   | *(required for `http-delete`)* | Base URL a tracked id is appended to: a bare id `42` is deleted as `DELETE <base>/42`. A tracked value that is itself an absolute `http(s)://…` URL is deleted as-is, ignoring `base`. |
| `headers`   | map of string→string | `{}` | Headers attached to every `http-delete` request — typically `Authorization`. Pull tokens from `${env.…}` / `${secrets.…}`. (`http-delete` only.) |
| `url`       | string   | *(required for `sql`)* | Database connection URL (`postgres://…`, `mysql://…`). Pull credentials from `${env.…}` / `${secrets.…}`. (`sql` only.) |
| `table`     | string   | *(required for `sql`)* | Table a tracked key is deleted from. (`sql` only.) |
| `key`       | string   | `id`           | Primary-key column matched in the `DELETE … WHERE <key> = $1`. (`sql` only.) |
| `concurrency` | integer | `8`           | How many cleanup calls run in parallel from `stop()`. Higher drains a large registry faster; lower is gentler on the target. |
| `continue_on_error` | bool | `true`   | Keep deleting the remaining resources when one delete fails (a `404`/`5xx` or SQL error). `false` stops at the first failure. Either way, failures are counted in `cleanup_errors`. |

`${env.…}` / `${secrets.…}` and other interpolation resolve before the config
reaches the plugin, so credentials stay out of the plan file.

## Metrics

The plugin reports the outcome of the teardown back into the run, so a leak is
visible in loadr's summary rather than discovered later in the shared database:

| Metric                       | Kind    | Meaning |
|------------------------------|---------|---------|
| `cleanup_resources_deleted`  | counter | One per resource successfully removed (HTTP `DELETE` returning `2xx`/`404`, or a SQL `DELETE` affecting the row). |
| `cleanup_errors`             | counter | One per resource that could **not** be removed: a `DELETE` that failed, timed out, or returned an unexpected non-`2xx`/`404` status, or a SQL error. |

A clean run shows `cleanup_resources_deleted` equal to the number of resources
tracked and `cleanup_errors` at zero. A non-zero `cleanup_errors` means data was
left behind — gate on it so a failed teardown fails the run instead of silently
polluting the environment:

```yaml
thresholds:
  cleanup_errors:             [ "count==0" ]
  cleanup_resources_deleted:  [ "count>0" ]
```

A `404` is treated as **success**, not an error: if the resource is already gone
(the test itself deleted it, or a previous cleanup removed it) the goal — that it
no longer exists — is met.

## Notes

- **Track from a hook, delete at the end.** VUs only *append* ids to the registry
  during the run (`session.services.<name>.track(...)`); the actual `DELETE`s all
  happen in `stop()`, after the last VU retires. Tracking is a cheap in-memory
  push, so it adds no network round-trip to the hot path.
- **Track what you create, when you create it.** Only ids the hook pushes are
  cleaned up. Guard the `track` call behind a success check (`res.status === 201`)
  so you never register a resource that was not actually created.
- **Ids or URLs.** A bare id is appended to `base` (`http-delete`) or bound to the
  `key` column (`sql`); an absolute URL is deleted as-is. Registering the response
  `Location` header is the simplest correlation when the API returns one.
- **Best-effort teardown, surfaced as metrics.** By default a failed delete does
  not stop the rest (`continue_on_error: true`) and does not change the run's exit
  code directly — but it increments `cleanup_errors`, so gate on that metric in
  `thresholds:` if a leak should fail the run.
- **Runs even after a bad run.** `stop()` fires on the normal end of the run and
  on a threshold-driven abort, so resources created before the failure are still
  cleaned up. A hard kill (`SIGKILL`) skips `stop()`; the registry is in-memory, so
  in that case the run's data is not torn down.
- **Keep credentials in the environment.** The API `Authorization` header and the
  SQL connection `url` are secrets — pass them via `${env.…}` / `${secrets.…}`,
  never hard-coded in the plan.
- **SQL deletes are parameterised.** Tracked ids are bound as query parameters,
  never interpolated into the statement, so an id that came from a response body
  cannot turn into SQL injection against your own database.
- **In-process service.** Native service plugins run in-process with full
  privileges (see [Native plugins](native.md#safety-notes)); this one does no I/O
  beyond the cleanup calls it issues at run end.
```
