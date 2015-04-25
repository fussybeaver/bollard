use std;
use std::io::{self, Read, Write, Result, ErrorKind};
use std::error::Error;
use unix_socket;

pub struct UnixStream {
    addr: String
}

impl UnixStream {
    pub fn connect(addr: &str) -> UnixStream {
        let unix_stream = UnixStream {
            addr: addr.to_string()
        };
        return unix_stream;
    }
    
    pub fn read(&self, buf: &[u8]) -> Result<String> {
        let mut stream = match unix_socket::UnixStream::connect(&self.addr.clone()) {
            Ok(stream) => stream,
            Err(e) => {
                let err = io::Error::new(ErrorKind::NotConnected,
                                         e.description());
                return Err(err);
            }
        };
        
        match stream.write_all(buf) {
            Ok(_) => {}
            Err(e) => {
                let err = io::Error::new(ErrorKind::ConnectionAborted,
                                         e.description());
                return Err(err);
            }
        };

        const BUFFER_SIZE: usize = 1024;
        let mut buffer: [u8; BUFFER_SIZE] = [0; BUFFER_SIZE];
        let mut raw = String::new();
        loop {
            let len = match stream.read(&mut buffer) {
                Ok(len) => len,
                Err(e) => {
                    let err = io::Error::new(ErrorKind::ConnectionAborted,
                                             e.description());
                    return Err(err);
                }
            };
            match std::str::from_utf8(&buffer[0 .. len]) {
                Ok(buf) => raw.push_str(buf),
                Err(e) => {
                    let err = io::Error::new(ErrorKind::InvalidInput,
                                             e.description());
                    return Err(err);
                }
            }
            if len < BUFFER_SIZE { break; }
        }
        return Ok(raw);
    }
}
