# Redis plugin

`loadr-plugin-redis` adds Redis as a load-test target. It is a **native
protocol plugin**: Redis support is not built into loadr core. The plugin speaks
the **RESP** wire protocol directly over a raw TCP connection — no client
library, no OpenSSL, no pipelining — so every request is one command in, one
reply out, timed end to end. Once the plugin is installed, a request to a
`redis://` (or `rediss://`) URL routes straight to it.

The contract it uses is documented in
[Developing a plugin](developing.md#native-protocol-plugins).

## Build and install

```bash
cargo build -p loadr-plugin-redis --release

# `loadr plugin install` copies a directory that holds plugin.toml next to the
# artifact named by its `entry`. Stage the built cdylib beside the manifest:
mkdir -p dist
cp plugins/loadr-plugin-redis/plugin.toml dist/
cp target/release/libloadr_plugin_redis.so dist/   # .dylib on macOS, .dll on Windows
loadr plugin install dist
loadr plugin info redis
```

Installing copies `plugin.toml` and the artifact into `~/.loadr/plugins/redis/`.
The manifest declares the URL schemes the plugin serves:

```toml
[plugin]
name = "redis"
kind = "protocol"
type = "native"
entry = "libloadr_plugin_redis.so"
schemes = ["redis", "rediss"]
```

## The target URL

```
redis://host[:port][/db]
```

- **scheme** must be `redis` (or `rediss`).
- **port** defaults to `6379` when omitted.
- **db** — an optional numeric path selects a database. On a freshly opened
  connection the plugin issues `SELECT <db>` before the first command; a failing
  `SELECT` surfaces as a connection error. `redis://host/3` selects db 3;
  `redis://host` leaves the default db 0.

## Use it in a test

List the plugin under `plugins:` and target a `redis://` URL. The command is the
`plugin.command` argv array:

```yaml
plugins:
  - name: redis            # or: { name: redis, path: target/release/libloadr_plugin_redis.so }

scenarios:
  main:
    executor: constant-vus
    vus: 20
    duration: 15s
    flow:
      - request:
          name: set session
          url: redis://cache.example.com:6379
          plugin:
            command: ["SET", "session:${vu}", "active"]
          checks:
            - { type: status, equals: 0 }      # 0 = OK, 1 = RESP error reply
            - { type: body_contains, value: OK }

      - request:
          name: get session
          url: redis://cache.example.com:6379
          plugin:
            command: ["GET", "session:${vu}"]
          checks:
            - { type: body_contains, value: active }

      - request:
          name: increment counter
          url: redis://cache.example.com:6379
          plugin:
            command: ["INCR", "page:views"]
          checks:
            - { type: body_matches, pattern: '^[0-9]+$' }   # integer reply
```

A complete runnable plan is in `examples/30-redis.yaml`.

## Expressing the command

The command is the `plugin.command` array — its elements (strings or numbers)
become the command name and its arguments, encoded as a RESP array of bulk
strings. As a fallback, the request **`body`** is accepted: a single line whose
whitespace-separated tokens form the command.

```yaml
plugin: { command: ["SET", "session:${vu}", "active"] }   # preferred (argv)
# or, via the body fallback:
body: "PING"
```

`${...}` interpolation works inside any string element, so per-VU keys and
data-feed values flow straight into the command. The argv form (unlike the body
fallback) can carry argument values that contain spaces. An empty command is
rejected ("no redis command provided").

## Replies, status, and body

A request **succeeds at the transport level** whenever the plugin gets a
well-formed RESP reply. Whether that reply is an *error reply* is reflected in
`status`:

| Reply | `status` | Body | `extras.reply_type` |
|-------|----------|------|----------------------|
| `+OK` simple string | `0` | the string (`OK`) | `string` |
| `:42` integer | `0` | the number as text (`42`) | `integer` |
| `$5\r\nhello` bulk string | `0` | the bytes (`hello`) | `bulk` |
| `*…` array | `0` | the array rendered as JSON | `array` |
| `$-1` / `*-1` null | `0` | empty | `nil` |
| `-ERR …` error reply | `1` | — | `error` |

So a missing key (`GET` of an absent key → nil) is a *success* with an empty
body, while `-ERR unknown command` is a *failure* (`status` = 1, the message
also lands in `error`). A connection failure or timeout is reported as
`status: 0` with `error` set and no reply.

`extras` carries the parsed reply for assertions and extraction:

- `extras.reply_type` — one of `string`, `integer`, `bulk`, `array`, `nil`,
  `error`.
- `extras.value` — the reply as JSON: a string for simple/bulk/error replies,
  a number for integers, an array for multi-bulk replies, `null` for nil.

## Metrics

loadr turns the plugin's response into a dedicated metric family named after the
protocol (`redis`):

| Metric              | Kind    | Meaning |
|---------------------|---------|---------|
| `redis_reqs`        | counter | One per command |
| `redis_req_duration`| trend   | Command round-trip latency (ms) |

A request is marked **failed** when the command errors (a RESP error reply, a
connection failure, or a timeout). `http_req_failed` therefore tracks the Redis
failure rate too, and `checks` / `assert` entries can gate on `status` (0 = ok).

```yaml
thresholds:
  checks: [ "rate>0.99" ]
  redis_req_duration: [ "p(95)<100ms" ]
```

## Connection pooling

The plugin keeps an internal pool of live RESP connections keyed by
`host:port`, shared across every VU. A command checks out an idle connection
(running the optional `SELECT` on a fresh socket only), reuses it for the
exchange, and returns it for the next caller — so concurrent VUs reuse a small
set of sockets rather than reconnecting on every command. A connection left in
an error state is dropped instead of returned, so the next caller transparently
re-establishes it. The plugin owns a single Tokio runtime and `block_on`s the
async socket I/O, because the protocol ABI is synchronous and carries no per-VU
context across the FFI boundary.

## Testing against a real server

The example harness brings up `redis:7-alpine`:

```bash
docker compose -f examples/harness/docker-compose.yml up -d redis

LOADR_TEST_REDIS_URL=redis://127.0.0.1:6379 \
  cargo test -p loadr-plugin-redis
```

The integration tests no-op when `LOADR_TEST_REDIS_URL` is unset.
