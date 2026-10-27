//! The profile record (`desktop-cards-profile`, up to version 9).
//!
//! Version 2 adds the `windows` and `workspaces` sections and per-variant
//! window `gaps` under Appearance; version 3 adds `wallpaper` and
//! `desktop_icons` (`desktop_look`); version 4 adds the desktop tools'
//! widget members (Music, Notes and Focus visibility, positions and card
//! options). Version 5 adds optional layout dimensions (`widget_sizes`), with
//! null restoring automatic sizing. Older records (exports, library copies and the built-in
//! presets) stay readable exactly as they are: they keep their version in
//! memory and may not contain a later version's members. A record is
//! written as the oldest version that holds its members (5 with sizes, 4 with tool
//! members, 3 with version 3 sections, otherwise 2), so older apps keep
//! reading what they can.
//!
//! Transport comes in two kinds. Shared (portable) profiles — `export` and
//! `import`, the files people pass around and the built-in presets — never
//! carry this computer's own pictures, display names or arrangement:
//! `export` leaves them out and says so, `import` refuses them. Local
//! profiles in the library (`export_local`, `import_local`) keep them.
//!
//! Every section and every member is optional; absent means the profile
//! leaves that setting alone. Captures fill members with exact canonical
//! values; curated presets may use intent forms (corner and density names,
//! a glass percentage, `#hex` colors). Both become the same exact
//! per-variant values before a patch is built. The record has no commands,
//! namespaces, keys or versions, and paths only as local wallpaper
//! sources: COSMIC keys come only from the adapters' fixed tables, and
//! widget members only from its allowlist.
use crate::desktop::appearance::{self, ColorRole, Surface, ThemeVariant, VariantValues, theme};
use crate::desktop::panels::{self, Anchor, AutoHide, AutoHideBehavior, Background, PanelSize};
use crate::desktop::store::widget::{self, WidgetGroup};
use crate::desktop::store::{ConfigKey, DesktopError, RawValue, rule_for};
use crate::desktop::windows::{Behavior, TileBehavior};
use crate::desktop::workspaces::{self, TypingAction, WorkspaceLayout, WorkspaceMode};
use cosmic::config::FontConfig;
use cosmic::cosmic_theme::color::ColorReprOption;
use cosmic::cosmic_theme::palette::Srgba;
use cosmic::cosmic_theme::{AlphaMap, CornerRadii, Density, Roundness, Spacing};
use serde::de::{self, Deserializer};
use serde::ser::Serializer;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::fmt;

pub use super::desktop_look::{
    BackgroundLook, DesktopIconsLook, DisplaysName, FitName, GradientLook, OrderName, Rgb,
    SlideshowLook, SourceLook, WallpaperLook,
};

pub const FORMAT: &str = "desktop-cards-profile";
/// Frozen composition provenance requires version 10.
/// Clock face, weight, italic and surface choices retain version 9.
pub const VERSION: u32 = 16;
pub const VERSION_15: u32 = 15;
pub const VERSION_14: u32 = 14;
pub const VERSION_13: u32 = 13;
pub const VERSION_12: u32 = 12;
pub const VERSION_11: u32 = 11;
pub const VERSION_10: u32 = 10;
pub const VERSION_9: u32 = 9;
pub const VERSION_8: u32 = 8;
pub const VERSION_7: u32 = 7;
pub const VERSION_6: u32 = 6;
pub const VERSION_5: u32 = 5;
pub const VERSION_4: u32 = 4;
/// Written for records with version 3 sections and no tool members.
pub const VERSION_3: u32 = 3;
/// Written for records without version 3 sections.
pub const VERSION_2: u32 = 2;
/// The first format; still read, never written.
pub const LEGACY_VERSION: u32 = 1;
pub const MAX_PROFILE_BYTES: usize = 256 * 1024;
const MAX_NAME: usize = 80;
const SECTIONS_V1: [&str; 8] = [
    "format",
    "version",
    "name",
    "dock",
    "panel",
    "appearance",
    "widget_style",
    "widget_layout",
];
/// Sections only version 2 (and later) records have.
const SECTIONS_V2: [&str; 2] = ["windows", "workspaces"];
/// Sections only version 3 records have.
const SECTIONS_V3: [&str; 2] = ["wallpaper", "desktop_icons"];
/// What a shared profile can't import.
pub const LOCAL_ONLY: &str = "This profile refers to pictures or displays on another computer. Shared profiles can include built-in, COSMIC and color wallpapers.";

#[derive(Clone, Debug, PartialEq)]
pub enum ProfileError {
    /// Not a usable profile (malformed, out of range, too large).
    Invalid(String),
    /// A newer format version or a section this version does not support.
    Unsupported(String),
    NotFound(String),
    /// Reading or writing the library failed, or writes are disabled.
    Storage(String),
}

impl fmt::Display for ProfileError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Invalid(message)
            | Self::Unsupported(message)
            | Self::NotFound(message)
            | Self::Storage(message) => write!(f, "{message}"),
        }
    }
}

impl std::error::Error for ProfileError {}

impl From<DesktopError> for ProfileError {
    fn from(error: DesktopError) -> Self {
        Self::Storage(error.to_string())
    }
}

/// A part of the desktop a profile can include. The caller selects which
/// domains a plan uses; each COSMIC key and widget member belongs to one.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Domain {
    Dock,
    Panel,
    /// Theme, toolkit and compositor appearance, including window gaps.
    Appearance,
    /// Window behavior: tiling, focus, edge snapping and title-bar buttons.
    Windows,
    Workspaces,
    /// Backgrounds and, in local profiles, the display arrangement.
    Wallpaper,
    DesktopIcons,
    WidgetStyle,
    WidgetLayout,
}

