use bytes::Buf;
use bytes::BytesMut;
use futures_core::Stream;
use hyper::body::Body;
use hyper::body::Bytes;
use hyper::body::Incoming;
use hyper::upgrade::Upgraded;
use log::debug;
use log::trace;
use pin_project_lite::pin_project;
use serde::de::DeserializeOwned;
use std::pin::Pin;
use std::task::{Context, Poll};
use std::{cmp, io, marker::PhantomData};

use tokio::io::AsyncWrite;
use tokio::io::{AsyncRead, ReadBuf};
use tokio_util::codec::Decoder;

use crate::container::LogOutput;

use crate::errors::Error;
use crate::errors::Error::JsonDataError;

#[derive(Debug, Copy, Clone)]
enum NewlineLogOutputDecoderState {
    WaitingHeader,
    WaitingPayload(u8, usize), // StreamType, Length
}

#[derive(Debug, Copy, Clone)]
pub(crate) struct NewlineLogOutputDecoder {
    state: NewlineLogOutputDecoderState,
    is_tcp: bool,
}

impl NewlineLogOutputDecoder {
    pub(crate) fn new(is_tcp: bool) -> NewlineLogOutputDecoder {
        NewlineLogOutputDecoder {
            state: NewlineLogOutputDecoderState::WaitingHeader,
            is_tcp,
        }
    }
}

impl Decoder for NewlineLogOutputDecoder {
    type Item = LogOutput;
    type Error = io::Error;

    fn decode(&mut self, src: &mut BytesMut) -> Result<Option<Self::Item>, Self::Error> {
        loop {
            match self.state {
                NewlineLogOutputDecoderState::WaitingHeader => {
                    // `start_exec` API on unix socket will emit values without a header
                    if !src.is_empty() && src[0] > 2 {
                        if self.is_tcp {
                            return Ok(Some(LogOutput::Console {
                                message: src.split().freeze(),
                            }));
                        }
                        let nl_index = src.iter().position(|b| *b == b'\n');
                        if let Some(pos) = nl_index {
                            return Ok(Some(LogOutput::Console {
                                message: src.split_to(pos + 1).freeze(),
                            }));
                        } else {
                            return Ok(None);
                        }
                    }

                    if src.len() < 8 {
                        return Ok(None);
                    }

                    let header = src.split_to(8);
                    let length =
                        u32::from_be_bytes([header[4], header[5], header[6], header[7]]) as usize;
                    self.state = NewlineLogOutputDecoderState::WaitingPayload(header[0], length);
                }
                NewlineLogOutputDecoderState::WaitingPayload(typ, length) => {
                    if src.len() < length {
                        return Ok(None);
                    } else {
                        trace!("NewlineLogOutputDecoder: Reading payload");
                        let message = src.split_to(length).freeze();
                        let item = match typ {
                            0 => LogOutput::StdIn { message },
                            1 => LogOutput::StdOut { message },
                            2 => LogOutput::StdErr { message },
                            _ => unreachable!(),
                        };

                        self.state = NewlineLogOutputDecoderState::WaitingHeader;
                        return Ok(Some(item));
                    }
                }
            }
        }
    }

    fn decode_eof(&mut self, src: &mut BytesMut) -> Result<Option<Self::Item>, Self::Error> {
        // Try a normal decode first (handles any complete frames still in the buffer).
        if let Some(item) = self.decode(src)? {
            return Ok(Some(item));
        }
        // At EOF, flush whatever is left as a Console message rather than letting
        // FramedRead error with "bytes remaining on stream".  This is the common
        // case for TTY containers whose final output line has no trailing newline.
        if !src.is_empty() {
            debug!(
                "NewlineLogOutputDecoder::decode_eof: flushing {} trailing bytes: {:?}",
                src.len(),
                src
            );
            return Ok(Some(LogOutput::Console {
                message: src.split().freeze(),
            }));
        }
        Ok(None)
    }
}

pin_project! {
    #[derive(Debug)]
    pub(crate) struct JsonLineDecoder<T> {
        ty: PhantomData<T>,
    }
}

impl<T> JsonLineDecoder<T> {
    #[inline]
    pub(crate) fn new() -> JsonLineDecoder<T> {
        JsonLineDecoder { ty: PhantomData }
    }
}

