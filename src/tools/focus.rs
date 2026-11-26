//! The focus timer: focus rounds and breaks timed against the wall clock.
//!
//! A running round stores the moment it ends, not a countdown, so it stays
//! right across restarts, suspend and slow redraws: whatever happens, the
//! remaining time is that end minus now. The session (phase, end, rounds
//! done today) is runtime state in its own file, separate from the
//! durations and card options in `config.json`.
use crate::config::FocusPreferences;
use serde::{Deserialize, Serialize};
use std::path::Path;

const FILE: &str = "focus.json";
const FORMAT: &str = "desktop-cards-focus";
const MAX_BYTES: usize = 4096;
/// A round that ended longer ago than this (the computer was off or
/// asleep) is shown as done without a late notification.
pub const LATE_NOTICE_MS: i64 = 2 * 60 * 1000;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Phase {
    Focus,
    Break,
    LongBreak,
}
impl Phase {
    pub fn title(self) -> &'static str {
        match self {
            Self::Focus => "Focus",
            Self::Break => "Break",
            Self::LongBreak => "Long break",
        }
    }
    pub fn minutes(self, preferences: &FocusPreferences) -> u16 {
        match self {
            Self::Focus => preferences.focus_minutes,
            Self::Break => preferences.break_minutes,
            Self::LongBreak => preferences.long_break_minutes,
        }
    }
    fn duration_ms(self, preferences: &FocusPreferences) -> i64 {
        i64::from(self.minutes(preferences).max(1)) * 60_000
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum Status {
    /// Waiting to start the phase; its length follows the preferences.
    Ready,
    /// Counting down to `ends_at` (Unix milliseconds).
    Running {
        ends_at: i64,
    },
    Paused {
        remaining: i64,
    },
    /// The phase ended; the next one waits to be started.
    Done,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Session {
    pub format: String,
    pub phase: Phase,
    #[serde(flatten)]
    pub status: Status,
    /// Length of the round in progress, for its progress bar.
    pub length: i64,
    /// Focus rounds completed on `day`.
    pub rounds: u32,
    /// The local date (`YYYY-MM-DD`) `rounds` counts.
    pub day: String,
}

/// Something worth telling the user about.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Event {
    /// `phase` ended now (or recently enough to say so).
    Finished { phase: Phase, next: Phase },
}

impl Default for Session {
    fn default() -> Self {
        Self {
            format: FORMAT.into(),
            phase: Phase::Focus,
            status: Status::Ready,
            length: Phase::Focus.duration_ms(&FocusPreferences::default()),
            rounds: 0,
            day: String::new(),
        }
    }
}

impl Session {
    pub fn path() -> Option<std::path::PathBuf> {
        super::state_path(FILE)
    }
    /// The saved session; a missing, damaged or foreign file starts fresh.
    pub fn load() -> Self {
        Self::path()
            .map(|path| Self::load_from(&path))
            .unwrap_or_default()
    }
    pub fn load_from(path: &Path) -> Self {
        super::read_bounded(path, MAX_BYTES)
            .ok()
            .flatten()
            .and_then(|bytes| serde_json::from_slice::<Self>(&bytes).ok())
            .filter(|session| session.valid())
            .unwrap_or_default()
    }
    pub fn save(&self) -> Result<(), String> {
        let path = Self::path().ok_or("No state folder")?;
        self.save_to(&path)
    }
    pub fn save_to(&self, path: &Path) -> Result<(), String> {
        let bytes = serde_json::to_vec(self).map_err(|error| error.to_string())?;
        super::write_private(path, &bytes)
    }
    fn valid(&self) -> bool {
        let day = 24 * 3_600_000;
        self.format == FORMAT
            && (0..=day).contains(&self.length)
            && self.day.len() <= 10
            && match self.status {
                Status::Paused { remaining } => (0..=day).contains(&remaining),
                _ => true,
            }
    }

    pub fn running(&self) -> bool {
        matches!(self.status, Status::Running { .. })
    }
    /// The length the current phase has or would have.
    pub fn length(&self, preferences: &FocusPreferences) -> i64 {
        match self.status {
            Status::Ready => self.phase.duration_ms(preferences),
            _ => self.length.max(1),
        }
    }
    /// Milliseconds left, never negative and never more than the round.
    pub fn remaining(&self, now: i64, preferences: &FocusPreferences) -> i64 {
        let length = self.length(preferences);
        match self.status {
            Status::Ready => length,
            Status::Running { ends_at } => (ends_at - now).clamp(0, length),
            Status::Paused { remaining } => remaining.clamp(0, length),
            Status::Done => 0,
        }
    }
    /// Elapsed share of the round, 0–1.
    pub fn progress(&self, now: i64, preferences: &FocusPreferences) -> f32 {
        let length = self.length(preferences).max(1);
        (1. - self.remaining(now, preferences) as f32 / length as f32).clamp(0., 1.)
    }
    /// Rounds done today (an older day counts none).
    pub fn rounds_today(&self, today: &str) -> u32 {
        if self.day == today { self.rounds } else { 0 }
    }
    /// The phase after the current one.
    pub fn next_phase(&self, preferences: &FocusPreferences, today: &str) -> Phase {
        match self.phase {
            Phase::Focus => {
                // The round in progress counts once it is done.
                let done =
                    self.rounds_today(today) + u32::from(!matches!(self.status, Status::Done));
                let every = u32::from(preferences.long_break_every);
                if every > 0 && done > 0 && done % every == 0 {
                    Phase::LongBreak
                } else {
                    Phase::Break
                }
            }
            Phase::Break | Phase::LongBreak => Phase::Focus,
        }
    }

    fn begin(&mut self, phase: Phase, now: i64, preferences: &FocusPreferences) {
        self.phase = phase;
        self.length = phase.duration_ms(preferences);
        self.status = Status::Running {
            ends_at: now + self.length,
        };
    }
    /// Start, pause or resume, whichever the state calls for.
    pub fn toggle(&mut self, now: i64, preferences: &FocusPreferences, today: &str) {
        match self.status {
            Status::Ready => self.begin(self.phase, now, preferences),
            Status::Running { .. } => self.pause(now, preferences),
            Status::Paused { remaining } => {
                self.status = Status::Running {
                    ends_at: now + remaining,
                }
            }
            Status::Done => {
                let next = self.next_phase(preferences, today);
                self.begin(next, now, preferences);
            }
        }
    }
    /// Takes over a quick timer as a focus round: its time left, running or
    /// paused as it was, measured against its full length `total`.
    pub fn adopt(&mut self, total: i64, remaining: i64, running: bool, now: i64) {
        let day = 24 * 3_600_000;
        let remaining = remaining.clamp(1_000, day);
        self.phase = Phase::Focus;
        self.length = total.clamp(remaining, day);
        self.status = if running {
            Status::Running {
                ends_at: now + remaining,
            }
        } else {
            Status::Paused { remaining }
        };
    }
    pub fn pause(&mut self, now: i64, preferences: &FocusPreferences) {
        if self.running() {
            self.status = Status::Paused {
                remaining: self.remaining(now, preferences),
            };
        }
    }
    /// Stops the round and returns to a ready focus round. Rounds already
    /// done stay counted.
    pub fn reset(&mut self, preferences: &FocusPreferences) {
        self.phase = Phase::Focus;
        self.status = Status::Ready;
        self.length = Phase::Focus.duration_ms(preferences);
    }
    /// Leaves the current phase without counting it: the next phase waits
    /// to be started.
    pub fn skip(&mut self, preferences: &FocusPreferences, today: &str) {
        let next = match self.status {
            // A skipped focus round is not a completed one.
            Status::Done => self.next_phase(preferences, today),
            _ => match self.phase {
                Phase::Focus => Phase::Break,
                Phase::Break | Phase::LongBreak => Phase::Focus,
            },
        };
        self.phase = next;
        self.status = Status::Ready;
        self.length = next.duration_ms(preferences);
    }
    /// Advances a round whose end has passed. Returns what ended, when it
    /// ended recently enough to announce.
    pub fn tick(&mut self, now: i64, preferences: &FocusPreferences, today: &str) -> Option<Event> {
        let Status::Running { ends_at } = self.status else {
            return None;
        };
        if now < ends_at {
            return None;
        }
        let ended = self.phase;
        if ended == Phase::Focus
            && let Some(finished) = chrono::DateTime::from_timestamp_millis(ends_at)
        {
            // A persisted round may be observed days after it expired. Count
            // the local expiry date, never the date of restart or resume.
            let completed_on = finished
                .with_timezone(&chrono::Local)
                .format("%Y-%m-%d")
                .to_string();
            if self.day != completed_on {
                self.day = completed_on;
                self.rounds = 0;
            }
            self.rounds = self.rounds.saturating_add(1);
        }
        self.status = Status::Done;
        let next = self.next_phase(preferences, today);
        let recent = now.saturating_sub(ends_at) <= LATE_NOTICE_MS;
        if ended == Phase::Focus && preferences.auto_start_breaks && recent {
            // The break starts from the moment focus ended.
            self.begin(next, ends_at, preferences);
        }
        recent.then_some(Event::Finished { phase: ended, next })
    }
    /// When a running phase ends (Unix milliseconds).
    pub fn ends_at(&self) -> Option<i64> {
        match self.status {
            Status::Running { ends_at } => Some(ends_at),
            _ => None,
        }
    }
    /// When the visible countdown next changes (Unix milliseconds): the next
    /// whole second, or the end itself.
    pub fn next_change(&self, now: i64) -> Option<i64> {
        let Status::Running { ends_at } = self.status else {
            return None;
        };
        let left = (ends_at - now).max(0);
        let step = left % 1000;
        Some(now + if step == 0 { left.min(1000) } else { step })
    }
}

/// `m:ss`, or `h:mm:ss` from an hour, rounded up so a round never shows
/// 0:00 while it still runs.
pub fn clock(remaining_ms: i64) -> String {
    let seconds = (remaining_ms.max(0) + 999) / 1000;
    let (hours, minutes, seconds) = (seconds / 3600, seconds / 60 % 60, seconds % 60);
    if hours > 0 {
        format!("{hours}:{minutes:02}:{seconds:02}")
    } else {
        format!("{minutes}:{seconds:02}")
    }
}

/// The notification text for a finished phase.
pub fn announcement(event: Event, preferences: &FocusPreferences) -> (String, String) {
    let Event::Finished { phase, next } = event;
    let minutes = next.minutes(preferences);
    match phase {
        Phase::Focus => (
            "Focus round done".into(),
            if preferences.auto_start_breaks {
                format!(
                    "Your {minutes}-minute {} has started.",
                    next.title().to_lowercase()
                )
            } else {
                format!(
                    "Time for a {minutes}-minute {}.",
                    next.title().to_lowercase()
                )
            },
        ),
        Phase::Break | Phase::LongBreak => (
            "Break over".into(),
            format!("Ready for {minutes} minutes of focus."),
        ),
    }
}

pub fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|elapsed| elapsed.as_millis() as i64)
        .unwrap_or(0)
}

