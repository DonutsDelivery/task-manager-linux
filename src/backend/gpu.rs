use crate::model::GpuInfo;
use nvml_wrapper::Nvml;
use nvml_wrapper::enums::device::UsedGpuMemory;
use std::collections::HashMap;

// ---------------------------------------------------------------------------
// Sysfs helpers
// ---------------------------------------------------------------------------

fn read_sysfs_u64(path: &str) -> Option<u64> {
    std::fs::read_to_string(path).ok()?.trim().parse().ok()
}

fn read_sysfs_string(path: &str) -> Option<String> {
    Some(std::fs::read_to_string(path).ok()?.trim().to_string())
}

fn find_hwmon_path(device_path: &str) -> Option<String> {
    let hwmon_dir = format!("{}/hwmon", device_path);
    let entries = std::fs::read_dir(&hwmon_dir).ok()?;
    for entry in entries.flatten() {
        return Some(entry.path().to_string_lossy().to_string());
    }
    None
}

// ---------------------------------------------------------------------------
// GPU backend detection
// ---------------------------------------------------------------------------

enum GpuBackend {
    Nvidia(Nvml),
    Amd {
        card_path: String,   // e.g. /sys/class/drm/card0
        device_path: String, // e.g. /sys/class/drm/card0/device
        hwmon_path: Option<String>,
        name: String,
    },
    Intel {
        card_path: String,
        device_path: String,
        hwmon_path: Option<String>,
        name: String,
    },
    None,
}

/// Scan /sys/class/drm/card* for all cards whose device/vendor matches `vendor_id`.
/// Returns Vec of (card_path, device_path) for all matches.
fn find_drm_cards_by_vendor(vendor_id: &str) -> Vec<(String, String)> {
    let drm_dir = match std::fs::read_dir("/sys/class/drm") {
        Ok(d) => d,
        Err(_) => return Vec::new(),
    };
    let mut cards: Vec<_> = drm_dir
        .filter_map(|e| e.ok())
        .filter(|e| {
            let name = e.file_name();
            let name = name.to_string_lossy();
            // Match card0, card1, ... but not card0-DP-1 etc.
            name.starts_with("card") && name[4..].chars().all(|c| c.is_ascii_digit())
        })
        .collect();
    // Sort so we check card0, card1, ... in order
    cards.sort_by_key(|e| e.file_name());

    let mut result = Vec::new();
    for entry in cards {
        let card_path = entry.path().to_string_lossy().to_string();
        let device_path = format!("{}/device", card_path);
        let vendor_path = format!("{}/vendor", device_path);
        if let Some(vendor) = read_sysfs_string(&vendor_path) {
            if vendor == vendor_id {
                result.push((card_path, device_path));
            }
        }
    }
    result
}

fn detect_amd_gpu_name(device_path: &str, hwmon_path: &Option<String>) -> String {
    // Try product_name first (newer kernels / some dGPUs)
    if let Some(name) = read_sysfs_string(&format!("{}/product_name", device_path)) {
        if !name.is_empty() {
            return name;
        }
    }
    // Try hwmon name
    if let Some(ref hp) = hwmon_path {
        if let Some(name) = read_sysfs_string(&format!("{}/name", hp)) {
            if !name.is_empty() {
                return name;
            }
        }
    }
    "AMD GPU".to_string()
}

fn detect_intel_gpu_name(card_path: &str, device_path: &str) -> String {
    // Try device/label (sometimes present on discrete Intel Arc)
    if let Some(name) = read_sysfs_string(&format!("{}/label", device_path)) {
        if !name.is_empty() {
            return name;
        }
    }
    // Try card-level label
    if let Some(name) = read_sysfs_string(&format!("{}/device/label", card_path)) {
        if !name.is_empty() {
            return name;
        }
    }
    "Intel GPU".to_string()
}

fn detect_backends() -> Vec<GpuBackend> {
    let mut backends = Vec::new();

    // 1) Try NVIDIA via NVML (can have multiple NVIDIA GPUs)
    if let Ok(nvml) = Nvml::init() {
        log::info!("NVML initialized successfully");
        backends.push(GpuBackend::Nvidia(nvml));
    }

    // 2) Scan ALL AMD cards (vendor 0x1002)
    for (card_path, device_path) in find_drm_cards_by_vendor("0x1002") {
        let hwmon_path = find_hwmon_path(&device_path);
        let name = detect_amd_gpu_name(&device_path, &hwmon_path);
        log::info!("AMD GPU detected via sysfs: {} ({})", name, card_path);
        backends.push(GpuBackend::Amd {
            card_path,
            device_path,
            hwmon_path,
            name,
        });
    }

    // 3) Scan ALL Intel cards (vendor 0x8086)
    for (card_path, device_path) in find_drm_cards_by_vendor("0x8086") {
        let hwmon_path = find_hwmon_path(&device_path);
        let name = detect_intel_gpu_name(&card_path, &device_path);
        log::info!("Intel GPU detected via sysfs: {} ({})", name, card_path);
        backends.push(GpuBackend::Intel {
            card_path,
            device_path,
            hwmon_path,
            name,
        });
    }

    if backends.is_empty() {
        log::warn!("No GPU detected - GPU monitoring disabled");
    }

    backends
}

