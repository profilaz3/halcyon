//! First-run setup through the production Settings view in the headless
//! renderer, on a private fixture desktop: every screen at the smallest and
//! a common window size, a look and dock choices applied live, Cancel
//! putting the desktop back and Finish keeping it.
use super::*;
use crate::native_test::Native;
use cosmic::iced::{Event, mouse};

const DOCK: &str = "com.system76.CosmicPanel.Dock";
const PANEL: &str = "com.system76.CosmicPanel.Panel";

fn child(name: &str) -> bool {
    if std::env::var("DESKTOP_CARDS_WIZARD_TEST").as_deref() == Ok(name) {
        return false;
    }
    let root = std::env::temp_dir().join(format!("cards-wizard-{}-{name}", std::process::id()));
    for dir in ["config", "data", "cache", "state", "desktop"] {
        std::fs::create_dir_all(root.join(dir)).unwrap();
    }
    std::fs::write(root.join("dconf-profile"), "").unwrap();
    std::fs::write(root.join("config.json"), "{}").unwrap();
    let result = std::process::Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            &format!("settings::wizard_tests::{name}"),
            "--include-ignored",
            "--nocapture",
        ])
        .env("DESKTOP_CARDS_WIZARD_TEST", name)
        .env("DESKTOP_CARDS_CONFIG", root.join("config.json"))
        .env("DESKTOP_CARDS_DESKTOP_ROOT", root.join("desktop"))
        .env("XDG_CONFIG_HOME", root.join("config"))
        .env("XDG_DATA_HOME", root.join("data"))
        .env("XDG_CACHE_HOME", root.join("cache"))
        .env("XDG_STATE_HOME", root.join("state"))
        .env("DCONF_PROFILE", root.join("dconf-profile"))
        .output()
        .unwrap();
    std::fs::remove_dir_all(root).unwrap();
    if matches!(name, "responsiveness_probe" | "rendering_probe") {
        eprint!("{}", String::from_utf8_lossy(&result.stdout));
    }
    assert!(
        result.status.success(),
        "{}\n{}",
        String::from_utf8_lossy(&result.stdout),
        String::from_utf8_lossy(&result.stderr)
    );
    true
}

fn desktop_root() -> std::path::PathBuf {
    std::path::PathBuf::from(std::env::var_os("DESKTOP_CARDS_DESKTOP_ROOT").unwrap())
}

/// A stock dock and panel, and the bundled wallpapers where the installer
/// puts them.
fn seed_desktop() {
    let system = |namespace: &str, key: &str, value: &str| {
        let path = desktop_root()
            .join("system/cosmic")
            .join(namespace)
            .join("v1")
            .join(key);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, value).unwrap();
    };
    system(
        "com.system76.CosmicPanel",
        "entries",
        "[\"Panel\",\"Dock\"]",
    );
    system(DOCK, "anchor", "Bottom");
    system(
        DOCK,
        "plugins_center",
        "Some([\"com.system76.CosmicAppList\"])",
    );
    system(PANEL, "anchor", "Top");
    system(
        PANEL,
        "plugins_wings",
        "Some(([\"com.system76.CosmicPanelWorkspacesButton\"], [\"com.system76.CosmicAppletTime\"]))",
    );
    let store = crate::desktop::DesktopStore::from_env().unwrap();
    let bundled = crate::desktop::wallpaper::bundled_dir(&store);
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
}

/// The user's own value of a COSMIC key, if they have one.
fn user(namespace: &str, key: &str) -> Option<String> {
    std::fs::read_to_string(
        desktop_root()
            .join("config/cosmic")
            .join(namespace)
            .join("v1")
            .join(key),
    )
    .ok()
}

fn click(app: &mut Settings, native: &mut Native, id: &str) {
    let bounds = native.bounds(app.settings_view(), id);
    assert!(
        bounds.x >= 0.
            && bounds.x + bounds.width <= native.size.width
            && bounds.y >= 0.
            && bounds.y + bounds.height <= native.size.height,
        "{id} is off screen: {bounds:?}"
    );
    let point = bounds.center();
    for input in [
        mouse::Event::CursorMoved { position: point },
        mouse::Event::ButtonPressed(mouse::Button::Left),
        mouse::Event::ButtonReleased(mouse::Button::Left),
    ] {
        for message in native
            .event(app.settings_view(), Event::Mouse(input), point)
            .0
        {
            super::tests::send_settings(app, message);
        }
    }
}

fn open(size: Size) -> (Settings, Native) {
    OPEN_WIZARD.store(true, std::sync::atomic::Ordering::Relaxed);
    let (mut app, initial) = Settings::init(Core::default(), Page::Overview);
    if app.native_worker_needed() {
        super::tests::settle_settings(&mut app, initial);
    }
    let _ = app.update(Message::Resized(size));
    assert!(app.wizard.is_some());
    (app, Native::new(size))
}

fn step(app: &Settings) -> wizard::Step {
    app.wizard.as_ref().unwrap().step
}

#[test]
fn onboarding_full_display_explains_and_recovers_without_discarding_choices() {
    if child("onboarding_full_display_explains_and_recovers_without_discarding_choices") {
        return;
    }
    seed_desktop();
    use crate::displays::{
        OutputEvent,
        test_support::{Wire, info},
    };
    let wire = Wire::new();
    for size in [Size::new(820., 600.), Size::new(1280., 800.)] {
        Config::save_changes(json!({
            "enabled_widgets":["clock"], "glance":{"enabled":false},
            "notes":{"enabled":false}, "widget_sizes":{"clock":[730.,430.]}
        }))
        .unwrap();
        let baseline = wizard::widgets::raw().unwrap();
        let (mut app, mut native) = open(size);
        let _ = app.update(Message::DisplayOutput(
            OutputEvent::Created(Some(info(Some("A"), (800, 600)))),
            wire.output(1),
        ));
        let _ = app.update(Message::Wizard(wizard::Message::Show(
            wizard::Step::Widgets,
        )));
        click(&mut app, &mut native, "wizard-widget-notes");
        click(&mut app, &mut native, "wizard-next");
        assert_eq!(step(&app), wizard::Step::Arrange);
        assert!(
            app.wizard
                .as_ref()
                .unwrap()
                .widget_plan(&app.config)
                .error
                .is_some()
        );
        let warning = native.bounds(app.settings_view(), "wizard-step-error");
        let footer = native.bounds(app.settings_view(), "wizard-next");
        assert!(
            warning.y >= 0. && warning.y + warning.height <= footer.y,
            "The error is visible above the footer: {warning:?}, {footer:?}"
        );
        native.scroll(app.settings_view(), "wizard-body", 1000.);
        assert_eq!(
            warning,
            native.bounds(app.settings_view(), "wizard-step-error")
        );
        native.capture(
            app.settings_view(),
            &controls_theme(&app.config),
            &format!("wizard-full-display-{}", size.width),
        );
        // A direct request also cannot write the invalid layout.
        let _ = app.update(Message::Wizard(wizard::Message::Next));
        assert_eq!(step(&app), wizard::Step::Arrange);
        assert_eq!(wizard::widgets::raw().unwrap(), baseline);
        // The primary pointer action now offers correction, not a disabled end.
        click(&mut app, &mut native, "wizard-next");
        assert_eq!(step(&app), wizard::Step::Widgets);
        let draft = &app.wizard.as_ref().unwrap().widgets;
        assert!(draft.selected(Widget::Clock) && draft.selected(Widget::Notes));
        assert_eq!(wizard::widgets::raw().unwrap(), baseline);
        native.scroll(app.settings_view(), "wizard-body", 0.);
        click(&mut app, &mut native, "wizard-widget-clock");
        click(&mut app, &mut native, "wizard-next");
        assert!(
            app.wizard
                .as_ref()
                .unwrap()
                .widget_plan(&app.config)
                .error
                .is_none()
        );
        click(&mut app, &mut native, "wizard-next");
        assert_eq!(step(&app), wizard::Step::Ready);
        assert!(Config::load().notes.enabled);
        assert!(!Config::load().widget_visible(Widget::Clock));
        click(&mut app, &mut native, "wizard-cancel");
        assert!(app.wizard.is_none());
        assert_eq!(Config::load().widget_sizes["clock"], [730., 430.]);
        assert!(!Config::load().notes.enabled);
    }
}

