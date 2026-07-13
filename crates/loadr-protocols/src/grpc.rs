//! Dynamic gRPC handler: invokes any service/method without generated code.
//!
//! Message descriptors come either from `.proto` files compiled in-process
//! with protox, or from gRPC server reflection (v1). Calls go through
//! `tonic::client::Grpc` with a [`DynamicCodec`] that encodes/decodes
//! [`prost_reflect::DynamicMessage`] values, so all four call shapes (unary,
//! server-/client-streaming, bidi) work from plain JSON messages.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, OnceLock, PoisonError, RwLock};
use std::time::Instant;

use async_trait::async_trait;
use bytes::{BufMut as _, Bytes};
use loadr_config::{GrpcTransport, HttpDefaults};
use loadr_core::error::ProtocolError;
use loadr_core::protocol::{
    GrpcRequest, PreparedRequest, ProtocolHandler, ProtocolResponse, Timings,
};
use loadr_core::vu::VuContext;
use prost::Message as _;
use prost_reflect::{DescriptorPool, DynamicMessage, MethodDescriptor};
use tonic::client::GrpcService;
use tonic::codec::{Codec, DecodeBuf, Decoder, EncodeBuf, Encoder};
use tonic::codegen::{Body as HttpBody, StdError};
use tonic::metadata::{MetadataKey, MetadataValue};
use tonic::transport::{Channel, Endpoint};
use tonic::Status;
use tonic_reflection::pb::v1::server_reflection_client::ServerReflectionClient;
use tonic_reflection::pb::v1::server_reflection_request::MessageRequest;
use tonic_reflection::pb::v1::server_reflection_response::MessageResponse;
use tonic_reflection::pb::v1::ServerReflectionRequest;

use crate::grpc_transport::{RawChannel, TlsParams};
use crate::net::ms_since;

// ---------------------------------------------------------------------------
// Descriptor pool cache (global: compiling protos / reflection is expensive)
// ---------------------------------------------------------------------------

fn pool_cache() -> &'static Mutex<HashMap<String, DescriptorPool>> {
    static CACHE: OnceLock<Mutex<HashMap<String, DescriptorPool>>> = OnceLock::new();
    CACHE.get_or_init(|| Mutex::new(HashMap::new()))
}

/// tower::buffer queue size per pooled channel (`LOADR_GRPC_BUFFER_SIZE`).
fn grpc_buffer_size() -> usize {
    static SIZE: OnceLock<usize> = OnceLock::new();
    *SIZE.get_or_init(|| {
        std::env::var("LOADR_GRPC_BUFFER_SIZE")
            .ok()
            .and_then(|v| v.parse::<usize>().ok())
            .unwrap_or(4096)
            .max(1)
    })
}

fn cache_get(key: &str) -> Option<DescriptorPool> {
    pool_cache()
        .lock()
        .ok()
        .and_then(|map| map.get(key).cloned())
}

fn cache_put(key: String, pool: DescriptorPool) {
    if let Ok(mut map) = pool_cache().lock() {
        map.insert(key, pool);
    }
}

// ---------------------------------------------------------------------------
// Dynamic codec
// ---------------------------------------------------------------------------

/// tonic codec for [`DynamicMessage`] using the method's I/O descriptors.
#[derive(Clone)]
struct DynamicCodec {
    output: prost_reflect::MessageDescriptor,
}

impl DynamicCodec {
    fn for_method(method: &MethodDescriptor) -> Self {
        DynamicCodec {
            output: method.output(),
        }
    }
}

/// Outbound message: built dynamically per call, or pre-encoded bytes served
/// from the per-VU literal-message cache (skips JSON→DynamicMessage→encode).
#[derive(Clone)]
enum Outbound {
    Dynamic(DynamicMessage),
    Encoded(Bytes),
}

impl Codec for DynamicCodec {
    type Encode = Outbound;
    type Decode = DynamicMessage;
    type Encoder = DynamicEncoder;
    type Decoder = DynamicDecoder;

    fn encoder(&mut self) -> Self::Encoder {
        DynamicEncoder
    }

    fn decoder(&mut self) -> Self::Decoder {
        DynamicDecoder {
            desc: self.output.clone(),
        }
    }
}

struct DynamicEncoder;

impl Encoder for DynamicEncoder {
    type Item = Outbound;
    type Error = Status;

    fn encode(&mut self, item: Outbound, dst: &mut EncodeBuf<'_>) -> Result<(), Status> {
        match item {
            Outbound::Dynamic(message) => message
                .encode(dst)
                .map_err(|e| Status::internal(format!("failed to encode message: {e}"))),
            Outbound::Encoded(bytes) => {
                dst.put_slice(&bytes);
                Ok(())
            }
        }
    }
}

struct DynamicDecoder {
    desc: prost_reflect::MessageDescriptor,
}

impl Decoder for DynamicDecoder {
    type Item = DynamicMessage;
    type Error = Status;

