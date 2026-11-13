// Included inside settings::tests to share the isolated desktop/native harness.
// The Edge menu page: the strip's settings, the item list and its edits,
// and the Add picker.
use crate::config::edge_menu::{Action, Item, Kind, OpenOn, Side};
use edge_menu::Edit as EdgeEdit;

fn edge_page() -> Settings {
    let (mut app, task) = Settings::init(Core::default(), Page::EdgeMenu);
    settle_settings(&mut app, task);
    send_settings(&mut app, Message::Page(Page::EdgeMenu));
    app
}

fn edge_send(app: &mut Settings, edit: EdgeEdit) {
    send_settings(app, Message::EdgeMenu(edit));
    assert!(!app.error, "{}", app.status);
}

fn edge_saved() -> serde_json::Value {
    let text = std::fs::read_to_string(Config::path().unwrap()).unwrap();
    serde_json::from_str::<serde_json::Value>(&text).unwrap()["edge_menu"].clone()
}

/// A desktop entry in the fixture's own application list.
fn edge_fixture_app(id: &str, name: &str, icon: &str) {
    let store = crate::desktop::DesktopStore::from_env().unwrap();
    let dir = store.fixture_root().unwrap().join("applications");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(
        dir.join(format!("{id}.desktop")),
        format!("[Desktop Entry]\nType=Application\nName={name}\nIcon={icon}\nExec=true\n"),
    )
    .unwrap();
}

#[test]
fn edge_menu_strip_settings_save_and_reload() {
    let name = "edge_menu_strip_settings_save_and_reload";
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
    let _root = isolated_root();
    let mut app = edge_page();
    assert_eq!(Page::parse("edge-menu"), Page::EdgeMenu);
    edge_send(&mut app, EdgeEdit::Enabled(false));
    edge_send(&mut app, EdgeEdit::Side(Side::Left));
    edge_send(&mut app, EdgeEdit::Position(0));
    // A slider moves without saving, and saves once let go.
    edge_send(&mut app, EdgeEdit::Draft(edge_menu::Field::Length, 40));
    assert_eq!(Config::load().edge_menu.length, 100);
    edge_send(&mut app, EdgeEdit::Commit);
    edge_send(&mut app, EdgeEdit::Open(OpenOn::Click));
    let saved = Config::load().edge_menu;
    assert!(!saved.enabled);
    assert_eq!(saved.side, Side::Left);
    assert_eq!((saved.position, saved.length), (0, 40));
    assert_eq!(saved.open, OpenOn::Click);
    assert_eq!(saved.dwell(), None);
    // Undo names the setting it takes back.
    assert_eq!(
        crate::desktop::store::widget::label("/edge_menu/open"),
        "Edge menu opening"
    );
    // The description says how to reach the menu with the strip off.
    let mut native = crate::native_test::Native::new(Size::new(1280., 800.));
    send_settings(&mut app, Message::Resized(Size::new(1280., 800.)));
    send_settings(&mut app, Message::Explain(view::explain::key("Edge menu")));
    let text = native.all_text(app.settings_view()).join(" ");
    assert!(text.contains("halcyon --toggle drawer"), "{text}");
    assert!(native.duplicate_ids(app.settings_view()).is_empty());
}

