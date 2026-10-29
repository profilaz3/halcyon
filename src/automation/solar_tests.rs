use super::{
    solar::{Context, Event, Source},
    *,
};
use crate::{
    config::{Config, Location},
    desktop::{
        profiles::{self, Installed, Profile},
        store::tests::Fixture,
    },
};
use chrono::{DateTime, Duration, Utc};
use serde_json::json;

fn city(zone: &str, lat: f64, lon: f64) -> Location {
    Location {
        name: "Test city".into(),
        latitude: lat,
        longitude: lon,
        timezone: Some(zone.into()),
    }
}
fn setup() -> (Automation, Context) {
    let mut settings = Automation {
        enabled: true,
        manual_city: Some(city("Europe/Zagreb", 45.815, 15.982)),
        ..Default::default()
    };
    settings.rules.push(Rule {
        id: "r1".into(),
        enabled: true,
        since: 0,
        profile: ProfileRef::Moment(crate::moments::Phase::Day),
        name: "Day".into(),
        when: Trigger::Solar {
            event: Event::Sunrise,
            days: Days::EVERY,
            source: Source::Manual,
        },
    });
    settings.version = settings.written_version();
    let context = Context::from_config(&Config::default(), &settings);
    (settings, context)
}
fn instant(s: &str) -> DateTime<Utc> {
    s.parse().unwrap()
}
fn decision(
    settings: &Automation,
    context: &Context,
    state: &mut State,
    at: DateTime<Utc>,
) -> decide::Decision {
    decide::decide_with_solar(settings, state, &Utc, at, None, || None, context)
}

