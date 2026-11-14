use super::*;
use cosmic::iced::futures::StreamExt;
use cosmic::iced::platform_specific::runtime::wayland::layer_surface::Action as LayerAction;
use cosmic::iced::runtime::{Action as RuntimeAction, platform_specific, task};
use std::io::{BufRead, BufReader};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

// Every test executes in a disposable child, bounded even when flock blocks the UI.
fn isolated(name: &str) -> bool {
    if std::env::var("HALCYON_ARRANGE_TEST").as_deref() == Ok(name) {
        return false;
    }
    let folder = crate::test_env::TempPath::new("arrange");
    let root = folder.root();
    for part in ["config", "data", "cache", "state", "system/cosmic"] {
        std::fs::create_dir_all(root.join(part)).unwrap();
    }
    std::fs::write(root.join("config.json"), r#"{"enabled_widgets":["weather"],"placement":{"snap":false},"widget_positions":{"weather":[0.2,0.2]},"future_option":42}"#).unwrap();
    let mut child = Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            &format!("app::layout::input_safety_tests::{name}"),
            "--nocapture",
        ])
        .env("HALCYON_ARRANGE_TEST", name)
        .env("DESKTOP_CARDS_CONFIG", root.join("config.json"))
        .env("DESKTOP_CARDS_DESKTOP_ROOT", &root)
        .env("XDG_CONFIG_HOME", root.join("config"))
        .env("XDG_DATA_HOME", root.join("data"))
        .env("XDG_CACHE_HOME", root.join("cache"))
        .env("XDG_STATE_HOME", root.join("state"))
        .env_remove("WAYLAND_DISPLAY")
        .env_remove("DISPLAY")
        .spawn()
        .unwrap();
    let deadline = Instant::now() + Duration::from_secs(20);
    loop {
        if let Some(status) = child.try_wait().unwrap() {
            assert!(status.success(), "fixture failed: {}", root.display());
            return true;
        }
        if Instant::now() >= deadline {
            child.kill().unwrap();
            child.wait().unwrap();
            panic!(
                "20s watchdog: Arrange blocked before Escape could release input; fixture {}",
                root.display()
            );
        }
        std::thread::sleep(Duration::from_millis(20));
    }
}

fn emitted(
    cards: &mut Cards,
    task: Task<Message>,
) -> (Vec<window::Id>, Vec<SctkLayerSurfaceSettings>) {
    let mut pending = vec![task];
    let mut destroyed = Vec::new();
    let mut created = Vec::new();
    while let Some(task) = pending.pop() {
        if let Some(mut stream) = task::into_stream(task) {
            cosmic::iced::futures::executor::block_on(async {
                while let Some(action) = stream.next().await {
                    match action {
                        RuntimeAction::Output(cosmic::Action::App(message)) => {
                            pending.push(cards.update(message))
                        }
                        RuntimeAction::PlatformSpecific(platform_specific::Action::Wayland(
                            platform_specific::wayland::Action::LayerSurface(action),
                        )) => match action {
                            LayerAction::Destroy(id) => destroyed.push(id),
                            LayerAction::LayerSurface { builder } => created.push(builder),
                            _ => {}
                        },
                        _ => {}
                    }
                }
            });
        }
    }
    (destroyed, created)
}
fn escape(cards: &mut Cards) -> Task<Message> {
    cards.update(Message::SurfaceInput(
        cards.ids[5],
        Box::new(Message::LayoutKey(
            keyboard::Key::Named(keyboard::key::Named::Escape),
            false,
        )),
    ))
}
fn moved() -> (Cards, crate::displays::test_support::Wire) {
    let (mut cards, _) = Cards::init(Core::default(), Config::load());
    let wire = cards.test_output_ready();
    let task = cards.update(Message::Arrange);
    let (_, created) = emitted(&mut cards, task);
    assert!(
        created
            .iter()
            .any(|s| s.keyboard_interactivity == KeyboardInteractivity::Exclusive)
    );
    cards.put_layout_point(Widget::Weather, [600., 300.]);
    assert!(cards.commit_arrange());
    assert!(!cards.layout_editor.session_actions.is_empty());
    (cards, wire)
}
fn released(cards: &mut Cards, old: window::Id, task: Task<Message>) {
    let (destroyed, created) = emitted(cards, task);
    assert!(
        destroyed.contains(&old),
        "must emit Destroy for captured layer"
    );
    assert!(
        created
            .iter()
            .all(|s| s.keyboard_interactivity == KeyboardInteractivity::None),
        "no exclusive replacement"
    );
    assert!(
        created
            .iter()
            .filter(|s| s.namespace == "halcyon-arrange")
            .all(|s| s.input_zone.as_ref().is_some_and(Vec::is_empty))
    );
    assert!(!cards.layout_editor.active);
    assert!(cards.paused_layout.is_some(), "retain recovery state");
}

#[test]
fn arrange_cancel_error_releases_input() {
    if isolated("arrange_cancel_error_releases_input") {
        return;
    }
    let (mut cards, _wire) = moved();
    Config::save_changes(
        serde_json::json!({"widget_positions":{"weather":[0.9,0.9]},"theme":"ocean"}),
    )
    .unwrap();
    let old = cards.ids[5];
    let task = escape(&mut cards);
    released(&mut cards, old, task);
    assert_eq!(Config::load().widget_positions["weather"], [0.9, 0.9]);
    assert_eq!(Config::load().theme, "ocean");
    let paused = cards.paused_layout.as_ref().unwrap();
    assert!(!paused.active && paused.error.is_some());
    assert_eq!(paused.session_actions.len(), 1);
    let draft = paused.draft.clone();
    let task = cards.update(Message::Arrange);
    assert!(
        emitted(&mut cards, task).1.is_empty(),
        "Arrange shows recovery without recapturing"
    );
    let mut native = recovery_native();
    native.capture(
        cards.view_window(cards.ids[2]),
        &cosmic::Theme::dark(),
        "arrange-conflict-recovery",
    );
    assert!(click(&mut cards, &mut native, "arrange-retry").is_empty());
    assert_eq!(cards.paused_layout.as_ref().unwrap().draft, draft);
    assert!(
        click(&mut cards, &mut native, "arrange-keep")
            .iter()
            .all(|s| s.keyboard_interactivity == KeyboardInteractivity::None)
    );
    assert!(cards.paused_layout.is_none());
    let task = cards.update(Message::SurfaceMessage(
        cards.display_generation,
        Box::new(Message::LayoutResume),
    ));
    assert!(
        emitted(&mut cards, task).1.is_empty(),
        "a stale recovery click cannot start another session"
    );
    assert!(!cards.layout_editor.active);
    assert_eq!(Config::load().widget_positions["weather"], [0.9, 0.9]);
    assert_eq!(Config::load().theme, "ocean");
}

struct Holder(std::process::Child);
impl Holder {
    fn new(path: &std::path::Path) -> Self {
        let mut child = Command::new("python3").args(["-c", "import fcntl,sys; f=open(sys.argv[1],'a'); fcntl.flock(f,fcntl.LOCK_EX); print('held',flush=True); sys.stdin.read()"])
            .arg(path).stdin(Stdio::piped()).stdout(Stdio::piped()).spawn().unwrap();
        let mut line = String::new();
        BufReader::new(child.stdout.take().unwrap())
            .read_line(&mut line)
            .unwrap();
        assert_eq!(line.trim(), "held");
        Self(child)
    }
}
impl Drop for Holder {
    fn drop(&mut self) {
        self.0.stdin.take();
        self.0.wait().unwrap();
    }
}
fn contention(desktop: bool) {
    let (mut cards, _wire) = moved();
    let path = Config::path().unwrap();
    let lock_path = if desktop {
        path.parent().unwrap().join("state/desktop.lock")
    } else {
        path.with_extension("json.lock")
    };
    let holder = Holder::new(&lock_path);
    let saved = std::fs::read(&path).unwrap();
    let start = Instant::now();
    // A different UI writer cannot block delivery of Escape either.
    let task = cards.update(Message::ToggleWidget(Widget::Calendar));
    emitted(&mut cards, task);
    cards.layout_editor.selected = Some(Widget::Weather);
    let task = cards.update(Message::LayoutKey(
        keyboard::Key::Named(keyboard::key::Named::ArrowRight),
        false,
    ));
    emitted(&mut cards, task);
    assert!(
        cards.layout_editor.error.is_some(),
        "failed gesture stays recoverable"
    );
    let old = cards.ids[5];
    let task = escape(&mut cards);
    released(&mut cards, old, task);
    assert!(
        start.elapsed() < Duration::from_secs(2),
        "contention must return promptly"
    );
    assert_eq!(std::fs::read(&path).unwrap(), saved);
    assert_eq!(
        cards.paused_layout.as_ref().unwrap().session_actions.len(),
        1
    );
    let mut native = recovery_native();
    native.capture(
        cards.view_window(cards.ids[2]),
        &cosmic::Theme::dark(),
        if desktop {
            "arrange-desktop-busy"
        } else {
            "arrange-config-busy"
        },
    );
    assert!(click(&mut cards, &mut native, "arrange-retry").is_empty());
    assert!(cards.paused_layout.is_some());
    drop(holder);
    Config::save_changes(serde_json::json!({"theme":"ocean", "devbox":{"network":false}})).unwrap();
    assert!(
        click(&mut cards, &mut native, "arrange-retry")
            .iter()
            .all(|s| s.keyboard_interactivity == KeyboardInteractivity::None)
    );
    assert!(cards.paused_layout.is_none());
    let config = Config::load();
    assert_eq!(config.widget_positions["weather"], [0.2, 0.2]);
    assert_eq!(config.theme, "ocean");
    assert!(!config.devbox.network);
    let raw: serde_json::Value = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
    assert_eq!(raw["future_option"], 42);
    assert_eq!(
        crate::desktop::actions::history_store()
            .unwrap()
            .latest_change()
            .unwrap()
            .unwrap()
            .label,
        "Cancel arrangement"
    );
}
#[test]
fn arrange_config_lock_does_not_block_escape() {
    if !isolated("arrange_config_lock_does_not_block_escape") {
        contention(false);
    }
}
#[test]
fn arrange_desktop_lock_does_not_block_escape() {
    if !isolated("arrange_desktop_lock_does_not_block_escape") {
        contention(true);
    }
}

