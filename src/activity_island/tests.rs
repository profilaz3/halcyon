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
                color: None,
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
        color: None,
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
        color: None,
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
        color: None,
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
    // No call link, no Join: not on the card, not as a control.
    assert!(!ui.all_text(app.capsule(true)).iter().any(|t| t.contains("Join")));
    let messages = click(&mut ui, &app, "activity-open-calendar");
    assert!(!messages.iter().any(|m| matches!(m, Message::Join(_))));
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
        color: None,
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
                color: None,
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
                color: None,
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
        color: None,
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
            color: None,
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
            let panel = app.palette().surface;
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
        let palette = app.palette();
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
            color: None,
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
            let panel = app.palette().surface;
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
        entry: String::new(),
        timeout_ms: 0,
        progress: None,
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
                    app.palette().surface,
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
                app.palette().surface,
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
        // One app each, so none of them group.
        notify(&mut app, note(id, &format!("App {id}"), "", &format!("Message {id}"), ""));
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
        notify(&mut app, note(id, &format!("App {id}"), "", "More", ""));
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
        "activity-note-row-more-3",
        "activity-note-row-dismiss-3",
        "activity-note-row-2",
        "activity-note-row-more-2",
        "activity-note-row-dismiss-2",
        "activity-note-row-1",
        "activity-note-row-more-1",
        "activity-note-row-dismiss-1",
        "activity-note-clear",
    ]
    .into_iter()
    .map(|id| (id.to_owned(), ui.bounds(app.capsule(true), id)))
    .collect();
    let mut reached = Vec::new();
    for _ in 0..24 {
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
    // Both from Mail read as one group.
    for id in ["activity-note-row-4", "activity-note-row-2", "activity-note-group-Mail"] {
        assert!(tall.try_bounds(app.capsule(true), id).is_some(), "{id}");
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
        color: None,
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
                app.palette().surface,
                wall,
                scale,
            );
        }
    }
}

