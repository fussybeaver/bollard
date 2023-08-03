//! GRPC plumbing to interact with Docker's buildkit client
#![cfg(feature = "buildkit")]
#![allow(dead_code)]

use crate::fsutil::types::Packet;
//use crate::moby::filesync::v1::file_sync_server::DiffCopyStream;
//use crate::moby::filesync::v1::file_sync_server::TarStreamStream;
use crate::moby::filesync::v1::BytesMessage as FileSyncBytesMessage;
use crate::moby::filesync::v1::file_sync_server::FileSync;
use crate::moby::filesync::v1::file_send_server::FileSend;
use crate::moby::upload::v1::upload_server::Upload;
use crate::moby::upload::v1::BytesMessage as UploadBytesMessage;
use crate::health::health_check_response::ServingStatus;
use crate::health::health_server::Health;
use crate::health::{HealthCheckRequest, HealthCheckResponse};

use std::collections::HashMap;
use std::pin::Pin;
use std::task::{Context, Poll};

use futures_core::Stream;
use rand::RngCore;
use tokio::io::{AsyncRead, AsyncWrite};
use tonic::transport::server::Connected;
use tonic::{Code, Request, Response, Status, Streaming};
use tower::make::MakeConnection;

#[allow(missing_debug_implementations)]
pub(crate) struct GrpcTransport {
    pub(crate) read: Pin<Box<dyn AsyncRead + Send>>,
    pub(crate) write: Pin<Box<dyn AsyncWrite + Send>>,
}

impl Connected for GrpcTransport {
    type ConnectInfo = ();

    fn connect_info(&self) -> Self::ConnectInfo {}
}

impl AsyncRead for GrpcTransport {
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut tokio::io::ReadBuf<'_>,
    ) -> Poll<std::io::Result<()>> {
        Pin::new(&mut self.read).poll_read(cx, buf)
    }
}

