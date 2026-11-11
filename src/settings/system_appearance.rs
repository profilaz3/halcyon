//! System appearance editor state. One draft covers both theme variants,
//! the mode, toolkit and compositor options; it survives navigation and is
//! written only by Apply, through the shared `DesktopStore`.
use crate::desktop::DesktopStore;
use crate::desktop::appearance::catalog::{FontCatalog, IconCatalog};
use crate::desktop::appearance::{
    self, AppearanceDraft, Blocked, ColorRole, Control, FontRole, Surface, SystemMode,
    ThemeVariant, Value, WindowEffect,
};
use crate::desktop::panels::Setting;
use crate::desktop::store::{ConfigKey, DesktopError, Journal, LatestChange, Owner};
use crate::settings::desktop::Notice;
use cosmic::cosmic_theme::palette::Srgba;
use cosmic::cosmic_theme::{Density, Roundness};
use cosmic::iced::font::{Style, Weight};
use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;

/// The notice after Use as accent on the Wallpaper page.
pub const ACCENT_NOTICE: &str = "Accent from your wallpaper — review, then Apply";

/// The searchable list that is open, if any.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Picker {
    Font(FontRole),
    Icons,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Message {
    /// Which variant the controls and preview show. Never changes the mode.
    Variant(ThemeVariant),
    AdvancedColors(bool),
    Mode(SystemMode),
    /// A swatch, or `None` for the automatic (derived) color.
    Color(ColorRole, Option<Srgba>),
    HexInput(ColorRole, String),
    HexSubmit(ColorRole),
    OutlineWidth(u32),
    Roundness(Roundness),
    Density(Density),
    Frosted(Surface, bool),
    Blur(u8),
    Glass(u8),
    Effects(bool),
    Window(WindowEffect, bool),
    Picker(Option<Picker>),
    Search(String),
    FontFamily(FontRole, String),
    FontWeight(FontRole, Weight),
    FontStyle(FontRole, Style),
    IconTheme(String),
    /// Replace the unsaved accent with the wallpaper's (see
    /// `suggest_accent`).
    ReplaceAccent,
    /// Keep the unsaved accent; the wallpaper's is dropped.
    KeepAccent,
    Apply,
    Discard,
    Undo,
    Refresh,
    UseCurrent,
    /// Keeps the edits over a value changed outside this editor.
    KeepMine,
    Recover,
    KeepRecovery,
}

impl Message {
    /// Messages that write desktop settings or the change journal.
    pub fn writes(&self) -> bool {
        matches!(
            self,
            Self::Apply | Self::Undo | Self::Recover | Self::KeepRecovery
        )
    }
}

pub fn search_id() -> cosmic::widget::Id {
    cosmic::widget::Id::new("appearance-search")
}
/// The open picker: its search field and list.
pub fn picker_id() -> cosmic::widget::Id {
    cosmic::widget::Id::new("appearance-picker")
}

/// Scrolls `content` just enough to show the open picker, its search field
/// first, so a picker opened near the bottom edge is not focused unseen.
pub fn reveal_picker<T: 'static>(content: cosmic::widget::Id) -> cosmic::Task<T> {
    use cosmic::iced::runtime::{Action, task};
    task::effect(Action::widget(Reveal {
        content,
        view: None,
        picker: None,
    }))
}

struct Reveal {
    content: cosmic::widget::Id,
    /// The scroll area's bounds and current offset.
    view: Option<(cosmic::iced::Rectangle, f32)>,
    picker: Option<cosmic::iced::Rectangle>,
}

impl cosmic::iced::advanced::widget::Operation for Reveal {
    fn traverse(
        &mut self,
        operate: &mut dyn FnMut(&mut dyn cosmic::iced::advanced::widget::Operation),
    ) {
        operate(self);
    }
    fn scrollable(
        &mut self,
        id: Option<&cosmic::widget::Id>,
        bounds: cosmic::iced::Rectangle,
        _content: cosmic::iced::Rectangle,
        translation: cosmic::iced::Vector,
        _state: &mut dyn cosmic::iced::advanced::widget::operation::Scrollable,
    ) {
        if id == Some(&self.content) {
            self.view = Some((bounds, translation.y));
        }
    }
    fn container(&mut self, id: Option<&cosmic::widget::Id>, bounds: cosmic::iced::Rectangle) {
        if id == Some(&picker_id()) {
            self.picker = Some(bounds);
        }
    }
    fn finish(&self) -> cosmic::iced::advanced::widget::operation::Outcome<()> {
        use cosmic::iced::advanced::widget::operation::{Outcome, scrollable};
        let (Some((view, offset)), Some(picker)) = (self.view, self.picker) else {
            return Outcome::None;
        };
        // Content coordinates: the picker's layout is not scrolled.
        let top = picker.y - view.y - 12.;
        let bottom = picker.y + picker.height - view.y + 12.;
        let wanted = if bottom > offset + view.height {
            (bottom - view.height).min(top)
        } else if top < offset {
            top
        } else {
            return Outcome::None;
        };
        Outcome::Chain(Box::new(scrollable::scroll_to(
            self.content.clone(),
            scrollable::AbsoluteOffset {
                x: None,
                y: Some(wanted.max(0.)),
            },
        )))
    }
}