/// Explicit diagnostic, not a wall-time CI assertion. Uses an isolated desktop
/// and the real widget tree; software-rendered timings are not GPU frame rates.
#[test]
#[ignore]
fn responsiveness_probe() {
    if child("responsiveness_probe") {
        return;
    }
    seed_desktop();
    let measure = |label: &str, f: &mut dyn FnMut()| {
        let start = std::time::Instant::now();
        f();
        println!(
            "PERF {label}: {:.3} ms",
            start.elapsed().as_secs_f64() * 1000.
        );
    };
    let start = std::time::Instant::now();
    let (mut app, mut native) = open(Size::new(1280., 800.));
    println!(
        "PERF init_and_renderer: {:.3} ms",
        start.elapsed().as_secs_f64() * 1000.
    );
    let theme = controls_theme(&app.config);
    measure("welcome_cold_view", &mut || {
        drop(app.settings_view());
    });
    measure("welcome_render", &mut || {
        native.render(app.settings_view(), &theme);
    });
    measure("start_click", &mut || {
        click(&mut app, &mut native, "wizard-start");
    });
    measure("look_cold_view", &mut || {
        drop(app.settings_view());
    });
    measure("look_warm_views_20", &mut || {
        for _ in 0..20 {
            drop(app.settings_view());
        }
    });
    app.wizard.as_mut().unwrap().step = wizard::Step::Bars;
    measure("bars_resize_views_80", &mut || {
        for n in 0..80 {
            app.window = Size::new(820. + n as f32 * 3., 600. + n as f32);
            drop(app.settings_view());
        }
    });
    app.wizard = None;
    app.window = Size::new(1280., 800.);
    for page in [
        Page::Overview,
        Page::Themes,
        Page::General,
        Page::DesktopDock,
        Page::Calendar,
        Page::Appearance,
    ] {
        measure(&format!("page_{page:?}"), &mut || {
            let _ = app.update(Message::Page(page));
        });
        measure(&format!("view_{page:?}_20"), &mut || {
            for _ in 0..20 {
                drop(app.settings_view());
            }
        });
        if matches!(page, Page::Themes | Page::General | Page::DesktopDock) {
            native.render(app.settings_view(), &theme);
            measure(&format!("software_scroll_{page:?}_10"), &mut || {
                for _ in 0..10 {
                    native.event(
                        app.settings_view(),
                        Event::Mouse(mouse::Event::WheelScrolled {
                            delta: mouse::ScrollDelta::Lines { x: 0., y: -1. },
                        }),
                        cosmic::iced::Point::new(700., 400.),
                    );
                    native.render(app.settings_view(), &theme);
                }
            });
        }
    }
}

/// Offline CPU/GPU comparison on the same widget tree. Screenshot readback is
/// included, so these timings are not compositor presentation rates.
#[test]
#[ignore]
fn rendering_probe() {
    if child("rendering_probe") {
        return;
    }
    seed_desktop();
    let (mut app, _) = open(Size::new(1280., 800.));
    app.wizard = None;
    let theme = controls_theme(&app.config);
    for backend in ["tiny-skia", "wgpu"] {
        let mut native = Native::with_backend(Size::new(1280., 800.), backend);
        println!("RENDERER {}", native.renderer_name());
        for page in [Page::Themes, Page::General, Page::DesktopDock] {
            let _ = app.update(Message::Page(page));
            native.render(app.settings_view(), &theme);
            let start = std::time::Instant::now();
            for n in 0..30 {
                native.event(
                    app.settings_view(),
                    Event::Mouse(mouse::Event::WheelScrolled {
                        delta: mouse::ScrollDelta::Lines {
                            x: 0.,
                            y: if n < 15 { -1. } else { 1. },
                        },
                    }),
                    cosmic::iced::Point::new(700., 400.),
                );
                let pixels = native.render(app.settings_view(), &theme);
                assert_eq!(pixels.len(), 1280 * 800 * 4);
                assert!(pixels.chunks_exact(4).any(|pixel| pixel[3] != 0));
            }
            println!(
                "RENDER {backend} {page:?} 30 scroll/draw/readback cycles: {:.3} ms",
                start.elapsed().as_secs_f64() * 1000.
            );
        }
    }
}

#[test]
fn every_screen_fits_and_cancel_puts_the_desktop_back() {
    if child("every_screen_fits_and_cancel_puts_the_desktop_back") {
        return;
    }
    seed_desktop();
    crate::test_env::native_worker(&desktop_root(), true);
    for size in [Size::new(820., 600.), Size::new(1280., 800.)] {
        let (mut app, mut native) = open(size);
        let theme = controls_theme(&app.config);
        let shot = |app: &Settings, native: &mut Native, name: &str| {
            native.capture(
                app.settings_view(),
                &theme,
                &format!("wizard-{name}-{}", size.width),
            )
        };
        shot(&app, &mut native, "1-welcome");
        click(&mut app, &mut native, "wizard-start");
        assert_eq!(step(&app), wizard::Step::Look);
        click(&mut app, &mut native, "wizard-look-calm");
        let wizard = app.wizard.as_ref().unwrap();
        assert_eq!(wizard.error, None);
        assert_eq!(wizard.look, Some("calm"));
        assert_eq!(
            Config::load().appearance.family,
            Some(crate::desktop_theme::Family::Soft)
        );
        shot(&app, &mut native, "2-look");
        // Trying another look replaces the first.
        click(&mut app, &mut native, "wizard-look-evening");
        assert_eq!(app.wizard.as_ref().unwrap().look, Some("evening"));
        click(&mut app, &mut native, "wizard-next");
        assert_eq!(step(&app), wizard::Step::Bars);
        let footer = native.bounds(app.settings_view(), "wizard-next");
        for id in [
            "wizard-panel-bar",
            "wizard-panel-islands",
            "wizard-panel-pill",
            "wizard-panel-rounded",
            "wizard-panel-square",
            "wizard-dock-pill",
            "wizard-dock-rounded",
            "wizard-dock-square",
            "wizard-dock-floating",
            "wizard-dock-attached",
            "wizard-dock-shown",
            "wizard-dock-hides",
        ] {
            let bounds = native.bounds(app.settings_view(), id);
            assert!(
                bounds.x >= 0.
                    && bounds.x + bounds.width <= size.width
                    && bounds.y >= 0.
                    && bounds.y + bounds.height < footer.y,
                "{id} must fit above the fixed footer without scrolling: {bounds:?}"
            );
        }
        click(&mut app, &mut native, "wizard-dock-rounded");
        click(&mut app, &mut native, "wizard-dock-hides");
        shot(&app, &mut native, "3-bars");
        click(&mut app, &mut native, "wizard-next");
        assert_eq!(app.wizard.as_ref().unwrap().error, None);
        assert_eq!(step(&app), wizard::Step::Widgets);
        shot(&app, &mut native, "4-widgets");
        if !app
            .wizard
            .as_ref()
            .unwrap()
            .widgets
            .selected(Widget::Weather)
        {
            click(&mut app, &mut native, "wizard-widget-weather");
        }
        click(&mut app, &mut native, "wizard-next");
        assert_eq!(step(&app), wizard::Step::Arrange);
        shot(&app, &mut native, "5-arrange");
        click(&mut app, &mut native, "wizard-next");
        assert_eq!(step(&app), wizard::Step::Weather);
        assert_eq!(user(DOCK, "autohide").as_deref(), Some("OnOverlap"));
        assert_eq!(user(DOCK, "border_radius").as_deref(), Some("12"));
        // Applets and their order are never part of a look.
        assert_eq!(user(DOCK, "plugins_center"), None);
        assert_eq!(user(PANEL, "plugins_wings"), None);
        shot(&app, &mut native, "4-weather");
        click(&mut app, &mut native, "wizard-next");
        assert_eq!(step(&app), wizard::Step::Ready);
        shot(&app, &mut native, "5-ready");
        let startup = native.bounds(app.settings_view(), "wizard-startup");
        let finish = native.bounds(app.settings_view(), "wizard-finish");
        assert!(
            startup.y + startup.height < finish.y,
            "startup stays above footer: {startup:?}"
        );
        click(&mut app, &mut native, "wizard-cancel");
        assert!(
            app.wizard.is_none(),
            "{:?}",
            app.wizard.as_ref().and_then(|w| w.error.clone())
        );
        let config = Config::load();
        assert!(config.onboarded);
        assert_eq!(config.appearance.family, None, "the look is undone");
        for key in ["autohide", "border_radius"] {
            assert!(user(DOCK, key).is_none(), "{key}: {:?}", user(DOCK, key));
        }
        // The next size starts from a fresh first run.
        Config::save_changes(serde_json::json!({"onboarded": false})).unwrap();
    }
}

#[test]
fn finishing_keeps_the_look_and_skip_leaves_the_desktop_alone() {
    if child("finishing_keeps_the_look_and_skip_leaves_the_desktop_alone") {
        return;
    }
    seed_desktop();
    let size = Size::new(1020., 760.);
    let (mut app, mut native) = open(size);
    click(&mut app, &mut native, "wizard-leave");
    assert!(app.wizard.is_none());
    assert!(Config::load().onboarded);
    assert_eq!(Config::load().appearance.family, None);
    assert_eq!(user(DOCK, "border_radius"), None, "Skip changes nothing");

    let (mut app, mut native) = open(size);
    click(&mut app, &mut native, "wizard-start");
    click(&mut app, &mut native, "wizard-look-focus");
    for _ in 0..4 {
        click(&mut app, &mut native, "wizard-next");
    }
    assert_eq!(step(&app), wizard::Step::Ready);
    click(&mut app, &mut native, "wizard-finish");
    assert!(app.wizard.is_none());
    assert_eq!(app.page, Page::Overview);
    let config = Config::load();
    assert_eq!(
        config.appearance.family,
        Some(crate::desktop_theme::Family::Bare)
    );
    assert!(config.widget_visible(crate::drawer::Widget::Focus));
    assert!(config.widget_visible(crate::drawer::Widget::Clock));
    assert!(!config.widget_visible(crate::drawer::Widget::Devbox));
    assert!(!config.glance.enabled && !config.glance.show_calendar);
    // The old checklist doesn't follow the wizard.
    assert!(!app.needs_setup());
}

