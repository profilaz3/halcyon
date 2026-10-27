//! Automation regressions on a fake clock (a zone with daylight saving),
//! fake displays and isolated fixture stores. Nothing reads the real clock,
//! displays or settings.
use super::decide::Effect;
use super::run::Ran;
use super::*;
use crate::desktop::profiles::{
    self, Domain, Installed, Profile, ProfileLibrary, SourceLook, WallpaperLook,
};
use crate::desktop::store::Owner;
use crate::desktop::store::tests::Fixture;
use chrono::{
    DateTime, Duration, FixedOffset, MappedLocalTime, NaiveDate, NaiveDateTime, TimeZone, Utc,
};

// Tests that exercise the runner directly supply the same explicit admission
// boundary as Job::run; production commits never synthesize missing guards.
fn run(
    store: &crate::desktop::DesktopStore,
    installed: &Installed,
    target: &ProfileRef,
    name: &str,
    held: &std::collections::BTreeSet<Domain>,
) -> Ran {
    crate::desktop::actions::scheduled_epoch(
        store.restore_epoch().unwrap(),
        store.manual_at(),
        || super::run::run(store, installed, target, name, held),
    )
}

/// Central European time with 2026's changes: 02:00 → 03:00 on March 29,
/// 03:00 → 02:00 on October 25 (both at 01:00 UTC).
#[derive(Clone, Copy, Debug)]
struct Cet;

fn spring() -> NaiveDateTime {
    day(2026, 3, 29).and_hms_opt(1, 0, 0).unwrap()
}
fn fall() -> NaiveDateTime {
    day(2026, 10, 25).and_hms_opt(1, 0, 0).unwrap()
}
fn std_offset() -> FixedOffset {
    FixedOffset::east_opt(3600).unwrap()
}
fn dst_offset() -> FixedOffset {
    FixedOffset::east_opt(7200).unwrap()
}
impl Cet {
    fn offset_at(utc: NaiveDateTime) -> FixedOffset {
        if utc >= spring() && utc < fall() {
            dst_offset()
        } else {
            std_offset()
        }
    }
}
impl TimeZone for Cet {
    type Offset = FixedOffset;
    fn from_offset(_: &FixedOffset) -> Self {
        Cet
    }
    fn offset_from_local_date(&self, local: &NaiveDate) -> MappedLocalTime<FixedOffset> {
        self.offset_from_local_datetime(&local.and_hms_opt(12, 0, 0).unwrap())
    }
    fn offset_from_local_datetime(&self, local: &NaiveDateTime) -> MappedLocalTime<FixedOffset> {
        let as_std = Self::offset_at(*local - Duration::hours(1)) == std_offset();
        let as_dst = Self::offset_at(*local - Duration::hours(2)) == dst_offset();
        match (as_dst, as_std) {
            (true, true) => MappedLocalTime::Ambiguous(dst_offset(), std_offset()),
            (true, false) => MappedLocalTime::Single(dst_offset()),
            (false, true) => MappedLocalTime::Single(std_offset()),
            (false, false) => MappedLocalTime::None,
        }
    }
    fn offset_from_utc_date(&self, utc: &NaiveDate) -> FixedOffset {
        Self::offset_at(utc.and_hms_opt(12, 0, 0).unwrap())
    }
    fn offset_from_utc_datetime(&self, utc: &NaiveDateTime) -> FixedOffset {
        Self::offset_at(*utc)
    }
}

fn day(y: i32, m: u32, d: u32) -> NaiveDate {
    NaiveDate::from_ymd_opt(y, m, d).unwrap()
}
/// The moment of a Cet wall time (the earlier one when it repeats).
fn at(y: i32, m: u32, d: u32, h: u32, min: u32) -> DateTime<Utc> {
    clock::instant(&Cet, day(y, m, d).and_hms_opt(h, min, 0).unwrap())
}
fn utc(y: i32, m: u32, d: u32, h: u32, min: u32) -> DateTime<Utc> {
    Utc.from_utc_datetime(&day(y, m, d).and_hms_opt(h, min, 0).unwrap())
}
fn schedule(id: &str, h: u32, m: u32, days: Days, name: &str) -> Rule {
    Rule {
        id: id.into(),
        enabled: true,
        since: 0,
        profile: ProfileRef::Preset(name.to_lowercase()),
        name: name.into(),
        when: Trigger::Schedule {
            time: Time::new(h, m).unwrap(),
            days,
        },
    }
}
fn display(id: &str, change: Change, name: Option<&str>, profile: &str) -> Rule {
    Rule {
        id: id.into(),
        enabled: true,
        since: 0,
        profile: ProfileRef::Preset(profile.to_lowercase()),
        name: profile.into(),
        when: Trigger::Display(DisplayWhen {
            change,
            display: name.map(String::from),
            label: None,
        }),
    }
}
fn on(rules: Vec<Rule>) -> Automation {
    Automation {
        enabled: true,
        rules,
        ..Default::default()
    }
}
fn names(due: &[Due]) -> Vec<&str> {
    due.iter().map(|due| due.name.as_str()).collect()
}
/// Decides with no manual changes and no displays.
fn tick(automation: &Automation, state: &mut State, now: DateTime<Utc>) -> Vec<String> {
    decide(automation, state, &Cet, now, None, || None)
        .due
        .iter()
        .map(|due| due.name.clone())
        .collect()
}
fn watch(
    automation: &Automation,
    state: &mut State,
    now: DateTime<Utc>,
    displays: &[&str],
) -> Vec<String> {
    let displays: Vec<String> = displays.iter().map(|name| name.to_string()).collect();
    decide(automation, state, &Cet, now, Some(&displays), || None)
        .due
        .iter()
        .map(|due| due.name.clone())
        .collect()
}
fn day_and_evening() -> Automation {
    on(vec![
        schedule("r1", 7, 0, Days::EVERY, "Day"),
        schedule("r2", 19, 0, Days::EVERY, "Evening"),
    ])
}

#[test]
fn nothing_is_active_on_upgrade_and_a_missing_file_means_off() {
    let fixture = Fixture::new("automation-default");
    let store = fixture.store();
    let automation = load(&store).unwrap();
    assert_eq!(automation, Automation::default());
    assert!(!automation.enabled && automation.rules.is_empty());
    assert!(!automation.running() && !automation.watches_displays());
    let mut state = State::default();
    let now = at(2026, 10, 18, 19, 0);
    assert!(tick(&automation, &mut state, now).is_empty());
    assert_eq!(next_check(&automation, &state, &Cet, now), None);
    // Loading created nothing.
    assert!(!fixture.0.join("state").exists());
}

#[test]
fn a_scheduled_time_applies_once_even_across_a_restart() {
    let automation = day_and_evening();
    let mut state = State::default();
    assert!(tick(&automation, &mut state, at(2026, 10, 18, 18, 59)).is_empty());
    assert_eq!(
        tick(&automation, &mut state, at(2026, 10, 18, 19, 0)),
        ["Evening"]
    );
    assert!(
        tick(
            &automation,
            &mut state,
            at(2026, 10, 18, 19, 0) + Duration::seconds(30)
        )
        .is_empty()
    );
    // Saved and read back by a restarted desktop.
    let mut restarted = State::parse(&serde_json::to_vec(&state).unwrap());
    assert!(tick(&automation, &mut restarted, at(2026, 10, 18, 19, 5)).is_empty());
    assert!(tick(&automation, &mut restarted, at(2026, 10, 18, 23, 0)).is_empty());
    assert_eq!(
        tick(&automation, &mut restarted, at(2026, 10, 19, 7, 0)),
        ["Day"]
    );
}

#[test]
fn after_sleep_only_the_latest_missed_time_catches_up() {
    let automation = day_and_evening();
    let mut state = State::default();
    tick(&automation, &mut state, at(2026, 10, 16, 18, 0));
    // Asleep Friday 18:00 → Monday 10:00: three evenings and three mornings
    // passed; only Monday's 07:00 applies, once.
    assert_eq!(
        tick(&automation, &mut state, at(2026, 10, 19, 10, 0)),
        ["Day"]
    );
    assert!(tick(&automation, &mut state, at(2026, 10, 19, 10, 1)).is_empty());
    // A short sleep across one time catches that one up.
    assert_eq!(
        tick(&automation, &mut state, at(2026, 10, 19, 21, 30)),
        ["Evening"]
    );
}

#[test]
fn a_manual_change_after_a_missed_time_is_kept() {
    let automation = day_and_evening();
    let mut state = State::default();
    tick(&automation, &mut state, at(2026, 10, 18, 18, 0));
    // Asleep through 19:00; the person applied something at 20:15 (e.g. on
    // another session) before the desktop looked again at 21:00.
    let manual = at(2026, 10, 18, 20, 15);
    let decision = decide(
        &automation,
        &mut state,
        &Cet,
        at(2026, 10, 18, 21, 0),
        None,
        || Some(manual),
    );
    assert!(decision.due.is_empty());
    assert_eq!(decision.kept, 1);
    let last = state.last().unwrap();
    assert_eq!(last.due.name, "Evening");
    assert_eq!(
        last.effect,
        Effect::KeptManual {
            manual_at: manual.timestamp()
        }
    );
    // A change from before the time doesn't block it.
    let mut state = State::default();
    tick(&automation, &mut state, at(2026, 10, 18, 18, 0));
    let earlier = at(2026, 10, 18, 18, 30);
    let decision = decide(
        &automation,
        &mut state,
        &Cet,
        at(2026, 10, 18, 21, 0),
        None,
        || Some(earlier),
    );
    assert_eq!(names(&decision.due), ["Evening"]);
}

/// Seconds are whole: a choice made in the second a switch fell due is
/// kept, both for a missed time and for a switch that waited.
#[test]
fn a_manual_change_in_the_due_second_is_kept() {
    let automation = day_and_evening();
    let mut state = State::default();
    tick(&automation, &mut state, at(2026, 10, 18, 18, 0));
    let due_second = at(2026, 10, 18, 19, 0) + Duration::milliseconds(400);
    let decision = decide(
        &automation,
        &mut state,
        &Cet,
        at(2026, 10, 18, 21, 0),
        None,
        || Some(due_second),
    );
    assert!(decision.due.is_empty());
    assert_eq!(decision.kept, 1);
    let due = state.last().unwrap().due.clone();
    assert!(decide::still_due(&due, Some(due_second), due.at + 5).is_err());
    // The second before still lets it run.
    let before = at(2026, 10, 18, 19, 0) - Duration::milliseconds(1);
    assert!(decide::still_due(&due, Some(before), due.at + 5).is_ok());
    let mut state = State::default();
    tick(&automation, &mut state, at(2026, 10, 18, 18, 0));
    let decision = decide(
        &automation,
        &mut state,
        &Cet,
        at(2026, 10, 18, 21, 0),
        None,
        || Some(before),
    );
    assert_eq!(names(&decision.due), ["Evening"]);
}