#[derive(serde::Serialize, serde::Deserialize)]
pub(super) struct SavedAppearance {
    pub(super) draft: AppearanceDraft,
    pub(super) variant: ThemeVariant,
    pub(super) hex: BTreeMap<(ThemeVariant, ColorRole), String>,
}

#[derive(Default)]
pub struct AppearanceEditor {
    store: Option<Result<DesktopStore, String>>,
    draft: Option<Result<AppearanceDraft, String>>,
    pub variant: ThemeVariant,
    pub fonts: FontCatalog,
    pub icons: IconCatalog,
    /// The persistent latest change, when this page made it.
    undo: Option<LatestChange>,
    /// The owner and label of a latest change another page made; its Undo
    /// lives on that page.
    elsewhere: Option<(Owner, String)>,
    pub notice: Option<Notice>,
    recovery: Option<Result<Journal, String>>,
    pub effects: bool,
    pub advanced_colors: bool,
    pub picker: Option<Picker>,
    pub search: String,
    /// Unsubmitted text, per variant and role, independent of applied values.
    hex: BTreeMap<(ThemeVariant, ColorRole), String>,
    /// Typed text that was submitted or left unapplied while not a color.
    hex_error: BTreeSet<(ThemeVariant, ColorRole)>,
    /// A wallpaper accent waiting for Replace accent / Cancel, because an
    /// accent edit (chosen or typed) is not applied yet.
    pub accent_choice: Option<Srgba>,
}

impl AppearanceEditor {
    /// First visit: choose the store, read values and discover fonts and
    /// icon themes. Later visits refresh untouched values. Reads only.
    pub fn open(&mut self) {
        if self.store.is_some() {
            self.refresh();
            return;
        }
        let store = DesktopStore::from_env().map_err(|error| error.to_string());
        if let Ok(store) = &store {
            let fixture = store.fixture_root().map(std::path::Path::to_path_buf);
            self.fonts = match fixture.as_ref().map(|root| root.join("fonts")) {
                Some(file) if file.is_file() => {
                    FontCatalog::from_manifest(&std::fs::read_to_string(file).unwrap_or_default())
                }
                Some(_) => FontCatalog::default(),
                None => FontCatalog::installed(),
            };
            self.icons = match fixture {
                Some(root) => {
                    IconCatalog::discover(&[root.join("icons")], &appearance::catalog::languages())
                }
                None => IconCatalog::installed(),
            };
        }
        self.store = Some(store);
        self.reload(true);
        if let Some(Ok(draft)) = &self.draft {
            self.variant = draft.active_variant();
        }
    }
    pub fn unfinished_variant(&self, variant: ThemeVariant) -> bool {
        self.hex
            .iter()
            .any(|(slot, value)| slot.0 == variant && Self::parse_typed(slot, value).is_none())
    }