impl Domain {
    pub const ALL: [Self; 9] = [
        Self::Dock,
        Self::Panel,
        Self::Appearance,
        Self::Windows,
        Self::Workspaces,
        Self::Wallpaper,
        Self::DesktopIcons,
        Self::WidgetStyle,
        Self::WidgetLayout,
    ];
    pub fn title(self) -> &'static str {
        match self {
            Self::Dock => "Dock",
            Self::Panel => "Panel",
            Self::Appearance => "Appearance",
            Self::Windows => "Windows",
            Self::Workspaces => "Workspaces",
            Self::Wallpaper => "Wallpaper",
            Self::DesktopIcons => "Desktop icons",
            Self::WidgetStyle => "Widget style",
            Self::WidgetLayout => "Widget layout",
        }
    }
    pub fn key(self) -> &'static str {
        match self {
            Self::Dock => "dock",
            Self::Panel => "panel",
            Self::Appearance => "appearance",
            Self::Windows => "windows",
            Self::Workspaces => "workspaces",
            Self::Wallpaper => "wallpaper",
            Self::DesktopIcons => "desktop_icons",
            Self::WidgetStyle => "widget_style",
            Self::WidgetLayout => "widget_layout",
        }
    }
    pub fn parse(value: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|domain| domain.key() == value)
    }
    pub(crate) fn widget_group(self) -> Option<WidgetGroup> {
        match self {
            Self::WidgetStyle => Some(WidgetGroup::Style),
            Self::WidgetLayout => Some(WidgetGroup::Layout),
            _ => None,
        }
    }
}

/// `Some(value)` for a member that is present, even when it is `null`, so
/// "no override" and "not specified" stay distinct.
fn present<'de, D, T>(deserializer: D) -> Result<Option<T>, D::Error>
where
    D: Deserializer<'de>,
    T: Deserialize<'de>,
{
    T::deserialize(deserializer).map(Some)
}

/// Applets in the three regions, by installed applet ID.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Applets {
    #[serde(default)]
    pub start: Vec<String>,
    #[serde(default)]
    pub center: Vec<String>,
    #[serde(default)]
    pub end: Vec<String>,
}

/// One Dock or Panel. `output` is deliberately absent: the target keeps
/// its own monitor binding.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PanelLook {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub island_gap: Option<u16>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub islands: Option<dock_islands_core::Settings>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub anchor: Option<Anchor>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub floating: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub margin: Option<u16>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub size: Option<PanelSize>,
    /// `null`: no per-region size override.
    #[serde(
        default,
        deserialize_with = "present",
        skip_serializing_if = "Option::is_none"
    )]
    pub size_wings: Option<Option<(Option<PanelSize>, Option<PanelSize>)>>,
    #[serde(
        default,
        deserialize_with = "present",
        skip_serializing_if = "Option::is_none"
    )]
    pub size_center: Option<Option<PanelSize>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub extend: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub autohide: Option<AutoHide>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub exclusive_zone: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub autohide_behavior: Option<AutoHideBehavior>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub opacity: Option<f32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub border_radius: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub spacing: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub padding: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub background: Option<Background>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub keep_style: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub applets: Option<Applets>,
}

impl PanelLook {
    /// `(panel key, encoded value)` for every present member except applets.
    pub(crate) fn values(&self) -> Result<Vec<(&'static str, RawValue)>, String> {
        let mut values = Vec::new();
        macro_rules! member {
            ($field:ident => $key:literal) => {
                if let Some(value) = &self.$field {
                    values.push((
                        $key,
                        RawValue::encode(value).map_err(|error| error.to_string())?,
                    ));
                }
            };
        }
        member!(anchor => "anchor");
        member!(floating => "anchor_gap");
        member!(margin => "margin");
        member!(size => "size");
        member!(size_wings => "size_wings");
        member!(size_center => "size_center");
        member!(extend => "expand_to_edges");
        member!(autohide => "autohide");
        member!(exclusive_zone => "exclusive_zone");
        member!(autohide_behavior => "autohide_behavior");
        member!(opacity => "opacity");
        member!(border_radius => "border_radius");
        member!(spacing => "spacing");
        member!(padding => "padding");
        member!(background => "background");
        member!(keep_style => "keep_style_on_maximize");
        Ok(values)
    }
    fn validate(&self, title: &str) -> Result<(), String> {
        if self.island_gap.is_some_and(|gap| gap > 64) {
            return Err("Island gap must be between 0 and 64 pixels".into());
        }
        if let Some(s) = &self.islands {
            s.validate()?;
        }
        for (name, value) in self.values()? {
            let rule = panels::RULES
                .iter()
                .find(|rule| rule.key == name)
                .ok_or_else(|| format!("{title}: unsupported setting {name}"))?;
            (rule.validate)(&value)
                .map_err(|message| format!("{title} {}: {message}", panels::key_label(name)))?;
        }
        if let Some(applets) = &self.applets {
            for list in [&applets.start, &applets.center, &applets.end] {
                if list.len() > 64
                    || list.iter().any(|id| {
                        id.is_empty() || id.len() > 255 || id.chars().any(char::is_control)
                    })
                {
                    return Err(format!("{title}: invalid applet list"));
                }
            }
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ModeName {
    Light,
    Dark,
    Automatic,
}

/// A color role: `"auto"` (derived), `"#RRGGBB"`/`"#RRGGBBAA"`, or exact
/// `[r, g, b, a]` components in 0–1 (captures that are not 8-bit exact).
#[derive(Clone, Debug, PartialEq)]
pub enum ColorChoice {
    Auto,
    Hex(String),
    Exact([f32; 4]),
}

impl ColorChoice {
    /// The color for `role`, or an error for a malformed value.
    pub fn color(&self, role: ColorRole) -> Result<Option<Srgba>, String> {
        match self {
            Self::Auto => Ok(None),
            Self::Hex(text) => appearance::parse_hex(text, role.alpha())
                .map(Some)
                .ok_or_else(|| format!("{}: invalid color {text}", role.title())),
            Self::Exact([r, g, b, a]) => {
                if [r, g, b, a]
                    .iter()
                    .all(|c| c.is_finite() && (0.0..=1.0).contains(*c))
                {
                    Ok(Some(Srgba::new(
                        *r,
                        *g,
                        *b,
                        if role.alpha() { *a } else { 1. },
                    )))
                } else {
                    Err(format!("{}: color out of range", role.title()))
                }
            }
        }
    }
    /// Hex when it round-trips exactly, otherwise the exact components.
    pub fn of(color: Option<Srgba>, role: ColorRole) -> Self {
        match color {
            None => Self::Auto,
            Some(color) => {
                let text = appearance::hex(color, role.alpha());
                if appearance::parse_hex(&text, role.alpha()) == Some(color) {
                    Self::Hex(text)
                } else {
                    Self::Exact([color.red, color.green, color.blue, color.alpha])
                }
            }
        }
    }
}

impl Serialize for ColorChoice {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        match self {
            Self::Auto => serializer.serialize_str("auto"),
            Self::Hex(text) => serializer.serialize_str(text),
            Self::Exact(components) => components.serialize(serializer),
        }
    }
}

impl<'de> Deserialize<'de> for ColorChoice {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        match Value::deserialize(deserializer)? {
            Value::String(text) if text == "auto" => Ok(Self::Auto),
            Value::String(text) if text.len() <= 9 => Ok(Self::Hex(text)),
            Value::Array(items) if items.len() == 3 || items.len() == 4 => {
                let mut components = [1.0f32; 4];
                for (slot, item) in components.iter_mut().zip(&items) {
                    *slot = item
                        .as_f64()
                        .ok_or_else(|| de::Error::custom("color components must be numbers"))?
                        as f32;
                }
                Ok(Self::Exact(components))
            }
            _ => Err(de::Error::custom(
                "a color is \"auto\", \"#RRGGBB\" or [r, g, b, a]",
            )),
        }
    }
}

/// A corner preset name or exact radii.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Corners {
    Preset(Roundness),
    Exact(CornerRadii),
}

