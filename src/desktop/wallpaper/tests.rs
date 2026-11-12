use super::*;
use crate::desktop::store::tests::Fixture;
use crate::desktop::store::{Fault, Owner};
use std::fs;

/// cosmic-bg 91d525b `data/v1/all`, verbatim (note `#true`).
const SYSTEM_ALL: &str = r#"(
    output: "all",
    source: Path("/usr/share/backgrounds/cosmic/orion_nebula_nasa_heic0601a.jpg"),
    filter_by_theme: #true,
    rotation_frequency: 3600,
    filter_method: Lanczos,
    scaling_mode: Zoom,
    sampling_method: Alphanumeric,
)"#;

fn entry(output: &str, source: &str) -> String {
    format!(
        "(output: \"{output}\", source: {source}, filter_by_theme: true, rotation_frequency: 1234, filter_method: Nearest, scaling_mode: Zoom, sampling_method: Random)"
    )
}
fn path(text: &str) -> String {
    format!("Path(\"{text}\")")
}
fn seeded(name: &str, outputs: &[&str]) -> Fixture {
    let fixture = Fixture::new(name);
    fs::write(fixture.0.join("outputs"), outputs.join("\n")).unwrap();
    fixture
}
fn decoded(raw: &RawValue) -> BgEntry {
    raw.decode().unwrap()
}
fn written(plan: &Plan, key: &ConfigKey) -> BgEntry {
    decoded(plan.patch.get(key).unwrap().desired.as_ref().unwrap())
}
fn out(name: &str) -> ConfigKey {
    output_key(name).unwrap()
}
fn red() -> BgSource {
    BgSource::Color(BgColor::Single([0.8, 0.1, 0.1]))
}
fn dp1() -> Target {
    Target::Output("DP-1".into())
}

#[test]
fn edit_preserves_unused_and_untouched_entry_members() {
    let fixture = seeded("bg-preserve", &["DP-1"]);
    fixture.user(NAMESPACE, SHARED, &entry("all", &path("/pictures/old.png")));
    let mut draft = WallpaperDraft::load(&fixture.store()).unwrap();
    draft.set_source(&Target::Shared, BgSource::Path("/pictures/new.png".into()));
    let plan = draft.plan();
    assert!(plan.blocked.is_empty(), "{:?}", plan.blocked);
    assert_eq!(plan.patch.len(), 1);
    let value = written(&plan, &shared_key());
    assert_eq!(
        value,
        BgEntry {
            output: "all".into(),
            source: BgSource::Path("/pictures/new.png".into()),
            filter_by_theme: true,
            rotation_frequency: 1234,
            transition_duration: None,
            filter_method: FilterMethod::Nearest,
            scaling_mode: ScalingMode::Zoom,
            sampling_method: SamplingMethod::Random,
        }
    );
}

#[test]
fn unknown_members_or_variants_are_kept_and_block_edits() {
    let unknown = entry("all", &path("/p.png")).replace(
        "sampling_method: Random)",
        "sampling_method: Random, future: 1)",
    );
    let tile = entry("all", &path("/p.png")).replace("scaling_mode: Zoom", "scaling_mode: Tile");
    let too_long = entry("all", &path("/p.png")).replace("1234", "999999999");
    for (case, text) in [("member", unknown), ("variant", tile), ("bounds", too_long)] {
        let fixture = seeded(&format!("bg-unknown-{case}"), &["DP-1", "DP-2"]);
        fixture.user(NAMESPACE, SHARED, &text);
        fixture.user(NAMESPACE, SAME_ON_ALL, "false");
        fixture.user(NAMESPACE, BACKGROUNDS, "[\"DP-2\"]");
        fixture.user(NAMESPACE, "output.DP-2", &entry("DP-2", &path("/two.png")));
        let mut draft = WallpaperDraft::load(&fixture.store()).unwrap();
        assert!(
            matches!(draft.entry(&Target::Shared), Some(Setting::Unsupported(_))),
            "{case}"
        );
        draft.set_scaling(&Target::Shared, ScalingMode::Stretch);
        let plan = draft.plan();
        assert!(!plan.blocked.is_empty(), "{case}: shared edit is blocked");
        assert!(plan.patch.get(&shared_key()).is_none(), "{case}");
        // Another display with its own readable entry stays editable.
        draft.discard();
        draft.set_source(&Target::Output("DP-2".into()), red());
        let plan = draft.plan();
        assert!(plan.blocked.is_empty(), "{case}: {:?}", plan.blocked);
        assert_eq!(written(&plan, &out("DP-2")).source, red());
        // A display that would start from the kept shared entry can't.
        draft.discard();
        draft.set_source(&dp1(), red());
        assert!(!draft.plan().blocked.is_empty(), "{case}");
    }
    // An unreadable display entry blocks that display even when unlisted.
    let fixture = seeded("bg-unknown-output", &["DP-1"]);
    fixture.user(NAMESPACE, SAME_ON_ALL, "false");
    fixture.user(
        NAMESPACE,
        "output.DP-1",
        &entry("DP-1", &path("/p.png")).replace("Zoom", "Span"),
    );
    let mut draft = WallpaperDraft::load(&fixture.store()).unwrap();
    draft.set_source(&dp1(), red());
    let plan = draft.plan();
    assert!(plan.patch.get(&out("DP-1")).is_none());
    assert_eq!(plan.blocked[0].key, out("DP-1"));

    // A listed entry beyond the supported bounds is kept and blocked, but
    // previewed as cosmic-bg shows it (its own, not the shared one); one
    // cosmic-bg can't read falls back to the shared background there too.
    let fixture = seeded("bg-unknown-listed", &["DP-1", "DP-2"]);
    fixture.user(NAMESPACE, SHARED, &entry("all", &path("/shared.png")));
    fixture.user(NAMESPACE, SAME_ON_ALL, "false");
    fixture.user(NAMESPACE, BACKGROUNDS, "[\"DP-1\", \"DP-2\"]");
    fixture.user(
        NAMESPACE,
        "output.DP-1",
        &entry("DP-1", &path("/one.png")).replace("1234", "999999999"),
    );
    fixture.user(
        NAMESPACE,
        "output.DP-2",
        &entry("DP-2", &path("/two.png")).replace("Zoom", "Tile"),
    );
    let mut draft = WallpaperDraft::load(&fixture.store()).unwrap();
    assert!(
        matches!(draft.effective(&dp1()), Setting::Unsupported(text) if text.contains("/one.png"))
    );
    assert_eq!(
        draft
            .effective(&Target::Output("DP-2".into()))
            .known()
            .map(|entry| entry.source.clone()),
        Some(BgSource::Path("/shared.png".into()))
    );
    draft.set_scaling(&dp1(), ScalingMode::Stretch);
    assert_eq!(draft.plan().blocked[0].key, out("DP-1"));
}

