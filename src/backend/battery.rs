use std::fs;
use std::path::Path;

#[derive(Debug, Clone, Default)]
pub struct BatteryInfo {
    pub available: bool,
    pub percent: f64,
    pub status: String,      // "Charging", "Discharging", "Full", "Not charging", "Unknown"
    pub power_watts: f64,
    pub time_remaining_secs: u64,
    pub ac_connected: bool,
    pub energy_now: u64,     // microWh
    pub energy_full: u64,    // microWh
}

pub struct BatteryCollector {
    battery_path: Option<String>,
    ac_path: Option<String>,
}

impl BatteryCollector {
    pub fn new() -> Self {
        // Find first battery
        let battery_path = find_power_supply("Battery");
        let ac_path = find_power_supply_ac();

        if battery_path.is_some() {
            log::info!("Battery found: {:?}", battery_path);
        } else {
            log::info!("No battery found (desktop PC?)");
        }

        Self { battery_path, ac_path }
    }

    pub fn collect(&self) -> BatteryInfo {
        let bat_path = match &self.battery_path {
            Some(p) => p,
            None => return BatteryInfo::default(),
        };

        let mut info = BatteryInfo {
            available: true,
            ..Default::default()
        };

        // Read energy values (try energy_now first, then charge_now)
        info.energy_now = read_sysfs_u64(&format!("{}/energy_now", bat_path))
            .or_else(|| {
                // Some batteries use charge_now (in µAh) + voltage_now to calculate energy
                let charge = read_sysfs_u64(&format!("{}/charge_now", bat_path))?;
                let voltage = read_sysfs_u64(&format!("{}/voltage_now", bat_path))?;
                Some(charge * voltage / 1_000_000) // µAh * µV -> µWh
            })
            .unwrap_or(0);

        info.energy_full = read_sysfs_u64(&format!("{}/energy_full", bat_path))
            .or_else(|| {
                let charge = read_sysfs_u64(&format!("{}/charge_full", bat_path))?;
                let voltage = read_sysfs_u64(&format!("{}/voltage_now", bat_path))?;
                Some(charge * voltage / 1_000_000)
            })
            .unwrap_or(0);

        // Percent
        // Try capacity first (direct percentage), else calculate
        info.percent = read_sysfs_u64(&format!("{}/capacity", bat_path))
            .map(|c| c as f64)
            .unwrap_or_else(|| {
                if info.energy_full > 0 {
                    (info.energy_now as f64 / info.energy_full as f64) * 100.0
                } else {
                    0.0
                }
            });

        // Status
        info.status = read_sysfs_string(&format!("{}/status", bat_path))
            .unwrap_or_else(|| "Unknown".to_string());

        // Power draw
        let power_now = read_sysfs_u64(&format!("{}/power_now", bat_path))
            .or_else(|| {
                let current = read_sysfs_u64(&format!("{}/current_now", bat_path))?;
                let voltage = read_sysfs_u64(&format!("{}/voltage_now", bat_path))?;
                Some(current * voltage / 1_000_000) // µA * µV -> µW
            })
            .unwrap_or(0);
        info.power_watts = power_now as f64 / 1_000_000.0; // µW to W

        // Time remaining
        if info.power_watts > 0.0 {
            let energy_wh = if info.status == "Charging" {
                (info.energy_full - info.energy_now) as f64 / 1_000_000.0
            } else {
                info.energy_now as f64 / 1_000_000.0
            };
            info.time_remaining_secs = ((energy_wh / info.power_watts) * 3600.0) as u64;
        }

        // AC status
        if let Some(ac_path) = &self.ac_path {
            info.ac_connected = read_sysfs_u64(&format!("{}/online", ac_path))
                .map(|v| v == 1)
                .unwrap_or(false);
        } else {
            // Infer from battery status
            info.ac_connected = info.status == "Charging" || info.status == "Full";
        }

        info
    }
}

fn find_power_supply(supply_type: &str) -> Option<String> {
    let ps_dir = "/sys/class/power_supply";
    let entries = fs::read_dir(ps_dir).ok()?;

    for entry in entries.flatten() {
        let path = entry.path();
        let type_path = path.join("type");
        if let Ok(t) = fs::read_to_string(&type_path) {
            if t.trim() == supply_type {
                return Some(path.to_string_lossy().to_string());
            }
        }
    }
    None
}

fn find_power_supply_ac() -> Option<String> {
    let ps_dir = "/sys/class/power_supply";
    let entries = fs::read_dir(ps_dir).ok()?;

    for entry in entries.flatten() {
        let path = entry.path();
        let type_path = path.join("type");
        if let Ok(t) = fs::read_to_string(&type_path) {
            let t = t.trim();
            if t == "Mains" || t == "USB" {
                return Some(path.to_string_lossy().to_string());
            }
        }
    }
    None
}

fn read_sysfs_u64(path: &str) -> Option<u64> {
    fs::read_to_string(path).ok()?.trim().parse().ok()
}

fn read_sysfs_string(path: &str) -> Option<String> {
    Some(fs::read_to_string(path).ok()?.trim().to_string())
}
