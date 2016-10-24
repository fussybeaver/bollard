extern crate hyper;

use std;
use std::error::Error;
use std::io::{BufRead, BufReader};
use hyper::client::response::Response;

use rustc_serialize::json;

pub struct StatsReader {
    buf: BufReader<Response>,
}

impl StatsReader {
   pub fn new(r: Response) -> StatsReader {
        StatsReader {
            buf: BufReader::new(r),
        }
    }

    pub fn next(&mut self) -> std::io::Result<Stats> {
        let mut line = String::new();
        match self.buf.read_line(&mut line) {
            Ok(_) => {
                match json::decode::<Stats>(&line) {
                    Ok(stats) => Ok(stats),
                    Err(e) => Err(std::io::Error::new(std::io::ErrorKind::InvalidData, e.description()))
                }
            },
            Err(e) => Err(e)
        }
    }
}

#[derive(Debug, Clone, RustcEncodable, RustcDecodable)]
pub struct Stats {
    pub read: String,
    pub network: Network,
    pub memory_stats: MemoryStats,
    pub cpu_stats: CpuStats,
    pub blkio_stats: BlkioStats
}

#[derive(Debug, Clone, RustcEncodable, RustcDecodable)]
pub struct Network {
    pub rx_dropped: u64,
    pub rx_bytes: u64,
    pub rx_errors: u64,
    pub tx_packets: u64,
    pub tx_dropped: u64,
    pub rx_packets: u64,
    pub tx_errors: u64,
    pub tx_bytes: u64
}

#[derive(Debug, Clone, RustcEncodable, RustcDecodable)]
pub struct MemoryStats {
    pub max_usage: u64,
    pub usage: u64,
    pub failcnt: u64,
    pub limit: u64,
    pub stats: MemoryStat
}

#[derive(Debug, Clone, RustcEncodable, RustcDecodable)]
pub struct MemoryStat {
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

#[derive(Debug, Clone, RustcEncodable, RustcDecodable)]
pub struct CpuStats {
    pub cpu_usage: CpuUsage,
    pub system_cpu_usage: u64,
    pub throttling_data: ThrottlingData
}

#[derive(Debug, Clone, RustcEncodable, RustcDecodable)]
pub struct CpuUsage {
    pub percpu_usage: Vec<u64>,
    pub usage_in_usermode: u64,
    pub total_usage: u64,
    pub usage_in_kernelmode: u64
}

#[derive(Debug, Clone, RustcEncodable, RustcDecodable)]
pub struct ThrottlingData {
    pub periods: u64,
    pub throttled_periods: u64,
    pub throttled_time: u64
}

#[derive(Debug, Clone, RustcEncodable, RustcDecodable)]
pub struct BlkioStats {
    pub io_service_bytes_recursive: Vec<BlkioStat>,
    pub io_serviced_recursive: Vec<BlkioStat>,
    pub io_queue_recursive: Vec<BlkioStat>,
    pub io_service_time_recursive: Vec<BlkioStat>,
    pub io_wait_time_recursive: Vec<BlkioStat>,
    pub io_merged_recursive: Vec<BlkioStat>,
    pub io_time_recursive: Vec<BlkioStat>,
    pub sectors_recursive: Vec<BlkioStat>
}

#[derive(Debug, Clone, RustcEncodable, RustcDecodable)]
pub struct BlkioStat {
    pub major: u64,
    pub minor: u64,
    pub op: String,
    pub value: u64
}