/// A density name or exact spacing.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum SpacingChoice {
    Preset(Density),
    Exact(Spacing),
}

/// cosmic-settings' glass percentage or an exact alpha map.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Glass {
    Percent(u8),
    Exact(AlphaMap),
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FrostedLook {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub panel: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub applets: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub system_interface: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub windows: Option<bool>,
}

/// One theme variant's values. Light and Dark are independent.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VariantLook {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub accent: Option<ColorChoice>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub background: Option<ColorChoice>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub primary: Option<ColorChoice>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub secondary: Option<ColorChoice>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub outline: Option<ColorChoice>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub outline_width: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub corners: Option<Corners>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub spacing: Option<SpacingChoice>,
    /// Blur step 0–13, as cosmic-settings shows it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub blur: Option<u8>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub glass: Option<Glass>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub frosted: Option<FrostedLook>,
    /// Window gaps `[outer, inner]` in pixels (version 2). The inner gap is
    /// widened to the outline width when applied.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub gaps: Option<(u32, u32)>,
}

impl VariantLook {
    fn color_mut(&mut self, role: ColorRole) -> &mut Option<ColorChoice> {
        match role {
            ColorRole::Accent => &mut self.accent,
            ColorRole::Background => &mut self.background,
            ColorRole::Primary => &mut self.primary,
            ColorRole::Secondary => &mut self.secondary,
            ColorRole::Outline => &mut self.outline,
        }
    }
    fn color(&self, role: ColorRole) -> Option<&ColorChoice> {
        match role {
            ColorRole::Accent => self.accent.as_ref(),
            ColorRole::Background => self.background.as_ref(),
            ColorRole::Primary => self.primary.as_ref(),
            ColorRole::Secondary => self.secondary.as_ref(),
            ColorRole::Outline => self.outline.as_ref(),
        }
    }
    /// The exact builder values this look sets, validated against the same
    /// key rules the store applies.
    pub fn values(&self, variant: ThemeVariant) -> Result<VariantValues, String> {
        let mut values = VariantValues::default();
        for role in ColorRole::ALL {
            if let Some(choice) = self.color(role) {
                values.colors.insert(role, choice.color(role)?);
            }
        }
        values.outline_width = self.outline_width;
        values.radii = self.corners.as_ref().map(|corners| match corners {
            Corners::Preset(roundness) => CornerRadii::from(*roundness),
            Corners::Exact(radii) => *radii,
        });
        values.spacing = self.spacing.as_ref().map(|spacing| match spacing {
            SpacingChoice::Preset(density) => Spacing::from(*density),
            SpacingChoice::Exact(spacing) => *spacing,
        });
        values.blur = match self.blur {
            Some(step) => Some(
                appearance::blur_from_step(step)
                    .ok_or_else(|| format!("Blur step {step} is out of range"))?,
            ),
            None => None,
        };
        values.alpha = match &self.glass {
            Some(Glass::Percent(percent)) if *percent <= 100 => {
                Some(appearance::glass_alpha_map(*percent))
            }
            Some(Glass::Percent(percent)) => {
                return Err(format!("Glass {percent}% is out of range"));
            }
            Some(Glass::Exact(map)) => Some(*map),
            None => None,
        };
        if let Some(frosted) = &self.frosted {
            for (surface, on) in [
                (Surface::Panel, frosted.panel),
                (Surface::Applets, frosted.applets),
                (Surface::SystemInterface, frosted.system_interface),
                (Surface::Windows, frosted.windows),
            ] {
                // Legacy/imported setups may explicitly turn effects off,
                // but must not newly activate unvalidated native blur paths.
                if on == Some(false) {
                    values.frosted.insert(surface, false);
                }
            }
        }
        // Same validators as the store's builder key rules.
        let check = |name: &str, value: RawValue| -> Result<(), String> {
            let key = theme::builder_key(variant, name);
            check_key(&key, &value)
                .map_err(|message| format!("{} {name}: {message}", variant.title()))
        };
        for (role, color) in &values.colors {
            let name = match role {
                ColorRole::Accent => "accent",
                ColorRole::Background => "bg_color",
                ColorRole::Primary => "primary_container_bg",
                ColorRole::Secondary => "secondary_container_bg",
                ColorRole::Outline => "window_hint",
            };
            check(name, encode(&ColorReprOption::from(*color))?)?;
        }
        if let Some(width) = values.outline_width {
            check("active_hint", encode(&width)?)?;
        }
        if let Some(radii) = &values.radii {
            check("corner_radii", encode(radii)?)?;
        }
        if let Some(alpha) = &values.alpha {
            check("alpha_map", encode(alpha)?)?;
        }
        values.gaps = self.gaps;
        if let Some(gaps) = &values.gaps {
            check("gaps", encode(gaps)?)?;
        }
        Ok(values)
    }
    /// The look of a variant's current builder values, exactly.
    pub fn of(values: &VariantValues) -> Self {
        let mut look = Self::default();
        for (role, color) in &values.colors {
            *look.color_mut(*role) = Some(ColorChoice::of(*color, *role));
        }
        look.outline_width = values.outline_width;
        look.corners = values.radii.map(|radii| {
            let preset = Roundness::from(radii);
            if CornerRadii::from(preset) == radii {
                Corners::Preset(preset)
            } else {
                Corners::Exact(radii)
            }
        });
        look.spacing = values.spacing.map(|spacing| {
            [Density::Compact, Density::Standard, Density::Spacious]
                .into_iter()
                .find(|density| Spacing::from(*density) == spacing)
                .map_or(SpacingChoice::Exact(spacing), SpacingChoice::Preset)
        });
        look.blur = values.blur.map(|blur| blur as u8);
        look.glass = values.alpha.map(|map| {
            appearance::exact_glass_percent(&map).map_or(Glass::Exact(map), Glass::Percent)
        });
        if !values.frosted.is_empty() {
            let get = |surface| values.frosted.get(&surface).copied();
            look.frosted = Some(FrostedLook {
                panel: get(Surface::Panel),
                applets: get(Surface::Applets),
                system_interface: get(Surface::SystemInterface),
                windows: get(Surface::Windows),
            });
        }
        look.gaps = values.gaps;
        look
    }
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WindowLook {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub clip_floating: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub clip_tiled: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub shadow_tiled: Option<bool>,
}

/// The tiling scope, as profiles name it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TilingScope {
    /// Tiling applies to every workspace and rearranges existing windows.
    Global,
    /// Tiling is the default for new windows and workspaces.
    PerWorkspace,
}

