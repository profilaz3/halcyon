//! The Automation page's editor against isolated fixture stores.
use super::*;
use crate::desktop::store::tests::Fixture;

fn editor(name: &str) -> (Fixture, AutomationEditor) {
    let fixture = Fixture::new(name);
    std::fs::write(fixture.0.join("outputs"), "eDP-1\nDP-2\n").unwrap();
    std::fs::write(fixture.0.join("output-names"), "DP-2=Dell U2720Q\n").unwrap();
    std::fs::write(fixture.0.join("ports"), "HDMI-A-1\nDP-2\n").unwrap();
    let editor = AutomationEditor::with_store(fixture.store());
    (fixture, editor)
}

fn saved(fixture: &Fixture) -> Automation {
    automation::load(&fixture.store()).unwrap()
}

#[test]
fn opening_reads_only_and_offers_every_profile_and_display() {
    let (fixture, editor) = editor("auto-open");
    assert!(!fixture.0.join("state").exists());
    assert_eq!(editor.settings().unwrap(), &Automation::default());
    let names: Vec<&str> = editor
        .profiles
        .iter()
        .map(|(_, name)| name.as_str())
        .collect();
    assert_eq!(
        names,
        [
            "Everyday",
            "Slow morning",
            "After hours",
            "Deep work",
            "Writing desk",
            "Studio",
            "Tidal",
            "Paper",
            "Moss",
            "Carbon",
            "Clean desktop",
            "Tidal · Day",
            "Tidal · Evening",
            "Tidal · Night",
            "Paper · Day",
            "Paper · Evening",
            "Paper · Night",
            "Moss · Day",
            "Moss · Evening",
            "Moss · Night",
            "Day moment",
            "Night moment"
        ]
    );
    let choices = editor.display_choices();
    assert_eq!(
        choices,
        [
            ("DP-2".to_string(), "Dell U2720Q · DP-2".to_string(), true),
            ("eDP-1".to_string(), "eDP-1".to_string(), true),
            ("HDMI-A-1".to_string(), "HDMI-A-1".to_string(), false),
        ]
    );
}

#[test]
fn the_first_rule_turns_automation_on_and_later_edits_restart_it() {
    let (fixture, mut editor) = editor("auto-first");
    editor.update(Message::New(Kind::Schedule), true);
    assert!(editor.dirty());
    assert_eq!(editor.draft.as_ref().unwrap().time, "19:00");
    // Save waits for a profile, with a reason, and writes nothing.
    editor.update(Message::Save, true);
    assert!(matches!(&editor.notice, Some(Notice::Error(text)) if text == "Choose a profile."));
    assert!(!fixture.0.join("state").exists());
    editor.update(Message::Draft(DraftEdit::Time("7 pm".into())), true);
    editor.update(Message::Draft(DraftEdit::Days(Days::WEEKDAYS)), true);
    editor.update(
        Message::Draft(DraftEdit::Profile(ProfileRef::Preset("evening".into()))),
        true,
    );
    let before = automation::now();
    editor.update(Message::Save, true);
    assert!(editor.draft.is_none() && !editor.dirty());
    let settings = saved(&fixture);
    assert!(settings.enabled && settings.since >= before);
    assert_eq!(settings.rules.len(), 1);
    let rule = &settings.rules[0];
    assert_eq!(rule.name, "After hours");
    assert_eq!(rule.when.summary(true), "Weekdays at 19:00");
    assert!(rule.enabled && rule.since >= before);
    assert!(
        matches!(&editor.notice, Some(Notice::Done(text)) if text.ends_with("automation is on"))
    );
    // Opening an existing rule is not an unsaved change; Cancel writes nothing.
    let file = std::fs::read(automation::path(&fixture.store(), automation::FILE)).unwrap();
    editor.update(Message::Edit("r1".into()), true);
    assert!(!editor.dirty());
    editor.update(Message::Draft(DraftEdit::Day(5)), true);
    assert!(editor.dirty());
    editor.update(Message::Cancel, true);
    assert_eq!(
        std::fs::read(automation::path(&fixture.store(), automation::FILE)).unwrap(),
        file
    );
    // A second rule doesn't touch the switch; turned off, it stays off.
    editor.update(Message::Enable(false), true);
    editor.update(Message::New(Kind::Display), true);
    editor.update(
        Message::Draft(DraftEdit::Display(Some("DP-2".into()))),
        true,
    );
    editor.update(
        Message::Draft(DraftEdit::Profile(ProfileRef::Preset("halcyon".into()))),
        true,
    );
    editor.update(Message::Save, true);
    let settings = saved(&fixture);
    assert!(!settings.enabled);
    assert_eq!(settings.rules[1].when.summary(true), "Dell U2720Q connects");
}

