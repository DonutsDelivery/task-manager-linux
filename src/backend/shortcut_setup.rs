use std::fs;
use std::path::PathBuf;
use std::process::Command;

use crate::APP_ID;

const BIN_NAME: &str = "donut-taskmgr";
/// Key used for the GNOME, Cinnamon and MATE custom keybinding entries.
const BINDING_KEY: &str = "donut-taskmgr";

const ICON_SVG: &str = include_str!("../../icons/online.donutsdelivery.DonutTaskManager.svg");

const DESKTOP_ENTRY: &str = "\
[Desktop Entry]
Type=Application
Name=DonutTaskManager
GenericName=System Monitor
Comment=System process manager and performance monitor
Exec={bin_path}
Icon=online.donutsdelivery.DonutTaskManager
Terminal=false
Categories=System;Monitor;GTK;
Keywords=task;process;system;monitor;cpu;memory;gpu;
StartupNotify=true
StartupWMClass=online.donutsdelivery.DonutTaskManager
X-KDE-SubstituteUID=false
X-KDE-Shortcuts=Ctrl+Shift+Esc
";

// Names used by builds before the DonutTaskManager rename. install() removes
// what those builds wrote so the user does not end up with two entries.
const LEGACY_BIN_NAME: &str = "task-manager-linux";
const LEGACY_DESKTOP_FILE: &str = "task-manager.desktop";
const LEGACY_AUTOSTART_FILE: &str = "task-manager-shortcut.desktop";
const LEGACY_BINDING_KEY: &str = "task-manager";

fn home() -> PathBuf {
    dirs::home_dir().expect("no home dir")
}

fn bin_dest() -> PathBuf {
    home().join(".local/bin").join(BIN_NAME)
}

fn desktop_dest() -> PathBuf {
    home()
        .join(".local/share/applications")
        .join(format!("{}.desktop", APP_ID))
}

fn icon_dest() -> PathBuf {
    home()
        .join(".local/share/icons/hicolor/scalable/apps")
        .join(format!("{}.svg", APP_ID))
}

/// Check whether the shortcut is already registered.
pub fn is_installed() -> bool {
    desktop_dest().exists() && bin_dest().exists()
}

/// Install binary, desktop file, and register global shortcut for the detected DE.
/// Returns a user-facing status message.
pub fn install() -> Result<String, String> {
    let current_exe = crate::util::own_executable()
        .map_err(|e| format!("Cannot determine current executable: {}", e))?;

    // Copy binary via temp file + rename to avoid "text file busy" when
    // overwriting the running executable.
    let bin_dst = bin_dest();
    let bin_dir = bin_dst.parent().unwrap();
    fs::create_dir_all(bin_dir)
        .map_err(|e| format!("Failed to create ~/.local/bin: {}", e))?;
    let tmp_dst = bin_dir.join(format!(".{}.tmp", BIN_NAME));
    fs::copy(&current_exe, &tmp_dst)
        .map_err(|e| format!("Failed to copy binary: {}", e))?;

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&tmp_dst, fs::Permissions::from_mode(0o755))
            .map_err(|e| format!("Failed to set binary permissions: {}", e))?;
    }

    fs::rename(&tmp_dst, &bin_dst)
        .map_err(|e| format!("Failed to install binary: {}", e))?;

    // Write desktop file
    let desktop_dst = desktop_dest();
    fs::create_dir_all(desktop_dst.parent().unwrap())
        .map_err(|e| format!("Failed to create applications dir: {}", e))?;
    let content = DESKTOP_ENTRY.replace("{bin_path}", &bin_dst.to_string_lossy());
    fs::write(&desktop_dst, content)
        .map_err(|e| format!("Failed to write desktop file: {}", e))?;

    // Icon for the desktop file. A missing icon is not worth failing over.
    let icon_dst = icon_dest();
    if let Some(parent) = icon_dst.parent() {
        let _ = fs::create_dir_all(parent);
    }
    let _ = fs::write(&icon_dst, ICON_SVG);

    remove_legacy_files();

    // Register shortcut for the detected DE
    let shortcut_result = register_shortcut(&bin_dst);

    let mut msg = "Shortcut installed successfully!".to_string();
    match shortcut_result {
        Ok(note) => msg.push_str(&format!("\n\n{}", note)),
        Err(e) => msg.push_str(&format!("\n\nNote: {}\nYou can set Ctrl+Shift+Esc manually in your desktop settings.", e)),
    }

    Ok(msg)
}

/// Delete the launcher, binary and autostart entry that older builds installed.
fn remove_legacy_files() {
    let _ = fs::remove_file(home().join(".local/share/applications").join(LEGACY_DESKTOP_FILE));
    let _ = fs::remove_file(home().join(".local/bin").join(LEGACY_BIN_NAME));
    let _ = fs::remove_file(home().join(".config/autostart").join(LEGACY_AUTOSTART_FILE));
}

