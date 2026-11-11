//! Keyboard and screen-reader reach for Settings' own controls: Tab lands on
//! every switch, slider and dropdown, the keys a person expects work there,
//! and each control has a name.
use super::*;
use crate::native_test::Native;
use cosmic::Apply;
use cosmic::iced::keyboard::{self, Key, key};
use cosmic::iced::{Event, Point, Rectangle, event::Status};
use iced_accessibility::accesskit::Role;

const SIZE: Size = Size::new(820., 600.);

/// Runs `name` in a child process with its own fixture desktop, like the
/// other Settings tests, so nothing touches the real one.
fn child(name: &str) -> bool {
    if std::env::var("DESKTOP_CARDS_UX_TEST").as_deref() == Ok(name) {
        return false;
    }
    let root = std::env::temp_dir().join(format!("cards-keys-{}-{name}", std::process::id()));
    for dir in ["config", "data", "cache", "state", "desktop"] {
        std::fs::create_dir_all(root.join(dir)).unwrap();
    }
    // The fixture desktop offers the system's Reduce motion setting.
    std::fs::create_dir_all(
        root.join("desktop/gsettings")
            .join(crate::desktop::motion::SCHEMA),
    )
    .unwrap();
    std::fs::write(root.join("dconf-profile"), "").unwrap();
    std::fs::write(
        root.join("config.json"),
        r#"{"setup_dismissed":true,"enabled_widgets":["clock","calendar","weather"]}"#,
    )
    .unwrap();
    let result = std::process::Command::new(std::env::current_exe().unwrap())
        .args(["--exact", &format!("settings::keyboard_tests::{name}")])
        .env("DESKTOP_CARDS_UX_TEST", name)
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
    assert!(
        result.status.success(),
        "{}\n{}",
        String::from_utf8_lossy(&result.stdout),
        String::from_utf8_lossy(&result.stderr)
    );
    true
}

fn pressed(key: Key, code: key::Code) -> Event {
    Event::Keyboard(keyboard::Event::KeyPressed {
        key: key.clone(),
        modified_key: key,
        physical_key: key::Physical::Code(code),
        location: keyboard::Location::Standard,
        modifiers: keyboard::Modifiers::empty(),
        text: None,
        repeat: false,
    })
}
fn released(key: Key, code: key::Code) -> Event {
    Event::Keyboard(keyboard::Event::KeyReleased {
        key: key.clone(),
        modified_key: key,
        physical_key: key::Physical::Code(code),
        location: keyboard::Location::Standard,
        modifiers: keyboard::Modifiers::empty(),
    })
}
fn space() -> Event {
    pressed(Key::Character(" ".into()), key::Code::Space)
}
fn named(named: key::Named, code: key::Code) -> Event {
    pressed(Key::Named(named), code)
}

/// What Settings does on Tab: focus the next control and scroll it into view.
fn tab<'a, M>(native: &mut Native, view: impl Fn() -> cosmic::Element<'a, M>) {
    native.operate(
        view(),
        Box::new(cosmic::iced::advanced::widget::operation::focusable::focus_next::<()>()),
    );
    native.operate(view(), Box::new(navigation::FocusReveal::default()));
}

/// Tabs until focus sits on the control in the same row as `title`, to its
/// right; panics if Tab never gets there.
fn tab_to(app: &Settings, native: &mut Native, title: &str) -> Rectangle {
    for _ in 0..80 {
        tab(native, || app.settings_view());
        let Some(focus) = native.focused(app.settings_view()) else {
            continue;
        };
        let text = native.text_bounds(app.settings_view(), title);
        let same_row = focus.y < text.y + text.height && text.y < focus.y + focus.height + 24.;
        // The control sits at the row's end, past the title and its "?".
        if same_row && focus.x > native.size.width / 2. {
            return focus;
        }
    }
    panic!("Tab never reached the control for {title:?}");
}

/// Sends a key with the pointer parked in a corner: keys must work without it.
fn key_on(app: &Settings, native: &mut Native, event: Event) -> (Vec<Message>, Status) {
    native.event(app.settings_view(), event, Point::new(1., 1.))
}

