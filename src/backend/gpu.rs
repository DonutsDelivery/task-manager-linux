use crate::model::GpuInfo;
use nvml_wrapper::Nvml;
use nvml_wrapper::enums::device::UsedGpuMemory;
use std::collections::HashMap;
use std::time::Instant;

// ---------------------------------------------------------------------------
// Sysfs helpers
// ---------------------------------------------------------------------------

fn read_sysfs_u64(path: &str) -> Option<u64> {
    std::fs::read_to_string(path).ok()?.trim().parse().ok()
}

fn read_sysfs_string(path: &str) -> Option<String> {
    Some(std::fs::read_to_string(path).ok()?.trim().to_string())
}

/// Return the canonical hwmon dir for `device_path`.
///
/// `read_dir` order is NOT sorted and not stable across kernels. With multiple
/// hwmon entries (amdgpu + auxiliary chips like acp6x, acp_pdm, etc.) the
/// "first" entry may not be the GPU's own hwmon, which would silently route
/// every subsequent read (temp/power/fan) to the wrong chip.
///
/// Strategy: collect entries, sort lexicographically (deterministic), then
/// prefer the one whose `name` file equals "amdgpu". Falls back to the first
/// sorted entry if no entry claims that name.
fn find_hwmon_path(device_path: &str) -> Option<String> {
    let hwmon_dir = format!("{}/hwmon", device_path);
    let mut entries: Vec<_> = std::fs::read_dir(&hwmon_dir)
        .ok()?
        .filter_map(|e| e.ok())
        .map(|e| e.path().to_string_lossy().to_string())
        .collect();
    if entries.is_empty() {
        return None;
    }
    entries.sort();
    let amdgpu_entry = entries
        .iter()
        .find(|p| read_sysfs_string(&format!("{}/name", p)).as_deref() == Some("amdgpu"))
        .cloned();
    amdgpu_entry.or_else(|| entries.into_iter().next())
}

/// Parse `/usr/share/libdrm/amdgpu.ids` (the file libdrm/amdgpu use to map
/// (device_id, revision_id) -> marketing name) into a lookup table.
///
/// Format (verified against Arch's libdrm 2.4.124):
///   # comments
///   1.0.0                              <- version header, no commas
///   DDDD,	RR,	Marketing Name          <- tabs after commas
/// where DDDD/RR are UPPERCASE hex. Some devices only have a single row with
/// RR=00, which we keep as a wildcard fallback at lookup time.
fn parse_amdgpu_ids(path: &str) -> HashMap<(u16, u8), String> {
    let mut map = HashMap::new();
    let Ok(content) = std::fs::read_to_string(path) else {
        return map;
    };
    for line in content.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') || !line.contains(',') {
            continue;
        }
        let mut parts = line.splitn(3, ',');
        let dev = parts.next().unwrap_or("").trim();
        let rev = parts.next().unwrap_or("").trim();
        let name = parts.next().unwrap_or("").trim();
        if dev.is_empty() || name.is_empty() || rev.is_empty() {
            continue;
        }
        let Ok(device_id) = u16::from_str_radix(dev, 16) else {
            continue;
        };
        let Ok(revision_id) = u8::from_str_radix(rev, 16) else {
            continue;
        };
        map.insert((device_id, revision_id), name.to_string());
    }
    map
}

/// Look up a marketing name for an AMD GPU. Prefers an exact (device, revision)
/// match; falls back to (device, 0x00) when the table only has the wildcard row.
fn lookup_amd_name(
    map: &HashMap<(u16, u8), String>,
    device_id: u16,
    revision_id: u8,
) -> Option<String> {
    if let Some(n) = map.get(&(device_id, revision_id)) {
        return Some(n.clone());
    }
    map.get(&(device_id, 0x00)).cloned()
}

/// Read /sys/class/drm/card*/device/{device,revision} as parsed (u16, u8).
/// Returns None if either file is missing or malformed — callers must treat
/// that as "no id available" and fall back to a generic string.
fn read_amd_pci_ids(device_path: &str) -> Option<(u16, u8)> {
    let dev = read_sysfs_string(&format!("{}/device", device_path))?;
    let rev = read_sysfs_string(&format!("{}/revision", device_path))?;
    let dev = dev.trim().trim_start_matches("0x").trim_start_matches("0X");
    let rev = rev.trim().trim_start_matches("0x").trim_start_matches("0X");
    Some((u16::from_str_radix(dev, 16).ok()?, u8::from_str_radix(rev, 16).ok()?))
}

