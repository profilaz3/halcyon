//! Widgets are easy to manage and honest: the Widgets page links to how
//! widgets look (WID-07), Daily overview says what each part needs
//! (WID-10), and previews with made-up data say "Sample" while Weather
//! shows the desktop's own reading once there is one (WID-11).
use super::*;
use crate::native_test::Native;
use cosmic::iced::{Event, mouse};

/// Runs `name` again in a child process with its own fixture desktop and
/// app config, so nothing touches the live session.
fn child(name: &str) -> bool {
    if std::env::var("DESKTOP_CARDS_WIDGETS_UX_TEST").as_deref() == Ok(name) {
        return false;
    }
    let root = std::env::temp_dir().join(format!("cards-widgets-ux-{}-{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    for dir in ["config", "data", "cache", "state", "desktop"] {
        std::fs::create_dir_all(root.join(dir)).unwrap();
    }
    std::fs::write(root.join("dconf-profile"), "").unwrap();
    std::fs::write(
        root.join("config.json"),
        r#"{"setup_dismissed":true,"onboarded":true,"enabled_widgets":["clock","calendar","weather","devbox","projects"]}"#,
    )
    .unwrap();
    let result = std::process::Command::new(std::env::current_exe().unwrap())
        .args(["--exact", &format!("settings::widgets_ux_tests::{name}")])
        .env("DESKTOP_CARDS_WIDGETS_UX_TEST", name)
        .env("DESKTOP_CARDS_CONFIG", root.join("config.json"))
        .env("DESKTOP_CARDS_DESKTOP_ROOT", root.join("desktop"))
        .env("DESKTOP_CARDS_CACHE_DIR", root.join("cache"))
        .env("XDG_CONFIG_HOME", root.join("config"))
        .env("XDG_DATA_HOME", root.join("data"))
        .env("XDG_CACHE_HOME", root.join("cache"))
        .env("XDG_STATE_HOME", root.join("state"))
        .env("DCONF_PROFILE", root.join("dconf-profile"))
        .env_remove("DBUS_SESSION_BUS_ADDRESS")
        .output()
        .unwrap();
    std::fs::remove_dir_all(root).unwrap();
    assert!(
        result.status.success(),
        "{}\n{}",
        String::from_utf8_lossy(&result.stdout),
        String::from_utf8_lossy(&result.stderr)
    );
    true
}

const SIZE: Size = Size::new(1280., 800.);

fn open(page: Page) -> (Settings, Native) {
    let (mut app, task) = Settings::init(Core::default(), page);
    super::tests::settle_settings(&mut app, task);
    super::tests::send_settings(&mut app, Message::Resized(SIZE));
    (app, Native::new(SIZE))
}

/// The messages a click on `id` sends, after scrolling it into view.
fn press(app: &Settings, native: &mut Native, id: &str) -> Vec<Message> {
    let at = super::tests::native_reveal(app, native, id).center();
    let mut sent = Vec::new();
    for input in [
        mouse::Event::CursorMoved { position: at },
        mouse::Event::ButtonPressed(mouse::Button::Left),
        mouse::Event::ButtonReleased(mouse::Button::Left),
    ] {
        sent.extend(native.event(app.settings_view(), Event::Mouse(input), at).0);
    }
    sent
}

fn click(app: &mut Settings, native: &mut Native, id: &str) {
    for message in press(app, native, id) {
        let _ = app.update(message);
    }
}

/// Both shades at the audit's laptop size, when a capture folder is set.
fn capture(app: &Settings, native: &mut Native, name: &str) {
    for (theme, shade) in [
        (cosmic::Theme::light(), "light"),
        (cosmic::Theme::dark(), "dark"),
    ] {
        native.capture(app.settings_view(), &theme, &format!("{name}-{shade}"));
    }
}

/// WID-07: the Widgets page has a Look group whose rows say how widgets
/// look now and open the section that sets each: Glass and transparency
/// for widgets, the widget font with the other fonts, and the day and night
/// styles on Time of day. The Clock's Same as widgets opens the font too.
#[test]
fn wid07_the_widgets_page_links_to_how_widgets_look() {
    if child("wid07_the_widgets_page_links_to_how_widgets_look") {
        return;
    }
    let (mut app, mut native) = open(Page::Overview);
    let glass = match app
        .config
        .materials
        .effective(crate::materials::Target::Widgets)
    {
        Some(finish) => format!(
            "{}, {}% see-through",
            finish.kind.title(),
            100 - finish.opacity()
        ),
        None => app.finish_summary(None),
    };
    super::tests::native_reveal(&app, &mut native, "widgets-look");
    for (id, title, value) in [
        (
            "widgets-look-glass",
            "Glass and transparency",
            glass.as_str(),
        ),
        ("widgets-look-lens", "Widget glass", "Off"),
        ("widgets-look-font", "Widget font", "Match style"),
        ("widgets-look-styles", "Day and night styles", "Off"),
    ] {
        let row = native.bounds(app.settings_view(), id);
        let title = native.text_bounds(app.settings_view(), title);
        assert!(row.contains(title.center()), "{id}: {row:?} {title:?}");
        let value = native
            .text_all(app.settings_view(), value)
            .into_iter()
            .find(|text| row.contains(text.center()))
            .unwrap_or_else(|| panic!("{id} says {value}"));
        assert!(value.width > 0.);
    }
    // The group sits with the other widget settings, before behaviour.
    let look = native.bounds(app.settings_view(), "widgets-look");
    let behave = native.bounds(app.settings_view(), "widgets-behave");
    assert!(look.y + look.height <= behave.y, "{look:?} {behave:?}");
    capture(&app, &mut native, "wid07-widgets-look");
    // The font opens Fonts and icons at the widget font.
    click(&mut app, &mut native, "widgets-look-font");
    assert_eq!(app.page, Page::DesktopAppearance);
    assert_eq!(
        app.search_selected.map(|hit| hit.anchor()),
        Some("widget-font")
    );
    // Glass opens Appearance with widgets picked.
    super::tests::send_settings(&mut app, Message::Page(Page::Overview));
    click(&mut app, &mut native, "widgets-look-glass");
    assert_eq!(app.page, Page::Appearance);
    assert_eq!(app.material_target, crate::materials::Target::Widgets);
    // Widget glass opens its own editor, not the effects list.
    super::tests::send_settings(&mut app, Message::Page(Page::Overview));
    click(&mut app, &mut native, "widgets-look-lens");
    assert_eq!(app.page, Page::WeatherEffects);
    assert!(app.rain.glass_editor);
    // The styles open Time of day ready for widget styles.
    super::tests::send_settings(&mut app, Message::Page(Page::Overview));
    click(&mut app, &mut native, "widgets-look-styles");
    assert_eq!(app.page, Page::Moments.canonical());
    assert_eq!(app.automation.widget_mode, Some(true));
    // The row says which style shows once one does.
    app.config.moments.active = Some(crate::moments::Phase::Night);
    super::tests::send_settings(&mut app, Message::Page(Page::Overview));
    super::tests::native_reveal(&app, &mut native, "widgets-look");
    native.text_bounds(app.settings_view(), "Showing the night style");
    // The Clock's Same as widgets names the widget font and opens it.
    app.config.widget_font = Some("Inter".into());
    super::tests::send_settings(&mut app, Message::Page(Page::Clock));
    super::tests::native_reveal(&app, &mut native, "clock-widget-font");
    native.text_bounds(app.settings_view(), "Uses the widget font: Inter");
    click(&mut app, &mut native, "clock-widget-font");
    assert_eq!(app.page, Page::DesktopAppearance);
    assert_eq!(
        app.search_selected.map(|hit| hit.anchor()),
        Some("widget-font")
    );
}

/// WID-10: each part of Daily overview is a row that says what it shows or
/// still needs, as the Widgets list says it, and opens its page; the
/// preview says it is a sample.
#[test]
fn wid10_daily_overview_says_what_each_part_needs() {
    if child("wid10_daily_overview_says_what_each_part_needs") {
        return;
    }
    let (mut app, mut native) = open(Page::Glance);
    app.calendar.connection = crate::calendar::Connection::default();
    assert!(app.config.location.is_none());
    for (id, status) in [
        (
            "glance-open-clock",
            if app.config.clock.format_24h {
                "24-hour"
            } else {
                "12-hour"
            },
        ),
        ("glance-open-weather", "Choose a city"),
        ("glance-open-calendar", "Connect your calendar"),
    ] {
        let row = super::tests::native_reveal(&app, &mut native, id);
        let text = native
            .text_all(app.settings_view(), status)
            .into_iter()
            .find(|text| row.contains(text.center()))
            .unwrap_or_else(|| panic!("{id} says {status}"));
        assert!(text.width > 0.);
        // Its switch is on the same row.
        let toggle = native.bounds(
            app.settings_view(),
            &id.replace("glance-open-", "glance-toggle-"),
        );
        assert!((toggle.center().y - row.center().y).abs() < 20., "{id}");
    }
    // The preview's events and weather are made up, and it says so.
    let scene = native.bounds(app.settings_view(), "desktop-scene");
    let note = native.bounds(app.settings_view(), "glance-sample-note");
    native.text_bounds(app.settings_view(), "Sample");
    assert!(scene.y + scene.height <= note.y, "{scene:?} {note:?}");
    capture(&app, &mut native, "wid10-daily-overview");
    // A switch still switches the part.
    let sent = press(&app, &mut native, "glance-toggle-weather");
    assert!(
        sent.iter()
            .any(|m| matches!(m, Message::GlanceSection("show_weather", _))),
        "{sent:?}"
    );
    // Each part opens its own page.
    click(&mut app, &mut native, "glance-open-weather");
    assert_eq!(app.page, Page::Weather);
    super::tests::send_settings(&mut app, Message::Page(Page::Glance));
    click(&mut app, &mut native, "glance-open-calendar");
    assert_eq!(app.page, Page::Calendar);
    // Ready parts say what they show.
    app.config.location = Some(zagreb());
    app.calendar.connection.backend = crate::calendar::Backend::Subscription;
    app.calendar.connection.account = "Calendar links".into();
    super::tests::send_settings(&mut app, Message::Page(Page::Glance));
    for (id, status) in [
        ("glance-open-weather", "Zagreb"),
        ("glance-open-calendar", "Calendar links"),
    ] {
        let row = super::tests::native_reveal(&app, &mut native, id);
        assert!(
            native
                .text_all(app.settings_view(), status)
                .into_iter()
                .any(|text| row.contains(text.center())),
            "{id} says {status}"
        );
    }
}

fn zagreb() -> crate::config::Location {
    crate::config::Location {
        name: "Zagreb".into(),
        latitude: 45.8,
        longitude: 16.0,
        timezone: None,
    }
}

/// WID-11: every preview with made-up data says "Sample" right under it,
/// the same way on every page, and the Widgets picture too. Weather shows
/// the desktop's own reading for the chosen city once there is one, and
/// then says nothing of samples; so does Daily overview without events.
#[test]
fn wid11_previews_say_sample_and_weather_shows_the_desktops_reading() {
    if child("wid11_previews_say_sample_and_weather_shows_the_desktops_reading") {
        return;
    }
    let (mut app, mut native) = open(Page::Devbox);
    // Nothing playing and an empty note: the cards show examples.
    app.tools.feed = None;
    app.tools.note = Ok(crate::tools::notes::Note {
        text: String::new(),
        updated_at: None,
    });
    for (page, id) in [
        (Page::Devbox, "devbox-sample-note"),
        (Page::Projects, "projects-sample-note"),
        (Page::Music, "music-sample-note"),
        (Page::Notes, "notes-sample-note"),
        (Page::Devices, "devices-sample-note"),
        (Page::Calendar, "calendar-sample-note"),
        (Page::Weather, "weather-preview-note"),
        (Page::Glance, "glance-sample-note"),
        (Page::Overview, "home-sample-note"),
    ] {
        super::tests::send_settings(&mut app, Message::Page(page));
        if page == Page::Calendar {
            app.calendar.connection = crate::calendar::Connection::default();
        }
        let scene = native.bounds(app.settings_view(), "desktop-scene");
        let note = native.bounds(app.settings_view(), id);
        assert!(scene.y + scene.height <= note.y, "{id}: {scene:?} {note:?}");
        assert!(
            note.y - (scene.y + scene.height) < 40.,
            "{id} is right under it"
        );
        let said = native
            .all_text(app.settings_view())
            .into_iter()
            .find(|text| text.starts_with("Sample"))
            .unwrap_or_else(|| panic!("{id} says Sample"));
        let text = native.text_bounds(app.settings_view(), &said);
        assert!(note.contains(text.center()), "{id}: {said}");
        capture(&app, &mut native, &format!("wid11-{}", page.key()));
    }
    // A written note is the real thing.
    super::tests::send_settings(&mut app, Message::Page(Page::Notes));
    app.tools.note = Ok(crate::tools::notes::Note {
        text: "Call Sam".into(),
        updated_at: None,
    });
    assert!(
        native
            .try_bounds(app.settings_view(), "notes-sample-note")
            .is_none()
    );
    // The desktop saved a reading for the chosen city: Weather shows it.
    let city = zagreb();
    let today = chrono::Local::now().date_naive();
    let snapshot = crate::data::WeatherSnapshot {
        location: city.clone(),
        updated_at: chrono::Local::now().to_rfc3339(),
        weather: serde_json::from_value(serde_json::json!({
            "current": {
                "temperature_2m": 7.0, "apparent_temperature": 5.0, "weather_code": 61,
                "wind_speed_10m": 12.0, "relative_humidity_2m": 88.0, "is_day": 1
            },
            "daily": {
                "time": (0..3)
                    .map(|n| (today + chrono::Days::new(n)).format("%Y-%m-%d").to_string())
                    .collect::<Vec<_>>(),
                "temperature_2m_max": [9.0, 11.0, 12.0],
                "temperature_2m_min": [3.0, 4.0, 6.0],
                "weather_code": [61, 3, 1],
                "precipitation_probability_max": [80.0, 30.0, 10.0]
            }
        }))
        .unwrap(),
    };
    snapshot
        .save_at(&crate::data::WeatherSnapshot::path().unwrap())
        .unwrap();
    app.config.location = Some(city);
    super::tests::send_settings(&mut app, Message::Page(Page::Weather));
    assert!(
        native
            .try_bounds(app.settings_view(), "weather-preview-note")
            .is_none()
    );
    // The preview is the desktop's card with that reading.
    let reading = app
        .weather_reading
        .snapshot
        .as_ref()
        .expect("the desktop's reading");
    let card = crate::widget_presentation::WEATHER_SIZE;
    let shown = Native::new(Size::new(card[0], card[1]))
        .all_text(crate::app::settings_weather_reading_sized(
            &app.config, reading, crate::widget_presentation::WEATHER_SIZE,
        ));
    assert!(shown.iter().any(|text| text == "7°"), "{shown:?}");
    assert!(!shown.iter().any(|text| text == "21°"), "not the sample");
    capture(&app, &mut native, "wid11-weather-reading");
    // Daily overview with the clock and weather only: nothing made up.
    app.config.glance.initialized = true;
    app.config.glance.show_clock = true;
    app.config.glance.show_weather = true;
    app.config.glance.show_calendar = false;
    super::tests::send_settings(&mut app, Message::Page(Page::Glance));
    assert!(
        native
            .try_bounds(app.settings_view(), "glance-sample-note")
            .is_none()
    );
    // With events it is a sample again: those are always made up.
    app.config.glance.show_calendar = true;
    super::tests::send_settings(&mut app, Message::Page(Page::Weather));
    super::tests::send_settings(&mut app, Message::Page(Page::Glance));
    native.bounds(app.settings_view(), "glance-sample-note");
}

/// Background services watches what you choose: a service, a system
/// service, a container or a port, checked before it is saved and at most
/// six; each can be removed again (ADR 0014).
#[test]
fn background_services_watch_what_you_choose() {
    if child("background_services_watch_what_you_choose") {
        return;
    }
    use crate::config::Watched;
    let (mut app, mut native) = open(Page::Devbox);
    let send = |app: &mut Settings, message| super::tests::send_settings(app, message);
    send(&mut app, Message::WatchName("syncthing".into()));
    send(&mut app, Message::WatchAdd);
    send(&mut app, Message::WatchKind(3));
    send(&mut app, Message::WatchName("11434".into()));
    send(&mut app, Message::WatchAdd);
    assert_eq!(
        Config::load().devbox.watched,
        [
            Watched::UserUnit {
                name: "syncthing.service".into()
            },
            Watched::Port { port: 11434 }
        ]
    );
    // Not a port, a duplicate, and a name that could be an option: refused.
    for (kind, text) in [(3, "70000"), (3, "11434"), (2, "--privileged")] {
        send(&mut app, Message::WatchKind(kind));
        send(&mut app, Message::WatchName(text.into()));
        send(&mut app, Message::WatchAdd);
        assert!(app.watch_error.is_some(), "{text}");
    }
    assert_eq!(Config::load().devbox.watched.len(), 2);
    super::tests::native_reveal(&app, &mut native, "devbox-watched");
    let shown = native.all_text(app.settings_view());
    for text in ["Background services", "syncthing.service", "Your service", "11434", "Port"] {
        assert!(shown.iter().any(|t| t == text), "{text} in {shown:?}");
    }
    capture(&app, &mut native, "devbox-watched");
    send(&mut app, Message::WatchRemove(0));
    assert_eq!(Config::load().devbox.watched, [Watched::Port { port: 11434 }]);
}

#[test]
fn data_tiles_are_added_from_the_fields_a_file_holds() {
    if child("data_tiles_are_added_from_the_fields_a_file_holds") {
        return;
    }
    use crate::settings::tools::{Message as Edit, tile_editor::Message as Tile};
    use crate::tiles::Show;
    let dir = crate::test_env::TempPath::new("tiles-settings");
    let file = dir.root().join("nas.json");
    std::fs::write(
        &file,
        r#"{"disk": {"used_percent": 71.5, "health": "good"}, "uptime_days": 12}"#,
    )
    .unwrap();
    let (mut app, mut native) = open(Page::Tiles);
    let send = |app: &mut Settings, message| {
        super::tests::send_settings(app, Message::Tools(Edit::Tile(message)))
    };
    let shown = native.all_text(app.settings_view());
    assert!(shown.iter().any(|t| t == "Sample · add a tile below"), "{shown:?}");
    // The file is read once and its fields offered, with what each holds.
    send(&mut app, Tile::Source(file.display().to_string()));
    send(&mut app, Tile::ReadSource);
    super::tests::native_reveal(&app, &mut native, "tile-field-0");
    let shown = native.all_text(app.settings_view());
    for text in [
        "Fields it holds", "disk.used_percent", "71.5", "disk.health", "good", "uptime_days",
    ] {
        assert!(shown.iter().any(|t| t == text), "{text} in {shown:?}");
    }
    // A click picks a field and names the tile after it.
    click(&mut app, &mut native, "tile-field-0");
    assert_eq!(app.tools.tiles.draft.field, "disk.used_percent");
    assert_eq!(app.tools.tiles.draft.label, "Used percent");
    send(&mut app, Tile::Show(1));
    send(&mut app, Tile::Unit("%".into()));
    capture(&app, &mut native, "tiles-settings-form");
    click(&mut app, &mut native, "tile-add");
    let saved = Config::load().tiles.tiles;
    assert_eq!(saved.len(), 1);
    assert_eq!(saved[0].source, crate::tiles::Source::File { path: file.clone() });
    assert_eq!(saved[0].show, Show::Progress { max: 100. });
    assert_eq!((saved[0].unit.as_str(), saved[0].every), ("%", 60));
    // The preview reads the saved tile instead of the sample.
    let shown = native.all_text(app.settings_view());
    assert!(!shown.iter().any(|t| t == "Sample · add a tile below"));
    assert!(shown.iter().any(|t| t == "71.5"), "{shown:?}");
    let described = "Progress bar · nas.json · disk.used_percent · every 1 min";
    assert!(shown.iter().any(|t| t == described), "{shown:?}");
    // Refused: no name, a relative path and an empty range.
    send(&mut app, Tile::Template(0));
    send(&mut app, Tile::Label(String::new()));
    send(&mut app, Tile::Add);
    assert_eq!(app.tools.tiles.error.as_deref(), Some("Give the tile a name."));
    send(&mut app, Tile::Label("Health".into()));
    send(&mut app, Tile::Source("nas.json".into()));
    send(&mut app, Tile::Add);
    assert_eq!(
        app.tools.tiles.error.as_deref(),
        Some("Choose a file, or type its full path.")
    );
    send(&mut app, Tile::Source(file.display().to_string()));
    send(&mut app, Tile::Show(2));
    send(&mut app, Tile::Min("5".into()));
    send(&mut app, Tile::Max("5".into()));
    send(&mut app, Tile::Add);
    assert_eq!(
        app.tools.tiles.error.as_deref(),
        Some("The largest value must be above the smallest.")
    );
    send(&mut app, Tile::Show(0));
    send(&mut app, Tile::Field("disk.health".into()));
    send(&mut app, Tile::Every(300));
    send(&mut app, Tile::Add);
    assert_eq!(app.tools.tiles.error, None);
    assert_eq!(Config::load().tiles.tiles.len(), 2);
    super::tests::native_reveal(&app, &mut native, "tiles-list");
    capture(&app, &mut native, "tiles-settings-list");
    click(&mut app, &mut native, "tile-remove-0");
    let left = Config::load().tiles.tiles;
    assert_eq!(left.len(), 1);
    assert_eq!(left[0].field, "disk.health");
}

#[test]
fn data_tiles_read_a_web_address_on_your_network() {
    if child("data_tiles_read_a_web_address_on_your_network") {
        return;
    }
    use crate::settings::tools::{Message as Edit, tile_editor::Message as Tile};
    use std::io::{Read, Write};
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    std::thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(mut stream) = stream else { break };
            let _ = stream.read(&mut [0u8; 4096]);
            let body = r#"{"printer": {"state": "printing", "progress": 42}}"#;
            let _ = write!(
                stream,
                "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            );
        }
    });
    let address = format!("http://127.0.0.1:{port}/printer");
    let (mut app, mut native) = open(Page::Tiles);
    let send = |app: &mut Settings, message| {
        super::tests::send_settings(app, Message::Tools(Edit::Tile(message)))
    };
    send(&mut app, Tile::Source(address.clone()));
    // Read runs a request; its answer is settled like the app would.
    for message in press(&app, &mut native, "tile-read") {
        super::tests::send_settings(&mut app, message);
    }
    super::tests::native_reveal(&app, &mut native, "tile-field-0");
    let shown = native.all_text(app.settings_view());
    for text in ["printer.state", "printing", "printer.progress", "42"] {
        assert!(shown.iter().any(|t| t == text), "{text} in {shown:?}");
    }
    send(&mut app, Tile::Field("printer.progress".into()));
    send(&mut app, Tile::Show(1));
    send(&mut app, Tile::Every(10));
    send(&mut app, Tile::Add);
    assert_eq!(
        app.tools.tiles.error.as_deref(),
        Some("Web addresses are read every 30 s at most.")
    );
    send(&mut app, Tile::Every(30));
    send(&mut app, Tile::Add);
    let saved = Config::load().tiles.tiles;
    assert_eq!(saved.len(), 1);
    assert_eq!(saved[0].source, crate::tiles::Source::Http { url: address });
    let shown = native.all_text(app.settings_view());
    assert!(shown.iter().any(|t| t == "42"), "the preview reads it: {shown:?}");
    let described = "Progress bar · 127.0.0.1 · printer.progress · every 30 s";
    assert!(shown.iter().any(|t| t == described), "{shown:?}");
    super::tests::native_reveal(&app, &mut native, "tile-source");
    capture(&app, &mut native, "tiles-settings-web");
    // Plain http to a server off the local network is refused before asking.
    send(&mut app, Tile::Source("http://example.com/status.json".into()));
    send(&mut app, Tile::ReadSource);
    assert_eq!(
        app.tools.tiles.error.as_deref(),
        Some("Plain http works only on your own network. Use https.")
    );
}
