//! Automation page state: the master switch, pause choices, the ordered
//! rule list (on/off, order and delete save at once) and one rule draft
//! (Save / Cancel), with the desktop's status and recent switches.
//!
//! Settings writes `automation.json`; the desktop writes the runtime state
//! this page shows. Nothing here applies a profile or changes the desktop.
//! While Settings holds unapplied desktop drafts it declares their parts in
//! the hold file, so a switch touching them waits instead of making the
//! drafts stale (`sync_hold`).
use crate::automation::solar::{self, Context, Event, Source};
use crate::automation::{
    self, Automation, Change, Days, DisplayWhen, Pause, ProfileRef, Rule, State, Time, Trigger,
    displays::Display,
};
use crate::desktop::DesktopStore;
use crate::desktop::profiles::{self, Domain, ProfileLibrary};
use crate::settings::desktop::Notice;
use chrono::Local;
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
use std::path::PathBuf;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Kind {
    Schedule,
    Solar,
    Display,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PauseFor {
    /// Only the next scheduled switch.
    Next,
    Hour,
    /// Until local midnight.
    Today,
    /// Until resumed.
    Resume,
}

#[derive(Clone, Debug, PartialEq)]
pub enum DraftEdit {
    Kind(Kind),
    Event(Event),
    Source(Source),
    Time(String),
    Day(usize),
    Days(Days),
    Change(Change),
    /// `None`: external displays as a group.
    Display(Option<String>),
    Profile(ProfileRef),
}

#[derive(Clone, Debug, PartialEq)]
pub enum Message {
    AdaptiveRoutine(crate::desktop::profiles::adaptive::Look, Source),
    ManualCity(crate::config::Location),
    Enable(bool),
    Rule(String, bool),
    Move(String, i32),
    Edit(String),
    New(Kind),
    /// Begin a local schedule draft for a look chosen on another page.
    Schedule(ProfileRef),
    Draft(DraftEdit),
    Save,
    Cancel,
    /// `false` asks for confirmation (or withdraws it); `true` deletes.
    Delete(String, bool),
    Pause(PauseFor),
    Resume,
    /// The settings, the desktop's state or the displays may have changed.
    Refresh,
    /// Replace unreadable settings with empty ones (automation off).
    StartOver,
}

impl Message {
    /// Whether the message may write the automation settings.
    pub fn writes(&self) -> bool {
        !matches!(
            self,
            Self::Edit(_)
                | Self::New(_)
                | Self::Schedule(_)
                | Self::Draft(_)
                | Self::Cancel
                | Self::Delete(_, false)
                | Self::Refresh
        )
    }
}

/// A rule being made or edited.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct RuleDraft {
    /// The rule edited; `None` for a new one.
    pub id: Option<String>,
    pub kind: Kind,
    pub time: String,
    #[serde(default = "default_sun_event")]
    pub event: Event,
    #[serde(default)]
    pub source: Source,
    pub days: Days,
    pub change: Change,
    pub display: Option<String>,
    pub label: Option<String>,
    pub profile: Option<ProfileRef>,
    /// The draft as opened, to tell whether anything changed.
    start: Box<Option<RuleDraft>>,
    /// Exact original rule, checked inside the writer lock before Save.
    baseline: Option<Rule>,
}

fn default_sun_event() -> Event {
    Event::Sunset
}

