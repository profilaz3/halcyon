use crate::config::{DevboxPreferences, Location};
pub(crate) use hardware::{GpuMetrics, NetworkMetrics};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::sync::OnceLock;
use std::time::Duration;

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Weather {
    pub current: Current,
    pub daily: Daily,
}
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Current {
    pub temperature_2m: f64,
    pub apparent_temperature: f64,
    pub weather_code: u32,
    pub wind_speed_10m: f64,
    pub relative_humidity_2m: f64,
    /// Optional so older cache files and unavailable provider readings still load.
    #[serde(default)]
    pub dew_point_2m: Option<f64>,
    pub is_day: u8,
}
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Daily {
    pub time: Vec<String>,
    pub temperature_2m_max: Vec<f64>,
    pub temperature_2m_min: Vec<f64>,
    pub weather_code: Vec<u32>,
    pub precipitation_probability_max: Vec<Option<f64>>,
}
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct WeatherSnapshot {
    pub location: Location,
    pub updated_at: String,
    pub weather: Weather,
}
impl WeatherSnapshot {
    pub(crate) fn path() -> Option<PathBuf> {
        std::env::var_os("DESKTOP_CARDS_CACHE_DIR")
            .map(PathBuf::from)
            .or_else(crate::paths::cache_dir)
            .map(|dir| dir.join("weather.json"))
    }
    pub fn cached(location: &Location) -> Option<Self> {
        Self::read_at(&Self::path()?, location)
    }
    fn read_at(path: &Path, location: &Location) -> Option<Self> {
        let bytes = crate::tools::read_bounded(path, 256 * 1024).ok()??;
        let snapshot: Self = serde_json::from_slice(&bytes).ok()?;
        (snapshot.location == *location
            && chrono::DateTime::parse_from_rfc3339(&snapshot.updated_at).is_ok())
        .then_some(snapshot)
    }
    /// Owner-only: the cache names the chosen city and its coordinates.
    pub(crate) fn save_at(&self, path: &Path) -> Result<(), String> {
        let bytes = serde_json::to_vec(self).map_err(|e| e.to_string())?;
        crate::tools::write_private(path, &bytes)?;
        // A cache folder made by an older version may be readable by others.
        if let Some(parent) = path.parent() {
            use std::os::unix::fs::PermissionsExt;
            let _ = std::fs::set_permissions(parent, std::fs::Permissions::from_mode(0o700));
        }
        Ok(())
    }
}
pub async fn fetch_weather(loc: &Location) -> Result<WeatherSnapshot, String> {
    static CLIENT: OnceLock<Result<reqwest::Client, String>> = OnceLock::new();
    let client = CLIENT
        .get_or_init(|| {
            reqwest::Client::builder()
                .timeout(crate::providers::weather::DEADLINE)
                .build()
                .map_err(|e| e.to_string())
        })
        .as_ref()
        .map_err(Clone::clone)?;
    fetch_weather_from(client, "https://api.open-meteo.com/v1/forecast", loc).await
}

async fn fetch_weather_from(
    client: &reqwest::Client,
    endpoint: &str,
    loc: &Location,
) -> Result<WeatherSnapshot, String> {
    let mut response = client
        .get(endpoint)
        .query(&[
            ("latitude", loc.latitude.to_string()),
            ("longitude", loc.longitude.to_string()),
            (
                "current",
                "temperature_2m,apparent_temperature,weather_code,wind_speed_10m,relative_humidity_2m,dew_point_2m,is_day".into(),
            ),
            ("daily", "temperature_2m_max,temperature_2m_min,weather_code,precipitation_probability_max".into()),
            ("timezone", "auto".into()),
            ("forecast_days", "3".into()),
        ])
        .send()
        .await
        .map_err(|e| e.to_string())?
        .error_for_status()
        .map_err(|e| e.to_string())?;
    const MAX_BYTES: usize = 256 * 1024;
    if response
        .content_length()
        .is_some_and(|len| len > MAX_BYTES as u64)
    {
        return Err("Weather response is too large".into());
    }
    let mut bytes = Vec::new();
    while let Some(chunk) = response.chunk().await.map_err(|e| e.to_string())? {
        if chunk.len() > MAX_BYTES - bytes.len() {
            return Err("Weather response is too large".into());
        }
        bytes.extend_from_slice(&chunk);
    }
    Ok(WeatherSnapshot {
        location: loc.clone(),
        updated_at: chrono::Local::now().to_rfc3339(),
        weather: serde_json::from_slice(&bytes).map_err(|e| e.to_string())?,
    })
}

#[cfg(test)]
pub(crate) async fn test_fetch_weather(
    client: &reqwest::Client,
    endpoint: &str,
    location: &Location,
) -> Result<WeatherSnapshot, String> {
    fetch_weather_from(client, endpoint, location).await
}

pub fn weather_description(code: u32) -> &'static str {
    match code {
        0 => "Clear skies",
        1 => "Mostly clear",
        2 => "Partly cloudy",
        3 => "Overcast",
        45 | 48 => "Foggy",
        51 | 53 | 55 => "Drizzle",
        56 | 57 => "Freezing drizzle",
        61 | 63 | 65 => "Rain",
        66 | 67 => "Freezing rain",
        71 | 73 | 75 | 77 => "Snow",
        80..=82 => "Rain showers",
        85 | 86 => "Snow showers",
        95..=99 => "Thunderstorms",
        _ => "Weather update",
    }
}

#[derive(Debug, Clone, Default)]
pub struct Metrics {
    pub cpu: f32,
    /// Whether this poll produced a CPU delta, rather than a retained reading.
    pub cpu_sampled: bool,
    pub cpu_temperature: Option<f32>,
    pub memory_used: f32,
    pub memory_cache: f32,
    pub memory_free: f32,
    pub memory_total: f32,
    /// Whether this poll produced a complete memory breakdown.
    pub memory_sampled: bool,
    /// Used and total swap in GiB; a zero total means no swap is configured.
    pub swap: Option<(f32, f32)>,
    pub disk: Option<(f64, f64)>,
    pub network: Option<hardware::NetworkMetrics>,
    pub gpus: Vec<hardware::GpuMetrics>,
    pub uptime: String,
    pub t3: bool,
    pub dictation: String,
}
#[derive(Debug, Clone, Default)]
pub struct Sampler {
    previous_cpu: Option<(u64, u64)>,
    previous_cpu_at: Option<std::time::Instant>,
    temperature_input: Option<PathBuf>,
    temperature_checked: Option<std::time::Instant>,
    network: hardware::NetworkSampler,
    gpu: hardware::GpuSampler,
}
impl Sampler {
    fn sample_cpu(&mut self, metrics: &mut Metrics, stat: Option<&str>, at: std::time::Instant) {
        metrics.cpu_sampled = false;
        let current = stat.and_then(|stat| {
            let mut fields = stat.lines().next()?.split_whitespace();
            if fields.next()? != "cpu" {
                return None;
            }
            let values = fields
                .take(8)
                .map(str::parse::<u64>)
                .collect::<Result<Vec<_>, _>>()
                .ok()?;
            if values.len() < 5 {
                return None;
            }
            Some((
                values
                    .iter()
                    .try_fold(0u64, |sum, value| sum.checked_add(*value))?,
                values[3].checked_add(values[4])?,
            ))
        });
        if let Some(now) = current {
            if let Some(old) = self.previous_cpu
                && self.previous_cpu_at.is_some_and(|previous| {
                    at.saturating_duration_since(previous) <= Duration::from_secs(120)
                })
                && now.0 > old.0
                && now.1 >= old.1
                && now.1 - old.1 <= now.0 - old.0
            {
                metrics.cpu = cpu_usage(old, now);
                metrics.cpu_sampled = true;
            }
            self.previous_cpu = Some(now);
            self.previous_cpu_at = Some(at);
        } else {
            self.previous_cpu = None;
            self.previous_cpu_at = None;
        }
    }

