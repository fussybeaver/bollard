extern crate hyper;

use std;
use std::iter;
use std::io::{BufRead, BufReader};
use hyper::client::response::Response;

use serde_json;

use errors::*;

pub struct StatsReader {
    buf: BufReader<Response>,
}

impl StatsReader {
   pub fn new(r: Response) -> StatsReader {
        StatsReader {
            buf: BufReader::new(r),
        }
    }
}

impl iter::Iterator for StatsReader {
    type Item = Result<Stats>;

    fn next(&mut self) -> Option<Result<Stats>> {
        let mut line = String::new();
        if let Err(err) = self.buf.read_line(&mut line) {
            return Some(Err(err.into()));
        }
        Some(serde_json::from_str::<Stats>(&line)
            .chain_err(|| ErrorKind::ParseError("Stats", line)))
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Stats {
    pub read: String,
    pub networks: Network,
    pub memory_stats: MemoryStats,
    pub cpu_stats: CpuStats,
    pub blkio_stats: BlkioStats
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Network {
    pub rx_dropped: Option<u64>,
    pub rx_bytes: Option<u64>,
    pub rx_errors: Option<u64>,
    pub tx_packets: Option<u64>,
    pub tx_dropped: Option<u64>,
    pub rx_packets: Option<u64>,
    pub tx_errors: Option<u64>,
    pub tx_bytes: Option<u64>
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryStats {
    pub max_usage: u64,
    pub usage: u64,
    pub failcnt: Option<u64>,
    pub limit: u64,
    pub stats: MemoryStat
}

#[derive(Debug, Clone, Serialize, Deserialize)]
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
    pub total_pgpgin: u64
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CpuStats {
    pub cpu_usage: CpuUsage,
    pub system_cpu_usage: u64,
    pub throttling_data: ThrottlingData
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CpuUsage {
    pub percpu_usage: Vec<u64>,
    pub usage_in_usermode: u64,
    pub total_usage: u64,
    pub usage_in_kernelmode: u64
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ThrottlingData {
    pub periods: u64,
    pub throttled_periods: u64,
    pub throttled_time: u64
}

#[derive(Debug, Clone, Serialize, Deserialize)]
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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BlkioStat {
    pub major: u64,
    pub minor: u64,
    pub op: String,
    pub value: u64
}
