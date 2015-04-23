use std;
use std::io::{Read, Write, Result, Error, ErrorKind};
use openssl;

pub struct TcpStream {
    addr: String,
    tls: bool
}

impl TcpStream {
    pub fn connect(addr: &str, tls: bool) -> TcpStream {
        let tcp_stream = TcpStream {
            addr: addr.to_string(),
            tls: tls
        };
        return tcp_stream;
    }

    pub fn read(&self, buf: &[u8]) -> Result<String> {
        let mut stream = match std::net::TcpStream::connect(&*self.addr) {
            Ok(stream) => stream,
            Err(_) => {
                let err = Error::new(ErrorKind::NotConnected,
                                     "TCP connection is not connected.");
                return Err(err);
            }
        };

        let raw = match self.tls {
            false => {
                let _ = stream.write(buf);
                let raw = try!(self.read_from_stream(&mut stream));
                raw
            }
            true => {
                let context = match openssl::ssl::SslContext::new(openssl::ssl::SslMethod::Sslv23) {
                    Ok(context) => context,
                    Err(_) => {
                        let err = Error::new(ErrorKind::NotConnected,
                                             "");
                        return Err(err);
                    }
                };

                /*match context.set_CA_file(&a) {
                    Ok(_) => {}
                    Err(e) => {
                        println!("a");
                        panic!("{}", e);
                    }
                }

                match context.set_certificate_file(&b, openssl::x509::X509FileType::PEM) {
                    Ok(_) => {}
                    Err(e) => {
                        println!("b");
                        panic!("{}", e);
                    }
                }*/

                /*match context.set_private_key_file(&c, openssl::x509::X509FileType::PEM) {
                    Ok(_) => {}
                    Err(e) => {
                        println!("c");
                        panic!("{}", e);
                    }
                }*/

                let mut ssl_stream = match openssl::ssl::SslStream::new(&context, stream) {
                    Ok(stream) => stream,
                    Err(e) => {
                        println!("{}", e);
                        let err = Error::new(ErrorKind::NotConnected,
                                             "b");
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
        println!("read_from_stream");
        const BUFFER_SIZE: usize = 1024;
        let mut buffer: [u8; BUFFER_SIZE] = [0; BUFFER_SIZE];
        let mut raw = String::new();
        loop {
            let len = match stream.read(&mut buffer) {
                Ok(size) => size,
                Err(_) => {
                    let err = Error::new(ErrorKind::NotConnected,
                                         "");
                    return Err(err);
                }
            };

            if len <= 0 { break; }

            match std::str::from_utf8(&buffer[0 .. len]) {
                Ok(buf) => raw.push_str(buf),
                Err(_) => {
                    let err = Error::new(ErrorKind::NotConnected,
                                         "");
                    return Err(err);
                }
            }
        }

        println!("{}", raw);

        return Ok(raw);
    }
}
