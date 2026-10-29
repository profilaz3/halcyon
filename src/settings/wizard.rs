//! First-run setup: Welcome, Look, Dock & panel, Widgets, Arrange, optional Weather and Ready, one
//! decision per screen in the Settings window without the sidebar.
//!
//! A look and the dock and panel choices are applied as they are made,
//! through the same profile plan as Setups, so they never touch applets,
//! their order or anything else the look doesn't own. Every change is a
//! normal desktop change; Cancel undoes the wizard's own changes, newest
//! first. Choosing a city is a preference and stays.
pub mod follow_up;
pub mod widgets;
use crate::desktop::DesktopStore;
use crate::desktop::panels::MAX_RADIUS;
use crate::desktop::profiles::{self, Profile};
use crate::desktop::store::LatestChange;
use crate::displays::Registry;
use crate::drawer::Widget;
use crate::layout_state::Snapshot;
use dock_islands_core::Placement;
use serde_json::json;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Step {
    Welcome,
    Look,
    Bars,
    Widgets,
    Arrange,
    Weather,
    Ready,
}

impl Step {
    pub const ALL: [Self; 7] = [
        Self::Welcome,
        Self::Look,
        Self::Bars,
        Self::Widgets,
        Self::Arrange,
        Self::Weather,
        Self::Ready,
    ];
    pub fn index(self) -> usize {
        Self::ALL.iter().position(|step| *step == self).unwrap_or(0)
    }
    fn next(self) -> Option<Self> {
        Self::ALL.get(self.index() + 1).copied()
    }
    fn back(self) -> Option<Self> {
        self.index().checked_sub(1).map(|index| Self::ALL[index])
    }
    fn key(self) -> &'static str {
        match self {
            Self::Welcome => "welcome",
            Self::Look => "look",
            Self::Bars => "bars",
            Self::Widgets => "widgets",
            Self::Arrange => "arrange",
            Self::Weather => "weather",
            Self::Ready => "ready",
        }
    }
    fn parse(key: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|step| step.key() == key)
    }
    pub fn title(self) -> &'static str {
        match self {
            Self::Welcome => "Halcyon",
            Self::Look => "Look",
            Self::Bars => "Dock & panel",
            Self::Widgets => "Your widgets",
            Self::Arrange => "Arrange widgets",
            Self::Weather => "Weather",
            Self::Ready => "Ready",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Shape {
    Pill,
    Rounded,
    Square,
}

impl Shape {
    pub const ALL: [Self; 3] = [Self::Pill, Self::Rounded, Self::Square];
    pub fn title(self) -> &'static str {
        match self {
            Self::Pill => "Pill",
            Self::Rounded => "Rounded",
            Self::Square => "Square",
        }
    }
    pub fn radius(self) -> u32 {
        match self {
            Self::Pill => MAX_RADIUS,
            Self::Rounded => 12,
            Self::Square => 0,
        }
    }
}

/// The dock and panel choices. The default is what every starter look sets.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Bars {
    /// Islands, kept together; otherwise one bar across the screen.
    pub islands: bool,
    pub panel_shape: Shape,
    pub dock_shape: Shape,
    /// The dock floats off the screen edge.
    pub floating: bool,
    /// The dock hides behind windows.
    pub hides: bool,
}

impl Bars {
    pub fn starter(islands: bool) -> Self {
        Self {
            islands,
            panel_shape: Shape::Pill,
            dock_shape: Shape::Pill,
            floating: true,
            hides: false,
        }
    }

    /// What the dock and panel show now, so an untouched step shows (and
    /// keeps) the person's own setup.
    pub fn current(store: &DesktopStore, islands_available: bool) -> Self {
        use crate::desktop::panels::{AutoHide, PanelDraft, PanelKind};
        let shape = |radius: Option<u32>| match radius {
            Some(0) => Shape::Square,
            Some(radius) if radius < 40 => Shape::Rounded,
            _ => Shape::Pill,
        };
        let dock = PanelDraft::load(store, PanelKind::Dock).ok();
        let panel = PanelDraft::load(store, PanelKind::Panel).ok();
        Self {
            islands: islands_available
                && panel
                    .as_ref()
                    .and_then(|panel| panel.islands().known().map(|islands| islands.enabled))
                    .unwrap_or(false),
            panel_shape: shape(
                panel
                    .as_ref()
                    .and_then(|panel| panel.radius().known().copied()),
            ),
            dock_shape: shape(
                dock.as_ref()
                    .and_then(|dock| dock.radius().known().copied()),
            ),
            floating: dock
                .as_ref()
                .and_then(|dock| dock.floating().known().copied())
                .unwrap_or(false),
            hides: dock
                .as_ref()
                .and_then(|dock| dock.autohide().known().copied())
                .is_some_and(|mode| mode != AutoHide::Never),
        }
    }

