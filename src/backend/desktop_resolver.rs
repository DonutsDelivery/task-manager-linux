use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;

pub struct DesktopResolver {
    /// Map from executable basename (lowercase) -> human-readable app name
    name_map: HashMap<String, String>,
}

impl DesktopResolver {
    pub fn new() -> Self {
        let mut resolver = Self {
            name_map: HashMap::new(),
        };
        resolver.scan();
        resolver
    }

    fn scan(&mut self) {
        let dirs = desktop_entry_dirs();
        for dir in dirs {
            if let Ok(entries) = fs::read_dir(&dir) {
                for entry in entries.flatten() {
                    let path = entry.path();
                    if path.extension().map(|e| e == "desktop").unwrap_or(false) {
                        self.parse_desktop_file(&path);
                    }
                }
            }
        }
        log::info!("Desktop resolver loaded {} entries", self.name_map.len());
    }

    fn parse_desktop_file(&mut self, path: &std::path::Path) {
        let content = match fs::read_to_string(path) {
            Ok(c) => c,
            Err(_) => return,
        };

        let mut name = None;
        let mut exec = None;
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
            } else if let Some(val) = line.strip_prefix("Exec=") {
                exec = Some(val.to_string());
            }
        }

        if let (Some(name), Some(exec)) = (name, exec) {
            // Extract executable basename from Exec line
            let exec_cmd = exec.split_whitespace().next().unwrap_or("");
            let basename = exec_cmd.rsplit('/').next().unwrap_or(exec_cmd);
            // Remove common wrappers
            let basename = basename
                .strip_prefix("env ")
                .unwrap_or(basename)
                .trim();

            if !basename.is_empty() && !name.is_empty() {
                self.name_map.insert(basename.to_string(), name.clone());
                self.name_map.insert(basename.to_lowercase(), name);
            }
        }
    }

    pub fn names(&self) -> &HashMap<String, String> {
        &self.name_map
    }
}

fn desktop_entry_dirs() -> Vec<PathBuf> {
    let mut dirs = vec![
        PathBuf::from("/usr/share/applications"),
        PathBuf::from("/usr/local/share/applications"),
    ];

    if let Ok(home) = std::env::var("HOME") {
        dirs.push(PathBuf::from(format!("{}/.local/share/applications", home)));
    }

    if let Ok(xdg) = std::env::var("XDG_DATA_DIRS") {
        for dir in xdg.split(':') {
            dirs.push(PathBuf::from(format!("{}/applications", dir)));
        }
    }

    // Flatpak and Snap locations
    dirs.push(PathBuf::from("/var/lib/flatpak/exports/share/applications"));
    dirs.push(PathBuf::from("/var/lib/snapd/desktop/applications"));

    dirs
}
