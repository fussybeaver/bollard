use rustc_serialize::json;
use std;
use std::error::Error;
use std::collections::HashMap;

#[derive(Debug, Clone, RustcEncodable, RustcDecodable)]
#[allow(non_snake_case)]
//Labels, HostConfig
pub struct Container {
    pub Id: String,
    pub Image: String,
    pub Status: String,
    pub Command: String,
    pub Created: u64,
    pub Names: Vec<String>,
    pub Ports: Vec<Port>,
    pub SizeRw: Option<u64>, // I guess it is optional on Mac.
    pub SizeRootFs: Option<u64>,
    pub Labels: Option<HashMap<String, String>>,
    pub HostConfig: HostConfig
}

#[derive(Debug, Clone, RustcEncodable, RustcDecodable)]
#[allow(non_snake_case)]
pub struct Port {
    pub IP: Option<String>,
    pub PrivatePort: u64,
    pub PublicPort: Option<u64>,
    pub Type: String
}

#[derive(Debug, Clone, RustcEncodable, RustcDecodable)]
#[allow(non_snake_case)]
pub struct HostConfig {
    pub NetworkMode: String
}

#[derive(Debug, Clone, RustcEncodable, RustcDecodable)]
#[allow(non_snake_case)]
pub struct ContainerInfo {
    pub AppArmorProfile: String,
    pub Args: Vec<String>,
    pub Config: Config,
    pub Created: String,
    pub Driver: String,
    // ExecIDs
    // GraphDriver
    // HostConfig
    pub HostnamePath: String,
    pub HostsPath: String,
    pub Id: String,
    pub Image: String,
    pub LogPath: String,
    pub MountLabel: String,
    pub Mounts: Vec<Mount>,
    pub Name: String,
    pub NetworkSettings: NetworkSettings,
    pub Path: String,
    pub ProcessLabel: String,
    pub ResolvConfPath: String,
    pub RestartCount: u64,
    pub State: State,
}

/// This type represents a `struct{}` in the Go code.
pub type UnspecifiedObject = HashMap<String, String>;

#[derive(Debug, Clone, RustcEncodable, RustcDecodable)]
#[allow(non_snake_case)]
pub struct Config {
    pub AttachStderr: bool,
    pub AttachStdin: bool,
    pub AttachStdout: bool,
    // TODO: Verify that this is never just a `String`.
    //pub Cmd: Vec<String>,
    pub Domainname: String,
    // TODO: The source says `Option<String>` but I've seen
    // `Option<Vec<String>>` on the wire.  Ignore until we figure it out.
    //pub Entrypoint: Option<Vec<String>>,
    pub Env: Vec<String>,
    pub ExposedPorts: Option<HashMap<String, UnspecifiedObject>>,
    pub Hostname: String,
    pub Image: String,
    pub Labels: HashMap<String, String>,
    // TODO: We don't know exacly what this vec contains.
    //pub OnBuild: Option<Vec<???>>,
    pub OpenStdin: bool,
    pub StdinOnce: bool,
    pub Tty: bool,
    pub User: String,
    pub Volumes: Option<HashMap<String, UnspecifiedObject>>,
    pub WorkingDir: String,
}

#[derive(Debug, Clone, RustcEncodable, RustcDecodable)]
#[allow(non_snake_case)]
pub struct Mount {
    // Name (optional)
    // Driver (optional)
    pub Source: String,
    pub Destination: String,
    pub Mode: String,
    pub RW: bool,
    pub Propagation: String,
}

#[derive(Debug, Clone, RustcEncodable, RustcDecodable)]
#[allow(non_snake_case)]
pub struct NetworkSettings {
    pub Bridge: String,
    pub EndpointID: String,
    pub Gateway: String,
    pub GlobalIPv6Address: String,
    pub GlobalIPv6PrefixLen: u32,
    pub HairpinMode: bool,
    pub IPAddress: String,
    pub IPPrefixLen: u32,
    pub IPv6Gateway: String,
    pub LinkLocalIPv6Address: String,
    pub LinkLocalIPv6PrefixLen: u32,
    pub MacAddress: String,
    pub Networks: HashMap<String, Network>,
    pub Ports: Option<HashMap<String, Option<Vec<PortMapping>>>>,
    pub SandboxID: String,
    pub SandboxKey: String,
    // These two are null in the current output.
    //pub SecondaryIPAddresses: ,
    //pub SecondaryIPv6Addresses: ,
}

#[derive(Debug, Clone, RustcEncodable, RustcDecodable)]
#[allow(non_snake_case)]
pub struct Network {
    pub Aliases: Option<Vec<String>>,
    pub EndpointID: String,
    pub Gateway: String,
    pub GlobalIPv6Address: String,
    pub GlobalIPv6PrefixLen: u32,
    //pub IPAMConfig: ,
    pub IPAddress: String,
    pub IPPrefixLen: u32,
    pub IPv6Gateway: String,
    //pub Links:
    pub MacAddress: String,
    pub NetworkID: String,
}

#[derive(Debug, Clone, RustcEncodable, RustcDecodable)]
#[allow(non_snake_case)]
pub struct PortMapping {
    pub HostIp: String,
    pub HostPort: String,
}

#[derive(Debug, Clone, RustcEncodable, RustcDecodable)]
#[allow(non_snake_case)]
pub struct State {
    pub Status: String,
    pub Running: bool,
    pub Paused: bool,
    pub Restarting: bool,
    pub OOMKilled: bool,
    pub Dead: bool,
    // I don't know whether PIDs can be negative here.  They're normally
    // positive, but sometimes negative PIDs are used in certain APIs.
    pub Pid: i64,
    pub ExitCode: i64,
    pub Error: String,
    pub StartedAt: String,
    pub FinishedAt: String
}

impl std::fmt::Display for Container {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::result::Result<(), std::fmt::Error> {
        write!(f, "{}", self.Id)
    }
}

impl std::fmt::Display for ContainerInfo {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::result::Result<(), std::fmt::Error> {
        write!(f, "{}", self.Id)
    }
}