    /// A dock-and-panel-only record with just the `touched` choices, so
    /// everything else stays as the person has it. `None` when nothing was
    /// touched.
    pub fn profile(
        self,
        touched: Touched,
        islands_available: bool,
        gap: u16,
    ) -> Result<Option<Profile>, String> {
        let mut panel = serde_json::Map::new();
        if touched.panel_shape {
            panel.insert("border_radius".into(), json!(self.panel_shape.radius()));
        }
        if touched.islands && islands_available {
            panel.insert("floating".into(), json!(self.islands));
            panel.insert("margin".into(), json!(if self.islands { 6 } else { 0 }));
            let islands = dock_islands_core::Settings {
                enabled: self.islands,
                placement: Placement::Together,
                gap,
                ..Default::default()
            };
            panel.insert(
                "islands".into(),
                serde_json::to_value(islands).expect("islands serialize"),
            );
        }
        let mut dock = serde_json::Map::new();
        if touched.dock_shape {
            dock.insert("border_radius".into(), json!(self.dock_shape.radius()));
        }
        if touched.floating {
            dock.insert("floating".into(), json!(self.floating));
            dock.insert("margin".into(), json!(if self.floating { 6 } else { 0 }));
        }
        if touched.hides {
            dock.insert(
                "autohide".into(),
                json!(if self.hides { "OnOverlap" } else { "Never" }),
            );
        }
        if panel.is_empty() && dock.is_empty() {
            return Ok(None);
        }
        let mut record = json!({
            "format": "desktop-cards-profile", "version": profiles::VERSION, "name": "Dock & panel",
        });
        if !panel.is_empty() {
            record["panel"] = panel.into();
        }
        if !dock.is_empty() {
            record["dock"] = dock.into();
        }
        profiles::import(record.to_string().as_bytes())
            .map(Some)
            .map_err(|error| error.to_string())
    }
}

/// The dock and panel choices someone actually made on that step.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Touched {
    pub islands: bool,
    pub panel_shape: bool,
    pub dock_shape: bool,
    pub floating: bool,
    pub hides: bool,
}

impl Touched {
    fn any(self) -> bool {
        self.islands || self.panel_shape || self.dock_shape || self.floating || self.hides
    }
}

#[derive(Clone, Debug)]
pub enum Message {
    /// The identity of the page which rendered a control; survives worker queues.
    Rendered(Step, u64, Box<Message>),
    Start,
    /// Leaves the wizard as it is now (on Welcome: before anything changed).
    Leave,
    Next,
    Back,
    /// Returns to a step from Ready's summary.
    Show(Step),
    Skip,
    Cancel,
    Finish,
    Look(&'static str),
    Islands(bool),
    PanelShape(Shape),
    DockShape(Shape),
    Floating(bool),
    Hides(bool),
    Startup(bool),
    Widget(Widget),
    GroupWidgets(bool),
    SelectWidget(String),
    MoveWidget(String, i8),
    MoveSelected(i8),
    DragWidget(String),
    OverWidget(String),
    LeaveWidget(String),
    DropWidget,
    CancelWidgetDrag,
    UndoWidgetMove,
    SetupWidgetOrder,
    UseSetupWidgets,
    WidgetDisplay(Option<String>),
    MoveWidgetsHere,
}

impl Message {
    pub fn action(&self) -> &Self {
        match self {
            Self::Rendered(_, _, action) => action.action(),
            other => other,
        }
    }
}
fn next_view() -> u64 {
    static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);
    NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
}

/// Drag completion is observed outside the row, including lost focus/Escape.
pub fn widget_input(
    event: cosmic::iced::Event,
    status: cosmic::iced::event::Status,
) -> Option<Message> {
    use cosmic::iced::{
        Event,
        keyboard::{self, key::Named},
        mouse,
    };
    match event {
        Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)) => {
            Some(Message::DropWidget)
        }
        Event::Mouse(mouse::Event::CursorLeft)
        | Event::Window(cosmic::iced::window::Event::Unfocused) => Some(Message::CancelWidgetDrag),
        Event::Keyboard(keyboard::Event::KeyPressed {
            key: keyboard::Key::Named(Named::Escape),
            ..
        }) => Some(Message::CancelWidgetDrag),
        Event::Keyboard(keyboard::Event::KeyPressed {
            key: keyboard::Key::Named(key),
            modifiers,
            ..
        }) if modifiers.alt()
            && !modifiers.control()
            && !modifiers.logo()
            && status == cosmic::iced::event::Status::Ignored =>
        {
            match key {
                Named::ArrowUp => Some(Message::MoveSelected(-1)),
                Named::ArrowDown => Some(Message::MoveSelected(1)),
                _ => None,
            }
        }
        _ => None,
    }
}

/// What Settings does after a wizard message.
#[derive(Debug, PartialEq, Eq)]
pub enum Outcome {
    None,
    /// The desktop changed: reread preferences and editors.
    Changed,
    /// The wizard is over. `finish` starts the cards (and at sign-in when
    /// `startup`); otherwise nothing more happens.
    Done {
        finish: bool,
        startup: bool,
    },
}

