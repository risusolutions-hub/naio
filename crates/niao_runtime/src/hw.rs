//! Shared hardware probing, budgets, and thermal-throttle state for the
//! ncpu / ngpu / nram / nnpu / ndevice standard libraries.
//!
//! Design rules:
//! - Zero new dependencies. Native file reads on Linux (`/proc`, `/sys`);
//!   vendor tools elsewhere (`nvidia-smi`, `rocm-smi`, `wmic`, `sysctl`, ...).
//! - Every external probe runs under a 500 ms watchdog and is cached (~1 s),
//!   so polling APIs stay cheap in tight loops.
//! - Readings that cannot be obtained are reported as unknown (`-1` / `None`),
//!   never invented.
//! - Budgets/throttle are plain atomics: one load on the hot path.

use std::process::Command;
use std::sync::atomic::{AtomicBool, AtomicI64, AtomicU8, AtomicU64, Ordering};
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

// ===========================================================================
// Command runner with watchdog timeout
// ===========================================================================

const CMD_TIMEOUT_MS: u64 = 500;

/// Run a command, capture stdout as UTF-8. Returns None on failure/timeout.
pub fn run_cmd(program: &str, args: &[&str]) -> Option<String> {
    run_cmd_timeout(program, args, CMD_TIMEOUT_MS)
}

pub fn run_cmd_timeout(program: &str, args: &[&str], timeout_ms: u64) -> Option<String> {
    let program = program.to_string();
    let args: Vec<String> = args.iter().map(|s| s.to_string()).collect();
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let mut cmd = Command::new(&program);
        cmd.args(&args);
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            cmd.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
        }
        let out = cmd.output().ok().and_then(|o| {
            if o.status.success() {
                String::from_utf8(o.stdout).ok()
            } else {
                None
            }
        });
        let _ = tx.send(out);
    });
    rx.recv_timeout(Duration::from_millis(timeout_ms)).ok().flatten()
}

// ===========================================================================
// Generic 1-second probe cache
// ===========================================================================

struct CacheSlot<T: Clone> {
    inner: Mutex<Option<(Instant, T)>>,
}

impl<T: Clone> CacheSlot<T> {
    const fn new() -> Self {
        CacheSlot {
            inner: Mutex::new(None),
        }
    }

    fn get_or(&self, ttl_ms: u64, compute: impl FnOnce() -> T) -> T {
        let mut slot = match self.inner.lock() {
            Ok(s) => s,
            Err(p) => p.into_inner(),
        };
        if let Some((at, v)) = slot.as_ref() {
            if at.elapsed() < Duration::from_millis(ttl_ms) {
                return v.clone();
            }
        }
        let v = compute();
        *slot = Some((Instant::now(), v.clone()));
        v
    }

    fn invalidate(&self) {
        if let Ok(mut slot) = self.inner.lock() {
            *slot = None;
        }
    }
}

// ===========================================================================
// CPU
// ===========================================================================

pub fn logical_cores() -> usize {
    std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(1)
}

pub fn physical_cores() -> i64 {
    static SLOT: CacheSlot<i64> = CacheSlot::new();
    SLOT.get_or(3_600_000, || {
        if cfg!(target_os = "linux") {
            if let Ok(text) = std::fs::read_to_string("/proc/cpuinfo") {
                let mut pairs: Vec<(String, String)> = Vec::new();
                let (mut phys, mut core) = (String::new(), String::new());
                for line in text.lines() {
                    let lower = line.to_ascii_lowercase();
                    if lower.starts_with("physical id") {
                        phys = line.split(':').nth(1).unwrap_or("").trim().to_string();
                    } else if lower.starts_with("core id") {
                        core = line.split(':').nth(1).unwrap_or("").trim().to_string();
                        let pair = (phys.clone(), core.clone());
                        if !pairs.contains(&pair) {
                            pairs.push(pair);
                        }
                    }
                }
                if !pairs.is_empty() {
                    return pairs.len() as i64;
                }
            }
        }
        if cfg!(windows) {
            if let Some(out) = run_cmd("wmic", &["cpu", "get", "NumberOfCores", "/value"]) {
                let total: i64 = out
                    .lines()
                    .filter_map(|l| l.trim().strip_prefix("NumberOfCores="))
                    .filter_map(|v| v.trim().parse::<i64>().ok())
                    .sum();
                if total > 0 {
                    return total;
                }
            }
        }
        if cfg!(target_os = "macos") {
            if let Some(out) = run_cmd("sysctl", &["-n", "hw.physicalcpu"]) {
                if let Ok(n) = out.trim().parse::<i64>() {
                    return n;
                }
            }
        }
        -1
    })
}

