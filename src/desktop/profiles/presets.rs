//! Curated desktop setups, each with a distinct purpose and widget composition.
//! Stable IDs keep existing schedules working. Selecting is read-only; Apply
//! uses the same scoped plan and Undo as saved setups. Widget layout is an
//! explicit replacement, while notes, accounts, folders, display assignments,
//! applets and timer state remain the user's. Missing resources degrade locally.
use super::format::{Profile, import};
use super::themes;
use crate::desktop_theme::Family;
use dock_islands_core::{Arrangement, Placement, Section, Separate, Settings, Together};
use serde_json::{Value, json};

/// A built-in profile: stable ID, short description and the record.
#[derive(Clone, Debug, PartialEq)]
pub struct Preset {
    pub id: &'static str,
    pub description: &'static str,
    pub profile: Profile,
}

/// The quick switch, listed apart from the starter looks.
pub const CLEAN: &str = "presentation";

/// Earlier releases' presets, kept for schedules that still name them.
const RENAMED: [(&str, &str); 3] = [
    ("minimal", "focus"),
    ("glass", "evening"),
    ("workbench", "halcyon"),
];

/// A coordinated appearance paired with a useful desktop composition.
struct Look {
    id: &'static str,
    name: &'static str,
    description: &'static str,
    family: Family,
    /// The bundled wallpaper (`wallpaper::BUNDLED`).
    wallpaper: &'static str,
    /// The widget palette nearest the wallpaper.
    palette: &'static str,
    /// The wallpaper's own accent, as the automatic colors pick it. It
    /// seeds the COSMIC, widget, dock and panel colors until the running
    /// cards keep them following the wallpaper.
    seed: [u8; 3],
    light: bool,
}

/// Corner radius COSMIC rounds into a pill at any bar size.
const PILL: u32 = crate::desktop::panels::MAX_RADIUS;

const LOOKS: [Look; 10] = [
    Look {
        id: "halcyon",
        name: "Everyday",
        description: "Your day at a glance, with system activity off to the side.",
        family: Family::Frosted,
        wallpaper: "halcyon-night",
        palette: "ocean",
        seed: [0x9b, 0xae, 0xee],
        light: false,
    },
    Look {
        id: "calm",
        name: "Slow morning",
        description: "The time, the weather, and a little space to collect your thoughts.",
        family: Family::Soft,
        wallpaper: "halcyon-calm",
        palette: "teal",
        seed: [0x6a, 0xc2, 0xc6],
        light: false,
    },
    Look {
        id: "evening",
        name: "After hours",
        description: "A quiet clock and your music. Leave the workday behind.",
        family: Family::Soft,
        wallpaper: "halcyon-evening",
        palette: "lavender",
        seed: [0xb2, 0xa4, 0xf3],
        light: false,
    },
    Look {
        id: "focus",
        name: "Deep work",
        description: "One timer, a quiet clock, and room for the task in front of you.",
        family: Family::Bare,
        wallpaper: "halcyon-focus",
        palette: "graphite",
        seed: [0x8d, 0xb4, 0xe7],
        light: false,
    },
    Look {
        id: "paper",
        name: "Writing desk",
        description: "A warm, light desktop with a notebook always within reach.",
        family: Family::Paper,
        wallpaper: "halcyon-paper",
        palette: "amber",
        seed: [0xd5, 0xa7, 0x76],
        light: true,
    },
    Look {
        id: "rose",
        name: "Studio",
        description: "Ideas on the left. Projects and system activity on the right.",
        family: Family::Glass,
        wallpaper: "halcyon-rose",
        palette: "rose",
        seed: [0xf1, 0x92, 0x95],
        light: false,
    },
    Look {
        id: "tidal",
        name: "Tidal",
        description: "A large, open clock with weather and music along the edge.",
        family: Family::Soft,
        wallpaper: "halcyon-calm",
        palette: "teal",
        seed: [0x6a, 0xc2, 0xc6],
        light: false,
    },
    Look {
        id: "paper-studio",
        name: "Paper",
        description: "An analog clock, a notebook and a focus timer on a warm, light desktop.",
        family: Family::Paper,
        wallpaper: "halcyon-paper",
        palette: "amber",
        seed: [0xd5, 0xa7, 0x76],
        light: true,
    },
    Look {
        id: "moss",
        name: "Moss",
        description: "Soft green panels for the time, weather and your music.",
        family: Family::Soft,
        wallpaper: "teal",
        palette: "teal",
        seed: [0x9d, 0xcf, 0xbe],
        light: false,
    },
    Look {
        id: "carbon",
        name: "Carbon",
        description: "A compact clock, system readings and projects with fine outlines.",
        family: Family::Outline,
        wallpaper: "graphite",
        palette: "graphite",
        seed: [0xa4, 0xc1, 0xdf],
        light: false,
    },
];

/// Deep-merges `patch` into `base`; objects merge, everything else replaces.
fn merge(base: &mut Value, patch: Value) {
    match (base, patch) {
        (Value::Object(base), Value::Object(patch)) => {
            for (key, value) in patch {
                merge(base.entry(key).or_insert(Value::Null), value);
            }
        }
        (base, patch) => *base = patch,
    }
}