impl RuleDraft {
    fn new(kind: Kind, h24: bool) -> Self {
        let mut draft = Self {
            id: None,
            kind,
            time: Time::new(19, 0).unwrap().format(h24),
            event: Event::Sunset,
            source: Source::Weather,
            days: Days::EVERY,
            change: Change::Connect,
            display: None,
            label: None,
            profile: None,
            start: Box::new(None),
            baseline: None,
        };
        draft.start = Box::new(Some(draft.clone()));
        draft
    }
    fn from_rule(rule: &Rule, h24: bool) -> Self {
        let mut draft = Self::new(
            if matches!(rule.when, Trigger::Solar { .. }) {
                Kind::Solar
            } else if rule.when.is_schedule() {
                Kind::Schedule
            } else {
                Kind::Display
            },
            h24,
        );
        draft.id = Some(rule.id.clone());
        draft.baseline = Some(rule.clone());
        draft.profile = Some(rule.profile.clone());
        match &rule.when {
            Trigger::Schedule { time, days } => {
                draft.time = time.format(h24);
                draft.days = *days;
            }
            Trigger::Solar {
                event,
                days,
                source,
            } => {
                draft.event = *event;
                draft.days = *days;
                draft.source = *source;
            }
            Trigger::Display(when) => {
                draft.change = when.change;
                draft.display = when.display.clone();
                draft.label = when.label.clone();
            }
        }
        draft.start = Box::new(None);
        draft.start = Box::new(Some(draft.clone()));
        draft
    }
    /// Unsaved: a new rule, or an edit that changed something.
    pub fn dirty(&self) -> bool {
        let mut plain = self.clone();
        plain.start = Box::new(None);
        self.id.is_none()
            || self.start.as_ref().as_ref().is_none_or(|start| {
                let mut start = start.clone();
                start.start = Box::new(None);
                start != plain
            })
    }
    pub fn parsed_time(&self) -> Option<Time> {
        Time::parse(&self.time)
    }
    /// Saving the first new rule also enables automation. Used by the action
    /// label and checked again against current settings inside the writer lock.
    pub fn enables_automation(&self, settings: &Automation) -> bool {
        self.id.is_none() && settings.rules.is_empty() && !settings.enabled
    }
    /// What keeps Save off, in the order a person fixes it.
    pub fn missing(&self) -> Option<&'static str> {
        match self.kind {
            Kind::Schedule if self.parsed_time().is_none() => Some("Enter a time like 19:00"),
            Kind::Schedule | Kind::Solar if self.days.is_empty() => Some("Choose at least one day"),
            _ if self.profile.is_none() => Some("Choose a profile"),
            _ => None,
        }
    }
    fn trigger(&self) -> Option<Trigger> {
        Some(match self.kind {
            Kind::Schedule => Trigger::Schedule {
                time: self.parsed_time()?,
                days: self.days,
            },
            Kind::Solar => Trigger::Solar {
                event: self.event,
                days: self.days,
                source: self.source,
            },
            Kind::Display => Trigger::Display(DisplayWhen {
                change: self.change,
                display: self.display.clone(),
                label: self.display.as_ref().and(self.label.clone()),
            }),
        })
    }
}

#[derive(Default)]
pub struct AutomationEditor {
    store: Option<Result<DesktopStore, String>>,
    settings: Option<Result<Automation, String>>,
    pub state: State,
    pub solar: Context,
    /// Built-in starting points, then saved profiles: what a rule can apply.
    pub profiles: Vec<(ProfileRef, String)>,
    pub profile_labels: Vec<String>,
    profile_scopes: Vec<(ProfileRef, Vec<Domain>)>,
    pub displays: Vec<Display>,
    /// Display connectors with nothing connected.
    pub ports: Vec<String>,
    pub draft: Option<RuleDraft>,
    pub confirm_delete: Option<String>,
    pub notice: Option<Notice>,
    /// The last change someone made through Halcyon, when it isn't
    /// automation's own.
    pub manual_at: Option<i64>,
    /// The desktop's latest change is automation's own last switch (Undo is
    /// on the Profiles page).
    pub undoable: bool,
    /// The parts in the hold file this window wrote.
    hold_written: BTreeSet<Domain>,
    /// Whether automation runs, as last read (`None`: not read yet).
    running: Option<bool>,
}

impl AutomationEditor {
    pub fn open(&mut self) {
        if self.store.is_none() {
            self.store = Some(DesktopStore::from_env().map_err(|error| error.to_string()));
        }
        self.refresh();
    }
    #[cfg(test)]
    pub fn with_store(store: DesktopStore) -> Self {
        let mut editor = Self {
            store: Some(Ok(store)),
            ..Default::default()
        };
        editor.refresh();
        editor
    }
    pub(super) fn restore_session(&mut self, saved: RuleDraft) -> Result<(), String> {
        if saved.time.len() > 64
            || saved.id.as_ref().is_some_and(|id| id.len() > 128)
            || saved
                .display
                .as_ref()
                .is_some_and(|display| display.len() > 128)
            || saved
                .start
                .as_ref()
                .as_ref()
                .is_some_and(|start| start.start.is_some())
            || saved.id.as_deref() != saved.baseline.as_ref().map(|rule| rule.id.as_str())
        {
            return Err("The saved schedule draft is not valid".into());
        }
        self.draft = Some(saved);
        self.notice = Some(Notice::Done(super::session_drafts::RECOVERED.into()));
        Ok(())
    }

