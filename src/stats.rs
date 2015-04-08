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
    pub rx_dropped: u64,
    pub rx_bytes: u64,
    pub rx_errors: u64,
    pub tx_packets: u64,
    pub tx_dropped: u64,
    pub rx_packets: u64,
    pub tx_errors: u64,
    pub tx_bytes: u64
}

#[allow(non_camel_case_types)]
#[derive(RustcEncodable, RustcDecodable)]
pub struct memory_stats {
    pub max_usage: u64,
    pub usage: u64,
    pub failcnt: u64,
    pub limit: u64,
    pub stats: stats
}

#[allow(non_camel_case_types)]
#[derive(RustcEncodable, RustcDecodable)]
pub struct stats {
    pub total_pgmajfault: u64,
    pub cache: u64,
    pub mapped_file: u64,
    pub total_inactive_file: u64,
    pub pgpgout: u64,
    pub rss: u64,
    pub total_mapped_file: u64,
    pub writeback: u64,
    pub unevictable: u64,
    pub pgpgin: u64,
    pub total_unevictable: u64,
    pub pgmajfault: u64,
    pub total_rss: u64,
    pub total_rss_huge: u64,
    pub total_writeback: u64,
    pub total_inactive_anon: u64,
    pub rss_huge: u64,
    pub hierarchical_memory_limit: u64,
    pub hierarchical_memsw_limit: u64,
    pub total_pgfault: u64,
    pub total_active_file: u64,
    pub active_anon: u64,
    pub total_active_anon: u64,
    pub total_pgpgout: u64,
    pub total_cache: u64,
    pub inactive_anon: u64,
    pub active_file: u64,
    pub pgfault: u64,
    pub inactive_file: u64,
    pub total_pgpgin: u64,
    pub swap: u64,
    pub total_swap: u64
}

#[allow(non_camel_case_types)]
#[derive(RustcEncodable, RustcDecodable)]
pub struct cpu_stats {
    pub cpu_usage: cpu_usage,
    pub system_cpu_usage: u64,
    pub throttling_data: throttling_data
}

#[allow(non_camel_case_types)]
#[derive(RustcEncodable, RustcDecodable)]
pub struct cpu_usage {
    pub percpu_usage: Vec<u64>,
    pub usage_in_usermode: u64,
    pub total_usage: u64,
    pub usage_in_kernelmode: u64
}

#[allow(non_camel_case_types)]
#[derive(RustcEncodable, RustcDecodable)]
pub struct throttling_data {
    pub periods: u64,
    pub throttled_periods: u64,
    pub throttled_time: u64
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
    pub major: u64,
    pub minor: u64,
    pub op: String,
    pub value: u64
}