    fn unfinished_color(&self) -> bool {
        self.hex
            .iter()
            .any(|(slot, value)| Self::parse_typed(slot, value).is_none())
    }
    pub(super) fn session_draft(&self) -> Option<SavedAppearance> {
        self.dirty()
            .then(|| {
                self.draft().cloned().map(|draft| SavedAppearance {
                    draft,
                    variant: self.variant,
                    hex: self.hex.clone(),
                })
            })
            .flatten()
    }
    pub(super) fn restore_session(&mut self, saved: SavedAppearance) -> Result<(), String> {
        if saved.hex.len() > 10 || saved.hex.values().any(|text| text.len() > 36) {
            return Err("The saved appearance draft has invalid color text".into());
        }
        let mut draft = saved.draft;
        let store = self.store().ok_or("Desktop appearance is unavailable")?;
        let fresh = AppearanceDraft::load(store).map_err(|error| error.to_string())?;
        draft.refresh(fresh.into_snapshot());
        self.variant = saved.variant;
        self.hex = saved.hex;
        self.draft = Some(Ok(draft));
        self.settle_hex(false, true);
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
        match (&self.store, &self.draft) {
            (Some(Err(error)), _) | (_, Some(Err(error))) => Some(error),
            _ => None,
        }
    }
    pub fn writable(&self) -> bool {
        self.store().is_some_and(DesktopStore::writable)
    }
    pub fn draft(&self) -> Option<&AppearanceDraft> {
        self.draft.as_ref().and_then(|draft| draft.as_ref().ok())
    }
    pub fn dirty(&self) -> bool {
        self.draft().is_some_and(AppearanceDraft::dirty) || self.unfinished_color()
    }
    pub fn blocked(&self) -> Vec<Blocked> {
        self.draft()
            .map(AppearanceDraft::blocked)
            .unwrap_or_default()
    }
    /// Undo is offered for this page's own latest change only, and never
    /// while an interrupted change waits to be restored or kept.
    #[cfg(test)]
    pub fn can_undo(&self) -> bool {
        self.undo.is_some() && self.recovery.is_none() && !self.dirty() && self.writable()
    }
    /// Apply needs a writable store, edits, no pending recovery and no edit
    /// that would have to rewrite a value this version cannot represent.
    pub fn can_apply(&self) -> bool {
        self.draft().is_some_and(AppearanceDraft::dirty)
            && self.writable()
            && self.recovery.is_none()
            && self.blocked().is_empty()
    }
    /// Typed text for `role` of the shown variant, if any.
    pub fn hex_text(&self, role: ColorRole) -> Option<&str> {
        self.hex.get(&(self.variant, role)).map(String::as_str)
    }
    /// Whether the shown variant's `role` holds text that is not a color and
    /// should say so.
    pub fn hex_error(&self, role: ColorRole) -> bool {
        self.hex_error.contains(&(self.variant, role))
    }
    fn parse_typed(&(_, role): &(ThemeVariant, ColorRole), text: &str) -> Option<Srgba> {
        appearance::parse_hex(text, role.alpha())
    }
    /// Unsubmitted text belongs to its field, including a valid-looking prefix.
    /// Other actions and refreshes never turn it into a command or discard it.
    fn settle_hex(&mut self, _applied: bool, flag: bool) {
        if flag {
            let invalid: Vec<_> = self
                .hex
                .iter()
                .filter(|(slot, text)| Self::parse_typed(slot, text).is_none())
                .map(|(slot, _)| *slot)
                .collect();
            self.hex_error.extend(invalid);
        }
        let hex = &self.hex;
        self.hex_error.retain(|slot| hex.contains_key(slot));
    }
    pub fn recovery(&self) -> Option<&Result<Journal, String>> {
        self.recovery.as_ref()
    }
    pub fn state_key(&self) -> String {
        format!(
            "{:?}:{:?}:{:?}:{:?}:{}:{:?}:{}:{:?}:{}:{:?}",
            self.variant,
            self.draft().map(|draft| draft.edits().collect::<Vec<_>>()),
            self.draft().map(AppearanceDraft::conflicts),
            self.notice,
            self.effects,
            self.picker,
            self.search,
            self.hex,
            self.recovery.is_some(),
            self.accent_choice,
        )
    }
    #[cfg(test)]
    pub fn with_store(store: DesktopStore) -> Self {
        let mut editor = Self {
            store: Some(Ok(store)),
            ..Default::default()
        };
        editor.reload(true);
        editor
    }

    /// Use as accent (Wallpaper page): sets the accent of both variants in
    /// the draft, opening the editor if needed; never applies. Other edits
    /// stay. If either accent already has an unapplied value or typed text,
    /// the color waits for Replace accent / Cancel instead.
    #[cfg_attr(not(test), allow(dead_code))] // Legacy route retained for draft compatibility coverage.
    pub fn suggest_accent(&mut self, color: Srgba) {
        if !self.opened() {
            self.open();
        }
        if self.draft().is_none() {
            return;
        }
        if self.accent_pending() {
            self.accent_choice = Some(color);
        } else {
            self.replace_accent(color);
        }
    }
    /// Either variant's accent has an unapplied value or typed text.
    pub fn accent_pending(&self) -> bool {
        [ThemeVariant::Light, ThemeVariant::Dark]
            .into_iter()
            .any(|variant| {
                self.hex.contains_key(&(variant, ColorRole::Accent))
                    || self.draft().is_some_and(|draft| {
                        draft.edited(Control::Color(variant, ColorRole::Accent))
                    })
            })
    }
    /// An explicit choice on Wallpaper & colors replaces only legacy accent
    /// edits. Outline/surface edits and unfinished input for other roles stay.
    pub fn clear_pending_accent(&mut self) {
        self.hex.retain(|(_, role), _| *role != ColorRole::Accent);
        self.hex_error
            .retain(|(_, role)| *role != ColorRole::Accent);
        if let Some(Ok(draft)) = &mut self.draft {
            draft.clear_accent_edits();
        }
        self.accent_choice = None;
        self.notice = None;
    }
    fn replace_accent(&mut self, color: Srgba) {
        self.accent_choice = None;
        for variant in [ThemeVariant::Light, ThemeVariant::Dark] {
            let slot = (variant, ColorRole::Accent);
            self.hex.remove(&slot);
            self.hex_error.remove(&slot);
            self.edit(
                Control::Color(variant, ColorRole::Accent),
                Value::Color(Some(color)),
            );
        }
        self.notice = Some(Notice::Done(ACCENT_NOTICE.into()));
    }
    /// Existing user directories to watch for outside edits.
    pub fn watch_paths(&self) -> Vec<PathBuf> {
        let Some(store) = self.store() else {
            return Vec::new();
        };
        let mut paths: Vec<PathBuf> = Vec::new();
        for key in AppearanceDraft::keys() {
            let path = store.user_path(&key);
            if path.parent().is_some_and(std::path::Path::is_dir) && !paths.contains(&path) {
                paths.push(path);
            }
        }
        paths
    }

