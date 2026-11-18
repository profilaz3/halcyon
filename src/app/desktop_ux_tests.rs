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
    let (mut cards, _) = Cards::init(Core::default(), Config::load());
    let check = cards.refresh_edge_available();
    if cards.edge.checking.is_some() {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        let _guard = runtime.enter();
        config_writes::finish(&mut cards, check);
    }
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
    // Settings are saved off the UI thread; let a save land.
    let input = Message::KeyInput(cards.ids[2], key, modifiers);
    super::config_writes::update_saved(cards, input);
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
        (
            Key::Named(Named::Enter),
            Modifiers::SHIFT,
            DrawerKey::Interact,
        ),
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
    // FIRST-07: a damaged settings file says so and that the desktop still
    // works, and offers Settings' repair, not a Try again that can't work.
    // The parser's words wait under Details.
    std::fs::write(&path, "{broken").unwrap();
    let task = cards.update(Message::SetWidgetVisible(Widget::Music, false));
    super::config_writes::finish(&mut cards, task);
    let notice = cards.notice.as_ref().expect("the failure is shown");
    assert_eq!(notice.title, "Halcyon’s settings file is damaged");
    assert!(cards.sidebar.visible, "the edge menu opens to show it");
    let mut native = rail();
    native.text_bounds(cards.control_rail(), "Halcyon’s settings file is damaged");
    native.text_bounds(
        cards.control_rail(),
        "Your desktop still works. Your change wasn’t kept.",
    );
    native.text_bounds(cards.control_rail(), "Open Settings to repair");
    native.bounds(cards.control_rail(), "notice-repair");
    assert!(
        native
            .try_bounds(cards.control_rail(), "notice-retry")
            .is_none(),
        "trying again can't help"
    );
    let error = notice.error.clone();
    assert!(
        native.text_all(cards.control_rail(), &error).is_empty(),
        "details start hidden"
    );
    capture_rail(&cards, "fb04-damaged");
    let _ = cards.update(Message::NoticeDetails);
    assert!(!native.text_all(cards.control_rail(), &error).is_empty());
    let _ = cards.update(Message::NoticeDismiss);
    // A save that fails for another reason (a link, which settings never
    // follow) explains itself with Try again.
    let elsewhere = path.with_file_name("elsewhere.json");
    std::fs::write(&elsewhere, &saved).unwrap();
    std::fs::remove_file(&path).unwrap();
    std::os::unix::fs::symlink(&elsewhere, &path).unwrap();
    let task = cards.update(Message::SetWidgetVisible(Widget::Music, false));
    super::config_writes::finish(&mut cards, task);
    let notice = cards.notice.as_ref().expect("the failure is shown");
    assert_eq!(notice.title, "Couldn’t save the widget list");
    native.text_bounds(cards.control_rail(), "Couldn’t save the widget list");
    native.text_bounds(cards.control_rail(), "Your change wasn’t kept.");
    native.bounds(cards.control_rail(), "notice-retry");
    native.bounds(cards.control_rail(), "notice-dismiss");
    assert!(
        native
            .try_bounds(cards.control_rail(), "notice-repair")
            .is_none()
    );
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
    std::fs::remove_file(&path).unwrap();
    std::fs::write(&path, &saved).unwrap();
    let task = cards.update(Message::NoticeRetry);
    super::config_writes::finish(&mut cards, task);
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
    let error = "Could not save: the settings file is busy";
    cards.paused_layout = Some(layout::LayoutEditor::failed(false, Some(error), None));
    cards.sidebar.show(Instant::now());
    let mut native = rail();
    native.text_bounds(cards.control_rail(), "Your arrangement wasn’t saved");
    for text in ["Keep arranging", "Put everything back", "Details"] {
        let bounds = native.text_bounds(cards.control_rail(), text);
        assert!(bounds.height < 24., "{text} stays on one line: {bounds:?}");
    }
    // The long label wraps inside its button rather than being cut off.
    let label = native.text_bounds(cards.control_rail(), "Keep what’s on the desktop");
    let button = native.bounds(cards.control_rail(), "arrange-keep");
    assert!(
        label.y >= button.y && label.y + label.height <= button.y + button.height,
        "{label:?} inside {button:?}"
    );
    // Each choice says what it does.
    native.text_bounds(cards.control_rail(), "Leaves the desktop as it is now.");
    native.text_bounds(
        cards.control_rail(),
        "Undoes every change you made while arranging.",
    );
    assert!(native.text_all(cards.control_rail(), error).is_empty());
    capture_rail(&cards, "fb08-recovery");
    let _ = cards.update(Message::RecoveryDetails);
    native.text_bounds(cards.control_rail(), "Hide details");
    assert!(!native.text_all(cards.control_rail(), error).is_empty());
    let _ = cards.update(Message::RecoveryDetails);

    // A failed Cancel left the saved changes on the desktop, and says so.
    cards.paused_layout = Some(layout::LayoutEditor::failed(true, Some(error), None));
    native.text_bounds(cards.control_rail(), "Couldn’t put everything back");
    assert!(
        native
            .text_all(cards.control_rail(), "Your arrangement wasn’t saved")
            .is_empty()
    );
    // A failed Put everything back shows on the card, not only in Details.
    cards.paused_layout.as_mut().unwrap().retry_failed = true;
    native.text_bounds(
        cards.control_rail(),
        "That didn’t work either. Try again, or keep what’s on the desktop.",
    );
    capture_rail(&cards, "fb08-recovery-cancel-failed");
    cards.paused_layout = Some(layout::LayoutEditor::failed(false, Some(error), None));
    cards.paused_layout.as_mut().unwrap().retry_failed = true;
    native.text_bounds(
        cards.control_rail(),
        "Couldn’t put everything back. Try again, or keep what’s on the desktop.",
    );

    // A plain reason is the body; with no other error there is no Details.
    let reason = "Displays changed. Reconnect the original displays to keep arranging, or choose another option.";
    cards.paused_layout = Some(layout::LayoutEditor::failed(false, None, Some(reason)));
    native.text_bounds(cards.control_rail(), reason);
    assert!(
        native
            .try_bounds(cards.control_rail(), "arrange-details")
            .is_none()
    );
}