#[test]
fn edge_menu_items_reorder_remove_reset_and_keep_unknown_entries() {
    let name = "edge_menu_items_reorder_remove_reset_and_keep_unknown_entries";
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
    let _root = isolated_root();
    let mut app = edge_page();
    let builtin = app.config.edge_items();
    assert!(!app.config.edge_menu.customized());
    // Adding starts the list from the built-in menu.
    edge_send(&mut app, EdgeEdit::Add(Item::action(Action::DoNotDisturb)));
    let items = Config::load().edge_items();
    assert_eq!(items.len(), builtin.len() + 1);
    assert_eq!(items[..builtin.len()], builtin[..]);
    // Listed once.
    edge_send(&mut app, EdgeEdit::Add(Item::action(Action::DoNotDisturb)));
    assert_eq!(Config::load().edge_items().len(), builtin.len() + 1);
    let last = builtin.len();
    let dnd = |items: &[Item]| {
        items
            .iter()
            .position(|item| item.as_action() == Some(Action::DoNotDisturb))
            .unwrap()
    };
    // Keyboard: Move up and Move down.
    edge_send(&mut app, EdgeEdit::MoveUp(last));
    assert_eq!(dnd(&Config::load().edge_items()), last - 1);
    edge_send(&mut app, EdgeEdit::MoveDown(last - 1));
    assert_eq!(dnd(&Config::load().edge_items()), last);
    // Moving past either end does nothing.
    edge_send(&mut app, EdgeEdit::MoveDown(last));
    assert_eq!(dnd(&Config::load().edge_items()), last);
    // Drag to the top.
    edge_send(&mut app, EdgeEdit::DragStart(last));
    edge_send(&mut app, EdgeEdit::DragOver(3));
    edge_send(&mut app, EdgeEdit::DragOver(0));
    edge_send(&mut app, EdgeEdit::DragDrop);
    assert_eq!(dnd(&Config::load().edge_items()), 0);
    // A drag let go outside the list, or cancelled, changes nothing.
    edge_send(&mut app, EdgeEdit::DragStart(0));
    edge_send(&mut app, EdgeEdit::DragOver(4));
    edge_send(&mut app, EdgeEdit::DragCancel);
    assert_eq!(dnd(&Config::load().edge_items()), 0);
    // Remove.
    edge_send(&mut app, EdgeEdit::Remove(0));
    assert_eq!(Config::load().edge_items(), builtin);
    // An item from a newer Halcyon survives edits around it.
    let mut raw = edge_saved()["items"].as_array().unwrap().clone();
    let hologram = serde_json::json!({"type": "hologram", "colour": "teal", "depth": [1, 2]});
    raw.push(hologram.clone());
    raw[1]["badge"] = serde_json::json!(7);
    app.config = Config::save_edge_menu(serde_json::json!({ "items": raw })).unwrap();
    send_settings(&mut app, Message::Page(Page::EdgeMenu));
    let count = app.config.edge_items().len();
    edge_send(&mut app, EdgeEdit::MoveUp(count - 1));
    let saved = edge_saved()["items"].as_array().unwrap().clone();
    assert_eq!(saved[count - 2], hologram);
    assert_eq!(saved[1]["badge"], 7, "unknown fields of known items stay");
    assert!(
        !app.edge_menu.available[count - 2],
        "an unknown item is unavailable"
    );
    // A section title is renamed by typing, kept with Enter.
    edge_send(&mut app, EdgeEdit::TitleText(0, "Pinned".into()));
    edge_send(&mut app, EdgeEdit::Commit);
    assert!(matches!(
        &Config::load().edge_items()[0].kind,
        Kind::Title { text } if text == "Pinned"
    ));
    // Reset brings back the built-in menu.
    edge_send(&mut app, EdgeEdit::Reset);
    let config = Config::load();
    assert!(!config.edge_menu.customized());
    assert_eq!(config.edge_items(), builtin);
}