fn right_click(ui: &mut crate::native_test::Native, app: &Applet, id: &str) -> Vec<Message> {
    use cosmic::iced::{Event, mouse};
    let at = ui.bounds(app.capsule(true), id).center();
    ui.event(
        app.capsule(true),
        Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Right)),
        at,
    )
    .0
}
fn slack(id: u32, title: &str, body: &str) -> notifications::Note {
    let mut n = note(id, "Slack", "user-available-symbolic", title, body);
    n.entry = "com.slack.Slack".into();
    n
}
/// Files once, Slack four times with one reply action, then an urgent Slack.
fn grouped() -> Applet {
    let mut app = app();
    app.snapshot.music = Some(music());
    let _ = app.update(Message::Notes(notifications::Event::Ready(true)));
    notify(&mut app, note(1, "Files", "folder-symbolic", "Copy finished", "3 files"));
    notify(&mut app, slack(2, "Ana", "Standup moved"));
    let mut reply = slack(3, "Build", "Deploy is green");
    reply.actions = vec![("reply".into(), "Reply".into())];
    notify(&mut app, reply);
    notify(&mut app, slack(4, "Lee", "Lunch?"));
    notify(&mut app, slack(5, "Ana", "Running late"));
    let mut urgent = slack(6, "Incident", "Payments are down");
    urgent.urgent = true;
    notify(&mut app, urgent);
    app.now_ms = app.notes[0].at_ms;
    app
}
const SLACK: &str = "com.slack.Slack";
/// Reopens a popup that asked to grow, as the shell does.
fn regrow(app: &mut Applet) -> crate::native_test::Native {
    if let Some(id) = app.promote_popup {
        let _ = app.update(Message::PopupGone(id));
    }
    app.motion.retarget(1., false, Instant::now());
    crate::native_test::Native::new(cosmic::iced::Size::new(
        app.frame().popup_width(),
        app.popup_height(),
    ))
}
#[test]
fn notification_groups_gather_an_app_and_keep_urgent_ones_on_top() {
    let mut app = grouped();
    assert_eq!(
        app.lines(),
        vec![
            inbox::Line::Note(6),
            inbox::Line::Group(SLACK.into(), vec![5, 4, 3, 2]),
            inbox::Line::Note(1),
        ]
    );
    // Closed, the pill counts the app's notifications.
    app.kind = Kind::Notification(5);
    app.popup = Some(window::Id::unique());
    app.motion.retarget(0., false, Instant::now());
    let mut ui = crate::native_test::Native::new(cosmic::iced::Size::new(
        app.frame().popup_width(),
        app.frame().popup_height(),
    ));
    let text = ui.all_text(app.capsule(true));
    assert!(text.iter().any(|t| t == "Slack · 4"), "{text:?}");
    app.popup = None;
    app.kind = Kind::Notification(6);
    let mut ui = open_list(&mut app);
    assert!(app.listing());
    assert!(ui.duplicate_ids(app.capsule(true)).is_empty());
    assert!(ui.text_bounds(app.capsule(true), "6 notifications").width > 0.);
    let text = ui.all_text(app.capsule(true));
    for line in ["Slack · 4 new", "Ana · Running late", "Incident"] {
        assert!(text.iter().any(|t| t == line), "{line}: {text:?}");
    }
    // Urgent on top, the group, then the single row; members stay folded.
    let group = format!("activity-note-group-{SLACK}");
    let urgent = ui.bounds(app.capsule(true), "activity-note-row-6");
    let head = ui.bounds(app.capsule(true), &group);
    let files = ui.bounds(app.capsule(true), "activity-note-row-1");
    assert!(urgent.y < head.y && head.y < files.y);
    for id in 2..=5 {
        assert!(ui.try_bounds(app.capsule(true), &format!("activity-note-row-{id}")).is_none());
    }
    assert!(!text.iter().any(|t| t.starts_with("and ")), "{text:?}");
    // Opening the group lists its notifications in place, newest first.
    assert!(matches!(
        click(&mut ui, &app, &group).as_slice(),
        [Message::GroupToggle(key)] if key == SLACK
    ));
    let _ = app.update(Message::GroupToggle(SLACK.into()));
    assert_eq!(app.open_group.as_deref(), Some(SLACK));
    assert!(app.promote_popup.is_some(), "the popup grows for the members");
    let mut ui = regrow(&mut app);
    assert!(app.popup.is_some() && app.listing());
    assert_eq!(app.popup_extra, app.list_extra());
    let height = app.popup_height();
    let mut last = ui.bounds(app.capsule(true), &group).y;
    for id in [5, 4, 3, 2] {
        let row = ui.bounds(app.capsule(true), &format!("activity-note-row-{id}"));
        assert!(row.y > last && row.y + row.height <= height, "member {id}: {row:?}");
        // Members carry no More of their own; the group does.
        assert!(ui.try_bounds(app.capsule(true), &format!("activity-note-row-more-{id}")).is_none());
        let dismiss = ui.bounds(app.capsule(true), &format!("activity-note-row-dismiss-{id}"));
        assert!((dismiss.center_y() - row.center_y()).abs() < 1.);
        last = row.y;
    }
    let files = ui.bounds(app.capsule(true), "activity-note-row-1");
    assert!(files.y > last && files.y + files.height <= height);
    assert!(matches!(
        click(&mut ui, &app, "activity-note-row-action-3-0").as_slice(),
        [Message::NoteAction(3, key)] if key == "reply"
    ));
    assert!(matches!(
        click(&mut ui, &app, "activity-note-row-dismiss-4").as_slice(),
        [Message::NoteDismiss(4)]
    ));
    // One group at a time.
    notify(&mut app, note(7, "Mail", "", "First", ""));
    notify(&mut app, note(8, "Mail", "", "Second", ""));
    let _ = app.update(Message::GroupToggle("Mail".into()));
    assert_eq!(app.open_group.as_deref(), Some("Mail"));
    let _ = app.update(Message::GroupToggle("Mail".into()));
    assert!(app.open_group.is_none());
    // The group's dismiss clears the group; the urgent one stays.
    let mut ui = regrow(&mut app);
    assert!(matches!(
        click(&mut ui, &app, &format!("activity-note-group-dismiss-{SLACK}")).as_slice(),
        [Message::GroupDismiss(key)] if key == SLACK
    ));
    let _ = app.update(Message::GroupDismiss(SLACK.into()));
    let left: Vec<u32> = app.notes.iter().map(|n| n.id).collect();
    assert_eq!(left, vec![8, 7, 6, 1]);
    let recent: Vec<u32> = app.recent.iter().map(|r| r.note.id).collect();
    assert_eq!(recent, vec![5, 4, 3, 2], "Recent, newest first");
    assert!(app.recent.iter().all(|r| !r.alive));
    assert!(app.listing());
    // A single notification from an app is a plain row.
    let _ = app.update(Message::NoteDismiss(8));
    assert!(!app.lines().iter().any(|l| matches!(l, inbox::Line::Group(..))));
    // Right-click on a row opens its app's rules and raises nothing.
    let (tx, mut rx) = tokio::sync::mpsc::channel(8);
    app.commands = Some(tx);
    let mut ui = regrow(&mut app);
    for message in right_click(&mut ui, &app, "activity-note-row-7") {
        let _ = app.update(message);
    }
    assert_eq!(app.menu.as_ref().map(|m| m.app.as_str()), Some("Mail"));
    assert!(rx.try_recv().is_err(), "the player stays where it is");
}
#[test]
fn recent_keeps_twenty_in_memory_and_opens_what_it_can() {
    let mut app = app();
    let _ = app.update(Message::Notes(notifications::Event::Ready(true)));
    for id in 1..=25 {
        notify(&mut app, note(id, &format!("App {id}"), "", &format!("Note {id}"), ""));
        let _ = app.update(Message::NoteDismiss(id));
    }
    assert_eq!(app.recent.len(), inbox::RECENT);
    assert_eq!(app.recent[0].note.id, 25);
    assert_eq!(app.recent[inbox::RECENT - 1].note.id, 6);
    // The same id comes back once, at the top.
    notify(&mut app, note(10, "App 10", "", "Again", ""));
    assert!(app.recent.iter().all(|r| r.note.id != 10), "live again, not recent");
    let _ = app.update(Message::NoteDismiss(10));
    assert_eq!(app.recent.iter().filter(|r| r.note.id == 10).count(), 1);
    assert_eq!(app.recent[0].note.id, 10);
    // Closed by the app: gone from the server. Timed out: still there.
    notify(&mut app, note(40, "Mail", "", "Closed", ""));
    notify(&mut app, note(41, "Mail", "", "Timed out", ""));
    let _ = app.update(Message::Notes(notifications::Event::Closed(40)));
    let _ = app.update(Message::Notes(notifications::Event::Expired(41)));
    assert!(app.notes.is_empty());
    let alive = |app: &Applet, id| app.recent.iter().find(|r| r.note.id == id).map(|r| r.alive);
    assert_eq!(alive(&app, 40), Some(false));
    assert_eq!(alive(&app, 41), Some(true));
    // Its own timeout moves a notification to Recent without polling.
    let mut timed = note(42, "Mail", "", "Short lived", "");
    timed.timeout_ms = 5_000;
    notify(&mut app, timed);
    assert!(app.expiry_at.is_some(), "one deadline for the soonest timeout");
    app.notes[0].at_ms -= 6_000;
    app.expiry_at = Some(Instant::now());
    let _ = app.update(Message::Wake(Duration::ZERO));
    assert!(app.note(42).is_none() && alive(&app, 42) == Some(true));
    assert!(app.expiry_at.is_none());
    // With nothing live, the island still reaches Recent.
    let mut ui = crate::native_test::Native::new(cosmic::iced::Size::new(
        app.frame().popup_width(),
        app.frame().popup_height(),
    ));
    app.popup = Some(window::Id::unique());
    app.motion.retarget(1., false, Instant::now());
    let shown = ui.bounds(app.capsule(true), "activity-recent-show");
    assert!(shown.width > 0.);
    app.popup = None;
    let _ = app.update(Message::ShowRecent);
    assert!(app.listing() && app.recent_open && app.notes.is_empty());
    let mut ui = regrow(&mut app);
    assert!(ui.text_bounds(app.capsule(true), "Recent").width > 0.);
    assert!(ui.text_bounds(app.capsule(true), "Recent · 20").width > 0.);
    assert!(ui.try_bounds(app.capsule(true), "activity-note-clear").is_none());
    // Alive and openable: a button. Gone, with no app known: plain text.
    assert!(ui.try_bounds(app.capsule(true), "activity-recent-row-42").is_some());
    assert!(ui.try_bounds(app.capsule(true), "activity-recent-row-40").is_none());
    assert!(ui.all_text(app.capsule(true)).iter().any(|t| t == "Closed"));
    assert!(matches!(
        click(&mut ui, &app, "activity-recent-row-42").as_slice(),
        [Message::RecentOpen(42)]
    ));
    let _ = app.update(Message::RecentOpen(42));
    assert_eq!(alive(&app, 42), Some(false), "opened once, it is gone");
    let _ = app.update(Message::RecentOpen(40));
    // Clear empties it and closes an island with nothing else.
    let _ = app.update(Message::ShowRecent);
    let mut ui = regrow(&mut app);
    assert!(matches!(
        click(&mut ui, &app, "activity-recent-clear").as_slice(),
        [Message::RecentClear]
    ));
    let _ = app.update(Message::RecentClear);
    assert!(app.recent.is_empty() && app.motion.target() == 0.);
    // Turning notifications off forgets it.
    notify(&mut app, note(50, "Mail", "", "Kept", ""));
    let _ = app.update(Message::NoteDismiss(50));
    let mut snapshot = app.snapshot.clone();
    snapshot.preferences.notifications = false;
    let _ = app.update(Message::Backend(ClientEvent::Snapshot(snapshot)));
    assert!(app.recent.is_empty());
}
#[test]
fn recent_and_notification_content_never_leave_memory() {
    let mut app = grouped();
    let (tx, mut rx) = tokio::sync::mpsc::channel(32);
    app.commands = Some(tx);
    let _ = app.update(Message::NotesClear);
    assert_eq!(app.recent.len(), 6);
    let _ = open_list(&mut app);
    let _ = app.update(Message::RecentToggle);
    let _ = app.update(Message::NoteMenu(5));
    let _ = app.update(Message::NoteRule(Some(preferences::AppRule::ListOnly)));
    // The only thing sent for saving is the app's rule: no titles, no bodies.
    let mut sent = Vec::new();
    while let Ok(command) = rx.try_recv() {
        sent.push(command);
    }
    assert!(
        matches!(sent.as_slice(), [Command::NoteRule { app, entry, rule: Some(preferences::AppRule::ListOnly) }]
            if app == "Slack" && entry == SLACK),
        "{sent:?}"
    );
    let saved = serde_json::to_string(&sent).unwrap()
        + &serde_json::to_string(&app.snapshot.preferences).unwrap();
    for secret in ["Running late", "Payments are down", "Copy finished", "Ana"] {
        assert!(!saved.contains(secret), "{secret} in {saved}");
    }
}
#[test]
fn app_rules_mute_always_show_and_list_only() {
    use preferences::AppRule;
    let (mut app, mut snapshot) = revealing();
    // Right-click on a row, a group or the More button offers the rules.
    for id in 1..=3 {
        notify(&mut app, slack(id, &format!("Message {id}"), ""));
        close(&mut app);
    }
    notify(&mut app, note(4, "Files", "folder-symbolic", "Copy finished", ""));
    close(&mut app);
    let mut ui = open_list(&mut app);
    assert!(app.listing());
    let group = format!("activity-note-group-{SLACK}");
    let menu = |pressed: Vec<Message>| {
        pressed.iter().find_map(|m| match m {
            Message::NoteMenu(id) => Some(*id),
            _ => None,
        })
    };
    assert_eq!(menu(right_click(&mut ui, &app, &group)), Some(3));
    assert_eq!(menu(right_click(&mut ui, &app, "activity-note-row-4")), Some(4));
    assert!(matches!(
        click(&mut ui, &app, &format!("activity-note-group-more-{SLACK}")).as_slice(),
        [Message::NoteMenu(3)]
    ));
    assert!(matches!(
        click(&mut ui, &app, "activity-note-row-more-4").as_slice(),
        [Message::NoteMenu(4)]
    ));
    let _ = app.update(Message::NoteMenu(3));
    let mut ui = regrow(&mut app);
    assert!(ui.duplicate_ids(app.capsule(true)).is_empty());
    let text = ui.all_text(app.capsule(true));
    for line in ["Slack", "Mute this app", "Always show", "List only, don’t pop up", "Default"] {
        assert!(text.iter().any(|t| t == line), "{line}: {text:?}");
    }
    for id in ["activity-rule-back", "activity-rule-mute", "activity-rule-reset"] {
        let b = ui.bounds(app.capsule(true), id);
        assert!(b.y + b.height <= app.popup_height(), "{id}: {b:?}");
    }
    // Escape leaves the menu first, then the island.
    let escape = || Message::Key(keyboard::Key::Named(keyboard::key::Named::Escape));
    let _ = app.update(escape());
    assert!(app.menu.is_none() && app.popup.is_some());
    // Mute: the app's notifications go, and its arrivals only count.
    let _ = app.update(Message::NoteMenu(3));
    assert!(matches!(click(&mut ui, &app, "activity-rule-mute").as_slice(), [Message::NoteRule(Some(AppRule::Mute))]));
    let _ = app.update(Message::NoteRule(Some(AppRule::Mute)));
    assert_eq!(app.snapshot.preferences.rule_for("Slack", SLACK), Some(AppRule::Mute));
    assert!(app.notes.iter().all(|n| n.app != "Slack"));
    assert_eq!(app.muted, 3);
    // The desktop's echo without the rule yet does not undo it.
    let _ = app.update(Message::Backend(ClientEvent::Snapshot(snapshot.clone())));
    assert_eq!(app.snapshot.preferences.rule_for("Slack", SLACK), Some(AppRule::Mute));
    snapshot.preferences.app_rules = app.snapshot.preferences.app_rules.clone();
    let _ = app.update(Message::Backend(ClientEvent::Snapshot(snapshot.clone())));
    close(&mut app);
    notify(&mut app, slack(5, "Muted", ""));
    assert!(app.popup.is_none() && app.note(5).is_none() && app.muted == 4);
    let _ = app.update(Message::NoteDismiss(4));
    assert!(app.recent.iter().all(|r| r.note.app != "Slack"));
    let _ = app.update(Message::ShowRecent);
    let ui_text = regrow(&mut app).all_text(app.capsule(true));
    assert!(ui_text.iter().any(|t| t == "4 from muted apps"), "{ui_text:?}");
    close(&mut app);
    // Always show: shown even when the island only lists, but not in
    // Do Not Disturb unless urgent.
    snapshot.preferences.reveals = false;
    preferences_rule(&mut app, &mut snapshot, "Mail", Some(AppRule::Always));
    notify(&mut app, note(6, "Chat", "", "Listed", ""));
    assert!(app.popup.is_none() && app.note(6).is_some());
    notify(&mut app, note(7, "Mail", "", "Shown", ""));
    settle(&mut app);
    assert!(app.popup.is_some() && app.kind == Kind::Notification(7));
    close(&mut app);
    snapshot.dnd = true;
    let _ = app.update(Message::Backend(ClientEvent::Snapshot(snapshot.clone())));
    notify(&mut app, note(8, "Mail", "", "Quiet", ""));
    settle(&mut app);
    assert!(app.popup.is_none());
    let mut urgent = note(9, "Mail", "", "Loud", "");
    urgent.urgent = true;
    notify(&mut app, urgent);
    settle(&mut app);
    assert!(app.popup.is_some() && app.kind == Kind::Notification(9));
    close(&mut app);
    snapshot.dnd = false;
    // List only: listed, never shown.
    snapshot.preferences.reveals = true;
    preferences_rule(&mut app, &mut snapshot, "Calendar", Some(AppRule::ListOnly));
    notify(&mut app, note(10, "Calendar", "", "Listed only", ""));
    settle(&mut app);
    assert!(app.popup.is_none() && app.note(10).is_some());
    // Reset: back to the default, which shows it.
    preferences_rule(&mut app, &mut snapshot, "Calendar", None);
    assert!(app.snapshot.preferences.rule_for("Calendar", "").is_none());
    notify(&mut app, note(11, "Calendar", "", "Shown again", ""));
    settle(&mut app);
    assert!(app.popup.is_some() && app.kind == Kind::Notification(11));
}
/// Lets the pause after the island closed pass, so what waits shows.
fn settle(app: &mut Applet) {
    if app.popup.is_none() && !app.pending.is_empty() {
        wake(app, PENDING_PAUSE + NOTE_GAP);
    }
}
/// Sets a rule the way the desktop echoes it back.
fn preferences_rule(
    app: &mut Applet,
    snapshot: &mut Snapshot,
    name: &str,
    rule: Option<preferences::AppRule>,
) {
    snapshot.preferences.set_rule(name, "", rule);
    app.rules_wish = None;
    let _ = app.update(Message::Backend(ClientEvent::Snapshot(snapshot.clone())));
    assert_eq!(app.snapshot.preferences.rule_for(name, ""), rule);
}
#[test]
fn notification_groups_recent_and_rules_render_for_review() {
    use cosmic::applet::cosmic_panel_config::CosmicPanelBackground;
    let root = std::env::var_os("HALCYON_ACTIVITY_EVIDENCE").map(std::path::PathBuf::from);
    let scale = 2f32;
    type Setup = fn(&mut Applet);
    let states: [(&str, Setup); 5] = [
        ("grouped", |_| {}),
        ("group-open", |app| {
            let _ = app.update(Message::GroupToggle(SLACK.into()));
        }),
        ("recent", |app| {
            let _ = app.update(Message::NoteDismiss(1));
            let _ = app.update(Message::Notes(notifications::Event::Closed(6)));
        }),
        ("recent-open", |app| {
            let _ = app.update(Message::NoteDismiss(1));
            let _ = app.update(Message::Notes(notifications::Event::Closed(6)));
            app.muted = 2;
            let _ = app.update(Message::RecentToggle);
        }),
        ("menu", |app| {
            let _ = app.update(Message::NoteMenu(5));
        }),
    ];
    for (theme_name, theme, background, wall) in [
        ("dark", cosmic::Theme::dark(), CosmicPanelBackground::Dark, ([46., 58., 74.], [96., 84., 110.])),
        ("light", cosmic::Theme::light(), CosmicPanelBackground::Light, ([214., 196., 170.], [168., 186., 204.])),
    ] {
        for (name, setup) in states {
            let mut app = grouped();
            app.core.applet.background = background.clone();
            let _ = open_list(&mut app);
            setup(&mut app);
            let mut ui = regrow(&mut app);
            assert!(app.listing(), "{name}");
            assert!(ui.duplicate_ids(app.capsule(true)).is_empty(), "{name}");
            let frame = app.frame();
            let height = app.popup_height();
            let edge = (frame.popup_width() - WIDTH) / 2.;
            for id in [
                "activity-note-clear",
                "activity-recent-toggle",
                "activity-recent-clear",
                "activity-rule-reset",
            ] {
                if let Some(b) = ui.try_bounds(app.capsule(true), id) {
                    assert!(b.x >= edge - 0.5 && b.x + b.width <= frame.popup_width() - edge + 0.5, "{name}: {id} {b:?}");
                    assert!(b.y + b.height <= height + 0.5, "{name}: {id} {b:?}");
                }
            }
            let pixels = ui.render_scaled(app.capsule(true), &theme, scale as f64);
            let Some(root) = &root else { continue };
            save_scene(
                &root.join(format!("inbox-{name}-{theme_name}.png")),
                &pixels,
                ((frame.popup_width() * scale) as usize, (height * scale) as usize),
                &frame,
                app.palette().surface,
                wall,
                scale,
            );
        }
    }
}