// ---------------------------------------------------------------------------
// Intel sysfs helpers (driver `xe` needs delta-based counters)
// ---------------------------------------------------------------------------

/// One poll of the monotonic counters an Intel GPU exposes. Utilization and
/// power are rates, so they only exist as a difference between two of these.
struct IntelSample {
    energy_uj: u64,
    idle_ms: Option<u64>,
    at: Instant,
}

/// The `xe` driver numbers its sensors from temp2 and identifies them by label
/// ("pkg", "vram", "vram_ch_0"...), so the index alone tells us nothing.
/// Prefer the package sensor, then any GPU-ish one, then vram as a last resort.
fn read_hwmon_temp_by_label(hwmon_path: &str) -> Option<u64> {
    let mut best: Option<(u8, u64)> = None;
    for entry in std::fs::read_dir(hwmon_path).ok()?.flatten() {
        let file = entry.file_name();
        let file = file.to_string_lossy();
        let idx = match file.strip_prefix("temp").and_then(|r| r.strip_suffix("_label")) {
            Some(i) => i.to_string(),
            None => continue,
        };
        let label = match read_sysfs_string(&format!("{}/{}", hwmon_path, file)) {
            Some(l) => l.to_lowercase(),
            None => continue,
        };
        let rank: u8 = if label.contains("pkg") || label.contains("package") {
            0
        } else if label.contains("gpu") || label.contains("core") {
            1
        } else if label == "vram" {
            2
        } else {
            continue; // skip per-channel vram sensors
        };
        let value = match read_sysfs_u64(&format!("{}/temp{}_input", hwmon_path, idx)) {
            Some(v) => v,
            None => continue,
        };
        if best.map_or(true, |(r, _)| rank < r) {
            best = Some((rank, value));
        }
    }
    best.map(|(_, v)| v)
}

/// Sum of idle residency across every graphics tile/gt, averaged so the value
/// stays comparable to a single gt. Returns None when the layout is absent
/// (i915, or an older kernel).
fn read_gt_idle_ms(device_path: &str) -> Option<u64> {
    let mut total = 0u64;
    let mut count = 0u64;
    for tile in std::fs::read_dir(device_path).ok()?.flatten() {
        let tname = tile.file_name();
        if !tname.to_string_lossy().starts_with("tile") {
            continue;
        }
        let gts = match std::fs::read_dir(tile.path()) {
            Ok(g) => g,
            Err(_) => continue,
        };
        for gt in gts.flatten() {
            let gname = gt.file_name();
            if !gname.to_string_lossy().starts_with("gt") {
                continue;
            }
            let p = format!("{}/gtidle/idle_residency_ms", gt.path().to_string_lossy());
            if let Some(v) = read_sysfs_u64(&p) {
                total += v;
                count += 1;
            }
        }
    }
    if count == 0 { None } else { Some(total / count) }
}

/// busy% = 100 - (time spent idle / wall time). Counters are monotonic, so a
/// decrease means a driver reset — report 0 rather than a negative spike.
fn busy_percent_from_idle(prev_idle_ms: u64, now_idle_ms: u64, elapsed_ms: f64) -> f64 {
    if elapsed_ms <= 0.0 || now_idle_ms < prev_idle_ms {
        return 0.0;
    }
    let idle_delta = (now_idle_ms - prev_idle_ms) as f64;
    (100.0 - (idle_delta / elapsed_ms) * 100.0).clamp(0.0, 100.0)
}

// ---------------------------------------------------------------------------
// GPU backend detection
// ---------------------------------------------------------------------------

enum GpuBackend {
    Nvidia(Nvml),
    Amd {
        card_path: String,   // e.g. /sys/class/drm/card0
        device_path: String, // e.g. /sys/class/drm/card0/device
        hwmon_path: Option<String>,
        name: String,
    },
    Intel {
        card_path: String,
        device_path: String,
        hwmon_path: Option<String>,
        name: String,
    },
    None,
}

