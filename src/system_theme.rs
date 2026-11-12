//! Palette links: optional COSMIC accent matching (`--apply-cosmic-colors`,
//! `--apply-palette-links … --colors`) and the matching shared background
//! (`--apply-palette-links … --wallpaper`), plus `--restore-background`.
//!
//! The accent derivation lives in `desktop::appearance` and is shared with
//! the System appearance editor; the background is rewritten through the
//! wallpaper adapter, keeping every other member of `all`. Both parts are
//! applied as ONE plain store change with the same lock, conflict check,
//! journal and rollback. The first colour change still records every
//! original theme file in `cosmic-colors-original.json`.
use crate::config::Palette;
use crate::desktop::appearance::{self, AppearanceDraft};
use crate::desktop::panels::Setting;
use crate::desktop::wallpaper::{self, BgSource, Target, WallpaperDraft};
use crate::desktop::{DesktopError, DesktopPatch, DesktopStore, RawValue};
use cosmic::cosmic_theme::palette::Srgb;
use serde::{Deserialize, Serialize};
use std::fs;
use std::io::{self, Read, Write};
use std::os::fd::AsRawFd;
use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

const IDS: [&str; 4] = [
    "com.system76.CosmicTheme.Dark",
    "com.system76.CosmicTheme.Dark.Builder",
    "com.system76.CosmicTheme.Light",
    "com.system76.CosmicTheme.Light.Builder",
];
const BACKUP: &str = "cosmic-colors-original.json";
/// Taken before the shared store lock, so an older Halcyon release
/// that only knows this lock never interleaves with us.
const LEGACY_LOCK: &str = "cosmic-colors.lock";
/// The controller's background backup is one RON value.
const MAX_BACKGROUND_BYTES: u64 = 256 * 1024;
static NEXT_FILE: AtomicU64 = AtomicU64::new(0);

/// Which parts of a card palette to carry over to COSMIC.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Links {
    /// The shared background becomes the palette's bundled wallpaper.
    pub wallpaper: bool,
    /// Both COSMIC accents follow the palette.
    pub colors: bool,
}

fn preview_refusal(links: Links) -> Option<String> {
    std::env::var_os("DESKTOP_CARDS_CONFIG").map(|_| {
        if links.wallpaper {
            "Palette matching cannot be applied from preview mode".into()
        } else {
            "COSMIC colors cannot be applied from preview mode".into()
        }
    })
}

/// Controller requests carry app preferences and native changes through one
/// transaction. The Python parent must not hold config.json.lock.
#[derive(Clone, Copy, Debug)]
pub enum ControllerRequest<'a> {
    Palette(&'a str),
    CosmicColors(bool),
}

pub fn controller(request: ControllerRequest<'_>) -> Result<(), String> {
    if std::env::var_os("DESKTOP_CARDS_CONFIG").is_some() {
        return Err("Desktop changes are disabled in an isolated preview".into());
    }
    let store = DesktopStore::from_env().map_err(|e| e.to_string())?;
    let bundled = wallpaper::bundled_dir(&store);
    controller_with(&store, request, &bundled)
}

pub fn controller_with(
    store: &DesktopStore,
    request: ControllerRequest<'_>,
    bundled: &Path,
) -> Result<(), String> {
    use crate::desktop::store::{Owner, Source, WidgetPatch, widget};
    let (raw, config) = wallpaper::automatic::read_config(store)?;
    let mut widgets = WidgetPatch::new();
    let (name, links) = match request {
        ControllerRequest::Palette(name) => {
            if !Palette::CHOICES.iter().any(|(key, _)| *key == name) {
                return Err("Unknown Halcyon palette".into());
            }
            widgets.set("/theme", widget::get(&raw, "/theme"), Some(name.into()))?;
            widgets.set(
                "/appearance/accent",
                widget::get(&raw, "/appearance/accent"),
                config.appearance.family.map(|_| {
                    let color = Palette::named(name).accent;
                    serde_json::json!([
                        (color.r * 255.).round() as u8,
                        (color.g * 255.).round() as u8,
                        (color.b * 255.).round() as u8,
                    ])
                }),
            )?;
            widgets.set(
                "/appearance/color_policy",
                widget::get(&raw, "/appearance/color_policy"),
                Some("custom".into()),
            )?;
            widgets.set(
                "/appearance/wallpaper_colors",
                widget::get(&raw, "/appearance/wallpaper_colors"),
                Some(false.into()),
            )?;
            if config.appearance.color_source == crate::config::ColorSource::Desktop {
                widgets.set(
                    "/appearance/color_source",
                    widget::get(&raw, "/appearance/color_source"),
                    Some("custom".into()),
                )?;
            }
            (
                name,
                Links {
                    wallpaper: config.appearance.match_wallpaper,
                    colors: config.appearance.match_cosmic_colors,
                },
            )
        }
        ControllerRequest::CosmicColors(on) => {
            widgets.set(
                "/appearance/match_cosmic_colors",
                widget::get(&raw, "/appearance/match_cosmic_colors"),
                Some(on.into()),
            )?;
            (
                config.theme.as_str(),
                Links {
                    wallpaper: false,
                    colors: on,
                },
            )
        }
    };
    let mut patch = DesktopPatch::new();
    if links.colors {
        let color = Palette::named(name).accent;
        patch = appearance::palette_patch(
            &store
                .load(&AppearanceDraft::keys())
                .map_err(|e| e.to_string())?,
            Srgb::new(color.r, color.g, color.b),
        )?;
    }
    if links.wallpaper {
        patch
            .merge(background_patch(
                store,
                &wallpaper::bundled_path(bundled, name),
            )?)
            .map_err(|e| e.to_string())?;
    }
    // Planning is read-only. The same raw preferences are guarded under the
    // config lock when native and app writes commit together.
    for pointer in [
        "/theme",
        "/appearance/match_wallpaper",
        "/appearance/match_cosmic_colors",
        "/appearance/color_source",
        "/appearance/family",
    ] {
        if !widgets.iter().any(|(p, _)| p == pointer) {
            let original = widget::get(&raw, pointer);
            widgets.set(pointer, original.clone(), original)?;
        }
    }
    store.preflight(&patch).map_err(|e| e.to_string())?;
    if patch.is_empty() && widgets.is_empty() {
        return Ok(());
    }
    // Preserve the old controller's restore-wallpaper entry point using the
    // same immutable evidence; the collection itself no longer needs editing.
    if links.wallpaper {
        let baseline = crate::desktop::recovery::ensure_baseline(
            store,
            crate::desktop::recovery::BaselineKind::OriginalDesktop,
        )
        .map_err(|e| e.to_string())?;
        if let Some(parent) = store.app_config().and_then(Path::parent) {
            let saved = parent.join("original-background.ron");
            let original = baseline
                .domains
                .iter()
                .flat_map(|d| &d.native)
                .find(|r| r.supported_key().as_ref() == Some(&wallpaper::shared_key()));
            if let Some(bytes) = original
                .map(|r| r.contents())
                .transpose()
                .map_err(|e| e.to_string())?
                .flatten()
            {
                match fs::OpenOptions::new()
                    .write(true)
                    .create_new(true)
                    .mode(0o600)
                    .custom_flags(libc::O_NOFOLLOW)
                    .open(saved)
                {
                    Ok(mut file) => {
                        file.write_all(&bytes)
                            .and_then(|_| file.sync_all())
                            .map_err(|e| e.to_string())?;
                    }
                    Err(e) if e.kind() == io::ErrorKind::AlreadyExists => {}
                    Err(e) => return Err(e.to_string()),
                }
            }
        }
    }
    store
        .apply_action(
            &patch,
            Some(&widgets),
            Owner::Appearance,
            match request {
                ControllerRequest::Palette(_) => "CLI palette",
                ControllerRequest::CosmicColors(_) => "CLI COSMIC colors",
            },
            Source::Manual,
        )
        .map(|_| ())
        .map_err(|e| e.to_string())
}