#[test]
fn turning_automation_or_a_rule_on_never_replays_earlier_times() {
    let mut automation = day_and_evening();
    let mut state = State::default();
    // The desktop last looked a week ago; automation is turned on at 14:00.
    tick(&automation, &mut state, at(2026, 10, 11, 12, 0));
    automation.since = at(2026, 10, 18, 14, 0).timestamp();
    assert!(tick(&automation, &mut state, at(2026, 10, 18, 14, 0)).is_empty());
    assert_eq!(
        tick(&automation, &mut state, at(2026, 10, 18, 19, 0)),
        ["Evening"]
    );
    // A rule made at 20:00 for 09:00 waits for tomorrow's 09:00 even after
    // a restart that missed the evening.
    let mut automation = on(vec![schedule("r1", 9, 0, Days::EVERY, "Work")]);
    automation.rules[0].since = at(2026, 10, 18, 20, 0).timestamp();
    let mut state = State::default();
    tick(&automation, &mut state, at(2026, 10, 18, 8, 0));
    assert!(tick(&automation, &mut state, at(2026, 10, 18, 20, 30)).is_empty());
    assert_eq!(
        tick(&automation, &mut state, at(2026, 10, 19, 9, 0)),
        ["Work"]
    );
}

#[test]
fn a_pause_skips_switches_without_catching_them_up_later() {
    let mut automation = day_and_evening();
    let mut state = State::default();
    tick(&automation, &mut state, at(2026, 10, 18, 18, 0));
    automation.paused = Some(Pause {
        since: at(2026, 10, 18, 18, 30).timestamp(),
        until: Some(at(2026, 10, 18, 21, 0).timestamp()),
        skip: false,
    });
    assert!(tick(&automation, &mut state, at(2026, 10, 18, 19, 0)).is_empty());
    assert!(tick(&automation, &mut state, at(2026, 10, 18, 21, 0)).is_empty());
    // The desktop was off during the whole pause: still not caught up.
    let mut cold = State::default();
    tick(&automation, &mut cold, at(2026, 10, 18, 18, 0));
    assert!(tick(&automation, &mut cold, at(2026, 10, 18, 22, 0)).is_empty());
    // The next morning switches as usual.
    assert_eq!(
        tick(&automation, &mut state, at(2026, 10, 19, 7, 0)),
        ["Day"]
    );
    // Skip next: a pause that ends right after the next switch.
    let (local, _) = next_switch(&automation, &state, &Cet, at(2026, 10, 19, 8, 0)).unwrap();
    assert_eq!(local, day(2026, 10, 19).and_hms_opt(19, 0, 0).unwrap());
    automation.paused = Some(Pause {
        since: at(2026, 10, 19, 8, 0).timestamp(),
        until: Some(clock::instant(&Cet, local).timestamp() + 1),
        skip: true,
    });
    assert!(tick(&automation, &mut state, at(2026, 10, 19, 19, 0)).is_empty());
    let (after, _) = next_switch(&automation, &state, &Cet, at(2026, 10, 19, 8, 0)).unwrap();
    assert_eq!(after, day(2026, 10, 20).and_hms_opt(7, 0, 0).unwrap());
    assert_eq!(
        tick(&automation, &mut state, at(2026, 10, 20, 7, 0)),
        ["Day"]
    );
    // Until resumed: nothing is next.
    automation.paused = Some(Pause {
        since: at(2026, 10, 20, 8, 0).timestamp(),
        until: None,
        skip: false,
    });
    assert_eq!(
        next_switch(&automation, &state, &Cet, at(2026, 10, 20, 9, 0)),
        None
    );
    assert!(tick(&automation, &mut state, at(2026, 10, 20, 19, 0)).is_empty());
}

#[test]
fn a_time_inside_the_spring_gap_happens_once_when_the_gap_ends() {
    let automation = on(vec![schedule("r1", 2, 30, Days::EVERY, "Night")]);
    let mut state = State::default();
    tick(&automation, &mut state, at(2026, 3, 28, 23, 0));
    // 02:30 doesn't exist on March 29; the clock jumps 02:00 → 03:00 at
    // 01:00 UTC, which is when it happens.
    let (local, _) = next_switch(&automation, &state, &Cet, at(2026, 3, 28, 23, 0)).unwrap();
    assert_eq!(clock::instant(&Cet, local), utc(2026, 3, 29, 1, 0));
    assert!(tick(&automation, &mut state, utc(2026, 3, 29, 0, 59)).is_empty());
    assert_eq!(
        tick(&automation, &mut state, utc(2026, 3, 29, 1, 0)),
        ["Night"]
    );
    assert!(tick(&automation, &mut state, utc(2026, 3, 29, 2, 0)).is_empty());
    // A wake scheduled for the gap lands on its end, within a minute.
    let check = next_check(&automation, &state, &Cet, utc(2026, 3, 30, 0, 29)).unwrap();
    assert_eq!(check, utc(2026, 3, 30, 0, 30));
}

#[test]
fn the_repeated_autumn_hour_switches_only_once() {
    let automation = on(vec![schedule("r1", 2, 30, Days::EVERY, "Night")]);
    let mut state = State::default();
    tick(&automation, &mut state, at(2026, 10, 24, 23, 0));
    // 02:30 happens at 00:30 UTC (summer time) and again at 01:30 UTC.
    assert_eq!(
        tick(&automation, &mut state, utc(2026, 10, 25, 0, 30)),
        ["Night"]
    );
    assert!(tick(&automation, &mut state, utc(2026, 10, 25, 1, 30)).is_empty());
    assert!(tick(&automation, &mut state, utc(2026, 10, 25, 2, 30)).is_empty());
    assert_eq!(
        tick(&automation, &mut state, at(2026, 10, 26, 2, 30)),
        ["Night"]
    );
}

#[test]
fn time_zone_changes_follow_the_local_wall_clock_without_repeats() {
    let automation = on(vec![schedule("r1", 20, 0, Days::EVERY, "Evening")]);
    let berlin = FixedOffset::east_opt(2 * 3600).unwrap();
    let lisbon = FixedOffset::east_opt(0).unwrap();
    let tokyo = FixedOffset::east_opt(9 * 3600).unwrap();
    let local = |tz: &FixedOffset, h: u32| {
        tz.from_local_datetime(&day(2026, 10, 18).and_hms_opt(h, 0, 0).unwrap())
            .unwrap()
            .with_timezone(&Utc)
    };
    let run = |state: &mut State, tz: &FixedOffset, now: DateTime<Utc>| {
        decide(&automation, state, tz, now, None, || None).due.len()
    };
    let mut state = State::default();
    run(&mut state, &berlin, local(&berlin, 19));
    assert_eq!(run(&mut state, &berlin, local(&berlin, 20)), 1);
    // Flying west: 20:00 comes round again on the same date; it was had.
    assert_eq!(run(&mut state, &lisbon, local(&lisbon, 19)), 0);
    assert_eq!(run(&mut state, &lisbon, local(&lisbon, 20)), 0);
    assert_eq!(run(&mut state, &lisbon, local(&lisbon, 21)), 0);
    // Flying east past 20:00 the next day catches that one up, once.
    let mut state = State::default();
    run(&mut state, &berlin, local(&berlin, 15));
    let next_day = tokyo
        .from_local_datetime(&day(2026, 10, 18).and_hms_opt(23, 0, 0).unwrap())
        .unwrap()
        .with_timezone(&Utc);
    assert_eq!(run(&mut state, &tokyo, next_day), 1);
    assert_eq!(run(&mut state, &tokyo, next_day + Duration::minutes(1)), 0);
}

#[test]
fn a_clock_set_back_neither_repeats_nor_stalls() {
    let automation = day_and_evening();
    let mut state = State::default();
    tick(&automation, &mut state, at(2026, 10, 18, 18, 0));
    assert_eq!(
        tick(&automation, &mut state, at(2026, 10, 18, 19, 0)),
        ["Evening"]
    );
    // Set back two hours: 19:00 is not applied a second time.
    assert!(tick(&automation, &mut state, at(2026, 10, 18, 17, 0)).is_empty());
    assert!(tick(&automation, &mut state, at(2026, 10, 18, 19, 0)).is_empty());
    // A clock that was years ahead and is corrected starts again from now,
    // instead of waiting until it catches up.
    let mut state = State::default();
    tick(&automation, &mut state, at(2030, 1, 1, 12, 0));
    let decision = decide(
        &automation,
        &mut state,
        &Cet,
        at(2026, 10, 18, 12, 0),
        None,
        || None,
    );
    assert!(decision.due.is_empty() && decision.persist);
    assert_eq!(
        tick(&automation, &mut state, at(2026, 10, 18, 19, 0)),
        ["Evening"]
    );
}

#[test]
fn simultaneous_rules_resolve_by_list_order() {
    let automation = on(vec![
        schedule("r1", 19, 0, Days::WEEKDAYS, "Evening"),
        schedule("r2", 19, 0, Days::EVERY, "Glass"),
        schedule("r3", 18, 0, Days::EVERY, "Workbench"),
    ]);
    let mut state = State::default();
    tick(&automation, &mut state, at(2026, 10, 16, 17, 0));
    // Friday 19:00: both 19:00 rules; the higher one wins, and the earlier
    // 18:00 one missed during sleep is superseded.
    assert_eq!(
        tick(&automation, &mut state, at(2026, 10, 16, 19, 30)),
        ["Evening"]
    );
    // Saturday: only the every-day rule.
    tick(&automation, &mut state, at(2026, 10, 17, 18, 30));
    assert_eq!(
        tick(&automation, &mut state, at(2026, 10, 17, 19, 0)),
        ["Glass"]
    );
}

#[test]
fn display_changes_switch_once_per_change_with_the_first_matching_rule() {
    let automation = on(vec![
        display("r1", Change::Connect, Some("HDMI-A-1"), "Presentation"),
        display("r2", Change::Connect, None, "Workbench"),
        display("r3", Change::Disconnect, None, "Minimal"),
    ]);
    let mut state = State::default();
    let now = at(2026, 10, 18, 9, 0);
    // The first observation is only a baseline.
    assert!(watch(&automation, &mut state, now, &["eDP-1"]).is_empty());
    assert!(
        watch(
            &automation,
            &mut state,
            now + Duration::minutes(1),
            &["eDP-1"]
        )
        .is_empty()
    );
    // Docked: the first external display.
    assert_eq!(
        watch(
            &automation,
            &mut state,
            now + Duration::minutes(2),
            &["DP-2", "eDP-1"]
        ),
        ["Workbench"]
    );
    assert!(
        watch(
            &automation,
            &mut state,
            now + Duration::minutes(3),
            &["DP-2", "eDP-1"]
        )
        .is_empty()
    );
    // A projector on HDMI while docked: the named rule.
    assert_eq!(
        watch(
            &automation,
            &mut state,
            now + Duration::minutes(4),
            &["DP-2", "HDMI-A-1", "eDP-1"]
        ),
        ["Presentation"]
    );
    // Unplugging the projector alone: still docked, nothing.
    assert!(
        watch(
            &automation,
            &mut state,
            now + Duration::minutes(5),
            &["DP-2", "eDP-1"]
        )
        .is_empty()
    );
    // Undocked: the last external display left.
    assert_eq!(
        watch(
            &automation,
            &mut state,
            now + Duration::minutes(6),
            &["eDP-1"]
        ),
        ["Minimal"]
    );
    // Projector and dock at once: one switch, the higher rule.
    assert_eq!(
        watch(
            &automation,
            &mut state,
            now + Duration::minutes(7),
            &["DP-2", "HDMI-A-1", "eDP-1"]
        ),
        ["Presentation"]
    );
}

