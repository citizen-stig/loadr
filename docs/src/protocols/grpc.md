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
- `grpcs://` uses the standard TLS config (custom CAs, mTLS).

### Shared-pool sizing

Size a shared pool from the number of RPCs that can be in flight at once
(Little's Law), independently for each agent and endpoint:

```text
in_flight = target_RPC/s_per_agent × p99_latency_seconds
effective_streams_per_connection = min(client_limit, server_MAX_CONCURRENT_STREAMS)
pool_size ≥ ceil(in_flight × headroom / effective_streams_per_connection)
```

Use a headroom factor of 1.25–1.5 for latency variation and bursts. With the
raw transport, the client limit defaults to 512; use a lower value if the
server advertises a lower HTTP/2 `MAX_CONCURRENT_STREAMS`. The channel
transport has no equivalent loadr semaphore, so size against the server limit
and confirm the result with a pool-size sweep.

The target is **gRPC calls per second**, not executor iterations or business
transactions per second. If one transaction makes multiple RPCs, include all
of them in the target RPC rate. In distributed runs, apply the calculation to
the rate handled by one agent, because every agent owns its own pool.

For example, one agent targeting 500,000 RPC/s at 200 ms p99 needs 100,000
concurrent streams. With raw's 512-stream default, the absolute minimum is
`ceil(100,000 / 512) = 196` connections. At 1.5× headroom it is
`ceil(100,000 × 1.5 / 512) = 293`; round up to a practical pool size such as
320:

```yaml
grpc:
  transport: raw
  channel_pool_size: 320
```

By comparison, a 64-connection raw pool permits at most 32,768 concurrent
streams. At 500,000 RPC/s it reaches that stream-cap ceiling at about 65.5 ms;
at 200 ms the corresponding ceiling is about 163,840 RPC/s. These are
transport-capacity upper bounds: CPU, network, server capacity, flow control,
and response size may produce a lower ceiling.

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
