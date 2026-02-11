use crate::model::ProcessInfo;
use std::collections::HashMap;

#[derive(Debug, Clone)]
pub struct UserInfo {
    pub uid: u32,
    pub username: String,
    pub session_count: u32,
    pub cpu_percent: f64,
    pub memory_bytes: u64,
    pub process_count: u32,
}

pub fn collect_users(processes: &[ProcessInfo]) -> Vec<UserInfo> {
    let mut user_map: HashMap<u32, UserInfo> = HashMap::new();

    for proc in processes {
        let entry = user_map.entry(proc.uid).or_insert_with(|| UserInfo {
            uid: proc.uid,
            username: proc.user.clone(),
            session_count: 0,
            cpu_percent: 0.0,
            memory_bytes: 0,
            process_count: 0,
        });
        entry.cpu_percent += proc.cpu_percent;
        entry.memory_bytes += proc.memory_bytes;
        entry.process_count += 1;
    }

    // Get session counts from `who` command output
    if let Ok(output) = std::process::Command::new("who").output() {
        let text = String::from_utf8_lossy(&output.stdout);
        for line in text.lines() {
            if let Some(username) = line.split_whitespace().next() {
                for info in user_map.values_mut() {
                    if info.username == username {
                        info.session_count += 1;
                        break;
                    }
                }
            }
        }
    }

    let mut result: Vec<UserInfo> = user_map.into_values().collect();
    result.sort_by(|a, b| {
        b.cpu_percent
            .partial_cmp(&a.cpu_percent)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    result
}

pub fn logoff_user(username: &str) -> Result<(), String> {
    let status = std::process::Command::new("loginctl")
        .args(["terminate-user", username])
        .status()
        .map_err(|e| format!("Failed to run loginctl: {}", e))?;
    if status.success() {
        Ok(())
    } else {
        Err(format!(
            "loginctl terminate-user {} failed with exit code {:?}",
            username,
            status.code()
        ))
    }
}