#[test]
fn display_rules_ignore_changes_seen_before_they_were_turned_on() {
    let mut automation = on(vec![display(
        "r1",
        Change::Connect,
        Some("DP-2"),
        "Workbench",
    )]);
    let mut state = State::default();
    let now = at(2026, 10, 18, 9, 0);
    watch(&automation, &mut state, now, &["eDP-1"]);
    // Saved state from an earlier session; the rule was made after it.
    automation.rules[0].since = (now + Duration::hours(1)).timestamp();
    assert!(
        watch(
            &automation,
            &mut state,
            now + Duration::hours(2),
            &["DP-2", "eDP-1"]
        )
        .is_empty()
    );
    // Changes after the rule exists do count.
    watch(
        &automation,
        &mut state,
        now + Duration::hours(3),
        &["eDP-1"],
    );
    assert_eq!(
        watch(
            &automation,
            &mut state,
            now + Duration::hours(4),
            &["DP-2", "eDP-1"]
        ),
        ["Workbench"]
    );
    // While paused, a change is seen but not acted on, and not replayed.
    automation.paused = Some(Pause {
        since: (now + Duration::hours(5)).timestamp(),
        until: Some((now + Duration::hours(6)).timestamp()),
        skip: false,
    });
    watch(
        &automation,
        &mut state,
        now + Duration::minutes(5 * 60 + 10),
        &["eDP-1"],
    );
    assert!(
        watch(
            &automation,
            &mut state,
            now + Duration::minutes(5 * 60 + 20),
            &["DP-2", "eDP-1"]
        )
        .is_empty()
    );
    assert!(
        watch(
            &automation,
            &mut state,
            now + Duration::hours(7),
            &["DP-2", "eDP-1"]
        )
        .is_empty()
    );
    // After the pause, a change seen during it still counts as a baseline.
    watch(
        &automation,
        &mut state,
        now + Duration::hours(8),
        &["eDP-1"],
    );
    assert_eq!(
        watch(
            &automation,
            &mut state,
            now + Duration::hours(9),
            &["DP-2", "eDP-1"]
        ),
        ["Workbench"]
    );
}

#[test]
fn a_missed_time_and_a_display_change_apply_oldest_first_once_each() {
    let automation = on(vec![
        schedule("r1", 7, 0, Days::EVERY, "Day"),
        display("r2", Change::Connect, Some("DP-2"), "Workbench"),
        display("r3", Change::Disconnect, Some("DP-2"), "Day"),
    ]);
    let mut state = State::default();
    watch(&automation, &mut state, at(2026, 10, 17, 22, 0), &["eDP-1"]);
    // Booted docked the next morning.
    assert_eq!(
        watch(
            &automation,
            &mut state,
            at(2026, 10, 18, 8, 0),
            &["DP-2", "eDP-1"]
        ),
        ["Day", "Workbench"]
    );
    // The same profile from both: applied once.
    watch(
        &automation,
        &mut state,
        at(2026, 10, 18, 23, 0),
        &["DP-2", "eDP-1"],
    );
    assert_eq!(
        watch(&automation, &mut state, at(2026, 10, 19, 8, 0), &["eDP-1"]),
        ["Day"]
    );
}

#[test]
fn the_desktop_looks_again_within_a_minute_and_not_at_all_when_off() {
    let mut automation = day_and_evening();
    let state = State::default();
    let now = at(2026, 10, 18, 18, 59) + Duration::seconds(30);
    assert_eq!(
        next_check(&automation, &state, &Cet, now),
        Some(at(2026, 10, 18, 19, 0))
    );
    let early = at(2026, 10, 18, 12, 0);
    assert_eq!(
        next_check(&automation, &state, &Cet, early),
        Some(early + Duration::seconds(60))
    );
    automation.enabled = false;
    assert_eq!(next_check(&automation, &state, &Cet, early), None);
    automation.enabled = true;
    for rule in &mut automation.rules {
        rule.enabled = false;
    }
    assert_eq!(next_check(&automation, &state, &Cet, early), None);
}

#[test]
fn a_waiting_switch_is_replaced_by_a_newer_one_and_dropped_when_paused() {
    let mut automation = day_and_evening();
    let mut state = State::default();
    tick(&automation, &mut state, at(2026, 10, 18, 6, 0));
    let due = decide(
        &automation,
        &mut state,
        &Cet,
        at(2026, 10, 18, 7, 0),
        None,
        || None,
    )
    .due;
    state.pending = Some(due[0].clone());
    // Nothing new: it keeps waiting.
    tick(&automation, &mut state, at(2026, 10, 18, 12, 0));
    assert_eq!(
        state.pending.as_ref().map(|due| due.name.as_str()),
        Some("Day")
    );
    // Applied in Settings meanwhile: the waiting switch is no longer wanted.
    let kept = decide::still_due(
        state.pending.as_ref().unwrap(),
        Some(at(2026, 10, 18, 12, 5)),
        at(2026, 10, 18, 12, 6).timestamp(),
    );
    assert!(matches!(
        kept,
        Err(Outcome {
            effect: Effect::KeptManual { .. },
            ..
        })
    ));
    // A newer switch replaces it.
    assert_eq!(
        tick(&automation, &mut state, at(2026, 10, 18, 19, 0)),
        ["Evening"]
    );
    assert_eq!(state.pending, None);
    state.pending = Some(due[0].clone());
    automation.paused = Some(Pause {
        since: at(2026, 10, 18, 19, 30).timestamp(),
        until: None,
        skip: false,
    });
    tick(&automation, &mut state, at(2026, 10, 18, 20, 0));
    assert_eq!(state.pending, None);
}

#[test]
fn skip_next_skips_only_that_switch_and_display_rules_keep_working() {
    let mut automation = on(vec![
        schedule("r1", 7, 0, Days::EVERY, "Day"),
        schedule("r2", 19, 0, Days::EVERY, "Evening"),
        display("r3", Change::Connect, None, "Workbench"),
    ]);
    let mut state = State::default();
    assert!(watch(&automation, &mut state, at(2026, 10, 18, 8, 0), &["eDP-1"]).is_empty());
    // At 08:00 the person skips the next switch: Evening at 19:00.
    let (local, _) = next_switch(&automation, &state, &Cet, at(2026, 10, 18, 8, 0)).unwrap();
    automation.paused = Some(Pause {
        since: at(2026, 10, 18, 8, 0).timestamp(),
        until: Some(clock::instant(&Cet, local).timestamp() + 1),
        skip: true,
    });
    assert!(
        automation
            .paused_at(at(2026, 10, 18, 9, 0).timestamp())
            .is_none()
    );
    // Docking still switches.
    assert_eq!(
        watch(
            &automation,
            &mut state,
            at(2026, 10, 18, 9, 0),
            &["DP-2", "eDP-1"]
        ),
        ["Workbench"]
    );
    // A rule made after the skip isn't skipped either.
    let mut lunch = schedule("r4", 12, 0, Days::EVERY, "Lunch");
    lunch.since = at(2026, 10, 18, 9, 30).timestamp();
    automation.rules.push(lunch);
    assert_eq!(
        watch(
            &automation,
            &mut state,
            at(2026, 10, 18, 12, 0),
            &["DP-2", "eDP-1"]
        ),
        ["Lunch"]
    );
    // The named switch is skipped, and not caught up later.
    assert!(
        watch(
            &automation,
            &mut state,
            at(2026, 10, 18, 19, 0),
            &["DP-2", "eDP-1"]
        )
        .is_empty()
    );
    assert!(
        watch(
            &automation,
            &mut state,
            at(2026, 10, 18, 21, 0),
            &["DP-2", "eDP-1"]
        )
        .is_empty()
    );
    // The next day is as usual.
    assert_eq!(
        watch(
            &automation,
            &mut state,
            at(2026, 10, 19, 19, 0),
            &["DP-2", "eDP-1"]
        ),
        ["Evening"]
    );
    // A real pause still holds back display changes too.
    automation.paused = Some(Pause {
        since: at(2026, 10, 19, 20, 0).timestamp(),
        until: None,
        skip: false,
    });
    assert!(watch(&automation, &mut state, at(2026, 10, 19, 21, 0), &["eDP-1"]).is_empty());
    assert!(
        watch(
            &automation,
            &mut state,
            at(2026, 10, 19, 22, 0),
            &["DP-2", "eDP-1"]
        )
        .is_empty()
    );
}

#[test]
fn a_waiting_switch_never_runs_over_a_newer_one() {
    let automation = on(vec![
        schedule("r1", 7, 0, Days::EVERY, "Day"),
        display("r2", Change::Connect, None, "Workbench"),
    ]);
    let mut state = State::default();
    watch(&automation, &mut state, at(2026, 10, 19, 6, 0), &["eDP-1"]);
    let due = decide(
        &automation,
        &mut state,
        &Cet,
        at(2026, 10, 19, 7, 0),
        Some(&["eDP-1".to_string()]),
        || None,
    )
    .due;
    assert_eq!(names(&due), ["Day"]);
    // Day waits for Settings; then the dock is plugged in.
    state.pending = Some(due[0].clone());
    assert_eq!(
        watch(
            &automation,
            &mut state,
            at(2026, 10, 19, 7, 30),
            &["DP-2", "eDP-1"]
        ),
        ["Workbench"]
    );
    // Day is gone for good, and the history says why.
    assert_eq!(state.pending, None);
    let replaced = &state.recent[0];
    assert_eq!(replaced.due.name, "Day");
    assert_eq!(
        replaced.effect,
        Effect::Replaced {
            by: "Workbench".into()
        }
    );
    assert_eq!(
        status::outcome_line(replaced, true).what,
        "Day was replaced by Workbench"
    );
}

#[test]
fn times_and_days_read_naturally() {
    for (text, expected) in [
        ("19:00", (19, 0)),
        ("7:30", (7, 30)),
        ("0730", (7, 30)),
        ("19", (19, 0)),
        ("7pm", (19, 0)),
        ("7:15 PM", (19, 15)),
        ("12am", (0, 0)),
        ("12:05 pm", (12, 5)),
        ("19.45", (19, 45)),
    ] {
        let time = Time::parse(text).unwrap_or_else(|| panic!("{text}"));
        assert_eq!((time.hour(), time.minute()), expected, "{text}");
    }
    for text in [
        "", "24:00", "7:5", "13pm", "noon", "19:60", "1:2:3", "1日", "日1", "🕐", "é12", "１２",
        "١٢", "7:日", "1日pm",
    ] {
        assert_eq!(Time::parse(text), None, "{text}");
    }
    assert_eq!(Time::new(19, 5).unwrap().format(false), "7:05 PM");
    assert_eq!(Time::new(0, 30).unwrap().format(false), "12:30 AM");
    assert_eq!(Days::EVERY.label(), "Every day");
    assert_eq!(Days::WEEKDAYS.label(), "Weekdays");
    assert_eq!(Days::WEEKEND.label(), "Weekends");
    assert_eq!(Days::NONE.toggled(0).label(), "Mondays");
    assert_eq!(
        Days::NONE.toggled(0).toggled(2).toggled(4).label(),
        "Mon, Wed, Fri"
    );
    let rule = display("r1", Change::Disconnect, None, "Minimal");
    assert_eq!(
        rule.when.summary(true),
        "The last external display disconnects"
    );
    assert_eq!(
        schedule("r2", 7, 0, Days::WEEKDAYS, "Day")
            .when
            .summary(true),
        "Weekdays at 07:00"
    );
}

