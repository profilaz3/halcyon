//! A small freedesktop icon lookup over explicit directories, so the
//! overlay and the previews resolve icons the way the desktop will, in tests
//! and fixtures too. A theme is the union of `<dir>/<theme>` across the
//! search directories, described by the first `index.theme` found.
use std::collections::HashMap;
use std::path::PathBuf;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Kind {
    Fixed,
    Scalable,
    Threshold,
}

#[derive(Clone, Debug)]
struct Dir {
    path: String,
    size: u32,
    scale: u32,
    min: u32,
    max: u32,
    threshold: u32,
    kind: Kind,
}

impl Dir {
    /// The spec's DirectorySizeDistance.
    fn distance(&self, size: u32) -> u32 {
        let scaled = |value: u32| value * self.scale;
        match self.kind {
            Kind::Fixed => scaled(self.size).abs_diff(size),
            Kind::Scalable => {
                if size < scaled(self.min) {
                    scaled(self.min) - size
                } else if size > scaled(self.max) {
                    size - scaled(self.max)
                } else {
                    0
                }
            }
            Kind::Threshold => {
                let low = scaled(self.size.saturating_sub(self.threshold));
                let high = scaled(self.size + self.threshold);
                if size < low {
                    low - size
                } else if size > high {
                    size - high
                } else {
                    0
                }
            }
        }
    }
}

#[derive(Clone, Debug, Default)]
struct Theme {
    roots: Vec<PathBuf>,
    dirs: Vec<Dir>,
    inherits: Vec<String>,
}

/// Parses an `index.theme`: its directories and what it inherits.
fn parse(text: &str) -> Option<(Vec<Dir>, Vec<String>)> {
    let mut sections: HashMap<String, HashMap<String, String>> = HashMap::new();
    let mut current = None;
    for line in text.lines() {
        let line = line.trim();
        if line.starts_with('#') || line.is_empty() {
            continue;
        }
        if let Some(name) = line
            .strip_prefix('[')
            .and_then(|line| line.strip_suffix(']'))
        {
            current = Some(name.to_string());
            sections.entry(name.to_string()).or_default();
        } else if let (Some(section), Some((key, value))) = (&current, line.split_once('=')) {
            let key = key.trim();
            if !key.contains('[') {
                sections
                    .entry(section.clone())
                    .or_default()
                    .entry(key.to_string())
                    .or_insert_with(|| value.trim().to_string());
            }
        }
    }
    let head = sections.get("Icon Theme")?;
    let list = |key: &str| -> Vec<String> {
        head.get(key)
            .map(|value| {
                value
                    .split(',')
                    .map(str::trim)
                    .filter(|item| !item.is_empty())
                    .map(str::to_string)
                    .collect()
            })
            .unwrap_or_default()
    };
    let mut names = list("Directories");
    names.extend(list("ScaledDirectories"));
    let mut dirs = Vec::new();
    for name in names.into_iter().take(4096) {
        let Some(section) = sections.get(&name) else {
            continue;
        };
        if name.split('/').any(|part| part == ".." || part.is_empty()) {
            continue;
        }
        let number = |key: &str| section.get(key).and_then(|value| value.parse::<u32>().ok());
        let Some(size) = number("Size").filter(|size| (1..=4096).contains(size)) else {
            continue;
        };
        let kind = match section.get("Type").map(String::as_str) {
            Some("Fixed") => Kind::Fixed,
            Some("Scalable") => Kind::Scalable,
            _ => Kind::Threshold,
        };
        dirs.push(Dir {
            path: name,
            size,
            scale: number("Scale").unwrap_or(1).clamp(1, 8),
            min: number("MinSize").unwrap_or(size),
            max: number("MaxSize").unwrap_or(size),
            threshold: number("Threshold").unwrap_or(2),
            kind,
        });
    }
    Some((dirs, list("Inherits")))
}

/// Icon lookups over a fixed list of icon directories.
pub struct Resolver {
    search: Vec<PathBuf>,
    themes: HashMap<String, Option<Theme>>,
}

impl Resolver {
    pub fn new(search: Vec<PathBuf>) -> Self {
        Self {
            search,
            themes: HashMap::new(),
        }
    }

    fn theme(&mut self, id: &str) -> Option<&Theme> {
        if !self.themes.contains_key(id) {
            let theme = super::valid_id(id).then(|| self.load(id)).flatten();
            self.themes.insert(id.to_string(), theme);
        }
        self.themes.get(id).and_then(Option::as_ref)
    }

    fn load(&self, id: &str) -> Option<Theme> {
        let roots: Vec<PathBuf> = self
            .search
            .iter()
            .map(|dir| dir.join(id))
            .filter(|dir| dir.is_dir())
            .collect();
        let (dirs, inherits) = roots.iter().find_map(|root| {
            let bytes = super::read(&root.join("index.theme"), 1024 * 1024)?;
            parse(&String::from_utf8_lossy(&bytes))
        })?;
        Some(Theme {
            roots,
            dirs,
            inherits,
        })
    }

    /// The theme, what it inherits depth first, and hicolor last.
    pub fn chain(&mut self, id: &str) -> Vec<String> {
        let mut out = Vec::new();
        self.walk(id, &mut out, 0);
        out.retain(|theme| theme != "hicolor");
        out.push("hicolor".into());
        out
    }

    fn walk(&mut self, id: &str, out: &mut Vec<String>, depth: usize) {
        if depth > 12 || out.iter().any(|seen| seen == id) {
            return;
        }
        let Some(theme) = self.theme(id) else {
            return;
        };
        let inherits = theme.inherits.clone();
        out.push(id.to_string());
        for parent in inherits {
            self.walk(&parent, out, depth + 1);
        }
    }

    /// The best file for `name` at `size` in this one theme: the smallest
    /// size distance, an SVG on a tie, then the larger picture.
    pub fn find_in(&mut self, theme: &str, name: &str, size: u32) -> Option<PathBuf> {
        if name.is_empty() || name.contains(['/', '\\']) || name.starts_with('.') {
            return None;
        }
        let theme = self.theme(theme)?;
        let mut best: Option<((u32, bool, u32), PathBuf)> = None;
        for dir in &theme.dirs {
            let distance = dir.distance(size);
            if best.as_ref().is_some_and(|(rank, _)| rank.0 < distance) {
                continue;
            }
            for root in &theme.roots {
                for extension in ["svg", "png"] {
                    let path = root.join(&dir.path).join(format!("{name}.{extension}"));
                    if !path.is_file() {
                        continue;
                    }
                    // Lower is better: distance, then svg, then larger.
                    let rank = (
                        distance,
                        extension != "svg",
                        u32::MAX - dir.size * dir.scale,
                    );
                    if best.as_ref().is_none_or(|(old, _)| rank < *old) {
                        best = Some((rank, path));
                    }
                }
            }
        }
        best.map(|(_, path)| path)
    }

    /// The first theme in `chain` that has `name`.
    pub fn find(&mut self, chain: &[String], name: &str, size: u32) -> Option<(String, PathBuf)> {
        chain
            .iter()
            .find_map(|theme| Some((theme.clone(), self.find_in(theme, name, size)?)))
    }

    /// The first of `names` the chain has, for previews with fallbacks.
    pub fn find_any(&mut self, chain: &[String], names: &[&str], size: u32) -> Option<PathBuf> {
        names
            .iter()
            .find_map(|name| self.find(chain, name, size).map(|(_, path)| path))
    }
}
