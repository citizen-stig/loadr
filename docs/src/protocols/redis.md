# Redis

Drive a Redis server with real commands and measure the round trip. loadr
speaks the **RESP** wire protocol directly over a raw TCP connection — no
client library, no pipelining — so every request is one command in, one reply
out, timed end to end.

```yaml
- request:
    name: set greeting
    url: redis://cache.example.com:6379
    body: "SET greeting hello"        # one RESP command per request
    checks:
      - { type: status, equals: 0 }   # 0 = OK, non-zero = RESP error reply
      - { type: body_contains, value: OK }
```

## When to use

Reach for this when the thing under test *is* Redis: cache warm-up storms,
key-space contention, `INCR` hot keys, or simply checking that latency holds
under a steady command rate. For anything that merely *uses* Redis behind an
HTTP API, test the API with the `http` handler instead.

## The target URL

```
redis://host[:port][/db]
```

- **scheme** must be `redis`.
- **port** defaults to `6379` when omitted.
- **db** — an optional numeric path selects a database. On a freshly opened
  connection loadr issues `SELECT <db>` before the first command; a failing
  `SELECT` surfaces as a connection error. `redis://host/3` selects db 3;
  `redis://host` leaves the default db 0.

```yaml
url: redis://127.0.0.1:6379       # default db
url: redis://cache.internal/2     # SELECT 2 on connect
```

## Expressing the command

The command is taken from the request **`body`**: a single line whose
whitespace-separated tokens become the command and its arguments. loadr encodes
them as a RESP array of bulk strings and sends exactly that.

```yaml
- request: { name: ping,  url: redis://localhost, body: "PING" }
- request: { name: write, url: redis://localhost, body: "SET session:${vu} active" }
- request: { name: read,  url: redis://localhost, body: "GET session:${vu}" }
- request: { name: bump,  url: redis://localhost, body: "INCR page:views" }
- request: { name: ttl,   url: redis://localhost, body: "EXPIRE session:${vu} 60" }
```

`${...}` interpolation works in the body like anywhere else, so per-VU keys and
data-feed values flow straight into the command.

Because the body is split on whitespace, arguments that themselves contain
spaces cannot be expressed this way — use distinct keys/values, or a value that
is a single token. An empty body is rejected ("no redis command provided").

## Connection pooling

Connections are **pooled per virtual user**, keyed by `host:port`:

- The first command from a VU to a given endpoint opens a TCP socket
  (`TCP_NODELAY` set), runs the optional `SELECT`, and keeps the socket.
- Every later command from that VU to the same endpoint **reuses** the open
  socket — no reconnect, no re-`SELECT`. This shows up in the timings: the
  first request has a non-zero `connect` phase, reused ones do not.
- If a pooled socket is found to be broken (the previous command left it in an
  error state, or the peer dropped it), loadr transparently discards it and
  dials a fresh one for that command.

Pools are per-VU, so N virtual users hold up to N live connections per endpoint
— size your scenario `vus` with the server's connection limit in mind.

## Replies, status, and body

A request **succeeds at the transport level** whenever loadr gets a well-formed
RESP reply. Whether that reply is an *error reply* is reflected in `status`:

| Reply | `status` | Body | `extras.reply_type` |
|-------|----------|------|----------------------|
| `+OK` simple string | `0` | the string (`OK`) | `string` |
| `:42` integer | `0` | the number as text (`42`) | `integer` |
| `$5\r\nhello` bulk string | `0` | the bytes (`hello`) | `bulk` |
| `*…` array | `0` | the array rendered as JSON | `array` |
| `$-1` / `*-1` null | `0` | empty | `nil` |
| `-ERR …` error reply | non-zero | the error text | `error` |

So a missing key (`GET` of an absent key → nil) is a *success* with an empty
body, while `-ERR unknown command` is a *failure* (`status` ≠ 0, the message
also lands in `error`). A connection failure or timeout is reported as
`status: 0` with `error` set and no reply.

`extras` carries the parsed reply for assertions and extraction:

- `extras.reply_type` — one of `string`, `integer`, `bulk`, `array`, `nil`,
  `error`.
- `extras.value` — the reply as JSON: a string for simple/bulk/error replies,
  a number for integers, an array for multi-bulk replies, `null` for nil.

## Checks and assertions

Checks run against the same body and status every other protocol exposes:

```yaml
- request:
    name: increment counter
    url: redis://localhost
    body: "INCR jobs:done"
    assert:
      - { type: status, equals: 0 }                  # not an error reply
    checks:
      - { type: body_matches, pattern: '^[0-9]+$' }  # integer came back
      - { type: duration, name: cache is fast, max: 5ms }

- request:
    name: read flag
    url: redis://localhost
    body: "GET feature:beta"
    checks:
      - { type: body_contains, value: "on" }
      - { type: size, name: non-empty, min: 1 }      # fail if key was nil
```

- `status` — `equals: 0` to require a non-error reply (or `one_of`/`matches`).
- `body_contains` / `body_matches` — match against the reply value as text
  (the bulk value, the simple string, or the integer's digits).
- `size` — bound the reply length; `min: 1` is a handy "key existed" guard,
  since a nil reply has an empty body.
- `duration` — cap the per-command round trip.

Checks are recorded to the `checks` metric and never fail the request;
`assert` entries mark the request failed (and can abort via `on_failure`).

Extract reply values for later steps with the usual extractors, e.g. a
`boundary`/`body_matches` extractor over the reply text.

## Timings & metrics

The handler measures the command lifecycle: the TCP `connect` phase (first
command only), `sending` while the command is written and flushed, and
`waiting` while the reply is read. `duration` is their sum. `bytes_sent` counts
the encoded command, `bytes_received` the reply body.

Standard request metrics apply (`data_sent`, `data_received`, and the request
rate/duration series), so thresholds work as for any protocol:

```yaml
thresholds:
  checks: [ "rate>0.99" ]
```
