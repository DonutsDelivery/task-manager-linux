use std::collections::HashMap;
use x11rb::connection::Connection;
use x11rb::protocol::xproto::*;

pub struct WindowResolver {
    /// X11 connection (native X11 session or XWayland). Native Wayland windows
    /// never appear here.
    x11: Option<(x11rb::rust_connection::RustConnection, u32)>,
}

impl WindowResolver {
    pub fn new() -> Self {
        // Try X11 first, even on Wayland: XWayland may be available.
        match x11rb::connect(None) {
            Ok((conn, screen_num)) => {
                let root = conn.setup().roots[screen_num].root;
                log::info!("X11 connection established for window resolver");
                Self {
                    x11: Some((conn, root)),
                }
            }
            Err(e) => {
                log::info!("X11 unavailable ({}), window titles come from .desktop entries and /proc", e);
                Self { x11: None }
            }
        }
    }

    /// Titles of windows that X11 knows about, keyed by PID.
    ///
    /// Processes without an X11 window (native Wayland clients, or any process
    /// when X11 is unavailable) are left out on purpose. The caller then falls
    /// back to the .desktop entry name and finally the /proc comm name. Filling
    /// them in here with the comm name would hide the .desktop name.
    pub fn collect(&self) -> HashMap<u32, String> {
        match &self.x11 {
            Some((conn, root)) => self.collect_x11(conn, *root),
            None => HashMap::new(),
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
    let found = title
        .rfind(" — ")
        .map(|pos| (pos, " — ".len()))
        .or_else(|| title.rfind(" - ").map(|pos| (pos, " - ".len())));
    if let Some((pos, sep_len)) = found {
        let after = title[pos + sep_len..].trim();
        if !after.is_empty() {
            return after.to_string();
        }
    }
    title.to_string()
}
