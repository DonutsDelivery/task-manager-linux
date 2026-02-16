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

        // Scan systemd user units (only if systemd is available)
        if crate::backend::services::is_systemd_available() {
            Self::scan_systemd_user(&mut entries);
        } else {
            log::info!("systemd not detected, skipping systemd user units scan");
        }

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
        let mut wm_class = None;
        let mut launch_minimized = None;
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
            } else if let Some(val) = line.strip_prefix("StartupWMClass=") {
                wm_class = Some(val.trim().to_string());
            } else if let Some(val) = line.strip_prefix("X-TaskManager-LaunchMinimized=") {
                launch_minimized = Some(val.trim().eq_ignore_ascii_case("true"));
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

        // Fallback wm_class: derive from desktop file basename
        let wm_class = wm_class.unwrap_or_else(|| {
            path.file_stem()
                .unwrap_or_default()
                .to_string_lossy()
                .to_string()
        });

        Some(StartupEntry {
            name,
            comment: comment.unwrap_or_default(),
            exec: exec.unwrap_or_default(),
            icon: icon.unwrap_or_default(),
            enabled,
            launch_minimized: launch_minimized.unwrap_or(false),
            wm_class,
            file_path: path.to_string_lossy().to_string(),
            source: StartupSource::Autostart,
            active_state: String::new(),
        })
    }

    fn scan_systemd_user(entries: &mut Vec<StartupEntry>) {
        // List all user service unit files (enabled, disabled, static)
        let output = match Command::new("systemctl")
            .args([
                "--user",
                "list-unit-files",
                "--type=service",
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

        // Build a map of unit name → active state from list-units
        let active_map = Self::get_user_active_states();

        let stdout = String::from_utf8_lossy(&output.stdout);
        for line in stdout.lines() {
            let parts: Vec<&str> = line.split_whitespace().collect();
            if parts.len() >= 2 {
                let unit_name = parts[0];
                let unit_file_state = parts[1]; // enabled, disabled, static, masked, etc.

                // Skip template units (contain @) and static/masked units
                if unit_name.contains('@') {
                    continue;
                }
                if unit_file_state == "static" || unit_file_state == "masked" || unit_file_state == "indirect" {
                    continue;
                }

                let display_name = unit_name
                    .strip_suffix(".service")
                    .unwrap_or(unit_name)
                    .to_string();

                let description = Self::get_systemd_description(unit_name);
                let enabled = unit_file_state == "enabled";
                let active_state = active_map
                    .get(unit_name)
                    .cloned()
                    .unwrap_or_else(|| "inactive".to_string());

                entries.push(StartupEntry {
                    name: display_name,
                    comment: description,
                    exec: unit_name.to_string(),
                    icon: String::new(),
                    enabled,
                    launch_minimized: false,
                    wm_class: String::new(),
                    file_path: unit_name.to_string(),
                    source: StartupSource::SystemdUser,
                    active_state,
                });
            }
        }
    }

    /// Get a map of unit name → active state for all user services.
    fn get_user_active_states() -> std::collections::HashMap<String, String> {
        let mut map = std::collections::HashMap::new();
        let output = match Command::new("systemctl")
            .args([
                "--user",
                "list-units",
                "--type=service",
                "--all",
                "--no-legend",
                "--no-pager",
            ])
            .output()
        {
            Ok(o) => o,
            Err(_) => return map,
        };
        if !output.status.success() {
            return map;
        }
        let stdout = String::from_utf8_lossy(&output.stdout);
        for line in stdout.lines() {
            let line = line.trim().trim_start_matches('\u{25CF}').trim();
            let fields: Vec<&str> = line.split_whitespace().collect();
            if fields.len() >= 3 {
                // fields: UNIT LOAD ACTIVE SUB DESCRIPTION...
                map.insert(fields[0].to_string(), fields[2].to_string());
            }
        }
        map
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
                .map_err(|e| {
                    if e.kind() == std::io::ErrorKind::ReadOnlyFilesystem {
                        "Cannot create autostart dir: filesystem is read-only (immutable distro?)".to_string()
                    } else {
                        format!("Cannot create autostart dir: {}", e)
                    }
                })?;
            let dest = user_dir.join(
                path.file_name()
                    .ok_or_else(|| "Invalid file path".to_string())?,
            );
            if !dest.exists() {
                fs::copy(path, &dest)
                    .map_err(|e| {
                        if e.kind() == std::io::ErrorKind::ReadOnlyFilesystem {
                            "Cannot copy desktop file: filesystem is read-only (immutable distro?)".to_string()
                        } else {
                            format!("Cannot copy desktop file: {}", e)
                        }
                    })?;
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
            .map_err(|e| {
                if e.kind() == std::io::ErrorKind::ReadOnlyFilesystem {
                    format!("Cannot write {}: filesystem is read-only (immutable distro?)", user_path.display())
                } else {
                    format!("Cannot write {}: {}", user_path.display(), e)
                }
            })?;

        log::info!(
            "Toggled autostart for '{}' to {} ({})",
            entry.name,
            enabled,
            user_path.display()
        );

        Ok(())
    }

    fn toggle_systemd_user(entry: &StartupEntry, enabled: bool) -> Result<(), String> {
        if !crate::backend::services::is_systemd_available() {
            return Err("systemd not available on this system".to_string());
        }

        let action = if enabled { "enable" } else { "disable" };
        let unit = &entry.file_path;

        let output = Command::new("systemctl")
            .args(["--user", action, unit])
            .output()
            .map_err(|e| format!("Failed to run systemctl: {}", e))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            let stderr_str = stderr.trim();

            // Check for read-only filesystem errors (immutable distros)
            if stderr_str.contains("Read-only file system") {
                return Err("Cannot modify: filesystem is read-only (immutable distro?)".to_string());
            }

            return Err(format!(
                "systemctl --user {} {} failed: {}",
                action, unit, stderr_str
            ));
        }

        log::info!(
            "Toggled systemd user unit '{}' to {}",
            entry.name,
            enabled
        );

        Ok(())
    }

    /// Start/stop/restart a systemd user service.
    pub fn service_action(entry: &StartupEntry, action: &str) -> Result<(), String> {
        if entry.source != StartupSource::SystemdUser {
            return Err("Not a systemd service".to_string());
        }

        if !crate::backend::services::is_systemd_available() {
            return Err("systemd not available on this system".to_string());
        }

        let valid_actions = ["start", "stop", "restart"];
        if !valid_actions.contains(&action) {
            return Err(format!("Invalid action: {}", action));
        }

        let unit = &entry.file_path;
        let output = Command::new("systemctl")
            .args(["--user", action, unit])
            .output()
            .map_err(|e| format!("Failed to run systemctl: {}", e))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            let stderr_str = stderr.trim();

            // Check for read-only filesystem errors (immutable distros)
            if stderr_str.contains("Read-only file system") {
                return Err("Cannot modify: filesystem is read-only (immutable distro?)".to_string());
            }

            return Err(format!(
                "systemctl --user {} {} failed: {}",
                action, unit, stderr_str
            ));
        }

        log::info!("Service action '{}' on '{}' succeeded", action, entry.name);
        Ok(())
    }

    pub fn toggle_launch_mode(entry: &StartupEntry, minimized: bool) -> Result<(), String> {
        if entry.source != StartupSource::Autostart {
            return Ok(()); // systemd services don't have windows
        }

        // Update the desktop file with our custom key
        Self::set_desktop_key(&entry.file_path, "X-TaskManager-LaunchMinimized", if minimized { "true" } else { "false" })?;

        // Manage KWin window rule
        if !entry.wm_class.is_empty() {
            if minimized {
                kwin_rules::add_minimize_rule(&entry.name, &entry.wm_class)?;
            } else {
                kwin_rules::remove_minimize_rule(&entry.wm_class)?;
            }
            kwin_rules::reconfigure();
        }

        log::info!(
            "Set launch mode for '{}' to {} (wmclass: {})",
            entry.name,
            if minimized { "Background" } else { "Normal" },
            entry.wm_class
        );

        Ok(())
    }

    fn set_desktop_key(file_path: &str, key: &str, value: &str) -> Result<(), String> {
        let path = Path::new(file_path);

        // If the file is in /etc/xdg/autostart, copy to user dir first
        let user_path = if file_path.starts_with("/etc/xdg/autostart") {
            let home = std::env::var("HOME").map_err(|e| format!("Cannot get HOME: {}", e))?;
            let user_dir = PathBuf::from(format!("{}/.config/autostart", home));
            fs::create_dir_all(&user_dir)
                .map_err(|e| {
                    if e.kind() == std::io::ErrorKind::ReadOnlyFilesystem {
                        "Cannot create autostart dir: filesystem is read-only (immutable distro?)".to_string()
                    } else {
                        format!("Cannot create autostart dir: {}", e)
                    }
                })?;
            let dest = user_dir.join(
                path.file_name()
                    .ok_or_else(|| "Invalid file path".to_string())?,
            );
            if !dest.exists() {
                fs::copy(path, &dest)
                    .map_err(|e| {
                        if e.kind() == std::io::ErrorKind::ReadOnlyFilesystem {
                            "Cannot copy desktop file: filesystem is read-only (immutable distro?)".to_string()
                        } else {
                            format!("Cannot copy desktop file: {}", e)
                        }
                    })?;
            }
            dest
        } else {
            path.to_path_buf()
        };

        let content = fs::read_to_string(&user_path)
            .map_err(|e| format!("Cannot read {}: {}", user_path.display(), e))?;

        let mut lines: Vec<String> = content.lines().map(|l| l.to_string()).collect();
        let prefix = format!("{}=", key);
        let entry_line = format!("{}={}", key, value);
        let mut found = false;

        for line in lines.iter_mut() {
            if line.starts_with(&prefix) {
                *line = entry_line.clone();
                found = true;
                break;
            }
        }

        if !found {
            // Insert before the next section or at end of [Desktop Entry]
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
            lines.insert(insert_pos, entry_line);
        }

        let new_content = lines.join("\n");
        let new_content = if new_content.ends_with('\n') {
            new_content
        } else {
            format!("{}\n", new_content)
        };

        fs::write(&user_path, new_content)
            .map_err(|e| {
                if e.kind() == std::io::ErrorKind::ReadOnlyFilesystem {
                    format!("Cannot write {}: filesystem is read-only (immutable distro?)", user_path.display())
                } else {
                    format!("Cannot write {}: {}", user_path.display(), e)
                }
            })?;

        Ok(())
    }
}

