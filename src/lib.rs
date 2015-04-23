//! Docker
#![doc(html_root_url="https://ghmlee.github.io/rust-docker/doc")]
extern crate openssl;
extern crate unix_socket;
extern crate rustc_serialize;

mod tcp;
mod unix;
mod http;
mod test;
pub mod container;
pub mod stats;
pub mod info;
pub mod image;

use std::io::{Result, Error, ErrorKind};
use std::path::Path;
use rustc_serialize::json;
use tcp::TcpStream;
use unix::UnixStream;
use http::Http;
use container::Container;
use stats::Stats;
use info::Info;
use image::Image;

pub struct Docker {
    protocol: Protocol,
    tls: bool,
    addr: String,
    http: Http,
    key_path: Option<String>,
    cert_path: Option<String>,
    ca_path: Option<String>
}

enum Protocol {
    UNIX,
    TCP
}

impl Docker {
    pub fn connect(addr: &str) -> Result<Docker> {
        let components: Vec<&str> = addr.split("://").collect();
        if components.len() != 2 {
            let err = Error::new(ErrorKind::InvalidInput,
                                 "The address is invalid.");
            return Err(err);
        }
        
        let protocol = components[0];
        let path = components[1];

        let protocol = match protocol {
            "unix" => Protocol::UNIX,
            "tcp" => Protocol::TCP,
            _ => {
                let err = Error::new(ErrorKind::InvalidInput,
                                     "The protocol is not supported.");
                return Err(err);
            }
        };

        let docker = Docker {
            protocol: protocol,
            tls: false,
            addr: path.to_string(),
            http: Http::new(),
            key_path: None,
            cert_path: None,
            ca_path: None
        };
        return Ok(docker);
    }

    pub fn set_tls(&mut self, tls: bool) {
        self.tls = tls;
    }

    pub fn set_private_key_file(&mut self, path: &Path) -> Result<()> {
        self.key_path = match path.to_str() {
            Some(v) => Some(v.to_string()),
            None => {
                let err = Error::new(ErrorKind::InvalidInput,
                                     "The path is invalid.");
                return Err(err);
            }
        };
        return Ok(());
    }

    pub fn set_certificate_file(&mut self, path: &Path) -> Result<()> {
        self.cert_path = match path.to_str() {
            Some(v) => Some(v.to_string()),
            None => {
                let err = Error::new(ErrorKind::InvalidInput,
                                     "The path is invalid.");
                return Err(err);
            }
        };
        return Ok(());
    }

    pub fn set_ca_file(&mut self, path: &Path) -> Result<()> {
        self.ca_path = match path.to_str() {
            Some(v) => Some(v.to_string()),
            None => {
                let err = Error::new(ErrorKind::InvalidInput,
                                     "The path is invalid.");
                return Err(err);
            }
        };
        return Ok(());
    }

    pub fn get_containers(&self, all: bool) -> Result<Vec<Container>> {
        let a = match all {
            true => "1",
            false => "0"
        };
        let request = format!("GET /containers/json?all={}&size=1 HTTP/1.1\r\n\r\n", a);
        let raw = try!(self.read(request.as_bytes()));
        let response = try!(self.http.get_response(&raw));
        let body: Vec<Container> = match json::decode(&response) {
            Ok(body) => body,
            Err(_) => {
                let err = Error::new(ErrorKind::InvalidInput,
                                     "Container struct is invalid with a response.");
                return Err(err);
            }
        };
        return Ok(body);
    }

    pub fn get_stats(&self, container: &Container) -> Result<Stats> {
        if container.Status.contains("Up") == false {
            let err = Error::new(ErrorKind::InvalidInput,
                                 "This container is already stopped.");
            return Err(err);
        }

        let request = format!("GET /containers/{}/stats HTTP/1.1\r\n\r\n", container.Id);
        let raw = try!(self.read(request.as_bytes()));
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
        let raw = try!(self.read(request.as_bytes()));
        let response = try!(self.http.get_response(&raw));
        let body: Vec<Image> = match json::decode(&response) {
            Ok(body) => body,
            Err(_) => {
                let err = Error::new(ErrorKind::InvalidInput,
                                     "Image struct is invalid with a response.");
                return Err(err);
            }
        };
        return Ok(body);
    }

    pub fn get_info(&self) -> Result<Info> {
        let request = "GET /info HTTP/1.1\r\n\r\n";
        let raw = try!(self.read(request.as_bytes()));
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

    fn read(&self, buf: &[u8]) -> Result<String> {
        return match self.protocol {
            Protocol::UNIX => {
                let stream = UnixStream::connect(&self.addr);
                stream.read(buf)
            }
            Protocol::TCP => {
                
                match self.tls {
                    false => {
                        let stream = TcpStream::connect(&self.addr);
                        stream.read(buf)
                    }
                    true => {
                        if self.key_path == None ||
                           self.cert_path == None ||
                           self.ca_path == None {
                            let err = Error::new(ErrorKind::InvalidInput,
                                                 "key, cert, CA are required on TLS.");
                            return Err(err);
                        }

                        let key_path = self.key_path.clone().unwrap();
                        let cert_path = self.cert_path.clone().unwrap();
                        let ca_path = self.ca_path.clone().unwrap();

                        let mut stream = TcpStream::connect(&self.addr);

                        stream.set_tls(self.tls);
                        stream.set_private_key_file(&key_path);
                        stream.set_certificate_file(&cert_path);
                        stream.set_ca_file(&ca_path);

                        stream.read(buf)
                    }
                }
            }
            
        };
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
