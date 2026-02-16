#[derive(Debug, Clone)]
pub struct StartupEntry {
    pub name: String,
    pub comment: String,
    pub exec: String,
    pub icon: String,
    pub enabled: bool,
    pub launch_minimized: bool,
    pub wm_class: String,
    pub file_path: String,
    pub source: StartupSource,
    /// For systemd services: "active", "inactive", "failed"; empty for autostart entries
    pub active_state: String,
}

#[derive(Debug, Clone, PartialEq)]
pub enum StartupSource {
    Autostart,
    SystemdUser,
}

impl std::fmt::Display for StartupSource {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            StartupSource::Autostart => write!(f, "Autostart"),
            StartupSource::SystemdUser => write!(f, "Systemd"),
        }
    }
}
