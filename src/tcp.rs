use std;
use std::io::{self, Read, Write, Result, ErrorKind};
use std::path::Path;
use std::error::Error;
use openssl;

pub struct TcpStream {
    tls: bool,
    key_path: Option<String>,
    cert_path: Option<String>,
    ca_path: Option<String>,
    stream: std::net::TcpStream
}

impl TcpStream {
    pub fn connect(addr: &str) -> Result<TcpStream> {
        let stream = try!(std::net::TcpStream::connect(addr));
        let tcp_stream = TcpStream {
            tls: false,
            key_path: None,
            cert_path: None,
            ca_path: None,
            stream: stream
        };
        return Ok(tcp_stream);
    }

    pub fn set_tls(&mut self, tls: bool) {
        self.tls = tls;
    }

    pub fn set_private_key_file(&mut self, path: &str) {
        self.key_path = Some(path.to_string());
    }

    pub fn set_certificate_file(&mut self, path: &str) {
        self.cert_path = Some(path.to_string());
    }

    pub fn set_ca_file(&mut self, path: &str) {
        self.ca_path = Some(path.to_string());
    }

    pub fn read(&mut self, buf: &[u8]) -> Result<String> {
        let raw = match self.tls {
            false => {
                let _ = self.stream.write(buf);
                let raw = try!(self.read_from_stream(&mut self.stream.try_clone().unwrap()));
                raw
            }
            true => {
                let mut context = match openssl::ssl::SslContext::new(openssl::ssl::SslMethod::Tlsv1) {
                    Ok(context) => context,
                    Err(e) => {
                        let err = io::Error::new(ErrorKind::NotConnected,
                                                 e.description());
                        return Err(err);
                    }
                };

                let key = self.key_path.clone().unwrap();
                let cert = self.cert_path.clone().unwrap();
                let ca = self.ca_path.clone().unwrap();

                let key_path = Path::new(&key);
                let cert_path = Path::new(&cert);
                let ca_path = Path::new(&ca);

                match context.set_private_key_file(&key_path, openssl::x509::X509FileType::PEM) {
                    Ok(_) => {}
                    Err(e) => {
                        let err = io::Error::new(ErrorKind::InvalidInput,
                                                 e.description());
                        return Err(err);
                    }
                }

                match context.set_certificate_file(&cert_path, openssl::x509::X509FileType::PEM) {
                    Ok(_) => {}
                    Err(e) => {
                        let err = io::Error::new(ErrorKind::NotConnected,
                                                 e.description());
                        return Err(err);
                    }
                }

                match context.set_CA_file(&ca_path) {
                    Ok(_) => {}
                    Err(e) => {
                        let err = io::Error::new(ErrorKind::NotConnected,
                                                 e.description());
                        return Err(err);
                    }
                }

                let mut ssl_stream = match openssl::ssl::SslStream::new(&context, self.stream.try_clone().unwrap()) {
                    Ok(stream) => stream,
                    Err(e) => {
                        let err = io::Error::new(ErrorKind::NotConnected,
                                                 e.description());
                        return Err(err);
                    }
                };

                let _ = ssl_stream.write(&*buf);
                let raw = try!(self.read_from_stream(&mut ssl_stream.try_clone().unwrap()));
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
