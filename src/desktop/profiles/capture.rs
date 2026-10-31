//! The applied desktop as a profile record. Reads only.
//!
//! Effective values are captured: the user layer, else the system default,
//! else the upstream default (and Config's defaults for widgets), so a
//! capture applied to the same desktop changes nothing. Each theme variant
//! is read from its own builder. A value this version cannot decode is left
//! out with a warning naming it, never reserialized.
//!
//! The wallpaper is the effective shared background, its source classified
//! as a bundled palette wallpaper, a trusted system picture or a personal
//! picture or folder; the display arrangement is captured only when it is
//! complete and coherent.
use super::format::{
    AppearanceLook, Applets, BackgroundLook, DesktopIconsLook, DisplaysName, FitName, GradientLook,
    ModeName, PanelLook, Profile, ProfileError, Rgb, SlideshowLook, SourceLook, VariantLook,
    WallpaperLook, WindowLook, WindowsBehaviorLook, WorkspacesLook,
};
use crate::config;
use crate::desktop::appearance::{
    AppearanceDraft, Control, FontRole, MAX_GAP, SystemMode, ThemeVariant, Value as Edit,
    VariantValues, WindowEffect,
};
use crate::desktop::desktop_icons::DesktopIconsDraft;
use crate::desktop::panels::{PanelDraft, PanelKind, Setting};
use crate::desktop::store::widget::{self, WidgetGroup};
use crate::desktop::store::{DesktopStore, rule_for};
use crate::desktop::wallpaper::{
    self, BgColor, BgEntry, BgSource, Class, Displays, ScalingMode, Target, WallpaperDraft,
};
use crate::desktop::windows::{Behavior, Choice, WindowsDraft};
use crate::desktop::workspaces::WorkspacesDraft;
use serde::de::DeserializeOwned;
use serde_json::Value;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

/// A captured record and what could not be captured.
#[derive(Clone, Debug, PartialEq)]
pub struct Capture {
    pub profile: Profile,
    pub warnings: Vec<String>,
}

/// Captures the applied (not drafted) Dock, Panel, Appearance (with window
/// gaps), window behavior, Workspaces, wallpaper, desktop icons and widget
/// settings as a profile named `name`. Writes nothing.
pub fn capture(store: &DesktopStore, name: &str) -> Result<Capture, ProfileError> {
    let mut profile = Profile::new(name).ok_or_else(|| {
        ProfileError::Invalid("Profile names have 1–80 characters and no line breaks.".into())
    })?;
    let mut warnings = Vec::new();
    for kind in PanelKind::ALL {
        let look = capture_panel(store, kind, &mut warnings)?;
        match kind {
            PanelKind::Dock => profile.dock = look,
            PanelKind::Panel => profile.panel = look,
        }
    }
    profile.appearance = Some(capture_appearance(store, &mut warnings)?);
    profile.windows = Some(capture_windows(store, &mut warnings)?);
    profile.workspaces = capture_workspaces(store, &mut warnings)?;
    profile.wallpaper = capture_wallpaper(store, &mut warnings);
    profile.desktop_icons = capture_desktop_icons(store, &mut warnings);
    match store.app_config() {
        Some(path) => match read_widgets(path) {
            Ok(effective) => {
                profile.widget_style = Some(capture_widgets(
                    &effective,
                    WidgetGroup::Style,
                    &mut warnings,
                ));
                profile.widget_layout = Some(capture_widgets(
                    &effective,
                    WidgetGroup::Layout,
                    &mut warnings,
                ));
            }
            Err(error) => warnings.push(format!("Widget settings were not captured: {error}")),
        },
        None => warnings.push("Widget settings aren’t available here; not captured.".into()),
    }
    // The capture is the version it is stored as, so it reads back equal.
    profile.version = profile.written_version();
    profile.validate()?;
    Ok(Capture { profile, warnings })
}

