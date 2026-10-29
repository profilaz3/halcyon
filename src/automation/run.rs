//! Applying a due switch through the profile engine: the same plan, asset
//! checks, conflict detection, journaled transaction and Undo record as the
//! Profiles page's Apply. Blocking; the desktop runs it off the UI thread.
use super::decide::Effect;
use super::rules::ProfileRef;
use crate::desktop::profiles::{self, Domain, Installed, Profile, ProfileId, ProfileLibrary};
use crate::desktop::{DesktopError, DesktopStore};
use std::collections::BTreeSet;

/// What became of one switch.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Ran {
    /// Applied; the store's change ID and the parts left out, with why.
    Applied {
        change: String,
        skipped: Vec<String>,
    },
    /// The desktop already matched (any blocked parts left out).
    AlreadySet,
    /// Settings has unapplied changes to these parts.
    Waiting(BTreeSet<Domain>),
    Failed(String),
    KeptManual(i64),
}

impl Ran {
    pub fn effect(&self) -> Effect {
        match self {
            Self::Applied { skipped, .. } => Effect::Applied {
                skipped: skipped.clone(),
            },
            Self::AlreadySet => Effect::AlreadySet,
            Self::Waiting(parts) => Effect::Waiting {
                parts: parts.iter().map(|part| part.title().to_string()).collect(),
            },
            Self::KeptManual(manual_at) => Effect::KeptManual {
                manual_at: *manual_at,
            },
            Self::Failed(reason) => Effect::Failed {
                reason: reason.clone(),
            },
        }
    }
}

/// The profile a rule names, or why it can't be used.
pub fn load(store: &DesktopStore, target: &ProfileRef, name: &str) -> Result<Profile, String> {
    match target {
        ProfileRef::Moment(phase) => {
            let mut profile = Profile::new(&format!("{} moment", phase.title())).unwrap();
            profile.widget_style = Some(serde_json::json!({"moments":{"active":phase}}));
            Ok(profile)
        }
        ProfileRef::Preset(id) => {
            let preset = profiles::preset(id)
                .ok_or_else(|| format!("The look “{name}” isn’t available."))?;
            // Fitted to this person: no empty Projects card, and their own
            // widget choices kept. No file yet means a new person; a file
            // that can't be read must not be mistaken for one.
            let unreadable = || "Your widget settings couldn't be read.".to_string();
            let text = store
                .app_config()
                .map(crate::config::read_config_text)
                .transpose()
                .map_err(|_| unreadable())?
                .flatten();
            let config = match text {
                Some(text) => {
                    crate::config::Config::from_bytes(text.as_bytes()).map_err(|_| unreadable())?
                }
                None => crate::config::Config::default(),
            };
            Ok(profiles::fitted(
                preset,
                &config,
                &crate::desktop::wallpaper::bundled_dir(store),
                profiles::islands_available(store),
            )
            .profile)
        }
        ProfileRef::Saved(id) => {
            let id = ProfileId::parse(id).map_err(|error| error.to_string())?;
            ProfileLibrary::new(store)
                .load(&id)
                .map_err(|error| match error {
                    profiles::ProfileError::NotFound(_) => {
                        format!(
                            "The profile “{name}” was deleted. Choose another one for this rule."
                        )
                    }
                    error => format!("“{name}” can’t be read: {error}"),
                })
        }
    }
}

/// Applies every part of the profile that can be applied. Parts that are
/// blocked (a moved wallpaper picture, a missing component) are left out
/// and named; the other parts and every unrelated setting stay intact. A
/// conflict with a change made meanwhile is planned again once. Nothing is
/// written while `held` parts have unapplied Settings changes, while an
/// interrupted change waits for a decision, or in preview mode.
pub fn run(
    store: &DesktopStore,
    installed: &Installed,
    target: &ProfileRef,
    name: &str,
    held: &BTreeSet<Domain>,
) -> Ran {
    run_for_cause(store, installed, target, name, held, false, None)
}
pub(super) fn run_for_cause(
    store: &DesktopStore,
    installed: &Installed,
    target: &ProfileRef,
    name: &str,
    held: &BTreeSet<Domain>,
    display: bool,
    solar: Option<(&super::solar::Context, super::solar::Source)>,
) -> Ran {
    if !store.writable() {
        return Ran::Failed("Automatic switching is off in preview mode.".into());
    }
    match store.pending_recovery() {
        Ok(None) => {}
        Ok(Some(_)) => {
            return Ran::Failed(
                "An interrupted change needs a decision on the Profiles page first.".into(),
            );
        }
        Err(error) => return Ran::Failed(error.to_string()),
    }
    for attempt in 0..2 {
        // Fitted again on every attempt, so a retry never applies a look
        // fitted to widget settings that have since changed.
        let profile = match load(store, target, name).and_then(|profile| {
            if solar.is_some() && !matches!(target, ProfileRef::Moment(_)) {
                super::appearance::project(&profile)
            } else {
                Ok(profile)
            }
        }) {
            Ok(profile) => profile,
            Err(reason) => return Ran::Failed(reason),
        };
        let label = label(&profile.name);
        let selected: BTreeSet<Domain> = profile.domains().into_iter().collect();
        let mut plan = profiles::plan(store, installed, &profile, &selected);
        if let Some((context, source)) = solar {
            if let Err(reason) = context
                .guards(source)
                .and_then(|guards| plan.guard_widgets(guards))
            {
                return Ran::Failed(reason);
            }
        }
        plan.from_display_rule(display);
        let skipped = Vec::new();
        // A coordinated setup is atomic. A missing asset or unavailable
        // component cannot silently narrow the categories the rule chose.
        if let Some(reason) = plan.blocking_reasons().into_iter().next() {
            return Ran::Failed(reason);
        }
        if plan.is_empty() {
            return Ran::AlreadySet;
        }
        let waits: BTreeSet<Domain> = plan
            .domains
            .iter()
            .filter(|part| !part.changes.is_empty() && held.contains(&part.domain))
            .map(|part| part.domain)
            .collect();
        if !waits.is_empty() {
            return Ran::Waiting(waits);
        }
        match profiles::apply_as(store, &plan, &label) {
            Ok(latest) => {
                return Ran::Applied {
                    change: latest.id,
                    skipped,
                };
            }
            Err(DesktopError::Conflict(_) | DesktopError::FieldConflict { .. }) if attempt == 0 => {
                continue;
            }
            Err(DesktopError::InvalidValue { message, .. }) if message == "Nothing to change" => {
                return Ran::AlreadySet;
            }
            Err(error) => return Ran::Failed(error.to_string()),
        }
    }
    Ran::Failed("Settings kept changing while switching. Nothing was changed.".into())
}

/// The Undo label of an automatic switch.
pub fn label(name: &str) -> String {
    format!("Profile “{name}” (automatic)")
        .chars()
        .take(120)
        .collect()
}
