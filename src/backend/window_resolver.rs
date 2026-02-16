use std::collections::HashMap;
use std::fs;
use x11rb::connection::Connection;
use x11rb::protocol::xproto::*;

enum ResolverBackend {
    X11 {
        conn: x11rb::rust_connection::RustConnection,
        root: u32,
    },
    Wayland,
    None,
}

pub struct WindowResolver {
    backend: ResolverBackend,
}

impl WindowResolver {
    pub fn new() -> Self {
        // Detect session type
        let is_wayland = std::env::var("XDG_SESSION_TYPE")
            .map(|v| v == "wayland")
            .unwrap_or(false)
            || std::env::var("WAYLAND_DISPLAY").is_ok();

        // Try X11 first (even on Wayland, XWayland may be available)
        match x11rb::connect(None) {
            Ok((conn, screen_num)) => {
                let root = conn.setup().roots[screen_num].root;
                log::info!("X11 connection established for window resolver");
                Self {
                    backend: ResolverBackend::X11 { conn, root },
                }
            }
            Err(e) => {
                if is_wayland {
                    log::info!("X11 unavailable on Wayland session, using /proc-based fallback");
                    Self {
                        backend: ResolverBackend::Wayland,
                    }
                } else {
                    log::warn!("Failed to connect to X11: {} — window titles unavailable", e);
                    Self {
                        backend: ResolverBackend::None,
                    }
                }
            }
        }
    }

    pub fn collect(&self) -> HashMap<u32, String> {
        match &self.backend {
            ResolverBackend::X11 { conn, root } => self.collect_x11(conn, *root),
            ResolverBackend::Wayland => self.collect_wayland(),
            ResolverBackend::None => HashMap::new(),
        }
    }

    fn collect_x11(&self, conn: &x11rb::rust_connection::RustConnection, root: u32) -> HashMap<u32, String> {
        let mut map = HashMap::new();

        // Get _NET_CLIENT_LIST
        let atom_client_list = match intern_atom(conn, "_NET_CLIENT_LIST") {
            Some(a) => a,
            None => return map,
        };

        let reply = match conn.get_property(false, root, atom_client_list, AtomEnum::WINDOW, 0, 1024) {
            Ok(cookie) => match cookie.reply() {
                Ok(r) => r,
                Err(_) => return map,
            },
            Err(_) => return map,
        };

        let windows: Vec<u32> = reply.value32().map(|iter| iter.collect()).unwrap_or_default();

        let atom_pid = intern_atom(conn, "_NET_WM_PID");
        let atom_name = intern_atom(conn, "_NET_WM_NAME");
        let atom_utf8 = intern_atom(conn, "UTF8_STRING");

        for win in windows {
            let pid = get_window_pid(conn, win, atom_pid);
            let title = get_window_title(conn, win, atom_name, atom_utf8);

            if let (Some(pid), Some(title)) = (pid, title) {
                if !title.is_empty() {
                    // Use the shorter part before " — " or " - " for cleaner names
                    let clean = clean_window_title(&title);
                    map.insert(pid, clean);
                }
            }
        }

        map
    }

    fn collect_wayland(&self) -> HashMap<u32, String> {
        let mut map = HashMap::new();

        // Scan /proc for GUI processes
        let proc_dir = match fs::read_dir("/proc") {
            Ok(dir) => dir,
            Err(_) => return map,
        };

        for entry in proc_dir.flatten() {
            let file_name = entry.file_name();
            let name = file_name.to_string_lossy();

            // Only process numeric directories (PIDs)
            if let Ok(pid) = name.parse::<u32>() {
                // Check if this is a GUI process by reading its environment
                if is_wayland_gui_process(pid) {
                    // Read comm as a basic window title
                    if let Some(title) = read_proc_comm(pid) {
                        map.insert(pid, title);
                    }
                }
            }
        }

        map
    }
}

fn intern_atom(conn: &x11rb::rust_connection::RustConnection, name: &str) -> Option<u32> {
    conn.intern_atom(false, name.as_bytes())
        .ok()?
        .reply()
        .ok()
        .map(|r| r.atom)
}

fn get_window_pid(
    conn: &x11rb::rust_connection::RustConnection,
    window: u32,
    atom_pid: Option<u32>,
) -> Option<u32> {
    let atom = atom_pid?;
    let reply = conn
        .get_property(false, window, atom, AtomEnum::CARDINAL, 0, 1)
        .ok()?
        .reply()
        .ok()?;
    reply.value32().and_then(|mut iter| iter.next())
}

fn get_window_title(
    conn: &x11rb::rust_connection::RustConnection,
    window: u32,
    atom_name: Option<u32>,
    atom_utf8: Option<u32>,
) -> Option<String> {
    // Try _NET_WM_NAME (UTF-8) first
    if let (Some(name_atom), Some(utf8_atom)) = (atom_name, atom_utf8) {
        if let Ok(cookie) = conn.get_property(false, window, name_atom, utf8_atom, 0, 256) {
            if let Ok(reply) = cookie.reply() {
                if reply.value_len > 0 {
                    return Some(String::from_utf8_lossy(&reply.value).to_string());
                }
            }
        }
    }

    // Fallback to WM_NAME
    if let Ok(cookie) = conn.get_property(false, window, AtomEnum::WM_NAME, AtomEnum::STRING, 0, 256) {
        if let Ok(reply) = cookie.reply() {
            if reply.value_len > 0 {
                return Some(String::from_utf8_lossy(&reply.value).to_string());
            }
        }
    }

    None
}

fn clean_window_title(title: &str) -> String {
    // For most apps, take the application name part
    // e.g. "Document.txt - Firefox" -> "Firefox"
    // e.g. "Terminal — fish" -> "Terminal"
    if let Some(pos) = title.rfind(" — ").or_else(|| title.rfind(" - ")) {
        let after = title[pos + 3..].trim();
        if !after.is_empty() {
            return after.to_string();
        }
    }
    title.to_string()
}

fn is_wayland_gui_process(pid: u32) -> bool {
    let environ_path = format!("/proc/{}/environ", pid);
    let environ_data = match fs::read(&environ_path) {
        Ok(data) => data,
        Err(_) => return false,
    };

    // Environment variables are null-separated
    let env_str = String::from_utf8_lossy(&environ_data);
    for var in env_str.split('\0') {
        if var == "GDK_BACKEND=wayland" || var.starts_with("WAYLAND_DISPLAY=") {
            return true;
        }
    }

    // Also check for common GUI indicators
    // Many GUI apps won't have explicit WAYLAND_DISPLAY in environ but will have DISPLAY
    // and be running under Wayland compositor
    for var in env_str.split('\0') {
        if var.starts_with("DISPLAY=") {
            // It has a display, likely a GUI app
            // Check if cmdline suggests GUI (has common GUI toolkit names)
            if let Some(cmdline) = read_proc_cmdline(pid) {
                let lower = cmdline.to_lowercase();
                if lower.contains("gtk") || lower.contains("qt") || lower.contains("electron")
                    || lower.contains("firefox") || lower.contains("chrome") {
                    return true;
                }
            }
        }
    }

    false
}

fn read_proc_comm(pid: u32) -> Option<String> {
    let comm_path = format!("/proc/{}/comm", pid);
    fs::read_to_string(&comm_path)
        .ok()
        .map(|s| s.trim().to_string())
}

fn read_proc_cmdline(pid: u32) -> Option<String> {
    let cmdline_path = format!("/proc/{}/cmdline", pid);
    fs::read_to_string(&cmdline_path)
        .ok()
        .map(|s| s.replace('\0', " ").trim().to_string())
}