/// The helper is deliberately separate from the GUI's preview configuration.
pub fn apply(name: &str) -> Result<(), String> {
    apply_links(
        name,
        Links {
            wallpaper: false,
            colors: true,
        },
    )
}

/// Applies the chosen links of palette `name` to the live desktop.
pub fn apply_links(name: &str, links: Links) -> Result<(), String> {
    if let Some(refusal) = preview_refusal(links) {
        return Err(refusal);
    }
    let store = DesktopStore::live().map_err(|error| error.to_string())?;
    let bundled = wallpaper::bundled_dir(&store);
    apply_links_with(&store, name, links, &bundled)
}

/// Matches the accent of both COSMIC themes to palette `name` in `store`.
#[cfg(test)]
pub fn apply_with(store: &DesktopStore, name: &str) -> Result<(), String> {
    apply_links_with(
        store,
        name,
        Links {
            wallpaper: false,
            colors: true,
        },
        Path::new("/nonexistent"),
    )
}

/// Applies the links of palette `name` in `store` as one change: the
/// shared background's source becomes `<bundled>/<name>.jpg` (every
/// other member kept, `output` = `all`) and/or both accents follow the
/// palette.
///
/// Refusals that can be predicted (an unknown palette, a read-only store,
/// fixture paths that lead outside its root, a missing bundled file, a
/// shared background set elsewhere) are reported before the legacy lock or
/// colour backup is created. Everything is planned again under the lock.
pub fn apply_links_with(
    store: &DesktopStore,
    name: &str,
    links: Links,
    bundled: &Path,
) -> Result<(), String> {
    if !Palette::CHOICES.iter().any(|(key, _)| *key == name) {
        return Err("Unknown Halcyon palette".into());
    }
    if !links.wallpaper && !links.colors {
        return Ok(());
    }
    let color = Palette::named(name).accent;
    let accent = Srgb::new(color.r, color.g, color.b);
    let text = |error: DesktopError| error.to_string();
    let lock_path = store.state_file(LEGACY_LOCK).map_err(text)?;
    let backup = match links.colors {
        true => Some(store.state_file(BACKUP).map_err(text)?),
        false => None,
    };
    let picture = wallpaper::bundled_path(bundled, name);
    let plan = || -> Result<DesktopPatch, String> {
        let mut patch = DesktopPatch::new();
        if links.colors {
            let snapshot = store.load(&AppearanceDraft::keys()).map_err(text)?;
            patch = appearance::palette_patch(&snapshot, accent)?;
        }
        if links.wallpaper {
            patch
                .merge(background_patch(store, &picture)?)
                .map_err(text)?;
        }
        store.preflight(&patch).map_err(text)?;
        Ok(patch)
    };
    // Reads only; repeated under the lock below.
    plan()?;
    fs::create_dir_all(store.state_dir()).map_err(|e| e.to_string())?;
    let lock = fs::OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .mode(0o600)
        .open(lock_path)
        .map_err(|e| e.to_string())?;
    loop {
        // SAFETY: the owned file remains alive until the change finishes.
        if unsafe { libc::flock(lock.as_raw_fd(), libc::LOCK_EX) } == 0 {
            break;
        }
        let error = io::Error::last_os_error();
        if error.kind() != io::ErrorKind::Interrupted {
            return Err(error.to_string());
        }
    }
    if store.pending_recovery().map_err(text)?.is_some() {
        return Err(recovery_message(links));
    }
    let patch = plan()?;
    crate::desktop::recovery::ensure_baseline(
        store,
        crate::desktop::recovery::BaselineKind::OriginalDesktop,
    )
    .map_err(text)?;
    let original = match &backup {
        Some(backup) => {
            let root = store
                .user_root()
                .parent()
                .ok_or("Configuration directory unavailable")?;
            backup_once(root, backup)?;
            format!(" Original backup: {}", backup.display())
        }
        None => String::new(),
    };
    let what = match (links.wallpaper, links.colors) {
        (true, true) => "The background and COSMIC colors",
        (true, false) => "The background",
        (false, _) => "COSMIC colors",
    };
    let restored = if links.wallpaper {
        "previous settings restored"
    } else {
        "previous colors restored"
    };
    match store.apply(&patch) {
        Ok(_) => Ok(()),
        Err(DesktopError::RolledBack(error)) => Err(format!(
            "{what} were not applied; {restored}: {error}.{original}"
        )),
        Err(DesktopError::RollbackIncomplete { error, keys, .. }) => Err(format!(
            "{what} change failed: {error}. Could not restore: {}. Open Halcyon settings to restore.{original}",
            keys.iter()
                .map(ToString::to_string)
                .collect::<Vec<_>>()
                .join(", ")
        )),
        Err(DesktopError::Conflict(_)) => Err(format!(
            "{what} changed while matching; nothing was changed. Try again."
        )),
        Err(DesktopError::RecoveryPending) => Err(recovery_message(links)),
        Err(error) => Err(format!("{error}.{original}")),
    }
}