pub fn cpu_brand() -> String {
    static SLOT: CacheSlot<String> = CacheSlot::new();
    SLOT.get_or(3_600_000, || {
        if cfg!(target_os = "linux") {
            if let Ok(text) = std::fs::read_to_string("/proc/cpuinfo") {
                for line in text.lines() {
                    if line.to_ascii_lowercase().starts_with("model name") {
                        return line.split(':').nth(1).unwrap_or("").trim().to_string();
                    }
                }
            }
        }
        if cfg!(windows) {
            if let Some(out) = run_cmd("wmic", &["cpu", "get", "Name", "/value"]) {
                for line in out.lines() {
                    if let Some(name) = line.trim().strip_prefix("Name=") {
                        if !name.trim().is_empty() {
                            return name.trim().to_string();
                        }
                    }
                }
            }
        }
        if cfg!(target_os = "macos") {
            if let Some(out) = run_cmd("sysctl", &["-n", "machdep.cpu.brand_string"]) {
                let t = out.trim();
                if !t.is_empty() {
                    return t.to_string();
                }
            }
        }
        "unknown".to_string()
    })
}

/// Linux: (busy, total) jiffies from /proc/stat first line.
fn proc_stat_totals() -> Option<(u64, u64)> {
    let text = std::fs::read_to_string("/proc/stat").ok()?;
    let line = text.lines().next()?;
    let nums: Vec<u64> = line
        .split_whitespace()
        .skip(1)
        .filter_map(|v| v.parse().ok())
        .collect();
    if nums.len() < 4 {
        return None;
    }
    let idle = nums[3] + nums.get(4).copied().unwrap_or(0); // idle + iowait
    let total: u64 = nums.iter().sum();
    Some((total - idle, total))
}

/// System CPU usage percent (0..=100). -1.0 when unavailable.
pub fn cpu_usage_pct() -> f64 {
    static SLOT: CacheSlot<f64> = CacheSlot::new();
    SLOT.get_or(1000, || {
        if cfg!(target_os = "linux") {
            static LAST: Mutex<Option<(u64, u64)>> = Mutex::new(None);
            if let Some((busy, total)) = proc_stat_totals() {
                let mut last = match LAST.lock() {
                    Ok(l) => l,
                    Err(p) => p.into_inner(),
                };
                let usage = match *last {
                    Some((pb, pt)) if total > pt => {
                        (busy - pb) as f64 / (total - pt) as f64 * 100.0
                    }
                    _ => {
                        // First call: short two-point sample.
                        drop(last);
                        std::thread::sleep(Duration::from_millis(120));
                        let second = proc_stat_totals();
                        last = match LAST.lock() {
                            Ok(l) => l,
                            Err(p) => p.into_inner(),
                        };
                        match second {
                            Some((b2, t2)) if t2 > total => {
                                (b2 - busy) as f64 / (t2 - total) as f64 * 100.0
                            }
                            _ => -1.0,
                        }
                    }
                };
                if usage >= 0.0 {
                    if let Some(cur) = proc_stat_totals() {
                        *last = Some(cur);
                    }
                    return usage.clamp(0.0, 100.0);
                }
                return -1.0;
            }
            return -1.0;
        }
        if cfg!(windows) {
            if let Some(out) = run_cmd("wmic", &["cpu", "get", "loadpercentage", "/value"]) {
                let vals: Vec<f64> = out
                    .lines()
                    .filter_map(|l| l.trim().strip_prefix("LoadPercentage="))
                    .filter_map(|v| v.trim().parse::<f64>().ok())
                    .collect();
                if !vals.is_empty() {
                    return (vals.iter().sum::<f64>() / vals.len() as f64).clamp(0.0, 100.0);
                }
            }
            return -1.0;
        }
        if cfg!(target_os = "macos") {
            if let Some(out) = run_cmd_timeout("top", &["-l", "1", "-n", "0"], 1500) {
                for line in out.lines() {
                    if line.starts_with("CPU usage:") {
                        if let Some(idle_part) = line.split(',').find(|p| p.contains("idle")) {
                            let digits: String = idle_part
                                .chars()
                                .filter(|c| c.is_ascii_digit() || *c == '.')
                                .collect();
                            if let Ok(idle) = digits.parse::<f64>() {
                                return (100.0 - idle).clamp(0.0, 100.0);
                            }
                        }
                    }
                }
            }
            return -1.0;
        }
        -1.0
    })
}

