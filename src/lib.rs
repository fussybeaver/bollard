//! Docker
#![doc(html_root_url="https://ghmlee.github.io/rust-docker/doc")]
extern crate unix_socket;
extern crate rustc_serialize;

pub mod container;
pub mod stats;
pub mod info;

use std::io::{self, Read, Write};
use unix_socket::UnixStream;
use rustc_serialize::json;

use container::Container;
use stats::Stats;
use info::Info;

pub struct Docker;

impl Docker {
    pub fn new() -> Docker {
        return Docker;
    }

    pub fn get_containers(&self) -> io::Result<Vec<Container>> {
        let request = "GET /containers/json HTTP/1.1\r\n\r\n";
        let response = try!(self.read(request));
        let decoded_body: Vec<Container> = json::decode(&response).unwrap();
        return Ok(decoded_body);
    }

    pub fn get_stats(&self, container: &Container) -> io::Result<Stats> {
        let request = format!("GET /containers/{}/stats HTTP/1.1\r\n\r\n", container.Id);
        let response = try!(self.read(&request));
        let decoded_body: Stats = json::decode(&response).unwrap();
        return Ok(decoded_body);
    }

    pub fn get_info(&self) -> io::Result<Info> {
        let request = "GET /info HTTP/1.1\r\n\r\n";
        let response = try!(self.read(request));
        let decoded_body: Info = json::decode(&response).unwrap();
        return Ok(decoded_body);
    }

    fn read(&self, request: &str) -> io::Result<String> {
        let mut unix_stream = try!(UnixStream::connect("/var/run/docker.sock"));
        try!(unix_stream.write_all(request.as_bytes()));

        const BUFFER_SIZE: usize = 1024;
        let mut buffer: [u8; BUFFER_SIZE] = [0; BUFFER_SIZE];
        let mut raw = String::new();
        loop {
            let len = try!(unix_stream.read(&mut buffer));
            match std::str::from_utf8(&buffer[0 .. len]) {
                Ok(buf) => raw.push_str(buf),
                Err(_) => {} // It is required to handle this error.
            }
            
            if len < BUFFER_SIZE { break; }
        }
        
        let http_response: Vec<&str> = raw[..].split("\r\n\r\n").collect();
        
        //let http_header = http_response[0];
        let http_body = http_response[1];
        let chunked_content_body: Vec<&str> = http_body[..].split("\r\n").collect();
        let mut content_body = String::new();
        
        if chunked_content_body.len() == 1 {
            content_body.push_str(http_body);
        } else {
            let mut index: i64 = 0;
            for chunk in chunked_content_body.iter() {
                index = index + 1;
                if index % 2 != 0 { continue; }
                content_body.push_str(chunk);
            }
        }

        return Ok(content_body);
    }
}

#[test]
fn it_works() {
    Docker::new();
}
