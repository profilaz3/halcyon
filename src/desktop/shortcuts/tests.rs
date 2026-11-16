//! Shortcut identity, default/custom merging, faithful rewriting and the
//! paused-command lifecycle. Every test writes only below its own fixture.
use super::*;
use crate::desktop::store::rule_for;
use crate::desktop::store::tests::Fixture;
use crate::desktop::{DesktopStore, Owner};

const DEFAULTS: &str = r#"{
    // COSMIC's own defaults, comments included.
    (modifiers: [Super], key: "t"): System(Terminal),
    (modifiers: [Super], key: "q"): Close,
    (modifiers: [Super, Shift], key: "Prior"): Workspace(1),
    (modifiers: [Super], key: "m"): Maximize,
}"#;

fn draft(store: &DesktopStore) -> ShortcutsDraft {
    ShortcutsDraft::load(store).unwrap()
}

fn apply(store: &DesktopStore, draft: &ShortcutsDraft) -> crate::desktop::LatestChange {
    let plan = draft.plan();
    assert!(plan.blocked.is_empty(), "{:?}", plan.blocked);
    store
        .apply_owned(&plan.patch, None, Owner::Shortcuts, "Shortcuts")
        .unwrap()
}

fn row<'a>(rows: &'a [Row], combo: &str) -> &'a Row {
    let ident = Binding::parse(combo).unwrap().ident();
    rows.iter()
        .find(|row| row.ident() == ident && !row.unused)
        .unwrap_or_else(|| panic!("{combo}: {rows:#?}"))
}