/// CPU package temperature in °C. -1 when unavailable.
pub fn cpu_temp_c() -> i64 {
    static SLOT: CacheSlot<i64> = CacheSlot::new();
    SLOT.get_or(1000, || {
        if cfg!(target_os = "linux") {
            let mut best: i64 = -1;
            // thermal zones
            if let Ok(entries) = std::fs::read_dir("/sys/class/thermal") {
                for e in entries.flatten() {
                    let path = e.path();
                    let ty = std::fs::read_to_string(path.join("type")).unwrap_or_default();
                    let ty = ty.to_ascii_lowercase();
                    let relevant = ty.contains("cpu")
                        || ty.contains("x86_pkg")
                        || ty.contains("soc")
                        || ty.contains("acpi");
                    if !relevant {
                        continue;
                    }
                    if let Ok(raw) = std::fs::read_to_string(path.join("temp")) {
                        if let Ok(milli) = raw.trim().parse::<i64>() {
                            best = best.max(milli / 1000);
                        }
                    }
                }
            }
            // hwmon coretemp / k10temp
            if let Ok(entries) = std::fs::read_dir("/sys/class/hwmon") {
                for e in entries.flatten() {
                    let path = e.path();
                    let name = std::fs::read_to_string(path.join("name")).unwrap_or_default();
                    let name = name.trim().to_ascii_lowercase();
                    if name != "coretemp" && name != "k10temp" && name != "zenpower" {
                        continue;
                    }
                    if let Ok(raw) = std::fs::read_to_string(path.join("temp1_input")) {
                        if let Ok(milli) = raw.trim().parse::<i64>() {
                            best = best.max(milli / 1000);
                        }
                    }
                }
            }
            return best;
        }
        if cfg!(windows) {
            // Usually requires admin and often unsupported; try, degrade to -1.
            if let Some(out) = run_cmd(
                "wmic",
                &[
                    "/namespace:\\\\root\\wmi",
                    "PATH",
                    "MSAcpi_ThermalZoneTemperature",
                    "get",
                    "CurrentTemperature",
                    "/value",
                ],
            ) {
                let temps: Vec<i64> = out
                    .lines()
                    .filter_map(|l| l.trim().strip_prefix("CurrentTemperature="))
                    .filter_map(|v| v.trim().parse::<i64>().ok())
                    .map(|deci_kelvin| deci_kelvin / 10 - 273)
                    .filter(|c| (0..=120).contains(c))
                    .collect();
                if let Some(max) = temps.iter().max() {
                    return *max;
                }
            }
            return -1;
        }
        -1
    })
}

// ===========================================================================
// RAM
// ===========================================================================

