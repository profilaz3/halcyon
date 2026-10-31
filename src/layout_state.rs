//! Small runtime geometry snapshot shared with the Settings placement preview.
use crate::{
    config::Config,
    drawer::{self, Widget},
    layout,
};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::{
    Mutex,
    atomic::{AtomicU64, Ordering},
};

static GENERATION: AtomicU64 = AtomicU64::new(0);
static PUBLISHED: Mutex<u64> = Mutex::new(0);

pub fn next_generation() -> u64 {
    GENERATION.fetch_add(1, Ordering::Relaxed) + 1
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WidgetGeometry {
    pub key: String,
    pub size: [f32; 2],
    #[serde(default)]
    pub natural_height: Option<f32>,
    #[serde(default)]
    pub natural_size: Option<[f32; 2]>,
    pub position: [f32; 2],
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DisplayState {
    pub output: Option<String>,
    pub ready: bool,
    pub epoch: u64,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Snapshot {
    #[serde(default)]
    pub monitors: std::collections::BTreeMap<String, [f32; 2]>,
    #[serde(default)]
    pub widget_displays: std::collections::BTreeMap<String, String>,
    #[serde(default)]
    pub display: Option<DisplayState>,
    pub screen: [f32; 2],
    pub widgets: Vec<WidgetGeometry>,
}

pub fn path() -> Option<PathBuf> {
    if std::env::var_os("DESKTOP_CARDS_CONFIG").is_some() {
        Config::path().map(|path| path.with_file_name("layout-state.json"))
    } else {
        crate::paths::runtime_dir().map(|dir| dir.join("layout-state.json"))
    }
}

fn cached() -> Option<Snapshot> {
    let bytes = std::fs::read(path()?).ok()?;
    let state: Snapshot = serde_json::from_slice(&bytes).ok()?;
    state.valid().then(|| {
        let mut state = state;
        // Older publishers measured effective list heights in `size` only.
        // Capture that evidence before an automatic-size projection replaces it.
        for g in &mut state.widgets {
            if is_content_sized(Widget::parse(&g.key).unwrap()) && g.natural_height.is_none() {
                g.natural_height = Some(g.size[1].max(g.natural_size.map_or(0., |s| s[1])));
            }
        }
        state
    })
}

impl Snapshot {
    fn valid(&self) -> bool {
        self.monitors.len() <= 32
            && self.monitors.iter().all(|(k, s)| {
                crate::displays::valid_name(k)
                    && s.iter().all(|v| v.is_finite() && *v >= 1. && *v <= 32768.)
            })
            && self.widget_displays.len() <= Widget::ALL.len()
            && self
                .widget_displays
                .iter()
                .all(|(k, d)| Widget::parse(k).is_some() && crate::displays::valid_name(d))
            && self
                .screen
                .iter()
                .all(|v| v.is_finite() && *v >= 1. && *v <= 32768.)
            && self.widgets.len() == Widget::ALL.len()
            && Widget::ALL
                .iter()
                .all(|w| self.widgets.iter().filter(|g| g.key == w.key()).count() == 1)
            && self.widgets.iter().all(|g| {
                g.size
                    .iter()
                    .all(|v| v.is_finite() && *v > 0. && *v <= 32768.)
                    && g.natural_size.is_none_or(crate::layout::valid_size)
                    && g.natural_height
                        .is_none_or(|height| height.is_finite() && height > 0.)
                    && g.position.iter().all(|v| v.is_finite())
            })
    }
}

/// A public reference canvas. Never reads the desktop geometry cache.
pub(crate) fn public_preview(config: &Config) -> Snapshot {
    project(default_positions(fallback(config), config), config)
}

fn fallback(config: &Config) -> Snapshot {
    Snapshot {
        monitors: Default::default(),
        widget_displays: Default::default(),
        display: None,
        screen: [1920., 1080.],
        widgets: Widget::ALL
            .into_iter()
            .map(|widget| WidgetGeometry {
                natural_size: None,
                key: widget.key().into(),
                size: match widget {
                    Widget::Clock => crate::widget_presentation::CLOCK_SIZE,
                    Widget::Calendar => [360., 320.],
                    Widget::Weather => crate::widget_presentation::WEATHER_SIZE,
                    Widget::Devbox => [360., devbox_height(config, 0)],
                    Widget::Projects => [360., 200.],
                    Widget::Glance => [config.glance.auto_width(config.glance_panel()), 220.],
                    tool => crate::tools::cards::size(tool, config),
                },
                natural_height: match widget {
                    Widget::Glance => Some(220.),
                    Widget::Devbox => Some(devbox_height(config, 0)),
                    _ => None,
                },
                position: [0., 0.],
            })
            .collect(),
    }
}

fn size_for_tool(state: &Snapshot, widget: Widget) -> [f32; 2] {
    state
        .widgets
        .iter()
        .find(|g| g.key == widget.key())
        .map(|g| g.size)
        .unwrap_or([280., 160.])
}

fn default_positions(mut state: Snapshot, config: &Config) -> Snapshot {
    // Tool recipes can change their automatic dimensions before the running
    // desktop publishes a new snapshot (including a saved Setup preview).
    for geometry in &mut state.widgets {
        if let Some(widget @ (Widget::Music | Widget::Focus)) = Widget::parse(&geometry.key) {
            let natural = crate::tools::cards::size(widget, config);
            let natural = [
                natural[0].min((state.screen[0] - 48.).max(1.)),
                natural[1].min((state.screen[1] - 96.).max(1.)),
            ];
            geometry.natural_size = Some(natural);
            if !config.widget_sizes.contains_key(&geometry.key) {
                geometry.size = natural;
            }
        }
    }
    let ordered: Vec<_> = config
        .ordered_right_cards()
        .into_iter()
        .filter(|w| {
            config.widget_visible(*w)
                && (state.widget_displays.is_empty()
                    || state.display.as_ref().and_then(|d| d.output.as_ref())
                        == state.widget_displays.get(w.key()))
        })
        .collect();
    let size_for = |widget: Widget| {
        state
            .widgets
            .iter()
            .find(|g| g.key == widget.key())
            .unwrap()
            .size
    };
    let total = ordered.iter().map(|w| size_for(*w)[1]).sum::<f32>()
        + ordered.len().saturating_sub(1) as f32 * drawer::CARD_GAP;
    let mut right_y = (state.screen[1] - total) / 2.;
    for widget in ordered {
        let g = state
            .widgets
            .iter_mut()
            .find(|g| g.key == widget.key())
            .unwrap();
        g.position = layout::clamp(
            [state.screen[0] - g.size[0] - 42., right_y],
            g.size,
            state.screen,
        );
        right_y += g.size[1] + drawer::CARD_GAP;
    }
    let focus_height = size_for_tool(&state, Widget::Focus)[1];
    for g in &mut state.widgets {
        if let Some(tool) = Widget::parse(&g.key).filter(|widget| widget.is_tool()) {
            g.position = layout::tool_position(tool, g.size, focus_height, state.screen);
            continue;
        }
        let point = match g.key.as_str() {
            "clock" | "glance" => [82., 88.],
            "calendar" => [42., (state.screen[1] - g.size[1]) / 2.],
            _ => g.position,
        };
        if g.key == "glance" && !config.widget_sizes.contains_key("glance") {
            g.size[1] = g
                .natural_height
                .unwrap_or(g.size[1])
                .min((state.screen[1] - point[1] - 48.).max(1.));
            g.position = layout::clamp(point, [g.size[0], 1.], state.screen);
        } else {
            g.position = layout::clamp(point, g.size, state.screen);
        }
    }
    state
}

/// Glance grows below its top edge. Persist that edge against a stable vertical
/// reference instead of normalizing it against the changing agenda height.
pub fn encode_position(
    widget: Widget,
    point: [f32; 2],
    size: [f32; 2],
    screen: [f32; 2],
) -> [f32; 2] {
    layout::encode(
        point,
        if widget == Widget::Glance {
            [size[0], 1.]
        } else {
            size
        },
        screen,
    )
}
pub fn decode_position(
    widget: Widget,
    point: [f32; 2],
    size: [f32; 2],
    screen: [f32; 2],
) -> [f32; 2] {
    layout::decode(
        point,
        if widget == Widget::Glance {
            [size[0], 1.]
        } else {
            size
        },
        screen,
    )
}
/// Moving a busy agenda must remain possible. Its viewport shrinks below the
/// new anchor rather than forcing a full-height widget back to the screen top.
pub fn placement_size(widget: Widget, size: [f32; 2]) -> [f32; 2] {
    if widget == Widget::Glance {
        [size[0], size[1].min(180.)]
    } else {
        size
    }
}

/// Settings discovers outputs independently of the optional widget service.
/// Reuse cached content measurements, but bind geometry and save contexts to
/// the live registry even when no layout-state file has ever been published.
pub fn for_display(
    config: &Config,
    display: Option<&str>,
    automatic: bool,
    displays: &crate::displays::Registry,
) -> Snapshot {
    let mut state = cached().unwrap_or_else(|| fallback(config));
    let live = !displays.outputs().is_empty();
    let selected = if live {
        state.monitors = displays
            .outputs()
            .iter()
            .filter_map(|output| {
                let key = displays.key(output)?;
                let size = output
                    .info
                    .logical_size
                    .filter(|(w, h)| *w > 0 && *h > 0)
                    .map(|(w, h)| [w as f32, h as f32])
                    .or_else(|| state.monitors.get(&key).copied())
                    .unwrap_or([1920., 1080.]);
                Some((key, size))
            })
            .collect();
        state.widget_displays = displays.placements(config);
        displays
            .selected(display.or(config.widget_display.as_deref()))
            .and_then(|o| displays.key(o))
    } else {
        display.map(str::to_owned)
    };
    if let Some(name) = selected.as_deref()
        && let Some(screen) = state.monitors.get(name)
    {
        state.screen = *screen;
        state.display = Some(DisplayState {
            output: Some(name.into()),
            ready: true,
            epoch: state.display.as_ref().map(|d| d.epoch).unwrap_or(0),
        });
    }
    let mut config = if live {
        displays.effective_config(config)
    } else {
        config.on_displays(&state.widget_displays)
    };
    for g in &mut state.widgets {
        if let Some(size) = g.natural_size {
            g.size = size;
        }
    }
    if automatic {
        config.widget_sizes.clear();
        config.widget_positions.clear();
    }
    project(state, &config)
}
pub fn defaults(config: &Config) -> Snapshot {
    let mut state = cached().unwrap_or_else(|| fallback(config));
    for g in &mut state.widgets {
        if let Some(size) = g.natural_size {
            g.size = size;
        }
    }
    let mut automatic = config.clone();
    automatic.widget_sizes.clear();
    default_positions(state, &automatic)
}
pub fn read(config: &Config) -> Snapshot {
    project(defaults(config), config)
}

fn is_content_sized(widget: Widget) -> bool {
    matches!(widget, Widget::Devbox | Widget::Projects | Widget::Calendar)
}

/// Shared saved-viewport rule: list cards grow to their content, bounded by
/// the output. Callers retain the saved dimensions; only effective geometry grows.
pub fn effective_explicit_size(
    widget: Widget,
    saved: [f32; 2],
    content_height: Option<f32>,
    screen: [f32; 2],
) -> [f32; 2] {
    let mut size = layout::sized(widget, saved, screen);
    if is_content_sized(widget)
        && let Some(height) = content_height
    {
        size[1] = size[1].max(height.min((screen[1] - 96.).max(1.)));
    }
    size
}

/// Shared automatic geometry for built-in and personal system widgets.
pub fn devbox_height(config: &crate::config::Config, gpu_count: usize) -> f32 {
    config.recipes.devbox.as_ref().map_or_else(
        || devbox_content_height(&config.devbox, gpu_count),
        crate::recipes::Recipe::natural_height,
    )
}

/// System monitor content without starting a metrics sampler. Runtime supplies
/// its GPU count; before its first snapshot Settings assumes the placeholder GPU.
pub fn devbox_content_height(
    preferences: &crate::config::DevboxPreferences,
    gpu_count: usize,
) -> f32 {
    use crate::config::DevboxSection;
    let history = DevboxSection::ALL.into_iter().any(|section| {
        section.enabled(preferences)
            && match section {
                DevboxSection::Cpu => preferences.history.cpu,
                DevboxSection::Memory => preferences.history.memory,
                DevboxSection::Disk => preferences.history.disk,
                DevboxSection::Network => preferences.history.network,
                DevboxSection::Gpu => preferences.history.gpu,
                _ => false,
            }
    });
    let section_height = 16. + 4. + if history { 14. } else { 5. };
    let heights: Vec<f32> = preferences
        .ordered_sections()
        .into_iter()
        .filter(|section| section.enabled(preferences))
        .map(|section| {
            if section == DevboxSection::Gpu {
                let count = gpu_count.max(1);
                count as f32 * section_height + count.saturating_sub(1) as f32 * 8.
            } else {
                section_height
            }
        })
        .collect();
    36. + if heights.is_empty() {
        20.
    } else {
        heights.iter().sum::<f32>() + heights.len().saturating_sub(1) as f32 * 8.
    }
}

/// Project a preview from cached geometry without reading the desktop again.
pub fn project(mut state: Snapshot, config: &Config) -> Snapshot {
    for g in &mut state.widgets {
        if let Some(size) = config.widget_sizes.get(&g.key) {
            let widget = Widget::parse(&g.key).unwrap();
            let content = g.natural_height.or_else(|| {
                is_content_sized(widget).then(|| {
                    g.natural_size
                        .map_or(g.size[1], |natural| natural[1].max(g.size[1]))
                })
            });
            g.size = effective_explicit_size(widget, *size, content, state.screen);
        }
    }
    state = default_positions(state, config);
    for g in &mut state.widgets {
        if let Some(position) = config.widget_positions.get(&g.key) {
            g.position = decode_position(
                Widget::parse(&g.key).unwrap(),
                *position,
                g.size,
                state.screen,
            );
            if config.widget_sizes.contains_key(&g.key) {
                g.position = layout::clamp(g.position, g.size, state.screen);
            }
            if g.key == "glance" && !config.widget_sizes.contains_key("glance") {
                g.size[1] = g
                    .natural_height
                    .unwrap_or(g.size[1])
                    .min((state.screen[1] - g.position[1] - 48.).max(1.));
            }
        }
    }
    state
}

pub fn publish(state: &Snapshot, generation: u64) {
    let Some(path) = path() else {
        return;
    };
    let Ok(mut published) = PUBLISHED.lock() else {
        return;
    };
    publish_ordered(&path, state, generation, &mut published);
}
fn publish_ordered(path: &std::path::Path, state: &Snapshot, generation: u64, published: &mut u64) {
    if generation < *published {
        return;
    }
    let Ok(bytes) = serde_json::to_vec(state) else {
        return;
    };
    if !state.valid() {
        return;
    }
    *published = generation;
    if std::fs::read(path).ok().as_deref() == Some(bytes.as_slice()) {
        return;
    }
    let Some(parent) = path.parent() else {
        return;
    };
    if std::fs::create_dir_all(parent).is_err() {
        return;
    }
    let temporary = path.with_extension(format!("{}-{generation}.tmp", std::process::id()));
    if std::fs::write(&temporary, bytes).is_ok() {
        let _ = std::fs::rename(&temporary, path);
    }
    let _ = std::fs::remove_file(temporary);
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn explicit_glance_inherits_a_safe_viewport_from_an_automatic_bottom_anchor() {
        let mut config = Config::default();
        config.glance.enabled = true;
        config.widget_positions.insert("glance".into(), [0.5, 0.99]);
        config.widget_sizes.insert("glance".into(), [480., 320.]);
        let state = read(&config);
        let g = state.widgets.iter().find(|g| g.key == "glance").unwrap();
        assert!(g.position[1] + g.size[1] <= state.screen[1] - 48.);
    }

    #[test]
    fn snapshot_rejects_missing_duplicate_and_nonfinite_geometry() {
        let mut snapshot = fallback(&Config::default());
        assert!(snapshot.valid());
        snapshot.screen[0] = f32::NAN;
        assert!(!snapshot.valid());
        snapshot.screen[0] = 1920.;
        snapshot.widgets[0].key = snapshot.widgets[1].key.clone();
        assert!(!snapshot.valid());
    }
    #[test]
    fn default_preview_uses_the_actual_card_sizes_and_visible_order() {
        let mut config = Config::default();
        config.enabled_widgets = vec!["weather".into(), "projects".into()];
        let state = default_positions(fallback(&Config::default()), &config);
        let weather = state.widgets.iter().find(|g| g.key == "weather").unwrap();
        let projects = state.widgets.iter().find(|g| g.key == "projects").unwrap();
        assert_eq!(
            projects.position[1] - weather.position[1],
            weather.size[1] + drawer::CARD_GAP
        );
        assert_eq!(weather.position[0], projects.position[0]);
    }
    #[test]
    fn glance_anchor_is_independent_of_agenda_height_and_preview_restores_growth() {
        let screen = [1920., 1080.];
        let anchor = [430., 400.];
        let encoded = encode_position(Widget::Glance, anchor, [312., 220.], screen);
        assert_eq!(
            decode_position(Widget::Glance, encoded, [312., 632.], screen),
            anchor
        );
        let mut state = fallback(&Config::default());
        let glance = state
            .widgets
            .iter_mut()
            .find(|g| g.key == "glance")
            .unwrap();
        glance.natural_height = Some(1400.);
        glance.size[1] = 632.;
        let state = default_positions(state, &Config::default());
        let glance = state.widgets.iter().find(|g| g.key == "glance").unwrap();
        assert_eq!(glance.position, [82., 88.]);
        assert_eq!(glance.size[1], 1080. - 88. - 48.);
        assert_eq!(placement_size(Widget::Glance, glance.size), [280., 180.]);
    }
    #[test]
    fn older_snapshot_cannot_overwrite_newer_geometry() {
        let path = std::env::temp_dir().join(format!(
            "desktop-cards-geometry-{}.json",
            std::process::id()
        ));
        let older = fallback(&Config::default());
        let mut newer = older.clone();
        newer.screen = [2560., 1440.];
        let mut generation = 0;
        publish_ordered(&path, &newer, 2, &mut generation);
        publish_ordered(&path, &older, 1, &mut generation);
        let stored: Snapshot = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
        let _ = std::fs::remove_file(path);
        assert_eq!(stored.screen, newer.screen);
    }
    #[test]
    fn auditfix_ob03_uncached_system_monitor_and_published_lists_expand() {
        let mut config = Config::default();
        config.widget_sizes.insert("devbox".into(), [360., 180.]);
        let snapshot = project(fallback(&config), &config);
        let system = snapshot.widgets.iter().find(|g| g.key == "devbox").unwrap();
        assert_eq!(devbox_content_height(&config.devbox, 0), 226.);
        assert_eq!(system.size[1], 226.);
        assert_eq!(config.widget_sizes["devbox"], [360., 180.]);
        for w in [Widget::Devbox, Widget::Projects, Widget::Calendar] {
            let mut snapshot = fallback(&config);
            let g = snapshot
                .widgets
                .iter_mut()
                .find(|g| g.key == w.key())
                .unwrap();
            g.natural_height = Some(800.);
            config.widget_sizes.insert(w.key().into(), [360., 180.]);
            snapshot.screen = [1920., 600.];
            let projected = project(snapshot, &config);
            assert_eq!(
                projected
                    .widgets
                    .iter()
                    .find(|g| g.key == w.key())
                    .unwrap()
                    .size[1],
                504.
            );
        }
    }
}

#[cfg(test)]
#[test]
fn tool_recipe_projection_refreshes_cached_geometry_and_keeps_explicit_sizes() {
    let original = Config::default();
    let cached = fallback(&original);
    let mut candidate = original.clone();
    candidate.recipes.music = Some(crate::recipes::Recipe::template(
        crate::recipes::Kind::Music,
        0,
    ));
    candidate.recipes.focus = Some(crate::recipes::Recipe::template(
        crate::recipes::Kind::Focus,
        0,
    ));
    let projected = project(cached.clone(), &candidate);
    for widget in [Widget::Music, Widget::Focus] {
        let geometry = projected
            .widgets
            .iter()
            .find(|g| g.key == widget.key())
            .unwrap();
        assert_eq!(geometry.size, crate::tools::cards::size(widget, &candidate));
    }
    candidate.widget_sizes.insert("music".into(), [480., 230.]);
    let explicit = project(cached, &candidate);
    assert_eq!(
        explicit
            .widgets
            .iter()
            .find(|g| g.key == "music")
            .unwrap()
            .size,
        [480., 230.]
    );
    let reset = project(explicit, &original);
    for widget in [Widget::Music, Widget::Focus] {
        assert_eq!(
            reset
                .widgets
                .iter()
                .find(|g| g.key == widget.key())
                .unwrap()
                .size,
            crate::tools::cards::size(widget, &original)
        );
    }
}
