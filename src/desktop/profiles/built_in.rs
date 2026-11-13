//! Turning a handcrafted look into a built-in one.
//!
//! A built-in look ships inside Halcyon as `resources/looks/<id>.json`, in
//! the shared export format (see `format::export`). It must look the same on
//! every computer and say nothing about the one it was made on, so before a
//! look is written as a built-in:
//!
//! - its wallpaper must be one of Halcyon's bundled wallpapers (or a plain
//!   color or gradient): a personal picture, folder or a COSMIC system
//!   picture is refused, never copied;
//! - anything local or personal is removed: the display arrangement and
//!   per-display backgrounds, display-specific island positions, where it
//!   was mixed from and who shared it;
//! - fonts and the icon theme are checked against what a default Pop!_OS /
//!   COSMIC install ships. Others are kept, and the result says they fall
//!   back to the current ones where they are missing.
//!
//! Settings writes the file where the person chose, or by default to
//! `$XDG_DATA_HOME/halcyon/built-in-looks/<id>.json`. Nothing else is
//! written, and the desktop is never touched.
use super::format::{Profile, export};
use super::{SourceLook, WallpaperLook};
use std::path::PathBuf;

/// Interface and monospace fonts every default COSMIC install has.
pub const DEFAULT_FONTS: [&str; 4] = ["Open Sans", "Fira Sans", "Fira Mono", "Noto Sans Mono"];
/// Icon themes every default COSMIC install has.
pub const DEFAULT_ICON_THEMES: [&str; 2] = ["Cosmic", "Pop"];
/// The folder under `$XDG_DATA_HOME/halcyon` built-in looks go to by default.
pub const FOLDER: &str = "built-in-looks";

/// Why a look can't become built-in as it is.
#[derive(Clone, Debug, PartialEq)]
pub enum Refusal {
    /// The wallpaper is the person's own picture or folder (or a COSMIC
    /// system picture another install may not have).
    PersonalWallpaper,
    /// The look has no wallpaper of its own.
    NoWallpaper,
    /// The record itself isn't valid.
    Invalid(String),
}

impl Refusal {
    pub fn message(&self) -> String {
        match self {
            Self::PersonalWallpaper => "A built-in look can only use one of Halcyon’s wallpapers. Your own pictures stay on this computer. Pick a Halcyon wallpaper for this look first.".into(),
            Self::NoWallpaper => "A built-in look needs one of Halcyon’s wallpapers. Pick one for this look first.".into(),
            Self::Invalid(message) => message.clone(),
        }
    }
}

/// A look ready to ship: its ID, the file's bytes, what was removed and
/// what falls back on a computer that doesn't have it.
#[derive(Clone, Debug, PartialEq)]
pub struct BuiltIn {
    pub id: String,
    pub bytes: Vec<u8>,
    /// Personal or local members left out, as the notice names them.
    pub removed: Vec<&'static str>,
    /// Fonts and icon themes a default install may not have.
    pub fallbacks: Vec<String>,
}

/// A stable built-in ID from a look's name: lowercase letters, digits and
/// single hyphens, at most 48 characters.
pub fn id_for(name: &str) -> String {
    let mut id = String::new();
    for c in name.chars().flat_map(char::to_lowercase) {
        if c.is_ascii_lowercase() || c.is_ascii_digit() {
            id.push(c);
        } else if !id.is_empty() && !id.ends_with('-') {
            id.push('-');
        }
        if id.len() >= 48 {
            break;
        }
    }
    let id = id.trim_end_matches('-').to_string();
    if id.is_empty() { "my-look".into() } else { id }
}

/// Whether `id` can name a file in `resources/looks`.
pub fn valid_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 48
        && !id.starts_with('-')
        && !id.ends_with('-')
        && !id.contains("--")
        && id
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
}

/// Whether the look's wallpaper can ship: a bundled Halcyon wallpaper, a
/// color or a gradient.
pub fn wallpaper_ships(look: Option<&WallpaperLook>) -> Result<(), Refusal> {
    let source = look
        .and_then(|look| look.background.as_ref())
        .and_then(|background| background.source.as_ref())
        .ok_or(Refusal::NoWallpaper)?;
    match source {
        SourceLook::Bundled(name) if crate::desktop::wallpaper::bundled_name(name).is_some() => {
            Ok(())
        }
        SourceLook::Color(_) | SourceLook::Gradient(_) => Ok(()),
        _ => Err(Refusal::PersonalWallpaper),
    }
}

/// The first member a built-in look may not carry, if any: anything a
/// shared look can't carry, plus where it was mixed from or shared by.
pub fn personal_member(profile: &Profile) -> Option<&'static str> {
    if let Some(member) = profile.local_member() {
        return Some(member);
    }
    if profile.composition.is_some() {
        return Some("where it was mixed from");
    }
    if profile.package.is_some() {
        return Some("who shared it");
    }
    profile
        .wallpaper
        .as_ref()
        .is_some_and(|look| look.displays.is_some())
        .then_some("the display arrangement")
}

