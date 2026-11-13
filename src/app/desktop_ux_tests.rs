//! The desktop's menus, notices and keyboard: the edge menu (AX-04, FB-04,
//! FB-08, DT-03), a card's right-click menu (DT-02) and the idle Music card
//! (DT-14).
use super::*;
use crate::native_test::Native;
use cosmic::Application;
use cosmic::iced::Size;
use cosmic::iced::keyboard::{Key, Modifiers, key::Named};

fn isolated(name: &str) -> bool {
    if std::env::var("HALCYON_DESKTOP_UX_TEST").as_deref() == Ok(name) {
        return false;
    }
    let root =
        std::env::temp_dir().join(format!("halcyon-desktop-ux-{}-{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    for dir in ["config", "data", "cache", "state", "runtime"] {
        std::fs::create_dir_all(root.join(dir)).unwrap();
    }
    std::fs::write(
        root.join("config.json"),
        r#"{"enabled_widgets":["weather","devbox","projects"],"music":{"enabled":true}}"#,
    )
    .unwrap();
    let status = std::process::Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            &format!("app::desktop_ux_tests::{name}"),
            "--nocapture",
        ])
        .env("HALCYON_DESKTOP_UX_TEST", name)
        .env("DESKTOP_CARDS_CONFIG", root.join("config.json"))
        .env("DESKTOP_CARDS_DESKTOP_ROOT", &root)
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
    std::fs::remove_dir_all(root).unwrap();
    true
}

fn cards() -> Cards {
    let (cards, _) = Cards::init(Core::default(), Config::load());
    cards
}

fn rail() -> Native {
    Native::new(Size::new(
        drawer::SIDEBAR_WIDTH as f32,
        drawer::SIDEBAR_HEIGHT as f32,
    ))
}

/// The edge menu in both themes, when a capture folder is set.
fn capture_rail(cards: &Cards, name: &str) {
    for (theme, shade) in [
        (cosmic::Theme::light(), "light"),
        (cosmic::Theme::dark(), "dark"),
    ] {
        rail().capture(cards.control_rail(), &theme, &format!("{name}-{shade}"));
    }
}

fn press(cards: &mut Cards, key: Key, modifiers: Modifiers) {
    let _ = cards.update(Message::KeyInput(cards.ids[2], key, modifiers));
}

#[test]
fn ax04_keys_map_to_drawer_moves() {
    use drawer::DrawerKey;
    let none = Modifiers::empty();
    for (key, modifiers, expected) in [
        (Key::Named(Named::Escape), none, DrawerKey::Close),
        (Key::Named(Named::ArrowDown), none, DrawerKey::Next),
        (Key::Named(Named::Tab), none, DrawerKey::Next),
        (Key::Named(Named::ArrowUp), none, DrawerKey::Previous),
        (
            Key::Named(Named::Tab),
            Modifiers::SHIFT,
            DrawerKey::Previous,
        ),
        (
            Key::Named(Named::ArrowUp),
            Modifiers::ALT,
            DrawerKey::MoveUp,
        ),
        (
            Key::Named(Named::ArrowDown),
            Modifiers::ALT,
            DrawerKey::MoveDown,
        ),
        (Key::Named(Named::Enter), none, DrawerKey::Activate),
        (Key::Character(" ".into()), none, DrawerKey::Activate),
    ] {
        assert_eq!(
            DrawerKey::from_key(&key, modifiers),
            Some(expected),
            "{key:?}"
        );
    }
    assert_eq!(DrawerKey::from_key(&Key::Character("a".into()), none), None);
}