    pub fn opened(&self) -> bool {
        self.store.is_some()
    }
    pub fn store(&self) -> Option<&DesktopStore> {
        self.store.as_ref().and_then(|store| store.as_ref().ok())
    }
    pub fn unavailable(&self) -> Option<&str> {
        self.store
            .as_ref()
            .and_then(|store| store.as_ref().err())
            .map(String::as_str)
    }
    pub fn writable(&self) -> bool {
        self.store().is_some_and(DesktopStore::writable)
    }
    /// The saved settings (automation off until read).
    pub fn settings(&self) -> Result<&Automation, &str> {
        match &self.settings {
            Some(Ok(settings)) => Ok(settings),
            Some(Err(problem)) => Err(problem),
            None => Err(""),
        }
    }
    #[cfg(test)]
    pub fn dirty(&self) -> bool {
        self.draft.as_ref().is_some_and(RuleDraft::dirty)
    }
    /// Files whose changes the page shows.
    pub fn watch_paths(&self) -> Vec<PathBuf> {
        self.store()
            .map(|store| {
                let mut paths = vec![
                    automation::path(store, automation::FILE),
                    automation::path(store, automation::STATE_FILE),
                ];
                paths.extend(store.app_config().map(std::path::Path::to_path_buf));
                paths
            })
            .unwrap_or_default()
    }
    pub fn state_key(&self) -> String {
        format!(
            "{:?}:{:?}:{:?}:{:?}:{:?}:{:?}:{:?}:{:?}:{:?}:{:?}",
            self.settings,
            self.state,
            self.profiles,
            self.displays,
            self.draft,
            self.confirm_delete,
            self.notice,
            self.manual_at,
            self.solar,
            self.profile_scopes
        )
    }

    /// Re-reads the settings, the desktop's state, the profiles, the
    /// connected displays and the last manual change. Reads only.
    pub fn refresh(&mut self) {
        let Some(store) = self.store() else {
            return;
        };
        let settings = automation::load(store);
        let state = automation::load_state(store);
        let mut known = profiles::presets()
            .into_iter()
            .chain(profiles::adaptive::presets())
            .map(|preset| (ProfileRef::Preset(preset.id.into()), preset.profile))
            .collect::<Vec<_>>();
        let builtins = known.len();
        if let Ok(entries) = ProfileLibrary::new(store).list() {
            known.extend(entries.into_iter().filter_map(|entry| {
                Some((
                    ProfileRef::Saved(entry.id.as_str().into()),
                    entry.profile.ok()?,
                ))
            }));
        }
        let scopes = known
            .iter()
            .map(|(id, profile)| (id.clone(), profile.domains()))
            .collect();
        let mut choices = known
            .into_iter()
            .map(|(id, profile)| (id, profile.name))
            .collect::<Vec<_>>();
        choices.splice(
            builtins..builtins,
            crate::moments::Phase::ALL.into_iter().map(|phase| {
                (
                    ProfileRef::Moment(phase),
                    format!("{} moment", phase.title()),
                )
            }),
        );
        let displays = automation::displays::describe(store);
        let ports = automation::displays::ports(store);
        let latest = store.latest_change().ok().flatten();
        let undoable = latest
            .as_ref()
            .is_some_and(|latest| state.applied.as_deref() == Some(latest.id.as_str()));
        let manual_at = latest
            .filter(|latest| state.applied.as_deref() != Some(latest.id.as_str()))
            .and_then(|latest| chrono::DateTime::parse_from_rfc3339(&latest.created).ok())
            .map(|at| at.timestamp());
        self.solar = settings
            .as_ref()
            .map(|settings| Context::read(store, settings))
            .unwrap_or_default();
        self.settings = Some(settings);
        self.state = state;
        self.profile_labels = choices.iter().map(|(_, name)| name.clone()).collect();
        self.profiles = choices;
        self.profile_scopes = scopes;
        self.displays = displays;
        self.ports = ports;
        self.manual_at = manual_at;
        self.undoable = undoable;
        self.running = Some(
            self.writable()
                && self
                    .settings
                    .as_ref()
                    .is_some_and(|settings| settings.as_ref().is_ok_and(Automation::running)),
        );
    }

