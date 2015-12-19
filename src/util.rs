use std;
use std::vec::Vec;
use std::io::{Read, Write, Result};
use std::time::Duration;
use std::net::{SocketAddr, Shutdown};
use hyper::net::NetworkStream;

pub struct MemoryStream {
    buf: Vec<u8>,
    pos: usize,
}

impl MemoryStream {
    pub fn with_input(input: &[u8]) -> MemoryStream {
        MemoryStream {
            buf: input.to_vec(),
            pos: 0,
        }
    }

    pub fn into_inner(self) -> Vec<u8> {
        return self.buf
    }
}

impl Read for MemoryStream {
    fn read(&mut self, mut buf: &mut [u8]) -> Result<usize> {
        let (_, to_write) = self.buf.split_at(self.pos);
        let n = try!(buf.write(to_write));
        self.pos = self.pos + n;
        Ok(n)
    }
}

impl Write for MemoryStream {
    fn write(&mut self, buf: &[u8]) -> Result<usize> {
        self.buf.extend(buf);
        Ok(buf.len())
    }

    fn flush(&mut self) -> Result<()> {
        Ok(())
    }
}

impl NetworkStream for MemoryStream {
    fn peer_addr(&mut self) -> Result<SocketAddr> {
        Ok("127.0.0.1:1337".parse().unwrap())
    }

    fn set_read_timeout(&self, _dur: Option<Duration>) -> Result<()> {
        Ok(())
    }

    fn set_write_timeout(&self, _dur: Option<Duration>) -> Result<()> {
        Ok(())
    }

    fn close(&mut self, _how: Shutdown) -> Result<()> {
        Ok(())
    }
}
