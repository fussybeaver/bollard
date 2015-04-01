#[derive(RustcEncodable, RustcDecodable)]
#[allow(non_snake_case)]
pub struct Info {
    pub Containers: f64,
    pub Images: f64,
    pub Driver: String,
    pub DriverStatus: Vec<(String, String)>,
    pub ExecutionDriver: String,
    pub KernelVersion: String,
    pub NCPU: f64,
    pub MemTotal: f64,
    pub Name: String,
    pub ID: String,
    pub Debug: f64, // bool
    pub NFd: f64,
    pub NGoroutines: f64,
    pub NEventsListener: f64,
    pub InitPath: String,
    pub InitSha1: String,
    pub IndexServerAddress: String,
    pub MemoryLimit: f64, // bool
    pub SwapLimit: f64, // bool
    pub IPv4Forwarding: f64, // bool
    pub Labels: Option<Vec<String>>,
    pub DockerRootDir: String,
    pub OperatingSystem: String,
}