#[test]
fn ax04_toggle_opens_the_edge_menu_for_the_keyboard_and_moves_through_it() {
    if isolated("ax04_toggle_opens_the_edge_menu_for_the_keyboard_and_moves_through_it") {
        return;
    }
    let mut cards = cards();
    let items = cards.drawer_items();
    // "Customize this menu" closes the list, under Arrange and Settings.
    assert_eq!(items.last(), Some(&drawer::DrawerItem::Customize));
    assert_eq!(items[items.len() - 2], drawer::DrawerItem::Settings);
    assert_eq!(items[items.len() - 3], drawer::DrawerItem::Arrange);
    let _ = cards.update(Message::DrawerToggle);
    assert!(cards.sidebar.visible);
    assert_eq!(cards.sidebar.focus, Some(0));
    capture_rail(&cards, "ax04-drawer-focus");
    // Up from the top wraps to Customize; down wraps back.
    press(&mut cards, Key::Named(Named::ArrowUp), Modifiers::empty());
    assert_eq!(cards.sidebar.focus, Some(items.len() - 1));
    press(&mut cards, Key::Named(Named::Tab), Modifiers::empty());
    assert_eq!(cards.sidebar.focus, Some(0));
    // Alt+Down moves the first card down, saves it, and keeps the keyboard on it.
    let order = cards.config.ordered_right_cards();
    let first = order[0];
    press(&mut cards, Key::Named(Named::ArrowDown), Modifiers::ALT);
    assert_eq!(cards.config.ordered_right_cards()[1], first);
    assert_eq!(Config::load().ordered_right_cards()[1], first);
    assert_eq!(cards.sidebar.focus, Some(1));
    // Enter turns the focused widget off and saves it.
    assert!(cards.config.widget_visible(first));
    press(&mut cards, Key::Named(Named::Enter), Modifiers::empty());
    assert!(!cards.config.widget_visible(first));
    assert!(!Config::load().widget_visible(first));
    // Escape closes, and a second toggle closes an open menu.
    press(&mut cards, Key::Named(Named::Escape), Modifiers::empty());
    assert!(!cards.sidebar.visible);
    assert_eq!(cards.sidebar.focus, None);
    let _ = cards.update(Message::DrawerToggle);
    let _ = cards.update(Message::DrawerToggle);
    assert!(!cards.sidebar.visible);
}

#[test]
fn dt03_edge_menu_offers_arrange_and_settings_side_by_side() {
    if isolated("dt03_edge_menu_offers_arrange_and_settings_side_by_side") {
        return;
    }
    let mut cards = cards();
    cards.sidebar.show(Instant::now());
    let mut native = rail();
    let arrange = native.text_bounds(cards.control_rail(), "Arrange");
    let settings = native.text_bounds(cards.control_rail(), "Settings");
    assert!(
        (arrange.center_y() - settings.center_y()).abs() < 1.,
        "one row"
    );
    assert!(arrange.x < settings.x);
    assert!(settings.x + settings.width <= drawer::SIDEBAR_WIDTH as f32);
    capture_rail(&cards, "dt03-drawer");
}

#[test]
fn fb04_a_failed_save_explains_itself_and_try_again_finishes_it() {
    if isolated("fb04_a_failed_save_explains_itself_and_try_again_finishes_it") {
        return;
    }
    let mut cards = cards();
    let path = Config::path().unwrap();
    let saved = std::fs::read(&path).unwrap();
    std::fs::write(&path, "{broken").unwrap();
    let _ = cards.update(Message::SetWidgetVisible(Widget::Music, false));
    let notice = cards.notice.as_ref().expect("the failure is shown");
    assert_eq!(notice.title, "Couldn’t save the widget list");
    assert!(cards.sidebar.visible, "the edge menu opens to show it");
    let mut native = rail();
    native.text_bounds(cards.control_rail(), "Couldn’t save the widget list");
    native.text_bounds(cards.control_rail(), "Your change wasn’t kept.");
    native.bounds(cards.control_rail(), "notice-retry");
    native.bounds(cards.control_rail(), "notice-dismiss");
    let error = notice.error.clone();
    assert!(
        native.text_all(cards.control_rail(), &error).is_empty(),
        "details start hidden"
    );
    capture_rail(&cards, "fb04-notice");
    let _ = cards.update(Message::NoticeDetails);
    assert!(!native.text_all(cards.control_rail(), &error).is_empty());
    capture_rail(&cards, "fb04-notice-details");
    // Try again after the file is readable again.
    std::fs::write(&path, &saved).unwrap();
    let _ = cards.update(Message::NoticeRetry);
    assert!(cards.notice.is_none());
    assert!(!Config::load().widget_visible(Widget::Music));
    // Dismiss just clears it.
    cards.raise_notice("Title", "Body", "error".into(), None);
    assert!(
        native
            .try_bounds(cards.control_rail(), "notice-retry")
            .is_none()
    );
    let _ = cards.update(Message::NoticeDismiss);
    assert!(cards.notice.is_none());
}