impl From<TileBehavior> for TilingScope {
    fn from(value: TileBehavior) -> Self {
        match value {
            TileBehavior::Global => Self::Global,
            TileBehavior::PerWorkspace => Self::PerWorkspace,
        }
    }
}
impl From<TilingScope> for TileBehavior {
    fn from(value: TilingScope) -> Self {
        match value {
            TilingScope::Global => Self::Global,
            TilingScope::PerWorkspace => Self::PerWorkspace,
        }
    }
}

/// Window behavior (version 2). Member names are COSMIC's own keys. Window
/// gaps are theme values and live under Appearance.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WindowsBehaviorLook {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub autotile: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub autotile_behavior: Option<TilingScope>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub active_hint: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub focus_follows_cursor: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cursor_follows_focus: Option<bool>,
    /// Milliseconds.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub focus_follows_cursor_delay: Option<u64>,
    /// Pixels; 0 turns edge snapping off.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub edge_snap_threshold: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub show_minimize: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub show_maximize: Option<bool>,
}

impl WindowsBehaviorLook {
    /// `(behavior, encoded value)` for every present member.
    pub(crate) fn values(&self) -> Result<Vec<(Behavior, RawValue)>, String> {
        let mut values = Vec::new();
        let flags = [
            (Behavior::Autotile, self.autotile),
            (Behavior::ActiveHint, self.active_hint),
            (Behavior::FocusFollowsCursor, self.focus_follows_cursor),
            (Behavior::CursorFollowsFocus, self.cursor_follows_focus),
            (Behavior::ShowMinimize, self.show_minimize),
            (Behavior::ShowMaximize, self.show_maximize),
        ];
        for (behavior, value) in flags {
            if let Some(on) = value {
                values.push((behavior, encode(&on)?));
            }
        }
        if let Some(scope) = self.autotile_behavior {
            values.push((Behavior::Scope, encode(&TileBehavior::from(scope))?));
        }
        if let Some(delay) = self.focus_follows_cursor_delay {
            values.push((Behavior::FocusDelay, encode(&delay)?));
        }
        if let Some(distance) = self.edge_snap_threshold {
            values.push((Behavior::EdgeSnap, encode(&distance)?));
        }
        values.sort_by_key(|(behavior, _)| *behavior);
        Ok(values)
    }
    fn validate(&self) -> Result<(), String> {
        for (behavior, value) in self.values()? {
            check_key(&behavior.key(), &value)
                .map_err(|message| format!("Windows {}: {message}", behavior.title()))?;
        }
        Ok(())
    }
}

/// Workspaces (version 2). Member names and values are COSMIC's own.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WorkspacesLook {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub workspace_mode: Option<WorkspaceModeName>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub workspace_layout: Option<WorkspaceLayoutName>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub action_on_typing: Option<TypingActionName>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub workspace_wraparound: Option<bool>,
}

macro_rules! names {
    ($name:ident <=> $native:ty { $($variant:ident),+ $(,)? }) => {
        #[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
        #[serde(rename_all = "snake_case")]
        pub enum $name { $($variant),+ }
        impl From<$native> for $name {
            fn from(value: $native) -> Self {
                match value { $(<$native>::$variant => Self::$variant),+ }
            }
        }
        impl From<$name> for $native {
            fn from(value: $name) -> Self {
                match value { $($name::$variant => Self::$variant),+ }
            }
        }
    };
}
names!(WorkspaceModeName <=> WorkspaceMode { OutputBound, Global });
names!(WorkspaceLayoutName <=> WorkspaceLayout { Vertical, Horizontal });
names!(TypingActionName <=> TypingAction { None, OpenLauncher, OpenApplications });