#[test]
fn system_default_text_resolves_like_cosmic_bg() {
    // Observed: ron 0.12 does not accept the `#true` token, so the system
    // default does not decode and cosmic-bg's `Entry::fallback()` applies;
    // both outcomes would resolve to the same values.
    let parsed = ron::from_str::<ron::Value>(SYSTEM_ALL);
    eprintln!("ron parse of the installed default: {parsed:?}");
    assert!(parsed.is_err(), "#true parsed: {parsed:?}");
    let fixture = seeded("bg-system-default", &["eDP-1"]);
    fixture.system(NAMESPACE, SHARED, SYSTEM_ALL);
    let store = fixture.store();
    let mut draft = WallpaperDraft::load(&store).unwrap();
    assert_eq!(
        draft.entry(&Target::Shared),
        Some(Setting::Known(BgEntry::fallback()))
    );
    assert_eq!(draft.displays(), Setting::Known(Displays::Same));
    draft.set_scaling(&Target::Shared, ScalingMode::Stretch);
    let plan = draft.plan();
    assert!(plan.blocked.is_empty());
    let entry = plan.patch.get(&shared_key()).unwrap();
    assert_eq!(entry.original, None, "creates the user value");
    store.apply(&plan.patch).unwrap();
    let stored: BgEntry = RawValue::parse(fixture.read(NAMESPACE, SHARED).unwrap())
        .unwrap()
        .decode()
        .unwrap();
    assert_eq!(
        stored,
        BgEntry {
            scaling_mode: ScalingMode::Stretch,
            ..BgEntry::fallback()
        }
    );
}

#[test]
fn display_edit_writes_only_its_entry_and_the_list() {
    let fixture = seeded("bg-display-edit", &["DP-1", "eDP-1"]);
    fixture.user(NAMESPACE, SHARED, &entry("all", &path("/shared.png")));
    fixture.user(NAMESPACE, SAME_ON_ALL, "false");
    let store = fixture.store();
    let mut draft = WallpaperDraft::load(&store).unwrap();
    assert_eq!(draft.displays(), Setting::Known(Displays::Separate));
    draft.set_source(&dp1(), red());
    let plan = draft.plan();
    assert!(plan.blocked.is_empty(), "{:?}", plan.blocked);
    let keys: Vec<_> = plan.patch.keys().cloned().collect();
    assert_eq!(keys, vec![list_key(), out("DP-1")]);
    let guards: Vec<_> = plan.patch.guards().map(|(key, _)| key.clone()).collect();
    assert_eq!(guards, vec![shared_key(), same_key()]);
    let before = fixture.read(NAMESPACE, SHARED);
    store.apply(&plan.patch).unwrap();
    assert_eq!(fixture.read(NAMESPACE, SHARED), before, "shared untouched");
    assert_eq!(fixture.read(NAMESPACE, "output.eDP-1"), None);
    let list: Vec<String> = RawValue::parse(fixture.read(NAMESPACE, BACKGROUNDS).unwrap())
        .unwrap()
        .decode()
        .unwrap();
    assert_eq!(list, vec!["DP-1".to_string()]);
    let own: BgEntry = RawValue::parse(fixture.read(NAMESPACE, "output.DP-1").unwrap())
        .unwrap()
        .decode()
        .unwrap();
    assert_eq!(own.source, red());
    assert_eq!(own.rotation_frequency, 1234, "starts from the shared entry");
    let reloaded = WallpaperDraft::load(&store).unwrap();
    assert_eq!(reloaded.effective(&dp1()).known().unwrap().source, red());
    assert_eq!(
        reloaded
            .effective(&Target::Output("eDP-1".into()))
            .known()
            .unwrap()
            .source,
        BgSource::Path("/shared.png".into())
    );
}

