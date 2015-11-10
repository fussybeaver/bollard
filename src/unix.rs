use std;
use std::io::{Read, Write};
use std::error::Error;

use unix_socket;

pub struct UnixStream {
    stream: unix_socket::UnixStream
}

impl UnixStream {
    pub fn connect(addr: &str) -> std::io::Result<UnixStream> {
        let stream = try!(unix_socket::UnixStream::connect(addr));
        
        let unix_stream = UnixStream {
            stream: stream
        };
        
        return Ok(unix_stream);
    }
    
    pub fn read(&mut self, buf: &[u8]) -> std::io::Result<Vec<u8>> {
        let mut stream = self.stream.try_clone().unwrap();
        
        match stream.write_all(buf) {
            Ok(_) => {}
            Err(e) => {
                let err = std::io::Error::new(std::io::ErrorKind::ConnectionAborted,
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
                    let err = std::io::Error::new(std::io::ErrorKind::ConnectionAborted,
                                                  e.description());
                    return Err(err);
                }
            };
            
            for i in 0..len { raw.push(buffer[i]); }

            if len > 4 && buffer[len - 5] == 48 && buffer[len - 4] == 13 && buffer[len - 3] == 10 && buffer[len - 2] == 13 && buffer[len - 1] == 10 { break; }
            if len > 1 && buffer[len - 2] == 13 && buffer[len - 1] == 10 { continue; }
            if len < BUFFER_SIZE { break; }
        }
        return Ok(raw);
    }
}

impl Clone for UnixStream {
    fn clone(&self) -> Self {
        let stream = UnixStream {
            stream: self.stream.try_clone().unwrap()
        };
        
        return stream;
    }
}