    fn sample_memory(metrics: &mut Metrics, info: Option<&str>, swap: bool) {
        metrics.memory_sampled = false;
        metrics.swap = None;
        if let Some(info) = info {
            if let Some(memory) = MemoryBreakdown::from_meminfo(info) {
                metrics.memory_total = memory.total;
                metrics.memory_used = memory.used;
                metrics.memory_cache = memory.cache;
                metrics.memory_free = memory.free;
                metrics.memory_sampled = true;
            }
            if swap {
                metrics.swap = swap_usage(info);
            }
        }
    }

    pub fn sample(&mut self, previous: &Metrics, preferences: &DevboxPreferences) -> Metrics {
        let mut m = previous.clone();
        let stat = preferences
            .cpu
            .then(|| std::fs::read_to_string("/proc/stat").ok())
            .flatten();
        self.sample_cpu(&mut m, stat.as_deref(), std::time::Instant::now());
        let info = preferences
            .memory
            .then(|| std::fs::read_to_string("/proc/meminfo").ok())
            .flatten();
        Self::sample_memory(&mut m, info.as_deref(), preferences.swap);
        if preferences.cpu {
            if self
                .temperature_checked
                .is_none_or(|at| at.elapsed() >= Duration::from_secs(60))
            {
                self.temperature_input = cpu_temperature_input();
                self.temperature_checked = Some(std::time::Instant::now());
            }
            m.cpu_temperature = self
                .temperature_input
                .as_ref()
                .and_then(|path| std::fs::read_to_string(path).ok())
                .and_then(|value| value.trim().parse::<f32>().ok())
                .map(|value| value / 1000.)
                .filter(|value| value.is_finite() && (-40. ..=150.).contains(value));
        }
        m.disk = preferences.disk.then(disk_usage).flatten();
        m.network = if preferences.network {
            self.network.sample()
        } else {
            self.network = hardware::NetworkSampler::default();
            None
        };
        m.gpus = if preferences.gpu {
            self.gpu.sample()
        } else {
            self.gpu = hardware::GpuSampler::default();
            Vec::new()
        };
        if preferences.uptime
            && let Ok(v) = std::fs::read_to_string("/proc/uptime")
        {
            let mins = v
                .split_whitespace()
                .next()
                .and_then(|v| v.parse::<f64>().ok())
                .unwrap_or(0.) as u64
                / 60;
            m.uptime = if mins >= 1440 {
                format!("{}d {}h", mins / 1440, mins % 1440 / 60)
            } else {
                format!("{}h {}m", mins / 60, mins % 60)
            };
        }
        if preferences.services {
            m.dictation = std::env::var("XDG_RUNTIME_DIR")
                .ok()
                .and_then(|p| std::fs::read_to_string(format!("{p}/voxtype/state")).ok())
                .map(|s| s.trim().to_string())
                .unwrap_or_else(|| "offline".into());
        }
        m
    }
}
mod hardware {
    use std::collections::{BTreeMap, BTreeSet};
    use std::path::{Path, PathBuf};
    use std::time::{Duration, Instant};

    #[derive(Debug, Clone, Default)]
    pub struct NetworkMetrics {
        pub download: Option<f64>,
        pub upload: Option<f64>,
        pub interfaces: usize,
    }

    #[derive(Debug, Clone, Default)]
    pub struct GpuMetrics {
        pub id: String,
        pub name: String,
        pub utilization: Option<f32>,
        /// Preserved when a bounded NVIDIA query result is reused.
        pub utilization_at: Option<Instant>,
        pub memory: Option<(u64, u64)>,
        pub temperature: Option<f32>,
        pub asleep: bool,
    }

    #[derive(Debug, Clone, Copy)]
    struct NetworkCounter {
        index: u64,
        received: u64,
        sent: u64,
    }

    #[derive(Debug, Clone, Default)]
    pub(super) struct NetworkSampler {
        previous: Option<(Instant, BTreeMap<String, NetworkCounter>)>,
    }
    impl NetworkSampler {
        pub(super) fn sample(&mut self) -> Option<NetworkMetrics> {
            let Ok(stat) = std::fs::read_to_string("/proc/net/dev") else {
                self.previous = None;
                return None;
            };
            let current = network_counters(&stat, Path::new("/sys/class/net"));
            let now = Instant::now();
            let rates = self.previous.as_ref().and_then(|(at, previous)| {
                network_rates(previous, &current, now.duration_since(*at))
            });
            let interfaces = current.len();
            self.previous = Some((now, current));
            Some(NetworkMetrics {
                download: rates.map(|(download, _)| download),
                upload: rates.map(|(_, upload)| upload),
                interfaces,
            })
        }
    }

    fn network_rates(
        previous: &BTreeMap<String, NetworkCounter>,
        current: &BTreeMap<String, NetworkCounter>,
        elapsed: Duration,
    ) -> Option<(f64, f64)> {
        // A hidden card or a resumed machine needs a new baseline, not an old
        // average presented as live traffic. The longest normal interval is 60s.
        if elapsed.is_zero() || elapsed > Duration::from_secs(120) {
            return None;
        }
        let mut total = (0., 0.);
        let mut matched = false;
        for (name, now) in current {
            let Some(old) = previous.get(name).filter(|old| old.index == now.index) else {
                continue;
            };
            let (Some(received), Some(sent)) = (
                now.received.checked_sub(old.received),
                now.sent.checked_sub(old.sent),
            ) else {
                continue;
            };
            total.0 += received as f64 / elapsed.as_secs_f64();
            total.1 += sent as f64 / elapsed.as_secs_f64();
            matched = true;
        }
        matched.then_some(total)
    }

    fn network_counters(stat: &str, sys: &Path) -> BTreeMap<String, NetworkCounter> {
        let mut samples = BTreeMap::new();
        let mut seen = BTreeSet::new();
        for line in stat.lines() {
            let Some((name, values)) = line.split_once(':') else {
                continue;
            };
            let name = name.trim();
            let path = sys.join(name);
            // Sample physical links only. Bridges, bonds, VPNs and containers
            // account for bytes already counted on the underlying physical link.
            if name == "lo"
                || !path.join("device").exists()
                || read_text(&path.join("operstate")).as_deref() != Some("up")
            {
                continue;
            }
            let Some(index) = read_u64(&path.join("ifindex")) else {
                continue;
            };
            if read_u64(&path.join("iflink")) != Some(index) || !seen.insert(index) {
                continue;
            }
            let mut values = values.split_whitespace();
            let received = values.next().and_then(|value| value.parse().ok());
            let sent = values.nth(7).and_then(|value| value.parse().ok());
            if let (Some(received), Some(sent)) = (received, sent) {
                samples.insert(
                    name.into(),
                    NetworkCounter {
                        index,
                        received,
                        sent,
                    },
                );
            }
        }
        samples
    }

    #[derive(Debug, Clone)]
    struct GpuDevice {
        path: PathBuf,
        id: String,
        name: String,
        vendor: u64,
        temperature: Option<PathBuf>,
    }

