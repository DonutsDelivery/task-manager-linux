use std::fs;

#[derive(Debug, Clone, Default)]
pub struct PsiMetrics {
    pub cpu: PsiResource,
    pub memory: PsiResource,
    pub io: PsiResource,
}

#[derive(Debug, Clone, Default)]
pub struct PsiResource {
    pub some_avg10: f64,
    pub some_avg60: f64,
    pub some_avg300: f64,
    pub full_avg10: f64,  // CPU doesn't have "full" line
    pub full_avg60: f64,
    pub full_avg300: f64,
}

pub struct PsiCollector;

impl PsiCollector {
    pub fn new() -> Self {
        Self
    }

    pub fn collect(&self) -> PsiMetrics {
        PsiMetrics {
            cpu: read_psi_resource("/proc/pressure/cpu"),
            memory: read_psi_resource("/proc/pressure/memory"),
            io: read_psi_resource("/proc/pressure/io"),
        }
    }
}

fn read_psi_resource(path: &str) -> PsiResource {
    let content = match fs::read_to_string(path) {
        Ok(c) => c,
        Err(_) => return PsiResource::default(),
    };

    let mut resource = PsiResource::default();

    for line in content.lines() {
        if line.starts_with("some") {
            if let Some(values) = parse_psi_line(line) {
                resource.some_avg10 = values.0;
                resource.some_avg60 = values.1;
                resource.some_avg300 = values.2;
            }
        } else if line.starts_with("full") {
            if let Some(values) = parse_psi_line(line) {
                resource.full_avg10 = values.0;
                resource.full_avg60 = values.1;
                resource.full_avg300 = values.2;
            }
        }
    }

    resource
}

fn parse_psi_line(line: &str) -> Option<(f64, f64, f64)> {
    // Line format: "some avg10=0.00 avg60=0.00 avg300=0.00 total=0"
    let parts: Vec<&str> = line.split_whitespace().collect();

    let mut avg10 = 0.0;
    let mut avg60 = 0.0;
    let mut avg300 = 0.0;

    for part in parts {
        if let Some(value_str) = part.strip_prefix("avg10=") {
            avg10 = value_str.parse().unwrap_or(0.0);
        } else if let Some(value_str) = part.strip_prefix("avg60=") {
            avg60 = value_str.parse().unwrap_or(0.0);
        } else if let Some(value_str) = part.strip_prefix("avg300=") {
            avg300 = value_str.parse().unwrap_or(0.0);
        }
    }

    Some((avg10, avg60, avg300))
}