/// (total_mb, available_mb); (-1, -1) when unavailable.
pub fn ram_stats_mb() -> (i64, i64) {
    static SLOT: CacheSlot<(i64, i64)> = CacheSlot::new();
    SLOT.get_or(1000, || {
        if cfg!(target_os = "linux") {
            if let Ok(text) = std::fs::read_to_string("/proc/meminfo") {
                let field = |key: &str| -> Option<i64> {
                    text.lines()
                        .find(|l| l.starts_with(key))
                        .and_then(|l| l.split_whitespace().nth(1))
                        .and_then(|v| v.parse::<i64>().ok())
                        .map(|kb| kb / 1024)
                };
                if let (Some(total), Some(avail)) = (field("MemTotal:"), field("MemAvailable:")) {
                    return (total, avail);
                }
            }
            return (-1, -1);
        }
        if cfg!(windows) {
            if let Some(out) = run_cmd(
                "wmic",
                &["OS", "get", "FreePhysicalMemory,TotalVisibleMemorySize", "/value"],
            ) {
                let mut free_kb = -1i64;
                let mut total_kb = -1i64;
                for line in out.lines() {
                    let line = line.trim();
                    if let Some(v) = line.strip_prefix("FreePhysicalMemory=") {
                        free_kb = v.trim().parse().unwrap_or(-1);
                    } else if let Some(v) = line.strip_prefix("TotalVisibleMemorySize=") {
                        total_kb = v.trim().parse().unwrap_or(-1);
                    }
                }
                if free_kb > 0 && total_kb > 0 {
                    return (total_kb / 1024, free_kb / 1024);
                }
            }
            return (-1, -1);
        }
        if cfg!(target_os = "macos") {
            let total = run_cmd("sysctl", &["-n", "hw.memsize"])
                .and_then(|o| o.trim().parse::<i64>().ok())
                .map(|b| b / 1024 / 1024)
                .unwrap_or(-1);
            let avail = run_cmd("vm_stat", &[]).and_then(|out| {
                let mut page_size: i64 = 16384;
                if let Some(first) = out.lines().next() {
                    if let Some(idx) = first.find("page size of") {
                        let digits: String = first[idx..]
                            .chars()
                            .filter(|c| c.is_ascii_digit())
                            .collect();
                        page_size = digits.parse().unwrap_or(16384);
                    }
                }
                let grab = |key: &str| -> i64 {
                    out.lines()
                        .find(|l| l.starts_with(key))
                        .map(|l| {
                            let digits: String =
                                l.chars().filter(|c| c.is_ascii_digit()).collect();
                            digits.parse().unwrap_or(0)
                        })
                        .unwrap_or(0)
                };
                let pages = grab("Pages free:") + grab("Pages inactive:");
                Some(pages * page_size / 1024 / 1024)
            });
            return (total, avail.unwrap_or(-1));
        }
        (-1, -1)
    })
}

pub fn process_mb() -> i64 {
    (crate::mem::process_rss_bytes() / (1024 * 1024)) as i64
}

// ===========================================================================
// GPU
// ===========================================================================

#[derive(Clone, Debug)]
pub struct GpuInfo {
    pub index: i64,
    pub name: String,
    pub vendor: String,
    pub vram_total_mb: i64,
    pub vram_used_mb: i64,
    pub util_pct: i64,
    pub temp_c: i64,
}

#[derive(Clone)]
pub struct GpuSnapshot {
    pub backend: &'static str,
    pub gpus: Vec<GpuInfo>,
}

fn probe_nvidia() -> Option<Vec<GpuInfo>> {
    let out = run_cmd(
        "nvidia-smi",
        &[
            "--query-gpu=index,name,memory.total,memory.used,utilization.gpu,temperature.gpu",
            "--format=csv,noheader,nounits",
        ],
    )?;
    let mut gpus = Vec::new();
    for line in out.lines() {
        let cols: Vec<&str> = line.split(',').map(|c| c.trim()).collect();
        if cols.len() < 6 {
            continue;
        }
        let num = |s: &str| -> i64 { s.parse::<f64>().map(|v| v as i64).unwrap_or(-1) };
        gpus.push(GpuInfo {
            index: num(cols[0]),
            name: cols[1].to_string(),
            vendor: "nvidia".to_string(),
            vram_total_mb: num(cols[2]),
            vram_used_mb: num(cols[3]),
            util_pct: num(cols[4]),
            temp_c: num(cols[5]),
        });
    }
    if gpus.is_empty() {
        None
    } else {
        Some(gpus)
    }
}

