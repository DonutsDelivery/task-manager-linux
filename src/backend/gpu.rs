use crate::model::GpuInfo;
use nvml_wrapper::Nvml;
use nvml_wrapper::enums::device::UsedGpuMemory;
use std::collections::HashMap;

pub struct GpuCollector {
    nvml: Option<Nvml>,
}

impl GpuCollector {
    pub fn new() -> Self {
        let nvml = Nvml::init().ok();
        if nvml.is_some() {
            log::info!("NVML initialized successfully");
        } else {
            log::warn!("NVML not available - GPU monitoring disabled");
        }
        Self { nvml }
    }

    pub fn collect_system(&self) -> GpuInfo {
        let nvml = match &self.nvml {
            Some(n) => n,
            None => return GpuInfo::default(),
        };

        let device = match nvml.device_by_index(0) {
            Ok(d) => d,
            Err(_) => return GpuInfo::default(),
        };

        let name = device.name().unwrap_or_else(|_| "Unknown GPU".to_string());
        let utilization = device.utilization_rates().ok();
        let memory_info = device.memory_info().ok();
        let temp = device.temperature(nvml_wrapper::enum_wrappers::device::TemperatureSensor::Gpu).unwrap_or(0);
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

    pub fn collect_per_process(&self) -> HashMap<u32, u64> {
        let mut map = HashMap::new();
        let nvml = match &self.nvml {
            Some(n) => n,
            None => return map,
        };

        let device = match nvml.device_by_index(0) {
            Ok(d) => d,
            Err(_) => return map,
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

        map
    }
}
