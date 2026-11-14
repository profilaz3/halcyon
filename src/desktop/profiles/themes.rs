//! Appearance-only records share the profile planner, library and transaction.
//! The strict boundary here is also enforced on imported theme records.
use super::*;
use crate::config::Appearance;
use crate::desktop::{
    DesktopStore,
    appearance::{self, AppearanceDraft, ColorRole, Control, ThemeVariant, Value as Edit},
    panels::Background,
    store::widget,
};
use crate::desktop_theme::{self, Family};
use serde_json::{Value, json};

/// The stable library ID of the appearance retained before the first theme.
pub const BEFORE_THEMES: &str = "before-themes";
pub const STYLE: &[&str] = &[
    "/theme",
    "/appearance/family",
    "/appearance/native_mode_palettes",
    "/appearance/color_policy",
    "/appearance/overrides",
    "/appearance/surface",
    "/appearance/wallpaper_light",
    "/appearance/accent",
    "/appearance/color_source",
    "/appearance/sync_colors",
    "/appearance/wallpaper_colors",
    "/appearance/opacity",
    "/appearance/corner_radius",
    "/appearance/border_opacity",
    "/appearance/contrast_assist",
    "/glance/style",
    "/glance/opacity",
];
pub fn family(profile: &Profile) -> Option<Family> {
    profile
        .widget_style
        .as_ref()
        .and_then(|v| v.pointer("/appearance/family"))
        .and_then(|v| serde_json::from_value(v.clone()).ok())
        .flatten()
}
/// Whether a record chooses a whole style. A Reset of one surface to the
/// applied style holds only that surface (see the Themes editor).
pub fn selects_style(p: &Profile) -> bool {
    family(p).is_some() && p.dock.is_some() && p.panel.is_some() && p.appearance.is_some()
}
/// Native appearance alone: the Windows Reset. Its outline follows the
/// accent wherever COSMIC draws it, including a color only the derived theme
/// holds; choosing a style leaves such an outline as it is.
pub fn resets_windows(p: &Profile) -> bool {
    p.appearance_only
        && p.appearance.is_some()
        && LookColor::of(p).is_none()
        && !selects_style(p)
        && p.dock.is_none()
        && p.panel.is_none()
}

/// A look's color, chosen independently of its style.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LookColor {
    /// Follows the wallpaper; the look is recolored from its analysis.
    Wallpaper,
    /// One of the wallpaper's hues or a custom color, kept as it is.
    Fixed([u8; 3]),
    /// The style's own color.
    Style,
}

impl LookColor {
    /// A color equal to the style's own is that style's, as the Color row
    /// shows it, so another style uses its own.
    fn choose(family: Option<Family>, automatic: bool, accent: Option<[u8; 3]>) -> Self {
        match accent {
            _ if automatic => Self::Wallpaper,
            Some(rgb) if Some(rgb) != family.map(Family::accent) => Self::Fixed(rgb),
            _ => Self::Style,
        }
    }
    /// The color an applied look uses.
    pub fn applied(a: &Appearance) -> Self {
        match a.color_policy {
            Some(crate::config::ColorPolicy::Wallpaper) => Self::Wallpaper,
            Some(crate::config::ColorPolicy::Theme) => Self::Style,
            Some(crate::config::ColorPolicy::Custom) => Self::Fixed(
                a.accent
                    .unwrap_or_else(|| a.family.map_or([52, 180, 180], Family::accent)),
            ),
            None => Self::choose(a.family, a.wallpaper_colors, a.accent),
        }
    }
    /// The color a record chooses; `None` when it has no color fields, as a
    /// surface Reset leaves them out.
    pub fn of(p: &Profile) -> Option<Self> {
        let style = p.widget_style.as_ref()?.get("appearance")?;
        if let Some(policy) = style.get("color_policy").and_then(|v| v.as_str()) {
            match policy {
                "wallpaper" => return Some(Self::Wallpaper),
                "theme" => return Some(Self::Style),
                "custom" => {
                    if let Some(rgb) = style
                        .get("accent")
                        .and_then(|v| serde_json::from_value(v.clone()).ok())
                    {
                        return Some(Self::Fixed(rgb));
                    }
                }
                _ => {}
            }
        }
        let automatic = style
            .get("wallpaper_colors")
            .map(|v| v.as_bool() == Some(true));
        let accent = style.get("accent").map(|v| {
            serde_json::from_value::<Option<[u8; 3]>>(v.clone())
                .ok()
                .flatten()
        });
        if automatic.is_none() && accent.is_none() {
            return None;
        }
        Some(Self::choose(
            family(p),
            automatic.unwrap_or(false),
            accent.flatten(),
        ))
    }
    /// Gives a style record this color. Explicit native surfaces stay.
    pub fn set(self, p: &mut Profile) {
        let style = p.widget_style.get_or_insert_with(|| json!({}));
        let _ = widget::put(
            style,
            "/appearance/wallpaper_colors",
            Some(json!(self == Self::Wallpaper)),
        );
        let _ = widget::put(
            style,
            "/appearance/color_policy",
            Some(json!(match self {
                Self::Wallpaper => "wallpaper",
                Self::Style => "theme",
                Self::Fixed(_) => "custom",
            })),
        );
        match self {
            Self::Wallpaper => {}
            Self::Fixed(rgb) => recolor(p, Some(rgb)),
            Self::Style => recolor(p, None),
        }
    }
}
fn hex(rgb: [u8; 3]) -> ColorChoice {
    ColorChoice::Hex(format!("#{:02x}{:02x}{:02x}", rgb[0], rgb[1], rgb[2]))
}
/// The native accents for a shared color, one per COSMIC variant. Theme
/// selection, Wallpaper & colors and the runtime all use these, so a later
/// automatic update never makes linked bars look Custom. The stored source
/// accent remains the user's own RGB.
pub fn native_accents(rgb: [u8; 3]) -> [(ThemeVariant, [u8; 3]); 2] {
    ThemeVariant::ALL.map(|variant| {
        (
            variant,
            desktop_theme::native_accent(rgb, variant == ThemeVariant::Dark),
        )
    })
}
/// Stages a shared color's native accents in both variants.
pub fn link_accent(draft: &mut AppearanceDraft, rgb: [u8; 3]) {
    for (variant, [r, g, b]) in native_accents(rgb) {
        draft.set_linked_accent(
            variant,
            cosmic::cosmic_theme::palette::Srgba::new(
                f32::from(r) / 255.,
                f32::from(g) / 255.,
                f32::from(b) / 255.,
                1.,
            ),
        );
    }
}
/// Refresh only accents still linked to applied intent, including explicit
/// equal-valued overrides. Opening an editor never establishes a link.
pub fn linked_accents(draft: &mut AppearanceDraft, original: Appearance, rgb: [u8; 3]) {
    for (variant, native) in native_accents(rgb) {
        let key = appearance::theme::builder_key(variant, "accent");
        if original.overrides & crate::desktop::actions::override_for_native(&key) != 0 {
            continue;
        }
        if original.family.is_some() && !linked_accent(draft, original, variant) {
            continue;
        }
        if original.family.is_none() && original.accent.is_some() {
            let old = desktop_theme::native_accent(
                original.accent.unwrap(),
                variant == ThemeVariant::Dark,
            );
            if !same(base_color(draft, variant, ColorRole::Accent).flatten(), old) {
                continue;
            }
        }
        let [r, g, b] = native;
        draft.set_linked_accent(
            variant,
            cosmic::cosmic_theme::palette::Srgba::new(
                f32::from(r) / 255.,
                f32::from(g) / 255.,
                f32::from(b) / 255.,
                1.,
            ),
        );
    }
}