/// The shared background with `picture` as its source, planned through
/// the wallpaper draft so the same validation, preservation and source
/// checks apply. Reads only.
fn background_patch(store: &DesktopStore, picture: &Path) -> Result<DesktopPatch, String> {
    wallpaper::check_path(picture)
        .map_err(|_| format!("The palette wallpaper is missing: {}", picture.display()))?;
    // Only the shared entry is edited: the fixed keys are all it reads.
    let mut draft =
        WallpaperDraft::load_keys(store, &[], Vec::new()).map_err(|error| error.to_string())?;
    if let Some(Setting::Unsupported(_)) = draft.entry(&Target::Shared) {
        return Err(
            "The shared background is set elsewhere with settings this version does not support; nothing was changed."
                .into(),
        );
    }
    draft.set_source(&Target::Shared, BgSource::Path(picture.to_path_buf()));
    draft.check_sources()?;
    let plan = draft.plan();
    if let Some(blocked) = plan.blocked.first() {
        return Err(format!("{} {}", blocked.key, blocked.reason));
    }
    Ok(plan.patch)
}

/// Restores the controller's one-time backup of the shared background
/// (`original-background.ron`) to the live desktop.
pub fn restore_background(path: &Path) -> Result<(), String> {
    if std::env::var_os("DESKTOP_CARDS_CONFIG").is_some() {
        return Err("The wallpaper cannot be restored from preview mode".into());
    }
    let store = DesktopStore::live().map_err(|error| error.to_string())?;
    restore_background_with(&store, path)
}

/// Writes the background backup at `path` as `all` in `store`, through the
/// store, when it is a valid, supported entry; otherwise nothing changes.
pub fn restore_background_with(store: &DesktopStore, path: &Path) -> Result<(), String> {
    let invalid = |reason: String| {
        format!(
            "The saved wallpaper {} can’t be restored ({reason}); nothing was changed.",
            path.display()
        )
    };
    let file = fs::File::open(path).map_err(|error| invalid(error.to_string()))?;
    let mut text = String::new();
    file.take(MAX_BACKGROUND_BYTES + 1)
        .read_to_string(&mut text)
        .map_err(|error| invalid(error.to_string()))?;
    if text.len() as u64 > MAX_BACKGROUND_BYTES {
        return Err(invalid("it is too large".into()));
    }
    let value = RawValue::parse(text).map_err(|error| invalid(error.to_string()))?;
    wallpaper::check_entry(&value).map_err(invalid)?;
    if value
        .decode::<wallpaper::BgEntry>()
        .is_ok_and(|entry| entry.output != wallpaper::SHARED)
    {
        return Err(invalid("it is not the shared background".into()));
    }
    let key = wallpaper::shared_key();
    let mut patch = DesktopPatch::new();
    patch.set(
        key.clone(),
        store.read_user(&key).map_err(|error| error.to_string())?,
        Some(value),
    );
    store.preflight(&patch).map_err(|error| error.to_string())?;
    match store.apply(&patch) {
        Ok(_) => Ok(()),
        Err(DesktopError::RecoveryPending) => Err(
            "An earlier desktop change was interrupted. Open Halcyon settings to restore or keep it; the wallpaper was not changed."
                .into(),
        ),
        Err(error) => Err(error.to_string()),
    }
}

fn recovery_message(links: Links) -> String {
    let what = match (links.wallpaper, links.colors) {
        (true, true) => "the background and COSMIC colors were",
        (true, false) => "the background was",
        (false, _) => "COSMIC colors were",
    };
    format!(
        "An earlier desktop change was interrupted. Open Halcyon settings to restore or keep it; {what} not changed."
    )
}

#[derive(Debug, Serialize, Deserialize)]
struct Original {
    path: PathBuf,
    bytes: Option<Vec<u8>>,
    mode: u32,
}

impl Original {
    fn read(path: &Path) -> Result<Self, String> {
        match fs::symlink_metadata(path) {
            Ok(metadata) if metadata.is_file() => Ok(Self {
                path: path.into(),
                bytes: Some(fs::read(path).map_err(|e| e.to_string())?),
                mode: metadata.permissions().mode() & 0o777,
            }),
            Ok(_) => Err(format!(
                "Theme setting is not a regular file: {}",
                path.display()
            )),
            Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(Self {
                path: path.into(),
                bytes: None,
                mode: 0o600,
            }),
            Err(error) => Err(format!("{}: {error}", path.display())),
        }
    }
}

fn atomic(path: &Path, bytes: &[u8], mode: u32) -> Result<(), String> {
    let parent = path.parent().ok_or("Theme path has no parent")?;
    fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    let temporary = parent.join(format!(
        ".desktop-cards-{}-{}.tmp",
        std::process::id(),
        NEXT_FILE.fetch_add(1, Ordering::Relaxed)
    ));
    let result = (|| -> io::Result<()> {
        let mut file = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(mode)
            .open(&temporary)?;
        file.set_permissions(fs::Permissions::from_mode(mode))?;
        file.write_all(bytes)?;
        file.sync_all()?;
        fs::rename(&temporary, path)
    })();
    let _ = fs::remove_file(temporary);
    result.map_err(|error| format!("{}: {error}", path.display()))
}