fn record(look: &Look) -> Value {
    // The family's own record is what Colors & theme applies: widget style,
    // dock and panel bars, and COSMIC colors from the seed.
    let mut base = themes::record(look.family);
    base.appearance_only = false;
    base.appearance = Some(themes::appearance(look.family, Some(look.seed)));
    let mut value = serde_json::to_value(base).expect("profiles serialize");
    // Keep applets and physical outputs; vary the bar treatment and whether
    // the dock steps aside. The panel remains available in every setup.
    let bar = json!({"background": crate::desktop::panels::Background::ThemeDefault,
        "floating": !look.light, "margin": if look.light { 0 } else { 6 },
        "border_radius": if look.light { 12 } else { PILL }});
    let islands = islands_composition(look.id).expect("curated setup");
    merge(
        &mut value,
        json!({
            "name": look.name,
            "appearance": {"mode": if look.light { "light" } else { "dark" }},
            "widget_style": {"theme": look.palette,
                "appearance": {"accent": look.seed, "wallpaper_colors": true,
                    "wallpaper_light": look.light},
                // A see-through At a glance (on a panel over a light
                // wallpaper, where light text wouldn't read), and a system
                // monitor that shows only what every computer has.
                "glance": {"style": if look.light { "panel" } else { "transparent" },
                    "own_background": true, "density": "comfortable"},
                "devbox": {"cpu": true, "gpu": true, "memory": true, "disk": true,
                    "network": true, "uptime": true, "swap": false, "services": false}},
            "wallpaper": {"background": {"source": {"bundled": look.wallpaper}, "fit": "fill"}},
            "widget_layout": composition(look.id),
            "dock": bar.clone(),
            "panel": bar,
        }),
    );
    merge(
        &mut value["panel"],
        json!({"spacing": 6, "padding": 2, "floating": true, "margin": if look.light { 8 } else { 6 },
            "islands": serde_json::to_value(islands).expect("islands serialize")}),
    );
    merge(
        &mut value["dock"],
        json!({"autohide":
        if matches!(look.id, "focus" | "evening") { "Always" } else { "Never" }}),
    );
    // New looks state their Clock choices explicitly. Legacy IDs retain their
    // original records so saved schedules do not acquire a different face.
    let clock = match look.id {
        "tidal" => Some(json!({"face":"digital","surface":"none","scale":150,
            "alignment":"left","center_vertically":true,"weight":"light","italic":false})),
        "paper-studio" => Some(json!({"face":"analog","surface":"panel","scale":100,
            "alignment":"center","center_vertically":true,"weight":"normal","italic":false})),
        "moss" => Some(json!({"face":"digital","surface":"panel","scale":120,
            "alignment":"center","center_vertically":true,"weight":"normal","italic":false})),
        "carbon" => Some(json!({"face":"digital","surface":"none","scale":90,
            "alignment":"left","center_vertically":true,"weight":"normal","italic":false})),
        _ => None,
    };
    if let Some(clock) = clock {
        value["version"] = super::format::VERSION_9.into();
        merge(
            &mut value["widget_style"],
            json!({"clock":clock,"clock_font":null,"appearance":{"color_policy":"wallpaper"}}),
        );
    }
    value
}

/// Portable group placement: moves no applet between groups and contains no
/// connector names. Every recipe has an explicit, non-default arrangement;
/// the planner can distinguish it from a style that preserves placement.
pub fn islands_composition(id: &str) -> Option<Settings> {
    use Section::{Center, End, Start};
    let (placement, gap, together, separate) = match id {
        "halcyon" => (
            Placement::Separate,
            18,
            Together::default(),
            Separate {
                positions: [500, 5000, 9500],
                ..Default::default()
            },
        ),
        "focus" | "tidal" => (
            Placement::Together,
            10,
            Together {
                order: [Center, Start, End],
                position: 5000,
            },
            Separate::default(),
        ),
        "paper" | "paper-studio" => (
            Placement::Separate,
            24,
            Together::default(),
            Separate {
                positions: [1800, 5000, 8200],
                ..Default::default()
            },
        ),
        "rose" | "carbon" => (
            Placement::Separate,
            16,
            Together::default(),
            Separate {
                positions: [500, 2000, 9500],
                ..Default::default()
            },
        ),
        "calm" | "moss" => (
            Placement::Together,
            28,
            Together {
                position: 5500,
                ..Default::default()
            },
            Separate::default(),
        ),
        "evening" => (
            Placement::Together,
            12,
            Together {
                position: 9500,
                ..Default::default()
            },
            Separate::default(),
        ),
        _ => return None,
    };
    Some(Settings {
        enabled: true,
        placement,
        gap,
        default_layout: Arrangement { together, separate },
        ..Default::default()
    })
}

pub fn islands_description(id: &str) -> Option<&'static str> {
    Some(match id {
        "halcyon" => "Three islands, balanced across the panel",
        "focus" | "tidal" => "A compact group in the center",
        "paper" | "paper-studio" => "Three inset islands with soft square corners",
        "rose" | "carbon" => "Two islands to the left, one to the right",
        "calm" | "moss" => "A relaxed center group with room between islands",
        "evening" => "A small group tucked to the right",
        _ => return None,
    })
}

/// No content or account fields: switching the visible tools never erases data.
fn composition(id: &str) -> Value {
    let (widgets, glance, weather, notes, music, focus, positions) = match id {
        "tidal" => (
            json!(["clock", "weather"]),
            false,
            false,
            false,
            true,
            false,
            json!({"clock":[0.04,0.09],"weather":[0.04,0.87],"music":[0.96,0.87]}),
        ),
        "paper-studio" => (
            json!(["clock"]),
            false,
            false,
            true,
            false,
            true,
            json!({"clock":[0.05,0.1],"notes":[0.93,0.18],"focus":[0.93,0.87]}),
        ),
        "moss" => (
            json!(["clock", "weather"]),
            false,
            false,
            false,
            true,
            false,
            json!({"clock":[0.08,0.13],"weather":[0.08,0.84],"music":[0.92,0.84]}),
        ),
        "carbon" => (
            json!(["clock", "devbox", "projects"]),
            false,
            false,
            false,
            false,
            false,
            json!({"clock":[0.04,0.08],"devbox":[0.96,0.1],"projects":[0.96,0.9]}),
        ),
        "halcyon" => (
            json!(["devbox"]),
            true,
            true,
            false,
            false,
            false,
            json!({"glance":[0.035,0.09],"devbox":[0.97,0.58]}),
        ),
        "calm" => (
            json!([]),
            true,
            true,
            true,
            false,
            false,
            json!({"glance":[0.06,0.12],"notes":[0.92,0.58]}),
        ),
        "evening" => (
            json!(["clock"]),
            false,
            false,
            false,
            true,
            false,
            json!({"clock":[0.07,0.1],"music":[0.92,0.82]}),
        ),
        "focus" => (
            json!(["clock"]),
            false,
            false,
            false,
            false,
            true,
            json!({"clock":[0.06,0.12],"focus":[0.93,0.75]}),
        ),
        "paper" => (
            json!(["clock"]),
            false,
            false,
            true,
            false,
            false,
            json!({"clock":[0.05,0.1],"notes":[0.92,0.35]}),
        ),
        "rose" => (
            json!(["devbox", "projects"]),
            false,
            false,
            true,
            false,
            false,
            json!({"notes":[0.04,0.55],"devbox":[0.97,0.1],"projects":[0.97,0.92]}),
        ),
        _ => unreachable!("known curated setup"),
    };
    let mut value = json!({"enabled_widgets": widgets, "widget_positions": positions,
        "glance": {"enabled":glance,"initialized":true,"show_clock":true,
            "show_weather":weather,"show_calendar":false},
        "notes":{"enabled":notes},"music":{"enabled":music},"focus":{"enabled":focus}});
    if id == "paper-studio" {
        value["widget_sizes"] = json!({"clock":[320,340]});
    }
    value
}

/// Short, purpose-led labels for the setup browser; stable IDs are internal.
pub fn occasion(id: &str) -> &'static str {
    match id {
        "tidal" => "Open space",
        "paper-studio" => "A light workspace",
        "moss" => "Music and weather",
        "carbon" => "System at a glance",
        "halcyon" => "A little of everything",
        "calm" => "Start gently",
        "focus" => "Make room to concentrate",
        "paper" => "Think on paper",
        "rose" => "Build something",
        "evening" => "Switch off from work",
        _ => "Your desktop, your way",
    }
}