    pub fn profile_scope(&self, profile: &ProfileRef, kind: Kind) -> String {
        if matches!(profile, ProfileRef::Moment(_)) {
            return "Widget style only · wallpaper, layout and content stay.".into();
        }
        if kind == Kind::Solar {
            return "Appearance only · layout, applets and content stay.".into();
        }
        self.profile_scopes
            .iter()
            .find(|(id, _)| id == profile)
            .map(|(_, domains)| {
                format!(
                    "Changes: {}.",
                    domains
                        .iter()
                        .map(|d| d.title())
                        .collect::<Vec<_>>()
                        .join(", ")
                )
            })
            .unwrap_or_else(|| "The chosen setup is unavailable.".into())
    }

    /// The profile name to show for a rule (the saved name when it's gone).
    pub fn profile_name(&self, profile: &ProfileRef, saved: &str) -> String {
        self.profiles
            .iter()
            .find(|(known, _)| known == profile)
            .map_or_else(|| saved.to_string(), |(_, name)| name.clone())
    }
    /// What's wrong with a rule as it stands, if anything.
    pub fn rule_problem(&self, rule: &Rule) -> Option<String> {
        if let Trigger::Solar { source, .. } = rule.when {
            if let Err(reason) = self.solar.city(source) {
                return Some(reason);
            }
        }
        if self
            .profiles
            .iter()
            .any(|(known, _)| *known == rule.profile)
        {
            return None;
        }
        Some(match &rule.profile {
            ProfileRef::Moment(phase) => format!("{} moment is unavailable", phase.title()),
            ProfileRef::Saved(_) => format!("“{}” was deleted", rule.name),
            ProfileRef::Preset(_) => format!("“{}” isn’t available", rule.name),
        })
    }
    /// Displays a display rule can name: connected ones, then ones rules
    /// remember that aren't connected now.
    pub fn display_choices(&self) -> Vec<(String, String, bool)> {
        let mut choices: Vec<(String, String, bool)> = self
            .displays
            .iter()
            .map(|display| (display.name.clone(), display.title(), true))
            .collect();
        // Free ports too, so a rule can wait for a display not plugged in.
        for port in &self.ports {
            if choices.iter().all(|(known, _, _)| known != port) {
                choices.push((port.clone(), port.clone(), false));
            }
        }
        let remembered = self
            .settings()
            .ok()
            .into_iter()
            .flat_map(|settings| settings.rules.iter())
            .filter_map(|rule| match &rule.when {
                Trigger::Display(when) => {
                    when.display.clone().map(|name| (name, when.label.clone()))
                }
                _ => None,
            })
            .chain(self.draft.iter().filter_map(|draft| {
                draft
                    .display
                    .clone()
                    .map(|name| (name, draft.label.clone()))
            }));
        for (name, label) in remembered {
            if choices.iter().all(|(known, _, _)| *known != name) {
                let title = match label {
                    Some(label) => format!("{label} · {name}"),
                    None => name.clone(),
                };
                choices.push((name, title, false));
            }
        }
        choices
    }

    /// Whether drafts need declaring: automation is on, or a hold is still
    /// written. Reads the settings once, if this page wasn't opened.
    pub fn hold_matters(&mut self) -> bool {
        if self.running.is_none() {
            if self.store.is_none() {
                self.store = Some(DesktopStore::from_env().map_err(|error| error.to_string()));
            }
            self.running = Some(
                self.store()
                    .filter(|store| store.writable())
                    .is_some_and(|store| automation::load(store).is_ok_and(|s| s.running())),
            );
        }
        self.running == Some(true) || !self.hold_written.is_empty()
    }
    /// Declares the parts with unapplied Settings drafts while automation
    /// runs (written only when they change; an empty set removes the file).
    /// Without automation nothing is written.
    pub fn sync_hold(&mut self, parts: BTreeSet<Domain>) {
        if !self.hold_matters() {
            return;
        }
        let Some(store) = self.store().filter(|store| store.writable()) else {
            return;
        };
        let wanted = if self.running == Some(true) {
            parts
        } else {
            BTreeSet::new()
        };
        if (!wanted.is_empty() || wanted != self.hold_written)
            && automation::write_hold(store, &wanted).is_ok()
        {
            self.hold_written = wanted;
        }
    }
    /// Release the live lease. Manual saves already have durable store history;
    /// write_hold also preserves any stamp left by an older Settings version.
    pub fn release_hold(&mut self) {
        if !self.hold_written.is_empty()
            && let Some(store) = self.store().filter(|store| store.writable())
        {
            let _ = automation::write_hold(store, &BTreeSet::new());
        }
        self.hold_written.clear();
    }