#[test]
fn switches_order_pause_and_delete_save_at_once() {
    let (fixture, mut editor) = editor("auto-controls");
    for (kind, preset) in [(Kind::Schedule, "focus"), (Kind::Display, "presentation")] {
        editor.update(Message::New(kind), true);
        editor.update(
            Message::Draft(DraftEdit::Profile(ProfileRef::Preset(preset.into()))),
            true,
        );
        editor.update(Message::Save, true);
    }
    let since = saved(&fixture).rules[0].since;
    editor.update(Message::Rule("r1".into(), false), true);
    assert!(!saved(&fixture).rules[0].enabled);
    editor.update(Message::Rule("r1".into(), true), true);
    assert!(saved(&fixture).rules[0].since >= since);
    editor.update(Message::Move("r2".into(), -1), true);
    let order: Vec<String> = saved(&fixture)
        .rules
        .iter()
        .map(|rule| rule.id.clone())
        .collect();
    assert_eq!(order, ["r2", "r1"]);
    // The top rule can't move further up.
    editor.update(Message::Move("r2".into(), -1), true);
    assert_eq!(saved(&fixture).rules[0].id, "r2");
    editor.update(Message::Pause(PauseFor::Hour), true);
    let pause = saved(&fixture).paused.unwrap();
    assert_eq!(pause.until.unwrap() - pause.since, 3600);
    editor.update(Message::Pause(PauseFor::Resume), true);
    assert_eq!(saved(&fixture).paused.unwrap().until, None);
    let before = automation::now();
    editor.update(Message::Resume, true);
    let settings = saved(&fixture);
    assert!(settings.paused.is_none() && settings.since >= before);
    // A skip paused nothing: undoing it restarts nothing either.
    automation::update(&fixture.store(), |settings| {
        settings.since = 1;
        Ok(())
    })
    .unwrap();
    editor.update(Message::Pause(PauseFor::Next), true);
    assert!(saved(&fixture).paused.unwrap().skip);
    editor.update(Message::Resume, true);
    let skipped = saved(&fixture);
    assert!(skipped.paused.is_none());
    assert_eq!(skipped.since, 1);
    // Delete asks first.
    editor.update(Message::Delete("r1".into(), false), true);
    assert_eq!(saved(&fixture).rules.len(), 2);
    editor.update(Message::Delete("r1".into(), true), true);
    assert_eq!(saved(&fixture).rules.len(), 1);
}

#[test]
fn a_deleted_profile_is_named_on_its_rule() {
    let (fixture, mut editor) = editor("auto-deleted");
    let store = fixture.store();
    let profile = profiles::preset("evening").unwrap().profile;
    let mut copy = profile.clone();
    copy.name = "Evening".into();
    let id = ProfileLibrary::new(&store).create(&copy).unwrap();
    editor.refresh();
    editor.update(Message::New(Kind::Schedule), true);
    editor.update(
        Message::Draft(DraftEdit::Profile(ProfileRef::Saved(id.as_str().into()))),
        true,
    );
    editor.update(Message::Save, true);
    let rule = saved(&fixture).rules[0].clone();
    assert_eq!(editor.rule_problem(&rule), None);
    ProfileLibrary::new(&store).delete(&id).unwrap();
    editor.refresh();
    assert_eq!(
        editor.rule_problem(&rule).as_deref(),
        Some("“Evening” was deleted")
    );
    assert_eq!(editor.profile_name(&rule.profile, &rule.name), "Evening");
}

