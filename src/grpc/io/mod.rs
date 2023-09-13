#![cfg(feature = "buildkit")]

use std::{
    pin::Pin,
    task::{Context, Poll},
};

use tokio::io::{AsyncRead, AsyncWrite};
use tokio_util::codec::FramedRead;
use tonic::transport::server::Connected;

use crate::read::NewlineLogOutputDecoder;

use self::into_async_read::IntoAsyncRead;

pub(crate) mod into_async_read;
pub(crate) mod reader_stream;

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

#[allow(missing_debug_implementations)]
/// An AsyncRead/AsyncWrite type allowing reads along a docker container exec pipe.
pub struct GrpcFramedTransport {
    read: IntoAsyncRead<FramedRead<Pin<Box<dyn AsyncRead + Send>>, NewlineLogOutputDecoder>>,
    write: Pin<Box<dyn AsyncWrite + Send>>,
}

impl GrpcFramedTransport {
    pub(crate) fn new(
        read: Pin<Box<dyn AsyncRead + Send>>,
        write: Pin<Box<dyn AsyncWrite + Send>>,
        capacity: usize,
    ) -> Self {
        let output = FramedRead::with_capacity(read, NewlineLogOutputDecoder::new(true), capacity);
        let read = IntoAsyncRead::new(output);
        Self { read, write }
    }
}

impl Connected for GrpcFramedTransport {
    type ConnectInfo = ();

    fn connect_info(&self) -> Self::ConnectInfo {}
}

impl AsyncRead for GrpcFramedTransport {
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut tokio::io::ReadBuf<'_>,
    ) -> Poll<std::io::Result<()>> {
        Pin::new(&mut self.read).poll_read(cx, buf)
    }
}

impl AsyncWrite for GrpcFramedTransport {
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