/// Drop one quoted entry from a gsettings string-array value such as
/// `['/a/', '/b/']`.
fn remove_list_entry(list: &str, entry: &str) -> String {
    let quoted = format!("'{}'", entry);
    list.replace(&format!("{}, ", quoted), "")
        .replace(&format!(", {}", quoted), "")
        .replace(&quoted, "")
}

/// Detect DE and register the shortcut using the appropriate method.
fn register_shortcut(bin_path: &std::path::Path) -> Result<String, String> {
    let desktop = std::env::var("XDG_CURRENT_DESKTOP").unwrap_or_default().to_uppercase();

    if desktop.contains("KDE") || desktop.contains("PLASMA") {
        register_kde_shortcut()?;
        Ok("Ctrl+Shift+Esc is configured for KDE.\nLog out and back in for the shortcut to take effect.".into())
    } else if desktop.contains("GNOME") || desktop.contains("UNITY") {
        register_gnome_shortcut(bin_path)?;
        Ok("Ctrl+Shift+Esc is configured for GNOME.\nThe shortcut is active immediately.".into())
    } else if desktop.contains("XFCE") {
        register_xfce_shortcut(bin_path)?;
        Ok("Ctrl+Shift+Esc is configured for XFCE.\nThe shortcut is active immediately.".into())
    } else if desktop.contains("CINNAMON") {
        register_cinnamon_shortcut(bin_path)?;
        Ok("Ctrl+Shift+Esc is configured for Cinnamon.\nThe shortcut is active immediately.".into())
    } else if desktop.contains("MATE") {
        register_mate_shortcut(bin_path)?;
        Ok("Ctrl+Shift+Esc is configured for MATE.\nThe shortcut is active immediately.".into())
    } else {
        // Universal fallback: install evdev-based shortcut daemon via XDG autostart
        install_evdev_daemon(bin_path)?;
        Ok("Ctrl+Shift+Esc is configured via background listener.\nThe listener will start automatically on next login.\nNote: your user must be in the 'input' group.\nRun: sudo usermod -aG input $USER && log out/in".into())
    }
}

/// Plasma 6 ships kwriteconfig6, Plasma 5 only kwriteconfig5.
fn kwriteconfig_command() -> &'static str {
    ["kwriteconfig6", "kwriteconfig5"]
        .into_iter()
        .find(|cmd| Command::new(*cmd).arg("--help").output().is_ok())
        .unwrap_or("kwriteconfig6")
}

fn register_kde_shortcut() -> Result<(), String> {
    let cmd = kwriteconfig_command();

    // Remove the binding the old task-manager.desktop entry registered.
    let _ = Command::new(cmd)
        .args([
            "--file", "kglobalshortcutsrc",
            "--group", "services", "--group", LEGACY_DESKTOP_FILE,
            "--key", "_launch", "--delete",
        ])
        .output();

    Command::new(cmd)
        .args([
            "--file", "kglobalshortcutsrc",
            "--group", "services", "--group", &format!("{}.desktop", APP_ID),
            "--key", "_launch",
            "Ctrl+Shift+Esc,none,DonutTaskManager",
        ])
        .output()
        .map_err(|e| format!("{} not found or failed: {}", cmd, e))?;
    Ok(())
}

fn register_gnome_shortcut(bin_path: &std::path::Path) -> Result<(), String> {
    let base = "/org/gnome/settings-daemon/plugins/media-keys/custom-keybindings";
    let path = format!("{}/{}/", base, BINDING_KEY);
    let legacy_path = format!("{}/{}/", base, LEGACY_BINDING_KEY);
    let schema = "org.gnome.settings-daemon.plugins.media-keys";
    let custom_schema = format!("{}.custom-keybinding:{}", schema, path);

    // Read existing custom keybindings list
    let existing = Command::new("gsettings")
        .args(["get", schema, "custom-keybindings"])
        .output()
        .map_err(|e| format!("gsettings not found: {}", e))?;
    let existing_str = String::from_utf8_lossy(&existing.stdout).trim().to_string();

    // Drop the binding an older build registered under the old path
    let legacy_schema = format!("{}.custom-keybinding:{}", schema, legacy_path);
    let existing_str = remove_list_entry(&existing_str, &legacy_path);
    let _ = Command::new("gsettings")
        .args(["reset-recursively", &legacy_schema])
        .output();

    // Add our path if not already present
    let new_list = if existing_str.contains(&path) {
        existing_str.clone()
    } else if existing_str == "@as []" || existing_str == "[]" || existing_str.is_empty() {
        format!("['{}']", path)
    } else {
        // Insert before the closing bracket
        existing_str.replace(']', &format!(", '{}']", path))
    };

    run_gsettings(&[schema, "custom-keybindings", &new_list])?;
    run_gsettings(&[&custom_schema, "name", "DonutTaskManager"])?;
    run_gsettings(&[&custom_schema, "command", &bin_path.to_string_lossy()])?;
    run_gsettings(&[&custom_schema, "binding", "<Control><Shift>Escape"])?;

    Ok(())
}

