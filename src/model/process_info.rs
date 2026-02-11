use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProcessInfo {
    pub pid: i32,
    pub ppid: i32,
    pub name: String,
    pub display_name: String,
    pub command: String,
    pub exe_path: String,
    pub state: String,
    pub cpu_percent: f64,
    pub memory_bytes: u64,
    pub memory_percent: f64,
    pub vram_bytes: u64,
    pub disk_read_bytes: u64,
    pub disk_write_bytes: u64,
    pub disk_read_rate: f64,
    pub disk_write_rate: f64,
    pub nice: i32,
    pub threads: u64,
    pub start_time: u64,
    pub uid: u32,
    pub user: String,
    pub container_type: String,
    // Internal tracking for CPU delta calculation
    pub total_cpu_time: u64,
    pub prev_cpu_time: u64,
    pub prev_disk_read: u64,
    pub prev_disk_write: u64,
}

impl Default for ProcessInfo {
    fn default() -> Self {
        Self {
            pid: 0,
            ppid: 0,
            name: String::new(),
            display_name: String::new(),
            command: String::new(),
            exe_path: String::new(),
            state: String::from("?"),
            cpu_percent: 0.0,
            memory_bytes: 0,
            memory_percent: 0.0,
            vram_bytes: 0,
            disk_read_bytes: 0,
            disk_write_bytes: 0,
            disk_read_rate: 0.0,
            disk_write_rate: 0.0,
            nice: 0,
            threads: 0,
            start_time: 0,
            uid: 0,
            user: String::new(),
            container_type: String::new(),
            total_cpu_time: 0,
            prev_cpu_time: 0,
            prev_disk_read: 0,
            prev_disk_write: 0,
        }
    }
}