/// A family's native look: accents for both variants, the active-window
/// outline following that accent, and the owned variant's surfaces (its
/// background is what the dock and panel paint).
pub fn appearance(family: Family, accent: Option<[u8; 3]>) -> AppearanceLook {
    let seed = accent.unwrap_or_else(|| family.accent());
    let look = |variant: ThemeVariant| {
        let dark = variant == ThemeVariant::Dark;
        let mut look = VariantLook {
            accent: Some(hex(desktop_theme::native_accent(seed, dark))),
            outline: Some(ColorChoice::Auto),
            ..Default::default()
        };
        // Color synchronization does not own native backdrop effects. Leave
        // existing system preferences untouched, including on legacy Frosted.
        look.background = Some(hex(family.surface_for(seed, dark)));
        look.primary = Some(hex(family.container_for(seed, dark)));
        look
    };
    AppearanceLook {
        light: Some(look(ThemeVariant::Light)),
        dark: Some(look(ThemeVariant::Dark)),
        ..Default::default()
    }
}
pub fn record(family: Family) -> Profile {
    let mut p = Profile::new(family.title()).unwrap();
    p.appearance_only = true;
    p.version = super::format::VERSION_7;
    let (opacity, radius, border) = family.geometry();
    p.widget_style = Some(
        json!({"appearance":{"family":family,"native_mode_palettes":1,"surface":null,"opacity":opacity,"corner_radius":radius,"border_opacity":border,"accent":null,"color_source":"custom","sync_colors":true,"wallpaper_colors":false,"contrast_assist":false}}),
    );
    let (opacity, radius, padding, spacing) = family.bar();
    let bar = PanelLook {
        island_gap: Some(family.spec().bar.island_gap),
        opacity: Some(opacity),
        border_radius: Some(radius),
        padding: Some(padding),
        spacing: Some(spacing),
        // COSMIC owns mode switching, including while Halcyon is stopped.
        background: Some(Background::ThemeDefault),
        floating: Some(family.spec().bar.floating),
        margin: family.spec().bar.floating.then_some(6),
        ..Default::default()
    };
    p.dock = Some(bar.clone());
    p.panel = Some(bar);
    p.appearance = Some(appearance(family, None));
    p
}

/// Explicit repair is a normal reviewable profile, with no applets, placement,
/// geometry, widget mode, or wallpaper edits. Legacy installations have no
/// reliable fixed-color provenance, so opening Settings never applies it.
pub fn repair_matching(store: &DesktopStore) -> Result<Profile, ProfileError> {
    let (_, config) =
        crate::desktop::wallpaper::automatic::read_config(store).map_err(ProfileError::Storage)?;
    let family = config
        .appearance
        .family
        .ok_or_else(|| ProfileError::Invalid("Choose a style first".into()))?;
    let mut profile = Profile::new("Repair theme matching").expect("fixed valid name");
    profile.appearance_only = true;
    profile.version = super::format::VERSION_7;
    let mut look = appearance(family, config.appearance.accent);
    for variant in [&mut look.light, &mut look.dark].into_iter().flatten() {
        variant.accent = None;
        variant.outline = None;
        variant.frosted = None;
    }
    profile.appearance = Some(look);
    let bar = PanelLook {
        background: Some(Background::ThemeDefault),
        ..Default::default()
    };
    profile.dock = Some(bar.clone());
    profile.panel = Some(bar);
    profile.widget_style = Some(json!({"appearance":{"native_mode_palettes":1}}));
    let current = capture(store, "Current surfaces")?.profile;
    preserve_explicit(config.appearance, &current, &mut profile)?;
    Ok(profile)
}