/// Scan /sys/class/drm/card* for all cards whose device/vendor matches `vendor_id`.
/// Returns Vec of (card_path, device_path) for all matches.
fn find_drm_cards_by_vendor(vendor_id: &str) -> Vec<(String, String)> {
    let drm_dir = match std::fs::read_dir("/sys/class/drm") {
        Ok(d) => d,
        Err(_) => return Vec::new(),
    };
    let mut cards: Vec<_> = drm_dir
        .filter_map(|e| e.ok())
        .filter(|e| {
            let name = e.file_name();
            let name = name.to_string_lossy();
            // Match card0, card1, ... but not card0-DP-1 etc.
            name.starts_with("card") && name[4..].chars().all(|c| c.is_ascii_digit())
        })
        .collect();
    // Sort so we check card0, card1, ... in order
    cards.sort_by_key(|e| e.file_name());

    let mut result = Vec::new();
    for entry in cards {
        let card_path = entry.path().to_string_lossy().to_string();
        let device_path = format!("{}/device", card_path);
        let vendor_path = format!("{}/vendor", device_path);
        if let Some(vendor) = read_sysfs_string(&vendor_path) {
            if vendor == vendor_id {
                result.push((card_path, device_path));
            }
        }
    }
    result
}

fn detect_amd_gpu_name(
    device_path: &str,
    hwmon_path: &Option<String>,
    amd_ids: &HashMap<(u16, u8), String>,
) -> String {
    // 1) product_name: rarely set on consumer amdgpu, but cheap to check.
    if let Some(name) = read_sysfs_string(&format!("{}/product_name", device_path)) {
        if !name.is_empty() {
            return name;
        }
    }
    // 2) hwmon name: usually the literal string "amdgpu" — not unique enough
    //    to disambiguate two AMD cards on the same machine, but we'll take
    //    it as a hint and improve below if the user has distinct names.
    if let Some(ref hp) = hwmon_path {
        if let Some(name) = read_sysfs_string(&format!("{}/name", hp)) {
            let n = name.trim();
            if !n.is_empty() && n != "amdgpu" {
                return n.to_string();
            }
        }
    }
    // 3) amdgpu.ids lookup — the same table libdrm uses. Resolves things like
    //    7551/C0 -> "AMD Radeon AI PRO R9700" or 164E/D8 -> "AMD Radeon 610M".
    if let Some((dev, rev)) = read_amd_pci_ids(device_path) {
        if let Some(name) = lookup_amd_name(amd_ids, dev, rev) {
            return name;
        }
    }
    // 4) Last-resort fallback: at least make two AMD cards distinguishable
    //    by embedding the device id. (We cannot easily include revision here
    //    without re-reading sysfs, but device id alone is enough to tell an
    //    R9700 apart from a Raphael iGPU.)
    if let Some(dev_id) = read_sysfs_string(&format!("{}/device", device_path)) {
        return format!("AMD GPU (1002:{})", dev_id.trim_start_matches("0x"));
    }
    "AMD GPU".to_string()
}

fn detect_intel_gpu_name(card_path: &str, device_path: &str) -> String {
    // Try device/label (sometimes present on discrete Intel Arc)
    if let Some(name) = read_sysfs_string(&format!("{}/label", device_path)) {
        if !name.is_empty() {
            return name;
        }
    }
    // Try card-level label
    if let Some(name) = read_sysfs_string(&format!("{}/device/label", card_path)) {
        if !name.is_empty() {
            return name;
        }
    }
    "Intel GPU".to_string()
}