#[test]
fn ax02_tab_reaches_reduce_motion_and_space_flips_it() {
    if child("ax02_tab_reaches_reduce_motion_and_space_flips_it") {
        return;
    }
    use crate::settings::tests::{send_settings, settle_settings};
    let (mut app, initial) = Settings::init(Core::default(), Page::DesktopMotion);
    settle_settings(&mut app, initial);
    send_settings(&mut app, Message::Page(Page::DesktopMotion));
    send_settings(&mut app, Message::Resized(SIZE));
    assert_eq!(
        app.motion.current(),
        Some(&Ok(true)),
        "apps animate at first"
    );
    let mut native = Native::new(SIZE);
    let focus = tab_to(&app, &mut native, "Reduce motion");

    let (messages, status) = key_on(&app, &mut native, space());
    assert_eq!(status, Status::Captured);
    assert!(
        matches!(
            &messages[..],
            [Message::Motion(motion::Message::Animations(false))]
        ),
        "{messages:?}"
    );
    for message in messages {
        send_settings(&mut app, message);
    }
    assert_eq!(app.motion.shown(), Some(false));
    // The switch keeps focus after it redraws, so Space flips it back.
    assert_eq!(native.focused(app.settings_view()), Some(focus));
    native.capture(
        app.settings_view(),
        &cosmic::theme::active(),
        "ax02-reduce-motion-focus",
    );
    let (messages, _) = key_on(&app, &mut native, space());
    assert!(
        matches!(
            &messages[..],
            [Message::Motion(motion::Message::Animations(true))]
        ),
        "{messages:?}"
    );
    for message in messages {
        send_settings(&mut app, message);
    }
    assert_eq!(app.motion.shown(), Some(true));
    // Enter does the same as Space.
    let (messages, _) = key_on(
        &app,
        &mut native,
        named(key::Named::Enter, key::Code::Enter),
    );
    assert!(
        matches!(
            &messages[..],
            [Message::Motion(motion::Message::Animations(false))]
        ),
        "{messages:?}"
    );
}

#[test]
fn ax02_arrows_step_a_focused_slider() {
    if child("ax02_arrows_step_a_focused_slider") {
        return;
    }
    use crate::settings::tests::{send_settings, settle_settings};
    let (mut app, initial) = Settings::init(Core::default(), Page::Clock);
    settle_settings(&mut app, initial);
    send_settings(&mut app, Message::Resized(SIZE));
    let scale = app.config.clock.scale;
    let mut native = Native::new(SIZE);
    let reached = (0..80).any(|_| {
        tab(&mut native, || app.settings_view());
        let slider = native.bounds(app.settings_view(), "clock-scale");
        native
            .focused(app.settings_view())
            .is_some_and(|focus| slider.intersects(&focus))
    });
    assert!(reached, "Tab reaches the clock size slider");

    for (key, code, expected) in [
        (key::Named::ArrowRight, key::Code::ArrowRight, scale + 10),
        (key::Named::ArrowUp, key::Code::ArrowUp, scale + 20),
        (key::Named::ArrowLeft, key::Code::ArrowLeft, scale + 10),
        (key::Named::ArrowDown, key::Code::ArrowDown, scale),
    ] {
        let (messages, status) = key_on(&app, &mut native, named(key, code));
        assert_eq!(status, Status::Captured, "{key:?}");
        assert!(
            matches!(&messages[..], [Message::ClockScale(n)] if *n == expected),
            "{key:?}: {messages:?}"
        );
        for message in messages {
            send_settings(&mut app, message);
        }
        assert_eq!(app.config.clock.scale, expected);
    }
    // The clock preview grows with the size, so bring the slider back.
    native.operate(
        app.settings_view(),
        Box::new(navigation::FocusReveal::default()),
    );
    native.capture(
        app.settings_view(),
        &cosmic::Theme::dark(),
        "ax02-clock-size-focus",
    );
}

/// A slider that saves on release saves when the arrow key comes up.
#[test]
fn ax02_arrow_release_completes_a_saving_slider() {
    use super::view::focus::slider;
    use super::view::slider_completion::completed_slider;
    let view = || -> cosmic::Element<'static, Message> {
        cosmic::widget::container(
            slider(1u16..=60, 10, Message::Notice)
                .on_release(Message::SaveNotice)
                .apply(|c| completed_slider(c.into(), Message::SaveNotice)),
        )
        .padding(20)
        .into()
    };
    let mut native = Native::new(Size::new(400., 100.));
    tab(&mut native, view);
    assert!(native.focused(view()).is_some(), "Tab reaches the slider");
    let at = Point::new(1., 1.);
    let (messages, _) = native.event(
        view(),
        named(key::Named::ArrowRight, key::Code::ArrowRight),
        at,
    );
    assert!(
        matches!(&messages[..], [Message::Notice(11)]),
        "{messages:?}"
    );
    let (messages, _) = native.event(
        view(),
        released(Key::Named(key::Named::ArrowRight), key::Code::ArrowRight),
        at,
    );
    assert!(
        matches!(&messages[..], [Message::SaveNotice]),
        "{messages:?}"
    );
}