/// Built-in family selection preserves each customized property.
/// Saved variations and explicit Reset bypass this adaptation.
pub fn preserve_custom(
    store: &DesktopStore,
    installed: &Installed,
    next: &mut Profile,
) -> Result<(), ProfileError> {
    let (_, config) =
        crate::desktop::wallpaper::automatic::read_config(store).map_err(ProfileError::Storage)?;
    if let Some(style) = &mut next.widget_style {
        widget::put(
            style,
            "/appearance/sync_colors",
            Some(json!(config.appearance.sync_colors)),
        )
        .map_err(ProfileError::Invalid)?;
        widget::put(
            style,
            "/appearance/overrides",
            Some(json!(config.appearance.overrides)),
        )
        .map_err(ProfileError::Invalid)?;
    }
    let captured = capture(store, "Current surfaces")?.profile;
    let Some(previous) = config.appearance.family else {
        LookColor::applied(&config.appearance).set(next);
        return preserve_explicit(config.appearance, &captured, next);
    };
    // Preserving unproven legacy roles is not a native palette migration.
    // The planner separately verifies that a generation request includes
    // accepted native work; this adapter must not request it for kept roles.
    if config.appearance.native_mode_palettes != 1
        && let Some(style) = &mut next.widget_style
    {
        widget::put(style, "/appearance/native_mode_palettes", Some(json!(0)))
            .map_err(ProfileError::Invalid)?;
    }
    let mut expected = record(previous);
    expected.appearance = Some(appearance(previous, config.appearance.accent));
    let comparison = plan(
        store,
        installed,
        &expected,
        &[Domain::Dock, Domain::Panel].into_iter().collect(),
    );
    let draft = AppearanceDraft::load(store)?;
    // Only the surfaces the bars paint keep bar backgrounds; an accent or
    // outline override is a window detail (see `native_custom`).
    let custom_colors = native_custom(&draft, config.appearance).surfaces;
    let mut kept_variants = Vec::new();
    for (domain, current, before, target) in [
        (Domain::Dock, &captured.dock, &expected.dock, &mut next.dock),
        (
            Domain::Panel,
            &captured.panel,
            &expected.panel,
            &mut next.panel,
        ),
    ] {
        let custom_bar = comparison
            .domains
            .iter()
            .any(|p| p.domain == domain && !p.changes.is_empty());
        let keep_background =
            config.appearance.overrides & (1 << if domain == Domain::Dock { 12 } else { 20 }) != 0
                || custom_colors
                || current
                    .as_ref()
                    .zip(before.as_ref())
                    .is_some_and(|(current, before)| current.background != before.background);
        let linked_background = target.as_ref().and_then(|p| p.background.clone());
        if custom_bar || config.appearance.overrides != 0 {
            if let (Some(current), Some(before), Some(target)) =
                (current.as_ref(), before.as_ref(), target.as_mut())
            {
                let start = if domain == Domain::Dock { 8 } else { 16 };
                macro_rules! keep {
                    ($field:ident, $bit:expr) => {
                        if current.$field != before.$field
                            || config.appearance.overrides & (1 << (start + $bit)) != 0
                        {
                            target.$field = current.$field.clone();
                        }
                    };
                }
                keep!(opacity, 0);
                keep!(border_radius, 1);
                keep!(padding, 2);
                keep!(spacing, 3);
                keep!(background, 4);
                if current.island_gap != before.island_gap {
                    target.island_gap = current.island_gap;
                }
            }
        }
        if let Some(target) = target {
            if keep_background {
                target.background = current.as_ref().and_then(|p| p.background.clone());
                match target.background {
                    Some(Background::Dark) => kept_variants.push(ThemeVariant::Dark),
                    Some(Background::Light) => kept_variants.push(ThemeVariant::Light),
                    // Theme follows the current native mode, even when the
                    // previous family owns only the opposite variant's roles.
                    Some(Background::ThemeDefault) => kept_variants.push(draft.active_variant()),
                    // A style's own bar color: the windows' colors stay on
                    // the variant in use, as for the theme's default.
                    Some(Background::Color(_)) => kept_variants.push(draft.active_variant()),
                    None => {}
                }
            } else {
                // A geometry override does not unlink native colors.
                target.background = linked_background;
            }
        }
    }
    let a = config.appearance;
    let (opacity, corners, border) = previous.geometry();
    if let Some(style) = &mut next.widget_style {
        for (pointer, value, differs) in [
            ("/appearance/surface", json!(a.surface), a.surface.is_some()),
            (
                "/appearance/opacity",
                json!(a.opacity),
                a.opacity != opacity,
            ),
            (
                "/appearance/corner_radius",
                json!(a.corner_radius),
                a.corner_radius != corners,
            ),
            (
                "/appearance/border_opacity",
                json!(a.border_opacity),
                a.border_opacity != border,
            ),
            (
                "/appearance/contrast_assist",
                json!(a.contrast_assist),
                a.contrast_assist,
            ),
        ] {
            if differs || a.overrides & crate::desktop::actions::override_for_widget(pointer) != 0 {
                widget::put(style, pointer, Some(value)).map_err(ProfileError::Invalid)?;
            }
        }
    }
    // Keep native color overrides on their original variants. Exact capture
    // values and hex family roles must be compared as colors, not syntax.
    if let (Some(current), Some(target)) = (&captured.appearance, &mut next.appearance) {
        for variant in ThemeVariant::ALL {
            let before = expected
                .appearance
                .as_ref()
                .and_then(|a| a.variant(variant));
            let current = current.variant(variant);
            let target = match variant {
                ThemeVariant::Light => &mut target.light,
                ThemeVariant::Dark => &mut target.dark,
            };
            if let (Some(before), Some(current), Some(target)) = (before, current, target) {
                if !linked_accent(&draft, config.appearance, variant) {
                    target.accent = current.accent.clone();
                }
                for (role, owned, actual, desired) in [
                    (
                        ColorRole::Background,
                        &before.background,
                        &current.background,
                        &mut target.background,
                    ),
                    (
                        ColorRole::Primary,
                        &before.primary,
                        &current.primary,
                        &mut target.primary,
                    ),
                ] {
                    if kept_variants.contains(&variant)
                        || (owned.is_some()
                            && !linked_surface(&draft, config.appearance, variant, role))
                    {
                        *desired = actual.clone();
                    }
                }
            }
        }
    }
    // A window outline chosen after the first theme stays; one that predates
    // themes (still equal to Before Themes) now follows the accent.
    if let (Some(current), Some(target)) = (&captured.appearance, &mut next.appearance) {
        let before = ProfileLibrary::new(store)
            .load(&ProfileId::parse(BEFORE_THEMES).expect("stable id"))
            .ok()
            .and_then(|p| p.appearance);
        for variant in ThemeVariant::ALL {
            let outline = |look: Option<&AppearanceLook>| {
                look.and_then(|a| a.variant(variant))
                    .and_then(|v| v.outline.as_ref())
                    .and_then(|c| c.color(ColorRole::Outline).ok())
            };
            let now = outline(Some(current));
            let original = outline(before.as_ref());
            let target = match variant {
                ThemeVariant::Light => &mut target.light,
                ThemeVariant::Dark => &mut target.dark,
            };
            if let (Some(Some(_)), Some(target)) = (now, target)
                && original.is_some_and(|original| original != now.flatten())
            {
                target.outline = current.variant(variant).and_then(|v| v.outline.clone());
            }
        }
    }
    // The color is independent of the selected style: following the
    // wallpaper, a fixed color or the style's own carries over.
    LookColor::applied(&config.appearance).set(next);
    preserve_explicit(config.appearance, &captured, next)
}
/// An explicit choice remains custom even when equal to an old default, and
/// even when it was made before selecting the first family.
fn preserve_explicit(
    a: Appearance,
    current: &Profile,
    next: &mut Profile,
) -> Result<(), ProfileError> {
    for (current, target, start) in [
        (&current.dock, &mut next.dock, 8),
        (&current.panel, &mut next.panel, 16),
    ] {
        if let (Some(current), Some(target)) = (current, target) {
            macro_rules! keep {
                ($field:ident,$bit:expr) => {
                    if a.overrides & (1 << (start + $bit)) != 0 {
                        target.$field = current.$field.clone();
                    }
                };
            }
            keep!(opacity, 0);
            keep!(border_radius, 1);
            keep!(padding, 2);
            keep!(spacing, 3);
            keep!(background, 4);
        }
    }
    if let Some(style) = &mut next.widget_style {
        for (pointer, value) in [
            ("/appearance/opacity", json!(a.opacity)),
            ("/appearance/corner_radius", json!(a.corner_radius)),
            ("/appearance/border_opacity", json!(a.border_opacity)),
            ("/appearance/surface", json!(a.surface)),
            ("/appearance/contrast_assist", json!(a.contrast_assist)),
        ] {
            if a.overrides & crate::desktop::actions::override_for_widget(pointer) != 0 {
                widget::put(style, pointer, Some(value)).map_err(ProfileError::Invalid)?;
            }
        }
    }
    if let (Some(current), Some(target)) = (&current.appearance, &mut next.appearance) {
        for variant in ThemeVariant::ALL {
            let target = match variant {
                ThemeVariant::Dark => &mut target.dark,
                ThemeVariant::Light => &mut target.light,
            };
            if let (Some(current), Some(target)) = (current.variant(variant), target) {
                let start = if variant == ThemeVariant::Dark {
                    24
                } else {
                    32
                };
                macro_rules! keep {
                    ($field:ident,$bit:expr) => {
                        if a.overrides & (1 << (start + $bit)) != 0 {
                            target.$field = current.$field.clone();
                        }
                    };
                }
                keep!(accent, 0);
                keep!(background, 1);
                keep!(primary, 2);
                keep!(secondary, 3);
                keep!(outline, 4);
            }
        }
    }
    Ok(())
}
/// Every built-in theme: the styles, then the ready-made themes.
pub fn presets() -> Vec<Preset> {
    Family::ALL
        .into_iter()
        .map(|f| Preset {
            id: f.preset(),
            description: f.description(),
            profile: record(f),
        })
        .chain(named().iter().cloned())
        .collect()
}

