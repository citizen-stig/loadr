# TCP & UDP

Raw socket round trips for protocols of your own: connect/bind, send a
payload, read a response, measure.

```yaml
- request:
    name: tcp ping
    url: tcp://gateway.example.com:7000
    socket:
      send_text: "PING ${vu}\r\n"     # UTF-8 payload with interpolation
      read_bytes: 64                  # read exactly N bytes...
      # read_until_close: true        # ...or until the server closes
      read_timeout: 2s                # default: the request timeout
    checks:
      - { type: body_contains, value: PONG }

- request:
    name: udp probe
    url: udp://stats.example.com:8125
    socket:
      send_hex: "deadbeef 0102"       # hex payload (whitespace ignored)
      read_timeout: 500ms             # waits for one datagram; absence = failure
```

Behaviour:

- **TCP** — connect (timed), send, then read per the options: `read_bytes`
  for a fixed length, `read_until_close` until EOF, or (default) a single
  read of whatever arrives first.
- **UDP** — bind an ephemeral port, `send_to`, then receive one datagram
  (or `read_bytes` worth) within `read_timeout`.

The received bytes become the response body, so every extractor and condition
(regex, boundary, size, `body_matches`…) works on binary-ish payloads via
their text forms.

Metrics: `tcp_reqs`/`tcp_req_duration`, `udp_reqs`/`udp_req_duration`,
`data_sent`, `data_received`.
