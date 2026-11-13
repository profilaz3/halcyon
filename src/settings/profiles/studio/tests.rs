//! Look studio behavior. Every test writes only below its own temporary
//! fixture root: the desktop, the library and built-in looks are all inside.
use super::*;
use crate::desktop::profiles::SourceLook;
use crate::desktop::store::tests::Fixture;
use std::collections::BTreeMap;

const DOCK: &str = "com.system76.CosmicPanel.Dock";
const PREFERENCES: &str = r#"{"theme":"teal","location":{"name":"SENTINEL","latitude":1.0,"longitude":2.0},"appearance":{"opacity":92}}"#;

struct Desk {
    fixture: Fixture,
    config: PathBuf,
}

impl Desk {
    fn new(name: &str) -> Self {
        let fixture = Fixture::new(&format!("studio-{name}"));
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
    fn editor(&self) -> ProfilesEditor {
        ProfilesEditor::with_store(self.fixture.store().with_app_config(self.config.clone()))
    }
    /// Every desktop, widget and Undo file: all but the library and the
    /// built-in looks folder.
    fn desktop(&self) -> BTreeMap<PathBuf, Vec<u8>> {
        let library = self.fixture.0.join("state/profiles");
        let built = self.fixture.0.join(".local");
        self.fixture
            .tree()
            .into_iter()
            .filter(|(path, _)| !path.starts_with(&library) && !path.starts_with(&built))
            .filter_map(|(path, bytes)| Some((path, bytes?)))
            .filter(|(_, bytes)| !bytes.is_empty())
            .collect()
    }
}

fn studio(editor: &mut ProfilesEditor, edit: Edit) -> Outcome {
    editor.update(Message::Studio(edit))
}

fn draft(editor: &ProfilesEditor) -> &Draft {
    editor.studio.as_ref().expect("the studio is open")
}

fn saved(editor: &ProfilesEditor) -> Vec<(ProfileId, Profile)> {
    let store = editor.store().unwrap();
    ProfileLibrary::new(store)
        .list()
        .unwrap()
        .into_iter()
        .map(|entry| (entry.id, entry.profile.unwrap()))
        .collect()
}

/// Runs the wallpaper color jobs a draft queued, as the app does.
fn finish_theme(editor: &mut ProfilesEditor) {
    for job in editor.take_theme_jobs() {
        editor.update(job.run());
    }
}

/// A look as the library stores it: in the oldest format that holds it.
fn stored(profile: &Profile) -> Profile {
    let mut profile = profile.clone();
    profile.version = 0;
    profile
}

fn bundled(profile: &Profile) -> Option<String> {
    match profile
        .wallpaper
        .as_ref()?
        .background
        .as_ref()?
        .source
        .as_ref()?
    {
        SourceLook::Bundled(name) => Some(name.clone()),
        _ => None,
    }
}

#[test]
fn studio_edits_change_only_the_draft_until_apply() {
    let desk = Desk::new("draft-only");
    let before = desk.desktop();
    let mut editor = desk.editor();
    studio(&mut editor, Edit::Start(Start::Desktop));
    let opened = draft(&editor).profile.clone();
    for (element, pick) in [
        (Element::Wallpaper, Pick::Wallpaper("halcyon-calm")),
        (Element::Weather, Pick::Weather("snow")),
        (Element::Colors, Pick::Accent([0xe0, 0x50, 0x70])),
        (Element::Colors, Pick::Mode(ModeName::Light)),
        (Element::Windows, Pick::Corners(2)),
        (Element::Windows, Pick::Gaps(10, 14)),
        (Element::Font, Pick::Font("Fira Sans".into())),
        (
            Element::Dock,
            Pick::Look(Selection::Preset("presentation".into())),
        ),
    ] {
        studio(&mut editor, Edit::Open(Some(element)));
        studio(&mut editor, Edit::Pick(element, pick));
    }
    studio(&mut editor, Edit::Name("Studio draft".into()));
    studio(&mut editor, Edit::Description("Calm and snowy".into()));
    studio(&mut editor, Edit::Undo);
    studio(&mut editor, Edit::Redo);
    studio(&mut editor, Edit::Open(None));
    let edited = &draft(&editor).profile;
    assert_ne!(*edited, opened);
    assert_eq!(bundled(edited).as_deref(), Some("halcyon-calm"));
    assert_eq!(edited.name, "Studio draft");
    assert!(
        editor.plan().is_some(),
        "the draft is planned for the preview"
    );
    assert_eq!(desk.desktop(), before, "the desktop is untouched");
    assert!(saved(&editor).is_empty(), "nothing was saved");

    // Reviewing shows the parts; only Apply writes.
    studio(&mut editor, Edit::Review(true));
    assert!(draft(&editor).reviewing);
    finish_theme(&mut editor);
    assert_eq!(desk.desktop(), before, "reviewing writes nothing");
    // Fixtures have no installed wallpapers; the parts picker leaves the
    // wallpaper out, as a person would.
    editor.update(Message::Include(Domain::Wallpaper, false));
    assert!(editor.can_apply());
    assert_eq!(editor.update(Message::Apply), Outcome::Changed);
    assert_ne!(desk.desktop(), before, "Apply writes the draft");

    // Closing leaves the studio without touching anything else.
    let applied = desk.desktop();
    let mut editor = desk.editor();
    studio(&mut editor, Edit::Start(Start::Desktop));
    studio(
        &mut editor,
        Edit::Pick(Element::Weather, Pick::Weather("rain")),
    );
    studio(&mut editor, Edit::Close);
    assert!(editor.studio.is_none());
    assert_eq!(desk.desktop(), applied);
}

#[test]
fn each_picker_sets_exactly_its_element() {
    let desk = Desk::new("pickers");
    let mut editor = desk.editor();
    studio(&mut editor, Edit::Start(Start::Plain));
    let mut checked = 0;
    for element in Element::ALL {
        studio(&mut editor, Edit::Open(Some(element)));
        let choices = editor.studio_choices().to_vec();
        assert!(!choices.is_empty(), "{element:?} has choices");
        assert!(
            choices.iter().any(|choice| choice.pick == Pick::Desktop),
            "{element:?} offers your desktop"
        );
        let before = draft(&editor).profile.clone();
        for choice in &choices {
            let after = editor
                .picked(element, &choice.pick)
                .unwrap_or_else(|| panic!("{element:?} {:?}", choice.pick));
            assert_eq!(after, choice.profile, "{element:?} {:?}", choice.pick);
            assert_eq!(after.name, before.name);
            assert_eq!(after.description, before.description);
            for other in Element::ALL.into_iter().filter(|other| *other != element) {
                assert!(
                    other.same(&before, &after),
                    "{element:?} {:?} changed {other:?}",
                    choice.pick
                );
            }
            match &choice.pick {
                Pick::Desktop => assert!(element.same(&after, draft(&editor).desktop())),
                Pick::Look(selection) => {
                    let source = editor.look(selection).unwrap();
                    assert!(element.same(&after, source), "{element:?} {selection:?}");
                }
                Pick::Leave => assert!(!element.has(&after)),
                _ => {}
            }
            checked += 1;
        }
        // Picking through the editor keeps to the element, too.
        let last = choices.last().unwrap().pick.clone();
        studio(&mut editor, Edit::Pick(element, last.clone()));
        let after = draft(&editor).profile.clone();
        for other in Element::ALL.into_iter().filter(|other| *other != element) {
            assert!(
                other.same(&before, &after),
                "{element:?} {last:?} {other:?}"
            );
        }
    }
    assert!(checked > 40, "{checked} choices checked");
}

#[test]
fn from_my_desktop_takes_the_current_value() {
    let desk = Desk::new("from-desktop");
    let mut editor = desk.editor();
    studio(&mut editor, Edit::Start(Start::Plain));
    for element in [
        Element::Dock,
        Element::Panel,
        Element::Font,
        Element::Colors,
    ] {
        studio(&mut editor, Edit::Pick(element, Pick::Desktop));
        let draft = draft(&editor);
        assert!(element.same(&draft.profile, draft.desktop()), "{element:?}");
    }
    let dock = draft(&editor).profile.dock.clone().expect("a dock");
    assert_eq!(
        serde_json::to_value(&dock).unwrap()["size"],
        "L",
        "the desktop's dock size"
    );
    // A change to the desktop after the studio opened is not taken.
    desk.fixture.user(DOCK, "size", "S");
    studio(&mut editor, Edit::Pick(Element::Dock, Pick::Desktop));
    assert_eq!(draft(&editor).profile.dock, Some(dock));
}

#[test]
fn undo_redo_and_discard_move_only_the_draft() {
    let desk = Desk::new("undo");
    let mut editor = desk.editor();
    studio(&mut editor, Edit::Start(Start::Plain));
    let opened = draft(&editor).profile.clone();
    assert!(!draft(&editor).can_undo());
    studio(
        &mut editor,
        Edit::Pick(Element::Weather, Pick::Weather("fog")),
    );
    studio(&mut editor, Edit::Pick(Element::Windows, Pick::Corners(2)));
    let edited = draft(&editor).profile.clone();
    studio(&mut editor, Edit::Undo);
    studio(&mut editor, Edit::Undo);
    assert_eq!(draft(&editor).profile, opened);
    assert!(draft(&editor).can_redo());
    studio(&mut editor, Edit::Redo);
    studio(&mut editor, Edit::Redo);
    assert_eq!(draft(&editor).profile, edited);
    assert!(draft(&editor).dirty());
    studio(&mut editor, Edit::Discard);
    assert_eq!(draft(&editor).profile, opened);
    assert!(!draft(&editor).dirty());
    // Discard is one step back, too.
    studio(&mut editor, Edit::Undo);
    assert_eq!(draft(&editor).profile, edited);
}

#[test]
fn save_save_as_rename_duplicate_and_delete_go_through_the_library() {
    let desk = Desk::new("library");
    let before = desk.desktop();
    let mut editor = desk.editor();
    studio(&mut editor, Edit::Start(Start::Desktop));
    studio(&mut editor, Edit::Name("Studio look".into()));
    studio(&mut editor, Edit::Save);
    let first = draft(&editor).saved().expect("saved as a new look");
    assert!(!draft(&editor).dirty());
    let looks = saved(&editor);
    assert_eq!(looks.len(), 1);
    assert_eq!(looks[0].0, first);
    assert_eq!(stored(&looks[0].1), stored(&draft(&editor).profile));

    // Save writes the edits into the same look.
    studio(
        &mut editor,
        Edit::Pick(Element::Weather, Pick::Weather("rain")),
    );
    assert!(draft(&editor).dirty());
    studio(&mut editor, Edit::Save);
    assert!(!draft(&editor).dirty());
    let looks = saved(&editor);
    assert_eq!(looks.len(), 1);
    assert_eq!(stored(&looks[0].1), stored(&draft(&editor).profile));

    // Save as new keeps the first look and edits the new one.
    studio(&mut editor, Edit::Name("Second look".into()));
    studio(&mut editor, Edit::SaveAsNew);
    let second = draft(&editor).saved().unwrap();
    assert_ne!(second, first);
    let names: Vec<String> = saved(&editor).into_iter().map(|(_, p)| p.name).collect();
    assert_eq!(names, ["Second look", "Studio look"]);

    // Rename renames the saved look, keeping its other parts as saved.
    studio(&mut editor, Edit::Pick(Element::Windows, Pick::Corners(2)));
    studio(&mut editor, Edit::Name("Renamed look".into()));
    assert!(draft(&editor).renamed());
    studio(&mut editor, Edit::Rename);
    let stored = ProfileLibrary::new(editor.store().unwrap())
        .load(&second)
        .unwrap();
    assert_eq!(stored.name, "Renamed look");
    assert!(Element::Windows.same(&stored, &looks[0].1));
    assert!(draft(&editor).dirty(), "the corner change is still unsaved");

    // Duplicate stores a copy with the unsaved edits and edits the copy.
    studio(&mut editor, Edit::Duplicate);
    let copy = draft(&editor).saved().unwrap();
    assert!(copy != first && copy != second);
    assert_eq!(draft(&editor).profile.name, "Renamed look copy");
    assert!(!draft(&editor).dirty());
    assert_eq!(saved(&editor).len(), 3);

    // Delete asks first, then removes only the look being edited.
    studio(&mut editor, Edit::Delete(false));
    assert!(draft(&editor).confirm_delete);
    assert_eq!(saved(&editor).len(), 3, "asking deletes nothing");
    studio(&mut editor, Edit::Delete(true));
    assert!(editor.studio.is_none(), "the studio closes");
    let left: Vec<ProfileId> = saved(&editor).into_iter().map(|(id, _)| id).collect();
    assert!(left.contains(&first) && left.contains(&second) && !left.contains(&copy));

    // A saved look opens for editing as itself.
    studio(&mut editor, Edit::Start(Start::Saved(first.clone())));
    assert_eq!(draft(&editor).saved(), Some(first));
    assert_eq!(draft(&editor).profile.name, "Studio look");
    assert!(!draft(&editor).dirty());
    assert_eq!(
        desk.desktop(),
        before,
        "the library never touches the desktop"
    );
}

#[test]
fn a_curated_look_opens_as_a_copy() {
    let desk = Desk::new("curated");
    let mut editor = desk.editor();
    editor.update(Message::Select(Selection::Preset(profiles::CLEAN.into())));
    let name = editor.profile().unwrap().name.clone();
    studio(&mut editor, Edit::Start(Start::Selected));
    assert_eq!(draft(&editor).saved(), None);
    assert_eq!(draft(&editor).profile.name, format!("{name} copy"));
    studio(&mut editor, Edit::Save);
    assert!(draft(&editor).saved().is_some(), "Save stores a new look");
    assert_eq!(saved(&editor).len(), 1);
}

#[test]
fn make_built_in_refuses_personal_wallpapers_and_writes_a_clean_file() {
    let desk = Desk::new("built-in");
    let before = desk.desktop();
    let mut editor = desk.editor();
    studio(&mut editor, Edit::Start(Start::Desktop));
    studio(&mut editor, Edit::Name("Studio harbor".into()));
    let mut personal = draft(&editor).profile.clone();
    personal.wallpaper = Some(
        serde_json::from_value(json!({
            "background": {"source": {"image": "/home/someone/Pictures/cat.jpg"}, "fit": "fill"}
        }))
        .unwrap(),
    );
    editor.studio.as_mut().unwrap().profile = personal;
    studio(&mut editor, Edit::MakeBuiltIn);
    let folder = desk.fixture.0.join(".local/share/halcyon/built-in-looks");
    assert!(matches!(
        draft(&editor).built,
        Some(Built::Refused {
            wallpaper: true,
            ..
        })
    ));
    assert!(!folder.exists(), "nothing written");
    assert!(matches!(
        studio(&mut editor, Edit::MakeBuiltInAs),
        Outcome::None
    ));

    // A Halcyon wallpaper makes it shippable; personal data stays out.
    studio(
        &mut editor,
        Edit::Pick(Element::Wallpaper, Pick::Wallpaper("halcyon-ridge")),
    );
    assert_eq!(draft(&editor).built, None, "a new edit clears the result");
    studio(&mut editor, Edit::MakeBuiltIn);
    let Some(Built::Written { path, .. }) = draft(&editor).built.clone() else {
        panic!("expected a written file, got {:?}", draft(&editor).built);
    };
    assert_eq!(path, folder.join("studio-harbor.json"));
    let text = fs::read_to_string(&path).unwrap();
    assert!(!text.contains("SENTINEL") && !text.contains("/home/"));
    let shipped = profiles::parse_built_in("studio-harbor", &text).unwrap();
    assert_eq!(shipped.name, "Studio harbor");
    assert_eq!(bundled(&shipped).as_deref(), Some("halcyon-ridge"));
    assert_eq!(
        studio(&mut editor, Edit::CopyPath),
        Outcome::Copy(path.display().to_string())
    );

    // Make built-in as a file asks where, inside the fixture.
    match studio(&mut editor, Edit::MakeBuiltInAs) {
        Outcome::Dialog(Request::Export {
            id,
            file_name,
            bytes,
            confine,
        }) => {
            assert_eq!(file_name, "studio-harbor.json");
            assert_eq!(String::from_utf8(bytes).unwrap(), text);
            assert_eq!(confine.as_deref(), Some(desk.fixture.0.as_path()));
            let chosen = desk.fixture.0.join("chosen.json");
            editor.update(Message::Exported(id, Ok(Some(chosen.clone()))));
            assert!(matches!(
                &draft(&editor).built,
                Some(Built::Written { path, .. }) if *path == chosen
            ));
        }
        other => panic!("expected an export dialog, got {other:?}"),
    }
    assert_eq!(desk.desktop(), before, "the desktop is untouched");
}

#[test]
fn the_draft_survives_a_parked_preview() {
    let desk = Desk::new("parked");
    let mut editor = desk.editor();
    studio(&mut editor, Edit::Start(Start::Plain));
    studio(
        &mut editor,
        Edit::Pick(Element::Weather, Pick::Weather("snow")),
    );
    studio(&mut editor, Edit::Open(Some(Element::Weather)));
    let draft_before = draft(&editor).clone();
    let json = serde_json::to_string(&draft_before).unwrap();
    let back: Draft = serde_json::from_str(&json).unwrap();
    assert_eq!(back, draft_before);
    assert!(back.validate().is_ok());
}
