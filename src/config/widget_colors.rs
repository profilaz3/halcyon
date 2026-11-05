//! App-owned colors with readable widget ink. Legacy desktop-following
//! configurations keep a read-only snapshot until an intentional color Apply.
use super::{ColorSource, Config, Palette};
use crate::desktop::{ConfigKey, DesktopStore};
use cosmic::cosmic_theme::{Component, Container, ThemeBuilder};
use cosmic::iced::{Color, Subscription};

fn luminance(color: Color) -> f32 {
    let linear = |v: f32| {
        if v <= 0.04045 {
            v / 12.92
        } else {
            ((v + 0.055) / 1.055).powf(2.4)
        }
    };
    0.2126 * linear(color.r) + 0.7152 * linear(color.g) + 0.0722 * linear(color.b)
}

/// A desktop accent is also a button fill; it is not necessarily suitable for
/// small text. Keep its hue, moving only as far toward light/dark as needed.
fn readable(color: Color, background: Color) -> Color {
    let background = luminance(background);
    let contrast = |color| {
        let ink = luminance(color);
        (ink.max(background) + 0.05) / (ink.min(background) + 0.05)
    };
    if contrast(color) >= 4.5 {
        return color;
    }
    let target = if contrast(Color::WHITE) > contrast(Color::BLACK) {
        1.
    } else {
        0.
    };
    let blend = |amount| {
        Color::from_rgb(
            color.r + (target - color.r) * amount,
            color.g + (target - color.g) * amount,
            color.b + (target - color.b) * amount,
        )
    };
    let (mut low, mut high) = (0., 1.);
    for _ in 0..16 {
        let middle = (low + high) / 2.;
        if contrast(blend(middle)) >= 4.6 {
            high = middle;
        } else {
            low = middle;
        }
    }
    blend(high)
}

impl Palette {
    /// Tint the card from the chosen source color; keep the exact source
    /// in configuration and use readable derived ink for small widget text.
    pub fn from_accent(rgb: [u8; 3]) -> Self {
        let source = Color::from_rgb8(rgb[0], rgb[1], rgb[2]);
        let background = Color::from_rgb(
            0.075 + source.r * 0.10,
            0.075 + source.g * 0.10,
            0.085 + source.b * 0.10,
        );
        Self {
            accent: readable(source, background),
            background,
            foreground: Color::from_rgb8(242, 245, 249),
            // Secondary ink leans slightly toward the source, so it reads
            // warm on a warm card and cool on a cool one.
            muted: readable(
                Color::from_rgb(
                    0.64 + source.r * 0.12,
                    0.64 + source.g * 0.12,
                    0.66 + source.b * 0.12,
                ),
                background,
            ),
        }
    }

    /// Transparent text sits over the wallpaper, outside the desktop theme's
    /// surfaces. Use light ink and a pastel accent even with a light/dark theme
    /// whose button-fill accent would vanish on a vivid wallpaper.
    pub fn on_wallpaper(self) -> Self {
        let reference = Color::from_rgb8(13, 113, 127);
        Self {
            foreground: Color::from_rgb8(245, 248, 252),
            muted: Color::from_rgb8(221, 232, 239),
            accent: readable(self.accent, reference),
            ..self
        }
    }
}

fn keys() -> Vec<ConfigKey> {
    let mut keys = vec![ConfigKey::new("com.system76.CosmicTheme.Mode", 1, "is_dark").unwrap()];
    for namespace in [
        "com.system76.CosmicTheme.Light",
        "com.system76.CosmicTheme.Dark",
    ] {
        for name in ["background", "accent"] {
            keys.push(ConfigKey::new(namespace, 2, name).unwrap());
        }
    }
    keys
}

