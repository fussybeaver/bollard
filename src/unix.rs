use std;
use std::fmt;
use std::io;
use std::io::{ErrorKind, Read, Write};
use std::net::{SocketAddr, ToSocketAddrs, SocketAddrV4, Ipv4Addr, Shutdown};
use std::time::Duration;
use unix_socket::UnixStream;
use hyper;
use hyper::net::{NetworkConnector, NetworkStream};

pub struct HttpUnixStream(pub UnixStream);

impl Clone for HttpUnixStream {
    #[inline]
    fn clone(&self) -> HttpUnixStream {
        HttpUnixStream(self.0.try_clone().unwrap())
    }
}

impl fmt::Debug for HttpUnixStream {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.write_str("HttpUnixStream(_)")
    }
}

impl Read for HttpUnixStream {
    #[inline]
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        self.0.read(buf)
    }
}

impl Write for HttpUnixStream {
    #[inline]
    fn write(&mut self, msg: &[u8]) -> io::Result<usize> {
        self.0.write(msg)
    }

    #[inline]
    fn flush(&mut self) -> io::Result<()> {
        self.0.flush()
    }
}

#[cfg(windows)]
impl ::std::os::windows::io::AsRawSocket for HttpUnixStream {
    fn as_raw_socket(&self) -> ::std::os::windows::io::RawSocket {
        self.0.as_raw_socket()
    }
}

#[cfg(windows)]
impl ::std::os::windows::io::FromRawSocket for HttpUnixStream {
    unsafe fn from_raw_socket(sock: ::std::os::windows::io::RawSocket) -> HttpUnixStream {
        HttpUnixStream(UnixStream::from_raw_socket(sock))
    }
}

#[cfg(unix)]
impl ::std::os::unix::io::AsRawFd for HttpUnixStream {
    fn as_raw_fd(&self) -> ::std::os::unix::io::RawFd {
        self.0.as_raw_fd()
    }
}

#[cfg(unix)]
impl ::std::os::unix::io::FromRawFd for HttpUnixStream {
    unsafe fn from_raw_fd(fd: ::std::os::unix::io::RawFd) -> HttpUnixStream {
        HttpUnixStream(UnixStream::from_raw_fd(fd))
    }
}

impl NetworkStream for HttpUnixStream {
    #[inline]
    fn peer_addr(&mut self) -> io::Result<SocketAddr> {
        Ok(SocketAddr::V4(SocketAddrV4::new(Ipv4Addr::new(127, 0, 0, 1), 80)))
    }

    #[inline]
    fn set_read_timeout(&self, dur: Option<Duration>) -> io::Result<()> {
        self.0.set_read_timeout(dur)
    }

    #[inline]
    fn set_write_timeout(&self, dur: Option<Duration>) -> io::Result<()> {
        self.0.set_write_timeout(dur)
    }

    #[inline]
    fn close(&mut self, how: Shutdown) -> io::Result<()> {
        match self.0.shutdown(how) {
            Ok(_) => Ok(()),
            // see https://github.com/hyperium/hyper/issues/508
            Err(ref e) if e.kind() == ErrorKind::NotConnected => Ok(()),
            err => err
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct HttpUnixConnector {
    path: String,
}

impl HttpUnixConnector {
    pub fn new(path: &String) -> HttpUnixConnector {
        HttpUnixConnector {
            path: path.clone(),
        }
    }
}

impl NetworkConnector for HttpUnixConnector {
    type Stream = HttpUnixStream;

    fn connect(&self, host: &str, port: u16, scheme: &str) -> hyper::error::Result<HttpUnixStream> {
        Ok(HttpUnixStream(try!(UnixStream::connect(self.path.clone()))))
    }
}
