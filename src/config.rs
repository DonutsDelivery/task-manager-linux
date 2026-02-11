use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    pub window_width: i32,
    pub window_height: i32,
    pub refresh_interval_ms: u64,
    pub visible_columns: Vec<String>,
    pub sort_column: String,
    pub sort_ascending: bool,
    pub show_all_processes: bool,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            window_width: 1200,
            window_height: 800,
            refresh_interval_ms: 1000,
            visible_columns: vec![
                "name".into(),
                "pid".into(),
                "cpu".into(),
                "memory".into(),
                "vram".into(),
                "disk_read".into(),
                "disk_write".into(),
                "state".into(),
            ],
            sort_column: "cpu".into(),
            sort_ascending: false,
            show_all_processes: true,
        }
    }
}

impl Config {
    pub fn load() -> Self {
        let path = config_path();
        if let Ok(data) = fs::read_to_string(&path) {
            serde_json::from_str(&data).unwrap_or_default()
        } else {
            Config::default()
        }
    }

    pub fn save(&self) {
        let path = config_path();
        if let Some(parent) = path.parent() {
            let _ = fs::create_dir_all(parent);
        }
        if let Ok(data) = serde_json::to_string_pretty(self) {
            let _ = fs::write(&path, data);
        }
    }
}

fn config_path() -> PathBuf {
    dirs::config_dir()
        .unwrap_or_else(|| PathBuf::from("~/.config"))
        .join("task-manager-linux")
        .join("config.json")
}