fn probe_rocm() -> Option<Vec<GpuInfo>> {
    // rocm-smi CSV output: one line per card; readings best-effort.
    let out = run_cmd("rocm-smi", &["--showtemp", "--showuse", "--csv"])?;
    let mut gpus = Vec::new();
    for line in out.lines().skip(1) {
        if !line.to_ascii_lowercase().contains("card") {
            continue;
        }
        let cols: Vec<&str> = line.split(',').map(|c| c.trim()).collect();
        let idx = gpus.len() as i64;
        let mut temp = -1i64;
        let mut util = -1i64;
        for col in &cols[1..] {
            let cleaned: String = col
                .chars()
                .filter(|c| c.is_ascii_digit() || *c == '.')
                .collect();
            if cleaned.is_empty() {
                continue;
            }
            if let Ok(v) = cleaned.parse::<f64>() {
                if temp < 0 && (10.0..=120.0).contains(&v) {
                    temp = v as i64;
                } else if util < 0 && (0.0..=100.0).contains(&v) {
                    util = v as i64;
                }
            }
        }
        gpus.push(GpuInfo {
            index: idx,
            name: format!("AMD GPU {idx}"),
            vendor: "amd".to_string(),
            vram_total_mb: -1,
            vram_used_mb: -1,
            util_pct: util,
            temp_c: temp,
        });
    }
    if gpus.is_empty() {
        None
    } else {
        Some(gpus)
    }
}

/// Detection-only fallback: names without live stats.
fn probe_generic() -> Vec<GpuInfo> {
    let mut gpus = Vec::new();
    if cfg!(windows) {
        if let Some(out) = run_cmd("wmic", &["path", "win32_VideoController", "get", "Name", "/value"]) {
            for line in out.lines() {
                if let Some(name) = line.trim().strip_prefix("Name=") {
                    let name = name.trim();
                    if name.is_empty() {
                        continue;
                    }
                    let lower = name.to_ascii_lowercase();
                    let vendor = if lower.contains("nvidia") {
                        "nvidia"
                    } else if lower.contains("amd") || lower.contains("radeon") {
                        "amd"
                    } else if lower.contains("intel") {
                        "intel"
                    } else {
                        "unknown"
                    };
                    gpus.push(GpuInfo {
                        index: gpus.len() as i64,
                        name: name.to_string(),
                        vendor: vendor.to_string(),
                        vram_total_mb: -1,
                        vram_used_mb: -1,
                        util_pct: -1,
                        temp_c: -1,
                    });
                }
            }
        }
    } else if cfg!(target_os = "linux") {
        if let Some(out) = run_cmd("lspci", &[]) {
            for line in out.lines() {
                if line.contains("VGA") || line.contains("3D controller") {
                    let name = line.split(':').next_back().unwrap_or(line).trim();
                    let lower = name.to_ascii_lowercase();
                    let vendor = if lower.contains("nvidia") {
                        "nvidia"
                    } else if lower.contains("amd") || lower.contains("radeon") {
                        "amd"
                    } else if lower.contains("intel") {
                        "intel"
                    } else {
                        "unknown"
                    };
                    gpus.push(GpuInfo {
                        index: gpus.len() as i64,
                        name: name.to_string(),
                        vendor: vendor.to_string(),
                        vram_total_mb: -1,
                        vram_used_mb: -1,
                        util_pct: -1,
                        temp_c: -1,
                    });
                }
            }
        }
    } else if cfg!(target_os = "macos") && cfg!(target_arch = "aarch64") {
        gpus.push(GpuInfo {
            index: 0,
            name: "Apple Silicon GPU".to_string(),
            vendor: "apple".to_string(),
            vram_total_mb: -1,
            vram_used_mb: -1,
            util_pct: -1,
            temp_c: -1,
        });
    }
    gpus
}

