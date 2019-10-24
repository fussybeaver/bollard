#![cfg(windows)]

use bytes::{Buf, BufMut};
use futures::{Async, Future, Poll};
use hyper::client::connect::{Connect, Connected, Destination};
use mio::Ready;
use mio_named_pipes::NamedPipe;
use tokio_io::{AsyncRead, AsyncWrite};
use tokio_reactor::PollEvented;
use winapi::um::winbase::*;

use std::fmt;
use std::fs::OpenOptions;
use std::io::{self, Read, Write};
use std::mem;
use std::os::windows::fs::*;
use std::os::windows::io::*;
use std::path::Path;

use docker::ClientType;
use uri::Uri;

pub struct NamedPipeStream {
    io: PollEvented<NamedPipe>,
}

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

    pub fn poll_read_ready(&self, ready: Ready) -> Poll<Ready, io::Error> {
        self.io.poll_read_ready(ready)
    }

    pub fn poll_write_ready(&self) -> Poll<Ready, io::Error> {
        self.io.poll_write_ready()
    }
}

impl Read for NamedPipeStream {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        self.io.read(buf)
    }
}

impl Write for NamedPipeStream {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.io.write(buf)
    }
    fn flush(&mut self) -> io::Result<()> {
        self.io.flush()
    }
}

impl<'a> Read for &'a NamedPipeStream {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        (&self.io).read(buf)
    }
}

impl<'a> Write for &'a NamedPipeStream {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        (&self.io).write(buf)
    }

    fn flush(&mut self) -> io::Result<()> {
        (&self.io).flush()
    }
}

impl AsyncRead for NamedPipeStream {
    unsafe fn prepare_uninitialized_buffer(&self, _: &mut [u8]) -> bool {
        false
    }

    fn read_buf<B: BufMut>(&mut self, buf: &mut B) -> Poll<usize, io::Error> {
        <&NamedPipeStream>::read_buf(&mut &*self, buf)
    }
}

impl AsyncWrite for NamedPipeStream {
    fn shutdown(&mut self) -> Poll<(), io::Error> {
        <&NamedPipeStream>::shutdown(&mut &*self)
    }

    fn write_buf<B: Buf>(&mut self, buf: &mut B) -> Poll<usize, io::Error> {
        <&NamedPipeStream>::write_buf(&mut &*self, buf)
    }
}

impl<'a> AsyncRead for &'a NamedPipeStream {
    unsafe fn prepare_uninitialized_buffer(&self, _: &mut [u8]) -> bool {
        false
    }

    fn read_buf<B: BufMut>(&mut self, buf: &mut B) -> Poll<usize, io::Error> {
        if let Async::NotReady = <NamedPipeStream>::poll_read_ready(self, Ready::readable())? {
            return Ok(Async::NotReady);
        }

        let res = unsafe {
            let mut bytes = buf.bytes_mut();
            self.io.get_ref().read(&mut bytes)
        };

        match res {
            Ok(r) => {
                unsafe {
                    buf.advance_mut(r);
                }
                Ok(r.into())
            }

            Err(ref e) if e.kind() == io::ErrorKind::BrokenPipe => Ok(0.into()),

            Err(ref e) if e.kind() == io::ErrorKind::WouldBlock => {
                self.io.clear_write_ready()?;
                Ok(Async::NotReady)
            }

            Err(e) => Err(e),
        }
    }
}

impl<'a> AsyncWrite for &'a NamedPipeStream {
    fn shutdown(&mut self) -> Poll<(), io::Error> {
        Ok(().into())
    }

    #[allow(unused_unsafe)]
    fn write_buf<B: Buf>(&mut self, buf: &mut B) -> Poll<usize, io::Error> {
        if let Async::NotReady = <NamedPipeStream>::poll_write_ready(self)? {
            return Ok(Async::NotReady);
        }

        let res = unsafe {
            let bytes = buf.bytes();
            self.io.get_ref().write(bytes)
        };

        match res {
            Ok(r) => {
                buf.advance(r);
                Ok(r.into())
            }

            Err(ref e) if e.kind() == io::ErrorKind::WouldBlock => {
                self.io.clear_write_ready()?;
                Ok(Async::NotReady)
            }

            Err(e) => Err(e),
        }
    }
}

impl fmt::Debug for NamedPipeStream {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        self.io.get_ref().fmt(f)
    }
}

impl AsRawHandle for NamedPipeStream {
    fn as_raw_handle(&self) -> RawHandle {
        self.io.get_ref().as_raw_handle()
    }
}

impl Future for ConnectFuture {
    type Item = NamedPipeStream;
    type Error = io::Error;

    fn poll(&mut self) -> Poll<NamedPipeStream, io::Error> {
        match self.inner {
            State::Waiting(ref mut stream) => {
                if let Async::NotReady = stream.io.poll_write_ready()? {
                    return Ok(Async::NotReady);
                }

                if let Some(e) = stream.io.get_ref().take_error()? {
                    return Err(e);
                }
            }
            State::Error(_) => match mem::replace(&mut self.inner, State::Empty) {
                State::Error(e) => return Err(e),
                _ => unreachable!(),
            },
            State::Empty => panic!("can't poll stream twice"),
        }

        match mem::replace(&mut self.inner, State::Empty) {
            State::Waiting(stream) => Ok(Async::Ready(stream)),
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
    type Future = ConnectorConnectFuture;

    fn connect(&self, destination: Destination) -> Self::Future {
        ConnectorConnectFuture::Start(destination)
    }
}

#[derive(Debug)]
pub enum ConnectorConnectFuture {
    Start(Destination),
    Connect(ConnectFuture),
}

const NAMED_PIPE_SCHEME: &str = "net.pipe";

impl Future for ConnectorConnectFuture {
    type Item = (NamedPipeStream, Connected);
    type Error = io::Error;

    fn poll(&mut self) -> Poll<Self::Item, Self::Error> {
        loop {
            let next_state = match self {
                ConnectorConnectFuture::Start(destination) => {
                    if destination.scheme() != NAMED_PIPE_SCHEME {
                        return Err(io::Error::new(
                            io::ErrorKind::InvalidInput,
                            format!("Invalid scheme {:?}", destination.scheme()),
                        ));
                    }

                    let path = match Uri::socket_path_dest(&destination, &ClientType::NamedPipe) {
                        Some(path) => path,

                        None => {
                            return Err(io::Error::new(
                                io::ErrorKind::InvalidInput,
                                format!("Invalid uri {:?}", destination),
                            ));
                        }
                    };

                    ConnectorConnectFuture::Connect(NamedPipeStream::connect(&path))
                }

                ConnectorConnectFuture::Connect(f) => match f.poll() {
                    Ok(Async::Ready(stream)) => {
                        return Ok(Async::Ready((stream, Connected::new())))
                    }
                    Ok(Async::NotReady) => return Ok(Async::NotReady),
                    Err(err) => return Err(err),
                },
            };

            *self = next_state;
        }
    }
}
