use std::fmt::Error;
use std::fmt::{Display, Formatter};
use std::collections::HashMap;

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

#[derive(RustcEncodable, RustcDecodable)]
#[allow(non_snake_case)]
pub struct ContainerInfo {
    pub AppArmorProfile: String,
    pub Args: Vec<String>,
    // Config
    pub Created: String,
    pub Driver: String,
    pub ExecDriver: String,
    // ExecIDs
    // HostConfig
    pub HostnamePath: String,
    pub HostsPath: String,
    pub LogPath: String,
    pub Id: String,
    pub Image: String,
    pub MountLabel: String,
    pub Name: String,
    // NetworkSettings
    pub Path: String,
    pub ProcessLabel: String,
    pub ResolvConfPath: String,
    pub RestartCount: u64,
    // State
    pub Volumes: HashMap<String, String>,
    pub VolumesRW: HashMap<String, bool>
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

impl Display for Container {
    fn fmt(&self, f: &mut Formatter) -> Result<(), Error> {
        write!(f, "{}", self.Id)
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

impl Clone for ContainerInfo {
    fn clone(&self) -> Self {
        let container_info = ContainerInfo {
            AppArmorProfile: self.AppArmorProfile.clone(),
            Args: self.Args.clone(),
            // Config
            Created: self.Created.clone(),
            Driver: self.Driver.clone(),
            ExecDriver: self.ExecDriver.clone(),
            // ExecIDs
            // HostConfig
            HostnamePath: self.HostnamePath.clone(),
            HostsPath: self.HostsPath.clone(),
            LogPath: self.LogPath.clone(),
            Id: self.Id.clone(),
            Image: self.Image.clone(),
            MountLabel: self.MountLabel.clone(),
            Name: self.Name.clone(),
            // NetworkSettings
            Path: self.Path.clone(),
            ProcessLabel: self.ProcessLabel.clone(),
            ResolvConfPath: self.ResolvConfPath.clone(),
            RestartCount: self.RestartCount,
            // State
            Volumes: self.Volumes.clone(),
            VolumesRW: self.VolumesRW.clone()
        };
        return container_info;
    }
}

impl Display for ContainerInfo {
    fn fmt(&self, f: &mut Formatter) -> Result<(), Error> {
        write!(f, "{}", self.Id)
    }
}
