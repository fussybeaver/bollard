use std;
use std::error::Error;
use std::path::Path;
use std::sync::Arc;
use std::convert::AsRef;

use openssl;
use rustc_serialize::json;

use tcp::TcpStream;
use unix::UnixStream;
use http::{Client, Response};

use container::{Container, ContainerInfo};
use process::{Process, Top};
use stats::Stats;
use system::SystemInfo;
use image::{Image, ImageStatus};
use filesystem::FilesystemChange;

pub struct Docker {
    protocol: Protocol,
    addr: String,
    tls: bool,
    client: Client,
    //unix_stream: Option<Arc<UnixStream>>,
    //tcp_stream: Option<Arc<TcpStream>>,
    ssl_context: Option<Arc<openssl::ssl::SslContext>>
}

enum Protocol {
    UNIX,
    TCP
}

impl Docker {
    pub fn connect(addr: &str) -> std::io::Result<Docker> {
        let components: Vec<&str> = addr.split("://").collect();
        if components.len() != 2 {
            let err = std::io::Error::new(std::io::ErrorKind::InvalidInput,
                                          "The address is invalid.");
            return Err(err);
        }
        
        let protocol = components[0];
        let path = components[1].to_string();

        let protocol = match protocol {
            "unix" => Protocol::UNIX,
            "tcp" => Protocol::TCP,
            _ => {
                let err = std::io::Error::new(std::io::ErrorKind::InvalidInput,
                                              "The protocol is not supported.");
                return Err(err);
            }
        };

        /*let mut unix_stream = match protocol {
            Protocol::UNIX => {
                let stream = try!(UnixStream::connect(&*path));
                Some(Arc::new(stream))
            }
            _ => None
        };

        let mut tcp_stream = match protocol {
            Protocol::TCP => {
                let stream = try!(TcpStream::connect(&*path));
                Some(Arc::new(stream))
            }
            _ => None
        };*/

        let docker = Docker {
            protocol: protocol,
            addr: path,
            tls: false,
            client: Client::new(),
            //unix_stream: None,
            //tcp_stream: None,
            ssl_context: None
        };
        return Ok(docker);
    }

    pub fn set_tls(&mut self, key: &Path, cert: &Path, ca: &Path) -> std::io::Result<()> {
        self.tls = true;
        let mut context = match openssl::ssl::SslContext::new(openssl::ssl::SslMethod::Tlsv1) {
            Ok(context) => context,
            Err(e) => {
                let err = std::io::Error::new(std::io::ErrorKind::NotConnected,
                                              e.description());
                return Err(err);
            }
        };

        match context.set_private_key_file(key, openssl::x509::X509FileType::PEM) {
            Ok(_) => {}
            Err(e) => {
                let err = std::io::Error::new(std::io::ErrorKind::InvalidInput,
                                              e.description());
                return Err(err);
            }
        }

        match context.set_certificate_file(cert, openssl::x509::X509FileType::PEM) {
            Ok(_) => {}
            Err(e) => {
                let err = std::io::Error::new(std::io::ErrorKind::NotConnected,
                                              e.description());
                return Err(err);
            }
        }

        match context.set_CA_file(ca) {
            Ok(_) => {}
            Err(e) => {
                let err = std::io::Error::new(std::io::ErrorKind::NotConnected,
                                              e.description());
                return Err(err);
            }
        }

        self.ssl_context = Some(Arc::new(context));
        return Ok(());
    }

    //
    // Containers
    //
    
    pub fn get_containers(&self, all: bool) -> std::io::Result<Vec<Container>> {
        let a = match all {
            true => "1",
            false => "0"
        };
        
        let request = format!("GET /containers/json?all={}&size=1 HTTP/1.1\r\n\r\n", a);
        let raw = try!(self.read(request.as_bytes()));
        let response = try!(self.get_response(&raw));
        try!(self.get_status_code(&response));
        let body = try!(response.get_encoded_body());
        let containers: Vec<Container> = match json::decode(&body) {
            Ok(containers) => containers,
            Err(e) => {
                let err = std::io::Error::new(std::io::ErrorKind::InvalidInput,
                                              e.description());
                return Err(err);
            }
        };
        
        return Ok(containers);
    }
    
