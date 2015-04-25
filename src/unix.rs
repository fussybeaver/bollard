use std;
use std::io::{self, Read, Write, Result, ErrorKind};
use std::error::Error;
use unix_socket;

pub struct UnixStream {
    stream: unix_socket::UnixStream
}

impl UnixStream {
    pub fn connect(addr: &str) -> Result<UnixStream> {
        let stream = try!(unix_socket::UnixStream::connect(addr));
        let unix_stream = UnixStream {
            stream: stream
        };
        return Ok(unix_stream);
    }
    
    pub fn read(&mut self, buf: &[u8]) -> Result<String> {
        match self.stream.write_all(buf) {
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
            let len = match self.stream.read(&mut buffer) {
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