    fn decode(&mut self, src: &mut DecodeBuf<'_>) -> Result<Option<DynamicMessage>, Status> {
        let mut message = DynamicMessage::new(self.desc.clone());
        message
            .merge(&mut *src)
            .map_err(|e| Status::internal(format!("failed to decode message: {e}")))?;
        Ok(Some(message))
    }
}

// ---------------------------------------------------------------------------
// Channels: per-VU (default) and shared pool (opt-in), on either transport
// ---------------------------------------------------------------------------

/// A fixed set of lazily connected channels, handed out round-robin.
/// Cloning a `Channel` or `RawChannel` is cheap and shares the underlying
/// connection, so a small pool multiplexes arbitrarily many concurrent
/// streams.
struct RoundRobin<T: Clone> {
    items: Vec<T>,
    next: AtomicU64,
}

impl<T: Clone> RoundRobin<T> {
    fn next(&self) -> T {
        let i = self.next.fetch_add(1, Ordering::Relaxed) as usize % self.items.len();
        self.items[i].clone()
    }

    fn len(&self) -> usize {
        self.items.len()
    }
}

/// Lazily connected gRPC channels per endpoint, stored per VU.
#[derive(Default)]
struct GrpcChannels {
    channels: HashMap<String, Channel>,
    /// VU-local memo of shared pools (the pools themselves are global).
    pools: HashMap<String, Arc<RoundRobin<Channel>>>,
    /// Raw hyper-h2 channels per endpoint (`transport: raw`).
    raws: HashMap<String, RawChannel>,
    /// VU-local memo of shared raw pools.
    raw_pools: HashMap<String, Arc<RoundRobin<RawChannel>>>,
    /// Resolved call state, one entry per distinct request shape this VU has
    /// executed (usually one). Linear scan on string keys — no per-request
    /// allocation. A VU runs one request at a time, so `&mut` access and the
    /// cached `Grpc` client's poll_ready discipline are safe; never share
    /// these across VUs.
    calls: Vec<CachedCall>,
}

/// Everything about a (endpoint, service, method) call that is invariant
/// across iterations: descriptors, path, codec, shape, a client pinned to the
/// VU's channel, and encoded bodies for literal messages.
struct CachedCall {
    identity: CachedCallIdentity,
    input_desc: prost_reflect::MessageDescriptor,
    path: http::uri::PathAndQuery,
    codec: DynamicCodec,
    shape: (bool, bool),
    client: CachedClient,
    /// Encoded literal message frames keyed by the message `Arc`'s pointer
    /// identity (stable for the run: the Arc is owned by the compiled plan).
    encoded: HashMap<usize, EncodedMessages>,
}

struct CachedCallIdentity {
    endpoint: String,
    service: String,
    method_name: String,
    reflection: bool,
    proto_files: Vec<PathBuf>,
    /// Part of the identity: the same files can resolve differently under
    /// different include roots.
    proto_includes: Vec<PathBuf>,
    pool_size: Option<usize>,
    transport: GrpcTransport,
}

struct EncodedMessages {
    frames: Vec<Bytes>,
    bytes_sent: u64,
}

impl CachedCall {
    fn matches(&self, endpoint: &str, grpc: &GrpcRequest, transport: GrpcTransport) -> bool {
        self.identity.matches(endpoint, grpc, transport)
    }
}

impl CachedCallIdentity {
    fn matches(&self, endpoint: &str, grpc: &GrpcRequest, transport: GrpcTransport) -> bool {
        self.endpoint == endpoint
            && self.service == grpc.service
            && self.method_name == grpc.method
            && self.reflection == grpc.reflection
            && self.proto_files == grpc.proto_files
            && self.proto_includes == grpc.proto_includes
            && self.pool_size == grpc.channel_pool_size
            && self.transport == transport
    }
}

/// The client a request runs on, per its effective `transport`.
enum CallChannel {
    Buffered(Channel),
    Raw(RawChannel),
}

/// Per-VU client retained with the rest of the resolved call state. Keeping
/// both variants cached avoids rebuilding tonic's client facade on every call.
enum CachedClient {
    Buffered(tonic::client::Grpc<Channel>),
    Raw(tonic::client::Grpc<RawChannel>),
}

/// Global registry of shared pools, keyed by (endpoint, size).
type PoolRegistry<T> = RwLock<HashMap<(String, usize), Arc<RoundRobin<T>>>>;

