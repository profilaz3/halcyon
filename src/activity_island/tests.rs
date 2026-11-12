// Native and state regressions for Activity island.
use super::*;
use super::reactive::NOTE_GAP;
fn app() -> Applet {
    let (mut app, _) = Applet::init(Core::default(), ());
    let (tx, _) = tokio::sync::mpsc::channel(4);
    app.commands = Some(tx);
    app.active = true;
    app.kind = Kind::Music;
    app.format_24h = false;
    app.snapshot.focus = bridge::Focus {
        key: "focus-1".into(),
        label: "Focus".into(),
        remaining_ms: 25 * 60_000,
        ready: true,
        ..Default::default()
    };
    app
}
fn click(ui: &mut crate::native_test::Native, app: &Applet, id: &str) -> Vec<Message> {
    use cosmic::iced::{Event, mouse};
    let at = ui.bounds(app.capsule(true), id).center();
    let _ = ui.event(
        app.capsule(true),
        Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)),
        at,
    );
    ui.event(
        app.capsule(true),
        Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)),
        at,
    )
    .0
}
#[test]
fn hover_grace_reversal_and_keyboard_hold() {
    let mut app = app();
    let id = window::Id::unique();
    app.popup = Some(id);
    app.motion.retarget(1., false, Instant::now());
    let _ = app.update(Message::Enter(false));
    let _ = app.update(Message::Leave(false));
    assert!(
        app.leave_deadline
            .is_some_and(|at| at > Instant::now() + Duration::from_secs(4)),
        "the panel-to-popup handoff is not a departure, only a bounded wait"
    );
    let _ = app.update(Message::Enter(true));
    assert!(app.leave_deadline.is_none(), "the popup has the pointer");
    let _ = app.update(Message::Leave(false));
    assert!(
        app.leave_deadline.is_none(),
        "a late panel leave cannot end an interaction the popup owns"
    );
    let _ = app.update(Message::Leave(true));
    assert!(
        app.leave_deadline
            .is_some_and(|at| at <= Instant::now() + LEAVE_GRACE)
    );
    let _ = app.update(Message::Enter(true));
    assert!(app.leave_deadline.is_none());
    let _ = app.update(Message::Close);
    assert_eq!(app.motion.target(), 0.);
    let _ = app.update(Message::Enter(true));
    assert_eq!(app.motion.target(), 1.);
    assert_eq!(app.popup, Some(id));
    let _ = app.update(Message::Key(keyboard::Key::Named(
        keyboard::key::Named::Tab,
    )));
    assert!(app.held);
    let _ = app.update(Message::Leave(true));
    assert!(app.leave_deadline.is_none());
    let _ = app.update(Message::Key(keyboard::Key::Named(
        keyboard::key::Named::Escape,
    )));
    assert!(!app.held);
    assert_eq!(app.motion.target(), 0.);
}
#[test]
fn focus_handoff_inside_does_not_cancel_controls_but_outside_dismisses() {
    let mut app = app();
    let id = window::Id::unique();
    app.popup = Some(id);
    app.held = true;
    app.motion.retarget(1., false, Instant::now());
    let _ = app.update(Message::Enter(true));
    let _ = app.update(Message::FocusLost(id));
    assert_eq!(app.motion.target(), 1.);
    let _ = app.update(Message::Leave(true));
    let _ = app.update(Message::FocusLost(window::Id::unique()));
    assert_eq!(app.motion.target(), 1.);
    let _ = app.update(Message::FocusLost(id));
    assert_eq!(app.motion.target(), 0.);
    assert!(!app.held);
}
#[test]
fn redraws_are_bounded_and_stop_with_the_animation() {
    use cosmic::iced::{Event, Point, Size};
    let mut app = app();
    app.popup = Some(window::Id::unique());
    let now = Instant::now();
    app.motion.retarget(1., true, now);
    let mut ui = crate::native_test::Native::new(Size::new(WIDTH, HEIGHT));
    let mut frames = 0;
    for ms in 0..1_000 {
        let (messages, _) = ui.event(
            app.capsule(true),
            Event::Window(window::Event::RedrawRequested(
                now + Duration::from_millis(ms),
            )),
            Point::new(160., 16.),
        );
        for m in messages {
            if matches!(m, Message::Frame(_)) {
                frames += 1;
                let _ = app.update(m);
            }
        }
    }
    assert!(!app.motion.active());
    assert!(
        frames > 10 && frames < 60,
        "{frames} frames for a settled transition"
    );
    let (messages, _) = ui.event(
        app.capsule(true),
        Event::Window(window::Event::RedrawRequested(now + Duration::from_secs(2))),
        Point::new(160., 16.),
    );
    assert!(!messages.iter().any(|m| matches!(m, Message::Frame(_))));
}
#[test]
fn reconnect_drops_stale_snapshot_and_reduced_motion_settles() {
    let mut app = app();
    app.popup = Some(window::Id::unique());
    app.motion.retarget(1., true, Instant::now());
    let mut snapshot = app.snapshot.clone();
    snapshot.reduced_motion = true;
    let _ = app.update(Message::Backend(ClientEvent::Snapshot(snapshot)));
    assert!(!app.motion.active());
    assert_eq!(app.motion.value, 1.);
    let _ = app.update(Message::Backend(ClientEvent::Offline));
    assert!(app.commands.is_none());
    assert_eq!(app.snapshot, Snapshot::default());
}
#[test]
fn native_controls_are_inert_when_collapsed_and_fit_scaled_surfaces() {
    use cosmic::iced::{Event, Size, mouse};
    let mut app = app();
    app.kind = Kind::Focus;
    let mut ui = crate::native_test::Native::new(Size::new(WIDTH, HEIGHT));
    app.motion.retarget(1., false, Instant::now());
    let click = ui
        .bounds(app.capsule(true), "activity-focus-action")
        .center();
    app.motion.retarget(0., false, Instant::now());
    let _ = ui.event(
        app.capsule(true),
        Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)),
        click,
    );
    let (messages, _) = ui.event(
        app.capsule(true),
        Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)),
        click,
    );
    assert!(!messages.iter().any(|m| matches!(m, Message::Control(_))));
    app.motion.retarget(1., false, Instant::now());
    let _ = ui.event(
        app.capsule(true),
        Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)),
        click,
    );
    let (messages, _) = ui.event(
        app.capsule(true),
        Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)),
        click,
    );
    assert!(
        messages
            .iter()
            .any(|m| matches!(m,Message::Control(Command::Focus{key}) if key=="focus-1"))
    );
    let root = std::env::var_os("HALCYON_ACTIVITY_EVIDENCE").map(std::path::PathBuf::from);
    for scale in [1., 1.5, 2.] {
        for (name, kind) in [
            ("focus", Kind::Focus),
            ("music", Kind::Music),
            ("calendar", Kind::Calendar),
        ] {
            app.kind = kind;
            app.snapshot.music = Some(bridge::Music {
                key: "track-1".into(),
                title: "Golden Hour".into(),
                artist: "Aura · Solstice".into(),
                playing: true,
                play: true,
                pause: true,
                next: true,
                previous: true,
            });
            let now = chrono::Utc::now().timestamp_millis();
            app.snapshot.calendar = Some(bridge::Calendar {
                key: "event-1".into(),
                title: "Design catch-up".into(),
                starts_at_ms: now + 180_000,
                ends_at_ms: now + 1_800_000,
                all_day: false,
                join: None,
            });
            let bytes = ui.render_scaled(app.capsule(true), &cosmic::Theme::dark(), scale);
            assert_eq!(
                bytes.len(),
                (WIDTH as f64 * scale) as usize * (HEIGHT as f64 * scale) as usize * 4
            );
            if let Some(root) = &root {
                std::fs::create_dir_all(root).unwrap();
                image::save_buffer(
                    root.join(format!("{name}-{scale}.png")),
                    &bytes,
                    (WIDTH as f64 * scale) as u32,
                    (HEIGHT as f64 * scale) as u32,
                    image::ColorType::Rgba8,
                )
                .unwrap();
            }
        }
    }
}

#[test]
fn active_chooser_keeps_controls_accessible_without_permanent_tabs() {
    use cosmic::iced::Size;
    let mut app = app();
    app.popup = Some(window::Id::unique());
    app.motion.retarget(1., false, Instant::now());
    app.snapshot.music = Some(music());
    app.snapshot.focus.running = true;
    app.snapshot.focus.ends_at_ms = Some(app.now_ms + 90_000);
    app.snapshot.calendar = Some(bridge::Calendar {
        key: "meeting".into(),
        title: "Design review".into(),
        starts_at_ms: app.now_ms + 180_000,
        ends_at_ms: app.now_ms + 900_000,
        all_day: false,
        join: None,
    });
    let mut ui = crate::native_test::Native::new(Size::new(WIDTH, HEIGHT));
    for (kind, id) in [
        (Kind::Focus, "focus"),
        (Kind::Calendar, "calendar"),
        (Kind::Music, "music"),
    ] {
        // Three or fewer others show as icons; the list stays one message away.
        let _ = app.update(Message::Choose);
        assert!(app.chooser && app.held);
        ui.scroll(app.capsule(true), "activity-choices", 0.);
        let target = format!("activity-choice-{id}");
        let b = ui.bounds(app.capsule(true), &target);
        if b.y + b.height > HEIGHT - 10. {
            ui.scroll(
                app.capsule(true),
                "activity-choices",
                b.y + b.height - (HEIGHT - 10.),
            );
        }
        let b = ui.bounds(app.capsule(true), &target);
        assert!(
            b.y >= 40. && b.y + b.height <= HEIGHT - 10. + 0.1,
            "chooser row must be revealed: {b:?}"
        );
        for m in click(&mut ui, &app, &target) {
            let _ = app.update(m);
        }
        assert_eq!(app.kind, kind);
        assert!(!app.chooser);
        assert!(ui.duplicate_ids(app.capsule(true)).is_empty());
    }
    let messages = click(&mut ui, &app, "activity-pause");
    assert!(
        messages
            .iter()
            .any(|m| matches!(m,Message::Control(Command::Pause{key}) if key=="track-1"))
    );
    let messages = click(&mut ui, &app, "activity-collapse");
    assert!(messages.iter().any(|m| matches!(m, Message::Close)));
}

#[test]
fn calendar_boundaries_expire_without_a_new_provider_snapshot() {
    let mut app = app();
    let base = chrono::Utc::now().timestamp_millis();
    app.now_ms = base;
    app.snapshot.calendar = Some(bridge::Calendar {
        key: "event-1".into(),
        title: "Meeting".into(),
        starts_at_ms: base + 600_000,
        ends_at_ms: base + 900_000,
        all_day: false,
        join: None,
    });
    app.snapshot.focus.running = true;
    app.snapshot.focus.ends_at_ms = Some(base + 1_500_000);
    assert_eq!(app.automatic_kind(), Kind::Focus);
    let now = Instant::now();
    app.refresh_deadline(now);
    assert_eq!(app.priority_deadline, Some(now + Duration::from_secs(300)));
    app.now_ms = base + 300_000;
    assert_eq!(app.automatic_kind(), Kind::Calendar);
    app.refresh_deadline(now);
    assert_eq!(app.priority_deadline, Some(now + Duration::from_secs(300)));
    app.now_ms = base + 600_000;
    app.refresh_deadline(now);
    assert_eq!(app.priority_deadline, Some(now + Duration::from_secs(120)));
    app.now_ms = base + 720_000;
    assert_eq!(app.automatic_kind(), Kind::Focus);
    app.now_ms = base + 900_000;
    assert_eq!(app.automatic_kind(), Kind::Focus);
    app.kind = Kind::Calendar;
    app.motion.retarget(1., false, now);
    let mut ui = crate::native_test::Native::new(cosmic::iced::Size::new(WIDTH, HEIGHT));
    assert!(
        ui.text_bounds(app.capsule(true), "No upcoming meetings")
            .width
            > 0.
    );
}

#[test]
fn a_meeting_link_becomes_the_main_action_and_empty_calendars_have_nothing_to_dismiss() {
    let mut app = app();
    let base = chrono::Utc::now().timestamp_millis();
    app.now_ms = base;
    app.kind = Kind::Calendar;
    app.motion.retarget(1., false, Instant::now());
    let mut ui = crate::native_test::Native::new(cosmic::iced::Size::new(WIDTH, HEIGHT));
    app.snapshot.calendar = Some(bridge::Calendar {
        key: "event-1".into(),
        title: "Standup".into(),
        starts_at_ms: base - 60_000,
        ends_at_ms: base + 900_000,
        all_day: false,
        join: Some("https://meet.example.com/abc".into()),
    });
    assert!(ui.text_bounds(app.capsule(true), "Join").width > 0.);
    let messages = click(&mut ui, &app, "activity-open-calendar");
    assert!(
        messages
            .iter()
            .any(|m| matches!(m, Message::Join(url) if url == "https://meet.example.com/abc"))
    );
    app.snapshot.calendar.as_mut().unwrap().join = None;
    assert!(ui.text_bounds(app.capsule(true), "Calendar settings").width > 0.);
    assert!(ui.try_bounds(app.capsule(true), "activity-dismiss-calendar").is_some());
    app.snapshot.calendar = None;
    assert!(ui.try_bounds(app.capsule(true), "activity-dismiss-calendar").is_none());
}

#[test]
fn automatic_priority_does_not_replace_an_open_activity() {
    let mut app = app();
    app.popup = Some(window::Id::unique());
    app.kind = Kind::Music;
    let mut snapshot = app.snapshot.clone();
    let now = chrono::Utc::now().timestamp_millis();
    snapshot.calendar = Some(bridge::Calendar {
        key: "event".into(),
        title: "Busy".into(),
        starts_at_ms: now - 1_000,
        ends_at_ms: now + 60_000,
        all_day: false,
        join: None,
    });
    let _ = app.update(Message::Backend(ClientEvent::Snapshot(snapshot)));
    assert_eq!(app.kind, Kind::Music);
    app.priority_deadline = Some(Instant::now());
    let _ = app.update(Message::Wake(Duration::ZERO));
    assert_eq!(app.kind, Kind::Music);
    app.snapshot.reduced_motion = true;
    let _ = app.update(Message::Close);
    assert!(app.popup.is_none());
    assert_eq!(app.kind, Kind::Calendar);
}

