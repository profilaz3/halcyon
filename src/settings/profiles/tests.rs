//! Profiles page behavior. Every test writes only below its own temporary
//! fixture root: COSMIC trees, the library, Undo and the widget config are
//! all inside it.
use super::*;
use crate::desktop::store::tests::Fixture;
use std::collections::BTreeMap;

const DOCK: &str = "com.system76.CosmicPanel.Dock";
const PREFERENCES: &str = r#"{"theme":"teal","location":{"name":"SENTINEL","latitude":1.0,"longitude":2.0},"appearance":{"opacity":92,"match_wallpaper":true},"future_field":7}"#;

struct Desk {
    fixture: Fixture,
    config: PathBuf,
}

impl Desk {
    fn new(name: &str) -> Self {
        let fixture = Fixture::new(&format!("profiles-page-{name}"));
        fixture.system(
            "com.system76.CosmicPanel",
            "entries",
            "[\"Panel\",\"Dock\"]",
        );
        fixture.system(DOCK, "anchor", "Bottom");
        fixture.system(DOCK, "size", "L");
        fixture.system("com.system76.CosmicPanel.Panel", "anchor", "Top");
        fixture.user("com.system76.CosmicTheme.Mode", "is_dark", "true");
        let config = fixture.0.join("app/config.json");
        fs::create_dir_all(config.parent().unwrap()).unwrap();
        fs::write(&config, PREFERENCES).unwrap();
        Self { fixture, config }
    }
    fn store(&self) -> DesktopStore {
        self.fixture.store().with_app_config(self.config.clone())
    }
    fn editor(&self) -> ProfilesEditor {
        ProfilesEditor::with_store(self.store())
    }
    /// Every file holding desktop settings, the widget config, a journal
    /// or Undo: all files but the library, exports and empty lock files.
    fn desktop(&self) -> BTreeMap<PathBuf, Vec<u8>> {
        let library = self.fixture.0.join("state/profiles");
        let exports = self.fixture.0.join("out");
        self.fixture
            .tree()
            .into_iter()
            .filter(|(path, _)| !path.starts_with(&library) && !path.starts_with(&exports))
            .filter_map(|(path, bytes)| Some((path, bytes?)))
            .filter(|(path, bytes)| {
                !(bytes.is_empty() && path.extension().is_some_and(|ext| ext == "lock"))
            })
            .collect()
    }
    fn widget_config(&self) -> serde_json::Value {
        serde_json::from_slice(&fs::read(&self.config).unwrap()).unwrap()
    }
}

fn name(editor: &mut ProfilesEditor, action: NameAction, text: &str) {
    assert_eq!(
        editor.update(Message::Name(Some(action))),
        Outcome::FocusName
    );
    editor.update(Message::NameInput(text.into()));
    editor.update(Message::NameSubmit);
}

fn saved_names(editor: &ProfilesEditor) -> Vec<(ProfileId, String)> {
    editor
        .library()
        .unwrap()
        .iter()
        .map(|entry| {
            (
                entry.id.clone(),
                entry.profile.as_ref().unwrap().name.clone(),
            )
        })
        .collect()
}

fn export_request(editor: &mut ProfilesEditor) -> (u64, Vec<u8>, Option<PathBuf>) {
    match editor.update(Message::Export) {
        Outcome::Dialog(Request::Export {
            id, bytes, confine, ..
        }) => (id, bytes, confine),
        other => panic!("expected an export dialog, got {other:?}"),
    }
}

fn import_request(editor: &mut ProfilesEditor) -> u64 {
    match editor.update(Message::Import) {
        Outcome::Dialog(Request::Import { id }) => id,
        other => panic!("expected an import dialog, got {other:?}"),
    }
}

#[test]
fn browsing_capturing_importing_and_exporting_never_change_the_desktop() {
    let desk = Desk::new("browse");
    let before = desk.desktop();
    let mut editor = desk.editor();
    assert_eq!(*editor.selection(), Selection::Current);
    assert!(editor.preview().is_some(), "the current desktop is shown");
    for preset in [
        "calm",
        "evening",
        "focus",
        "halcyon",
        "paper",
        "rose",
        "presentation",
    ] {
        editor.update(Message::Select(Selection::Preset(preset)));
        assert!(
            editor.plan().is_some_and(ProfilePlan::can_apply),
            "{preset}"
        );
    }
    editor.update(Message::Include(Domain::Dock, false));
    editor.update(Message::Details(true));
    name(&mut editor, NameAction::Capture, "Desk");
    assert!(matches!(editor.selection(), Selection::Saved(_)));
    assert!(matches!(&editor.notice, Some(Notice::Done(text)) if text.contains("Desk")));
    // A capture of the applied desktop plans to nothing.
    assert!(editor.plan().unwrap().is_empty());

    let (id, bytes, confine) = export_request(&mut editor);
    assert_eq!(confine.as_deref(), Some(desk.fixture.0.as_path()));
    let file = desk.fixture.0.join("out/desk.json");
    fs::create_dir_all(file.parent().unwrap()).unwrap();
    write_export(&file, &bytes, confine.as_deref()).unwrap();
    editor.update(Message::Exported(id, Ok(Some(file.clone()))));
    assert!(matches!(&editor.notice, Some(Notice::Done(text)) if text.contains("desk.json")));
    let exported = fs::read_to_string(&file).unwrap();
    assert!(!exported.contains("SENTINEL") && !exported.contains("future_field"));

    let id = import_request(&mut editor);
    editor.update(Message::Imported(id, read_import(&file).map(Some)));
    assert_eq!(saved_names(&editor).len(), 2, "imported as a new entry");
    assert!(editor.latest().is_none());
    assert_eq!(
        desk.desktop(),
        before,
        "nothing outside the library changed"
    );
}

#[test]
fn apply_writes_only_included_parts_and_its_undo_survives_a_restart() {
    let desk = Desk::new("apply");
    let original = fs::read(&desk.config).unwrap();
    let mut editor = desk.editor();
    editor.update(Message::Select(Selection::Preset("focus")));
    // A starter look sets the whole desktop; leave out all but the widgets.
    for domain in [
        Domain::Dock,
        Domain::Panel,
        Domain::Appearance,
        Domain::Wallpaper,
    ] {
        editor.update(Message::Include(domain, false));
    }
    assert!(!editor.selected_domains().contains(&Domain::Dock));
    assert_eq!(editor.update(Message::Apply), Outcome::Changed);
    assert_eq!(desk.fixture.read(DOCK, "size"), None, "Dock was excluded");
    assert_eq!(
        desk.fixture.read(DOCK, "autohide"),
        None,
        "Dock was excluded"
    );
    assert!(
        !desk
            .fixture
            .0
            .join("config/cosmic/com.system76.CosmicTheme.Dark.Builder")
            .exists(),
        "Appearance was excluded"
    );
    let config = desk.widget_config();
    assert_eq!(config["theme"], "graphite");
    assert_eq!(config["location"]["name"], "SENTINEL");
    assert_eq!(config["future_field"], 7);
    assert_eq!(config["appearance"]["match_wallpaper"], true);
    // The selection stays; the plan now has nothing left to do.
    assert_eq!(*editor.selection(), Selection::Current);
    assert!(editor.plan().is_none());

    // After a restart the profile's Undo is still this page's.
    let mut restarted = desk.editor();
    let latest = restarted.latest().cloned().expect("profile Undo kept");
    assert_eq!(latest.owner, Owner::Profile);
    assert_eq!(latest.label, "Profile “Deep work”");
    assert!(restarted.can_undo() && restarted.elsewhere().is_none());
    assert_eq!(
        change_domains(&latest),
        BTreeSet::from([Domain::WidgetStyle, Domain::WidgetLayout])
    );
    assert_eq!(restarted.update(Message::Undo), Outcome::Changed);
    assert_eq!(fs::read(&desk.config).unwrap(), original);
    assert!(restarted.latest().is_none() && !restarted.can_undo());
}

#[test]
fn another_pages_latest_change_is_named_but_never_undone_here() {
    let desk = Desk::new("owner");
    let mut editor = desk.editor();
    editor.update(Message::Select(Selection::Preset("evening")));
    editor.update(Message::Apply);
    assert!(editor.can_undo());
    // A Dock & Panel apply replaces the single latest-change slot.
    let store = desk.store();
    let mut draft = PanelDraft::load(&store, PanelKind::Dock).unwrap();
    draft.set_anchor(crate::desktop::panels::Anchor::Left);
    store
        .apply_owned(&draft.patch(), None, Owner::DockPanel, "Dock & Panel")
        .unwrap();
    let mut editor = desk.editor();
    assert!(!editor.can_undo());
    assert_eq!(
        editor.elsewhere(),
        Some(&(Owner::DockPanel, "Dock position".to_string()))
    );
    let before = desk.desktop();
    assert_eq!(editor.update(Message::Undo), Outcome::None);
    assert_eq!(desk.desktop(), before);
}

#[test]
fn a_desktop_change_after_the_preview_conflicts_instead_of_reapplying() {
    let desk = Desk::new("conflict");
    let mut editor = desk.editor();
    editor.update(Message::Select(Selection::Preset("evening")));
    // Someone changes the Dock after the preview was made.
    desk.fixture.user(DOCK, "opacity", "0.5");
    let before = desk.desktop();
    assert_eq!(editor.update(Message::Apply), Outcome::None);
    assert!(editor.stale() && !editor.can_apply());
    assert!(
        matches!(&editor.notice, Some(Notice::Error(text)) if text.contains("Dock opacity") && text.contains("Nothing was changed")),
        "{:?}",
        editor.notice
    );
    assert_eq!(desk.desktop(), before, "nothing was written");
    assert_eq!(*editor.selection(), Selection::Preset("evening"));
    // Only an explicit refresh plans again; then Apply uses the new plan.
    editor.update(Message::Replan);
    assert!(!editor.stale() && editor.can_apply());
    assert_eq!(editor.update(Message::Apply), Outcome::Changed);
    let planned = profiles::preset("evening")
        .unwrap()
        .profile
        .dock
        .unwrap()
        .opacity
        .unwrap();
    let written: f32 = desk.fixture.read(DOCK, "opacity").unwrap().parse().unwrap();
    assert!((written - planned).abs() < 0.001, "{written} {planned}");
}

#[test]
fn dialog_results_complete_only_the_request_that_started_them() {
    let desk = Desk::new("dialogs");
    let mut editor = desk.editor();
    editor.update(Message::Select(Selection::Preset("evening")));
    let (id, bytes, confine) = export_request(&mut editor);
    // The selection changes while the chooser is open; a second dialog
    // waits for the first.
    editor.update(Message::Select(Selection::Preset("halcyon")));
    assert!(editor.busy() && !editor.can_apply());
    assert_eq!(editor.update(Message::Export), Outcome::None);
    assert_eq!(editor.update(Message::Import), Outcome::None);
    editor.update(Message::Exported(id + 7, Ok(None)));
    assert!(editor.busy(), "an unknown result is ignored");
    let file = desk.fixture.0.join("out/evening.json");
    fs::create_dir_all(file.parent().unwrap()).unwrap();
    write_export(&file, &bytes, confine.as_deref()).unwrap();
    editor.update(Message::Exported(id, Ok(Some(file.clone()))));
    assert!(!editor.busy());
    assert!(matches!(&editor.notice, Some(Notice::Done(text)) if text.contains("“After hours”")));
    assert_eq!(*editor.selection(), Selection::Preset("halcyon"));
    assert_eq!(
        profiles::import(&fs::read(&file).unwrap()).unwrap().name,
        "After hours",
        "the file holds the profile the export started from"
    );

    // Cancel does nothing; a bad file is reported and stores nothing.
    let id = import_request(&mut editor);
    editor.update(Message::Imported(id, Ok(None)));
    assert!(editor.notice.is_some() && saved_names(&editor).is_empty());
    let id = import_request(&mut editor);
    editor.update(Message::Imported(
        id,
        Ok(Some(b"{\"format\":\"other\"}".to_vec())),
    ));
    assert!(
        matches!(&editor.notice, Some(Notice::Error(text)) if text.starts_with("Not imported"))
    );
    assert!(saved_names(&editor).is_empty());
    assert_eq!(*editor.selection(), Selection::Preset("halcyon"));
    // A late result for a finished request changes nothing.
    editor.update(Message::Imported(id, Ok(Some(fs::read(&file).unwrap()))));
    assert!(saved_names(&editor).is_empty());
    assert!(editor.latest().is_none(), "nothing was applied");
}

#[test]
fn equal_names_get_separate_entries_and_delete_confirms_and_removes_one() {
    let desk = Desk::new("library");
    let mut editor = desk.editor();
    name(&mut editor, NameAction::Capture, "Desk");
    name(&mut editor, NameAction::Capture, "Desk");
    let entries = saved_names(&editor);
    assert_eq!(entries.len(), 2);
    assert_ne!(entries[0].0, entries[1].0);
    let Selection::Saved(second) = editor.selection().clone() else {
        panic!("the new capture is selected");
    };
    // Delete needs its confirmation and removes only the selected entry.
    editor.update(Message::Delete(true));
    assert_eq!(saved_names(&editor).len(), 2);
    editor.update(Message::Delete(false));
    assert!(editor.confirm_delete);
    editor.update(Message::Delete(true));
    let left = saved_names(&editor);
    assert_eq!(left.len(), 1);
    assert_ne!(left[0].0, second);
    assert_eq!(*editor.selection(), Selection::Current);

    // Built-ins are duplicated into the library, never changed.
    editor.update(Message::Select(Selection::Preset("focus")));
    editor.update(Message::Name(Some(NameAction::Duplicate)));
    assert_eq!(editor.name.as_ref().unwrap().text, "Deep work copy");
    editor.update(Message::NameSubmit);
    let Selection::Saved(copy) = editor.selection().clone() else {
        panic!("the copy is selected");
    };
    assert_eq!(editor.profile().unwrap().name, "Deep work copy");
    assert_eq!(profiles::preset("focus").unwrap().profile.name, "Deep work");
    // Rename keeps the ID; long names are allowed up to the limit.
    let long = "A".repeat(80);
    name(&mut editor, NameAction::Rename(copy.clone()), &long);
    assert_eq!(*editor.selection(), Selection::Saved(copy.clone()));
    assert_eq!(editor.profile().unwrap().name, long);
    name(&mut editor, NameAction::Rename(copy), &"B".repeat(81));
    assert!(matches!(editor.notice, Some(Notice::Error(_))));
    assert!(
        editor.name.is_some(),
        "the field stays open to fix the name"
    );
    assert_eq!(editor.profile().unwrap().name, long);
}