#[test]
fn solar_reference_matches_noaa_across_seasons_within_one_minute() {
    // NOAA's published two-pass sunrise/sunset calculator, retained in private
    // evidence with the generator. The fixture is data, not executable code.
    let rows: serde_json::Value = serde_json::from_str(include_str!(
        "../../docs/future-builder/probes/solar-reference.json"
    ))
    .unwrap();
    for row in rows.as_array().unwrap() {
        let zone: chrono_tz::Tz = row["zone"].as_str().unwrap().parse().unwrap();
        for (key, event) in [("sunrise", Event::Sunrise), ("sunset", Event::Sunset)] {
            let Some(seconds) = row[key].as_i64() else {
                continue;
            };
            let expected = DateTime::from_timestamp(seconds, 0).unwrap();
            let actual = solar::occurrence(
                event,
                expected.with_timezone(&zone).date_naive(),
                row["latitude"].as_f64().unwrap(),
                row["longitude"].as_f64().unwrap(),
                zone,
            )
            .unwrap();
            assert!(
                (actual - expected).num_seconds().abs() <= 60,
                "{row} {key}: {actual} != {expected}"
            );
        }
    }
}
#[test]
fn solar_binding_uses_weather_choice_not_global_visibility_or_output() {
    let (settings, _) = setup();
    let mut config = Config {
        location: Some(city("Europe/Zagreb", 45.815, 15.982)),
        widgets_visible: false,
        enabled_widgets: vec!["weather".into()],
        ..Default::default()
    };
    config.glance.enabled = false;
    let initial = Context::from_config(&config, &settings)
        .revision(Source::Weather)
        .unwrap();
    config.widget_display = Some("OFFLINE-1".into());
    assert_eq!(
        Context::from_config(&config, &settings).revision(Source::Weather),
        Some(initial.clone())
    );
    config.glance.enabled = true;
    config.glance.show_weather = false;
    assert!(
        Context::from_config(&config, &settings)
            .city(Source::Weather)
            .is_err()
    );
    config.glance.show_weather = true;
    config.enabled_widgets.clear();
    assert_eq!(
        Context::from_config(&config, &settings).revision(Source::Weather),
        Some(initial)
    );
    config.location.as_mut().unwrap().timezone = None;
    assert!(
        Context::from_config(&config, &settings)
            .city(Source::Weather)
            .is_err()
    );
    assert!(
        Context::from_config(&config, &settings)
            .city(Source::Manual)
            .is_ok()
    );
}
#[test]
fn solar_old_locations_and_rules_migrate_without_inventing_zone() {
    let old: Location =
        serde_json::from_value(json!({"name":"Zagreb","latitude":45.8,"longitude":16})).unwrap();
    assert!(old.timezone.is_none());
    assert!(solar::validate_city(&old).is_err());
    let old = Automation::default();
    assert_eq!(Automation::parse(&old.to_bytes().unwrap()).unwrap(), old);
    let (new, _) = setup();
    assert_eq!(new.written_version(), 3);
    assert_eq!(Automation::parse(&new.to_bytes().unwrap()).unwrap(), new);
    let mut future = serde_json::to_value(&new).unwrap();
    future["version"] = 4.into();
    assert!(Automation::parse(&serde_json::to_vec(&future).unwrap()).is_err());
}
#[test]
fn solar_once_per_city_date_and_latest_after_sleep_restart() {
    let (settings, context) = setup();
    let mut state = State::default();
    let start = instant("2026-03-28T00:00:00Z");
    assert!(
        decision(&settings, &context, &mut state, start)
            .due
            .is_empty()
    );
    let at = solar::next(&context, Source::Manual, Event::Sunrise, Days::EVERY, start).unwrap();
    assert_eq!(decision(&settings, &context, &mut state, at).due.len(), 1);
    state = State::parse(&serde_json::to_vec(&state).unwrap());
    assert!(decision(&settings, &context, &mut state, at).due.is_empty());
    let resumed = instant("2026-04-02T22:00:00Z");
    let found = decision(&settings, &context, &mut state, resumed).due;
    assert_eq!(found.len(), 1);
    assert!(matches!(&found[0].cause,decide::Cause::Solar{date,..} if date=="2026-04-02"));
    assert!(
        decision(
            &settings,
            &context,
            &mut state,
            resumed - Duration::hours(2)
        )
        .due
        .is_empty()
    );
}
#[test]
fn solar_city_change_retires_waiting_and_queued_work() {
    let (mut settings, context) = setup();
    let mut state = State::default();
    let start = instant("2026-06-21T00:00:00Z");
    decision(&settings, &context, &mut state, start);
    let at = solar::next(&context, Source::Manual, Event::Sunrise, Days::EVERY, start).unwrap();
    let due = decision(&settings, &context, &mut state, at)
        .due
        .pop()
        .unwrap();
    assert!(due.is_current_with(&settings, at.timestamp(), &context));
    state.pending = Some(due.clone());
    settings.manual_city = Some(city("America/New_York", 40.71, -74.01));
    let changed = Context::from_config(&Config::default(), &settings);
    assert!(!due.is_current_with(&settings, at.timestamp(), &changed));
    assert!(
        decision(&settings, &changed, &mut state, at + Duration::seconds(1))
            .due
            .is_empty()
    );
    assert!(state.pending.is_none());
    // Changing a rule within the same second also retires its pending event.
    settings.rules[0].when = Trigger::Solar {
        event: Event::Noon,
        days: Days::EVERY,
        source: Source::Manual,
    };
    assert!(!due.is_current_with(&settings, at.timestamp(), &context));
}
#[test]
fn solar_priority_pause_manual_changes_and_system_zone_are_shared() {
    let (mut settings, context) = setup();
    let mut second = settings.rules[0].clone();
    second.id = "r2".into();
    second.name = "Other".into();
    settings.rules.push(second);
    let start = instant("2026-06-21T00:00:00Z");
    let mut state = State::default();
    decision(&settings, &context, &mut state, start);
    let at = decide::next_instant(
        &settings,
        &state,
        &chrono_tz::Pacific::Honolulu,
        start,
        &context,
    )
    .unwrap()
    .0;
    assert_eq!(
        decision(&settings, &context, &mut state, at).due[0].rule,
        "r1"
    );
    let mut state = State::default();
    decision(&settings, &context, &mut state, start);
    let kept =
        decide::decide_with_solar(&settings, &mut state, &Utc, at, None, || Some(at), &context);
    assert!(kept.due.is_empty());
    assert_eq!(kept.kept, 1);
    let mut state = State::default();
    decision(&settings, &context, &mut state, start);
    settings.paused = Some(Pause {
        since: start.timestamp(),
        until: Some(at.timestamp() + 1),
        skip: true,
    });
    assert!(decision(&settings, &context, &mut state, at).due.is_empty());
    let next = decide::next_instant(&settings, &state, &Utc, at, &context)
        .unwrap()
        .0;
    assert!(next > at + Duration::hours(20));
}
fn desk() -> (Fixture, crate::desktop::DesktopStore) {
    let fixture = Fixture::new("solar-commit");
    let path = fixture.0.join("app/config.json");
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(&path,serde_json::to_vec(&json!({"location":city("Europe/Zagreb",45.815,15.982),"enabled_widgets":["weather"],"glance":{"enabled":false},"appearance":{"opacity":90}})).unwrap()).unwrap();
    let store = fixture.store().with_app_config(path);
    (fixture, store)
}
#[test]
fn solar_weather_change_between_plan_and_commit_writes_nothing() {
    let (_fixture, store) = desk();
    let settings = Automation::default();
    let context = Context::read(&store, &settings);
    let mut profile = Profile::new("Solar").unwrap();
    profile.widget_style = Some(json!({"appearance":{"opacity":80}}));
    let mut plan = profiles::plan(
        &store,
        &Installed::default(),
        &profile,
        &profile.domains().into_iter().collect(),
    );
    plan.guard_widgets(context.guards(Source::Weather).unwrap())
        .unwrap();
    let path = store.app_config().unwrap();
    let mut changed: serde_json::Value =
        serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();
    changed["location"] = serde_json::to_value(city("America/New_York", 40.71, -74.01)).unwrap();
    std::fs::write(path, serde_json::to_vec(&changed).unwrap()).unwrap();
    let result = crate::desktop::actions::scheduled_epoch(
        store.restore_epoch().unwrap(),
        store.manual_at(),
        || profiles::apply_as(&store, &plan, "Solar"),
    );
    assert!(
        matches!(
            result,
            Err(crate::desktop::DesktopError::FieldConflict { .. })
        ),
        "{result:?}"
    );
    let after: serde_json::Value = serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();
    assert_eq!(after, changed);
    assert!(store.latest_change().unwrap().is_none());
}
#[test]
fn solar_projection_excludes_geometry_content_sources_and_enablement() {
    let mut p = Profile::new("Visual").unwrap();
    p.widget_style = Some(
        json!({"appearance":{"opacity":0.5},"clock":{"size":180,"show_date":false},"glance":{"show_weather":false},"location":city("Europe/Zagreb",45.8,16.),"project_roots":["/private"],"recipes":{"clock":{}},"widget_font":"Noto Sans"}),
    );
    p.widget_layout = Some(json!({"enabled_widgets":[],"widget_sizes":{"clock":[800,900]}}));
    p.panel = Some(profiles::PanelLook {
        islands: Some(dock_islands_core::Settings {
            enabled: true,
            ..Default::default()
        }),
        opacity: Some(0.7),
        spacing: Some(12),
        ..Default::default()
    });
    let projected = appearance::project(&p).unwrap();
    assert!(projected.widget_layout.is_none());
    assert_eq!(
        projected.widget_style.unwrap(),
        json!({"appearance":{"opacity":0.5},"widget_font":"Noto Sans"})
    );
    let panel = projected.panel.unwrap();
    assert_eq!(panel.opacity, Some(0.7));
    assert!(panel.islands.is_none() && panel.spacing.is_none());
}

