//! Dock and panel semantic builders. Settings completes ordinary controls
//! through the shared action path; guarded snapshots handle outside edits.
pub mod islands;
use crate::desktop::DesktopStore;
use crate::desktop::applets::{AppletCatalog, AppletGroup};
use crate::desktop::panels::{
    self, Anchor, AppletDrag, AutoHide, Background, Output, PanelDraft, PanelKind, PanelSize,
    Region, Regions, Setting, Slot, Timing,
};
use crate::desktop::store::{ConfigKey, DesktopError, DesktopPatch, Journal, LatestChange, Owner};
use std::collections::BTreeMap;
use std::path::PathBuf;

#[derive(Clone, Debug, PartialEq)]
pub enum Message {
    Islands(islands::Edit),
    Select(PanelKind),
    Anchor(Anchor),
    Size(PanelSize),
    AutoHide(AutoHide),
    Timing(Timing, u32),
    Floating(bool),
    Margin(u16),
    Extend(bool),
    Opacity(u8),
    Penumbra(bool),
    PenumbraHeight(u32),
    PenumbraMaximizedOpacity(u8),
    Radius(u32),
    RadiusChoice(u32),
    Spacing(u32),
    Padding(u32),
    Output(Output),
    Background(Background),
    KeepStyle(bool),
    Advanced(bool),
    Picker(Option<Region>),
    PickerRegion(Region),
    AppletGroup(AppletGroup),
    SelectApplet(Option<Slot>),
    MoveTo(Slot, Region),
    Starter(bool),
    UseStarter,
    PanelStyle(PanelStyle),
    Add(Region, String),
    /// Filter the applet picker and the installed list.
    Search(String),
    /// Show or hide every installed applet with where it is used.
    Overview(bool),
    /// Add an installed applet to the selected bar's usual region.
    AddToBar(String),
    Remove(Slot),
    Step(Slot, i32),
    DragStart(Slot),
    DragOver(Slot),
    DragLeave(Slot),
    Drop,
    /// A release hit-tested by an actual visible drop zone, not stale hover.
    DropAt(Slot),
    DragCancel,
    Apply,
    Discard,
    Undo,
    /// Re-read live values; edits and their conflict baselines are kept.
    Refresh,
    /// Drop the edits that changed outside this editor and adopt the current
    /// values; every other edit stays.
    UseCurrent,
    Recover,
    KeepRecovery,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PanelStyle {
    FullWidth,
    Islands,
}

impl Message {
    /// Messages that write desktop settings or the change journal. Capture
    /// commands may send these only to an explicit fixture root.
    pub fn writes(&self) -> bool {
        matches!(
            self,
            Self::Apply | Self::Undo | Self::Recover | Self::KeepRecovery
        )
    }
}

/// A per-occurrence applet control. Ids derive from the slot, so duplicate
/// applet IDs keep distinct controls.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AppletControl {
    Select,
    Up,
    Down,
    Remove,
}

