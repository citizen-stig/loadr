# HTTP

The HTTP client is built directly on hyper with a custom connection layer so
**every phase of every request is measured** — no averaged guesses:

| Metric | Phase |
|---|---|
| `http_req_blocked` | waiting for a connection (dns + connect + tls on cold connections; ~0 on reuse) |
| `http_req_connecting` | TCP connect |
| `http_req_tls_handshaking` | TLS handshake |
| `http_req_sending` | writing the request |
| `http_req_waiting` | time to first byte (TTFB) |
| `http_req_receiving` | reading the body |
| `http_req_duration` | sending + waiting + receiving |

Plus `http_reqs`, `http_req_failed` (transport error or status ≥ 400),
`data_sent`, `data_received`. Samples carry `name`, `method`, `status`,
`scenario`, `group`, `proto` tags.

## Versions

`defaults.http.version`:

- `auto` (default) — ALPN negotiation; HTTP/2 when the server offers it.
- `http1` — force HTTP/1.1.
- `http2` — offer only h2 over TLS.
- `http2-prior-knowledge` — HTTP/2 without negotiation, including plaintext.

HTTP/2 connections are multiplexed; HTTP/1.1 connections are kept alive and
reused **per VU** (a VU models one user agent: its own connections and cookie
jar). `keep_alive: false` closes after each request.

## TLS & mTLS

```yaml
defaults:
  http:
    tls:
      ca_file: ./internal-ca.pem        # extra trust roots (PEM, may contain several)
      cert_file: ./client.pem           # client certificate (mTLS)
      key_file: ./client-key.pem
      server_name: api.internal         # SNI override
      insecure_skip_verify: false       # accept any cert (testing only!)
```

Roots default to the bundled Mozilla store (webpki-roots). Everything is
rustls — no OpenSSL dependency.

## Redirects, compression, proxies

- Redirects followed by default (`max_redirects: 10`); 301/302/303 switch to
  GET, 307/308 preserve method and body. Timings accumulate across hops; the
  reported `url` is the final one.
- `compression: true` sends `Accept-Encoding: gzip, deflate, br` and
  transparently decompresses. `data_received` counts wire (compressed) bytes.
- `proxy: http://host:3128` routes plaintext requests via absolute-form and
  HTTPS via `CONNECT`.

## Cookies

Automatic per-VU jars (RFC 6265 domain/path/secure/expiry matching) — see
[Requests](../yaml/requests.md#cookies).