#[test]
fn settings_hold_is_written_only_while_automation_runs() {
    let (fixture, mut editor) = editor("auto-hold");
    let store = fixture.store();
    let hold = automation::path(&store, automation::HOLD_FILE);
    let dock: BTreeSet<Domain> = [Domain::Dock].into_iter().collect();
    // Off: a draft holds nothing, and nothing is created.
    editor.sync_hold(dock.clone());
    assert!(!fixture.0.join("state").exists());
    editor.update(Message::New(Kind::Schedule), true);
    editor.update(
        Message::Draft(DraftEdit::Profile(ProfileRef::Preset("evening".into()))),
        true,
    );
    editor.update(Message::Save, true);
    editor.sync_hold(dock.clone());
    assert_eq!(automation::read_hold(&store), dock);
    // Applied or discarded: released.
    editor.sync_hold(BTreeSet::new());
    assert!(!hold.exists());
    editor.sync_hold(dock.clone());
    assert!(hold.exists());
    // Turning automation off releases it too; closing the window as well.
    editor.update(Message::Enable(false), true);
    editor.sync_hold(dock.clone());
    assert!(!hold.exists());
    editor.update(Message::Enable(true), true);
    editor.sync_hold(dock);
    assert!(hold.exists());
    editor.release_hold();
    assert!(!hold.exists());
    // A current drag renews/releases its lease without a second save stamp.
    editor.sync_hold([Domain::WidgetLayout].into_iter().collect());
    assert_eq!(automation::hold_applied(&store), None);
    editor.sync_hold(BTreeSet::new());
    editor.release_hold();
    assert!(!hold.exists());
    // A stamp from an older release is preserved when the same lease renews.
    let before = automation::now();
    std::fs::write(
        &hold,
        serde_json::to_vec(&automation::Hold {
            expires: before + 10,
            pid: std::process::id(),
            parts: vec![],
            applied: Some(before),
        })
        .unwrap(),
    )
    .unwrap();
    editor.sync_hold([Domain::WidgetLayout].into_iter().collect());
    editor.release_hold();
    assert!(automation::read_hold(&store).is_empty());
    assert_eq!(automation::hold_applied(&store), Some(before));
}

