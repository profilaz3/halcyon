//! Settings ownership, search and the shared pending/Undo controls.
#[cfg(test)]
pub(super) use super::preference_undo::PreferenceUndo;
use super::{AppSettingsPage, Message, Page, Settings};
use crate::desktop::panels::PanelKind;
use cosmic::app::Task;

#[cfg(test)]
#[path = "navigation_ux_tests.rs"]
mod ux_regressions;

/// What a page changes, used by navigation and search-result context.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Area {
    Looks,
    Appearance,
    Wallpaper,
    Widgets,
    Bars,
    Windows,
    Accessibility,
    App,
}

impl Area {
    pub fn label(self) -> &'static str {
        match self {
            Self::Looks => "Looks",
            Self::Appearance => "Appearance",
            Self::Wallpaper => "Wallpaper",
            Self::Widgets => "Widgets",
            Self::Bars => "Dock and panel",
            Self::Windows => "Windows",
            Self::Accessibility => "Accessibility",
            Self::App => "Halcyon",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(super) enum SearchOpen {
    #[default]
    None,
    City,
    Effects,
    Readability,
    /// The widget material's see-through control.
    Transparency,
    Metric(crate::config::DevboxSection),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SearchHit {
    pub page: Page,
    pub label: &'static str,
    pub terms: &'static str,
    pub advanced: bool,
    anchor: &'static str,
    destination: AppSettingsPage,
    pub(super) bar: Option<crate::desktop::panels::PanelKind>,
    pub(super) open: SearchOpen,
    /// The folded section that holds an advanced control, as the screen
    /// names it. Left out when its name depends on the page’s state.
    section: Option<&'static str>,
    /// The label is the name of the page, tab or bar the result opens. Such
    /// a result comes before the rows on that page.
    names_page: bool,
}

impl SearchHit {
    pub(super) fn app_destination(self) -> AppSettingsPage {
        self.destination
    }
    /// Which object tab holds this control.
    pub(super) fn object_tab(self) -> ObjectTab {
        match self.anchor {
            "widget-design" | "dock-advanced" | "dock-corners" | "music-visualizers"
            | "music-spectrum" => {
                ObjectTab::Design
            }
            _ => ObjectTab::Content,
        }
    }
    pub fn anchor(self) -> &'static str {
        self.anchor
    }
    #[cfg(test)]
    pub(super) fn names_page_result(&self) -> bool {
        self.names_page
    }
    /// This result opens the screen `screen` shows.
    #[cfg(test)]
    pub(super) fn opens(&self, screen: &Self) -> bool {
        self.page.canonical() == screen.page.canonical()
            && (self.page != Page::General || self.destination == screen.destination)
            && (self.bar.is_none() || self.bar == screen.bar)
    }
    /// Both open the same row on the same screen.
    fn same_row(&self, other: &Self) -> bool {
        self.page.canonical() == other.page.canonical()
            && self.label == other.label
            && self.bar == other.bar
            && (self.page != Page::General || self.destination == other.destination)
    }
    const fn at(mut self, anchor: &'static str) -> Self {
        self.anchor = anchor;
        self
    }
    const fn destination(mut self, destination: AppSettingsPage) -> Self {
        self.destination = destination;
        self
    }
    const fn open(mut self, open: SearchOpen) -> Self {
        self.open = open;
        self
    }
    const fn bar(mut self, bar: crate::desktop::panels::PanelKind) -> Self {
        self.bar = Some(bar);
        self
    }
    const fn section(mut self, section: &'static str) -> Self {
        self.section = Some(section);
        self
    }
    const fn names_page(mut self) -> Self {
        self.names_page = true;
        self
    }
    /// Where the result is, as the sidebar, tab and section name it. A
    /// step that repeats the one before it is left out.
    pub(super) fn crumb(self) -> String {
        let page = if self.page == Page::General {
            self.destination.tab_title()
        } else if let Some(bar) = self.bar {
            bar.title()
        } else {
            self.page.nav_title()
        };
        let section = self.section.filter(|_| self.advanced);
        let mut steps: Vec<&str> = Vec::new();
        for step in [Some(self.page.area().label()), Some(page), section]
            .into_iter()
            .flatten()
        {
            if steps.last() != Some(&step) && step != self.label {
                steps.push(step);
            }
        }
        steps.join(" › ")
    }
}

impl Settings {
    pub(super) fn prepare_search_control(&mut self, hit: SearchHit) {
        if matches!(hit.open, SearchOpen::Readability | SearchOpen::Transparency) {
            self.material_target = crate::materials::Target::Widgets;
            self.material_adjust = true;
        }
    }
}

pub fn reveal(target: &'static str) -> Task<Message> {
    use cosmic::iced::runtime::{Action, task};
    task::effect(Action::widget(Reveal {
        target: Some(cosmic::widget::Id::new(target)),
        ..Default::default()
    }))
}

/// The sidebar's scroll area.
pub(super) const SIDEBAR: &str = "settings-sidebar";

/// Scrolls the sidebar until the entry for `page` is in view.
pub(super) fn reveal_in_sidebar(page: Page) -> Task<Message> {
    use cosmic::iced::runtime::{Action, task};
    let root = page.sidebar();
    let id = if root == Page::PINNED {
        "nav-changes".to_owned()
    } else {
        format!("nav-{}", root.key())
    };
    task::effect(Action::widget(Reveal {
        target: Some(cosmic::widget::Id::new(id)),
        ..Default::default()
    }))
}

/// Reveal inside every containing scroll area, including independent inspectors.
/// The same operation handles keyboard focus; resizing does not reset offsets.
#[derive(Default)]
pub(super) struct Reveal {
    target: Option<cosmic::widget::Id>,
    offset: cosmic::iced::Vector,
    scopes: Vec<(cosmic::widget::Id, cosmic::iced::Rectangle, f32, f32)>,
    pending: Option<(
        Option<cosmic::widget::Id>,
        cosmic::iced::Rectangle,
        cosmic::iced::Rectangle,
        cosmic::iced::Vector,
    )>,
    moves: Vec<(cosmic::widget::Id, f32)>,
}
impl Reveal {
    #[cfg(test)]
    pub(super) fn target(id: &str) -> Self {
        Self {
            target: Some(cosmic::widget::Id::new(id.to_owned())),
            ..Default::default()
        }
    }
    fn show(&mut self, bounds: cosmic::iced::Rectangle) {
        let mut top = bounds.y - self.offset.y;
        for (id, view, scroll, maximum) in self.scopes.iter().rev() {
            let height = bounds.height.min((view.height - 12.).max(1.));
            let delta = if top < view.y + 6. {
                top - view.y - 6.
            } else if top + height > view.y + view.height - 6. {
                top + height - view.y - view.height + 6.
            } else {
                0.
            };
            let next = (scroll + delta).clamp(0., *maximum);
            if (next - scroll).abs() > 0.5 {
                self.moves.push((id.clone(), next));
                top -= next - scroll;
            }
        }
    }
}
impl cosmic::iced::advanced::widget::Operation for Reveal {
    fn traverse(&mut self, f: &mut dyn FnMut(&mut dyn cosmic::iced::advanced::widget::Operation)) {
        let previous = (self.offset, self.scopes.len());
        if let Some((id, mut bounds, content, translation)) = self.pending.take() {
            bounds.x -= self.offset.x;
            bounds.y -= self.offset.y;
            if let Some(id) = id {
                self.scopes.push((
                    id,
                    bounds,
                    translation.y,
                    (content.height - bounds.height).max(0.),
                ));
            }
            self.offset += translation;
        }
        f(self);
        self.offset = previous.0;
        self.scopes.truncate(previous.1);
    }
    fn scrollable(
        &mut self,
        id: Option<&cosmic::widget::Id>,
        bounds: cosmic::iced::Rectangle,
        content: cosmic::iced::Rectangle,
        translation: cosmic::iced::Vector,
        _: &mut dyn cosmic::iced::advanced::widget::operation::Scrollable,
    ) {
        self.pending = Some((id.cloned(), bounds, content, translation));
    }
    fn container(&mut self, id: Option<&cosmic::widget::Id>, bounds: cosmic::iced::Rectangle) {
        if self.target.is_some() && id == self.target.as_ref() {
            self.show(bounds);
        }
    }
    fn focusable(
        &mut self,
        id: Option<&cosmic::widget::Id>,
        bounds: cosmic::iced::Rectangle,
        state: &mut dyn cosmic::iced::advanced::widget::operation::Focusable,
    ) {
        if (self.target.is_none() && state.is_focused())
            || (self.target.is_some() && id == self.target.as_ref())
        {
            self.show(bounds);
        }
    }
    fn finish(&self) -> cosmic::iced::advanced::widget::operation::Outcome<()> {
        use cosmic::iced::advanced::widget::operation::Outcome;
        if self.moves.is_empty() {
            Outcome::None
        } else {
            Outcome::Chain(Box::new(ScrollTargets(self.moves.clone())))
        }
    }
}
struct ScrollTargets(Vec<(cosmic::widget::Id, f32)>);
impl cosmic::iced::advanced::widget::Operation for ScrollTargets {
    fn traverse(&mut self, f: &mut dyn FnMut(&mut dyn cosmic::iced::advanced::widget::Operation)) {
        f(self);
    }
    fn scrollable(
        &mut self,
        id: Option<&cosmic::widget::Id>,
        _: cosmic::iced::Rectangle,
        _: cosmic::iced::Rectangle,
        _: cosmic::iced::Vector,
        state: &mut dyn cosmic::iced::advanced::widget::operation::Scrollable,
    ) {
        if let Some((_, y)) = self.0.iter().find(|(target, _)| Some(target) == id) {
            state.scroll_to(
                cosmic::iced::advanced::widget::operation::scrollable::AbsoluteOffset {
                    x: None,
                    y: Some(*y),
                },
            );
        }
    }
}

/// The dock and panel’s folded section, as its toggle and search name it.
pub(super) const BAR_ADVANCED: &str = "Corners, spacing and timing";
/// The Windows page’s folded section, as its toggle and search name it.
const WINDOW_DETAILS: &str = "Corners, outline color and shadows";

// Every label is the name its screen shows for the row or heading a result
// opens; other words people may use belong in the terms.
pub(super) const WIDGET_READABILITY: SearchHit = hit(
    Page::Appearance,
    "Easier to read",
    "widget readability contrast readable bright busy wallpaper small text",
    true,
)
.at("contrast-assist")
.open(SearchOpen::Readability);

pub(super) const LOOKS: SearchHit = hit(
    Page::DesktopProfiles,
    "Looks",
    "setups profiles presets starter looks saved calm evening focus workbench paper rose clean desktop presentation welcome rain snow fog weather mix combine import share file save current",
    false,
)
.names_page();
pub(super) const STARTUP: SearchHit = hit(
    Page::General,
    "Running and startup",
    "general halcyon app settings startup login sign in background service running stopped stop start on off setup again",
    false,
)
.names_page();
pub(super) const ACTIVITY_ISLAND: SearchHit = hit(
    Page::DesktopDock,
    "Activity pill",
    "activity island capsule notifications do not disturb dnd camera microphone privacy bluetooth meetings music media",
    false,
)
.at("panel-items")
.bar(crate::desktop::panels::PanelKind::Panel);
pub(super) const SEE_THROUGH: SearchHit = hit(
    Page::Appearance,
    "Glass and transparency",
    "see-through widgets transparent transparency see through opacity clear glass frosted",
    false,
)
.names_page()
.at("material-scope")
.open(SearchOpen::Transparency);
/// The dock’s own see-through slider, which Glass and transparency links to.
pub(super) const DOCK_SEE_THROUGH: SearchHit = hit(
    Page::DesktopDock,
    "See-through",
    "dock transparent transparency opacity glass see through",
    false,
)
.at("native-bar-opacity")
.bar(crate::desktop::panels::PanelKind::Dock);
/// Blur behind frosted windows, which Glass and transparency links to.
pub(super) const WINDOW_BLUR: SearchHit = hit(
    Page::DesktopWindows,
    "Frosted surfaces and blur",
    "desktop effects blur frosted glass shadows windows transparent transparency",
    true,
)
.at("appearance-effects")
.open(SearchOpen::Effects)
.section(WINDOW_DETAILS);

pub(super) const PRIVACY: SearchHit = hit(
    Page::General,
    "Share usage data",
    "privacy usage reporting anonymous telemetry statistics crashes system",
    false,
)
.destination(AppSettingsPage::Privacy);

pub(super) const CHANGES: SearchHit = hit(
    Page::Changes,
    "Changes",
    "why did my desktop change history automatic scheduled undo recent changes log",
    false,
)
.names_page();
pub(super) const TIME_OF_DAY: SearchHit = hit(
    Page::DesktopAutomation,
    "Time of day",
    "change with time of day day and night style widget style moments clock night style adaptive daily cycle day evening night moments automation schedule automatic time displays monitors rules preset sun solar dawn sunrise noon golden hour sunset city timezone",
    false,
)
.names_page();
pub(super) const SCENE_COVERS_CLOCK: SearchHit =
    hit(Page::Clock, "Let the scene cover the clock", "", false);
pub(super) const MATCH_WEATHER: SearchHit =
    hit(Page::DesktopWallpaper, "Match the weather", "", false);

const SEARCH: &[SearchHit] = &[
    hit(
        Page::General,
        "Help",
        "help diagnostics support problem report status troubleshoot how things are running",
        false,
    )
    .names_page()
    .destination(AppSettingsPage::Help),
    CHANGES,
    hit(
        Page::Themes,
        "Mode",
        "dark light automatic night sunset theme",
        false,
    )
    .at("appearance-mode"),
    hit(
        Page::Themes,
        "Style",
        "shared desktop themes theme look soft outline paper bare frosted widgets save share",
        false,
    ),
    hit(
        Page::Themes,
        "Accent color",
        "accent colour colors highlight buttons wallpaper custom",
        false,
    )
    .at("appearance-accent"),
    hit(
        Page::Themes,
        "Use these colors in apps",
        "colors colours apps applications cosmic sync",
        false,
    )
    .at("colors-scope"),
    hit(
        Page::Themes,
        "Corners",
        "corners roundness rounded square pill radius windows widgets dock panel",
        false,
    )
    .at("appearance-corners"),
    hit(
        Page::Themes,
        "Density",
        "density compact standard spacious spacing apps",
        false,
    )
    .at("appearance-density"),
    WIDGET_READABILITY,
    hit(
        Page::General,
        "Animate settings",
        "settings animations motion transitions disclosure",
        false,
    )
    .at("settings-motion")
    .destination(AppSettingsPage::Interface),
    hit(
        Page::General,
        "Show explanations",
        "show explanations help descriptions hints question mark tooltips text",
        false,
    )
    .at("settings-explanations")
    .destination(AppSettingsPage::Interface),
    hit(
        Page::Devbox,
        "Reorder",
        "system monitor devbox reorder readings drag move order",
        false,
    )
    .at("devbox-reorder-anchor"),
    hit(
        Page::Weather,
        "Location",
        "weather location city forecast belgrade search",
        false,
    )
    .open(SearchOpen::City),
    hit(
        Page::Weather,
        "Units",
        "weather units celsius fahrenheit temperature wind speed",
        false,
    ),
    hit(
        Page::Weather,
        "Dew point",
        "weather dew point dewpoint humidity moisture temperature daily overview",
        // The switch sits in the Weather page's folded "Dew point and how
        // often it updates" section; search opens it.
        true,
    )
    .at("weather-dew-point")
    .section("Dew point and how often it updates"),
    hit(
        Page::Weather,
        "Update every",
        "weather refresh update interval frequency how often",
        true,
    )
    .section("Dew point and how often it updates"),
    hit(
        Page::Devbox,
        "Processor (CPU)",
        "system monitor devbox cpu processor temperature readings history graph sparkline",
        false,
    )
    .at("devbox-cpu")
    .open(SearchOpen::Metric(crate::config::DevboxSection::Cpu)),
    hit(
        Page::Devbox,
        "Memory",
        "system monitor devbox ram memory usage cache swap readings bar",
        false,
    )
    .at("devbox-memory")
    .open(SearchOpen::Metric(crate::config::DevboxSection::Memory)),
    hit(
        Page::Devbox,
        "Disk",
        "system monitor devbox disk usage storage readings bar",
        false,
    )
    .at("devbox-disk")
    .open(SearchOpen::Metric(crate::config::DevboxSection::Disk)),
    hit(
        Page::Devbox,
        "Network",
        "system monitor devbox network activity speed readings history graph sparkline",
        false,
    )
    .at("devbox-network")
    .open(SearchOpen::Metric(crate::config::DevboxSection::Network)),
    hit(
        Page::Devbox,
        "Graphics (GPU)",
        "system monitor devbox gpu graphics readings history graph sparkline",
        false,
    )
    .at("devbox-gpu")
    .open(SearchOpen::Metric(crate::config::DevboxSection::Gpu)),
    hit(
        Page::Devbox,
        "Update every",
        "system monitor devbox refresh update interval how often",
        true,
    )
    .section("How often it updates"),
    // Before the widget corners: transparency is what someone searching it wants.
    SEE_THROUGH,
    hit(
        Page::Appearance,
        "Widget corners",
        "appearance widget card corners radius rounded square opacity border outline style finish",
        true,
    )
    .at("widget-corners"),
    hit(
        Page::Overview,
        "Arrange widgets",
        "layout drag drop move resize size width height placement position grid snap guides arrange",
        false,
    )
    .at("canvas-start"),
    hit(
        Page::Calendar,
        "Calendars",
        "calendar today tomorrow meetings join highlights add link",
        false,
    ),
    hit(
        Page::Calendar,
        "Update every",
        "calendar sync refresh update interval account how often",
        true,
    )
    .section("Sync and sign-in setup"),
    hit(
        Page::Clock,
        "Time and date",
        "clock time date hour format 12 24",
        false,
    ),
    hit(
        Page::Clock,
        "Face",
        "clock face typography size scale alignment center analog digital weight italic surface design font depth landscape lighting light ridge hills follow look style",
        false,
    )
    .at("widget-design"),
    hit(
        Page::Glance,
        "Daily overview",
        "at a glance daily overview combined separate cards clock weather calendar",
        false,
    ),
    hit(
        Page::Glance,
        "Look",
        "at a glance daily overview font background surface layout spacing opacity design follow look style",
        false,
    )
    .at("widget-design"),
    hit(
        Page::Projects,
        "Project folders",
        "projects folders servers branches repositories worktrees",
        false,
    ),
    hit(
        Page::Music,
        "Player",
        "music audio media player artwork progress",
        false,
    ),
    hit(
        Page::Music,
        "Animation",
        "music audio visualizer visualiser radial bars orbit dots pulse rings around the cover",
        false,
    )
    .at("music-visualizers"),
    hit(
        Page::Music,
        "Bars along the bottom",
        "music audio spectrum analyzer bars sound visualizer",
        false,
    )
    .at("music-spectrum"),
    hit(
        Page::Widgets,
        "Widgets",
        "widgets show hide turn on off add cards my widgets display monitor screen second external output",
        false,
    )
    .names_page()
    .at("desktop-widgets"),
    hit(
        Page::Notes,
        "Notes",
        "notes notebook text checklists checklist",
        false,
    )
    .names_page(),
    hit(
        Page::Focus,
        "Focus timer",
        "focus timer pomodoro breaks notifications",
        false,
    )
    .names_page(),
    DOCK_SEE_THROUGH,
    hit(
        Page::DesktopDock,
        "See-through",
        "panel transparent transparency opacity glass see through",
        false,
    )
    .at("native-bar-opacity")
    .bar(PanelKind::Panel),
    hit(
        Page::DesktopAppearance,
        "Text size",
        "font text size bigger smaller larger zoom scaling scale display read",
        false,
    ),
    hit(
        Page::General,
        "Set up again",
        "setup wizard onboarding first run start over welcome tour again",
        false,
    ),
    hit(
        Page::DesktopAutomation,
        "Where you are",
        "city location timezone sunrise sunset sun times",
        false,
    ),
    hit(
        Page::DesktopAppearance,
        "Fonts and icons",
        "desktop font fonts typeface typography interface fixed width icons icon theme",
        false,
    )
    .names_page()
    .at("appearance-fonts"),
    hit(
        Page::DesktopWindows,
        "Window corners",
        "window corners roundness rounded outline shadows",
        true,
    )
    .at("window-corners")
    .open(SearchOpen::Effects)
    .section(WINDOW_DETAILS),
    WINDOW_BLUR,
    hit(
        Page::Projects,
        "Update every",
        "projects refresh update interval frequency how often",
        true,
    )
    .section("How often it updates"),
    hit(
        Page::DesktopDock,
        "Spacing",
        "dock padding spacing gap between items inner edge",
        true,
    )
    .at("dock-advanced")
    .bar(crate::desktop::panels::PanelKind::Dock)
    .section(BAR_ADVANCED),
    hit(
        Page::DesktopDock,
        "Spacing",
        "panel padding spacing gap between items inner edge",
        true,
    )
    .at("dock-advanced")
    .bar(crate::desktop::panels::PanelKind::Panel)
    .section(BAR_ADVANCED),
    hit(
        Page::DesktopDock,
        "Hide timing",
        "dock timing hide delay slide speed duration reveal autohide",
        true,
    )
    .at("dock-advanced")
    .bar(crate::desktop::panels::PanelKind::Dock)
    .section(BAR_ADVANCED),
    hit(
        Page::DesktopDock,
        "Hide timing",
        "panel timing hide delay slide speed duration reveal autohide",
        true,
    )
    .at("dock-advanced")
    .bar(crate::desktop::panels::PanelKind::Panel)
    .section(BAR_ADVANCED),
    hit(
        Page::DesktopDock,
        "Dock corners",
        "dock corners radius rounded square pill",
        true,
    )
    .at("dock-corners")
    .bar(crate::desktop::panels::PanelKind::Dock)
    .section(BAR_ADVANCED),
    hit(
        Page::DesktopDock,
        "Panel corners",
        "panel corners radius rounded square pill",
        true,
    )
    .at("dock-corners")
    .bar(crate::desktop::panels::PanelKind::Panel)
    .section(BAR_ADVANCED),
    hit(
        Page::DesktopWallpaper,
        "Moving effect",
        "wallpaper effects rain snow fog cloud shadows living droplets flakes opacity mist wind refraction animation",
        false,
    )
    .at("rain-controls"),
    hit(
        Page::DesktopWallpaper,
        "Wallpaper",
        "wallpaper background pictures slideshow gradient widget color colors palette accent sync automatic follow desktop pop os",
        false,
    )
    .names_page(),
    hit(
        Page::DesktopIcons,
        "Icons on the desktop",
        "desktop files icons trash home mounted drives size spacing",
        false,
    )
    .names_page(),
    hit(
        Page::DesktopDock,
        "Dock",
        "dock applets position size autohide blend gradient shape compact full width floating",
        false,
    )
    .names_page()
    .bar(crate::desktop::panels::PanelKind::Dock),
    hit(
        Page::DesktopDock,
        "Panel",
        "top bar panel islands applets position size autohide shape compact full width",
        false,
    )
    .names_page()
    .bar(crate::desktop::panels::PanelKind::Panel),
    hit(
        Page::DesktopDock,
        "Background",
        "dock panel background match style follow look dark light color",
        false,
    )
    .at("dock-background"),
    hit(
        Page::DesktopDock,
        "Visibility",
        "dock panel autohide auto-hide hide behind windows always visible",
        false,
    )
    .at("dock-visibility"),
    ACTIVITY_ISLAND,
    hit(
        Page::DesktopWindows,
        "Tile windows automatically",
        "windows tiling tile arranging gaps focus controls snapping",
        false,
    ),
    hit(
        Page::DesktopWorkspaces,
        "Workspaces",
        "workspaces horizontal vertical navigation wrap",
        false,
    ),
    hit(
        Page::DesktopWindowRules,
        "Apps that don’t tile",
        "floating apps exceptions window rules tiling",
        false,
    )
    .names_page(),
    hit(
        Page::DesktopShortcuts,
        "Keyboard shortcuts",
        "shortcuts keyboard keys commands bindings hotkeys",
        false,
    )
    .names_page(),
    hit(
        Page::DesktopMotion,
        "Reduce motion",
        "accessibility motion animations supported apps gtk reduced",
        false,
    ),
    LOOKS,
    hit(
        Page::Builder,
        "Widget builder",
        "make your own widget recipe custom clock weather calendar agenda cpu memory disk network gpu uptime system readings native parts compose",
        false,
    )
    .names_page(),
    TIME_OF_DAY,
    STARTUP,
    hit(
        Page::Widgets,
        "Widget controls",
        "widget controls sidebar screen edge menu side panel slide delay animation",
        false,
    )
    .at("widgets-behave"),
    PRIVACY,
    hit(
        Page::General,
        "Updates",
        "updates installation version islands addon add-on diagnostics check",
        false,
    )
    .names_page()
    .destination(AppSettingsPage::Updates),
    hit(
        Page::General,
        "What’s new",
        "release notes new features changes changelog version whats",
        false,
    )
    .names_page()
    .destination(AppSettingsPage::WhatsNew),
    hit(
        Page::Changes,
        "Desktop from before Halcyon",
        "recovery restore reset previous desktop wallpaper hide widgets earlier setups before halcyon",
        false,
    )
    .at("changes-earlier"),
];

/// Row titles and section headings the pages draw that no result above
/// names. `search_finds_every_title_the_pages_draw` fails when a page draws
/// a title that neither this table nor the results above hold.
const ROWS: &[SearchHit] = &[
    hit(Page::Appearance, "Clear glass", "", false),
    hit(Page::Appearance, "Frosted", "", false),
    hit(Page::Appearance, "Liquid", "", false),
    hit(Page::Appearance, "Other surfaces", "", false),
    hit(Page::Appearance, "Smoked glass", "", false),
    hit(Page::Appearance, "Widget corners, opacity and outline", "", false),
    hit(Page::Builder, "Add a part", "", false),
    hit(Page::Builder, "Basics", "", false),
    hit(Page::Builder, "Clock face", "", false),
    hit(Page::Builder, "Date", "", false),
    hit(Page::Builder, "Go back to the built-in clock", "", false),
    hit(Page::Builder, "Make a copy", "", false),
    hit(Page::Builder, "More", "", false),
    hit(Page::Builder, "Name", "", false),
    hit(Page::Builder, "Parts, top to bottom", "", false),
    hit(Page::Builder, "Save without using it", "", false),
    hit(Page::Builder, "Size", "", false),
    hit(Page::Builder, "Type", "", false),
    hit(Page::Calendar, "Add a calendar", "", false),
    hit(Page::Calendar, "Hide event details", "", false),
    hit(Page::Calendar, "Highlight meetings", "", false),
    hit(Page::Calendar, "Sync and sign-in setup", "", false),
    hit(Page::Calendar, "Tomorrow", "", false),
    hit(Page::Clock, "24-hour time", "", false),
    hit(Page::Clock, "Alignment", "", false),
    SCENE_COVERS_CLOCK,
    hit(Page::Clock, "Match the desktop style", "", false),
    hit(Page::Clock, "Show the date", "", false),
    hit(Page::Clock, "Text", "", false),
    hit(Page::Clock, "Text size", "", false),
    hit(Page::DesktopAppearance, "Fixed-width font", "", false),
    hit(Page::DesktopAppearance, "Icon theme", "", false),
    hit(Page::DesktopAppearance, "Interface font", "", false),
    hit(Page::DesktopAppearance, "Match style", "", false),
    hit(Page::DesktopAppearance, "Widget font", "", false),
    hit(Page::DesktopAutomation, "Set times and screens", "", false),
    hit(Page::Moments, "Showing now", "", false),
    hit(Page::DesktopAutomation, "What changes", "", false),
    hit(Page::DesktopAutomation, "Where you are", "", false),
    hit(Page::DesktopDock, "Between items", "", true)
        .at("dock-advanced")
        .bar(PanelKind::Dock)
        .section(BAR_ADVANCED),
    hit(Page::DesktopDock, "Blend into wallpaper", "", false).bar(PanelKind::Dock),
    hit(Page::DesktopDock, BAR_ADVANCED, "", true)
        .at("dock-advanced")
        .bar(PanelKind::Dock)
        .section(BAR_ADVANCED),
    hit(Page::DesktopDock, "Floating", "", false).bar(PanelKind::Dock),
    hit(Page::DesktopDock, "Hide after", "", true)
        .at("dock-advanced")
        .bar(PanelKind::Dock)
        .section(BAR_ADVANCED),
    hit(Page::DesktopDock, "Inner padding", "", true)
        .at("dock-advanced")
        .bar(PanelKind::Dock)
        .section(BAR_ADVANCED),
    hit(Page::DesktopDock, "Keep style when maximized", "", true)
        .at("dock-advanced")
        .bar(PanelKind::Dock)
        .section(BAR_ADVANCED),
    hit(Page::DesktopDock, "Position", "", false).bar(PanelKind::Dock),
    hit(Page::DesktopDock, "Reveal edge", "", true)
        .at("dock-advanced")
        .bar(PanelKind::Dock)
        .section(BAR_ADVANCED),
    hit(Page::DesktopDock, "Screen edge", "", false).bar(PanelKind::Dock),
    hit(Page::DesktopDock, "See-through", "", false).bar(PanelKind::Dock),
    hit(Page::DesktopDock, "Shape", "", false).bar(PanelKind::Dock),
    hit(Page::DesktopDock, "Show after", "", true)
        .at("dock-advanced")
        .bar(PanelKind::Dock)
        .section(BAR_ADVANCED),
    hit(Page::DesktopDock, "Size", "", false).bar(PanelKind::Dock),
    hit(Page::DesktopDock, "Slide duration", "", true)
        .at("dock-advanced")
        .bar(PanelKind::Dock)
        .section(BAR_ADVANCED),
    hit(Page::DesktopDock, "Style", "", false).bar(PanelKind::Dock),
    hit(Page::DesktopDock, "Between items", "", true)
        .at("dock-advanced")
        .bar(PanelKind::Panel)
        .section(BAR_ADVANCED),
    hit(Page::DesktopDock, "Blend into wallpaper", "", false).bar(PanelKind::Panel),
    hit(Page::DesktopDock, BAR_ADVANCED, "", true)
        .at("dock-advanced")
        .bar(PanelKind::Panel)
        .section(BAR_ADVANCED),
    hit(Page::DesktopDock, "Floating", "", false).bar(PanelKind::Panel),
    hit(Page::DesktopDock, "Hide after", "", true)
        .at("dock-advanced")
        .bar(PanelKind::Panel)
        .section(BAR_ADVANCED),
    hit(Page::DesktopDock, "Inner padding", "", true)
        .at("dock-advanced")
        .bar(PanelKind::Panel)
        .section(BAR_ADVANCED),
    hit(Page::DesktopDock, "Keep style when maximized", "", true)
        .at("dock-advanced")
        .bar(PanelKind::Panel)
        .section(BAR_ADVANCED),
    hit(Page::DesktopDock, "Position", "", false).bar(PanelKind::Panel),
    hit(Page::DesktopDock, "Reveal edge", "", true)
        .at("dock-advanced")
        .bar(PanelKind::Panel)
        .section(BAR_ADVANCED),
    hit(Page::DesktopDock, "Screen edge", "", false).bar(PanelKind::Panel),
    hit(Page::DesktopDock, "See-through", "", false).bar(PanelKind::Panel),
    hit(Page::DesktopDock, "Shape", "", false).bar(PanelKind::Panel),
    hit(Page::DesktopDock, "Show after", "", true)
        .at("dock-advanced")
        .bar(PanelKind::Panel)
        .section(BAR_ADVANCED),
    hit(Page::DesktopDock, "Size", "", false).bar(PanelKind::Panel),
    hit(Page::DesktopDock, "Slide duration", "", true)
        .at("dock-advanced")
        .bar(PanelKind::Panel)
        .section(BAR_ADVANCED),
    hit(Page::DesktopDock, "Style", "", false).bar(PanelKind::Panel),
    hit(Page::DesktopIcons, "Show on the desktop", "", false),
    hit(Page::DesktopIcons, "Size", "", false),
    hit(Page::DesktopMotion, "Dock and panel slide speed", "", false),
    hit(Page::DesktopMotion, "Easier-to-read widgets", "", false),
    hit(Page::DesktopMotion, "Keyboard", "", false),
    hit(Page::DesktopMotion, "Motion", "", false),
    hit(Page::DesktopMotion, "Reading", "", false),
    MATCH_WEATHER,
    hit(Page::DesktopWallpaper, "Picture size", "", false),
    hit(Page::DesktopWallpaper, "Same on all displays", "", false),
    hit(Page::DesktopWindows, "Active window outline", "", false),
    hit(Page::DesktopWindows, "Arranging windows", "", false),
    hit(Page::DesktopWindows, "Choosing a window", "", false),
    hit(Page::DesktopWindows, "Corners, outline color and shadows", "", false),
    hit(Page::DesktopWindows, "Direction", "", false),
    hit(Page::DesktopWindows, "Effects", "", false),
    hit(Page::DesktopWindows, "Maximize button", "", false),
    hit(Page::DesktopWindows, "Minimize button", "", false),
    hit(Page::DesktopWindows, "Move the pointer to the window you switch to", "", false),
    hit(Page::DesktopWindows, "Outline the active window", "", false),
    hit(Page::DesktopWindows, "Outline width", "", false),
    hit(Page::DesktopWindows, "Overview", "", false),
    hit(Page::DesktopWindows, "Round floating window corners", "", false),
    hit(Page::DesktopWindows, "Round tiled window corners", "", false),
    hit(Page::DesktopWindows, "Select a window by pointing at it", "", false),
    hit(Page::DesktopWindows, "Shadows on tiled windows", "", false),
    hit(Page::DesktopWindows, "Snap to screen edges", "", false),
    hit(Page::DesktopWindows, "Switch workspaces", "", false),
    hit(Page::DesktopWindows, "Title bar and outline", "", false),
    hit(Page::DesktopWindows, "When you start typing", "", false),
    hit(Page::DesktopWindows, "Wrap around", "", false),
    hit(Page::Devbox, "Background services", "", false),
    hit(Page::Devbox, "How often it updates", "", false),
    hit(Page::Devbox, "Time since start", "", false),
    hit(Page::Focus, "Break", "", false),
    hit(Page::Focus, "Focus", "", false),
    hit(Page::Focus, "How long", "", false),
    hit(Page::Focus, "Long break", "", false),
    hit(Page::Focus, "Long break after", "", false),
    hit(Page::Focus, "Send a notification", "", false),
    hit(Page::Focus, "Start the break automatically", "", false),
    hit(Page::Focus, "When time is up", "", false),
    hit(Page::General, "How things are running", "", false).destination(AppSettingsPage::Help),
    hit(Page::General, "Report a problem", "", false).destination(AppSettingsPage::Help),
    hit(Page::General, "Running in the background", "", false).destination(AppSettingsPage::Help),
    hit(Page::General, "This window", "", false).destination(AppSettingsPage::Interface),
    hit(Page::General, "Running", "", false).destination(AppSettingsPage::Operation),
    hit(Page::General, "Set up again", "", false).destination(AppSettingsPage::Operation),
    hit(Page::General, "Setting up", "", false).destination(AppSettingsPage::Operation),
    hit(Page::General, "Start when you sign in", "", false).destination(AppSettingsPage::Operation),
    hit(Page::General, "Always stays on your computer", "", false).destination(AppSettingsPage::Privacy),
    hit(Page::General, "Privacy policy", "", false).destination(AppSettingsPage::Privacy),
    hit(Page::General, "Sharing", "", false).destination(AppSettingsPage::Privacy),
    hit(Page::General, "Your calendar, notes, accounts and projects", "", false).destination(AppSettingsPage::Privacy),
    hit(Page::General, "Get ready for your own designs", "", false).destination(AppSettingsPage::Updates),
    hit(Page::General, "Islands panel", "", false).destination(AppSettingsPage::Updates),
    hit(Page::General, "More about this version", "", false).destination(AppSettingsPage::Updates),
    hit(Page::General, "This version", "", false).destination(AppSettingsPage::Updates),
    hit(Page::General, "Updates come from your system", "", false).destination(AppSettingsPage::Updates),
    hit(Page::General, "Version", "", false).destination(AppSettingsPage::Updates),
    hit(Page::Glance, "Background", "", false),
    hit(Page::Glance, "Clock font", "", false),
    hit(Page::Glance, "Font", "", false),
    hit(Page::Glance, "Include in this card", "", false),
    hit(Page::Glance, "Same as text", "", false),
    hit(Page::Glance, "Same as widgets", "", false),
    hit(Page::Glance, "Spacing", "", false),
    hit(Page::Glance, "Text", "", false),
    hit(Page::Glance, "Tomorrow’s events", "", false),
    hit(Page::Music, "Album art", "", false),
    hit(Page::Music, "Around the cover", "", false),
    hit(Page::Music, "Card", "", false),
    hit(Page::Music, "Progress bar", "", false),
    hit(Page::Notes, "Card size", "", false),
    hit(Page::Notes, "Your note", "", false),
    hit(Page::Overview, "Set up your desktop", "", false),
    hit(Page::Overview, "Show widgets", "", false),
    hit(Page::PendingChanges, "Unfinished changes", "", false).names_page(),
    hit(Page::Projects, "Add a folder", "", false),
    hit(Page::Projects, "How often it updates", "", false),
    hit(Page::Themes, "Color", "", false),
    hit(Page::Themes, "Surfaces", "", false),
    hit(Page::Themes, "Text and icons", "", false),
    hit(Page::Weather, "Dew point and how often it updates", "", false),
    hit(Page::Widgets, "How widgets behave", "", false).at("widgets-behave"),
    hit(Page::Widgets, "Slide in", "", false).at("widgets-behave"),
];

/// Every search result, for tests that open each one.
#[cfg(test)]
pub(super) fn all_search_hits() -> &'static [SearchHit] {
    index()
}
/// A result that opens one page, tab and fold, for tests that walk every
/// screen.
#[cfg(test)]
pub(super) fn probe(
    page: Page,
    destination: AppSettingsPage,
    bar: Option<crate::desktop::panels::PanelKind>,
    advanced: bool,
) -> SearchHit {
    let mut probe = hit(page, "", "", advanced).destination(destination);
    probe.bar = bar;
    probe
}
/// Row titles that are not settings, and why search leaves them out.
#[cfg(test)]
pub(super) const NOT_INDEXED: &[(&str, &str)] = &[
    ("Desktop", "a heading on Wallpaper over Icons on the desktop, which has its own result"),
    ("Clock type", "a part of the day’s own value; search opens the Clock setting"),
    ("Clock weight", "a part of the day’s own value; search opens the Clock setting"),
    ("Clock italic", "a part of the day’s own value; search opens the Clock setting"),
    ("Clock background", "a part of the day’s own value; search opens the Clock setting"),
    ("Widget look", "a part of the day’s own value; search opens Glass and transparency"),
    ("No glass", "a choice in the glass picker; “glass” finds it"),
    ("Layout", "a section of the widget builder; “layout” opens Arrange widgets"),
    ("Size and place", "on every widget’s Position tab; the widget’s name finds it"),
    ("Move or resize", "on every widget page; the widget’s name finds it"),
    ("Make your own design", "a link on every widget page; “Widget builder” finds it"),
    ("More in COSMIC Settings", "a link out of Halcyon"),
    ("Before Halcyon", "a group in the Changes list, which follows your history"),
    ("Changed Daily overview", "an entry in the Changes list"),
    ("Wallpaper from before Halcyon", "an entry in the Changes list"),
    ("Nothing to recover", "shown only when there is nothing to list"),
    ("No apps yet", "shown only when there is nothing to list"),
    ("Nothing waiting", "shown only when there is nothing to list"),
    ("No shortcuts of your own yet", "shown only when there is nothing to list"),
    ("Tiling is off", "a notice, not a setting"),
    ("Update your widgets?", "a confirmation that opens from “Get ready…”"),
    ("Displays", "a status row on Help that opens the setting it’s about"),
    ("Islands panel", "a status row on Help that opens the setting it’s about; Updates has the group"),
    ("Set up your desktop", "a card shown until setup is finished; Home has it in search"),
    ("Status", "a status row that opens the setting it’s about"),
    ("Check again", "a button"),
    ("Search settings", "a key listed under Keyboard on Accessibility; “keyboard” finds the list"),
    ("Undo", "a key listed under Keyboard on Accessibility; “keyboard” finds the list"),
    ("Go back", "a key listed under Keyboard on Accessibility; “keyboard” finds the list"),
    ("Move between controls", "a key listed under Keyboard on Accessibility; “keyboard” finds the list"),
    ("Close", "a key listed under Keyboard on Accessibility; “keyboard” finds the list"),
];

/// Labels the screen draws only in a state the test fixture lacks, and why.
#[cfg(test)]
pub(super) const UNCHECKED_LABELS: &[(&str, &str)] = &[
    (
        "Activity pill",
        "named by its item in the panel’s Contents or by the Add Activity pill button, \
         and the fixture has no Halcyon panel to offer either",
    ),
    ("Tomorrow", "in Calendar’s Agenda, shown once a calendar is connected"),
    ("Hide event details", "in Calendar’s Agenda, shown once a calendar is connected"),
    ("Highlight meetings", "in Calendar’s Agenda, shown once a calendar is connected"),
];

const fn hit(page: Page, label: &'static str, terms: &'static str, advanced: bool) -> SearchHit {
    SearchHit {
        page,
        label,
        terms,
        advanced,
        anchor: if advanced {
            "settings-advanced"
        } else {
            "page-start"
        },
        destination: AppSettingsPage::Operation,
        open: SearchOpen::None,
        bar: None,
        section: None,
        names_page: false,
    }
}

/// What a search found. `partial` says no result matched every word, so the
/// results are the ones that match the most words.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Matches {
    pub hits: Vec<SearchHit>,
    pub partial: bool,
}