#[test]
fn compositor_dismissal_clears_pointer_state_and_old_deadlines() {
    let mut app = app();
    let id = window::Id::unique();
    app.popup = Some(id);
    app.held = true;
    app.hovered = true;
    app.hover_deadline = Some(Instant::now());
    app.leave_deadline = Some(Instant::now());
    let _ = app.update(Message::PopupGone(window::Id::unique()));
    assert_eq!(app.popup, Some(id));
    let _ = app.update(Message::PopupGone(id));
    assert!(!app.held && !app.hovered);
    assert!(app.hover_deadline.is_none() && app.leave_deadline.is_none());
    let _ = app.update(Message::Enter(false));
    assert!(
        app.hover_deadline.is_some(),
        "hover works again after native dismissal"
    );
}

#[test]
fn unavailable_music_and_elapsed_focus_have_no_stale_actions() {
    use cosmic::iced::{Event, Size, mouse};
    let mut app = app();
    app.motion.retarget(1., false, Instant::now());
    app.snapshot.music = Some(bridge::Music {
        key: "old-track".into(),
        title: "Saved track".into(),
        artist: String::new(),
        playing: true,
        play: true,
        pause: true,
        next: true,
        previous: true,
    });
    app.snapshot.music_problem = true;
    app.kind = Kind::Music;
    let mut ui = crate::native_test::Native::new(Size::new(WIDTH, HEIGHT));
    assert!(
        ui.text_bounds(app.capsule(true), "Player unavailable")
            .width
            > 0.
    );
    for id in ["activity-previous", "activity-pause", "activity-next"] {
        let point = ui.bounds(app.capsule(true), id).center();
        let _ = ui.event(
            app.capsule(true),
            Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)),
            point,
        );
        let (messages, _) = ui.event(
            app.capsule(true),
            Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)),
            point,
        );
        assert!(!messages.iter().any(|m| matches!(m, Message::Control(_))));
    }
    app.kind = Kind::Focus;
    app.snapshot.focus.running = true;
    app.snapshot.focus.ends_at_ms = Some(app.now_ms - 1);
    assert!(ui.text_bounds(app.capsule(true), "Focus complete").width > 0.);
    let point = ui
        .bounds(app.capsule(true), "activity-focus-action")
        .center();
    let _ = ui.event(
        app.capsule(true),
        Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)),
        point,
    );
    let (messages, _) = ui.event(
        app.capsule(true),
        Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)),
        point,
    );
    assert!(!messages.iter().any(|m| matches!(m, Message::Control(_))));
    app.snapshot.focus.running = false;
    app.snapshot.focus.done = true;
    assert!(ui.text_bounds(app.capsule(true), "Start break").width > 0.);
    app.snapshot.focus.label = "Break".into();
    assert!(ui.text_bounds(app.capsule(true), "Start focus").width > 0.);
}

#[test]
fn connection_retries_do_not_reenable_a_pending_service_start() {
    let mut app = app();
    app.commands = None;
    let _ = app.update(Message::Start);
    assert!(app.starting);
    let _ = app.update(Message::Backend(ClientEvent::Offline));
    assert!(
        app.starting,
        "socket retry must not allow another service start"
    );
    let _ = app.update(Message::SettingsOpened(Ok(())));
    assert!(
        app.starting,
        "Settings lifecycle is separate from service startup"
    );
    let _ = app.update(Message::Started(Err("Could not start Halcyon".into())));
    assert!(!app.starting);
    assert!(app.problem.is_some());
    let _ = app.update(Message::Backend(ClientEvent::Offline));
    assert_eq!(
        app.problem.as_deref(),
        Some("Could not start Halcyon"),
        "periodic connection retries must not erase an actionable failure"
    );
}

#[test]
fn queued_commands_clear_old_errors_and_disconnection_is_explicit() {
    let mut app = app();
    let (tx, mut rx) = tokio::sync::mpsc::channel(1);
    app.commands = Some(tx);
    app.problem = Some("Old failure".into());
    let _ = app.update(Message::Control(Command::Focus {
        key: "focus-1".into(),
    }));
    assert!(app.problem.is_none());
    assert!(matches!(rx.try_recv(), Ok(Command::Focus { key }) if key == "focus-1"));
    app.commands = None;
    let _ = app.update(Message::Control(Command::Focus {
        key: "focus-1".into(),
    }));
    assert_eq!(app.problem.as_deref(), Some("Halcyon is reconnecting"));
}

#[test]
fn states_fit_native_surface_with_stable_activity_targets() {
    use cosmic::iced::Size;
    let root = std::env::var_os("HALCYON_ACTIVITY_EVIDENCE").map(std::path::PathBuf::from);
    let mut app = app();
    app.popup = Some(window::Id::unique());
    app.motion.retarget(1., false, Instant::now());
    let mut ui = crate::native_test::Native::new(Size::new(WIDTH, HEIGHT));
    let target_bounds = [
        ui.bounds(app.capsule(true), "activity-title"),
        ui.bounds(app.capsule(true), "activity-collapse"),
    ];
    for b in target_bounds {
        assert!(b.x >= 0. && b.y >= 0. && b.x + b.width <= WIDTH && b.y + b.height <= HEIGHT);
    }
    let base = app.now_ms;
    for state in [
        "music-playing",
        "music-paused",
        "music-empty",
        "music-unavailable",
        "focus-ready",
        "focus-running",
        "focus-paused",
        "focus-complete",
        "break-complete",
        "calendar-empty",
        "calendar-error",
        "calendar-private",
        "calendar-now",
        "calendar-later",
        "offline",
        "starting",
        "start-failed",
    ] {
        app.kind = match state.split('-').next().unwrap() {
            "music" => Kind::Music,
            "calendar" => Kind::Calendar,
            _ => Kind::Focus,
        };
        app.snapshot.music_problem = state == "music-unavailable";
        app.snapshot.calendar_problem = state == "calendar-error";
        app.snapshot.focus.ready = state != "focus-paused";
        app.snapshot.focus.done = matches!(state, "focus-complete" | "break-complete");
        app.snapshot.focus.label = if state == "break-complete" {
            "Break"
        } else {
            "Focus"
        }
        .into();
        app.snapshot.music =
            matches!(state, "music-playing" | "music-paused").then(|| bridge::Music {
                playing: state == "music-playing",
                ..music()
            });
        app.snapshot.focus.running = state == "focus-running";
        app.snapshot.focus.ends_at_ms = (state == "focus-running").then_some(base + 900_000);
        app.snapshot.focus.remaining_ms = 1_500_000;
        if state == "calendar-private" {
            app.snapshot.calendar = Some(bridge::Calendar {
                key: "private".into(),
                title: "Busy".into(),
                starts_at_ms: base + 180_000,
                ends_at_ms: base + 1_800_000,
                all_day: false,
                join: None,
            });
        }
        if matches!(state, "calendar-now" | "calendar-later") {
            let later = state == "calendar-later";
            app.snapshot.calendar = Some(bridge::Calendar {
                key: "meeting".into(),
                title: "Quarterly planning review with the whole design team".into(),
                starts_at_ms: base + if later { 12_600_000 } else { -600_000 },
                ends_at_ms: base + if later { 16_200_000 } else { 1_200_000 },
                all_day: false,
                join: None,
            });
        }
        if state == "calendar-empty" {
            app.snapshot.calendar = None;
        }
        if state == "offline" {
            let _ = app.update(Message::Backend(ClientEvent::Offline));
        }
        app.starting = state == "starting";
        app.problem = (state == "start-failed").then(|| "Could not start Halcyon".into());
        assert!(ui.duplicate_ids(app.capsule(true)).is_empty());
        assert_eq!(
            ui.bounds(app.capsule(true), "activity-title"),
            target_bounds[0]
        );
        assert_eq!(
            ui.bounds(app.capsule(true), "activity-collapse"),
            target_bounds[1]
        );
        for (name, theme) in [
            ("dark", cosmic::Theme::dark()),
            ("light", cosmic::Theme::light()),
        ] {
            app.core.applet.background = if name == "light" {
                cosmic::applet::cosmic_panel_config::CosmicPanelBackground::Light
            } else {
                cosmic::applet::cosmic_panel_config::CosmicPanelBackground::Dark
            };
            let bytes = ui.render_scaled(app.capsule(true), &theme, 2.);
            for bounds in &target_bounds {
                let mut readable = 0;
                for y in (bounds.y * 2.) as usize..((bounds.y + bounds.height) * 2.) as usize {
                    for x in (bounds.x * 2.) as usize..((bounds.x + bounds.width) * 2.) as usize {
                        let i = (y * WIDTH as usize * 2 + x) * 4;
                        if if name == "light" {
                            bytes[i..i + 3].iter().any(|v| *v < 150)
                        } else {
                            bytes[i..i + 3].iter().any(|v| *v > 120)
                        } {
                            readable += 1;
                        }
                    }
                }
                assert!(
                    readable > 12,
                    "{state}/{name}: activity label must contrast with the panel palette"
                );
            }
            if let Some(root) = &root {
                std::fs::create_dir_all(root).unwrap();
                image::save_buffer(
                    root.join(format!("{state}-{name}.png")),
                    &bytes,
                    WIDTH as u32 * 2,
                    HEIGHT as u32 * 2,
                    image::ColorType::Rgba8,
                )
                .unwrap();
            }
        }
    }
}

#[test]
fn pointer_leave_retires_stale_native_cursor_until_reentry() {
    use cosmic::iced::{Event, Size, mouse};
    let mut app = app();
    app.kind = Kind::Focus;
    app.motion.retarget(1., false, Instant::now());
    let mut ui = crate::native_test::Native::new(Size::new(WIDTH, HEIGHT));
    let point = ui
        .bounds(app.capsule(true), "activity-focus-action")
        .center();
    let _ = ui.event(
        app.capsule(true),
        Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)),
        point,
    );
    // This is the exact cursor shape observed from the real COSMIC proxy:
    // CursorLeft arrives while the runtime still supplies the last inside point.
    let _ = ui.event(
        app.capsule(true),
        Event::Mouse(mouse::Event::CursorLeft),
        point,
    );
    let _ = ui.event(
        app.capsule(true),
        Event::Window(window::Event::RedrawRequested(Instant::now())),
        point,
    );
    let (messages, _) = ui.event(
        app.capsule(true),
        Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)),
        point,
    );
    assert!(
        !messages
            .iter()
            .any(|m| matches!(m, Message::Control(_) | Message::Enter(_))),
        "outside release must not use stale inside coordinates"
    );
    let _ = ui.event(
        app.capsule(true),
        Event::Mouse(mouse::Event::CursorMoved { position: point }),
        point,
    );
    let _ = ui.event(
        app.capsule(true),
        Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)),
        point,
    );
    let (messages, _) = ui.event(
        app.capsule(true),
        Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)),
        point,
    );
    assert!(
        messages
            .iter()
            .any(|m| matches!(m, Message::Control(Command::Focus { .. })))
    );
}

fn music() -> bridge::Music {
    bridge::Music {
        key: "track-1".into(),
        title: "Golden Hour".into(),
        artist: "Aura · Solstice".into(),
        playing: true,
        play: true,
        pause: true,
        next: true,
        previous: true,
    }
}

#[test]
fn flick_while_the_popup_maps_cannot_leave_it_stuck_open() {
    let mut app = app();
    let _ = app.update(Message::Enter(false));
    let id = window::Id::unique();
    app.popup = Some(id);
    app.motion.retarget(1., false, Instant::now());
    app.hover_deadline = None;
    // The pointer went while mapping: the popup never receives any pointer event.
    let _ = app.update(Message::Leave(false));
    assert!(!app.hovered);
    let deadline = app.leave_deadline.expect("a bounded wait");
    app.leave_deadline = Some(Instant::now());
    let _ = app.update(Message::Wake(Duration::ZERO));
    assert_eq!(app.motion.target(), 0., "the unentered popup closes");
    assert!(deadline > Instant::now() + Duration::from_secs(4));
    // A pinned view is the user's choice and has no such wait.
    let mut app = self::app();
    app.popup = Some(id);
    app.held = true;
    let _ = app.update(Message::Leave(false));
    assert!(app.leave_deadline.is_none());
}

#[test]
fn escape_stays_dismissed_until_the_pointer_leaves_the_capsule() {
    let mut app = app();
    app.snapshot.reduced_motion = true;
    let id = window::Id::unique();
    app.popup = Some(id);
    app.motion.retarget(1., false, Instant::now());
    let _ = app.update(Message::Enter(true));
    let _ = app.update(Message::Key(keyboard::Key::Named(
        keyboard::key::Named::Escape,
    )));
    assert!(app.popup.is_none());
    let _ = app.update(Message::Enter(false));
    assert!(
        app.hover_deadline.is_none(),
        "a small motion must not reopen what Escape just closed"
    );
    let _ = app.update(Message::Leave(false));
    let _ = app.update(Message::Enter(false));
    assert!(app.hover_deadline.is_some(), "a fresh hover opens again");
    // Pointer elsewhere: there is nothing to suppress.
    let mut app = self::app();
    app.snapshot.reduced_motion = true;
    app.popup = Some(id);
    let _ = app.update(Message::Key(keyboard::Key::Named(
        keyboard::key::Named::Escape,
    )));
    let _ = app.update(Message::Enter(false));
    assert!(app.hover_deadline.is_some());
}

#[test]
fn pressing_the_closing_capsule_reopens_and_keeps_it() {
    let mut app = app();
    app.popup = Some(window::Id::unique());
    app.motion.retarget(1., false, Instant::now());
    let _ = app.update(Message::Close);
    assert_eq!(app.motion.target(), 0.);
    let _ = app.update(Message::Hold);
    assert_eq!(app.motion.target(), 1.);
    assert!(app.held);
}

#[test]
fn opening_settings_dismisses_the_popup_and_keeps_an_earlier_failure() {
    let mut app = app();
    app.popup = Some(window::Id::unique());
    app.motion.retarget(1., false, Instant::now());
    app.held = true;
    let _ = app.update(Message::Settings("calendar"));
    assert_eq!(app.motion.target(), 0.);
    assert!(!app.held);
    app.problem = Some("Could not start Halcyon".into());
    let _ = app.update(Message::SettingsOpened(Ok(())));
    assert_eq!(app.problem.as_deref(), Some("Could not start Halcyon"));
    let _ = app.update(Message::SettingsOpened(Err(
        "Could not open Settings".into()
    )));
    assert_eq!(app.problem.as_deref(), Some("Could not open Settings"));
}

