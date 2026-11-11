//! Connected displays, by DRM connector (`DP-2`), with the model name the
//! display reports in its EDID so people can recognize it. A fixture root
//! lists them in `<root>/outputs` and names them in `<root>/output-names`
//! (`DP-2=Dell U2720Q` lines) and lists free ports in `<root>/ports`.
//! Reads only.
use crate::desktop::DesktopStore;
use std::path::Path;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Display {
    pub name: String,
    pub label: Option<String>,
    pub builtin: bool,
}

impl Display {
    /// "AOC 24G2W1G5 · DP-1", or the connector alone.
    pub fn title(&self) -> String {
        crate::displays::display_title(&self.name, self.label.as_deref())
    }
}

/// Connected connectors, sorted.
pub fn connected(store: &DesktopStore) -> Vec<String> {
    crate::desktop::wallpaper::discover_outputs(store)
}

/// Connected displays with their names.
pub fn describe(store: &DesktopStore) -> Vec<Display> {
    let names = connected(store);
    let labels: Vec<(String, String)> = match store.fixture_root() {
        Some(root) => std::fs::read_to_string(root.join("output-names"))
            .unwrap_or_default()
            .lines()
            .filter_map(|line| line.split_once('='))
            .map(|(name, label)| (name.trim().to_string(), clean(label)))
            .collect(),
        None => names
            .iter()
            .filter_map(|name| Some((name.clone(), edid_label(name)?)))
            .collect(),
    };
    names
        .into_iter()
        .map(|name| Display {
            label: labels
                .iter()
                .find(|(known, _)| *known == name)
                .map(|(_, label)| label.clone())
                .filter(|label| !label.is_empty()),
            builtin: super::rules::builtin(&name),
            name,
        })
        .collect()
}

/// Display connectors with nothing connected (`HDMI-A-1`), sorted.
pub fn ports(store: &DesktopStore) -> Vec<String> {
    let connected = connected(store);
    let mut names: Vec<String> = match store.fixture_root() {
        Some(root) => std::fs::read_to_string(root.join("ports"))
            .unwrap_or_default()
            .lines()
            .map(|line| line.trim().to_string())
            .collect(),
        None => std::fs::read_dir("/sys/class/drm")
            .into_iter()
            .flatten()
            .flatten()
            .filter_map(|entry| {
                let name = entry.file_name().to_string_lossy().into_owned();
                let (card, connector) = name.split_once('-')?;
                card.starts_with("card")
                    .then(|| connector.to_string())
                    .filter(|connector| !connector.starts_with("Writeback"))
            })
            .collect(),
    };
    names.retain(|name| {
        crate::desktop::wallpaper::valid_output_name(name) && !connected.contains(name)
    });
    names.sort();
    names.dedup();
    names
}

fn edid_label(connector: &str) -> Option<String> {
    let drm = Path::new("/sys/class/drm");
    let entry = std::fs::read_dir(drm).ok()?.flatten().find(|entry| {
        entry
            .file_name()
            .to_str()
            .and_then(|name| name.split_once('-'))
            .is_some_and(|(card, rest)| card.starts_with("card") && rest == connector)
    })?;
    let bytes = std::fs::read(entry.path().join("edid")).ok()?;
    parse_edid(&bytes)
}

/// "AOC 24G2W1G5": the manufacturer's three letters and the monitor name
/// descriptor, from a base EDID block.
pub fn parse_edid(bytes: &[u8]) -> Option<String> {
    if bytes.len() < 128 || bytes[..8] != [0, 255, 255, 255, 255, 255, 255, 0] {
        return None;
    }
    let maker = u16::from_be_bytes([bytes[8], bytes[9]]);
    let letters: String = [10u16, 5, 0]
        .iter()
        .map(|shift| ((maker >> shift) & 31) as u8)
        .filter(|code| (1..=26).contains(code))
        .map(|code| char::from(b'A' + code - 1))
        .collect();
    let model = [54usize, 72, 90, 108].iter().find_map(|&start| {
        let block = &bytes[start..start + 18];
        (block[..3] == [0, 0, 0] && block[3] == 0xfc).then(|| {
            let text: String = block[5..]
                .iter()
                .take_while(|byte| **byte != b'\n')
                .map(|byte| char::from(*byte))
                .collect();
            clean(&text)
        })
    });
    let parts: Vec<String> = [Some(letters), model]
        .into_iter()
        .flatten()
        .filter(|part| !part.is_empty())
        .collect();
    (!parts.is_empty()).then(|| parts.join(" "))
}

fn clean(text: &str) -> String {
    text.chars()
        .filter(|c| !c.is_control())
        .take(40)
        .collect::<String>()
        .trim()
        .to_string()
}