#[test]
fn output_entries_name_their_own_display() {
    let fixture = seeded("bg-names", &["DP-1", "eDP-1"]);
    fixture.user(NAMESPACE, SHARED, &entry("all", &path("/shared.png")));
    fixture.user(NAMESPACE, SAME_ON_ALL, "false");
    // A readable unlisted leftover is replaced from the shared entry. (One
    // that names another display is kept: see
    // `mismatched_entry_names_are_kept_and_block_edits`.)
    fixture.user(NAMESPACE, "output.DP-1", &entry("DP-1", &path("/old.png")));
    let mut draft = WallpaperDraft::load(&fixture.store()).unwrap();
    draft.set_scaling(&dp1(), ScalingMode::Stretch);
    draft.set_source(&Target::Output("eDP-1".into()), red());
    draft.set_rotation(&Target::Shared, 600);
    let plan = draft.plan();
    assert!(plan.blocked.is_empty(), "{:?}", plan.blocked);
    for name in ["DP-1", "eDP-1"] {
        assert_eq!(written(&plan, &out(name)).output, name);
    }
    assert_eq!(written(&plan, &shared_key()).output, "all");
    assert_eq!(
        written(&plan, &out("DP-1")).source,
        BgSource::Path("/shared.png".into()),
        "the dormant leftover is not resurrected"
    );
}

#[test]
fn disconnected_listed_displays_are_kept() {
    let fixture = seeded("bg-disconnected", &["DP-1"]);
    fixture.user(NAMESPACE, SAME_ON_ALL, "false");
    fixture.user(NAMESPACE, BACKGROUNDS, "[\"DP-2\"]");
    fixture.user(NAMESPACE, "output.DP-2", &entry("DP-2", &path("/two.png")));
    let store = fixture.store();
    let mut draft = WallpaperDraft::load(&store).unwrap();
    let views = draft.display_views();
    let two = views.iter().find(|view| view.name == "DP-2").unwrap();
    assert!(!two.connected && two.listed);
    assert!(matches!(two.own, Some(Setting::Known(_))));
    draft.set_source(&dp1(), red());
    let plan = draft.plan();
    let list: Vec<String> = plan
        .patch
        .get(&list_key())
        .unwrap()
        .desired
        .as_ref()
        .unwrap()
        .decode()
        .unwrap();
    assert_eq!(list, vec!["DP-2".to_string(), "DP-1".to_string()]);
    let before = fixture.read(NAMESPACE, "output.DP-2");
    store.apply(&plan.patch).unwrap();
    assert_eq!(fixture.read(NAMESPACE, "output.DP-2"), before);
}

#[test]
fn same_on_all_on_clears_the_list_and_keeps_output_files() {
    let fixture = seeded("bg-same-on", &["DP-1", "eDP-1"]);
    fixture.user(NAMESPACE, SAME_ON_ALL, "false");
    fixture.user(NAMESPACE, BACKGROUNDS, "[\"DP-1\"]");
    fixture.user(NAMESPACE, "output.DP-1", &entry("DP-1", &path("/one.png")));
    let store = fixture.store();
    let mut draft = WallpaperDraft::load(&store).unwrap();
    draft.set_displays(Displays::Same);
    let plan = draft.plan();
    let keys: Vec<_> = plan.patch.keys().cloned().collect();
    assert_eq!(keys, vec![list_key(), same_key()]);
    assert_eq!(
        plan.patch
            .get(&same_key())
            .unwrap()
            .desired
            .as_ref()
            .unwrap(),
        &RawValue::parse("true").unwrap()
    );
    let list: Vec<String> = plan
        .patch
        .get(&list_key())
        .unwrap()
        .desired
        .as_ref()
        .unwrap()
        .decode()
        .unwrap();
    assert!(list.is_empty());
    let before = fixture.read(NAMESPACE, "output.DP-1");
    store.apply(&plan.patch).unwrap();
    assert_eq!(fixture.read(NAMESPACE, "output.DP-1"), before);
}

/// A draft that writes all four kinds of key: the mode turns off, the
/// shared entry changes and DP-1 gets its own entry and joins the list.
fn four_key_fixture(name: &str) -> (Fixture, DesktopPatch) {
    let fixture = seeded(name, &["DP-1"]);
    fixture.user(NAMESPACE, SHARED, &entry("all", &path("/shared.png")));
    fixture.user(NAMESPACE, SAME_ON_ALL, "true");
    let mut draft = WallpaperDraft::load(&fixture.store()).unwrap();
    draft.set_displays(Displays::Separate);
    draft.set_rotation(&Target::Shared, 600);
    draft.set_source(&dp1(), red());
    let plan = draft.plan();
    assert!(plan.blocked.is_empty(), "{:?}", plan.blocked);
    assert_eq!(plan.patch.len(), 4);
    (fixture, plan.patch)
}
fn rank_order() -> Vec<ConfigKey> {
    vec![out("DP-1"), shared_key(), list_key(), same_key()]
}

#[test]
fn cosmic_bg_keys_are_written_entries_list_then_mode() {
    for limit in 1..=3 {
        let (fixture, patch) = four_key_fixture(&format!("bg-order-{limit}"));
        let store = fixture.store();
        let originals: Vec<_> = rank_order()
            .iter()
            .map(|key| store.read_user(key).unwrap())
            .collect();
        store.inject(Some(Fault::InterruptAfter(limit)));
        assert!(store.apply(&patch).is_err());
        store.inject(None);
        for (index, key) in rank_order().iter().enumerate() {
            let now = store.read_user(key).unwrap();
            if index < limit {
                assert_eq!(
                    now.as_ref(),
                    patch.get(key).unwrap().desired.as_ref(),
                    "after {limit}: {key} written"
                );
            } else {
                assert_eq!(
                    now, originals[index],
                    "after {limit}: {key} not yet written"
                );
            }
        }
        let report = store.recover().unwrap();
        assert_eq!(report.restored, rank_order()[..limit].to_vec());
        for (index, key) in rank_order().iter().enumerate() {
            assert_eq!(store.read_user(key).unwrap(), originals[index]);
        }
    }
}

