#![cfg(feature = "buildkit_providerless")]

use std::{
    io,
    pin::Pin,
    task::{Context, Poll},
};

use bytes::{Bytes, BytesMut};
use hyper::rt::{Read, Write};
use log::trace;
use tokio::io::{AsyncRead, AsyncWrite};
use tokio_util::codec::{Decoder, FramedRead};
use tonic::transport::server::Connected;

use self::into_async_read::IntoAsyncRead;

pub(crate) mod into_async_read;
pub(crate) mod reader_stream;

/// Keep Docker exec frames bounded to the configured tonic receive limit.
const MAX_DOCKER_EXEC_FRAME_SIZE: usize = 16 << 20;

#[repr(u8)]
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
enum DockerExecStreamType {
    Stdout = 1,
    Stderr = 2,
}

impl TryFrom<u8> for DockerExecStreamType {
    type Error = io::Error;

    fn try_from(value: u8) -> Result<Self, Self::Error> {
        match value {
            1 => Ok(Self::Stdout),
            2 => Ok(Self::Stderr),
            value => Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!("unexpected Docker exec stream type: {value}"),
            )),
        }
    }
}

#[derive(Debug, Copy, Clone)]
enum DockerExecDecoderState {
    WaitingHeader,
    WaitingPayload {
        stream_type: DockerExecStreamType,
        length: usize,
    },
}

#[derive(Debug, Copy, Clone)]
struct DockerExecDecoder {
    state: DockerExecDecoderState,
}

impl DockerExecDecoder {
    fn new() -> Self {
        Self {
            state: DockerExecDecoderState::WaitingHeader,
        }
    }
}

impl Decoder for DockerExecDecoder {
    type Item = Bytes;
    type Error = io::Error;

    fn decode(&mut self, src: &mut BytesMut) -> Result<Option<Self::Item>, Self::Error> {
        loop {
            match self.state {
                DockerExecDecoderState::WaitingHeader => {
                    if src.len() < 8 {
                        return Ok(None);
                    }

                    let header = src.split_to(8);
                    let stream_type = DockerExecStreamType::try_from(header[0])?;
                    if header[1] != 0 || header[2] != 0 || header[3] != 0 {
                        return Err(io::Error::new(
                            io::ErrorKind::InvalidData,
                            "non-zero reserved bytes in Docker exec stream header",
                        ));
                    }

                    let length =
                        u32::from_be_bytes([header[4], header[5], header[6], header[7]]) as usize;
                    if length > MAX_DOCKER_EXEC_FRAME_SIZE {
                        return Err(io::Error::new(
                            io::ErrorKind::InvalidData,
                            format!(
                                "Docker exec frame exceeds maximum size: {length} > {MAX_DOCKER_EXEC_FRAME_SIZE}"
                            ),
                        ));
                    }
                    self.state = DockerExecDecoderState::WaitingPayload {
                        stream_type,
                        length,
                    };
                }
                DockerExecDecoderState::WaitingPayload {
                    stream_type,
                    length,
                } => {
                    if src.len() < length {
                        return Ok(None);
                    }

                    let payload = src.split_to(length).freeze();
                    self.state = DockerExecDecoderState::WaitingHeader;

                    if stream_type == DockerExecStreamType::Stdout {
                        if !payload.is_empty() {
                            return Ok(Some(payload));
                        }
                    } else {
                        trace!("discarding {} bytes from Docker exec stderr", payload.len());
                    }
                }
            }
        }
    }

    fn decode_eof(&mut self, src: &mut BytesMut) -> Result<Option<Self::Item>, Self::Error> {
        if let Some(item) = self.decode(src)? {
            return Ok(Some(item));
        }

        if src.is_empty() && matches!(self.state, DockerExecDecoderState::WaitingHeader) {
            return Ok(None);
        }

        Err(io::Error::new(
            io::ErrorKind::UnexpectedEof,
            "truncated Docker exec stream frame",
        ))
    }
}

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
/// An AsyncRead/AsyncWrite transport for a Docker exec pipe with stdout demultiplexing.
pub struct GrpcFramedTransport {
    read: IntoAsyncRead<FramedRead<Pin<Box<dyn AsyncRead + Send>>, DockerExecDecoder>>,
    write: Pin<Box<dyn AsyncWrite + Send>>,
}