mod resources {
    // `FILES`: every `resources/themes/<id>.json` as `theme-<id>`, embedded
    // by build.rs.
    include!(concat!(env!("OUT_DIR"), "/built_in_themes.rs"));
}

/// A ready-made theme shipped as `resources/themes/<id>.json` (`id` is
/// `theme-<id>`): a style in its own colors, with its own dock and panel.
/// It applies exactly as it was saved, so it must choose all of them and
/// keep its color instead of following the wallpaper.
pub fn parse_named(id: &str, text: &str) -> Result<Profile, String> {
    let profile = import(text.as_bytes()).map_err(|error| format!("{id}: {error}"))?;
    if Family::ALL.into_iter().any(|f| f.preset() == id) {
        return Err(format!("{id}: this ID belongs to a style"));
    }
    if !profile.appearance_only {
        return Err(format!("{id}: a built-in theme holds appearance only"));
    }
    validate(&profile).map_err(|error| format!("{id}: {error}"))?;
    if profile.wallpaper.is_some() {
        return Err(format!(
            "{id}: a built-in theme keeps the wallpaper; a look can bring one"
        ));
    }
    if !selects_style(&profile) {
        return Err(format!(
            "{id}: a built-in theme chooses a style, colors, the dock and the panel"
        ));
    }
    if !matches!(LookColor::of(&profile), Some(LookColor::Fixed(_))) {
        return Err(format!(
            "{id}: a built-in theme keeps its own color instead of following the wallpaper"
        ));
    }
    if profile.description.is_none() {
        return Err(format!("{id}: a built-in theme needs a description"));
    }
    if super::built_in::personal_member(&profile).is_some() {
        return Err(format!("{id}: a built-in theme can't carry personal data"));
    }
    Ok(profile)
}

/// The ready-made themes, parsed once. A file that doesn't parse is left
/// out (the tests refuse it before release).
pub fn named() -> &'static [Preset] {
    static THEMES: std::sync::OnceLock<Vec<Preset>> = std::sync::OnceLock::new();
    THEMES.get_or_init(|| {
        resources::FILES
            .iter()
            .filter_map(|(id, text)| {
                let profile = parse_named(id, text).ok()?;
                // Parsed once for the life of the app, like the built-in looks.
                let description: &'static str = Box::leak(
                    profile
                        .description
                        .clone()
                        .unwrap_or_default()
                        .into_boxed_str(),
                );
                Some(Preset {
                    id,
                    description,
                    profile,
                })
            })
            .collect()
    })
}

/// Whether `id` is a ready-made theme, which applies as saved instead of
/// keeping the desktop's own color and bar choices (see `preserve_custom`).
pub fn is_named(id: &str) -> bool {
    named().iter().any(|preset| preset.id == id)
}