    fn load(&self) -> Result<AppearanceDraft, DesktopError> {
        let store = self
            .store()
            .ok_or_else(|| DesktopError::Io("Desktop settings unavailable".into()))?;
        AppearanceDraft::load(store)
    }
    fn refresh_recovery(&mut self) {
        if let Some(store) = self.store() {
            let recovery = store
                .pending_recovery()
                .map_err(|error| error.to_string())
                .transpose();
            // A record another page or a profile made, or one that cannot
            // be used, is not this page's Undo.
            let latest = store.latest_change().ok().flatten();
            self.elsewhere = latest
                .as_ref()
                .filter(|latest| latest.owner != Owner::Appearance)
                .map(|latest| (latest.owner, latest.label.clone()));
            let undo = latest.filter(|latest| latest.owner == Owner::Appearance);
            self.recovery = recovery;
            self.undo = undo;
        }
    }
    fn reload(&mut self, rebase: bool) {
        if self.store().is_none() {
            return;
        }
        self.refresh_recovery();
        match self.load() {
            Ok(fresh) => match &mut self.draft {
                Some(Ok(draft)) if !rebase => draft.refresh(fresh.into_snapshot()),
                _ => self.draft = Some(Ok(fresh)),
            },
            Err(error) => {
                if self.draft().is_some() {
                    self.notice = Some(Notice::failed(&error));
                } else {
                    self.draft = Some(Err(error.to_string()));
                }
            }
        }
    }
    pub fn refresh(&mut self) {
        self.reload(false);
    }

    fn edit(&mut self, control: Control, value: Value) {
        if let Some(Ok(draft)) = &mut self.draft {
            draft.set(control, value);
            self.notice = None;
        }
    }

