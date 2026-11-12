//! The quick timer, separate from Focus. The desktop owns it, so it outlives
//! the island and `halcyon-ctl timer` reaches it through the command file.
//! Only the end time and a paused remainder are written to disk.
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// The longest timer: a day.
pub const MAX_MS: i64 = 24 * 3_600_000;
/// The picker's presets, in minutes.
pub const PRESETS: [i64; 4] = [1, 5, 10, 25];
/// A timer that ended longer ago than this while nothing ran is dropped
/// instead of announced.
const STALE_MS: i64 = 60_000;
const FILE: &str = "activity-island/timer.json";

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Timer {
    /// What the ring measures against.
    pub total_ms: i64,
    /// While running: when it ends, in Unix milliseconds.
    pub ends_at_ms: Option<i64>,
    /// While paused: what is left.
    pub paused_ms: i64,
    /// Reached zero; waits for Dismiss or +1 min.
    pub done: bool,
}
impl Timer {
    pub fn start(ms: i64, now: i64) -> Self {
        let ms = ms.clamp(1_000, MAX_MS);
        Self {
            total_ms: ms,
            ends_at_ms: Some(now + ms),
            paused_ms: 0,
            done: false,
        }
    }
    pub fn running(&self) -> bool {
        self.ends_at_ms.is_some()
    }
    pub fn paused(&self) -> bool {
        !self.done && self.ends_at_ms.is_none()
    }
    pub fn remaining(&self, now: i64) -> i64 {
        match self.ends_at_ms {
            _ if self.done => 0,
            Some(ends) => (ends - now).max(0),
            None => self.paused_ms,
        }
    }
    /// How much of the ring is still lit, 1 at the start.
    pub fn fraction(&self, now: i64) -> f32 {
        if self.total_ms <= 0 {
            return 0.;
        }
        (self.remaining(now) as f32 / self.total_ms as f32).clamp(0., 1.)
    }
    /// +1 min: extends a running or paused timer; a finished one starts again.
    pub fn add(&mut self, ms: i64, now: i64) {
        if self.done {
            *self = Self::start(ms, now);
            return;
        }
        let room = MAX_MS - self.remaining(now);
        let ms = ms.clamp(0, room.max(0));
        match &mut self.ends_at_ms {
            Some(ends) => *ends += ms,
            None => self.paused_ms += ms,
        }
        self.total_ms = (self.total_ms + ms).min(MAX_MS);
    }
    pub fn pause(&mut self, now: i64) {
        if let Some(ends) = self.ends_at_ms.take() {
            self.paused_ms = (ends - now).max(0);
        }
    }
    pub fn resume(&mut self, now: i64) {
        if self.paused() {
            self.ends_at_ms = Some(now + self.paused_ms);
            self.paused_ms = 0;
        }
    }
    /// Marks a running timer done once its end has passed. True when this
    /// call finished it: the moment to chime.
    pub fn expire(&mut self, now: i64) -> bool {
        match self.ends_at_ms {
            Some(ends) if now >= ends => {
                self.ends_at_ms = None;
                self.paused_ms = 0;
                self.done = true;
                true
            }
            _ => false,
        }
    }
}

/// What reaches the disk.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Saved {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    ends_at_ms: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    paused_ms: Option<i64>,
}

pub fn path() -> Option<PathBuf> {
    crate::tools::state_path(FILE)
}
/// Writes the timer, or removes the file when there is none to keep.
pub fn save_to(path: &Path, timer: Option<&Timer>) -> Result<(), String> {
    let saved = match timer {
        Some(t) if t.running() => Saved {
            ends_at_ms: t.ends_at_ms,
            paused_ms: None,
        },
        Some(t) if t.paused() => Saved {
            ends_at_ms: None,
            paused_ms: Some(t.paused_ms),
        },
        _ => {
            return match std::fs::remove_file(path) {
                Err(e) if e.kind() != std::io::ErrorKind::NotFound => Err(e.to_string()),
                _ => Ok(()),
            };
        }
    };
    let bytes = serde_json::to_vec(&saved).map_err(|e| e.to_string())?;
    crate::tools::write_private(path, &bytes)
}
/// The saved timer, if one is still worth showing. The original length is
/// not kept, so the ring measures against what was left, rounded up to a
/// whole minute.
pub fn load_from(path: &Path, now: i64) -> Option<Timer> {
    let bytes = crate::tools::read_bounded(path, 1024).ok()??;
    let saved: Saved = serde_json::from_slice(&bytes).ok()?;
    let whole = |ms: i64| ((ms + 59_999) / 60_000).max(1) * 60_000;
    match saved {
        Saved {
            ends_at_ms: Some(ends),
            paused_ms: None,
        } => {
            let left = ends - now;
            if left > MAX_MS || left < -STALE_MS {
                return None;
            }
            let mut timer = Timer {
                total_ms: whole(left.max(0)),
                ends_at_ms: Some(ends),
                paused_ms: 0,
                done: false,
            };
            timer.expire(now);
            Some(timer)
        }
        Saved {
            ends_at_ms: None,
            paused_ms: Some(left),
        } if (1..=MAX_MS).contains(&left) => Some(Timer {
            total_ms: whole(left),
            ends_at_ms: None,
            paused_ms: left,
            done: false,
        }),
        _ => None,
    }
}