fn recovery_native() -> crate::native_test::Native {
    crate::native_test::Native::new(cosmic::iced::Size::new(
        drawer::SIDEBAR_WIDTH as f32,
        drawer::SIDEBAR_HEIGHT as f32,
    ))
}
fn click(
    cards: &mut Cards,
    native: &mut crate::native_test::Native,
    id: &str,
) -> Vec<SctkLayerSurfaceSettings> {
    let point = native.bounds(cards.view_window(cards.ids[2]), id).center();
    assert!(
        point.y > 0. && point.y < drawer::SIDEBAR_HEIGHT as f32,
        "visible recovery control"
    );
    let mut count = 0;
    let mut created = Vec::new();
    for event in [
        mouse::Event::CursorMoved { position: point },
        mouse::Event::ButtonPressed(mouse::Button::Left),
        mouse::Event::ButtonReleased(mouse::Button::Left),
    ] {
        let (messages, _) = native.event(
            cards.view_window(cards.ids[2]),
            cosmic::iced::Event::Mouse(event),
            point,
        );
        count += messages.len();
        for message in messages {
            let task = cards.update(message);
            created.extend(emitted(cards, task).1);
        }
    }
    assert!(count > 0, "real widget input emitted the recovery action");
    created
}

#[test]
fn arrange_save_error_can_resume_draft_without_losing_history() {
    if isolated("arrange_save_error_can_resume_draft_without_losing_history") {
        return;
    }
    let (mut cards, _wire) = moved();
    cards.put_layout_point(Widget::Weather, [800., 400.]);
    let draft = cards.layout_editor.draft.clone();
    let path = Config::path().unwrap();
    let saved = std::fs::read(&path).unwrap();
    std::fs::write(&path, "{broken").unwrap();
    let old = cards.ids[5];
    let task = cards.update(Message::LayoutFinish(true));
    released(&mut cards, old, task);
    assert_eq!(cards.paused_layout.as_ref().unwrap().draft, draft);
    assert_eq!(std::fs::read(&path).unwrap(), b"{broken");
    std::fs::write(&path, &saved).unwrap();
    let mut native = recovery_native();
    let created = click(&mut cards, &mut native, "arrange-resume");
    assert!(
        created
            .iter()
            .any(|s| s.keyboard_interactivity == KeyboardInteractivity::Exclusive)
    );
    assert_eq!(cards.layout_editor.draft, draft);
    assert_eq!(cards.layout_editor.session_actions.len(), 1);
    let task = cards.update(Message::LayoutFinish(true));
    let (_, created) = emitted(&mut cards, task);
    assert!(
        created
            .iter()
            .all(|s| s.keyboard_interactivity == KeyboardInteractivity::None)
    );
    assert!(!cards.layout_editor.active && cards.paused_layout.is_none());
    assert_eq!(Config::load().widget_positions["weather"], draft["weather"]);
}

#[test]
fn arrange_stale_surface_tasks_and_inputs_cannot_reopen_or_edit() {
    if isolated("arrange_stale_surface_tasks_and_inputs_cannot_reopen_or_edit") {
        return;
    }
    let (mut cards, _) = Cards::init(Core::default(), Config::load());
    let wire = cards.test_output_ready();
    let entry = cards.update(Message::Arrange);
    let old_id = cards.ids[5];
    let old_epoch = cards.display_generation;
    let exit = escape(&mut cards);
    assert!(
        emitted(&mut cards, entry).1.is_empty(),
        "late entry must not recreate capture"
    );
    // Start another session before the old exit's surface continuation runs.
    let entry = cards.update(Message::Arrange);
    assert!(
        emitted(&mut cards, exit).1.is_empty(),
        "old exit cannot replace the new probe"
    );
    assert!(
        emitted(&mut cards, entry)
            .1
            .iter()
            .any(|s| s.keyboard_interactivity == KeyboardInteractivity::Exclusive)
    );
    cards.put_layout_point(Widget::Weather, [700., 350.]);
    let draft = cards.layout_editor.draft.clone();
    for message in [
        Message::SurfaceMessage(old_epoch, Box::new(Message::LayoutFinish(false))),
        Message::SurfaceInput(
            old_id,
            Box::new(Message::LayoutKey(
                keyboard::Key::Named(keyboard::key::Named::Escape),
                false,
            )),
        ),
        Message::ArrangeSurface(old_epoch),
    ] {
        let task = cards.update(message);
        assert!(emitted(&mut cards, task).1.is_empty());
        assert!(cards.layout_editor.active);
        assert_eq!(cards.layout_editor.draft, draft);
    }
    let output = cards.selected_output.as_ref().unwrap().handle.clone();
    let retired = cards.ids[5];
    let pending_epoch = cards.display_generation;
    let task = cards.update(Message::OutputChanged(
        crate::displays::OutputEvent::Removed,
        output,
    ));
    assert!(emitted(&mut cards, task).0.contains(&retired));
    assert!(!cards.layout_editor.active);
    assert_eq!(cards.paused_layout.as_ref().unwrap().draft, draft);
    let task = cards.update(Message::ArrangeSurface(pending_epoch));
    assert!(emitted(&mut cards, task).1.is_empty());
    let task = cards.update(Message::SurfaceResized(
        retired,
        cosmic::iced::Size::new(800., 600.),
    ));
    assert!(emitted(&mut cards, task).0.contains(&retired));
    let replacement = wire.output(2);
    let task = cards.update(Message::OutputChanged(
        crate::displays::OutputEvent::Created(Some(crate::displays::test_support::info(
            Some("DP-2"),
            (1280, 720),
        ))),
        replacement,
    ));
    assert!(
        emitted(&mut cards, task)
            .1
            .iter()
            .all(|s| s.keyboard_interactivity == KeyboardInteractivity::None)
    );
    let task = cards.update(Message::SurfaceResized(
        cards.ids[5],
        cosmic::iced::Size::new(1280., 720.),
    ));
    assert!(
        emitted(&mut cards, task)
            .1
            .iter()
            .all(|s| s.keyboard_interactivity == KeyboardInteractivity::None)
    );
    assert!(!cards.layout_editor.active);
    let task = cards.update(Message::Arrange);
    assert!(
        emitted(&mut cards, task)
            .1
            .iter()
            .any(|s| s.keyboard_interactivity == KeyboardInteractivity::Exclusive)
    );
    assert_eq!(cards.layout_editor.draft, draft);
}

#[test]
fn arrange_receipts_exclude_another_writers_newer_history() {
    if isolated("arrange_receipts_exclude_another_writers_newer_history") {
        return;
    }
    let (result, receipts) = crate::desktop::actions::with_receipts(|| {
        crate::desktop::actions::in_action("Owned arrangement", || {
            Config::save_changes(serde_json::json!({"widget_positions":{"weather":[0.5,0.4]}}))
        })
        .unwrap();
        std::thread::spawn(|| {
            crate::desktop::actions::in_action("Other writer", || {
                Config::save_changes(serde_json::json!({"theme":"ocean"}))
            })
        })
        .join()
        .unwrap()
        .unwrap();
        Err::<(), _>("later read failed")
    });
    assert!(result.is_err());
    assert_eq!(receipts.len(), 1);
    assert_eq!(receipts[0].label, "Owned arrangement");
    assert_eq!(
        crate::desktop::actions::history_store()
            .unwrap()
            .latest_change()
            .unwrap()
            .unwrap()
            .label,
        "Other writer"
    );
    crate::desktop::actions::cancel_arrangement(&receipts).unwrap();
    assert_eq!(Config::load().widget_positions["weather"], [0.2, 0.2]);
    assert_eq!(Config::load().theme, "ocean");
}

