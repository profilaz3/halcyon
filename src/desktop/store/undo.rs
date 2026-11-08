//! The persistent latest change (`desktop-undo.json`).
//!
//! One record names the last owned transaction that fully succeeded: who
//! made it, a short label, every COSMIC key it wrote and its widget part.
//! It is replaced only inside a later successful transaction and cleared by
//! a successful Undo, so a failed Apply never loses the previous entry. It
//! is local state: profiles never read or export it, and a record that
//! fails validation is reported, never offered.
use super::widget::WidgetChange;
use super::{AppliedChange, ChangeEntry, DesktopError, JournalEntry, valid_name};
use serde::{Deserialize, Serialize};

pub(super) const FORMAT: &str = "desktop-cards-undo";
pub(super) const VERSION: u32 = 1;
const MAX_LABEL: usize = 120;
const MAX_ENTRIES: usize = crate::desktop::recovery::MAX_FILES;
pub(super) const HISTORY_LIMIT: usize = 24;

/// Provenance is durable and independent of the user-facing label.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Source {
    #[default]
    Manual,
    Derived,
    Scheduled,
}

/// Which part of the app made a change. A page offers Undo only for its own.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Owner {
    Recovery,
    /// The Dock & Panel page.
    DockPanel,
    Preferences,
    /// The System appearance page.
    Appearance,
    /// A profile Apply.
    Profile,
    /// The Windows page (window behavior and window gaps).
    Windows,
    /// The Workspaces page.
    Workspaces,
    /// The Floating apps page (tiling exceptions).
    WindowRules,
    /// The Shortcuts page (custom shortcuts and paused commands).
    Shortcuts,
    /// The Wallpaper page (backgrounds and per-display backgrounds).
    Wallpaper,
    /// The Desktop icons page.
    DesktopIcons,
}

impl Owner {
    pub fn key(self) -> &'static str {
        match self {
            Self::Recovery => "recovery",
            Self::DockPanel => "dock-panel",
            Self::Preferences => "preferences",
            Self::Appearance => "appearance",
            Self::Profile => "profile",
            Self::Windows => "windows",
            Self::Workspaces => "workspaces",
            Self::WindowRules => "window-rules",
            Self::Shortcuts => "shortcuts",
            Self::Wallpaper => "wallpaper",
            Self::DesktopIcons => "desktop-icons",
        }
    }
    pub const ALL: [Self; 11] = [
        Self::Recovery,
        Self::DockPanel,
        Self::Preferences,
        Self::Appearance,
        Self::Profile,
        Self::Windows,
        Self::Workspaces,
        Self::WindowRules,
        Self::Shortcuts,
        Self::Wallpaper,
        Self::DesktopIcons,
    ];
    pub fn parse(value: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|owner| owner.key() == value)
    }
}

/// A short user-facing label: 1–120 characters, no control characters.
pub fn valid_label(label: &str) -> bool {
    !label.trim().is_empty()
        && label.chars().count() <= MAX_LABEL
        && !label.chars().any(char::is_control)
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub(super) struct UndoFile {
    pub format: String,
    pub version: u32,
    pub id: String,
    pub owner: String,
    pub label: String,
    pub created: String,
    pub entries: Vec<JournalEntry>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub widget: Option<WidgetChange>,
    #[serde(default)]
    pub source: Source,
    /// Flat, oldest first. Legacy version-one records have an empty history.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub history: Vec<UndoFile>,
    #[serde(
        default,
        deserialize_with = "super::activity::deserialize",
        skip_serializing_if = "Vec::is_empty"
    )]
    pub recent: Vec<super::activity::Summary>,
    #[serde(default, deserialize_with = "super::activity::deserialize_partial")]
    pub recent_partial: bool,
    /// Derived transitions after this action, coalesced by touched field.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub derived: Vec<JournalEntry>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub derived_widget: Vec<super::WidgetFieldChange>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub transaction_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub manual_at: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub palette_ack: Option<(u64, u64)>,
    /// The Dock or Panel item this change added, removed or moved.
    #[serde(
        default,
        deserialize_with = "super::activity::deserialize_item",
        skip_serializing_if = "Option::is_none"
    )]
    pub item: Option<super::activity::ItemChange>,
}

/// The validated latest change, ready for Undo.
#[derive(Clone, Debug, PartialEq)]
pub struct LatestChange {
    pub id: String,
    pub owner: Owner,
    pub label: String,
    pub created: String,
    pub change: AppliedChange,
    pub widget: Option<WidgetChange>,
    /// The Dock or Panel item this change added, removed or moved.
    pub item: Option<super::activity::ItemChange>,
}