/// Identity is modifiers + keysym: modifier order, keysym aliases and
/// names don't make a new shortcut; a custom entry shadows its default and
/// `Disable` suppresses it. Renaming keeps the binding and every other
/// entry's text, including modifier-only, Super-only and keycode entries.
#[test]
fn shortcut_identity_and_defaults_are_preserved() {
    let fixture = Fixture::new("shortcuts-identity");
    fixture.system(NAMESPACE, "defaults", DEFAULTS);
    let custom = r#"{
    (modifiers: [Shift, Super], key: "Page_Up", description: Some("Old name")): Spawn("sentinel-a --flag"),
    (modifiers: [Super], key: "q"): Disable,
    (modifiers: [Ctrl, Alt]): Maximize,
    (modifiers: [Super]): System(Launcher),
    (modifiers: [Super, Alt], keycode: Some(38), description: Some("Hardware")): Spawn("sentinel-hw"),
}"#;
    fixture.user(NAMESPACE, "custom", custom);
    let store = fixture.store();
    let mut draft = draft(&store);
    let rows = draft.rows();

    // Normalized order and the Prior/Page_Up alias are one identity, which
    // shadows the default and names what would come back.
    let alias = Binding::parse("super+shift+Prior").unwrap();
    assert_eq!(alias, Binding::parse("Shift+Super+Page_Up").unwrap());
    let shadow = row(&rows, "Super+Shift+Prior");
    assert_eq!(shadow.name, "Old name");
    assert_eq!(shadow.default.as_deref(), Some("Go to workspace 1"));
    assert_eq!(
        rows.iter()
            .filter(|row| row.ident() == alias.ident())
            .count(),
        1,
        "the shadowed default isn't listed twice"
    );
    // Single characters are their lowercase keysym; T and t are one key here.
    assert_eq!(
        Binding::parse("Super+T").unwrap(),
        Binding::parse("super+t").unwrap()
    );
    assert!(Binding::parse_new("Super+Shift").is_err());
    assert!(
        Binding::parse_new("k").is_err(),
        "a plain letter can't stand alone"
    );
    assert!(Binding::parse_new("XF86AudioMute").is_ok());
    assert!(Binding::parse_new("Super").is_ok());
    assert!(Binding::parse("Super+Nonsense").is_err());

    // Disable suppresses the default; a keycode binding is labelled and kept.
    assert_eq!(
        row(&rows, "Super+Q").kind,
        Kind::Builtin {
            label: "Close window".into(),
            on: false
        }
    );
    let hardware = rows.iter().find(|row| row.keycode()).unwrap();
    assert_eq!(hardware.binding.keycode, Some(38));
    assert_eq!(hardware.binding.to_string(), "Super+Alt");
    // COSMIC ignores only the keycode: this one still runs on Super+Alt
    // (modifier-only), so the note must not say it is never matched.
    assert!(KEYCODE_NOTE.contains("ignored") && KEYCODE_NOTE.contains("still run"));
    assert!(!KEYCODE_NOTE.contains("not matched"));
    assert!(matches!(
        draft.save_command(
            Some(hardware.target()),
            "Renamed",
            &hardware.binding,
            "x",
            Choice::default()
        ),
        Err(Ask::Refused(_))
    ));
    // Modifier-only and Super-only entries are listed, not flattened.
    assert_eq!(
        row(&rows, "Super").kind,
        Kind::Override {
            label: "Open launcher".into()
        }
    );
    let partial = Binding::parse_partial("Ctrl+Alt").unwrap();
    assert!(rows.iter().any(|row| row.ident() == partial.ident()));

    // Renaming keeps the identity and every untouched entry byte for byte.
    draft
        .save_command(
            Some(Target::Entry(alias.ident())),
            "New name",
            &alias,
            "sentinel-a --flag",
            Choice::default(),
        )
        .unwrap();
    let written = draft.plan();
    let text = written
        .patch
        .get(&custom_key())
        .and_then(|entry| entry.desired.clone())
        .unwrap();
    for kept in [
        r#"(modifiers: [Super], key: "q"): Disable"#,
        r#"(modifiers: [Ctrl, Alt]): Maximize"#,
        r#"(modifiers: [Super]): System(Launcher)"#,
        r#"(modifiers: [Super, Alt], keycode: Some(38), description: Some("Hardware")): Spawn("sentinel-hw")"#,
    ] {
        assert!(text.as_str().contains(kept), "{kept}\n{}", text.as_str());
    }
    let reread = format::parse_map(text.as_str()).unwrap();
    assert_eq!(reread.len(), 5);
    assert_eq!(reread[0].binding, alias);
    assert_eq!(reread[0].binding.description.as_deref(), Some("New name"));
    assert!(
        written
            .patch
            .guards()
            .any(|(key, _)| *key == defaults_key())
    );

    // A taken combination asks; Replace shadows the default.
    let terminal = Binding::parse("Super+T").unwrap();
    assert_eq!(
        draft.save_command(None, "Shell", &terminal, "sentinel-b", Choice::default()),
        Err(Ask::Replace {
            combo: "Super+T".into(),
            current: "Open terminal".into()
        })
    );
    draft
        .save_command(
            None,
            "Shell",
            &terminal,
            "sentinel-b",
            Choice {
                replace: true,
                ..Choice::default()
            },
        )
        .unwrap();
    let change = apply(&store, &draft);
    let fresh = ShortcutsDraft::load(&store).unwrap();
    let rows = fresh.rows();
    assert_eq!(row(&rows, "Super+T").name, "Shell");
    assert_eq!(
        row(&rows, "Super+T").default.as_deref(),
        Some("Open terminal")
    );
    assert_eq!(row(&rows, "Super+Shift+Prior").name, "New name");
    store.undo_latest(&change.id).unwrap();
    assert_eq!(fixture.read(NAMESPACE, "custom").as_deref(), Some(custom));

    // Removing a shadowing command asks before the default comes back.
    let mut draft = ShortcutsDraft::load(&store).unwrap();
    assert_eq!(
        draft.remove(Target::Entry(alias.ident()), Choice::default()),
        Err(Ask::Resume {
            combo: "Super+Shift+Prior".into(),
            default: "Go to workspace 1".into()
        })
    );
    draft
        .remove(
            Target::Entry(alias.ident()),
            Choice {
                resume: Some(false),
                ..Choice::default()
            },
        )
        .unwrap();
    assert_eq!(
        row(&draft.rows(), "Super+Shift+Prior").kind,
        Kind::Builtin {
            label: "Go to workspace 1".into(),
            on: false
        },
        "kept off, not silently restored"
    );
    draft.set_default(alias.ident(), true).unwrap();
    assert!(
        row(&draft.rows(), "Super+Shift+Prior").kind
            == Kind::Builtin {
                label: "Go to workspace 1".into(),
                on: true
            }
    );
}

