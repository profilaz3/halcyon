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
fn click<'a, M>(native: &mut Native, view: impl Fn() -> Element<'a, M>, point: Point) -> Vec<M> {
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
                "precipitation_probability_max": [10.0, 80.0, null, 40.0]},
            "hourly": {
                "time": (0..25)
                    .map(|n| (Local::now().naive_local() + chrono::Duration::hours(n))
                        .format("%Y-%m-%dT%H:00").to_string())
                    .collect::<Vec<_>>(),
                "temperature_2m": (0..25).map(|n| 10.0 + f64::from(n % 5)).collect::<Vec<_>>(),
                "weather_code": vec![3; 25],
                "precipitation_probability": (0..25).map(|n| f64::from(n * 4)).collect::<Vec<_>>(),
                "is_day": vec![1; 25]
            }
        }))
        .unwrap(),
    );
    let shown = native.all_text(cards.reading_view(Widget::Weather));
    for text in [
        "Zagreb",
        "Next hours",
        "Now",
        "92%",
        "Forecast",
        "Today",
        "Tomorrow",
        "Rain 80%",
        "Rain 40%",
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
            Some(Message::ReadingDetails(Widget::Folder)),
            None,
        )
    };
    let mut native = Native::new(Size::new(size[0], size[1]));
    let shown = native.all_text(card(None));
    let more = shown
        .iter()
        .find(|t| t.ends_with(" more →"))
        .unwrap_or_else(|| panic!("an overflow count in {shown:?}"));
    let visible: usize = more.trim_end_matches(" more →").parse().unwrap();
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
fn markets_alert_once_per_crossing_refresh_and_choose_a_range() {
    if isolated("markets_alert_once_per_crossing_refresh_and_choose_a_range") {
        return;
    }
    use crate::markets::{Alert, Crossed, Range};
    let mut cards = cards();
    cards.config.markets.symbols = vec!["AAPL".into(), "MSFT".into()];
    cards.config.markets.alerts.insert(
        "AAPL".into(),
        Alert {
            above: Some(200.),
            below: None,
        },
    );
    let quote = |price: f64| crate::markets::Quote {
        symbol: "AAPL".into(),
        name: "Apple".into(),
        currency: "USD".into(),
        price,
        change: None,
        percent: None,
        timestamp: 1,
        market_open: None,
    };
    let mut see = |cards: &mut Cards, price| {
        cards.tools.markets.quotes = vec![quote(price)];
        let _ = cards.market_alerts();
        cards.tools.market_alerted.get("AAPL").copied()
    };
    assert_eq!(see(&mut cards, 190.), None);
    assert_eq!(see(&mut cards, 201.), Some(Crossed::Above));
    // Still past it: no second notice is due; back inside re-arms it.
    assert_eq!(see(&mut cards, 205.), Some(Crossed::Above));
    assert_eq!(see(&mut cards, 199.), None);
    assert_eq!(see(&mut cards, 202.), Some(Crossed::Above));

    // Refresh marks the reading old and restarts the feed.
    cards.tools.markets.received_at = Some(Instant::now());
    let before = cards.tools.markets_refresh;
    let _ = cards.update_tool(crate::tools::cards::Action::MarketsRefresh);
    assert!(cards.tools.markets.received_at.is_none());
    assert_eq!(cards.tools.markets_refresh, before + 1);
    assert!(cards.can_refresh(Widget::Markets));

    // The card's own controls: refresh, the ranges and the alert's mark.
    let mut config = cards.config.clone();
    config.markets.show_chart = true;
    config.markets.chart_range = Range::Month;
    config.markets.selected = "AAPL".into();
    let mut state = crate::tools::cards::sample_markets_for(&config.markets);
    if let Some(q) = state.quotes.iter_mut().find(|q| q.symbol == "AAPL") {
        q.price = 250.;
    }
    let dims = [360., 420.];
    let card = || {
        crate::tools::cards::markets::<crate::tools::cards::Action>(
            &state,
            &config,
            Some(|a| a),
            Some(dims),
        )
    };
    let mut native = Native::new(Size::new(dims[0], dims[1]));
    let shown = native.all_text(card());
    assert!(shown.iter().any(|t| t.starts_with("▲ ")), "{shown:?}");
    for label in ["1W", "1M", "3M"] {
        assert!(shown.iter().any(|t| t == label), "{label} in {shown:?}");
    }
    let at = native.bounds(card(), "markets-range-week").center();
    assert!(
        click(&mut native, card, at)
            .iter()
            .any(|a| matches!(a, crate::tools::cards::Action::MarketRange(Range::Week)))
    );
    let at = native.bounds(card(), "markets-refresh").center();
    assert!(
        click(&mut native, card, at)
            .iter()
            .any(|a| matches!(a, crate::tools::cards::Action::MarketsRefresh))
    );
    native.capture(card(), &cosmic::Theme::dark(), "markets-card-ranges");
}