/// The ready-made theme the applied appearance is: its style, color and
/// widget shape. None after any of them changed.
pub fn applied_named(a: &Appearance) -> Option<&'static Preset> {
    named().iter().find(|preset| {
        let style = preset
            .profile
            .widget_style
            .as_ref()
            .and_then(|s| s.get("appearance"));
        let number = |key: &str| style.and_then(|s| s.get(key)).and_then(Value::as_u64);
        a.family.is_some()
            && a.family == family(&preset.profile)
            && Some(LookColor::applied(a)) == LookColor::of(&preset.profile)
            && number("opacity") == Some(u64::from(a.opacity))
            && number("corner_radius") == Some(u64::from(a.corner_radius))
            && number("border_opacity") == Some(u64::from(a.border_opacity))
    })
}
pub fn matching(family: Family) -> WallpaperLook {
    let (a, b) = family.wallpaper();
    let rgb = |[r, g, b]: [u8; 3]| Rgb::Hex(format!("#{r:02x}{g:02x}{b:02x}"));
    WallpaperLook {
        background: Some(BackgroundLook {
            source: Some(SourceLook::Gradient(GradientLook {
                stops: vec![rgb(a), rgb(b)],
                angle: 120.,
            })),
            ..Default::default()
        }),
        ..Default::default()
    }
}
fn visual_bar(p: &PanelLook) -> PanelLook {
    PanelLook {
        island_gap: p.island_gap.or_else(|| p.islands.as_ref().map(|s| s.gap)),
        opacity: p.opacity,
        border_radius: p.border_radius,
        spacing: p.spacing,
        padding: p.padding,
        background: p.background.clone(),
        // Floating is part of a style's look (round styles float).
        floating: p.floating,
        margin: p.margin,
        ..Default::default()
    }
}
fn visual_variant(v: &VariantLook) -> VariantLook {
    VariantLook {
        accent: v.accent.clone(),
        background: v.background.clone(),
        primary: v.primary.clone(),
        secondary: v.secondary.clone(),
        outline: v.outline.clone(),
        // Imported/previously saved color themes cannot opt into global blur.
        // Explicit system settings and guarded Undo remain separate actions.
        ..Default::default()
    }
}
pub(super) fn visual_appearance(a: &AppearanceLook) -> AppearanceLook {
    AppearanceLook {
        light: a.light.as_ref().map(visual_variant),
        dark: a.dark.as_ref().map(visual_variant),
        ..Default::default()
    }
}
/// Historical theme records may carry frosting flags. Keep the old file
/// allowlist distinct from the effective color-only projection: loading must
/// preserve the record, while applying must ignore both legacy On and Off.
fn compatible_appearance(a: &AppearanceLook) -> AppearanceLook {
    let variant = |v: &VariantLook| VariantLook {
        frosted: v.frosted.clone().filter(|f| f.panel != Some(true)),
        ..visual_variant(v)
    };
    AppearanceLook {
        light: a.light.as_ref().map(variant),
        dark: a.dark.as_ref().map(variant),
        ..Default::default()
    }
}
pub fn validate(p: &Profile) -> Result<(), String> {
    if p.widget_layout.is_some()
        || p.windows.is_some()
        || p.workspaces.is_some()
        || p.desktop_icons.is_some()
    {
        return Err("Themes contain appearance only; layout and content belong in a preset".into());
    }
    for bar in [&p.dock, &p.panel].into_iter().flatten() {
        if visual_bar(bar) != *bar {
            return Err(
                "Themes cannot change bar placement, visibility or applet arrangement".into(),
            );
        }
    }
    if p.appearance
        .as_ref()
        .is_some_and(|a| compatible_appearance(a) != *a)
    {
        return Err(
            "Themes cannot change application fonts, mode, density or window layout".into(),
        );
    }
    if let Some(style) = &p.widget_style {
        for (pointer, _) in super::format::widget_members(style, widget::WidgetGroup::Style)? {
            if !STYLE.contains(&pointer) {
                return Err(format!("Themes cannot change {}", widget::label(pointer)));
            }
        }
    }
    if let Some(w) = &p.wallpaper {
        let allowed = WallpaperLook {
            background: w.background.as_ref().map(|s| BackgroundLook {
                source: s.source.clone(),
                ..Default::default()
            }),
            ..Default::default()
        };
        if *w != allowed {
            return Err("A theme can include a shared wallpaper, not display assignments".into());
        }
    }
    Ok(())
}
pub fn capture(store: &DesktopStore, name: &str) -> Result<Capture, ProfileError> {
    let mut c = super::capture(store, name)?;
    let p = &mut c.profile;
    p.appearance_only = true;
    p.dock = p.dock.as_ref().map(visual_bar);
    p.panel = p.panel.as_ref().map(visual_bar);
    p.appearance = p.appearance.as_ref().map(visual_appearance);
    p.widget_layout = None;
    p.windows = None;
    p.workspaces = None;
    p.desktop_icons = None;
    p.wallpaper = None;
    let original = p.widget_style.take().unwrap_or(json!({}));
    let mut style = json!({});
    for pointer in STYLE {
        if let Some(value) = original.pointer(pointer) {
            widget::put(&mut style, pointer, Some(value.clone())).map_err(ProfileError::Invalid)?;
        }
    }
    // Explicit null restores the legacy renderer in the saved initial look.
    if style.pointer("/appearance/family").is_none() {
        widget::put(&mut style, "/appearance/family", Some(Value::Null))
            .map_err(ProfileError::Invalid)?;
    }
    p.widget_style = Some(style);
    p.version = super::format::VERSION_7;
    p.validate()?;
    Ok(c)
}
fn same(current: Option<cosmic::cosmic_theme::palette::Srgba>, expected: [u8; 3]) -> bool {
    current.is_some_and(|v| {
        [
            v.red - f32::from(expected[0]) / 255.,
            v.green - f32::from(expected[1]) / 255.,
            v.blue - f32::from(expected[2]) / 255.,
            v.alpha - 1.,
        ]
        .into_iter()
        .all(|d| d.abs() < 0.005)
    })
}
/// The draft's starting value for a role, ignoring its own edits.
fn base_color(
    draft: &AppearanceDraft,
    variant: ThemeVariant,
    role: ColorRole,
) -> Option<Option<cosmic::cosmic_theme::palette::Srgba>> {
    AppearanceDraft::from_snapshot(draft.snapshot().clone())
        .color(variant, role)
        .known()
        .copied()
}
/// Only surfaces still unset or equal to the previous family roles follow
/// wallpaper tint. An explicit background edit stays Custom through future
/// updates.
/// Surfaces the previous release derived still count as linked, so they
/// move to the current derivation instead of turning Custom.
pub fn linked_surface(
    draft: &AppearanceDraft,
    original: Appearance,
    variant: ThemeVariant,
    role: ColorRole,
) -> bool {
    if original.native_mode_palettes != 1 {
        return false;
    }
    let key = appearance::theme::builder_key(
        variant,
        match role {
            ColorRole::Background => "bg_color",
            ColorRole::Primary => "primary_container_bg",
            ColorRole::Secondary => "secondary_container_bg",
            ColorRole::Outline => "window_hint",
            ColorRole::Accent => "accent",
        },
    );
    if original.overrides & crate::desktop::actions::override_for_native(&key) != 0 {
        return false;
    }
    let Some(family) = original.family else {
        return false;
    };
    let seed = original.accent.unwrap_or_else(|| family.accent());
    let expected = match role {
        ColorRole::Background => family.surface_for(seed, variant == ThemeVariant::Dark),
        ColorRole::Primary => family.container_for(seed, variant == ThemeVariant::Dark),
        _ => return false,
    };
    let previous = desktop_theme::previous_native(family, original.accent).1;
    let current = base_color(draft, variant, role);
    // COSMIC's own default (unset) was never chosen by anyone, so windows
    // follow the style's tint from there too. Unreadable values stay put.
    if current == Some(None) {
        return true;
    }
    let current = current.flatten();
    same(current, expected)
        || (family.light() == (variant == ThemeVariant::Light) && same(current, previous))
}
/// Whether a variant's native accent is what the shared color gives it,
/// in this release or the previous one.
pub fn linked_accent(draft: &AppearanceDraft, original: Appearance, variant: ThemeVariant) -> bool {
    let key = appearance::theme::builder_key(variant, "accent");
    if original.overrides & crate::desktop::actions::override_for_native(&key) != 0 {
        return false;
    }
    let Some(family) = original.family else {
        return false;
    };
    let seed = original.accent.unwrap_or_else(|| family.accent());
    let current = base_color(draft, variant, ColorRole::Accent).flatten();
    same(
        current,
        desktop_theme::native_accent(seed, variant == ThemeVariant::Dark),
    ) || same(
        current,
        desktop_theme::previous_native(family, original.accent).0,
    )
}
/// Native roles of an applied family that no longer follow it.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct NativeCustom {
    /// The owned variant's background or container: what the bars paint.
    pub surfaces: bool,
    /// Accents changed elsewhere, or an outline that does not follow them.
    pub windows: bool,
}
pub fn native_custom(draft: &AppearanceDraft, original: Appearance) -> NativeCustom {
    if original.family.is_none() {
        return NativeCustom::default();
    }
    NativeCustom {
        surfaces: ThemeVariant::ALL.into_iter().any(|variant| {
            [ColorRole::Background, ColorRole::Primary]
                .into_iter()
                .any(|role| !linked_surface(draft, original, variant, role))
        }),
        windows: ThemeVariant::ALL.into_iter().any(|variant| {
            !linked_accent(draft, original, variant)
                || base_color(draft, variant, ColorRole::Outline) != Some(None)
                // COSMIC also draws a color only its derived theme holds.
                || draft.derived_outline(variant).is_some()
        }),
    }
}

