use std::fs;
use crate::model::MemoryInfo;

pub struct MemoryCollector;

impl MemoryCollector {
    pub fn new() -> Self {
        Self
    }

    pub fn collect(&self) -> MemoryInfo {
        let mut info = MemoryInfo::default();
        let meminfo = fs::read_to_string("/proc/meminfo").unwrap_or_default();

        for line in meminfo.lines() {
            let parts: Vec<&str> = line.split_whitespace().collect();
            if parts.len() < 2 {
                continue;
            }
            let val: u64 = parts[1].parse().unwrap_or(0) * 1024; // kB to bytes
            match parts[0] {
                "MemTotal:" => info.total = val,
                "MemAvailable:" => info.available = val,
                "Cached:" => info.cached = val,
                "SwapTotal:" => info.swap_total = val,
                "SwapFree:" => info.swap_used = info.swap_total.saturating_sub(val),
                _ => {}
            }
        }
        info.used = info.total.saturating_sub(info.available);
        // Fix swap: SwapFree line sets swap_used incorrectly if SwapTotal hasn't been read yet
        // Re-parse to be safe:
        let mut swap_free = 0u64;
        for line in meminfo.lines() {
            let parts: Vec<&str> = line.split_whitespace().collect();
            if parts.len() >= 2 && parts[0] == "SwapFree:" {
                swap_free = parts[1].parse().unwrap_or(0) * 1024;
            }
        }
        info.swap_used = info.swap_total.saturating_sub(swap_free);

        info
    }
}