// Reproduce the recovery-before-watcher ordering with two real client proxies.
// 0: failed exit reads the new pin; 1: Keep reads it; 2: successful Retry reads it.
fn recovery_pin_before_watcher(path: u8) {
    use crate::displays::{OutputEvent, test_support::info};
    Config::save_changes(serde_json::json!({"widget_display":"test-output"})).unwrap();
    let (mut cards, wire) = moved();
    let output_b = wire.output(2);
    let task = cards.update(Message::OutputChanged(
        OutputEvent::Created(Some(info(Some("DP-B"), (1280, 720)))),
        output_b.clone(),
    ));
    assert!(emitted(&mut cards, task).1.is_empty());
    let draft = cards.layout_editor.draft.clone();
    let receipts = cards.layout_editor.session_actions.clone();
    let guard = cards.layout_editor.guard.clone();
    let history_len = cards.layout_editor.history.len();
    let config_path = Config::path().unwrap();
    let publish_pin = || {
        Config::save_changes(serde_json::json!({
            "widget_display":"DP-B", "theme":"ocean", "devbox":{"network":false}
        }))
        .unwrap();
    };
    if path != 0 {
        // Keep has a real field conflict. Retry first fails on a held lock.
        let holder = if path == 2 {
            Some(Holder::new(&config_path.with_extension("json.lock")))
        } else {
            Config::save_changes(serde_json::json!({"widget_positions":{"weather":[0.9,0.9]}}))
                .unwrap();
            None
        };
        let captured = cards.ids[5];
        let task = escape(&mut cards);
        released(&mut cards, captured, task);
        assert_eq!(
            cards.selected_output.as_ref().unwrap().name(),
            Some("test-output")
        );
        drop(holder);
    }
    let old_ids = cards.ids;
    let old_epoch = cards.display_generation;
    publish_pin(); // Deliberately no FilesChanged before the recovery action.
    let holder = (path == 0).then(|| Holder::new(&config_path.with_extension("json.lock")));
    let start = Instant::now();
    let task = match path {
        0 => escape(&mut cards),
        1 => cards.update(Message::LayoutKeepSaved),
        2 => cards.update(Message::LayoutRetryCancel),
        _ => unreachable!(),
    };
    assert!(start.elapsed() < Duration::from_secs(2));
    let (mut destroyed, mut created) = emitted(&mut cards, task);
    drop(holder);
    let task = cards.update(Message::FilesChanged);
    let (watch_destroyed, watch_created) = emitted(&mut cards, task);
    destroyed.extend(watch_destroyed);
    created.extend(watch_created);
    eprintln!(
        "recovery path {path}: config pin {:?}, selected {:?}, epoch {} -> {}, created {} surfaces",
        cards.config.widget_display,
        cards.selected_output.as_ref().unwrap().name(),
        old_epoch,
        cards.display_generation,
        created.len()
    );
    assert_eq!(cards.config.widget_display.as_deref(), Some("DP-B"));
    assert_eq!(
        cards.selected_output.as_ref().unwrap().handle,
        output_b,
        "recovery must migrate the externally saved pin before the watcher consumes B == B"
    );
    assert!(
        old_ids.iter().all(|id| destroyed.contains(id)),
        "retire every A surface, including the captured probe"
    );
    assert!(cards.ids.iter().all(|id| !old_ids.contains(id)));
    assert!(
        !cards.output_known,
        "B must get its own measurement before widgets appear"
    );
    // The arrange strip measures B's width and the edge strip its height.
    let mut names: Vec<_> = created.iter().map(|s| s.namespace.as_str()).collect();
    names.sort();
    assert_eq!(
        names,
        ["halcyon-arrange", "halcyon-edge"],
        "fresh migration probes, no stale Arrange replacement"
    );
    let probe = created
        .iter()
        .find(|s| s.namespace == "halcyon-arrange")
        .unwrap();
    assert_eq!(probe.id, cards.ids[5]);
    assert_eq!(probe.keyboard_interactivity, KeyboardInteractivity::None);
    assert!(probe.input_zone.as_ref().is_some_and(Vec::is_empty));
    let current_ids = cards.ids;
    let task = cards.update(Message::SurfaceResized(
        cards.ids[5],
        cosmic::iced::Size::new(1280., 720.),
    ));
    created.extend(emitted(&mut cards, task).1);
    assert!(
        created.len() > 1,
        "ready B gets actual widget/controls surfaces"
    );
    for surface in &created {
        assert!(
            matches!(&surface.output, IcedOutput::Output(output) if output == &output_b),
            "{} bound to B",
            surface.namespace
        );
        assert!(cards.ids.contains(&surface.id));
        assert_eq!(surface.keyboard_interactivity, KeyboardInteractivity::None);
    }
    // Deliver late continuations after B is ready, when output_known cannot
    // mask an epoch-order bug. Failed exit also scheduled an intermediate epoch.
    let mut stale_epochs = vec![old_epoch];
    if path == 0 {
        stale_epochs.push(old_epoch + 1);
    }
    for epoch in stale_epochs {
        let task = cards.update(Message::ArrangeSurface(epoch));
        let (destroyed, created) = emitted(&mut cards, task);
        assert!(
            destroyed.is_empty() && created.is_empty(),
            "stale epoch {epoch}: destroyed {destroyed:?}, created {:?}",
            created.iter().map(|s| &s.namespace).collect::<Vec<_>>()
        );
    }
    let task = cards.update(Message::SurfaceMessage(
        old_epoch,
        Box::new(Message::LayoutResume),
    ));
    assert!(emitted(&mut cards, task).1.is_empty());
    let task = cards.update(Message::SurfaceResized(
        old_ids[5],
        cosmic::iced::Size::new(1920., 1080.),
    ));
    let (destroyed, created) = emitted(&mut cards, task);
    assert_eq!(destroyed, vec![old_ids[5]]);
    assert!(created.is_empty());
    assert_eq!(cards.ids, current_ids);
    assert!(cards.output_known && !cards.layout_editor.active);
    if path == 0 {
        let paused = cards.paused_layout.as_ref().unwrap();
        assert!(!paused.active && paused.exit_failed && paused.error.is_some());
        assert_eq!(paused.draft, draft);
        assert_eq!(paused.session_actions, receipts);
        assert_eq!(paused.guard, guard);
        assert_eq!(paused.history.len(), history_len);
    } else {
        assert!(cards.paused_layout.is_none());
    }
    let saved = Config::load();
    assert_eq!(saved.widget_display.as_deref(), Some("DP-B"));
    assert_eq!(saved.theme, "ocean");
    assert!(!saved.devbox.network);
    assert_eq!(cards.config.theme, saved.theme);
    assert_eq!(
        saved.widget_positions["weather"],
        match path {
            0 => draft["weather"],
            1 => [0.9, 0.9],
            _ => [0.2, 0.2],
        }
    );
    let raw: serde_json::Value =
        serde_json::from_slice(&std::fs::read(config_path).unwrap()).unwrap();
    assert_eq!(raw["future_option"], 42);
    let latest = crate::desktop::actions::history_store()
        .unwrap()
        .latest_change()
        .unwrap()
        .unwrap();
    assert_eq!(
        latest.label,
        if path == 2 {
            "Cancel arrangement"
        } else {
            "Widget arrangement"
        }
    );
}

#[test]
fn arrange_recovery_pin_failed_exit_before_watcher() {
    if !isolated("arrange_recovery_pin_failed_exit_before_watcher") {
        recovery_pin_before_watcher(0);
    }
}
#[test]
fn arrange_recovery_pin_keep_before_watcher() {
    if !isolated("arrange_recovery_pin_keep_before_watcher") {
        recovery_pin_before_watcher(1);
    }
}
#[test]
fn arrange_recovery_pin_retry_before_watcher() {
    if !isolated("arrange_recovery_pin_retry_before_watcher") {
        recovery_pin_before_watcher(2);
    }
}