impl WorkspacesLook {
    pub fn is_empty(&self) -> bool {
        *self == Self::default()
    }
    fn validate(&self) -> Result<(), String> {
        let mut config = workspaces::WorkspaceConfig::default();
        if let Some(mode) = self.workspace_mode {
            config.workspace_mode = mode.into();
        }
        if let Some(layout) = self.workspace_layout {
            config.workspace_layout = layout.into();
        }
        if let Some(action) = self.action_on_typing {
            config.action_on_typing = action.into();
        }
        if let Some(wrap) = self.workspace_wraparound {
            config.workspace_wraparound = wrap;
        }
        check_key(&workspaces::key(), &encode(&config)?)
            .map_err(|message| format!("Workspaces: {message}"))
    }
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AppearanceLook {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mode: Option<ModeName>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub interface_density: Option<Density>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub header_size: Option<Density>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub interface_font: Option<FontConfig>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub monospace_font: Option<FontConfig>,
    /// An installed icon theme's directory name.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub icon_theme: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub window: Option<WindowLook>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub light: Option<VariantLook>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dark: Option<VariantLook>,
}

impl AppearanceLook {
    pub fn variant(&self, variant: ThemeVariant) -> Option<&VariantLook> {
        match variant {
            ThemeVariant::Light => self.light.as_ref(),
            ThemeVariant::Dark => self.dark.as_ref(),
        }
    }
    fn validate(&self) -> Result<(), String> {
        let toolkit = |name: &str| {
            ConfigKey::new(appearance::TOOLKIT, 1, name).expect("toolkit keys are valid")
        };
        for (name, font) in [
            ("interface_font", &self.interface_font),
            ("monospace_font", &self.monospace_font),
        ] {
            if let Some(font) = font {
                check_key(&toolkit(name), &encode(font)?)
                    .map_err(|message| format!("Appearance font: {message}"))?;
            }
        }
        if let Some(name) = &self.icon_theme {
            check_key(&toolkit("icon_theme"), &encode(name)?)
                .map_err(|message| format!("Appearance icon theme: {message}"))?;
        }
        for variant in [ThemeVariant::Light, ThemeVariant::Dark] {
            if let Some(look) = self.variant(variant) {
                look.values(variant)?;
            }
        }
        Ok(())
    }
}

fn encode<T: Serialize>(value: &T) -> Result<RawValue, String> {
    RawValue::encode(value).map_err(|error| error.to_string())
}

fn check_key(key: &ConfigKey, value: &RawValue) -> Result<(), String> {
    let rule = rule_for(key).ok_or_else(|| format!("{key} is not editable"))?;
    (rule.validate)(value)
}

/// A complete profile record.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Profile {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub package: Option<super::packages::Origin>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub composition: Option<super::mix::Provenance>,
    #[serde(default, skip_serializing_if = "is_false")]
    pub appearance_only: bool,
    pub format: String,
    pub version: u32,
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dock: Option<PanelLook>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub panel: Option<PanelLook>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub appearance: Option<AppearanceLook>,
    /// Config-shaped subset of the widget style allowlist.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub widget_style: Option<Value>,
    /// Config-shaped subset of the widget layout allowlist.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub widget_layout: Option<Value>,
    /// Window behavior (version 2).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub windows: Option<WindowsBehaviorLook>,
    /// Workspaces (version 2).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub workspaces: Option<WorkspacesLook>,
    /// Wallpaper (version 3).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub wallpaper: Option<WallpaperLook>,
    /// Desktop icons (version 3).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub desktop_icons: Option<DesktopIconsLook>,
}

/// A profile name: trimmed, 1–80 characters, no control characters.
pub fn valid_name(name: &str) -> bool {
    let trimmed = name.trim();
    !trimmed.is_empty()
        && trimmed == name
        && name.chars().count() <= MAX_NAME
        && !name.chars().any(char::is_control)
}

fn is_false(value: &bool) -> bool {
    !*value
}