    /// Applies `message`; returns the input that should take keyboard focus.
    pub fn update(&mut self, message: Message) -> Option<cosmic::widget::Id> {
        let variant = self.variant;
        // After these the queued wallpaper accent waits only while an
        // unapplied accent is still there to replace.
        let settles = matches!(
            message,
            Message::Apply
                | Message::Undo
                | Message::Refresh
                | Message::UseCurrent
                | Message::KeepMine
                | Message::Recover
                | Message::KeepRecovery
        );
        let focus = self.update_message(variant, message);
        if settles && !self.accent_pending() {
            self.accent_choice = None;
        }
        focus
    }
    fn update_message(
        &mut self,
        variant: ThemeVariant,
        message: Message,
    ) -> Option<cosmic::widget::Id> {
        match message {
            Message::Variant(variant) => {
                // Typed colors are already in the draft; other text is
                // kept for the variant it belongs to.
                self.settle_hex(false, true);
                self.variant = variant;
            }
            Message::Mode(mode) => self.edit(Control::Mode, Value::Mode(mode)),
            Message::AdvancedColors(open) => self.advanced_colors = open,
            Message::Color(role, color) => {
                self.hex.remove(&(variant, role));
                self.hex_error.remove(&(variant, role));
                self.edit(Control::Color(variant, role), Value::Color(color));
            }
            Message::HexInput(role, text) => {
                let slot = (variant, role);
                let text: String = text.chars().filter(|c| !c.is_control()).take(9).collect();
                self.hex_error.remove(&slot);
                self.hex.insert(slot, text);
            }
            Message::HexSubmit(role) => {
                let slot = (variant, role);
                if let Some(text) = self.hex.get(&slot) {
                    if let Some(color) = Self::parse_typed(&slot, text) {
                        self.edit(Control::Color(variant, role), Value::Color(Some(color)));
                        self.hex.remove(&slot);
                    } else {
                        self.hex_error.insert(slot);
                    }
                }
            }
            Message::OutlineWidth(width) => self.edit(Control::OutlineWidth, Value::Number(width)),
            Message::Roundness(roundness) => {
                self.edit(Control::Roundness, Value::Roundness(roundness));
            }
            Message::Density(density) => self.edit(Control::Density, Value::Density(density)),
            Message::Frosted(surface, on) => self.edit(Control::Frosted(surface), Value::Flag(on)),
            Message::Blur(step) => {
                if let Some(strength) = appearance::blur_from_step(step) {
                    self.edit(Control::Blur, Value::Blur(strength));
                }
            }
            Message::Glass(percent) => self.edit(Control::Glass, Value::Number(percent.into())),
            Message::Effects(open) => self.effects = open,
            Message::Window(effect, on) => self.edit(Control::Window(effect), Value::Flag(on)),
            Message::Picker(picker) => {
                self.picker = picker;
                self.search.clear();
                return picker.map(|_| search_id());
            }
            Message::Search(text) => {
                self.search = text.chars().filter(|c| !c.is_control()).take(80).collect();
            }
            Message::FontFamily(role, family) => {
                if let Some(Setting::Known(mut font)) = self.draft().map(|draft| draft.font(role)) {
                    let available = self.fonts.family(role.monospace(), &family).cloned();
                    font.family = available
                        .as_ref()
                        .map_or(family.clone(), |known| known.name.clone());
                    // Keep the weight and style when the new family has them.
                    if let Some(known) = &available {
                        if !known.weights.contains(&font.weight) {
                            font.weight = if known.weights.contains(&Weight::Normal) {
                                Weight::Normal
                            } else {
                                known.weights[0]
                            };
                        }
                        if font.style != Style::Normal && !known.italic {
                            font.style = Style::Normal;
                        }
                    }
                    self.edit(Control::Font(role), Value::Font(font));
                    self.picker = None;
                    self.search.clear();
                }
            }
            Message::FontWeight(role, weight) => {
                if let Some(Setting::Known(mut font)) = self.draft().map(|draft| draft.font(role)) {
                    font.weight = weight;
                    self.edit(Control::Font(role), Value::Font(font));
                }
            }
            Message::FontStyle(role, style) => {
                if let Some(Setting::Known(mut font)) = self.draft().map(|draft| draft.font(role)) {
                    font.style = style;
                    self.edit(Control::Font(role), Value::Font(font));
                }
            }
            Message::IconTheme(id) => {
                self.edit(Control::IconTheme, Value::Name(id));
                self.picker = None;
                self.search.clear();
            }
            Message::ReplaceAccent => {
                if let Some(color) = self.accent_choice.take() {
                    self.replace_accent(color);
                }
            }
            Message::KeepAccent => self.accent_choice = None,
            Message::Apply => self.apply(),
            Message::Discard => {
                self.accent_choice = None;
                if let Some(Ok(draft)) = &mut self.draft {
                    draft.discard();
                }
                self.hex.clear();
                self.hex_error.clear();
                self.notice = None;
                self.reload(true);
            }
            Message::Undo => self.undo(),
            Message::Refresh => {
                self.refresh();
                self.settle_hex(false, false);
            }
            Message::UseCurrent => {
                self.use_current(&[]);
                self.settle_hex(false, false);
            }
            Message::KeepMine => {
                match self.load() {
                    Ok(fresh) => {
                        if let Some(Ok(draft)) = &mut self.draft {
                            draft.keep_edits(fresh.into_snapshot());
                        }
                        self.notice = Some(Notice::Done(super::drafts::KEEPING.into()));
                    }
                    Err(error) => self.notice = Some(Notice::failed(&error)),
                }
                self.settle_hex(false, false);
            }
            Message::Recover => self.recover(true),
            Message::KeepRecovery => self.recover(false),
        }
        None
    }

    fn use_current(&mut self, keys: &[ConfigKey]) {
        match self.load() {
            Ok(fresh) => {
                if let Some(Ok(draft)) = &mut self.draft {
                    draft.resolve_conflicts(fresh.into_snapshot(), keys);
                }
                if keys.is_empty() {
                    self.notice = Some(Notice::Done("Using the current values".into()));
                }
            }
            Err(error) => self.notice = Some(Notice::failed(&error)),
        }
    }

    fn apply(&mut self) {
        self.reload(false);
        if !self.can_apply() {
            return;
        }
        let (Some(store), Some(draft)) = (self.store(), self.draft()) else {
            return;
        };
        if !draft.blocked().is_empty() {
            return;
        }
        let patch = draft.patch();
        if patch.is_empty() {
            return;
        }
        let explicit_colors = draft
            .edits()
            .any(|(control, _)| matches!(control, Control::Color(_, ColorRole::Accent)));
        match store.apply_owned(&patch, None, Owner::Appearance, "Appearance") {
            Ok(change) => {
                if explicit_colors
                    && let Err(error) =
                        crate::desktop::wallpaper::automatic::keep_manual_colors(store)
                {
                    eprintln!("Keeping manual wallpaper colors: {error}");
                }
                self.undo = Some(change);
                self.reload(true);
                self.settle_hex(true, true);
                self.notice = Some(Notice::Done("Applied to your desktop".into()));
            }
            Err(DesktopError::Conflict(keys)) => {
                self.reload(false);
                self.settle_hex(false, true);
                self.notice = Some(Notice::Error(format!(
                    "{} changed outside this editor. Review the conflict; your edits are kept.",
                    describe(&keys)
                )));
            }
            Err(error) => {
                if error == DesktopError::RecoveryPending {
                    self.reload(false);
                } else {
                    self.refresh_recovery();
                }
                self.notice = Some(Notice::failed(&error));
            }
        }
    }