// These regressions drive real config transactions and the production output
// messages. The socketpair proxies never connect to a compositor.
fn auditfix_measure_outputs(cards: &mut Cards) -> Vec<SctkLayerSurfaceSettings> {
    let mut created = Vec::new();
    if !cards.output_known {
        let (width, height) = cards
            .selected_output
            .as_ref()
            .unwrap()
            .info
            .logical_size
            .unwrap();
        let task = cards.update(Message::SurfaceResized(
            cards.ids[5],
            cosmic::iced::Size::new(width as f32, height as f32),
        ));
        created.extend(emitted(cards, task).1);
    }
    // Another display's probe is a strip along the top (its width) and one
    // along the left (its height).
    let extra: Vec<_> = cards
        .extra_probes
        .iter()
        .filter(|probe| probe.size().is_none())
        .flat_map(|probe| {
            let (width, height) = probe.output.info.logical_size.unwrap();
            [
                (probe.ids[0], cosmic::iced::Size::new(width as f32, 1.)),
                (probe.ids[1], cosmic::iced::Size::new(1., height as f32)),
            ]
        })
        .collect();
    for (id, size) in extra {
        let task = cards.update(Message::SurfaceResized(id, size));
        created.extend(emitted(cards, task).1);
    }
    created
}

fn auditfix_two_display_arrange() -> (Cards, crate::displays::test_support::Wire) {
    use crate::displays::{
        OutputEvent,
        test_support::{Wire, info},
    };
    let config = Config::save_changes(serde_json::json!({
        "widget_display":"eDP-1", "enabled_widgets":["weather"],
        "placement":{"snap":false},
        "widget_monitors":{"weather":{"display":"DP-2"}},
        "monitor_layouts":{"weather":{
            "DP-2":{"position":[0.2,0.2],"size":[360.,180.]},
            "eDP-1":{"position":[0.8,0.8],"size":[360.,180.]}
        }}
    }))
    .unwrap();
    let wire = Wire::new();
    let (mut cards, _) = Cards::init(Core::default(), config);
    for (id, name, size) in [(1, "eDP-1", (1920, 1080)), (2, "DP-2", (1280, 720))] {
        let task = cards.update(Message::OutputChanged(
            OutputEvent::Created(Some(info(Some(name), size))),
            wire.output(id),
        ));
        emitted(&mut cards, task);
    }
    auditfix_measure_outputs(&mut cards);
    let task = cards.update(Message::ArrangeDisplay("DP-2".into()));
    emitted(&mut cards, task);
    auditfix_measure_outputs(&mut cards);
    assert!(cards.layout_editor.active && cards.arranging_widget(Widget::Weather));
    assert_eq!(cards.screen_size, [1280., 720.]);
    cards.put_layout_point(Widget::Weather, [550., 280.]);
    assert!(cards.commit_arrange());
    assert!(!cards.layout_editor.session_actions.is_empty());
    (cards, wire)
}

#[test]
fn auditfix_run02_identity_after_removal_cannot_retarget_failed_transaction() {
    if isolated("auditfix_run02_identity_after_removal_cannot_retarget_failed_transaction") {
        return;
    }
    for save in [true, false] {
        let (mut cards, _wire) = auditfix_two_display_arrange();
        cards.put_layout_point(Widget::Weather, [780., 400.]);
        let draft = cards.layout_editor.draft.clone();
        let baseline = cards.layout_editor.baseline["weather"];
        let receipts = cards.layout_editor.session_actions.clone();
        let original = cards.selected_output.as_ref().unwrap().handle.clone();
        let path = Config::path().unwrap();
        let saved = std::fs::read(&path).unwrap();
        let holder = Holder::new(&path.with_extension("json.lock"));
        let task = cards.update(Message::LayoutFinish(save));
        assert!(
            emitted(&mut cards, task)
                .1
                .iter()
                .all(|s| s.keyboard_interactivity == KeyboardInteractivity::None)
        );
        drop(holder);
        assert!(cards.paused_layout.as_ref().unwrap().exit_failed);
        let task = cards.update(Message::OutputChanged(
            crate::displays::OutputEvent::Removed,
            original,
        ));
        emitted(&mut cards, task);
        auditfix_measure_outputs(&mut cards);
        // Deliver the actual follow-up reconciliation, both an empty identity
        // result and a newly identified fallback. Neither proves a DP-2 alias.
        for identities in [
            BTreeMap::new(),
            [("eDP-1".into(), "edid:fallback".into())].into(),
        ] {
            let task = cards.update(Message::DisplayIdentities(
                cards.displays.generation,
                identities,
            ));
            emitted(&mut cards, task);
            let paused = cards.paused_layout.as_ref().unwrap();
            assert_eq!(paused.contexts["weather"], "DP-2");
            assert_eq!(paused.baseline["weather"], baseline);
            assert_eq!(paused.draft, draft);
            assert_eq!(paused.session_actions, receipts);
            let task = cards.update(Message::LayoutResume);
            assert!(emitted(&mut cards, task).1.is_empty());
            assert!(!cards.layout_editor.active);
            assert!(cards.paused_layout.as_ref().unwrap().exit_failed);
            assert_eq!(std::fs::read(&path).unwrap(), saved);
        }
    }
}

#[test]
fn auditfix_run03_failed_exit_resumes_original_nondefault_output() {
    if isolated("auditfix_run03_failed_exit_resumes_original_nondefault_output") {
        return;
    }
    for save in [true, false] {
        let (mut cards, _wire) = auditfix_two_display_arrange();
        let original = cards.selected_output.as_ref().unwrap().handle.clone();
        cards.put_layout_point(Widget::Weather, [780., 400.]);
        cards.layout_editor.selected = Some(Widget::Weather);
        let draft = cards.layout_editor.draft.clone();
        let receipts = cards.layout_editor.session_actions.clone();
        let path = Config::path().unwrap();
        let holder = Holder::new(&path.with_extension("json.lock"));
        let task = cards.update(Message::LayoutFinish(save));
        assert!(
            emitted(&mut cards, task)
                .1
                .iter()
                .all(|s| s.keyboard_interactivity == KeyboardInteractivity::None)
        );
        drop(holder);
        auditfix_measure_outputs(&mut cards);
        assert_eq!(
            cards.selected_output.as_ref().unwrap().name(),
            Some("eDP-1")
        );
        assert_eq!(cards.screen_size, [1920., 1080.]);
        let task = cards.update(Message::LayoutResume);
        let mut created = emitted(&mut cards, task).1;
        created.extend(auditfix_measure_outputs(&mut cards));
        assert!(cards.layout_editor.active && cards.arranging_widget(Widget::Weather));
        assert_eq!(cards.selected_output.as_ref().unwrap().handle, original);
        assert_eq!(cards.screen_size, [1280., 720.]);
        assert_eq!(cards.layout_editor.draft, draft);
        assert_eq!(cards.layout_editor.session_actions, receipts);
        assert!(created.iter().any(|s| s.namespace == "halcyon-arrange"
            && s.keyboard_interactivity == KeyboardInteractivity::Exclusive
            && matches!(&s.output, IcedOutput::Output(output) if output == &original)));
        let task = cards.update(Message::LayoutKey(
            keyboard::Key::Named(keyboard::key::Named::ArrowRight),
            false,
        ));
        emitted(&mut cards, task);
        let nudged = cards.layout_editor.draft["weather"];
        assert!(nudged[0] > draft["weather"][0]);
        assert_eq!(nudged[1], draft["weather"][1]);
        let task = cards.update(Message::LayoutFinish(true));
        emitted(&mut cards, task);
        auditfix_measure_outputs(&mut cards);
        assert!(!cards.layout_editor.active && cards.paused_layout.is_none());
        let saved = Config::load();
        assert_eq!(
            saved.monitor_layouts["weather"]["DP-2"].position,
            Some(nudged)
        );
        assert_eq!(
            saved.monitor_layouts["weather"]["eDP-1"].position,
            Some([0.8, 0.8])
        );
        assert_eq!(
            cards.selected_output.as_ref().unwrap().name(),
            Some("eDP-1")
        );
    }
}

