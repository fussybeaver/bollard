use std::io::{self, Read, Write, Result, ErrorKind};
use std::error::Error;
use unix_socket;

pub struct UnixStream {
    addr: String
}

impl UnixStream {
    pub fn connect(addr: &str) -> Result<UnixStream> {
        let unix_stream = UnixStream {
            addr: addr.to_string()
        };
        return Ok(unix_stream);
    }
    
    pub fn read(&mut self, buf: &[u8]) -> Result<Vec<u8>> {
        let mut stream = try!(unix_socket::UnixStream::connect(&*self.addr));
        
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
        let mut raw: Vec<u8> = Vec::new();
        loop {
            let len = match stream.read(&mut buffer) {
                Ok(len) => len,
                Err(e) => {
                    let err = io::Error::new(ErrorKind::ConnectionAborted,
                                             e.description());
                    return Err(err);
                }
            };
            
            for i in 0..len {
                raw.push(buffer[i]);
            }

            if len < BUFFER_SIZE { break; }
        }
        return Ok(raw);
    }
}
