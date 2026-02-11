use super::ProcessInfo;
use super::AppGroup;
use std::collections::HashMap;

#[derive(Debug, Clone, Default)]
pub struct CpuInfo {
    pub total_percent: f64,
    pub per_core_percent: Vec<f64>,
    pub core_count: usize,
    pub model_name: String,
    pub frequency_mhz: f64,
    pub uptime_secs: u64,
    pub temperature_celsius: f64,
}

#[derive(Debug, Clone, Default)]
pub struct MemoryInfo {
    pub total: u64,
    pub used: u64,
    pub available: u64,
    pub cached: u64,
    pub swap_total: u64,
    pub swap_used: u64,
}

#[derive(Debug, Clone, Default)]
pub struct DiskInfo {
    pub devices: Vec<DiskDevice>,
}

#[derive(Debug, Clone, Default)]
pub struct DiskDevice {
    pub name: String,
    pub read_bytes_sec: f64,
    pub write_bytes_sec: f64,
    pub total_read: u64,
    pub total_write: u64,
}

#[derive(Debug, Clone, Default)]
pub struct NetworkInfo {
    pub interfaces: Vec<NetworkInterface>,
}

#[derive(Debug, Clone, Default)]
pub struct NetworkInterface {
    pub name: String,
    pub rx_bytes_sec: f64,
    pub tx_bytes_sec: f64,
    pub total_rx: u64,
    pub total_tx: u64,
}

#[derive(Debug, Clone, Default)]
pub struct GpuInfo {
    pub available: bool,
    pub name: String,
    pub utilization_percent: f64,
    pub vram_used: u64,
    pub vram_total: u64,
    pub temperature: u32,
    pub power_watts: f64,
    pub power_limit_watts: f64,
    pub fan_speed_percent: u32,
}

#[derive(Debug, Clone, Default)]
pub struct BatteryInfo {
    pub available: bool,
    pub percent: f64,
    pub status: String,
    pub power_watts: f64,
    pub time_remaining_secs: u64,
    pub ac_connected: bool,
}

#[derive(Debug, Clone)]
pub struct SystemSnapshot {
    pub processes: Vec<ProcessInfo>,
    pub app_groups: Vec<AppGroup>,
    pub cpu: CpuInfo,
    pub memory: MemoryInfo,
    pub disk: DiskInfo,
    pub network: NetworkInfo,
    pub gpu: GpuInfo,
    pub battery: BatteryInfo,
    pub process_count: usize,
    pub thread_count: u64,
    pub app_histories: HashMap<String, crate::backend::history::AppHistory>,
}

impl Default for SystemSnapshot {
    fn default() -> Self {
        Self {
            processes: Vec::new(),
            app_groups: Vec::new(),
            cpu: CpuInfo::default(),
            memory: MemoryInfo::default(),
            disk: DiskInfo::default(),
            network: NetworkInfo::default(),
            gpu: GpuInfo::default(),
            battery: BatteryInfo::default(),
            process_count: 0,
            thread_count: 0,
            app_histories: HashMap::new(),
        }
    }
}