/// Save a held layout, close Settings before the desktop looked, and let a
/// desktop that was stopped meanwhile start again: the waiting switch still
/// keeps the saved layout, even after Settings opens again with new drafts.
#[test]
fn a_saved_held_layout_outlives_settings_and_a_desktop_restart() {
    use crate::automation::run::Ran;
    use crate::automation::runtime::Automator;
    use crate::automation::{Effect, update};
    use chrono::{FixedOffset, TimeZone, Utc};
    use std::time::Instant;

    let fixture = Fixture::new("auto-layout-durable");
    fixture.system(
        "com.system76.CosmicPanel",
        "entries",
        "[\"Panel\",\"Dock\"]",
    );
    fixture.system("com.system76.CosmicPanel.Dock", "anchor", "Bottom");
    fixture.system("com.system76.CosmicPanel.Panel", "anchor", "Top");
    fixture.user("com.system76.CosmicTheme.Mode", "is_dark", "true");
    std::fs::write(fixture.0.join("outputs"), "DP-1\n").unwrap();
    let config = fixture.0.join("app/config.json");
    std::fs::create_dir_all(config.parent().unwrap()).unwrap();
    std::fs::write(
        &config,
        r#"{"theme":"teal","location":{"name":"SENTINEL","latitude":1.0,"longitude":2.0},
            "enabled_widgets":["clock","weather"],"future_field":7}"#,
    )
    .unwrap();
    let desktop = fixture.store().with_app_config(config.clone());
    update(&desktop, |settings| {
        settings.enabled = true;
        settings.rules = vec![Rule {
            id: "r1".into(),
            enabled: true,
            since: 0,
            profile: ProfileRef::Preset("presentation".into()),
            name: "Presentation".into(),
            when: Trigger::Schedule {
                time: Time::new(19, 0).unwrap(),
                days: Days::EVERY,
            },
        }];
        Ok(())
    })
    .unwrap();
    let utc0 = FixedOffset::east_opt(0).unwrap();
    let at = |h, m| Utc.with_ymd_and_hms(2020, 1, 1, h, m, 0).unwrap();
    let now = Instant::now();

    // Settings holds the layout (and the Dock) while Presentation is due.
    let held: BTreeSet<Domain> = [Domain::WidgetLayout, Domain::Dock].into_iter().collect();
    let mut editor = AutomationEditor::with_store(fixture.store());
    editor.sync_hold(held);
    let mut automator = Automator::with_store(Some(fixture.store()));
    automator.evaluate_with(&utc0, at(18, 0), now, Vec::new);
    let job = automator
        .evaluate_with(&utc0, at(19, 0), now, Vec::new)
        .unwrap();
    let results = job.run(&desktop);
    assert!(matches!(results[0].1, Ran::Waiting(_)), "{results:?}");
    automator.finished(results, at(19, 0).timestamp(), now);
    assert!(automator.state.pending.is_some());
    // The desktop stops. The person saves the layout and closes Settings.
    drop(automator);
    let mut patch = crate::desktop::store::WidgetPatch::new();
    patch
        .set(
            "/widget_positions/clock",
            None,
            Some(serde_json::json!([0.2, 0.3])),
        )
        .unwrap();
    crate::desktop::actions::commit(
        &desktop,
        &crate::desktop::DesktopPatch::new(),
        Some(&patch),
        crate::desktop::store::Owner::Preferences,
        "Widget arrangement",
        crate::desktop::store::Source::Manual,
    )
    .unwrap();
    let layout = std::fs::read(&config).unwrap();
    editor.sync_hold([Domain::Dock].into_iter().collect());
    editor.release_hold();
    // Settings opens again with another draft before the desktop is back.
    let mut again = AutomationEditor::with_store(fixture.store());
    again.sync_hold([Domain::Appearance].into_iter().collect());
    assert!(automation::hold_applied(&fixture.store()).is_none());
    assert!(desktop.manual_at().is_some());
    again.release_hold();

    // The desktop starts again and retries what was waiting.
    let mut automator = Automator::with_store(Some(fixture.store()));
    assert!(
        automator
            .evaluate_with(&utc0, at(19, 5), now, Vec::new)
            .is_none()
    );
    assert_eq!(automator.state.pending, None);
    assert!(matches!(
        automator.state.last().unwrap().effect,
        Effect::KeptManual { .. }
    ));
    assert_eq!(std::fs::read(&config).unwrap(), layout);
    // The durable manual choice covers older occurrences; a later one runs.
    let later = (Utc::now() + chrono::Duration::days(2))
        .date_naive()
        .and_hms_opt(19, 0, 0)
        .unwrap()
        .and_utc();
    let job = automator
        .evaluate_with(&utc0, later, now, Vec::new)
        .unwrap();
    assert!(job.held.is_empty());
    let results = job.run(&desktop);
    assert!(matches!(results[0].1, Ran::Applied { .. }), "{results:?}");
}

#[test]
fn preview_mode_changes_nothing() {
    let fixture = Fixture::new("auto-preview");
    let mut editor = AutomationEditor::with_store(fixture.store().read_only("preview"));
    editor.update(Message::Enable(true), true);
    assert!(matches!(&editor.notice, Some(Notice::Error(_))));
    editor.update(Message::New(Kind::Schedule), true);
    editor.update(
        Message::Draft(DraftEdit::Profile(ProfileRef::Preset("evening".into()))),
        true,
    );
    editor.update(Message::Save, true);
    editor.sync_hold([Domain::Dock].into_iter().collect());
    assert!(!fixture.0.join("state").exists());
}

