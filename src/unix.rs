use std;
use std::io::{Read, Write, Result, Error, ErrorKind};
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
            Err(_) => {
                let err = Error::new(ErrorKind::NotConnected,
                                     "The stream is not connected.");
                return Err(err);
            }
        };
        
        match stream.write_all(buf) {
            Ok(_) => {}
            Err(_) => {
                let err = Error::new(ErrorKind::ConnectionAborted,
                                     "A write operation is failed to the stream.");
                return Err(err);
            }
        };

        const BUFFER_SIZE: usize = 1024;
        let mut buffer: [u8; BUFFER_SIZE] = [0; BUFFER_SIZE];
        let mut raw = String::new();
        loop {
            let len = match stream.read(&mut buffer) {
                Ok(len) => len,
                Err(_) => {
                    let err = Error::new(ErrorKind::ConnectionAborted,
                                         "A read operation is failed from the stream.");
                    return Err(err);
                }
            };
            match std::str::from_utf8(&buffer[0 .. len]) {
                Ok(buf) => raw.push_str(buf),
                Err(_) => {
                    let err = Error::new(ErrorKind::InvalidInput,
                                         "Docker returns invalid utf-8 buffers.");
                    return Err(err);
                }
            }
            if len < BUFFER_SIZE { break; }
        }
        return Ok(raw);
    }
}
