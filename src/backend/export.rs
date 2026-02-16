use std::fs::{File, OpenOptions};
use std::io::Write;
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

/// Escape a string for CSV by wrapping in quotes if it contains special chars
/// and doubling internal quotes
fn csv_escape(s: &str) -> String {
    if s.contains('"') || s.contains(',') || s.contains('\n') {
        format!("\"{}\"", s.replace('"', "\"\""))
    } else {
        s.to_string()
    }
}

/// Export process list to CSV
pub fn export_processes_csv(
    path: &Path,
    processes: &[crate::model::ProcessInfo],
) -> Result<(), String> {
    let mut file = File::create(path).map_err(|e| format!("Failed to create file: {}", e))?;

    // Write header
    writeln!(
        file,
        "PID,PPID,Name,DisplayName,State,CPU%,Memory(bytes),Memory%,VRAM(bytes),\
         DiskRead(B/s),DiskWrite(B/s),Threads,Nice,User,Container,SystemdUnit,\
         IOClass,IOPriority,SecurityLabel,Command,ExePath"
    )
    .map_err(|e| format!("Failed to write header: {}", e))?;

    // Write process rows
    for p in processes {
        writeln!(
            file,
            "{},{},{},{},{},{:.2},{},{:.2},{},{:.2},{:.2},{},{},{},{},{},{},{},{},{},{}",
            p.pid,
            p.ppid,
            csv_escape(&p.name),
            csv_escape(&p.display_name),
            csv_escape(&p.state),
            p.cpu_percent,
            p.memory_bytes,
            p.memory_percent,
            p.vram_bytes,
            p.disk_read_rate,
            p.disk_write_rate,
            p.threads,
            p.nice,
            csv_escape(&p.user),
            csv_escape(&p.container_type),
            csv_escape(&p.systemd_unit),
            csv_escape(&p.io_class),
            p.io_priority,
            csv_escape(&p.security_label),
            csv_escape(&p.command),
            csv_escape(&p.exe_path),
        )
        .map_err(|e| format!("Failed to write process row: {}", e))?;
    }

    Ok(())
}

/// Export performance snapshot to CSV (appends a row per call for time-series)
pub fn export_performance_csv(
    path: &Path,
    snapshot: &crate::model::SystemSnapshot,
    append: bool,
) -> Result<(), String> {
    let mut file = if append {
        OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)
            .map_err(|e| format!("Failed to open file for append: {}", e))?
    } else {
        let mut f = File::create(path).map_err(|e| format!("Failed to create file: {}", e))?;
        // Write header for new file
        writeln!(
            f,
            "Timestamp,CPU%,MemoryUsed(bytes),MemoryTotal(bytes),MemoryAvailable(bytes),\
             MemoryCached(bytes),SwapUsed(bytes),SwapTotal(bytes),GPU%,GPUName,\
             VRAMUsed(bytes),VRAMTotal(bytes),GPUTemp(C),GPUPower(W),DiskRead(B/s),\
             DiskWrite(B/s),NetRx(B/s),NetTx(B/s),ProcessCount,ThreadCount,\
             BatteryPercent,BatteryStatus,BatteryPower(W),CPUTemp(C),CPUFreq(MHz),Uptime(s)"
        )
        .map_err(|e| format!("Failed to write header: {}", e))?;
        f
    };

    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);

    let disk_read: f64 = snapshot.disk.devices.iter().map(|d| d.read_bytes_sec).sum();
    let disk_write: f64 = snapshot
        .disk
        .devices
        .iter()
        .map(|d| d.write_bytes_sec)
        .sum();
    let net_rx: f64 = snapshot
        .network
        .interfaces
        .iter()
        .map(|i| i.rx_bytes_sec)
        .sum();
    let net_tx: f64 = snapshot
        .network
        .interfaces
        .iter()
        .map(|i| i.tx_bytes_sec)
        .sum();

    writeln!(
        file,
        "{},{:.2},{},{},{},{},{},{},{:.2},{},{},{},{},{:.2},{:.2},{:.2},{:.2},{:.2},{},{},{:.2},{},{:.2},{:.2},{:.2},{}",
        timestamp,
        snapshot.cpu.total_percent,
        snapshot.memory.used,
        snapshot.memory.total,
        snapshot.memory.available,
        snapshot.memory.cached,
        snapshot.memory.swap_used,
        snapshot.memory.swap_total,
        snapshot.gpu.first().map(|g| g.utilization_percent).unwrap_or(0.0),
        csv_escape(&snapshot.gpu.first().map(|g| g.name.as_str()).unwrap_or("")),
        snapshot.gpu.first().map(|g| g.vram_used).unwrap_or(0),
        snapshot.gpu.first().map(|g| g.vram_total).unwrap_or(0),
        snapshot.gpu.first().map(|g| g.temperature).unwrap_or(0),
        snapshot.gpu.first().map(|g| g.power_watts).unwrap_or(0.0),
        disk_read,
        disk_write,
        net_rx,
        net_tx,
        snapshot.process_count,
        snapshot.thread_count,
        snapshot.battery.percent,
        csv_escape(&snapshot.battery.status),
        snapshot.battery.power_watts,
        snapshot.cpu.temperature_celsius,
        snapshot.cpu.frequency_mhz,
        snapshot.cpu.uptime_secs,
    )
    .map_err(|e| format!("Failed to write performance row: {}", e))?;

    Ok(())
}