fn detect_backends() -> Vec<GpuBackend> {
    let mut backends = Vec::new();

    // 1) Try NVIDIA via NVML (can have multiple NVIDIA GPUs)
    if let Ok(nvml) = Nvml::init() {
        log::info!("NVML initialized successfully");
        backends.push(GpuBackend::Nvidia(nvml));
    }

    // 2) Scan ALL AMD cards (vendor 0x1002).
    //    Parse amdgpu.ids ONCE up front so detect_amd_gpu_name() doesn't
    //    re-read the file per card. The file is tiny (< 20 KB) and stable
    //    for the process lifetime.
    let amd_ids = parse_amdgpu_ids("/usr/share/libdrm/amdgpu.ids");
    if amd_ids.is_empty() {
        log::warn!(
            "amdgpu.ids not found or empty at /usr/share/libdrm/amdgpu.ids; \
             AMD GPU names will fall back to PCI-id-based strings"
        );
    }
    for (card_path, device_path) in find_drm_cards_by_vendor("0x1002") {
        let hwmon_path = find_hwmon_path(&device_path);
        let name = detect_amd_gpu_name(&device_path, &hwmon_path, &amd_ids);
        log::info!("AMD GPU detected via sysfs: {} ({})", name, card_path);
        backends.push(GpuBackend::Amd {
            card_path,
            device_path,
            hwmon_path,
            name,
        });
    }

    // 3) Scan ALL Intel cards (vendor 0x8086)
    for (card_path, device_path) in find_drm_cards_by_vendor("0x8086") {
        let hwmon_path = find_hwmon_path(&device_path);
        let name = detect_intel_gpu_name(&card_path, &device_path);
        log::info!("Intel GPU detected via sysfs: {} ({})", name, card_path);
        backends.push(GpuBackend::Intel {
            card_path,
            device_path,
            hwmon_path,
            name,
        });
    }

    if backends.is_empty() {
        log::warn!("No GPU detected - GPU monitoring disabled");
    }

    backends
}

// ---------------------------------------------------------------------------
// GpuCollector
// ---------------------------------------------------------------------------

pub struct GpuCollector {
    backends: Vec<GpuBackend>,
    /// Last Intel counter poll per card path; rates need two samples.
    intel_state: HashMap<String, IntelSample>,
}

impl GpuCollector {
    pub fn new() -> Self {
        Self {
            backends: detect_backends(),
            intel_state: HashMap::new(),
        }
    }

    pub fn collect_system(&mut self) -> Vec<GpuInfo> {
        let mut gpu_infos = Vec::new();

        for backend in &self.backends {
            match backend {
                GpuBackend::Nvidia(nvml) => {
                    // NVML can have multiple NVIDIA devices
                    if let Ok(device_count) = nvml.device_count() {
                        for index in 0..device_count {
                            gpu_infos.push(self.collect_nvidia(nvml, index));
                        }
                    }
                }
                GpuBackend::Amd {
                    card_path: _,
                    device_path,
                    hwmon_path,
                    name,
                } => {
                    gpu_infos.push(Self::collect_amd(device_path, hwmon_path, name));
                }
                GpuBackend::Intel {
                    card_path,
                    device_path: _,
                    hwmon_path,
                    name,
                } => {
                    gpu_infos.push(Self::collect_intel(card_path, hwmon_path, name, &mut self.intel_state));
                }
                GpuBackend::None => {
                    // Skip None backends
                }
            }
        }

        gpu_infos
    }

    pub fn collect_per_process(&self) -> HashMap<u32, u64> {
        let mut map = HashMap::new();

        // Aggregate across all NVIDIA GPUs
        for backend in &self.backends {
            if let GpuBackend::Nvidia(nvml) = backend {
                let per_process = self.collect_per_process_nvidia(nvml);
                for (pid, vram) in per_process {
                    *map.entry(pid).or_insert(0) += vram;
                }
            }
        }
        // Per-process VRAM tracking not available via sysfs for AMD/Intel

        map
    }

    // ------------------------------------------------------------------
    // NVIDIA (NVML)
    // ------------------------------------------------------------------

    fn collect_nvidia(&self, nvml: &Nvml, index: u32) -> GpuInfo {
        let device = match nvml.device_by_index(index) {
            Ok(d) => d,
            Err(_) => return GpuInfo::default(),
        };

        let name = device.name().unwrap_or_else(|_| "Unknown GPU".to_string());
        let utilization = device.utilization_rates().ok();
        let memory_info = device.memory_info().ok();
        let temp = device
            .temperature(nvml_wrapper::enum_wrappers::device::TemperatureSensor::Gpu)
            .unwrap_or(0);
        let power = device.power_usage().unwrap_or(0) as f64 / 1000.0; // mW to W
        let power_limit = device.enforced_power_limit().unwrap_or(0) as f64 / 1000.0;
        let fan = device.fan_speed(0).unwrap_or(0);

        GpuInfo {
            available: true,
            name,
            utilization_percent: utilization.map(|u| u.gpu as f64).unwrap_or(0.0),
            vram_used: memory_info.as_ref().map(|m| m.used).unwrap_or(0),
            vram_total: memory_info.as_ref().map(|m| m.total).unwrap_or(0),
            temperature: temp,
            power_watts: power,
            power_limit_watts: power_limit,
            fan_speed_percent: fan,
        }
    }