/// Every result: the hand-written ones with their anchors and synonyms, then
/// each row title and heading the pages draw that none of them names.
fn index() -> &'static [SearchHit] {
    static INDEX: std::sync::OnceLock<Vec<SearchHit>> = std::sync::OnceLock::new();
    INDEX.get_or_init(|| {
        let mut index = SEARCH.to_vec();
        for hit in &mut index {
            let title = if hit.page == Page::General {
                hit.destination.title()
            } else {
                hit.page.title()
            };
            hit.names_page |= hit.label == title && hit.bar.is_none() && !hit.advanced;
        }
        // Every page is found by its title, ahead of the rows on it.
        let pages = Page::ALL
            .into_iter()
            .filter(|page| *page == page.canonical() && *page != Page::General)
            // Headed “Unfinished changes” and “Keyboard shortcuts”, which find them.
            .filter(|page| !matches!(page, Page::PendingChanges | Page::DesktopShortcuts))
            .map(|page| hit(page, page.title(), "", false).names_page())
            .chain(
                [AppSettingsPage::Home]
                    .into_iter()
                    .chain(AppSettingsPage::ALL)
                    .chain([AppSettingsPage::WhatsNew])
                    .map(|destination| {
                        hit(Page::General, destination.title(), "", false)
                            .destination(destination)
                            .names_page()
                    }),
            );
        for row in pages.chain(ROWS.iter().copied()) {
            if !index.iter().any(|hit| hit.same_row(&row)) {
                index.push(row);
            }
        }
        index
    })
}