#[test]
fn interrupted_wallpaper_change_recovers_in_rank_order() {
    let (fixture, patch) = four_key_fixture("bg-journal-order");
    let store = fixture.store();
    let config = || crate::desktop::store::tests::tree(&fixture.0.join("config"));
    let before = config();
    store.inject(Some(Fault::InterruptBeforeWidget));
    assert!(store.apply(&patch).is_err());
    store.inject(None);
    let journal = store.pending_recovery().unwrap().unwrap();
    let recorded: Vec<ConfigKey> = journal
        .entries
        .iter()
        .map(|entry| ConfigKey::new(&entry.namespace, entry.version, &entry.key).unwrap())
        .collect();
    assert_eq!(recorded, rank_order());
    let report = store.recover().unwrap();
    assert_eq!(report.restored, rank_order());
    assert_eq!(config(), before, "every key restored");
}

#[test]
fn cosmic_settings_edit_during_draft_conflicts_and_keeps_other_edits() {
    let fixture = seeded("bg-outside", &["DP-1"]);
    fixture.user(NAMESPACE, SHARED, &entry("all", &path("/shared.png")));
    fixture.user(NAMESPACE, SAME_ON_ALL, "false");
    fixture.user(NAMESPACE, BACKGROUNDS, "[\"DP-1\"]");
    fixture.user(NAMESPACE, "output.DP-1", &entry("DP-1", &path("/one.png")));
    let store = fixture.store();
    let mut draft = WallpaperDraft::load(&store).unwrap();
    draft.set_source(&Target::Shared, red());
    draft.set_scaling(&dp1(), ScalingMode::Stretch);
    // COSMIC Settings changes DP-1 meanwhile.
    fixture.user(
        NAMESPACE,
        "output.DP-1",
        &entry("DP-1", &path("/other.png")),
    );
    let config = || crate::desktop::store::tests::tree(&fixture.0.join("config"));
    let files = config();
    let Err(DesktopError::Conflict(keys)) = store.apply(&draft.plan().patch) else {
        panic!("expected a conflict");
    };
    assert_eq!(keys, vec![out("DP-1")]);
    assert_eq!(config(), files, "nothing written");
    // Use current: only the DP-1 edit depended on that key.
    draft.resolve_conflicts(WallpaperDraft::load(&store).unwrap().into_snapshot(), &keys);
    assert!(draft.dirty());
    assert_eq!(
        draft
            .entry(&Target::Shared)
            .unwrap()
            .known()
            .unwrap()
            .source,
        red()
    );
    assert_eq!(
        draft.entry(&dp1()).unwrap().known().unwrap().source,
        BgSource::Path("/other.png".into())
    );
    let plan = draft.plan();
    assert_eq!(
        plan.patch.keys().cloned().collect::<Vec<_>>(),
        vec![shared_key()]
    );
    store.apply(&plan.patch).unwrap();
}

#[test]
fn system_default_change_is_stale_before_apply() {
    let fixture = seeded("bg-stale", &["DP-1"]);
    fixture.system(NAMESPACE, SHARED, &entry("all", &path("/system-a.png")));
    let store = fixture.store();
    let mut draft = WallpaperDraft::load(&store).unwrap();
    draft.set_scaling(&Target::Shared, ScalingMode::Stretch);
    assert!(
        draft
            .stale(&WallpaperDraft::load(&store).unwrap().into_snapshot())
            .is_empty()
    );
    fixture.system(NAMESPACE, SHARED, &entry("all", &path("/system-b.png")));
    let fresh = WallpaperDraft::load(&store).unwrap().into_snapshot();
    assert_eq!(draft.stale(&fresh), vec![shared_key()]);
}

#[test]
fn undo_refuses_after_outside_change() {
    let fixture = seeded("bg-undo", &["DP-1"]);
    fixture.user(NAMESPACE, SHARED, &entry("all", &path("/shared.png")));
    let store = fixture.store();
    let mut draft = WallpaperDraft::load(&store).unwrap();
    draft.set_source(&Target::Shared, red());
    let change = store
        .apply_owned(&draft.plan().patch, None, Owner::Workspaces, "Wallpaper")
        .unwrap();
    fixture.user(NAMESPACE, SHARED, &entry("all", &path("/settings.png")));
    let files = fixture.tree();
    assert!(matches!(
        store.undo_latest(&change.id),
        Err(DesktopError::Conflict(keys)) if keys == vec![shared_key()]
    ));
    assert_eq!(fixture.tree(), files);
}

#[test]
fn palette_links_conflict_with_open_wallpaper_draft() {
    use crate::system_theme::{Links, apply_links_with};
    let fixture = seeded("bg-links", &["DP-1"]);
    fixture.user(NAMESPACE, SHARED, &entry("all", &path("/shared.png")));
    let bundled = fixture.0.join("pictures/Desktop Cards");
    fs::create_dir_all(&bundled).unwrap();
    fs::write(bundled.join("teal.jpg"), b"png").unwrap();
    let store = fixture.store();
    let mut draft = WallpaperDraft::load(&store).unwrap();
    draft.set_scaling(&Target::Shared, ScalingMode::Stretch);
    let links = Links {
        wallpaper: true,
        colors: false,
    };
    apply_links_with(&store, "teal", links, &bundled).unwrap();
    draft.refresh(WallpaperDraft::load(&store).unwrap().into_snapshot());
    assert_eq!(draft.conflicts(), &[shared_key()]);
    assert!(matches!(
        store.apply(&draft.plan().patch),
        Err(DesktopError::Conflict(_))
    ));
}

