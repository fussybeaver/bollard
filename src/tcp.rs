use std;
use std::io::{self, Read, Write, Result, ErrorKind};
use std::sync::Arc;
use std::error::Error;
use openssl;

pub struct TcpStream {
    addr: String,
    tls: bool,
    ssl_context: Option<Arc<openssl::ssl::SslContext>>
}

impl TcpStream {
    pub fn connect(addr: &str) -> Result<TcpStream> {
        let tcp_stream = TcpStream {
            addr: addr.to_string(),
            tls: false,
            ssl_context: None
        };
        return Ok(tcp_stream);
    }

    pub fn set_ssl_context(&mut self, ssl_context: Arc<openssl::ssl::SslContext>) -> Result<()> {
        self.tls = true;
        self.ssl_context = Some(ssl_context);
        return Ok(());
    }

    pub fn read(&mut self, buf: &[u8]) -> Result<Vec<u8>> {
        let raw = match self.tls {
            false => {
                let mut stream = try!(std::net::TcpStream::connect(&*self.addr));
                let _ = stream.write(buf);
                let raw = try!(self.read_from_stream(&mut stream));
                raw
            }
            true => {
                let stream = try!(std::net::TcpStream::connect(&*self.addr));
                let ssl_context = self.ssl_context.clone().unwrap().clone();
                let mut ssl_stream = match openssl::ssl::SslStream::new(&*ssl_context, stream) {
                    Ok(stream) => stream,
                    Err(e) => {
                        let err = io::Error::new(ErrorKind::NotConnected,
                                                 e.description());
                        return Err(err);
                    }
                };
                let _ = ssl_stream.write(buf);
                let raw = try!(self.read_from_stream(&mut ssl_stream));
                raw
            }
        };
        return Ok(raw);
    }

    fn read_from_stream<S: Read>(&self, stream: &mut S) -> Result<Vec<u8>> {
        const BUFFER_SIZE: usize = 1024;
        let mut buffer: [u8; BUFFER_SIZE] = [0; BUFFER_SIZE];
        let mut raw: Vec<u8> = Vec::new();
        let mut is_shaked = false;
        loop {
            let len = match stream.read(&mut buffer) {
                Ok(size) => size,
                Err(e) => {
                    let err = io::Error::new(ErrorKind::NotConnected,
                                             e.description());
                    return Err(err);
                }
            };

            for i in 0..len {
                raw.push(buffer[i]);
            }

            if is_shaked == false && len <= BUFFER_SIZE { is_shaked = true; continue; }
            if len < BUFFER_SIZE { break; }
        }

        return Ok(raw);
    }
}
