//! A few facts about the system, for the daily report: the distribution's ID
//! and version, the COSMIC version from the package database, graphics vendors,
//! and the number of displays with the largest one's resolution. Fixed values
//! only: no names, models, serials, hostnames or paths. COSMIC programs are
//! never run to learn their version.
use serde::Serialize;
use std::io::{BufRead, Read};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::Duration;

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize)]
pub struct System {
    pub distro: Option<String>,
    pub distro_version: Option<String>,
    pub cosmic: Option<String>,
    pub gpu: Option<Vec<&'static str>>,
    pub displays: Option<u8>,
    pub resolution: Option<String>,
}

/// Displays seen by this desktop process: how many, and the largest current mode.
static DISPLAYS: Mutex<Option<(usize, Option<(u32, u32)>)>> = Mutex::new(None);
pub fn displays(count: usize, largest: Option<(u32, u32)>) {
    if let Ok(mut seen) = DISPLAYS.lock() {
        *seen = Some((count, largest));
    }
}

pub(crate) async fn observe() -> System {
    let (displays, resolution) = display_fields(DISPLAYS.lock().ok().and_then(|seen| *seen));
    if cfg!(test) || crate::installation::isolated() {
        return System::default();
    }
    let facts = tokio::time::timeout(
        Duration::from_secs(5),
        tokio::task::spawn_blocking(|| read(Path::new("/"))),
    )
    .await
    .ok()
    .and_then(Result::ok)
    .unwrap_or_default();
    System {
        displays,
        resolution,
        ..facts
    }
}

fn display_fields(seen: Option<(usize, Option<(u32, u32)>)>) -> (Option<u8>, Option<String>) {
    let Some((count, largest)) = seen.filter(|(count, _)| (1..=16).contains(count)) else {
        return (None, None);
    };
    let resolution = largest
        .filter(|(w, h)| (1..=16384).contains(w) && (1..=16384).contains(h))
        .map(|(w, h)| format!("{w}x{h}"));
    (Some(count as u8), resolution)
}

pub(super) fn read(root: &Path) -> System {
    let (distro, distro_version) = os_release(root);
    System {
        distro,
        distro_version,
        cosmic: cosmic_version(root),
        gpu: gpus(root),
        displays: None,
        resolution: None,
    }
}

fn rooted(root: &Path, absolute: &str) -> PathBuf {
    root.join(absolute.trim_start_matches('/'))
}
fn read_small(path: &Path, max: u64) -> Option<String> {
    let file = std::fs::File::open(path).ok()?;
    if !file.metadata().ok()?.is_file() {
        return None;
    }
    let mut text = String::new();
    file.take(max + 1).read_to_string(&mut text).ok()?;
    (text.len() as u64 <= max).then_some(text)
}

/// os-release IDs are lowercase letters, digits and `._-`; anything else is
/// not reported rather than cleaned up.
fn os_id(value: &str, max: usize) -> Option<String> {
    let value = value.trim().trim_matches(|c| c == '"' || c == '\'');
    let mut bytes = value.bytes();
    (value.len() <= max
        && bytes
            .next()
            .is_some_and(|b| b.is_ascii_lowercase() || b.is_ascii_digit())
        && bytes.all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b"._-".contains(&b)))
    .then(|| value.to_owned())
}
fn os_release(root: &Path) -> (Option<String>, Option<String>) {
    let Some(text) = read_small(&rooted(root, "/etc/os-release"), 65536)
        .or_else(|| read_small(&rooted(root, "/usr/lib/os-release"), 65536))
    else {
        return (None, None);
    };
    let field = |key: &str| {
        text.lines()
            .find_map(|line| line.trim().strip_prefix(key)?.strip_prefix('='))
    };
    let id = field("ID").and_then(|v| os_id(v, 32));
    let version = id
        .as_ref()
        .and_then(|_| field("VERSION_ID"))
        .and_then(|v| os_id(v, 16));
    (id, version)
}

/// The upstream part of a package version: `1:1.0.3~alpha-1` gives `1.0.3`.
fn upstream(version: &str) -> Option<String> {
    let version = version.trim();
    let version = version.split_once(':').map_or(version, |(_, v)| v);
    let parts: Vec<&str> = version
        .split(|c: char| !(c.is_ascii_digit() || c == '.'))
        .next()?
        .split('.')
        .take(3)
        .collect();
    let valid = parts
        .iter()
        .enumerate()
        .all(|(i, p)| !p.is_empty() && p.len() <= if i == 0 { 4 } else { 6 });
    valid.then(|| parts.join("."))
}

