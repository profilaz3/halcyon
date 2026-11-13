// Included in settings::tests to use the isolated native app harness.

/// A small icon theme whose icons are colored tiles, as an archive the
/// fixture catalog offers for `pack`.
fn icon_pack_archive(dest: &std::path::Path, theme: &str, name: &str) {
    let mut bytes = Vec::new();
    {
        let encoder = flate2::write::GzEncoder::new(&mut bytes, flate2::Compression::fast());
        let mut tar = tar::Builder::new(encoder);
        let mut add = |path: String, data: &[u8]| {
            let mut header = tar::Header::new_gnu();
            header.set_size(data.len() as u64);
            header.set_mode(0o644);
            header.set_cksum();
            tar.append_data(&mut header, path, data).unwrap();
        };
        add(
            format!("{theme}/index.theme"),
            format!(
                "[Icon Theme]\nName={name}\nInherits=hicolor\nDirectories=scalable/places,scalable/apps\n\n\
                 [scalable/places]\nSize=48\nMinSize=16\nMaxSize=512\nType=Scalable\nContext=Places\n\n\
                 [scalable/apps]\nSize=48\nMinSize=16\nMaxSize=512\nType=Scalable\nContext=Applications\n"
            )
            .as_bytes(),
        );
        let folder = |color: &str| {
            format!(
                "<svg xmlns='http://www.w3.org/2000/svg' viewBox='0 0 48 48'>\
                 <path d='M4 10a3 3 0 0 1 3-3h12l4 4h18a3 3 0 0 1 3 3v24a3 3 0 0 1-3 3H7a3 3 0 0 1-3-3z' fill='{color}'/>\
                 <rect x='4' y='16' width='40' height='24' rx='3' fill='#ffffff' fill-opacity='.18'/></svg>"
            )
        };
        add(
            format!("{theme}/scalable/places/folder.svg"),
            folder("#e2a33a").as_bytes(),
        );
        add(
            format!("{theme}/scalable/places/user-home.svg"),
            folder("#5a9be0").as_bytes(),
        );
        for (icon, color, mark) in [
            ("utilities-terminal", "#2d2f3a", "M14 18l6 6-6 6M24 31h10"),
            (
                "web-browser",
                "#2f7de1",
                "M24 12a12 12 0 1 0 .1 0M12 24h24M24 12c5 4 5 20 0 24",
            ),
            ("system-file-manager", "#d97a2b", "M13 16h9l3 3h10v14H13z"),
            (
                "preferences-system",
                "#7a5bd6",
                "M24 16a8 8 0 1 0 .1 0M24 20a4 4 0 1 0 .1 0",
            ),
            (
                "accessories-text-editor",
                "#3aa676",
                "M15 15h18M15 21h18M15 27h12M15 33h8",
            ),
            (
                "image-viewer",
                "#d84f73",
                "M13 32l7-9 6 7 4-5 6 7zM30 17a3 3 0 1 0 .1 0",
            ),
        ] {
            add(
                format!("{theme}/scalable/apps/{icon}.svg"),
                format!(
                    "<svg xmlns='http://www.w3.org/2000/svg' viewBox='0 0 48 48'>\
                     <rect x='4' y='4' width='40' height='40' rx='10' fill='{color}'/>\
                     <path d='{mark}' fill='none' stroke='#fff' stroke-width='2.6' stroke-linecap='round' stroke-linejoin='round'/></svg>"
                )
                .as_bytes(),
            );
        }
        tar.into_inner().unwrap().finish().unwrap();
    }
    std::fs::create_dir_all(dest.parent().unwrap()).unwrap();
    std::fs::write(dest, bytes).unwrap();
}

