use std;
use std::io::{Result, Error, ErrorKind};
use std::collections::HashMap;

pub struct Response {
    pub http_version: String,
    pub status_code: u16,
    pub headers: HashMap<String, String>,
    pub body: Vec<u8>
}
    
pub fn get_response(raw: &Vec<u8>) -> std::io::Result<Response> {
    let mut headers_raw: Vec<u8> = Vec::new();
    let mut body_raw: Vec<u8> = Vec::new();
    let mut body_index: usize = 0;

    // headers
    for i in 0..raw.len() {
        if i + 3 >= raw.len() { break; }
        
        if raw[i] == 13 && raw[i+1] == 10 && raw[i+2] == 13 && raw[i+3] == 10 { // CRLFCRLF
            body_index = i + 4;
            if body_index >= raw.len() { body_index = raw.len() - 1; }
            break;
        }
        
        headers_raw.push(raw[i]);
    }

    // body
    for i in body_index..raw.len() {
        body_raw.push(raw[i]);
    }
    
    let headers = match String::from_utf8(headers_raw) {
        Ok(headers) => headers,
        Err(_) => {
            let err = Error::new(ErrorKind::InvalidInput,
                                 "Docker returns an invalid http response.");
            return Err(err);
        }
    };

    let mut http_version = String::new();
    let mut status_code: u16 = 0;
    let mut headers_map: HashMap<String, String> = HashMap::new();
    let mut index: usize = 0;
        
    let lines: Vec<&str> = headers.split("\r\n").collect();
    for line in lines.iter() {
        index += 1;

        if index == 1 {
            let items: Vec<&str> = line.split(" ").collect();
            if items.len() < 2 { continue; }
            http_version = items[0].to_string();
            status_code = match std::str::FromStr::from_str(items[1]) {
                Ok(i) => i,
                Err(_) => 0
            };
            continue;
        }

        let items: Vec<&str> = line.split(": ").collect();
        if items.len() != 2 { continue; }
        let key = items[0].to_string();
        let value = items[1].to_string();
        headers_map.insert(key, value);
    }
        
    let response = Response{
        http_version: http_version,
        status_code: status_code,
        headers: headers_map,
        body: body_raw
    };
    
    return Ok(response);
}

impl Response {
    pub fn get_encoded_body(&self) -> Result<String> {
        let encoded_body = match String::from_utf8(self.body.clone()) {
            Ok(headers) => headers,
            Err(_) => {
                let err = Error::new(ErrorKind::InvalidInput,
                                     "Docker returns an invalid http response.");
                return Err(err);
            }
        };

        let chunked_body: Vec<&str> = encoded_body.split("\r\n").collect();
        if chunked_body.len() == 1 { // not chunked
            return Ok(encoded_body.clone());
        } else { // chunked
            let mut chunks = String::new();
            let mut index: i64 = 0;
            for chunk in chunked_body.iter() {
                if chunk.len() == 0 { continue; }
                index = index + 1;
                if index % 2 != 0 { continue; }
                chunks.push_str(chunk);
            }
            return Ok(chunks);
        }
    }
}
