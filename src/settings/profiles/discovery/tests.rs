use super::*;
use crate::desktop::store::tests::Fixture;
#[test]
fn fb19_remix_saves_an_independent_copy_and_leaves_original_and_desktop_intact() {
    let f = Fixture::new("fb19-remix");
    let config = f.0.join("config.json");
    fs::write(&config, b"{\"theme\":\"teal\"}").unwrap();
    let store = f.store().with_app_config(config.clone());
    let mut editor = ProfilesEditor::with_store(store);
    editor.update(Message::Select(Selection::Preset("tidal")));
    let before = fs::read(&config).unwrap();
    let original = editor.profile().unwrap().clone();
    editor.update(Message::Name(Some(NameAction::Duplicate)));
    editor.update(Message::NameInput("Original".into()));
    editor.update(Message::NameSubmit);
    let Selection::Saved(id) = editor.selection().clone() else {
        panic!("saved");
    };
    let original_bytes = profiles::export_local(editor.profile().unwrap()).unwrap();
    editor.update(Message::RemixSelected);
    assert!(editor.mix.is_some());
    assert!(!editor.mix.as_ref().unwrap().choosing);
    assert_eq!(
        editor
            .profile()
            .unwrap()
            .package
            .as_ref()
            .unwrap()
            .ancestors
            .len(),
        1
    );
    editor.update(Message::MixPick(
        profiles::mix::Component::Style,
        Selection::Preset("paper-studio"),
    ));
    editor.update(Message::Name(Some(NameAction::SaveMix)));
    editor.update(Message::NameInput("Mine".into()));
    editor.update(Message::NameSubmit);
    assert_ne!(editor.selection(), &Selection::Saved(id.clone()));
    let library = ProfileLibrary::new(editor.store().unwrap());
    assert_eq!(
        profiles::export_local(&library.load(&id).unwrap()).unwrap(),
        original_bytes
    );
    assert_eq!(fs::read(&config).unwrap(), before);
    assert_eq!(original.package.as_ref().unwrap().credits.author, "Halcyon");
    assert_eq!(
        editor
            .profile()
            .unwrap()
            .package
            .as_ref()
            .unwrap()
            .ancestors
            .len(),
        2
    );
    assert_eq!(
        editor.discovery_sources().len(),
        2,
        "the remix itself is not an upstream source"
    );
}
#[test]
fn fb19_revision_navigation_distinguishes_exact_sources_from_new_curated_updates() {
    let f = Fixture::new("fb19-revision");
    let store = f.store();
    let mut editor = ProfilesEditor::with_store(store);
    editor.update(Message::Select(Selection::Preset("tidal")));
    let mut copy = editor.profile().unwrap().clone();
    copy.name = "My older tidal".into();
    copy.package.as_mut().unwrap().identity.revision = "0".repeat(64);
    let id = ProfileLibrary::new(editor.store().unwrap())
        .create(&copy)
        .unwrap();
    editor.refresh();
    editor.update(Message::Select(Selection::Saved(id.clone())));
    let links = editor.discovery_sources();
    assert_eq!(links.len(), 1);
    assert!(links[0].exact.is_none());
    assert_eq!(links[0].latest, Some(Selection::Preset("tidal")));
    let bytes = profiles::export_local(editor.profile().unwrap()).unwrap();
    editor.update(Message::Select(links[0].latest.clone().unwrap()));
    assert_eq!(
        profiles::export_local(
            &ProfileLibrary::new(editor.store().unwrap())
                .load(&id)
                .unwrap()
        )
        .unwrap(),
        bytes
    );
    editor.update(Message::RemixSelected);
    assert!(editor.discovery_sources()[0].exact.is_some());
}
#[test]
fn fb19_native_discovery_and_keyboard_remix_fit_compact_and_wide() {
    if crate::displays::test_support::isolated(
        "settings::profiles::discovery::tests::fb19_native_discovery_and_keyboard_remix_fit_compact_and_wide",
    ) {
        return;
    }
    use crate::settings::{Page, Settings};
    use cosmic::{
        Application,
        app::Core,
        iced::{Event, Point, Size, keyboard},
    };
    let path = Config::path().unwrap();
    fs::write(&path, b"{\"onboarded\":true,\"setup_dismissed\":true}").unwrap();
    let before = fs::read(&path).unwrap();
    let (mut app, _) = Settings::init(Core::default(), Page::DesktopProfiles);
    for (w, h) in [(820., 600.), (1280., 800.)] {
        app.profiles.update(Message::Discard);
        app.profiles.update(Message::DiscoveryQuery("paper".into()));
        crate::settings::tests::send_settings(
            &mut app,
            crate::settings::Message::Resized(Size::new(w, h)),
        );
        app.open_sections.remove("setup-library-actions");
        let mut native = crate::native_test::Native::new(Size::new(w, h));
        let search = native.bounds(app.settings_view(), "discovery-search");
        assert!(search.y + search.height <= h);
        native.capture(
            app.settings_view(),
            &cosmic::Theme::dark(),
            &format!("fb19-discover-{w}"),
        );
        app.profiles
            .update(Message::Select(Selection::Preset("paper-studio")));
        assert!(
            native
                .try_bounds(app.settings_view(), "setup-remix")
                .is_none(),
            "Closed Save & export must not expose hidden actions to focus"
        );
        crate::settings::tests::native_reveal(&app, &mut native, "setup-library-actions");
        native.operate(
            app.settings_view(),
            Box::new(cosmic::iced::advanced::widget::operation::focusable::focus(
                cosmic::widget::Id::new("setup-library-actions"),
            )),
        );
        let key = keyboard::Key::Named(keyboard::key::Named::Enter);
        let (messages, _) = native.event(
            app.settings_view(),
            Event::Keyboard(keyboard::Event::KeyPressed {
                key: key.clone(),
                modified_key: key,
                physical_key: keyboard::key::Physical::Unidentified(
                    keyboard::key::NativeCode::Unidentified,
                ),
                location: keyboard::Location::Standard,
                modifiers: keyboard::Modifiers::empty(),
                text: None,
                repeat: false,
            }),
            Point::new(1., 1.),
        );
        assert!(matches!(
            messages.as_slice(),
            [crate::settings::Message::ToggleSection(
                "setup-library-actions"
            )]
        ));
        for message in messages {
            crate::settings::tests::send_settings(&mut app, message);
        }
        assert!(app.open_sections.contains("setup-library-actions"));
        crate::settings::tests::native_reveal(&app, &mut native, "setup-remix");
        let b = native.bounds(app.settings_view(), "setup-remix");
        assert!(b.y >= 0. && b.y + b.height <= h);
        native.operate(
            app.settings_view(),
            Box::new(cosmic::iced::advanced::widget::operation::focusable::focus(
                cosmic::widget::Id::new("setup-remix"),
            )),
        );
        let key = keyboard::Key::Named(keyboard::key::Named::Enter);
        let (messages, _) = native.event(
            app.settings_view(),
            Event::Keyboard(keyboard::Event::KeyPressed {
                key: key.clone(),
                modified_key: key,
                physical_key: keyboard::key::Physical::Unidentified(
                    keyboard::key::NativeCode::Unidentified,
                ),
                location: keyboard::Location::Standard,
                modifiers: keyboard::Modifiers::empty(),
                text: None,
                repeat: false,
            }),
            Point::new(1., 1.),
        );
        assert!(matches!(
            messages.as_slice(),
            [crate::settings::Message::Profiles(Message::RemixSelected)]
        ));
        crate::settings::tests::send_settings(&mut app, messages.into_iter().next().unwrap());
        assert!(app.profiles.mix.is_some());
        native.capture(
            app.settings_view(),
            &cosmic::Theme::dark(),
            &format!("fb19-remix-{w}"),
        );
        app.native_busy = true;
        let _ = app.update(crate::settings::Message::Profiles(Message::RemixSelected));
        assert!(app.native_deferred.is_empty());
        app.native_busy = false;
    }
    assert_eq!(fs::read(&path).unwrap(), before);
}