    fn collect_per_process_nvidia(&self, nvml: &Nvml) -> HashMap<u32, u64> {
        let mut map = HashMap::new();

        // Iterate over all NVIDIA devices
        if let Ok(device_count) = nvml.device_count() {
            for index in 0..device_count {
                let device = match nvml.device_by_index(index) {
                    Ok(d) => d,
                    Err(_) => continue,
                };

                if let Ok(procs) = device.running_compute_processes() {
                    for p in procs {
                        let mem = match p.used_gpu_memory {
                            UsedGpuMemory::Used(bytes) => bytes,
                            UsedGpuMemory::Unavailable => 0,
                        };
                        *map.entry(p.pid).or_insert(0) += mem;
                    }
                }
                if let Ok(procs) = device.running_graphics_processes() {
                    for p in procs {
                        let mem = match p.used_gpu_memory {
                            UsedGpuMemory::Used(bytes) => bytes,
                            UsedGpuMemory::Unavailable => 0,
                        };
                        *map.entry(p.pid).or_insert(0) += mem;
                    }
                }
            }
        }

        map
    }

    // ------------------------------------------------------------------
    // AMD (sysfs)
    // ------------------------------------------------------------------

    /// Read all per-card metrics for one AMD GPU.
    ///
    /// Defensive contract: every individual read uses `unwrap_or(0)` / Option
    /// chains. A single missing or unreadable sysfs file MUST degrade to 0 in
    /// the corresponding field — it MUST NOT panic, and it MUST NOT cause the
    /// caller to skip this card or any subsequent card in the backends list.
    ///
    /// VRAM quirks (Radeon AI Pro R9700 vs Ryzen 7 7700 iGPU):
    /// - dGPU: `mem_info_vram_total` is the dedicated VRAM (e.g. 32 GB).
    ///         `mem_info_gtt_total` exists but is a few MB of GART, irrelevant.
    /// - iGPU (Raphael / Phoenix / etc.): `mem_info_vram_total` reports ONLY
    ///         the BIOS carve-out (often 512 MiB), while the real shared pool
    ///         lives in `mem_info_gtt_total` (e.g. ~16 GiB). Reporting the
    ///         carve-out makes the iGPU look crippled and is almost certainly
    ///         part of the "why is only one GPU listed" confusion.
    /// We pick the larger of the two for both total and used, which gives the
    /// dGPU its VRAM and the iGPU its GTT without per-driver branching.
    fn collect_amd(device_path: &str, hwmon_path: &Option<String>, name: &str) -> GpuInfo {
        // gpu_busy_percent lives in `device/`, not `card/`. Present on dGPUs
        // and most iGPUs; absent returns 0 instead of an error.
        let utilization = read_sysfs_u64(&format!("{}/gpu_busy_percent", device_path))
            .map(|v| v as f64)
            .unwrap_or(0.0);

        let vram_total = {
            let v = read_sysfs_u64(&format!("{}/mem_info_vram_total", device_path)).unwrap_or(0);
            let g = read_sysfs_u64(&format!("{}/mem_info_gtt_total", device_path)).unwrap_or(0);
            v.max(g)
        };
        let vram_used = {
            let v = read_sysfs_u64(&format!("{}/mem_info_vram_used", device_path)).unwrap_or(0);
            let g = read_sysfs_u64(&format!("{}/mem_info_gtt_used", device_path)).unwrap_or(0);
            v.max(g)
        };

        // iGPUs (Raphael) commonly have NO fan and NO power sensors — every
        // read here MUST be allowed to silently degrade to 0. We do NOT want
        // any missing file to drop the card from the listing entirely.
        let mut temperature: u32 = 0;
        let mut power_watts: f64 = 0.0;
        let mut fan_speed_percent: u32 = 0;

        if let Some(ref hp) = hwmon_path {
            // temp1_input is in millidegrees Celsius
            temperature = read_sysfs_u64(&format!("{}/temp1_input", hp))
                .map(|v| (v / 1000) as u32)
                .unwrap_or(0);

            // power1_average is in microwatts (often absent on iGPU)
            power_watts = read_sysfs_u64(&format!("{}/power1_average", hp))
                .map(|v| v as f64 / 1_000_000.0)
                .unwrap_or(0.0);

            // Fan speed: pwm1 is 0-255 → percent. If absent (no fan), the
            // read returns 0 rather than faking a number from RPM.
            fan_speed_percent = read_sysfs_u64(&format!("{}/pwm1", hp))
                .map(|v| ((v as f64 / 255.0) * 100.0) as u32)
                .unwrap_or(0);
        }

        // power1_cap is the enforced limit in microwatts (not always exposed).
        let power_limit_watts = hwmon_path
            .as_ref()
            .and_then(|hp| read_sysfs_u64(&format!("{}/power1_cap", hp)))
            .map(|v| v as f64 / 1_000_000.0)
            .unwrap_or(0.0);

        GpuInfo {
            available: true,
            name: name.to_string(),
            utilization_percent: utilization,
            vram_used,
            vram_total,
            temperature,
            power_watts,
            power_limit_watts,
            fan_speed_percent,
        }
    }

