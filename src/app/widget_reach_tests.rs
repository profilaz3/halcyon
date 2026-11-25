//! Reaching more of a widget from the desktop: full readings for Devices and
//! Weather, the Folder card's overflow and subfolders, Refresh now in a
//! card's right-click menu, and Peek's arrow keys.
use super::*;
use crate::native_test::Native;
use cosmic::Application;
use cosmic::iced::keyboard::{Key, Modifiers, key::Named};
use cosmic::iced::{Event, Point, Size, mouse};

fn isolated(name: &str) -> bool {
    if std::env::var("HALCYON_WIDGET_REACH_TEST").as_deref() == Ok(name) {
        return false;
    }
    let root = crate::test_env::TempPath::new("widget-reach");
    for dir in ["config", "data", "cache", "state", "runtime"] {
        std::fs::create_dir_all(root.join(dir)).unwrap();
    }
    std::fs::write(
        root.join("config.json"),
        r#"{"enabled_widgets":["weather","calendar","projects"],"devices":{"enabled":true}}"#,
    )
    .unwrap();
    let status = std::process::Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            &format!("app::widget_reach_tests::{name}"),
            "--nocapture",
        ])
        .env("HALCYON_WIDGET_REACH_TEST", name)
        .env("DESKTOP_CARDS_CONFIG", root.join("config.json"))
        .env("DESKTOP_CARDS_DESKTOP_ROOT", root.root())
        .env("DESKTOP_CARDS_CACHE_DIR", root.join("cache"))
        .env("XDG_CONFIG_HOME", root.join("config"))
        .env("XDG_DATA_HOME", root.join("data"))
        .env("XDG_CACHE_HOME", root.join("cache"))
        .env("XDG_STATE_HOME", root.join("state"))
        .env("XDG_RUNTIME_DIR", root.join("runtime"))
        .env_remove("WAYLAND_DISPLAY")
        .env_remove("DISPLAY")
        .env_remove("DBUS_SESSION_BUS_ADDRESS")
        .status()
        .unwrap();
    assert!(status.success(), "fixture {}", root.display());
    true
}

fn cards() -> Cards {
    Cards::init(Core::default(), Config::load()).0
}

/// Presses and releases at `point`, as a click.
fn click<M>(native: &mut Native, view: impl Fn() -> Element<'static, M>, point: Point) -> Vec<M> {
    let mut messages = Vec::new();
    for input in [
        mouse::Event::CursorMoved { position: point },
        mouse::Event::ButtonPressed(mouse::Button::Left),
        mouse::Event::ButtonReleased(mouse::Button::Left),
    ] {
        messages.extend(native.event(view(), Event::Mouse(input), point).0);
    }
    messages
}

fn device(
    name: &str,
    kind: crate::devices::Kind,
    battery: Option<u8>,
    charging: bool,
) -> crate::devices::Device {
    crate::devices::Device {
        id: format!("bt:{name}"),
        name: name.into(),
        kind,
        battery,
        charging: Some(charging),
        ..Default::default()
    }
}

fn devices() -> Vec<crate::devices::Device> {
    use crate::devices::Kind;
    vec![
        device("Sony WH-1000XM5", Kind::Headphones, Some(80), true),
        device("MX Anywhere 3", Kind::Mouse, Some(15), false),
        device("Pixel 8", Kind::Phone, Some(64), false),
        device("Keychron K3", Kind::Keyboard, None, false),
        device("Xbox Controller", Kind::Gamepad, Some(41), false),
    ]
}