#[test]
fn edge_menu_picker_lists_apps_and_a_missing_app_is_unavailable() {
    let name = "edge_menu_picker_lists_apps_and_a_missing_app_is_unavailable";
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
    edge_fixture_app("org.example.Player", "Player", "multimedia-player");
    edge_fixture_app(
        "org.example.Editor",
        "Text Editor",
        "accessories-text-editor",
    );
    let folder = root.join("Projects");
    std::fs::create_dir_all(&folder).unwrap();
    let mut app = edge_page();
    edge_send(&mut app, EdgeEdit::Picker(true));
    let apps: Vec<_> = app
        .edge_menu
        .matching_apps()
        .iter()
        .map(|app| app.name.clone())
        .collect();
    assert_eq!(apps, ["Player", "Text Editor"]);
    edge_send(&mut app, EdgeEdit::AppSearch("edit".into()));
    assert_eq!(app.edge_menu.matching_apps().len(), 1);
    assert!(
        app.edge_menu
            .looks
            .as_ref()
            .is_some_and(|looks| !looks.is_empty()),
        "built-in Looks are offered"
    );
    let items = vec![
        Item::title("Shortcuts"),
        Item::new(Kind::App {
            app: "org.example.Player".into(),
            name: "Player".into(),
            icon: "multimedia-player".into(),
        }),
        Item::new(Kind::App {
            app: "org.example.Gone".into(),
            name: "Gone".into(),
            icon: String::new(),
        }),
        edge_menu::folder_item(&folder),
        edge_menu::folder_item(&root.join("Deleted")),
        Item::new(Kind::Separator),
        Item::action(Action::Timer),
    ];
    app.config = Config::save_edge_menu(serde_json::json!({ "items": items })).unwrap();
    send_settings(&mut app, Message::Page(Page::EdgeMenu));
    assert_eq!(
        app.edge_menu.available,
        [true, true, false, true, false, true, true]
    );
    let mut native = crate::native_test::Native::new(Size::new(1280., 800.));
    send_settings(&mut app, Message::Resized(Size::new(1280., 800.)));
    let names: Vec<_> = native
        .a11y(app.settings_view())
        .into_iter()
        .filter_map(|(_, name, _)| name)
        .collect();
    for wanted in ["Move Player up", "Move Gone down", "Remove Gone"] {
        assert!(
            names.iter().any(|name| name == wanted),
            "{wanted}: {names:?}"
        );
    }
    let text = native.all_text(app.settings_view()).join(" ");
    assert!(text.contains("App · Unavailable"), "{text}");
    assert!(native.duplicate_ids(app.settings_view()).is_empty());
}

/// The page with the strip off, the list beside its preview, and the Add
/// picker, at 1280×800 in both themes.
#[test]
fn edge_menu_page_captures() {
    let name = "edge_menu_page_captures";
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
    edge_fixture_app("org.example.Player", "Player", "multimedia-player");
    edge_fixture_app("firefox", "Firefox", "firefox");
    edge_fixture_app("org.gnome.Nautilus", "Files", "system-file-manager");
    let mut app = edge_page();
    let size = Size::new(1280., 800.);
    send_settings(&mut app, Message::Resized(size));
    edge_send(&mut app, EdgeEdit::Enabled(false));
    let mut native = crate::native_test::Native::new(size);
    for (theme, shade) in [
        (cosmic::Theme::dark(), "dark"),
        (cosmic::Theme::light(), "light"),
    ] {
        native.capture(
            app.settings_view(),
            &theme,
            &format!("edge-menu-off-{shade}"),
        );
    }
    edge_send(&mut app, EdgeEdit::Enabled(true));
    let downloads = root.join("Downloads");
    std::fs::create_dir_all(&downloads).unwrap();
    let items = vec![
        Item::title("Apps"),
        Item::new(Kind::App {
            app: "firefox".into(),
            name: "Firefox".into(),
            icon: "firefox".into(),
        }),
        Item::new(Kind::App {
            app: "org.gnome.Nautilus".into(),
            name: "Files".into(),
            icon: "system-file-manager".into(),
        }),
        edge_menu::folder_item(&downloads),
        Item::new(Kind::Separator),
        Item::title("Quick actions"),
        Item::action(Action::ShowWidgets),
        Item::action(Action::DoNotDisturb),
        Item::action(Action::Timer),
        Item::action(Action::Focus),
        Item::widget(Widget::Weather),
        Item::new(Kind::Separator),
        Item::action(Action::Arrange),
        Item::action(Action::Settings),
    ];
    app.config = Config::save_edge_menu(serde_json::json!({ "items": items })).unwrap();
    send_settings(&mut app, Message::Page(Page::EdgeMenu));
    // The list's top, beside the preview of the menu it makes.
    native.operate(
        app.settings_view(),
        Box::new(navigation::Reveal::target("edge-menu-items")),
    );
    for (theme, shade) in [
        (cosmic::Theme::dark(), "dark"),
        (cosmic::Theme::light(), "light"),
    ] {
        native.capture(
            app.settings_view(),
            &theme,
            &format!("edge-menu-customize-{shade}"),
        );
    }
    edge_send(&mut app, EdgeEdit::Picker(true));
    // Taller than the window: its top, with the widget and action tiles.
    native.operate(
        app.settings_view(),
        Box::new(navigation::Reveal::target("edge-picker")),
    );
    for (theme, shade) in [
        (cosmic::Theme::dark(), "dark"),
        (cosmic::Theme::light(), "light"),
    ] {
        native.capture(
            app.settings_view(),
            &theme,
            &format!("edge-menu-add-picker-{shade}"),
        );
    }
    assert!(native.duplicate_ids(app.settings_view()).is_empty());
}

