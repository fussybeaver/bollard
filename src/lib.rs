//! Docker
#![doc(html_root_url="https://ghmlee.github.io/rust-docker/doc")]
extern crate unix_socket;
extern crate rustc_serialize;

pub mod container;
pub mod stats;
pub mod info;
pub mod image;
mod http;
mod test;

use std::io::{self, Read, Write};
use unix_socket::UnixStream;
use rustc_serialize::json;
use container::Container;
use stats::Stats;
use info::Info;
use image::Image;
use http::Http;

pub struct Docker {
    http: Http
}

impl Docker {
    pub fn new() -> Docker {
        let docker = Docker {
            http: Http::new()
        };
        return docker;
    }

    pub fn get_containers(&self, all: bool) -> io::Result<Vec<Container>> {
        let a = match all {
            true => "1",
            false => "0"
        };
        let request = format!("GET /containers/json?all={}&size=1 HTTP/1.1\r\n\r\n", a);
        let raw = try!(self.read(&request));
        let response = try!(self.http.get_response(&raw));
        let body: Vec<Container> = match json::decode(&response) {
            Ok(body) => body,
            Err(_) => {
                let err = io::Error::new(io::ErrorKind::InvalidInput,
                                         "Container struct is invalid with a response.\n{}");
                return Err(err);
            }
        };
        return Ok(body);
    }

    pub fn get_stats(&self, container: &Container) -> io::Result<Stats> {
        let request = format!("GET /containers/{}/stats HTTP/1.1\r\n\r\n", container.Id);
        let raw = try!(self.read(&request));
        let response = try!(self.http.get_response(&raw));
        let body: Stats = match json::decode(&response) {
            Ok(body) => body,
            Err(_) => {
                let err = io::Error::new(io::ErrorKind::InvalidInput,
                                         "Stats struct is invalid with a response.");
                return Err(err);
            }
        };
        return Ok(body);
    }

    pub fn get_images(&self, all: bool) -> io::Result<Vec<Image>> {
        let a = match all {
            true => "1",
            false => "0"
        };
        let request = format!("GET /images/json?all={} HTTP/1.1\r\n\r\n", a);
        let raw = try!(self.read(&request));
        let response = try!(self.http.get_response(&raw));
        let body: Vec<Image> = match json::decode(&response) {
            Ok(body) => body,
            Err(_) => {
                let err = io::Error::new(io::ErrorKind::InvalidInput,
                                         "Image struct is invalid with a response.\n{}");
                return Err(err);
            }
        };
        return Ok(body);
    }

    pub fn get_info(&self) -> io::Result<Info> {
        let request = "GET /info HTTP/1.1\r\n\r\n";
        let raw = try!(self.read(&request));
        let response = try!(self.http.get_response(&raw));
        let body: Info = match json::decode(&response) {
            Ok(body) => body,
            Err(_) => {
                let err = io::Error::new(io::ErrorKind::InvalidInput,
                                         "Info struct is invalid with a response.");
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
}

#[test]
fn new() {
    let _ = Docker::new();
}

#[test]
fn get_containers() {
    let http = Http::new();
    let raw = test::get_containers_response();
    let response = match http.get_response(&raw) {
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
    let http = Http::new();
    let raw = test::get_stats_response();
    let response = match http.get_response(&raw) {
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
    let http = Http::new();
    let raw = test::get_info_response();
    let response = match http.get_response(&raw) {
        Ok(response) => response,
        Err(_) => { assert!(false); return; }
    };
    let _: Info = match json::decode(&response) {
        Ok(body) => body,
        Err(_) => { assert!(false); return; }
    };
}

#[test]
fn get_images() {
    let http = Http::new();
    let raw = test::get_images_response();
    let response = match http.get_response(&raw) {
        Ok(response) => response,
        Err(_) => { assert!(false); return; }
    };
    let _: Vec<Image> = match json::decode(&response) {
        Ok(body) => body,
        Err(_) => { assert!(false); return; }
    };
}
