//! `icon-theme.cache` for a theme Halcyon writes. Without it GTK apps scan
//! every folder of the theme for each lookup.
use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

pub const FILE: &str = "icon-theme.cache";

const HAS_SUFFIX_XPM: u16 = 1 << 0;
const HAS_SUFFIX_SVG: u16 = 1 << 1;
const HAS_SUFFIX_PNG: u16 = 1 << 2;
const EMPTY: u32 = u32::MAX;
/// GLib's spaced primes, which gtk-update-icon-cache also picks from.
const PRIMES: [u32; 24] = [
    11, 19, 37, 73, 109, 163, 251, 367, 557, 823, 1237, 1861, 2777, 4177, 6247, 9371, 14057, 21089,
    31627, 47431, 71143, 106721, 160073, 240101,
];

/// Writes `theme/icon-theme.cache` for `directories` (relative to `theme`),
/// with gtk-update-icon-cache when it is installed and Halcyon's own writer
/// otherwise, then marks the theme folder as no newer than the cache so GTK
/// trusts it.
pub fn write(theme: &Path, directories: &[String]) -> Result<(), String> {
    let tool = std::process::Command::new("gtk-update-icon-cache")
        .args(["--force", "--index-only", "--quiet"])
        .arg(theme)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .is_ok_and(|status| status.success());
    if !(tool && theme.join(FILE).is_file()) {
        write_own(theme, directories)?;
    }
    settle(theme)
}

/// Halcyon's own writer, in the GTK icon cache format 1.0.
pub fn write_own(theme: &Path, directories: &[String]) -> Result<(), String> {
    let bytes = build(theme, directories);
    let temporary = theme.join(format!(".{FILE}.{}", super::unique()));
    fs::write(&temporary, bytes)
        .and_then(|()| fs::rename(&temporary, theme.join(FILE)))
        .map_err(|error| {
            let _ = fs::remove_file(&temporary);
            error.to_string()
        })
}

/// GTK ignores a cache older than its theme folder, so the folder takes the
/// cache's time, as gtk-update-icon-cache does. Run again after the folder
/// is moved into place.
pub fn settle(theme: &Path) -> Result<(), String> {
    let written = fs::metadata(theme.join(FILE))
        .and_then(|meta| meta.modified())
        .map_err(|error| error.to_string())?;
    fs::File::open(theme)
        .and_then(|folder| folder.set_modified(written))
        .map_err(|error| error.to_string())
}

/// The hash GTK uses for icon names (over signed bytes).
fn hash(name: &str) -> u32 {
    let mut bytes = name.bytes().map(|byte| byte as i8 as i32 as u32);
    let Some(first) = bytes.next() else {
        return 0;
    };
    bytes.fold(first, |h, byte| (h << 5).wrapping_sub(h).wrapping_add(byte))
}

/// Icon name to (directory index, suffix flags), from what is on disk.
fn icons(theme: &Path, directories: &[String]) -> BTreeMap<String, Vec<(u16, u16)>> {
    let mut icons: BTreeMap<String, Vec<(u16, u16)>> = BTreeMap::new();
    for (index, directory) in directories.iter().enumerate() {
        let Ok(entries) = fs::read_dir(theme.join(directory)) else {
            continue;
        };
        let mut found: BTreeMap<String, u16> = BTreeMap::new();
        for entry in entries.flatten() {
            let name = entry.file_name();
            let Some(name) = name.to_str() else {
                continue;
            };
            let Some((stem, suffix)) = name.rsplit_once('.') else {
                continue;
            };
            let flag = match suffix {
                "png" => HAS_SUFFIX_PNG,
                "svg" => HAS_SUFFIX_SVG,
                "xpm" => HAS_SUFFIX_XPM,
                _ => continue,
            };
            if stem.is_empty() || stem.contains('\0') {
                continue;
            }
            *found.entry(stem.to_string()).or_default() |= flag;
        }
        for (name, flags) in found {
            icons.entry(name).or_default().push((index as u16, flags));
        }
    }
    icons
}

fn put(bytes: &mut Vec<u8>, at: usize, value: u32) {
    bytes[at..at + 4].copy_from_slice(&value.to_be_bytes());
}

fn push_u32(bytes: &mut Vec<u8>, value: u32) {
    bytes.extend_from_slice(&value.to_be_bytes());
}