#[test]
fn auditfix_run04_failed_completed_gesture_survives_geometry_and_reentry() {
    if isolated("auditfix_run04_failed_completed_gesture_survives_geometry_and_reentry") {
        return;
    }
    let (mut cards, _wire) = moved();
    let receipts = cards.layout_editor.session_actions.clone();
    let path = Config::path().unwrap();
    let saved = std::fs::read(&path).unwrap();
    let holder = Holder::new(&path.with_extension("json.lock"));
    cards.layout_editor.selected = Some(Widget::Weather);
    let task = cards.update(Message::LayoutKey(
        keyboard::Key::Named(keyboard::key::Named::ArrowRight),
        false,
    ));
    emitted(&mut cards, task);
    assert!(cards.layout_editor.error.is_some());
    assert!(!cards.layout_editor.exit_failed);
    let draft = cards.layout_editor.draft.clone();
    let history_len = cards.layout_editor.history.len();
    let output = cards.selected_output.as_ref().unwrap().handle.clone();
    let task = cards.update(Message::OutputChanged(
        crate::displays::OutputEvent::InfoUpdate(crate::displays::test_support::info(
            Some("test-output"),
            (1600, 900),
        )),
        output,
    ));
    assert!(
        emitted(&mut cards, task)
            .1
            .iter()
            .all(|s| s.keyboard_interactivity == KeyboardInteractivity::None)
    );
    auditfix_measure_outputs(&mut cards);
    drop(holder);
    let task = cards.update(Message::ArrangeDisplay("test-output".into()));
    // No Arrange surface: only the edge menu opens, to offer Resume.
    assert!(
        emitted(&mut cards, task)
            .1
            .iter()
            .all(|s| s.namespace == "halcyon-controls")
    );
    let paused = cards.paused_layout.as_ref().unwrap();
    assert!(paused.exit_failed && !paused.active);
    assert_eq!(paused.draft, draft);
    assert_eq!(paused.history.len(), history_len);
    assert_eq!(paused.session_actions, receipts);
    assert_eq!(std::fs::read(&path).unwrap(), saved);
    let task = cards.update(Message::LayoutResume);
    assert!(
        emitted(&mut cards, task)
            .1
            .iter()
            .any(|s| s.keyboard_interactivity == KeyboardInteractivity::Exclusive)
    );
    assert_eq!(cards.layout_editor.draft, draft);
    assert_eq!(cards.layout_editor.session_actions, receipts);
    // Cancel still owns exactly the earlier successful gesture's receipt.
    let task = cards.update(Message::LayoutFinish(false));
    emitted(&mut cards, task);
    assert!(cards.paused_layout.is_none() && !cards.layout_editor.active);
    assert_eq!(Config::load().widget_positions["weather"], [0.2, 0.2]);
}

#[test]
fn auditfix_run02_paused_alias_on_original_handle_preserves_undo() {
    if isolated("auditfix_run02_paused_alias_on_original_handle_preserves_undo") {
        return;
    }
    let (mut cards, _wire) = auditfix_two_display_arrange();
    let original = cards.selected_output.as_ref().unwrap().handle.clone();
    let receipts = cards.layout_editor.session_actions.clone();
    let draft = cards.layout_editor.draft.clone();
    let path = Config::path().unwrap();
    let holder = Holder::new(&path.with_extension("json.lock"));
    let task = cards.update(Message::LayoutFinish(false));
    emitted(&mut cards, task);
    drop(holder);
    auditfix_measure_outputs(&mut cards);
    let task = cards.update(Message::DisplayIdentities(
        cards.displays.generation,
        [("DP-2".into(), "edid:original".into())].into(),
    ));
    emitted(&mut cards, task);
    let paused = cards.paused_layout.as_ref().unwrap();
    assert_eq!(paused.contexts["weather"], "edid:original");
    assert_eq!(paused.editing_display.as_deref(), Some("edid:original"));
    assert_eq!(paused.draft, draft);
    assert_eq!(paused.session_actions, receipts);
    let task = cards.update(Message::LayoutResume);
    emitted(&mut cards, task);
    auditfix_measure_outputs(&mut cards);
    assert!(cards.layout_editor.active && cards.arranging_widget(Widget::Weather));
    assert_eq!(cards.selected_output.as_ref().unwrap().handle, original);
    let task = cards.update(Message::LayoutFinish(false));
    emitted(&mut cards, task);
    auditfix_measure_outputs(&mut cards);
    assert!(cards.paused_layout.is_none());
    let saved = Config::load();
    assert_eq!(
        saved.monitor_layouts["weather"]["DP-2"].position,
        Some([0.2, 0.2])
    );
    assert_eq!(
        saved.monitor_layouts["weather"]["eDP-1"].position,
        Some([0.8, 0.8])
    );
    assert!(!saved.monitor_layouts["weather"].contains_key("edid:original"));
}

#[test]
fn auditfix_run04_keep_or_retry_after_geometry_restores_default_display() {
    if isolated("auditfix_run04_keep_or_retry_after_geometry_restores_default_display") {
        return;
    }
    for keep in [true, false] {
        let (mut cards, _wire) = auditfix_two_display_arrange();
        let committed = cards.layout_editor.draft["weather"];
        let path = Config::path().unwrap();
        let holder = Holder::new(&path.with_extension("json.lock"));
        cards.layout_editor.selected = Some(Widget::Weather);
        let task = cards.update(Message::LayoutKey(
            keyboard::Key::Named(keyboard::key::Named::ArrowRight),
            false,
        ));
        emitted(&mut cards, task);
        assert!(cards.layout_editor.error.is_some());
        let output = cards.selected_output.as_ref().unwrap().handle.clone();
        let task = cards.update(Message::OutputChanged(
            crate::displays::OutputEvent::InfoUpdate(crate::displays::test_support::info(
                Some("DP-2"),
                (1024, 768),
            )),
            output,
        ));
        emitted(&mut cards, task);
        auditfix_measure_outputs(&mut cards);
        drop(holder);
        assert!(cards.paused_layout.as_ref().unwrap().exit_failed);
        let task = cards.update(if keep {
            Message::LayoutKeepSaved
        } else {
            Message::LayoutRetryCancel
        });
        emitted(&mut cards, task);
        auditfix_measure_outputs(&mut cards);
        assert!(cards.paused_layout.is_none() && !cards.layout_editor.active);
        assert!(cards.arrange_display.is_none());
        assert_eq!(
            cards.selected_output.as_ref().unwrap().name(),
            Some("eDP-1")
        );
        assert_eq!(
            Config::load().monitor_layouts["weather"]["DP-2"].position,
            Some(if keep { committed } else { [0.2, 0.2] })
        );
    }
}

fn auditfix_pending_arrange_recovery(save: bool) -> (Cards, crate::displays::test_support::Wire) {
    let (mut cards, wire) = auditfix_two_display_arrange();
    let path = Config::path().unwrap();
    let holder = Holder::new(&path.with_extension("json.lock"));
    let task = cards.update(Message::LayoutSelect(Widget::Weather));
    emitted(&mut cards, task);
    let task = cards.update(Message::LayoutKey(
        keyboard::Key::Named(keyboard::key::Named::ArrowRight),
        false,
    ));
    emitted(&mut cards, task);
    assert!(cards.layout_editor.error.is_some());
    assert_ne!(cards.layout_editor.draft, cards.layout_editor.baseline);
    let task = cards.update(Message::LayoutFinish(save));
    auditfix_no_arrange_capture(&emitted(&mut cards, task).1);
    drop(holder);
    auditfix_no_arrange_capture(&auditfix_measure_outputs(&mut cards));
    assert_eq!(
        cards.selected_output.as_ref().unwrap().name(),
        Some("eDP-1")
    );
    let task = cards.update(Message::LayoutResume);
    auditfix_no_arrange_capture(&emitted(&mut cards, task).1);
    assert!(!cards.output_known && !cards.layout_editor.active);
    assert_eq!(cards.selected_output.as_ref().unwrap().name(), Some("DP-2"));
    (cards, wire)
}

fn auditfix_no_arrange_capture(created: &[SctkLayerSurfaceSettings]) {
    assert!(
        created.iter().all(|surface| {
            surface.keyboard_interactivity == KeyboardInteractivity::None
                && (surface.namespace != "halcyon-arrange"
                    || surface.input_zone.as_ref().is_some_and(Vec::is_empty))
        }),
        "recovery must not create an input-capturing fallback overlay"
    );
}