fn read_palette(store: &DesktopStore) -> Result<Palette, String> {
    let keys = keys();
    let snapshot = store.load(&keys).map_err(|error| error.to_string())?;
    let dark = snapshot
        .effective(&keys[0])
        .map(|value| value.decode::<bool>().map_err(|error| error.to_string()))
        .transpose()?
        .unwrap_or(true);
    // Normal desktop updates already contain both derived roles. Build
    // fallback colors only for sparse/default themes, not on every refresh.
    let defaults = std::cell::LazyCell::new(|| {
        if dark {
            ThemeBuilder::dark()
        } else {
            ThemeBuilder::light()
        }
        .build()
    });
    let offset = if dark { 3 } else { 1 };
    let background: Container = snapshot
        .effective(&keys[offset])
        .map(|value| value.decode().map_err(|error| error.to_string()))
        .transpose()?
        .unwrap_or_else(|| defaults.background(false).clone());
    let accent: Component = snapshot
        .effective(&keys[offset + 1])
        .map(|value| value.decode().map_err(|error| error.to_string()))
        .transpose()?
        .unwrap_or_else(|| defaults.accent.clone());
    let color = |value: cosmic::cosmic_theme::palette::Srgba| -> Result<Color, String> {
        if [value.red, value.green, value.blue, value.alpha]
            .iter()
            .any(|v| !v.is_finite() || !(0.0..=1.0).contains(v))
        {
            return Err("The desktop theme has an unsupported color".into());
        }
        // Card opacity is the user's own setting; the desktop cannot hide
        // text by contributing transparent foreground roles.
        Ok(Color::from_rgb(value.red, value.green, value.blue))
    };
    let background = (color(background.base)?, color(background.on)?);
    let foreground = readable(background.1, background.0);
    let background = background.0;
    let mix = |fg: f32, bg: f32| fg * 0.72 + bg * 0.28;
    Ok(Palette {
        accent: readable(color(accent.base)?, background),
        background,
        foreground,
        muted: readable(
            Color::from_rgb(
                mix(foreground.r, background.r),
                mix(foreground.g, background.g),
                mix(foreground.b, background.b),
            ),
            background,
        ),
    })
}

impl Config {
    /// Refresh only when following. A temporarily unreadable theme keeps
    /// the last good colors (or the saved custom palette on first load).
    pub fn refresh_desktop_palette(&mut self) -> Result<bool, String> {
        if self.appearance.color_source == ColorSource::Custom {
            return Ok(self.desktop_palette.take().is_some());
        }
        let store = DesktopStore::from_env().map_err(|error| error.to_string())?;
        let next = read_palette(&store)?;
        let changed = self.desktop_palette != Some(next);
        self.desktop_palette = Some(next);
        Ok(changed)
    }

