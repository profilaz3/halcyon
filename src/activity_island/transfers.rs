//! Copies and downloads, as apps report them. Two sources reach the island:
//! the Unity launcher protocol (`com.canonical.Unity.LauncherEntry.Update`,
//! sent by Electron and GTK apps) and a notification's `value` hint. Nothing
//! here polls: the island only learns what the apps say.
use std::collections::{HashMap, VecDeque};
use std::time::{Duration, Instant};
use zbus::zvariant::{OwnedValue, Value};

/// A finished job shows as Done this long, then retracts.
pub const DONE: Duration = Duration::from_secs(6);
/// A job no app has spoken about for this long is forgotten.
const STALE: Duration = Duration::from_secs(120);
/// Rates come from this much recent history.
const WINDOW: Duration = Duration::from_secs(20);
/// An estimate shows only once this much history agrees.
const SETTLE: Duration = Duration::from_secs(4);
/// The last estimates must agree within this share to show.
const AGREE: f64 = 0.2;
/// Jobs followed at once, at most.
const JOBS: usize = 8;

/// One report from an app.
#[derive(Clone, Debug, PartialEq)]
pub struct Update {
    pub key: String,
    pub app: String,
    /// The desktop file id, for Open.
    pub desktop: Option<String>,
    /// The notification that carries it, when it came from one.
    pub note: Option<u32>,
    /// 0 to 1, when the report has one.
    pub progress: Option<f64>,
    /// False once the app hides its progress: finished or cancelled.
    pub visible: Option<bool>,
}

/// The launcher protocol's body: the app's `application://id.desktop` URI
/// and its changed properties. Reports without progress are ignored.
#[cfg(test)]
pub fn launcher_update(uri: &str, props: &HashMap<String, OwnedValue>) -> Option<Update> {
    launcher_update_with(uri, props, app_name)
}
fn launcher_update_with(
    uri: &str,
    props: &HashMap<String, OwnedValue>,
    name: impl FnOnce(&str) -> String,
) -> Option<Update> {
    let desktop = uri.strip_prefix("application://")?.trim();
    if desktop.is_empty()
        || desktop.len() > 255
        || desktop.starts_with('.')
        || desktop.chars().any(|c| c.is_control() || c == '/')
    {
        return None;
    }
    let progress = props.get("progress").and_then(|v| match &**v {
        Value::F64(p) if p.is_finite() => Some(p.clamp(0., 1.)),
        _ => None,
    });
    let visible = props.get("progress-visible").and_then(|v| match &**v {
        Value::Bool(b) => Some(*b),
        _ => None,
    });
    if progress.is_none() && visible.is_none() {
        return None;
    }
    Some(Update {
        key: format!("launcher:{desktop}"),
        app: name(desktop),
        desktop: Some(desktop.to_owned()),
        note: None,
        progress,
        visible,
    })
}
/// A notification's `value` hint, 0 to 100, as the spec has it.
pub fn note_progress(hint: Option<&Value<'_>>) -> Option<u8> {
    let value: i64 = match hint? {
        Value::I32(v) => (*v).into(),
        Value::U32(v) => (*v).into(),
        Value::I64(v) => *v,
        Value::U8(v) => (*v).into(),
        Value::F64(v) if v.is_finite() => v.round() as i64,
        _ => return None,
    };
    Some(value.clamp(0, 100) as u8)
}
pub fn note_update(id: u32, app: &str, progress: u8) -> Update {
    Update {
        key: format!("note:{id}"),
        app: app.to_owned(),
        desktop: None,
        note: Some(id),
        progress: Some(f64::from(progress) / 100.),
        visible: Some(true),
    }
}
/// The app's own name from its desktop file, else a readable id.
fn app_name(desktop: &str) -> String {
    let stem = desktop.strip_suffix(".desktop").unwrap_or(desktop);
    let shared =
        std::env::var("XDG_DATA_DIRS").unwrap_or_else(|_| "/usr/local/share:/usr/share".into());
    let dirs = std::env::var_os("XDG_DATA_HOME")
        .map(std::path::PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| std::path::Path::new(&h).join(".local/share")))
        .into_iter()
        .chain(
            shared
                .split(':')
                .filter(|d| !d.is_empty())
                .take(16)
                .map(std::path::PathBuf::from),
        );
    for dir in dirs {
        let path = dir.join("applications").join(format!("{stem}.desktop"));
        if let Ok(Some(bytes)) = crate::tools::read_bounded(&path, 64 * 1024)
            && let Some(name) = desktop_name(&String::from_utf8_lossy(&bytes))
        {
            return name;
        }
    }
    readable_id(stem)
}
fn desktop_name(text: &str) -> Option<String> {
    let mut entry = false;
    for line in text.lines() {
        let line = line.trim();
        if line.starts_with('[') {
            entry = line == "[Desktop Entry]";
        } else if entry && let Some(name) = line.strip_prefix("Name=") {
            let name: String = name
                .trim()
                .chars()
                .filter(|c| !c.is_control())
                .take(48)
                .collect();
            return (!name.is_empty()).then_some(name);
        }
    }
    None
}
/// `org.mozilla.firefox` reads as Firefox.
fn readable_id(stem: &str) -> String {
    let last = stem.rsplit('.').next().unwrap_or(stem);
    let mut chars = last.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().chain(chars).take(48).collect(),
        None => "App".into(),
    }
}

