use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use crate::model::startup_entry::{StartupEntry, StartupSource};

pub struct StartupCollector;

impl StartupCollector {
    pub fn collect() -> Vec<StartupEntry> {
        let mut entries = Vec::new();
        let mut seen_files = HashSet::new();

        // Scan user autostart directory first (takes precedence)
        if let Ok(home) = std::env::var("HOME") {
            let user_dir = PathBuf::from(format!("{}/.config/autostart", home));
            Self::scan_autostart_dir(&user_dir, &mut entries, &mut seen_files);
        }

        // Scan system autostart directory
        let system_dir = PathBuf::from("/etc/xdg/autostart");
        Self::scan_autostart_dir(&system_dir, &mut entries, &mut seen_files);

        // Scan systemd user units
        Self::scan_systemd_user(&mut entries);

        // Sort by name for consistent display
        entries.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));

        entries
    }

    fn scan_autostart_dir(
        dir: &Path,
        entries: &mut Vec<StartupEntry>,
        seen_files: &mut HashSet<String>,
    ) {
        let read_dir = match fs::read_dir(dir) {
            Ok(rd) => rd,
            Err(_) => return,
        };

        for entry in read_dir.flatten() {
            let path = entry.path();
            if path.extension().map(|e| e == "desktop").unwrap_or(false) {
                // Use filename as dedup key so user dir overrides system dir
                let filename = path
                    .file_name()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .to_string();
                if seen_files.contains(&filename) {
                    continue;
                }
                if let Some(se) = Self::parse_desktop_file(&path) {
                    seen_files.insert(filename);
                    entries.push(se);
                }
            }
        }
    }

    fn parse_desktop_file(path: &Path) -> Option<StartupEntry> {
        let content = fs::read_to_string(path).ok()?;

        let mut name = None;
        let mut comment = None;
        let mut exec = None;
        let mut icon = None;
        let mut hidden = false;
        let mut gnome_autostart_enabled = None;
        let mut in_desktop_entry = false;

        for line in content.lines() {
            let line = line.trim();
            if line == "[Desktop Entry]" {
                in_desktop_entry = true;
                continue;
            }
            if line.starts_with('[') {
                in_desktop_entry = false;
                continue;
            }
            if !in_desktop_entry {
                continue;
            }

            if let Some(val) = line.strip_prefix("Name=") {
                if name.is_none() {
                    name = Some(val.to_string());
                }
            } else if let Some(val) = line.strip_prefix("Comment=") {
                if comment.is_none() {
                    comment = Some(val.to_string());
                }
            } else if let Some(val) = line.strip_prefix("Exec=") {
                exec = Some(val.to_string());
            } else if let Some(val) = line.strip_prefix("Icon=") {
                icon = Some(val.to_string());
            } else if let Some(val) = line.strip_prefix("Hidden=") {
                hidden = val.trim().eq_ignore_ascii_case("true");
            } else if let Some(val) = line.strip_prefix("X-GNOME-Autostart-enabled=") {
                gnome_autostart_enabled = Some(val.trim().eq_ignore_ascii_case("true"));
            }
        }

        let name = name.unwrap_or_else(|| {
            path.file_stem()
                .unwrap_or_default()
                .to_string_lossy()
                .to_string()
        });

        // Determine enabled status:
        // - Hidden=true means disabled
        // - X-GNOME-Autostart-enabled=false means disabled
        // - Otherwise enabled
        let enabled = if hidden {
            false
        } else {
            gnome_autostart_enabled.unwrap_or(true)
        };

        Some(StartupEntry {
            name,
            comment: comment.unwrap_or_default(),
            exec: exec.unwrap_or_default(),
            icon: icon.unwrap_or_default(),
            enabled,
            file_path: path.to_string_lossy().to_string(),
            source: StartupSource::Autostart,
        })
    }

    fn scan_systemd_user(entries: &mut Vec<StartupEntry>) {
        let output = match Command::new("systemctl")
            .args([
                "--user",
                "list-unit-files",
                "--type=service",
                "--state=enabled",
                "--no-legend",
                "--no-pager",
            ])
            .output()
        {
            Ok(o) => o,
            Err(e) => {
                log::warn!("Failed to list systemd user units: {}", e);
                return;
            }
        };

        if !output.status.success() {
            return;
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        for line in stdout.lines() {
            let parts: Vec<&str> = line.split_whitespace().collect();
            if parts.len() >= 2 {
                let unit_name = parts[0];
                // Skip template units (contain @)
                if unit_name.contains('@') {
                    continue;
                }
                let display_name = unit_name
                    .strip_suffix(".service")
                    .unwrap_or(unit_name)
                    .to_string();

                // Try to get the description from systemctl show
                let description = Self::get_systemd_description(unit_name);

                entries.push(StartupEntry {
                    name: display_name,
                    comment: description,
                    exec: unit_name.to_string(),
                    icon: String::new(),
                    enabled: true, // listed as enabled
                    file_path: unit_name.to_string(),
                    source: StartupSource::SystemdUser,
                });
            }
        }
    }

    fn get_systemd_description(unit_name: &str) -> String {
        let output = Command::new("systemctl")
            .args(["--user", "show", unit_name, "--property=Description", "--no-pager"])
            .output();

        match output {
            Ok(o) if o.status.success() => {
                let stdout = String::from_utf8_lossy(&o.stdout);
                for line in stdout.lines() {
                    if let Some(val) = line.strip_prefix("Description=") {
                        let val = val.trim();
                        if !val.is_empty() {
                            return val.to_string();
                        }
                    }
                }
                String::new()
            }
            _ => String::new(),
        }
    }

    pub fn toggle_autostart(entry: &StartupEntry, enabled: bool) -> Result<(), String> {
        match entry.source {
            StartupSource::Autostart => Self::toggle_desktop_autostart(entry, enabled),
            StartupSource::SystemdUser => Self::toggle_systemd_user(entry, enabled),
        }
    }

    fn toggle_desktop_autostart(entry: &StartupEntry, enabled: bool) -> Result<(), String> {
        let path = Path::new(&entry.file_path);

        // If the file is in /etc/xdg/autostart, copy to user dir first
        let user_path = if entry.file_path.starts_with("/etc/xdg/autostart") {
            let home = std::env::var("HOME").map_err(|e| format!("Cannot get HOME: {}", e))?;
            let user_dir = PathBuf::from(format!("{}/.config/autostart", home));
            fs::create_dir_all(&user_dir)
                .map_err(|e| format!("Cannot create autostart dir: {}", e))?;
            let dest = user_dir.join(
                path.file_name()
                    .ok_or_else(|| "Invalid file path".to_string())?,
            );
            if !dest.exists() {
                fs::copy(path, &dest)
                    .map_err(|e| format!("Cannot copy desktop file: {}", e))?;
            }
            dest
        } else {
            path.to_path_buf()
        };

        let content = fs::read_to_string(&user_path)
            .map_err(|e| format!("Cannot read {}: {}", user_path.display(), e))?;

        let mut lines: Vec<String> = content.lines().map(|l| l.to_string()).collect();
        let mut found_gnome_autostart = false;
        let mut found_hidden = false;

        for line in lines.iter_mut() {
            if line.starts_with("X-GNOME-Autostart-enabled=") {
                *line = format!("X-GNOME-Autostart-enabled={}", enabled);
                found_gnome_autostart = true;
            } else if line.starts_with("Hidden=") {
                *line = format!("Hidden={}", !enabled);
                found_hidden = true;
            }
        }

        // If keys weren't found, add them in the [Desktop Entry] section
        if !found_gnome_autostart || !found_hidden {
            // Find the end of [Desktop Entry] section or end of file
            let mut insert_pos = lines.len();
            let mut in_desktop_entry = false;
            for (i, line) in lines.iter().enumerate() {
                let trimmed = line.trim();
                if trimmed == "[Desktop Entry]" {
                    in_desktop_entry = true;
                    continue;
                }
                if in_desktop_entry && trimmed.starts_with('[') {
                    insert_pos = i;
                    break;
                }
            }
            if !found_gnome_autostart {
                lines.insert(
                    insert_pos,
                    format!("X-GNOME-Autostart-enabled={}", enabled),
                );
                insert_pos += 1;
            }
            if !found_hidden {
                lines.insert(insert_pos, format!("Hidden={}", !enabled));
            }
        }

        let new_content = lines.join("\n");
        // Ensure trailing newline
        let new_content = if new_content.ends_with('\n') {
            new_content
        } else {
            format!("{}\n", new_content)
        };

        fs::write(&user_path, new_content)
            .map_err(|e| format!("Cannot write {}: {}", user_path.display(), e))?;

        log::info!(
            "Toggled autostart for '{}' to {} ({})",
            entry.name,
            enabled,
            user_path.display()
        );

        Ok(())
    }

    fn toggle_systemd_user(entry: &StartupEntry, enabled: bool) -> Result<(), String> {
        let action = if enabled { "enable" } else { "disable" };
        let unit = &entry.file_path;

        let output = Command::new("systemctl")
            .args(["--user", action, unit])
            .output()
            .map_err(|e| format!("Failed to run systemctl: {}", e))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(format!(
                "systemctl --user {} {} failed: {}",
                action, unit, stderr
            ));
        }

        log::info!(
            "Toggled systemd user unit '{}' to {}",
            entry.name,
            enabled
        );

        Ok(())
    }
}