const CLEAN_RECORD: &str = r##"{
  "format": "desktop-cards-profile", "version": 4, "name": "Clean desktop",
  "dock": { "autohide": "Always" },
  "widget_layout": { "enabled_widgets": [],
                     "glance": { "enabled": false, "show_weather": false, "show_calendar": false },
                     "music": { "enabled": false }, "notes": { "enabled": false },
                     "focus": { "enabled": false } }
}"##;

/// The built-in presets in display order: the starter looks, then the
/// quick switch. Pure data: no files are read.
pub fn presets() -> Vec<Preset> {
    LOOKS
        .iter()
        .map(|look| Preset {
            id: look.id,
            description: look.description,
            profile: import(record(look).to_string().as_bytes())
                .expect("built-in presets are valid"),
        })
        .chain(std::iter::once(Preset {
            id: CLEAN,
            description: "Hide all widgets and let the dock hide. Handy for screen sharing.",
            profile: import(CLEAN_RECORD.as_bytes()).expect("built-in presets are valid"),
        }))
        .collect()
}

/// Additional studio layouts do not change the first-run wizard's choices.
pub fn studio_presets() -> Vec<Preset> {
    presets().into_iter().chain([spine(), ridge()]).collect()
}
fn ridge() -> Preset {
    let mut profile = presets()
        .into_iter()
        .find(|p| p.id == "tidal")
        .unwrap()
        .profile;
    profile.version = super::format::VERSION_11;
    profile.name = "Ridge".into();
    profile
        .wallpaper
        .as_mut()
        .unwrap()
        .background
        .as_mut()
        .unwrap()
        .source = Some(super::SourceLook::Bundled("halcyon-ridge".into()));
    for bar in [&mut profile.panel, &mut profile.dock]
        .into_iter()
        .flatten()
    {
        bar.autohide = Some(crate::desktop::panels::AutoHide::Always);
    }
    profile.widget_style.as_mut().unwrap()["clock"] = json!({"face":"digital","surface":"none","depth":true,"scale":200,"weight":"bold","italic":false,"alignment":"center","center_vertically":false});
    profile.widget_layout = Some(
        json!({"enabled_widgets":["clock"],"widget_positions":{"clock":[0.5,0.25]},"widget_sizes":{"clock":[800,330]},"glance":{"enabled":false,"initialized":true},"notes":{"enabled":false},"music":{"enabled":false},"focus":{"enabled":false}}),
    );
    profile.validate().expect("Ridge is a valid prepared setup");
    Preset {
        id: "ridge",
        description: "A quiet landscape with a clock behind the mountain. Bars hide until needed.",
        profile,
    }
}
fn spine() -> Preset {
    let mut profile = presets()
        .into_iter()
        .find(|p| p.id == "carbon")
        .unwrap()
        .profile;
    profile.name = "Spine".into();
    let panel = profile.panel.as_mut().unwrap();
    panel.anchor = Some(crate::desktop::panels::Anchor::Left);
    panel.size = Some(crate::desktop::panels::PanelSize::M);
    panel.extend = Some(true);
    panel.floating = Some(true);
    panel.margin = Some(8);
    panel.padding = Some(4);
    panel.border_radius = Some(14);
    panel.islands = Some(Settings::default());
    panel.island_gap = None;
    profile.dock.as_mut().unwrap().autohide = Some(crate::desktop::panels::AutoHide::Always);
    profile
        .wallpaper
        .as_mut()
        .unwrap()
        .background
        .as_mut()
        .unwrap()
        .source = Some(super::SourceLook::Bundled("halcyon-workbench".into()));
    profile.widget_layout = Some(json!({
        "enabled_widgets":["clock","weather","devbox"],
        "widget_positions":{"clock":[0.32,0.10],"weather":[0.32,0.86],"devbox":[0.96,0.10]},
        "widget_sizes":{}, "glance":{"enabled":false,"initialized":true},
        "notes":{"enabled":false},"music":{"enabled":false},"focus":{"enabled":false}
    }));
    profile.widget_style.as_mut().unwrap()["clock"]["scale"] = 125.into();
    profile.validate().expect("Spine is a valid portable setup");
    Preset {
        id: "spine",
        description: "A standard vertical panel with time, weather and system readings.",
        profile,
    }
}

/// The built-in preset with `id`, including the earlier releases' names.
pub fn preset(id: &str) -> Option<Preset> {
    if id.starts_with("adaptive-") {
        return super::adaptive::preset(id);
    }
    if id == "ridge" {
        return Some(ridge());
    }
    if id == "spine" {
        return Some(spine());
    }
    let id = RENAMED
        .iter()
        .find(|(old, _)| *old == id)
        .map_or(id, |(_, new)| new);
    presets().into_iter().find(|preset| preset.id == id)
}

/// The bundled wallpaper a look shows, by palette name.
fn bundled(preset: &Preset) -> Option<&str> {
    match preset
        .profile
        .wallpaper
        .as_ref()?
        .background
        .as_ref()?
        .source
        .as_ref()?
    {
        super::desktop_look::SourceLook::Bundled(name) => Some(name),
        _ => None,
    }
}

fn installed(preset: &Preset, bundled_dir: &std::path::Path) -> bool {
    bundled(preset)
        .is_none_or(|name| crate::desktop::wallpaper::bundled_path(bundled_dir, name).is_file())
}

/// Whether Islands is installed, independently of enablement or runtime health.
pub fn islands_available(store: &crate::desktop::DesktopStore) -> bool {
    crate::desktop::islands::Installation::discover(store).installed()
}

/// Keep a person's selected system readings. The recipe controls visibility
/// and position, not the information they've configured inside a widget.
fn keep_widget_content(preset: &mut Preset, config: &crate::config::Config) {
    if config.saved.devbox
        && let Some(Value::Object(style)) = preset.profile.widget_style.as_mut()
    {
        style.remove("devbox");
    }
}

