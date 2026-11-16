// Included inside settings::tests to share the isolated desktop/native harness.
// The bars, shortcuts and island are easy to use: your floating apps come
// first and defaults fold away (BAR-03), shortcuts say what is yours, open
// apps and move built-in keys (BAR-04, BAR-05, BAR-11), the island's page
// gives a way on and explains itself (BAR-06, BAR-08), and item edits wait
// for a pause and restart the bar once (BAR-09).

const RULES: &str = "com.system76.CosmicSettings.WindowRules";
const SHORTCUTS: &str = "com.system76.CosmicSettings.Shortcuts";
const PANEL: &str = "com.system76.CosmicPanel.Panel";

/// A desktop entry in the fixture's applications folder.
fn bars_app(root: &std::path::Path, id: &str, body: &str) {
    let dir = root.join("cosmic-root/applications");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(
        dir.join(format!("{id}.desktop")),
        format!("[Desktop Entry]\nType=Application\n{body}"),
    )
    .unwrap();
}

fn bars_user(root: &std::path::Path, namespace: &str, key: &str) -> Option<String> {
    std::fs::read_to_string(
        root.join("cosmic-root/config/cosmic")
            .join(namespace)
            .join("v1")
            .join(key),
    )
    .ok()
}

fn bars_open(root: &std::path::Path, page: Page) -> (Settings, crate::native_test::Native) {
    let mut app = open_isolated_editor(root);
    let size = Size::new(1280., 800.);
    send_settings(&mut app, Message::Resized(size));
    send_settings(&mut app, Message::Page(page));
    (app, crate::native_test::Native::new(size))
}

fn bars_capture(app: &Settings, native: &mut crate::native_test::Native, name: &str) {
    for (theme, shade) in [
        (cosmic::Theme::light(), "light"),
        (cosmic::Theme::dark(), "dark"),
    ] {
        native.capture(app.settings_view(), &theme, &format!("{name}-{shade}"));
    }
}

