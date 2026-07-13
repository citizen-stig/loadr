# gRPC

loadr calls gRPC services **dynamically** — no code generation, no `protoc`
binary. Describe the service either with `.proto` files (compiled in-process
by protox) or via **server reflection**.

```yaml
- request:
    name: say hello
    url: grpc://greeter.example.com:50051       # grpcs:// for TLS
    grpc:
      proto_files: [ protos/helloworld.proto ]  # relative to the test file
      proto_includes: [ protos/ ]               # import search paths
      service: helloworld.Greeter
      method: SayHello
      message: { name: "vu-${vu}" }             # request message as JSON
      metadata: { x-api-key: "${secrets.key}" }
    assert:
      - { type: status, equals: 0 }             # gRPC code: 0 = OK
      - { type: jsonpath, expression: "$.message", exists: true }
```

With reflection instead of files:

```yaml
grpc:
  reflection: true
  service: helloworld.Greeter
  method: SayHello
  message: { name: "world" }
```

## Streaming

All four shapes are supported. Streaming requests provide `messages`
(a list) instead of `message`:

```yaml
grpc:
  reflection: true
  service: helloworld.Greeter
  method: LotsOfReplies          # server streaming: responses collected
  message: { name: "stream" }
---
grpc:
  service: pkg.Ingest
  method: Push                   # client streaming
  messages: [ { v: 1 }, { v: 2 }, { v: 3 } ]
```

The response body is the (last) response message rendered as JSON, so
`jsonpath` extraction/assertions work naturally. `extras.messages` holds
every streamed response; `extras.message_count` the count.

## Semantics & metrics

- `status` is the gRPC status code (0 = OK); non-zero marks the request
  failed. `status_text` carries the code name and message.
- Metrics: `grpc_reqs`, `grpc_req_duration`, plus `data_sent`/`data_received`.
- By default, channels are pooled **per VU** per endpoint; proto descriptor
  pools are compiled once and cached process-wide.
- One connection per VU stops scaling past a couple thousand VUs against a
  single endpoint. Set `channel_pool_size` to instead share a fixed pool of
  HTTP/2 channels across **all** VUs (round-robined; each multiplexes many
  concurrent streams):

  ```yaml
  grpc:
    channel_pool_size: 8   # 8 shared connections instead of one per VU
  ```
- With `transport: channel`, each pooled channel queues outbound calls in a
  bounded `tower::buffer`; its depth defaults to 4096 and is configurable via
  the `LOADR_GRPC_BUFFER_SIZE` environment variable. It only applies to
  `channel_pool_size` channels, not the default per-VU ones — raise it if
  many VUs share a pooled channel and calls start stalling in `ready()`
  before the connection itself is the bottleneck.
- `grpcs://` uses the standard TLS config (custom CAs, mTLS).

## Transport

`transport` selects the client stack driving the calls (default: `channel`):

```yaml
grpc:
  transport: raw           # channel (default) | raw
  channel_pool_size: 8     # works with either transport
```

- `channel` — tonic's `Channel`: every request goes through a tower::buffer
  queue owned by a per-channel worker task.
- `raw` — drives hyper HTTP/2 directly from the VU task: no intermediate
  queue, no per-channel worker, fewer wakeups per request. An experimental
  performance path for high-rate runs; measure with an A/B before relying
  on it.

Raw specifics:

- In-flight streams per connection are capped (default 512,
  `LOADR_GRPC_MAX_STREAMS_PER_CONN`) so a slow server cannot grow pending
  streams without bound.
- After a failed dial, calls fail fast with `Unavailable` for 500 ms before
  the next dial attempt. Connection failures surface exactly like the
  channel transport's: status 14, `connection failed: ...`.
- `grpcs://` with `transport: raw` honors `insecure_skip_verify` and TLS
  `min_version`/`max_version` (the channel transport warns and ignores
  them).
- The raw transport sends no `user-agent` header (tonic sends
  `tonic/<version>`).

`LOADR_GRPC_TRANSPORT=raw|channel` overrides `transport` for every request
in the process — handy for whole-fleet A/B runs without editing plans.