#[test]
fn settings_round_trip_and_refuse_damaged_or_newer_files() {
    let fixture = Fixture::new("automation-file");
    let store = fixture.store();
    let saved = update(&store, |automation| {
        automation.enabled = true;
        automation.since = 100;
        let id = automation.next_id();
        automation
            .rules
            .push(schedule(&id, 19, 0, Days::WEEKDAYS, "Evening"));
        let id = automation.next_id();
        automation
            .rules
            .push(display(&id, Change::Connect, Some("DP-2"), "Workbench"));
        Ok(())
    })
    .unwrap();
    assert_eq!(saved.rules[1].id, "r2");
    assert_eq!(load(&store).unwrap(), saved);
    let file = path(&store, FILE);
    let text = std::fs::read_to_string(&file).unwrap();
    assert!(
        text.contains("\"19:00\"") && text.contains("\"mon\""),
        "{text}"
    );
    use std::os::unix::fs::PermissionsExt;
    assert_eq!(
        std::fs::metadata(&file).unwrap().permissions().mode() & 0o777,
        0o600
    );
    // A failed change writes nothing.
    assert!(update(&store, |_| Err("no".into())).is_err());
    assert!(
        update(&store, |automation| {
            automation.rules[0].when = Trigger::Schedule {
                time: Time::new(8, 0).unwrap(),
                days: Days::NONE,
            };
            Ok(())
        })
        .is_err()
    );
    assert_eq!(std::fs::read_to_string(&file).unwrap(), text);
    // Newer and damaged files are refused with a reason, never half-used.
    let newer = text.replace("\"version\": 1", "\"version\": 4");
    std::fs::write(&file, newer).unwrap();
    assert!(load(&store).unwrap_err().contains("newer"));
    std::fs::write(&file, "{").unwrap();
    assert!(load(&store).unwrap_err().contains("damaged"));
    // Damaged runtime state starts fresh.
    std::fs::write(path(&store, STATE_FILE), "nonsense").unwrap();
    assert_eq!(load_state(&store), State::default());
    // A read-only (preview) store can't save.
    let preview = fixture.store().read_only("preview");
    assert!(update(&preview, |_| Ok(())).is_err());
}

#[test]
fn held_parts_count_only_while_their_settings_window_runs() {
    let fixture = Fixture::new("automation-hold");
    let store = fixture.store();
    assert!(read_hold(&store).is_empty());
    let parts = [Domain::Dock, Domain::Wallpaper].into_iter().collect();
    write_hold(&store, &parts).unwrap();
    assert_eq!(read_hold(&store), parts);
    let expired = serde_json::to_vec(&Hold {
        expires: now() - 1,
        pid: std::process::id(),
        parts: vec!["wallpaper".into()],
        applied: None,
    })
    .unwrap();
    std::fs::write(path(&store, HOLD_FILE), expired).unwrap();
    assert!(
        read_hold(&store).is_empty(),
        "a hung live window cannot renew a gesture lease"
    );
    // Written by a window that is gone (no such process).
    let stale = serde_json::to_vec(&Hold {
        expires: now() + 10,
        pid: u32::MAX - 7,
        parts: vec!["dock".into()],
        applied: None,
    })
    .unwrap();
    std::fs::write(path(&store, HOLD_FILE), stale).unwrap();
    assert!(read_hold(&store).is_empty());
    write_hold(&store, &Default::default()).unwrap();
    assert!(!path(&store, HOLD_FILE).exists());
}

/// A fixture desktop with a widget configuration, like the presets' tests.
fn desk(name: &str) -> (Fixture, crate::desktop::DesktopStore) {
    let fixture = Fixture::new(name);
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
    let store = fixture.store().with_app_config(config);
    (fixture, store)
}

fn config_text(fixture: &Fixture) -> String {
    std::fs::read_to_string(fixture.0.join("app/config.json")).unwrap()
}

#[test]
fn scheduled_preset_keeps_applied_design_choices_and_rejects_damaged_design() {
    use crate::desktop::store::{DesktopPatch, WidgetPatch};
    let (fixture, store) = desk("automation-applied-design");
    let path = store.app_config().unwrap();
    let target = ProfileRef::Preset("carbon".into());
    store.migrate_design().unwrap();
    let frozen = config_text(&fixture);
    let mut patch = WidgetPatch::new();
    patch.set("/devbox/cpu", None, Some(false.into())).unwrap();
    store
        .apply_owned(
            &DesktopPatch::new(),
            Some(&patch),
            Owner::Preferences,
            "CPU off",
        )
        .unwrap();
    let fitted = super::run::load(&store, &target, "Carbon").unwrap();
    assert!(
        fitted
            .widget_style
            .as_ref()
            .unwrap()
            .get("devbox")
            .is_none()
    );
    let installed = Installed::discover(&store);
    assert!(matches!(
        run(&store, &installed, &target, "Carbon", &Default::default()),
        Ran::Applied { .. }
    ));
    let applied = crate::config::read_config_text(path).unwrap().unwrap();
    assert!(
        !crate::config::Config::from_bytes(applied.as_bytes())
            .unwrap()
            .devbox
            .cpu
    );
    assert_eq!(
        config_text(&fixture),
        frozen,
        "the migrated source is provenance, not applied state"
    );
    std::fs::write(crate::design::storage::path(path), b"damaged").unwrap();
    assert!(super::run::load(&store, &target, "Carbon").is_err());
    // Only a genuinely absent configuration may use defaults.
    let missing = fixture
        .store()
        .with_app_config(fixture.0.join("absent.json"));
    assert!(super::run::load(&missing, &target, "Carbon").is_ok());
}

#[test]
fn a_switch_uses_the_profile_engine_once_and_records_an_automatic_undo() {
    let (fixture, store) = desk("automation-run");
    let installed = Installed::discover(&store);
    let target = ProfileRef::Preset("presentation".into());
    let none = Default::default();
    let ran = run(&store, &installed, &target, "Presentation", &none);
    let Ran::Applied { change, skipped } = &ran else {
        panic!("{ran:?}");
    };
    assert!(skipped.is_empty(), "{skipped:?}");
    assert!(store.latest_change().unwrap().is_none());
    assert!(store.manual_at().is_none());
    let record: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(store.state_dir().join("desktop-undo.json")).unwrap(),
    )
    .unwrap();
    assert_eq!(record["id"], change.as_str());
    assert_eq!(record["source"], "scheduled");
    // Unrelated widget settings are preserved.
    let config = config_text(&fixture);
    assert!(
        config.contains("SENTINEL") && config.contains("future_field"),
        "{config}"
    );
    // Running it again writes nothing and keeps the Undo record.
    let before = fixture.tree();
    assert_eq!(
        run(&store, &installed, &target, "Presentation", &none),
        Ran::AlreadySet
    );
    assert_eq!(fixture.tree(), before);
    assert!(
        store.undo_latest(change).is_err(),
        "automatic work is not a manual Undo"
    );
}

#[test]
fn a_switch_waits_for_unapplied_settings_and_explains_missing_profiles() {
    let (fixture, store) = desk("automation-wait");
    let installed = Installed::discover(&store);
    let before = fixture.tree();
    let held = [
        Domain::WidgetLayout,
        Domain::Appearance,
        Domain::Dock,
        Domain::Panel,
    ]
    .into_iter()
    .collect();
    let ran = run(
        &store,
        &installed,
        &ProfileRef::Preset("presentation".into()),
        "Presentation",
        &held,
    );
    assert!(matches!(ran, Ran::Waiting(_)), "{ran:?}");
    assert_eq!(fixture.tree(), before);
    let gone = run(
        &store,
        &installed,
        &ProfileRef::Saved("p-gone".into()),
        "Evening",
        &Default::default(),
    );
    assert_eq!(
        gone,
        Ran::Failed("The profile “Evening” was deleted. Choose another one for this rule.".into())
    );
    let preview = fixture.store().read_only("preview");
    assert!(matches!(
        run(
            &preview,
            &installed,
            &ProfileRef::Preset("evening".into()),
            "Glass",
            &Default::default()
        ),
        Ran::Failed(_)
    ));
    assert_eq!(fixture.tree(), before);
}

#[test]
fn a_moved_wallpaper_leaves_out_only_that_part() {
    let (fixture, store) = desk("automation-asset");
    let installed = Installed::discover(&store);
    let mut profile = Profile::new("Evening").unwrap();
    profile.widget_style = Some(serde_json::json!({"theme": "rose"}));
    profile.wallpaper = Some(WallpaperLook {
        background: Some(profiles::BackgroundLook {
            source: Some(SourceLook::Image(fixture.0.join("home/me/gone.png"))),
            ..Default::default()
        }),
        ..Default::default()
    });
    profile.version = profile.written_version();
    let id = ProfileLibrary::new(&store).create(&profile).unwrap();
    let before = fixture.tree();
    let ran = run(
        &store,
        &installed,
        &ProfileRef::Saved(id.as_str().into()),
        "Evening",
        &Default::default(),
    );
    assert!(
        matches!(&ran,Ran::Failed(reason) if reason.contains("gone.png")),
        "{ran:?}"
    );
    assert_eq!(fixture.tree(), before, "a coordinated setup is atomic");
    let line = status::outcome_line(
        &Outcome {
            transaction: None,
            at: 0,
            due: Due {
                revision: None,
                rule: "r1".into(),
                profile: ProfileRef::Saved(id.as_str().into()),
                name: "Evening".into(),
                cause: Cause::Schedule("2026-10-18T19:00:00".into()),
                at: 0,
            },
            effect: ran.effect(),
        },
        true,
    );
    assert_eq!(line.what, "Couldn’t apply Evening");
    assert_eq!(line.cause, "Scheduled for 19:00");
    assert!(line.why.unwrap().contains("gone.png"));
}

#[test]
fn an_interrupted_change_stops_automation_until_it_is_decided() {
    let (fixture, store) = desk("automation-recovery");
    let installed = Installed::discover(&store);
    let glass = ProfileRef::Preset("evening".into());
    store.inject(Some(crate::desktop::store::Fault::InterruptBeforeRecord));
    assert!(matches!(
        run(&store, &installed, &glass, "Glass", &Default::default()),
        Ran::Failed(_)
    ));
    store.inject(None);
    assert!(store.pending_recovery().unwrap().is_some());
    let before = fixture.tree();
    assert_eq!(
        run(&store, &installed, &glass, "Glass", &Default::default()),
        Ran::Failed("An interrupted change needs a decision on the Profiles page first.".into())
    );
    assert_eq!(fixture.tree(), before);
}

#[test]
fn importing_or_applying_a_profile_never_turns_automation_on() {
    let (fixture, store) = desk("automation-import");
    // As Setups offers it here: without bundled wallpapers, the rest applies.
    let evening = profiles::fitted(
        profiles::preset("evening").unwrap(),
        &Default::default(),
        &crate::desktop::wallpaper::bundled_dir(&store),
        false,
    );
    let exported = profiles::export(&evening.profile).unwrap();
    let mut value: serde_json::Value = serde_json::from_slice(&exported.bytes).unwrap();
    // Even a file that carries automation-looking members.
    value["automation"] = serde_json::json!({"enabled": true, "rules": []});
    let bytes = serde_json::to_vec(&value).unwrap();
    if let Ok(profile) = profiles::import(&bytes) {
        ProfileLibrary::new(&store).create(&profile).unwrap();
    }
    let profile = profiles::import(&exported.bytes).unwrap();
    let id = ProfileLibrary::new(&store).create(&profile).unwrap();
    let installed = Installed::discover(&store);
    let selected = profile.domains().into_iter().collect();
    let plan = profiles::plan(&store, &installed, &profile, &selected);
    profiles::apply(&store, &plan).unwrap();
    assert!(!path(&store, FILE).exists());
    assert_eq!(load(&store).unwrap(), Automation::default());
    let _ = (id, fixture);
}

