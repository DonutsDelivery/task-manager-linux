use crate::model::service_entry::ServiceEntry;
use std::process::Command;

pub struct ServicesCollector;

impl ServicesCollector {
    pub fn collect() -> Vec<ServiceEntry> {
        let output = match Command::new("systemctl")
            .args(["list-units", "--type=service", "--all", "--no-legend", "--no-pager"])
            .output()
        {
            Ok(o) => o,
            Err(e) => {
                log::error!("Failed to run systemctl list-units: {}", e);
                return Vec::new();
            }
        };

        if !output.status.success() {
            log::error!(
                "systemctl list-units exited with {}: {}",
                output.status,
                String::from_utf8_lossy(&output.stderr)
            );
            return Vec::new();
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        let mut entries = Vec::new();

        for line in stdout.lines() {
            let line = line.trim();
            if line.is_empty() {
                continue;
            }

            // Format: "UNIT LOAD ACTIVE SUB DESCRIPTION..."
            // The UNIT field may have a leading bullet marker on some systems, strip it.
            let line = line.trim_start_matches('\u{25CF}').trim();

            // Use split_whitespace to handle variable column spacing
            let fields: Vec<&str> = line.split_whitespace().collect();
            if fields.len() < 4 {
                continue;
            }
            let unit = fields[0];
            let load_state = fields[1];
            let active_state = fields[2];
            let sub_state = fields[3];
            let description = if fields.len() > 4 {
                fields[4..].join(" ")
            } else {
                String::new()
            };

            // Strip .service suffix from unit name
            let name = unit.strip_suffix(".service").unwrap_or(unit).to_string();

            // Look up the unit file state for this unit
            let unit_file_state = get_unit_file_state(unit);

            entries.push(ServiceEntry {
                name,
                description,
                load_state: load_state.to_string(),
                active_state: active_state.to_string(),
                sub_state: sub_state.to_string(),
                unit_file_state,
            });
        }

        entries
    }

    pub fn service_action(name: &str, action: &str) -> Result<(), String> {
        let valid_actions = ["start", "stop", "restart", "enable", "disable"];
        if !valid_actions.contains(&action) {
            return Err(format!("Invalid action: {}", action));
        }

        let service_name = if name.ends_with(".service") {
            name.to_string()
        } else {
            format!("{}.service", name)
        };

        let output = Command::new("pkexec")
            .args(["systemctl", action, &service_name])
            .output()
            .map_err(|e| format!("Failed to execute pkexec systemctl {} {}: {}", action, service_name, e))?;

        if output.status.success() {
            Ok(())
        } else {
            let stderr = String::from_utf8_lossy(&output.stderr);
            Err(format!(
                "systemctl {} {} failed (exit {}): {}",
                action,
                service_name,
                output.status,
                stderr.trim()
            ))
        }
    }
}

/// Look up the UnitFileState for a given unit via systemctl show.
fn get_unit_file_state(unit: &str) -> String {
    let output = Command::new("systemctl")
        .args(["show", "--property=UnitFileState", "--no-pager", unit])
        .output();
    match output {
        Ok(o) if o.status.success() => {
            let stdout = String::from_utf8_lossy(&o.stdout);
            for line in stdout.lines() {
                if let Some(val) = line.strip_prefix("UnitFileState=") {
                    return val.trim().to_string();
                }
            }
            String::new()
        }
        _ => String::new(),
    }
}
