#[derive(RustcEncodable, RustcDecodable)]
pub struct Stats {
    pub read: String,
    pub network: network,
    pub memory_stats: memory_stats,
    pub cpu_stats: cpu_stats,
    pub blkio_stats: blkio_stats
}

#[allow(non_camel_case_types)]
#[derive(RustcEncodable, RustcDecodable)]
pub struct network {
    pub rx_dropped: f64,
    pub rx_bytes: f64,
    pub rx_errors: f64,
    pub tx_packets: f64,
    pub tx_dropped: f64,
    pub rx_packets: f64,
    pub tx_errors: f64,
    pub tx_bytes: f64
}

#[allow(non_camel_case_types)]
#[derive(RustcEncodable, RustcDecodable)]
pub struct memory_stats {    
    pub max_usage: f64,
    pub usage: f64,
    pub failcnt: f64,
    pub limit: f64,
    pub stats: stats
}

#[allow(non_camel_case_types)]
#[derive(RustcEncodable, RustcDecodable)]
pub struct stats {
    total_pgmajfault: f64,
    cache: f64,
    mapped_file: f64,
    total_inactive_file: f64,
    pgpgout: f64,
    rss: f64,
    total_mapped_file: f64,
    writeback: f64,
    unevictable: f64,
    pgpgin: f64,
    total_unevictable: f64,
    pgmajfault: f64,
    total_rss: f64,
    total_rss_huge: f64,
    total_writeback: f64,
    total_inactive_anon: f64,
    rss_huge: f64,
    hierarchical_memory_limit: f64,
    total_pgfault: f64,
    total_active_file: f64,
    active_anon: f64,
    total_active_anon: f64,
    total_pgpgout: f64,
    total_cache: f64,
    inactive_anon: f64,
    active_file: f64,
    pgfault: f64,
    inactive_file: f64,
    total_pgpgin: f64
}

#[allow(non_camel_case_types)]
#[derive(RustcEncodable, RustcDecodable)]
pub struct cpu_stats {
    cpu_usage: cpu_usage,
    system_cpu_usage: f64,
    throttling_data: throttling_data
}

#[allow(non_camel_case_types)]
#[derive(RustcEncodable, RustcDecodable)]
pub struct cpu_usage {
    percpu_usage: Vec<f64>,
    usage_in_usermode: f64,
    total_usage: f64,
    usage_in_kernelmode: f64
}

#[allow(non_camel_case_types)]
#[derive(RustcEncodable, RustcDecodable)]
pub struct throttling_data {
    periods: f64,
    throttled_periods: f64,
    throttled_time: f64
}

#[allow(non_camel_case_types)]
#[derive(RustcEncodable, RustcDecodable)]
pub struct blkio_stats {
    io_service_bytes_recursive: Vec<f64>,
    io_serviced_recursive: Vec<f64>,
    io_queue_recursive: Vec<f64>,
    io_service_time_recursive: Vec<f64>,
    io_wait_time_recursive: Vec<f64>,
    io_merged_recursive: Vec<f64>,
    io_time_recursive: Vec<f64>,
    sectors_recursive: Vec<f64>
}