#[test]
fn unreadable_entries_are_shown_and_can_be_deleted() {
    let desk = Desk::new("unreadable");
    let mut editor = desk.editor();
    name(&mut editor, NameAction::Capture, "Desk");
    let dir = desk.fixture.0.join("state/profiles");
    fs::write(dir.join("broken.json"), b"not json").unwrap();
    editor.update(Message::Replan);
    let broken = ProfileId::parse("broken").unwrap();
    assert!(
        editor
            .library()
            .unwrap()
            .iter()
            .any(|entry| entry.id == broken && entry.profile.is_err())
    );
    editor.update(Message::Select(Selection::Saved(broken)));
    assert!(editor.load_error().is_some() && editor.plan().is_none());
    editor.update(Message::Delete(false));
    editor.update(Message::Delete(true));
    assert!(!dir.join("broken.json").exists());
    assert_eq!(saved_names(&editor).len(), 1);
}

#[test]
fn imports_are_bounded_and_exports_stay_inside_a_fixture() {
    let desk = Desk::new("files");
    let root = &desk.fixture.0;
    let big = root.join("big.json");
    fs::write(&big, vec![b' '; MAX_PROFILE_BYTES + 1]).unwrap();
    assert!(read_import(&big).unwrap_err().contains("too large"));
    assert!(read_import(root).is_err(), "a folder is not a profile");
    let outside = Fixture::new("profiles-page-outside");
    let target = outside.0.join("escape.json");
    assert!(write_export(&target, b"{}", Some(root)).is_err());
    assert!(!target.exists());
    // Replacing an existing file leaves only the new bytes behind.
    let file = root.join("out.json");
    fs::write(&file, b"old").unwrap();
    write_export(&file, b"new", Some(root)).unwrap();
    assert_eq!(fs::read(&file).unwrap(), b"new");
    let stray = fs::read_dir(root)
        .unwrap()
        .filter_map(Result::ok)
        .any(|entry| entry.file_name().to_string_lossy().ends_with(".tmp"));
    assert!(!stray);
}

#[test]
fn restore_after_an_interrupted_profile_names_what_was_kept() {
    let desk = Desk::new("recover");
    let mut editor = desk.editor();
    editor.update(Message::Select(Selection::Preset("halcyon")));
    let store = desk.store();
    let plan = editor.plan().unwrap().clone();
    store.inject(Some(crate::desktop::store::Fault::InterruptBeforeRecord));
    assert!(profiles::apply(&store, &plan).is_err());
    let mut editor = desk.editor();
    assert!(editor.recovery().is_some());
    assert!(!editor.can_apply() && !editor.can_undo());
    // The card palette was changed again before Restore.
    let mut config = desk.widget_config();
    config["theme"] = "rose".into();
    fs::write(&desk.config, serde_json::to_vec(&config).unwrap()).unwrap();
    editor.update(Message::Select(Selection::Preset("halcyon")));
    assert_eq!(editor.update(Message::Recover), Outcome::Changed);
    assert!(editor.recovery().is_none());
    assert!(
        matches!(&editor.notice, Some(Notice::Done(text)) if text.contains("Card palette")),
        "{:?}",
        editor.notice
    );
    assert!(
        editor
            .notes
            .iter()
            .any(|note| note.contains("Card palette"))
    );
    assert_eq!(desk.widget_config()["theme"], "rose");
}

mod wallpaper_parts {
    //! The Wallpaper and Desktop icons parts on the page: the backdrop,
    //! Apply's asset recheck, Undo's parts and the export notice.
    use super::*;
    use crate::desktop::profiles::{
        BackgroundLook, DesktopIconsLook, DisplaysName, GradientLook, LOCAL_ONLY, Rgb, SourceLook,
        WallpaperLook,
    };
    use crate::desktop::wallpaper::BgColor;

    fn desk(name: &str) -> Desk {
        let desk = Desk::new(name);
        fs::write(desk.fixture.0.join("outputs"), "eDP-1\n").unwrap();
        desk
    }
    fn look(source: SourceLook) -> WallpaperLook {
        WallpaperLook {
            background: Some(BackgroundLook {
                source: Some(source),
                ..Default::default()
            }),
            ..Default::default()
        }
    }
    fn blue() -> SourceLook {
        SourceLook::Color(Rgb::Hex("#336699".into()))
    }
    fn profile(name: &str, wallpaper: WallpaperLook) -> Profile {
        let mut profile = Profile::new(name).unwrap();
        profile.wallpaper = Some(wallpaper);
        profile.desktop_icons = Some(DesktopIconsLook {
            show_trash: Some(true),
            ..Default::default()
        });
        profile
    }
    /// Stores `profile` in the library and selects it.
    fn select_saved(desk: &Desk, editor: &mut ProfilesEditor, profile: &Profile) -> ProfileId {
        let id = ProfileLibrary::new(&desk.store()).create(profile).unwrap();
        editor.update(Message::Replan);
        editor.update(Message::Select(Selection::Saved(id.clone())));
        id
    }
    fn picture(desk: &Desk, relative: &str) -> PathBuf {
        let path = desk.fixture.0.join(relative);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(&path, b"picture").unwrap();
        path
    }

    #[test]
    fn profile_preview_shows_planned_background_only_when_included() {
        let desk = desk("backdrop");
        let mut editor = desk.editor();
        assert_eq!(editor.backdrop(), &Backdrop::Neutral, "the current desktop");
        assert!(editor.take_jobs().is_empty());
        select_saved(&desk, &mut editor, &profile("Blue", look(blue())));
        let Backdrop::Planned(entry) = editor.backdrop().clone() else {
            panic!("{:?}", editor.backdrop());
        };
        assert_eq!(
            entry.source,
            BgSource::Color(BgColor::Single(Rgb::Hex("#336699".into()).srgb().unwrap()))
        );
        // Made off the UI thread: nothing until the job's result arrives.
        assert!(editor.backdrop_image().is_none());
        let jobs = editor.take_jobs();
        assert_eq!(jobs.len(), 1);
        for job in jobs {
            editor.update(job.run());
        }
        assert!(editor.backdrop_image().is_some());

        // Left out: neutral. Included again: the kept picture, no new work.
        editor.update(Message::Include(Domain::Wallpaper, false));
        assert_eq!(editor.backdrop(), &Backdrop::Neutral);
        assert!(editor.backdrop_image().is_none());
        editor.update(Message::Include(Domain::Wallpaper, true));
        assert!(editor.backdrop_image().is_some());
        assert!(editor.take_jobs().is_empty());

        // A wallpaper that can't be applied (a built-in that isn't
        // installed) is never shown as if it were.
        select_saved(
            &desk,
            &mut editor,
            &profile("Amber", look(SourceLook::Bundled("amber".into()))),
        );
        assert!(!editor.plan().unwrap().can_apply());
        assert_eq!(editor.backdrop(), &Backdrop::Neutral);
        assert!(editor.take_jobs().is_empty());

        // A result for a selection that moved on is dropped.
        let gradient = SourceLook::Gradient(GradientLook {
            stops: vec![Rgb::Hex("#102030".into()), Rgb::Hex("#405060".into())],
            angle: 90.,
        });
        select_saved(&desk, &mut editor, &profile("Gradient", look(gradient)));
        let stale = editor.take_jobs();
        select_saved(
            &desk,
            &mut editor,
            &profile("Green", look(SourceLook::Color(Rgb::Hex("#20A040".into())))),
        );
        let current = editor.take_jobs();
        for job in stale {
            editor.update(job.run());
        }
        assert!(editor.backdrop_image().is_none(), "stale result dropped");
        for job in current {
            editor.update(job.run());
        }
        assert!(editor.backdrop_image().is_some());
    }

    #[test]
    fn apply_rechecks_wallpaper_assets_and_leaving_it_out_applies_the_rest() {
        let desk = desk("recheck");
        let teal = picture(&desk, "pictures/Desktop Cards/teal.jpg");
        let mut editor = desk.editor();
        select_saved(
            &desk,
            &mut editor,
            &profile("Teal", look(SourceLook::Bundled("teal".into()))),
        );
        assert!(editor.can_apply(), "{:?}", editor.plan());
        // The picture disappears after the preview was made.
        fs::remove_file(&teal).unwrap();
        let before = desk.desktop();
        assert_eq!(editor.update(Message::Apply), Outcome::None);
        assert!(
            matches!(&editor.notice, Some(Notice::Error(text))
                if text.starts_with("Wallpaper: The built-in Teal wallpaper isn’t installed")
                    && text.ends_with("Nothing was changed.")),
            "{:?}",
            editor.notice
        );
        assert_eq!(desk.desktop(), before, "no part was written");
        let plan = editor.plan().unwrap();
        assert!(!plan.can_apply());
        assert!(
            plan.domains
                .iter()
                .any(|part| part.domain == Domain::Wallpaper && !part.blocked.is_empty()),
            "the part now shows why"
        );
        // The footer never calls a blocked preview "matching".
        assert_eq!(
            editor.blocked_status(),
            Some("Leave out the blocked part to apply the rest")
        );
        // Leave out Wallpaper: the rest applies; Undo covers what changed.
        editor.update(Message::Include(Domain::Wallpaper, false));
        assert!(editor.can_apply());
        assert_eq!(editor.blocked_status(), None);
        assert_eq!(editor.update(Message::Apply), Outcome::Changed);
        // Desktop icons change only after a refresh; Apply says so.
        assert_eq!(
            editor.notes,
            [crate::desktop::desktop_icons::REFRESH_NOTE.to_string()]
        );
        assert!(
            desk.fixture
                .read("com.system76.CosmicFiles", "desktop")
                .unwrap()
                .contains("show_trash: true")
        );
        assert_eq!(
            desk.fixture.read("com.system76.CosmicBackground", "all"),
            None
        );
        assert_eq!(
            editor.undo_domains(),
            BTreeSet::from([Domain::DesktopIcons])
        );
    }

    #[test]
    fn change_domains_maps_background_and_files_namespaces() {
        let desk = desk("domains");
        let before = desk.desktop();
        let mut editor = desk.editor();
        select_saved(&desk, &mut editor, &profile("Blue", look(blue())));
        assert_eq!(
            editor.apply_domains(),
            BTreeSet::from([Domain::Wallpaper, Domain::DesktopIcons])
        );
        assert_eq!(editor.update(Message::Apply), Outcome::Changed);
        let latest = editor.latest().cloned().unwrap();
        assert_eq!(
            change_domains(&latest),
            BTreeSet::from([Domain::Wallpaper, Domain::DesktopIcons])
        );
        assert_eq!(editor.update(Message::Undo), Outcome::Changed);
        let after: BTreeMap<_, _> = desk
            .desktop()
            .into_iter()
            .filter(|(path, _)| path.starts_with(desk.fixture.0.join("config")))
            .collect();
        let before: BTreeMap<_, _> = before
            .into_iter()
            .filter(|(path, _)| path.starts_with(desk.fixture.0.join("config")))
            .collect();
        assert_eq!(after, before, "Undo restores both parts");
    }

    #[test]
    fn exporting_a_local_profile_names_what_stays_and_shared_files_refuse_local_members() {
        let desk = desk("export");
        let beach = picture(&desk, "home/me/beach.png");
        picture(&desk, "pictures/Desktop Cards/teal.jpg");
        let local = profile(
            "Mine",
            WallpaperLook {
                displays: Some(DisplaysName::Separate),
                background: Some(BackgroundLook {
                    source: Some(SourceLook::Image(beach)),
                    ..Default::default()
                }),
                per_display: Some(
                    [(
                        "eDP-1".to_string(),
                        BackgroundLook {
                            source: Some(SourceLook::Bundled("teal".into())),
                            ..Default::default()
                        },
                    )]
                    .into(),
                ),
            },
        );
        let mut editor = desk.editor();
        select_saved(&desk, &mut editor, &local);
        assert!(editor.can_apply(), "a local profile applies in full");
        let (id, bytes, confine) = export_request(&mut editor);
        let text = String::from_utf8(bytes.clone()).unwrap();
        assert!(!text.contains("beach") && !text.contains("eDP-1") && !text.contains("wallpaper"));
        let file = desk.fixture.0.join("out/mine.json");
        fs::create_dir_all(file.parent().unwrap()).unwrap();
        write_export(&file, &bytes, confine.as_deref()).unwrap();
        editor.update(Message::Exported(id, Ok(Some(file.clone()))));
        assert_eq!(
            editor.notes,
            ["Kept on this computer: your own wallpaper, display-specific backgrounds"]
        );
        // Someone importing it gets the rest; no wallpaper part is left.
        let id = import_request(&mut editor);
        editor.update(Message::Imported(id, read_import(&file).map(Some)));
        assert_eq!(editor.profile().unwrap().domains(), [Domain::DesktopIcons]);

        // A shared file with this computer's pictures is refused; nothing
        // is stored.
        let count = editor.library().unwrap().len();
        let leaked = desk.fixture.0.join("out/leaked.json");
        fs::write(
            &leaked,
            crate::desktop::profiles::export_local(&local).unwrap(),
        )
        .unwrap();
        let id = import_request(&mut editor);
        editor.update(Message::Imported(id, read_import(&leaked).map(Some)));
        assert_eq!(
            editor.notice,
            Some(Notice::Error(format!("Not imported. {LOCAL_ONLY}")))
        );
        assert_eq!(editor.library().unwrap().len(), count);
    }
}

