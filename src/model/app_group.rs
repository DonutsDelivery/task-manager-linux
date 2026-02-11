use super::ProcessInfo;

#[derive(Debug, Clone)]
pub struct AppGroup {
    pub leader: ProcessInfo,
    pub children: Vec<ProcessInfo>,
    pub total_cpu: f64,
    pub total_memory: u64,
    pub total_vram: u64,
    pub total_disk_read_rate: f64,
    pub total_disk_write_rate: f64,
}

impl AppGroup {
    pub fn new(leader: ProcessInfo) -> Self {
        let total_cpu = leader.cpu_percent;
        let total_memory = leader.memory_bytes;
        let total_vram = leader.vram_bytes;
        let total_disk_read_rate = leader.disk_read_rate;
        let total_disk_write_rate = leader.disk_write_rate;
        Self {
            leader,
            children: Vec::new(),
            total_cpu,
            total_memory,
            total_vram,
            total_disk_read_rate,
            total_disk_write_rate,
        }
    }

    pub fn add_child(&mut self, child: ProcessInfo) {
        self.total_cpu += child.cpu_percent;
        self.total_memory += child.memory_bytes;
        self.total_vram += child.vram_bytes;
        self.total_disk_read_rate += child.disk_read_rate;
        self.total_disk_write_rate += child.disk_write_rate;
        self.children.push(child);
    }

    pub fn process_count(&self) -> usize {
        1 + self.children.len()
    }

    pub fn display_name(&self) -> &str {
        &self.leader.display_name
    }

    pub fn pid(&self) -> i32 {
        self.leader.pid
    }
}