/// `preset` adapted to this person: Projects joins only when there are
/// project folders to show, instead of an empty card; a look whose
/// bundled wallpaper isn't installed (in `bundled_dir`) keeps the current
/// wallpaper; and without the Islands component (`islands` false) the
/// panel stays one bar in the look's shape. None of these refuse the look.
pub fn fitted(
    mut preset: Preset,
    config: &crate::config::Config,
    bundled_dir: &std::path::Path,
    islands: bool,
) -> Preset {
    if config.project_roots.is_empty()
        && let Some(Value::Array(widgets)) = preset
            .profile
            .widget_layout
            .as_mut()
            .and_then(|layout| layout.get_mut("enabled_widgets"))
    {
        widgets.retain(|widget| widget != "projects");
    }
    if preset.id != CLEAN {
        keep_widget_content(&mut preset, config);
    }
    if !installed(&preset, bundled_dir) {
        preset.profile.wallpaper = None;
        if let Some(style) = preset.profile.widget_style.as_mut() {
            // With no bundled picture, retain the current wallpaper's ink.
            style["appearance"]["wallpaper_light"] = config.appearance.wallpaper_light.into();
        }
    }
    if !islands {
        for look in [&mut preset.profile.dock, &mut preset.profile.panel]
            .into_iter()
            .flatten()
        {
            look.islands = None;
            look.island_gap = None;
        }
    }
    preset
}

