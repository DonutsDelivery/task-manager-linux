use crate::model::ProcessInfo;
use std::collections::HashMap;
use std::fs;

pub struct ProcessCollector {
    prev_processes: HashMap<i32, (u64, u64, u64)>, // pid -> (cpu_time, disk_read, disk_write)
    prev_total_cpu: u64,
    total_memory: u64,
}

impl ProcessCollector {
    pub fn new() -> Self {
        let total_memory = get_total_memory();
        Self {
            prev_processes: HashMap::new(),
            prev_total_cpu: 0,
            total_memory,
        }
    }

    pub fn collect(
        &mut self,
        gpu_vram: &HashMap<u32, u64>,
        desktop_names: &HashMap<String, String>,
        window_titles: &HashMap<u32, String>,
    ) -> Vec<ProcessInfo> {
        let total_cpu = read_total_cpu_time();
        let delta_total = total_cpu.saturating_sub(self.prev_total_cpu);
        let num_cores = num_cpus();

        let mut processes = Vec::new();
        let proc_entries = fs::read_dir("/proc").unwrap_or_else(|_| {
            fs::read_dir("/proc").unwrap()
        });

        for entry in proc_entries.flatten() {
            let name = entry.file_name();
            let name_str = name.to_string_lossy();
            let pid: i32 = match name_str.parse() {
                Ok(p) => p,
                Err(_) => continue,
            };

            if let Some(mut info) = read_process(pid) {
                // CPU percent
                let prev = self.prev_processes.get(&pid);
                let prev_cpu = prev.map(|(c, _, _)| *c).unwrap_or(0);
                let cpu_delta = info.total_cpu_time.saturating_sub(prev_cpu);
                info.cpu_percent = if delta_total > 0 {
                    (cpu_delta as f64 / delta_total as f64) * 100.0 * num_cores as f64
                } else {
                    0.0
                };
                info.prev_cpu_time = prev_cpu;

                // Memory percent
                info.memory_percent = if self.total_memory > 0 {
                    (info.memory_bytes as f64 / self.total_memory as f64) * 100.0
                } else {
                    0.0
                };

                // Disk I/O rates
                let prev_dr = prev.map(|(_, r, _)| *r).unwrap_or(info.disk_read_bytes);
                let prev_dw = prev.map(|(_, _, w)| *w).unwrap_or(info.disk_write_bytes);
                info.disk_read_rate = info.disk_read_bytes.saturating_sub(prev_dr) as f64;
                info.disk_write_rate = info.disk_write_bytes.saturating_sub(prev_dw) as f64;
                info.prev_disk_read = prev_dr;
                info.prev_disk_write = prev_dw;

                // GPU VRAM
                if let Some(&vram) = gpu_vram.get(&(pid as u32)) {
                    info.vram_bytes = vram;
                }

                // Display name resolution
                resolve_display_name(&mut info, window_titles, desktop_names);

                self.prev_processes.insert(pid, (
                    info.total_cpu_time,
                    info.disk_read_bytes,
                    info.disk_write_bytes,
                ));

                processes.push(info);
            }
        }

        self.prev_total_cpu = total_cpu;

        // Prune dead processes
        let live_pids: std::collections::HashSet<i32> = processes.iter().map(|p| p.pid).collect();
        self.prev_processes.retain(|pid, _| live_pids.contains(pid));

        processes
    }
}

fn read_process(pid: i32) -> Option<ProcessInfo> {
    let stat = fs::read_to_string(format!("/proc/{}/stat", pid)).ok()?;
    let status = fs::read_to_string(format!("/proc/{}/status", pid)).ok()?;

    let mut info = ProcessInfo::default();
    info.pid = pid;

    // Parse stat - handle comm field which may contain spaces and parens
    let comm_start = stat.find('(')?;
    let comm_end = stat.rfind(')')?;
    info.name = stat[comm_start + 1..comm_end].to_string();

    let rest = &stat[comm_end + 2..];
    let fields: Vec<&str> = rest.split_whitespace().collect();
    if fields.len() < 22 {
        return None;
    }

    info.state = fields[0].to_string();
    info.ppid = fields[1].parse().unwrap_or(0);
    info.nice = fields[16].parse().unwrap_or(0);
    info.threads = fields[17].parse().unwrap_or(0);
    info.start_time = fields[19].parse().unwrap_or(0);

    let utime: u64 = fields[11].parse().unwrap_or(0);
    let stime: u64 = fields[12].parse().unwrap_or(0);
    info.total_cpu_time = utime + stime;

    // Parse status for uid and memory
    for line in status.lines() {
        if let Some(val) = line.strip_prefix("Uid:") {
            info.uid = val.split_whitespace().next()
                .and_then(|s| s.parse().ok()).unwrap_or(0);
        } else if let Some(val) = line.strip_prefix("VmRSS:") {
            info.memory_bytes = val.trim().split_whitespace().next()
                .and_then(|s| s.parse::<u64>().ok()).unwrap_or(0) * 1024;
        }
    }

    // User name
    info.user = get_username(info.uid);

    // Command line
    info.command = fs::read_to_string(format!("/proc/{}/cmdline", pid))
        .unwrap_or_default()
        .replace('\0', " ")
        .trim()
        .to_string();

    // Exe path
    info.exe_path = fs::read_link(format!("/proc/{}/exe", pid))
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or_default();

    // Disk I/O
    if let Ok(io) = fs::read_to_string(format!("/proc/{}/io", pid)) {
        for line in io.lines() {
            if let Some(val) = line.strip_prefix("read_bytes: ") {
                info.disk_read_bytes = val.trim().parse().unwrap_or(0);
            } else if let Some(val) = line.strip_prefix("write_bytes: ") {
                info.disk_write_bytes = val.trim().parse().unwrap_or(0);
            }
        }
    }

    info.display_name = info.name.clone();

    Some(info)
}

