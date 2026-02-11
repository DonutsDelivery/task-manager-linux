use std::fs;

pub struct CpuCollector {
    prev_total: Vec<u64>,
    prev_idle: Vec<u64>,
    pub core_count: usize,
    pub model_name: String,
}

impl CpuCollector {
    pub fn new() -> Self {
        let core_count = num_cores();
        let model_name = cpu_model_name();
        Self {
            prev_total: vec![0; core_count + 1],
            prev_idle: vec![0; core_count + 1],
            core_count,
            model_name,
        }
    }

    pub fn collect(&mut self) -> (f64, Vec<f64>, f64) {
        let stat = fs::read_to_string("/proc/stat").unwrap_or_default();
        let mut total_percent = 0.0;
        let mut per_core = Vec::new();
        let mut freq = 0.0;

        for (i, line) in stat.lines().enumerate() {
            if line.starts_with("cpu") {
                let parts: Vec<&str> = line.split_whitespace().collect();
                if parts.len() < 8 {
                    continue;
                }
                // cpu/cpu0/cpu1...
                let is_total = parts[0] == "cpu";
                let idx = if is_total { 0 } else {
                    match parts[0].strip_prefix("cpu") {
                        Some(n) => n.parse::<usize>().unwrap_or(0) + 1,
                        None => continue,
                    }
                };

                if idx >= self.prev_total.len() {
                    self.prev_total.resize(idx + 1, 0);
                    self.prev_idle.resize(idx + 1, 0);
                }

                let user: u64 = parts[1].parse().unwrap_or(0);
                let nice: u64 = parts[2].parse().unwrap_or(0);
                let system: u64 = parts[3].parse().unwrap_or(0);
                let idle: u64 = parts[4].parse().unwrap_or(0);
                let iowait: u64 = parts[5].parse().unwrap_or(0);
                let irq: u64 = parts[6].parse().unwrap_or(0);
                let softirq: u64 = parts[7].parse().unwrap_or(0);

                let total = user + nice + system + idle + iowait + irq + softirq;
                let idle_total = idle + iowait;

                let dtotal = total.saturating_sub(self.prev_total[idx]);
                let didle = idle_total.saturating_sub(self.prev_idle[idx]);

                let percent = if dtotal > 0 {
                    ((dtotal - didle) as f64 / dtotal as f64) * 100.0
                } else {
                    0.0
                };

                self.prev_total[idx] = total;
                self.prev_idle[idx] = idle_total;

                if is_total {
                    total_percent = percent;
                } else {
                    per_core.push(percent);
                }
            }
            if i > self.core_count + 2 {
                break;
            }
        }

        // Read frequency from scaling_cur_freq
        if let Ok(f) = fs::read_to_string("/sys/devices/system/cpu/cpu0/cpufreq/scaling_cur_freq") {
            freq = f.trim().parse::<f64>().unwrap_or(0.0) / 1000.0; // kHz -> MHz
        }

        (total_percent, per_core, freq)
    }
}

fn num_cores() -> usize {
    fs::read_to_string("/proc/stat")
        .unwrap_or_default()
        .lines()
        .filter(|l| l.starts_with("cpu") && !l.starts_with("cpu "))
        .count()
}

fn cpu_model_name() -> String {
    fs::read_to_string("/proc/cpuinfo")
        .unwrap_or_default()
        .lines()
        .find(|l| l.starts_with("model name"))
        .and_then(|l| l.split(':').nth(1))
        .map(|s| s.trim().to_string())
        .unwrap_or_else(|| "Unknown CPU".to_string())
}

pub fn uptime_secs() -> u64 {
    fs::read_to_string("/proc/uptime")
        .unwrap_or_default()
        .split_whitespace()
        .next()
        .and_then(|s| s.parse::<f64>().ok())
        .map(|f| f as u64)
        .unwrap_or(0)
}