    fn discover_gpus(root: &Path) -> Vec<GpuDevice> {
        let Ok(entries) = std::fs::read_dir(root) else {
            return Vec::new();
        };
        let mut cards: Vec<_> = entries.flatten().map(|entry| entry.path()).collect();
        cards.sort();
        let mut seen = BTreeSet::new();
        let mut devices = Vec::new();
        for card in cards {
            let card_name = card.file_name().unwrap_or_default().to_string_lossy();
            if !card_name.strip_prefix("card").is_some_and(|suffix| {
                !suffix.is_empty() && suffix.bytes().all(|byte| byte.is_ascii_digit())
            }) {
                continue;
            }
            let Ok(path) = card.join("device").canonicalize() else {
                continue;
            };
            if !seen.insert(path.clone()) {
                continue;
            }
            let vendor = read_hex(&path.join("vendor")).unwrap_or(0);
            let name = read_text(&path.join("product_name"))
                .filter(|name| !name.is_empty())
                .or_else(|| pci_device_name(vendor, read_hex(&path.join("device"))?))
                .unwrap_or_else(|| match vendor {
                    0x1002 => "AMD graphics".into(),
                    0x8086 => "Intel graphics".into(),
                    0x10de => "NVIDIA graphics".into(),
                    _ => format!("Graphics · {card_name}"),
                });
            let temperature = std::fs::read_dir(path.join("hwmon"))
                .ok()
                .into_iter()
                .flatten()
                .flatten()
                .map(|entry| entry.path().join("temp1_input"))
                .find(|path| path.is_file());
            let id = path
                .file_name()
                .and_then(|name| name.to_str())
                .and_then(pci_bus_id)
                .unwrap_or_else(|| path.to_string_lossy().into_owned());
            devices.push(GpuDevice {
                path,
                id,
                name,
                vendor,
                temperature,
            });
        }
        devices
    }

    fn sample_gpu(device: &GpuDevice) -> GpuMetrics {
        let mut gpu = GpuMetrics {
            id: device.id.clone(),
            name: device.name.clone(),
            asleep: read_text(&device.path.join("power/runtime_status")).as_deref()
                == Some("suspended"),
            ..GpuMetrics::default()
        };
        // Reading some drivers' telemetry wakes a runtime-suspended discrete GPU.
        if gpu.asleep {
            return gpu;
        }
        gpu.utilization = read_text(&device.path.join("gpu_busy_percent"))
            .and_then(|value| valid_number(&value, 0., 100.));
        gpu.utilization_at = gpu.utilization.map(|_| Instant::now());
        gpu.memory = read_u64(&device.path.join("mem_info_vram_used"))
            .zip(read_u64(&device.path.join("mem_info_vram_total")))
            .filter(|(used, total)| *total > 0 && used <= total);
        gpu.temperature = device
            .temperature
            .as_ref()
            .and_then(|path| read_text(path))
            .and_then(|value| valid_number(&value, -40_000., 150_000.))
            .map(|value| value / 1_000.);
        gpu
    }

    fn read_text(path: &Path) -> Option<String> {
        std::fs::read_to_string(path)
            .ok()
            .map(|value| value.trim().into())
    }
    fn read_u64(path: &Path) -> Option<u64> {
        read_text(path)?.parse().ok()
    }
    fn read_hex(path: &Path) -> Option<u64> {
        u64::from_str_radix(read_text(path)?.trim_start_matches("0x"), 16).ok()
    }
    fn valid_number(value: &str, min: f32, max: f32) -> Option<f32> {
        value
            .parse::<f32>()
            .ok()
            .filter(|number| number.is_finite() && (min..=max).contains(number))
    }
    fn pci_bus_id(value: &str) -> Option<String> {
        let (address, function) = value.trim().split_once('.')?;
        let parts: Vec<_> = address.split(':').collect();
        if parts.len() != 3 {
            return None;
        }
        let hex = |value: &str| u32::from_str_radix(value, 16).ok();
        Some(format!(
            "{:04x}:{:02x}:{:02x}.{:x}",
            hex(parts[0])?,
            hex(parts[1])?,
            hex(parts[2])?,
            hex(function)?
        ))
    }
    fn pci_device_name(vendor: u64, device: u64) -> Option<String> {
        static IDS: std::sync::OnceLock<Option<String>> = std::sync::OnceLock::new();
        let ids = IDS
            .get_or_init(|| {
                [
                    "/usr/share/hwdata/pci.ids",
                    "/usr/share/misc/pci.ids",
                    "/usr/share/pci.ids",
                ]
                .iter()
                .find_map(|path| std::fs::read_to_string(path).ok())
            })
            .as_ref()?;
        let prefix = format!("{vendor:04x}  ");
        let device_prefix = format!("\t{device:04x}  ");
        let mut found_vendor = false;
        for line in ids.lines() {
            if line.starts_with('#') || line.is_empty() {
                continue;
            }
            if !found_vendor {
                found_vendor = line.starts_with(&prefix);
            } else if !line.starts_with('\t') {
                break;
            } else if let Some(name) = line.strip_prefix(&device_prefix) {
                return Some(name.trim().into());
            }
        }
        None
    }

    fn parse_nvidia_csv(text: &str) -> BTreeMap<String, GpuMetrics> {
        text.lines()
            .filter_map(|line| {
                let fields: Vec<_> = line.split(',').map(str::trim).collect();
                if fields.len() != 6 || fields[1].is_empty() {
                    return None;
                }
                let mib = |value: &str| value.parse::<u64>().ok()?.checked_mul(1_048_576);
                let id = pci_bus_id(fields[0])?;
                let utilization = valid_number(fields[2], 0., 100.);
                Some((
                    id.clone(),
                    GpuMetrics {
                        id,
                        name: fields[1].into(),
                        utilization,
                        utilization_at: utilization.map(|_| Instant::now()),
                        memory: mib(fields[3])
                            .zip(mib(fields[4]))
                            .filter(|(used, total)| *total > 0 && used <= total),
                        temperature: valid_number(fields[5], -40., 150.),
                        asleep: false,
                    },
                ))
            })
            .collect()
    }

