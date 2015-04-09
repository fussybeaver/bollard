//! Docker
#![doc(html_root_url="https://ghmlee.github.io/rust-docker/doc")]
extern crate unix_socket;
extern crate rustc_serialize;

pub mod container;
pub mod stats;
pub mod info;
mod test;

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

    pub fn get_containers(&self, all: bool) -> io::Result<Vec<Container>> {
        let a = match all {
            true => "1",
            false => "0"
        };
        let request = format!("GET /containers/json?all={}&size=1 HTTP/1.1\r\n\r\n", a);
        let raw = try!(self.read(&request));
        let response = try!(self.get_response(&raw));
        let body: Vec<Container> = match json::decode(&response) {
            Ok(body) => body,
            Err(_) => {
                let err = io::Error::new(io::ErrorKind::InvalidInput,
                                         "Container is invalid with a response.\n{}");
                return Err(err);
            }
        };
        return Ok(body);
    }

    pub fn get_stats(&self, container: &Container) -> io::Result<Stats> {
        let request = format!("GET /containers/{}/stats HTTP/1.1\r\n\r\n", container.Id);
        let raw = try!(self.read(&request));
        let response = try!(self.get_response(&raw));
        let body: Stats = match json::decode(&response) {
            Ok(body) => body,
            Err(_) => {
                let err = io::Error::new(io::ErrorKind::InvalidInput,
                                         "Stats is invalid with a response.");
                return Err(err);
            }
        };
        return Ok(body);
    }

    pub fn get_info(&self) -> io::Result<Info> {
        let request = "GET /info HTTP/1.1\r\n\r\n";
        let raw = try!(self.read(&request));
        let response = try!(self.get_response(&raw));
        let body: Info = match json::decode(&response) {
            Ok(body) => body,
            Err(_) => {
                let err = io::Error::new(io::ErrorKind::InvalidInput,
                                         "Info is invalid with a response.");
                return Err(err);
            }
        };
        return Ok(body);
    }

    fn read(&self, request: &str) -> io::Result<String> {
        let mut stream = match UnixStream::connect("/var/run/docker.sock") {
            Ok(stream) => stream,
            Err(_) => {
                let err = io::Error::new(io::ErrorKind::NotConnected,
                                         "The stream is not connected.");
                return Err(err);
            }
        };

        match stream.write_all(request.as_bytes()) {
            Ok(_) => {}
            Err(_) => {
                let err = io::Error::new(io::ErrorKind::ConnectionAborted,
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
                    let err = io::Error::new(io::ErrorKind::ConnectionAborted,
                                             "A read operation is failed from the stream.");
                    return Err(err);
                }
            };
            match std::str::from_utf8(&buffer[0 .. len]) {
                Ok(buf) => raw.push_str(buf),
                Err(_) => {} // It is required to handle this error.
            }
            if len < BUFFER_SIZE { break; }
        }
        return Ok(raw);
    }

    fn get_response(&self, raw: &str) -> io::Result<String> {
        let http_response: Vec<&str> = raw.split("\r\n\r\n").collect();

        if http_response.len() < 2 {
            let err = io::Error::new(io::ErrorKind::InvalidInput,
                                     "Docker returns an invalid response.");
            return Err(err);
        }
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
fn new() {
    let _ = Docker::new();
}

#[test]
fn get_containers() {
    let docker = Docker::new();
    let raw = test::get_containers_response();
    let response = match docker.get_response(&raw) {
        Ok(response) => response,
        Err(_) => { assert!(false); return; }
    };
    let _: Vec<Container> = match json::decode(&response) {
        Ok(body) => body,
        Err(_) => { assert!(false); return; }
    };
}

#[test]
fn get_stats() {
    let docker = Docker::new();
    let raw = test::get_stats_response();
    let response = match docker.get_response(&raw) {
        Ok(response) => response,
        Err(_) => { assert!(false); return; }
    };
    let _: Stats = match json::decode(&response) {
        Ok(body) => body,
        Err(_) => { assert!(false); return; }
    };
}

#[test]
fn get_info() {
    let docker = Docker::new();
    let raw = test::get_info_response();
    let response = match docker.get_response(&raw) {
        Ok(response) => response,
        Err(_) => { assert!(false); return; }
    };
    let _: Info = match json::decode(&response) {
        Ok(body) => body,
        Err(_) => { assert!(false); return; }
    };
}
