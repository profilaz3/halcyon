//! Installing and removing icon packs. A pack's archive is unpacked into a
//! hidden staging folder inside the user icon directory, every icon theme in
//! it is moved into place, and a manifest records which themes Halcyon put
//! there. A theme Halcyon did not install is never replaced or removed.
use super::{Paths, archive, overlay, valid_id};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Manifest {
    #[serde(default)]
    pub themes: Vec<Entry>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Entry {
    /// The theme's directory name.
    pub id: String,
    /// The catalog item it came from.
    pub pack_id: u64,
    pub pack: String,
    /// Seconds since the epoch.
    pub installed: u64,
}

impl Manifest {
    pub fn load(paths: &Paths) -> Self {
        super::read(&paths.manifest(), 1024 * 1024)
            .and_then(|bytes| serde_json::from_slice::<Self>(&bytes).ok())
            .map(|mut manifest| {
                manifest.themes.retain(|entry| valid_id(&entry.id));
                manifest
            })
            .unwrap_or_default()
    }
    pub fn save(&self, paths: &Paths) -> Result<(), String> {
        let json = serde_json::to_vec_pretty(self).map_err(|error| error.to_string())?;
        super::write_atomic(&paths.manifest(), &json).map_err(|error| error.to_string())
    }
    pub fn get(&self, id: &str) -> Option<&Entry> {
        self.themes.iter().find(|entry| entry.id == id)
    }
    pub fn contains(&self, id: &str) -> bool {
        self.get(id).is_some()
    }
    /// Theme ids installed from one catalog item.
    pub fn of_pack(&self, pack_id: u64) -> Vec<String> {
        self.themes
            .iter()
            .filter(|entry| entry.pack_id == pack_id)
            .map(|entry| entry.id.clone())
            .collect()
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Outcome {
    /// Themes now installed, in the archive's order.
    pub installed: Vec<String>,
    /// Themes left alone because a theme of that name is already there and
    /// Halcyon did not install it.
    pub skipped: Vec<String>,
}

impl Outcome {
    /// One line for the UI.
    pub fn message(&self) -> String {
        let names = |ids: &[String]| ids.join(", ");
        match (self.installed.is_empty(), self.skipped.is_empty()) {
            (false, true) => format!("Installed {}", names(&self.installed)),
            (false, false) => format!(
                "Installed {}. Kept your own {} as it was.",
                names(&self.installed),
                names(&self.skipped)
            ),
            (true, false) => format!(
                "Nothing was installed: {} is already on this computer and wasn’t installed by Halcyon",
                names(&self.skipped)
            ),
            (true, true) => "The pack has no icon theme in it".into(),
        }
    }
}

/// The index of an icon theme: `[Icon Theme]` with at least one directory.
pub fn is_theme(dir: &Path) -> bool {
    let index = dir.join("index.theme");
    let Ok(meta) = fs::symlink_metadata(&index) else {
        return false;
    };
    if !meta.is_file() {
        return false;
    }
    let Some(bytes) = super::read(&index, 512 * 1024) else {
        return false;
    };
    let text = String::from_utf8_lossy(&bytes);
    let mut in_theme = false;
    for line in text.lines() {
        let line = line.trim();
        if line.starts_with('[') {
            in_theme = line == "[Icon Theme]";
        } else if in_theme
            && let Some((key, value)) = line.split_once('=')
            && key.trim() == "Directories"
            && value.split(',').any(|dir| !dir.trim().is_empty())
        {
            return true;
        }
    }
    false
}

/// Every icon theme folder in an unpacked archive, outermost first, at most
/// four levels down. A theme's own subfolders are not searched.
pub fn find_themes(root: &Path) -> Vec<PathBuf> {
    let mut found = Vec::new();
    let mut level = vec![root.to_path_buf()];
    for _ in 0..=4 {
        let mut next = Vec::new();
        for dir in level {
            let Ok(entries) = fs::read_dir(&dir) else {
                continue;
            };
            let mut children: Vec<PathBuf> = entries
                .flatten()
                .filter(|entry| entry.file_type().is_ok_and(|kind| kind.is_dir()))
                .map(|entry| entry.path())
                .collect();
            children.sort();
            for child in children {
                if is_theme(&child) {
                    found.push(child);
                } else {
                    next.push(child);
                }
            }
        }
        level = next;
    }
    found
}

fn now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|time| time.as_secs())
        .unwrap_or_default()
}

/// Unpacks `archive` and installs every theme in it. Themes Halcyon
/// installed before are replaced; any other theme of the same name is kept
/// and the new one skipped.
pub fn install_archive(
    paths: &Paths,
    archive_path: &Path,
    pack_id: u64,
    pack: &str,
) -> Result<Outcome, String> {
    fs::create_dir_all(&paths.icons).map_err(|error| error.to_string())?;
    let staging = paths
        .icons
        .join(format!(".halcyon-install-{}", super::unique()));
    let result = install_staged(paths, archive_path, &staging, pack_id, pack);
    let _ = fs::remove_dir_all(&staging);
    result
}

fn install_staged(
    paths: &Paths,
    archive_path: &Path,
    staging: &Path,
    pack_id: u64,
    pack: &str,
) -> Result<Outcome, String> {
    archive::extract(archive_path, staging, archive::Limits::default())?;
    let themes = find_themes(staging);
    if themes.is_empty() {
        return Err("The pack has no icon theme in it".into());
    }
    let mut manifest = Manifest::load(paths);
    let mut outcome = Outcome::default();
    for theme in themes {
        let Some(id) = theme
            .file_name()
            .and_then(|name| name.to_str())
            .map(str::to_string)
        else {
            continue;
        };
        if !valid_id(&id) || overlay::is_overlay(&id) || outcome.installed.contains(&id) {
            continue;
        }
        let target = paths.icons.join(&id);
        let exists = fs::symlink_metadata(&target).is_ok();
        if exists && !manifest.contains(&id) {
            outcome.skipped.push(id);
            continue;
        }
        if exists {
            // Halcyon's own earlier copy: swap it out, then remove it.
            let old = paths
                .icons
                .join(format!(".halcyon-old-{}", super::unique()));
            fs::rename(&target, &old).map_err(|error| error.to_string())?;
            if let Err(error) = fs::rename(&theme, &target) {
                let _ = fs::rename(&old, &target);
                return Err(error.to_string());
            }
            let _ = fs::remove_dir_all(&old);
        } else {
            fs::rename(&theme, &target).map_err(|error| error.to_string())?;
        }
        manifest.themes.retain(|entry| entry.id != id);
        manifest.themes.push(Entry {
            id: id.clone(),
            pack_id,
            pack: pack.to_string(),
            installed: now(),
        });
        outcome.installed.push(id);
    }
    manifest.save(paths)?;
    Ok(outcome)
}

/// Removes a theme Halcyon installed. Anything not in the manifest is
/// refused, whatever its name.
pub fn remove(paths: &Paths, id: &str) -> Result<(), String> {
    let mut manifest = Manifest::load(paths);
    if !valid_id(id) || !manifest.contains(id) {
        return Err(format!("{id} wasn’t installed by Halcyon, so it stays"));
    }
    let target = paths.icons.join(id);
    match fs::symlink_metadata(&target) {
        Ok(meta) if meta.is_dir() => {
            fs::remove_dir_all(&target).map_err(|error| error.to_string())?;
        }
        Ok(_) => return Err(format!("{id} isn’t a folder, so it stays")),
        Err(_) => {}
    }
    manifest.themes.retain(|entry| entry.id != id);
    manifest.save(paths)
}

/// Download progress in bytes, with the total when the server states it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Progress {
    pub done: u64,
    pub total: Option<u64>,
}