/// BAR-03: your own floating apps come first, with Add; COSMIC's defaults
/// fold under their count. Patterns read as written, and a contains or
/// other pattern that matches one installed app takes its name. Saving
/// keeps COSMIC's order.
#[test]
fn bar03_floating_apps_list_yours_first_and_fold_the_defaults() {
    let name = "bar03_floating_apps_list_yours_first_and_fold_the_defaults";
    if desktop_test_child(
        name,
        DesktopEnv {
            app_config: true,
            fixture: true,
            ..Default::default()
        },
    ) {
        return;
    }
    use crate::settings::window_rules::Message as Rules;
    let root = isolated_root();
    seed(
        &root.join("cosmic-root/system/cosmic"),
        &[(
            RULES,
            "tiling_exception_defaults",
            r#"[(appid: "^com\\.example\\.Calc$", titles: [""]), (appid: "steam", titles: ["^Friends", ".*"]), (appid: "^org\\.example\\.Clocks", titles: ["Alarm"])]"#,
        )],
    );
    seed(
        &root.join("cosmic-root/config/cosmic"),
        &[(
            RULES,
            "tiling_exception_custom",
            r#"[(appid: "^org\\.example\\.Notes$", title: "", enabled: true), (appid: "telegram", title: "", enabled: true), (appid: "^(foo|bar)$", title: "", enabled: true)]"#,
        )],
    );
    bars_app(&root, "org.example.Notes", "Name=Notes\nIcon=notes\n");
    bars_app(
        &root,
        "org.telegram.desktop",
        "Name=Telegram\nIcon=telegram\n",
    );
    bars_app(&root, "com.example.Calc", "Name=Calculator\n");
    bars_app(&root, "org.example.Clocks", "Name=Clocks\n");
    let (mut app, mut native) = bars_open(&root, Page::DesktopWindowRules);
    // Yours first, with Add inside; the defaults below, folded.
    let yours = native.bounds(app.settings_view(), "window-rules-yours");
    let add = native.bounds(app.settings_view(), "window-rules-add");
    assert!(yours.contains(add.center()), "{yours:?} {add:?}");
    let defaults = native.bounds(app.settings_view(), "window-rules-defaults");
    assert!(
        yours.y + yours.height <= defaults.y,
        "{yours:?} {defaults:?}"
    );
    native.text_bounds(app.settings_view(), "Your apps");
    native.text_bounds(app.settings_view(), "COSMIC defaults (4)");
    for hidden in ["Calculator", "Clocks", "App ID contains “steam”"] {
        assert!(
            native.text_all(app.settings_view(), hidden).is_empty(),
            "{hidden} is folded"
        );
    }
    // The exact ID and the contains rule name their app; a pattern that
    // names none reads as written.
    native.text_bounds(app.settings_view(), "Notes");
    native.text_bounds(app.settings_view(), "Telegram");
    native.text_bounds(
        app.settings_view(),
        "App ID contains “telegram” · Any title",
    );
    native.text_bounds(app.settings_view(), "^(foo|bar)$");
    assert!(
        native
            .text_all(app.settings_view(), "App ID pattern")
            .is_empty()
    );
    bars_capture(&app, &mut native, "bar03-floating-folded");
    // Unfolded: each default is told apart, a prefix names its app.
    native_click(&mut app, &mut native, "window-rules-defaults");
    assert!(app.window_rules.defaults_open);
    native.text_bounds(app.settings_view(), "Calculator");
    native.text_bounds(app.settings_view(), "Clocks");
    native.text_bounds(
        app.settings_view(),
        "App ID matches “^org\\.example\\.Clocks” · Title contains “Alarm”",
    );
    assert_eq!(
        native
            .text_all(app.settings_view(), "App ID contains “steam”")
            .len(),
        2
    );
    bars_capture(&app, &mut native, "bar03-floating-open");
    native_click(&mut app, &mut native, "window-rules-defaults");
    assert!(!app.window_rules.defaults_open);
    // A new app lands in Your apps, and the list keeps COSMIC's order.
    native_click(&mut app, &mut native, "window-rules-add");
    assert!(app.window_rules.form.is_some());
    send_settings(
        &mut app,
        Message::WindowRules(Rules::AppId("org.example.New".into())),
    );
    send_settings(&mut app, Message::WindowRules(Rules::Save));
    let saved = bars_user(&root, RULES, "tiling_exception_custom").unwrap();
    let at = |text: &str| {
        saved
            .find(text)
            .unwrap_or_else(|| panic!("{text}: {saved}"))
    };
    assert!(at("Notes") < at("telegram") && at("telegram") < at("foo|bar"));
    assert!(at("foo|bar") < at("org\\\\.example\\\\.New"), "{saved}");
    let yours = native.bounds(app.settings_view(), "window-rules-yours");
    let new = native.text_bounds(app.settings_view(), "org.example.New");
    assert!(yours.contains(new.center()), "{yours:?} {new:?}");
}

