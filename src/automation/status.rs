//! The words Settings shows: what happens next, what happened last and why.
use super::clock;
use super::decide::{Effect, Outcome, State};
use super::rules::{Automation, Time, Trigger};
use chrono::{DateTime, NaiveDateTime, TimeZone, Timelike, Utc};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tone {
    /// Off or nothing to do.
    Quiet,
    Active,
    Paused,
    /// Something needs a look (a missing profile, a waiting switch).
    Attention,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Status {
    pub headline: String,
    pub detail: Option<String>,
    pub tone: Tone,
}

/// "19:00", "tomorrow at 07:00", "Monday at 07:00", "Oct 25 at 07:00".
pub fn when(local: NaiveDateTime, now: NaiveDateTime, h24: bool) -> String {
    let time = Time::new(local.hour(), local.minute())
        .map(|time| time.format(h24))
        .unwrap_or_default();
    let days = (local.date() - now.date()).num_days();
    match days {
        0 => format!("at {time}"),
        1 => format!("tomorrow at {time}"),
        2..=6 => format!("{} at {time}", local.format("%A")),
        _ => format!("{} at {time}", local.format("%b %-d")),
    }
}

/// "07:00", "yesterday 19:00", "Mon 19:00", "Oct 2 19:00" for history.
pub fn stamp(local: NaiveDateTime, now: NaiveDateTime, h24: bool) -> String {
    let time = Time::new(local.hour(), local.minute())
        .map(|time| time.format(h24))
        .unwrap_or_default();
    match (now.date() - local.date()).num_days() {
        0 => time,
        1 => format!("Yesterday {time}"),
        2..=6 => format!("{} {time}", local.format("%a")),
        _ => format!("{} {time}", local.format("%b %-d")),
    }
}

/// One history entry in words.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Line {
    /// "Applied Day", "Kept your changes instead of Evening", …
    pub what: String,
    /// "Scheduled for 19:00", "Dell U2720Q connected".
    pub cause: String,
    /// Why it went that way, when that needs saying.
    pub why: Option<String>,
}

pub fn outcome_line(outcome: &Outcome, h24: bool) -> Line {
    let name = &outcome.due.name;
    let (what, why) = match &outcome.effect {
        Effect::Applied { skipped } if skipped.is_empty() => (format!("Applied {name}"), None),
        Effect::Applied { skipped } => (
            format!("Applied {name} without {}", parts(skipped)),
            Some(skipped.join(" ")),
        ),
        Effect::AlreadySet => (format!("{name} was already in place"), None),
        Effect::KeptManual { .. } => (
            format!("Kept your changes instead of {name}"),
            Some("You changed the desktop after it was due.".into()),
        ),
        Effect::Waiting { parts } => (
            format!("{name} is waiting"),
            Some(waiting_choice(&parts.join(", "))),
        ),
        Effect::Failed { reason } => (format!("Couldn’t apply {name}"), Some(reason.clone())),
        Effect::Replaced { by } => (format!("{name} was replaced by {by}"), None),
    };
    let cause = match &outcome.due.cause {
        super::decide::Cause::Schedule(_) => format!("Scheduled for {}", outcome.due.reason(h24)),
        super::decide::Cause::Display(_) | super::decide::Cause::Solar { .. } => {
            outcome.due.reason(h24)
        }
    };
    Line { what, cause, why }
}

/// What each way out of a wait does: applying the drafts is a manual
/// change, so it wins; discarding them lets the switch run.
fn waiting_choice(parts: &str) -> String {
    format!(
        "Settings has unapplied {parts} changes. Discard them to switch, or apply them to keep yours instead."
    )
}

/// "Wallpaper" from "Wallpaper: … was moved." lines.
fn parts(skipped: &[String]) -> String {
    skipped
        .iter()
        .map(|line| line.split_once(':').map_or(line.as_str(), |(part, _)| part))
        .collect::<Vec<_>>()
        .join(", ")
}

