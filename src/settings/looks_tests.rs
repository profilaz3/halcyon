// Included inside settings::tests to share the isolated desktop/native harness.
// Curated Looks: what the preview draws for each part, the parts picker and
// Apply/Undo of only the chosen parts.
use crate::desktop::profiles::Domain;

/// A live desktop with a standard panel (Islands installed but off), a dock,
/// the bundled wallpapers and the app's own config, as most people have
/// before trying a look.
fn looks_desktop(root: &std::path::Path) -> Settings {
    let mut app = open_isolated_editor(root);
    let store = app.desktop.store().unwrap().clone();
    let fixture = store.fixture_root().unwrap().to_path_buf();
    crate::desktop::islands::fixture_installed(&fixture);
    let bundled = crate::desktop::wallpaper::bundled_dir(&store);
    std::fs::create_dir_all(&bundled).unwrap();
    for (name, _) in crate::desktop::wallpaper::BUNDLED {
        let _ = std::fs::copy(
            crate::desktop::wallpaper::bundled_path(
                &std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("assets/wallpapers"),
                name,
            ),
            crate::desktop::wallpaper::bundled_path(&bundled, name),
        );
    }
    let key = crate::desktop::islands::key(crate::desktop::panels::PanelKind::Panel);
    let off = dock_islands_core::Settings {
        enabled: false,
        ..Default::default()
    };
    seed(
        &root.join("cosmic-root/config/cosmic"),
        &[
            (key.namespace(), key.key(), &ron::to_string(&off).unwrap()),
            (
                "com.system76.CosmicPanel.Panel",
                "plugins_wings",
                "Some(([\"com.system76.CosmicAppletWorkspaces\"],[\"com.system76.CosmicAppletAudio\"]))",
            ),
            (
                "com.system76.CosmicPanel.Panel",
                "plugins_center",
                "Some([\"com.system76.CosmicAppletTime\"])",
            ),
        ],
    );
    app.desktop = Default::default();
    app.desktop.islands.installation = crate::desktop::islands::Installation::discover(&store);
    assert!(app.desktop.islands.installation.installed());
    app.profiles = profiles::ProfilesEditor::default();
    app.profiles.open();
    app.profiles.open_home();
    send_settings(&mut app, Message::Page(Page::DesktopProfiles));
    looks_drive(&mut app);
    app
}

/// Runs the background work (backdrops, theme colors) the fixture never
/// launches as tasks.
fn looks_drive(app: &mut Settings) {
    for _ in 0..32 {
        let backdrops = app.profiles.take_jobs();
        let themes = app.profiles.take_theme_jobs();
        let icons = app.profiles.take_icon_jobs();
        let art = app.profiles.take_icon_art_jobs();
        if backdrops.is_empty() && themes.is_empty() && icons.is_empty() && art.is_empty() {
            return;
        }
        for job in art {
            app.profiles.update(profiles::Message::IconArt(job.run()));
        }
        for job in icons {
            app.profiles.update(job.run());
        }
        for job in themes {
            app.profiles.update(job.run());
        }
        for job in backdrops {
            app.profiles.update(job.run());
        }
    }
}

fn looks_send(app: &mut Settings, message: profiles::Message) {
    send_settings(app, Message::Profiles(message));
    looks_drive(app);
}

fn looks_select(app: &mut Settings, id: &'static str) {
    looks_send(
        app,
        profiles::Message::Select(profiles::Selection::Preset(id)),
    );
}

/// Keeps exactly `parts` of the selected look.
fn looks_only(app: &mut Settings, parts: &[Domain]) {
    for domain in app.profiles.profile().unwrap().domains() {
        looks_send(
            app,
            profiles::Message::Include(domain, parts.contains(&domain)),
        );
    }
}

fn looks_capture(app: &Settings, native: &mut crate::native_test::Native, name: &str) {
    native.scroll(app.settings_view(), "settings-content", 0.);
    for (theme, tone) in [
        (cosmic::Theme::dark(), "dark"),
        (cosmic::Theme::light(), "light"),
    ] {
        native.capture(
            app.settings_view(),
            &theme,
            &format!("looks-{name}-{}-{tone}", native.size.width),
        );
    }
}