/// Style, look, handle and behavior: each saves at once (sliders when let
/// go), Undo takes the last one back and names it, the page shows the three
/// groups and the style tiles, and the preview follows every change.
#[test]
fn edge_menu_style_handle_and_behavior_save_undo_and_preview() {
    use crate::config::edge_menu::{Close, Displays, Frame, HandleColor, IconSize, Speed, Style};
    let name = "edge_menu_style_handle_and_behavior_save_undo_and_preview";
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
    let _root = isolated_root();
    let mut app = edge_page();
    let size = Size::new(1280., 800.);
    send_settings(&mut app, Message::Resized(size));
    let mut native = crate::native_test::Native::new(size);
    let text = native.all_text(app.settings_view());
    for title in [
        "Appearance",
        "Style",
        "Icon size",
        "Section titles",
        "Background opacity",
        "Corner roundness",
        "Handle",
        "Visible handle",
        "Behavior",
        "Close after use",
        "Animation",
        "Displays",
    ] {
        assert!(text.iter().any(|t| t == title), "{title}: {text:?}");
    }
    for id in ["edge-style-list", "edge-style-icons", "edge-style-grid"] {
        assert!(native.try_bounds(app.settings_view(), id).is_some(), "{id}");
    }
    let list_width = app.samples.edge_menu_view().1[0];

    edge_send(&mut app, EdgeEdit::Style(Style::Icons));
    assert_eq!(Config::load().edge_menu.style, Style::Icons);
    let icons_width = app.samples.edge_menu_view().1[0];
    assert!(
        icons_width < list_width / 2.,
        "{icons_width} vs {list_width}"
    );
    // The grid's own rows only show for the grid.
    assert!(
        !native
            .all_text(app.settings_view())
            .iter()
            .any(|t| t == "Columns")
    );
    edge_send(&mut app, EdgeEdit::Style(Style::Grid));
    let text = native.all_text(app.settings_view());
    assert!(text.iter().any(|t| t == "Columns"), "{text:?}");
    assert!(text.iter().any(|t| t == "Labels under icons"), "{text:?}");
    edge_send(&mut app, EdgeEdit::Columns(3));
    let three = app.samples.edge_menu_view().1[0];
    edge_send(&mut app, EdgeEdit::Columns(2));
    let two = app.samples.edge_menu_view().1[0];
    assert!(three > two, "a column more is wider: {three} vs {two}");
    edge_send(&mut app, EdgeEdit::Columns(3));
    edge_send(&mut app, EdgeEdit::Labels(false));
    edge_send(&mut app, EdgeEdit::IconSize(IconSize::Large));
    edge_send(&mut app, EdgeEdit::SectionTitles(false));
    edge_send(&mut app, EdgeEdit::Frame(Frame::Widget));
    edge_send(&mut app, EdgeEdit::Draft(edge_menu::Field::Opacity, 70));
    assert_eq!(Config::load().edge_menu.opacity, 100, "saved when let go");
    edge_send(&mut app, EdgeEdit::Commit);
    edge_send(&mut app, EdgeEdit::Draft(edge_menu::Field::Roundness, 8));
    edge_send(&mut app, EdgeEdit::Commit);
    // The handle's own rows show once it is visible.
    assert!(
        !native
            .all_text(app.settings_view())
            .iter()
            .any(|t| t == "Handle thickness")
    );
    edge_send(&mut app, EdgeEdit::Handle(true));
    let text = native.all_text(app.settings_view());
    for title in ["Handle thickness", "Handle opacity", "Handle color"] {
        assert!(text.iter().any(|t| t == title), "{title}: {text:?}");
    }
    edge_send(&mut app, EdgeEdit::Draft(edge_menu::Field::HandleSize, 8));
    edge_send(&mut app, EdgeEdit::Commit);
    edge_send(
        &mut app,
        EdgeEdit::Draft(edge_menu::Field::HandleOpacity, 90),
    );
    edge_send(&mut app, EdgeEdit::Commit);
    edge_send(&mut app, EdgeEdit::HandleColor(HandleColor::Neutral));
    edge_send(&mut app, EdgeEdit::Close(Close::Always));
    edge_send(&mut app, EdgeEdit::Speed(Speed::Fast));
    edge_send(&mut app, EdgeEdit::Displays(Displays::Chosen));
    edge_send(&mut app, EdgeEdit::Output("DP-1".into(), true));
    edge_send(&mut app, EdgeEdit::Output("HDMI-A-1".into(), true));
    edge_send(&mut app, EdgeEdit::Output("DP-1".into(), false));

    let saved = Config::load().edge_menu;
    assert_eq!(saved.style, Style::Grid);
    assert_eq!(saved.columns, 3);
    assert!(!saved.labels);
    assert_eq!(saved.icon_size, IconSize::Large);
    assert!(!saved.section_titles);
    assert_eq!(saved.frame, Frame::Widget);
    assert_eq!((saved.opacity, saved.roundness), (70, 8));
    assert!(saved.handle);
    assert_eq!((saved.handle_size, saved.handle_opacity), (8, 90));
    assert_eq!(saved.handle_color, HandleColor::Neutral);
    assert_eq!(saved.close, Close::Always);
    assert_eq!(saved.speed, Speed::Fast);
    assert_eq!(saved.displays, Displays::Chosen);
    assert_eq!(saved.outputs, ["HDMI-A-1"]);
    // The preview draws the handle as it is set.
    assert!(app.samples.edge_menu_settings().handle);
    assert_eq!(app.samples.edge_menu_settings().handle_size, 8);

    // Undo names and takes back the last change, then the one before.
    assert_eq!(
        crate::desktop::store::widget::label("/edge_menu/style"),
        "Edge menu style"
    );
    assert_eq!(
        crate::desktop::store::widget::label("/edge_menu/handle_size"),
        "Edge handle thickness"
    );
    send_settings(&mut app, Message::UndoPreferences);
    assert_eq!(Config::load().edge_menu.outputs, ["DP-1", "HDMI-A-1"]);
    edge_send(&mut app, EdgeEdit::Style(Style::List));
    send_settings(&mut app, Message::UndoPreferences);
    assert_eq!(Config::load().edge_menu.style, Style::Grid);
    assert_eq!(app.samples.edge_menu_settings().style, Style::Grid);
    assert!(native.duplicate_ids(app.settings_view()).is_empty());
}