#[test]
fn display_names_come_from_the_edid() {
    let mut edid = vec![0u8; 128];
    edid[..8].copy_from_slice(&[0, 255, 255, 255, 255, 255, 255, 0]);
    // "AOC": A=1, O=15, C=3.
    let maker: u16 = (1 << 10) | (15 << 5) | 3;
    edid[8..10].copy_from_slice(&maker.to_be_bytes());
    edid[72..77].copy_from_slice(&[0, 0, 0, 0xfc, 0]);
    edid[77..90].copy_from_slice(b"24G2W1G5\n    ");
    assert_eq!(displays::parse_edid(&edid).as_deref(), Some("AOC 24G2W1G5"));
    assert_eq!(displays::parse_edid(&edid[..64]), None);
    let fixture = Fixture::new("automation-displays");
    std::fs::write(fixture.0.join("outputs"), "eDP-1\nDP-2\n").unwrap();
    std::fs::write(fixture.0.join("output-names"), "DP-2=Dell U2720Q\n").unwrap();
    let found = displays::describe(&fixture.store());
    assert_eq!(found.len(), 2);
    assert_eq!(found[0].title(), "Dell U2720Q · DP-2");
    assert!(found[1].builtin && found[1].label.is_none());
}

#[test]
fn the_status_says_what_happens_next_and_why() {
    let mut automation = day_and_evening();
    let mut state = State::default();
    let now = at(2026, 10, 18, 12, 0);
    tick(&automation, &mut state, now);
    let words = |automation: &Automation, state: &State, manual: Option<i64>| {
        let status = status::status(automation, state, &Cet, now, manual, true);
        (
            status.headline,
            status.detail.unwrap_or_default(),
            status.tone,
        )
    };
    let (headline, _, tone) = words(&automation, &state, None);
    assert_eq!(headline, "Next: Evening at 19:00");
    assert_eq!(tone, status::Tone::Active);
    // A manual change after the last switch is named, with when it ends.
    let manual = at(2026, 10, 18, 11, 30).timestamp();
    let (_, detail, _) = words(&automation, &state, Some(manual));
    assert_eq!(
        detail,
        "You changed the desktop at 11:30 · it stays until the next switch"
    );
    automation.paused = Some(Pause {
        since: now.timestamp() - 60,
        until: Some(status::tomorrow(&Cet, now)),
        skip: false,
    });
    let (headline, detail, tone) = words(&automation, &state, None);
    assert_eq!(headline, "Paused until tomorrow");
    assert_eq!(detail, "Then Day tomorrow at 07:00");
    assert_eq!(tone, status::Tone::Paused);
    automation.paused = Some(Pause {
        since: now.timestamp() - 60,
        until: Some(at(2026, 10, 18, 19, 0).timestamp() + 1),
        skip: true,
    });
    let (headline, detail, _) = words(&automation, &state, None);
    assert_eq!(headline, "Skipping Evening at 19:00");
    assert_eq!(detail, "Then Day tomorrow at 07:00");
    // With display rules, the skip says they still switch.
    let mut with_display = automation.clone();
    with_display
        .rules
        .push(display("r3", Change::Connect, None, "Workbench"));
    assert_eq!(
        words(&with_display, &state, None).1,
        "Then Day tomorrow at 07:00 · display rules still switch"
    );
    // Waiting for Settings: both ways out are named.
    let mut waiting = state.clone();
    let due = Due {
        revision: None,
        rule: "r2".into(),
        profile: ProfileRef::Preset("evening".into()),
        name: "Evening".into(),
        cause: Cause::Schedule("2026-10-18T19:00:00".into()),
        at: at(2026, 10, 18, 19, 0).timestamp(),
    };
    waiting.pending = Some(due.clone());
    waiting.record(Outcome {
        transaction: None,
        at: due.at,
        due: due.clone(),
        effect: Effect::Waiting {
            parts: vec!["Dock".into()],
        },
    });
    automation.paused = None;
    let (headline, detail, tone) = words(&automation, &waiting, None);
    assert_eq!(headline, "Evening is waiting");
    assert_eq!(
        detail,
        "Settings has unapplied Dock changes. Discard them to switch, or apply them to keep yours instead."
    );
    assert_eq!(tone, status::Tone::Attention);
    assert_eq!(
        status::outcome_line(&waiting.recent[0], true)
            .why
            .as_deref(),
        Some(detail.as_str())
    );
    // Applying them was a change of the person's own: kept, and said why.
    let kept = status::outcome_line(
        &Outcome {
            transaction: None,
            at: due.at + 180,
            due,
            effect: Effect::KeptManual {
                manual_at: at(2026, 10, 18, 19, 3).timestamp(),
            },
        },
        true,
    );
    assert_eq!(kept.what, "Kept your changes instead of Evening");
    assert_eq!(
        kept.why.as_deref(),
        Some("You changed the desktop after it was due.")
    );
    automation.paused = None;
    automation.enabled = false;
    assert_eq!(words(&automation, &state, None).0, "Off");
    let mut displays_only = on(vec![display("r1", Change::Connect, None, "Workbench")]);
    assert_eq!(
        words(&displays_only, &state, None).0,
        "Watching for display changes"
    );
    displays_only.rules[0].enabled = false;
    assert_eq!(words(&displays_only, &state, None).0, "No rules turned on");
}

mod runtime_loop {
    use super::*;
    use crate::automation::runtime::{Automator, HOLD_CHECK, SETTLE};
    use std::collections::BTreeSet;
    use std::time::Instant;

    fn held() -> BTreeSet<Domain> {
        [
            Domain::WidgetLayout,
            Domain::Appearance,
            Domain::Dock,
            Domain::Panel,
        ]
        .into_iter()
        .collect()
    }
    /// Presentation, due at 19:00 on Jan 1 2020, waits for held Settings
    /// parts. (In 2020, so every real change the test makes comes after.)
    fn waiting(fixture: &Fixture, store: &crate::desktop::DesktopStore, now: Instant) -> Automator {
        configure(
            store,
            vec![schedule("r1", 19, 0, Days::EVERY, "Presentation")],
        );
        let mut automator = Automator::with_store(Some(fixture.store()));
        automator.evaluate_with(&utc0(), utc(2020, 1, 1, 18, 0), now, Vec::new);
        let job = automator
            .evaluate_with(&utc0(), utc(2020, 1, 1, 19, 0), now, Vec::new)
            .unwrap();
        assert!(!job.held.is_empty());
        let results = job.run(store);
        assert!(matches!(results[0].1, Ran::Waiting(_)), "{results:?}");
        automator.finished(results, utc(2020, 1, 1, 19, 0).timestamp(), now);
        assert!(automator.state.pending.is_some());
        automator
    }
    fn writes_hold_of(store: &crate::desktop::DesktopStore, pid: u32, parts: &BTreeSet<Domain>) {
        let hold = Hold {
            expires: now() + 10,
            pid,
            parts: parts.iter().map(|part| part.key().to_string()).collect(),
            applied: None,
        };
        let file = path(store, HOLD_FILE);
        std::fs::create_dir_all(file.parent().unwrap()).unwrap();
        std::fs::write(file, serde_json::to_vec(&hold).unwrap()).unwrap();
    }

    fn configure(store: &crate::desktop::DesktopStore, rules: Vec<Rule>) {
        update(store, |automation| {
            *automation = on(rules);
            Ok(())
        })
        .unwrap();
    }
    fn utc0() -> FixedOffset {
        FixedOffset::east_opt(0).unwrap()
    }
    fn names(job: &Option<runtime::Job>) -> Vec<String> {
        job.iter()
            .flat_map(|job| &job.due)
            .map(|due| due.name.clone())
            .collect()
    }

    #[test]
    fn task2_invalid_ownership_does_not_consume_event_or_strand_scheduler() {
        let (fixture, store) = desk("ownership-read-retry");
        configure(
            &store,
            vec![schedule("r1", 19, 0, Days::EVERY, "Presentation")],
        );
        let mut automator = Automator::with_store(Some(fixture.store()));
        let instant = Instant::now();
        automator.evaluate_with(&utc0(), utc(2026, 10, 18, 18, 59), instant, Vec::new);
        let before = serde_json::to_value(&automator.state).unwrap();
        let epoch = store.state_dir().join("restore-epoch.json");
        std::fs::write(&epoch, b"{damaged").unwrap();
        assert!(
            automator
                .evaluate_with(&utc0(), utc(2026, 10, 18, 19, 0), instant, Vec::new)
                .is_none()
        );
        assert_eq!(
            serde_json::to_value(&automator.state).unwrap(),
            before,
            "unreadable ownership is not permission to consume an event"
        );
        assert!(
            automator.wake().is_some(),
            "must retry instead of staying busy"
        );
        std::fs::remove_file(epoch).unwrap();
        assert!(
            automator
                .evaluate_with(&utc0(), utc(2026, 10, 18, 19, 0), instant, Vec::new)
                .is_some()
        );
    }

    #[test]
    fn without_settings_the_desktop_does_nothing_and_writes_nothing() {
        let (fixture, store) = desk("runtime-off");
        let before = fixture.tree();
        let mut automator = Automator::with_store(Some(store));
        let now = Instant::now();
        assert!(
            automator
                .evaluate_with(&utc0(), utc(2026, 10, 18, 19, 0), now, Vec::new)
                .is_none()
        );
        assert_eq!(automator.wake(), None);
        assert_eq!(automator.problem(), None);
        assert_eq!(fixture.tree(), before);
        // Preview mode (no writable store): nothing either.
        let mut preview = Automator::with_store(None);
        assert!(
            preview
                .evaluate_with(&utc0(), utc(2026, 10, 18, 19, 0), now, Vec::new)
                .is_none()
        );
        assert!(preview.paths().is_empty());
    }