#[test]
fn an_untouched_dock_and_panel_step_keeps_the_persons_own_setup() {
    if child("an_untouched_dock_and_panel_step_keeps_the_persons_own_setup") {
        return;
    }
    seed_desktop();
    // Their own dock: hidden, square, attached.
    let own = |key: &str, value: &str| {
        let path = desktop_root()
            .join("config/cosmic")
            .join(DOCK)
            .join("v1")
            .join(key);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, value).unwrap();
    };
    own("autohide", "Always");
    own("border_radius", "0");
    let (mut app, mut native) = open(Size::new(1020., 760.));
    let bars = app.wizard.as_ref().unwrap().bars;
    assert!(bars.hides, "the step shows their dock as it is");
    assert_eq!(bars.dock_shape, wizard::Shape::Square);
    click(&mut app, &mut native, "wizard-start");
    click(&mut app, &mut native, "wizard-skip");
    assert_eq!(step(&app), wizard::Step::Bars);
    click(&mut app, &mut native, "wizard-next");
    assert_eq!(step(&app), wizard::Step::Widgets);
    assert_eq!(user(DOCK, "autohide").as_deref(), Some("Always"));
    assert_eq!(user(DOCK, "border_radius").as_deref(), Some("0"));
    assert_eq!(user(DOCK, "anchor_gap"), None);
    assert_eq!(user(PANEL, "border_radius"), None);
    // Changing one thing writes only that.
    click(&mut app, &mut native, "wizard-back");
    click(&mut app, &mut native, "wizard-dock-pill");
    click(&mut app, &mut native, "wizard-next");
    assert_eq!(user(DOCK, "autohide").as_deref(), Some("Always"));
    assert_eq!(user(DOCK, "border_radius").as_deref(), Some("160"));
    assert_eq!(user(PANEL, "border_radius"), None);
}

#[test]
fn cancel_undoes_setup_after_a_city_and_after_reopening() {
    if child("cancel_undoes_setup_after_a_city_and_after_reopening") {
        return;
    }
    seed_desktop();
    let size = Size::new(1020., 760.);
    let (mut app, mut native) = open(size);
    click(&mut app, &mut native, "wizard-start");
    click(&mut app, &mut native, "wizard-look-calm");
    assert!(Config::load().appearance.family.is_some());
    // Close Settings halfway: setup resumes, with its changes.
    drop(app);
    let (mut app, mut native) = open(size);
    let wizard = app.wizard.as_ref().unwrap();
    assert_eq!(wizard.step, wizard::Step::Look);
    assert_eq!(wizard.look, Some("calm"));
    assert!(wizard.has_changes());
    // A newer, unrelated change: choosing a city.
    click(&mut app, &mut native, "wizard-next");
    click(&mut app, &mut native, "wizard-next");
    assert_eq!(step(&app), wizard::Step::Widgets);
    click(&mut app, &mut native, "wizard-next");
    click(&mut app, &mut native, "wizard-next");
    assert_eq!(step(&app), wizard::Step::Weather);
    app.cities = vec![City {
        name: "Belgrade".into(),
        latitude: 44.8,
        longitude: 20.5,
        admin1: String::new(),
        country: "Serbia".into(),
        timezone: None,
    }];
    let _ = app.update(Message::ChooseCity(0));
    assert!(Config::load().location.is_some());
    click(&mut app, &mut native, "wizard-cancel");
    assert!(
        app.wizard.is_none(),
        "{:?}",
        app.wizard.as_ref().and_then(|wizard| wizard.error.clone())
    );
    let config = Config::load();
    assert_eq!(config.appearance.family, None, "the look is undone");
    assert!(config.location.is_some(), "the city stays");
    // Nothing left to resume.
    let store = crate::desktop::DesktopStore::from_env().unwrap();
    assert!(!wizard::Wizard::pending(&store));
}

#[test]
fn skipping_dock_and_panel_keeps_them_and_ready_says_so() {
    if child("skipping_dock_and_panel_keeps_them_and_ready_says_so") {
        return;
    }
    seed_desktop();
    let (mut app, mut native) = open(Size::new(1020., 760.));
    click(&mut app, &mut native, "wizard-start");
    click(&mut app, &mut native, "wizard-skip");
    let before = app.wizard.as_ref().unwrap().bars;
    click(&mut app, &mut native, "wizard-dock-square");
    assert_ne!(app.wizard.as_ref().unwrap().bars, before);
    click(&mut app, &mut native, "wizard-skip");
    assert_eq!(step(&app), wizard::Step::Widgets);
    assert_eq!(
        app.wizard.as_ref().unwrap().bars,
        before,
        "the summary shows what's applied"
    );
    assert_eq!(user(DOCK, "border_radius"), None, "nothing was written");
}

#[test]
fn busy_sliders_keep_latest_sample_and_preserve_completion_and_undo_order() {
    if child("busy_sliders_keep_latest_sample_and_preserve_completion_and_undo_order") {
        return;
    }
    seed_desktop();
    let (mut app, _) = Settings::init(Core::default(), Page::DesktopDock);
    app.native_busy = true;
    app.last_desktop_change = Some((
        crate::desktop::Owner::DockPanel,
        "Earlier action".into(),
        "shown-id".into(),
    ));
    for value in 0..100 {
        let _ = app.update(Message::DesktopEditor(desktop::Message::Opacity(value)));
    }
    assert_eq!(
        app.native_deferred.len(),
        1,
        "obsolete pointer samples must not form a backlog"
    );
    let _ = app.update(Message::DesktopEditor(desktop::Message::Apply));
    let _ = app.update(Message::DesktopEditor(desktop::Message::Select(
        crate::desktop::panels::PanelKind::Panel,
    )));
    for value in 0..50 {
        let _ = app.update(Message::DesktopEditor(desktop::Message::Opacity(value)));
    }
    let _ = app.update(Message::DesktopEditor(desktop::Message::Apply));
    let _ = app.update(Message::UndoDesktop);
    let queued = &app.native_deferred;
    assert_eq!(queued.len(), 6);
    assert!(matches!(
        &queued[0],
        Message::DesktopEditor(desktop::Message::Opacity(99))
    ));
    assert!(matches!(
        &queued[1],
        Message::DesktopEditor(desktop::Message::Apply)
    ));
    assert!(matches!(
        &queued[2],
        Message::DesktopEditor(desktop::Message::Select(
            crate::desktop::panels::PanelKind::Panel
        ))
    ));
    assert!(matches!(
        &queued[3],
        Message::DesktopEditor(desktop::Message::Opacity(49))
    ));
    assert!(matches!(
        &queued[4],
        Message::DesktopEditor(desktop::Message::Apply)
    ));
    assert!(matches!(&queued[5], Message::UndoDesktopShown(Some(id)) if id == "shown-id"));
    app.native_deferred.pop_back(); // Verify final slider values before Undo.
    app.native_busy = false;
    while let Some(message) = app.native_deferred.pop_front() {
        let _ = app.update(message);
    }
    assert_eq!(user(DOCK, "opacity").as_deref(), Some("0.99"));
    assert_eq!(user(PANEL, "opacity").as_deref(), Some("0.49"));
}

#[test]
fn dock_panel_navigation_does_not_wait_for_islands_and_checks_discover_once() {
    if child("dock_panel_navigation_does_not_wait_for_islands_and_checks_discover_once") {
        return;
    }
    seed_desktop();
    let (mut app, _) = Settings::init(Core::default(), Page::DesktopDock);
    let root = desktop_root();
    crate::desktop::islands::fixture_installed(&root);
    let store = crate::desktop::DesktopStore::from_env().unwrap();
    app.desktop.islands.installation = crate::desktop::islands::Installation::discover(&store);
    let counter = root.join("check-count");
    crate::test_env::native_worker(&root, true);
    std::fs::write(root.join("slow-islands-probe.py"), format!(
        "import pathlib,time\np=pathlib.Path({:?})\np.write_text(str(int(p.read_text())+1 if p.exists() else 1))\ntime.sleep(0.3)\nprint(pathlib.Path({:?}).read_text())\n",
        counter.to_str().unwrap(), root.join("islands-installation.json").to_str().unwrap()
    )).unwrap();
    for kind in [
        crate::desktop::panels::PanelKind::Panel,
        crate::desktop::panels::PanelKind::Dock,
    ] {
        let _ = app.update(Message::DesktopEditor(desktop::Message::Select(kind)));
        assert_eq!(
            app.desktop.selected, kind,
            "tab selection updates immediately"
        );
        assert!(!app.native_busy, "tab selection is not a native write job");
        assert!(app.native_deferred.is_empty());
        assert!(
            !counter.exists(),
            "plain navigation must not start compatibility discovery"
        );
    }
    // Real checking still observes installation afresh, but only once per read.
    app.desktop.check_islands();
    assert_eq!(std::fs::read_to_string(counter).unwrap(), "1");
}