/// Manages KWin window rules in ~/.config/kwinrulesrc for launch-minimized behavior.
mod kwin_rules {
    use std::fs;
    use std::path::PathBuf;
    use std::process::Command;

    fn config_path() -> PathBuf {
        dirs::config_dir()
            .unwrap_or_else(|| PathBuf::from(
                format!("{}/.config", std::env::var("HOME").unwrap_or_default())
            ))
            .join("kwinrulesrc")
    }

    /// Section name for our managed rules: deterministic from wm_class
    fn section_name(wm_class: &str) -> String {
        format!("taskmgr-minimize-{}", wm_class.to_lowercase().replace(' ', "-"))
    }

    pub fn add_minimize_rule(app_name: &str, wm_class: &str) -> Result<(), String> {
        let path = config_path();
        let content = fs::read_to_string(&path).unwrap_or_default();
        let mut config = KwinConfig::parse(&content);
        let sec = section_name(wm_class);

        // Remove existing rule for this wmclass if any
        config.remove_section(&sec);

        // Add the new rule section
        config.add_section(&sec, &[
            ("Description", &format!("TaskMgr: Start {} minimized", app_name)),
            ("wmclass", wm_class),
            ("wmclassmatch", "1"),
            ("wmclasscomplete", "false"),
            ("minimize", "true"),
            ("minimizerule", "2"), // Apply Initially
        ]);

        // Update [General] rules list and count
        config.register_rule(&sec);

        fs::write(&path, config.to_string())
            .map_err(|e| format!("Cannot write kwinrulesrc: {}", e))?;

        Ok(())
    }