#[derive(Clone)]
pub struct Wizard {
    pub step: Step,
    view_id: u64,
    /// The starter look on the desktop, chosen here.
    pub look: Option<&'static str>,
    pub bars: Bars,
    /// The dock and panel as the desktop shows them; Skip returns to these.
    applied_bars: Bars,
    /// Choices made on the dock and panel step since they were last applied.
    touched: Touched,
    pub islands_available: bool,
    /// The wizard's own desktop changes, oldest first, for Cancel. Kept in a
    /// state file too, so Cancel still works after closing and reopening.
    changes: Vec<LatestChange>,
    pub error: Option<String>,
    pub startup: bool,
    /// The person chose Start at sign-in themselves.
    pub startup_touched: bool,
    pub widgets: widgets::Draft,
    pub displays: Registry,
    geometry: Snapshot,
    other_geometry: Vec<Snapshot>,
    bar_insets: Vec<(crate::desktop::panels::Output, usize, f32)>,
    visited: Vec<Step>,
}

/// Setup in progress, kept between Settings sessions.
const STATE_FILE: &str = "setup-wizard.json";
const STATE_FORMAT: &str = "halcyon-setup";

impl Wizard {
    /// `bars` is what the desktop shows now (`Bars::current`); `startup`
    /// whether the cards start at sign-in now (on for a first run).
    pub fn new(islands_available: bool, bars: Bars, startup: bool) -> Self {
        let config = crate::config::Config::load();
        let geometry = crate::layout_state::read(&config);
        let widgets = widgets::Draft::new(
            &config,
            &widgets::raw().unwrap_or_else(|_| json!({})),
            &geometry,
        );
        Self {
            widgets,
            geometry,
            other_geometry: Vec::new(),
            displays: Registry::default(),
            bar_insets: Vec::new(),
            visited: Vec::new(),
            step: Step::Welcome,
            view_id: next_view(),
            look: None,
            bars,
            applied_bars: bars,
            touched: Touched::default(),
            islands_available,
            changes: Vec::new(),
            error: None,
            startup,
            startup_touched: false,
        }
    }

    /// Setup someone left before finishing, with its changes, if any.
    pub fn resume(
        store: &DesktopStore,
        islands_available: bool,
        bars: Bars,
        startup: bool,
    ) -> Option<Self> {
        let path = store.state_file(STATE_FILE).ok()?;
        if std::fs::metadata(&path).ok()?.len() > 2 * 1024 * 1024 {
            return None;
        }
        let text = std::fs::read_to_string(path).ok()?;
        let saved: serde_json::Value = serde_json::from_str(&text).ok()?;
        if saved["format"] != STATE_FORMAT {
            return None;
        }
        let mut wizard = Self::new(islands_available, bars, startup);
        wizard.step = saved["step"]
            .as_str()
            .and_then(Step::parse)
            .unwrap_or(Step::Look);
        wizard.look = saved["look"]
            .as_str()
            .and_then(profiles::preset)
            .map(|preset| preset.id);
        wizard.changes = saved["changes"]
            .as_array()?
            .iter()
            .filter_map(|change| LatestChange::decode(change.as_str()?).ok())
            .collect();
        if let Some(value) = saved.get("widgets") {
            // A malformed draft never reaches the transaction layer; old sessions still resume.
            let mut draft: widgets::Draft = serde_json::from_value(value.clone()).ok()?;
            if !draft.valid() {
                return None;
            }
            draft.migrate_move();
            wizard.widgets = draft;
        }
        wizard.visited = saved["visited"]
            .as_array()
            .map(|steps| {
                steps
                    .iter()
                    .take(32)
                    .filter_map(|s| Step::parse(s.as_str()?))
                    .collect()
            })
            .unwrap_or_default();
        wizard.startup = saved["startup"].as_bool().unwrap_or(startup);
        wizard.startup_touched = saved["startup_touched"].as_bool().unwrap_or(false);
        wizard.capture_insets(Some(store));
        Some(wizard)
    }

    /// Whether setup was left unfinished (it resumes on the next launch).
    pub fn pending(store: &DesktopStore) -> bool {
        store.state_file(STATE_FILE).is_ok_and(|path| path.exists())
    }

    fn save(&self, store: &DesktopStore) -> Result<(), String> {
        let path = store.state_file(STATE_FILE).map_err(|e| e.to_string())?;
        use std::os::unix::fs::DirBuilderExt;
        std::fs::DirBuilder::new()
            .recursive(true)
            .mode(0o700)
            .create(store.state_dir())
            .map_err(|e| e.to_string())?;
        let changes: Vec<String> = self
            .changes
            .iter()
            .filter_map(|change| change.encode().ok())
            .collect();
        let state = json!({
            "format": STATE_FORMAT, "version": 1, "step": self.step.key(),
            "look": self.look, "changes": changes, "widgets": self.widgets,
            "visited": self.visited.iter().map(|s| s.key()).collect::<Vec<_>>(),
            "startup": self.startup, "startup_touched": self.startup_touched,
        });
        crate::config::write_config_text(&path, Some(&state.to_string()))
    }

    /// Setup is over: nothing to resume.
    pub fn forget(store: &DesktopStore) {
        if let Ok(path) = store.state_file(STATE_FILE) {
            let _ = crate::config::write_config_text(&path, None);
        }
    }

