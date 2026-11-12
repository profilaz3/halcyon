// Included in settings::tests to use the isolated native app harness.

/// The Devices widget is off until chosen, has its own page and search
/// words, and saves its switch like the other tools. With
/// `DESKTOP_CARDS_LAYOUT_EVIDENCE` set, the page and the card are captured
/// in both shades, the card with three devices and with none.
#[test]
fn devices_widget_is_off_until_chosen_and_renders_full_and_empty() {
    let name = "devices_widget_is_off_until_chosen_and_renders_full_and_empty";
    if desktop_test_child(
        name,
        DesktopEnv {
            app_config: true,
            fixture: true,
            icons: Icons::Shown,
            ..Default::default()
        },
    ) {
        return;
    }
    use crate::drawer::Widget;
    let _root = isolated_root();
    // Wiring: off by default, a tool with its own page, profile members and
    // a place in the default layout.
    assert!(!Config::default().widget_enabled(Widget::Devices));
    assert!(Widget::TOOLS.contains(&Widget::Devices));
    assert_eq!(navigation::widget_page(Widget::Devices), Page::Devices);
    assert_eq!(navigation::widget_of(Page::Devices), Some(Widget::Devices));
    assert_eq!(
        crate::desktop::store::widget::tool_of("/devices/enabled"),
        Some("devices")
    );
    assert_eq!(
        crate::desktop::store::widget::tool_of("/widget_sizes/devices"),
        Some("devices")
    );
    let size = crate::tools::cards::size(Widget::Devices, &Config::default());
    assert!(size[0] >= 200. && size[1] >= 120., "{size:?}");

    let (mut app, task) = Settings::init(Core::default(), Page::Devices);
    settle_settings(&mut app, task);
    assert!(!app.config.widget_selected(Widget::Devices));
    for size in [Size::new(820., 600.), Size::new(1280., 800.)] {
        send_settings(&mut app, Message::Resized(size));
        let mut native = crate::native_test::Native::new(size);
        assert!(native.duplicate_ids(app.settings_view()).is_empty());
        let scene = native.bounds(app.settings_view(), "desktop-scene");
        assert!(
            scene.width > 0. && scene.x >= 0. && scene.x + scene.width <= size.width,
            "{scene:?}"
        );
        for device in ["WH-1000XM5", "MX Anywhere 3", "Pixel 8"] {
            let text = native.text_bounds(app.settings_view(), device);
            assert!(text.width > 0., "{device} in the preview");
        }
        for (theme, shade) in [
            (cosmic::Theme::light(), "light"),
            (cosmic::Theme::dark(), "dark"),
        ] {
            native.capture(
                app.settings_view(),
                &theme,
                &format!("devices-page-{shade}-{}", size.width),
            );
        }
    }
    // The switch saves like every other tool, and turns back off.
    send_settings(&mut app, Message::Widget(Widget::Devices, true));
    assert!(Config::load().widget_enabled(Widget::Devices));
    send_settings(&mut app, Message::Widget(Widget::Devices, false));
    assert!(!Config::load().widget_enabled(Widget::Devices));

    // The card itself, as the desktop draws it, in a light and a dark style.
    use crate::desktop_theme::Family;
    let styled = |light: bool| {
        let mut config = Config::default();
        let family = Family::ALL
            .into_iter()
            .find(|f| f.light() == light)
            .unwrap();
        let a = &mut config.appearance;
        a.family = Some(family);
        (a.opacity, a.corner_radius, a.border_opacity) = family.geometry();
        config
    };
    let empty: Vec<crate::devices::Device> = Vec::new();
    let full = crate::tools::cards::sample_devices();
    for (dims, label) in [(size, "default"), ([420., 260.], "large")] {
        let mut native = crate::native_test::Native::new(Size::new(dims[0], dims[1]));
        for (theme, shade) in [
            (cosmic::Theme::light(), "light"),
            (cosmic::Theme::dark(), "dark"),
        ] {
            let config = styled(shade == "light");
            let card = |list: &[crate::devices::Device]| {
                crate::tools::cards::devices::<Message>(list, &config, Some(dims))
            };
            let none = native.text_bounds(card(&empty), "No devices connected");
            assert!(none.width > 0. && none.x + none.width <= dims[0]);
            for device in &full {
                let text = native.text_bounds(card(&full), &device.name);
                assert!(
                    text.width > 0. && text.y + text.height <= dims[1],
                    "{label}"
                );
            }
            native.capture(
                card(&full),
                &theme,
                &format!("devices-card-{label}-{shade}"),
            );
            native.capture(
                card(&empty),
                &theme,
                &format!("devices-card-{label}-empty-{shade}"),
            );
        }
    }
}