    pub fn remove_minimize_rule(wm_class: &str) -> Result<(), String> {
        let path = config_path();
        let content = fs::read_to_string(&path).unwrap_or_default();
        let mut config = KwinConfig::parse(&content);
        let sec = section_name(wm_class);

        config.unregister_rule(&sec);
        config.remove_section(&sec);

        fs::write(&path, config.to_string())
            .map_err(|e| format!("Cannot write kwinrulesrc: {}", e))?;

        Ok(())
    }

    pub fn reconfigure() {
        // Try qdbus6 first (KDE 6), fall back to qdbus
        let result = Command::new("qdbus6")
            .args(["org.kde.KWin", "/KWin", "reconfigure"])
            .output();
        if result.is_err() {
            let _ = Command::new("qdbus")
                .args(["org.kde.KWin", "/KWin", "reconfigure"])
                .output();
        }
    }

    struct KwinConfig {
        sections: Vec<(String, Vec<(String, String)>)>,
    }

    impl KwinConfig {
        fn parse(content: &str) -> Self {
            let mut sections = Vec::new();
            let mut current_name = String::new();
            let mut current_kvs: Vec<(String, String)> = Vec::new();

            for line in content.lines() {
                let trimmed = line.trim();
                if trimmed.starts_with('[') && trimmed.ends_with(']') {
                    if !current_name.is_empty() {
                        sections.push((current_name, current_kvs));
                    }
                    current_name = trimmed[1..trimmed.len() - 1].to_string();
                    current_kvs = Vec::new();
                } else if let Some(eq_pos) = trimmed.find('=') {
                    let key = trimmed[..eq_pos].to_string();
                    let val = trimmed[eq_pos + 1..].to_string();
                    current_kvs.push((key, val));
                }
                // Skip blank lines / comments
            }
            if !current_name.is_empty() {
                sections.push((current_name, current_kvs));
            }

            Self { sections }
        }