#[test]
fn auditfix_state01_pending_resume_removal_keeps_draft_receipts_and_recovery_choices() {
    if isolated("auditfix_state01_pending_resume_removal_keeps_draft_receipts_and_recovery_choices")
    {
        return;
    }
    for save in [true, false] {
        for keep in [true, false] {
            let (mut cards, _wire) = auditfix_pending_arrange_recovery(save);
            let editor = cards.paused_layout.as_ref().unwrap();
            let draft = editor.draft.clone();
            let receipts = editor.session_actions.clone();
            let baseline = editor.baseline.clone();
            let guard = editor.guard.clone();
            let history_len = editor.history.len();
            let original = cards.selected_output.as_ref().unwrap().handle.clone();
            let old_probe = cards.ids[5];
            let old_epoch = cards.display_generation;
            let path = Config::path().unwrap();
            let saved = std::fs::read(&path).unwrap();
            let committed = Config::load().monitor_layouts["weather"]["DP-2"].position;

            // The reviewed gap: removal after Resume admission, before measurement.
            let task = cards.update(Message::OutputChanged(
                crate::displays::OutputEvent::Removed,
                original,
            ));
            auditfix_no_arrange_capture(&emitted(&mut cards, task).1);
            let task = cards.update(Message::SurfaceResized(
                old_probe,
                cosmic::iced::Size::new(1280., 720.),
            ));
            let (destroyed, created) = emitted(&mut cards, task);
            assert!(destroyed.contains(&old_probe));
            auditfix_no_arrange_capture(&created);
            let task = cards.update(Message::ArrangeSurface(old_epoch));
            auditfix_no_arrange_capture(&emitted(&mut cards, task).1);
            auditfix_no_arrange_capture(&auditfix_measure_outputs(&mut cards));
            // The identity completion on the fallback must not rebase the draft.
            let task = cards.update(Message::DisplayIdentities(
                cards.displays.generation,
                BTreeMap::new(),
            ));
            auditfix_no_arrange_capture(&emitted(&mut cards, task).1);
            for request in [
                Message::Arrange,
                Message::ArrangeDisplay("eDP-1".into()),
                Message::LayoutResume,
            ] {
                let task = cards.update(request);
                auditfix_no_arrange_capture(&emitted(&mut cards, task).1);
            }
            assert!(!cards.layout_editor.active && cards.pending_arrange.is_none());
            assert!(cards.arrange_display.is_none());
            assert_eq!(
                cards.selected_output.as_ref().unwrap().name(),
                Some("eDP-1")
            );
            let editor = cards.paused_layout.as_ref().unwrap();
            assert!(editor.exit_failed && editor.pending_resume.is_none());
            assert_eq!(editor.contexts["weather"], "DP-2");
            assert_eq!(editor.draft, draft);
            assert_eq!(editor.session_actions, receipts);
            assert_eq!(editor.baseline, baseline);
            assert_eq!(editor.guard, guard);
            assert_eq!(editor.history.len(), history_len);
            assert_eq!(editor.selected, Some(Widget::Weather));
            assert!(cards.arrange_recovery_view().is_some());
            assert_eq!(std::fs::read(&path).unwrap(), saved);

            let task = cards.update(if keep {
                Message::LayoutKeepSaved
            } else {
                Message::LayoutRetryCancel
            });
            auditfix_no_arrange_capture(&emitted(&mut cards, task).1);
            auditfix_no_arrange_capture(&auditfix_measure_outputs(&mut cards));
            assert!(cards.paused_layout.is_none() && !cards.layout_editor.active);
            let config = Config::load();
            assert_eq!(
                config.monitor_layouts["weather"]["DP-2"].position,
                if keep { committed } else { Some([0.2, 0.2]) }
            );
            assert_eq!(
                config.monitor_layouts["weather"]["eDP-1"].position,
                Some([0.8, 0.8])
            );
        }
    }
}

#[test]
fn auditfix_state01_pending_resume_blocks_arrange_display_until_original_probe() {
    if isolated("auditfix_state01_pending_resume_blocks_arrange_display_until_original_probe") {
        return;
    }
    for save in [true, false] {
        let (mut cards, _wire) = auditfix_pending_arrange_recovery(save);
        let original = cards.selected_output.as_ref().unwrap().handle.clone();
        let probe = cards.ids[5];
        let editor = cards.paused_layout.as_ref().unwrap();
        let draft = editor.draft.clone();
        let receipts = editor.session_actions.clone();
        let guard = editor.guard.clone();
        let saved = std::fs::read(Config::path().unwrap()).unwrap();
        for request in [
            Message::ArrangeDisplay("eDP-1".into()),
            Message::ArrangeDisplay("DP-2".into()),
            Message::Arrange,
            Message::LayoutResume,
        ] {
            let task = cards.update(request);
            assert!(emitted(&mut cards, task).1.is_empty());
            assert_eq!(cards.ids[5], probe);
            assert_eq!(cards.selected_output.as_ref().unwrap().handle, original);
        }
        // A proven alias on the original handle is still allowed during probing.
        let task = cards.update(Message::DisplayIdentities(
            cards.displays.generation,
            [("DP-2".into(), "edid:original".into())].into(),
        ));
        auditfix_no_arrange_capture(&emitted(&mut cards, task).1);
        let editor = cards.paused_layout.as_ref().unwrap();
        assert!(editor.exit_failed && editor.pending_resume.is_some());
        assert_eq!(editor.draft, draft);
        assert_eq!(editor.session_actions, receipts);
        assert_eq!(editor.guard, guard);
        assert_eq!(std::fs::read(Config::path().unwrap()).unwrap(), saved);
        let created = auditfix_measure_outputs(&mut cards);
        assert!(created.iter().any(|surface| surface.namespace == "halcyon-arrange"
            && surface.keyboard_interactivity == KeyboardInteractivity::Exclusive
            && matches!(&surface.output, IcedOutput::Output(output) if output == &original)));
        assert!(cards.layout_editor.active && cards.paused_layout.is_none());
        assert!(!cards.layout_editor.exit_failed && cards.layout_editor.pending_resume.is_none());
        assert_eq!(cards.screen_size, [1280., 720.]);
        assert_eq!(cards.layout_editor.draft, draft);
        assert_eq!(cards.layout_editor.session_actions, receipts);
        assert_eq!(cards.layout_editor.contexts["weather"], "edid:original");
        let task = cards.update(Message::LayoutFinish(false));
        auditfix_no_arrange_capture(&emitted(&mut cards, task).1);
        auditfix_no_arrange_capture(&auditfix_measure_outputs(&mut cards));
        assert!(cards.paused_layout.is_none());
        let config = Config::load();
        assert_eq!(
            config.monitor_layouts["weather"]["DP-2"].position,
            Some([0.2, 0.2])
        );
        assert_eq!(
            config.monitor_layouts["weather"]["eDP-1"].position,
            Some([0.8, 0.8])
        );
        assert!(!config.monitor_layouts["weather"].contains_key("edid:original"));
    }
}

#[test]
fn auditfix_state01_pending_resume_geometry_or_context_change_returns_to_recovery() {
    if isolated("auditfix_state01_pending_resume_geometry_or_context_change_returns_to_recovery") {
        return;
    }
    for context_change in [true, false] {
        let (mut cards, _wire) = auditfix_pending_arrange_recovery(true);
        let editor = cards.paused_layout.as_ref().unwrap();
        let draft = editor.draft.clone();
        let receipts = editor.session_actions.clone();
        let old_probe = cards.ids[5];
        let task = if context_change {
            let config = Config::save_changes(serde_json::json!({
                "widget_monitors":{"weather":{"display":"eDP-1"}}
            }))
            .unwrap();
            cards.apply_config(config)
        } else {
            cards.update(Message::OutputChanged(
                crate::displays::OutputEvent::InfoUpdate(crate::displays::test_support::info(
                    Some("DP-2"),
                    (1024, 768),
                )),
                cards.selected_output.as_ref().unwrap().handle.clone(),
            ))
        };
        auditfix_no_arrange_capture(&emitted(&mut cards, task).1);
        let task = cards.update(Message::SurfaceResized(
            old_probe,
            cosmic::iced::Size::new(1280., 720.),
        ));
        auditfix_no_arrange_capture(&emitted(&mut cards, task).1);
        auditfix_no_arrange_capture(&auditfix_measure_outputs(&mut cards));
        assert!(!cards.layout_editor.active && cards.pending_arrange.is_none());
        assert!(cards.arrange_display.is_none());
        assert_eq!(
            cards.selected_output.as_ref().unwrap().name(),
            Some("eDP-1")
        );
        let editor = cards.paused_layout.as_ref().unwrap();
        assert!(editor.exit_failed && editor.pending_resume.is_none());
        assert_eq!(editor.draft, draft);
        assert_eq!(editor.session_actions, receipts);
        assert_eq!(editor.contexts["weather"], "DP-2");
        let task = cards.update(Message::LayoutRetryCancel);
        auditfix_no_arrange_capture(&emitted(&mut cards, task).1);
        auditfix_no_arrange_capture(&auditfix_measure_outputs(&mut cards));
        assert!(cards.paused_layout.is_none());
        let config = Config::load();
        assert_eq!(
            config.monitor_layouts["weather"]["DP-2"].position,
            Some([0.2, 0.2])
        );
        assert_eq!(
            config.monitor_layouts["weather"]["eDP-1"].position,
            Some([0.8, 0.8])
        );
        assert_eq!(
            config.widget_monitors["weather"].display.as_deref(),
            Some(if context_change { "eDP-1" } else { "DP-2" })
        );
    }
}

