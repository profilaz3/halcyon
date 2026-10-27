use super::*;
use crate::{config::Config, drawer::Widget};
use std::collections::BTreeSet;

/// Exercise populated optional records and every supported dynamic map key.
/// Null defaults alone cannot expose a missing monitor/layout child owner.
fn populated() -> Config {
    use crate::widget_style::resolver::{ClockFace, ClockSurface, ClockWeight};
    let mut config = Config::default();
    config.location = Some(crate::config::Location {
        name: "Fixture".into(),
        latitude: 45.,
        longitude: 16.,
        timezone: None,
    });
    config.project_roots = vec!["/fixture/projects".into()];
    config.widget_font = Some("Noto Sans".into());
    config.clock_font = Some("Noto Serif".into());
    config.glance.font = Some("Noto Sans".into());
    config.glance.clock_font = Some("Noto Serif".into());
    config.music.player = Some("org.mpris.MediaPlayer2.fixture".into());
    config.widget_display = Some("DP-1".into());
    config.appearance.family = Some(crate::desktop_theme::Family::Soft);
    config.appearance.surface = Some(crate::desktop_theme::Surface::Panel);
    config.appearance.color_policy = Some(crate::config::ColorPolicy::Wallpaper);
    config.appearance.accent = Some([90, 150, 170]);
    config.clock.face = Some(ClockFace::Analog);
    config.clock.weight = Some(ClockWeight::Bold);
    config.clock.italic = Some(true);
    config.clock.surface = Some(ClockSurface::Panel);
    config.moments.active = Some(crate::moments::Phase::Day);
    for look in [&mut config.moments.day, &mut config.moments.night] {
        look.clock = crate::moments::Clock {
            face: config.clock.face,
            weight: config.clock.weight,
            italic: config.clock.italic,
            surface: config.clock.surface,
        };
    }
    let finish = Some(crate::materials::Finish::new(
        crate::materials::Kind::ALL[0],
    ));
    config.materials = crate::materials::Materials {
        shared: finish,
        widgets: finish,
        dock: finish,
        panel: finish,
    };
    config.recipes.clock = Some(crate::recipes::Recipe::starter(crate::recipes::Kind::Clock));
    config.recipes.weather = Some(crate::recipes::Recipe::starter(
        crate::recipes::Kind::Weather,
    ));
    for widget in Widget::ALL {
        let key = widget.key().to_string();
        config.widget_positions.insert(key.clone(), [0.2, 0.3]);
        config.widget_sizes.insert(key.clone(), [400., 400.]);
        config.widget_monitors.insert(
            key.clone(),
            crate::config::monitors::WidgetMonitor {
                display: Some("DP-1".into()),
                hide_disconnected: true,
            },
        );
        config.monitor_layouts.insert(
            key,
            BTreeMap::from([(
                "DP-1".into(),
                crate::config::monitors::MonitorLayout {
                    position: Some([0.2, 0.3]),
                    size: Some([400., 400.]),
                },
            )]),
        );
    }
    config
}

#[test]
fn architecture_every_serialized_preference_has_a_transaction_owner() {
    fn visit(pointer: &str, value: &Value, owners: &mut BTreeSet<String>) {
        if let Some(owner) = field(pointer) {
            assert!(owner.accepts(value), "invalid fixture {pointer}: {value}");
            owners.insert(pointer.into());
            return;
        }
        let object = value
            .as_object()
            .unwrap_or_else(|| panic!("unowned preference {pointer}"));
        if object.is_empty() {
            assert!(
                matches!(
                    pointer,
                    "/widget_positions" | "/widget_sizes" | "/widget_monitors" | "/monitor_layouts"
                ),
                "unowned empty record {pointer}"
            );
        }
        for (key, value) in object {
            visit(
                &format!("{pointer}/{}", key.replace('~', "~0").replace('/', "~1")),
                value,
                owners,
            );
        }
    }
    let fields: Vec<_> = FIELDS
        .iter()
        .chain(LOCAL_FIELDS)
        .map(|f| f.pointer)
        .collect();
    assert_eq!(
        fields.len(),
        fields.iter().collect::<BTreeSet<_>>().len(),
        "duplicate field ownership"
    );
    let mut owners = BTreeSet::new();
    for config in [Config::default(), populated()] {
        let raw = serde_json::to_value(config).unwrap();
        let _: Config = serde_json::from_value(raw.clone()).unwrap();
        visit("", &raw, &mut owners);
        // The actual labelled write path must accept the same projection.
        crate::desktop::actions::preference_patch(&serde_json::json!({}), &raw).unwrap();
    }
    for widget in Widget::ALL {
        for field in ["widget_positions", "widget_sizes", "monitor_layouts"] {
            assert!(owners.contains(&format!("/{field}/{}", widget.key())));
        }
    }
}