#[test]
fn onboarding_widgets_stage_resume_apply_once_and_cancel_without_losing_content() {
    if child("onboarding_widgets_stage_resume_apply_once_and_cancel_without_losing_content") {
        return;
    }
    seed_desktop();
    Config::save_changes(json!({"enabled_widgets":["clock","weather"],"glance":{"enabled":false},"notes":{"enabled":false},"widget_sizes":{"notes":[360.,240.]},"location":{"name":"Zagreb","latitude":45.8,"longitude":16.0},"project_roots":["/private/projects"],"unknown":"keep"})).unwrap();
    let baseline = wizard::widgets::raw().unwrap();
    let size = Size::new(820., 600.);
    let (mut app, mut native) = open(size);
    let _ = app.update(Message::Wizard(wizard::Message::Show(
        wizard::Step::Widgets,
    )));
    click(&mut app, &mut native, "wizard-widget-notes");
    let _ = app.update(Message::Wizard(wizard::Message::GroupWidgets(true)));
    assert_eq!(
        wizard::widgets::raw().unwrap(),
        baseline,
        "preview is staged"
    );
    click(&mut app, &mut native, "wizard-next");
    click(&mut app, &mut native, "wizard-down-glance");
    assert_eq!(
        app.wizard.as_ref().unwrap().widgets.active()[0],
        Widget::Notes
    );
    click(&mut app, &mut native, "wizard-back");
    assert!(app.wizard.as_ref().unwrap().widgets.selected(Widget::Notes));
    drop(app);
    let (mut app, mut native) = open(size);
    assert_eq!(step(&app), wizard::Step::Widgets);
    assert_eq!(
        app.wizard.as_ref().unwrap().widgets.active()[0],
        Widget::Notes
    );
    let store = crate::desktop::DesktopStore::from_env().unwrap();
    let state = std::fs::read_to_string(store.state_file("setup-wizard.json").unwrap()).unwrap();
    assert!(!state.contains("/private/projects") && !state.contains("Zagreb"));
    click(&mut app, &mut native, "wizard-next");
    let expected = app.wizard.as_ref().unwrap().widget_plan(&app.config);
    assert!(expected.error.is_none(), "{:?}", expected.error);
    native.capture(
        app.settings_view(),
        &controls_theme(&app.config),
        "widgets-arranged-820",
    );
    click(&mut app, &mut native, "wizard-next");
    assert_eq!(
        step(&app),
        wizard::Step::Ready,
        "existing city skips Weather"
    );
    let saved = Config::load();
    assert!(saved.glance.enabled && saved.notes.enabled);
    assert!(!saved.widget_visible(Widget::Clock));
    assert_eq!(saved.project_roots, vec!["/private/projects"]);
    assert_eq!(saved.widget_sizes["notes"], [360., 240.]);
    for (key, position) in expected.positions {
        assert_eq!(saved.widget_positions[&key], position);
    }
    let receipt = store.latest_change().unwrap().unwrap().id;
    click(&mut app, &mut native, "wizard-back");
    assert_eq!(step(&app), wizard::Step::Arrange);
    click(&mut app, &mut native, "wizard-next");
    assert_eq!(
        store.latest_change().unwrap().unwrap().id,
        receipt,
        "Next does not reapply"
    );
    click(&mut app, &mut native, "wizard-cancel");
    let mut after = wizard::widgets::raw().unwrap();
    after.as_object_mut().unwrap().remove("onboarded");
    assert_eq!(
        after, baseline,
        "one receipt restores selection and geometry exactly"
    );
}

#[test]
fn onboarding_widgets_conflict_and_failed_write_keep_draft_for_retry() {
    if child("onboarding_widgets_conflict_and_failed_write_keep_draft_for_retry") {
        return;
    }
    seed_desktop();
    Config::save_changes(json!({"enabled_widgets":["clock"],"glance":{"enabled":false}})).unwrap();
    let (mut app, _) = open(Size::new(820., 600.));
    let _ = app.update(Message::Wizard(wizard::Message::Show(
        wizard::Step::Widgets,
    )));
    let _ = app.update(Message::Wizard(wizard::Message::Widget(Widget::Notes)));
    let _ = app.update(Message::Wizard(wizard::Message::Next));
    Config::save_changes(json!({"enabled_widgets":["clock","calendar"]})).unwrap();
    let _ = app.update(Message::Wizard(wizard::Message::Next));
    assert_eq!(step(&app), wizard::Step::Arrange);
    assert!(app.wizard.as_ref().unwrap().error.is_some());
    assert!(!Config::load().notes.enabled);
    assert!(app.wizard.as_ref().unwrap().widgets.pending);
    Config::save_changes(json!({"enabled_widgets":["clock"]})).unwrap();
    let store = crate::desktop::DesktopStore::from_env().unwrap();
    let before = wizard::widgets::raw().unwrap();
    store.inject(Some(crate::desktop::store::Fault::FailWidget));
    let outcome =
        app.wizard
            .as_mut()
            .unwrap()
            .update(wizard::Message::Next, Some(&store), &Config::load());
    assert_eq!(outcome, wizard::Outcome::Changed);
    assert!(app.wizard.as_ref().unwrap().error.is_some());
    assert_eq!(wizard::widgets::raw().unwrap(), before);
    store.inject(None);
    let _ = app.update(Message::Wizard(wizard::Message::Next));
    assert_eq!(step(&app), wizard::Step::Ready);
    assert!(Config::load().notes.enabled);
}

#[test]
fn onboarding_widgets_empty_and_customized_choices_survive_setup_changes() {
    if child("onboarding_widgets_empty_and_customized_choices_survive_setup_changes") {
        return;
    }
    seed_desktop();
    let (mut app, _) = open(Size::new(820., 600.));
    let _ = app.update(Message::Wizard(wizard::Message::Show(
        wizard::Step::Widgets,
    )));
    for w in wizard::widgets::CHOICES {
        if app.wizard.as_ref().unwrap().widgets.selected(w) {
            let _ = app.update(Message::Wizard(wizard::Message::Widget(w)));
        }
    }
    let _ = app.update(Message::Wizard(wizard::Message::Show(wizard::Step::Look)));
    let _ = app.update(Message::Wizard(wizard::Message::Look("calm")));
    assert!(
        app.wizard.as_ref().unwrap().widgets.active().is_empty(),
        "a later setup does not replace customized choices"
    );
    let _ = app.update(Message::Wizard(wizard::Message::Show(
        wizard::Step::Arrange,
    )));
    let _ = app.update(Message::Wizard(wizard::Message::Next));
    assert_eq!(step(&app), wizard::Step::Ready);
    assert!(!Config::load().widgets_visible);
    assert!(
        Widget::ALL
            .into_iter()
            .all(|w| !Config::load().widget_visible(w))
    );
    let _ = app.update(Message::Wizard(wizard::Message::Back));
    assert_eq!(
        step(&app),
        wizard::Step::Arrange,
        "Back skips unvisited Weather"
    );
    let _ = app.update(Message::Wizard(wizard::Message::UseSetupWidgets));
    assert!(
        app.wizard.as_ref().unwrap().error.is_none(),
        "{:?}",
        app.wizard.as_ref().unwrap().error
    );
    assert!(!app.wizard.as_ref().unwrap().widgets.active().is_empty());
    assert!(app.wizard.as_ref().unwrap().widgets.pending);
    assert!(
        !Config::load().widgets_visible,
        "resetting a setup is still a draft"
    );
}

#[test]
fn onboarding_widgets_disconnection_cancels_drag_and_preserves_other_display() {
    if child("onboarding_widgets_disconnection_cancels_drag_and_preserves_other_display") {
        return;
    }
    use crate::displays::{
        OutputEvent,
        test_support::{Wire, info},
    };
    seed_desktop();
    Config::save_changes(json!({"enabled_widgets":["clock","weather"],"widget_display":"A","widget_monitors":{"notes":{"display":"B","hide_disconnected":true}},"notes":{"enabled":true},"glance":{"enabled":false}})).unwrap();
    let (mut app, _) = open(Size::new(820., 600.));
    let wire = Wire::new();
    let a = wire.output(1);
    let b = wire.output(2);
    let _ = app.update(Message::DisplayOutput(
        OutputEvent::Created(Some(info(Some("A"), (1920, 1080)))),
        a,
    ));
    let _ = app.update(Message::DisplayOutput(
        OutputEvent::Created(Some(info(Some("B"), (1080, 1920)))),
        b.clone(),
    ));
    let _ = app.update(Message::Wizard(wizard::Message::Show(
        wizard::Step::Arrange,
    )));
    let plan = app.wizard.as_ref().unwrap().widget_plan(&app.config);
    assert_eq!(plan.elsewhere, vec![Widget::Notes]);
    let _ = app.update(Message::Wizard(wizard::Message::DragWidget("clock".into())));
    let _ = app.update(Message::DisplayOutput(OutputEvent::Removed, b));
    assert!(app.wizard.as_ref().unwrap().widgets.dragging.is_none());
    let _ = app.update(Message::Wizard(wizard::Message::MoveWidget(
        "weather".into(),
        -1,
    )));
    let _ = app.update(Message::Wizard(wizard::Message::Next));
    let c = Config::load();
    assert_eq!(c.widget_monitors["notes"].display.as_deref(), Some("B"));
    assert!(c.widget_monitors["notes"].hide_disconnected);
    assert!(!c.monitor_layouts.contains_key("notes"));
}