/// How long is left, from recent progress. Quiet until the rate settles.
#[derive(Clone, Debug, Default)]
pub struct Eta {
    samples: VecDeque<(Instant, f64)>,
    /// Seconds left as estimated at each report.
    estimates: VecDeque<(Instant, f64)>,
}
impl Eta {
    pub fn push(&mut self, at: Instant, fraction: f64) {
        if self
            .samples
            .back()
            .is_some_and(|(_, last)| fraction + 0.001 < *last)
        {
            // Went backwards: a new pass, a retry or a different file.
            *self = Self::default();
        }
        if self
            .samples
            .back()
            .is_some_and(|(t, f)| *f == fraction || *t == at)
        {
            return;
        }
        self.samples.push_back((at, fraction));
        while self.samples.len() > 2
            && self
                .samples
                .front()
                .is_some_and(|(t, _)| at.saturating_duration_since(*t) > WINDOW)
        {
            self.samples.pop_front();
        }
        while self.samples.len() > 64 {
            self.samples.pop_front();
        }
        let (Some((t0, f0)), Some((t1, f1))) = (self.samples.front(), self.samples.back()) else {
            return;
        };
        let span = t1.saturating_duration_since(*t0);
        if self.samples.len() < 3 || span < SETTLE || f1 <= f0 {
            return;
        }
        let rate = (f1 - f0) / span.as_secs_f64();
        self.estimates.push_back((*t1, (1. - f1) / rate));
        while self.estimates.len() > 3 {
            self.estimates.pop_front();
        }
    }
    /// Seconds left at `now`, once the last three estimates agree on when
    /// the job ends.
    pub fn remaining(&self, now: Instant) -> Option<f64> {
        if self.estimates.len() < 3 {
            return None;
        }
        let &(last_at, last) = self.estimates.back()?;
        // Each estimate's finish, as seconds after the latest report.
        let ends = self
            .estimates
            .iter()
            .map(|(at, secs)| secs - last_at.saturating_duration_since(*at).as_secs_f64());
        let low = ends.clone().fold(f64::INFINITY, f64::min);
        let high = ends.fold(f64::NEG_INFINITY, f64::max);
        if !(high.is_finite() && low > 0. && (high - low) <= AGREE * last.max(5.)) {
            return None;
        }
        Some((last - now.saturating_duration_since(last_at).as_secs_f64()).max(0.))
    }
}
/// What the card says about the time left.
pub fn eta_text(seconds: f64) -> String {
    let s = seconds.ceil() as i64;
    match s {
        ..=4 => "Almost done".into(),
        5..60 => format!("{} s left", (s + 4) / 5 * 5),
        60..3600 => format!("About {} min left", (s + 30) / 60),
        _ => {
            let minutes = (s + 30) / 60;
            match minutes % 60 {
                0 => format!("About {} h left", minutes / 60),
                m => format!("About {} h {m} min left", minutes / 60),
            }
        }
    }
}

