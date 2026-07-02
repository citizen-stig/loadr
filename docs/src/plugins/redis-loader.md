# Redis loader plugin

> **Status:** planned — not yet in the signed [plugin index](installing.md). This
> page documents the intended shape; the config keys and metric name may still
> change before the first release.

`loadr-plugin-redis-loader` is a **service** plugin in the *data sources &
feeders* role. Instead of driving a target, it acts as a **data source**: a
long-running component that connects to Redis and hands each VU a value it pops
from a list or reads from a stream. Because the values come from one shared
Redis key rather than a local file, the feed is a **distributed shared feeder**
— every worker in a distributed run draws from the same queue, so a work item is
consumed exactly once across the whole fleet.

Like the [redis](redis.md) protocol plugin, it speaks the **RESP** wire protocol
directly over a raw TCP socket — no `redis` crate, no C client, no OpenSSL. It
reuses that plugin's socket and connection-pool approach, so installing it adds
no build toolchain requirement.

The contract it uses is documented in
[Developing a plugin](developing.md#services).

## Install

`redis-loader` will ship in the signed [plugin index](installing.md), so once
released you install it by name — no build toolchain required:

```bash
loadr plugin install redis-loader
loadr plugin info redis-loader
```

This resolves `redis-loader` in the index, picks the artifact for your host
target, checks it against the plugin ABI your `loadr` build provides, downloads
it, verifies its sha256 and unpacks it into your plugins directory
(`~/.loadr/plugins/redis-loader/`, or `$LOADR_PLUGINS_DIR`).

The installed manifest declares a service plugin:

```toml
[plugin]
name = "redis-loader"
kind = "service"
type = "native"
entry = "libloadr_plugin_redis_loader.so"
```

To run straight from a build tree instead, point the plan's `plugins:` entry at
the built artifact
(`path: target/release/libloadr_plugin_redis_loader.so`) rather than resolving
it by name.

## Use it in a test

List the plugin under `plugins:`, then declare a `data:` feeder whose `service:`
names it. The plugin's `config:` block tells it where to connect and how to pop
values; each read binds a value the VUs reference through the usual
`${data.<name>.value}` interpolation.

```yaml
plugins:
  - name: redis-loader        # or: { name: redis-loader, path: target/release/libloadr_plugin_redis_loader.so }

data:
  jobs:
    service: redis-loader     # this feeder is backed by the service plugin
    config:
      url: redis://queue.example.com:6379
      key: queue              # the Redis list/stream key to drain
      mode: lpop              # lpop | rpop | xread
    on_eof: stop              # what to do when the key is empty (see below)

scenarios:
  process_queue:
    executor: constant-vus
    vus: 25
    duration: 5m
    flow:
      - request:
          name: process job
          method: POST
          url: https://api.example.com/jobs
          body: { json: { id: "${data.jobs.value}" } }
          checks:
            - { type: status, equals: 200 }
```

Every VU that reaches the feeder is handed the next value popped from `queue`.
Because the pop happens on the shared Redis server, two VUs — on the same worker
or on different distributed workers — never receive the same item.

## Config reference

The feeder's behaviour is set through the plugin `config:` block:

| Key    | Required | Default | Meaning |
|--------|----------|---------|---------|
| `url`  | yes      | —       | Redis endpoint, `redis://host[:port][/db]` (or `rediss://`). Port defaults to `6379`; an optional `/db` path selects a database via `SELECT`. |
| `key`  | yes      | —       | The Redis key to draw values from — a list for the `*pop` modes, a stream for `xread`. |
| `mode` | no       | `lpop`  | How a value is taken (see table below). |

`mode` selects the read command:

| `mode`  | Redis command | Source | Order |
|---------|---------------|--------|-------|
| `lpop`  | `LPOP key`    | list   | head-first (FIFO with `RPUSH`) |
| `rpop`  | `RPOP key`    | list   | tail-first (LIFO / stack) |
| `xread` | `XREAD` on `key` | stream | by stream ID, advancing a cursor |

The feeder honours the standard feeder `on_eof:` policy when the key drains:
`stop` retires the VU (the default for a work queue), `recycle` blocks and
re-polls for new items. `${data.jobs.value}` binds the popped value; for stream
entries, individual fields are also exposed (for example
`${data.jobs.field.<name>}`).

## Metrics

The plugin emits one counter as it feeds:

| Metric              | Kind    | Meaning |
|---------------------|---------|---------|
| `redis_loader_rows` | counter | One per value handed to a VU (rows popped / stream entries read) |

Track it to confirm the queue is actually draining at the rate you expect, and
to reconcile items consumed against items enqueued:

```yaml
thresholds:
  redis_loader_rows: [ "count>0" ]
```

## Notes

- **Distributed shared feeder.** The whole point of the plugin: the cursor lives
  in Redis, not in the loadr process. A `data:` CSV/JSON feeder with
  `mode: shared` is shared only within a single worker; `redis-loader` is shared
  across *every* worker in a distributed run, so a job is processed exactly once
  fleet-wide.
- **Reuses the redis socket layer.** Connections are raw RESP over TCP, pooled by
  `host:port` and shared across VUs — the same approach as the
  [redis](redis.md) protocol plugin, which is why no C client is needed.
- **`lpop` vs `rpop`.** Pair `RPUSH` producers with `lpop` for FIFO ordering; use
  `rpop` for LIFO / stack semantics.
- **Empty-key behaviour.** With `on_eof: stop` an empty key ends the VU, which is
  usually what you want for a finite work queue; `on_eof: recycle` keeps VUs
  polling for newly enqueued items, useful when a producer runs alongside the
  test.