#[test]
fn onboarding_widgets_native_drag_keyboard_target_and_light_views() {
    if child("onboarding_widgets_native_drag_keyboard_target_and_light_views") {
        return;
    }
    seed_desktop();
    Config::save_changes(json!({"enabled_widgets":["clock","weather"],"glance":{"enabled":false},"notes":{"enabled":true}})).unwrap();
    let (mut app, mut native) = open(Size::new(820., 600.));
    let _ = app.update(Message::Wizard(wizard::Message::Show(
        wizard::Step::Arrange,
    )));
    let before = app.wizard.as_ref().unwrap().widgets.active();
    let key = before[0].key();
    let target = before[1].key();
    let grip = native
        .bounds(app.settings_view(), &format!("wizard-grip-{key}"))
        .center();
    for event in [
        mouse::Event::CursorMoved { position: grip },
        mouse::Event::ButtonPressed(mouse::Button::Left),
    ] {
        for message in native
            .event(app.settings_view(), Event::Mouse(event), grip)
            .0
        {
            let _ = app.update(message);
        }
    }
    assert_eq!(
        app.wizard.as_ref().unwrap().widgets.dragging.as_deref(),
        Some(key)
    );
    let over = native
        .bounds(app.settings_view(), &format!("wizard-order-{target}"))
        .center();
    for message in native
        .event(
            app.settings_view(),
            Event::Mouse(mouse::Event::CursorMoved { position: over }),
            over,
        )
        .0
    {
        let _ = app.update(message);
    }
    let release = wizard::widget_input(
        Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)),
        cosmic::iced::event::Status::Ignored,
    )
    .unwrap();
    let _ = app.update(Message::Wizard(release));
    assert_eq!(app.wizard.as_ref().unwrap().widgets.active()[1].key(), key);
    assert!(
        !Config::load().widget_positions.contains_key(key),
        "drag remains a draft"
    );
    // Focus resolution ignores the stale clicked selection and resolves the actual native ID.
    native.operate(
        app.settings_view(),
        Box::new(cosmic::iced::advanced::widget::operation::focusable::focus(
            cosmic::widget::Id::new(format!("wizard-order-{target}")),
        )),
    );
    let found = std::sync::Arc::new(std::sync::Mutex::new(None));
    let output = found.clone();
    native.operate(
        app.settings_view(),
        Box::new(cosmic::iced::advanced::widget::operation::map(
            Box::new(wizard::FocusedWidget::default()),
            move |key| {
                *output.lock().unwrap() = Some(key);
            },
        )),
    );
    let found = found.lock().unwrap().clone().unwrap();
    assert_eq!(found, target);
    for (size, name) in [
        (Size::new(820., 600.), "820"),
        (Size::new(1280., 800.), "1280"),
    ] {
        let _ = app.update(Message::Resized(size));
        native = Native::new(size);
        for step in [
            wizard::Step::Widgets,
            wizard::Step::Arrange,
            wizard::Step::Ready,
        ] {
            let _ = app.update(Message::Wizard(wizard::Message::Show(step)));
            native.capture(
                app.settings_view(),
                &cosmic::Theme::light(),
                &format!("onboarding-{step:?}-light-{name}"),
            );
            native.capture(
                app.settings_view(),
                &cosmic::Theme::dark(),
                &format!("onboarding-{step:?}-dark-{name}"),
            );
            if name == "820" {
                native.capture_scaled(
                    app.settings_view(),
                    &cosmic::Theme::dark(),
                    1.5,
                    &format!("onboarding-{step:?}-150"),
                );
            }
        }
    }
}

#[test]
fn onboarding_widgets_worker_serializes_apply_and_queued_back() {
    if child("onboarding_widgets_worker_serializes_apply_and_queued_back") {
        return;
    }
    use cosmic::iced::futures::StreamExt;
    seed_desktop();
    Config::save_changes(json!({"enabled_widgets":["clock"],"glance":{"enabled":false}})).unwrap();
    let (mut app, _) = open(Size::new(820., 600.));
    let _ = app.update(Message::Wizard(wizard::Message::Show(
        wizard::Step::Widgets,
    )));
    let _ = app.update(Message::Wizard(wizard::Message::Widget(Widget::Notes)));
    let _ = app.update(Message::Wizard(wizard::Message::Next));
    // The same worker admission as the installed app, on the private desktop.
    crate::test_env::native_worker(&desktop_root(), true);
    std::fs::write(
        desktop_root().join("status-islands-probe.py"),
        "print('{}')\n",
    )
    .unwrap();
    let task = app.update(Message::Wizard(wizard::Message::Next));
    assert!(app.native_busy);
    let _ = app.update(Message::Wizard(wizard::Message::Back));
    assert_eq!(app.native_deferred.len(), 1);
    let messages = tokio::runtime::Builder::new_current_thread()
        .enable_time()
        .build()
        .unwrap()
        .block_on(async {
            let mut messages = Vec::new();
            if let Some(mut stream) = cosmic::iced::runtime::task::into_stream(task) {
                while let Some(action) = stream.next().await {
                    if let cosmic::iced::runtime::Action::Output(cosmic::Action::App(message)) =
                        action
                    {
                        messages.push(message);
                    }
                }
            }
            messages
        });
    assert_eq!(messages.len(), 1);
    for message in messages {
        let _ = app.update(message);
    }
    assert!(!app.native_busy);
    assert!(app.native_deferred.is_empty());
    assert_eq!(step(&app), wizard::Step::Arrange);
    assert!(Config::load().notes.enabled);
    assert!(!app.wizard.as_ref().unwrap().widgets.pending);
}

#[test]
fn onboarding_widgets_reorders_multiple_displays_in_one_transaction() {
    if child("onboarding_widgets_reorders_multiple_displays_in_one_transaction") {
        return;
    }
    use crate::displays::{
        OutputEvent,
        test_support::{Wire, info},
    };
    seed_desktop();
    Config::save_changes(json!({"enabled_widgets":["clock","weather"],"glance":{"enabled":false},"notes":{"enabled":true},"music":{"enabled":true},"widget_display":"A","widget_monitors":{"notes":{"display":"B"},"music":{"display":"B"}}})).unwrap();
    let (mut app, _) = open(Size::new(820., 600.));
    let wire = Wire::new();
    let _ = app.update(Message::DisplayOutput(
        OutputEvent::Created(Some(info(Some("A"), (1920, 1080)))),
        wire.output(1),
    ));
    let _ = app.update(Message::DisplayOutput(
        OutputEvent::Created(Some(info(Some("B"), (1080, 1920)))),
        wire.output(2),
    ));
    let _ = app.update(Message::Wizard(wizard::Message::Show(
        wizard::Step::Arrange,
    )));
    let _ = app.update(Message::Wizard(wizard::Message::SetupWidgetOrder));
    let _ = app.update(Message::Wizard(wizard::Message::WidgetDisplay(Some(
        "B".into(),
    ))));
    let _ = app.update(Message::Wizard(wizard::Message::SetupWidgetOrder));
    let plan = app.wizard.as_ref().unwrap().widget_plan(&app.config);
    assert!(plan.error.is_none(), "{:?}", plan.error);
    assert_eq!(plan.positions.len(), 4);
    let _ = app.update(Message::Wizard(wizard::Message::Next));
    assert!(app.wizard.as_ref().unwrap().error.is_none());
    let saved = Config::load();
    for (key, display) in [
        ("clock", "A"),
        ("weather", "A"),
        ("notes", "B"),
        ("music", "B"),
    ] {
        assert_eq!(
            saved.monitor_layouts[key][display].position,
            Some(plan.positions[key])
        );
    }
}

#[test]
fn onboarding_widgets_legacy_sessions_and_progress_write_failure_are_explicit() {
    if child("onboarding_widgets_legacy_sessions_and_progress_write_failure_are_explicit") {
        return;
    }
    seed_desktop();
    let store = crate::desktop::DesktopStore::from_env().unwrap();
    let path = store.state_file("setup-wizard.json").unwrap();
    let mut fresh = wizard::Wizard::new(false, wizard::Bars::starter(false), true);
    fresh.update(wizard::Message::Start, Some(&store), &Config::load());
    assert!(fresh.error.is_none(), "{:?}", fresh.error);
    assert!(path.is_file());
    for step in ["welcome", "look", "bars", "weather", "ready"] {
        crate::config::write_config_text(
            &path,
            Some(
                &json!({"format":"halcyon-setup","version":1,"step":step,"changes":[]}).to_string(),
            ),
        )
        .unwrap();
        assert!(
            wizard::Wizard::resume(&store, false, wizard::Bars::starter(false), true).is_some(),
            "legacy {step}"
        );
    }
    std::fs::remove_file(&path).unwrap();
    std::fs::create_dir(&path).unwrap();
    let mut wizard = wizard::Wizard::new(false, wizard::Bars::starter(false), true);
    wizard.update(wizard::Message::Start, Some(&store), &Config::load());
    assert!(
        wizard
            .error
            .as_deref()
            .is_some_and(|e| e.starts_with("Could not save setup progress:"))
    );
    std::fs::remove_dir(path).unwrap();
}