/// Get-or-create the shared pool of `size` items for `endpoint` in `pools`.
/// Double-checked read→write; `build` does no I/O and no `.await` is held
/// across the lock, so building under the write lock is fine.
fn shared_pool<T: Clone>(
    pools: &PoolRegistry<T>,
    endpoint: &str,
    size: usize,
    build: impl Fn() -> Result<T, ProtocolError>,
) -> Result<Arc<RoundRobin<T>>, ProtocolError> {
    let key = (endpoint.to_string(), size);
    if let Some(pool) = pools
        .read()
        .unwrap_or_else(PoisonError::into_inner)
        .get(&key)
    {
        return Ok(pool.clone());
    }
    let mut map = pools.write().unwrap_or_else(PoisonError::into_inner);
    if let Some(pool) = map.get(&key) {
        return Ok(pool.clone());
    }
    let mut items = Vec::with_capacity(size);
    for _ in 0..size {
        items.push(build()?);
    }
    let pool = Arc::new(RoundRobin {
        items,
        next: AtomicU64::new(0),
    });
    map.insert(key, pool.clone());
    Ok(pool)
}

/// Parse a `LOADR_GRPC_TRANSPORT` value (`channel` | `raw`, case-insensitive).
fn parse_transport(value: Option<&str>) -> Option<GrpcTransport> {
    let value = value?.trim();
    if value.is_empty() {
        return None;
    }
    match value.to_ascii_lowercase().as_str() {
        "channel" => Some(GrpcTransport::Channel),
        "raw" => Some(GrpcTransport::Raw),
        other => {
            tracing::warn!("ignoring unknown LOADR_GRPC_TRANSPORT value `{other}`");
            None
        }
    }
}

// ---------------------------------------------------------------------------
// Handler
// ---------------------------------------------------------------------------

/// Dynamic gRPC protocol handler.
pub struct GrpcHandler {
    tls: Option<tonic::transport::ClientTlsConfig>,
    /// rustls config for `transport: raw` (ALPN `h2`). Unlike the tonic
    /// path it honors `insecure_skip_verify` and TLS version pinning.
    raw_tls: Arc<rustls::ClientConfig>,
    /// SNI override from `tls.server_name` (raw transport).
    server_name: Option<String>,
    /// Fleet-wide transport override from `LOADR_GRPC_TRANSPORT`.
    transport_override: Option<GrpcTransport>,
    base_dir: PathBuf,
    /// Shared channel pools. Consulted only on a VU's first pooled request
    /// per endpoint; hits are memoized per VU.
    channel_pools: PoolRegistry<Channel>,
    /// Same, for `transport: raw`.
    raw_pools: PoolRegistry<RawChannel>,
}

impl GrpcHandler {
    /// Build the handler; TLS material (for `grpcs://`) is loaded once.
    pub fn new(defaults: &HttpDefaults, base_dir: &Path) -> Result<Self, ProtocolError> {
        let tls_cfg = &defaults.tls;
        let mut tls = None;
        if tls_cfg.ca_file.is_some() || tls_cfg.cert_file.is_some() || tls_cfg.server_name.is_some()
        {
            let mut config = tonic::transport::ClientTlsConfig::new();
            if let Some(ca) = &tls_cfg.ca_file {
                let path = crate::tls::resolve_path(base_dir, ca);
                let pem = std::fs::read(&path).map_err(|e| {
                    ProtocolError::Tls(format!("cannot read ca_file {}: {e}", path.display()))
                })?;
                config = config.ca_certificate(tonic::transport::Certificate::from_pem(pem));
            }
            if let (Some(cert), Some(key)) = (&tls_cfg.cert_file, &tls_cfg.key_file) {
                let cert_pem = std::fs::read(crate::tls::resolve_path(base_dir, cert))
                    .map_err(|e| ProtocolError::Tls(format!("cannot read cert_file: {e}")))?;
                let key_pem = std::fs::read(crate::tls::resolve_path(base_dir, key))
                    .map_err(|e| ProtocolError::Tls(format!("cannot read key_file: {e}")))?;
                config = config.identity(tonic::transport::Identity::from_pem(cert_pem, key_pem));
            }
            if let Some(name) = &tls_cfg.server_name {
                config = config.domain_name(name.clone());
            }
            tls = Some(config);
        }
        if tls_cfg.insecure_skip_verify {
            tracing::warn!(
                "the grpc `channel` transport does not support insecure_skip_verify; \
                 ignored unless `transport: raw`"
            );
        }
        let raw_tls = Arc::new(crate::tls::client_config(
            tls_cfg,
            base_dir,
            vec![b"h2".to_vec()],
        )?);
        Ok(GrpcHandler {
            tls,
            raw_tls,
            server_name: tls_cfg.server_name.clone(),
            transport_override: parse_transport(
                std::env::var("LOADR_GRPC_TRANSPORT").ok().as_deref(),
            ),
            base_dir: base_dir.to_path_buf(),
            channel_pools: RwLock::new(HashMap::new()),
            raw_pools: RwLock::new(HashMap::new()),
        })
    }