/// `profile` without anything personal or local, and what was removed.
pub fn strip_personal(profile: &Profile) -> (Profile, Vec<&'static str>) {
    let (mut profile, mut removed) = profile.portable();
    if profile.composition.take().is_some() {
        removed.push("where it was mixed from");
    }
    if profile.package.take().is_some() {
        removed.push("who shared it");
    }
    if let Some(look) = profile.wallpaper.as_mut()
        && look.displays.take().is_some()
        && !removed.contains(&super::DISPLAY_ARRANGEMENT)
    {
        removed.push(super::DISPLAY_ARRANGEMENT);
    }
    (profile, removed)
}

/// Fonts and icon themes `profile` names that a default install may not
/// have, each once, in the order they appear.
pub fn fallbacks(profile: &Profile) -> Vec<String> {
    let mut names = Vec::new();
    let mut push = |text: String| {
        if !names.contains(&text) {
            names.push(text);
        }
    };
    if let Some(appearance) = &profile.appearance {
        for font in [&appearance.interface_font, &appearance.monospace_font]
            .into_iter()
            .flatten()
        {
            if !DEFAULT_FONTS.contains(&font.family.as_str()) {
                push(format!("the font {}", font.family));
            }
        }
        if let Some(theme) = &appearance.icon_theme
            && !DEFAULT_ICON_THEMES.contains(&theme.as_str())
        {
            push(format!("the icon theme {theme}"));
        }
    }
    if let Some(style) = &profile.widget_style {
        for pointer in [
            "/widget_font",
            "/clock_font",
            "/glance/font",
            "/glance/clock_font",
        ] {
            if let Some(family) = style.pointer(pointer).and_then(font_family)
                && !DEFAULT_FONTS.contains(&family.as_str())
            {
                push(format!("the font {family}"));
            }
        }
    }
    names
}

/// A widget font setting's family: a name, or an object with `family`.
fn font_family(value: &serde_json::Value) -> Option<String> {
    value
        .as_str()
        .or_else(|| value.get("family").and_then(serde_json::Value::as_str))
        .filter(|family| !family.is_empty())
        .map(str::to_string)
}

/// `profile` as built-in look `id`: refused for a personal wallpaper,
/// otherwise stripped of anything personal and encoded in the shared
/// export format.
pub fn prepare(profile: &Profile, id: &str) -> Result<BuiltIn, Refusal> {
    if !valid_id(id) {
        return Err(Refusal::Invalid(
            "A built-in look’s ID uses lowercase letters, digits and hyphens.".into(),
        ));
    }
    wallpaper_ships(profile.wallpaper.as_ref())?;
    let (stripped, removed) = strip_personal(profile);
    let exported = export(&stripped).map_err(|error| Refusal::Invalid(error.to_string()))?;
    // The file must load exactly as the app loads its resources.
    super::presets::parse_built_in(id, std::str::from_utf8(&exported.bytes).unwrap_or(""))
        .map_err(Refusal::Invalid)?;
    Ok(BuiltIn {
        id: id.to_string(),
        bytes: exported.bytes,
        removed,
        fallbacks: fallbacks(&stripped),
    })
}

