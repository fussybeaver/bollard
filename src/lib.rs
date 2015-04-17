//! Docker
#![doc(html_root_url="https://ghmlee.github.io/rust-docker/doc")]
extern crate unix_socket;
extern crate rustc_serialize;

pub mod container;
pub mod stats;
pub mod info;
pub mod image;
mod unix;
mod http;
mod test;

use std::io::{Result, Error, ErrorKind};
use rustc_serialize::json;
use unix::UnixStream;
use http::Http;
use container::Container;
use stats::Stats;
use info::Info;
use image::Image;

pub struct Docker {
    unix_stream: UnixStream,
    http: Http
}

impl Docker {
    pub fn new() -> Docker {
        let docker = Docker {
            unix_stream: UnixStream::connect("/var/run/docker.sock"),
            http: Http::new()
        };
        return docker;
    }

    pub fn get_containers(&self, all: bool) -> Result<Vec<Container>> {
        let a = match all {
            true => "1",
            false => "0"
        };
        let request = format!("GET /containers/json?all={}&size=1 HTTP/1.1\r\n\r\n", a);
        let raw = try!(self.unix_stream.read(request.as_bytes()));
        let response = try!(self.http.get_response(&raw));
        let body: Vec<Container> = match json::decode(&response) {
            Ok(body) => body,
            Err(_) => {
                let err = Error::new(ErrorKind::InvalidInput,
                                     "Container struct is invalid with a response.\n{}");
                return Err(err);
            }
        };
        return Ok(body);
    }

    pub fn get_stats(&self, container: &Container) -> Result<Stats> {
        let request = format!("GET /containers/{}/stats HTTP/1.1\r\n\r\n", container.Id);
        let raw = try!(self.unix_stream.read(request.as_bytes()));
        let response = try!(self.http.get_response(&raw));
        let body: Stats = match json::decode(&response) {
            Ok(body) => body,
            Err(_) => {
                let err = Error::new(ErrorKind::InvalidInput,
                                     "Stats struct is invalid with a response.");
                return Err(err);
            }
        };
        return Ok(body);
    }

    pub fn get_images(&self, all: bool) -> Result<Vec<Image>> {
        let a = match all {
            true => "1",
            false => "0"
        };
        let request = format!("GET /images/json?all={} HTTP/1.1\r\n\r\n", a);
        let raw = try!(self.unix_stream.read(request.as_bytes()));
        let response = try!(self.http.get_response(&raw));
        let body: Vec<Image> = match json::decode(&response) {
            Ok(body) => body,
            Err(_) => {
                let err = Error::new(ErrorKind::InvalidInput,
                                     "Image struct is invalid with a response.\n{}");
                return Err(err);
            }
        };
        return Ok(body);
    }

    pub fn get_info(&self) -> Result<Info> {
        let request = "GET /info HTTP/1.1\r\n\r\n";
        let raw = try!(self.unix_stream.read(request.as_bytes()));
        let response = try!(self.http.get_response(&raw));
        let body: Info = match json::decode(&response) {
            Ok(body) => body,
            Err(_) => {
                let err = Error::new(ErrorKind::InvalidInput,
                                     "Info struct is invalid with a response.");
                return Err(err);
            }
        };
        return Ok(body);
    }
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