    #[test]
    fn a_scheduled_switch_runs_once_and_survives_a_restart() {
        let (fixture, store) = desk("runtime-schedule");
        configure(
            &store,
            vec![schedule("r1", 19, 0, Days::EVERY, "Presentation")],
        );
        let mut automator = Automator::with_store(Some(
            fixture
                .store()
                .with_app_config(fixture.0.join("app/config.json")),
        ));
        let now = Instant::now();
        assert!(
            automator
                .evaluate_with(&utc0(), utc(2026, 10, 18, 18, 59), now, Vec::new)
                .is_none()
        );
        // The wake is the switch itself (under a minute away).
        let wake = automator.wake().unwrap();
        assert!(wake > now && wake <= now + std::time::Duration::from_secs(61));
        let job = automator.evaluate_with(&utc0(), utc(2026, 10, 18, 19, 0), now, Vec::new);
        assert_eq!(names(&job), ["Presentation"]);
        // Busy: a second look waits for the job.
        assert!(
            automator
                .evaluate_with(&utc0(), utc(2026, 10, 18, 19, 0), now, Vec::new)
                .is_none()
        );
        let results = job.unwrap().run(&store);
        assert!(matches!(results[0].1, Ran::Applied { .. }));
        let change = match &results[0].1 {
            Ran::Applied { change, .. } => change.clone(),
            other => panic!("{other:?}"),
        };
        assert!(store.latest_change().unwrap().is_none());
        automator.finished(results, utc(2026, 10, 18, 19, 0).timestamp(), now);
        assert_eq!(automator.state.applied.as_deref(), Some(change.as_str()));
        assert!(matches!(
            automator.state.last().unwrap().effect,
            Effect::Applied { .. }
        ));
        assert!(
            automator
                .evaluate_with(&utc0(), utc(2026, 10, 18, 19, 1), now, Vec::new)
                .is_none()
        );
        // A restarted desktop reads the saved state: no repeat.
        let mut restarted = Automator::with_store(Some(fixture.store()));
        assert_eq!(restarted.state.applied.as_deref(), Some(change.as_str()));
        assert!(
            restarted
                .evaluate_with(&utc0(), utc(2026, 10, 18, 19, 3), now, Vec::new)
                .is_none()
        );
        // Its own change is not mistaken for a manual one tomorrow.
        let job = restarted.evaluate_with(&utc0(), utc(2026, 10, 19, 19, 0), now, Vec::new);
        assert_eq!(names(&job), ["Presentation"]);
    }

    #[test]
    fn display_changes_settle_before_they_count() {
        let (fixture, store) = desk("runtime-displays");
        configure(
            &store,
            vec![display("r1", Change::Connect, Some("DP-2"), "Workbench")],
        );
        let mut automator = Automator::with_store(Some(fixture.store()));
        let start = Instant::now();
        let at = |seconds: u64| start + std::time::Duration::from_secs(seconds);
        let now = utc(2026, 10, 18, 9, 0);
        let look = |automator: &mut Automator, seconds: u64, displays: &[&str]| {
            let displays: Vec<String> = displays.iter().map(|name| name.to_string()).collect();
            let job = automator.evaluate_with(
                &utc0(),
                now + Duration::seconds(seconds as i64),
                at(seconds),
                move || displays,
            );
            names(&job)
        };
        assert!(look(&mut automator, 0, &["eDP-1"]).is_empty());
        assert_eq!(automator.wake(), Some(at(0) + SETTLE));
        assert!(look(&mut automator, 5, &["eDP-1"]).is_empty());
        // The dock flaps: plugged, gone, plugged again within seconds.
        automator.displays_changed(at(10));
        assert!(look(&mut automator, 10, &["DP-2", "eDP-1"]).is_empty());
        automator.displays_changed(at(12));
        assert!(look(&mut automator, 12, &["eDP-1"]).is_empty());
        automator.displays_changed(at(13));
        assert!(look(&mut automator, 13, &["DP-2", "eDP-1"]).is_empty());
        assert!(look(&mut automator, 16, &["DP-2", "eDP-1"]).is_empty());
        // Stable for the settle time: one switch.
        assert_eq!(look(&mut automator, 18, &["DP-2", "eDP-1"]), ["Workbench"]);
        automator.finished(Vec::new(), now.timestamp(), at(18));
        assert!(look(&mut automator, 80, &["DP-2", "eDP-1"]).is_empty());
    }

    #[test]
    fn applying_the_held_dock_draft_keeps_it_and_skips_the_waiting_switch() {
        let (fixture, store) = desk("runtime-hold-apply");
        write_hold(&store, &held()).unwrap();
        let now = Instant::now();
        let mut automator = waiting(&fixture, &store, now);
        // The person applies their Dock draft (Dock & Panel's own Apply).
        let mut patch = crate::desktop::DesktopPatch::new();
        patch.set(
            crate::desktop::ConfigKey::new("com.system76.CosmicPanel.Dock", 1, "anchor").unwrap(),
            None,
            Some(crate::desktop::RawValue::parse("Left").unwrap()),
        );
        store
            .apply_owned(&patch, None, Owner::DockPanel, "Dock & Panel")
            .unwrap();
        let mut rest = held();
        rest.remove(&Domain::Dock);
        write_hold(&store, &rest).unwrap();
        assert!(automator.reload());
        let after = fixture.tree();
        assert!(
            automator
                .evaluate_with(&utc0(), utc(2020, 1, 1, 19, 4), now, Vec::new)
                .is_none()
        );
        assert_eq!(automator.state.pending, None);
        let last = automator.state.last().unwrap();
        assert!(matches!(last.effect, Effect::KeptManual { .. }));
        let line = status::outcome_line(last, true);
        assert_eq!(line.what, "Kept your changes instead of Presentation");
        assert_eq!(
            line.why.as_deref(),
            Some("You changed the desktop after it was due.")
        );
        // Nothing but the state file was written; the Dock stays as applied.
        let changed: Vec<_> = fixture
            .tree()
            .into_iter()
            .filter(|(path, bytes)| after.get(path) != Some(bytes))
            .map(|(path, _)| path)
            .collect();
        assert_eq!(changed, [path(&store, STATE_FILE)]);
        // Discarding the other drafts later doesn't bring it back.
        clear_hold(&store).unwrap();
        assert!(automator.reload());
        assert!(
            automator
                .evaluate_with(&utc0(), utc(2020, 1, 1, 19, 6), now, Vec::new)
                .is_none()
        );
    }

    #[test]
    fn architecture_current_layout_save_uses_durable_manual_history() {
        let (fixture, store) = desk("current-layout-hold");
        write_hold(&store, &held()).unwrap();
        let now = Instant::now();
        let mut automator = waiting(&fixture, &store, now);
        let mut widgets = crate::desktop::store::WidgetPatch::new();
        widgets
            .set(
                "/widget_positions/clock",
                None,
                Some(serde_json::json!([0.2, 0.3])),
            )
            .unwrap();
        crate::desktop::actions::commit(
            &store,
            &crate::desktop::DesktopPatch::new(),
            Some(&widgets),
            Owner::Preferences,
            "Widget arrangement",
            crate::desktop::store::Source::Manual,
        )
        .unwrap();
        assert!(store.manual_at().is_some());
        write_hold(&store, &BTreeSet::new()).unwrap();
        assert!(hold_applied(&store).is_none());
        assert!(!path(&store, HOLD_FILE).exists());
        automator.reload();
        assert!(
            automator
                .evaluate_with(&utc0(), utc(2020, 1, 1, 19, 3), now, Vec::new)
                .is_none()
        );
        assert!(matches!(
            automator.state.last().unwrap().effect,
            Effect::KeptManual { .. }
        ));
    }

    #[test]
    fn a_legacy_layout_stamp_survives_upgrade_and_keeps_the_waiting_switch() {
        let (fixture, store) = desk("runtime-hold-layout");
        write_hold(&store, &held()).unwrap();
        let now = Instant::now();
        let mut automator = waiting(&fixture, &store, now);
        // An older release left this stamp without a store history entry.
        let stamped = utc(2020, 1, 1, 19, 2).timestamp();
        write(
            &store,
            HOLD_FILE,
            &serde_json::to_vec(&Hold {
                expires: super::super::now().saturating_add(10),
                pid: std::process::id(),
                parts: vec![],
                applied: Some(stamped),
            })
            .unwrap(),
        )
        .unwrap();
        write_hold(&store, &BTreeSet::new()).unwrap();
        assert_eq!(hold_applied(&store), Some(stamped));
        assert!(automator.reload());
        assert!(
            automator
                .evaluate_with(&utc0(), utc(2020, 1, 1, 19, 3), now, Vec::new)
                .is_none()
        );
        assert!(matches!(
            automator.state.last().unwrap().effect,
            Effect::KeptManual { .. }
        ));
    }