    pub fn get_processes(&self, container: &Container) -> std::io::Result<Vec<Process>> {
        let request = format!("GET /containers/{}/top HTTP/1.1\r\n\r\n", container.Id);
        let raw = try!(self.read(request.as_bytes()));
        let response = try!(self.get_response(&raw));
        try!(self.get_status_code(&response));
        let body = try!(response.get_encoded_body()); 
        
        let top: Top = match json::decode(&body) {
            Ok(top) => top,
            Err(e) => {
                let err = std::io::Error::new(std::io::ErrorKind::InvalidInput,
                                              e.description());
                return Err(err);
            }
        };

        let mut processes: Vec<Process> = Vec::new();
        let mut process_iter = top.Processes.iter();
        loop {
            let process = match process_iter.next() {
                Some(process) => process,
                None => { break; }
            };

            let mut p = Process{
                user: String::new(),
                pid: String::new(),
                cpu: None,
                memory: None,
                vsz: None,
                rss: None,
                tty: None,
                stat: None,
                start: None,
                time: None,
                command: String::new()
            };
            
            let mut value_iter = process.iter();
            let mut i: usize = 0;
            loop {
                let value = match value_iter.next() {
                    Some(value) => value,
                    None => { break; }
                };
                let key = &top.Titles[i];
                match key.as_ref() {
                    "USER" => { p.user = value.clone() },
                    "PID" => { p.pid = value.clone() },
                    "%CPU" => { p.cpu = Some(value.clone()) },
                    "%MEM" => { p.memory = Some(value.clone()) },
                    "VSZ" => { p.vsz = Some(value.clone()) },
                    "RSS" => { p.rss = Some(value.clone()) },
                    "TTY" => { p.tty = Some(value.clone()) },
                    "STAT" => { p.stat = Some(value.clone()) },
                    "START" => { p.start = Some(value.clone()) },
                    "TIME" => { p.time = Some(value.clone()) },
                    "COMMAND" => { p.command = value.clone() },
                    _ => {}
                }

                i = i + 1;
            };

            processes.push(p);
        }

        return Ok(processes);
    }

    pub fn get_stats(&self, container: &Container) -> std::io::Result<Stats> {
        if container.Status.contains("Up") == false {
            let err = std::io::Error::new(std::io::ErrorKind::InvalidInput,
                                          "The container is already stopped.");
            return Err(err);
        }

        let request = format!("GET /containers/{}/stats HTTP/1.1\r\n\r\n", container.Id);
        let raw = try!(self.read(request.as_bytes()));
        let response = try!(self.get_response(&raw));
        try!(self.get_status_code(&response));
        let body = try!(response.get_encoded_body());
        
        let stats: Stats = match json::decode(&body) {
            Ok(stats) => stats,
            Err(e) => {
                let err = std::io::Error::new(std::io::ErrorKind::InvalidInput,
                                              e.description());
                return Err(err);
            }
        };
        return Ok(stats);
    }

    //
    // Image
    //
    
    pub fn create_image(&self, image: String, tag: String) -> std::io::Result<Vec<ImageStatus>> {
        let request = format!("POST /images/create?fromImage={}&tag={} HTTP/1.1\r\n\r\n", image, tag);
        let raw = try!(self.read(request.as_bytes()));
        let response = try!(self.get_response(&raw));
        let body = format!("[{}]", try!(response.get_encoded_body()));
        let fixed = body.replace("}{", "},{");
        
        let statuses: Vec<ImageStatus> = match json::decode(&fixed) {
            Ok(statuses) => statuses,
            Err(e) => {
                let err = std::io::Error::new(std::io::ErrorKind::InvalidInput,
                                              e.description());
                return Err(err);
            }
        };
        return Ok(statuses);
    }