impl GrpcFramedTransport {
    pub(crate) fn new(
        read: Pin<Box<dyn AsyncRead + Send>>,
        write: Pin<Box<dyn AsyncWrite + Send>>,
        capacity: usize,
    ) -> Self {
        let output = FramedRead::with_capacity(read, DockerExecDecoder::new(), capacity);
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
    use bytes::BytesMut;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio_util::codec::Decoder;

    use super::{
        DockerExecDecoder, DockerExecDecoderState, DockerExecStreamType, GrpcFramedTransport,
        MAX_DOCKER_EXEC_FRAME_SIZE,
    };

    fn frame_header(stream_type: u8, length: usize) -> Vec<u8> {
        let mut frame = Vec::with_capacity(8);
        frame.extend_from_slice(&[stream_type, 0, 0, 0]);
        frame.extend_from_slice(&(length as u32).to_be_bytes());
        frame
    }

    fn frame(stream_type: DockerExecStreamType, payload: &[u8]) -> Vec<u8> {
        let mut frame = frame_header(stream_type as u8, payload.len());
        frame.extend_from_slice(payload);
        frame
    }

    #[tokio::test]
    async fn grpc_framed_transport_forwards_stdout_and_discards_stderr() {
        let stdout = b"payload\n\0\xff";
        let stdout_tail = b"tail";
        let stderr = b"buildctl diagnostic\n";
        let mut input = frame(DockerExecStreamType::Stderr, stderr);
        input.extend_from_slice(&frame(DockerExecStreamType::Stdout, stdout));
        input.extend_from_slice(&frame(DockerExecStreamType::Stdout, stdout_tail));

        let (mut source, reader) = tokio::io::duplex(input.len());
        let writer = tokio::spawn(async move {
            for chunk in input.chunks(3) {
                source.write_all(chunk).await.unwrap();
            }
            source.shutdown().await.unwrap();
        });

        let mut transport =
            GrpcFramedTransport::new(Box::pin(reader), Box::pin(tokio::io::sink()), 8 * 1024);
        let mut actual = Vec::new();
        transport.read_to_end(&mut actual).await.unwrap();

        writer.await.unwrap();
        assert_eq!(actual, [stdout.as_slice(), stdout_tail.as_slice()].concat());
    }

    #[test]
    fn docker_exec_decoder_rejects_unknown_stream_types() {
        let mut decoder = DockerExecDecoder::new();
        let mut src = BytesMut::from(&frame_header(3, 7)[..]);

        let error = decoder.decode(&mut src).unwrap_err();
        assert_eq!(error.kind(), std::io::ErrorKind::InvalidData);
    }

    #[test]
    fn docker_exec_decoder_rejects_non_zero_reserved_header_bytes() {
        let mut decoder = DockerExecDecoder::new();
        let mut src = BytesMut::from(&[1, 0, 1, 0, 0, 0, 0, 0][..]);

        let error = decoder.decode(&mut src).unwrap_err();
        assert_eq!(error.kind(), std::io::ErrorKind::InvalidData);
    }

    #[test]
    fn docker_exec_decoder_rejects_truncated_frames() {
        let mut decoder = DockerExecDecoder::new();
        let mut src = BytesMut::from(
            &frame_header(DockerExecStreamType::Stdout as u8, b"payload".len())[..5],
        );

        let error = decoder.decode_eof(&mut src).unwrap_err();
        assert_eq!(error.kind(), std::io::ErrorKind::UnexpectedEof);
    }

    #[test]
    fn docker_exec_decoder_accepts_maximum_frame_length() {
        let mut decoder = DockerExecDecoder::new();
        let mut src = BytesMut::from(
            &frame_header(
                DockerExecStreamType::Stdout as u8,
                MAX_DOCKER_EXEC_FRAME_SIZE,
            )[..],
        );

        assert!(decoder.decode(&mut src).unwrap().is_none());
        assert!(matches!(
            decoder.state,
            DockerExecDecoderState::WaitingPayload {
                stream_type: DockerExecStreamType::Stdout,
                length: MAX_DOCKER_EXEC_FRAME_SIZE,
            }
        ));
    }

    #[test]
    fn docker_exec_decoder_rejects_frame_length_above_maximum() {
        let mut decoder = DockerExecDecoder::new();
        let mut src = BytesMut::from(
            &frame_header(
                DockerExecStreamType::Stdout as u8,
                MAX_DOCKER_EXEC_FRAME_SIZE + 1,
            )[..],
        );

        let error = decoder.decode(&mut src).unwrap_err();
        assert_eq!(error.kind(), std::io::ErrorKind::InvalidData);
    }
}
