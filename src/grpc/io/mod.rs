#![cfg(feature = "buildkit_providerless")]

use std::{
    pin::Pin,
    task::{Context, Poll},
};

use hyper::rt::{Read, Write};
use tokio::io::{AsyncRead, AsyncWrite};
use tokio_util::codec::FramedRead;
use tonic::transport::server::Connected;

use crate::read::NewlineLogOutputDecoder;

use self::into_async_read::IntoAsyncRead;

pub(crate) mod into_async_read;
pub(crate) mod reader_stream;

#[allow(missing_debug_implementations)]
/// An unframed asynchronous transport for a BuildKit gRPC connection.
pub struct GrpcTransport {
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

impl Read for GrpcTransport {
    fn poll_read(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        mut buf: hyper::rt::ReadBufCursor<'_>,
    ) -> Poll<Result<(), std::io::Error>> {
        let n = unsafe {
            let mut tbuf = tokio::io::ReadBuf::uninit(buf.as_mut());
            match tokio::io::AsyncRead::poll_read(self, cx, &mut tbuf) {
                Poll::Ready(Ok(())) => tbuf.filled().len(),
                other => return other,
            }
        };

        unsafe {
            buf.advance(n);
        }
        Poll::Ready(Ok(()))
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

impl Write for GrpcTransport {
    fn poll_write(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<Result<usize, std::io::Error>> {
        tokio::io::AsyncWrite::poll_write(self, cx, buf)
    }

    fn poll_flush(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), std::io::Error>> {
        tokio::io::AsyncWrite::poll_flush(self, cx)
    }

    fn poll_shutdown(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
    ) -> Poll<Result<(), std::io::Error>> {
        tokio::io::AsyncWrite::poll_shutdown(self, cx)
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

impl Read for GrpcFramedTransport {
    fn poll_read(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        mut buf: hyper::rt::ReadBufCursor<'_>,
    ) -> Poll<Result<(), std::io::Error>> {
        let n = unsafe {
            let mut tbuf = tokio::io::ReadBuf::uninit(buf.as_mut());
            match tokio::io::AsyncRead::poll_read(self, cx, &mut tbuf) {
                Poll::Ready(Ok(())) => tbuf.filled().len(),
                other => return other,
            }
        };

        unsafe {
            buf.advance(n);
        }
        Poll::Ready(Ok(()))
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

impl Write for GrpcFramedTransport {
    fn poll_write(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<Result<usize, std::io::Error>> {
        tokio::io::AsyncWrite::poll_write(self, cx, buf)
    }

    fn poll_flush(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), std::io::Error>> {
        tokio::io::AsyncWrite::poll_flush(self, cx)
    }

    fn poll_shutdown(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
    ) -> Poll<Result<(), std::io::Error>> {
        tokio::io::AsyncWrite::poll_shutdown(self, cx)
    }
}

#[cfg(test)]
mod tests {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    use super::GrpcTransport;

    #[tokio::test]
    async fn grpc_transport_preserves_newline_and_binary_data() {
        let expected = b"\0\0\0\0\0\0\0\x08payload\n\xff\x01\nlast".to_vec();
        let (mut source, reader) = tokio::io::duplex(expected.len());
        let payload = expected.clone();

        let writer = tokio::spawn(async move {
            source.write_all(&payload).await.unwrap();
            source.shutdown().await.unwrap();
        });

        let mut transport = GrpcTransport {
            read: Box::pin(reader),
            write: Box::pin(tokio::io::sink()),
        };
        let mut actual = Vec::new();
        transport.read_to_end(&mut actual).await.unwrap();

        writer.await.unwrap();
        assert_eq!(actual, expected);
    }
}