#[test]
fn auditfix_state01_pending_resume_recovery_choices_cancel_probe_and_preserve_conflicts() {
    if isolated(
        "auditfix_state01_pending_resume_recovery_choices_cancel_probe_and_preserve_conflicts",
    ) {
        return;
    }
    for conflict in [false, true] {
        for keep in [true, false] {
            let (mut cards, _wire) = auditfix_pending_arrange_recovery(false);
            let editor = cards.paused_layout.as_ref().unwrap();
            let draft = editor.draft.clone();
            let receipts = editor.session_actions.clone();
            let committed = Config::load().monitor_layouts["weather"]["DP-2"].position;
            let old_probe = cards.ids[5];
            if conflict {
                let mut raw: serde_json::Value =
                    serde_json::from_slice(&std::fs::read(Config::path().unwrap()).unwrap())
                        .unwrap();
                raw["monitor_layouts"]["weather"]["DP-2"]["position"] =
                    serde_json::json!([0.95, 0.95]);
                Config::save_changes(serde_json::json!({
                    "monitor_layouts":raw["monitor_layouts"],
                    "theme":"ocean"
                }))
                .unwrap();
            }
            let saved = std::fs::read(Config::path().unwrap()).unwrap();
            let task = cards.update(if keep {
                Message::LayoutKeepSaved
            } else {
                Message::LayoutRetryCancel
            });
            auditfix_no_arrange_capture(&emitted(&mut cards, task).1);
            let task = cards.update(Message::SurfaceResized(
                old_probe,
                cosmic::iced::Size::new(1280., 720.),
            ));
            auditfix_no_arrange_capture(&emitted(&mut cards, task).1);
            auditfix_no_arrange_capture(&auditfix_measure_outputs(&mut cards));
            assert!(!cards.layout_editor.active && cards.pending_arrange.is_none());
            assert!(cards.arrange_display.is_none());
            assert_eq!(
                cards.selected_output.as_ref().unwrap().name(),
                Some("eDP-1")
            );
            if conflict && !keep {
                let editor = cards.paused_layout.as_ref().unwrap();
                assert!(editor.exit_failed && editor.pending_resume.is_none());
                assert_eq!(editor.draft, draft);
                assert_eq!(editor.session_actions, receipts);
                assert_eq!(std::fs::read(Config::path().unwrap()).unwrap(), saved);
                let task = cards.update(Message::LayoutKeepSaved);
                auditfix_no_arrange_capture(&emitted(&mut cards, task).1);
            }
            assert!(cards.paused_layout.is_none());
            let config = Config::load();
            assert_eq!(
                config.monitor_layouts["weather"]["DP-2"].position,
                if conflict {
                    Some([0.95, 0.95])
                } else if keep {
                    committed
                } else {
                    Some([0.2, 0.2])
                }
            );
            assert_eq!(
                config.monitor_layouts["weather"]["eDP-1"].position,
                Some([0.8, 0.8])
            );
            if conflict {
                assert_eq!(config.theme, "ocean");
            }
        }
    }
}

// Adding and removing widgets in Arrange, with the real overlay.

/// Arrange on a 1920 × 1080 display showing `widgets`, with music off.
fn arranging(widgets: &[&str]) -> (Cards, crate::displays::test_support::Wire) {
    std::fs::write(
        Config::path().unwrap(),
        serde_json::json!({
            "enabled_widgets": widgets,
            "music": {"enabled": false},
            "placement": {"snap": false},
            "widget_positions": {"weather": [0.2, 0.2]},
        })
        .to_string(),
    )
    .unwrap();
    let (mut cards, _) = Cards::init(Core::default(), Config::load());
    let wire = cards.test_output_ready();
    cards.screen_size = [1920., 1080.];
    let task = cards.update(Message::Arrange);
    emitted(&mut cards, task);
    assert!(cards.layout_editor.active);
    (cards, wire)
}
fn overlay(cards: &Cards) -> crate::native_test::Native {
    crate::native_test::Native::new(cosmic::iced::Size::new(
        cards.screen_size[0],
        cards.screen_size[1],
    ))
}
/// Clicks the overlay at `point` with real input and applies what it sends.
/// Settings is never launched: its task is dropped unrun.
fn press_overlay(
    cards: &mut Cards,
    native: &mut crate::native_test::Native,
    point: Point,
) -> Vec<Message> {
    let mut sent = Vec::new();
    for event in [
        mouse::Event::CursorMoved { position: point },
        mouse::Event::ButtonPressed(mouse::Button::Left),
        mouse::Event::ButtonReleased(mouse::Button::Left),
    ] {
        let (messages, _) = native.event(
            cards.layout_view(),
            cosmic::iced::Event::Mouse(event),
            point,
        );
        for message in messages {
            sent.push(message.clone());
            if let Message::WidgetSettings(_) = message {
                drop(cards.update(message));
            } else {
                let task = cards.update(message);
                emitted(cards, task);
            }
        }
    }
    sent
}
fn click_overlay(
    cards: &mut Cards,
    native: &mut crate::native_test::Native,
    id: &str,
) -> Vec<Message> {
    let point = native.bounds(cards.layout_view(), id).center();
    press_overlay(cards, native, point)
}
fn shown(config: &Config) -> Vec<Widget> {
    Widget::ALL
        .into_iter()
        .filter(|widget| config.widget_visible(*widget))
        .collect()
}
fn assert_apart(cards: &Cards) {
    let rects: Vec<(Widget, [f32; 4])> = Widget::ALL
        .into_iter()
        .filter(|widget| cards.arranging_widget(*widget))
        .map(|widget| {
            let at = cards.position_from(widget, &cards.layout_editor.draft);
            let size = cards.widget_size(widget);
            (widget, [at[0], at[1], size[0], size[1]])
        })
        .collect();
    for (i, (wa, a)) in rects.iter().enumerate() {
        for (wb, b) in &rects[i + 1..] {
            let apart = a[0] + a[2] <= b[0]
                || b[0] + b[2] <= a[0]
                || a[1] + a[3] <= b[1]
                || b[1] + b[3] <= a[1];
            assert!(apart, "{wa:?} {a:?} overlaps {wb:?} {b:?}");
        }
    }
}

#[test]
fn arrange_remove_then_undo_brings_the_widget_back() {
    if isolated("arrange_remove_then_undo_brings_the_widget_back") {
        return;
    }
    let (mut cards, _wire) = arranging(&["weather", "devbox"]);
    let at = cards.layout_editor.draft["weather"];
    let mut native = overlay(&cards);
    let sent = click_overlay(&mut cards, &mut native, "arrange-remove-weather");
    assert!(
        sent.iter()
            .any(|m| matches!(m, Message::LayoutShow(Widget::Weather, false)))
    );
    assert!(!cards.arranging_widget(Widget::Weather));
    assert!(!Config::load().widget_visible(Widget::Weather));
    assert!(cards.layout_editor.active);
    let task = cards.update(Message::LayoutUndo);
    emitted(&mut cards, task);
    assert!(cards.arranging_widget(Widget::Weather));
    assert!(Config::load().widget_visible(Widget::Weather));
    assert_eq!(cards.layout_editor.draft["weather"], at);
    assert_eq!(
        Config::load().widget_positions.get("weather"),
        Some(&[0.2, 0.2])
    );

    // The inspector's Remove, then the Z key.
    let task = cards.update(Message::LayoutSelect(Widget::Weather));
    emitted(&mut cards, task);
    click_overlay(&mut cards, &mut native, "arrange-inspector-remove");
    assert!(!Config::load().widget_visible(Widget::Weather));
    assert_eq!(cards.layout_editor.selected, None);
    let task = cards.update(Message::SurfaceInput(
        cards.ids[5],
        Box::new(Message::LayoutKey(
            keyboard::Key::Character("z".into()),
            false,
        )),
    ));
    emitted(&mut cards, task);
    assert!(Config::load().widget_visible(Widget::Weather));
    assert!(cards.arranging_widget(Widget::Weather));
}

#[test]
fn arrange_cancel_restores_removed_and_added_widgets() {
    if isolated("arrange_cancel_restores_removed_and_added_widgets") {
        return;
    }
    let (mut cards, _wire) = arranging(&["weather", "devbox"]);
    let before = std::fs::read_to_string(Config::path().unwrap()).unwrap();
    let was = shown(&Config::load());
    let mut native = overlay(&cards);
    click_overlay(&mut cards, &mut native, "arrange-remove-weather");
    click_overlay(&mut cards, &mut native, "arrange-remove-devbox");
    click_overlay(&mut cards, &mut native, "arrange-add");
    click_overlay(&mut cards, &mut native, "arrange-add-notes");
    assert_eq!(shown(&Config::load()), vec![Widget::Notes]);
    let task = cards.update(Message::LayoutFinish(false));
    emitted(&mut cards, task);
    assert!(!cards.layout_editor.active && cards.paused_layout.is_none());
    assert_eq!(shown(&Config::load()), was);
    assert_eq!(shown(&cards.config), was);
    let after: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(Config::path().unwrap()).unwrap()).unwrap();
    let before: serde_json::Value = serde_json::from_str(&before).unwrap();
    for key in ["enabled_widgets", "widget_positions"] {
        assert_eq!(after.get(key), before.get(key), "{key}");
    }
    assert_ne!(
        after.pointer("/notes/enabled"),
        Some(&serde_json::Value::Bool(true))
    );
}