    fn undo(&mut self) {
        let (Some(store), Some(change)) = (self.store(), self.undo.as_ref()) else {
            return;
        };
        if self.dirty() {
            return;
        }
        match store.undo_latest(&change.id) {
            Ok(_) => {
                self.undo = None;
                self.reload(true);
                // Typed colors the restored values replaced give way;
                // unfinished text stays, unflagged.
                self.settle_hex(false, false);
                self.notice = Some(Notice::Done("Previous settings restored".into()));
            }
            Err(DesktopError::Conflict(keys)) => {
                self.notice = Some(Notice::Error(format!(
                    "Can’t undo: {} changed since. Nothing was changed.",
                    describe(&keys)
                )));
            }
            Err(error) => {
                if error == DesktopError::RecoveryPending {
                    self.reload(false);
                } else {
                    self.refresh_recovery();
                }
                self.notice = Some(Notice::failed(&error));
            }
        }
    }

    fn recover(&mut self, restore: bool) {
        let Some(store) = self.store() else {
            return;
        };
        let result = if restore {
            store
                .recover()
                .map(|report| super::recovery::summary(&report))
        } else {
            store
                .discard_recovery()
                .map(|()| "Current settings kept".to_string())
        };
        match result {
            Ok(message) => {
                self.undo = None;
                self.reload(false);
                self.settle_hex(false, false);
                self.notice = Some(Notice::Done(message));
            }
            Err(error) => self.notice = Some(Notice::failed(&error)),
        }
    }
}

