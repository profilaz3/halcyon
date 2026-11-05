//! System appearance: the COSMIC light and dark themes, the theme mode, the
//! toolkit's density, fonts and icon theme, and the compositor's window
//! clipping and shadows.
//!
//! Sources (pinned): libcosmic dc11d6a `cosmic-theme` ThemeBuilder/Theme v2
//! and ThemeMode v1, `src/config` CosmicTk v1; cosmic-comp 4d14528
//! `cosmic-comp-config` AppearanceConfig (`com.system76.CosmicComp/v1`).
//!
//! An `AppearanceDraft` keeps the snapshot it started from and one logical
//! edit per control. The `DesktopPatch` is derived from that base when it is
//! needed, so the derived theme roles always match the inputs Apply checks:
//! every key the derivation writes is a patch entry and every input it reads
//! is a guard. Nothing here writes files.
pub mod catalog;
pub mod theme;

use super::panels::Setting;
use super::store::{
    ConfigKey, DesktopError, DesktopPatch, DesktopStore, KeyRule, Namespace, RawValue, Snapshot,
};
use cosmic::config::FontConfig;
use cosmic::cosmic_theme::color::{ColorRepr, ColorReprOption};
use cosmic::cosmic_theme::palette::{Srgb, Srgba};
use cosmic::cosmic_theme::{
    AlphaMap, BlurStrength, Component, Container, CornerRadii, CosmicPaletteInner, Density,
    Roundness, Spacing, ThemeBuilder,
};
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
pub use theme::{Blocked, ThemeVariant};
use theme::{builder_key, theme_key};

pub const MODE: &str = "com.system76.CosmicTheme.Mode";
pub const TOOLKIT: &str = "com.system76.CosmicTk";
pub const COMPOSITOR: &str = "com.system76.CosmicComp";
pub const MAX_OUTLINE: u32 = 64;
pub const MAX_GAP: u32 = 500;

/// Decodes `raw` as `T` and refuses values with members `T` would silently
/// drop (a newer field, say), so they are never reserialized without them.
pub fn decode_exact<T: DeserializeOwned + Serialize>(raw: &RawValue) -> Result<T, String> {
    let value: T = raw.decode()?;
    let original: ron::Value = raw.decode()?;
    let encoded = RawValue::encode(&value).map_err(|error| error.to_string())?;
    let encoded: ron::Value = encoded.decode()?;
    if unknown_members(&original, &encoded) {
        return Err("contains settings this version does not recognize".into());
    }
    Ok(value)
}

/// Whether `raw` has a struct member, at any depth, that `typed` lacks.
/// Shapes that differ in representation (a hex string for a color struct)
/// are compared no further.
fn unknown_members(raw: &ron::Value, typed: &ron::Value) -> bool {
    match (raw, typed) {
        (ron::Value::Map(raw), ron::Value::Map(typed)) => {
            raw.iter().any(|(key, value)| match typed.get(key) {
                Some(other) => unknown_members(value, other),
                None => true,
            })
        }
        (ron::Value::Seq(raw), ron::Value::Seq(typed)) => {
            raw.len() != typed.len()
                || raw
                    .iter()
                    .zip(typed)
                    .any(|(raw, typed)| unknown_members(raw, typed))
        }
        (ron::Value::Option(Some(raw)), ron::Value::Option(Some(typed))) => {
            unknown_members(raw, typed)
        }
        _ => false,
    }
}

/// Theme mode: COSMIC's `is_dark` and `auto_switch` (sunrise/sunset).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum SystemMode {
    Light,
    Dark,
    Automatic,
}

impl SystemMode {
    pub const ALL: [Self; 3] = [Self::Light, Self::Dark, Self::Automatic];
    pub fn title(self) -> &'static str {
        match self {
            Self::Light => "Light",
            Self::Dark => "Dark",
            Self::Automatic => "Automatic",
        }
    }
}

/// Per-variant colors, each a builder override (`None` = derived).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum ColorRole {
    Accent,
    Background,
    Primary,
    Secondary,
    Outline,
}

impl ColorRole {
    pub const ALL: [Self; 5] = [
        Self::Accent,
        Self::Background,
        Self::Primary,
        Self::Secondary,
        Self::Outline,
    ];
    pub fn title(self) -> &'static str {
        match self {
            Self::Accent => "Accent",
            Self::Background => "Window background",
            Self::Primary => "Primary surface",
            Self::Secondary => "Secondary surface",
            Self::Outline => "Active window outline",
        }
    }
    pub fn key(self) -> &'static str {
        match self {
            Self::Accent => "accent",
            Self::Background => "background",
            Self::Primary => "primary",
            Self::Secondary => "secondary",
            Self::Outline => "outline",
        }
    }
    pub fn config_key(self, variant: ThemeVariant) -> ConfigKey {
        builder_key(
            variant,
            match self {
                Self::Accent => "accent",
                Self::Background => "bg_color",
                Self::Primary => "primary_container_bg",
                Self::Secondary => "secondary_container_bg",
                Self::Outline => "window_hint",
            },
        )
    }
    pub fn parse(value: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|role| role.key() == value)
    }
    /// Surfaces store an alpha channel; accent and outline do not.
    pub fn alpha(self) -> bool {
        matches!(self, Self::Background | Self::Primary | Self::Secondary)
    }
    fn get(self, builder: &ThemeBuilder) -> Option<Srgba> {
        let rgb = |color: Option<Srgb>| color.map(|c| Srgba::new(c.red, c.green, c.blue, 1.));
        match self {
            Self::Accent => rgb(builder.accent),
            Self::Background => builder.bg_color,
            Self::Primary => builder.primary_container_bg,
            Self::Secondary => builder.secondary_container_bg,
            Self::Outline => rgb(builder.window_hint),
        }
    }
    fn set(self, builder: &mut ThemeBuilder, color: Option<Srgba>) {
        let rgb = color.map(|c| Srgb::new(c.red, c.green, c.blue));
        match self {
            Self::Accent => builder.accent = rgb,
            Self::Background => builder.bg_color = color,
            Self::Primary => builder.primary_container_bg = color,
            Self::Secondary => builder.secondary_container_bg = color,
            Self::Outline => builder.window_hint = rgb,
        }
    }
}

