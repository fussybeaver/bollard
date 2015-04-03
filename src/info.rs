#[derive(RustcEncodable, RustcDecodable)]
#[allow(non_snake_case)]
pub struct Info {
    pub Containers: i64,
    pub Images: i64,
    pub Driver: String,
    pub DriverStatus: Vec<(String, String)>,
    pub ExecutionDriver: String,
    pub KernelVersion: String,
    pub NCPU: i64,
    pub MemTotal: i64,
    pub Name: String,
    pub ID: String,
    pub Debug: i64, // bool
    pub NFd: i64,
    pub NGoroutines: i64,
    pub NEventsListener: i64,
    pub InitPath: String,
    pub InitSha1: String,
    pub IndexServerAddress: String,
    pub MemoryLimit: i64, // bool
    pub SwapLimit: i64, // bool
    pub IPv4Forwarding: i64, // bool
    pub Labels: Option<Vec<String>>,
    pub DockerRootDir: String,
    pub OperatingSystem: String,
}
