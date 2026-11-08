//! One Corners choice for windows, widgets, the dock and the panel.
//!
//! Each surface stores corners its own way: widget cards a radius of up to
//! 32 px, the bars a radius of up to `MAX_RADIUS` (a pill), and windows one
//! of COSMIC’s three roundnesses. A step names one value per surface. Saved
//! values are never rewritten to fit a step; they only *read* as the step
//! whose range they fall in, so nothing looks different until someone
//! chooses a step.
use crate::desktop::DesktopStore;
use crate::desktop::appearance::{AppearanceDraft, Control, Value};
use crate::desktop::panels::{MAX_RADIUS, PanelDraft, PanelKind, Setting};
use crate::desktop::store::{DesktopPatch, LatestChange, Owner, WidgetPatch, widget};
use cosmic::cosmic_theme::Roundness;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Step {
    Square,
    Rounded,
    VeryRound,
    Pill,
}

/// The change label in Changes and beside Undo.
pub const LABEL: &str = "Corners";
const WIDGET_POINTER: &str = "/appearance/corner_radius";

impl Step {
    pub const ALL: [Self; 4] = [Self::Square, Self::Rounded, Self::VeryRound, Self::Pill];

    pub fn title(self) -> &'static str {
        match self {
            Self::Square => "Square",
            Self::Rounded => "Rounded",
            Self::VeryRound => "Very round",
            Self::Pill => "Pill",
        }
    }
    pub fn key(self) -> &'static str {
        match self {
            Self::Square => "square",
            Self::Rounded => "rounded",
            Self::VeryRound => "very-round",
            Self::Pill => "pill",
        }
    }
    /// Widget cards: the values the widget choice always used, with the
    /// largest card radius for Pill.
    pub fn widget(self) -> u16 {
        match self {
            Self::Square => 0,
            Self::Rounded => 8,
            Self::VeryRound => 16,
            Self::Pill => 32,
        }
    }
    /// The dock and panel: the values their own choice always used.
    pub fn bar(self) -> u32 {
        match self {
            Self::Square => 0,
            Self::Rounded => 12,
            Self::VeryRound => 24,
            Self::Pill => MAX_RADIUS,
        }
    }
    /// COSMIC has no pill-shaped windows, so Pill windows are Very round.
    pub fn window(self) -> Roundness {
        match self {
            Self::Square => Roundness::Square,
            Self::Rounded => Roundness::SlightlyRound,
            Self::VeryRound | Self::Pill => Roundness::Round,
        }
    }
    /// The step a widget or bar radius reads as.
    pub fn of_radius(radius: u32) -> Self {
        match radius {
            0..4 => Self::Square,
            4..16 => Self::Rounded,
            16..32 => Self::VeryRound,
            _ => Self::Pill,
        }
    }
    /// The steps a window roundness reads as: round windows suit both Very
    /// round and Pill.
    fn of_window(roundness: Roundness) -> &'static [Self] {
        match roundness {
            Roundness::Square => &[Self::Square],
            Roundness::SlightlyRound => &[Self::Rounded],
            Roundness::Round => &[Self::VeryRound, Self::Pill],
        }
    }
    /// The one step every known surface reads as, or `None` when they
    /// differ. Values this version can’t read are left out.
    pub fn shown(widget: u16, bars: &[u32], window: Option<Roundness>) -> Option<Self> {
        let mut steps = vec![Self::of_radius(u32::from(widget))];
        steps.retain(|step| bars.iter().all(|bar| Self::of_radius(*bar) == *step));
        if let Some(window) = window {
            steps.retain(|step| Self::of_window(window).contains(step));
        }
        steps.first().copied()
    }
}

/// The bars whose corners can be read: listed or not, a bar with settings
/// keeps its corners in step with the others.
fn bars(store: &DesktopStore) -> Vec<PanelDraft> {
    PanelKind::ALL
        .into_iter()
        .filter_map(|kind| PanelDraft::load(store, kind).ok())
        .filter(|draft| draft.installed() && draft.radius().known().is_some())
        .collect()
}

/// The step the desktop shows now. Reads only.
pub fn current(store: &DesktopStore, widget: u16) -> Option<Step> {
    let bars: Vec<u32> = bars(store)
        .iter()
        .filter_map(|draft| draft.radius().known().copied())
        .collect();
    let window = AppearanceDraft::load(store)
        .ok()
        .and_then(|draft| draft.roundness(draft.active_variant()).known().copied());
    Step::shown(widget, &bars, window)
}