    /// `grpc://` → `http://`, `grpcs://` → `https://`; strips any path.
    fn endpoint_uri(&self, raw: &str) -> Result<(String, bool), ProtocolError> {
        let url = url::Url::parse(raw)
            .map_err(|e| ProtocolError::InvalidRequest(format!("invalid url `{raw}`: {e}")))?;
        let (scheme, tls) = match url.scheme() {
            "grpc" | "http" => ("http", false),
            "grpcs" | "https" => ("https", true),
            other => {
                return Err(ProtocolError::InvalidRequest(format!(
                    "grpc handler cannot handle scheme `{other}`"
                )))
            }
        };
        let host = url
            .host_str()
            .ok_or_else(|| ProtocolError::InvalidRequest(format!("url `{raw}` has no host")))?;
        let port = url
            .port()
            .ok_or_else(|| ProtocolError::InvalidRequest(format!("url `{raw}` has no port")))?;
        Ok((format!("{scheme}://{host}:{port}"), tls))
    }

    /// Endpoint for a shared pooled channel: large fixed HTTP/2 windows (many
    /// concurrent streams share one connection) plus keepalive.
    fn pooled_endpoint(&self, endpoint: &str, tls: bool) -> Result<Endpoint, ProtocolError> {
        let mut ep = Endpoint::from_shared(endpoint.to_string())
            .map_err(|e| {
                ProtocolError::InvalidRequest(format!("invalid grpc endpoint `{endpoint}`: {e}"))
            })?
            .initial_stream_window_size(4 * 1024 * 1024)
            .initial_connection_window_size(8 * 1024 * 1024)
            .http2_keep_alive_interval(std::time::Duration::from_secs(30))
            .keep_alive_while_idle(true)
            // Many VUs share each pooled channel; widen the tower::buffer
            // request queue (tonic default 1024) so `ready()` doesn't stall
            // before the connection itself is the limit.
            .buffer_size(grpc_buffer_size());
        if tls {
            ep = ep
                .tls_config(self.tls.clone().unwrap_or_default())
                .map_err(|e| ProtocolError::Tls(format!("grpc tls config error: {e}")))?;
        }
        Ok(ep)
    }

    fn channel(
        &self,
        ctx: &mut VuContext,
        endpoint: &str,
        tls: bool,
        pool_size: Option<usize>,
    ) -> Result<Channel, ProtocolError> {
        if let Some(size) = pool_size {
            // Config validation rejects 0; clamp anyway so `% len` can never
            // divide by zero if a caller bypasses validation.
            let size = size.max(1);
            // Hot path: VU-local memo — no lock, no allocation.
            let state = ctx.extensions.get_or_insert_with(GrpcChannels::default);
            if let Some(pool) = state.pools.get(endpoint) {
                if pool.len() == size {
                    return Ok(pool.next());
                }
            }
            // First use (or size changed for this endpoint): global map.
            let pool = shared_pool(&self.channel_pools, endpoint, size, || {
                Ok(self.pooled_endpoint(endpoint, tls)?.connect_lazy())
            })?;
            let channel = pool.next();
            let state = ctx.extensions.get_or_insert_with(GrpcChannels::default);
            state.pools.insert(endpoint.to_string(), pool);
            return Ok(channel);
        }
        // ---- existing per-VU path: keep byte-for-byte as it is today ----
        let channels = ctx.extensions.get_or_insert_with(GrpcChannels::default);
        if let Some(ch) = channels.channels.get(endpoint) {
            return Ok(ch.clone());
        }
        let mut ep = Endpoint::from_shared(endpoint.to_string()).map_err(|e| {
            ProtocolError::InvalidRequest(format!("invalid grpc endpoint `{endpoint}`: {e}"))
        })?;
        if tls {
            let config = self.tls.clone().unwrap_or_default();
            ep = ep
                .tls_config(config)
                .map_err(|e| ProtocolError::Tls(format!("grpc tls config error: {e}")))?;
        }
        let channel = ep.connect_lazy();
        let channels = ctx.extensions.get_or_insert_with(GrpcChannels::default);
        channels
            .channels
            .insert(endpoint.to_string(), channel.clone());
        Ok(channel)
    }

    /// `transport: raw` counterpart of [`Self::channel`]: per-VU connection by
    /// default, shared round-robin pool when `pool_size` is set.
    fn raw(
        &self,
        ctx: &mut VuContext,
        endpoint: &str,
        tls: bool,
        pool_size: Option<usize>,
    ) -> Result<RawChannel, ProtocolError> {
        if let Some(size) = pool_size {
            let size = size.max(1);
            let state = ctx.extensions.get_or_insert_with(GrpcChannels::default);
            if let Some(pool) = state.raw_pools.get(endpoint) {
                if pool.len() == size {
                    return Ok(pool.next());
                }
            }
            let pool = shared_pool(&self.raw_pools, endpoint, size, || {
                self.raw_channel(endpoint, tls)
            })?;
            let raw = pool.next();
            let state = ctx.extensions.get_or_insert_with(GrpcChannels::default);
            state.raw_pools.insert(endpoint.to_string(), pool);
            return Ok(raw);
        }
        let state = ctx.extensions.get_or_insert_with(GrpcChannels::default);
        if let Some(raw) = state.raws.get(endpoint) {
            return Ok(raw.clone());
        }
        let raw = self.raw_channel(endpoint, tls)?;
        let state = ctx.extensions.get_or_insert_with(GrpcChannels::default);
        state.raws.insert(endpoint.to_string(), raw.clone());
        Ok(raw)
    }