#[test]
fn recovered_preset_selection_requires_review_and_preserves_included_parts() {
    let desk = Desk::new("session-preset");
    let mut editor = desk.editor();
    editor.update(Message::Select(Selection::Preset("evening")));
    editor.update(Message::Include(Domain::Dock, false));
    let saved = editor.session_draft().unwrap();
    let text = ron::to_string(&saved).unwrap();
    let before = desk.desktop();
    let mut reopened = desk.editor();
    reopened
        .restore_session(ron::from_str(&text).unwrap())
        .unwrap();
    assert_eq!(reopened.selection(), &Selection::Preset("evening"));
    assert!(!reopened.included(Domain::Dock));
    assert!(reopened.stale());
    assert!(!reopened.can_apply());
    assert_eq!(reopened.update(Message::Apply), Outcome::None);
    reopened.refresh();
    assert!(
        reopened.stale(),
        "navigation cannot silently approve a new plan"
    );
    assert_eq!(desk.desktop(), before);
    reopened.update(Message::Replan);
    assert!(!reopened.stale());
    assert!(!reopened.included(Domain::Dock));
    reopened.update(Message::Discard);
    assert!(reopened.session_draft().is_none());
}

#[test]
fn home_snapshot_and_late_wallpaper_leave_preset_plan_and_desktop_untouched() {
    let desk = Desk::new("home-snapshot");
    let background = |color: &str| {
        format!(
            "(output: \"all\", source: Color(Single({color})), filter_by_theme: false, rotation_frequency: 3600)"
        )
    };
    desk.fixture.user(
        "com.system76.CosmicBackground",
        "all",
        &background("(0.1, 0.2, 0.3)"),
    );
    let mut editor = desk.editor();
    editor.update(Message::Select(Selection::Preset("evening")));
    editor.update(Message::Include(Domain::Dock, false));
    let plan = format!("{:?}", editor.plan());
    let before = desk.desktop();
    editor.open_home();
    let jobs = editor.take_jobs();
    assert_eq!(jobs.len(), 1, "Home requests the applied wallpaper");
    let old_job = jobs.into_iter().next().unwrap();
    assert_eq!(
        editor.preview().unwrap().config.as_ref().unwrap().theme,
        "lavender"
    );
    editor.open_home();
    assert!(
        editor.take_jobs().is_empty(),
        "do not decode twice while pending"
    );
    assert_eq!(desk.desktop(), before);

    desk.fixture.user(
        "com.system76.CosmicBackground",
        "all",
        &background("(0.3, 0.2, 0.1)"),
    );
    editor.open_home();
    let new_job = editor.take_jobs().pop().expect("new wallpaper");
    let before = desk.desktop();
    editor.update(new_job.run());
    let image = editor
        .home_image()
        .expect("new background rendered")
        .clone();
    editor.update(old_job.run());
    assert_eq!(
        editor.home_image(),
        Some(&image),
        "late old background cannot replace the current one"
    );
    editor.open_home();
    assert!(editor.take_jobs().is_empty(), "finished picture is reused");
    assert_eq!(editor.selection(), &Selection::Preset("evening"));
    assert!(!editor.included(Domain::Dock));
    assert_eq!(
        format!("{:?}", editor.plan()),
        plan,
        "Home never replans the preset"
    );
    assert_eq!(desk.desktop(), before);
}

fn finish_theme(editor: &mut ProfilesEditor) {
    for job in editor.take_theme_jobs() {
        editor.update(job.run());
    }
}
#[test]
fn shared_theme_preview_variations_reset_and_undo_use_one_transaction() {
    let desk = Desk::new("shared-theme-journey");
    let mut editor = desk.editor();
    let original = fs::read(&desk.config).unwrap();
    editor.update(Message::Select(Selection::Preset("theme-soft")));
    finish_theme(&mut editor);
    assert_eq!(
        fs::read(&desk.config).unwrap(),
        original,
        "choosing only previews"
    );
    assert!(
        editor.can_apply(),
        "{:?}",
        editor.plan().unwrap().blocking_reasons()
    );
    assert_eq!(editor.update(Message::Apply), Outcome::Changed);
    let selected = desk.widget_config();
    assert_eq!(selected["appearance"]["family"], "soft");
    assert_eq!(selected["location"]["name"], "SENTINEL");
    let baseline = ProfileLibrary::new(&desk.store())
        .load(&ProfileId::parse("before-themes").unwrap())
        .unwrap();
    assert!(baseline.appearance_only);
    name(&mut editor, NameAction::CaptureTheme, "Custom saved");
    assert!(editor.profile().unwrap().appearance_only);
    finish_theme(&mut editor);
    let (_, exported, _) = export_request(&mut editor);
    assert!(profiles::import(&exported).unwrap().appearance_only);
    // Complete chooser before proceeding; a dialog never applies the draft.
    let pending = editor.pending.as_ref().unwrap().0;
    editor.update(Message::Exported(pending, Ok(None)));
    let mut raw = desk.widget_config();
    raw["appearance"]["corner_radius"] = 3.into();
    fs::write(&desk.config, raw.to_string()).unwrap();
    editor.update(Message::ResetSurface(Domain::WidgetStyle));
    assert_eq!(desk.widget_config()["appearance"]["corner_radius"], 3);
    assert_eq!(editor.update(Message::Apply), Outcome::Changed);
    assert_eq!(desk.widget_config()["appearance"]["corner_radius"], 25);
    assert_eq!(desk.widget_config()["location"], selected["location"]);
    assert_eq!(editor.update(Message::Undo), Outcome::Changed);
    assert_eq!(desk.widget_config()["appearance"]["corner_radius"], 3);
    let imported = profiles::import(&exported).unwrap();
    assert!(imported.widget_layout.is_none());
}

#[test]
fn review_r1_matching_wallpaper_must_analyse_effective_display_background() {
    use crate::desktop::wallpaper::{self, BgColor, BgEntry, BgSource, automatic};
    let desk = Desk::new("review-r1-effective-background");
    fs::write(desk.fixture.0.join("outputs"), "eDP-1\n").unwrap();
    desk.fixture
        .user(wallpaper::NAMESPACE, "same-on-all", "false");
    desk.fixture
        .user(wallpaper::NAMESPACE, "backgrounds", "[\"eDP-1\"]");
    let entry = BgEntry::new(
        "eDP-1",
        BgSource::Color(BgColor::Single([0.95, 0.95, 0.95])),
    );
    desk.fixture.user(
        wallpaper::NAMESPACE,
        "output.eDP-1",
        &ron::to_string(&entry).unwrap(),
    );
    let mut editor = desk.editor();
    editor.update(Message::Select(Selection::Preset("theme-bare")));
    finish_theme(&mut editor);
    editor.update(Message::ThemeColors(true));
    editor.update(Message::ThemeWallpaper(true));
    finish_theme(&mut editor);
    assert!(editor.can_apply(), "{:?}", editor.plan());
    assert_eq!(editor.update(Message::Apply), Outcome::Changed);
    let store = desk.store();
    let draft = wallpaper::WallpaperDraft::load(&store).unwrap();
    let effective = automatic::source(&store, &draft);
    let actual_light = automatic::light(&effective.analyse().unwrap());
    assert!(
        actual_light,
        "the effective built-in output must still be light"
    );
    let (_, config) = automatic::read_config(&store).unwrap();
    assert!(
        config.appearance.wallpaper_light == actual_light,
        "Bare got light ink for the unused dark shared gradient; runtime prepared {:?}",
        automatic::prepare(&store)
    );
}

#[test]
fn review_r1_selecting_another_family_must_preserve_explicit_custom_surface() {
    let desk = Desk::new("review-r1-custom-across-family");
    let mut editor = desk.editor();
    editor.update(Message::Select(Selection::Preset("theme-soft")));
    finish_theme(&mut editor);
    assert_eq!(editor.update(Message::Apply), Outcome::Changed);
    let mut raw = desk.widget_config();
    raw["appearance"]["surface"] = serde_json::json!("transparent");
    raw["appearance"]["corner_radius"] = serde_json::json!(3);
    fs::write(&desk.config, raw.to_string()).unwrap();
    editor.update(Message::Select(Selection::Preset("theme-paper")));
    finish_theme(&mut editor);
    assert_eq!(editor.update(Message::Apply), Outcome::Changed);
    let changed = desk.widget_config();
    assert_eq!(
        changed["appearance"]["surface"], "transparent",
        "Custom must remain until Reset to theme"
    );
    assert_eq!(changed["appearance"]["corner_radius"], 3);
}

#[test]
fn review_r1_reset_dock_must_preserve_shared_wallpaper_color_choice() {
    use crate::desktop::wallpaper::{self, BgColor, BgEntry, BgSource};
    let desk = Desk::new("review-r1-reset-keeps-color-source");
    let entry = BgEntry::new("all", BgSource::Color(BgColor::Single([0.2, 0.5, 0.7])));
    desk.fixture.user(
        wallpaper::NAMESPACE,
        "all",
        &ron::to_string(&entry).unwrap(),
    );
    let mut editor = desk.editor();
    editor.update(Message::Select(Selection::Preset("theme-soft")));
    editor.update(Message::ThemeColors(true));
    finish_theme(&mut editor);
    assert!(editor.can_apply());
    assert_eq!(editor.update(Message::Apply), Outcome::Changed);
    assert_eq!(desk.widget_config()["appearance"]["wallpaper_colors"], true);
    assert!(
        editor.custom_surfaces.is_empty(),
        "{:?}",
        editor.custom_surfaces
    );
    for (domain, namespace) in [
        (Domain::Dock, DOCK),
        (Domain::Panel, "com.system76.CosmicPanel.Panel"),
    ] {
        desk.fixture.user(namespace, "opacity", "0.64");
        editor.refresh_surface_status();
        assert!(editor.custom_surfaces.contains(&domain));
        editor.update(Message::ResetSurface(domain));
        assert_eq!(editor.update(Message::Apply), Outcome::Changed);
        assert_eq!(desk.widget_config()["appearance"]["wallpaper_colors"], true);
        assert!(
            editor.custom_surfaces.is_empty(),
            "{:?}",
            editor.custom_surfaces
        );
    }
    drop(editor);
    let store = desk.store();
    let old_accent = desk.widget_config()["appearance"]["accent"].clone();
    desk.fixture.user(
        wallpaper::NAMESPACE,
        "all",
        &ron::to_string(&BgEntry::new(
            "all",
            BgSource::Color(BgColor::Single([0.7, 0.1, 0.2])),
        ))
        .unwrap(),
    );
    let prepared = wallpaper::automatic::prepare(&store).unwrap().unwrap();
    wallpaper::automatic::commit(&store, prepared).unwrap();
    assert_ne!(desk.widget_config()["appearance"]["accent"], old_accent);
    assert!(desk.editor().custom_surfaces.is_empty());
}

#[test]
fn review_r1_planned_wallpaper_images_preview_and_acknowledge_only_the_analyzed_source() {
    use crate::desktop::wallpaper::{WallpaperDraft, automatic};
    for retained_output in [false, true] {
        let desk = Desk::new(&format!("review-r1-planned-image-{retained_output}"));
        let image = desk.fixture.0.join("planned.png");
        image::RgbImage::from_pixel(32, 32, image::Rgb([245u8, 245, 245]))
            .save(&image)
            .unwrap();
        let mut p = profiles::themes::record(crate::desktop_theme::Family::Bare);
        p.wallpaper = Some(profiles::WallpaperLook {
            background: Some(profiles::BackgroundLook {
                source: Some(profiles::SourceLook::Image(image.clone())),
                ..Default::default()
            }),
            ..Default::default()
        });
        p.widget_style.as_mut().unwrap()["appearance"]["wallpaper_colors"] = true.into();
        if retained_output {
            fs::write(desk.fixture.0.join("outputs"), "HDMI-A-1\neDP-1\n").unwrap();
            desk.fixture
                .user(wallpaper::NAMESPACE, "same-on-all", "false");
            desk.fixture
                .user(wallpaper::NAMESPACE, "backgrounds", "[\"eDP-1\"]");
            desk.fixture.user(wallpaper::NAMESPACE, "output.eDP-1", "(output:\"eDP-1\",source:Color(Single((0.03,0.03,0.03))),filter_by_theme:false,rotation_frequency:300)");
        }
        let id = ProfileLibrary::new(&desk.store()).create(&p).unwrap();
        let mut editor = desk.editor();
        editor.update(Message::Select(Selection::Saved(id)));
        finish_theme(&mut editor);
        let analyzed = editor.theme_source.clone().unwrap();
        assert_eq!(
            analyzed.picture(),
            if retained_output {
                None
            } else {
                Some(image.as_path())
            }
        );
        assert_eq!(
            editor.backdrop,
            Backdrop::Planned(analyzed.background.clone().unwrap())
        );
        assert_eq!(
            editor.profile().unwrap().widget_style.as_ref().unwrap()["appearance"]["wallpaper_light"],
            !retained_output
        );
        assert_eq!(editor.update(Message::Apply), Outcome::Changed);
        let store = desk.store();
        let effective = automatic::source(&store, &WallpaperDraft::load(&store).unwrap());
        assert_eq!(effective.stamp, analyzed.stamp);
        assert_eq!(
            automatic::read_config(&store)
                .unwrap()
                .1
                .appearance
                .wallpaper_light,
            !retained_output
        );
        assert!(automatic::prepare(&store).unwrap().is_none());
        // An acknowledgment from the old source must not suppress a new source.
        if retained_output {
            desk.fixture.user(wallpaper::NAMESPACE, "output.eDP-1", "(output:\"eDP-1\",source:Color(Single((0.9,0.9,0.9))),filter_by_theme:false,rotation_frequency:300)");
        } else {
            image::RgbImage::from_pixel(33, 32, image::Rgb([10u8, 10, 10]))
                .save(&image)
                .unwrap();
        }
        automatic::acknowledge_analyzed(&store, &analyzed).unwrap();
        assert!(automatic::prepare(&store).unwrap().is_some());
    }
}