#[test]
fn looks_apply_label_counts_the_parts_kept_on() {
    let name = "looks_apply_label_counts_the_parts_kept_on";
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
    let root = isolated_root();
    let mut app = looks_desktop(&root);
    looks_select(&mut app, "carbon");
    let total = app.profiles.profile().unwrap().domains().len();
    assert!(total >= 8, "a look is a whole desktop: {total}");
    assert_eq!(app.apply_label(), format!("Apply all {total} parts"));
    looks_send(
        &mut app,
        profiles::Message::Parts(profiles::Parts::OnlyColors),
    );
    assert_eq!(app.apply_label(), format!("Apply 2 of {total} parts"));
    looks_send(
        &mut app,
        profiles::Message::Parts(profiles::Parts::KeepLayout),
    );
    assert_eq!(
        app.apply_label(),
        format!("Apply {} of {total} parts", total - 3)
    );
    looks_send(
        &mut app,
        profiles::Message::Include(Domain::Wallpaper, false),
    );
    assert_eq!(
        app.apply_label(),
        format!("Apply {} of {total} parts", total - 4)
    );
    // The chips name each part and its state; the quick choices are there.
    let mut native = crate::native_test::Native::new(Size::new(1280., 800.));
    for id in [
        "looks-parts-everything",
        "looks-parts-colors",
        "looks-parts-layout",
        "setup-include-panel",
        "setup-include-appearance",
        "setup-include-windows",
        "setup-include-desktopicons",
    ] {
        assert!(native.try_bounds(app.settings_view(), id).is_some(), "{id}");
    }
    assert!(native.duplicate_ids(app.settings_view()).is_empty());
}

/// The rows of the rendered picture between `from` and `to` (fractions of
/// its height), as RGBA bytes.
fn looks_band(
    native: &mut crate::native_test::Native,
    app: &Settings,
    id: &str,
    from: f32,
    to: f32,
) -> Vec<u8> {
    native.scroll(app.settings_view(), "settings-content", 0.);
    let area = native.bounds(app.settings_view(), id);
    let pixels = native.render(app.settings_view(), &cosmic::Theme::dark());
    let width = native.size.width.round() as usize;
    let (x0, x1) = (
        area.x.round() as usize,
        (area.x + area.width).round() as usize,
    );
    let (y0, y1) = (
        (area.y + area.height * from).round() as usize,
        (area.y + area.height * to).round() as usize,
    );
    let mut band = Vec::new();
    for y in y0..y1 {
        band.extend_from_slice(&pixels[(y * width + x0) * 4..(y * width + x1) * 4]);
    }
    band
}

#[test]
fn looks_picture_draws_only_the_chosen_parts() {
    let name = "looks_picture_draws_only_the_chosen_parts";
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
    let root = isolated_root();
    let mut app = looks_desktop(&root);
    send_settings(&mut app, Message::Resized(Size::new(1280., 800.)));
    let mut native = crate::native_test::Native::new(Size::new(1280., 800.));
    looks_select(&mut app, "halcyon");
    // The top panel band and the bottom dock band of the picture.
    let (top, bottom) = ((0.0, 0.08), (0.9, 1.0));
    looks_send(&mut app, profiles::Message::Compare(true));
    let now_top = looks_band(&mut native, &app, "profile-scene-current", top.0, top.1);
    let now_bottom = looks_band(
        &mut native,
        &app,
        "profile-scene-current",
        bottom.0,
        bottom.1,
    );
    looks_send(&mut app, profiles::Message::Compare(false));
    // Only the panel: the top band changes (islands appear), the dock does not.
    looks_only(&mut app, &[Domain::Panel]);
    let panel = app.profiles.preview().unwrap();
    let islands = panel
        .bars
        .iter()
        .find(|bar| bar.kind() == crate::desktop::panels::PanelKind::Panel)
        .unwrap()
        .islands();
    assert!(
        islands.known().unwrap().enabled,
        "islands come with the panel"
    );
    assert_ne!(
        looks_band(&mut native, &app, "profile-scene-candidate", top.0, top.1),
        now_top
    );
    assert_eq!(
        looks_band(
            &mut native,
            &app,
            "profile-scene-candidate",
            bottom.0,
            bottom.1
        ),
        now_bottom
    );
    // Only the dock: the panel band is the one on the desktop now.
    looks_only(&mut app, &[Domain::Dock]);
    let preview = app.profiles.preview().unwrap();
    let islands = preview
        .bars
        .iter()
        .find(|bar| bar.kind() == crate::desktop::panels::PanelKind::Panel)
        .unwrap()
        .islands();
    assert!(
        !islands.known().unwrap().enabled,
        "the panel stays standard"
    );
    assert_eq!(
        looks_band(&mut native, &app, "profile-scene-candidate", top.0, top.1),
        now_top
    );
    // Holding the picture shows the desktop now, whatever is chosen.
    looks_only(&mut app, &[Domain::Panel, Domain::Dock]);
    looks_send(&mut app, profiles::Message::Hold(true));
    assert_eq!(
        looks_band(&mut native, &app, "profile-scene-current", top.0, top.1),
        now_top
    );
    looks_send(&mut app, profiles::Message::Hold(false));
    // Choosing parts never writes.
    assert!(app.profiles.latest().is_none());
}