fn capture_panel(
    store: &DesktopStore,
    kind: PanelKind,
    warnings: &mut Vec<String>,
) -> Result<Option<PanelLook>, ProfileError> {
    let draft = PanelDraft::load(store, kind)?;
    if !draft.installed() {
        warnings.push(format!(
            "The {} isn’t set up on this computer; not captured.",
            kind.title()
        ));
        return Ok(None);
    }
    fn known<T: DeserializeOwned>(
        draft: &PanelDraft,
        name: &str,
        warnings: &mut Vec<String>,
    ) -> Option<T> {
        match draft.value::<T>(name) {
            Setting::Known(value) => Some(value),
            Setting::Unsupported(_) => {
                warnings.push(format!(
                    "{} {} has a value this version doesn’t recognize; not captured.",
                    draft.kind().title(),
                    crate::desktop::panels::key_label(name)
                ));
                None
            }
        }
    }
    let islands = match draft.islands() {
        Setting::Known(mut s) if s.enabled => {
            if crate::desktop::islands::Client::connect(store)
                .and_then(|mut c| c.capabilities())
                .is_ok()
            {
                // Shared presets keep a useful normalized arrangement without
                // exporting connector names. A named panel uses its own layout.
                let layout = match draft.output() {
                    Setting::Known(crate::desktop::panels::Output::Name(name)) => {
                        s.outputs.get(&name)
                    }
                    _ => s.outputs.values().next(),
                }
                .cloned();
                if let Some(layout) = layout {
                    s.default_layout = layout;
                }
                Some(s)
            } else {
                warnings.push(format!(
                    "{} island layout was left out because its component is unavailable",
                    kind.title()
                ));
                None
            }
        }
        _ => None,
    };
    let look = PanelLook {
        island_gap: None,
        islands,
        anchor: known(&draft, "anchor", warnings),
        floating: known(&draft, "anchor_gap", warnings),
        margin: known(&draft, "margin", warnings),
        size: known(&draft, "size", warnings),
        size_wings: known(&draft, "size_wings", warnings),
        size_center: known(&draft, "size_center", warnings),
        extend: known(&draft, "expand_to_edges", warnings),
        autohide: known(&draft, "autohide", warnings),
        exclusive_zone: known(&draft, "exclusive_zone", warnings),
        autohide_behavior: known(&draft, "autohide_behavior", warnings),
        opacity: known(&draft, "opacity", warnings),
        border_radius: known(&draft, "border_radius", warnings),
        spacing: known(&draft, "spacing", warnings),
        padding: known(&draft, "padding", warnings),
        background: known(&draft, "background", warnings),
        keep_style: known(&draft, "keep_style_on_maximize", warnings),
        applets: match draft.applets() {
            Setting::Known(regions) => Some(Applets {
                start: regions.start,
                center: regions.center,
                end: regions.end,
            }),
            Setting::Unsupported(_) => {
                warnings.push(format!(
                    "{} applets have a value this version doesn’t recognize; not captured.",
                    kind.title()
                ));
                None
            }
        },
    };
    Ok(Some(look))
}

