use std;
use std::io::{self, Read, Write, Result, ErrorKind};
use std::path::Path;
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

    pub fn set_tls(&mut self, key: &Path, cert: &Path, ca: &Path) -> Result<()> {
        self.tls = true;

        let mut context = match openssl::ssl::SslContext::new(openssl::ssl::SslMethod::Tlsv1) {
            Ok(context) => context,
            Err(e) => {
                let err = io::Error::new(ErrorKind::NotConnected,
                                         e.description());
                return Err(err);
            }
        };

        match context.set_private_key_file(key, openssl::x509::X509FileType::PEM) {
            Ok(_) => {}
            Err(e) => {
                let err = io::Error::new(ErrorKind::InvalidInput,
                                         e.description());
                return Err(err);
            }
        }

        match context.set_certificate_file(cert, openssl::x509::X509FileType::PEM) {
            Ok(_) => {}
            Err(e) => {
                let err = io::Error::new(ErrorKind::NotConnected,
                                         e.description());
                return Err(err);
            }
        }

        match context.set_CA_file(ca) {
            Ok(_) => {}
            Err(e) => {
                let err = io::Error::new(ErrorKind::NotConnected,
                                         e.description());
                return Err(err);
            }
        }

        self.ssl_context = Some(Arc::new(context));

        return Ok(());
    }

    pub fn read(&mut self, buf: &[u8]) -> Result<String> {
        let raw = match self.tls {
            false => {
                let mut stream = try!(std::net::TcpStream::connect(&*self.addr));
                let _ = stream.write(buf);
                let raw = try!(self.read_from_stream(&mut stream));
                raw
            }
            true => {
                let stream = try!(std::net::TcpStream::connect(&*self.addr));
                let ssl_context = self.ssl_context.as_mut().unwrap().clone();
                let mut ssl_stream = match openssl::ssl::SslStream::new(&ssl_context, stream) {
                    Ok(stream) => stream,
                    Err(e) => {
                        let err = io::Error::new(ErrorKind::NotConnected,
                                                 e.description());
                        return Err(err);
                    }
                };
                let _ = ssl_stream.write(&*buf);
                let raw = try!(self.read_from_stream(&mut ssl_stream));
                raw
            }
        };
        return Ok(raw);
    }

    fn read_from_stream<S: Read>(&self, stream: &mut S) -> Result<String> {
        const BUFFER_SIZE: usize = 1024;
        let mut buffer: [u8; BUFFER_SIZE] = [0; BUFFER_SIZE];
        let mut raw = String::new();
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
            match std::str::from_utf8(&buffer[0 .. len]) {
                Ok(buf) => raw.push_str(buf),
                Err(e) => {
                    let err = io::Error::new(ErrorKind::NotConnected,
                                             e.description());
                    return Err(err);
                }
            }
            if is_shaked == false && len <= BUFFER_SIZE { is_shaked = true; continue; }
            if len < BUFFER_SIZE { break; }
        }
        return Ok(raw);
    }
}
