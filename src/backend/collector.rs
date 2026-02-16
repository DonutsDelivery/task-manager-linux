use crate::backend::cpu::{self, CpuCollector};
use crate::backend::disk::DiskCollector;
use crate::backend::gpu::GpuCollector;
use crate::backend::memory::MemoryCollector;
use crate::backend::network::NetworkCollector;
use crate::backend::process::ProcessCollector;
use crate::backend::battery::BatteryCollector;
use crate::backend::history::AppHistoryTracker;
use crate::backend::DesktopResolver;
use crate::backend::WindowResolver;
use crate::model::{AppGroup, SystemSnapshot};
use std::collections::HashMap;
use std::thread;
use std::time::Duration;

pub struct Collector {
    tx: flume::Sender<SystemSnapshot>,
}

impl Collector {
    pub fn new() -> (Self, flume::Receiver<SystemSnapshot>) {
        let (tx, rx) = flume::bounded(2);
        (Self { tx }, rx)
    }

    pub fn start(self) {
        thread::Builder::new()
            .name("collector".into())
            .spawn(move || {
                self.run();
            })
            .expect("Failed to spawn collector thread");
    }

    fn run(self) {
        let mut cpu_collector = CpuCollector::new();
        let memory_collector = MemoryCollector::new();
        let mut disk_collector = DiskCollector::new();
        let mut network_collector = NetworkCollector::new();
        let gpu_collector = GpuCollector::new();
        let mut process_collector = ProcessCollector::new();
        let battery_collector = BatteryCollector::new();
        let mut history_tracker = AppHistoryTracker::new();
        let desktop_resolver = DesktopResolver::new();
        let window_resolver = WindowResolver::new();

        // Initial collection to prime deltas
        let _ = cpu_collector.collect();
        thread::sleep(Duration::from_millis(500));

        loop {
            let (cpu_total, cpu_per_core, cpu_freq, cpu_temp, cpu_per_core_temps, cpu_per_core_freqs) = cpu_collector.collect();
            let memory = memory_collector.collect();
            let disk = disk_collector.collect();
            let network = network_collector.collect();
            let gpu_system = gpu_collector.collect_system();
            let gpu_vram = gpu_collector.collect_per_process();
            let battery = battery_collector.collect();
            let window_titles = window_resolver.collect();

            let processes = process_collector.collect(
                &gpu_vram,
                desktop_resolver.names(),
                &window_titles,
            );

            let thread_count: u64 = processes.iter().map(|p| p.threads).sum();
            let process_count = processes.len();

            let app_groups = build_app_groups(&processes);

            // Update history tracker
            history_tracker.update(&app_groups);
            let app_histories = history_tracker.snapshot();

            let battery_model = crate::model::BatteryInfo {
                available: battery.available,
                percent: battery.percent,
                status: battery.status,
                power_watts: battery.power_watts,
                time_remaining_secs: battery.time_remaining_secs,
                ac_connected: battery.ac_connected,
            };

            let snapshot = SystemSnapshot {
                processes,
                app_groups,
                cpu: crate::model::CpuInfo {
                    total_percent: cpu_total,
                    per_core_percent: cpu_per_core,
                    core_count: cpu_collector.core_count,
                    model_name: cpu_collector.model_name.clone(),
                    frequency_mhz: cpu_freq,
                    uptime_secs: cpu::uptime_secs(),
                    temperature_celsius: cpu_temp,
                    per_core_temperatures: cpu_per_core_temps,
                    per_core_frequencies: cpu_per_core_freqs,
                },
                memory,
                disk,
                network,
                gpu: gpu_system,
                battery: battery_model,
                process_count,
                thread_count,
                app_histories,
            };

            if self.tx.send(snapshot).is_err() {
                log::info!("Collector channel closed, shutting down");
                break;
            }

            thread::sleep(Duration::from_secs(1));
        }
    }
}

fn build_app_groups(processes: &[crate::model::ProcessInfo]) -> Vec<AppGroup> {
    let pid_map: HashMap<i32, &crate::model::ProcessInfo> =
        processes.iter().map(|p| (p.pid, p)).collect();

    let mut groups: HashMap<i32, AppGroup> = HashMap::new();
    let mut assigned: HashMap<i32, i32> = HashMap::new(); // child_pid -> group leader pid

    // Find group leaders (processes with windows, or whose parent is pid 1 or systemd)
    for proc in processes {
        let is_leader = proc.ppid <= 1
            || proc.ppid == proc.pid
            || !pid_map.contains_key(&proc.ppid)
            || !proc.display_name.is_empty() && proc.display_name != proc.name;

        if is_leader && !assigned.contains_key(&proc.pid) {
            groups.insert(proc.pid, AppGroup::new(proc.clone()));
            assigned.insert(proc.pid, proc.pid);
        }
    }

    // Assign children to their leaders
    for proc in processes {
        if assigned.contains_key(&proc.pid) {
            continue;
        }

        // Walk up the parent chain to find a leader
        let mut parent = proc.ppid;
        let mut leader_pid = None;
        let mut visited = std::collections::HashSet::new();
        while parent > 1 && visited.insert(parent) {
            if let Some(&lead) = assigned.get(&parent) {
                leader_pid = Some(lead);
                break;
            }
            if let Some(p) = pid_map.get(&parent) {
                parent = p.ppid;
            } else {
                break;
            }
        }

        if let Some(lead) = leader_pid {
            assigned.insert(proc.pid, lead);
            if let Some(group) = groups.get_mut(&lead) {
                group.add_child(proc.clone());
            }
        } else {
            // No leader found, make this process its own group
            groups.insert(proc.pid, AppGroup::new(proc.clone()));
            assigned.insert(proc.pid, proc.pid);
        }
    }

    let mut result: Vec<AppGroup> = groups.into_values().collect();
    result.sort_by(|a, b| b.total_cpu.partial_cmp(&a.total_cpu).unwrap_or(std::cmp::Ordering::Equal));
    result
}