#[test]
fn without_a_connection_no_activity_is_selected_or_selectable() {
    use cosmic::iced::{Event, Size, mouse};
    let mut app = app();
    let _ = app.update(Message::Backend(ClientEvent::Offline));
    app.motion.retarget(1., false, Instant::now());
    let mut ui = crate::native_test::Native::new(Size::new(WIDTH, HEIGHT));
    assert!(app.eligible_kinds().is_empty());
    assert!(
        ui.try_bounds(app.capsule(true), "activity-others")
            .is_none()
    );
    for kind in [Kind::Music, Kind::Focus, Kind::Calendar] {
        let before = app.kind;
        let _ = app.update(Message::Select(kind));
        assert_eq!(app.kind, before, "disconnected selections are rejected");
    }
    assert_eq!(app.content().title, "Halcyon isn't running");
    // The recovery stays reachable, and a failure names itself in the title.
    let start = ui.bounds(app.capsule(true), "activity-start").center();
    let _ = ui.event(
        app.capsule(true),
        Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)),
        start,
    );
    let (messages, _) = ui.event(
        app.capsule(true),
        Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)),
        start,
    );
    assert!(messages.iter().any(|m| matches!(m, Message::Start)));
    let _ = app.update(Message::Started(Err("Could not start Halcyon".into())));
    assert_eq!(app.content().title, "Could not start Halcyon");
}

#[test]
fn content_says_one_thing_once_in_simple_words() {
    let mut app = app();
    let now = chrono::Utc::now().timestamp_millis();
    app.now_ms = now;
    app.kind = Kind::Music;
    app.snapshot.music = Some(music());
    let playing = app.content();
    assert_eq!(
        (playing.cue.as_str(), playing.meta.as_str()),
        ("", "Aura · Solstice")
    );
    app.snapshot.music.as_mut().unwrap().playing = false;
    assert_eq!(app.content().cue, "Paused");
    app.kind = Kind::Focus;
    let ready = app.content();
    assert_eq!(
        (
            ready.title.as_str(),
            ready.cue.as_str(),
            ready.meta.as_str()
        ),
        ("25:00", "Ready", "Focus session")
    );
    app.snapshot.focus.ready = false;
    assert_eq!(app.content().cue, "Paused");
    app.snapshot.focus.running = true;
    app.snapshot.focus.remaining_ms = 1_500_000;
    app.snapshot.focus.ends_at_ms = Some(now + 750_000);
    let running = app.content();
    assert_eq!(running.cue, "Focus");
    assert!(running.meta.starts_with("Ends "));
    assert!((running.progress.unwrap() - 0.5).abs() < 0.01);
    app.snapshot.focus.running = false;
    app.snapshot.focus.ends_at_ms = None;
    app.snapshot.focus.done = true;
    let complete = app.content();
    assert_eq!(complete.title, "Focus complete");
    assert!(
        !complete.meta.contains("Focus") && complete.cue.is_empty(),
        "completion does not repeat its own name"
    );
    app.snapshot.focus.done = false;
    app.kind = Kind::Calendar;
    app.snapshot.calendar = Some(bridge::Calendar {
        key: "event".into(),
        title: "Design catch-up".into(),
        starts_at_ms: now + 7 * 3_600_000,
        ends_at_ms: now + 8 * 3_600_000,
        all_day: false,
        join: None,
    });
    assert_eq!(
        app.content().cue,
        "In 7 h",
        "no count of hundreds of minutes"
    );
    assert_eq!(calendar_cue(now + 59 * 60_000, now), "In 59 min");
    assert_eq!(calendar_cue(now + 60 * 60_000, now), "In 1 h");
    assert_eq!(calendar_cue(now, now), "Now");
    app.snapshot.calendar.as_mut().unwrap().starts_at_ms = now - 600_000;
    app.snapshot.calendar.as_mut().unwrap().ends_at_ms = now + 600_000;
    assert!((app.content().progress.unwrap() - 0.5).abs() < 0.01);
}

#[test]
fn event_times_follow_the_panel_clock_format() {
    use chrono::{Local, TimeZone};
    let at = |h, m| {
        Local
            .with_ymd_and_hms(2026, 11, 3, h, m, 0)
            .unwrap()
            .timestamp_millis()
    };
    assert_eq!(clock_range(at(14, 42), at(15, 9), true), "14:42–15:09");
    assert_eq!(clock_range(at(14, 42), at(15, 9), false), "2:42–3:09 PM");
    assert_eq!(
        clock_range(at(11, 30), at(12, 15), false),
        "11:30 AM–12:15 PM"
    );
    assert_eq!(clock_time(at(9, 5), false), "9:05 AM");
}

#[test]
fn layout_shares_edges_and_controls_never_overlap() {
    use cosmic::iced::{Rectangle, Size};
    let mut app = app();
    app.motion.retarget(1., false, Instant::now());
    app.snapshot.music = Some(music());
    let mut ui = crate::native_test::Native::new(Size::new(WIDTH, HEIGHT));
    let right = |b: Rectangle| b.x + b.width;
    let separate = |a: Rectangle, b: Rectangle| a.intersection(&b).is_none_or(|i| i.area() < 1.);
    for (kind, action) in [
        (Kind::Music, "activity-next"),
        (Kind::Focus, "activity-focus-action"),
        (Kind::Calendar, "activity-open-calendar"),
    ] {
        app.kind = kind;
        let collapse = ui.bounds(app.capsule(true), "activity-collapse");
        let act = ui.bounds(app.capsule(true), action);
        assert!((right(collapse) - (WIDTH - 14.)).abs() < 0.5);
        assert!(act.x >= 14. && right(act) <= WIDTH - 14.);
        assert!(separate(collapse, act));
        assert!(act.y >= collapse.y + collapse.height && act.y + act.height <= HEIGHT - 8.);
        assert!(act.height >= 30.);
    }
    app.kind = Kind::Music;
    app.snapshot.music.as_mut().unwrap().title = "A long track title ".repeat(8);
    app.snapshot.music.as_mut().unwrap().artist = "An artist with a very long name ".repeat(5);
    let title = ui.bounds(app.capsule(true), "activity-title");
    let collapse = ui.bounds(app.capsule(true), "activity-collapse");
    assert!(separate(title, collapse) && right(title) <= collapse.x);
    let details = ui.text_bounds(
        app.capsule(true),
        &"An artist with a very long name ".repeat(5),
    );
    let previous = ui.bounds(app.capsule(true), "activity-previous");
    assert!(details.y + details.height <= previous.y);
    assert!(right(details) <= WIDTH - 14.);
    let pause = ui.bounds(app.capsule(true), "activity-pause");
    let next = ui.bounds(app.capsule(true), "activity-next");
    assert!(separate(previous, pause) && separate(pause, next));
    assert_eq!(pause.width, 44.);
    assert!(ui.duplicate_ids(app.capsule(true)).is_empty());
}

#[test]
fn compact_capsule_and_opening_popup_match_so_nothing_jumps() {
    use cosmic::iced::Size;
    let root = std::env::var_os("HALCYON_ACTIVITY_EVIDENCE").map(std::path::PathBuf::from);
    let mut app = app();
    let now = chrono::Utc::now().timestamp_millis();
    app.now_ms = now;
    let scale = 2.;
    let mut ui = crate::native_test::Native::new(Size::new(WIDTH, HEIGHT));
    let rows = COMPACT_HEIGHT as usize * 2;
    let stride = WIDTH as usize * 2 * 4;
    let states: [(&str, Kind); 6] = [
        ("music-playing", Kind::Music),
        ("music-paused", Kind::Music),
        ("focus-running", Kind::Focus),
        ("focus-ready", Kind::Focus),
        ("calendar-soon", Kind::Calendar),
        ("offline", Kind::Focus),
    ];
    for (name, kind) in states {
        app.kind = kind;
        app.snapshot.music = Some(bridge::Music {
            playing: name != "music-paused",
            ..music()
        });
        app.snapshot.focus.running = name == "focus-running";
        app.snapshot.focus.ends_at_ms = (name == "focus-running").then_some(now + 900_000);
        app.snapshot.calendar = Some(bridge::Calendar {
            key: "event".into(),
            title: "Design catch-up".into(),
            starts_at_ms: now + 180_000,
            ends_at_ms: now + 1_800_000,
            all_day: false,
            join: None,
        });
        if name == "offline" {
            let _ = app.update(Message::Backend(ClientEvent::Offline));
        }
        app.motion.retarget(0., false, Instant::now());
        let compact = ui.render_scaled(app.capsule(false), &cosmic::Theme::dark(), scale);
        let opening = ui.render_scaled(app.capsule(true), &cosmic::Theme::dark(), scale);
        let width = (app.slot_width() * scale as f32) as usize * 4;
        let offset = ((WIDTH - app.slot_width()) / 2. * scale as f32) as usize * 4;
        for row in 0..rows {
            let a = &compact[row * stride..row * stride + width];
            let b = &opening[row * stride + offset..row * stride + offset + width];
            assert!(
                a == b,
                "{name}: first popup frame differs from the centered panel slot at row {row}"
            );
        }
        if let Some(root) = &root {
            std::fs::create_dir_all(root).unwrap();
            // Show the capsule on the panel it sits in, not on transparency.
            let panel = Palette::for_panel(&app.core.applet).surface;
            let bg = [panel.r, panel.g, panel.b].map(|c| c * 255.);
            let mut shown = compact[..rows * stride].to_vec();
            for px in shown.chunks_exact_mut(4) {
                let a = px[3] as f32 / 255.;
                for i in 0..3 {
                    px[i] = (px[i] as f32 * a + bg[i] * (1. - a)).round() as u8;
                }
                px[3] = 255;
            }
            image::save_buffer(
                root.join(format!("compact-{name}.png")),
                &shown,
                WIDTH as u32 * 2,
                rows as u32,
                image::ColorType::Rgba8,
            )
            .unwrap();
        }
    }
    let _ = app.update(Message::Backend(ClientEvent::Connected(
        tokio::sync::mpsc::channel(1).0,
    )));
    app.snapshot.focus.key = "focus-1".into();
    app.kind = Kind::Music;
    if let Some(root) = &root {
        for percent in [15, 35, 55, 70, 85, 100] {
            app.motion.value = percent as f32 / 100.;
            let bytes = ui.render_scaled(app.capsule(true), &cosmic::Theme::dark(), scale);
            image::save_buffer(
                root.join(format!("transition-{percent:03}.png")),
                &bytes,
                WIDTH as u32 * 2,
                HEIGHT as u32 * 2,
                image::ColorType::Rgba8,
            )
            .unwrap();
        }
    }
}

#[test]
fn reduced_or_other_edges_open_complete_and_rows_stay_stable_across_activities() {
    use cosmic::iced::Size;
    let mut app = app();
    app.supported = false;
    app.popup = Some(window::Id::unique());
    let _ = app.update(Message::Open);
    assert_eq!(
        app.motion.value, 1.,
        "no capsule growth outside the top edge"
    );
    assert!(!app.motion.active());
    app.supported = true;
    let mut ui = crate::native_test::Native::new(Size::new(WIDTH, HEIGHT));
    app.snapshot.music = Some(music());
    let mut pin = None;
    for kind in [Kind::Music, Kind::Focus, Kind::Calendar] {
        app.kind = kind;
        let at = ui.bounds(app.capsule(true), "activity-collapse");
        assert_eq!(*pin.get_or_insert(at), at, "the pin never moves");
    }
    // Pinning cannot shift the title, whatever label it shows.
    app.kind = Kind::Music;
    let title = ui.bounds(app.capsule(true), "activity-title");
    app.held = true;
    assert_eq!(ui.bounds(app.capsule(true), "activity-title"), title);
    if let Some(root) = std::env::var_os("HALCYON_ACTIVITY_EVIDENCE").map(std::path::PathBuf::from)
    {
        std::fs::create_dir_all(&root).unwrap();
        for (name, theme) in [
            ("dark", cosmic::Theme::dark()),
            ("light", cosmic::Theme::light()),
        ] {
            app.core.applet.background = if name == "light" {
                cosmic::applet::cosmic_panel_config::CosmicPanelBackground::Light
            } else {
                cosmic::applet::cosmic_panel_config::CosmicPanelBackground::Dark
            };
            for (id, label) in [("activity-collapse", "pin"), ("activity-pause", "pause")] {
                let at = ui.bounds(app.capsule(true), id).center();
                let _ = ui.render_hover(app.capsule(true), &theme, at);
                std::thread::sleep(Duration::from_millis(550));
                let _ = ui.render_hover(app.capsule(true), &theme, at);
                let bytes = ui.render_hover(app.capsule(true), &theme, at);
                image::save_buffer(
                    root.join(format!("tooltip-{label}-{name}.png")),
                    &bytes,
                    WIDTH as u32,
                    HEIGHT as u32,
                    image::ColorType::Rgba8,
                )
                .unwrap();
            }
        }
    }
}

#[test]
fn crossing_the_capsule_waits_for_rest_up_to_a_limit() {
    use cosmic::iced::{Event, Point, Size, mouse};
    let mut app = app();
    let mut ui = crate::native_test::Native::new(Size::new(WIDTH, HEIGHT));
    let moved = |x: f32| {
        let position = Point::new(x, 16.);
        (
            Event::Mouse(mouse::Event::CursorMoved { position }),
            position,
        )
    };
    // Outside the panel-owned island: inert. The whole visible island now
    // receives hover, including the space around a short title.
    let (event, at) = moved(-5.);
    let (messages, _) = ui.event(app.capsule(false), event, at);
    assert!(messages.is_empty(), "outside the island is inert");
    let _ = app.update(Message::Enter(false));
    let first = app.hover_deadline.expect("waiting to open");
    std::thread::sleep(Duration::from_millis(20));
    let (event, at) = moved(160.);
    let (messages, _) = ui.event(app.capsule(false), event.clone(), at);
    let (messages2, _) = ui.event(app.capsule(false), event, at);
    let reported: Vec<_> = messages.into_iter().chain(messages2).collect();
    assert!(reported.iter().any(|m| matches!(m, Message::Moved)));
    for message in reported {
        let _ = app.update(message);
    }
    assert!(
        app.hover_deadline.unwrap() > first,
        "motion postpones opening"
    );
    assert!(
        app.hover_deadline.unwrap() <= app.hover_cap,
        "but never beyond the limit"
    );
    let cap = Instant::now() + Duration::from_millis(10);
    app.hover_cap = cap;
    let _ = app.update(Message::Moved);
    assert_eq!(app.hover_deadline, Some(cap), "steady motion still opens");
    // Nothing is reported once no longer waiting.
    let _ = app.update(Message::Leave(false));
    assert!(app.hover_deadline.is_none());
    let (event, at) = moved(161.);
    let (messages, _) = ui.event(app.capsule(false), event, at);
    assert!(!messages.iter().any(|m| matches!(m, Message::Moved)));
}