    /// A successful global Undo already reversed this exact receipt. Removing
    /// it by ID leaves unrelated and conflicting wizard receipts available to Cancel.
    pub fn forget_undone(&mut self, id: &str, store: &DesktopStore) {
        let before = self.changes.len();
        self.changes.retain(|change| change.id != id);
        if before == self.changes.len() {
            return;
        }
        if self.changes.is_empty() {
            self.look = None;
        }
        self.bars = Bars::current(store, self.islands_available);
        self.applied_bars = self.bars;
        self.capture_insets(Some(store));
        if let Err(error) = self.save(store) {
            self.error = Some(format!("Could not save setup progress: {error}"));
        }
    }

    /// Ordinary Settings can undo a receipt while setup is closed. Patch only
    /// that validated receipt in existing progress, without constructing a new
    /// wizard or round-tripping drafts and unknown receipts through `resume`.
    /// Call only after the exact global Undo succeeded, never from history absence.
    pub fn forget_saved_undone(id: &str, store: &DesktopStore) -> Result<(), String> {
        // Preview preferences have their own Undo and must never access the
        // real desktop's unfinished setup state.
        if !store.writable() {
            return Ok(());
        }
        let path = store.state_file(STATE_FILE).map_err(|e| e.to_string())?;
        let metadata = match std::fs::metadata(&path) {
            Ok(metadata) => metadata,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
            Err(error) => return Err(error.to_string()),
        };
        if metadata.len() > 2 * 1024 * 1024 {
            return Err("Saved setup progress is too large.".into());
        }
        let text = std::fs::read_to_string(&path).map_err(|e| e.to_string())?;
        let mut saved: serde_json::Value = serde_json::from_str(&text)
            .map_err(|_| "Saved setup progress is damaged.".to_string())?;
        if saved["format"] != STATE_FORMAT || saved["version"] != 1 {
            return Err("Saved setup progress has an unsupported format or version.".into());
        }
        let changes = saved
            .get_mut("changes")
            .and_then(serde_json::Value::as_array_mut)
            .ok_or("Saved setup receipts are damaged.")?;
        let before = changes.len();
        changes.retain(|change| {
            !change
                .as_str()
                .and_then(|text| LatestChange::decode(text).ok())
                .is_some_and(|change| change.id == id)
        });
        if before == changes.len() {
            return Ok(());
        }
        if changes.is_empty() {
            saved["look"] = serde_json::Value::Null;
        }
        crate::config::write_config_text(&path, Some(&saved.to_string()))
    }

    pub fn has_changes(&self) -> bool {
        !self.changes.is_empty()
    }

    pub fn rendered(&self, action: Message) -> Message {
        match action {
            Message::Rendered(..) => action,
            _ => Message::Rendered(self.step, self.view_id, Box::new(action)),
        }
    }
    pub fn accepts(&self, message: &Message) -> bool {
        match message {
            Message::Rendered(step, id, action) => {
                *step == self.step && *id == self.view_id && self.accepts(action)
            }
            _ => true,
        }
    }
    pub fn admitted(&self, message: Message) -> Option<Message> {
        if !self.accepts(&message) {
            return None;
        }
        match message {
            Message::Rendered(_, _, action) => self.admitted(*action),
            other => Some(other),
        }
    }

    pub fn update(
        &mut self,
        message: Message,
        store: Option<&DesktopStore>,
        config: &crate::config::Config,
    ) -> Outcome {
        let Some(message) = self.admitted(message) else {
            return Outcome::None;
        };
        if matches!(message, Message::DropWidget | Message::OverWidget(_))
            && self.widgets.dragging.is_none()
        {
            return Outcome::None;
        }
        let transient = matches!(
            message,
            Message::OverWidget(_)
                | Message::LeaveWidget(_)
                | Message::SelectWidget(_)
                | Message::DragWidget(_)
                | Message::CancelWidgetDrag
        );
        if !transient {
            self.error = None;
            self.capture_insets(store);
        }
        let outcome = self.handle(message, store, config);
        if !transient {
            let current = if outcome == Outcome::Changed {
                crate::config::Config::try_load().ok()
            } else {
                None
            };
            self.refresh_widgets(current.as_ref().unwrap_or(config), &self.displays.clone());
        }

        if !transient
            && !matches!(outcome, Outcome::Done { .. })
            && let Some(store) = store
        {
            if let Err(e) = self.save(store) {
                self.error = Some(format!("Could not save setup progress: {e}"));
            }
        }
        outcome
    }

