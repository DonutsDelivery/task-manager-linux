pub fn format_bytes(bytes: u64) -> String {
    const KB: u64 = 1024;
    const MB: u64 = 1024 * KB;
    const GB: u64 = 1024 * MB;
    const TB: u64 = 1024 * GB;

    if bytes >= TB {
        format!("{:.1} TB", bytes as f64 / TB as f64)
    } else if bytes >= GB {
        format!("{:.1} GB", bytes as f64 / GB as f64)
    } else if bytes >= MB {
        format!("{:.1} MB", bytes as f64 / MB as f64)
    } else if bytes >= KB {
        format!("{:.1} KB", bytes as f64 / KB as f64)
    } else {
        format!("{} B", bytes)
    }
}

pub fn format_bytes_rate(bytes_per_sec: f64) -> String {
    const KB: f64 = 1024.0;
    const MB: f64 = 1024.0 * KB;
    const GB: f64 = 1024.0 * MB;

    if bytes_per_sec >= GB {
        format!("{:.1} GB/s", bytes_per_sec / GB)
    } else if bytes_per_sec >= MB {
        format!("{:.1} MB/s", bytes_per_sec / MB)
    } else if bytes_per_sec >= KB {
        format!("{:.1} KB/s", bytes_per_sec / KB)
    } else {
        format!("{:.0} B/s", bytes_per_sec)
    }
}

pub fn format_percent(value: f64) -> String {
    if value >= 10.0 {
        format!("{:.1}%", value)
    } else {
        format!("{:.2}%", value)
    }
}

pub fn format_duration(secs: u64) -> String {
    let days = secs / 86400;
    let hours = (secs % 86400) / 3600;
    let mins = (secs % 3600) / 60;

    if days > 0 {
        format!("{}d {}h {}m", days, hours, mins)
    } else if hours > 0 {
        format!("{}h {}m", hours, mins)
    } else {
        format!("{}m", mins)
    }
}

pub fn format_frequency(mhz: f64) -> String {
    if mhz >= 1000.0 {
        format!("{:.2} GHz", mhz / 1000.0)
    } else {
        format!("{:.0} MHz", mhz)
    }
}

pub fn format_temperature(celsius: f64) -> String {
    if celsius > 0.0 {
        format!("{:.0}°C", celsius)
    } else {
        "N/A".to_string()
    }
}
