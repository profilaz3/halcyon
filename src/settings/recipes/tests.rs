use super::*;
#[test]
fn fb17_editor_reorder_primary_protection_and_copy_identity() {
    let mut editor = Editor::default();
    let id = editor.draft.id.clone();
    editor.update(Message::Remove(0));
    assert!(editor.draft.parts.contains(&Part::Face));
    editor.update(Message::Add(Part::Caption));
    editor.update(Message::Move(2, true));
    assert_eq!(editor.draft.parts[1], Part::Caption);
    editor.update(Message::Arrangement(true, Arrangement::Stack));
    assert_eq!(editor.draft.wide, Arrangement::Stack);
    editor.update(Message::Duplicate);
    assert_ne!(editor.draft.id, id);
    assert!(editor.expected.is_none());
    editor.draft.validate().unwrap();
}
#[test]
fn fb17_native_builder_save_apply_reset_undo_and_library_delete_are_independent() {
    if crate::displays::test_support::isolated(
        "settings::recipes::tests::fb17_native_builder_save_apply_reset_undo_and_library_delete_are_independent",
    ) {
        return;
    }
    use crate::settings::tests::send_settings;
    std::fs::write(
        Config::path().unwrap(),
        br#"{"onboarded":true,"setup_dismissed":true,"theme":"teal"}"#,
    )
    .unwrap();
    let store = crate::desktop::actions::history_store().unwrap();
    store.migrate_design().unwrap();
    let frozen = std::fs::read(Config::path().unwrap()).unwrap();
    let (mut app, _) = Settings::init(Core::default(), Page::Builder);
    let send = |app: &mut Settings, m| send_settings(app, super::super::Message::Recipe(m));
    send(&mut app, Message::Name("Morning paper".into()));
    send(&mut app, Message::Add(Part::Caption));
    send(&mut app, Message::Save);
    assert!(!app.recipe_editor.error, "{:?}", app.recipe_editor.notice);
    let saved = app.recipe_editor.expected.clone().unwrap();
    send(&mut app, Message::Apply);
    assert!(!app.recipe_editor.error, "{:?}", app.recipe_editor.notice);
    assert_eq!(app.config.recipes.clock.as_ref(), Some(&saved));
    assert_eq!(std::fs::read(Config::path().unwrap()).unwrap(), frozen);
    send(&mut app, Message::Delete);
    send(&mut app, Message::ConfirmDelete);
    assert!(app.recipe_editor.library.entries.is_empty());
    assert_eq!(
        Config::try_load().unwrap().recipes.clock,
        Some(saved.clone())
    );
    send(&mut app, Message::UseBuiltin);
    assert!(app.config.recipes.clock.is_none());
    send_settings(&mut app, super::super::Message::UndoDesktop);
    assert_eq!(app.config.recipes.clock, Some(saved));
    for (w, h) in [(820., 600.), (1280., 800.)] {
        send_settings(&mut app, super::super::Message::Resized(Size::new(w, h)));
        let mut native = crate::native_test::Native::new(Size::new(w, h));
        let bounds = native.bounds(app.settings_view(), "recipe-apply");
        assert!(bounds.y >= 0. && bounds.y + bounds.height <= h);
        native.capture(
            app.settings_view(),
            &cosmic::Theme::dark(),
            &format!("fb17-builder-{w}"),
        );
    }
}
#[test]
fn fb17_recipe_apply_requires_migration_and_stale_baseline_is_refused() {
    if crate::displays::test_support::isolated(
        "settings::recipes::tests::fb17_recipe_apply_requires_migration_and_stale_baseline_is_refused",
    ) {
        return;
    }
    std::fs::write(Config::path().unwrap(), br#"{"onboarded":true}"#).unwrap();
    let mut editor = Editor::default();
    assert!(editor.work(Message::Apply, None).is_err());
    let store = crate::desktop::actions::history_store().unwrap();
    store.migrate_design().unwrap();
    editor.work(Message::Apply, None).unwrap();
    let saved = Config::try_load().unwrap().recipes.clock.unwrap();
    editor.draft.name = "New name".into();
    editor.draft.seal();
    assert!(editor.work(Message::Apply, None).is_err());
    editor
        .work(Message::Apply, Some(serde_json::to_value(saved).unwrap()))
        .unwrap();
    assert_eq!(
        Config::try_load().unwrap().recipes.clock.unwrap().name,
        "New name"
    );
}
#[test]
fn fb17_capture_preserves_legacy_versions_but_keeps_personal_and_reset_intent() {
    if crate::displays::test_support::isolated(
        "settings::recipes::tests::fb17_capture_preserves_legacy_versions_but_keeps_personal_and_reset_intent",
    ) {
        return;
    }
    std::fs::write(
        Config::path().unwrap(),
        br#"{"onboarded":true,"theme":"teal"}"#,
    )
    .unwrap();
    let store = crate::desktop::actions::history_store().unwrap();
    store.migrate_design().unwrap();
    let capture = |name| {
        crate::desktop::profiles::capture(&store, name)
            .unwrap()
            .profile
    };
    assert!(capture("Legacy").written_version() < 13);
    let mut editor = Editor::default();
    editor.work(Message::Apply, None).unwrap();
    let applied = capture("Personal");
    assert_eq!(applied.written_version(), 13);
    assert_eq!(
        applied.widget_style.unwrap()["recipes"]["clock"]["id"],
        editor.draft.id
    );
    let destination = Config::path().unwrap().with_file_name("legacy-export.json");
    assert!(crate::design::commands::export(&Config::path().unwrap(), &destination, true).is_err());
    assert!(!destination.exists());
    editor
        .work(
            Message::UseBuiltin,
            Some(serde_json::to_value(&editor.draft).unwrap()),
        )
        .unwrap();
    let reset = capture("Reset");
    assert_eq!(reset.written_version(), 13);
    assert!(reset.widget_style.unwrap()["recipes"]["clock"].is_null());
    crate::design::commands::export(&Config::path().unwrap(), &destination, true).unwrap();
}

#[test]
fn calendar_recipe_native_builder_apply_undo_and_preview_preserve_accounts() {
    if crate::displays::test_support::isolated(
        "settings::recipes::tests::calendar_recipe_native_builder_apply_undo_and_preview_preserve_accounts",
    ) {
        return;
    }
    use crate::settings::tests::send_settings;
    std::fs::write(Config::path().unwrap(),br#"{"onboarded":true,"setup_dismissed":true,"calendar_private":true,"theme":"teal","glance":{"enabled":false}}"#).unwrap();
    let store = crate::desktop::actions::history_store().unwrap();
    store.migrate_design().unwrap();
    let frozen = std::fs::read(Config::path().unwrap()).unwrap();
    let (mut app, init) = Settings::init(Core::default(), Page::Builder);
    drop(init);
    send_settings(
        &mut app,
        super::super::Message::Recipe(Message::New(Kind::Calendar)),
    );
    for (w, h) in [(820., 600.), (1280., 800.)] {
        app.window = cosmic::iced::Size::new(w, h);
        let mut native = crate::native_test::Native::new(app.window);
        native.capture(
            app.settings_view(),
            &cosmic::Theme::dark(),
            &format!("calendar-builder-{w}"),
        );
        send_settings(
            &mut app,
            super::super::Message::Recipe(Message::Missing(true)),
        );
        native.capture(
            app.settings_view(),
            &cosmic::Theme::light(),
            &format!("calendar-builder-missing-{w}"),
        );
        send_settings(
            &mut app,
            super::super::Message::Recipe(Message::Missing(false)),
        );
    }
    assert_eq!(std::fs::read(Config::path().unwrap()).unwrap(), frozen);
    send_settings(&mut app, super::super::Message::Recipe(Message::Apply));
    let applied = Config::try_load().unwrap();
    assert!(applied.recipes.calendar.is_some());
    assert!(applied.calendar_private);
    let change = store.latest_change().unwrap().unwrap();
    store.undo_latest(&change.id).unwrap();
    let restored = Config::try_load().unwrap();
    assert!(restored.recipes.calendar.is_none());
    assert!(restored.calendar_private);
    assert_eq!(std::fs::read(Config::path().unwrap()).unwrap(), frozen);
}

#[test]
fn system_recipe_native_builder_apply_edit_reset_and_undo() {
    if crate::displays::test_support::isolated(
        "settings::recipes::tests::system_recipe_native_builder_apply_edit_reset_and_undo",
    ) {
        return;
    }
    use crate::settings::tests::send_settings;
    std::fs::write(Config::path().unwrap(),br#"{"onboarded":true,"setup_dismissed":true,"theme":"teal","devbox":{"memory":false,"services":true}}"#).unwrap();
    let store = crate::desktop::actions::history_store().unwrap();
    store.migrate_design().unwrap();
    let frozen = std::fs::read(Config::path().unwrap()).unwrap();
    let (mut app, init) = Settings::init(Core::default(), Page::Builder);
    drop(init);
    send_settings(
        &mut app,
        super::super::Message::Recipe(Message::New(Kind::System)),
    );
    for (w, h) in [(820., 600.), (1280., 800.)] {
        app.window = cosmic::iced::Size::new(w, h);
        let mut native = crate::native_test::Native::new(app.window);
        let apply = native.bounds(app.settings_view(), "recipe-apply");
        assert!(apply.y >= 0. && apply.y + apply.height <= h);
        native.capture(
            app.settings_view(),
            &cosmic::Theme::dark(),
            &format!("system-builder-{w}"),
        );
        send_settings(
            &mut app,
            super::super::Message::Recipe(Message::Missing(true)),
        );
        native.capture(
            app.settings_view(),
            &cosmic::Theme::light(),
            &format!("system-builder-missing-{w}"),
        );
        send_settings(
            &mut app,
            super::super::Message::Recipe(Message::Missing(false)),
        );
    }
    assert_eq!(std::fs::read(Config::path().unwrap()).unwrap(), frozen);
    send_settings(&mut app, super::super::Message::Recipe(Message::Apply));
    let applied = Config::try_load().unwrap();
    assert!(applied.recipes.devbox.is_some() && !applied.devbox.memory && applied.devbox.services);
    send_settings(&mut app, super::super::Message::Page(Page::Devbox));
    app.window = cosmic::iced::Size::new(820., 600.);
    let mut native = crate::native_test::Native::new(app.window);
    native.capture(
        app.settings_view(),
        &cosmic::Theme::dark(),
        "system-recipe-settings-820",
    );
    send_settings(&mut app, super::super::Message::OpenRecipe(Kind::System));
    assert_eq!(app.page, Page::Builder);
    assert_eq!(app.recipe_editor.draft, applied.recipes.devbox.unwrap());
    send_settings(&mut app, super::super::Message::Recipe(Message::UseBuiltin));
    assert!(app.config.recipes.devbox.is_none());
    send_settings(&mut app, super::super::Message::UndoDesktop);
    assert!(app.config.recipes.devbox.is_some());
    assert_eq!(std::fs::read(Config::path().unwrap()).unwrap(), frozen);
}

#[test]
fn system_recipe_editor_keeps_one_reading_and_respects_part_limit() {
    let mut editor = Editor::default();
    editor.update(Message::New(Kind::System));
    editor.update(Message::Remove(0));
    assert_eq!(editor.draft.primary(), Part::Memory);
    editor.update(Message::Remove(1));
    editor.update(Message::Remove(0));
    assert_eq!(editor.draft.parts, vec![Part::Memory]);
    for part in Kind::System.parts() {
        editor.update(Message::Add(*part));
    }
    assert_eq!(editor.draft.parts.len(), 8);
    editor.draft.validate().unwrap();
}

#[test]
fn tool_recipe_library_apply_undo_preserves_live_session_and_preferences() {
    if crate::displays::test_support::isolated(
        "settings::recipes::tests::tool_recipe_library_apply_undo_preserves_live_session_and_preferences",
    ) {
        return;
    }
    use crate::settings::tests::send_settings;
    let root = crate::test_env::assert_isolated();
    crate::test_env::native_worker(&root, true);
    // Read-only system icon assets make the native captures representative;
    // preferences, buses and runtime files remain inside the private fixture.
    if std::env::var_os("DESKTOP_CARDS_LAYOUT_EVIDENCE").is_some()
        && std::path::Path::new("/usr/share/icons").is_dir()
    {
        std::os::unix::fs::symlink("/usr/share/icons", root.join("data/icons")).unwrap();
    }
    std::fs::write(Config::path().unwrap(), br#"{"onboarded":true,"setup_dismissed":true,"music":{"player":"spotify","show_artwork":false},"focus":{"focus_minutes":50}}"#).unwrap();
    let store = crate::desktop::actions::history_store().unwrap();
    store.migrate_design().unwrap();
    let mut session = crate::tools::focus::Session::default();
    session.toggle(
        crate::tools::focus::now_ms(),
        &Config::try_load().unwrap().focus,
        &crate::tools::focus::today(),
    );
    session.save().unwrap();
    let session_bytes = std::fs::read(crate::tools::focus::Session::path().unwrap()).unwrap();
    let preferences = std::fs::read(Config::path().unwrap()).unwrap();
    let (mut app, _) = Settings::init(Core::default(), Page::Builder);
    for kind in [Kind::Music, Kind::Focus] {
        let send =
            |app: &mut Settings, edit| send_settings(app, super::super::Message::Recipe(edit));
        send(&mut app, Message::New(kind));
        send(&mut app, Message::Template(1));
        send(&mut app, Message::Finish(Some(model::Finish::Paper)));
        send(&mut app, Message::Save);
        assert!(!app.recipe_editor.error, "{:?}", app.recipe_editor.notice);
        send(&mut app, Message::Apply);
        assert!(!app.recipe_editor.error, "{:?}", app.recipe_editor.notice);
        assert_eq!(
            app.config.recipes.get(kind).unwrap().name,
            if kind == Kind::Music {
                "Player"
            } else {
                "Type"
            }
        );
        let exported = crate::desktop::profiles::capture(&store, "Tool design")
            .unwrap()
            .profile;
        assert_eq!(exported.written_version(), 17);
        let mut downgrade = exported.clone();
        downgrade.version = 16;
        assert!(downgrade.validate().is_err());
        for (width, height) in [(820., 600.), (1280., 800.)] {
            send_settings(
                &mut app,
                super::super::Message::Resized(Size::new(width, height)),
            );
            send_settings(
                &mut app,
                super::super::Message::Page(if kind == Kind::Music {
                    Page::Music
                } else {
                    Page::Focus
                }),
            );
            let mut native = crate::native_test::Native::new(Size::new(width, height));
            native.capture(
                app.settings_view(),
                &cosmic::Theme::dark(),
                &format!("tool-settings-{kind:?}-{width}"),
            );
            send_settings(&mut app, super::super::Message::Page(Page::Builder));
        }
        send_settings(&mut app, super::super::Message::UndoDesktop);
        assert!(app.config.recipes.get(kind).is_none());
        assert_eq!(
            std::fs::read(crate::tools::focus::Session::path().unwrap()).unwrap(),
            session_bytes
        );
        assert_eq!(std::fs::read(Config::path().unwrap()).unwrap(), preferences);
        for (width, height) in [(820., 600.), (1280., 800.)] {
            send_settings(
                &mut app,
                super::super::Message::Resized(Size::new(width, height)),
            );
            let mut native = crate::native_test::Native::new(Size::new(width, height));
            let bounds = native.bounds(app.settings_view(), "recipe-apply");
            assert!(bounds.y + bounds.height <= height);
            native.capture(
                app.settings_view(),
                &cosmic::Theme::dark(),
                &format!("tool-builder-{kind:?}-{width}"),
            );
            native.capture(
                app.settings_view(),
                &cosmic::Theme::light(),
                &format!("tool-builder-{kind:?}-{width}-light"),
            );
        }
    }
}

#[test]
fn tool_recipe_loading_another_kind_resets_the_sample_state() {
    let mut editor = Editor::default();
    editor.update(Message::New(Kind::Music));
    editor.update(Message::PreviewState(
        crate::tools::cards::recipe::PreviewState::NoArtwork,
    ));
    let focus = Recipe::starter(Kind::Focus);
    editor
        .library
        .entries
        .insert(focus.id.clone(), focus.clone());
    editor.update(Message::Load(focus.id.clone()));
    assert_eq!(editor.draft, focus);
    assert_eq!(
        editor.preview_state,
        crate::tools::cards::recipe::PreviewState::Active
    );
}

#[test]
fn visualizer_settings_save_reload_undo_and_native_controls() {
    if crate::displays::test_support::isolated(
        "settings::recipes::tests::visualizer_settings_save_reload_undo_and_native_controls",
    ) {
        return;
    }
    use crate::settings::tests::send_settings;
    use crate::tools::visualizer::{Circle, Preferences};
    let root = crate::test_env::assert_isolated();
    crate::test_env::native_worker(&root, true);
    if std::path::Path::new("/usr/share/icons").is_dir() {
        let _ = std::os::unix::fs::symlink("/usr/share/icons", root.join("data/icons"));
    }
    std::fs::write(
        Config::path().unwrap(),
        br#"{"onboarded":true,"setup_dismissed":true,"music":{"enabled":true,"player":"spotify"}}"#,
    )
    .unwrap();
    let store = crate::desktop::actions::history_store().unwrap();
    store.migrate_design().unwrap();
    let (mut app, _) = Settings::init(Core::default(), Page::Music);
    let prefs = Preferences {
        spectrum: true,
        opacity: 37,
        circle: Circle::Dots,
        ..Default::default()
    };
    send_settings(
        &mut app,
        super::super::Message::Tools(crate::settings::tools::Message::Visualizer(prefs.clone())),
    );
    assert!(
        !Config::try_load().unwrap().music.visualizer.spectrum,
        "slider gesture is not committed early"
    );
    send_settings(
        &mut app,
        super::super::Message::Tools(crate::settings::tools::Message::VisualizerSave),
    );
    assert!(!app.error, "{}", app.status);
    assert_eq!(Config::try_load().unwrap().music.visualizer, prefs);
    assert_eq!(
        Config::try_load().unwrap().music.player.as_deref(),
        Some("spotify")
    );
    for width in [820., 1280.] {
        app.window.width = width;
        let mut native = crate::native_test::Native::new(cosmic::iced::Size::new(width, 800.));
        native.capture(
            app.settings_view(),
            &cosmic::Theme::dark(),
            &format!("visualizer-settings-{width}"),
        );
    }
    let exported = crate::desktop::profiles::capture(&store, "Audio design")
        .unwrap()
        .profile;
    assert_eq!(exported.written_version(), 18);
    let mut old = exported.clone();
    old.version = 17;
    assert!(old.validate().is_err());
    let change = store.latest_change().unwrap().unwrap();
    let undone = store.undo_latest(&change.id);
    assert!(undone.is_ok(), "{undone:?}");
    assert_eq!(
        Config::try_load().unwrap().music.visualizer,
        Preferences::default()
    );
}