#[test]
fn onboarding_widgets_ready_requires_review_of_unapplied_choices() {
    if child("onboarding_widgets_ready_requires_review_of_unapplied_choices") {
        return;
    }
    seed_desktop();
    let (mut app, _) = open(Size::new(820., 600.));
    let before = wizard::widgets::raw().unwrap();
    let _ = app.update(Message::Wizard(wizard::Message::Show(wizard::Step::Ready)));
    let _ = app.update(Message::Wizard(wizard::Message::Show(
        wizard::Step::Widgets,
    )));
    let _ = app.update(Message::Wizard(wizard::Message::Widget(Widget::Notes)));
    let _ = app.update(Message::Wizard(wizard::Message::Back));
    assert_eq!(step(&app), wizard::Step::Ready);
    let _ = app.update(Message::Wizard(wizard::Message::Finish));
    assert_eq!(step(&app), wizard::Step::Arrange);
    assert_eq!(
        wizard::widgets::raw().unwrap(),
        before,
        "Finish cannot discard a staged edit"
    );
    let order = app.wizard.as_ref().unwrap().widgets.active();
    let _ = app.update(Message::Wizard(wizard::Message::SetupWidgetOrder));
    let _ = app.update(Message::UndoDesktop);
    assert_eq!(app.wizard.as_ref().unwrap().widgets.active(), order);
    assert_eq!(
        wizard::widgets::raw().unwrap(),
        before,
        "Ctrl+Z only undoes a move here"
    );
    let _ = app.update(Message::Wizard(wizard::Message::Skip));
    assert!(!app.wizard.as_ref().unwrap().widgets.pending);
    assert_eq!(wizard::widgets::raw().unwrap(), before);
}

fn auditfix_complete_native(app: &mut Settings, task: Task<Message>) {
    use cosmic::iced::futures::StreamExt;
    let messages = tokio::runtime::Builder::new_current_thread()
        .enable_time()
        .build()
        .unwrap()
        .block_on(async {
            let mut messages = Vec::new();
            if let Some(mut stream) = cosmic::iced::runtime::task::into_stream(task) {
                while let Some(action) = stream.next().await {
                    if let cosmic::iced::runtime::Action::Output(cosmic::Action::App(message)) =
                        action
                    {
                        messages.push(message);
                    }
                }
            }
            messages
        });
    for message in messages {
        let _ = app.update(message);
    }
}

#[test]
fn auditfix_ob04_resume_and_apply_recapture_named_panel_clearance() {
    if child("auditfix_ob04_resume_and_apply_recapture_named_panel_clearance") {
        return;
    }
    seed_desktop();
    let system = desktop_root().join("system/cosmic");
    std::fs::write(
        system.join("com.system76.CosmicPanel/v1/entries"),
        "[\"Panel\"]",
    )
    .unwrap();
    for (key, value) in [
        ("anchor", "Left"),
        ("size", "M"),
        ("padding", "4"),
        ("margin", "0"),
        ("autohide", "Never"),
        ("output", "Name(\"A\")"),
    ] {
        std::fs::write(system.join(PANEL).join("v1").join(key), value).unwrap();
    }
    Config::save_changes(
        json!({"enabled_widgets":["clock"],"glance":{"enabled":false},"widget_display":"A"}),
    )
    .unwrap();
    use crate::displays::{
        OutputEvent,
        test_support::{Wire, info},
    };
    let wire = Wire::new();
    let (mut app, _) = open(Size::new(820., 600.));
    for (id, name) in [(1, "A"), (2, "B")] {
        let _ = app.update(Message::DisplayOutput(
            OutputEvent::Created(Some(info(Some(name), (1920, 1080)))),
            wire.output(id),
        ));
    }
    let _ = app.update(Message::Wizard(wizard::Message::Show(
        wizard::Step::Arrange,
    )));
    let _ = app.update(Message::Wizard(wizard::Message::SetupWidgetOrder));
    let first = app.wizard.as_ref().unwrap().widget_plan(&app.config);
    let clock = first
        .snapshot
        .widgets
        .iter()
        .find(|g| g.key == "clock")
        .unwrap();
    assert_eq!(clock.position[0], 88.);
    let displays = app.displays.clone();
    drop(app);
    let (mut app, _) = open(Size::new(820., 600.));
    app.displays = displays;
    app.wizard
        .as_mut()
        .unwrap()
        .refresh_widgets(&app.config, &app.displays);
    assert_eq!(step(&app), wizard::Step::Arrange);
    let resumed = app.wizard.as_ref().unwrap().widget_plan(&app.config);
    assert_eq!(first.positions, resumed.positions);
    // A newly changed bar is also reread at review/apply, without restarting.
    std::fs::write(system.join(PANEL).join("v1/padding"), "8").unwrap();
    let _ = app.update(Message::Wizard(wizard::Message::Next));
    assert!(app.wizard.as_ref().unwrap().error.is_none());
    let applied = app.wizard.as_ref().unwrap().widget_plan(&app.config);
    assert!(
        (applied
            .snapshot
            .widgets
            .iter()
            .find(|g| g.key == "clock")
            .unwrap()
            .position[0]
            - 96.)
            .abs()
            < 0.01
    );
    // The named left bar on A must not consume space when moving to B.
    let _ = app.update(Message::Wizard(wizard::Message::Show(
        wizard::Step::Arrange,
    )));
    let _ = app.update(Message::Wizard(wizard::Message::WidgetDisplay(Some(
        "B".into(),
    ))));
    let _ = app.update(Message::Wizard(wizard::Message::MoveWidgetsHere));
    let plan = app.wizard.as_ref().unwrap().widget_plan(&app.config);
    assert_eq!(
        plan.snapshot
            .widgets
            .iter()
            .find(|g| g.key == "clock")
            .unwrap()
            .position[0],
        24.
    );
}

fn auditfix_ob05_undo_then_cancel(worker: bool) {
    seed_desktop();
    let (mut app, _) = open(Size::new(820., 600.));
    let _ = app.update(Message::Wizard(wizard::Message::Start));
    let _ = app.update(Message::Wizard(wizard::Message::Look("calm")));
    assert!(Config::load().appearance.family.is_some());
    let before_notes = Config::load().notes.enabled;
    let _ = app.update(Message::Wizard(wizard::Message::Show(
        wizard::Step::Arrange,
    )));
    let _ = app.update(Message::Wizard(wizard::Message::Widget(Widget::Notes)));
    let _ = app.update(Message::Wizard(wizard::Message::Next));
    assert!(app.wizard.as_ref().unwrap().error.is_none());
    let _ = app.update(Message::Wizard(wizard::Message::Show(wizard::Step::Ready)));
    let store = crate::desktop::DesktopStore::from_env().unwrap();
    let id = store.latest_change().unwrap().unwrap().id;
    assert_ne!(Config::load().notes.enabled, before_notes);
    if worker {
        crate::test_env::native_worker(&desktop_root(), true);
        std::fs::write(
            desktop_root().join("status-islands-probe.py"),
            "print('{}')\n",
        )
        .unwrap();
    }
    let task = app.update(Message::UndoDesktop);
    if worker {
        assert!(app.native_busy);
        auditfix_complete_native(&mut app, task);
        std::fs::remove_file(desktop_root().join("status-islands-probe.py")).unwrap();
        crate::test_env::native_worker(&desktop_root(), false);
    }
    assert!(!app.error, "{}", app.status);
    assert_eq!(Config::load().notes.enabled, before_notes);
    let state: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(store.state_file("setup-wizard.json").unwrap()).unwrap(),
    )
    .unwrap();
    assert!(state["changes"].as_array().unwrap().iter().all(|c| {
        crate::desktop::store::LatestChange::decode(c.as_str().unwrap())
            .unwrap()
            .id
            != id
    }));
    drop(app);
    let (mut app, _) = open(Size::new(820., 600.));
    let _ = app.update(Message::Wizard(wizard::Message::Cancel));
    assert!(app.wizard.is_none());
    assert_eq!(Config::load().appearance.family, None);
}

#[test]
fn auditfix_ob05_ready_global_undo_retires_wizard_receipt_across_reopen() {
    if child("auditfix_ob05_ready_global_undo_retires_wizard_receipt_across_reopen") {
        return;
    }
    auditfix_ob05_undo_then_cancel(false);
}

#[test]
fn auditfix_ob05_worker_undo_retires_wizard_receipt_before_cancel() {
    if child("auditfix_ob05_worker_undo_retires_wizard_receipt_before_cancel") {
        return;
    }
    auditfix_ob05_undo_then_cancel(true);
}