#[derive(Clone, Debug)]
pub struct Job {
    pub key: String,
    pub app: String,
    pub desktop: Option<String>,
    pub note: Option<u32>,
    pub fraction: f64,
    pub eta: Eta,
    seen: Instant,
    pub done_at: Option<Instant>,
}
impl Job {
    pub fn remaining(&self, now: Instant) -> Option<f64> {
        self.eta.remaining(now)
    }
}

/// Every job an app is reporting, and those just finished.
#[derive(Clone, Debug, Default)]
pub struct Tracker {
    jobs: Vec<Job>,
}
impl Tracker {
    /// True when a job finished with this report: the moment for Done.
    pub fn update(&mut self, u: Update, now: Instant) -> bool {
        let index = self.jobs.iter().position(|j| j.key == u.key);
        let hidden = u.visible == Some(false);
        let Some(index) = index else {
            if hidden || u.progress.is_none_or(|p| p >= 1.) {
                return false;
            }
            if self.jobs.len() >= JOBS {
                self.jobs.remove(0);
            }
            let mut eta = Eta::default();
            eta.push(now, u.progress.unwrap_or(0.));
            self.jobs.push(Job {
                key: u.key,
                app: u.app,
                desktop: u.desktop,
                note: u.note,
                fraction: u.progress.unwrap_or(0.),
                eta,
                seen: now,
                done_at: None,
            });
            return false;
        };
        let job = &mut self.jobs[index];
        job.seen = now;
        if let Some(p) = u.progress {
            if job.done_at.is_some() && p < 0.97 && !hidden {
                // The same app started another.
                job.done_at = None;
                job.eta = Eta::default();
            }
            job.fraction = p;
            job.eta.push(now, p);
        }
        if job.done_at.is_some() {
            return false;
        }
        if hidden || job.fraction >= 1. {
            if job.fraction >= 0.97 {
                job.fraction = 1.;
                job.done_at = Some(now);
                return true;
            }
            // Hidden short of the end: cancelled, nothing to announce.
            self.jobs.remove(index);
        }
        false
    }
    /// The notification carrying a job went away.
    pub fn closed(&mut self, note: u32, now: Instant) -> bool {
        let key = format!("note:{note}");
        if !self.jobs.iter().any(|j| j.key == key) {
            return false;
        }
        self.update(
            Update {
                key,
                app: String::new(),
                desktop: None,
                note: Some(note),
                progress: None,
                visible: Some(false),
            },
            now,
        )
    }
    /// Retires Done and silent jobs. True when anything changed.
    pub fn tick(&mut self, now: Instant) -> bool {
        let before = self.jobs.len();
        self.jobs.retain(|j| match j.done_at {
            Some(at) => now.saturating_duration_since(at) < DONE,
            None => now.saturating_duration_since(j.seen) < STALE,
        });
        before != self.jobs.len()
    }
    pub fn dismiss_done(&mut self) {
        self.jobs.retain(|j| j.done_at.is_none());
    }
    pub fn is_empty(&self) -> bool {
        self.jobs.is_empty()
    }
    pub fn running(&self) -> usize {
        self.jobs.iter().filter(|j| j.done_at.is_none()).count()
    }
    /// The job the island shows: while any runs, the one finishing last
    /// (an unknown estimate counts as last); otherwise the latest Done.
    pub fn lead(&self, now: Instant) -> Option<&Job> {
        let running = self
            .jobs
            .iter()
            .filter(|j| j.done_at.is_none())
            .max_by(|a, b| {
                let left = |j: &Job| j.remaining(now).unwrap_or(f64::INFINITY);
                left(a)
                    .total_cmp(&left(b))
                    .then(b.fraction.total_cmp(&a.fraction))
            });
        running.or_else(|| {
            self.jobs
                .iter()
                .filter(|j| j.done_at.is_some())
                .max_by_key(|j| j.done_at)
        })
    }
    /// When the next job retires on its own.
    pub fn deadline(&self) -> Option<Instant> {
        self.jobs
            .iter()
            .map(|j| match j.done_at {
                Some(at) => at + DONE,
                None => j.seen + STALE,
            })
            .min()
    }
}

