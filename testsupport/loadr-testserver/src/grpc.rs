use std::net::SocketAddr;
use std::pin::Pin;

use futures::Stream;
use tokio::sync::oneshot;
use tonic::{Request, Response, Status, Streaming};

use crate::TestServerError;

/// Generated protobuf/tonic code for `loadr.test.Echo`.
pub mod pb {
    #![allow(clippy::all, clippy::pedantic)]
    include!(concat!(env!("OUT_DIR"), "/loadr.test.rs"));
}

/// The compiled `FileDescriptorSet` for `proto/echo.proto`, usable for
/// dynamic codecs or reflection clients.
pub const FILE_DESCRIPTOR_SET: &[u8] =
    include_bytes!(concat!(env!("OUT_DIR"), "/echo_descriptor.bin"));

use pb::echo_server::{Echo, EchoServer};
use pb::{EchoRequest, EchoResponse};

type EchoResult<T> = Result<Response<T>, Status>;
type ResponseStream = Pin<Box<dyn Stream<Item = Result<EchoResponse, Status>> + Send>>;

#[derive(Debug, Default)]
struct EchoService;

#[tonic::async_trait]
impl Echo for EchoService {
    async fn unary_echo(&self, request: Request<EchoRequest>) -> EchoResult<EchoResponse> {
        let req = request.into_inner();
        Ok(Response::new(EchoResponse {
            message: req.message,
            index: 0,
            payload: req.payload,
        }))
    }

    type ServerStreamEchoStream = ResponseStream;

    async fn server_stream_echo(
        &self,
        request: Request<EchoRequest>,
    ) -> EchoResult<Self::ServerStreamEchoStream> {
        let req = request.into_inner();
        let repeat = if req.repeat > 0 { req.repeat } else { 3 };
        let message = req.message;
        let payload = req.payload;
        let stream = futures::stream::iter((0..repeat).map(move |index| {
            Ok(EchoResponse {
                message: message.clone(),
                index,
                payload: payload.clone(),
            })
        }));
        Ok(Response::new(Box::pin(stream)))
    }

    async fn client_stream_echo(
        &self,
        request: Request<Streaming<EchoRequest>>,
    ) -> EchoResult<EchoResponse> {
        let mut inbound = request.into_inner();
        let mut combined = String::new();
        let mut payload = Vec::new();
        let mut count = 0i32;
        while let Some(req) = inbound.message().await? {
            combined.push_str(&req.message);
            payload = req.payload;
            count += 1;
        }
        Ok(Response::new(EchoResponse {
            message: combined,
            index: count,
            payload,
        }))
    }

    type BidiEchoStream = ResponseStream;

    async fn bidi_echo(
        &self,
        request: Request<Streaming<EchoRequest>>,
    ) -> EchoResult<Self::BidiEchoStream> {
        let inbound = request.into_inner();
        let stream =
            futures::stream::unfold((Some(inbound), 0i32), |(inbound, index)| async move {
                let mut inbound = inbound?;
                match inbound.message().await {
                    Ok(Some(req)) => Some((
                        Ok(EchoResponse {
                            message: req.message,
                            index,
                            payload: req.payload,
                        }),
                        (Some(inbound), index + 1),
                    )),
                    Ok(None) => None,
                    Err(status) => Some((Err(status), (None, index))),
                }
            });
        Ok(Response::new(Box::pin(stream)))
    }
}

/// In-process tonic gRPC echo server implementing `loadr.test.Echo`:
///
/// - `UnaryEcho` echoes the request message (index 0).
/// - `ServerStreamEcho` sends `repeat` responses (default 3 when `repeat <= 0`)
///   with incrementing indexes.
/// - `ClientStreamEcho` concatenates all request messages; `index` carries the
///   message count.
/// - `BidiEcho` echoes each request with an incrementing index.
///
/// Also serves gRPC v1 server reflection. Shuts down on drop.
pub struct GrpcEchoServer {
    /// Bound address (always `127.0.0.1` with an ephemeral port).
    pub addr: SocketAddr,
    shutdown: Option<oneshot::Sender<()>>,
}

impl GrpcEchoServer {
    /// Spawns the server on `127.0.0.1` with an ephemeral port.
    pub async fn spawn() -> Result<Self, TestServerError> {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
        let addr = listener.local_addr()?;
        let (tx, rx) = oneshot::channel::<()>();
        let reflection = tonic_reflection::server::Builder::configure()
            .register_encoded_file_descriptor_set(FILE_DESCRIPTOR_SET)
            .build_v1()
            .map_err(|e| TestServerError::Grpc(e.to_string()))?;
        let incoming = futures::stream::unfold(listener, |listener| async move {
            let accepted = listener.accept().await.map(|(stream, _)| stream);
            Some((accepted, listener))
        });
        tokio::spawn(async move {
            let result = tonic::transport::Server::builder()
                .add_service(EchoServer::new(EchoService))
                .add_service(reflection)
                .serve_with_incoming_shutdown(incoming, async {
                    let _ = rx.await;
                })
                .await;
            if let Err(e) = result {
                tracing::warn!(error = %e, "grpc test server exited with error");
            }
            tracing::debug!("grpc test server stopped");
        });
        tracing::debug!(%addr, "grpc test server listening");
        Ok(Self {
            addr,
            shutdown: Some(tx),
        })
    }

    /// Base URL suitable for `EchoClient::connect`, e.g. `http://127.0.0.1:54321`.
    pub fn url(&self) -> String {
        format!("http://{}", self.addr)
    }

    /// Alias for [`url`](Self::url).
    pub fn base_url(&self) -> String {
        self.url()
    }

    /// The compiled `FileDescriptorSet` bytes for `proto/echo.proto`.
    pub fn file_descriptor_set_bytes() -> &'static [u8] {
        FILE_DESCRIPTOR_SET
    }

    /// Stops the server. Also happens automatically on drop.
    pub fn shutdown(&mut self) {
        if let Some(tx) = self.shutdown.take() {
            let _ = tx.send(());
        }
    }
}

impl Drop for GrpcEchoServer {
    fn drop(&mut self) {
        self.shutdown();
    }
}