static GPU_SLOT: CacheSlot<(&'static str, Vec<GpuInfo>)> = CacheSlot::new();

pub fn gpu_snapshot() -> GpuSnapshot {
    let (backend, gpus) = GPU_SLOT.get_or(1000, || {
        if let Some(gpus) = probe_nvidia() {
            return ("nvidia-smi", gpus);
        }
        if let Some(gpus) = probe_rocm() {
            return ("rocm-smi", gpus);
        }
        ("detect-only", probe_generic())
    });
    GpuSnapshot { backend, gpus }
}

/// Drop the cached GPU snapshot so the next read re-probes immediately.
pub fn gpu_refresh() {
    GPU_SLOT.invalidate();
}

// ===========================================================================
// NPU
// ===========================================================================

#[derive(Clone)]
pub struct NpuInfo {
    pub present: bool,
    pub vendor: String,
    pub name: String,
    pub note: String,
}

pub fn npu_detect() -> NpuInfo {
    static SLOT: CacheSlot<(bool, String, String, String)> = CacheSlot::new();
    let (present, vendor, name, note) = SLOT.get_or(3_600_000, || {
        if cfg!(target_os = "macos") && cfg!(target_arch = "aarch64") {
            return (
                true,
                "apple".into(),
                "Apple Neural Engine".into(),
                "Detected via Apple Silicon; scheduled by Core ML/OS.".into(),
            );
        }
        if cfg!(target_os = "linux") {
            if std::path::Path::new("/dev/accel").exists()
                || std::path::Path::new("/sys/class/accel").exists()
            {
                return (
                    true,
                    "intel".into(),
                    "Linux accel device (NPU)".into(),
                    "Detected /dev/accel or /sys/class/accel.".into(),
                );
            }
        }
        let brand = cpu_brand().to_ascii_lowercase();
        if brand.contains("core(tm) ultra") || brand.contains("core ultra") {
            return (
                true,
                "intel".into(),
                "Intel AI Boost NPU".into(),
                "Inferred from CPU brand (Core Ultra).".into(),
            );
        }
        if brand.contains("snapdragon") {
            return (
                true,
                "qualcomm".into(),
                "Qualcomm Hexagon NPU".into(),
                "Inferred from CPU brand (Snapdragon).".into(),
            );
        }
        if brand.contains("ryzen ai") {
            return (
                true,
                "amd".into(),
                "AMD Ryzen AI NPU".into(),
                "Inferred from CPU brand (Ryzen AI).".into(),
            );
        }
        (
            false,
            "none".into(),
            "".into(),
            "No NPU detected on this system.".into(),
        )
    });
    NpuInfo {
        present,
        vendor,
        name,
        note,
    }
}

// ===========================================================================
// Budgets & throttle (global, atomic)
// ===========================================================================

pub static CPU_LIMIT_PCT: AtomicU8 = AtomicU8::new(100);
pub static GPU_LIMIT_PCT: AtomicU8 = AtomicU8::new(100);
pub static NPU_LIMIT_PCT: AtomicU8 = AtomicU8::new(100);
/// 0 = no MB limit.
pub static RAM_LIMIT_MB: AtomicI64 = AtomicI64::new(0);
/// 0 = no percent limit.
pub static RAM_LIMIT_PCT: AtomicU8 = AtomicU8::new(0);
/// 0 = thermal guard off for that device.
pub static GPU_MAX_TEMP_C: AtomicU8 = AtomicU8::new(0);
pub static CPU_MAX_TEMP_C: AtomicU8 = AtomicU8::new(0);
/// 0 ok · 1 warm · 2 hot · 3 critical.
pub static THROTTLE_LEVEL: AtomicU8 = AtomicU8::new(0);

fn throttle_reason_slot() -> &'static Mutex<String> {
    static SLOT: OnceLock<Mutex<String>> = OnceLock::new();
    SLOT.get_or_init(|| Mutex::new(String::from("ok")))
}

pub fn set_throttle(level: u8, reason: &str) {
    THROTTLE_LEVEL.store(level.min(3), Ordering::Relaxed);
    if let Ok(mut r) = throttle_reason_slot().lock() {
        *r = reason.to_string();
    }
}

pub fn throttle_level() -> u8 {
    THROTTLE_LEVEL.load(Ordering::Relaxed)
}

pub fn throttle_reason() -> String {
    throttle_reason_slot()
        .lock()
        .map(|r| r.clone())
        .unwrap_or_else(|_| "ok".to_string())
}

/// Worker count under CPU limit + current throttle. Always >= 1.
pub fn allowed_threads() -> usize {
    let base = logical_cores();
    let pct = CPU_LIMIT_PCT.load(Ordering::Relaxed).clamp(1, 100) as usize;
    let mut n = (base * pct).div_euclid(100).max(1);
    match throttle_level() {
        2 => n = (n / 2).max(1),
        3 => n = 1,
        _ => {}
    }
    n
}

