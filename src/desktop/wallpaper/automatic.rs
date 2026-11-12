//! Wallpaper-owned colors shared by Settings and the existing Cards runtime.
//! Source selection is deterministic; decoding and theme planning run on the
//! blocking pool. The transaction pins both wallpaper and color baselines.
use super::{BgColor, BgSource, Target, WallpaperDraft, media, palette};
use crate::config::{self, Config};
use crate::desktop::appearance::AppearanceDraft;
use crate::desktop::profiles::Domain;
use crate::desktop::store::{DesktopPatch, DesktopStore, LatestChange, WidgetPatch, widget};
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};

#[cfg(test)]
const ACK: &str = "wallpaper-colors-seen.json";

#[derive(Clone, Debug, PartialEq)]
enum Input {
    Color(BgColor),
    Picture(PathBuf),
    Unavailable(String),
}

#[derive(Clone, Debug, PartialEq)]
pub struct Source {
    pub stamp: u64,
    epoch: Option<u64>,
    pub label: String,
    /// The effective entry, also used by shared-theme previews.
    pub background: Option<super::BgEntry>,
    input: Input,
}

fn built_in(name: &str) -> bool {
    ["eDP", "LVDS", "DSI"]
        .iter()
        .any(|prefix| name.starts_with(prefix))
}

/// The built-in connected display, then the first connected connector by
/// name. With no connected outputs, use Shared. The selected editor tab
/// or display never changes the source of the shared accent.
pub fn source(store: &DesktopStore, draft: &WallpaperDraft) -> Source {
    let mut displays: Vec<_> = draft
        .display_views()
        .into_iter()
        .filter(|v| v.connected)
        .collect();
    displays.sort_by_key(|view| (!built_in(&view.name), view.name.clone()));
    let output = displays.first().map(|view| view.name.clone());
    let target = output
        .as_ref()
        .map_or(Target::Shared, |name| Target::Output(name.clone()));
    let label = output.as_ref().map_or_else(
        || "Shared background".into(),
        |name| crate::displays::display_title(name, None),
    );
    let background = draft.effective(&target);
    let input = match background.known().map(|entry| &entry.source) {
        Some(BgSource::Color(color)) => Input::Color(color.clone()),
        Some(BgSource::Path(path)) if path.is_dir() => output
            .as_deref()
            .and_then(|name| media::current_slideshow_image(store, name))
            .filter(|picture| picture.starts_with(path) && picture.is_file())
            .map(Input::Picture)
            .unwrap_or_else(|| {
                Input::Unavailable(
                    "Colors update when the slideshow shows its first picture".into(),
                )
            }),
        Some(BgSource::Path(path)) => Input::Picture(path.clone()),
        None => Input::Unavailable("This background format cannot provide colors".into()),
    };
    let version = match &input {
        Input::Picture(path) => std::fs::metadata(path)
            .ok()
            .map(|meta| (meta.len(), meta.modified().ok())),
        _ => None,
    };
    let fingerprint = format!(
        "{target:?}:{:?}:{input:?}:{version:?}",
        background.known().map(|entry| &entry.source)
    );
    let bytes = Sha256::digest(fingerprint.as_bytes());
    let stamp = u64::from_be_bytes(bytes[..8].try_into().expect("eight digest bytes")).max(1);
    Source {
        epoch: store.restore_epoch().ok(),
        stamp,
        label,
        background: background.known().cloned(),
        input,
    }
}

impl Source {
    /// Preview-owned analysis shares the bounded, cancellable image workers.
    /// The runtime's existing background owner retains the synchronous entry.
    pub async fn analyse_preview(
        &self,
        cancel: tokio::sync::watch::Receiver<bool>,
    ) -> Result<palette::Suggestion, String> {
        if *cancel.borrow() || cancel.has_changed().is_err() {
            return Err("Wallpaper preview canceled".into());
        }
        let Input::Picture(path) = &self.input else {
            return self.analyse();
        };
        let pixels = media::worker::thumbnail(path.clone(), cancel).await.ok_or(
            "Cannot read colors from this picture. Use PNG or JPEG; your last colors are kept",
        )?;
        let suggestion = palette::from_pixels(&pixels);
        recommended(&suggestion)
            .ok_or("This picture has no usable colors. Keeping your last colors")?;
        Ok(suggestion)
    }
    /// Never call on the UI thread: bounded PNG/JPEG decode and median cut.
    pub fn analyse(&self) -> Result<palette::Suggestion, String> {
        let suggestion = match &self.input {
            Input::Color(color) => palette::from_color(color),
            Input::Picture(path) if !path.is_file() => return Err("The wallpaper picture is missing. Keeping your last colors".into()),
            Input::Picture(path) => match media::thumbnail(path) {
                media::Preview::Ready(pixels) => palette::from_pixels(&pixels),
                media::Preview::None(_) => return Err("Cannot read colors from this picture. Use PNG or JPEG; your last colors are kept".into()),
            },
            Input::Unavailable(reason) => return Err(reason.clone()),
        };
        recommended(&suggestion)
            .ok_or("This picture has no usable colors. Keeping your last colors")?;
        Ok(suggestion)
    }
    pub fn picture(&self) -> Option<&Path> {
        match &self.input {
            Input::Picture(path) => Some(path),
            _ => None,
        }
    }
}