#[test]
fn gradient_and_colour_validation_refuse_runtime_panics() {
    let gradient = |stops: &str, radius: &str| {
        RawValue::parse(entry(
            "all",
            &format!("Color(Gradient((colors: [{stops}], radius: {radius})))"),
        ))
        .unwrap()
    };
    let single =
        |rgb: &str| RawValue::parse(entry("all", &format!("Color(Single(({rgb})))"))).unwrap();
    for refused in [
        gradient("", "0.0"),
        gradient("(0.1, 0.2, 0.3)", "0.0"),
        gradient("(0.1, 0.2, 0.3), (0.1, 0.2, NaN)", "0.0"),
        gradient("(0.1, 0.2, 0.3), (0.4, 0.5, 0.6)", "NaN"),
        gradient("(0.1, 0.2, 0.3), (0.4, 0.5, 0.6)", "400.0"),
        single("NaN, 0.0, 0.0"),
        single("1.5, 0.0, 0.0"),
        single("-0.1, 0.0, 0.0"),
        RawValue::parse(entry("all", &path("relative.png"))).unwrap(),
        RawValue::parse(entry("", &path("/p.png"))).unwrap(),
    ] {
        assert!(check_entry(&refused).is_err(), "{}", refused.as_str());
    }
    assert!(check_entry(&gradient("(0.1, 0.2, 0.3), (0.4, 0.5, 0.6)", "90.0")).is_ok());
    assert!(check_entry(&single("0.0, 0.5, 1.0")).is_ok());
    // The store refuses such a value too.
    let fixture = seeded("bg-refuse", &[]);
    let mut patch = DesktopPatch::new();
    patch.set(shared_key(), None, Some(single("NaN, 0.0, 0.0")));
    assert!(matches!(
        fixture.store().apply(&patch),
        Err(DesktopError::InvalidValue { .. })
    ));
    assert_eq!(fixture.read(NAMESPACE, SHARED), None);
}

#[test]
fn wildcard_rule_matches_only_valid_output_keys() {
    use crate::desktop::store::rule_for;
    assert!(rule_for(&out("DP-1")).is_some());
    assert!(rule_for(&out("eDP-1.2")).is_some());
    assert!(ConfigKey::new(NAMESPACE, 1, "output.").is_err());
    for refused in [
        ConfigKey::new(NAMESPACE, 1, "outputs").unwrap(),
        ConfigKey::new(NAMESPACE, 2, "output.DP-1").unwrap(),
        ConfigKey::new("com.system76.CosmicPanel", 1, "output.DP-1").unwrap(),
        ConfigKey::new("com.system76.CosmicBackgroundX", 1, "output.DP-1").unwrap(),
    ] {
        assert!(rule_for(&refused).is_none(), "{refused}");
    }
    // One display wildcard per background namespace (shared, dark, light).
    let wildcards: Vec<_> = crate::desktop::rules()
        .filter(|rule| rule.key.contains('*'))
        .map(|rule| match rule.namespace {
            crate::desktop::store::Namespace::Exact(namespace) => (namespace, rule.key),
            crate::desktop::store::Namespace::Entry(prefix) => (prefix, "entry"),
        })
        .collect();
    let expected: Vec<_> = super::modes::NAMESPACES
        .iter()
        .map(|namespace| (*namespace, "output.*"))
        .collect();
    assert_eq!(wildcards, expected);
}

#[test]
fn existing_domains_keep_key_order() {
    let fixture = seeded("bg-other-order", &[]);
    let store = fixture.store();
    let dock = "com.system76.CosmicPanel.Dock";
    let mode = "com.system76.CosmicTheme.Mode";
    let keys = [
        ConfigKey::new(mode, 1, "is_dark").unwrap(),
        ConfigKey::new(dock, 1, "anchor").unwrap(),
        ConfigKey::new(mode, 1, "auto_switch").unwrap(),
        ConfigKey::new(dock, 1, "opacity").unwrap(),
        ConfigKey::new(dock, 1, "size").unwrap(),
    ];
    let values = ["true", "Bottom", "false", "0.5", "M"];
    let mut patch = DesktopPatch::new();
    for (key, value) in keys.iter().zip(values) {
        assert_eq!(crate::desktop::write_rank(key), 0);
        patch.set(key.clone(), None, Some(RawValue::parse(value).unwrap()));
    }
    store.inject(Some(Fault::InterruptBeforeWidget));
    assert!(store.apply(&patch).is_err());
    store.inject(None);
    let recorded: Vec<ConfigKey> = store
        .pending_recovery()
        .unwrap()
        .unwrap()
        .entries
        .iter()
        .map(|entry| ConfigKey::new(&entry.namespace, entry.version, &entry.key).unwrap())
        .collect();
    let mut sorted = keys.to_vec();
    sorted.sort();
    assert_eq!(recorded, sorted);
    store.recover().unwrap();
}

/// Pictures inside a fixture root, so source checks pass.
fn pictures(fixture: &Fixture, names: &[&str]) -> PathBuf {
    let dir = fixture.0.join("pictures");
    fs::create_dir_all(&dir).unwrap();
    for name in names {
        fs::write(dir.join(name), b"png").unwrap();
    }
    dir
}
fn guards(plan: &Plan) -> Vec<ConfigKey> {
    plan.patch.guards().map(|(key, _)| key.clone()).collect()
}

