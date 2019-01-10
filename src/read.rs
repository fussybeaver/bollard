use bytes::BytesMut;
use failure::Error;
use futures::{Async, Stream};
use hyper::Chunk;
use serde::de::DeserializeOwned;
use serde_json;
use std::{
    cmp,
    io::{self, Read},
    marker::PhantomData,
};
use tokio_codec::Decoder;
use tokio_io::AsyncRead;

use errors::JsonDataError;

#[derive(Debug, Copy, Clone)]
pub(crate) struct LineDecoder {}
impl LineDecoder {
    pub(crate) fn new() -> LineDecoder {
        LineDecoder {}
    }
}
use container::LogOutput;

impl Decoder for LineDecoder {
    type Item = LogOutput;
    type Error = Error;
    fn decode(&mut self, src: &mut BytesMut) -> Result<Option<Self::Item>, Self::Error> {
        let nl_index = src.iter().position(|b| *b == b'\n');

        if let Some(pos) = nl_index {
            let slice = src.split_to(pos + 1);
            let slice = &slice[..slice.len() - 1];

            match &slice[0..1] {
                &[0] if slice.len() <= 8 => Ok(Some(LogOutput::StdIn {
                    message: String::new(),
                })),
                &[0] => ::std::str::from_utf8(&slice[8..]).map(|s| {
                    Some(LogOutput::StdIn {
                        message: s.to_string(),
                    })
                }),
                &[1] if slice.len() <= 8 => Ok(Some(LogOutput::StdOut {
                    message: String::new(),
                })),
                &[1] => ::std::str::from_utf8(&slice[8..]).map(|s| {
                    Some(LogOutput::StdOut {
                        message: s.to_string(),
                    })
                }),
                &[2] if slice.len() <= 8 => Ok(Some(LogOutput::StdErr {
                    message: String::new(),
                })),
                &[2] => ::std::str::from_utf8(&slice[8..]).map(|s| {
                    Some(LogOutput::StdErr {
                        message: s.to_string(),
                    })
                }),
                _ => {
                    debug!("LineDecoder returning due to unknown first byte");
                    Ok(None)
                }
            }
            .map_err(|e| e.into())
        } else {
            debug!("LineDecoder returning due to an empty line");
            Ok(None)
        }
    }
}

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

        if let Some(pos) = nl_index {
            let slice = src.split_to(pos + 1);
            let slice = &slice[..slice.len() - 1];

            debug!(
                "Decoding JSON line from stream: {}",
                ::std::str::from_utf8(&slice).unwrap()
            );

            match serde_json::from_slice(slice) {
                Ok(json) => Ok(json),
                Err(ref e) if e.is_data() => ::std::str::from_utf8(&slice)
                    .map_err(|e| e.into())
                    .and_then(|content| {
                        Err(JsonDataError {
                            message: e.to_string(),
                            column: e.column(),
                            contents: content.to_string(),
                        }
                        .into())
                    }),
                Err(e) => Err(e.into()),
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

#[derive(Debug)]
pub(crate) struct StreamReader<S> {
    stream: S,
    state: ReadState,
}

impl<S> StreamReader<S>
where
    S: Stream<Item = Chunk, Error = Error>,
{
    #[inline]
    pub(crate) fn new(stream: S) -> StreamReader<S> {
        StreamReader {
            stream,
            state: ReadState::NotReady,
        }
    }
}

impl<S> Read for StreamReader<S>
where
    S: Stream<Item = Chunk, Error = Error>,
{
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        loop {
            let ret;

            match self.state {
                ReadState::Ready(ref mut chunk, ref mut pos) => {
                    let chunk_start = *pos;
                    let len = cmp::min(buf.len(), chunk.len() - chunk_start);
                    let chunk_end = chunk_start + len;

                    buf[..len].copy_from_slice(&chunk[chunk_start..chunk_end]);
                    *pos += len;

                    if *pos == chunk.len() {
                        ret = len;
                    } else {
                        return Ok(len);
                    }
                }

                ReadState::NotReady => match self.stream.poll() {
                    Ok(Async::Ready(Some(chunk))) => {
                        self.state = ReadState::Ready(chunk, 0);

                        continue;
                    }
                    Ok(Async::Ready(None)) => return Ok(0),
                    Ok(Async::NotReady) => {
                        return Err(io::ErrorKind::WouldBlock.into());
                    }
                    Err(e) => {
                        return Err(io::Error::new(io::ErrorKind::Other, e.to_string()));
                    }
                },
            }

            self.state = ReadState::NotReady;

            return Ok(ret);
        }
    }
}

impl<S> AsyncRead for StreamReader<S> where S: Stream<Item = Chunk, Error = Error> {}
