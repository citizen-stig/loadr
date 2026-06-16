//! In-process echo servers for loadr integration tests.
//!
//! Every server binds `127.0.0.1` on an ephemeral port, runs on the ambient
//! tokio runtime, and shuts down when the handle is dropped (or via an
//! explicit [`shutdown`](HttpTestServer::shutdown) call).
//!
//! Provided servers:
//! - [`HttpTestServer`] — axum-based HTTP server with echo/status/delay/json/
//!   xml/html/cookies/gzip/redirect/login/large/headers/counter endpoints,
//!   plus a TLS variant ([`HttpTestServer::spawn_tls`]) serving HTTP/1.1 and
//!   h2 with a self-signed certificate.
//! - [`WsEchoServer`] — WebSocket echo server.
//! - [`SseTestServer`] — Server-Sent Events streaming server (`/events`).
//! - [`GrpcEchoServer`] — tonic gRPC echo service (unary, server-stream,
//!   client-stream, bidi) with v1 server reflection.
//! - [`TcpEchoServer`] / [`UdpEchoServer`] — raw byte echo.
//!
//! Redis moved to the `loadr-plugin-redis` crate, whose integration tests run
//! against a real Redis (`LOADR_TEST_REDIS_URL`), so no in-process RESP server
//! is kept here.

mod error;
mod grpc;
mod http_server;
mod sse;
mod tcp;
mod udp;
mod ws;

pub use error::TestServerError;
pub use grpc::{pb, GrpcEchoServer, FILE_DESCRIPTOR_SET};
pub use http_server::HttpTestServer;
pub use sse::SseTestServer;
pub use tcp::TcpEchoServer;
pub use udp::UdpEchoServer;
pub use ws::WsEchoServer;