/// Records every theme and mode file once, before the first change. An
/// existing backup is kept as it is (it must still be readable).
fn backup_once(root: &Path, backup: &Path) -> Result<(), String> {
    if backup.exists() {
        let _: Vec<Original> =
            serde_json::from_slice(&fs::read(backup).map_err(|e| e.to_string())?)
                .map_err(|error| format!("Existing COSMIC color backup is invalid: {error}"))?;
        return Ok(());
    }
    fn collect(root: &Path, path: &Path, files: &mut Vec<Original>) -> Result<(), String> {
        let entries = match fs::read_dir(path) {
            Ok(entries) => entries,
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(()),
            Err(error) => return Err(error.to_string()),
        };
        for entry in entries {
            let entry = entry.map_err(|e| e.to_string())?;
            if entry.file_type().map_err(|e| e.to_string())?.is_dir() {
                collect(root, &entry.path(), files)?;
            } else {
                let mut original = Original::read(&entry.path())?;
                original.path = original
                    .path
                    .strip_prefix(root)
                    .map_err(|e| e.to_string())?
                    .into();
                files.push(original);
            }
        }
        Ok(())
    }
    let mut files = Vec::new();
    for id in IDS.into_iter().chain(["com.system76.CosmicTheme.Mode"]) {
        collect(root, &root.join("cosmic").join(id), &mut files)?;
    }
    files.sort_by(|left, right| left.path.cmp(&right.path));
    atomic(
        backup,
        &serde_json::to_vec_pretty(&files).map_err(|e| e.to_string())?,
        0o600,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::desktop::store::{Access, Fault};
    use crate::desktop::{ConfigKey, DesktopPatch, RawValue};
    use cosmic::cosmic_config::{Config, ConfigSet, CosmicConfigEntry};
    use cosmic::cosmic_theme::palette::{Srgba, WithAlpha};
    use cosmic::cosmic_theme::{Theme, ThemeBuilder};
    use std::collections::BTreeMap;

    struct Staging(PathBuf);
    impl Staging {
        fn new() -> Self {
            let path = std::env::temp_dir().join(format!(
                "desktop-cards-theme-{}-{}",
                std::process::id(),
                NEXT_FILE.fetch_add(1, Ordering::Relaxed)
            ));
            fs::create_dir(&path).unwrap();
            fs::set_permissions(&path, fs::Permissions::from_mode(0o700)).unwrap();
            Self(path)
        }
    }
    impl Drop for Staging {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    /// A store over `root` (a config directory) with no system defaults.
    fn store(root: &Path, data: &Path) -> DesktopStore {
        DesktopStore::with_roots(root.join("cosmic"), Vec::new(), data.into(), Access::Live)
    }
    fn apply_at(name: &str, root: &Path, data: &Path) -> Result<(), String> {
        apply_with(&store(root, data), name)
    }
    fn read_entry<T: CosmicConfigEntry>(config: &Config) -> T {
        T::get_entry(config).unwrap_or_else(|(errors, value)| {
            assert!(
                errors.iter().all(|error| !error.is_err()
                    || matches!(error, cosmic::cosmic_config::Error::GetKey(_, e) if e.kind() == io::ErrorKind::NotFound)),
                "{errors:?}"
            );
            value
        })
    }

    fn files(root: &Path) -> BTreeMap<PathBuf, Vec<u8>> {
        fn walk(root: &Path, path: &Path, result: &mut BTreeMap<PathBuf, Vec<u8>>) {
            let Ok(entries) = fs::read_dir(path) else {
                return;
            };
            for entry in entries {
                let path = entry.unwrap().path();
                if path.is_dir() {
                    walk(root, &path, result);
                } else {
                    result.insert(
                        path.strip_prefix(root).unwrap().into(),
                        fs::read(path).unwrap(),
                    );
                }
            }
        }
        let mut result = BTreeMap::new();
        walk(root, root, &mut result);
        result
    }

    fn fixture(root: &Path) {
        for (index, builder) in [ThemeBuilder::dark(), ThemeBuilder::light()]
            .into_iter()
            .enumerate()
        {
            let mut builder = builder.bg_color(Srgba::new(0.18, 0.21, 0.26, 1.));
            builder.spacing.space_m = 37;
            builder.gaps = (13, 17);
            builder.window_hint = Some(Srgb::new(0.7, 0.3, 0.2));
            let b = Config::with_custom_path(IDS[index * 2 + 1], 2, root.into()).unwrap();
            let t = Config::with_custom_path(IDS[index * 2], 2, root.into()).unwrap();
            builder.write_entry(&b).unwrap();
            let mut theme = builder.build();
            theme.button.base = Srgba::new(0.2, 0.25, 0.3, 0.8);
            theme.write_entry(&t).unwrap();
            let folder = root.join("cosmic").join(IDS[index * 2]).join("v2");
            fs::write(folder.join("future_control"), "keep this unknown setting\n").unwrap();
            fs::set_permissions(folder.join("accent"), fs::Permissions::from_mode(0o640)).unwrap();
        }
        let mode = root.join("cosmic/com.system76.CosmicTheme.Mode/v1");
        fs::create_dir_all(&mode).unwrap();
        fs::write(mode.join("is_dark"), "false\n").unwrap();
        fs::write(mode.join("auto_switch"), "true\n").unwrap();
    }

    fn stored<T: Serialize>(value: &T) -> String {
        RawValue::encode(value).unwrap().as_str().to_string()
    }

    #[test]
    fn accents_follow_cosmic_derivation_in_both_modes_without_resetting_customizations() {
        let temp = Staging::new();
        let root = temp.0.join("config");
        fixture(&root);
        let before: Vec<Theme> = [IDS[0], IDS[2]]
            .map(|id| read_entry(&Config::with_custom_path(id, 2, root.clone()).unwrap()))
            .into();
        apply_at("ocean", &root, &temp.0.join("data")).unwrap();
        let requested = Palette::named("ocean").accent;
        let accent = Srgb::new(requested.r, requested.g, requested.b);
        for (index, current) in before.into_iter().enumerate() {
            let builder: ThemeBuilder =
                read_entry(&Config::with_custom_path(IDS[index * 2 + 1], 2, root.clone()).unwrap());
            let changed: Theme =
                read_entry(&Config::with_custom_path(IDS[index * 2], 2, root.clone()).unwrap());
            let expected = builder.clone().accent(accent).build();
            assert_eq!(stored(&changed.accent), stored(&expected.accent));
            assert_eq!(
                stored(&changed.accent_button),
                stored(&expected.accent_button)
            );
            assert_eq!(
                stored(&changed.text_button.on),
                stored(&expected.text_button.on)
            );
            assert_eq!(
                stored(&changed.button.focus),
                stored(&expected.button.focus)
            );
            assert_eq!(changed.button.base, current.button.base);
            assert_eq!(changed.bg_color(), current.bg_color());
            assert_eq!(changed.spacing, current.spacing);
            assert_eq!(changed.corner_radii, current.corner_radii);
            assert_eq!(changed.gaps, current.gaps);
            assert_eq!(changed.window_hint, current.window_hint);
            assert_eq!(changed.is_dark, current.is_dark);
            assert_eq!(changed.palette, current.palette);
        }
    }

    #[test]
    fn file_updates_preserve_mode_and_nonaccent_preferences_and_keep_first_backup() {
        let temp = Staging::new();
        let root = temp.0.join("config");
        let data = temp.0.join("data");
        fixture(&root);
        let before = files(&root);
        apply_at("ocean", &root, &data).unwrap();
        let after = files(&root);
        for (path, bytes) in &before {
            let key = path.file_name().unwrap().to_str().unwrap();
            if [
                "spacing",
                "corner_radii",
                "gaps",
                "window_hint",
                "palette",
                "future_control",
                "is_dark",
                "auto_switch",
            ]
            .contains(&key)
            {
                assert_eq!(after[path], *bytes, "{} changed", path.display());
            }
        }
        for index in 0..2 {
            let config = Config::with_custom_path(IDS[index * 2], 2, root.clone()).unwrap();
            let updated: Theme = read_entry(&config);
            assert_eq!(
                updated.bg_color(),
                Srgb::<u8>::new(46, 54, 66)
                    .into_format::<f32>()
                    .with_alpha(1.)
            );
            assert_eq!(updated.spacing.space_m, 37);
            assert_eq!(updated.gaps, (13, 17));
            let accent = root.join("cosmic").join(IDS[index * 2]).join("v2/accent");
            assert_eq!(
                fs::metadata(accent).unwrap().permissions().mode() & 0o777,
                0o640
            );
            let builder = Config::with_custom_path(IDS[index * 2 + 1], 2, root.clone()).unwrap();
            let builder: ThemeBuilder = read_entry(&builder);
            let requested = Palette::named("ocean").accent;
            let requested: Srgb<u8> =
                Srgb::new(requested.r, requested.g, requested.b).into_format();
            assert_eq!(builder.accent.map(Srgb::into_format), Some(requested));
        }
        let backup = fs::read(data.join(BACKUP)).unwrap();
        let originals: Vec<Original> = serde_json::from_slice(&backup).unwrap();
        let backed_up: BTreeMap<_, _> = originals
            .into_iter()
            .map(|original| (original.path, original.bytes.unwrap()))
            .collect();
        assert_eq!(backed_up, before);
        apply_at("rose", &root, &data).unwrap();
        assert_eq!(fs::read(data.join(BACKUP)).unwrap(), backup);
        // The shared journal is gone; both locks exist in the data folder.
        assert!(data.join(LEGACY_LOCK).exists() && data.join("desktop.lock").exists());
        assert!(!data.join("desktop-journal.json").exists());
    }

    #[test]
    fn failed_second_mode_preparation_and_invalid_backup_leave_colors_unchanged() {
        let temp = Staging::new();
        let root = temp.0.join("config");
        let data = temp.0.join("data");
        fixture(&root);
        let broken = root.join("cosmic").join(IDS[2]).join("v2/accent");
        let original = fs::read(&broken).unwrap();
        fs::write(&broken, "invalid RON (").unwrap();
        let before = files(&root);
        assert!(apply_at("ocean", &root, &data).is_err());
        assert_eq!(files(&root), before);
        assert!(!data.exists(), "no lock or backup for an unreadable theme");
        fs::write(broken, original).unwrap();
        fs::create_dir_all(&data).unwrap();
        fs::write(data.join(BACKUP), "invalid backup").unwrap();
        let before = files(&root);
        assert!(apply_at("rose", &root, &data).is_err());
        assert_eq!(files(&root), before);
    }

    #[test]
    fn partial_apply_failure_restores_existing_files_and_removes_new_keys() {
        let temp = Staging::new();
        let root = temp.0.join("config");
        let data = temp.0.join("data");
        fixture(&root);
        // The light theme starts without some derived keys, so the change
        // adds files; they must disappear again.
        let light = root.join("cosmic").join(IDS[2]).join("v2");
        fs::remove_file(light.join("accent_button")).unwrap();
        let before = files(&root);
        let store = store(&root, &data);
        let snapshot = store.load(&AppearanceDraft::keys()).unwrap();
        let patch = appearance::palette_patch(&snapshot, Srgb::new(0.9, 0.2, 0.4)).unwrap();
        let last: ConfigKey = patch.keys().last().unwrap().clone();
        assert!(
            patch
                .keys()
                .any(|key| key.namespace() == IDS[2] && key.key() == "accent_button")
        );
        store.inject(Some(Fault::FailWrite(last)));
        let error = apply_with(&store, "rose").unwrap_err();
        assert!(error.contains("previous colors restored"), "{error}");
        assert_eq!(files(&root), before);
        assert!(!light.join("accent_button").exists());
        assert_eq!(
            fs::metadata(root.join("cosmic").join(IDS[0]).join("v2/accent"))
                .unwrap()
                .permissions()
                .mode()
                & 0o777,
            0o640
        );
    }

    #[test]
    fn absent_and_sparse_theme_files_use_mode_specific_defaults() {
        let temp = Staging::new();
        let root = temp.0.join("config");
        let data = temp.0.join("data");
        let light = Config::with_custom_path(IDS[3], 2, root.clone()).unwrap();
        light.set("gaps", (21u32, 34u32)).unwrap();
        apply_at("graphite", &root, &data).unwrap();
        // Read like COSMIC apps that start from the variant's defaults.
        let sparse = |id, default: Theme| {
            let config = Config::with_custom_path(id, 2, root.clone()).unwrap();
            let mut value = default;
            let keys: Vec<String> = serde_json::to_value(&value)
                .unwrap()
                .as_object()
                .unwrap()
                .keys()
                .cloned()
                .collect();
            let _ = value.update_keys(&config, &keys);
            value
        };
        let dark = sparse(IDS[0], ThemeBuilder::dark().build());
        let light = sparse(IDS[2], ThemeBuilder::light().build());
        assert!(dark.is_dark);
        assert!(!light.is_dark);
        assert_ne!(
            dark.accent_color(),
            ThemeBuilder::dark().build().accent_color()
        );
        assert_ne!(
            light.accent_color(),
            ThemeBuilder::light().build().accent_color()
        );
        assert!(!root.join("cosmic/com.system76.CosmicTheme.Mode").exists());
        assert_eq!(
            fs::read_to_string(root.join("cosmic").join(IDS[3]).join("v2/gaps")).unwrap(),
            "(21, 34)"
        );
    }

    #[test]
    fn pending_recovery_blocks_palette_matching_before_any_write() {
        let temp = Staging::new();
        let root = temp.0.join("config");
        let data = temp.0.join("data");
        fixture(&root);
        let store = store(&root, &data);
        // An interrupted editor change leaves the shared journal behind.
        let key = ConfigKey::new("com.system76.CosmicTk", 1, "icon_theme").unwrap();
        let mut patch = DesktopPatch::new();
        patch.set(key, None, Some(RawValue::parse("\"Pop\"").unwrap()));
        store.inject(Some(Fault::InterruptAfter(0)));
        assert!(store.apply(&patch).is_err());
        store.inject(None);
        let before = files(&root);
        let error = apply_at("ocean", &root, &data).unwrap_err();
        assert!(error.contains("interrupted"), "{error}");
        assert_eq!(files(&root), before);
        assert!(
            !data.join(BACKUP).exists(),
            "no backup before a refused change"
        );
    }

    /// The editor and palette matching target the same keys: whichever
    /// writes second sees the other's change as a conflict.
    #[test]
    fn palette_matching_while_an_appearance_draft_is_open_conflicts_cleanly() {
        use crate::desktop::appearance::{ColorRole, Control, ThemeVariant, Value, parse_hex};
        let temp = Staging::new();
        let root = temp.0.join("config");
        let data = temp.0.join("data");
        fixture(&root);
        let store = store(&root, &data);
        let mut draft = AppearanceDraft::load(&store).unwrap();
        draft.set(
            Control::Color(ThemeVariant::Light, ColorRole::Accent),
            Value::Color(parse_hex("#E01B24", false)),
        );
        apply_at("teal", &root, &data).unwrap();
        let matched = files(&root);
        draft.refresh(store.load(&AppearanceDraft::keys()).unwrap());
        assert!(!draft.conflicts().is_empty());
        assert!(matches!(
            store.apply(&draft.patch()),
            Err(DesktopError::Conflict(_))
        ));
        assert_eq!(files(&root), matched, "the palette change is kept");
    }

    /// `apply_with` is reusable with any store: a read-only store, or a
    /// fixture whose state, auxiliary files or written keys lead into a
    /// second tree, is refused before the legacy lock or backup exists.
    /// Both temporary trees stay byte-identical.
    #[test]
    fn refused_stores_and_fixture_escapes_create_no_lock_or_backup_in_either_tree() {
        use crate::desktop::store::tests::tree;
        use std::os::unix::fs::symlink;
        let temp = Staging::new();
        let live = temp.0.join("fake-live");
        fixture(&live.join("config"));
        fs::create_dir_all(live.join("state")).unwrap();
        // Existing files a symlinked leaf in the fixture points to.
        fs::create_dir_all(live.join("targets")).unwrap();
        for name in [BACKUP, LEGACY_LOCK, "desktop.lock", "desktop-journal.json"] {
            fs::write(live.join("targets").join(name), "live\n").unwrap();
        }
        type Link = fn(&Path, &Path);
        let cases: [(&str, Link); 8] = [
            ("state", |fixture, live| {
                fs::remove_dir(fixture.join("state")).unwrap();
                symlink(live.join("state"), fixture.join("state")).unwrap();
            }),
            ("backup leaf", |fixture, live| {
                symlink(
                    live.join("targets").join(BACKUP),
                    fixture.join("state").join(BACKUP),
                )
                .unwrap();
            }),
            ("legacy lock leaf", |fixture, live| {
                let name = LEGACY_LOCK;
                symlink(
                    live.join("targets").join(name),
                    fixture.join("state").join(name),
                )
                .unwrap();
            }),
            ("store lock leaf", |fixture, live| {
                let name = "desktop.lock";
                symlink(
                    live.join("targets").join(name),
                    fixture.join("state").join(name),
                )
                .unwrap();
            }),
            ("journal leaf", |fixture, live| {
                let name = "desktop-journal.json";
                symlink(
                    live.join("targets").join(name),
                    fixture.join("state").join(name),
                )
                .unwrap();
            }),
            ("config", |fixture, live| {
                fs::remove_dir_all(fixture.join("config")).unwrap();
                symlink(live.join("config"), fixture.join("config")).unwrap();
            }),
            ("namespace", |fixture, live| {
                let dark = "config/cosmic/com.system76.CosmicTheme.Dark";
                fs::remove_dir_all(fixture.join(dark)).unwrap();
                symlink(live.join(dark), fixture.join(dark)).unwrap();
            }),
            ("key leaf", |fixture, live| {
                let accent = "config/cosmic/com.system76.CosmicTheme.Light/v2/accent";
                fs::remove_file(fixture.join(accent)).unwrap();
                symlink(live.join(accent), fixture.join(accent)).unwrap();
            }),
        ];
        let mut failures = Vec::new();
        for (name, link) in cases {
            let root = temp.0.join(name.replace(' ', "-"));
            fixture(&root.join("config"));
            fs::create_dir_all(root.join("state")).unwrap();
            link(&root, &live);
            let (live_before, fixture_before) = (tree(&live), tree(&root));
            let store = DesktopStore::fixture(&root).unwrap();
            let result = apply_with(&store, "ocean");
            match &result {
                Err(error)
                    if error.contains("outside the fixture root")
                        && !error.contains("Original backup") => {}
                _ => failures.push(format!("{name}: {result:?}")),
            }
            if tree(&live) != live_before {
                failures.push(format!("{name}: fake live tree changed"));
            }
            if tree(&root) != fixture_before {
                failures.push(format!("{name}: fixture changed"));
            }
        }

        // A read-only store (what DESKTOP_CARDS_CONFIG previews get).
        let preview = temp.0.join("preview");
        fixture(&preview.join("config"));
        let before = tree(&preview);
        let store = store(&preview.join("config"), &preview.join("data")).read_only("preview");
        let result = apply_with(&store, "ocean");
        if result != Err("preview".into()) {
            failures.push(format!("read-only: {result:?}"));
        }
        if tree(&preview) != before {
            failures.push("read-only: files created".into());
        }
        assert!(failures.is_empty(), "{failures:#?}");

        // A fresh fixture without a state folder still works, inside itself.
        let fresh = temp.0.join("fresh");
        fixture(&fresh.join("config"));
        let live_before = tree(&live);
        apply_with(&DesktopStore::fixture(&fresh).unwrap(), "ocean").unwrap();
        let backup: Vec<Original> =
            serde_json::from_slice(&fs::read(fresh.join("state").join(BACKUP)).unwrap()).unwrap();
        assert!(!backup.is_empty());
        assert!(fresh.join("state").join(LEGACY_LOCK).exists());
        assert_ne!(
            fs::read(fresh.join("config/cosmic").join(IDS[0]).join("v2/accent")).unwrap(),
            fs::read(live.join("config/cosmic").join(IDS[0]).join("v2/accent")).unwrap()
        );
        assert_eq!(tree(&live), live_before);
    }

    mod links {
        use super::*;
        use crate::desktop::store::tests::Fixture;
        use crate::desktop::wallpaper::{
            BgColor, BgEntry, FilterMethod, NAMESPACE, SamplingMethod, ScalingMode,
        };

        const MEMBERS: &str = "filter_by_theme: true, rotation_frequency: 1234, filter_method: Nearest, scaling_mode: Fit((0.1, 0.2, 0.3)), sampling_method: Random";

        fn shared(source: &str) -> String {
            format!("(output: \"all\", source: {source}, {MEMBERS})")
        }
        /// A fixture with the theme files, a shared background and the
        /// bundled pictures folder (empty).
        fn seeded(name: &str, source: &str) -> (Fixture, PathBuf) {
            let fixture = Fixture::new(name);
            super::fixture(&fixture.0.join("config"));
            fixture.user(NAMESPACE, "all", &shared(source));
            let bundled = fixture.0.join("pictures/Desktop Cards");
            fs::create_dir_all(&bundled).unwrap();
            (fixture, bundled)
        }
        fn current(fixture: &Fixture) -> BgEntry {
            RawValue::parse(fixture.read(NAMESPACE, "all").unwrap())
                .unwrap()
                .decode()
                .unwrap()
        }
        fn expected(source: BgSource) -> BgEntry {
            BgEntry {
                output: "all".into(),
                source,
                filter_by_theme: true,
                rotation_frequency: 1234,
                transition_duration: None,
                filter_method: FilterMethod::Nearest,
                scaling_mode: ScalingMode::Fit([0.1, 0.2, 0.3]),
                sampling_method: SamplingMethod::Random,
            }
        }
        const BOTH: Links = Links {
            wallpaper: true,
            colors: true,
        };
        const WALLPAPER: Links = Links {
            wallpaper: true,
            colors: false,
        };

        /// Background and accents are one change: one journal holds both
        /// parts (the shared entry after the theme keys), and recovery
        /// undoes both.
        #[test]
        fn palette_links_set_shared_source_and_accents_in_one_change() {
            let (fixture, bundled) = seeded("links-both", "Path(\"/old.png\")");
            fs::write(bundled.join("teal.jpg"), b"png").unwrap();
            let store = fixture.store();
            let config = || crate::desktop::store::tests::tree(&fixture.0.join("config"));
            let before = config();
            store.inject(Some(Fault::InterruptBeforeWidget));
            assert!(apply_links_with(&store, "teal", BOTH, &bundled).is_err());
            store.inject(None);
            let journal = store.pending_recovery().unwrap().unwrap();
            let keys: Vec<String> = journal
                .entries
                .iter()
                .map(|entry| format!("{}/{}", entry.namespace, entry.key))
                .collect();
            assert!(keys.iter().any(|key| key == &format!("{}/accent", IDS[0])));
            assert!(keys.iter().any(|key| key == &format!("{}/accent", IDS[2])));
            assert_eq!(keys.last().unwrap(), &format!("{NAMESPACE}/all"));
            store.recover().unwrap();
            assert_eq!(config(), before);

            apply_links_with(&store, "teal", BOTH, &bundled).unwrap();
            assert_eq!(
                current(&fixture),
                expected(BgSource::Path(bundled.join("teal.jpg")))
            );
            let accent =
                |root: &Path| fs::read(root.join("config/cosmic").join(IDS[0]).join("v2/accent"));
            assert_ne!(
                accent(&fixture.0).ok(),
                before
                    .get(
                        &fixture
                            .0
                            .join("config/cosmic")
                            .join(IDS[0])
                            .join("v2/accent")
                    )
                    .cloned()
                    .flatten()
            );
            assert!(fixture.0.join("state").join(BACKUP).exists());
            assert!(store.pending_recovery().unwrap().is_none());
        }

        /// A colour background (refused by the old text replacement) becomes
        /// the bundled picture; no colour backup or theme change.
        #[test]
        fn palette_links_switch_a_colour_background_to_the_bundled_image() {
            let (fixture, bundled) = seeded("links-colour", "Color(Single((0.2, 0.3, 0.4)))");
            fs::write(bundled.join("rose.jpg"), b"png").unwrap();
            let theme =
                crate::desktop::store::tests::tree(&fixture.0.join("config/cosmic").join(IDS[0]));
            apply_links_with(&fixture.store(), "rose", WALLPAPER, &bundled).unwrap();
            assert_eq!(
                current(&fixture),
                expected(BgSource::Path(bundled.join("rose.jpg")))
            );
            assert_eq!(
                crate::desktop::store::tests::tree(&fixture.0.join("config/cosmic").join(IDS[0])),
                theme
            );
            assert!(!fixture.0.join("state").join(BACKUP).exists());
            // A colors-only change does not depend on the background.
            fixture.user(NAMESPACE, "all", "(output: \"all\", future: 1)");
            apply_links_with(
                &fixture.store(),
                "rose",
                Links {
                    wallpaper: false,
                    colors: true,
                },
                &bundled.join("missing"),
            )
            .unwrap();
        }

        /// A missing bundled picture or a shared background set elsewhere
        /// refuses before any lock, backup or setting is written.
        #[test]
        fn palette_links_refuse_missing_bundled_file_before_any_write() {
            let (fixture, bundled) = seeded("links-missing", "Path(\"/old.png\")");
            let files = fixture.tree();
            let error = apply_links_with(&fixture.store(), "ocean", BOTH, &bundled).unwrap_err();
            assert!(error.contains("missing"), "{error}");
            assert_eq!(fixture.tree(), files);
            fs::write(bundled.join("ocean.jpg"), b"png").unwrap();
            fixture.user(
                NAMESPACE,
                "all",
                &shared("Color(Gradient((colors: [(0.1, 0.2, 0.3)], radius: 0.0)))"),
            );
            let files = fixture.tree();
            let error = apply_links_with(&fixture.store(), "ocean", BOTH, &bundled).unwrap_err();
            assert!(error.contains("set elsewhere"), "{error}");
            assert_eq!(fixture.tree(), files);
        }

        #[test]
        fn restore_background_refuses_invalid_backup() {
            let (fixture, _) = seeded("links-restore", "Path(\"/current.png\")");
            let store = fixture.store();
            let saved = fixture.0.join("original-background.ron");
            for invalid in [
                "not ron (".to_string(),
                format!("(output: \"all\", source: Path(\"/a.png\"), {MEMBERS}, future: 1)"),
                shared("Color(Single((NaN, 0.0, 0.0)))"),
                shared("Path(\"/a.png\")").replace("\"all\"", "\"DP-1\""),
                format!("{}{}", shared("Path(\"/a.png\")"), " ".repeat(300 * 1024)),
            ] {
                fs::write(&saved, &invalid).unwrap();
                let files = fixture.tree();
                assert!(
                    restore_background_with(&store, &saved).is_err(),
                    "{invalid:.80}"
                );
                assert_eq!(fixture.tree(), files, "{invalid:.80}");
            }
            let valid = shared("Color(Single((0.2, 0.3, 0.4)))");
            fs::write(&saved, &valid).unwrap();
            restore_background_with(&store, &saved).unwrap();
            assert_eq!(
                fixture.read(NAMESPACE, "all").as_deref(),
                Some(valid.as_str())
            );
            assert_eq!(
                current(&fixture).source,
                BgSource::Color(BgColor::Single([0.2, 0.3, 0.4]))
            );
        }
    }

    #[test]
    fn preview_guard_refuses_before_creating_configuration_or_backup_files() {
        if std::env::var_os("DESKTOP_CARDS_TEST_THEME_PREVIEW").is_some() {
            assert!(apply("ocean").unwrap_err().contains("preview mode"));
            return;
        }
        let temp = Staging::new();
        let result = std::process::Command::new(std::env::current_exe().unwrap())
            .args(["--exact", "system_theme::tests::preview_guard_refuses_before_creating_configuration_or_backup_files"])
            .env("DESKTOP_CARDS_TEST_THEME_PREVIEW", "1")
            .env("DESKTOP_CARDS_CONFIG", temp.0.join("preview.json"))
            .env("XDG_CONFIG_HOME", temp.0.join("config"))
            .env("XDG_DATA_HOME", temp.0.join("data"))
            .status().unwrap();
        assert!(result.success());
        assert!(!temp.0.join("config").exists());
        assert!(!temp.0.join("data").exists());
    }
}

#[cfg(test)]
mod controller_tests {
    use super::*;
    use crate::desktop::store::tests::Fixture;
    fn fixture(name: &str) -> (Fixture, DesktopStore, PathBuf) {
        let fixture = Fixture::new(name);
        let store = fixture
            .store()
            .with_app_config(fixture.0.join("config.json"));
        let bundled = fixture.0.join("pictures");
        fs::create_dir_all(&bundled).unwrap();
        fs::copy(
            Path::new(env!("CARGO_MANIFEST_DIR")).join("assets/wallpapers/rose.jpg"),
            bundled.join("rose.jpg"),
        )
        .unwrap();
        (fixture, store, bundled)
    }
    #[test]
    fn auditfix_cli_palette_changes_authoritative_theme_colors_and_undo_restores_them() {
        use crate::config::{ColorPolicy, Config};
        let (_fixture, store, bundled) = fixture("auditfix-cli-family");
        let original = serde_json::json!({
            "theme":"teal",
            "appearance":{"family":"paper","color_policy":"theme","match_wallpaper":false,"opacity":83}
        });
        fs::write(
            store.app_config().unwrap(),
            serde_json::to_vec(&original).unwrap(),
        )
        .unwrap();
        let before: Config = serde_json::from_value(original).unwrap();
        let mut previous = before.palette().accent;
        for name in ["rose", "ocean"] {
            controller_with(&store, ControllerRequest::Palette(name), &bundled).unwrap();
            let (_, current) = wallpaper::automatic::read_config(&store).unwrap();
            assert_eq!(current.appearance.family, before.appearance.family);
            assert_eq!(current.appearance.opacity, 83);
            assert_eq!(current.appearance.color_policy, Some(ColorPolicy::Custom));
            assert_ne!(current.palette().accent, previous);
            previous = current.palette().accent;
        }
        let latest = store.latest_change().unwrap().unwrap();
        store.undo_latest(&latest.id).unwrap();
        let (_, restored) = wallpaper::automatic::read_config(&store).unwrap();
        assert_eq!(restored.theme, "rose");
        assert_ne!(restored.palette().accent, previous);
    }

    #[test]
    fn fresh_cli_palette_captures_absent_app_and_preserves_wallpaper_collection() {
        let (fixture, store, bundled) = fixture("cli-fresh");
        fixture.user(
            "com.system76.CosmicSettings.Wallpaper",
            "custom-images",
            "[\"/fixture/old.png\"]\n",
        );
        controller_with(&store, ControllerRequest::Palette("rose"), &bundled).unwrap();
        let baseline = crate::desktop::recovery::load_baseline(&store)
            .unwrap()
            .unwrap();
        assert_eq!(
            baseline.kind,
            crate::desktop::recovery::BaselineKind::OriginalDesktop
        );
        assert!(!baseline.app_config_present);
        let (raw, _) = wallpaper::automatic::read_config(&store).unwrap();
        assert_eq!(raw["theme"], "rose");
        assert_eq!(
            fixture
                .read("com.system76.CosmicSettings.Wallpaper", "custom-images")
                .unwrap(),
            "[\"/fixture/old.png\"]\n"
        );
        let entry: wallpaper::BgEntry = store
            .read_user(&wallpaper::shared_key())
            .unwrap()
            .unwrap()
            .decode()
            .unwrap();
        assert_eq!(entry.source, BgSource::Path(bundled.join("rose.jpg")));
    }
    #[test]
    fn cli_color_enable_and_palette_only_preserve_unrelated_preferences() {
        let (_fixture, store, bundled) = fixture("cli-colors");
        fs::write(store.app_config().unwrap(), r#"{"theme":"teal","appearance":{"color_source":"desktop","accent":[4,45,67],"match_wallpaper":false,"opacity":83},"future_setting":42}"#).unwrap();
        controller_with(&store, ControllerRequest::CosmicColors(true), &bundled).unwrap();
        controller_with(&store, ControllerRequest::Palette("ocean"), &bundled).unwrap();
        let (raw, config) = wallpaper::automatic::read_config(&store).unwrap();
        assert_eq!(raw["future_setting"], 42);
        assert_eq!(config.theme, "ocean");
        assert_eq!(config.appearance.opacity, 83);
        assert_eq!(
            config.appearance.color_source,
            crate::config::ColorSource::Custom
        );
        assert!(raw.pointer("/appearance/accent").is_none());
        assert!(config.appearance.match_cosmic_colors);
        assert!(store.read_user(&wallpaper::shared_key()).unwrap().is_none());
        let before = fs::read(store.user_path(&appearance::theme::builder_key(
            appearance::ThemeVariant::Dark,
            "accent",
        )))
        .unwrap();
        controller_with(&store, ControllerRequest::CosmicColors(false), &bundled).unwrap();
        assert_eq!(
            fs::read(store.user_path(&appearance::theme::builder_key(
                appearance::ThemeVariant::Dark,
                "accent"
            )))
            .unwrap(),
            before
        );
    }
    #[test]
    fn cli_capture_failure_and_missing_wallpaper_leave_both_participants_unchanged() {
        let (fixture, store, bundled) = fixture("cli-blocked");
        fs::write(store.app_config().unwrap(), "{\"theme\":\"teal\"}\n").unwrap();
        fixture.user(wallpaper::NAMESPACE, "all", "(output:\"all\",source:Color(Single((0.1,0.2,0.3))),filter:Linear,scaling:Zoom,rotation_frequency:300)");
        let original = fs::read(store.app_config().unwrap()).unwrap();
        assert!(controller_with(&store, ControllerRequest::Palette("ocean"), &bundled).is_err());
        let damaged = store.state_dir().join("desktop-recovery/original");
        fs::create_dir_all(&damaged).unwrap();
        fs::write(damaged.join("manifest.json"), "broken").unwrap();
        for request in [
            ControllerRequest::Palette("rose"),
            ControllerRequest::CosmicColors(true),
        ] {
            assert!(controller_with(&store, request, &bundled).is_err());
            assert_eq!(fs::read(store.app_config().unwrap()).unwrap(), original);
        }
        assert_eq!(fs::read(damaged.join("manifest.json")).unwrap(), b"broken");
    }
}
