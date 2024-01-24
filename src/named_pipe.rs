#![cfg(windows)]

use hyper::rt::ReadBufCursor;
use hyper_util::client::legacy::connect::{Connected, Connection};
use hyper_util::rt::TokioIo;
use pin_project_lite::pin_project;
use tokio::io::AsyncWrite;
use tokio::net::windows::named_pipe::{ClientOptions, NamedPipeClient};
use tokio::time;

use std::ffi::OsStr;
use std::future::Future;
use std::io;
use std::path::Path;
use std::pin::Pin;
use std::task::{Context, Poll};
use std::time::Duration;

use winapi::shared::winerror;

use crate::docker::ClientType;
use crate::uri::Uri;

pin_project! {
    pub struct NamedPipeStream {
        #[pin]
        io: NamedPipeClient,
    }
}

impl NamedPipeStream {
    pub async fn connect<A>(addr: A) -> Result<NamedPipeStream, io::Error>
    where
        A: AsRef<Path> + AsRef<OsStr>,
    {
        let opts = ClientOptions::new();

        let client = loop {
            match opts.open(&addr) {
                Ok(client) => break client,
                Err(e) if e.raw_os_error() == Some(winerror::ERROR_PIPE_BUSY as i32) => (),
                Err(e) => return Err(e),
            };

            time::sleep(Duration::from_millis(50)).await;
        };

        Ok(NamedPipeStream { io: client })
    }
}

impl hyper::rt::Read for NamedPipeStream {
    fn poll_read(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: ReadBufCursor<'_>,
    ) -> Poll<io::Result<()>> {
        let mut t = TokioIo::new(self.project().io);
        Pin::new(&mut t).poll_read(cx, buf)
    }
}

impl hyper::rt::Write for NamedPipeStream {
    fn poll_write(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<io::Result<usize>> {
        Pin::new(&mut self.io).poll_write(cx, buf)
    }

    fn poll_write_vectored(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        bufs: &[io::IoSlice<'_>],
    ) -> Poll<io::Result<usize>> {
        Pin::new(&mut self.io).poll_write_vectored(cx, bufs)
    }

    fn poll_flush(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        Poll::Ready(Ok(()))
    }

    fn poll_shutdown(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        self.poll_flush(cx)
    }
}

#[derive(Clone, Copy, Debug)]
pub struct NamedPipeConnector;

impl tower_service::Service<hyper::Uri> for NamedPipeConnector {
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
                _ => Err(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    format!("Invalid scheme {:?}", destination.scheme()),
                )),
            }?;

            match Uri::socket_path_dest(&destination, &ClientType::NamedPipe) {
                Some(path) => Ok(NamedPipeStream::connect(&path).await?),

                None => Err(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    format!("Invalid uri {:?}", destination),
                )),
            }
        };

        Box::pin(fut)
    }
}

impl Connection for NamedPipeStream {
    fn connected(&self) -> Connected {
        Connected::new()
    }
}

const NAMED_PIPE_SCHEME: &str = "net.pipe";