/// `#RRGGBB` or, for surfaces, `#RRGGBBAA`, quantized like COSMIC's hex
/// encoding so a value round-trips unchanged.
pub fn parse_hex(input: &str, alpha: bool) -> Option<Srgba> {
    let hex = input.trim().trim_start_matches('#');
    if !hex.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return None;
    }
    let byte = |index: usize| u8::from_str_radix(hex.get(index..index + 2)?, 16).ok();
    let a = match (hex.len(), alpha) {
        (6, _) => 255,
        (8, true) => byte(6)?,
        _ => return None,
    };
    Some(Srgba::<u8>::new(byte(0)?, byte(2)?, byte(4)?, a).into_format())
}
pub fn hex(color: Srgba, alpha: bool) -> String {
    let c: Srgba<u8> = color.into_format();
    if alpha && c.alpha != 255 {
        format!("#{:02X}{:02X}{:02X}{:02X}", c.red, c.green, c.blue, c.alpha)
    } else {
        format!("#{:02X}{:02X}{:02X}", c.red, c.green, c.blue)
    }
}
fn quantize(color: Srgba, alpha: bool) -> Srgba {
    let mut c: Srgba<u8> = color.into_format();
    if !alpha {
        c.alpha = 255;
    }
    c.into_format()
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum FontRole {
    Interface,
    Monospace,
}

impl FontRole {
    pub const ALL: [Self; 2] = [Self::Interface, Self::Monospace];
    pub fn key(self) -> &'static str {
        match self {
            Self::Interface => "interface_font",
            Self::Monospace => "monospace_font",
        }
    }
    pub fn title(self) -> &'static str {
        match self {
            Self::Interface => "Interface font",
            Self::Monospace => "Fixed-width font",
        }
    }
    pub fn monospace(self) -> bool {
        self == Self::Monospace
    }
    fn default(self) -> FontConfig {
        let defaults = cosmic::config::CosmicTk::default();
        match self {
            Self::Interface => defaults.interface_font,
            Self::Monospace => defaults.monospace_font,
        }
    }
}

/// Frosted (blurred) surfaces.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum Surface {
    Panel,
    Applets,
    SystemInterface,
    Windows,
}

impl Surface {
    pub const ALL: [Self; 4] = [
        Self::Panel,
        Self::Applets,
        Self::SystemInterface,
        Self::Windows,
    ];
    pub fn key(self) -> &'static str {
        match self {
            Self::Panel => "frosted_panel",
            Self::Applets => "frosted_applets",
            Self::SystemInterface => "frosted_system_interface",
            Self::Windows => "frosted_windows",
        }
    }
    pub fn title(self) -> &'static str {
        match self {
            Self::Panel => "Panel and dock",
            Self::Applets => "Applet popups",
            Self::SystemInterface => "System interface",
            Self::Windows => "App windows",
        }
    }
    pub fn parse(value: &str) -> Option<Self> {
        Self::ALL
            .into_iter()
            .find(|surface| surface.key().trim_start_matches("frosted_") == value)
    }
    fn get(self, builder: &ThemeBuilder) -> bool {
        match self {
            Self::Panel => builder.frosted_panel,
            Self::Applets => builder.frosted_applets,
            Self::SystemInterface => builder.frosted_system_interface,
            Self::Windows => builder.frosted_windows,
        }
    }
    fn set(self, builder: &mut ThemeBuilder, value: bool) {
        match self {
            Self::Panel => builder.frosted_panel = value,
            Self::Applets => builder.frosted_applets = value,
            Self::SystemInterface => builder.frosted_system_interface = value,
            Self::Windows => builder.frosted_windows = value,
        }
    }
}

/// `cosmic-comp-config` AppearanceConfig at 4d14528, field for field.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CompositorAppearance {
    pub clip_floating_windows: bool,
    pub clip_tiled_windows: bool,
    pub shadow_tiled_windows: bool,
}

impl Default for CompositorAppearance {
    fn default() -> Self {
        Self {
            clip_floating_windows: true,
            clip_tiled_windows: true,
            shadow_tiled_windows: false,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum WindowEffect {
    ClipFloating,
    ClipTiled,
    ShadowTiled,
}

impl WindowEffect {
    pub const ALL: [Self; 3] = [Self::ClipFloating, Self::ClipTiled, Self::ShadowTiled];
    pub fn title(self) -> &'static str {
        match self {
            Self::ClipFloating => "Round floating window corners",
            Self::ClipTiled => "Round tiled window corners",
            Self::ShadowTiled => "Shadows on tiled windows",
        }
    }
    pub fn key(self) -> &'static str {
        match self {
            Self::ClipFloating => "clip_floating",
            Self::ClipTiled => "clip_tiled",
            Self::ShadowTiled => "shadow_tiled",
        }
    }
    pub fn parse(value: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|effect| effect.key() == value)
    }
    fn get(self, config: &CompositorAppearance) -> bool {
        match self {
            Self::ClipFloating => config.clip_floating_windows,
            Self::ClipTiled => config.clip_tiled_windows,
            Self::ShadowTiled => config.shadow_tiled_windows,
        }
    }
    fn set(self, config: &mut CompositorAppearance, value: bool) {
        match self {
            Self::ClipFloating => config.clip_floating_windows = value,
            Self::ClipTiled => config.clip_tiled_windows = value,
            Self::ShadowTiled => config.shadow_tiled_windows = value,
        }
    }
}

/// Blur strength as the 0–13 step cosmic-settings shows.
pub fn blur_from_step(step: u8) -> Option<BlurStrength> {
    BlurStrength::try_from(step).ok()
}
/// cosmic-settings' glass slider (0–1) mapped to an alpha map.
pub fn glass_alpha_map(percent: u8) -> AlphaMap {
    let glass = f32::from(percent.min(100)) / 100.;
    let defaults = AlphaMap::default();
    let offset = (glass - 0.5) / 0.5
        * if glass < 0.5 {
            defaults.extremely_high_2
        } else {
            1.0 - defaults.extremely_low
        };
    defaults.offset(offset)
}

/// Exact builder values for one variant, as a profile captured them. Only
/// the fields that are `Some` (or present) are set; the others keep the
/// variant's current value. Nothing is quantized or shared between variants.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct VariantValues {
    pub colors: BTreeMap<ColorRole, Option<Srgba>>,
    pub outline_width: Option<u32>,
    pub radii: Option<CornerRadii>,
    pub spacing: Option<Spacing>,
    pub blur: Option<BlurStrength>,
    pub alpha: Option<AlphaMap>,
    pub frosted: BTreeMap<Surface, bool>,
    /// Window gaps `(outer, inner)`, as COSMIC stores them.
    pub gaps: Option<(u32, u32)>,
}

impl VariantValues {
    /// Sets these values on `builder`. A wider outline widens the inner gap
    /// as cosmic-settings does, and set gaps keep the inner gap at least the
    /// outline width; without either the gaps are left alone.
    pub fn apply(&self, builder: &mut ThemeBuilder) {
        for (role, color) in &self.colors {
            role.set(builder, *color);
        }
        if let Some(width) = self.outline_width {
            builder.active_hint = width.min(MAX_OUTLINE);
        }
        if let Some((outer, inner)) = self.gaps {
            builder.gaps = (outer.min(MAX_GAP), inner.min(MAX_GAP));
        }
        if self.outline_width.is_some() || self.gaps.is_some() {
            builder.gaps.1 = builder.gaps.1.max(builder.active_hint);
        }
        if let Some(radii) = self.radii {
            builder.corner_radii = radii;
        }
        if let Some(spacing) = self.spacing {
            builder.spacing = spacing;
        }
        if let Some(blur) = self.blur {
            builder.frosted = blur;
        }
        if let Some(alpha) = self.alpha {
            builder.alpha_map = alpha;
        }
        for (surface, on) in &self.frosted {
            surface.set(builder, *on);
        }
    }
    /// These values as `builder` holds them now (every field filled).
    pub fn of(builder: &ThemeBuilder) -> Self {
        Self {
            colors: ColorRole::ALL
                .into_iter()
                .map(|role| (role, role.get(builder)))
                .collect(),
            outline_width: Some(builder.active_hint),
            radii: Some(builder.corner_radii),
            spacing: Some(builder.spacing),
            blur: Some(builder.frosted),
            alpha: Some(builder.alpha_map),
            frosted: Surface::ALL
                .into_iter()
                .map(|surface| (surface, surface.get(builder)))
                .collect(),
            gaps: Some(builder.gaps),
        }
    }
}

