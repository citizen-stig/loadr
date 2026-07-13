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
`jsonpath` extraction/assertions work naturally. `extras.message_count` holds
the number of streamed responses.

**Automatic skip**: when nothing needs the response body — no `extract`, no
`assert`/`checks` beyond `status`/`duration`/`header` (jsonpath, body/size
checks, `js`, ... all count as reading it), and no script `afterRequest`
hook — loadr skips building it entirely: no `DynamicMessage`, no JSON
conversion. This is automatic, with no config knob; the common status-only
load-generation case gets the full speedup for free. `body` is empty and
`message_count` still reflects every frame received; stream draining, status
and timings are unaffected.

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
- Each pooled channel queues outbound calls in a bounded `tower::buffer`;
  its depth defaults to 4096 and is configurable via the
  `LOADR_GRPC_BUFFER_SIZE` environment variable. It only applies to
  `channel_pool_size` channels, not the default per-VU ones — raise it if
  many VUs share a pooled channel and calls start stalling in `ready()`
  before the connection itself is the bottleneck.
- `grpcs://` uses the standard TLS config (custom CAs, mTLS).
