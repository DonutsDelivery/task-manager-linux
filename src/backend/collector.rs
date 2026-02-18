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

fn is_kernel_thread(proc: &crate::model::ProcessInfo) -> bool {
    // kthreadd (PID 2) and all its children are kernel threads
    proc.pid == 2 || proc.ppid == 2 || (proc.ppid == 0 && proc.pid != 1)
}

fn build_app_groups(processes: &[crate::model::ProcessInfo]) -> Vec<AppGroup> {
    let mut kernel_procs: Vec<&crate::model::ProcessInfo> = Vec::new();
    let mut by_name: HashMap<String, Vec<&crate::model::ProcessInfo>> = HashMap::new();

    for proc in processes {
        if is_kernel_thread(proc) {
            kernel_procs.push(proc);
        } else {
            // Group by exe path (handles Firefox/Brave/etc. with varied comm names)
            // Fall back to process name when exe_path is empty
            let key = if !proc.exe_path.is_empty() {
                proc.exe_path.to_lowercase()
            } else {
                proc.name.to_lowercase()
            };
            by_name.entry(key).or_default().push(proc);
        }
    }

    let mut result: Vec<AppGroup> = Vec::new();

    // Bundle all kernel threads under one "Kernel" group
    if !kernel_procs.is_empty() {
        let mut leader_info = crate::model::ProcessInfo {
            pid: 0,
            display_name: "Kernel".to_string(),
            name: "kernel".to_string(),
            ..Default::default()
        };
        // Sum up kernel thread stats for the leader
        for kp in &kernel_procs {
            leader_info.cpu_percent += kp.cpu_percent;
            leader_info.memory_bytes += kp.memory_bytes;
            leader_info.threads += 1;
        }
        let mut group = AppGroup::new(leader_info);
        for kp in &kernel_procs {
            group.add_child((*kp).clone());
        }
        result.push(group);
    }

    // Group userspace processes by exe path, then merge singletons by name prefix
    let mut groups_by_key: Vec<(String, Vec<&crate::model::ProcessInfo>)> = by_name.into_iter().collect();

    // Collect singleton groups (only 1 process) for prefix merging
    let mut singletons: Vec<(String, &crate::model::ProcessInfo)> = Vec::new();
    let mut multi: Vec<Vec<&crate::model::ProcessInfo>> = Vec::new();
    for (key, procs) in groups_by_key.drain(..) {
        if procs.len() == 1 {
            singletons.push((key, procs[0]));
        } else {
            multi.push(procs);
        }
    }

    // Merge singletons that share a name prefix (e.g. akonadi_*, gvfsd-*, xdg-*)
    let mut by_prefix: HashMap<String, Vec<&crate::model::ProcessInfo>> = HashMap::new();
    let mut no_prefix: Vec<&crate::model::ProcessInfo> = Vec::new();
    for (_key, proc) in &singletons {
        let name = &proc.name;
        // Extract prefix before first _ or - (if name has one)
        let prefix = name.find(|c: char| c == '_' || c == '-')
            .map(|i| &name[..i]);
        if let Some(pfx) = prefix {
            by_prefix.entry(pfx.to_lowercase()).or_default().push(proc);
        } else {
            no_prefix.push(proc);
        }
    }

    // Prefix groups with 2+ members get merged; others stay individual
    for (_, procs) in &by_prefix {
        if procs.len() >= 2 {
            multi.push(procs.clone());
        } else {
            no_prefix.extend(procs.iter());
        }
    }
    // Remaining singletons become their own group
    for proc in &no_prefix {
        multi.push(vec![proc]);
    }

    // Build AppGroups from all collected groups
    for procs in &multi {
        let leader_idx = procs.iter().enumerate()
            .min_by_key(|(_, p)| {
                let has_display = if p.display_name != p.name && !p.display_name.is_empty() { 0 } else { 1 };
                (has_display, p.pid)
            })
            .map(|(i, _)| i)
            .unwrap_or(0);

        let mut group = AppGroup::new(procs[leader_idx].clone());
        for (i, proc) in procs.iter().enumerate() {
            if i != leader_idx {
                group.add_child((*proc).clone());
            }
        }
        result.push(group);
    }

    result.sort_by(|a, b| b.total_cpu.partial_cmp(&a.total_cpu).unwrap_or(std::cmp::Ordering::Equal));
    result
}