/// One editable control. Color and gap controls belong to one variant; the
/// others apply to both variants, like cosmic-settings. `Variant`,
/// `InterfaceDensity` and `HeaderSize` are for profiles: exact per-variant
/// builder values and the two toolkit density keys on their own. The gap
/// controls are edited from the Windows page, through its own draft.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum Control {
    Mode,
    Color(ThemeVariant, ColorRole),
    OutlineWidth,
    Roundness,
    Density,
    Frosted(Surface),
    Blur,
    Glass,
    Font(FontRole),
    IconTheme,
    Window(WindowEffect),
    InterfaceDensity,
    HeaderSize,
    /// The gap between windows and the screen edges.
    OuterGap(ThemeVariant),
    /// The gap between tiled windows; never below the active outline.
    InnerGap(ThemeVariant),
    Variant(ThemeVariant),
}

impl Control {
    fn themed(self) -> bool {
        !matches!(
            self,
            Self::Mode
                | Self::Font(_)
                | Self::IconTheme
                | Self::Window(_)
                | Self::InterfaceDensity
                | Self::HeaderSize
        )
    }
    fn affects(self, variant: ThemeVariant) -> bool {
        match self {
            Self::Color(owner, _)
            | Self::OuterGap(owner)
            | Self::InnerGap(owner)
            | Self::Variant(owner) => owner == variant,
            control => control.themed(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum Value {
    Mode(SystemMode),
    Color(Option<Srgba>),
    Number(u32),
    Roundness(Roundness),
    Density(Density),
    Flag(bool),
    Blur(BlurStrength),
    Font(FontConfig),
    Name(String),
    Variant(Box<VariantValues>),
}

/// The patch for a draft, plus edits that cannot be applied faithfully.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Plan {
    pub patch: DesktopPatch,
    pub blocked: Vec<Blocked>,
}

/// Colors and metrics the sample window shows, after the draft's changes.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Preview {
    pub background: Srgba,
    pub on_background: Srgba,
    pub divider: Srgba,
    pub primary: Srgba,
    pub on_primary: Srgba,
    pub secondary: Srgba,
    pub on_secondary: Srgba,
    pub accent: Srgba,
    pub on_accent: Srgba,
    pub button: Srgba,
    pub on_button: Srgba,
    pub outline: Srgba,
    pub outline_width: u32,
    pub radius_s: f32,
    pub radius_m: f32,
    pub frosted: bool,
}

fn key(namespace: &str, version: u64, name: &str) -> ConfigKey {
    ConfigKey::new(namespace, version, name).expect("appearance keys are valid")
}
fn mode_key(name: &str) -> ConfigKey {
    key(MODE, 1, name)
}
fn toolkit_key(name: &str) -> ConfigKey {
    key(TOOLKIT, 1, name)
}
fn compositor_key() -> ConfigKey {
    key(COMPOSITOR, 1, "appearance_settings")
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct AppearanceDraft {
    base: Snapshot,
    edits: BTreeMap<Control, Value>,
    conflicts: Vec<ConfigKey>,
}

impl AppearanceDraft {
    /// Every key the draft reads: all writable keys plus derivation inputs.
    pub fn keys() -> Vec<ConfigKey> {
        let mut keys = vec![mode_key("is_dark"), mode_key("auto_switch")];
        keys.extend(
            [
                "icon_theme",
                "interface_density",
                "header_size",
                "interface_font",
                "monospace_font",
            ]
            .map(toolkit_key),
        );
        keys.push(compositor_key());
        for variant in [ThemeVariant::Dark, ThemeVariant::Light] {
            keys.extend(
                theme::BUILDER_WRITES
                    .iter()
                    .chain(&theme::BUILDER_INPUTS)
                    .map(|name| builder_key(variant, name)),
            );
            keys.extend(
                theme::THEME_WRITES
                    .iter()
                    .map(|name| theme_key(variant, name)),
            );
        }
        keys
    }
    /// Reads only; creates no files or directories.
    pub fn load(store: &DesktopStore) -> Result<Self, DesktopError> {
        Ok(Self::from_snapshot(store.load(&Self::keys())?))
    }
    pub fn from_snapshot(base: Snapshot) -> Self {
        Self {
            base,
            edits: BTreeMap::new(),
            conflicts: Vec::new(),
        }
    }
    /// The values the draft started from, without its edits.
    #[allow(dead_code)] // Profiles (stage 3) captures and compares these.
    pub fn snapshot(&self) -> &Snapshot {
        &self.base
    }
    pub fn into_snapshot(self) -> Snapshot {
        self.base
    }
    pub fn dirty(&self) -> bool {
        !self.edits.is_empty()
    }
    pub fn edits(&self) -> impl Iterator<Item = (&Control, &Value)> {
        self.edits.iter()
    }
    pub fn edited(&self, control: Control) -> bool {
        self.edits.contains_key(&control)
    }
    pub fn clear_accent_edits(&mut self) {
        self.edits
            .retain(|control, _| !matches!(control, Control::Color(_, ColorRole::Accent)));
    }
    pub fn conflicts(&self) -> &[ConfigKey] {
        &self.conflicts
    }
    pub fn discard(&mut self) {
        self.edits.clear();
        self.conflicts.clear();
    }
    #[allow(dead_code)] // Profiles (stage 3).
    pub fn rebase(&mut self, snapshot: Snapshot) {
        self.base = snapshot;
        self.discard();
    }

    fn effective(&self, key: &ConfigKey) -> Option<&RawValue> {
        self.base.effective(key)
    }
    fn read<T: DeserializeOwned + Serialize>(&self, key: &ConfigKey, default: T) -> Setting<T> {
        match self.effective(key) {
            Some(raw) => match decode_exact(raw) {
                Ok(value) => Setting::Known(value),
                Err(_) => Setting::Unsupported(raw.as_str().trim().to_string()),
            },
            None => Setting::Known(default),
        }
    }
    pub fn base_builder(&self, variant: ThemeVariant) -> Result<ThemeBuilder, Blocked> {
        theme::read_builder(&self.base, variant)
    }
    /// The builder with this draft's edits for `variant`.
    pub fn builder(&self, variant: ThemeVariant) -> Result<ThemeBuilder, Blocked> {
        let mut builder = self.base_builder(variant)?;
        for (control, value) in &self.edits {
            if !control.affects(variant) {
                continue;
            }
            match (control, value) {
                (Control::Color(_, role), Value::Color(color)) => role.set(&mut builder, *color),
                (Control::OutlineWidth, Value::Number(width)) => {
                    builder.active_hint = *width;
                    // As cosmic-settings: the inner gap is never below the outline.
                    builder.gaps.1 = builder.gaps.1.max(*width);
                }
                (Control::Roundness, Value::Roundness(roundness)) => {
                    builder.corner_radii = CornerRadii::from(*roundness);
                }
                (Control::Density, Value::Density(density)) => {
                    builder.spacing = Spacing::from(*density);
                }
                (Control::Frosted(surface), Value::Flag(on)) => surface.set(&mut builder, *on),
                (Control::Blur, Value::Blur(strength)) => builder.frosted = *strength,
                (Control::Glass, Value::Number(percent)) => {
                    builder.alpha_map = glass_alpha_map(*percent as u8);
                }
                (Control::OuterGap(_), Value::Number(gap)) => builder.gaps.0 = *gap,
                (Control::InnerGap(_), Value::Number(gap)) => builder.gaps.1 = *gap,
                (Control::Variant(_), Value::Variant(values)) => values.apply(&mut builder),
                _ => {}
            }
        }
        // An edited inner gap is never below the outline. An edited outer
        // gap leaves the inner one exactly as it is.
        if self.edits.contains_key(&Control::InnerGap(variant)) {
            builder.gaps.1 = builder.gaps.1.max(builder.active_hint);
        }
        Ok(builder)
    }

    /// The current value of `control`, from the edit or the base. `None`
    /// when the base cannot be read (shown as unsupported).
    fn base_value(&self, control: Control) -> Option<Value> {
        let both = |get: &dyn Fn(&ThemeBuilder) -> Value| {
            let dark = self.base_builder(ThemeVariant::Dark).ok().map(|b| get(&b));
            let light = self.base_builder(ThemeVariant::Light).ok().map(|b| get(&b));
            (dark == light).then_some(dark).flatten()
        };
        match control {
            Control::Mode => self.mode_base().known().copied().map(Value::Mode),
            Control::Color(variant, role) => self
                .base_builder(variant)
                .ok()
                .map(|builder| Value::Color(role.get(&builder))),
            Control::OutlineWidth => both(&|b| Value::Number(b.active_hint)),
            Control::Roundness => both(&|b| Value::Roundness(b.corner_radii.into())),
            Control::Density => {
                let toolkit = self
                    .read(&toolkit_key("interface_density"), Density::Standard)
                    .known()
                    .copied();
                let header = self
                    .read(&toolkit_key("header_size"), Density::Standard)
                    .known()
                    .copied();
                let spacing = both(&|b| Value::Density(b.spacing.into()));
                match (toolkit, header, spacing) {
                    (Some(a), Some(b), Some(Value::Density(c))) if a == b && b == c => {
                        Some(Value::Density(a))
                    }
                    _ => None,
                }
            }
            Control::Frosted(surface) => both(&|b| Value::Flag(surface.get(b))),
            Control::Blur => both(&|b| Value::Blur(b.frosted)),
            Control::Glass => both(&|b| Value::Number(glass_percent(&b.alpha_map))),
            Control::Font(role) => self
                .read(&toolkit_key(role.key()), role.default())
                .known()
                .cloned()
                .map(Value::Font),
            Control::IconTheme => self
                .read(&toolkit_key("icon_theme"), "Cosmic".to_string())
                .known()
                .cloned()
                .map(Value::Name),
            Control::Window(effect) => self
                .compositor_base()
                .known()
                .map(|config| Value::Flag(effect.get(config))),
            Control::InterfaceDensity | Control::HeaderSize => self
                .read(
                    &toolkit_key(toolkit_density_key(control)),
                    Density::Standard,
                )
                .known()
                .copied()
                .map(Value::Density),
            Control::OuterGap(variant) => self
                .base_builder(variant)
                .ok()
                .map(|builder| Value::Number(builder.gaps.0)),
            Control::InnerGap(variant) => self
                .base_builder(variant)
                .ok()
                .map(|builder| Value::Number(builder.gaps.1)),
            // Compared field by field in `set`.
            Control::Variant(_) => None,
        }
    }
    /// Records an edit, or forgets it when it matches the starting value.
    pub fn set(&mut self, control: Control, value: Value) {
        let value = match (control, value) {
            (Control::Color(_, role), Value::Color(color)) => {
                Value::Color(color.map(|color| quantize(color, role.alpha())))
            }
            (Control::OutlineWidth, Value::Number(width)) => Value::Number(width.min(MAX_OUTLINE)),
            (Control::Glass, Value::Number(percent)) => Value::Number(percent.min(100)),
            (Control::OuterGap(_), Value::Number(gap)) => Value::Number(gap.min(MAX_GAP)),
            // An edited inner gap is never below the outline (see `builder`):
            // compare and keep the value Apply writes, so dragging below
            // the outline onto the current value is no edit.
            (Control::InnerGap(variant), Value::Number(gap)) => {
                let outline = self.builder(variant).map_or(0, |b| b.active_hint);
                Value::Number(gap.min(MAX_GAP).max(outline))
            }
            (_, value) => value,
        };
        // "Follow the accent" also clears an outline color only the derived
        // theme holds, so it is an edit even where the builder follows.
        if let (Control::Color(variant, ColorRole::Outline), Value::Color(None)) = (control, &value)
            && self.derived_outline(variant).is_some()
        {
            self.edits.insert(control, value);
            return;
        }
        if let (Control::Variant(variant), Value::Variant(values)) = (control, &value) {
            // An edit that changes nothing on the base builder is no edit.
            let unchanged = self.base_builder(variant).is_ok_and(|base| {
                let mut edited = base.clone();
                values.apply(&mut edited);
                VariantValues::of(&edited) == VariantValues::of(&base) && edited.gaps == base.gaps
            });
            if unchanged {
                self.edits.remove(&control);
            } else {
                self.edits.insert(control, value);
            }
            return;
        }
        if self.base_value(control).as_ref() == Some(&value) {
            self.edits.remove(&control);
        } else {
            self.edits.insert(control, value);
        }
    }
    /// Drops one edit, keeping the others.
    pub fn forget(&mut self, control: Control) {
        self.edits.remove(&control);
    }
    /// A shared app color is an explicit intent for both variants, even
    /// when one already matches. Keep its inputs pinned until the combined
    /// transaction commits so outside changes cannot silently break sync.
    /// Each variant gets its own tone (see `themes::link_accent`).
    pub fn set_linked_accent(&mut self, variant: ThemeVariant, color: Srgba) {
        self.edits.insert(
            Control::Color(variant, ColorRole::Accent),
            Value::Color(Some(quantize(color, false))),
        );
    }
    #[allow(dead_code)] // Profiles (stage 3) reads logical values.
    pub fn value(&self, control: Control) -> Option<Value> {
        self.edits
            .get(&control)
            .cloned()
            .or_else(|| self.base_value(control))
    }

    fn mode_base(&self) -> Setting<SystemMode> {
        let is_dark = self.read(&mode_key("is_dark"), true);
        let auto = self.read(&mode_key("auto_switch"), false);
        match (is_dark, auto) {
            (_, Setting::Known(true)) => Setting::Known(SystemMode::Automatic),
            (Setting::Known(true), Setting::Known(false)) => Setting::Known(SystemMode::Dark),
            (Setting::Known(false), Setting::Known(false)) => Setting::Known(SystemMode::Light),
            (Setting::Unsupported(raw), _) | (_, Setting::Unsupported(raw)) => {
                Setting::Unsupported(raw)
            }
        }
    }
    pub fn mode(&self) -> Setting<SystemMode> {
        match self.edits.get(&Control::Mode) {
            Some(Value::Mode(mode)) => Setting::Known(*mode),
            _ => self.mode_base(),
        }
    }
    /// The variant COSMIC shows now (not changed by previewing the other).
    pub fn active_variant(&self) -> ThemeVariant {
        match self.read(&mode_key("is_dark"), true) {
            Setting::Known(false) => ThemeVariant::Light,
            _ => ThemeVariant::Dark,
        }
    }
    pub fn color(&self, variant: ThemeVariant, role: ColorRole) -> Setting<Option<Srgba>> {
        match self.builder(variant) {
            Ok(builder) => Setting::Known(role.get(&builder)),
            Err(blocked) => Setting::Unsupported(
                self.effective(&blocked.key)
                    .map(|raw| raw.as_str().trim().to_string())
                    .unwrap_or_default(),
            ),
        }
    }
    /// An active-window outline color only the derived theme holds while
    /// the builder follows the accent (another tool wrote it). COSMIC draws
    /// it. Reads the starting values, without this draft's edits.
    pub fn derived_outline(&self, variant: ThemeVariant) -> Option<Srgb> {
        if self.base_builder(variant).ok()?.window_hint.is_some() {
            return None;
        }
        self.read(&theme_key(variant, "window_hint"), None::<Srgb>)
            .known()
            .copied()
            .flatten()
    }
    /// The derived outline color an explicit "follow the accent" replaces.
    fn relinked_outline(&self, variant: ThemeVariant) -> Option<Srgb> {
        (self.edits.get(&Control::Color(variant, ColorRole::Outline)) == Some(&Value::Color(None)))
            .then(|| self.derived_outline(variant))
            .flatten()
    }
    /// Whether COSMIC draws the variant's active-window outline in the
    /// accent after this draft.
    pub fn outline_follows(&self, variant: ThemeVariant) -> bool {
        self.color(variant, ColorRole::Outline) == Setting::Known(None)
            && (self.derived_outline(variant).is_none() || self.relinked_outline(variant).is_some())
    }
    fn theme_setting<T>(
        &self,
        variant: ThemeVariant,
        get: impl Fn(&ThemeBuilder) -> T,
    ) -> Setting<T> {
        match self.builder(variant) {
            Ok(builder) => Setting::Known(get(&builder)),
            Err(blocked) => Setting::Unsupported(
                self.effective(&blocked.key)
                    .map(|raw| raw.as_str().trim().to_string())
                    .unwrap_or_default(),
            ),
        }
    }
    pub fn outline_width(&self, variant: ThemeVariant) -> Setting<u32> {
        self.theme_setting(variant, |b| b.active_hint)
    }
    /// The inner window gap after the draft's edits (the Windows page
    /// edits it; System appearance shows it for the outline coupling).
    pub fn inner_gap(&self, variant: ThemeVariant) -> Setting<u32> {
        self.theme_setting(variant, |b| b.gaps.1)
    }
    /// Both window gaps `(outer, inner)` after the draft's edits: the
    /// values Apply writes, inner already widened to the outline.
    pub fn gaps(&self, variant: ThemeVariant) -> Setting<(u32, u32)> {
        self.theme_setting(variant, |b| b.gaps)
    }
    pub fn roundness(&self, variant: ThemeVariant) -> Setting<Roundness> {
        self.theme_setting(variant, |b| b.corner_radii.into())
    }
    /// Radii that match none of the three presets are kept and shown as custom.
    pub fn custom_radii(&self, variant: ThemeVariant) -> bool {
        self.builder(variant).is_ok_and(|b| {
            let roundness: Roundness = b.corner_radii.into();
            CornerRadii::from(roundness) != b.corner_radii
        })
    }
    pub fn density(&self) -> Setting<Density> {
        match self.edits.get(&Control::Density) {
            Some(Value::Density(density)) => Setting::Known(*density),
            _ => self.read(&toolkit_key("interface_density"), Density::Standard),
        }
    }
    pub fn frosted(&self, variant: ThemeVariant, surface: Surface) -> Setting<bool> {
        self.theme_setting(variant, |b| surface.get(b))
    }
    pub fn blur(&self, variant: ThemeVariant) -> Setting<BlurStrength> {
        self.theme_setting(variant, |b| b.frosted)
    }
    pub fn glass(&self, variant: ThemeVariant) -> Setting<u32> {
        self.theme_setting(variant, |b| glass_percent(&b.alpha_map))
    }
    pub fn font(&self, role: FontRole) -> Setting<FontConfig> {
        match self.edits.get(&Control::Font(role)) {
            Some(Value::Font(font)) => Setting::Known(font.clone()),
            _ => self.read(&toolkit_key(role.key()), role.default()),
        }
    }
    pub fn icon_theme(&self) -> Setting<String> {
        match self.edits.get(&Control::IconTheme) {
            Some(Value::Name(name)) => Setting::Known(name.clone()),
            _ => self.read(&toolkit_key("icon_theme"), "Cosmic".to_string()),
        }
    }
    fn compositor_base(&self) -> Setting<CompositorAppearance> {
        self.read(&compositor_key(), CompositorAppearance::default())
    }
    pub fn window(&self, effect: WindowEffect) -> Setting<bool> {
        match self.edits.get(&Control::Window(effect)) {
            Some(Value::Flag(on)) => Setting::Known(*on),
            _ => match self.compositor_base() {
                Setting::Known(config) => Setting::Known(effect.get(&config)),
                Setting::Unsupported(raw) => Setting::Unsupported(raw),
            },
        }
    }

    /// The patch Apply writes, derived from the base and the edits.
    pub fn plan(&self) -> Plan {
        let mut patch = DesktopPatch::new();
        let mut blocked = Vec::new();
        let user = |key: &ConfigKey| self.base.user(key).cloned();
        // Theme mode. Automatic keeps the current light/dark choice.
        if let Some(Value::Mode(mode)) = self.edits.get(&Control::Mode) {
            let (is_dark, auto) = match mode {
                SystemMode::Light => (Some(false), Some(false)),
                SystemMode::Dark => (Some(true), Some(false)),
                SystemMode::Automatic => (None, Some(true)),
            };
            for (name, default, desired) in
                [("is_dark", true, is_dark), ("auto_switch", false, auto)]
            {
                let key = mode_key(name);
                let current = self.read(&key, default).known().copied();
                match desired {
                    Some(value) if current != Some(value) => {
                        patch.set(key.clone(), user(&key), encode(&value));
                    }
                    _ => patch.guard(key.clone(), user(&key)),
                }
            }
        }
        // Both theme variants.
        for variant in [ThemeVariant::Dark, ThemeVariant::Light] {
            if !self.edits.keys().any(|control| control.affects(variant)) {
                continue;
            }
            let (base, edited) = match (self.base_builder(variant), self.builder(variant)) {
                (Ok(base), Ok(edited)) => (base, edited),
                (Err(error), _) | (_, Err(error)) => {
                    blocked.push(error);
                    continue;
                }
            };
            let mut staged = DesktopPatch::new();
            if let Err(reason) =
                theme::builder_patch(&mut staged, &self.base, variant, &base, &edited)
            {
                blocked.push(Blocked {
                    key: builder_key(variant, "accent"),
                    reason,
                });
                continue;
            }
            let old = base.clone().build();
            let new = edited.build();
            // The derivation keeps customized theme roles; an explicit
            // "follow the accent" also replaces an outline color only the
            // derived theme holds.
            let drawn = self.relinked_outline(variant).map(|hint| {
                let mut drawn = old.clone();
                drawn.window_hint = Some(hint);
                drawn
            });
            let derivation = theme::Derivation {
                fallback: &old,
                old: drawn.as_ref().unwrap_or(&old),
                new: &new,
                whole: theme::Whole::WhenChanged,
            };
            match theme::theme_patch(&mut staged, &self.base, variant, &derivation) {
                Ok(()) => patch.extend(staged),
                Err(errors) => blocked.extend(errors),
            }
        }
        // Toolkit: density, fonts and icons.
        if let Some(Value::Density(density)) = self.edits.get(&Control::Density) {
            for name in ["interface_density", "header_size"] {
                let key = toolkit_key(name);
                patch.set(key.clone(), user(&key), encode(density));
            }
        }
        for role in FontRole::ALL {
            if let Some(Value::Font(font)) = self.edits.get(&Control::Font(role)) {
                let key = toolkit_key(role.key());
                match self.read(&key, role.default()) {
                    Setting::Known(_) => patch.set(key.clone(), user(&key), encode(font)),
                    Setting::Unsupported(reason) => blocked.push(Blocked {
                        key,
                        reason: format!("unrecognized value {reason}"),
                    }),
                }
            }
        }
        for control in [Control::InterfaceDensity, Control::HeaderSize] {
            if let Some(Value::Density(density)) = self.edits.get(&control) {
                let key = toolkit_key(toolkit_density_key(control));
                match self.read(&key, Density::Standard) {
                    Setting::Known(_) => patch.set(key.clone(), user(&key), encode(density)),
                    Setting::Unsupported(reason) => blocked.push(Blocked {
                        key,
                        reason: format!("unrecognized value {reason}"),
                    }),
                }
            }
        }
        if let Some(Value::Name(name)) = self.edits.get(&Control::IconTheme) {
            let key = toolkit_key("icon_theme");
            patch.set(key.clone(), user(&key), encode(name));
        }
        // Compositor: one struct for all three switches.
        if WindowEffect::ALL
            .into_iter()
            .any(|effect| self.edits.contains_key(&Control::Window(effect)))
        {
            let key = compositor_key();
            match self.compositor_base() {
                Setting::Known(mut config) => {
                    for effect in WindowEffect::ALL {
                        if let Some(Value::Flag(on)) = self.edits.get(&Control::Window(effect)) {
                            effect.set(&mut config, *on);
                        }
                    }
                    patch.set(key.clone(), user(&key), encode(&config));
                }
                Setting::Unsupported(_) => blocked.push(Blocked {
                    key,
                    reason: "contains settings this version does not recognize".into(),
                }),
            }
        }
        Plan { patch, blocked }
    }
    pub fn patch(&self) -> DesktopPatch {
        self.plan().patch
    }
    pub fn blocked(&self) -> Vec<Blocked> {
        if self.edits.is_empty() {
            return Vec::new();
        }
        self.plan().blocked
    }

    /// Keys whose current values this edit derives from or writes.
    fn pinned(&self, only: Option<Control>) -> Vec<ConfigKey> {
        let draft;
        let source = match only {
            Some(control) => {
                let mut single = Self::from_snapshot(self.base.clone());
                if let Some(value) = self.edits.get(&control) {
                    single.edits.insert(control, value.clone());
                }
                draft = single;
                &draft
            }
            None => self,
        };
        let plan = source.plan();
        let mut keys: Vec<ConfigKey> = plan.patch.keys().cloned().collect();
        keys.extend(plan.patch.guards().map(|(key, _)| key.clone()));
        keys.extend(plan.blocked.into_iter().map(|blocked| blocked.key));
        keys
    }
    /// Adopts outside changes to keys no edit depends on. Keys an edit
    /// writes or derives from keep their base so Apply detects the change;
    /// those that changed are listed as conflicts until resolved.
    pub fn refresh(&mut self, snapshot: Snapshot) {
        let pinned = self.pinned(None);
        let fresh = Self::from_snapshot(snapshot.clone());
        let independent: Vec<_> = ThemeVariant::ALL
            .into_iter()
            .filter(|&variant| {
                let (Ok(old), Ok(new), Ok(edited)) = (
                    self.base_builder(variant),
                    fresh.base_builder(variant),
                    self.builder(variant),
                ) else {
                    return false;
                };
                // An edit's explicit input must still be the value it started on.
                // Shared generated files alone do not make two edits conflict.
                if self
                    .edits
                    .keys()
                    .filter(|control| control.affects(variant))
                    .any(|&control| {
                        matches!(control, Control::Variant(_))
                            || self.base_value(control) != fresh.base_value(control)
                    })
                {
                    return false;
                }
                let mut local = DesktopPatch::new();
                if theme::builder_patch(&mut local, &self.base, variant, &old, &edited).is_err()
                    || local
                        .keys()
                        .any(|key| self.base.user(key) != snapshot.user(key))
                {
                    return false;
                }
                let old = old.build();
                let new = new.build();
                theme::derived_transition_matches(
                    &self.base, &snapshot, variant, &old, &new, &pinned,
                )
            })
            .collect();
        let mut base = snapshot;
        for key in pinned {
            if ThemeVariant::of(&key).is_some_and(|(variant, _)| independent.contains(&variant)) {
                self.conflicts.retain(|conflict| *conflict != key);
                continue;
            }
            let old = self.base.entry(&key).cloned().unwrap_or_default();
            let changed = base.user(&key) != old.user.as_ref();
            if changed && !self.conflicts.contains(&key) {
                self.conflicts.push(key.clone());
            } else if !changed {
                self.conflicts.retain(|conflict| *conflict != key);
            }
            base.insert(key, old);
        }
        self.conflicts.sort();
        self.base = base;
    }
    /// Keeps every edit over the outside changes: the current values of
    /// the conflicting keys become the baseline, so Apply writes the edits
    /// over them and still refuses anything changed after this.
    pub fn keep_edits(&mut self, snapshot: Snapshot) {
        let keys = std::mem::take(&mut self.conflicts);
        self.base.adopt(&snapshot, &keys);
        self.refresh(snapshot);
    }
    /// Drops every edit that depends on a conflicting key (and so the whole
    /// coupled set), adopts the current values and keeps the other edits.
    pub fn resolve_conflicts(&mut self, snapshot: Snapshot, keys: &[ConfigKey]) {
        let conflicted: Vec<ConfigKey> =
            keys.iter().chain(self.conflicts.iter()).cloned().collect();
        let controls: Vec<Control> = self.edits.keys().copied().collect();
        for control in controls {
            if self
                .pinned(Some(control))
                .iter()
                .any(|key| conflicted.contains(key))
            {
                self.edits.remove(&control);
            }
        }
        self.conflicts.clear();
        for key in &conflicted {
            if let Some(entry) = snapshot.entry(key) {
                self.base.insert(key.clone(), entry.clone());
            }
        }
        self.refresh(snapshot);
    }

    /// The sample window for `variant` after the draft's changes. Reads only.
    pub fn preview(&self, variant: ThemeVariant) -> Preview {
        let builder = self
            .builder(variant)
            .unwrap_or_else(|_| variant.default_builder());
        let built = builder.clone().build();
        let plan = self.plan();
        let value =
            |name: &str| theme::theme_value::<Container>(&plan.patch, &self.base, variant, name);
        let component =
            |name: &str| theme::theme_value::<Component>(&plan.patch, &self.base, variant, name);
        let background = value("background").unwrap_or_else(|| built.background(false).clone());
        let primary = value("primary").unwrap_or_else(|| built.primary(false).clone());
        let secondary = value("secondary").unwrap_or_else(|| built.secondary(false).clone());
        let accent = component("accent").unwrap_or_else(|| built.accent.clone());
        let button = component("button").unwrap_or_else(|| built.button.clone());
        let radii =
            theme::theme_value::<CornerRadii>(&plan.patch, &self.base, variant, "corner_radii")
                .unwrap_or(built.corner_radii);
        let hint =
            theme::theme_value::<Option<Srgb>>(&plan.patch, &self.base, variant, "window_hint")
                .unwrap_or(built.window_hint);
        let width = theme::theme_value::<u32>(&plan.patch, &self.base, variant, "active_hint")
            .unwrap_or(built.active_hint);
        Preview {
            background: background.base,
            on_background: background.on,
            divider: background.divider,
            primary: primary.base,
            on_primary: primary.on,
            secondary: secondary.base,
            on_secondary: secondary.on,
            accent: accent.base,
            on_accent: accent.on,
            button: button.base,
            on_button: button.on,
            outline: hint
                .map(|c| Srgba::new(c.red, c.green, c.blue, 1.))
                .unwrap_or(accent.base),
            outline_width: width,
            radius_s: radii.radius_s[0],
            radius_m: radii.radius_m[0],
            frosted: Surface::ALL
                .into_iter()
                .any(|surface| surface.get(&builder)),
        }
    }
    /// What an Automatic color resolves to now, for its swatch.
    pub fn derived_color(&self, variant: ThemeVariant, role: ColorRole) -> Srgba {
        let preview = self.preview(variant);
        match role {
            ColorRole::Accent => preview.accent,
            ColorRole::Background => preview.background,
            ColorRole::Primary => preview.primary,
            ColorRole::Secondary => preview.secondary,
            ColorRole::Outline => preview.outline,
        }
    }
}

fn toolkit_density_key(control: Control) -> &'static str {
    match control {
        Control::HeaderSize => "header_size",
        _ => "interface_density",
    }
}

/// The glass percentage whose alpha map is exactly `map`, if any.
pub fn exact_glass_percent(map: &AlphaMap) -> Option<u8> {
    let percent = glass_percent(map).min(100) as u8;
    (glass_alpha_map(percent) == *map).then_some(percent)
}

fn glass_percent(map: &AlphaMap) -> u32 {
    (map.guess_offset().clamp(0., 1.) * 100.).round() as u32
}

fn encode<T: Serialize>(value: &T) -> Option<RawValue> {
    RawValue::encode(value).ok()
}

/// Palette matching (`--apply-cosmic-colors`): `accent` in both variants,
/// derived exactly as before this module existed. The old theme is rebuilt
/// with the theme's current accent so customized non-accent roles survive,
/// and the accent components are always replaced together.
pub fn palette_patch(snapshot: &Snapshot, accent: Srgb) -> Result<DesktopPatch, String> {
    let mut patch = DesktopPatch::new();
    for variant in [ThemeVariant::Dark, ThemeVariant::Light] {
        let unreadable = |blocked: Blocked| {
            format!(
                "Could not read COSMIC theme {}: {}",
                blocked.key, blocked.reason
            )
        };
        let builder = theme::read_builder(snapshot, variant).map_err(unreadable)?;
        let fallback = builder.clone().build();
        let accent_key = theme_key(variant, "accent");
        let current_accent = match snapshot.effective(&accent_key) {
            Some(raw) => {
                decode_exact::<Component>(raw)
                    .map_err(|reason| {
                        unreadable(Blocked {
                            key: accent_key.clone(),
                            reason,
                        })
                    })?
                    .base
            }
            None => fallback.accent_color(),
        };
        let old = builder.clone().accent(current_accent.color).build();
        let edited = builder.clone().accent(accent);
        let new = edited.clone().build();
        let mut staged = DesktopPatch::new();
        theme::builder_patch(&mut staged, snapshot, variant, &builder, &edited)?;
        theme::theme_patch(
            &mut staged,
            snapshot,
            variant,
            &theme::Derivation {
                fallback: &fallback,
                old: &old,
                new: &new,
                whole: theme::Whole::Always,
            },
        )
        .map_err(|blocked| {
            blocked
                .into_iter()
                .map(unreadable)
                .collect::<Vec<_>>()
                .join("; ")
        })?;
        patch.extend(staged);
    }
    Ok(patch)
}

/// Short user-facing name of an appearance key, for conflict messages.
pub fn key_label(key: &ConfigKey) -> String {
    if let Some((variant, builder)) = ThemeVariant::of(key) {
        let what = match key.key() {
            "accent" | "accent_button" | "accent_text" => "accent",
            "bg_color" | "background" | "transparent_background" => "background",
            "primary_container_bg" | "primary" | "transparent_primary" => "primary surface",
            "secondary_container_bg" | "secondary" | "transparent_secondary" => "secondary surface",
            "window_hint" | "active_hint" => "window outline",
            "gaps" => "window gaps",
            "corner_radii" => "roundness",
            "spacing" => "density",
            name if name.starts_with("frosted") || name == "alpha_map" => "frosted effects",
            _ if builder => "theme settings",
            _ => "theme colors",
        };
        return format!("{} {what}", variant.title());
    }
    match (key.namespace(), key.key()) {
        (MODE, _) => "theme mode".into(),
        (TOOLKIT, "interface_density" | "header_size") => "density".into(),
        (TOOLKIT, "interface_font") => "interface font".into(),
        (TOOLKIT, "monospace_font") => "monospace font".into(),
        (TOOLKIT, "icon_theme") => "icon theme".into(),
        (COMPOSITOR, _) => "window corners and shadows".into(),
        _ => key.to_string(),
    }
}

// Validators: the value must decode exactly as the pinned type, and stay
// within the ranges cosmic-settings offers.

fn exact<T: DeserializeOwned + Serialize>(value: &RawValue) -> Result<(), String> {
    decode_exact::<T>(value).map(drop)
}
fn finite(values: &[f32], max: f32) -> Result<(), String> {
    values
        .iter()
        .all(|value| value.is_finite() && (0.0..=max).contains(value))
        .then_some(())
        .ok_or_else(|| "Value out of range".into())
}
fn check_color(value: &RawValue) -> Result<(), String> {
    let color: Option<Srgba> = decode_exact::<ColorReprOption>(value)?.into();
    color.map_or(Ok(()), |c| finite(&[c.red, c.green, c.blue, c.alpha], 1.))
}
fn check_color_repr(value: &RawValue) -> Result<(), String> {
    let c: Srgba = decode_exact::<ColorRepr>(value)?.into();
    finite(&[c.red, c.green, c.blue, c.alpha], 1.)
}
fn check_plain_color(value: &RawValue) -> Result<(), String> {
    decode_exact::<Option<Srgb>>(value)?.map_or(Ok(()), |c| finite(&[c.red, c.green, c.blue], 1.))
}
fn check_outline(value: &RawValue) -> Result<(), String> {
    (decode_exact::<u32>(value)? <= MAX_OUTLINE)
        .then_some(())
        .ok_or_else(|| format!("Outline must be at most {MAX_OUTLINE}"))
}
fn check_gaps(value: &RawValue) -> Result<(), String> {
    let (outer, inner) = decode_exact::<(u32, u32)>(value)?;
    (outer <= MAX_GAP && inner <= MAX_GAP)
        .then_some(())
        .ok_or_else(|| format!("Gaps must be at most {MAX_GAP}"))
}
fn check_radii(value: &RawValue) -> Result<(), String> {
    let r = decode_exact::<CornerRadii>(value)?;
    for radius in [
        r.radius_0,
        r.radius_xs,
        r.radius_s,
        r.radius_m,
        r.radius_l,
        r.radius_xl,
    ] {
        finite(&radius, 1000.)?;
    }
    Ok(())
}
fn check_alpha_map(value: &RawValue) -> Result<(), String> {
    let m = decode_exact::<AlphaMap>(value)?;
    finite(
        &[
            m.extremely_low,
            m.extremely_low_2,
            m.very_low,
            m.very_low_2,
            m.low,
            m.low_2,
            m.medium,
            m.medium_2,
            m.high,
            m.high_2,
            m.very_high,
            m.very_high_2,
            m.extremely_high,
            m.extremely_high_2,
        ],
        1.,
    )
}
fn check_font(value: &RawValue) -> Result<(), String> {
    let font = decode_exact::<FontConfig>(value)?;
    valid_name(&font.family, 128)
        .then_some(())
        .ok_or_else(|| "Invalid font family".into())
}
fn check_icon_theme(value: &RawValue) -> Result<(), String> {
    let name = decode_exact::<String>(value)?;
    (valid_name(&name, 128) && !name.contains('/') && name != "." && name != "..")
        .then_some(())
        .ok_or_else(|| "Invalid icon theme".into())
}
fn valid_name(value: &str, max: usize) -> bool {
    !value.trim().is_empty() && value.len() <= max && !value.chars().any(char::is_control)
}

macro_rules! rules {
    ($namespace:expr, $version:expr; $($key:literal => $validate:expr),+ $(,)?) => {
        [$(KeyRule {
            namespace: Namespace::Exact($namespace),
            version: $version,
            key: $key,
            validate: $validate,
        }),+]
    };
}
macro_rules! builder_rules {
    ($namespace:expr) => {
        rules![$namespace, 2;
            "accent" => check_color,
            "bg_color" => check_color,
            "primary_container_bg" => check_color,
            "secondary_container_bg" => check_color,
            "window_hint" => check_color,
            "active_hint" => check_outline,
            "gaps" => check_gaps,
            "corner_radii" => check_radii,
            "spacing" => exact::<Spacing>,
            "frosted" => exact::<BlurStrength>,
            "alpha_map" => check_alpha_map,
            "frosted_windows" => exact::<bool>,
            "frosted_system_interface" => exact::<bool>,
            "frosted_panel" => exact::<bool>,
            "frosted_applets" => exact::<bool>,
        ]
    };
}
macro_rules! theme_rules {
    ($namespace:expr) => {
        rules![$namespace, 2;
            "button" => exact::<Component>,
            "accent" => exact::<Component>,
            "success" => exact::<Component>,
            "destructive" => exact::<Component>,
            "warning" => exact::<Component>,
            "accent_button" => exact::<Component>,
            "success_button" => exact::<Component>,
            "destructive_button" => exact::<Component>,
            "warning_button" => exact::<Component>,
            "icon_button" => exact::<Component>,
            "link_button" => exact::<Component>,
            "list_button" => exact::<Component>,
            "text_button" => exact::<Component>,
            "background" => exact::<Container>,
            "primary" => exact::<Container>,
            "secondary" => exact::<Container>,
            "transparent_background" => exact::<Container>,
            "transparent_primary" => exact::<Container>,
            "transparent_secondary" => exact::<Container>,
            "palette" => exact::<CosmicPaletteInner>,
            "spacing" => exact::<Spacing>,
            "corner_radii" => check_radii,
            "gaps" => check_gaps,
            "active_hint" => check_outline,
            "window_hint" => check_plain_color,
            "frosted" => exact::<BlurStrength>,
            "frosted_windows" => exact::<bool>,
            "frosted_system_interface" => exact::<bool>,
            "frosted_panel" => exact::<bool>,
            "frosted_applets" => exact::<bool>,
            "alpha_map" => check_alpha_map,
            "shade" => check_color_repr,
            "accent_text" => check_color,
            "control_tint" => check_color,
            "text_tint" => check_color,
        ]
    };
}

static DARK_BUILDER: [KeyRule; 15] = builder_rules!("com.system76.CosmicTheme.Dark.Builder");
static LIGHT_BUILDER: [KeyRule; 15] = builder_rules!("com.system76.CosmicTheme.Light.Builder");
static DARK_THEME: [KeyRule; 35] = theme_rules!("com.system76.CosmicTheme.Dark");
static LIGHT_THEME: [KeyRule; 35] = theme_rules!("com.system76.CosmicTheme.Light");
static OTHER: [KeyRule; 8] = [
    KeyRule {
        namespace: Namespace::Exact(MODE),
        version: 1,
        key: "is_dark",
        validate: exact::<bool>,
    },
    KeyRule {
        namespace: Namespace::Exact(MODE),
        version: 1,
        key: "auto_switch",
        validate: exact::<bool>,
    },
    KeyRule {
        namespace: Namespace::Exact(TOOLKIT),
        version: 1,
        key: "icon_theme",
        validate: check_icon_theme,
    },
    KeyRule {
        namespace: Namespace::Exact(TOOLKIT),
        version: 1,
        key: "interface_density",
        validate: exact::<Density>,
    },
    KeyRule {
        namespace: Namespace::Exact(TOOLKIT),
        version: 1,
        key: "header_size",
        validate: exact::<Density>,
    },
    KeyRule {
        namespace: Namespace::Exact(TOOLKIT),
        version: 1,
        key: "interface_font",
        validate: check_font,
    },
    KeyRule {
        namespace: Namespace::Exact(TOOLKIT),
        version: 1,
        key: "monospace_font",
        validate: check_font,
    },
    KeyRule {
        namespace: Namespace::Exact(COMPOSITOR),
        version: 1,
        key: "appearance_settings",
        validate: exact::<CompositorAppearance>,
    },
];

/// Every appearance key the adapter may write.
pub fn rules() -> impl Iterator<Item = &'static KeyRule> {
    DARK_BUILDER
        .iter()
        .chain(&LIGHT_BUILDER)
        .chain(&DARK_THEME)
        .chain(&LIGHT_THEME)
        .chain(&OTHER)
}

#[cfg(test)]
mod tests;