/// Search stays entirely local. Whole words and prefixes come before aliases
/// and spelling corrections; a typo never displaces a literal result. A
/// result named after its page comes before the rows on pages.
pub fn search(query: &str) -> Vec<SearchHit> {
    find(query).hits
}

/// [`search`], and whether the results match only some of the words.
pub fn find(query: &str) -> Matches {
    use std::cmp::Reverse;

    let mut query = query_words(query);
    if query.is_empty() {
        return Matches::default();
    }
    // Repeating a word must not inflate its relevance.
    let mut seen = std::collections::HashSet::new();
    query.retain(|word| seen.insert(word.clone()));
    let phrase = query.join(" ");
    let mut complete = Vec::new();
    let mut some = Vec::new();
    for (index, entry) in index().iter().copied().enumerate() {
        let label = search_words(entry.label);
        let page = search_words(entry.page.title());
        let terms = search_words(entry.terms);
        let parent = search_words(entry.page.nav().title());
        let fields = [(&label, 12), (&page, 8), (&terms, 6), (&parent, 2)];
        let mut corrections = 0;
        let mut aliases = 0;
        let mut prefixes = 0;
        let mut relevance = 0;
        let mut found = 0;
        // Each word found is in this result's own name.
        let mut in_label = true;
        for word in &query {
            let best = fields
                .iter()
                .enumerate()
                .flat_map(|(field, (tokens, weight))| {
                    tokens.iter().filter_map(move |token| {
                        word_match(word, token).map(|(typo, alias, prefix)| {
                            (typo, alias, prefix, Reverse(*weight), field)
                        })
                    })
                })
                .min();
            if let Some((typo, alias, prefix, Reverse(weight), field)) = best {
                corrections += usize::from(typo);
                aliases += usize::from(alias);
                prefixes += usize::from(prefix);
                relevance += weight;
                found += 1;
                in_label &= field == 0 && !typo;
            }
        }
        if found == 0 {
            continue;
        }
        // Every word found is in the page's name, typed and not a synonym.
        let names_page = entry.names_page && in_label && aliases == 0;
        if label.join(" ") == phrase {
            relevance += 100;
        }
        if page.len() == query.len()
            && query
                .iter()
                .zip(&page)
                .all(|(word, token)| word_match(word, token).is_some())
        {
            relevance += 40;
            // A broad page query should open its common controls before its
            // update interval, even if that interval repeats the page name.
            if !entry.advanced {
                relevance += 6;
            }
        }
        let rank = (
            Reverse(found),
            // Typing a page's name, not a synonym of it, opens that page.
            !names_page,
            corrections,
            aliases,
            prefixes,
            Reverse(relevance),
            // On a tie, a hand-written result before a row title.
            index >= SEARCH.len(),
            entry.advanced,
            index,
        );
        if found == query.len() {
            complete.push((rank, entry));
        } else {
            some.push((rank, entry));
        }
    }
    // Nothing matches every word: show what matches the most of them.
    let partial = complete.is_empty();
    let mut matches = if partial { some } else { complete };
    matches.sort_unstable_by_key(|(rank, _)| *rank);
    Matches {
        partial: partial && !matches.is_empty(),
        hits: matches.into_iter().take(10).map(|(_, hit)| hit).collect(),
    }
}