    fn handle(
        &mut self,
        message: Message,
        store: Option<&DesktopStore>,
        config: &crate::config::Config,
    ) -> Outcome {
        match message {
            Message::Rendered(..) => unreachable!("validated before dispatch"),
            Message::Start => {
                self.capture_insets(store);
                self.go(Step::Look);
            }
            Message::Leave => {
                return Outcome::Done {
                    finish: false,
                    startup: false,
                };
            }
            Message::Back => {
                self.widgets.cancel_drag();
                if let Some(step) = self.visited.pop().or_else(|| self.step.back()) {
                    self.step = step;
                    self.view_id = next_view();
                }
            }
            Message::Show(step) => self.go(step),
            Message::Skip => {
                if matches!(self.step, Step::Widgets | Step::Arrange) {
                    self.reset_widgets(config);
                    if self.error.is_none() {
                        self.after_widgets(config);
                    }
                    return Outcome::None;
                }
                // Skipping the dock and panel keeps them as they are.
                if self.step == Step::Bars {
                    self.bars = self.applied_bars;
                    self.touched = Touched::default();
                }
                self.advance();
            }
            Message::Next => {
                if self.step == Step::Arrange {
                    return self.apply_widgets(store, config);
                }
                if self.step == Step::Bars && self.touched.any() {
                    let changed = self.apply_bars(store);
                    if self.error.is_some() {
                        return changed;
                    }
                    self.advance();
                    return changed;
                }
                self.advance();
            }
            Message::Look(id) => return self.choose_look(id, store, config),
            Message::Islands(on) => {
                self.bars.islands = on && self.islands_available;
                self.touched.islands = true;
            }
            Message::PanelShape(shape) => {
                self.bars.panel_shape = shape;
                self.touched.panel_shape = true;
            }
            Message::DockShape(shape) => {
                self.bars.dock_shape = shape;
                self.touched.dock_shape = true;
            }
            Message::Floating(on) => {
                self.bars.floating = on;
                self.touched.floating = true;
            }
            Message::Hides(on) => {
                self.bars.hides = on;
                self.touched.hides = true;
            }
            Message::Widget(w) => self.widgets.toggle(w),
            Message::GroupWidgets(on) => self.widgets.group(on),
            Message::SelectWidget(key) => self.widgets.focused = Some(key),
            Message::MoveWidget(key, delta) => self.move_widget(&key, delta, config),
            Message::MoveSelected(delta) => {
                if let Some(key) = self.widgets.focused.clone() {
                    self.move_widget(&key, delta, config);
                }
            }
            Message::DragWidget(key) => {
                self.widgets.dragging = Some(key.clone());
                self.widgets.focused = Some(key);
            }
            Message::OverWidget(key) => {
                if self.widgets.dragging.is_some() {
                    self.widgets.over = Some(key);
                }
            }
            Message::LeaveWidget(key) => {
                if self.widgets.over.as_deref() == Some(&key) {
                    self.widgets.over = None;
                }
            }
            Message::DropWidget => {
                if let (Some(key), Some(target)) =
                    (self.widgets.dragging.clone(), self.widgets.over.clone())
                {
                    let plan = self.widget_plan(config);
                    self.widgets.move_to(&key, &target, &plan.visible);
                }
                self.widgets.cancel_drag();
            }
            Message::CancelWidgetDrag => self.widgets.cancel_drag(),
            Message::UndoWidgetMove => self.widgets.undo(),
            Message::SetupWidgetOrder => {
                let visible = self.widget_plan(config).visible;
                self.widgets.reset_order(&visible);
            }
            Message::UseSetupWidgets => {
                if let Some(preset) = self.look.and_then(profiles::preset) {
                    let result = (|| -> Result<widgets::Draft, String> {
                        let raw = widgets::raw()?;
                        let mut projected =
                            serde_json::to_value(config).map_err(|e| e.to_string())?;
                        if let Some(layout) = preset.profile.widget_layout {
                            for field in crate::desktop::store::widget::fields(
                                crate::desktop::store::widget::WidgetGroup::Layout,
                            ) {
                                if let Some(value) =
                                    crate::desktop::store::widget::get(&layout, field.pointer)
                                {
                                    crate::desktop::store::widget::put(
                                        &mut projected,
                                        field.pointer,
                                        Some(value),
                                    )?;
                                }
                            }
                        }
                        projected["widgets_visible"] = true.into();
                        let projected = crate::config::check_config_value(&projected)?;
                        let geometry = crate::layout_state::read(&projected);
                        let mut draft = widgets::Draft::new(&projected, &raw, &geometry);
                        draft.display = self.widgets.display.clone();
                        draft.target = self.widgets.target.clone();
                        let visible = draft
                            .plan(
                                &projected,
                                self.geometry.clone(),
                                &self.displays,
                                self.insets_for(&self.geometry),
                            )
                            .visible;
                        draft.reset_order(&visible);
                        Ok(draft)
                    })();
                    match result {
                        Ok(draft) => self.widgets = draft,
                        Err(e) => self.error = Some(e),
                    }
                }
            }
            Message::WidgetDisplay(display) => {
                if display
                    .as_deref()
                    .is_none_or(|d| self.displays.unique(d).is_some())
                {
                    self.widgets.choose_display(display);
                }
            }
            Message::MoveWidgetsHere => self.widgets.move_here(),
            Message::Startup(on) => {
                self.startup = on;
                self.startup_touched = true;
            }
            Message::Cancel => {
                let changed = self.undo_all(store);
                if self.error.is_some() {
                    return changed;
                }
                return Outcome::Done {
                    finish: false,
                    startup: false,
                };
            }
            Message::Finish => {
                if self.widgets.pending {
                    self.go(Step::Arrange);
                    return Outcome::None;
                }
                return Outcome::Done {
                    finish: true,
                    startup: self.startup,
                };
            }
        }
        Outcome::None
    }

