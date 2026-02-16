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

    pub fn collect(&mut self) -> (f64, Vec<f64>, f64, f64, Vec<f64>, Vec<(f64, String)>) {
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

        let temperature = read_cpu_temperature();
        let per_core_temps = read_per_core_temperatures();
        let per_core_freqs = read_per_core_frequencies();

        (total_percent, per_core, freq, temperature, per_core_temps, per_core_freqs)
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

fn read_cpu_temperature() -> f64 {
    // Try hwmon: look for coretemp (Intel) or k10temp (AMD)
    if let Ok(entries) = fs::read_dir("/sys/class/hwmon") {
        for entry in entries.flatten() {
            let path = entry.path();
            let name = fs::read_to_string(path.join("name")).unwrap_or_default();
            let name = name.trim();
            if name == "coretemp" || name == "k10temp" {
                if let Ok(temp_str) = fs::read_to_string(path.join("temp1_input")) {
                    if let Ok(millideg) = temp_str.trim().parse::<f64>() {
                        return millideg / 1000.0;
                    }
                }
            }
        }
    }

    // Fallback: thermal_zone0
    if let Ok(temp_str) = fs::read_to_string("/sys/class/thermal/thermal_zone0/temp") {
        if let Ok(millideg) = temp_str.trim().parse::<f64>() {
            return millideg / 1000.0;
        }
    }

    0.0
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

fn read_per_core_temperatures() -> Vec<f64> {
    let mut temps = Vec::new();

    // Try hwmon: look for coretemp (Intel) or k10temp (AMD)
    if let Ok(entries) = fs::read_dir("/sys/class/hwmon") {
        for entry in entries.flatten() {
            let path = entry.path();
            let name = fs::read_to_string(path.join("name")).unwrap_or_default();
            let name = name.trim();

            if name == "coretemp" {
                // Intel: temp1 is package, temp2+ are per-core
                for i in 2..=64 {
                    let temp_file = format!("temp{}_input", i);
                    if let Ok(temp_str) = fs::read_to_string(path.join(&temp_file)) {
                        if let Ok(millideg) = temp_str.trim().parse::<f64>() {
                            temps.push(millideg / 1000.0);
                        }
                    } else {
                        break; // No more temp files
                    }
                }
                return temps;
            } else if name == "k10temp" {
                // AMD Ryzen: typically only exposes Tctl (temp1)
                if let Ok(temp_str) = fs::read_to_string(path.join("temp1_input")) {
                    if let Ok(millideg) = temp_str.trim().parse::<f64>() {
                        temps.push(millideg / 1000.0);
                    }
                }
                return temps;
            }
        }
    }

    temps
}

fn read_per_core_frequencies() -> Vec<(f64, String)> {
    let mut freqs = Vec::new();

    // Iterate through cpu0, cpu1, cpu2, etc.
    for i in 0..256 {
        let freq_path = format!("/sys/devices/system/cpu/cpu{}/cpufreq/scaling_cur_freq", i);
        let governor_path = format!("/sys/devices/system/cpu/cpu{}/cpufreq/scaling_governor", i);

        let freq = match fs::read_to_string(&freq_path) {
            Ok(f) => f.trim().parse::<f64>().unwrap_or(0.0) / 1000.0, // kHz -> MHz
            Err(_) => break, // No more CPU cores
        };

        let governor = fs::read_to_string(&governor_path)
            .unwrap_or_else(|_| "unknown".to_string())
            .trim()
            .to_string();

        freqs.push((freq, governor));
    }

    freqs
}
