use std::convert::Infallible;
use std::net::SocketAddr;
use std::time::Duration;

use axum::response::sse::{Event, Sse};
use axum::routing::get;
use axum::Router;
use futures::stream::{self, Stream};
use futures::StreamExt as _;
use tokio::net::TcpListener;
use tokio::sync::oneshot;

use crate::TestServerError;

/// Number of events the test stream emits before closing.
const EVENT_COUNT: usize = 5;

/// In-process Server-Sent Events test server.
///
/// `GET /events` streams [`EVENT_COUNT`] `tick` events ~50ms apart
/// (`event: tick`, `data: {"n":N}`) then closes the stream. Shuts down on drop.
pub struct SseTestServer {
    /// Bound address (always `127.0.0.1` with an ephemeral port).
    pub addr: SocketAddr,
    shutdown: Option<oneshot::Sender<()>>,
}

impl SseTestServer {
    /// Spawns the server on `127.0.0.1` with an ephemeral port.
    pub async fn spawn() -> Result<Self, TestServerError> {
        let listener = TcpListener::bind("127.0.0.1:0").await?;
        let addr = listener.local_addr()?;
        let (tx, rx) = oneshot::channel::<()>();
        let app = Router::new().route("/events", get(events_handler));
        tokio::spawn(async move {
            let serve = axum::serve(listener, app).with_graceful_shutdown(async move {
                let _ = rx.await;
            });
            if let Err(e) = serve.await {
                tracing::warn!(error = %e, "sse test server exited with error");
            }
        });
        tracing::debug!(%addr, "sse test server listening");
        Ok(Self {
            addr,
            shutdown: Some(tx),
        })
    }

    /// The events endpoint URL, e.g. `http://127.0.0.1:54321/events`.
    pub fn url(&self) -> String {
        format!("http://{}/events", self.addr)
    }

    /// Base URL without a path, e.g. `http://127.0.0.1:54321`.
    pub fn base_url(&self) -> String {
        format!("http://{}", self.addr)
    }

    /// Stops the server. Also happens automatically on drop.
    pub fn shutdown(&mut self) {
        if let Some(tx) = self.shutdown.take() {
            let _ = tx.send(());
        }
    }
}

impl Drop for SseTestServer {
    fn drop(&mut self) {
        self.shutdown();
    }
}

async fn events_handler() -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
    let stream = stream::iter(0..EVENT_COUNT).then(|n| async move {
        tokio::time::sleep(Duration::from_millis(50)).await;
        Ok(Event::default()
            .event("tick")
            .data(format!("{{\"n\":{n}}}")))
    });
    Sse::new(stream)
}