#[test]
fn escape_collapse_cannot_be_reversed_by_pointer_reentry() {
    let mut app = app();
    app.supported = true;
    app.popup = Some(window::Id::unique());
    app.motion.retarget(1., false, Instant::now());
    let _ = app.update(Message::Enter(true));
    let _ = app.update(Message::Key(keyboard::Key::Named(
        keyboard::key::Named::Escape,
    )));
    assert!(app.motion.active());
    assert_eq!(app.motion.target(), 0.);
    // Crossing the shrinking outline can produce another Enter before the
    // popup has been destroyed. Escape still wins over that pointer motion.
    let _ = app.update(Message::Leave(true));
    let _ = app.update(Message::Enter(true));
    assert_eq!(
        app.motion.target(),
        0.,
        "Escape must keep closing while animated"
    );
    // A deliberate press is different from pointer motion and still works.
    let _ = app.update(Message::Hold);
    assert_eq!(app.motion.target(), 1.);
    assert!(app.held);
    assert!(app.hover_block.is_none());
}

#[test]
fn standard_panel_closes_activity_and_reenable_preserves_the_applet() {
    let (fresh, _) = Applet::init(Core::default(), ());
    assert!(
        !fresh.active,
        "nothing is exposed before host observation"
    );
    let mut app = app();
    app.popup = Some(window::Id::unique());
    app.held = true;
    app.motion.retarget(1., false, Instant::now());
    let _ = app.update(Message::PanelMode(false));
    assert!(!app.active);
    assert!(app.popup.is_none());
    assert!(app.commands.is_none());
    assert!(!app.held);
    assert!(!app.motion.active());
    assert_eq!(app.motion.value, 0.);
    let _ = app.update(Message::Hold);
    let _ = app.update(Message::Open);
    let _ = app.update(Message::Enter(false));
    assert!(app.popup.is_none());
    assert!(app.hover_deadline.is_none());
    let (tx, _) = tokio::sync::mpsc::channel(4);
    let _ = app.update(Message::Backend(ClientEvent::Connected(tx)));
    assert!(
        app.commands.is_none(),
        "a late client cannot revive a disabled applet"
    );
    let _ = app.update(Message::PanelMode(true));
    assert!(app.active);
    assert!(app.popup.is_none(), "re-enable is quiet until interaction");
}

#[test]
fn panel_palette_stays_readable_and_compact_surface_is_unpainted() {
    use cosmic::applet::cosmic_panel_config::CosmicPanelBackground as Background;
    let mut app = app();
    app.snapshot.music = Some(music());
    let mut ui = crate::native_test::Native::new(cosmic::iced::Size::new(WIDTH, HEIGHT));
    for (name, background) in [
        ("dark", Background::Dark),
        ("light", Background::Light),
        ("colored", Background::Color([0.25, 0.4, 0.55])),
    ] {
        app.core.applet.background = background;
        let palette = Palette::for_panel(&app.core.applet);
        assert!(contrast(palette.ink, palette.surface) >= 4.5);
        for kind in [Kind::Music, Kind::Focus, Kind::Calendar] {
            assert!(contrast(palette.accent(kind), palette.surface) >= 4.5);
        }
        app.motion.retarget(0., false, Instant::now());
        let theme = cosmic::Theme::dark();
        let compact = ui.render(app.capsule(false), &theme);
        let first = ui.render(app.capsule(true), &theme);
        let stride = WIDTH as usize * 4;
        let width = app.slot_width() as usize * 4;
        let offset = ((WIDTH - app.slot_width()) / 2.) as usize * 4;
        for row in 0..HEIGHT as usize {
            assert!(
                compact[row * stride..row * stride + width]
                    == first[row * stride + offset..row * stride + offset + width],
                "first popup frame must not repaint the host: row {row}"
            );
        }
        let pixel = |data: &[u8], x: usize, y: usize| data[(y * WIDTH as usize + x) * 4 + 3];
        assert_eq!(
            pixel(&compact, WIDTH as usize / 2, 29),
            0,
            "resting panel supplies the background"
        );
        app.motion.retarget(1., false, Instant::now());
        let open = ui.render(app.capsule(true), &theme);
        assert_eq!(
            pixel(&open, WIDTH as usize / 2, 74),
            255,
            "expanded text has an opaque reading surface"
        );
        if let Some(root) =
            std::env::var_os("HALCYON_ACTIVITY_EVIDENCE").map(std::path::PathBuf::from)
        {
            std::fs::create_dir_all(&root).unwrap();
            for (state, pixels) in [("compact", compact), ("open", open)] {
                image::save_buffer(
                    root.join(format!("panel-{name}-{state}.png")),
                    &pixels,
                    WIDTH as u32,
                    HEIGHT as u32,
                    image::ColorType::Rgba8,
                )
                .unwrap();
            }
        }
    }
}

#[test]
fn panel_outer_frame_preserves_title_and_controls_at_all_supported_sizes() {
    use cosmic::iced::Size;
    let mut app = app();
    app.snapshot.music = Some(music());
    for (height, padding, attached) in [32., 40., 56., 64., 80.]
        .into_iter()
        .flat_map(|h| [0., 4., 16.].map(|p| (h, p)))
        .flat_map(|(h, p)| [(h, p, false), (h, p, true)])
    {
        {
            app.panel_frame = host::Frame {
                height,
                padding,
                radius: 16.,
                attached,
                opacity: 0.8,
            };
            let frame = app.frame();
            let mut main = crate::native_test::Native::new(Size::new(WIDTH, height));
            let mut popup = crate::native_test::Native::new(Size::new(
                frame.popup_width(),
                frame.popup_height(),
            ));
            app.motion.retarget(0., false, Instant::now());
            let a = main.bounds(app.capsule(false), "activity-title");
            let b = popup.bounds(app.capsule(true), "activity-title");
            assert_eq!(
                a.x,
                b.x - (frame.popup_width() - app.slot_width()) / 2.,
                "popup's centered anchor and negative host offset keep the title in place"
            );
            assert_eq!(a.y, b.y - padding);
            assert_eq!(a.size(), b.size());
            app.motion.retarget(1., false, Instant::now());
            for id in [
                "activity-title",
                "activity-collapse",
                "activity-pause",
                "activity-next",
            ] {
                let b = popup.bounds(app.capsule(true), id);
                assert!(b.x >= padding && b.y >= padding);
                assert!(b.x + b.width <= frame.popup_width() - padding + 0.1);
                assert!(b.y + b.height <= frame.popup_height() + 0.1);
                if attached && id != "activity-title" && id != "activity-collapse" {
                    assert!(
                        b.y >= frame.bar(),
                        "{id} hangs below a Standard bar, not over its neighbours"
                    );
                }
            }
            if attached {
                let open = popup.bounds(app.capsule(true), "activity-title");
                assert_eq!((open.x, open.y), (b.x, b.y), "the title stays in the bar");
                let pin = popup.bounds(app.capsule(true), "activity-collapse");
                assert!(pin.y + pin.height <= frame.bar() + 0.1);
            }
        }
    }
}

#[test]
fn panel_geometry_change_closes_old_popup_before_using_new_anchor() {
    let mut app = app();
    app.popup = Some(window::Id::unique());
    app.motion.retarget(1., false, Instant::now());
    let frame = host::Frame {
        height: 56.,
        padding: 8.,
        radius: 24.,
        attached: false,
        opacity: 1.,
    };
    let _ = app.update(Message::PanelObserved(host::Observation {
        reveals_allowed: true,
        enabled: true,
        frame,
    }));
    assert!(app.popup.is_none());
    assert!(!app.motion.active());
    assert_eq!(app.frame(), frame);
    assert!(app.active);
}

#[test]
fn popup_owns_the_title_during_motion_without_a_second_copy_underneath() {
    let mut app = app();
    app.snapshot.music = Some(music());
    let mut ui = crate::native_test::Native::new(cosmic::iced::Size::new(WIDTH, HEIGHT));
    app.popup = Some(window::Id::unique());
    for progress in [0.1, 0.5, 1.] {
        app.motion.value = progress;
        let main = ui.render(app.capsule(false), &cosmic::Theme::dark());
        assert!(
            main.chunks_exact(4).all(|p| p[3] == 0),
            "the popup carries the only title while moving or open"
        );
        let popup = ui.render(app.capsule(true), &cosmic::Theme::dark());
        assert!(popup.chunks_exact(4).any(|p| p[3] > 0));
    }
    app.motion.value = 0.;
    let restored = ui.render(app.capsule(false), &cosmic::Theme::dark());
    assert!(
        restored.chunks_exact(4).any(|p| p[3] > 0),
        "restore the resting title before removing the popup"
    );
}

#[test]
fn reactive_popup_promotion_waits_for_teardown_and_cancellation_cannot_reopen() {
    let mut a = app();
    let old = window::Id::unique();
    a.popup = Some(old);
    a.motion.retarget(1., false, Instant::now());
    a.selected_source = Some("original-player".into());
    let _ = a.update(Message::Hold);
    assert_eq!(a.promote_popup, Some(old));
    assert_eq!(a.popup, Some(old));
    let _ = a.update(Message::FocusLost(old));
    assert_eq!(a.motion.target(), 1.);
    let _ = a.update(Message::PopupGone(old));
    assert!(a.held && a.popup.is_some() && a.popup != Some(old));
    assert!(a.promote_popup.is_none());
    assert_eq!(a.selected_source.as_deref(), Some("original-player"));
    let current = a.popup;
    let _ = a.update(Message::PopupGone(old));
    assert_eq!(a.popup, current, "stale teardown cannot close replacement");
    for standard in [false, true] {
        let mut a = app();
        a.popup = Some(old);
        a.motion.retarget(1., false, Instant::now());
        let _ = a.update(Message::Hold);
        let _ = a.update(if standard {
            Message::PanelMode(false)
        } else {
            Message::Close
        });
        let _ = a.update(Message::PopupGone(old));
        assert!(a.popup.is_none() && a.promote_popup.is_none() && !a.held);
    }
}

#[test]
fn slim_compact_identity_is_centered_and_popup_retains_full_transport_targets() {
    use cosmic::iced::Size;
    let mut a = app();
    a.snapshot.music = Some(music());
    let mut ui = crate::native_test::Native::new(Size::new(WIDTH, HEIGHT));
    let title = ui.text_bounds(a.capsule(false), "Golden Hour");
    let group_center = (title.x - 24. + title.x + title.width) / 2.;
    assert!(
        (group_center - a.slot_width() / 2.).abs() < 1.,
        "media identity: {title:?}, slot {}",
        a.slot_width()
    );
    assert_eq!(a.slot_width(), 220.);
    a.kind = Kind::Idle;
    a.snapshot.music = None;
    // The quiet state shows its icon too, so the icon and title center as one group.
    let title = ui.text_bounds(a.capsule(false), "Activities");
    assert!(
        ((title.x - 24. + title.x + title.width) / 2. - a.slot_width() / 2.).abs() < 1.,
        "quiet identity: {title:?}"
    );
    a.kind = Kind::Music;
    a.snapshot.music = Some(music());
    a.popup = Some(window::Id::unique());
    a.motion.retarget(1., false, Instant::now());
    for id in ["activity-previous", "activity-pause", "activity-next"] {
        let b = ui.bounds(a.capsule(true), id);
        assert_eq!(b.width, 44.);
        assert_eq!(b.height, 44.);
        assert!(b.y + b.height <= HEIGHT - 8.);
    }
    assert_eq!(a.frame().popup_width(), 296.);
    assert_eq!(a.frame().popup_height(), 128.);
}

#[test]
fn slim_identity_alignment_moves_continuously_during_expansion() {
    let mut a = app();
    a.snapshot.music = Some(music());
    let mut ui = crate::native_test::Native::new(cosmic::iced::Size::new(WIDTH, HEIGHT));
    let mut previous: Option<f32> = None;
    for step in 0..=100 {
        a.motion.value = step as f32 / 100.;
        let b = ui.text_bounds(a.capsule(true), "Golden Hour");
        if let Some(x) = previous {
            assert!(
                (b.x - x).abs() < 2.,
                "title jumped at progress {}: {:?} to {}",
                a.motion.value,
                x,
                b.x
            );
        }
        previous = Some(b.x);
    }
}