    fn go(&mut self, step: Step) {
        self.widgets.cancel_drag();
        if self.step != step {
            self.visited.push(self.step);
            if self.visited.len() > 32 {
                self.visited.remove(0);
            }
            self.step = step;
            self.view_id = next_view();
        }
    }
    fn advance(&mut self) {
        if let Some(step) = self.step.next() {
            self.go(step);
        }
    }
    fn after_widgets(&mut self, config: &crate::config::Config) {
        self.go(
            if self.widgets.selected(Widget::Weather) && config.location.is_none() {
                Step::Weather
            } else {
                Step::Ready
            },
        );
    }
    pub fn refresh_widgets(&mut self, config: &crate::config::Config, displays: &Registry) {
        if self.displays.generation != displays.generation
            || self.displays.contexts(config) != displays.contexts(config)
        {
            self.widgets.cancel_drag();
        }
        let previous_target = self.widgets.target.clone();
        self.displays = displays.clone();
        let projected = self.widgets.config(config);
        self.geometry = crate::layout_state::for_display(
            &projected,
            self.widgets.display.as_deref(),
            false,
            displays,
        );
        if !self.widgets.pending
            && let Ok(raw) = widgets::raw()
        {
            self.widgets.refresh_current(config, &raw, &self.geometry);
        }
        self.widgets.target = self
            .geometry
            .display
            .as_ref()
            .and_then(|d| d.output.clone())
            .unwrap_or_default();
        if previous_target != self.widgets.target
            && (previous_target.is_empty()
                || displays.unique(&previous_target).is_none()
                || displays.same_choice(Some(&previous_target), Some(&self.widgets.target)))
        {
            for target in &mut self.widgets.arranged_on {
                if *target == previous_target {
                    *target = self.widgets.target.clone();
                }
            }
            self.widgets.arranged_on.sort();
            self.widgets.arranged_on.dedup();
        }
        self.other_geometry = displays
            .outputs()
            .iter()
            .filter_map(|o| displays.key(o))
            .filter(|key| key != &self.widgets.target)
            .take(31)
            .map(|key| crate::layout_state::for_display(&projected, Some(&key), false, displays))
            .collect();
    }
    pub fn widget_plan(&self, config: &crate::config::Config) -> widgets::Plan {
        let mut plan = self.widgets.plan(
            config,
            self.geometry.clone(),
            &self.displays,
            self.insets_for(&self.geometry),
        );
        if self.widgets.pending {
            for geometry in &self.other_geometry {
                let other = self.widgets.plan(
                    config,
                    geometry.clone(),
                    &self.displays,
                    self.insets_for(geometry),
                );
                if plan.error.is_none() {
                    plan.error = other.error;
                }
                plan.positions.extend(other.positions);
            }
        }
        plan
    }
    fn reset_widgets(&mut self, config: &crate::config::Config) {
        match widgets::raw() {
            Ok(raw) => {
                let customized = self.widgets.customized;
                self.widgets = widgets::Draft::new(config, &raw, &self.geometry);
                self.widgets.customized = customized;
            }
            Err(e) => self.error = Some(e),
        }
    }
    fn move_widget(&mut self, key: &str, delta: i8, config: &crate::config::Config) {
        let plan = self.widget_plan(config);
        if let Some(index) = plan.visible.iter().position(|w| w.key() == key) {
            let target = index as isize + delta as isize;
            if let Some(w) = usize::try_from(target)
                .ok()
                .and_then(|i| plan.visible.get(i))
            {
                self.widgets.move_to(key, w.key(), &plan.visible);
            }
        }
    }
    fn insets_for(&self, geometry: &Snapshot) -> [f32; 4] {
        use crate::desktop::panels::Output;
        let output = geometry.display.as_ref().and_then(|d| d.output.as_deref());
        let mut insets = [24.; 4];
        for (target, edge, thickness) in &self.bar_insets {
            // Active follows compositor focus; without that signal reserve it
            // conservatively. Named bars reserve only their own output.
            if !matches!(target, Output::Name(name) if output.is_some() && !self.displays.same_choice(Some(name), output))
            {
                insets[*edge] += thickness;
            }
        }
        insets
    }
    fn capture_insets(&mut self, store: Option<&DesktopStore>) {
        use crate::desktop::panels::{Anchor, AutoHide, PanelDraft, PanelKind, PanelSize};
        let Some(store) = store else {
            return;
        };
        self.bar_insets.clear();
        for kind in [PanelKind::Dock, PanelKind::Panel] {
            if let Ok(bar) = PanelDraft::load(store, kind) {
                if !bar.listed() || bar.autohide().known() == Some(&AutoHide::Always) {
                    continue;
                }
                let edge = match bar.anchor().known().copied().unwrap_or(Anchor::Bottom) {
                    Anchor::Left => 0,
                    Anchor::Top => 1,
                    Anchor::Right => 2,
                    Anchor::Bottom => 3,
                };
                let thickness = bar
                    .size()
                    .known()
                    .cloned()
                    .unwrap_or(PanelSize::M)
                    .thickness() as f32
                    + 2. * bar.padding().known().copied().unwrap_or(4) as f32
                    + bar.margin().known().copied().unwrap_or(0) as f32;
                self.bar_insets.push((
                    bar.output()
                        .known()
                        .cloned()
                        .unwrap_or(crate::desktop::panels::Output::All),
                    edge,
                    thickness,
                ));
            }
        }
    }
    fn apply_widgets(
        &mut self,
        store: Option<&DesktopStore>,
        config: &crate::config::Config,
    ) -> Outcome {
        let Some(store) = store else {
            self.error = Some("Desktop settings are unavailable.".into());
            return Outcome::None;
        };
        let plan = self.widget_plan(config);
        let result = self.widgets.patch(&plan, &self.displays).and_then(|patch| {
            if patch.is_empty() {
                return Ok(None);
            }
            store
                .apply_owned(
                    &crate::desktop::DesktopPatch::new(),
                    Some(&patch),
                    crate::desktop::Owner::Profile,
                    "Onboarding widgets",
                )
                .map(Some)
                .map_err(|e| e.to_string())
        });
        match result {
            Ok(change) => {
                self.changes.extend(change);
                match widgets::raw() {
                    Ok(raw) => self.widgets.committed(&raw),
                    Err(e) => {
                        self.error = Some(e);
                        return Outcome::Changed;
                    }
                }
                self.after_widgets(config);
            }
            Err(e) => self.error = Some(e),
        }
        Outcome::Changed
    }