    #[derive(Debug, Clone, Default)]
    pub(super) struct GpuSampler {
        devices: Vec<GpuDevice>,
        next_discovery: Option<Instant>,
        nvidia_command: Option<PathBuf>,
        nvidia_cache: BTreeMap<String, GpuMetrics>,
        next_nvidia: Option<Instant>,
    }
    impl GpuSampler {
        pub(super) fn sample(&mut self) -> Vec<GpuMetrics> {
            let now = Instant::now();
            if self.next_discovery.is_none_or(|at| now >= at) {
                self.devices = discover_gpus(Path::new("/sys/class/drm"));
                self.next_discovery = Some(now + Duration::from_secs(60));
                self.nvidia_command = if self.devices.iter().any(|device| device.vendor == 0x10de) {
                    std::env::var_os("PATH").and_then(|path| {
                        std::env::split_paths(&path)
                            .map(|dir| dir.join("nvidia-smi"))
                            .find(|path| path.is_file())
                    })
                } else {
                    None
                };
            }
            self.devices.retain(|device| device.path.is_dir());
            let mut samples: Vec<_> = self.devices.iter().map(sample_gpu).collect();
            let active_nvidia: Vec<_> = self
                .devices
                .iter()
                .zip(&samples)
                .filter(|(device, gpu)| {
                    device.vendor == 0x10de && !gpu.asleep && gpu.utilization.is_none()
                })
                .map(|(device, _)| device.id.as_str())
                .collect();
            if !active_nvidia.is_empty()
                && self.next_nvidia.is_none_or(|at| now >= at)
                && let Some(command) = &self.nvidia_command
            {
                // Only the blocking metrics worker reaches this fallback. One
                // bounded query covers active NVIDIA cards, at most twice/minute.
                let result = std::process::Command::new("/usr/bin/timeout")
                    .args(["--kill-after=1s", "2s"]).arg(command)
                    .arg(format!("--id={}", active_nvidia.join(",")))
                    .args([
                        "--query-gpu=pci.bus_id,name,utilization.gpu,memory.used,memory.total,temperature.gpu",
                        "--format=csv,noheader,nounits",
                    ])
                    .stderr(std::process::Stdio::null()).output()
                    .ok().filter(|output| output.status.success())
                    .and_then(|output| String::from_utf8(output.stdout).ok());
                self.nvidia_cache = result.as_deref().map(parse_nvidia_csv).unwrap_or_default();
                self.next_nvidia =
                    Some(now + Duration::from_secs(if result.is_some() { 30 } else { 60 }));
            }
            for (device, gpu) in self.devices.iter().zip(&mut samples) {
                if device.vendor == 0x10de
                    && !gpu.asleep
                    && gpu.utilization.is_none()
                    && let Some(cached) = self.nvidia_cache.get(&device.id)
                {
                    *gpu = cached.clone();
                }
            }
            samples
        }
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        struct HardwareTree(PathBuf);
        impl HardwareTree {
            fn new() -> Self {
                static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
                let path = std::env::temp_dir().join(format!(
                    "desktop-cards-hardware-{}-{}",
                    std::process::id(),
                    NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed),
                ));
                std::fs::create_dir_all(&path).unwrap();
                Self(path)
            }
            fn write(&self, relative: &str, value: &str) {
                let path = self.0.join(relative);
                std::fs::create_dir_all(path.parent().unwrap()).unwrap();
                std::fs::write(path, value).unwrap();
            }
        }
        impl Drop for HardwareTree {
            fn drop(&mut self) {
                let _ = std::fs::remove_dir_all(&self.0);
            }
        }

        fn counters(rows: &[(&str, u64, u64, u64)]) -> BTreeMap<String, NetworkCounter> {
            rows.iter()
                .map(|(name, index, received, sent)| {
                    (
                        (*name).into(),
                        NetworkCounter {
                            index: *index,
                            received: *received,
                            sent: *sent,
                        },
                    )
                })
                .collect()
        }

        #[test]
        fn network_throughput_uses_real_elapsed_time_and_each_interface_delta() {
            let before = counters(&[("wlan0", 3, 1_000, 2_000), ("eth0", 2, 100, 200)]);
            let after = counters(&[("wlan0", 3, 7_000, 6_000), ("eth0", 2, 2_100, 4_200)]);
            assert_eq!(
                network_rates(&before, &after, Duration::from_secs(2)),
                Some((4_000., 4_000.))
            );
            assert_eq!(
                network_rates(&before, &after, Duration::from_millis(500)),
                Some((16_000., 16_000.))
            );
        }

        #[test]
        fn network_reconnects_and_resets_never_become_traffic_spikes() {
            let before = counters(&[("wlan0", 3, 1_000, 2_000), ("eth0", 2, 100, 200)]);
            let after = counters(&[("wlan0", 3, 100, 4_000), ("eth0", 2, 300, 600)]);
            assert_eq!(
                network_rates(&before, &after, Duration::from_secs(2)),
                Some((100., 200.))
            );
            let replaced = counters(&[("wlan0", 4, 9_000_000, 8_000_000)]);
            assert_eq!(
                network_rates(&before, &replaced, Duration::from_secs(2)),
                None
            );
            assert_eq!(
                network_rates(&BTreeMap::new(), &after, Duration::from_secs(2)),
                None
            );
            assert_eq!(network_rates(&before, &after, Duration::ZERO), None);
            assert_eq!(
                network_rates(&before, &after, Duration::from_secs(600)),
                None
            );
        }

        #[test]
        fn network_excludes_down_virtual_and_stacked_interfaces() {
            let tree = HardwareTree::new();
            for (name, state, index, link, physical) in [
                ("wlan0", "up", 3, 3, true),
                ("eth0", "down", 2, 2, true),
                ("lo", "up", 1, 1, false),
                ("veth0", "up", 4, 4, false),
                ("vlan0", "up", 5, 3, true),
            ] {
                tree.write(&format!("{name}/operstate"), state);
                tree.write(&format!("{name}/ifindex"), &index.to_string());
                tree.write(&format!("{name}/iflink"), &link.to_string());
                if physical {
                    tree.write(&format!("{name}/device/vendor"), "0x1234");
                }
            }
            let stat = "Inter-| Receive | Transmit\n face |bytes packets |bytes packets\n\
                wlan0: 1234 1 0 0 0 0 0 0 5678 1 0 0 0 0 0 0\n\
                eth0: 9999 1 0 0 0 0 0 0 9999 1 0 0 0 0 0 0\n\
                lo: 9999 1 0 0 0 0 0 0 9999 1 0 0 0 0 0 0\n\
                veth0: 9999 1 0 0 0 0 0 0 9999 1 0 0 0 0 0 0\n\
                vlan0: 9999 1 0 0 0 0 0 0 9999 1 0 0 0 0 0 0\n";
            let samples = network_counters(stat, &tree.0);
            assert_eq!(samples.len(), 1);
            assert_eq!(samples["wlan0"].received, 1234);
            assert_eq!(samples["wlan0"].sent, 5678);
        }

        #[test]
        fn gpu_reads_real_sysfs_units_and_ignores_connector_entries() {
            let tree = HardwareTree::new();
            tree.write("card0/device/vendor", "0x1002");
            tree.write("card0/device/device", "0x1638");
            tree.write("card0/device/product_name", "AMD test graphics");
            tree.write("card0/device/gpu_busy_percent", "37");
            tree.write("card0/device/mem_info_vram_used", "268435456");
            tree.write("card0/device/mem_info_vram_total", "536870912");
            tree.write("card0/device/hwmon/hwmon0/temp1_input", "45000");
            tree.write("card0-DP-1/device/vendor", "0x1002");
            tree.write("renderD128/device/vendor", "0x1002");
            let devices = discover_gpus(&tree.0);
            assert_eq!(devices.len(), 1);
            let gpu = sample_gpu(&devices[0]);
            assert_eq!(gpu.id, devices[0].id);
            assert!(gpu.utilization_at.is_some());
            assert_eq!(gpu.name, "AMD test graphics");
            assert_eq!(gpu.utilization, Some(37.));
            assert_eq!(gpu.memory, Some((268_435_456, 536_870_912)));
            assert_eq!(gpu.temperature, Some(45.));
            tree.write("card0/device/power/runtime_status", "suspended");
            let sleeping = sample_gpu(&devices[0]);
            assert!(sleeping.asleep);
            assert!(sleeping.utilization.is_none() && sleeping.memory.is_none());
        }

        #[test]
        fn absent_or_unsupported_gpu_readings_stay_unavailable() {
            let tree = HardwareTree::new();
            assert!(discover_gpus(&tree.0).is_empty());
            assert!(discover_gpus(&tree.0.join("missing")).is_empty());
            tree.write("card0/device/vendor", "0x8086");
            tree.write("card0/device/device", "0x0001");
            tree.write("card0/device/gpu_busy_percent", "NaN");
            tree.write("card0/device/mem_info_vram_used", "123");
            tree.write("card0/device/mem_info_vram_total", "0");
            let devices = discover_gpus(&tree.0);
            assert_eq!(devices.len(), 1);
            let gpu = sample_gpu(&devices[0]);
            assert!(gpu.utilization.is_none() && gpu.memory.is_none() && gpu.temperature.is_none());
        }