/// BAR-04, BAR-05, BAR-11: with none of your own, Your shortcuts says so
/// first and lists the built-in ones under their own heading. A built-in
/// action moves to other keys as COSMIC does it; the form opens installed
/// apps; recording asks the desktop to hold its own shortcuts back.
#[test]
fn bar04_shortcuts_say_whats_yours_open_apps_and_move_builtin_keys() {
    let name = "bar04_shortcuts_say_whats_yours_open_apps_and_move_builtin_keys";
    if desktop_test_child(
        name,
        DesktopEnv {
            app_config: true,
            fixture: true,
            ..Default::default()
        },
    ) {
        return;
    }
    use crate::settings::shortcuts::{Category, Filter, Message as Keys};
    let root = isolated_root();
    seed(
        &root.join("cosmic-root/system/cosmic"),
        &[(
            SHORTCUTS,
            "defaults",
            r#"{(modifiers: [Super], key: "t"): System(Terminal), (modifiers: [Super], key: "q"): Close}"#,
        )],
    );
    bars_app(
        &root,
        "org.example.Editor",
        "Name=Editor\nExec=sentinel-editor %U\n",
    );
    let (mut app, mut native) = bars_open(&root, Page::DesktopShortcuts);
    assert_eq!(app.shortcuts.filter, Filter::Custom);
    // The note first, with Add; the built-in ones under their heading.
    let note = native.bounds(app.settings_view(), "shortcuts-empty");
    native.text_bounds(app.settings_view(), "No shortcuts of your own yet");
    let add = native.bounds(app.settings_view(), "shortcuts-add-first");
    assert!(note.contains(add.center()));
    let heading = native.bounds(app.settings_view(), "shortcuts-builtin-heading");
    native.text_bounds(app.settings_view(), "Built-in shortcuts");
    assert!(note.y + note.height <= heading.y, "{note:?} {heading:?}");
    bars_capture(&app, &mut native, "bar04-shortcuts-empty");
    // Change keys on a built-in row moves its action.
    send_settings(&mut app, Message::Shortcuts(Keys::Category(Category::Apps)));
    native_click(&mut app, &mut native, "shortcut-change-Super+T");
    let form = app.shortcuts.form.as_ref().expect("form");
    assert!(form.rebinding.is_some() && form.name == "Open terminal");
    assert!(
        !native
            .text_all(app.settings_view(), "Change keys")
            .is_empty()
    );
    assert!(
        native
            .try_bounds(app.settings_view(), "shortcut-pick-app")
            .is_none()
    );
    send_settings(
        &mut app,
        Message::Shortcuts(Keys::Combo("Super+Alt+T".into())),
    );
    bars_capture(&app, &mut native, "bar04-change-keys");
    native_click(&mut app, &mut native, "shortcut-save");
    assert!(app.shortcuts.form.is_none(), "{:?}", app.shortcuts.notice());
    let custom = bars_user(&root, SHORTCUTS, "custom").expect("written");
    let read = crate::desktop::shortcuts::format::parse_map(&custom).unwrap();
    assert_eq!(read.len(), 2, "{custom}");
    assert_eq!(read[0].action_raw, "System(Terminal)");
    assert_eq!(
        read[0].ident(),
        crate::desktop::shortcuts::Binding::parse("Super+Alt+T")
            .unwrap()
            .ident()
    );
    assert!(read[1].is_disable());
    // Now there is one of your own, so the note goes.
    assert!(
        native
            .try_bounds(app.settings_view(), "shortcuts-empty")
            .is_none()
    );
    assert_eq!(
        native.text_all(app.settings_view(), "Your shortcuts").len(),
        2,
        "the tab and the group"
    );
    send_settings(&mut app, Message::UndoDesktop);
    assert!(
        bars_user(&root, SHORTCUTS, "custom").is_none_or(|text| {
            crate::desktop::shortcuts::format::parse_map(&text)
                .unwrap()
                .is_empty()
        }),
        "Undo puts the defaults back"
    );
    // Add opens an installed app: it fills the command and a name.
    native_click(&mut app, &mut native, "shortcuts-add-first");
    native_click(&mut app, &mut native, "shortcut-pick-app");
    native_click(&mut app, &mut native, "app-pick-org.example.Editor");
    let form = app.shortcuts.form.as_ref().unwrap();
    assert_eq!(form.command, "sentinel-editor");
    assert_eq!(form.name, "Open Editor");
    assert!(!form.picker);
    // Recording holds the desktop's own shortcuts back while it lasts.
    assert!(!app.shortcuts_inhibit);
    native_click(&mut app, &mut native, "shortcut-record");
    assert!(app.shortcuts.recording() && app.shortcuts_inhibit);
    native.text_bounds(
        app.settings_view(),
        "If the desktop uses those keys, cancel and type the combination instead.",
    );
    send_settings(&mut app, Message::Shortcuts(Keys::Inhibited(true)));
    native.text_bounds(
        app.settings_view(),
        "Desktop shortcuts are paused while you record.",
    );
    bars_capture(&app, &mut native, "bar11-recording");
    send_settings(
        &mut app,
        Message::Shortcuts(Keys::Recorded(Ok("Super+Alt+E".into()))),
    );
    assert!(!app.shortcuts.recording() && !app.shortcuts_inhibit);
    assert_eq!(app.shortcuts.form.as_ref().unwrap().combo, "Super+Alt+E");
    // Leaving the page while recording lets the desktop have its keys back.
    send_settings(&mut app, Message::Shortcuts(Keys::Record));
    assert!(app.shortcuts_inhibit);
    send_settings(&mut app, Message::Page(Page::Home));
    assert!(!app.shortcuts_inhibit);
}