/// Stable widget id of an applet row's control.
pub fn control_id(slot: Slot, control: AppletControl) -> cosmic::widget::Id {
    let name = match control {
        AppletControl::Select => "select",
        AppletControl::Up => "up",
        AppletControl::Down => "down",
        AppletControl::Remove => "remove",
    };
    cosmic::widget::Id::new(format!(
        "applet-{}-{}-{name}",
        slot.region.key(),
        slot.index
    ))
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Notice {
    Done(String),
    Error(String),
}

#[derive(Default)]
pub struct DesktopEditor {
    pub islands: islands::State,
    pub penumbra_supported: bool,
    store: Option<Result<DesktopStore, String>>,
    drafts: BTreeMap<PanelKind, PanelDraft>,
    pub selected: PanelKind,
    pub catalog: AppletCatalog,
    pub outputs: Vec<String>,
    /// The persistent latest change, when this page made it.
    undo: Option<LatestChange>,
    /// The owner and label of a latest change another page made; its Undo
    /// lives on that page.
    elsewhere: Option<(Owner, String)>,
    pub notice: Option<Notice>,
    pub advanced: bool,
    pub picker: Option<Region>,
    pub applet_group: AppletGroup,
    pub starter_open: bool,
    pub has_battery: bool,
    /// Text filtering the picker and the installed list.
    pub applet_query: String,
    /// The installed applets list is open.
    pub overview: bool,
    pub drag: AppletDrag,
    /// The applet occurrence last moved, kept highlighted for keyboard users.
    pub focus: Option<Slot>,
    recovery: Option<Result<Journal, String>>,
}

impl DesktopEditor {
    pub fn display_snapshot(&self) -> Self {
        Self {
            islands: self.islands.display_snapshot(),
            penumbra_supported: self.penumbra_supported,
            store: self.store.clone(),
            drafts: self.drafts.clone(),
            selected: self.selected,
            catalog: self.catalog.clone(),
            outputs: self.outputs.clone(),
            undo: self.undo.clone(),
            elsewhere: self.elsewhere.clone(),
            notice: self.notice.clone(),
            advanced: self.advanced,
            picker: self.picker,
            applet_group: self.applet_group,
            starter_open: self.starter_open,
            has_battery: self.has_battery,
            applet_query: self.applet_query.clone(),
            overview: self.overview,
            drag: self.drag.clone(),
            focus: self.focus,
            recovery: self.recovery.clone(),
        }
    }
    /// First visit: choose the store and read current values. Later visits
    /// refresh untouched values and keep the drafts. Reads only.
    pub fn open(&mut self) {
        if self.store.is_some() {
            // Applets installed since the last visit show up; reads only.
            self.discover_applets();
            self.refresh();
            return;
        }
        let store = DesktopStore::from_env().map_err(|error| error.to_string());
        if let Ok(store) = &store {
            self.catalog = Self::catalog_for(store);
            // Fixture discovery never consults the live machine's hardware.
            self.has_battery = store.fixture_root().map_or_else(
                || {
                    std::fs::read_dir("/sys/class/power_supply")
                        .into_iter()
                        .flatten()
                        .flatten()
                        .any(|e| {
                            std::fs::read_to_string(e.path().join("type"))
                                .is_ok_and(|t| t.trim() == "Battery")
                        })
                },
                |root| root.join("battery").is_file(),
            );
            let outputs = store
                .fixture_root()
                .map(|root| root.join("outputs"))
                .filter(|file| file.is_file());
            self.outputs = match outputs {
                Some(file) => std::fs::read_to_string(file)
                    .unwrap_or_default()
                    .lines()
                    .map(str::trim)
                    .filter(|line| !line.is_empty())
                    .map(String::from)
                    .collect(),
                None => panels::connected_outputs(std::path::Path::new("/sys/class/drm")),
            };
        }
        self.store = Some(store);
        self.reload(true);
        self.check_islands();
    }
    #[cfg(test)]
    pub(super) fn session_draft(&self) -> Option<Vec<PanelDraft>> {
        // A crashed editor cancels Arrange. Recover the draft from before
        // its lease, never a half-finished native pointer gesture.
        let drafts: Vec<_> = self
            .drafts
            .values()
            .cloned()
            .map(|mut draft| {
                if let Some(lease) = &self.islands.lease
                    && lease.kind == draft.kind()
                {
                    draft.set_islands(lease.before.clone());
                }
                draft
            })
            .filter(PanelDraft::dirty)
            .collect();
        (!drafts.is_empty()).then_some(drafts)
    }
    pub(super) fn restore_session(&mut self, saved: Vec<PanelDraft>) -> Result<(), String> {
        if saved.len() > PanelKind::ALL.len() {
            return Err("The saved dock and top bar draft has too many entries".into());
        }
        let mut restored = BTreeMap::new();
        for mut draft in saved {
            let kind = draft.kind();
            let current = self
                .draft(kind)
                .ok_or("The current dock or top bar cannot be read")?;
            draft.refresh(current.clone().into_snapshot());
            if restored.insert(kind, draft).is_some() {
                return Err("The saved dock and top bar draft repeats an entry".into());
            }
        }
        self.drafts.extend(restored);
        self.notice = Some(Notice::Done(super::session_drafts::RECOVERED.into()));
        Ok(())
    }

    pub fn opened(&self) -> bool {
        self.store.is_some()
    }
    fn catalog_for(store: &DesktopStore) -> AppletCatalog {
        let applications = store
            .fixture_root()
            .map(|root| root.join("applications"))
            .filter(|dir| dir.is_dir());
        match applications {
            Some(dir) => AppletCatalog::discover(&[dir]),
            None => AppletCatalog::installed(),
        }
    }
    fn discover_applets(&mut self) {
        if let Some(store) = self.store() {
            self.catalog = Self::catalog_for(store);
        }
    }
    /// The region an applet added from the installed list goes to: the
    /// dock's middle, or the end of a panel where status applets sit.
    pub fn usual_region(&self) -> Region {
        match self.selected {
            PanelKind::Dock => Region::Center,
            PanelKind::Panel => Region::End,
        }
    }
    /// Where `id` is placed in each bar's draft, with the bar's direction.
    pub fn placements(&self, id: &str) -> Vec<(PanelKind, Region, bool)> {
        let mut found = Vec::new();
        for kind in PanelKind::ALL {
            let Some(draft) = self.draft(kind) else {
                continue;
            };
            let horizontal = draft
                .anchor()
                .known()
                .is_none_or(|anchor| anchor.horizontal());
            if let Some(regions) = draft.applets().known() {
                for region in Region::ALL {
                    if regions.list(region).iter().any(|item| item == id) {
                        found.push((kind, region, horizontal));
                    }
                }
            }
        }
        found
    }
    /// Whether an installed applet matches the search text.
    pub fn matches(&self, info: &crate::desktop::applets::AppletInfo) -> bool {
        let query = self.applet_query.trim().to_lowercase();
        query
            .split_whitespace()
            .all(|word| info.search_text().contains(word))
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
    pub fn draft(&self, kind: PanelKind) -> Option<&PanelDraft> {
        self.drafts.get(&kind)
    }
    pub fn current(&self) -> Option<&PanelDraft> {
        self.drafts.get(&self.selected)
    }
    pub fn dirty(&self) -> bool {
        self.drafts.values().any(PanelDraft::dirty)
    }
    #[cfg_attr(not(test), allow(dead_code))]
    pub fn changes(&self) -> usize {
        self.drafts.values().map(|draft| draft.patch().len()).sum()
    }
    /// Undo is offered for this page's own latest change only, and never
    /// while an interrupted change waits to be restored or kept.
    #[cfg(test)]
    pub fn can_undo(&self) -> bool {
        self.undo.is_some() && self.recovery.is_none() && !self.dirty() && self.writable()
    }
    /// Apply is offered only for a writable store with edits and no
    /// interrupted change waiting to be restored or kept.
    #[cfg(test)]
    pub fn can_apply(&self) -> bool {
        self.dirty() && self.writable() && self.recovery.is_none() && self.islands_can_apply()
    }
    /// Changes whenever anything visible changes; used by capture mode.
    pub fn state_key(&self) -> String {
        let state = format!(
            "{:?}:{}:{}:{:?}:{:?}:{}:{:?}:{:?}:{:?}:{:?}",
            self.selected,
            self.applet_query,
            self.overview,
            self.drafts
                .values()
                .map(PanelDraft::patch)
                .collect::<Vec<_>>(),
            self.notice,
            self.advanced,
            self.picker,
            self.drag,
            self.focus,
            self.recovery.is_some(),
        );
        format!(
            "{state}:{:?}:{:?}:{}",
            (
                self.islands.selected,
                self.islands.group,
                self.arranging(),
                &self.islands.status,
                &self.islands.gap_text
            ),
            self.applet_group,
            self.starter_open
        )
    }
    pub fn recovery(&self) -> Option<&Result<Journal, String>> {
        self.recovery.as_ref()
    }
    /// Existing user directories to watch for external edits. Only existing
    /// parents are returned, because the watcher creates missing ones.
    pub fn watch_paths(&self) -> Vec<PathBuf> {
        let Some(store) = self.store() else {
            return Vec::new();
        };
        PanelKind::ALL
            .into_iter()
            .flat_map(PanelDraft::keys)
            .map(|key| store.user_path(&key))
            .filter(|path| path.parent().is_some_and(std::path::Path::is_dir))
            .collect()
    }

    fn load(&self, kind: PanelKind) -> Result<PanelDraft, DesktopError> {
        let store = self
            .store()
            .ok_or_else(|| DesktopError::Io("Desktop settings unavailable".into()))?;
        PanelDraft::load(store, kind)
    }
    /// Re-reads whether an interrupted change is waiting. Reads only.
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
                .filter(|latest| latest.owner != Owner::DockPanel)
                .map(|latest| (latest.owner, latest.label.clone()));
            let undo = latest.filter(|latest| latest.owner == Owner::DockPanel);
            self.recovery = recovery;
            self.undo = undo;
        }
    }
    /// Re-reads both panels. `rebase` drops edits (after Apply/Undo).
    fn reload(&mut self, rebase: bool) {
        if self.store().is_none() {
            return;
        }
        self.refresh_recovery();
        for kind in PanelKind::ALL {
            match self.load(kind) {
                Ok(fresh) => match self.drafts.get_mut(&kind) {
                    Some(draft) if !rebase => draft.refresh(snapshot_of(fresh)),
                    _ => {
                        self.drafts.insert(kind, fresh);
                    }
                },
                Err(error) => {
                    self.notice = Some(Notice::Error(error.to_string()));
                }
            }
        }
    }
    pub fn refresh(&mut self) {
        self.reload(false);
    }

    fn edit(&mut self, change: impl FnOnce(&mut PanelDraft)) {
        if let Some(draft) = self.drafts.get_mut(&self.selected) {
            change(draft);
            self.notice = None;
        }
    }
    fn edit_applets(&mut self, change: impl FnOnce(&mut Regions) -> Option<Slot>) {
        let Some(Setting::Known(mut regions)) = self.current().map(PanelDraft::applets) else {
            return;
        };
        let focus = change(&mut regions);
        self.focus = focus;
        self.edit(|draft| draft.set_applets(&regions));
    }

    /// Applies `message`. After an arrow move it returns the control that
    /// should take keyboard focus: the moved occurrence's button for the same
    /// direction, or the other direction at an edge.
    pub fn update(&mut self, message: Message) -> Option<cosmic::widget::Id> {
        if self.drag.active()
            && !matches!(
                message,
                Message::DragOver(_)
                    | Message::DragLeave(_)
                    | Message::Drop
                    | Message::DropAt(_)
                    | Message::DragCancel
                    | Message::Refresh
            )
        {
            self.drag.cancel();
        }
        match message {
            Message::Islands(edit) => self.island_edit(edit),
            Message::Select(kind) => {
                if self.arranging() {
                    self.island_edit(islands::Edit::Cancel);
                }
                self.selected = kind;
                // The selected draft is already loaded. Old measured geometry
                // belongs to the other bar; Arrange/Check obtains fresh data
                // when needed without delaying plain tab navigation.
                self.islands.geometry = None;
                self.islands.measured_output = None;
                self.picker = None;
                self.focus = None;
                self.starter_open = false;
            }
            Message::Anchor(anchor) => self.edit(|draft| draft.set_anchor(anchor)),
            Message::Size(size) => self.edit(|draft| draft.set_size(size)),
            Message::AutoHide(mode) => self.edit(|draft| draft.set_autohide(mode)),
            Message::Timing(timing, value) => self.edit(|draft| draft.set_timing(timing, value)),
            Message::Floating(value) => self.edit(|draft| draft.set_floating(value)),
            Message::Margin(value) => self.edit(|draft| draft.set_margin(value)),
            Message::Extend(value) => self.edit(|draft| draft.set_extend(value)),
            Message::Opacity(value) => self.edit(|draft| draft.set_opacity_percent(value)),
            Message::Penumbra(value) => {
                if self.penumbra_supported || !value {
                    self.edit(|draft| draft.set_penumbra(value));
                }
            }
            Message::PenumbraHeight(value) => self.edit(|draft| draft.set_penumbra_height(value)),
            Message::PenumbraMaximizedOpacity(value) => {
                self.edit(|draft| draft.set_penumbra_maximized_opacity(value));
            }
            Message::Radius(value) | Message::RadiusChoice(value) => {
                self.edit(|draft| draft.set_radius(value))
            }
            Message::Spacing(value) => self.edit(|draft| draft.set_spacing(value)),
            Message::Padding(value) => self.edit(|draft| draft.set_padding(value)),
            Message::Output(output) => {
                self.edit(|draft| draft.set_output(output));
                self.check_islands();
            }
            Message::Background(value) => self.edit(|draft| draft.set_background(value)),
            Message::KeepStyle(value) => self.edit(|draft| draft.set_keep_style(value)),
            Message::Advanced(open) => self.advanced = open,
            Message::Picker(region) => {
                self.picker = region;
                self.applet_query.clear();
                self.applet_group = AppletGroup::All;
                self.starter_open = false;
            }
            Message::PickerRegion(region) => self.picker = Some(region),
            Message::AppletGroup(group) => self.applet_group = group,
            Message::SelectApplet(slot) => {
                self.focus = slot.filter(|s| {
                    self.current()
                        .and_then(|d| d.applets().known().cloned())
                        .is_some_and(|r| s.index < r.list(s.region).len())
                });
                self.picker = None;
            }
            Message::MoveTo(slot, region) => {
                self.edit_applets(|regions| {
                    regions.move_item(
                        slot,
                        Slot {
                            region,
                            index: regions.list(region).len(),
                        },
                    )
                });
                return self
                    .focus
                    .map(|slot| control_id(slot, AppletControl::Select));
            }
            Message::Starter(open) => {
                self.starter_open = open;
                self.picker = None;
            }
            Message::UseStarter if self.selected == PanelKind::Panel => {
                let starter = self.catalog.starter(self.has_battery);
                if starter != Regions::default() {
                    self.edit(|d| d.set_applets(&starter));
                    self.focus = None;
                    self.starter_open = false;
                }
            }
            Message::UseStarter => {}
            Message::PanelStyle(style) if self.selected == PanelKind::Panel => {
                if style == PanelStyle::Islands
                    && !self.store().is_some_and(|s| {
                        crate::desktop::islands::Installation::discover(s).installed()
                    })
                {
                    return None;
                }
                if self
                    .current()
                    .is_some_and(|d| d.islands().known().is_some())
                {
                    let first_islands = self.current().and_then(|d| d.islands().known().cloned())
                        == Some(dock_islands_core::Settings::default());
                    self.island_edit(islands::Edit::Enabled(style == PanelStyle::Islands));
                    if style == PanelStyle::Islands && first_islands {
                        // Separate defaults to left/centre/right; retain every
                        // saved island position and the dock's independent style.
                        self.island_edit(islands::Edit::Placement(
                            dock_islands_core::Placement::Separate,
                        ));
                    }
                    self.edit(|d| {
                        if style != PanelStyle::Islands {
                            d.set_extend(style == PanelStyle::FullWidth);
                        }
                        d.set_floating(style != PanelStyle::FullWidth);
                        if style == PanelStyle::Islands
                            && first_islands
                            && d.radius().known() == Some(&0)
                        {
                            d.set_radius(12);
                        }
                    });
                }
            }
            Message::PanelStyle(_) => {}
            Message::Search(text) => self.applet_query = text.chars().take(64).collect(),
            Message::Overview(open) => {
                self.overview = open;
                self.applet_query.clear();
                if open {
                    self.picker = None;
                }
            }
            Message::AddToBar(id) => {
                let region = self.usual_region();
                self.edit_applets(|regions| Some(regions.add(region, id)));
            }
            Message::Add(region, id) => {
                self.edit_applets(|regions| Some(regions.add(region, id)));
                if self.selected == PanelKind::Panel {
                    self.picker = None;
                }
            }
            Message::Remove(slot) => {
                self.edit_applets(|regions| {
                    regions.remove(slot);
                    None
                });
            }
            Message::Step(slot, delta) => {
                self.edit_applets(|regions| regions.step(slot, delta));
                return self
                    .focus
                    .and_then(|moved| self.focus_after_step(moved, delta));
            }
            Message::DragStart(slot) => {
                if self
                    .current()
                    .is_some_and(|draft| draft.applets().known().is_some())
                {
                    self.drag.start(slot);
                }
            }
            Message::DragOver(slot) => self.drag.over(slot),
            Message::DragLeave(slot) => self.drag.leave(slot),
            Message::DragCancel => self.drag.cancel(),
            Message::DropAt(slot) => {
                self.drag.over(slot);
                return self.update(Message::Drop);
            }
            Message::Drop => {
                let mut drag = std::mem::take(&mut self.drag);
                if drag.source.is_some() && drag.target.is_some() {
                    self.edit_applets(|regions| drag.finish(regions));
                }
            }
            Message::Apply => self.apply(),
            Message::Discard => {
                if self.arranging() {
                    self.island_edit(islands::Edit::Cancel);
                }
                for draft in self.drafts.values_mut() {
                    draft.discard();
                }
                self.picker = None;
                self.focus = None;
                self.starter_open = false;
                self.notice = None;
                self.reload(true);
            }
            Message::Undo => self.undo(),
            Message::Refresh => self.refresh(),
            Message::UseCurrent => self.use_current(),
            Message::Recover => self.recover(true),
            Message::KeepRecovery => self.recover(false),
        }
        None
    }

    /// Which control of the moved occurrence keeps focus. The view enables
    /// Up everywhere except the very first applet and Down everywhere except
    /// the very last, across all three regions.
    fn focus_after_step(&self, moved: Slot, delta: i32) -> Option<cosmic::widget::Id> {
        let Setting::Known(regions) = self.current()?.applets() else {
            return None;
        };
        let can_up = !(moved.region == Region::Start && moved.index == 0);
        let can_down = !(moved.region == Region::End && moved.index + 1 == regions.end.len());
        let control = match (delta < 0, can_up, can_down) {
            (true, true, _) | (false, true, false) => AppletControl::Up,
            (false, _, true) | (true, false, true) => AppletControl::Down,
            _ => AppletControl::Remove,
        };
        Some(control_id(moved, control))
    }

    fn apply(&mut self) {
        // Live Arrange owns its draft until Done/Cancel. Ordinary control
        // completion must not commit or disrupt that lease.
        if self.arranging() {
            return;
        }
        if self
            .islands
            .gap_text
            .as_ref()
            .is_some_and(|text| text.parse::<u16>().ok().is_none_or(|gap| gap > 64))
        {
            self.notice = Some(Notice::Error("Gap must be between 0 and 64 pixels".into()));
            return;
        }
        if self.drafts.values().any(|d| {
            d.edited().any(|key| key == "islands") && d.islands().known().is_some_and(|s| s.enabled)
        }) {
            self.check_islands();
        }
        if !self.islands_can_apply() {
            self.notice = Some(Notice::Error(
                "A compatible Islands panel component is required".into(),
            ));
            return;
        }
        let island_change = self
            .drafts
            .values()
            .any(|d| d.edited().any(|n| n == "islands"));
        let Some(store) = self.store() else {
            return;
        };
        let mut patch = DesktopPatch::new();
        for draft in self.drafts.values() {
            if let Err(error) = patch.merge(draft.patch()) {
                self.notice = Some(Notice::Error(error.to_string()));
                return;
            }
        }
        if patch.is_empty() {
            return;
        }
        match store.apply_owned(&patch, None, Owner::DockPanel, "Dock & Panel") {
            Ok(change) => {
                self.undo = Some(change);
                self.focus = None;
                self.picker = None;
                self.reload(true);
                self.notice = Some(Notice::Done("Applied to your desktop".into()));
                if island_change
                    && self.islands.compatible
                    && let Err(error) = self.acknowledge_islands()
                {
                    self.notice = Some(Notice::Error(format!(
                        "Settings saved. {error}. Undo is available."
                    )));
                }
            }
            Err(DesktopError::Conflict(keys)) => {
                for kind in PanelKind::ALL {
                    if let (Ok(fresh), Some(draft)) =
                        (self.load(kind), self.drafts.get(&kind).cloned())
                    {
                        let mut draft = draft;
                        draft.resolve_conflicts(&snapshot_of(fresh), &keys);
                        self.drafts.insert(kind, draft);
                    }
                }
                self.notice = Some(Notice::Error(format!(
                    "{} changed outside this editor. Your other edits are kept.",
                    describe(&keys)
                )));
            }
            Err(error) => {
                self.after_write_error(&error);
                self.notice = Some(Notice::Error(error.to_string()));
            }
        }
    }

    /// Adopts the current value of every conflicting key group; other edits
    /// and other panels are untouched. Reads only.
    fn use_current(&mut self) {
        for kind in PanelKind::ALL {
            let conflicted = self
                .drafts
                .get(&kind)
                .is_some_and(|draft| !draft.conflicts().is_empty());
            if !conflicted {
                continue;
            }
            match self.load(kind) {
                Ok(fresh) => {
                    if let Some(draft) = self.drafts.get_mut(&kind) {
                        draft.resolve_conflicts(&snapshot_of(fresh), &[]);
                    }
                    self.notice = Some(Notice::Done("Using the current values".into()));
                }
                Err(error) => self.notice = Some(Notice::Error(error.to_string())),
            }
        }
    }

    /// After a failed write the draft is kept. A rollback that could not
    /// finish leaves its journal, so Restore must be offered right away.
    fn after_write_error(&mut self, error: &DesktopError) {
        if *error == DesktopError::RecoveryPending {
            self.reload(false);
        } else {
            self.refresh_recovery();
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
                self.notice = Some(Notice::Done("Previous settings restored".into()));
            }
            Err(DesktopError::Conflict(keys)) => {
                self.notice = Some(Notice::Error(format!(
                    "Can’t undo: {} changed since. Nothing was changed.",
                    describe(&keys)
                )));
            }
            Err(error) => {
                self.after_write_error(&error);
                self.notice = Some(Notice::Error(error.to_string()));
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
                self.notice = Some(Notice::Done(message));
            }
            Err(error) => self.notice = Some(Notice::Error(error.to_string())),
        }
    }
}

