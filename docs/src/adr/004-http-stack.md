# ADR-004: HTTP stack — hyper with a hand-rolled timing connector

**Status**: accepted

## Context

A load tool's HTTP numbers are its product. We need per-phase timings (DNS,
connect, TLS, send, TTFB, receive), exact byte counts, HTTP/1.1 + HTTP/2 with
version forcing, mTLS/custom CAs, redirects, decompression, proxies — and a
connection model that matches what a "virtual user" means.

## Decision

Build directly on `hyper` (client conn API) with our own connection
establishment: tokio DNS lookup, `TcpStream::connect`, `tokio-rustls`
handshake — each phase individually timed — then `http1`/`http2` handshakes
picked by ALPN or configuration. Connections are pooled **per VU**.
No reqwest.

## Rationale

- reqwest (and most high-level clients) hide connection reuse and phase
  boundaries; you simply cannot report honest `http_req_blocked`/
  `http_req_tls_handshaking` through it.
- Per-VU pooling models reality: a VU is one user agent with its own
  keep-alive connections and cookie jar. Global pools (the reqwest default)
  understate connection-establishment cost dramatically and produce
  multiplexing patterns no real client population exhibits.
- rustls everywhere: no OpenSSL build matrix, distroless-friendly, and one
  TLS config shared by HTTP, WebSocket and gRPC.

## Consequences

- We own redirects, decompression (gzip/deflate/br), proxy CONNECT, cookie
  injection and byte accounting — all unit-tested against the in-repo test
  server (including TLS with generated certs and forced HTTP/1.1 vs h2).
- HTTP/3 is future work; the connector design (phase-timed dial + versioned
  handshake) has a clear slot for quinn.