fn resolve_display_name(
    info: &mut ProcessInfo,
    window_titles: &HashMap<u32, String>,
    desktop_names: &HashMap<String, String>,
) {
    // Priority 1: window title
    if let Some(title) = window_titles.get(&(info.pid as u32)) {
        if !title.is_empty() {
            info.display_name = title.clone();
            return;
        }
    }

    // Priority 2: desktop entry name
    let exe_basename = info.exe_path.rsplit('/').next().unwrap_or(&info.name);
    if let Some(desktop_name) = desktop_names.get(exe_basename) {
        info.display_name = desktop_name.clone();
        return;
    }
    // Also try lowercase
    if let Some(desktop_name) = desktop_names.get(&exe_basename.to_lowercase()) {
        info.display_name = desktop_name.clone();
        return;
    }

    // Priority 3: for interpreters (python, bash, etc.), derive name from script path in cmdline
    if is_interpreter(exe_basename) {
        if let Some(name) = script_display_name(&info.command) {
            info.display_name = name;
            return;
        }
    }

    // Priority 4: comm name (already set as default)
}

/// Check if an executable basename is a script interpreter.
fn is_interpreter(basename: &str) -> bool {
    let name = basename.to_lowercase();
    matches!(
        name.as_str(),
        "bash" | "sh" | "zsh" | "fish" | "dash"
        | "python" | "python3" | "python2"
        | "perl" | "ruby" | "node" | "nodejs"
        | "java" | "javaw" | "dotnet" | "mono"
    )
}

/// Extract a display name from the script path in a command line.
/// e.g. "python /home/user/Programs/ComfyUI/main.py --port 8188" -> "ComfyUI"
fn script_display_name(cmdline: &str) -> Option<String> {
    // Find first argument that looks like a script path (contains '/' or ends in script extension)
    for arg in cmdline.split_whitespace().skip(1) {
        if arg.starts_with('-') {
            continue;
        }
        if arg.contains('/') || arg.ends_with(".py") || arg.ends_with(".sh")
            || arg.ends_with(".rb") || arg.ends_with(".pl") || arg.ends_with(".js")
        {
            let path = std::path::Path::new(arg);
            let stem = path.file_stem()?.to_string_lossy();

            // For generic entry points like main.py, app.py, use parent dir name
            let generic = ["main", "app", "run", "__main__", "manage", "server", "index", "cli"];
            if generic.contains(&stem.as_ref()) {
                if let Some(parent) = path.parent() {
                    let dir_name = parent.file_name()?.to_string_lossy();
                    if !dir_name.is_empty() {
                        return Some(dir_name.to_string());
                    }
                }
            }

            return Some(stem.to_string());
        }
    }
    None
}

fn read_total_cpu_time() -> u64 {
    fs::read_to_string("/proc/stat")
        .unwrap_or_default()
        .lines()
        .next()
        .map(|line| {
            line.split_whitespace()
                .skip(1)
                .filter_map(|s| s.parse::<u64>().ok())
                .sum()
        })
        .unwrap_or(0)
}

fn get_total_memory() -> u64 {
    fs::read_to_string("/proc/meminfo")
        .unwrap_or_default()
        .lines()
        .find(|l| l.starts_with("MemTotal:"))
        .and_then(|l| l.split_whitespace().nth(1))
        .and_then(|s| s.parse::<u64>().ok())
        .unwrap_or(0) * 1024
}

fn num_cpus() -> usize {
    fs::read_to_string("/proc/stat")
        .unwrap_or_default()
        .lines()
        .filter(|l| l.starts_with("cpu") && !l.starts_with("cpu "))
        .count()
        .max(1)
}

fn get_username(uid: u32) -> String {
    fs::read_to_string("/etc/passwd")
        .unwrap_or_default()
        .lines()
        .find(|line| {
            line.split(':').nth(2)
                .and_then(|s| s.parse::<u32>().ok())
                .map(|u| u == uid)
                .unwrap_or(false)
        })
        .and_then(|line| line.split(':').next())
        .map(|s| s.to_string())
        .unwrap_or_else(|| uid.to_string())
}