#[test]
fn private_data_dirs_are_never_trusted_system_roots() {
    use std::os::unix::fs::symlink;
    let fixture = Fixture::new("bg-trusted-roots");
    let root = &fixture.0;
    let home = root.join("home");
    let data_home = root.join("xdg-data");
    let system = root.join("sys/share");
    let flatpak = home.join(".local/share/flatpak/exports/share");
    for dir in [
        home.join(".local/share/backgrounds"),
        flatpak.join("backgrounds"),
        data_home.join("backgrounds"),
        system.join("backgrounds"),
    ] {
        fs::create_dir_all(dir).unwrap();
    }
    fs::write(home.join(".local/share/backgrounds/me.png"), b"png").unwrap();
    fs::write(system.join("backgrounds/a.png"), b"png").unwrap();
    // Aliases into the private trees, and a home reached through a link.
    symlink(home.join(".local/share"), root.join("sys/alias")).unwrap();
    symlink(&data_home, root.join("opt")).unwrap();
    symlink(&home, root.join("homelink")).unwrap();
    let data_dirs = [
        flatpak.clone(),
        home.join(".local/share"),
        root.join("homelink/.local/share"),
        data_home.clone(),
        root.join("sys/alias"),
        root.join("opt"),
        system.clone(),
        PathBuf::from("/usr/share"),
        PathBuf::from("relative/share"),
    ]
    .map(|dir| dir.display().to_string())
    .join(":");
    let private = [root.join("homelink"), data_home.clone()];
    let dirs = trusted_background_dirs(Some(&data_dirs), &private);
    assert_eq!(
        dirs,
        vec![
            system.join("backgrounds"),
            PathBuf::from("/usr/share/backgrounds")
        ]
    );
    assert_eq!(
        trusted_background_dirs(None, &private),
        vec![
            PathBuf::from("/usr/local/share/backgrounds"),
            PathBuf::from("/usr/share/backgrounds")
        ]
    );
    let bundled = root.join("pictures/Desktop Cards");
    for personal in [
        home.join(".local/share/backgrounds/me.png"),
        root.join("sys/alias/backgrounds/me.png"),
    ] {
        assert_eq!(
            classify(&personal, &bundled, &dirs),
            Class::Personal,
            "{}",
            personal.display()
        );
    }
    assert_eq!(
        classify(&system.join("backgrounds/a.png"), &bundled, &dirs),
        Class::System("a.png".into())
    );
}

#[test]
fn folder_walk_bounds_each_listing_before_collecting_it() {
    let fixture = Fixture::new("bg-walk-bounds");
    let limits = Limits {
        depth: WALK_DEPTH,
        entries: 10,
        dirs: WALK_DIRS,
    };
    let big = fixture.0.join("big");
    fs::create_dir_all(&big).unwrap();
    for index in 0..60 {
        fs::write(big.join(format!("{index:02}.txt")), "text").unwrap();
    }
    let (walk, taken) = walk_with(&big, limits, |_| true).unwrap();
    assert!(walk.truncated);
    assert!(taken <= limits.entries + 1, "took {taken} entries");
    // Across folders the budget is shared: 2 + 4 + at most 5 of 60.
    let tree = fixture.0.join("tree");
    fs::create_dir_all(tree.join("a")).unwrap();
    fs::create_dir_all(tree.join("b")).unwrap();
    for index in 0..4 {
        fs::write(tree.join(format!("a/{index}.txt")), "text").unwrap();
    }
    for index in 0..60 {
        fs::write(tree.join(format!("b/{index:02}.txt")), "text").unwrap();
    }
    let (walk, taken) = walk_with(&tree, limits, |_| true).unwrap();
    assert!(walk.truncated);
    assert!(taken <= limits.entries + 1, "took {taken} entries");
}

#[test]
fn folder_too_large_to_check_is_not_reported_as_empty() {
    let fixture = Fixture::new("bg-walk-truncated");
    let limits = Limits {
        depth: WALK_DEPTH,
        entries: 10,
        dirs: WALK_DIRS,
    };
    let big = fixture.0.join("big");
    fs::create_dir_all(&big).unwrap();
    for index in 0..60 {
        fs::write(big.join(format!("{index:02}.txt")), "text").unwrap();
    }
    let error = folder_has_image_with(&big, limits).unwrap_err();
    assert!(error.contains("too large to check"), "{error}");
    // Within the bounds the answers are unchanged.
    let small = fixture.0.join("small");
    fs::create_dir_all(small.join("sub")).unwrap();
    fs::write(small.join("notes.txt"), "text").unwrap();
    fs::write(small.join("sub/b.JPG"), "jpeg").unwrap();
    assert_eq!(folder_has_image_with(&small, limits), Ok(true));
    let empty = fixture.0.join("empty");
    fs::create_dir_all(&empty).unwrap();
    assert_eq!(folder_has_image_with(&empty, limits), Ok(false));
    fs::remove_file(small.join("sub/b.JPG")).unwrap();
    assert_eq!(folder_has_image_with(&small, limits), Ok(false));
    // A depth bound reached without a picture is not "no pictures" either.
    let deep = fixture.0.join("deep");
    fs::create_dir_all(deep.join("1/2/3")).unwrap();
    fs::write(deep.join("1/2/3/c.png"), "png").unwrap();
    let shallow = Limits { depth: 2, ..limits };
    assert!(folder_has_image_with(&deep, shallow).is_err());
    assert_eq!(folder_has_image(&deep), Ok(true));
}