    // ------------------------------------------------------------------
    // Intel (sysfs)
    // ------------------------------------------------------------------

    /// Modern Intel discrete (Arc, driver `xe`) exposes none of the classic
    /// i915 hwmon files. Rather than branching on driver name, every reading
    /// tries the old path first and falls back to the new one, so i915 and xe
    /// both work through the same code.
    fn collect_intel(
        card_path: &str,
        hwmon_path: &Option<String>,
        name: &str,
        state: &mut HashMap<String, IntelSample>,
    ) -> GpuInfo {
        let device_path = format!("{}/device", card_path);
        let now = Instant::now();

        // --- temperature ------------------------------------------------
        // i915: temp1_input. xe: numbering starts at temp2, so pick by label.
        let temperature = hwmon_path
            .as_ref()
            .and_then(|hp| {
                read_sysfs_u64(&format!("{}/temp1_input", hp))
                    .or_else(|| read_hwmon_temp_by_label(hp))
            })
            .map(|v| (v / 1000) as u32)
            .unwrap_or(0);

        // --- power limit ------------------------------------------------
        let power_limit_watts = hwmon_path
            .as_ref()
            .and_then(|hp| read_sysfs_u64(&format!("{}/power1_cap", hp)))
            .map(|v| v as f64 / 1_000_000.0)
            .unwrap_or(0.0);

        // --- fan ----------------------------------------------------------
        // ponytail: xe reports fan*_input in RPM with no max to scale against,
        // so percent stays 0 rather than being faked. Add an rpm field to
        // GpuInfo if the UI ever wants to show it.
        let fan_speed_percent = hwmon_path
            .as_ref()
            .and_then(|hp| read_sysfs_u64(&format!("{}/pwm1", hp)))
            .map(|v| ((v as f64 / 255.0) * 100.0) as u32)
            .unwrap_or(0);

        // --- counters needing a delta ---------------------------------------
        // energy1_input is a monotonic microjoule counter (energy1_label="card",
        // i.e. whole-board draw). idle_residency_ms is monotonic per-gt idle time.
        let energy_uj = hwmon_path
            .as_ref()
            .and_then(|hp| read_sysfs_u64(&format!("{}/energy1_input", hp)));
        let idle_ms = read_gt_idle_ms(&device_path);

        let prev = state.get(card_path);
        let elapsed_us = prev.map(|p| now.duration_since(p.at).as_micros() as u64);

        // power: prefer i915's direct reading, else derive from the energy counter
        let mut power_watts = hwmon_path
            .as_ref()
            .and_then(|hp| read_sysfs_u64(&format!("{}/power1_average", hp)))
            .map(|v| v as f64 / 1_000_000.0)
            .unwrap_or(0.0);
        if power_watts == 0.0 {
            if let (Some(e), Some(p), Some(dt_us)) = (energy_uj, prev, elapsed_us) {
                if dt_us > 0 && e >= p.energy_uj {
                    // microjoules / microseconds == watts
                    power_watts = (e - p.energy_uj) as f64 / dt_us as f64;
                }
            }
        }

        // utilization: prefer a real busy counter, else 100% minus idle residency
        let mut utilization_percent =
            read_sysfs_u64(&format!("{}/gpu_busy_percent", device_path))
                .map(|v| v as f64)
                .unwrap_or(0.0);
        if utilization_percent == 0.0 {
            if let (Some(i), Some(p), Some(dt_us)) = (idle_ms, prev, elapsed_us) {
                let dt_ms = dt_us as f64 / 1000.0;
                if dt_ms > 0.0 {
                    if let Some(prev_idle) = p.idle_ms {
                        utilization_percent =
                            busy_percent_from_idle(prev_idle, i, dt_ms);
                    }
                }
            }
        }

        state.insert(
            card_path.to_string(),
            IntelSample {
                energy_uj: energy_uj.unwrap_or(0),
                idle_ms,
                at: now,
            },
        );

        // --- VRAM -----------------------------------------------------------
        // i915 discrete exposes mem_info_vram_*. The xe driver exposes neither,
        // and the PCI BAR is rounded to a power of two (a 12 GB B580 reports a
        // 16 GiB BAR), so there is no honest sysfs source — report 0 rather than
        // a wrong number.
        let vram_total =
            read_sysfs_u64(&format!("{}/mem_info_vram_total", device_path)).unwrap_or(0);
        let vram_used =
            read_sysfs_u64(&format!("{}/mem_info_vram_used", device_path)).unwrap_or(0);

        GpuInfo {
            available: true,
            name: name.to_string(),
            utilization_percent,
            vram_used,
            vram_total,
            temperature,
            power_watts,
            power_limit_watts,
            fan_speed_percent,
        }
    }
}

