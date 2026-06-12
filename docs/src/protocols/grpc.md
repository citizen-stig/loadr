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
- Channels are pooled per VU per endpoint; proto descriptor pools are
  compiled once and cached process-wide.
- `grpcs://` uses the standard TLS config (custom CAs, mTLS).