const COSMIC_PACKAGE: &str = "cosmic-session";
fn cosmic_version(root: &Path) -> Option<String> {
    dpkg_version(root)
        .or_else(|| crate::installation::cosmic_rpm_version(root))
        .or_else(|| pacman_version(root))
        .and_then(|v| upstream(&v))
}
/// Reads the dpkg status file a line at a time; it can be several megabytes.
fn dpkg_version(root: &Path) -> Option<String> {
    let file = std::fs::File::open(rooted(root, "/var/lib/dpkg/status")).ok()?;
    let reader = std::io::BufReader::new(file.take(64 * 1024 * 1024));
    let (mut package, mut installed, mut version) = (false, false, None);
    for line in reader.lines() {
        let line = line.ok()?;
        if line.is_empty() {
            if package && installed {
                return version;
            }
            (package, installed, version) = (false, false, None);
        } else if let Some(name) = line.strip_prefix("Package: ") {
            package = name.trim() == COSMIC_PACKAGE;
        } else if let Some(status) = line.strip_prefix("Status: ") {
            installed = status.trim().ends_with(" installed");
        } else if let Some(v) = line.strip_prefix("Version: ") {
            version = Some(v.trim().to_owned());
        }
    }
    (package && installed).then_some(version).flatten()
}
/// Arch keeps one directory per installed package: `cosmic-session-1.0.3-1`.
fn pacman_version(root: &Path) -> Option<String> {
    std::fs::read_dir(rooted(root, "/var/lib/pacman/local"))
        .ok()?
        .flatten()
        .filter_map(|entry| entry.file_name().into_string().ok())
        .find_map(|name| {
            let rest = name.strip_prefix(COSMIC_PACKAGE)?.strip_prefix('-')?;
            rest.starts_with(|c: char| c.is_ascii_digit() || c == ':')
                .then(|| rest.to_owned())
        })
}

