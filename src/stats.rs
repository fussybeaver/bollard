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
    pub rx_dropped: i64,
    pub rx_bytes: i64,
    pub rx_errors: i64,
    pub tx_packets: i64,
    pub tx_dropped: i64,
    pub rx_packets: i64,
    pub tx_errors: i64,
    pub tx_bytes: i64
}

#[allow(non_camel_case_types)]
#[derive(RustcEncodable, RustcDecodable)]
pub struct memory_stats {
    pub max_usage: i64,
    pub usage: i64,
    pub failcnt: i64,
    pub limit: i64,
    pub stats: stats
}

#[allow(non_camel_case_types)]
#[derive(RustcEncodable, RustcDecodable)]
pub struct stats {
    pub total_pgmajfault: i64,
    pub cache: i64,
    pub mapped_file: i64,
    pub total_inactive_file: i64,
    pub pgpgout: i64,
    pub rss: i64,
    pub total_mapped_file: i64,
    pub writeback: i64,
    pub unevictable: i64,
    pub pgpgin: i64,
    pub total_unevictable: i64,
    pub pgmajfault: i64,
    pub total_rss: i64,
    pub total_rss_huge: i64,
    pub total_writeback: i64,
    pub total_inactive_anon: i64,
    pub rss_huge: i64,
    pub hierarchical_memory_limit: f64,
    pub hierarchical_memsw_limit: f64,
    pub total_pgfault: i64,
    pub total_active_file: i64,
    pub active_anon: i64,
    pub total_active_anon: i64,
    pub total_pgpgout: i64,
    pub total_cache: i64,
    pub inactive_anon: i64,
    pub active_file: i64,
    pub pgfault: i64,
    pub inactive_file: i64,
    pub total_pgpgin: i64,
    pub swap: i64,
    pub total_swap: i64
}

#[allow(non_camel_case_types)]
#[derive(RustcEncodable, RustcDecodable)]
pub struct cpu_stats {
    pub cpu_usage: cpu_usage,
    pub system_cpu_usage: i64,
    pub throttling_data: throttling_data
}

#[allow(non_camel_case_types)]
#[derive(RustcEncodable, RustcDecodable)]
pub struct cpu_usage {
    pub percpu_usage: Vec<i64>,
    pub usage_in_usermode: i64,
    pub total_usage: i64,
    pub usage_in_kernelmode: i64
}

#[allow(non_camel_case_types)]
#[derive(RustcEncodable, RustcDecodable)]
pub struct throttling_data {
    pub periods: i64,
    pub throttled_periods: i64,
    pub throttled_time: i64
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
    pub major: i64,
    pub minor: i64,
    pub op: String,
    pub value: i64
}