/// BAR-06, BAR-08: with the panel on another edge, the island's page says
/// so and moves the panel to the top; its options and how to use it show
/// before it is added, each with what it does.
#[test]
fn bar06_island_page_says_why_and_moves_the_panel_to_the_top() {
    let name = "bar06_island_page_says_why_and_moves_the_panel_to_the_top";
    if desktop_test_child(
        name,
        DesktopEnv {
            app_config: true,
            fixture: true,
            ..Default::default()
        },
    ) {
        return;
    }
    let root = isolated_root();
    seed(
        &root.join("cosmic-root/config/cosmic"),
        &[(PANEL, "anchor", "Bottom")],
    );
    let packaged = std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/packaging/io.github.profilaz3.Halcyon.Activity.desktop"
    ))
    .unwrap();
    let dir = root.join("cosmic-root/applications");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(
        dir.join(format!("{}.desktop", crate::activity_island::APPLET_ID)),
        packaged,
    )
    .unwrap();
    let (mut app, mut native) = bars_open(&root, Page::ActivityIsland);
    // Said plainly, with the step that fixes it.
    native.text_bounds(app.settings_view(), "Your panel is on the bottom edge");
    native.text_bounds(
        app.settings_view(),
        "The Activity island needs a top panel.",
    );
    native_reveal(&app, &mut native, "island-move-panel");
    // What it offers shows already, with how to use it.
    native_reveal(&app, &mut native, "activity-preference-media");
    native_reveal(&app, &mut native, "activity-gestures");
    for (gesture, effect) in crate::settings::view::GESTURES {
        native.text_bounds(app.settings_view(), gesture);
        native.text_bounds(app.settings_view(), effect);
    }
    bars_capture(&app, &mut native, "bar06-island-bottom-panel");
    // Each option says what it does behind its "?".
    send_settings(&mut app, Message::ShowExplanations(true));
    native.text_bounds(app.settings_view(), crate::settings::view::POPUPS_NOTE);
    native.text_bounds(
        app.settings_view(),
        "Opens when a connected device falls to 20% and to 10%.",
    );
    native.text_bounds(
        app.settings_view(),
        "Opens by itself for a few seconds when something new happens.",
    );
    bars_capture(&app, &mut native, "bar08-island-explained");
    send_settings(&mut app, Message::ShowExplanations(false));
    // Move panel to the top uses the Panel page's own Screen edge.
    native_click(&mut app, &mut native, "island-move-panel");
    assert_eq!(bars_user(&root, PANEL, "anchor").as_deref(), Some("Top"));
    assert!(
        native
            .try_bounds(app.settings_view(), "island-move-panel")
            .is_none()
    );
    // Then it can be added here.
    native_click(&mut app, &mut native, "panel-add-activity");
    pause_items(&mut app);
    assert!(
        bars_user(&root, PANEL, "plugins_center")
            .is_some_and(|text| text.contains(crate::activity_island::APPLET_ID))
    );
    assert!(
        native
            .try_bounds(app.settings_view(), "island-open-panel")
            .is_none()
    );
    native_reveal(&app, &mut native, "activity-preference-media");
}

