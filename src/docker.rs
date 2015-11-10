use std;
use std::error::Error;
use std::path::Path;

use tcp::TcpStream;
use unix::UnixStream;
use http::{self, Response};

use container::{Container, ContainerInfo};
use process::{Process, Top};
use stats::Stats;
use system::SystemInfo;
use image::{Image, ImageStatus};
use filesystem::FilesystemChange;
use version::Version;

use rustc_serialize::json;

pub struct Docker {
    protocol: Protocol,
    unix_stream: Option<UnixStream>,
    tcp_stream: Option<TcpStream>,
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

        let unix_stream = match protocol {
            Protocol::UNIX => {
                let stream = try!(UnixStream::connect(&*path));
                Some(stream)
            }
            _ => None
        };

        let tcp_stream = match protocol {
            Protocol::TCP => {
                let stream = try!(TcpStream::connect(&*path));
                Some(stream)
            }
            _ => None
        };

        let docker = Docker {
            protocol: protocol,
            unix_stream: unix_stream,
            tcp_stream: tcp_stream
        };
        return Ok(docker);
    }

    pub fn set_tls(&mut self, key: &Path, cert: &Path, ca: &Path) -> std::io::Result<()> {
        match self.tcp_stream {
            Some(_) => {
                let mut tcp_stream = self.tcp_stream.as_mut().unwrap();
                try!(tcp_stream.set_ssl_context(key, cert, ca));
            }
            None => {}
        }

        return Ok(());
    }

    //
    // Containers
    //
    
    pub fn get_containers(&mut self, all: bool) -> std::io::Result<Vec<Container>> {
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
    
    pub fn get_processes(&mut self, container: &Container) -> std::io::Result<Vec<Process>> {
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

    pub fn get_stats(&mut self, container: &Container) -> std::io::Result<Stats> {
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
    
    pub fn create_image(&mut self, image: String, tag: String) -> std::io::Result<Vec<ImageStatus>> {
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

    pub fn get_images(&mut self, all: bool) -> std::io::Result<Vec<Image>> {
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

    pub fn get_system_info(&mut self) -> std::io::Result<SystemInfo> {
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

    pub fn get_container_info(&mut self, container: &Container) -> std::io::Result<ContainerInfo> {
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
    
    pub fn get_filesystem_changes(&mut self, container: &Container) -> std::io::Result<Vec<FilesystemChange>> {
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

    pub fn export_container(&mut self, container: &Container) -> std::io::Result<Vec<u8>> {
        let request = format!("GET /containers/{}/export HTTP/1.1\r\n\r\n", container.Id);
        let raw = try!(self.read(request.as_bytes()));
        let response = try!(self.get_response(&raw));
        try!(self.get_status_code(&response));
        
        return Ok(response.body);
    }

     pub fn ping(&mut self) -> std::io::Result<String> {
        let request = format!("GET /_ping HTTP/1.1\r\n\r\n");
        let raw = try!(self.read(request.as_bytes()));
        let response = try!(self.get_response(&raw));
        try!(self.get_status_code(&response));
        let encoded_body = try!(response.get_encoded_body());

        return Ok(encoded_body);
     }

    pub fn get_version(&mut self) -> std::io::Result<Version> {
        let request = format!("GET /version HTTP/1.1\r\n\r\n");
        let raw = try!(self.read(request.as_bytes()));
        let response = try!(self.get_response(&raw));
        try!(self.get_status_code(&response));
        let encoded_body = try!(response.get_encoded_body());

        let version: Version = match json::decode(&encoded_body){
            Ok(body) => body,
            Err(e) => {
                let err = std::io::Error::new(std::io::ErrorKind::InvalidInput, e.description());
                return Err(err);
            }
        };
        return Ok(version);
    }

    fn read(&mut self, buf: &[u8]) -> std::io::Result<Vec<u8>> {
        return match self.protocol {
            Protocol::UNIX => {
                let mut stream = self.unix_stream.as_mut().unwrap();
                stream.read(buf)
            }
            Protocol::TCP => {
                let mut stream = self.tcp_stream.as_mut().unwrap();
                stream.read(buf)
            }
        };
    }

    fn get_response(&self, raw: &Vec<u8>) -> std::io::Result<Response> {
        http::get_response(raw)
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



impl Clone for Docker {
    fn clone(&self)-> Self {
        let protocol = match self.protocol {
            Protocol::UNIX => Protocol::UNIX,
            Protocol::TCP => Protocol::TCP
        };
        
        let unix_stream = match self.unix_stream {
            Some(ref unix_stream) => {
                Some(unix_stream.clone())
            }
            None => None
        };

        let tcp_stream = match self.tcp_stream {
            Some(ref tcp_stream) => {
                Some(tcp_stream.clone())
            }
            None => None
        };
        
        let docker = Docker {
            protocol: protocol,
            unix_stream: unix_stream,
            tcp_stream: tcp_stream
        };
        return docker;
    }
}