impl Profile {
    /// An empty profile; `None` for an invalid name.
    pub fn new(name: &str) -> Option<Self> {
        valid_name(name).then(|| Self {
            package: None,
            composition: None,
            appearance_only: false,
            format: FORMAT.into(),
            version: VERSION,
            name: name.to_string(),
            dock: None,
            panel: None,
            appearance: None,
            widget_style: None,
            widget_layout: None,
            windows: None,
            workspaces: None,
            wallpaper: None,
            desktop_icons: None,
        })
    }
    /// Domains this record has a section for.
    pub fn domains(&self) -> Vec<Domain> {
        Domain::ALL
            .into_iter()
            .filter(|domain| self.has(*domain))
            .collect()
    }
    pub fn has(&self, domain: Domain) -> bool {
        match domain {
            Domain::Dock => self.dock.is_some(),
            Domain::Panel => self.panel.is_some(),
            Domain::Appearance => self.appearance.is_some(),
            Domain::Windows => self.windows.is_some(),
            Domain::Workspaces => self.workspaces.is_some(),
            Domain::Wallpaper => self.wallpaper.is_some(),
            Domain::DesktopIcons => self.desktop_icons.is_some(),
            Domain::WidgetStyle => self.widget_style.is_some(),
            Domain::WidgetLayout => self.widget_layout.is_some(),
        }
    }
    pub(crate) fn widget_section(&self, group: WidgetGroup) -> Option<&Value> {
        match group {
            WidgetGroup::Style => self.widget_style.as_ref(),
            WidgetGroup::Layout => self.widget_layout.as_ref(),
        }
    }
    /// The first member only version 2 records may have, if any.
    fn version_2_member(&self) -> Option<&'static str> {
        if self.windows.is_some() {
            return Some("windows");
        }
        if self.workspaces.is_some() {
            return Some("workspaces");
        }
        let appearance = self.appearance.as_ref()?;
        [appearance.light.as_ref(), appearance.dark.as_ref()]
            .into_iter()
            .flatten()
            .any(|look| look.gaps.is_some())
            .then_some("gaps")
    }
    /// The first member only version 3 records may have, if any.
    fn version_3_member(&self) -> Option<&'static str> {
        if self.wallpaper.is_some() {
            return Some("wallpaper");
        }
        self.desktop_icons.is_some().then_some("desktop_icons")
    }
    /// The first desktop tool member, which only version 4 records may
    /// have, if any.
    fn version_4_member(&self) -> Option<&'static str> {
        [WidgetGroup::Style, WidgetGroup::Layout]
            .into_iter()
            .filter_map(|group| self.widget_section(group).map(|section| (group, section)))
            .flat_map(|(group, section)| widget_members(section, group).unwrap_or_default())
            .find(|(pointer, _)| widget::tool_of(pointer).is_some())
            .map(|(pointer, _)| widget::field(pointer).map_or(pointer, |field| field.label))
    }
    /// The version this record is written as.
    pub fn written_version(&self) -> u32 {
        if self.has_extended_recipes() {
            return VERSION;
        }
        if let Some(package) = &self.package {
            return if package.ancestors.is_empty() {
                VERSION_14
            } else {
                VERSION_15
            };
        }
        if self.has_recipes() {
            return VERSION_13;
        }
        if self.has_moments() {
            return VERSION_12;
        }
        if self.has_depth() {
            return VERSION_11;
        }
        if self.composition.is_some() {
            return VERSION_10;
        }
        if self.has_clock_presentation() {
            return VERSION_9;
        }
        if self.has_materials() {
            return VERSION_8;
        }
        if self.appearance_only || self.has_theme_members() {
            return VERSION_7;
        }
        if self.has_islands() {
            return VERSION_6;
        }
        if self
            .widget_layout
            .as_ref()
            .is_some_and(|layout| layout.get("widget_sizes").is_some())
        {
            VERSION_5
        } else if self.version_4_member().is_some() {
            VERSION_4
        } else if self.version_3_member().is_some() {
            VERSION_3
        } else {
            VERSION_2
        }
    }
    /// The first member a shared profile may not have: a personal picture
    /// or folder, display names or the display arrangement.
    pub fn local_member(&self) -> Option<&'static str> {
        if self.composition.as_ref().is_some_and(|p| p.has_local_ids()) {
            return Some("local source IDs");
        }
        if [&self.dock, &self.panel]
            .into_iter()
            .flatten()
            .any(|l| l.islands.as_ref().is_some_and(|s| !s.outputs.is_empty()))
        {
            return Some("display-specific island positions");
        }
        self.wallpaper
            .as_ref()
            .and_then(WallpaperLook::local_member)
    }
    /// The record as a shared profile has it, and what was left out (see
    /// `WallpaperLook::strip_local`). A wallpaper section left empty goes.
    pub fn portable(&self) -> (Self, Vec<&'static str>) {
        let mut profile = self.clone();
        let mut omitted = Vec::new();
        if let Some(composition) = &mut profile.composition {
            if composition.strip_local_ids() {
                omitted.push("local source IDs");
            }
        }
        if let Some(look) = &mut profile.wallpaper {
            omitted.extend(look.strip_local());
            if look.is_empty() && !omitted.is_empty() {
                profile.wallpaper = None;
            }
        }
        for look in [&mut profile.dock, &mut profile.panel]
            .into_iter()
            .flatten()
        {
            if let Some(s) = &mut look.islands
                && !s.outputs.is_empty()
            {
                s.outputs.clear();
                if !omitted.contains(&"display-specific island positions") {
                    omitted.push("display-specific island positions");
                }
            }
        }
        (profile, omitted)
    }
    fn has_extended_recipes(&self) -> bool {
        self.widget_style
            .as_ref()
            .and_then(|v| v.get("recipes"))
            .is_some_and(|v| v.get("calendar").is_some() || v.get("devbox").is_some())
    }
    fn has_recipes(&self) -> bool {
        self.widget_style
            .as_ref()
            .and_then(|v| v.get("recipes"))
            .is_some()
    }
    fn has_moments(&self) -> bool {
        self.widget_style
            .as_ref()
            .and_then(|v| v.get("moments"))
            .is_some()
    }
    fn has_depth(&self) -> bool {
        self.widget_style
            .as_ref()
            .and_then(|v| v.pointer("/clock/depth"))
            .is_some()
    }
    fn has_clock_presentation(&self) -> bool {
        self.widget_style
            .as_ref()
            .and_then(|v| v.get("clock"))
            .is_some_and(|clock| {
                ["face", "weight", "italic", "surface"]
                    .iter()
                    .any(|key| clock.get(key).is_some())
            })
    }
    fn has_materials(&self) -> bool {
        self.widget_style
            .as_ref()
            .is_some_and(|v| v.get("materials").is_some() || v.get("material_motion").is_some())
    }
    fn has_theme_members(&self) -> bool {
        [&self.dock, &self.panel]
            .into_iter()
            .flatten()
            .any(|b| b.island_gap.is_some())
            || self.widget_style.as_ref().is_some_and(|v| {
                [
                    "/appearance/family",
                    "/appearance/surface",
                    "/appearance/wallpaper_light",
                ]
                .iter()
                .any(|p| v.pointer(p).is_some())
            })
    }
    fn has_islands(&self) -> bool {
        [&self.dock, &self.panel]
            .into_iter()
            .flatten()
            .any(|l| l.islands.is_some())
    }
    /// Checks everything before the record is stored or used.
    pub fn validate(&self) -> Result<(), ProfileError> {
        if self.format != FORMAT {
            return Err(ProfileError::Invalid("Not a Halcyon profile.".into()));
        }
        if !(LEGACY_VERSION..=VERSION).contains(&self.version) {
            return Err(ProfileError::Unsupported(format!(
                "This profile uses format version {}; this version of Halcyon reads versions {LEGACY_VERSION} to {VERSION}.",
                self.version
            )));
        }
        if self.version == LEGACY_VERSION
            && let Some(member) = self.version_2_member()
        {
            return Err(ProfileError::Invalid(format!(
                "This version 1 profile includes “{member}”, which only version 2 profiles have."
            )));
        }
        if self.version < VERSION_3
            && let Some(member) = self.version_3_member()
        {
            return Err(ProfileError::Invalid(format!(
                "This version {} profile includes “{member}”, which only version 3 profiles have.",
                self.version
            )));
        }
        if self.version < VERSION_4
            && let Some(member) = self.version_4_member()
        {
            return Err(ProfileError::Invalid(format!(
                "This version {} profile includes “{member}”, which only version 4 profiles have.",
                self.version
            )));
        }
        if self.version < VERSION_5
            && self
                .widget_layout
                .as_ref()
                .is_some_and(|v| v.get("widget_sizes").is_some())
        {
            return Err(ProfileError::Invalid(
                "Widget sizes require profile version 5.".into(),
            ));
        }
        if self.has_islands() && self.version < VERSION_6 {
            return Err(ProfileError::Invalid(
                "Dock Islands requires profile version 6".into(),
            ));
        }
        if (self.appearance_only || self.has_theme_members()) && self.version < VERSION_7 {
            return Err(ProfileError::Invalid(
                "Shared themes require profile version 7".into(),
            ));
        }
        if self.has_materials() && self.version < VERSION_8 {
            return Err(ProfileError::Invalid(
                "Materials require profile version 8".into(),
            ));
        }
        if let Some(composition) = &self.composition {
            if self.version < VERSION_10 {
                return Err(ProfileError::Invalid(
                    "Composition requires profile version 10".into(),
                ));
            }
            composition.validate().map_err(ProfileError::Invalid)?;
        }
        if let Some(package) = &self.package {
            if self.version < VERSION_14 {
                return Err(ProfileError::Invalid(
                    "Package credits require profile version 14".into(),
                ));
            }
            package.validate().map_err(ProfileError::Invalid)?;
            if !package.ancestors.is_empty() && self.version < VERSION_15 {
                return Err(ProfileError::Invalid(
                    "Remix credits require profile version 15".into(),
                ));
            }
        }
        if self.has_extended_recipes() && self.version < VERSION {
            return Err(ProfileError::Invalid(
                "Calendar recipes require profile version 16".into(),
            ));
        }
        if self.has_recipes() && self.version < VERSION_13 {
            return Err(ProfileError::Invalid(
                "Personal recipes require profile version 13".into(),
            ));
        }
        if self.has_moments() && self.version < VERSION_12 {
            return Err(ProfileError::Invalid(
                "Moments require profile version 12".into(),
            ));
        }
        if self.has_depth() && self.version < VERSION_11 {
            return Err(ProfileError::Invalid(
                "Clock depth requires profile version 11".into(),
            ));
        }
        if self.has_clock_presentation() && self.version < VERSION_9 {
            return Err(ProfileError::Invalid(
                "Clock presentation choices require profile version 9".into(),
            ));
        }
        if self.appearance_only {
            super::themes::validate(self).map_err(ProfileError::Invalid)?;
        }
        if !valid_name(&self.name) {
            return Err(ProfileError::Invalid(
                "Profile names have 1–80 characters and no line breaks.".into(),
            ));
        }
        let invalid = ProfileError::Invalid;
        if let Some(look) = &self.dock {
            look.validate("Dock").map_err(invalid)?;
        }
        if let Some(look) = &self.panel {
            look.validate("Panel").map_err(invalid)?;
        }
        if let Some(look) = &self.appearance {
            look.validate().map_err(invalid)?;
        }
        if let Some(look) = &self.windows {
            look.validate().map_err(invalid)?;
        }
        if let Some(look) = &self.workspaces {
            look.validate().map_err(invalid)?;
        }
        if let Some(look) = &self.wallpaper {
            look.validate().map_err(invalid)?;
        }
        if let Some(look) = &self.desktop_icons {
            look.validate().map_err(invalid)?;
        }
        for group in [WidgetGroup::Style, WidgetGroup::Layout] {
            if let Some(section) = self.widget_section(group) {
                widget_members(section, group).map_err(invalid)?;
            }
        }
        Ok(())
    }
}

