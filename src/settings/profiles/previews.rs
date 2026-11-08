//! Themes, setups and adaptive looks share one planning engine. Only one review
//! is live; the others wait here with the plan they were last seen against, so
//! a newer desktop change is reconciled instead of silently accepted.
use super::*;

const MAX_STEPS: usize = 32;

/// Which screen a review belongs to.
#[derive(
    Clone,
    Copy,
    Debug,
    Default,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    serde::Serialize,
    serde::Deserialize,
)]
pub enum PreviewKind {
    Theme,
    #[default]
    Setup,
    Adaptive,
}
impl PreviewKind {
    pub fn title(self) -> &'static str {
        match self {
            Self::Theme => "Theme",
            Self::Setup => "Look",
            Self::Adaptive => "Adaptive look",
        }
    }
}

/// One earlier state of the live review.
#[derive(Clone)]
pub(super) struct PreviewStep {
    selection: Selection,
    excluded: BTreeSet<Domain>,
    theme_override: Option<Profile>,
    custom: Option<String>,
    mix: Option<mix::Draft>,
    label: &'static str,
    baseline: Option<profiles::PreviewBaseline>,
    source: Option<profiles::PreviewBaseline>,
    touched: bool,
    review_required: bool,
    color_options: colors::Options,
}

/// A review that is not on screen. Nothing here touches the desktop.
#[derive(Clone)]
pub struct ParkedPreview {
    pub kind: PreviewKind,
    pub title: String,
    selection: Selection,
    excluded: BTreeSet<Domain>,
    theme_override: Option<Profile>,
    custom: Option<String>,
    mix: Option<mix::Draft>,
    package: Option<packages::Draft>,
    baseline: Option<profiles::PreviewBaseline>,
    source: Option<profiles::PreviewBaseline>,
    steps: Vec<PreviewStep>,
    review_required: bool,
    color_options: colors::Options,
}
impl ParkedPreview {
    pub fn selection(&self) -> &Selection {
        &self.selection
    }
    pub fn conflicted(&self) -> bool {
        self.baseline
            .as_ref()
            .zip(self.source.as_ref())
            .is_some_and(|(baseline, source)| baseline.conflicts_with(source))
    }
}

/// Saved across restarts. Like every session draft, this is an intention only.
#[derive(serde::Serialize, serde::Deserialize)]
pub(in crate::settings) struct SavedPreview {
    kind: PreviewKind,
    title: String,
    selection: SavedSelection,
    #[serde(default)]
    custom: Option<String>,
    #[serde(default, with = "json_payload")]
    mix: Option<mix::Draft>,
    #[serde(default, with = "json_payload")]
    package: Option<packages::Draft>,
    #[serde(default)]
    baseline: Option<profiles::PreviewBaseline>,
    #[serde(default)]
    source: Option<profiles::PreviewBaseline>,
    #[serde(default)]
    color_options: colors::Options,
}

/// Profile records contain JSON-specific enum decoding. Keep them in their
/// native format inside the session's RON envelope, just like selection.theme.
mod json_payload {
    use serde::{Deserialize, Serialize, de::DeserializeOwned};
    pub fn serialize<T: Serialize, S: serde::Serializer>(
        value: &Option<T>,
        serializer: S,
    ) -> Result<S::Ok, S::Error> {
        value
            .as_ref()
            .map(serde_json::to_string)
            .transpose()
            .map_err(serde::ser::Error::custom)?
            .serialize(serializer)
    }
    pub fn deserialize<'de, T: DeserializeOwned, D: serde::Deserializer<'de>>(
        deserializer: D,
    ) -> Result<Option<T>, D::Error> {
        Option::<String>::deserialize(deserializer)?
            .as_deref()
            .map(serde_json::from_str)
            .transpose()
            .map_err(serde::de::Error::custom)
    }
}