#[test]
fn recovered_schedule_keeps_original_rule_and_refuses_outside_replacement() {
    let (fixture, mut editor) = editor("auto-session-conflict");
    editor.update(Message::New(Kind::Schedule), true);
    editor.update(
        Message::Draft(DraftEdit::Profile(ProfileRef::Preset("evening".into()))),
        true,
    );
    editor.update(Message::Save, true);
    editor.update(Message::Edit("r1".into()), true);
    editor.update(Message::Draft(DraftEdit::Time("21:30".into())), true);
    let text = ron::to_string(editor.draft.as_ref().unwrap()).unwrap();
    let recovered: RuleDraft = ron::from_str(&text).unwrap();
    let mut reopened = AutomationEditor::with_store(fixture.store());
    reopened.restore_session(recovered).unwrap();
    assert!(reopened.dirty());
    assert_eq!(reopened.draft.as_ref().unwrap().time, "21:30");
    // A concurrent rule change must not be overwritten just because a
    // recovered editor has read the latest rule list for display.
    automation::update(&fixture.store(), |settings| {
        settings.rules[0].profile = ProfileRef::Preset("focus".into());
        Ok(())
    })
    .unwrap();
    reopened.refresh();
    let before = saved(&fixture);
    reopened.update(Message::Save, true);
    assert_eq!(saved(&fixture), before);
    assert!(reopened.dirty());
    assert!(
        matches!(reopened.notice, Some(Notice::Error(ref message)) if message.contains("changed outside this draft"))
    );
    reopened.update(Message::Cancel, true);
    assert!(!reopened.dirty());
}

#[test]
fn incomplete_new_schedule_reopens_without_starting_automation() {
    let (fixture, mut editor) = editor("auto-session-incomplete");
    editor.update(Message::New(Kind::Schedule), true);
    editor.update(Message::Draft(DraftEdit::Time("later".into())), true);
    let recovered: RuleDraft =
        ron::from_str(&ron::to_string(editor.draft.as_ref().unwrap()).unwrap()).unwrap();
    let mut reopened = AutomationEditor::with_store(fixture.store());
    reopened.restore_session(recovered).unwrap();
    assert_eq!(reopened.draft.as_ref().unwrap().time, "later");
    assert!(reopened.draft.as_ref().unwrap().missing().is_some());
    assert_eq!(saved(&fixture), Automation::default());
    assert!(
        !fixture.0.join("state").exists(),
        "recovery itself writes nothing"
    );
}

#[test]
fn adaptive_cycle_requires_city_and_adds_once_without_touching_desktop_or_existing_rules() {
    use crate::desktop::profiles::adaptive::Look;
    let (fixture, mut editor) = editor("adaptive-cycle");
    editor.update(Message::AdaptiveRoutine(Look::Tidal, Source::Manual), true);
    assert!(!fixture.0.join("state/automation.json").exists());
    editor.update(
        Message::ManualCity(crate::config::Location {
            name: "Zagreb".into(),
            latitude: 45.815,
            longitude: 15.982,
            timezone: Some("Europe/Zagreb".into()),
        }),
        true,
    );
    editor.update(Message::New(Kind::Schedule), true);
    editor.update(
        Message::Draft(DraftEdit::Profile(ProfileRef::Moment(
            crate::moments::Phase::Day,
        ))),
        true,
    );
    editor.update(Message::Save, true);
    editor.update(Message::Pause(PauseFor::Resume), true);
    let before = saved(&fixture);
    let config = fixture.store().app_config().map(|p| std::fs::read(p).ok());
    editor.update(Message::AdaptiveRoutine(Look::Tidal, Source::Manual), true);
    let added = saved(&fixture);
    assert_eq!(added.rules.len(), 4);
    assert_eq!(added.rules[0], before.rules[0]);
    assert_eq!(added.paused, before.paused);
    assert_eq!(added.version, 3);
    for rule in &added.rules[1..] {
        assert!(rule.enabled);
        assert!(matches!(
            rule.when,
            Trigger::Solar {
                source: Source::Manual,
                ..
            }
        ));
    }
    editor.update(Message::AdaptiveRoutine(Look::Tidal, Source::Manual), true);
    editor.update(Message::AdaptiveRoutine(Look::Moss, Source::Manual), true);
    assert_eq!(saved(&fixture), added);
    assert_eq!(
        fixture.store().app_config().map(|p| std::fs::read(p).ok()),
        config
    );
}