#[test]
fn auditfix_ob06_rendered_next_cannot_apply_after_worker_navigation() {
    if child("auditfix_ob06_rendered_next_cannot_apply_after_worker_navigation") {
        return;
    }
    seed_desktop();
    Config::save_changes(
        json!({"enabled_widgets":["clock"],"glance":{"enabled":false},"notes":{"enabled":false}}),
    )
    .unwrap();
    let (mut app, mut native) = open(Size::new(820., 600.));
    let _ = app.update(Message::Wizard(wizard::Message::Show(
        wizard::Step::Widgets,
    )));
    let _ = app.update(Message::Wizard(wizard::Message::Widget(Widget::Notes)));
    // Obtain the actual message emitted by the rendered control.
    let point = native.bounds(app.settings_view(), "wizard-next").center();
    let mut emitted = Vec::new();
    for event in [
        mouse::Event::CursorMoved { position: point },
        mouse::Event::ButtonPressed(mouse::Button::Left),
        mouse::Event::ButtonReleased(mouse::Button::Left),
    ] {
        emitted.extend(
            native
                .event(app.settings_view(), Event::Mouse(event), point)
                .0,
        );
    }
    let next = emitted.into_iter().find(|m| matches!(m, Message::Wizard(edit) if matches!(edit.action(), wizard::Message::Next))).expect("Next emitted");
    assert!(matches!(
        &next,
        Message::Wizard(wizard::Message::Rendered(wizard::Step::Widgets, _, _))
    ));
    let before = wizard::widgets::raw().unwrap();
    crate::test_env::native_worker(&desktop_root(), true);
    std::fs::write(
        desktop_root().join("status-islands-probe.py"),
        "print('{}')\n",
    )
    .unwrap();
    let task = app.update(next.clone());
    assert!(app.native_busy);
    let _ = app.update(next.clone());
    assert_eq!(app.native_deferred.len(), 1);
    auditfix_complete_native(&mut app, task);
    assert!(!app.native_busy);
    assert!(app.native_deferred.is_empty());
    assert_eq!(step(&app), wizard::Step::Arrange);
    assert_eq!(wizard::widgets::raw().unwrap(), before);
    assert!(app.wizard.as_ref().unwrap().widgets.pending);
    // Returning to the same step does not make the old rendered identity current.
    let _ = app.update(Message::Wizard(wizard::Message::Back));
    assert_eq!(step(&app), wizard::Step::Widgets);
    let _ = app.update(next);
    assert!(!app.native_busy);
    assert_eq!(step(&app), wizard::Step::Widgets);
    // A newly rendered Apply commits once; a duplicate cannot skip Weather.
    std::fs::remove_file(desktop_root().join("status-islands-probe.py")).unwrap();
    crate::test_env::native_worker(&desktop_root(), false);
    let _ = app.update(Message::Wizard(wizard::Message::Widget(Widget::Weather)));
    let _ = app.update(Message::Wizard(wizard::Message::Next));
    let apply = Message::Wizard(app.wizard.as_ref().unwrap().rendered(wizard::Message::Next));
    crate::test_env::native_worker(&desktop_root(), true);
    std::fs::write(
        desktop_root().join("status-islands-probe.py"),
        "print('{}')\n",
    )
    .unwrap();
    let task = app.update(apply.clone());
    let _ = app.update(apply);
    auditfix_complete_native(&mut app, task);
    assert!(!app.native_busy);
    assert_eq!(step(&app), wizard::Step::Weather);
    assert!(Config::load().notes.enabled);
}

#[test]
fn auditfix_ob05_failed_global_undo_keeps_receipt_for_conflict_review() {
    if child("auditfix_ob05_failed_global_undo_keeps_receipt_for_conflict_review") {
        return;
    }
    seed_desktop();
    Config::save_changes(
        json!({"enabled_widgets":["clock"],"glance":{"enabled":false},"notes":{"enabled":false}}),
    )
    .unwrap();
    let (mut app, _) = open(Size::new(820., 600.));
    let _ = app.update(Message::Wizard(wizard::Message::Show(
        wizard::Step::Arrange,
    )));
    let _ = app.update(Message::Wizard(wizard::Message::Widget(Widget::Notes)));
    let _ = app.update(Message::Wizard(wizard::Message::Next));
    let _ = app.update(Message::Wizard(wizard::Message::Show(wizard::Step::Ready)));
    let store = crate::desktop::DesktopStore::from_env().unwrap();
    let id = store.latest_change().unwrap().unwrap().id;
    let state_path = store.state_file("setup-wizard.json").unwrap();
    let progress = std::fs::read(&state_path).unwrap();
    // Simulate an external writer, outside the owned action history.
    let mut raw = wizard::widgets::raw().unwrap();
    raw["notes"]["enabled"] = false.into();
    std::fs::write(Config::path().unwrap(), raw.to_string()).unwrap();
    let _ = app.update(Message::UndoDesktop);
    assert!(app.error);
    assert!(app.undo_review.is_some());
    assert_eq!(store.latest_change().unwrap().unwrap().id, id);
    assert!(app.wizard.as_ref().unwrap().has_changes());
    assert_eq!(std::fs::read(state_path).unwrap(), progress);
    assert_eq!(wizard::widgets::raw().unwrap(), raw);
}

/// Leave two real wizard transactions and a separate, unapplied draft on disk.
/// The next Settings instance must take the ordinary destination/init path.
fn auditfix_state02_unfinished_setup() -> (crate::desktop::DesktopStore, String, String) {
    seed_desktop();
    OPEN_WIZARD.store(true, std::sync::atomic::Ordering::Relaxed);
    let (mut app, _) = Settings::init(Core::default(), Page::Overview);
    let _ = app.update(Message::Wizard(wizard::Message::Start));
    let _ = app.update(Message::Wizard(wizard::Message::Look("focus")));
    assert!(app.wizard.as_ref().unwrap().error.is_none());
    assert!(Config::load().appearance.family.is_some());
    assert!(!Config::load().notes.enabled);
    let store = crate::desktop::DesktopStore::from_env().unwrap();
    let look_id = store.latest_change().unwrap().unwrap().id;
    let _ = app.update(Message::Wizard(wizard::Message::Show(
        wizard::Step::Arrange,
    )));
    let _ = app.update(Message::Wizard(wizard::Message::Widget(Widget::Notes)));
    let _ = app.update(Message::Wizard(wizard::Message::Next));
    assert!(app.wizard.as_ref().unwrap().error.is_none());
    assert!(Config::load().notes.enabled);
    let widget_id = store.latest_change().unwrap().unwrap().id;
    assert_ne!(widget_id, look_id);
    let _ = app.update(Message::Wizard(wizard::Message::Show(
        wizard::Step::Widgets,
    )));
    let _ = app.update(Message::Wizard(wizard::Message::Widget(Widget::Music)));
    let _ = app.update(Message::Wizard(wizard::Message::Startup(false)));
    assert!(app.wizard.as_ref().unwrap().widgets.pending);
    assert!(app.wizard.as_ref().unwrap().error.is_none());
    drop(app);
    (store, widget_id, look_id)
}

fn auditfix_state02_shown_undo(app: &mut Settings, id: &str, worker: bool) {
    assert!(app.wizard.is_none());
    assert_eq!(
        app.last_desktop_change
            .as_ref()
            .map(|(_, _, id)| id.as_str()),
        Some(id)
    );
    let probe = desktop_root().join("status-islands-probe.py");
    if worker {
        crate::test_env::native_worker(&desktop_root(), true);
        std::fs::write(&probe, "print('{}')\n").unwrap();
    }
    let task = app.update(Message::UndoDesktopShown(Some(id.into())));
    assert_eq!(app.native_busy, worker);
    if worker {
        auditfix_complete_native(app, task);
        std::fs::remove_file(probe).unwrap();
        crate::test_env::native_worker(&desktop_root(), false);
    }
    assert!(!app.native_busy);
    assert!(app.wizard.is_none());
}

fn auditfix_state02_ordinary_undo_then_cancel(worker: bool) {
    let (store, widget_id, look_id) = auditfix_state02_unfinished_setup();
    let path = store.state_file("setup-wizard.json").unwrap();
    let mut expected: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
    // Updating a closed wizard must preserve fields not modeled by this version.
    expected["future_progress"] = json!({"page":"review", "choices":[1,2,3]});
    std::fs::write(&path, expected.to_string()).unwrap();
    let receipts = expected["changes"].as_array_mut().unwrap();
    assert_eq!(receipts.len(), 2);
    assert_eq!(
        crate::desktop::store::LatestChange::decode(receipts[1].as_str().unwrap())
            .unwrap()
            .id,
        widget_id
    );
    receipts.pop();

    let (mut app, _) = Settings::init(Core::default(), Page::General);
    auditfix_state02_shown_undo(&mut app, &widget_id, worker);
    assert!(!app.error, "{}", app.status);
    assert!(!Config::load().notes.enabled);
    assert!(Config::load().appearance.family.is_some());
    let saved: serde_json::Value = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
    assert_eq!(saved, expected, "all unrelated durable progress stays");
    assert_eq!(store.latest_change().unwrap().unwrap().id, look_id);

    let _ = app.update(Message::OpenWizard);
    let resumed = app.wizard.as_ref().unwrap();
    assert_eq!(resumed.step, wizard::Step::Widgets);
    assert_eq!(resumed.look, Some("focus"));
    assert!(!resumed.startup);
    assert!(resumed.startup_touched);
    assert!(resumed.widgets.pending);
    assert_eq!(
        serde_json::to_value(&resumed.widgets).unwrap(),
        saved["widgets"]
    );
    let _ = app.update(Message::Wizard(wizard::Message::Cancel));
    assert!(
        app.wizard.is_none(),
        "{:?}",
        app.wizard.as_ref().and_then(|w| w.error.as_ref())
    );
    assert_eq!(Config::load().appearance.family, None);
    assert!(!Config::load().notes.enabled);
    assert!(!path.exists());
}