fn capture_appearance(
    store: &DesktopStore,
    warnings: &mut Vec<String>,
) -> Result<AppearanceLook, ProfileError> {
    let draft = AppearanceDraft::load(store)?;
    let mut unsupported = |what: &str| {
        warnings.push(format!(
            "{what} has a value this version doesn’t recognize; not captured."
        ));
    };
    let mut look = AppearanceLook::default();
    match draft.mode() {
        Setting::Known(mode) => {
            look.mode = Some(match mode {
                SystemMode::Light => ModeName::Light,
                SystemMode::Dark => ModeName::Dark,
                SystemMode::Automatic => ModeName::Automatic,
            });
        }
        Setting::Unsupported(_) => unsupported("Theme mode"),
    }
    for (control, slot, what) in [
        (
            Control::InterfaceDensity,
            &mut look.interface_density,
            "Interface density",
        ),
        (Control::HeaderSize, &mut look.header_size, "Header size"),
    ] {
        match draft.value(control) {
            Some(Edit::Density(density)) => *slot = Some(density),
            _ => unsupported(what),
        }
    }
    for role in FontRole::ALL {
        match draft.font(role) {
            Setting::Known(font) => match role {
                FontRole::Interface => look.interface_font = Some(font),
                FontRole::Monospace => look.monospace_font = Some(font),
            },
            Setting::Unsupported(_) => unsupported(role.title()),
        }
    }
    match draft.icon_theme() {
        Setting::Known(name) => look.icon_theme = Some(name),
        Setting::Unsupported(_) => unsupported("Icon theme"),
    }
    let mut window = WindowLook::default();
    let mut window_known = true;
    for effect in WindowEffect::ALL {
        match draft.window(effect) {
            Setting::Known(on) => match effect {
                WindowEffect::ClipFloating => window.clip_floating = Some(on),
                WindowEffect::ClipTiled => window.clip_tiled = Some(on),
                WindowEffect::ShadowTiled => window.shadow_tiled = Some(on),
            },
            Setting::Unsupported(_) => window_known = false,
        }
    }
    if window_known {
        look.window = Some(window);
    } else {
        unsupported("Window corners and shadows");
    }
    for variant in [ThemeVariant::Light, ThemeVariant::Dark] {
        match draft.base_builder(variant) {
            Ok(builder) => {
                let mut variant_look = VariantLook::of(&VariantValues::of(&builder));
                if builder.gaps.0 > MAX_GAP || builder.gaps.1 > MAX_GAP {
                    variant_look.gaps = None;
                    warnings.push(format!(
                        "{} window gaps are outside the range profiles support; not captured.",
                        variant.title()
                    ));
                }
                match variant {
                    ThemeVariant::Light => look.light = Some(variant_look),
                    ThemeVariant::Dark => look.dark = Some(variant_look),
                }
            }
            Err(blocked) => warnings.push(format!(
                "The {} theme has settings this version doesn’t recognize ({}); not captured.",
                variant.title(),
                blocked.key
            )),
        }
    }
    Ok(look)
}

/// Every window behavior value this version can represent and profiles
/// allow; the others are named and left out.
fn capture_windows(
    store: &DesktopStore,
    warnings: &mut Vec<String>,
) -> Result<WindowsBehaviorLook, ProfileError> {
    let draft = WindowsDraft::load(store)?;
    let mut look = WindowsBehaviorLook::default();
    for behavior in Behavior::ALL {
        let choice = match draft.base_value(behavior) {
            Setting::Known(choice) => choice,
            Setting::Unsupported(_) => {
                warnings.push(format!(
                    "Windows {} has a value this version doesn’t recognize; not captured.",
                    behavior.title()
                ));
                continue;
            }
        };
        // Values outside what profiles may write (a very long focus delay)
        // are left out rather than making the whole capture invalid.
        let allowed = choice_raw(choice).is_some_and(|raw| {
            rule_for(&behavior.key()).is_some_and(|rule| (rule.validate)(&raw).is_ok())
        });
        if !allowed {
            warnings.push(format!(
                "Windows {} is outside the range profiles support; not captured.",
                behavior.title()
            ));
            continue;
        }
        match (behavior, choice) {
            (Behavior::Autotile, Choice::Flag(on)) => look.autotile = Some(on),
            (Behavior::Scope, Choice::Scope(scope)) => look.autotile_behavior = Some(scope.into()),
            (Behavior::ActiveHint, Choice::Flag(on)) => look.active_hint = Some(on),
            (Behavior::FocusFollowsCursor, Choice::Flag(on)) => {
                look.focus_follows_cursor = Some(on);
            }
            (Behavior::CursorFollowsFocus, Choice::Flag(on)) => {
                look.cursor_follows_focus = Some(on);
            }
            (Behavior::FocusDelay, Choice::Millis(ms)) => {
                look.focus_follows_cursor_delay = Some(ms);
            }
            (Behavior::EdgeSnap, Choice::Pixels(px)) => look.edge_snap_threshold = Some(px),
            (Behavior::ShowMinimize, Choice::Flag(on)) => look.show_minimize = Some(on),
            (Behavior::ShowMaximize, Choice::Flag(on)) => look.show_maximize = Some(on),
            _ => {}
        }
    }
    Ok(look)
}