/// `$XDG_DATA_HOME/halcyon/built-in-looks` (or `~/.local/share/...`).
pub fn default_folder() -> Option<PathBuf> {
    let data = std::env::var_os("XDG_DATA_HOME")
        .map(PathBuf::from)
        .filter(|path| path.is_absolute())
        .or_else(|| {
            std::env::var_os("HOME")
                .map(PathBuf::from)
                .filter(|path| path.is_absolute())
                .map(|home| home.join(".local/share"))
        })?;
    Some(data.join("halcyon").join(FOLDER))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::desktop::profiles::mix::{Origin, Provenance, SourceKind};
    use crate::desktop::profiles::{DisplaysName, built_in_looks, parse_built_in};

    fn harbor() -> Profile {
        built_in_looks()
            .iter()
            .find(|look| look.id == "harbor")
            .expect("resources/looks/harbor.json loads")
            .profile
            .clone()
    }

    fn origin(kind: SourceKind, id: Option<&str>) -> Origin {
        Origin {
            kind,
            name: "Somewhere".into(),
            id: id.map(str::to_string),
            fingerprint: "a".repeat(64),
        }
    }

    #[test]
    fn every_resource_look_loads_with_a_description_and_bundled_wallpaper() {
        assert_eq!(
            super::super::presets::resources::FILES.len(),
            built_in_looks().len(),
            "a file in resources/looks does not load"
        );
        for look in built_in_looks() {
            assert!(valid_id(look.id), "{}", look.id);
            assert!(!look.description.is_empty(), "{}", look.id);
            assert!(wallpaper_ships(look.profile.wallpaper.as_ref()).is_ok());
            assert_eq!(personal_member(&look.profile), None, "{}", look.id);
            assert!(fallbacks(&look.profile).is_empty(), "{}", look.id);
            assert!(crate::desktop::profiles::preset(look.id).is_some());
        }
        let harbor = harbor();
        assert_eq!(harbor.name, "Harbor");
        assert!(harbor.version <= crate::desktop::profiles::VERSION);
    }

    #[test]
    fn personal_wallpapers_are_refused_and_never_written() {
        for source in [
            SourceLook::Image("/home/someone/Pictures/cat.jpg".into()),
            SourceLook::Folder("/home/someone/Pictures".into()),
            SourceLook::System("pop/kate-hazen-unleash-your-robot.png".into()),
            SourceLook::Bundled("not-a-halcyon-wallpaper".into()),
        ] {
            let mut probe = harbor();
            probe
                .wallpaper
                .as_mut()
                .unwrap()
                .background
                .as_mut()
                .unwrap()
                .source = Some(source.clone());
            assert_eq!(
                prepare(&probe, "harbor-two"),
                Err(Refusal::PersonalWallpaper),
                "{source:?}"
            );
        }
        let mut bare = harbor();
        bare.wallpaper = None;
        assert_eq!(prepare(&bare, "bare"), Err(Refusal::NoWallpaper));
        assert!(Refusal::PersonalWallpaper.message().contains("Halcyon"));
        assert!(matches!(
            prepare(&harbor(), "Not An ID"),
            Err(Refusal::Invalid(_))
        ));
    }

    #[test]
    fn personal_data_is_stripped_and_the_file_round_trips_through_the_loader() {
        let mut look = harbor();
        look.composition = Some(Provenance {
            picture: origin(SourceKind::Library, Some("p20260101-1-0")),
            style: origin(SourceKind::Desktop, None),
            layout: origin(SourceKind::Builtin, None),
        });
        look.wallpaper.as_mut().unwrap().displays = Some(DisplaysName::Same);
        assert!(personal_member(&look).is_some());
        assert!(
            parse_built_in(
                "probe",
                &String::from_utf8(crate::desktop::profiles::export_local(&look).unwrap()).unwrap()
            )
            .is_err()
        );
        let built = prepare(&look, "harbor-copy").unwrap();
        assert!(built.removed.contains(&"where it was mixed from"));
        assert!(built.removed.contains(&"local source IDs"));
        assert!(built.removed.contains(&super::super::DISPLAY_ARRANGEMENT));
        let text = std::str::from_utf8(&built.bytes).unwrap();
        for personal in ["p20260101", "Somewhere", "displays", "composition"] {
            assert!(!text.contains(personal), "{personal} in {text}");
        }
        let loaded = parse_built_in("harbor-copy", text).unwrap();
        let mut expected = harbor();
        expected.composition = None;
        assert_eq!(loaded.wallpaper, expected.wallpaper);
        assert_eq!(loaded.appearance, expected.appearance);
        assert_eq!(loaded.panel, expected.panel);
        assert_eq!(loaded.widget_style, expected.widget_style);
        assert_eq!(loaded.description, expected.description);
        // Code look IDs stay with the code looks.
        assert!(parse_built_in("tidal", text).is_err());
        assert!(parse_built_in("adaptive-x", text).is_err());
    }

    #[test]
    fn fonts_and_icons_outside_a_default_install_are_named_as_fallbacks() {
        let mut look = harbor();
        let appearance = look.appearance.as_mut().unwrap();
        appearance.interface_font.as_mut().unwrap().family = "Comic Neue".into();
        appearance.icon_theme = Some("Papirus".into());
        look.widget_style.as_mut().unwrap()["clock_font"] = "Fira Sans".into();
        let built = prepare(&look, "harbor-fonts").unwrap();
        assert_eq!(
            built.fallbacks,
            vec![
                "the font Comic Neue".to_string(),
                "the icon theme Papirus".into()
            ]
        );
        assert!(fallbacks(&harbor()).is_empty());
    }

    #[test]
    fn ids_come_from_names() {
        assert_eq!(id_for("Harbor at Dusk!"), "harbor-at-dusk");
        assert_eq!(id_for("  ---  "), "my-look");
        assert_eq!(id_for("Café 2"), "caf-2");
        assert!(id_for(&"long name ".repeat(20)).len() <= 48);
        assert!(valid_id(&id_for(&"long name ".repeat(20))));
        for bad in ["", "-a", "a-", "a--b", "A", "a_b", "a.json"] {
            assert!(!valid_id(bad), "{bad}");
        }
    }
}
