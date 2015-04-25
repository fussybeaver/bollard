use std;
use std::io::{Result, Error, ErrorKind};
use std::collections::HashMap;

pub struct Http;

struct Response {
    pub http_version: String,
    pub status_code: u16,
    pub headers: HashMap<String, String>,
    pub body: String
}

impl Http {
    pub fn new() -> Http {
        return Http;
    }
    
    pub fn get_response(&self, raw: &str) -> Result<Response> {
        let http_response: Vec<&str> = raw.split("\r\n\r\n").collect();

        if http_response.len() < 2 {
            let err = Error::new(ErrorKind::InvalidInput,
                                 "Docker returns an invalid response.");
            return Err(err);
        }
        let http_header = http_response[0];
        let http_body = http_response[1];
        let chunked_content_body: Vec<&str> = http_body.split("\r\n").collect();
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

        let response = Response::new(&http_header, &content_body);
        return Ok(response);
    }
}

impl Response {
    pub fn new(headers: &str, body: &str) -> Response {
        let mut http_version = String::new();
        let mut status_code: u16 = 0;
        let mut refined_headers: HashMap<String, String> = HashMap::new();
        let lines: Vec<&str> = headers.split("\r\n").collect();
        let mut index = 0;
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
            refined_headers.insert(key, value);
        }
        let response = Response {
            http_version: http_version,
            status_code: status_code,
            headers: refined_headers,
            body: body.to_string()
        };
        return response;
    }
}