#[test]
fn review_r1_theme_preview_rechecks_planned_images_and_wallpaper_exclusion() {
    let desk = Desk::new("review-r1-planned-image-conflict");
    // The desktop's own wallpaper, so the test never falls back to the
    // computer's installed COSMIC default.
    let current = desk.fixture.0.join("current.png");
    image::RgbImage::from_pixel(32, 32, image::Rgb([40u8, 60, 90]))
        .save(&current)
        .unwrap();
    desk.fixture.system(
        "com.system76.CosmicBackground",
        "all",
        &format!(
            "(output: \"all\", source: Path({:?}), filter_by_theme: true, rotation_frequency: 3600, filter_method: Lanczos, scaling_mode: Zoom, sampling_method: Alphanumeric)",
            current.to_string_lossy()
        ),
    );
    let image = desk.fixture.0.join("planned.png");
    image::RgbImage::from_pixel(32, 32, image::Rgb([245u8, 245, 245]))
        .save(&image)
        .unwrap();
    let mut p = profiles::themes::record(crate::desktop_theme::Family::Bare);
    p.wallpaper = Some(profiles::WallpaperLook {
        background: Some(profiles::BackgroundLook {
            source: Some(profiles::SourceLook::Image(image.clone())),
            ..Default::default()
        }),
        ..Default::default()
    });
    p.widget_style.as_mut().unwrap()["appearance"]["wallpaper_colors"] = true.into();
    let id = ProfileLibrary::new(&desk.store()).create(&p).unwrap();
    let mut editor = desk.editor();
    editor.update(Message::Select(Selection::Saved(id)));
    finish_theme(&mut editor);
    image::RgbImage::from_pixel(33, 32, image::Rgb([10u8, 10, 10]))
        .save(&image)
        .unwrap();
    assert_eq!(editor.update(Message::Apply), Outcome::None);
    assert!(editor.stale());
    editor.update(Message::Replan);
    finish_theme(&mut editor);
    assert_eq!(
        editor.profile().unwrap().widget_style.as_ref().unwrap()["appearance"]["wallpaper_light"],
        false
    );
    editor.update(Message::Include(Domain::Wallpaper, false));
    finish_theme(&mut editor);
    assert_eq!(
        editor.theme_source.as_ref().unwrap().stamp,
        wallpaper::automatic::source(
            &desk.store(),
            &wallpaper::WallpaperDraft::load(&desk.store()).unwrap()
        )
        .stamp
    );
    let saved = editor.session_draft().unwrap();
    let mut reopened = desk.editor();
    reopened.restore_session(saved).unwrap();
    finish_theme(&mut reopened);
    assert!(!reopened.included(Domain::Wallpaper));
    assert_eq!(
        reopened.theme_source.as_ref().unwrap().stamp,
        editor.theme_source.as_ref().unwrap().stamp
    );
}

#[test]
fn review_r1_family_switches_keep_custom_widgets_and_bars_until_reset_or_saved_restore() {
    let desk = Desk::new("review-r1-family-overrides");
    let mut editor = desk.editor();
    editor.update(Message::Select(Selection::Preset("theme-soft")));
    finish_theme(&mut editor);
    assert_eq!(editor.update(Message::Apply), Outcome::Changed);
    let saved = profiles::themes::capture(&desk.store(), "Saved Soft")
        .unwrap()
        .profile;
    let id = ProfileLibrary::new(&desk.store()).create(&saved).unwrap();
    let mut raw = desk.widget_config();
    raw["appearance"]["surface"] = "transparent".into();
    raw["appearance"]["corner_radius"] = 3.into();
    raw["appearance"]["contrast_assist"] = true.into();
    fs::write(&desk.config, raw.to_string()).unwrap();
    desk.fixture.user(DOCK, "opacity", "0.64");
    desk.fixture
        .user("com.system76.CosmicPanel.Panel", "border_radius", "9");
    for family in ["theme-paper", "theme-outline", "theme-bare", "theme-soft"] {
        editor.update(Message::Select(Selection::Preset(family)));
        finish_theme(&mut editor);
        assert_eq!(
            editor.update(Message::Apply),
            Outcome::Changed,
            "{family}: {:?}",
            editor.notice
        );
        let raw = desk.widget_config();
        assert_eq!(raw["appearance"]["surface"], "transparent");
        assert_eq!(raw["appearance"]["corner_radius"], 3);
        assert_eq!(raw["appearance"]["contrast_assist"], true);
        assert!(
            wallpaper::automatic::read_config(&desk.store())
                .unwrap()
                .1
                .widget_custom()
        );
        assert_eq!(
            editor.custom_surfaces,
            [Domain::Dock, Domain::Panel].into_iter().collect()
        );
        assert_eq!(
            PanelDraft::load(&desk.store(), PanelKind::Dock)
                .unwrap()
                .value::<f32>("opacity")
                .known()
                .copied(),
            Some(0.64)
        );
        assert_eq!(
            PanelDraft::load(&desk.store(), PanelKind::Panel)
                .unwrap()
                .value::<u32>("border_radius")
                .known()
                .copied(),
            Some(9)
        );
    }
    for domain in [Domain::WidgetStyle, Domain::Dock, Domain::Panel] {
        editor.update(Message::ResetSurface(domain));
        assert_eq!(editor.update(Message::Apply), Outcome::Changed);
    }
    assert!(editor.custom_surfaces.is_empty());
    assert!(
        !wallpaper::automatic::read_config(&desk.store())
            .unwrap()
            .1
            .widget_custom()
    );
    // Deliberate saved-variation restoration still replaces overrides.
    desk.fixture.user(DOCK, "opacity", "0.64");
    let mut raw = desk.widget_config();
    raw["appearance"]["surface"] = "transparent".into();
    fs::write(&desk.config, raw.to_string()).unwrap();
    editor.update(Message::Select(Selection::Saved(id)));
    finish_theme(&mut editor);
    assert_eq!(editor.update(Message::Apply), Outcome::Changed);
    assert!(editor.custom_surfaces.is_empty());
    assert!(
        !wallpaper::automatic::read_config(&desk.store())
            .unwrap()
            .1
            .widget_custom()
    );
}

#[test]
fn review_r1_native_custom_colors_survive_repeated_family_switches() {
    use crate::desktop::appearance::{ColorRole, Control, ThemeVariant, Value};
    let desk = Desk::new("review-r1-native-custom-colors");
    let mut editor = desk.editor();
    editor.update(Message::Select(Selection::Preset("theme-soft")));
    finish_theme(&mut editor);
    assert_eq!(editor.update(Message::Apply), Outcome::Changed);
    let mut custom = AppearanceDraft::load(&desk.store()).unwrap();
    let color = crate::desktop::appearance::parse_hex("#315274", true);
    custom.set(
        Control::Color(ThemeVariant::Dark, ColorRole::Background),
        Value::Color(color),
    );
    desk.store().apply(&custom.plan().patch).unwrap();
    for family in ["theme-paper", "theme-outline", "theme-bare", "theme-soft"] {
        editor.update(Message::Select(Selection::Preset(family)));
        finish_theme(&mut editor);
        assert_eq!(editor.update(Message::Apply), Outcome::Changed);
        assert_eq!(
            AppearanceDraft::load(&desk.store())
                .unwrap()
                .color(ThemeVariant::Dark, ColorRole::Background)
                .known()
                .copied(),
            Some(color)
        );
        assert_eq!(
            editor.custom_surfaces,
            [Domain::Dock, Domain::Panel].into_iter().collect()
        );
    }
}

#[test]
fn review_r2_native_custom_with_theme_background_survives_family_roundtrip() {
    use crate::desktop::appearance::{ColorRole, Control, ThemeVariant, Value};
    for native in ThemeVariant::ALL {
        for role in [ColorRole::Background, ColorRole::Primary] {
            let desk = Desk::new(&format!("review-r2-theme-background-{native:?}-{role:?}"));
            desk.fixture.user(
                "com.system76.CosmicTheme.Mode",
                "is_dark",
                if native == ThemeVariant::Dark {
                    "true"
                } else {
                    "false"
                },
            );
            desk.fixture.user(wallpaper::NAMESPACE, "all",
            "(output:\"all\",source:Color(Single((0.2,0.45,0.7))),filter_by_theme:false,rotation_frequency:300)");
            let mut editor = desk.editor();
            let (initial, families) = if native == ThemeVariant::Dark {
                (
                    "theme-soft",
                    ["theme-paper", "theme-outline", "theme-bare", "theme-soft"],
                )
            } else {
                (
                    "theme-paper",
                    ["theme-outline", "theme-paper", "theme-bare", "theme-paper"],
                )
            };
            editor.update(Message::Select(Selection::Preset(initial)));
            let automatic = role == ColorRole::Primary;
            editor.update(Message::ThemeColors(automatic));
            finish_theme(&mut editor);
            assert_eq!(editor.update(Message::Apply), Outcome::Changed);
            for namespace in [DOCK, "com.system76.CosmicPanel.Panel"] {
                desk.fixture.user(namespace, "background", "ThemeDefault");
            }
            let mut custom = AppearanceDraft::load(&desk.store()).unwrap();
            let mut color = crate::desktop::appearance::parse_hex("#315274", true);
            custom.set(Control::Color(native, role), Value::Color(color));
            desk.store().apply(&custom.plan().patch).unwrap();
            for (index, family) in families.into_iter().enumerate() {
                editor.update(Message::Select(Selection::Preset(family)));
                finish_theme(&mut editor);
                assert_eq!(editor.update(Message::Apply), Outcome::Changed);
                let appearance = AppearanceDraft::load(&desk.store()).unwrap();
                let actual = appearance.color(native, role).known().copied();
                assert_eq!(
                    actual,
                    Some(color),
                    "{native:?} Custom {role:?} at {family}"
                );
                assert_eq!(
                    appearance.active_variant(),
                    native,
                    "family must not switch native mode"
                );
                assert_eq!(
                    wallpaper::automatic::read_config(&desk.store())
                        .unwrap()
                        .1
                        .appearance
                        .wallpaper_colors,
                    automatic
                );
                assert_eq!(
                    editor.custom_surfaces,
                    [Domain::Dock, Domain::Panel].into_iter().collect()
                );
                for kind in PanelKind::ALL {
                    assert_eq!(
                        PanelDraft::load(&desk.store(), kind)
                            .unwrap()
                            .value::<crate::desktop::panels::Background>("background")
                            .known(),
                        Some(&crate::desktop::panels::Background::ThemeDefault)
                    );
                }
                if index == 1 {
                    // A subsequent native edit, not an old saved override, must win.
                    color = crate::desktop::appearance::parse_hex("#7A3852", true);
                    let mut custom = appearance;
                    custom.set(Control::Color(native, role), Value::Color(color));
                    desk.store().apply(&custom.plan().patch).unwrap();
                }
            }
        }
    }
}

fn r2_image_theme(retained_output: bool) -> (Desk, ProfilesEditor, PathBuf) {
    let desk = Desk::new(&format!(
        "review-r2-replaced-planned-preview-{retained_output}"
    ));
    let image = desk.fixture.0.join("planned.png");
    image::RgbImage::from_pixel(32, 32, image::Rgb([245u8, 245, 245]))
        .save(&image)
        .unwrap();
    let mut p = profiles::themes::record(crate::desktop_theme::Family::Bare);
    p.wallpaper = Some(profiles::WallpaperLook {
        background: Some(profiles::BackgroundLook {
            source: Some(profiles::SourceLook::Image(image.clone())),
            ..Default::default()
        }),
        ..Default::default()
    });
    p.widget_style.as_mut().unwrap()["appearance"]["wallpaper_colors"] = true.into();
    let id = ProfileLibrary::new(&desk.store()).create(&p).unwrap();
    let mut editor = desk.editor();
    if retained_output {
        fs::write(desk.fixture.0.join("outputs"), "eDP-1\n").unwrap();
        desk.fixture
            .user(wallpaper::NAMESPACE, "same-on-all", "false");
        desk.fixture
            .user(wallpaper::NAMESPACE, "backgrounds", "[\"eDP-1\"]");
        let entry = wallpaper::BgEntry::new("eDP-1", wallpaper::BgSource::Path(image.clone()));
        desk.fixture.user(
            wallpaper::NAMESPACE,
            "output.eDP-1",
            &ron::to_string(&entry).unwrap(),
        );
        editor.update(Message::Select(Selection::Preset("theme-bare")));
        editor.update(Message::ThemeColors(true));
        editor.update(Message::ThemeWallpaper(true));
    } else {
        editor.update(Message::Select(Selection::Saved(id)));
    }
    (desk, editor, image)
}

fn r2_backdrop_pixel(editor: &ProfilesEditor) -> [u8; 3] {
    let Handle::Rgba { pixels, .. } = editor.backdrop_image().unwrap() else {
        panic!("the preview should contain decoded pixels")
    };
    [pixels[0], pixels[1], pixels[2]]
}

