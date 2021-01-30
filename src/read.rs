use bytes::BytesMut;
use futures_core::Stream;
use hyper::body::Bytes;
use pin_project::pin_project;
use serde::de::DeserializeOwned;
use std::pin::Pin;
use std::string::String;
use std::task::{Context, Poll};
use std::{
    cmp,
    io,
    marker::PhantomData,
};
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
}

impl NewlineLogOutputDecoder {
    pub(crate) fn new() -> NewlineLogOutputDecoder {
        NewlineLogOutputDecoder {
            state: NewlineLogOutputDecoderState::WaitingHeader,
        }
    }
}

impl Decoder for NewlineLogOutputDecoder {
    type Item = LogOutput;
    type Error = Error;

    fn decode(&mut self, src: &mut BytesMut) -> Result<Option<Self::Item>, Self::Error> {
        loop {
            match self.state {
                NewlineLogOutputDecoderState::WaitingHeader => {
                    // `start_exec` API on unix socket will emit values without a header
                    if !src.is_empty() && src[0] > 2 {
                        debug!(
                            "NewlineLogOutputDecoder: no header found, return LogOutput::Console"
                        );
                        return Ok(Some(LogOutput::Console {
                            message: src.split().freeze(),
                        }));
                    }

                    if src.len() < 8 {
                        debug!("NewlineLogOutputDecoder: not enough data for read header");
                        return Ok(None);
                    }

                    let header = src.split_to(8);
                    let length =
                        u32::from_be_bytes([header[4], header[5], header[6], header[7]]) as usize;
                    debug!(
                        "NewlineLogOutputDecoder: read header, type = {}, length = {}",
                        header[0], length
                    );
                    self.state = NewlineLogOutputDecoderState::WaitingPayload(header[0], length);
                }
                NewlineLogOutputDecoderState::WaitingPayload(typ, length) => {
                    if src.len() < length {
                        debug!("NewlineLogOutputDecoder: not enough data to read");
                        return Ok(None);
                    } else {
                        debug!("NewlineLogOutputDecoder: Reading payload");
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
}

#[pin_project]
#[derive(Debug)]
pub(crate) struct JsonLineDecoder<T> {
    ty: PhantomData<T>,
}

impl<T> JsonLineDecoder<T> {
    #[inline]
    pub(crate) fn new() -> JsonLineDecoder<T> {
        JsonLineDecoder { ty: PhantomData }
    }
}

fn decode_json_from_slice<T: DeserializeOwned>(slice: &[u8]) -> Result<T, Error> {
    debug!(
        "Decoding JSON line from stream: {}",
        String::from_utf8_lossy(slice).to_string()
    );

    match serde_json::from_slice(slice) {
        Ok(json) => Ok(json),
        Err(ref e) if e.is_data() => Err(JsonDataError {
            message: e.to_string(),
            column: e.column(),
            contents: String::from_utf8_lossy(slice).to_string(),
        }),
        Err(e) => Err(e.into()),
    }
}

impl<T> Decoder for JsonLineDecoder<T>
where
    T: DeserializeOwned,
{
    type Item = T;
    type Error = Error;
    fn decode(&mut self, src: &mut BytesMut) -> Result<Option<Self::Item>, Self::Error> {
        let nl_index = src.iter().position(|b| *b == b'\n');

        if !src.is_empty() {
            if let Some(pos) = nl_index {
                let slice = src.split_to(pos + 1);
                let slice = &slice[..slice.len() - 1];

                decode_json_from_slice(&slice)
            } else {
                // OSX will not send a newline for some API endpoints (`/events`),
                if src[src.len() - 1] == b'}' {
                    let slice = src.split_to(src.len());
                    decode_json_from_slice(&slice)
                } else {
                    Ok(None)
                }
            }
        } else {
            Ok(None)
        }
    }
}

#[derive(Debug)]
enum ReadState {
    Ready(Bytes, usize),
    NotReady,
}

#[pin_project]
#[derive(Debug)]
pub(crate) struct StreamReader<S> {
    #[pin]
    stream: S,
    state: ReadState,
}

impl<S> StreamReader<S>
where
    S: Stream<Item = Result<Bytes, Error>>,
{
    #[inline]
    pub(crate) fn new(stream: S) -> StreamReader<S> {
        StreamReader {
            stream,
            state: ReadState::NotReady,
        }
    }
}

impl<S> AsyncRead for StreamReader<S>
where
    S: Stream<Item = Result<Bytes, Error>>,
{
    fn poll_read(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        read_buf: &mut ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        let mut this = self.project();
        loop {
            let ret;

            match this.state {
                ReadState::Ready(ref mut chunk, ref mut pos) => {
                    let chunk_start = *pos;
                    let buf = read_buf.initialize_unfilled();
                    let len = cmp::min(buf.len(), chunk.len() - chunk_start);
                    let chunk_end = chunk_start + len;

                    buf[..len].copy_from_slice(&chunk[chunk_start..chunk_end]);
                    *pos += len;
                    read_buf.advance(len);

                    if *pos == chunk.len() {
                        ret = len;
                    } else {
                        return Poll::Ready(Ok(()));
                    }
                }

                ReadState::NotReady => match this.stream.as_mut().poll_next(cx) {
                    Poll::Ready(Some(Ok(chunk))) => {
                        *this.state = ReadState::Ready(chunk, 0);

                        continue;
                    }
                    Poll::Ready(None) => return Poll::Ready(Ok(())),
                    Poll::Pending => {
                        return Poll::Pending;
                    }
                    Poll::Ready(Some(Err(e))) => {
                        return Poll::Ready(Err(io::Error::new(
                            io::ErrorKind::Other,
                            e.to_string(),
                        )));
                    }
                },
            }

            *this.state = ReadState::NotReady;

            return Poll::Ready(Ok(()));
        }
    }
}