/// A dropdown opens with Space, arrows move through its choices, Enter picks
/// one and Escape closes it without picking.
#[test]
fn ax02_dropdown_opens_picks_and_closes_from_the_keyboard() {
    use super::view::focus::control;
    const CHOICES: [&str; 3] = ["Small", "Medium", "Large"];
    let view = || -> cosmic::Element<'static, Message> {
        cosmic::widget::container(control(
            Some("Size".into()),
            None,
            cosmic::widget::dropdown(&CHOICES, Some(0), |i| Message::Notice(i as u16)).into(),
        ))
        .padding(20)
        .into()
    };
    let mut native = Native::new(Size::new(400., 300.));
    let at = Point::new(1., 299.);
    tab(&mut native, view);
    assert!(native.focused(view()).is_some(), "Tab reaches the dropdown");

    let (messages, status) = native.event(view(), space(), at);
    assert!(messages.is_empty() && status == Status::Captured);
    // (Captures draw the page only, not the open list.)
    native.capture(view(), &cosmic::Theme::light(), "ax02-dropdown-focus");

    for _ in 0..2 {
        let (_, status) = native.event(
            view(),
            named(key::Named::ArrowDown, key::Code::ArrowDown),
            at,
        );
        assert_eq!(status, Status::Captured, "the open list takes the arrows");
    }
    let (messages, _) = native.event(view(), named(key::Named::Enter, key::Code::Enter), at);
    assert!(
        matches!(&messages[..], [Message::Notice(1)]),
        "second choice: {messages:?}"
    );
    // Picking closes the list: arrows go back to the page.
    let (_, status) = native.event(
        view(),
        named(key::Named::ArrowDown, key::Code::ArrowDown),
        at,
    );
    assert_eq!(status, Status::Ignored);

    // Open again, then Escape closes it with nothing picked.
    let _ = native.event(view(), space(), at);
    let (_, status) = native.event(
        view(),
        named(key::Named::ArrowDown, key::Code::ArrowDown),
        at,
    );
    assert_eq!(status, Status::Captured, "open again");
    let (messages, status) = native.event(view(), named(key::Named::Escape, key::Code::Escape), at);
    assert!(messages.is_empty() && status == Status::Captured);
    let (messages, status) = native.event(
        view(),
        named(key::Named::ArrowDown, key::Code::ArrowDown),
        at,
    );
    assert!(
        messages.is_empty() && status == Status::Ignored,
        "list closed"
    );
}

/// What a screen reader hears: every switch, slider and dropdown has a name.
#[test]
fn ax01_controls_have_accessible_names() {
    if child("ax01_controls_have_accessible_names") {
        return;
    }
    use crate::settings::tests::{send_settings, settle_settings};
    let (mut app, initial) = Settings::init(Core::default(), Page::DesktopMotion);
    settle_settings(&mut app, initial);
    send_settings(&mut app, Message::Page(Page::DesktopMotion));
    send_settings(&mut app, Message::Resized(SIZE));
    let mut native = Native::new(SIZE);
    let nodes = native.a11y(app.settings_view());
    assert!(
        nodes.iter().any(
            |(role, name, _)| *role == Role::Switch && name.as_deref() == Some("Reduce motion")
        ),
        "{nodes:#?}"
    );
    for page in [Page::Clock, Page::DesktopMotion, Page::General] {
        send_settings(&mut app, Message::Page(page));
        let nodes = native.a11y(app.settings_view());
        let unnamed: Vec<_> = nodes
            .iter()
            .filter(|(role, name, _)| {
                matches!(role, Role::Switch | Role::Slider | Role::ComboBox)
                    && name.as_deref().is_none_or(str::is_empty)
            })
            .collect();
        assert!(unnamed.is_empty(), "{page:?}: {unnamed:#?}");
    }
    send_settings(&mut app, Message::Page(Page::Clock));
    let nodes = native.a11y(app.settings_view());
    assert!(
        nodes.iter().any(|(role, _, _)| *role == Role::Slider),
        "{nodes:#?}"
    );
}
