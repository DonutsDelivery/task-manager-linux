use std::collections::{HashMap, VecDeque};

const MAX_SAMPLES: usize = 300; // 5 minutes at 1 sample/sec

#[derive(Debug, Clone)]
pub struct AppHistory {
    pub display_name: String,
    pub cpu_samples: VecDeque<f64>,
    pub mem_samples: VecDeque<f64>,
}

impl AppHistory {
    fn new(name: &str) -> Self {
        Self {
            display_name: name.to_string(),
            cpu_samples: VecDeque::with_capacity(MAX_SAMPLES),
            mem_samples: VecDeque::with_capacity(MAX_SAMPLES),
        }
    }

    fn push(&mut self, cpu: f64, mem: f64) {
        self.cpu_samples.push_back(cpu);
        self.mem_samples.push_back(mem);
        if self.cpu_samples.len() > MAX_SAMPLES {
            self.cpu_samples.pop_front();
        }
        if self.mem_samples.len() > MAX_SAMPLES {
            self.mem_samples.pop_front();
        }
    }
}

pub struct AppHistoryTracker {
    histories: HashMap<String, AppHistory>,
}

impl AppHistoryTracker {
    pub fn new() -> Self {
        Self {
            histories: HashMap::new(),
        }
    }

    /// Update history from current app groups.
    /// Call once per collection cycle with the app groups from the snapshot.
    pub fn update(&mut self, app_groups: &[crate::model::AppGroup]) {
        let mut seen = std::collections::HashSet::new();

        for group in app_groups {
            let name = group.display_name().to_string();
            seen.insert(name.clone());

            let history = self.histories
                .entry(name.clone())
                .or_insert_with(|| AppHistory::new(&name));

            history.push(group.total_cpu, group.total_memory as f64);
        }

        // Prune histories for apps that have exited (keep for a while in case they restart)
        // Only remove if they haven't been seen for MAX_SAMPLES cycles
        self.histories.retain(|name, history| {
            if seen.contains(name) {
                true
            } else {
                // Push zero values for absent apps
                history.push(0.0, 0.0);
                // Keep until all samples are zero (faded out)
                history.cpu_samples.iter().any(|&v| v > 0.0)
                    || history.mem_samples.iter().any(|&v| v > 0.0)
            }
        });
    }

    /// Get a snapshot of all current histories.
    pub fn snapshot(&self) -> HashMap<String, AppHistory> {
        self.histories.clone()
    }

    /// Get history for a specific app.
    pub fn get(&self, name: &str) -> Option<&AppHistory> {
        self.histories.get(name)
    }
}