/// The fixture catalog: the saved OCS page, a drawn preview for each pack
/// and an installable archive for Papirus and Tela.
fn icon_pack_catalog(dir: &std::path::Path) {
    use crate::desktop::icon_packs::ocs;
    let json = include_bytes!("../desktop/icon_packs/testdata/ocs-page.json");
    std::fs::create_dir_all(dir.join("thumbs")).unwrap();
    std::fs::write(dir.join("catalog.json"), json).unwrap();
    let page = ocs::parse(json, 0).unwrap();
    let palette = [
        [94, 129, 172],
        [64, 160, 120],
        [212, 120, 70],
        [150, 100, 200],
        [70, 150, 200],
        [200, 80, 110],
        [120, 120, 140],
        [60, 140, 160],
    ];
    for (index, pack) in page.packs.iter().enumerate() {
        let [r, g, b] = palette[index % palette.len()];
        // A gradient with a row of rounded tiles, like an icon pack preview.
        let picture = image::RgbaImage::from_fn(640, 400, |x, y| {
            let t = (x + y) as f32 / 1040.;
            let tile = (x % 120 > 30 && x % 120 < 100) && (160..230).contains(&y);
            if tile {
                image::Rgba([255, 255, 255, 230])
            } else {
                image::Rgba([
                    (r as f32 * (1. - t * 0.5)) as u8,
                    (g as f32 * (1. - t * 0.4)) as u8,
                    (b as f32 * (0.7 + t * 0.3)) as u8,
                    255,
                ])
            }
        });
        if let Some(url) = &pack.preview {
            let name = url.rsplit('/').next().unwrap();
            picture.save(dir.join("thumbs").join(name)).unwrap();
        }
    }
    for (id, theme, name) in [
        (1166289u64, "Papirus-Fixture", "Papirus Fixture"),
        (1279924, "Tela-Fixture", "Tela Fixture"),
    ] {
        let pack = page.packs.iter().find(|pack| pack.id == id).unwrap();
        let file = pack.files[0].link.rsplit('/').next().unwrap().to_string();
        icon_pack_archive(&dir.join("files").join(file), theme, name);
    }
}

/// Scrolls the page so `id` sits near the top, as a person would.
fn reveal_section(app: &Settings, native: &mut crate::native_test::Native, id: &str) {
    native.scroll(app.settings_view(), "settings-content", 0.);
    let top = native.bounds(app.settings_view(), "settings-viewport").y;
    let target = native.bounds(app.settings_view(), id).y;
    native.scroll(
        app.settings_view(),
        "settings-content",
        (target - top - 12.).max(0.),
    );
}

fn capture_icons(app: &mut Settings, id: &str, name: &str) {
    for size in [Size::new(820., 600.), Size::new(1280., 800.)] {
        send_settings(app, Message::Resized(size));
        let mut native = crate::native_test::Native::new(size);
        assert!(
            native.duplicate_ids(app.settings_view()).is_empty(),
            "{name}: {:?}",
            native.duplicate_ids(app.settings_view())
        );
        // Nothing runs off the side at this width.
        let section = native.bounds(app.settings_view(), id);
        assert!(
            section.x >= 0. && section.x + section.width <= size.width + 0.5,
            "{name} at {}: {section:?}",
            size.width
        );
        for (theme, shade) in [
            (cosmic::Theme::dark(), "dark"),
            (cosmic::Theme::light(), "light"),
        ] {
            // The fixture desktop is dark; a light desktop renders its own
            // tiles.
            app.icon_packs.previews = app.icon_packs.previews_in(shade == "dark");
            reveal_section(app, &mut native, id);
            native.capture(
                app.settings_view(),
                &theme,
                &format!("icons-{name}-{shade}-{}", size.width),
            );
        }
    }
}

fn icon_theme_now(app: &Settings) -> Option<String> {
    let draft = app.system_appearance.draft()?;
    assert!(
        !draft.edited(crate::desktop::appearance::Control::IconTheme),
        "the icon theme is applied, not left as an edit"
    );
    match draft.icon_theme() {
        crate::desktop::panels::Setting::Known(theme) => Some(theme),
        crate::desktop::panels::Setting::Unsupported(_) => None,
    }
}

/// Clicks the widget `id` in a window tall enough to show the whole page,
/// and returns the messages it sent.
fn press(app: &Settings, id: &str) -> Vec<Message> {
    use cosmic::iced::{Event, mouse};
    let size = Size::new(1280., 4000.);
    let mut native = crate::native_test::Native::new(size);
    let bounds = native.bounds(app.settings_view(), id);
    let mut sent = Vec::new();
    for input in [
        mouse::Event::CursorMoved {
            position: bounds.center(),
        },
        mouse::Event::ButtonPressed(mouse::Button::Left),
        mouse::Event::ButtonReleased(mouse::Button::Left),
    ] {
        sent.extend(
            native
                .event(app.settings_view(), Event::Mouse(input), bounds.center())
                .0,
        );
    }
    sent
}