fn search_words(text: &str) -> Vec<String> {
    text.split(|character: char| !character.is_alphanumeric())
        .filter(|word| !word.is_empty())
        .map(str::to_lowercase)
        .collect()
}

/// Words that say nothing about which setting someone wants.
const STOP_WORDS: &[&str] = &[
    "a", "an", "and", "are", "at", "be", "by", "can", "do", "does", "for", "from", "how", "i",
    "in", "is", "it", "its", "me", "my", "of", "on", "or", "the", "this", "to", "when", "where",
    "which", "with", "you", "your",
];

/// Phrases people type for something the screens name in other words.
const PHRASES: &[(&str, &str)] = &[
    ("time zone", "timezone"),
    ("sign in", "signin"),
    ("log in", "signin"),
    ("first run", "wizard"),
    ("start over", "wizard"),
    ("see through", "seethrough"),
    ("night mode", "dark"),
    ("dark mode", "dark"),
    ("light mode", "light"),
    ("do not disturb", "dnd"),
    ("add widget", "widgets"),
    ("add widgets", "widgets"),
    ("add a widget", "widgets"),
];

/// The query's words, without stop words unless that leaves nothing.
fn query_words(query: &str) -> Vec<String> {
    let mut text = format!(" {} ", search_words(query).join(" "));
    for (phrase, word) in PHRASES {
        text = text.replace(&format!(" {phrase} "), &format!(" {word} "));
    }
    let words = search_words(&text);
    let meaningful: Vec<String> = words
        .iter()
        .filter(|word| !STOP_WORDS.contains(&word.as_str()))
        .cloned()
        .collect();
    if meaningful.is_empty() { words } else { meaningful }
}