/// Generated edits never turn a linked baseline into a Custom surface.
/// Callers retain both the native and app-color baselines across recovery.
pub fn linked_colors(draft: &mut AppearanceDraft, original: Appearance, rgb: [u8; 3]) {
    let Some(family) = original.family else {
        return;
    };
    let next = appearance(family, Some(rgb));
    for variant in ThemeVariant::ALL {
        for role in [ColorRole::Background, ColorRole::Primary] {
            let get = |look: &AppearanceLook| {
                let v = look.variant(variant)?;
                match role {
                    ColorRole::Background => v.background.as_ref(),
                    _ => v.primary.as_ref(),
                }
                .and_then(|c| c.color(role).ok())
                .flatten()
            };
            if let Some(after) = get(&next)
                && linked_surface(draft, original, variant, role)
            {
                draft.set(Control::Color(variant, role), Edit::Color(Some(after)));
            }
        }
    }
}

/// Change shared colors without replacing any explicit native surface override.
pub fn recolor(p: &mut Profile, accent: Option<[u8; 3]>) {
    let Some(f) = family(p) else {
        return;
    };
    let previous = p
        .widget_style
        .as_ref()
        .and_then(|s| s.pointer("/appearance/accent"))
        .and_then(|v| serde_json::from_value(v.clone()).ok())
        .flatten();
    let old = appearance(f, previous);
    let next = appearance(f, accent);
    let flags = p
        .widget_style
        .as_ref()
        .and_then(|s| s.pointer("/appearance/overrides"))
        .and_then(Value::as_u64)
        .unwrap_or(0);
    if let Some(current) = &mut p.appearance {
        for (v, before, after, start) in [
            (&mut current.light, old.light, next.light, 32),
            (&mut current.dark, old.dark, next.dark, 24),
        ] {
            if let (Some(v), Some(before), Some(after)) = (v, before, after) {
                if flags & (1 << start) == 0 {
                    v.accent = after.accent;
                }
                if flags & (1 << (start + 1)) == 0 && v.background == before.background {
                    v.background = after.background;
                }
                if flags & (1 << (start + 2)) == 0 && v.primary == before.primary {
                    v.primary = after.primary;
                }
            }
        }
    }
    if let Some(s) = &mut p.widget_style {
        let _ = widget::put(s, "/appearance/accent", Some(json!(accent)));
    }
}

#[cfg(test)]
mod frosted_tests {
    use super::*;
    #[test]
    fn color_themes_never_write_native_frosting() {
        for family in Family::ALL {
            let look = appearance(family, None);
            for variant in [look.light.as_ref(), look.dark.as_ref()]
                .into_iter()
                .flatten()
            {
                assert!(variant.frosted.is_none(), "{family:?}");
                assert!(variant.accent.is_some());
                assert!(variant.background.is_some());
            }
        }
        let legacy: VariantLook = serde_json::from_value(json!({
            "accent": "#247a91",
            "frosted": {"windows": true, "applets": true, "system_interface": true}
        }))
        .unwrap();
        let colors = visual_variant(&legacy);
        assert!(colors.frosted.is_none());
        assert_eq!(colors.accent, legacy.accent);
    }
}