#[test]
fn review_r2_replaced_planned_image_refreshes_visible_backdrop_pixels() {
    for retained_output in [false, true] {
        let (desk, mut editor, image) = r2_image_theme(retained_output);
        finish_theme(&mut editor);
        for job in editor.take_jobs() {
            editor.update(job.run());
        }
        assert_eq!(r2_backdrop_pixel(&editor), [245, 245, 245]);
        let old_handle = editor.backdrop_image().unwrap().clone();
        let old_source = editor.theme_source.clone().unwrap();
        image::RgbImage::from_pixel(33, 32, image::Rgb([10u8, 10, 10]))
            .save(&image)
            .unwrap();
        let before = desk.desktop();
        assert_eq!(editor.update(Message::Apply), Outcome::None);
        assert!(editor.stale());
        assert_eq!(desk.desktop(), before, "stale Apply writes nothing");
        assert!(editor.backdrop_image().is_none());
        editor.update(Message::Replan);
        assert_ne!(
            editor.theme_source.as_ref().unwrap().stamp,
            old_source.stamp
        );
        assert!(
            editor.backdrop_image().is_none(),
            "old cached pixels cannot satisfy the new version"
        );
        finish_theme(&mut editor);
        for job in editor.take_jobs() {
            editor.update(job.run());
        }
        assert_eq!(
            editor.profile().unwrap().widget_style.as_ref().unwrap()["appearance"]["wallpaper_light"],
            false
        );
        assert!(editor.can_apply());
        assert_eq!(
            r2_backdrop_pixel(&editor),
            [10, 10, 10],
            "retained output={retained_output}"
        );
        assert_ne!(
            editor.backdrop_image(),
            Some(&old_handle),
            "replace the real image handle"
        );
        let stamp = editor.theme_source.as_ref().unwrap().stamp;
        assert_eq!(editor.update(Message::Apply), Outcome::Changed);
        let store = desk.store();
        assert_eq!(
            wallpaper::automatic::source(&store, &wallpaper::WallpaperDraft::load(&store).unwrap())
                .stamp,
            stamp
        );
        assert!(
            !wallpaper::automatic::read_config(&store)
                .unwrap()
                .1
                .appearance
                .wallpaper_light
        );
        assert!(
            wallpaper::automatic::prepare(&store).unwrap().is_none(),
            "acknowledge the analyzed version"
        );
    }
}

#[test]
fn review_r2_explicit_variants_reset_and_saved_restoration_remain_deliberate() {
    use crate::desktop::appearance::{ColorRole, Control, ThemeVariant, Value};
    use crate::desktop::panels::Background;
    for native in ThemeVariant::ALL {
        let desk = Desk::new(&format!("review-r2-explicit-background-{native:?}"));
        desk.fixture.user(
            "com.system76.CosmicTheme.Mode",
            "is_dark",
            if native == ThemeVariant::Dark {
                "true"
            } else {
                "false"
            },
        );
        let (variant, initial, families, background) = if native == ThemeVariant::Dark {
            (
                ThemeVariant::Light,
                "theme-paper",
                ["theme-soft", "theme-paper", "theme-outline", "theme-paper"],
                Background::Light,
            )
        } else {
            (
                ThemeVariant::Dark,
                "theme-soft",
                ["theme-paper", "theme-outline", "theme-bare", "theme-soft"],
                Background::Dark,
            )
        };
        let mut editor = desk.editor();
        editor.update(Message::Select(Selection::Preset(initial)));
        finish_theme(&mut editor);
        assert_eq!(editor.update(Message::Apply), Outcome::Changed);
        let saved = profiles::themes::capture(&desk.store(), "Uncustomized family")
            .unwrap()
            .profile;
        let id = ProfileLibrary::new(&desk.store()).create(&saved).unwrap();
        let customize = || {
            for kind in PanelKind::ALL {
                desk.fixture.user(
                    &kind.namespace(),
                    "background",
                    &ron::to_string(&background).unwrap(),
                );
            }
            let mut appearance = AppearanceDraft::load(&desk.store()).unwrap();
            for (role, hex) in [
                (ColorRole::Background, "#315274"),
                (ColorRole::Primary, "#7A3852"),
            ] {
                appearance.set(
                    Control::Color(variant, role),
                    Value::Color(crate::desktop::appearance::parse_hex(hex, true)),
                );
            }
            desk.store().apply(&appearance.plan().patch).unwrap();
        };
        customize();
        for family in families {
            editor.update(Message::Select(Selection::Preset(family)));
            finish_theme(&mut editor);
            assert_eq!(editor.update(Message::Apply), Outcome::Changed);
            let appearance = AppearanceDraft::load(&desk.store()).unwrap();
            assert_eq!(appearance.active_variant(), native);
            for (role, hex) in [
                (ColorRole::Background, "#315274"),
                (ColorRole::Primary, "#7A3852"),
            ] {
                assert_eq!(
                    appearance.color(variant, role).known().copied(),
                    Some(crate::desktop::appearance::parse_hex(hex, true)),
                    "{native:?} {family} {role:?}"
                );
            }
        }
        // Theme background uses the native mode. Reset and an explicitly
        // selected saved variation still restore their recorded surface roles.
        for kind in PanelKind::ALL {
            desk.fixture
                .user(&kind.namespace(), "background", "ThemeDefault");
        }
        for domain in [Domain::Dock, Domain::Panel] {
            editor.update(Message::ResetSurface(domain));
            // Both variants are now generated together. Dock resets the
            // shared surface roles; Panel already uses ThemeDefault here.
            assert_eq!(
                editor.update(Message::Apply),
                if domain == Domain::Dock {
                    Outcome::Changed
                } else {
                    Outcome::None
                }
            );
        }
        assert!(editor.custom_surfaces.is_empty());
        customize();
        editor.update(Message::Select(Selection::Saved(id)));
        finish_theme(&mut editor);
        assert_eq!(editor.update(Message::Apply), Outcome::Changed);
        assert!(editor.custom_surfaces.is_empty());
        let actual = profiles::themes::capture(&desk.store(), "Restored")
            .unwrap()
            .profile;
        assert_eq!(actual.appearance, saved.appearance);
    }
}

#[test]
fn review_r2_obsolete_theme_and_backdrop_results_cannot_replace_current_pixels() {
    for retained_output in [false, true] {
        for old_first in [false, true] {
            let (desk, mut editor, image) = r2_image_theme(retained_output);
            let old_colors: Vec<_> = editor
                .take_theme_jobs()
                .into_iter()
                .map(ThemeJob::run)
                .collect();
            let old_pictures: Vec<_> = editor
                .take_jobs()
                .into_iter()
                .map(BackdropJob::run)
                .collect();
            let old_stamp = editor.theme_source.as_ref().unwrap().stamp;
            image::RgbImage::from_pixel(33, 32, image::Rgb([10u8, 10, 10]))
                .save(&image)
                .unwrap();
            editor.update(Message::Replan);
            let new_stamp = editor.theme_source.as_ref().unwrap().stamp;
            assert_ne!(new_stamp, old_stamp);
            let deliver_old = |editor: &mut ProfilesEditor| {
                for message in old_colors.iter().chain(&old_pictures).cloned() {
                    editor.update(message);
                }
                assert_eq!(editor.theme_source.as_ref().unwrap().stamp, new_stamp);
                assert!(
                    !editor.stale(),
                    "obsolete requests must not invalidate current work"
                );
            };
            if old_first {
                deliver_old(&mut editor);
                assert!(editor.theme_loading());
                assert!(editor.backdrop_image().is_none());
            }
            finish_theme(&mut editor);
            for job in editor.take_jobs() {
                editor.update(job.run());
            }
            let handle = editor.backdrop_image().unwrap().clone();
            assert_eq!(r2_backdrop_pixel(&editor), [10, 10, 10]);
            if !old_first {
                deliver_old(&mut editor);
                assert_eq!(editor.backdrop_image(), Some(&handle));
            }
            assert_eq!(
                editor.profile().unwrap().widget_style.as_ref().unwrap()["appearance"]["wallpaper_light"],
                false
            );
            // Reusing the same analyzed version should not decode it again.
            editor.update(Message::Replan);
            finish_theme(&mut editor);
            assert!(editor.take_jobs().is_empty());
            assert_eq!(editor.backdrop_image(), Some(&handle));
            assert_eq!(editor.update(Message::Apply), Outcome::Changed);
            assert!(
                wallpaper::automatic::prepare(&desk.store())
                    .unwrap()
                    .is_none()
            );
        }
    }
}

#[test]
fn review_r2_file_changes_during_analysis_or_decode_require_another_review() {
    for retained_output in [false, true] {
        for colors_ready in [false, true] {
            let (desk, mut editor, image) = r2_image_theme(retained_output);
            let colors: Vec<_> = editor
                .take_theme_jobs()
                .into_iter()
                .map(ThemeJob::run)
                .collect();
            let pictures: Vec<_> = editor
                .take_jobs()
                .into_iter()
                .map(BackdropJob::run)
                .collect();
            if colors_ready {
                for message in colors.iter().cloned() {
                    editor.update(message);
                }
            }
            image::RgbImage::from_pixel(33, 32, image::Rgb([10u8, 10, 10]))
                .save(&image)
                .unwrap();
            let before = desk.desktop();
            if !colors_ready {
                for message in colors {
                    editor.update(message);
                }
            }
            for message in pictures {
                editor.update(message);
            }
            assert!(
                editor.stale(),
                "a current request can still read an obsolete file version"
            );
            assert!(!editor.can_apply());
            assert!(editor.backdrop_image().is_none());
            assert_eq!(editor.update(Message::Apply), Outcome::None);
            assert_eq!(desk.desktop(), before);
            editor.update(Message::Replan);
            finish_theme(&mut editor);
            for job in editor.take_jobs() {
                editor.update(job.run());
            }
            assert_eq!(r2_backdrop_pixel(&editor), [10, 10, 10]);
            assert!(editor.can_apply());
        }
    }
}

/// A solid background, so wallpaper colors can be analyzed in the fixture.
fn solid_wallpaper(desk: &Desk, rgb: [f32; 3]) {
    use crate::desktop::wallpaper::{self, BgColor, BgEntry, BgSource};
    desk.fixture.user(
        wallpaper::NAMESPACE,
        "all",
        &ron::to_string(&BgEntry::new("all", BgSource::Color(BgColor::Single(rgb)))).unwrap(),
    );
}
fn style_value(editor: &ProfilesEditor, pointer: &str) -> Option<serde_json::Value> {
    editor
        .profile()
        .and_then(|p| p.widget_style.as_ref())
        .and_then(|s| s.pointer(pointer))
        .cloned()
}
fn themes_editor(desk: &Desk) -> ProfilesEditor {
    let mut editor = ProfilesEditor { ..desk.editor() };
    editor.open();
    editor.enter(previews::PreviewKind::Theme);
    finish_theme(&mut editor);
    editor
}