#[test]
fn the_folder_reading_lists_everything_and_acts_on_each_entry() {
    if isolated("the_folder_reading_lists_everything_and_acts_on_each_entry") {
        return;
    }
    let root = crate::test_env::TempPath::new("folder-reading");
    std::fs::create_dir_all(root.join("Sketches/Old")).unwrap();
    for (i, name) in ["notes.txt", "plan.md", "photo.png"].iter().enumerate() {
        std::fs::write(root.join(name), vec![b'x'; 2048 * (i + 1)]).unwrap();
    }
    let mut cards = cards();
    assert!(!cards.has_reading(Widget::Folder), "no folder chosen yet");
    cards.config.folder.path = Some(root.to_path_buf());
    assert!(cards.has_reading(Widget::Folder));
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    let listing = runtime.block_on(crate::folder::read_all(root.to_path_buf(), false));
    assert_eq!(listing.count, 4);
    assert_eq!(listing.entries.len(), 4);
    let _ = cards.update(Message::FolderReading(listing));
    let mut native = Native::new(Size::new(520., 480.));
    let shown = native.all_text(cards.reading_view(Widget::Folder));
    for text in [
        "4 items",
        "Sketches",
        "notes.txt",
        "Open in Files",
        "Terminal here",
        "Copy path",
    ] {
        assert!(shown.iter().any(|t| t == text), "{text} in {shown:?}");
    }
    assert!(
        shown.iter().any(|t| t.starts_with("6.0 KB · ")),
        "{shown:?}"
    );
    native.capture(
        cards.reading_view(Widget::Folder),
        &cosmic::Theme::dark(),
        "reading-folder",
    );
    // A listing for another folder never shows here.
    let stale = runtime.block_on(crate::folder::read_all(root.join("Sketches"), false));
    let _ = cards.update(Message::FolderReading(stale));
    assert_eq!(cards.tools.folder_reading.as_ref().unwrap().count, 4);
    // Into a subfolder, the trail leads back.
    let _ = cards.update(Message::FolderBrowse(Some(root.join("Sketches"))));
    let inner = runtime.block_on(crate::folder::read_all(root.join("Sketches"), false));
    let _ = cards.update(Message::FolderReading(inner));
    let shown = native.all_text(cards.reading_view(Widget::Folder));
    assert!(shown.iter().any(|t| t == "›"), "{shown:?}");
    assert!(shown.iter().any(|t| t == "Old") && shown.iter().any(|t| t == "1 item"));
    // A terminal opens in the folder: COSMIC's with its working directory,
    // else the session's default terminal, else it says none is installed.
    let bin = crate::test_env::TempPath::new("folder-terminal");
    let fake = |name: &str| {
        use std::os::unix::fs::PermissionsExt;
        let path = bin.join(name);
        std::fs::write(&path, "#!/bin/sh\n").unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
        path
    };
    // SAFETY: an isolated test child, before any thread reads PATH.
    unsafe { std::env::set_var("PATH", bin.root()) };
    let dir = root.join("Sketches");
    assert!(super::tools::terminal_command(&dir).is_err());
    let default = fake("x-terminal-emulator");
    assert_eq!(
        super::tools::terminal_command(&dir).unwrap(),
        (default, vec![])
    );
    let cosmic = fake("cosmic-term");
    assert_eq!(
        super::tools::terminal_command(&dir).unwrap(),
        (
            cosmic,
            vec!["-w".to_string(), dir.to_string_lossy().into_owned()]
        )
    );
}

#[test]
fn file_sizes_read_like_a_file_manager() {
    use super::widget_details::size_text;
    assert_eq!(size_text(512), "512 B");
    assert_eq!(size_text(3_500), "3.4 KB");
    assert_eq!(size_text(12 * 1024 * 1024 + 1), "12 MB");
    assert_eq!(size_text(5 * 1024 * 1024 * 1024), "5.0 GB");
}