#[test]
fn solar_saved_setup_changes_only_appearance_and_records_recovery() {
    let (_fixture, store) = desk();
    let path = store.app_config().unwrap();
    let mut before: serde_json::Value =
        serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();
    before["clock"] = json!({"scale":170,"show_date":false});
    before["widget_positions"] = json!({"weather":[0.1,0.2]});
    before["widget_sizes"] = json!({"weather":[340,260]});
    before["calendar_private"] = true.into();
    std::fs::write(path, serde_json::to_vec(&before).unwrap()).unwrap();
    let context = Context::read(&store, &Automation::default());
    let mut profile = Profile::new("Solar saved look").unwrap();
    profile.widget_style = Some(
        json!({"appearance":{"opacity":80},"clock":{"scale":90,"show_date":true},"calendar_show_today":false}),
    );
    profile.widget_layout = Some(
        json!({"enabled_widgets":["clock"],"widget_positions":{"weather":[0.9,0.8]},"widget_sizes":{"weather":[400,400]}}),
    );
    profile.version = profile.written_version();
    let id = profiles::ProfileLibrary::new(&store)
        .create(&profile)
        .unwrap();
    let ran = crate::desktop::actions::scheduled_epoch(
        store.restore_epoch().unwrap(),
        store.manual_at(),
        || {
            run::run_for_cause(
                &store,
                &Installed::default(),
                &ProfileRef::Saved(id.as_str().into()),
                "Solar saved look",
                &Default::default(),
                false,
                Some((&context, Source::Weather)),
            )
        },
    );
    assert!(matches!(ran, run::Ran::Applied { .. }), "{ran:?}");
    let after: serde_json::Value = serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();
    let mut expected = before.clone();
    expected["appearance"]["opacity"] = 80.into();
    assert_eq!(after, expected);
    let run::Ran::Applied { change, .. } = ran else {
        unreachable!()
    };
    let receipt: serde_json::Value = serde_json::from_slice(
        &std::fs::read(store.state_dir().join("desktop-undo.json")).unwrap(),
    )
    .unwrap();
    assert_eq!(receipt["id"], change);
    assert_eq!(receipt["source"], "scheduled");
    assert!(store.latest_change().unwrap().is_none());
    assert!(store.manual_at().is_none());
    assert!(store.undo_latest(&change).is_err());
}