#[test]
fn devices_and_weather_open_a_full_reading() {
    if isolated("devices_and_weather_open_a_full_reading") {
        return;
    }
    let mut cards = cards();
    assert!(cards.has_reading(Widget::Devices) && cards.has_reading(Widget::Weather));
    assert!(
        !cards.has_reading(Widget::Clock),
        "a clock has nothing more to read"
    );

    // Every device, with what its battery is doing.
    cards.tools.devices = devices();
    let mut native = Native::new(Size::new(520., 480.));
    let shown = native.all_text(cards.reading_view(Widget::Devices));
    for name in [
        "Sony WH-1000XM5",
        "MX Anywhere 3",
        "Pixel 8",
        "Keychron K3",
        "Xbox Controller",
    ] {
        assert!(shown.iter().any(|t| t == name), "{name} in {shown:?}");
    }
    for state in [
        "Charging",
        "Battery low",
        "Battery not reported",
        "15%",
        "Controller",
    ] {
        assert!(shown.iter().any(|t| t == state), "{state} in {shown:?}");
    }
    native.capture(
        cards.reading_view(Widget::Devices),
        &cosmic::Theme::dark(),
        "reading-devices",
    );
    cards.tools.devices.clear();
    let empty = native.all_text(cards.reading_view(Widget::Devices));
    assert!(
        empty.iter().any(|t| t == "No devices connected"),
        "{empty:?}"
    );

    // The Devices card's "N more →" opens that reading.
    let list = devices();
    let config = cards.config.clone();
    let card = || {
        crate::tools::cards::devices::<Message>(
            &list,
            &config,
            Some(Message::ReadingDetails(Widget::Devices)),
            Some([260., 140.]),
        )
    };
    let mut small = Native::new(Size::new(260., 140.));
    let more = small
        .all_text(card())
        .into_iter()
        .find(|t| t.ends_with(" more →"))
        .expect("the card says how many more");
    let at = small.text_bounds(card(), &more).center();
    let pressed = click(&mut small, card, at);
    assert!(
        pressed
            .iter()
            .any(|m| matches!(m, Message::ReadingDetails(Widget::Devices))),
        "{pressed:?}"
    );

    // Weather: every day the card cuts for room, with conditions and rain.
    cards.config.location = Some(crate::config::Location {
        name: "Zagreb".into(),
        latitude: 45.8,
        longitude: 16.0,
        timezone: None,
    });
    let today = Local::now().date_naive();
    let days: Vec<String> = (0..4)
        .map(|n| (today + Days::days(n)).format("%Y-%m-%d").to_string())
        .collect();
    cards.weather = Some(
        serde_json::from_value(serde_json::json!({
            "current": {"temperature_2m": 12.0, "apparent_temperature": 10.0, "weather_code": 3,
                "wind_speed_10m": 14.0, "relative_humidity_2m": 71.0, "is_day": 1},
            "daily": {"time": days, "temperature_2m_max": [14.0, 16.0, 11.0, 9.0],
                "temperature_2m_min": [6.0, 7.0, 4.0, 2.0], "weather_code": [3, 61, 2, 71],
                "precipitation_probability_max": [10.0, 80.0, null, 40.0]}
        }))
        .unwrap(),
    );
    let shown = native.all_text(cards.reading_view(Widget::Weather));
    for text in [
        "Zagreb", "Forecast", "Today", "Tomorrow", "Rain 80%", "Rain 40%",
    ] {
        assert!(shown.iter().any(|t| t == text), "{text} in {shown:?}");
    }
    native.capture(
        cards.reading_view(Widget::Weather),
        &cosmic::Theme::dark(),
        "reading-weather",
    );
    let fourth = (today + Days::days(3)).format("%a %-d").to_string();
    assert!(shown.contains(&fourth), "{fourth} in {shown:?}");
    assert!(shown.iter().any(|t| t.starts_with("Feels ")), "{shown:?}");
    cards.config.location = None;
    cards.weather = None;
    let choose = native.all_text(cards.reading_view(Widget::Weather));
    assert!(
        choose.iter().any(|t| t.starts_with("Choose a city")),
        "{choose:?}"
    );
}

#[test]
fn folder_card_counts_what_it_leaves_out_and_opens_subfolders_inside() {
    let mut listing = crate::folder::sample();
    listing.count = 11;
    let mut config = Config::default();
    config.folder.path = Some(listing.source.clone());
    let size = [310., 214.];
    let sketches = listing.source.join("Sketches");
    let card = |back: Option<Message>| {
        crate::tools::cards::folder_in(
            &listing,
            &config,
            size,
            None,
            |p| Some(Message::Open(format!("file://{}", p.display()))),
            |p| Some(Message::FolderBrowse(Some(p.to_path_buf()))),
            back,
            None,
        )
    };
    let mut native = Native::new(Size::new(size[0], size[1]));
    let shown = native.all_text(card(None));
    let more = shown
        .iter()
        .find(|t| t.ends_with(" more"))
        .unwrap_or_else(|| panic!("an overflow count in {shown:?}"));
    let visible: usize = more.trim_end_matches(" more").parse().unwrap();
    let (cols, rows) = crate::folder::layout(size);
    assert_eq!(visible, 11 - listing.entries.len().min(cols * rows));
    native.capture(
        card(Some(Message::FolderBrowse(None))),
        &cosmic::Theme::dark(),
        "folder-card-browsing",
    );
    // A subfolder opens in the card; a file opens in its app.
    let at = native.text_bounds(card(None), "Sketches").center();
    let pressed = click(&mut native, || card(None), at);
    assert!(
        pressed
            .iter()
            .any(|m| matches!(m, Message::FolderBrowse(Some(p)) if *p == sketches)),
        "{pressed:?}"
    );
    let at = native.text_bounds(card(None), "Field notes.pdf").center();
    let pressed = click(&mut native, || card(None), at);
    assert!(
        pressed.iter().any(|m| matches!(m, Message::Open(_))),
        "{pressed:?}"
    );
    // Inside a subfolder, Back is offered and goes up.
    let back = || card(Some(Message::FolderBrowse(None)));
    let at = native.bounds(back(), "folder-back").center();
    let pressed = click(&mut native, back, at);
    assert!(
        pressed
            .iter()
            .any(|m| matches!(m, Message::FolderBrowse(None))),
        "{pressed:?}"
    );
}