impl LatestChange {
    /// The record as text, for a caller that keeps its own changes (the setup
    /// wizard, across closing and reopening). Read back with `decode`.
    pub fn encode(&self) -> Result<String, DesktopError> {
        serde_json::to_string(&self.to_file())
            .map_err(|error| DesktopError::UndoUnavailable(error.to_string()))
    }
    /// A record from `encode`, validated like the store's own.
    pub fn decode(text: &str) -> Result<Self, DesktopError> {
        let file: UndoFile = serde_json::from_str(text)
            .map_err(|_| DesktopError::UndoUnavailable("The saved change is damaged.".into()))?;
        Self::from_file(file)
    }
    pub(super) fn to_file(&self) -> UndoFile {
        UndoFile {
            format: FORMAT.into(),
            version: if self.widget.as_ref().is_some_and(|w| w.design_v1) {
                2
            } else {
                VERSION
            },
            id: self.id.clone(),
            owner: self.owner.key().into(),
            label: self.label.clone(),
            created: self.created.clone(),
            entries: self.change.entries.iter().map(JournalEntry::from).collect(),
            widget: self.widget.clone(),
            source: Source::Manual,
            history: Vec::new(),
            recent: Vec::new(),
            recent_partial: false,
            derived: Vec::new(),
            derived_widget: Vec::new(),
            transaction_id: None,
            manual_at: Some(self.created.clone()),
            palette_ack: None,
            item: self.item.clone(),
        }
    }
    /// Validates a record read from disk. Anything unexpected, including a
    /// newer record version, is refused with an explanation.
    pub(super) fn from_file(file: UndoFile) -> Result<Self, DesktopError> {
        let invalid = |message: &str| DesktopError::UndoUnavailable(message.to_string());
        if file.format != FORMAT {
            return Err(invalid("The saved Undo record is not recognized."));
        }
        if !matches!(file.version, VERSION | 2)
            || (file.version == VERSION && file.widget.as_ref().is_some_and(|w| w.design_v1))
        {
            return Err(invalid(
                "The saved Undo record comes from a different version of Halcyon.",
            ));
        }
        if !valid_name(&file.id, 96) {
            return Err(invalid("The saved Undo record is damaged."));
        }
        let owner = Owner::parse(&file.owner)
            .ok_or_else(|| invalid("The saved Undo record has an unknown owner."))?;
        if !valid_label(&file.label) || file.created.len() > 64 {
            return Err(invalid("The saved Undo record is damaged."));
        }
        if file.entries.len() > MAX_ENTRIES {
            return Err(invalid("The saved Undo record is too large."));
        }
        let mut entries: Vec<ChangeEntry> = Vec::with_capacity(file.entries.len());
        for entry in &file.entries {
            let entry = ChangeEntry::try_from(entry)?;
            if entries.iter().any(|seen| seen.key == entry.key) {
                return Err(invalid("The saved Undo record is damaged."));
            }
            entries.push(entry);
        }
        if let Some(widget) = &file.widget {
            widget.validate().map_err(DesktopError::UndoUnavailable)?;
        }
        if entries.is_empty()
            && file.widget.is_none()
            && ((file.source == Source::Manual && owner != Owner::Recovery)
                || file.transaction_id.is_none())
        {
            return Err(invalid("The saved Undo record is empty."));
        }
        Ok(Self {
            id: file.id.clone(),
            owner,
            label: file.label,
            created: file.created,
            change: AppliedChange {
                id: file.id,
                entries,
            },
            widget: file.widget,
            // Optional naming never makes a record unusable for Undo.
            item: file.item.filter(super::activity::ItemChange::valid),
        })
    }
}