#[test]
fn fb08_unsaved_arrangement_offers_plain_choices_and_hides_the_error() {
    if isolated("fb08_unsaved_arrangement_offers_plain_choices_and_hides_the_error") {
        return;
    }
    let mut cards = cards();
    let mut editor = layout::LayoutEditor::default();
    editor.exit_failed = true;
    cards.paused_layout = Some(editor);
    cards.sidebar.show(Instant::now());
    let mut native = rail();
    native.text_bounds(cards.control_rail(), "Your arrangement wasn’t saved");
    for text in [
        "Keep arranging",
        "Discard changes",
        "Undo all of Arrange",
        "Details",
    ] {
        let bounds = native.text_bounds(cards.control_rail(), text);
        assert!(bounds.height < 24., "{text} stays on one line: {bounds:?}");
    }
    capture_rail(&cards, "fb08-recovery");
    let _ = cards.update(Message::RecoveryDetails);
    native.text_bounds(cards.control_rail(), "Hide details");
}

#[test]
fn dt02_menu_names_the_widget() {
    assert_eq!(
        layout::widget_menu_labels(Widget::Devbox),
        (
            "System monitor settings…".into(),
            "Hide system monitor".into()
        )
    );
    assert_eq!(
        layout::widget_menu_labels(Widget::Music),
        ("Music settings…".into(), "Hide music".into())
    );
}

#[test]
fn dt02_right_click_menu_hides_the_widget_it_was_opened_on() {
    if isolated("dt02_right_click_menu_hides_the_widget_it_was_opened_on") {
        return;
    }
    let mut cards = cards();
    let _ = cards.update(Message::WidgetMenu(Widget::Music));
    let (id, widget) = cards.menu.expect("menu opened");
    assert_eq!(widget, Widget::Music);
    let size = Size::new(layout::MENU_WIDTH as f32, 120.);
    let mut native = Native::new(size);
    for text in ["Music settings…", "Arrange widgets", "Hide music"] {
        let bounds = native.text_bounds(cards.view_window(id), text);
        assert!(bounds.y + bounds.height <= size.height, "{text} fits");
    }
    for (theme, shade) in [
        (cosmic::Theme::light(), "light"),
        (cosmic::Theme::dark(), "dark"),
    ] {
        native.capture(cards.view_window(id), &theme, &format!("dt02-menu-{shade}"));
    }
    // Escape closes it without a pick.
    let _ = cards.update(Message::KeyInput(
        id,
        Key::Named(Named::Escape),
        Modifiers::empty(),
    ));
    assert!(cards.menu.is_none());
    let _ = cards.update(Message::WidgetMenu(Widget::Music));
    let _ = cards.update(Message::WidgetMenuPick(layout::MenuPick::Hide));
    assert!(cards.menu.is_none());
    assert!(!Config::load().widget_visible(Widget::Music));
    // Arranging turns the menu off so right clicks stay with the editor.
    cards.layout_editor.active = true;
    let mut native = Native::new(Size::new(360., 140.));
    let card = cards.with_menu(Widget::Music, cosmic::widget::text("card").into());
    let (messages, _) = native.event(
        card,
        cosmic::iced::Event::Mouse(cosmic::iced::mouse::Event::ButtonPressed(
            cosmic::iced::mouse::Button::Right,
        )),
        cosmic::iced::Point::new(10., 10.),
    );
    assert!(messages.is_empty());
}