#[test]
fn folder_browsing_stays_inside_the_chosen_folder() {
    if isolated("folder_browsing_stays_inside_the_chosen_folder") {
        return;
    }
    let root = crate::test_env::TempPath::new("folder-browse");
    let inner = root.join("Sketches");
    std::fs::create_dir_all(inner.join("Old")).unwrap();
    let mut cards = cards();
    cards.config.folder.path = Some(root.to_path_buf());
    assert_eq!(cards.folder_shown().as_deref(), Some(&*root));
    let _ = cards.update(Message::FolderBrowse(Some(inner.join("Old"))));
    assert_eq!(cards.folder_shown(), Some(inner.join("Old")));
    // Anything outside the chosen folder, or not a folder, is refused.
    for outside in [
        std::env::temp_dir(),
        root.join("missing"),
        root.to_path_buf(),
    ] {
        let _ = cards.update(Message::FolderBrowse(Some(outside.clone())));
        assert_eq!(
            cards.folder_shown().as_deref(),
            Some(&*root),
            "{}",
            outside.display()
        );
    }
    let _ = cards.update(Message::FolderBrowse(Some(inner.clone())));
    let _ = cards.update(Message::FolderBrowse(None));
    assert_eq!(cards.folder_shown().as_deref(), Some(&*root));
    // Choosing another folder shows that one, not the old subfolder.
    let _ = cards.update(Message::FolderBrowse(Some(inner.clone())));
    let other = crate::test_env::TempPath::new("folder-other");
    cards.config.folder.path = Some(other.to_path_buf());
    assert_eq!(cards.folder_shown().as_deref(), Some(&*other));
}

#[test]
fn the_widget_menu_refreshes_what_has_a_source() {
    if isolated("the_widget_menu_refreshes_what_has_a_source") {
        return;
    }
    let mut cards = cards();
    assert!(cards.can_refresh(Widget::Calendar) && cards.can_refresh(Widget::Devbox));
    assert!(!cards.can_refresh(Widget::Weather), "no city yet");
    assert!(!cards.can_refresh(Widget::Clock) && !cards.can_refresh(Widget::Notes));
    let _ = cards.update(Message::WidgetMenu(Widget::Calendar));
    let (id, _) = cards.menu.expect("menu opened");
    let mut native = Native::new(Size::new(layout::MENU_WIDTH as f32, 200.));
    let shown = native.all_text(cards.view_window(id));
    assert!(shown.iter().any(|t| t == "Refresh now"), "{shown:?}");
    native.capture(
        cards.view_window(id),
        &cosmic::Theme::dark(),
        "widget-menu-refresh",
    );
    // With one display there is nowhere to move it.
    assert!(
        !shown.iter().any(|t| t.starts_with("Move to ")),
        "{shown:?}"
    );
    cards.next_calendar = Instant::now() + Duration::from_secs(3600);
    cards.next_today_calendar = Instant::now() + Duration::from_secs(3600);
    let before = Instant::now();
    let _ = cards.update(Message::WidgetMenuPick(layout::MenuPick::Refresh));
    assert!(cards.menu.is_none());
    assert!(cards.next_calendar <= Instant::now() && cards.next_calendar >= before);
    assert!(cards.next_today_calendar <= Instant::now());
}

#[test]
fn peek_pages_with_the_arrow_keys() {
    if isolated("peek_pages_with_the_arrow_keys") {
        return;
    }
    let mut cards = cards();
    let widgets = vec![
        Widget::Calendar,
        Widget::Weather,
        Widget::Projects,
        Widget::Devices,
        Widget::Devbox,
        Widget::Clock,
    ];
    let _ = cards.update(Message::Peek(Some(("Desk check".into(), widgets))));
    let peek = cards.peek.as_mut().unwrap();
    // A narrow output, so the widgets need more than one page.
    peek.size = [420., 600.];
    let id = peek.id;
    let key = |cards: &mut Cards, named| {
        let _ = cards.update(Message::KeyInput(id, Key::Named(named), Modifiers::empty()));
        cards.peek.as_ref().unwrap().page
    };
    let next = cards
        .peek_next(cards.peek.as_ref().unwrap())
        .expect("a second page");
    assert_eq!(
        key(&mut cards, Named::ArrowLeft),
        0,
        "nothing before the first page"
    );
    assert_eq!(key(&mut cards, Named::ArrowRight), next);
    assert_eq!(key(&mut cards, Named::ArrowLeft), 0);
    assert_eq!(key(&mut cards, Named::PageDown), next);
    let mut last = next;
    while let Some(after) = cards.peek_next(cards.peek.as_ref().unwrap()) {
        assert_eq!(key(&mut cards, Named::ArrowRight), after);
        last = after;
    }
    assert_eq!(
        key(&mut cards, Named::ArrowRight),
        last,
        "nothing after the last page"
    );
    let _ = cards.update(Message::KeyInput(
        id,
        Key::Named(Named::Escape),
        Modifiers::empty(),
    ));
    assert!(cards.peek.is_none());
}

