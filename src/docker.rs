use std::io::{self, Result, ErrorKind};
use std::error::Error;
use std::path::Path;
use std::sync::Arc;
use rustc_serialize::json;
use tcp::TcpStream;
use unix::UnixStream;
use http::Http;
use container::Container;
use stats::Stats;
use info::Info;
use image::Image;
use openssl;
#[cfg(test)]
use test;

pub struct Docker {
    protocol: Protocol,
    addr: String,
    http: Http,
    tls: bool,
    ssl_context: Option<Arc<openssl::ssl::SslContext>>
}

enum Protocol {
    UNIX,
    TCP
}

impl Docker {
    pub fn connect(addr: &str) -> Result<Docker> {
        let components: Vec<&str> = addr.split("://").collect();
        if components.len() != 2 {
            let err = io::Error::new(ErrorKind::InvalidInput,
                                     "The address is invalid.");
            return Err(err);
        }
        
        let protocol = components[0];
        let path = components[1];

        let protocol = match protocol {
            "unix" => Protocol::UNIX,
            "tcp" => Protocol::TCP,
            _ => {
                let err = io::Error::new(ErrorKind::InvalidInput,
                                         "The protocol is not supported.");
                return Err(err);
            }
        };

        let docker = Docker {
            protocol: protocol,
            addr: path.to_string(),
            http: Http::new(),
            tls: false,
            ssl_context: None
        };
        return Ok(docker);
    }

    pub fn set_tls(&mut self, key: &Path, cert: &Path, ca: &Path) -> Result<()> {
        self.tls = true;
        let mut context = match openssl::ssl::SslContext::new(openssl::ssl::SslMethod::Tlsv1) {
            Ok(context) => context,
            Err(e) => {
                let err = io::Error::new(ErrorKind::NotConnected,
                                         e.description());
                return Err(err);
            }
        };

        match context.set_private_key_file(key, openssl::x509::X509FileType::PEM) {
            Ok(_) => {}
            Err(e) => {
                let err = io::Error::new(ErrorKind::InvalidInput,
                                         e.description());
                return Err(err);
            }
        }

        match context.set_certificate_file(cert, openssl::x509::X509FileType::PEM) {
            Ok(_) => {}
            Err(e) => {
                let err = io::Error::new(ErrorKind::NotConnected,
                                         e.description());
                return Err(err);
            }
        }

        match context.set_CA_file(ca) {
            Ok(_) => {}
            Err(e) => {
                let err = io::Error::new(ErrorKind::NotConnected,
                                         e.description());
                return Err(err);
            }
        }

        self.ssl_context = Some(Arc::new(context));
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
        match response.status_code {
            200 => {}
            400 => {
                let err = io::Error::new(ErrorKind::InvalidInput,
                                         "Docker returns an error with 400 status code.");
                return Err(err);
            }
            500 => {
                let err = io::Error::new(ErrorKind::InvalidInput,
                                         "Docker returns an error with 500 status code.");
                return Err(err);
            }
            _ => {
                let err = io::Error::new(ErrorKind::InvalidInput,
                                         "Docker returns an error with an invalid status code.");
                return Err(err);
            }
        }
        let body: Vec<Container> = match json::decode(&response.body) {
            Ok(body) => body,
            Err(e) => {
                let err = io::Error::new(ErrorKind::InvalidInput,
                                         e.description());
                return Err(err);
            }
        };
        return Ok(body);
    }