#[test]
fn dt14_idle_music_card_keeps_its_slot_and_offers_the_last_track() {
    use crate::tools::cards::{Action, MusicView, music as card};
    use crate::tools::music;
    let config = Config::default();
    let size = Size::new(480., 360.);
    let mut native = Native::new(size);
    // Nothing played yet: one muted line, centred in the full card.
    let nothing = || {
        card::<Message>(
            MusicView {
                feed: None,
                player: None,
                art: None,
                now: Instant::now(),
                manually_selected: false,
                problem: false,
                last: None,
            },
            &config,
            None,
            Some([480., 360.]),
        )
    };
    let title = native.text_bounds(nothing(), "Nothing playing");
    let middle = title.y + title.height / 2.;
    assert!(
        (middle - 180.).abs() < 24.,
        "the line sits in the middle of the card: {title:?}"
    );
    // The player that played it is still there: the track and a Resume
    // that sends Play to that player.
    let mut stopped = crate::tools::cards::recipe::sample_player(Instant::now());
    let last = music::LastTrack::of(&stopped).unwrap();
    stopped.status = music::Playback::Stopped;
    stopped.title.clear();
    let feed = music::Feed::Players(vec![stopped.clone()]);
    let resumable_at = |[width, height]: [f32; 2]| {
        card(
            MusicView {
                feed: Some(&feed),
                player: Some(&stopped),
                art: None,
                now: Instant::now(),
                manually_selected: false,
                problem: false,
                last: Some(&last),
            },
            &config,
            Some(Message::Tool),
            Some([width, height]),
        )
    };
    let resumable = || resumable_at([480., 360.]);
    // One row like the playing card: artwork, the title over "artist ·
    // where it played", and Resume at the row's end, centred in the card.
    // 360×144 is the size the card has on a real desktop.
    for size in [[480., 360.], [360., 144.]] {
        let mut native = Native::new(Size::new(size[0], size[1]));
        let title = native.text_bounds(resumable_at(size), &last.title);
        let subtitle = native.text_bounds(
            resumable_at(size),
            &format!("{} · Last played in {}", last.author, last.player),
        );
        let resume = native.bounds(resumable_at(size), "music-resume");
        let middle = size[1] / 2.;
        assert!(subtitle.y > title.y, "{title:?} {subtitle:?}");
        assert!(
            (resume.center().y - middle).abs() < 6.,
            "Resume centred in the card: {resume:?} at {size:?}"
        );
        assert!(
            ((title.y + subtitle.y + subtitle.height) / 2. - middle).abs() < 8.,
            "text centred in the card: {title:?} {subtitle:?} at {size:?}"
        );
        assert!(
            resume.x >= subtitle.x + subtitle.width && resume.x + resume.width <= size[0],
            "Resume ends the row: {resume:?} at {size:?}"
        );
        assert!(
            size[0] - (resume.x + resume.width) < 32.,
            "{resume:?} at {size:?}"
        );
    }
    let resume = native.bounds(resumable(), "music-resume");
    let press = cosmic::iced::Event::Mouse(cosmic::iced::mouse::Event::ButtonPressed(
        cosmic::iced::mouse::Button::Left,
    ));
    let (messages, _) = native.event(resumable(), press.clone(), resume.center());
    assert!(
        messages.iter().any(|message| matches!(
            message,
            Message::Tool(Action::Music(music::TransportRequest { bus, command: music::Command::Play }))
                if *bus == stopped.bus
        )),
        "{messages:?}"
    );
    // Its player quit: the track stays, and Resume does nothing.
    let empty = music::Feed::Players(Vec::new());
    let closed = || {
        card(
            MusicView {
                feed: Some(&empty),
                player: None,
                art: None,
                now: Instant::now(),
                manually_selected: false,
                problem: false,
                last: Some(&last),
            },
            &config,
            Some(Message::Tool),
            Some([480., 360.]),
        )
    };
    native.text_bounds(
        closed(),
        &format!("{} · {} isn’t open", last.author, last.player),
    );
    let resume = native.bounds(closed(), "music-resume");
    assert!(native.event(closed(), press, resume.center()).0.is_empty());
    for (theme, shade) in [
        (cosmic::Theme::light(), "light"),
        (cosmic::Theme::dark(), "dark"),
    ] {
        native.capture(nothing(), &theme, &format!("dt14-music-idle-{shade}"));
        native.capture(resumable(), &theme, &format!("dt14-music-last-{shade}"));
        Native::new(Size::new(360., 144.)).capture(
            resumable_at([360., 144.]),
            &theme,
            &format!("dt14-music-last-360x144-{shade}"),
        );
    }
}