/// An island whose commands can be read back.
fn listening() -> (Applet, tokio::sync::mpsc::Receiver<Command>) {
    let mut app = app();
    let (tx, rx) = tokio::sync::mpsc::channel(16);
    app.commands = Some(tx);
    (app, rx)
}
fn timed(app: &mut Applet, position_ms: i64) {
    app.snapshot.music = Some(music());
    app.snapshot.music_seek = true;
    app.snapshot.timing = Some(bridge::MusicTiming {
        key: "track-1".into(),
        position_ms,
        length_ms: 180_000,
        rate: 1.,
    });
    app.music_sampled = Instant::now();
    app.kind = Kind::Music;
}
#[test]
fn live_timer_picks_a_length_starts_pauses_and_cancels() {
    let (mut app, mut rx) = listening();
    app.kind = Kind::Idle;
    let _ = app.update(Message::Live(live::Msg::TimerPicker));
    assert_eq!(app.ui.view, live::View::Picker);
    assert!(app.popup.is_some(), "the picker opens the island");
    assert_eq!(app.popup_extra, app.live_extra());
    assert!(app.popup_extra > 0., "the picker has room for its presets");
    assert_eq!(app.live_heading(1.).as_deref(), Some("New timer"));
    let _ = app.update(Message::Live(live::Msg::TimerStep(-1)));
    assert_eq!(app.ui.minutes, 10);
    let _ = app.update(Message::Live(live::Msg::TimerStep(1)));
    let _ = app.update(Message::Live(live::Msg::TimerStep(1)));
    assert_eq!(app.ui.minutes, 20);

    let _ = app.update(Message::Live(live::Msg::TimerStart(5)));
    assert!(matches!(
        rx.try_recv(),
        Ok(Command::TimerStart { ms: 300_000 })
    ));
    assert_eq!((app.kind, app.ui.view), (Kind::Timer, live::View::Card));
    assert!(app.timer_live() && app.live_panel());
    assert_eq!(app.live_heading(1.).as_deref(), Some("Timer"));
    let content = app.content();
    assert!(
        content.title == "5:00" || content.title == "4:59",
        "{}",
        content.title
    );
    assert!(content.meta.starts_with("Ends at "), "{}", content.meta);
    assert!(app.eligible_kinds().contains(&Kind::Timer));

    let _ = app.update(Message::Live(live::Msg::TimerPause));
    assert!(matches!(rx.try_recv(), Ok(Command::TimerPause)));
    assert!(app.timer().is_some_and(|t| t.paused()));
    assert_eq!(app.content().meta, "Paused");
    let _ = app.update(Message::Live(live::Msg::TimerResume));
    assert!(matches!(rx.try_recv(), Ok(Command::TimerResume)));
    assert!(app.timer().is_some_and(|t| t.running()));
    let _ = app.update(Message::Live(live::Msg::TimerAdd));
    assert!(matches!(
        rx.try_recv(),
        Ok(Command::TimerAdd { ms: 60_000 })
    ));
    assert!(app.timer().is_some_and(|t| t.total_ms == 360_000));

    let _ = app.update(Message::Live(live::Msg::TimerCancel));
    assert!(matches!(rx.try_recv(), Ok(Command::TimerCancel)));
    assert!(app.timer().is_none());
    assert!(!app.eligible_kinds().contains(&Kind::Timer));
}
#[test]
fn live_escape_leaves_a_live_view_before_closing() {
    let mut app = app();
    let _ = app.update(Message::Live(live::Msg::TimerPicker));
    let _ = app.update(Message::Key(keyboard::Key::Named(
        keyboard::key::Named::Escape,
    )));
    assert_eq!(app.ui.view, live::View::Card);
    assert!(app.popup.is_some(), "the first Escape only goes back");
}
#[test]
fn a_finished_timer_reveals_past_the_throttle_and_waits_but_respects_do_not_disturb() {
    let (mut app, mut snapshot) = revealing();
    let now = chrono::Utc::now().timestamp_millis();
    snapshot.timer = Some(timer::Timer::start(60_000, now - 120_000));
    let _ = app.update(Message::Backend(ClientEvent::Snapshot(snapshot.clone())));
    // Counting down is quiet.
    assert!(app.popup.is_none());
    // A notification just showed, so an ordinary reveal would wait.
    app.last_reveal = Some(Instant::now());
    let mut done = snapshot.timer.unwrap();
    assert!(done.expire(now));
    snapshot.timer = Some(done);
    let _ = app.update(Message::Backend(ClientEvent::Snapshot(snapshot.clone())));
    assert!(app.popup.is_some(), "the finished timer shows at once");
    assert_eq!(app.kind, Kind::Timer);
    assert_eq!(app.content().title, "Timer done");
    assert_eq!(app.eligible_kinds().first(), Some(&Kind::Timer));
    wake(&mut app, Duration::from_secs(30));
    assert!(app.popup.is_some(), "it waits for Dismiss or +1 min");
    assert_eq!(app.motion.target(), 1.);
    // A repeat of the same snapshot does not reveal again.
    close(&mut app);
    let _ = app.update(Message::Backend(ClientEvent::Snapshot(snapshot.clone())));
    assert!(app.popup.is_none());

    let (mut app, mut snapshot) = revealing();
    snapshot.dnd = true;
    let _ = app.update(Message::Backend(ClientEvent::Snapshot(snapshot.clone())));
    snapshot.timer = Some(done);
    let _ = app.update(Message::Backend(ClientEvent::Snapshot(snapshot)));
    assert!(
        app.popup.is_none(),
        "Do Not Disturb keeps the island closed"
    );
}
#[test]
fn transfers_from_notifications_show_progress_then_done_then_retract() {
    let mut app = app();
    let _ = app.update(Message::Notes(notifications::Event::Ready(true)));
    let mut copy = note(7, "Files", "folder-symbolic", "Copying", "3 files");
    copy.progress = Some(40);
    notify(&mut app, copy.clone());
    assert!(!app.ui.jobs.is_empty());
    assert!(app.eligible_kinds().contains(&Kind::Transfer));
    app.kind = Kind::Transfer;
    let content = app.content();
    assert_eq!(
        (content.title.as_str(), content.cue.as_str()),
        ("Files", "40%")
    );
    assert_eq!(
        content.meta, "In progress",
        "no estimate before a steady rate"
    );
    copy.progress = Some(99);
    notify(&mut app, copy);
    let _ = app.update(Message::Notes(notifications::Event::Closed(7)));
    app.kind = Kind::Transfer;
    assert_eq!(app.content().title, "Done");
    assert!(
        app.ui
            .jobs
            .lead(Instant::now())
            .is_some_and(|j| j.done_at.is_some())
    );
    let _ = app.update(Message::Wake(transfers::DONE + Duration::from_secs(1)));
    assert!(app.ui.jobs.is_empty(), "Done retracts after a few seconds");
    assert!(!app.eligible_kinds().contains(&Kind::Transfer));
}
#[test]
fn progress_steps_tell_start_update_and_finish_apart() {
    use super::reactive::{Step, progress_step};
    assert_eq!(progress_step(None, None), Step::Plain);
    assert_eq!(progress_step(Some(None), None), Step::Plain);
    assert_eq!(progress_step(None, Some(5)), Step::Start);
    assert_eq!(progress_step(Some(None), Some(5)), Step::Start);
    assert_eq!(progress_step(Some(Some(5)), Some(6)), Step::Update);
    assert_eq!(progress_step(Some(Some(5)), Some(100)), Step::Finish);
    assert_eq!(progress_step(Some(Some(5)), None), Step::Finish);
    assert_eq!(progress_step(Some(Some(100)), Some(100)), Step::Update);
    assert_eq!(progress_step(Some(Some(100)), Some(3)), Step::Update);
    assert_eq!(progress_step(None, Some(100)), Step::Plain);
}
fn progress_note(id: u32, app: &str, value: u8) -> notifications::Note {
    let mut copy = note(id, app, "folder-symbolic", "Copying", "3 files");
    copy.progress = Some(value);
    copy
}
#[test]
fn progress_notifications_update_in_place_and_reveal_only_at_start_and_end() {
    let (mut app, _snapshot) = revealing();
    notify(&mut app, note(1, "Mail", "", "Older", ""));
    settle(&mut app);
    close(&mut app);
    wake(&mut app, NOTE_GAP);
    // Start: revealed once, as any arrival.
    notify(&mut app, progress_note(7, "Files", 10));
    settle(&mut app);
    assert!(app.popup.is_some() && app.kind == Kind::Notification(7));
    close(&mut app);
    wake(&mut app, NOTE_GAP);
    notify(&mut app, note(8, "Chat", "", "Newer", ""));
    settle(&mut app);
    close(&mut app);
    let order = |app: &Applet| app.notes.iter().map(|n| n.id).collect::<Vec<_>>();
    assert_eq!(order(&app), [8, 7, 1]);
    // Every new value: in place, in the list, the group and the job, and
    // never revealed, however long since the last reveal.
    for value in [25, 50, 75, 90] {
        wake(&mut app, Duration::from_secs(60));
        notify(&mut app, progress_note(7, "Files", value));
        settle(&mut app);
        assert!(app.popup.is_none(), "{value}% must not reveal");
        assert!(
            !app.pending.contains(&7),
            "{value}% must not wait to reveal"
        );
        assert_eq!(order(&app), [8, 7, 1], "{value}% keeps its place");
        assert_eq!(app.note(7).and_then(|n| n.progress), Some(value));
        let job = app.ui.jobs.for_note(7).expect("the job follows it");
        assert!((job.fraction - f64::from(value) / 100.).abs() < 1e-9);
    }
    assert_eq!(app.group_ids(&app.note(7).unwrap().app_key()), [7]);
    // Finish: revealed once more, as the live activity's Done.
    wake(&mut app, Duration::from_secs(60));
    notify(&mut app, progress_note(7, "Files", 100));
    assert!(app.popup.is_some() && app.kind == Kind::Transfer);
    app.kind = Kind::Transfer;
    assert_eq!(app.content().title, "Done");
    assert_eq!(order(&app), [8, 7, 1]);
    close(&mut app);
    // A repeat of the end does not reveal again.
    wake(&mut app, Duration::from_secs(60));
    notify(&mut app, progress_note(7, "Files", 100));
    settle(&mut app);
    assert!(app.popup.is_none());
    // Ending by dropping the value, with no job left to show Done: the
    // notification itself is revealed once.
    app.ui.jobs = transfers::Tracker::default();
    wake(&mut app, Duration::from_secs(60));
    notify(&mut app, progress_note(9, "Files", 30));
    settle(&mut app);
    close(&mut app);
    app.ui.jobs = transfers::Tracker::default();
    wake(&mut app, Duration::from_secs(60));
    notify(&mut app, note(9, "Files", "", "Copied", "3 files"));
    settle(&mut app);
    assert!(app.popup.is_some() && app.kind == Kind::Notification(9));
}
#[test]
fn progress_follows_each_apps_rule() {
    use preferences::AppRule;
    let (mut app, mut snapshot) = revealing();
    // Mute: neither listed nor followed.
    preferences_rule(&mut app, &mut snapshot, "Files", Some(AppRule::Mute));
    notify(&mut app, progress_note(7, "Files", 10));
    assert!(app.note(7).is_none() && app.ui.jobs.is_empty() && app.popup.is_none());
    // List only: listed and followed, never revealed, not even when done.
    preferences_rule(&mut app, &mut snapshot, "Files", Some(AppRule::ListOnly));
    notify(&mut app, progress_note(7, "Files", 10));
    settle(&mut app);
    assert!(app.note(7).is_some() && app.ui.jobs.for_note(7).is_some());
    assert!(app.popup.is_none());
    notify(&mut app, progress_note(7, "Files", 100));
    settle(&mut app);
    assert!(app.ui.jobs.for_note(7).is_some_and(|j| j.done_at.is_some()));
    assert!(app.popup.is_none(), "List only never reveals Done");
    // Always show: start and Done reveal even with reveals off.
    snapshot.preferences.reveals = false;
    preferences_rule(&mut app, &mut snapshot, "Files", Some(AppRule::Always));
    app.ui.jobs = transfers::Tracker::default();
    notify(&mut app, progress_note(8, "Files", 10));
    settle(&mut app);
    assert!(app.popup.is_some() && app.kind == Kind::Notification(8));
    close(&mut app);
    wake(&mut app, Duration::from_secs(60));
    notify(&mut app, progress_note(8, "Files", 100));
    assert!(app.popup.is_some() && app.kind == Kind::Transfer);
    close(&mut app);
    // Without a rule and with reveals off, Done stays quiet.
    preferences_rule(&mut app, &mut snapshot, "Files", None);
    app.ui.jobs = transfers::Tracker::default();
    notify(&mut app, progress_note(9, "Files", 10));
    wake(&mut app, Duration::from_secs(60));
    notify(&mut app, progress_note(9, "Files", 100));
    assert!(app.popup.is_none());
    // Muting an app with a running job drops the job too.
    snapshot.preferences.reveals = true;
    app.ui.jobs = transfers::Tracker::default();
    notify(&mut app, progress_note(10, "Files", 20));
    assert!(app.ui.jobs.for_note(10).is_some());
    preferences_rule(&mut app, &mut snapshot, "Files", Some(AppRule::Mute));
    assert!(app.ui.jobs.is_empty() && app.note(10).is_none());
}
fn download(bytes: u64, rate: Option<f64>, paused: bool, finished: bool) -> transfers::Update {
    let name = "ubuntu-26.04-desktop-amd64.iso";
    let dir = std::path::Path::new("/home/user/Downloads");
    transfers::Update {
        key: "download:41".into(),
        app: "Firefox".into(),
        visible: Some(true),
        finished,
        file: Some(transfers::FileState {
            name: name.into(),
            bytes,
            rate,
            paused,
            path: if finished {
                dir.join(name)
            } else {
                dir.join(format!("{name}.part"))
            },
        }),
        owners: vec![
            ("Firefox".into(), "firefox".into()),
            ("Firefox".into(), "org.mozilla.firefox".into()),
        ],
        ..Default::default()
    }
}
fn downloading(app: &mut Applet) {
    let _ = app.update(Message::Live(live::Msg::Transfer(download(
        1_240_000_000,
        Some(21_400_000.),
        false,
        false,
    ))));
    app.kind = Kind::Transfer;
}
fn downloaded(app: &mut Applet) {
    downloading(app);
    let _ = app.update(Message::Live(live::Msg::Transfer(download(
        6_114_656_256,
        None,
        false,
        true,
    ))));
    app.kind = Kind::Transfer;
}
#[test]
fn downloads_show_size_and_rate_without_a_percentage_then_done() {
    use preferences::AppRule;
    let (mut app, mut snapshot) = revealing();
    downloading(&mut app);
    assert!(app.popup.is_none(), "a starting download does not reveal");
    let content = app.content();
    assert_eq!(content.title, "Downloading ubuntu-26.04-desktop-amd64.iso");
    assert_eq!(content.cue, "", "no percentage without a total");
    assert_eq!(content.meta, "1.2 GB · 21 MB/s");
    assert_eq!(content.progress, Some(transfers::SWEEP));
    let job = app.ui.jobs.lead(Instant::now()).unwrap();
    assert!(job.indeterminate() && job.remaining(Instant::now()).is_none());
    // Paused: said so, with no rate.
    let _ = app.update(Message::Live(live::Msg::Transfer(download(
        1_240_000_000,
        None,
        true,
        false,
    ))));
    let content = app.content();
    assert_eq!(
        (content.cue.as_str(), content.meta.as_str()),
        ("Paused", "1.2 GB")
    );
    // Renamed to its final name: Done, revealed, with the folder and file.
    let _ = app.update(Message::Live(live::Msg::Transfer(download(
        6_114_656_256,
        None,
        false,
        true,
    ))));
    assert!(app.popup.is_some() && app.kind == Kind::Transfer);
    let content = app.content();
    assert_eq!(
        (content.title.as_str(), content.meta.as_str()),
        ("Done", "ubuntu-26.04-desktop-amd64.iso · 6.1 GB")
    );
    let mut ui = regrow(&mut app);
    let text = ui.all_text(app.capsule(true));
    assert!(text.iter().any(|t| t == "Open folder"), "{text:?}");
    assert!(text.iter().any(|t| t == "Open"), "{text:?}");
    assert!(matches!(
        click(&mut ui, &app, "activity-transfer-folder").as_slice(),
        [Message::Live(live::Msg::TransferOpenFolder)]
    ));
    close(&mut app);
    // List only for Firefox: followed, but Done is not revealed.
    preferences_rule(&mut app, &mut snapshot, "Firefox", Some(AppRule::ListOnly));
    app.ui.jobs = transfers::Tracker::default();
    wake(&mut app, Duration::from_secs(60));
    downloaded(&mut app);
    assert!(app.popup.is_none() && !app.ui.jobs.is_empty());
    // Muted: not followed at all.
    preferences_rule(&mut app, &mut snapshot, "Firefox", Some(AppRule::Mute));
    assert!(app.ui.jobs.is_empty(), "muting drops the job");
    downloading(&mut app);
    assert!(app.ui.jobs.is_empty());
    // Deleted or stalled: the job goes without a word.
    preferences_rule(&mut app, &mut snapshot, "Firefox", None);
    downloading(&mut app);
    let mut gone = download(0, None, false, false);
    gone.file = None;
    gone.visible = Some(false);
    let _ = app.update(Message::Live(live::Msg::Transfer(gone)));
    assert!(app.ui.jobs.is_empty() && app.popup.is_none());
}
#[test]
fn music_seek_moves_by_keyboard_within_the_track() {
    let (mut app, mut rx) = listening();
    timed(&mut app, 2_000);
    app.popup = Some(window::Id::unique());
    app.held = true;
    assert!(app.scrubbing());
    let left = keyboard::Key::Named(keyboard::key::Named::ArrowLeft);
    let right = keyboard::Key::Named(keyboard::key::Named::ArrowRight);
    let _ = app.update(Message::Key(left));
    assert!(matches!(
        rx.try_recv(),
        Ok(Command::Seek { position_ms: 0, .. })
    ));
    let _ = app.update(Message::Key(right.clone()));
    match rx.try_recv() {
        Ok(Command::Seek { key, position_ms }) => {
            assert_eq!(key, "track-1");
            assert!((5_000..5_200).contains(&position_ms), "{position_ms}");
        }
        other => panic!("{other:?}"),
    }
    // Never past the end, so the player does not skip ahead.
    app.ui.seek = None;
    timed(&mut app, 179_500);
    let _ = app.update(Message::Key(right.clone()));
    assert!(matches!(
        rx.try_recv(),
        Ok(Command::Seek {
            position_ms: 179_000,
            ..
        })
    ));
    // A player that cannot seek shows no scrubber and takes no keys.
    app.snapshot.music_seek = false;
    assert!(!app.scrubbing());
    let _ = app.update(Message::Key(right));
    assert!(rx.try_recv().is_err());
}
#[test]
fn output_list_regrows_the_open_island_to_fit() {
    let mut app = app();
    app.snapshot.music = Some(music());
    let _ = app.update(Message::Live(live::Msg::Outputs));
    let first = app.popup.expect("the outputs open the island");
    assert_eq!(app.ui.view, live::View::Outputs);
    assert!(app.ui.sinks_loading);
    let sink = |index, name: &str, label: &str, default| outputs::Sink {
        index,
        name: name.into(),
        label: label.into(),
        icon: "audio-speakers-symbolic",
        default,
    };
    let sinks: Vec<_> = (0..5)
        .map(|i| sink(i, &format!("sink-{i}"), &format!("Speaker {i}"), i == 0))
        .collect();
    let _ = app.update(Message::Live(live::Msg::OutputsLoaded(Ok(sinks.clone()))));
    assert!(!app.ui.sinks_loading);
    assert_eq!(
        app.promote_popup,
        Some(first),
        "a longer list reopens taller"
    );
    let _ = app.update(Message::PopupGone(first));
    assert!(app.popup.is_some_and(|id| id != first));
    assert_eq!(app.popup_extra, app.live_extra());
    let _ = app.update(Message::Live(live::Msg::OutputPick(sinks[3].clone())));
    assert_eq!(app.ui.view, live::View::Card);
    let picked: Vec<_> = app
        .ui
        .sinks
        .iter()
        .filter(|s| s.default)
        .map(|s| s.index)
        .collect();
    assert_eq!(picked, [3], "the choice shows before pactl answers");
}
#[test]
fn live_state_round_trips_through_the_bridge() {
    let mut snapshot = app().snapshot;
    snapshot.timer = Some(timer::Timer::start(90_000, 1_000));
    snapshot.music_seek = true;
    snapshot.music_tint = Some([200, 40, 90]);
    let text = serde_json::to_string(&snapshot).unwrap();
    assert_eq!(serde_json::from_str::<Snapshot>(&text).unwrap(), snapshot);
    for command in [
        Command::Seek {
            key: "k".into(),
            position_ms: 42,
        },
        Command::TimerStart { ms: 60_000 },
        Command::TimerAdd { ms: 60_000 },
        Command::TimerPause,
        Command::TimerResume,
        Command::TimerCancel,
    ] {
        let text = serde_json::to_string(&command).unwrap();
        let back: Command = serde_json::from_str(&text).unwrap();
        assert_eq!(serde_json::to_string(&back).unwrap(), text);
    }
}
#[test]
fn the_island_style_round_trips_and_old_choices_stay_readable() {
    use preferences::{IslandStyle, Preferences};
    // Saved before the style existed: still loads, on the theme's colours.
    let old: Preferences = serde_json::from_str(r#"{"media":true,"lead_minutes":10}"#).unwrap();
    assert_eq!(old.style, IslandStyle::Theme);
    assert_eq!(old.lead_minutes, 10);
    let dark: Preferences = serde_json::from_str(r#"{"style":"dark"}"#).unwrap();
    assert_eq!(dark.style, IslandStyle::Dark);
    assert!(serde_json::to_string(&dark).unwrap().contains(r#""style":"dark""#));
    for style in IslandStyle::ALL {
        assert_eq!(IslandStyle::ALL[style.index()], style);
        let text = format!(r#"{{"style":"{}"}}"#, style.key());
        assert_eq!(serde_json::from_str::<Preferences>(&text).unwrap().style, style);
    }
    // Through the bridge, and into the palette the views draw with.
    let mut app = app();
    app.snapshot.preferences.style = IslandStyle::Dark;
    let text = serde_json::to_string(&app.snapshot).unwrap();
    let back = serde_json::from_str::<Snapshot>(&text).unwrap();
    assert_eq!(back.preferences.style, IslandStyle::Dark);
    assert_eq!(app.palette(), palette::Palette::dark());
    app.snapshot.preferences.style = IslandStyle::Theme;
    assert!(!app.palette().own_pill);
    // An older snapshot without the newer fields still reads.
    let mut value: serde_json::Value = serde_json::from_str(&text).unwrap();
    value["preferences"].as_object_mut().unwrap().remove("style");
    value.as_object_mut().unwrap().remove("music_art");
    let old = serde_json::from_value::<Snapshot>(value).unwrap();
    assert_eq!(old.preferences.style, IslandStyle::Theme);
}
/// A copy at 62% whose rate has been steady long enough for an estimate.
fn copying(app: &mut Applet) {
    let now = Instant::now();
    for (i, fraction) in [0.30, 0.38, 0.46, 0.54, 0.62].into_iter().enumerate() {
        let at = now - Duration::from_secs(4 * (4 - i as u64));
        app.ui.jobs.update(
            transfers::Update {
                key: "files".into(),
                app: "Files".into(),
                desktop: Some("com.system76.CosmicFiles.desktop".into()),
                note: None,
                progress: Some(fraction),
                visible: Some(true),
                ..Default::default()
            },
            at,
        );
    }
    app.kind = Kind::Transfer;
}
/// Pictures of the progress, timer, scrubber and output states in both
/// island styles, written only when evidence is asked for. Every control
/// stays inside the card.
#[test]
fn live_activities_render_for_review() {
    use cosmic::applet::cosmic_panel_config::CosmicPanelBackground;
    use cosmic::iced::Size;
    let Some(root) = std::env::var_os("HALCYON_ACTIVITY_EVIDENCE").map(std::path::PathBuf::from)
    else {
        return;
    };
    let root = root.join("island-live");
    let scale = 2f32;
    type Setup = fn(&mut Applet);
    let states: [(&str, f32, &[&str], Setup); 13] = [
        ("progress-collapsed", 0., &[], copying),
        ("downloading-collapsed", 0., &[], downloading),
        (
            "downloading",
            1.,
            &["activity-transfer-folder"],
            downloading,
        ),
        (
            "download-done",
            1.,
            &["activity-transfer-folder", "activity-transfer-open"],
            downloaded,
        ),
        ("progress-expanded", 1., &[], copying),
        (
            "progress-done",
            1.,
            &["activity-transfer-open", "activity-transfer-dismiss"],
            |app| {
                copying(app);
                app.ui.jobs.update(
                    transfers::Update {
                        key: "files".into(),
                        app: "Files".into(),
                        desktop: Some("com.system76.CosmicFiles.desktop".into()),
                        note: None,
                        progress: Some(1.),
                        visible: Some(false),
                        ..Default::default()
                    },
                    Instant::now(),
                );
            },
        ),
        (
            "timer-picker",
            1.,
            &[
                "activity-timer-1",
                "activity-timer-25",
                "activity-timer-less",
                "activity-timer-more",
                "activity-timer-start",
            ],
            |app| {
                app.kind = Kind::Idle;
                app.ui.view = live::View::Picker;
            },
        ),
        ("timer-collapsed", 0., &[], |app| {
            let now = chrono::Utc::now().timestamp_millis();
            app.snapshot.timer = Some(timer::Timer::start(5 * 60_000, now - 72_000));
            app.kind = Kind::Timer;
        }),
        (
            "timer-expanded",
            1.,
            &[
                "activity-timer-add",
                "activity-timer-pause",
                "activity-timer-cancel",
            ],
            |app| {
                let now = chrono::Utc::now().timestamp_millis();
                app.snapshot.timer = Some(timer::Timer::start(5 * 60_000, now - 72_000));
                app.kind = Kind::Timer;
            },
        ),
        (
            "timer-done",
            1.,
            &["activity-timer-dismiss", "activity-timer-again"],
            |app| {
                let now = chrono::Utc::now().timestamp_millis();
                let mut t = timer::Timer::start(60_000, now - 61_000);
                t.expire(now);
                app.snapshot.timer = Some(t);
                app.kind = Kind::Timer;
            },
        ),
        ("music-scrubber", 1., &["activity-outputs"], |app| {
            timed(app, 72_000);
            app.snapshot.music_tint = Some([226, 88, 64]);
        }),
        ("music-collapsed", 0., &[], |app| {
            timed(app, 72_000);
            app.snapshot.music_tint = Some([226, 88, 64]);
        }),
        ("outputs", 1., &["activity-output-1"], |app| {
            timed(app, 72_000);
            app.snapshot.music_tint = Some([226, 88, 64]);
            let sink = |index, label: &str, icon, default| outputs::Sink {
                index,
                name: format!("sink-{index}"),
                label: label.into(),
                icon,
                default,
            };
            app.ui.sinks = vec![
                sink(0, "Speakers", "audio-speakers-symbolic", false),
                sink(1, "WH-1000XM5", "audio-headphones-symbolic", true),
                sink(2, "LG HDR 4K", "video-display-symbolic", false),
            ];
            app.ui.view = live::View::Outputs;
        }),
    ];
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
        for (name, p, ids, setup) in states {
            let mut app = app();
            setup(&mut app);
            app.core.applet.background = background.clone();
            app.popup = Some(window::Id::unique());
            app.held = true;
            app.popup_extra = app.live_extra();
            let frame = app.frame();
            let (w, h) = (frame.popup_width(), app.popup_height());
            let mut ui = crate::native_test::Native::new(Size::new(w, h));
            app.motion.retarget(p, false, Instant::now());
            assert!(ui.duplicate_ids(app.capsule(true)).is_empty(), "{name}");
            for id in ids {
                let b = ui.bounds(app.capsule(true), id);
                assert!(
                    b.x >= 0. && b.y >= 0. && b.x + b.width <= w + 0.5 && b.y + b.height <= h + 0.5,
                    "{name}: {id} {b:?} outside {w}x{h}"
                );
            }
            let pixels = ui.render_scaled(app.capsule(true), &theme, scale as f64);
            save_scene(
                &root.join(format!("{name}-{theme_name}.png")),
                &pixels,
                ((w * scale) as usize, (h * scale) as usize),
                &frame,
                app.palette().surface,
                wall,
                scale,
            );
        }
    }
}
/// An update waits for the island to be left alone, a quiet moment longer,
/// and any use puts it off again. The exit itself is never reached here: no
/// test sends Updated, which would read the session's restart ledger.
#[test]
fn update_restart_waits_for_a_quiet_island() {
    let mut app = app();
    let program = crate::upgrade::Program::running().unwrap();
    let now = Instant::now();
    assert!(app.idle(), "a fresh island is idle");
    app.arm_restart(now);
    assert_eq!(app.restart_at, None, "nothing to restart into");

    app.updated = Some((program.clone(), now));
    app.hovered = true;
    app.arm_restart(now);
    assert_eq!(app.restart_at, None, "a pointer on it puts it off");
    assert!(!app.restart_due(now + Duration::from_secs(60)));

    app.hovered = false;
    app.arm_restart(now);
    assert_eq!(app.restart_at, Some(now + RESTART_QUIET));
    // Later messages keep the first quiet moment rather than pushing it on.
    app.arm_restart(now + Duration::from_secs(1));
    assert_eq!(app.restart_at, Some(now + RESTART_QUIET));
    assert!(!app.restart_due(now + Duration::from_secs(1)));
    assert!(app.restart_due(now + RESTART_QUIET));

    // Opening it disarms; a due wake-up while it is open does nothing.
    app.popup = Some(window::Id::unique());
    app.arm_restart(now + RESTART_QUIET);
    assert_eq!(app.restart_at, None);
    assert!(!app.restart_due(now + Duration::from_secs(60)));
    let _ = app.update(Message::RestartDue(Duration::ZERO));
    assert!(app.updated.is_some(), "the update is still waiting");
    for busy in [
        |a: &mut Applet| a.held = true,
        |a: &mut Applet| a.reveal_deadline = Some(Instant::now()),
        |a: &mut Applet| a.starting = true,
    ] {
        let mut other = self::app();
        other.updated = Some((program.clone(), now));
        busy(&mut other);
        other.arm_restart(now);
        assert_eq!(other.restart_at, None);
    }

    // A backed-off update waits for its own time even when idle sooner.
    app.popup = None;
    let later = now + Duration::from_secs(30);
    app.updated = Some((program, later));
    app.arm_restart(now);
    assert_eq!(app.restart_at, Some(later));
    assert!(!app.restart_due(now + RESTART_QUIET));
}

/// A small gradient standing in for album art.
fn artwork(app: &mut Applet) {
    let (w, h) = (64usize, 64usize);
    let mut rgba = Vec::with_capacity(w * h * 4);
    for y in 0..h {
        for x in 0..w {
            rgba.extend([
                (200 + x / 2) as u8,
                (60 + y * 2) as u8,
                (90 + (x + y) / 2) as u8,
                255,
            ]);
        }
    }
    app.snapshot.music_art = tint::thumbnail(&rgba, w as u32, h as u32);
    app.snapshot.music_tint = Some([226, 88, 64]);
    app.keep_art();
}
fn system_card(app: &mut Applet, source: system::Source, title: &str, detail: &str, cue: &str) {
    let mut a = activity(source, title, detail, None);
    a.cue = cue.into();
    a.urgent = source == system::Source::Power;
    app.snapshot.system = vec![a];
    app.kind = Kind::System(source);
}
fn meeting(app: &mut Applet, starts_in_ms: i64, join: Option<&str>, color: Option<[u8; 3]>) {
    app.snapshot.calendar = Some(bridge::Calendar {
        key: "meeting".into(),
        title: "Design review".into(),
        starts_at_ms: app.now_ms + starts_in_ms,
        ends_at_ms: app.now_ms + starts_in_ms + 1_800_000,
        all_day: false,
        join: join.map(str::to_owned),
        color,
    });
    app.kind = Kind::Calendar;
}

/// Every island state in each style: Follow the theme on a light and on a
/// dark panel, and Dark. Ids listed per state must sit inside the surface.
#[test]
fn island_styles_render_for_review() {
    use super::preferences::IslandStyle;
    use cosmic::applet::cosmic_panel_config::CosmicPanelBackground;
    use cosmic::iced::Size;
    let Some(root) = std::env::var_os("HALCYON_ACTIVITY_EVIDENCE").map(std::path::PathBuf::from)
    else {
        return;
    };
    let scale = 2f32;
    type Setup = fn(&mut Applet);
    let states: &[(&str, f32, &[&str], Setup)] = &[
        ("idle-collapsed", 0., &[], |app| app.kind = Kind::Idle),
        ("idle", 1., &["activity-dnd", "activity-timer", "activity-preferences"], |app| {
            app.kind = Kind::Idle
        }),
        ("music-collapsed", 0., &[], |app| {
            timed(app, 72_000);
            artwork(app);
        }),
        (
            "music",
            1.,
            &["activity-artwork", "activity-previous", "activity-pause", "activity-next", "activity-outputs"],
            |app| {
                timed(app, 72_000);
                artwork(app);
            },
        ),
        ("music-no-art-paused", 1., &["activity-artwork", "activity-play"], |app| {
            app.snapshot.music = Some(bridge::Music {
                playing: false,
                ..music()
            });
            app.kind = Kind::Music;
        }),
        ("music-empty", 1., &["activity-music-settings"], |app| {
            app.snapshot.music = None;
            app.kind = Kind::Music;
        }),
        ("focus-collapsed", 0., &[], |app| {
            app.snapshot.focus.running = true;
            app.snapshot.focus.ready = false;
            app.snapshot.focus.ends_at_ms = Some(app.now_ms + 900_000);
            app.kind = Kind::Focus;
        }),
        ("focus-running", 1., &["activity-focus-action"], |app| {
            app.snapshot.focus.running = true;
            app.snapshot.focus.ready = false;
            app.snapshot.focus.ends_at_ms = Some(app.now_ms + 900_000);
            app.kind = Kind::Focus;
        }),
        ("focus-ready", 1., &["activity-focus-action"], |app| app.kind = Kind::Focus),
        ("focus-paused", 1., &["activity-focus-action"], |app| {
            app.snapshot.focus.ready = false;
            app.snapshot.focus.remaining_ms = 754_000;
            app.kind = Kind::Focus;
        }),
        ("focus-complete", 1., &["activity-focus-action"], |app| {
            app.snapshot.focus.done = true;
            app.kind = Kind::Focus;
        }),
        ("calendar-collapsed", 0., &[], |app| {
            meeting(app, 180_000, Some("https://meet.google.com/abc-defg-hij"), Some([0x33, 0xb6, 0x79]))
        }),
        (
            "calendar-soon-join",
            1.,
            &["activity-dismiss-calendar", "activity-open-calendar"],
            |app| meeting(app, 180_000, Some("https://meet.google.com/abc-defg-hij"), Some([0x33, 0xb6, 0x79])),
        ),
        (
            "calendar-now-no-link",
            1.,
            &["activity-dismiss-calendar", "activity-open-calendar"],
            |app| meeting(app, -600_000, None, Some([0xd5, 0x00, 0x00])),
        ),
        ("calendar-later-default-colour", 1., &["activity-open-calendar"], |app| {
            meeting(app, 12_600_000, None, None)
        }),
        ("calendar-empty", 1., &["activity-open-calendar"], |app| {
            app.snapshot.calendar = None;
            app.kind = Kind::Calendar;
        }),
        ("timer-collapsed", 0., &[], |app| {
            app.snapshot.timer = Some(timer::Timer::start(5 * 60_000, app.now_ms - 72_000));
            app.kind = Kind::Timer;
        }),
        (
            "timer",
            1.,
            &["activity-timer-add", "activity-timer-cancel", "activity-timer-pause"],
            |app| {
                app.snapshot.timer = Some(timer::Timer::start(5 * 60_000, app.now_ms - 72_000));
                app.kind = Kind::Timer;
            },
        ),
        ("timer-done", 1., &["activity-timer-dismiss", "activity-timer-again"], |app| {
            let mut t = timer::Timer::start(60_000, app.now_ms - 61_000);
            t.expire(app.now_ms);
            app.snapshot.timer = Some(t);
            app.kind = Kind::Timer;
        }),
        ("timer-picker", 1., &["activity-timer-1", "activity-timer-start"], |app| {
            app.kind = Kind::Idle;
            app.ui.view = live::View::Picker;
        }),
        ("downloading", 1., &["activity-transfer-folder"], downloading),
        ("download-done", 1., &["activity-transfer-open"], downloaded),
        ("copying-collapsed", 0., &[], copying),
        ("notification", 1., &["activity-note-dismiss"], |app| {
            notify(app, note(1, "Chat", "user-available-symbolic", "Sam", "Running five minutes late, start without me"));
            app.kind = Kind::Notification(1);
        }),
        ("notification-urgent", 1., &["activity-note-dismiss"], |app| {
            let mut n = note(9, "Power", "battery-caution-symbolic", "Battery critically low", "5% left. Plug in now.");
            n.urgent = true;
            notify(app, n);
            app.kind = Kind::Notification(9);
        }),
        ("system-power", 1., &["activity-system-settings"], |app| {
            system_card(app, system::Source::Power, "Battery low", "About 20 minutes left", "9%")
        }),
        ("system-audio", 1., &["activity-system-settings"], |app| {
            system_card(app, system::Source::Audio, "Headphones", "Now playing through WH-1000XM5", "")
        }),
        ("system-microphone", 1., &["activity-system-settings"], |app| {
            system_card(app, system::Source::Microphone, "Microphone on", "Firefox", "")
        }),
        ("system-camera", 1., &["activity-system-settings"], |app| {
            system_card(app, system::Source::Camera, "Camera on", "Video call", "")
        }),
        ("system-network", 1., &["activity-system-settings"], |app| {
            system_card(app, system::Source::Network, "Connected", "Home Wi-Fi", "")
        }),
        ("system-bluetooth", 1., &["activity-system-settings"], |app| {
            system_card(app, system::Source::Bluetooth, "Buds", "Connected", "70%")
        }),
        ("system-updates", 1., &["activity-system-settings"], |app| {
            system_card(app, system::Source::Updates, "12 updates", "Ready to install", "")
        }),
        ("camera-dot-collapsed", 0., &["activity-privacy-camera"], |app| {
            timed(app, 72_000);
            app.snapshot.system = vec![activity(system::Source::Camera, "Camera on", "", None)];
        }),
        ("chooser", 1., &["activity-choice-timer"], |app| {
            app.snapshot.music = Some(music());
            app.kind = Kind::Music;
            app.chooser = true;
        }),
        ("offline", 1., &["activity-settings", "activity-start"], |app| {
            app.commands = None;
        }),
    ];
    let styles = [
        ("theme-light", IslandStyle::Theme, cosmic::Theme::light(), CosmicPanelBackground::Light, ([214., 196., 170.], [168., 186., 204.])),
        ("theme-dark", IslandStyle::Theme, cosmic::Theme::dark(), CosmicPanelBackground::Dark, ([46., 58., 74.], [96., 84., 110.])),
        // Dark on a light panel: the island owns its surface either way.
        ("dark", IslandStyle::Dark, cosmic::Theme::light(), CosmicPanelBackground::Light, ([214., 196., 170.], [168., 186., 204.])),
    ];
    for attached in [false, true] {
        for (style_name, style, theme, background, wall) in &styles {
            for (name, p, ids, setup) in states {
                // The Standard bar gets the open cards only.
                if attached && *p < 1. {
                    continue;
                }
                let mut app = app();
                app.now_ms = chrono::Utc::now().timestamp_millis();
                let _ = app.update(Message::Notes(notifications::Event::Ready(true)));
                app.snapshot.preferences.style = *style;
                app.core.applet.background = background.clone();
                if attached {
                    app.supported = true;
                    app.panel_frame = host::Frame {
                        height: 32.,
                        padding: 4.,
                        radius: 8.,
                        attached: true,
                        opacity: 0.85,
                    };
                }
                setup(&mut app);
                app.popup = Some(window::Id::unique());
                app.held = true;
                app.popup_extra = app.live_extra();
                app.motion.retarget(*p, false, Instant::now());
                let frame = app.frame();
                let (w, h) = (frame.popup_width(), app.popup_height());
                let mut ui = crate::native_test::Native::new(Size::new(w, h));
                let label = format!("{name} {style_name} attached={attached}");
                assert!(ui.duplicate_ids(app.capsule(true)).is_empty(), "{label}");
                for id in *ids {
                    let b = ui.bounds(app.capsule(true), id);
                    assert!(
                        b.width > 0. && b.x >= 0. && b.y >= 0. && b.x + b.width <= w + 0.5 && b.y + b.height <= h + 0.5,
                        "{label}: {id} {b:?} outside {w}x{h}"
                    );
                }
                let pixels = ui.render_scaled(app.capsule(true), theme, scale as f64);
                let panel = Palette::new(&app.core.applet, IslandStyle::Theme).surface;
                save_scene(
                    &root.join(format!(
                        "{}{name}-{style_name}.png",
                        if attached { "standard-" } else { "" }
                    )),
                    &pixels,
                    ((w * scale) as usize, (h * scale) as usize),
                    &frame,
                    panel,
                    *wall,
                    scale,
                );
            }
        }
    }
}