    fn choose_look(
        &mut self,
        id: &'static str,
        store: Option<&DesktopStore>,
        config: &crate::config::Config,
    ) -> Outcome {
        if self.look == Some(id) {
            return Outcome::None;
        }
        let Some(store) = store else {
            self.error = Some("Desktop settings are unavailable.".into());
            return Outcome::None;
        };
        let Some(preset) = profiles::preset(id) else {
            return Outcome::None;
        };
        // Another look goes on top of the last one: if it can't be applied,
        // the last one stays, still selected. Cancel undoes both.
        let bundled = crate::desktop::wallpaper::bundled_dir(store);
        let mut preset = profiles::fitted(preset, config, &bundled, self.islands_available);
        if self.widgets.customized {
            preset.profile.widget_layout = None;
        }
        match apply(store, &preset.profile, true) {
            Ok(change) => {
                self.changes.push(change);
                self.look = Some(id);
                self.bars = Bars::current(store, self.islands_available);
                self.applied_bars = self.bars;
                self.touched = Touched::default();
                self.capture_insets(Some(store));
                if let Ok(current) = crate::config::Config::try_load() {
                    if !self.widgets.customized {
                        self.reset_widgets(&current);
                    }
                    self.refresh_widgets(&current, &self.displays.clone());
                }
            }
            Err(error) => self.error = Some(error),
        }
        Outcome::Changed
    }

    fn apply_bars(&mut self, store: Option<&DesktopStore>) -> Outcome {
        let Some(store) = store else {
            self.error = Some("Desktop settings are unavailable.".into());
            return Outcome::None;
        };
        let gap = self
            .look
            .and_then(profiles::preset)
            .and_then(|preset| preset.profile.panel?.islands)
            .map_or(14, |islands| islands.gap);
        let applied = self
            .bars
            .profile(self.touched, self.islands_available, gap)
            .and_then(|profile| {
                profile
                    .map(|profile| apply(store, &profile, false))
                    .transpose()
            });
        match applied {
            Ok(change) => {
                self.changes.extend(change);
                self.capture_insets(Some(store));
                self.applied_bars = self.bars;
                self.touched = Touched::default();
            }
            Err(error) => self.error = Some(error),
        }
        Outcome::Changed
    }

    /// Undoes the wizard's own changes, newest first, leaving anything else
    /// (a chosen city, an edit made elsewhere) alone. A change whose settings
    /// were edited since stays, and stays listed for another try.
    fn undo_all(&mut self, store: Option<&DesktopStore>) -> Outcome {
        let Some(store) = store else {
            return Outcome::None;
        };
        let any = !self.changes.is_empty();
        while let Some(change) = self.changes.last() {
            if let Err(error) = store.undo_change(change) {
                self.error = Some(format!(
                    "{error} Some of setup's changes stay; undo them from Setups or the Dock & panel page."
                ));
                break;
            }
            self.changes.pop();
        }
        if self.changes.is_empty() {
            self.look = None;
        }
        self.bars = Bars::current(store, self.islands_available);
        self.applied_bars = self.bars;
        self.touched = Touched::default();
        if any { Outcome::Changed } else { Outcome::None }
    }
}

