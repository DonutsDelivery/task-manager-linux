use std::fs;
use crate::model::{NetworkInfo, NetworkInterface};

pub struct NetworkCollector {
    prev_stats: Vec<(String, u64, u64)>,
    prev_time: std::time::Instant,
}

impl NetworkCollector {
    pub fn new() -> Self {
        Self {
            prev_stats: Vec::new(),
            prev_time: std::time::Instant::now(),
        }
    }

    pub fn collect(&mut self) -> NetworkInfo {
        let elapsed = self.prev_time.elapsed().as_secs_f64().max(0.001);
        let netdev = fs::read_to_string("/proc/net/dev").unwrap_or_default();
        let mut interfaces = Vec::new();
        let mut current_stats = Vec::new();

        for line in netdev.lines().skip(2) {
            let line = line.trim();
            let parts: Vec<&str> = line.split_whitespace().collect();
            if parts.len() < 10 {
                continue;
            }
            let name = parts[0].trim_end_matches(':').to_string();
            if name == "lo" {
                continue;
            }

            let rx_bytes: u64 = parts[1].parse().unwrap_or(0);
            let tx_bytes: u64 = parts[9].parse().unwrap_or(0);

            let prev = self.prev_stats.iter().find(|(n, _, _)| n == &name);
            let (rx_rate, tx_rate) = if let Some((_, prev_rx, prev_tx)) = prev {
                (
                    (rx_bytes.saturating_sub(*prev_rx)) as f64 / elapsed,
                    (tx_bytes.saturating_sub(*prev_tx)) as f64 / elapsed,
                )
            } else {
                (0.0, 0.0)
            };

            current_stats.push((name.clone(), rx_bytes, tx_bytes));
            interfaces.push(NetworkInterface {
                name,
                rx_bytes_sec: rx_rate,
                tx_bytes_sec: tx_rate,
                total_rx: rx_bytes,
                total_tx: tx_bytes,
            });
        }

        self.prev_stats = current_stats;
        self.prev_time = std::time::Instant::now();

        NetworkInfo { interfaces }
    }
}