/// Export app groups to CSV
pub fn export_app_groups_csv(
    path: &Path,
    app_groups: &[crate::model::AppGroup],
) -> Result<(), String> {
    let mut file = File::create(path).map_err(|e| format!("Failed to create file: {}", e))?;

    // Write header
    writeln!(
        file,
        "LeaderPID,AppName,ProcessCount,TotalCPU%,TotalMemory(bytes),\
         TotalVRAM(bytes),TotalDiskRead(B/s),TotalDiskWrite(B/s)"
    )
    .map_err(|e| format!("Failed to write header: {}", e))?;

    // Write app group rows
    for group in app_groups {
        writeln!(
            file,
            "{},{},{},{:.2},{},{},{:.2},{:.2}",
            group.leader.pid,
            csv_escape(&group.leader.display_name),
            group.process_count(),
            group.total_cpu,
            group.total_memory,
            group.total_vram,
            group.total_disk_read_rate,
            group.total_disk_write_rate,
        )
        .map_err(|e| format!("Failed to write app group row: {}", e))?;
    }

    Ok(())
}

/// Export disk device stats to CSV
pub fn export_disk_csv(
    path: &Path,
    disk_info: &crate::model::DiskInfo,
) -> Result<(), String> {
    let mut file = File::create(path).map_err(|e| format!("Failed to create file: {}", e))?;

    // Write header
    writeln!(
        file,
        "Device,ReadRate(B/s),WriteRate(B/s),TotalRead(bytes),TotalWrite(bytes)"
    )
    .map_err(|e| format!("Failed to write header: {}", e))?;

    // Write disk device rows
    for device in &disk_info.devices {
        writeln!(
            file,
            "{},{:.2},{:.2},{},{}",
            csv_escape(&device.name),
            device.read_bytes_sec,
            device.write_bytes_sec,
            device.total_read,
            device.total_write,
        )
        .map_err(|e| format!("Failed to write disk row: {}", e))?;
    }

    Ok(())
}

/// Export network interface stats to CSV
pub fn export_network_csv(
    path: &Path,
    network_info: &crate::model::NetworkInfo,
) -> Result<(), String> {
    let mut file = File::create(path).map_err(|e| format!("Failed to create file: {}", e))?;

    // Write header
    writeln!(
        file,
        "Interface,RxRate(B/s),TxRate(B/s),TotalRx(bytes),TotalTx(bytes)"
    )
    .map_err(|e| format!("Failed to write header: {}", e))?;

    // Write network interface rows
    for iface in &network_info.interfaces {
        writeln!(
            file,
            "{},{:.2},{:.2},{},{}",
            csv_escape(&iface.name),
            iface.rx_bytes_sec,
            iface.tx_bytes_sec,
            iface.total_rx,
            iface.total_tx,
        )
        .map_err(|e| format!("Failed to write network row: {}", e))?;
    }

    Ok(())
}