/// Other words for what the screens call something.
fn synonyms(word: &str) -> &'static [&'static str] {
    match word {
        "transparent" | "transparency" | "seethrough" | "translucent" => {
            &["opacity", "transparency", "glass"]
        }
        "colour" => &["color"],
        "colours" => &["colors"],
        "autostart" | "boot" | "signin" | "login" => &["startup", "sign"],
        "hotkey" | "hotkeys" | "keybinding" | "keybindings" => &["shortcuts"],
        "wallpapers" | "background" | "backdrop" => &["wallpaper"],
        "taskbar" | "launcher" => &["dock", "panel"],
        "topbar" | "menubar" | "bar" => &["panel"],
        "monitor" | "monitors" | "screen" | "screens" | "displays" | "output" => &["display"],
        "timezone" => &["timezone", "city"],
        "google" | "ics" | "ical" | "caldav" | "outlook" | "agenda" | "events" => &["calendar"],
        "bigger" | "smaller" | "larger" | "scale" | "scaling" | "zoom" => &["size"],
        "theme" | "themes" => &["style"],
        "wizard" | "onboarding" | "setup" => &["set", "wizard"],
        "left" | "right" | "top" | "bottom" | "side" => &["position"],
        "image" | "images" | "photo" | "photos" | "picture" | "pictures" => &["picture", "wallpaper"],
        "upgrade" | "upgrades" => &["updates"],
        "tiling" | "tile" => &["tile"],
        "font" | "fonts" => &["fonts", "text"],
        _ => &[],
    }
}

/// (spelling correction, synonym, prefix). Short words and technical names
/// such as CPU and RAM are never fuzzy matched.
fn word_match(word: &str, token: &str) -> Option<(bool, bool, bool)> {
    if token == word {
        return Some((false, false, false));
    }
    if token.starts_with(word) {
        return Some((false, false, true));
    }
    for alias in synonyms(word) {
        if token.starts_with(alias) {
            return Some((false, true, token != *alias));
        }
    }
    one_typo(word, token).then_some((true, false, false))
}

/// One insertion, deletion, replacement or adjacent transposition after the
/// first letter. Corrections need words of at least five letters, so short
/// words, acronyms and partly typed words never match something unrelated.
fn one_typo(left: &str, right: &str) -> bool {
    if !left.is_ascii() || !right.is_ascii() {
        return false;
    }
    let (left, right) = (left.as_bytes(), right.as_bytes());
    if left.len().min(right.len()) < 5
        || left.len().abs_diff(right.len()) > 1
        || left[0] != right[0]
    {
        return false;
    }
    let common = left.iter().zip(right).take_while(|(a, b)| a == b).count();
    match left.len().cmp(&right.len()) {
        std::cmp::Ordering::Equal => {
            common < left.len()
                && (left[common + 1..] == right[common + 1..]
                    || (common + 1 < left.len()
                        && left[common] == right[common + 1]
                        && left[common + 1] == right[common]
                        && left[common + 2..] == right[common + 2..]))
        }
        std::cmp::Ordering::Less => left[common..] == right[common + 1..],
        std::cmp::Ordering::Greater => left[common + 1..] == right[common..],
    }
}

pub fn search_id() -> cosmic::widget::Id {
    cosmic::widget::Id::new("settings-search")
}

impl Page {
    /// Breadcrumb ownership also defines Alt+Left and search-result context.
    /// Only the few detailed editors that open from a menu page go Back;
    /// every other page is in the menu itself.
    pub(super) fn parent(self) -> Option<Self> {
        match self.canonical() {
            // Flat tabs and root landings have no parent.
            Self::Overview
            | Self::Themes
            | Self::DesktopProfiles
            | Self::DesktopWallpaper
            | Self::DesktopDock
            | Self::DesktopWindows
            | Self::DesktopMotion
            | Self::General
            | Self::Changes => None,
            // Detail editors that open from rows on Appearance.
            Self::Appearance | Self::DesktopAppearance => Some(Self::Themes),
            Self::DesktopAutomation => Some(Self::DesktopProfiles),
            Self::AdaptiveLooks => Some(Self::DesktopAutomation),
            Self::PendingChanges => Some(Self::Changes),
            Self::DesktopWindowRules | Self::DesktopShortcuts => Some(Self::DesktopWindows),
            Self::DesktopIcons => Some(Self::DesktopWallpaper),
            _ => Some(Self::Overview),
        }
    }
    /// The sidebar group a page belongs to, which also says what it changes.
    pub(super) fn area(self) -> Area {
        // The menu group a page sits in.
        match self.sidebar() {
            Self::Themes => Area::Appearance,
            Self::DesktopProfiles => Area::Looks,
            Self::DesktopWallpaper => Area::Wallpaper,
            Self::DesktopDock => Area::Bars,
            Self::DesktopWindows => Area::Windows,
            Self::DesktopMotion => Area::Accessibility,
            Self::General | Self::Changes => Area::App,
            _ => Area::Widgets,
        }
    }
    pub(super) fn save_hint(self) -> &'static str {
        match self {
            Self::Overview => "Edit layout to preview changes before applying",
            Self::Themes => "Choose a style to preview it here",
            Self::DesktopWallpaper => "Choose a wallpaper to preview it here",
            Self::DesktopProfiles => "Choose a look to see what it includes",
            Self::Notes => "",
            _ => "",
        }
    }
}

pub fn latest_desktop_change() -> Option<(crate::desktop::Owner, String, String)> {
    latest_desktop_record().map(|change| (change.owner, change.label, change.id))
}
fn latest_desktop_record() -> Option<crate::desktop::store::LatestChange> {
    crate::desktop::actions::history_store()
        .ok()?
        .latest_change()
        .ok()?
}