impl<T> Decoder for JsonLineDecoder<T>
where
    T: DeserializeOwned,
{
    type Item = T;
    type Error = Error;

    // Docker streams whitespace-separated JSON values (progress messages are
    // CRLF-terminated). Rather than split on '\n' and reason about the leftover
    // bytes -- which mishandles a "\r\n" terminator that arrives split from its
    // value across a read boundary, orphaning a lone '\r' that later surfaces as
    // a spurious "bytes remaining on stream" (#560) -- parse one value at a time
    // with serde's StreamDeserializer, which skips any inter-value whitespace
    // ('\r', '\n', spaces, blank lines) for free.
    fn decode(&mut self, src: &mut BytesMut) -> Result<Option<Self::Item>, Self::Error> {
        decode_stream(src, false)
    }

    // At EOF a trailing value need not be whitespace-terminated, so an
    // incomplete-value error on the remainder is only a real truncation if
    // non-whitespace bytes remain; pure trailing whitespace (e.g. a '\r' whose
    // '\n' never arrived) is a clean end.
    fn decode_eof(&mut self, src: &mut BytesMut) -> Result<Option<Self::Item>, Self::Error> {
        decode_stream(src, true)
    }
}

/// Pulls the next whitespace-separated JSON value out of `src`. `result` is that
/// value (`Some(Ok)`), a decode error (`Some(Err)`, which is `is_eof()` when the
/// value is only partially buffered), or `None` when the buffer holds nothing
/// but whitespace. `consumed` is how many bytes that step read: leading
/// whitespace plus the value, with any trailing bytes left in place.
fn decode_stream<T: DeserializeOwned>(src: &mut BytesMut, eof: bool) -> Result<Option<T>, Error> {
    let (result, consumed) = {
        let mut stream = serde_json::Deserializer::from_slice(src).into_iter::<T>();
        let result = stream.next();
        (result, stream.byte_offset())
    };
    match result {
        Some(Ok(value)) => {
            src.advance(consumed);
            Ok(Some(value))
        }
        // Malformed JSON (not merely incomplete): a genuine protocol error.
        Some(Err(e)) if !e.is_eof() => Err(JsonDataError {
            message: e.to_string(),
            column: e.column(),
            #[cfg(feature = "json_data_content")]
            contents: String::from_utf8_lossy(src).to_string(),
        }),
        // Incomplete next value, or a whitespace-only buffer (StreamDeserializer
        // yields None). Wait for more bytes; at EOF, pure trailing whitespace is
        // a clean end and anything else is a real truncation.
        Some(Err(_)) | None => {
            if eof && !src.iter().all(|b| b.is_ascii_whitespace()) {
                return Err(Error::IOError {
                    err: std::io::Error::other("bytes remaining on stream"),
                });
            }
            if eof {
                src.clear();
            }
            Ok(None)
        }
    }
}

#[derive(Debug)]
enum ReadState {
    Ready(Bytes, usize),
    NotReady,
}

pin_project! {
    #[derive(Debug)]
    pub(crate) struct StreamReader {
        #[pin]
        stream: Incoming,
        state: ReadState,
    }
}

impl StreamReader {
    #[inline]
    pub(crate) fn new(stream: Incoming) -> StreamReader {
        StreamReader {
            stream,
            state: ReadState::NotReady,
        }
    }
}

impl AsyncRead for StreamReader {
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        read_buf: &mut ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        loop {
            match self.as_mut().project().state {
                ReadState::Ready(ref mut chunk, ref mut pos) => {
                    let chunk_start = *pos;
                    let buf = read_buf.initialize_unfilled();
                    let len = cmp::min(buf.len(), chunk.len() - chunk_start);
                    let chunk_end = chunk_start + len;

                    buf[..len].copy_from_slice(&chunk[chunk_start..chunk_end]);
                    *pos += len;
                    read_buf.advance(len);

                    if *pos != chunk.len() {
                        return Poll::Ready(Ok(()));
                    }
                }

                ReadState::NotReady => match self.as_mut().project().stream.poll_frame(cx) {
                    Poll::Ready(Some(Ok(frame))) if frame.is_data() => {
                        *self.as_mut().project().state =
                            ReadState::Ready(frame.into_data().unwrap(), 0);

                        continue;
                    }
                    Poll::Ready(Some(Ok(_frame))) => return Poll::Ready(Ok(())),
                    Poll::Ready(None) => return Poll::Ready(Ok(())),
                    Poll::Pending => {
                        return Poll::Pending;
                    }
                    Poll::Ready(Some(Err(e))) => {
                        return Poll::Ready(Err(io::Error::other(e.to_string())));
                    }
                },
            }

            *self.as_mut().project().state = ReadState::NotReady;

            return Poll::Ready(Ok(()));
        }
    }
}

