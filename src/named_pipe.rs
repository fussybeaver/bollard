#![cfg(windows)]

use hyper::client::connect::Connected;
use pin_project::pin_project;
use tokio::io::{AsyncRead, AsyncWrite, ReadBuf};
use tokio::net::NamedPipe;

use std::fmt;
use std::future::Future;
use std::io;
use std::path::Path;
use std::pin::Pin;
use std::task::{Context, Poll};

use crate::docker::ClientType;
use crate::uri::Uri;

#[pin_project]
pub struct NamedPipeStream {
    #[pin]
    io: NamedPipe
}

impl NamedPipeStream {
    pub async fn connect<A>(addr: A) -> Result<NamedPipeStream, io::Error>
    where
        A: AsRef<Path>,
    {
        let io = NamedPipe::connect(addr.as_ref().as_os_str()).await?;

        Ok(NamedPipeStream { io })
    }
}

impl AsyncRead for NamedPipeStream {

    fn poll_read(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<Result<(), io::Error>> {
        self.project().io.poll_read(cx, buf)
    }
}

impl AsyncWrite for NamedPipeStream {
    fn poll_shutdown(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), io::Error>> {
        self.project().io.poll_shutdown(cx)
    }

    fn poll_write(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        bytes: &[u8],
    ) -> Poll<Result<usize, io::Error>> {
        self.project().io.poll_write(cx, bytes)
    }

    fn poll_flush(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), io::Error>> {
        self.project().io.poll_flush(cx)
    }
}

impl fmt::Debug for NamedPipeStream {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.io.fmt(f)
    }
}

#[derive(Clone, Copy, Debug)]
pub struct NamedPipeConnector;

impl hyper::service::Service<hyper::Uri> for NamedPipeConnector {
    type Response = NamedPipeStream;
    type Error = io::Error;
    type Future =
        Pin<Box<dyn Future<Output = Result<Self::Response, Self::Error>> + Send + 'static>>;

    fn poll_ready(&mut self, _cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        Poll::Ready(Ok(()))
    }

    fn call(&mut self, destination: hyper::Uri) -> Self::Future {
        let fut = async move {
            match destination.scheme() {
                Some(scheme) if scheme == NAMED_PIPE_SCHEME => Ok(()),
                _ => {
                    Err(io::Error::new(
                        io::ErrorKind::InvalidInput,
                        format!("Invalid scheme {:?}", destination.scheme()),
                    ))
                }
            }?;

            match Uri::socket_path_dest(&destination, &ClientType::NamedPipe) {
                Some(path) => Ok(NamedPipeStream::connect(&path).await?),

                None => {
                    Err(io::Error::new(
                        io::ErrorKind::InvalidInput,
                        format!("Invalid uri {:?}", destination),
                    ))
                }
            }
        };

        Box::pin(fut)
    }
}

impl hyper::client::connect::Connection for NamedPipeStream {
    fn connected(&self) -> Connected {
        Connected::new()
    }
}

const NAMED_PIPE_SCHEME: &str = "net.pipe";