/// PCI vendors of the graphics cards, as one of four fixed names.
fn gpus(root: &Path) -> Option<Vec<&'static str>> {
    let mut found = Vec::new();
    for entry in std::fs::read_dir(rooted(root, "/sys/class/drm"))
        .ok()?
        .flatten()
    {
        let name = entry.file_name();
        let Some(n) = name.to_str().and_then(|n| n.strip_prefix("card")) else {
            continue;
        };
        if n.is_empty() || !n.bytes().all(|b| b.is_ascii_digit()) {
            continue;
        }
        let Some(vendor) = read_small(&entry.path().join("device/vendor"), 16) else {
            continue;
        };
        found.push(match vendor.trim() {
            "0x1002" => "amd",
            "0x8086" => "intel",
            "0x10de" => "nvidia",
            _ => "other",
        });
    }
    let ordered: Vec<_> = ["amd", "intel", "nvidia", "other"]
        .into_iter()
        .filter(|v| found.contains(v))
        .collect();
    (!ordered.is_empty()).then_some(ordered)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn root() -> PathBuf {
        let mut bytes = [0u8; 8];
        getrandom::fill(&mut bytes).unwrap();
        let p =
            std::env::temp_dir().join(format!("halcyon-system-{:x}", u64::from_le_bytes(bytes)));
        std::fs::create_dir_all(&p).unwrap();
        p
    }
    fn write(root: &Path, path: &str, text: &str) {
        let path = rooted(root, path);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, text).unwrap();
    }

    #[test]
    fn distribution_id_and_version_use_a_fixed_charset() {
        let r = root();
        write(
            &r,
            "/etc/os-release",
            "NAME=\"Pop!_OS\"\nVERSION=\"24.04 LTS\"\nID=pop\nID_LIKE=\"ubuntu debian\"\nVERSION_ID=\"24.04\"\nPRETTY_NAME=\"Pop!_OS 24.04 LTS\"\n",
        );
        assert_eq!(os_release(&r), (Some("pop".into()), Some("24.04".into())));
        write(
            &r,
            "/etc/os-release",
            "ID=\"opensuse-tumbleweed\"\nVERSION_ID=\"20261110\"\n",
        );
        assert_eq!(
            os_release(&r),
            (Some("opensuse-tumbleweed".into()), Some("20261110".into()))
        );
        write(&r, "/etc/os-release", "ID=arch\n");
        assert_eq!(os_release(&r), (Some("arch".into()), None));
        for bad in [
            "ID=\"My Distro\"",
            "ID=Pop",
            "ID=pop/../x",
            "ID=",
            "ID=\"-pop\"",
        ] {
            write(&r, "/etc/os-release", &format!("{bad}\nVERSION_ID=1\n"));
            assert_eq!(os_release(&r), (None, None), "{bad}");
        }
        write(&r, "/etc/os-release", &format!("ID={}\n", "a".repeat(33)));
        assert_eq!(os_release(&r).0, None);
        write(
            &r,
            "/etc/os-release",
            "ID=fedora\nVERSION_ID=\"43 (Workstation)\"\n",
        );
        assert_eq!(os_release(&r), (Some("fedora".into()), None));
        std::fs::remove_file(rooted(&r, "/etc/os-release")).unwrap();
        write(&r, "/usr/lib/os-release", "ID=debian\nVERSION_ID=\"13\"\n");
        assert_eq!(os_release(&r), (Some("debian".into()), Some("13".into())));
    }

    #[test]
    fn cosmic_version_comes_from_the_package_database() {
        let r = root();
        assert_eq!(cosmic_version(&r), None);
        write(
            &r,
            "/var/lib/dpkg/status",
            "Package: cosmic-session-extra\nStatus: install ok installed\nVersion: 9.9.9\n\n\
             Package: cosmic-session\nStatus: deinstall ok config-files\nVersion: 0.1.0\n\n\
             Package: cosmic-session\nStatus: install ok installed\nArchitecture: amd64\nVersion: 1.11.0~1792525615~24.04~4b0a0a2\n",
        );
        assert_eq!(cosmic_version(&r).as_deref(), Some("1.11.0"));
        assert_eq!(upstream("1:1.0.3-1").as_deref(), Some("1.0.3"));
        assert_eq!(upstream("1.0.0~alpha.7").as_deref(), Some("1.0.0"));
        assert_eq!(upstream("2.0").as_deref(), Some("2.0"));
        assert_eq!(upstream("1.2.3.4").as_deref(), Some("1.2.3"));
        assert_eq!(upstream("git"), None);
        assert_eq!(upstream("12345.1"), None);

        let r = root();
        std::fs::create_dir_all(rooted(&r, "/var/lib/pacman/local/cosmic-session-1.0.3-1"))
            .unwrap();
        std::fs::create_dir_all(rooted(&r, "/var/lib/pacman/local/cosmic-session-git-r1-1"))
            .unwrap();
        assert_eq!(cosmic_version(&r).as_deref(), Some("1.0.3"));
    }

    #[test]
    fn graphics_vendors_are_one_of_four_names() {
        let r = root();
        assert_eq!(gpus(&r), None);
        write(&r, "/sys/class/drm/card1/device/vendor", "0x1002\n");
        write(&r, "/sys/class/drm/card1-DP-1/device/vendor", "0x10de\n");
        write(&r, "/sys/class/drm/renderD128/device/vendor", "0x10de\n");
        assert_eq!(gpus(&r), Some(vec!["amd"]));
        write(&r, "/sys/class/drm/card0/device/vendor", "0x8086\n");
        write(&r, "/sys/class/drm/card2/device/vendor", "0x1af4\n");
        write(&r, "/sys/class/drm/card3/device/vendor", "0x1002\n");
        assert_eq!(gpus(&r), Some(vec!["amd", "intel", "other"]));
    }

    #[test]
    fn displays_report_a_count_and_the_largest_resolution() {
        assert_eq!(display_fields(None), (None, None));
        assert_eq!(display_fields(Some((0, None))), (None, None));
        assert_eq!(
            display_fields(Some((2, Some((3840, 2160))))),
            (Some(2), Some("3840x2160".into()))
        );
        assert_eq!(display_fields(Some((1, Some((0, 0))))), (Some(1), None));
        assert_eq!(display_fields(Some((17, Some((1920, 1080))))), (None, None));
    }

    #[test]
    fn the_report_shape_matches_the_collector() {
        let r = root();
        write(&r, "/etc/os-release", "ID=pop\nVERSION_ID=\"24.04\"\n");
        let value = serde_json::to_value(read(&r)).unwrap();
        assert_eq!(
            value,
            serde_json::json!({"distro": "pop", "distro_version": "24.04", "cosmic": null,
                "gpu": null, "displays": null, "resolution": null})
        );
    }
}