pin_project! {
    #[derive(Debug)]
    pub(crate) struct AsyncUpgraded {
        #[pin]
        inner: Upgraded,
    }
}

impl AsyncUpgraded {
    pub(crate) fn new(upgraded: Upgraded) -> Self {
        Self { inner: upgraded }
    }
}

impl AsyncRead for AsyncUpgraded {
    fn poll_read(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        read_buf: &mut ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        let n = {
            let mut hbuf = hyper::rt::ReadBuf::new(read_buf.initialize_unfilled());
            match hyper::rt::Read::poll_read(self.project().inner, cx, hbuf.unfilled()) {
                Poll::Ready(Ok(())) => hbuf.filled().len(),
                other => return other,
            }
        };
        read_buf.advance(n);

        Poll::Ready(Ok(()))
    }
}

impl AsyncWrite for AsyncUpgraded {
    fn poll_write(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<Result<usize, io::Error>> {
        hyper::rt::Write::poll_write(self.project().inner, cx, buf)
    }

    fn poll_flush(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), io::Error>> {
        hyper::rt::Write::poll_flush(self.project().inner, cx)
    }

    fn poll_shutdown(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), io::Error>> {
        hyper::rt::Write::poll_shutdown(self.project().inner, cx)
    }
}

pin_project! {
    #[derive(Debug)]
    pub(crate) struct IncomingStream {
        #[pin]
        inner: Incoming,
    }
}

impl IncomingStream {
    pub(crate) fn new(incoming: Incoming) -> Self {
        Self { inner: incoming }
    }
}

impl Stream for IncomingStream {
    type Item = Result<Bytes, Error>;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        match futures_util::ready!(self.as_mut().project().inner.poll_frame(cx)?) {
            Some(frame) => match frame.into_data() {
                Ok(data) => Poll::Ready(Some(Ok(data))),
                Err(_) => Poll::Ready(None),
            },
            None => Poll::Ready(None),
        }
    }
}

#[cfg(feature = "websocket")]
pub(crate) mod websocket {
    use bytes::{Bytes, BytesMut};
    use futures_core::Stream;
    use futures_util::stream::{SplitSink, SplitStream};
    use pin_project_lite::pin_project;
    use std::cmp;
    use std::io;
    use std::pin::Pin;
    use std::task::{Context, Poll};
    use tokio::io::{AsyncRead, AsyncWrite, ReadBuf};
    use tokio_tungstenite::tungstenite::Message;
    use tokio_tungstenite::WebSocketStream;

    #[derive(Debug)]
    enum ReaderState {
        /// Ready to read from the current chunk at the given position.
        Ready(Bytes, usize),
        /// Waiting for the next WebSocket message.
        Waiting,
        /// The WebSocket stream has been closed.
        Closed,
    }

    pin_project! {
        /// Wraps a WebSocket read stream to implement [`AsyncRead`].
        ///
        /// Reads binary and text WebSocket messages and provides their payloads
        /// as a contiguous byte stream suitable for use with [`FramedRead`](tokio_util::codec::FramedRead).
        #[derive(Debug)]
        pub struct WebSocketReader<S> {
            #[pin]
            stream: SplitStream<WebSocketStream<S>>,
            state: ReaderState,
        }
    }

    impl<S> WebSocketReader<S> {
        /// Create a new `WebSocketReader` from a WebSocket split stream.
        pub fn new(stream: SplitStream<WebSocketStream<S>>) -> Self {
            Self {
                stream,
                state: ReaderState::Waiting,
            }
        }
    }