    /// Watch actual desktop roles rather than the app's deliberately dark
    /// Settings theme. The subscription sleeps entirely for custom colors.
    pub fn color_subscription(&self) -> Subscription<()> {
        if self.appearance.color_source != ColorSource::Desktop {
            return Subscription::none();
        }
        let Ok(store) = DesktopStore::from_env() else {
            return Subscription::none();
        };
        crate::watch::read_only_files(keys().iter().map(|key| store.user_path(key)).collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::desktop::store::Access;
    use std::path::PathBuf;

    struct Fixture(PathBuf);
    impl Fixture {
        fn new() -> Self {
            static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
            let root = std::env::temp_dir().join(format!(
                "desktop-cards-follow-colors-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
            ));
            std::fs::create_dir_all(&root).unwrap();
            Self(root)
        }
        fn store(&self) -> DesktopStore {
            DesktopStore::with_roots(
                self.0.join("config/cosmic"),
                Vec::new(),
                self.0.join("state"),
                Access::Fixture(self.0.clone()),
            )
        }
        fn write(&self, key: &ConfigKey, value: &impl serde::Serialize) {
            let file = self.store().user_path(key);
            std::fs::create_dir_all(file.parent().unwrap()).unwrap();
            std::fs::write(file, ron::to_string(value).unwrap()).unwrap();
        }
    }
    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn saved_app_color_is_readable_and_independent_of_desktop_roles() {
        let mut config: Config = serde_json::from_value(serde_json::json!({
            "theme":"teal", "appearance":{"color_source":"custom", "accent":[4, 45, 67], "opacity":83}
        })).unwrap();
        config.desktop_palette = Some(Palette::named("rose"));
        let first = config.palette();
        assert_ne!(first.accent, Palette::named("teal").accent);
        assert_ne!(first.accent, Palette::named("rose").accent);
        let contrast = (luminance(first.accent).max(luminance(first.background)) + 0.05)
            / (luminance(first.accent).min(luminance(first.background)) + 0.05);
        assert!(contrast >= 4.5);
        config.desktop_palette = Some(Palette::named("amber"));
        assert_eq!(
            config.palette(),
            first,
            "outside theme edits cannot become our colors"
        );
        assert_eq!(
            serde_json::to_value(config).unwrap()["appearance"]["accent"],
            serde_json::json!([4, 45, 67])
        );
    }

    #[test]
    fn following_reads_the_active_variants_actual_colors_without_creating_state() {
        let fixture = Fixture::new();
        let keys = keys();
        // Already-readable role colors are followed exactly; difficult
        // dark/pale accents are covered by the contrast regression below.
        let light = ThemeBuilder::light()
            .accent(cosmic::cosmic_theme::palette::Srgb::new(0.55, 0.1, 0.2))
            .build();
        let dark = ThemeBuilder::dark()
            .accent(cosmic::cosmic_theme::palette::Srgb::new(0.4, 0.85, 1.0))
            .build();
        fixture.write(&keys[1], light.background(false));
        fixture.write(&keys[2], &light.accent);
        fixture.write(&keys[3], dark.background(false));
        fixture.write(&keys[4], &dark.accent);
        for (is_dark, expected) in [(false, &light), (true, &dark)] {
            fixture.write(&keys[0], &is_dark);
            let palette = read_palette(&fixture.store()).unwrap();
            let accent = expected.accent.base;
            // COSMIC writes these roles as hexadecimal RGBA, quantizing
            // the builder's floats to one byte per channel.
            for (actual, expected) in [
                (palette.accent.r, accent.red),
                (palette.accent.g, accent.green),
                (palette.accent.b, accent.blue),
                (palette.background.r, expected.background(false).base.red),
                (palette.foreground.r, expected.background(false).on.red),
            ] {
                assert!((actual - expected).abs() <= 1.0 / 255.0 + f32::EPSILON);
            }
        }
        assert!(!fixture.0.join("state").exists(), "following is read-only");
        assert!(!fixture.0.join("config.json").exists());
    }

    #[test]
    fn dark_and_pale_desktop_accents_remain_readable_widget_text() {
        let fixture = Fixture::new();
        let keys = keys();
        for (dark, base) in [(true, [0.0, 0.26, 0.31]), (false, [0.70, 0.96, 0.83])] {
            let theme = if dark {
                ThemeBuilder::dark()
            } else {
                ThemeBuilder::light()
            }
            .accent(cosmic::cosmic_theme::palette::Srgb::new(
                base[0], base[1], base[2],
            ))
            .build();
            let offset = if dark { 3 } else { 1 };
            fixture.write(&keys[0], &dark);
            fixture.write(&keys[offset], theme.background(false));
            fixture.write(&keys[offset + 1], &theme.accent);
            let palette = read_palette(&fixture.store()).unwrap();
            let linear = |v: f32| {
                if v <= 0.04045 {
                    v / 12.92
                } else {
                    ((v + 0.055) / 1.055).powf(2.4)
                }
            };
            let luminance = |color: Color| {
                0.2126 * linear(color.r) + 0.7152 * linear(color.g) + 0.0722 * linear(color.b)
            };
            let background = luminance(palette.background);
            for text in [palette.accent, palette.foreground, palette.muted] {
                let ink = luminance(text);
                let ratio = (ink.max(background) + 0.05) / (ink.min(background) + 0.05);
                assert!(
                    ratio >= 4.5,
                    "Widget text must stay readable: {dark} {text:?} contrast {ratio}"
                );
            }
            assert!(
                !fixture.0.join("state").exists(),
                "Color following never writes the desktop"
            );
        }
    }

    #[test]
    fn cached_colors_are_opt_in_and_never_serialized_or_override_card_opacity() {
        let mut config: Config =
            serde_json::from_str(r#"{"theme":"amber","appearance":{"opacity":83}}"#).unwrap();
        let original = config.palette();
        assert_eq!(config.appearance.color_source, ColorSource::Custom);
        config.desktop_palette = Some(Palette::named("teal"));
        assert_eq!(config.palette(), original, "upgrade keeps the custom look");
        config.appearance.color_source = ColorSource::Desktop;
        assert_eq!(config.palette().accent, Palette::named("teal").accent);
        assert_eq!(config.palette().background.a, 0.83);
        let raw = serde_json::to_value(&config).unwrap();
        assert!(raw.get("desktop_palette").is_none());
        assert_eq!(raw["appearance"]["color_source"], "desktop");
        let restored: Config = serde_json::from_value(raw).unwrap();
        assert!(restored.desktop_palette.is_none());
        assert_eq!(
            restored.palette(),
            original,
            "unavailable desktop uses retained custom palette"
        );
    }

    #[test]
    fn unavailable_or_unrecognized_theme_data_is_never_rewritten() {
        let fixture = Fixture::new();
        let before = std::fs::read_dir(&fixture.0).unwrap().count();
        assert!(
            read_palette(&fixture.store()).is_ok(),
            "missing roles use COSMIC defaults"
        );
        assert_eq!(std::fs::read_dir(&fixture.0).unwrap().count(), before);
        let key = &keys()[0];
        fixture.write(key, &"future-mode");
        let file = fixture.store().user_path(key);
        let bytes = std::fs::read(&file).unwrap();
        assert!(read_palette(&fixture.store()).is_err());
        assert_eq!(std::fs::read(&file).unwrap(), bytes);
        assert!(!fixture.0.join("state").exists());
    }
}
