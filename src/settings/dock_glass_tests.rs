// The Dock and panel close-up draws the glass the Islands panel really
// draws: a glass style shows the wallpaper through the dock, a solid style
// keeps its dock solid even with Rainy evening's Clear glass saved.

#[test]
fn dock_closeup_shows_style_glass_on_ridge() {
    let name = "dock_closeup_shows_style_glass_on_ridge";
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
    use crate::desktop::panels::{Anchor, PanelKind};
    use crate::desktop::wallpaper::{BgEntry, BgSource, bundled_dir, bundled_path};
    use crate::desktop_theme::Family;
    use crate::materials::{Finish, Kind};
    let root = isolated_root();
    let cosmic = root.join("cosmic-root");
    let (probe, _) = Settings::init(Core::default(), Page::DesktopWallpaper);
    let bundled = bundled_dir(probe.wallpaper.store().unwrap());
    drop(probe);
    let ridge = bundled_path(&bundled, "halcyon-ridge");
    std::fs::create_dir_all(&bundled).unwrap();
    std::fs::copy(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("assets/wallpapers/halcyon-ridge.png"),
        &ridge,
    )
    .unwrap();
    seed(
        &cosmic.join("config/cosmic"),
        &[
            (
                "com.system76.CosmicBackground",
                "all",
                &ron::to_string(&BgEntry::new("all", BgSource::Path(ridge.clone()))).unwrap(),
            ),
            ("com.system76.CosmicPanel.Dock", "anchor", "Bottom"),
            ("com.system76.CosmicPanel.Dock", "opacity", "1.0"),
        ],
    );
    let mut app = open_isolated_editor(&root);
    drive_wallpaper(&mut app);
    if app.profiles.home_image().is_none() {
        app.profiles.open_home();
        for job in app.profiles.take_jobs() {
            app.profiles.update(job.run());
        }
    }
    assert!(app.profiles.home_image().is_some(), "Ridge is drawn");
    let store = app.desktop.store().unwrap().clone();
    crate::desktop::islands::fixture_installed(store.fixture_root().unwrap());
    app.desktop.islands.installation = crate::desktop::islands::Installation::discover(&store);
    assert!(app.desktop.islands.installation.installed());
    send_settings(&mut app, Message::Page(Page::DesktopDock));
    send_settings(
        &mut app,
        Message::DesktopEditor(desktop::Message::Select(PanelKind::Dock)),
    );
    let dock = app.desktop.draft(PanelKind::Dock).unwrap();
    assert_eq!(dock.anchor().known(), Some(&Anchor::Bottom));

    let rain = Finish {
        kind: Kind::Clear,
        opacity: Some(45),
        tint: 20,
    };
    let size = Size::new(1280., 800.);
    send_settings(&mut app, Message::Resized(size));
    let theme = cosmic::Theme::dark();
    // The page with and without the dock's glass, the scene only.
    let mut picture = |app: &mut Settings, family: Family, glass: bool, label: &str| {
        app.config.appearance.family = Some(family);
        app.config.materials.dock = glass.then_some(rain);
        app.config.materials.panel = glass.then_some(rain);
        let mut native = crate::native_test::Native::new(size);
        let scene = native.bounds(app.settings_view(), "desktop-scene");
        let _ = native.render(app.settings_view(), &theme);
        let pixels = native.render(app.settings_view(), &theme);
        native.capture(app.settings_view(), &theme, &format!("dock-glass-{label}"));
        let width = size.width as usize;
        let mut rows = Vec::new();
        for y in scene.y as usize..(scene.y + scene.height) as usize {
            let start = (y * width + scene.x as usize) * 4;
            rows.push(pixels[start..start + scene.width as usize * 4].to_vec());
        }
        rows
    };
    for family in [Family::Midnight, Family::Terminal, Family::Bauhaus] {
        let id = family.id();
        let plain = picture(&mut app, family, false, &format!("{id}-no-glass"));
        let rainy = picture(&mut app, family, true, &format!("{id}-rain-glass"));
        assert!(
            plain == rainy,
            "{id}: Rainy evening's glass leaves the solid dock unchanged"
        );
    }
    let plain = picture(&mut app, Family::Glass, false, "glass-no-glass");
    let rainy = picture(&mut app, Family::Glass, true, "glass-rain-glass");
    let changed = plain.iter().zip(&rainy).filter(|(a, b)| a != b).count();
    assert!(
        changed > 10,
        "Frosted shows Ridge through its 45% dock ({changed} rows differ)"
    );
}