/// Plans `profile` for all its parts and applies it as one change; the
/// change's ID, for Undo. `keep_colors` keeps a look's colors from being
/// recomputed for the wallpaper it just set, as Setups does.
fn apply(
    store: &DesktopStore,
    profile: &Profile,
    keep_colors: bool,
) -> Result<LatestChange, String> {
    use crate::desktop::panels::PanelKind;
    let installed = profiles::Installed::discover(store);
    let selected = profile.domains().into_iter().collect();
    let mut plan = profiles::plan(store, &installed, profile, &selected);
    if !keep_colors {
        for (kind, look) in [
            (PanelKind::Panel, &profile.panel),
            (PanelKind::Dock, &profile.dock),
        ] {
            if let Some(islands) = look.as_ref().and_then(|look| look.islands.as_ref()) {
                plan.choose_islands(store, kind, islands.enabled)
                    .map_err(|error| error.to_string())?;
            }
        }
    }
    if !plan.can_apply() {
        return Err(plan.blocking_reasons().join(" "));
    }
    if let Some(reason) = plan.recheck().into_iter().next() {
        return Err(reason);
    }
    let latest = profiles::apply(store, &plan).map_err(|error| error.to_string())?;
    if keep_colors {
        let _ = crate::desktop::wallpaper::automatic::keep_manual_colors(store);
    }
    Ok(latest)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_dock_and_panel_choice_is_a_valid_record() {
        for islands_available in [true, false] {
            for islands in [true, false] {
                for panel_shape in Shape::ALL {
                    for dock_shape in Shape::ALL {
                        for floating in [true, false] {
                            for hides in [true, false] {
                                let bars = Bars {
                                    islands,
                                    panel_shape,
                                    dock_shape,
                                    floating,
                                    hides,
                                };
                                let all = Touched {
                                    islands: true,
                                    panel_shape: true,
                                    dock_shape: true,
                                    floating: true,
                                    hides: true,
                                };
                                let record = bars.profile(all, islands_available, 12);
                                assert!(record.is_ok(), "{bars:?} {islands_available}: {record:?}");
                            }
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn only_the_choices_made_are_written() {
        let bars = Bars::starter(true);
        assert!(
            bars.profile(Touched::default(), true, 12)
                .unwrap()
                .is_none()
        );
        let only_hiding = Touched {
            hides: true,
            ..Default::default()
        };
        let profile = bars.profile(only_hiding, true, 12).unwrap().unwrap();
        assert!(profile.panel.is_none(), "the panel stays as it is");
        let dock = profile.dock.unwrap();
        assert!(dock.autohide.is_some());
        assert!(dock.border_radius.is_none() && dock.floating.is_none() && dock.margin.is_none());
    }
}

#[cfg(test)]
mod task2_tests {
    use super::*;
    #[test]
    fn task2_explicit_wizard_islands_choice_is_saved_for_disabled_installation() {
        let f = crate::desktop::store::tests::Fixture::new("wizard-disabled-installation");
        crate::desktop::islands::fixture_installed(&f.0);
        f.system("com.system76.CosmicPanel", "entries", "[\"Panel\"]");
        f.system("com.system76.CosmicPanel.Panel", "anchor", "Top");
        let path = f.0.join("config.json");
        std::fs::write(&path, "{}").unwrap();
        let store = f.store().with_app_config(path);
        let profile = Bars::starter(true)
            .profile(
                Touched {
                    islands: true,
                    ..Default::default()
                },
                true,
                12,
            )
            .unwrap()
            .unwrap();
        apply(&store, &profile, false).unwrap();
        let raw = store
            .read_user(&crate::desktop::islands::key(
                crate::desktop::panels::PanelKind::Panel,
            ))
            .unwrap()
            .unwrap();
        assert!(raw.decode::<dock_islands_core::Settings>().unwrap().enabled);
    }
}

/// Resolve keyboard reorder against the actually focused native button, not
/// the last clicked row. Tab traversal and pointer selection can differ.
#[derive(Default)]
pub struct FocusedWidget(Option<String>);
impl cosmic::iced::advanced::widget::Operation<String> for FocusedWidget {
    fn traverse(
        &mut self,
        operate: &mut dyn FnMut(&mut dyn cosmic::iced::advanced::widget::Operation<String>),
    ) {
        operate(self);
    }
    fn focusable(
        &mut self,
        id: Option<&cosmic::widget::Id>,
        _: cosmic::iced::Rectangle,
        state: &mut dyn cosmic::iced::advanced::widget::operation::Focusable,
    ) {
        if state.is_focused() {
            for widget in Widget::ALL {
                for prefix in ["wizard-order-", "wizard-up-", "wizard-down-"] {
                    if id
                        == Some(&cosmic::widget::Id::new(format!(
                            "{prefix}{}",
                            widget.key()
                        )))
                    {
                        self.0 = Some(widget.key().into());
                    }
                }
            }
        }
    }
    fn finish(&self) -> cosmic::iced::advanced::widget::operation::Outcome<String> {
        self.0.clone().map_or(
            cosmic::iced::advanced::widget::operation::Outcome::None,
            cosmic::iced::advanced::widget::operation::Outcome::Some,
        )
    }
}