fn choice_raw(choice: Choice) -> Option<crate::desktop::store::RawValue> {
    use crate::desktop::store::RawValue;
    match choice {
        Choice::Flag(on) => RawValue::encode(&on),
        Choice::Scope(scope) => RawValue::encode(&scope),
        Choice::Millis(ms) => RawValue::encode(&ms),
        Choice::Pixels(px) => RawValue::encode(&px),
    }
    .ok()
}

/// All four workspace members, or a warning when the struct has members or
/// values this version doesn't recognize.
fn capture_workspaces(
    store: &DesktopStore,
    warnings: &mut Vec<String>,
) -> Result<Option<WorkspacesLook>, ProfileError> {
    let draft = WorkspacesDraft::load(store)?;
    Ok(match draft.base_config() {
        Setting::Known(config) => Some(WorkspacesLook {
            workspace_mode: Some(config.workspace_mode.into()),
            workspace_layout: Some(config.workspace_layout.into()),
            action_on_typing: Some(config.action_on_typing.into()),
            workspace_wraparound: Some(config.workspace_wraparound),
        }),
        Setting::Unsupported(_) => {
            warnings.push(
                "Workspace settings have values this version doesn’t recognize; not captured."
                    .into(),
            );
            None
        }
    })
}

/// The widget configuration with Config's defaults filled in, as JSON.
fn read_widgets(path: &std::path::Path) -> Result<(Value, Value), String> {
    let text = config::read_config_text(path)?;
    let mut raw = config::parse_config_text(text.as_deref())?;
    let typed = config::check_config_value(&raw)?;
    // Export the visible layout, without exporting this machine's display IDs.
    let contexts = if config::Config::path().as_deref() == Some(path) {
        crate::layout_state::read(&typed).widget_displays
    } else {
        Default::default()
    };
    let contexts = crate::drawer::Widget::ALL
        .into_iter()
        .filter_map(|w| {
            contexts
                .get(w.key())
                .cloned()
                .or_else(|| typed.monitor_preference(w).map(str::to_owned))
                .map(|display| (w.key().to_string(), display))
        })
        .collect();
    let typed = typed.on_displays(&contexts);
    if typed.multi_monitor() {
        raw["widget_positions"] = serde_json::json!(typed.widget_positions);
        raw["widget_sizes"] = serde_json::json!(typed.widget_sizes);
    }
    let effective = serde_json::to_value(&typed).map_err(|error| error.to_string())?;
    Ok((raw, effective))
}