        #[test]
        fn gpu_identity_survives_drm_card_renumbering() {
            let tree = HardwareTree::new();
            tree.write("devices/integrated/vendor", "0x1002");
            std::fs::create_dir_all(tree.0.join("card0")).unwrap();
            std::os::unix::fs::symlink("../devices/integrated", tree.0.join("card0/device"))
                .unwrap();
            let original = discover_gpus(&tree.0);
            std::fs::rename(tree.0.join("card0"), tree.0.join("card2")).unwrap();
            let renumbered = discover_gpus(&tree.0);
            assert_eq!(original.len(), 1);
            assert_eq!(renumbered.len(), 1);
            assert_eq!(original[0].id, renumbered[0].id);
        }

        #[test]
        fn disappeared_gpus_are_removed_without_waiting_for_discovery() {
            let tree = HardwareTree::new();
            tree.write("card0/device/vendor", "0x1002");
            tree.write("card0/device/gpu_busy_percent", "37");
            let mut sampler = GpuSampler {
                devices: discover_gpus(&tree.0),
                next_discovery: Some(Instant::now() + Duration::from_secs(60)),
                ..Default::default()
            };
            assert_eq!(sampler.sample().len(), 1);
            std::fs::remove_dir_all(tree.0.join("card0/device")).unwrap();
            assert!(sampler.sample().is_empty());
        }

        #[test]
        fn nvidia_csv_keeps_missing_values_unavailable_and_converts_mib() {
            let metrics = parse_nvidia_csv(
                "00000000:01:00.0, NVIDIA RTX Test, 37, 512, 4096, 47\n\
                 00000000:02:00.0, NVIDIA Other, [N/A], [N/A], [N/A], [N/A]\n\
                 invalid,row\n",
            );
            assert_eq!(metrics.len(), 2);
            let first = &metrics["0000:01:00.0"];
            assert_eq!(first.id, "0000:01:00.0");
            assert!(first.utilization_at.is_some());
            assert_eq!(first.utilization, Some(37.));
            assert_eq!(first.memory, Some((536_870_912, 4_294_967_296)));
            assert_eq!(first.temperature, Some(47.));
            let unavailable = &metrics["0000:02:00.0"];
            assert!(unavailable.utilization.is_none() && unavailable.memory.is_none());
        }
    }
}

#[derive(Debug, PartialEq)]
struct MemoryBreakdown {
    total: f32,
    used: f32,
    cache: f32,
    free: f32,
}
impl MemoryBreakdown {
    fn from_meminfo(info: &str) -> Option<Self> {
        let kb = |key: &str| {
            info.lines()
                .find(|line| line.starts_with(key))?
                .split_whitespace()
                .nth(1)?
                .parse::<u64>()
                .ok()
        };
        let total = kb("MemTotal:")?;
        if total == 0 {
            return None;
        }
        // Match modern Linux system monitors: used = total - available.
        // Not all cached memory is available for new allocations. Split only
        // the available estimate into unused RAM and estimated reusable cache.
        let available = kb("MemAvailable:")?.min(total);
        // Under pressure MemAvailable can be below MemFree because the kernel
        // reserves some free pages. Count those reserves as used, not free.
        let free = kb("MemFree:")?.min(available);
        const KIB_PER_GIB: f32 = 1024. * 1024.;
        Some(Self {
            total: total as f32 / KIB_PER_GIB,
            used: (total - available) as f32 / KIB_PER_GIB,
            cache: (available - free) as f32 / KIB_PER_GIB,
            free: free as f32 / KIB_PER_GIB,
        })
    }
}
fn swap_usage(info: &str) -> Option<(f32, f32)> {
    let kb = |key: &str| {
        info.lines()
            .find(|line| line.starts_with(key))?
            .split_whitespace()
            .nth(1)?
            .parse::<u64>()
            .ok()
    };
    let total = kb("SwapTotal:")?;
    let free = kb("SwapFree:")?.min(total);
    const KIB_PER_GIB: f32 = 1024. * 1024.;
    Some((
        (total - free) as f32 / KIB_PER_GIB,
        total as f32 / KIB_PER_GIB,
    ))
}
fn cpu_temperature_input() -> Option<PathBuf> {
    for entry in std::fs::read_dir("/sys/class/hwmon").ok()?.flatten() {
        let dir = entry.path();
        let name = std::fs::read_to_string(dir.join("name")).unwrap_or_default();
        if !matches!(name.trim(), "k10temp" | "coretemp" | "zenpower") {
            continue;
        }
        // Prefer a die/package sensor, falling back to AMD's Tctl control sensor.
        for wanted in ["Tdie", "Package id 0", "Tctl"] {
            for label in std::fs::read_dir(&dir).ok()?.flatten() {
                let path = label.path();
                if !path.file_name()?.to_string_lossy().ends_with("_label") {
                    continue;
                }
                if std::fs::read_to_string(&path).ok()?.trim() == wanted {
                    let input = path.with_file_name(
                        path.file_name()?
                            .to_string_lossy()
                            .replace("_label", "_input"),
                    );
                    return input.is_file().then_some(input);
                }
            }
        }
    }
    None
}
fn disk_usage() -> Option<(f64, f64)> {
    let mut stats = std::mem::MaybeUninit::<libc::statvfs>::uninit();
    // SAFETY: a valid NUL-terminated path and an appropriately sized output object.
    if unsafe { libc::statvfs(c"/".as_ptr(), stats.as_mut_ptr()) } != 0 {
        return None;
    }
    let stats = unsafe { stats.assume_init() };
    let unit = stats.f_frsize as f64 / 1073741824.;
    Some((
        (stats.f_blocks - stats.f_bfree) as f64 * unit,
        stats.f_blocks as f64 * unit,
    ))
}
pub fn weather_icon(code: u32, is_day: bool) -> &'static str {
    match code {
        0 | 1 => {
            if is_day {
                "☀"
            } else {
                "☾"
            }
        }
        2 | 3 | 45 | 48 => "☁",
        71..=77 | 85 | 86 => "❄",
        95..=99 => "ϟ",
        _ => "☂",
    }
}

#[derive(Debug, Clone, Deserialize, Serialize, Default)]
pub struct CalendarData {
    pub account: String,
    pub month: String,
    pub updated_at: String,
    pub events: Vec<CalendarEvent>,
    #[serde(default)]
    pub connection_revision: String,
    #[serde(default)]
    pub calendar_id: String,
}
#[derive(Default)]
pub struct CalendarAgenda<'a> {
    pub events: Vec<&'a CalendarEvent>,
    pub earlier: usize,
}

impl CalendarData {
    /// Hide completed timed events only when looking at today. Historical
    /// dates remain browsable, and all-day events stay visible all day.
    pub fn agenda_at(
        &self,
        day: chrono::NaiveDate,
        now: chrono::DateTime<chrono::Local>,
        show_earlier: bool,
    ) -> CalendarAgenda<'_> {
        let mut agenda = CalendarAgenda::default();
        for event in self.events.iter().filter(|event| event.on_date(day)) {
            let earlier = day == now.date_naive() && event.finished_at(now);
            agenda.earlier += usize::from(earlier);
            if !earlier || show_earlier {
                agenda.events.push(event);
            }
        }
        agenda
    }
}
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct CalendarEvent {
    pub title: String,
    pub start: String,
    pub end: String,
    pub all_day: bool,
    pub url: String,
    #[serde(default)]
    pub meeting_url: Option<String>,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub location: String,
    #[serde(default)]
    pub calendar_name: String,
    #[serde(default)]
    pub calendar_color: String,
}
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum MeetingState {
    Later,
    Soon(i64),
    Live,
    Finished,
}
impl CalendarEvent {
    pub fn chronological_order(&self, other: &Self) -> std::cmp::Ordering {
        let key = |event: &Self| {
            chrono::DateTime::parse_from_rfc3339(&event.start)
                .ok()
                .map(|time| time.timestamp())
                .or_else(|| {
                    use chrono::TimeZone;
                    let date = chrono::NaiveDate::parse_from_str(&event.start, "%Y-%m-%d").ok()?;
                    chrono::Local
                        .from_local_datetime(&date.and_hms_opt(0, 0, 0)?)
                        .earliest()
                        .map(|time| time.timestamp())
                })
        };
        key(self)
            .cmp(&key(other))
            .then(self.title.cmp(&other.title))
    }

