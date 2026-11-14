//! Which headline features are switched on, read from saved settings for the
//! existing daily report. Only fixed on/off values and fixed mode names: no
//! locations, names, paths, display connectors, rules or widget content, and
//! no counting of clicks or time spent. `None` means the setting was unreadable.
use crate::config::Config;
use crate::desktop::panels::{PanelDraft, PanelKind, Setting};
use crate::drawer::Widget;
use serde::Serialize;
use std::time::Duration;

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize)]
pub(crate) struct Features {
    pub wallpaper_effect: Option<&'static str>,
    pub activity_island: Option<bool>,
    pub penumbra: Option<bool>,
    pub islands: Option<bool>,
    pub widgets: Option<Vec<&'static str>>,
    pub visualizer: Option<bool>,
    pub wallpaper_depth: Option<bool>,
    pub theme_style: Option<&'static str>,
    pub automation: Option<bool>,
}

fn effect(config: &Config) -> &'static str {
    use crate::rain::Effect;
    if !config.rain.enabled {
        return "off";
    }
    match config.rain.effect {
        Effect::Rain => "rain",
        Effect::Snow => "snow",
        Effect::Fog => "fog",
        Effect::Clouds => "clouds",
    }
}

fn theme_style(config: &Config) -> &'static str {
    use crate::desktop_theme::Family;
    match config.appearance.family {
        None => "default",
        Some(Family::Soft) => "soft",
        Some(Family::Outline) => "outline",
        Some(Family::Paper) => "paper",
        Some(Family::Bare) => "bare",
        Some(Family::Glass) => "glass",
        Some(Family::Frosted) => "frosted",
        Some(Family::Midnight) => "midnight",
        Some(Family::Terminal) => "terminal",
        Some(Family::Neon) => "neon",
        Some(Family::Editorial) => "editorial",
        Some(Family::Retro) => "retro",
        Some(Family::Pastel) => "pastel",
        Some(Family::Bauhaus) => "bauhaus",
    }
}

/// True when any listed bar has it on, false when every listed bar is known
/// to have it off, and unknown when a listed bar's value cannot be read.
fn any_bar(bars: &[PanelDraft], on: impl Fn(&PanelDraft) -> Option<bool>) -> Option<bool> {
    let mut unknown = false;
    for bar in bars.iter().filter(|bar| bar.listed()) {
        match on(bar) {
            Some(true) => return Some(true),
            Some(false) => {}
            None => unknown = true,
        }
    }
    (!unknown).then_some(false)
}

fn known<T>(setting: Setting<T>) -> Option<T> {
    match setting {
        Setting::Known(value) => Some(value),
        Setting::Unsupported(_) => None,
    }
}

/// Pure snapshot of already loaded settings, so it can be tested with fixtures.
pub(crate) fn snapshot(
    config: Option<&Config>,
    bars: Option<&[PanelDraft]>,
    automation: Option<bool>,
) -> Features {
    let bar = |on: fn(&PanelDraft) -> Option<bool>| bars.and_then(|bars| any_bar(bars, on));
    Features {
        wallpaper_effect: config.map(effect),
        activity_island: bar(|b| {
            known(b.applets()).map(|r| r.contains(crate::activity_island::APPLET_ID))
        }),
        penumbra: bar(|b| known(b.penumbra()).map(|g| g.is_some_and(|g| g.enabled))),
        islands: bar(|b| known(b.islands()).map(|s| s.enabled)),
        widgets: config.map(|c| {
            Widget::ALL
                .into_iter()
                .filter(|w| c.widget_visible(*w))
                .map(Widget::key)
                .collect()
        }),
        visualizer: config.map(|c| c.music.enabled && c.music.visualizer.active()),
        wallpaper_depth: config.map(|c| c.clock.depth),
        theme_style: config.map(theme_style),
        automation,
    }
}