#[test]
fn fb08_keep_line_counts_the_saved_changes() {
    assert_eq!(
        layout::keep_line(0, false),
        "Leaves the desktop as it is now."
    );
    assert_eq!(
        layout::keep_line(0, true),
        "Drops the changes that weren’t saved."
    );
    assert_eq!(layout::keep_line(1, false), "Keeps your saved change.");
    assert_eq!(
        layout::keep_line(3, true),
        "Keeps your 3 saved changes and drops the unsaved ones."
    );
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
    super::config_writes::update_saved(
        &mut cards,
        Message::WidgetMenuPick(layout::MenuPick::Hide),
    );
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
fn wid12_hiding_from_the_menu_says_so_briefly_with_undo() {
    if isolated("wid12_hiding_from_the_menu_says_so_briefly_with_undo") {
        return;
    }
    let mut cards = cards();
    assert!(Config::load().widget_visible(Widget::Music));
    let _ = cards.update(Message::WidgetMenu(Widget::Music));
    super::config_writes::update_saved(&mut cards, Message::WidgetMenuPick(layout::MenuPick::Hide));
    assert!(!Config::load().widget_visible(Widget::Music));
    // The edge menu opens on its own, says what happened and offers Undo.
    assert!(cards.sidebar.visible);
    let notice = cards.notice.as_ref().expect("a confirmation");
    assert!(notice.brief);
    let mut native = rail();
    native.text_bounds(cards.control_rail(), "Music hidden");
    native.text_bounds(cards.control_rail(), "Undo");
    assert!(
        native
            .try_bounds(cards.control_rail(), "notice-details")
            .is_none()
    );
    capture_rail(&cards, "wid12-hidden");
    // Undo shows the widget again.
    super::config_writes::update_saved(&mut cards, Message::NoticeRetry);
    assert!(Config::load().widget_visible(Widget::Music));
    assert!(cards.notice.is_none());

    // Left alone, the menu slides away after a few seconds and the
    // confirmation goes with it.
    let _ = cards.update(Message::WidgetMenu(Widget::Music));
    super::config_writes::update_saved(&mut cards, Message::WidgetMenuPick(layout::MenuPick::Hide));
    assert!(cards.notice.is_some());
    let later = Instant::now() + super::panel::BRIEF_NOTICE + Duration::from_millis(50);
    cards.sidebar.tick(later, false);
    assert!(!cards.sidebar.visible);
    let _ = cards.update(Message::Animate);
    assert!(cards.notice.is_none());
    assert!(!Config::load().widget_visible(Widget::Music));

    // A failure already in the menu is not replaced.
    cards.raise_notice("Couldn’t save", "Body", "error".into(), None);
    let _ = cards.update(Message::WidgetMenu(Widget::Weather));
    super::config_writes::update_saved(&mut cards, Message::WidgetMenuPick(layout::MenuPick::Hide));
    assert!(!cards.notice.as_ref().unwrap().brief);
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
    let _ = native.event(resumable(), press.clone(), resume.center());
    let release = cosmic::iced::Event::Mouse(cosmic::iced::mouse::Event::ButtonReleased(
        cosmic::iced::mouse::Button::Left,
    ));
    let (messages, _) = native.event(resumable(), release.clone(), resume.center());
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

#[test]
fn edge_availability_uses_one_background_check_and_ignores_a_replaced_list() {
    if isolated("edge_availability_uses_one_background_check_and_ignores_a_replaced_list") {
        return;
    }
    let root = std::path::PathBuf::from(std::env::var_os("DESKTOP_CARDS_DESKTOP_ROOT").unwrap());
    let (config, _) = custom_menu_config(&root);
    let (mut cards, _) = Cards::init(Core::default(), config);
    assert!(
        cards.edge.checking.is_none(),
        "a closed menu does not scan applications at startup"
    );
    let first = cards.refresh_edge_available();
    let original = cards.edge.checking.clone().unwrap();
    drop(cards.refresh_edge_available());
    assert!(std::sync::Arc::ptr_eq(
        cards.edge.checking.as_ref().unwrap(),
        &original
    ));
    assert!(!cards.edge_available(1));
    assert!(
        cards.edge_item_message(5).is_some(),
        "widget controls work during the scan"
    );

    let old_menu = cards.config.edge_menu.clone();
    cards.config.edge_menu.items.as_mut().unwrap().swap(1, 2);
    drop(cards.edge_menu_changed(&old_menu));
    let second = cards.refresh_edge_available();
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    let _guard = runtime.enter();
    config_writes::finish(&mut cards, first);
    assert!(!cards.edge_available(1));
    assert!(
        cards.edge.checking.is_some(),
        "the old result cannot finish the new scan"
    );
    config_writes::finish(&mut cards, second);
    assert!(!cards.edge_available(1));
    assert!(cards.edge_available(2));
    assert!(cards.edge.checking.is_none());
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

/// The built-in menu has room for every widget, Devices included, with all
/// three right column cards shown.
#[test]
fn built_in_edge_menu_fits_every_widget() {
    if isolated("built_in_edge_menu_fits_every_widget") {
        return;
    }
    let mut cards = cards();
    let mut config = cards.config.clone();
    config.glance.enabled = false;
    let _ = cards.apply_config(config);
    assert!(cards.config.edge_menu.classic());
    assert_eq!(cards.config.ordered_right_cards().len(), 3);
    let _ = cards.update(Message::DrawerToggle);
    let mut rail = rail();
    // Named like a customized menu and its page in Settings.
    rail.text_bounds(cards.control_rail(), "Edge menu");
    assert!(rail.text_all(cards.control_rail(), "Widgets").is_empty());
    let devices = rail.text_bounds(cards.control_rail(), "Devices");
    let customize = rail.bounds(cards.control_rail(), "edge-customize");
    assert!(
        devices.y + devices.height < customize.y,
        "{devices:?} {customize:?}"
    );
    assert!(
        customize.y + customize.height <= drawer::SIDEBAR_HEIGHT as f32 - drawer::RAIL_PADDING,
        "{customize:?}"
    );
    assert!(
        cards
            .drawer_items()
            .contains(&drawer::DrawerItem::Widget(Widget::Devices))
    );
    capture_rail(&cards, "edge-menu-every-widget");
    // With widgets placed by hand, the menu points at Arrange, right below.
    let mut config = cards.config.clone();
    config.widget_positions.insert("clock".into(), [0.2, 0.2]);
    let _ = cards.apply_config(config);
    assert!(cards.custom_layout());
    rail.text_bounds(
        cards.control_rail(),
        "Your own layout. Use Arrange below to move widgets.",
    );
}

/// The open menu at its surface's own size, room for names included.
fn surface_for(cards: &Cards) -> Native {
    Native::new(Size::new(
        cards.controls_width() as f32,
        cards.controls_height() as f32,
    ))
}

/// The menu in both themes at its surface's size, when a capture folder is
/// set.
fn capture_menu(cards: &mut Cards, name: &str) {
    let desktop = cards.config.desktop_palette;
    let source = cards.config.appearance.color_source;
    // The menu draws in the desktop's colors, so the light picture uses a
    // light desktop palette.
    let light = crate::config::Palette {
        accent: cosmic::iced::Color::from_rgb8(0x7c, 0x4d, 0xd8),
        background: cosmic::iced::Color::from_rgb8(0xf6, 0xf4, 0xf9),
        foreground: cosmic::iced::Color::from_rgb8(0x1f, 0x1c, 0x26),
        muted: cosmic::iced::Color::from_rgb8(0x6a, 0x66, 0x74),
    };
    for (theme, shade, palette) in [
        (cosmic::Theme::light(), "light", Some(light)),
        (cosmic::Theme::dark(), "dark", desktop),
    ] {
        cards.config.desktop_palette = palette;
        if shade == "light" {
            cards.config.appearance.color_source = crate::config::ColorSource::Desktop;
        } else {
            cards.config.appearance.color_source = source;
        }
        surface_for(cards).capture(cards.control_rail(), &theme, &format!("{name}-{shade}"));
    }
    cards.config.desktop_palette = desktop;
    cards.config.appearance.color_source = source;
}

/// Saves `changes` to the menu's settings and hands them to `cards`.
fn set_edge(cards: &mut Cards, changes: serde_json::Value) {
    let config = Config::save_edge_menu(changes).unwrap();
    let _ = cards.apply_config(config);
}

/// In the Icons menu every item still has its name and state for a screen
/// reader, a switch that is on and the running timer show it on the icon,
/// the name shows beside the item under the pointer or the keyboard, and
/// the keyboard moves through and reorders the items as in the list.
#[test]
fn icons_menu_names_items_with_their_state_and_keeps_the_keyboard() {
    if isolated("icons_menu_names_items_with_their_state_and_keeps_the_keyboard") {
        return;
    }
    use crate::config::edge_menu::{Action, Kind};
    let root = std::path::PathBuf::from(std::env::var_os("DESKTOP_CARDS_DESKTOP_ROOT").unwrap());
    let (config, _) = custom_menu_config(&root);
    let items = config.edge_items();
    let mut cards = cards();
    let list_width = cards.controls_width();
    set_edge(&mut cards, serde_json::json!({ "style": "icons" }));
    assert!(cards.compact());
    assert!(
        cards.rail_width() < 80,
        "a slim column: {}",
        cards.rail_width()
    );
    assert!(cards.rail_width() < list_width);
    assert_eq!(cards.config.edge_items(), items, "the same list");
    cards.activity.dnd = true;
    let now = chrono::Utc::now().timestamp_millis();
    cards.activity.timer = Some(crate::activity_island::timer::Timer::start(272_500, now));
    let _ = cards.update(Message::DrawerToggle);
    let names: Vec<String> = surface_for(&cards)
        .a11y(cards.control_rail())
        .into_iter()
        .filter_map(|(_, name, _)| name)
        .collect();
    for wanted in [
        "Player",
        "Gone, unavailable",
        "Projects",
        "Weather, on",
        "Do not disturb, on",
        "Customize this menu",
    ] {
        assert!(names.iter().any(|n| n == wanted), "{wanted}: {names:?}");
    }
    let effect = format!(
        "Weather effect, {}",
        if cards.config.rain.enabled {
            "on"
        } else {
            "off"
        }
    );
    assert!(names.contains(&effect), "{effect}: {names:?}");
    assert!(
        names
            .iter()
            .any(|n| n.starts_with("Timer, 4:3") && n.ends_with(" left")),
        "the time left: {names:?}"
    );
    // The keyboard starts on Player, and its name shows beside it.
    assert_eq!(
        cards.drawer_focus(),
        Some(drawer::DrawerItem::Entry(1)),
        "the title is skipped"
    );
    let text = surface_for(&cards).all_text(cards.control_rail());
    assert!(text.iter().any(|t| t == "Player"), "{text:?}");
    // Down to Do not disturb: its name and state follow the keyboard.
    let dnd = items
        .iter()
        .position(|item| item.as_action() == Some(Action::DoNotDisturb))
        .unwrap();
    while cards.drawer_focus() != Some(drawer::DrawerItem::Entry(dnd)) {
        press(&mut cards, Key::Named(Named::ArrowDown), Modifiers::empty());
    }
    let text = surface_for(&cards).all_text(cards.control_rail());
    assert!(
        text.iter().any(|t| t == "Do not disturb") && text.iter().any(|t| t == "On"),
        "{text:?}"
    );
    capture_menu(&mut cards, "edge-menu-icons-focus");
    // The pointer on the timer shows its name and time left instead.
    let timer = items
        .iter()
        .position(|item| item.as_action() == Some(Action::Timer))
        .unwrap();
    let _ = cards.update(Message::EdgeTip(Some(drawer::DrawerItem::Entry(timer))));
    let text = surface_for(&cards).all_text(cards.control_rail());
    assert!(text.iter().any(|t| t == "Timer"), "{text:?}");
    assert!(text.iter().any(|t| t.ends_with(" left")), "{text:?}");
    assert!(
        !text.iter().any(|t| t == "Do not disturb"),
        "one name at a time"
    );
    capture_menu(&mut cards, "edge-menu-icons-switch-and-timer");
    let _ = cards.update(Message::EdgeTip(None));
    // Back to the top; Alt+Down moves Player below Gone and saves.
    while cards.sidebar.focus != Some(0) {
        press(&mut cards, Key::Named(Named::ArrowUp), Modifiers::empty());
    }
    press(&mut cards, Key::Named(Named::ArrowDown), Modifiers::ALT);
    let player = |items: &[crate::config::edge_menu::Item]| {
        items
            .iter()
            .position(
                |item| matches!(&item.kind, Kind::App { app, .. } if app == "org.example.Player"),
            )
            .unwrap()
    };
    assert_eq!(player(&Config::load().edge_items()), 2);
    assert_eq!(
        cards.drawer_items()[cards.sidebar.focus.unwrap()],
        drawer::DrawerItem::Entry(2),
        "the keyboard stays on it"
    );
    assert_eq!(
        Config::load().edge_menu.style,
        crate::config::edge_menu::Style::Icons
    );
    press(&mut cards, Key::Named(Named::ArrowUp), Modifiers::ALT);
    assert_eq!(player(&Config::load().edge_items()), 1);
    // Enter on a switch flips it and the menu stays.
    let weather = cards
        .drawer_items()
        .iter()
        .position(|item| *item == drawer::DrawerItem::Entry(5))
        .unwrap();
    while cards.sidebar.focus != Some(weather) {
        press(
            &mut cards,
            Key::Named(Named::ArrowRight),
            Modifiers::empty(),
        );
    }
    let was = cards.config.widget_visible(Widget::Weather);
    press(&mut cards, Key::Named(Named::Enter), Modifiers::empty());
    assert_eq!(cards.config.widget_visible(Widget::Weather), !was);
    assert!(cards.sidebar.visible);
}

/// The grid in two and three columns: the same items, wider and shorter
/// than the icon column, every item still reached by the keyboard, and the
/// built-in menu drawn the same way without saving a list of its own.
#[test]
fn grid_menu_sets_icons_in_columns_and_the_keyboard_reaches_each() {
    if isolated("grid_menu_sets_icons_in_columns_and_the_keyboard_reaches_each") {
        return;
    }
    let mut cards = cards();
    // The built-in menu, as icons: no list saved, every row an item.
    set_edge(&mut cards, serde_json::json!({ "style": "icons" }));
    let icons = (cards.rail_width(), cards.controls_height());
    assert!(cards.config.edge_menu.items.is_none());
    let builtin = cards.drawer_items();
    assert!(builtin.iter().all(|item| matches!(
        item,
        drawer::DrawerItem::Entry(_) | drawer::DrawerItem::Customize
    )));
    let _ = cards.update(Message::DrawerToggle);
    capture_menu(&mut cards, "edge-menu-icons");
    set_edge(
        &mut cards,
        serde_json::json!({ "style": "grid", "columns": 2 }),
    );
    let two = (cards.rail_width(), cards.controls_height());
    capture_menu(&mut cards, "edge-menu-grid-2");
    set_edge(&mut cards, serde_json::json!({ "columns": 3 }));
    let three = (cards.rail_width(), cards.controls_height());
    capture_menu(&mut cards, "edge-menu-grid-3");
    assert!(
        icons.0 < two.0 && two.0 < three.0,
        "{icons:?} {two:?} {three:?}"
    );
    assert!(
        three.1 < two.1 && two.1 < icons.1,
        "{icons:?} {two:?} {three:?}"
    );
    assert_eq!(
        cards.drawer_items(),
        builtin,
        "the same items in every style"
    );
    // Names under the icons in the grid; every item is spoken in full.
    let text = surface_for(&cards).all_text(cards.control_rail());
    assert!(text.iter().any(|t| t == "Weather"), "{text:?}");
    let names: Vec<String> = surface_for(&cards)
        .a11y(cards.control_rail())
        .into_iter()
        .filter_map(|(_, name, _)| name)
        .collect();
    assert!(
        names.iter().any(|n| n.starts_with("Weather, ")),
        "{names:?}"
    );
    // The keyboard goes round all of them.
    let count = builtin.len();
    for step in 1..=count {
        press(
            &mut cards,
            Key::Named(Named::ArrowRight),
            Modifiers::empty(),
        );
        assert_eq!(cards.sidebar.focus, Some(step % count));
    }
    // Moving an item from the grid starts a list of its own.
    let first = cards.drawer_items()[0];
    press(&mut cards, Key::Named(Named::ArrowDown), Modifiers::ALT);
    assert!(Config::load().edge_menu.customized());
    assert_eq!(cards.drawer_items()[cards.sidebar.focus.unwrap()], {
        let drawer::DrawerItem::Entry(at) = first else {
            unreachable!()
        };
        drawer::DrawerItem::Entry(at + 1)
    });
    // List again, no labels or titles left over: the rows are back.
    set_edge(
        &mut cards,
        serde_json::json!({ "style": "list", "items": null }),
    );
    assert!(!cards.compact());
    assert_eq!(cards.controls_width(), drawer::SIDEBAR_WIDTH);
}

/// After an item is used, the menu closes or stays as the setting says:
/// by default switches keep it open and everything else closes it; Never
/// keeps it open after a Look; Always closes it after a switch too.
#[test]
fn close_after_use_follows_the_setting_for_each_kind_of_item() {
    if isolated("close_after_use_follows_the_setting_for_each_kind_of_item") {
        return;
    }
    use super::edge::closes_after;
    use crate::config::edge_menu::{Action, Close, Item, Kind};
    for (close, switch, message, closes) in [
        (
            Close::Actions,
            true,
            Message::ToggleWidget(Widget::Weather),
            false,
        ),
        (Close::Actions, false, Message::LaunchApp("x".into()), true),
        (
            Close::Actions,
            false,
            Message::FocusCommand("toggle".into()),
            true,
        ),
        (Close::Never, false, Message::LaunchApp("x".into()), false),
        (Close::Never, false, Message::Arrange, true),
        (Close::Never, false, Message::Settings, true),
        (Close::Always, true, Message::SetDnd(true), true),
    ] {
        assert_eq!(
            closes_after(close, switch, &message),
            closes,
            "{close:?} {message:?}"
        );
    }
    let items = vec![
        Item::widget(Widget::Weather),
        Item::action(Action::Focus),
        Item::new(Kind::Look {
            look: crate::automation::ProfileRef::Preset("night".into()),
            name: "Night".into(),
        }),
    ];
    let mut cards = cards();
    set_edge(
        &mut cards,
        serde_json::json!({ "items": items, "style": "icons" }),
    );
    let mut run = |cards: &mut Cards, index: usize| {
        cards.sidebar.show(Instant::now());
        let _ = cards.update(Message::EdgeItem(index));
        cards.sidebar.visible
    };
    assert!(run(&mut cards, 0), "a switch keeps it open");
    assert!(!run(&mut cards, 1), "an action closes it");
    assert!(!run(&mut cards, 2), "a Look closes it");
    set_edge(&mut cards, serde_json::json!({ "close": "never" }));
    assert!(run(&mut cards, 2), "kept open");
    assert!(run(&mut cards, 0));
    set_edge(&mut cards, serde_json::json!({ "close": "always" }));
    assert!(!run(&mut cards, 0), "a switch closes it too");
}

/// The visible handle draws on the strip in the accent or a neutral shade;
/// hidden, the strip draws nothing at all.
#[test]
fn the_visible_handle_draws_on_the_strip_and_the_hidden_one_is_clear() {
    if isolated("the_visible_handle_draws_on_the_strip_and_the_hidden_one_is_clear") {
        return;
    }
    let mut cards = cards();
    let strip = cards.ids[3];
    let alpha = |cards: &Cards| {
        let thickness = cards.config.edge_menu.strip_thickness();
        let pixels = Native::new(Size::new(thickness as f32, 200.))
            .render(cards.edge_handle(strip), &cosmic::Theme::dark());
        pixels.chunks(4).map(|p| p[3]).max().unwrap_or(0)
    };
    assert_eq!(alpha(&cards), 0, "hidden: nothing drawn");
    set_edge(
        &mut cards,
        serde_json::json!({ "handle": true, "handle_size": 6, "handle_opacity": 100 }),
    );
    assert_eq!(cards.config.edge_menu.strip_thickness(), 6);
    assert!(alpha(&cards) > 200, "visible: an opaque bar");
    set_edge(
        &mut cards,
        serde_json::json!({ "handle": true, "handle_size": 6, "handle_opacity": 30 }),
    );
    let faint = alpha(&cards);
    assert!(faint > 40 && faint < 120, "its opacity follows: {faint}");
    // A picture of the handle at the edge of a desktop.
    if std::env::var_os("DESKTOP_CARDS_LAYOUT_EVIDENCE").is_some() {
        let mut light = cards.config.clone();
        light.edge_menu.handle_opacity = 70;
        let _ = cards.apply_config(light);
        let p = cards.config.surface_palette();
        for (theme, shade, backdrop) in [
            (
                cosmic::Theme::dark(),
                "dark",
                cosmic::iced::Color::from_rgb8(0x2a, 0x2d, 0x3a),
            ),
            (
                cosmic::Theme::light(),
                "light",
                cosmic::iced::Color::from_rgb8(0xd8, 0xdc, 0xe6),
            ),
        ] {
            let _ = p;
            let view = cosmic::widget::container(
                cosmic::widget::row::with_children(vec![
                    cosmic::iced::widget::space().width(Length::Fill).into(),
                    cosmic::widget::container(cards.edge_handle(strip))
                        .width(Length::Fixed(6.))
                        .height(Length::Fixed(220.))
                        .into(),
                ])
                .align_y(cosmic::iced::Alignment::Center)
                .height(Length::Fill),
            )
            .width(Length::Fill)
            .height(Length::Fill)
            .class(cosmic::theme::Container::custom(move |_| {
                cosmic::iced::widget::container::Style {
                    background: Some(cosmic::iced::Background::Color(backdrop)),
                    ..Default::default()
                }
            }));
            Native::new(Size::new(240., 320.)).capture(
                view.into(),
                &theme,
                &format!("edge-handle-visible-{shade}"),
            );
        }
    }
}

/// WID-03: with every widget hidden, the edge menu says so at the top and
/// offers Show widgets; the widget switches show the saved choice and wait
/// instead of quietly saving "on" again, and Arrange offers the same way
/// back in place of "All widgets added".
#[test]
fn wid03_hidden_widgets_have_a_way_back_in_the_edge_menu_and_arrange() {
    if isolated("wid03_hidden_widgets_have_a_way_back_in_the_edge_menu_and_arrange") {
        return;
    }
    use cosmic::iced::{Event, mouse};
    let mut cards = cards();
    let _output = cards.test_output_ready();
    super::config_writes::update_saved(&mut cards, Message::SetWidgetsVisible(false));
    assert!(!cards.config.widgets_visible);
    assert!(cards.config.widget_selected(Widget::Weather));
    let _ = cards.update(Message::DrawerToggle);
    assert_eq!(
        cards.drawer_items().first(),
        Some(&drawer::DrawerItem::ShowWidgets)
    );
    // The menu grows by the row, and everything still fits.
    assert!(cards.controls_height() > drawer::SIDEBAR_HEIGHT);
    let mut native = rail_for(&cards);
    native.text_bounds(cards.control_rail(), "Widgets are hidden");
    let show = native.bounds(cards.control_rail(), "edge-show-widgets");
    let weather = native.text_bounds(cards.control_rail(), "Weather");
    assert!(show.y + show.height < weather.y, "{show:?} {weather:?}");
    let customize = native.bounds(cards.control_rail(), "edge-customize");
    assert!(
        customize.y + customize.height
            <= cards.controls_height() as f32 - drawer::RAIL_PADDING + 0.5,
        "{customize:?}"
    );
    capture_rail_sized(&cards, "wid03-edge-menu-hidden");
    // A widget's row does nothing while every widget is hidden.
    let at = weather.center();
    let mut sent = Vec::new();
    for event in [
        mouse::Event::CursorMoved { position: at },
        mouse::Event::ButtonPressed(mouse::Button::Left),
        mouse::Event::ButtonReleased(mouse::Button::Left),
    ] {
        sent.extend(
            native
                .event(cards.control_rail(), Event::Mouse(event), at)
                .0,
        );
    }
    assert!(
        !sent
            .iter()
            .any(|m| matches!(m, Message::ToggleWidget(_) | Message::SidebarPress(_))),
        "{sent:?}"
    );
    // Nor does Enter on it.
    press(&mut cards, Key::Named(Named::ArrowDown), Modifiers::empty());
    assert_eq!(
        cards.drawer_focus(),
        Some(drawer::DrawerItem::Widget(Widget::Weather))
    );
    press(&mut cards, Key::Named(Named::Enter), Modifiers::empty());
    assert!(Config::load().widget_selected(Widget::Weather));
    assert!(!Config::load().widgets_visible);
    // The command line's toggle flips the saved choice instead of saving
    // "on" for a widget that already is.
    super::config_writes::update_saved(&mut cards, Message::ToggleWidget(Widget::Weather));
    assert!(!Config::load().widget_selected(Widget::Weather));
    super::config_writes::update_saved(&mut cards, Message::ToggleWidget(Widget::Weather));
    assert!(Config::load().widget_selected(Widget::Weather));
    // Arrange: nothing to arrange, said so, and the way to show them.
    let _ = cards.update(Message::HideControls);
    cards.screen_size = [1280., 800.];
    let _ = cards.arrange();
    assert!(cards.layout_editor.active);
    let mut overlay = Native::new(Size::new(1280., 800.));
    assert!(
        overlay
            .text_all(cards.layout_view(), "All widgets added")
            .is_empty()
    );
    overlay.text_bounds(cards.layout_view(), "Widgets are hidden");
    overlay.capture(
        cards.layout_view(),
        &cosmic::Theme::dark(),
        "wid03-arrange-hidden",
    );
    let show = overlay.bounds(cards.layout_view(), "arrange-show-widgets");
    let mut sent = Vec::new();
    for event in [
        mouse::Event::CursorMoved {
            position: show.center(),
        },
        mouse::Event::ButtonPressed(mouse::Button::Left),
        mouse::Event::ButtonReleased(mouse::Button::Left),
    ] {
        sent.extend(
            overlay
                .event(cards.layout_view(), Event::Mouse(event), show.center())
                .0,
        );
    }
    assert!(
        matches!(sent.as_slice(), [Message::SetWidgetsVisible(true)]),
        "{sent:?}"
    );
    for message in sent {
        super::config_writes::update_saved(&mut cards, message);
    }
    assert!(Config::load().widgets_visible);
    assert!(cards.layout_editor.active, "still arranging");
    assert!(cards.arranging_widget(Widget::Weather));
    overlay.bounds(cards.layout_view(), "arrange-add");
    // Hide in Arrange now hides a widget that came back.
    let task = cards.update(Message::LayoutShow(Widget::Weather, false));
    super::config_writes::settle(&mut cards, task);
    assert!(!cards.arranging_widget(Widget::Weather));
    let task = cards.update(Message::LayoutFinish(true));
    super::config_writes::settle(&mut cards, task);
    assert!(!Config::load().widget_selected(Widget::Weather));
    assert!(Config::load().widgets_visible);
    // In the edge menu, Show widgets brings the switches back at once.
    super::config_writes::update_saved(&mut cards, Message::SetWidgetsVisible(false));
    let _ = cards.update(Message::DrawerToggle);
    assert_eq!(cards.sidebar.focus, Some(0));
    press(&mut cards, Key::Named(Named::Enter), Modifiers::empty());
    assert!(Config::load().widgets_visible);
    assert_eq!(cards.controls_height(), drawer::SIDEBAR_HEIGHT);
    assert!(
        rail_for(&cards)
            .try_bounds(cards.control_rail(), "edge-show-widgets")
            .is_none()
    );
}

/// WID-03 in a customized menu and its Icons style: the same row first, and
/// a widget's item waits while every widget is hidden.
#[test]
fn wid03_custom_and_icon_menus_offer_show_widgets_while_hidden() {
    if isolated("wid03_custom_and_icon_menus_offer_show_widgets_while_hidden") {
        return;
    }
    use crate::config::edge_menu::Style;
    let root = std::path::PathBuf::from(std::env::var_os("DESKTOP_CARDS_DESKTOP_ROOT").unwrap());
    let (_, _) = custom_menu_config(&root);
    let mut cards = cards();
    let _output = cards.test_output_ready();
    let weather = 5;
    assert!(matches!(
        cards.edge_item_message(weather),
        Some(Message::ToggleWidget(Widget::Weather))
    ));
    let shown = cards.controls_height();
    super::config_writes::update_saved(&mut cards, Message::SetWidgetsVisible(false));
    assert!(cards.edge_item_message(weather).is_none(), "it waits");
    let item = &cards.config.edge_items()[weather];
    assert_eq!(cards.item_state(item), Some(true), "the saved choice");
    assert_eq!(
        cards.controls_height(),
        shown + (super::edge::SHOW_ROW + drawer::HEADER_GAP) as u32
    );
    let _ = cards.update(Message::DrawerToggle);
    assert_eq!(cards.drawer_items()[0], drawer::DrawerItem::ShowWidgets);
    let mut native = rail_for(&cards);
    native.text_bounds(cards.control_rail(), "Widgets are hidden");
    native.bounds(cards.control_rail(), "edge-show-widgets");
    let names: Vec<String> = native
        .a11y(cards.control_rail())
        .into_iter()
        .filter_map(|(_, name, _)| name)
        .collect();
    assert!(
        names.iter().any(|n| n == "Weather, on, widgets are hidden"),
        "{names:?}"
    );
    capture_custom_rail(&cards, "wid03-edge-menu-custom-hidden");
    // The Icons style: Show widgets first, named in its bubble.
    let mut config = cards.config.clone();
    config.edge_menu.style = Style::Icons;
    let _ = cards.apply_config(config);
    let mut native = surface_for(&cards);
    let show = native.bounds(cards.control_rail(), "edge-show-widgets");
    let first = native.bounds(cards.control_rail(), "edge-item-1");
    assert!(show.y + show.height <= first.y, "{show:?} {first:?}");
    let _ = cards.update(Message::EdgeTip(Some(drawer::DrawerItem::ShowWidgets)));
    native.text_bounds(cards.control_rail(), "Widgets are hidden");
    capture_menu(&mut cards, "wid03-edge-menu-icons-hidden");
    super::config_writes::update_saved(&mut cards, Message::SetWidgetsVisible(true));
    assert!(
        surface_for(&cards)
            .try_bounds(cards.control_rail(), "edge-show-widgets")
            .is_none()
    );
}

/// The built-in menu at its own height, in both themes, when a capture
/// folder is set.
fn capture_rail_sized(cards: &Cards, name: &str) {
    for (theme, shade) in [
        (cosmic::Theme::light(), "light"),
        (cosmic::Theme::dark(), "dark"),
    ] {
        rail_for(cards).capture(cards.control_rail(), &theme, &format!("{name}-{shade}"));
    }
}

/// WID-04: over a maximized or full-screen window, a click on the invisible
/// strip is left to the window: no menu, and the hover wait stops until the
/// pointer leaves. Resting the pointer still opens it, and a visible handle
/// or opening on click keeps a click opening it.
#[test]
fn wid04_a_click_on_the_invisible_strip_over_a_maximized_window_opens_nothing() {
    if isolated("wid04_a_click_on_the_invisible_strip_over_a_maximized_window_opens_nothing") {
        return;
    }
    use crate::config::edge_menu::OpenOn;
    let mut cards = cards();
    let _output = cards.test_output_ready();
    let strip = cards.ids[3];
    let name = cards
        .selected_output
        .as_ref()
        .and_then(crate::displays::Output::name)
        .unwrap()
        .to_string();
    assert!(cards.edge_click_guard(), "on by default");
    // Over the desktop, a click opens it at once, as always.
    let _ = cards.update(Message::EdgeClick(strip));
    assert!(cards.sidebar.visible);
    cards.sidebar.hide();
    // Over a maximized window: the click does nothing and stops the wait.
    cards.rain.desk.covered.insert(name.clone());
    let now = Instant::now();
    let _ = cards.update(Message::EdgeHover(strip, true));
    assert!(cards.sidebar.next_transition().is_some(), "waiting to open");
    let _ = cards.update(Message::EdgeClick(strip));
    assert!(!cards.sidebar.visible);
    assert!(
        cards.sidebar.next_transition().is_none(),
        "the wait stopped"
    );
    cards.sidebar.tick(now + Duration::from_secs(5), true);
    assert!(!cards.sidebar.visible);
    // Leaving and resting there again opens it after the delay.
    let _ = cards.update(Message::EdgeHover(strip, false));
    let _ = cards.update(Message::EdgeHover(strip, true));
    cards
        .sidebar
        .tick(Instant::now() + drawer::EDGE_DWELL, true);
    assert!(cards.sidebar.visible, "resting still opens it");
    cards.sidebar.hide();
    let _ = cards.update(Message::EdgeHover(strip, false));
    // A visible handle is seen, so a click on it is meant.
    let mut config = cards.config.clone();
    config.edge_menu.handle = true;
    let _ = cards.apply_config(config);
    assert!(!cards.edge_click_guard());
    let _ = cards.update(Message::EdgeClick(strip));
    assert!(cards.sidebar.visible);
    cards.sidebar.hide();
    // Opening on click: the click is the only way, so it still opens.
    let mut config = cards.config.clone();
    config.edge_menu.handle = false;
    config.edge_menu.open = OpenOn::Click;
    let _ = cards.apply_config(config);
    assert!(!cards.edge_click_guard());
    let _ = cards.update(Message::EdgeClick(strip));
    assert!(cards.sidebar.visible);
}

/// WID-08: on a 1280x800 display the default right column keeps every row
/// of the System monitor. When the column has to shrink, Weather without a
/// city and the Projects list give up the room first; a weather reading
/// keeps its height too. A tall display shrinks nothing.
#[test]
fn wid08_a_laptop_screen_keeps_the_whole_system_monitor() {
    if isolated("wid08_a_laptop_screen_keeps_the_whole_system_monitor") {
        return;
    }
    let mut cards = cards();
    let _output = cards.test_output_ready();
    let _ = cards.apply_config(Config::default());
    assert_eq!(
        cards.visible_cards(),
        [Widget::Weather, Widget::Devbox, Widget::Projects]
    );
    assert!(cards.config.location.is_none());
    let weather = crate::widget_presentation::WEATHER_SIZE[1];
    let column = |cards: &Cards| {
        cards
            .visible_cards()
            .into_iter()
            .map(|widget| cards.card_height(widget))
            .sum::<f32>()
            + 2. * drawer::CARD_GAP
    };
    let room = 800. - 96.;
    let whole_monitor = |cards: &Cards| {
        assert_eq!(
            cards.card_height(Widget::Devbox),
            cards.devbox_natural_height(),
            "the System monitor keeps every row, Uptime included"
        );
        let size = cards.widget_size(Widget::Devbox);
        let mut native = Native::new(Size::new(size[0], size[1]));
        let uptime = native.text_bounds(cards.devbox(), "Uptime");
        assert!(uptime.y + uptime.height <= size[1], "{uptime:?} {size:?}");
        native
    };
    let monitor = |cards: &Cards| {
        let size = cards.widget_size(Widget::Devbox);
        Native::new(Size::new(size[0], size[1]))
    };
    // The default widgets at 1280x800: the Projects card without folders
    // takes only the room it needs, so nothing is cut off.
    cards.screen_size = [1280., 800.];
    assert!(column(&cards) <= room + 0.5, "{}", column(&cards));
    whole_monitor(&cards);
    // A long Projects list makes the column too tall: Weather without a
    // city and the list give the room, the monitor keeps its rows.
    let projects: Vec<serde_json::Value> = (0..8)
        .map(|n| {
            serde_json::json!({
                "path": format!("/sample/p{n}"), "name": format!("Project {n}"),
                "title": format!("Project {n}"), "repository_id": format!("p{n}"),
                "group": "sample", "upstream": true, "error": false, "branch": "main",
                "changed": 2, "untracked": 0, "conflicts": 0, "ahead": 0, "behind": 0,
                "servers": [], "recent": 0.0
            })
        })
        .collect();
    let mut config = Config::default();
    config.project_roots = vec!["/sample".into()];
    let _ = cards.apply_config(config);
    cards.projects = Some(
        serde_json::from_value(serde_json::json!({
            "updated_at": chrono::Local::now().to_rfc3339(),
            "projects": projects,
        }))
        .unwrap(),
    );
    assert!(cards.projects_natural_height() >= 300.);
    assert!(column(&cards) <= room + 0.5, "{}", column(&cards));
    whole_monitor(&cards);
    assert!(cards.card_height(Widget::Weather) < weather);
    assert!(cards.card_height(Widget::Projects) >= drawer::PROJECTS_LEAST);
    for (theme, shade) in [
        (cosmic::Theme::light(), "light"),
        (cosmic::Theme::dark(), "dark"),
    ] {
        monitor(&cards).capture(
            cards.devbox(),
            &theme,
            &format!("wid08-monitor-1280x800-{shade}"),
        );
    }
    // With a city, the weather reading keeps its height; the list gives.
    let mut config = cards.config.clone();
    config.location = Some(crate::config::Location {
        name: "Zagreb".into(),
        latitude: 45.8,
        longitude: 16.0,
        timezone: None,
    });
    let _ = cards.apply_config(config);
    assert_eq!(cards.card_height(Widget::Weather), weather);
    assert!(column(&cards) <= room + 0.5, "{}", column(&cards));
    whole_monitor(&cards);
    // A tall display has room for every card at its full height.
    cards.screen_size = [1920., 1080.];
    assert_eq!(cards.card_height(Widget::Weather), weather);
    assert_eq!(cards.card_height(Widget::Projects), 300.);
    whole_monitor(&cards);
}

#[test]
fn keyboard_enters_focus_card_starts_timer_and_leaves_on_escape() {
    if isolated("keyboard_enters_focus_card_starts_timer_and_leaves_on_escape") {
        return;
    }
    use cosmic::iced::advanced::widget::operation::{focusable, scoped};
    use cosmic::iced::{Event, Point, keyboard};
    let mut cards = cards();
    cards.config.focus.enabled = true;
    let _ = cards.update(Message::DrawerToggle);
    cards.sidebar.focus = cards
        .drawer_items()
        .iter()
        .position(|item| *item == drawer::DrawerItem::Widget(Widget::Focus));
    assert!(cards.sidebar.focus.is_some());
    let _ = cards.update(Message::KeyInput(
        cards.ids[2],
        Key::Named(Named::Enter),
        Modifiers::SHIFT,
    ));
    assert_eq!(cards.interacting_tool, Some(Widget::Focus));
    assert!(!cards.sidebar.visible);
    let mut native = Native::new(Size::new(380., 300.));
    let mut actions = Vec::new();
    let count = native
        .focus_stops(cards.individual_widget(Widget::Focus))
        .len();
    for _ in 0..=count {
        native.operate(
            cards.individual_widget(Widget::Focus),
            Box::new(scoped("tool-controls".into(), focusable::focus_next())),
        );
        let key = Key::Named(Named::Enter);
        let event = Event::Keyboard(keyboard::Event::KeyPressed {
            key: key.clone(),
            modified_key: key,
            physical_key: keyboard::key::Physical::Code(keyboard::key::Code::Enter),
            location: keyboard::Location::Standard,
            modifiers: Modifiers::empty(),
            text: None,
            repeat: false,
        });
        actions.extend(
            native
                .event(cards.individual_widget(Widget::Focus), event, Point::ORIGIN)
                .0,
        );
    }
    let start = actions
        .into_iter()
        .find(|message| {
            matches!(
                message,
                Message::Tool(crate::tools::cards::Action::FocusToggle)
            )
        })
        .expect("Enter reaches the timer control");
    let _ = cards.update(start);
    assert!(cards.tools.focus.running());
    let id = cards.widget_id(Widget::Focus);
    let _ = cards.update(Message::KeyInput(
        id,
        Key::Named(Named::Escape),
        Modifiers::empty(),
    ));
    assert!(cards.interacting_tool.is_none());
}