    pub fn update(&mut self, message: Message, h24: bool) {
        if message.writes() && !matches!(message, Message::Refresh) {
            self.notice = None;
        }
        let store = self.store().cloned();
        match message {
            Message::AdaptiveRoutine(look, source) => {
                self.change(|settings, now| {
                    let store=store.as_ref().ok_or("Desktop settings unavailable")?;
                    Context::read(store,settings).city(source)?;
                    if settings.rules.iter().any(|r| matches!(&r.profile,ProfileRef::Preset(id) if id.starts_with("adaptive-"))) {
                        return Err("Adaptive rules already exist. Edit them in Automation.".into());
                    }
                    if settings.rules.len()+3 > automation::rules::MAX_RULES {
                        return Err("Remove three rules before adding this daily cycle.".into());
                    }
                    for phase in crate::desktop::profiles::adaptive::Phase::ALL {
                      let id=settings.next_id();
                      settings.rules.push(Rule {
                        id, enabled:true, since:now,
                        profile:ProfileRef::Preset(look.id(phase).into()),
                        name:format!("{} · {}",look.title(),phase.title()),
                        when:Trigger::Solar { event:phase.event(),days:Days::EVERY,source },
                      });
                    }
                    if !settings.enabled { settings.enabled=true;settings.since=now; }
                    Ok(())
                }, Some("Daily cycle added. It starts at the next sun event.".into()));
            }
            Message::ManualCity(city) => {
                self.change(
                    |settings, _| {
                        solar::validate_city(&city)?;
                        settings.manual_city = Some(city);
                        Ok(())
                    },
                    Some("Schedule city saved".into()),
                );
            }
            Message::Enable(on) => self.change(
                |settings, now| {
                    settings.enabled = on;
                    settings.paused = None;
                    if on {
                        settings.since = now;
                    }
                    Ok(())
                },
                None,
            ),
            Message::Rule(id, on) => self.change(
                |settings, now| {
                    if on
                        && let Some(rule) = settings.rule(&id)
                        && let Trigger::Solar { source, .. } = rule.when
                    {
                        Context::read(
                            store.as_ref().ok_or("Desktop settings unavailable")?,
                            settings,
                        )
                        .city(source)?;
                    }
                    let rule = rule_mut(settings, &id)?;
                    rule.enabled = on;
                    if on {
                        rule.since = now;
                    }
                    Ok(())
                },
                None,
            ),
            Message::Move(id, step) => self.change(
                |settings, _| {
                    let index = settings
                        .rules
                        .iter()
                        .position(|rule| rule.id == id)
                        .ok_or_else(gone)?;
                    let target = index as i32 + step.signum();
                    if (0..settings.rules.len() as i32).contains(&target) {
                        settings.rules.swap(index, target as usize);
                    }
                    Ok(())
                },
                None,
            ),
            Message::Edit(id) => {
                self.confirm_delete = None;
                if let Ok(rule) = self.settings().map(|settings| settings.rule(&id).cloned())
                    && let Some(rule) = rule
                {
                    self.draft = Some(RuleDraft::from_rule(&rule, h24));
                }
            }
            Message::New(kind) => {
                self.confirm_delete = None;
                let mut draft = RuleDraft::new(kind, h24);
                if kind == Kind::Solar && !self.solar.weather_enabled {
                    draft.source = Source::Manual;
                }
                self.draft = Some(draft);
            }
            Message::Schedule(profile) => {
                // A cross-page shortcut must never overwrite an existing draft.
                if self.draft.is_some() {
                    self.notice = Some(Notice::Error(
                        "Finish or cancel the current rule first.".into(),
                    ));
                } else {
                    self.confirm_delete = None;
                    let mut draft = RuleDraft::new(Kind::Schedule, h24);
                    draft.profile = Some(profile);
                    self.draft = Some(draft);
                }
            }
            Message::Draft(edit) => self.edit_draft(edit),
            Message::Save => self.save(),
            Message::Cancel => self.draft = None,
            Message::Delete(id, false) => {
                self.confirm_delete = if self.confirm_delete.as_ref() == Some(&id) {
                    None
                } else {
                    Some(id)
                };
            }
            Message::Delete(id, true) => {
                self.confirm_delete = None;
                let name = self
                    .settings()
                    .ok()
                    .and_then(|settings| settings.rule(&id))
                    .map(|rule| rule.name.clone());
                self.write(
                    |settings, _| {
                        settings.rules.retain(|rule| rule.id != id);
                        Ok(())
                    },
                    name.map(|name| format!("Removed the rule for {name}")),
                );
                if self
                    .draft
                    .as_ref()
                    .is_some_and(|draft| draft.id.as_deref() == Some(id.as_str()))
                {
                    self.draft = None;
                }
            }
            Message::Pause(choice) => {
                let tz = Local;
                let now = chrono::Utc::now();
                let next = self.settings().ok().and_then(|settings| {
                    automation::decide::next_instant(settings, &self.state, &tz, now, &self.solar)
                        .map(|(at, _)| at.timestamp())
                });
                let (until, skip) = match choice {
                    PauseFor::Next => match next {
                        Some(at) => (Some(at + 1), true),
                        None => return,
                    },
                    PauseFor::Hour => (Some(now.timestamp() + 3600), false),
                    PauseFor::Today => (Some(automation::status::tomorrow(&tz, now)), false),
                    PauseFor::Resume => (None, false),
                };
                self.write(
                    |settings, now| {
                        settings.paused = Some(Pause {
                            since: now,
                            until,
                            skip,
                        });
                        Ok(())
                    },
                    None,
                );
            }
            Message::Resume => self.change(
                |settings, now| {
                    // Nothing from a pause is caught up afterwards. A skip
                    // paused nothing, so it just ends.
                    if settings.paused.take().is_some_and(|pause| !pause.skip) {
                        settings.since = now;
                    }
                    Ok(())
                },
                None,
            ),
            Message::Refresh => self.refresh(),
            Message::StartOver => {
                let Some(store) = self.store() else {
                    return;
                };
                let result = automation::reset(store);
                match result {
                    Ok(()) => self.notice = Some(Notice::Done("Automation settings reset".into())),
                    Err(error) => self.notice = Some(Notice::Error(error)),
                }
                self.refresh();
            }
        }
    }