#[test]
fn icon_glyphs_are_centered_in_their_targets_and_transport_row() {
    use cosmic::applet::cosmic_panel_config::CosmicPanelBackground;
    use cosmic::iced::Size;
    let mut a = app();
    a.popup = Some(window::Id::unique());
    a.motion.retarget(1., false, Instant::now());
    a.snapshot.music = Some(music());
    let root = std::env::var_os("HALCYON_ACTIVITY_EVIDENCE").map(std::path::PathBuf::from);
    for (name, theme, background) in [
        ("dark", cosmic::Theme::dark(), CosmicPanelBackground::Dark),
        (
            "light",
            cosmic::Theme::light(),
            CosmicPanelBackground::Light,
        ),
    ] {
        a.core.applet.background = background;
        for playing in [true, false] {
            a.snapshot.music.as_mut().unwrap().playing = playing;
            let primary = if playing {
                "activity-pause"
            } else {
                "activity-play"
            };
            for disabled in [false, true] {
                a.snapshot.music_problem = disabled;
                for scale in [1., 1.5, 2.] {
                    let mut ui = crate::native_test::Native::new(Size::new(WIDTH, HEIGHT));
                    let targets = ["activity-previous", primary, "activity-next"]
                        .map(|id| ui.bounds(a.capsule(true), id));
                    assert_eq!(targets[1].center().x, WIDTH / 2.);
                    assert_eq!(
                        targets[1].center().x - targets[0].center().x,
                        targets[2].center().x - targets[1].center().x
                    );
                    let pixels = ui.render_scaled(a.capsule(true), &theme, scale);
                    let width = (WIDTH as f64 * scale) as usize;
                    let at = |x: usize, y: usize| &pixels[(y * width + x) * 4..][..3];
                    for (id, bounds) in ["activity-previous", primary, "activity-next"]
                        .into_iter()
                        .zip(targets)
                        .chain(std::iter::once((
                            "activity-collapse",
                            ui.bounds(a.capsule(true), "activity-collapse"),
                        )))
                    {
                        let extent = if id == "activity-collapse" { 28. } else { 44. };
                        assert_eq!(bounds.size(), Size::new(extent, extent));
                        // Inspect the rendered glyph, not just the correctly centered
                        // hitbox: the old native row left its icon four pixels off.
                        let s = scale as f32;
                        let base = at(
                            (bounds.center().x * s) as usize,
                            ((bounds.y + 4.) * s) as usize,
                        );
                        let mut ink = Vec::new();
                        let inset = ((extent - 28.) / 2.).max(4.);
                        for y in ((bounds.y + inset) * s) as usize
                            ..((bounds.y + extent - inset) * s) as usize
                        {
                            for x in ((bounds.x + inset) * s) as usize
                                ..((bounds.x + extent - inset) * s) as usize
                            {
                                if at(x, y).iter().zip(base).any(|(a, b)| a.abs_diff(*b) > 20) {
                                    ink.push((x, y));
                                }
                            }
                        }
                        assert!(
                            !ink.is_empty(),
                            "{name} {id} has visible ink (disabled={disabled})"
                        );
                        let cx = (ink.iter().map(|p| p.0).min().unwrap()
                            + ink.iter().map(|p| p.0).max().unwrap()
                            + 1) as f32
                            / (2. * s);
                        let cy = (ink.iter().map(|p| p.1).min().unwrap()
                            + ink.iter().map(|p| p.1).max().unwrap()
                            + 1) as f32
                            / (2. * s);
                        assert!(
                            (cx - bounds.center().x).abs() <= 1.
                                && (cy - bounds.center().y).abs() <= 1.,
                            "{name} {id} at {scale}x (disabled={disabled}): glyph ({cx},{cy}), target {bounds:?}"
                        );
                    }
                    if scale == 1. && !disabled {
                        if let Some(root) = &root {
                            std::fs::create_dir_all(root).unwrap();
                            image::save_buffer(
                                root.join(format!(
                                    "aligned-{name}-{}.png",
                                    if playing { "playing" } else { "paused" }
                                )),
                                &pixels,
                                WIDTH as u32,
                                HEIGHT as u32,
                                image::ColorType::Rgba8,
                            )
                            .unwrap();
                        }
                    }
                    if disabled {
                        assert!(
                            !click(&mut ui, &a, primary)
                                .iter()
                                .any(|m| matches!(m, Message::Control(_)))
                        );
                    }
                }
            }
        }
    }
}

#[test]
fn standard_bar_card_hangs_from_the_bar_and_renders_over_a_translucent_panel() {
    use cosmic::applet::cosmic_panel_config::CosmicPanelBackground;
    use cosmic::iced::Size;
    let mut app = app();
    app.snapshot.music = Some(music());
    app.popup = Some(window::Id::unique());
    app.panel_frame = host::Frame {
        height: 32.,
        padding: 4.,
        radius: 8.,
        attached: true,
        opacity: 0.8,
    };
    let frame = app.frame();
    let mut popup =
        crate::native_test::Native::new(Size::new(frame.popup_width(), frame.popup_height()));
    let root = std::env::var_os("HALCYON_ACTIVITY_EVIDENCE").map(std::path::PathBuf::from);
    let scale = 2f64;
    let (pw, ph) = (
        (frame.popup_width() * scale as f32) as usize,
        (frame.popup_height() * scale as f32) as usize,
    );
    for (theme_name, theme, background, wall) in [
        (
            "dark",
            cosmic::Theme::dark(),
            CosmicPanelBackground::Dark,
            ([46., 58., 74.], [96., 84., 110.]),
        ),
        (
            "light",
            cosmic::Theme::light(),
            CosmicPanelBackground::Light,
            ([214., 196., 170.], [168., 186., 204.]),
        ),
    ] {
        app.core.applet.background = background;
        app.snapshot.calendar = Some(bridge::Calendar {
            key: "event-1".into(),
            title: "Design catch-up".into(),
            starts_at_ms: app.now_ms + 180_000,
            ends_at_ms: app.now_ms + 1_800_000,
            all_day: false,
            join: None,
        });
        app.snapshot.system = vec![system::Activity {
            source: system::Source::Updates,
            title: "Updating software".into(),
            detail: "Open the updater for details".into(),
            cue: "42%".into(),
            progress: Some(42),
            urgent: false,
            announce: false,
            expires_ms: None,
        }];
        for (state, kind, p) in [
            ("music", Kind::Music, 1.),
            ("focus", Kind::Focus, 1.),
            ("calendar", Kind::Calendar, 1.),
            ("updates", Kind::System(system::Source::Updates), 1.),
            ("opening", Kind::Music, 0.4),
        ] {
            app.kind = kind;
            app.motion.retarget(p, false, Instant::now());
            assert!(popup.duplicate_ids(app.capsule(true)).is_empty());
            if p == 1. {
                // Every control fits inside the card, never clipped by its edge.
                let card_left = (frame.popup_width() - surface::ATTACHED_WIDTH) / 2.;
                for id in [
                    "activity-pause",
                    "activity-play",
                    "activity-next",
                    "activity-previous",
                    "activity-start",
                    "activity-focus-action",
                    "activity-dismiss-calendar",
                    "activity-open-calendar",
                    "activity-system-settings",
                    "activity-settings",
                ] {
                    let Some(b) = popup.try_bounds(app.capsule(true), id) else {
                        continue;
                    };
                    if b.y >= frame.bar() {
                        assert!(
                            b.x >= card_left && b.x + b.width <= card_left + surface::ATTACHED_WIDTH,
                            "{state}: {id} leaves the card"
                        );
                    }
                }
            }
            let pixels = popup.render_scaled(app.capsule(true), &theme, scale);
            assert_eq!(pixels.len(), pw * ph * 4);
            // Nothing is painted over the bar beside the slot: neighbours stay visible.
            let row = (frame.bar() * scale as f32) as usize / 2;
            for x in [0, 8, pw - 9, pw - 1] {
                assert_eq!(pixels[(row * pw + x) * 4 + 3], 0, "{state}: bar pixel {x}");
            }
            let Some(root) = &root else { continue };
            let panel = Palette::for_panel(&app.core.applet).surface;
            save_scene(
                &root.join(format!("bar-{state}-{theme_name}.png")),
                &pixels,
                (pw, ph),
                &frame,
                panel,
                wall,
                scale as f32,
            );
        }
    }
}

/// The popup over a gradient wallpaper and the panel's translucent bar.
fn save_scene(
    path: &std::path::Path,
    pixels: &[u8],
    (pw, ph): (usize, usize),
    frame: &host::Frame,
    panel: Color,
    wall: ([f32; 3], [f32; 3]),
    scale: f32,
) {
    let (sw, sh) = (pw + 160 * scale as usize, ph + 24 * scale as usize);
    let bar = (frame.bar() * scale) as usize;
    let mut scene = vec![0u8; sw * sh * 4];
    for y in 0..sh {
        let t = y as f32 / sh as f32;
        for x in 0..sw {
            let mut c = [0.; 3];
            for i in 0..3 {
                c[i] = wall.0[i] + (wall.1[i] - wall.0[i]) * t;
                if y < bar {
                    let o = frame.opacity;
                    c[i] = c[i] * (1. - o) + [panel.r, panel.g, panel.b][i] * 255. * o;
                }
            }
            let px = &mut scene[(y * sw + x) * 4..][..4];
            for i in 0..3 {
                px[i] = c[i].round() as u8;
            }
            px[3] = 255;
        }
    }
    let left = (sw - pw) / 2;
    for y in 0..ph {
        for x in 0..pw {
            let src = &pixels[(y * pw + x) * 4..][..4];
            let a = src[3] as f32 / 255.;
            let dst = &mut scene[(y * sw + left + x) * 4..][..4];
            // The renderer returns premultiplied colour.
            for i in 0..3 {
                dst[i] = (src[i] as f32 + dst[i] as f32 * (1. - a)).round() as u8;
            }
        }
    }
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    image::save_buffer(path, &scene, sw as u32, sh as u32, image::ColorType::Rgba8).unwrap();
}
fn note(id: u32, app: &str, icon: &str, title: &str, body: &str) -> notifications::Note {
    notifications::Note {
        id,
        app: app.into(),
        icon: icon.into(),
        title: title.into(),
        body: body.into(),
        actions: vec![],
        default: true,
        urgent: false,
        low: false,
        resident: false,
        at_ms: 0,
    }
}
fn notify(app: &mut Applet, note: notifications::Note) {
    let _ = app.update(Message::Notes(notifications::Event::Notify(note)));
}
#[test]
fn notifications_lead_with_the_newest_and_count_the_rest() {
    use cosmic::iced::Size;
    let mut app = app();
    app.snapshot.music = Some(music());
    let _ = app.update(Message::Notes(notifications::Event::Ready(true)));
    notify(&mut app, note(1, "Files", "folder-symbolic", "Copy finished", "3 files"));
    let mut mail = note(2, "Mail", "mail-mark-read-symbolic", "Ana Ruiz", "Lunch at noon?");
    mail.actions = vec![("reply".into(), "Reply".into()), ("archive".into(), "Archive".into())];
    notify(&mut app, mail);
    assert_eq!(app.kind, Kind::Notification(2), "the newest notification leads");
    assert_eq!(
        app.eligible_kinds(),
        vec![Kind::Notification(2), Kind::Notification(1), Kind::Music]
    );
    // A replacement keeps one entry and moves to the front.
    notify(&mut app, note(1, "Files", "folder-symbolic", "Copy finished", "4 files"));
    assert_eq!(app.notes.len(), 2);
    assert_eq!(app.kind, Kind::Notification(1));
    notify(&mut app, note(3, "Chat", "user-available-symbolic", "Build channel", "Deploy is green"));
    assert_eq!(app.kind, Kind::Notification(3));
    let frame = app.frame();
    let mut ui = crate::native_test::Native::new(Size::new(frame.popup_width(), frame.popup_height()));
    app.popup = Some(window::Id::unique());
    app.motion.retarget(0., false, Instant::now());
    // Resting: the newest notification, a count of the others, then one
    // icon per other activity, all in the slot.
    let title = ui.bounds(app.capsule(true), "activity-title");
    for other in ["note-count", "other-music"] {
        let b = ui.bounds(app.capsule(true), &format!("activity-{other}"));
        assert!(b.x >= title.x + title.width - 1., "{other} sits after the title");
        assert!(b.x + b.width <= (frame.popup_width() + app.slot_width()) / 2.);
    }
    assert!(ui.text_bounds(app.capsule(true), "+2").width > 0.);
    for id in [
        "activity-other-notification-1",
        "activity-other-notification-2",
        "activity-others",
    ] {
        assert!(ui.try_bounds(app.capsule(true), id).is_none(), "{id}");
    }
    assert!(matches!(
        click(&mut ui, &app, "activity-note-count").as_slice(),
        [Message::ShowNotes]
    ));
    for m in click(&mut ui, &app, "activity-other-music") {
        let _ = app.update(m);
    }
    assert_eq!(app.kind, Kind::Music, "an icon selects its activity");
    // Behind another activity, the newest notification stands for them all.
    assert!(
        ui.try_bounds(app.capsule(true), "activity-note-count")
            .is_none()
    );
    assert!(matches!(
        click(&mut ui, &app, "activity-other-notification-3").as_slice(),
        [Message::ShowNotes]
    ));
    assert!(
        ui.try_bounds(app.capsule(true), "activity-other-notification-1")
            .is_none()
    );
    // The count follows each new one.
    notify(&mut app, note(4, "Calendar", "", "Room booked", ""));
    app.kind = Kind::Notification(4);
    assert!(ui.text_bounds(app.capsule(true), "+3").width > 0.);
    assert!(
        ui.try_bounds(app.capsule(true), "activity-other-music")
            .is_some()
    );
    // Open: message, the app's buttons and Dismiss fit the card.
    app.kind = Kind::Notification(2);
    app.motion.retarget(1., false, Instant::now());
    assert_eq!(app.content().meta, "Lunch at noon?");
    for id in ["activity-note-dismiss", "activity-note-action-0", "activity-note-action-1"] {
        let b = ui.bounds(app.capsule(true), id);
        assert!(b.x >= 0. && b.x + b.width <= frame.popup_width(), "{id}: {b:?}");
    }
    assert!(ui.duplicate_ids(app.capsule(true)).is_empty());
    assert!(matches!(
        click(&mut ui, &app, "activity-title").as_slice(),
        [Message::NoteAction(2, action)] if action == "default"
    ));
    // Dismiss moves on to the next notification while the card stays open.
    let _ = app.update(Message::NoteDismiss(2));
    assert!(app.note(2).is_none());
    assert_eq!(app.kind, Kind::Notification(4));
    // Closed by its app or from COSMIC; expiry never arrives as a close.
    let _ = app.update(Message::Notes(notifications::Event::Closed(4)));
    assert_eq!(app.kind, Kind::Notification(3));
    // Hidden text leaves the app name.
    app.snapshot.preferences.notification_text = false;
    assert_eq!(app.content().meta, "Chat");
    let _ = app.update(Message::NotesClear);
    assert!(app.notes.is_empty());
}
#[test]
fn notification_preferences_reveals_and_popups_follow_the_island() {
    let mut app = app();
    app.host_reveals = true;
    app.observed = true;
    let mut snapshot = app.snapshot.clone();
    snapshot.quiet = Some(false);
    let _ = app.update(Message::Backend(ClientEvent::Snapshot(snapshot.clone())));
    assert_eq!(app.popups, Some(false), "COSMIC keeps its popups by default");
    let _ = app.update(Message::Notes(notifications::Event::Ready(true)));
    // With COSMIC's popups on, the island does not open a second one.
    notify(&mut app, note(1, "Mail", "", "Hello", ""));
    assert!(app.popup.is_none());
    snapshot.preferences.replace_popups = true;
    let _ = app.update(Message::Backend(ClientEvent::Snapshot(snapshot.clone())));
    assert_eq!(app.popups, Some(true));
    notify(&mut app, note(2, "Mail", "", "Again", ""));
    assert!(app.popup.is_some() && app.reveal_deadline.is_some());
    assert_eq!(app.kind, Kind::Notification(2));
    let _ = app.update(Message::Close);
    if let Some(id) = app.popup {
        let _ = app.update(Message::PopupGone(id));
    }
    // Fullscreen or locked: kept, not revealed yet.
    snapshot.quiet = Some(true);
    let _ = app.update(Message::Backend(ClientEvent::Snapshot(snapshot.clone())));
    notify(&mut app, note(3, "Mail", "", "Quiet", ""));
    assert!(app.popup.is_none() && app.note(3).is_some());
    // Losing Halcyon gives COSMIC its popups back.
    let _ = app.update(Message::Backend(ClientEvent::Offline));
    assert_eq!(app.popups, Some(false));
    assert!(!app.eligible_kinds().contains(&Kind::Notification(3)));
    // Turning notifications off forgets them and stops new ones.
    let (tx, _) = tokio::sync::mpsc::channel(4);
    let _ = app.update(Message::Backend(ClientEvent::Connected(tx)));
    snapshot.preferences.notifications = false;
    let _ = app.update(Message::Backend(ClientEvent::Snapshot(snapshot.clone())));
    assert!(app.notes.is_empty());
    assert_eq!(app.popups, Some(false));
    notify(&mut app, note(4, "Mail", "", "Ignored", ""));
    assert!(app.notes.is_empty());
    // System sources follow their own switches.
    snapshot.preferences.notifications = true;
    snapshot.preferences.network = false;
    snapshot.system = vec![system::Activity {
        source: system::Source::Network,
        title: "Offline".into(),
        detail: String::new(),
        cue: String::new(),
        progress: None,
        urgent: false,
        announce: false,
        expires_ms: None,
    }];
    let _ = app.update(Message::Backend(ClientEvent::Snapshot(snapshot)));
    assert!(app.system_activity(system::Source::Network).is_none());
}
#[test]
fn notifications_render_on_islands_and_standard_bars() {
    use cosmic::applet::cosmic_panel_config::CosmicPanelBackground;
    use cosmic::iced::Size;
    let mut app = app();
    app.snapshot.music = Some(music());
    let _ = app.update(Message::Notes(notifications::Event::Ready(true)));
    notify(&mut app, note(1, "Files", "folder-symbolic", "Copy finished", "3 files copied to Documents"));
    notify(&mut app, note(2, "Calendar", "x-office-calendar-symbolic", "Room booked", ""));
    let mut mail = note(
        3,
        "Mail",
        "mail-mark-read-symbolic",
        "Ana Ruiz",
        "Are we still on for lunch at noon? I booked the place by the river, bring the slides if you can.",
    );
    mail.actions = vec![("reply".into(), "Reply".into())];
    notify(&mut app, mail);
    let root = std::env::var_os("HALCYON_ACTIVITY_EVIDENCE").map(std::path::PathBuf::from);
    let scale = 2f32;
    for (shape, attached) in [("island", false), ("bar", true)] {
        app.panel_frame = host::Frame {
            height: 32.,
            padding: 4.,
            radius: 8.,
            attached,
            opacity: if attached { 0.8 } else { 1. },
        };
        let frame = app.frame();
        let mut ui =
            crate::native_test::Native::new(Size::new(frame.popup_width(), frame.popup_height()));
        app.popup = Some(window::Id::unique());
        for (theme_name, theme, background, wall) in [
            ("dark", cosmic::Theme::dark(), CosmicPanelBackground::Dark, ([46., 58., 74.], [96., 84., 110.])),
            ("light", cosmic::Theme::light(), CosmicPanelBackground::Light, ([214., 196., 170.], [168., 186., 204.])),
        ] {
            app.core.applet.background = background;
            for (state, p, urgent) in [
                ("badge", 0., false),
                ("open", 1., false),
                ("urgent", 1., true),
            ] {
                app.kind = Kind::Notification(3);
                app.notes[0].urgent = urgent;
                app.motion.retarget(p, false, Instant::now());
                assert!(ui.duplicate_ids(app.capsule(true)).is_empty());
                if p == 0. {
                    // Resting: the newest notification and a count of the others.
                    let count = ui.bounds(app.capsule(true), "activity-note-count");
                    assert!(
                        count.x + count.width
                            <= (frame.popup_width() + app.slot_width()) / 2. + 0.5,
                        "{shape}: {count:?}"
                    );
                }
                if p == 1. {
                    let edge = (frame.popup_width() - if attached { surface::ATTACHED_WIDTH } else { WIDTH }) / 2.;
                    for id in ["activity-note-dismiss", "activity-note-action-0", "activity-note-clear"] {
                        let Some(b) = ui.try_bounds(app.capsule(true), id) else {
                            continue;
                        };
                        assert!(
                            b.x >= edge - 0.5 && b.x + b.width <= frame.popup_width() - edge + 0.5,
                            "{shape} {state}: {id} {b:?}"
                        );
                        assert!(b.y + b.height <= frame.popup_height(), "{shape} {state}: {id} {b:?}");
                    }
                }
                let pixels = ui.render_scaled(app.capsule(true), &theme, scale as f64);
                let Some(root) = &root else { continue };
                let (pw, ph) = (
                    (frame.popup_width() * scale) as usize,
                    (frame.popup_height() * scale) as usize,
                );
                save_scene(
                    &root.join(format!("note-{shape}-{state}-{theme_name}.png")),
                    &pixels,
                    (pw, ph),
                    &frame,
                    Palette::for_panel(&app.core.applet).surface,
                    wall,
                    scale,
                );
            }
            // Open on the list: every notification in a taller popup.
            app.kind = Kind::Notification(3);
            app.list = true;
            app.popup_extra = app.list_extra();
            app.motion.retarget(1., false, Instant::now());
            let height = app.popup_height();
            let mut tall = crate::native_test::Native::new(Size::new(frame.popup_width(), height));
            assert!(tall.duplicate_ids(app.capsule(true)).is_empty());
            let edge = (frame.popup_width()
                - if attached {
                    surface::ATTACHED_WIDTH
                } else {
                    WIDTH
                })
                / 2.;
            for id in [
                "activity-note-row-3",
                "activity-note-row-2",
                "activity-note-row-1",
                "activity-note-row-dismiss-1",
                "activity-note-clear",
            ] {
                let b = tall.bounds(app.capsule(true), id);
                assert!(
                    b.x >= edge - 0.5 && b.x + b.width <= frame.popup_width() - edge + 0.5,
                    "{shape} list: {id} {b:?}"
                );
                assert!(b.y + b.height <= height, "{shape} list: {id} {b:?}");
            }
            let pixels = tall.render_scaled(app.capsule(true), &theme, scale as f64);
            app.list = false;
            app.popup_extra = 0.;
            let Some(root) = &root else { continue };
            save_scene(
                &root.join(format!("note-{shape}-list-{theme_name}.png")),
                &pixels,
                (
                    (frame.popup_width() * scale) as usize,
                    (height * scale) as usize,
                ),
                &frame,
                Palette::for_panel(&app.core.applet).surface,
                wall,
                scale,
            );
        }
    }
}

