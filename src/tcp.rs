use std;
use std::io::{self, Read, Write, Result, ErrorKind};
use std::path::Path;
use std::error::Error;
use openssl;

pub struct TcpStream {
    stream: std::net::TcpStream,
    tls: bool,
    ssl_stream: Option<openssl::ssl::SslStream<std::net::TcpStream>>
}

impl TcpStream {
    pub fn connect(addr: &str) -> Result<TcpStream> {
        let stream = try!(std::net::TcpStream::connect(addr));
        let tcp_stream = TcpStream {
            stream: stream,
            tls: false,
            ssl_stream: None
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

        let stream = self.stream.try_clone().unwrap();
        let ssl_stream = match openssl::ssl::SslStream::new(&context, stream) {
            Ok(stream) => stream,
            Err(e) => {
                let err = io::Error::new(ErrorKind::NotConnected,
                                         e.description());
                return Err(err);
            }
        };

        self.ssl_stream = Some(ssl_stream);

        return Ok(());
    }

    pub fn read(&mut self, buf: &[u8]) -> Result<String> {
        let raw = match self.tls {
            false => {
                let mut stream = self.stream.try_clone().unwrap();
                let _ = stream.write(buf);
                let raw = try!(self.read_from_stream(&mut stream));
                raw
            }
            true => {
                let mut stream = self.ssl_stream.as_mut().unwrap().try_clone().unwrap();
                let _ = stream.write(&*buf);
                let raw = try!(self.read_from_stream(&mut stream));
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