/// Review pictures of the System monitor and the idle Music card, written
/// only when `DESKTOP_CARDS_LAYOUT_EVIDENCE` names a folder.
#[test]
fn evidence_monitor_and_music_cards() {
    let sample = settings_preview::monitor_sample();
    let histories = history::MetricHistories::settings_sample(&sample.metrics);
    let all: Config = serde_json::from_value(serde_json::json!({
        "devbox":{"cpu":true,"memory":true,"disk":true,"network":true,"gpu":true,
            "uptime":false,"services":false,"swap":true,"memory_cache":true,
            "history":{"cpu":true,"memory":true,"disk":true,"network":true,"gpu":true}}
    }))
    .unwrap();
    let plain: Config = serde_json::from_value(serde_json::json!({
        "devbox":{"uptime":false,"services":false}
    }))
    .unwrap();
    for (theme, shade) in [
        (cosmic::Theme::light(), "light"),
        (cosmic::Theme::dark(), "dark"),
    ] {
        for (config, name) in [(&all, "history"), (&plain, "bars")] {
            let height = devbox::natural_height(config, sample.metrics.gpus.len());
            let mut native = Native::new(Size::new(360., height));
            native.capture(
                devbox::view(config, &sample.metrics, &histories, height),
                &theme,
                &format!("monitor-{name}-{shade}"),
            );
        }
        let config = Config::default();
        let mut native = Native::new(Size::new(360., 140.));
        let mut stopped = crate::tools::cards::recipe::sample_player(Instant::now());
        let last = crate::tools::music::LastTrack::of(&stopped).unwrap();
        stopped.status = crate::tools::music::Playback::Stopped;
        stopped.title.clear();
        let feed = crate::tools::music::Feed::Players(vec![stopped.clone()]);
        for (name, last, feed) in [("idle", None, None), ("last", Some(&last), Some(&feed))] {
            native.capture(
                crate::tools::cards::music::<Message>(
                    crate::tools::cards::MusicView {
                        feed,
                        player: feed.map(|_| &stopped),
                        art: None,
                        now: Instant::now(),
                        manually_selected: false,
                        problem: false,
                        last,
                    },
                    &config,
                    Some(Message::Tool),
                    Some([360., 140.]),
                ),
                &theme,
                &format!("music-{name}-{shade}"),
            );
        }
    }
}