#[test]
fn arrange_done_keeps_a_removed_widget_off() {
    if isolated("arrange_done_keeps_a_removed_widget_off") {
        return;
    }
    let (mut cards, _wire) = arranging(&["weather", "devbox"]);
    let mut native = overlay(&cards);
    click_overlay(&mut cards, &mut native, "arrange-remove-weather");
    let task = cards.update(Message::LayoutFinish(true));
    emitted(&mut cards, task);
    assert!(!cards.layout_editor.active && cards.paused_layout.is_none());
    assert_eq!(shown(&Config::load()), vec![Widget::Devbox]);
    assert_eq!(shown(&cards.config), vec![Widget::Devbox]);
}

#[test]
fn arrange_add_lists_hidden_widgets_and_places_them_apart() {
    if isolated("arrange_add_lists_hidden_widgets_and_places_them_apart") {
        return;
    }
    let (mut cards, _wire) = arranging(&["weather", "devbox"]);
    let mut native = overlay(&cards);
    // The list opens from the toolbar and holds only the hidden widgets.
    assert!(
        native
            .try_bounds(cards.layout_view(), "arrange-add-clock")
            .is_none()
    );
    click_overlay(&mut cards, &mut native, "arrange-add");
    assert!(cards.layout_editor.adding);
    for widget in Widget::ALL {
        let listed = native
            .try_bounds(
                cards.layout_view(),
                &format!("arrange-add-{}", widget.key()),
            )
            .is_some();
        assert_eq!(listed, !cards.config.widget_visible(widget), "{widget:?}");
    }
    // Put Weather on the clock's spot: the clock must find another.
    let clock = cards.position_from(Widget::Clock, &cards.layout_editor.draft);
    cards.put_layout_point(Widget::Weather, clock);
    assert!(cards.commit_arrange());
    click_overlay(&mut cards, &mut native, "arrange-add-clock");
    assert!(cards.arranging_widget(Widget::Clock));
    assert!(Config::load().widget_visible(Widget::Clock));
    assert_eq!(cards.layout_editor.selected, Some(Widget::Clock));
    assert!(!cards.layout_editor.adding);
    assert_apart(&cards);
    for widget in [Widget::Notes, Widget::Focus, Widget::Glance] {
        click_overlay(&mut cards, &mut native, "arrange-add");
        click_overlay(
            &mut cards,
            &mut native,
            &format!("arrange-add-{}", widget.key()),
        );
        assert!(cards.arranging_widget(widget), "{widget:?}");
        assert_apart(&cards);
    }
    // Daily overview shows the clock, weather and calendar itself, so they
    // are not offered while it is on.
    let addable = cards.addable_widgets();
    for widget in [Widget::Clock, Widget::Weather, Widget::Calendar] {
        assert!(!addable.contains(&widget), "{widget:?}");
    }
    // With every widget shown the button says so and opens nothing.
    for widget in addable {
        let task = cards.update(Message::LayoutShow(widget, true));
        emitted(&mut cards, task);
    }
    assert!(cards.addable_widgets().is_empty());
    assert!(
        native
            .text_all(cards.layout_view(), "All widgets added")
            .len()
            == 1
    );
    let task = cards.update(Message::LayoutAddMenu);
    emitted(&mut cards, task);
    assert!(!cards.layout_editor.adding);
    let task = cards.update(Message::LayoutFinish(true));
    emitted(&mut cards, task);
    let config = Config::load();
    assert!(config.widget_visible(Widget::Devices));
    assert!(config.widget_visible(Widget::Focus));
}

#[test]
fn arrange_widget_settings_keeps_arranging() {
    if isolated("arrange_widget_settings_keeps_arranging") {
        return;
    }
    let (mut cards, _wire) = arranging(&["weather", "devbox"]);
    let mut native = overlay(&cards);
    cards.put_layout_point(Widget::Weather, [700., 300.]);
    assert!(cards.commit_arrange());
    let draft = cards.layout_editor.draft.clone();
    let history = cards.layout_editor.history.len();
    let receipts = cards.layout_editor.session_actions.len();
    let sent = click_overlay(&mut cards, &mut native, "arrange-settings-weather");
    assert!(
        sent.iter()
            .any(|m| matches!(m, Message::WidgetSettings(Widget::Weather)))
    );
    assert!(cards.layout_editor.active && cards.layout_editor.aside);
    assert_eq!(cards.layout_editor.draft, draft);
    assert_eq!(cards.layout_editor.history.len(), history);
    assert_eq!(cards.layout_editor.session_actions.len(), receipts);
    // The overlay waits behind the Settings window, without the keyboard,
    // also if its surface is made again.
    let probe = |cards: &Cards| {
        let task = cards.layout_probe(cards.ids[5], true);
        let mut copy = Cards::init(Core::default(), Config::load()).0;
        emitted(&mut copy, task).1.pop().unwrap()
    };
    let aside = probe(&cards);
    assert_eq!(aside.layer, Layer::Bottom);
    assert_eq!(aside.keyboard_interactivity, KeyboardInteractivity::None);
    // The inspector's Settings works the same way.
    let task = cards.update(Message::LayoutSelect(Widget::Devbox));
    emitted(&mut cards, task);
    let point = native
        .bounds(cards.layout_view(), "arrange-inspector-settings")
        .center();
    // While aside, a first click anywhere only brings Arrange back.
    let sent = press_overlay(&mut cards, &mut native, point);
    assert!(
        sent.iter().all(|m| matches!(m, Message::LayoutFront)),
        "{sent:?}"
    );
    assert!(!cards.layout_editor.aside && cards.layout_editor.drag.is_none());
    let front = probe(&cards);
    assert_eq!(front.layer, Layer::Overlay);
    assert_eq!(
        front.keyboard_interactivity,
        KeyboardInteractivity::Exclusive
    );
    let sent = press_overlay(&mut cards, &mut native, point);
    assert!(
        sent.iter()
            .any(|m| matches!(m, Message::WidgetSettings(Widget::Devbox)))
    );
    assert!(cards.layout_editor.aside && cards.layout_editor.active);
    assert_eq!(cards.layout_editor.draft, draft);
    // Done still saves the draft afterwards.
    let task = cards.update(Message::LayoutFront);
    emitted(&mut cards, task);
    let task = cards.update(Message::LayoutFinish(true));
    emitted(&mut cards, task);
    assert_eq!(Config::load().widget_positions["weather"], draft["weather"]);
}

#[test]
fn arrange_card_buttons_take_the_click_before_the_drag() {
    if isolated("arrange_card_buttons_take_the_click_before_the_drag") {
        return;
    }
    // Clock is the smallest card.
    let (mut cards, _wire) = arranging(&["weather", "clock"]);
    let mut native = overlay(&cards);
    for widget in [Widget::Weather, Widget::Clock] {
        for id in ["settings", "remove"] {
            let point = native
                .bounds(
                    cards.layout_view(),
                    &format!("arrange-{id}-{}", widget.key()),
                )
                .center();
            let at = cards.position_from(widget, &cards.layout_editor.draft);
            let size = cards.widget_size(widget);
            assert!(
                point.x > at[0] && point.x < at[0] + size[0] && point.y > at[1],
                "{widget:?} {id} inside its card"
            );
            native.event(
                cards.layout_view(),
                cosmic::iced::Event::Mouse(mouse::Event::CursorMoved { position: point }),
                point,
            );
            let (messages, _) = native.event(
                cards.layout_view(),
                cosmic::iced::Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)),
                point,
            );
            assert!(
                !messages
                    .iter()
                    .any(|m| matches!(m, Message::LayoutPress(_) | Message::LayoutResizePress(_))),
                "{widget:?} {id}: {messages:?}"
            );
            native.event(
                cards.layout_view(),
                cosmic::iced::Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)),
                point,
            );
        }
        // The rest of the card still drags.
        let at = cards.position_from(widget, &cards.layout_editor.draft);
        let point = Point::new(at[0] + 20., at[1] + 60.);
        let sent = press_overlay(&mut cards, &mut native, point);
        assert!(
            sent.iter()
                .any(|m| matches!(m, Message::LayoutPress(w) if *w == widget)),
            "{widget:?}: {sent:?}"
        );
        cards.cancel_layout_drag();
    }
    // A real click on the Clock's × removes it and starts no drag.
    let sent = click_overlay(&mut cards, &mut native, "arrange-remove-clock");
    assert!(
        !sent
            .iter()
            .any(|m| matches!(m, Message::LayoutPress(_) | Message::LayoutResizePress(_)))
    );
    assert!(cards.layout_editor.drag.is_none());
    assert!(!cards.arranging_widget(Widget::Clock));
}