impl Settings {
    /// Re-reads the change Undo targets, with the summary Changes shows for it.
    pub(super) fn refresh_latest_change(&mut self) {
        let latest = latest_desktop_record();
        self.last_desktop_summary = latest.as_ref().map(|change| change.summary());
        self.last_desktop_change = latest.map(|change| (change.owner, change.label, change.id));
    }
    pub(super) fn advanced_open(&self, page: Page) -> bool {
        self.advanced_pages.contains(&page)
    }
    #[cfg(test)]
    pub(super) fn pending_pages(&self) -> Vec<Page> {
        [
            (Page::WidgetLayout, self.placement.dirty()),
            (
                Page::DesktopProfiles,
                self.profiles.session_draft().is_some() || !self.profiles.parked().is_empty(),
            ),
            (Page::Notes, self.tools.dirty()),
            (Page::DesktopDock, self.desktop.dirty()),
            (Page::DesktopAppearance, self.system_appearance.dirty()),
            (Page::DesktopWallpaper, self.wallpaper.dirty()),
            (Page::DesktopIcons, self.desktop_icons.dirty()),
            (Page::DesktopWindows, self.windows.dirty()),
            (Page::DesktopWorkspaces, self.workspaces.dirty()),
            (
                Page::DesktopWindowRules,
                self.window_rules.session_draft().is_some(),
            ),
            (
                Page::DesktopShortcuts,
                self.shortcuts.session_draft().is_some(),
            ),
            (Page::DesktopAutomation, self.automation.dirty()),
        ]
        .into_iter()
        .filter_map(|(page, dirty)| dirty.then_some(page))
        .collect()
    }
    /// Undo exactly the action the user saw; a newer unseen action requires
    /// refreshing the label before it can be reversed.
    pub(super) fn undo_desktop_change(&mut self) -> Task<Message> {
        if !self.appearance_pending.is_empty() || self.notice_editing {
            let revision = crate::desktop::store::revision();
            let baseline = self.gesture_config.take();
            crate::desktop::actions::guarded(baseline.as_ref(), || self.flush_edits());
            if self.error {
                return Task::none();
            }
            if crate::desktop::store::revision() != revision {
                self.refresh_latest_change();
            }
        }
        let shown = self
            .last_desktop_change
            .as_ref()
            .map(|(_, _, id)| id.clone());
        self.undo_shown_change(shown)
    }
    /// The caller carries the original request through any queue delay.
    pub(super) fn undo_shown_change(&mut self, shown: Option<String>) -> Task<Message> {
        if self.native_worker_needed() {
            return self.background_undo(shown, None);
        }
        let undone = shown.clone();
        let result = crate::desktop::actions::history_store().and_then(|store| {
            let id = shown.ok_or_else(|| {
                crate::desktop::DesktopError::UndoUnavailable("No settings action to undo".into())
            })?;
            let result = store.undo_latest(&id);
            self.undo_review = if matches!(
                result,
                Err(crate::desktop::DesktopError::Conflict(_)
                    | crate::desktop::DesktopError::FieldConflict { .. })
            ) {
                store.review_undo(&id).ok()
            } else {
                None
            };
            result
        });
        match result {
            Ok(_) => {
                self.status.clear();
                self.error = false;
                if let Some(id) = &undone {
                    self.profiles.forget_applied(id);
                }
                #[cfg(test)]
                {
                    self.preference_undo = None;
                }
            }
            Err(error) => self.failed(error.to_string()),
        }
        self.refresh_latest_change();
        self.after_desktop_change()
    }
    /// Puts back one older change from Changes. A setting changed since
    /// stops it and opens the review, which keeps those values.
    pub(super) fn put_back_change(&mut self, id: String) -> Task<Message> {
        if self.native_worker_needed() {
            return self.background_put_back(id);
        }
        let result = crate::desktop::actions::history_store().and_then(|store| {
            let result = store.put_back(&id);
            self.undo_review = if matches!(
                result,
                Err(crate::desktop::DesktopError::Conflict(_)
                    | crate::desktop::DesktopError::FieldConflict { .. })
            ) {
                store.review_put_back(&id).ok()
            } else {
                None
            };
            result
        });
        match result {
            Ok(_) => {
                self.status.clear();
                self.error = false;
                self.profiles.forget_applied(&id);
                self.wizard_undo_completed(Some(id));
            }
            Err(error) => self.failed(error.to_string()),
        }
        self.refresh_latest_change();
        self.after_desktop_change()
    }
    pub(super) fn resolve_undo_review(
        &mut self,
        review: crate::desktop::store::UndoReview,
        restore: bool,
    ) -> Task<Message> {
        if self.native_worker_needed() {
            return self.background_undo(None, Some((review, restore)));
        }
        let result = crate::desktop::actions::history_store().and_then(|store| {
            if restore {
                store.accept_undo_review(&review)
            } else {
                store.keep_undo_current(&review)
            }
        });
        match result {
            Ok(_) => {
                if restore {
                    self.profiles.forget_applied(review.id());
                    self.wizard_undo_completed(Some(review.id().to_string()));
                }
                self.undo_review = None;
                self.error = false;
                self.status.clear();
            }
            Err(error) => self.failed(error.to_string()),
        }
        self.refresh_latest_change();
        self.after_desktop_change()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use cosmic::Application;
    #[test]
    fn architecture_search_routing_survives_label_changes() {
        if crate::displays::test_support::isolated(
            "settings::navigation::tests::architecture_search_routing_survives_label_changes",
        ) {
            return;
        }
        let (mut app, _) = Settings::init(cosmic::app::Core::default(), Page::Overview);
        for original in SEARCH {
            let renamed = SearchHit {
                label: "Renamed control",
                ..*original
            };
            assert_eq!(renamed.anchor(), original.anchor());
            assert_eq!(renamed.app_destination(), original.app_destination());
            let _ = app.handle(Message::SearchResult(renamed));
            assert_eq!(app.page, Page::parse(renamed.page.key()));
            if renamed.page == Page::General {
                assert_eq!(app.app_settings_page, renamed.app_destination());
            }
            match renamed.open {
                SearchOpen::City => assert!(app.city_editing),
                SearchOpen::Effects => assert!(app.system_appearance.effects),
                SearchOpen::Readability | SearchOpen::Transparency => {
                    assert!(app.material_adjust);
                    assert_eq!(app.material_target, crate::materials::Target::Widgets);
                }
                SearchOpen::Metric(metric) => assert!(app.devbox_details.contains(&metric)),
                SearchOpen::None => {}
            }
        }
    }

    #[test]
    fn every_route_has_one_main_home_and_old_links_still_work() {
        let entries: Vec<Page> = Page::SIDEBAR
            .iter()
            .flat_map(|(_, pages)| pages.iter().copied())
            .collect();
        for page in Page::ALL {
            // Every page lights exactly one menu entry.
            assert!(
                entries.contains(&page.sidebar()) || page.sidebar() == Page::PINNED,
                "{page:?}"
            );
            let destination = match page {
                Page::WidgetLayout | Page::Widgets | Page::Desktop => Page::Overview,
                Page::DesktopWorkspaces => Page::DesktopWindows,
                // Adaptive looks are chosen on the time of day page.
                Page::AdaptiveLooks => Page::DesktopAutomation,
                page => page,
            };
            assert_eq!(Page::parse(page.key()), destination);
            // Back never leaves a page's own sidebar entry for another group.
            if let Some(parent) = page.parent() {
                assert_eq!(parent.area(), page.area(), "{page:?}");
            }
        }
        assert_eq!(Page::parse("layout"), Page::Overview);
        assert_eq!(Page::parse("devbox"), Page::Devbox);
        assert_eq!(Page::parse("presets"), Page::DesktopProfiles);
        assert_eq!(Page::parse("home"), Page::Overview);
        assert_eq!(Page::parse("desktop"), Page::Overview);
        assert_eq!(Page::parse("widgets"), Page::Overview);
        assert_eq!(Page::parse("dock"), Page::DesktopDock);
        assert_eq!(Page::parse("panel"), Page::DesktopDock);
        assert_eq!(Page::parse("settings"), Page::General);
        assert_eq!(Page::parse("adaptive"), Page::DesktopAutomation);
        assert_eq!(Page::Calendar.parent(), Some(Page::Overview));
        assert_eq!(Page::DesktopDock.parent(), None);
        assert_eq!(Page::DesktopWorkspaces.sidebar(), Page::DesktopWindows);
        assert_eq!(Page::Moments.sidebar(), Page::DesktopProfiles);
        assert_eq!(
            entries,
            [
                Page::DesktopProfiles,
                Page::Themes,
                Page::DesktopWallpaper,
                Page::Overview,
                Page::DesktopDock,
                Page::DesktopWindows,
                Page::DesktopMotion,
                Page::General
            ]
        );
        // Every root is its own destination and has no parent.
        for page in entries {
            assert_eq!(page.sidebar(), page, "{page:?}");
            assert_eq!(page.parent(), None, "{page:?}");
        }
        // Roots have no parent; detailed editors go Back to their root.
        assert_eq!(Page::DesktopAppearance.parent(), Some(Page::Themes));
        assert_eq!(Page::Moments.parent(), Some(Page::DesktopProfiles));
        assert_eq!(Page::DesktopMotion.parent(), None);
        assert_eq!(
            Page::DesktopWindowRules.parent(),
            Some(Page::DesktopWindows)
        );
    }
    #[test]
    fn task_search_finds_fields_and_opens_advanced_only_when_needed() {
        assert_eq!(search("CITY")[0].page, Page::Weather);
        assert_eq!(search("gpu")[0].page, Page::Devbox);
        assert_eq!(search("calendar update")[0].page, Page::Calendar);
        assert!(search("calendar update")[0].advanced);
        assert!(search("  ").is_empty());
        assert!(search("nonexistentthing").is_empty());
    }

    #[test]
    fn search_ranks_specific_controls_and_common_page_options_first() {
        assert_eq!(search("memory usage")[0].label, "Memory");
        // A page’s own name opens the page, ahead of the rows on it.
        for (query, page) in [
            ("calendar", Page::Calendar),
            ("weather", Page::Weather),
            ("clock", Page::Clock),
            ("time", Page::DesktopAutomation),
        ] {
            let hit = search(query)[0];
            assert_eq!(hit.page, page, "{query}");
            assert!(hit.names_page, "{query}");
        }
        assert_eq!(search("weather location")[0].label, "Location");
        assert_eq!(search("clock time")[0].label, "Time and date");
        let update = search("calendar update")[0];
        assert_eq!((update.page, update.label), (Page::Calendar, "Update every"));
        let colors = search("colors");
        assert_eq!(colors[0].label, "Use these colors in apps");
        assert!(colors.iter().any(|hit| hit.label == "Accent color"));
        assert!(colors.iter().any(|hit| hit.page == Page::DesktopWallpaper));
        assert_eq!(colors, search("colors"));
    }

    #[test]
    fn search_accepts_common_typos_without_fuzzy_acronyms_or_unrelated_words() {
        for query in ["calender", "calndar", "calendarr", "clalendar", "claendar"] {
            assert_eq!(search(query)[0].page, Page::Calendar, "{query}");
        }
        assert_eq!(search("netwrok")[0].label, "Network");
        let update = search("calender updat")[0];
        assert_eq!((update.page, update.label), (Page::Calendar, "Update every"));
        for query in [
            "cpx",
            "gpx",
            "zxyw",
            "caaaalendar",
            "---",
            "🪴",
        ] {
            assert!(search(query).is_empty(), "{query}");
        }
        // A word that matches nothing leaves the others to rank alone.
        let partial = find("calendar banana");
        assert!(partial.partial);
        assert_eq!(partial.hits[0].page, Page::Calendar);
    }

    #[test]
    fn search_synonyms_reach_existing_controls_and_keep_their_anchors() {
        for query in ["transparent", "transparency", "widget transparent"] {
            let hit = search(query)[0];
            assert_eq!(hit.page, Page::Appearance, "{query}");
            assert!(!hit.advanced);
            // Straight to the widget material's see-through control.
            assert_eq!(hit, SEE_THROUGH, "{query}");
        }
        for query in [
            "colours",
            "accent",
            "palette",
            "wallpaper colors",
            "widget colors",
        ] {
            let hit = search(query)[0];
            assert!(
                matches!(hit.page, Page::DesktopWallpaper | Page::Themes),
                "{query}"
            );
            assert!(!hit.advanced);
        }
        assert_eq!(search("autostart")[0].label, "Running and startup");
        assert_eq!(search("hotkeys")[0].page, Page::DesktopShortcuts);
        let hit = search("memory cache")[0];
        assert_eq!(hit.anchor(), "devbox-memory");
        let hit = search("dock spacing")[0];
        assert!(hit.advanced);
        assert_eq!(hit.anchor(), "dock-advanced");
        let hit = search("calendar refresh")[0];
        assert!(hit.advanced);
        assert_eq!(hit.anchor(), "settings-advanced");
    }

    #[test]
    fn search_handles_punctuation_and_multiple_words_without_dropping_terms() {
        assert_eq!(search("GPU-readings")[0].label, "Graphics (GPU)");
        assert_eq!(search("  MEMORY / cache  ")[0].label, "Memory");
        assert_eq!(
            search("calendar update"),
            search("calendar calendar update")
        );
        // Builder now supports both Calendar and GPU parts; use an actually
        // unsupported second term to verify that search retains every word.
        assert!(find("calendar bluetooth").partial);
        assert!(search("calendar agenda").iter().any(|hit| hit.page == Page::Builder));
        assert_eq!(search("gpu recipe")[0].page, Page::Builder);
        assert!(find("calendar mystery option").partial);
        assert!(!find("calendar update").partial);
        assert!(search("desktop").len() <= 10);
    }

    // IA-01: what people type, including the exact names of rows they can
    // see, reaches the page that has it.
    #[test]
    fn search_finds_what_people_type_on_the_right_page() {
        use crate::desktop::panels::PanelKind;
        for (query, page, destination) in [
            ("Text size", Page::DesktopAppearance, None),
            ("font size", Page::DesktopAppearance, None),
            ("bigger text", Page::DesktopAppearance, None),
            ("Show the date", Page::Clock, None),
            ("Picture size", Page::DesktopWallpaper, None),
            ("Start when you sign in", Page::General, Some(AppSettingsPage::Operation)),
            ("Set up again", Page::General, Some(AppSettingsPage::Operation)),
            ("setup wizard", Page::General, Some(AppSettingsPage::Operation)),
            ("onboarding", Page::General, Some(AppSettingsPage::Operation)),
            ("start over", Page::General, Some(AppSettingsPage::Operation)),
            ("first run", Page::General, Some(AppSettingsPage::Operation)),
            ("check for updates", Page::General, Some(AppSettingsPage::Updates)),
            ("update", Page::General, Some(AppSettingsPage::Updates)),
            ("add widget", Page::Overview, None),
            ("time zone", Page::DesktopAutomation, None),
            ("schedule", Page::DesktopAutomation, None),
            ("taskbar", Page::DesktopDock, None),
            ("second monitor", Page::Overview, None),
            ("dock on left", Page::DesktopDock, None),
            ("dock transparency", Page::DesktopDock, None),
        ] {
            let found = find(query);
            assert!(!found.partial, "{query}");
            let hit = found.hits[0];
            assert_eq!(hit.page.canonical(), page, "{query}: {hit:?}");
            if let Some(destination) = destination {
                assert_eq!(hit.app_destination(), destination, "{query}: {hit:?}");
            }
        }
        assert_eq!(search("dock on left")[0].bar, Some(PanelKind::Dock));
        assert_eq!(search("dock transparency")[0].label, "See-through");
        // Stop words never decide a result, and a typo needs five letters.
        assert_eq!(search("the dock"), search("dock"));
        assert!(search("clok").is_empty());
        assert!(
            search("icloud").iter().all(|hit| hit.label != "Moving effect"),
            "{:?}",
            search("icloud")
        );
    }

    #[test]
    fn layout_queries_reach_the_home_editor() {
        for query in ["layout", "arrange widgets", "resize widgets", "grid snap"] {
            let hit = search(query)[0];
            assert_eq!(hit.page, Page::Overview, "{query}");
            assert_eq!(hit.anchor(), "canvas-start", "{query}");
            assert!(!hit.advanced);
        }
    }

    #[test]
    fn history_queries_reach_the_metric_details_instead_of_update_interval() {
        for (query, anchor) in [
            ("cpu history", "devbox-cpu"),
            ("memory graph", "devbox-memory"),
            ("disk sparkline", "devbox-disk"),
            ("network history", "devbox-network"),
            ("gpu history", "devbox-gpu"),
            ("swap", "devbox-memory"),
        ] {
            let hit = search(query)[0];
            assert_eq!(hit.page, Page::Devbox, "{query}");
            assert_eq!(hit.anchor(), anchor, "{query}");
            assert!(!hit.advanced, "{query}");
        }
        // Memory and Disk show bars, so only CPU, Network and GPU have a
        // history graph to find.
        let hits = search("history");
        assert_eq!(hits.len(), 4);
        assert_eq!(
            hits.iter()
                .filter(|hit| hit.page == Page::Devbox && !hit.advanced)
                .count(),
            3
        );
        assert!(
            hits.iter()
                .any(|hit| hit.page == Page::Changes && hit.label == "Changes")
        );
    }
}

/// The same navigation component used by COSMIC Settings.
pub(super) fn model(page: Page) -> cosmic::widget::nav_bar::Model {
    let mut model = cosmic::widget::nav_bar::Model::default();
    for destination in Page::SIDEBAR
        .iter()
        .flat_map(|(_, pages)| pages.iter())
        .copied()
    {
        model
            .insert()
            .text(destination.root_title())
            .icon(cosmic::widget::icon::from_name(destination.nav_icon()).size(18))
            .data(destination);
    }
    select(&mut model, page);
    model
}
pub(super) fn select(model: &mut cosmic::widget::nav_bar::Model, page: Page) {
    let id = model
        .iter()
        .find(|&id| model.data::<Page>(id) == Some(&page.sidebar()));
    if let Some(id) = id {
        model.activate(id);
    }
}

fn remembered_app_destination() -> Option<String> {
    crate::desktop::DesktopStore::from_env().ok().and_then(|store| {
        std::fs::read_to_string(store.state_dir().join("settings-app-destination")).ok()
    })
}
/// Settings was last left on the former Restore section, now Changes.
pub(super) fn left_in_restore() -> bool {
    remembered_app_destination().is_some_and(|key| key.trim() == "restore")
}
pub(super) fn app_destination() -> AppSettingsPage {
    remembered_app_destination()
        .and_then(|key| {
            AppSettingsPage::ALL
                .into_iter()
                .find(|page| page.key() == key.trim())
        })
        .unwrap_or_default()
}
pub(super) fn remember_app_destination(page: AppSettingsPage) {
    if super::capture_dir().is_some() && !super::preview() {
        return;
    }
    if let Ok(store) = crate::desktop::DesktopStore::from_env()
        && let Ok(path) = store.state_file("settings-app-destination")
    {
        if std::fs::create_dir_all(store.state_dir()).is_ok() {
            let _ = crate::config::write_config_text(&path, Some(page.key()));
        }
    }
}

/// Traverse focus and then reveal it, in order; no pointer scrolling required.
pub(super) fn keyboard_focus(backwards: bool) -> Task<Message> {
    let focus = if backwards {
        cosmic::iced::widget::operation::focus_previous()
    } else {
        cosmic::iced::widget::operation::focus_next()
    };
    focus.chain(cosmic::iced::runtime::task::effect(
        cosmic::iced::runtime::Action::widget(FocusReveal::default()),
    ))
}
pub(super) type FocusReveal = Reveal;

/// Each object has one editor with these three tabs.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ObjectTab {
    #[default]
    Content,
    Design,
    Position,
}
impl ObjectTab {
    pub(super) const ALL: [Self; 3] = [Self::Content, Self::Design, Self::Position];
    pub(super) fn title(self) -> &'static str {
        match self {
            Self::Content => "Content",
            Self::Design => "Design",
            Self::Position => "Position",
        }
    }
}

