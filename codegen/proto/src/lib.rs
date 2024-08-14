#![allow(missing_docs, unused_qualifications)]
#![cfg(not(feature = "build"))]

pub mod fsutil {
    pub mod types {
        include!("generated/fsutil.types.rs");
    }
}

pub mod google {
    pub mod protobuf {
        include!("generated/google.protobuf.rs");
    }
    pub mod rpc {
        include!("generated/google.rpc.rs");
    }
}

pub mod health {
    include!("generated/grpc.health.v1.rs");
}

pub mod moby {
    pub mod buildkit {
        pub mod secrets {
            pub mod v1 {
                include!("generated/moby.buildkit.secrets.v1.rs");
            }
        }
        pub mod v1 {
            include!("generated/moby.buildkit.v1.rs");
            pub mod sourcepolicy {
                include!("generated/moby.buildkit.v1.sourcepolicy.rs");
            }
            pub mod types {
                include!("generated/moby.buildkit.v1.types.rs");
            }
        }
    }
    pub mod filesync {
        pub mod v1 {
            include!("generated/moby.filesync.v1.rs");
            
            use tonic::codegen::*;
            /// Generated trait containing gRPC methods that should be implemented for use with FileSendServer.
            #[async_trait]
            pub trait FileSendPacket: Send + Sync + 'static {
                /// Server streaming response type for the DiffCopy method.
                type DiffCopyStream: tonic::codegen::tokio_stream::Stream<
                        Item = std::result::Result<crate::fsutil::types::Packet, tonic::Status>,
                    > + Send
                    + 'static;
                async fn diff_copy(
                    &self,
                    request: tonic::Request<tonic::Streaming<crate::fsutil::types::Packet>>,
                ) -> std::result::Result<tonic::Response<Self::DiffCopyStream>, tonic::Status>;
            }
            /// FileSend allows sending files from the server back to the client.
            #[derive(Debug)]
            pub struct FileSendPacketServer<T: FileSendPacket> {
                inner: _Inner<T>,
                accept_compression_encodings: EnabledCompressionEncodings,
                send_compression_encodings: EnabledCompressionEncodings,
                max_decoding_message_size: Option<usize>,
                max_encoding_message_size: Option<usize>,
            }
            struct _Inner<T>(Arc<T>);
            impl<T: FileSendPacket> FileSendPacketServer<T> {
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
                pub fn with_interceptor<F>(inner: T, interceptor: F) -> InterceptedService<Self, F>
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
            impl<T, B> tonic::codegen::Service<http::Request<B>> for FileSendPacketServer<T>
            where
                T: FileSendPacket,
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
                    let _inner = self.inner.clone();
                    match req.uri().path() {
                        "/moby.filesync.v1.FileSend/DiffCopy" => {
                            #[allow(non_camel_case_types)]
                            struct DiffCopySvc<T: FileSendPacket>(pub Arc<T>);
                            impl<T: FileSendPacket>
                                tonic::server::StreamingService<crate::fsutil::types::Packet>
                                for DiffCopySvc<T>
                            {
                                type Response = crate::fsutil::types::Packet;
                                type ResponseStream = T::DiffCopyStream;
                                type Future =
                                    BoxFuture<tonic::Response<Self::ResponseStream>, tonic::Status>;
                                fn call(
                                    &mut self,
                                    request: tonic::Request<tonic::Streaming<crate::fsutil::types::Packet>>,
                                ) -> Self::Future {
                                    let inner = Arc::clone(&self.0);
                                    let fut = async move {
                                        <T as FileSendPacket>::diff_copy(&inner, request).await
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
                                let method = DiffCopySvc(inner);
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
            impl<T: FileSendPacket> Clone for FileSendPacketServer<T> {
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
            impl<T: FileSendPacket> Clone for _Inner<T> {
                fn clone(&self) -> Self {
                    Self(Arc::clone(&self.0))
                }
            }
            impl<T: std::fmt::Debug> std::fmt::Debug for _Inner<T> {
                fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                    write!(f, "{:?}", self.0)
                }
            }
            impl<T: FileSendPacket> tonic::server::NamedService for FileSendPacketServer<T> {
                const NAME: &'static str = "moby.filesync.v1.FileSend";
            }
        }
    }
    pub mod upload {
        pub mod v1 {
            include!("generated/moby.upload.v1.rs");
        }
    }
    pub mod sshforward {
        pub mod v1 {
            include!("generated/moby.sshforward.v1.rs");
        }
    }
}

#[allow(clippy::all)]
pub mod pb {
    include!("generated/pb.rs");
}

use std::fmt::{self, Display, Formatter};

impl Display for moby::buildkit::v1::StatusResponse {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        write!(
            f,
            "StatusResponse: {{ vertexes: {:?}, statuses: {:?}, logs: ",
            self.vertexes, self.statuses
        )
        .and_then(|_| {
            if self.logs.is_empty() {
                write!(f, "[]")
            } else {
                let mut iter = self.logs.iter().peekable();
                let mut next = iter.next();
                let mut result = Ok(());
                while next.is_some() {
                    result = result.and_then(|_| write!(f, "{}", next.unwrap()));
                    next = iter.next();
                    if iter.peek().is_some() {
                        result = result.and_then(|_| write!(f, ", "));
                    }
                }
                result
            }
        })
        .and_then(|_| write!(f, r#", warnings: {:?} }}"#, self.warnings))
    }
}

impl Display for moby::buildkit::v1::VertexLog {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        write!(
            f,
            r#"VertexLog: {{ vertex: {:?}, timestamp: {:?}, stream: {:?}, msg: \"{}\" }}"#,
            self.vertex,
            self.timestamp,
            self.stream,
            String::from_utf8_lossy(&self.msg).trim(),
        )
    }
}

impl AsRef<[u8]> for moby::buildkit::v1::BytesMessage {
    fn as_ref(&self) -> &[u8] {
        self.data.as_ref()
    }
}
