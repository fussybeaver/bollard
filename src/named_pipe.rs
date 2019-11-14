#![cfg(windows)]

use futures_core::ready;
use hyper::client::connect::{Connect, Connected, Destination};
use mio_named_pipes::NamedPipe;
use pin_project::pin_project;
use tokio_io::{AsyncRead, AsyncWrite};
use tokio_net::util::PollEvented;
use winapi::um::winbase::*;

use std::fmt;
use std::fs::OpenOptions;
use std::future::Future;
use std::io::{self, Read, Write};
use std::mem;
use std::os::windows::fs::*;
use std::os::windows::io::*;
use std::path::Path;
use std::pin::Pin;
use std::task::{Context, Poll};

use crate::docker::ClientType;
use crate::uri::Uri;

pub struct NamedPipeStream {
    io: PollEvented<NamedPipe>,
}

#[pin_project]
#[derive(Debug)]
pub struct ConnectFuture {
    inner: State,
}

#[derive(Debug)]
enum State {
    Waiting(NamedPipeStream),
    Error(io::Error),
    Empty,
}

impl NamedPipeStream {
    pub fn connect<A>(addr: A) -> ConnectFuture
    where
        A: AsRef<Path>,
    {
        let mut opts = OpenOptions::new();
        opts.read(true)
            .write(true)
            .custom_flags(FILE_FLAG_OVERLAPPED | SECURITY_SQOS_PRESENT);

        let inner = match opts.open(addr) {
            Ok(file) => State::Waiting(NamedPipeStream::new(unsafe {
                NamedPipe::from_raw_handle(file.into_raw_handle())
            })),
            Err(e) => State::Error(e),
        };

        ConnectFuture { inner }
    }

    pub fn new(stream: NamedPipe) -> NamedPipeStream {
        let io = PollEvented::new(stream);
        NamedPipeStream { io }
    }
}

impl AsyncRead for NamedPipeStream {
    unsafe fn prepare_uninitialized_buffer(&self, _: &mut [u8]) -> bool {
        false
    }

    fn poll_read(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        bytes: &mut [u8],
    ) -> Poll<Result<usize, io::Error>> {
        match self.io.get_ref().read(bytes) {
            Ok(r) => Poll::Ready(Ok(r)),
            Err(ref e) if e.kind() == io::ErrorKind::BrokenPipe => Poll::Ready(Ok(0)),

            Err(ref e) if e.kind() == io::ErrorKind::WouldBlock => {
                self.io.clear_write_ready(cx)?;
                Poll::Pending
            }

            Err(e) => Poll::Ready(Err(e)),
        }
    }
}

impl AsyncWrite for NamedPipeStream {
    fn poll_shutdown(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<Result<(), io::Error>> {
        Poll::Ready(Ok(()))
    }

    fn poll_write(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        bytes: &[u8],
    ) -> Poll<Result<usize, io::Error>> {
        match self.io.get_ref().write(bytes) {
            Ok(r) => Poll::Ready(Ok(r.into())),

            Err(ref e) if e.kind() == io::ErrorKind::WouldBlock => {
                self.io.clear_write_ready(cx)?;
                Poll::Pending
            }

            Err(e) => Poll::Ready(Err(e)),
        }
    }

    fn poll_flush(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<Result<(), io::Error>> {
        Poll::Ready(self.io.get_ref().flush())
    }
}

impl fmt::Debug for NamedPipeStream {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.io.get_ref().fmt(f)
    }
}

impl AsRawHandle for NamedPipeStream {
    fn as_raw_handle(&self) -> RawHandle {
        self.io.get_ref().as_raw_handle()
    }
}

impl Future for ConnectFuture {
    type Output = Result<NamedPipeStream, io::Error>;

    fn poll(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
    ) -> Poll<Result<NamedPipeStream, io::Error>> {
        let this = self.project();
        match this.inner {
            State::Waiting(ref mut stream) => {
                ready!(stream.io.poll_write_ready(cx)?);

                if let Some(e) = stream.io.get_ref().take_error()? {
                    return Poll::Ready(Err(e));
                }
            }
            State::Error(_) => match mem::replace(this.inner, State::Empty) {
                State::Error(e) => return Poll::Ready(Err(e)),
                _ => unreachable!(),
            },
            State::Empty => panic!("can't poll stream twice"),
        }

        match mem::replace(this.inner, State::Empty) {
            State::Waiting(stream) => Poll::Ready(Ok(stream)),
            _ => unreachable!(),
        }
    }
}

pub trait IsZero {
    fn is_zero(&self) -> bool;
}

impl IsZero for i32 {
    fn is_zero(&self) -> bool {
        *self == 0
    }
}

#[derive(Clone, Copy, Debug)]
pub struct NamedPipeConnector;

impl NamedPipeConnector {
    pub fn new() -> Self {
        NamedPipeConnector
    }
}

impl Connect for NamedPipeConnector {
    type Transport = NamedPipeStream;
    type Error = io::Error;
    type Future = Pin<Box<ConnectorConnectFuture>>;

    fn connect(&self, destination: Destination) -> Self::Future {
        Box::pin(ConnectorConnectFuture {
            state: ConnectorConnectState::Start(destination),
        })
    }
}

#[derive(Debug)]
pub enum ConnectorConnectState {
    Start(Destination),
    Connect(Pin<Box<ConnectFuture>>),
}

#[pin_project]
#[derive(Debug)]
pub struct ConnectorConnectFuture {
    state: ConnectorConnectState,
}

const NAMED_PIPE_SCHEME: &str = "net.pipe";

impl Future for ConnectorConnectFuture {
    type Output = Result<(NamedPipeStream, Connected), io::Error>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let this = self.project();
        loop {
            let next_state = match this.state {
                ConnectorConnectState::Start(destination) => {
                    if destination.scheme() != NAMED_PIPE_SCHEME {
                        return Poll::Ready(Err(io::Error::new(
                            io::ErrorKind::InvalidInput,
                            format!("Invalid scheme {:?}", destination.scheme()),
                        )));
                    }

                    let path = match Uri::socket_path_dest(&destination, &ClientType::NamedPipe) {
                        Some(path) => path,

                        None => {
                            return Poll::Ready(Err(io::Error::new(
                                io::ErrorKind::InvalidInput,
                                format!("Invalid uri {:?}", destination),
                            )));
                        }
                    };

                    ConnectorConnectFuture {
                        state: ConnectorConnectState::Connect(Box::pin(NamedPipeStream::connect(
                            &path,
                        ))),
                    }
                }

                ConnectorConnectState::Connect(f) => match f.as_mut().poll(cx) {
                    Poll::Ready(Ok(stream)) => return Poll::Ready(Ok((stream, Connected::new()))),
                    Poll::Pending => return Poll::Pending,
                    Poll::Ready(Err(err)) => return Poll::Ready(Err(err)),
                },
            };

            *this.state = next_state.state;
        }
    }
}
