//! The desktop's side of automation, without the UI: it follows the
//! settings file, settles display changes, asks `decide` what is due, hands
//! switches to a blocking job and records what became of them. The desktop
//! app only feeds it the clock and events and runs its jobs.
//!
//! Display changes are confirmed only after the set of connected displays
//! has stayed the same for `SETTLE`, so a monitor that reconnects while it
//! wakes, or a dock that brings up its displays one by one, makes one
//! change, not several.
use super::decide::{self, Due, Effect, Outcome, State};
use super::rules::Automation;
use super::run::{Ran, run_for_cause};
use crate::desktop::DesktopStore;
use crate::desktop::profiles::{Domain, Installed};
use chrono::{DateTime, Local, TimeZone, Utc};
use std::collections::BTreeSet;
use std::path::PathBuf;
use std::time::{Duration, Instant};

pub const SETTLE: Duration = Duration::from_secs(5);
/// Displays are looked at this often while a display rule is on (an
/// output event from the compositor looks sooner).
pub const DISPLAY_POLL: Duration = Duration::from_secs(60);
/// While a switch waits for a Settings window, the window is looked for
/// this often, so a hold left by one that crashed is released.
pub const HOLD_CHECK: Duration = Duration::from_secs(15);

/// Switches for the blocking job, in order, with the Settings hold.
#[derive(Clone, Debug, PartialEq)]
pub struct Job {
    pub due: Vec<Due>,
    pub held: BTreeSet<Domain>,
    pub epoch: u64,
}

impl Job {
    /// Runs every switch in order (blocking), oldest first. A later switch
    /// replaces an earlier one that waits (`Automator::finished`), so a
    /// waiting switch never runs over a newer one afterwards.
    pub fn run(self, store: &DesktopStore) -> Vec<(Due, Ran)> {
        let installed = Installed::discover(store);
        let mut results = Vec::new();
        for due in self.due {
            let expected = store.manual_at();
            let manual = manual_at(store);
            let ran = if store.restore_epoch().ok() != Some(self.epoch) {
                Ran::KeptManual(chrono::Utc::now().timestamp())
            } else if let Some(at) = manual.filter(|at| at.timestamp() >= due.at) {
                Ran::KeptManual(at.timestamp())
            } else {
                match super::lock_rules(store) {
                    Err(reason) => Ran::Failed(reason),
                    Ok(_lock) => match super::load(store) {
                        Err(reason) => Ran::Failed(reason),
                        Ok(settings) => {
                            let context = if settings.rules.iter().any(|r| {
                                r.enabled && matches!(r.when, super::Trigger::Solar { .. })
                            }) {
                                super::solar::Context::read(store, &settings)
                            } else {
                                super::solar::Context::default()
                            };
                            if !due.is_current_with(&settings, super::now(), &context) {
                                Ran::Failed(
                                    "The automation rule changed, or its city changed. This switch was cancelled.".into(),
                                )
                            } else {
                                let solar =
                                    settings.rule(&due.rule).and_then(|rule| match rule.when {
                                        super::Trigger::Solar { source, .. } => {
                                            Some((&context, source))
                                        }
                                        _ => None,
                                    });
                                crate::desktop::actions::scheduled_epoch(
                                    self.epoch,
                                    expected,
                                    || {
                                        run_for_cause(
                                            store,
                                            &installed,
                                            &due.profile,
                                            &due.name,
                                            &super::read_hold(store),
                                            matches!(due.cause, decide::Cause::Display(_)),
                                            solar,
                                        )
                                    },
                                )
                            }
                        }
                    },
                }
            };
            results.push((due, ran));
        }
        results
    }
}

pub struct Automator {
    store: Option<DesktopStore>,
    settings: Option<Automation>,
    raw: Option<Vec<u8>>,
    problem: Option<String>,
    pub state: State,
    displays: Option<Vec<String>>,
    settling: Option<(Vec<String>, Instant)>,
    next_display_check: Option<Instant>,
    wake: Option<Instant>,
    busy: bool,
    again: bool,
    retry_pending: bool,
    held: BTreeSet<Domain>,
}