impl ProfilesEditor {
    pub fn owner(&self) -> PreviewKind {
        self.owner
    }
    pub fn touched(&self) -> bool {
        self.touched
    }
    pub fn parked(&self) -> &[ParkedPreview] {
        &self.parked
    }
    pub fn conflicts(&self) -> &BTreeSet<Domain> {
        &self.conflicts
    }
    pub fn conflict_values(&self) -> &[profiles::PreviewConflict] {
        &self.conflict_values
    }
    pub fn can_undo_step(&self) -> bool {
        !self.steps.is_empty()
    }
    pub fn step_label(&self) -> Option<&'static str> {
        self.steps.last().map(|step| step.label)
    }
    /// Whether a review is open on screen, even one with no steps yet.
    pub fn review_open(&self) -> bool {
        self.selection != Selection::Current
            || self.profile().is_some()
            || self.mix.is_some()
            || self.has_custom_input()
    }
    fn has_custom_input(&self) -> bool {
        self.custom
            .as_deref()
            .is_some_and(|text| !text.trim().is_empty())
    }
    fn preview_title(&self) -> String {
        match self.owner {
            PreviewKind::Theme => "Theme preview".into(),
            kind => self
                .profile()
                .map(|profile| profile.name.clone())
                .filter(|name| !name.is_empty())
                .map_or_else(|| format!("{} preview", kind.title()), |name| name),
        }
    }
    /// Whether leaving should keep this review. Choosing a theme is the edit;
    /// opening a setup to look at it is not.
    pub fn keeps_intent(&self, kind: PreviewKind) -> bool {
        if !self.review_open() {
            return false;
        }
        match kind {
            PreviewKind::Theme => {
                self.has_custom_input()
                    || self.plan.as_ref().is_some_and(|plan| {
                        !plan.is_empty()
                            || !plan.blocking_reasons().is_empty()
                            || self.session_review
                    })
            }
            PreviewKind::Setup | PreviewKind::Adaptive => self.touched,
        }
    }
    /// Leaves a screen. Returns the title of a review that now waits.
    pub fn park(&mut self, kind: PreviewKind) -> Option<String> {
        let kept = self.keeps_intent(kind).then(|| ParkedPreview {
            kind,
            title: self.preview_title(),
            selection: self.selection.clone(),
            excluded: self.excluded.clone(),
            theme_override: self.profile().cloned(),
            custom: self.custom.clone(),
            mix: self.mix.clone(),
            package: self.package.clone(),
            baseline: self.preview_baseline.clone(),
            source: self.preview_source.clone(),
            steps: self.steps.clone(),
            review_required: self.session_review,
            color_options: self.color_options.clone(),
        });
        let title = kept.as_ref().map(|parked| parked.title.clone());
        if let Some(parked) = kept {
            self.parked.retain(|other| {
                !(other.kind == kind
                    && (kind == PreviewKind::Theme
                        || same_selection(&other.selection, &parked.selection)))
            });
            self.parked.push(parked);
        }
        self.leave_review();
        title
    }
    fn leave_review(&mut self) {
        self.select(Selection::Current);
        self.steps.clear();
        self.touched = false;
        self.conflicts.clear();
    }
    /// Enters a screen and brings its waiting review back, if it has one.
    pub fn enter(&mut self, kind: PreviewKind) -> bool {
        if self.review_open() {
            if self.owner == kind {
                return false;
            }
            self.park(self.owner);
        }
        self.owner = kind;
        let found = self.parked.iter().rposition(|parked| parked.kind == kind);
        found.is_some_and(|index| {
            let parked = self.parked.remove(index);
            self.resume(parked);
            true
        })
    }
    pub fn resume_kept_selection(&mut self, selection: &Selection) {
        if self.review_open()
            && (self.owner != PreviewKind::Setup || !same_selection(&self.selection, selection))
        {
            self.park(self.owner);
        }
        self.owner = PreviewKind::Setup;
        let _ = self.resume_selection(selection);
    }
    /// Opens the waiting review of exactly this selection.
    pub(super) fn resume_selection(&mut self, selection: &Selection) -> bool {
        let kind = self.owner;
        let found = self.parked.iter().rposition(|parked| {
            parked.kind == kind && same_selection(&parked.selection, selection)
        });
        found.is_some_and(|index| {
            let parked = self.parked.remove(index);
            self.resume(parked);
            true
        })
    }
    fn resume(&mut self, parked: ParkedPreview) {
        self.select(parked.selection);
        if parked.theme_override.is_some() {
            self.theme_override = parked.theme_override;
        }
        self.excluded = parked.excluded;
        self.custom = parked.custom;
        self.mix = parked.mix;
        self.package = parked.package;
        self.steps = parked.steps;
        self.color_options = parked.color_options;
        self.touched = true;
        self.preview_baseline = parked.baseline;
        self.preview_source = parked.source;
        self.session_review =
            parked.review_required || (self.preview_baseline.is_none() && self.profile().is_some());
        self.queue_theme_colors();
        self.replan_retained();
    }
    /// Older callers choose a whole domain. Resolve its conflicting values
    /// individually so unrelated intent within that domain is preserved.
    pub fn resolve_conflict(&mut self, domain: Domain, keep_preview: bool) {
        let ids = self
            .conflict_values
            .iter()
            .filter(|value| value.domain == domain)
            .map(|value| value.id.clone())
            .collect::<Vec<_>>();
        for id in ids {
            self.resolve_value_conflict(&id, keep_preview);
        }
    }
    pub fn resolve_value_conflict(&mut self, id: &str, keep_preview: bool) {
        if !self.conflict_values.iter().any(|value| value.id == id) {
            return;
        }
        if let (Some(baseline), Some(current)) = (&mut self.preview_baseline, &self.preview_source)
        {
            baseline.resolve(id, current, keep_preview);
        }
        self.replan_retained();
    }
    /// Records the review as it was before an edit, if the edit changed it.
    pub(super) fn record_step(&mut self, before: PreviewStep) {
        let same = before.selection == self.selection
            && before.excluded == self.excluded
            && before.theme_override == self.theme_override
            && before.custom == self.custom
            && before.mix == self.mix
            && before.color_options == self.color_options;
        if same {
            return;
        }
        self.steps.push(before);
        if self.steps.len() > MAX_STEPS {
            self.steps.remove(0);
        }
    }
    pub(super) fn step_state(&self, label: &'static str) -> PreviewStep {
        PreviewStep {
            selection: self.selection.clone(),
            excluded: self.excluded.clone(),
            theme_override: self.theme_override.clone(),
            custom: self.custom.clone(),
            mix: self.mix.clone(),
            label,
            baseline: self.preview_baseline.clone(),
            source: self.preview_source.clone(),
            touched: self.touched,
            review_required: self.session_review,
            color_options: self.color_options.clone(),
        }
    }
    /// Undoes one review edit. It never reaches desktop history; an empty
    /// history leaves the review exactly as it is.
    pub fn undo_step(&mut self) -> bool {
        let Some(step) = self.steps.pop() else {
            return false;
        };
        let steps = std::mem::take(&mut self.steps);
        self.select(step.selection);
        self.theme_override = step.theme_override;
        self.excluded = step.excluded;
        self.custom = step.custom;
        self.mix = step.mix;
        self.color_options = step.color_options;
        self.preview_baseline = step.baseline;
        self.preview_source = step.source;
        self.session_review = step.review_required;
        self.queue_theme_colors();
        self.replan_retained();
        self.steps = steps;
        self.touched = step.touched;
        true
    }
    /// Forgets what was kept for this review once it is applied or discarded.
    pub(super) fn forget_review(&mut self) {
        let kind = self.owner;
        let selection = self.selection.clone();
        self.parked.retain(|parked| {
            !(parked.kind == kind
                && (kind == PreviewKind::Theme || same_selection(&parked.selection, &selection)))
        });
        self.steps.clear();
        self.touched = false;
        self.conflicts.clear();
    }
    /// The review on screen and those waiting, for the next session.
    pub(in crate::settings) fn saved_previews(&self) -> Vec<SavedPreview> {
        let live = self.keeps_intent(self.owner).then(|| SavedPreview {
            kind: self.owner,
            title: self.preview_title(),
            selection: saved_selection(&self.selection, &self.excluded, self.profile()),
            custom: self.custom.clone(),
            mix: self.mix.clone(),
            package: self.package.clone(),
            baseline: self.preview_baseline.clone(),
            source: self.preview_source.clone(),
            color_options: self.color_options.clone(),
        });
        self.parked
            .iter()
            .map(|parked| SavedPreview {
                kind: parked.kind,
                title: parked.title.clone(),
                selection: saved_selection(
                    &parked.selection,
                    &parked.excluded,
                    parked.theme_override.as_ref(),
                ),
                custom: parked.custom.clone(),
                mix: parked.mix.clone(),
                package: parked.package.clone(),
                baseline: parked.baseline.clone(),
                source: parked.source.clone(),
                color_options: parked.color_options.clone(),
            })
            .chain(live)
            .collect()
    }
    /// Restore atomically: one unreadable item must not overwrite the rest of
    /// the recovery file with a partially restored list at the next checkpoint.
    pub(in crate::settings) fn restore_previews(
        &mut self,
        saved: Vec<SavedPreview>,
    ) -> Result<(), String> {
        let mut restored = Vec::new();
        for item in saved {
            let theme_override = item
                .selection
                .theme
                .as_deref()
                .map(|text| profiles::import_local(text.as_bytes()).map_err(|e| e.to_string()))
                .transpose()?;
            let selection = if item.selection.preset {
                match self.preset_for(&item.selection.id) {
                    Some(preset) => Selection::Preset(preset.id),
                    None if theme_override.is_some() => Selection::Current,
                    None => return Err("A saved preview's look is no longer available".into()),
                }
            } else if item.selection.id.is_empty() {
                Selection::Current
            } else {
                Selection::Saved(ProfileId::parse(&item.selection.id).map_err(|e| e.to_string())?)
            };
            let excluded = item
                .selection
                .excluded
                .iter()
                .map(|name| {
                    Domain::parse(name)
                        .ok_or_else(|| "A saved preview has an unsupported section".to_string())
                })
                .collect::<Result<_, _>>()?;
            if let Some(mix) = &item.mix {
                mix.validate()?;
            }
            if let Some(package) = &item.package {
                package.profile.validate().map_err(|e| e.to_string())?;
            }
            restored.push(ParkedPreview {
                kind: item.kind,
                title: item.title,
                selection,
                excluded,
                theme_override,
                custom: item.custom,
                mix: item.mix,
                package: item.package,
                baseline: item.baseline,
                source: item.source,
                steps: Vec::new(),
                review_required: true,
                color_options: item.color_options,
            });
        }
        self.parked.extend(restored);
        Ok(())
    }
}

fn saved_selection(
    selection: &Selection,
    excluded: &BTreeSet<Domain>,
    theme: Option<&Profile>,
) -> SavedSelection {
    let (preset, id) = match selection {
        Selection::Current => (false, String::new()),
        Selection::Preset(id) => (true, id.to_string()),
        Selection::Saved(id) => (false, id.as_str().to_string()),
    };
    SavedSelection {
        theme: theme.and_then(|profile| serde_json::to_string(profile).ok()),
        preset,
        id,
        excluded: excluded.iter().map(|domain| domain.key().into()).collect(),
    }
}

fn same_selection(left: &Selection, right: &Selection) -> bool {
    if left == right {
        return true;
    }
    match (left, right) {
        (Selection::Preset(left), Selection::Preset(right)) => profiles::preset(left)
            .zip(profiles::preset(right))
            .is_some_and(|(left, right)| left.id == right.id),
        _ => false,
    }
}

#[cfg(test)]
mod tests;