/// Cooperative pacing: sleep according to throttle level.
/// Level 0 → no sleep · 1 → 2 ms · 2 → 8 ms · 3 → 25 ms.
pub fn pace() {
    let ms = match throttle_level() {
        1 => 2,
        2 => 8,
        3 => 25,
        _ => return,
    };
    std::thread::sleep(Duration::from_millis(ms));
}

/// Effective RAM budget for this process in MB (0 = unlimited).
pub fn ram_budget_mb() -> i64 {
    let mb = RAM_LIMIT_MB.load(Ordering::Relaxed);
    let pct = RAM_LIMIT_PCT.load(Ordering::Relaxed) as i64;
    let (total, _) = ram_stats_mb();
    let from_pct = if pct > 0 && total > 0 {
        total * pct / 100
    } else {
        0
    };
    match (mb > 0, from_pct > 0) {
        (true, true) => mb.min(from_pct),
        (true, false) => mb,
        (false, true) => from_pct,
        (false, false) => 0,
    }
}

/// Whether `extra_mb` more memory would fit inside budget and system headroom.
pub fn ram_ok(extra_mb: i64) -> bool {
    let budget = ram_budget_mb();
    if budget > 0 && process_mb() + extra_mb > budget {
        return false;
    }
    let (_, avail) = ram_stats_mb();
    if avail >= 0 && avail - extra_mb < 256 {
        return false;
    }
    true
}

/// "low" | "medium" | "high" | "critical" system memory pressure.
pub fn ram_pressure() -> &'static str {
    let (total, avail) = ram_stats_mb();
    if total <= 0 || avail < 0 {
        return "unknown";
    }
    let used_pct = (total - avail) * 100 / total.max(1);
    match used_pct {
        0..=69 => "low",
        70..=84 => "medium",
        85..=94 => "high",
        _ => "critical",
    }
}

// ===========================================================================
// Safety guard thread
// ===========================================================================

#[derive(Clone, Default)]
pub struct GuardStatus {
    pub running: bool,
    pub interval_ms: u64,
    pub ticks: u64,
    pub level: u8,
    pub reason: String,
    pub gpu_temp_c: i64,
    pub cpu_temp_c: i64,
    pub ram_used_pct: i64,
}

static GUARD_RUNNING: AtomicBool = AtomicBool::new(false);
static GUARD_STOP: AtomicBool = AtomicBool::new(false);
static GUARD_TICKS: AtomicU64 = AtomicU64::new(0);

fn guard_status_slot() -> &'static Mutex<GuardStatus> {
    static SLOT: OnceLock<Mutex<GuardStatus>> = OnceLock::new();
    SLOT.get_or_init(|| Mutex::new(GuardStatus::default()))
}

pub fn guard_running() -> bool {
    GUARD_RUNNING.load(Ordering::Relaxed)
}

pub fn guard_status() -> GuardStatus {
    let mut st = guard_status_slot()
        .lock()
        .map(|s| s.clone())
        .unwrap_or_default();
    st.running = guard_running();
    st.ticks = GUARD_TICKS.load(Ordering::Relaxed);
    st.level = throttle_level();
    st.reason = throttle_reason();
    st
}

fn level_for(temp: i64, max: u8) -> u8 {
    if max == 0 || temp < 0 {
        return 0;
    }
    let max = max as i64;
    if temp >= max + 5 {
        3
    } else if temp >= max {
        2
    } else if temp >= max - 10 {
        1
    } else {
        0
    }
}

