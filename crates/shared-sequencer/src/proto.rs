//! Protocol buffer files for the fuel-sequencer API.
// Automatically generated. Do not edit manually.
#![allow(missing_docs)]

pub mod cosmos_proto {
    // @generated
    /// InterfaceDescriptor describes an interface type to be used with
    /// accepts_interface and implements_interface and declared by declare_interface.
    #[allow(clippy::derive_partial_eq_without_eq)]
    #[derive(Clone, PartialEq, ::prost::Message)]
    pub struct InterfaceDescriptor {
        /// name is the name of the interface. It should be a short-name (without
        /// a period) such that the fully qualified name of the interface will be
        /// package.name, ex. for the package a.b and interface named C, the
        /// fully-qualified name will be a.b.C.
        #[prost(string, tag = "1")]
        pub name: ::prost::alloc::string::String,
        /// description is a human-readable description of the interface and its
        /// purpose.
        #[prost(string, tag = "2")]
        pub description: ::prost::alloc::string::String,
    }
    /// ScalarDescriptor describes an scalar type to be used with
    /// the scalar field option and declared by declare_scalar.
    /// Scalars extend simple protobuf built-in types with additional
    /// syntax and semantics, for instance to represent big integers.
    /// Scalars should ideally define an encoding such that there is only one
    /// valid syntactical representation for a given semantic meaning,
    /// i.e. the encoding should be deterministic.
    #[allow(clippy::derive_partial_eq_without_eq)]
    #[derive(Clone, PartialEq, ::prost::Message)]
    pub struct ScalarDescriptor {
        /// name is the name of the scalar. It should be a short-name (without
        /// a period) such that the fully qualified name of the scalar will be
        /// package.name, ex. for the package a.b and scalar named C, the
        /// fully-qualified name will be a.b.C.
        #[prost(string, tag = "1")]
        pub name: ::prost::alloc::string::String,
        /// description is a human-readable description of the scalar and its
        /// encoding format. For instance a big integer or decimal scalar should
        /// specify precisely the expected encoding format.
        #[prost(string, tag = "2")]
        pub description: ::prost::alloc::string::String,
        /// field_type is the type of field with which this scalar can be used.
        /// Scalars can be used with one and only one type of field so that
        /// encoding standards and simple and clear. Currently only string and
        /// bytes fields are supported for scalars.
        #[prost(enumeration = "ScalarType", repeated, tag = "3")]
        pub field_type: ::prost::alloc::vec::Vec<i32>,
    }
    #[derive(
        Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, ::prost::Enumeration,
    )]
    #[repr(i32)]
    pub enum ScalarType {
        Unspecified = 0,
        String = 1,
        Bytes = 2,
    }
    impl ScalarType {
        /// String value of the enum field names used in the ProtoBuf definition.
        ///
        /// The values are not transformed in any way and thus are considered stable
        /// (if the ProtoBuf definition does not change) and safe for programmatic use.
        pub fn as_str_name(&self) -> &'static str {
            match self {
                ScalarType::Unspecified => "SCALAR_TYPE_UNSPECIFIED",
                ScalarType::String => "SCALAR_TYPE_STRING",
                ScalarType::Bytes => "SCALAR_TYPE_BYTES",
            }
        }
        /// Creates an enum from field names used in the ProtoBuf definition.
        pub fn from_str_name(value: &str) -> ::core::option::Option<Self> {
            match value {
                "SCALAR_TYPE_UNSPECIFIED" => Some(Self::Unspecified),
                "SCALAR_TYPE_STRING" => Some(Self::String),
                "SCALAR_TYPE_BYTES" => Some(Self::Bytes),
                _ => None,
            }
        }
    }
    // @@protoc_insertion_point(module)
}
pub mod amino {
    // @generated
    // @@protoc_insertion_point(module)
}
pub mod fuelsequencer {
    pub mod sequencing {
        pub mod module {
            // @generated
            /// Module is the config object for the module.
            #[allow(clippy::derive_partial_eq_without_eq)]
            #[derive(Clone, PartialEq, ::prost::Message)]
            pub struct Module {
                /// authority defines the custom module authority. If not set, defaults to the
                /// governance module.
                #[prost(string, tag = "1")]
                pub authority: ::prost::alloc::string::String,
            }
            // @@protoc_insertion_point(module)
        }
        pub mod v1 {
            // @generated
            /// QueryParamsRequest is request type for the Query/Params RPC method.
            #[allow(clippy::derive_partial_eq_without_eq)]
            #[derive(Clone, PartialEq, ::prost::Message)]
            pub struct QueryParamsRequest {}
            /// QueryParamsResponse is response type for the Query/Params RPC method.
            #[allow(clippy::derive_partial_eq_without_eq)]
            #[derive(Clone, PartialEq, ::prost::Message)]
            pub struct QueryParamsResponse {
                /// params holds all the parameters of this module.
                #[prost(message, optional, tag = "1")]
                pub params: ::core::option::Option<super::Params>,
            }
            #[allow(clippy::derive_partial_eq_without_eq)]
            #[derive(Clone, PartialEq, ::prost::Message)]
            pub struct QueryGetTopicRequest {
                #[prost(bytes = "bytes", tag = "1")]
                pub id: ::prost::bytes::Bytes,
            }
            #[allow(clippy::derive_partial_eq_without_eq)]
            #[derive(Clone, PartialEq, ::prost::Message)]
            pub struct QueryGetTopicResponse {
                #[prost(message, optional, tag = "1")]
                pub topic: ::core::option::Option<super::Topic>,
            }
            #[allow(clippy::derive_partial_eq_without_eq)]
            #[derive(Clone, PartialEq, ::prost::Message)]
            pub struct QueryAllTopicRequest {
                #[prost(message, optional, tag = "1")]
                pub pagination: ::core::option::Option<
                    super::super::super::cosmos::base::query::v1beta1::PageRequest,
                >,
            }
            #[allow(clippy::derive_partial_eq_without_eq)]
            #[derive(Clone, PartialEq, ::prost::Message)]
            pub struct QueryAllTopicResponse {
                #[prost(message, repeated, tag = "1")]
                pub topic: ::prost::alloc::vec::Vec<super::Topic>,
                #[prost(message, optional, tag = "2")]
                pub pagination: ::core::option::Option<
                    super::super::super::cosmos::base::query::v1beta1::PageResponse,
                >,
            }
            /// MsgUpdateParams is the Msg/UpdateParams request type.
            #[allow(clippy::derive_partial_eq_without_eq)]
            #[derive(Clone, PartialEq, ::prost::Message)]
            pub struct MsgUpdateParams {
                /// authority is the address that controls the module (defaults to x/gov unless
                /// overwritten).
                #[prost(string, tag = "1")]
                pub authority: ::prost::alloc::string::String,
                // params defines the module parameters to update.
                /// NOTE: All parameters must be supplied.
                #[prost(message, optional, tag = "2")]
                pub params: ::core::option::Option<super::Params>,
            }
            /// MsgUpdateParamsResponse defines the response structure for executing a
            /// MsgUpdateParams message.
            #[allow(clippy::derive_partial_eq_without_eq)]
            #[derive(Clone, PartialEq, ::prost::Message)]
            pub struct MsgUpdateParamsResponse {}
            #[allow(clippy::derive_partial_eq_without_eq)]
            #[derive(Clone, PartialEq, ::prost::Message)]
            pub struct MsgPostBlob {
                /// from is the address on FuelSequencer that is posting the blob.
                #[prost(string, tag = "1")]
                pub from: ::prost::alloc::string::String,
                /// topic is the Topic that this blob belongs to, it is a 32-byte hash.
                #[prost(bytes = "bytes", tag = "2")]
                pub topic: ::prost::bytes::Bytes,
                /// order is the blob sequencer order.
                #[prost(string, tag = "3")]
                pub order: ::prost::alloc::string::String,
                /// data is the blob data bytes.
                #[prost(bytes = "bytes", tag = "4")]
                pub data: ::prost::bytes::Bytes,
            }
            #[allow(clippy::derive_partial_eq_without_eq)]
            #[derive(Clone, PartialEq, ::prost::Message)]
            pub struct MsgPostBlobResponse {
                /// nonce uniquely identifies any message that we send to Ethereum.
                #[prost(string, tag = "1")]
                pub nonce: ::prost::alloc::string::String,
                /// from is the address on FuelSequencer that is posting the blob.
                /// This address is lowercase, regardless of the one in MsgPostBlob.
                #[prost(string, tag = "2")]
                pub from: ::prost::alloc::string::String,
                /// topic is the Topic that this blob belongs to, it is a 32-byte hash.
                #[prost(bytes = "bytes", tag = "3")]
                pub topic: ::prost::bytes::Bytes,
                /// order is the blob sequencer order.
                #[prost(string, tag = "4")]
                pub order: ::prost::alloc::string::String,
                /// data is the blob data bytes.
                #[prost(bytes = "bytes", tag = "5")]
                pub data: ::prost::bytes::Bytes,
            }
            // @generated
            /// Generated client implementations.
            pub mod query_client {
                #![allow(
                    unused_variables,
                    dead_code,
                    missing_docs,
                    clippy::let_unit_value
                )]
                use tonic::codegen::{
                    http::Uri,
                    *,
                };
                #[derive(Debug, Clone)]
                pub struct QueryClient<T> {
                    inner: tonic::client::Grpc<T>,
                }
                impl QueryClient<tonic::transport::Channel> {
                    /// Attempt to create a new client by connecting to a given endpoint.
                    pub async fn connect<D>(
                        dst: D,
                    ) -> Result<Self, tonic::transport::Error>
                    where
                        D: TryInto<tonic::transport::Endpoint>,
                        D::Error: Into<StdError>,
                    {
                        let conn =
                            tonic::transport::Endpoint::new(dst)?.connect().await?;
                        Ok(Self::new(conn))
                    }
                }
                impl<T> QueryClient<T>
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
                    ) -> QueryClient<InterceptedService<T, F>>
                    where
                        F: tonic::service::Interceptor,
                        T::ResponseBody: Default,
                        T:
                            tonic::codegen::Service<
                                http::Request<tonic::body::BoxBody>,
                                Response = http::Response<
                                    <T as tonic::client::GrpcService<
                                        tonic::body::BoxBody,
                                    >>::ResponseBody,
                                >,
                            >,
                        <T as tonic::codegen::Service<
                            http::Request<tonic::body::BoxBody>,
                        >>::Error: Into<StdError> + Send + Sync,
                    {
                        QueryClient::new(InterceptedService::new(inner, interceptor))
                    }
                    /// Compress requests with the given encoding.
                    ///
                    /// This requires the server to support it otherwise it might respond with an
                    /// error.
                    #[must_use]
                    pub fn send_compressed(
                        mut self,
                        encoding: CompressionEncoding,
                    ) -> Self {
                        self.inner = self.inner.send_compressed(encoding);
                        self
                    }
                    /// Enable decompressing responses.
                    #[must_use]
                    pub fn accept_compressed(
                        mut self,
                        encoding: CompressionEncoding,
                    ) -> Self {
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
                    pub async fn params(
                        &mut self,
                        request: impl tonic::IntoRequest<super::QueryParamsRequest>,
                    ) -> std::result::Result<
                        tonic::Response<super::QueryParamsResponse>,
                        tonic::Status,
                    > {
                        self.inner.ready().await.map_err(|e| {
                            tonic::Status::new(
                                tonic::Code::Unknown,
                                format!("Service was not ready: {}", e.into()),
                            )
                        })?;
                        let codec = tonic::codec::ProstCodec::default();
                        let path = http::uri::PathAndQuery::from_static(
                            "/fuelsequencer.sequencing.v1.Query/Params",
                        );
                        let mut req = request.into_request();
                        req.extensions_mut().insert(GrpcMethod::new(
                            "fuelsequencer.sequencing.v1.Query",
                            "Params",
                        ));
                        self.inner.unary(req, path, codec).await
                    }
                    pub async fn topic(
                        &mut self,
                        request: impl tonic::IntoRequest<super::QueryGetTopicRequest>,
                    ) -> std::result::Result<
                        tonic::Response<super::QueryGetTopicResponse>,
                        tonic::Status,
                    > {
                        self.inner.ready().await.map_err(|e| {
                            tonic::Status::new(
                                tonic::Code::Unknown,
                                format!("Service was not ready: {}", e.into()),
                            )
                        })?;
                        let codec = tonic::codec::ProstCodec::default();
                        let path = http::uri::PathAndQuery::from_static(
                            "/fuelsequencer.sequencing.v1.Query/Topic",
                        );
                        let mut req = request.into_request();
                        req.extensions_mut().insert(GrpcMethod::new(
                            "fuelsequencer.sequencing.v1.Query",
                            "Topic",
                        ));
                        self.inner.unary(req, path, codec).await
                    }
                    pub async fn topic_all(
                        &mut self,
                        request: impl tonic::IntoRequest<super::QueryAllTopicRequest>,
                    ) -> std::result::Result<
                        tonic::Response<super::QueryAllTopicResponse>,
                        tonic::Status,
                    > {
                        self.inner.ready().await.map_err(|e| {
                            tonic::Status::new(
                                tonic::Code::Unknown,
                                format!("Service was not ready: {}", e.into()),
                            )
                        })?;
                        let codec = tonic::codec::ProstCodec::default();
                        let path = http::uri::PathAndQuery::from_static(
                            "/fuelsequencer.sequencing.v1.Query/TopicAll",
                        );
                        let mut req = request.into_request();
                        req.extensions_mut().insert(GrpcMethod::new(
                            "fuelsequencer.sequencing.v1.Query",
                            "TopicAll",
                        ));
                        self.inner.unary(req, path, codec).await
                    }
                }
            }
            /// Generated server implementations.
            pub mod query_server {
                #![allow(
                    unused_variables,
                    dead_code,
                    missing_docs,
                    clippy::let_unit_value
                )]
                use tonic::codegen::*;
                /// Generated trait containing gRPC methods that should be implemented for use with QueryServer.
                #[async_trait]
                pub trait Query: Send + Sync + 'static {
                    async fn params(
                        &self,
                        request: tonic::Request<super::QueryParamsRequest>,
                    ) -> std::result::Result<
                        tonic::Response<super::QueryParamsResponse>,
                        tonic::Status,
                    >;
                    async fn topic(
                        &self,
                        request: tonic::Request<super::QueryGetTopicRequest>,
                    ) -> std::result::Result<
                        tonic::Response<super::QueryGetTopicResponse>,
                        tonic::Status,
                    >;
                    async fn topic_all(
                        &self,
                        request: tonic::Request<super::QueryAllTopicRequest>,
                    ) -> std::result::Result<
                        tonic::Response<super::QueryAllTopicResponse>,
                        tonic::Status,
                    >;
                }
                #[derive(Debug)]
                pub struct QueryServer<T: Query> {
                    inner: _Inner<T>,
                    accept_compression_encodings: EnabledCompressionEncodings,
                    send_compression_encodings: EnabledCompressionEncodings,
                    max_decoding_message_size: Option<usize>,
                    max_encoding_message_size: Option<usize>,
                }
                struct _Inner<T>(Arc<T>);
                impl<T: Query> QueryServer<T> {
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
                    pub fn accept_compressed(
                        mut self,
                        encoding: CompressionEncoding,
                    ) -> Self {
                        self.accept_compression_encodings.enable(encoding);
                        self
                    }
                    /// Compress responses with the given encoding, if the client supports it.
                    #[must_use]
                    pub fn send_compressed(
                        mut self,
                        encoding: CompressionEncoding,
                    ) -> Self {
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
                impl<T, B> tonic::codegen::Service<http::Request<B>> for QueryServer<T>
                where
                    T: Query,
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
                            "/fuelsequencer.sequencing.v1.Query/Params" => {
                                #[allow(non_camel_case_types)]
                                struct ParamsSvc<T: Query>(pub Arc<T>);
                                impl<T: Query>
                                    tonic::server::UnaryService<super::QueryParamsRequest>
                                    for ParamsSvc<T>
                                {
                                    type Response = super::QueryParamsResponse;
                                    type Future = BoxFuture<
                                        tonic::Response<Self::Response>,
                                        tonic::Status,
                                    >;
                                    fn call(
                                        &mut self,
                                        request: tonic::Request<
                                            super::QueryParamsRequest,
                                        >,
                                    ) -> Self::Future
                                    {
                                        let inner = Arc::clone(&self.0);
                                        let fut = async move {
                                            <T as Query>::params(&inner, request).await
                                        };
                                        Box::pin(fut)
                                    }
                                }
                                let accept_compression_encodings =
                                    self.accept_compression_encodings;
                                let send_compression_encodings =
                                    self.send_compression_encodings;
                                let max_decoding_message_size =
                                    self.max_decoding_message_size;
                                let max_encoding_message_size =
                                    self.max_encoding_message_size;
                                let inner = self.inner.clone();
                                let fut = async move {
                                    let inner = inner.0;
                                    let method = ParamsSvc(inner);
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
                                    let res = grpc.unary(method, req).await;
                                    Ok(res)
                                };
                                Box::pin(fut)
                            }
                            "/fuelsequencer.sequencing.v1.Query/Topic" => {
                                #[allow(non_camel_case_types)]
                                struct TopicSvc<T: Query>(pub Arc<T>);
                                impl<T: Query>
                                    tonic::server::UnaryService<
                                        super::QueryGetTopicRequest,
                                    > for TopicSvc<T>
                                {
                                    type Response = super::QueryGetTopicResponse;
                                    type Future = BoxFuture<
                                        tonic::Response<Self::Response>,
                                        tonic::Status,
                                    >;
                                    fn call(
                                        &mut self,
                                        request: tonic::Request<
                                            super::QueryGetTopicRequest,
                                        >,
                                    ) -> Self::Future
                                    {
                                        let inner = Arc::clone(&self.0);
                                        let fut = async move {
                                            <T as Query>::topic(&inner, request).await
                                        };
                                        Box::pin(fut)
                                    }
                                }
                                let accept_compression_encodings =
                                    self.accept_compression_encodings;
                                let send_compression_encodings =
                                    self.send_compression_encodings;
                                let max_decoding_message_size =
                                    self.max_decoding_message_size;
                                let max_encoding_message_size =
                                    self.max_encoding_message_size;
                                let inner = self.inner.clone();
                                let fut = async move {
                                    let inner = inner.0;
                                    let method = TopicSvc(inner);
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
                                    let res = grpc.unary(method, req).await;
                                    Ok(res)
                                };
                                Box::pin(fut)
                            }
                            "/fuelsequencer.sequencing.v1.Query/TopicAll" => {
                                #[allow(non_camel_case_types)]
                                struct TopicAllSvc<T: Query>(pub Arc<T>);
                                impl<T: Query>
                                    tonic::server::UnaryService<
                                        super::QueryAllTopicRequest,
                                    > for TopicAllSvc<T>
                                {
                                    type Response = super::QueryAllTopicResponse;
                                    type Future = BoxFuture<
                                        tonic::Response<Self::Response>,
                                        tonic::Status,
                                    >;
                                    fn call(
                                        &mut self,
                                        request: tonic::Request<
                                            super::QueryAllTopicRequest,
                                        >,
                                    ) -> Self::Future
                                    {
                                        let inner = Arc::clone(&self.0);
                                        let fut = async move {
                                            <T as Query>::topic_all(&inner, request).await
                                        };
                                        Box::pin(fut)
                                    }
                                }
                                let accept_compression_encodings =
                                    self.accept_compression_encodings;
                                let send_compression_encodings =
                                    self.send_compression_encodings;
                                let max_decoding_message_size =
                                    self.max_decoding_message_size;
                                let max_encoding_message_size =
                                    self.max_encoding_message_size;
                                let inner = self.inner.clone();
                                let fut = async move {
                                    let inner = inner.0;
                                    let method = TopicAllSvc(inner);
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
                                    let res = grpc.unary(method, req).await;
                                    Ok(res)
                                };
                                Box::pin(fut)
                            }
                            _ => Box::pin(async move {
                                Ok(http::Response::builder()
                                    .status(200)
                                    .header("grpc-status", "12")
                                    .header("content-type", "application/grpc")
                                    .body(empty_body())
                                    .unwrap())
                            }),
                        }
                    }
                }
                impl<T: Query> Clone for QueryServer<T> {
                    fn clone(&self) -> Self {
                        let inner = self.inner.clone();
                        Self {
                            inner,
                            accept_compression_encodings: self
                                .accept_compression_encodings,
                            send_compression_encodings: self.send_compression_encodings,
                            max_decoding_message_size: self.max_decoding_message_size,
                            max_encoding_message_size: self.max_encoding_message_size,
                        }
                    }
                }
                impl<T: Query> Clone for _Inner<T> {
                    fn clone(&self) -> Self {
                        Self(Arc::clone(&self.0))
                    }
                }
                impl<T: std::fmt::Debug> std::fmt::Debug for _Inner<T> {
                    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                        write!(f, "{:?}", self.0)
                    }
                }
                impl<T: Query> tonic::server::NamedService for QueryServer<T> {
                    const NAME: &'static str = "fuelsequencer.sequencing.v1.Query";
                }
            }
            /// Generated client implementations.
            pub mod msg_client {
                #![allow(
                    unused_variables,
                    dead_code,
                    missing_docs,
                    clippy::let_unit_value
                )]
                use tonic::codegen::{
                    http::Uri,
                    *,
                };
                #[derive(Debug, Clone)]
                pub struct MsgClient<T> {
                    inner: tonic::client::Grpc<T>,
                }
                impl MsgClient<tonic::transport::Channel> {
                    /// Attempt to create a new client by connecting to a given endpoint.
                    pub async fn connect<D>(
                        dst: D,
                    ) -> Result<Self, tonic::transport::Error>
                    where
                        D: TryInto<tonic::transport::Endpoint>,
                        D::Error: Into<StdError>,
                    {
                        let conn =
                            tonic::transport::Endpoint::new(dst)?.connect().await?;
                        Ok(Self::new(conn))
                    }
                }
                impl<T> MsgClient<T>
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
                    ) -> MsgClient<InterceptedService<T, F>>
                    where
                        F: tonic::service::Interceptor,
                        T::ResponseBody: Default,
                        T:
                            tonic::codegen::Service<
                                http::Request<tonic::body::BoxBody>,
                                Response = http::Response<
                                    <T as tonic::client::GrpcService<
                                        tonic::body::BoxBody,
                                    >>::ResponseBody,
                                >,
                            >,
                        <T as tonic::codegen::Service<
                            http::Request<tonic::body::BoxBody>,
                        >>::Error: Into<StdError> + Send + Sync,
                    {
                        MsgClient::new(InterceptedService::new(inner, interceptor))
                    }
                    /// Compress requests with the given encoding.
                    ///
                    /// This requires the server to support it otherwise it might respond with an
                    /// error.
                    #[must_use]
                    pub fn send_compressed(
                        mut self,
                        encoding: CompressionEncoding,
                    ) -> Self {
                        self.inner = self.inner.send_compressed(encoding);
                        self
                    }
                    /// Enable decompressing responses.
                    #[must_use]
                    pub fn accept_compressed(
                        mut self,
                        encoding: CompressionEncoding,
                    ) -> Self {
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
                    pub async fn update_params(
                        &mut self,
                        request: impl tonic::IntoRequest<super::MsgUpdateParams>,
                    ) -> std::result::Result<
                        tonic::Response<super::MsgUpdateParamsResponse>,
                        tonic::Status,
                    > {
                        self.inner.ready().await.map_err(|e| {
                            tonic::Status::new(
                                tonic::Code::Unknown,
                                format!("Service was not ready: {}", e.into()),
                            )
                        })?;
                        let codec = tonic::codec::ProstCodec::default();
                        let path = http::uri::PathAndQuery::from_static(
                            "/fuelsequencer.sequencing.v1.Msg/UpdateParams",
                        );
                        let mut req = request.into_request();
                        req.extensions_mut().insert(GrpcMethod::new(
                            "fuelsequencer.sequencing.v1.Msg",
                            "UpdateParams",
                        ));
                        self.inner.unary(req, path, codec).await
                    }
                    pub async fn post_blob(
                        &mut self,
                        request: impl tonic::IntoRequest<super::MsgPostBlob>,
                    ) -> std::result::Result<
                        tonic::Response<super::MsgPostBlobResponse>,
                        tonic::Status,
                    > {
                        self.inner.ready().await.map_err(|e| {
                            tonic::Status::new(
                                tonic::Code::Unknown,
                                format!("Service was not ready: {}", e.into()),
                            )
                        })?;
                        let codec = tonic::codec::ProstCodec::default();
                        let path = http::uri::PathAndQuery::from_static(
                            "/fuelsequencer.sequencing.v1.Msg/PostBlob",
                        );
                        let mut req = request.into_request();
                        req.extensions_mut().insert(GrpcMethod::new(
                            "fuelsequencer.sequencing.v1.Msg",
                            "PostBlob",
                        ));
                        self.inner.unary(req, path, codec).await
                    }
                }
            }
            /// Generated server implementations.
            pub mod msg_server {
                #![allow(
                    unused_variables,
                    dead_code,
                    missing_docs,
                    clippy::let_unit_value
                )]
                use tonic::codegen::*;
                /// Generated trait containing gRPC methods that should be implemented for use with MsgServer.
                #[async_trait]
                pub trait Msg: Send + Sync + 'static {
                    async fn update_params(
                        &self,
                        request: tonic::Request<super::MsgUpdateParams>,
                    ) -> std::result::Result<
                        tonic::Response<super::MsgUpdateParamsResponse>,
                        tonic::Status,
                    >;
                    async fn post_blob(
                        &self,
                        request: tonic::Request<super::MsgPostBlob>,
                    ) -> std::result::Result<
                        tonic::Response<super::MsgPostBlobResponse>,
                        tonic::Status,
                    >;
                }
                #[derive(Debug)]
                pub struct MsgServer<T: Msg> {
                    inner: _Inner<T>,
                    accept_compression_encodings: EnabledCompressionEncodings,
                    send_compression_encodings: EnabledCompressionEncodings,
                    max_decoding_message_size: Option<usize>,
                    max_encoding_message_size: Option<usize>,
                }
                struct _Inner<T>(Arc<T>);
                impl<T: Msg> MsgServer<T> {
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
                    pub fn accept_compressed(
                        mut self,
                        encoding: CompressionEncoding,
                    ) -> Self {
                        self.accept_compression_encodings.enable(encoding);
                        self
                    }
                    /// Compress responses with the given encoding, if the client supports it.
                    #[must_use]
                    pub fn send_compressed(
                        mut self,
                        encoding: CompressionEncoding,
                    ) -> Self {
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
                impl<T, B> tonic::codegen::Service<http::Request<B>> for MsgServer<T>
                where
                    T: Msg,
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
                            "/fuelsequencer.sequencing.v1.Msg/UpdateParams" => {
                                #[allow(non_camel_case_types)]
                                struct UpdateParamsSvc<T: Msg>(pub Arc<T>);
                                impl<T: Msg>
                                    tonic::server::UnaryService<super::MsgUpdateParams>
                                    for UpdateParamsSvc<T>
                                {
                                    type Response = super::MsgUpdateParamsResponse;
                                    type Future = BoxFuture<
                                        tonic::Response<Self::Response>,
                                        tonic::Status,
                                    >;
                                    fn call(
                                        &mut self,
                                        request: tonic::Request<super::MsgUpdateParams>,
                                    ) -> Self::Future
                                    {
                                        let inner = Arc::clone(&self.0);
                                        let fut = async move {
                                            <T as Msg>::update_params(&inner, request)
                                                .await
                                        };
                                        Box::pin(fut)
                                    }
                                }
                                let accept_compression_encodings =
                                    self.accept_compression_encodings;
                                let send_compression_encodings =
                                    self.send_compression_encodings;
                                let max_decoding_message_size =
                                    self.max_decoding_message_size;
                                let max_encoding_message_size =
                                    self.max_encoding_message_size;
                                let inner = self.inner.clone();
                                let fut = async move {
                                    let inner = inner.0;
                                    let method = UpdateParamsSvc(inner);
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
                                    let res = grpc.unary(method, req).await;
                                    Ok(res)
                                };
                                Box::pin(fut)
                            }
                            "/fuelsequencer.sequencing.v1.Msg/PostBlob" => {
                                #[allow(non_camel_case_types)]
                                struct PostBlobSvc<T: Msg>(pub Arc<T>);
                                impl<T: Msg>
                                    tonic::server::UnaryService<super::MsgPostBlob>
                                    for PostBlobSvc<T>
                                {
                                    type Response = super::MsgPostBlobResponse;
                                    type Future = BoxFuture<
                                        tonic::Response<Self::Response>,
                                        tonic::Status,
                                    >;
                                    fn call(
                                        &mut self,
                                        request: tonic::Request<super::MsgPostBlob>,
                                    ) -> Self::Future
                                    {
                                        let inner = Arc::clone(&self.0);
                                        let fut = async move {
                                            <T as Msg>::post_blob(&inner, request).await
                                        };
                                        Box::pin(fut)
                                    }
                                }
                                let accept_compression_encodings =
                                    self.accept_compression_encodings;
                                let send_compression_encodings =
                                    self.send_compression_encodings;
                                let max_decoding_message_size =
                                    self.max_decoding_message_size;
                                let max_encoding_message_size =
                                    self.max_encoding_message_size;
                                let inner = self.inner.clone();
                                let fut = async move {
                                    let inner = inner.0;
                                    let method = PostBlobSvc(inner);
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
                                    let res = grpc.unary(method, req).await;
                                    Ok(res)
                                };
                                Box::pin(fut)
                            }
                            _ => Box::pin(async move {
                                Ok(http::Response::builder()
                                    .status(200)
                                    .header("grpc-status", "12")
                                    .header("content-type", "application/grpc")
                                    .body(empty_body())
                                    .unwrap())
                            }),
                        }
                    }
                }
                impl<T: Msg> Clone for MsgServer<T> {
                    fn clone(&self) -> Self {
                        let inner = self.inner.clone();
                        Self {
                            inner,
                            accept_compression_encodings: self
                                .accept_compression_encodings,
                            send_compression_encodings: self.send_compression_encodings,
                            max_decoding_message_size: self.max_decoding_message_size,
                            max_encoding_message_size: self.max_encoding_message_size,
                        }
                    }
                }
                impl<T: Msg> Clone for _Inner<T> {
                    fn clone(&self) -> Self {
                        Self(Arc::clone(&self.0))
                    }
                }
                impl<T: std::fmt::Debug> std::fmt::Debug for _Inner<T> {
                    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                        write!(f, "{:?}", self.0)
                    }
                }
                impl<T: Msg> tonic::server::NamedService for MsgServer<T> {
                    const NAME: &'static str = "fuelsequencer.sequencing.v1.Msg";
                }
            }

            // @@protoc_insertion_point(module)
        }
        // @generated
        /// Params defines the parameters for the module.
        #[allow(clippy::derive_partial_eq_without_eq)]
        #[derive(Clone, PartialEq, ::prost::Message)]
        pub struct Params {
            /// max_blob_size_bytes is the maximum size of blob that can be submitted in
            /// bytes.
            #[prost(uint64, tag = "1")]
            pub max_blob_size_bytes: u64,
        }
        #[allow(clippy::derive_partial_eq_without_eq)]
        #[derive(Clone, PartialEq, ::prost::Message)]
        pub struct Topic {
            /// id uniquely identifies the topic with a 32-byte hash.
            #[prost(bytes = "bytes", tag = "1")]
            pub id: ::prost::bytes::Bytes,
            /// owner is the sequencer address, who is the owner of this topic, assigned at
            /// creation.
            #[prost(string, tag = "2")]
            pub owner: ::prost::alloc::string::String,
            /// order is the sequential order of the topic blobs.
            #[prost(string, tag = "3")]
            pub order: ::prost::alloc::string::String,
        }
        /// GenesisState defines the sequencing module's genesis state.
        #[allow(clippy::derive_partial_eq_without_eq)]
        #[derive(Clone, PartialEq, ::prost::Message)]
        pub struct GenesisState {
            /// params defines all the parameters of the module.
            #[prost(message, optional, tag = "1")]
            pub params: ::core::option::Option<Params>,
            /// topicList hold all the list of topics.
            #[prost(message, repeated, tag = "2")]
            pub topic_list: ::prost::alloc::vec::Vec<Topic>,
        }
        // @@protoc_insertion_point(module)
    }
    pub mod bridge {
        pub mod v1 {
            // @generated
            /// QueryParamsRequest is request type for the Query/Params RPC method.
            #[allow(clippy::derive_partial_eq_without_eq)]
            #[derive(Clone, PartialEq, ::prost::Message)]
            pub struct QueryParamsRequest {}
            /// QueryParamsResponse is response type for the Query/Params RPC method.
            #[allow(clippy::derive_partial_eq_without_eq)]
            #[derive(Clone, PartialEq, ::prost::Message)]
            pub struct QueryParamsResponse {
                /// params holds all the parameters of this module.
                #[prost(message, optional, tag = "1")]
                pub params: ::core::option::Option<super::Params>,
            }
            #[allow(clippy::derive_partial_eq_without_eq)]
            #[derive(Clone, PartialEq, ::prost::Message)]
            pub struct QueryGetSupplyDeltaInfoRequest {}
            #[allow(clippy::derive_partial_eq_without_eq)]
            #[derive(Clone, PartialEq, ::prost::Message)]
            pub struct QueryGetSupplyDeltaInfoResponse {
                #[prost(message, optional, tag = "1")]
                pub supply_delta_info: ::core::option::Option<super::SupplyDeltaInfo>,
            }
            #[allow(clippy::derive_partial_eq_without_eq)]
            #[derive(Clone, PartialEq, ::prost::Message)]
            pub struct QueryGetLastEthereumNonceRequest {}
            #[allow(clippy::derive_partial_eq_without_eq)]
            #[derive(Clone, PartialEq, ::prost::Message)]
            pub struct QueryGetLastEthereumNonceResponse {
                #[prost(string, tag = "1")]
                pub nonce: ::prost::alloc::string::String,
            }
            #[allow(clippy::derive_partial_eq_without_eq)]
            #[derive(Clone, PartialEq, ::prost::Message)]
            pub struct QueryGetLastEthereumBlockSyncedRequest {}
            #[allow(clippy::derive_partial_eq_without_eq)]
            #[derive(Clone, PartialEq, ::prost::Message)]
            pub struct QueryGetLastEthereumBlockSyncedResponse {
                #[prost(uint64, tag = "1")]
                pub block: u64,
            }
            #[allow(clippy::derive_partial_eq_without_eq)]
            #[derive(Clone, PartialEq, ::prost::Message)]
            pub struct QueryGetEthereumEventIndexOffsetRequest {}
            #[allow(clippy::derive_partial_eq_without_eq)]
            #[derive(Clone, PartialEq, ::prost::Message)]
            pub struct QueryGetEthereumEventIndexOffsetResponse {
                #[prost(uint64, tag = "1")]
                pub offset: u64,
            }
            #[allow(clippy::derive_partial_eq_without_eq)]
            #[derive(Clone, PartialEq, ::prost::Message)]
            pub struct QuerySequencerAddressFromEthereumAddressRequest {
                #[prost(string, tag = "1")]
                pub ethereum_address: ::prost::alloc::string::String,
            }
            #[allow(clippy::derive_partial_eq_without_eq)]
            #[derive(Clone, PartialEq, ::prost::Message)]
            pub struct QuerySequencerAddressFromEthereumAddressResponse {
                #[prost(string, tag = "1")]
                pub sequencer_address: ::prost::alloc::string::String,
            }
            #[allow(clippy::derive_partial_eq_without_eq)]
            #[derive(Clone, PartialEq, ::prost::Message)]
            pub struct QueryGetLastEthBlockUpdateTimeRequest {}
            #[allow(clippy::derive_partial_eq_without_eq)]
            #[derive(Clone, PartialEq, ::prost::Message)]
            pub struct QueryGetLastEthBlockUpdateTimeResponse {
                #[prost(message, optional, tag = "1")]
                pub last_eth_block_update_time:
                    ::core::option::Option<::prost_types::Timestamp>,
            }
            /// MsgUpdateParams is the Msg/UpdateParams request type.
            #[allow(clippy::derive_partial_eq_without_eq)]
            #[derive(Clone, PartialEq, ::prost::Message)]
            pub struct MsgUpdateParams {
                /// authority is the address that controls the module (defaults to x/gov unless
                /// overwritten).
                #[prost(string, tag = "1")]
                pub authority: ::prost::alloc::string::String,
                // params defines the module parameters to update.
                /// NOTE: All parameters must be supplied.
                #[prost(message, optional, tag = "2")]
                pub params: ::core::option::Option<super::Params>,
            }
            /// MsgUpdateParamsResponse defines the response structure for executing a
            /// MsgUpdateParams message.
            #[allow(clippy::derive_partial_eq_without_eq)]
            #[derive(Clone, PartialEq, ::prost::Message)]
            pub struct MsgUpdateParamsResponse {}
            #[allow(clippy::derive_partial_eq_without_eq)]
            #[derive(Clone, PartialEq, ::prost::Message)]
            pub struct MsgSupplyDelta {
                /// authority ensures that users cannot execute this message.
                #[prost(string, tag = "1")]
                pub authority: ::prost::alloc::string::String,
            }
            #[allow(clippy::derive_partial_eq_without_eq)]
            #[derive(Clone, PartialEq, ::prost::Message)]
            pub struct MsgSupplyDeltaResponse {
                /// nonce uniquely identifies any message that we send to Ethereum.
                #[prost(string, tag = "1")]
                pub nonce: ::prost::alloc::string::String,
                /// supply_delta reports the change in the bridge token's supply due to mints
                /// and burns.
                #[prost(string, tag = "2")]
                pub supply_delta: ::prost::alloc::string::String,
            }
            #[allow(clippy::derive_partial_eq_without_eq)]
            #[derive(Clone, PartialEq, ::prost::Message)]
            pub struct MsgWithdrawToEthereum {
                /// from is the user address that is withdrawing the tokens from the Sequencer.
                /// It can be in Hex or Bech32 format.
                #[prost(string, tag = "1")]
                pub from: ::prost::alloc::string::String,
                /// to is the user address on Ethereum that will be receiving the tokens.
                #[prost(string, tag = "2")]
                pub to: ::prost::alloc::string::String,
                /// amount is the tokens being sent, which must be in the expected bridge
                /// token.
                #[prost(message, optional, tag = "3")]
                pub amount: ::core::option::Option<
                    super::super::super::cosmos::base::v1beta1::Coin,
                >,
            }
            #[allow(clippy::derive_partial_eq_without_eq)]
            #[derive(Clone, PartialEq, ::prost::Message)]
            pub struct MsgWithdrawToEthereumResponse {
                /// nonce uniquely identifies any message that we send to Ethereum.
                #[prost(string, tag = "1")]
                pub nonce: ::prost::alloc::string::String,
                /// from is the user address that is withdrawing the tokens from the Sequencer.
                /// It can be in Hex or Bech32 format, as supplied in MsgWithdrawToEthereum.
                /// This address is lowercase, regardless of the one in MsgWithdrawToEthereum.
                #[prost(string, tag = "2")]
                pub from: ::prost::alloc::string::String,
                /// to is the user address on Ethereum that will be receiving the tokens.
                /// This address is lowercase, regardless of the one in MsgWithdrawToEthereum.
                #[prost(string, tag = "3")]
                pub to: ::prost::alloc::string::String,
                /// amount is the tokens being sent, which must be the expected bridge token.
                #[prost(message, optional, tag = "4")]
                pub amount: ::core::option::Option<
                    super::super::super::cosmos::base::v1beta1::Coin,
                >,
            }
            #[allow(clippy::derive_partial_eq_without_eq)]
            #[derive(Clone, PartialEq, ::prost::Message)]
            pub struct MsgDepositFromEthereum {
                /// authority ensures that users cannot execute this message.
                #[prost(string, tag = "1")]
                pub authority: ::prost::alloc::string::String,
                /// the sending Ethereum address in hex format
                #[prost(string, tag = "2")]
                pub depositor: ::prost::alloc::string::String,
                /// recipient address in hex or bech32 format. If the recipient is the null
                /// address, the Sequencer uses the depositor address as the recipient.
                #[prost(string, tag = "3")]
                pub recipient: ::prost::alloc::string::String,
                /// the amount sent encoded as string to prevent loss of precision. Sign is
                /// also preserved
                #[prost(string, tag = "4")]
                pub amount: ::prost::alloc::string::String,
                /// vesting duration encoded in string to prevent loss of precision. Sign is
                /// also preserved. This can be zero if no duration is specified.
                #[prost(string, tag = "5")]
                pub lockup: ::prost::alloc::string::String,
            }
            #[allow(clippy::derive_partial_eq_without_eq)]
            #[derive(Clone, PartialEq, ::prost::Message)]
            pub struct MsgDepositFromEthereumResponse {}
            #[allow(clippy::derive_partial_eq_without_eq)]
            #[derive(Clone, PartialEq, ::prost::Message)]
            pub struct MsgIndex {
                /// authority ensures that users cannot execute this message.
                #[prost(string, tag = "1")]
                pub authority: ::prost::alloc::string::String,
                /// num_injected_event_txs is the number of Ethereum events injected as
                /// txs in the current block, excluding MsgSupplyDelta and MsgIndex.
                #[prost(uint64, tag = "2")]
                pub num_injected_event_txs: u64,
                /// new_ethereum_block is a boolean which indicates whether a new Ethereum
                /// block has been queried from the Sidecar and that the events from it were
                /// fully consumed by the Sequencer. This is needed to determine when
                /// LastEthereumBlockSynced should be incremented by MsgIndex. If
                /// false but the events list is not empty, the block was partially consumed.
                #[prost(bool, tag = "3")]
                pub new_ethereum_block: bool,
                /// block_number is the block that these events belong to. This is expected to
                /// be LastEthereumBlockSynced+1, since the events are from the next block.
                #[prost(uint64, tag = "4")]
                pub block_number: u64,
            }
            #[allow(clippy::derive_partial_eq_without_eq)]
            #[derive(Clone, PartialEq, ::prost::Message)]
            pub struct MsgIndexResponse {}
            // @generated
            /// Generated client implementations.
            pub mod query_client {
                #![allow(
                    unused_variables,
                    dead_code,
                    missing_docs,
                    clippy::let_unit_value
                )]
                use tonic::codegen::{
                    http::Uri,
                    *,
                };
                #[derive(Debug, Clone)]
                pub struct QueryClient<T> {
                    inner: tonic::client::Grpc<T>,
                }
                impl QueryClient<tonic::transport::Channel> {
                    /// Attempt to create a new client by connecting to a given endpoint.
                    pub async fn connect<D>(
                        dst: D,
                    ) -> Result<Self, tonic::transport::Error>
                    where
                        D: TryInto<tonic::transport::Endpoint>,
                        D::Error: Into<StdError>,
                    {
                        let conn =
                            tonic::transport::Endpoint::new(dst)?.connect().await?;
                        Ok(Self::new(conn))
                    }
                }
                impl<T> QueryClient<T>
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
                    ) -> QueryClient<InterceptedService<T, F>>
                    where
                        F: tonic::service::Interceptor,
                        T::ResponseBody: Default,
                        T:
                            tonic::codegen::Service<
                                http::Request<tonic::body::BoxBody>,
                                Response = http::Response<
                                    <T as tonic::client::GrpcService<
                                        tonic::body::BoxBody,
                                    >>::ResponseBody,
                                >,
                            >,
                        <T as tonic::codegen::Service<
                            http::Request<tonic::body::BoxBody>,
                        >>::Error: Into<StdError> + Send + Sync,
                    {
                        QueryClient::new(InterceptedService::new(inner, interceptor))
                    }
                    /// Compress requests with the given encoding.
                    ///
                    /// This requires the server to support it otherwise it might respond with an
                    /// error.
                    #[must_use]
                    pub fn send_compressed(
                        mut self,
                        encoding: CompressionEncoding,
                    ) -> Self {
                        self.inner = self.inner.send_compressed(encoding);
                        self
                    }
                    /// Enable decompressing responses.
                    #[must_use]
                    pub fn accept_compressed(
                        mut self,
                        encoding: CompressionEncoding,
                    ) -> Self {
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
                    pub async fn params(
                        &mut self,
                        request: impl tonic::IntoRequest<super::QueryParamsRequest>,
                    ) -> std::result::Result<
                        tonic::Response<super::QueryParamsResponse>,
                        tonic::Status,
                    > {
                        self.inner.ready().await.map_err(|e| {
                            tonic::Status::new(
                                tonic::Code::Unknown,
                                format!("Service was not ready: {}", e.into()),
                            )
                        })?;
                        let codec = tonic::codec::ProstCodec::default();
                        let path = http::uri::PathAndQuery::from_static(
                            "/fuelsequencer.bridge.v1.Query/Params",
                        );
                        let mut req = request.into_request();
                        req.extensions_mut().insert(GrpcMethod::new(
                            "fuelsequencer.bridge.v1.Query",
                            "Params",
                        ));
                        self.inner.unary(req, path, codec).await
                    }
                    pub async fn last_ethereum_nonce(
                        &mut self,
                        request: impl tonic::IntoRequest<
                            super::QueryGetLastEthereumNonceRequest,
                        >,
                    ) -> std::result::Result<
                        tonic::Response<super::QueryGetLastEthereumNonceResponse>,
                        tonic::Status,
                    > {
                        self.inner.ready().await.map_err(|e| {
                            tonic::Status::new(
                                tonic::Code::Unknown,
                                format!("Service was not ready: {}", e.into()),
                            )
                        })?;
                        let codec = tonic::codec::ProstCodec::default();
                        let path = http::uri::PathAndQuery::from_static(
                            "/fuelsequencer.bridge.v1.Query/LastEthereumNonce",
                        );
                        let mut req = request.into_request();
                        req.extensions_mut().insert(GrpcMethod::new(
                            "fuelsequencer.bridge.v1.Query",
                            "LastEthereumNonce",
                        ));
                        self.inner.unary(req, path, codec).await
                    }
                    pub async fn last_ethereum_block_synced(
                        &mut self,
                        request: impl tonic::IntoRequest<
                            super::QueryGetLastEthereumBlockSyncedRequest,
                        >,
                    ) -> std::result::Result<
                        tonic::Response<super::QueryGetLastEthereumBlockSyncedResponse>,
                        tonic::Status,
                    > {
                        self.inner.ready().await.map_err(|e| {
                            tonic::Status::new(
                                tonic::Code::Unknown,
                                format!("Service was not ready: {}", e.into()),
                            )
                        })?;
                        let codec = tonic::codec::ProstCodec::default();
                        let path = http::uri::PathAndQuery::from_static(
                            "/fuelsequencer.bridge.v1.Query/LastEthereumBlockSynced",
                        );
                        let mut req = request.into_request();
                        req.extensions_mut().insert(GrpcMethod::new(
                            "fuelsequencer.bridge.v1.Query",
                            "LastEthereumBlockSynced",
                        ));
                        self.inner.unary(req, path, codec).await
                    }
                    pub async fn ethereum_event_index_offset(
                        &mut self,
                        request: impl tonic::IntoRequest<
                            super::QueryGetEthereumEventIndexOffsetRequest,
                        >,
                    ) -> std::result::Result<
                        tonic::Response<super::QueryGetEthereumEventIndexOffsetResponse>,
                        tonic::Status,
                    > {
                        self.inner.ready().await.map_err(|e| {
                            tonic::Status::new(
                                tonic::Code::Unknown,
                                format!("Service was not ready: {}", e.into()),
                            )
                        })?;
                        let codec = tonic::codec::ProstCodec::default();
                        let path = http::uri::PathAndQuery::from_static(
                            "/fuelsequencer.bridge.v1.Query/EthereumEventIndexOffset",
                        );
                        let mut req = request.into_request();
                        req.extensions_mut().insert(GrpcMethod::new(
                            "fuelsequencer.bridge.v1.Query",
                            "EthereumEventIndexOffset",
                        ));
                        self.inner.unary(req, path, codec).await
                    }
                    pub async fn supply_delta_info(
                        &mut self,
                        request: impl tonic::IntoRequest<
                            super::QueryGetSupplyDeltaInfoRequest,
                        >,
                    ) -> std::result::Result<
                        tonic::Response<super::QueryGetSupplyDeltaInfoResponse>,
                        tonic::Status,
                    > {
                        self.inner.ready().await.map_err(|e| {
                            tonic::Status::new(
                                tonic::Code::Unknown,
                                format!("Service was not ready: {}", e.into()),
                            )
                        })?;
                        let codec = tonic::codec::ProstCodec::default();
                        let path = http::uri::PathAndQuery::from_static(
                            "/fuelsequencer.bridge.v1.Query/SupplyDeltaInfo",
                        );
                        let mut req = request.into_request();
                        req.extensions_mut().insert(GrpcMethod::new(
                            "fuelsequencer.bridge.v1.Query",
                            "SupplyDeltaInfo",
                        ));
                        self.inner.unary(req, path, codec).await
                    }
                    pub async fn sequencer_address_from_ethereum_address(
                        &mut self,
                        request: impl tonic::IntoRequest<
                            super::QuerySequencerAddressFromEthereumAddressRequest,
                        >,
                    ) -> std::result::Result<
                        tonic::Response<
                            super::QuerySequencerAddressFromEthereumAddressResponse,
                        >,
                        tonic::Status,
                    > {
                        self.inner.ready().await.map_err(|e| {
                            tonic::Status::new(
                                tonic::Code::Unknown,
                                format!("Service was not ready: {}", e.into()),
                            )
                        })?;
                        let codec = tonic::codec::ProstCodec::default();
                        let path = http::uri::PathAndQuery::from_static(
                "/fuelsequencer.bridge.v1.Query/SequencerAddressFromEthereumAddress",
            );
                        let mut req = request.into_request();
                        req.extensions_mut().insert(GrpcMethod::new(
                            "fuelsequencer.bridge.v1.Query",
                            "SequencerAddressFromEthereumAddress",
                        ));
                        self.inner.unary(req, path, codec).await
                    }
                    pub async fn last_eth_block_update_time(
                        &mut self,
                        request: impl tonic::IntoRequest<
                            super::QueryGetLastEthBlockUpdateTimeRequest,
                        >,
                    ) -> std::result::Result<
                        tonic::Response<super::QueryGetLastEthBlockUpdateTimeResponse>,
                        tonic::Status,
                    > {
                        self.inner.ready().await.map_err(|e| {
                            tonic::Status::new(
                                tonic::Code::Unknown,
                                format!("Service was not ready: {}", e.into()),
                            )
                        })?;
                        let codec = tonic::codec::ProstCodec::default();
                        let path = http::uri::PathAndQuery::from_static(
                            "/fuelsequencer.bridge.v1.Query/LastEthBlockUpdateTime",
                        );
                        let mut req = request.into_request();
                        req.extensions_mut().insert(GrpcMethod::new(
                            "fuelsequencer.bridge.v1.Query",
                            "LastEthBlockUpdateTime",
                        ));
                        self.inner.unary(req, path, codec).await
                    }
                }
            }
            /// Generated server implementations.
            pub mod query_server {
                #![allow(
                    unused_variables,
                    dead_code,
                    missing_docs,
                    clippy::let_unit_value
                )]
                use tonic::codegen::*;
                /// Generated trait containing gRPC methods that should be implemented for use with QueryServer.
                #[async_trait]
                pub trait Query: Send + Sync + 'static {
                    async fn params(
                        &self,
                        request: tonic::Request<super::QueryParamsRequest>,
                    ) -> std::result::Result<
                        tonic::Response<super::QueryParamsResponse>,
                        tonic::Status,
                    >;
                    async fn last_ethereum_nonce(
                        &self,
                        request: tonic::Request<super::QueryGetLastEthereumNonceRequest>,
                    ) -> std::result::Result<
                        tonic::Response<super::QueryGetLastEthereumNonceResponse>,
                        tonic::Status,
                    >;
                    async fn last_ethereum_block_synced(
                        &self,
                        request: tonic::Request<
                            super::QueryGetLastEthereumBlockSyncedRequest,
                        >,
                    ) -> std::result::Result<
                        tonic::Response<super::QueryGetLastEthereumBlockSyncedResponse>,
                        tonic::Status,
                    >;
                    async fn ethereum_event_index_offset(
                        &self,
                        request: tonic::Request<
                            super::QueryGetEthereumEventIndexOffsetRequest,
                        >,
                    ) -> std::result::Result<
                        tonic::Response<super::QueryGetEthereumEventIndexOffsetResponse>,
                        tonic::Status,
                    >;
                    async fn supply_delta_info(
                        &self,
                        request: tonic::Request<super::QueryGetSupplyDeltaInfoRequest>,
                    ) -> std::result::Result<
                        tonic::Response<super::QueryGetSupplyDeltaInfoResponse>,
                        tonic::Status,
                    >;
                    async fn sequencer_address_from_ethereum_address(
                        &self,
                        request: tonic::Request<
                            super::QuerySequencerAddressFromEthereumAddressRequest,
                        >,
                    ) -> std::result::Result<
                        tonic::Response<
                            super::QuerySequencerAddressFromEthereumAddressResponse,
                        >,
                        tonic::Status,
                    >;
                    async fn last_eth_block_update_time(
                        &self,
                        request: tonic::Request<
                            super::QueryGetLastEthBlockUpdateTimeRequest,
                        >,
                    ) -> std::result::Result<
                        tonic::Response<super::QueryGetLastEthBlockUpdateTimeResponse>,
                        tonic::Status,
                    >;
                }
                #[derive(Debug)]
                pub struct QueryServer<T: Query> {
                    inner: _Inner<T>,
                    accept_compression_encodings: EnabledCompressionEncodings,
                    send_compression_encodings: EnabledCompressionEncodings,
                    max_decoding_message_size: Option<usize>,
                    max_encoding_message_size: Option<usize>,
                }
                struct _Inner<T>(Arc<T>);
                impl<T: Query> QueryServer<T> {
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
                    pub fn accept_compressed(
                        mut self,
                        encoding: CompressionEncoding,
                    ) -> Self {
                        self.accept_compression_encodings.enable(encoding);
                        self
                    }
                    /// Compress responses with the given encoding, if the client supports it.
                    #[must_use]
                    pub fn send_compressed(
                        mut self,
                        encoding: CompressionEncoding,
                    ) -> Self {
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
                impl<T, B> tonic::codegen::Service<http::Request<B>> for QueryServer<T>
                where
                    T: Query,
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
                "/fuelsequencer.bridge.v1.Query/Params" => {
                    #[allow(non_camel_case_types)]
                    struct ParamsSvc<T: Query>(pub Arc<T>);
                    impl<T: Query> tonic::server::UnaryService<super::QueryParamsRequest> for ParamsSvc<T> {
                        type Response = super::QueryParamsResponse;
                        type Future = BoxFuture<tonic::Response<Self::Response>, tonic::Status>;
                        fn call(
                            &mut self,
                            request: tonic::Request<super::QueryParamsRequest>,
                        ) -> Self::Future {
                            let inner = Arc::clone(&self.0);
                            let fut = async move { <T as Query>::params(&inner, request).await };
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
                        let method = ParamsSvc(inner);
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
                        let res = grpc.unary(method, req).await;
                        Ok(res)
                    };
                    Box::pin(fut)
                }
                "/fuelsequencer.bridge.v1.Query/LastEthereumNonce" => {
                    #[allow(non_camel_case_types)]
                    struct LastEthereumNonceSvc<T: Query>(pub Arc<T>);
                    impl<T: Query>
                        tonic::server::UnaryService<super::QueryGetLastEthereumNonceRequest>
                        for LastEthereumNonceSvc<T>
                    {
                        type Response = super::QueryGetLastEthereumNonceResponse;
                        type Future = BoxFuture<tonic::Response<Self::Response>, tonic::Status>;
                        fn call(
                            &mut self,
                            request: tonic::Request<super::QueryGetLastEthereumNonceRequest>,
                        ) -> Self::Future {
                            let inner = Arc::clone(&self.0);
                            let fut = async move {
                                <T as Query>::last_ethereum_nonce(&inner, request).await
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
                        let method = LastEthereumNonceSvc(inner);
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
                        let res = grpc.unary(method, req).await;
                        Ok(res)
                    };
                    Box::pin(fut)
                }
                "/fuelsequencer.bridge.v1.Query/LastEthereumBlockSynced" => {
                    #[allow(non_camel_case_types)]
                    struct LastEthereumBlockSyncedSvc<T: Query>(pub Arc<T>);
                    impl<T: Query>
                        tonic::server::UnaryService<super::QueryGetLastEthereumBlockSyncedRequest>
                        for LastEthereumBlockSyncedSvc<T>
                    {
                        type Response = super::QueryGetLastEthereumBlockSyncedResponse;
                        type Future = BoxFuture<tonic::Response<Self::Response>, tonic::Status>;
                        fn call(
                            &mut self,
                            request: tonic::Request<super::QueryGetLastEthereumBlockSyncedRequest>,
                        ) -> Self::Future {
                            let inner = Arc::clone(&self.0);
                            let fut = async move {
                                <T as Query>::last_ethereum_block_synced(&inner, request).await
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
                        let method = LastEthereumBlockSyncedSvc(inner);
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
                        let res = grpc.unary(method, req).await;
                        Ok(res)
                    };
                    Box::pin(fut)
                }
                "/fuelsequencer.bridge.v1.Query/EthereumEventIndexOffset" => {
                    #[allow(non_camel_case_types)]
                    struct EthereumEventIndexOffsetSvc<T: Query>(pub Arc<T>);
                    impl<T: Query>
                        tonic::server::UnaryService<super::QueryGetEthereumEventIndexOffsetRequest>
                        for EthereumEventIndexOffsetSvc<T>
                    {
                        type Response = super::QueryGetEthereumEventIndexOffsetResponse;
                        type Future = BoxFuture<tonic::Response<Self::Response>, tonic::Status>;
                        fn call(
                            &mut self,
                            request: tonic::Request<super::QueryGetEthereumEventIndexOffsetRequest>,
                        ) -> Self::Future {
                            let inner = Arc::clone(&self.0);
                            let fut = async move {
                                <T as Query>::ethereum_event_index_offset(&inner, request).await
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
                        let method = EthereumEventIndexOffsetSvc(inner);
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
                        let res = grpc.unary(method, req).await;
                        Ok(res)
                    };
                    Box::pin(fut)
                }
                "/fuelsequencer.bridge.v1.Query/SupplyDeltaInfo" => {
                    #[allow(non_camel_case_types)]
                    struct SupplyDeltaInfoSvc<T: Query>(pub Arc<T>);
                    impl<T: Query>
                        tonic::server::UnaryService<super::QueryGetSupplyDeltaInfoRequest>
                        for SupplyDeltaInfoSvc<T>
                    {
                        type Response = super::QueryGetSupplyDeltaInfoResponse;
                        type Future = BoxFuture<tonic::Response<Self::Response>, tonic::Status>;
                        fn call(
                            &mut self,
                            request: tonic::Request<super::QueryGetSupplyDeltaInfoRequest>,
                        ) -> Self::Future {
                            let inner = Arc::clone(&self.0);
                            let fut = async move {
                                <T as Query>::supply_delta_info(&inner, request).await
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
                        let method = SupplyDeltaInfoSvc(inner);
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
                        let res = grpc.unary(method, req).await;
                        Ok(res)
                    };
                    Box::pin(fut)
                }
                "/fuelsequencer.bridge.v1.Query/SequencerAddressFromEthereumAddress" => {
                    #[allow(non_camel_case_types)]
                    struct SequencerAddressFromEthereumAddressSvc<T: Query>(pub Arc<T>);
                    impl<T: Query>
                        tonic::server::UnaryService<
                            super::QuerySequencerAddressFromEthereumAddressRequest,
                        > for SequencerAddressFromEthereumAddressSvc<T>
                    {
                        type Response = super::QuerySequencerAddressFromEthereumAddressResponse;
                        type Future = BoxFuture<tonic::Response<Self::Response>, tonic::Status>;
                        fn call(
                            &mut self,
                            request: tonic::Request<
                                super::QuerySequencerAddressFromEthereumAddressRequest,
                            >,
                        ) -> Self::Future {
                            let inner = Arc::clone(&self.0);
                            let fut = async move {
                                <T as Query>::sequencer_address_from_ethereum_address(
                                    &inner, request,
                                )
                                .await
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
                        let method = SequencerAddressFromEthereumAddressSvc(inner);
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
                        let res = grpc.unary(method, req).await;
                        Ok(res)
                    };
                    Box::pin(fut)
                }
                "/fuelsequencer.bridge.v1.Query/LastEthBlockUpdateTime" => {
                    #[allow(non_camel_case_types)]
                    struct LastEthBlockUpdateTimeSvc<T: Query>(pub Arc<T>);
                    impl<T: Query>
                        tonic::server::UnaryService<super::QueryGetLastEthBlockUpdateTimeRequest>
                        for LastEthBlockUpdateTimeSvc<T>
                    {
                        type Response = super::QueryGetLastEthBlockUpdateTimeResponse;
                        type Future = BoxFuture<tonic::Response<Self::Response>, tonic::Status>;
                        fn call(
                            &mut self,
                            request: tonic::Request<super::QueryGetLastEthBlockUpdateTimeRequest>,
                        ) -> Self::Future {
                            let inner = Arc::clone(&self.0);
                            let fut = async move {
                                <T as Query>::last_eth_block_update_time(&inner, request).await
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
                        let method = LastEthBlockUpdateTimeSvc(inner);
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
                        let res = grpc.unary(method, req).await;
                        Ok(res)
                    };
                    Box::pin(fut)
                }
                _ => Box::pin(async move {
                    Ok(http::Response::builder()
                        .status(200)
                        .header("grpc-status", "12")
                        .header("content-type", "application/grpc")
                        .body(empty_body())
                        .unwrap())
                }),
            }
                    }
                }
                impl<T: Query> Clone for QueryServer<T> {
                    fn clone(&self) -> Self {
                        let inner = self.inner.clone();
                        Self {
                            inner,
                            accept_compression_encodings: self
                                .accept_compression_encodings,
                            send_compression_encodings: self.send_compression_encodings,
                            max_decoding_message_size: self.max_decoding_message_size,
                            max_encoding_message_size: self.max_encoding_message_size,
                        }
                    }
                }
                impl<T: Query> Clone for _Inner<T> {
                    fn clone(&self) -> Self {
                        Self(Arc::clone(&self.0))
                    }
                }
                impl<T: std::fmt::Debug> std::fmt::Debug for _Inner<T> {
                    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                        write!(f, "{:?}", self.0)
                    }
                }
                impl<T: Query> tonic::server::NamedService for QueryServer<T> {
                    const NAME: &'static str = "fuelsequencer.bridge.v1.Query";
                }
            }
            /// Generated client implementations.
            pub mod msg_client {
                #![allow(
                    unused_variables,
                    dead_code,
                    missing_docs,
                    clippy::let_unit_value
                )]
                use tonic::codegen::{
                    http::Uri,
                    *,
                };
                #[derive(Debug, Clone)]
                pub struct MsgClient<T> {
                    inner: tonic::client::Grpc<T>,
                }
                impl MsgClient<tonic::transport::Channel> {
                    /// Attempt to create a new client by connecting to a given endpoint.
                    pub async fn connect<D>(
                        dst: D,
                    ) -> Result<Self, tonic::transport::Error>
                    where
                        D: TryInto<tonic::transport::Endpoint>,
                        D::Error: Into<StdError>,
                    {
                        let conn =
                            tonic::transport::Endpoint::new(dst)?.connect().await?;
                        Ok(Self::new(conn))
                    }
                }
                impl<T> MsgClient<T>
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
                    ) -> MsgClient<InterceptedService<T, F>>
                    where
                        F: tonic::service::Interceptor,
                        T::ResponseBody: Default,
                        T:
                            tonic::codegen::Service<
                                http::Request<tonic::body::BoxBody>,
                                Response = http::Response<
                                    <T as tonic::client::GrpcService<
                                        tonic::body::BoxBody,
                                    >>::ResponseBody,
                                >,
                            >,
                        <T as tonic::codegen::Service<
                            http::Request<tonic::body::BoxBody>,
                        >>::Error: Into<StdError> + Send + Sync,
                    {
                        MsgClient::new(InterceptedService::new(inner, interceptor))
                    }
                    /// Compress requests with the given encoding.
                    ///
                    /// This requires the server to support it otherwise it might respond with an
                    /// error.
                    #[must_use]
                    pub fn send_compressed(
                        mut self,
                        encoding: CompressionEncoding,
                    ) -> Self {
                        self.inner = self.inner.send_compressed(encoding);
                        self
                    }
                    /// Enable decompressing responses.
                    #[must_use]
                    pub fn accept_compressed(
                        mut self,
                        encoding: CompressionEncoding,
                    ) -> Self {
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
                    pub async fn update_params(
                        &mut self,
                        request: impl tonic::IntoRequest<super::MsgUpdateParams>,
                    ) -> std::result::Result<
                        tonic::Response<super::MsgUpdateParamsResponse>,
                        tonic::Status,
                    > {
                        self.inner.ready().await.map_err(|e| {
                            tonic::Status::new(
                                tonic::Code::Unknown,
                                format!("Service was not ready: {}", e.into()),
                            )
                        })?;
                        let codec = tonic::codec::ProstCodec::default();
                        let path = http::uri::PathAndQuery::from_static(
                            "/fuelsequencer.bridge.v1.Msg/UpdateParams",
                        );
                        let mut req = request.into_request();
                        req.extensions_mut().insert(GrpcMethod::new(
                            "fuelsequencer.bridge.v1.Msg",
                            "UpdateParams",
                        ));
                        self.inner.unary(req, path, codec).await
                    }
                    pub async fn supply_delta(
                        &mut self,
                        request: impl tonic::IntoRequest<super::MsgSupplyDelta>,
                    ) -> std::result::Result<
                        tonic::Response<super::MsgSupplyDeltaResponse>,
                        tonic::Status,
                    > {
                        self.inner.ready().await.map_err(|e| {
                            tonic::Status::new(
                                tonic::Code::Unknown,
                                format!("Service was not ready: {}", e.into()),
                            )
                        })?;
                        let codec = tonic::codec::ProstCodec::default();
                        let path = http::uri::PathAndQuery::from_static(
                            "/fuelsequencer.bridge.v1.Msg/SupplyDelta",
                        );
                        let mut req = request.into_request();
                        req.extensions_mut().insert(GrpcMethod::new(
                            "fuelsequencer.bridge.v1.Msg",
                            "SupplyDelta",
                        ));
                        self.inner.unary(req, path, codec).await
                    }
                    pub async fn withdraw_to_ethereum(
                        &mut self,
                        request: impl tonic::IntoRequest<super::MsgWithdrawToEthereum>,
                    ) -> std::result::Result<
                        tonic::Response<super::MsgWithdrawToEthereumResponse>,
                        tonic::Status,
                    > {
                        self.inner.ready().await.map_err(|e| {
                            tonic::Status::new(
                                tonic::Code::Unknown,
                                format!("Service was not ready: {}", e.into()),
                            )
                        })?;
                        let codec = tonic::codec::ProstCodec::default();
                        let path = http::uri::PathAndQuery::from_static(
                            "/fuelsequencer.bridge.v1.Msg/WithdrawToEthereum",
                        );
                        let mut req = request.into_request();
                        req.extensions_mut().insert(GrpcMethod::new(
                            "fuelsequencer.bridge.v1.Msg",
                            "WithdrawToEthereum",
                        ));
                        self.inner.unary(req, path, codec).await
                    }
                    pub async fn deposit_from_ethereum(
                        &mut self,
                        request: impl tonic::IntoRequest<super::MsgDepositFromEthereum>,
                    ) -> std::result::Result<
                        tonic::Response<super::MsgDepositFromEthereumResponse>,
                        tonic::Status,
                    > {
                        self.inner.ready().await.map_err(|e| {
                            tonic::Status::new(
                                tonic::Code::Unknown,
                                format!("Service was not ready: {}", e.into()),
                            )
                        })?;
                        let codec = tonic::codec::ProstCodec::default();
                        let path = http::uri::PathAndQuery::from_static(
                            "/fuelsequencer.bridge.v1.Msg/DepositFromEthereum",
                        );
                        let mut req = request.into_request();
                        req.extensions_mut().insert(GrpcMethod::new(
                            "fuelsequencer.bridge.v1.Msg",
                            "DepositFromEthereum",
                        ));
                        self.inner.unary(req, path, codec).await
                    }
                    pub async fn index(
                        &mut self,
                        request: impl tonic::IntoRequest<super::MsgIndex>,
                    ) -> std::result::Result<
                        tonic::Response<super::MsgIndexResponse>,
                        tonic::Status,
                    > {
                        self.inner.ready().await.map_err(|e| {
                            tonic::Status::new(
                                tonic::Code::Unknown,
                                format!("Service was not ready: {}", e.into()),
                            )
                        })?;
                        let codec = tonic::codec::ProstCodec::default();
                        let path = http::uri::PathAndQuery::from_static(
                            "/fuelsequencer.bridge.v1.Msg/Index",
                        );
                        let mut req = request.into_request();
                        req.extensions_mut().insert(GrpcMethod::new(
                            "fuelsequencer.bridge.v1.Msg",
                            "Index",
                        ));
                        self.inner.unary(req, path, codec).await
                    }
                }
            }
            /// Generated server implementations.
            pub mod msg_server {
                #![allow(
                    unused_variables,
                    dead_code,
                    missing_docs,
                    clippy::let_unit_value
                )]
                use tonic::codegen::*;
                /// Generated trait containing gRPC methods that should be implemented for use with MsgServer.
                #[async_trait]
                pub trait Msg: Send + Sync + 'static {
                    async fn update_params(
                        &self,
                        request: tonic::Request<super::MsgUpdateParams>,
                    ) -> std::result::Result<
                        tonic::Response<super::MsgUpdateParamsResponse>,
                        tonic::Status,
                    >;
                    async fn supply_delta(
                        &self,
                        request: tonic::Request<super::MsgSupplyDelta>,
                    ) -> std::result::Result<
                        tonic::Response<super::MsgSupplyDeltaResponse>,
                        tonic::Status,
                    >;
                    async fn withdraw_to_ethereum(
                        &self,
                        request: tonic::Request<super::MsgWithdrawToEthereum>,
                    ) -> std::result::Result<
                        tonic::Response<super::MsgWithdrawToEthereumResponse>,
                        tonic::Status,
                    >;
                    async fn deposit_from_ethereum(
                        &self,
                        request: tonic::Request<super::MsgDepositFromEthereum>,
                    ) -> std::result::Result<
                        tonic::Response<super::MsgDepositFromEthereumResponse>,
                        tonic::Status,
                    >;
                    async fn index(
                        &self,
                        request: tonic::Request<super::MsgIndex>,
                    ) -> std::result::Result<
                        tonic::Response<super::MsgIndexResponse>,
                        tonic::Status,
                    >;
                }
                #[derive(Debug)]
                pub struct MsgServer<T: Msg> {
                    inner: _Inner<T>,
                    accept_compression_encodings: EnabledCompressionEncodings,
                    send_compression_encodings: EnabledCompressionEncodings,
                    max_decoding_message_size: Option<usize>,
                    max_encoding_message_size: Option<usize>,
                }
                struct _Inner<T>(Arc<T>);
                impl<T: Msg> MsgServer<T> {
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
                    pub fn accept_compressed(
                        mut self,
                        encoding: CompressionEncoding,
                    ) -> Self {
                        self.accept_compression_encodings.enable(encoding);
                        self
                    }
                    /// Compress responses with the given encoding, if the client supports it.
                    #[must_use]
                    pub fn send_compressed(
                        mut self,
                        encoding: CompressionEncoding,
                    ) -> Self {
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
                impl<T, B> tonic::codegen::Service<http::Request<B>> for MsgServer<T>
                where
                    T: Msg,
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
                            "/fuelsequencer.bridge.v1.Msg/UpdateParams" => {
                                #[allow(non_camel_case_types)]
                                struct UpdateParamsSvc<T: Msg>(pub Arc<T>);
                                impl<T: Msg>
                                    tonic::server::UnaryService<super::MsgUpdateParams>
                                    for UpdateParamsSvc<T>
                                {
                                    type Response = super::MsgUpdateParamsResponse;
                                    type Future = BoxFuture<
                                        tonic::Response<Self::Response>,
                                        tonic::Status,
                                    >;
                                    fn call(
                                        &mut self,
                                        request: tonic::Request<super::MsgUpdateParams>,
                                    ) -> Self::Future
                                    {
                                        let inner = Arc::clone(&self.0);
                                        let fut = async move {
                                            <T as Msg>::update_params(&inner, request)
                                                .await
                                        };
                                        Box::pin(fut)
                                    }
                                }
                                let accept_compression_encodings =
                                    self.accept_compression_encodings;
                                let send_compression_encodings =
                                    self.send_compression_encodings;
                                let max_decoding_message_size =
                                    self.max_decoding_message_size;
                                let max_encoding_message_size =
                                    self.max_encoding_message_size;
                                let inner = self.inner.clone();
                                let fut = async move {
                                    let inner = inner.0;
                                    let method = UpdateParamsSvc(inner);
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
                                    let res = grpc.unary(method, req).await;
                                    Ok(res)
                                };
                                Box::pin(fut)
                            }
                            "/fuelsequencer.bridge.v1.Msg/SupplyDelta" => {
                                #[allow(non_camel_case_types)]
                                struct SupplyDeltaSvc<T: Msg>(pub Arc<T>);
                                impl<T: Msg>
                                    tonic::server::UnaryService<super::MsgSupplyDelta>
                                    for SupplyDeltaSvc<T>
                                {
                                    type Response = super::MsgSupplyDeltaResponse;
                                    type Future = BoxFuture<
                                        tonic::Response<Self::Response>,
                                        tonic::Status,
                                    >;
                                    fn call(
                                        &mut self,
                                        request: tonic::Request<super::MsgSupplyDelta>,
                                    ) -> Self::Future
                                    {
                                        let inner = Arc::clone(&self.0);
                                        let fut = async move {
                                            <T as Msg>::supply_delta(&inner, request)
                                                .await
                                        };
                                        Box::pin(fut)
                                    }
                                }
                                let accept_compression_encodings =
                                    self.accept_compression_encodings;
                                let send_compression_encodings =
                                    self.send_compression_encodings;
                                let max_decoding_message_size =
                                    self.max_decoding_message_size;
                                let max_encoding_message_size =
                                    self.max_encoding_message_size;
                                let inner = self.inner.clone();
                                let fut = async move {
                                    let inner = inner.0;
                                    let method = SupplyDeltaSvc(inner);
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
                                    let res = grpc.unary(method, req).await;
                                    Ok(res)
                                };
                                Box::pin(fut)
                            }
                            "/fuelsequencer.bridge.v1.Msg/WithdrawToEthereum" => {
                                #[allow(non_camel_case_types)]
                                struct WithdrawToEthereumSvc<T: Msg>(pub Arc<T>);
                                impl<T: Msg>
                                    tonic::server::UnaryService<
                                        super::MsgWithdrawToEthereum,
                                    > for WithdrawToEthereumSvc<T>
                                {
                                    type Response = super::MsgWithdrawToEthereumResponse;
                                    type Future = BoxFuture<
                                        tonic::Response<Self::Response>,
                                        tonic::Status,
                                    >;
                                    fn call(
                                        &mut self,
                                        request: tonic::Request<
                                            super::MsgWithdrawToEthereum,
                                        >,
                                    ) -> Self::Future
                                    {
                                        let inner = Arc::clone(&self.0);
                                        let fut = async move {
                                            <T as Msg>::withdraw_to_ethereum(
                                                &inner, request,
                                            )
                                            .await
                                        };
                                        Box::pin(fut)
                                    }
                                }
                                let accept_compression_encodings =
                                    self.accept_compression_encodings;
                                let send_compression_encodings =
                                    self.send_compression_encodings;
                                let max_decoding_message_size =
                                    self.max_decoding_message_size;
                                let max_encoding_message_size =
                                    self.max_encoding_message_size;
                                let inner = self.inner.clone();
                                let fut = async move {
                                    let inner = inner.0;
                                    let method = WithdrawToEthereumSvc(inner);
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
                                    let res = grpc.unary(method, req).await;
                                    Ok(res)
                                };
                                Box::pin(fut)
                            }
                            "/fuelsequencer.bridge.v1.Msg/DepositFromEthereum" => {
                                #[allow(non_camel_case_types)]
                                struct DepositFromEthereumSvc<T: Msg>(pub Arc<T>);
                                impl<T: Msg>
                                    tonic::server::UnaryService<
                                        super::MsgDepositFromEthereum,
                                    > for DepositFromEthereumSvc<T>
                                {
                                    type Response = super::MsgDepositFromEthereumResponse;
                                    type Future = BoxFuture<
                                        tonic::Response<Self::Response>,
                                        tonic::Status,
                                    >;
                                    fn call(
                                        &mut self,
                                        request: tonic::Request<
                                            super::MsgDepositFromEthereum,
                                        >,
                                    ) -> Self::Future
                                    {
                                        let inner = Arc::clone(&self.0);
                                        let fut = async move {
                                            <T as Msg>::deposit_from_ethereum(
                                                &inner, request,
                                            )
                                            .await
                                        };
                                        Box::pin(fut)
                                    }
                                }
                                let accept_compression_encodings =
                                    self.accept_compression_encodings;
                                let send_compression_encodings =
                                    self.send_compression_encodings;
                                let max_decoding_message_size =
                                    self.max_decoding_message_size;
                                let max_encoding_message_size =
                                    self.max_encoding_message_size;
                                let inner = self.inner.clone();
                                let fut = async move {
                                    let inner = inner.0;
                                    let method = DepositFromEthereumSvc(inner);
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
                                    let res = grpc.unary(method, req).await;
                                    Ok(res)
                                };
                                Box::pin(fut)
                            }
                            "/fuelsequencer.bridge.v1.Msg/Index" => {
                                #[allow(non_camel_case_types)]
                                struct IndexSvc<T: Msg>(pub Arc<T>);
                                impl<T: Msg> tonic::server::UnaryService<super::MsgIndex> for IndexSvc<T> {
                                    type Response = super::MsgIndexResponse;
                                    type Future = BoxFuture<
                                        tonic::Response<Self::Response>,
                                        tonic::Status,
                                    >;
                                    fn call(
                                        &mut self,
                                        request: tonic::Request<super::MsgIndex>,
                                    ) -> Self::Future
                                    {
                                        let inner = Arc::clone(&self.0);
                                        let fut = async move {
                                            <T as Msg>::index(&inner, request).await
                                        };
                                        Box::pin(fut)
                                    }
                                }
                                let accept_compression_encodings =
                                    self.accept_compression_encodings;
                                let send_compression_encodings =
                                    self.send_compression_encodings;
                                let max_decoding_message_size =
                                    self.max_decoding_message_size;
                                let max_encoding_message_size =
                                    self.max_encoding_message_size;
                                let inner = self.inner.clone();
                                let fut = async move {
                                    let inner = inner.0;
                                    let method = IndexSvc(inner);
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
                                    let res = grpc.unary(method, req).await;
                                    Ok(res)
                                };
                                Box::pin(fut)
                            }
                            _ => Box::pin(async move {
                                Ok(http::Response::builder()
                                    .status(200)
                                    .header("grpc-status", "12")
                                    .header("content-type", "application/grpc")
                                    .body(empty_body())
                                    .unwrap())
                            }),
                        }
                    }
                }
                impl<T: Msg> Clone for MsgServer<T> {
                    fn clone(&self) -> Self {
                        let inner = self.inner.clone();
                        Self {
                            inner,
                            accept_compression_encodings: self
                                .accept_compression_encodings,
                            send_compression_encodings: self.send_compression_encodings,
                            max_decoding_message_size: self.max_decoding_message_size,
                            max_encoding_message_size: self.max_encoding_message_size,
                        }
                    }
                }
                impl<T: Msg> Clone for _Inner<T> {
                    fn clone(&self) -> Self {
                        Self(Arc::clone(&self.0))
                    }
                }
                impl<T: std::fmt::Debug> std::fmt::Debug for _Inner<T> {
                    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                        write!(f, "{:?}", self.0)
                    }
                }
                impl<T: Msg> tonic::server::NamedService for MsgServer<T> {
                    const NAME: &'static str = "fuelsequencer.bridge.v1.Msg";
                }
            }

            // @@protoc_insertion_point(module)
        }
        pub mod module {
            // @generated
            /// Module is the config object for the module.
            #[allow(clippy::derive_partial_eq_without_eq)]
            #[derive(Clone, PartialEq, ::prost::Message)]
            pub struct Module {
                /// authority defines the custom module authority. If not set, defaults to the
                /// governance module.
                #[prost(string, tag = "1")]
                pub authority: ::prost::alloc::string::String,
            }
            // @@protoc_insertion_point(module)
        }
        // @generated
        /// An EthOwnedBaseAccount wraps a BaseAccount that is known to be owned and
        /// controlled by an Ethereum address.
        #[allow(clippy::derive_partial_eq_without_eq)]
        #[derive(Clone, PartialEq, ::prost::Message)]
        pub struct EthOwnedBaseAccount {
            #[prost(message, optional, tag = "1")]
            pub base_account:
                ::core::option::Option<super::super::cosmos::auth::v1beta1::BaseAccount>,
            /// account_owner is the Ethereum address that owns and controls this account.
            #[prost(string, tag = "2")]
            pub account_owner: ::prost::alloc::string::String,
        }
        /// An EthOwnedContinuousVestingAccount wraps a ContinuousVestingAccount
        /// that is known to be owned and controlled by an Ethereum address.
        #[allow(clippy::derive_partial_eq_without_eq)]
        #[derive(Clone, PartialEq, ::prost::Message)]
        pub struct EthOwnedContinuousVestingAccount {
            #[prost(message, optional, tag = "1")]
            pub vesting_account: ::core::option::Option<
                super::super::cosmos::vesting::v1beta1::ContinuousVestingAccount,
            >,
            /// account_owner is the Ethereum address that owns and controls this account.
            #[prost(string, tag = "2")]
            pub account_owner: ::prost::alloc::string::String,
        }
        /// AuthorizeTx contains a list of sdk.Msg's. It should be used when sending
        /// transactions from Ethereum to the Sequencer.
        #[allow(clippy::derive_partial_eq_without_eq)]
        #[derive(Clone, PartialEq, ::prost::Message)]
        pub struct AuthorizeTx {
            #[prost(message, repeated, tag = "1")]
            pub messages: ::prost::alloc::vec::Vec<::prost_types::Any>,
        }
        #[allow(clippy::derive_partial_eq_without_eq)]
        #[derive(Clone, PartialEq, ::prost::Message)]
        pub struct EventSupplyDeltaReported {
            /// supply_delta is the latest supply delta reported to Ethereum.
            #[prost(string, tag = "1")]
            pub supply_delta: ::prost::alloc::string::String,
            /// nonce is the latest nonce reported to Ethereum.
            #[prost(string, tag = "2")]
            pub nonce: ::prost::alloc::string::String,
        }
        #[allow(clippy::derive_partial_eq_without_eq)]
        #[derive(Clone, PartialEq, ::prost::Message)]
        pub struct EventWithdrawToEthereumReported {
            /// nonce uniquely identifies any message that we send to Ethereum.
            #[prost(string, tag = "1")]
            pub nonce: ::prost::alloc::string::String,
            /// from is the user address that is withdrawing the tokens from the Sequencer.
            /// can be in Hex or Bech32 format.
            #[prost(string, tag = "2")]
            pub from: ::prost::alloc::string::String,
            /// to is the user address on Ethereum that will be receiving the tokens.
            #[prost(string, tag = "3")]
            pub to: ::prost::alloc::string::String,
            /// amount is the tokens being sent, which must be the expected bridge token.
            #[prost(message, optional, tag = "4")]
            pub amount: ::core::option::Option<super::super::cosmos::base::v1beta1::Coin>,
        }
        #[allow(clippy::derive_partial_eq_without_eq)]
        #[derive(Clone, PartialEq, ::prost::Message)]
        pub struct EventDepositEventProcessed {
            /// the sending Ethereum address in hex format
            #[prost(string, tag = "1")]
            pub depositor: ::prost::alloc::string::String,
            /// recipient Ethereum address in hex or bech32 format
            #[prost(string, tag = "2")]
            pub recipient: ::prost::alloc::string::String,
            /// amount is the tokens being sent, which must be denominated in the expected
            /// bridge token.
            #[prost(message, optional, tag = "3")]
            pub amount: ::core::option::Option<super::super::cosmos::base::v1beta1::Coin>,
            /// vesting duration encoded in string to prevent loss of precision. Sign is
            /// also preserved. This can be zero if no duration is specified.
            #[prost(string, tag = "4")]
            pub lockup: ::prost::alloc::string::String,
        }
        #[allow(clippy::derive_partial_eq_without_eq)]
        #[derive(Clone, PartialEq, ::prost::Message)]
        pub struct EventDepositEventFailed {
            /// event_details is the marshalled event that failed to be processed.
            #[prost(bytes = "bytes", tag = "1")]
            pub event_details: ::prost::bytes::Bytes,
        }
        /// Params defines the parameters for the module.
        #[allow(clippy::derive_partial_eq_without_eq)]
        #[derive(Clone, PartialEq, ::prost::Message)]
        pub struct Params {
            /// bridge_denom is the assumed denom for the bridged token, used when minting
            /// upon deposits, burning when withdrawing, and tracking changes in its
            /// supply that will be reported to Ethereum, amongst other scenarios.
            #[prost(string, tag = "1")]
            pub bridge_denom: ::prost::alloc::string::String,
            /// ethereum_proxy_contract_address is the contract address we expect to
            /// receive deposit and authorize messages from.
            #[prost(string, tag = "2")]
            pub ethereum_proxy_contract_address: ::prost::alloc::string::String,
            /// authorize_messages_allowed is a whitelist for authorize messages that we
            /// can receive and process.
            #[prost(string, repeated, tag = "3")]
            pub authorize_messages_allowed:
                ::prost::alloc::vec::Vec<::prost::alloc::string::String>,
            /// supply_delta_period is the frequency in block at which we report supply
            /// delta info to Ethereum.
            #[prost(uint64, tag = "4")]
            pub supply_delta_period: u64,
            /// vesting_start_time is the common vesting starting time for vesting accounts
            /// that will be created through deposits from Ethereum.
            #[prost(message, optional, tag = "5")]
            pub vesting_start_time: ::core::option::Option<::prost_types::Timestamp>,
            /// additional_blocked_addresses is a list of Cosmos SDK-based addresses that
            /// are explicitly disallowed from being controlled by authorize messages
            /// within the Sequencer system. This can include addresses of module accounts,
            /// validator operators, or any other addresses deemed necessary to protect
            /// from unauthorized control actions.
            #[prost(string, repeated, tag = "6")]
            pub additional_blocked_addresses:
                ::prost::alloc::vec::Vec<::prost::alloc::string::String>,
            /// max_eth_block_update_delay is the maximum amount of time that the Sequencer
            /// allows validators to not sync up with Ethereum. Once
            /// max_eth_block_update_delay is exceeded, the Sequencer's block production
            /// will halt until validators sync up with next Ethereum block.
            #[prost(message, optional, tag = "7")]
            pub max_eth_block_update_delay:
                ::core::option::Option<::prost_types::Duration>,
            /// injected_event_tx_max_bytes is the maximum amount of block space that an
            /// injected event tx can take in terms of bytes. An Authorize event gets
            /// skipped and never included in a block if it can't be converted into a tx
            /// that can respect this limit. On the other hand, if a Deposit event cannot
            /// be converted into a Tx that can respect this limit, the chain halts.
            #[prost(uint64, tag = "8")]
            pub injected_event_tx_max_bytes: u64,
        }
        #[allow(clippy::derive_partial_eq_without_eq)]
        #[derive(Clone, PartialEq, ::prost::Message)]
        pub struct SupplyDeltaInfo {
            /// last_supply is the last supply recorded for the bridge token.
            #[prost(string, tag = "1")]
            pub last_supply: ::prost::alloc::string::String,
            /// delta is the amount of bridge tokens that should be minted (positive) or
            /// burned (negative) on Ethereum.
            #[prost(string, tag = "2")]
            pub delta: ::prost::alloc::string::String,
            /// offset is used to account for changes in supply that we do not want to post
            /// to Ethereum, including deposits and withdrawals, because Ethereum will know
            /// about these anyways. Note that offset can be positive (to account for
            /// unreported burn) or negative (to account for unreported mint)
            #[prost(string, tag = "3")]
            pub offset: ::prost::alloc::string::String,
        }
        /// GenesisState defines the bridge module's genesis state.
        #[allow(clippy::derive_partial_eq_without_eq)]
        #[derive(Clone, PartialEq, ::prost::Message)]
        pub struct GenesisState {
            /// params defines all the parameters of the module.
            #[prost(message, optional, tag = "1")]
            pub params: ::core::option::Option<Params>,
            /// supply_delta_info is the starting point for changes in the bridge token
            /// supply that we should report to Ethereum.
            #[prost(message, optional, tag = "2")]
            pub supply_delta_info: ::core::option::Option<SupplyDeltaInfo>,
            /// last_ethereum_nonce is the last nonce used in messages towards Ethereum.
            /// In other words, the next nonce to be used is this value +1.
            #[prost(bytes = "bytes", tag = "3")]
            pub last_ethereum_nonce: ::prost::bytes::Bytes,
            /// last_ethereum_block_synced is the last Ethereum block synced.
            /// In other words, the next block to be synced is this value +1.
            #[prost(uint64, tag = "4")]
            pub last_ethereum_block_synced: u64,
            /// ethereum_event_index_offset is the number of events to skip
            /// from the next Ethereum block to query from the Sidecar. This
            /// is used if queried events are larger than the maximum block size.
            #[prost(uint64, tag = "5")]
            pub ethereum_event_index_offset: u64,
            /// last_eth_block_update_time is the time of the last Sequencer
            /// block at which consensus was reach by the validators to sync
            /// up with an Ethereum block.
            #[prost(message, optional, tag = "6")]
            pub last_eth_block_update_time:
                ::core::option::Option<::prost_types::Timestamp>,
        }
        #[allow(clippy::derive_partial_eq_without_eq)]
        #[derive(Clone, PartialEq, ::prost::Message)]
        pub struct Index {
            /// num_injected_txs_total is the number of Ethereum events injected as
            /// txs in the current block, with any SupplyDelta, but excluding MsgIndex.
            #[prost(uint64, tag = "1")]
            pub num_injected_txs_total: u64,
            /// num_injected_txs_ante is the number of injected transactions that
            /// have been seen by the AnteHandler.
            #[prost(uint64, tag = "2")]
            pub num_injected_txs_ante: u64,
            /// num_failed_special_txs is the number of failed special transactions, i.e.
            /// MsgIndex, MsgDepositFromEthereum, and MsgSupplyDelta. We should stop block
            /// production if any of these fails because something is really wrong.
            #[prost(uint64, tag = "3")]
            pub num_failed_special_txs: u64,
        }
        // @@protoc_insertion_point(module)
    }
    pub mod service {
        pub mod v1 {
            // @generated
            /// DepositEvent represents a deposit event raised by the proxy contract.
            /// This message represents the event structure on the Sequencer.
            #[allow(clippy::derive_partial_eq_without_eq)]
            #[derive(Clone, PartialEq, ::prost::Message)]
            pub struct DepositEvent {
                /// the sending Ethereum address in hex format
                #[prost(string, tag = "1")]
                pub depositor: ::prost::alloc::string::String,
                /// recipient address in hex or bech32 format. If the recipient is the null
                /// address, the Sequencer uses the depositor address as the recipient.
                #[prost(string, tag = "2")]
                pub recipient: ::prost::alloc::string::String,
                /// the amount sent encoded as string to prevent loss of precision. Sign is
                /// also preserved
                #[prost(string, tag = "3")]
                pub amount: ::prost::alloc::string::String,
                /// vesting duration encoded in string to prevent loss of precision. Sign is
                /// also preserved. This can be zero if no duration is specified.
                #[prost(string, tag = "4")]
                pub lockup: ::prost::alloc::string::String,
            }
            /// AuthorizeEvent represents an authorize event raised by the proxy contract.
            /// This message represents the event structure on the Sequencer.
            #[allow(clippy::derive_partial_eq_without_eq)]
            #[derive(Clone, PartialEq, ::prost::Message)]
            pub struct AuthorizeEvent {
                /// the Ethereum address granting authorization in hex format
                #[prost(string, tag = "1")]
                pub sender: ::prost::alloc::string::String,
                /// message to be executed
                #[prost(bytes = "bytes", tag = "2")]
                pub data: ::prost::bytes::Bytes,
            }
            /// QueryBlockEventsRequest defines the request type for the the GetBlockEvents
            /// method.
            #[allow(clippy::derive_partial_eq_without_eq)]
            #[derive(Clone, PartialEq, ::prost::Message)]
            pub struct QueryBlockEventsRequest {
                #[prost(string, tag = "1")]
                pub block_number: ::prost::alloc::string::String,
            }
            /// QueryBlockEventsResponse defines the response type for the GetBlockEvents
            /// method.
            #[allow(clippy::derive_partial_eq_without_eq)]
            #[derive(Clone, PartialEq, ::prost::Message)]
            pub struct QueryBlockEventsResponse {
                /// events defines the list of events.
                #[prost(message, repeated, tag = "1")]
                pub events: ::prost::alloc::vec::Vec<Event>,
            }
            /// Event stores the event data, type and contract address queried from Ethereum.
            #[allow(clippy::derive_partial_eq_without_eq)]
            #[derive(Clone, PartialEq, ::prost::Message)]
            pub struct Event {
                #[prost(string, tag = "1")]
                pub event_type: ::prost::alloc::string::String,
                #[prost(bytes = "bytes", tag = "2")]
                pub data: ::prost::bytes::Bytes,
                #[prost(string, tag = "3")]
                pub contract_address: ::prost::alloc::string::String,
            }
            // @generated
            /// Generated client implementations.
            pub mod sidecar_client {
                #![allow(
                    unused_variables,
                    dead_code,
                    missing_docs,
                    clippy::let_unit_value
                )]
                use tonic::codegen::{
                    http::Uri,
                    *,
                };
                #[derive(Debug, Clone)]
                pub struct SidecarClient<T> {
                    inner: tonic::client::Grpc<T>,
                }
                impl SidecarClient<tonic::transport::Channel> {
                    /// Attempt to create a new client by connecting to a given endpoint.
                    pub async fn connect<D>(
                        dst: D,
                    ) -> Result<Self, tonic::transport::Error>
                    where
                        D: TryInto<tonic::transport::Endpoint>,
                        D::Error: Into<StdError>,
                    {
                        let conn =
                            tonic::transport::Endpoint::new(dst)?.connect().await?;
                        Ok(Self::new(conn))
                    }
                }
                impl<T> SidecarClient<T>
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
                    ) -> SidecarClient<InterceptedService<T, F>>
                    where
                        F: tonic::service::Interceptor,
                        T::ResponseBody: Default,
                        T:
                            tonic::codegen::Service<
                                http::Request<tonic::body::BoxBody>,
                                Response = http::Response<
                                    <T as tonic::client::GrpcService<
                                        tonic::body::BoxBody,
                                    >>::ResponseBody,
                                >,
                            >,
                        <T as tonic::codegen::Service<
                            http::Request<tonic::body::BoxBody>,
                        >>::Error: Into<StdError> + Send + Sync,
                    {
                        SidecarClient::new(InterceptedService::new(inner, interceptor))
                    }
                    /// Compress requests with the given encoding.
                    ///
                    /// This requires the server to support it otherwise it might respond with an
                    /// error.
                    #[must_use]
                    pub fn send_compressed(
                        mut self,
                        encoding: CompressionEncoding,
                    ) -> Self {
                        self.inner = self.inner.send_compressed(encoding);
                        self
                    }
                    /// Enable decompressing responses.
                    #[must_use]
                    pub fn accept_compressed(
                        mut self,
                        encoding: CompressionEncoding,
                    ) -> Self {
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
                    pub async fn get_block_events(
                        &mut self,
                        request: impl tonic::IntoRequest<super::QueryBlockEventsRequest>,
                    ) -> std::result::Result<
                        tonic::Response<super::QueryBlockEventsResponse>,
                        tonic::Status,
                    > {
                        self.inner.ready().await.map_err(|e| {
                            tonic::Status::new(
                                tonic::Code::Unknown,
                                format!("Service was not ready: {}", e.into()),
                            )
                        })?;
                        let codec = tonic::codec::ProstCodec::default();
                        let path = http::uri::PathAndQuery::from_static(
                            "/fuelsequencer.service.v1.Sidecar/GetBlockEvents",
                        );
                        let mut req = request.into_request();
                        req.extensions_mut().insert(GrpcMethod::new(
                            "fuelsequencer.service.v1.Sidecar",
                            "GetBlockEvents",
                        ));
                        self.inner.unary(req, path, codec).await
                    }
                }
            }
            /// Generated server implementations.
            pub mod sidecar_server {
                #![allow(
                    unused_variables,
                    dead_code,
                    missing_docs,
                    clippy::let_unit_value
                )]
                use tonic::codegen::*;
                /// Generated trait containing gRPC methods that should be implemented for use with SidecarServer.
                #[async_trait]
                pub trait Sidecar: Send + Sync + 'static {
                    async fn get_block_events(
                        &self,
                        request: tonic::Request<super::QueryBlockEventsRequest>,
                    ) -> std::result::Result<
                        tonic::Response<super::QueryBlockEventsResponse>,
                        tonic::Status,
                    >;
                }
                #[derive(Debug)]
                pub struct SidecarServer<T: Sidecar> {
                    inner: _Inner<T>,
                    accept_compression_encodings: EnabledCompressionEncodings,
                    send_compression_encodings: EnabledCompressionEncodings,
                    max_decoding_message_size: Option<usize>,
                    max_encoding_message_size: Option<usize>,
                }
                struct _Inner<T>(Arc<T>);
                impl<T: Sidecar> SidecarServer<T> {
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
                    pub fn accept_compressed(
                        mut self,
                        encoding: CompressionEncoding,
                    ) -> Self {
                        self.accept_compression_encodings.enable(encoding);
                        self
                    }
                    /// Compress responses with the given encoding, if the client supports it.
                    #[must_use]
                    pub fn send_compressed(
                        mut self,
                        encoding: CompressionEncoding,
                    ) -> Self {
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
                impl<T, B> tonic::codegen::Service<http::Request<B>> for SidecarServer<T>
                where
                    T: Sidecar,
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
                            "/fuelsequencer.service.v1.Sidecar/GetBlockEvents" => {
                                #[allow(non_camel_case_types)]
                                struct GetBlockEventsSvc<T: Sidecar>(pub Arc<T>);
                                impl<T: Sidecar>
                                    tonic::server::UnaryService<
                                        super::QueryBlockEventsRequest,
                                    > for GetBlockEventsSvc<T>
                                {
                                    type Response = super::QueryBlockEventsResponse;
                                    type Future = BoxFuture<
                                        tonic::Response<Self::Response>,
                                        tonic::Status,
                                    >;
                                    fn call(
                                        &mut self,
                                        request: tonic::Request<
                                            super::QueryBlockEventsRequest,
                                        >,
                                    ) -> Self::Future
                                    {
                                        let inner = Arc::clone(&self.0);
                                        let fut = async move {
                                            <T as Sidecar>::get_block_events(
                                                &inner, request,
                                            )
                                            .await
                                        };
                                        Box::pin(fut)
                                    }
                                }
                                let accept_compression_encodings =
                                    self.accept_compression_encodings;
                                let send_compression_encodings =
                                    self.send_compression_encodings;
                                let max_decoding_message_size =
                                    self.max_decoding_message_size;
                                let max_encoding_message_size =
                                    self.max_encoding_message_size;
                                let inner = self.inner.clone();
                                let fut = async move {
                                    let inner = inner.0;
                                    let method = GetBlockEventsSvc(inner);
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
                                    let res = grpc.unary(method, req).await;
                                    Ok(res)
                                };
                                Box::pin(fut)
                            }
                            _ => Box::pin(async move {
                                Ok(http::Response::builder()
                                    .status(200)
                                    .header("grpc-status", "12")
                                    .header("content-type", "application/grpc")
                                    .body(empty_body())
                                    .unwrap())
                            }),
                        }
                    }
                }
                impl<T: Sidecar> Clone for SidecarServer<T> {
                    fn clone(&self) -> Self {
                        let inner = self.inner.clone();
                        Self {
                            inner,
                            accept_compression_encodings: self
                                .accept_compression_encodings,
                            send_compression_encodings: self.send_compression_encodings,
                            max_decoding_message_size: self.max_decoding_message_size,
                            max_encoding_message_size: self.max_encoding_message_size,
                        }
                    }
                }
                impl<T: Sidecar> Clone for _Inner<T> {
                    fn clone(&self) -> Self {
                        Self(Arc::clone(&self.0))
                    }
                }
                impl<T: std::fmt::Debug> std::fmt::Debug for _Inner<T> {
                    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                        write!(f, "{:?}", self.0)
                    }
                }
                impl<T: Sidecar> tonic::server::NamedService for SidecarServer<T> {
                    const NAME: &'static str = "fuelsequencer.service.v1.Sidecar";
                }
            }

            // @@protoc_insertion_point(module)
        }
    }
}
pub mod google {
    pub mod api {
        // @generated
        /// Defines the HTTP configuration for an API service. It contains a list of
        /// [HttpRule][google.api.HttpRule], each specifying the mapping of an RPC method
        /// to one or more HTTP REST API methods.
        #[allow(clippy::derive_partial_eq_without_eq)]
        #[derive(Clone, PartialEq, ::prost::Message)]
        pub struct Http {
            /// A list of HTTP configuration rules that apply to individual API methods.
            ///
            /// **NOTE:** All service configuration rules follow "last one wins" order.
            #[prost(message, repeated, tag = "1")]
            pub rules: ::prost::alloc::vec::Vec<HttpRule>,
            /// When set to true, URL path parameters will be fully URI-decoded except in
            /// cases of single segment matches in reserved expansion, where "%2F" will be
            /// left encoded.
            ///
            /// The default behavior is to not decode RFC 6570 reserved characters in multi
            /// segment matches.
            #[prost(bool, tag = "2")]
            pub fully_decode_reserved_expansion: bool,
        }
        /// # gRPC Transcoding
        ///
        /// gRPC Transcoding is a feature for mapping between a gRPC method and one or
        /// more HTTP REST endpoints. It allows developers to build a single API service
        /// that supports both gRPC APIs and REST APIs. Many systems, including [Google
        /// APIs](<https://github.com/googleapis/googleapis>),
        /// [Cloud Endpoints](<https://cloud.google.com/endpoints>), [gRPC
        /// Gateway](<https://github.com/grpc-ecosystem/grpc-gateway>),
        /// and [Envoy](<https://github.com/envoyproxy/envoy>) proxy support this feature
        /// and use it for large scale production services.
        ///
        /// `HttpRule` defines the schema of the gRPC/REST mapping. The mapping specifies
        /// how different portions of the gRPC request message are mapped to the URL
        /// path, URL query parameters, and HTTP request body. It also controls how the
        /// gRPC response message is mapped to the HTTP response body. `HttpRule` is
        /// typically specified as an `google.api.http` annotation on the gRPC method.
        ///
        /// Each mapping specifies a URL path template and an HTTP method. The path
        /// template may refer to one or more fields in the gRPC request message, as long
        /// as each field is a non-repeated field with a primitive (non-message) type.
        /// The path template controls how fields of the request message are mapped to
        /// the URL path.
        ///
        /// Example:
        ///
        /// ```text
        /// service Messaging {
        /// rpc GetMessage(GetMessageRequest) returns (Message) {
        /// option (google.api.http) = {
        /// get: "/v1/{name=messages/*}"
        /// };
        /// }
        /// }
        /// message GetMessageRequest {
        /// string name = 1; // Mapped to URL path.
        /// }
        /// message Message {
        /// string text = 1; // The resource content.
        /// }
        /// ```
        ///
        /// This enables an HTTP REST to gRPC mapping as below:
        ///
        /// HTTP | gRPC
        /// -----|-----
        /// `GET /v1/messages/123456`  | `GetMessage(name: "messages/123456")`
        ///
        /// Any fields in the request message which are not bound by the path template
        /// automatically become HTTP query parameters if there is no HTTP request body.
        /// For example:
        ///
        /// ```text
        /// service Messaging {
        /// rpc GetMessage(GetMessageRequest) returns (Message) {
        /// option (google.api.http) = {
        /// get:"/v1/messages/{message_id}"
        /// };
        /// }
        /// }
        /// message GetMessageRequest {
        /// message SubMessage {
        /// string subfield = 1;
        /// }
        /// string message_id = 1; // Mapped to URL path.
        /// int64 revision = 2;    // Mapped to URL query parameter `revision`.
        /// SubMessage sub = 3;    // Mapped to URL query parameter `sub.subfield`.
        /// }
        /// ```
        ///
        /// This enables a HTTP JSON to RPC mapping as below:
        ///
        /// HTTP | gRPC
        /// -----|-----
        /// `GET /v1/messages/123456?revision=2&sub.subfield=foo` |
        /// `GetMessage(message_id: "123456" revision: 2 sub: SubMessage(subfield:
        /// "foo"))`
        ///
        /// Note that fields which are mapped to URL query parameters must have a
        /// primitive type or a repeated primitive type or a non-repeated message type.
        /// In the case of a repeated type, the parameter can be repeated in the URL
        /// as `...?param=A&param=B`. In the case of a message type, each field of the
        /// message is mapped to a separate parameter, such as
        /// `...?foo.a=A&foo.b=B&foo.c=C`.
        ///
        /// For HTTP methods that allow a request body, the `body` field
        /// specifies the mapping. Consider a REST update method on the
        /// message resource collection:
        ///
        /// ```text
        /// service Messaging {
        /// rpc UpdateMessage(UpdateMessageRequest) returns (Message) {
        /// option (google.api.http) = {
        /// patch: "/v1/messages/{message_id}"
        /// body: "message"
        /// };
        /// }
        /// }
        /// message UpdateMessageRequest {
        /// string message_id = 1; // mapped to the URL
        /// Message message = 2;   // mapped to the body
        /// }
        /// ```
        ///
        /// The following HTTP JSON to RPC mapping is enabled, where the
        /// representation of the JSON in the request body is determined by
        /// protos JSON encoding:
        ///
        /// HTTP | gRPC
        /// -----|-----
        /// `PATCH /v1/messages/123456 { "text": "Hi!" }` | `UpdateMessage(message_id:
        /// "123456" message { text: "Hi!" })`
        ///
        /// The special name `*` can be used in the body mapping to define that
        /// every field not bound by the path template should be mapped to the
        /// request body.  This enables the following alternative definition of
        /// the update method:
        ///
        /// ```text
        /// service Messaging {
        /// rpc UpdateMessage(Message) returns (Message) {
        /// option (google.api.http) = {
        /// patch: "/v1/messages/{message_id}"
        /// body: "*"
        /// };
        /// }
        /// }
        /// message Message {
        /// string message_id = 1;
        /// string text = 2;
        /// }
        /// ```
        ///
        ///
        /// The following HTTP JSON to RPC mapping is enabled:
        ///
        /// HTTP | gRPC
        /// -----|-----
        /// `PATCH /v1/messages/123456 { "text": "Hi!" }` | `UpdateMessage(message_id:
        /// "123456" text: "Hi!")`
        ///
        /// Note that when using `*` in the body mapping, it is not possible to
        /// have HTTP parameters, as all fields not bound by the path end in
        /// the body. This makes this option more rarely used in practice when
        /// defining REST APIs. The common usage of `*` is in custom methods
        /// which don't use the URL at all for transferring data.
        ///
        /// It is possible to define multiple HTTP methods for one RPC by using
        /// the `additional_bindings` option. Example:
        ///
        /// ```text
        /// service Messaging {
        /// rpc GetMessage(GetMessageRequest) returns (Message) {
        /// option (google.api.http) = {
        /// get: "/v1/messages/{message_id}"
        /// additional_bindings {
        /// get: "/v1/users/{user_id}/messages/{message_id}"
        /// }
        /// };
        /// }
        /// }
        /// message GetMessageRequest {
        /// string message_id = 1;
        /// string user_id = 2;
        /// }
        /// ```
        ///
        /// This enables the following two alternative HTTP JSON to RPC mappings:
        ///
        /// HTTP | gRPC
        /// -----|-----
        /// `GET /v1/messages/123456` | `GetMessage(message_id: "123456")`
        /// `GET /v1/users/me/messages/123456` | `GetMessage(user_id: "me" message_id:
        /// "123456")`
        ///
        /// ## Rules for HTTP mapping
        ///
        /// 1. Leaf request fields (recursive expansion nested messages in the request
        /// ```text
        /// message) are classified into three categories:
        /// - Fields referred by the path template. They are passed via the URL path.
        /// - Fields referred by the [HttpRule.body][google.api.HttpRule.body]. They are passed via the HTTP
        /// request body.
        /// - All other fields are passed via the URL query parameters, and the
        /// parameter name is the field path in the request message. A repeated
        /// field can be represented as multiple query parameters under the same
        /// name.
        /// 2. If [HttpRule.body][google.api.HttpRule.body] is "*", there is no URL query parameter, all fields
        /// are passed via URL path and HTTP request body.
        /// 3. If [HttpRule.body][google.api.HttpRule.body] is omitted, there is no HTTP request body, all
        /// fields are passed via URL path and URL query parameters.
        /// ```
        ///
        /// ### Path template syntax
        ///
        /// ```text
        /// Template = "/" Segments \[ Verb \] ;
        /// Segments = Segment { "/" Segment } ;
        /// Segment  = "*" | "**" | LITERAL | Variable ;
        /// Variable = "{" FieldPath \[ "=" Segments \] "}" ;
        /// FieldPath = IDENT { "." IDENT } ;
        /// Verb     = ":" LITERAL ;
        /// ```
        ///
        /// The syntax `*` matches a single URL path segment. The syntax `**` matches
        /// zero or more URL path segments, which must be the last part of the URL path
        /// except the `Verb`.
        ///
        /// The syntax `Variable` matches part of the URL path as specified by its
        /// template. A variable template must not contain other variables. If a variable
        /// matches a single path segment, its template may be omitted, e.g. `{var}`
        /// is equivalent to `{var=*}`.
        ///
        /// The syntax `LITERAL` matches literal text in the URL path. If the `LITERAL`
        /// contains any reserved character, such characters should be percent-encoded
        /// before the matching.
        ///
        /// If a variable contains exactly one path segment, such as `"{var}"` or
        /// `"{var=*}"`, when such a variable is expanded into a URL path on the client
        /// side, all characters except `\[-_.~0-9a-zA-Z\]` are percent-encoded. The
        /// server side does the reverse decoding. Such variables show up in the
        /// [Discovery
        /// Document](<https://developers.google.com/discovery/v1/reference/apis>) as
        /// `{var}`.
        ///
        /// If a variable contains multiple path segments, such as `"{var=foo/*}"`
        /// or `"{var=**}"`, when such a variable is expanded into a URL path on the
        /// client side, all characters except `\[-_.~/0-9a-zA-Z\]` are percent-encoded.
        /// The server side does the reverse decoding, except "%2F" and "%2f" are left
        /// unchanged. Such variables show up in the
        /// [Discovery
        /// Document](<https://developers.google.com/discovery/v1/reference/apis>) as
        /// `{+var}`.
        ///
        /// ## Using gRPC API Service Configuration
        ///
        /// gRPC API Service Configuration (service config) is a configuration language
        /// for configuring a gRPC service to become a user-facing product. The
        /// service config is simply the YAML representation of the `google.api.Service`
        /// proto message.
        ///
        /// As an alternative to annotating your proto file, you can configure gRPC
        /// transcoding in your service config YAML files. You do this by specifying a
        /// `HttpRule` that maps the gRPC method to a REST endpoint, achieving the same
        /// effect as the proto annotation. This can be particularly useful if you
        /// have a proto that is reused in multiple services. Note that any transcoding
        /// specified in the service config will override any matching transcoding
        /// configuration in the proto.
        ///
        /// Example:
        ///
        /// ```text
        /// http:
        /// rules:
        /// # Selects a gRPC method and applies HttpRule to it.
        /// - selector: example.v1.Messaging.GetMessage
        /// get: /v1/messages/{message_id}/{sub.subfield}
        /// ```
        ///
        /// ## Special notes
        ///
        /// When gRPC Transcoding is used to map a gRPC to JSON REST endpoints, the
        /// proto to JSON conversion must follow the [proto3
        /// specification](<https://developers.google.com/protocol-buffers/docs/proto3#json>).
        ///
        /// While the single segment variable follows the semantics of
        /// [RFC 6570](<https://tools.ietf.org/html/rfc6570>) Section 3.2.2 Simple String
        /// Expansion, the multi segment variable **does not** follow RFC 6570 Section
        /// 3.2.3 Reserved Expansion. The reason is that the Reserved Expansion
        /// does not expand special characters like `?` and `#`, which would lead
        /// to invalid URLs. As the result, gRPC Transcoding uses a custom encoding
        /// for multi segment variables.
        ///
        /// The path variables **must not** refer to any repeated or mapped field,
        /// because client libraries are not capable of handling such variable expansion.
        ///
        /// The path variables **must not** capture the leading "/" character. The reason
        /// is that the most common use case "{var}" does not capture the leading "/"
        /// character. For consistency, all path variables must share the same behavior.
        ///
        /// Repeated message fields must not be mapped to URL query parameters, because
        /// no client library can support such complicated mapping.
        ///
        /// If an API needs to use a JSON array for request or response body, it can map
        /// the request or response body to a repeated field. However, some gRPC
        /// Transcoding implementations may not support this feature.
        #[allow(clippy::derive_partial_eq_without_eq)]
        #[derive(Clone, PartialEq, ::prost::Message)]
        pub struct HttpRule {
            /// Selects a method to which this rule applies.
            ///
            /// Refer to [selector][google.api.DocumentationRule.selector] for syntax details.
            #[prost(string, tag = "1")]
            pub selector: ::prost::alloc::string::String,
            /// The name of the request field whose value is mapped to the HTTP request
            /// body, or `*` for mapping all request fields not captured by the path
            /// pattern to the HTTP body, or omitted for not having any HTTP request body.
            ///
            /// NOTE: the referred field must be present at the top-level of the request
            /// message type.
            #[prost(string, tag = "7")]
            pub body: ::prost::alloc::string::String,
            /// Optional. The name of the response field whose value is mapped to the HTTP
            /// response body. When omitted, the entire response message will be used
            /// as the HTTP response body.
            ///
            /// NOTE: The referred field must be present at the top-level of the response
            /// message type.
            #[prost(string, tag = "12")]
            pub response_body: ::prost::alloc::string::String,
            /// Additional HTTP bindings for the selector. Nested bindings must
            /// not contain an `additional_bindings` field themselves (that is,
            /// the nesting may only be one level deep).
            #[prost(message, repeated, tag = "11")]
            pub additional_bindings: ::prost::alloc::vec::Vec<HttpRule>,
            /// Determines the URL pattern is matched by this rules. This pattern can be
            /// used with any of the {get|put|post|delete|patch} methods. A custom method
            /// can be defined using the 'custom' field.
            #[prost(oneof = "http_rule::Pattern", tags = "2, 3, 4, 5, 6, 8")]
            pub pattern: ::core::option::Option<http_rule::Pattern>,
        }
        /// Nested message and enum types in `HttpRule`.
        pub mod http_rule {
            /// Determines the URL pattern is matched by this rules. This pattern can be
            /// used with any of the {get|put|post|delete|patch} methods. A custom method
            /// can be defined using the 'custom' field.
            #[allow(clippy::derive_partial_eq_without_eq)]
            #[derive(Clone, PartialEq, ::prost::Oneof)]
            pub enum Pattern {
                /// Maps to HTTP GET. Used for listing and getting information about
                /// resources.
                #[prost(string, tag = "2")]
                Get(::prost::alloc::string::String),
                /// Maps to HTTP PUT. Used for replacing a resource.
                #[prost(string, tag = "3")]
                Put(::prost::alloc::string::String),
                /// Maps to HTTP POST. Used for creating a resource or performing an action.
                #[prost(string, tag = "4")]
                Post(::prost::alloc::string::String),
                /// Maps to HTTP DELETE. Used for deleting a resource.
                #[prost(string, tag = "5")]
                Delete(::prost::alloc::string::String),
                /// Maps to HTTP PATCH. Used for updating a resource.
                #[prost(string, tag = "6")]
                Patch(::prost::alloc::string::String),
                /// The custom pattern is used for specifying an HTTP method that is not
                /// included in the `pattern` field, such as HEAD, or "*" to leave the
                /// HTTP method unspecified for this rule. The wild-card rule is useful
                /// for services that provide content to Web (HTML) clients.
                #[prost(message, tag = "8")]
                Custom(super::CustomHttpPattern),
            }
        }
        /// A custom pattern is used for defining custom HTTP verb.
        #[allow(clippy::derive_partial_eq_without_eq)]
        #[derive(Clone, PartialEq, ::prost::Message)]
        pub struct CustomHttpPattern {
            /// The name of this custom HTTP verb.
            #[prost(string, tag = "1")]
            pub kind: ::prost::alloc::string::String,
            /// The path matched by this custom verb.
            #[prost(string, tag = "2")]
            pub path: ::prost::alloc::string::String,
        }
        // @@protoc_insertion_point(module)
    }
}
pub mod gogoproto {
    // @generated
    // @@protoc_insertion_point(module)
}
pub mod cosmos {
    pub mod vesting {
        pub mod v1beta1 {
            // @generated
            /// BaseVestingAccount implements the VestingAccount interface. It contains all
            /// the necessary fields needed for any vesting account implementation.
            #[allow(clippy::derive_partial_eq_without_eq)]
            #[derive(Clone, PartialEq, ::prost::Message)]
            pub struct BaseVestingAccount {
                #[prost(message, optional, tag = "1")]
                pub base_account:
                    ::core::option::Option<super::super::auth::v1beta1::BaseAccount>,
                #[prost(message, repeated, tag = "2")]
                pub original_vesting:
                    ::prost::alloc::vec::Vec<super::super::base::v1beta1::Coin>,
                #[prost(message, repeated, tag = "3")]
                pub delegated_free:
                    ::prost::alloc::vec::Vec<super::super::base::v1beta1::Coin>,
                #[prost(message, repeated, tag = "4")]
                pub delegated_vesting:
                    ::prost::alloc::vec::Vec<super::super::base::v1beta1::Coin>,
                /// Vesting end time, as unix timestamp (in seconds).
                #[prost(int64, tag = "5")]
                pub end_time: i64,
            }
            /// ContinuousVestingAccount implements the VestingAccount interface. It
            /// continuously vests by unlocking coins linearly with respect to time.
            #[allow(clippy::derive_partial_eq_without_eq)]
            #[derive(Clone, PartialEq, ::prost::Message)]
            pub struct ContinuousVestingAccount {
                #[prost(message, optional, tag = "1")]
                pub base_vesting_account: ::core::option::Option<BaseVestingAccount>,
                /// Vesting start time, as unix timestamp (in seconds).
                #[prost(int64, tag = "2")]
                pub start_time: i64,
            }
            /// DelayedVestingAccount implements the VestingAccount interface. It vests all
            /// coins after a specific time, but non prior. In other words, it keeps them
            /// locked until a specified time.
            #[allow(clippy::derive_partial_eq_without_eq)]
            #[derive(Clone, PartialEq, ::prost::Message)]
            pub struct DelayedVestingAccount {
                #[prost(message, optional, tag = "1")]
                pub base_vesting_account: ::core::option::Option<BaseVestingAccount>,
            }
            /// Period defines a length of time and amount of coins that will vest.
            #[allow(clippy::derive_partial_eq_without_eq)]
            #[derive(Clone, PartialEq, ::prost::Message)]
            pub struct Period {
                /// Period duration in seconds.
                #[prost(int64, tag = "1")]
                pub length: i64,
                #[prost(message, repeated, tag = "2")]
                pub amount: ::prost::alloc::vec::Vec<super::super::base::v1beta1::Coin>,
            }
            /// PeriodicVestingAccount implements the VestingAccount interface. It
            /// periodically vests by unlocking coins during each specified period.
            #[allow(clippy::derive_partial_eq_without_eq)]
            #[derive(Clone, PartialEq, ::prost::Message)]
            pub struct PeriodicVestingAccount {
                #[prost(message, optional, tag = "1")]
                pub base_vesting_account: ::core::option::Option<BaseVestingAccount>,
                #[prost(int64, tag = "2")]
                pub start_time: i64,
                #[prost(message, repeated, tag = "3")]
                pub vesting_periods: ::prost::alloc::vec::Vec<Period>,
            }
            /// PermanentLockedAccount implements the VestingAccount interface. It does
            /// not ever release coins, locking them indefinitely. Coins in this account can
            /// still be used for delegating and for governance votes even while locked.
            ///
            /// Since: cosmos-sdk 0.43
            #[allow(clippy::derive_partial_eq_without_eq)]
            #[derive(Clone, PartialEq, ::prost::Message)]
            pub struct PermanentLockedAccount {
                #[prost(message, optional, tag = "1")]
                pub base_vesting_account: ::core::option::Option<BaseVestingAccount>,
            }
            // @@protoc_insertion_point(module)
        }
    }
    pub mod auth {
        pub mod v1beta1 {
            // @generated
            /// BaseAccount defines a base account type. It contains all the necessary fields
            /// for basic account functionality. Any custom account type should extend this
            /// type for additional functionality (e.g. vesting).
            #[allow(clippy::derive_partial_eq_without_eq)]
            #[derive(Clone, PartialEq, ::prost::Message)]
            pub struct BaseAccount {
                #[prost(string, tag = "1")]
                pub address: ::prost::alloc::string::String,
                #[prost(message, optional, tag = "2")]
                pub pub_key: ::core::option::Option<::prost_types::Any>,
                #[prost(uint64, tag = "3")]
                pub account_number: u64,
                #[prost(uint64, tag = "4")]
                pub sequence: u64,
            }
            /// ModuleAccount defines an account for modules that holds coins on a pool.
            #[allow(clippy::derive_partial_eq_without_eq)]
            #[derive(Clone, PartialEq, ::prost::Message)]
            pub struct ModuleAccount {
                #[prost(message, optional, tag = "1")]
                pub base_account: ::core::option::Option<BaseAccount>,
                #[prost(string, tag = "2")]
                pub name: ::prost::alloc::string::String,
                #[prost(string, repeated, tag = "3")]
                pub permissions: ::prost::alloc::vec::Vec<::prost::alloc::string::String>,
            }
            /// ModuleCredential represents a unclaimable pubkey for base accounts controlled by modules.
            ///
            /// Since: cosmos-sdk 0.47
            #[allow(clippy::derive_partial_eq_without_eq)]
            #[derive(Clone, PartialEq, ::prost::Message)]
            pub struct ModuleCredential {
                /// module_name is the name of the module used for address derivation (passed into address.Module).
                #[prost(string, tag = "1")]
                pub module_name: ::prost::alloc::string::String,
                /// derivation_keys is for deriving a module account address (passed into address.Module)
                /// adding more keys creates sub-account addresses (passed into address.Derive)
                #[prost(bytes = "bytes", repeated, tag = "2")]
                pub derivation_keys: ::prost::alloc::vec::Vec<::prost::bytes::Bytes>,
            }
            /// Params defines the parameters for the auth module.
            #[allow(clippy::derive_partial_eq_without_eq)]
            #[derive(Clone, PartialEq, ::prost::Message)]
            pub struct Params {
                #[prost(uint64, tag = "1")]
                pub max_memo_characters: u64,
                #[prost(uint64, tag = "2")]
                pub tx_sig_limit: u64,
                #[prost(uint64, tag = "3")]
                pub tx_size_cost_per_byte: u64,
                #[prost(uint64, tag = "4")]
                pub sig_verify_cost_ed25519: u64,
                #[prost(uint64, tag = "5")]
                pub sig_verify_cost_secp256k1: u64,
            }
            // @@protoc_insertion_point(module)
        }
    }
    pub mod app {
        pub mod v1alpha1 {
            // @generated
            /// ModuleDescriptor describes an app module.
            #[allow(clippy::derive_partial_eq_without_eq)]
            #[derive(Clone, PartialEq, ::prost::Message)]
            pub struct ModuleDescriptor {
                /// go_import names the package that should be imported by an app to load the
                /// module in the runtime module registry. It is required to make debugging
                /// of configuration errors easier for users.
                #[prost(string, tag = "1")]
                pub go_import: ::prost::alloc::string::String,
                /// use_package refers to a protobuf package that this module
                /// uses and exposes to the world. In an app, only one module should "use"
                /// or own a single protobuf package. It is assumed that the module uses
                /// all of the .proto files in a single package.
                #[prost(message, repeated, tag = "2")]
                pub use_package: ::prost::alloc::vec::Vec<PackageReference>,
                /// can_migrate_from defines which module versions this module can migrate
                /// state from. The framework will check that one module version is able to
                /// migrate from a previous module version before attempting to update its
                /// config. It is assumed that modules can transitively migrate from earlier
                /// versions. For instance if v3 declares it can migrate from v2, and v2
                /// declares it can migrate from v1, the framework knows how to migrate
                /// from v1 to v3, assuming all 3 module versions are registered at runtime.
                #[prost(message, repeated, tag = "3")]
                pub can_migrate_from: ::prost::alloc::vec::Vec<MigrateFromInfo>,
            }
            /// PackageReference is a reference to a protobuf package used by a module.
            #[allow(clippy::derive_partial_eq_without_eq)]
            #[derive(Clone, PartialEq, ::prost::Message)]
            pub struct PackageReference {
                /// name is the fully-qualified name of the package.
                #[prost(string, tag = "1")]
                pub name: ::prost::alloc::string::String,
                /// revision is the optional revision of the package that is being used.
                /// Protobuf packages used in Cosmos should generally have a major version
                /// as the last part of the package name, ex. foo.bar.baz.v1.
                /// The revision of a package can be thought of as the minor version of a
                /// package which has additional backwards compatible definitions that weren't
                /// present in a previous version.
                ///
                /// A package should indicate its revision with a source code comment
                /// above the package declaration in one of its files containing the
                /// text "Revision N" where N is an integer revision. All packages start
                /// at revision 0 the first time they are released in a module.
                ///
                /// When a new version of a module is released and items are added to existing
                /// .proto files, these definitions should contain comments of the form
                /// "Since: Revision N" where N is an integer revision.
                ///
                /// When the module runtime starts up, it will check the pinned proto
                /// image and panic if there are runtime protobuf definitions that are not
                /// in the pinned descriptor which do not have
                /// a "Since Revision N" comment or have a "Since Revision N" comment where
                /// N is <= to the revision specified here. This indicates that the protobuf
                /// files have been updated, but the pinned file descriptor hasn't.
                ///
                /// If there are items in the pinned file descriptor with a revision
                /// greater than the value indicated here, this will also cause a panic
                /// as it may mean that the pinned descriptor for a legacy module has been
                /// improperly updated or that there is some other versioning discrepancy.
                /// Runtime protobuf definitions will also be checked for compatibility
                /// with pinned file descriptors to make sure there are no incompatible changes.
                ///
                /// This behavior ensures that:
                /// * pinned proto images are up-to-date
                /// * protobuf files are carefully annotated with revision comments which
                /// ```text
                /// are important good client UX
                /// ```
                /// * protobuf files are changed in backwards and forwards compatible ways
                #[prost(uint32, tag = "2")]
                pub revision: u32,
            }
            /// MigrateFromInfo is information on a module version that a newer module
            /// can migrate from.
            #[allow(clippy::derive_partial_eq_without_eq)]
            #[derive(Clone, PartialEq, ::prost::Message)]
            pub struct MigrateFromInfo {
                /// module is the fully-qualified protobuf name of the module config object
                /// for the previous module version, ex: "cosmos.group.module.v1.Module".
                #[prost(string, tag = "1")]
                pub module: ::prost::alloc::string::String,
            }
            // @@protoc_insertion_point(module)
        }
    }
    pub mod msg {
        pub mod v1 {
            // @generated
            // @@protoc_insertion_point(module)
        }
    }
    pub mod base {
        pub mod v1beta1 {
            // @generated
            /// Coin defines a token with a denomination and an amount.
            ///
            /// NOTE: The amount field is an Int which implements the custom method
            /// signatures required by gogoproto.
            #[allow(clippy::derive_partial_eq_without_eq)]
            #[derive(Clone, PartialEq, ::prost::Message)]
            pub struct Coin {
                #[prost(string, tag = "1")]
                pub denom: ::prost::alloc::string::String,
                #[prost(string, tag = "2")]
                pub amount: ::prost::alloc::string::String,
            }
            /// DecCoin defines a token with a denomination and a decimal amount.
            ///
            /// NOTE: The amount field is an Dec which implements the custom method
            /// signatures required by gogoproto.
            #[allow(clippy::derive_partial_eq_without_eq)]
            #[derive(Clone, PartialEq, ::prost::Message)]
            pub struct DecCoin {
                #[prost(string, tag = "1")]
                pub denom: ::prost::alloc::string::String,
                #[prost(string, tag = "2")]
                pub amount: ::prost::alloc::string::String,
            }
            // @@protoc_insertion_point(module)
        }
        pub mod query {
            pub mod v1beta1 {
                // @generated
                /// PageRequest is to be embedded in gRPC request messages for efficient
                /// pagination. Ex:
                ///
                /// ```text
                /// message SomeRequest {
                /// Foo some_parameter = 1;
                /// PageRequest pagination = 2;
                /// }
                /// ```
                #[allow(clippy::derive_partial_eq_without_eq)]
                #[derive(Clone, PartialEq, ::prost::Message)]
                pub struct PageRequest {
                    /// key is a value returned in PageResponse.next_key to begin
                    /// querying the next page most efficiently. Only one of offset or key
                    /// should be set.
                    #[prost(bytes = "bytes", tag = "1")]
                    pub key: ::prost::bytes::Bytes,
                    /// offset is a numeric offset that can be used when key is unavailable.
                    /// It is less efficient than using key. Only one of offset or key should
                    /// be set.
                    #[prost(uint64, tag = "2")]
                    pub offset: u64,
                    /// limit is the total number of results to be returned in the result page.
                    /// If left empty it will default to a value to be set by each app.
                    #[prost(uint64, tag = "3")]
                    pub limit: u64,
                    /// count_total is set to true  to indicate that the result set should include
                    /// a count of the total number of items available for pagination in UIs.
                    /// count_total is only respected when offset is used. It is ignored when key
                    /// is set.
                    #[prost(bool, tag = "4")]
                    pub count_total: bool,
                    /// reverse is set to true if results are to be returned in the descending order.
                    ///
                    /// Since: cosmos-sdk 0.43
                    #[prost(bool, tag = "5")]
                    pub reverse: bool,
                }
                /// PageResponse is to be embedded in gRPC response messages where the
                /// corresponding request message has used PageRequest.
                ///
                /// ```text
                /// message SomeResponse {
                /// repeated Bar results = 1;
                /// PageResponse page = 2;
                /// }
                /// ```
                #[allow(clippy::derive_partial_eq_without_eq)]
                #[derive(Clone, PartialEq, ::prost::Message)]
                pub struct PageResponse {
                    /// next_key is the key to be passed to PageRequest.key to
                    /// query the next page most efficiently. It will be empty if
                    /// there are no more results.
                    #[prost(bytes = "bytes", tag = "1")]
                    pub next_key: ::prost::bytes::Bytes,
                    /// total is total number of results available if PageRequest.count_total
                    /// was set, its value is undefined otherwise
                    #[prost(uint64, tag = "2")]
                    pub total: u64,
                }
                // @@protoc_insertion_point(module)
            }
        }
    }
}