/// One place Back can return to, with what made it that place.
#[derive(Clone, Debug, PartialEq)]
pub(super) struct Route {
    page: Page,
    app: AppSettingsPage,
    tab: ObjectTab,
    query: String,
    search_open: bool,
    scroll: f32,
    display: Option<String>,
    bar: crate::desktop::panels::PanelKind,
    arranging: bool,
    saved_collection: bool,
    setup_review: Option<super::profiles::Selection>,
    report: Option<String>,
}
const MAX_HISTORY: usize = 40;

/// A retained preview; resuming returns to the screen that owns it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum KeptTarget {
    Wallpaper,
    Colors,
    Setup(super::profiles::Selection),
    Adaptive,
    Placement,
    Canvas,
    Note,
    Page(Page),
    SetupChoices,
}

/// One row of the “Not applied” list.
#[derive(Clone, Debug)]
pub(super) struct Kept {
    pub title: String,
    pub root: Page,
    pub conflict: bool,
    pub target: KeptTarget,
}

/// What Undo and Ctrl+Z act on here. They never disagree.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum UndoTarget {
    Note,
    Preview(Option<&'static str>),
    Canvas,
    Desktop(String, String),
    None,
}

pub(super) fn preview_kind(page: Page) -> Option<super::profiles::previews::PreviewKind> {
    use super::profiles::previews::PreviewKind;
    match page {
        Page::Themes => Some(PreviewKind::Theme),
        Page::DesktopProfiles => Some(PreviewKind::Setup),
        Page::AdaptiveLooks => Some(PreviewKind::Adaptive),
        _ => None,
    }
}

pub(super) fn widget_of(page: Page) -> Option<crate::drawer::Widget> {
    use crate::drawer::Widget;
    Some(match page {
        Page::Glance => Widget::Glance,
        Page::Clock => Widget::Clock,
        Page::Weather => Widget::Weather,
        Page::Calendar => Widget::Calendar,
        Page::Devbox => Widget::Devbox,
        Page::Projects => Widget::Projects,
        Page::Music => Widget::Music,
        Page::Notes => Widget::Notes,
        Page::Focus => Widget::Focus,
        _ => return None,
    })
}
pub(super) fn widget_page(widget: crate::drawer::Widget) -> Page {
    use crate::drawer::Widget;
    match widget {
        Widget::Glance => Page::Glance,
        Widget::Clock => Page::Clock,
        Widget::Weather => Page::Weather,
        Widget::Calendar => Page::Calendar,
        Widget::Devbox => Page::Devbox,
        Widget::Projects => Page::Projects,
        Widget::Music => Page::Music,
        Widget::Notes => Page::Notes,
        Widget::Focus => Page::Focus,
    }
}