#[test]
fn solar_projection_keeps_imported_asset_identity_and_widget_only_theme_scope() {
    use profiles::packages::{Credits, Origin, Revision};
    let mut profile = Profile::new("Imported appearance").unwrap();
    profile.appearance_only = true;
    profile.widget_style = Some(json!({"appearance":{"sync_colors":false,"opacity":80}}));
    profile.package = Some(Origin {
        identity: Revision {
            id: format!("s-{}", "a".repeat(32)),
            revision: "b".repeat(64),
        },
        credits: Credits {
            author: "Artist".into(),
            ..Default::default()
        },
        parent: None,
        picture: Some("c".repeat(64)),
        ancestors: vec![],
    });
    let projected = appearance::project(&profile).unwrap();
    assert!(projected.appearance_only);
    assert_eq!(projected.widget_style, profile.widget_style);
    assert_eq!(projected.package, profile.package);
}

#[test]
fn solar_weekdays_follow_city_date_and_backward_clock_retires_future_wait() {
    let (mut settings, mut context) = setup();
    settings.manual_city = Some(city("Pacific/Kiritimati", 1.87, -157.4));
    context.manual = settings.manual_city.clone();
    settings.rules[0].when = Trigger::Solar {
        event: Event::Sunrise,
        days: Days::WEEKDAYS,
        source: Source::Manual,
    };
    let start = instant("2026-06-21T00:00:00Z");
    let mut state = State::default();
    decision(&settings, &context, &mut state, start);
    let at = decide::next_instant(
        &settings,
        &state,
        &chrono_tz::Pacific::Honolulu,
        start,
        &context,
    )
    .unwrap()
    .0;
    assert_eq!(
        at.date_naive().to_string(),
        "2026-06-21",
        "Sunday UTC is Monday at the city"
    );
    let due = decision(&settings, &context, &mut state, at)
        .due
        .pop()
        .unwrap();
    assert!(matches!(&due.cause,decide::Cause::Solar{date,..} if date=="2026-06-22"));
    state.pending = Some(due);
    decision(&settings, &context, &mut state, at - Duration::hours(30));
    assert!(state.pending.is_none());
}
