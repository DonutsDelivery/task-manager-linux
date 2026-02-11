use std::fs;
use crate::model::{DiskInfo, DiskDevice};

pub struct DiskCollector {
    prev_stats: Vec<(String, u64, u64)>,
    prev_time: std::time::Instant,
}

impl DiskCollector {
    pub fn new() -> Self {
        Self {
            prev_stats: Vec::new(),
            prev_time: std::time::Instant::now(),
        }
    }

    pub fn collect(&mut self) -> DiskInfo {
        let elapsed = self.prev_time.elapsed().as_secs_f64().max(0.001);
        let diskstats = fs::read_to_string("/proc/diskstats").unwrap_or_default();
        let mut devices = Vec::new();
        let mut current_stats = Vec::new();

        for line in diskstats.lines() {
            let parts: Vec<&str> = line.split_whitespace().collect();
            if parts.len() < 14 {
                continue;
            }
            let name = parts[2].to_string();
            // Only include real block devices (sd*, nvme*, vd*), skip partitions
            let is_disk = (name.starts_with("sd") && name.len() == 3)
                || (name.starts_with("nvme") && name.contains("n") && !name.contains("p"))
                || (name.starts_with("vd") && name.len() == 3);
            if !is_disk {
                continue;
            }

            let read_sectors: u64 = parts[5].parse().unwrap_or(0);
            let write_sectors: u64 = parts[9].parse().unwrap_or(0);
            let read_bytes = read_sectors * 512;
            let write_bytes = write_sectors * 512;

            let prev = self.prev_stats.iter().find(|(n, _, _)| n == &name);
            let (read_rate, write_rate) = if let Some((_, prev_r, prev_w)) = prev {
                (
                    (read_bytes.saturating_sub(*prev_r)) as f64 / elapsed,
                    (write_bytes.saturating_sub(*prev_w)) as f64 / elapsed,
                )
            } else {
                (0.0, 0.0)
            };

            current_stats.push((name.clone(), read_bytes, write_bytes));
            devices.push(DiskDevice {
                name,
                read_bytes_sec: read_rate,
                write_bytes_sec: write_rate,
                total_read: read_bytes,
                total_write: write_bytes,
            });
        }

        self.prev_stats = current_stats;
        self.prev_time = std::time::Instant::now();

        DiskInfo { devices }
    }
}