        fn to_string(&self) -> String {
            let mut result = String::new();
            for (name, kvs) in &self.sections {
                result.push_str(&format!("[{}]\n", name));
                for (k, v) in kvs {
                    result.push_str(&format!("{}={}\n", k, v));
                }
                result.push('\n');
            }
            result
        }

        fn find_section_mut(&mut self, name: &str) -> Option<&mut Vec<(String, String)>> {
            self.sections.iter_mut()
                .find(|(n, _)| n == name)
                .map(|(_, kvs)| kvs)
        }

        fn remove_section(&mut self, name: &str) {
            self.sections.retain(|(n, _)| n != name);
        }

        fn add_section(&mut self, name: &str, entries: &[(&str, &str)]) {
            let kvs: Vec<(String, String)> = entries
                .iter()
                .map(|(k, v)| (k.to_string(), v.to_string()))
                .collect();
            self.sections.push((name.to_string(), kvs));
        }

        fn get_value(&self, section: &str, key: &str) -> Option<&str> {
            self.sections.iter()
                .find(|(n, _)| n == section)
                .and_then(|(_, kvs)| {
                    kvs.iter().find(|(k, _)| k == key).map(|(_, v)| v.as_str())
                })
        }

        fn set_value(&mut self, section: &str, key: &str, value: &str) {
            if let Some(kvs) = self.find_section_mut(section) {
                if let Some(kv) = kvs.iter_mut().find(|(k, _)| k == key) {
                    kv.1 = value.to_string();
                } else {
                    kvs.push((key.to_string(), value.to_string()));
                }
            } else {
                self.sections.push((
                    section.to_string(),
                    vec![(key.to_string(), value.to_string())],
                ));
            }
        }

        fn register_rule(&mut self, rule_name: &str) {
            let rules = self.get_value("General", "rules")
                .unwrap_or("")
                .to_string();
            let mut rule_list: Vec<String> = if rules.is_empty() {
                Vec::new()
            } else {
                rules.split(',').map(|s| s.to_string()).collect()
            };

            if !rule_list.iter().any(|r| r == rule_name) {
                rule_list.push(rule_name.to_string());
            }

            let count = rule_list.len();
            self.set_value("General", "rules", &rule_list.join(","));
            self.set_value("General", "count", &count.to_string());
        }

        fn unregister_rule(&mut self, rule_name: &str) {
            let rules = self.get_value("General", "rules")
                .unwrap_or("")
                .to_string();
            let mut rule_list: Vec<String> = if rules.is_empty() {
                Vec::new()
            } else {
                rules.split(',').map(|s| s.to_string()).collect()
            };

            rule_list.retain(|r| r != rule_name);
            let count = rule_list.len();
            self.set_value("General", "rules", &rule_list.join(","));
            self.set_value("General", "count", &count.to_string());
        }
    }
}
