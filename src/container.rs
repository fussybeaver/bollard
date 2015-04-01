#[derive(RustcEncodable, RustcDecodable)]
#[allow(non_snake_case)]
pub struct Container {
    pub Id: String,
    pub Image: String,
    pub Status: String,
    pub Command: String,
    pub Created: f64,
    pub Names: Vec<String>,
    pub Ports: Vec<Port>
}

#[derive(RustcEncodable, RustcDecodable)]
#[allow(non_snake_case)]
pub struct Port {
    pub IP: Option<String>,
    pub PrivatePort: f64,
    pub PublicPort: f64,
    pub Type: String
}

impl Clone for Port {
    fn clone(&self) -> Port {
        let port = Port {
            IP: self.IP.clone(),
            PrivatePort: self.PrivatePort.clone(),
            PublicPort: self.PublicPort.clone(),
            Type: self.Type.clone()
        };
        return port;
    }
    
    /*fn clone_from(&mut self, source: &Self) {
        self.IP = source.IP.clone();
        self.PrivatePort = source.PrivatePort.clone();
        self.PublicPort = source.PublicPort.clone();
        self.Type = source.Type.clone();
    }*/
}