/// Unknown actions keep their text through rewrites and count as off.
/// Unknown binding fields, malformed text, RON extensions and duplicate
/// combinations are never rewritten silently; the reason is given, and a
/// duplicate can be resolved by removing one chosen copy.
#[test]
fn unknown_shortcut_actions_survive_or_block_safely() {
    let fixture = Fixture::new("shortcuts-unknown");
    fixture.system(NAMESPACE, "defaults", DEFAULTS);
    fixture.user(
        NAMESPACE,
        "custom",
        r#"{(modifiers: [Super], key: "u"): FutureAction(3, "x"), (modifiers: [Super], key: "e", description: Some("Edit")): Spawn("sentinel-e")}"#,
    );
    let store = fixture.store();
    let mut draft = draft(&store);
    assert_eq!(
        row(&draft.rows(), "Super+U").kind,
        Kind::Unknown {
            raw: r#"FutureAction(3, "x")"#.into()
        }
    );
    let edit = Binding::parse("Super+E").unwrap();
    draft
        .set_running(edit.ident(), false, Choice::default())
        .unwrap();
    let plan = draft.plan();
    let text = plan
        .patch
        .get(&custom_key())
        .unwrap()
        .desired
        .clone()
        .unwrap();
    assert!(
        text.as_str().contains(r#"FutureAction(3, "x")"#),
        "{}",
        text.as_str()
    );
    assert!((rule_for(&custom_key()).unwrap().validate)(&text).is_ok());

    // Upstream refuses unknown binding fields for the whole map; so do we,
    // and nothing is planned or treated as empty.
    for (case, body) in [
        (
            "unknown field",
            r#"{(modifiers: [Super], key: "x", repeat: true): Close}"#,
        ),
        ("malformed", r#"{(modifiers: [Super], key: "x"): Close"#),
        (
            "extensions",
            "#![enable(implicit_some)]\n{(modifiers: [Super], key: \"x\", description: \"n\"): Close}",
        ),
    ] {
        fixture.user(NAMESPACE, "custom", body);
        let mut draft = ShortcutsDraft::load(&store).unwrap();
        assert!(draft.custom_problem().is_some(), "{case}");
        let free = Binding::parse("Super+Z").unwrap();
        assert!(
            matches!(
                draft.save_command(None, "Zed", &free, "z", Choice::default()),
                Err(Ask::Refused(_))
            ),
            "{case}"
        );
        assert!(!draft.dirty() && draft.plan().patch.is_empty(), "{case}");
        // COSMIC ignores an unreadable custom list: the defaults are what runs.
        if case != "extensions" {
            assert_eq!(
                row(&draft.rows(), "Super+Q").kind,
                Kind::Builtin {
                    label: "Close window".into(),
                    on: true
                },
                "{case}"
            );
        }
    }
    assert!(
        (rule_for(&custom_key()).unwrap().validate)(
            &RawValue::parse(r#"{(modifiers: [Super], key: "x", repeat: true): Close}"#).unwrap()
        )
        .is_err()
    );

    // Two entries for one combination: both shown, edits refused until one
    // chosen copy is removed; nothing collapses them.
    fixture.user(
        NAMESPACE,
        "custom",
        r#"{(modifiers: [Super], key: "d", description: Some("One")): Spawn("a"), (modifiers: [Super], key: "d", description: Some("Two")): Spawn("b")}"#,
    );
    let mut draft = ShortcutsDraft::load(&store).unwrap();
    let dups: Vec<_> = draft.rows().into_iter().filter(|r| r.duplicate).collect();
    assert_eq!(dups.len(), 2);
    let d = Binding::parse("Super+D").unwrap();
    assert!(matches!(
        draft.set_running(d.ident(), false, Choice::default()),
        Err(Ask::Refused(reason)) if reason.contains("more than once")
    ));
    draft.remove_copy(0).unwrap();
    assert_eq!(draft.custom().unwrap().len(), 1);
    assert_eq!(row(&draft.rows(), "Super+D").name, "Two");
    assert!(
        draft
            .set_running(d.ident(), false, Choice::default())
            .is_ok()
    );

    // Unreadable paused state blocks pausing, not unrelated additions.
    fixture.user(
        NAMESPACE,
        "custom",
        r#"{(modifiers: [Super], key: "e", description: Some("Edit")): Spawn("sentinel-e")}"#,
    );
    fixture.user(
        PAUSED_NAMESPACE,
        "disabled_commands",
        r#"[(binding: (modifiers: [Super], key: "p"), command: "x", future: 1)]"#,
    );
    let mut draft = ShortcutsDraft::load(&store).unwrap();
    assert!(matches!(
        draft.set_running(edit.ident(), false, Choice::default()),
        Err(Ask::Refused(_))
    ));
    let free = Binding::parse("Super+Shift+Z").unwrap();
    draft
        .save_command(None, "Zed", &free, "z", Choice::default())
        .unwrap();
    let plan = draft.plan();
    assert!(
        plan.patch.get(&paused_key()).is_none(),
        "paused state untouched"
    );
    assert!(plan.patch.guards().any(|(key, _)| *key == paused_key()));
}

/// Pausing keeps the command in the app's own key in the same transaction
/// as the native `Disable`; Undo restores both. A paused binding another
/// tool took over is only resumed through an explicit Replace.
#[test]
fn paused_commands_keep_their_command_and_undo_together() {
    let fixture = Fixture::new("shortcuts-paused");
    fixture.system(NAMESPACE, "defaults", DEFAULTS);
    let custom = r#"{(modifiers: [Super, Ctrl], key: "k", description: Some("Mail")): Spawn("sentinel-mail --x")}"#;
    fixture.user(NAMESPACE, "custom", custom);
    let store = fixture.store();
    let mail = Binding::parse("Super+Ctrl+K").unwrap();
    let mut draft = draft(&store);
    draft
        .set_running(mail.ident(), false, Choice::default())
        .unwrap();
    let change = apply(&store, &draft);
    assert_eq!(change.change.entries().len(), 2, "both keys in one change");
    let reopened = ShortcutsDraft::load(&store).unwrap();
    assert_eq!(
        row(&reopened.rows(), "Super+Ctrl+K").kind,
        Kind::Command {
            command: "sentinel-mail --x".into(),
            running: false,
            displaced: None
        }
    );
    assert_eq!(row(&reopened.rows(), "Super+Ctrl+K").name, "Mail");
    store.undo_latest(&change.id).unwrap();
    assert_eq!(fixture.read(NAMESPACE, "custom").as_deref(), Some(custom));
    assert_eq!(fixture.read(PAUSED_NAMESPACE, "disabled_commands"), None);

    // Pause again, then rename and re-key it while paused: both follow.
    let mut draft = ShortcutsDraft::load(&store).unwrap();
    draft
        .set_running(mail.ident(), false, Choice::default())
        .unwrap();
    apply(&store, &draft);
    let mut draft = ShortcutsDraft::load(&store).unwrap();
    let moved = Binding::parse("Super+Ctrl+J").unwrap();
    draft
        .save_command(
            Some(Target::Paused(mail.ident())),
            "Mail app",
            &moved,
            "sentinel-mail --x",
            Choice::default(),
        )
        .unwrap();
    let paused = draft.paused().unwrap();
    assert_eq!(paused.len(), 1);
    assert_eq!(paused[0].binding, moved);
    assert_eq!(paused[0].binding.description.as_deref(), Some("Mail app"));
    let custom_now = draft.custom().unwrap();
    assert_eq!(custom_now.len(), 1);
    assert!(custom_now[0].is_disable() && custom_now[0].binding == moved);
    apply(&store, &draft);

    // Another tool replaces the paused binding: resuming asks first.
    fixture.user(
        NAMESPACE,
        "custom",
        r#"{(modifiers: [Super, Ctrl], key: "j"): Minimize}"#,
    );
    let mut draft = ShortcutsDraft::load(&store).unwrap();
    let rows = draft.rows();
    let paused_row = rows
        .iter()
        .find(|row| matches!(row.kind, Kind::Command { running: false, .. }))
        .unwrap();
    assert_eq!(
        paused_row.kind,
        Kind::Command {
            command: "sentinel-mail --x".into(),
            running: false,
            displaced: Some("Minimize window".into())
        }
    );
    assert_eq!(
        draft.set_running(moved.ident(), true, Choice::default()),
        Err(Ask::Replace {
            combo: "Super+Ctrl+J".into(),
            current: "Minimize window".into()
        })
    );
    assert!(!draft.dirty(), "cancel leaves nothing");
    draft
        .set_running(
            moved.ident(),
            true,
            Choice {
                replace: true,
                ..Choice::default()
            },
        )
        .unwrap();
    assert!(draft.paused().unwrap().is_empty());
    assert_eq!(
        draft.custom().unwrap()[0].command(),
        Some("sentinel-mail --x")
    );
    let change = apply(&store, &draft);
    assert_eq!(
        fixture
            .read(PAUSED_NAMESPACE, "disabled_commands")
            .as_deref(),
        Some("[]")
    );
    store.undo_latest(&change.id).unwrap();
    assert!(
        fixture
            .read(PAUSED_NAMESPACE, "disabled_commands")
            .unwrap()
            .contains("sentinel-mail")
    );
}

/// Saving an edit form unchanged is no edit, so nothing waits for Apply.
#[test]
fn unchanged_command_edit_is_not_an_edit() {
    let fixture = Fixture::new("shortcuts-noop");
    fixture.user(
        NAMESPACE,
        "custom",
        r#"{
    (
        modifiers: [Super, Ctrl],
        key: "k",
        description: Some("Mail"),
    ): Spawn("mail"),
}"#,
    );
    let store = fixture.store();
    let mut draft = draft(&store);
    let mail = Binding::parse("Super+Ctrl+K").unwrap();
    draft
        .save_command(
            Some(Target::Entry(mail.ident())),
            "Mail",
            &mail,
            "mail",
            Choice::default(),
        )
        .unwrap();
    assert!(!draft.dirty());
    assert!(draft.plan().patch.is_empty());
}

/// BAR-04: a built-in action moves to other keys the way COSMIC Settings
/// does it: the custom list runs the same action on the new keys and turns
/// the old keys off. Keys in use ask first; Undo brings everything back.
#[test]
fn builtin_actions_move_to_other_keys() {
    let fixture = Fixture::new("shortcuts-rebind");
    fixture.system(NAMESPACE, "defaults", DEFAULTS);
    let store = fixture.store();
    let mut draft = draft(&store);
    let terminal = Binding::parse("Super+T").unwrap();
    let new = Binding::parse("Super+Alt+T").unwrap();
    draft
        .rebind_builtin(terminal.ident(), &new, Choice::default())
        .unwrap();
    let rows = draft.rows();
    let moved = row(&rows, "Super+Alt+T");
    assert_eq!(
        moved.kind,
        Kind::Override {
            label: "Open terminal".into()
        }
    );
    assert!(moved.custom);
    let old = row(&rows, "Super+T");
    assert_eq!(
        old.kind,
        Kind::Builtin {
            label: "Open terminal".into(),
            on: false
        }
    );
    let custom = draft.custom().unwrap();
    assert_eq!(custom.len(), 2);
    assert_eq!(custom[0].action_raw, "System(Terminal)");
    assert!(custom[1].is_disable());
    let before = fixture.read(NAMESPACE, "custom");
    let change = apply(&store, &draft);
    let written = fixture.read(NAMESPACE, "custom").unwrap();
    assert!(written.contains("System(Terminal)"), "{written}");
    assert!(written.contains("Disable"), "{written}");
    // COSMIC reads it back as the same two entries.
    let read = format::parse_map(&written).unwrap();
    assert_eq!(read.len(), 2);
    assert_eq!(read[0].ident(), new.ident());
    // Keys another shortcut uses ask before replacing it.
    let mut draft = ShortcutsDraft::load(&store).unwrap();
    let close = Binding::parse("Super+Q").unwrap();
    let maximize = Binding::parse("Super+M").unwrap();
    assert_eq!(
        draft.rebind_builtin(maximize.ident(), &close, Choice::default()),
        Err(Ask::Replace {
            combo: "Super+Q".into(),
            current: "Close window".into(),
        })
    );
    assert!(!draft.dirty());
    // The same keys change nothing, and a plain letter isn't keys.
    draft
        .rebind_builtin(maximize.ident(), &maximize, Choice::default())
        .unwrap();
    assert!(!draft.dirty());
    assert!(matches!(
        draft.rebind_builtin(
            maximize.ident(),
            &Binding::parse("k").unwrap(),
            Choice::default()
        ),
        Err(Ask::Refused(_))
    ));
    // Undo puts the defaults back as they were.
    store.undo_latest(&change.id).unwrap();
    assert_eq!(fixture.read(NAMESPACE, "custom"), before);
}
