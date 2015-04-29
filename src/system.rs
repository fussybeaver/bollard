#[derive(RustcEncodable, RustcDecodable)]
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

impl Clone for SystemInfo {
    fn clone(&self) -> Self {
        let system_info = SystemInfo {
            Containers: self.Containers,
            Images: self.Images,
            Driver: self.Driver.clone(),
            DriverStatus: self.DriverStatus.clone(),
            ExecutionDriver: self.ExecutionDriver.clone(),
            KernelVersion: self.KernelVersion.clone(),
            NCPU: self.NCPU,
            MemTotal: self.MemTotal,
            Name: self.Name.clone(),
            ID: self.ID.clone(),
            Debug: self.Debug,
            NFd: self.NFd,
            NGoroutines: self.NGoroutines,
            NEventsListener: self.NEventsListener,
            InitPath: self.InitPath.clone(),
            InitSha1: self.InitSha1.clone(),
            IndexServerAddress: self.IndexServerAddress.clone(),
            MemoryLimit: self.MemoryLimit,
            SwapLimit: self.SwapLimit,
            IPv4Forwarding: self.IPv4Forwarding,
            Labels: self.Labels.clone(),
            DockerRootDir: self.DockerRootDir.clone(),
            OperatingSystem: self.OperatingSystem.clone()
        };
        return system_info;
    }
}