    pub fn get_stats(&self, container: &Container) -> Result<Stats> {
        if container.Status.contains("Up") == false {
            let err = io::Error::new(ErrorKind::InvalidInput,
                                     "The container is already stopped.");
            return Err(err);
        }

        let request = format!("GET /containers/{}/stats HTTP/1.1\r\n\r\n", container.Id);
        let raw = try!(self.read(request.as_bytes()));
        let response = try!(self.http.get_response(&raw));
        match response.status_code {
            200 => {}
            400 => {
                let err = io::Error::new(ErrorKind::InvalidInput,
                                         "Docker returns an error with 400 status code.");
                return Err(err);
            }
            500 => {
                let err = io::Error::new(ErrorKind::InvalidInput,
                                         "Docker returns an error with 500 status code.");
                return Err(err);
            }
            _ => {
                let err = io::Error::new(ErrorKind::InvalidInput,
                                         "Docker returns an error with an invalid status code.");
                return Err(err);
            }
        }
        let body: Stats = match json::decode(&response.body) {
            Ok(body) => body,
            Err(e) => {
                let err = io::Error::new(ErrorKind::InvalidInput,
                                         e.description());
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
        match response.status_code {
            200 => {}
            400 => {
                let err = io::Error::new(ErrorKind::InvalidInput,
                                         "Docker returns an error with 400 status code.");
                return Err(err);
            }
            500 => {
                let err = io::Error::new(ErrorKind::InvalidInput,
                                         "Docker returns an error with 500 status code.");
                return Err(err);
            }
            _ => {
                let err = io::Error::new(ErrorKind::InvalidInput,
                                         "Docker returns an error with an invalid status code.");
                return Err(err);
            }
        }
        let body: Vec<Image> = match json::decode(&response.body) {
            Ok(body) => body,
            Err(e) => {
                let err = io::Error::new(ErrorKind::InvalidInput,
                                         e.description());
                return Err(err);
            }
        };
        return Ok(body);
    }

    pub fn get_info(&self) -> Result<Info> {
        let request = "GET /info HTTP/1.1\r\n\r\n";
        let raw = try!(self.read(request.as_bytes()));
        let response = try!(self.http.get_response(&raw));
        match response.status_code {
            200 => {}
            400 => {
                let err = io::Error::new(ErrorKind::InvalidInput,
                                         "Docker returns an error with 400 status code.");
                return Err(err);
            }
            500 => {
                let err = io::Error::new(ErrorKind::InvalidInput,
                                         "Docker returns an error with 500 status code.");
                return Err(err);
            }
            _ => {
                let err = io::Error::new(ErrorKind::InvalidInput,
                                         "Docker returns an error with an invalid status code.");
                return Err(err);
            }
        }
        let body: Info = match json::decode(&response.body) {
            Ok(body) => body,
            Err(e) => {
                let err = io::Error::new(ErrorKind::InvalidInput,
                                         e.description());
                return Err(err);
            }
        };
        return Ok(body);
    }

    fn read(&self, buf: &[u8]) -> Result<String> {
        return match self.protocol {
            Protocol::UNIX => {
                let mut stream = try!(UnixStream::connect(&*self.addr));
                stream.read(buf)
            }
            Protocol::TCP => {
                let mut stream = try!(TcpStream::connect(&*self.addr));
                if self.tls == true {
                    let ssl_context = self.ssl_context.clone().unwrap().clone();
                    try!(stream.set_ssl_context(ssl_context));
                }
                stream.read(buf)
            }
        };
    }
}

#[test]
#[cfg(test)]
fn get_containers() {
    let http = Http::new();
    let raw = test::get_containers_response();
    let response = match http.get_response(&raw) {
        Ok(response) => response,
        Err(_) => { assert!(false); return; }
    };
    match response.status_code {
        200 => {}
        _ => { assert!(false); return; }
    }
    let _: Vec<Container> = match json::decode(&response.body) {
        Ok(body) => body,
        Err(_) => { assert!(false); return; }
    };
}

#[test]
#[cfg(test)]
fn get_stats() {
    let http = Http::new();
    let raw = test::get_stats_response();
    let response = match http.get_response(&raw) {
        Ok(response) => response,
        Err(_) => { assert!(false); return; }
    };
    match response.status_code {
        200 => {}
        _ => { assert!(false); return; }
    }    
    let _: Stats = match json::decode(&response.body) {
        Ok(body) => body,
        Err(_) => { assert!(false); return; }
    };
}

#[test]
#[cfg(test)]
fn get_info() {
    let http = Http::new();
    let raw = test::get_info_response();
    let response = match http.get_response(&raw) {
        Ok(response) => response,
        Err(_) => { assert!(false); return; }
    };
    match response.status_code {
        200 => {}
        _ => { assert!(false); return; }
    }
    let _: Info = match json::decode(&response.body) {
        Ok(body) => body,
        Err(_) => { assert!(false); return; }
    };
}

#[test]
#[cfg(test)]
fn get_images() {
    let http = Http::new();
    let raw = test::get_images_response();
    let response = match http.get_response(&raw) {
        Ok(response) => response,
        Err(_) => { assert!(false); return; }
    };
    match response.status_code {
        200 => {}
        _ => { assert!(false); return; }
    }
    let _: Vec<Image> = match json::decode(&response.body) {
        Ok(body) => body,
        Err(_) => { assert!(false); return; }
    };
}