impl Automator {
    /// The desktop's store; automation stays inactive in preview mode or
    /// when the store can't be used.
    pub fn from_env() -> Self {
        let store = DesktopStore::from_env().ok().filter(DesktopStore::writable);
        Self::with_store(store)
    }
    pub fn with_store(store: Option<DesktopStore>) -> Self {
        let mut automator = Self {
            state: store.as_ref().map(super::load_state).unwrap_or_default(),
            store,
            settings: None,
            raw: None,
            problem: None,
            displays: None,
            settling: None,
            next_display_check: None,
            wake: None,
            busy: false,
            again: false,
            // A switch left waiting by the previous session is tried again.
            retry_pending: true,
            held: BTreeSet::new(),
        };
        automator.reload();
        automator
    }
    /// Files to watch: the settings and the Settings hold.
    pub fn paths(&self) -> Vec<PathBuf> {
        self.store
            .as_ref()
            .map(|store| {
                vec![
                    super::path(store, super::FILE),
                    super::path(store, super::HOLD_FILE),
                ]
            })
            .unwrap_or_default()
    }
    #[cfg_attr(not(test), allow(dead_code))]
    pub fn problem(&self) -> Option<&str> {
        self.problem.as_deref()
    }
    /// Re-reads the settings and the hold. `true` when either changed, so
    /// the caller evaluates now.
    pub fn reload(&mut self) -> bool {
        let Some(store) = &self.store else {
            return false;
        };
        let mut changed = false;
        let raw = std::fs::read(super::path(store, super::FILE)).ok();
        if raw != self.raw || (self.settings.is_none() && self.problem.is_none()) {
            changed = true;
            match super::load(store) {
                Ok(settings) => {
                    self.settings = Some(settings);
                    self.problem = None;
                }
                Err(problem) => {
                    if self.problem.as_ref() != Some(&problem) {
                        eprintln!("Automation: {problem}");
                    }
                    self.settings = None;
                    self.problem = Some(problem);
                }
            }
            self.raw = raw;
            // Look now: new rules start from the displays as they are.
            self.wake = Some(Instant::now());
            if !self.watching_displays() {
                self.displays = None;
                self.settling = None;
                self.next_display_check = None;
            }
        }
        let held = super::read_hold(store);
        if held != self.held {
            self.held = held;
            self.retry_pending = true;
            self.wake = Some(Instant::now());
            changed = true;
        }
        changed
    }
    fn running(&self) -> bool {
        self.store.is_some() && self.settings.as_ref().is_some_and(Automation::running)
    }
    fn watching_displays(&self) -> bool {
        self.settings
            .as_ref()
            .is_some_and(Automation::watches_displays)
    }
    /// When to evaluate next (`None`: nothing to do until a file changes).
    pub fn wake(&self) -> Option<Instant> {
        if self.busy {
            return None;
        }
        self.wake
    }
    /// A display may have changed: look again soon.
    pub fn displays_changed(&mut self, now: Instant) {
        if self.watching_displays() {
            self.next_display_check = Some(now);
            self.wake = Some(self.wake.map_or(now, |wake| wake.min(now)));
        }
    }
    /// `lost` passed while the monotonic clock stood still (suspend): the
    /// next look comes that much sooner, and displays are read again, since
    /// they may have changed while asleep.
    pub fn resumed(&mut self, lost: Duration, now: Instant) {
        self.wake = self.wake.map(|wake| wake.checked_sub(lost).unwrap_or(now));
        if self.watching_displays() {
            self.displays_changed(now);
        }
    }
    /// Whether it is time to evaluate.
    pub fn due(&self, now: Instant) -> bool {
        self.wake().is_some_and(|wake| wake <= now)
    }

    /// Evaluates with the local time zone and the real displays.
    pub fn evaluate(&mut self, now: DateTime<Utc>, instant: Instant) -> Option<Job> {
        // Read only when a display rule is on and a look is due.
        let read = self
            .store
            .as_ref()
            .filter(|_| {
                self.watching_displays() && self.next_display_check.is_none_or(|at| at <= instant)
            })
            .map(super::displays::connected);
        self.evaluate_with(&Local, now, instant, move || read.unwrap_or_default())
    }