/// The Arrange overlay at a common laptop size, nothing selected and one
/// card selected, when a capture folder is set.
#[test]
fn arrange_overlay_capture() {
    if isolated("arrange_overlay_capture") {
        return;
    }
    let mut cards = cards();
    let _output = cards.test_output_ready();
    cards.screen_size = [1280., 800.];
    let _ = cards.arrange();
    assert!(cards.layout_editor.active);
    let mut native = Native::new(Size::new(1280., 800.));
    let theme = cosmic::Theme::dark();
    native.capture(cards.layout_view(), &theme, "arrange-overlay");
    cards.layout_editor.selected = Some(Widget::Weather);
    native.capture(cards.layout_view(), &theme, "arrange-overlay-selected");
    let _ = cards.update(Message::LayoutAddMenu);
    assert!(cards.layout_editor.adding);
    native.capture(cards.layout_view(), &theme, "arrange-overlay-add");
    // The narrowest display the tests arrange on keeps the toolbar's text
    // on screen, its buttons wrapping under the title.
    cards.screen_size = [820., 600.];
    let mut narrow = Native::new(Size::new(820., 600.));
    assert_eq!(narrow.overflowing_text(cards.layout_view()), Vec::new());
    let done = narrow.bounds(cards.layout_view(), "arrange-done");
    assert!(done.x + done.width <= 820. - 8., "{done:?}");
    narrow.capture(cards.layout_view(), &theme, "arrange-overlay-narrow");
}

/// A customized edge menu at its own height, in both themes, when a capture
/// folder is set.
fn capture_custom_rail(cards: &Cards, name: &str) {
    let size = Size::new(drawer::SIDEBAR_WIDTH as f32, cards.controls_height() as f32);
    for (theme, shade) in [
        (cosmic::Theme::light(), "light"),
        (cosmic::Theme::dark(), "dark"),
    ] {
        Native::new(size).capture(cards.control_rail(), &theme, &format!("{name}-{shade}"));
    }
}

/// Every item type a customized menu can hold, with an app that is
/// installed, one that was removed, a folder and an entry from a newer
/// Halcyon.
fn custom_menu_config(root: &std::path::Path) -> (Config, std::path::PathBuf) {
    use crate::config::edge_menu::{Action, Item, Kind};
    let apps = root.join("data/applications");
    std::fs::create_dir_all(&apps).unwrap();
    std::fs::write(
        apps.join("org.example.Player.desktop"),
        "[Desktop Entry]\nType=Application\nName=Player\nIcon=multimedia-player\nExec=sentinel-run\n",
    )
    .unwrap();
    let folder = root.join("Projects");
    std::fs::create_dir_all(&folder).unwrap();
    let app = |id: &str, name: &str, icon: &str| {
        Item::new(Kind::App {
            app: id.into(),
            name: name.into(),
            icon: icon.into(),
        })
    };
    let mut items = vec![
        Item::title("Apps"),
        app("org.example.Player", "Player", "multimedia-player"),
        app("org.example.Gone", "Gone", ""),
        Item::new(Kind::Folder {
            path: folder.to_string_lossy().into_owned(),
            name: "Projects".into(),
        }),
        Item::new(Kind::Separator),
        Item::widget(Widget::Weather),
    ];
    items.extend(Action::ALL.map(Item::action));
    items.push(Item::new(Kind::Look {
        look: crate::automation::ProfileRef::Preset("night".into()),
        name: "Night".into(),
    }));
    let mut config = Config::load();
    config.edge_menu.items = Some(items);
    let mut value = serde_json::to_value(&config).unwrap();
    value["edge_menu"]["items"]
        .as_array_mut()
        .unwrap()
        .push(serde_json::json!({"type": "hologram", "depth": 3}));
    std::fs::write(Config::path().unwrap(), value.to_string()).unwrap();
    (Config::load(), folder)
}