fn register_xfce_shortcut(bin_path: &std::path::Path) -> Result<(), String> {
    // xfconf-query for xfce4-keyboard-shortcuts
    Command::new("xfconf-query")
        .args([
            "-c", "xfce4-keyboard-shortcuts",
            "-p", "/commands/custom/<Control><Shift>Escape",
            "-n", "-t", "string",
            "-s", &bin_path.to_string_lossy(),
        ])
        .output()
        .map_err(|e| format!("xfconf-query failed: {}", e))?;
    Ok(())
}

fn register_cinnamon_shortcut(bin_path: &std::path::Path) -> Result<(), String> {
    let schema = "org.cinnamon.desktop.keybindings.custom-keybinding";
    let base = "/org/cinnamon/desktop/keybindings/custom-keybindings";
    let path = format!("{}/{}/", base, BINDING_KEY);
    let legacy_path = format!("{}/{}/", base, LEGACY_BINDING_KEY);
    let custom_schema = format!("{}:{}", schema, path);

    // Read existing list
    let existing = Command::new("gsettings")
        .args(["get", "org.cinnamon.desktop.keybindings", "custom-list"])
        .output()
        .map_err(|e| format!("gsettings not found: {}", e))?;
    let existing_str = String::from_utf8_lossy(&existing.stdout).trim().to_string();

    // Drop the binding an older build registered under the old name
    let legacy_schema = format!("{}:{}", schema, legacy_path);
    let existing_str = remove_list_entry(&existing_str, LEGACY_BINDING_KEY);
    let _ = Command::new("gsettings")
        .args(["reset-recursively", &legacy_schema])
        .output();

    let entry = format!("'{}'", BINDING_KEY);
    let new_list = if existing_str.contains(&entry) {
        existing_str.clone()
    } else if existing_str == "@as []" || existing_str == "[]" || existing_str.is_empty() {
        format!("[{}]", entry)
    } else {
        existing_str.replace(']', &format!(", {}]", entry))
    };

    run_gsettings(&["org.cinnamon.desktop.keybindings", "custom-list", &new_list])?;
    run_gsettings(&[&custom_schema, "name", "DonutTaskManager"])?;
    run_gsettings(&[&custom_schema, "command", &bin_path.to_string_lossy()])?;
    run_gsettings(&[&custom_schema, "binding", "['<Control><Shift>Escape']"])?;

    Ok(())
}

fn register_mate_shortcut(bin_path: &std::path::Path) -> Result<(), String> {
    // MATE uses dconf paths similar to GNOME 2
    let base = format!("/org/mate/desktop/keybindings/{}", BINDING_KEY);

    // Remove the binding an older build registered under the old name
    let _ = Command::new("dconf")
        .args([
            "reset",
            "-f",
            &format!("/org/mate/desktop/keybindings/{}/", LEGACY_BINDING_KEY),
        ])
        .output();

    Command::new("dconf")
        .args([
            "write",
            &format!("{}/action", base),
            &format!("'{}'", bin_path.display()),
        ])
        .output()
        .map_err(|e| format!("dconf not found: {}", e))?;

    Command::new("dconf")
        .args([
            "write",
            &format!("{}/name", base),
            "'DonutTaskManager'",
        ])
        .output()
        .map_err(|e| format!("dconf failed: {}", e))?;

    Command::new("dconf")
        .args([
            "write",
            &format!("{}/binding", base),
            "'<Control><Shift>Escape'",
        ])
        .output()
        .map_err(|e| format!("dconf failed: {}", e))?;

    Ok(())
}

fn autostart_dest() -> PathBuf {
    dirs::home_dir()
        .expect("no home dir")
        .join(format!(".config/autostart/{}-shortcut.desktop", APP_ID))
}

fn install_evdev_daemon(bin_path: &std::path::Path) -> Result<(), String> {
    let autostart_dst = autostart_dest();
    fs::create_dir_all(autostart_dst.parent().unwrap())
        .map_err(|e| format!("Failed to create autostart dir: {}", e))?;

    let content = format!(
        "[Desktop Entry]\n\
         Type=Application\n\
         Name=DonutTaskManager Shortcut Listener\n\
         Comment=Listens for Ctrl+Shift+Escape to launch DonutTaskManager\n\
         Exec={} --shortcut-daemon\n\
         Hidden=false\n\
         NoDisplay=true\n\
         X-GNOME-Autostart-enabled=true\n",
        bin_path.display()
    );

    fs::write(&autostart_dst, content)
        .map_err(|e| format!("Failed to write autostart entry: {}", e))?;

    Ok(())
}

/// Check if the evdev daemon is installed (has autostart entry).
pub fn is_daemon_installed() -> bool {
    autostart_dest().exists()
}

fn run_gsettings(args: &[&str]) -> Result<(), String> {
    let output = Command::new("gsettings")
        .arg("set")
        .args(args)
        .output()
        .map_err(|e| format!("gsettings failed: {}", e))?;
    if !output.status.success() {
        return Err(format!("gsettings error: {}", String::from_utf8_lossy(&output.stderr)));
    }
    Ok(())
}