pub fn recommended(suggestion: &palette::Suggestion) -> Option<[u8; 3]> {
    suggestion.accent.or_else(|| {
        suggestion
            .swatches
            .first()
            .map(|swatch| palette::visible_accent(swatch.rgb))
    })
}

/// A theme keeps its own color on a grey picture instead of a grey accent.
pub fn recommended_for(
    suggestion: &palette::Suggestion,
    family: Option<crate::desktop_theme::Family>,
) -> Option<[u8; 3]> {
    match family {
        Some(family) if suggestion.accents.is_empty() && !suggestion.swatches.is_empty() => {
            Some(family.accent())
        }
        _ => recommended(suggestion),
    }
}

/// Polarity from the bounded wallpaper analysis, shared by previews and runtime.
pub fn light(suggestion: &palette::Suggestion) -> bool {
    let sum: f32 = suggestion.swatches.iter().map(|s| s.share).sum();
    suggestion
        .swatches
        .iter()
        .map(|s| {
            crate::desktop_theme::luminance(cosmic::iced::Color::from_rgb8(
                s.rgb[0], s.rgb[1], s.rgb[2],
            )) * s.share
        })
        .sum::<f32>()
        / sum.max(0.001)
        > 0.35
}

pub fn read_config(store: &DesktopStore) -> Result<(serde_json::Value, Config), String> {
    let path = store
        .app_config()
        .ok_or("Widget settings are unavailable")?;
    let text = config::read_config_text(path)?;
    let raw = config::parse_config_text(text.as_deref())?;
    let typed = config::check_config_value(&raw)?;
    Ok((raw, typed))
}

/// Only a live, bounded gesture can briefly defer automatic work.
pub fn paused(store: &DesktopStore) -> bool {
    // Current Settings writes only WidgetLayout leases. Retain these bounded
    // legacy domains for an older Settings process still open during upgrade.
    // Recovered forms and chooser selections never participate.
    crate::automation::read_hold(store).iter().any(|domain| {
        matches!(
            domain,
            Domain::Wallpaper | Domain::WidgetStyle | Domain::Appearance
        )
    })
}

pub fn watch_paths(store: &DesktopStore) -> Vec<PathBuf> {
    let mut paths = WallpaperDraft::keys()
        .iter()
        .map(|key| store.user_path(key))
        .collect::<Vec<_>>();
    if let Ok(draft) = WallpaperDraft::load(store) {
        paths.extend(draft.watch_keys().iter().map(|key| store.user_path(key)));
        paths.extend(source(store, &draft).picture().map(Path::to_path_buf));
    }
    paths.extend(media::state_path(store));
    paths.push(store.state_dir().join(crate::automation::HOLD_FILE));
    paths.sort();
    paths.dedup();
    paths
}

#[derive(Clone, Debug)]
pub struct Prepared {
    pub source: Source,
    patch: DesktopPatch,
    widgets: WidgetPatch,
}