impl UndoFile {
    /// Advance only an action's own expected fields through an unbroken app
    /// transition. A changed external baseline cannot establish this chain.
    fn advance(&mut self, entries: &[JournalEntry], widget: Option<&WidgetChange>) {
        for entry in entries {
            let same = |old: &&JournalEntry| {
                old.namespace == entry.namespace
                    && old.version == entry.version
                    && old.key == entry.key
            };
            let Some(owned) = self.entries.iter().find(same) else {
                continue;
            };
            let index = self.derived.iter().position(|old| {
                old.namespace == entry.namespace
                    && old.version == entry.version
                    && old.key == entry.key
            });
            let expected = index.map_or(&owned.after, |index| &self.derived[index].after);
            let advanced = if expected == &entry.before {
                Some(entry.after.clone())
            } else {
                // Generated containers also contain styling changed by later
                // manual actions. Attribute the derived members independently.
                super::history::advance_native(entry, expected)
            };
            if let Some(after) = advanced {
                if let Some(index) = index {
                    self.derived[index].after = after;
                } else if &after != expected {
                    let mut transition = entry.clone();
                    transition.before = owned.after.clone();
                    transition.after = after;
                    self.derived.push(transition);
                }
            }
        }
        if let (Some(owned), Some(widget)) = (&self.widget, widget) {
            for field in &widget.fields {
                let Some(original) = owned.fields.iter().find(|old| old.pointer == field.pointer)
                else {
                    continue;
                };
                if let Some(old) = self
                    .derived_widget
                    .iter_mut()
                    .find(|old| old.pointer == field.pointer)
                {
                    if old.after == field.before {
                        old.after = field.after.clone();
                    }
                } else if original.after == field.before {
                    self.derived_widget.push(field.clone());
                }
            }
        }
    }
    pub(super) fn attribute_derived(
        &mut self,
        entries: &[JournalEntry],
        widget: Option<&WidgetChange>,
    ) {
        for file in &mut self.history {
            file.advance(entries, widget);
        }
        if self.source == Source::Manual {
            self.advance(entries, widget);
        }
    }
    pub(super) fn attribute_restoration(
        &mut self,
        removed: &Self,
        entries: &[ChangeEntry],
        widget: Option<&WidgetChange>,
    ) {
        let bridge: Vec<_> = entries
            .iter()
            .filter_map(|entry| {
                let original = removed.entries.iter().find(|old| {
                    old.namespace == entry.key.namespace()
                        && old.version == entry.key.version()
                        && old.key == entry.key.key()
                })?;
                let mut bridge = JournalEntry::from(entry);
                bridge.before = original.before.clone();
                Some(bridge)
            })
            .collect();
        let widgets = widget.and_then(|widget| {
            let removed = removed.widget.as_ref()?;
            let mut bridge = widget.clone();
            bridge.fields = widget
                .fields
                .iter()
                .filter_map(|field| {
                    let original = removed
                        .fields
                        .iter()
                        .find(|old| old.pointer == field.pointer)?;
                    let mut field = field.clone();
                    field.before = original.before.clone();
                    Some(field)
                })
                .collect();
            Some(bridge)
        });
        self.advance(&bridge, widgets.as_ref());
    }
    pub(super) fn validate_history(&self) -> Result<(), DesktopError> {
        if self.history.len() >= HISTORY_LIMIT || self.history.iter().any(|f| !f.history.is_empty())
        {
            return Err(DesktopError::UndoUnavailable(
                "The saved history is too large.".into(),
            ));
        }
        for file in self.history.iter().chain(std::iter::once(self)) {
            LatestChange::from_file(file.clone())?;
            if file.derived.len() > MAX_ENTRIES || file.derived_widget.len() > MAX_ENTRIES {
                return Err(DesktopError::UndoUnavailable(
                    "The saved history is too large.".into(),
                ));
            }
            for entry in &file.derived {
                ChangeEntry::try_from(entry)?;
            }
            for field in &file.derived_widget {
                if super::widget::field(&field.pointer).is_none() {
                    return Err(DesktopError::UndoUnavailable(
                        "Unknown history field".into(),
                    ));
                }
            }
        }
        Ok(())
    }
    pub(super) fn manual(&self) -> Option<&Self> {
        if self.source == Source::Manual {
            Some(self)
        } else {
            self.history
                .iter()
                .rev()
                .find(|file| file.source == Source::Manual)
        }
    }
    pub(super) fn followed_by(mut self, mut next: Self) -> Self {
        next.recent_partial = self.recent_partial || self.recent.is_empty();
        if self.recent.is_empty() {
            for file in self.history.iter().chain(std::iter::once(&self)) {
                if super::activity::has_changes(file) {
                    super::activity::append(
                        &mut next.recent,
                        super::activity::summarize(file, false),
                    );
                }
            }
        } else {
            next.recent = std::mem::take(&mut self.recent);
        }
        let mut history = std::mem::take(&mut self.history);
        // Only manual actions consume history slots. A scheduled action is
        // still the latest observation, but never the visible Undo action.
        if self.source == Source::Manual {
            history.push(self);
        }
        let keep = HISTORY_LIMIT - 1;
        if history.len() > keep {
            history.drain(..history.len() - keep);
        }
        next.history = history;
        // Bound history by bytes as well as count. The journal also retains
        // the previous record, so leave room for one full config transaction.
        while !next.history.is_empty()
            && serde_json::to_vec(&next).is_ok_and(|bytes| bytes.len() > 3 * 1024 * 1024)
        {
            next.history.remove(0);
        }
        next
    }
}