/// The page's Appearance, Handle and Behavior groups with the style tiles,
/// wide and narrow, in both themes, with the Icons style and a visible
/// handle in the preview.
#[test]
fn edge_menu_groups_and_style_picker_captures() {
    use crate::config::edge_menu::Style;
    let name = "edge_menu_groups_and_style_picker_captures";
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
    let _root = isolated_root();
    let mut app = edge_page();
    edge_send(&mut app, EdgeEdit::Style(Style::Icons));
    edge_send(&mut app, EdgeEdit::Handle(true));
    edge_send(&mut app, EdgeEdit::Draft(edge_menu::Field::HandleSize, 6));
    edge_send(&mut app, EdgeEdit::Commit);
    for (size, wide) in [
        (Size::new(1280., 800.), "1280"),
        (Size::new(820., 800.), "820"),
    ] {
        send_settings(&mut app, Message::Resized(size));
        let mut native = crate::native_test::Native::new(size);
        assert!(native.duplicate_ids(app.settings_view()).is_empty());
        for (target, part) in [
            ("edge-menu-appearance", "appearance"),
            ("edge-menu-strip", "handle"),
            ("edge-menu-behavior", "behavior"),
        ] {
            native.operate(
                app.settings_view(),
                Box::new(navigation::Reveal::target(target)),
            );
            for (theme, shade) in [
                (cosmic::Theme::dark(), "dark"),
                (cosmic::Theme::light(), "light"),
            ] {
                native.capture(
                    app.settings_view(),
                    &theme,
                    &format!("edge-menu-settings-{part}-{wide}-{shade}"),
                );
            }
        }
    }
}