fn push_string(bytes: &mut Vec<u8>, text: &str) {
    bytes.extend_from_slice(text.as_bytes());
    bytes.push(0);
    while bytes.len() % 4 != 0 {
        bytes.push(0);
    }
}

fn build(theme: &Path, directories: &[String]) -> Vec<u8> {
    let icons = icons(theme, directories);
    let buckets = PRIMES
        .iter()
        .copied()
        .find(|prime| *prime as usize >= icons.len())
        .unwrap_or(PRIMES[PRIMES.len() - 1]);
    let mut chains: Vec<Vec<(&String, &Vec<(u16, u16)>)>> = vec![Vec::new(); buckets as usize];
    for (name, images) in &icons {
        chains[(hash(name) % buckets) as usize].push((name, images));
    }
    // Header: version 1.0, hash offset, directory list offset.
    let mut bytes = Vec::new();
    bytes.extend_from_slice(&1u16.to_be_bytes());
    bytes.extend_from_slice(&0u16.to_be_bytes());
    push_u32(&mut bytes, 12);
    push_u32(&mut bytes, 0);
    push_u32(&mut bytes, buckets);
    let table = bytes.len();
    for _ in 0..buckets {
        push_u32(&mut bytes, EMPTY);
    }
    for (bucket, chain) in chains.iter().enumerate() {
        let mut link = table + bucket * 4;
        for (name, images) in chain {
            let icon = bytes.len();
            put(&mut bytes, link, icon as u32);
            link = icon;
            push_u32(&mut bytes, EMPTY);
            push_u32(&mut bytes, (icon + 12) as u32);
            push_u32(&mut bytes, 0);
            push_string(&mut bytes, name);
            let list = bytes.len();
            put(&mut bytes, icon + 8, list as u32);
            push_u32(&mut bytes, images.len() as u32);
            for (directory, flags) in images.iter() {
                bytes.extend_from_slice(&directory.to_be_bytes());
                bytes.extend_from_slice(&flags.to_be_bytes());
                push_u32(&mut bytes, 0);
            }
        }
    }
    let list = bytes.len();
    put(&mut bytes, 8, list as u32);
    push_u32(&mut bytes, directories.len() as u32);
    let offsets = bytes.len();
    for _ in directories {
        push_u32(&mut bytes, 0);
    }
    for (index, directory) in directories.iter().enumerate() {
        let at = bytes.len();
        put(&mut bytes, offsets + index * 4, at as u32);
        push_string(&mut bytes, directory);
    }
    bytes
}

/// Reads a cache back: icon name to (directory, flags) for each image.
#[cfg(test)]
pub fn read(bytes: &[u8]) -> Option<BTreeMap<String, Vec<(String, u16)>>> {
    let u16_at = |at: usize| Some(u16::from_be_bytes(bytes.get(at..at + 2)?.try_into().ok()?));
    let u32_at = |at: usize| Some(u32::from_be_bytes(bytes.get(at..at + 4)?.try_into().ok()?));
    let string_at = |at: usize| {
        let rest = bytes.get(at..)?;
        let end = rest.iter().position(|byte| *byte == 0)?;
        String::from_utf8(rest[..end].to_vec()).ok()
    };
    if (u16_at(0)?, u16_at(2)?) != (1, 0) {
        return None;
    }
    let table = u32_at(4)? as usize;
    let list = u32_at(8)? as usize;
    let directories = (0..u32_at(list)? as usize)
        .map(|index| string_at(u32_at(list + 4 + index * 4)? as usize))
        .collect::<Option<Vec<_>>>()?;
    let buckets = u32_at(table)?;
    let mut icons = BTreeMap::new();
    for bucket in 0..buckets as usize {
        let mut icon = u32_at(table + 4 + bucket * 4)?;
        while icon != EMPTY {
            let at = icon as usize;
            let name = string_at(u32_at(at + 4)? as usize)?;
            if hash(&name) % buckets != bucket as u32 {
                return None;
            }
            let images = u32_at(at + 8)? as usize;
            let mut found = Vec::new();
            for index in 0..u32_at(images)? as usize {
                let image = images + 4 + index * 8;
                let directory = directories.get(u16_at(image)? as usize)?.clone();
                found.push((directory, u16_at(image + 2)?));
            }
            icons.insert(name, found);
            icon = u32_at(at)?;
        }
    }
    Some(icons)
}