#[test]
fn looks_screenshots_gallery_parts_islands_and_compare() {
    let name = "looks_screenshots_gallery_parts_islands_and_compare";
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
    let root = isolated_root();
    let mut app = looks_desktop(&root);
    for size in [Size::new(1280., 800.), Size::new(820., 800.)] {
        send_settings(&mut app, Message::Resized(size));
        let mut native = crate::native_test::Native::new(size);
        looks_send(
            &mut app,
            profiles::Message::Select(profiles::Selection::Current),
        );
        looks_capture(&app, &mut native, "gallery");
        looks_select(&mut app, "halcyon");
        looks_capture(&app, &mut native, "all-parts");
        looks_send(
            &mut app,
            profiles::Message::Parts(profiles::Parts::OnlyColors),
        );
        looks_capture(&app, &mut native, "only-colors");
        looks_only(&mut app, &[Domain::Panel]);
        looks_capture(&app, &mut native, "islands-with-panel");
        looks_only(&mut app, &[Domain::Dock, Domain::Appearance]);
        looks_capture(&app, &mut native, "islands-without-panel");
        looks_send(
            &mut app,
            profiles::Message::Parts(profiles::Parts::Everything),
        );
        looks_send(&mut app, profiles::Message::Highlight(Some(Domain::Panel)));
        looks_capture(&app, &mut native, "highlight-panel");
        looks_send(&mut app, profiles::Message::Highlight(None));
        looks_send(&mut app, profiles::Message::Compare(true));
        looks_capture(&app, &mut native, "compare-now");
        looks_send(&mut app, profiles::Message::Compare(false));
        looks_select(&mut app, "snow");
        looks_capture(&app, &mut native, "snow");
        looks_select(&mut app, "carbon");
        looks_capture(&app, &mut native, "carbon");
    }
}

fn studio_send(app: &mut Settings, edit: profiles::studio::Edit) {
    looks_send(app, profiles::Message::Studio(edit));
}

/// Captures the studio scrolled to `y`, in both tones.
fn studio_capture(app: &Settings, native: &mut crate::native_test::Native, name: &str, y: f32) {
    assert!(
        native.duplicate_ids(app.settings_view()).is_empty(),
        "{name}: {:?}",
        native.duplicate_ids(app.settings_view())
    );
    native.scroll(app.settings_view(), "settings-content", y);
    for (theme, tone) in [
        (cosmic::Theme::dark(), "dark"),
        (cosmic::Theme::light(), "light"),
    ] {
        native.capture(
            app.settings_view(),
            &theme,
            &format!("studio-{name}-{}-{tone}", native.size.width),
        );
    }
}