/// Fonts and icons: installed themes as cards of real icons, the pack
/// browser (fixture catalog, no network) through search, errors, install,
/// Apply and Remove, the app icon shapes and accent folders. With
/// `DESKTOP_CARDS_LAYOUT_EVIDENCE` set, each state is captured at 820 and
/// 1280 wide, dark and light.
#[test]
fn icon_packs_cards_browser_shape_and_folders() {
    let name = "icon_packs_cards_browser_shape_and_folders";
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
    use crate::desktop::icon_packs::ocs::Sort;
    use crate::desktop::icon_packs::overlay::{self, Shape};
    use crate::desktop::icon_packs::source::Source;
    use crate::settings::icon_packs::{Install, Load, Message as Packs};
    let root = isolated_root();
    let fixture = root.join("cosmic-root");
    // Real system themes stand in for the fixture's empty Cosmic and Pop,
    // so the cards show real icons; the test still runs without them.
    let system = std::path::Path::new("/usr/share/icons");
    for id in ["Cosmic", "Pop", "Adwaita", "breeze", "hicolor"] {
        let theme = fixture.join("icons").join(id);
        if system.join(id).join("index.theme").is_file() {
            let _ = std::fs::remove_dir_all(&theme);
            std::os::unix::fs::symlink(system.join(id), &theme).unwrap();
        }
    }
    let catalog = root.join("icon-catalog");
    icon_pack_catalog(&catalog);

    let (mut app, task) = Settings::init(Core::default(), Page::DesktopAppearance);
    app.icon_packs.with_source(Source::Fixture(catalog.clone()));
    settle_settings(&mut app, task);
    assert!(app.icon_packs.paths.is_some());
    let base = icon_theme_now(&app).unwrap_or_else(|| "Cosmic".into());

    // Installed themes are cards with their own icons; hicolor is not one.
    let mut native = crate::native_test::Native::new(Size::new(1280., 800.));
    assert!(
        native
            .try_bounds(app.settings_view(), "icon-theme-Cosmic")
            .is_some()
    );
    assert!(
        native
            .try_bounds(app.settings_view(), "icon-theme-hicolor")
            .is_none()
    );
    if system.join("Cosmic/index.theme").is_file() {
        let samples = &app.icon_packs.scan.samples["Cosmic"];
        assert!(
            samples.iter().flatten().count() >= 6,
            "real sample icons for Cosmic: {samples:?}"
        );
    }
    capture_icons(&mut app, "appearance-icons", "cards");

    // The browser: skeletons, then the catalog and Popular row.
    send_settings(&mut app, Message::IconPacks(Packs::Browse(true)));
    let browser = app.icon_packs.browser.as_ref().unwrap();
    assert_eq!(browser.state, Load::Ready, "{:?}", browser.state);
    assert_eq!(browser.packs.len(), 8);
    let curated: Vec<_> = browser
        .curated
        .iter()
        .map(|pack| pack.name.as_str())
        .collect();
    assert!(
        curated.first() == Some(&"Papirus") && curated.len() >= 2,
        "{curated:?}"
    );
    assert!(
        app.icon_packs
            .thumbs
            .values()
            .all(|thumb| matches!(thumb, crate::settings::icon_packs::Thumb::Ready(_))),
        "{:?}",
        app.icon_packs.thumbs
    );
    capture_icons(&mut app, "icon-packs-browser", "browser");
    // Sorting asks again; Top rated puts the best score first.
    send_settings(&mut app, Message::IconPacks(Packs::Sort(Sort::TopRated)));
    let browser = app.icon_packs.browser.as_ref().unwrap();
    let scores: Vec<_> = browser.packs.iter().map(|pack| pack.score).collect();
    assert!(
        scores.windows(2).all(|pair| pair[0] >= pair[1]),
        "{scores:?}"
    );
    send_settings(&mut app, Message::IconPacks(Packs::Sort(Sort::Popular)));

    // Loading: skeletons until the page arrives.
    let _ = app.update(Message::IconPacks(Packs::Sort(Sort::Newest)));
    assert_eq!(
        app.icon_packs.browser.as_ref().unwrap().state,
        Load::Loading
    );
    capture_icons(&mut app, "icon-packs-browser", "browser-loading");
    send_settings(&mut app, Message::IconPacks(Packs::Sort(Sort::Popular)));

    // Empty search, then back.
    send_settings(&mut app, Message::IconPacks(Packs::Query("zzzz".into())));
    send_settings(&mut app, Message::IconPacks(Packs::Search));
    assert!(app.icon_packs.browser.as_ref().unwrap().packs.is_empty());
    capture_icons(&mut app, "icon-packs-browser", "browser-empty");
    send_settings(&mut app, Message::IconPacks(Packs::Query("tela".into())));
    send_settings(&mut app, Message::IconPacks(Packs::Search));
    let found: Vec<_> = app
        .icon_packs
        .browser
        .as_ref()
        .unwrap()
        .packs
        .iter()
        .map(|pack| pack.id)
        .collect();
    assert_eq!(found.len(), 2, "{found:?}");
    send_settings(&mut app, Message::IconPacks(Packs::Query(String::new())));
    assert_eq!(app.icon_packs.browser.as_ref().unwrap().packs.len(), 8);

    // No connection: an error with Retry, which recovers.
    app.icon_packs.with_source(Source::Offline);
    send_settings(&mut app, Message::IconPacks(Packs::Retry));
    assert!(matches!(
        app.icon_packs.browser.as_ref().unwrap().state,
        Load::Failed(_)
    ));
    capture_icons(&mut app, "icon-packs-browser", "browser-error");
    let sent = press(&app, "icon-packs-retry");
    assert!(
        matches!(sent.as_slice(), [Message::IconPacks(Packs::Retry)]),
        "{sent:?}"
    );
    app.icon_packs.with_source(Source::Fixture(catalog.clone()));
    send_settings(&mut app, Message::IconPacks(Packs::Retry));
    assert_eq!(app.icon_packs.browser.as_ref().unwrap().state, Load::Ready);

    // Install: progress, then Apply.
    let papirus = 1166289;
    let sent = press(&app, &format!("icon-pack-install-{papirus}"));
    assert!(
        matches!(sent.as_slice(), [Message::IconPacks(Packs::Install(id))] if *id == papirus),
        "{sent:?}"
    );
    let task = app.update(Message::IconPacks(Packs::Install(papirus)));
    assert_eq!(
        app.icon_packs.installs.get(&papirus),
        Some(&Install::Preparing)
    );
    // Show the download bar half way while the task waits.
    app.icon_packs.installs.insert(
        papirus,
        Install::Downloading(crate::desktop::icon_packs::install::Progress {
            done: 4 * 1024 * 1024,
            total: Some(9 * 1024 * 1024),
        }),
    );
    capture_icons(&mut app, "icon-packs-browser", "browser-installing");
    app.icon_packs.installs.insert(papirus, Install::Preparing);
    settle_settings(&mut app, task);
    assert_eq!(
        app.icon_packs.installs.get(&papirus),
        Some(&Install::Done(vec!["Papirus-Fixture".into()]))
    );
    assert_eq!(
        app.icon_packs.manifest.of_pack(papirus),
        ["Papirus-Fixture"]
    );
    assert!(fixture.join("icons/Papirus-Fixture/index.theme").is_file());
    // The new theme is a card already, before it is applied.
    let mut native = crate::native_test::Native::new(Size::new(1280., 4000.));
    assert!(
        native
            .try_bounds(app.settings_view(), "icon-theme-Papirus-Fixture")
            .is_some()
    );
    let sent = press(&app, &format!("icon-pack-install-{papirus}"));
    assert!(
        matches!(sent.as_slice(), [Message::IconPacks(Packs::Apply(id))] if *id == papirus),
        "{sent:?}"
    );
    capture_icons(&mut app, "icon-packs-browser", "browser-installed");
    send_settings(&mut app, Message::IconPacks(Packs::Apply(papirus)));
    assert_eq!(icon_theme_now(&app).as_deref(), Some("Papirus-Fixture"));
    // The shape and folder previews now draw the applied theme's icons.
    let folder = app.icon_packs.scan.folder.clone().unwrap();
    assert!(
        folder.to_string_lossy().contains("Papirus-Fixture"),
        "{folder:?}"
    );
    assert!(app.icon_packs.previews.folder.is_some());
    capture_icons(&mut app, "icon-packs-browser", "browser-applied");
    send_settings(&mut app, Message::IconPacks(Packs::Browse(false)));
    assert!(app.icon_packs.browser.is_none());

    // App icon shapes: an overlay over the chosen theme, Original restores it.
    let overlays = |fixture: &std::path::Path| {
        std::fs::read_dir(fixture.join("icons"))
            .unwrap()
            .flatten()
            .map(|entry| entry.file_name().to_string_lossy().into_owned())
            .filter(|id| overlay::is_overlay(id))
            .collect::<Vec<_>>()
    };
    for shape in [Shape::Circle, Shape::Squircle, Shape::Rounded] {
        let sent = press(&app, &format!("icon-shape-{}", shape.key()));
        assert!(
            matches!(sent.as_slice(), [Message::IconPacks(Packs::Shape(s))] if *s == shape),
            "{sent:?}"
        );
        send_settings(&mut app, Message::IconPacks(Packs::Shape(shape)));
        let theme = icon_theme_now(&app).unwrap();
        assert!(overlay::is_overlay(&theme), "{shape:?}: {theme}");
        assert_eq!(overlays(&fixture), [theme.clone()]);
        let index = std::fs::read_to_string(fixture.join("icons").join(&theme).join("index.theme"))
            .unwrap();
        assert!(
            index.contains("Inherits=Papirus-Fixture,Cosmic,hicolor"),
            "{index}"
        );
        assert!(app.icon_packs.build.is_none());
        // The chooser still shows the base as the one in use.
        assert_eq!(
            app.icon_packs.style.base.as_deref(),
            Some("Papirus-Fixture")
        );
        capture_icons(&mut app, "icon-shape", &format!("shape-{}", shape.key()));
    }
    send_settings(&mut app, Message::IconPacks(Packs::Shape(Shape::Original)));
    assert_eq!(icon_theme_now(&app).as_deref(), Some("Papirus-Fixture"));
    assert!(overlays(&fixture).is_empty(), "{:?}", overlays(&fixture));
    capture_icons(&mut app, "icon-shape", "shape-original");
    capture_icons(&mut app, "icon-folders", "folders-off");

    // Folders in the accent color, then both off restores the base.
    let sent = press(&app, "icon-folders-toggle");
    assert!(
        matches!(sent.as_slice(), [Message::IconPacks(Packs::Tint(true))]),
        "{sent:?}"
    );
    send_settings(&mut app, Message::IconPacks(Packs::Tint(true)));
    let theme = icon_theme_now(&app).unwrap();
    assert!(overlay::is_overlay(&theme), "{theme}");
    // The pack's own folder, tinted, and no app icons when only tinting.
    let dir = fixture.join("icons").join(&theme);
    for size in [64, 128, 256] {
        assert!(
            dir.join(format!("{size}x{size}/places/folder.png"))
                .is_file()
        );
        assert!(!dir.join(format!("{size}x{size}/apps")).exists());
    }
    capture_icons(&mut app, "icon-folders", "folders-on");
    let sent = press(&app, "icon-folders-toggle");
    assert!(
        matches!(sent.as_slice(), [Message::IconPacks(Packs::Tint(false))]),
        "{sent:?}"
    );
    send_settings(&mut app, Message::IconPacks(Packs::Tint(false)));
    assert_eq!(icon_theme_now(&app).as_deref(), Some("Papirus-Fixture"));
    assert!(overlays(&fixture).is_empty());

    // Remove: never the theme in use, never one Halcyon did not install.
    send_settings(
        &mut app,
        Message::IconPacks(Packs::Remove("Papirus-Fixture".into())),
    );
    assert!(fixture.join("icons/Papirus-Fixture").is_dir());
    send_settings(&mut app, Message::IconPacks(Packs::Use(base.clone())));
    assert_eq!(icon_theme_now(&app), Some(base.clone()));
    // Choosing another theme cleared the warning.
    assert!(app.icon_packs.notice.is_none());
    send_settings(
        &mut app,
        Message::IconPacks(Packs::ConfirmRemove(Some("Papirus-Fixture".into()))),
    );
    capture_icons(&mut app, "icon-theme-Papirus-Fixture", "cards-remove");
    send_settings(
        &mut app,
        Message::IconPacks(Packs::Remove("Papirus-Fixture".into())),
    );
    assert!(!fixture.join("icons/Papirus-Fixture").exists());
    assert!(app.icon_packs.manifest.of_pack(papirus).is_empty());
    send_settings(&mut app, Message::IconPacks(Packs::Remove("Pop".into())));
    assert!(fixture.join("icons/Pop").exists());
    let mut native = crate::native_test::Native::new(Size::new(1280., 4000.));
    assert!(
        native
            .try_bounds(app.settings_view(), "icon-theme-Papirus-Fixture")
            .is_none()
    );
}
