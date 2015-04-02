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
    pub total_pgmajfault: f64,
    pub cache: f64,
    pub mapped_file: f64,
    pub total_inactive_file: f64,
    pub pgpgout: f64,
    pub rss: f64,
    pub total_mapped_file: f64,
    pub writeback: f64,
    pub unevictable: f64,
    pub pgpgin: f64,
    pub total_unevictable: f64,
    pub pgmajfault: f64,
    pub total_rss: f64,
    pub total_rss_huge: f64,
    pub total_writeback: f64,
    pub total_inactive_anon: f64,
    pub rss_huge: f64,
    pub hierarchical_memory_limit: f64,
    pub hierarchical_memsw_limit: f64,
    pub total_pgfault: f64,
    pub total_active_file: f64,
    pub active_anon: f64,
    pub total_active_anon: f64,
    pub total_pgpgout: f64,
    pub total_cache: f64,
    pub inactive_anon: f64,
    pub active_file: f64,
    pub pgfault: f64,
    pub inactive_file: f64,
    pub total_pgpgin: f64,
    pub swap: f64,
    pub total_swap: f64
}

#[allow(non_camel_case_types)]
#[derive(RustcEncodable, RustcDecodable)]
pub struct cpu_stats {
    pub cpu_usage: cpu_usage,
    pub system_cpu_usage: f64,
    pub throttling_data: throttling_data
}

#[allow(non_camel_case_types)]
#[derive(RustcEncodable, RustcDecodable)]
pub struct cpu_usage {
    pub percpu_usage: Vec<f64>,
    pub usage_in_usermode: f64,
    pub total_usage: f64,
    pub usage_in_kernelmode: f64
}

#[allow(non_camel_case_types)]
#[derive(RustcEncodable, RustcDecodable)]
pub struct throttling_data {
    pub periods: f64,
    pub throttled_periods: f64,
    pub throttled_time: f64
}

#[allow(non_camel_case_types)]
#[derive(RustcEncodable, RustcDecodable)]
pub struct blkio_stats {
    pub io_service_bytes_recursive: Vec<blkio_stat>,
    pub io_serviced_recursive: Vec<blkio_stat>,
    pub io_queue_recursive: Vec<blkio_stat>,
    pub io_service_time_recursive: Vec<blkio_stat>,
    pub io_wait_time_recursive: Vec<blkio_stat>,
    pub io_merged_recursive: Vec<blkio_stat>,
    pub io_time_recursive: Vec<blkio_stat>,
    pub sectors_recursive: Vec<blkio_stat>
}

#[allow(non_camel_case_types)]
#[derive(RustcEncodable, RustcDecodable)]
pub struct blkio_stat {
    pub major: f64,
    pub minor: f64,
    pub op: String,
    pub value: f64
}