    impl<S> AsyncRead for WebSocketReader<S>
    where
        S: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin,
    {
        fn poll_read(
            mut self: Pin<&mut Self>,
            cx: &mut Context<'_>,
            read_buf: &mut ReadBuf<'_>,
        ) -> Poll<io::Result<()>> {
            loop {
                match self.as_mut().project().state {
                    ReaderState::Ready(ref chunk, ref mut pos) => {
                        let chunk_start = *pos;
                        let buf = read_buf.initialize_unfilled();
                        let len = cmp::min(buf.len(), chunk.len() - chunk_start);
                        let chunk_end = chunk_start + len;

                        buf[..len].copy_from_slice(&chunk[chunk_start..chunk_end]);
                        *pos += len;
                        read_buf.advance(len);

                        if *pos >= chunk.len() {
                            *self.as_mut().project().state = ReaderState::Waiting;
                        }
                        return Poll::Ready(Ok(()));
                    }
                    ReaderState::Waiting => {
                        match self.as_mut().project().stream.poll_next(cx) {
                            Poll::Ready(Some(Ok(msg))) => match msg {
                                Message::Binary(data) => {
                                    *self.as_mut().project().state = ReaderState::Ready(data, 0);
                                    continue;
                                }
                                Message::Text(text) => {
                                    *self.as_mut().project().state = ReaderState::Ready(
                                        Bytes::copy_from_slice(text.as_bytes()),
                                        0,
                                    );
                                    continue;
                                }
                                Message::Close(_) => {
                                    *self.as_mut().project().state = ReaderState::Closed;
                                    return Poll::Ready(Ok(()));
                                }
                                // Ping/Pong frames are handled by tungstenite automatically
                                Message::Ping(_) | Message::Pong(_) | Message::Frame(_) => {
                                    continue;
                                }
                            },
                            Poll::Ready(Some(Err(e))) => {
                                return Poll::Ready(Err(io::Error::other(e.to_string())));
                            }
                            Poll::Ready(None) => {
                                *self.as_mut().project().state = ReaderState::Closed;
                                return Poll::Ready(Ok(()));
                            }
                            Poll::Pending => {
                                return Poll::Pending;
                            }
                        }
                    }
                    ReaderState::Closed => {
                        return Poll::Ready(Ok(()));
                    }
                }
            }
        }
    }

    pin_project! {
        /// Wraps a WebSocket write sink to implement [`AsyncWrite`].
        ///
        /// Buffers writes and sends the accumulated data as a single binary
        /// WebSocket message when flushed.
        #[derive(Debug)]
        pub struct WebSocketWriter<S> {
            #[pin]
            sink: SplitSink<WebSocketStream<S>, Message>,
            buffer: BytesMut,
        }
    }

    impl<S> WebSocketWriter<S>
    where
        S: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin,
    {
        /// Create a new `WebSocketWriter` from a WebSocket split sink.
        pub fn new(sink: SplitSink<WebSocketStream<S>, Message>) -> Self {
            Self {
                sink,
                buffer: BytesMut::new(),
            }
        }
    }