    /// Build one raw channel handle for `endpoint` (no I/O; dials lazily).
    fn raw_channel(&self, endpoint: &str, tls: bool) -> Result<RawChannel, ProtocolError> {
        let tls_params = if tls {
            let url = url::Url::parse(endpoint).map_err(|e| {
                ProtocolError::InvalidRequest(format!("invalid grpc endpoint `{endpoint}`: {e}"))
            })?;
            Some(TlsParams {
                config: self.raw_tls.clone(),
                server_name: crate::tls::server_name(self.server_name.as_deref(), &url)?,
            })
        } else {
            None
        };
        RawChannel::new(endpoint, tls_params)
    }

    /// Compile `.proto` files in-process (cached globally per file set).
    fn pool_from_protos(&self, grpc: &GrpcRequest) -> Result<DescriptorPool, ProtocolError> {
        let files: Vec<PathBuf> = grpc
            .proto_files
            .iter()
            .map(|p| crate::tls::resolve_path(&self.base_dir, p))
            .collect();
        let mut includes: Vec<PathBuf> = grpc
            .proto_includes
            .iter()
            .map(|p| crate::tls::resolve_path(&self.base_dir, p))
            .collect();
        for file in &files {
            if let Some(parent) = file.parent() {
                if !includes.contains(&parent.to_path_buf()) {
                    includes.push(parent.to_path_buf());
                }
            }
        }
        if includes.is_empty() {
            includes.push(self.base_dir.clone());
        }

        let key = format!(
            "protos:{}::{}",
            files
                .iter()
                .map(|p| p.display().to_string())
                .collect::<Vec<_>>()
                .join("|"),
            includes
                .iter()
                .map(|p| p.display().to_string())
                .collect::<Vec<_>>()
                .join("|"),
        );
        if let Some(pool) = cache_get(&key) {
            return Ok(pool);
        }

        let fds = protox::compile(&files, &includes).map_err(|e| {
            ProtocolError::InvalidRequest(format!("failed to compile proto files: {e}"))
        })?;
        let pool = DescriptorPool::from_file_descriptor_set(fds).map_err(|e| {
            ProtocolError::InvalidRequest(format!("invalid file descriptor set: {e}"))
        })?;
        cache_put(key, pool.clone());
        Ok(pool)
    }

    /// Fetch descriptors via gRPC server reflection v1 (cached per
    /// endpoint+symbol). The client is generic over the transport; on a
    /// cache hit it is dropped without any I/O.
    async fn pool_from_reflection<T>(
        &self,
        mut client: ServerReflectionClient<T>,
        endpoint: &str,
        symbol: &str,
    ) -> Result<DescriptorPool, String>
    where
        T: GrpcService<tonic::body::Body>,
        T::Error: Into<StdError>,
        T::ResponseBody: HttpBody<Data = bytes::Bytes> + Send + 'static,
        <T::ResponseBody as HttpBody>::Error: Into<StdError> + Send,
    {
        let key = format!("reflection:{endpoint}::{symbol}");
        if let Some(pool) = cache_get(&key) {
            return Ok(pool);
        }

        let request = ServerReflectionRequest {
            host: String::new(),
            message_request: Some(MessageRequest::FileContainingSymbol(symbol.to_string())),
        };
        let response = client
            .server_reflection_info(futures::stream::iter([request]))
            .await
            .map_err(|e| format!("server reflection call failed: {e}"))?;
        let mut stream = response.into_inner();
        let message = stream
            .message()
            .await
            .map_err(|e| format!("server reflection stream failed: {e}"))?
            .ok_or_else(|| "server reflection returned no response".to_string())?;

        let files = match message.message_response {
            Some(MessageResponse::FileDescriptorResponse(fd)) => fd.file_descriptor_proto,
            Some(MessageResponse::ErrorResponse(err)) => {
                return Err(format!(
                    "server reflection error {}: {}",
                    err.error_code, err.error_message
                ));
            }
            _ => return Err("unexpected server reflection response".to_string()),
        };

        let mut fds = prost_types::FileDescriptorSet::default();
        for bytes in files {
            let file = prost_types::FileDescriptorProto::decode(bytes.as_slice())
                .map_err(|e| format!("invalid file descriptor from reflection: {e}"))?;
            fds.file.push(file);
        }
        let pool = DescriptorPool::from_file_descriptor_set(fds)
            .map_err(|e| format!("invalid descriptor set from reflection: {e}"))?;
        cache_put(key, pool.clone());
        Ok(pool)
    }
}

#[async_trait]
impl ProtocolHandler for GrpcHandler {
    fn name(&self) -> &str {
        "grpc"
    }