impl AsyncWrite for GrpcTransport {
    fn poll_write(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<std::io::Result<usize>> {
        Pin::new(&mut self.write).poll_write(cx, buf)
    }

    fn poll_flush(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<std::io::Result<()>> {
        Pin::new(&mut self.write).poll_flush(cx)
    }

    fn poll_shutdown(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
    ) -> Poll<Result<(), std::io::Error>> {
        Pin::new(&mut self.write).poll_shutdown(cx)
    }
}

pub(crate) struct HealthServerImpl {
    service_map: HashMap<String, ServingStatus>,
    shutdown: bool,
}

impl HealthServerImpl {
    pub fn new() -> Self {
        let mut service_map = HashMap::new();
        service_map.insert(String::from(""), ServingStatus::Serving);
        Self {
            service_map,
            shutdown: false,
        }
    }

    pub fn shutdown(mut self) {
        self.shutdown = true;
        for (_, val) in self.service_map.iter_mut() {
            *val = ServingStatus::NotServing;
        }
    }
}

#[tonic::async_trait]
impl Health for HealthServerImpl {
    type WatchStream = Pin<Box<dyn Stream<Item = Result<HealthCheckResponse, Status>> + Send>>;
    async fn check(
        &self,
        request: Request<HealthCheckRequest>,
    ) -> Result<Response<HealthCheckResponse>, Status> {
        debug!("Received GRPC Health Request: {:#?}", request);
        if let Some(status) = self.service_map.get(&request.get_ref().service) {
            Ok(Response::new(HealthCheckResponse {
                status: *status as i32,
            }))
        } else {
            Err(Status::new(Code::NotFound, "unknown service"))
        }
    }
    async fn watch(
        &self,
        _request: Request<HealthCheckRequest>,
    ) -> Result<Response<Self::WatchStream>, Status> {
        unimplemented!();
    }
}

pub(crate) struct FileSendImpl {
}

impl FileSendImpl {
    pub fn new() -> Self {
        Self {}
    }
}

#[tonic::async_trait]
impl FileSend for FileSendImpl 
{
    type DiffCopyStream = Pin<Box<dyn Stream<Item = Result<FileSyncBytesMessage, Status>> + Send>>;
    async fn diff_copy(
        &self,
        request: Request<Streaming<FileSyncBytesMessage>>
    ) -> Result<Response<Self::DiffCopyStream>, Status> {
        debug!("Protobuf FileSend diff_copy triggered: {:#?}", request);

        use tokio::sync::mpsc;
        use futures_util::StreamExt;
        use tokio_stream::wrappers::ReceiverStream;
        use std::io::Write;

        let mut in_stream = request.into_inner();
        let (tx, rx) = mpsc::channel(128);


        tokio::spawn(async move {
            let file = std::fs::File::create("/tmp/bollard-oci.dump").expect("unable to create file");
            let mut f = std::io::BufWriter::new(file);
            while let Some(result) = in_stream.next().await {
                match result {
                    Ok(v) => {
                        //debug!("string: {}", &String::from_utf8_lossy(&v.data));
                        f.write_all(&v.data).expect("unable to write data");
                        tx
                        .send(Ok(v))
                        .await
                        .expect("working rx")
                    },
                    Err(err) => {
                        unimplemented!("foo");
                    }
                }
            }
        });

        let out_stream = ReceiverStream::new(rx);

        Ok(Response::new(Box::pin(out_stream)))
        //unimplemented!("Need to implement diff_copy");
    }

}

use std::io::Read;

pub(crate) struct UploadProvider {
    pub(crate) store: HashMap<String, Vec<u8>>
}

impl UploadProvider {
    pub fn new() -> Self {
        Self {
            store: HashMap::new(),
        }
    }

    pub fn add(&mut self, reader: Vec<u8>) -> String {
        let id = new_id();
        let key = format!("http://buildkit-session/{}", id);

        self.store.insert(format!("/{}", id), reader);
        key
    }
}

#[tonic::async_trait]
impl Upload for UploadProvider {
        type PullStream = Pin<Box<dyn Stream<Item = Result<UploadBytesMessage, Status>> + Send>>;

        async fn pull(
            &self,
            request: Request<Streaming<UploadBytesMessage>>,
        ) -> Result<Response<Self::PullStream>, Status> {
            let key = request.metadata().get("urlpath").unwrap();
            debug!("found metadata key {:#?}", key);

            let str: String = String::from(key.to_str().unwrap());
            debug!("hashmap... {:#?}", self.store.keys());
            
            let read: &Vec<u8> = self.store.get(&str).unwrap();
            debug!("trying to pull... {:#?}", request);

            let out_stream = futures_util::stream::once(futures_util::future::ok(UploadBytesMessage {
                data: read.to_owned(),
            }));

            Ok(Response::new(Box::pin(out_stream)))
        }
}

use http::StatusCode;
use std::future::Future;
use tower::Service;
use http::request::Builder;
use hyper::{body::Bytes, Body, Method};

pub(crate) struct GrpcClient {
    pub(crate) client: crate::Docker,
    pub(crate) session_id: String,
}

impl Service<http::Uri> for GrpcClient {
    type Response = GrpcTransport;
    type Error = crate::errors::Error;
    type Future = Pin<Box<dyn Future<Output = Result<Self::Response, Self::Error>> + Send>>;

    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        Poll::Ready(Ok(()))
    }

    fn call(&mut self, req: http::Uri) -> Self::Future {
        // create the body
        let opt: Option<serde_json::Value> = None;
        let url = "/grpc";
        let client = self.client.clone();
        let req = client.build_request(
            &url,
            Builder::new()
                .method(Method::POST)
                .header("Connection", "Upgrade")
                .header("Upgrade", "h2c")
                .header("X-Docker-Expose-Session-Uuid", &self.session_id),
            opt,
            Ok(Body::empty()),
        );
        let fut = async move {
            client.process_upgraded(req).await.and_then(|(read, write)| {
                debug!("process upgraded");
                        let output = Box::pin(read);
                        let input = Box::pin(write);
                        Ok(GrpcTransport {
                            read: output,
                            write: input,
                        })
                    })
        };

        // Return the response as an immediate future
        Box::pin(fut)
    }
}

// Reference: https://github.com/moby/buildkit/blob/master/identity/randomid.go
pub(crate) fn new_id() -> String {
    let mut p: [u8; 17] = Default::default();
    rand::thread_rng().fill_bytes(&mut p);
    p[0] |= 0x80; // set high bit to avoid the need for padding
    num::BigInt::from_bytes_be(num::bigint::Sign::Plus, &p[..]).to_str_radix(36)[1..26].to_string()
}

#[cfg(test)]
mod tests {

    #[test]
    fn test_new_id() {
        let s = super::new_id();
        assert_eq!(s.len(), 25);
    }
}