    impl<S> AsyncWrite for WebSocketWriter<S>
    where
        S: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin,
    {
        fn poll_write(
            self: Pin<&mut Self>,
            _cx: &mut Context<'_>,
            buf: &[u8],
        ) -> Poll<Result<usize, io::Error>> {
            let this = self.project();
            this.buffer.extend_from_slice(buf);
            Poll::Ready(Ok(buf.len()))
        }

        fn poll_flush(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), io::Error>> {
            use futures_util::Sink;

            let mut this = self.project();

            if !this.buffer.is_empty() {
                match this.sink.as_mut().poll_ready(cx) {
                    Poll::Ready(Ok(())) => {}
                    Poll::Ready(Err(e)) => {
                        return Poll::Ready(Err(io::Error::other(e)));
                    }
                    Poll::Pending => return Poll::Pending,
                }

                let data = this.buffer.split().freeze();
                if let Err(e) = this.sink.as_mut().start_send(Message::Binary(data)) {
                    return Poll::Ready(Err(io::Error::other(e)));
                }
            }

            match this.sink.poll_flush(cx) {
                Poll::Ready(Ok(())) => Poll::Ready(Ok(())),
                Poll::Ready(Err(e)) => Poll::Ready(Err(io::Error::other(e))),
                Poll::Pending => Poll::Pending,
            }
        }

        fn poll_shutdown(
            self: Pin<&mut Self>,
            cx: &mut Context<'_>,
        ) -> Poll<Result<(), io::Error>> {
            use futures_util::Sink;

            let mut this = self.project();

            // Flush any remaining buffered data
            if !this.buffer.is_empty() {
                match this.sink.as_mut().poll_ready(cx) {
                    Poll::Ready(Ok(())) => {}
                    Poll::Ready(Err(e)) => {
                        return Poll::Ready(Err(io::Error::other(e)));
                    }
                    Poll::Pending => return Poll::Pending,
                }

                let data = this.buffer.split().freeze();
                if let Err(e) = this.sink.as_mut().start_send(Message::Binary(data)) {
                    return Poll::Ready(Err(io::Error::other(e)));
                }
            }

            // Close the WebSocket connection
            match this.sink.poll_close(cx) {
                Poll::Ready(Ok(())) => Poll::Ready(Ok(())),
                Poll::Ready(Err(e)) => Poll::Ready(Err(io::Error::other(e))),
                Poll::Pending => Poll::Pending,
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use bytes::{BufMut, BytesMut};
    use tokio_util::codec::Decoder;

    use crate::container::LogOutput;

    use super::{JsonLineDecoder, NewlineLogOutputDecoder};

    #[test]
    fn json_decode_empty() {
        let mut buf = BytesMut::from(&b""[..]);
        let mut codec: JsonLineDecoder<()> = JsonLineDecoder::new();

        assert_eq!(codec.decode(&mut buf).unwrap(), None);
    }

    #[test]
    fn json_decode() {
        let mut buf = BytesMut::from(&b"{}\n{}\n\n{}\n"[..]);
        let mut codec: JsonLineDecoder<HashMap<(), ()>> = JsonLineDecoder::new();

        // Inter-value whitespace (including the blank line) is skipped, so the
        // three objects decode back to back; the trailing "\n" is not a value.
        assert_eq!(codec.decode(&mut buf).unwrap(), Some(HashMap::new()));
        assert_eq!(codec.decode(&mut buf).unwrap(), Some(HashMap::new()));
        assert_eq!(codec.decode(&mut buf).unwrap(), Some(HashMap::new()));
        assert_eq!(codec.decode(&mut buf).unwrap(), None);
        assert_eq!(codec.decode_eof(&mut buf).unwrap(), None);
        assert!(buf.is_empty());
    }

    // Regression for the "bytes remaining on stream" flake (#560): a
    // CRLF-terminated value whose "\r\n" arrives split from the value across a
    // read boundary must not leave an orphaned '\r' that errors at EOF. Docker's
    // progress stream uses CRLF, so this is the common case.
    #[test]
    fn json_decode_crlf_split_before_terminator() {
        let mut codec: JsonLineDecoder<HashMap<String, String>> = JsonLineDecoder::new();

        // The value arrives; its terminating "\r\n" has not yet been read.
        let mut buf = BytesMut::from(&b"{\"status\":\"Downloaded\"}"[..]);
        let expected: HashMap<String, String> = [("status".to_string(), "Downloaded".to_string())]
            .into_iter()
            .collect();
        assert_eq!(codec.decode(&mut buf).unwrap(), Some(expected));

        // The orphaned terminator arrives on its own: inter-value whitespace, so
        // nothing to yield and no error, at EOF too.
        buf.put(&b"\r\n"[..]);
        assert_eq!(codec.decode(&mut buf).unwrap(), None);
        assert_eq!(codec.decode_eof(&mut buf).unwrap(), None);
        assert!(buf.is_empty());
    }

    // A genuinely partial value at EOF is a real truncation and must still error
    // (the fix must not mask that by treating everything as benign).
    #[test]
    fn json_decode_eof_on_partial_value_errors() {
        let mut buf = BytesMut::from(&b"{\"status\":\"Downlo"[..]);
        let mut codec: JsonLineDecoder<HashMap<String, String>> = JsonLineDecoder::new();
        assert!(codec.decode_eof(&mut buf).is_err());
    }

    #[test]
    fn json_partial_decode() {
        let mut buf = BytesMut::from(&b"{}\n{}\n\n{"[..]);
        let mut codec: JsonLineDecoder<HashMap<(), ()>> = JsonLineDecoder::new();

        // Each decode advances past exactly one value; trailing whitespace is
        // left for the next call to skip. The trailing partial "{" waits.
        assert_eq!(codec.decode(&mut buf).unwrap(), Some(HashMap::new()));
        assert_eq!(codec.decode(&mut buf).unwrap(), Some(HashMap::new()));
        assert_eq!(codec.decode(&mut buf).unwrap(), None);
        buf.put(&b"}"[..]);
        assert_eq!(codec.decode(&mut buf).unwrap(), Some(HashMap::new()));
        assert_eq!(codec.decode(&mut buf).unwrap(), None);
        assert!(buf.is_empty());
    }

    #[test]
    fn json_partial_decode_no_newline() {
        let mut buf = BytesMut::from(&b"{\"status\":\"Extracting\",\"progressDetail\":{\"current\":33980416,\"total\":102266715}"[..]);
        let mut codec: JsonLineDecoder<crate::models::CreateImageInfo> = JsonLineDecoder::new();

        let expected = crate::models::CreateImageInfo {
            status: Some(String::from("Extracting")),
            progress_detail: Some(crate::models::ProgressDetail {
                current: Some(33980416),
                total: Some(102266715),
            }),
            ..Default::default()
        };
        assert_eq!(codec.decode(&mut buf).unwrap(), None);
        assert_eq!(buf, &b"{\"status\":\"Extracting\",\"progressDetail\":{\"current\":33980416,\"total\":102266715}"[..]);
        buf.put(&b"}"[..]);
        assert_eq!(codec.decode(&mut buf).unwrap(), Some(expected));
        assert!(buf.is_empty());
    }

    #[test]
    fn json_partial_decode_newline() {
        let mut buf = BytesMut::from(&b"{\"status\":\"Extracting\",\"progressDetail\":{\"current\":33980416,\"total\":102266715}\n"[..]);
        let mut codec: JsonLineDecoder<crate::models::CreateImageInfo> = JsonLineDecoder::new();

        let expected = crate::models::CreateImageInfo {
            status: Some(String::from("Extracting")),
            progress_detail: Some(crate::models::ProgressDetail {
                current: Some(33980416),
                total: Some(102266715),
            }),
            ..Default::default()
        };
        // The object is incomplete (missing its outer '}'); the embedded '\n'
        // is not a frame boundary, so decode waits and leaves the buffer intact.
        assert_eq!(codec.decode(&mut buf).unwrap(), None);
        assert_eq!(buf, &b"{\"status\":\"Extracting\",\"progressDetail\":{\"current\":33980416,\"total\":102266715}\n"[..]);
        buf.put(&b"}"[..]);
        assert_eq!(codec.decode(&mut buf).unwrap(), Some(expected));
        assert!(buf.is_empty());
    }

    #[test]
    fn json_decode_escaped_newline() {
        let mut buf = BytesMut::from(&b"\"foo\\nbar\""[..]);
        let mut codec: JsonLineDecoder<String> = JsonLineDecoder::new();

        assert_eq!(
            codec.decode(&mut buf).unwrap(),
            Some(String::from("foo\nbar"))
        );
    }

    #[test]
    fn json_decode_lacking_newline() {
        let mut buf = BytesMut::from(&b"{}"[..]);
        let mut codec: JsonLineDecoder<HashMap<(), ()>> = JsonLineDecoder::new();

        assert_eq!(codec.decode(&mut buf).unwrap(), Some(HashMap::new()));
        assert!(buf.is_empty());
    }

    #[test]
    fn newline_decode_no_header() {
        let expected = &b"2023-01-14T23:17:27.496421984-05:00 [lighttpd] 2023/01/14 23"[..];
        let mut buf = BytesMut::from(expected);
        let mut codec: NewlineLogOutputDecoder = NewlineLogOutputDecoder::new(true);

        assert_eq!(
            codec.decode(&mut buf).unwrap(),
            Some(LogOutput::Console {
                message: bytes::Bytes::from(expected)
            })
        );

        let mut buf =
            BytesMut::from(&b"2023-01-14T23:17:27.496421984-05:00 [lighttpd] 2023/01/14 23"[..]);
        let mut codec: NewlineLogOutputDecoder = NewlineLogOutputDecoder::new(false);

        assert_eq!(codec.decode(&mut buf).unwrap(), None);

        buf.put(
            &b":17:27 2023-01-14 23:17:26: server.c.1513) server started (lighttpd/1.4.59)\r\n"[..],
        );

        let expected = &b"2023-01-14T23:17:27.496421984-05:00 [lighttpd] 2023/01/14 23:17:27 2023-01-14 23:17:26: server.c.1513) server started (lighttpd/1.4.59)\r\n"[..];
        assert_eq!(
            codec.decode(&mut buf).unwrap(),
            Some(LogOutput::Console {
                message: bytes::Bytes::from(expected)
            })
        );
    }

    #[test]
    fn newline_decode_eof_no_trailing_newline() {
        // TTY containers (tty=true) emit raw bytes without the 8-byte multiplexed
        // header.  When the final chunk has no trailing newline, decode() returns
        // None and decode_eof() must flush the bytes as Console instead of letting
        // FramedRead error with "bytes remaining on stream".
        let payload = b"inital input string";
        let mut buf = BytesMut::from(&payload[..]);
        let mut codec = NewlineLogOutputDecoder::new(false);

        // No newline yet — decode() waits for more data.
        assert_eq!(codec.decode(&mut buf).unwrap(), None);
        assert_eq!(&buf[..], payload);

        // At EOF, decode_eof() must flush the remainder as a Console frame.
        assert_eq!(
            codec.decode_eof(&mut buf).unwrap(),
            Some(LogOutput::Console {
                message: bytes::Bytes::from_static(payload),
            })
        );
        assert!(buf.is_empty());
    }
}