fn snapshot_of(draft: PanelDraft) -> crate::desktop::Snapshot {
    draft.into_snapshot()
}

/// “Dock size, Panel applets” for a list of keys.
pub fn describe(keys: &[ConfigKey]) -> String {
    let mut parts: Vec<String> = Vec::new();
    for key in keys {
        if key.namespace() == dock_islands_core::NAMESPACE {
            parts.push(format!("{} islands", key.key()));
            continue;
        }
        let panel = PanelKind::ALL
            .into_iter()
            .find(|kind| kind.namespace() == key.namespace())
            .map(PanelKind::title)
            .unwrap_or(key.namespace());
        let part = format!("{panel} {}", panels::key_label(key.key()));
        if !parts.contains(&part) {
            parts.push(part);
        }
    }
    parts.join(", ")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::desktop::store::tests::Fixture;

    const DOCK: &str = "com.system76.CosmicPanel.Dock";

    #[test]
    fn penumbra_requires_its_own_support_and_does_not_enable_islands() {
        let fixture = Fixture::new("penumbra-editor");
        let mut editor = editor(&fixture);
        editor.islands.compatible = true;
        editor.update(Message::Penumbra(true));
        assert!(!editor.dirty());
        editor.penumbra_supported = true;
        editor.update(Message::Penumbra(true));
        assert_eq!(
            editor.current().unwrap().edited().collect::<Vec<_>>(),
            ["margin", "top_edge_gradient"]
        );
        editor.update(Message::Apply);
        assert!(!editor.dirty());
        assert!(!editor.current().unwrap().islands().known().unwrap().enabled);
        assert!(
            editor
                .current()
                .unwrap()
                .penumbra()
                .known()
                .unwrap()
                .unwrap()
                .enabled
        );
        editor.penumbra_supported = false;
        editor.update(Message::Penumbra(false));
        editor.update(Message::Apply);
        assert!(
            !editor
                .current()
                .unwrap()
                .penumbra()
                .known()
                .unwrap()
                .unwrap()
                .enabled
        );
        editor.update(Message::Undo);
        assert!(
            editor
                .current()
                .unwrap()
                .penumbra()
                .known()
                .unwrap()
                .unwrap()
                .enabled
        );
    }

    fn editor(fixture: &Fixture) -> DesktopEditor {
        fixture.system(
            "com.system76.CosmicPanel",
            "entries",
            "[\"Panel\",\"Dock\"]",
        );
        fixture.system(DOCK, "anchor", "Bottom");
        fixture.system(DOCK, "size", "L");
        fixture.system(DOCK, "plugins_center", "Some([\"a\", \"b\"])");
        fixture.system("com.system76.CosmicPanel.Panel", "anchor", "Top");
        let store = fixture.store();
        let mut editor = DesktopEditor {
            catalog: AppletCatalog::default(),
            store: Some(Ok(store)),
            ..Default::default()
        };
        editor.reload(true);
        editor
    }

    #[test]
    fn panel_starter_is_explicit_installed_only_and_keeps_the_dock() {
        let fixture = Fixture::new("panel-starter");
        let panel = PanelKind::Panel.namespace();
        let original = "Some(([\"unknown\", \"unknown\"], [\"other\"]))";
        fixture.user(&panel, "plugins_wings", original);
        let apps = fixture.0.join("applications");
        std::fs::create_dir_all(&apps).unwrap();
        for suffix in [
            "CosmicPanelWorkspacesButton",
            "CosmicAppletTime",
            "CosmicAppletAudio",
            "CosmicAppletBattery",
        ] {
            std::fs::write(
                apps.join(format!("com.system76.{suffix}.desktop")),
                format!("[Desktop Entry]\nName={suffix}\nX-CosmicApplet=true\n"),
            )
            .unwrap();
        }
        let mut editor = editor(&fixture);
        editor.catalog = AppletCatalog::discover(&[apps]);
        let dock = editor.draft(PanelKind::Dock).unwrap().clone();
        let before = fixture.tree();
        editor.update(Message::Select(PanelKind::Panel));
        editor.update(Message::Starter(true));
        assert!(!editor.dirty());
        assert_eq!(fixture.tree(), before);
        assert_eq!(
            editor.current().unwrap().applets().known().unwrap().start,
            ["unknown", "unknown"]
        );
        editor.update(Message::UseStarter);
        let desired = editor.current().unwrap().applets().known().unwrap().clone();
        assert_eq!(desired.start, ["com.system76.CosmicPanelWorkspacesButton"]);
        assert_eq!(desired.center, ["com.system76.CosmicAppletTime"]);
        assert_eq!(desired.end, ["com.system76.CosmicAppletAudio"]);
        assert_eq!(editor.draft(PanelKind::Dock), Some(&dock));
        assert_eq!(fixture.tree(), before);
        let restored: Vec<PanelDraft> =
            ron::from_str(&ron::to_string(&editor.session_draft().unwrap()).unwrap()).unwrap();
        editor.update(Message::Discard);
        editor.restore_session(restored).unwrap();
        assert_eq!(editor.current().unwrap().applets(), Setting::Known(desired));
        editor.update(Message::Apply);
        assert!(!editor.dirty());
        assert_eq!(editor.draft(PanelKind::Dock), Some(&dock));
        editor.update(Message::Undo);
        assert_eq!(
            fixture.read(&panel, "plugins_wings").as_deref(),
            Some(original)
        );
        assert_eq!(fixture.read(&panel, "plugins_center"), None);
        editor.has_battery = true;
        editor.update(Message::UseStarter);
        assert!(
            editor
                .current()
                .unwrap()
                .applets()
                .known()
                .unwrap()
                .contains("com.system76.CosmicAppletBattery")
        );
    }

    #[test]
    fn panel_selection_moves_one_occurrence_and_preserves_unknown_applets() {
        let fixture = Fixture::new("panel-selection");
        let panel = PanelKind::Panel.namespace();
        fixture.user(
            &panel,
            "plugins_wings",
            "Some(([\"unknown\", \"repeat\", \"repeat\"], [\"end\"]))",
        );
        let mut editor = editor(&fixture);
        editor.update(Message::Select(PanelKind::Panel));
        let before = fixture.tree();
        let slot = Slot {
            region: Region::Start,
            index: 2,
        };
        editor.update(Message::SelectApplet(Some(slot)));
        assert!(!editor.dirty());
        assert_eq!(
            editor
                .update(Message::MoveTo(slot, Region::Center))
                .unwrap()
                .to_string(),
            "applet-center-0-select"
        );
        let r = editor.current().unwrap().applets().known().unwrap().clone();
        assert_eq!(r.start, ["unknown", "repeat"]);
        assert_eq!(r.center, ["repeat"]);
        editor.update(Message::Step(editor.focus.unwrap(), 1));
        assert_eq!(
            editor.focus,
            Some(Slot {
                region: Region::End,
                index: 0
            })
        );
        editor.update(Message::Remove(editor.focus.unwrap()));
        assert_eq!(
            editor.current().unwrap().applets().known().unwrap().end,
            ["end"]
        );
        assert_eq!(fixture.tree(), before);
        editor.update(Message::Opacity(43));
        fixture.user(&panel, "plugins_wings", "Some(([\"external\"], []))");
        editor.update(Message::Refresh);
        assert!(!editor.current().unwrap().conflicts().is_empty());
        editor.update(Message::UseCurrent);
        assert_eq!(
            editor.current().unwrap().applets().known().unwrap().start,
            ["external"]
        );
        assert_eq!(editor.current().unwrap().opacity(), Setting::Known(0.43));
    }

    #[test]
    fn panel_style_preserves_saved_island_layouts_and_dock_draft() {
        let fixture = Fixture::new("panel-style");
        crate::desktop::islands::fixture_installed(&fixture.0);
        let mut editor = editor(&fixture);
        editor.update(Message::Size(PanelSize::XL));
        let dock = editor.draft(PanelKind::Dock).unwrap().clone();
        editor.update(Message::Select(PanelKind::Panel));
        editor.update(Message::PanelStyle(PanelStyle::Islands));
        let s = editor.current().unwrap().islands().known().unwrap().clone();
        assert!(s.enabled);
        assert_eq!(s.placement, dock_islands_core::Placement::Separate);
        assert!(
            editor.can_apply(),
            "installed settings remain manageable without a running host"
        );
        let mut saved = s;
        saved.placement = dock_islands_core::Placement::Together;
        saved.layout_mut("DP-1").separate.positions = [200, 4100, 8500];
        editor.edit(|d| d.set_islands(saved.clone()));
        editor.update(Message::PanelStyle(PanelStyle::FullWidth));
        assert_eq!(editor.current().unwrap().extend(), Setting::Known(true));
        editor.update(Message::PanelStyle(PanelStyle::Islands));
        assert_eq!(editor.current().unwrap().islands(), Setting::Known(saved));
        assert_eq!(editor.draft(PanelKind::Dock), Some(&dock));
        editor.update(Message::Discard);
        assert!(!editor.dirty());
    }

    #[test]
    fn arrange_queries_the_active_panel_instead_of_guessing_a_drm_output() {
        let fixture = Fixture::new("editor-island-active-output");
        let mut editor = editor(&fixture);
        editor.outputs = vec!["DP-1".into(), "DP-2".into()];
        editor.edit(|d| d.set_output(Output::Active));
        assert_eq!(editor.island_output(), "");
        editor.edit(|d| d.set_output(Output::All));
        assert_eq!(editor.island_output(), "");
        editor.edit(|d| d.set_output(Output::Name("DP-2".into())));
        assert_eq!(editor.island_output(), "DP-2");
    }

    #[test]
    fn auditfix_runtime_refresh_keeps_requested_identity_and_clears_lost_measurement() {
        let fixture = Fixture::new("auditfix-islands-output");
        let mut editor = editor(&fixture);
        for configured in [Output::Name("DP-2".into()), Output::All] {
            editor.edit(|d| d.set_output(configured.clone()));
            let requested = editor.requested_island_output();
            editor.accept_runtime(islands::RuntimeStatus {
                panel: Some(editor.selected),
                requested_output: requested.clone(),
                output: Some("eDP-1".into()),
                compatible: true,
                ..Default::default()
            });
            assert_eq!(editor.island_output(), "eDP-1");
            assert_eq!(editor.requested_island_output(), requested);
            editor.accept_runtime(islands::RuntimeStatus {
                panel: Some(editor.selected),
                requested_output: requested.clone(),
                output: Some("DP-2".into()),
                compatible: true,
                ..Default::default()
            });
            assert_eq!(editor.island_output(), "DP-2");
            editor.accept_runtime(islands::RuntimeStatus {
                panel: Some(editor.selected),
                requested_output: requested.clone(),
                ..Default::default()
            });
            assert!(editor.islands.measured_output.is_none());
            assert_eq!(editor.island_output(), requested);
        }
    }

    #[test]
    fn auditfix_ordinary_island_moves_and_gap_completion_commit_but_invalid_text_does_not() {
        use crate::settings::Message as SettingsMessage;
        use islands::Edit;
        let fixture = Fixture::new("auditfix-islands-completion");
        crate::desktop::islands::fixture_installed(&fixture.0);
        let mut editor = editor(&fixture);
        editor.islands.group = true;
        let apply = |editor: &mut DesktopEditor, edit: Edit| {
            let message = SettingsMessage::DesktopEditor(Message::Islands(edit.clone()));
            editor.update(Message::Islands(edit));
            if let Some(SettingsMessage::DesktopEditor(completion)) =
                crate::settings::direct::completion(&message)
            {
                editor.update(completion);
            }
        };
        let key = dock_islands_core::NAMESPACE;
        apply(&mut editor, Edit::Move(1));
        assert!(!editor.dirty());
        assert!(fixture.read(key, "Dock").is_some());
        apply(&mut editor, Edit::GapText("23".into()));
        assert!(editor.dirty());
        apply(&mut editor, Edit::GapFinished);
        assert!(!editor.dirty());
        let saved = fixture.read(key, "Dock").unwrap();
        let settings: dock_islands_core::Settings = ron::from_str(&saved).unwrap();
        assert_eq!(settings.gap, 23);
        apply(&mut editor, Edit::GapText("99".into()));
        apply(&mut editor, Edit::GapFinished);
        assert_eq!(fixture.read(key, "Dock").unwrap(), saved);
        assert!(matches!(editor.notice, Some(Notice::Error(_))));
    }

    #[test]
    fn edits_stay_local_until_apply_and_undo_restores_absence() {
        let fixture = Fixture::new("editor-apply");
        let mut editor = editor(&fixture);
        let before = fixture.tree();
        editor.update(Message::Anchor(Anchor::Left));
        editor.update(Message::Select(PanelKind::Panel));
        editor.update(Message::Size(PanelSize::S));
        // Switching panels keeps both drafts; nothing is written yet.
        assert_eq!(editor.changes(), 2);
        assert_eq!(fixture.tree(), before);
        editor.update(Message::Apply);
        assert_eq!(
            editor.notice,
            Some(Notice::Done("Applied to your desktop".into()))
        );
        assert!(!editor.dirty());
        assert_eq!(fixture.read(DOCK, "anchor").as_deref(), Some("Left"));
        assert!(editor.can_undo());
        editor.update(Message::Undo);
        assert_eq!(fixture.read(DOCK, "anchor"), None);
        assert!(!editor.can_undo());
    }

    #[test]
    fn undo_survives_a_restart_belongs_to_this_page_and_waits_for_recovery() {
        let fixture = Fixture::new("editor-persistent-undo");
        let mut editor = editor(&fixture);
        editor.update(Message::Anchor(Anchor::Left));
        editor.update(Message::Apply);
        // A new settings process offers the same Undo.
        let mut restarted = DesktopEditor {
            store: Some(Ok(fixture.store())),
            ..Default::default()
        };
        restarted.reload(true);
        assert!(restarted.can_undo());
        // Never while an interrupted change waits to be restored or kept.
        let journal = fixture.0.join("state/desktop-journal.json");
        std::fs::write(
            &journal,
            r#"{"format":"desktop-cards-journal","version":1,"id":"x","operation":"apply","created":"","entries":[]}"#,
        )
        .unwrap();
        restarted.update(Message::Refresh);
        assert!(restarted.recovery().is_some());
        assert!(!restarted.can_undo());
        restarted.update(Message::KeepRecovery);
        assert!(
            restarted.can_undo(),
            "keeping current values keeps the Undo"
        );
        // Another owner's latest change is not this page's Undo.
        let store = fixture.store();
        let mut patch = DesktopPatch::new();
        let key = ConfigKey::new("com.system76.CosmicTk", 1, "icon_theme").unwrap();
        patch.set(
            key,
            None,
            Some(crate::desktop::RawValue::parse("\"Pop\"").unwrap()),
        );
        store
            .apply_owned(&patch, None, Owner::Appearance, "Appearance")
            .unwrap();
        restarted.update(Message::Refresh);
        assert!(!restarted.can_undo(), "a newer change by another page");
        restarted.update(Message::Undo);
        assert_eq!(fixture.read(DOCK, "anchor").as_deref(), Some("Left"));
    }

    #[test]
    fn conflicts_keep_other_edits_and_undo_respects_newer_changes() {
        let fixture = Fixture::new("editor-conflict");
        let mut editor = editor(&fixture);
        editor.update(Message::Anchor(Anchor::Top));
        editor.update(Message::Opacity(40));
        fixture.user(DOCK, "anchor", "Right");
        editor.update(Message::Refresh);
        assert_eq!(editor.current().unwrap().conflicts(), ["anchor"]);
        editor.update(Message::Apply);
        assert_eq!(
            editor.notice,
            Some(Notice::Error(
                "Dock position changed outside this editor. Your other edits are kept.".into()
            ))
        );
        assert_eq!(
            editor.current().unwrap().anchor(),
            Setting::Known(Anchor::Right)
        );
        assert_eq!(editor.changes(), 1);
        editor.update(Message::Apply);
        assert_eq!(fixture.read(DOCK, "opacity").as_deref(), Some("0.4"));
        fixture.user(DOCK, "opacity", "0.9");
        editor.update(Message::Undo);
        assert!(
            matches!(&editor.notice, Some(Notice::Error(text)) if text.starts_with("Can’t undo"))
        );
        assert_eq!(fixture.read(DOCK, "opacity").as_deref(), Some("0.9"));
    }

    #[test]
    fn use_current_value_resolves_only_the_conflicting_group() {
        let fixture = Fixture::new("editor-use-current");
        let mut editor = editor(&fixture);
        editor.update(Message::Anchor(Anchor::Top));
        editor.update(Message::Opacity(40));
        editor.update(Message::Select(PanelKind::Panel));
        editor.update(Message::Size(PanelSize::S));
        fixture.user(DOCK, "anchor", "Right");
        editor.update(Message::Refresh);
        assert_eq!(
            editor.draft(PanelKind::Dock).unwrap().conflicts(),
            ["anchor"]
        );
        let before = fixture.tree();
        editor.update(Message::UseCurrent);
        let dock = editor.draft(PanelKind::Dock).unwrap();
        assert!(dock.conflicts().is_empty());
        assert_eq!(dock.anchor(), Setting::Known(Anchor::Right));
        assert_eq!(dock.edited().collect::<Vec<_>>(), ["opacity"]);
        assert!(editor.draft(PanelKind::Panel).unwrap().dirty());
        assert_eq!(fixture.tree(), before, "resolving writes nothing");
        assert!(!Message::UseCurrent.writes());
        editor.update(Message::Apply);
        assert_eq!(fixture.read(DOCK, "anchor").as_deref(), Some("Right"));
        assert_eq!(fixture.read(DOCK, "opacity").as_deref(), Some("0.4"));
    }

    #[test]
    fn failed_rollback_offers_restore_immediately_and_restore_keeps_newer_edits() {
        use crate::desktop::store::Fault;
        let fixture = Fixture::new("editor-rollback");
        let mut editor = editor(&fixture);
        editor.update(Message::Anchor(Anchor::Left));
        editor.update(Message::Opacity(40));
        let key = |name: &str| ConfigKey::new(DOCK, 1, name).unwrap();
        // anchor is written, opacity fails, and restoring anchor fails too.
        editor
            .store()
            .unwrap()
            .inject(Some(Fault::FailWriteAndRollback {
                write: key("opacity"),
                rollback: key("anchor"),
            }));
        editor.update(Message::Apply);
        let Some(Notice::Error(error)) = &editor.notice else {
            panic!("{:?}", editor.notice)
        };
        assert!(error.contains("Use Restore to retry"), "{error}");
        assert_eq!(fixture.read(DOCK, "anchor").as_deref(), Some("Left"));
        // Restore is offered now, not after a second failed Apply.
        assert!(matches!(editor.recovery(), Some(Ok(_))));
        assert!(editor.dirty(), "the draft is kept");
        assert!(!editor.can_apply());
        // The watcher sees our own partial write while the journal waits.
        editor.update(Message::Refresh);
        assert_eq!(editor.current().unwrap().conflicts(), ["anchor"]);
        // Someone sets opacity before the user restores.
        fixture.user(DOCK, "opacity", "0.9");
        editor.store().unwrap().inject(None);
        editor.update(Message::Recover);
        assert_eq!(
            editor.notice,
            Some(Notice::Done(
                "Interrupted change undone; kept newer Dock opacity".into()
            ))
        );
        assert_eq!(fixture.read(DOCK, "anchor"), None);
        assert_eq!(fixture.read(DOCK, "opacity").as_deref(), Some("0.9"));
        assert!(editor.recovery().is_none());
        // anchor is back at its base; only the newer opacity conflicts.
        assert_eq!(editor.current().unwrap().conflicts(), ["opacity"]);
        assert!(editor.can_apply());
        editor.update(Message::Apply);
        assert_eq!(
            fixture.read(DOCK, "anchor"),
            None,
            "conflict writes nothing"
        );
        assert_eq!(
            editor.current().unwrap().edited().collect::<Vec<_>>(),
            ["anchor"]
        );
        editor.update(Message::Apply);
        assert_eq!(fixture.read(DOCK, "anchor").as_deref(), Some("Left"));
        assert_eq!(fixture.read(DOCK, "opacity").as_deref(), Some("0.9"));
    }

    #[test]
    fn keyboard_steps_return_focus_to_the_moved_occurrence() {
        let fixture = Fixture::new("editor-focus");
        // Duplicate occurrences of "s" stay distinct.
        fixture.user(DOCK, "plugins_center", "Some([\"s\", \"s\", \"b\"])");
        let mut editor = editor(&fixture);
        let slot = |region, index| Slot { region, index };
        let focus = |editor: &mut DesktopEditor, from: Slot, delta: i32| {
            editor
                .update(Message::Step(from, delta))
                .map(|id| id.to_string())
        };
        // The second "s" moves up and keeps Up focused at its new place.
        assert_eq!(
            focus(&mut editor, slot(Region::Center, 1), -1).as_deref(),
            Some("applet-center-0-up")
        );
        assert_eq!(editor.focus, Some(slot(Region::Center, 0)));
        // Moving down twice from the last center row: into End, which makes
        // it the very last applet, so Down hands focus to Up.
        assert_eq!(
            focus(&mut editor, slot(Region::Center, 2), 1).as_deref(),
            Some("applet-end-0-up")
        );
        // The first applet overall: Up hands focus to Down.
        assert_eq!(
            focus(&mut editor, slot(Region::Center, 0), -1).as_deref(),
            Some("applet-start-0-down")
        );
        let Setting::Known(regions) = editor.current().unwrap().applets() else {
            panic!()
        };
        assert_eq!(regions.start, ["s"]);
        assert_eq!(regions.center, ["s"]);
        assert_eq!(regions.end, ["b"]);
        // A step that cannot move requests no focus; other messages neither.
        assert_eq!(focus(&mut editor, slot(Region::Start, 0), -1), None);
        assert_eq!(editor.update(Message::Opacity(40)), None);
        assert_eq!(
            control_id(slot(Region::End, 3), AppletControl::Remove).to_string(),
            "applet-end-3-remove"
        );
    }

    #[test]
    fn pointer_drop_and_keyboard_steps_edit_the_draft_and_cancel_is_a_no_op() {
        let fixture = Fixture::new("editor-drag");
        let mut editor = editor(&fixture);
        let start = Slot {
            region: Region::Center,
            index: 0,
        };
        editor.update(Message::DragStart(start));
        editor.update(Message::DragOver(Slot {
            region: Region::End,
            index: 0,
        }));
        editor.update(Message::DragCancel);
        editor.update(Message::Drop);
        assert!(!editor.dirty());
        editor.update(Message::DragStart(start));
        editor.update(Message::DragOver(Slot {
            region: Region::End,
            index: 0,
        }));
        editor.update(Message::Drop);
        let Setting::Known(regions) = editor.current().unwrap().applets() else {
            panic!()
        };
        assert_eq!(regions.end, ["a"]);
        assert_eq!(regions.center, ["b"]);
        editor.update(Message::Step(
            Slot {
                region: Region::End,
                index: 0,
            },
            -1,
        ));
        assert_eq!(
            editor.focus,
            Some(Slot {
                region: Region::Center,
                index: 1
            })
        );
        editor.update(Message::Step(
            Slot {
                region: Region::Center,
                index: 1,
            },
            -1,
        ));
        let Setting::Known(regions) = editor.current().unwrap().applets() else {
            panic!()
        };
        assert_eq!(regions.center, ["a", "b"]);
        assert!(!editor.dirty());
        // Any other control abandons an in-progress drag without moving.
        editor.update(Message::DragStart(start));
        editor.update(Message::DragOver(Slot {
            region: Region::End,
            index: 0,
        }));
        editor.update(Message::Opacity(50));
        editor.update(Message::Drop);
        assert_eq!(
            editor.current().unwrap().edited().collect::<Vec<_>>(),
            ["opacity"]
        );
        editor.update(Message::Discard);
        assert!(!editor.dirty());
    }
}