    fn edit_draft(&mut self, edit: DraftEdit) {
        let Some(draft) = &mut self.draft else {
            return;
        };
        match edit {
            DraftEdit::Kind(kind) => {
                // Existing rules keep their trigger type; new drafts can choose
                // when the selected appearance should apply.
                if draft.id.is_none() {
                    draft.kind = kind;
                    if kind == Kind::Solar && !self.solar.weather_enabled {
                        draft.source = Source::Manual;
                    }
                }
            }
            DraftEdit::Event(event) => draft.event = event,
            DraftEdit::Source(source) => draft.source = source,
            DraftEdit::Time(text) => draft.time = text.chars().take(12).collect(),
            DraftEdit::Day(index) => draft.days = draft.days.toggled(index),
            DraftEdit::Days(days) => draft.days = days,
            DraftEdit::Change(change) => draft.change = change,
            DraftEdit::Display(name) => {
                draft.label = name.as_ref().and_then(|name| {
                    self.displays
                        .iter()
                        .find(|display| display.name == *name)
                        .and_then(|display| display.label.clone())
                        .or_else(|| {
                            (draft.display.as_ref() == Some(name))
                                .then(|| draft.label.clone())
                                .flatten()
                        })
                });
                draft.display = name;
            }
            DraftEdit::Profile(profile) => draft.profile = Some(profile),
        }
    }