fn guard_tick() {
    let gpu_max = GPU_MAX_TEMP_C.load(Ordering::Relaxed);
    let cpu_max = CPU_MAX_TEMP_C.load(Ordering::Relaxed);

    let snapshot = gpu_snapshot();
    let gpu_temp = snapshot.gpus.iter().map(|g| g.temp_c).max().unwrap_or(-1);
    let cpu_temp = cpu_temp_c();
    let (total, avail) = ram_stats_mb();
    let ram_used_pct = if total > 0 && avail >= 0 {
        (total - avail) * 100 / total
    } else {
        -1
    };

    let mut level = 0u8;
    let mut reason = String::from("ok");
    let g_level = level_for(gpu_temp, gpu_max);
    if g_level > level {
        level = g_level;
        reason = format!("gpu temperature {gpu_temp}C (max {gpu_max}C)");
    }
    let c_level = level_for(cpu_temp, cpu_max);
    if c_level > level {
        level = c_level;
        reason = format!("cpu temperature {cpu_temp}C (max {cpu_max}C)");
    }
    if ram_used_pct >= 0 {
        let ram_level: u8 = match ram_used_pct {
            0..=89 => 0,
            90..=95 => 1,
            _ => 2,
        };
        if ram_level > level {
            level = ram_level;
            reason = format!("system memory {ram_used_pct}% used");
        }
    }
    let budget = ram_budget_mb();
    if budget > 0 && process_mb() > budget {
        let over = 2u8;
        if over > level {
            level = over;
            reason = format!("process memory {}MB over budget {}MB", process_mb(), budget);
        }
    }
    set_throttle(level, &reason);
    GUARD_TICKS.fetch_add(1, Ordering::Relaxed);
    if let Ok(mut st) = guard_status_slot().lock() {
        st.gpu_temp_c = gpu_temp;
        st.cpu_temp_c = cpu_temp;
        st.ram_used_pct = ram_used_pct;
    }
}

/// Start the background safety monitor. Returns false if already running.
pub fn guard_start(interval_ms: u64) -> bool {
    if GUARD_RUNNING.swap(true, Ordering::SeqCst) {
        return false;
    }
    GUARD_STOP.store(false, Ordering::SeqCst);
    GUARD_TICKS.store(0, Ordering::Relaxed);
    if let Ok(mut st) = guard_status_slot().lock() {
        st.interval_ms = interval_ms;
    }
    std::thread::Builder::new()
        .name("niao-hw-guard".into())
        .spawn(move || {
            while !GUARD_STOP.load(Ordering::SeqCst) {
                guard_tick();
                let mut waited = 0u64;
                while waited < interval_ms && !GUARD_STOP.load(Ordering::SeqCst) {
                    let step = 50.min(interval_ms - waited);
                    std::thread::sleep(Duration::from_millis(step));
                    waited += step;
                }
            }
            GUARD_RUNNING.store(false, Ordering::SeqCst);
        })
        .map(|_| true)
        .unwrap_or_else(|_| {
            GUARD_RUNNING.store(false, Ordering::SeqCst);
            false
        })
}

pub fn guard_stop() {
    GUARD_STOP.store(true, Ordering::SeqCst);
    // Reset throttle so a stopped guard never leaves the app slowed down.
    set_throttle(0, "guard stopped");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn threads_respect_limit_and_throttle() {
        CPU_LIMIT_PCT.store(100, Ordering::Relaxed);
        set_throttle(0, "ok");
        let full = allowed_threads();
        assert!(full >= 1);
        CPU_LIMIT_PCT.store(50, Ordering::Relaxed);
        let half = allowed_threads();
        assert!(half <= full && half >= 1);
        set_throttle(3, "test");
        assert_eq!(allowed_threads(), 1);
        set_throttle(0, "ok");
        CPU_LIMIT_PCT.store(100, Ordering::Relaxed);
    }

    #[test]
    fn level_thresholds() {
        assert_eq!(level_for(-1, 80), 0);
        assert_eq!(level_for(60, 0), 0);
        assert_eq!(level_for(69, 80), 0);
        assert_eq!(level_for(72, 80), 1);
        assert_eq!(level_for(80, 80), 2);
        assert_eq!(level_for(85, 80), 3);
    }

    #[test]
    fn ram_budget_combines_limits() {
        RAM_LIMIT_MB.store(0, Ordering::Relaxed);
        RAM_LIMIT_PCT.store(0, Ordering::Relaxed);
        assert_eq!(ram_budget_mb(), 0);
        RAM_LIMIT_MB.store(1024, Ordering::Relaxed);
        assert_eq!(ram_budget_mb(), 1024);
        RAM_LIMIT_MB.store(0, Ordering::Relaxed);
    }

    #[test]
    fn guard_start_stop() {
        assert!(guard_start(100));
        assert!(guard_running());
        assert!(!guard_start(100)); // already running
        guard_stop();
        for _ in 0..50 {
            if !guard_running() {
                break;
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        assert!(!guard_running());
    }
}