// ---------------------------------------------------------------------------
// GpuCollector
// ---------------------------------------------------------------------------

pub struct GpuCollector {
    backends: Vec<GpuBackend>,
}

impl GpuCollector {
    pub fn new() -> Self {
        Self {
            backends: detect_backends(),
        }
    }

    pub fn collect_system(&self) -> Vec<GpuInfo> {
        let mut gpu_infos = Vec::new();

        for backend in &self.backends {
            match backend {
                GpuBackend::Nvidia(nvml) => {
                    // NVML can have multiple NVIDIA devices
                    if let Ok(device_count) = nvml.device_count() {
                        for index in 0..device_count {
                            gpu_infos.push(self.collect_nvidia(nvml, index));
                        }
                    }
                }
                GpuBackend::Amd {
                    card_path: _,
                    device_path,
                    hwmon_path,
                    name,
                } => {
                    gpu_infos.push(Self::collect_amd(device_path, hwmon_path, name));
                }
                GpuBackend::Intel {
                    card_path,
                    device_path: _,
                    hwmon_path,
                    name,
                } => {
                    gpu_infos.push(Self::collect_intel(card_path, hwmon_path, name));
                }
                GpuBackend::None => {
                    // Skip None backends
                }
            }
        }

        gpu_infos
    }

    pub fn collect_per_process(&self) -> HashMap<u32, u64> {
        let mut map = HashMap::new();

        // Aggregate across all NVIDIA GPUs
        for backend in &self.backends {
            if let GpuBackend::Nvidia(nvml) = backend {
                let per_process = self.collect_per_process_nvidia(nvml);
                for (pid, vram) in per_process {
                    *map.entry(pid).or_insert(0) += vram;
                }
            }
        }
        // Per-process VRAM tracking not available via sysfs for AMD/Intel

        map
    }

    // ------------------------------------------------------------------
    // NVIDIA (NVML)
    // ------------------------------------------------------------------

    fn collect_nvidia(&self, nvml: &Nvml, index: u32) -> GpuInfo {
        let device = match nvml.device_by_index(index) {
            Ok(d) => d,
            Err(_) => return GpuInfo::default(),
        };

        let name = device.name().unwrap_or_else(|_| "Unknown GPU".to_string());
        let utilization = device.utilization_rates().ok();
        let memory_info = device.memory_info().ok();
        let temp = device
            .temperature(nvml_wrapper::enum_wrappers::device::TemperatureSensor::Gpu)
            .unwrap_or(0);
        let power = device.power_usage().unwrap_or(0) as f64 / 1000.0; // mW to W
        let power_limit = device.enforced_power_limit().unwrap_or(0) as f64 / 1000.0;
        let fan = device.fan_speed(0).unwrap_or(0);

        GpuInfo {
            available: true,
            name,
            utilization_percent: utilization.map(|u| u.gpu as f64).unwrap_or(0.0),
            vram_used: memory_info.as_ref().map(|m| m.used).unwrap_or(0),
            vram_total: memory_info.as_ref().map(|m| m.total).unwrap_or(0),
            temperature: temp,
            power_watts: power,
            power_limit_watts: power_limit,
            fan_speed_percent: fan,
        }
    }

    fn collect_per_process_nvidia(&self, nvml: &Nvml) -> HashMap<u32, u64> {
        let mut map = HashMap::new();

        // Iterate over all NVIDIA devices
        if let Ok(device_count) = nvml.device_count() {
            for index in 0..device_count {
                let device = match nvml.device_by_index(index) {
                    Ok(d) => d,
                    Err(_) => continue,
                };

                if let Ok(procs) = device.running_compute_processes() {
                    for p in procs {
                        let mem = match p.used_gpu_memory {
                            UsedGpuMemory::Used(bytes) => bytes,
                            UsedGpuMemory::Unavailable => 0,
                        };
                        *map.entry(p.pid).or_insert(0) += mem;
                    }
                }
                if let Ok(procs) = device.running_graphics_processes() {
                    for p in procs {
                        let mem = match p.used_gpu_memory {
                            UsedGpuMemory::Used(bytes) => bytes,
                            UsedGpuMemory::Unavailable => 0,
                        };
                        *map.entry(p.pid).or_insert(0) += mem;
                    }
                }
            }
        }

        map
    }