pub fn today() -> String {
    chrono::Local::now().format("%Y-%m-%d").to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_adopted_timer_runs_as_a_focus_round_and_counts_as_one() {
        let prefs = FocusPreferences::default();
        let mut session = Session::default();
        session.phase = Phase::Break;
        use chrono::TimeZone;
        let now = chrono::Local
            .with_ymd_and_hms(2026, 11, 25, 12, 0, 0)
            .unwrap()
            .timestamp_millis();
        // Forty minutes, thirty left, measured against the forty.
        session.adopt(2_400_000, 1_800_000, true, now);
        assert_eq!(session.phase, Phase::Focus);
        assert_eq!(session.remaining(now, &prefs), 1_800_000);
        assert!((session.progress(now, &prefs) - 0.25).abs() < 1e-6);
        let event = session.tick(now + 1_800_000, &prefs, "2026-11-25");
        assert!(matches!(
            event,
            Some(Event::Finished {
                phase: Phase::Focus,
                ..
            })
        ));
        assert_eq!(session.rounds_today("2026-11-25"), 1);
        // Paused stays paused; time left is never more than its round.
        session.adopt(60_000, 90_000, false, 0);
        assert_eq!(session.status, Status::Paused { remaining: 90_000 });
        assert_eq!(session.length, 90_000);
    }

    const MIN: i64 = 60_000;
    const DAY: &str = "2026-10-18";

    fn at(offset: i64) -> i64 {
        use chrono::TimeZone;
        chrono::Local
            .with_ymd_and_hms(2026, 10, 18, 12, 0, 0)
            .unwrap()
            .timestamp_millis()
            + offset
    }

    fn prefs() -> FocusPreferences {
        FocusPreferences::default()
    }

    #[test]
    fn a_round_runs_pauses_resumes_and_finishes_against_the_wall_clock() {
        let p = prefs();
        let mut s = Session::default();
        assert_eq!(s.remaining(at(0), &p), 25 * MIN);
        s.toggle(at(1_000), &p, DAY);
        assert_eq!(
            s.status,
            Status::Running {
                ends_at: at(1_000 + 25 * MIN)
            }
        );
        assert_eq!(s.remaining(at(1_000 + 5 * MIN), &p), 20 * MIN);
        // Pausing keeps the remaining time however long the pause lasts.
        s.toggle(at(1_000 + 5 * MIN), &p, DAY);
        assert_eq!(
            s.status,
            Status::Paused {
                remaining: 20 * MIN
            }
        );
        assert_eq!(s.tick(at(1_000 + 90 * MIN), &p, DAY), None);
        s.toggle(at(100 * MIN), &p, DAY);
        assert_eq!(s.remaining(at(100 * MIN), &p), 20 * MIN);
        assert_eq!(s.tick(at(120 * MIN - 1), &p, DAY), None);
        assert_eq!(
            s.tick(at(120 * MIN), &p, DAY),
            Some(Event::Finished {
                phase: Phase::Focus,
                next: Phase::Break
            })
        );
        assert_eq!(s.status, Status::Done);
        assert_eq!(s.rounds_today(DAY), 1);
        assert_eq!(s.remaining(at(120 * MIN), &p), 0);
        // Start the break from the finished state.
        s.toggle(at(121 * MIN), &p, DAY);
        assert_eq!(s.phase, Phase::Break);
        assert_eq!(s.remaining(at(121 * MIN), &p), 5 * MIN);
    }

    #[test]
    fn every_fourth_round_earns_a_long_break_and_skips_do_not_count() {
        let p = prefs();
        let mut s = Session::default();
        let mut now = at(0);
        for round in 1..=4 {
            assert_eq!(s.phase, Phase::Focus);
            s.toggle(now, &p, DAY);
            now += 25 * MIN;
            let expected = if round == 4 {
                Phase::LongBreak
            } else {
                Phase::Break
            };
            assert_eq!(
                s.tick(now, &p, DAY),
                Some(Event::Finished {
                    phase: Phase::Focus,
                    next: expected
                })
            );
            s.toggle(now, &p, DAY);
            assert_eq!(s.phase, expected);
            s.skip(&p, DAY);
        }
        assert_eq!(s.rounds_today(DAY), 4);
        // Skipping a running focus round goes to a break without counting it.
        s.toggle(now, &p, DAY);
        s.skip(&p, DAY);
        assert_eq!((s.phase, s.status), (Phase::Break, Status::Ready));
        assert_eq!(s.rounds_today(DAY), 4);
        // Reset returns to focus and keeps the count.
        s.reset(&p);
        assert_eq!((s.phase, s.status), (Phase::Focus, Status::Ready));
        assert_eq!(s.rounds_today(DAY), 4);
        // A new day starts counting again.
        assert_eq!(s.rounds_today("2026-10-19"), 0);
        let none = FocusPreferences {
            long_break_every: 0,
            ..p
        };
        assert_eq!(s.next_phase(&none, DAY), Phase::Break);
    }

    #[test]
    fn a_round_that_ended_while_away_is_done_without_a_late_notice() {
        let p = FocusPreferences {
            auto_start_breaks: true,
            ..prefs()
        };
        let mut s = Session::default();
        s.toggle(at(0), &p, DAY);
        // Back from a long suspend: done, quietly, and no break auto-starts.
        assert_eq!(s.tick(at(3 * 3_600_000), &p, DAY), None);
        assert_eq!(s.status, Status::Done);
        // A recent end auto-starts the break from the moment focus ended.
        let mut s = Session::default();
        s.toggle(at(0), &p, DAY);
        assert!(s.tick(at(25 * MIN + 500), &p, DAY).is_some());
        assert_eq!(s.phase, Phase::Break);
        assert_eq!(s.remaining(at(25 * MIN + 500), &p), 5 * MIN - 500);
    }

    #[test]
    fn a_clock_set_backwards_never_shows_more_than_the_round() {
        let p = prefs();
        let mut s = Session::default();
        s.toggle(10 * MIN, &p, DAY);
        assert_eq!(s.remaining(0, &p), 25 * MIN);
        assert!((s.progress(0, &p) - 0.).abs() < f32::EPSILON);
        assert!((s.progress(10 * MIN + 25 * MIN / 2, &p) - 0.5).abs() < 0.001);
    }

    #[test]
    fn ready_rounds_follow_new_durations_but_running_ones_keep_theirs() {
        let mut p = prefs();
        let mut s = Session::default();
        p.focus_minutes = 50;
        assert_eq!(s.remaining(0, &p), 50 * MIN);
        s.toggle(0, &p, DAY);
        p.focus_minutes = 15;
        assert_eq!(s.remaining(0, &p), 50 * MIN);
    }

    #[test]
    fn the_countdown_wakes_on_whole_seconds_and_rounds_up() {
        let p = prefs();
        let mut s = Session::default();
        s.toggle(0, &p, DAY);
        assert_eq!(s.next_change(250), Some(1_000));
        assert_eq!(s.next_change(1_000), Some(2_000));
        assert_eq!(s.next_change(25 * MIN - 200), Some(25 * MIN));
        assert_eq!(clock(25 * MIN), "25:00");
        assert_eq!(clock(59_001), "1:00");
        assert_eq!(clock(1), "0:01");
        assert_eq!(clock(0), "0:00");
        assert_eq!(clock(90 * MIN), "1:30:00");
        s.pause(1_000, &p);
        assert_eq!(s.next_change(1_000), None);
    }

    #[test]
    fn sessions_round_trip_and_damaged_files_start_fresh() {
        let dir = std::env::temp_dir().join(format!("desktop-cards-focus-{}", std::process::id()));
        let path = dir.join("focus.json");
        let p = prefs();
        let mut s = Session::default();
        s.toggle(5_000, &p, DAY);
        s.save_to(&path).unwrap();
        assert_eq!(Session::load_from(&path), s);
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(
            std::fs::metadata(&path).unwrap().permissions().mode() & 0o777,
            0o600
        );
        std::fs::write(&path, b"{\"format\":\"other\"}").unwrap();
        assert_eq!(Session::load_from(&path), Session::default());
        std::fs::write(&path, vec![b' '; MAX_BYTES + 1]).unwrap();
        assert_eq!(Session::load_from(&path), Session::default());
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn auditfix_w04_persisted_expiry_counts_its_local_day_once_after_restart() {
        use chrono::TimeZone;
        let p = FocusPreferences {
            auto_start_breaks: true,
            ..prefs()
        };
        let expiry = chrono::Local
            .with_ymd_and_hms(2026, 10, 24, 23, 50, 0)
            .unwrap()
            .timestamp_millis();
        let now = chrono::Local
            .with_ymd_and_hms(2026, 10, 25, 12, 0, 0)
            .unwrap()
            .timestamp_millis();
        let dir = std::env::temp_dir().join(format!("halcyon-auditfix-w04-{}", std::process::id()));
        let path = dir.join("focus.json");
        let mut session = Session {
            day: "2026-10-24".into(),
            rounds: 3,
            status: Status::Running { ends_at: expiry },
            ..Session::default()
        };
        session.save_to(&path).unwrap();
        session = Session::load_from(&path);
        assert_eq!(session.tick(now, &p, "2026-10-25"), None);
        assert_eq!(session.status, Status::Done);
        assert_eq!(session.rounds_today("2026-10-24"), 4);
        assert_eq!(session.rounds_today("2026-10-25"), 0);
        assert_eq!(session.next_phase(&p, "2026-10-25"), Phase::Break);
        session.save_to(&path).unwrap();
        session = Session::load_from(&path);
        assert_eq!(session.tick(now + MIN, &p, "2026-10-25"), None);
        assert_eq!(session.rounds, 4, "restarts cannot count the expiry twice");
        session.reset(&p);
        session.toggle(now, &p, "2026-10-25");
        assert_eq!(
            session.tick(now + 25 * MIN, &p, "2026-10-25"),
            Some(Event::Finished {
                phase: Phase::Focus,
                next: Phase::Break,
            })
        );
        assert_eq!(session.rounds_today("2026-10-25"), 1);
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn auditfix_w04_round_crossing_midnight_counts_the_expiry_day_and_notifies() {
        use chrono::TimeZone;
        let p = prefs();
        let start = chrono::Local
            .with_ymd_and_hms(2026, 10, 24, 23, 40, 0)
            .unwrap()
            .timestamp_millis();
        let mut session = Session {
            day: "2026-10-24".into(),
            rounds: 3,
            ..Session::default()
        };
        session.toggle(start, &p, "2026-10-24");
        let end = session.ends_at().unwrap();
        assert_eq!(
            session.tick(end + 500, &p, "2026-10-25"),
            Some(Event::Finished {
                phase: Phase::Focus,
                next: Phase::Break,
            })
        );
        assert_eq!(session.day, "2026-10-25");
        assert_eq!(session.rounds_today("2026-10-25"), 1);
    }

    #[test]
    fn announcements_name_the_next_phase() {
        let p = prefs();
        let (title, body) = announcement(
            Event::Finished {
                phase: Phase::Focus,
                next: Phase::Break,
            },
            &p,
        );
        assert_eq!(title, "Focus round done");
        assert_eq!(body, "Time for a 5-minute break.");
        let (title, body) = announcement(
            Event::Finished {
                phase: Phase::LongBreak,
                next: Phase::Focus,
            },
            &p,
        );
        assert_eq!(title, "Break over");
        assert_eq!(body, "Ready for 25 minutes of focus.");
    }
}