#[test]
fn first_theme_keeps_the_existing_color_choice_or_starts_from_the_wallpaper() {
    let cases = [
        // Following the wallpaper stays on.
        (r#"{"appearance":{"wallpaper_colors":true}}"#, true, None),
        // An explicit fixed color is kept rather than silently replaced.
        (
            r#"{"appearance":{"accent":[30,120,200]}}"#,
            false,
            Some([30, 120, 200]),
        ),
        // Someone who never chose starts with their wallpaper's colors.
        ("{}", true, None),
    ];
    for (index, (config, auto, accent)) in cases.into_iter().enumerate() {
        let desk = Desk::new(&format!("first-theme-colors-{index}"));
        solid_wallpaper(&desk, [0.1, 0.45, 0.4]);
        fs::write(&desk.config, config).unwrap();
        let mut editor = themes_editor(&desk);
        assert!(
            editor.suggestion().is_some(),
            "the applied wallpaper is analyzed"
        );
        editor.update(Message::Select(Selection::Preset("theme-soft")));
        finish_theme(&mut editor);
        assert_eq!(
            style_value(&editor, "/appearance/wallpaper_colors"),
            Some(auto.into()),
            "{config}"
        );
        let chosen: Option<[u8; 3]> = style_value(&editor, "/appearance/accent")
            .and_then(|v| serde_json::from_value(v).ok())
            .flatten();
        if auto {
            let hue = crate::desktop_theme::tone::lch(chosen.expect("a wallpaper color")).h;
            let picture = crate::desktop_theme::tone::lch([26, 115, 102]).h;
            assert!((hue - picture).abs() < 0.2, "{chosen:?}");
        } else {
            assert_eq!(chosen, accent, "{config}");
        }
    }
    // Without a readable wallpaper there is nothing to follow.
    let desk = Desk::new("first-theme-colors-unavailable");
    {
        use crate::desktop::wallpaper::{self, BgEntry, BgSource};
        let missing = desk.fixture.0.join("missing.png");
        desk.fixture.user(
            wallpaper::NAMESPACE,
            "all",
            &ron::to_string(&BgEntry::new("all", BgSource::Path(missing))).unwrap(),
        );
    }
    fs::write(&desk.config, "{}").unwrap();
    let mut editor = themes_editor(&desk);
    editor.update(Message::Select(Selection::Preset("theme-soft")));
    finish_theme(&mut editor);
    assert_eq!(
        style_value(&editor, "/appearance/wallpaper_colors"),
        Some(false.into())
    );
    assert!(editor.theme_problem().is_none());
}

#[test]
fn style_changes_keep_the_color_and_matching_wallpaper_choices() {
    let desk = Desk::new("style-keeps-color");
    solid_wallpaper(&desk, [0.6, 0.2, 0.5]);
    let mut editor = themes_editor(&desk);
    editor.update(Message::Select(Selection::Preset("theme-soft")));
    finish_theme(&mut editor);
    let blue = [40, 110, 200];
    editor.update(Message::ThemeAccent(blue));
    editor.update(Message::ThemeWallpaper(true));
    finish_theme(&mut editor);
    for family in ["theme-outline", "theme-paper", "theme-bare"] {
        editor.update(Message::Select(Selection::Preset(family)));
        finish_theme(&mut editor);
        assert_eq!(
            style_value(&editor, "/appearance/accent"),
            Some(serde_json::json!(blue)),
            "{family}"
        );
        assert_eq!(
            style_value(&editor, "/appearance/wallpaper_colors"),
            Some(false.into())
        );
        let profile = editor.profile().unwrap();
        let family = profiles::themes::family(profile).unwrap();
        assert_eq!(profile.wallpaper, Some(profiles::themes::matching(family)));
    }
    // A style's own color is that style's, not carried as a fixed color.
    editor.update(Message::ThemeColors(false));
    editor.update(Message::Select(Selection::Preset("theme-soft")));
    finish_theme(&mut editor);
    assert_eq!(
        style_value(&editor, "/appearance/accent"),
        Some(serde_json::Value::Null)
    );
    // Following the wallpaper carries too.
    editor.update(Message::ThemeColors(true));
    finish_theme(&mut editor);
    editor.update(Message::Select(Selection::Preset("theme-outline")));
    finish_theme(&mut editor);
    assert_eq!(
        style_value(&editor, "/appearance/wallpaper_colors"),
        Some(true.into())
    );
    assert!(
        editor.can_apply(),
        "{:?}",
        editor.plan().map(|p| p.blocking_reasons())
    );
}

#[test]
fn custom_colors_apply_only_complete_hex_and_color_choices_edit_the_applied_look() {
    let desk = Desk::new("custom-theme-color");
    solid_wallpaper(&desk, [0.2, 0.3, 0.6]);
    let mut editor = themes_editor(&desk);
    editor.update(Message::Select(Selection::Preset("theme-soft")));
    finish_theme(&mut editor);
    let before = style_value(&editor, "/appearance/accent");
    editor.update(Message::ThemeCustom(Some("#5E8".into())));
    assert_eq!(editor.custom.as_deref(), Some("#5E8"));
    assert_eq!(
        style_value(&editor, "/appearance/accent"),
        before,
        "incomplete text"
    );
    editor.update(Message::ThemeCustom(Some("#5E81AC".into())));
    assert_eq!(
        style_value(&editor, "/appearance/accent"),
        Some(serde_json::json!([94, 129, 172]))
    );
    assert_eq!(
        style_value(&editor, "/appearance/wallpaper_colors"),
        Some(false.into())
    );
    editor.update(Message::ThemeCustom(None));
    assert!(editor.custom.is_none());
    assert_eq!(editor.update(Message::Apply), Outcome::Changed);
    assert_eq!(
        desk.widget_config()["appearance"]["accent"],
        serde_json::json!([94, 129, 172])
    );
    // With nothing selected, a color choice edits the applied look.
    editor.update(Message::Discard);
    assert_eq!(*editor.selection(), Selection::Current);
    editor.update(Message::ThemeColors(true));
    finish_theme(&mut editor);
    assert_eq!(
        editor.profile().and_then(profiles::themes::family),
        Some(crate::desktop_theme::Family::Soft)
    );
    assert_eq!(
        style_value(&editor, "/appearance/wallpaper_colors"),
        Some(true.into())
    );
}

#[test]
fn window_outline_is_its_own_surface_with_a_scoped_reset() {
    use crate::desktop::appearance::{ColorRole, Control, ThemeVariant, Value, parse_hex};
    let desk = Desk::new("windows-surface");
    let mut editor = themes_editor(&desk);
    editor.update(Message::Select(Selection::Preset("theme-soft")));
    finish_theme(&mut editor);
    assert_eq!(editor.update(Message::Apply), Outcome::Changed);
    assert!(
        editor.custom_surfaces.is_empty(),
        "{:?}",
        editor.custom_surfaces
    );
    // A later outline choice in Theme & fonts is a window detail only.
    let orange = parse_hex("#FFAD00", false);
    let mut native = AppearanceDraft::load(&desk.store()).unwrap();
    native.set(
        Control::Color(ThemeVariant::Dark, ColorRole::Outline),
        Value::Color(orange),
    );
    desk.store().apply(&native.plan().patch).unwrap();
    editor.refresh();
    assert_eq!(
        editor.custom_surfaces,
        [Domain::Appearance].into_iter().collect(),
        "bars keep matching their theme"
    );
    // Another style keeps that deliberate outline.
    editor.update(Message::Select(Selection::Preset("theme-paper")));
    finish_theme(&mut editor);
    assert_eq!(editor.update(Message::Apply), Outcome::Changed);
    let outline = |desk: &Desk| {
        AppearanceDraft::load(&desk.store())
            .unwrap()
            .color(ThemeVariant::Dark, ColorRole::Outline)
            .known()
            .copied()
    };
    assert_eq!(outline(&desk), Some(orange));
    // Reset links it to the accent again and leaves the bars alone.
    let dock = desk.fixture.read(DOCK, "opacity");
    editor.update(Message::ResetSurface(Domain::Appearance));
    let plan = editor.plan().unwrap();
    assert!(
        plan.domains
            .iter()
            .all(|part| matches!(part.domain, Domain::Appearance | Domain::WidgetStyle))
    );
    assert_eq!(editor.update(Message::Apply), Outcome::Changed);
    assert_eq!(outline(&desk), Some(None));
    assert_eq!(desk.fixture.read(DOCK, "opacity"), dock);
    assert!(editor.custom_surfaces.is_empty());
    assert_eq!(editor.update(Message::Undo), Outcome::Changed);
    assert_eq!(outline(&desk), Some(orange));
}

#[test]
fn a_theme_draft_saved_by_the_previous_release_recovers_with_current_roles_and_its_choices() {
    use crate::desktop::appearance::ThemeVariant;
    use crate::desktop_theme::{Family, previous_native};
    let desk = Desk::new("session-previous-theme");
    let editor = themes_editor(&desk);
    // What the previous release saved for a pending Soft choice: its own
    // single accent and surfaces, no window outline, wallpaper colors and the
    // matching gradient chosen.
    let mut old = profiles::themes::record(Family::Soft);
    let (accent, surface) = previous_native(Family::Soft, None);
    let hex = |[r, g, b]: [u8; 3]| profiles::ColorChoice::Hex(format!("#{r:02x}{g:02x}{b:02x}"));
    let look = old.appearance.as_mut().unwrap();
    for variant in [&mut look.light, &mut look.dark] {
        let variant = variant.as_mut().unwrap();
        variant.accent = Some(hex(accent));
        variant.outline = None;
    }
    let dark = old.appearance.as_mut().unwrap().dark.as_mut().unwrap();
    dark.background = Some(hex(surface));
    dark.primary = Some(hex(surface));
    old.widget_style.as_mut().unwrap()["appearance"]["wallpaper_colors"] = true.into();
    old.wallpaper = Some(profiles::themes::matching(Family::Soft));
    let saved = SavedSelection {
        preset: true,
        id: "theme-soft".into(),
        excluded: Vec::new(),
        theme: Some(serde_json::to_string(&old).unwrap()),
    };
    let text = ron::to_string(&saved).unwrap();
    drop(editor);
    let mut reopened = themes_editor(&desk);
    reopened
        .restore_session(ron::from_str(&text).unwrap())
        .unwrap();
    finish_theme(&mut reopened);
    let p = reopened.profile().unwrap();
    assert_eq!(
        style_value(&reopened, "/appearance/wallpaper_colors"),
        Some(true.into())
    );
    assert_eq!(
        p.wallpaper,
        Some(profiles::themes::matching(Family::Soft)),
        "choices carry"
    );
    for variant in ThemeVariant::ALL {
        assert_eq!(
            p.appearance
                .as_ref()
                .unwrap()
                .variant(variant)
                .unwrap()
                .outline,
            Some(profiles::ColorChoice::Auto),
            "{variant:?}: the recovered look links the window outline"
        );
    }
    let background = p
        .appearance
        .as_ref()
        .unwrap()
        .dark
        .as_ref()
        .unwrap()
        .background
        .clone();
    assert_ne!(
        background,
        Some(hex(surface)),
        "today's surfaces, not the saved ones"
    );
    assert!(reopened.stale(), "a recovered draft still waits for review");
}

/// Two distinct hues, so the Color row offers one besides "From wallpaper".
fn two_hue_wallpaper(desk: &Desk) -> Vec<[u8; 3]> {
    use crate::desktop::wallpaper::{self, BgColor, BgEntry, BgGradient, BgSource};
    let picture = BgColor::Gradient(BgGradient {
        colors: vec![[0.6, 0.05, 0.3], [0.02, 0.35, 0.3]],
        radius: 90.,
    });
    desk.fixture.user(
        wallpaper::NAMESPACE,
        "all",
        &ron::to_string(&BgEntry::new("all", BgSource::Color(picture.clone()))).unwrap(),
    );
    let hues = wallpaper::palette::from_color(&picture).accents;
    assert!(hues.len() > 1, "{hues:?}");
    hues
}
fn choose_color(editor: &mut ProfilesEditor, color: LookColor) {
    editor.update(match color {
        LookColor::Fixed(rgb) => Message::ThemeAccent(rgb),
        LookColor::Style => Message::ThemeColors(false),
        LookColor::Wallpaper => Message::ThemeColors(true),
    });
    finish_theme(editor);
}

#[test]
fn reopened_style_changes_keep_the_applied_color_choice() {
    use crate::desktop::appearance::ThemeVariant;
    for (index, kind) in ["custom", "hue", "style", "wallpaper"]
        .into_iter()
        .enumerate()
    {
        let desk = Desk::new(&format!("reopen-keeps-color-{index}"));
        let hues = two_hue_wallpaper(&desk);
        let color = match kind {
            "custom" => LookColor::Fixed([94, 129, 172]),
            "hue" => LookColor::Fixed(hues[1]),
            "style" => LookColor::Style,
            _ => LookColor::Wallpaper,
        };
        let mut editor = themes_editor(&desk);
        editor.update(Message::Select(Selection::Preset("theme-soft")));
        finish_theme(&mut editor);
        choose_color(&mut editor, color);
        assert_eq!(editor.update(Message::Apply), Outcome::Changed);
        let applied = desk.widget_config()["appearance"].clone();
        drop(editor);

        // Reopened: nothing pending, the applied look is Current, and a
        // style-only choice keeps its color.
        let mut reopened = themes_editor(&desk);
        assert_eq!(*reopened.selection(), Selection::Current);
        reopened.update(Message::Select(Selection::Preset("theme-outline")));
        finish_theme(&mut reopened);
        assert_eq!(
            LookColor::of(reopened.profile().unwrap()),
            Some(color),
            "{kind}"
        );
        assert_eq!(reopened.update(Message::Apply), Outcome::Changed);
        let stored = desk.widget_config()["appearance"].clone();
        assert_eq!(stored["family"], "outline");
        assert_eq!(
            stored["wallpaper_colors"], applied["wallpaper_colors"],
            "{kind}"
        );
        if color == LookColor::Style {
            assert_eq!(stored["accent"], serde_json::Value::Null, "Outline's own");
        } else {
            assert_eq!(stored["accent"], applied["accent"], "{kind}");
        }
        // COSMIC follows the same color, so Windows matches Outline.
        let (_, config) = crate::desktop::wallpaper::automatic::read_config(&desk.store()).unwrap();
        let native = AppearanceDraft::load(&desk.store()).unwrap();
        for variant in ThemeVariant::ALL {
            assert!(
                profiles::themes::linked_accent(&native, config.appearance, variant),
                "{kind}: {variant:?}"
            );
        }
        assert!(
            reopened.custom_surfaces.is_empty(),
            "{kind}: {:?}",
            reopened.custom_surfaces
        );
    }
}

#[test]
fn color_row_choices_from_current_and_pending_choices_lead() {
    let desk = Desk::new("current-and-pending-colors");
    two_hue_wallpaper(&desk);
    let blue = [94, 129, 172];
    let mut editor = themes_editor(&desk);
    editor.update(Message::Select(Selection::Preset("theme-soft")));
    finish_theme(&mut editor);
    choose_color(&mut editor, LookColor::Fixed(blue));
    assert_eq!(editor.update(Message::Apply), Outcome::Changed);
    drop(editor);
    let mut editor = themes_editor(&desk);
    // A wallpaper choice from Current edits the applied look, color included.
    editor.update(Message::ThemeWallpaper(true));
    finish_theme(&mut editor);
    assert_eq!(*editor.selection(), Selection::Preset("theme-soft"));
    assert_eq!(
        LookColor::of(editor.profile().unwrap()),
        Some(LookColor::Fixed(blue))
    );
    editor.update(Message::Select(Selection::Preset("theme-outline")));
    finish_theme(&mut editor);
    let p = editor.profile().unwrap();
    assert_eq!(LookColor::of(p), Some(LookColor::Fixed(blue)));
    assert_eq!(
        p.wallpaper,
        Some(profiles::themes::matching(
            crate::desktop_theme::Family::Outline
        ))
    );
    // A pending choice of the style's own color leads over the applied one.
    choose_color(&mut editor, LookColor::Style);
    editor.update(Message::Select(Selection::Preset("theme-paper")));
    finish_theme(&mut editor);
    assert_eq!(
        style_value(&editor, "/appearance/accent"),
        Some(serde_json::Value::Null)
    );
    assert_eq!(
        LookColor::of(editor.profile().unwrap()),
        Some(LookColor::Style)
    );
    // It survives recovery too, rather than turning into the applied color.
    let saved = ron::to_string(&editor.session_draft().unwrap()).unwrap();
    drop(editor);
    let mut reopened = themes_editor(&desk);
    reopened
        .restore_session(ron::from_str(&saved).unwrap())
        .unwrap();
    finish_theme(&mut reopened);
    assert_eq!(
        LookColor::of(reopened.profile().unwrap()),
        Some(LookColor::Style)
    );
    // Discarding returns to the applied color.
    reopened.update(Message::Discard);
    reopened.update(Message::Select(Selection::Preset("theme-bare")));
    finish_theme(&mut reopened);
    assert_eq!(
        LookColor::of(reopened.profile().unwrap()),
        Some(LookColor::Fixed(blue))
    );
}

#[test]
fn recovered_surface_resets_keep_their_scope_and_leave_colors_alone() {
    for domain in [Domain::WidgetStyle, Domain::Dock] {
        let desk = Desk::new(&format!("recover-reset-{}", domain.key()));
        solid_wallpaper(&desk, [0.6, 0.2, 0.5]);
        let mut editor = themes_editor(&desk);
        editor.update(Message::Select(Selection::Preset("theme-soft")));
        choose_color(&mut editor, LookColor::Wallpaper);
        assert_eq!(editor.update(Message::Apply), Outcome::Changed);
        let applied = desk.widget_config()["appearance"].clone();
        let opacity = desk.fixture.read(DOCK, "opacity");
        // Customize the surface, then choose its Reset.
        if domain == Domain::WidgetStyle {
            let mut config = desk.widget_config();
            config["appearance"]["corner_radius"] = 3.into();
            fs::write(&desk.config, config.to_string()).unwrap();
        } else {
            desk.fixture.user(DOCK, "opacity", "0.64");
        }
        editor.refresh();
        assert!(editor.custom_surfaces.contains(&domain) || domain == Domain::WidgetStyle);
        editor.update(Message::ResetSurface(domain));
        assert!(editor.resetting());
        assert!(!editor.theme_loading(), "a Reset takes no wallpaper colors");
        let reset = editor.profile().unwrap().clone();
        let scope = reset.domains();
        assert!(
            scope.contains(&domain) && !scope.contains(&Domain::Panel),
            "{scope:?}"
        );
        let saved = ron::to_string(&editor.session_draft().expect("pending")).unwrap();
        drop(editor);

        let mut reopened = themes_editor(&desk);
        reopened
            .restore_session(ron::from_str(&saved).unwrap())
            .unwrap();
        finish_theme(&mut reopened);
        assert_eq!(
            reopened.profile(),
            Some(&reset),
            "{domain:?} is restored as it was"
        );
        assert!(reopened.resetting() && !reopened.theme_loading());
        assert!(reopened.stale(), "a recovered draft still waits for review");
        reopened.update(Message::Replan);
        finish_theme(&mut reopened);
        assert_eq!(reopened.update(Message::Apply), Outcome::Changed);
        let stored = desk.widget_config()["appearance"].clone();
        if domain == Domain::WidgetStyle {
            assert_eq!(stored["corner_radius"], 25, "the Reset still resets");
        } else {
            assert_eq!(desk.fixture.read(DOCK, "opacity"), opacity);
        }
        for key in ["wallpaper_colors", "accent", "family"] {
            assert_eq!(stored[key], applied[key], "{domain:?}: {key}");
        }
        assert!(
            reopened.custom_surfaces.is_empty(),
            "{:?}",
            reopened.custom_surfaces
        );
    }
}

#[test]
fn an_outline_only_the_derived_theme_colors_stays_customized_until_windows_reset() {
    use crate::desktop::appearance::ThemeVariant;
    let desk = Desk::new("derived-only-outline");
    solid_wallpaper(&desk, [0.2, 0.4, 0.7]);
    let hint = desk
        .fixture
        .0
        .join("config/cosmic/com.system76.CosmicTheme.Dark/v2/window_hint");
    fs::create_dir_all(hint.parent().unwrap()).unwrap();
    fs::write(
        &hint,
        "Some((red:0.3882353,green:0.8156863,blue:0.8745098))",
    )
    .unwrap();
    let original = fs::read(&hint).unwrap();
    let drawn = |desk: &Desk| {
        let preview = AppearanceDraft::load(&desk.store())
            .unwrap()
            .preview(ThemeVariant::Dark);
        (preview.outline, preview.accent)
    };
    let mut editor = themes_editor(&desk);
    // Choosing styles leaves a color COSMIC draws from another tool, and
    // says so: Windows is Customized while the bars match.
    for family in ["theme-soft", "theme-paper"] {
        editor.update(Message::Select(Selection::Preset(family)));
        finish_theme(&mut editor);
        assert_eq!(editor.update(Message::Apply), Outcome::Changed);
        let (outline, accent) = drawn(&desk);
        assert_ne!(outline, accent, "{family}");
        assert_eq!(fs::read(&hint).unwrap(), original, "{family}");
        assert_eq!(
            editor.custom_surfaces,
            [Domain::Appearance].into_iter().collect(),
            "{family}"
        );
    }
    // Windows Reset relinks it, and only the window part changes.
    editor.update(Message::ResetSurface(Domain::Appearance));
    let plan = editor.plan().unwrap();
    assert!(
        plan.domains
            .iter()
            .all(|part| matches!(part.domain, Domain::Appearance | Domain::WidgetStyle))
    );
    assert!(plan.can_apply(), "{:?}", plan.blocking_reasons());
    let dock = desk.fixture.read(DOCK, "opacity");
    assert_eq!(editor.update(Message::Apply), Outcome::Changed);
    let (outline, accent) = drawn(&desk);
    assert_eq!(outline, accent, "the outline follows the accent");
    assert_eq!(desk.fixture.read(DOCK, "opacity"), dock);
    assert!(
        editor.custom_surfaces.is_empty(),
        "{:?}",
        editor.custom_surfaces
    );
    assert_eq!(editor.update(Message::Undo), Outcome::Changed);
    assert_eq!(
        fs::read(&hint).unwrap(),
        original,
        "Undo restores it exactly"
    );
}

#[test]
fn a_style_leaves_the_dock_and_panel_alone_when_asked() {
    let desk = Desk::new("style-bars");
    let mut editor = desk.editor();
    editor.update(Message::Select(Selection::Preset("theme-outline")));
    let all = editor.selected_domains();
    assert!(all.contains(&Domain::Dock) && all.contains(&Domain::Panel));
    editor.set_style_bars(false);
    let kept = editor.selected_domains();
    assert!(!kept.contains(&Domain::Dock) && !kept.contains(&Domain::Panel));
    assert!(kept.contains(&Domain::WidgetStyle) && kept.contains(&Domain::Appearance));
    assert!(editor.plan().is_some_and(|plan| {
        plan.domains
            .iter()
            .all(|part| !matches!(part.domain, Domain::Dock | Domain::Panel))
    }));
    // A whole setup still includes its dock: the option is about styles.
    editor.update(Message::Select(Selection::Preset("focus")));
    assert!(editor.selected_domains().contains(&Domain::Dock));
    editor.set_style_bars(true);
    editor.update(Message::Select(Selection::Preset("theme-outline")));
    assert!(editor.selected_domains().contains(&Domain::Dock));
}

#[test]
fn theme_matching_repair_waits_for_review_and_uses_the_existing_apply_transaction() {
    let desk = Desk::new("theme-mode-repair-review");
    let mut raw = desk.widget_config();
    raw["appearance"]["family"] = serde_json::json!(crate::desktop_theme::Family::Soft);
    fs::write(&desk.config, raw.to_string()).unwrap();
    desk.fixture
        .user(DOCK, "background", "Color((0.1,0.2,0.3))");
    let before = desk.desktop();
    let mut editor = desk.editor();
    editor.update(Message::RepairThemeMatching);
    assert_eq!(
        desk.desktop(),
        before,
        "repair selection is a read-only review"
    );
    assert!(editor.can_apply(), "{:?}", editor.notice);
    editor.update(Message::Apply);
    assert_eq!(
        desk.fixture.read(DOCK, "background").unwrap(),
        "ThemeDefault"
    );
    assert_eq!(
        desk.widget_config()["appearance"]["native_mode_palettes"],
        1
    );
    let saved = crate::desktop::recovery::load_baseline(&desk.store())
        .unwrap()
        .unwrap();
    let original = saved
        .domains
        .iter()
        .flat_map(|d| &d.native)
        .find(|r| r.namespace == DOCK && r.key_name().unwrap() == "background")
        .unwrap();
    assert_eq!(
        original.contents().unwrap().unwrap(),
        b"Color((0.1,0.2,0.3))"
    );
}

#[test]
fn review_r1_family_selection_retains_unproven_native_roles_through_recolor() {
    use crate::desktop::appearance::{AppearanceDraft, ColorRole, ThemeVariant};
    use crate::desktop::wallpaper::{self, BgColor, BgEntry, BgSource, automatic};
    for legacy in [false, true] {
        for sync in [false, true] {
            if !legacy && sync {
                continue;
            }
            let desk = Desk::new(&format!("review-r1-provenance-{legacy}-{sync}"));
            let mut raw = desk.widget_config();
            raw["appearance"]["sync_colors"] = serde_json::json!(sync);
            raw["appearance"]["color_policy"] = serde_json::json!("custom");
            raw["appearance"]["match_wallpaper"] = serde_json::json!(false);
            if legacy {
                raw["appearance"]["family"] = serde_json::json!("soft");
                desk.fixture
                    .user(DOCK, "background", "Color((0.1,0.2,0.3))");
                let path = desk
                    .fixture
                    .0
                    .join("config/cosmic/com.system76.CosmicTheme.Light.Builder/v2/bg_color");
                fs::create_dir_all(path.parent().unwrap()).unwrap();
                fs::write(path, "Some(\"#EEEEEEFF\")").unwrap();
            }
            fs::write(&desk.config, raw.to_string()).unwrap();
            let mut editor = desk.editor();
            editor.update(Message::Select(Selection::Preset("theme-paper")));
            finish_theme(&mut editor);
            assert!(editor.can_apply(), "{:?}", editor.notice);
            assert_eq!(editor.update(Message::Apply), Outcome::Changed);
            let (_, cfg) = automatic::read_config(&desk.store()).unwrap();
            assert_eq!(
                cfg.appearance.native_mode_palettes, 0,
                "legacy={legacy}, sync={sync}"
            );
            assert!(
                cfg.appearance.family.is_some(),
                "repair must remain available"
            );
            if legacy {
                assert_eq!(
                    desk.fixture.read(DOCK, "background").unwrap(),
                    "Color((0.1,0.2,0.3))"
                );
            }
            if legacy {
                assert_eq!(
                    fs::read_to_string(
                        desk.fixture.0.join(
                            "config/cosmic/com.system76.CosmicTheme.Light.Builder/v2/bg_color"
                        )
                    )
                    .unwrap(),
                    "Some(\"#EEEEEEFF\")"
                );
            }
            let before = AppearanceDraft::load(&desk.store()).unwrap();
            // Re-enable native synchronization and automatic tint, as a later
            // user action. Neither action grants ownership of missing roles.
            let mut raw = desk.widget_config();
            raw["appearance"]["sync_colors"] = serde_json::json!(true);
            raw["appearance"]["color_policy"] = serde_json::json!("wallpaper");
            raw["appearance"]["wallpaper_colors"] = serde_json::json!(true);
            raw["appearance"]["match_wallpaper"] = serde_json::json!(true);
            fs::write(&desk.config, raw.to_string()).unwrap();
            desk.fixture.user(
                wallpaper::NAMESPACE,
                "all",
                &ron::to_string(&BgEntry::new(
                    "all",
                    BgSource::Color(BgColor::Single([0.8, 0.3, 0.2])),
                ))
                .unwrap(),
            );
            automatic::commit(
                &desk.store(),
                automatic::prepare(&desk.store()).unwrap().unwrap(),
            )
            .unwrap();
            let after = AppearanceDraft::load(&desk.store()).unwrap();
            for variant in ThemeVariant::ALL {
                for role in [ColorRole::Background, ColorRole::Primary] {
                    let key = crate::desktop::appearance::theme::builder_key(
                        variant,
                        match role {
                            ColorRole::Background => "bg_color",
                            _ => "primary_container_bg",
                        },
                    );
                    assert_eq!(
                        after.snapshot().user(&key),
                        before.snapshot().user(&key),
                        "unproven {variant:?} {role:?}"
                    );
                }
            }
            if legacy {
                assert_eq!(
                    desk.fixture.read(DOCK, "background").unwrap(),
                    "Color((0.1,0.2,0.3))"
                );
            }
        }
    }
}

#[test]
fn auditfix_set02_wallpaper_only_editor_apply_keeps_colors_due_across_restart() {
    use crate::desktop::wallpaper::automatic;
    for broad in [false, true] {
        let desk = Desk::new(&format!("auditfix-wallpaper-only-{broad}"));
        solid_wallpaper(&desk, [0.02, 0.02, 0.02]);
        let mut editor = desk.editor();
        editor.update(Message::Select(Selection::Preset("theme-bare")));
        editor.update(Message::ThemeColors(true));
        finish_theme(&mut editor);
        assert_eq!(editor.update(Message::Apply), Outcome::Changed);
        assert!(automatic::prepare(&desk.store()).unwrap().is_none());
        let old = automatic::read_config(&desk.store()).unwrap().1.appearance;
        assert!(!old.wallpaper_light);

        let mut profile = Profile::new("Light wallpaper").unwrap();
        profile.wallpaper = Some(profiles::WallpaperLook {
            background: Some(profiles::BackgroundLook {
                source: Some(profiles::SourceLook::Color(profiles::Rgb::Hex(
                    "#80E8A0".into(),
                ))),
                ..Default::default()
            }),
            ..Default::default()
        });
        if broad {
            profile.widget_style = Some(serde_json::json!({"theme": "rose"}));
        }
        profile.version = profile.written_version();
        let id = ProfileLibrary::new(&desk.store()).create(&profile).unwrap();
        editor.update(Message::Select(Selection::Saved(id)));
        if broad {
            editor.update(Message::Include(Domain::WidgetStyle, false));
        }
        assert_eq!(editor.selected_domains(), [Domain::Wallpaper].into());
        assert_eq!(editor.update(Message::Apply), Outcome::Changed);
        let untouched = automatic::read_config(&desk.store()).unwrap().1.appearance;
        assert!(untouched.wallpaper_colors);
        assert_eq!(untouched.accent, old.accent);
        assert_eq!(untouched.wallpaper_light, old.wallpaper_light);

        // Read from a fresh store, as the restarted runtime does. This goes
        // through the editor boundary that the earlier engine test missed.
        let store = desk.store();
        let prepared = automatic::prepare(&store)
            .unwrap()
            .expect("new wallpaper must remain unseen");
        automatic::commit(&store, prepared).unwrap();
        let current = automatic::read_config(&store).unwrap().1.appearance;
        assert!(current.wallpaper_light);
        assert!(current.wallpaper_colors);
        assert_ne!(current.accent, old.accent);
        assert!(automatic::prepare(&store).unwrap().is_none());
    }
}

#[test]
fn auditfix_ux04_current_wallpaper_replacement_invalidates_cache_and_late_pixels() {
    let desk = Desk::new("auditfix-current-picture-version");
    let path = desk.fixture.0.join("current.png");
    let replace = |rgb| {
        // Atomic replacement even keeps the old mtime: inode identity must
        // still invalidate the cache when path, size and mtime all agree.
        let modified = fs::metadata(&path).ok().and_then(|m| m.modified().ok());
        let replacement = path.with_file_name("replacement.png");
        image::RgbImage::from_pixel(16, 16, image::Rgb(rgb))
            .save(&replacement)
            .unwrap();
        if let Some(modified) = modified {
            fs::File::options()
                .write(true)
                .open(&replacement)
                .unwrap()
                .set_modified(modified)
                .unwrap();
        }
        fs::rename(replacement, &path).unwrap();
    };
    replace([20u8, 20, 20]);
    desk.fixture.user(
        wallpaper::NAMESPACE,
        "all",
        &ron::to_string(&BgEntry::new("all", BgSource::Path(path.clone()))).unwrap(),
    );
    let mut editor = desk.editor();
    editor.take_jobs();
    editor.open_home();
    for job in editor.take_jobs() {
        editor.update(job.run());
    }
    let pixel = |editor: &ProfilesEditor| {
        let Handle::Rgba { pixels, .. } = editor.home_image().expect("current pixels") else {
            panic!("RGBA")
        };
        [pixels[0], pixels[1], pixels[2]]
    };
    assert_eq!(pixel(&editor), [20, 20, 20]);
    editor.open_home();
    assert!(editor.take_jobs().is_empty(), "unchanged version is reused");

    replace([240, 240, 240]);
    editor.open_home();
    let old_job = editor.take_jobs().pop().expect("same path, new version");
    let old_result = old_job.run();
    replace([80, 80, 80]);
    editor.update(old_result);
    assert!(
        editor.home_image().is_none(),
        "late decoded pixels must not become current"
    );
    let fresh = editor
        .take_jobs()
        .pop()
        .expect("recheck queued current version");
    editor.update(fresh.run());
    assert_eq!(pixel(&editor), [80, 80, 80]);
    assert_eq!(editor.selection(), &Selection::Current);
    editor.open_home();
    assert!(editor.take_jobs().is_empty());

    // A failed decode is cached too, but repairing the same path must
    // allow another attempt without restarting Settings.
    fs::write(&path, b"not a PNG").unwrap();
    editor.open_home();
    let broken = editor.take_jobs().pop().unwrap();
    editor.update(broken.run());
    assert!(editor.home_image().is_none());
    editor.open_home();
    assert!(editor.take_jobs().is_empty());
    replace([160, 160, 160]);
    editor.open_home();
    let repaired = editor
        .take_jobs()
        .pop()
        .expect("repaired image is a new version");
    editor.update(repaired.run());
    assert_eq!(pixel(&editor), [160, 160, 160]);
    assert!(editor.backdrops.len() <= BACKDROPS);
}

#[test]
fn fb09_backdrop_requests_coalesce_and_obsolete_work_cannot_replace_current_pixels() {
    let desk = Desk::new("fb09-backdrop-coalescing");
    let mut editor = desk.editor();
    editor.take_jobs();
    let set_background = |step: usize| {
        let entry = BgEntry::new(
            "all",
            BgSource::Color(wallpaper::BgColor::Single([step as f32 / 100., 0.2, 0.4])),
        );
        desk.fixture.user(
            wallpaper::NAMESPACE,
            "all",
            &ron::to_string(&entry).unwrap(),
        );
    };
    set_background(0);
    editor.open_home();
    let stale = editor.take_jobs().pop().unwrap();
    editor.queue_current_colors();
    let stale_colors = editor.take_theme_jobs().pop().unwrap();
    for step in 1..=100 {
        set_background(step);
        editor.open_home();
        editor.queue_current_colors();
        assert!(editor.theme_jobs.len() <= 2);
        assert!(
            editor.jobs.len() <= 2,
            "only current home and candidate jobs retained"
        );
    }
    assert!(*stale.cancel.borrow());
    assert!(*stale_colors.cancel.borrow());
    let current = editor.take_jobs().pop().unwrap();
    editor.update(stale.run());
    assert!(editor.home_image().is_none());
    editor.update(current.run());
    let image = editor.home_image().unwrap().clone();
    editor.open_home();
    assert!(editor.take_jobs().is_empty());
    assert_eq!(editor.home_image(), Some(&image));
    set_background(55);
    editor.open_home();
    let waiting = editor.take_jobs().pop().unwrap();
    drop(editor);
    assert!(
        waiting.cancel.has_changed().is_err(),
        "owner drop cancels outstanding work"
    );
}

fn mix_fixture(name: &str) -> (Desk, ProfilesEditor, ProfileId) {
    let desk = Desk::new(name);
    let mut source = Profile::new("Picture source").unwrap();
    source.wallpaper = Some(profiles::WallpaperLook {
        background: Some(profiles::BackgroundLook {
            source: Some(profiles::SourceLook::Color(profiles::Rgb::of_srgb([
                0.16, 0.27, 0.32,
            ]))),
            ..Default::default()
        }),
        ..Default::default()
    });
    let id = ProfileLibrary::new(&desk.store()).create(&source).unwrap();
    let editor = desk.editor();
    (desk, editor, id)
}
fn choose_mix(editor: &mut ProfilesEditor, picture: ProfileId) {
    use profiles::mix::Component;
    editor.update(Message::MixStart);
    editor.update(Message::MixPick(
        Component::Picture,
        Selection::Saved(picture),
    ));
    editor.update(Message::MixPick(
        Component::Style,
        Selection::Preset("paper-studio"),
    ));
    editor.update(Message::MixPick(
        Component::Layout,
        Selection::Preset("tidal"),
    ));
    finish_theme(editor);
}
#[test]
fn fb12_mix_save_is_independent_and_apply_undo_is_one_transaction() {
    let (desk, mut editor, picture) = mix_fixture("mix-transaction");
    let before = desk.desktop();
    let original = fs::read(&desk.config).unwrap();
    choose_mix(&mut editor, picture.clone());
    assert!(editor.profile().unwrap().composition.is_some());
    assert!(!editor.mix.as_ref().unwrap().choosing);
    assert_eq!(desk.desktop(), before, "preview must not apply");
    assert_eq!(
        editor
            .profile()
            .unwrap()
            .widget_style
            .as_ref()
            .unwrap()
            .pointer("/clock/face"),
        Some(&serde_json::json!("analog"))
    );
    assert!(
        editor.can_apply(),
        "{:?} {:?}",
        editor.plan(),
        editor.notice
    );
    name(&mut editor, NameAction::SaveMix, "My independent mix");
    finish_theme(&mut editor);
    assert_eq!(desk.desktop(), before, "saving must not apply");
    assert!(matches!(editor.selection(), Selection::Saved(_)));
    ProfileLibrary::new(&desk.store()).delete(&picture).unwrap();
    editor.refresh();
    finish_theme(&mut editor);
    assert_eq!(editor.profile().unwrap().name, "My independent mix");
    assert!(
        editor.can_apply(),
        "{:?} {:?}",
        editor.plan(),
        editor.notice
    );
    assert_eq!(editor.update(Message::Apply), Outcome::Changed);
    assert_eq!(desk.widget_config()["location"]["name"], "SENTINEL");
    assert_eq!(editor.latest().unwrap().owner, Owner::Profile);
    assert_eq!(editor.update(Message::Undo), Outcome::Changed);
    assert_eq!(fs::read(&desk.config).unwrap(), original);
}
#[test]
fn fb12_mix_incomplete_cancel_and_replacement_are_read_only() {
    use profiles::mix::Component;
    let (desk, mut editor, picture) = mix_fixture("mix-cancel");
    let before = desk.desktop();
    editor.update(Message::MixStart);
    editor.update(Message::MixPick(
        Component::Picture,
        Selection::Saved(picture.clone()),
    ));
    assert!(editor.profile().is_none() && !editor.can_apply());
    assert_eq!(editor.update(Message::Apply), Outcome::None);
    editor.update(Message::Discard);
    assert!(editor.mix.is_none());
    choose_mix(&mut editor, picture);
    let old = editor.profile().unwrap().clone();
    editor.update(Message::MixPick(
        Component::Style,
        Selection::Preset("carbon"),
    ));
    finish_theme(&mut editor);
    assert_eq!(editor.profile().unwrap().wallpaper, old.wallpaper);
    assert_eq!(editor.profile().unwrap().widget_layout, old.widget_layout);
    assert_ne!(editor.profile().unwrap().widget_style, old.widget_style);
    editor.update(Message::Discard);
    assert_eq!(desk.desktop(), before);
}
#[test]
fn fb12_mix_retains_conflict_and_old_color_jobs_cannot_change_new_mix() {
    use profiles::mix::Component;
    let (desk, mut editor, picture) = mix_fixture("mix-conflict");
    choose_mix(&mut editor, picture);
    editor.update(Message::MixPick(
        Component::Style,
        Selection::Preset("carbon"),
    ));
    let old = editor.take_theme_jobs();
    editor.update(Message::MixPick(
        Component::Style,
        Selection::Preset("paper-studio"),
    ));
    for job in old {
        editor.update(job.run());
    }
    assert!(editor.theme_loading());
    finish_theme(&mut editor);
    desk.fixture.user(DOCK, "opacity", "0.4");
    let changed = desk.desktop();
    assert_eq!(editor.update(Message::Apply), Outcome::None);
    assert!(editor.stale());
    assert!(editor.mix.is_some());
    assert_eq!(desk.desktop(), changed);
}

#[test]
fn fb12_saving_mix_keeps_only_included_domains() {
    let (desk, mut editor, picture) = mix_fixture("mix-selected-domains");
    desk.fixture.user(
        wallpaper::NAMESPACE,
        "all",
        &ron::to_string(&BgEntry::new(
            wallpaper::SHARED,
            BgSource::Color(wallpaper::BgColor::Single([0.2, 0.3, 0.4])),
        ))
        .unwrap(),
    );
    choose_mix(&mut editor, picture);
    editor.update(Message::Include(Domain::Wallpaper, false));
    finish_theme(&mut editor);
    let before = desk.desktop();
    name(&mut editor, NameAction::SaveMix, "Style and layout");
    assert!(matches!(editor.selection(), Selection::Saved(_)));
    assert!(editor.profile().unwrap().wallpaper.is_none());
    assert!(editor.profile().unwrap().widget_style.is_some());
    assert!(editor.profile().unwrap().widget_layout.is_some());
    assert_eq!(desk.desktop(), before);
}

#[test]
fn fb13_share_cancel_reselection_and_failure_never_write_or_open_stale_dialogs() {
    let desk = Desk::new("share-fencing");
    let mut editor = desk.editor();
    let before = desk.desktop();
    editor.update(Message::Select(Selection::Preset("carbon")));
    editor.update(Message::ShareImage);
    assert!(editor.sharing());
    let first = editor.take_share_jobs().pop().unwrap();
    editor.update(Message::ShareImage);
    assert!(
        editor.take_share_jobs().is_empty(),
        "duplicate request is coalesced"
    );
    editor.update(Message::Select(Selection::Preset("paper-studio")));
    assert!(*first.cancel.borrow());
    assert!(!editor.sharing());
    assert_eq!(
        editor.update(Message::ShareReady(
            first.id,
            Ok(b"\x89PNG\r\n\x1a\nstale".to_vec())
        )),
        Outcome::None
    );
    editor.update(Message::ShareImage);
    let second = editor.take_share_jobs().pop().unwrap();
    editor.update(Message::CancelShare);
    assert!(*second.cancel.borrow());
    editor.update(Message::ShareImage);
    let third = editor.take_share_jobs().pop().unwrap();
    editor.update(Message::ShareReady(
        third.id,
        Err("Fixture renderer failed".into()),
    ));
    assert!(!editor.busy());
    assert!(
        matches!(&editor.notice,Some(Notice::Error(error)) if error=="Fixture renderer failed")
    );
    assert_eq!(desk.desktop(), before);
}

#[test]
fn fb13_share_dialog_is_confined_and_saved_png_does_not_apply_the_setup() {
    let desk = Desk::new("share-save");
    let mut editor = desk.editor();
    editor.update(Message::Select(Selection::Preset("carbon")));
    let before = desk.desktop();
    editor.update(Message::ShareImage);
    let job = editor.take_share_jobs().pop().unwrap();
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .enable_all()
        .build()
        .unwrap();
    let id = job.id;
    let bytes = runtime.block_on(job.run()).unwrap();
    let Outcome::Dialog(Request::ExportImage {
        id,
        file_name,
        bytes,
        confine,
    }) = editor.update(Message::ShareReady(id, Ok(bytes)))
    else {
        panic!("image dialog");
    };
    assert!(file_name.ends_with("-preview.png"));
    assert_eq!(confine.as_deref(), Some(desk.fixture.0.as_path()));
    let out = desk.fixture.0.join("out");
    fs::create_dir_all(&out).unwrap();
    let target = out.join(file_name);
    write_export(&target, &bytes, confine.as_deref()).unwrap();
    editor.update(Message::Exported(id, Ok(Some(target.clone()))));
    assert_eq!(fs::read(&target).unwrap(), bytes);
    assert_eq!(desk.desktop(), before);
    assert!(!editor.busy());
    assert!(matches!(editor.notice, Some(Notice::Done(_))));
}

#[test]
fn fb13_share_detects_external_library_change_with_the_same_selection() {
    let desk = Desk::new("share-library-replacement");
    let store = desk.store();
    let library = ProfileLibrary::new(&store);
    let id = library
        .create(&profiles::preset("carbon").unwrap().profile)
        .unwrap();
    let mut editor = desk.editor();
    editor.update(Message::Select(Selection::Saved(id.clone())));
    editor.update(Message::ShareImage);
    let job = editor.take_share_jobs().pop().unwrap();
    library.rename(&id, "Changed elsewhere").unwrap();
    editor.refresh();
    assert_eq!(editor.selection(), &Selection::Saved(id));
    assert_eq!(
        editor.update(Message::ShareReady(
            job.id,
            Ok(b"\x89PNG\r\n\x1a\nstale".to_vec())
        )),
        Outcome::None
    );
    assert!(!editor.busy());
    assert!(matches!(&editor.notice,Some(Notice::Error(text)) if text.contains("setup changed")));
}