/// The sound theme's alarm, played once with a deadline. Nothing heavy: the
/// PipeWire or PulseAudio player the system already has.
pub fn chime() {
    if crate::tools::preview() {
        return;
    }
    let Some(sound) = sound(Path::new("/usr/share/sounds")) else {
        return;
    };
    std::thread::spawn(move || {
        for player in ["pw-play", "paplay"] {
            let Ok(mut child) = std::process::Command::new("timeout")
                .args(["5", player])
                .arg(&sound)
                .stdin(std::process::Stdio::null())
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::null())
                .spawn()
            else {
                continue;
            };
            if child.wait().is_ok_and(|s| s.success()) {
                return;
            }
        }
    });
}
/// COSMIC on Pop ships its own theme beside freedesktop's; either has the
/// alarm, under `stereo/` or one folder below it.
fn sound(root: &Path) -> Option<PathBuf> {
    const NAMES: [&str; 2] = ["alarm-clock-elapsed.oga", "complete.oga"];
    for name in NAMES {
        for theme in ["Pop", "freedesktop"] {
            let stereo = root.join(theme).join("stereo");
            let direct = stereo.join(name);
            if direct.is_file() {
                return Some(direct);
            }
            let Ok(entries) = std::fs::read_dir(&stereo) else {
                continue;
            };
            for entry in entries.flatten().take(32) {
                let nested = entry.path().join(name);
                if nested.is_file() {
                    return Some(nested);
                }
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    fn temp(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "halcyon-timer-{name}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }
    #[test]
    fn timer_counts_pauses_extends_and_expires_once() {
        let mut t = Timer::start(5 * 60_000, 1_000);
        assert_eq!(t.remaining(61_000), 4 * 60_000);
        assert!((t.fraction(61_000) - 0.8).abs() < 1e-6);
        t.pause(61_000);
        assert!(t.paused());
        assert_eq!(t.remaining(999_999), 4 * 60_000, "paused time stands still");
        t.add(60_000, 999_999);
        assert_eq!(t.remaining(0), 5 * 60_000);
        assert_eq!(t.total_ms, 6 * 60_000);
        t.resume(100_000);
        assert_eq!(t.ends_at_ms, Some(400_000));
        assert!(!t.expire(399_999));
        assert!(t.expire(400_000));
        assert!(!t.expire(500_000), "the chime plays once");
        assert!(t.done && t.remaining(400_000) == 0);
        t.resume(500_000);
        assert!(t.done, "a finished timer does not resume");
        t.add(60_000, 500_000);
        assert_eq!(t, Timer::start(60_000, 500_000), "+1 min starts again");
    }
    #[test]
    fn timer_is_bounded_to_a_day() {
        let mut t = Timer::start(MAX_MS * 3, 0);
        assert_eq!(t.remaining(0), MAX_MS);
        t.add(60_000, 0);
        assert_eq!(t.remaining(0), MAX_MS);
        assert_eq!(Timer::start(0, 0).total_ms, 1_000);
    }
    #[test]
    fn timer_persists_only_the_end_or_the_paused_remainder() {
        let dir = temp("persist");
        let path = dir.join("activity-island/timer.json");
        let running = Timer::start(10 * 60_000, 0);
        save_to(&path, Some(&running)).unwrap();
        assert_eq!(
            std::fs::read_to_string(&path).unwrap(),
            r#"{"ends_at_ms":600000}"#
        );
        let back = load_from(&path, 30_000).unwrap();
        assert_eq!(back.ends_at_ms, Some(600_000));
        assert_eq!(back.total_ms, 10 * 60_000, "9.5 min left rounds up");
        let mut paused = running;
        paused.pause(90_000);
        save_to(&path, Some(&paused)).unwrap();
        assert_eq!(
            std::fs::read_to_string(&path).unwrap(),
            r#"{"paused_ms":510000}"#
        );
        let back = load_from(&path, 9_999_999).unwrap();
        assert!(back.paused() && back.remaining(0) == 510_000);
        assert_eq!(back.total_ms, 9 * 60_000);
        let mut done = running;
        done.expire(600_000);
        save_to(&path, Some(&done)).unwrap();
        assert!(!path.exists(), "a finished timer leaves nothing behind");
        save_to(&path, None).unwrap();
        std::fs::remove_dir_all(dir).unwrap();
    }
    #[test]
    fn timer_restore_finishes_a_just_missed_end_and_drops_stale_or_damaged_files() {
        let dir = temp("restore");
        let path = dir.join("timer.json");
        std::fs::write(&path, r#"{"ends_at_ms":100000}"#).unwrap();
        let back = load_from(&path, 130_000).unwrap();
        assert!(back.done, "ended while the desktop was away: announce it");
        assert!(load_from(&path, 100_000 + STALE_MS + 1).is_none());
        for damaged in [
            "{",
            r#"{"ends_at_ms":1,"paused_ms":5}"#,
            r#"{"paused_ms":-5}"#,
            r#"{"paused_ms":99999999999}"#,
            r#"{"other":1}"#,
            "{}",
        ] {
            std::fs::write(&path, damaged).unwrap();
            assert!(load_from(&path, 0).is_none(), "{damaged}");
        }
        assert!(load_from(&dir.join("missing.json"), 0).is_none());
        std::fs::remove_dir_all(dir).unwrap();
    }
    #[test]
    fn timer_chime_prefers_the_alarm_in_either_layout() {
        let dir = temp("sound");
        assert!(sound(&dir).is_none());
        let fallback = dir.join("freedesktop/stereo/complete.oga");
        std::fs::create_dir_all(fallback.parent().unwrap()).unwrap();
        std::fs::write(&fallback, b"").unwrap();
        assert_eq!(sound(&dir), Some(fallback));
        let nested = dir.join("Pop/stereo/alert/alarm-clock-elapsed.oga");
        std::fs::create_dir_all(nested.parent().unwrap()).unwrap();
        std::fs::write(&nested, b"").unwrap();
        assert_eq!(sound(&dir), Some(nested));
        std::fs::remove_dir_all(dir).unwrap();
    }
}