    pub fn get_images(&self, all: bool) -> std::io::Result<Vec<Image>> {
        let a = match all {
            true => "1",
            false => "0"
        };
        let request = format!("GET /images/json?all={} HTTP/1.1\r\n\r\n", a);
        let raw = try!(self.read(request.as_bytes()));
        let response = try!(self.get_response(&raw));
        try!(self.get_status_code(&response));
        let body = try!(response.get_encoded_body());
        
        let images: Vec<Image> = match json::decode(&body) {
            Ok(images) => images,
            Err(e) => {
                let err = std::io::Error::new(std::io::ErrorKind::InvalidInput,
                                              e.description());
                return Err(err);
            }
        };
        return Ok(images);
    }

    pub fn get_system_info(&self) -> std::io::Result<SystemInfo> {
        let request = "GET /info HTTP/1.1\r\n\r\n";
        let raw = try!(self.read(request.as_bytes()));
        let response = try!(self.get_response(&raw));
        try!(self.get_status_code(&response));
        let body = try!(response.get_encoded_body());
        
        let info: SystemInfo = match json::decode(&body) {
            Ok(info) => info,
            Err(e) => {
                let err = std::io::Error::new(std::io::ErrorKind::InvalidInput,
                                              e.description());
                return Err(err);
            }
        };
        return Ok(info);
    }

    pub fn get_container_info(&self, container: &Container) -> std::io::Result<ContainerInfo> {
        let request = format!("GET /containers/{}/json HTTP/1.1\r\n\r\n", container.Id);
        let raw = try!(self.read(request.as_bytes()));
        let response = try!(self.get_response(&raw));
        try!(self.get_status_code(&response));
        let body = try!(response.get_encoded_body());
        
        let container_info: ContainerInfo = match json::decode(&body) {
            Ok(body) => body,
            Err(e) => {
                let err = std::io::Error::new(std::io::ErrorKind::InvalidInput,
                                              e.description());
                return Err(err);
            }
        };
        return Ok(container_info);
    }
    
    pub fn get_filesystem_changes(&self, container: &Container) -> std::io::Result<Vec<FilesystemChange>> {
        let request = format!("GET /containers/{}/changes HTTP/1.1\r\n\r\n", container.Id);
        let raw = try!(self.read(request.as_bytes()));
        let response = try!(self.get_response(&raw));
        try!(self.get_status_code(&response));
        let body = try!(response.get_encoded_body());
        
        let filesystem_changes: Vec<FilesystemChange> = match json::decode(&body) {
            Ok(body) => body,
            Err(e) => {
                let err = std::io::Error::new(std::io::ErrorKind::InvalidInput,
                                              e.description());
                return Err(err);
            }
        };
        return Ok(filesystem_changes);
    }

    pub fn export_container(&self, container: &Container) -> std::io::Result<Vec<u8>> {
        let request = format!("GET /containers/{}/export HTTP/1.1\r\n\r\n", container.Id);
        let raw = try!(self.read(request.as_bytes()));
        let response = try!(self.get_response(&raw));
        try!(self.get_status_code(&response));
        
        return Ok(response.body);
    }

     pub fn ping(&self) -> std::io::Result<String> {
        let request = format!("GET /_ping HTTP/1.1\r\n\r\n");
        let raw = try!(self.read(request.as_bytes()));
        let response = try!(self.get_response(&raw));
        try!(self.get_status_code(&response));
        let encoded_body = try!(response.get_encoded_body());

        return Ok(encoded_body);
     }

    fn read(&self, buf: &[u8]) -> std::io::Result<Vec<u8>> {
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

    fn get_response(&self, raw: &Vec<u8>) -> std::io::Result<Response> {
        self.client.get_response(raw)
    }

    fn get_status_code(&self, response: &Response) -> std::io::Result<()> {
        let status_code = response.status_code;
        match status_code / 100 {
            2 => { Ok(()) }
            _ => {
                let desc = format!("Docker returns an error with {} status code.", status_code);
                let err = std::io::Error::new(std::io::ErrorKind::InvalidInput, desc);
                return Err(err);
            }
        }
    }
}