fn read() -> Features {
    let config = Config::path()
        .and_then(|path| crate::config::read_config_text(&path).ok().flatten())
        .and_then(|text| Config::from_bytes(text.as_bytes()).ok());
    let store = crate::desktop::store::DesktopStore::live().ok();
    let bars = store.as_ref().and_then(|store| {
        [PanelKind::Panel, PanelKind::Dock]
            .into_iter()
            .map(|kind| PanelDraft::load(store, kind).ok())
            .collect::<Option<Vec<_>>>()
    });
    let automation = store
        .as_ref()
        .and_then(|store| crate::automation::load(store).ok())
        .map(|a| a.running());
    snapshot(config.as_ref(), bars.as_deref(), automation)
}

/// Local file reads only, bounded in time; any failure reports unknown values.
pub(crate) async fn observe() -> Features {
    if cfg!(test) || crate::installation::isolated() {
        return Features::default();
    }
    tokio::time::timeout(Duration::from_secs(5), tokio::task::spawn_blocking(read))
        .await
        .ok()
        .and_then(Result::ok)
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::desktop::store::DesktopStore;

    fn bars(files: &[(&str, &str, &str)]) -> Vec<PanelDraft> {
        let folder = crate::test_env::TempPath::new("features");
        let root = folder.root();
        std::fs::create_dir_all(root.join("system/cosmic")).unwrap();
        for (namespace, key, value) in files {
            let dir = root.join("config/cosmic").join(namespace).join("v1");
            std::fs::create_dir_all(&dir).unwrap();
            std::fs::write(dir.join(key), value).unwrap();
        }
        let store = DesktopStore::fixture(root).unwrap();
        let out = [PanelKind::Panel, PanelKind::Dock]
            .into_iter()
            .map(|kind| PanelDraft::load(&store, kind).unwrap())
            .collect();
        out
    }

    #[test]
    fn snapshot_reports_only_fixed_feature_values() {
        let mut config = Config {
            widgets_visible: true,
            enabled_widgets: vec!["weather".into(), "clock".into()],
            location: None,
            ..Default::default()
        };
        config.rain.enabled = true;
        config.rain.effect = crate::rain::Effect::Snow;
        config.clock.depth = true;
        config.appearance.family = Some(crate::desktop_theme::Family::Glass);
        let panel = "com.system76.CosmicPanel.Panel";
        let bars = bars(&[
            ("com.system76.CosmicPanel", "entries", r#"["Panel"]"#),
            (
                panel,
                "plugins_center",
                r#"Some(["io.github.profilaz3.Halcyon.Activity"])"#,
            ),
            (panel, "plugins_wings", "None"),
            (
                panel,
                "top_edge_gradient",
                "Some((enabled: true, height: 96, opacity: 0.4, maximized_color: (36, 38, 42), maximized_opacity: 1.0))",
            ),
        ]);
        let features = snapshot(Some(&config), Some(&bars), Some(false));
        assert_eq!(features.wallpaper_effect, Some("snow"));
        assert_eq!(features.activity_island, Some(true));
        assert_eq!(features.penumbra, Some(true));
        assert_eq!(features.islands, Some(false));
        assert_eq!(features.wallpaper_depth, Some(true));
        assert_eq!(features.theme_style, Some("glass"));
        assert_eq!(features.automation, Some(false));
        let widgets = features.widgets.clone().unwrap();
        assert!(
            widgets
                .iter()
                .all(|w| Widget::ALL.iter().any(|x| x.key() == *w))
        );
        let body = serde_json::to_string(&features).unwrap();
        assert!(body.len() < 400, "{body}");
        let keys: Vec<_> = serde_json::to_value(&features)
            .unwrap()
            .as_object()
            .unwrap()
            .keys()
            .cloned()
            .collect();
        assert_eq!(keys.len(), 9);

        // A bar that is not listed does not count, and unreadable settings are unknown.
        let unlisted = super::tests::bars(&[
            ("com.system76.CosmicPanel", "entries", r#"["Dock"]"#),
            (
                panel,
                "plugins_center",
                r#"Some(["io.github.profilaz3.Halcyon.Activity"])"#,
            ),
        ]);
        let features = snapshot(None, Some(&unlisted), None);
        assert_eq!(features.activity_island, Some(false));
        assert_eq!(features.wallpaper_effect, None);
        assert_eq!(features.widgets, None);
        assert_eq!(snapshot(None, None, None), Features::default());
    }
}