#[test]
fn auditfix_state02_ordinary_settings_undo_then_reopen_cancel() {
    if child("auditfix_state02_ordinary_settings_undo_then_reopen_cancel") {
        return;
    }
    auditfix_state02_ordinary_undo_then_cancel(false);
}

#[test]
fn auditfix_state02_ordinary_settings_worker_undo_then_reopen_cancel() {
    if child("auditfix_state02_ordinary_settings_worker_undo_then_reopen_cancel") {
        return;
    }
    auditfix_state02_ordinary_undo_then_cancel(true);
}

fn auditfix_state02_ordinary_conflict_keeps_progress(worker: bool) {
    let (store, widget_id, _) = auditfix_state02_unfinished_setup();
    let path = store.state_file("setup-wizard.json").unwrap();
    let progress = std::fs::read(&path).unwrap();
    // Matching the old field value is not proof that the receipt was undone.
    let mut external = wizard::widgets::raw().unwrap();
    external["notes"]["enabled"] = false.into();
    std::fs::write(Config::path().unwrap(), external.to_string()).unwrap();
    let (mut app, _) = Settings::init(Core::default(), Page::General);
    auditfix_state02_shown_undo(&mut app, &widget_id, worker);
    assert!(app.error);
    assert!(app.undo_review.is_some());
    assert_eq!(store.latest_change().unwrap().unwrap().id, widget_id);
    assert_eq!(std::fs::read(&path).unwrap(), progress);
    assert_eq!(wizard::widgets::raw().unwrap(), external);

    let _ = app.update(Message::OpenWizard);
    let _ = app.update(Message::Wizard(wizard::Message::Cancel));
    let wizard = app
        .wizard
        .as_ref()
        .expect("Cancel must keep the real conflict");
    assert!(wizard.error.is_some());
    assert!(wizard.has_changes());
    assert_eq!(std::fs::read(&path).unwrap(), progress);
    assert_eq!(wizard::widgets::raw().unwrap(), external);
}

#[test]
fn auditfix_state02_ordinary_settings_conflict_keeps_receipt() {
    if child("auditfix_state02_ordinary_settings_conflict_keeps_receipt") {
        return;
    }
    auditfix_state02_ordinary_conflict_keeps_progress(false);
}

#[test]
fn auditfix_state02_ordinary_settings_worker_conflict_keeps_receipt() {
    if child("auditfix_state02_ordinary_settings_worker_conflict_keeps_receipt") {
        return;
    }
    auditfix_state02_ordinary_conflict_keeps_progress(true);
}

#[test]
fn auditfix_state02_ordinary_undo_preserves_opaque_receipts_and_progress() {
    if child("auditfix_state02_ordinary_undo_preserves_opaque_receipts_and_progress") {
        return;
    }
    let (store, widget_id, look_id) = auditfix_state02_unfinished_setup();
    let path = store.state_file("setup-wizard.json").unwrap();
    let mut expected: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
    expected["future_progress"] = json!({"revision":2});
    let receipts = expected["changes"].as_array_mut().unwrap();
    assert_eq!(receipts.len(), 2);
    receipts.push(json!(format!("{{\"id\":\"{widget_id}\"}}")));
    receipts.push(json!({"id":widget_id, "future_receipt":true}));
    receipts.push(json!("damaged receipt"));
    std::fs::write(&path, expected.to_string()).unwrap();
    expected["changes"].as_array_mut().unwrap().remove(1);
    let (mut app, _) = Settings::init(Core::default(), Page::General);
    auditfix_state02_shown_undo(&mut app, &widget_id, false);
    assert!(!app.error, "{}", app.status);
    assert!(!Config::load().notes.enabled);
    assert_eq!(store.latest_change().unwrap().unwrap().id, look_id);
    let saved: serde_json::Value = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
    assert_eq!(saved, expected);
}

#[test]
fn auditfix_state02_ordinary_undo_does_not_create_or_replace_unknown_state() {
    if child("auditfix_state02_ordinary_undo_does_not_create_or_replace_unknown_state") {
        return;
    }
    seed_desktop();
    let store = crate::desktop::DesktopStore::from_env().unwrap();
    let path = store.state_file("setup-wizard.json").unwrap();
    for (state, invalid) in [
        (None, false),
        (Some("{broken".into()), true),
        (Some(json!({"format":"unknown", "version":1, "changes":[]}).to_string()), true),
        (Some(json!({"format":"halcyon-setup", "version":99, "changes":[]}).to_string()), true),
        (Some(json!({"format":"halcyon-setup", "version":1, "changes":{}}).to_string()), true),
        (Some(" ".repeat(2 * 1024 * 1024 + 1)), true),
        (Some(json!({"format":"halcyon-setup", "version":1, "changes":["unknown receipt"], "future_progress":true}).to_string()), false),
    ] {
        let before = Config::load().notes.enabled;
        crate::desktop::actions::preferences(&Config::path().unwrap(), "Notes", |raw| {
            raw["notes"] = json!({"enabled":!before});
            Ok(())
        })
        .unwrap();
        let id = store.latest_change().unwrap().unwrap().id;
        if let Some(text) = state.as_ref() {
            std::fs::write(&path, text).unwrap();
        }
        let (mut app, _) = Settings::init(Core::default(), Page::General);
        auditfix_state02_shown_undo(&mut app, &id, false);
        assert_eq!(Config::load().notes.enabled, before);
        assert_eq!(app.error, invalid, "{}", app.status);
        if invalid {
            assert!(app.status.starts_with("Undo succeeded, but could not save setup progress:"));
        }
        assert_eq!(std::fs::read_to_string(&path).ok(), state);
    }
}

#[test]
fn polish050_compact_choices_and_seven_step_progress_keep_default_bar_navigation() {
    if child("polish050_compact_choices_and_seven_step_progress_keep_default_bar_navigation") {
        return;
    }
    seed_desktop();
    for size in [Size::new(820., 600.), Size::new(1024., 760.)] {
        let (mut app, mut native) = open(size);
        click(&mut app, &mut native, "wizard-start");
        click(&mut app, &mut native, "wizard-look-calm");
        click(&mut app, &mut native, "wizard-next");
        assert_eq!(step(&app), wizard::Step::Bars);
        native.text_bounds(app.settings_view(), "Step 3 of 7 · Dock & panel");
        let baseline = wizard::widgets::raw().unwrap();
        let dock = user(DOCK, "border_radius");
        let footer = native.bounds(app.settings_view(), "wizard-next");
        for id in [
            "wizard-panel-bar",
            "wizard-panel-islands",
            "wizard-panel-square",
            "wizard-dock-rounded",
            "wizard-dock-floating",
            "wizard-dock-hides",
        ] {
            let bounds = native.bounds(app.settings_view(), id);
            assert!(
                bounds.y >= 0. && bounds.y + bounds.height < footer.y,
                "{id}: {bounds:?}"
            );
        }
        native.capture(
            app.settings_view(),
            &controls_theme(&app.config),
            &format!("polish050-bars-{}", size.width),
        );
        click(&mut app, &mut native, "wizard-next");
        assert_eq!(
            step(&app),
            wizard::Step::Widgets,
            "Untouched defaults accept Next"
        );
        assert_eq!(wizard::widgets::raw().unwrap(), baseline);
        assert_eq!(user(DOCK, "border_radius"), dock);
        native.text_bounds(app.settings_view(), "Step 4 of 7 · Your widgets");
        let footer = native.bounds(app.settings_view(), "wizard-next");
        for widget in wizard::widgets::CHOICES {
            let bounds = native.bounds(
                app.settings_view(),
                &format!("wizard-widget-{}", widget.key()),
            );
            assert!(
                bounds.y >= 0. && bounds.y + bounds.height < footer.y,
                "{} must be visible without inner scrolling: {bounds:?}, {footer:?}",
                widget.key()
            );
        }
        let grouping = native.bounds(app.settings_view(), "wizard-widget-group");
        assert!(grouping.y >= 0. && grouping.y + grouping.height < footer.y);
        let selected = app.wizard.as_ref().unwrap().widgets.selected.clone();
        let grouped = app.wizard.as_ref().unwrap().widgets.grouped;
        click(&mut app, &mut native, "wizard-widget-group");
        assert_eq!(app.wizard.as_ref().unwrap().widgets.grouped, !grouped);
        assert_eq!(app.wizard.as_ref().unwrap().widgets.selected, selected);
        click(&mut app, &mut native, "wizard-widget-group");
        assert_eq!(app.wizard.as_ref().unwrap().widgets.grouped, grouped);
        assert_eq!(
            wizard::widgets::raw().unwrap(),
            baseline,
            "Group preview must not commit before Arrange"
        );
        native.capture(
            app.settings_view(),
            &controls_theme(&app.config),
            &format!("polish050-widget-choices-{}", size.width),
        );
        for current in [
            wizard::Step::Arrange,
            wizard::Step::Weather,
            wizard::Step::Ready,
        ] {
            let _ = app.update(Message::Wizard(wizard::Message::Show(current)));
            native.text_bounds(
                app.settings_view(),
                &format!("Step {} of 7 · {}", current.index() + 1, current.title()),
            );
        }
        click(&mut app, &mut native, "wizard-cancel");
        assert!(app.wizard.is_none());
    }
}