    async fn execute(
        &self,
        ctx: &mut VuContext,
        request: &PreparedRequest,
    ) -> Result<ProtocolResponse, ProtocolError> {
        let grpc = request.options.grpc.as_ref().ok_or_else(|| {
            ProtocolError::InvalidRequest("grpc request requires `grpc:` options".to_string())
        })?;
        let (endpoint, tls) = self.endpoint_uri(&request.url)?;
        let transport = self.transport_override.unwrap_or(grpc.transport);

        let start = Instant::now();

        // Resolve call state (descriptors, path, codec, client) — cached per
        // VU after the first iteration of each request shape.
        let state = ctx.extensions.get_or_insert_with(GrpcChannels::default);
        let call_idx = match state
            .calls
            .iter()
            .position(|c| c.matches(&endpoint, grpc, transport))
        {
            Some(idx) => idx,
            None => {
                let channel = match transport {
                    GrpcTransport::Channel => CallChannel::Buffered(self.channel(
                        ctx,
                        &endpoint,
                        tls,
                        grpc.channel_pool_size,
                    )?),
                    GrpcTransport::Raw => {
                        CallChannel::Raw(self.raw(ctx, &endpoint, tls, grpc.channel_pool_size)?)
                    }
                };

                // Resolve the descriptor pool.
                let pool = if grpc.reflection {
                    let reflected = async {
                        match &channel {
                            CallChannel::Buffered(channel) => {
                                self.pool_from_reflection(
                                    ServerReflectionClient::new(channel.clone()),
                                    &endpoint,
                                    &grpc.service,
                                )
                                .await
                            }
                            CallChannel::Raw(raw) => {
                                self.pool_from_reflection(
                                    ServerReflectionClient::with_origin(
                                        raw.clone(),
                                        raw.origin().clone(),
                                    ),
                                    &endpoint,
                                    &grpc.service,
                                )
                                .await
                            }
                        }
                    };
                    match tokio::time::timeout(request.timeout, reflected).await {
                        Ok(Ok(pool)) => pool,
                        Ok(Err(message)) => {
                            return Ok(grpc_error_response(message, start, &request.url));
                        }
                        Err(_) => return Ok(crate::http::timeout_response(request, start)),
                    }
                } else if !grpc.proto_files.is_empty() {
                    self.pool_from_protos(grpc)?
                } else {
                    return Err(ProtocolError::InvalidRequest(
                        "grpc request needs `proto_files` or `reflection: true`".to_string(),
                    ));
                };

                let service = pool.get_service_by_name(&grpc.service).ok_or_else(|| {
                    ProtocolError::InvalidRequest(format!("service `{}` not found", grpc.service))
                })?;
                let method = service
                    .methods()
                    .find(|m| m.name() == grpc.method)
                    .ok_or_else(|| {
                        ProtocolError::InvalidRequest(format!(
                            "method `{}` not found on `{}`",
                            grpc.method, grpc.service
                        ))
                    })?;
                let path = http::uri::PathAndQuery::try_from(format!(
                    "/{}/{}",
                    service.full_name(),
                    method.name()
                ))
                .map_err(|e| ProtocolError::InvalidRequest(format!("invalid grpc path: {e}")))?;

                let client = match channel {
                    CallChannel::Buffered(channel) => {
                        CachedClient::Buffered(tonic::client::Grpc::new(channel))
                    }
                    CallChannel::Raw(raw) => {
                        let origin = raw.origin().clone();
                        CachedClient::Raw(tonic::client::Grpc::with_origin(raw, origin))
                    }
                };

                let cached = CachedCall {
                    identity: CachedCallIdentity {
                        endpoint: endpoint.clone(),
                        service: grpc.service.clone(),
                        method_name: grpc.method.clone(),
                        reflection: grpc.reflection,
                        proto_files: grpc.proto_files.clone(),
                        proto_includes: grpc.proto_includes.clone(),
                        pool_size: grpc.channel_pool_size,
                        transport,
                    },
                    input_desc: method.input(),
                    path,
                    codec: DynamicCodec::for_method(&method),
                    shape: (method.is_client_streaming(), method.is_server_streaming()),
                    client,
                    encoded: HashMap::new(),
                };
                let state = ctx.extensions.get_or_insert_with(GrpcChannels::default);
                state.calls.push(cached);
                state.calls.len() - 1
            }
        };
        let state = ctx.extensions.get_or_insert_with(GrpcChannels::default);
        let cached = &mut state.calls[call_idx];

        // Build outbound messages: pre-encoded bytes for literal messages
        // (cached by the message Arc's identity), dynamic otherwise.
        let literal_key = if grpc.message_literal {
            if !grpc.messages.is_empty() {
                Some(Arc::as_ptr(&grpc.messages) as usize)
            } else {
                grpc.message.as_ref().map(|m| Arc::as_ptr(m) as usize)
            }
        } else {
            None
        };
        let (outbound, bytes_sent) = match literal_key {
            Some(key) => {
                if !cached.encoded.contains_key(&key) {
                    let raw: Vec<&serde_json::Value> = if !grpc.messages.is_empty() {
                        grpc.messages.iter().collect()
                    } else {
                        grpc.message.iter().map(|m| m.as_ref()).collect()
                    };
                    let mut frames = Vec::with_capacity(raw.len());
                    let mut total: u64 = 0;
                    for json in raw {
                        let message =
                            DynamicMessage::deserialize(cached.input_desc.clone(), json.clone())
                                .map_err(|e| {
                                    ProtocolError::InvalidRequest(format!(
                                        "message does not match `{}`: {e}",
                                        cached.input_desc.full_name()
                                    ))
                                })?;
                        let bytes = Bytes::from(message.encode_to_vec());
                        total += bytes.len() as u64 + 5;
                        frames.push(bytes);
                    }
                    cached.encoded.insert(
                        key,
                        EncodedMessages {
                            frames,
                            bytes_sent: total,
                        },
                    );
                }
                let enc = &cached.encoded[&key];
                let outbound: Vec<Outbound> = enc
                    .frames
                    .iter()
                    .map(|b| Outbound::Encoded(b.clone()))
                    .collect();
                (outbound, enc.bytes_sent)
            }
            None => {
                let raw: Vec<&serde_json::Value> = if !grpc.messages.is_empty() {
                    grpc.messages.iter().collect()
                } else {
                    grpc.message.iter().map(|m| m.as_ref()).collect()
                };
                let mut messages = Vec::with_capacity(raw.len().max(1));
                if raw.is_empty() {
                    messages.push(DynamicMessage::new(cached.input_desc.clone()));
                } else {
                    for json in raw {
                        let message =
                            DynamicMessage::deserialize(cached.input_desc.clone(), json.clone())
                                .map_err(|e| {
                                    ProtocolError::InvalidRequest(format!(
                                        "message does not match `{}`: {e}",
                                        cached.input_desc.full_name()
                                    ))
                                })?;
                        messages.push(message);
                    }
                }
                let bytes_sent: u64 = messages.iter().map(|m| m.encoded_len() as u64 + 5).sum();
                let outbound: Vec<Outbound> = messages.into_iter().map(Outbound::Dynamic).collect();
                (outbound, bytes_sent)
            }
        };

        let metadata = build_metadata(grpc, &request.headers)?;
        let shape = cached.shape;
        let path = cached.path.clone();
        let codec = cached.codec.clone();
        let outcome = match &mut cached.client {
            CachedClient::Buffered(client) => {
                let call = invoke(client, shape, outbound, path, codec, metadata);
                tokio::time::timeout(request.timeout, call).await
            }
            CachedClient::Raw(client) => {
                let call = invoke(client, shape, outbound, path, codec, metadata);
                tokio::time::timeout(request.timeout, call).await
            }
        };
        let outcome = match outcome {
            Ok(outcome) => outcome,
            Err(_) => return Ok(crate::http::timeout_response(request, start)),
        };

        let elapsed = ms_since(start);
        let timings = Timings {
            waiting_ms: elapsed,
            duration_ms: elapsed,
            ..Timings::default()
        };

        match outcome {
            Ok(responses) => {
                let json: Vec<serde_json::Value> = responses
                    .iter()
                    .map(|m| serde_json::to_value(m).unwrap_or(serde_json::Value::Null))
                    .collect();
                let body = json
                    .last()
                    .map(|v| serde_json::to_vec(v).unwrap_or_default())
                    .unwrap_or_default();
                let bytes_received: u64 =
                    responses.iter().map(|m| m.encoded_len() as u64 + 5).sum();
                let count = json.len();
                Ok(ProtocolResponse {
                    status: 0,
                    status_text: "OK".to_string(),
                    headers: Vec::new(),
                    body: body.into(),
                    timings,
                    bytes_sent,
                    bytes_received,
                    protocol_version: "grpc".to_string(),
                    error: None,
                    url: request.url.clone(),
                    extras: serde_json::json!({
                        "messages": json,
                        "message_count": count,
                    }),
                })
            }
            Err(status) => {
                let code = status.code();
                Ok(ProtocolResponse {
                    status: code as i64,
                    status_text: format!("{:?}: {}", code, status.message()),
                    timings,
                    bytes_sent,
                    protocol_version: "grpc".to_string(),
                    url: request.url.clone(),
                    ..ProtocolResponse::default()
                })
            }
        }
    }
}

