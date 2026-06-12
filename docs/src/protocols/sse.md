# Server-Sent Events

A `request` with an `sse://`/`sses://` URL opens a one-way Server-Sent Events
stream: connect → `GET` with `Accept: text/event-stream` → read events
frame-by-frame until a stop condition → close.

```yaml
- request:
    name: order updates
    url: sse://events.example.com/orders/stream
    headers:
      Authorization: Bearer ${token}        # sent on the GET handshake
      Last-Event-ID: "${cursor}"
    checks:
      - { type: body_contains, value: '"status":"shipped"' }   # runs on the LAST event's data
```

The handler always issues a `GET` (any other method is an error) and adds
`Accept: text/event-stream`, `Cache-Control: no-cache` and
`Connection: keep-alive` for you. Caller `headers` and the VU's cookie jar are
merged in. `sse://` maps to `http://`; `sses://` maps to `https://` and uses the
same TLS configuration as HTTP (custom CAs, mTLS, `insecure_skip_verify`,
`server_name`).

## Wire format

The stream is parsed per the SSE spec: `event:`, `data:`, `id:` and `retry:`
fields are accumulated and an event is dispatched on each blank line. Multiple
`data:` lines are joined with `\n`; a missing `event:` defaults to `message`;
comment lines (starting with `:`) are ignored; `retry:` is recognised but not
acted upon (reads are single-shot). A leading space after the field colon is
stripped, and both `\n` and `\r\n` line endings are handled.

## Stop conditions

By default the stream is read until the server closes it or the request
`timeout` elapses. Three limits bound the read (whichever is hit first wins, and
the request `timeout` always caps everything):

| Option | Meaning |
|---|---|
| `events` | Stop after this many events have been dispatched. |
| `until` | Stop on the first event whose `data` contains this substring. |
| `duration` | Stop after this wall-clock window (e.g. `10s`, `500ms`, `2m`, or a bare number of seconds). |

## Metrics

| Metric | Meaning |
|---|---|
| `plugin_reqs` | Count of completed SSE requests |
| `plugin_req_duration` | send + wait (TTFB) + receive time |
| `data_sent` / `data_received` | request bytes / streamed event bytes |
| `http_req_failed` | failure rate (transport error or stream read error) |

Samples are tagged `proto=sse` alongside the usual `name`, `method` and
`status`. The reported `status` is the HTTP status of the stream response (e.g.
`200`); a connection or handshake failure reports status `0` with an `error`.

## Extraction, checks and assertions

The **data of the last received event** becomes the response body, so every
extractor and condition (`body_contains`, `body_matches`, regex, `size`,
`status`, `header`…) operates on it. A `js` condition sees the response as
`response` with `status`, `status_text`, `body`, `headers`, `duration_ms`,
`error`, `url` and `protocol` in scope.

```yaml
checks:
  - { type: body_contains, name: shipped, value: '"status":"shipped"' }
assert:
  - { type: status, equals: 200 }
  - { type: js, expression: 'response.body.length > 0' }
```

`checks` are recorded to the `checks` metric and never fail the request;
`assert` failures mark the request failed.

Beyond the response body, the handler also reports `events_received`,
`last_event` (`{ "type", "data", "id" }`) and the parsed `events` (capped at the
first 100) as protocol extras, which surface in run reports.
