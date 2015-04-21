use std::io::{Result, Error, ErrorKind};

pub struct Http;

impl Http {
    pub fn new() -> Http {
        return Http;
    }
    
    pub fn get_response(&self, raw: &str) -> Result<String> {
        let http_response: Vec<&str> = raw.split("\r\n\r\n").collect();

        if http_response.len() < 2 {
            let err = Error::new(ErrorKind::InvalidInput,
                                 "Docker returns an invalid response.");
            return Err(err);
        }
        //let http_header = http_response[0];
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

        return Ok(content_body);
    }
}