fn grpc_error_response(message: String, start: Instant, url: &str) -> ProtocolResponse {
    let elapsed = ms_since(start);
    ProtocolResponse {
        status: 0,
        error: Some(message),
        protocol_version: "grpc".to_string(),
        url: url.to_string(),
        timings: Timings {
            duration_ms: elapsed,
            ..Timings::default()
        },
        ..ProtocolResponse::default()
    }
}

fn build_metadata(
    grpc: &GrpcRequest,
    headers: &[(String, String)],
) -> Result<tonic::metadata::MetadataMap, ProtocolError> {
    let mut map = tonic::metadata::MetadataMap::new();
    for (name, value) in grpc.metadata.iter().chain(headers.iter()) {
        let key = MetadataKey::from_bytes(name.to_ascii_lowercase().as_bytes()).map_err(|e| {
            ProtocolError::InvalidRequest(format!("invalid metadata key `{name}`: {e}"))
        })?;
        let value: MetadataValue<_> = value.parse().map_err(|e| {
            ProtocolError::InvalidRequest(format!("invalid metadata value for `{name}`: {e}"))
        })?;
        map.append(key, value);
    }
    Ok(map)
}

/// Run the call in the right shape and collect every response message.
/// Generic over the transport: `Grpc<Channel>` and `Grpc<RawChannel>`
/// monomorphize to the same code paths.
async fn invoke<T>(
    client: &mut tonic::client::Grpc<T>,
    shape: (bool, bool),
    messages: Vec<Outbound>,
    path: http::uri::PathAndQuery,
    codec: DynamicCodec,
    metadata: tonic::metadata::MetadataMap,
) -> Result<Vec<DynamicMessage>, Status>
where
    T: GrpcService<tonic::body::Body>,
    T::ResponseBody: HttpBody + Send + 'static,
    <T::ResponseBody as HttpBody>::Error: Into<StdError>,
{
    client.ready().await.map_err(|e| {
        let e: StdError = e.into();
        Status::unavailable(format!("connection failed: {e}"))
    })?;

    match shape {
        // Unary
        (false, false) => {
            let message = messages
                .into_iter()
                .next()
                .ok_or_else(|| Status::internal("missing request message"))?;
            let mut request = tonic::Request::new(message);
            *request.metadata_mut() = metadata;
            let response = client.unary(request, path, codec).await?;
            Ok(vec![response.into_inner()])
        }
        // Server streaming
        (false, true) => {
            let message = messages
                .into_iter()
                .next()
                .ok_or_else(|| Status::internal("missing request message"))?;
            let mut request = tonic::Request::new(message);
            *request.metadata_mut() = metadata;
            let response = client.server_streaming(request, path, codec).await?;
            collect_stream(response.into_inner()).await
        }
        // Client streaming
        (true, false) => {
            let mut request = tonic::Request::new(futures::stream::iter(messages));
            *request.metadata_mut() = metadata;
            let response = client.client_streaming(request, path, codec).await?;
            Ok(vec![response.into_inner()])
        }
        // Bidi streaming
        (true, true) => {
            let mut request = tonic::Request::new(futures::stream::iter(messages));
            *request.metadata_mut() = metadata;
            let response = client.streaming(request, path, codec).await?;
            collect_stream(response.into_inner()).await
        }
    }
}