    pub fn details_key(&self) -> String {
        use sha2::{Digest, Sha256};
        let mut hash = Sha256::new();
        for value in [&self.calendar_name, &self.start, &self.end, &self.title] {
            hash.update(value.as_bytes());
            hash.update([0]);
        }
        format!(
            "halcyon-event:{}",
            hash.finalize()
                .iter()
                .map(|b| format!("{b:02x}"))
                .collect::<String>()
        )
    }
    pub fn source_color(&self) -> Option<cosmic::iced::Color> {
        let raw = self.calendar_color.strip_prefix('#')?;
        if raw.len() != 6 {
            return None;
        }
        let n = u32::from_str_radix(raw, 16).ok()?;
        Some(cosmic::iced::Color::from_rgb8(
            (n >> 16) as u8,
            (n >> 8) as u8,
            n as u8,
        ))
    }
    pub fn private_copy(&self) -> Self {
        Self {
            title: "Busy".into(),
            start: self.start.clone(),
            end: self.end.clone(),
            all_day: self.all_day,
            calendar_color: self.calendar_color.clone(),
            ..Default::default()
        }
    }

    pub fn meeting_state_at(
        &self,
        now: chrono::DateTime<chrono::Local>,
        notice_minutes: i64,
    ) -> MeetingState {
        let (Ok(start), Ok(end)) = (
            chrono::DateTime::parse_from_rfc3339(&self.start),
            chrono::DateTime::parse_from_rfc3339(&self.end),
        ) else {
            return MeetingState::Later;
        };
        if now >= end {
            return MeetingState::Finished;
        }
        if now >= start {
            return MeetingState::Live;
        }
        let seconds = start.signed_duration_since(now).num_seconds();
        if seconds <= notice_minutes.clamp(1, 60) * 60 {
            MeetingState::Soon(((seconds + 59) / 60).max(1))
        } else {
            MeetingState::Later
        }
    }
    pub fn join_url(&self) -> Option<&str> {
        let url = self.meeting_url.as_deref()?;
        reqwest::Url::parse(url)
            .ok()
            .filter(|u| {
                u.scheme() == "https"
                    && u.has_host()
                    && u.username().is_empty()
                    && u.password().is_none()
            })
            .map(|_| url)
    }
    pub fn on_date(&self, day: chrono::NaiveDate) -> bool {
        if !self.all_day {
            let start = chrono::DateTime::parse_from_rfc3339(&self.start).ok();
            let end = chrono::DateTime::parse_from_rfc3339(&self.end).ok();
            return match (start, end) {
                (Some(start), Some(end)) => {
                    let last = if end > start {
                        end - chrono::Duration::nanoseconds(1)
                    } else {
                        start
                    };
                    day >= start.with_timezone(&chrono::Local).date_naive()
                        && day <= last.with_timezone(&chrono::Local).date_naive()
                }
                _ => false,
            };
        }
        let start = self
            .start
            .get(..10)
            .and_then(|s| chrono::NaiveDate::parse_from_str(s, "%Y-%m-%d").ok());
        let end = self
            .end
            .get(..10)
            .and_then(|s| chrono::NaiveDate::parse_from_str(s, "%Y-%m-%d").ok());
        match (start, end) {
            (Some(start), Some(end)) => {
                day >= start
                    && if self.all_day {
                        day < end.max(start + chrono::Duration::days(1))
                    } else {
                        day <= end
                    }
            }
            _ => false,
        }
    }
    pub fn time_label(&self) -> String {
        if self.all_day {
            return "All day".into();
        }
        chrono::DateTime::parse_from_rfc3339(&self.start)
            .map(|d| d.with_timezone(&chrono::Local).format("%H:%M").to_string())
            .unwrap_or_else(|_| "—".into())
    }
    pub fn finished(&self) -> bool {
        self.finished_at(chrono::Local::now())
    }
    pub fn finished_at(&self, now: chrono::DateTime<chrono::Local>) -> bool {
        !self.all_day && chrono::DateTime::parse_from_rfc3339(&self.end).is_ok_and(|d| d <= now)
    }
}
#[derive(Debug, Clone, Deserialize, Default)]
pub struct ProjectsData {
    pub updated_at: String,
    pub projects: Vec<Project>,
}
#[derive(Debug, Clone, Deserialize)]
pub struct Project {
    pub path: String,
    pub name: String,
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub repository_id: String,
    #[serde(default)]
    pub recent: f64,
    pub group: String,
    pub branch: String,
    pub changed: usize,
    pub untracked: usize,
    pub conflicts: usize,
    pub ahead: usize,
    pub behind: usize,
    pub upstream: bool,
    pub error: bool,
    pub servers: Vec<ProjectServer>,
}
#[derive(Debug, Clone, Deserialize)]
pub struct ProjectServer {
    pub port: u16,
    pub web: bool,
}
pub async fn fetch_projects(roots: Vec<String>) -> Result<ProjectsData, String> {
    tokio::task::spawn_blocking(move || {
        use std::io::Write;
        use std::process::Stdio;
        let input = serde_json::to_vec(&roots).map_err(|_| "Invalid project folders")?;
        if roots.len() > 128 || input.len() > crate::config::MAX_CONFIG_BYTES {
            return Err("Too many project folders".into());
        }
        let helper =
            crate::paths::helper("project-status.py").ok_or("Project status unavailable")?;
        let config = crate::config::Config::path().ok_or("Project configuration unavailable")?;
        let mut child = std::process::Command::new("/usr/bin/timeout")
            .args(["25s", "/usr/bin/python3"])
            .arg(helper)
            .arg("--roots-stdin")
            .env("DESKTOP_CARDS_CONFIG", config)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|e| e.to_string())?;
        // The deadline process remains owned and reaped even if input delivery
        // fails. Taking/dropping stdin also gives the bounded helper its EOF.
        let sent = child
            .stdin
            .take()
            .expect("piped project input")
            .write_all(&input);
        let output = child.wait_with_output().map_err(|e| e.to_string())?;
        sent.map_err(|_| "Could not send project folders".to_string())?;
        if !output.status.success() {
            return Err("Project status unavailable".into());
        }
        #[derive(Deserialize)]
        struct Reply {
            requested_roots: Vec<String>,
            #[serde(flatten)]
            data: ProjectsData,
        }
        let reply: Reply = serde_json::from_slice(&output.stdout)
            .map_err(|_| "Invalid project status response".to_string())?;
        if reply.requested_roots != roots {
            return Err("Project status belongs to different folders".into());
        }
        Ok(reply.data)
    })
    .await
    .map_err(|e| e.to_string())?
}
impl CalendarData {
    pub fn cached(month: &str) -> Option<Self> {
        crate::calendar::store_cached(&crate::calendar::load_connection(), month)
    }
}
pub fn cpu_usage(old: (u64, u64), now: (u64, u64)) -> f32 {
    let total = now.0.saturating_sub(old.0);
    if total == 0 {
        return 0.;
    }
    (100. * (total.saturating_sub(now.1.saturating_sub(old.1))) as f32 / total as f32)
        .clamp(0., 100.)
}
pub async fn t3_active() -> bool {
    tokio::task::spawn_blocking(|| {
        std::process::Command::new("/usr/bin/timeout")
            .args([
                "3s",
                "systemctl",
                "--user",
                "is-active",
                "--quiet",
                "t3code.service",
            ])
            .status()
            .is_ok_and(|s| s.success())
    })
    .await
    .unwrap_or(false)
}
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn project_helper_receives_resolved_config_path() {
        if let Ok(expected) = std::env::var("HALCYON_PROJECT_ROUTE_FIXTURE") {
            let roots = vec!["requested A".to_string()];
            let data = tokio::runtime::Runtime::new()
                .unwrap()
                .block_on(fetch_projects(roots.clone()))
                .unwrap();
            let observed: serde_json::Value = serde_json::from_str(&data.updated_at).unwrap();
            assert_eq!(observed["config"], expected);
            assert_eq!(observed["args"], serde_json::json!(["--roots-stdin"]));
            assert_eq!(observed["roots"], serde_json::json!(roots));
            return;
        }
        struct Fixture(std::path::PathBuf);
        impl Drop for Fixture {
            fn drop(&mut self) {
                let _ = std::fs::remove_dir_all(&self.0);
            }
        }
        let root = Fixture(
            std::env::temp_dir().join(format!("halcyon-project-route-{}", std::process::id())),
        );
        let helper = root
            .0
            .as_path()
            .join("lib/cosmic-ext-halcyon/project-status.py");
        std::fs::create_dir_all(helper.parent().unwrap()).unwrap();
        std::fs::write(
            &helper,
            r#"import json, os, sys
roots = json.load(sys.stdin)
observed = {'config': os.environ['DESKTOP_CARDS_CONFIG'], 'args': sys.argv[1:], 'roots': roots}
print(json.dumps({'updated_at': json.dumps(observed), 'projects': [], 'requested_roots': roots}))
"#,
        )
        .unwrap();
        for explicit in [false, true] {
            let expected = if explicit {
                root.0.as_path().join("override.json")
            } else {
                root.0.as_path().join("config/halcyon/config.json")
            };
            let mut command = std::process::Command::new(std::env::current_exe().unwrap());
            command
                .args([
                    "--exact",
                    "data::tests::project_helper_receives_resolved_config_path",
                    "--nocapture",
                ])
                .env("HOME", root.0.as_path())
                .env("XDG_CONFIG_HOME", root.0.as_path().join("config"))
                .env("HALCYON_PREFIX", root.0.as_path())
                .env("HALCYON_PROJECT_ROUTE_FIXTURE", &expected)
                .env_remove("DESKTOP_CARDS_CONFIG");
            if explicit {
                command.env("DESKTOP_CARDS_CONFIG", &expected);
            }
            assert!(command.status().unwrap().success(), "explicit={explicit}");
        }
    }

    #[test]
    fn cpu_history_freshness_requires_two_valid_recent_counter_samples() {
        let start = std::time::Instant::now();
        let mut sampler = Sampler::default();
        let mut metrics = Metrics {
            cpu: 77.,
            ..Default::default()
        };
        sampler.sample_cpu(&mut metrics, Some("cpu 50 0 0 50 0"), start);
        assert!(!metrics.cpu_sampled);
        assert_eq!(metrics.cpu, 77.);
        sampler.sample_cpu(
            &mut metrics,
            Some("cpu 100 0 0 100 0"),
            start + Duration::from_secs(3),
        );
        assert!(metrics.cpu_sampled);
        assert_eq!(metrics.cpu, 50.);
        sampler.sample_cpu(&mut metrics, None, start + Duration::from_secs(6));
        assert!(!metrics.cpu_sampled);
        assert_eq!(metrics.cpu, 50.);
        sampler.sample_cpu(
            &mut metrics,
            Some("cpu 150 0 0 150 0"),
            start + Duration::from_secs(9),
        );
        assert!(!metrics.cpu_sampled);
        sampler.sample_cpu(
            &mut metrics,
            Some("cpu 200 0 0 200 0"),
            start + Duration::from_secs(300),
        );
        assert!(!metrics.cpu_sampled);
        sampler.sample_cpu(
            &mut metrics,
            Some("cpu 1 0 0 1 0"),
            start + Duration::from_secs(303),
        );
        assert!(!metrics.cpu_sampled);
    }

    #[test]
    fn malformed_memory_retains_the_number_but_does_not_repeat_it_in_history() {
        let mut metrics = Metrics::default();
        Sampler::sample_memory(
            &mut metrics,
            Some("MemTotal: 8388608 kB\nMemFree: 2097152 kB\nMemAvailable: 4194304 kB\n"),
            false,
        );
        assert!(metrics.memory_sampled);
        assert_eq!(metrics.memory_used, 4.);
        Sampler::sample_memory(&mut metrics, Some("MemTotal: 8388608 kB\n"), false);
        assert!(!metrics.memory_sampled);
        assert_eq!(metrics.memory_used, 4.);
        Sampler::sample_memory(&mut metrics, None, false);
        assert!(!metrics.memory_sampled);
        assert_eq!(metrics.memory_used, 4.);
    }

    #[test]
    fn disabled_disk_poll_cannot_restore_a_stale_capacity_reading() {
        let previous = Metrics {
            disk: Some((50., 200.)),
            ..Default::default()
        };
        let preferences: DevboxPreferences = serde_json::from_value(serde_json::json!({
            "cpu": false, "memory": false, "disk": false, "network": false,
            "gpu": false, "uptime": false, "services": false
        }))
        .unwrap();
        // The UI can enable disk/history before this disabled poll completes.
        // Its result must carry no disk measurement for the new preferences.
        let sampled = Sampler::default().sample(&previous, &preferences);
        assert_eq!(sampled.disk, None);
    }
    #[test]
    fn weather_cache_survives_restart_but_never_uses_another_city() {
        let directory =
            std::env::temp_dir().join(format!("desktop-cards-weather-test-{}", std::process::id()));
        let path = directory.join("weather.json");
        let location = Location {
            name: "Test city".into(),
            latitude: 1.,
            longitude: 2.,
            timezone: None,
        };
        let snapshot = WeatherSnapshot {
            location: location.clone(),
            updated_at: "2026-10-17T16:00:00+02:00".into(),
            weather: Weather {
                current: Current {
                    temperature_2m: 14.,
                    apparent_temperature: 12.,
                    weather_code: 3,
                    wind_speed_10m: 4.,
                    relative_humidity_2m: 38.,
                    dew_point_2m: Some(-0.1),
                    is_day: 1,
                },
                daily: Daily {
                    time: vec![],
                    temperature_2m_max: vec![],
                    temperature_2m_min: vec![],
                    weather_code: vec![],
                    precipitation_probability_max: vec![],
                },
            },
        };
        snapshot.save_at(&path).unwrap();
        let restored = WeatherSnapshot::read_at(&path, &location).unwrap();
        assert_eq!(restored.updated_at, snapshot.updated_at);
        assert_eq!(restored.weather.current.temperature_2m, 14.);
        assert_eq!(restored.weather.current.dew_point_2m, Some(-0.1));
        // Existing caches and missing provider readings remain usable.
        let mut old = serde_json::to_value(&snapshot).unwrap();
        old["weather"]["current"]
            .as_object_mut()
            .unwrap()
            .remove("dew_point_2m");
        std::fs::write(&path, serde_json::to_vec(&old).unwrap()).unwrap();
        assert_eq!(
            WeatherSnapshot::read_at(&path, &location)
                .unwrap()
                .weather
                .current
                .dew_point_2m,
            None
        );
        old["weather"]["current"]["dew_point_2m"] = serde_json::Value::Null;
        std::fs::write(&path, serde_json::to_vec(&old).unwrap()).unwrap();
        assert_eq!(
            WeatherSnapshot::read_at(&path, &location)
                .unwrap()
                .weather
                .current
                .temperature_2m,
            14.
        );
        let other = Location {
            latitude: 3.,
            ..location
        };
        assert!(WeatherSnapshot::read_at(&path, &other).is_none());
        std::fs::write(&path, "partial JSON").unwrap();
        assert!(WeatherSnapshot::read_at(&path, &snapshot.location).is_none());
        std::fs::remove_dir_all(directory).unwrap();
    }
    #[test]
    fn meeting_highlight_respects_start_end_and_notice_window() {
        use chrono::TimeZone;
        let start = chrono::Local
            .with_ymd_and_hms(2026, 10, 16, 11, 0, 0)
            .unwrap();
        let end = start + chrono::Duration::minutes(30);
        let event = CalendarEvent {
            title: "Call".into(),
            start: start.to_rfc3339(),
            end: end.to_rfc3339(),
            all_day: false,
            url: String::new(),
            meeting_url: Some("https://meet.google.com/example".into()),
            ..Default::default()
        };
        assert_eq!(
            event.meeting_state_at(start - chrono::Duration::minutes(11), 10),
            MeetingState::Later
        );
        assert_eq!(
            event.meeting_state_at(start - chrono::Duration::minutes(10), 10),
            MeetingState::Soon(10)
        );
        assert_eq!(
            event.meeting_state_at(start - chrono::Duration::seconds(1), 10),
            MeetingState::Soon(1)
        );
        assert_eq!(event.meeting_state_at(start, 10), MeetingState::Live);
        assert_eq!(event.meeting_state_at(end, 10), MeetingState::Finished);
        assert!(event.join_url().is_some());
        let mut bad = event;
        bad.meeting_url = Some("javascript:alert(1)".into());
        assert!(bad.join_url().is_none());
        bad.meeting_url = Some("https://user:password@example.com".into());
        assert!(bad.join_url().is_none());
    }
    #[test]
    fn cpu_handles_idle_load_and_counter_reset() {
        assert_eq!(cpu_usage((100, 50), (200, 100)), 50.);
        assert_eq!(cpu_usage((100, 50), (200, 150)), 0.);
        assert_eq!(cpu_usage((200, 100), (100, 50)), 0.);
    }
    #[test]
    fn swap_reports_used_and_total_in_gib() {
        assert_eq!(
            swap_usage("SwapTotal: 4194304 kB\nSwapFree: 1048576 kB\n"),
            Some((3., 4.))
        );
    }
    #[test]
    fn swap_distinguishes_no_configured_space_from_unavailable_data() {
        assert_eq!(
            swap_usage("SwapTotal: 0 kB\nSwapFree: 0 kB\n"),
            Some((0., 0.))
        );
        for info in [
            "",
            "SwapFree: 0 kB\n",
            "SwapTotal: 4194304 kB\n",
            "SwapTotal: invalid kB\nSwapFree: 0 kB\n",
            "SwapTotal: 4194304 kB\nSwapFree: invalid kB\n",
            "SwapTotal: 4194304 kB\nSwapFree: -1 kB\n",
        ] {
            assert_eq!(swap_usage(info), None, "{info:?}");
        }
    }
    #[test]
    fn swap_bounds_free_space_by_total() {
        assert_eq!(
            swap_usage("SwapTotal: 4194304 kB\nSwapFree: 8388608 kB\n"),
            Some((0., 4.))
        );
    }
    #[test]
    fn memory_uses_kernel_availability_instead_of_subtracting_all_cache() {
        // Captured from the affected desktop: the old formula showed 5.6 GiB
        // while the system monitor and `free` reported 8.0 GiB for this sample.
        let info = "MemTotal:       32129576 kB\n\
                    MemFree:         2204936 kB\n\
                    MemAvailable:   23698068 kB\n\
                    Buffers:          520376 kB\n\
                    Cached:         22541300 kB\n\
                    SReclaimable:    1102992 kB\n\
                    Shmem:            139648 kB\n";
        let memory = MemoryBreakdown::from_meminfo(info).unwrap();
        assert_eq!(format!("{:.1}", memory.total), "30.6");
        assert_eq!(format!("{:.1}", memory.used), "8.0");
        assert_eq!(format!("{:.1}", memory.cache), "20.5");
        assert_eq!(format!("{:.1}", memory.free), "2.1");
        assert!((memory.used + memory.cache + memory.free - memory.total).abs() < 0.00001);
    }
    #[test]
    fn memory_reserves_stay_used_when_available_is_below_free() {
        let memory = MemoryBreakdown::from_meminfo(
            "MemTotal: 33554432 kB\nMemFree: 4194304 kB\nMemAvailable: 2097152 kB\n",
        )
        .unwrap();
        assert_eq!(
            memory,
            MemoryBreakdown {
                total: 32.,
                used: 30.,
                cache: 0.,
                free: 2.,
            }
        );
        let exhausted = MemoryBreakdown::from_meminfo(
            "MemTotal: 33554432 kB\nMemFree: 1024 kB\nMemAvailable: 0 kB\n",
        )
        .unwrap();
        assert_eq!(exhausted.used, 32.);
        assert_eq!(exhausted.cache + exhausted.free, 0.);
    }
    #[test]
    fn incomplete_memory_sample_does_not_replace_last_valid_reading() {
        for info in [
            "MemTotal: 33554432 kB\nMemFree: 4194304 kB\n",
            "MemTotal: 33554432 kB\nMemFree: 4194304 kB\nMemAvailable: invalid kB\n",
            "MemTotal: 0 kB\nMemFree: 0 kB\nMemAvailable: 0 kB\n",
        ] {
            assert!(MemoryBreakdown::from_meminfo(info).is_none());
        }
    }
    #[test]
    fn all_day_end_is_exclusive() {
        let e = CalendarEvent {
            title: "Trip".into(),
            start: "2026-10-16".into(),
            end: "2026-10-18".into(),
            all_day: true,
            url: String::new(),
            meeting_url: None,
            ..Default::default()
        };
        assert!(e.on_date(chrono::NaiveDate::from_ymd_opt(2026, 10, 17).unwrap()));
        assert!(!e.on_date(chrono::NaiveDate::from_ymd_opt(2026, 10, 18).unwrap()));
    }
    #[test]
    fn midnight_end_does_not_mark_the_next_day() {
        use chrono::TimeZone;
        let start = chrono::Local
            .with_ymd_and_hms(2026, 10, 16, 22, 0, 0)
            .unwrap();
        let end = chrono::Local
            .with_ymd_and_hms(2026, 10, 17, 0, 0, 0)
            .unwrap();
        let e = CalendarEvent {
            title: "Evening".into(),
            start: start.to_rfc3339(),
            end: end.to_rfc3339(),
            all_day: false,
            url: String::new(),
            meeting_url: None,
            ..Default::default()
        };
        assert!(e.on_date(start.date_naive()));
        assert!(!e.on_date(end.date_naive()));
    }
}
