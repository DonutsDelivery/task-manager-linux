use evdev::{Device, EventSummary, KeyCode};
use std::os::fd::AsRawFd;
use std::process::Command;

/// Run the evdev shortcut listener daemon.
/// Monitors all keyboards for Ctrl+Shift+Escape and launches the task manager.
/// This function never returns under normal operation.
pub fn run_daemon() -> ! {
    loop {
        if let Err(e) = listen_loop() {
            eprintln!("shortcut-daemon: {}, retrying in 3s", e);
            std::thread::sleep(std::time::Duration::from_secs(3));
        }
    }
}

fn find_keyboards() -> Vec<Device> {
    evdev::enumerate()
        .filter_map(|(_, d)| {
            let keys = d.supported_keys()?;
            if keys.contains(KeyCode::KEY_ESC)
                && keys.contains(KeyCode::KEY_LEFTCTRL)
                && keys.contains(KeyCode::KEY_LEFTSHIFT)
            {
                Some(d)
            } else {
                None
            }
        })
        .collect()
}

fn listen_loop() -> Result<(), String> {
    let mut keyboards = find_keyboards();
    if keyboards.is_empty() {
        return Err("no keyboard devices found (is user in 'input' group?)".into());
    }

    eprintln!(
        "shortcut-daemon: monitoring {} keyboard(s) for Ctrl+Shift+Escape",
        keyboards.len()
    );

    let mut pollfds: Vec<libc::pollfd> = keyboards
        .iter()
        .map(|d| libc::pollfd {
            fd: d.as_raw_fd(),
            events: libc::POLLIN,
            revents: 0,
        })
        .collect();

    let mut ctrl = false;
    let mut shift = false;

    loop {
        let ret = unsafe { libc::poll(pollfds.as_mut_ptr(), pollfds.len() as _, -1) };
        if ret < 0 {
            let err = std::io::Error::last_os_error();
            if err.kind() == std::io::ErrorKind::Interrupted {
                continue;
            }
            return Err(format!("poll: {}", err));
        }

        for (i, pfd) in pollfds.iter().enumerate() {
            if pfd.revents & libc::POLLIN == 0 {
                continue;
            }

            let events = keyboards[i]
                .fetch_events()
                .map_err(|e| format!("fetch_events: {}", e))?;

            for ev in events {
                if let EventSummary::Key(_, code, value) = ev.destructure() {
                    let pressed = value == 1;
                    let released = value == 0;

                    match code {
                        KeyCode::KEY_LEFTCTRL | KeyCode::KEY_RIGHTCTRL => {
                            if pressed { ctrl = true; } else if released { ctrl = false; }
                        }
                        KeyCode::KEY_LEFTSHIFT | KeyCode::KEY_RIGHTSHIFT => {
                            if pressed { shift = true; } else if released { shift = false; }
                        }
                        KeyCode::KEY_ESC if pressed && ctrl && shift => {
                            launch_task_manager();
                        }
                        _ => {}
                    }
                }
            }
        }
    }
}

fn launch_task_manager() {
    let bin = dirs::home_dir()
        .expect("no home dir")
        .join(".local/bin/task-manager-linux");

    let bin_path = if bin.exists() {
        bin
    } else {
        std::env::current_exe().unwrap_or(bin)
    };

    eprintln!("shortcut-daemon: launching {}", bin_path.display());

    let _ = Command::new(&bin_path)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn();
}