#[cfg(test)]
#[path = "theme_color_tests.rs"]
mod color_tests;

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::desktop::store::tests::Fixture;
    use std::fs;

    /// The themes the person saved that ship as ready-made themes, exactly
    /// as Save style wrote them.
    pub(crate) const SAVED: [(&str, &str); 2] = [
        (
            "theme-rainy-lake",
            include_str!("testdata/saved-themes/rainy-lake.json"),
        ),
        (
            "theme-deep-purple",
            include_str!("testdata/saved-themes/deep-purple.json"),
        ),
    ];

    /// What a saved theme records, as `capture` writes it, in one case. A
    /// ready-made theme keeps its own color where the saved one followed
    /// the wallpaper, and the wallpaper's lightness is the desktop's.
    pub(crate) fn recorded(p: &Profile) -> Value {
        let mut value = serde_json::to_value(p).unwrap();
        let mut out = json!({});
        for key in ["dock", "panel", "appearance", "widget_style"] {
            out[key] = value[key].take();
        }
        let style = out["widget_style"]["appearance"].as_object_mut().unwrap();
        for key in ["color_policy", "wallpaper_colors", "wallpaper_light"] {
            style.remove(key);
        }
        serde_json::from_str(&out.to_string().to_lowercase()).unwrap()
    }

    #[test]
    fn ready_made_themes_ship_parse_and_keep_their_own_color() {
        assert_eq!(named().len(), resources::FILES.len(), "every file parses");
        for (id, text) in resources::FILES {
            let profile = parse_named(id, text).unwrap();
            assert!(is_named(id));
            assert!(presets().iter().any(|p| p.id == *id));
            assert!(Family::parse(id).is_none(), "{id} is not a style's ID");
            assert!(matches!(LookColor::of(&profile), Some(LookColor::Fixed(_))));
            assert!(valid_description(profile.description.as_deref().unwrap()));
            let description = profile.description.as_deref().unwrap();
            assert!(
                !description.contains(['\u{2013}', '\u{2014}']),
                "{id}: {description}"
            );
        }
        for (id, saved) in SAVED {
            let shipped = named().iter().find(|p| p.id == id).unwrap();
            let saved = import(saved.as_bytes()).unwrap();
            assert_eq!(shipped.profile.name, saved.name);
            assert_eq!(recorded(&shipped.profile), recorded(&saved), "{id}");
        }
        // A wallpaper, a following color or a style's ID is refused.
        let (id, text) = resources::FILES[0];
        let mut value: Value = serde_json::from_str(text).unwrap();
        value["widget_style"]["appearance"]["color_policy"] = "wallpaper".into();
        value["widget_style"]["appearance"]["wallpaper_colors"] = true.into();
        assert!(parse_named(id, &value.to_string()).is_err());
        let mut value: Value = serde_json::from_str(text).unwrap();
        value["wallpaper"] = json!({"background":{"source":{"bundled":"ocean"}}});
        assert!(parse_named(id, &value.to_string()).is_err());
        assert!(parse_named("theme-soft", text).is_err());
    }

    /// Each ready-made theme also ships as a look with a bundled wallpaper
    /// and a weather effect. Applied on a default install, it leaves the
    /// desktop wearing exactly the saved theme.
    #[test]
    fn ready_made_looks_wear_their_saved_themes_exactly() {
        for (id, wallpaper, rain, depth) in [
            ("rainy-lake", "halcyon-fog", true, true),
            ("deep-purple", "halcyon-night", false, false),
        ] {
            let saved = SAVED
                .iter()
                .find(|(theme, _)| *theme == format!("theme-{id}"))
                .map(|(_, text)| import(text.as_bytes()).unwrap())
                .unwrap();
            let look = super::super::built_in_looks()
                .iter()
                .find(|look| look.id == id)
                .unwrap_or_else(|| panic!("resources/looks/{id}.json loads"));
            assert_eq!(look.profile.name, saved.name);
            assert_eq!(look.profile.version, super::super::VERSION);
            assert!(!look.description.contains(['\u{2013}', '\u{2014}']));
            assert_eq!(
                look.profile
                    .wallpaper
                    .as_ref()
                    .unwrap()
                    .background
                    .as_ref()
                    .unwrap()
                    .source,
                Some(SourceLook::Bundled(wallpaper.into()))
            );
            let style = look.profile.widget_style.as_ref().unwrap();
            assert_eq!(
                style.pointer("/rain/enabled").and_then(Value::as_bool) == Some(true),
                rain,
                "{id}"
            );
            if rain {
                assert_eq!(style.pointer("/rain/effect"), Some(&json!("rain")));
            }
            assert_eq!(
                style.pointer("/clock/depth").and_then(Value::as_bool) == Some(true),
                depth,
                "{id}"
            );
            let fixture = Fixture::new(&format!("ready-made-look-{id}"));
            fixture.system(
                "com.system76.CosmicPanel",
                "entries",
                "[\"Panel\",\"Dock\"]",
            );
            fixture.system("com.system76.CosmicPanel.Dock", "anchor", "Bottom");
            fixture.system("com.system76.CosmicPanel.Panel", "anchor", "Top");
            fixture.user("com.system76.CosmicTheme.Mode", "is_dark", "true");
            let bundled = fixture.0.join("pictures/Desktop Cards");
            fs::create_dir_all(&bundled).unwrap();
            let assets = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("assets/wallpapers");
            fs::copy(
                crate::desktop::wallpaper::bundled_path(&assets, wallpaper),
                crate::desktop::wallpaper::bundled_path(&bundled, wallpaper),
            )
            .unwrap();
            let path = fixture.0.join("config.json");
            fs::write(
                &path,
                r#"{"theme":"teal","appearance":{"wallpaper_colors":true}}"#,
            )
            .unwrap();
            let store = fixture.store().with_app_config(path.clone());
            let installed = Installed::discover(&store);
            let config = crate::config::Config::from_bytes(&fs::read(&path).unwrap()).unwrap();
            let dir = crate::desktop::wallpaper::bundled_dir(&store);
            let p = super::super::fitted(look.clone(), &config, &dir, false).profile;
            assert!(
                p.wallpaper.is_some(),
                "{id}: the bundled wallpaper is found"
            );
            let plan = plan(&store, &installed, &p, &p.domains().into_iter().collect());
            assert!(plan.can_apply(), "{id}: {:?}", plan.blocking_reasons());
            apply(&store, &plan).unwrap();
            let captured = capture(&store, "Applied").unwrap().profile;
            assert_eq!(recorded(&captured), recorded(&saved), "{id}");
        }
    }

    #[test]
    fn ready_made_themes_apply_exactly_as_saved_in_both_modes() {
        for (id, saved) in SAVED {
            let saved = import(saved.as_bytes()).unwrap();
            let theme = named().iter().find(|p| p.id == id).unwrap();
            for dark in [true, false] {
                let fixture = Fixture::new(&format!("ready-made-{id}-{dark}"));
                fixture.system(
                    "com.system76.CosmicPanel",
                    "entries",
                    "[\"Panel\",\"Dock\"]",
                );
                fixture.system("com.system76.CosmicPanel.Dock", "anchor", "Bottom");
                fixture.system("com.system76.CosmicPanel.Panel", "anchor", "Top");
                fixture.user(
                    "com.system76.CosmicTheme.Mode",
                    "is_dark",
                    &dark.to_string(),
                );
                // Someone else's look: a style, a wallpaper color and a
                // custom dock.
                for variant in ["Dark", "Light"] {
                    let dir = fixture
                        .0
                        .join("config/cosmic")
                        .join(format!("com.system76.CosmicTheme.{variant}.Builder"))
                        .join("v2");
                    fs::create_dir_all(&dir).unwrap();
                    fs::write(dir.join("accent"), "Some(\"#40352CFF\")").unwrap();
                }
                fixture.system("com.system76.CosmicPanel.Dock", "opacity", "0.5");
                let path = fixture.0.join("config.json");
                fs::write(
                    &path,
                    r#"{"theme":"amber","appearance":{"family":"soft","native_mode_palettes":1,"opacity":70,"corner_radius":8,"border_opacity":30,"accent":[64,53,44],"color_source":"custom","sync_colors":true,"wallpaper_colors":true,"color_policy":"wallpaper","overrides":256},"glance":{"style":"panel"}}"#,
                )
                .unwrap();
                let store = fixture.store().with_app_config(path.clone());
                let installed = Installed::discover(&store);
                let p = &theme.profile;
                let plan = plan(&store, &installed, p, &p.domains().into_iter().collect());
                assert!(plan.can_apply(), "{id}: {:?}", plan.blocking_reasons());
                apply(&store, &plan).unwrap();
                let captured = capture(&store, "Applied").unwrap().profile;
                assert_eq!(recorded(&captured), recorded(&saved), "{id}, dark {dark}");
                let config = crate::config::Config::from_bytes(&fs::read(&path).unwrap()).unwrap();
                let a = &config.appearance;
                assert_eq!(a.color_policy, Some(crate::config::ColorPolicy::Custom));
                assert!(!a.wallpaper_colors);
                assert_eq!(a.overrides, 0, "{id}: the old dock override is gone");
                assert_eq!(applied_named(a).map(|p| p.id), Some(id));
                // The native colors are the saved ones, in both variants.
                let draft = AppearanceDraft::load(&store).unwrap();
                for variant in ThemeVariant::ALL {
                    let expected = saved.appearance.as_ref().unwrap().variant(variant).unwrap();
                    for (role, choice) in [
                        (ColorRole::Accent, &expected.accent),
                        (ColorRole::Background, &expected.background),
                        (ColorRole::Primary, &expected.primary),
                    ] {
                        let want = choice.as_ref().unwrap().color(role).unwrap().unwrap();
                        let have = draft
                            .color(variant, role)
                            .known()
                            .copied()
                            .flatten()
                            .unwrap();
                        assert_eq!(
                            appearance::hex(have, false),
                            appearance::hex(want, false),
                            "{id} {variant:?} {role:?}"
                        );
                    }
                }
            }
        }
    }
    #[test]
    fn shared_theme_apply_undo_capture_and_share_preserve_content_and_geometry() {
        for f in Family::ALL {
            let fixture = Fixture::new(&format!("shared-theme-{}", f.id()));
            fixture.system("com.system76.CosmicPanel", "entries", "[\"Dock\"]");
            fixture.system("com.system76.CosmicPanel.Dock", "anchor", "Left");
            fixture.system("com.system76.CosmicPanel.Panel", "anchor", "Top");
            fixture.system(
                "com.system76.CosmicPanel.Dock",
                "plugins_center",
                "Some([\"custom-applet\"])",
            );
            let path = fixture.0.join("config.json");
            let original = json!({"theme":"rose","location":{"name":"PRIVATE","latitude":45.,"longitude":16.},"weather":{"show_dew_point":true},"widget_positions":{"weather":[0.7,0.2]},"widget_sizes":{"weather":[280.,236.]},"glance":{"enabled":true,"show_clock":false,"show_tomorrow":true,"density":"compact"},"devbox":{"swap":false,"memory_cache":false},"calendar_show_tomorrow":true,"appearance":{"opacity":84},"future":{"keep":42}});
            fs::write(&path, original.to_string()).unwrap();
            let store = fixture.store().with_app_config(path.clone());
            let installed = Installed::discover(&store);
            let p = record(f);
            p.validate().unwrap();
            let exported = export(&p).unwrap();
            assert!(exported.omitted.is_empty());
            assert_eq!(import(&exported.bytes).unwrap(), p);
            let before = fs::read(&path).unwrap();
            let plan = plan(&store, &installed, &p, &p.domains().into_iter().collect());
            assert!(plan.can_apply(), "{:?}", plan.blocking_reasons());
            let change = apply(&store, &plan).unwrap();
            let raw: Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
            for key in [
                "location",
                "weather",
                "widget_positions",
                "widget_sizes",
                "glance",
                "devbox",
                "calendar_show_tomorrow",
                "future",
            ] {
                assert_eq!(raw[key], original[key], "{f:?}: {key}");
            }
            for (namespace, key) in [
                ("com.system76.CosmicPanel", "entries"),
                ("com.system76.CosmicPanel.Dock", "anchor"),
                ("com.system76.CosmicPanel.Dock", "plugins_center"),
            ] {
                assert!(
                    !plan
                        .patch()
                        .iter()
                        .any(|(k, _)| k.namespace() == namespace && k.key() == key)
                );
            }
            let baseline = ProfileLibrary::new(&store)
                .load(&ProfileId::parse("before-themes").unwrap())
                .unwrap();
            assert!(baseline.appearance_only);
            assert!(baseline.widget_layout.is_none());
            assert_eq!(
                baseline
                    .widget_style
                    .as_ref()
                    .unwrap()
                    .pointer("/appearance/family"),
                Some(&Value::Null)
            );
            let captured = capture(&store, "My variation").unwrap().profile;
            let shared = import(&export(&captured).unwrap().bytes).unwrap();
            assert!(shared.appearance_only);
            assert!(shared.widget_layout.is_none());
            assert!(
                !String::from_utf8(export(&shared).unwrap().bytes)
                    .unwrap()
                    .contains("PRIVATE")
            );
            let saved_before = export_local(&baseline).unwrap();
            assert!(
                ProfileLibrary::new(&store)
                    .delete(&ProfileId::parse("before-themes").unwrap())
                    .is_err()
            );
            store.undo_latest(&change.id).unwrap();
            assert_eq!(fs::read(&path).unwrap(), before);
            let p = record(Family::Paper);
            apply(
                &store,
                &super::super::plan(&store, &installed, &p, &p.domains().into_iter().collect()),
            )
            .unwrap();
            assert_eq!(
                export_local(
                    &ProfileLibrary::new(&store)
                        .load(&ProfileId::parse("before-themes").unwrap())
                        .unwrap()
                )
                .unwrap(),
                saved_before
            );
        }
    }
    #[test]
    fn shared_theme_transport_refuses_content_and_newer_fields_and_keeps_old_exports() {
        let base = record(Family::Soft);
        let mut v = serde_json::to_value(&base).unwrap();
        for patch in [
            json!({"widget_layout":{"enabled_widgets":[]}}),
            json!({"widget_style":{"devbox":{"swap":false}}}),
            json!({"dock":{"anchor":"Left"}}),
            json!({"appearance":{"mode":"light"}}),
            json!({"version":6}),
            json!({"version":super::format::VERSION + 1}),
            json!({"widget_style":{"appearance":{"family":"future"}}}),
        ] {
            let mut invalid = v.clone();
            for (k, val) in patch.as_object().unwrap() {
                invalid[k] = val.clone();
            }
            assert!(
                import(&serde_json::to_vec(&invalid).unwrap()).is_err(),
                "{invalid}"
            );
        }
        v.as_object_mut().unwrap().remove("appearance_only");
        v["widget_style"] = json!({"theme":"teal"});
        v["dock"] = Value::Null;
        v["panel"] = Value::Null;
        v["appearance"] = Value::Null;
        v["version"] = 2.into();
        let old = import(&serde_json::to_vec(&v).unwrap()).unwrap();
        assert_eq!(old.written_version(), 2);
        assert!(
            serde_json::from_slice::<Value>(&export(&old).unwrap().bytes)
                .unwrap()
                .get("appearance_only")
                .is_none()
        );
    }
}
