#[derive(RustcEncodable, RustcDecodable)]
#[allow(non_snake_case)]
pub struct Container {
    pub Id: String,
    pub Image: String,
    pub Status: String,
    pub Command: String,
    pub Created: u64,
    pub Names: Vec<String>,
    pub Ports: Vec<Port>,
    pub SizeRw: u64,
    pub SizeRootFs: u64
}

#[derive(RustcEncodable, RustcDecodable)]
#[allow(non_snake_case)]
pub struct Port {
    pub IP: Option<String>,
    pub PrivatePort: u64,
    pub PublicPort: Option<u64>,
    pub Type: String
}

impl Clone for Container {
    fn clone(&self) -> Self {
        let container = Container {
            Id: self.Id.clone(),
            Image: self.Image.clone(),
            Status: self.Status.clone(),
            Command: self.Command.clone(),
            Created: self.Created.clone(),
            Names: self.Names.clone(),
            Ports: self.Ports.clone(),
            SizeRw: self.SizeRw,
            SizeRootFs: self.SizeRootFs
        };
        return container;
    }
}

impl Clone for Port {
    fn clone(&self) -> Self {
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