/// Launcher reports from every app on the session bus.
pub fn subscription() -> cosmic::iced::Subscription<Update> {
    if crate::tools::preview() {
        return cosmic::iced::Subscription::none();
    }
    cosmic::iced::Subscription::run_with("activity-transfers", |_| {
        cosmic::iced::stream::channel(16, async |mut out| {
            listen(out).await;
            std::future::pending().await
        })
    })
}
/// Waits on the bus for launcher reports and hands each on until no one
/// listens. It sleeps between signals: nothing here polls.
async fn listen(mut out: cosmic::iced::futures::channel::mpsc::Sender<Update>) {
    use cosmic::iced::futures::{SinkExt, StreamExt};
    let stream = async {
        let session = zbus::Connection::session().await.ok()?;
        let rule = zbus::MatchRule::builder()
            .msg_type(zbus::message::Type::Signal)
            .interface("com.canonical.Unity.LauncherEntry")
            .ok()?
            .member("Update")
            .ok()?
            .build();
        let stream = zbus::MessageStream::for_match_rule(rule, &session, Some(32))
            .await
            .ok()?;
        Some((session, stream))
    };
    let Some((_session, mut stream)) = stream.await else {
        return;
    };
    let mut names: HashMap<String, String> = HashMap::new();
    while let Some(message) = stream.next().await {
        let Ok(message) = message else { continue };
        let Ok((uri, props)) = message
            .body()
            .deserialize::<(String, HashMap<String, OwnedValue>)>()
        else {
            continue;
        };
        let Some(update) = launcher_update_cached(&uri, &props, &mut names) else {
            continue;
        };
        if out.send(update).await.is_err() {
            return;
        }
    }
}
/// Reads each app's desktop file once.
fn launcher_update_cached(
    uri: &str,
    props: &HashMap<String, OwnedValue>,
    names: &mut HashMap<String, String>,
) -> Option<Update> {
    let update = launcher_update_with(uri, props, |desktop| {
        names
            .get(desktop)
            .cloned()
            .unwrap_or_else(|| app_name(desktop))
    })?;
    if let Some(desktop) = &update.desktop
        && names.len() < 64
    {
        names
            .entry(desktop.clone())
            .or_insert_with(|| update.app.clone());
    }
    Some(update)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn props(pairs: &[(&str, Value<'static>)]) -> HashMap<String, OwnedValue> {
        pairs
            .iter()
            .map(|(k, v)| ((*k).to_owned(), OwnedValue::try_from(v.clone()).unwrap()))
            .collect()
    }
    fn job(key: &str, progress: f64) -> Update {
        Update {
            key: key.into(),
            app: key.into(),
            desktop: None,
            note: None,
            progress: Some(progress),
            visible: Some(true),
        }
    }
    #[test]
    fn transfers_parse_launcher_progress_and_ignore_badge_only_updates() {
        let u = launcher_update(
            "application://org.example.Files.desktop",
            &props(&[
                ("progress", Value::F64(0.25)),
                ("progress-visible", Value::Bool(true)),
                ("count", Value::I64(3)),
            ]),
        )
        .unwrap();
        assert_eq!(u.key, "launcher:org.example.Files.desktop");
        assert_eq!(u.desktop.as_deref(), Some("org.example.Files.desktop"));
        assert_eq!((u.progress, u.visible), (Some(0.25), Some(true)));
        assert!(!u.app.is_empty());
        // What T3 Code sends: a badge, no progress.
        assert!(
            launcher_update(
                "application://com.t3tools.T3Code.desktop",
                &props(&[
                    ("count", Value::I64(0)),
                    ("count-visible", Value::Bool(false))
                ]),
            )
            .is_none()
        );
        let clamped = launcher_update(
            "application://a.desktop",
            &props(&[("progress", Value::F64(7.))]),
        )
        .unwrap();
        assert_eq!(clamped.progress, Some(1.));
        for bad in ["a.desktop", "application://", "application://../x.desktop"] {
            assert!(
                launcher_update(bad, &props(&[("progress", Value::F64(0.5))])).is_none(),
                "{bad}"
            );
        }
        assert!(
            launcher_update(
                "application://a.desktop",
                &props(&[("progress", Value::Str("1".into()))])
            )
            .is_none()
        );
    }
    #[test]
    fn transfers_read_the_notification_value_hint() {
        assert_eq!(note_progress(Some(&Value::I32(42))), Some(42));
        assert_eq!(note_progress(Some(&Value::I32(-3))), Some(0));
        assert_eq!(note_progress(Some(&Value::U32(250))), Some(100));
        assert_eq!(note_progress(Some(&Value::Str("5".into()))), None);
        assert_eq!(note_progress(None), None);
        let u = note_update(7, "Files", 30);
        assert_eq!(
            (u.key.as_str(), u.note, u.progress),
            ("note:7", Some(7), Some(0.3))
        );
    }
    #[test]
    fn transfers_name_apps_from_their_desktop_file_or_id() {
        assert_eq!(
            desktop_name(
                "[Desktop Action new]\nName=New\n[Desktop Entry]\nName=Files\nName[de]=Dateien"
            ),
            Some("Files".into())
        );
        assert_eq!(desktop_name("[Desktop Entry]\nExec=x"), None);
        assert_eq!(readable_id("org.mozilla.firefox"), "Firefox");
        assert_eq!(readable_id(""), "App");
    }
    #[test]
    fn transfers_eta_waits_for_a_steady_rate_then_counts_down() {
        let t = Instant::now();
        let s = |secs: f64| t + Duration::from_secs_f64(secs);
        let mut eta = Eta::default();
        eta.push(s(0.), 0.0);
        eta.push(s(1.), 0.01);
        eta.push(s(2.), 0.02);
        assert_eq!(eta.remaining(s(2.)), None, "too little history");
        for i in 3..=6 {
            eta.push(s(i as f64), i as f64 * 0.01);
        }
        // 1 % a second with 94 % to go.
        let left = eta.remaining(s(6.)).unwrap();
        assert!((left - 94.).abs() < 0.5, "{left}");
        let later = eta.remaining(s(10.)).unwrap();
        assert!(
            (later - 90.).abs() < 0.5,
            "counts down between reports: {later}"
        );
        // A burst then a stall disagrees with itself: hidden again.
        eta.push(s(7.), 0.30);
        eta.push(s(8.), 0.31);
        eta.push(s(14.), 0.311);
        assert_eq!(eta.remaining(s(14.)), None);
        // Going backwards starts over.
        eta.push(s(15.), 0.05);
        assert_eq!(eta.remaining(s(15.)), None);
    }
    #[test]
    fn transfers_eta_reads_as_short_plain_text() {
        assert_eq!(eta_text(2.), "Almost done");
        assert_eq!(eta_text(31.), "35 s left");
        assert_eq!(eta_text(150.), "About 3 min left");
        assert_eq!(eta_text(3600.), "About 1 h left");
        assert_eq!(eta_text(4500.), "About 1 h 15 min left");
    }
    #[test]
    fn transfers_track_the_job_finishing_last_and_retract_done() {
        let t = Instant::now();
        let s = |secs: u64| t + Duration::from_secs(secs);
        let mut tracker = Tracker::default();
        assert!(!tracker.update(job("a", 0.0), s(0)));
        tracker.update(job("b", 0.0), s(0));
        for i in 1..=6u64 {
            // a: 10 %/s, b: 1 %/s.
            tracker.update(job("a", (i as f64 * 0.1).min(0.9)), s(i));
            tracker.update(job("b", i as f64 * 0.01), s(i));
        }
        assert_eq!(tracker.running(), 2);
        assert_eq!(tracker.lead(s(6)).unwrap().key, "b", "b finishes last");
        // a hides at the end: Done.
        let mut end = job("a", 1.0);
        end.visible = Some(false);
        assert!(tracker.update(end, s(7)));
        assert_eq!(tracker.running(), 1);
        assert_eq!(
            tracker.lead(s(7)).unwrap().key,
            "b",
            "running jobs come first"
        );
        // b is cancelled short of the end: dropped, no Done.
        let mut cancel = job("b", 0.07);
        cancel.visible = Some(false);
        assert!(!tracker.update(cancel, s(7)));
        let lead = tracker.lead(s(7)).unwrap();
        assert_eq!((lead.key.as_str(), lead.done_at.is_some()), ("a", true));
        assert_eq!(tracker.deadline(), Some(s(7) + DONE));
        assert!(!tracker.tick(s(12)));
        assert!(tracker.tick(s(13)), "Done retracts");
        assert!(tracker.is_empty());
        // A hidden report for an unknown job, or a complete one, starts nothing.
        let mut ghost = job("c", 0.5);
        ghost.visible = Some(false);
        tracker.update(ghost, s(20));
        tracker.update(job("d", 1.0), s(20));
        assert!(tracker.is_empty());
        // Silence ends a job too.
        tracker.update(job("e", 0.2), s(20));
        assert!(!tracker.tick(s(20) + STALE - Duration::from_secs(1)));
        assert!(tracker.tick(s(20) + STALE));
    }
    #[test]
    fn transfers_from_notifications_finish_when_closed_near_the_end() {
        let t = Instant::now();
        let mut tracker = Tracker::default();
        tracker.update(note_update(4, "Files", 40), t);
        assert!(!tracker.closed(9, t), "another notification");
        tracker.update(note_update(4, "Files", 99), t + Duration::from_secs(1));
        assert!(tracker.closed(4, t + Duration::from_secs(2)));
        assert!(tracker.lead(t).unwrap().done_at.is_some());
        tracker.dismiss_done();
        assert!(tracker.is_empty());
    }

    /// Idle cost of the always-on listener, measured on the real session
    /// bus: `HALCYON_IDLE_SECS=60 cargo test --release --bin halcyon --
    /// --ignored transfers_listener_idles --nocapture`.
    #[test]
    #[ignore = "measures for a minute on the session bus"]
    fn transfers_listener_idles_without_waking() {
        fn ticks() -> (u64, u64) {
            let stat = std::fs::read_to_string("/proc/self/stat").unwrap();
            let fields: Vec<&str> = stat[stat.rfind(')').unwrap() + 2..].split(' ').collect();
            let cpu = fields[11].parse::<u64>().unwrap() + fields[12].parse::<u64>().unwrap();
            let status = std::fs::read_to_string("/proc/self/status").unwrap();
            let switches = status
                .lines()
                .find_map(|l| l.strip_prefix("voluntary_ctxt_switches:"))
                .and_then(|v| v.trim().parse().ok())
                .unwrap_or(0);
            (cpu, switches)
        }
        let secs: u64 = std::env::var("HALCYON_IDLE_SECS")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(60);
        let runtime = tokio::runtime::Runtime::new().unwrap();
        let (out, mut reports) = cosmic::iced::futures::channel::mpsc::channel(16);
        let (cpu, switches) = runtime.block_on(async {
            // Connect first, so the measurement is only the wait.
            let listening = listen(out);
            tokio::pin!(listening);
            let _ = tokio::time::timeout(Duration::from_secs(2), &mut listening).await;
            let before = ticks();
            let _ = tokio::time::timeout(Duration::from_secs(secs), &mut listening).await;
            let after = ticks();
            (after.0 - before.0, after.1 - before.1)
        });
        let mut seen = 0;
        while let Ok(Some(_)) = reports.try_next() {
            seen += 1;
        }
        // Clock ticks are hundredths of a second.
        let percent = cpu as f64 / secs as f64;
        println!(
            "listener idle over {secs} s: {percent:.3}% CPU, {:.2} wakes/s, {seen} reports",
            switches as f64 / secs as f64
        );
        assert!(percent < 0.1, "{percent}% CPU");
    }
}