fn capture_widgets(
    (raw, effective): &(Value, Value),
    group: WidgetGroup,
    warnings: &mut Vec<String>,
) -> Value {
    let mut section = Value::Object(Default::default());
    let has_materials = raw
        .get("materials")
        .and_then(Value::as_object)
        .is_some_and(|members| members.values().any(|value| !value.is_null()));
    let clock_fields = [
        "/clock/face",
        "/clock/weight",
        "/clock/italic",
        "/clock/surface",
    ];
    let has_clock_presentation = clock_fields
        .iter()
        .any(|pointer| raw.pointer(pointer).is_some());
    for field in widget::fields(group) {
        if field.pointer == "/rain" && raw.get("rain").is_none() {
            continue;
        }
        // Never-configured recipes keep legacy exports compatible. An applied
        // recipe or explicit reset is captured so later Apply restores intent.
        if field.pointer.starts_with("/recipes/") && raw.pointer(field.pointer).is_none() {
            continue;
        }
        if field.pointer.starts_with("/moments/") && raw.get("moments").is_none() {
            continue;
        }
        if field.pointer == "/clock/depth" && raw.pointer(field.pointer).is_none() {
            continue;
        }
        // Never-configured new choices do not make legacy captures require a
        // newer reader. Once used (including explicit reset), capture all four.
        if clock_fields.contains(&field.pointer) && !has_clock_presentation {
            continue;
        }
        // Defaults alone must not raise an older setup's format version.
        if field.pointer.starts_with("/materials/") && !has_materials {
            continue;
        }
        // A material setup owns motion's effective default too. Omission on
        // apply means leave unchanged, so keep both explicit boolean choices.
        if field.pointer == "/material_motion"
            && !has_materials
            && raw.get("material_motion").is_none()
        {
            continue;
        }
        if [
            "/appearance/family",
            "/appearance/surface",
            "/appearance/wallpaper_light",
        ]
        .contains(&field.pointer)
            && effective
                .pointer("/appearance/family")
                .is_none_or(Value::is_null)
        {
            continue;
        }
        // A tool someone never set up stays out, so the profile leaves it
        // alone (and keeps the older format) wherever it is applied.
        if widget::tool_of(field.pointer).is_some_and(|tool| raw.get(tool).is_none()) {
            continue;
        }
        let removable = field.pointer.starts_with("/widget_positions/")
            || field.pointer.starts_with("/widget_sizes/");
        let value = if removable {
            // A widget without a saved position uses its default place.
            widget::get(raw, field.pointer).unwrap_or(Value::Null)
        } else {
            match widget::get(effective, field.pointer).or_else(|| {
                (field.pointer == "/rain")
                    .then(|| widget::get(raw, field.pointer))
                    .flatten()
            }) {
                Some(value) => value,
                None => continue,
            }
        };
        if (removable && value.is_null()) || field.accepts(&value) {
            let _ = widget::put(&mut section, field.pointer, Some(value));
        } else {
            warnings.push(format!(
                "{} has a value this version doesn’t support in profiles; not captured.",
                field.label
            ));
        }
    }
    section
}

/// Where wallpaper sources are classified (`wallpaper::classify`).
struct Places {
    bundled_dir: PathBuf,
    system_dirs: Vec<PathBuf>,
}

/// The shared background and, when complete and coherent, the display
/// arrangement with every listed display's own background. `None` (with a
/// warning) when nothing can be represented.
pub(super) fn capture_wallpaper(
    store: &DesktopStore,
    warnings: &mut Vec<String>,
) -> Option<WallpaperLook> {
    let draft = match WallpaperDraft::load(store) {
        Ok(draft) => draft,
        Err(error) => {
            warnings.push(format!("The wallpaper was not captured: {error}"));
            return None;
        }
    };
    let places = Places {
        bundled_dir: wallpaper::bundled_dir(store),
        system_dirs: wallpaper::background_dirs(store),
    };
    let mut look = WallpaperLook::default();
    match draft.effective(&Target::Shared) {
        Setting::Known(entry) => look.background = Some(background_look(&entry, &places, warnings)),
        Setting::Unsupported(_) => warnings.push(
            "The shared background has settings this version doesn’t recognize; not captured."
                .into(),
        ),
    }
    match draft.displays() {
        Setting::Known(Displays::Same) => look.displays = Some(DisplaysName::Same),
        Setting::Known(Displays::Separate) => match separate_backgrounds(&draft, &places, warnings) {
            Some(map) => {
                look.displays = Some(DisplaysName::Separate);
                look.per_display = Some(map);
            }
            None => warnings.push(
                "Some displays have backgrounds this version can’t represent; the display arrangement was not captured."
                    .into(),
            ),
        },
        Setting::Unsupported(_) => warnings.push(
            "Whether all displays share a background is set to a value this version doesn’t recognize; the display arrangement was not captured."
                .into(),
        ),
    }
    (!look.is_empty()).then_some(look)
}