    // ------------------------------------------------------------------
    // AMD (sysfs)
    // ------------------------------------------------------------------

    fn collect_amd(device_path: &str, hwmon_path: &Option<String>, name: &str) -> GpuInfo {
        let utilization = read_sysfs_u64(&format!("{}/gpu_busy_percent", device_path))
            .map(|v| v as f64)
            .unwrap_or(0.0);

        let vram_total =
            read_sysfs_u64(&format!("{}/mem_info_vram_total", device_path)).unwrap_or(0);
        let vram_used =
            read_sysfs_u64(&format!("{}/mem_info_vram_used", device_path)).unwrap_or(0);

        let mut temperature: u32 = 0;
        let mut power_watts: f64 = 0.0;
        let mut fan_speed_percent: u32 = 0;

        if let Some(ref hp) = hwmon_path {
            // temp1_input is in millidegrees Celsius
            temperature = read_sysfs_u64(&format!("{}/temp1_input", hp))
                .map(|v| (v / 1000) as u32)
                .unwrap_or(0);

            // power1_average is in microwatts
            power_watts = read_sysfs_u64(&format!("{}/power1_average", hp))
                .map(|v| v as f64 / 1_000_000.0)
                .unwrap_or(0.0);

            // Fan speed: pwm1 is 0-255, convert to percent
            // Or try fan1_input (RPM) — use pwm1 for percentage
            fan_speed_percent = read_sysfs_u64(&format!("{}/pwm1", hp))
                .map(|v| ((v as f64 / 255.0) * 100.0) as u32)
                .unwrap_or(0);
        }

        // AMD sysfs doesn't expose a power limit file consistently;
        // try power1_cap (microwatts)
        let power_limit_watts = hwmon_path
            .as_ref()
            .and_then(|hp| read_sysfs_u64(&format!("{}/power1_cap", hp)))
            .map(|v| v as f64 / 1_000_000.0)
            .unwrap_or(0.0);

        GpuInfo {
            available: true,
            name: name.to_string(),
            utilization_percent: utilization,
            vram_used,
            vram_total,
            temperature,
            power_watts,
            power_limit_watts,
            fan_speed_percent,
        }
    }

    // ------------------------------------------------------------------
    // Intel (sysfs)
    // ------------------------------------------------------------------

    fn collect_intel(card_path: &str, hwmon_path: &Option<String>, name: &str) -> GpuInfo {
        // Intel integrated GPUs expose much less info than discrete.
        // Intel Arc (discrete) may have hwmon entries.

        let mut temperature: u32 = 0;
        let mut power_watts: f64 = 0.0;
        let mut fan_speed_percent: u32 = 0;

        if let Some(ref hp) = hwmon_path {
            temperature = read_sysfs_u64(&format!("{}/temp1_input", hp))
                .map(|v| (v / 1000) as u32)
                .unwrap_or(0);

            power_watts = read_sysfs_u64(&format!("{}/power1_average", hp))
                .map(|v| v as f64 / 1_000_000.0)
                .unwrap_or(0.0);

            fan_speed_percent = read_sysfs_u64(&format!("{}/pwm1", hp))
                .map(|v| ((v as f64 / 255.0) * 100.0) as u32)
                .unwrap_or(0);
        }

        let power_limit_watts = hwmon_path
            .as_ref()
            .and_then(|hp| read_sysfs_u64(&format!("{}/power1_cap", hp)))
            .map(|v| v as f64 / 1_000_000.0)
            .unwrap_or(0.0);

        // Intel discrete (Arc) may have VRAM info under device/
        let device_path = format!("{}/device", card_path);
        let vram_total =
            read_sysfs_u64(&format!("{}/mem_info_vram_total", device_path)).unwrap_or(0);
        let vram_used =
            read_sysfs_u64(&format!("{}/mem_info_vram_used", device_path)).unwrap_or(0);

        // Try to read current frequency (informational — fits utilization_percent
        // as a rough indicator when no busy_percent exists)
        let _cur_freq = read_sysfs_u64(&format!("{}/gt_cur_freq_mhz", card_path));
        let _max_freq = read_sysfs_u64(&format!("{}/gt_max_freq_mhz", card_path));

        // Utilization: Intel doesn't expose gpu_busy_percent in sysfs for
        // most cases, but some discrete cards may. Try it.
        let utilization =
            read_sysfs_u64(&format!("{}/gpu_busy_percent", device_path))
                .map(|v| v as f64)
                .unwrap_or(0.0);

        GpuInfo {
            available: true,
            name: name.to_string(),
            utilization_percent: utilization,
            vram_used,
            vram_total,
            temperature,
            power_watts,
            power_limit_watts,
            fan_speed_percent,
        }
    }
}
