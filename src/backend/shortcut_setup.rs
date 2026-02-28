use std::fs;
use std::path::PathBuf;
use std::process::Command;

const DESKTOP_ENTRY: &str = "\
[Desktop Entry]
Type=Application
Name=Task Manager
GenericName=System Monitor
Comment=System process manager and performance monitor
Exec={bin_path}
Icon=task-manager-linux
Terminal=false
Categories=System;Monitor;
Keywords=task;process;system;monitor;cpu;memory;gpu;
StartupNotify=true
X-KDE-SubstituteUID=false
X-KDE-Shortcuts=Ctrl+Shift+Esc
";

fn bin_dest() -> PathBuf {
    dirs::home_dir()
        .expect("no home dir")
        .join(".local/bin/task-manager-linux")
}

fn desktop_dest() -> PathBuf {
    dirs::home_dir()
        .expect("no home dir")
        .join(".local/share/applications/task-manager.desktop")
}

/// Check whether the shortcut is already registered.
pub fn is_installed() -> bool {
    desktop_dest().exists() && bin_dest().exists()
}

/// Install binary, desktop file, and register global shortcut for the detected DE.
/// Returns a user-facing status message.
pub fn install() -> Result<String, String> {
    let current_exe = std::env::current_exe()
        .map_err(|e| format!("Cannot determine current executable: {}", e))?;

    // Copy binary via temp file + rename to avoid "text file busy" when
    // overwriting the running executable.
    let bin_dst = bin_dest();
    let bin_dir = bin_dst.parent().unwrap();
    fs::create_dir_all(bin_dir)
        .map_err(|e| format!("Failed to create ~/.local/bin: {}", e))?;
    let tmp_dst = bin_dir.join(".task-manager-linux.tmp");
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

    // Register shortcut for the detected DE
    let shortcut_result = register_shortcut(&bin_dst);

    let mut msg = "Shortcut installed successfully!".to_string();
    match shortcut_result {
        Ok(note) => msg.push_str(&format!("\n\n{}", note)),
        Err(e) => msg.push_str(&format!("\n\nNote: {}\nYou can set Ctrl+Shift+Esc manually in your desktop settings.", e)),
    }

    Ok(msg)
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

fn register_kde_shortcut() -> Result<(), String> {
    Command::new("kwriteconfig6")
        .args([
            "--file", "kglobalshortcutsrc",
            "--group", "services", "--group", "task-manager.desktop",
            "--key", "_launch",
            "Ctrl+Shift+Esc,none,Task Manager",
        ])
        .output()
        .map_err(|e| format!("kwriteconfig6 not found or failed: {}", e))?;
    Ok(())
}

fn register_gnome_shortcut(bin_path: &std::path::Path) -> Result<(), String> {
    let path = "/org/gnome/settings-daemon/plugins/media-keys/custom-keybindings/task-manager/";
    let schema = "org.gnome.settings-daemon.plugins.media-keys";
    let custom_schema = format!("{}.custom-keybinding:{}", schema, path);

    // Read existing custom keybindings list
    let existing = Command::new("gsettings")
        .args(["get", schema, "custom-keybindings"])
        .output()
        .map_err(|e| format!("gsettings not found: {}", e))?;
    let existing_str = String::from_utf8_lossy(&existing.stdout).trim().to_string();

    // Add our path if not already present
    let new_list = if existing_str.contains(path) {
        existing_str.clone()
    } else if existing_str == "@as []" || existing_str.is_empty() {
        format!("['{}']", path)
    } else {
        // Insert before the closing bracket
        existing_str.replace(']', &format!(", '{}']", path))
    };

    run_gsettings(&[schema, "custom-keybindings", &new_list])?;
    run_gsettings(&[&custom_schema, "name", "Task Manager"])?;
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
    let path = "/org/cinnamon/desktop/keybindings/custom-keybindings/task-manager/";
    let custom_schema = format!("{}:{}", schema, path);

    // Read existing list
    let existing = Command::new("gsettings")
        .args(["get", "org.cinnamon.desktop.keybindings", "custom-list"])
        .output()
        .map_err(|e| format!("gsettings not found: {}", e))?;
    let existing_str = String::from_utf8_lossy(&existing.stdout).trim().to_string();

    let entry = "'task-manager'";
    let new_list = if existing_str.contains("task-manager") {
        existing_str.clone()
    } else if existing_str == "@as []" || existing_str.is_empty() {
        format!("[{}]", entry)
    } else {
        existing_str.replace(']', &format!(", {}]", entry))
    };

    run_gsettings(&["org.cinnamon.desktop.keybindings", "custom-list", &new_list])?;
    run_gsettings(&[&custom_schema, "name", "Task Manager"])?;
    run_gsettings(&[&custom_schema, "command", &bin_path.to_string_lossy()])?;
    run_gsettings(&[&custom_schema, "binding", "['<Control><Shift>Escape']"])?;

    Ok(())
}

fn register_mate_shortcut(bin_path: &std::path::Path) -> Result<(), String> {
    // MATE uses dconf paths similar to GNOME 2
    Command::new("dconf")
        .args([
            "write",
            "/org/mate/desktop/keybindings/task-manager/action",
            &format!("'{}'", bin_path.display()),
        ])
        .output()
        .map_err(|e| format!("dconf not found: {}", e))?;

    Command::new("dconf")
        .args([
            "write",
            "/org/mate/desktop/keybindings/task-manager/name",
            "'Task Manager'",
        ])
        .output()
        .map_err(|e| format!("dconf failed: {}", e))?;

    Command::new("dconf")
        .args([
            "write",
            "/org/mate/desktop/keybindings/task-manager/binding",
            "'<Control><Shift>Escape'",
        ])
        .output()
        .map_err(|e| format!("dconf failed: {}", e))?;

    Ok(())
}

fn autostart_dest() -> PathBuf {
    dirs::home_dir()
        .expect("no home dir")
        .join(".config/autostart/task-manager-shortcut.desktop")
}

fn install_evdev_daemon(bin_path: &std::path::Path) -> Result<(), String> {
    let autostart_dst = autostart_dest();
    fs::create_dir_all(autostart_dst.parent().unwrap())
        .map_err(|e| format!("Failed to create autostart dir: {}", e))?;

    let content = format!(
        "[Desktop Entry]\n\
         Type=Application\n\
         Name=Task Manager Shortcut Listener\n\
         Comment=Listens for Ctrl+Shift+Escape to launch Task Manager\n\
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