/// Read-only work for the existing Cards task executor. Only the currently
/// acknowledged source is skipped, so overrides and Undo survive restart
/// without suppressing a later return to an older wallpaper.
pub fn prepare(store: &DesktopStore) -> Result<Option<Prepared>, String> {
    let epoch = store.restore_epoch().map_err(|e| e.to_string())?;
    let (raw, config) = read_config(store)?;
    if !store.writable()
        || (!config.appearance.wallpaper_colors && config.appearance.family.is_none())
    {
        return Ok(None);
    }
    if paused(store) {
        return Err("Automatic colors will continue when the current gesture finishes".into());
    }
    let draft = WallpaperDraft::load(store).map_err(|error| error.to_string())?;
    let mut source = source(store, &draft);
    source.epoch = Some(epoch);
    if store
        .palette_seen(epoch, source.stamp)
        .map_err(|e| e.to_string())?
    {
        return Ok(None);
    }
    let suggestion = source.analyse()?;
    let rgb = recommended_for(&suggestion, config.appearance.family).expect("analysis checked");
    let mut patch = if config.appearance.sync_colors && config.appearance.wallpaper_colors {
        let mut appearance = AppearanceDraft::load(store).map_err(|error| error.to_string())?;
        crate::desktop::profiles::themes::linked_colors(&mut appearance, config.appearance, rgb);
        crate::desktop::profiles::themes::linked_accents(&mut appearance, config.appearance, rgb);
        let plan = appearance.plan();
        if !plan.blocked.is_empty() {
            return Err("Pop!_OS theme colors need attention. Your last colors are kept".into());
        }
        plan.patch
    } else {
        DesktopPatch::new()
    };
    draft.guard(&mut patch);
    let mut widgets = WidgetPatch::new();
    if config.appearance.wallpaper_colors {
        for (pointer, desired) in [
            ("/appearance/accent", serde_json::json!(rgb)),
            ("/appearance/color_source", serde_json::json!("custom")),
            (
                "/appearance/wallpaper_color_stamp",
                serde_json::json!(source.stamp),
            ),
        ] {
            widgets.set(pointer, widget::get(&raw, pointer), Some(desired))?;
        }
    }
    if config.appearance.family.is_some() {
        let original = widget::get(&raw, "/appearance/wallpaper_light");
        let polarity = light(&suggestion);
        widgets.set(
            "/appearance/wallpaper_light",
            original.clone(),
            if polarity == config.appearance.wallpaper_light {
                original
            } else {
                Some(serde_json::json!(polarity))
            },
        )?;
    }
    // A disable, scope change, palette, or profile while the decode was
    // running invalidates the whole result, including its desktop part.
    for pointer in [
        "/theme",
        "/appearance/family",
        "/appearance/native_mode_palettes",
        "/appearance/surface",
        "/appearance/wallpaper_colors",
        "/appearance/color_policy",
        "/appearance/overrides",
        "/appearance/sync_colors",
    ] {
        let original = widget::get(&raw, pointer);
        widgets.set(pointer, original.clone(), original)?;
    }
    Ok(Some(Prepared {
        source,
        patch,
        widgets,
    }))
}

/// The UI has accepted the versioned result. Recheck picture metadata and
/// live gesture holds immediately before the store rechecks all settings under
/// its existing writer locks. A failed transaction never acknowledges it.
pub fn commit(store: &DesktopStore, prepared: Prepared) -> Result<Option<LatestChange>, String> {
    if paused(store) {
        return Err("Automatic colors will continue when the current gesture finishes".into());
    }
    let current = WallpaperDraft::load(store).map_err(|error| error.to_string())?;
    if source(store, &current).stamp != prepared.source.stamp {
        return Err("The wallpaper changed while colors were being found".into());
    }
    store
        .apply_palette(
            &prepared.patch,
            &prepared.widgets,
            prepared
                .source
                .epoch
                .ok_or("Cannot read restore ownership")?,
            prepared.source.stamp,
        )
        .map_err(|e| e.to_string())
}

fn acknowledge(store: &DesktopStore, analyzed: &Source) -> Result<(), String> {
    let draft = WallpaperDraft::load(store).map_err(|e| e.to_string())?;
    if source(store, &draft).stamp != analyzed.stamp {
        return Err("Wallpaper changed before acknowledgement".into());
    }
    let mut patch = DesktopPatch::new();
    draft.guard(&mut patch);
    store
        .apply_palette(
            &patch,
            &WidgetPatch::new(),
            analyzed.epoch.ok_or("Cannot read restore ownership")?,
            analyzed.stamp,
        )
        .map(|_| ())
        .map_err(|e| e.to_string())
}

/// A theme may have planned a different wallpaper. Never acknowledge a
/// retained display assignment or slideshow frame that was not analysed.
pub fn acknowledge_analyzed(store: &DesktopStore, analyzed: &Source) -> Result<(), String> {
    let draft = WallpaperDraft::load(store).map_err(|error| error.to_string())?;
    if source(store, &draft).stamp == analyzed.stamp {
        acknowledge(store, analyzed)?;
    }
    Ok(())
}

/// Successful Settings Apply or Undo defines the current wallpaper's colors,
/// including an advanced draft held across a wallpaper change. Previews and
/// failed writes never call this; the next wallpaper resumes the link.
pub fn keep_manual_colors(store: &DesktopStore) -> Result<(), String> {
    if store.app_config().is_none() {
        return Ok(());
    }
    let config = read_config(store)?.1;
    if config.appearance.wallpaper_colors || config.appearance.family.is_some() {
        let draft = WallpaperDraft::load(store).map_err(|error| error.to_string())?;
        acknowledge(store, &source(store, &draft))?;
    }
    Ok(())
}

#[cfg(test)]
#[path = "automatic_tests.rs"]
mod tests;