impl Settings {
    pub(super) fn canvas_visible(&self) -> bool {
        self.canvas.is_some()
            && ((self.page == Page::Overview && self.arranging)
                || (widget_of(self.page).is_some() && self.object_tab == ObjectTab::Position))
    }
    fn route(&self) -> Route {
        Route {
            page: self.page,
            app: self.app_settings_page,
            tab: self.object_tab,
            query: self.search_query.clone(),
            search_open: self.search_open,
            scroll: self.page_scroll.get(self.page.key()).copied().unwrap_or(0.),
            display: self.placement.display.clone(),
            bar: self.desktop.selected,
            arranging: self.arranging,
            saved_collection: self.profiles.saved_collection,
            setup_review: (self.page == Page::DesktopProfiles && self.profiles.review_open())
                .then(|| self.profiles.selection().clone()),
            report: self.support.report.clone(),
        }
    }
    pub(super) fn remember_route(&mut self) {
        let route = self.route();
        if self.history.last() != Some(&route) {
            self.history.push(route);
            if self.history.len() > MAX_HISTORY {
                self.history.remove(0);
            }
        }
    }
    /// The header's one way up: from search results back to them, from a
    /// view opened inside a page back to that page, and from a subpage to
    /// its parent. Sidebar pages have none. It never names an unrelated page.
    pub(super) fn up_link(&self) -> Option<(String, Message)> {
        if !self.search_query.trim().is_empty() {
            return None;
        }
        let last = self.history.last();
        if last.is_some_and(|route| route.search_open || !route.query.trim().is_empty()) {
            return Some(("Search results".into(), Message::Back));
        }
        if self.page == Page::PendingChanges && !self.history.last().is_some_and(|r| r.page != Page::Changes) {
            let back = self.history.last().is_some_and(|r| r.page == Page::Changes);
            return Some((
                Page::Changes.nav_title().into(),
                if back { Message::Back } else { Message::Page(Page::Changes) },
            ));
        }
        if self.page == Page::Overview && self.canvas_visible() {
            // The link stays while a drag or save is running (doing nothing
            // then), so the canvas never shifts under the pointer.
            return Some((
                Page::Overview.nav_title().into(),
                self.canvas_done().unwrap_or(Message::Preview),
            ));
        }
        // Release notes belong to Updates, where they are opened.
        if self.page == Page::General && self.app_settings_page == AppSettingsPage::WhatsNew {
            let updates = AppSettingsPage::Updates;
            let back = last.is_some_and(|r| r.page == Page::General && r.app == updates);
            return Some((
                updates.tab_title().into(),
                if back {
                    Message::Back
                } else {
                    Message::AppSettingsPage(updates)
                },
            ));
        }
        let inside = match self.page {
            Page::General
                if self.support.report.is_some()
                    || self.app_settings_page != AppSettingsPage::Home =>
            {
                Some(Page::General)
            }
            Page::DesktopProfiles
                if self.profiles.package.is_some()
                    || self.profiles.mix.is_some()
                    || self.profiles.review_open() =>
            {
                Some(Page::DesktopProfiles)
            }
            _ => None,
        };
        if let Some(page) = inside {
            let back = self.history.is_empty() || last.is_some_and(|route| route.page == page);
            return back.then(|| (page.nav_title().into(), Message::Back));
        }
        let parent = self.page.parent()?;
        let back = last.is_some_and(|route| route.page == parent);
        Some((
            parent.nav_title().into(),
            if back {
                Message::Back
            } else {
                Message::Page(parent)
            },
        ))
    }
    /// The destination Back names, or the parent when nothing came before.
    pub(super) fn back_title(&self) -> Option<String> {
        if !self.search_query.trim().is_empty() {
            return None;
        }
        match self.history.last() {
            Some(route) if route.search_open || !route.query.trim().is_empty() => {
                Some("Search results".into())
            }
            Some(route) => Some(self.route_title(route)),
            None if self.page == Page::DesktopProfiles && self.profiles.review_open() => {
                Some("Looks".into())
            }
            None => self.page.parent().map(|parent| parent.nav_title().into()),
        }
    }
    fn route_title(&self, route: &Route) -> String {
        match route.page {
            Page::General if route.report.is_some() => "Support report".into(),
            Page::General => match route.app {
                AppSettingsPage::Home => "Halcyon".into(),
                page => page.tab_title().into(),
            },
            Page::DesktopDock => match route.bar {
                crate::desktop::panels::PanelKind::Dock => "Dock".into(),
                crate::desktop::panels::PanelKind::Panel => "Panel".into(),
            },
            page => page.nav_title().into(),
        }
    }
    pub(super) fn go_back(&mut self) -> Task<Message> {
        // Back completes a gesture just like leaving does.
        self.flush_edits();
        let Some(route) = self.history.pop() else {
            return match self.page.parent() {
                Some(parent) => {
                    self.quiet_navigation = true;
                    let task = self.handle(Message::Page(parent));
                    self.quiet_navigation = false;
                    task
                }
                None if self.page == Page::General
                    && self.app_settings_page != AppSettingsPage::Home =>
                {
                    self.app_settings_page = AppSettingsPage::Home;
                    self.support.report = None;
                    Task::none()
                }
                None => Task::none(),
            };
        };
        // Page also schedules a scroll restoration. Give both operations the
        // same saved offset so unordered task completion cannot restore a newer one.
        self.page_scroll
            .insert(route.page.key().into(), route.scroll);
        self.quiet_navigation = true;
        let task = self.handle(Message::Page(route.page));
        self.quiet_navigation = false;
        self.app_settings_page = route.app;
        self.object_tab = route.tab;
        self.arranging = route.arranging && self.canvas.is_some();
        self.support.report = route.report;
        if route.page == Page::DesktopProfiles {
            if let Some(selection) = route.setup_review {
                if self.profiles.selection() != &selection || !self.profiles.review_open() {
                    self.profiles
                        .update(super::profiles::Message::Select(selection));
                }
            } else {
                let _ = self.leave_review_to_browse();
            }
        }
        if route.display != self.placement.display
            && let Some(display) = route.display
        {
            self.select_monitor_preview(display);
        }
        if route.page == Page::DesktopDock && route.bar != self.desktop.selected {
            let _ = self
                .desktop
                .update(super::desktop::Message::Select(route.bar));
        }
        self.search_query = route.query;
        self.search_open = route.search_open;
        self.profiles.saved_collection = route.saved_collection;
        self.page_scroll
            .insert(route.page.key().into(), route.scroll);
        Task::batch([
            task,
            cosmic::iced::widget::scrollable::scroll_to(
                cosmic::widget::Id::new("settings-content"),
                cosmic::iced::widget::scrollable::AbsoluteOffset {
                    x: None,
                    y: Some(route.scroll),
                },
            ),
        ])
    }
    /// A root click always opens that root's stable landing, from the top.
    pub(super) fn open_root(&mut self, root: Page) -> Task<Message> {
        let same = self.page == root && !self.arranging;
        if root == Page::DesktopProfiles && self.page == root {
            let _ = self.leave_review_to_browse();
        }
        if root == Page::General
            && self.page == root
            && self.app_settings_page != AppSettingsPage::Home
        {
            self.remember_route();
        }
        if !same {
            self.page_scroll.remove(root.key());
        }
        let task = self.handle(Message::Page(root));
        self.arranging = false;
        self.search_selected = None;
        if root == Page::General {
            self.app_settings_page = AppSettingsPage::Home;
            self.support.report = None;
        }
        if root == Page::DesktopProfiles {
            let _ = self.leave_review_to_browse();
            self.profiles.saved_collection = false;
        }
        self.page_scroll.remove(root.key());
        Task::batch([
            task,
            cosmic::iced::widget::scrollable::snap_to(
                cosmic::widget::Id::new("settings-content"),
                cosmic::iced::widget::scrollable::RelativeOffset {
                    x: None,
                    y: Some(0.),
                },
            ),
        ])
    }
    /// Says what leaving keeps, and parks the shared review engine.
    pub(super) fn note_leaving(&mut self, next: Page) {
        self.kept_note = None;
        if let Some(kind) = preview_kind(self.page)
            && preview_kind(next) != Some(kind)
        {
            let selection = self.profiles.selection().clone();
            if let Some(title) = self.profiles.park(kind) {
                use super::profiles::previews::PreviewKind;
                let target = match kind {
                    PreviewKind::Theme => KeptTarget::Colors,
                    PreviewKind::Setup => KeptTarget::Setup(selection),
                    PreviewKind::Adaptive => KeptTarget::Adaptive,
                };
                self.kept_note = Some((format!("{title} kept. Not applied."), target));
            }
        }
        if self.page == Page::DesktopWallpaper && self.wallpaper.dirty() {
            self.kept_note = Some((
                "Wallpaper preview kept. Not applied.".into(),
                KeptTarget::Wallpaper,
            ));
        }
        if self.canvas_visible() && self.canvas_dirty() {
            self.kept_note = Some((
                "Widget placement preview kept. Not applied.".into(),
                KeptTarget::Canvas,
            ));
        }
        if self.page == Page::Notes && self.tools.unsaved() {
            self.kept_note = Some(("Note kept. Not saved yet.".into(), KeptTarget::Note));
        }
        if self.page == Page::Builder && self.recipe_editor.dirty() {
            self.kept_note = Some((
                "Widget design preview kept. Not applied.".into(),
                KeptTarget::Page(Page::Builder),
            ));
        }
    }
    /// Leaves a setup review for the list. Back stays on the Setups screen.
    pub(super) fn leave_review_to_browse(&mut self) -> Task<Message> {
        use super::profiles::previews::PreviewKind;
        let selection = self.profiles.selection().clone();
        self.kept_note = self.profiles.park(PreviewKind::Setup).map(|title| {
            (
                format!("{title} kept. Not applied."),
                KeptTarget::Setup(selection),
            )
        });
        Task::none()
    }
    pub(super) fn resume_kept(&mut self, target: KeptTarget) -> Task<Message> {
        self.kept_note = None;
        match target {
            KeptTarget::Wallpaper => self.handle(Message::Page(Page::DesktopWallpaper)),
            KeptTarget::Colors => self.handle(Message::Page(Page::Themes)),
            KeptTarget::Adaptive => self.handle(Message::Page(Page::DesktopAutomation)),
            KeptTarget::Setup(selection) => {
                if self.page == Page::DesktopProfiles && self.profiles.selection() != &selection {
                    let _ = self.leave_review_to_browse();
                }
                let task = self.handle(Message::Page(Page::DesktopProfiles));
                if self.profiles.selection() != &selection {
                    self.profiles.resume_kept_selection(&selection);
                }
                task
            }
            KeptTarget::Canvas | KeptTarget::Placement => {
                let task = self.handle(Message::Page(Page::Overview));
                self.arranging = self.canvas.is_some();
                task
            }
            KeptTarget::Note => self.handle(Message::Page(Page::Notes)),
            KeptTarget::Page(page) => self.handle(Message::Page(page)),
            KeptTarget::SetupChoices => {
                self.setup_kept = false;
                self.wizard = Some(super::open_wizard());
                Task::none()
            }
        }
    }
    /// Everything that is waiting. Leaving never applies or discards these.
    pub(super) fn kept_previews(&self) -> Vec<Kept> {
        use super::profiles::previews::PreviewKind;
        let mut items = Vec::new();
        let mut push = |title: &str, root, conflict, target| {
            items.push(Kept {
                title: title.into(),
                root,
                conflict,
                target,
            });
        };
        if self.wallpaper.dirty() {
            push(
                "Wallpaper",
                Page::Overview,
                self.wallpaper.effect_conflict(),
                KeptTarget::Wallpaper,
            );
        }
        if self.canvas_dirty() {
            push(
                "Widget placement",
                Page::Overview,
                false,
                KeptTarget::Canvas,
            );
        } else if self.placement.dirty() {
            push(
                "Widget placement",
                Page::Overview,
                false,
                KeptTarget::Placement,
            );
        }
        if self.tools.unsaved() {
            push("Unsaved note", Page::Overview, false, KeptTarget::Note);
        }
        if self.recipe_editor.dirty() {
            push(
                "Widget design",
                Page::Overview,
                false,
                KeptTarget::Page(Page::Builder),
            );
        }
        // The review on screen counts too, so the list does not jump on leaving.
        if preview_kind(self.page).is_some() && self.profiles.keeps_intent(self.profiles.owner()) {
            let (title, root, target) = match self.profiles.owner() {
                PreviewKind::Theme => ("Appearance", Page::Themes, KeptTarget::Colors),
                PreviewKind::Setup => (
                    self.profiles.profile().map_or("Look", |p| p.name.as_str()),
                    Page::DesktopProfiles,
                    KeptTarget::Setup(self.profiles.selection().clone()),
                ),
                PreviewKind::Adaptive => {
                    ("Adaptive look", Page::DesktopProfiles, KeptTarget::Adaptive)
                }
            };
            push(title, root, !self.profiles.conflicts().is_empty(), target);
        }
        for parked in self.profiles.parked() {
            let (title, root, target) = match parked.kind {
                PreviewKind::Theme => ("Appearance", Page::Themes, KeptTarget::Colors),
                PreviewKind::Setup => (
                    parked.title.as_str(),
                    Page::DesktopProfiles,
                    KeptTarget::Setup(parked.selection().clone()),
                ),
                PreviewKind::Adaptive => {
                    ("Adaptive look", Page::DesktopProfiles, KeptTarget::Adaptive)
                }
            };
            push(title, root, parked.conflicted(), target);
        }
        if self.setup_kept {
            push(
                "Setup choices",
                Page::General,
                false,
                KeptTarget::SetupChoices,
            );
        }
        if self.system_appearance.dirty() {
            push(
                "Fonts and icons",
                Page::Themes,
                false,
                KeptTarget::Page(Page::DesktopAppearance),
            );
        }
        if self.automation.dirty() {
            push(
                Page::DesktopAutomation.title(),
                Page::DesktopProfiles,
                false,
                KeptTarget::Page(Page::DesktopAutomation),
            );
        }
        if self.window_rules.dirty() {
            push(
                Page::DesktopWindowRules.title(),
                Page::Overview,
                false,
                KeptTarget::Page(Page::DesktopWindowRules),
            );
        }
        if self.shortcuts.dirty() {
            push(
                "Shortcuts",
                Page::Overview,
                false,
                KeptTarget::Page(Page::DesktopShortcuts),
            );
        }
        if self.desktop.dirty() {
            push(
                "Dock and panel",
                Page::Overview,
                false,
                KeptTarget::Page(Page::DesktopDock),
            );
        }
        if self.desktop_icons.dirty() {
            push(
                Page::DesktopIcons.title(),
                Page::Overview,
                false,
                KeptTarget::Page(Page::DesktopIcons),
            );
        }
        if self.windows.dirty() || self.workspaces.dirty() {
            push(
                "Windows and workspaces",
                Page::Overview,
                false,
                KeptTarget::Page(Page::DesktopWindows),
            );
        }
        items
    }
    /// The one Undo this screen offers. Ctrl+Z and the button both use it.
    pub(super) fn undo_target(&self) -> UndoTarget {
        if self.page == Page::Notes && self.object_tab == ObjectTab::Content {
            return UndoTarget::Note;
        }
        if self.page == Page::Builder {
            return UndoTarget::Preview(
                self.recipe_editor
                    .can_undo_step()
                    .then_some("widget design edit"),
            );
        }
        if self.page == Page::DesktopWallpaper && self.wallpaper.preview_open() {
            return UndoTarget::Preview(self.wallpaper.can_undo_step().then_some("wallpaper edit"));
        }
        if preview_kind(self.page).is_some() && self.profiles.review_open() {
            return UndoTarget::Preview(self.profiles.step_label());
        }
        if self.canvas_visible() {
            return UndoTarget::Canvas;
        }
        match &self.last_desktop_change {
            Some((_, label, id)) => UndoTarget::Desktop(id.clone(), label.clone()),
            None => UndoTarget::None,
        }
    }
    pub(super) fn undo_preview_step(&mut self) -> Task<Message> {
        if self.page == Page::Builder {
            self.status = if self.recipe_editor.undo_step() {
                "Widget design edit undone"
            } else {
                "Nothing to undo in this widget design"
            }
            .into();
            self.error = false;
            self.checkpoint_session_drafts();
            return Task::none();
        }
        if self.page == Page::DesktopWallpaper {
            match self.wallpaper.undo_step() {
                Ok(true) => {
                    self.rain.clear();
                    self.status = "Undid a preview change".into();
                    self.error = false;
                }
                Ok(false) => {
                    self.status = "Nothing to undo in this preview".into();
                    self.error = false;
                }
                Err(error) => self.failed(error),
            }
            self.checkpoint_session_drafts();
            return self.media_tasks();
        }
        if preview_kind(self.page).is_some() {
            self.status = if self.profiles.undo_step() {
                "Undid a preview change"
            } else {
                "Nothing to undo in this preview"
            }
            .into();
            self.error = false;
            self.checkpoint_session_drafts();
            return self.media_tasks();
        }
        Task::none()
    }
    /// Position shares the landing's placement editor, right here in this tab.
    pub(super) fn open_position(&mut self, widget: crate::drawer::Widget) -> Task<Message> {
        self.object_tab = ObjectTab::Position;
        let start = if self.legacy_layout_recovery() {
            Task::none()
        } else {
            self.update_canvas(super::canvas::Message::Start)
        };
        // Selecting comes after the editor opens; the listener is admitted then.
        let selection =
            self.update_placement(super::placement::Message::Select(widget.key().into()));
        Task::batch([start, selection])
    }
    pub(super) fn select_object_tab(&mut self, tab: ObjectTab) -> Task<Message> {
        let Some(widget) = widget_of(self.page) else {
            self.object_tab = tab;
            return Task::none();
        };
        if self.object_tab == tab {
            return Task::none();
        }
        if tab == ObjectTab::Position {
            let combined = self.config.glance.enabled
                && matches!(
                    widget,
                    crate::drawer::Widget::Clock
                        | crate::drawer::Widget::Weather
                        | crate::drawer::Widget::Calendar
                );
            let selected = if combined {
                crate::drawer::Widget::Glance
            } else {
                widget
            };
            return self.open_position(selected);
        }
        // Leaving the tab keeps the placement draft; only a gesture stops.
        self.placement.leave();
        self.object_tab = tab;
        cosmic::iced::widget::scrollable::snap_to(
            cosmic::widget::Id::new("settings-content"),
            cosmic::iced::widget::scrollable::RelativeOffset {
                x: None,
                y: Some(0.),
            },
        )
    }
}