async fn collect_stream(
    mut stream: tonic::Streaming<DynamicMessage>,
) -> Result<Vec<DynamicMessage>, Status> {
    let mut out = Vec::new();
    while let Some(message) = stream.message().await? {
        out.push(message);
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_transport_values() {
        assert_eq!(parse_transport(None), None);
        assert_eq!(parse_transport(Some("")), None);
        assert_eq!(parse_transport(Some("  ")), None);
        assert_eq!(parse_transport(Some("raw")), Some(GrpcTransport::Raw));
        assert_eq!(parse_transport(Some(" RAW ")), Some(GrpcTransport::Raw));
        assert_eq!(
            parse_transport(Some("Channel")),
            Some(GrpcTransport::Channel)
        );
        assert_eq!(parse_transport(Some("bogus")), None);
    }

    #[test]
    fn call_cache_identity_discriminates_transport_and_pool_size() {
        let request = GrpcRequest {
            proto_files: vec![PathBuf::from("echo.proto")],
            proto_includes: vec![PathBuf::from("protos")],
            reflection: false,
            service: "loadr.test.Echo".to_string(),
            method: "UnaryEcho".to_string(),
            channel_pool_size: Some(2),
            transport: GrpcTransport::Channel,
            ..GrpcRequest::default()
        };
        let identity = CachedCallIdentity {
            endpoint: "http://127.0.0.1:50051".to_string(),
            service: request.service.clone(),
            method_name: request.method.clone(),
            reflection: request.reflection,
            proto_files: request.proto_files.clone(),
            proto_includes: request.proto_includes.clone(),
            pool_size: request.channel_pool_size,
            transport: GrpcTransport::Channel,
        };

        assert!(identity.matches("http://127.0.0.1:50051", &request, GrpcTransport::Channel));
        assert!(!identity.matches("http://127.0.0.1:50051", &request, GrpcTransport::Raw));

        let different_pool = GrpcRequest {
            channel_pool_size: Some(4),
            ..request
        };
        assert!(!identity.matches(
            "http://127.0.0.1:50051",
            &different_pool,
            GrpcTransport::Channel
        ));
    }
}