    fn save(&mut self) {
        let Some(draft) = self.draft.clone() else {
            return;
        };
        if let Some(missing) = draft.missing() {
            self.notice = Some(Notice::Error(format!("{missing}.")));
            return;
        }
        let (Some(trigger), Some(profile)) = (draft.trigger(), draft.profile.clone()) else {
            return;
        };
        let name = self.profile_name(&profile, "");
        let store = self.store().cloned();
        let mut turned_on = false;
        let result = self.write(
            |settings, now| {
                if let Trigger::Solar { source, .. } = trigger {
                    Context::read(store.as_ref().ok_or("Desktop settings unavailable")?, settings).city(source)?;
                }
                match &draft.id {
                    Some(id) => {
                        let rule = rule_mut(settings, id)?;
                        if draft.baseline.as_ref() != Some(&*rule) {
                            return Err("This rule changed outside this draft. Cancel and reopen it to use the current rule; nothing was saved.".into());
                        }
                        rule.when = trigger;
                        rule.profile = profile;
                        rule.name = name.clone();
                        rule.since = now;
                    }
                    None => {
                        // The editor exposes this as “Save & enable”.
                        if draft.enables_automation(settings) {
                            settings.enabled = true;
                            settings.since = now;
                            turned_on = true;
                        }
                        let id = settings.next_id();
                        settings.rules.push(Rule {
                            id,
                            enabled: true,
                            since: now,
                            profile,
                            name: name.clone(),
                            when: trigger,
                        });
                    }
                }
                Ok(())
            },
            Some(format!("Saved the rule for {name}")),
        );
        if result {
            self.draft = None;
            if turned_on {
                self.notice = Some(Notice::Done(format!(
                    "Saved the rule for {name} · automation is on"
                )));
            }
        }
    }

    fn change(
        &mut self,
        change: impl FnOnce(&mut Automation, i64) -> Result<(), String>,
        done: Option<String>,
    ) {
        self.write(change, done);
    }
    /// Writes a change to the current settings; `true` when it was saved.
    fn write(
        &mut self,
        change: impl FnOnce(&mut Automation, i64) -> Result<(), String>,
        done: Option<String>,
    ) -> bool {
        let Some(store) = self.store() else {
            return false;
        };
        if !store.writable() {
            self.notice = Some(Notice::Error(
                "Automation can’t be changed in preview mode".into(),
            ));
            return false;
        }
        let now = automation::now();
        let result = automation::update(store, |settings| change(settings, now));
        let saved = result.is_ok();
        match result {
            Ok(settings) => {
                self.solar = Context::read(store, &settings);
                self.running = Some(settings.running());
                self.settings = Some(Ok(settings));
                if let Some(done) = done {
                    self.notice = Some(Notice::Done(done));
                }
            }
            Err(error) => self.notice = Some(Notice::Error(error)),
        }
        saved
    }
}

fn rule_mut<'a>(settings: &'a mut Automation, id: &str) -> Result<&'a mut Rule, String> {
    settings
        .rules
        .iter_mut()
        .find(|rule| rule.id == id)
        .ok_or_else(gone)
}

fn gone() -> String {
    "This rule was removed meanwhile.".into()
}

#[cfg(test)]
mod tests;

#[cfg(test)]
mod polish050_tests {
    use super::*;

    #[test]
    fn polish050_schedule_shortcut_retains_target_and_never_replaces_a_draft() {
        let mut editor = AutomationEditor::default();
        let day = ProfileRef::Moment(crate::moments::Phase::Day);
        let night = ProfileRef::Moment(crate::moments::Phase::Night);
        assert!(!Message::Schedule(day.clone()).writes());
        editor.update(Message::Schedule(day.clone()), true);
        assert_eq!(editor.draft.as_ref().unwrap().profile, Some(day.clone()));
        editor.update(Message::Draft(DraftEdit::Kind(Kind::Solar)), true);
        assert_eq!(editor.draft.as_ref().unwrap().source, Source::Manual);
        assert_eq!(editor.draft.as_ref().unwrap().profile, Some(day));
        let before = editor.draft.clone();
        editor.update(Message::Schedule(night), true);
        assert_eq!(editor.draft, before);
        assert!(matches!(editor.notice, Some(Notice::Error(_))));
    }

    #[test]
    fn polish050_schedule_scope_distinguishes_solar_projection_from_setup_layout() {
        let mut editor = AutomationEditor::default();
        let profile = ProfileRef::Preset("tidal".into());
        editor.profile_scopes.push((
            profile.clone(),
            vec![Domain::Wallpaper, Domain::WidgetLayout],
        ));
        assert!(
            editor
                .profile_scope(&profile, Kind::Schedule)
                .contains("Widget layout")
        );
        assert!(
            !editor
                .profile_scope(&profile, Kind::Solar)
                .contains("Changes:")
        );
        assert!(
            editor
                .profile_scope(&profile, Kind::Solar)
                .contains("layout, applets and content stay")
        );
        assert!(
            editor
                .profile_scope(&ProfileRef::Moment(crate::moments::Phase::Day), Kind::Solar)
                .starts_with("Widget style only")
        );
    }
}