/// Every listed display's own background, or `None` when any of them (or
/// the list) can't be represented exactly: a partial map would later
/// remove the displays it left out.
fn separate_backgrounds(
    draft: &WallpaperDraft,
    places: &Places,
    warnings: &mut Vec<String>,
) -> Option<BTreeMap<String, BackgroundLook>> {
    // A list this version can't rewrite is kept, never captured in part.
    if draft
        .snapshot()
        .effective(&wallpaper::list_key())
        .is_some_and(|raw| wallpaper::check_list(raw).is_err())
    {
        return None;
    }
    let mut map = BTreeMap::new();
    let mut notes = Vec::new();
    for view in draft.display_views().into_iter().filter(|view| view.listed) {
        // Each listed display must show exactly its own readable entry
        // (not one claimed by another display or kept as unsupported).
        let shown = draft.effective(&Target::Output(view.name.clone()));
        match (view.own, shown) {
            (Some(Setting::Known(own)), Setting::Known(shown)) if own == shown => {
                map.insert(view.name, background_look(&own, places, &mut notes));
            }
            _ => return None,
        }
    }
    warnings.extend(notes);
    Some(map)
}

/// A native entry as a profile background: its source (classified), and
/// for pictures the fit and for folders the slideshow.
fn background_look(entry: &BgEntry, places: &Places, warnings: &mut Vec<String>) -> BackgroundLook {
    let mut look = BackgroundLook::default();
    match &entry.source {
        BgSource::Color(BgColor::Single(rgb)) => {
            look.source = Some(SourceLook::Color(Rgb::of_srgb(*rgb)));
        }
        BgSource::Color(BgColor::Gradient(gradient)) => {
            look.source = Some(SourceLook::Gradient(GradientLook {
                stops: gradient
                    .colors
                    .iter()
                    .copied()
                    .map(Rgb::of_linear)
                    .collect(),
                angle: gradient.radius,
            }));
        }
        BgSource::Path(path) => {
            let folder = match std::fs::metadata(path) {
                Ok(metadata) => metadata.is_dir(),
                Err(_) => {
                    warnings.push(format!(
                        "Your wallpaper “{}” is missing; the profile keeps where it was.",
                        super::desktop_look::file_label(path)
                    ));
                    !wallpaper::is_image(path)
                }
            };
            look.source = Some(source_look(path, folder, places));
            match &entry.scaling_mode {
                ScalingMode::Zoom => look.fit = Some(FitName::Fill),
                ScalingMode::Stretch => look.fit = Some(FitName::Stretch),
                ScalingMode::Fit(rgb) => {
                    look.fit = Some(FitName::Fit);
                    look.fit_color = Some(Rgb::of_srgb(*rgb));
                }
            }
            if folder {
                look.slideshow = Some(SlideshowLook {
                    every: Some(entry.rotation_frequency),
                    order: Some(entry.sampling_method.into()),
                });
            }
        }
    }
    look
}

fn source_look(path: &Path, folder: bool, places: &Places) -> SourceLook {
    match wallpaper::classify(path, &places.bundled_dir, &places.system_dirs) {
        Class::Bundled(name) => SourceLook::Bundled(name.to_string()),
        Class::System(relative) if super::desktop_look::valid_relative(&relative) => {
            SourceLook::System(relative)
        }
        _ if folder => SourceLook::Folder(path.to_path_buf()),
        _ => SourceLook::Image(path.to_path_buf()),
    }
}

/// Every desktop icons member, exactly (also values between the page's
/// slider steps); `None` with a warning for a value this version doesn't
/// recognize.
pub(super) fn capture_desktop_icons(
    store: &DesktopStore,
    warnings: &mut Vec<String>,
) -> Option<DesktopIconsLook> {
    match DesktopIconsDraft::load(store).map(|draft| draft.base_config()) {
        Ok(Setting::Known(config)) => Some(DesktopIconsLook::of(&config)),
        Ok(Setting::Unsupported(_)) => {
            warnings.push(
                "Desktop icon settings have values this version doesn’t recognize; not captured."
                    .into(),
            );
            None
        }
        Err(error) => {
            warnings.push(format!("Desktop icons were not captured: {error}"));
            None
        }
    }
}
