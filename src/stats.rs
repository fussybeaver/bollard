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

#[derive(RustcEncodable, RustcDecodable)]
pub struct Stats {
    pub read: String,
    pub network: Network,
    pub memory_stats: MemoryStats,
    pub cpu_stats: CpuStats,
    pub blkio_stats: BlkioStats
}

#[derive(RustcEncodable, RustcDecodable)]
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

#[derive(RustcEncodable, RustcDecodable)]
pub struct MemoryStats {
    pub max_usage: u64,
    pub usage: u64,
    pub failcnt: u64,
    pub limit: u64,
    pub stats: MemoryStat
}

#[derive(RustcEncodable, RustcDecodable)]
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

#[derive(RustcEncodable, RustcDecodable)]
pub struct CpuStats {
    pub cpu_usage: CpuUsage,
    pub system_cpu_usage: u64,
    pub throttling_data: ThrottlingData
}

#[derive(RustcEncodable, RustcDecodable)]
pub struct CpuUsage {
    pub percpu_usage: Vec<u64>,
    pub usage_in_usermode: u64,
    pub total_usage: u64,
    pub usage_in_kernelmode: u64
}

#[derive(RustcEncodable, RustcDecodable)]
pub struct ThrottlingData {
    pub periods: u64,
    pub throttled_periods: u64,
    pub throttled_time: u64
}

#[derive(RustcEncodable, RustcDecodable)]
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

#[derive(RustcEncodable, RustcDecodable)]
pub struct BlkioStat {
    pub major: u64,
    pub minor: u64,
    pub op: String,
    pub value: u64
}

impl Clone for Stats {
    fn clone(&self) -> Stats {
        let stats = Stats {
            read: self.read.clone(),
            network: self.network.clone(),
            memory_stats: self.memory_stats.clone(),
            cpu_stats: self.cpu_stats.clone(),
            blkio_stats: self.blkio_stats.clone()
        };
        return stats;
    }
}

impl Clone for Network {
    fn clone(&self) -> Self {
        let network = Network {
            rx_dropped: self.rx_dropped,
            rx_bytes: self.rx_bytes,
            rx_errors: self.rx_errors,
            tx_packets: self.tx_packets,
            tx_dropped: self.tx_dropped,
            rx_packets: self.rx_packets,
            tx_errors: self.tx_errors,
            tx_bytes: self.tx_bytes
        };
        return network;
    }
}

impl Clone for MemoryStats {
    fn clone(&self) -> Self {
        let memory_stats = MemoryStats {
            max_usage: self.max_usage,
            usage: self.usage,
            failcnt: self.failcnt,
            limit: self.limit,
            stats: self.stats.clone()
        };
        return memory_stats;
    }
}

impl Clone for MemoryStat {
    fn clone(&self) -> Self {
        let memory_stat = MemoryStat {
            total_pgmajfault: self.total_pgmajfault,
            cache: self.cache,
            mapped_file: self.mapped_file,
            total_inactive_file: self.total_inactive_file,
            pgpgout: self.pgpgout,
            rss: self.rss,
            total_mapped_file: self.total_mapped_file,
            writeback: self.writeback,
            unevictable: self.unevictable,
            pgpgin: self.pgpgin,
            total_unevictable: self.total_unevictable,
            pgmajfault: self.pgmajfault,
            total_rss: self.total_rss,
            total_rss_huge: self.total_rss_huge,
            total_writeback: self.total_writeback,
            total_inactive_anon: self.total_inactive_anon,
            rss_huge: self.rss_huge,
            hierarchical_memory_limit: self.hierarchical_memory_limit,
            hierarchical_memsw_limit: self.hierarchical_memsw_limit,
            total_pgfault: self.total_pgfault,
            total_active_file: self.total_active_file,
            active_anon: self.active_anon,
            total_active_anon: self.total_active_anon,
            total_pgpgout: self.total_pgpgout,
            total_cache: self.total_cache,
            inactive_anon: self.inactive_anon,
            active_file: self.active_file,
            pgfault: self.pgfault,
            inactive_file: self.inactive_file,
            total_pgpgin: self.total_pgpgin,
            swap: self.swap,
            total_swap: self.total_swap
        };
        return memory_stat;
    }
}

impl Clone for CpuStats {
    fn clone(&self) -> Self {
        let cpu_stats = CpuStats {
            cpu_usage: self.cpu_usage.clone(),
            system_cpu_usage: self.system_cpu_usage,
            throttling_data: self.throttling_data.clone()
        };
        return cpu_stats;
    }
}

impl Clone for CpuUsage {
    fn clone(&self) -> Self {
        let cpu_usage = CpuUsage {
            percpu_usage: self.percpu_usage.clone(),
            usage_in_usermode: self.usage_in_usermode,
            total_usage: self.total_usage,
            usage_in_kernelmode: self.usage_in_kernelmode
        };
        return cpu_usage;
    }
}

impl Clone for ThrottlingData {
    fn clone(&self) -> Self {
        let throttling_data = ThrottlingData {
            periods: self.periods,
            throttled_periods: self.throttled_periods,
            throttled_time: self.throttled_time
        };
        return throttling_data;
    }
}

impl Clone for BlkioStats {
    fn clone(&self) -> Self {
        let blkio_stats = BlkioStats {
            io_service_bytes_recursive: self.io_service_bytes_recursive.clone(),
            io_serviced_recursive: self.io_serviced_recursive.clone(),
            io_queue_recursive: self.io_queue_recursive.clone(),
            io_service_time_recursive: self.io_service_time_recursive.clone(),
            io_wait_time_recursive: self.io_wait_time_recursive.clone(),
            io_merged_recursive: self.io_merged_recursive.clone(),
            io_time_recursive: self.io_time_recursive.clone(),
            sectors_recursive: self.sectors_recursive.clone()
        };
        return blkio_stats;
    }
}

impl Clone for BlkioStat {
    fn clone(&self) -> Self {
        let blkio_stat = BlkioStat {
            major: self.major,
            minor: self.minor,
            op: self.op.clone(),
            value: self.value
        };
        return blkio_stat;
    }
}
