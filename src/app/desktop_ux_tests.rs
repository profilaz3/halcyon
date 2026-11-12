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
    assert_eq!(items.last(), Some(&drawer::DrawerItem::Settings));
    assert_eq!(items[items.len() - 2], drawer::DrawerItem::Arrange);
    let _ = cards.update(Message::DrawerToggle);
    assert!(cards.sidebar.visible);
    assert_eq!(cards.sidebar.focus, Some(0));
    capture_rail(&cards, "ax04-drawer-focus");
    // Up from the top wraps to Settings; down wraps back.
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