#[test]
fn data_tiles_show_what_their_files_hold_and_say_when_it_is_old_or_missing() {
    use crate::tiles::{Show, Source, State, Tile};
    let dir = crate::test_env::TempPath::new("tiles-card");
    let status = dir.root().join("status.json");
    std::fs::write(&status, r#"{"status": "up", "queue": 3, "percent": "40"}"#).unwrap();
    std::fs::write(dir.root().join("broken.json"), "{status: up}").unwrap();
    let tile = |label: &str, file: &str, field: &str, show| Tile {
        label: label.into(),
        source: Source::File {
            path: dir.root().join(file),
        },
        field: field.into(),
        show,
        unit: if field == "percent" {
            "%".into()
        } else {
            String::new()
        },
        every: 60,
    };
    let tiles: &'static [Tile] = Box::leak(Box::new([
        tile("Server", "status.json", "status", Show::Text),
        tile(
            "Download",
            "status.json",
            "percent",
            Show::Progress { max: 100. },
        ),
        tile("Queue", "status.json", "queue", Show::Graph),
        tile("Backup", "backup.json", "", Show::Text),
        tile(
            "Disk",
            "broken.json",
            "used",
            Show::Gauge { min: 0., max: 100. },
        ),
    ]));
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    let read = |previous: &State| {
        runtime.block_on(crate::tiles::read_all(tiles.to_vec(), previous.clone()))
    };
    let config = Config::default();
    let size = crate::tools::cards::size(Widget::Tiles, &{
        let mut c = Config::default();
        c.tiles.tiles = tiles.to_vec();
        c
    });
    let mut native = Native::new(Size::new(size[0], size[1]));
    let shown = |native: &mut Native, state: &State| {
        let state: &'static State = Box::leak(Box::new(state.clone()));
        native.all_text(crate::tools::cards::tiles::<Message>(
            state,
            tiles,
            &config,
            Some(size),
            None,
        ))
    };
    let first = read(&State::default());
    let text = shown(&mut native, &first);
    for want in [
        "Data tiles",
        "Server",
        "up",
        "Download",
        "40",
        "%",
        "Queue",
        "3",
        "Backup",
        "File not found",
        "Disk",
        "Not valid JSON (line 1)",
    ] {
        assert!(text.iter().any(|t| t == want), "{want} in {text:?}");
    }
    let state: &'static State = Box::leak(Box::new(first.clone()));
    native.capture(
        crate::tools::cards::tiles::<Message>(state, tiles, &config, Some(size), None),
        &cosmic::Theme::dark(),
        "tiles-card",
    );
    // A file nobody has written for two hours says so; its value stays.
    std::fs::File::options()
        .write(true)
        .open(&status)
        .unwrap()
        .set_modified(std::time::SystemTime::now() - std::time::Duration::from_secs(7200))
        .unwrap();
    let old = read(&first);
    let text = shown(&mut native, &old);
    assert!(text.iter().any(|t| t == "Unchanged for 2 h"), "{text:?}");
    assert!(text.iter().any(|t| t == "up"), "{text:?}");
    // A read that fails after a good one keeps the value and says why.
    std::fs::remove_file(&status).unwrap();
    let gone = read(&old);
    let text = shown(&mut native, &gone);
    assert!(text.iter().any(|t| t == "up"), "{text:?}");
    assert!(
        text.iter()
            .any(|t| t == "File not found · under a minute old"),
        "{text:?}"
    );
    assert_eq!(
        gone.readings[2].history.len(),
        2,
        "the graph kept both readings"
    );
    // With no tiles, the card's button opens its Settings page.
    let empty = || {
        crate::tools::cards::tiles(
            Box::leak(Box::default()),
            &[],
            &config,
            None,
            Some(Message::SettingsPage("tiles")),
        )
    };
    let [w, h] = crate::tools::cards::size(Widget::Tiles, &config);
    let mut native = Native::new(Size::new(w, h));
    let at = native.text_bounds(empty(), "Add a tile").center();
    let pressed = click(&mut native, empty, at);
    assert!(
        pressed
            .iter()
            .any(|m| matches!(m, Message::SettingsPage("tiles"))),
        "{pressed:?}"
    );
}