/// Downloads `url` to `dest` through a `.part` file, refusing more than
/// `limit` bytes.
pub async fn download(
    url: &str,
    dest: &Path,
    limit: u64,
    mut progress: impl FnMut(Progress),
) -> Result<(), String> {
    use std::io::Write;
    let client = super::ocs::client_for(std::time::Duration::from_secs(15 * 60))?;
    let mut response = client
        .get(url)
        .send()
        .await
        .and_then(reqwest::Response::error_for_status)
        .map_err(|error| super::ocs::describe(&error))?;
    let total = response.content_length();
    if total.is_some_and(|total| total > limit) {
        return Err(format!(
            "The pack is larger than {} MB",
            limit / 1024 / 1024
        ));
    }
    if let Some(parent) = dest.parent() {
        fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    }
    let part = dest.with_extension("part");
    let result = async {
        let mut file = fs::File::create(&part).map_err(|error| error.to_string())?;
        let mut done = 0u64;
        progress(Progress { done, total });
        while let Some(chunk) = response
            .chunk()
            .await
            .map_err(|error| super::ocs::describe(&error))?
        {
            done += chunk.len() as u64;
            if done > limit {
                return Err(format!(
                    "The pack is larger than {} MB",
                    limit / 1024 / 1024
                ));
            }
            file.write_all(&chunk).map_err(|error| error.to_string())?;
            progress(Progress { done, total });
        }
        file.flush().map_err(|error| error.to_string())?;
        fs::rename(&part, dest).map_err(|error| error.to_string())
    }
    .await;
    if result.is_err() {
        let _ = fs::remove_file(&part);
    }
    result
}