/// Each item of a customized menu sends the message the desktop already
/// handles for it; titles, separators, a removed app and an unknown entry
/// send nothing.
#[test]
fn edge_items_trigger_their_existing_actions_and_missing_apps_are_unavailable() {
    if isolated("edge_items_trigger_their_existing_actions_and_missing_apps_are_unavailable") {
        return;
    }
    use crate::activity_island::bridge::{Command, Event};
    use crate::config::edge_menu::Action;
    let root = std::path::PathBuf::from(std::env::var_os("DESKTOP_CARDS_DESKTOP_ROOT").unwrap());
    let (config, folder) = custom_menu_config(&root);
    let items = config.edge_items();
    assert_eq!(items.len(), 6 + Action::ALL.len() + 2);
    let cards = cards();
    let message = |index: usize| cards.edge_item_message(index);
    let at = |action: Action| 6 + Action::ALL.iter().position(|a| *a == action).unwrap();
    assert!(message(0).is_none(), "a title");
    assert!(matches!(message(1), Some(Message::LaunchApp(id)) if id == "org.example.Player"));
    assert!(!cards.edge_available(2), "a removed app");
    assert!(message(2).is_none());
    assert!(matches!(message(3), Some(Message::Open(path)) if path == folder.to_string_lossy()));
    assert!(message(4).is_none(), "a separator");
    assert!(matches!(
        message(5),
        Some(Message::ToggleWidget(Widget::Weather))
    ));
    let shown = cards.config.widgets_visible;
    assert!(
        matches!(message(at(Action::ShowWidgets)), Some(Message::SetWidgetsVisible(v)) if v == !shown)
    );
    assert!(matches!(
        message(at(Action::Arrange)),
        Some(Message::Arrange)
    ));
    assert!(matches!(
        message(at(Action::Settings)),
        Some(Message::Settings)
    ));
    assert!(matches!(
        message(at(Action::DoNotDisturb)),
        Some(Message::SetDnd(true))
    ));
    assert!(matches!(
        message(at(Action::Timer)),
        Some(Message::Activity(Event::Command(Command::TimerStart {
            ms: 300_000
        })))
    ));
    assert!(matches!(message(at(Action::Focus)), Some(Message::FocusCommand(c)) if c == "toggle"));
    let rain = cards.config.rain.enabled;
    assert!(
        matches!(message(at(Action::WeatherEffect)), Some(Message::SetWeatherEffect(v)) if v == !rain)
    );
    let look = items.len() - 2;
    assert!(matches!(
        message(look),
        Some(Message::ApplyLook(crate::automation::ProfileRef::Preset(id), name))
            if id == "night" && name == "Night"
    ));
    let unknown = items.len() - 1;
    assert!(!cards.edge_available(unknown));
    assert!(message(unknown).is_none(), "from a newer Halcyon");
    // A screen reader hears every row's name and state.
    let mut cards = cards;
    let _ = cards.update(Message::DrawerToggle);
    let names: Vec<String> = rail_for(&cards)
        .a11y(cards.control_rail())
        .into_iter()
        .filter_map(|(_, name, _)| name)
        .collect();
    for wanted in [
        "Player",
        "Gone, Unavailable",
        "Projects",
        "Do not disturb, off",
    ] {
        assert!(names.iter().any(|n| n == wanted), "{wanted}: {names:?}");
    }
}

fn rail_for(cards: &Cards) -> Native {
    Native::new(Size::new(
        drawer::SIDEBAR_WIDTH as f32,
        cards.controls_height() as f32,
    ))
}