/// Flattens a config-shaped widget section into allowlisted members of
/// `group`: `(pointer, value)`, where `None` (from `null`) removes a widget
/// position so the default applies. Anything else is refused by name.
pub(crate) fn widget_members(
    section: &Value,
    group: WidgetGroup,
) -> Result<Vec<(&'static str, Option<Value>)>, String> {
    fn walk(
        value: &Value,
        pointer: &str,
        group: WidgetGroup,
        members: &mut Vec<(&'static str, Option<Value>)>,
    ) -> Result<(), String> {
        let name = || pointer.trim_start_matches('/').replace('/', ".");
        if let Some(field) = widget::FIELDS
            .iter()
            .find(|field| field.pointer == pointer && field.group == group)
        {
            let removable =
                pointer.starts_with("/widget_positions/") || pointer.starts_with("/widget_sizes/");
            return match value {
                Value::Null if removable => {
                    members.push((field.pointer, None));
                    Ok(())
                }
                value if field.accepts(value) => {
                    members.push((field.pointer, Some(value.clone())));
                    Ok(())
                }
                _ => Err(format!("Invalid value for {}", field.label)),
            };
        }
        let prefix = format!("{pointer}/");
        let parent = pointer.is_empty()
            || widget::fields(group).any(|field| field.pointer.starts_with(&prefix));
        match value.as_object() {
            Some(object) if parent => {
                for (key, member) in object {
                    if key.is_empty() || key.contains('/') || key.contains('~') {
                        return Err("Unsupported widget setting".into());
                    }
                    walk(member, &format!("{pointer}/{key}"), group, members)?;
                }
                Ok(())
            }
            _ => Err(format!("Unsupported widget setting “{}”", name())),
        }
    }
    if !section.is_object() {
        return Err("Widget settings must be an object".into());
    }
    let mut members = Vec::new();
    walk(section, "", group, &mut members)?;
    Ok(members)
}

/// Parses a shared profile (a file the caller chose, or a preset) without
/// side effects. Journals, Undo records and anything else are refused, as
/// are newer format versions, unknown sections and members only local
/// profiles may have, each with a reason.
pub fn import(bytes: &[u8]) -> Result<Profile, ProfileError> {
    let profile = import_local(bytes)?;
    if profile.local_member().is_some() {
        return Err(ProfileError::Invalid(LOCAL_ONLY.into()));
    }
    Ok(profile)
}

/// Parses a profile from this computer's library: like `import`, but it
/// may refer to this computer's pictures and displays.
pub fn import_local(bytes: &[u8]) -> Result<Profile, ProfileError> {
    if bytes.len() > MAX_PROFILE_BYTES {
        return Err(ProfileError::Invalid("This profile is too large.".into()));
    }
    let value: Value = serde_json::from_slice(bytes)
        .map_err(|_| ProfileError::Invalid("This file is not a Halcyon profile.".into()))?;
    let object = value
        .as_object()
        .ok_or_else(|| ProfileError::Invalid("This file is not a Halcyon profile.".into()))?;
    match object.get("format").and_then(Value::as_str) {
        Some(FORMAT) => {}
        Some("desktop-cards-journal" | "desktop-cards-undo") => {
            return Err(ProfileError::Invalid(
                "This is a Halcyon recovery record, not a profile.".into(),
            ));
        }
        _ => {
            return Err(ProfileError::Invalid(
                "This file is not a Halcyon profile.".into(),
            ));
        }
    }
    let version = match object.get("version").and_then(Value::as_u64) {
        Some(version) if (u64::from(LEGACY_VERSION)..=u64::from(VERSION)).contains(&version) => {
            version as u32
        }
        Some(version) if version > u64::from(VERSION) => {
            return Err(ProfileError::Unsupported(format!(
                "This profile was made by a newer version of Halcyon (format {version})."
            )));
        }
        _ => {
            return Err(ProfileError::Invalid(
                "This profile has no supported format version.".into(),
            ));
        }
    };
    // The version a section first appears in.
    let since = |key: &str| {
        if key == "package" {
            Some(VERSION_14)
        } else if key == "composition" {
            Some(VERSION_10)
        } else if key == "appearance_only" {
            Some(VERSION_7)
        } else if SECTIONS_V1.contains(&key) {
            Some(LEGACY_VERSION)
        } else if SECTIONS_V2.contains(&key) {
            Some(VERSION_2)
        } else if SECTIONS_V3.contains(&key) {
            Some(VERSION_3)
        } else {
            None
        }
    };
    for key in object.keys() {
        match since(key) {
            None => {
                return Err(ProfileError::Unsupported(format!(
                    "This profile includes “{}”, which this version of Halcyon does not support.",
                    key.chars().take(64).collect::<String>()
                )));
            }
            Some(first) if first > version => {
                return Err(ProfileError::Invalid(format!(
                    "This version {version} profile includes “{key}”, which only version {first} profiles have."
                )));
            }
            Some(_) => {}
        }
    }
    let profile: Profile = serde_json::from_value(value)
        .map_err(|error| ProfileError::Invalid(format!("This profile is not valid: {error}")))?;
    profile.validate()?;
    Ok(profile)
}

/// A shared profile's bytes and what was kept on this computer.
#[derive(Clone, Debug, PartialEq)]
pub struct Exported {
    pub bytes: Vec<u8>,
    /// What a shared profile can't carry, as the notice names it (“your own
    /// wallpaper”, “display-specific backgrounds”, “the display
    /// arrangement”); empty when nothing was left out.
    pub omitted: Vec<&'static str>,
}

/// The shared bytes of `profile`: its typed record without anything local
/// (`Profile::portable`), pretty-printed, as the oldest version that holds
/// what remains (`Profile::written_version`).
pub fn export(profile: &Profile) -> Result<Exported, ProfileError> {
    profile.validate()?;
    let (portable, omitted) = profile.portable();
    Ok(Exported {
        bytes: encode_record(portable)?,
        omitted,
    })
}

/// The library bytes of `profile`: everything, also its own pictures and
/// displays, with the same version rule as `export`.
pub fn export_local(profile: &Profile) -> Result<Vec<u8>, ProfileError> {
    profile.validate()?;
    encode_record(profile.clone())
}

fn encode_record(mut profile: Profile) -> Result<Vec<u8>, ProfileError> {
    profile.version = profile.written_version();
    let mut bytes = serde_json::to_vec_pretty(&profile)
        .map_err(|error| ProfileError::Invalid(error.to_string()))?;
    bytes.push(b'\n');
    if bytes.len() > MAX_PROFILE_BYTES {
        return Err(ProfileError::Invalid("This profile is too large.".into()));
    }
    Ok(bytes)
}

#[cfg(test)]
mod frosted_panel_tests {
    use super::*;

    #[test]
    fn setups_can_disable_but_never_enable_native_frosting() {
        for variant in ThemeVariant::ALL {
            for on in [false, true] {
                let look: VariantLook = serde_json::from_value(serde_json::json!({
                    "frosted": {
                        "panel": on, "applets": on,
                        "system_interface": on, "windows": on
                    }
                }))
                .unwrap();
                let values = look.values(variant).unwrap();
                for surface in Surface::ALL {
                    assert_eq!(
                        values.frosted.get(&surface),
                        if on { None } else { Some(&false) }
                    );
                }
            }
        }
    }
}
