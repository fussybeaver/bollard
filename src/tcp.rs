use std;
use std::io::{Read, Write};
use std::path::Path;
use std::error::Error;

use openssl;

pub struct TcpStream {
    tls: bool,
    stream: std::net::TcpStream,
    ssl_stream: Option<openssl::ssl::SslStream<std::net::TcpStream>>
}

impl TcpStream {
    pub fn connect(addr: &str) -> std::io::Result<TcpStream> {
        let stream = try!(std::net::TcpStream::connect(addr));
        
        let tcp_stream = TcpStream {
            tls: false,
            stream: stream,
            ssl_stream: None,
        };
        
        return Ok(tcp_stream);
    }

    pub fn set_ssl_context(&mut self, key: &Path, cert: &Path, ca: &Path) -> std::io::Result<()> {
        self.tls = true;

        let mut context = match openssl::ssl::SslContext::new(openssl::ssl::SslMethod::Tlsv1) {
            Ok(context) => context,
            Err(e) => {
                let err = std::io::Error::new(std::io::ErrorKind::NotConnected,
                                              e.description());
                return Err(err);
            }
        };

        match context.set_private_key_file(key, openssl::x509::X509FileType::PEM) {
            Ok(_) => {}
            Err(e) => {
                let err = std::io::Error::new(std::io::ErrorKind::InvalidInput,
                                              e.description());
                return Err(err);
            }
        }

        match context.set_certificate_file(cert, openssl::x509::X509FileType::PEM) {
            Ok(_) => {}
            Err(e) => {
                let err = std::io::Error::new(std::io::ErrorKind::NotConnected,
                                              e.description());
                return Err(err);
            }
        }

        match context.set_CA_file(ca) {
            Ok(_) => {}
            Err(e) => {
                let err = std::io::Error::new(std::io::ErrorKind::NotConnected,
                                              e.description());
                return Err(err);
            }
        }
        
        let stream = self.stream.try_clone().unwrap();
        let ssl_stream = match openssl::ssl::SslStream::new(&context, stream) {
            Ok(ssl_stream) => ssl_stream,
            Err(e) => {
                let err = std::io::Error::new(std::io::ErrorKind::NotConnected,
                                              e.description());
                return Err(err);
            }
        };

        self.ssl_stream = Some(ssl_stream);
        
        return Ok(());
    }

    pub fn read(&mut self, buf: &[u8]) -> std::io::Result<Vec<u8>> {
        let raw = match self.tls {
            false => {
                let mut stream = self.stream.try_clone().unwrap();
                let _ = stream.write(buf);
                let raw = try!(TcpStream::read_from_stream(&mut stream));
                raw
            }
            true => {
                let mut ssl_stream = self.ssl_stream.as_mut().unwrap().try_clone().unwrap();
                let _ = ssl_stream.write(buf);
                let raw = try!(TcpStream::read_from_stream(&mut ssl_stream));
                raw
            }
        };
        return Ok(raw);
    }

    fn read_from_stream<S: Read>(stream: &mut S) -> std::io::Result<Vec<u8>> {
        const BUFFER_SIZE: usize = 1024;
        let mut buffer: [u8; BUFFER_SIZE] = [0; BUFFER_SIZE];
        let mut raw: Vec<u8> = Vec::new();
        let mut is_shaked = false;
        loop {
            let len = match stream.read(&mut buffer) {
                Ok(size) => size,
                Err(e) => {
                    let err = std::io::Error::new(std::io::ErrorKind::NotConnected,
                                                  e.description());
                    return Err(err);
                }
            };

            if len == 4 &&
                buffer[0] == 13 &&
                buffer[1] == 10 &&
                buffer[2] == 13 &&
                buffer[3] == 10  { break; }

            for i in 0..len { raw.push(buffer[i]); }

            if len > 1 && buffer[len - 2] == 13 && buffer[len - 1] == 10 { is_shaked = false; continue; }
            if is_shaked == false && len <= BUFFER_SIZE { is_shaked = true; continue; }
            if len < BUFFER_SIZE { break; }
        }

        return Ok(raw);
    }
}

impl Clone for TcpStream {
    fn clone(&self) -> Self {
        let ssl_stream = match self.ssl_stream {
            Some(ref ssl_stream) => {
                Some(ssl_stream.try_clone().unwrap())
            }
            None => None
        };
        
        let stream = TcpStream {
            tls: self.tls.clone(),
            stream: self.stream.try_clone().unwrap(),
            ssl_stream: ssl_stream
        };

        return stream;
    }
}