    #[test]
    fn a_hold_left_by_a_settings_window_that_died_is_released_by_itself() {
        let (fixture, store) = desk("runtime-hold-dead");
        // A stand-in Settings process (its command line has `--settings`).
        let mut settings = std::process::Command::new("sh")
            .args(["-c", "sleep 60; true", "--settings"])
            .spawn()
            .unwrap();
        // Until the child has started `sh`, its command line is still this
        // test's own; wait for the stand-in to be what it claims to be.
        let started = Instant::now();
        while !std::fs::read(format!("/proc/{}/cmdline", settings.id())).is_ok_and(|cmdline| {
            cmdline
                .split(|byte| *byte == 0)
                .any(|arg| arg == b"--settings")
        }) {
            assert!(
                started.elapsed() < std::time::Duration::from_secs(5),
                "the stand-in never started"
            );
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        writes_hold_of(&store, settings.id(), &held());
        assert_eq!(read_hold(&store), held());
        let now = Instant::now();
        let mut automator = waiting(&fixture, &store, now);
        // Still running: it waits, and looks again soon.
        assert!(
            automator
                .evaluate_with(&utc0(), utc(2020, 1, 1, 19, 1), now, Vec::new)
                .is_none()
        );
        assert!(
            automator
                .wake()
                .is_some_and(|wake| wake <= now + HOLD_CHECK)
        );
        // The window is killed; the hold file stays as it was (no file
        // event, no reload). The next ordinary look releases it.
        settings.kill().unwrap();
        settings.wait().unwrap();
        let job = automator
            .evaluate_with(&utc0(), utc(2020, 1, 1, 19, 2), now, Vec::new)
            .unwrap();
        assert!(job.held.is_empty());
        let results = job.run(&store);
        assert!(matches!(results[0].1, Ran::Applied { .. }), "{results:?}");
        automator.finished(results, utc(2020, 1, 1, 19, 2).timestamp(), now);
        assert_eq!(automator.state.pending, None);
    }

    #[test]
    fn a_newer_switch_in_the_same_look_replaces_one_that_waits() {
        let (fixture, store) = desk("runtime-supersede");
        configure(
            &store,
            vec![
                schedule("r1", 7, 0, Days::EVERY, "Day"),
                display("r2", Change::Connect, None, "Workbench"),
            ],
        );
        let mut automator = Automator::with_store(Some(fixture.store()));
        let now = Instant::now();
        let due = |rule: &str, name: &str, cause: Cause, at: DateTime<Utc>| Due {
            revision: None,
            rule: rule.into(),
            profile: ProfileRef::Preset(name.to_lowercase()),
            name: name.into(),
            cause,
            at: at.timestamp(),
        };
        let day = due(
            "r1",
            "Day",
            Cause::Schedule("2026-10-19T07:00:00".into()),
            utc(2026, 10, 19, 7, 0),
        );
        let workbench = due(
            "r2",
            "Workbench",
            Cause::Display(DisplayWhen {
                change: Change::Connect,
                display: None,
                label: None,
            }),
            utc(2026, 10, 19, 8, 0),
        );
        // Booted docked after a missed 07:00: Day waits for a Dock draft,
        // Workbench (no Dock) applies.
        automator.finished(
            vec![
                (day, Ran::Waiting([Domain::Dock].into_iter().collect())),
                (
                    workbench,
                    Ran::Applied {
                        change: "c1".into(),
                        skipped: Vec::new(),
                    },
                ),
            ],
            utc(2026, 10, 19, 8, 0).timestamp(),
            now,
        );
        assert_eq!(automator.state.pending, None);
        let lines: Vec<String> = automator
            .state
            .recent
            .iter()
            .map(|outcome| status::outcome_line(outcome, true).what)
            .collect();
        assert_eq!(
            lines,
            [
                "Applied Workbench",
                "Day was replaced by Workbench",
                "Day is waiting"
            ]
        );
        // The Dock draft goes away: Day does not come back over Workbench.
        assert!(
            automator
                .evaluate_with(&utc0(), utc(2026, 10, 19, 8, 5), now, Vec::new)
                .is_none()
        );
    }

    #[test]
    fn a_switch_waits_for_settings_drafts_then_runs_or_respects_a_manual_apply() {
        let (fixture, store) = desk("runtime-hold");
        configure(
            &store,
            vec![schedule("r1", 19, 0, Days::EVERY, "Presentation")],
        );
        let held = held();
        write_hold(&store, &held).unwrap();
        let mut automator = Automator::with_store(Some(fixture.store()));
        let now = Instant::now();
        automator.evaluate_with(&utc0(), utc(2026, 10, 18, 18, 59), now, Vec::new);
        let job = automator
            .evaluate_with(&utc0(), utc(2026, 10, 18, 19, 0), now, Vec::new)
            .unwrap();
        assert_eq!(job.held, held);
        let before = fixture.tree();
        let results = job.run(&store);
        assert!(matches!(results[0].1, Ran::Waiting(_)));
        automator.finished(results, utc(2026, 10, 18, 19, 0).timestamp(), now);
        assert!(automator.state.pending.is_some());
        let status = status::status(
            &load(&store).unwrap(),
            &automator.state,
            &utc0(),
            utc(2026, 10, 18, 19, 1),
            None,
            true,
        );
        assert_eq!(status.headline, "Presentation is waiting");
        // Nothing else was written but the state file.
        let changed: Vec<_> = fixture
            .tree()
            .into_iter()
            .filter(|(path, bytes)| before.get(path) != Some(bytes))
            .map(|(path, _)| path)
            .collect();
        assert_eq!(changed, [path(&store, STATE_FILE)]);
        // Still held: no retry on an ordinary tick.
        assert!(
            automator
                .evaluate_with(&utc0(), utc(2026, 10, 18, 19, 2), now, Vec::new)
                .is_none()
        );
        // Settings applied or discarded its drafts: tried again, and applied.
        clear_hold(&store).unwrap();
        assert!(automator.reload());
        let job = automator
            .evaluate_with(&utc0(), utc(2026, 10, 18, 19, 5), now, Vec::new)
            .unwrap();
        assert!(job.held.is_empty());
        let results = job.run(&store);
        assert!(matches!(results[0].1, Ran::Applied { .. }));
        automator.finished(results, utc(2026, 10, 18, 19, 5).timestamp(), now);
        assert_eq!(automator.state.pending, None);
        // Waiting again, but this time the person applies a profile in
        // Settings meanwhile: their change is kept. (A fresh desktop on an
        // earlier day, so the real time of that apply is later.)
        let (fixture, store) = desk("runtime-hold-manual");
        configure(
            &store,
            vec![schedule("r1", 19, 0, Days::EVERY, "Presentation")],
        );
        assert!(store.latest_change().unwrap().is_none());
        write_hold(&store, &held).unwrap();
        let mut automator = Automator::with_store(Some(fixture.store()));
        automator.evaluate_with(&utc0(), utc(2020, 1, 1, 18, 0), now, Vec::new);
        let job = automator
            .evaluate_with(&utc0(), utc(2020, 1, 1, 19, 0), now, Vec::new)
            .unwrap();
        let results = job.run(&store);
        automator.finished(results, utc(2020, 1, 1, 19, 0).timestamp(), now);
        assert!(automator.state.pending.is_some());
        // A manual apply of a look, as Setups offers it in this fixture.
        let evening = profiles::fitted(
            profiles::preset("evening").unwrap(),
            &Default::default(),
            &crate::desktop::wallpaper::bundled_dir(&store),
            false,
        )
        .profile;
        let installed = Installed::discover(&store);
        let selected = evening.domains().into_iter().collect();
        profiles::apply(
            &store,
            &profiles::plan(&store, &installed, &evening, &selected),
        )
        .unwrap();
        clear_hold(&store).unwrap();
        assert!(automator.reload());
        assert!(
            automator
                .evaluate_with(&utc0(), utc(2020, 1, 1, 19, 10), now, Vec::new)
                .is_none()
        );
        assert_eq!(automator.state.pending, None);
        assert!(matches!(
            automator.state.last().unwrap().effect,
            Effect::KeptManual { .. }
        ));
    }
}

#[test]
fn task2_queued_schedule_is_superseded_by_restore_but_future_event_runs() {
    // A real fitted Presentation needs the installed bar defaults. Keep
    // the post-Restore assertion about an actual successful future switch.
    let (_f, store) = desk("schedule-restore");
    crate::desktop::recovery::ensure_baseline(
        &store,
        crate::desktop::recovery::BaselineKind::CurrentDesktop,
    )
    .unwrap();
    let settings = update(&store, |settings| {
        *settings = on(vec![schedule("r1", 10, 0, Days::EVERY, "Presentation")]);
        Ok(())
    })
    .unwrap();
    let due = decide::Due {
        revision: Some(decide::Revision::new(&settings, &settings.rules[0])),
        rule: "r1".into(),
        profile: ProfileRef::Preset("presentation".into()),
        name: "Clean desktop".into(),
        cause: decide::Cause::Schedule("2026-10-23T10:00:00".into()),
        at: Utc::now().timestamp() - 1,
    };
    let job = runtime::Job {
        due: vec![due.clone()],
        held: Default::default(),
        epoch: store.restore_epoch().unwrap(),
    };
    let plan = crate::desktop::recovery::plan_restore(
        &store,
        &[crate::desktop::recovery::RecoveryDomain::DockPanel],
    )
    .unwrap();
    crate::desktop::recovery::apply_restore(&store, &plan).unwrap();
    assert!(matches!(job.run(&store)[0].1, Ran::KeptManual(_)));
    let mut future = due;
    future.at = Utc::now().timestamp() + 60;
    let job = runtime::Job {
        due: vec![future],
        held: Default::default(),
        epoch: store.restore_epoch().unwrap(),
    };
    let results = job.run(&store);
    assert!(
        matches!(results[0].1, Ran::Applied { .. } | Ran::AlreadySet),
        "{results:?}"
    );
}
#[test]
fn task2r1_journal_before_epoch_recovery_supersedes_queued_schedule() {
    // A real fitted Presentation needs the installed bar defaults. Keep
    // the post-Restore assertion about an actual successful future switch.
    let (f, store) = desk("schedule-restore");
    crate::desktop::recovery::ensure_baseline(
        &store,
        crate::desktop::recovery::BaselineKind::CurrentDesktop,
    )
    .unwrap();
    let settings = update(&store, |settings| {
        *settings = on(vec![schedule("r1", 10, 0, Days::EVERY, "Presentation")]);
        Ok(())
    })
    .unwrap();
    let due = decide::Due {
        revision: Some(decide::Revision::new(&settings, &settings.rules[0])),
        rule: "r1".into(),
        profile: ProfileRef::Preset("presentation".into()),
        name: "Clean desktop".into(),
        cause: decide::Cause::Schedule("2026-10-23T10:00:00".into()),
        at: Utc::now().timestamp() + 30,
    };
    let job = runtime::Job {
        due: vec![due.clone()],
        held: Default::default(),
        epoch: store.restore_epoch().unwrap(),
    };
    let plan = crate::desktop::recovery::plan_restore(
        &store,
        &[crate::desktop::recovery::RecoveryDomain::DockPanel],
    )
    .unwrap();
    store.inject(Some(crate::desktop::store::Fault::InterruptBeforeEpoch));
    assert!(crate::desktop::recovery::apply_restore(&store, &plan).is_err());
    let store = f.store().with_app_config(f.0.join("app/config.json"));
    store.recover().unwrap();
    assert!(matches!(job.run(&store)[0].1, Ran::KeptManual(_)));
    let mut future = due;
    future.at = Utc::now().timestamp() + 60;
    let job = runtime::Job {
        due: vec![future],
        held: Default::default(),
        epoch: store.restore_epoch().unwrap(),
    };
    let results = job.run(&store);
    assert!(
        matches!(results[0].1, Ran::Applied { .. } | Ran::AlreadySet),
        "{results:?}"
    );
}

#[test]
fn auditfix_set04_pending_revision_survives_restart_only_when_unchanged() {
    for edit in [
        "unchanged",
        "time",
        "days",
        "display",
        "rule_since",
        "activation",
        "disabled",
        "missing",
        "legacy",
    ] {
        let (fixture, store) = desk(&format!("auditfix-pending-{edit}"));
        update(&store, |settings| {
            *settings = on(vec![schedule("r1", 7, 0, Days::EVERY, "Presentation")]);
            Ok(())
        })
        .unwrap();
        let settings = load(&store).unwrap();
        let mut state = State::default();
        tick(&settings, &mut state, at(2026, 10, 18, 6, 59));
        let event = decide(
            &settings,
            &mut state,
            &Cet,
            at(2026, 10, 18, 7, 0),
            None,
            || None,
        )
        .due
        .pop()
        .unwrap();
        state.pending = Some(event.clone());
        if edit == "legacy" {
            state.pending.as_mut().unwrap().revision = None;
        }
        save_state(&store, &state).unwrap();
        update(&store, |settings| {
            match edit {
                "time" => {
                    settings.rules[0].when = Trigger::Schedule {
                        time: Time::new(9, 0).unwrap(),
                        days: Days::EVERY,
                    }
                }
                // Same since and an occurrence still included by the new rule:
                // full trigger identity, not occurrence membership, matters.
                "days" => {
                    settings.rules[0].when = Trigger::Schedule {
                        time: Time::new(7, 0).unwrap(),
                        days: Days::WEEKEND,
                    }
                }
                "display" => {
                    settings.rules[0].when =
                        display("r1", Change::Connect, Some("DP-2"), "Presentation").when
                }
                "rule_since" => settings.rules[0].since = at(2026, 10, 18, 8, 0).timestamp(),
                "activation" => settings.since = at(2026, 10, 18, 8, 0).timestamp(),
                "disabled" => settings.enabled = false,
                "missing" => settings.rules.clear(),
                _ => {}
            }
            Ok(())
        })
        .unwrap();
        let mut restarted = runtime::Automator::with_store(Some(store.clone()));
        let job = restarted.evaluate_with(
            &Cet,
            at(2026, 10, 18, 8, 5),
            std::time::Instant::now(),
            Vec::new,
        );
        if edit == "unchanged" {
            assert_eq!(job.unwrap().due, [event]);
        } else {
            assert!(
                job.is_none(),
                "{edit} must not replay the waiting occurrence"
            );
            assert!(restarted.state.pending.is_none());
            assert!(
                load_state(&store).pending.is_none(),
                "invalidation is durable"
            );
        }
        assert!(store.latest_change().unwrap().is_none());
        drop(fixture);
    }
}

#[test]
fn auditfix_set04_display_trigger_revision_invalidates_persisted_pending() {
    let mut settings = on(vec![display(
        "r1",
        Change::Connect,
        Some("DP-1"),
        "Presentation",
    )]);
    let mut state = State::default();
    watch(&settings, &mut state, at(2026, 10, 18, 7, 0), &["eDP-1"]);
    let due = decide(
        &settings,
        &mut state,
        &Cet,
        at(2026, 10, 18, 7, 5),
        Some(&["eDP-1".into(), "DP-1".into()]),
        || None,
    )
    .due
    .pop()
    .unwrap();
    state.pending = Some(due);
    state = State::parse(&serde_json::to_vec(&state).unwrap());
    settings.rules[0].when = display("r1", Change::Disconnect, Some("DP-2"), "Presentation").when;
    let result = decide(
        &settings,
        &mut state,
        &Cet,
        at(2026, 10, 18, 8, 5),
        None,
        || None,
    );
    assert!(result.due.is_empty());
    assert!(result.persist);
    assert!(state.pending.is_none());
}

#[test]
fn auditfix_set04_queued_job_rechecks_rules_before_actual_execution() {
    for changed in [false, true] {
        let (fixture, store) = desk(&format!("auditfix-queued-rule-{changed}"));
        update(&store, |settings| {
            *settings = on(vec![schedule("r1", 7, 0, Days::EVERY, "Presentation")]);
            Ok(())
        })
        .unwrap();
        let mut runtime = runtime::Automator::with_store(Some(store.clone()));
        let instant = std::time::Instant::now();
        runtime.evaluate_with(&Cet, at(2026, 10, 18, 6, 59), instant, Vec::new);
        let job = runtime
            .evaluate_with(&Cet, at(2026, 10, 18, 7, 0), instant, Vec::new)
            .unwrap();
        // Change after queueing, without a watcher reload or manual desktop
        // write. It must be fenced by the worker, not just the pending UI.
        if changed {
            update(&store, |settings| {
                settings.rules[0].when = Trigger::Schedule {
                    time: Time::new(9, 0).unwrap(),
                    days: Days::EVERY,
                };
                Ok(())
            })
            .unwrap();
        }
        let before = fixture.tree();
        let result = job.run(&store);
        if changed {
            assert!(
                matches!(&result[0].1, Ran::Failed(reason) if reason.contains("rule changed")),
                "{result:?}"
            );
            assert_eq!(
                fixture.tree(),
                before,
                "no profile, journal, history or widget write"
            );
        } else {
            assert!(matches!(&result[0].1, Ran::Applied { .. }), "{result:?}");
        }
    }
}

#[test]
fn auditfix_set04_rule_saves_and_reset_share_execution_lock() {
    let (_fixture, store) = desk("auditfix-rule-execution-lock");
    update(&store, |settings| {
        *settings = on(vec![schedule("r1", 7, 0, Days::EVERY, "Presentation")]);
        Ok(())
    })
    .unwrap();
    let before = std::fs::read(path(&store, FILE)).unwrap();
    let lock = lock_rules(&store).unwrap();
    // A UI save must return busy while the execution fence is held; it
    // cannot publish a revised rule inside an older rule's transaction.
    let saved = crate::config::with_nonblocking_locks(|| {
        update(&store, |settings| {
            settings.rules[0].since = 42;
            Ok(())
        })
    });
    assert!(saved.is_err());
    assert!(crate::config::with_nonblocking_locks(|| reset(&store)).is_err());
    assert_eq!(std::fs::read(path(&store, FILE)).unwrap(), before);
    drop(lock);
    reset(&store).unwrap();
    assert_eq!(load(&store).unwrap(), Automation::default());
}

fn moment_rule(id: &str, h: u32, m: u32, phase: crate::moments::Phase) -> Rule {
    let mut rule = schedule(id, h, m, Days::EVERY, phase.title());
    rule.profile = ProfileRef::Moment(phase);
    rule
}
#[test]
fn fb15_moment_rule_versions_and_hyphenated_setup_ids_roundtrip() {
    let old = on(vec![schedule("r1", 7, 0, Days::EVERY, "Paper-studio")]);
    assert!(old.validate().is_ok());
    assert_eq!(
        Automation::parse(&old.to_bytes().unwrap()).unwrap().version,
        1
    );
    let mut rules = on(vec![moment_rule("r1", 7, 0, crate::moments::Phase::Day)]);
    assert!(
        rules.validate().is_err(),
        "version 1 cannot encode moment targets"
    );
    let bytes = rules.to_bytes().unwrap();
    let parsed = Automation::parse(&bytes).unwrap();
    assert_eq!(parsed.version, 2);
    assert_eq!(parsed.rules, rules.rules);
    rules.version = 4;
    assert!(Automation::parse(&serde_json::to_vec(&rules).unwrap()).is_err());
}
#[test]
fn fb15_moment_dst_resume_clock_rollback_priority_and_manual_policy() {
    use crate::moments::Phase;
    let mut rules = on(vec![
        moment_rule("r1", 2, 30, Phase::Day),
        moment_rule("r2", 2, 30, Phase::Night),
    ]);
    rules.version = 2;
    let mut state = State::default();
    tick(&rules, &mut state, at(2026, 3, 28, 22, 0));
    assert!(tick(&rules, &mut state, at(2026, 3, 29, 1, 59)).is_empty());
    assert_eq!(tick(&rules, &mut state, at(2026, 3, 29, 3, 0)), ["Day"]);
    assert!(tick(&rules, &mut state, at(2026, 3, 29, 4, 0)).is_empty());
    state = State::default();
    tick(&rules, &mut state, at(2026, 10, 24, 22, 0));
    assert_eq!(tick(&rules, &mut state, at(2026, 10, 25, 2, 30)), ["Day"]);
    assert!(tick(&rules, &mut state, utc(2026, 10, 25, 1, 30)).is_empty());
    assert!(tick(&rules, &mut state, at(2026, 10, 25, 1, 0)).is_empty());
    assert!(tick(&rules, &mut state, utc(2026, 10, 25, 1, 30)).is_empty());
    rules.rules[1].when = Trigger::Schedule {
        time: Time::new(19, 0).unwrap(),
        days: Days::EVERY,
    };
    state = State::default();
    tick(&rules, &mut state, at(2026, 10, 25, 1, 0));
    assert_eq!(tick(&rules, &mut state, at(2026, 10, 25, 21, 0)), ["Night"]);
    state = State::default();
    tick(&rules, &mut state, at(2026, 10, 25, 1, 0));
    let result = decide(
        &rules,
        &mut state,
        &Cet,
        at(2026, 10, 25, 21, 0),
        None,
        || Some(at(2026, 10, 25, 20, 0)),
    );
    assert!(result.due.is_empty());
}
#[test]
fn fb15_moment_activation_only_writes_selector_and_is_idempotent() {
    let (fixture, store) = desk("fb15-run");
    let installed = Installed::discover(&store);
    let before: serde_json::Value = serde_json::from_str(&config_text(&fixture)).unwrap();
    let target = ProfileRef::Moment(crate::moments::Phase::Day);
    let result = run(&store, &installed, &target, "Day", &Default::default());
    assert!(matches!(result, Ran::Applied { .. }), "{result:?}");
    let after: serde_json::Value = serde_json::from_str(&config_text(&fixture)).unwrap();
    for (k, v) in before.as_object().unwrap() {
        assert_eq!(after.get(k), Some(v));
    }
    assert_eq!(after["moments"], serde_json::json!({"active":"day"}));
    let before = fixture.tree();
    assert_eq!(
        run(&store, &installed, &target, "Day", &Default::default()),
        Ran::AlreadySet
    );
    assert_eq!(fixture.tree(), before);
}
#[test]
fn fb15_queued_moment_respects_manual_edit_rule_revision_and_restore() {
    use crate::moments::Phase;
    for cause in ["manual", "revision", "restore", "none"] {
        let (fixture, store) = desk(&format!("fb15-{cause}"));
        crate::desktop::recovery::ensure_baseline(
            &store,
            crate::desktop::recovery::BaselineKind::CurrentDesktop,
        )
        .unwrap();
        let settings = update(&store, |s| {
            *s = on(vec![moment_rule("r1", 7, 0, Phase::Day)]);
            Ok(())
        })
        .unwrap();
        let due = decide::Due {
            revision: Some(decide::Revision::new(&settings, &settings.rules[0])),
            rule: "r1".into(),
            profile: ProfileRef::Moment(Phase::Day),
            name: "Day".into(),
            cause: decide::Cause::Schedule("2026-10-27T07:00:00".into()),
            at: Utc::now().timestamp() - 1,
        };
        let job = runtime::Job {
            due: vec![due],
            held: Default::default(),
            epoch: store.restore_epoch().unwrap(),
        };
        match cause {
            "manual" => {
                let mut p = Profile::new("Night").unwrap();
                p.widget_style = Some(serde_json::json!({"moments":{"active":"night"}}));
                let plan = profiles::plan(
                    &store,
                    &Installed::discover(&store),
                    &p,
                    &[Domain::WidgetStyle].into(),
                );
                profiles::apply(&store, &plan).unwrap();
            }
            "revision" => {
                update(&store, |s| {
                    s.rules[0].profile = ProfileRef::Moment(Phase::Night);
                    Ok(())
                })
                .unwrap();
            }
            "restore" => {
                let plan = crate::desktop::recovery::plan_restore(
                    &store,
                    &[crate::desktop::recovery::RecoveryDomain::WidgetAppearance],
                )
                .unwrap();
                crate::desktop::recovery::apply_restore(&store, &plan).unwrap();
            }
            _ => {}
        }
        let before = fixture.tree();
        let results = job.run(&store);
        match cause {
            "none" => assert!(matches!(results[0].1, Ran::Applied { .. }), "{results:?}"),
            "revision" => assert!(
                matches!(&results[0].1,Ran::Failed(s) if s.contains("rule changed")),
                "{results:?}"
            ),
            _ => assert!(
                matches!(results[0].1, Ran::KeptManual(_)),
                "{cause}: {results:?}"
            ),
        }
        if cause != "none" {
            assert_eq!(fixture.tree(), before);
        }
    }
}

#[test]
fn adaptive_actual_switch_preserves_content_placement_and_native_mode() {
    use crate::desktop::profiles::adaptive::{Look, Phase};
    let (fixture, store) = desk("adaptive-apply");
    let installed = Installed::discover(&store);
    let before: serde_json::Value = serde_json::from_str(&config_text(&fixture)).unwrap();
    let mode = std::fs::read(
        store.user_path(
            &crate::desktop::store::ConfigKey::new("com.system76.CosmicTheme.Mode", 1, "is_dark")
                .unwrap(),
        ),
    )
    .unwrap();
    for phase in Phase::ALL {
        let target = ProfileRef::Preset(Look::Tidal.id(phase).into());
        let result = run(&store, &installed, &target, "Adaptive", &Default::default());
        assert!(matches!(result, Ran::Applied { .. }), "{result:?}");
        let after: serde_json::Value = serde_json::from_str(&config_text(&fixture)).unwrap();
        for key in ["enabled_widgets", "location", "future_field"] {
            assert_eq!(after[key], before[key]);
        }
        assert_eq!(
            std::fs::read(
                store.user_path(
                    &crate::desktop::store::ConfigKey::new(
                        "com.system76.CosmicTheme.Mode",
                        1,
                        "is_dark"
                    )
                    .unwrap()
                )
            )
            .unwrap(),
            mode
        );
        assert_eq!(
            run(&store, &installed, &target, "Adaptive", &Default::default()),
            Ran::AlreadySet
        );
    }
}