/// What a starter look still needs from the user to look complete.
pub fn missing(
    preset: &Preset,
    bundled_dir: &std::path::Path,
    _islands: bool,
) -> Option<&'static str> {
    if !installed(preset, bundled_dir) {
        Some(
            "The built-in wallpapers aren’t installed, so your wallpaper stays. Reinstall Halcyon to restore them.",
        )
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn fb13_spine_stays_out_of_wizard_and_blocks_vertical_islands_without_mutation() {
        use crate::desktop::panels::{Anchor, PanelDraft, PanelKind};
        use crate::desktop::profiles::{Domain, Installed, apply, plan};
        let spine = super::preset("spine").unwrap();
        assert!(super::presets().iter().all(|p| p.id != "spine"));
        assert_eq!(super::studio_presets().len(), super::presets().len() + 2);
        assert_eq!(
            spine.profile.panel.as_ref().unwrap().anchor,
            Some(Anchor::Left)
        );
        let (fixture, store) = desk("spine");
        crate::desktop::islands::fixture_installed(&fixture.0);
        let key = crate::desktop::islands::key(PanelKind::Panel);
        let mut current = dock_islands_core::Settings::default();
        current.enabled = true;
        fixture.user(
            key.namespace(),
            key.key(),
            &ron::to_string(&current).unwrap(),
        );
        let selected = [Domain::Panel].into_iter().collect();
        let before = fixture.tree();
        let blocked = plan(
            &store,
            &Installed::discover(&store),
            &spine.profile,
            &selected,
        );
        assert!(!blocked.can_apply());
        assert!(
            blocked
                .blocking_reasons()
                .iter()
                .any(|s| s.contains("Turn off Islands"))
        );
        assert_eq!(fixture.tree(), before);
        current.enabled = false;
        fixture.user(
            key.namespace(),
            key.key(),
            &ron::to_string(&current).unwrap(),
        );
        let ready = plan(
            &store,
            &Installed::discover(&store),
            &spine.profile,
            &selected,
        );
        assert!(ready.can_apply(), "{:?}", ready.blocking_reasons());
        assert!(
            ready.patch().guards().any(|(guard, _)| guard == &key),
            "enablement must be guarded"
        );
        current.enabled = true;
        fixture.user(
            key.namespace(),
            key.key(),
            &ron::to_string(&current).unwrap(),
        );
        assert!(
            apply(&store, &ready).is_err(),
            "concurrent enable must conflict"
        );
        current.enabled = false;
        fixture.user(
            key.namespace(),
            key.key(),
            &ron::to_string(&current).unwrap(),
        );
        let ready = plan(
            &store,
            &Installed::discover(&store),
            &spine.profile,
            &selected,
        );
        let before = PanelDraft::load(&store, PanelKind::Panel).unwrap();
        apply(&store, &ready).unwrap();
        assert_eq!(
            PanelDraft::load(&store, PanelKind::Panel)
                .unwrap()
                .anchor()
                .known(),
            Some(&Anchor::Left)
        );
        assert_eq!(
            PanelDraft::load(&store, PanelKind::Panel)
                .unwrap()
                .applets(),
            before.applets()
        );
        store
            .undo_latest(&store.latest_change().unwrap().unwrap().id)
            .unwrap();
        assert_eq!(
            PanelDraft::load(&store, PanelKind::Panel).unwrap().anchor(),
            before.anchor()
        );
    }

    use super::*;
    use crate::desktop::profiles::{Domain, Installed, ProfileLibrary, apply, plan};
    use crate::desktop::store::tests::Fixture;
    use std::collections::BTreeSet;

    const DOCK: &str = "com.system76.CosmicPanel.Dock";
    const PANEL: &str = "com.system76.CosmicPanel.Panel";

    fn desk(name: &str) -> (Fixture, crate::desktop::DesktopStore) {
        let fixture = Fixture::new(name);
        fixture.system(
            "com.system76.CosmicPanel",
            "entries",
            "[\"Panel\",\"Dock\"]",
        );
        fixture.system(DOCK, "anchor", "Bottom");
        fixture.system(DOCK, "size", "L");
        fixture.system(DOCK, "size_wings", "Some((Some(XL), None))");
        fixture.system(DOCK, "output", "Name(\"DP-1\")");
        fixture.system(
            DOCK,
            "plugins_center",
            "Some([\"com.system76.CosmicAppList\"])",
        );
        fixture.system(PANEL, "anchor", "Top");
        fixture.user("com.system76.CosmicTheme.Mode", "is_dark", "true");
        // The installer puts the bundled wallpapers in Pictures/Desktop Cards.
        let bundled = fixture.0.join("pictures/Desktop Cards");
        std::fs::create_dir_all(&bundled).unwrap();
        for (name, _) in crate::desktop::wallpaper::BUNDLED {
            std::fs::copy(
                crate::desktop::wallpaper::bundled_path(
                    &std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("assets/wallpapers"),
                    name,
                ),
                crate::desktop::wallpaper::bundled_path(&bundled, name),
            )
            .unwrap();
        }
        let config = fixture.0.join("app/config.json");
        std::fs::create_dir_all(config.parent().unwrap()).unwrap();
        std::fs::write(
            &config,
            r#"{"theme":"teal","location":{"name":"SENTINEL","latitude":1.0,"longitude":2.0},
                "project_roots":["/home/someone/work"],"enabled_widgets":["calendar","projects","devbox","weather","clock"],
                "glance":{"enabled":true,"show_calendar":true,"show_weather":true},
                "appearance":{"match_wallpaper":true,"match_cosmic_colors":true},"future_field":7}"#,
        )
        .unwrap();
        let store = fixture.store().with_app_config(config);
        (fixture, store)
    }

    #[test]
    fn fb10_new_looks_are_versioned_portable_choices_and_keep_legacy_ids() {
        for (id, face, surface) in [
            ("tidal", "digital", "none"),
            ("paper-studio", "analog", "panel"),
            ("moss", "digital", "panel"),
            ("carbon", "digital", "none"),
        ] {
            let p = preset(id).unwrap();
            assert_eq!(p.profile.version, super::super::format::VERSION_9);
            let style = p.profile.widget_style.as_ref().unwrap();
            assert_eq!(style["clock"]["face"], face);
            assert_eq!(style["clock"]["surface"], surface);
            assert!(style.get("location").is_none());
            assert!(style.get("notes").is_none());
            let exported = super::super::export(&p.profile).unwrap();
            assert!(exported.omitted.is_empty());
            let loaded = import(&exported.bytes).unwrap();
            assert_eq!(loaded.widget_style, p.profile.widget_style);
            assert_eq!(loaded.widget_layout, p.profile.widget_layout);
            assert!(
                serde_json::to_value(loaded.panel.unwrap())
                    .unwrap()
                    .get("output")
                    .is_none()
            );
        }
        for id in ["halcyon", "calm", "evening", "focus", "paper", "rose"] {
            let p = preset(id).unwrap();
            assert!(
                p.profile.widget_style.unwrap().get("clock").is_none(),
                "{id}"
            );
        }
        assert_eq!(preset("paper").unwrap().profile.name, "Writing desk");
        assert_eq!(preset("minimal").unwrap(), preset("focus").unwrap());
    }

    #[test]
    fn fb10_looks_replace_explicit_color_policy_and_clock_style_without_content_loss() {
        let (_fixture, store) = desk("fb10-colors");
        let path = store.app_config().unwrap();
        let mut value: Value = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
        value["appearance"]["color_policy"] = json!("custom");
        value["clock_font"] = json!("Uninstalled personal font");
        value["clock"] =
            json!({"face":"analog","surface":"panel","format_24h":false,"show_date":false});
        let before = serde_json::to_vec_pretty(&value).unwrap();
        std::fs::write(&path, &before).unwrap();
        let installed = Installed::discover(&store);
        let dir = crate::desktop::wallpaper::bundled_dir(&store);
        for (id, face) in [
            ("tidal", "digital"),
            ("paper-studio", "analog"),
            ("moss", "digital"),
            ("carbon", "digital"),
        ] {
            let config = crate::config::Config::from_bytes(&before).unwrap();
            let p = fitted(preset(id).unwrap(), &config, &dir, false).profile;
            let change = apply(
                &store,
                &plan(&store, &installed, &p, &p.domains().into_iter().collect()),
            )
            .unwrap();
            let current: Value = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
            assert_eq!(current["appearance"]["color_policy"], "wallpaper", "{id}");
            assert_eq!(current["clock"]["face"], face, "{id}");
            assert_eq!(current["clock"]["format_24h"], false);
            assert_eq!(current["clock"]["show_date"], false);
            assert!(current["clock_font"].is_null());
            for key in ["location", "project_roots", "future_field"] {
                assert_eq!(current[key], value[key]);
            }
            store.undo_latest(&change.id).unwrap();
            assert_eq!(std::fs::read(&path).unwrap(), before, "{id}: Undo");
        }
    }

    #[test]
    fn writing_desk_and_dark_setup_apply_readable_wallpaper_ink() {
        let (_fixture, store) = desk("preset-wallpaper-ink");
        let installed = Installed::discover(&store);
        let dir = crate::desktop::wallpaper::bundled_dir(&store);
        for (id, light) in [
            ("paper", true),
            ("halcyon", false),
            ("tidal", false),
            ("paper-studio", true),
            ("moss", false),
            ("carbon", false),
        ] {
            let config = crate::config::Config::from_bytes(
                &std::fs::read(store.app_config().unwrap()).unwrap(),
            )
            .unwrap();
            let p = fitted(preset(id).unwrap(), &config, &dir, false).profile;
            apply(
                &store,
                &plan(&store, &installed, &p, &p.domains().into_iter().collect()),
            )
            .unwrap();
            // Settings acknowledges a setup's colors; they must already be
            // correct without waiting for a later wallpaper change.
            crate::desktop::wallpaper::automatic::keep_manual_colors(&store).unwrap();
            let applied = crate::config::Config::from_bytes(
                &std::fs::read(store.app_config().unwrap()).unwrap(),
            )
            .unwrap();
            assert_eq!(applied.appearance.wallpaper_light, light);
            let ink = crate::desktop_theme::luminance(applied.wallpaper_palette().foreground);
            assert!(if light { ink < 0.15 } else { ink > 0.8 }, "{id}: {ink}");
        }
    }

    #[test]
    fn presets_are_distinct_valid_complete_looks_and_loading_writes_nothing() {
        let (fixture, store) = desk("presets-valid");
        let before = fixture.tree();
        let all = presets();
        let ids: Vec<_> = all.iter().map(|preset| preset.id).collect();
        assert_eq!(
            ids,
            [
                "halcyon",
                "calm",
                "evening",
                "focus",
                "paper",
                "rose",
                "tidal",
                "paper-studio",
                "moss",
                "carbon",
                CLEAN
            ]
        );
        let installed = Installed::discover(&store);
        let dir = crate::desktop::wallpaper::bundled_dir(&store);
        let config = crate::config::Config::default();
        let mut seen = Vec::new();
        for preset in &all {
            let profile = &preset.profile;
            profile.validate().unwrap();
            // Fonts, icons, monitors and applets stay the user's.
            if let Some(look) = &profile.appearance {
                assert!(look.interface_font.is_none() && look.monospace_font.is_none());
                assert!(look.icon_theme.is_none());
            }
            for look in [&profile.dock, &profile.panel].into_iter().flatten() {
                assert!(look.applets.is_none(), "{}", preset.id);
            }
            if preset.id != CLEAN {
                // A starter look is a whole desktop: wallpaper, style, colors, widgets.
                let domains = profile.domains();
                for domain in [
                    Domain::Wallpaper,
                    Domain::WidgetStyle,
                    Domain::Appearance,
                    Domain::WidgetLayout,
                    Domain::Dock,
                ] {
                    assert!(domains.contains(&domain), "{} lacks {domain:?}", preset.id);
                }
                assert!(super::themes::family(profile).is_some(), "{}", preset.id);
                let wallpaper = serde_json::to_value(&profile.wallpaper).unwrap();
                let name = wallpaper["background"]["source"]["bundled"]
                    .as_str()
                    .unwrap();
                assert!(
                    std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                        .join(format!("assets/wallpapers/{name}.jpg"))
                        .exists(),
                    "{name}"
                );
                assert!(
                    crate::desktop::wallpaper::bundled_name(name).is_some(),
                    "{name}"
                );
                let style = profile.widget_style.as_ref().unwrap();
                assert!(
                    crate::config::Palette::CHOICES
                        .iter()
                        .any(|(key, _)| style["theme"] == *key),
                    "{}",
                    preset.id
                );
                // Colors keep following the wallpaper after the look is applied.
                assert_eq!(
                    style["appearance"]["wallpaper_colors"], true,
                    "{}",
                    preset.id
                );
                // Every recipe uses installed native bars, never applet rewrites.
                let panel = profile.panel.as_ref().unwrap();
                assert_eq!(
                    panel.islands.as_ref(),
                    islands_composition(preset.id).as_ref()
                );
                assert!(panel.anchor.is_none());
                assert!(profile.dock.as_ref().unwrap().islands.is_none());
                for bar in [panel, profile.dock.as_ref().unwrap()] {
                    assert!(matches!(
                        bar.background,
                        Some(crate::desktop::panels::Background::ThemeDefault)
                    ));
                }
                let layout = profile.widget_layout.as_ref().unwrap();
                assert_eq!(layout["glance"]["show_calendar"], false);
                for widget in ["music", "notes", "focus"] {
                    assert!(layout[widget]["enabled"].is_boolean());
                    assert_eq!(
                        layout[widget].as_object().unwrap().len(),
                        1,
                        "visibility only"
                    );
                }
                // The installed COSMIC panel crashes with a frosted background.
                let raw = serde_json::to_string(profile).unwrap();
                assert!(!raw.contains("\"panel\":true"), "{}", preset.id);
            }
            // This fixture has no Islands component, so the looks keep a bar.
            let profile = &fitted(preset.clone(), &config, &dir, false).profile;
            if preset.id != CLEAN {
                assert!(profile.panel.as_ref().unwrap().islands.is_none());
                assert!(
                    missing(preset, &dir, false).is_none(),
                    "absent add-ons have one link in App settings"
                );
            }
            let selected: BTreeSet<Domain> = profile.domains().into_iter().collect();
            let plan = plan(&store, &installed, profile, &selected);
            assert!(
                plan.can_apply(),
                "{}: {:?}",
                preset.id,
                plan.blocking_reasons()
            );
            let changes: Vec<_> = plan
                .domains
                .iter()
                .flat_map(|domain| domain.changes.clone())
                .collect();
            assert!(!seen.contains(&changes), "{} repeats a preset", preset.id);
            seen.push(changes);
        }
        assert!(
            ProfileLibrary::new(&store).list().unwrap().is_empty(),
            "presets are not library entries"
        );
        assert_eq!(
            fixture.tree(),
            before,
            "loading and planning presets writes nothing"
        );
    }

    #[test]
    fn setup_islands_are_distinct_and_fit_short_long_and_empty_groups() {
        use dock_islands_core::{Span, resolve};
        let mut signatures = std::collections::BTreeSet::new();
        for id in ["halcyon", "focus", "paper", "rose", "calm", "evening"] {
            let settings = islands_composition(id).unwrap();
            settings.validate().unwrap();
            // Private native acceptance consumes the exact compiled recipes.
            if let Some(path) = std::env::var_os("HALCYON_SETUP_RECIPE_EVIDENCE") {
                let path = std::path::PathBuf::from(path);
                std::fs::create_dir_all(&path).unwrap();
                std::fs::write(
                    path.join(format!("{id}.json")),
                    serde_json::to_vec_pretty(&settings).unwrap(),
                )
                .unwrap();
                std::fs::write(
                    path.join(format!("{id}.ron")),
                    ron::to_string(&settings).unwrap(),
                )
                .unwrap();
            }
            assert_ne!(
                settings.default_layout,
                Arrangement::default(),
                "{id} must request its composition"
            );
            assert!(settings.outputs.is_empty());
            let layout = resolve(
                &settings,
                "",
                Span {
                    start: 0.,
                    length: 1920.,
                },
                [96., 80., 224.],
                32.,
            );
            assert!(
                signatures.insert(format!("{:?}", layout.spans)),
                "{id} repeats another arrangement"
            );
            for width in [240., 480., 900., 1920.] {
                for natural in [
                    [96., 80., 224.],
                    [0., 80., 224.],
                    [4000., 40., 800.],
                    [0., 0., 80.],
                ] {
                    let layout = resolve(
                        &settings,
                        "",
                        Span {
                            start: 0.,
                            length: width,
                        },
                        natural,
                        32.,
                    );
                    let mut spans: Vec<_> = layout.spans.into_iter().flatten().collect();
                    spans.sort_by(|a, b| a.start.total_cmp(&b.start));
                    assert!(
                        spans
                            .iter()
                            .all(|s| s.start >= -0.001 && s.end() <= width + 0.001),
                        "{id}: {spans:?}"
                    );
                    assert!(
                        spans
                            .windows(2)
                            .all(|pair| pair[0].end() <= pair[1].start + 0.001),
                        "{id}: {spans:?}"
                    );
                    for (i, n) in natural.into_iter().enumerate() {
                        assert_eq!(layout.spans[i].is_some(), n > 0.);
                    }
                }
            }
        }
    }

    #[test]
    fn setup_islands_plan_preview_and_undo_preserve_applets_enablement_and_local_layouts() {
        use crate::desktop::panels::{PanelDraft, PanelKind};
        let (fixture, store) = desk("setup-islands-plan");
        crate::desktop::islands::fixture_installed(&fixture.0);
        fixture.system(
            PANEL,
            "plugins_wings",
            "Some(([\"custom.Start\"],[\"custom.End\",\"custom.End\"]))",
        );
        fixture.system(PANEL, "plugins_center", "Some([\"custom.Center\"])");
        let key = crate::desktop::islands::key(PanelKind::Panel);
        for enabled in [false, true] {
            let mut current = Settings {
                enabled,
                ..Default::default()
            };
            current.default_layout.together.position = 1234;
            current
                .outputs
                .insert("DP-1".into(), current.default_layout.clone());
            fixture.user(
                key.namespace(),
                key.key(),
                &ron::to_string(&current).unwrap(),
            );
            let baseline = PanelDraft::load(&store, PanelKind::Panel).unwrap();
            for id in ["halcyon", "focus", "paper", "rose", "calm", "evening"] {
                let preset = preset(id).unwrap();
                let look = preset.profile.panel.as_ref().unwrap();
                let selected = [Domain::Panel].into_iter().collect();
                let proposal = plan(
                    &store,
                    &Installed::discover(&store),
                    &preset.profile,
                    &selected,
                );
                assert!(
                    proposal.can_apply(),
                    "{id}: {:?}",
                    proposal.blocking_reasons()
                );
                let desired = proposal
                    .patch()
                    .get(&key)
                    .unwrap()
                    .desired
                    .as_ref()
                    .unwrap()
                    .decode::<Settings>()
                    .unwrap();
                assert_eq!(desired.enabled, enabled);
                assert_eq!(
                    desired.default_layout,
                    look.islands.as_ref().unwrap().default_layout
                );
                assert_eq!(desired.placement, look.islands.as_ref().unwrap().placement);
                assert_eq!(desired.outputs, current.outputs);
                assert!(
                    proposal
                        .patch()
                        .keys()
                        .all(|k| !k.key().starts_with("plugins_") && k.key() != "output")
                );
                let preview = super::super::preview_panel_style(baseline.clone(), look, true);
                assert_eq!(preview.islands().known(), Some(&desired));
                assert_eq!(preview.applets(), baseline.applets());
                let applied = apply(&store, &proposal).unwrap();
                assert_eq!(
                    PanelDraft::load(&store, PanelKind::Panel)
                        .unwrap()
                        .islands()
                        .known(),
                    Some(&desired)
                );
                store.undo_latest(&applied.id).unwrap();
                let restored = PanelDraft::load(&store, PanelKind::Panel).unwrap();
                assert_eq!(restored.islands().known(), Some(&current));
                assert_eq!(restored.applets(), baseline.applets());
                let excluded = plan(
                    &store,
                    &Installed::discover(&store),
                    &preset.profile,
                    &[Domain::WidgetStyle].into_iter().collect(),
                );
                assert!(excluded.patch().get(&key).is_none());
            }
        }
    }

    #[test]
    fn earlier_names_resolve_and_looks_fit_what_this_desktop_can_show() {
        assert_eq!(preset("minimal").unwrap().id, "focus");
        assert_eq!(preset("glass").unwrap().id, "evening");
        assert_eq!(preset("workbench").unwrap().id, "halcyon");
        assert!(preset("nonexistent").is_none());
        let (fixture, store) = desk("presets-fitted");
        let dir = crate::desktop::wallpaper::bundled_dir(&store);
        let config = crate::config::Config::default();
        let workbench = preset("calm").unwrap();
        assert!(missing(&workbench, &dir, true).is_none());
        assert!(missing(&workbench, &dir, false).is_none());
        let with = fitted(workbench.clone(), &config, &dir, true);
        assert!(with.profile.wallpaper.is_some());
        assert!(with.profile.panel.as_ref().unwrap().islands.is_some());
        // Without Islands the panel keeps its pill shape as one bar.
        let bar = fitted(workbench.clone(), &config, &dir, false);
        let panel = bar.profile.panel.as_ref().unwrap();
        assert!(panel.islands.is_none());
        assert_eq!(panel.border_radius, Some(PILL));
        // Without the bundled wallpapers the rest of the look still applies.
        std::fs::remove_dir_all(fixture.0.join("pictures")).unwrap();
        assert!(
            missing(&workbench, &dir, true)
                .unwrap()
                .contains("Reinstall Halcyon")
        );
        let without = fitted(workbench, &config, &dir, true);
        assert!(without.profile.wallpaper.is_none());
        assert!(without.profile.widget_style.is_some());
    }

    #[test]
    fn presentation_hides_private_cards_and_a_look_sets_the_whole_desktop() {
        let (fixture, store) = desk("presets-apply");
        let installed = Installed::discover(&store);
        let presentation = preset(CLEAN).unwrap();
        let selected: BTreeSet<Domain> = presentation.profile.domains().into_iter().collect();
        apply(
            &store,
            &plan(&store, &installed, &presentation.profile, &selected),
        )
        .unwrap();
        let config: crate::config::Config =
            serde_json::from_slice(&std::fs::read(store.app_config().unwrap()).unwrap()).unwrap();
        use crate::drawer::Widget;
        for widget in [
            Widget::Projects,
            Widget::Calendar,
            Widget::Weather,
            Widget::Devbox,
            Widget::Glance,
        ] {
            assert!(!config.widget_visible(widget), "{widget:?} still shows");
        }
        assert!(!config.glance.show_calendar && !config.glance.show_weather);
        // Private and unrelated members and palette links are untouched.
        let raw = std::fs::read_to_string(store.app_config().unwrap()).unwrap();
        for kept in ["SENTINEL", "/home/someone/work", "future_field", "teal"] {
            assert!(raw.contains(kept), "{kept} lost");
        }
        assert!(config.appearance.match_wallpaper && config.appearance.match_cosmic_colors);

        let dir = crate::desktop::wallpaper::bundled_dir(&store);
        let calm = fitted(preset("calm").unwrap(), &Default::default(), &dir, false);
        let selected: BTreeSet<Domain> = calm.profile.domains().into_iter().collect();
        apply(&store, &plan(&store, &installed, &calm.profile, &selected)).unwrap();
        let config: crate::config::Config =
            serde_json::from_slice(&std::fs::read(store.app_config().unwrap()).unwrap()).unwrap();
        assert_eq!(config.appearance.family, Some(Family::Soft));
        assert_eq!(config.appearance.accent, Some([0x6a, 0xc2, 0xc6]));
        assert!(config.appearance.wallpaper_colors);
        assert!(config.glance.enabled && config.glance.own_background);
        assert!(config.widget_visible(Widget::Notes));
        assert!(!config.widget_visible(Widget::Devbox));
        assert!(!config.glance.show_calendar);
        assert!(config.devbox.gpu && config.devbox.uptime && !config.devbox.services);
        // After Clean desktop nothing of theirs shows, so only the look's.
        assert!(!config.widget_visible(Widget::Projects));
        // The monitor binding and the applets stay the target's.
        assert_eq!(fixture.read(DOCK, "output"), None);
        assert_eq!(fixture.read(DOCK, "plugins_center"), None);
        let background = std::fs::read_to_string(
            fixture
                .0
                .join("config/cosmic/com.system76.CosmicBackground/v1/all"),
        )
        .unwrap_or_default();
        assert!(background.contains("halcyon-calm"), "{background}");
    }

    #[test]
    fn setups_replace_composition_but_preserve_content_and_custom_readings() {
        let dir = std::path::Path::new("/nonexistent");
        let own = crate::config::Config::from_bytes(
            br#"{"enabled_widgets":["calendar","weather"],"glance":{"enabled":false},"devbox":{"swap":false}}"#,
        ).unwrap();
        let slow = fitted(preset("calm").unwrap(), &own, dir, false);
        let layout = slow.profile.widget_layout.as_ref().unwrap();
        assert_eq!(layout["enabled_widgets"], json!([]));
        assert_eq!(layout["glance"]["enabled"], true);
        assert_eq!(layout["notes"]["enabled"], true);
        assert!(
            slow.profile
                .widget_style
                .as_ref()
                .unwrap()
                .get("devbox")
                .is_none()
        );
        let mut seen = std::collections::BTreeSet::new();
        for id in ["halcyon", "calm", "evening", "focus", "paper", "rose"] {
            let p = fitted(preset(id).unwrap(), &own, dir, false);
            let v = p.profile.widget_layout.as_ref().unwrap();
            let signature = json!([
                v["enabled_widgets"],
                v["glance"]["enabled"],
                v["notes"]["enabled"],
                v["music"]["enabled"],
                v["focus"]["enabled"]
            ])
            .to_string();
            assert!(seen.insert(signature), "{id} repeats another composition");
            assert!(!v.to_string().contains("text"));
            assert!(!v.to_string().contains("duration"));
        }
        let studio = fitted(preset("rose").unwrap(), &own, dir, false);
        assert!(
            !studio.profile.widget_layout.as_ref().unwrap()["enabled_widgets"]
                .as_array()
                .unwrap()
                .contains(&json!("projects"))
        );
        let mut projects = own.clone();
        projects.project_roots.push("/my/project".into());
        let studio = fitted(preset("rose").unwrap(), &projects, dir, false);
        assert!(
            studio.profile.widget_layout.as_ref().unwrap()["enabled_widgets"]
                .as_array()
                .unwrap()
                .contains(&json!("projects"))
        );
    }

    #[test]
    fn curated_layout_can_be_excluded_and_every_setup_can_be_undone() {
        let (fixture, store) = desk("curated-undo");
        let installed = Installed::discover(&store);
        let dir = crate::desktop::wallpaper::bundled_dir(&store);
        let config =
            crate::config::Config::from_bytes(&std::fs::read(store.app_config().unwrap()).unwrap())
                .unwrap();
        let original = std::fs::read(store.app_config().unwrap()).unwrap();
        for id in [
            "tidal",
            "paper-studio",
            "moss",
            "carbon",
            "halcyon",
            "calm",
            "evening",
            "focus",
            "paper",
            "rose",
        ] {
            let p = fitted(preset(id).unwrap(), &config, &dir, false).profile;
            let selected = p.domains().into_iter().collect();
            let plan = plan(&store, &installed, &p, &selected);
            assert!(plan.can_apply(), "{id}: {:?}", plan.blocking_reasons());
            let applied = apply(&store, &plan).unwrap();
            let bytes = std::fs::read(store.app_config().unwrap()).unwrap();
            let current: Value = serde_json::from_slice(&bytes).unwrap();
            for key in ["location", "project_roots", "future_field"] {
                assert_eq!(
                    current[key],
                    serde_json::from_slice::<Value>(&original).unwrap()[key],
                    "{id}/{key}"
                );
            }
            store.undo_latest(&applied.id).unwrap();
            assert_eq!(
                std::fs::read(store.app_config().unwrap()).unwrap(),
                original,
                "{id} undo"
            );
            assert_eq!(fixture.read(DOCK, "plugins_center"), None);
            let selected = p
                .domains()
                .into_iter()
                .filter(|d| *d != Domain::WidgetLayout)
                .collect();
            let filtered = super::super::plan(&store, &installed, &p, &selected);
            assert!(!filtered.widget_patch().changes().any(|(pointer, _)| {
                pointer.starts_with("/enabled_widgets")
                    || pointer.ends_with("/enabled")
                    || pointer.starts_with("/widget_positions/")
            }));
        }
    }
    #[test]
    fn task2_imported_islands_fields_adapt_without_sidecars_or_activation() {
        let (fixture, store) = desk("imported-islands-adaptation");
        let mut profile = preset("halcyon").unwrap().profile;
        profile.panel.as_mut().unwrap().island_gap = Some(42);
        let installed = Installed::discover(&store);
        let selected = [Domain::Panel].into_iter().collect();
        let proposal = plan(&store, &installed, &profile, &selected);
        assert!(
            !proposal
                .patch()
                .keys()
                .any(|k| k.namespace() == dock_islands_core::NAMESPACE)
        );
        assert!(proposal.can_apply(), "{:?}", proposal.blocking_reasons());
        crate::desktop::profiles::apply(&store, &proposal).unwrap();
        assert!(
            !store
                .user_path(&crate::desktop::islands::key(
                    crate::desktop::panels::PanelKind::Panel
                ))
                .exists()
        );
        crate::desktop::islands::fixture_installed(&fixture.0);
        let proposal = plan(&store, &installed, &profile, &selected);
        for (_, entry) in proposal
            .patch()
            .iter()
            .filter(|(k, _)| k.namespace() == dock_islands_core::NAMESPACE)
        {
            assert!(
                !entry
                    .desired
                    .as_ref()
                    .unwrap()
                    .decode::<dock_islands_core::Settings>()
                    .unwrap()
                    .enabled
            );
        }
    }
    #[test]
    fn task2_installed_disabled_gap_can_be_managed_without_runtime() {
        let (fixture, store) = desk("offline-gap");
        crate::desktop::islands::fixture_installed(&fixture.0);
        let mut profile = super::super::Profile::new("Spacing").unwrap();
        profile.panel = Some(super::super::PanelLook {
            island_gap: Some(42),
            ..Default::default()
        });
        let proposal = plan(
            &store,
            &Installed::discover(&store),
            &profile,
            &[Domain::Panel].into_iter().collect(),
        );
        let key = crate::desktop::islands::key(crate::desktop::panels::PanelKind::Panel);
        let setting = proposal
            .patch()
            .get(&key)
            .expect("installed spacing is manageable")
            .desired
            .as_ref()
            .unwrap()
            .decode::<dock_islands_core::Settings>()
            .unwrap();
        assert_eq!(setting.gap, 42);
        assert!(!setting.enabled);
    }

    #[test]
    fn task2_explicit_islands_choice_retains_the_plans_original_guard() {
        let (fixture, store) = desk("wizard-islands-guard");
        crate::desktop::islands::fixture_installed(&fixture.0);
        let profile = preset("halcyon").unwrap().profile;
        let mut proposal = plan(
            &store,
            &Installed::discover(&store),
            &profile,
            &[Domain::Panel].into_iter().collect(),
        );
        let key = crate::desktop::islands::key(crate::desktop::panels::PanelKind::Panel);
        let mut changed = dock_islands_core::Settings::default();
        changed.gap = 17;
        fixture.user(
            key.namespace(),
            key.key(),
            &ron::to_string(&changed).unwrap(),
        );
        proposal
            .choose_islands(&store, crate::desktop::panels::PanelKind::Panel, true)
            .unwrap();
        assert!(
            crate::desktop::profiles::apply(&store, &proposal).is_err(),
            "explicit enable must not erase a concurrent settings edit"
        );
    }
}