/// Same → Separate shows each listed display's own entry again (and the
/// shared one where a listed display has none): those are guarded and
/// rechecked, and an outside change to one refuses the prepared patch.
#[test]
fn separate_guards_the_entries_it_shows() {
    let fixture = seeded("bg-deps-separate", &["DP-1", "DP-2", "eDP-1"]);
    let dir = pictures(&fixture, &["shared.png", "one.png", "two.png"]);
    let file = |name: &str| path(&dir.join(name).display().to_string());
    fixture.user(NAMESPACE, SHARED, &entry("all", &file("shared.png")));
    fixture.user(NAMESPACE, SAME_ON_ALL, "true");
    fixture.user(NAMESPACE, BACKGROUNDS, "[\"DP-1\", \"DP-2\"]");
    fixture.user(NAMESPACE, "output.DP-1", &entry("DP-1", &file("one.png")));
    let store = fixture.store();
    let mut draft = WallpaperDraft::load(&store).unwrap();
    draft.set_displays(Displays::Separate);
    assert_eq!(draft.check_sources(), Ok(()));
    let plan = draft.plan();
    assert!(plan.blocked.is_empty(), "{:?}", plan.blocked);
    assert_eq!(plan.patch.keys().cloned().collect::<Vec<_>>(), [same_key()]);
    assert_eq!(
        guards(&plan),
        [shared_key(), list_key(), out("DP-1"), out("DP-2")]
    );
    fixture.user(NAMESPACE, "output.DP-1", &entry("DP-1", &file("two.png")));
    let fresh = WallpaperDraft::load(&store).unwrap().into_snapshot();
    assert_eq!(draft.stale(&fresh), [out("DP-1")]);
    let config = || crate::desktop::store::tests::tree(&fixture.0.join("config"));
    let files = config();
    assert!(matches!(
        store.apply(&plan.patch),
        Err(DesktopError::Conflict(keys)) if keys == [out("DP-1")]
    ));
    assert_eq!(config(), files, "nothing written");
}

/// Separate → Same makes the shared entry visible where a display showed
/// its own.
#[test]
fn same_guards_the_shared_entry_it_shows() {
    let fixture = seeded("bg-deps-same", &["DP-1"]);
    let dir = pictures(&fixture, &["shared.png", "one.png", "other.png"]);
    let file = |name: &str| path(&dir.join(name).display().to_string());
    fixture.user(NAMESPACE, SHARED, &entry("all", &file("shared.png")));
    fixture.user(NAMESPACE, SAME_ON_ALL, "false");
    fixture.user(NAMESPACE, BACKGROUNDS, "[\"DP-1\"]");
    fixture.user(NAMESPACE, "output.DP-1", &entry("DP-1", &file("one.png")));
    let store = fixture.store();
    let mut draft = WallpaperDraft::load(&store).unwrap();
    draft.set_displays(Displays::Same);
    let plan = draft.plan();
    assert_eq!(
        plan.patch.keys().cloned().collect::<Vec<_>>(),
        [list_key(), same_key()]
    );
    assert_eq!(guards(&plan), [shared_key()]);
    fixture.user(NAMESPACE, SHARED, &entry("all", &file("other.png")));
    let fresh = WallpaperDraft::load(&store).unwrap().into_snapshot();
    assert_eq!(draft.stale(&fresh), [shared_key()]);
    let config = || crate::desktop::store::tests::tree(&fixture.0.join("config"));
    let files = config();
    assert!(matches!(
        store.apply(&plan.patch),
        Err(DesktopError::Conflict(keys)) if keys == [shared_key()]
    ));
    assert_eq!(config(), files, "nothing written");
}

/// Use shared reads the shared entry, not the other displays: a change to
/// an unrelated display (even one with a missing picture) doesn't block.
#[test]
fn use_shared_guards_the_shared_entry_only() {
    let fixture = seeded("bg-deps-shared-only", &["DP-1", "eDP-1"]);
    let dir = pictures(&fixture, &["shared.png", "one.png", "other.png"]);
    let file = |name: &str| path(&dir.join(name).display().to_string());
    let shared = entry("all", &file("shared.png"));
    fixture.user(NAMESPACE, SHARED, &shared);
    fixture.user(NAMESPACE, SAME_ON_ALL, "false");
    fixture.user(NAMESPACE, BACKGROUNDS, "[\"DP-1\", \"eDP-1\"]");
    fixture.user(NAMESPACE, "output.DP-1", &entry("DP-1", &file("one.png")));
    fixture.user(
        NAMESPACE,
        "output.eDP-1",
        &entry("eDP-1", &path("/missing.png")),
    );
    let store = fixture.store();
    let mut draft = WallpaperDraft::load(&store).unwrap();
    draft.use_shared("DP-1");
    assert_eq!(draft.check_sources(), Ok(()));
    let plan = draft.plan();
    assert!(plan.blocked.is_empty(), "{:?}", plan.blocked);
    assert_eq!(plan.patch.keys().cloned().collect::<Vec<_>>(), [list_key()]);
    assert_eq!(guards(&plan), [shared_key(), same_key()]);
    fixture.user(
        NAMESPACE,
        "output.eDP-1",
        &entry("eDP-1", &path("/missing-2.png")),
    );
    let fresh = || WallpaperDraft::load(&store).unwrap().into_snapshot();
    assert!(draft.stale(&fresh()).is_empty());
    fixture.user(NAMESPACE, SHARED, &entry("all", &file("other.png")));
    assert_eq!(draft.stale(&fresh()), [shared_key()]);
    assert!(matches!(
        store.apply(&plan.patch),
        Err(DesktopError::Conflict(keys)) if keys == [shared_key()]
    ));
    fixture.user(NAMESPACE, SHARED, &shared);
    store.apply(&plan.patch).unwrap();
}