#[test]
fn looks_screenshots_studio() {
    use profiles::studio::{Edit as Studio, Element as Part, Pick, Start};
    let name = "looks_screenshots_studio";
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
    let root = isolated_root();
    let mut app = looks_desktop(&root);
    let desktop = app.profiles.store().unwrap().user_root().to_path_buf();
    let before = crate::desktop::store::tests::tree(&desktop);
    for size in [Size::new(1280., 800.), Size::new(820., 800.)] {
        send_settings(&mut app, Message::Resized(size));
        let mut native = crate::native_test::Native::new(size);
        let tiles = if size.width > 1000. { 470. } else { 380. };
        looks_send(
            &mut app,
            profiles::Message::Select(profiles::Selection::Current),
        );
        studio_send(&mut app, Studio::Start(Start::Desktop));
        studio_send(&mut app, Studio::Name("Evening harbor".into()));
        studio_send(
            &mut app,
            Studio::Description("Warm evening light, soft corners".into()),
        );
        studio_send(
            &mut app,
            Studio::Pick(Part::Wallpaper, Pick::Wallpaper("halcyon-evening")),
        );
        studio_send(&mut app, Studio::Pick(Part::Weather, Pick::Weather("snow")));
        studio_send(&mut app, Studio::Pick(Part::Windows, Pick::Corners(1)));
        studio_capture(&app, &mut native, "draft", 0.);
        looks_send(&mut app, profiles::Message::Highlight(Some(Domain::Dock)));
        studio_capture(&app, &mut native, "tiles", tiles);
        looks_send(&mut app, profiles::Message::Highlight(None));
        studio_send(&mut app, Studio::Open(Some(Part::Wallpaper)));
        studio_capture(&app, &mut native, "wallpaper-picker", tiles);
        studio_send(&mut app, Studio::Open(Some(Part::Panel)));
        studio_capture(&app, &mut native, "panel-picker", tiles);
        studio_send(&mut app, Studio::Open(Some(Part::Colors)));
        studio_capture(&app, &mut native, "colors-picker", tiles);
        studio_send(&mut app, Studio::Open(None));
        studio_send(&mut app, Studio::Save);
        studio_capture(&app, &mut native, "saved", 0.);
        studio_send(&mut app, Studio::Menu(true));
        studio_capture(&app, &mut native, "menu", 0.);
        studio_send(&mut app, Studio::Delete(false));
        studio_capture(&app, &mut native, "delete", 0.);
        studio_send(&mut app, Studio::Delete(false));
        studio_send(&mut app, Studio::MakeBuiltIn);
        studio_capture(&app, &mut native, "built-in", 0.);
        let mut personal = app.profiles.studio.as_ref().unwrap().profile.clone();
        personal.wallpaper = Some(
            serde_json::from_value(serde_json::json!({
                "background": {"source": {"image": "/home/someone/cat.jpg"}, "fit": "fill"}
            }))
            .unwrap(),
        );
        app.profiles.studio.as_mut().unwrap().profile = personal;
        studio_send(&mut app, Studio::MakeBuiltIn);
        studio_capture(&app, &mut native, "built-in-refused", 0.);
        studio_send(&mut app, Studio::Review(true));
        studio_capture(&app, &mut native, "review", tiles);
        studio_send(&mut app, Studio::Close);
        assert!(app.profiles.studio.is_none());
    }
    assert_eq!(
        crate::desktop::store::tests::tree(&desktop),
        before,
        "the studio never wrote the desktop"
    );
}

#[test]
fn looks_screenshots_icon_shape_and_tint() {
    use crate::desktop::icon_packs::overlay::Shape;
    use profiles::studio::{Edit as Studio, Element as Part, Pick, Start};
    let name = "looks_screenshots_icon_shape_and_tint";
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
    let root = isolated_root();
    let mut app = looks_desktop(&root);
    let size = Size::new(1280., 800.);
    send_settings(&mut app, Message::Resized(size));
    let mut native = crate::native_test::Native::new(size);
    // Calm: circle app icons and accent folders, in the picture and its
    // icons label.
    looks_select(&mut app, "calm");
    let style = app.profiles.preview().unwrap().icons.clone().unwrap();
    assert_eq!(style.shape, Shape::Circle);
    assert!(style.tint);
    looks_capture(&app, &mut native, "icons-calm");
    // Left out, Appearance shows the desktop's own icons.
    looks_send(
        &mut app,
        profiles::Message::Include(Domain::Appearance, false),
    );
    let kept = app.profiles.preview().unwrap().icons.clone().unwrap();
    assert_eq!(kept.shape, Shape::Original);
    looks_capture(&app, &mut native, "icons-calm-appearance-kept");
    looks_send(
        &mut app,
        profiles::Message::Include(Domain::Appearance, true),
    );

    // The studio's Icons tile and picker with a squircle and accent folders.
    looks_send(
        &mut app,
        profiles::Message::Select(profiles::Selection::Current),
    );
    studio_send(&mut app, Studio::Start(Start::Desktop));
    studio_send(&mut app, Studio::Name("Shaped harbor".into()));
    studio_send(
        &mut app,
        Studio::Pick(Part::Icons, Pick::IconShape(Shape::Squircle)),
    );
    studio_send(&mut app, Studio::Pick(Part::Icons, Pick::FolderTint(true)));
    let draft = &app.profiles.studio.as_ref().unwrap().profile;
    let look = draft.appearance.as_ref().unwrap();
    assert_eq!(look.icon_shape, Some(Shape::Squircle));
    assert_eq!(look.folder_tint, Some(true));
    looks_send(
        &mut app,
        profiles::Message::Highlight(Some(Domain::Appearance)),
    );
    studio_capture(&app, &mut native, "icons-tile", 470.);
    looks_send(&mut app, profiles::Message::Highlight(None));
    studio_send(&mut app, Studio::Open(Some(Part::Icons)));
    studio_capture(&app, &mut native, "icons-picker", 470.);
    studio_capture(&app, &mut native, "icons-picker-shapes", 900.);
    studio_send(&mut app, Studio::Close);
}