/// “Dark accent, icon theme” for a list of keys; panel keys use the Dock &
/// Panel names, so a shared journal reads the same everywhere.
pub fn describe(keys: &[ConfigKey]) -> String {
    let mut parts: Vec<String> = Vec::new();
    for key in keys {
        let part = if key
            .namespace()
            .starts_with(crate::desktop::panels::NAMESPACE)
        {
            crate::settings::desktop::describe(std::slice::from_ref(key))
        } else if let Some(label) = crate::desktop::windows::key_label(key)
            .or_else(|| crate::desktop::workspaces::key_label(key))
            .or_else(|| crate::desktop::window_rules::key_label(key))
            .or_else(|| crate::desktop::shortcuts::key_label(key))
            .or_else(|| crate::desktop::wallpaper::key_label(key))
            .or_else(|| crate::desktop::desktop_icons::key_label(key))
        {
            label
        } else {
            appearance::key_label(key)
        };
        if !parts.contains(&part) {
            parts.push(part);
        }
    }
    parts.join(", ")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::desktop::store::Fault;
    use crate::desktop::store::tests::Fixture;

    fn editor(fixture: &Fixture) -> AppearanceEditor {
        fixture.user("com.system76.CosmicTheme.Mode", "is_dark", "true");
        let mut editor = AppearanceEditor {
            store: Some(Ok(fixture.store())),
            ..Default::default()
        };
        editor.reload(true);
        editor
    }

    #[test]
    fn variant_selection_previews_without_changing_the_mode_and_drafts_persist() {
        let fixture = Fixture::new("appearance-editor");
        let mut editor = editor(&fixture);
        let before = fixture.tree();
        editor.update(Message::Variant(ThemeVariant::Light));
        assert!(!editor.dirty(), "previewing is not an edit");
        editor.update(Message::HexInput(ColorRole::Accent, "#zz".into()));
        editor.update(Message::HexSubmit(ColorRole::Accent));
        editor.update(Message::HexSubmit(ColorRole::Accent));
        assert!(editor.hex_error(ColorRole::Accent));
        assert!(
            editor.dirty(),
            "unfinished text is recoverable pending work"
        );
        assert!(!editor.can_apply());
        editor.update(Message::HexInput(ColorRole::Accent, "#E66100".into()));
        editor.update(Message::HexSubmit(ColorRole::Accent));
        editor.update(Message::HexSubmit(ColorRole::Accent));
        editor.update(Message::Variant(ThemeVariant::Dark));
        editor.update(Message::Color(
            ColorRole::Background,
            appearance::parse_hex("#202020", true),
        ));
        let draft = editor.draft().unwrap();
        assert!(draft.edited(Control::Color(ThemeVariant::Light, ColorRole::Accent)));
        assert!(draft.edited(Control::Color(ThemeVariant::Dark, ColorRole::Background)));
        assert!(
            draft
                .patch()
                .keys()
                .all(|key| key.namespace() != "com.system76.CosmicTheme.Mode")
        );
        assert_eq!(fixture.tree(), before, "nothing is written before Apply");
        // A refresh (watcher, or returning to the page) keeps both drafts.
        editor.update(Message::Refresh);
        assert_eq!(editor.draft().unwrap().edits().count(), 2);
        editor.update(Message::Apply);
        assert_eq!(
            editor.notice,
            Some(Notice::Done("Applied to your desktop".into()))
        );
        assert_eq!(
            fixture
                .read("com.system76.CosmicTheme.Mode", "is_dark")
                .as_deref(),
            Some("true")
        );
        assert!(editor.can_undo());
        editor.update(Message::Undo);
        let config = |fixture: &Fixture| {
            let mut tree = fixture.tree();
            tree.retain(|path, contents| {
                contents.is_some() && path.starts_with(fixture.0.join("config"))
            });
            tree
        };
        assert_eq!(config(&fixture), {
            let mut tree = before.clone();
            tree.retain(|path, contents| {
                contents.is_some() && path.starts_with(fixture.0.join("config"))
            });
            tree
        });
    }

    /// An outline color only COSMIC's derived theme holds is drawn, so the
    /// outline is not shown as following; "Follow accent" relinks it.
    #[test]
    fn follow_accent_relinks_an_outline_only_the_derived_theme_colors() {
        use ThemeVariant::Dark;
        let fixture = Fixture::new("appearance-editor-derived-outline");
        let path = fixture
            .0
            .join("config/cosmic/com.system76.CosmicTheme.Dark/v2/window_hint");
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(
            &path,
            "Some((red:0.3882353,green:0.8156863,blue:0.8745098))",
        )
        .unwrap();
        let mut editor = editor(&fixture);
        editor.update(Message::Variant(Dark));
        assert!(!editor.draft().unwrap().outline_follows(Dark));
        editor.update(Message::Color(ColorRole::Outline, None));
        assert!(editor.can_apply());
        editor.update(Message::Apply);
        assert!(!editor.dirty());
        let draft = editor.draft().unwrap();
        let preview = draft.preview(Dark);
        assert!(draft.outline_follows(Dark) && preview.outline == preview.accent);
    }

    /// Undo settles complete typed colors. Unfinished text must first be
    /// completed or discarded, just like the other editors' pending work.
    /// The Undo is this page's own persistent latest change.
    #[test]
    fn undo_settles_typed_colors_and_is_offered_only_for_this_page() {
        use ThemeVariant::Light;
        let fixture = Fixture::new("appearance-editor-undo-hex");
        let mut editor = editor(&fixture);
        editor.update(Message::Variant(Light));
        editor.update(Message::HexInput(ColorRole::Accent, "#E66100".into()));
        editor.update(Message::HexSubmit(ColorRole::Accent));
        editor.update(Message::Apply);
        assert!(editor.can_undo());
        // Typing the applied color again is no edit; Undo stays available.
        editor.update(Message::HexInput(ColorRole::Accent, "#E66100".into()));
        editor.update(Message::HexSubmit(ColorRole::Accent));
        editor.update(Message::HexInput(ColorRole::Outline, "#12".into()));
        editor.update(Message::HexSubmit(ColorRole::Outline));
        assert!(editor.dirty() && !editor.can_undo());
        let before = fixture.tree();
        editor.update(Message::Undo);
        assert_eq!(
            fixture.tree(),
            before,
            "unfinished input is not silently skipped"
        );
        // Choosing the derived outline cancels only its unfinished text.
        editor.update(Message::Color(ColorRole::Outline, None));
        assert!(!editor.dirty() && editor.can_undo());
        let mut restarted = AppearanceEditor {
            store: Some(Ok(fixture.store())),
            ..Default::default()
        };
        restarted.reload(true);
        assert!(restarted.can_undo(), "offered again after a restart");
        editor.update(Message::Undo);
        assert_eq!(
            editor.notice,
            Some(Notice::Done("Previous settings restored".into()))
        );
        assert_eq!(
            editor.hex_text(ColorRole::Accent),
            None,
            "stale color text settled"
        );
        assert_eq!(editor.hex_text(ColorRole::Outline), None);
        assert!(!editor.hex_error(ColorRole::Outline));
        assert!(!editor.can_undo());
    }

    /// A complete color updates the draft and preview as it is typed;
    /// incomplete text stays visible without touching the last valid color,
    /// and nothing typed is lost to a variant switch or Apply.
    #[test]
    fn typed_colors_preview_immediately_and_survive_variant_changes_and_apply() {
        use ThemeVariant::{Dark, Light};
        let fixture = Fixture::new("appearance-editor-typing");
        let mut editor = editor(&fixture);
        let before = fixture.tree();
        editor.update(Message::Variant(Light));
        let original = editor.draft().unwrap().color(Light, ColorRole::Accent);
        editor.update(Message::HexInput(ColorRole::Accent, "#e66100".into()));
        assert_eq!(
            editor.draft().unwrap().color(Light, ColorRole::Accent),
            original,
            "even complete text waits for Enter"
        );
        editor.update(Message::Variant(Dark));
        editor.update(Message::Refresh);
        editor.update(Message::Variant(Light));
        assert_eq!(editor.hex_text(ColorRole::Accent), Some("#e66100"));
        assert_eq!(fixture.tree(), before);
        editor.update(Message::HexSubmit(ColorRole::Accent));
        assert_eq!(
            editor.draft().unwrap().preview(Light).accent,
            appearance::parse_hex("#E66100", false).unwrap()
        );
        editor.update(Message::HexInput(ColorRole::Outline, "#12".into()));
        editor.update(Message::HexInput(ColorRole::Primary, "#123456".into()));
        assert!(
            editor.can_apply(),
            "unfinished fields do not block completed actions"
        );
        editor.update(Message::Apply);
        assert_eq!(editor.hex_text(ColorRole::Outline), Some("#12"));
        assert_eq!(
            editor.hex_text(ColorRole::Primary),
            Some("#123456"),
            "unsubmitted valid text is also retained"
        );
        assert_eq!(editor.hex_text(ColorRole::Accent), None);
        editor.update(Message::HexSubmit(ColorRole::Outline));
        assert!(editor.hex_error(ColorRole::Outline));
        editor.update(Message::HexSubmit(ColorRole::Primary));
        editor.update(Message::Apply);
        assert_eq!(
            editor.draft().unwrap().color(Light, ColorRole::Primary),
            Setting::Known(appearance::parse_hex("#123456", true))
        );
        assert_eq!(editor.hex_text(ColorRole::Outline), Some("#12"));
    }

    #[test]
    fn conflicts_offer_use_current_and_failed_rollback_offers_restore() {
        let fixture = Fixture::new("appearance-editor-conflict");
        let mut editor = editor(&fixture);
        editor.update(Message::IconTheme("Pop".into()));
        editor.update(Message::Density(Density::Spacious));
        fixture.user("com.system76.CosmicTk", "icon_theme", "\"Papirus\"");
        editor.update(Message::Refresh);
        assert_eq!(editor.draft().unwrap().conflicts().len(), 1);
        let before = fixture.tree();
        editor.update(Message::UseCurrent);
        assert_eq!(fixture.tree(), before);
        let draft = editor.draft().unwrap();
        assert!(!draft.edited(Control::IconTheme));
        assert!(draft.edited(Control::Density));
        assert_eq!(draft.icon_theme(), Setting::Known("Papirus".into()));
        // A write that fails and cannot be rolled back keeps the draft and
        // offers Restore immediately; Apply waits for the decision.
        let tk = |name: &str| ConfigKey::new("com.system76.CosmicTk", 1, name).unwrap();
        editor
            .store()
            .unwrap()
            .inject(Some(Fault::FailWriteAndRollback {
                write: tk("interface_density"),
                rollback: tk("header_size"),
            }));
        editor.update(Message::Apply);
        assert!(
            matches!(&editor.notice, Some(Notice::Error(text)) if text.contains("Use Restore"))
        );
        assert!(matches!(editor.recovery(), Some(Ok(_))));
        assert!(editor.dirty() && !editor.can_apply());
        editor.store().unwrap().inject(None);
        editor.update(Message::Recover);
        assert!(editor.recovery().is_none());
        assert!(
            fixture
                .read("com.system76.CosmicTk", "header_size")
                .is_none()
        );
        assert!(editor.can_apply());
    }

    /// A queued wallpaper accent waits while an unapplied accent edit
    /// remains (Refresh, a failed action) and goes once Apply settles it.
    #[test]
    fn queued_accent_choice_clears_once_no_accent_is_pending() {
        let fixture = Fixture::new("appearance-accent-choice");
        let mut editor = editor(&fixture);
        editor.update(Message::Color(
            ColorRole::Accent,
            appearance::parse_hex("#C64600", false),
        ));
        let wallpaper = Srgba::new(0.2, 0.4, 0.6, 1.0);
        editor.suggest_accent(wallpaper);
        assert_eq!(editor.accent_choice, Some(wallpaper));
        editor.update(Message::Refresh);
        assert_eq!(editor.accent_choice, Some(wallpaper), "still pending");
        editor.update(Message::Apply);
        assert!(!editor.dirty());
        assert_eq!(editor.accent_choice, None, "nothing unapplied to replace");
    }
}