/// cosmic-bg places an entry on the display its `output` member names, so a
/// stored entry naming another display (or a shared entry not naming
/// `all`) is kept as it is, not silently retargeted by an edit.
#[test]
fn mismatched_entry_names_are_kept_and_block_edits() {
    let fixture = seeded("bg-mismatch", &["DP-1", "DP-2", "HDMI-1", "eDP-1"]);
    fixture.user(NAMESPACE, SHARED, &entry("all", &path("/shared.png")));
    fixture.user(NAMESPACE, SAME_ON_ALL, "false");
    fixture.user(NAMESPACE, BACKGROUNDS, "[\"DP-1\", \"DP-2\"]");
    let wrong = entry("HDMI-1", &path("/one.png"));
    fixture.user(NAMESPACE, "output.DP-1", &wrong);
    fixture.user(NAMESPACE, "output.DP-2", &entry("DP-2", &path("/two.png")));
    // An unlisted leftover naming another display is kept too.
    let leftover = entry("DP-9", &path("/old.png"));
    fixture.user(NAMESPACE, "output.eDP-1", &leftover);
    let store = fixture.store();
    let mut draft = WallpaperDraft::load(&store).unwrap();
    assert!(matches!(draft.entry(&dp1()), Some(Setting::Unsupported(_))));
    // HDMI-1 shows DP-1's file at runtime: not confidently the shared one.
    assert!(matches!(
        draft.effective(&Target::Output("HDMI-1".into())),
        Setting::Unsupported(_)
    ));
    for target in [dp1(), Target::Output("eDP-1".into())] {
        draft.discard();
        draft.set_scaling(&target, ScalingMode::Stretch);
        let plan = draft.plan();
        let Target::Output(name) = &target else {
            unreachable!()
        };
        assert!(plan.patch.get(&out(name)).is_none(), "{name}");
        assert!(
            plan.blocked.iter().any(|blocked| blocked.key == out(name)),
            "{name}: {:?}",
            plan.blocked
        );
    }
    // Independent valid entries stay usable; the kept files are untouched.
    draft.discard();
    draft.set_source(&Target::Output("DP-2".into()), red());
    draft.set_rotation(&Target::Shared, 600);
    let plan = draft.plan();
    assert!(plan.blocked.is_empty(), "{:?}", plan.blocked);
    store.apply(&plan.patch).unwrap();
    assert_eq!(fixture.read(NAMESPACE, "output.DP-1").unwrap(), wrong);
    assert_eq!(fixture.read(NAMESPACE, "output.eDP-1").unwrap(), leftover);

    // A shared entry not naming `all`, stored or as the system default.
    for layer in ["user", "system"] {
        let fixture = seeded(&format!("bg-mismatch-shared-{layer}"), &["DP-1"]);
        let text = entry("elsewhere", &path("/shared.png"));
        match layer {
            "user" => fixture.user(NAMESPACE, SHARED, &text),
            _ => fixture.system(NAMESPACE, SHARED, &text),
        }
        let mut draft = WallpaperDraft::load(&fixture.store()).unwrap();
        assert!(
            matches!(draft.entry(&Target::Shared), Some(Setting::Unsupported(_))),
            "{layer}"
        );
        draft.set_rotation(&Target::Shared, 600);
        let plan = draft.plan();
        assert!(plan.patch.is_empty(), "{layer}");
        assert_eq!(plan.blocked[0].key, shared_key(), "{layer}");
    }
}

/// A display list this version can't rewrite (here a duplicate) still
/// works at runtime, so a listed display's preview is not claimed to be the
/// shared background. Shared edits stay usable; the list is untouched.
#[test]
fn unsupported_display_list_is_not_previewed_as_shared() {
    let fixture = seeded("bg-unsupported-list", &["DP-1", "eDP-1"]);
    fixture.user(NAMESPACE, SHARED, &entry("all", &path("/shared.png")));
    fixture.user(NAMESPACE, SAME_ON_ALL, "false");
    let list = "[\"DP-1\", \"DP-1\"]";
    fixture.user(NAMESPACE, BACKGROUNDS, list);
    fixture.user(NAMESPACE, "output.DP-1", &entry("DP-1", &path("/one.png")));
    let store = fixture.store();
    let mut draft = WallpaperDraft::load(&store).unwrap();
    assert!(matches!(draft.effective(&dp1()), Setting::Unsupported(_)));
    assert!(matches!(draft.entry(&dp1()), Some(Setting::Unsupported(_))));
    assert_eq!(
        draft
            .effective(&Target::Output("eDP-1".into()))
            .known()
            .map(|entry| entry.source.clone()),
        Some(BgSource::Path("/shared.png".into()))
    );
    draft.set_source(&Target::Shared, red());
    let plan = draft.plan();
    assert!(plan.blocked.is_empty(), "{:?}", plan.blocked);
    assert_eq!(
        plan.patch.keys().cloned().collect::<Vec<_>>(),
        [shared_key()]
    );
    store.apply(&plan.patch).unwrap();
    assert_eq!(fixture.read(NAMESPACE, BACKGROUNDS).unwrap(), list);
}
