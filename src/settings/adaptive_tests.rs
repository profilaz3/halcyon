// Included in settings::tests to use the isolated native app harness.
#[test]
fn adaptive_native_preview_cycle_and_navigation_are_read_only_until_enabled() {
    let name = "adaptive_native_preview_cycle_and_navigation_are_read_only_until_enabled";
    if desktop_test_child(
        name,
        DesktopEnv {
            app_config: true,
            fixture: true,
            ..Default::default()
        },
    ) {
        return;
    }
    use crate::desktop::profiles::adaptive::{Look, Phase};
    let root = isolated_root();
    let mut app = open_isolated_editor(&root);
    let path = app
        .desktop
        .store()
        .unwrap()
        .app_config()
        .unwrap()
        .to_path_buf();
    let before = std::fs::read(&path).unwrap();
    let city = Location {
        name: "Zagreb".into(),
        latitude: 45.815,
        longitude: 15.982,
        timezone: Some("Europe/Zagreb".into()),
    };
    send_settings(&mut app, Message::Page(Page::AdaptiveLooks));
    send_settings(
        &mut app,
        Message::AdaptiveSource(crate::automation::solar::Source::Manual),
    );
    send_settings(
        &mut app,
        Message::Automation(automation::Message::ManualCity(city)),
    );
    for size in [Size::new(820., 600.), Size::new(1280., 800.)] {
        send_settings(&mut app, Message::Resized(size));
        let mut native = crate::native_test::Native::new(size);
        for look in Look::ALL {
            for phase in Phase::ALL {
                native_send(&mut app, &mut native, Message::AdaptivePreview(look, phase));
                // Complete the real bounded backdrop work before inspecting pixels.
                for job in app.profiles.take_jobs() {
                    app.profiles.update(job.run());
                }
                for job in app.profiles.take_theme_jobs() {
                    app.profiles.update(job.run());
                }
                native.capture(
                    app.settings_view(),
                    &cosmic::Theme::dark(),
                    &format!("{}-{}", look.id(phase), size.width),
                );
                assert_eq!(std::fs::read(&path).unwrap(), before);
                assert_eq!(
                    app.profiles.profile().unwrap().name,
                    format!("{} · {}", look.title(), phase.title())
                );
            }
        }
        native.capture(
            app.settings_view(),
            &cosmic::Theme::light(),
            &format!("adaptive-light-{}", size.width),
        );
        let bounds = native.bounds(app.settings_view(), "adaptive-enable");
        assert!(
            bounds.width > 0.
                && bounds.x >= 0.
                && bounds.x + bounds.width <= size.width
                && bounds.y + bounds.height <= size.height,
            "{bounds:?}"
        );
    }
    send_settings(
        &mut app,
        Message::Automation(automation::Message::AdaptiveRoutine(
            Look::Moss,
            crate::automation::solar::Source::Manual,
        )),
    );
    assert_eq!(app.automation.settings().unwrap().rules.len(), 3);
    assert_eq!(std::fs::read(&path).unwrap(), before);
    send_settings(&mut app, Message::Page(Page::DesktopAutomation));
    assert_eq!(app.profiles.selection(), &profiles::Selection::Current);
    assert!(!app.profiles.can_apply());
}