/// The status card. `manual_at`: the last change someone made through
/// Halcyon after automation's own last switch.
pub fn status<Tz: TimeZone>(
    automation: &Automation,
    state: &State,
    tz: &Tz,
    now: DateTime<Utc>,
    manual_at: Option<i64>,
    h24: bool,
) -> Status {
    status_with_solar(
        automation,
        state,
        tz,
        now,
        manual_at,
        h24,
        &super::solar::Context::default(),
    )
}
pub fn status_with_solar<Tz: TimeZone>(
    automation: &Automation,
    state: &State,
    tz: &Tz,
    now: DateTime<Utc>,
    manual_at: Option<i64>,
    h24: bool,
    context: &super::solar::Context,
) -> Status {
    let now_local = clock::local(tz, now);
    let now_unix = clock::unix(now);
    let next = super::decide::next_instant(automation, state, tz, now, context)
        .map(|(at, index)| (clock::local(tz, at), index));
    let next_text = next.map(|(local, index)| {
        format!(
            "{} {}",
            automation.rules[index].name,
            when(local, now_local, h24)
        )
    });
    if !automation.enabled {
        return Status {
            headline: "Off".into(),
            detail: Some(if automation.rules.is_empty() {
                "Your first saved rule turns automation on.".into()
            } else {
                "Nothing switches automatically.".into()
            }),
            tone: Tone::Quiet,
        };
    }
    if automation.active().next().is_none() {
        return Status {
            headline: "No rules turned on".into(),
            detail: Some("Turn on a rule below, or add one.".into()),
            tone: Tone::Quiet,
        };
    }
    if let Some(pause) = automation.paused_at(now_unix) {
        let headline = match pause.until {
            Some(until) => {
                let end = clock::local(tz, clock::from_unix(until));
                let midnight = end.hour() == 0 && end.minute() == 0;
                match (end.date() - now_local.date()).num_days() {
                    1 if midnight => "Paused until tomorrow".into(),
                    _ => format!(
                        "Paused until {}",
                        when(end, now_local, h24).trim_start_matches("at ")
                    ),
                }
            }
            None => "Paused until you resume".into(),
        };
        return Status {
            headline,
            detail: next_text.map(|next| format!("Then {next}")),
            tone: Tone::Paused,
        };
    }
    if let Some(pending) = &state.pending {
        let parts = state
            .recent
            .iter()
            .find_map(|outcome| match &outcome.effect {
                Effect::Waiting { parts } if outcome.due == *pending => Some(parts.join(", ")),
                _ => None,
            })
            .unwrap_or_else(|| "desktop".into());
        return Status {
            headline: format!("{} is waiting", pending.name),
            detail: Some(waiting_choice(&parts)),
            tone: Tone::Attention,
        };
    }
    if let Some(until) = automation.skipping_at(now_unix).and_then(|skip| skip.until) {
        let at = clock::local(tz, clock::from_unix(until - 1));
        let skipped = automation
            .rules
            .iter()
            .filter(|rule| {
                matches!(rule.when, Trigger::Schedule { time, .. }
                    if time.hour() == at.hour() && time.minute() == at.minute())
            })
            .map(|rule| rule.name.clone())
            .next()
            .unwrap_or_else(|| "the next switch".into());
        let then = next_text.map(|next| format!("Then {next}"));
        let displays = automation
            .watches_displays()
            .then_some("display rules still switch");
        return Status {
            headline: format!("Skipping {skipped} {}", when(at, now_local, h24)),
            detail: Some(
                [then.as_deref(), displays]
                    .into_iter()
                    .flatten()
                    .collect::<Vec<_>>()
                    .join(" · "),
            )
            .filter(|detail| !detail.is_empty()),
            tone: Tone::Paused,
        };
    }
    let headline = match &next_text {
        Some(next) => format!("Next: {next}"),
        None if automation.watches_displays() => "Watching for display changes".into(),
        None => "No switch available yet".into(),
    };
    let last = state.last();
    let detail = match (manual_at, last) {
        (Some(manual), last)
            if last.is_none_or(|last| manual > last.at)
                && next.is_some()
                && manual > automation.since =>
        {
            let at = clock::local(tz, clock::from_unix(manual));
            Some(format!(
                "You changed the desktop at {} · it stays until the next switch",
                Time::new(at.hour(), at.minute())
                    .map(|time| time.format(h24))
                    .unwrap_or_default()
            ))
        }
        (_, Some(last)) => {
            let at = clock::local(tz, clock::from_unix(last.at));
            Some(format!(
                "Last: {} · {}",
                outcome_line(last, h24).what,
                stamp(at, now_local, h24)
            ))
        }
        _ => None,
    };
    let tone = match last.map(|last| &last.effect) {
        Some(Effect::Failed { .. }) => Tone::Attention,
        _ => Tone::Active,
    };
    Status {
        headline,
        detail,
        tone,
    }
}

/// The end of a "Until tomorrow" pause: the next local midnight.
pub fn tomorrow<Tz: TimeZone>(tz: &Tz, now: DateTime<Utc>) -> i64 {
    let local = clock::local(tz, now);
    let midnight = local
        .date()
        .succ_opt()
        .unwrap_or(local.date())
        .and_hms_opt(0, 0, 0)
        .unwrap_or(local);
    clock::unix(clock::instant(tz, midnight))
}
