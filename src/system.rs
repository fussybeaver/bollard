#[derive(Debug, Clone, RustcEncodable, RustcDecodable)]
#[allow(non_snake_case)]
pub struct SystemInfo {
    pub Containers: u64,
    pub Images: u64,
    pub Driver: String,
    pub DriverStatus: Vec<(String, String)>,
    pub ExecutionDriver: String,
    pub KernelVersion: String,
    pub NCPU: u64,
    pub MemTotal: u64,
    pub Name: String,
    pub ID: String,
    pub Debug: u64, // bool
    pub NFd: u64,
    pub NGoroutines: u64,
    pub NEventsListener: u64,
    pub InitPath: String,
    pub InitSha1: String,
    pub IndexServerAddress: String,
    pub MemoryLimit: u64, // bool
    pub SwapLimit: u64, // bool
    pub IPv4Forwarding: u64, // bool
    pub Labels: Option<Vec<String>>,
    pub DockerRootDir: String,
    pub OperatingSystem: String,
}