/// Music and three notifications, newest first: Chat (no default action),
/// Mail and Files, arriving 2 hours, 5 minutes and moments ago.
fn listed() -> Applet {
    let mut app = app();
    app.snapshot.music = Some(music());
    let _ = app.update(Message::Notes(notifications::Event::Ready(true)));
    notify(
        &mut app,
        note(1, "Files", "folder-symbolic", "Copy finished", "3 files"),
    );
    notify(
        &mut app,
        note(
            2,
            "Mail",
            "mail-mark-read-symbolic",
            "Ana Ruiz",
            "Lunch at noon?",
        ),
    );
    let mut chat = note(
        3,
        "Chat",
        "user-available-symbolic",
        "Build channel",
        "Deploy is green",
    );
    chat.default = false;
    notify(&mut app, chat);
    app.now_ms = app.notes[0].at_ms;
    app.notes[1].at_ms -= 5 * 60_000;
    app.notes[2].at_ms -= 2 * 3_600_000;
    app
}
/// Opens the island by hand and lets it finish growing.
fn open_list(app: &mut Applet) -> crate::native_test::Native {
    let _ = app.update(Message::Hold);
    app.motion.retarget(1., false, Instant::now());
    let size = cosmic::iced::Size::new(app.frame().popup_width(), app.popup_height());
    crate::native_test::Native::new(size)
}
#[test]
fn notification_list_shows_every_notification_newest_first() {
    let mut app = listed();
    let mut ui = open_list(&mut app);
    assert!(app.popup.is_some() && app.held && app.listing());
    assert_eq!(app.popup_extra, app.list_extra());
    assert!(app.popup_extra > 0., "the popup grows for the rows");
    let height = app.popup_height();
    assert_eq!(height, app.frame().popup_height() + app.popup_extra);
    assert!(ui.text_bounds(app.capsule(true), "3 notifications").width > 0.);
    assert!(ui.duplicate_ids(app.capsule(true)).is_empty());
    // Newest first, each with its own dismiss, all inside the popup.
    let mut last = 0.;
    for id in [3, 2, 1] {
        let row = ui.bounds(app.capsule(true), &format!("activity-note-row-{id}"));
        let dismiss = ui.bounds(
            app.capsule(true),
            &format!("activity-note-row-dismiss-{id}"),
        );
        assert!(row.y > last, "row {id} sits below the one before");
        assert!(
            dismiss.x >= row.x + row.width - 0.5,
            "row {id}: dismiss at its end"
        );
        assert!((dismiss.center_y() - row.center_y()).abs() < 1.);
        assert!(dismiss.x + dismiss.width <= app.frame().popup_width());
        assert!(row.y + row.height <= height, "row {id}: {row:?}");
        last = row.y;
    }
    let clear = ui.bounds(app.capsule(true), "activity-note-clear");
    assert!(
        clear.y > last && clear.y + clear.height <= height,
        "{clear:?}"
    );
    // One card at a time is gone, and so is the count.
    for id in [
        "activity-note-dismiss",
        "activity-note-count",
        "activity-other-notification-1",
    ] {
        assert!(ui.try_bounds(app.capsule(true), id).is_none(), "{id}");
    }
    assert!(
        ui.try_bounds(app.capsule(true), "activity-other-music")
            .is_some()
    );
    // App, message and when, as the preference allows.
    let text = ui.all_text(app.capsule(true));
    for line in [
        "Build channel",
        "Chat · Deploy is green",
        "Mail · Lunch at noon?",
        "now",
        "5 min",
    ] {
        assert!(text.iter().any(|t| t == line), "{line}: {text:?}");
    }
    assert!(
        text.contains(&clock_time(app.notes[2].at_ms, false)),
        "{text:?}"
    );
    assert!(!text.iter().any(|t| t.starts_with("and ")));
    app.snapshot.preferences.notification_text = false;
    let text = ui.all_text(app.capsule(true));
    assert!(text.iter().any(|t| t == "Chat") && !text.iter().any(|t| t.contains("Deploy")));
    app.snapshot.preferences.notification_text = true;
    // A row opens its notification, or its card when there is nothing to open.
    assert!(matches!(
        click(&mut ui, &app, "activity-note-row-2").as_slice(),
        [Message::NoteAction(2, action)] if action == "default"
    ));
    let pressed = click(&mut ui, &app, "activity-note-row-3");
    assert!(matches!(
        pressed.as_slice(),
        [Message::Select(Kind::Notification(3))]
    ));
    let _ = app.update(Message::Select(Kind::Notification(3)));
    assert!(!app.listing());
    assert!(
        ui.try_bounds(app.capsule(true), "activity-note-dismiss")
            .is_some()
    );
    // Closing forgets the list; opening by hand brings it back.
    close(&mut app);
    assert!(!app.list && app.popup.is_none());
    let _ = open_list(&mut app);
    assert!(app.listing());
    // A single notification keeps its own card.
    close(&mut app);
    let _ = app.update(Message::NoteDismiss(3));
    let _ = app.update(Message::NoteDismiss(2));
    let _ = open_list(&mut app);
    assert!(app.popup.is_some() && !app.listing() && app.popup_extra == 0.);
}
#[test]
fn notification_list_dismisses_one_and_clears_all() {
    let mut app = listed();
    let mut ui = open_list(&mut app);
    assert!(matches!(
        click(&mut ui, &app, "activity-note-row-dismiss-2").as_slice(),
        [Message::NoteDismiss(2)]
    ));
    let _ = app.update(Message::NoteDismiss(2));
    assert!(app.note(2).is_none() && app.popup.is_some() && app.listing());
    assert!(
        ui.try_bounds(app.capsule(true), "activity-note-row-2")
            .is_none()
    );
    assert!(ui.text_bounds(app.capsule(true), "2 notifications").width > 0.);
    let first = ui.bounds(app.capsule(true), "activity-note-row-3");
    let second = ui.bounds(app.capsule(true), "activity-note-row-1");
    assert!(second.y > first.y);
    // Dismissing the one the closed island showed moves on to the next.
    let _ = app.update(Message::NoteDismiss(3));
    assert_eq!(app.kind, Kind::Notification(1));
    assert!(app.listing());
    assert!(ui.text_bounds(app.capsule(true), "1 notification").width > 0.);
    notify(&mut app, note(5, "Chat", "", "Later", ""));
    assert!(app.listing(), "a new one joins the open list");
    assert!(
        ui.try_bounds(app.capsule(true), "activity-note-row-5")
            .is_some()
    );
    assert!(matches!(
        click(&mut ui, &app, "activity-note-clear").as_slice(),
        [Message::NotesClear]
    ));
    let _ = app.update(Message::NotesClear);
    assert!(app.notes.is_empty() && !app.listing());
    assert_eq!(app.motion.target(), 0.);
}
#[test]
fn notification_list_caps_its_rows_and_counts_the_rest() {
    let mut app = app();
    let _ = app.update(Message::Notes(notifications::Event::Ready(true)));
    for id in 1..=7 {
        notify(&mut app, note(id, "Mail", "", &format!("Message {id}"), ""));
    }
    for (shape, attached) in [("island", false), ("bar", true)] {
        app.panel_frame = host::Frame {
            height: 32.,
            padding: 4.,
            radius: 8.,
            attached,
            opacity: 1.,
        };
        let mut ui = open_list(&mut app);
        assert_eq!(app.list_rows(), LIST_ROWS);
        assert_eq!(app.popup_extra, list_height(LIST_ROWS) - LIST_ROOM);
        let height = app.popup_height();
        for id in 3..=7 {
            let row = ui.bounds(app.capsule(true), &format!("activity-note-row-{id}"));
            assert!(row.y + row.height <= height, "{shape}: row {id} {row:?}");
        }
        for id in 1..=2 {
            let row = format!("activity-note-row-{id}");
            assert!(
                ui.try_bounds(app.capsule(true), &row).is_none(),
                "{shape}: {row}"
            );
        }
        let more = ui.text_bounds(app.capsule(true), "and 2 more");
        let clear = ui.bounds(app.capsule(true), "activity-note-clear");
        assert!(
            (more.center_y() - clear.center_y()).abs() < 2.,
            "{shape}: {more:?} {clear:?}"
        );
        assert!(clear.y + clear.height <= height, "{shape}: {clear:?}");
        assert!(ui.text_bounds(app.capsule(true), "7 notifications").width > 0.);
        close(&mut app);
    }
    // Never more than the panel keeps.
    for id in 8..=30 {
        notify(&mut app, note(id, "Mail", "", "More", ""));
    }
    assert_eq!(app.notes.len(), notifications::LIMIT);
    let mut ui = open_list(&mut app);
    let more = format!("and {} more", notifications::LIMIT - LIST_ROWS);
    assert!(ui.text_bounds(app.capsule(true), &more).width > 0.);
}
#[test]
fn notification_list_follows_the_keyboard() {
    use cosmic::iced::advanced::widget::operation::focusable::focus_next;
    let mut app = listed();
    let mut ui = open_list(&mut app);
    let targets: Vec<(String, cosmic::iced::Rectangle)> = [
        "activity-note-row-3",
        "activity-note-row-dismiss-3",
        "activity-note-row-2",
        "activity-note-row-dismiss-2",
        "activity-note-row-1",
        "activity-note-row-dismiss-1",
        "activity-note-clear",
    ]
    .into_iter()
    .map(|id| (id.to_owned(), ui.bounds(app.capsule(true), id)))
    .collect();
    let mut reached = Vec::new();
    for _ in 0..16 {
        ui.operate(app.capsule(true), Box::new(focus_next::<()>()));
        if let Some(focus) = ui.focused(app.capsule(true))
            && let Some((id, _)) = targets.iter().find(|(_, b)| *b == focus)
            && !reached.contains(id)
        {
            reached.push(id.clone());
        }
    }
    let order: Vec<String> = targets.iter().map(|(id, _)| id.clone()).collect();
    assert_eq!(reached, order, "Tab walks the rows in reading order");
}