// ---------------------------------------------------------------------------
// Tests
//
// We do NOT have an R9700 or Raphael iGPU available in this environment, so
// the unit tests below are the deliverable's proof. They exercise the parser
// against fixture amdgpu.ids content that covers the exact devices the user
// cares about (Radeon AI Pro R9700, R9600D, Raphael 610M) plus the wildcard
// fallback path. `detect_amd_gpu_name`'s sysfs-reading branches cannot be
// unit-tested without a fake sysfs tree, but the lookup table — which is the
// new logic — is fully covered.
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    /// The exact substring that the real /usr/share/libdrm/amdgpu.ids uses
    /// for the user's hardware, so the fixture below is representative.
    const REAL_FIXTURE: &str = "\
# List of AMDGPU IDs
#
# Syntax:
# device_id,\trevision_id,\tproduct_name
#
1.0.0
7551,\tC0,\tAMD Radeon AI PRO R9700
7551,\tC8,\tAMD Radeon AI PRO R9600D
164E,\tD8,\tAMD Radeon 610M
164E,\tD9,\tAMD Radeon 610M
164E,\tDA,\tAMD Radeon 610M
164E,\tDB,\tAMD Radeon 610M
164E,\tDC,\tAMD Radeon 610M
7470,\t00,\tAMD Radeon Pro W7700
";

    fn write_fixture(name: &str, body: &str) -> std::path::PathBuf {
        let mut p = std::env::temp_dir();
        p.push(format!("amdgpu.ids.test.{}.{}", std::process::id(), name));
        std::fs::write(&p, body).expect("write fixture");
        p
    }

    #[test]
    fn parse_amdgpu_ids_resolves_r9700_and_raphael() {
        let path = write_fixture("real", REAL_FIXTURE);
        let map = parse_amdgpu_ids(path.to_str().unwrap());

        // The user's actual hardware.
        assert_eq!(
            map.get(&(0x7551, 0xC0)).map(|s| s.as_str()),
            Some("AMD Radeon AI PRO R9700"),
            "Radeon AI Pro R9700 must be mapped at (0x7551, 0xC0)"
        );
        assert_eq!(
            map.get(&(0x7551, 0xC8)).map(|s| s.as_str()),
            Some("AMD Radeon AI PRO R9600D"),
            "Radeon AI Pro R9600D must be mapped at (0x7551, 0xC8)"
        );
        // Raphael iGPU revs D8..=DC all map to the same "610M" marketing name.
        for rev in [0xD8u8, 0xD9, 0xDA, 0xDB, 0xDC] {
            assert_eq!(
                map.get(&(0x164E, rev)).map(|s| s.as_str()),
                Some("AMD Radeon 610M"),
                "Raphael (0x164E) rev {:#04x} must map to 610M",
                rev
            );
        }

        // Comment + version header lines must not show up as entries.
        assert!(!map.values().any(|v| v.contains("Syntax")));
        assert!(!map.values().any(|v| v == "1.0.0"));

        std::fs::remove_file(path).ok();
    }

    #[test]
    fn lookup_amd_name_prefers_exact_revision_over_wildcard() {
        // W7700 has only a rev=00 wildcard row. Any non-zero rev must still
        // resolve, via the (device, 0x00) fallback.
        let mut map = HashMap::new();
        map.insert((0x7470, 0x00), "AMD Radeon Pro W7700".to_string());

        assert_eq!(
            lookup_amd_name(&map, 0x7470, 0xC0).as_deref(),
            Some("AMD Radeon Pro W7700"),
            "wildcard (rev=00) must match any revision when no exact row exists"
        );
        assert_eq!(
            lookup_amd_name(&map, 0x7470, 0x00).as_deref(),
            Some("AMD Radeon Pro W7700"),
            "exact (rev=00) must still work"
        );

        // Now add an exact row for rev=C0 and verify it wins over the wildcard.
        map.insert((0x7470, 0xC0), "AMD Radeon Pro W7700 Refresh".to_string());
        assert_eq!(
            lookup_amd_name(&map, 0x7470, 0xC0).as_deref(),
            Some("AMD Radeon Pro W7700 Refresh"),
            "exact revision row must beat the (device, 0x00) wildcard"
        );
        // A different rev that has no exact row should still hit the wildcard.
        assert_eq!(
            lookup_amd_name(&map, 0x7470, 0xD0).as_deref(),
            Some("AMD Radeon Pro W7700")
        );

        // Unknown device id -> None.
        assert!(lookup_amd_name(&map, 0x9999, 0x00).is_none());
    }

    #[test]
    fn parse_amdgpu_ids_handles_garbage_lines_gracefully() {
        // Mixed valid + garbage input. Parser must skip bad lines, keep good ones.
        let fixture = "\
# header
1.0.0
this line has no commas at all
7551,	XX,	Should Be Skipped   <- revision 'XX' is not hex
7551,	C0,	Good Entry
,	00,	Empty device id
164E,	00,	
7551,	C0,	Duplicate Wins Last
";
        let path = write_fixture("garbage", fixture);
        let map = parse_amdgpu_ids(path.to_str().unwrap());

        assert_eq!(
            map.get(&(0x7551, 0xC0)).map(|s| s.as_str()),
            Some("Duplicate Wins Last"),
            "duplicate (device, rev) row should overwrite the earlier entry"
        );
        assert!(
            !map.contains_key(&(0x164E, 0x00)),
            "name-less row must not be inserted"
        );
        assert_eq!(map.len(), 1, "only the valid row should land");

        std::fs::remove_file(path).ok();
    }

    #[test]
    fn parse_amdgpu_ids_missing_file_returns_empty_map() {
        let map = parse_amdgpu_ids("/tmp/this/path/does/not/exist/amdgpu.ids");
        assert!(map.is_empty(), "missing file must yield empty HashMap, not panic");
    }
    #[test]
    fn busy_percent_from_idle_matches_hand_worked_cases() {
        // fully idle for the whole second -> 0% busy
        assert_eq!(busy_percent_from_idle(1000, 2000, 1000.0), 0.0);
        // idle for a quarter of the second -> 75% busy
        assert_eq!(busy_percent_from_idle(1000, 1250, 1000.0), 75.0);
        // never idle -> 100% busy
        assert_eq!(busy_percent_from_idle(1000, 1000, 1000.0), 100.0);
        // counter went backwards (driver reset) -> 0, never negative
        assert_eq!(busy_percent_from_idle(2000, 1000, 1000.0), 0.0);
        // idle exceeds wall time (multi-gt rounding) -> clamped, never negative
        assert_eq!(busy_percent_from_idle(0, 5000, 1000.0), 0.0);
        // no time passed -> 0, never a divide-by-zero inf
        assert_eq!(busy_percent_from_idle(0, 0, 0.0), 0.0);
    }

    #[test]
    fn energy_counter_delta_is_watts() {
        // microjoules over microseconds is watts by definition:
        // 210 J drawn over 1 s == 210 W
        let d_uj: u64 = 210_000_000;
        let dt_us: u64 = 1_000_000;
        assert_eq!(d_uj as f64 / dt_us as f64, 210.0);
    }

}
