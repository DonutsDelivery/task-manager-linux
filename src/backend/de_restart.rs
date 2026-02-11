use std::process::Command;

/// A single restart command for a DE component.
pub struct RestartCommand {
    /// Human-readable label (e.g. "Restart Plasma Shell")
    pub label: String,
    /// The program and args to execute
    pub program: String,
    pub args: Vec<String>,
}

/// Detected desktop environment info.
pub struct DesktopEnv {
    pub name: String,
    pub session_type: String,
    pub commands: Vec<RestartCommand>,
}

/// Detect the running DE and return available restart commands.
pub fn detect() -> Option<DesktopEnv> {
    let desktop = std::env::var("XDG_CURRENT_DESKTOP").unwrap_or_default().to_uppercase();
    let session = std::env::var("XDG_SESSION_TYPE").unwrap_or_default().to_lowercase();

    let (name, commands) = if desktop.contains("KDE") || desktop.contains("PLASMA") {
        let kwin = if session == "wayland" { "kwin_wayland" } else { "kwin_x11" };
        (
            "KDE Plasma".to_string(),
            vec![
                RestartCommand {
                    label: "Restart Plasma Shell".to_string(),
                    program: "plasmashell".to_string(),
                    args: vec!["--replace".to_string()],
                },
                RestartCommand {
                    label: format!("Restart KWin ({})", if session == "wayland" { "Wayland" } else { "X11" }),
                    program: kwin.to_string(),
                    args: vec!["--replace".to_string()],
                },
            ],
        )
    } else if desktop.contains("GNOME") {
        (
            "GNOME".to_string(),
            vec![RestartCommand {
                label: "Restart GNOME Shell".to_string(),
                program: "busctl".to_string(),
                args: vec![
                    "--user".to_string(),
                    "call".to_string(),
                    "org.gnome.Shell".to_string(),
                    "/org/gnome/Shell".to_string(),
                    "org.gnome.Shell".to_string(),
                    "Eval".to_string(),
                    "s".to_string(),
                    "Meta.restart(\"Restarting…\")".to_string(),
                ],
            }],
        )
    } else if desktop.contains("XFCE") {
        (
            "XFCE".to_string(),
            vec![RestartCommand {
                label: "Restart XFCE Panel".to_string(),
                program: "xfce4-panel".to_string(),
                args: vec!["--restart".to_string()],
            }],
        )
    } else if desktop.contains("X-CINNAMON") || desktop.contains("CINNAMON") {
        (
            "Cinnamon".to_string(),
            vec![RestartCommand {
                label: "Restart Cinnamon".to_string(),
                program: "cinnamon".to_string(),
                args: vec!["--replace".to_string()],
            }],
        )
    } else if desktop.contains("MATE") {
        (
            "MATE".to_string(),
            vec![RestartCommand {
                label: "Restart MATE Panel".to_string(),
                program: "mate-panel".to_string(),
                args: vec!["--replace".to_string()],
            }],
        )
    } else {
        return None;
    };

    Some(DesktopEnv {
        name,
        session_type: session,
        commands,
    })
}

/// Execute a restart command detached from this process (via setsid).
pub fn execute(cmd: &RestartCommand) -> Result<(), String> {
    use std::os::unix::process::CommandExt;

    unsafe {
        Command::new(&cmd.program)
            .args(&cmd.args)
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .pre_exec(|| {
                libc::setsid();
                Ok(())
            })
            .spawn()
            .map_err(|e| format!("Failed to start {}: {}", cmd.program, e))?;
    }

    Ok(())
}
