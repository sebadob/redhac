#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct Ack {
    #[prost(oneof = "ack::Method", tags = "1, 2, 3, 4, 5")]
    pub method: ::core::option::Option<ack::Method>,
}
/// Nested message and enum types in `Ack`.
pub mod ack {
    #[allow(clippy::derive_partial_eq_without_eq)]
    #[derive(Clone, PartialEq, ::prost::Oneof)]
    pub enum Method {
        #[prost(message, tag = "1")]
        GetAck(super::GetAck),
        #[prost(message, tag = "2")]
        PutAck(super::PutAck),
        #[prost(message, tag = "3")]
        DelAck(super::DelAck),
        #[prost(message, tag = "4")]
        MgmtAck(super::MgmtAck),
        #[prost(message, tag = "5")]
        Error(super::Error),
    }
}
/// A Cache key / value pair
/// Since it accepts any count of bytes as the value, it is generically working for any object, which can be serialized
/// manually.
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct CacheEntry {
    #[prost(string, tag = "1")]
    pub cache_name: ::prost::alloc::string::String,
    #[prost(string, tag = "2")]
    pub entry: ::prost::alloc::string::String,
    #[prost(bytes = "vec", tag = "3")]
    pub value: ::prost::alloc::vec::Vec<u8>,
}
/// The Cache Value being sent over the stream.
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct CacheRequest {
    #[prost(oneof = "cache_request::Method", tags = "1, 2, 3, 4, 5, 6")]
    pub method: ::core::option::Option<cache_request::Method>,
}
/// Nested message and enum types in `CacheRequest`.
pub mod cache_request {
    #[allow(clippy::derive_partial_eq_without_eq)]
    #[derive(Clone, PartialEq, ::prost::Oneof)]
    pub enum Method {
        #[prost(message, tag = "1")]
        Get(super::Get),
        #[prost(message, tag = "2")]
        Put(super::Put),
        #[prost(message, tag = "3")]
        Insert(super::Insert),
        #[prost(message, tag = "4")]
        Del(super::Del),
        #[prost(message, tag = "5")]
        Remove(super::Remove),
        #[prost(message, tag = "6")]
        MgmtReq(super::MgmtRequest),
    }
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct Error {
    #[prost(string, tag = "1")]
    pub error: ::prost::alloc::string::String,
}
/// GET message for a cache operation
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct Get {
    #[prost(string, tag = "1")]
    pub cache_name: ::prost::alloc::string::String,
    #[prost(string, tag = "2")]
    pub entry: ::prost::alloc::string::String,
}
/// ACK for a successful GET
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct GetAck {
    #[prost(string, tag = "1")]
    pub cache_name: ::prost::alloc::string::String,
    #[prost(string, tag = "2")]
    pub entry: ::prost::alloc::string::String,
    #[prost(bytes = "vec", optional, tag = "3")]
    pub value: ::core::option::Option<::prost::alloc::vec::Vec<u8>>,
}
/// PUT message for a cache operation
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct Put {
    #[prost(string, tag = "1")]
    pub cache_name: ::prost::alloc::string::String,
    #[prost(string, tag = "2")]
    pub entry: ::prost::alloc::string::String,
    #[prost(bytes = "vec", tag = "3")]
    pub value: ::prost::alloc::vec::Vec<u8>,
    #[prost(string, optional, tag = "4")]
    pub req_id: ::core::option::Option<::prost::alloc::string::String>,
}
/// INSERT message for a cache operation
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct Insert {
    #[prost(string, tag = "1")]
    pub cache_name: ::prost::alloc::string::String,
    #[prost(string, tag = "2")]
    pub entry: ::prost::alloc::string::String,
    #[prost(bytes = "vec", tag = "3")]
    pub value: ::prost::alloc::vec::Vec<u8>,
    #[prost(string, tag = "4")]
    pub req_id: ::prost::alloc::string::String,
    #[prost(message, optional, tag = "5")]
    pub ack_level: ::core::option::Option<AckLevel>,
}
/// ACK for a successful PUT
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct PutAck {
    #[prost(string, optional, tag = "1")]
    pub req_id: ::core::option::Option<::prost::alloc::string::String>,
    #[prost(bool, optional, tag = "2")]
    pub mod_res: ::core::option::Option<bool>,
}
/// DEL message for a cache operation
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct Del {
    #[prost(string, tag = "1")]
    pub cache_name: ::prost::alloc::string::String,
    #[prost(string, tag = "2")]
    pub entry: ::prost::alloc::string::String,
    #[prost(string, optional, tag = "3")]
    pub req_id: ::core::option::Option<::prost::alloc::string::String>,
}
/// REMOVE message for a cache operation
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct Remove {
    #[prost(string, tag = "1")]
    pub cache_name: ::prost::alloc::string::String,
    #[prost(string, tag = "2")]
    pub entry: ::prost::alloc::string::String,
    #[prost(string, tag = "3")]
    pub req_id: ::prost::alloc::string::String,
    #[prost(message, optional, tag = "4")]
    pub ack_level: ::core::option::Option<AckLevel>,
}
/// ACK for a successful DEL
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct DelAck {
    #[prost(string, optional, tag = "1")]
    pub req_id: ::core::option::Option<::prost::alloc::string::String>,
    #[prost(bool, optional, tag = "2")]
    pub mod_res: ::core::option::Option<bool>,
}
/// The AckLevel for HA cache modifying requests
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct AckLevel {
    #[prost(oneof = "ack_level::AckLevel", tags = "1, 2, 3")]
    pub ack_level: ::core::option::Option<ack_level::AckLevel>,
}
/// Nested message and enum types in `AckLevel`.
pub mod ack_level {
    #[allow(clippy::derive_partial_eq_without_eq)]
    #[derive(Clone, PartialEq, ::prost::Oneof)]
    pub enum AckLevel {
        #[prost(message, tag = "1")]
        LevelQuorum(super::Empty),
        #[prost(message, tag = "2")]
        LevelOnce(super::Empty),
        #[prost(message, tag = "3")]
        LevelLeader(super::Empty),
    }
}
/// The Cache Value being sent over the stream.
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct MgmtRequest {
    #[prost(oneof = "mgmt_request::Method", tags = "1, 2, 3, 4, 5, 6, 7, 8, 9")]
    pub method: ::core::option::Option<mgmt_request::Method>,
}
/// Nested message and enum types in `MgmtRequest`.
pub mod mgmt_request {
    #[allow(clippy::derive_partial_eq_without_eq)]
    #[derive(Clone, PartialEq, ::prost::Oneof)]
    pub enum Method {
        #[prost(message, tag = "1")]
        Ping(super::Ping),
        #[prost(message, tag = "2")]
        Health(super::Health),
        #[prost(message, tag = "3")]
        LeaderReq(super::LeaderReq),
        #[prost(message, tag = "4")]
        LeaderReqAck(super::LeaderReqAck),
        #[prost(message, tag = "5")]
        LeaderAck(super::LeaderAck),
        #[prost(message, tag = "6")]
        LeaderSwitch(super::LeaderSwitch),
        #[prost(message, tag = "7")]
        LeaderSwitchPriority(super::LeaderSwitchPriority),
        #[prost(message, tag = "8")]
        LeaderDead(super::LeaderDead),
        #[prost(message, tag = "9")]
        LeaderSwitchAck(super::LeaderSwitchAck),
    }
}
/// Just a ping with no content
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct Ping {}
/// Requests health information
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct Health {}
/// Is sent out if quorum was established and no leader is present to make a request for taking over that role
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct LeaderReq {
    /// The address where this host can be reached (multiple possible)
    #[prost(string, tag = "1")]
    pub addr: ::prost::alloc::string::String,
    /// Timestamp when the request has been created. In case of a conflict, the request with the earlier creation wins.
    #[prost(int64, tag = "2")]
    pub election_ts: i64,
}
/// Is sent out if quorum was established and 'enough' hosts have accepted the leader request
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct LeaderAck {
    /// The address where this host can be reached
    #[prost(string, tag = "1")]
    pub addr: ::prost::alloc::string::String,
    /// Timestamp when the request has been created. In case of a conflict, the request with the earlier creation wins.
    #[prost(int64, tag = "2")]
    pub election_ts: i64,
}
/// Is sent out, when the current leader is about to shut down
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct LeaderSwitch {
    /// The address of the new host that this leader votes for the next one. This creates less friction when changing.
    #[prost(string, tag = "1")]
    pub vote_host: ::prost::alloc::string::String,
    #[prost(int64, tag = "2")]
    pub election_ts: i64,
}
/// Is sent out, when there are multiple leadership requests to promote the one with the higher priority
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct LeaderSwitchPriority {
    #[prost(string, tag = "1")]
    pub vote_host: ::prost::alloc::string::String,
    #[prost(int64, tag = "2")]
    pub election_ts: i64,
}
/// Is sent out, when the leader is dead, so the sender can take over that role
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct LeaderDead {
    /// The address of the new host that this leader votes for the next one. This creates less friction when changing.
    #[prost(string, tag = "1")]
    pub vote_host: ::prost::alloc::string::String,
    #[prost(int64, tag = "2")]
    pub election_ts: i64,
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct MgmtAck {
    #[prost(oneof = "mgmt_ack::Method", tags = "1, 2, 3, 4, 5, 6")]
    pub method: ::core::option::Option<mgmt_ack::Method>,
}
/// Nested message and enum types in `MgmtAck`.
pub mod mgmt_ack {
    #[allow(clippy::derive_partial_eq_without_eq)]
    #[derive(Clone, PartialEq, ::prost::Oneof)]
    pub enum Method {
        #[prost(message, tag = "1")]
        Pong(super::Pong),
        #[prost(message, tag = "2")]
        HealthAck(super::HealthAck),
        #[prost(message, tag = "3")]
        LeaderInfo(super::LeaderInfo),
        #[prost(message, tag = "4")]
        LeaderReqAck(super::LeaderReqAck),
        /// TODO do we even need the leader_ack_ack?
        #[prost(message, tag = "5")]
        LeaderAckAck(super::LeaderAckAck),
        #[prost(message, tag = "6")]
        LeaderSwitchAck(super::LeaderSwitchAck),
    }
}
/// Just a pong with no content
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct Pong {}
/// Returns health information
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct HealthAck {
    /// uptime of this host in seconds
    #[prost(uint64, tag = "1")]
    pub uptime_secs: u64,
    /// if it has quorum or not
    #[prost(bool, tag = "2")]
    pub quorum: bool,
    /// if this host is a leader or follower
    #[prost(message, optional, tag = "3")]
    pub state: ::core::option::Option<State>,
    #[prost(message, optional, tag = "4")]
    pub leader: ::core::option::Option<Leader>,
    /// list of all configured hosts with their connection state
    #[prost(message, repeated, tag = "5")]
    pub host_health: ::prost::alloc::vec::Vec<HostHealth>,
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct State {
    #[prost(oneof = "state::Value", tags = "1, 2, 3, 4, 5, 6, 7")]
    pub value: ::core::option::Option<state::Value>,
}
/// Nested message and enum types in `State`.
pub mod state {
    #[allow(clippy::derive_partial_eq_without_eq)]
    #[derive(Clone, PartialEq, ::prost::Oneof)]
    pub enum Value {
        #[prost(message, tag = "1")]
        Leader(super::Empty),
        #[prost(message, tag = "2")]
        LeaderDead(super::Empty),
        #[prost(message, tag = "3")]
        LeaderSwitch(super::Empty),
        #[prost(message, tag = "4")]
        LeaderTxAwait(super::Empty),
        #[prost(message, tag = "5")]
        LeadershipRequested(super::Empty),
        #[prost(message, tag = "6")]
        Follower(super::Empty),
        #[prost(message, tag = "7")]
        Undefined(super::Empty),
    }
}
/// Information about the leader
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct Leader {
    #[prost(string, tag = "1")]
    pub addr: ::prost::alloc::string::String,
    #[prost(int64, tag = "2")]
    pub election_ts: i64,
    #[prost(bool, tag = "3")]
    pub connected: bool,
}
/// Information about the remote host
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct HostHealth {
    #[prost(string, tag = "1")]
    pub addr: ::prost::alloc::string::String,
    #[prost(bool, tag = "2")]
    pub connected: bool,
}
/// Will be sent out on a new client connection to inform about a possibly existing leader
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct LeaderInfo {
    #[prost(string, optional, tag = "1")]
    pub addr: ::core::option::Option<::prost::alloc::string::String>,
    #[prost(int64, tag = "2")]
    pub election_ts: i64,
    #[prost(bool, tag = "3")]
    pub has_quorum: bool,
}
/// Ack for accepting a LeaderReq
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct LeaderReqAck {
    /// The address of the acked leader
    #[prost(string, tag = "1")]
    pub addr: ::prost::alloc::string::String,
    /// The original timestamp from the LeaderReq itself
    #[prost(int64, tag = "2")]
    pub election_ts: i64,
}
/// Ack for accepting a LeaderAck
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct LeaderAckAck {
    #[prost(string, tag = "1")]
    pub addr: ::prost::alloc::string::String,
}
/// Ack for accepting a LeaderSwitch
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct LeaderSwitchAck {
    #[prost(string, tag = "1")]
    pub addr: ::prost::alloc::string::String,
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct Empty {}
/// Generated client implementations.
pub mod cache_client {
    #![allow(unused_variables, dead_code, missing_docs, clippy::let_unit_value)]
    use tonic::codegen::*;
    use tonic::codegen::http::Uri;
    /// Contains the endpoints for managing the Cache in a HA Cluster to exchange and update values remotely
    #[derive(Debug, Clone)]
    pub struct CacheClient<T> {
        inner: tonic::client::Grpc<T>,
    }
    impl CacheClient<tonic::transport::Channel> {
        /// Attempt to create a new client by connecting to a given endpoint.
        pub async fn connect<D>(dst: D) -> Result<Self, tonic::transport::Error>
        where
            D: TryInto<tonic::transport::Endpoint>,
            D::Error: Into<StdError>,
        {
            let conn = tonic::transport::Endpoint::new(dst)?.connect().await?;
            Ok(Self::new(conn))
        }
    }
    impl<T> CacheClient<T>
    where
        T: tonic::client::GrpcService<tonic::body::BoxBody>,
        T::Error: Into<StdError>,
        T::ResponseBody: Body<Data = Bytes> + Send + 'static,
        <T::ResponseBody as Body>::Error: Into<StdError> + Send,
    {
        pub fn new(inner: T) -> Self {
            let inner = tonic::client::Grpc::new(inner);
            Self { inner }
        }
        pub fn with_origin(inner: T, origin: Uri) -> Self {
            let inner = tonic::client::Grpc::with_origin(inner, origin);
            Self { inner }
        }
        pub fn with_interceptor<F>(
            inner: T,
            interceptor: F,
        ) -> CacheClient<InterceptedService<T, F>>
        where
            F: tonic::service::Interceptor,
            T::ResponseBody: Default,
            T: tonic::codegen::Service<
                http::Request<tonic::body::BoxBody>,
                Response = http::Response<
                    <T as tonic::client::GrpcService<tonic::body::BoxBody>>::ResponseBody,
                >,
            >,
            <T as tonic::codegen::Service<
                http::Request<tonic::body::BoxBody>,
            >>::Error: Into<StdError> + Send + Sync,
        {
            CacheClient::new(InterceptedService::new(inner, interceptor))
        }
        /// Compress requests with the given encoding.
        ///
        /// This requires the server to support it otherwise it might respond with an
        /// error.
        #[must_use]
        pub fn send_compressed(mut self, encoding: CompressionEncoding) -> Self {
            self.inner = self.inner.send_compressed(encoding);
            self
        }
        /// Enable decompressing responses.
        #[must_use]
        pub fn accept_compressed(mut self, encoding: CompressionEncoding) -> Self {
            self.inner = self.inner.accept_compressed(encoding);
            self
        }
        /// Limits the maximum size of a decoded message.
        ///
        /// Default: `4MB`
        #[must_use]
        pub fn max_decoding_message_size(mut self, limit: usize) -> Self {
            self.inner = self.inner.max_decoding_message_size(limit);
            self
        }
        /// Limits the maximum size of an encoded message.
        ///
        /// Default: `usize::MAX`
        #[must_use]
        pub fn max_encoding_message_size(mut self, limit: usize) -> Self {
            self.inner = self.inner.max_encoding_message_size(limit);
            self
        }
        /// Inserts / Updates a value from remote in the local caching layer
        pub async fn stream_values(
            &mut self,
            request: impl tonic::IntoStreamingRequest<Message = super::CacheRequest>,
        ) -> std::result::Result<
            tonic::Response<tonic::codec::Streaming<super::Ack>>,
            tonic::Status,
        > {
            self.inner
                .ready()
                .await
                .map_err(|e| {
                    tonic::Status::new(
                        tonic::Code::Unknown,
                        format!("Service was not ready: {}", e.into()),
                    )
                })?;
            let codec = tonic::codec::ProstCodec::default();
            let path = http::uri::PathAndQuery::from_static("/cache.Cache/StreamValues");
            let mut req = request.into_streaming_request();
            req.extensions_mut().insert(GrpcMethod::new("cache.Cache", "StreamValues"));
            self.inner.streaming(req, path, codec).await
        }
    }
}
/// Generated server implementations.
pub mod cache_server {
    #![allow(unused_variables, dead_code, missing_docs, clippy::let_unit_value)]
    use tonic::codegen::*;
    /// Generated trait containing gRPC methods that should be implemented for use with CacheServer.
    #[async_trait]
    pub trait Cache: Send + Sync + 'static {
        /// Server streaming response type for the StreamValues method.
        type StreamValuesStream: futures_core::Stream<
                Item = std::result::Result<super::Ack, tonic::Status>,
            >
            + Send
            + 'static;
        /// Inserts / Updates a value from remote in the local caching layer
        async fn stream_values(
            &self,
            request: tonic::Request<tonic::Streaming<super::CacheRequest>>,
        ) -> std::result::Result<
            tonic::Response<Self::StreamValuesStream>,
            tonic::Status,
        >;
    }
    /// Contains the endpoints for managing the Cache in a HA Cluster to exchange and update values remotely
    #[derive(Debug)]
    pub struct CacheServer<T: Cache> {
        inner: _Inner<T>,
        accept_compression_encodings: EnabledCompressionEncodings,
        send_compression_encodings: EnabledCompressionEncodings,
        max_decoding_message_size: Option<usize>,
        max_encoding_message_size: Option<usize>,
    }
    struct _Inner<T>(Arc<T>);
    impl<T: Cache> CacheServer<T> {
        pub fn new(inner: T) -> Self {
            Self::from_arc(Arc::new(inner))
        }
        pub fn from_arc(inner: Arc<T>) -> Self {
            let inner = _Inner(inner);
            Self {
                inner,
                accept_compression_encodings: Default::default(),
                send_compression_encodings: Default::default(),
                max_decoding_message_size: None,
                max_encoding_message_size: None,
            }
        }
        pub fn with_interceptor<F>(
            inner: T,
            interceptor: F,
        ) -> InterceptedService<Self, F>
        where
            F: tonic::service::Interceptor,
        {
            InterceptedService::new(Self::new(inner), interceptor)
        }
        /// Enable decompressing requests with the given encoding.
        #[must_use]
        pub fn accept_compressed(mut self, encoding: CompressionEncoding) -> Self {
            self.accept_compression_encodings.enable(encoding);
            self
        }
        /// Compress responses with the given encoding, if the client supports it.
        #[must_use]
        pub fn send_compressed(mut self, encoding: CompressionEncoding) -> Self {
            self.send_compression_encodings.enable(encoding);
            self
        }
        /// Limits the maximum size of a decoded message.
        ///
        /// Default: `4MB`
        #[must_use]
        pub fn max_decoding_message_size(mut self, limit: usize) -> Self {
            self.max_decoding_message_size = Some(limit);
            self
        }
        /// Limits the maximum size of an encoded message.
        ///
        /// Default: `usize::MAX`
        #[must_use]
        pub fn max_encoding_message_size(mut self, limit: usize) -> Self {
            self.max_encoding_message_size = Some(limit);
            self
        }
    }
    impl<T, B> tonic::codegen::Service<http::Request<B>> for CacheServer<T>
    where
        T: Cache,
        B: Body + Send + 'static,
        B::Error: Into<StdError> + Send + 'static,
    {
        type Response = http::Response<tonic::body::BoxBody>;
        type Error = std::convert::Infallible;
        type Future = BoxFuture<Self::Response, Self::Error>;
        fn poll_ready(
            &mut self,
            _cx: &mut Context<'_>,
        ) -> Poll<std::result::Result<(), Self::Error>> {
            Poll::Ready(Ok(()))
        }
        fn call(&mut self, req: http::Request<B>) -> Self::Future {
            let inner = self.inner.clone();
            match req.uri().path() {
                "/cache.Cache/StreamValues" => {
                    #[allow(non_camel_case_types)]
                    struct StreamValuesSvc<T: Cache>(pub Arc<T>);
                    impl<T: Cache> tonic::server::StreamingService<super::CacheRequest>
                    for StreamValuesSvc<T> {
                        type Response = super::Ack;
                        type ResponseStream = T::StreamValuesStream;
                        type Future = BoxFuture<
                            tonic::Response<Self::ResponseStream>,
                            tonic::Status,
                        >;
                        fn call(
                            &mut self,
                            request: tonic::Request<
                                tonic::Streaming<super::CacheRequest>,
                            >,
                        ) -> Self::Future {
                            let inner = Arc::clone(&self.0);
                            let fut = async move {
                                (*inner).stream_values(request).await
                            };
                            Box::pin(fut)
                        }
                    }
                    let accept_compression_encodings = self.accept_compression_encodings;
                    let send_compression_encodings = self.send_compression_encodings;
                    let max_decoding_message_size = self.max_decoding_message_size;
                    let max_encoding_message_size = self.max_encoding_message_size;
                    let inner = self.inner.clone();
                    let fut = async move {
                        let inner = inner.0;
                        let method = StreamValuesSvc(inner);
                        let codec = tonic::codec::ProstCodec::default();
                        let mut grpc = tonic::server::Grpc::new(codec)
                            .apply_compression_config(
                                accept_compression_encodings,
                                send_compression_encodings,
                            )
                            .apply_max_message_size_config(
                                max_decoding_message_size,
                                max_encoding_message_size,
                            );
                        let res = grpc.streaming(method, req).await;
                        Ok(res)
                    };
                    Box::pin(fut)
                }
                _ => {
                    Box::pin(async move {
                        Ok(
                            http::Response::builder()
                                .status(200)
                                .header("grpc-status", "12")
                                .header("content-type", "application/grpc")
                                .body(empty_body())
                                .unwrap(),
                        )
                    })
                }
            }
        }
    }
    impl<T: Cache> Clone for CacheServer<T> {
        fn clone(&self) -> Self {
            let inner = self.inner.clone();
            Self {
                inner,
                accept_compression_encodings: self.accept_compression_encodings,
                send_compression_encodings: self.send_compression_encodings,
                max_decoding_message_size: self.max_decoding_message_size,
                max_encoding_message_size: self.max_encoding_message_size,
            }
        }
    }
    impl<T: Cache> Clone for _Inner<T> {
        fn clone(&self) -> Self {
            Self(Arc::clone(&self.0))
        }
    }
    impl<T: std::fmt::Debug> std::fmt::Debug for _Inner<T> {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            write!(f, "{:?}", self.0)
        }
    }
    impl<T: Cache> tonic::server::NamedService for CacheServer<T> {
        const NAME: &'static str = "cache.Cache";
    }
}