/// BAR-09: Move to… is a visible control; edits wait for a pause, with
/// the arrows still usable, and then restart the bar once, as one change
/// Undo puts back; leaving the page or closing the window applies what
/// waited; Ctrl+Z before then puts the items back without writing. The
/// starter layout keeps the Activity island and says what it removes.
#[test]
fn bar09_items_wait_for_a_pause_and_restart_the_bar_once() {
    let name = "bar09_items_wait_for_a_pause_and_restart_the_bar_once";
    if desktop_test_child(
        name,
        DesktopEnv {
            app_config: true,
            fixture: true,
            ..Default::default()
        },
    ) {
        return;
    }
    use crate::desktop::panels::{PanelKind, Regions};
    let root = isolated_root();
    for (id, label) in [("a", "Alpha"), ("b", "Beta"), ("c", "Gamma")] {
        bars_app(&root, id, &format!("Name={label}\nX-CosmicApplet=true\n"));
    }
    let island = crate::activity_island::APPLET_ID;
    let center = format!("Some([\"{island}\"])");
    seed(
        &root.join("cosmic-root/config/cosmic"),
        &[
            (PANEL, "plugins_wings", r#"Some((["a", "b"], ["c"]))"#),
            (PANEL, "plugins_center", center.as_str()),
        ],
    );
    let (mut app, mut native) = bars_open(&root, Page::DesktopPanel);
    let items = |app: &Settings| {
        app.desktop
            .current()
            .unwrap()
            .applets()
            .known()
            .cloned()
            .unwrap()
    };
    let start = items(&app);
    let written = || bars_user(&root, PANEL, "plugins_wings");
    let before = written();
    // Each row has a visible Move to…, which opens its regions.
    native_click(&mut app, &mut native, "applet-start-0-move-to");
    assert_eq!(
        app.desktop.focus,
        Some(crate::desktop::panels::Slot {
            region: crate::desktop::panels::Region::Start,
            index: 0
        })
    );
    native.text_bounds(app.settings_view(), "Move to");
    assert!(!app.desktop.items_held());
    // Alpha goes from the left end to the right end: five steps, no write.
    for _ in 0..5 {
        let slot = app.desktop.focus.unwrap_or(crate::desktop::panels::Slot {
            region: crate::desktop::panels::Region::Start,
            index: 0,
        });
        let id = format!("applet-{}-{}-down", slot.region.key(), slot.index);
        native_click(&mut app, &mut native, &id);
        assert!(app.desktop.items_held(), "the arrows stay usable");
    }
    assert_eq!(written(), before, "nothing restarts while you move it");
    assert_eq!(items(&app).end, ["c", "a"]);
    native.text_bounds(
        app.settings_view(),
        "Applying when you pause. The panel restarts once.",
    );
    bars_capture(&app, &mut native, "bar09-items-held");
    // The pause: one write, one change, one Undo.
    pause_items(&mut app);
    assert_ne!(written(), before);
    assert!(!app.desktop.dirty());
    assert_eq!(
        app.last_desktop_summary.as_ref().unwrap().title(),
        "Moved Alpha to Right"
    );
    send_settings(&mut app, Message::UndoDesktop);
    assert_eq!(items(&app), start);
    assert_eq!(written(), before);
    // Ctrl+Z before the pause puts the items back without writing.
    send_settings(
        &mut app,
        Message::DesktopEditor(desktop::Message::Remove(crate::desktop::panels::Slot {
            region: crate::desktop::panels::Region::Start,
            index: 0,
        })),
    );
    send_settings(&mut app, Message::KeyboardUndo(false));
    assert!(!app.desktop.items_held());
    assert_eq!(items(&app), start);
    assert_eq!(written(), before);
    // Leaving the page applies what waited.
    send_settings(
        &mut app,
        Message::DesktopEditor(desktop::Message::Remove(crate::desktop::panels::Slot {
            region: crate::desktop::panels::Region::Start,
            index: 0,
        })),
    );
    send_settings(&mut app, Message::Page(Page::Home));
    assert!(!app.desktop.dirty());
    assert!(written().is_some_and(|text| !text.contains("\"a\"")));
    send_settings(&mut app, Message::Page(Page::DesktopPanel));
    send_settings(&mut app, Message::UndoDesktop);
    assert_eq!(items(&app), start);
    // So does closing the window.
    send_settings(
        &mut app,
        Message::DesktopEditor(desktop::Message::Remove(crate::desktop::panels::Slot {
            region: crate::desktop::panels::Region::Start,
            index: 0,
        })),
    );
    send_settings(&mut app, Message::FlushEdits);
    assert!(!app.desktop.dirty());
    assert!(written().is_some_and(|text| !text.contains("\"a\"")));
    send_settings(&mut app, Message::UndoDesktop);
    // The starter keeps the island and says what goes.
    send_settings(
        &mut app,
        Message::DesktopEditor(desktop::Message::Starter(true)),
    );
    native_reveal(&app, &mut native, "panel-starter-removes");
    native.text_bounds(app.settings_view(), "Keeps the Activity island.");
    native.text_bounds(app.settings_view(), "Removes Alpha, Beta, Gamma.");
    bars_capture(&app, &mut native, "bar09-starter");
    assert!(app.desktop.starter().contains(island));
    assert_ne!(app.desktop.starter(), Regions::default());
    // On the dock as well, a pause applies the items once.
    send_settings(&mut app, Message::OpenBar(PanelKind::Dock));
    let dock = || bars_user(&root, &PanelKind::Dock.namespace(), "plugins_wings");
    let dock_before = dock();
    native_click(&mut app, &mut native, "applets-add-start");
    native_click(&mut app, &mut native, "applet-picker-add-b");
    assert!(app.desktop.items_held());
    assert_eq!(dock(), dock_before);
    pause_items(&mut app);
    assert!(dock().is_some_and(|text| text.contains("\"b\"")));
}