    /// One evaluation: settles displays, decides, saves the state when it
    /// matters, and returns the switches to run (if not already running).
    pub fn evaluate_with<Tz: TimeZone>(
        &mut self,
        tz: &Tz,
        now: DateTime<Utc>,
        instant: Instant,
        connected: impl FnOnce() -> Vec<String>,
    ) -> Option<Job> {
        self.wake = None;
        // Clear invalid persisted waits even when all rules or the master
        // switch are disabled (the running check below would skip decide).
        if self.state.pending.as_ref().is_some_and(|pending| {
            self.settings.as_ref().is_none_or(|settings| {
                !pending.is_current_with(
                    settings,
                    now.timestamp(),
                    &self
                        .store
                        .as_ref()
                        .map(|store| super::solar::Context::read(store, settings))
                        .unwrap_or_default(),
                )
            })
        }) {
            self.state.pending = None;
            if let Some(store) = &self.store
                && let Err(error) = super::save_state(store, &self.state)
            {
                eprintln!("Automation: could not save its state: {error}");
            }
        }
        if !self.running() {
            return None;
        }
        if self.busy {
            self.again = true;
            return None;
        }
        let settings = self.settings.clone()?;
        let store = self.store.as_ref()?;
        let epoch = match store.restore_epoch() {
            Ok(epoch) => epoch,
            Err(_) => {
                // Do not consume a due event or mark a nonexistent job busy.
                self.wake = Some(instant + HOLD_CHECK);
                return None;
            }
        };
        // A Settings window that ended without releasing its hold (killed or
        // crashed) leaves the file unchanged: look whether it still runs.
        if !self.held.is_empty() {
            let held = super::read_hold(store);
            if held != self.held {
                self.held = held;
                self.retry_pending = true;
            }
        }
        let mut wakes = Vec::new();
        if settings.watches_displays() {
            if self.next_display_check.is_none_or(|at| at <= instant) {
                let mut read = connected();
                read.sort();
                read.dedup();
                if self.displays.as_ref() == Some(&read) {
                    self.settling = None;
                } else {
                    match &self.settling {
                        Some((pending, since))
                            if *pending == read && instant.duration_since(*since) >= SETTLE =>
                        {
                            self.displays = Some(read);
                            self.settling = None;
                        }
                        Some((pending, _)) if *pending == read => {}
                        _ => self.settling = Some((read, instant)),
                    }
                }
                self.next_display_check = Some(match &self.settling {
                    Some((_, since)) => *since + SETTLE,
                    None => instant + DISPLAY_POLL,
                });
            }
            wakes.extend(self.next_display_check);
        }
        let manual_at = || manual_at(store);
        let context = if settings
            .rules
            .iter()
            .any(|r| r.enabled && matches!(r.when, super::Trigger::Solar { .. }))
        {
            super::solar::Context::read(store, &settings)
        } else {
            super::solar::Context::default()
        };
        let decision = decide::decide_with_solar(
            &settings,
            &mut self.state,
            tz,
            now,
            self.displays.as_deref(),
            &manual_at,
            &context,
        );
        let mut persist = decision.persist || decision.kept > 0;
        let mut due = decision.due;
        if due.is_empty() && self.retry_pending {
            if let Some(pending) = self.state.pending.clone() {
                match decide::still_due(&pending, manual_at(), now.timestamp()) {
                    Ok(()) => due.push(pending),
                    Err(outcome) => {
                        self.state.record(outcome);
                        self.state.pending = None;
                        persist = true;
                    }
                }
            }
        }
        self.retry_pending = false;
        if persist && let Err(error) = super::save_state(store, &self.state) {
            eprintln!("Automation: could not save its state: {error}");
        }
        if let Some(at) = decide::next_check_with_solar(&settings, &self.state, tz, now, &context) {
            let wait = (at - now).to_std().unwrap_or_default();
            wakes.push(instant + wait.max(Duration::from_millis(200)));
        }
        if self.state.pending.is_some() && !self.held.is_empty() {
            wakes.push(instant + HOLD_CHECK);
        }
        self.wake = wakes.into_iter().min();
        if due.is_empty() {
            return None;
        }
        self.busy = true;
        Some(Job {
            epoch,
            due,
            held: self.held.clone(),
        })
    }

    /// Records what a job did. Returns `true` when another evaluation was
    /// asked for meanwhile.
    pub fn finished(&mut self, results: Vec<(Due, Ran)>, now: i64, instant: Instant) -> bool {
        self.busy = false;
        for (due, ran) in results {
            let was_pending = self.state.pending.as_ref() == Some(&due);
            if was_pending && matches!(ran, Ran::Waiting(_)) {
                // Still waiting: one history line is enough.
                continue;
            }
            if was_pending {
                self.state.pending = None;
            } else {
                // Newer than anything waiting (a job runs oldest first, and
                // a waiting switch is tried only on its own): it replaces
                // it, whatever becomes of it.
                self.state.replace_pending(&due, now);
            }
            match &ran {
                Ran::Applied { change, .. } => {
                    self.state.applied = Some(change.clone());
                }
                Ran::Waiting(_) => self.state.pending = Some(due.clone()),
                _ => {}
            }
            let outcome = Outcome {
                transaction: match &ran {
                    Ran::Applied { change, .. } => Some(change.clone()),
                    _ => None,
                },
                at: now,
                due,
                effect: ran.effect(),
            };
            if let Effect::Failed { reason } = &outcome.effect {
                eprintln!("Automation: {} not applied: {reason}", outcome.due.name);
            }
            self.state.record(outcome);
        }
        if let Some(store) = &self.store
            && let Err(error) = super::save_state(store, &self.state)
        {
            eprintln!("Automation: could not save its state: {error}");
        }
        let again = std::mem::take(&mut self.again);
        if again {
            self.wake = Some(instant);
        } else if self.wake.is_none() {
            self.wake = Some(instant + Duration::from_secs(1));
        }
        again
    }
}

/// Current manual precedence belongs to the store (including Restore).
/// Retain old layout stamps until a migration can retire them without making
/// a previously suppressed occurrence eligible after an upgrade.
fn manual_at(store: &DesktopStore) -> Option<DateTime<Utc>> {
    let recorded = store
        .manual_at()
        .and_then(|created| DateTime::parse_from_rfc3339(&created).ok())
        .map(|at| at.with_timezone(&Utc));
    let layout = super::hold_applied(store).and_then(|at| DateTime::from_timestamp(at, 0));
    recorded.max(layout)
}