/// Every surface’s change for `step`: the bars and windows as native keys,
/// widget cards as their preference. A value already at the step is not
/// written; one this version can’t read is kept.
fn patches(store: &DesktopStore, step: Step) -> Result<(DesktopPatch, WidgetPatch), String> {
    let mut patch = DesktopPatch::new();
    for mut draft in bars(store) {
        draft.set_radius(step.bar());
        patch
            .merge(draft.patch())
            .map_err(|error| error.to_string())?;
    }
    let mut windows = AppearanceDraft::load(store).map_err(|error| error.to_string())?;
    if let Setting::Known(_) = windows.roundness(windows.active_variant()) {
        windows.set(Control::Roundness, Value::Roundness(step.window()));
        if let Some(blocked) = windows.blocked().first() {
            return Err(blocked.reason.clone());
        }
        patch
            .merge(windows.patch())
            .map_err(|error| error.to_string())?;
    }
    let (raw, config) = crate::desktop::wallpaper::automatic::read_config(store)?;
    let mut widgets = WidgetPatch::new();
    if config.appearance.corner_radius != step.widget() {
        widgets.set(
            WIDGET_POINTER,
            widget::get(&raw, WIDGET_POINTER),
            Some(step.widget().into()),
        )?;
    }
    Ok((patch, widgets))
}

/// Applies `step` to every surface as one change, so one Undo and one
/// Changes entry cover it. `Ok(None)` when everything already matched.
pub fn apply(store: &DesktopStore, step: Step) -> Result<Option<LatestChange>, String> {
    let (patch, widgets) = patches(store, step)?;
    if patch.is_empty() && widgets.is_empty() {
        return Ok(None);
    }
    crate::desktop::actions::in_action(LABEL, || {
        store.apply_owned(&patch, Some(&widgets), Owner::Appearance, LABEL)
    })
    .map(Some)
    .map_err(|error| error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn earlier_choices_read_as_the_step_they_were_called() {
        // Widget corners: Square / Rounded / Very round were 0 / 8 / 16.
        for (radius, step) in [(0, Step::Square), (8, Step::Rounded), (16, Step::VeryRound)] {
            assert_eq!(Step::of_radius(radius), step);
            assert_eq!(Step::of_radius(u32::from(step.widget())), step);
        }
        // Dock and panel corners were 0 / 12 / 24 / pill.
        for (radius, step) in [
            (0, Step::Square),
            (12, Step::Rounded),
            (24, Step::VeryRound),
            (MAX_RADIUS, Step::Pill),
        ] {
            assert_eq!(Step::of_radius(radius), step);
            assert_eq!(step.bar(), radius);
        }
        // Window corners kept their three names.
        assert_eq!(Step::Square.window(), Roundness::Square);
        assert_eq!(Step::Rounded.window(), Roundness::SlightlyRound);
        assert_eq!(Step::VeryRound.window(), Roundness::Round);
        // Every step's own values read as that step everywhere.
        for step in Step::ALL {
            assert_eq!(
                Step::shown(
                    step.widget(),
                    &[step.bar(), step.bar()],
                    Some(step.window())
                ),
                Some(step),
                "{step:?}"
            );
        }
    }

    #[test]
    fn styles_read_as_one_step_and_differing_surfaces_as_none() {
        // Each built-in style's own widget and bar corners agree.
        for family in crate::desktop_theme::Family::ALL {
            let (_, widget, _) = family.geometry();
            let (_, bar, _, _) = family.bar();
            assert!(
                Step::shown(widget, &[bar, bar], None).is_some(),
                "{family:?}: widgets {widget}, bars {bar}"
            );
        }
        // Square widgets with round windows are not one step.
        assert_eq!(Step::shown(0, &[0], Some(Roundness::Round)), None);
        // A bar no one can read leaves the others to decide.
        assert_eq!(
            Step::shown(8, &[], Some(Roundness::SlightlyRound)),
            Some(Step::Rounded)
        );
        // Odd sizes read as the step whose range they fall in.
        assert_eq!(Step::of_radius(20), Step::VeryRound);
        assert_eq!(Step::of_radius(2), Step::Square);
        assert_eq!(Step::of_radius(40), Step::Pill);
    }
}
