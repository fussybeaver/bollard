use bytes::BytesMut;
use futures_core::Stream;
use hyper::Chunk;
use pin_project::pin_project;
use serde::de::DeserializeOwned;
use serde_json;
use std::pin::Pin;
use std::string::String;
use std::task::{Context, Poll};
use std::{
    cmp,
    io::{self},
    marker::PhantomData,
};
use tokio_codec::Decoder;
use tokio_io::AsyncRead;

use crate::container::LogOutput;

use crate::errors::Error;
use crate::errors::ErrorKind::{JsonDataError, JsonDeserializeError, StrParseError};

#[derive(Debug, Copy, Clone)]
pub(crate) struct NewlineLogOutputDecoder {}

impl NewlineLogOutputDecoder {
    pub(crate) fn new() -> NewlineLogOutputDecoder {
        NewlineLogOutputDecoder {}
    }
}

impl Decoder for NewlineLogOutputDecoder {
    type Item = LogOutput;
    type Error = Error;
    fn decode(&mut self, src: &mut BytesMut) -> Result<Option<Self::Item>, Self::Error> {
        println!("{:?}", src);
        let nl_index = src.iter().position(|b| *b == b'\n');

        if src.len() > 0 {
            let pos = nl_index.unwrap_or(src.len() - 1);

            let slice = src.split_to(pos + 1);
            let slice = &slice[..slice.len() - 1];

            if slice.len() == 0 {
                Ok(Some(LogOutput::Console {
                    message: String::new(),
                }))
            } else {
                match &slice[0] {
                    0 if slice.len() <= 8 => Ok(Some(LogOutput::StdIn {
                        message: String::new(),
                    })),
                    0 => Ok(Some(LogOutput::StdIn {
                        message: String::from_utf8_lossy(&slice[8..]).to_string(),
                    })),
                    1 if slice.len() <= 8 => Ok(Some(LogOutput::StdOut {
                        message: String::new(),
                    })),
                    1 => Ok(Some(LogOutput::StdOut {
                        message: String::from_utf8_lossy(&slice[8..]).to_string(),
                    })),
                    2 if slice.len() <= 8 => Ok(Some(LogOutput::StdErr {
                        message: String::new(),
                    })),
                    2 => Ok(Some(LogOutput::StdErr {
                        message: String::from_utf8_lossy(&slice[8..]).to_string(),
                    })),
                    _ =>
                    // `start_exec` API on unix socket will emit values without a header
                    {
                        Ok(Some(LogOutput::Console {
                            message: String::from_utf8_lossy(&slice).to_string(),
                        }))
                    }
                }
                .map_err(|e| {
                    StrParseError {
                        content: hex::encode(slice.to_owned()),
                        err: e,
                    }
                    .into()
                })
            }
        } else {
            debug!("NewlineLogOutputDecoder returning due to an empty line");
            Ok(None)
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

impl<T> Decoder for JsonLineDecoder<T>
where
    T: DeserializeOwned,
{
    type Item = T;
    type Error = Error;
    fn decode(&mut self, src: &mut BytesMut) -> Result<Option<Self::Item>, Self::Error> {
        let nl_index = src.iter().position(|b| *b == b'\n');

        if src.len() > 0 {
            let pos = nl_index.unwrap_or(src.len() - 1);

            let slice = src.split_to(pos + 1);
            let slice = &slice[..slice.len() - 1];

            debug!(
                "Decoding JSON line from stream: {}",
                String::from_utf8_lossy(&slice).to_string()
            );

            match serde_json::from_slice(slice) {
                Ok(json) => Ok(json),
                Err(ref e) if e.is_data() => Err(JsonDataError {
                    message: e.to_string(),
                    column: e.column(),
                    contents: String::from_utf8_lossy(&slice).to_string(),
                }
                .into()),
                Err(e) => Err(JsonDeserializeError {
                    content: String::from_utf8_lossy(slice).to_string(),
                    err: e,
                }
                .into()),
            }
        } else {
            Ok(None)
        }
    }
}

#[derive(Debug)]
enum ReadState {
    Ready(Chunk, usize),
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
    S: Stream<Item = Result<Chunk, Error>>,
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
    S: Stream<Item = Result<Chunk, Error>>,
{
    fn poll_read(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut [u8],
    ) -> Poll<io::Result<usize>> {
        let mut this = self.project();
        loop {
            let ret;

            match this.state {
                ReadState::Ready(ref mut chunk, ref mut pos) => {
                    let chunk_start = *pos;
                    let len = cmp::min(buf.len(), chunk.len() - chunk_start);
                    let chunk_end = chunk_start + len;

                    buf[..len].copy_from_slice(&chunk[chunk_start..chunk_end]);
                    *pos += len;

                    if *pos == chunk.len() {
                        ret = len;
                    } else {
                        return Poll::Ready(Ok(len));
                    }
                }

                ReadState::NotReady => match this.stream.as_mut().poll_next(cx) {
                    Poll::Ready(Some(Ok(chunk))) => {
                        *this.state = ReadState::Ready(chunk, 0);

                        continue;
                    }
                    Poll::Ready(None) => return Poll::Ready(Ok(0)),
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

            return Poll::Ready(Ok(ret));
        }
    }
}
