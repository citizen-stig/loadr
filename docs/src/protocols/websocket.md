# WebSocket

A `request` with a `ws://`/`wss://` URL (or `protocol: ws`) opens a WebSocket
session: connect → send frames → receive until a condition → close.

```yaml
- request:
    name: chat session
    url: wss://chat.example.com/ws
    headers: { Origin: https://chat.example.com }   # handshake headers
    ws:
      subprotocols: [ "chat.v2" ]
      send:
        - '{"type":"hello"}'                          # text frame
        - { text: '{"type":"msg","body":"hi ${vu}"}', delay: 500ms }
        - { binary_base64: "3q2+7w==", delay: 100ms } # binary frame
      receive_count: 2          # close after N received messages
      receive_until: '"done"'   # ...or when a text message contains this
      session_duration: 10s     # ...or after this long (request timeout still caps everything)
    checks:
      - { type: body_contains, value: '"type":"ack"' }   # runs on the LAST received message
```

Default receive behaviour (when neither `receive_count` nor `receive_until`
is set): wait for one message per sent frame.

## Metrics

| Metric | Meaning |
|---|---|
| `ws_connecting` | TCP + TLS + upgrade handshake time |
| `ws_session_duration` | open → close |
| `ws_msgs_sent` / `ws_msgs_received` | frame counters |
| `data_sent` / `data_received` | payload bytes |

Extraction and conditions operate on the **last received message** as the
response body; `extras` exposes `msgs_sent`, `msgs_received` and
`last_message` for `js` conditions.

wss:// uses the same TLS configuration as HTTP (custom CAs, mTLS,
`insecure_skip_verify`).