/// An island standing in for COSMIC's popups, with nothing open.
fn revealing() -> (Applet, Snapshot) {
    let mut app = app();
    app.host_reveals = true;
    app.observed = true;
    let mut snapshot = app.snapshot.clone();
    snapshot.quiet = Some(false);
    snapshot.preferences.replace_popups = true;
    let _ = app.update(Message::Backend(ClientEvent::Snapshot(snapshot.clone())));
    let _ = app.update(Message::Notes(notifications::Event::Ready(true)));
    assert_eq!(app.popups, Some(true));
    (app, snapshot)
}
fn close(app: &mut Applet) {
    let _ = app.update(Message::Close);
    if let Some(id) = app.popup {
        let _ = app.update(Message::PopupGone(id));
    }
}
/// Runs the island's timers as if `ahead` had passed.
fn wake(app: &mut Applet, ahead: Duration) {
    for at in [&mut app.pending_at, &mut app.reveal_deadline].into_iter().flatten() {
        *at = at.checked_sub(ahead).unwrap_or(*at);
    }
    app.last_reveal = app.last_reveal.and_then(|at| at.checked_sub(ahead));
    let _ = app.update(Message::Wake(Duration::ZERO));
}
#[test]
fn notification_rules_hold_back_low_and_quiet_ones_until_the_way_is_clear() {
    let (mut app, mut snapshot) = revealing();
    // Low urgency is listed but never shown, and never waits.
    let mut low = note(1, "Sync", "", "Synced", "");
    low.low = true;
    notify(&mut app, low);
    assert!(app.popup.is_none() && app.note(1).is_some() && app.pending.is_empty());
    // Fullscreen or locked: notifications wait, in order.
    snapshot.quiet = Some(true);
    let _ = app.update(Message::Backend(ClientEvent::Snapshot(snapshot.clone())));
    notify(&mut app, note(2, "Mail", "", "First", ""));
    notify(&mut app, note(3, "Chat", "", "Second", ""));
    assert!(app.popup.is_none());
    assert_eq!(app.pending, vec![2, 3]);
    snapshot.quiet = Some(false);
    let _ = app.update(Message::Backend(ClientEvent::Snapshot(snapshot.clone())));
    assert!(app.pending_at.is_some(), "the way clearing asks for them");
    wake(&mut app, Duration::ZERO);
    assert!(app.popup.is_some());
    assert_eq!(app.kind, Kind::Notification(2), "oldest first");
    assert_eq!(app.pending, vec![3]);
    // When that reveal ends, the next follows in place.
    wake(&mut app, NOTE_REVEAL);
    assert!(app.popup.is_some());
    assert_eq!(app.kind, Kind::Notification(3));
    assert!(app.pending.is_empty());
    wake(&mut app, NOTE_REVEAL);
    if let Some(id) = app.popup {
        let _ = app.update(Message::PopupGone(id));
    }
    assert!(app.popup.is_none());
    // Arriving while the island is open: shown after it closes.
    let _ = app.update(Message::Hold);
    notify(&mut app, note(4, "Mail", "", "While open", ""));
    assert_eq!(app.pending, vec![4]);
    close(&mut app);
    assert!(app.pending_at.is_some());
    wake(&mut app, PENDING_PAUSE + NOTE_GAP);
    assert_eq!(app.kind, Kind::Notification(4));
    assert!(app.popup.is_some() && app.reveal_deadline.is_some());
    close(&mut app);
    // Opening the island by hand counts as seeing what waited.
    snapshot.quiet = Some(true);
    let _ = app.update(Message::Backend(ClientEvent::Snapshot(snapshot.clone())));
    notify(&mut app, note(5, "Mail", "", "Seen", ""));
    let _ = app.update(Message::Hold);
    assert!(app.pending.is_empty());
}
#[test]
fn notification_rules_summarise_more_than_three_and_honour_do_not_disturb() {
    use cosmic::iced::Size;
    let (mut app, mut snapshot) = revealing();
    snapshot.quiet = Some(true);
    let _ = app.update(Message::Backend(ClientEvent::Snapshot(snapshot.clone())));
    for (id, app_name) in [(1, "Mail"), (2, "Chat"), (3, "Mail"), (4, "Files")] {
        notify(&mut app, note(id, app_name, "", "Waiting", ""));
    }
    snapshot.quiet = Some(false);
    let _ = app.update(Message::Backend(ClientEvent::Snapshot(snapshot.clone())));
    wake(&mut app, Duration::ZERO);
    assert_eq!(app.summary(), Some(4));
    assert!(app.pending.is_empty());
    let content = app.content();
    assert_eq!(content.title, "4 new notifications");
    assert_eq!(content.meta, "Files, Mail and 1 more");
    let frame = app.frame();
    let mut ui = crate::native_test::Native::new(Size::new(frame.popup_width(), frame.popup_height()));
    app.motion.retarget(1., false, Instant::now());
    for id in ["activity-note-all", "activity-note-clear"] {
        let b = ui.bounds(app.capsule(true), id);
        assert!(b.x >= 0. && b.x + b.width <= frame.popup_width(), "{id}: {b:?}");
    }
    assert!(ui.try_bounds(app.capsule(true), "activity-note-dismiss").is_none());
    // Its notifications are not listed again beside it, so the title fits.
    assert!(
        ui.try_bounds(app.capsule(true), "activity-others")
            .is_none()
    );
    assert!(
        ui.text_bounds(app.capsule(true), "4 new notifications")
            .width
            > 0.
    );
    assert!(matches!(
        click(&mut ui, &app, "activity-note-all").as_slice(),
        [Message::ShowNotes]
    ));
    assert!(matches!(
        click(&mut ui, &app, "activity-title").as_slice(),
        [Message::ShowNotes]
    ));
    // The reveal's popup is too small for the rows: it reopens in hand.
    let revealed = app.popup.unwrap();
    let _ = app.update(Message::ShowNotes);
    assert_eq!(app.promote_popup, Some(revealed));
    assert!(app.held && app.list && app.summary().is_none() && app.reveal_deadline.is_none());
    let _ = app.update(Message::PopupGone(revealed));
    assert!(app.popup.is_some_and(|id| id != revealed));
    assert!(app.listing() && app.held);
    assert_eq!(app.popup_extra, app.list_extra());
    app.motion.retarget(1., false, Instant::now());
    let mut tall =
        crate::native_test::Native::new(Size::new(frame.popup_width(), app.popup_height()));
    assert!(tall.text_bounds(app.capsule(true), "4 notifications").width > 0.);
    for id in 1..=4 {
        assert!(
            tall.try_bounds(app.capsule(true), &format!("activity-note-row-{id}"))
                .is_some()
        );
    }
    close(&mut app);
    assert!(app.batch.is_empty() && !app.list);
    // A burst during a reveal also reads as one summary.
    wake(&mut app, NOTE_GAP);
    notify(&mut app, note(5, "Chat", "", "One", ""));
    assert!(app.popup.is_some() && app.summary().is_none());
    for id in 6..=8 {
        notify(&mut app, note(id, "Chat", "", "More", ""));
    }
    assert_eq!(app.summary(), Some(4));
    // A critical one is never folded into the count.
    let mut alarm = note(10, "Power", "", "Battery critical", "");
    alarm.urgent = true;
    notify(&mut app, alarm);
    assert!(app.summary().is_none());
    assert_eq!(app.showing_critical(), Some(10));
    let _ = app.update(Message::NoteDismiss(10));
    close(&mut app);
    // Do Not Disturb: ordinary notifications neither show nor wait.
    snapshot.dnd = true;
    let _ = app.update(Message::Backend(ClientEvent::Snapshot(snapshot.clone())));
    wake(&mut app, NOTE_GAP);
    notify(&mut app, note(9, "Mail", "", "Hushed", ""));
    assert!(app.popup.is_none() && app.pending.is_empty() && app.note(9).is_some());
    // Calendar and system reveals stay quiet too, past any cooldown.
    let meeting = |key: &str, now_ms: i64| bridge::Calendar {
        key: key.into(),
        title: "Review".into(),
        starts_at_ms: now_ms + 120_000,
        ends_at_ms: now_ms + 900_000,
        all_day: false,
        join: None,
    };
    app.last_reveal = None;
    snapshot.calendar = Some(meeting("soon", app.now_ms));
    let _ = app.update(Message::Backend(ClientEvent::Snapshot(snapshot.clone())));
    assert!(app.popup.is_none());
    // The same news with Do Not Disturb off does reveal.
    snapshot.dnd = false;
    snapshot.calendar = Some(meeting("later", app.now_ms));
    let _ = app.update(Message::Backend(ClientEvent::Snapshot(snapshot)));
    assert!(app.popup.is_some() && app.kind == Kind::Calendar);
}
#[test]
fn critical_notifications_stay_until_someone_is_there() {
    let (mut app, mut snapshot) = revealing();
    snapshot.dnd = true;
    let _ = app.update(Message::Backend(ClientEvent::Snapshot(snapshot.clone())));
    let mut alarm = note(1, "Power", "", "Battery critical", "Plug in now");
    alarm.urgent = true;
    notify(&mut app, alarm);
    assert!(app.popup.is_some(), "critical gets through Do Not Disturb");
    assert_eq!(app.showing_critical(), Some(1));
    // Time alone does not take it away, and ordinary news does not cover it.
    wake(&mut app, NOTE_REVEAL * 3);
    assert!(app.popup.is_some() && app.reveal_deadline.is_some());
    snapshot.dnd = false;
    let _ = app.update(Message::Backend(ClientEvent::Snapshot(snapshot.clone())));
    notify(&mut app, note(2, "Chat", "", "Lunch?", ""));
    assert_eq!(app.kind, Kind::Notification(1));
    assert_eq!(app.pending, vec![2]);
    // A second critical one shows; the first waits rather than vanishing.
    let mut second = note(3, "Disk", "", "Disk almost full", "");
    second.urgent = true;
    notify(&mut app, second);
    assert_eq!(app.showing_critical(), Some(3));
    assert_eq!(app.pending, vec![2, 1]);
    app.pending.retain(|p| *p == 2);
    let _ = app.update(Message::Notes(notifications::Event::Closed(3)));
    let _ = app.update(Message::Select(Kind::Notification(1)));
    app.reveal_deadline = Some(Instant::now() + NOTE_REVEAL);
    assert_eq!(app.showing_critical(), Some(1));
    // Locking cuts it short; it comes back first after unlocking.
    snapshot.quiet = Some(true);
    let _ = app.update(Message::Backend(ClientEvent::Snapshot(snapshot.clone())));
    if let Some(id) = app.popup {
        let _ = app.update(Message::Frame(Instant::now() + Duration::from_secs(5)));
        if app.popup == Some(id) {
            let _ = app.update(Message::PopupGone(id));
        }
    }
    assert!(app.popup.is_none());
    assert_eq!(app.pending, vec![2, 1]);
    snapshot.quiet = Some(false);
    let _ = app.update(Message::Backend(ClientEvent::Snapshot(snapshot.clone())));
    wake(&mut app, PENDING_PAUSE + NOTE_GAP);
    assert_eq!(app.kind, Kind::Notification(1), "critical goes first");
    assert_eq!(app.pending, vec![2]);
    // Pointing at it is someone being there: it then closes like any card.
    let _ = app.update(Message::Enter(true));
    assert!(app.reveal_deadline.is_none());
    let _ = app.update(Message::Leave(true));
    assert!(app.leave_deadline.is_some());
}
#[test]
fn do_not_disturb_switches_from_the_island_and_shows_a_moon() {
    use cosmic::iced::Size;
    let (mut app, snapshot) = revealing();
    app.kind = Kind::Idle;
    let frame = app.frame();
    let mut ui = crate::native_test::Native::new(Size::new(frame.popup_width(), frame.popup_height()));
    app.popup = Some(window::Id::unique());
    app.motion.retarget(1., false, Instant::now());
    // Open and idle: the switch sits beside Activity settings, both in the card.
    for id in ["activity-dnd", "activity-preferences"] {
        let b = ui.bounds(app.capsule(true), id);
        assert!(b.x >= 0. && b.x + b.width <= frame.popup_width(), "{id}: {b:?}");
    }
    assert!(ui.try_bounds(app.capsule(true), "activity-dnd-on").is_none());
    assert!(matches!(
        click(&mut ui, &app, "activity-dnd").as_slice(),
        [Message::Dnd(true)]
    ));
    let _ = app.update(Message::Dnd(true));
    assert!(app.snapshot.dnd);
    // A snapshot from before the desktop saw the change does not undo it.
    let _ = app.update(Message::Backend(ClientEvent::Snapshot(snapshot.clone())));
    assert!(app.snapshot.dnd);
    let mut seen = snapshot.clone();
    seen.dnd = true;
    let _ = app.update(Message::Backend(ClientEvent::Snapshot(seen)));
    assert!(app.dnd_wish.is_none(), "settled once the desktop agrees");
    // Closed: a moon says notifications are held back.
    app.motion.retarget(0., false, Instant::now());
    let moon = ui.bounds(app.capsule(true), "activity-dnd-on");
    assert!(moon.x + moon.width <= (frame.popup_width() + app.slot_width()) / 2.);
    // The activity list has the switch too, and turns it off.
    app.motion.retarget(1., false, Instant::now());
    app.chooser = true;
    assert!(matches!(
        click(&mut ui, &app, "activity-choice-dnd").as_slice(),
        [Message::Dnd(false)]
    ));
    assert!(ui.duplicate_ids(app.capsule(true)).is_empty());
}
fn activity(source: system::Source, title: &str, detail: &str, expires_ms: Option<i64>) -> system::Activity {
    system::Activity {
        source,
        title: title.into(),
        detail: detail.into(),
        cue: String::new(),
        progress: None,
        urgent: false,
        announce: true,
        expires_ms,
    }
}
#[test]
fn privacy_dots_show_a_camera_or_microphone_on_beside_the_title() {
    use cosmic::iced::Size;
    let mut app = app();
    app.snapshot.music = Some(music());
    app.snapshot.system = vec![
        activity(system::Source::Camera, "Camera in use", "zoom", None),
        activity(system::Source::Microphone, "Microphone in use", "zoom", None),
    ];
    app.kind = Kind::Music;
    let frame = app.frame();
    let mut ui = crate::native_test::Native::new(Size::new(frame.popup_width(), frame.popup_height()));
    app.popup = Some(window::Id::unique());
    for p in [0., 1.] {
        app.motion.retarget(p, false, Instant::now());
        let title = ui.bounds(app.capsule(true), "activity-title");
        for id in ["activity-privacy-camera", "activity-privacy-microphone"] {
            let dot = ui.bounds(app.capsule(true), id);
            assert!(dot.width > 0. && dot.width <= DOT + 0.5, "{id}: {dot:?}");
            assert!(dot.x >= title.x && dot.x + dot.width <= title.x + title.width, "{id} sits in the title row");
        }
        assert!(ui.duplicate_ids(app.capsule(true)).is_empty());
        // Each shows once: as its dot, never also as an icon beside it.
        for source in [system::Source::Camera, system::Source::Microphone] {
            let id = format!("activity-other-{}", Kind::System(source).id());
            assert!(ui.try_bounds(app.capsule(true), &id).is_none(), "{id}");
        }
    }
    // The dot opens its card.
    assert!(matches!(
        click(&mut ui, &app, "activity-privacy-microphone-open").as_slice(),
        [Message::Select(Kind::System(system::Source::Microphone))]
    ));
    // Neither takes the island over, and its own card needs no dot.
    assert_eq!(app.automatic_kind(), Kind::Music);
    app.kind = Kind::System(system::Source::Camera);
    assert!(ui.try_bounds(app.capsule(true), "activity-privacy-camera").is_none());
    assert!(ui.try_bounds(app.capsule(true), "activity-privacy-microphone").is_some());
    // Each follows its own switch.
    app.snapshot.preferences.camera = false;
    app.kind = Kind::Music;
    assert!(ui.try_bounds(app.capsule(true), "activity-privacy-camera").is_none());
    assert!(!app.eligible_kinds().contains(&Kind::System(system::Source::Camera)));
}
#[test]
fn bluetooth_connections_reveal_briefly_and_follow_their_switch() {
    use cosmic::iced::Size;
    let (mut app, mut snapshot) = revealing();
    let mut buds = activity(system::Source::Bluetooth, "Buds", "Connected", Some(app.now_ms + 4000));
    buds.cue = "70%".into();
    snapshot.system = vec![buds];
    let _ = app.update(Message::Backend(ClientEvent::Snapshot(snapshot.clone())));
    assert!(app.popup.is_some() && app.reveal_deadline.is_some());
    assert_eq!(app.kind, Kind::System(system::Source::Bluetooth));
    let content = app.content();
    assert_eq!((content.title.as_str(), content.meta.as_str(), content.cue.as_str()), ("Buds", "Connected", "70%"));
    let frame = app.frame();
    let mut ui = crate::native_test::Native::new(Size::new(frame.popup_width(), frame.popup_height()));
    app.motion.retarget(1., false, Instant::now());
    assert!(ui.text_bounds(app.capsule(true), "Bluetooth settings").width > 0.);
    assert!(matches!(
        click(&mut ui, &app, "activity-system-settings").as_slice(),
        [Message::SystemSettings(system::Source::Bluetooth)]
    ));
    close(&mut app);
    snapshot.preferences.bluetooth = false;
    let _ = app.update(Message::Backend(ClientEvent::Snapshot(snapshot)));
    assert!(app.system_activity(system::Source::Bluetooth).is_none());
}
#[test]
fn pointer_middle_plays_right_raises_or_dismisses_and_swipes_send_reveals_away() {
    use cosmic::iced::{Event, Size, mouse};
    let (mut app, mut snapshot) = revealing();
    snapshot.music = Some(music());
    let _ = app.update(Message::Backend(ClientEvent::Snapshot(snapshot.clone())));
    let (tx, mut rx) = tokio::sync::mpsc::channel(8);
    app.commands = Some(tx);
    let frame = app.frame();
    let mut ui = crate::native_test::Native::new(Size::new(frame.popup_width(), frame.popup_height()));
    let press = |ui: &mut crate::native_test::Native, app: &Applet, button| {
        let at = ui.bounds(app.capsule(true), "activity-title").center();
        ui.event(app.capsule(true), Event::Mouse(mouse::Event::ButtonPressed(button)), at).0
    };
    // Resting or opened by hand: middle plays or pauses, right raises the player.
    app.kind = Kind::Music;
    app.popup = Some(window::Id::unique());
    app.motion.retarget(1., false, Instant::now());
    assert!(press(&mut ui, &app, mouse::Button::Middle).iter().any(|m| matches!(m, Message::Middle)));
    let _ = app.update(Message::Middle);
    assert!(matches!(rx.try_recv(), Ok(Command::Pause { key }) if key == "track-1"));
    assert!(press(&mut ui, &app, mouse::Button::Right).iter().any(|m| matches!(m, Message::Secondary)));
    let _ = app.update(Message::Secondary);
    assert!(matches!(rx.try_recv(), Ok(Command::Raise { key }) if key == "track-1"));
    // A wheel scrolls nothing away, and swipes leave a card opened by hand.
    let scroll = |ui: &mut crate::native_test::Native, app: &Applet, delta| {
        let at = ui.bounds(app.capsule(true), "activity-title").center();
        ui.event(app.capsule(true), Event::Mouse(mouse::Event::WheelScrolled { delta }), at).0
    };
    let swiped = |messages: Vec<Message>| messages.iter().any(|m| matches!(m, Message::Swipe));
    assert!(!swiped(scroll(&mut ui, &app, mouse::ScrollDelta::Lines { x: 0., y: 3. })));
    let _ = app.update(Message::Swipe);
    assert_eq!(app.motion.target(), 1.);
    close(&mut app);
    // A reveal: right-click sends it away even after the pointer reached it.
    notify(&mut app, note(1, "Mail", "", "Hello", ""));
    assert!(app.revealed && app.popup.is_some());
    let _ = app.update(Message::Enter(true));
    assert!(app.reveal_deadline.is_none() && app.revealed);
    let _ = app.update(Message::Secondary);
    assert_eq!(app.motion.target(), 0.);
    assert!(app.hover_block.is_some(), "the resting pointer does not reopen it");
    assert!(rx.try_recv().is_err(), "dismissing raises nothing");
    close(&mut app);
    assert!(!app.revealed);
    // A touchpad swipe, either way, past a small travel.
    app.hover_block = None;
    wake(&mut app, NOTE_GAP);
    notify(&mut app, note(2, "Mail", "", "Again", ""));
    app.motion.retarget(1., false, Instant::now());
    assert!(!swiped(scroll(&mut ui, &app, mouse::ScrollDelta::Pixels { x: 0., y: -12. })));
    assert!(swiped(scroll(&mut ui, &app, mouse::ScrollDelta::Pixels { x: 0., y: -36. })));
    assert!(!swiped(scroll(&mut ui, &app, mouse::ScrollDelta::Pixels { x: 0., y: -60. })), "once per gesture");
    let _ = app.update(Message::Swipe);
    assert_eq!(app.motion.target(), 0.);
    // Middle-click without a player does nothing.
    close(&mut app);
    snapshot.music = None;
    let _ = app.update(Message::Backend(ClientEvent::Snapshot(snapshot)));
    let _ = app.update(Message::Middle);
    assert!(rx.try_recv().is_err());
}
/// Pictures of the Do Not Disturb, privacy, summary, critical and Bluetooth
/// states, written only when evidence is asked for.
#[test]
fn tier_one_states_render_for_review() {
    use cosmic::applet::cosmic_panel_config::CosmicPanelBackground;
    use cosmic::iced::Size;
    let Some(root) = std::env::var_os("HALCYON_ACTIVITY_EVIDENCE").map(std::path::PathBuf::from)
    else {
        return;
    };
    let scale = 2f32;
    type Setup = fn(&mut Applet);
    let states: [(&str, f32, Setup); 7] = [
        ("closed-moon-and-dots", 0., |app| {
            app.snapshot.music = Some(music());
            app.snapshot.dnd = true;
            app.snapshot.system = vec![
                activity(system::Source::Camera, "Camera in use", "zoom", None),
                activity(system::Source::Microphone, "Microphone in use", "zoom", None),
            ];
            app.kind = Kind::Music;
        }),
        ("open-dots", 1., |app| {
            app.snapshot.music = Some(music());
            app.snapshot.system = vec![
                activity(system::Source::Camera, "Camera in use", "zoom", None),
                activity(system::Source::Microphone, "Microphone in use", "zoom", None),
            ];
            app.kind = Kind::Music;
        }),
        ("open-idle-dnd-off", 1., |app| app.kind = Kind::Idle),
        ("open-idle-dnd-on", 1., |app| {
            app.snapshot.dnd = true;
            app.kind = Kind::Idle;
        }),
        ("chooser-dnd", 1., |app| {
            app.snapshot.music = Some(music());
            app.kind = Kind::Music;
            app.chooser = true;
        }),
        ("summary", 1., |app| {
            for (id, name, icon) in [
                (1, "Mail", "mail-mark-read-symbolic"),
                (2, "Chat", "user-available-symbolic"),
                (3, "Mail", "mail-mark-read-symbolic"),
                (4, "Files", "folder-symbolic"),
                (5, "Calendar", "x-office-calendar-symbolic"),
            ] {
                notify(app, note(id, name, icon, "Waiting", ""));
            }
            app.kind = Kind::Notification(5);
            app.batch = vec![1, 2, 3, 4, 5];
        }),
        ("critical", 1., |app| {
            let mut battery = note(
                9,
                "Power",
                "battery-caution-symbolic",
                "Battery critically low",
                "5% left. Plug in now or the computer will suspend.",
            );
            battery.urgent = true;
            notify(app, battery);
            app.kind = Kind::Notification(9);
        }),
    ];
    for (theme_name, theme, background, wall) in [
        ("dark", cosmic::Theme::dark(), CosmicPanelBackground::Dark, ([46., 58., 74.], [96., 84., 110.])),
        ("light", cosmic::Theme::light(), CosmicPanelBackground::Light, ([214., 196., 170.], [168., 186., 204.])),
    ] {
        let mut shots: Vec<(&str, f32, Setup)> = states.to_vec();
        shots.push(("bluetooth", 1., |app| {
            let mut buds =
                activity(system::Source::Bluetooth, "Buds", "Connected", Some(app.now_ms + 4000));
            buds.cue = "70%".into();
            app.snapshot.system = vec![buds];
            app.kind = Kind::System(system::Source::Bluetooth);
        }));
        for (name, p, setup) in shots {
            let mut app = app();
            let _ = app.update(Message::Notes(notifications::Event::Ready(true)));
            setup(&mut app);
            app.core.applet.background = background.clone();
            let frame = app.frame();
            let mut ui =
                crate::native_test::Native::new(Size::new(frame.popup_width(), frame.popup_height()));
            app.popup = Some(window::Id::unique());
            app.motion.retarget(p, false, Instant::now());
            assert!(ui.duplicate_ids(app.capsule(true)).is_empty(), "{name}");
            let pixels = ui.render_scaled(app.capsule(true), &theme, scale as f64);
            save_scene(
                &root.join(format!("tier1-{name}-{theme_name}.png")),
                &pixels,
                ((frame.popup_width() * scale) as usize, (frame.popup_height() * scale) as usize),
                &frame,
                Palette::for_panel(&app.core.applet).surface,
                wall,
                scale,
            );
        }
    }
}