/// The keyboard reaches every item of a customized menu and "Customize this
/// menu"; Alt+arrows move an item and save the order; Customize closes the
/// menu on its way to Settings.
#[test]
fn custom_edge_menu_keyboard_moves_and_customize() {
    if isolated("custom_edge_menu_keyboard_moves_and_customize") {
        return;
    }
    use crate::config::edge_menu::Kind;
    let root = std::path::PathBuf::from(std::env::var_os("DESKTOP_CARDS_DESKTOP_ROOT").unwrap());
    let (config, _) = custom_menu_config(&root);
    let items = config.edge_items();
    let mut cards = cards();
    let drawer = cards.drawer_items();
    let focusable = items.iter().filter(|item| item.focusable()).count();
    assert_eq!(drawer.len(), focusable + 1);
    assert_eq!(
        drawer[0],
        drawer::DrawerItem::Entry(1),
        "the title is skipped"
    );
    assert_eq!(drawer.last(), Some(&drawer::DrawerItem::Customize));
    let _ = cards.update(Message::DrawerToggle);
    assert_eq!(cards.sidebar.focus, Some(0));
    capture_custom_rail(&cards, "edge-menu-custom");
    // Alt+Down moves Player below Gone and saves; the keyboard stays on it.
    press(&mut cards, Key::Named(Named::ArrowDown), Modifiers::ALT);
    let player = |items: &[crate::config::edge_menu::Item]| {
        items
            .iter()
            .position(
                |item| matches!(&item.kind, Kind::App { app, .. } if app == "org.example.Player"),
            )
            .unwrap()
    };
    assert_eq!(player(&cards.config.edge_items()), 2);
    assert_eq!(player(&Config::load().edge_items()), 2);
    assert_eq!(
        cards.drawer_items()[cards.sidebar.focus.unwrap()],
        drawer::DrawerItem::Entry(2)
    );
    assert_eq!(
        Config::load().edge_items().last().map(|item| &item.kind),
        items.last().map(|item| &item.kind),
        "the unknown entry is kept"
    );
    press(&mut cards, Key::Named(Named::ArrowUp), Modifiers::ALT);
    assert_eq!(player(&Config::load().edge_items()), 1);
    // Enter on a widget switches it, and the menu stays.
    let weather = cards
        .drawer_items()
        .iter()
        .position(|item| *item == drawer::DrawerItem::Entry(5))
        .unwrap();
    while cards.sidebar.focus != Some(weather) {
        press(&mut cards, Key::Named(Named::ArrowDown), Modifiers::empty());
    }
    let was = cards.config.widget_visible(Widget::Weather);
    press(&mut cards, Key::Named(Named::Enter), Modifiers::empty());
    assert_eq!(cards.config.widget_visible(Widget::Weather), !was);
    assert!(cards.sidebar.visible);
    // Up from the top wraps to Customize; Enter closes the menu.
    while cards.sidebar.focus != Some(0) {
        press(&mut cards, Key::Named(Named::ArrowUp), Modifiers::empty());
    }
    press(&mut cards, Key::Named(Named::ArrowUp), Modifiers::empty());
    assert_eq!(
        cards.drawer_items()[cards.sidebar.focus.unwrap()],
        drawer::DrawerItem::Customize
    );
    press(&mut cards, Key::Named(Named::Enter), Modifiers::empty());
    assert!(!cards.sidebar.visible);
}

/// The built-in menu, unchanged for anyone who never customized it, on the
/// left edge: the same rows with the rounded side facing the screen.
#[test]
fn default_edge_menu_matches_the_built_in_one_on_either_side() {
    if isolated("default_edge_menu_matches_the_built_in_one_on_either_side") {
        return;
    }
    use crate::config::edge_menu::{Side, default_items};
    let mut cards = cards();
    assert!(cards.config.edge_menu.items.is_none());
    // The list that customizing starts from is this menu, in order.
    let listed: Vec<drawer::DrawerItem> = default_items(&cards.config.ordered_right_cards())
        .iter()
        .filter_map(|item| {
            item.as_widget()
                .map(drawer::DrawerItem::Widget)
                .or_else(|| match item.as_action()? {
                    crate::config::edge_menu::Action::Arrange => Some(drawer::DrawerItem::Arrange),
                    crate::config::edge_menu::Action::Settings => {
                        Some(drawer::DrawerItem::Settings)
                    }
                    _ => None,
                })
        })
        .chain([drawer::DrawerItem::Customize])
        .collect();
    assert_eq!(cards.drawer_items(), listed);
    let mut config = cards.config.clone();
    config.edge_menu.side = Side::Left;
    let _ = cards.apply_config(config);
    let _ = cards.update(Message::DrawerToggle);
    assert_eq!(cards.drawer_items(), listed);
    capture_rail(&cards, "edge-menu-left");
}
