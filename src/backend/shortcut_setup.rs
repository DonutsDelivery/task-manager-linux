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
Icon=utilities-system-monitor
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

/// Install binary, desktop file, and register KDE global shortcut.
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

    // Register KDE shortcut if kwriteconfig6 is available
    let kde_result = register_kde_shortcut();

    let mut msg = "Shortcut installed successfully!".to_string();
    match kde_result {
        Ok(()) => msg.push_str("\n\nCtrl+Shift+Esc is configured.\nLog out and back in for the shortcut to take effect."),
        Err(e) => msg.push_str(&format!("\n\nNote: {}\nYou can set Ctrl+Shift+Esc manually in System Settings → Shortcuts.", e)),
    }

    Ok(msg)
}

fn register_kde_shortcut() -> Result<(), String> {
    // Use kwriteconfig6 to write shortcut config. This is safe and reliable.
    // The shortcut takes effect after kwin restarts (next login).
    Command::new("which")
        .arg("kwriteconfig6")
        .output()
        .ok()
        .filter(|o| o.status.success())
        .ok_or_else(|| "kwriteconfig6 not found (not KDE?)".to_string())?;

    Command::new("kwriteconfig6")
        .args([
            "--file", "kglobalshortcutsrc",
            "--group", "services", "--group", "task-manager.desktop",
            "--key", "_launch",
            "Ctrl+Shift+Esc,none,Task Manager",
        ])
        .status()
        .map_err(|e| format!("kwriteconfig6 failed: {}", e))?;

    Ok(())
}
