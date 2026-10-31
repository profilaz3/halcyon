use crate::config::{
    ClockAlignment, Config, DevboxSection, GlanceDensity, GlanceStyle, Location, RefreshIntervals,
};
use crate::drawer::Widget;
use cosmic::app::{Core, Task};
use cosmic::iced::{Length, Size, Subscription, window};
use cosmic::{Application, ApplicationExt, Element};
use serde::Deserialize;
use serde_json::json;
use std::path::PathBuf;
use std::time::{Duration, Instant};

mod automation;
mod calendar;
mod canvas;
mod city_search;
mod color_well;
mod colors;
mod depth;
mod design;
mod desktop;
mod desktop_icons;
mod direct;
#[cfg(test)]
mod display_tests;
mod drafts;
mod legacy;
mod moments;
mod monitors;
mod motion;
mod native_jobs;
mod navigation;
mod placement;
mod preference_undo;
mod privacy;
#[cfg(test)]
mod privacy_tests;
mod profiles;
mod rain;
mod recipes;
mod recovery;
mod reorder;
pub(crate) mod session_drafts;
pub(crate) mod sharing;
mod shortcuts;
mod support;
mod system_appearance;
mod tools;
mod updates;
mod view;
mod wallpaper;
mod window_rules;
mod windows;
mod wizard;
mod workspaces;

#[cfg(test)]
pub(crate) use wizard::widgets::Draft as TestWidgetDraft;

use city_search::CitySearch;
use reorder::Reorder;
use view::AppSettingsPage;

const FONT_PAGE_SIZE: usize = 12;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Page {
    Overview,
    WidgetLayout,
    PendingChanges,
    Appearance,
    Widgets,
    Glance,
    Calendar,
    Weather,
    Devbox,
    Clock,
    Projects,
    /// Desktop tools; one navigation entry ("Tools") with a tab each.
    Music,
    Notes,
    Focus,
    General,
    Desktop,
    /// Dock & Panel editor, a subpage of Pop!_OS.
    DesktopDock,
    /// System appearance editor, a subpage of Pop!_OS.
    DesktopAppearance,
    /// Backgrounds, slideshows and colors, a subpage of Pop!_OS.
    DesktopWallpaper,
    /// Desktop icon size and what the desktop shows, a subpage of Pop!_OS.
    DesktopIcons,
    /// Saved and built-in desktop looks, a subpage of Pop!_OS.
    DesktopProfiles,
    Themes,
    /// Tiling, window gaps, controls and focus, a subpage of Pop!_OS.
    DesktopWindows,
    /// Workspace layout and navigation, a subpage of Pop!_OS.
    DesktopWorkspaces,
    /// App animations and the sidebar's slide, a subpage of Pop!_OS.
    DesktopMotion,
    /// Apps that open floating while tiling, a subpage of Pop!_OS (also
    /// linked from Windows).
    DesktopWindowRules,
    /// Custom shortcuts and built-in bindings, a subpage of Pop!_OS.
    DesktopShortcuts,
    /// Profile schedules and display rules, a subpage of Pop!_OS.
    DesktopAutomation,
    Moments,
    AdaptiveLooks,
    Builder,
}
impl Page {
    /// Six stable roots; leaf routes keep their existing command keys.
    const SIDEBAR: [(Option<&'static str>, &'static [Self]); 1] = [(
        None,
        &[
            Self::Themes,
            Self::DesktopDock,
            Self::Widgets,
            Self::DesktopProfiles,
            Self::DesktopMotion,
            Self::General,
        ],
    )];
    const ALL: [Self; 31] = [
        Self::Overview,
        Self::WidgetLayout,
        Self::PendingChanges,
        Self::Appearance,
        Self::Widgets,
        Self::Glance,
        Self::Calendar,
        Self::Weather,
        Self::Devbox,
        Self::Clock,
        Self::Projects,
        Self::Music,
        Self::Notes,
        Self::Focus,
        Self::General,
        Self::Desktop,
        Self::DesktopDock,
        Self::DesktopAppearance,
        Self::DesktopWallpaper,
        Self::DesktopIcons,
        Self::DesktopProfiles,
        Self::Themes,
        Self::DesktopWindows,
        Self::DesktopWorkspaces,
        Self::DesktopMotion,
        Self::DesktopWindowRules,
        Self::DesktopShortcuts,
        Self::DesktopAutomation,
        Self::Moments,
        Self::AdaptiveLooks,
        Self::Builder,
    ];
    fn nav(self) -> Self {
        match self {
            Self::DesktopWallpaper => Self::DesktopWallpaper,
            Self::Themes | Self::DesktopAppearance => Self::Themes,
            Self::Desktop | Self::DesktopDock => Self::DesktopDock,
            Self::DesktopWindows | Self::DesktopWorkspaces | Self::DesktopWindowRules => {
                Self::DesktopWindows
            }
            Self::DesktopIcons => Self::DesktopIcons,
            Self::DesktopShortcuts => Self::DesktopShortcuts,
            Self::Overview | Self::WidgetLayout => Self::Overview,
            Self::Builder => Self::Builder,
            Self::Appearance => Self::Appearance,
            Self::DesktopProfiles
            | Self::DesktopAutomation
            | Self::Moments
            | Self::AdaptiveLooks => Self::DesktopProfiles,
            Self::DesktopMotion => Self::DesktopMotion,
            Self::General | Self::PendingChanges => Self::General,
            _ => Self::Widgets,
        }
    }
    /// Subpages keep their task selected in the native navigation bar.
    fn sidebar(self) -> Self {
        match self.canonical() {
            Self::Themes | Self::Appearance | Self::DesktopAppearance | Self::DesktopWallpaper => {
                Self::Themes
            }
            Self::DesktopDock
            | Self::DesktopWindows
            | Self::DesktopWindowRules
            | Self::DesktopIcons
            | Self::DesktopShortcuts => Self::DesktopDock,
            Self::DesktopProfiles
            | Self::DesktopAutomation
            | Self::Moments
            | Self::AdaptiveLooks => Self::DesktopProfiles,
            Self::DesktopMotion => Self::DesktopMotion,
            Self::General | Self::PendingChanges => Self::General,
            _ => Self::Widgets,
        }
    }
    fn root_index(self) -> usize {
        match self.sidebar() {
            Self::Themes => 0,
            Self::DesktopDock => 1,
            Self::Widgets => 2,
            Self::DesktopProfiles => 3,
            Self::DesktopMotion => 4,
            _ => 5,
        }
    }
    fn root_title(self) -> &'static str {
        match self.sidebar() {
            Self::Themes => "Appearance",
            Self::DesktopDock => "Desktop",
            Self::Widgets => "Widgets",
            Self::DesktopProfiles => "Setups",
            Self::DesktopMotion => "Accessibility",
            _ => "App settings",
        }
    }
    fn destinations(self) -> &'static [Self] {
        match self.sidebar() {
            Self::Themes => &[
                Self::Themes,
                Self::DesktopWallpaper,
                Self::Appearance,
                Self::DesktopAppearance,
            ],
            Self::DesktopDock => &[
                Self::DesktopDock,
                Self::DesktopWindows,
                Self::DesktopIcons,
                Self::DesktopShortcuts,
            ],
            Self::Widgets => &[Self::Widgets, Self::Overview, Self::Builder],
            Self::DesktopProfiles => &[Self::DesktopProfiles, Self::DesktopAutomation],
            Self::DesktopMotion => &[Self::DesktopMotion],
            _ => &[Self::General],
        }
    }
    /// The short sidebar name.
    fn nav_title(self) -> &'static str {
        match self {
            Self::DesktopAutomation => "Automation",
            Self::Overview => "Layout & displays",
            Self::Widgets => "My widgets",
            Self::WidgetLayout => "Layout",
            Self::Appearance => "Surface finishes",
            Self::DesktopAppearance => "Fonts & icons",
            Self::DesktopWallpaper => "Wallpaper",
            Self::Themes => "Theme & colors",
            Self::DesktopWindows => "Windows & workspaces",
            Self::DesktopIcons => "Desktop icons",
            Self::DesktopMotion => "Accessibility",
            page => page.title(),
        }
    }
    fn nav_icon(self) -> &'static str {
        match self {
            Self::Overview => "object-move-symbolic",
            Self::Widgets => "view-grid-symbolic",
            Self::Appearance => "applications-graphics-symbolic",
            Self::DesktopAppearance => "preferences-desktop-appearance-symbolic",
            Self::DesktopWallpaper => "preferences-desktop-wallpaper-symbolic",
            Self::Themes => "preferences-color-symbolic",
            Self::DesktopDock => "view-app-grid-symbolic",
            Self::DesktopWindows => "preferences-window-management-symbolic",
            Self::DesktopIcons => "user-desktop-symbolic",
            Self::DesktopShortcuts => "input-keyboard-symbolic",
            Self::DesktopMotion => "preferences-desktop-accessibility-symbolic",
            Self::DesktopProfiles => "starred-symbolic",
            Self::DesktopAutomation => "alarm-symbolic",
            Self::General => "emblem-system-symbolic",
            page => page.icon(),
        }
    }
    /// The tool a tools page edits.
    fn tool(self) -> Option<Widget> {
        match self {
            Self::Music => Some(Widget::Music),
            Self::Notes => Some(Widget::Notes),
            Self::Focus => Some(Widget::Focus),
            _ => None,
        }
    }
    fn key(self) -> &'static str {
        match self {
            Self::Overview => "overview",
            Self::WidgetLayout => "layout",
            Self::PendingChanges => "pending",
            Self::Appearance => "appearance",
            Self::Widgets => "widgets",
            Self::Glance => "glance",
            Self::Calendar => "calendar",
            Self::Weather => "weather",
            Self::Devbox => "devbox",
            Self::Clock => "clock",
            Self::Projects => "projects",
            Self::Music => "music",
            Self::Notes => "notes",
            Self::Focus => "focus",
            Self::General => "general",
            Self::Desktop => "desktop",
            Self::DesktopDock => "desktop-dock",
            Self::DesktopAppearance => "desktop-appearance",
            Self::DesktopWallpaper => "desktop-wallpaper",
            Self::DesktopIcons => "desktop-icons",
            Self::DesktopProfiles => "desktop-profiles",
            Self::Themes => "themes",
            Self::DesktopWindows => "desktop-windows",
            Self::DesktopWorkspaces => "desktop-workspaces",
            Self::DesktopMotion => "desktop-motion",
            Self::DesktopWindowRules => "desktop-window-rules",
            Self::DesktopShortcuts => "desktop-shortcuts",
            Self::DesktopAutomation => "desktop-automation",
            Self::Moments => "moments",
            Self::AdaptiveLooks => "adaptive",
            Self::Builder => "builder",
        }
    }
    fn title(self) -> &'static str {
        match self {
            Self::Overview => "Layout & displays",
            Self::WidgetLayout => "Arrange widgets",
            Self::PendingChanges => "Recover unfinished work",
            Self::Appearance => "Surface finishes",
            Self::Widgets => "My widgets",
            Self::Glance => "Daily overview",
            Self::Calendar => "Calendar",
            Self::Weather => "Weather",
            Self::Devbox => "System monitor",
            Self::Clock => "Clock",
            Self::Projects => "Projects",
            Self::Music => "Music",
            Self::Notes => "Notes",
            Self::Focus => "Focus timer",
            Self::General => "App settings",
            Self::Desktop => "Desktop",
            Self::DesktopDock => "Dock & panel",
            Self::DesktopAppearance => "Fonts & icons",
            Self::DesktopWallpaper => "Wallpaper",
            Self::DesktopIcons => "Desktop icons",
            Self::DesktopProfiles => "Setups",
            Self::Themes => "Theme & colors",
            Self::DesktopWindows => "Windows & workspaces",
            Self::DesktopWorkspaces => "Workspaces",
            Self::DesktopMotion => "Accessibility",
            Self::DesktopWindowRules => "Floating apps",
            Self::DesktopShortcuts => "Shortcuts",
            Self::DesktopAutomation => "Schedules & display rules",
            Self::Moments => "Moments",
            Self::AdaptiveLooks => "Adaptive looks",
            Self::Builder => "Widget builder",
        }
    }
    fn parse(value: &str) -> Self {
        // Stable command keys and descriptive aliases all reach the same editors.
        let value = match value {
            "home" => "overview",
            "setups" => "desktop-profiles",
            "look" => "themes",
            "widget-style" => "appearance",
            "system-monitor" => "devbox",
            "daily-overview" => "glance",
            "focus-timer" => "focus",
            "app-settings" => "general",
            "presets" => "desktop-profiles",
            "schedules" => "desktop-automation",
            _ => value,
        };
        Self::ALL
            .into_iter()
            .find(|page| page.key() == value)
            .unwrap_or(Self::Themes)
            .canonical()
    }
    /// Keep legacy layout links and recovered drafts, but use one editor.
    fn canonical(self) -> Self {
        match self {
            Self::WidgetLayout => Self::Overview,
            Self::Desktop => Self::DesktopDock,
            // Workspaces is a section of Windows.
            Self::DesktopWorkspaces => Self::DesktopWindows,
            other => other,
        }
    }
    fn icon(self) -> &'static str {
        match self {
            Self::Overview => "preferences-desktop-display-symbolic",
            Self::WidgetLayout => "object-move-symbolic",
            Self::PendingChanges => "document-save-symbolic",
            Self::Appearance => "preferences-desktop-wallpaper-symbolic",
            Self::Widgets => "view-grid-symbolic",
            Self::Glance => "preferences-system-time-symbolic",
            Self::Calendar => "x-office-calendar-symbolic",
            Self::Weather => "weather-few-clouds-symbolic",
            Self::Devbox => "utilities-system-monitor-symbolic",
            Self::Clock => "preferences-system-time-symbolic",
            Self::Projects => "folder-symbolic",
            Self::Music => "multimedia-audio-player-symbolic",
            Self::Notes => "accessories-text-editor-symbolic",
            Self::Focus => "alarm-symbolic",
            Self::General => "preferences-system-symbolic",
            Self::DesktopWallpaper => "preferences-desktop-wallpaper-symbolic",
            Self::Desktop
            | Self::DesktopDock
            | Self::DesktopAppearance
            | Self::DesktopIcons
            | Self::DesktopProfiles
            | Self::Themes
            | Self::DesktopWindows
            | Self::DesktopWorkspaces
            | Self::DesktopMotion
            | Self::DesktopWindowRules
            | Self::DesktopShortcuts
            | Self::DesktopAutomation
            | Self::Moments
            | Self::AdaptiveLooks
            | Self::Builder => "preferences-desktop-symbolic",
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct City {
    name: String,
    latitude: f64,
    longitude: f64,
    #[serde(default)]
    timezone: Option<String>,
    #[serde(default)]
    admin1: String,
    #[serde(default)]
    country: String,
}
impl City {
    fn location(&self) -> Location {
        Location {
            name: self.name.clone(),
            latitude: self.latitude,
            longitude: self.longitude,
            timezone: self
                .timezone
                .clone()
                .filter(|zone| zone.parse::<chrono_tz::Tz>().is_ok()),
        }
    }
    fn region(&self) -> String {
        [self.admin1.as_str(), self.country.as_str()]
            .into_iter()
            .filter(|part| !part.is_empty())
            .collect::<Vec<_>>()
            .join(" · ")
    }
}

#[derive(Debug, Clone, Default)]
pub struct ServiceState {
    running: bool,
    startup: bool,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AppearanceControl {
    Opacity,
    Corners,
    Border,
}
/// Which styles the browser lists.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StyleFilter {
    All,
    Dark,
    Light,
    SeeThrough,
}
impl StyleFilter {
    pub const ALL: [Self; 4] = [Self::All, Self::Dark, Self::Light, Self::SeeThrough];
    pub fn title(self) -> &'static str {
        match self {
            Self::All => "All",
            Self::Dark => "Dark",
            Self::Light => "Light",
            Self::SeeThrough => "See-through",
        }
    }
    pub fn shows(self, family: crate::desktop_theme::Family) -> bool {
        let spec = family.spec();
        match self {
            Self::All => true,
            Self::Dark => !spec.light,
            Self::Light => spec.light,
            Self::SeeThrough => spec.transparent || spec.frosted || spec.geometry.0 < 90,
        }
    }
}
/// Title bar actions; Settings draws the bar itself (see `view`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WindowControl {
    Drag,
    Menu,
    Maximize,
    Minimize,
    Close,
}
/// Which widget font a list chooses.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FontTarget {
    Text,
    Clock,
    /// At a glance's own fonts, over the widget fonts.
    GlanceText,
    GlanceClock,
}
#[derive(Debug, Clone)]
pub enum Message {
    AppSettingsPage(AppSettingsPage),
    ToggleScene,
    ToggleSceneObjects,
    ContentScrolled(f32),
    Updates(updates::Message),
    Support(support::Message),
    KeyboardFocus(bool),
    Fullscreen,
    ShareUsage(bool),
    UsageSaved(Result<bool, String>),
    OpenPrivacy,
    WidgetDisplay(Option<String>),
    WidgetMonitor(Widget, Option<String>),
    WidgetMonitorHide(Widget, bool),
    MonitorPreview(String),
    IdentifyDisplays,
    ClearDisplayLabels(Vec<window::Id>),
    DisplayIdentities(u64, std::collections::BTreeMap<String, String>),
    DisplayOutput(crate::displays::OutputEvent, crate::displays::WlOutput),
    Wizard(wizard::Message),
    /// Starts first-run setup again (App settings).
    OpenWizard,
    /// The shared color control: suggestions and COSMIC's color picker.
    ColorWell(color_well::Message),
    /// Opens (or, again, closes) a widget font list.
    FontMenu(Option<FontTarget>),
    FontSearch(String),
    FontPage(usize),
    FontCatalogReady(crate::desktop::appearance::catalog::FontCatalog),
    LookImagesReady,
    /// A widget font family; `None` returns to the default.
    WidgetFont(FontTarget, Option<String>),
    /// At a glance uses the widget style's surface again.
    GlanceMatchWidgets,
    /// Whether choosing a widget style also styles the dock and panel.
    StyleBars(bool),
    /// Opens (or leaves) the styles browser on Colors & theme.
    StyleBrowser(bool),
    StyleFilter(StyleFilter),
    /// Read-only sample cards never perform desktop actions.
    Preview,
    Tick,
    RefreshService,
    Page(Page),
    NavSelected(cosmic::widget::nav_bar::Id),
    SearchSettings(String),
    SearchFirst,
    SearchResult(navigation::SearchHit),
    RevealSearch(navigation::SearchHit),
    FocusSearch,
    /// Up one level: a subpage's parent, else Layout (Alt+Left).
    Back,
    /// Home: preview a built-in look on the Presets page. Nothing applies.
    TryPreset(&'static str),
    /// Home: open Dock & panel with this bar selected.
    OpenBar(crate::desktop::panels::PanelKind),
    ToggleAdvanced(Page),
    ToggleHelp(&'static str),
    ToggleSection(&'static str),
    UndoDesktop,
    UndoDesktopShown(Option<String>),
    ResolveUndoShown(crate::desktop::store::UndoReview, bool),
    RecoverySelect(usize, bool),
    RecoveryUseProposed(usize),
    RecoveryOpen(usize),
    RecoveryDiscard(usize),
    RecoveryApply,
    RecoveryApplyOne(usize),
    RecoveryText(usize, usize, String),
    UndoPreferences,
    Placement(placement::Message),
    Canvas(canvas::Message),
    EditWidgetLayout(Widget),
    DesktopEditor(desktop::Message),
    SystemAppearance(system_appearance::Message),
    Profiles(profiles::Message),
    Windows(windows::Message),
    Workspaces(workspaces::Message),
    Motion(motion::Message),
    /// The edge sidebar slides (`true`) or appears at once.
    SidebarSlide(bool),
    SettingsAnimations(bool),
    ReadSystemMotion,
    SystemMotion(u64, bool),
    WindowRules(window_rules::Message),
    Shortcuts(shortcuts::Message),
    Wallpaper(wallpaper::Message),
    Rain(rain::Edit),
    DesktopIcons(desktop_icons::Message),
    Tools(tools::Message),
    Automation(automation::Message),
    AdaptivePreview(
        crate::desktop::profiles::adaptive::Look,
        crate::desktop::profiles::adaptive::Phase,
    ),
    AdaptiveSource(crate::automation::solar::Source),
    Resized(Size),
    Widget(Widget, bool),
    RefreshInterval(Widget, u64),
    Move(Widget, i32),
    DragStart(Widget),
    DragOver(Widget),
    DragLeave(Widget),
    DragDrop,
    DragCancel,
    Devbox(&'static str, bool),
    DevboxHistory(&'static str, bool),
    DevboxDetails(DevboxSection),
    DevboxReorder(bool),
    GlanceSection(&'static str, bool),
    GlanceTomorrow(bool),
    GlanceStyle(GlanceStyle),
    GlanceDensity(GlanceDensity),
    GlanceOpacity(u8),
    DevboxMove(DevboxSection, i32),
    DevboxDragStart(DevboxSection),
    DevboxDragOver(DevboxSection),
    DevboxDragLeave(DevboxSection),
    ArrangeDesktop,
    OpenDesktop(&'static str),
    CalendarPrivacy(bool),
    Today(bool),
    Tomorrow(bool),
    Notice(u16),
    SaveNotice,
    Theme(String),
    WidgetColorSource(crate::config::ColorSource),
    DesktopPaletteChanged,
    AppearanceValue(AppearanceControl, u16),
    AppearancePreview(Widget),
    MaterialTarget(crate::materials::Target),
    MaterialChoose(Option<crate::materials::Kind>),
    MaterialAdjust(bool),
    MaterialValue(bool, u8),
    MaterialSave,
    MaterialMotion(bool),
    ContrastAssist(bool),
    WidgetSurface(Option<crate::desktop_theme::Surface>),
    ResetThemeSurface(crate::desktop::profiles::Domain),
    SaveAppearance,
    CommitEdits(u64),
    FlushEdits,
    /// The title bar's own buttons and gestures, handled by libcosmic.
    Window(WindowControl),
    AppearancePreset(u8, u16, u8),
    MatchWallpaper(bool),
    // Legacy preference route retained for compatibility regression coverage.
    #[cfg_attr(not(test), allow(dead_code))]
    MatchCosmicColors(bool),
    ClockFormat(bool),
    ClockDate(bool),
    ClockAlignment(ClockAlignment),
    ClockScale(u16),
    ClockVertical(bool),
    DepthPreviewReady(u64, Option<crate::desktop::wallpaper::media::Pixels>),
    ClockDepth(bool),
    Moment(moments::Message),
    Recipe(recipes::Message),
    OpenRecipe(crate::recipes::Kind),
    Design(design::Message),
    ClockFace(Option<crate::widget_style::resolver::ClockFace>),
    ClockWeight(Option<crate::widget_style::resolver::ClockWeight>),
    ClockItalic(Option<bool>),
    ClockSurface(Option<crate::widget_style::resolver::ClockSurface>),
    WeatherDewPoint(bool),
    CityInput(String),
    EditCity(bool),
    SearchCity,
    SearchCityAfter(u64),
    ClearCitySearch,
    Cities(u64, Result<Vec<City>, String>),
    ChooseCity(usize),
    ScheduleCityInput(String),
    EditScheduleCity(bool),
    SearchScheduleCity,
    SearchScheduleCityAfter(u64),
    ScheduleCities(u64, Result<Vec<City>, String>),
    ChooseScheduleCity(usize),
    FolderInput(String),
    AddFolder,
    BrowseFolder,
    FolderChosen(Result<Option<PathBuf>, String>),
    RemoveFolder(String),
    Desktop(bool),
    Background(bool),
    Startup(bool),
    Service(Result<ServiceState, String>),
    CommandDone(String, Result<(), String>),
    OpenCalendar,
    Calendar(calendar::Message),
    SetupDismissed(bool),
    SetupOpen(bool),
    DismissSetupTask(wizard::follow_up::Task),
    OpenWeatherSource,
    UndoDesktopRestore(String),
    DesktopRestorePreview,
    DesktopRestorePlanned(Result<crate::desktop::recovery::RestorePlan, String>),
    DesktopRestoreConfirm(bool),
    DesktopRestoreDone(Result<(), String>),
    ExportRecovery,
    RecoveryExported(Result<Option<PathBuf>, String>),
    OpenIslandsGuide,
    CopyIslandsDiagnostics,
    NativeDone(native_jobs::Delivery),
    InstallationChecked(
        u64,
        crate::desktop::islands::Installation,
        desktop::islands::RuntimeStatus,
    ),
    RefreshInstallation,
    RecheckInstallation,
    InstallationPresence(u64, crate::desktop::islands::Installation),
    InstallationReady(u64, crate::desktop::islands::Installation),
    ShowControls,
    Restore(bool),
    Capture,
    Captured(Page, window::Screenshot),
    /// Capture mode only: the last pointer position, so validation tools can
    /// locate the window before sending real pointer input.
    CaptureCursor(cosmic::iced::Point),
}

pub struct Settings {
    depth_preview: depth::Cache,
    rain: rain::State,
    design: design::State,
    moment_preview: crate::moments::Phase,
    adaptive_look: crate::desktop::profiles::adaptive::Look,
    adaptive_phase: crate::desktop::profiles::adaptive::Phase,
    adaptive_source: crate::automation::solar::Source,
    recipe_editor: recipes::Editor,
    moment_base_clock: bool,
    updates: updates::Updates,
    support: support::Support,
    usage_enabled: bool,
    usage_busy: bool,
    usage_error: Option<String>,
    displays: crate::displays::Registry,
    identified_displays: Vec<(window::Id, String)>,
    core: Core,
    page: Page,
    app_settings_page: AppSettingsPage,
    root_destinations: [Page; 6],
    collapsed_scenes: Vec<Page>,
    scene_objects_open: bool,
    page_scroll: std::collections::BTreeMap<String, f32>,
    /// First-run setup, shown instead of the pages while it runs.
    wizard: Option<wizard::Wizard>,
    search_query: String,
    setup_requested: bool,
    setup_followups: wizard::follow_up::State,
    search_selected: Option<navigation::SearchHit>,
    advanced_pages: Vec<Page>,
    help_open: Option<&'static str>,
    open_sections: std::collections::BTreeSet<&'static str>,
    last_desktop_change: Option<(crate::desktop::Owner, String, String)>,
    /// The latest change when this window opened; the Undo bar offers only
    /// changes made since, so an old change doesn't follow you around.
    undo_baseline: Option<String>,
    /// The color picker that is open, if any; one at a time.
    color_well: Option<color_well::Well>,
    /// The widget font list that is open, and its search.
    font_menu: Option<FontTarget>,
    font_search: String,
    font_page: usize,
    font_loading: bool,
    /// Colors & theme shows the styles browser instead of its main view.
    style_browser: bool,
    style_filter: StyleFilter,
    /// Families the renderer can draw, read when a font list first opens.
    font_catalog: Option<crate::desktop::appearance::catalog::FontCatalog>,
    #[cfg(test)]
    preference_undo: Option<navigation::PreferenceUndo>,
    gesture_config: Option<serde_json::Value>,
    undo_review: Option<crate::desktop::store::UndoReview>,
    native_property: u64,
    session_drafts: session_drafts::SessionDrafts,
    recovery_sheet: legacy::Recovery,
    desktop_recovery: recovery::DesktopRecovery,
    installation_generation: u64,
    installation_busy: bool,
    installation_retry: bool,
    installation_deadline: Option<std::time::Instant>,
    native_busy: bool,
    native_deferred: std::collections::VecDeque<Message>,
    native_tasks: Vec<Task<Message>>,
    config: Config,
    samples: crate::app::SettingsSamples,
    notice: u16,
    notice_editing: bool,
    material_target: crate::materials::Target,
    material_adjust: bool,
    material_draft: Option<crate::materials::Finish>,
    material_baseline: Option<crate::materials::Materials>,
    material_native: bool,
    appearance_draft: crate::config::Appearance,
    appearance_preview: Widget,
    appearance_editing: bool,
    appearance_pending: Vec<AppearanceControl>,
    edit_generation: u64,
    save_at: Option<Instant>,
    city_search: CitySearch,
    schedule_city_search: CitySearch,
    schedule_city_editing: bool,
    schedule_cities: Vec<City>,
    city_editing: bool,
    cities: Vec<City>,
    reorder: Reorder<Widget>,
    devbox_reorder: Reorder<DevboxSection>,
    devbox_reordering: bool,
    devbox_details: Vec<DevboxSection>,
    placement: placement::PlacementEditor,
    canvas: Option<canvas::Draft>,
    desktop: desktop::DesktopEditor,
    system_appearance: system_appearance::AppearanceEditor,
    profiles: profiles::ProfilesEditor,
    windows: windows::WindowsEditor,
    workspaces: workspaces::WorkspacesEditor,
    motion: motion::MotionEditor,
    system_animations: bool,
    motion_generation: u64,
    window_rules: window_rules::WindowRulesEditor,
    shortcuts: shortcuts::ShortcutsEditor,
    wallpaper: wallpaper::WallpaperEditor,
    desktop_icons: desktop_icons::DesktopIconsEditor,
    tools: tools::ToolsEditor,
    automation: automation::AutomationEditor,
    calendar: calendar::Editor,
    folder_input: String,
    folder_pending: bool,
    service: ServiceState,
    service_known: bool,
    busy: bool,
    status: String,
    error: bool,
    restore_confirm: bool,
    capture_key: String,
    nav_model: cosmic::widget::nav_bar::Model,
    window: Size,
}

fn initial_size() -> Size {
    capture_dir()
        .and_then(|_| std::env::var("DESKTOP_CARDS_SETTINGS_SIZE").ok())
        .and_then(|value| {
            value.split_once('x').and_then(|(w, h)| {
                Some(Size::new(
                    w.parse::<f32>().ok()?.clamp(820., 3840.),
                    h.parse::<f32>().ok()?.clamp(600., 2160.),
                ))
            })
        })
        .unwrap_or(Size::new(1020., 800.))
}

/// Set by `run` for the `styles` destination, read once by `init`.
static OPEN_STYLES: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
/// First-run setup, starting from the dock and panel as they are now, or
/// resuming setup left unfinished (with its changes, for Cancel).
fn open_wizard() -> wizard::Wizard {
    let store = crate::desktop::DesktopStore::from_env().ok();
    let islands = store
        .as_ref()
        .is_some_and(crate::desktop::profiles::islands_available);
    let bars = store
        .as_ref()
        .map(|store| wizard::Bars::current(store, islands))
        .unwrap_or_else(|| wizard::Bars::starter(islands));
    // A first run starts the cards at sign-in; running setup again starts
    // from how it is now.
    let startup = !Config::load().onboarded || preview() || startup_enabled();
    store
        .as_ref()
        .and_then(|store| wizard::Wizard::resume(store, islands, bars, startup))
        .unwrap_or_else(|| wizard::Wizard::new(islands, bars, startup))
}
/// Whether the cards start at sign-in (the user service is enabled).
fn startup_enabled() -> bool {
    std::process::Command::new("/usr/bin/systemctl")
        .args(["--user", "is-enabled", "--quiet", crate::paths::SERVICE])
        .status()
        .is_ok_and(|status| status.success())
}
/// Set by `run` for first-run setup (`setup`), read once by `init`.
static OPEN_WIZARD: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
pub fn run(page: &str) -> cosmic::iced::Result {
    let remembered = if page.is_empty() {
        crate::desktop::DesktopStore::from_env()
            .ok()
            .and_then(|s| std::fs::read_to_string(s.state_dir().join("settings-destination")).ok())
    } else {
        None
    };
    // Someone who has never chosen a look or finished setup starts with it,
    // and setup left unfinished picks up where it was.
    let first = (page.is_empty() && {
        let config = Config::load();
        let unfinished = crate::desktop::DesktopStore::from_env()
            .is_ok_and(|store| wizard::Wizard::pending(&store));
        !config.onboarded
            && (unfinished || (remembered.is_none() && config.appearance.family.is_none()))
    })
    .then_some("setup");
    let page = if first.is_some() { "" } else { page };
    let remembered = if first.is_some() { None } else { remembered };
    let page = remembered.as_deref().or(first).unwrap_or(page);
    let page = if page == "setup" {
        OPEN_WIZARD.store(true, std::sync::atomic::Ordering::Relaxed);
        "overview"
    } else {
        page
    };
    // `styles` opens the styles browser on Colors & theme.
    let page = if page == "styles" {
        OPEN_STYLES.store(true, std::sync::atomic::Ordering::Relaxed);
        "themes"
    } else {
        page
    };
    let size = initial_size();
    let limits = if capture_commands() && std::env::var_os("DESKTOP_CARDS_SETTINGS_SIZE").is_some()
    {
        // Native capture fixtures need an exact logical size even when the
        // compositor remembers a previous window geometry.
        cosmic::iced::Limits::NONE
            .min_width(size.width)
            .max_width(size.width)
            .min_height(size.height)
            .max_height(size.height)
    } else {
        cosmic::iced::Limits::NONE.min_width(820.).min_height(600.)
    };
    let settings = cosmic::app::Settings::default()
        .size(size)
        .size_limits(limits)
        .theme(controls_theme(&Config::load()))
        .transparent(false)
        .exit_on_close(true);
    cosmic::app::run::<Settings>(settings, Page::parse(page))
}

impl cosmic::Application for Settings {
    type Executor = cosmic::executor::Default;
    type Flags = Page;
    type Message = Message;
    const APP_ID: &'static str = "io.github.profilaz3.Halcyon";
    fn core(&self) -> &Core {
        &self.core
    }
    fn core_mut(&mut self) -> &mut Core {
        &mut self.core
    }
    fn init(mut core: Core, page: Page) -> (Self, Task<Message>) {
        core.set_keyboard_nav(false);
        let _native = crate::desktop::islands::Installation::native_view(Default::default());
        // Styles name their fonts; learn which are installed before drawing.
        crate::desktop_theme::load_installed_fonts();
        let page = page.canonical();
        let (config, design) = design::State::initialize();
        let wizard = OPEN_WIZARD
            .swap(false, std::sync::atomic::Ordering::Relaxed)
            .then_some(())
            .filter(|_| config.design_problem.is_none())
            .map(|_| open_wizard());
        crate::usage::start_status();
        let usage = crate::usage::preference();
        let mut app = Self {
            depth_preview: Default::default(),
            rain: Default::default(),
            design,
            moment_preview: crate::moments::Phase::Day,
            adaptive_look: Default::default(),
            adaptive_phase: Default::default(),
            adaptive_source: Default::default(),
            recipe_editor: recipes::Editor::loaded(),
            moment_base_clock: false,
            updates: updates::Updates::default(),
            support: support::Support::default(),
            usage_enabled: usage.as_ref().copied().unwrap_or(false),
            usage_error: usage.err(),
            usage_busy: false,
            displays: Default::default(),
            identified_displays: Vec::new(),
            core,
            page,
            wizard,
            search_query: String::new(),
            setup_requested: false,
            setup_followups: wizard::follow_up::State::load(),
            search_selected: None,
            advanced_pages: Vec::new(),
            help_open: None,
            open_sections: Default::default(),
            last_desktop_change: navigation::latest_desktop_change(),
            undo_baseline: navigation::latest_desktop_change().map(|(_, _, id)| id),
            color_well: None,
            font_menu: None,
            font_search: String::new(),
            font_page: 0,
            font_loading: false,
            style_browser: page == Page::Themes
                && OPEN_STYLES.swap(false, std::sync::atomic::Ordering::Relaxed),
            style_filter: StyleFilter::All,
            font_catalog: None,
            #[cfg(test)]
            preference_undo: None,
            gesture_config: None,
            undo_review: None,
            native_property: 0,
            session_drafts: session_drafts::SessionDrafts::default(),
            recovery_sheet: legacy::Recovery::load(),
            desktop_recovery: recovery::DesktopRecovery::default(),
            installation_generation: 0,
            installation_busy: false,
            installation_retry: false,
            installation_deadline: None,
            native_busy: false,
            native_deferred: Default::default(),
            native_tasks: Vec::new(),
            notice: config.meeting_notice_minutes.clamp(1, 60) as u16,
            material_target: Default::default(),
            material_adjust: false,
            material_draft: None,
            material_baseline: None,
            material_native: false,
            appearance_draft: config.appearance.clone(),
            appearance_preview: Widget::Weather,
            app_settings_page: navigation::app_destination(),
            root_destinations: navigation::root_destinations(page),
            collapsed_scenes: Vec::new(),
            scene_objects_open: false,
            page_scroll: Default::default(),
            nav_model: navigation::model(page),
            appearance_editing: false,
            appearance_pending: Vec::new(),
            edit_generation: 0,
            save_at: None,
            city_search: CitySearch::default(),
            schedule_city_search: CitySearch::default(),
            schedule_city_editing: false,
            schedule_cities: Vec::new(),
            city_editing: false,
            placement: placement::PlacementEditor::new(&config),
            canvas: None,
            desktop: desktop::DesktopEditor::default(),
            system_appearance: system_appearance::AppearanceEditor::default(),
            profiles: profiles::ProfilesEditor::default(),
            windows: windows::WindowsEditor::default(),
            workspaces: workspaces::WorkspacesEditor::default(),
            motion: motion::MotionEditor::default(),
            system_animations: false,
            motion_generation: 0,
            window_rules: window_rules::WindowRulesEditor::default(),
            shortcuts: shortcuts::ShortcutsEditor::default(),
            wallpaper: wallpaper::WallpaperEditor::default(),
            desktop_icons: desktop_icons::DesktopIconsEditor::default(),
            tools: tools::ToolsEditor::default(),
            automation: automation::AutomationEditor::default(),
            samples: crate::app::SettingsSamples::new(&config),
            config,
            notice_editing: false,
            cities: Vec::new(),
            reorder: Reorder::new(),
            devbox_reorder: Reorder::new(),
            devbox_reordering: false,
            devbox_details: Vec::new(),
            calendar: calendar::Editor::default(),
            folder_input: String::new(),
            folder_pending: false,
            service: ServiceState::default(),
            service_known: false,
            busy: false,
            status: page.save_hint().into(),
            error: false,
            restore_confirm: false,
            capture_key: String::new(),
            window: initial_size(),
        };
        app.set_header_title("Halcyon".into());
        // With frosted windows, libcosmic's title bar has no color of its own:
        // the compositor's blur shows through it, unlike the opaque pages, and
        // flickers as the desktop underneath redraws. Settings draws the bar
        // on its own opaque surface instead, and needs no window blur.
        app.core.window.show_headerbar = false;
        app.core.window.content_container = false;
        app.core
            // Settings and its popups do not request native backdrop effects.
            .set_auto_blur(Default::default());
        let title = app
            .core
            .main_window_id()
            .map(|id| app.set_window_title("Halcyon".into(), id))
            .unwrap_or_else(Task::none);
        let service = if matches!(page, Page::General | Page::Overview) {
            service_task()
        } else {
            Task::none()
        };
        if page == Page::DesktopDock {
            app.desktop.open();
        }
        if page == Page::Appearance && !app.native_worker_needed() {
            app.material_native = crate::desktop::actions::history_store()
                .is_ok_and(|s| crate::desktop::materials::native_support(&s));
        }
        if page == Page::DesktopAppearance {
            app.system_appearance.open();
        }
        if matches!(
            page,
            Page::DesktopProfiles | Page::Themes | Page::AdaptiveLooks
        ) {
            app.profiles.open();
        }
        if matches!(
            page,
            Page::DesktopAutomation
                | Page::Desktop
                | Page::DesktopProfiles
                | Page::AdaptiveLooks
                | Page::Moments
        ) {
            app.automation.open();
        }
        if page == Page::Themes {
            app.wallpaper.open();
            app.system_appearance.open();
        }
        if page == Page::AdaptiveLooks {
            app.adaptive_source = if app.automation.solar.weather_enabled {
                crate::automation::solar::Source::Weather
            } else {
                crate::automation::solar::Source::Manual
            };
            app.profiles
                .update(profiles::Message::Select(profiles::Selection::Preset(
                    app.adaptive_look.id(app.adaptive_phase),
                )));
        }
        app.restore_session_drafts();
        app.open_page_editor(page);
        if matches!(
            page,
            Page::Overview
                | Page::DesktopProfiles
                | Page::Themes
                | Page::Appearance
                | Page::DesktopDock
                | Page::DesktopIcons
                | Page::DesktopWindows
                | Page::DesktopAppearance
        ) || page.nav() == Page::Widgets
        {
            app.profiles.open_home();
        }
        let media = app.media_tasks();
        #[cfg(test)]
        view::profiles::prepare_look_images();
        let gallery = if view::profiles::look_images_ready() {
            Task::none()
        } else {
            cosmic::task::future(async {
                view::profiles::load_look_images().await;
                Message::LookImagesReady
            })
        };
        let tools = page
            .tool()
            .map(|tool| app.tools.open(tool))
            .unwrap_or_else(Task::none);
        let calendar = if page == Page::Calendar {
            app.calendar.check(false)
        } else {
            Task::none()
        };
        let motion = app.handle(Message::ReadSystemMotion);
        app.desktop_recovery.refresh();
        let installation = app.handle(Message::RefreshInstallation);
        let support = if page == Page::General
            && matches!(
                app.app_settings_page,
                AppSettingsPage::Help | AppSettingsPage::Restore
            ) {
            app.load_support()
        } else {
            Task::none()
        };
        (
            app,
            Task::batch([
                title,
                service,
                media,
                gallery,
                tools,
                calendar,
                motion,
                installation,
                support,
            ]),
        )
    }
    fn subscription(&self) -> Subscription<Message> {
        let paths = Config::path()
            .into_iter()
            .chain(Config::path().map(|p| crate::design::storage::path(&p)))
            .chain(settings_command_path())
            .chain(crate::usage::path())
            .chain(crate::layout_state::path())
            .chain(crate::tools::notes::path())
            .chain(crate::calendar::account_path())
            .chain(crate::calendar::client_path())
            .chain(crate::tools::focus::Session::path())
            .collect();
        let mut subscriptions = vec![
            cosmic::iced::event::listen_with(|event, _, _| match event {
                cosmic::iced::Event::PlatformSpecific(
                    cosmic::iced::event::PlatformSpecific::Wayland(
                        cosmic::iced::event::wayland::Event::Output(event, handle),
                    ),
                ) => Some(Message::DisplayOutput(event, handle)),
                _ => None,
            }),
            crate::watch::files(paths).map(|_| Message::Tick),
            window::resize_events().map(|(_, size)| Message::Resized(size)),
            self.config
                .color_subscription()
                .map(|_| Message::DesktopPaletteChanged),
        ];
        if self.page == Page::DesktopWallpaper && (self.config.rain.enabled || self.rain.preview) {
            subscriptions.push(
                crate::rain::lifecycle::watch().map(|s| Message::Rain(rain::Edit::Lifecycle(s))),
            );
        }
        if self
            .wizard
            .as_ref()
            .is_some_and(|w| w.step == wizard::Step::Arrange)
        {
            subscriptions.push(cosmic::iced::event::listen_with(|event, status, _| {
                wizard::widget_input(event, status).map(Message::Wizard)
            }));
        }
        if let Some(dir) = dirs::config_dir() {
            subscriptions.push(
                crate::watch::read_only_files(vec![
                    dir.join("dconf/user"),
                    dir.join("dconf/cosmic"),
                ])
                .map(|_| Message::ReadSystemMotion),
            );
        }
        if !(self.page == Page::DesktopShortcuts && self.shortcuts.recording()) {
            subscriptions.push(cosmic::iced::event::listen_with(|event, status, _| {
                use cosmic::iced::{Event, keyboard};
                match event {
                    Event::Window(window::Event::Focused) => Some(Message::RefreshInstallation),
                    Event::Keyboard(keyboard::Event::KeyPressed {
                        key: keyboard::Key::Named(keyboard::key::Named::Tab),
                        modifiers,
                        ..
                    }) if !modifiers.control()
                        && status == cosmic::iced::event::Status::Ignored =>
                    {
                        Some(Message::KeyboardFocus(modifiers.shift()))
                    }
                    Event::Keyboard(keyboard::Event::KeyPressed {
                        key: keyboard::Key::Named(keyboard::key::Named::F11),
                        ..
                    }) if status == cosmic::iced::event::Status::Ignored => {
                        Some(Message::Fullscreen)
                    }
                    Event::Keyboard(keyboard::Event::KeyPressed {
                        key: keyboard::Key::Character(ref key),
                        modifiers,
                        ..
                    }) if modifiers.control()
                        && key.eq_ignore_ascii_case("z")
                        && status == cosmic::iced::event::Status::Ignored =>
                    {
                        Some(Message::UndoDesktop)
                    }
                    Event::Keyboard(keyboard::Event::KeyPressed {
                        key: keyboard::Key::Character(key),
                        modifiers,
                        ..
                    }) if modifiers.control() && key.eq_ignore_ascii_case("f") => {
                        Some(Message::FocusSearch)
                    }
                    Event::Keyboard(keyboard::Event::KeyPressed {
                        key: keyboard::Key::Named(keyboard::key::Named::ArrowLeft),
                        modifiers,
                        ..
                    }) if modifiers.alt() => Some(Message::Back),
                    Event::Keyboard(keyboard::Event::KeyPressed {
                        key: keyboard::Key::Named(keyboard::key::Named::Escape),
                        ..
                    }) if status == cosmic::iced::event::Status::Ignored => {
                        Some(Message::SearchSettings(String::new()))
                    }
                    _ => None,
                }
            }));
        }
        if let Some(deadline) = self.save_at {
            let generation = self.edit_generation;
            subscriptions.push(
                crate::schedule::wake_at(deadline)
                    .with(generation)
                    .map(|(generation, _)| Message::CommitEdits(generation)),
            );
        }
        if let Some(deadline) = self.city_search.deadline {
            subscriptions.push(
                crate::schedule::wake_at(deadline)
                    .with(self.city_search.generation)
                    .map(|(generation, _)| Message::SearchCityAfter(generation)),
            );
        }
        if let Some(deadline) = self.schedule_city_search.deadline {
            subscriptions.push(
                crate::schedule::wake_at(deadline)
                    .with(("schedule-city", self.schedule_city_search.generation))
                    .map(|((_, generation), _)| Message::SearchScheduleCityAfter(generation)),
            );
        }
        if self.reorder.source.is_some() || self.devbox_reorder.source.is_some() {
            subscriptions.push(cosmic::iced::event::listen_with(
                |event, _, _| match event {
                    cosmic::iced::Event::Mouse(cosmic::iced::mouse::Event::ButtonReleased(
                        cosmic::iced::mouse::Button::Left,
                    )) => Some(Message::DragDrop),
                    cosmic::iced::Event::Mouse(cosmic::iced::mouse::Event::CursorLeft)
                    | cosmic::iced::Event::Window(window::Event::Unfocused)
                    | cosmic::iced::Event::Keyboard(cosmic::iced::keyboard::Event::KeyPressed {
                        key:
                            cosmic::iced::keyboard::Key::Named(
                                cosmic::iced::keyboard::key::Named::Escape,
                            ),
                        ..
                    }) => Some(Message::DragCancel),
                    _ => None,
                },
            ));
        }
        if self.placement_active() {
            subscriptions.push(cosmic::iced::event::listen_with(|event, status, _| {
                placement::input(event, status).map(Message::Placement)
            }));
        }

        if self.desktop.arranging() {
            // Only an active edit lease reads transient native drag results.
            subscriptions.push(
                cosmic::iced::time::every(std::time::Duration::from_millis(200)).map(|_| {
                    Message::DesktopEditor(desktop::Message::Islands(desktop::islands::Edit::Sync))
                }),
            );
            subscriptions.push(cosmic::iced::event::listen_with(|event, _, _| {
                if matches!(
                    event,
                    cosmic::iced::Event::Keyboard(cosmic::iced::keyboard::Event::KeyPressed {
                        key: cosmic::iced::keyboard::Key::Named(
                            cosmic::iced::keyboard::key::Named::Escape
                        ),
                        ..
                    })
                ) {
                    Some(Message::DesktopEditor(desktop::Message::Islands(
                        desktop::islands::Edit::Cancel,
                    )))
                } else {
                    None
                }
            }));
        }
        if self.page == Page::DesktopDock {
            let paths = self.desktop.watch_paths();
            if !paths.is_empty() {
                subscriptions.push(
                    crate::watch::files(paths)
                        .map(|_| Message::DesktopEditor(desktop::Message::Refresh)),
                );
            }
            if self.desktop.drag.active() {
                subscriptions.push(cosmic::iced::event::listen_with(|event, _, _| {
                    use cosmic::iced::{Event, keyboard, mouse};
                    let message = match event {
                        Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)) => {
                            // Visible zones handle valid releases themselves.
                            // Scrollables can suppress mouse-area exits when the
                            // pointer leaves their viewport; hover is not proof
                            // of a valid drop location.
                            desktop::Message::DragCancel
                        }
                        Event::Mouse(mouse::Event::CursorLeft)
                        | Event::Window(window::Event::Unfocused)
                        | Event::Keyboard(keyboard::Event::KeyPressed {
                            key: keyboard::Key::Named(keyboard::key::Named::Escape),
                            ..
                        }) => desktop::Message::DragCancel,
                        _ => return None,
                    };
                    Some(Message::DesktopEditor(message))
                }));
            }
        }
        if self.page == Page::DesktopAppearance {
            let paths = self.system_appearance.watch_paths();
            if !paths.is_empty() {
                subscriptions.push(
                    crate::watch::files(paths)
                        .map(|_| Message::SystemAppearance(system_appearance::Message::Refresh)),
                );
            }
        }
        if self.page == Page::DesktopWindows {
            let paths = self.windows.watch_paths();
            if !paths.is_empty() {
                subscriptions.push(
                    crate::watch::files(paths).map(|_| Message::Windows(windows::Message::Refresh)),
                );
            }
        }
        // Workspaces is edited on the Windows page.
        if matches!(self.page, Page::DesktopWindows | Page::DesktopWorkspaces) {
            let paths = self.workspaces.watch_paths();
            if !paths.is_empty() {
                subscriptions.push(
                    crate::watch::files(paths)
                        .map(|_| Message::Workspaces(workspaces::Message::Refresh)),
                );
            }
        }
        if self.page == Page::DesktopWindowRules {
            let paths = self.window_rules.watch_paths();
            if !paths.is_empty() {
                subscriptions.push(
                    crate::watch::files(paths)
                        .map(|_| Message::WindowRules(window_rules::Message::Refresh)),
                );
            }
        }
        if self.page == Page::DesktopWallpaper {
            let paths = self.wallpaper.watch_paths();
            if !paths.is_empty() {
                subscriptions.push(
                    crate::watch::read_only_files(paths)
                        .map(|_| Message::Wallpaper(wallpaper::Message::Refresh)),
                );
            }
        }
        if self.page == Page::DesktopIcons {
            let paths = self.desktop_icons.watch_paths();
            if !paths.is_empty() {
                subscriptions.push(
                    crate::watch::files(paths)
                        .map(|_| Message::DesktopIcons(desktop_icons::Message::Refresh)),
                );
            }
        }
        if self.page == Page::General
            && (self.support.recent_open || self.app_settings_page == AppSettingsPage::Help)
            && let Some(store) = self
                .support
                .local
                .store
                .as_ref()
                .or_else(|| self.desktop.store())
        {
            subscriptions.push(
                crate::watch::read_only_files(vec![
                    store.recent_change_path(),
                    crate::automation::path(store, crate::automation::FILE),
                    crate::automation::path(store, crate::automation::STATE_FILE),
                ])
                .map(|_| Message::Support(support::Message::Changed)),
            );
        }
        if matches!(
            self.page,
            Page::DesktopAutomation | Page::AdaptiveLooks | Page::Moments
        ) {
            let paths = self.automation.watch_paths();
            if !paths.is_empty() {
                subscriptions.push(
                    crate::watch::files(paths)
                        .map(|_| Message::Automation(automation::Message::Refresh)),
                );
            }
            // Displays come and go, and "today"/"tomorrow" move on.
            subscriptions.push(
                cosmic::iced::time::every(Duration::from_secs(15))
                    .map(|_| Message::Automation(automation::Message::Refresh)),
            );
        }
        if self.page == Page::DesktopShortcuts {
            let paths = self.shortcuts.watch_paths();
            if !paths.is_empty() {
                subscriptions.push(
                    crate::watch::files(paths)
                        .map(|_| Message::Shortcuts(shortcuts::Message::Refresh)),
                );
            }
        }
        if self.page == Page::General {
            subscriptions.push(
                cosmic::iced::time::every(Duration::from_secs(30)).map(|_| Message::RefreshService),
            );
        }
        // The preview uses the same seek notifications as the desktop card.
        if self.page == Page::Music {
            subscriptions.push(crate::tools::music::watch().map(|feed| {
                Message::Tools(tools::Message::Players(match feed {
                    crate::tools::music::Feed::Players(players) => Ok(players),
                    crate::tools::music::Feed::Unavailable(error) => Err(error),
                }))
            }));
        }
        // A live preview redraws once a second while it counts.
        if (self.page == Page::Focus && self.tools.focus.running())
            || (self.page == Page::Music
                && self.config.music.show_progress
                && self.config.music.visualizer.circle == crate::tools::visualizer::Circle::Off
                && self.tools.playing(&self.config))
        {
            subscriptions.push(
                cosmic::iced::time::every(Duration::from_secs(1))
                    .map(|_| Message::Tools(tools::Message::Redraw)),
            );
        }
        if capture_dir().is_some() {
            subscriptions
                .push(cosmic::iced::time::every(Duration::from_secs(1)).map(|_| Message::Tick));
            subscriptions.push(cosmic::iced::event::listen_with(
                |event, _, _| match event {
                    cosmic::iced::Event::Mouse(cosmic::iced::mouse::Event::CursorMoved {
                        position,
                    }) => Some(Message::CaptureCursor(position)),
                    _ => None,
                },
            ));
        }
        Subscription::batch(subscriptions)
    }
    fn view_window(&self, id: window::Id) -> Element<'_, Message> {
        if let Some((_, label)) = self
            .identified_displays
            .iter()
            .find(|(known, _)| *known == id)
        {
            return cosmic::widget::container(cosmic::widget::text::title3(label.clone()))
                .padding(20)
                .class(cosmic::theme::Container::Card)
                .into();
        }
        cosmic::widget::text::body("").into()
    }
    fn view(&self) -> Element<'_, Message> {
        use cosmic::Apply;
        let core = &self.core;
        let focused = core
            .focus_chain()
            .iter()
            .any(|id| Some(*id) == core.main_window_id());
        let maximized = core.window.is_maximized;
        let control = Message::Window;
        let mut header = cosmic::widget::header_bar()
            .title(core.window.header_title.as_str())
            .focused(focused)
            .maximized(maximized)
            .sharp_corners(core.window.sharp_corners)
            .on_drag(control(WindowControl::Drag))
            .on_right_click(control(WindowControl::Menu))
            .on_double_click(control(WindowControl::Maximize))
            .on_close(control(WindowControl::Close));
        if cosmic::config::show_maximize() {
            header = header.on_maximize(control(WindowControl::Maximize));
        }
        if cosmic::config::show_minimize() {
            header = header.on_minimize(control(WindowControl::Minimize));
        }
        let page = shortcuts::recording_view(
            self.settings_view(),
            self.page == Page::DesktopShortcuts && self.shortcuts.recording(),
        );
        cosmic::widget::column::with_children(vec![header.into(), page])
            .apply(cosmic::widget::container)
            .width(Length::Fill)
            .height(Length::Fill)
            .class(cosmic::theme::Container::custom(
                move |theme: &cosmic::Theme| {
                    let cosmic = theme.cosmic();
                    let base: cosmic::iced::Color =
                        cosmic.background(theme.transparent).base.into();
                    // Inside libcosmic's 1px window border, with its corners.
                    let radius = if maximized {
                        0.
                    } else {
                        let r = cosmic.radius_s()[0];
                        (if r < 4. { r } else { r + 4. } - 1.).max(0.)
                    };
                    cosmic::iced::widget::container::Style {
                        background: Some(cosmic::iced::Background::Color(cosmic::iced::Color {
                            a: 1.,
                            ..base
                        })),
                        text_color: Some(cosmic.background(theme.transparent).on.into()),
                        border: cosmic::iced::Border {
                            radius: radius.into(),
                            ..Default::default()
                        },
                        ..Default::default()
                    }
                },
            ))
            .into()
    }
    fn on_app_exit(&mut self) -> Option<Message> {
        self.updates.cancel();
        if self.native_busy || self.tools.saving() {
            return Some(Message::Window(WindowControl::Close));
        }
        self.flush_edits();
        if self.native_busy {
            // libcosmic dispatches this message instead of closing. The worker
            // drains queued Close only after its durable write completes.
            return Some(Message::Window(WindowControl::Close));
        }
        self.checkpoint_session_drafts();
        self.automation.release_hold();
        None
    }
    fn on_close_requested(&self, _: window::Id) -> Option<Message> {
        Some(Message::FlushEdits)
    }
    fn update(&mut self, message: Message) -> Task<Message> {
        if self.page == Page::Overview
            && self.canvas.is_none()
            && matches!(&message, Message::Placement(edit) if !matches!(edit,
                placement::Message::Select(_) | placement::Message::ClearSelection
                | placement::Message::Cancel | placement::Message::Undo | placement::Message::Escape
                | placement::Message::CancelDrag | placement::Message::Leave | placement::Message::Guides(_))
                && !(matches!(edit, placement::Message::Save) && self.legacy_layout_recovery()))
        {
            // Landing previews never commit gestures, including delayed input
            // from a canvas that was just applied or cancelled.
            return Task::none();
        }
        if self.config.design_problem.is_some() {
            match &message {
                Message::Design(_)
                | Message::Tools(tools::Message::NoteSaved(..))
                | Message::NativeDone(_)
                | Message::Window(_)
                | Message::Resized(_)
                | Message::FlushEdits => {}
                Message::Tick => return self.update_design(design::Message::Recheck),
                _ => return Task::none(),
            }
        }
        if matches!(message, Message::Window(WindowControl::Close)) {
            self.updates.cancel();
            if self.tools.saving() {
                self.tools.close_after_save = true;
                return Task::none();
            }
        }
        if let Message::Support(message) = message {
            return self.update_support(message);
        }
        if let Message::Updates(message) = message {
            return self.update_updates(message);
        }
        // Both Check again controls use the generation-ordered observation
        // pipeline. A desktop worker completion keeps the display snapshot and
        // must not be used to deliver a newly discovered installation status.
        let message = match message {
            Message::UndoDesktop
                if self
                    .wizard
                    .as_ref()
                    .is_some_and(|w| w.step == wizard::Step::Arrange) =>
            {
                Message::Wizard(wizard::Message::UndoWidgetMove)
            }
            Message::DesktopEditor(desktop::Message::Islands(desktop::islands::Edit::Check)) => {
                Message::RecheckInstallation
            }
            other => other,
        };
        // Bind legacy/programmatic primary actions at initial admission too.
        // A deferred message is already bound and must never be reinterpreted.
        let message = match message {
            Message::Wizard(
                edit @ (wizard::Message::Next
                | wizard::Message::Skip
                | wizard::Message::Start
                | wizard::Message::Finish
                | wizard::Message::Look(_)),
            ) => Message::Wizard(
                self.wizard
                    .as_ref()
                    .map_or(edit.clone(), |w| w.rendered(edit)),
            ),
            other => other,
        };
        if !self.native_busy
            && matches!(&message, Message::Wizard(edit) if matches!(edit.action(),
                wizard::Message::Next | wizard::Message::Start | wizard::Message::Look(_)))
        {
            if let Some(wizard) = self.wizard.as_mut() {
                wizard.refresh_widgets(&self.config, &self.displays);
            }
        }
        let _native = crate::desktop::islands::Installation::native_view(
            self.desktop.islands.installation.clone(),
        );
        if let Some(task) = self.dispatch_native(&message) {
            return Task::batch(
                std::iter::once(task).chain(std::mem::take(&mut self.native_tasks)),
            );
        }
        let revision = crate::desktop::store::revision();
        let checkpoint = checkpoint_message(&message);
        let scopes = self.preference_scope(&message);
        #[cfg(test)]
        let before = (!scopes.is_empty())
            .then(Config::try_load)
            .and_then(Result::ok);
        let refresh_undo =
            desktop_write_message(&message) || matches!(message, Message::Tick | Message::Page(_));
        let completion = direct::completion(&message)
            .filter(|completion| {
                self.canvas.is_none() || !matches!(completion, Message::Placement(_))
            })
            .filter(|completion| {
                // A color choice made inside a wallpaper preview belongs to its
                // single Use action, including asynchronous color analysis.
                !matches!(
                    completion,
                    Message::Wallpaper(wallpaper::Message::ApplyColors)
                ) || (!self.wallpaper.background_dirty()
                    && (!matches!(
                        message,
                        Message::Wallpaper(wallpaper::Message::AccentSubmit)
                    ) || self.wallpaper.valid_accent_input()))
            });
        if let Message::DesktopEditor(edit) = &message {
            self.native_property |= direct::bar_property(self.desktop.selected, edit);
        }
        let native_writable = match &message {
            Message::DesktopEditor(_) => self.desktop.writable(),
            Message::SystemAppearance(_) => self.system_appearance.writable(),
            _ => true,
        };
        let property = direct::property(self, &message);
        let native_property_action = matches!(
            message,
            Message::DesktopEditor(_) | Message::SystemAppearance(_)
        );
        let prior_action = (property != 0)
            .then(|| {
                crate::desktop::actions::history_store()
                    .ok()
                    .and_then(|s| s.latest_change().ok())
                    .flatten()
                    .map(|v| v.id)
            })
            .flatten();
        let label = if scopes.len() == 1 {
            crate::desktop::store::widget::label(&scopes[0])
        } else {
            direct::label(&message)
        };
        let starts_gesture = matches!(
            &message,
            Message::Rain(rain::Edit::Draft(_))
                | Message::AppearanceValue(..)
                | Message::Notice(_)
                | Message::Placement(
                    placement::Message::Press(_) | placement::Message::ResizePress(_)
                )
        );
        if starts_gesture && self.gesture_config.is_none() {
            self.gesture_config = Config::path()
                .and_then(|p| crate::config::read_config_text(&p).ok())
                .and_then(|t| crate::config::parse_config_text(t.as_deref()).ok());
        }
        let finishes_gesture = matches!(
            &message,
            Message::Rain(rain::Edit::Commit | rain::Edit::Set(_))
                | Message::SaveAppearance
                | Message::SaveNotice
                | Message::FlushEdits
                | Message::Page(_)
                | Message::Placement(placement::Message::Release | placement::Message::Save)
        );
        let baseline = finishes_gesture
            .then(|| self.gesture_config.clone())
            .flatten();
        let task = crate::desktop::actions::in_action(&label, || {
            crate::desktop::actions::property(property, || {
                crate::desktop::actions::guarded(baseline.as_ref(), || {
                    let task = self.handle(message);
                    let completed = self.completed_control(completion);
                    Task::batch([task, completed])
                })
            })
        });
        if property != 0 {
            let failed = !native_writable
                || self.error
                || (native_property_action
                    && (matches!(self.desktop.notice, Some(desktop::Notice::Error(_)))
                        || matches!(
                            self.system_appearance.notice,
                            Some(desktop::Notice::Error(_))
                        )));
            let current_action = crate::desktop::actions::history_store()
                .ok()
                .and_then(|s| s.latest_change().ok())
                .flatten()
                .map(|v| v.id);
            if !failed && current_action == prior_action {
                if let Err(error) = crate::desktop::actions::complete_property(property, &label) {
                    self.failed(error);
                }
            }
            self.native_property = 0;
        }
        if finishes_gesture {
            self.gesture_config = None;
        }
        let task = if crate::desktop::store::revision() != revision {
            Task::batch([task, self.after_desktop_change()])
        } else {
            task
        };

        if matches!(self.page, Page::Calendar | Page::Projects) {
            self.samples.update(&self.config);
        }
        #[cfg(test)]
        if let Some(before) = before
            && !self.error
        {
            if let Ok(after) = Config::try_load()
                && let Some(undo) =
                    navigation::PreferenceUndo::between_scoped(&before, &after, &scopes)
            {
                self.preference_undo = Some(undo);
            }
        }
        if refresh_undo || crate::desktop::store::revision() != revision {
            self.last_desktop_change = navigation::latest_desktop_change();
        }
        if checkpoint {
            self.checkpoint_session_drafts();
        }
        // Only actual overlapping gestures can defer scheduled switches.
        if self.automation.hold_matters() {
            // Only in-flight pointer gestures hold automation. Local previews,
            // incomplete fields and recovered forms have no runtime meaning.
            let parts = if self.placement.dragging() {
                [crate::desktop::profiles::Domain::WidgetLayout]
                    .into_iter()
                    .collect()
            } else {
                Default::default()
            };
            self.automation.sync_hold(parts);
        }
        let depth = self.depth_preview_sync();
        Task::batch(
            [task, depth]
                .into_iter()
                .chain(std::mem::take(&mut self.native_tasks)),
        )
    }
}

impl Settings {
    fn handle(&mut self, message: Message) -> Task<Message> {
        if let Some(task) = self.dispatch_native(&message) {
            return task;
        }
        match message {
            Message::Updates(message) => return self.update_updates(message),
            Message::Support(message) => return self.update_support(message),
            Message::KeyboardFocus(backwards) => return navigation::keyboard_focus(backwards),
            Message::Fullscreen => return self.core.toggle_maximize(None),
            Message::ShareUsage(enabled) => return self.change_usage(enabled),
            Message::UsageSaved(result) => {
                self.usage_busy = false;
                self.usage_error = result.err();
                self.refresh_usage();
            }
            Message::OpenPrivacy => {
                return run_command(
                    "/usr/bin/xdg-open".into(),
                    vec!["https://github.com/profilaz3/halcyon/blob/main/docs/PRIVACY.md".into()],
                    "Privacy details opened",
                );
            }
            Message::DisplayOutput(event, handle) => {
                self.displays.update(event, handle);
                if let Some(wizard) = self.wizard.as_mut() {
                    wizard.refresh_widgets(&self.config, &self.displays);
                }
                self.placement.update_displays(&self.config, &self.displays);
                self.refresh_canvas();
                if std::env::var_os("DESKTOP_CARDS_DESKTOP_ROOT").is_none() {
                    let generation = self.displays.generation;
                    return cosmic::task::future(async move {
                        let ids = tokio::task::spawn_blocking(crate::displays::physical_identities)
                            .await
                            .unwrap_or_default();
                        Message::DisplayIdentities(generation, ids)
                    });
                }
            }
            Message::DisplayIdentities(generation, ids) => {
                if self.displays.accept_identities(generation, ids) {
                    if let Some(wizard) = self.wizard.as_mut() {
                        wizard.refresh_widgets(&self.config, &self.displays);
                    }
                    self.placement.update_displays(&self.config, &self.displays);
                    self.refresh_canvas();
                }
            }
            Message::MonitorPreview(display) => self.select_monitor_preview(display),
            Message::IdentifyDisplays => return self.identify_displays(),
            Message::ClearDisplayLabels(ids) => {
                self.identified_displays.retain(|(id, _)| !ids.contains(id));
                return Task::batch(ids.into_iter().map(cosmic::iced::platform_specific::shell::commands::layer_surface::destroy_layer_surface));
            }
            Message::WidgetMonitor(widget, display) => {
                if display
                    .as_deref()
                    .is_some_and(|d| self.displays.unique(d).is_none())
                {
                    self.failed("That display is no longer connected.".into());
                } else {
                    self.save_widget_monitor(widget, "display", serde_json::json!(display));
                }
            }
            Message::WidgetMonitorHide(widget, hide) => {
                self.save_widget_monitor(widget, "hide_disconnected", hide.into())
            }
            Message::WidgetDisplay(name) => {
                if name != self.config.widget_display
                    && name
                        .as_deref()
                        .is_some_and(|name| self.displays.unique(name).is_none())
                {
                    self.failed(
                        "That display is no longer available with a unique connection name.".into(),
                    );
                } else {
                    self.placement.leave();
                    if self.placement.dirty() {
                        let _ = self.update_placement(placement::Message::Save);
                    }
                    if !self.placement.dirty() {
                        self.save(serde_json::json!({"widget_display": name}));
                        self.placement.refresh(&self.config);
                        self.refresh_canvas();
                    }
                }
            }
            Message::Preview => return Task::none(),
            Message::NativeDone(done) => return self.finish_native(done),
            Message::InstallationChecked(generation, mut state, runtime) => {
                if generation != self.installation_generation {
                    return Task::none();
                }
                if matches!(state, crate::desktop::islands::Installation::Pending)
                    && self.desktop.islands.installation.installed()
                {
                    state = self.desktop.islands.installation.checking();
                }
                self.installation_busy = false;
                self.installation_retry = state.needs_recheck();
                let _ = self.handle(Message::InstallationReady(generation, state));
                self.material_native = runtime.material;
                self.desktop.accept_runtime(runtime);
            }
            Message::Calendar(message) => return self.update_calendar(message),
            Message::Wizard(message) => return self.update_wizard(message),
            Message::OpenWizard => {
                self.wizard = Some(open_wizard());
            }
            Message::SetupOpen(open) => {
                let navigate = self.handle(Message::Page(Page::Overview));
                self.setup_requested = open;
                return navigate;
            }
            Message::SetupDismissed(on) => {
                self.save(json!({"setup_dismissed":on}));
                return self.handle(Message::SetupOpen(!on));
            }
            Message::DismissSetupTask(task) => match self.setup_followups.dismiss(task) {
                Ok(config) => {
                    self.config = config;
                    self.saved();
                }
                Err(error) => self.failed(error),
            },
            Message::SearchSettings(query) => {
                if !query.trim().is_empty() {
                    self.placement.leave();
                    self.reorder.cancel();
                    self.devbox_reorder.cancel();
                    self.desktop.drag.cancel();
                }
                self.search_query = query;
                self.search_selected = None;
            }
            Message::SearchFirst => {
                if let Some(hit) = navigation::search(&self.search_query).first().cloned() {
                    return self.handle(Message::SearchResult(hit));
                }
            }
            Message::SearchResult(hit) => {
                let navigation = self.handle(Message::Page(hit.page));
                if hit.page == Page::General {
                    self.app_settings_page = hit.app_destination();
                    navigation::remember_app_destination(self.app_settings_page);
                }
                if hit.open == navigation::SearchOpen::City {
                    self.city_editing = true;
                }
                if let navigation::SearchOpen::Metric(metric) = hit.open {
                    if !self.devbox_details.contains(&metric) {
                        self.devbox_details.push(metric);
                    }
                }
                if hit.advanced && !self.advanced_pages.contains(&hit.page) {
                    self.advanced_pages.push(hit.page);
                }
                if hit.advanced {
                    match hit.page {
                        Page::DesktopAppearance if hit.open == navigation::SearchOpen::Effects => {
                            self.system_appearance.effects = true
                        }
                        Page::DesktopAppearance => self.system_appearance.advanced_colors = true,
                        Page::DesktopDock => self.desktop.advanced = true,
                        _ => {}
                    }
                }
                self.prepare_search_control(hit);
                self.search_selected = Some(hit);
                let support = if hit.page == Page::General
                    && matches!(
                        self.app_settings_page,
                        AppSettingsPage::Help | AppSettingsPage::Restore
                    ) {
                    self.support.recent_open = hit.open == navigation::SearchOpen::Recent;
                    self.support.report = None;
                    self.load_support()
                } else {
                    Task::none()
                };
                return Task::batch([
                    navigation,
                    support,
                    cosmic::task::future(async move {
                        tokio::time::sleep(Duration::from_millis(40)).await;
                        Message::RevealSearch(hit)
                    }),
                ]);
            }
            Message::RevealSearch(hit) => {
                if self.search_selected == Some(hit) && self.search_query.is_empty() {
                    return navigation::reveal(hit.anchor());
                }
            }
            Message::FocusSearch => {
                return cosmic::widget::text_input::focus(navigation::search_id());
            }
            Message::NavSelected(id) => {
                if let Some(page) = self.nav_model.data::<Page>(id).copied() {
                    return self.handle(Message::Page(self.root_destinations[page.root_index()]));
                }
            }
            Message::Back => {
                if self.wizard.is_some() {
                    return self.update_wizard(wizard::Message::Back);
                }
                if self.page == Page::General && self.support.recent_open {
                    return self.update_support(support::Message::Recent(false));
                }
                if self.page == Page::General && self.support.report.is_some() {
                    return self.update_support(support::Message::Preview(false));
                }
                if !self.search_query.trim().is_empty() {
                    return self.handle(Message::SearchSettings(String::new()));
                }
                if self.page == Page::DesktopProfiles && self.profiles.package.is_some() {
                    return self.handle(Message::Profiles(profiles::Message::Package(
                        profiles::packages::Edit::Close,
                    )));
                }
                if self.page == Page::DesktopProfiles
                    && (self.profiles.mix.is_some()
                        || self.profiles.profile().is_some()
                        || self.profiles.selection() != &profiles::Selection::Current)
                {
                    return self.handle(Message::Profiles(profiles::Message::Discard));
                }
                if self.page == Page::Overview && self.setup_requested {
                    return self.handle(Message::SetupOpen(false));
                }
                if self.page == Page::Themes && self.style_browser {
                    return self.handle(Message::StyleBrowser(false));
                }
                let target = self.page.parent().unwrap_or(Page::Overview);
                if target != self.page {
                    return self.handle(Message::Page(target));
                }
            }
            Message::TryPreset(id) => {
                let navigate = self.handle(Message::Page(Page::DesktopProfiles));
                if *self.profiles.selection() == profiles::Selection::Preset(id) {
                    return navigate;
                }
                let select = self.handle(Message::Profiles(profiles::Message::Select(
                    profiles::Selection::Preset(id),
                )));
                return Task::batch([navigate, select]);
            }
            Message::OpenBar(kind) => {
                let navigate = self.handle(Message::Page(Page::DesktopDock));
                let select = self.handle(Message::DesktopEditor(desktop::Message::Select(kind)));
                return Task::batch([navigate, select]);
            }
            Message::ToggleAdvanced(page) => {
                if self.advanced_pages.contains(&page) {
                    self.advanced_pages.retain(|p| *p != page);
                } else {
                    self.advanced_pages.push(page);
                }
            }
            Message::UndoDesktop if self.canvas.is_some() && self.page == Page::Overview => {
                return self.update_canvas(canvas::Message::Undo);
            }
            Message::Canvas(event) => return self.update_canvas(event),
            Message::UndoDesktop => {
                let id = self
                    .last_desktop_change
                    .as_ref()
                    .map(|(_, _, id)| id.clone());
                let task = self.undo_desktop_change();
                if !self.native_busy && !self.error {
                    self.wizard_undo_completed(id);
                }
                return task;
            }
            Message::UndoDesktopShown(shown) => {
                // A rendered historical action must not skip an unfinished
                // gesture or flush it and silently retarget its captured ID.
                if !self.appearance_pending.is_empty() || self.notice_editing {
                    self.failed("Finish the current edit before undoing a previous change.".into());
                    return Task::none();
                }
                if self.last_desktop_change.as_ref().map(|(_, _, id)| id) != shown.as_ref() {
                    self.failed("The last settings action changed. Review Undo again.".into());
                    return Task::none();
                }
                let task = self.undo_shown_change(shown.clone());
                if !self.native_busy && !self.error {
                    self.wizard_undo_completed(shown);
                }
                return task;
            }
            Message::UndoDesktopRestore(expected) => {
                if self
                    .last_desktop_change
                    .as_ref()
                    .is_none_or(|(_, _, id)| *id != expected)
                {
                    self.failed("The last settings action changed. Review Undo again.".into());
                    return Task::none();
                }
                let task = self.undo_shown_change(Some(expected.clone()));
                if !self.native_busy && !self.error {
                    self.wizard_undo_completed(Some(expected));
                }
                return task;
            }
            Message::ResolveUndoShown(review, restore) => {
                return self.resolve_undo_review(review, restore);
            }
            Message::RecoveryText(index, field, text) => {
                if let Some(row) = self.recovery_sheet.rows.get_mut(index) {
                    if let Err(error) = row.set_text(field, text) {
                        self.failed(error);
                    }
                }
            }
            Message::RecoverySelect(index, selected) => {
                if let Some(row) = self.recovery_sheet.rows.get_mut(index) {
                    row.selected = selected;
                }
            }
            Message::RecoveryUseProposed(index) => {
                if let Some(row) = self.recovery_sheet.rows.get_mut(index) {
                    row.use_proposed = true;
                    row.review();
                }
            }
            Message::RecoveryDiscard(index) => {
                if let Err(error) = self.recovery_sheet.discard(index) {
                    self.failed(error);
                }
            }
            Message::RecoveryApplyOne(index) => match self.recovery_sheet.apply_one(index) {
                Ok(()) => {
                    self.error = false;
                    self.status.clear();
                    return self.after_desktop_change();
                }
                Err(error) => self.failed(error),
            },
            Message::RecoveryApply => match self.recovery_sheet.apply() {
                Ok(()) => {
                    self.error = false;
                    return self.after_desktop_change();
                }
                Err(error) => self.failed(error),
            },
            Message::RecoveryOpen(index) => return self.open_recovered_form(index),
            Message::UndoPreferences => {
                let id = self
                    .last_desktop_change
                    .as_ref()
                    .map(|(_, _, id)| id.clone());
                self.undo_preferences();
                if !self.native_busy && !self.error {
                    self.wizard_undo_completed(id);
                }
            }
            Message::EditWidgetLayout(widget) => {
                let widget = if self.config.glance.enabled
                    && matches!(widget, Widget::Clock | Widget::Weather | Widget::Calendar)
                {
                    Widget::Glance
                } else {
                    widget
                };
                let navigation = self.handle(Message::Page(Page::Overview));
                let selection =
                    self.update_placement(placement::Message::Select(widget.key().into()));
                let editor = if self.legacy_layout_recovery() {
                    Task::none()
                } else {
                    self.update_canvas(canvas::Message::Start)
                };
                return Task::batch([navigation, selection, editor]);
            }
            Message::Placement(message) => {
                // The widget list and the miniature share one pointer; a
                // column reorder in progress owns it until drop or cancel.
                if self.reorder.source.is_some()
                    || self.devbox_reorder.source.is_some()
                    || (!self.placement_active()
                        && !(self.legacy_layout_recovery()
                            && matches!(
                                message,
                                placement::Message::Save
                                    | placement::Message::Cancel
                                    | placement::Message::Undo
                            )))
                {
                    return Task::none();
                }
                let recovering =
                    self.legacy_layout_recovery() && matches!(message, placement::Message::Save);
                let task = self.update_placement(message);
                if recovering && !self.error && !self.placement.dirty() {
                    self.finish_layout_recovery();
                }
                return task;
            }
            Message::DesktopEditor(message) => {
                if self.page == Page::DesktopDock
                    && let Some(id) = self.desktop.update(message)
                {
                    // Keyboard focus follows the moved applet.
                    return cosmic::widget::button::focus(id);
                }
            }
            Message::Windows(message) => {
                if self.page == Page::DesktopWindows {
                    self.update_windows(message);
                }
            }
            Message::Workspaces(message) => {
                if matches!(self.page, Page::DesktopWindows | Page::DesktopWorkspaces) {
                    self.workspaces.update(message);
                }
            }
            Message::Motion(message) => return self.update_motion(message),
            Message::SidebarSlide(on) => self.save(json!({"sidebar_slide": on})),
            Message::SettingsAnimations(on) => self.save(json!({"settings_animations": on})),
            Message::ReadSystemMotion => {
                self.motion_generation = self.motion_generation.wrapping_add(1);
                let generation = self.motion_generation;
                return cosmic::task::future(async move {
                    let on = tokio::task::spawn_blocking(|| {
                        crate::desktop::DesktopStore::from_env()
                            .ok()
                            .and_then(|store| {
                                crate::desktop::motion::AppAnimations::for_store(&store)
                                    .read()
                                    .ok()
                            })
                            .unwrap_or(false)
                    })
                    .await
                    .unwrap_or(false);
                    Message::SystemMotion(generation, on)
                });
            }
            Message::SystemMotion(generation, on) => {
                if generation == self.motion_generation {
                    self.system_animations = on;
                }
            }
            Message::WindowRules(message) => {
                if self.page == Page::DesktopWindowRules {
                    self.window_rules.update(message);
                }
            }
            Message::Shortcuts(message) => {
                if self.page == Page::DesktopShortcuts {
                    self.shortcuts.update(message);
                }
            }
            Message::Wallpaper(message) => return self.update_wallpaper(message),
            Message::ColorWell(message) => return self.update_color_well(message),
            Message::FontMenu(target) => {
                self.font_menu = if self.font_menu == target {
                    None
                } else {
                    target
                };
                self.font_search.clear();
                self.font_page = 0;
                if self.font_menu.is_some() {
                    self.profiles.open_home();
                    let media = self.media_tasks();
                    if self.font_catalog.is_none() && !self.font_loading {
                        self.font_loading = true;
                        let fonts = cosmic::task::future(async {
                            Message::FontCatalogReady(
                                tokio::task::spawn_blocking(
                                    crate::desktop::appearance::catalog::FontCatalog::installed,
                                )
                                .await
                                .unwrap_or_default(),
                            )
                        });
                        return Task::batch([media, fonts]);
                    }
                    return media;
                }
            }
            Message::FontSearch(text) => {
                self.font_search = text;
                self.font_page = 0;
            }
            Message::FontPage(page) => {
                let total = self.widget_font_families().len();
                self.font_page = page.min(total.saturating_sub(1) / FONT_PAGE_SIZE);
            }
            Message::FontCatalogReady(catalog) => {
                self.font_loading = false;
                self.font_catalog = Some(catalog);
                self.font_page = 0;
            }
            Message::LookImagesReady => {}
            Message::Window(control) => return native_jobs::window_action(control),
            Message::WidgetFont(target, family)
                if self.canvas.is_some()
                    && self.page == Page::Overview
                    && matches!(target, FontTarget::Text | FontTarget::Clock) =>
            {
                self.font_menu = None;
                let pointer = if target == FontTarget::Clock {
                    "/clock_font"
                } else {
                    "/widget_font"
                };
                return self.update_canvas(canvas::Message::Style(pointer, json!(family)));
            }
            Message::WidgetFont(target, family) => {
                self.font_menu = None;
                self.font_search.clear();
                match target {
                    FontTarget::Text => self.save(json!({ "widget_font": family })),
                    FontTarget::Clock => self.save(json!({ "clock_font": family })),
                    FontTarget::GlanceText => self.save_glance(json!({ "font": family })),
                    FontTarget::GlanceClock => self.save_glance(json!({ "clock_font": family })),
                }
            }
            Message::DesktopIcons(message) => {
                if self.page == Page::DesktopIcons {
                    self.desktop_icons.update(message);
                }
            }
            Message::SystemAppearance(message) => {
                if matches!(self.page, Page::DesktopAppearance | Page::Themes)
                    && let Some(id) = self.system_appearance.update(message)
                {
                    // A picker opened: focus its search and bring it into view.
                    return Task::batch([
                        cosmic::widget::text_input::focus(id),
                        system_appearance::reveal_picker(cosmic::widget::Id::new(
                            "settings-content",
                        )),
                    ]);
                }
            }
            Message::Profiles(message) => return self.update_profiles(message),
            Message::Tools(message) => return self.update_tools(message),
            Message::AdaptivePreview(look, phase) => {
                if self.page != Page::AdaptiveLooks {
                    return Task::none();
                }
                self.adaptive_look = look;
                self.adaptive_phase = phase;
                return self.update_profiles(profiles::Message::Select(
                    profiles::Selection::Preset(look.id(phase)),
                ));
            }
            Message::AdaptiveSource(source) => {
                self.adaptive_source = source;
            }
            Message::Automation(message) => return self.update_automation(message),
            Message::AppSettingsPage(page) => {
                let navigate = if self.page != Page::General {
                    self.handle(Message::Page(Page::General))
                } else {
                    Task::none()
                };
                self.app_settings_page = page;
                self.support.recent_open = false;
                self.support.report = None;
                let support = if matches!(page, AppSettingsPage::Help | AppSettingsPage::Restore) {
                    self.load_support()
                } else {
                    Task::none()
                };
                self.search_selected = None;
                navigation::remember_app_destination(page);
                return Task::batch([
                    navigate,
                    support,
                    cosmic::iced::widget::scrollable::snap_to(
                        cosmic::widget::Id::new("settings-content"),
                        cosmic::iced::widget::scrollable::RelativeOffset {
                            x: None,
                            y: Some(0.),
                        },
                    ),
                ]);
            }
            Message::ToggleScene => {
                if self.collapsed_scenes.contains(&self.page) {
                    self.collapsed_scenes.retain(|page| *page != self.page);
                } else {
                    self.collapsed_scenes.push(self.page);
                }
            }
            Message::ToggleSceneObjects => {
                self.scene_objects_open = !self.scene_objects_open;
            }
            Message::ContentScrolled(y) => {
                if self.search_query.is_empty() {
                    self.page_scroll.insert(self.page.key().into(), y);
                }
                return Task::none();
            }
            Message::Resized(size) => self.window = size,
            Message::Tick => {
                self.refresh_usage();
                self.calendar.refresh();
                self.setup_followups = wizard::follow_up::State::load();
                match Config::try_load() {
                    Ok(config) => self.config = config,
                    Err(error)
                        if Config::path()
                            .as_deref()
                            .is_some_and(crate::design::storage::managed) =>
                    {
                        self.config.design_problem = Some(error);
                        self.design = design::State::load();
                        return Task::none();
                    }
                    Err(_) => {}
                }
                self.placement.refresh(&self.config);
                self.refresh_canvas();
                if let Some(wizard) = self.wizard.as_mut() {
                    wizard.refresh_widgets(&self.config, &self.displays);
                }
                if self.config.has_custom_positions() || self.placement.dirty() {
                    self.reorder.cancel();
                }
                if !self.appearance_editing {
                    self.appearance_draft = self.config.appearance.clone();
                }
                if !self.notice_editing {
                    self.notice = self.config.meeting_notice_minutes.clamp(1, 60) as u16;
                }
                self.tools.refresh();
                let mut tasks = Vec::new();
                if let Some(path) = settings_command_path() {
                    if let Ok(bytes) = std::fs::read(&path) {
                        let _ = std::fs::remove_file(path);
                        if let Ok(value) = serde_json::from_slice::<serde_json::Value>(&bytes) {
                            if let Some(page) = value["page"].as_str() {
                                tasks.push(self.update(Message::Page(Page::parse(page))));
                            }
                            // Capture audits drive the editor through its own
                            // messages instead of synthesizing desktop pointer input.
                            let commands = capture_commands();
                            if commands {
                                if let Some(state) = value["updates_fixture"].as_str() {
                                    self.updates.capture_fixture(state);
                                }
                                if let Some(action) = value["support"].as_str() {
                                    let action = match action {
                                        "report" => Some(support::Message::Preview(true)),
                                        "recent" => Some(support::Message::Recent(true)),
                                        "refresh" => Some(support::Message::Refresh),
                                        _ => None,
                                    };
                                    if let Some(action) = action {
                                        tasks.push(self.update(Message::Support(action)));
                                    }
                                }
                                if value["recent_details"] == true
                                    && let Some(entry) = self.support.local.recent.entries.last()
                                {
                                    tasks.push(self.update(Message::Support(
                                        support::Message::Details(Some(entry.id.clone())),
                                    )));
                                }
                                if value["toggle_scene"] == true {
                                    tasks.push(self.update(Message::ToggleScene));
                                }
                                if let Some(page) = value["app_settings"].as_str().and_then(|key| {
                                    AppSettingsPage::ALL.into_iter().find(|p| p.key() == key)
                                }) {
                                    tasks.push(self.update(Message::AppSettingsPage(page)));
                                }
                                // The same shortcuts used by Home and the
                                // keyboard, confined to isolated capture mode.
                                if value["back"] == true {
                                    tasks.push(self.update(Message::Back));
                                }
                                if let Some(preset) = value["try_preset"]
                                    .as_str()
                                    .and_then(crate::desktop::profiles::preset)
                                {
                                    tasks.push(self.update(Message::TryPreset(preset.id)));
                                }
                                if let Some(kind) = value["open_bar"].as_str().and_then(|name| {
                                    use crate::desktop::panels::PanelKind;
                                    match name {
                                        "dock" => Some(PanelKind::Dock),
                                        "panel" => Some(PanelKind::Panel),
                                        _ => None,
                                    }
                                }) {
                                    tasks.push(self.update(Message::OpenBar(kind)));
                                }
                                if let Some([widget, on]) =
                                    value["widget"].as_array().map(Vec::as_slice)
                                    && let (Some(widget), Some(on)) =
                                        (widget.as_str().and_then(Widget::parse), on.as_bool())
                                {
                                    tasks.push(self.update(Message::Widget(widget, on)));
                                }
                                if let Some(query) = value["search"].as_str() {
                                    tasks.push(self.update(Message::SearchSettings(query.into())));
                                }
                                if value["search_first"] == true {
                                    tasks.push(self.update(Message::SearchFirst));
                                }
                                if value["undo_preferences"] == true {
                                    tasks.push(self.update(Message::UndoPreferences));
                                }
                                if let Some(theme) = value["widget_theme"].as_str() {
                                    tasks.push(self.update(Message::Theme(theme.into())));
                                }
                                if let Some(source) = value["widget_colors"].as_str() {
                                    if let Some(source) = match source {
                                        "desktop" => Some(crate::config::ColorSource::Desktop),
                                        "custom" => Some(crate::config::ColorSource::Custom),
                                        _ => None,
                                    } {
                                        tasks.push(self.update(Message::WidgetColorSource(source)));
                                    }
                                }
                                for event in value["glance"].as_array().into_iter().flatten() {
                                    if let Some(message) = capture_glance(event) {
                                        tasks.push(self.update(message));
                                    }
                                }
                                if let Some(edit) = recipes::capture(&value["recipe"]) {
                                    tasks.push(self.update(Message::Recipe(edit)));
                                }
                                if let Some(edit) = moments::capture(&value["moment"]) {
                                    tasks.push(self.update(Message::Moment(edit)));
                                }
                                if let Some(command) = value["canvas"].as_str() {
                                    let edit = match command {
                                        "start" => Some(canvas::Message::Start),
                                        "cancel" => Some(canvas::Message::Cancel),
                                        "undo" => Some(canvas::Message::Undo),
                                        "apply" => self
                                            .canvas
                                            .as_ref()
                                            .map(|draft| canvas::Message::Apply(draft.id)),
                                        _ => None,
                                    };
                                    if let Some(edit) = edit {
                                        tasks.push(self.update(Message::Canvas(edit)));
                                    }
                                }
                                if let Some(edit) = canvas::capture_style(&value["canvas_style"]) {
                                    tasks.push(self.update(Message::Canvas(edit)));
                                }
                                for event in value["placement"].as_array().into_iter().flatten() {
                                    if let Some(message) = capture_placement(event) {
                                        if let placement::Message::Press(key) = &message {
                                            self.placement.select(key);
                                            continue;
                                        }
                                        tasks.push(self.update(Message::Placement(message)));
                                    }
                                }
                                if self.page == Page::DesktopDock {
                                    let fixture = self
                                        .desktop
                                        .store()
                                        .and_then(crate::desktop::DesktopStore::fixture_root)
                                        .is_some_and(std::path::Path::is_dir);
                                    for event in value["desktop"].as_array().into_iter().flatten() {
                                        if let Some(message) =
                                            capture_desktop(event).filter(|message| {
                                                capture_allows(message.writes(), fixture)
                                            })
                                        {
                                            tasks
                                                .push(self.update(Message::DesktopEditor(message)));
                                        }
                                    }
                                }
                                if self.page == Page::DesktopAppearance {
                                    let fixture = self
                                        .system_appearance
                                        .store()
                                        .and_then(crate::desktop::DesktopStore::fixture_root)
                                        .is_some_and(std::path::Path::is_dir);
                                    for event in
                                        value["appearance"].as_array().into_iter().flatten()
                                    {
                                        if let Some(message) =
                                            capture_appearance(event).filter(|message| {
                                                capture_allows(message.writes(), fixture)
                                            })
                                        {
                                            tasks.push(
                                                self.update(Message::SystemAppearance(message)),
                                            );
                                        }
                                    }
                                }
                                if matches!(self.page, Page::DesktopProfiles | Page::Themes) {
                                    for event in value["profiles"].as_array().into_iter().flatten()
                                    {
                                        tasks.extend(self.capture_profiles(event));
                                    }
                                }
                                if self.page == Page::DesktopWindows {
                                    let fixture = self
                                        .windows
                                        .store()
                                        .and_then(crate::desktop::DesktopStore::fixture_root)
                                        .is_some_and(std::path::Path::is_dir);
                                    for event in value["windows"].as_array().into_iter().flatten() {
                                        if let Some(message) =
                                            capture_windows(event).filter(|message| {
                                                capture_allows(message.writes(), fixture)
                                            })
                                        {
                                            tasks.push(self.update(Message::Windows(message)));
                                        }
                                    }
                                }
                                if matches!(
                                    self.page,
                                    Page::DesktopWindows | Page::DesktopWorkspaces
                                ) {
                                    let fixture = self
                                        .workspaces
                                        .store()
                                        .and_then(crate::desktop::DesktopStore::fixture_root)
                                        .is_some_and(std::path::Path::is_dir);
                                    for event in
                                        value["workspaces"].as_array().into_iter().flatten()
                                    {
                                        if let Some(message) =
                                            capture_workspaces(event).filter(|message| {
                                                capture_allows(message.writes(), fixture)
                                            })
                                        {
                                            tasks.push(self.update(Message::Workspaces(message)));
                                        }
                                    }
                                }
                                if self.page == Page::DesktopMotion {
                                    let fixture = crate::desktop::DesktopStore::from_env()
                                        .ok()
                                        .and_then(|store| {
                                            store.fixture_root().map(|root| root.is_dir())
                                        })
                                        .unwrap_or(false);
                                    for event in value["motion"].as_array().into_iter().flatten() {
                                        if let Some((message, writes)) = capture_motion(event)
                                            && capture_allows(writes, fixture)
                                        {
                                            tasks.push(self.update(message));
                                        }
                                    }
                                }
                                if self.page == Page::DesktopWindowRules {
                                    let fixture = self
                                        .window_rules
                                        .store()
                                        .and_then(crate::desktop::DesktopStore::fixture_root)
                                        .is_some_and(std::path::Path::is_dir);
                                    for event in
                                        value["window_rules"].as_array().into_iter().flatten()
                                    {
                                        if let Some(message) =
                                            capture_window_rules(event).filter(|message| {
                                                capture_allows(message.writes(), fixture)
                                            })
                                        {
                                            tasks.push(self.update(Message::WindowRules(message)));
                                        }
                                    }
                                }
                                if self.page == Page::DesktopWallpaper {
                                    let fixture = self
                                        .wallpaper
                                        .store()
                                        .and_then(crate::desktop::DesktopStore::fixture_root)
                                        .is_some_and(std::path::Path::is_dir);
                                    for event in value["wallpaper"].as_array().into_iter().flatten()
                                    {
                                        if let Some(message) =
                                            capture_wallpaper(event).filter(|message| {
                                                capture_allows(message.writes(), fixture)
                                            })
                                        {
                                            tasks.push(self.update(Message::Wallpaper(message)));
                                        }
                                    }
                                }
                                if self.page == Page::DesktopIcons {
                                    let fixture = self
                                        .desktop_icons
                                        .store()
                                        .and_then(crate::desktop::DesktopStore::fixture_root)
                                        .is_some_and(std::path::Path::is_dir);
                                    for event in
                                        value["desktop_icons"].as_array().into_iter().flatten()
                                    {
                                        if let Some(message) =
                                            capture_desktop_icons(event).filter(|message| {
                                                capture_allows(message.writes(), fixture)
                                            })
                                        {
                                            tasks.push(self.update(Message::DesktopIcons(message)));
                                        }
                                    }
                                }
                                if self.page == Page::DesktopShortcuts {
                                    let fixture = self
                                        .shortcuts
                                        .store()
                                        .and_then(crate::desktop::DesktopStore::fixture_root)
                                        .is_some_and(std::path::Path::is_dir);
                                    for event in value["shortcuts"].as_array().into_iter().flatten()
                                    {
                                        if let Some(message) =
                                            capture_shortcuts(event).filter(|message| {
                                                capture_allows(message.writes(), fixture)
                                            })
                                        {
                                            tasks.push(self.update(Message::Shortcuts(message)));
                                        }
                                    }
                                }
                                if self.page == Page::DesktopAutomation {
                                    let fixture = self
                                        .automation
                                        .store()
                                        .and_then(crate::desktop::DesktopStore::fixture_root)
                                        .is_some_and(std::path::Path::is_dir);
                                    for event in
                                        value["automation"].as_array().into_iter().flatten()
                                    {
                                        if let Some(message) =
                                            capture_automation(event).filter(|message| {
                                                capture_allows(message.writes(), fixture)
                                            })
                                        {
                                            tasks.push(self.update(Message::Automation(message)));
                                        }
                                    }
                                }
                                if self.page == Page::Devbox {
                                    for event in value["devbox"].as_array().into_iter().flatten() {
                                        if let Some(message) = capture_devbox(event) {
                                            tasks.push(self.update(message));
                                        }
                                    }
                                }
                            }
                            if let Some(id) = self.core.main_window_id() {
                                if commands && let Some(offset) = value["scroll"].as_f64() {
                                    tasks.push(cosmic::iced::widget::scrollable::snap_to(
                                        cosmic::widget::Id::new("settings-content"),
                                        cosmic::iced::widget::scrollable::RelativeOffset {
                                            x: None,
                                            y: Some(offset.clamp(0., 1.) as f32),
                                        },
                                    ));
                                }
                                // Screenshots only read the window into the capture folder.
                                if capture_dir().is_some() {
                                    tasks.push(cosmic::task::future(async {
                                        tokio::time::sleep(Duration::from_millis(500)).await;
                                        Message::Capture
                                    }));
                                }
                                if commands && value["close"].as_bool() == Some(true) {
                                    self.flush_edits();
                                    tasks.push(window::close(id));
                                } else {
                                    tasks.push(window::gain_focus(id));
                                }
                            }
                        }
                    }
                }
                if capture_dir().is_some() {
                    let key = format!(
                        "{}:{}:{:?}:{:?}:{:?}:{:?}:{}:{}:{}:{}:{}:{}:{:?}:{}:{}:{}:{}:{}:{:?}:{:?}",
                        format!("{}{}", self.automation.state_key(), self.motion.state_key()),
                        self.tools.state_key(),
                        self.page,
                        self.desktop.state_key(),
                        self.system_appearance.state_key(),
                        self.profiles.state_key(),
                        self.windows.state_key(),
                        self.workspaces.state_key(),
                        self.window_rules.state_key(),
                        self.shortcuts.state_key(),
                        self.wallpaper.state_key(),
                        self.desktop_icons.state_key(),
                        self.config,
                        self.status,
                        self.cities.len(),
                        self.city_search.searching,
                        self.restore_confirm,
                        self.folder_input,
                        self.devbox_reorder.source,
                        self.devbox_reorder.target
                    );
                    let key = format!(
                        "{key}:{}:{:?}:{:?}:{:?}",
                        self.search_query,
                        self.search_selected,
                        self.advanced_pages,
                        self.collapsed_scenes
                    );
                    let key = format!("{key}:{:?}:{:?}", self.recipe_editor, self.profiles.package);
                    if self.capture_key != key {
                        self.capture_key = key;
                        tasks.push(cosmic::task::future(async {
                            tokio::time::sleep(Duration::from_millis(300)).await;
                            Message::Capture
                        }));
                    }
                }
                return Task::batch(tasks);
            }
            Message::Page(page) => {
                let page = page.canonical();
                if self.page != page {
                    // A previous page's change stays available in Recent
                    // changes and keyboard Undo, not as this task's footer.
                    self.undo_baseline = navigation::latest_desktop_change().map(|(_, _, id)| id);
                }
                if self.page != page
                    && matches!(
                        self.page,
                        Page::Themes | Page::DesktopProfiles | Page::AdaptiveLooks
                    )
                {
                    self.profiles.update(profiles::Message::Discard);
                }
                if self.page != page && self.page == Page::DesktopWallpaper {
                    self.wallpaper.cancel_preview();
                    self.rain.clear();
                }
                self.devbox_reordering = false;
                self.devbox_reorder.cancel();
                self.help_open = None;
                self.color_well = None;
                self.style_browser = false;
                self.setup_requested = false;
                self.shortcuts.cancel_recording();
                self.flush_edits();
                // An unsaved layout draft survives a visit to a widget's own
                // page; only an in-progress drag is abandoned.
                self.placement.leave();
                self.placement.refresh(&self.config);
                self.refresh_canvas();
                self.page = page;
                self.root_destinations[page.root_index()] = page;
                if !(capture_dir().is_some() && !preview())
                    && let Ok(store) = crate::desktop::DesktopStore::from_env()
                {
                    if let Ok(path) = store.state_file("settings-destination") {
                        if std::fs::create_dir_all(store.state_dir()).is_ok() {
                            let _ = crate::config::write_config_text(&path, Some(page.key()));
                        }
                    }
                }
                navigation::select(&mut self.nav_model, page);
                self.search_query.clear();
                self.search_selected = None;
                self.last_desktop_change = navigation::latest_desktop_change();
                self.desktop.drag.cancel();
                if page == Page::DesktopDock {
                    self.desktop.open();
                }
                if page == Page::DesktopAppearance {
                    self.system_appearance.open();
                }
                if matches!(
                    page,
                    Page::DesktopProfiles | Page::Themes | Page::AdaptiveLooks
                ) {
                    self.profiles.open();
                }
                if matches!(
                    page,
                    Page::Overview
                        | Page::DesktopProfiles
                        | Page::Themes
                        | Page::Appearance
                        | Page::DesktopDock
                        | Page::DesktopIcons
                        | Page::DesktopWindows
                        | Page::DesktopAppearance
                ) || page.nav() == Page::Widgets
                {
                    self.profiles.open_home();
                }
                if matches!(
                    page,
                    Page::DesktopAutomation
                        | Page::Moments
                        | Page::Desktop
                        | Page::DesktopProfiles
                        | Page::AdaptiveLooks
                ) {
                    self.automation.open();
                }
                if page == Page::Themes {
                    self.wallpaper.open();
                    self.system_appearance.open();
                }
                if page == Page::AdaptiveLooks {
                    self.adaptive_source = if self.automation.solar.weather_enabled {
                        crate::automation::solar::Source::Weather
                    } else {
                        crate::automation::solar::Source::Manual
                    };
                    self.profiles
                        .update(profiles::Message::Select(profiles::Selection::Preset(
                            self.adaptive_look.id(self.adaptive_phase),
                        )));
                }
                self.open_page_editor(page);
                if !self.error {
                    self.status = if matches!(page, Page::Overview | Page::WidgetLayout)
                        && self.placement.dirty()
                    {
                        "Layout draft · Save layout to apply"
                    } else {
                        page.save_hint()
                    }
                    .into();
                }
                let tools = page
                    .tool()
                    .map(|tool| self.tools.open(tool))
                    .unwrap_or_else(Task::none);
                self.reorder.cancel();
                self.devbox_reorder.cancel();
                self.restore_confirm = false;
                let scroll = cosmic::iced::widget::scrollable::scroll_to(
                    cosmic::widget::Id::new("settings-content"),
                    cosmic::iced::widget::scrollable::AbsoluteOffset {
                        x: None,
                        y: Some(self.page_scroll.get(page.key()).copied().unwrap_or(0.)),
                    },
                );
                return Task::batch([
                    scroll,
                    if matches!(page, Page::General | Page::Overview) {
                        service_task()
                    } else {
                        Task::none()
                    },
                    self.media_tasks(),
                    tools,
                    if page == Page::Calendar {
                        self.calendar.check(false)
                    } else {
                        Task::none()
                    },
                ]);
            }
            Message::RefreshService => {
                if !self.busy {
                    return service_task();
                }
            }
            Message::Widget(widget, visible) => {
                let mut config = Config::load();
                match config.save_widget(widget, visible) {
                    Ok(()) => {
                        self.config = config;
                        self.placement.refresh(&self.config);
                        self.refresh_canvas();
                        self.saved();
                    }
                    Err(error) => self.failed(error),
                }
            }
            Message::RefreshInterval(widget, seconds) => {
                match Config::save_refresh(widget, seconds) {
                    Ok(config) => {
                        self.config = config;
                        self.saved();
                    }
                    Err(error) => self.failed(error),
                }
            }
            Message::Move(widget, direction) => {
                let mut config = Config::load();
                if config.has_custom_positions() || self.placement.dirty() {
                    return Task::none();
                }
                let mut order = config.ordered_right_cards();
                if let Some(index) = order.iter().position(|item| *item == widget) {
                    let target = index as i32 + direction;
                    if (0..order.len() as i32).contains(&target) {
                        order.swap(index, target as usize);
                        match config.save_order(&order) {
                            Ok(()) => {
                                self.config = config;
                                self.saved();
                            }
                            Err(error) => self.failed(error),
                        }
                    }
                }
            }
            Message::DragStart(widget) => {
                if self.page == Page::Overview
                    && self.devbox_reorder.source.is_none()
                    && !self.config.has_custom_positions()
                    && !self.placement.dirty()
                    && !self.placement.dragging()
                    && self.config.ordered_right_cards().contains(&widget)
                {
                    self.reorder.start(widget);
                }
            }
            Message::DragOver(widget) => self.reorder.over(widget),
            Message::DragLeave(widget) => self.reorder.leave(widget),
            Message::DragCancel => {
                self.reorder.cancel();
                self.devbox_reorder.cancel();
            }
            Message::DragDrop => {
                if self.devbox_reorder.source.is_some() {
                    let config = match Config::try_load() {
                        Ok(config) => config,
                        Err(error) => {
                            self.devbox_reorder.cancel();
                            self.failed(error);
                            return Task::none();
                        }
                    };
                    let mut order = config.devbox.ordered_sections();
                    self.config = config;
                    if self.devbox_reorder.finish(&mut order) {
                        self.save_devbox_order(&order);
                    }
                    return Task::none();
                }
                let mut config = Config::load();
                if config.has_custom_positions() || self.placement.dirty() {
                    self.reorder.cancel();
                    return Task::none();
                }
                let mut order = config.ordered_right_cards();
                if self.reorder.finish(&mut order) {
                    match config.save_order(&order) {
                        Ok(()) => {
                            self.config = config;
                            self.saved();
                        }
                        Err(error) => self.failed(error),
                    }
                }
            }
            Message::Devbox(key, value) => match Config::save_devbox(json!({key: value})) {
                Ok(config) => {
                    self.config = config;
                    self.saved();
                }
                Err(error) => self.failed(error),
            },
            Message::DevboxHistory(key, value) => match Config::save_devbox_history(key, value) {
                Ok(config) => {
                    self.config = config;
                    self.saved();
                }
                Err(error) => self.failed(error),
            },
            Message::DevboxReorder(on) => {
                self.devbox_reordering = on;
                self.devbox_reorder.cancel();
            }
            Message::DevboxDetails(section) => {
                if self.devbox_details.contains(&section) {
                    self.devbox_details.retain(|item| *item != section);
                } else {
                    self.devbox_details.push(section);
                }
            }
            Message::GlanceSection(key, value) => self.save_glance(json!({key: value})),
            Message::GlanceTomorrow(value) => self.save_glance(json!({"show_tomorrow": value})),
            Message::ToggleSection(key) => {
                if !self.open_sections.remove(key) {
                    self.open_sections.insert(key);
                }
            }
            Message::ToggleHelp(key) => {
                self.help_open = (self.help_open != Some(key)).then_some(key);
            }
            // With a widget style applied, choosing a background here keeps
            // this widget's own instead of the style's surface.
            Message::GlanceStyle(value) => self.save_glance(json!({
                "style": value,
                "own_background": self.config.appearance.family.is_some(),
            })),
            Message::GlanceMatchWidgets => self.save_glance(json!({"own_background": false})),
            Message::StyleBrowser(open) => {
                self.style_browser = open && self.page == Page::Themes;
                return cosmic::iced::widget::scrollable::snap_to(
                    cosmic::widget::Id::new("settings-content"),
                    cosmic::iced::widget::scrollable::RelativeOffset {
                        x: None,
                        y: Some(0.),
                    },
                );
            }
            Message::StyleFilter(filter) => self.style_filter = filter,
            Message::StyleBars(on) => {
                self.save(json!({"appearance": {"style_bars": on}}));
                self.profiles
                    .set_style_bars(self.config.appearance.style_bars);
            }
            Message::GlanceDensity(value) => self.save_glance(json!({"density": value})),
            Message::GlanceOpacity(value) => self.save_glance(json!({"opacity": value})),

            Message::DevboxMove(section, direction) => {
                if self.page != Page::Devbox
                    || self.devbox_reorder.source.is_some()
                    || self.reorder.source.is_some()
                    || self.placement.dragging()
                {
                    return Task::none();
                }
                let config = match Config::try_load() {
                    Ok(config) => config,
                    Err(error) => {
                        self.failed(error);
                        return Task::none();
                    }
                };
                let mut order = config.devbox.ordered_sections();
                self.config = config;
                if let Some(index) = order.iter().position(|item| *item == section) {
                    let target = index as i32 + direction.signum();
                    if target != index as i32 && (0..order.len() as i32).contains(&target) {
                        order.swap(index, target as usize);
                        self.save_devbox_order(&order);
                    }
                }
            }
            Message::DevboxDragStart(section) => {
                if self.page == Page::Devbox
                    && self.reorder.source.is_none()
                    && !self.placement.dragging()
                {
                    self.devbox_reorder.start(section);
                }
            }
            Message::DevboxDragOver(section) => self.devbox_reorder.over(section),
            Message::DevboxDragLeave(section) => self.devbox_reorder.leave(section),
            Message::ArrangeDesktop => {
                if self.busy {
                    return Task::none();
                }
                self.working("Opening desktop arrangement…");
                return desktop_command_on_display(
                    "arrange",
                    "Drag widgets, then save your layout",
                    self.placement.display.clone(),
                );
            }
            Message::OpenDesktop(page) => {
                return run_command(
                    "/usr/bin/cosmic-settings".into(),
                    vec![page.into()],
                    "System settings opened",
                );
            }
            Message::CalendarPrivacy(value) => self.save(json!({"calendar_private": value})),
            Message::Today(value) => self.save(json!({"calendar_show_today": value})),
            Message::Tomorrow(value) => self.save(json!({"calendar_show_tomorrow": value})),
            Message::WeatherDewPoint(value) => {
                match Config::save_weather(json!({"show_dew_point": value})) {
                    Ok(config) => {
                        self.config = config;
                        self.saved();
                    }
                    Err(error) => self.failed(error),
                }
            }
            Message::Notice(value) => {
                self.notice = value;
                self.notice_editing = true;
                // Gesture completion commits; idle time is not completion.
            }
            Message::SaveNotice | Message::SaveAppearance | Message::FlushEdits => {
                self.flush_edits()
            }
            Message::CommitEdits(_) => {}
            Message::MaterialTarget(target) => {
                self.save_material_draft();
                if self.native_busy {
                    self.native_deferred
                        .push_back(Message::MaterialTarget(target));
                    return Task::none();
                }
                self.material_target = target;
            }
            Message::MaterialChoose(kind) => {
                self.flush_edits();
                if self.native_busy {
                    self.native_deferred
                        .push_back(Message::MaterialChoose(kind));
                    return Task::none();
                }
                self.material_draft = None;
                self.material_baseline = None;
                let current = self.config.materials.effective(self.material_target);
                self.apply_material(kind.map(|kind| {
                    current
                        .filter(|f| f.kind == kind)
                        .unwrap_or_else(|| crate::materials::Finish::new(kind))
                }));
            }
            Message::MaterialAdjust(open) => self.material_adjust = open,
            Message::MaterialValue(opacity, value) => {
                if self.material_draft.is_none() {
                    self.material_baseline = Some(self.config.materials);
                }
                if let Some(mut finish) = self
                    .material_draft
                    .or(self.config.materials.effective(self.material_target))
                {
                    if opacity {
                        finish.opacity = Some(value);
                    } else {
                        finish.tint = value;
                    }
                    self.material_draft = Some(finish);
                }
            }
            Message::MaterialSave => self.save_material_draft(),
            Message::Rain(edit) => match edit {
                rain::Edit::Lifecycle(state) => self.rain.lifecycle = state,
                rain::Edit::Draft(p) => {
                    if p.validate().is_ok() {
                        self.rain
                            .base
                            .get_or_insert_with(|| self.config.rain.clone());
                        self.rain.draft = Some(p);
                    }
                }
                rain::Edit::Preview(on) => self.rain.preview = on,
                rain::Edit::Commit => {
                    if let Some(p) = self.rain.draft.take() {
                        self.save_rain(p);
                    }
                }
                rain::Edit::Set(p) => {
                    if p.validate().is_ok() {
                        self.rain.draft = None;
                        self.save_rain(p);
                    }
                }
            },
            Message::MaterialMotion(on) => self.save(json!({"material_motion":on})),
            Message::AppearancePreview(widget) => self.appearance_preview = widget,
            Message::WidgetSurface(value) => self.save_appearance(json!({"surface":value})),
            Message::ResetThemeSurface(domain) => {
                self.profiles.open();
                self.profiles
                    .update(profiles::Message::ResetSurface(domain));
                self.profiles.update(profiles::Message::Apply);
                return self.after_desktop_change();
            }
            Message::ContrastAssist(on) => {
                self.flush_edits();
                self.save_appearance(json!({"contrast_assist":on}));
            }
            Message::AppearanceValue(AppearanceControl::Opacity, value)
                if self.config.materials.widgets().is_some() =>
            {
                self.material_target = crate::materials::Target::Widgets;
                if self.material_draft.is_none() {
                    self.material_baseline = Some(self.config.materials);
                }
                let mut f = self.config.materials.widgets().unwrap();
                f.opacity = Some(value as u8);
                self.material_draft = Some(f);
            }
            Message::AppearanceValue(control, value) => {
                self.appearance_editing = true;
                if !self.appearance_pending.contains(&control) {
                    self.appearance_pending.push(control);
                }
                match control {
                    AppearanceControl::Opacity => self.appearance_draft.opacity = value as u8,
                    AppearanceControl::Corners => self.appearance_draft.corner_radius = value,
                    AppearanceControl::Border => self.appearance_draft.border_opacity = value as u8,
                }
                // Gesture completion commits; idle time is not completion.
            }
            Message::AppearancePreset(opacity, corners, border) => {
                self.flush_edits();
                self.save_appearance(
                    json!({"opacity": opacity, "corner_radius": corners, "border_opacity": border}),
                );
            }
            Message::MatchWallpaper(value) => {
                if self.busy {
                    return Task::none();
                }
                self.flush_edits();
                self.save_appearance(json!({"match_wallpaper": value}));
                if !self.error && value && !preview() {
                    self.working("Applying the matching wallpaper…");
                    return controller(
                        vec!["theme".into(), self.config.theme.clone()],
                        "Palette and wallpaper matched",
                    );
                }
            }
            Message::MatchCosmicColors(value) => {
                if self.busy {
                    return Task::none();
                }
                let had_edits = !self.appearance_pending.is_empty() || self.notice_editing;
                self.flush_edits();
                if had_edits && self.error {
                    return Task::none();
                }
                if preview() {
                    self.save_appearance(json!({"match_cosmic_colors": value}));
                } else {
                    self.working(if value {
                        "Matching COSMIC colors…"
                    } else {
                        "Stopping COSMIC color matching…"
                    });
                    return controller(
                        vec![
                            "cosmic-colors".into(),
                            if value { "on" } else { "off" }.into(),
                        ],
                        if value {
                            "COSMIC colors matched"
                        } else {
                            "COSMIC color matching off"
                        },
                    );
                }
            }
            Message::ClockFormat(value) => self.save_clock(json!({"format_24h": value})),
            Message::ClockDate(value) => self.save_clock(json!({"show_date": value})),
            Message::ClockAlignment(value) => self.save_clock(json!({"alignment": value})),
            Message::ClockScale(value) => self.save_clock(json!({"scale": value})),

            Message::ClockVertical(value) => self.save_clock(json!({"center_vertically": value})),
            Message::DepthPreviewReady(id, pixels) => self.depth_preview.ready(id, pixels),
            Message::Moment(edit) => self.update_moment(edit),
            Message::OpenRecipe(kind) => {
                let task = self.update(Message::Page(Page::Builder));
                if let Some(recipe) = self.config.recipes.get(kind).cloned() {
                    self.recipe_editor.expected =
                        self.recipe_editor.library.entries.get(&recipe.id).cloned();
                    self.recipe_editor.draft = recipe;
                    self.recipe_editor.notice = None;
                    self.recipe_editor.error = false;
                } else {
                    self.recipe_editor.update(recipes::Message::New(kind));
                }
                return task;
            }
            Message::Recipe(edit) => self.update_recipe(edit),
            Message::Design(edit) => return self.update_design(edit),
            Message::ClockDepth(value) => self.save_clock(json!({"depth": value})),
            edit @ (Message::ClockFace(_)
            | Message::ClockWeight(_)
            | Message::ClockItalic(_)
            | Message::ClockSurface(_)) => {
                let (key, value) = moments::clock_style_edit(&edit).unwrap();
                self.save_clock(json!({key: value}));
            }
            Message::Theme(theme) => {
                if self.busy {
                    return Task::none();
                }
                return self.update_wallpaper(wallpaper::Message::Palette(theme));
            }
            Message::WidgetColorSource(source) => {
                self.flush_edits();
                match Config::save_widget_color_source(source) {
                    Ok(config) => {
                        self.config = config;
                        self.appearance_draft = self.config.appearance.clone();
                        self.saved();
                    }
                    Err(error) => self.failed(error),
                }
            }
            Message::DesktopPaletteChanged => {
                if let Err(error) = self.config.refresh_desktop_palette() {
                    self.failed(error);
                }
            }
            Message::EditScheduleCity(open) => {
                self.schedule_city_editing = open;
                self.schedule_city_search.select(String::new());
                self.schedule_cities.clear();
            }
            Message::ScheduleCityInput(value) => {
                self.schedule_city_search.edit(value, Instant::now());
                self.schedule_cities.clear();
            }
            Message::SearchScheduleCity => {
                return self.begin_schedule_city_search(self.schedule_city_search.generation);
            }
            Message::SearchScheduleCityAfter(generation) => {
                return self.begin_schedule_city_search(generation);
            }
            Message::ScheduleCities(id, result) => {
                let error = result.as_ref().err().cloned();
                if !self.schedule_city_search.finish(id, error) {
                    return Task::none();
                }
                if let Ok(cities) = result {
                    self.schedule_cities = cities;
                }
            }
            Message::ChooseScheduleCity(index) => {
                if let Some(city) = self.schedule_cities.get(index) {
                    let location = city.location();
                    if let Err(reason) = crate::automation::solar::validate_city(&location) {
                        self.failed(reason);
                    } else {
                        self.schedule_city_editing = false;
                        self.schedule_city_search.select(city.name.clone());
                        self.schedule_cities.clear();
                        return self.update(Message::Automation(automation::Message::ManualCity(
                            location,
                        )));
                    }
                }
            }
            Message::CityInput(value) => {
                self.city_editing = true;
                self.city_search.edit(value, Instant::now());
                self.cities.clear();
            }
            Message::EditCity(open) => {
                self.city_editing = open;
                self.city_search.select(String::new());
                self.cities.clear();
                if open {
                    return cosmic::widget::text_input::focus(cosmic::widget::Id::new(
                        "weather-city",
                    ));
                }
            }
            Message::ClearCitySearch => {
                self.city_search.select(String::new());
                self.cities.clear();
            }
            Message::SearchCity => return self.begin_city_search(self.city_search.generation),
            Message::SearchCityAfter(generation) => return self.begin_city_search(generation),
            Message::Cities(id, result) => {
                let error = result.as_ref().err().cloned();
                if !self.city_search.finish(id, error) {
                    return Task::none();
                }
                if let Ok(cities) = result {
                    self.cities = cities;
                }
            }
            Message::ChooseCity(index) => {
                if let Some(city) = self.cities.get(index) {
                    let location = city.location();
                    let name = city.name.clone();
                    self.save(json!({"location": location}));
                    if !self.error {
                        self.city_search.select(name);
                        self.cities.clear();
                        self.city_editing = false;
                    }
                }
            }
            Message::FolderInput(value) => self.folder_input = value,
            Message::AddFolder => self.add_folder(self.folder_input.clone()),
            Message::BrowseFolder => {
                if self.folder_pending {
                    return Task::none();
                }
                self.folder_pending = true;
                return cosmic::task::future(async {
                    use cosmic::dialog::file_chooser::{self, open::Dialog};
                    let result = match Dialog::new()
                        .title("Choose a project folder")
                        .open_folder()
                        .await
                    {
                        Ok(response) => response
                            .url()
                            .to_file_path()
                            .map(Some)
                            .map_err(|_| "Choose a local folder".into()),
                        Err(file_chooser::Error::Cancelled) => Ok(None),
                        Err(error) => Err(format!("Could not open the folder chooser: {error}")),
                    };
                    Message::FolderChosen(result)
                });
            }
            Message::FolderChosen(result) => {
                self.folder_pending = false;
                match result {
                    Ok(Some(path)) => self.add_folder(path.to_string_lossy().into_owned()),
                    Ok(None) => (),
                    Err(error) => self.failed(error),
                }
            }
            Message::RemoveFolder(path) => {
                let mut roots = Config::load().project_roots;
                roots.retain(|root| *root != path);
                self.save(json!({"project_roots": roots}));
            }
            Message::Desktop(visible) => self.save(json!({"widgets_visible": visible})),
            Message::Background(running) => {
                if self.busy || preview() {
                    return Task::none();
                }
                self.working(if running {
                    "Starting background operation…"
                } else {
                    "Stopping background operation…"
                });
                return systemctl(
                    vec![if running { "start" } else { "stop" }],
                    "Background operation updated",
                );
            }
            Message::Startup(enabled) => {
                if self.busy || preview() {
                    return Task::none();
                }
                self.working("Updating startup preference…");
                return systemctl(
                    vec![if enabled { "enable" } else { "disable" }],
                    "Startup preference saved",
                );
            }
            Message::Service(result) => match result {
                Ok(service) => {
                    // Running setup again starts from the real sign-in state.
                    if let Some(wizard) = self.wizard.as_mut()
                        && self.config.onboarded
                        && !wizard.startup_touched
                    {
                        wizard.startup = service.startup;
                    }
                    self.service = service;
                    self.service_known = true;
                }
                Err(error) => {
                    self.service_known = false;
                    self.failed(error);
                }
            },
            Message::CommandDone(status, result) => {
                self.busy = false;
                if let Ok(config) = Config::try_load() {
                    self.config = config;
                }
                // Reinstalled pictures or a matched background: the page
                // rereads its tiles, and the media work that starts runs
                // whatever page shows.
                if self.wallpaper.opened() {
                    self.wallpaper.refresh();
                }
                let media = self.media_tasks();
                match result {
                    Ok(()) => {
                        self.status = status;
                        self.error = false;
                    }
                    Err(error) => self.failed(error),
                }
                if matches!(self.page, Page::General | Page::Overview) {
                    return Task::batch([service_task(), media]);
                }
                return media;
            }
            Message::OpenCalendar => {
                if self.calendar.connection.web_url().is_empty() {
                    return Task::none();
                }
                return run_command(
                    "/usr/bin/xdg-open".into(),
                    vec![self.calendar.connection.web_url()],
                    "Calendar opened",
                );
            }
            Message::DesktopRestorePreview => {
                if self.desktop_recovery.busy {
                    return Task::none();
                }
                self.desktop_recovery.busy = true;
                return cosmic::task::future(async {
                    let result = tokio::task::spawn_blocking(|| {
                        let store =
                            crate::desktop::DesktopStore::from_env().map_err(|e| e.to_string())?;
                        let mut domains = crate::desktop::recovery::RecoveryDomain::ALL.to_vec();
                        domains.push(crate::desktop::recovery::RecoveryDomain::AppAnimations);
                        crate::desktop::recovery::plan_restore(&store, &domains)
                            .map_err(|e| e.to_string())
                    })
                    .await
                    .map_err(|e| e.to_string())
                    .and_then(|v| v);
                    Message::DesktopRestorePlanned(result)
                });
            }
            Message::DesktopRestorePlanned(result) => {
                self.desktop_recovery.busy = false;
                match result {
                    Ok(plan) => self.desktop_recovery.plan = Some(plan),
                    Err(error) => self.failed(error),
                }
            }
            Message::DesktopRestoreConfirm(confirmed) => {
                let Some(plan) = self.desktop_recovery.plan.take() else {
                    return Task::none();
                };
                if !confirmed {
                    return Task::none();
                }
                self.desktop_recovery.busy = true;
                return cosmic::task::future(async move {
                    let result = tokio::task::spawn_blocking(move || {
                        let store =
                            crate::desktop::DesktopStore::from_env().map_err(|e| e.to_string())?;
                        crate::desktop::recovery::apply_restore(&store, &plan)
                            .map(|_| ())
                            .map_err(|e| e.to_string())
                    })
                    .await
                    .map_err(|e| e.to_string())
                    .and_then(|v| v);
                    Message::DesktopRestoreDone(result)
                });
            }
            Message::DesktopRestoreDone(result) => {
                self.desktop_recovery.busy = false;
                self.desktop_recovery.refresh();
                match result {
                    Ok(()) => {
                        self.config = Config::load();
                        self.saved();
                        self.status = "Desktop restored · Undo is available".into();
                        return self.after_desktop_change();
                    }
                    Err(error) => self.failed(error),
                }
            }
            Message::ExportRecovery => return recovery::export_task(),
            Message::RecoveryExported(result) => match result {
                Ok(Some(_)) => {
                    self.status = "Desktop recovery exported".into();
                    self.error = false;
                }
                Ok(None) => {}
                Err(error) => self.failed(error),
            },
            Message::RecheckInstallation => {
                self.installation_retry = false;
                return self.handle(Message::RefreshInstallation);
            }
            Message::RefreshInstallation => {
                let updates = self.update_updates(updates::Message::Inspect);
                if self.installation_busy || self.installation_retry {
                    return updates;
                }
                self.desktop_recovery.refresh();
                self.installation_busy = true;
                self.installation_generation += 1;
                let generation = self.installation_generation;
                let deadline = std::time::Instant::now() + std::time::Duration::from_secs(6);
                self.installation_deadline = Some(deadline);
                self.desktop.islands.installation = self.desktop.islands.installation.checking();
                return Task::batch([
                    updates,
                    cosmic::task::future(async move {
                        let state = tokio::task::spawn_blocking(move || {
                            crate::desktop::DesktopStore::from_env()
                                .map(|store| {
                                    crate::desktop::islands::Installation::discover_presence_until(
                                        &store, deadline,
                                    )
                                })
                                .unwrap_or_default()
                        })
                        .await
                        .unwrap_or_default();
                        Message::InstallationPresence(generation, state)
                    }),
                ]);
            }
            Message::InstallationPresence(generation, state) => {
                if generation != self.installation_generation {
                    return Task::none();
                }
                if !matches!(state, crate::desktop::islands::Installation::Pending) {
                    let _ = self.handle(Message::InstallationReady(generation, state));
                }
                let panel = self.desktop.selected;
                let output = self.desktop.requested_island_output();
                let deadline = self
                    .installation_deadline
                    .unwrap_or_else(std::time::Instant::now);
                return cosmic::task::future(async move {
                    let (state, runtime) = tokio::task::spawn_blocking(move || {
                        let Ok(store) = crate::desktop::DesktopStore::from_env() else {
                            return (Default::default(), Default::default());
                        };
                        let state =
                            crate::desktop::islands::Installation::discover_until(&store, deadline);
                        let runtime = desktop::islands::RuntimeStatus::read(
                            &store, &state, panel, output, deadline,
                        );
                        (state, runtime)
                    })
                    .await
                    .unwrap_or_default();
                    Message::InstallationChecked(generation, state, runtime)
                });
            }
            Message::InstallationReady(generation, state) => {
                if generation != self.installation_generation {
                    return Task::none();
                }
                let _native = crate::desktop::islands::Installation::native_view(state.clone());
                self.desktop.islands.installation = state;
                self.desktop.check_islands();
                self.profiles.refresh();
                if !self.desktop.islands.installation.installed()
                    && matches!(
                        self.material_target,
                        crate::materials::Target::Dock | crate::materials::Target::Panel
                    )
                {
                    self.material_target = crate::materials::Target::Widgets;
                    self.material_draft = None;
                    self.material_adjust = false;
                }
                if let Some(wizard) = &mut self.wizard {
                    wizard.islands_available = self.desktop.islands.installation.installed();
                }
            }
            Message::CopyIslandsDiagnostics => {
                if let Some(diagnostics) = self.desktop.islands.installation.diagnostics() {
                    return cosmic::iced::clipboard::write(diagnostics.to_owned());
                }
            }
            Message::OpenIslandsGuide => {
                return run_command(
                    "/usr/bin/xdg-open".into(),
                    vec!["https://github.com/profilaz3/halcyon/blob/main/packaging/islands/README.md".into()],
                    "Islands guide opened",
                );
            }
            Message::OpenWeatherSource => {
                return run_command(
                    "/usr/bin/xdg-open".into(),
                    vec!["https://open-meteo.com/".into()],
                    "Weather source opened",
                );
            }
            Message::ShowControls => {
                return controller(vec!["panel".into()], "Edge controls opened");
            }
            Message::Restore(confirm) => {
                if !confirm {
                    self.restore_confirm = !self.restore_confirm;
                } else if !self.busy && !preview() {
                    self.restore_confirm = false;
                    self.working("Restoring your previous wallpaper…");
                    return controller(
                        vec!["restore-wallpaper".into()],
                        "Previous wallpaper restored",
                    );
                }
            }
            Message::Capture => {
                if let Some(id) = self.core.main_window_id() {
                    let page = self.page;
                    // Let the frame after this message draw first: a text
                    // editor's glyphs belong to the frame that laid it out,
                    // and a screenshot re-presents the previous one.
                    return cosmic::iced::Task::future(tokio::time::sleep(Duration::from_millis(
                        120,
                    )))
                    .discard()
                    .chain(
                        window::screenshot(id)
                            .map(move |shot| cosmic::Action::App(Message::Captured(page, shot))),
                    );
                }
            }
            Message::CaptureCursor(position) => {
                if let Some(path) = capture_dir() {
                    let _ = std::fs::create_dir_all(&path);
                    let _ = std::fs::write(
                        path.join("cursor.json"),
                        format!("[{},{}]", position.x, position.y),
                    );
                }
            }
            Message::Captured(page, shot) => {
                if let Some(path) = capture_dir() {
                    let _ = std::fs::create_dir_all(&path);
                    let _ = std::fs::write(
                        path.join(format!("settings-{}.rgba", page.key())),
                        shot.rgba,
                    );
                    let _ = std::fs::write(
                        path.join(format!("settings-{}.json", page.key())),
                        format!("[{},{}]", shot.size.width, shot.size.height),
                    );
                }
            }
        }
        Task::none()
    }
}

fn desktop_write_message(message: &Message) -> bool {
    match message {
        Message::Rain(rain::Edit::Set(_) | rain::Edit::Commit) => true,
        Message::DesktopEditor(message) => message.writes(),
        Message::SystemAppearance(message) => message.writes(),
        Message::Profiles(message) => message.writes(),
        Message::Windows(message) => message.writes(),
        Message::Workspaces(message) => message.writes(),
        Message::Wallpaper(message) => message.writes(),
        Message::DesktopIcons(message) => message.writes(),
        Message::WindowRules(message) => message.writes(),
        Message::Shortcuts(message) => message.writes(),
        Message::Canvas(canvas::Message::Apply(_))
        | Message::CommandDone(..)
        | Message::UndoDesktop
        | Message::UndoDesktopShown(..)
        | Message::ResolveUndoShown(..)
        | Message::UndoDesktopRestore(..)
        | Message::DesktopRestoreDone(..) => true,
        _ => false,
    }
}

fn checkpoint_message(message: &Message) -> bool {
    !matches!(
        message,
        Message::Tick
            | Message::Support(..)
            | Message::KeyboardFocus(..)
            | Message::Fullscreen
            | Message::AppSettingsPage(..)
            | Message::ToggleScene
            | Message::ToggleHelp(..)
            | Message::ToggleSection(..)
            | Message::ToggleSceneObjects
            | Message::Profiles(profiles::Message::Collection(..) | profiles::Message::Compare(..))
            | Message::ContentScrolled(..)
            | Message::Preview
            | Message::EditCity(..)
            | Message::DevboxDetails(..)
            | Message::Capture
            | Message::Captured(..)
            | Message::CaptureCursor(..)
            | Message::Resized(..)
            | Message::DragOver(..)
            | Message::DragLeave(..)
            | Message::DevboxDragOver(..)
            | Message::DevboxDragLeave(..)
            | Message::DesktopEditor(desktop::Message::Islands(desktop::islands::Edit::Sync))
            | Message::Placement(placement::Message::Pointer(..))
            | Message::SearchSettings(..)
            | Message::FocusSearch
            | Message::RevealSearch(..)
            | Message::RefreshService
            | Message::Service(..)
            | Message::DesktopPaletteChanged
    )
}

impl Settings {
    #[cfg(test)]
    fn profile_waits(&self) -> Vec<(Page, bool, bool)> {
        Vec::new()
    }
    #[cfg(test)]
    fn theme_waits(&self, _: Page) -> Option<(Page, bool, bool)> {
        None
    }
    /// The miniature editor lives on the Widgets page; pointer and arrow-key
    /// listeners run only while it is on screen.
    fn placement_active(&self) -> bool {
        matches!(self.page, Page::Overview | Page::WidgetLayout)
            && (self.page != Page::Overview || self.canvas.is_some())
            && !self.setup_requested
            && self.search_query.trim().is_empty()
    }
    /// The Motion page: the app-animation toggle writes off the UI thread
    /// and reports in the status line.
    fn update_motion(&mut self, message: motion::Message) -> Task<Message> {
        match message {
            motion::Message::Animations(on) => {
                if self.page != Page::DesktopMotion {
                    return Task::none();
                }
                if let Some(job) = self.motion.toggle(on) {
                    self.working("Changing app animations…");
                    return cosmic::task::future(async move {
                        let message = tokio::task::spawn_blocking(move || job.run())
                            .await
                            .unwrap_or_else(|error| {
                                motion::Message::Written(on, Err(error.to_string()))
                            });
                        Message::Motion(message)
                    });
                }
            }
            motion::Message::Written(_, result) => {
                self.busy = false;
                self.motion_generation = self.motion_generation.wrapping_add(1);
                self.system_animations = result.clone().unwrap_or(false);
                match self.motion.written(result) {
                    Ok(status) => {
                        self.status = status;
                        self.error = false;
                    }
                    Err(error) => self.failed(error),
                }
            }
            motion::Message::Refresh => self.motion.refresh(),
        }
        Task::none()
    }
    fn update_profiles(&mut self, message: profiles::Message) -> Task<Message> {
        use profiles::Message as Edit;
        // A dialog result returns to the request that started it, whatever
        // page is showing; everything else belongs to the page.
        let result = matches!(
            message,
            Edit::Imported(..)
                | Edit::Exported(..)
                | Edit::Backdrop(..)
                | Edit::ThemeReady(..)
                | Edit::ShareReady(..)
        );
        if !result
            && !matches!(
                self.page,
                Page::DesktopProfiles | Page::Themes | Page::AdaptiveLooks
            )
        {
            return Task::none();
        }
        if self.page == Page::Themes
            && matches!(message, Edit::Apply)
            && self.profiles.profile().is_some_and(|p| !p.appearance_only)
        {
            self.profiles.notice = Some(desktop::Notice::Error(
                "Review this desktop setup in Setups.".into(),
            ));
            return Task::none();
        }
        if matches!(message, Edit::Apply | Edit::Undo) {
            // Complete any gesture before the local chooser is committed.
            // Only their own failure stops it; another page's error doesn't.
            let had_edits = !self.appearance_pending.is_empty() || self.notice_editing;
            self.flush_edits();
            if had_edits && self.error {
                self.profiles.notice = Some(desktop::Notice::Error(self.status.clone()));
                return Task::none();
            }
        }
        let mix_transition = matches!(
            message,
            Edit::MixStart
                | Edit::RemixSelected
                | Edit::MixPart(_)
                | Edit::MixClosePicker
                | Edit::MixPick(..)
                | Edit::Package(profiles::packages::Edit::Open | profiles::packages::Edit::Close)
        );
        let before = self.profiles.selection().clone();
        let outcome = self.profiles.update(message);
        let reveal = if mix_transition && self.page == Page::DesktopProfiles {
            self.page_scroll.insert(self.page.key().into(), 0.);
            Task::batch(
                [
                    "settings-content",
                    "mix-scroll",
                    "setup-scroll",
                    "package-scroll",
                ]
                .into_iter()
                .map(|id| {
                    cosmic::iced::widget::scrollable::snap_to(
                        cosmic::widget::Id::new(id),
                        cosmic::iced::widget::scrollable::RelativeOffset {
                            x: None,
                            y: Some(0.),
                        },
                    )
                }),
            )
        } else {
            self.reveal_setup_selection(&before)
        };
        Task::batch([self.profile_outcome(outcome), reveal])
    }
    fn reveal_setup_selection(&mut self, before: &profiles::Selection) -> Task<Message> {
        if self.page == Page::DesktopProfiles && before != self.profiles.selection() {
            self.page_scroll.insert(self.page.key().into(), 0.);
            cosmic::iced::widget::scrollable::snap_to(
                cosmic::widget::Id::new("settings-content"),
                cosmic::iced::widget::scrollable::RelativeOffset {
                    x: None,
                    y: Some(0.),
                },
            )
        } else {
            Task::none()
        }
    }
    fn profile_outcome(&mut self, outcome: profiles::Outcome) -> Task<Message> {
        use profiles::Outcome;
        let media = match outcome {
            Outcome::None => self.media_tasks(),
            Outcome::Changed => self.after_desktop_change(),
            Outcome::FocusName => {
                return Task::batch([
                    self.media_tasks(),
                    cosmic::widget::text_input::focus(profiles::name_id()),
                ]);
            }
            Outcome::Dialog(request) => {
                return Task::batch([self.media_tasks(), profile_dialog(request)]);
            }
        };
        media
    }
    fn wizard_undo_completed(&mut self, id: Option<String>) {
        if let (Some(id), Ok(store)) = (id, crate::desktop::DesktopStore::from_env()) {
            if let Some(wizard) = self.wizard.as_mut() {
                wizard.forget_undone(&id, &store);
            } else if let Err(error) = wizard::Wizard::forget_saved_undone(&id, &store) {
                self.failed(format!(
                    "Undo succeeded, but could not save setup progress: {error}"
                ));
            }
        }
    }
    fn update_wizard(&mut self, message: wizard::Message) -> Task<Message> {
        let Some(message) = self.wizard.as_ref().and_then(|w| w.admitted(message)) else {
            return Task::none();
        };
        if let wizard::Message::MoveSelected(delta) = message {
            return cosmic::iced::runtime::task::widget(wizard::FocusedWidget::default()).map(
                move |key| {
                    cosmic::Action::App(Message::Wizard(wizard::Message::MoveWidget(key, delta)))
                },
            );
        }
        let focus_after = matches!(
            message,
            wizard::Message::MoveWidget(..)
                | wizard::Message::UndoWidgetMove
                | wizard::Message::DropWidget
        );
        let Some(wizard) = self.wizard.as_mut() else {
            return Task::none();
        };
        let store = crate::desktop::DesktopStore::from_env().ok();
        let outcome = wizard.update(message, store.as_ref(), &self.config);
        let focus = focus_after
            .then(|| wizard.widgets.focused.clone())
            .flatten();
        let outcome = self.wizard_outcome(outcome);
        if let Some(key) = focus {
            Task::batch([
                outcome,
                cosmic::widget::button::focus(cosmic::widget::Id::new(format!(
                    "wizard-order-{key}"
                )))
                .chain(cosmic::iced::runtime::task::effect(
                    cosmic::iced::runtime::Action::widget(navigation::FocusReveal::default()),
                )),
            ])
        } else {
            outcome
        }
    }
    fn wizard_outcome(&mut self, outcome: wizard::Outcome) -> Task<Message> {
        let store = crate::desktop::DesktopStore::from_env().ok();
        match outcome {
            wizard::Outcome::None => Task::none(),
            wizard::Outcome::Changed => self.after_desktop_change(),
            wizard::Outcome::Done { finish, startup } => {
                self.wizard = None;
                if let Some(store) = &store {
                    wizard::Wizard::forget(store);
                }
                self.save(json!({"onboarded": true}));
                let refresh = self.after_desktop_change();
                let page = self.handle(Message::Page(Page::Overview));
                // The cards appear now, and at every sign-in when chosen.
                // The cards appear now; at sign-in only when chosen.
                let any_widgets = Widget::ALL
                    .into_iter()
                    .any(|w| self.config.widget_visible(w));
                let start = if finish && any_widgets && !preview() {
                    if startup {
                        systemctl(vec!["enable", "--now"], "Halcyon is running")
                    } else {
                        systemctl(vec!["disable"], "Halcyon won't start at sign-in")
                            .chain(systemctl(vec!["start"], "Halcyon is running"))
                    }
                } else {
                    Task::none()
                };
                Task::batch([refresh, page, start])
            }
        }
    }
    /// After a profile change, Undo or recovery: re-read the widget
    /// preferences and the other editors' current values. Their drafts stay.
    /// Returns the Wallpaper page's media work the refresh started.
    fn after_desktop_change(&mut self) -> Task<Message> {
        self.setup_followups = wizard::follow_up::State::load();
        if let Ok(config) = Config::try_load() {
            self.config = config;
        }
        if !self.appearance_editing {
            self.appearance_draft = self.config.appearance.clone();
        }
        self.placement.refresh(&self.config);
        self.refresh_canvas();
        if let Some(wizard) = self.wizard.as_mut() {
            wizard.refresh_widgets(&self.config, &self.displays);
        }
        if self.desktop.opened() {
            self.desktop.refresh();
        }
        if self.system_appearance.opened() {
            self.system_appearance.refresh();
        }
        if self.windows.opened() {
            self.windows.refresh();
        }
        if self.workspaces.opened() {
            self.workspaces.refresh();
        }
        if self.motion.opened() {
            self.motion.refresh();
        }
        if self.window_rules.opened() {
            self.window_rules.refresh();
        }
        if self.shortcuts.opened() {
            self.shortcuts.refresh();
        }
        if self.wallpaper.opened() {
            self.wallpaper.refresh();
        }
        if self.desktop_icons.opened() {
            self.desktop_icons.refresh();
        }
        self.profiles.refresh();
        self.media_tasks()
    }
    /// A chosen color reaches its editor as the text its field received.
    fn update_color_well(&mut self, message: color_well::Message) -> Task<Message> {
        use color_well::{Outcome, Target};
        let Outcome::Chosen(target, rgb) = color_well::update(&mut self.color_well, message) else {
            return Task::none();
        };
        let text = color_well::text(target, rgb);
        let messages = match target {
            Target::Accent => vec![
                Message::Wallpaper(wallpaper::Message::AccentText(text)),
                Message::Wallpaper(wallpaper::Message::AccentSubmit),
            ],
            Target::ThemeCustom => vec![Message::Profiles(profiles::Message::ThemeCustom(Some(
                text,
            )))],
            Target::Role(role, _) => vec![
                Message::SystemAppearance(system_appearance::Message::HexInput(role, text)),
                Message::SystemAppearance(system_appearance::Message::HexSubmit(role)),
            ],
            Target::Stop(index) => vec![Message::Wallpaper(wallpaper::Message::Stop(index, text))],
            Target::Fit => vec![Message::Wallpaper(wallpaper::Message::FitColor(text))],
        };
        Task::batch(
            messages
                .into_iter()
                .map(|message| cosmic::Application::update(self, message)),
        )
    }
    fn open_page_editor(&mut self, page: Page) {
        if page == Page::Appearance && !self.native_worker_needed() {
            self.material_native = crate::desktop::actions::history_store()
                .is_ok_and(|s| crate::desktop::materials::native_support(&s));
        }
        match page {
            Page::DesktopWindows => {
                self.windows.open();
                self.workspaces.open();
            }
            Page::DesktopWorkspaces => self.workspaces.open(),
            Page::DesktopMotion => self.motion.open(),
            Page::DesktopWindowRules => self.window_rules.open(),
            Page::DesktopShortcuts => self.shortcuts.open(),
            Page::DesktopWallpaper | Page::Appearance => self.wallpaper.open(),
            Page::DesktopIcons => self.desktop_icons.open(),
            _ => {}
        }
    }
    /// Windows uses the same completion path as the other inspectors.
    fn update_windows(&mut self, message: windows::Message) {
        self.windows.update(message);
    }
    /// Capture mode drives the Profiles page through its own messages. An
    /// import reads only a file inside the fixture root, without a dialog;
    /// exports are never written from here.
    fn capture_profiles(&mut self, event: &serde_json::Value) -> Vec<Task<Message>> {
        use crate::desktop::profiles::Domain;
        use profiles::{Message as Edit, NameAction, Outcome, Selection};
        let fixture = self
            .profiles
            .store()
            .and_then(crate::desktop::DesktopStore::fixture_root)
            .filter(|root| root.is_dir())
            .map(std::path::Path::to_path_buf);
        let mut messages = Vec::new();
        match event {
            serde_json::Value::String(action) => messages.push(match action.as_str() {
                "apply" => Edit::Apply,
                "undo" => Edit::Undo,
                "discard" => Edit::Discard,
                "details" => Edit::Details(true),
                "replan" => Edit::Replan,
                "recover" => Edit::Recover,
                "keep" => Edit::KeepRecovery,
                "remix" => Edit::RemixSelected,
                _ => return Vec::new(),
            }),
            serde_json::Value::Object(map) => {
                if let Some(query) = map.get("discover").and_then(serde_json::Value::as_str) {
                    messages.push(Edit::DiscoveryQuery(query.chars().take(80).collect()));
                }
                if let Some(action) = map.get("package").and_then(serde_json::Value::as_object) {
                    use profiles::packages::Edit as Package;
                    let value = action
                        .get("value")
                        .and_then(serde_json::Value::as_str)
                        .unwrap_or_default();
                    let edit = match action.get("action").and_then(serde_json::Value::as_str) {
                        Some("open") => Some(Package::Open),
                        Some("close") => Some(Package::Close),
                        Some("picture") => action
                            .get("value")
                            .and_then(serde_json::Value::as_bool)
                            .map(Package::Picture),
                        Some("author") => Some(Package::Author(value.chars().take(80).collect())),
                        Some("license") => {
                            Some(Package::License(value.chars().take(120).collect()))
                        }
                        _ => None,
                    };
                    messages.extend(edit.map(Edit::Package));
                }
                if let Some(name) = map.get("select").and_then(serde_json::Value::as_str) {
                    let saved = || {
                        self.profiles.library().ok()?.iter().find_map(|entry| {
                            (entry.profile.as_ref().ok()?.name == name).then(|| entry.id.clone())
                        })
                    };
                    let selection = match crate::desktop::profiles::preset(name) {
                        _ if name == "current" => Some(Selection::Current),
                        Some(preset) => Some(Selection::Preset(preset.id)),
                        None => crate::desktop_theme::Family::parse(name)
                            .map(|f| Selection::Preset(f.preset()))
                            .or_else(|| saved().map(Selection::Saved)),
                    };
                    messages.extend(selection.map(Edit::Select));
                }
                if map.get("mix_start").and_then(serde_json::Value::as_bool) == Some(true) {
                    messages.push(Edit::MixStart);
                }
                if let Some(parts) = map.get("mix_pick").and_then(serde_json::Value::as_array)
                    && let [part, source] = parts.as_slice()
                    && let (Some(part), Some(source)) = (part.as_str(), source.as_str())
                {
                    use crate::desktop::profiles::mix::Component;
                    let part = match part {
                        "picture" => Some(Component::Picture),
                        "style" => Some(Component::Style),
                        "layout" => Some(Component::Layout),
                        _ => None,
                    };
                    let source = if source == "current" {
                        Some(Selection::Current)
                    } else {
                        crate::desktop::profiles::preset(source).map(|p| Selection::Preset(p.id))
                    };
                    if let (Some(part), Some(source)) = (part, source) {
                        messages.push(Edit::MixPick(part, source));
                    }
                }
                if let Some(on) = map
                    .get("compare_current")
                    .and_then(serde_json::Value::as_bool)
                {
                    messages.push(Edit::Compare(on));
                }
                if let Some(on) = map
                    .get("saved_collection")
                    .and_then(serde_json::Value::as_bool)
                {
                    messages.push(Edit::Collection(on));
                }
                if let Some([domain, on]) = map
                    .get("include")
                    .and_then(serde_json::Value::as_array)
                    .map(Vec::as_slice)
                    && let (Some(domain), Some(on)) =
                        (domain.as_str().and_then(Domain::parse), on.as_bool())
                {
                    messages.push(Edit::Include(domain, on));
                }
                for (key, action) in [
                    (
                        "capture",
                        if self.page == Page::Themes {
                            NameAction::CaptureTheme
                        } else {
                            NameAction::Capture
                        },
                    ),
                    ("duplicate", NameAction::Duplicate),
                    ("save_mix", NameAction::SaveMix),
                ] {
                    if let Some(name) = map.get(key).and_then(serde_json::Value::as_str) {
                        messages.push(Edit::Name(Some(action)));
                        messages.push(Edit::NameInput(name.into()));
                        messages.push(Edit::NameSubmit);
                    }
                }
                if let Some(on) = map
                    .get("wallpaper_colors")
                    .and_then(serde_json::Value::as_bool)
                {
                    messages.push(Edit::ThemeColors(on));
                }
                if let Some(rgb) = map
                    .get("accent")
                    .and_then(serde_json::Value::as_str)
                    .and_then(wallpaper::parse_rgb)
                {
                    messages.push(Edit::ThemeAccent(rgb.map(|v| (v * 255.).round() as u8)));
                }
                if let Some(on) = map
                    .get("matching_wallpaper")
                    .and_then(serde_json::Value::as_bool)
                {
                    messages.push(Edit::ThemeWallpaper(on));
                }
                if let Some(domain) = map
                    .get("reset")
                    .and_then(serde_json::Value::as_str)
                    .and_then(Domain::parse)
                {
                    messages.push(Edit::ResetSurface(domain));
                }
                if let Some(path) = map.get("import").and_then(serde_json::Value::as_str) {
                    let inside = fixture.as_ref().and_then(|root| {
                        let root = root.canonicalize().ok()?;
                        let file = root.join(path).canonicalize().ok()?;
                        file.starts_with(&root).then_some(file)
                    });
                    if let Some(file) = inside
                        && let Outcome::Dialog(profiles::Request::Import { id }) =
                            self.profiles.update(Edit::Import)
                    {
                        messages.push(Edit::Imported(id, profiles::read_import(&file).map(Some)));
                    }
                }
            }
            _ => {}
        }
        messages
            .into_iter()
            .filter(|message| capture_allows(message.writes(), fixture.is_some()))
            .map(|message| self.update_profiles(message))
            .collect()
    }
    fn begin_schedule_city_search(&mut self, generation: u64) -> Task<Message> {
        let Some((query, registration)) = self.schedule_city_search.begin(generation) else {
            return Task::none();
        };
        let id = self.schedule_city_search.generation;
        self.schedule_cities.clear();
        cosmic::task::future(async move {
            Message::ScheduleCities(id, city_search::search(query, registration).await)
        })
    }
    fn begin_city_search(&mut self, generation: u64) -> Task<Message> {
        let Some((query, registration)) = self.city_search.begin(generation) else {
            return Task::none();
        };
        let id = self.city_search.generation;
        self.cities.clear();
        cosmic::task::future(async move {
            Message::Cities(id, city_search::search(query, registration).await)
        })
    }
    fn flush_edits(&mut self) {
        self.save_material_draft();
        if let Some(p) = self.rain.draft.take() {
            self.save_rain(p);
        }
        self.save_at = None;
        self.edit_generation = self.edit_generation.wrapping_add(1);
        let pending = std::mem::take(&mut self.appearance_pending);
        self.appearance_editing = false;
        let mut error = None;
        if !pending.is_empty() {
            let mut changes = serde_json::Map::new();
            for control in pending {
                let (key, value) = match control {
                    AppearanceControl::Opacity => ("opacity", json!(self.appearance_draft.opacity)),
                    AppearanceControl::Corners => {
                        ("corner_radius", json!(self.appearance_draft.corner_radius))
                    }
                    AppearanceControl::Border => (
                        "border_opacity",
                        json!(self.appearance_draft.border_opacity),
                    ),
                };
                changes.insert(key.into(), value);
            }
            self.save_appearance(changes.into());
            if self.error {
                error = Some(self.status.clone());
            }
        }
        if self.notice_editing {
            self.notice_editing = false;
            self.save(json!({"meeting_notice_minutes": self.notice}));
            self.notice = self.config.meeting_notice_minutes as u16;
        }
        // An unsaved note outlives the window as a draft; only Save puts it
        // on the desktop.
        if let Err(problem) = self.tools.keep_draft() {
            error.get_or_insert(format!("Could not keep the unsaved note: {problem}"));
        }
        if let Some(error) = error {
            self.failed(error);
        }
    }
    fn save_appearance(&mut self, changes: serde_json::Value) {
        match Config::save_appearance(changes) {
            Ok(config) => {
                self.appearance_draft = config.appearance.clone();
                self.config = config;
                self.saved();
            }
            Err(error) => {
                self.appearance_draft = self.config.appearance.clone();
                self.failed(error);
            }
        }
    }
    fn save_clock(&mut self, changes: serde_json::Value) {
        match Config::save_clock(changes) {
            Ok(config) => {
                self.config = config;
                self.saved();
            }
            Err(error) => self.failed(error),
        }
    }
    fn save_devbox_order(&mut self, order: &[DevboxSection]) {
        let keys: Vec<_> = order.iter().map(|section| section.key()).collect();
        match Config::save_devbox(json!({"order": keys})) {
            Ok(config) => {
                self.config = config;
                self.saved();
            }
            Err(error) => self.failed(error),
        }
    }
    fn update_automation(&mut self, message: automation::Message) -> Task<Message> {
        use automation::Message as Edit;
        if matches!(message, Edit::Schedule(_)) && self.page != Page::DesktopAutomation {
            let navigation = self.handle(Message::Page(Page::DesktopAutomation));
            let editor = self.update_automation(message);
            return Task::batch([navigation, editor]);
        }
        if !matches!(self.page, Page::DesktopAutomation | Page::AdaptiveLooks)
            && message != Edit::Refresh
        {
            return Task::none();
        }
        let focus = matches!(
            message,
            Edit::New(automation::Kind::Schedule) | Edit::Schedule(_) | Edit::Edit(_)
        );
        self.automation
            .update(message, self.config.clock.format_24h);
        if focus
            && self
                .automation
                .draft
                .as_ref()
                .is_some_and(|draft| draft.kind == automation::Kind::Schedule)
        {
            return cosmic::widget::text_input::focus(view::automation_time_id());
        }
        Task::none()
    }
    fn update_tools(&mut self, message: tools::Message) -> Task<Message> {
        let (task, saved) = self.tools.update(message, &self.config);
        let close = if matches!(&saved, Some(tools::Saved::Note(_))) {
            let requested = std::mem::take(&mut self.tools.close_after_save);
            requested
                && matches!(&saved, Some(tools::Saved::Note(Ok(()))))
                && self.tools.conflict.is_none()
        } else {
            false
        };
        match saved {
            Some(tools::Saved::Tool(widget, key, value)) => {
                match Config::save_tool(widget, json!({ key: value })) {
                    Ok(config) => {
                        self.config = config;
                        self.placement.refresh(&self.config);
                        self.refresh_canvas();
                        self.saved();
                    }
                    Err(error) => self.failed(error),
                }
            }
            Some(tools::Saved::Note(Ok(()))) if self.tools.conflict.is_none() => {
                self.status = if self.tools.dirty() {
                    "Changes saved · new edits pending"
                } else {
                    "Note saved"
                }
                .into();
                self.error = false;
            }
            Some(tools::Saved::Note(Ok(()))) => {}
            Some(tools::Saved::Note(Err(error))) => self.failed(error),
            None => {}
        }
        if close {
            Task::batch([
                task,
                Task::done(cosmic::Action::App(Message::Window(WindowControl::Close))),
            ])
        } else {
            task
        }
    }
    fn save_rain(&mut self, p: crate::rain::Preferences) {
        match Config::save_rain(self.rain.base.as_ref().unwrap_or(&self.config.rain), &p) {
            Ok(config) => {
                self.rain.base = None;
                if self.config.rain.enabled && !config.rain.enabled {
                    self.rain.clear();
                }
                self.config = config;
                self.saved();
            }
            Err(error) => {
                self.rain.draft = Some(p);
                self.failed(error);
            }
        }
    }
    fn save_glance(&mut self, changes: serde_json::Value) {
        match Config::save_glance(changes) {
            Ok(config) => {
                self.config = config;
                self.placement.refresh(&self.config);
                self.refresh_canvas();
                self.saved();
            }
            Err(error) => self.failed(error),
        }
    }
    fn save(&mut self, changes: serde_json::Value) {
        match Config::save_changes(changes) {
            Ok(config) => {
                self.config = config;
                self.saved();
            }
            Err(error) => self.failed(error),
        }
    }
    fn saved(&mut self) {
        self.status = "Saved".into();
        self.error = false;
    }
    fn failed(&mut self, error: String) {
        self.status = error;
        self.error = true;
    }
    fn working(&mut self, status: &str) {
        self.status = status.into();
        self.busy = true;
        self.error = false;
    }
    fn add_folder(&mut self, input: String) {
        match normalize_folder(&input) {
            Ok(path) => {
                let mut roots = Config::load().project_roots;
                if !roots
                    .iter()
                    .any(|root| normalize_folder(root).ok().as_ref() == Some(&path))
                {
                    roots.push(path);
                    self.save(json!({"project_roots": roots}));
                } else {
                    self.status = "That folder is already included".into();
                    self.error = false;
                }
                if !self.error {
                    self.folder_input.clear();
                }
            }
            Err(error) => self.failed(error),
        }
    }
}

impl Settings {
    /// Legacy aggregate used by desktop editor regression cases.
    #[cfg(test)]
    fn desktop_pending(&self) -> bool {
        self.automation.dirty()
            || self.desktop.dirty()
            || self.system_appearance.dirty()
            || self.wallpaper.dirty()
            || self.desktop_icons.dirty()
            || self.windows.dirty()
            || self.workspaces.dirty()
            || self.window_rules.dirty()
            || self.shortcuts.dirty()
    }
    /// Background media work the Wallpaper page (thumbnails, scans,
    /// analysis) and the Profiles page (its backdrop) asked for.
    fn media_tasks(&mut self) -> Task<Message> {
        let depth = self.depth_preview_sync();
        let wallpaper = self
            .wallpaper
            .take_jobs()
            .into_iter()
            .map(|job| wallpaper::task(job, Message::Wallpaper));
        let profiles = self
            .profiles
            .take_jobs()
            .into_iter()
            .map(|job| profiles::backdrop_task(job, Message::Profiles));
        let themes = self
            .profiles
            .take_theme_jobs()
            .into_iter()
            .map(|job| profiles::theme_task(job, Message::Profiles));
        let shares = self.profiles.take_share_jobs().into_iter().map(|job| {
            cosmic::task::future(async move {
                let id = job.id;
                Message::Profiles(profiles::Message::ShareReady(id, job.run().await))
            })
        });
        Task::batch(
            wallpaper
                .chain(profiles)
                .chain(themes)
                .chain(shares)
                .chain(std::iter::once(depth))
                .collect::<Vec<_>>(),
        )
    }
    /// Wallpaper routing: background results and dialog answers reach the
    /// editor whatever page shows (stale ones are dropped there); edits
    /// only on the page. Stop matching is the existing preference path.
    fn update_wallpaper(&mut self, message: wallpaper::Message) -> Task<Message> {
        use wallpaper::{Message as Edit, Outcome};
        let result = matches!(
            message,
            Edit::Thumb(..)
                | Edit::Animated(..)
                | Edit::Bundled(_)
                | Edit::Scanned(..)
                | Edit::Analysed(..)
                | Edit::AutoAnalysed(..)
                | Edit::Picked(..)
        );
        let color_control = matches!(self.page, Page::Appearance | Page::Themes)
            && matches!(
                message,
                Edit::ThemeColor
                    | Edit::Palette(_)
                    | Edit::SyncColors(_)
                    | Edit::AutomaticColors(_)
                    | Edit::Accent(_)
                    | Edit::SuggestAccent
                    | Edit::LinkOutline(_)
                    | Edit::KeepColors
                    | Edit::AccentText(_)
                    | Edit::AccentSubmit
                    | Edit::PaletteChoices(_)
                    | Edit::ApplyColors
                    | Edit::ClearAccent
                    | Edit::Undo
                    | Edit::Refresh
                    | Edit::UseCurrent
                    | Edit::Recover
                    | Edit::KeepRecovery
            );
        if !result && self.page != Page::DesktopWallpaper && !color_control {
            return Task::none();
        }
        if matches!(message, Edit::StopMatching) {
            return cosmic::Application::update(self, Message::MatchWallpaper(false));
        }
        if matches!(message, Edit::ResolveThemeAccent) {
            self.system_appearance.clear_pending_accent();
            return Task::none();
        }
        let refresh_theme = matches!(
            message,
            Edit::Apply | Edit::ApplyColors | Edit::Undo | Edit::Recover | Edit::KeepRecovery
        );
        let outcome = self.wallpaper.update(message);
        let mut tasks = vec![self.media_tasks()];
        match outcome {
            Outcome::None => {}
            Outcome::Dialog(request) => {
                tasks.push(wallpaper::dialog(request, Message::Wallpaper));
            }
            Outcome::Controller(args) => {
                if preview() {
                    self.failed("Reinstalling is off in preview mode".into());
                } else if !self.busy {
                    self.working("Reinstalling the bundled wallpapers…");
                    tasks.push(controller(args, "Bundled wallpapers reinstalled"));
                }
            }
        }
        if refresh_theme {
            if let Ok(config) = Config::try_load() {
                self.config = config;
                if !self.appearance_editing {
                    self.appearance_draft = self.config.appearance.clone();
                }
            }
            if self.system_appearance.opened() {
                self.system_appearance.refresh();
            }
            if self.windows.opened() {
                self.windows.refresh();
            }
            let _ = self.config.refresh_desktop_palette();
        }

        Task::batch(tasks)
    }
}

/// Runs a Profiles file dialog. The result carries the request's ID, so it
/// can only complete the request that started it. Cancelling does nothing.
fn profile_dialog(request: profiles::Request) -> Task<Message> {
    use cosmic::dialog::ashpd::desktop::ResponseError;
    use cosmic::dialog::file_chooser::{self, FileFilter};
    let filter = || {
        FileFilter::new("Halcyon setup or profile")
            .glob("*.json")
            .glob("*.halcyon")
    };
    let cancelled = |error: &file_chooser::Error| {
        matches!(
            error,
            file_chooser::Error::Cancelled
                | file_chooser::Error::Save(cosmic::dialog::ashpd::Error::Response(
                    ResponseError::Cancelled
                ))
        )
    };
    match request {
        profiles::Request::Import { id } => cosmic::task::future(async move {
            let result = match file_chooser::open::Dialog::new()
                .title("Import a profile")
                .filter(filter())
                .open_file()
                .await
            {
                Ok(response) => match response.url().to_file_path() {
                    Ok(path) => profiles::read_import(&path).map(Some),
                    Err(()) => Err("Choose a local file.".into()),
                },
                Err(error) if cancelled(&error) => Ok(None),
                Err(error) => Err(format!("Could not open the file chooser: {error}")),
            };
            Message::Profiles(profiles::Message::Imported(id, result))
        }),
        profiles::Request::Export {
            id,
            file_name,
            bytes,
            confine,
        } => cosmic::task::future(async move {
            let result = match file_chooser::save::Dialog::new()
                .title("Export profile".to_string())
                .file_name(file_name)
                .filter(filter())
                .save_file()
                .await
            {
                Ok(response) => match response.url().map(|url| url.to_file_path()) {
                    Some(Ok(path)) => profiles::write_export(&path, &bytes, confine.as_deref())
                        .map(|()| Some(path)),
                    Some(Err(())) => Err("Choose a local file.".into()),
                    None => Ok(None),
                },
                Err(error) if cancelled(&error) => Ok(None),
                Err(error) => Err(format!("Could not open the file chooser: {error}")),
            };
            Message::Profiles(profiles::Message::Exported(id, result))
        }),
        profiles::Request::ExportImage {
            id,
            file_name,
            bytes,
            confine,
        } => cosmic::task::future(async move {
            let result = match file_chooser::save::Dialog::new()
                .title("Save preview image".to_string())
                .file_name(file_name)
                .filter(FileFilter::new("PNG image").glob("*.png"))
                .save_file()
                .await
            {
                Ok(response) => match response.url().map(|url| url.to_file_path()) {
                    Some(Ok(path)) => profiles::write_export(&path, &bytes, confine.as_deref())
                        .map(|()| Some(path)),
                    Some(Err(())) => Err("Choose a local file.".into()),
                    None => Ok(None),
                },
                Err(error) if cancelled(&error) => Ok(None),
                Err(error) => Err(format!("Could not open the file chooser: {error}")),
            };
            Message::Profiles(profiles::Message::Exported(id, result))
        }),
    }
}

fn controls_theme(_config: &Config) -> cosmic::Theme {
    cosmic::theme::system_preference()
}

fn capture_placement(event: &serde_json::Value) -> Option<placement::Message> {
    use placement::Message as Edit;
    Some(match event.as_str() {
        Some("save") => Edit::Save,
        Some("cancel") => Edit::Cancel,
        Some("reset") => Edit::Reset,
        Some("automatic") => Edit::Automatic,
        Some("undo") => Edit::Undo,
        Some("center_x") => Edit::Center(0),
        Some("center_y") => Edit::Center(1),
        Some(_) => return None,
        None => {
            if let Some(key) = event["select"].as_str() {
                Edit::Press(key.into())
            } else if let Some(columns) = event["columns"].as_u64() {
                Edit::Columns(u8::try_from(columns).ok()?)
            } else if let Some(snap) = event["snap"].as_bool() {
                Edit::Snap(snap)
            } else {
                let resizing = event.get("resize").is_some();
                let step = event[if resizing { "resize" } else { "nudge" }].as_array()?;
                let axis = |index: usize| step.get(index)?.as_i64()?.clamp(-1, 1).try_into().ok();
                if resizing {
                    Edit::Resize([axis(0)?, axis(1)?], event["large"].as_bool() == Some(true))
                } else {
                    Edit::Nudge([axis(0)?, axis(1)?], event["large"].as_bool() == Some(true))
                }
            }
        }
    })
}
/// Capture-mode driver for the Dock & Panel editor, using its own messages.
fn capture_desktop(event: &serde_json::Value) -> Option<desktop::Message> {
    use crate::desktop::panels::{Region, Slot, Timing};
    use desktop::Message as Edit;
    let slot = |value: &serde_json::Value| {
        Some(Slot {
            region: Region::parse(value.get(0)?.as_str()?)?,
            index: usize::try_from(value.get(1)?.as_u64()?).ok()?,
        })
    };
    let number = |key: &str| {
        event[key]
            .as_u64()
            .and_then(|value| u32::try_from(value).ok())
    };
    fn parse<T: serde::de::DeserializeOwned>(event: &serde_json::Value, key: &str) -> Option<T> {
        serde_json::from_value(event.get(key)?.clone()).ok()
    }
    if let Some(name) = event.as_str() {
        return Some(match name {
            "apply" => Edit::Apply,
            "discard" => Edit::Discard,
            "undo" => Edit::Undo,
            "arrange" => Edit::Islands(desktop::islands::Edit::Arrange),
            "arrange_done" => Edit::Islands(desktop::islands::Edit::Done),
            "arrange_cancel" => Edit::Islands(desktop::islands::Edit::Cancel),
            "islands_check" => Edit::Islands(desktop::islands::Edit::Check),
            "starter" => Edit::Starter(true),
            "use_starter" => Edit::UseStarter,
            "drop" => Edit::Drop,
            "cancel" => Edit::DragCancel,
            "refresh" => Edit::Refresh,
            "use_current" => Edit::UseCurrent,
            "recover" => Edit::Recover,
            "keep" => Edit::KeepRecovery,
            _ => return None,
        });
    }
    Some(if let Some(kind) = event["select"].as_str() {
        Edit::Select(match kind {
            "dock" => crate::desktop::panels::PanelKind::Dock,
            "panel" => crate::desktop::panels::PanelKind::Panel,
            _ => return None,
        })
    } else if let Some(value) = event.get("select_applet") {
        Edit::SelectApplet(slot(value))
    } else if let Some(value) = event.get("move_to") {
        Edit::MoveTo(slot(value)?, Region::parse(value.get(2)?.as_str()?)?)
    } else if let Some(value) = event["panel_style"].as_str() {
        Edit::PanelStyle(match value {
            "full_width" => desktop::PanelStyle::FullWidth,
            "islands" => desktop::PanelStyle::Islands,
            _ => return None,
        })
    } else if let Some(value) = event["applet_group"].as_str() {
        Edit::AppletGroup(
            crate::desktop::applets::AppletGroup::ALL
                .into_iter()
                .find(|g| g.title().eq_ignore_ascii_case(value))?,
        )
    } else if let Some(enabled) = event["islands"].as_bool() {
        Edit::Islands(desktop::islands::Edit::Enabled(enabled))
    } else if let Some(mode) = parse(event, "island_placement") {
        Edit::Islands(desktop::islands::Edit::Placement(mode))
    } else if let Some(gap) = number("island_gap") {
        Edit::Islands(desktop::islands::Edit::Gap(u16::try_from(gap).ok()?))
    } else if let Some(anchor) = parse(event, "anchor") {
        Edit::Anchor(anchor)
    } else if let Some(size) = parse(event, "size") {
        Edit::Size(size)
    } else if let Some(mode) = parse(event, "autohide") {
        Edit::AutoHide(mode)
    } else if let Some(value) = event["floating"].as_bool() {
        Edit::Floating(value)
    } else if let Some(value) = event["extend"].as_bool() {
        Edit::Extend(value)
    } else if let Some(value) = event["advanced"].as_bool() {
        Edit::Advanced(value)
    } else if let Some(value) = number("opacity") {
        Edit::Opacity(u8::try_from(value).ok()?)
    } else if let Some(value) = number("radius") {
        Edit::Radius(value)
    } else if let Some(value) = number("spacing") {
        Edit::Spacing(value)
    } else if let Some(value) = number("padding") {
        Edit::Padding(value)
    } else if let Some(value) = number("margin") {
        Edit::Margin(u16::try_from(value).ok()?)
    } else if let Some(output) = parse(event, "output") {
        Edit::Output(output)
    } else if let Some(value) = event.get("picker") {
        Edit::Picker(value.as_str().and_then(Region::parse))
    } else if let Some(value) = event["applet_search"].as_str() {
        Edit::Search(value.to_string())
    } else if let Some(open) = event["installed"].as_bool() {
        Edit::Overview(open)
    } else if let Some(id) = event["add_to_bar"].as_str() {
        Edit::AddToBar(id.to_string())
    } else if let Some(add) = event.get("add") {
        Edit::Add(
            Region::parse(add.get(0)?.as_str()?)?,
            add.get(1)?.as_str()?.to_string(),
        )
    } else if let Some(value) = event.get("remove") {
        Edit::Remove(slot(value)?)
    } else if let Some(value) = event.get("step") {
        Edit::Step(slot(value)?, value.get(2)?.as_i64()?.signum() as i32)
    } else if let Some(value) = event.get("drag_start") {
        Edit::DragStart(slot(value)?)
    } else if let Some(value) = event.get("drag_over") {
        Edit::DragOver(slot(value)?)
    } else if let Some(value) = event.get("timing") {
        let timing = match value.get(0)?.as_str()? {
            "wait" => Timing::Wait,
            "transition" => Timing::Transition,
            "handle" => Timing::Handle,
            "unhide" => Timing::Unhide,
            _ => return None,
        };
        Edit::Timing(timing, u32::try_from(value.get(1)?.as_u64()?).ok()?)
    } else {
        return None;
    })
}
/// Capture-mode driver for the System appearance editor, using its messages.
fn capture_appearance(event: &serde_json::Value) -> Option<system_appearance::Message> {
    use crate::desktop::appearance::{ColorRole, FontRole, Surface, SystemMode, ThemeVariant};
    use system_appearance::{Message as Edit, Picker};
    if let Some(name) = event.as_str() {
        return Some(match name {
            "apply" => Edit::Apply,
            "discard" => Edit::Discard,
            "undo" => Edit::Undo,
            "refresh" => Edit::Refresh,
            "use_current" => Edit::UseCurrent,
            "recover" => Edit::Recover,
            "keep" => Edit::KeepRecovery,
            _ => return None,
        });
    }
    let text = |key: &str| event[key].as_str();
    let number = |key: &str| event[key].as_u64();
    let font_role = |value: &str| match value {
        "interface" => Some(FontRole::Interface),
        "monospace" => Some(FontRole::Monospace),
        _ => None,
    };
    Some(if let Some(variant) = text("variant") {
        Edit::Variant(match variant {
            "light" => ThemeVariant::Light,
            "dark" => ThemeVariant::Dark,
            _ => return None,
        })
    } else if let Some(mode) = text("mode") {
        Edit::Mode(match mode {
            "light" => SystemMode::Light,
            "dark" => SystemMode::Dark,
            "automatic" => SystemMode::Automatic,
            _ => return None,
        })
    } else if let Some(color) = event.get("color") {
        let role = ColorRole::parse(color.get(0)?.as_str()?)?;
        match color.get(1)?.as_str()? {
            "auto" => Edit::Color(role, None),
            hex => Edit::Color(
                role,
                Some(crate::desktop::appearance::parse_hex(hex, role.alpha())?),
            ),
        }
    } else if let Some(hex) = event.get("hex") {
        Edit::HexInput(
            ColorRole::parse(hex.get(0)?.as_str()?)?,
            hex.get(1)?.as_str()?.to_string(),
        )
    } else if let Some(role) = text("submit") {
        Edit::HexSubmit(ColorRole::parse(role)?)
    } else if let Some(width) = number("outline") {
        Edit::OutlineWidth(u32::try_from(width).ok()?)
    } else if let Some(value) = event.get("roundness") {
        Edit::Roundness(serde_json::from_value(value.clone()).ok()?)
    } else if let Some(value) = event.get("density") {
        Edit::Density(serde_json::from_value(value.clone()).ok()?)
    } else if let Some(value) = event.get("frosted") {
        Edit::Frosted(
            Surface::parse(value.get(0)?.as_str()?)?,
            value.get(1)?.as_bool()?,
        )
    } else if let Some(step) = number("blur") {
        Edit::Blur(u8::try_from(step).ok()?)
    } else if let Some(percent) = number("glass") {
        Edit::Glass(u8::try_from(percent.min(100)).ok()?)
    } else if let Some(open) = event["effects"].as_bool() {
        Edit::Effects(open)
    } else if let Some(value) = event.get("window") {
        Edit::Window(
            crate::desktop::appearance::WindowEffect::parse(value.get(0)?.as_str()?)?,
            value.get(1)?.as_bool()?,
        )
    } else if let Some(value) = event.get("picker") {
        Edit::Picker(match value.as_str() {
            None => None,
            Some("icons") => Some(Picker::Icons),
            Some(role) => Some(Picker::Font(font_role(role)?)),
        })
    } else if let Some(query) = text("search") {
        Edit::Search(query.to_string())
    } else if let Some(value) = event.get("font") {
        Edit::FontFamily(
            font_role(value.get(0)?.as_str()?)?,
            value.get(1)?.as_str()?.to_string(),
        )
    } else if let Some(id) = text("icons") {
        Edit::IconTheme(id.to_string())
    } else {
        return None;
    })
}
fn capture_devbox(event: &serde_json::Value) -> Option<Message> {
    let section = |key: &str| {
        DevboxSection::ALL
            .into_iter()
            .find(|section| section.key() == key)
    };
    Some(match event.as_str() {
        Some("drop") => Message::DragDrop,
        Some("cancel") => Message::DragCancel,
        Some(_) => return None,
        None => {
            if let Some(key) = event["history"].as_str() {
                let section = section(key)?;
                if matches!(section, DevboxSection::Uptime | DevboxSection::Services) {
                    return None;
                }
                Message::DevboxHistory(section.key(), event["value"].as_bool()?)
            } else if let Some(key) = event["toggle"].as_str() {
                let key = match key {
                    "memory_cache" => "memory_cache",
                    "swap" => "swap",
                    _ => section(key)?.key(),
                };
                Message::Devbox(key, event["value"].as_bool()?)
            } else if let Some(key) = event["move"].as_str() {
                Message::DevboxMove(section(key)?, event["direction"].as_i64()?.signum() as i32)
            } else if let Some(key) = event["drag_start"].as_str() {
                Message::DevboxDragStart(section(key)?)
            } else if let Some(key) = event["drag_over"].as_str() {
                Message::DevboxDragOver(section(key)?)
            } else {
                Message::DevboxDragLeave(section(event["drag_leave"].as_str()?)?)
            }
        }
    })
}
fn capture_glance(event: &serde_json::Value) -> Option<Message> {
    if let Some(enabled) = event["enabled"].as_bool() {
        return Some(Message::Widget(Widget::Glance, enabled));
    }
    if let Some(style) = event.get("style") {
        return Some(Message::GlanceStyle(
            serde_json::from_value(style.clone()).ok()?,
        ));
    }
    if let Some(density) = event.get("density") {
        return Some(Message::GlanceDensity(
            serde_json::from_value(density.clone()).ok()?,
        ));
    }
    if let Some(opacity) = event["opacity"].as_u64().filter(|value| *value <= 100) {
        return Some(Message::GlanceOpacity(opacity as u8));
    }
    let key = match event["section"].as_str()? {
        "show_clock" => "show_clock",
        "show_weather" => "show_weather",
        "show_calendar" => "show_calendar",
        "show_tomorrow" => "show_tomorrow",
        _ => return None,
    };
    Some(Message::GlanceSection(key, event["value"].as_bool()?))
}
fn preview() -> bool {
    std::env::var_os("DESKTOP_CARDS_CONFIG").is_some()
}
fn capture_dir() -> Option<PathBuf> {
    std::env::var_os("DESKTOP_CARDS_SETTINGS_CAPTURE_DIR").map(PathBuf::from)
}
/// Capture commands (editor events, scroll, close) need both an alternate
/// app config and a capture folder; either alone leaves only page links.
fn capture_commands() -> bool {
    capture_gate(preview(), capture_dir().is_some())
}
fn capture_gate(alternate_config: bool, capture_dir: bool) -> bool {
    alternate_config && capture_dir
}
/// A capture command may write desktop settings or the change journal only
/// through an explicit, existing fixture root; previews still edit drafts.
/// Capture-mode driver for the Windows editor, using its messages.
fn capture_windows(event: &serde_json::Value) -> Option<windows::Message> {
    use crate::desktop::appearance::ThemeVariant;
    use crate::desktop::windows::{Behavior, Gap, TileBehavior};
    use windows::Message as Edit;
    if let Some(name) = event.as_str() {
        return Some(match name {
            "apply" => Edit::Apply,
            // "apply_scope" is the earlier name of the first tiling step.
            "apply_first" | "apply_scope" => Edit::ApplyFirst,
            "discard" => Edit::Discard,
            "undo" => Edit::Undo,
            "refresh" => Edit::Refresh,
            "use_current" => Edit::UseCurrent,
            "recover" => Edit::Recover,
            "keep" => Edit::KeepRecovery,
            _ => return None,
        });
    }
    let text = |key: &str| event[key].as_str();
    let flag = |key: &str| event[key].as_bool();
    Some(if let Some(on) = flag("autotile") {
        Edit::Autotile(on)
    } else if let Some(scope) = text("scope") {
        Edit::Scope(match scope {
            "global" => TileBehavior::Global,
            "per_workspace" => TileBehavior::PerWorkspace,
            _ => return None,
        })
    } else if let Some(variant) = text("variant") {
        Edit::Variant(match variant {
            "light" => ThemeVariant::Light,
            "dark" => ThemeVariant::Dark,
            _ => return None,
        })
    } else if let Some(gap) = event.get("gap") {
        let side = match gap.get(0)?.as_str()? {
            "outer" => Gap::Outer,
            "inner" => Gap::Inner,
            _ => return None,
        };
        Edit::Gap(side, u32::try_from(gap.get(1)?.as_u64()?).ok()?)
    } else if let Some(pair) = event.get("flag") {
        let name = pair.get(0)?.as_str()?;
        let behavior = Behavior::ALL.into_iter().find(|b| b.name() == name)?;
        Edit::Flag(behavior, pair.get(1)?.as_bool()?)
    } else if let Some(delay) = text("delay") {
        Edit::DelayInput(delay.into())
    } else if let Some(on) = flag("edge_snap") {
        Edit::EdgeSnap(on)
    } else if let Some(distance) = text("snap") {
        Edit::SnapInput(distance.into())
    } else if let Some(open) = flag("advanced") {
        Edit::Advanced(open)
    } else {
        return None;
    })
}

/// Capture-mode driver for the Workspaces editor, using its messages.
fn capture_workspaces(event: &serde_json::Value) -> Option<workspaces::Message> {
    use crate::desktop::workspaces::{TypingAction, WorkspaceLayout, WorkspaceMode};
    use workspaces::Message as Edit;
    if let Some(name) = event.as_str() {
        return Some(match name {
            "apply" => Edit::Apply,
            "discard" => Edit::Discard,
            "undo" => Edit::Undo,
            "refresh" => Edit::Refresh,
            "use_current" => Edit::UseCurrent,
            "recover" => Edit::Recover,
            "keep" => Edit::KeepRecovery,
            _ => return None,
        });
    }
    let text = |key: &str| event[key].as_str();
    Some(if let Some(mode) = text("mode") {
        Edit::Mode(match mode {
            "output_bound" => WorkspaceMode::OutputBound,
            "global" => WorkspaceMode::Global,
            _ => return None,
        })
    } else if let Some(layout) = text("layout") {
        Edit::Layout(match layout {
            "vertical" => WorkspaceLayout::Vertical,
            "horizontal" => WorkspaceLayout::Horizontal,
            _ => return None,
        })
    } else if let Some(typing) = text("typing") {
        Edit::Typing(match typing {
            "none" => TypingAction::None,
            "open_launcher" => TypingAction::OpenLauncher,
            "open_applications" => TypingAction::OpenApplications,
            _ => return None,
        })
    } else if let Some(on) = event["wraparound"].as_bool() {
        Edit::Wraparound(on)
    } else {
        return None;
    })
}

/// Capture-mode driver for the Floating apps editor, using its messages.
fn capture_window_rules(event: &serde_json::Value) -> Option<window_rules::Message> {
    use window_rules::Message as Edit;
    if let Some(name) = event.as_str() {
        return Some(match name {
            "add" => Edit::Add,
            "save" => Edit::Save,
            "cancel" => Edit::Cancel,
            "apply" => Edit::Apply,
            "discard" => Edit::Discard,
            "undo" => Edit::Undo,
            "refresh" => Edit::Refresh,
            "use_current" => Edit::UseCurrent,
            "recover" => Edit::Recover,
            "keep" => Edit::KeepRecovery,
            _ => return None,
        });
    }
    let text = |key: &str| event[key].as_str().map(str::to_owned);
    let pair = |key: &str| -> Option<(String, String)> {
        let value = event.get(key)?;
        Some((
            value.get(0)?.as_str()?.into(),
            value.get(1)?.as_str()?.into(),
        ))
    };
    Some(if let Some(value) = text("search") {
        Edit::Search(value)
    } else if let Some((appid, title)) = pair("edit") {
        Edit::Edit(appid, title)
    } else if let Some(value) = text("app_id") {
        Edit::AppId(value)
    } else if let Some(value) = text("title") {
        Edit::Title(value)
    } else if let Some(on) = event["advanced"].as_bool() {
        Edit::Advanced(on)
    } else if let Some(value) = text("app_pattern") {
        Edit::AppPattern(value)
    } else if let Some(value) = text("title_pattern") {
        Edit::TitlePattern(value)
    } else if let Some(open) = event["picker"].as_bool() {
        Edit::Picker(open)
    } else if let Some(value) = text("pick") {
        Edit::Pick(value)
    } else if let Some(toggle) = event.get("toggle") {
        Edit::Toggle(
            toggle.get(0)?.as_str()?.into(),
            toggle.get(1)?.as_str()?.into(),
            toggle.get(2)?.as_bool()?,
        )
    } else if let Some((appid, title)) = pair("remove") {
        Edit::Remove(appid, title)
    } else if let Some((appid, title)) = pair("expand") {
        Edit::Expand(Some((appid, title)))
    } else {
        return None;
    })
}

/// Capture-mode driver for the Shortcuts editor, using its messages.
/// Combinations are given as text (`"Super+Shift+K"`).
fn capture_shortcuts(event: &serde_json::Value) -> Option<shortcuts::Message> {
    use crate::desktop::shortcuts::{Binding, Target};
    use shortcuts::{Answer, Filter, Message as Edit};
    if let Some(name) = event.as_str() {
        return Some(match name {
            "add" => Edit::Add,
            "save" => Edit::Save,
            "cancel" => Edit::Cancel,
            "replace" => Edit::Answer(Answer::Replace),
            "turn_back_on" => Edit::Answer(Answer::TurnBackOn),
            "keep_off" => Edit::Answer(Answer::KeepOff),
            "cancel_question" => Edit::Answer(Answer::Cancel),
            "apply" => Edit::Apply,
            "discard" => Edit::Discard,
            "undo" => Edit::Undo,
            "refresh" => Edit::Refresh,
            "use_current" => Edit::UseCurrent,
            "recover" => Edit::Recover,
            "keep" => Edit::KeepRecovery,
            _ => return None,
        });
    }
    let text = |key: &str| event[key].as_str().map(str::to_owned);
    let ident = |key: &str| {
        Binding::parse_partial(event[key].as_str()?)
            .ok()
            .map(|b| b.ident())
    };
    Some(if let Some(value) = text("search") {
        Edit::Search(value)
    } else if let Some(filter) = text("filter") {
        Edit::Filter(match filter.as_str() {
            "all" => Filter::All,
            "custom" => Filter::Custom,
            "builtin" => Filter::Builtin,
            _ => return None,
        })
    } else if let Some(at) = ident("edit") {
        Edit::Edit(Target::Entry(at))
    } else if let Some(at) = ident("edit_paused") {
        Edit::Edit(Target::Paused(at))
    } else if let Some(value) = text("name") {
        Edit::Name(value)
    } else if let Some(value) = text("combo") {
        Edit::Combo(value)
    } else if let Some(value) = text("command") {
        Edit::Command(value)
    } else if let Some(pair) = event.get("running") {
        let at = Binding::parse_partial(pair.get(0)?.as_str()?).ok()?.ident();
        Edit::Running(at, pair.get(1)?.as_bool()?)
    } else if let Some(pair) = event.get("default") {
        let at = Binding::parse_partial(pair.get(0)?.as_str()?).ok()?.ident();
        Edit::Default(at, pair.get(1)?.as_bool()?)
    } else if let Some(at) = ident("remove") {
        Edit::Remove(Target::Entry(at))
    } else if let Some(at) = ident("remove_paused") {
        Edit::Remove(Target::Paused(at))
    } else if let Some(at) = ident("restore") {
        Edit::Restore(at)
    } else if let Some(at) = ident("expand") {
        Edit::Expand(Some(at))
    } else {
        return None;
    })
}

/// Capture-mode driver for the Wallpaper editor, using its messages.
fn capture_wallpaper(event: &serde_json::Value) -> Option<wallpaper::Message> {
    use crate::desktop::wallpaper::{BgColor, BgGradient, SamplingMethod, Target, srgb_to_linear};
    use wallpaper::{FitChoice, Message as Edit, Pick, Tab, parse_rgb};
    if let Some(name) = event.as_str() {
        return Some(match name {
            "use_shared" => Edit::UseShared,
            "suggest_accent" => Edit::SuggestAccent,
            "clear_accent" => Edit::ClearAccent,
            "resolve_theme_accent" => Edit::ResolveThemeAccent,
            "keep_colors" => Edit::KeepColors,
            "apply" => Edit::Apply,
            "discard" => Edit::Discard,
            "undo" => Edit::Undo,
            "refresh" => Edit::Refresh,
            "use_current" => Edit::UseCurrent,
            "recover" => Edit::Recover,
            "keep" => Edit::KeepRecovery,
            _ => return None,
        });
    }
    let text = |key: &str| event[key].as_str();
    Some(if let Some(enabled) = event["automatic_colors"].as_bool() {
        Edit::AutomaticColors(enabled)
    } else if let Some(text) = text("accent_text") {
        Edit::AccentText(text.into())
    } else if let Some(sync) = event["sync_colors"].as_bool() {
        Edit::SyncColors(sync)
    } else if let Some(color) = text("accent") {
        Edit::Accent(parse_rgb(color)?.map(|channel| (channel * 255.).round() as u8))
    } else if let Some(page) = event["page"].as_u64() {
        // Gallery page (zero-based; clamped by the editor). Reads only.
        Edit::Page(usize::try_from(page).ok()?)
    } else if let Some(tab) = text("tab") {
        Edit::Tab(match tab {
            "desktop_cards" => Tab::Bundled,
            "cosmic" => Tab::Cosmic,
            "yours" => Tab::Yours,
            "colors" => Tab::Colors,
            _ => return None,
        })
    } else if let Some(select) = event.get("select") {
        Edit::Select(if let Some(name) = select["bundled"].as_str() {
            Pick::Bundled(name.into())
        } else if let Some(color) = select["color"].as_str() {
            Pick::Color(BgColor::Single(parse_rgb(color)?))
        } else if let Some(stops) = select["gradient"].as_array() {
            let colors = stops
                .iter()
                .map(|stop| Some(parse_rgb(stop.as_str()?)?.map(srgb_to_linear)))
                .collect::<Option<Vec<_>>>()?;
            if !(2..=8).contains(&colors.len()) {
                return None;
            }
            Pick::Color(BgColor::Gradient(BgGradient {
                colors,
                radius: select["angle"].as_f64().unwrap_or(180.).clamp(0., 360.) as f32,
            }))
        } else {
            return None;
        })
    } else if let Some(fit) = text("fit") {
        Edit::Fit(match fit {
            "fill" => FitChoice::Fill,
            "fit" => FitChoice::Fit,
            "stretch" => FitChoice::Stretch,
            _ => return None,
        })
    } else if let Some(color) = text("fit_color") {
        Edit::FitColor(color.into())
    } else if let Some(same) = event["same"].as_bool() {
        Edit::Same(same)
    } else if let Some(display) = text("display") {
        Edit::Display(match display {
            "shared" => Target::Shared,
            name if crate::desktop::wallpaper::valid_output_name(name) => {
                Target::Output(name.into())
            }
            _ => return None,
        })
    } else if let Some(seconds) = event["every"].as_u64() {
        Edit::Every(seconds)
    } else if let Some(order) = text("order") {
        Edit::Order(match order {
            "in_order" => SamplingMethod::Alphanumeric,
            "shuffle" => SamplingMethod::Random,
            _ => return None,
        })
    } else {
        return None;
    })
}

/// Capture-mode driver for the Desktop icons editor, using its messages.
fn capture_desktop_icons(event: &serde_json::Value) -> Option<desktop_icons::Message> {
    use crate::desktop::desktop_icons::Show;
    use desktop_icons::Message as Edit;
    if let Some(name) = event.as_str() {
        return Some(match name {
            "apply" => Edit::Apply,
            "discard" => Edit::Discard,
            "undo" => Edit::Undo,
            "refresh" => Edit::Refresh,
            "use_current" => Edit::UseCurrent,
            "recover" => Edit::Recover,
            "keep" => Edit::KeepRecovery,
            _ => return None,
        });
    }
    let percent = |key: &str| {
        event[key]
            .as_u64()
            .and_then(|value| u16::try_from(value).ok())
    };
    Some(if let Some(size) = percent("size") {
        Edit::Size(size)
    } else if let Some(spacing) = percent("spacing") {
        Edit::Spacing(spacing)
    } else if let Some(show) = event["show"].as_array() {
        let what = match show.first()?.as_str()? {
            "folder" => Show::Folder,
            "drives" => Show::Drives,
            "trash" => Show::Trash,
            _ => return None,
        };
        Edit::Show(what, show.get(1)?.as_bool()?)
    } else {
        return None;
    })
}

fn capture_automation(event: &serde_json::Value) -> Option<automation::Message> {
    use crate::automation::{Change, ProfileRef};
    use automation::{DraftEdit, Kind, Message as Edit, PauseFor};
    if let Some(name) = event.as_str() {
        return Some(match name {
            "save" => Edit::Save,
            "cancel" => Edit::Cancel,
            "resume" => Edit::Resume,
            "refresh" => Edit::Refresh,
            "start_over" => Edit::StartOver,
            _ => return None,
        });
    }
    let text = |key: &str| event[key].as_str().map(String::from);
    Some(if let Some(kind) = text("new") {
        Edit::New(if kind == "display" {
            Kind::Display
        } else if kind == "solar" {
            Kind::Solar
        } else {
            Kind::Schedule
        })
    } else if let Some(event) = event.get("sun_event") {
        Edit::Draft(DraftEdit::Event(
            serde_json::from_value(event.clone()).ok()?,
        ))
    } else if let Some(source) = event.get("city_source") {
        Edit::Draft(DraftEdit::Source(
            serde_json::from_value(source.clone()).ok()?,
        ))
    } else if let Some(city) = event.get("schedule_city") {
        Edit::ManualCity(serde_json::from_value(city.clone()).ok()?)
    } else if let Some(id) = text("edit") {
        Edit::Edit(id)
    } else if let Some(time) = text("time") {
        Edit::Draft(DraftEdit::Time(time))
    } else if let Some(day) = event["day"].as_u64() {
        Edit::Draft(DraftEdit::Day(day.min(6) as usize))
    } else if let Some(change) = text("change") {
        Edit::Draft(DraftEdit::Change(if change == "disconnect" {
            Change::Disconnect
        } else {
            Change::Connect
        }))
    } else if let Some(display) = event.get("display") {
        Edit::Draft(DraftEdit::Display(display.as_str().map(String::from)))
    } else if let Some(preset) = text("preset") {
        Edit::Draft(DraftEdit::Profile(ProfileRef::Preset(preset)))
    } else if let Some(saved) = text("saved") {
        Edit::Draft(DraftEdit::Profile(ProfileRef::Saved(saved)))
    } else if let Some(on) = event["enable"].as_bool() {
        Edit::Enable(on)
    } else if let Some(id) = text("delete") {
        Edit::Delete(id, event["confirm"].as_bool().unwrap_or(false))
    } else if let Some(pause) = text("pause") {
        Edit::Pause(match pause.as_str() {
            "next" => PauseFor::Next,
            "hour" => PauseFor::Hour,
            "today" => PauseFor::Today,
            _ => PauseFor::Resume,
        })
    } else {
        return None;
    })
}
/// A `"motion"` capture action and whether it writes desktop settings
/// (the sidebar slide is a Halcyon preference, saved like the rest).
fn capture_motion(event: &serde_json::Value) -> Option<(Message, bool)> {
    if let Some(on) = event["animations"].as_bool() {
        let message = motion::Message::Animations(on);
        let writes = message.writes();
        return Some((Message::Motion(message), writes));
    }
    if let Some(on) = event["slide"].as_bool() {
        return Some((Message::SidebarSlide(on), false));
    }
    (event.as_str() == Some("refresh"))
        .then_some((Message::Motion(motion::Message::Refresh), false))
}
fn capture_allows(writes: bool, fixture: bool) -> bool {
    fixture || !writes
}
fn settings_command_path() -> Option<PathBuf> {
    Config::path().map(|path| path.with_file_name("settings-command.json"))
}
fn desktop_command_path() -> Option<PathBuf> {
    if preview() {
        Config::path().map(|path| path.with_file_name("desktop-command.json"))
    } else {
        crate::paths::runtime_dir().map(|dir| dir.join("command.json"))
    }
}
fn desktop_command_on_display(
    action: &'static str,
    status: &'static str,
    display: Option<String>,
) -> Task<Message> {
    let path = desktop_command_path();
    let isolated = preview();
    cosmic::task::future(async move {
        let result = tokio::task::spawn_blocking(move || {
            if !isolated {
                let output = std::process::Command::new("/usr/bin/systemctl")
                    .args(["--user", "start", crate::paths::SERVICE])
                    .output()
                    .map_err(|error| format!("Could not show desktop widgets: {error}"))?;
                if !output.status.success() {
                    return Err("Could not show desktop widgets. Check the Halcyon service.".into());
                }
            }
            let path = path.ok_or_else(|| "Desktop session unavailable".to_string())?;
            let parent = path
                .parent()
                .ok_or_else(|| "Desktop command directory unavailable".to_string())?;
            std::fs::create_dir_all(parent).map_err(|error| error.to_string())?;
            let temp = path.with_extension("tmp");
            std::fs::write(
                &temp,
                json!({"action": action, "display": display}).to_string(),
            )
            .map_err(|error| error.to_string())?;
            std::fs::rename(temp, path).map_err(|error| error.to_string())
        })
        .await
        .unwrap_or_else(|error| Err(error.to_string()));
        Message::CommandDone(status.into(), result)
    })
}
fn normalize_folder(input: &str) -> Result<String, String> {
    let input = input.trim();
    if input.is_empty() {
        return Err("Choose a folder or enter its path".into());
    }
    let path = if input == "~" {
        dirs::home_dir().ok_or("Home directory unavailable")?
    } else if let Some(tail) = input.strip_prefix("~/") {
        dirs::home_dir()
            .ok_or("Home directory unavailable")?
            .join(tail)
    } else {
        PathBuf::from(input)
    };
    let path = path
        .canonicalize()
        .map_err(|_| "That folder does not exist".to_string())?;
    if !path.is_dir() {
        return Err("Choose a folder rather than a file".into());
    }
    Ok(path.to_string_lossy().into_owned())
}
fn run_command(command: PathBuf, args: Vec<String>, status: &'static str) -> Task<Message> {
    cosmic::task::future(async move {
        let result = tokio::task::spawn_blocking(move || {
            let output = std::process::Command::new(command)
                .args(args)
                .output()
                .map_err(|e| e.to_string())?;
            if output.status.success() {
                Ok(())
            } else {
                let error = String::from_utf8_lossy(&output.stderr).trim().to_string();
                Err(if error.is_empty() {
                    format!("Action failed ({})", output.status)
                } else {
                    error
                })
            }
        })
        .await
        .unwrap_or_else(|error| Err(error.to_string()));
        Message::CommandDone(status.into(), result)
    })
}
fn controller(args: Vec<String>, status: &'static str) -> Task<Message> {
    run_command(crate::paths::controller(), args, status)
}
fn systemctl(args: Vec<&'static str>, status: &'static str) -> Task<Message> {
    let mut command = vec!["--user".into()];
    command.extend(args.into_iter().map(String::from));
    command.push(crate::paths::SERVICE.into());
    run_command("/usr/bin/systemctl".into(), command, status)
}
fn service_task() -> Task<Message> {
    cosmic::task::future(async {
        let result = tokio::task::spawn_blocking(|| {
            let check = |verb| {
                std::process::Command::new("/usr/bin/systemctl")
                    .args(["--user", verb, crate::paths::SERVICE])
                    .output()
                    .map(|out| out.status.success())
                    .map_err(|e| e.to_string())
            };
            Ok(ServiceState {
                running: check("is-active")?,
                startup: check("is-enabled")?,
            })
        })
        .await
        .unwrap_or_else(|error| Err(error.to_string()));
        Message::Service(result)
    })
}

#[cfg(test)]
mod future_builder_probe;
#[cfg(test)]
mod onboarding_tests;
#[cfg(test)]
mod theme_journey_tests;
#[cfg(test)]
mod ux_tests;
#[cfg(test)]
mod wizard_tests;
#[cfg(test)]
mod tests {
    use super::*;
    use cosmic::Application;
    use cosmic::iced::advanced::widget::Tree;
    include!("settings/redesign_tests.rs");
    include!("settings/product_tests.rs");
    include!("settings/solar_tests.rs");
    include!("settings/adaptive_tests.rs");
    include!("settings/polish_tests.rs");

    /// What Wallpaper & colors recommends for a solid color background.
    fn recommended_color(rgb: [f32; 3]) -> [u8; 3] {
        crate::desktop::wallpaper::automatic::recommended(
            &crate::desktop::wallpaper::palette::from_color(
                &crate::desktop::wallpaper::BgColor::Single(rgb),
            ),
        )
        .unwrap()
    }

    fn devbox_test_child(name: &str) -> bool {
        if std::env::var("DESKTOP_CARDS_DEVBOX_SETTINGS_TEST").as_deref() == Ok(name) {
            return false;
        }
        let dir = std::env::temp_dir().join(format!("cards-devbox-{name}-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("config.json");
        std::fs::write(
            &path,
            r#"{"theme":"teal","widget_order":["weather","devbox","projects"],"widget_positions":{"weather":[0.5,0.4]},"devbox":{"services":false,"memory_cache":true,"swap":true,"order":["cpu","memory","disk","network","gpu","uptime","services"],"future_metric":17},"future_field":42}"#,
        )
        .unwrap();
        let status = std::process::Command::new(std::env::current_exe().unwrap())
            .args(["--exact", &format!("settings::tests::{name}")])
            .env("DESKTOP_CARDS_DEVBOX_SETTINGS_TEST", name)
            .env("DESKTOP_CARDS_CONFIG", &path)
            .env("DESKTOP_CARDS_SETTINGS_CAPTURE_DIR", dir.join("captures"))
            .status()
            .unwrap();
        let _ = std::fs::remove_dir_all(dir);
        assert!(status.success());
        true
    }

    fn devbox_command(app: &mut Settings, events: serde_json::Value) {
        std::fs::write(
            settings_command_path().unwrap(),
            serde_json::to_vec(&json!({"devbox": events})).unwrap(),
        )
        .unwrap();
        let _ = app.update(Message::Tick);
    }

    fn raw_preferences() -> serde_json::Value {
        serde_json::from_slice(&std::fs::read(Config::path().unwrap()).unwrap()).unwrap()
    }

    /// Which inputs an isolated desktop settings child receives.
    #[derive(Clone, Copy, Default)]
    struct DesktopEnv {
        /// `DESKTOP_CARDS_CONFIG` names an alternate app config.
        app_config: bool,
        /// `DESKTOP_CARDS_SETTINGS_CAPTURE_DIR` is set.
        capture: bool,
        /// `DESKTOP_CARDS_DESKTOP_ROOT` names a fixture root.
        fixture: bool,
    }

    const TEST_ROOT: &str = "DESKTOP_CARDS_TEST_ROOT";

    /// `namespace` may carry its version (`…Builder/v2`); v1 otherwise.
    fn seed(root: &std::path::Path, values: &[(&str, &str, &str)]) {
        for (namespace, key, value) in values {
            let folder = if namespace.contains('/') {
                root.join(namespace)
            } else {
                root.join(namespace).join("v1")
            };
            let path = folder.join(key);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(path, value).unwrap();
        }
    }

    /// Runs `name` in a child process. Every case gets its own HOME and XDG
    /// directories seeded like a live COSMIC desktop, and inherited fixture
    /// and capture inputs are scrubbed, so no case can reach the user's real
    /// configuration or state even if a guard regresses.
    fn desktop_test_child(name: &str, env: DesktopEnv) -> bool {
        if std::env::var("DESKTOP_CARDS_DESKTOP_SETTINGS_TEST").as_deref() == Ok(name) {
            return false;
        }
        let dir = std::env::temp_dir().join(format!("cards-desktop-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let home = dir.join("home");
        let defaults = [
            (
                "com.system76.CosmicPanel",
                "entries",
                "[\"Panel\",\"Dock\"]",
            ),
            ("com.system76.CosmicPanel.Dock", "anchor", "Bottom"),
            ("com.system76.CosmicPanel.Dock", "size", "L"),
            (
                "com.system76.CosmicPanel.Dock",
                "plugins_center",
                "Some([\"a\"])",
            ),
            ("com.system76.CosmicPanel.Panel", "anchor", "Top"),
            ("com.system76.CosmicTheme.Mode", "is_dark", "true"),
            ("com.system76.CosmicTk", "interface_density", "Standard"),
            (
                "com.system76.CosmicTheme.Dark.Builder/v2",
                "active_hint",
                "3",
            ),
            (
                "com.system76.CosmicTheme.Light.Builder/v2",
                "accent",
                "Some(\"#3584E4FF\")",
            ),
        ];
        // The fake live desktop: system defaults plus user overrides.
        seed(&dir.join("data/cosmic"), &defaults);
        seed(
            &home.join(".config/cosmic"),
            &[
                ("com.system76.CosmicPanel.Dock", "anchor", "Right"),
                (
                    "com.system76.CosmicTheme.Light.Builder/v2",
                    "accent",
                    "Some(\"#E5A50AFF\")",
                ),
            ],
        );
        std::fs::create_dir_all(home.join(".local/share")).unwrap();
        let preferences = r#"{"theme":"teal","future_field":42}"#;
        let config = dir.join("app/config.json");
        std::fs::create_dir_all(config.parent().unwrap()).unwrap();
        std::fs::write(&config, preferences).unwrap();
        // Without an alternate config the app uses its default path in HOME.
        let default_config = home.join(".config/halcyon/config.json");
        std::fs::create_dir_all(default_config.parent().unwrap()).unwrap();
        std::fs::write(&default_config, preferences).unwrap();
        let root = dir.join("cosmic-root");
        seed(&root.join("system/cosmic"), &defaults);
        // Fixture discovery: a font manifest and two icon themes.
        std::fs::write(
            root.join("fonts"),
            "interface\tOpen Sans\t400,600,700\titalic\ninterface\tFira Sans\t400\nmonospace\tNoto Sans Mono\t400,700\n",
        )
        .unwrap();
        for (id, name) in [("Cosmic", "COSMIC"), ("Pop", "Pop")] {
            let theme = root.join("icons").join(id);
            std::fs::create_dir_all(&theme).unwrap();
            std::fs::write(
                theme.join("index.theme"),
                format!("[Icon Theme]\nName={name}\nDirectories=scalable\n"),
            )
            .unwrap();
        }
        let mut command = std::process::Command::new(std::env::current_exe().unwrap());
        command
            .args(["--exact", &format!("settings::tests::{name}")])
            .env("DESKTOP_CARDS_DESKTOP_SETTINGS_TEST", name)
            .env(TEST_ROOT, &dir)
            .env("HOME", &home)
            .env("XDG_CONFIG_HOME", home.join(".config"))
            .env("XDG_DATA_HOME", home.join(".local/share"))
            .env("XDG_DATA_DIRS", dir.join("data"))
            .env("XDG_STATE_HOME", home.join(".local/state"))
            .env("XDG_CACHE_HOME", home.join(".cache"))
            .env("XDG_RUNTIME_DIR", dir.join("runtime"))
            .env_remove("DESKTOP_CARDS_CONFIG")
            .env_remove("DESKTOP_CARDS_SETTINGS_CAPTURE_DIR")
            .env_remove("DESKTOP_CARDS_SETTINGS_SIZE")
            .env_remove(crate::desktop::store::FIXTURE_ENV);
        if env.app_config {
            command.env("DESKTOP_CARDS_CONFIG", &config);
        }
        if env.capture {
            command.env("DESKTOP_CARDS_SETTINGS_CAPTURE_DIR", dir.join("captures"));
        }
        if env.fixture {
            command.env(crate::desktop::store::FIXTURE_ENV, &root);
        }
        if name == "rain_native_settings_preview_commit_undo_and_clipping"
            && std::env::var_os("VK_DRIVER_FILES").is_none()
            && std::env::var_os("VK_ICD_FILENAMES").is_none()
        {
            // XDG_DATA_DIRS above deliberately isolates desktop settings, but
            // Vulkan also uses it to find drivers. Keep the actual installed
            // drivers available without exposing the user's desktop config.
            let drivers: Vec<_> = ["/etc/vulkan/icd.d", "/usr/share/vulkan/icd.d"]
                .into_iter()
                .filter_map(|path| std::fs::read_dir(path).ok())
                .flatten()
                .filter_map(Result::ok)
                .map(|entry| entry.path())
                .filter(|path| path.extension().is_some_and(|ext| ext == "json"))
                .collect();
            if !drivers.is_empty() {
                command.env("VK_DRIVER_FILES", std::env::join_paths(drivers).unwrap());
            }
        }
        let status = command.status().unwrap();
        let _ = std::fs::remove_dir_all(dir);
        assert!(status.success());
        true
    }

    /// The child's temporary root. Refuses to continue unless HOME, the XDG
    /// directories and every desktop store path point inside it.
    fn isolated_root() -> PathBuf {
        let root = PathBuf::from(std::env::var_os(TEST_ROOT).expect("isolated child"));
        assert!(root.starts_with(std::env::temp_dir()));
        for name in ["HOME", "XDG_CONFIG_HOME", "XDG_DATA_HOME", "XDG_DATA_DIRS"] {
            let value = PathBuf::from(std::env::var_os(name).unwrap());
            assert!(value.starts_with(&root), "{name} escapes the test root");
        }
        assert!(Config::path().unwrap().starts_with(&root));
        root
    }

    /// The fake live desktop: user config, data and system defaults, with
    /// file contents. libcosmic's own theme and toolkit watchers create their
    /// empty `com.system76.CosmicTheme.*` and `com.system76.CosmicTk`
    /// directories when the app starts and renders; those directories are
    /// left out, any file in them is still compared.
    fn live_tree(root: &std::path::Path) -> std::collections::BTreeMap<PathBuf, Option<Vec<u8>>> {
        const LIBCOSMIC: [&str; 2] = ["com.system76.CosmicTheme.", "com.system76.CosmicTk"];
        let mut tree = crate::desktop::store::tests::tree(&root.join("home"));
        tree.extend(crate::desktop::store::tests::tree(&root.join("data")));
        let cosmic = root.join("home/.config/cosmic");
        tree.retain(|path, contents| {
            let libcosmic = contents.is_none()
                && path.strip_prefix(&cosmic).is_ok_and(|rest| {
                    let rest = rest.to_string_lossy();
                    LIBCOSMIC.iter().any(|prefix| rest.starts_with(prefix))
                });
            let navigation_dir = root.join("home/.local/share/halcyon");
            let navigation = path == &navigation_dir.join("settings-destination")
                || path == &navigation_dir.join("settings-app-destination")
                || (contents.is_none() && path == &navigation_dir);
            !libcosmic && !navigation && !path.ends_with("settings-command.json")
        });
        tree
    }

    fn destination_independent_state(
        state: &std::path::Path,
    ) -> std::collections::BTreeMap<PathBuf, Option<Vec<u8>>> {
        if !state.exists() {
            return Default::default();
        }
        let mut tree = crate::desktop::store::tests::tree(state);
        tree.retain(|path, _| {
            path != &state.join("settings-destination")
                && path != &state.join("settings-app-destination")
        });
        tree
    }

    fn dock_key(name: &str) -> crate::desktop::ConfigKey {
        crate::desktop::ConfigKey::new("com.system76.CosmicPanel.Dock", 1, name).unwrap()
    }

    /// Opens the editor and checks that its store reads and writes only
    /// below the test root before anything else happens.
    fn open_isolated_editor(root: &std::path::Path) -> Settings {
        let (mut app, task) = Settings::init(Core::default(), Page::DesktopDock);
        settle_settings(&mut app, task);
        let store = app.desktop.store().expect("desktop store");
        assert!(store.user_path(&dock_key("anchor")).starts_with(root));
        app
    }

    /// Applied files in the fixture root, without empty locks or the
    /// private recovery records that legitimately track unapplied edits.
    fn fixture_files(root: &std::path::Path) -> std::collections::BTreeMap<PathBuf, Vec<u8>> {
        crate::desktop::store::tests::tree(root)
            .into_iter()
            .filter_map(|(path, bytes)| Some((path, bytes?)))
            .filter(|(path, bytes)| {
                !(bytes.is_empty() && path.extension().is_some_and(|ext| ext == "lock"))
                    && !(path.parent() == Some(root.join("state").as_path())
                        && path.file_name().is_some_and(|name| {
                            name.to_string_lossy().starts_with("settings-")
                                || name == "desktop-undo.json"
                        }))
            })
            .collect()
    }

    #[test]
    fn clock_native_controls_and_center_actions_fit_small_settings_windows() {
        if desktop_test_child(
            "clock_native_controls_and_center_actions_fit_small_settings_windows",
            DesktopEnv {
                app_config: true,
                fixture: true,
                ..Default::default()
            },
        ) {
            return;
        }
        use crate::native_test::Native;
        use cosmic::iced::{Event, mouse};
        fn click(app: &mut Settings, native: &mut Native, id: &str) {
            let bounds = native_reveal(app, native, id);
            assert!(
                bounds.y >= 0. && bounds.y + bounds.height <= native.size.height,
                "Control must be visible: {id} {bounds:?}"
            );
            assert!(
                bounds.x + bounds.width <= native.size.width,
                "Control must fit horizontally: {id}"
            );
            let point = bounds.center();
            for input in [
                mouse::Event::CursorMoved { position: point },
                mouse::Event::ButtonPressed(mouse::Button::Left),
                mouse::Event::ButtonReleased(mouse::Button::Left),
            ] {
                for message in native
                    .event(app.settings_view(), Event::Mouse(input), point)
                    .0
                {
                    let _ = app.update(message);
                }
            }
        }
        for size in [Size::new(820., 600.), Size::new(1024., 720.)] {
            for combined in [false, true] {
                Config::save_changes(json!({"enabled_widgets":["clock","weather"],
                    "clock":{"scale":100,"alignment":"center","center_vertically":true},
                    "glance":{"enabled":combined,"initialized":true}, "widget_sizes":{"clock":[320,140]},
                    "widget_positions":{}})).unwrap();
                let (mut app, _) = Settings::init(Core::default(), Page::Clock);
                let _ = app.update(Message::Resized(size));
                let mut native = Native::new(size);
                click(&mut app, &mut native, "clock-scale-up");
                assert_eq!(Config::load().clock.scale, 110);
                native.capture(
                    app.settings_view(),
                    &controls_theme(&app.config),
                    &format!("clock-settings-{}-{combined}", size.width),
                );
                let saved_layout = raw_preferences();
                click(&mut app, &mut native, "clock-edit-layout");
                assert_eq!(app.page, Page::Overview);
                assert_eq!(
                    app.placement.selected(),
                    Some(if combined {
                        Widget::Glance
                    } else {
                        Widget::Clock
                    })
                );
                click(&mut app, &mut native, "layout-center-x");
                click(&mut app, &mut native, "layout-center-y");
                assert!(app.canvas.is_some());
                assert_eq!(
                    raw_preferences(),
                    saved_layout,
                    "alignment remains a local preview, including an already-centered no-op"
                );
                let key = if combined { "glance" } else { "clock" };
                let card = native.bounds(app.settings_view(), &format!("placement-card-{key}"));
                let desktop = native.bounds(app.settings_view(), "placement-canvas");
                assert!((card.center_x() - desktop.center_x()).abs() < 1.);
                assert!((card.center_y() - desktop.center_y()).abs() < 1.);
                native.capture(
                    app.settings_view(),
                    &controls_theme(&app.config),
                    &format!("center-controls-{}-{combined}", size.width),
                );
                if app.canvas_dirty() {
                    let id = app.canvas.as_ref().unwrap().id;
                    send_settings(&mut app, Message::Canvas(canvas::Message::Apply(id)));
                } else {
                    send_settings(&mut app, Message::Canvas(canvas::Message::Cancel));
                }
                assert!(!app.error, "{}", app.status);
                assert!(!app.placement.dirty());
            }
        }
    }

    #[test]
    fn home_native_drag_resize_keyboard_save_cancel_and_undo() {
        if desktop_test_child(
            "home_native_drag_resize_keyboard_save_cancel_and_undo",
            DesktopEnv {
                app_config: true,
                fixture: true,
                ..Default::default()
            },
        ) {
            return;
        }
        use crate::native_test::Native;
        use cosmic::iced::{Event, Point, keyboard, mouse};
        fn drive(app: &mut Settings, native: &mut Native, event: Event, point: Point) {
            let (messages, status) = native.event(app.settings_view(), event.clone(), point);
            for message in messages {
                let _ = app.update(message);
            }
            if let Some(message) = placement::input(event, status) {
                let _ = app.update(Message::Placement(message));
            }
        }
        fn pointer(app: &mut Settings, native: &mut Native, point: Point, press: Option<bool>) {
            let event = match press {
                Some(true) => mouse::Event::ButtonPressed(mouse::Button::Left),
                Some(false) => mouse::Event::ButtonReleased(mouse::Button::Left),
                None => mouse::Event::CursorMoved { position: point },
            };
            drive(app, native, Event::Mouse(event), point);
        }
        let mut evidence = Vec::new();
        for size in [Size::new(1024., 720.), Size::new(820., 600.)] {
            Config::save_changes(json!({"setup_dismissed":true, "enabled_widgets":["clock","weather"], "music":{"enabled":true}, "focus":{"enabled":true}, "placement":{"snap":false}, "widget_sizes":{}, "widget_positions":{"clock":[0.,0.],"weather":[0.25,0.4],"music":[0.7,0.2],"focus":[0.7,0.7]}})).unwrap();
            let before = raw_preferences();
            let (mut app, _) = Settings::init(Core::default(), Page::Overview);
            let _ = app.update(Message::Resized(size));
            send_settings(&mut app, Message::Canvas(canvas::Message::Start));
            let mut native = Native::new(size);
            let theme = controls_theme(&app.config);
            native.capture(
                app.settings_view(),
                &theme,
                &format!("home-{}x{}", size.width, size.height),
            );
            let original = native.bounds(app.settings_view(), "placement-card-weather");
            let grab = original.center();
            pointer(&mut app, &mut native, grab, None);
            pointer(&mut app, &mut native, grab, Some(true));
            assert_eq!(app.placement.selected(), Some(Widget::Weather));
            assert_eq!(app.page, Page::Overview);
            let moved = Point::new(grab.x + 24., grab.y - 18.);
            pointer(&mut app, &mut native, moved, None);
            pointer(&mut app, &mut native, moved, Some(false));
            let after_move = native.bounds(app.settings_view(), "placement-card-weather");
            assert!(
                (after_move.x - original.x - 24.).abs() < 1.,
                "pointer move {original:?} → {after_move:?}"
            );
            assert!(
                (after_move.y - original.y + 18.).abs() < 1.,
                "canvas must stay fixed while dragging: {original:?} → {after_move:?}"
            );
            let corner = Point::new(
                after_move.x + after_move.width - 4.,
                after_move.y + after_move.height - 4.,
            );
            pointer(&mut app, &mut native, corner, None);
            pointer(&mut app, &mut native, corner, Some(true));
            assert!(
                app.placement.dragging(),
                "resize grip starts a native gesture"
            );
            let bigger = Point::new(corner.x + 30., corner.y + 26.);
            pointer(&mut app, &mut native, bigger, None);
            pointer(&mut app, &mut native, bigger, Some(false));
            let resized = native.bounds(app.settings_view(), "placement-card-weather");
            assert!(
                (resized.width - after_move.width - 30.).abs() < 1.,
                "{after_move:?} → {resized:?}"
            );
            assert!((resized.height - after_move.height - 26.).abs() < 1.);
            assert_eq!(
                raw_preferences(),
                before,
                "gestures remain local until Apply"
            );
            assert!(app.canvas_dirty());
            let after_gestures = raw_preferences();
            let canvas = native.bounds(app.settings_view(), "placement-canvas");
            let cards: Vec<_> = Widget::ALL
                .into_iter()
                .filter(|widget| app.config.widget_visible(*widget))
                .filter_map(|widget| {
                    native.try_bounds(
                        app.settings_view(),
                        &format!("placement-card-{}", widget.key()),
                    )
                })
                .collect();
            let blank = (1..20)
                .flat_map(|y| (1..20).map(move |x| (x, y)))
                .map(|(x, y)| {
                    Point::new(
                        canvas.x + canvas.width * x as f32 / 20.,
                        canvas.y + canvas.height * y as f32 / 20.,
                    )
                })
                .find(|point| {
                    point.y < size.height - 160. && !cards.iter().any(|card| card.contains(*point))
                })
                .expect("the visible canvas has empty wallpaper outside every widget");
            pointer(&mut app, &mut native, blank, None);
            pointer(&mut app, &mut native, blank, Some(true));
            pointer(&mut app, &mut native, blank, Some(false));
            assert_eq!(
                app.page,
                Page::Overview,
                "empty wallpaper must not navigate away"
            );
            assert_eq!(
                app.placement.selected(),
                None,
                "empty canvas clears selection"
            );
            assert!(app.placement.dirty(), "deselecting retains draft gestures");
            assert_eq!(
                raw_preferences(),
                after_gestures,
                "deselecting does not change saved gestures"
            );
            let _ = app.update(Message::Placement(placement::Message::Select(
                "weather".into(),
            )));
            native.capture(
                app.settings_view(),
                &theme,
                &format!("home-resized-{}x{}", size.width, size.height),
            );
            let id = app.canvas.as_ref().unwrap().id;
            send_settings(&mut app, Message::Canvas(canvas::Message::Apply(id)));
            assert!(!app.error, "{}", app.status);
            assert_ne!(raw_preferences(), before);
            send_settings(&mut app, Message::Canvas(canvas::Message::Start));
            send_settings(
                &mut app,
                Message::Placement(placement::Message::Select("weather".into())),
            );
            let _ = app.update(Message::Placement(placement::Message::ResizeMode(true)));
            let old = app.placement.display_size("weather").unwrap();
            drive(
                &mut app,
                &mut native,
                Event::Keyboard(keyboard::Event::KeyPressed {
                    key: keyboard::Key::Named(keyboard::key::Named::ArrowDown),
                    modified_key: keyboard::Key::Named(keyboard::key::Named::ArrowDown),
                    physical_key: keyboard::key::Physical::Code(keyboard::key::Code::ArrowDown),
                    location: keyboard::Location::Standard,
                    modifiers: keyboard::Modifiers::empty(),
                    text: None,
                    repeat: false,
                }),
                bigger,
            );
            assert_eq!(
                app.placement.display_size("weather").unwrap()[1],
                old[1] + 8.
            );
            let desired = app.placement.display_size("weather").unwrap();
            let _ = app.update(Message::Page(Page::Weather));
            let _ = app.update(Message::Page(Page::WidgetLayout));
            assert_eq!(app.placement.display_size("weather"), Some(desired));
            let _ = app.update(Message::Page(Page::Overview));
            let save = native.bounds(app.settings_view(), "canvas-apply").center();
            assert!(save.y < size.height, "Save remains visible");
            pointer(&mut app, &mut native, save, None);
            pointer(&mut app, &mut native, save, Some(true));
            pointer(&mut app, &mut native, save, Some(false));
            assert_eq!(Config::load().widget_sizes["weather"], desired);
            assert!(!app.placement.dirty());
            Config::save_changes(json!({"theme":"amber"})).unwrap();
            let _ = app.update(Message::UndoPreferences);
            assert_eq!(Config::load().widget_sizes["weather"], old);
            assert_eq!(Config::load().theme, "amber");
            send_settings(&mut app, Message::Canvas(canvas::Message::Start));
            let _ = app.update(Message::Placement(placement::Message::Select(
                "music".into(),
            )));
            let _ = app.update(Message::Placement(placement::Message::Resize([0, 1], true)));
            assert!(app.canvas_dirty());
            send_settings(&mut app, Message::Canvas(canvas::Message::Cancel));
            assert_eq!(Config::load().widget_sizes["weather"], old);
            assert!(!app.placement.dirty());
            evidence.push(json!({"window":[size.width,size.height],"move_pixels":[after_move.x-original.x,after_move.y-original.y],"resize_pixels":[resized.width-after_move.width,resized.height-after_move.height],"saved_dimensions":desired,"saved_then_undone":true,"cancelled_tool_resize":true}));
        }
        if let Some(dir) = std::env::var_os("DESKTOP_CARDS_LAYOUT_EVIDENCE") {
            std::fs::write(
                PathBuf::from(dir).join("home-interaction.json"),
                serde_json::to_vec_pretty(&evidence).unwrap(),
            )
            .unwrap();
        }
    }

    #[test]
    fn search_opens_owned_controls_without_editing_a_hidden_layout() {
        if desktop_test_child(
            "search_opens_owned_controls_without_editing_a_hidden_layout",
            DesktopEnv {
                app_config: true,
                fixture: true,
                ..Default::default()
            },
        ) {
            return;
        }
        let root = isolated_root();
        let before = live_tree(&root);
        let preferences = raw_preferences();
        let (mut app, _) = Settings::init(Core::default(), Page::WidgetLayout);
        assert!(!app.placement_active());
        send_settings(&mut app, Message::Canvas(canvas::Message::Start));
        assert!(app.placement_active());
        let _ = app.update(Message::SearchSettings("gpu".into()));
        assert!(!app.placement_active());
        let _ = app.update(Message::Placement(placement::Message::Snap(false)));
        assert!(
            !app.placement.dirty(),
            "hidden layout ignores keyboard/edit actions"
        );
        let _ = app.update(Message::SearchFirst);
        assert_eq!(app.page, Page::Devbox);
        assert_eq!(app.search_selected.unwrap().anchor(), "devbox-gpu");
        assert!(app.search_query.is_empty());
        for (query, page, anchor) in [
            ("weather update", Page::Weather, "settings-advanced"),
            (
                "custom surface colors",
                Page::DesktopAppearance,
                "appearance-colors",
            ),
            (
                "desktop effects",
                Page::DesktopAppearance,
                "appearance-effects",
            ),
            ("dock timing", Page::DesktopDock, "dock-advanced"),
        ] {
            let _ = app.update(Message::SearchSettings(query.into()));
            let _ = app.update(Message::SearchFirst);
            assert_eq!(app.page, page);
            assert_eq!(app.search_selected.unwrap().anchor(), anchor);
            assert!(app.advanced_open(page));
        }
        assert!(app.system_appearance.advanced_colors && app.system_appearance.effects);
        assert!(app.desktop.advanced);
        assert!(app.pending_pages().is_empty());
        assert_eq!(raw_preferences(), preferences);
        assert_eq!(live_tree(&root), before);
    }

    #[test]
    fn menu_back_keeps_drafts_and_clears_search_before_leaving() {
        if desktop_test_child(
            "menu_back_keeps_drafts_and_clears_search_before_leaving",
            DesktopEnv {
                app_config: true,
                fixture: true,
                ..Default::default()
            },
        ) {
            return;
        }
        let root = isolated_root();
        let live = live_tree(&root);
        let preferences = raw_preferences();
        let (mut app, _) = Settings::init(Core::default(), Page::WidgetLayout);
        send_settings(&mut app, Message::Canvas(canvas::Message::Start));
        let _ = app.update(Message::Placement(placement::Message::Snap(false)));
        assert!(app.placement.dirty());
        let _ = app.update(Message::SearchSettings("gpu".into()));
        let _ = app.update(Message::Back);
        assert!(app.search_query.is_empty());
        assert_eq!(
            app.page,
            Page::Overview,
            "legacy layout links open Home; first Back dismisses search"
        );
        let _ = app.update(Message::Back);
        assert_eq!(
            app.page,
            Page::Widgets,
            "Back from Layout returns to the Widgets root"
        );
        assert!(app.placement.dirty(), "navigation never cancels the layout");
        let _ = app.update(Message::Page(Page::Weather));
        let _ = app.update(Message::Back);
        assert_eq!(app.page, Page::Widgets);
        // Workspaces is a sibling tab; Floating apps opens from Windows.
        let _ = app.update(Message::Page(Page::DesktopWindowRules));
        let _ = app.update(Message::Back);
        assert_eq!(app.page, Page::DesktopWindows);
        assert!(app.canvas_dirty());
        assert_eq!(
            raw_preferences(),
            preferences,
            "navigation preserves local edits without committing"
        );
        assert!(Config::load().placement.snap);
        send_settings(&mut app, Message::Page(Page::Overview));
        let id = app.canvas.as_ref().unwrap().id;
        send_settings(&mut app, Message::Canvas(canvas::Message::Apply(id)));
        assert!(!app.error, "{}", app.status);
        assert!(!Config::load().placement.snap);
        assert_eq!(live_tree(&root), live);
    }

    #[test]
    fn menu_preset_shortcut_preserves_exclusions_and_recovered_review() {
        if desktop_test_child(
            "menu_preset_shortcut_preserves_exclusions_and_recovered_review",
            DesktopEnv {
                app_config: true,
                fixture: true,
                ..Default::default()
            },
        ) {
            return;
        }
        use crate::desktop::profiles::Domain;
        use profiles::{Message as Edit, Selection};
        let root = isolated_root();
        let applied = fixture_files(&root.join("cosmic-root"));
        let preferences = raw_preferences();
        let (mut app, _) = Settings::init(Core::default(), Page::Overview);
        send_settings(&mut app, Message::TryPreset("glass"));
        assert_eq!(app.page, Page::DesktopProfiles);
        send_settings(
            &mut app,
            Message::Profiles(Edit::Include(Domain::Dock, false)),
        );
        assert!(!app.profiles.included(Domain::Dock));
        send_settings(&mut app, Message::Page(Page::Overview));
        assert_eq!(app.profiles.selection(), &Selection::Current);
        send_settings(&mut app, Message::TryPreset("glass"));
        assert!(
            app.profiles.included(Domain::Dock),
            "a new chooser starts with its declared categories"
        );
        assert_eq!(raw_preferences(), preferences);
        assert_eq!(fixture_files(&root.join("cosmic-root")), applied);
        app.on_app_exit();
        drop(app);
        let (reopened, _) = Settings::init(Core::default(), Page::DesktopProfiles);
        assert_eq!(reopened.profiles.selection(), &Selection::Current);
        assert!(reopened.recovery_sheet.rows.is_empty());
        assert_eq!(raw_preferences(), preferences);
    }

    #[test]
    fn menu_bar_shortcuts_select_the_hit_bar_without_losing_either_draft() {
        if desktop_test_child(
            "menu_bar_shortcuts_select_the_hit_bar_without_losing_either_draft",
            DesktopEnv {
                app_config: true,
                fixture: true,
                ..Default::default()
            },
        ) {
            return;
        }
        use crate::desktop::panels::{Anchor, PanelKind};
        let root = isolated_root();
        let applied = fixture_files(&root.join("cosmic-root"));
        let (mut app, _) = Settings::init(Core::default(), Page::Overview);
        for (kind, anchor) in [
            (PanelKind::Dock, Anchor::Left),
            (PanelKind::Panel, Anchor::Right),
        ] {
            let _ = app.update(Message::OpenBar(kind));
            assert_eq!(app.page, Page::DesktopDock);
            assert_eq!(app.desktop.current().unwrap().kind(), kind);
            let _ = app.update(Message::DesktopEditor(desktop::Message::Anchor(anchor)));
            let _ = app.update(Message::Page(Page::Overview));
        }
        let _ = app.update(Message::OpenBar(PanelKind::Dock));
        assert_eq!(
            app.desktop.current().unwrap().anchor().known(),
            Some(&Anchor::Left)
        );
        assert_eq!(
            app.desktop
                .draft(PanelKind::Panel)
                .unwrap()
                .anchor()
                .known(),
            Some(&Anchor::Right)
        );
        let _ = app.update(Message::Page(Page::Overview));
        assert_ne!(fixture_files(&root.join("cosmic-root")), applied);
    }

    #[test]
    fn menu_shortcuts_keep_undo_for_the_autosave_they_flush() {
        if desktop_test_child(
            "menu_shortcuts_keep_undo_for_the_autosave_they_flush",
            DesktopEnv {
                app_config: true,
                fixture: true,
                ..Default::default()
            },
        ) {
            return;
        }
        let root = isolated_root();
        std::fs::create_dir_all(root.join("cosmic-root/config/cosmic")).unwrap();
        let applied = fixture_files(&root.join("cosmic-root/config/cosmic"));
        let (mut app, _) = Settings::init(Core::default(), Page::Appearance);
        for shortcut in [
            Message::Back,
            Message::TryPreset("glass"),
            Message::OpenBar(crate::desktop::panels::PanelKind::Panel),
        ] {
            let _ = app.update(Message::Page(Page::Appearance));
            let original = app.config.appearance.opacity;
            let _ = app.update(Message::AppearanceValue(AppearanceControl::Opacity, 70));
            assert!(app.save_at.is_none());
            let _ = app.update(shortcut);
            assert_eq!(Config::load().appearance.opacity, 70);
            assert!(
                app.last_desktop_change.is_some(),
                "shortcut flush must be undoable"
            );
            let _ = app.update(Message::UndoPreferences);
            assert_eq!(Config::load().appearance.opacity, original);
            assert_eq!(raw_preferences()["future_field"], 42);
        }
        assert_eq!(
            fixture_files(&root.join("cosmic-root/config/cosmic")),
            applied
        );
    }

    #[test]
    fn menu_home_switch_has_a_clickable_undo_at_both_window_sizes() {
        if desktop_test_child(
            "menu_home_switch_has_a_clickable_undo_at_both_window_sizes",
            DesktopEnv {
                app_config: true,
                fixture: true,
                ..Default::default()
            },
        ) {
            return;
        }
        use cosmic::iced::advanced::{clipboard, renderer::Headless, widget::Operation};
        use cosmic::iced::runtime::user_interface::{Cache, UserInterface};
        use cosmic::iced::{Event, Rectangle, mouse};
        struct UndoTarget(Option<Rectangle>);
        impl Operation for UndoTarget {
            fn traverse(&mut self, operate: &mut dyn FnMut(&mut dyn Operation)) {
                operate(self);
            }
            fn focusable(
                &mut self,
                id: Option<&cosmic::widget::Id>,
                bounds: Rectangle,
                _: &mut dyn cosmic::iced::advanced::widget::operation::Focusable,
            ) {
                if id == Some(&cosmic::widget::Id::new("settings-undo")) {
                    self.0 = Some(bounds);
                }
            }
        }
        let _root = isolated_root();
        let mut renderer =
            cosmic::iced::futures::executor::block_on(<cosmic::Renderer as Headless>::new(
                cosmic::font::default(),
                cosmic::iced::Pixels(14.),
                Some("tiny-skia"),
            ))
            .expect("headless renderer");
        let (mut app, _) = Settings::init(Core::default(), Page::Overview);
        for size in [Size::new(1024., 720.), Size::new(820., 600.)] {
            let original = app.config.data_visible(Widget::Clock);
            let _ = app.update(Message::Resized(size));
            let _ = app.update(Message::Widget(Widget::Clock, !original));
            assert_eq!(Config::load().data_visible(Widget::Clock), !original);
            let mut ui =
                UserInterface::build(app.settings_view(), size, Cache::default(), &mut renderer);
            let mut target = UndoTarget(None);
            ui.operate(&renderer, &mut target);
            let bounds = target.0.expect("Home exposes the switch's Undo action");
            assert!(
                bounds.x >= 0.
                    && bounds.y >= 0.
                    && bounds.x + bounds.width <= size.width
                    && bounds.y + bounds.height <= size.height
            );
            let point = bounds.center();
            let mut messages = Vec::new();
            ui.update(
                &[
                    Event::Mouse(mouse::Event::CursorMoved { position: point }),
                    Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)),
                    Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)),
                ],
                mouse::Cursor::Available(point),
                &mut renderer,
                &mut clipboard::Null,
                &mut messages,
            );
            drop(ui);
            assert_eq!(messages.len(), 1);
            assert!(matches!(messages[0], Message::UndoDesktopShown(Some(_))));
            let _ = app.update(messages.pop().unwrap());
            assert_eq!(app.page, Page::Overview);
            assert_eq!(Config::load().data_visible(Widget::Clock), original);
            assert_eq!(raw_preferences()["future_field"], 42);
        }
    }

    #[test]
    fn menu_keyboard_focus_is_visible_and_enter_opens_the_focused_editor() {
        if desktop_test_child(
            "menu_keyboard_focus_is_visible_and_enter_opens_the_focused_editor",
            DesktopEnv {
                app_config: true,
                fixture: true,
                ..Default::default()
            },
        ) {
            return;
        }
        use cosmic::iced::advanced::{
            clipboard,
            renderer::{Headless, Style},
            widget::{
                Operation,
                operation::{Focusable, Outcome, focusable},
            },
        };
        use cosmic::iced::runtime::user_interface::{Cache, UserInterface};
        use cosmic::iced::{Event, Point, Rectangle, keyboard, mouse};

        #[derive(Default)]
        struct Focused(Option<(cosmic::widget::Id, Rectangle)>);
        impl Operation for Focused {
            fn traverse(&mut self, operate: &mut dyn FnMut(&mut dyn Operation)) {
                operate(self);
            }
            fn focusable(
                &mut self,
                id: Option<&cosmic::widget::Id>,
                bounds: Rectangle,
                state: &mut dyn Focusable,
            ) {
                if state.is_focused() {
                    self.0 = id.cloned().map(|id| (id, bounds));
                }
            }
        }
        let root = isolated_root();
        let applied = fixture_files(&root.join("cosmic-root"));
        let preferences = raw_preferences();
        let mut renderer =
            cosmic::iced::futures::executor::block_on(<cosmic::Renderer as Headless>::new(
                cosmic::font::default(),
                cosmic::iced::Pixels(14.),
                Some("tiny-skia"),
            ))
            .expect("headless renderer");
        let enter = Event::Keyboard(keyboard::Event::KeyPressed {
            key: keyboard::Key::Named(keyboard::key::Named::Enter),
            modified_key: keyboard::Key::Named(keyboard::key::Named::Enter),
            physical_key: keyboard::key::Physical::Code(keyboard::key::Code::Enter),
            location: keyboard::Location::Standard,
            modifiers: keyboard::Modifiers::empty(),
            text: None,
            repeat: false,
        });
        // A real pointer away from the content must not supply the missing cue.
        let away = mouse::Cursor::Available(Point::new(2., 2.));
        let mut failures = Vec::new();
        let mut evidence = Vec::new();
        for size in [Size::new(1024., 720.), Size::new(820., 600.)] {
            let pixels = Size::new(size.width as u32, size.height as u32);
            for (name, page, expected) in [
                ("gallery-hidden", Page::Widgets, Message::Page(Page::Glance)),
                ("gallery-shown", Page::Widgets, Message::Page(Page::Clock)),
                (
                    "home-editor",
                    Page::Overview,
                    Message::Canvas(canvas::Message::Start),
                ),
            ] {
                let (mut app, _) = Settings::init(Core::default(), page);
                let _ = app.update(Message::Resized(size));
                let theme = controls_theme(&app.config);
                let background = theme.cosmic().background(theme.transparent).base.into();
                let mut ui = UserInterface::build(
                    app.settings_view(),
                    size,
                    Cache::default(),
                    &mut renderer,
                );
                // Use the same focus-next operation as libcosmic's Tab handler.
                // Probe Enter's actual widget message to identify each target,
                // without applying any unrelated action along the way.
                let mut activated = None;
                for _ in 0..100 {
                    let mut operation: Box<dyn Operation> = Box::new(focusable::focus_next());
                    loop {
                        ui.operate(&renderer, operation.as_mut());
                        match operation.finish() {
                            Outcome::Chain(next) => operation = next,
                            _ => break,
                        }
                    }
                    let mut messages = Vec::new();
                    ui.update(
                        &[enter.clone()],
                        away,
                        &mut renderer,
                        &mut clipboard::Null,
                        &mut messages,
                    );
                    if messages.len() == 1
                        && match (&messages[0], &expected) {
                            (Message::Page(actual), Message::Page(expected)) => actual == expected,
                            (
                                Message::Canvas(canvas::Message::Start),
                                Message::Canvas(canvas::Message::Start),
                            ) => true,
                            (
                                Message::Placement(placement::Message::Select(actual)),
                                Message::Placement(placement::Message::Select(expected)),
                            ) => actual == expected,
                            (Message::OpenBar(actual), Message::OpenBar(expected)) => {
                                actual == expected
                            }
                            _ => false,
                        }
                    {
                        let mut focused = Focused::default();
                        ui.operate(&renderer, &mut focused);
                        let (id, bounds) = focused.0.expect("focused button");
                        // Desktop sidebar links can share the same destination.
                        if bounds.x >= 184. {
                            activated = Some((messages.pop().unwrap(), id, bounds));
                            break;
                        }
                    }
                }
                let (message, id, bounds) = activated
                    .unwrap_or_else(|| panic!("{name} at {size:?}: editor reachable by keyboard"));
                assert!(
                    bounds.x >= 0.
                        && bounds.y >= 0.
                        && bounds.x + bounds.width <= size.width
                        && bounds.y + bounds.height <= size.height,
                    "{name} {size:?}: {bounds:?}"
                );
                for (state, cursor, event) in [
                    ("active", away, Event::Mouse(mouse::Event::CursorLeft)),
                    (
                        "hovered",
                        mouse::Cursor::Available(bounds.center()),
                        Event::Mouse(mouse::Event::CursorLeft),
                    ),
                    (
                        "pressed",
                        mouse::Cursor::Available(bounds.center()),
                        enter.clone(),
                    ),
                ] {
                    ui.update(
                        &[event],
                        cursor,
                        &mut renderer,
                        &mut clipboard::Null,
                        &mut Vec::new(),
                    );
                    // Settle the real tooltip overlay before comparing frames.
                    ui.operate(&renderer, &mut Focused::default());
                    ui.draw(&mut renderer, &theme, &Style::default(), cursor);
                    let focused = renderer.screenshot(pixels, 1., background);
                    ui.operate(&renderer, &mut focusable::unfocus());
                    ui.draw(&mut renderer, &theme, &Style::default(), cursor);
                    let unfocused = renderer.screenshot(pixels, 1., background);
                    ui.operate(&renderer, &mut focusable::focus(id.clone()));
                    if state == "active" {
                        if let Some(dir) = std::env::var_os("DESKTOP_CARDS_FOCUS_EVIDENCE") {
                            let dir = PathBuf::from(dir);
                            std::fs::create_dir_all(&dir).unwrap();
                            for (suffix, pixels) in
                                [("focused", &focused), ("unfocused", &unfocused)]
                            {
                                image::save_buffer(
                                    dir.join(format!(
                                        "{name}-{}x{}-{suffix}.png",
                                        size.width, size.height
                                    )),
                                    pixels,
                                    size.width as u32,
                                    size.height as u32,
                                    image::ColorType::Rgba8,
                                )
                                .unwrap();
                            }
                        }
                    }
                    let changed = focused
                        .chunks_exact(4)
                        .zip(unfocused.chunks_exact(4))
                        .enumerate()
                        .filter(|(index, (a, b))| {
                            let x = (index % pixels.width as usize) as f32;
                            let y = (index / pixels.width as usize) as f32;
                            // Count visible changes along this button's edge,
                            // not text, another control or a tooltip elsewhere.
                            x >= bounds.x - 5.
                                && x <= bounds.x + bounds.width + 5.
                                && y >= bounds.y - 5.
                                && y <= bounds.y + bounds.height + 5.
                                && (x < bounds.x + 6.
                                    || x > bounds.x + bounds.width - 6.
                                    || y < bounds.y + 6.
                                    || y > bounds.y + bounds.height - 6.)
                                && a.iter().zip(b.iter()).any(|(a, b)| a.abs_diff(*b) > 24)
                        })
                        .count();
                    evidence.push(json!({
                        "control": name, "size": [size.width, size.height], "state": state,
                        "bounds": [bounds.x, bounds.y, bounds.width, bounds.height],
                        "changed_pixels": changed,
                    }));
                    if changed <= 50 {
                        failures.push(format!(
                            "{name} {size:?} {state}: focus must visibly change the rendered button ({changed} pixels)"
                        ));
                    }
                }
                drop(ui);
                let _ = app.update(message);
                match expected {
                    Message::Placement(placement::Message::Select(ref key)) => {
                        assert_eq!(app.page, Page::Overview);
                        assert_eq!(
                            app.placement.selected().map(Widget::key),
                            Some(key.as_str())
                        );
                    }
                    Message::Canvas(canvas::Message::Start) => {
                        assert_eq!(app.page, Page::Overview);
                        assert!(app.canvas.is_some());
                    }
                    Message::Page(page) => assert_eq!(app.page, page),
                    Message::OpenBar(kind) => {
                        assert_eq!(app.page, Page::DesktopDock);
                        assert_eq!(app.desktop.selected, kind);
                    }
                    _ => unreachable!(),
                }
            }
        }
        assert_eq!(raw_preferences(), preferences);
        assert_eq!(fixture_files(&root.join("cosmic-root")), applied);
        if let Some(dir) = std::env::var_os("DESKTOP_CARDS_FOCUS_EVIDENCE") {
            std::fs::write(
                PathBuf::from(dir).join("results.json"),
                serde_json::to_vec_pretty(&evidence).unwrap(),
            )
            .unwrap();
        }
        assert!(failures.is_empty(), "{}", failures.join("\n"));
    }

    #[test]
    fn profile_changes_wait_for_overlapping_drafts_and_keep_every_other_draft() {
        if desktop_test_child(
            "profile_changes_wait_for_overlapping_drafts_and_keep_every_other_draft",
            DesktopEnv {
                app_config: true,
                fixture: true,
                ..Default::default()
            },
        ) {
            return;
        }
        use profiles::{Message as Look, Selection};
        let root = isolated_root();
        let live = live_tree(&root);
        let (mut app, _) = Settings::init(Core::default(), Page::Appearance);
        let _ = app.update(Message::AppearanceValue(AppearanceControl::Opacity, 95));
        let generation = app.edit_generation;
        let _ = app.update(Message::SaveAppearance);
        let _ = app.update(Message::Page(Page::DesktopDock));
        let _ = app.update(Message::DesktopEditor(desktop::Message::Anchor(
            crate::desktop::panels::Anchor::Left,
        )));
        assert!(!app.desktop.dirty());
        let _ = app.update(Message::Page(Page::DesktopProfiles));
        let _ = app.update(Message::Profiles(Look::Select(Selection::Preset("focus"))));
        let _ = app.update(Message::Profiles(Look::Include(
            crate::desktop::profiles::Domain::Dock,
            false,
        )));
        assert!(app.profile_waits().is_empty());
        let _ = app.update(Message::Profiles(Look::Apply));
        assert!(app.profiles.can_undo(), "{:?}", app.profiles.notice);
        assert_eq!(
            Config::load().appearance.opacity,
            crate::desktop_theme::Family::Bare.geometry().0
        );
        let _ = app.update(Message::CommitEdits(generation));
        assert_eq!(
            Config::load().appearance.opacity,
            crate::desktop_theme::Family::Bare.geometry().0
        );
        let _ = app.update(Message::Page(Page::Weather));
        let _ = app.update(Message::UndoDesktop);
        assert!(!app.error, "{}", app.status);
        assert_eq!(Config::load().appearance.opacity, 95);
        assert_eq!(live_tree(&root), live);
    }

    #[test]
    fn profile_changes_ignore_other_pages_errors_but_not_a_failed_autosave() {
        if desktop_test_child(
            "profile_changes_ignore_other_pages_errors_but_not_a_failed_autosave",
            DesktopEnv {
                app_config: true,
                fixture: true,
                ..Default::default()
            },
        ) {
            return;
        }
        use crate::desktop::profiles::Domain;
        use profiles::{Message as Look, Selection};
        let root = isolated_root();
        let fixture = root.join("cosmic-root");
        let live = live_tree(&root);
        let (mut app, _) = Settings::init(Core::default(), Page::Projects);
        // A folder that isn't there leaves its error showing.
        let _ = app.update(Message::FolderInput(
            root.join("missing").display().to_string(),
        ));
        let _ = app.update(Message::AddFolder);
        assert!(app.error);
        let _ = app.update(Message::Page(Page::DesktopProfiles));
        assert!(app.error, "the Projects error is still showing");
        let _ = app.update(Message::Profiles(Look::Select(Selection::Preset(
            "evening",
        ))));
        let _ = app.update(Message::Profiles(Look::Apply));
        assert!(app.profiles.can_undo(), "{:?}", app.profiles.notice);
        assert_eq!(Config::load().theme, "lavender");
        // Nor does a failed service query hold back Undo.
        let _ = app.update(Message::Service(Err("Service status unavailable".into())));
        assert!(app.error);
        let _ = app.update(Message::Profiles(Look::Undo));
        assert!(
            !matches!(app.profiles.notice, Some(desktop::Notice::Error(_))),
            "{:?}",
            app.profiles.notice
        );
        assert_eq!(Config::load().theme, "teal");
        // A pending card edit that can't be saved stops the profile, and
        // nothing is written for it.
        let _ = app.update(Message::Profiles(Look::Select(Selection::Current)));
        let _ = app.update(Message::Profiles(Look::Select(Selection::Preset(
            "evening",
        ))));
        let _ = app.update(Message::Profiles(Look::Include(Domain::WidgetStyle, false)));
        assert!(!app.profiles.apply_domains().is_empty());
        let _ = app.update(Message::AppearanceValue(AppearanceControl::Opacity, 90));
        std::fs::write(Config::path().unwrap(), "{").unwrap();
        let files = fixture_files(&fixture);
        let _ = app.update(Message::Profiles(Look::Apply));
        assert!(app.error);
        assert!(
            matches!(app.profiles.notice, Some(desktop::Notice::Error(_))),
            "{:?}",
            app.profiles.notice
        );
        assert_eq!(fixture_files(&fixture), files);
        assert_eq!(std::fs::read(Config::path().unwrap()).unwrap(), b"{");
        assert_eq!(live_tree(&root), live, "the fake live desktop is untouched");
    }

    #[test]
    fn a_held_back_profile_undo_names_the_page_whatever_is_selected() {
        if desktop_test_child(
            "a_held_back_profile_undo_names_the_page_whatever_is_selected",
            DesktopEnv {
                app_config: true,
                fixture: true,
                ..Default::default()
            },
        ) {
            return;
        }
        use profiles::{Message as Look, Selection};
        let (mut app, _) = Settings::init(Core::default(), Page::DesktopProfiles);
        let _ = app.update(Message::Profiles(Look::Select(Selection::Preset(
            "evening",
        ))));
        let _ = app.update(Message::Profiles(Look::Apply));
        let _ = app.update(Message::Page(Page::DesktopDock));
        let _ = app.update(Message::DesktopEditor(desktop::Message::Anchor(
            crate::desktop::panels::Anchor::Left,
        )));
        assert!(!app.desktop.dirty());
        assert_eq!(app.last_desktop_change.as_ref().unwrap().1, "Dock position");
        let _ = app.update(Message::Page(Page::DesktopProfiles));
        let _ = app.update(Message::Profiles(Look::Select(Selection::Preset("focus"))));
        assert!(app.profile_waits().is_empty());
        let _ = app.update(Message::UndoDesktop);
        assert!(!app.error, "{}", app.status);
        assert_eq!(Config::load().theme, "lavender");
        let _ = app.update(Message::UndoDesktop);
        assert!(!app.error, "{}", app.status);
        assert_eq!(Config::load().theme, "teal");
    }

    #[test]
    fn profile_capture_commands_write_only_inside_the_fixture() {
        if desktop_test_child(
            "profile_capture_commands_write_only_inside_the_fixture",
            DesktopEnv {
                app_config: true,
                capture: true,
                fixture: true,
            },
        ) {
            return;
        }
        let root = isolated_root();
        let fixture = root.join("cosmic-root");
        let live = live_tree(&root);
        let (mut app, _) = Settings::init(Core::default(), Page::Desktop);
        send_command(
            &mut app,
            json!({"page":"desktop-profiles","profiles":[{"select":"evening"},{"include":["dock",false]},"apply"]}),
        );
        assert_eq!(app.page, Page::DesktopProfiles);
        assert!(app.profiles.can_undo(), "{:?}", app.profiles.notice);
        assert_eq!(app.config.theme, "lavender");
        assert!(
            app.profiles.library().unwrap().is_empty(),
            "applying stores no profile"
        );
        // Imports read only files inside the fixture root.
        let record = crate::desktop::profiles::export(
            &crate::desktop::profiles::preset("workbench")
                .unwrap()
                .profile,
        )
        .unwrap()
        .bytes;
        std::fs::write(root.join("escape.json"), &record).unwrap();
        std::fs::write(fixture.join("inside.json"), &record).unwrap();
        send_command(&mut app, json!({"profiles":[{"import":"../escape.json"}]}));
        assert!(app.profiles.library().unwrap().is_empty());
        send_command(&mut app, json!({"profiles":[{"import":"inside.json"}]}));
        assert_eq!(app.profiles.library().unwrap().len(), 1);
        assert!(matches!(
            app.profiles.selection(),
            profiles::Selection::Saved(_)
        ));
        assert_eq!(app.config.theme, "lavender", "importing applies nothing");
        send_command(&mut app, json!({"profiles":["undo"]}));
        assert_eq!(app.config.theme, "teal");
        assert_eq!(live_tree(&root), live, "the fake live desktop is untouched");
    }

    fn send_command(app: &mut Settings, command: serde_json::Value) {
        std::fs::write(
            settings_command_path().unwrap(),
            serde_json::to_vec(&command).unwrap(),
        )
        .unwrap();
        send_settings(app, Message::Tick);
    }

    #[test]
    fn dock_panel_drafts_survive_navigation_and_apply_only_to_the_fixture() {
        if desktop_test_child(
            "dock_panel_drafts_survive_navigation_and_apply_only_to_the_fixture",
            DesktopEnv {
                app_config: true,
                fixture: true,
                ..Default::default()
            },
        ) {
            return;
        }
        use crate::desktop::panels::{Anchor, PanelKind};
        assert_eq!(Page::parse("desktop"), Page::DesktopDock);
        let root = isolated_root();
        let live = live_tree(&root);
        let fixture = root.join("cosmic-root");
        let anchor = fixture.join("config/cosmic/com.system76.CosmicPanel.Dock/v1/anchor");
        crate::test_env::native_worker(&fixture, true);
        let (mut app, initial) = Settings::init(Core::default(), Page::DesktopDock);
        settle_settings(&mut app, initial);
        assert!(app.native_worker_needed());
        assert!(!anchor.exists());
        send_settings(
            &mut app,
            Message::DesktopEditor(desktop::Message::Opacity(73)),
        );
        assert!(
            app.desktop.dirty(),
            "an in-flight slider has no completed value"
        );
        send_settings(&mut app, Message::DesktopEditor(desktop::Message::Apply));
        assert!(!app.desktop.dirty());
        send_settings(
            &mut app,
            Message::DesktopEditor(desktop::Message::Anchor(Anchor::Left)),
        );
        assert_eq!(std::fs::read_to_string(&anchor).unwrap(), "Left");
        send_settings(&mut app, Message::Page(Page::Glance));
        send_settings(&mut app, Message::Page(Page::DesktopDock));
        for kind in PanelKind::ALL {
            send_settings(
                &mut app,
                Message::DesktopEditor(desktop::Message::Select(kind)),
            );
            send_settings(
                &mut app,
                Message::DesktopEditor(desktop::Message::Advanced(true)),
            );
            let mut tree = Tree::new(app.settings_view().as_widget());
            tree.diff(app.settings_view().as_widget_mut());
        }
        drop(app);
        let (mut app, initial) = Settings::init(Core::default(), Page::Weather);
        settle_settings(&mut app, initial);
        send_settings(&mut app, Message::UndoDesktop);
        assert!(!app.error, "{}", app.status);
        assert!(!anchor.exists());
        assert_eq!(live_tree(&root), live);
    }

    #[test]
    fn dock_panel_preview_without_fixture_reads_isolated_live_tree_but_cannot_apply() {
        if desktop_test_child(
            "dock_panel_preview_without_fixture_reads_isolated_live_tree_but_cannot_apply",
            DesktopEnv {
                app_config: true,
                ..Default::default()
            },
        ) {
            return;
        }
        use crate::desktop::panels::{Anchor, PanelSize, Setting};
        let root = isolated_root();
        let live = live_tree(&root);
        let mut app = open_isolated_editor(&root);
        assert!(app.desktop.opened());
        assert!(!app.desktop.writable());
        // Both layers of the fake live tree resolve.
        let dock = app.desktop.current().expect("the seeded Dock resolves");
        assert_eq!(dock.anchor(), Setting::Known(Anchor::Right));
        assert_eq!(dock.size(), Setting::Known(PanelSize::L));
        send_settings(
            &mut app,
            Message::DesktopEditor(desktop::Message::Opacity(37)),
        );
        assert!(!app.desktop.can_apply());
        send_settings(&mut app, Message::DesktopEditor(desktop::Message::Apply));
        assert!(app.desktop.dirty(), "a refused Apply keeps the draft");
        assert!(matches!(
            &app.desktop.notice,
            Some(desktop::Notice::Error(text)) if text.starts_with("Preview mode")
        ));
        send_settings(&mut app, Message::DesktopEditor(desktop::Message::Recover));
        send_settings(
            &mut app,
            Message::DesktopEditor(desktop::Message::KeepRecovery),
        );
        let mut tree = Tree::new(app.settings_view().as_widget());
        tree.diff(app.settings_view().as_widget_mut());
        assert!(!app.native_busy && app.native_deferred.is_empty());
        // Byte-identical, and no lock or journal appeared.
        assert_eq!(live_tree(&root), live);
        assert!(!root.join("home/.local/share/cosmic-desktop-cards").exists());
    }

    /// Builds and lays out the real settings view after each message, the way
    /// the runtime does. libcosmic keys widget state by custom id, so named
    /// applet controls must survive rows changing shape during drags and moves.
    #[test]
    fn applet_rows_lay_out_through_drags_and_keyboard_moves() {
        if desktop_test_child(
            "applet_rows_lay_out_through_drags_and_keyboard_moves",
            DesktopEnv {
                app_config: true,
                fixture: true,
                ..Default::default()
            },
        ) {
            return;
        }
        use crate::desktop::panels::{Region, Slot};
        use cosmic::iced::advanced::renderer::Headless;
        use cosmic::iced::runtime::user_interface::{Cache, UserInterface};
        use desktop::Message as Edit;
        let root = isolated_root();
        let wings = root.join("cosmic-root/config/cosmic/com.system76.CosmicPanel.Dock/v1");
        std::fs::create_dir_all(&wings).unwrap();
        std::fs::write(
            wings.join("plugins_wings"),
            r#"Some((["s", "t"], ["s", "u"]))"#,
        )
        .unwrap();
        // Installed applets: known rows carry no "Not installed" pill, so a
        // drag adds one in front of their controls.
        let applications = root.join("cosmic-root/applications");
        std::fs::create_dir_all(&applications).unwrap();
        for id in ["s", "t", "u"] {
            std::fs::write(
                applications.join(format!("{id}.desktop")),
                format!("[Desktop Entry]\nName=Applet {id}\nX-CosmicApplet=true\n"),
            )
            .unwrap();
        }
        let mut app = open_isolated_editor(&root);
        assert!(app.desktop.catalog.known("s"));
        let mut renderer =
            cosmic::iced::futures::executor::block_on(<cosmic::Renderer as Headless>::new(
                cosmic::font::default(),
                cosmic::iced::Pixels(14.),
                Some("tiny-skia"),
            ))
            .expect("headless renderer");
        let size = Size::new(820., 600.);
        let ui = UserInterface::build(app.settings_view(), size, Cache::default(), &mut renderer);
        let mut cache = ui.into_cache();
        let slot = |region, index| Slot { region, index };
        for message in [
            Edit::DragStart(slot(Region::Start, 1)),
            Edit::DragOver(slot(Region::End, 0)),
            Edit::DragOver(slot(Region::Center, 0)),
            Edit::DragCancel,
            Edit::Step(slot(Region::End, 0), -1),
            Edit::Step(slot(Region::Center, 1), -1),
            Edit::DragStart(slot(Region::Start, 0)),
            Edit::DragOver(slot(Region::End, 1)),
            Edit::Drop,
            Edit::Remove(slot(Region::End, 0)),
            Edit::Picker(Some(Region::Center)),
        ] {
            let _ = app.update(Message::DesktopEditor(message));
            cache =
                UserInterface::build(app.settings_view(), size, cache, &mut renderer).into_cache();
        }
        // The pinned header changes the page structure around the named
        // scrollable; switching pages and bars must lay out too.
        for message in [
            Message::Page(Page::Glance),
            Message::Page(Page::DesktopDock),
            Message::DesktopEditor(Edit::Select(crate::desktop::panels::PanelKind::Panel)),
            Message::Page(Page::Widgets),
            Message::Page(Page::DesktopDock),
            Message::DesktopEditor(Edit::Select(crate::desktop::panels::PanelKind::Dock)),
        ] {
            let _ = app.update(message);
            cache =
                UserInterface::build(app.settings_view(), size, cache, &mut renderer).into_cache();
        }
        assert!(!app.desktop.dirty());
        let _ = app.update(Message::DesktopEditor(Edit::Select(
            crate::desktop::panels::PanelKind::Panel,
        )));
        for size in [Size::new(820., 600.), Size::new(656., 480.)] {
            for message in [
                Edit::Add(Region::Start, "s".into()),
                Edit::Add(Region::Start, "s".into()),
                Edit::Add(Region::End, "unknown".into()),
                Edit::SelectApplet(Some(slot(Region::Start, 1))),
                Edit::MoveTo(slot(Region::Start, 1), Region::Center),
                Edit::Step(slot(Region::Center, 0), 1),
                Edit::DragStart(slot(Region::End, 0)),
                Edit::DragOver(slot(Region::Start, 0)),
                Edit::Drop,
                Edit::Picker(Some(Region::Center)),
                Edit::AppletGroup(crate::desktop::applets::AppletGroup::Media),
                Edit::PickerRegion(Region::End),
                Edit::Picker(None),
                Edit::Starter(true),
                Edit::Starter(false),
                Edit::PanelStyle(desktop::PanelStyle::Islands),
                Edit::PanelStyle(desktop::PanelStyle::FullWidth),
            ] {
                let _ = app.update(Message::DesktopEditor(message));
                cache = UserInterface::build(app.settings_view(), size, cache, &mut renderer)
                    .into_cache();
            }
        }
    }

    #[test]
    fn installed_applets_are_discovered_searched_and_added_without_writing() {
        if desktop_test_child(
            "installed_applets_are_discovered_searched_and_added_without_writing",
            DesktopEnv {
                app_config: true,
                fixture: true,
                ..Default::default()
            },
        ) {
            return;
        }
        use crate::desktop::panels::{PanelKind, Region};
        use cosmic::iced::advanced::renderer::Headless;
        use cosmic::iced::runtime::user_interface::{Cache, UserInterface};
        use desktop::Message as Edit;
        let root = isolated_root();
        let dock = root.join("cosmic-root/config/cosmic/com.system76.CosmicPanel.Dock/v1");
        std::fs::create_dir_all(&dock).unwrap();
        std::fs::write(dock.join("plugins_center"), r#"Some(["clock"])"#).unwrap();
        let panel = root.join("cosmic-root/config/cosmic/com.system76.CosmicPanel.Panel/v1");
        std::fs::create_dir_all(&panel).unwrap();
        std::fs::write(
            panel.join("plugins_wings"),
            r#"Some((["clock"], ["sound"]))"#,
        )
        .unwrap();
        let applications = root.join("cosmic-root/applications");
        std::fs::create_dir_all(&applications).unwrap();
        let applet = |id: &str, name: &str, comment: &str| {
            std::fs::write(
                applications.join(format!("{id}.desktop")),
                format!("[Desktop Entry]\nName={name}\nComment={comment}\nX-CosmicApplet=true\n"),
            )
            .unwrap();
        };
        applet("clock", "Time", "Date and time");
        applet("sound", "Sound", "Volume and devices");
        let mut app = open_isolated_editor(&root);
        let before = crate::desktop::store::tests::tree(&root.join("cosmic-root/config"));
        // Where each applet is used, across both bars.
        assert_eq!(
            app.desktop.placements("clock"),
            [
                (PanelKind::Dock, Region::Center, true),
                (PanelKind::Panel, Region::Start, true)
            ]
        );
        // An applet installed while Settings is open appears on the next visit.
        applet("battery", "Battery", "Power and charging");
        assert!(!app.desktop.catalog.known("battery"));
        let _ = app.update(Message::Page(Page::Widgets));
        let _ = app.update(Message::Page(Page::DesktopDock));
        assert!(app.desktop.catalog.known("battery"));
        let _ = app.update(Message::DesktopEditor(Edit::Overview(true)));
        let _ = app.update(Message::DesktopEditor(Edit::Search("POWER".into())));
        let shown: Vec<_> = app
            .desktop
            .catalog
            .sorted()
            .into_iter()
            .filter(|info| app.desktop.matches(info))
            .map(|info| info.id.clone())
            .collect();
        assert_eq!(shown, ["battery"]);
        assert_eq!(
            crate::desktop::store::tests::tree(&root.join("cosmic-root/config")),
            before,
            "discovery and search are read-only"
        );
        // Add is a completed action in the Dock's middle.
        let _ = app.update(Message::DesktopEditor(Edit::AddToBar("battery".into())));
        assert_eq!(
            app.desktop.placements("battery"),
            [(PanelKind::Dock, Region::Center, true)]
        );
        assert!(!app.desktop.dirty());
        assert_ne!(
            crate::desktop::store::tests::tree(&root.join("cosmic-root/config")),
            before,
            "adding an applet saves directly"
        );
        // The list, its search and the picker lay out in a small window.
        let mut renderer =
            cosmic::iced::futures::executor::block_on(<cosmic::Renderer as Headless>::new(
                cosmic::font::default(),
                cosmic::iced::Pixels(14.),
                Some("tiny-skia"),
            ))
            .expect("headless renderer");
        let size = Size::new(820., 600.);
        let mut cache = Cache::default();
        for message in [
            Edit::Search(String::new()),
            Edit::Select(PanelKind::Panel),
            Edit::Picker(Some(Region::End)),
            Edit::Overview(false),
        ] {
            let _ = app.update(Message::DesktopEditor(message));
            cache =
                UserInterface::build(app.settings_view(), size, cache, &mut renderer).into_cache();
        }
        assert_eq!(app.desktop.picker, Some(Region::End));
    }

    #[test]
    fn tools_pages_save_options_and_the_note_explicitly_and_lay_out_small() {
        if devbox_test_child("tools_pages_save_options_and_the_note_explicitly_and_lay_out_small") {
            return;
        }
        use cosmic::iced::advanced::renderer::Headless;
        use cosmic::iced::runtime::user_interface::{Cache, UserInterface};
        use cosmic::widget::text_editor;
        use tools::Message as Edit;
        let (mut app, _) = Settings::init(Core::default(), Page::parse("focus"));
        assert_eq!(app.page, Page::Focus);
        assert_eq!(app.page.nav(), Page::Widgets, "all tools belong to Widgets");
        // Options save into the tool's own section; everything else stays.
        let _ = app.update(Message::Widget(Widget::Focus, true));
        let _ = app.update(Message::Tools(Edit::Option(
            Widget::Focus,
            "focus_minutes",
            json!(50),
        )));
        let _ = app.update(Message::Tools(Edit::Option(
            Widget::Notes,
            "size",
            json!("large"),
        )));
        let _ = app.update(Message::Tools(Edit::Player(Some("spotify".into()))));
        let raw = raw_preferences();
        assert_eq!(raw["focus"], json!({"enabled": true, "focus_minutes": 50}));
        assert_eq!(raw["notes"]["size"], "large");
        assert_eq!(raw["music"]["player"], "spotify");
        assert_eq!(raw["future_field"], 42);
        assert!(app.config.widget_visible(Widget::Focus));
        // An invalid value is refused and reported, never saved.
        let _ = app.update(Message::Tools(Edit::Option(
            Widget::Focus,
            "focus_minutes",
            json!(0),
        )));
        assert!(app.error);
        assert_eq!(raw_preferences()["focus"]["focus_minutes"], 50);
        // The note saves only on Save.
        let _ = app.update(Message::Page(Page::Notes));
        for c in "[ ] Call Sam".chars() {
            let _ = app.update(Message::Tools(Edit::Note(text_editor::Action::Edit(
                text_editor::Edit::Insert(c),
            ))));
        }
        assert!(app.tools.dirty());
        assert_eq!(crate::tools::notes::load().unwrap().text, "");
        send_settings(&mut app, Message::Tools(Edit::NoteSave));
        assert!(!app.tools.dirty());
        assert_eq!(crate::tools::notes::load().unwrap().text, "[ ] Call Sam");
        assert!(
            !std::fs::read_to_string(Config::path().unwrap())
                .unwrap()
                .contains("Sam")
        );
        // The desktop ticks the item while this page has unsaved edits: the
        // edit is kept, Save explains, and the user chooses.
        let _ = app.update(Message::Tools(Edit::Note(text_editor::Action::Edit(
            text_editor::Edit::Insert('!'),
        ))));
        crate::tools::notes::save("[ ] Call Sam", "[x] Call Sam").unwrap();
        let _ = app.update(Message::Tick);
        assert!(app.tools.dirty() && app.tools.conflict.is_some());
        send_settings(&mut app, Message::Tools(Edit::NoteSave));
        assert_eq!(crate::tools::notes::load().unwrap().text, "[x] Call Sam");
        send_settings(&mut app, Message::Tools(Edit::NoteKeepMine));
        assert_eq!(crate::tools::notes::load().unwrap().text, "[ ] Call Sam!");
        assert!(app.tools.conflict.is_none() && !app.tools.dirty());
        // Saved explicit dimensions also reflow in each tool's detail preview.
        app.config = Config::save_changes(json!({"widget_sizes":{
            "music":[480,300],"notes":[480,320],"focus":[440,320]
        }}))
        .unwrap();
        // Each tab and the Widgets page lay out in the smallest window.
        let mut renderer =
            cosmic::iced::futures::executor::block_on(<cosmic::Renderer as Headless>::new(
                cosmic::font::default(),
                cosmic::iced::Pixels(14.),
                Some("tiny-skia"),
            ))
            .expect("headless renderer");
        let size = Size::new(820., 600.);
        let mut cache = Cache::default();
        for page in [Page::Music, Page::Notes, Page::Focus, Page::Widgets] {
            let _ = app.update(Message::Page(page));
            cache =
                UserInterface::build(app.settings_view(), size, cache, &mut renderer).into_cache();
        }
        let _ = app.update(Message::Tools(Edit::Players(Err("no bus".into()))));
        let _ = app.update(Message::Page(Page::Music));
        let _ = UserInterface::build(app.settings_view(), size, cache, &mut renderer);
    }

    #[test]
    fn notes_saves_are_async_bounded_and_keep_later_edits() {
        if crate::displays::test_support::isolated(
            "settings::tests::notes_saves_are_async_bounded_and_keep_later_edits",
        ) {
            return;
        }
        use crate::tools::notes;
        use cosmic::widget::text_editor;
        use tools::Message as Edit;
        notes::save("", "original").unwrap();
        let (mut app, _) = Settings::init(Core::default(), Page::Notes);
        app.tools.content = text_editor::Content::with_text("submitted");
        let (acquired, ready) = std::sync::mpsc::channel();
        let (release, released) = std::sync::mpsc::channel();
        let holder = std::thread::spawn(move || {
            let _lock = crate::config::lock_config(&notes::path().unwrap()).unwrap();
            acquired.send(()).unwrap();
            // Bound a regression's failure instead of hanging the whole suite.
            let _ = released.recv_timeout(std::time::Duration::from_secs(5));
        });
        ready
            .recv_timeout(std::time::Duration::from_secs(2))
            .unwrap();
        let before = std::time::Instant::now();
        let task = app.update(Message::Tools(Edit::NoteSave));
        assert!(before.elapsed() < std::time::Duration::from_secs(1));
        assert!(app.tools.saving());
        send_settings(&mut app, Message::Tools(Edit::NoteSave));
        assert!(
            app.tools.saving(),
            "a duplicate must not own another save completion"
        );
        settle_settings(&mut app, task);
        assert!(!app.tools.saving() && app.tools.problem.is_some());
        assert_eq!(notes::load().unwrap().text, "original");
        release.send(()).unwrap();
        holder.join().unwrap();

        let task = app.update(Message::Tools(Edit::NoteSave));
        let _ = app.update(Message::Tools(Edit::Note(text_editor::Action::Edit(
            text_editor::Edit::Insert('!'),
        ))));
        let later = app.tools.content.text();
        settle_settings(&mut app, task);
        assert_eq!(notes::load().unwrap().text, "submitted");
        assert_eq!(app.tools.base, "submitted");
        assert_eq!(app.tools.content.text(), later);
        assert!(app.tools.dirty() && app.tools.conflict.is_none());

        let task = app.update(Message::Tools(Edit::NoteSave));
        notes::save("submitted", "external").unwrap();
        settle_settings(&mut app, task);
        assert!(app.tools.conflict.is_some() && app.tools.dirty());
        assert_eq!(notes::load().unwrap().text, "external");
        send_settings(&mut app, Message::Tools(Edit::NoteKeepMine));
        assert!(app.tools.conflict.is_none() && !app.tools.dirty());

        let current = notes::load().unwrap().text;
        notes::save(&current, "[ ] task").unwrap();
        app.tools.refresh();
        let task = app.update(Message::Tools(Edit::Card(
            crate::tools::cards::Action::NoteToggle(0),
        )));
        assert!(app.tools.saving());
        send_settings(
            &mut app,
            Message::Tools(Edit::Card(crate::tools::cards::Action::NoteToggle(0))),
        );
        assert!(app.tools.saving());
        assert!(matches!(
            app.on_app_exit(),
            Some(Message::Window(WindowControl::Close))
        ));
        let _ = app.update(Message::Window(WindowControl::Close));
        assert!(app.tools.close_after_save);
        settle_settings(&mut app, task);
        assert_eq!(notes::load().unwrap().text, "[x] task");
        assert!(!app.tools.saving() && !app.tools.close_after_save);
    }

    #[test]
    fn an_unsaved_note_outlives_the_window_and_never_overwrites_a_newer_one() {
        if devbox_test_child("an_unsaved_note_outlives_the_window_and_never_overwrites_a_newer_one")
        {
            return;
        }
        use crate::tools::notes;
        use cosmic::widget::text_editor;
        use tools::Message as Edit;
        let type_text = |app: &mut Settings, text: &str| {
            for c in text.chars() {
                let _ = app.update(Message::Tools(Edit::Note(text_editor::Action::Edit(
                    text_editor::Edit::Insert(c),
                ))));
            }
        };
        notes::save("", "Buy milk").unwrap();
        // Typing, then closing the window: the note itself is untouched (only
        // Save writes it), and the edits are kept privately beside it.
        let (mut app, _) = Settings::init(Core::default(), Page::parse("notes"));
        assert!(!app.tools.restored);
        let _ = app.update(Message::Tools(Edit::Note(text_editor::Action::Move(
            text_editor::Motion::DocumentEnd,
        ))));
        type_text(&mut app, " and tea");
        let _ = app.update(Message::FlushEdits);
        app.on_app_exit();
        drop(app);
        assert_eq!(notes::load().unwrap().text, "Buy milk");
        let draft = notes::load_draft().unwrap();
        assert_eq!(
            (draft.text.as_str(), draft.base.as_str()),
            ("Buy milk and tea", "Buy milk")
        );
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(
            std::fs::metadata(notes::draft_path().unwrap())
                .unwrap()
                .permissions()
                .mode()
                & 0o777,
            0o600
        );
        // Opening again brings them back, still unsaved and explained.
        let (mut app, _) = Settings::init(Core::default(), Page::parse("notes"));
        assert!(app.tools.dirty() && app.tools.restored && app.tools.conflict.is_none());
        assert_eq!(app.tools.content.text().trim_end(), "Buy milk and tea");
        // Revert is still a deliberate choice, and forgets the draft.
        let _ = app.update(Message::Tools(Edit::NoteRevert));
        assert!(!app.tools.dirty() && !app.tools.restored);
        assert_eq!(notes::load_draft(), None);
        assert_eq!(notes::load().unwrap().text, "Buy milk");
        // Unsaved edits, then the desktop saves a different note before the
        // window closes: both texts survive and the user chooses.
        type_text(&mut app, "Oat ");
        notes::save("Buy milk", "Buy milk\n[ ] Call Sam").unwrap();
        app.on_app_exit();
        drop(app);
        assert_eq!(notes::load().unwrap().text, "Buy milk\n[ ] Call Sam");
        let (mut app, _) = Settings::init(Core::default(), Page::parse("notes"));
        assert!(app.tools.dirty() && app.tools.restored);
        assert_eq!(app.tools.content.text().trim_end(), "Oat Buy milk");
        assert_eq!(
            app.tools.conflict.as_ref().map(|note| note.text.as_str()),
            Some("Buy milk\n[ ] Call Sam")
        );
        // Save explains instead of replacing; Keep mine is the explicit choice.
        send_settings(&mut app, Message::Tools(Edit::NoteSave));
        assert_eq!(notes::load().unwrap().text, "Buy milk\n[ ] Call Sam");
        // Revert during the conflict shows the note as saved now, not the
        // older text these edits started from.
        let mut reverted = Settings::init(Core::default(), Page::parse("notes")).0;
        let _ = reverted.update(Message::Tools(Edit::NoteRevert));
        assert_eq!(
            reverted.tools.content.text().trim_end(),
            "Buy milk\n[ ] Call Sam"
        );
        assert!(!reverted.tools.dirty() && reverted.tools.conflict.is_none());
        drop(reverted);
        notes::keep_draft(Some(&notes::Draft {
            text: "Oat Buy milk".into(),
            base: "Buy milk".into(),
        }))
        .unwrap();
        send_settings(&mut app, Message::Tools(Edit::NoteKeepMine));
        assert_eq!(notes::load().unwrap().text, "Oat Buy milk");
        assert!(!app.tools.dirty() && app.tools.conflict.is_none());
        assert_eq!(notes::load_draft(), None);
        // The same text saved elsewhere settles cleanly: no false conflict.
        type_text(&mut app, "x");
        notes::save("Oat Buy milk", "xOat Buy milk").unwrap();
        let _ = app.update(Message::Tick);
        assert!(!app.tools.dirty() && app.tools.conflict.is_none());
        type_text(&mut app, "y");
        send_settings(&mut app, Message::Tools(Edit::NoteSave));
        assert_eq!(notes::load().unwrap().text, "xyOat Buy milk");
        assert!(app.tools.conflict.is_none() && !app.tools.dirty());
        // An unsaved note is marked in the navigation and the Notes tab.
        type_text(&mut app, "z");
        let _ = app.update(Message::Page(Page::Music));
        assert_eq!(notes::load_draft().unwrap().text, "xyzOat Buy milk");
        let _ = app.update(Message::Tools(Edit::NoteRevert));
        app.on_app_exit();
        assert_eq!(notes::load_draft(), None);
        // A kept draft whose text was saved meanwhile is simply settled.
        notes::keep_draft(Some(&notes::Draft {
            text: "xyOat Buy milk".into(),
            base: "stale".into(),
        }))
        .unwrap();
        let (app, _) = Settings::init(Core::default(), Page::parse("notes"));
        assert!(!app.tools.dirty() && !app.tools.restored && app.tools.conflict.is_none());
        assert_eq!(notes::load_draft(), None);
        // None of it is configuration.
        assert!(
            !std::fs::read_to_string(Config::path().unwrap())
                .unwrap()
                .contains("milk")
        );
    }

    #[test]
    fn capture_gates_are_strict() {
        use desktop::Message as Edit;
        assert!(capture_gate(true, true));
        assert!(!capture_gate(true, false));
        assert!(!capture_gate(false, true));
        for message in [Edit::Apply, Edit::Undo, Edit::Recover, Edit::KeepRecovery] {
            assert!(!capture_allows(message.writes(), false), "{message:?}");
            assert!(capture_allows(message.writes(), true), "{message:?}");
        }
        for message in [
            Edit::Opacity(37),
            Edit::Refresh,
            Edit::Discard,
            Edit::UseCurrent,
        ] {
            assert!(capture_allows(message.writes(), false), "{message:?}");
        }
        use system_appearance::Message as Look;
        for message in [Look::Apply, Look::Undo, Look::Recover, Look::KeepRecovery] {
            assert!(!capture_allows(message.writes(), false), "{message:?}");
        }
        for event in [
            json!("apply"),
            json!("undo"),
            json!("recover"),
            json!("keep"),
        ] {
            assert!(capture_appearance(&event).unwrap().writes(), "{event}");
        }
        for event in [
            json!({"variant":"light"}),
            json!({"color":["accent","#E66100"]}),
            json!({"color":["background","auto"]}),
            json!({"outline":4}),
            json!({"roundness":"Square"}),
            json!({"density":"Spacious"}),
            json!({"frosted":["panel",true]}),
            json!({"window":["shadow_tiled",true]}),
            json!({"picker":"interface"}),
            json!({"font":["monospace","Fira Mono"]}),
            json!({"icons":"Pop"}),
            json!("use_current"),
        ] {
            let message = capture_appearance(&event).unwrap_or_else(|| panic!("{event}"));
            assert!(!message.writes(), "{event}");
        }
        assert!(capture_appearance(&json!({"color":["accent","#12"]})).is_none());
    }

    /// Sends page links plus Dock & Panel and System appearance capture
    /// events, including writes, and returns to the Dock & Panel page.
    fn capture_edit_and_apply(app: &mut Settings) {
        send_command(
            app,
            json!({"page":"desktop-appearance","appearance":[{"color":["accent","#E66100"]},{"window":["shadow_tiled",true]},"apply","recover","keep"]}),
        );
        assert_eq!(app.page, Page::DesktopAppearance, "page links still work");
        send_command(
            app,
            json!({"page":"desktop-windows","windows":[{"autotile":true},{"gap":["outer",20]},"apply","undo","recover","keep"]}),
        );
        assert_eq!(app.page, Page::DesktopWindows, "page links still work");
        send_command(
            app,
            json!({"page":"desktop-workspaces","workspaces":[{"layout":"horizontal"},"apply","undo","recover","keep"]}),
        );
        // Workspaces is a section of the Windows page.
        assert_eq!(app.page, Page::DesktopWindows, "page links still work");
        send_command(
            app,
            json!({"page":"desktop-window-rules","window_rules":["add",{"app_id":"org.example.Sentinel"},"save","apply","undo","recover","keep"]}),
        );
        assert_eq!(app.page, Page::DesktopWindowRules, "page links still work");
        send_command(
            app,
            json!({"page":"desktop-shortcuts","shortcuts":["add",{"name":"Sentinel"},{"combo":"Super+Shift+F9"},{"command":"sentinel-command"},"save",{"running":["Super+Shift+F9",false]},"apply","undo","recover","keep"]}),
        );
        assert_eq!(app.page, Page::DesktopShortcuts, "page links still work");
        send_command(
            app,
            json!({"page":"desktop-dock","desktop":[{"opacity":37},"apply","recover","keep"]}),
        );
        assert_eq!(app.page, Page::DesktopDock, "page links still work");
    }

    #[test]
    fn capture_folder_alone_ignores_desktop_commands() {
        if desktop_test_child(
            "capture_folder_alone_ignores_desktop_commands",
            DesktopEnv {
                capture: true,
                ..Default::default()
            },
        ) {
            return;
        }
        let root = isolated_root();
        let live = live_tree(&root);
        let (mut app, _) = Settings::init(Core::default(), Page::Desktop);
        capture_edit_and_apply(&mut app);
        // The editor opened on the (fake) writable live tree; nothing reached it.
        assert!(app.desktop.writable());
        assert!(!app.desktop.dirty());
        assert_eq!(app.desktop.notice, None);
        assert!(app.system_appearance.writable());
        assert!(!app.system_appearance.dirty());
        assert_eq!(app.system_appearance.notice, None);
        assert_eq!(live_tree(&root), live);
    }

    #[test]
    fn alternate_config_alone_ignores_desktop_commands() {
        if desktop_test_child(
            "alternate_config_alone_ignores_desktop_commands",
            DesktopEnv {
                app_config: true,
                ..Default::default()
            },
        ) {
            return;
        }
        let root = isolated_root();
        let live = live_tree(&root);
        let (mut app, _) = Settings::init(Core::default(), Page::Desktop);
        capture_edit_and_apply(&mut app);
        assert!(!app.desktop.dirty());
        assert_eq!(app.desktop.notice, None);
        assert!(!app.system_appearance.dirty());
        assert_eq!(app.system_appearance.notice, None);
        assert_eq!(live_tree(&root), live);
    }

    #[test]
    fn capture_without_fixture_previews_edits_but_ignores_writes() {
        if desktop_test_child(
            "capture_without_fixture_previews_edits_but_ignores_writes",
            DesktopEnv {
                app_config: true,
                capture: true,
                ..Default::default()
            },
        ) {
            return;
        }
        let root = isolated_root();
        let live = live_tree(&root);
        let mut app = open_isolated_editor(&root);
        capture_edit_and_apply(&mut app);
        assert!(app.desktop.dirty(), "draft edits still preview");
        assert!(
            app.system_appearance.dirty(),
            "appearance drafts preview too"
        );
        assert!(!app.system_appearance.writable());
        assert!(
            app.window_rules.dirty() && app.shortcuts.dirty(),
            "new pages preview too"
        );
        assert!(!app.window_rules.writable() && !app.shortcuts.writable());
        assert_eq!(app.window_rules.notice(), None);
        assert_eq!(app.shortcuts.notice(), None);
        // Ignored before reaching the store: no read-only refusal either.
        assert_eq!(app.desktop.notice, None);
        assert_eq!(app.system_appearance.notice, None);
        assert!(!app.native_busy && app.native_deferred.is_empty());
        assert_eq!(live_tree(&root), live);
        assert!(!root.join("home/.local/share/cosmic-desktop-cards").exists());
    }

    #[test]
    fn capture_with_fixture_applies_and_undoes_in_the_fixture_only() {
        if desktop_test_child(
            "capture_with_fixture_applies_and_undoes_in_the_fixture_only",
            DesktopEnv {
                app_config: true,
                capture: true,
                fixture: true,
            },
        ) {
            return;
        }
        let root = isolated_root();
        let live = live_tree(&root);
        let fixture = root.join("cosmic-root");
        let opacity = fixture.join("config/cosmic/com.system76.CosmicPanel.Dock/v1/opacity");
        let mut app = open_isolated_editor(&root);
        assert_eq!(
            app.desktop.store().unwrap().fixture_root(),
            Some(fixture.as_path())
        );
        send_command(&mut app, json!({"desktop":[{"opacity":37},"apply"]}));
        assert_eq!(std::fs::read_to_string(&opacity).unwrap(), "0.37");
        assert!(!app.desktop.dirty());
        send_command(&mut app, json!({"desktop":["undo"]}));
        assert!(!opacity.exists());
        let effects = fixture.join("config/cosmic/com.system76.CosmicComp/v1/appearance_settings");
        send_command(
            &mut app,
            json!({"page":"desktop-appearance","appearance":[{"window":["shadow_tiled",true]},"apply"]}),
        );
        assert!(
            std::fs::read_to_string(&effects)
                .unwrap()
                .contains("shadow_tiled_windows: true")
        );
        assert!(!app.system_appearance.dirty());
        send_command(&mut app, json!({"appearance":["undo"]}));
        assert!(!effects.exists());
        assert_eq!(live_tree(&root), live);
    }

    #[test]
    fn system_appearance_drafts_survive_navigation_and_apply_only_to_the_fixture() {
        if desktop_test_child(
            "system_appearance_drafts_survive_navigation_and_apply_only_to_the_fixture",
            DesktopEnv {
                app_config: true,
                fixture: true,
                ..Default::default()
            },
        ) {
            return;
        }
        use crate::desktop::appearance::{ColorRole, ThemeVariant, WindowEffect};
        use system_appearance::Message as Look;
        let root = isolated_root();
        let live = live_tree(&root);
        let accent =
            root.join("cosmic-root/config/cosmic/com.system76.CosmicTheme.Light.Builder/v2/accent");
        let (mut app, _) = Settings::init(Core::default(), Page::DesktopAppearance);
        assert_eq!(app.system_appearance.icons.themes.len(), 2);
        assert_eq!(app.system_appearance.fonts.interface.len(), 2);
        let _ = app.update(Message::SystemAppearance(Look::Variant(
            ThemeVariant::Light,
        )));
        let _ = app.update(Message::SystemAppearance(Look::HexInput(
            ColorRole::Accent,
            "#E66100".into(),
        )));
        assert!(!accent.exists(), "text is local until submitted");
        let _ = app.update(Message::SystemAppearance(Look::Window(
            WindowEffect::ShadowTiled,
            true,
        )));
        assert!(!accent.exists(), "another control never submits text");
        let _ = app.update(Message::SystemAppearance(Look::HexSubmit(
            ColorRole::Accent,
        )));
        assert!(accent.exists());
        let _ = app.update(Message::Page(Page::Weather));
        drop(app);
        let (mut app, _) = Settings::init(Core::default(), Page::General);
        let _ = app.update(Message::UndoDesktop);
        assert!(!app.error, "{}", app.status);
        assert!(!accent.exists());
        assert_eq!(live_tree(&root), live);
    }

    #[test]
    fn system_appearance_preview_without_fixture_reads_isolated_live_tree_but_cannot_apply() {
        if desktop_test_child(
            "system_appearance_preview_without_fixture_reads_isolated_live_tree_but_cannot_apply",
            DesktopEnv {
                app_config: true,
                ..Default::default()
            },
        ) {
            return;
        }
        use crate::desktop::appearance::{ColorRole, ThemeVariant, parse_hex};
        use crate::desktop::panels::Setting;
        use system_appearance::Message as Look;
        let root = isolated_root();
        let live = live_tree(&root);
        let (mut app, _) = Settings::init(Core::default(), Page::DesktopAppearance);
        let store = app.system_appearance.store().expect("store");
        assert!(store.user_root().starts_with(&root));
        assert!(!app.system_appearance.writable());
        let draft = app.system_appearance.draft().unwrap();
        // The user override wins over the system default.
        assert_eq!(
            draft.color(ThemeVariant::Light, ColorRole::Accent),
            Setting::Known(parse_hex("#E5A50A", false))
        );
        let _ = app.update(Message::SystemAppearance(Look::Color(
            ColorRole::Background,
            parse_hex("#101010", true),
        )));
        assert!(app.system_appearance.dirty());
        assert!(!app.system_appearance.can_apply());
        let _ = app.update(Message::SystemAppearance(Look::Apply));
        let _ = app.update(Message::SystemAppearance(Look::Recover));
        let _ = app.update(Message::SystemAppearance(Look::KeepRecovery));
        let mut tree = Tree::new(app.settings_view().as_widget());
        tree.diff(app.settings_view().as_widget_mut());
        assert_eq!(live_tree(&root), live);
        assert!(!root.join("home/.local/share/cosmic-desktop-cards").exists());
    }

    /// Lays out the real System appearance view through pickers, the effects
    /// disclosure, variant switches and page changes at the minimum size.
    #[test]
    fn system_appearance_lays_out_through_pickers_effects_and_navigation() {
        if desktop_test_child(
            "system_appearance_lays_out_through_pickers_effects_and_navigation",
            DesktopEnv {
                app_config: true,
                fixture: true,
                ..Default::default()
            },
        ) {
            return;
        }
        use crate::desktop::appearance::{ColorRole, FontRole, ThemeVariant};
        use cosmic::iced::advanced::renderer::Headless;
        use cosmic::iced::runtime::user_interface::{Cache, UserInterface};
        use system_appearance::{Message as Look, Picker};
        let root = isolated_root();
        let fixture = root.join("cosmic-root");
        // A missing selected font and icon theme are shown, not replaced.
        seed(
            &fixture.join("config/cosmic"),
            &[
                ("com.system76.CosmicTk", "icon_theme", "\"Gone-Theme\""),
                (
                    "com.system76.CosmicTk",
                    "interface_font",
                    "(family: \"A Very Long Missing Font Family Name Indeed\", weight: Normal, stretch: Normal, style: Normal)",
                ),
            ],
        );
        let (mut app, _) = Settings::init(Core::default(), Page::DesktopAppearance);
        let mut renderer =
            cosmic::iced::futures::executor::block_on(<cosmic::Renderer as Headless>::new(
                cosmic::font::default(),
                cosmic::iced::Pixels(14.),
                Some("tiny-skia"),
            ))
            .expect("headless renderer");
        let size = Size::new(820., 600.);
        let mut cache =
            UserInterface::build(app.settings_view(), size, Cache::default(), &mut renderer)
                .into_cache();
        for message in [
            Message::SystemAppearance(Look::Picker(Some(Picker::Font(FontRole::Interface)))),
            Message::SystemAppearance(Look::Search("sans".into())),
            Message::SystemAppearance(Look::FontFamily(FontRole::Interface, "Open Sans".into())),
            Message::SystemAppearance(Look::Picker(Some(Picker::Icons))),
            Message::SystemAppearance(Look::Search("zzz".into())),
            Message::SystemAppearance(Look::Picker(None)),
            Message::SystemAppearance(Look::Effects(true)),
            Message::SystemAppearance(Look::Variant(ThemeVariant::Light)),
            Message::SystemAppearance(Look::HexInput(ColorRole::Primary, "#12".into())),
            Message::SystemAppearance(Look::HexSubmit(ColorRole::Primary)),
            Message::Page(Page::DesktopDock),
            Message::Page(Page::DesktopAppearance),
            Message::SystemAppearance(Look::Effects(false)),
            Message::Page(Page::Desktop),
            Message::Page(Page::Appearance),
            Message::Page(Page::DesktopAppearance),
        ] {
            let _ = app.update(message);
            cache =
                UserInterface::build(app.settings_view(), size, cache, &mut renderer).into_cache();
        }
        // A short (125%-scaled) viewport moves the header into the scroll.
        app.window = Size::new(656., 480.);
        assert!(!app.appearance_pinned());
        let small = Size::new(656., 480.);
        for message in [
            Message::SystemAppearance(Look::Effects(true)),
            Message::Page(Page::DesktopDock),
            Message::Page(Page::DesktopAppearance),
        ] {
            let _ = app.update(message);
            cache =
                UserInterface::build(app.settings_view(), small, cache, &mut renderer).into_cache();
        }
        let draft = app.system_appearance.draft().unwrap();
        assert!(!draft.edited(crate::desktop::appearance::Control::Font(
            FontRole::Interface
        )));
        assert_eq!(
            draft.icon_theme(),
            crate::desktop::panels::Setting::Known("Gone-Theme".into())
        );
        assert!(app.system_appearance.hex_error(ColorRole::Primary));
    }

    #[test]
    fn glance_position_keeps_default_column_reordering_available() {
        if devbox_test_child("glance_position_keeps_default_column_reordering_available") {
            return;
        }
        Config::save_changes(json!({"widget_positions":{"glance":[0.1,0.2]}, "widget_order":["devbox","weather","projects"]})).unwrap();
        let (mut app, _) = Settings::init(Core::default(), Page::WidgetLayout);
        assert!(!app.config.has_custom_positions());
        let _ = app.update(Message::Widget(Widget::Glance, true));
        assert_eq!(
            app.config.ordered_right_cards(),
            [Widget::Devbox, Widget::Projects]
        );
        let _ = app.update(Message::Move(Widget::Projects, -1));
        assert_eq!(
            app.config.ordered_right_cards(),
            [Widget::Projects, Widget::Devbox]
        );
        assert_eq!(
            raw_preferences()["widget_order"],
            json!(["projects", "weather", "devbox"])
        );
        let _ = app.update(Message::Widget(Widget::Glance, false));
        assert_eq!(
            app.config.ordered_right_cards(),
            [Widget::Projects, Widget::Weather, Widget::Devbox]
        );
        let _ = app.update(Message::DragStart(Widget::Weather));
        assert_eq!(app.reorder.source, Some(Widget::Weather));
        assert_eq!(
            raw_preferences()["widget_positions"],
            json!({"glance":[0.1,0.2]})
        );
    }

    #[test]
    fn glance_keeps_its_own_background_and_fonts_under_a_widget_style() {
        if devbox_test_child("glance_keeps_its_own_background_and_fonts_under_a_widget_style") {
            return;
        }
        let (mut app, _) = Settings::init(Core::default(), Page::Glance);
        app.config.appearance.family = Some(crate::desktop_theme::Family::Paper);
        assert!(app.config.glance_panel());
        let _ = app.update(Message::GlanceStyle(GlanceStyle::Transparent));
        assert!(app.config.glance.own_background, "{}", app.status);
        assert_eq!(app.config.glance.style, GlanceStyle::Transparent);
        let _ = app.update(Message::WidgetFont(
            FontTarget::GlanceText,
            Some("Hack".into()),
        ));
        let _ = app.update(Message::WidgetFont(
            FontTarget::GlanceClock,
            Some("Lora".into()),
        ));
        let saved = Config::try_load().unwrap();
        assert!(saved.glance.own_background);
        assert_eq!(saved.glance.font.as_deref(), Some("Hack"));
        assert_eq!(saved.glance.clock_font.as_deref(), Some("Lora"));
        // The widget fonts are untouched.
        assert_eq!(saved.widget_font, None);
        let _ = app.update(Message::GlanceMatchWidgets);
        assert!(!Config::try_load().unwrap().glance.own_background);
        let _ = app.update(Message::WidgetFont(FontTarget::GlanceText, None));
        assert_eq!(Config::try_load().unwrap().glance.font, None);
    }

    #[test]
    fn glance_and_history_settings_keep_the_placement_draft_and_original_widgets() {
        if devbox_test_child(
            "glance_and_history_settings_keep_the_placement_draft_and_original_widgets",
        ) {
            return;
        }
        let (mut app, _) = Settings::init(Core::default(), Page::WidgetLayout);
        let before = raw_preferences();
        let original = app.config.enabled_widgets.clone();
        send_settings(&mut app, Message::Canvas(canvas::Message::Start));
        let _ = app.update(Message::Placement(placement::Message::Snap(false)));
        let _ = app.update(Message::Widget(Widget::Glance, true));
        let _ = app.update(Message::Page(Page::Glance));
        let _ = app.update(Message::GlanceStyle(GlanceStyle::Panel));
        let _ = app.update(Message::GlanceDensity(GlanceDensity::Comfortable));
        let _ = app.update(Message::GlanceSection("show_tomorrow", true));
        let _ = app.update(Message::GlanceSection("show_weather", false));
        let _ = app.update(Message::DevboxHistory("cpu", true));
        assert!(app.placement.dirty());
        assert!(app.config.devbox.history.cpu);
        assert_eq!(app.config.glance.density, GlanceDensity::Comfortable);
        assert!(app.config.glance.show_tomorrow);
        assert!(!app.config.data_visible(Widget::Weather));
        let _ = app.update(Message::GlanceDensity(GlanceDensity::Compact));
        // Density does not change the selected days or the standalone
        // calendar's independent overview preferences.
        assert!(app.config.glance.show_tomorrow);
        let _ = app.update(Message::Widget(Widget::Glance, false));
        assert_eq!(app.config.enabled_widgets, original);
        assert!(app.config.widget_visible(Widget::Weather));
        let raw = raw_preferences();
        assert_eq!(raw["widget_positions"], before["widget_positions"]);
        assert_eq!(raw["future_field"], 42);
        assert_eq!(raw["devbox"]["future_metric"], 17);
        assert!(
            raw.get("placement").is_none(),
            "the layout draft remains local"
        );
        assert!(app.canvas_dirty());
        assert!(app.placement.dirty());
    }

    #[test]
    fn devbox_moves_and_details_preserve_hidden_metrics_and_other_preferences() {
        if devbox_test_child(
            "devbox_moves_and_details_preserve_hidden_metrics_and_other_preferences",
        ) {
            return;
        }
        let (mut app, _) = Settings::init(Core::default(), Page::WidgetLayout);
        send_settings(&mut app, Message::Canvas(canvas::Message::Start));
        let _ = app.update(Message::Placement(placement::Message::Snap(false)));
        let _ = app.update(Message::Page(Page::Devbox));
        assert!(app.placement.dirty());
        devbox_command(
            &mut app,
            json!([
                {"move":"services","direction":-1},
                {"toggle":"memory_cache","value":false},
                {"toggle":"swap","value":false}
            ]),
        );
        let raw = raw_preferences();
        assert_eq!(
            raw["devbox"]["order"],
            json!([
                "cpu", "memory", "disk", "network", "gpu", "services", "uptime"
            ])
        );
        assert_eq!(raw["devbox"]["services"], false);
        assert_eq!(raw["devbox"]["memory_cache"], false);
        assert_eq!(raw["devbox"]["swap"], false);
        assert_eq!(raw["devbox"]["future_metric"], 17);
        assert_eq!(raw["theme"], "teal");
        assert_eq!(raw["future_field"], 42);
        assert_eq!(
            raw["widget_order"],
            json!(["weather", "devbox", "projects"])
        );
        assert_eq!(raw["widget_positions"], json!({"weather":[0.5,0.4]}));
        devbox_command(
            &mut app,
            json!([
                {"toggle":"services","value":true},
                {"toggle":"memory_cache","value":true},
                {"move":"cpu","direction":-1},
                {"move":"uptime","direction":1}
            ]),
        );
        let latest = raw_preferences();
        assert_eq!(latest["devbox"]["services"], true);
        assert_eq!(latest["devbox"]["memory_cache"], true);
        assert_eq!(latest["devbox"]["swap"], false);
        assert_eq!(latest["devbox"]["order"], raw["devbox"]["order"]);
        assert!(
            app.placement.dirty(),
            "A Devbox change must preserve the layout draft"
        );
        assert_eq!(
            latest["placement"]["snap"],
            serde_json::Value::Null,
            "unapplied layout preferences stay local"
        );
        assert!(!app.busy, "Visibility must not run a service command");
    }

    #[test]
    fn devbox_drag_uses_latest_order_and_does_not_save_cancelled_changes() {
        if devbox_test_child("devbox_drag_uses_latest_order_and_does_not_save_cancelled_changes") {
            return;
        }
        let path = Config::path().unwrap();
        let (mut app, _) = Settings::init(Core::default(), Page::Devbox);
        let original = std::fs::read(&path).unwrap();
        for finish in [
            json!(["cancel", "drop"]),
            json!([{"drag_leave":"services"}, "drop"]),
        ] {
            devbox_command(
                &mut app,
                json!([{"drag_start":"memory"}, {"drag_over":"services"}]),
            );
            assert_eq!(std::fs::read(&path).unwrap(), original);
            devbox_command(&mut app, finish);
            assert_eq!(std::fs::read(&path).unwrap(), original);
        }
        devbox_command(
            &mut app,
            json!([{"drag_start":"memory"}, {"drag_over":"services"}]),
        );
        let _ = app.update(Message::Page(Page::Clock));
        devbox_command(&mut app, json!(["drop"]));
        assert_eq!(std::fs::read(&path).unwrap(), original);
        let _ = app.update(Message::Page(Page::Devbox));
        devbox_command(
            &mut app,
            json!([{"drag_start":"memory"}, {"drag_over":"cpu"}]),
        );
        Config::save_devbox(json!({
            "order":["disk", "cpu", "memory", "network", "gpu", "uptime", "services"],
            "network":false,
            "future_metric":29
        }))
        .unwrap();
        let _ = app.update(Message::DragDrop);
        let raw = raw_preferences();
        assert_eq!(
            raw["devbox"]["order"],
            json!([
                "disk", "memory", "cpu", "network", "gpu", "uptime", "services"
            ])
        );
        assert_eq!(raw["devbox"]["network"], false);
        assert_eq!(raw["devbox"]["services"], false);
        assert_eq!(raw["devbox"]["future_metric"], 29);
        assert_eq!(raw["widget_positions"], json!({"weather":[0.5,0.4]}));
        assert_eq!(raw["future_field"], 42);
    }

    #[test]
    fn dragging_cards_saves_only_on_drop_and_cancel_preserves_preferences() {
        if std::env::var_os("DESKTOP_CARDS_REORDER_TEST").is_none() {
            let dir =
                std::env::temp_dir().join(format!("cards-reorder-test-{}", std::process::id()));
            std::fs::create_dir_all(&dir).unwrap();
            let path = dir.join("config.json");
            std::fs::write(&path, r#"{"theme":"teal","widget_order":["weather","devbox","projects"],"enabled_widgets":["clock","weather"],"future_field":42}"#).unwrap();
            let status = std::process::Command::new(std::env::current_exe().unwrap())
                .args(["--exact", "settings::tests::dragging_cards_saves_only_on_drop_and_cancel_preserves_preferences"])
                .env("DESKTOP_CARDS_REORDER_TEST", "1").env("DESKTOP_CARDS_CONFIG", &path)
                .status().unwrap();
            let _ = std::fs::remove_dir_all(dir);
            assert!(status.success());
            return;
        }
        let (mut app, _) = Settings::init(Core::default(), Page::WidgetLayout);
        let original = vec![Widget::Weather, Widget::Devbox, Widget::Projects];
        let _ = app.update(Message::DragStart(Widget::Weather));
        let _ = app.update(Message::DragOver(Widget::Projects));
        assert_eq!(Config::load().ordered_right_cards(), original);
        let _ = app.update(Message::DragCancel);
        let _ = app.update(Message::DragDrop);
        assert_eq!(Config::load().ordered_right_cards(), original);
        let _ = app.update(Message::DragStart(Widget::Weather));
        let _ = app.update(Message::DragOver(Widget::Projects));
        let _ = app.update(Message::DragLeave(Widget::Projects));
        let _ = app.update(Message::DragDrop);
        assert_eq!(Config::load().ordered_right_cards(), original);
        let _ = app.update(Message::DragStart(Widget::Weather));
        let _ = app.update(Message::DragOver(Widget::Projects));
        let _ = app.update(Message::DragLeave(Widget::Weather));
        let _ = app.update(Message::DragDrop);
        let config = Config::try_load().unwrap();
        assert_eq!(
            config.ordered_right_cards(),
            vec![Widget::Devbox, Widget::Projects, Widget::Weather]
        );
        assert_eq!(config.theme, "teal");
        assert!(config.widget_visible(Widget::Weather));
        assert!(!config.widget_visible(Widget::Devbox));
        assert!(app.reorder.source.is_none() && app.reorder.target.is_none());
        let raw: serde_json::Value =
            serde_json::from_slice(&std::fs::read(Config::path().unwrap()).unwrap()).unwrap();
        assert_eq!(raw["future_field"], 42);
        Config::save_changes(json!({"widget_positions":{"weather":[0.5,0.4]}})).unwrap();
        let _ = app.update(Message::Tick);
        let _ = app.update(Message::DragStart(Widget::Weather));
        assert!(app.reorder.source.is_none());
        let _ = app.update(Message::Move(Widget::Weather, -1));
        assert_eq!(
            Config::load().ordered_right_cards(),
            vec![Widget::Devbox, Widget::Projects, Widget::Weather]
        );
    }
    #[test]
    fn slider_edits_save_without_mouse_release_and_preserve_other_preferences() {
        // Isolate the actual update/save path in a child, never changing this
        // process's environment or the user's preferences in parallel tests.
        if std::env::var_os("DESKTOP_CARDS_SLIDER_TEST").is_none() {
            let dir =
                std::env::temp_dir().join(format!("cards-slider-test-{}", std::process::id()));
            std::fs::create_dir_all(&dir).unwrap();
            let path = dir.join("config.json");
            std::fs::write(
                &path,
                r#"{"theme":"teal","appearance":{"match_wallpaper":false},"future_field":42}"#,
            )
            .unwrap();
            let status = std::process::Command::new(std::env::current_exe().unwrap())
                .args(["--exact", "settings::tests::slider_edits_save_without_mouse_release_and_preserve_other_preferences"])
                .env("DESKTOP_CARDS_SLIDER_TEST", "1").env("DESKTOP_CARDS_CONFIG", &path)
                .status().unwrap();
            let _ = std::fs::remove_dir_all(dir);
            assert!(status.success());
            return;
        }
        let (mut app, _) = Settings::init(Core::default(), Page::Appearance);
        let _ = app.update(Message::AppearanceValue(AppearanceControl::Opacity, 85));
        let stale_generation = app.edit_generation;
        assert!(app.save_at.is_none());
        let _ = app.update(Message::AppearanceValue(AppearanceControl::Corners, 12));
        let _ = app.update(Message::CommitEdits(stale_generation));
        assert_eq!(Config::load().appearance.opacity, 92);
        Config::save_appearance(json!({"border_opacity":35})).unwrap();
        let _ = app.update(Message::SaveAppearance);
        let config = Config::try_load().unwrap();
        assert_eq!(
            (
                config.appearance.opacity,
                config.appearance.corner_radius,
                config.appearance.border_opacity
            ),
            (85, 12, 35)
        );
        assert!(!config.appearance.match_wallpaper);
        assert!(!app.appearance_editing && app.save_at.is_none());
        let _ = app.update(Message::Notice(25));
        let _ = app.update(Message::SaveNotice);
        assert_eq!(Config::load().meeting_notice_minutes, 25);
        let _ = app.update(Message::AppearanceValue(AppearanceControl::Opacity, 90));
        let _ = app.update(Message::Page(Page::Clock));
        assert_eq!(Config::load().appearance.opacity, 90);
        let _ = app.update(Message::Notice(30));
        app.on_app_exit();
        assert_eq!(Config::load().meeting_notice_minutes, 30);
        let raw: serde_json::Value =
            serde_json::from_slice(&std::fs::read(Config::path().unwrap()).unwrap()).unwrap();
        assert_eq!(raw["future_field"], 42);
    }
    #[test]
    fn cosmic_color_matching_preview_only_saves_preferences_and_flushes_drafts() {
        if std::env::var_os("DESKTOP_CARDS_COSMIC_COLORS_TEST").is_none() {
            let dir = std::env::temp_dir()
                .join(format!("cards-cosmic-colors-test-{}", std::process::id()));
            std::fs::create_dir_all(&dir).unwrap();
            let path = dir.join("config.json");
            std::fs::write(
                &path,
                r#"{"theme":"teal","appearance":{"match_wallpaper":false,"future_tint":0.4},"future_field":42}"#,
            )
            .unwrap();
            let status = std::process::Command::new(std::env::current_exe().unwrap())
                .args(["--exact", "settings::tests::cosmic_color_matching_preview_only_saves_preferences_and_flushes_drafts"])
                .env("DESKTOP_CARDS_COSMIC_COLORS_TEST", "1")
                .env("DESKTOP_CARDS_CONFIG", &path)
                .status()
                .unwrap();
            let _ = std::fs::remove_dir_all(dir);
            assert!(status.success());
            return;
        }
        // Appearance now schedules read-only wallpaper thumbnails. Inspect
        // their actual completion rather than equating any Task with an OS write.
        fn assert_only_backdrop(task: Task<Message>) {
            use cosmic::iced::futures::StreamExt;
            let runtime = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .unwrap();
            runtime.block_on(async {
                if let Some(mut stream) = cosmic::iced::runtime::task::into_stream(task) {
                    while let Some(action) = stream.next().await {
                        assert!(
                            matches!(
                                action,
                                cosmic::iced::runtime::Action::Output(cosmic::Action::App(
                                    Message::Profiles(profiles::Message::Backdrop(_, _))
                                ))
                            ),
                            "Preview must only schedule read-only backdrop work"
                        );
                    }
                }
            });
        }
        let path = Config::path().unwrap();
        let (mut app, _) = Settings::init(Core::default(), Page::Appearance);
        assert!(!app.config.appearance.match_cosmic_colors);
        let _ = app.update(Message::AppearanceValue(AppearanceControl::Opacity, 85));
        let task = app.update(Message::MatchCosmicColors(true));
        assert_only_backdrop(task);
        assert!(!app.busy && !app.error);
        assert!(app.config.appearance.match_cosmic_colors);
        assert_eq!(app.config.appearance.opacity, 85);
        assert!(!app.appearance_editing && app.save_at.is_none());
        let _ = app.update(Message::AppearanceValue(AppearanceControl::Corners, 12));
        let task = app.update(Message::MatchCosmicColors(false));
        assert_only_backdrop(task);
        let config = Config::try_load().unwrap();
        assert!(!config.appearance.match_cosmic_colors);
        assert!(!config.appearance.match_wallpaper);
        assert_eq!(config.appearance.opacity, 85);
        assert_eq!(config.appearance.corner_radius, 12);
        assert_eq!(config.theme, "teal");
        let saved = std::fs::read(&path).unwrap();
        let raw: serde_json::Value = serde_json::from_slice(&saved).unwrap();
        assert_eq!(raw["appearance"]["future_tint"], 0.4);
        assert_eq!(raw["future_field"], 42);
        app.busy = true;
        assert_eq!(app.update(Message::MatchCosmicColors(true)).units(), 0);
        assert_eq!(std::fs::read(&path).unwrap(), saved);
        app.busy = false;
        let _ = app.update(Message::AppearanceValue(AppearanceControl::Opacity, 88));
        std::fs::write(&path, "{").unwrap();
        assert_eq!(app.update(Message::MatchCosmicColors(true)).units(), 0);
        assert!(app.error);
        assert!(!app.busy);
        assert!(!app.config.appearance.match_cosmic_colors);
    }

    #[test]
    fn graphical_pages_reconcile_with_settings_and_search_changes() {
        let (mut app, _) = Settings::init(Core::default(), Page::Appearance);
        let mut tree = Tree::new(app.settings_view().as_widget());
        for theme in ["lavender", "amber", "teal"] {
            app.config.theme = theme.into();
            for page in Page::ALL {
                app.page = page;
                for (today, tomorrow) in
                    [(true, false), (true, true), (false, true), (false, false)]
                {
                    app.config.calendar_show_today = today;
                    app.config.calendar_show_tomorrow = tomorrow;
                    tree.diff(app.settings_view().as_widget_mut());
                }
            }
        }
        app.page = Page::Weather;
        app.cities = vec![City {
            name: "Example city".into(),
            admin1: "Region".into(),
            country: "Country".into(),
            latitude: 1.,
            longitude: 2.,
            timezone: None,
        }];
        tree.diff(app.settings_view().as_widget_mut());
        app.cities.clear();
        app.city_search.searched = true;
        tree.diff(app.settings_view().as_widget_mut());
        app.page = Page::General;
        app.restore_confirm = true;
        tree.diff(app.settings_view().as_widget_mut());
        app.page = Page::Projects;
        app.config.project_roots.clear();
        tree.diff(app.settings_view().as_widget_mut());
    }
    #[test]
    fn stale_city_results_do_not_replace_a_new_query() {
        let (mut app, _) = Settings::init(Core::default(), Page::Weather);
        app.city_search.generation = 4;
        app.city_search.searching = true;
        let _ = app.update(Message::SearchCity);
        assert_eq!(
            app.city_search.generation, 4,
            "Enter must not start a duplicate search"
        );
        let _ = app.update(Message::Cities(
            3,
            Ok(vec![City {
                name: "Wrong city".into(),
                admin1: String::new(),
                country: String::new(),
                latitude: 0.,
                longitude: 0.,
                timezone: None,
            }]),
        ));
        assert!(app.cities.is_empty());
        assert!(app.city_search.searching);
        let _ = app.update(Message::Cities(4, Ok(Vec::new())));
        assert!(!app.city_search.searching);
        assert!(app.city_search.searched);
    }

    #[test]
    fn compact_city_cancel_ignores_late_results_and_preview_does_not_write() {
        if desktop_test_child(
            "compact_city_cancel_ignores_late_results_and_preview_does_not_write",
            DesktopEnv {
                app_config: true,
                fixture: true,
                ..Default::default()
            },
        ) {
            return;
        }
        let before = raw_preferences();
        let (mut app, _) = Settings::init(Core::default(), Page::Weather);
        let _ = app.update(Message::EditCity(true));
        let _ = app.update(Message::CityInput("Zagreb".into()));
        assert!(app.city_editing);
        let _ = app.city_search.begin(app.city_search.generation);
        let request = app.city_search.generation;
        let _ = app.update(Message::EditCity(false));
        let _ = app.update(Message::Cities(
            request,
            Ok(vec![City {
                name: "Zagreb".into(),
                admin1: String::new(),
                country: "Croatia".into(),
                latitude: 45.8,
                longitude: 15.9,
                timezone: None,
            }]),
        ));
        assert!(!app.city_editing && !app.city_search.searching);
        assert!(app.cities.is_empty() && app.city_search.deadline.is_none());
        assert_eq!(app.update(Message::Preview).units(), 0);
        assert_eq!(raw_preferences(), before);
    }

    #[test]
    fn combined_tomorrow_preserves_density_and_separate_days_through_save_and_undo() {
        if desktop_test_child(
            "combined_tomorrow_preserves_density_and_separate_days_through_save_and_undo",
            DesktopEnv {
                app_config: true,
                fixture: true,
                ..Default::default()
            },
        ) {
            return;
        }
        let (mut app, _) = Settings::init(Core::default(), Page::Calendar);
        let _ = app.update(Message::Widget(Widget::Glance, true));
        let before = app.config.clone();
        let _ = app.update(Message::GlanceTomorrow(true));
        assert_eq!(app.config.glance.density, GlanceDensity::Compact);
        assert!(app.config.glance.show_tomorrow);
        let saved = Config::try_load().unwrap();
        assert_eq!(saved.glance.density, GlanceDensity::Compact);
        assert!(saved.glance.show_tomorrow);
        assert_eq!(app.config.calendar_show_today, before.calendar_show_today);
        assert_eq!(
            app.config.calendar_show_tomorrow,
            before.calendar_show_tomorrow
        );
        let _ = app.update(Message::UndoPreferences);
        assert_eq!(app.config.glance, before.glance);
        assert_eq!(Config::try_load().unwrap().glance, before.glance);
        let _ = app.update(Message::GlanceTomorrow(true));
        Config::save_glance(json!({"density":"comfortable"})).unwrap();
        let _ = app.update(Message::UndoPreferences);
        let saved = Config::try_load().unwrap();
        assert!(!saved.glance.show_tomorrow);
        assert_eq!(
            saved.glance.density,
            GlanceDensity::Comfortable,
            "Undo of Tomorrow does not own a later density change"
        );
        let _ = app.update(Message::Widget(Widget::Glance, false));
        assert_eq!(app.config.calendar_show_today, before.calendar_show_today);
        assert_eq!(
            app.config.calendar_show_tomorrow,
            before.calendar_show_tomorrow
        );
    }
    #[test]
    fn compact_tomorrow_native_controls_persist_from_both_settings_pages() {
        if desktop_test_child(
            "compact_tomorrow_native_controls_persist_from_both_settings_pages",
            DesktopEnv {
                app_config: true,
                fixture: true,
                ..Default::default()
            },
        ) {
            return;
        }
        use crate::native_test::Native;
        use cosmic::iced::{Event, mouse};
        fn click(app: &mut Settings, native: &mut Native, id: &str) {
            let bounds = native_reveal(app, native, id);
            assert!(
                bounds.x >= 184.
                    && bounds.y >= 0.
                    && bounds.x + bounds.width <= native.size.width
                    && bounds.y + bounds.height <= native.size.height - 1.,
                "{id} must fit: {bounds:?} in {:?}",
                native.size
            );
            for input in [
                mouse::Event::CursorMoved {
                    position: bounds.center(),
                },
                mouse::Event::ButtonPressed(mouse::Button::Left),
                mouse::Event::ButtonReleased(mouse::Button::Left),
            ] {
                for message in native
                    .event(app.settings_view(), Event::Mouse(input), bounds.center())
                    .0
                {
                    let _ = app.update(message);
                }
            }
        }
        for size in [Size::new(820., 600.), Size::new(1024., 720.)] {
            for page in [Page::Glance, Page::Calendar] {
                Config::save_changes(json!({
                    "glance":{"enabled":true,"initialized":true,"density":"compact","show_calendar":true,"show_tomorrow":false},
                    "calendar_show_today":false,"calendar_show_tomorrow":true,
                    "setup_dismissed":true
                })).unwrap();
                let (mut app, _) = Settings::init(Core::default(), page);
                let _ = app.update(Message::Resized(size));
                app.calendar.connection.backend = crate::calendar::Backend::Google;
                app.calendar.connection.account = "demo@example.com".into();
                app.calendar.connection.calendar_name = "Work".into();
                app.calendar.loading = false;
                let mut native = Native::new(size);
                click(&mut app, &mut native, "glance-tomorrow");
                let saved = Config::try_load().unwrap();
                assert!(saved.glance.show_tomorrow);
                assert_eq!(saved.glance.density, GlanceDensity::Compact);
                assert!(!saved.calendar_show_today && saved.calendar_show_tomorrow);
                native.capture(
                    app.settings_view(),
                    &controls_theme(&app.config),
                    &format!("refined-{}-tomorrow-{}", page.key(), size.width),
                );
                click(&mut app, &mut native, "settings-undo");
                assert!(!Config::try_load().unwrap().glance.show_tomorrow);
                assert_eq!(app.config.glance.density, GlanceDensity::Compact);
            }
        }
    }

    #[test]
    fn refined_settings_native_help_and_pages_preserve_preferences() {
        if desktop_test_child(
            "refined_settings_native_help_and_pages_preserve_preferences",
            DesktopEnv {
                app_config: true,
                fixture: true,
                ..Default::default()
            },
        ) {
            return;
        }
        use crate::native_test::Native;
        use cosmic::iced::{Event, mouse};
        Config::save_changes(json!({
            "location":{"name":"Zagreb","latitude":45.8,"longitude":15.9},
            "setup_dismissed":true,
            "glance":{"enabled":true,"initialized":true,"density":"compact","show_tomorrow":true},
            "clock":{"alignment":"left"}
        }))
        .unwrap();
        let preferences = raw_preferences();
        for size in [Size::new(820., 600.), Size::new(1024., 720.)] {
            for page in [
                Page::Overview,
                Page::Widgets,
                Page::Glance,
                Page::Calendar,
                Page::Clock,
                Page::Weather,
                Page::Devbox,
                Page::Appearance,
                Page::General,
                Page::DesktopAppearance,
                Page::DesktopWindows,
                Page::DesktopWorkspaces,
                Page::DesktopMotion,
                Page::DesktopDock,
                Page::DesktopProfiles,
                Page::DesktopShortcuts,
                Page::DesktopAutomation,
            ] {
                let (mut app, _) = Settings::init(Core::default(), page);
                let _ = app.update(Message::Resized(size));
                app.calendar.connection.backend = crate::calendar::Backend::Google;
                app.calendar.connection.account = "demo@example.com".into();
                app.calendar.connection.calendar_name = "Work".into();
                app.calendar.loading = false;
                app.service_known = true;
                app.service.running = true;
                let mut native = Native::new(size);
                native.capture(
                    app.settings_view(),
                    &controls_theme(&app.config),
                    &format!("refined-{}-{}", page.key(), size.width),
                );
                if page == Page::Devbox {
                    let bounds = native_reveal(&app, &mut native, "devbox-details-memory");
                    for input in [
                        mouse::Event::CursorMoved {
                            position: bounds.center(),
                        },
                        mouse::Event::ButtonPressed(mouse::Button::Left),
                        mouse::Event::ButtonReleased(mouse::Button::Left),
                    ] {
                        for message in native
                            .event(app.settings_view(), Event::Mouse(input), bounds.center())
                            .0
                        {
                            let _ = app.update(message);
                        }
                    }
                    assert!(app.devbox_details.contains(&DevboxSection::Memory));
                    native.capture(
                        app.settings_view(),
                        &controls_theme(&app.config),
                        &format!("refined-devbox-details-{}", size.width),
                    );
                }
                if page == Page::Glance {
                    let bounds = native_reveal(&app, &mut native, "help-glance-sections");
                    let closed = native.render(app.settings_view(), &controls_theme(&app.config));
                    let messages = native
                        .event(
                            app.settings_view(),
                            Event::Mouse(mouse::Event::CursorMoved {
                                position: bounds.center(),
                            }),
                            bounds.center(),
                        )
                        .0;
                    assert!(messages.is_empty(), "Hovering help is read-only");
                    assert!(
                        native.render_hover(
                            app.settings_view(),
                            &controls_theme(&app.config),
                            bounds.center()
                        ) != closed,
                        "Hover reveals contextual help"
                    );
                    native.capture_hover(
                        app.settings_view(),
                        &controls_theme(&app.config),
                        bounds.center(),
                        &format!("refined-help-hover-{}", size.width),
                    );
                    for input in [
                        mouse::Event::CursorMoved {
                            position: bounds.center(),
                        },
                        mouse::Event::ButtonPressed(mouse::Button::Left),
                        mouse::Event::ButtonReleased(mouse::Button::Left),
                    ] {
                        for message in native
                            .event(app.settings_view(), Event::Mouse(input), bounds.center())
                            .0
                        {
                            let _ = app.update(message);
                        }
                    }
                    let explanation =
                        native.bounds(app.settings_view(), "help-text-glance-sections");
                    assert!(explanation.width > 100. && explanation.height > 10.);
                    assert!(explanation.x + explanation.width <= size.width);
                    native.capture(
                        app.settings_view(),
                        &controls_theme(&app.config),
                        &format!("refined-help-{}", size.width),
                    );
                    let _ = app.update(Message::Page(Page::Widgets));
                    assert!(app.help_open.is_none(), "Help closes when leaving its page");
                }
                assert_eq!(
                    raw_preferences(),
                    preferences,
                    "Inspecting {page:?} must not save"
                );
            }
        }
    }

    #[test]
    fn project_folder_validation_rejects_files_and_accepts_existing_directories() {
        assert!(normalize_folder("").is_err());
        assert!(normalize_folder("/this-folder-does-not-exist-desktop-cards").is_err());
        assert!(normalize_folder(concat!(env!("CARGO_MANIFEST_DIR"), "/Cargo.toml")).is_err());
        assert_eq!(
            normalize_folder(env!("CARGO_MANIFEST_DIR")).unwrap(),
            env!("CARGO_MANIFEST_DIR")
        );
    }

    fn fixture_user(root: &std::path::Path, namespace: &str, key: &str) -> Option<String> {
        std::fs::read_to_string(
            root.join("cosmic-root/config/cosmic")
                .join(namespace)
                .join(if namespace.contains("CosmicTheme") {
                    "v2"
                } else {
                    "v1"
                })
                .join(key),
        )
        .ok()
    }

    /// Windows and Workspaces through Settings messages: drafts survive
    /// navigation; an outside change conflicts without losing the other
    /// edits or the earlier Undo; Undo routes to the page that owns the
    /// latest change; a pending recovery holds every page.
    #[test]
    fn workflow_conflict_keeps_draft_and_prior_undo() {
        if desktop_test_child(
            "workflow_conflict_keeps_draft_and_prior_undo",
            DesktopEnv {
                app_config: true,
                fixture: true,
                ..Default::default()
            },
        ) {
            return;
        }
        use desktop::Message as Dock;
        let (mut app, _) = Settings::init(Core::default(), Page::DesktopDock);
        let _ = app.update(Message::DesktopEditor(Dock::Anchor(
            crate::desktop::panels::Anchor::Left,
        )));
        let store = crate::desktop::DesktopStore::from_env().unwrap();
        let prior = store.latest_change().unwrap().unwrap();
        let key =
            crate::desktop::ConfigKey::new("com.system76.CosmicPanel.Dock", 1, "opacity").unwrap();
        let _ = app.update(Message::DesktopEditor(Dock::Opacity(75)));
        let path = store.user_path(&key);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, "0.81").unwrap();
        let _ = app.update(Message::DesktopEditor(Dock::Apply));
        assert!(matches!(
            app.desktop.notice,
            Some(desktop::Notice::Error(_))
        ));
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "0.81");
        assert_eq!(store.latest_change().unwrap().unwrap().id, prior.id);
        let _ = app.update(Message::DesktopEditor(Dock::UseCurrent));
        let _ = app.update(Message::DesktopEditor(Dock::Opacity(72)));
        let _ = app.update(Message::DesktopEditor(Dock::Apply));
        assert!(!app.desktop.dirty());
        assert_ne!(store.latest_change().unwrap().unwrap().id, prior.id);
        let _ = app.update(Message::Page(Page::Overview));
        let _ = app.update(Message::UndoDesktop);
        assert_eq!(std::fs::read_to_string(path).unwrap(), "0.81");
        assert_eq!(raw_preferences()["future_field"], 42);
    }

    /// Floating apps and Shortcuts through Settings messages: opening writes
    /// nothing; forms and drafts survive navigation; each page owns its
    /// Undo, also after reopening, and links to the other's; a pending
    /// recovery holds both; unrelated drafts stay.
    #[test]
    fn floating_apps_and_shortcuts_own_undo_and_keep_drafts() {
        if desktop_test_child(
            "floating_apps_and_shortcuts_own_undo_and_keep_drafts",
            DesktopEnv {
                app_config: true,
                fixture: true,
                ..Default::default()
            },
        ) {
            return;
        }
        use crate::desktop::shortcuts::NAMESPACE as KEYS;
        use crate::desktop::window_rules::NAMESPACE as RULES;
        use shortcuts::Message as Keys;
        use window_rules::Message as Rules;
        let root = isolated_root();
        let fixture = root.join("cosmic-root");
        let live = live_tree(&root);
        std::fs::create_dir_all(fixture.join("applications")).unwrap();
        std::fs::write(
            fixture.join("applications/org.example.Notes.desktop"),
            "[Desktop Entry]\nType=Application\nName=Notes\nExec=sentinel-never-run\n",
        )
        .unwrap();
        seed(
            &fixture.join("system/cosmic"),
            &[(
                KEYS,
                "defaults",
                r#"{(modifiers: [Super], key: "t"): System(Terminal)}"#,
            )],
        );
        let (mut app, _) = Settings::init(Core::default(), Page::Desktop);
        let files = fixture_files(&fixture);
        let _ = app.update(Message::Page(Page::DesktopWindowRules));
        let _ = app.update(Message::Page(Page::DesktopShortcuts));
        assert_eq!(
            fixture_files(&fixture),
            files,
            "opening pages writes nothing"
        );
        assert_eq!(
            Page::parse("desktop-window-rules"),
            Page::DesktopWindowRules
        );
        assert_eq!(Page::parse("desktop-shortcuts"), Page::DesktopShortcuts);
        assert_eq!(Page::DesktopShortcuts.nav(), Page::DesktopShortcuts);

        // An unrelated Workspaces draft stays through everything below.
        let _ = app.update(Message::Page(Page::DesktopWorkspaces));
        let _ = app.update(Message::Workspaces(workspaces::Message::Wraparound(false)));

        // A half-typed form survives a visit elsewhere and holds Apply back.
        let _ = app.update(Message::Page(Page::DesktopWindowRules));
        let _ = app.update(Message::WindowRules(Rules::Add));
        let _ = app.update(Message::WindowRules(Rules::Picker(true)));
        let _ = app.update(Message::WindowRules(Rules::Pick(
            "org.example.Notes".into(),
        )));
        let _ = app.update(Message::Page(Page::DesktopShortcuts));
        let _ = app.update(Message::Page(Page::DesktopWindowRules));
        let form = app.window_rules.form.clone().expect("form kept");
        assert_eq!(form.app_id, "org.example.Notes");
        assert!(!app.window_rules.can_apply());
        let _ = app.update(Message::WindowRules(Rules::Save));
        let _ = app.update(Message::Page(Page::DesktopWorkspaces));
        let _ = app.update(Message::Page(Page::DesktopWindowRules));
        assert!(!app.window_rules.dirty(), "draft kept across pages");
        let _ = app.update(Message::WindowRules(Rules::Apply));
        assert!(
            fixture_user(&root, RULES, "tiling_exception_custom")
                .unwrap()
                .contains("Notes")
        );
        assert!(
            !app.workspaces.dirty(),
            "the unrelated Workspaces draft stays"
        );

        // Reopened after a restart, Floating apps still owns the Undo.
        let (mut app, _) = Settings::init(Core::default(), Page::DesktopWindowRules);
        assert!(
            app.window_rules.can_undo(),
            "{:?}",
            app.window_rules.notice()
        );
        let _ = app.update(Message::WindowRules(Rules::Undo));
        assert_eq!(fixture_user(&root, RULES, "tiling_exception_custom"), None);

        // Shortcuts makes the next change; Floating apps links there.
        let _ = app.update(Message::Page(Page::DesktopShortcuts));
        let _ = app.update(Message::Shortcuts(Keys::Add));
        let _ = app.update(Message::Shortcuts(Keys::Name("Notes".into())));
        let _ = app.update(Message::Shortcuts(Keys::Combo("Super+Alt+N".into())));
        let _ = app.update(Message::Shortcuts(Keys::Command("sentinel-notes".into())));
        let _ = app.update(Message::Page(Page::DesktopWindowRules));
        let _ = app.update(Message::Page(Page::DesktopShortcuts));
        assert!(app.shortcuts.form.is_some(), "form kept");
        let _ = app.update(Message::Shortcuts(Keys::Save));
        let _ = app.update(Message::Shortcuts(Keys::Apply));
        assert!(app.shortcuts.can_undo(), "{:?}", app.shortcuts.notice());
        let _ = app.update(Message::Page(Page::DesktopWindowRules));
        assert!(!app.window_rules.can_undo());
        let (owner, _) = app.window_rules.elsewhere().unwrap().clone();
        assert_eq!(recovery::owner_page(owner), Page::DesktopShortcuts);
        let (mut app, _) = Settings::init(Core::default(), Page::DesktopShortcuts);
        assert!(app.shortcuts.can_undo(), "Undo after reopening");

        // A pending recovery holds both pages.
        std::fs::write(
            fixture.join("state/desktop-journal.json"),
            r#"{"format":"desktop-cards-journal","version":1,"id":"x","operation":"apply","created":"","entries":[]}"#,
        )
        .unwrap();
        let _ = app.update(Message::Shortcuts(Keys::Refresh));
        assert!(app.shortcuts.recovery().is_some() && !app.shortcuts.can_undo());
        let _ = app.update(Message::Page(Page::DesktopWindowRules));
        let _ = app.update(Message::WindowRules(Rules::Add));
        let _ = app.update(Message::WindowRules(Rules::AppId("x.y".into())));
        let _ = app.update(Message::WindowRules(Rules::Save));
        assert!(!app.window_rules.dirty() && !app.window_rules.can_apply());
        let _ = app.update(Message::WindowRules(Rules::KeepRecovery));
        let _ = app.update(Message::Page(Page::DesktopShortcuts));
        let _ = app.update(Message::Shortcuts(Keys::Undo));
        assert_eq!(
            fixture_user(&root, KEYS, "custom"),
            None,
            "absence restored"
        );
        assert!(!root.join("sentinel-notes").exists());
        assert_eq!(live_tree(&root), live, "the live desktop is never touched");
    }

    /// Non-overlapping drafts survive other pages' Apply; Window gaps and
    /// System appearance edits of the same theme hold back each other's
    /// Apply and Undo with a way to the other page; Profiles count gap
    /// drafts as Appearance and wait for Windows and Workspaces drafts,
    /// including Undo with no profile selected.
    #[test]
    fn drafts_survive_or_explain_overlap() {
        if desktop_test_child(
            "drafts_survive_or_explain_overlap",
            DesktopEnv {
                app_config: true,
                fixture: true,
                ..Default::default()
            },
        ) {
            return;
        }
        use crate::desktop::appearance::{ColorRole, ThemeVariant};
        use system_appearance::Message as Look;
        let (mut app, _) = Settings::init(Core::default(), Page::DesktopAppearance);
        send_settings(
            &mut app,
            Message::SystemAppearance(Look::HexInput(ColorRole::Accent, "#123456".into())),
        );
        send_settings(&mut app, Message::Page(Page::DesktopWindows));
        send_settings(
            &mut app,
            Message::Windows(windows::Message::Flag(
                crate::desktop::windows::Behavior::FocusFollowsCursor,
                true,
            )),
        );
        assert!(!app.windows.dirty());
        send_settings(&mut app, Message::Page(Page::DesktopAppearance));
        assert_eq!(
            app.system_appearance.hex_text(ColorRole::Accent),
            Some("#123456")
        );
        send_settings(
            &mut app,
            Message::SystemAppearance(Look::HexSubmit(ColorRole::Accent)),
        );
        assert!(!app.system_appearance.dirty());
        let color = app
            .system_appearance
            .draft()
            .unwrap()
            .color(ThemeVariant::Dark, ColorRole::Accent);
        send_settings(&mut app, Message::Page(Page::DesktopDock));
        send_settings(
            &mut app,
            Message::DesktopEditor(desktop::Message::Opacity(78)),
        );
        send_settings(&mut app, Message::DesktopEditor(desktop::Message::Apply));
        send_settings(&mut app, Message::Page(Page::Themes));
        send_settings(
            &mut app,
            Message::Profiles(profiles::Message::Select(profiles::Selection::Preset(
                "theme-outline",
            ))),
        );
        for job in app.profiles.take_theme_jobs() {
            send_settings(&mut app, Message::Profiles(job.run()));
        }
        assert!(app.profile_waits().is_empty());
        send_settings(&mut app, Message::Profiles(profiles::Message::Apply));
        assert!(matches!(
            app.profiles.selection(),
            profiles::Selection::Current
        ));
        assert_eq!(
            app.system_appearance
                .draft()
                .unwrap()
                .color(ThemeVariant::Dark, ColorRole::Accent),
            color,
            "explicit native accent survives a theme"
        );
        assert_eq!(raw_preferences()["future_field"], 42);
    }

    #[test]
    fn rain_native_settings_preview_commit_undo_and_clipping() {
        if desktop_test_child(
            "rain_native_settings_preview_commit_undo_and_clipping",
            DesktopEnv {
                app_config: true,
                fixture: true,
                ..Default::default()
            },
        ) {
            return;
        }
        use crate::desktop::wallpaper::{BgEntry, BgSource, WallpaperDraft, shared_key};
        let store = crate::desktop::DesktopStore::from_env().unwrap();
        let path = store.user_path(&shared_key());
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(
            path,
            ron::to_string(&BgEntry::new(
                "all",
                BgSource::Path(
                    concat!(
                        env!("CARGO_MANIFEST_DIR"),
                        "/assets/looks/evening-wallpaper.jpg"
                    )
                    .into(),
                ),
            ))
            .unwrap(),
        )
        .unwrap();
        let paper_before = store.load(&WallpaperDraft::keys()).unwrap();
        let raw_before = raw_preferences();
        for size in [Size::new(820., 600.), Size::new(1280., 800.)] {
            let (mut app, initial) = Settings::init(Core::default(), Page::DesktopWallpaper);
            settle_settings(&mut app, initial);
            drive_wallpaper(&mut app);
            send_settings(&mut app, Message::Resized(size));
            app.rain.lifecycle = crate::rain::lifecycle::State {
                ready: true,
                ..Default::default()
            };
            let mut native = crate::native_test::Native::with_backend(size, "wgpu");
            native_click(&mut app, &mut native, "rain-try");
            assert!(app.rain.preview);
            assert!(!Config::load().rain.enabled);
            let mut p = app.config.rain.clone();
            p.enabled = true;
            p.mode = crate::rain::Mode::Hybrid;
            send_settings(&mut app, Message::Rain(rain::Edit::Set(p.clone())));
            assert_eq!(Config::load().rain, p);
            native.scroll(app.settings_view(), "settings-content", -10000.);
            native.capture(
                app.settings_view(),
                &controls_theme(&app.config),
                &format!("rain-settings-{}", size.width),
            );
            // Scrolling the preview partly offscreen must keep GPU viewport/UVs valid.
            native.scroll(app.settings_view(), "settings-content", 170.);
            native.capture(
                app.settings_view(),
                &controls_theme(&app.config),
                &format!("rain-scrolled-{}", size.width),
            );
            native_reveal(&mut app, &mut native, "rain-trails");
            native.capture(
                app.settings_view(),
                &controls_theme(&app.config),
                &format!("rain-trails-{}", size.width),
            );
            native_click(&mut app, &mut native, "rain-adjust");
            native_reveal(&mut app, &mut native, "rain-size");
            native.capture(
                app.settings_view(),
                &controls_theme(&app.config),
                &format!("rain-adjust-{}", size.width),
            );
            p.intensity = 82;
            p.trails = 0;
            send_settings(&mut app, Message::Rain(rain::Edit::Draft(p.clone())));
            assert_ne!(Config::load().rain.intensity, 82);
            assert_ne!(Config::load().rain.trails, 0);
            send_settings(&mut app, Message::Rain(rain::Edit::Commit));
            assert_eq!(Config::load().rain.intensity, 82);
            assert_eq!(Config::load().rain.trails, 0);
            send_settings(&mut app, Message::UndoDesktop);
            assert_ne!(Config::load().rain.intensity, 82);
            assert_eq!(Config::load().rain.trails, 35);
            send_settings(&mut app, Message::UndoDesktop);
            assert!(!Config::load().rain.enabled);
            assert_eq!(
                raw_preferences().get("future_field"),
                raw_before.get("future_field")
            );
            assert_eq!(
                store.load(&WallpaperDraft::keys()).unwrap(),
                paper_before,
                "rain never replaces native wallpaper ownership"
            );
            for effect in [
                crate::rain::Effect::Snow,
                crate::rain::Effect::Fog,
                crate::rain::Effect::Clouds,
            ] {
                let before = Config::load().rain;
                let mut next = before.clone();
                next.enabled = true;
                next.effect = effect;
                send_settings(&mut app, Message::Rain(rain::Edit::Set(next.clone())));
                app.rain.lifecycle = crate::rain::lifecycle::State {
                    ready: true,
                    ..Default::default()
                };
                native_reveal(
                    &mut app,
                    &mut native,
                    if effect == crate::rain::Effect::Fog {
                        "rain-opacity"
                    } else {
                        "rain-intensity"
                    },
                );
                native.capture(
                    app.settings_view(),
                    &controls_theme(&app.config),
                    &format!("effect-{effect:?}-settings-{}", size.width),
                );
                native_click(
                    &mut app,
                    &mut native,
                    if effect != crate::rain::Effect::Snow {
                        "rain-opacity"
                    } else {
                        "rain-size"
                    },
                );
                assert_ne!(Config::load().rain, next);
                assert_eq!(
                    Config::load().rain.intensity,
                    before.intensity,
                    "snow does not overwrite rain settings"
                );
                send_settings(&mut app, Message::UndoDesktop);
                assert_eq!(Config::load().rain, next);
                send_settings(&mut app, Message::UndoDesktop);
                assert_eq!(Config::load().rain, before);
            }
            send_settings(&mut app, Message::Page(Page::Themes));
            assert!(!app.rain.preview);
        }
    }

    /// Runs the Wallpaper page's background work synchronously and hands
    /// the results to the editor, like the app's task runner does (`update`
    /// turns pending work into tasks a test can't run).
    fn drive_wallpaper(app: &mut Settings) {
        for _ in 0..32 {
            let jobs = app.wallpaper.take_jobs();
            if jobs.is_empty() {
                return;
            }
            for job in jobs {
                app.wallpaper.update(job.run());
            }
        }
    }

    #[test]
    fn automatic_colors_native_switch_hex_recommended_apply_and_undo_fit() {
        if desktop_test_child(
            "automatic_colors_native_switch_hex_recommended_apply_and_undo_fit",
            DesktopEnv {
                app_config: true,
                fixture: true,
                ..Default::default()
            },
        ) {
            return;
        }
        use crate::desktop::wallpaper::{BgColor, BgEntry, BgSource, WallpaperDraft, shared_key};
        use crate::native_test::Native;
        use wallpaper::{Message as Paper, Pick};
        let original = raw_preferences();
        let store = crate::desktop::DesktopStore::from_env().unwrap();
        let path = store.user_path(&shared_key());
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        for size in [Size::new(820., 600.), Size::new(1024., 720.)] {
            std::fs::write(Config::path().unwrap(), original.to_string()).unwrap();
            std::fs::write(
                &path,
                ron::to_string(&BgEntry::new(
                    "all",
                    BgSource::Color(BgColor::Single([0.2, 0.4, 0.6])),
                ))
                .unwrap(),
            )
            .unwrap();
            // Colors live on Colors & theme; the wallpaper preview on Wallpaper.
            let (mut app, initial) = Settings::init(Core::default(), Page::Themes);
            settle_settings(&mut app, initial);
            let _ = app.update(Message::Resized(size));
            let mut native = Native::new(size);
            native_click(&mut app, &mut native, "colors-automatic");
            assert!(Config::load().appearance.wallpaper_colors);
            assert_eq!(
                Config::load().appearance.accent,
                Some(recommended_color([0.2, 0.4, 0.6]))
            );
            let applied = raw_preferences();
            native_hex(&mut app, &mut native, "#D34762", false);
            assert_eq!(
                raw_preferences(),
                applied,
                "typing complete prefixes is local"
            );
            native_hex(&mut app, &mut native, "#D34762", true);
            assert_eq!(Config::load().appearance.accent, Some([211, 71, 98]));
            assert!(!Config::load().appearance.wallpaper_colors);
            native_click(&mut app, &mut native, "wallpaper-best-color");
            assert_eq!(
                Config::load().appearance.accent,
                Some(recommended_color([0.2, 0.4, 0.6]))
            );
            native_click(&mut app, &mut native, "colors-automatic");
            let before = raw_preferences();
            let background = store.load(&WallpaperDraft::keys()).unwrap();
            send_settings(&mut app, Message::Page(Page::DesktopWallpaper));
            send_settings(
                &mut app,
                Message::Wallpaper(Paper::Select(Pick::Color(BgColor::Single([0.1, 0.7, 0.4])))),
            );
            assert_eq!(raw_preferences(), before);
            assert_eq!(store.load(&WallpaperDraft::keys()).unwrap(), background);
            native.capture(
                app.settings_view(),
                &controls_theme(&app.config),
                &format!("automatic-preview-{}", size.width),
            );
            native_click(&mut app, &mut native, "wallpaper-apply");
            assert_eq!(
                Config::load().appearance.accent,
                Some(recommended_color([0.1, 0.7, 0.4]))
            );
            native_click(&mut app, &mut native, "settings-undo");
            assert_eq!(raw_preferences(), before);
            assert_eq!(store.load(&WallpaperDraft::keys()).unwrap(), background);
        }
    }

    #[test]
    fn unified_colors_native_links_apply_undo_and_recovered_wallpaper() {
        if desktop_test_child(
            "unified_colors_native_links_apply_undo_and_recovered_wallpaper",
            DesktopEnv {
                app_config: true,
                fixture: true,
                ..Default::default()
            },
        ) {
            return;
        }
        use crate::native_test::Native;
        let root = isolated_root();
        let original = raw_preferences();
        for size in [Size::new(820., 600.), Size::new(1024., 720.)] {
            std::fs::write(Config::path().unwrap(), original.to_string()).unwrap();
            let (mut app, initial) = Settings::init(Core::default(), Page::Appearance);
            settle_settings(&mut app, initial);
            let _ = app.update(Message::Resized(size));
            // Open Colors & theme, then scroll to each real color control.
            send_settings(&mut app, Message::Page(Page::Themes));
            let mut native = Native::new(size);
            let wallpaper = fixture_user(
                &root.join("cosmic-root"),
                "com.system76.CosmicBackground",
                "all",
            );
            native_click(&mut app, &mut native, "colors-palettes");
            native_click(&mut app, &mut native, "colors-palette-rose");
            assert_eq!(Config::load().theme, "rose");
            assert!(!app.wallpaper.dirty());
            assert!(app.profile_waits().is_empty());
            assert_eq!(
                fixture_user(
                    &root.join("cosmic-root"),
                    "com.system76.CosmicBackground",
                    "all"
                ),
                wallpaper
            );
            native.capture(
                app.settings_view(),
                &controls_theme(&app.config),
                &format!("colors-direct-{}", size.width),
            );
            native_click(&mut app, &mut native, "settings-undo");
            assert_eq!(raw_preferences(), original);
            send_settings(&mut app, Message::Page(Page::Themes));
            native = Native::new(size);
            app.on_app_exit();
            drop(app);
            let (mut app, initial) = Settings::init(Core::default(), Page::DesktopWallpaper);
            settle_settings(&mut app, initial);
            assert!(!crate::desktop::wallpaper::automatic::paused(
                &crate::desktop::DesktopStore::from_env().unwrap()
            ));
            assert_eq!(raw_preferences(), original);
            send_settings(
                &mut app,
                Message::Wallpaper(wallpaper::Message::ClearAccent),
            );
        }
    }

    #[test]
    fn wallpaper_page_applies_backgrounds_without_color_controls() {
        if desktop_test_child(
            "wallpaper_page_applies_backgrounds_without_color_controls",
            DesktopEnv {
                app_config: true,
                fixture: true,
                ..Default::default()
            },
        ) {
            return;
        }
        use crate::desktop::wallpaper::{BgColor, BgGradient, srgb_to_linear};
        use crate::native_test::Native;
        use cosmic::iced::{Event, mouse};
        use wallpaper::{Message as Paper, Pick};
        fn click(app: &mut Settings, native: &mut Native, id: &str) {
            let bounds = native_reveal(app, native, id);
            assert!(
                bounds.x >= 0.
                    && bounds.y >= 0.
                    && bounds.x + bounds.width <= native.size.width
                    && bounds.y + bounds.height <= native.size.height,
                "visible control {id}: {bounds:?} in {:?}",
                native.size
            );
            for event in [
                mouse::Event::CursorMoved {
                    position: bounds.center(),
                },
                mouse::Event::ButtonPressed(mouse::Button::Left),
                mouse::Event::ButtonReleased(mouse::Button::Left),
            ] {
                for message in native
                    .event(app.settings_view(), Event::Mouse(event), bounds.center())
                    .0
                {
                    let _ = app.update(message);
                }
            }
        }
        let root = isolated_root();
        let fixture = root.join("cosmic-root");
        for size in [Size::new(820., 600.), Size::new(1024., 720.)] {
            let (mut app, _) = Settings::init(Core::default(), Page::DesktopWallpaper);
            let _ = app.update(Message::Resized(size));
            let before = fixture_files(&fixture);
            app.wallpaper
                .update(Paper::Select(Pick::Color(BgColor::Gradient(BgGradient {
                    colors: ["#336699", "#EE9933"]
                        .map(|text| wallpaper::parse_rgb(text).unwrap().map(srgb_to_linear))
                        .to_vec(),
                    radius: 90.,
                }))));
            drive_wallpaper(&mut app);
            let mut native = Native::new(size);
            let choose = native.bounds(app.settings_view(), "wallpaper-choose-picture");
            assert!(
                choose.y + choose.height <= size.height && choose.x + choose.width <= size.width,
                "picture chooser is visible at {size:?}"
            );
            assert!(
                native
                    .try_bounds(app.settings_view(), "colors-palettes")
                    .is_none(),
                "colors live on Colors & theme, not beside the wallpapers"
            );
            native.capture(
                app.settings_view(),
                &controls_theme(&app.config),
                &format!("wallpaper-page-{}", size.width),
            );
            click(&mut app, &mut native, "wallpaper-apply");
            assert!(!app.wallpaper.dirty());
            let applied = fixture_files(&fixture);
            assert_ne!(applied, before, "the wallpaper applies on its own");
            // Its Undo is offered right here, on the page that made the change.
            let baseline = fixture.join("state/desktop-recovery/original/manifest.json");
            let captured = std::fs::read(&baseline).unwrap();
            click(&mut app, &mut native, "settings-undo");
            assert_eq!(
                std::fs::read(&baseline).unwrap(),
                captured,
                "Undo never rotates the immutable baseline"
            );
            let ledger = fixture.join("state/desktop-recovery/created-native-keys.json");
            let created: Vec<crate::desktop::ConfigKey> =
                serde_json::from_slice(&std::fs::read(&ledger).unwrap()).unwrap();
            assert_eq!(
                created,
                vec![
                    crate::desktop::ConfigKey::new(crate::desktop::wallpaper::NAMESPACE, 1, "all")
                        .unwrap()
                ]
            );
            assert_eq!(
                std::fs::read(&ledger).unwrap(),
                applied[&ledger],
                "Undo retains exact creation evidence"
            );
            let mut restored = fixture_files(&fixture);
            if !before.contains_key(&ledger) {
                restored.remove(&ledger);
            }
            if !before.contains_key(&baseline) {
                restored.remove(&baseline);
            }
            assert_eq!(
                restored, before,
                "Undo restores the wallpaper and retains first-write evidence"
            );
            // Colors are chosen on Colors & theme; that flow is covered by
            // unified_colors_native_links_apply_undo_and_recovered_wallpaper.
        }
    }

    #[test]
    fn wallpaper_and_outline_apply_independently() {
        if desktop_test_child(
            "wallpaper_and_outline_apply_independently",
            DesktopEnv {
                app_config: true,
                fixture: true,
                ..Default::default()
            },
        ) {
            return;
        }
        use crate::desktop::appearance::{AppearanceDraft, ColorRole, ThemeVariant, parse_hex};
        use crate::desktop::wallpaper::{BgColor, WallpaperDraft};
        use system_appearance::Message as Look;
        use wallpaper::{Message as Paper, Pick};
        let (mut app, initial) = Settings::init(Core::default(), Page::DesktopAppearance);
        settle_settings(&mut app, initial);
        let orange = parse_hex("#FFAD00", false);
        send_settings(
            &mut app,
            Message::SystemAppearance(Look::Color(ColorRole::Outline, orange)),
        );
        send_settings(&mut app, Message::Page(Page::DesktopWallpaper));
        send_settings(
            &mut app,
            Message::Wallpaper(Paper::Select(Pick::Color(BgColor::Single([0.3, 0.4, 0.6])))),
        );
        send_settings(&mut app, Message::Wallpaper(Paper::AutomaticColors(true)));
        send_settings(&mut app, Message::Wallpaper(Paper::Apply));
        assert!(!app.wallpaper.dirty(), "{:?}", app.wallpaper.notice());
        assert!(app.config.appearance.wallpaper_colors);
        let store = crate::desktop::DesktopStore::from_env().unwrap();
        assert_eq!(
            AppearanceDraft::load(&store)
                .unwrap()
                .color(ThemeVariant::Dark, ColorRole::Outline)
                .known(),
            Some(&orange)
        );
        let applied = store.load(&WallpaperDraft::keys()).unwrap();
        send_settings(
            &mut app,
            Message::Wallpaper(Paper::Select(Pick::Color(BgColor::Single([0.6, 0.3, 0.5])))),
        );
        send_settings(&mut app, Message::Page(Page::DesktopAppearance));
        assert!(
            !app.wallpaper.background_dirty(),
            "leaving cancels a local preview"
        );
        let green = parse_hex("#12AB88", false);
        send_settings(
            &mut app,
            Message::SystemAppearance(Look::Color(ColorRole::Outline, green)),
        );
        send_settings(
            &mut app,
            Message::SystemAppearance(Look::HexInput(ColorRole::Accent, "#12".into())),
        );
        send_settings(&mut app, Message::Page(Page::DesktopWallpaper));
        assert_eq!(store.load(&WallpaperDraft::keys()).unwrap(), applied);
        send_settings(&mut app, Message::UndoDesktop);
        assert!(!app.error, "{}", app.status);
        assert_eq!(
            AppearanceDraft::load(&store)
                .unwrap()
                .color(ThemeVariant::Dark, ColorRole::Outline)
                .known(),
            Some(&orange)
        );
        assert_eq!(
            app.system_appearance.hex_text(ColorRole::Accent),
            Some("#12")
        );
        assert_eq!(store.load(&WallpaperDraft::keys()).unwrap(), applied);
    }

    #[test]
    fn automatic_colors_keep_independent_saved_edits_and_protect_accent_input() {
        if desktop_test_child(
            "automatic_colors_keep_independent_saved_edits_and_protect_accent_input",
            DesktopEnv {
                app_config: true,
                fixture: true,
                ..Default::default()
            },
        ) {
            return;
        }
        use crate::desktop::appearance::{ColorRole, parse_hex};
        use crate::desktop::wallpaper::{BgColor, BgEntry, BgSource, automatic, shared_key};
        use system_appearance::Message as Look;
        use wallpaper::Message as Paper;
        let (mut app, _) = Settings::init(Core::default(), Page::DesktopWallpaper);
        let store = crate::desktop::DesktopStore::from_env().unwrap();
        let path = store.user_path(&shared_key());
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(
            &path,
            ron::to_string(&BgEntry::new(
                "all",
                BgSource::Color(BgColor::Single([0.2, 0.4, 0.6])),
            ))
            .unwrap(),
        )
        .unwrap();
        send_settings(&mut app, Message::Wallpaper(Paper::Refresh));
        send_settings(&mut app, Message::Wallpaper(Paper::AutomaticColors(true)));
        drive_wallpaper(&mut app);
        assert!(Config::load().appearance.wallpaper_colors);
        send_settings(&mut app, Message::Page(Page::DesktopAppearance));
        send_settings(
            &mut app,
            Message::SystemAppearance(Look::Color(ColorRole::Outline, parse_hex("#FFAD00", false))),
        );
        send_settings(
            &mut app,
            Message::SystemAppearance(Look::HexInput(ColorRole::Accent, "#12".into())),
        );
        app.on_app_exit();
        assert!(!automatic::paused(&store));
        std::fs::write(
            store.state_file("settings-draft-appearance.json").unwrap(),
            "legacy malformed record",
        )
        .unwrap();
        std::fs::write(
            &path,
            ron::to_string(&BgEntry::new(
                "all",
                BgSource::Color(BgColor::Single([0.3, 0.7, 0.4])),
            ))
            .unwrap(),
        )
        .unwrap();
        let action = store.latest_change().unwrap().unwrap();
        automatic::commit(&store, automatic::prepare(&store).unwrap().unwrap()).unwrap();
        assert_eq!(store.latest_change().unwrap().unwrap().id, action.id);
        send_settings(&mut app, Message::SystemAppearance(Look::Refresh));
        assert_eq!(
            app.system_appearance.hex_text(ColorRole::Accent),
            Some("#12")
        );
        assert!(!automatic::paused(&store));
        assert_eq!(raw_preferences()["future_field"], 42);
    }

    #[test]
    fn wallpaper_accent_waits_for_theme_drafts_and_blocks_overlapping_profiles() {
        if desktop_test_child(
            "wallpaper_accent_waits_for_theme_drafts_and_blocks_overlapping_profiles",
            DesktopEnv {
                app_config: true,
                fixture: true,
                ..Default::default()
            },
        ) {
            return;
        }
        use crate::desktop::appearance::{ColorRole, parse_hex};
        use system_appearance::Message as Look;
        use wallpaper::{Message as Paper, Pick};
        let (mut app, _) = Settings::init(Core::default(), Page::DesktopAppearance);
        let _ = app.update(Message::SystemAppearance(Look::HexInput(
            ColorRole::Accent,
            "#12".into(),
        )));
        let _ = app.update(Message::SystemAppearance(Look::Color(
            ColorRole::Background,
            parse_hex("#222222", true),
        )));
        let _ = app.update(Message::Page(Page::DesktopWallpaper));
        drive_wallpaper(&mut app);
        app.wallpaper.update(Paper::Select(Pick::Color(
            crate::desktop::wallpaper::BgColor::Single([0.2, 0.4, 0.6]),
        )));
        drive_wallpaper(&mut app);
        let _ = app.update(Message::Wallpaper(Paper::Apply));
        assert!(!app.wallpaper.dirty(), "{:?}", app.wallpaper.notice());
        assert_eq!(
            app.system_appearance.hex_text(ColorRole::Accent),
            Some("#12")
        );
        assert!(app.theme_waits(Page::DesktopWallpaper).is_none());
        assert!(app.profile_waits().is_empty());
        let store = crate::desktop::DesktopStore::from_env().unwrap();
        assert!(!crate::desktop::wallpaper::automatic::paused(&store));
    }

    /// The two new subpages route by key, keep their drafts through
    /// navigation and mark the overview and the navigation as pending;
    /// Accent selection stays on Wallpaper with both changes drafted.
    #[test]
    fn subpages_route_and_mark_pending_on_overview() {
        if desktop_test_child(
            "subpages_route_and_mark_pending_on_overview",
            DesktopEnv {
                app_config: true,
                fixture: true,
                ..Default::default()
            },
        ) {
            return;
        }
        use wallpaper::{Message as Paper, Pick};
        assert_eq!(Page::DesktopWallpaper.nav(), Page::DesktopWallpaper);
        assert_eq!(Page::DesktopIcons.nav(), Page::DesktopIcons);
        let (mut app, _) = Settings::init(Core::default(), Page::DesktopWallpaper);
        drive_wallpaper(&mut app);
        let store = crate::desktop::DesktopStore::from_env().unwrap();
        let before = store
            .load(&crate::desktop::wallpaper::WallpaperDraft::keys())
            .unwrap();
        app.wallpaper.update(Paper::Select(Pick::Color(
            crate::desktop::wallpaper::BgColor::Single([0.2, 0.4, 0.6]),
        )));
        drive_wallpaper(&mut app);
        assert!(app.wallpaper.dirty());
        let _ = app.update(Message::Page(Page::DesktopIcons));
        assert!(!app.wallpaper.dirty(), "leaving cancels the local chooser");
        assert_eq!(
            store
                .load(&crate::desktop::wallpaper::WallpaperDraft::keys())
                .unwrap(),
            before
        );
        let _ = app.update(Message::DesktopIcons(desktop_icons::Message::Size(150)));
        let _ = app.update(Message::DesktopIcons(desktop_icons::Message::Apply));
        assert!(!app.desktop_icons.dirty());
        let _ = app.update(Message::Page(Page::Overview));
        let _ = app.update(Message::UndoDesktop);
        assert!(!app.error, "{}", app.status);
    }

    /// A Wallpaper or Desktop icons draft holds back a profile Apply or Undo
    /// touching that part, also unfinished colour text that is not a
    /// native edit yet; other parts stay usable and no draft is lost.
    #[test]
    fn profile_waits_for_dirty_wallpaper_and_icons_drafts() {
        if desktop_test_child(
            "profile_waits_for_dirty_wallpaper_and_icons_drafts",
            DesktopEnv {
                app_config: true,
                fixture: true,
                ..Default::default()
            },
        ) {
            return;
        }
        use crate::desktop::profiles::{
            BackgroundLook, DesktopIconsLook, Profile, ProfileLibrary, Rgb, SourceLook,
            WallpaperLook,
        };
        use profiles::{Message as Look, Selection};
        use wallpaper::Message as Paper;
        let store = crate::desktop::DesktopStore::from_env().unwrap();
        let mut profile = Profile::new("Blue desk").unwrap();
        profile.wallpaper = Some(WallpaperLook {
            background: Some(BackgroundLook {
                source: Some(SourceLook::Color(Rgb::Hex("#336699".into()))),
                ..Default::default()
            }),
            ..Default::default()
        });
        profile.desktop_icons = Some(DesktopIconsLook {
            show_trash: Some(true),
            ..Default::default()
        });
        let id = ProfileLibrary::new(&store).create(&profile).unwrap();
        let (mut app, initial) = Settings::init(Core::default(), Page::DesktopWallpaper);
        settle_settings(&mut app, initial);
        send_settings(
            &mut app,
            Message::Wallpaper(Paper::AccentText("#12".into())),
        );
        send_settings(&mut app, Message::Page(Page::DesktopIcons));
        send_settings(
            &mut app,
            Message::DesktopIcons(desktop_icons::Message::Size(150)),
        );
        send_settings(
            &mut app,
            Message::DesktopIcons(desktop_icons::Message::Apply),
        );
        assert!(!app.desktop_icons.dirty());
        send_settings(&mut app, Message::Page(Page::DesktopProfiles));
        send_settings(
            &mut app,
            Message::Profiles(Look::Select(Selection::Saved(id))),
        );
        assert!(app.profile_waits().is_empty());
        send_settings(&mut app, Message::Profiles(Look::Apply));
        assert!(matches!(app.profiles.selection(), Selection::Current));
        assert_eq!(app.wallpaper.color_text(&app.config), "#12");
        assert!(!crate::desktop::wallpaper::automatic::paused(&store));
        send_settings(&mut app, Message::UndoDesktop);
        assert!(!app.error, "{}", app.status);
        assert_eq!(app.wallpaper.color_text(&app.config), "#12");
        assert_eq!(raw_preferences()["future_field"], 42);
    }

    /// With matching on, the page's note offers Stop matching, which saves
    /// the existing preference and never runs the controller.
    #[test]
    fn matching_note_and_stop_matching_use_existing_preference_path() {
        if desktop_test_child(
            "matching_note_and_stop_matching_use_existing_preference_path",
            DesktopEnv {
                app_config: true,
                ..Default::default()
            },
        ) {
            return;
        }
        let root = isolated_root();
        let live = live_tree(&root);
        let (mut app, _) = Settings::init(Core::default(), Page::DesktopWallpaper);
        assert!(app.config.appearance.match_wallpaper, "the note shows");
        let _ = app.update(Message::Wallpaper(wallpaper::Message::StopMatching));
        assert!(!app.busy, "no controller run");
        assert!(!app.error, "{}", app.status);
        assert!(!app.config.appearance.match_wallpaper);
        let saved = raw_preferences();
        assert_eq!(saved["appearance"]["match_wallpaper"], json!(false));
        assert_eq!(saved["future_field"], json!(42), "other preferences kept");
        assert_eq!(live_tree(&root), live, "no desktop setting changed");
    }

    /// Capture commands for the two pages edit drafts in preview, but every
    /// writing action is ignored without a fixture.
    #[test]
    fn capture_actions_cannot_write_without_fixture() {
        for (message, writes) in [
            (wallpaper::Message::Apply, true),
            (wallpaper::Message::Undo, true),
            (wallpaper::Message::Recover, true),
            (wallpaper::Message::KeepRecovery, true),
            (wallpaper::Message::Reinstall, true),
            (wallpaper::Message::Remove(PathBuf::from("/a.png")), true),
            (wallpaper::Message::Tab(wallpaper::Tab::Colors), false),
            (wallpaper::Message::SuggestAccent, false),
            (wallpaper::Message::UseShared, false),
        ] {
            assert_eq!(message.writes(), writes, "{message:?}");
            assert_eq!(capture_allows(message.writes(), false), !writes);
        }
        for name in ["apply", "undo", "recover", "keep"] {
            assert!(capture_wallpaper(&json!(name)).unwrap().writes());
            assert!(capture_desktop_icons(&json!(name)).unwrap().writes());
        }
        if desktop_test_child(
            "capture_actions_cannot_write_without_fixture",
            DesktopEnv {
                app_config: true,
                capture: true,
                ..Default::default()
            },
        ) {
            return;
        }
        let root = isolated_root();
        let live = live_tree(&root);
        let mut app = open_isolated_editor(&root);
        send_command(
            &mut app,
            json!({"page":"desktop-wallpaper","wallpaper":[{"tab":"colors"},{"select":{"color":"#336699"}},{"same":false},{"display":"DP-1"},{"select":{"gradient":["#FF8800","#3366CC"],"angle":90}},"apply","undo","recover","keep"]}),
        );
        assert_eq!(app.page, Page::DesktopWallpaper);
        assert!(app.wallpaper.dirty(), "draft edits still preview");
        assert!(!app.wallpaper.writable());
        assert_eq!(app.wallpaper.notice(), None, "ignored before the store");
        send_command(
            &mut app,
            json!({"page":"desktop-icons","desktop_icons":[{"size":150},{"show":["trash",true]},"apply","undo","recover","keep"]}),
        );
        assert_eq!(app.page, Page::DesktopIcons);
        assert_eq!(app.desktop_icons.dirty(), app.desktop_icons.available());
        assert_eq!(app.desktop_icons.notice(), None);
        assert_eq!(live_tree(&root), live);
        assert!(!root.join("home/.local/share/cosmic-desktop-cards").exists());
    }

    /// After a controller command (Reinstall) the Wallpaper page rereads
    /// its tiles and the media work the refresh started is handed to the
    /// runtime, not left waiting for the next Wallpaper message.
    #[test]
    fn command_done_refreshes_wallpaper_tiles_and_dispatches_media() {
        if desktop_test_child(
            "command_done_refreshes_wallpaper_tiles_and_dispatches_media",
            DesktopEnv {
                app_config: true,
                fixture: true,
                ..Default::default()
            },
        ) {
            return;
        }
        let root = isolated_root();
        let fixture = root.join("cosmic-root");
        std::fs::write(fixture.join("outputs"), "DP-1\n").unwrap();
        let png = |path: &std::path::Path| {
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            image::RgbImage::from_pixel(8, 8, image::Rgb([30, 90, 160]))
                .save(path)
                .unwrap();
        };
        let (mut app, _) = Settings::init(Core::default(), Page::Desktop);
        let _ = app.update(Message::Page(Page::DesktopWallpaper));
        drive_wallpaper(&mut app);
        let later = fixture.join("home/later.png");
        let shared = app
            .wallpaper
            .store()
            .unwrap()
            .user_path(&crate::desktop::wallpaper::shared_key());
        std::fs::create_dir_all(shared.parent().unwrap()).unwrap();
        std::fs::write(
            &shared,
            format!(
                "(output: \"all\", source: Path(\"{}\"), filter_by_theme: false, rotation_frequency: 900)",
                later.display()
            ),
        )
        .unwrap();
        let _ = app.update(Message::Wallpaper(wallpaper::Message::Refresh));
        drive_wallpaper(&mut app);
        assert!(app.wallpaper.tiles().iter().all(|tile| tile.missing));
        // The controller reinstalls the pictures; the shown one appears.
        let bundled = crate::desktop::wallpaper::bundled_dir(app.wallpaper.store().unwrap());
        for (name, _) in crate::desktop::wallpaper::BUNDLED {
            png(&crate::desktop::wallpaper::bundled_path(&bundled, name));
        }
        png(&later);
        let _ = app.update(Message::CommandDone(
            "Bundled wallpapers reinstalled".into(),
            Ok(()),
        ));
        assert!(
            app.wallpaper.tiles().iter().all(|tile| !tile.missing),
            "no stale Missing pills or banner"
        );
        assert!(app.wallpaper.take_jobs().is_empty(), "work dispatched");
        assert!(app.wallpaper.in_flight() > 0, "the new picture decodes");
        // A profile change (or Undo) elsewhere takes the same path.
        drive_wallpaper(&mut app);
        let second = fixture.join("home/second.png");
        png(&second);
        std::fs::write(
            &shared,
            format!(
                "(output: \"all\", source: Path(\"{}\"), filter_by_theme: false, rotation_frequency: 900)",
                second.display()
            ),
        )
        .unwrap();
        let _ = app.after_desktop_change();
        assert_eq!(
            app.wallpaper.shown().image.as_deref(),
            Some(second.as_path())
        );
        assert!(app.wallpaper.take_jobs().is_empty());
        assert!(app.wallpaper.in_flight() > 0);
    }

    #[test]
    fn motion_page_reads_only_until_toggled_and_lays_out_unavailable_and_on() {
        if desktop_test_child(
            "motion_page_reads_only_until_toggled_and_lays_out_unavailable_and_on",
            DesktopEnv {
                app_config: true,
                fixture: true,
                ..Default::default()
            },
        ) {
            return;
        }
        use crate::desktop::motion::{KEY, SCHEMA};
        use cosmic::iced::advanced::renderer::Headless;
        use cosmic::iced::runtime::user_interface::{Cache, UserInterface};
        assert_eq!(Page::parse("desktop-motion"), Page::DesktopMotion);
        assert_eq!(Page::DesktopMotion.nav(), Page::DesktopMotion);
        let _isolated = isolated_root();
        let root = PathBuf::from(std::env::var_os(crate::desktop::store::FIXTURE_ENV).unwrap());
        let mut renderer =
            cosmic::iced::futures::executor::block_on(<cosmic::Renderer as Headless>::new(
                cosmic::font::default(),
                cosmic::iced::Pixels(14.),
                Some("tiny-skia"),
            ))
            .expect("headless renderer");
        let size = Size::new(820., 600.);
        let (mut app, _) = Settings::init(Core::default(), Page::Desktop);
        let mut cache =
            UserInterface::build(app.settings_view(), size, Cache::default(), &mut renderer)
                .into_cache();
        // Not installed in this fixture: unavailable, and the toggle does nothing.
        let _ = app.update(Message::Page(Page::DesktopMotion));
        assert!(matches!(app.motion.current(), Some(Err(_))));
        assert_eq!(
            app.update(Message::Motion(motion::Message::Animations(false)))
                .units(),
            0
        );
        cache = UserInterface::build(app.settings_view(), size, cache, &mut renderer).into_cache();
        // Installed: reading creates nothing; the toggle starts one write.
        let dir = root.join("gsettings").join(SCHEMA);
        std::fs::create_dir_all(&dir).unwrap();
        let _ = app.update(Message::Page(Page::Desktop));
        let _ = app.update(Message::Page(Page::DesktopMotion));
        assert_eq!(app.motion.current(), Some(&Ok(true)));
        assert!(!dir.join(KEY).exists());
        cache = UserInterface::build(app.settings_view(), size, cache, &mut renderer).into_cache();
        assert!(
            app.update(Message::Motion(motion::Message::Animations(false)))
                .units()
                > 0
        );
        assert_eq!(app.motion.shown(), Some(false));
        // The finished write reports; elsewhere, the toggle is ignored.
        let _ = app.update(Message::Motion(motion::Message::Written(false, Ok(false))));
        assert_eq!(app.status, "Apps now reduce motion");
        let _ = app.update(Message::Page(Page::Glance));
        assert_eq!(
            app.update(Message::Motion(motion::Message::Animations(true)))
                .units(),
            0
        );
        // The sidebar slide is an ordinary preference; others are kept.
        let _ = app.update(Message::Page(Page::DesktopMotion));
        assert!(app.config.sidebar_slide);
        let _ = app.update(Message::SidebarSlide(false));
        let saved: serde_json::Value =
            serde_json::from_slice(&std::fs::read(Config::path().unwrap()).unwrap()).unwrap();
        assert_eq!(saved["sidebar_slide"], false);
        assert_eq!(saved["future_field"], 42);
        let _ = UserInterface::build(app.settings_view(), size, cache, &mut renderer);
        // Capture actions that write need the fixture; the slide never does.
        assert!(matches!(
            capture_motion(&json!({"animations": false})),
            Some((_, true))
        ));
        assert!(matches!(
            capture_motion(&json!({"slide": true})),
            Some((_, false))
        ));
        assert!(!capture_allows(true, false));
    }

    #[test]
    fn automation_page_edits_rules_lays_out_and_holds_only_real_drafts() {
        if desktop_test_child(
            "automation_page_edits_rules_lays_out_and_holds_only_real_drafts",
            DesktopEnv {
                app_config: true,
                fixture: true,
                ..Default::default()
            },
        ) {
            return;
        }
        use crate::automation::{self as auto, ProfileRef};
        use automation::{DraftEdit, Kind, Message as Edit};
        use cosmic::iced::advanced::renderer::Headless;
        use cosmic::iced::runtime::user_interface::{Cache, UserInterface};
        assert_eq!(Page::parse("desktop-automation"), Page::DesktopAutomation);
        assert_eq!(Page::DesktopAutomation.nav(), Page::DesktopProfiles);
        let isolated = isolated_root();
        let live = live_tree(&isolated);
        let root = PathBuf::from(std::env::var_os(crate::desktop::store::FIXTURE_ENV).unwrap());
        std::fs::write(root.join("outputs"), "eDP-1\nDP-2\n").unwrap();
        std::fs::write(root.join("output-names"), "DP-2=Dell U2720Q\n").unwrap();
        let (mut app, _) = Settings::init(Core::default(), Page::Desktop);
        // Navigation may remember its destination; no applied state is written.
        let _ = app.update(Message::Page(Page::DesktopAutomation));
        assert!(app.automation.opened() && app.automation.writable());
        assert!(destination_independent_state(&root.join("state")).is_empty());
        let mut renderer =
            cosmic::iced::futures::executor::block_on(<cosmic::Renderer as Headless>::new(
                cosmic::font::default(),
                cosmic::iced::Pixels(14.),
                Some("tiny-skia"),
            ))
            .expect("headless renderer");
        let size = Size::new(820., 600.);
        let mut cache =
            UserInterface::build(app.settings_view(), size, Cache::default(), &mut renderer)
                .into_cache();
        // A schedule through the editor; the draft marks the navigation.
        let _ = app.update(Message::Automation(Edit::New(Kind::Schedule)));
        assert!(app.desktop_pending());
        let _ = app.update(Message::Automation(Edit::Draft(DraftEdit::Time(
            "21:30".into(),
        ))));
        let _ = app.update(Message::Automation(Edit::Draft(DraftEdit::Profile(
            ProfileRef::Preset("presentation".into()),
        ))));
        cache = UserInterface::build(app.settings_view(), size, cache, &mut renderer).into_cache();
        // Drafts survive a visit elsewhere; editor messages there are ignored.
        let _ = app.update(Message::Page(Page::Glance));
        let _ = app.update(Message::Automation(Edit::Save));
        let _ = app.update(Message::Page(Page::DesktopAutomation));
        assert!(app.automation.dirty());
        let _ = app.update(Message::Automation(Edit::Save));
        let settings = auto::load(app.automation.store().unwrap()).unwrap();
        assert!(settings.enabled && settings.rules.len() == 1);
        // A display rule editor and the populated list both fit.
        let _ = app.update(Message::Automation(Edit::New(Kind::Display)));
        cache = UserInterface::build(app.settings_view(), size, cache, &mut renderer).into_cache();
        let _ = app.update(Message::Automation(Edit::Cancel));
        // Completed direct choices leave no runtime hold.
        let _ = app.update(Message::Page(Page::DesktopDock));
        let _ = app.update(Message::DesktopEditor(desktop::Message::Anchor(
            crate::desktop::panels::Anchor::Left,
        )));
        let store = crate::desktop::DesktopStore::fixture(&root).unwrap();
        assert!(auto::read_hold(&store).is_empty());
        let _ = app.update(Message::DesktopEditor(desktop::Message::Discard));
        assert!(auto::read_hold(&store).is_empty());
        let _ = app.update(Message::DesktopEditor(desktop::Message::Anchor(
            crate::desktop::panels::Anchor::Left,
        )));
        assert!(auto::read_hold(&store).is_empty());
        app.on_app_exit();
        assert!(auto::read_hold(&store).is_empty());
        // Status and history from the desktop's state lay out too.
        let _ = app.update(Message::Page(Page::DesktopAutomation));
        let _ = app.update(Message::Automation(Edit::Pause(automation::PauseFor::Hour)));
        let _ = UserInterface::build(app.settings_view(), size, cache, &mut renderer);
        let _ = app.update(Message::Page(Page::Desktop));
        let _ = UserInterface::build(app.settings_view(), size, Cache::default(), &mut renderer);
        // Only the fixture's state was written; the live desktop is untouched.
        assert_eq!(live_tree(&isolated), live);
        assert!(
            root.join("config/cosmic").exists(),
            "the dock action was committed in the fixture"
        );
    }

    /// The redesigned Wallpaper & colors and Calendar pages, in COSMIC light
    /// and dark at compact and wide sizes: a real bundled wallpaper, the
    /// sample calendar account, and primary controls reachable through the
    /// compact stacked and wide paired layouts.
    #[test]
    fn settings_redesign_wallpaper_and_calendar_layout_in_light_and_dark() {
        if desktop_test_child(
            "settings_redesign_wallpaper_and_calendar_layout_in_light_and_dark",
            DesktopEnv {
                app_config: true,
                fixture: true,
                ..Default::default()
            },
        ) {
            return;
        }
        use crate::desktop::wallpaper::{BgEntry, BgSource, bundled_dir, bundled_path};
        use crate::native_test::Native;
        use cosmic::iced::Rectangle;
        use cosmic::iced::futures::StreamExt;
        fn settle(app: &mut Settings, task: Task<Message>) {
            let runtime = tokio::runtime::Builder::new_current_thread()
                .enable_time()
                .build()
                .unwrap();
            runtime.block_on(async {
                let mut tasks = std::collections::VecDeque::from([task]);
                while let Some(task) = tasks.pop_front() {
                    if let Some(mut stream) = cosmic::iced::runtime::task::into_stream(task) {
                        while let Some(action) = stream.next().await {
                            if let cosmic::iced::runtime::Action::Output(cosmic::Action::App(
                                message,
                            )) = action
                            {
                                tasks.push_back(app.update(message));
                            }
                        }
                    }
                }
            });
        }
        let root = isolated_root();
        let fixture = root.join("cosmic-root");
        let preferences = raw_preferences();
        let (probe, _) = Settings::init(Core::default(), Page::DesktopWallpaper);
        let bundled = bundled_dir(probe.wallpaper.store().unwrap());
        drop(probe);
        let assets = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("assets/wallpapers");
        for (name, _) in crate::config::Palette::CHOICES {
            let target = bundled_path(&bundled, name);
            std::fs::create_dir_all(target.parent().unwrap()).unwrap();
            std::fs::copy(assets.join(format!("{name}.jpg")), &target).unwrap();
        }
        let teal = bundled_path(&bundled, "teal");
        seed(
            &fixture.join("config/cosmic"),
            &[(
                "com.system76.CosmicBackground",
                "all",
                &ron::to_string(&BgEntry::new("all", BgSource::Path(teal.clone()))).unwrap(),
            )],
        );
        let before = fixture_files(&fixture);
        let visible = |rect: Rectangle, size: Size, id: &str| {
            assert!(
                rect.x >= 0.
                    && rect.y >= 0.
                    && rect.x + rect.width <= size.width
                    && rect.y + rect.height <= size.height - 48.,
                "{id} is visible after navigation: {rect:?} in {size:?}"
            );
        };
        let themes = [
            ("dark", cosmic::Theme::dark()),
            ("light", cosmic::Theme::light()),
        ];
        for size in [
            Size::new(820., 600.),
            Size::new(1024., 720.),
            Size::new(1120., 800.),
            Size::new(1280., 900.),
        ] {
            let (mut app, initial) = Settings::init(Core::default(), Page::DesktopWallpaper);
            let _ = app.update(Message::Resized(size));
            settle(&mut app, initial);
            drive_wallpaper(&mut app);
            assert_eq!(app.wallpaper.shown().image.as_deref(), Some(teal.as_path()));
            assert!(
                app.wallpaper.shown().handle.is_some(),
                "the preview shows the selected picture"
            );
            let mut native = Native::new(size);
            let preview = native.bounds(app.settings_view(), "wallpaper-preview");
            let choose = native.bounds(app.settings_view(), "wallpaper-choose-picture");
            assert!(
                choose.x > preview.x + preview.width || choose.y > preview.y + preview.height,
                "the picture chooser sits beside or below the preview: {preview:?} {choose:?}"
            );
            assert!(
                (preview.width / preview.height - 16. / 9.).abs() < 0.05,
                "{preview:?}"
            );
            assert!(
                preview.width >= 320.,
                "readable wallpaper preview at {size:?}"
            );
            for id in [
                "wallpaper-choose-picture",
                "wallpaper-preview",
                "nav-general",
            ] {
                visible(native_reveal(&app, &mut native, id), size, id);
            }
            for (name, theme) in &themes {
                native.capture(
                    app.settings_view(),
                    theme,
                    &format!("redesign-wallpaper-{name}-{}", size.width),
                );
            }
            // A staged wallpaper is previewed exactly; nothing is written.
            let task = app.update(Message::Wallpaper(wallpaper::Message::Select(
                wallpaper::Pick::Bundled("ocean".into()),
            )));
            settle(&mut app, task);
            drive_wallpaper(&mut app);
            assert_eq!(
                app.wallpaper.shown().image.as_deref(),
                Some(bundled_path(&bundled, "ocean").as_path())
            );
            assert!(app.wallpaper.dirty());
            for (name, theme) in &themes {
                native.capture(
                    app.settings_view(),
                    theme,
                    &format!("redesign-wallpaper-draft-{name}-{}", size.width),
                );
            }
            let _ = app.update(Message::Wallpaper(wallpaper::Message::Discard));
            assert!(!app.wallpaper.dirty());

            // Calendar with the sample account; no private data.
            let _ = app.update(Message::Page(Page::Calendar));
            app.calendar.connection.backend = crate::calendar::Backend::Google;
            app.calendar.connection.account = "demo@example.com".into();
            app.calendar.connection.calendar_name = "Work".into();
            app.calendar.loading = false;
            let mut native = Native::new(size);
            native.scroll(app.settings_view(), "settings-content", 0.);
            let stage = native.bounds(app.settings_view(), "widget-preview-stage");
            let account = native.bounds(app.settings_view(), "calendar-account-options");
            assert!(
                stage.x + stage.width <= account.x
                    || account.x + account.width <= stage.x
                    || stage.y + stage.height <= account.y
                    || account.y + account.height <= stage.y,
                "preview and primary account action never overlap: {stage:?} {account:?}"
            );
            visible(account, size, "calendar-account-options");
            for id in ["calendar-account-options", "settings-back"] {
                visible(native_reveal(&app, &mut native, id), size, id);
            }
            for (name, theme) in &themes {
                native.capture(
                    app.settings_view(),
                    theme,
                    &format!("redesign-calendar-{name}-{}", size.width),
                );
            }
        }
        assert_eq!(raw_preferences(), preferences, "rendering saves nothing");
        assert_eq!(fixture_files(&fixture), before, "no desktop file changed");
    }
    pub(super) fn settle_settings(app: &mut Settings, task: Task<Message>) {
        use cosmic::iced::futures::StreamExt;
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        runtime.block_on(async {
            let mut tasks = std::collections::VecDeque::from([task]);
            while let Some(task) = tasks.pop_front() {
                if let Some(mut stream) = cosmic::iced::runtime::task::into_stream(task) {
                    while let Some(action) = stream.next().await {
                        if let cosmic::iced::runtime::Action::Output(cosmic::Action::App(message)) =
                            action
                        {
                            tasks.push_back(app.update(message));
                        }
                    }
                }
            }
        });
    }
    pub(super) fn send_settings(app: &mut Settings, message: Message) {
        let task = app.update(message);
        settle_settings(app, task);
    }
    /// Reveal a real control through the page scrollable before asserting its
    /// reachability. Scrolling never changes application configuration.
    pub(super) fn native_reveal(
        app: &Settings,
        native: &mut crate::native_test::Native,
        id: &str,
    ) -> cosmic::iced::Rectangle {
        let fixed = id.starts_with("nav-")
            || matches!(
                id,
                "settings-search" | "settings-back" | "settings-undo" | "wallpaper-apply"
            );
        let viewport = if fixed {
            cosmic::iced::Rectangle::with_size(native.size)
        } else {
            native.bounds(app.settings_view(), "settings-viewport")
        };
        if !fixed {
            native.operate(
                app.settings_view(),
                Box::new(navigation::Reveal::target(id)),
            );
        }
        let bounds = native.bounds(app.settings_view(), id);
        assert!(
            bounds.x >= 0.
                && bounds.x + bounds.width <= native.size.width
                && bounds.y >= viewport.y
                && bounds.y + bounds.height <= viewport.y + viewport.height,
            "{id} must be visible after scrolling: {bounds:?} in {viewport:?}"
        );
        bounds
    }

    fn native_click(app: &mut Settings, native: &mut crate::native_test::Native, id: &str) {
        use cosmic::iced::{Event, mouse};
        let rect = native_reveal(app, native, id);
        assert!(
            rect.x >= 0.
                && rect.y >= 0.
                && rect.x + rect.width <= native.size.width
                && rect.y + rect.height <= native.size.height,
            "{id}: {rect:?} in {:?}",
            native.size
        );
        for event in [
            mouse::Event::CursorMoved {
                position: rect.center(),
            },
            mouse::Event::ButtonPressed(mouse::Button::Left),
            mouse::Event::ButtonReleased(mouse::Button::Left),
        ] {
            for message in native
                .event(app.settings_view(), Event::Mouse(event), rect.center())
                .0
            {
                send_settings(app, message);
            }
        }
    }
    /// Types `text` into the accent's color picker, opening it from its
    /// control; `submit` presses "Use color".
    fn native_hex(
        app: &mut Settings,
        native: &mut crate::native_test::Native,
        text: &str,
        submit: bool,
    ) {
        use cosmic::widget::color_picker::ColorPickerUpdate;
        let open = |app: &Settings| {
            app.color_well
                .as_ref()
                .is_some_and(|well| well.target == color_well::Target::Accent)
        };
        if !open(app) {
            native_click(app, native, "colors-hex");
        }
        assert!(open(app), "the accent's control opens the color picker");
        send_settings(
            app,
            Message::ColorWell(color_well::Message::Picker(ColorPickerUpdate::Input(
                text.into(),
            ))),
        );
        if submit {
            send_settings(
                app,
                Message::ColorWell(color_well::Message::Picker(ColorPickerUpdate::AppliedColor)),
            );
        }
    }

    #[test]
    fn architecture_equal_choices_are_explicit_and_have_one_durable_undo() {
        if desktop_test_child(
            "architecture_equal_choices_are_explicit_and_have_one_durable_undo",
            DesktopEnv {
                app_config: true,
                fixture: true,
                ..Default::default()
            },
        ) {
            return;
        }
        let (mut app, _) = Settings::init(Core::default(), Page::Appearance);
        let value = app.config.appearance.opacity;
        send_settings(
            &mut app,
            Message::AppearanceValue(AppearanceControl::Opacity, value.into()),
        );
        assert!(raw_preferences().pointer("/appearance/overrides").is_none());
        send_settings(&mut app, Message::SaveAppearance);
        assert_eq!(
            Config::load().appearance.overrides & 1,
            1,
            "choosing the same value is still an intentional customization"
        );
        let store = crate::desktop::DesktopStore::from_env().unwrap();
        let action = store.latest_change().unwrap().unwrap();
        drop(app);
        let (mut app, _) = Settings::init(Core::default(), Page::Themes);
        assert_eq!(store.latest_change().unwrap().unwrap().id, action.id);
        send_settings(&mut app, Message::UndoDesktop);
        assert!(!app.error, "{}", app.status);
        assert_eq!(Config::load().appearance.overrides & 1, 0);
        assert!(store.latest_change().unwrap().is_none());
        send_settings(
            &mut app,
            Message::Wallpaper(wallpaper::Message::Palette("rose".into())),
        );
        let chosen = crate::desktop_theme::bytes(crate::config::Palette::named("rose").accent);
        send_settings(
            &mut app,
            Message::Profiles(profiles::Message::Select(profiles::Selection::Preset(
                "theme-outline",
            ))),
        );
        send_settings(&mut app, Message::Profiles(profiles::Message::Apply));
        assert_eq!(
            Config::load().appearance.accent,
            Some(chosen),
            "the first theme preserves an explicitly chosen palette"
        );
        send_settings(
            &mut app,
            Message::OpenBar(crate::desktop::panels::PanelKind::Dock),
        );
        let value = app
            .desktop
            .current()
            .unwrap()
            .opacity()
            .known()
            .copied()
            .unwrap();
        send_settings(
            &mut app,
            Message::DesktopEditor(desktop::Message::Opacity((value * 100.).round() as u8)),
        );
        send_settings(&mut app, Message::DesktopEditor(desktop::Message::Apply));
        assert!(Config::load().appearance.overrides & (1 << 8) != 0);
        assert!(
            app.profiles
                .custom_surfaces
                .contains(&crate::desktop::profiles::Domain::Dock),
            "equal explicit geometry still offers a scoped reset"
        );
        send_settings(
            &mut app,
            Message::ResetThemeSurface(crate::desktop::profiles::Domain::Dock),
        );
        assert_eq!(Config::load().appearance.overrides & (0x1f << 8), 0);
    }
    #[test]
    fn architecture_wallpaper_preview_keeps_color_and_scope_local_until_use() {
        if desktop_test_child(
            "architecture_wallpaper_preview_keeps_color_and_scope_local_until_use",
            DesktopEnv {
                app_config: true,
                fixture: true,
                ..Default::default()
            },
        ) {
            return;
        }
        use crate::desktop::wallpaper::{BgColor, WallpaperDraft};
        use wallpaper::{Message as Paper, Pick};
        let (mut app, _) = Settings::init(Core::default(), Page::DesktopWallpaper);
        drive_wallpaper(&mut app);
        let store = crate::desktop::DesktopStore::from_env().unwrap();
        let before = raw_preferences();
        let native = store.load(&WallpaperDraft::keys()).unwrap();
        send_settings(
            &mut app,
            Message::Wallpaper(Paper::Select(Pick::Color(BgColor::Single([0.2, 0.6, 0.8])))),
        );
        send_settings(&mut app, Message::Wallpaper(Paper::SyncColors(false)));
        send_settings(&mut app, Message::Wallpaper(Paper::AutomaticColors(true)));
        drive_wallpaper(&mut app);
        assert_eq!(raw_preferences(), before);
        assert_eq!(store.load(&WallpaperDraft::keys()).unwrap(), native);
        send_settings(&mut app, Message::Wallpaper(Paper::Apply));
        drive_wallpaper(&mut app);
        assert!(Config::load().appearance.wallpaper_colors);
        assert!(!Config::load().appearance.sync_colors);
        assert_ne!(store.load(&WallpaperDraft::keys()).unwrap(), native);
        assert!(
            store
                .latest_change()
                .unwrap()
                .unwrap()
                .label
                .contains("Wallpaper")
        );
        send_settings(&mut app, Message::UndoDesktop);
        assert!(!app.error, "{}", app.status);
        assert_eq!(raw_preferences(), before);
        assert_eq!(store.load(&WallpaperDraft::keys()).unwrap(), native);
    }
    #[test]
    fn architecture_theme_scope_is_local_and_widgets_only_is_enforced() {
        if desktop_test_child(
            "architecture_theme_scope_is_local_and_widgets_only_is_enforced",
            DesktopEnv {
                app_config: true,
                fixture: true,
                ..Default::default()
            },
        ) {
            return;
        }
        use profiles::{Message as Look, Selection};
        let (mut app, _) = Settings::init(Core::default(), Page::Themes);
        let store = crate::desktop::DesktopStore::from_env().unwrap();
        let native = store
            .load(&crate::desktop::appearance::AppearanceDraft::keys())
            .unwrap();
        let before = raw_preferences();
        send_settings(
            &mut app,
            Message::Profiles(Look::Select(Selection::Preset("theme-outline"))),
        );
        for job in app.profiles.take_theme_jobs() {
            send_settings(&mut app, Message::Profiles(job.run()));
        }
        send_settings(&mut app, Message::Profiles(Look::ThemeScope(false)));
        assert_eq!(raw_preferences(), before);
        assert_eq!(
            store
                .load(&crate::desktop::appearance::AppearanceDraft::keys())
                .unwrap(),
            native
        );
        send_settings(&mut app, Message::Profiles(Look::Apply));
        assert_eq!(
            Config::load().appearance.family,
            Some(crate::desktop_theme::Family::Outline)
        );
        assert!(!Config::load().appearance.sync_colors);
        assert_eq!(
            store
                .load(&crate::desktop::appearance::AppearanceDraft::keys())
                .unwrap(),
            native
        );
        assert!(matches!(app.profiles.selection(), Selection::Current));
    }
    #[test]
    fn architecture_hiding_widgets_keeps_applied_background_policy_running() {
        if desktop_test_child(
            "architecture_hiding_widgets_keeps_applied_background_policy_running",
            DesktopEnv {
                app_config: true,
                fixture: true,
                ..Default::default()
            },
        ) {
            return;
        }
        use crate::desktop::wallpaper::{BgColor, BgEntry, BgSource, automatic, shared_key};
        Config::save_appearance(json!({"color_policy":"wallpaper","wallpaper_colors":true}))
            .unwrap();
        let store = crate::desktop::DesktopStore::from_env().unwrap();
        let path = store.user_path(&shared_key());
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(
            path,
            ron::to_string(&BgEntry::new(
                "all",
                BgSource::Color(BgColor::Single([0.4, 0.7, 0.2])),
            ))
            .unwrap(),
        )
        .unwrap();
        let (mut app, _) = Settings::init(Core::default(), Page::General);
        send_settings(&mut app, Message::Desktop(false));
        assert!(!Config::load().widgets_visible);
        assert!(Config::load().appearance.wallpaper_colors);
        assert!(!app.busy);
        let manual = store.latest_change().unwrap().unwrap();
        automatic::commit(&store, automatic::prepare(&store).unwrap().unwrap()).unwrap();
        assert_eq!(store.latest_change().unwrap().unwrap().id, manual.id);
        assert!(!automatic::paused(&store));
        assert!(Config::load().appearance.accent.is_some());
    }
    #[test]
    fn architecture_undo_rechecks_the_displayed_action_identity() {
        if desktop_test_child(
            "architecture_undo_rechecks_the_displayed_action_identity",
            DesktopEnv {
                app_config: true,
                fixture: true,
                ..Default::default()
            },
        ) {
            return;
        }
        let (mut app, initial) = Settings::init(Core::default(), Page::Clock);
        settle_settings(&mut app, initial);
        send_settings(&mut app, Message::ClockFormat(false));
        let shown = app.last_desktop_change.clone().unwrap();
        crate::desktop::actions::in_action("Outside choice", || {
            Config::save_appearance(json!({"opacity":73}))
        })
        .unwrap();
        let before = raw_preferences();
        send_settings(&mut app, Message::UndoDesktop);
        assert!(app.error, "an unseen newer action must be reviewed first");
        assert_eq!(raw_preferences(), before);
        assert_ne!(app.last_desktop_change.clone().unwrap().2, shown.2);
        send_settings(&mut app, Message::UndoDesktop);
        assert!(!app.error, "{}", app.status);
        assert_ne!(raw_preferences()["appearance"]["opacity"], 73);
        assert_eq!(Config::load().clock.format_24h, false);
    }
    #[test]
    fn independent_r1_submit_color_after_scope_change() {
        if desktop_test_child(
            "independent_r1_submit_color_after_scope_change",
            DesktopEnv {
                app_config: true,
                fixture: true,
                ..Default::default()
            },
        ) {
            return;
        }
        let (mut app, initial) = Settings::init(Core::default(), Page::Themes);
        settle_settings(&mut app, initial);
        send_settings(
            &mut app,
            Message::Wallpaper(wallpaper::Message::AccentText("#336699".into())),
        );
        assert!(app.wallpaper.valid_accent_input());
        send_settings(
            &mut app,
            Message::Wallpaper(wallpaper::Message::SyncColors(false)),
        );
        assert!(
            !Config::load().appearance.sync_colors,
            "scope action should finish immediately"
        );
        assert!(
            app.wallpaper.valid_accent_input(),
            "unsubmitted text should survive the independent action"
        );
        send_settings(
            &mut app,
            Message::Wallpaper(wallpaper::Message::AccentSubmit),
        );
        assert_eq!(Config::load().appearance.accent, Some([51, 102, 153]));
    }

    #[test]
    fn independent_r1_unfinished_hex_does_not_block_mode_or_other_inspectors() {
        if desktop_test_child(
            "independent_r1_unfinished_hex_does_not_block_mode_or_other_inspectors",
            DesktopEnv {
                app_config: true,
                fixture: true,
                ..Default::default()
            },
        ) {
            return;
        }
        use crate::desktop::appearance::{ColorRole, SystemMode};
        let (mut app, initial) = Settings::init(Core::default(), Page::DesktopAppearance);
        settle_settings(&mut app, initial);
        send_settings(
            &mut app,
            Message::SystemAppearance(system_appearance::Message::HexInput(
                ColorRole::Accent,
                "#F".into(),
            )),
        );
        send_settings(
            &mut app,
            Message::SystemAppearance(system_appearance::Message::Mode(SystemMode::Light)),
        );
        let store = crate::desktop::DesktopStore::from_env().unwrap();
        let draft = crate::desktop::appearance::AppearanceDraft::load(&store).unwrap();
        assert_eq!(draft.mode().known(), Some(&SystemMode::Light));
        assert_eq!(
            app.system_appearance.hex_text(ColorRole::Accent),
            Some("#F")
        );
        send_settings(
            &mut app,
            Message::OpenBar(crate::desktop::panels::PanelKind::Dock),
        );
        send_settings(
            &mut app,
            Message::DesktopEditor(desktop::Message::Anchor(
                crate::desktop::panels::Anchor::Left,
            )),
        );
        let dock = crate::desktop::panels::PanelDraft::load(
            &store,
            crate::desktop::panels::PanelKind::Dock,
        )
        .unwrap();
        assert_eq!(
            dock.anchor().known(),
            Some(&crate::desktop::panels::Anchor::Left)
        );
    }

    #[test]
    fn independent_r1_segmented_dock_corners_commit_directly() {
        if desktop_test_child(
            "independent_r1_segmented_dock_corners_commit_directly",
            DesktopEnv {
                app_config: true,
                fixture: true,
                ..Default::default()
            },
        ) {
            return;
        }
        let (mut app, initial) = Settings::init(Core::default(), Page::DesktopDock);
        settle_settings(&mut app, initial);
        let store = crate::desktop::DesktopStore::from_env().unwrap();
        let old = app
            .desktop
            .current()
            .unwrap()
            .radius()
            .known()
            .copied()
            .unwrap();
        let next = if old == 0 { 12 } else { 0 };
        // This is the real event emitted by the Square/Rounded/Pill segmented control.
        send_settings(
            &mut app,
            Message::DesktopEditor(desktop::Message::RadiusChoice(next)),
        );
        let dock = crate::desktop::panels::PanelDraft::load(
            &store,
            crate::desktop::panels::PanelKind::Dock,
        )
        .unwrap();
        assert_eq!(
            dock.radius().known(),
            Some(&next),
            "the completed segmented choice must persist without touching a second control"
        );
    }

    #[test]
    fn independent_r1_native_slider_keyboard_completion() {
        if desktop_test_child(
            "independent_r1_native_slider_keyboard_completion",
            DesktopEnv {
                app_config: true,
                fixture: true,
                ..Default::default()
            },
        ) {
            return;
        }
        use cosmic::iced::{Event, Point, Size, keyboard};
        let (mut app, initial) = Settings::init(Core::default(), Page::DesktopDock);
        settle_settings(&mut app, initial);
        let store = crate::desktop::DesktopStore::from_env().unwrap();
        let old = app
            .desktop
            .current()
            .unwrap()
            .opacity()
            .known()
            .copied()
            .unwrap();
        let value = (old * 100.).round() as u32;
        let mut native = crate::native_test::Native::new(Size::new(400., 60.));
        let slider = |value| {
            cosmic::widget::slider(0u32..=100, value, |value| {
                Message::DesktopEditor(desktop::Message::Opacity(value as u8))
            })
            .on_release(Message::DesktopEditor(desktop::Message::Apply))
            .step(1u32)
            .width(cosmic::iced::Length::Fill)
        };
        let slider = |value| {
            view::completed_slider(
                slider(value).into(),
                Message::DesktopEditor(desktop::Message::Apply),
            )
        };
        let key = keyboard::Key::Named(keyboard::key::Named::ArrowDown);
        let press = Event::Keyboard(keyboard::Event::KeyPressed {
            modified_key: key.clone(),
            key: key.clone(),
            physical_key: keyboard::key::Physical::Code(keyboard::key::Code::ArrowDown),
            location: keyboard::Location::Standard,
            modifiers: keyboard::Modifiers::empty(),
            text: None,
            repeat: false,
        });
        let release = Event::Keyboard(keyboard::Event::KeyReleased {
            modified_key: key.clone(),
            key,
            physical_key: keyboard::key::Physical::Code(keyboard::key::Code::ArrowDown),
            location: keyboard::Location::Standard,
            modifiers: keyboard::Modifiers::empty(),
        });
        let messages = native.event(slider(value), press, Point::new(200., 8.)).0;
        assert!(
            messages
                .iter()
                .any(|m| matches!(m, Message::DesktopEditor(desktop::Message::Opacity(_)))),
            "native slider must receive the key"
        );
        for message in messages {
            send_settings(&mut app, message);
        }
        for message in native
            .event(slider(value - 1), release, Point::new(200., 8.))
            .0
        {
            send_settings(&mut app, message);
        }
        let dock = crate::desktop::panels::PanelDraft::load(
            &store,
            crate::desktop::panels::PanelKind::Dock,
        )
        .unwrap();
        assert!(
            (dock.opacity().known().copied().unwrap() - (value - 1) as f32 / 100.).abs() < 0.001,
            "completed native key gesture must persist"
        );
    }
    fn repair_slider_event(pressed: bool, repeat: bool) -> cosmic::iced::Event {
        use cosmic::iced::keyboard::{self, key};
        let key = keyboard::Key::Named(key::Named::ArrowDown);
        let physical_key = key::Physical::Code(key::Code::ArrowDown);
        cosmic::iced::Event::Keyboard(if pressed {
            keyboard::Event::KeyPressed {
                modified_key: key.clone(),
                key,
                physical_key,
                location: keyboard::Location::Standard,
                modifiers: keyboard::Modifiers::empty(),
                text: None,
                repeat,
            }
        } else {
            keyboard::Event::KeyReleased {
                modified_key: key.clone(),
                key,
                physical_key,
                location: keyboard::Location::Standard,
                modifiers: keyboard::Modifiers::empty(),
            }
        })
    }

    #[test]
    fn repair_r1_corner_choices_and_advanced_radius_have_separate_completion() {
        if desktop_test_child(
            "repair_r1_corner_choices_and_advanced_radius_have_separate_completion",
            DesktopEnv {
                app_config: true,
                fixture: true,
                ..Default::default()
            },
        ) {
            return;
        }
        use crate::desktop::panels::{MAX_RADIUS, PanelDraft, PanelKind};
        let (mut app, initial) = Settings::init(Core::default(), Page::DesktopDock);
        settle_settings(&mut app, initial);
        let store = crate::desktop::DesktopStore::from_env().unwrap();
        for kind in PanelKind::ALL {
            send_settings(&mut app, Message::OpenBar(kind));
            for value in [0, 12, MAX_RADIUS] {
                let before = PanelDraft::load(&store, kind)
                    .unwrap()
                    .radius()
                    .known()
                    .copied()
                    .unwrap();
                send_settings(
                    &mut app,
                    Message::DesktopEditor(desktop::Message::RadiusChoice(value)),
                );
                assert_eq!(
                    PanelDraft::load(&store, kind).unwrap().radius().known(),
                    Some(&value)
                );
                assert!(!app.desktop.current().unwrap().dirty());
                assert!(store.latest_change().unwrap().is_some());
                send_settings(&mut app, Message::UndoDesktop);
                assert_eq!(
                    PanelDraft::load(&store, kind).unwrap().radius().known(),
                    Some(&before)
                );
            }
            let before = PanelDraft::load(&store, kind)
                .unwrap()
                .radius()
                .known()
                .copied()
                .unwrap();
            for value in [5, 7] {
                send_settings(
                    &mut app,
                    Message::DesktopEditor(desktop::Message::Radius(value)),
                );
            }
            assert_eq!(
                PanelDraft::load(&store, kind).unwrap().radius().known(),
                Some(&before)
            );
            send_settings(&mut app, Message::DesktopEditor(desktop::Message::Apply));
            assert_eq!(
                PanelDraft::load(&store, kind).unwrap().radius().known(),
                Some(&7)
            );
            send_settings(&mut app, Message::UndoDesktop);
            assert_eq!(
                PanelDraft::load(&store, kind).unwrap().radius().known(),
                Some(&before)
            );
        }
    }

    #[test]
    fn repair_r1_repeated_keyboard_slider_gestures_are_scoped_and_guarded() {
        if desktop_test_child(
            "repair_r1_repeated_keyboard_slider_gestures_are_scoped_and_guarded",
            DesktopEnv {
                app_config: true,
                fixture: true,
                ..Default::default()
            },
        ) {
            return;
        }
        use crate::desktop::panels::{PanelDraft, PanelKind};
        use cosmic::iced::{Length, Point, Size};
        let (mut app, initial) = Settings::init(Core::default(), Page::Themes);
        settle_settings(&mut app, initial);
        send_settings(
            &mut app,
            Message::Wallpaper(wallpaper::Message::AccentText("#12".into())),
        );
        send_settings(&mut app, Message::OpenBar(PanelKind::Dock));
        let store = crate::desktop::DesktopStore::from_env().unwrap();
        let opacity = || {
            (PanelDraft::load(&store, PanelKind::Dock)
                .unwrap()
                .opacity()
                .known()
                .copied()
                .unwrap()
                * 100.)
                .round() as u32
        };
        let before = opacity();
        let mut native = crate::native_test::Native::new(Size::new(400., 60.));
        let slider = |value| {
            view::completed_slider(
                cosmic::widget::slider(0u32..=100, value, |v| {
                    Message::DesktopEditor(desktop::Message::Opacity(v as u8))
                })
                .on_release(Message::DesktopEditor(desktop::Message::Apply))
                .width(Length::Fill)
                .into(),
                Message::DesktopEditor(desktop::Message::Apply),
            )
        };
        for step in 0..3 {
            for message in native
                .event(
                    slider(before - step),
                    repair_slider_event(true, step != 0),
                    Point::new(200., 8.),
                )
                .0
            {
                send_settings(&mut app, message);
            }
            assert_eq!(opacity(), before, "a held key has not completed");
        }
        for message in native
            .event(
                slider(before - 3),
                repair_slider_event(false, false),
                Point::new(410., 8.),
            )
            .0
        {
            send_settings(&mut app, message);
        }
        assert_eq!(
            opacity(),
            before - 3,
            "release completes even after the pointer left"
        );
        assert_eq!(app.wallpaper.color_text(&Config::load()), "#12");
        send_settings(&mut app, Message::UndoDesktop);
        assert_eq!(opacity(), before, "one Undo reverses all repeats");
        // A later external value during another held key is never overwritten.
        for message in native
            .event(
                slider(before),
                repair_slider_event(true, false),
                Point::new(200., 8.),
            )
            .0
        {
            send_settings(&mut app, message);
        }
        let key = crate::desktop::store::ConfigKey::new(PanelKind::Dock.namespace(), 1, "opacity")
            .unwrap();
        std::fs::write(store.user_path(&key), "0.41").unwrap();
        for message in native
            .event(
                slider(before - 1),
                repair_slider_event(false, false),
                Point::new(200., 8.),
            )
            .0
        {
            send_settings(&mut app, message);
        }
        assert_eq!(opacity(), 41);
        assert!(matches!(
            app.desktop.notice,
            Some(desktop::Notice::Error(_))
        ));
        // The same wrapper completes a widget preference, without submitting text.
        send_settings(&mut app, Message::Page(Page::Appearance));
        let before = Config::load().appearance.opacity;
        let slider = |value| {
            view::completed_slider(
                cosmic::widget::slider(70u8..=100, value, |v| {
                    Message::AppearanceValue(AppearanceControl::Opacity, u16::from(v))
                })
                .on_release(Message::SaveAppearance)
                .width(Length::Fill)
                .into(),
                Message::SaveAppearance,
            )
        };
        let mut native = crate::native_test::Native::new(Size::new(400., 60.));
        for message in native
            .event(
                slider(before),
                repair_slider_event(true, false),
                Point::new(200., 8.),
            )
            .0
        {
            send_settings(&mut app, message);
        }
        assert_eq!(Config::load().appearance.opacity, before);
        for message in native
            .event(
                slider(before - 1),
                repair_slider_event(false, false),
                Point::new(200., 8.),
            )
            .0
        {
            send_settings(&mut app, message);
        }
        assert_eq!(Config::load().appearance.opacity, before - 1);
        assert_eq!(app.wallpaper.color_text(&Config::load()), "#12");
        send_settings(&mut app, Message::UndoDesktop);
        assert_eq!(Config::load().appearance.opacity, before);
    }
    fn r2_native_event(
        app: &mut Settings,
        native: &mut crate::native_test::Native,
        event: cosmic::iced::Event,
        point: cosmic::iced::Point,
    ) -> Vec<Message> {
        let messages = native.event(app.settings_view(), event, point).0;
        for message in messages.clone() {
            send_settings(app, message);
        }
        messages
    }
    #[test]
    fn independent_r2_real_view_corner_buttons_and_slider_boundaries() {
        if desktop_test_child(
            "independent_r2_real_view_corner_buttons_and_slider_boundaries",
            DesktopEnv {
                app_config: true,
                fixture: true,
                ..Default::default()
            },
        ) {
            return;
        }
        use crate::desktop::panels::{MAX_RADIUS, PanelDraft, PanelKind};
        use cosmic::iced::{Event, Point, Size, keyboard, mouse, window};
        let (mut app, initial) = Settings::init(Core::default(), Page::DesktopDock);
        settle_settings(&mut app, initial);
        let store = crate::desktop::DesktopStore::from_env().unwrap();
        let opacity = |kind| {
            (PanelDraft::load(&store, kind)
                .unwrap()
                .opacity()
                .known()
                .copied()
                .unwrap()
                * 100.)
                .round() as u32
        };
        for kind in PanelKind::ALL {
            send_settings(&mut app, Message::OpenBar(kind));
            let mut native = crate::native_test::Native::new(Size::new(1024., 1400.));
            for (label, radius) in
                [("Square", 0), ("Rounded", 12), ("Pill", MAX_RADIUS)].into_iter()
            {
                let before = PanelDraft::load(&store, kind)
                    .unwrap()
                    .radius()
                    .known()
                    .copied()
                    .unwrap();
                let point =
                    native_reveal(&app, &mut native, &format!("native-bar-radius-{radius}"))
                        .center();
                assert!(point.y < native.size.height, "{label} visible");
                r2_native_event(
                    &mut app,
                    &mut native,
                    Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)),
                    point,
                );
                let messages = r2_native_event(
                    &mut app,
                    &mut native,
                    Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)),
                    point,
                );
                assert!(messages.iter().any(|m|matches!(m,Message::DesktopEditor(desktop::Message::RadiusChoice(n)) if *n==radius)),"actual {kind:?} {label} wiring: {messages:?}");
                assert_eq!(
                    PanelDraft::load(&store, kind).unwrap().radius().known(),
                    Some(&radius)
                );
                let action = store
                    .latest_change()
                    .unwrap()
                    .expect("completed choice has Undo");
                let undo_label = format!("Undo {}", action.label.to_lowercase());
                let point = native
                    .text_bounds(app.settings_view(), &undo_label)
                    .center();
                for event in [
                    mouse::Event::ButtonPressed(mouse::Button::Left),
                    mouse::Event::ButtonReleased(mouse::Button::Left),
                ] {
                    r2_native_event(&mut app, &mut native, Event::Mouse(event), point);
                }
                assert_eq!(
                    PanelDraft::load(&store, kind).unwrap().radius().known(),
                    Some(&before)
                );
                assert!(
                    store.latest_change().unwrap().is_none(),
                    "one click, one Undo"
                );
            }
            let arrow = |pressed, repeat| repair_slider_event(pressed, repeat);
            let bounds = native_reveal(&app, &mut native, "native-bar-opacity");
            let point = Point::new(bounds.center_x(), bounds.y + bounds.height - 8.);
            let before = opacity(kind);
            for i in 0..3 {
                let messages = r2_native_event(&mut app, &mut native, arrow(true, i > 0), point);
                assert!(
                    messages
                        .iter()
                        .any(|m| matches!(m, Message::DesktopEditor(desktop::Message::Opacity(_)))),
                    "actual opacity slider receives arrow at {point:?}: {messages:?}"
                );
                assert_eq!(opacity(kind), before, "repeat has not completed");
            }
            let mut unrelated = arrow(false, false);
            if let Event::Keyboard(keyboard::Event::KeyReleased { physical_key, .. }) =
                &mut unrelated
            {
                *physical_key = keyboard::key::Physical::Code(keyboard::key::Code::KeyA);
            }
            r2_native_event(&mut app, &mut native, unrelated, point);
            assert_eq!(opacity(kind), before, "unrelated release must not complete");
            r2_native_event(
                &mut app,
                &mut native,
                arrow(false, false),
                Point::new(-20., -20.),
            );
            assert_eq!(
                opacity(kind),
                before - 3,
                "matching release completes outside slider"
            );
            send_settings(&mut app, Message::UndoDesktop);
            assert_eq!(opacity(kind), before, "one Undo restores all repeats");
            assert!(
                store.latest_change().unwrap().is_none(),
                "no duplicate completion/history"
            );
            r2_native_event(&mut app, &mut native, arrow(true, false), point);
            r2_native_event(
                &mut app,
                &mut native,
                Event::Window(window::Event::Unfocused),
                Point::new(-20., -20.),
            );
            assert_eq!(
                opacity(kind),
                before - 1,
                "focus loss completes changed key gesture"
            );
            r2_native_event(&mut app, &mut native, arrow(false, false), point);
            send_settings(&mut app, Message::UndoDesktop);
            assert_eq!(opacity(kind), before);
            assert!(
                store.latest_change().unwrap().is_none(),
                "later release after focus loss is not a second action"
            );
            r2_native_event(
                &mut app,
                &mut native,
                Event::Keyboard(keyboard::Event::ModifiersChanged(keyboard::Modifiers::CTRL)),
                point,
            );
            r2_native_event(
                &mut app,
                &mut native,
                Event::Mouse(mouse::Event::WheelScrolled {
                    delta: mouse::ScrollDelta::Lines { x: 0., y: -1. },
                }),
                point,
            );
            assert_eq!(
                opacity(kind),
                before - 1,
                "Ctrl+wheel completes without pointer release"
            );
            send_settings(&mut app, Message::UndoDesktop);
            r2_native_event(
                &mut app,
                &mut native,
                Event::Keyboard(keyboard::Event::ModifiersChanged(
                    keyboard::Modifiers::empty(),
                )),
                point,
            );
            r2_native_event(
                &mut app,
                &mut native,
                Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)),
                point,
            );
            assert_eq!(opacity(kind), before, "pointer movement stays local");
            r2_native_event(
                &mut app,
                &mut native,
                Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)),
                Point::new(-20., -20.),
            );
            assert_ne!(opacity(kind), before, "pointer release still completes");
            send_settings(&mut app, Message::UndoDesktop);
            assert_eq!(opacity(kind), before);
            assert!(store.latest_change().unwrap().is_none());
        }
    }
    #[test]
    fn review_r1_repair_action_visibility_tracks_persisted_provenance() {
        if desktop_test_child(
            "review_r1_repair_action_visibility_tracks_persisted_provenance",
            DesktopEnv {
                app_config: true,
                fixture: true,
                ..Default::default()
            },
        ) {
            return;
        }
        let (mut app, initial) = Settings::init(Core::default(), Page::Themes);
        settle_settings(&mut app, initial);
        let mut native = crate::native_test::Native::new(cosmic::iced::Size::new(1024., 1400.));
        for (family, marker, visible) in [
            (Some(crate::desktop_theme::Family::Paper), 0, true),
            (Some(crate::desktop_theme::Family::Paper), 1, false),
            (None, 0, false),
        ] {
            let path = Config::path().unwrap();
            let mut raw: serde_json::Value =
                serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
            raw["appearance"]["family"] = serde_json::json!(family);
            raw["appearance"]["native_mode_palettes"] = serde_json::json!(marker);
            std::fs::write(path, raw.to_string()).unwrap();
            app.config = Config::load();
            assert_eq!(
                native
                    .try_bounds(app.settings_view(), "theme-matching-repair")
                    .is_some(),
                visible
            );
        }
    }

    #[test]
    fn independent_r2_native_retained_text_uses_current_scope_and_preserves_outside_fields() {
        if desktop_test_child(
            "independent_r2_native_retained_text_uses_current_scope_and_preserves_outside_fields",
            DesktopEnv {
                app_config: true,
                fixture: true,
                ..Default::default()
            },
        ) {
            return;
        }
        use cosmic::iced::Size;
        let (mut app, initial) = Settings::init(Core::default(), Page::Themes);
        settle_settings(&mut app, initial);
        let mut native = crate::native_test::Native::new(Size::new(1024., 1000.));
        native_hex(&mut app, &mut native, "#336699", false);
        send_settings(
            &mut app,
            Message::Wallpaper(wallpaper::Message::SyncColors(false)),
        );
        // The typed code waits in the picker until "Use color".
        assert_ne!(app.wallpaper.color_text(&Config::load()), "#336699");
        let path = Config::path().unwrap();
        let mut outside: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        outside["clock"]["show_date"] = serde_json::json!(false);
        outside["appearance"]["accent"] = serde_json::json!([71, 82, 93]);
        std::fs::write(&path, outside.to_string()).unwrap();
        native_hex(&mut app, &mut native, "#336699", true);
        assert_eq!(Config::load().appearance.accent, Some([51, 102, 153]));
        assert!(!Config::load().appearance.sync_colors);
        assert!(!Config::load().clock.show_date);
        send_settings(&mut app, Message::UndoDesktop);
        assert_eq!(
            Config::load().appearance.accent,
            Some([71, 82, 93]),
            "Undo restores the fresh submission baseline"
        );
        assert!(!Config::load().clock.show_date);
    }

    #[test]
    fn independent_r2_panel_corners_complete_from_actual_view() {
        if desktop_test_child(
            "independent_r2_panel_corners_complete_from_actual_view",
            DesktopEnv {
                app_config: true,
                fixture: true,
                ..Default::default()
            },
        ) {
            return;
        }
        use crate::desktop::panels::{PanelDraft, PanelKind};
        use cosmic::iced::{Event, Size, mouse};
        let (mut app, initial) = Settings::init(Core::default(), Page::DesktopDock);
        settle_settings(&mut app, initial);
        send_settings(&mut app, Message::OpenBar(PanelKind::Panel));
        let store = crate::desktop::DesktopStore::from_env().unwrap();
        let mut native = crate::native_test::Native::new(Size::new(1024., 1400.));
        let old = PanelDraft::load(&store, PanelKind::Panel)
            .unwrap()
            .radius()
            .known()
            .copied()
            .unwrap();
        assert_ne!(old, 0);
        let point = native_reveal(&app, &mut native, "native-bar-radius-0").center();
        r2_native_event(
            &mut app,
            &mut native,
            Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)),
            point,
        );
        let messages = r2_native_event(
            &mut app,
            &mut native,
            Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)),
            point,
        );
        eprintln!(
            "actual Panel Square event: {messages:?}; displayed={:?}; disk={:?}; history={:?}",
            app.desktop.current().unwrap().radius().known(),
            PanelDraft::load(&store, PanelKind::Panel)
                .unwrap()
                .radius()
                .known(),
            store.latest_change().unwrap().map(|v| v.label)
        );
        native.capture(
            app.settings_view(),
            &controls_theme(&app.config),
            "panel-square-completed",
        );
        assert_eq!(
            PanelDraft::load(&store, PanelKind::Panel)
                .unwrap()
                .radius()
                .known(),
            Some(&0),
            "completed Panel corner click must persist"
        );
        send_settings(&mut app, Message::UndoDesktop);
        assert_eq!(
            PanelDraft::load(&store, PanelKind::Panel)
                .unwrap()
                .radius()
                .known(),
            Some(&old)
        );
        assert!(
            store.latest_change().unwrap().is_none(),
            "one actual click creates one Undo"
        );
    }
    #[test]
    fn task2n1_legacy_repair_has_clickable_confirmation_at_supported_sizes() {
        if desktop_test_child(
            "task2n1_legacy_repair_has_clickable_confirmation_at_supported_sizes",
            DesktopEnv {
                app_config: true,
                fixture: true,
                ..Default::default()
            },
        ) {
            return;
        }
        use cosmic::iced::{Event, mouse};
        let store = crate::desktop::DesktopStore::from_env().unwrap();
        let key = crate::desktop::ConfigKey::new("com.system76.CosmicPanel.Dock", 1, "background")
            .unwrap();
        let path = store.user_path(&key);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        for (size, maximized) in [
            (Size::new(1280., 800.), false),
            (Size::new(1280., 800.), true),
            (Size::new(1280., 720.), true),
            (Size::new(820., 600.), false),
        ] {
            std::fs::write(Config::path().unwrap(),r#"{"appearance":{"family":"frosted","native_mode_palettes":0,"sync_colors":true},"setup_dismissed":true}"#).unwrap();
            std::fs::write(&path, "Color((0.1,0.2,0.3))").unwrap();
            let (mut app, initial) = Settings::init(Core::default(), Page::Themes);
            settle_settings(&mut app, initial);
            app.core.window.is_maximized = maximized;
            send_settings(&mut app, Message::Resized(size));
            let mut native = crate::native_test::Native::new(size);
            // Use the full window view, including its real header and pinned
            // footer. Only pointer-generated messages may select or apply Repair.
            let click = |app: &mut Settings, native: &mut crate::native_test::Native, id: &str| {
                let rect = native.bounds(app.view(), id);
                assert!(
                    rect.x >= 0.
                        && rect.y >= 0.
                        && rect.x + rect.width <= size.width
                        && rect.y + rect.height <= size.height,
                    "{id}: {rect:?} in {size:?}"
                );
                let mut emitted = false;
                for event in [
                    mouse::Event::CursorMoved {
                        position: rect.center(),
                    },
                    mouse::Event::ButtonPressed(mouse::Button::Left),
                    mouse::Event::ButtonReleased(mouse::Button::Left),
                ] {
                    for message in native
                        .event(app.view(), Event::Mouse(event), rect.center())
                        .0
                    {
                        emitted = true;
                        send_settings(app, message);
                    }
                }
                assert!(emitted, "rendered {id} must emit a real pointer action");
            };
            click(&mut app, &mut native, "theme-matching-repair");
            assert_eq!(app.page, Page::Themes);
            assert_eq!(
                app.profiles.profile().unwrap().name,
                "Repair theme matching"
            );
            assert!(app.profiles.can_apply(), "{:?}", app.profiles.notice);
            assert_eq!(
                std::fs::read_to_string(&path).unwrap(),
                "Color((0.1,0.2,0.3))",
                "Repair opens a read-only review"
            );
            native.capture(
                app.view(),
                &controls_theme(&app.config),
                &format!(
                    "n1-repair-preview-{}x{}-max{}",
                    size.width, size.height, maximized
                ),
            );
            click(&mut app, &mut native, "profiles-confirm");
            assert_eq!(
                Config::load().appearance.native_mode_palettes,
                1,
                "the pointer confirmation must commit Repair"
            );
            assert_eq!(std::fs::read_to_string(&path).unwrap(), "ThemeDefault");
            native.capture(
                app.view(),
                &controls_theme(&app.config),
                &format!(
                    "n1-repair-applied-{}x{}-max{}",
                    size.width, size.height, maximized
                ),
            );
        }
    }

    #[test]
    fn islands_toggle_check_again_retains_fresh_status() {
        if desktop_test_child(
            "islands_toggle_check_again_retains_fresh_status",
            DesktopEnv {
                app_config: true,
                fixture: true,
                ..Default::default()
            },
        ) {
            return;
        }
        let store = crate::desktop::DesktopStore::from_env().unwrap();
        let root = store.fixture_root().unwrap();
        crate::desktop::islands::fixture_installed(root);
        crate::test_env::native_worker(&root, true);
        std::fs::write(root.join("status-islands-probe.py"),
            "import pathlib\nprint((pathlib.Path(__file__).parent/'islands-installation.json').read_text())\n").unwrap();
        let (mut app, initial) = Settings::init(Core::default(), Page::DesktopDock);
        settle_settings(&mut app, initial);
        let path = root.join("islands-installation.json");
        let original = std::fs::read(&path).unwrap();
        let generation = app.installation_generation;
        // Discover a package removed outside Settings through the exact banner
        // message, then discover its return through the same control.
        let mut absent: serde_json::Value = serde_json::from_slice(&original).unwrap();
        absent["installation"] = "absent".into();
        absent["provenance"] = serde_json::Value::Null;
        std::fs::write(&path, serde_json::to_vec(&absent).unwrap()).unwrap();
        send_settings(
            &mut app,
            Message::DesktopEditor(desktop::Message::Islands(desktop::islands::Edit::Check)),
        );
        assert!(!app.desktop.islands.installation.installed());
        assert!(app.installation_generation > generation);
        std::fs::write(&path, original).unwrap();
        send_settings(
            &mut app,
            Message::DesktopEditor(desktop::Message::Islands(desktop::islands::Edit::Check)),
        );
        assert!(app.desktop.islands.installation.installed());
        assert!(!app.installation_busy);
    }

    #[test]
    fn runtime_boundary_worker_one_observation_and_whole_refresh_deadline() {
        if desktop_test_child(
            "runtime_boundary_worker_one_observation_and_whole_refresh_deadline",
            DesktopEnv {
                app_config: true,
                fixture: true,
                ..Default::default()
            },
        ) {
            return;
        }
        let store = crate::desktop::DesktopStore::from_env().unwrap();
        let root = store.fixture_root().unwrap();
        crate::desktop::islands::fixture_installed(root);
        let (mut app, initial) = Settings::init(Core::default(), Page::General);
        settle_settings(&mut app, initial);
        crate::test_env::native_worker(root, true);
        let helper = root.join("status-islands-probe.py");
        std::fs::write(
            &helper,
            r#"import json,sys,time
from pathlib import Path
root=Path(__file__).parent
presence='--presence' in sys.argv
with (root/'status-calls').open('a') as log: log.write('presence\n' if presence else 'compat\n')
record=json.loads((root/'islands-installation.json').read_text())
record.update(status_version=1,status='Checking',compatible=None)
print(json.dumps(record),flush=True)
if not presence:
    time.sleep(float((root/'status-delay').read_text()))
    record.update(status='Ready',compatible=True)
    print(json.dumps(record),flush=True)
"#,
        )
        .unwrap();
        // Reproduce the reviewed call chain independently: connect remains fresh
        // for write authority. It must not be used for the read-only UI refresh.
        std::fs::write(root.join("status-delay"), "0").unwrap();
        let observed = crate::desktop::islands::Installation::discover(&store);
        assert!(observed.installed());
        let _ = crate::desktop::islands::Client::connect(&store);
        assert_eq!(
            std::fs::read_to_string(root.join("status-calls")).unwrap(),
            "compat\ncompat\n"
        );
        eprintln!("reviewed discover + Client::connect chain: two compatibility invocations");
        for (delay, timeout) in [("0.15", false), ("30", true)] {
            std::fs::write(root.join("status-delay"), delay).unwrap();
            std::fs::write(root.join("status-calls"), "").unwrap();
            let start = std::time::Instant::now();
            let task = app.handle(Message::RecheckInstallation);
            assert!(start.elapsed() < std::time::Duration::from_millis(150));
            assert!(app.installation_busy);
            assert!(app.desktop.islands.installation.installed());
            assert_eq!(
                app.desktop.islands.installation.status_label(),
                "Checking compatibility"
            );
            let generation = app.installation_generation;
            let duplicate = app.handle(Message::RefreshInstallation);
            settle_settings(&mut app, duplicate);
            settle_settings(&mut app, task);
            let elapsed = start.elapsed();
            eprintln!(
                "actual Settings worker: delay={delay}s elapsed={elapsed:?} timeout={timeout}"
            );
            let calls = std::fs::read_to_string(root.join("status-calls")).unwrap();
            assert_eq!(
                calls, "presence\ncompat\n",
                "one full verification per refresh"
            );
            assert!(elapsed < std::time::Duration::from_secs(7), "{elapsed:?}");
            if timeout {
                assert!(elapsed > std::time::Duration::from_secs(5));
            }
            assert!(!app.installation_busy);
            assert_eq!(app.installation_retry, timeout);
            assert!(app.desktop.islands.installation.installed());
            send_settings(
                &mut app,
                Message::InstallationChecked(
                    generation - 1,
                    crate::desktop::islands::Installation::Absent,
                    Default::default(),
                ),
            );
            assert!(app.desktop.islands.installation.installed());
            if timeout {
                send_settings(&mut app, Message::RefreshInstallation);
                assert_eq!(app.installation_generation, generation);
                assert_eq!(
                    std::fs::read_to_string(root.join("status-calls")).unwrap(),
                    calls
                );
                assert_eq!(
                    app.desktop.islands.installation.status_label(),
                    "Checking compatibility"
                );
            }
            assert!(app.native_deferred.is_empty());
            assert!(app.native_tasks.is_empty());
        }
        // A deliberate retry after the timeout is the only new full check.
        std::fs::write(root.join("status-delay"), "0").unwrap();
        send_settings(&mut app, Message::RecheckInstallation);
        assert!(!app.installation_retry);
        assert_eq!(
            std::fs::read_to_string(root.join("status-calls")).unwrap(),
            "presence\ncompat\npresence\ncompat\n"
        );
    }

    #[test]
    fn runtime_boundary_installed_pending_coalesces_and_requires_explicit_retry() {
        if desktop_test_child(
            "runtime_boundary_installed_pending_coalesces_and_requires_explicit_retry",
            DesktopEnv {
                app_config: true,
                fixture: true,
                ..Default::default()
            },
        ) {
            return;
        }
        let store = crate::desktop::DesktopStore::from_env().unwrap();
        crate::desktop::islands::fixture_installed(store.fixture_root().unwrap());
        let (mut app, initial) = Settings::init(Core::default(), Page::General);
        settle_settings(&mut app, initial);
        let task = app.handle(Message::RefreshInstallation);
        let generation = app.installation_generation;
        assert!(app.installation_busy);
        assert!(app.desktop.islands.installation.installed());
        assert_eq!(
            app.desktop.islands.installation.status_label(),
            "Checking compatibility"
        );
        let duplicate = app.handle(Message::RefreshInstallation);
        assert_eq!(app.installation_generation, generation);
        settle_settings(&mut app, duplicate);
        settle_settings(&mut app, task);
        assert!(!app.installation_busy);
        let checking = app.desktop.islands.installation.checking();
        send_settings(
            &mut app,
            Message::InstallationChecked(generation, checking, Default::default()),
        );
        assert!(app.installation_retry);
        send_settings(&mut app, Message::RefreshInstallation);
        assert_eq!(app.installation_generation, generation);
        assert!(app.desktop.islands.installation.installed());
        send_settings(
            &mut app,
            Message::InstallationChecked(
                generation - 1,
                crate::desktop::islands::Installation::Absent,
                Default::default(),
            ),
        );
        assert!(app.desktop.islands.installation.installed());
        send_settings(&mut app, Message::RecheckInstallation);
        assert_eq!(app.installation_generation, generation + 1);
        assert!(!app.installation_busy);
        assert!(!app.installation_retry);
        assert!(app.native_deferred.is_empty());
        assert!(app.native_tasks.is_empty());
    }

    #[test]
    fn task2_native_recovery_confirmation_and_installed_only_addons() {
        if desktop_test_child(
            "task2_native_recovery_confirmation_and_installed_only_addons",
            DesktopEnv {
                app_config: true,
                fixture: true,
                ..Default::default()
            },
        ) {
            return;
        }
        let store = crate::desktop::DesktopStore::from_env().unwrap();
        let key =
            crate::desktop::ConfigKey::new("com.system76.CosmicPanel.Dock", 1, "opacity").unwrap();
        let before = store.read_user(&key).unwrap();
        let mut patch = crate::desktop::DesktopPatch::new();
        patch.set(
            key.clone(),
            before.clone(),
            Some(crate::desktop::RawValue::parse("0.71").unwrap()),
        );
        store.apply(&patch).unwrap();
        let (mut app, initial) = Settings::init(Core::default(), Page::General);
        settle_settings(&mut app, initial);
        send_settings(&mut app, Message::AppSettingsPage(AppSettingsPage::Updates));
        let size = Size::new(1024., 1800.);
        send_settings(&mut app, Message::Resized(size));
        let mut native = crate::native_test::Native::new(size);
        assert!(
            native
                .try_bounds(app.settings_view(), "get-islands")
                .is_none(),
            "unverified installations cannot start a privileged package install"
        );
        native.capture(
            app.settings_view(),
            &controls_theme(&app.config),
            "task2-addons-absent",
        );
        app.desktop.islands.installation = crate::desktop::islands::Installation::Pending;
        assert!(
            native
                .try_bounds(app.settings_view(), "get-islands")
                .is_none()
        );
        crate::desktop::islands::fixture_installed(store.fixture_root().unwrap());
        send_settings(&mut app, Message::RefreshInstallation);
        assert!(
            native
                .try_bounds(app.settings_view(), "get-islands")
                .is_none()
        );
        send_settings(&mut app, Message::AppSettingsPage(AppSettingsPage::Restore));
        native_click(&mut app, &mut native, "restore-desktop");
        assert_eq!(
            store.read_user(&key).unwrap().unwrap().as_str(),
            "0.71",
            "preview is read-only"
        );
        native.capture(
            app.settings_view(),
            &controls_theme(&app.config),
            "task2-recovery-confirm",
        );
        native_click(&mut app, &mut native, "restore-desktop-confirm");
        assert_eq!(
            store.read_user(&key).unwrap().as_ref().map(|v| v.as_str()),
            before.as_ref().map(|v| v.as_str())
        );
        native.capture(
            app.settings_view(),
            &controls_theme(&app.config),
            "task2-recovery-after",
        );
        native_click(&mut app, &mut native, "undo-desktop-restore");
        assert_eq!(store.read_user(&key).unwrap().unwrap().as_str(), "0.71");
        native_click(&mut app, &mut native, "restore-desktop");
        native_click(&mut app, &mut native, "restore-desktop-confirm");
        let (mut reopened, initial) = Settings::init(Core::default(), Page::General);
        settle_settings(&mut reopened, initial);
        send_settings(&mut reopened, Message::Resized(size));
        send_settings(
            &mut reopened,
            Message::AppSettingsPage(AppSettingsPage::Restore),
        );
        native_click(&mut reopened, &mut native, "undo-desktop-restore");
        app = reopened;
        assert_eq!(store.read_user(&key).unwrap().unwrap().as_str(), "0.71");
        std::fs::remove_file(
            store
                .fixture_root()
                .unwrap()
                .join("islands-installation.json"),
        )
        .unwrap();
        send_settings(&mut app, Message::RefreshInstallation);
        send_settings(&mut app, Message::AppSettingsPage(AppSettingsPage::Updates));
        assert!(
            native
                .try_bounds(app.settings_view(), "get-islands")
                .is_none()
        );
    }
    #[test]
    fn task2r1_native_starter_thumbnail_adapts_absent_and_pending_islands() {
        if desktop_test_child(
            "task2r1_native_starter_thumbnail_adapts_absent_and_pending_islands",
            DesktopEnv {
                app_config: true,
                fixture: true,
                ..Default::default()
            },
        ) {
            return;
        }
        let (mut app, initial) = Settings::init(Core::default(), Page::DesktopProfiles);
        settle_settings(&mut app, initial);
        let preset = crate::desktop::profiles::preset("halcyon").unwrap();
        let mut native = crate::native_test::Native::new(Size::new(320., 180.));
        let store = crate::desktop::DesktopStore::from_env().unwrap();
        crate::desktop::islands::fixture_installed(store.fixture_root().unwrap());
        let installed = crate::desktop::islands::Installation::discover(&store);
        let key = crate::desktop::islands::key(crate::desktop::panels::PanelKind::Panel);
        let enabled = dock_islands_core::Settings {
            enabled: true,
            ..Default::default()
        };
        seed(
            &store.fixture_root().unwrap().join("config/cosmic"),
            &[(
                key.namespace(),
                key.key(),
                &ron::to_string(&enabled).unwrap(),
            )],
        );
        app.profiles.open();
        app.desktop.islands.installation = installed;
        let original = native.render(
            app.preset_thumbnail(&preset, 180.),
            &controls_theme(&app.config),
        );
        app.desktop.islands.installation = crate::desktop::islands::Installation::Absent;
        let absent = native.render(
            app.preset_thumbnail(&preset, 180.),
            &controls_theme(&app.config),
        );
        assert!(
            absent != original,
            "absent starter must show the standard panel instead of Islands"
        );
        native.bounds(
            app.preset_thumbnail(&preset, 180.),
            "starter-standard-panel",
        );
        app.desktop.islands.installation = crate::desktop::islands::Installation::Pending;
        let pending = native.render(
            app.preset_thumbnail(&preset, 180.),
            &controls_theme(&app.config),
        );
        assert_eq!(pending, absent);
        native.capture(
            app.preset_thumbnail(&preset, 180.),
            &controls_theme(&app.config),
            "task2r1-standard-thumbnail",
        );
    }

    include!("settings/native_queue_tests.rs");

    #[test]
    fn task2r1_slow_probe_never_runs_in_native_pointer_or_focus_completion() {
        if desktop_test_child(
            "task2r1_slow_probe_never_runs_in_native_pointer_or_focus_completion",
            DesktopEnv {
                app_config: true,
                fixture: true,
                ..Default::default()
            },
        ) {
            return;
        }
        use cosmic::iced::futures::StreamExt;
        use cosmic::iced::{Event, mouse};
        let store = crate::desktop::DesktopStore::from_env().unwrap();
        let root = store.fixture_root().unwrap();
        crate::desktop::islands::fixture_installed(root);
        let (mut app, initial) = Settings::init(Core::default(), Page::General);
        settle_settings(&mut app, initial);
        send_settings(&mut app, Message::AppSettingsPage(AppSettingsPage::Updates));
        send_settings(&mut app, Message::Resized(Size::new(1024., 1800.)));
        crate::test_env::native_worker(&root, true);
        std::fs::write(
            root.join("slow-islands-probe.py"),
            format!(
                "import time,pathlib\ntime.sleep(0.6)\nprint(pathlib.Path({:?}).read_text())\n",
                root.join("islands-installation.json").to_str().unwrap()
            ),
        )
        .unwrap();
        let mut native = crate::native_test::Native::new(Size::new(1024., 1800.));
        let rect = native_reveal(&app, &mut native, "manage-islands");
        let _ = native.event(
            app.settings_view(),
            Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)),
            rect.center(),
        );
        let messages = native
            .event(
                app.settings_view(),
                Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)),
                rect.center(),
            )
            .0;
        assert!(!messages.is_empty());
        let mut pending = Vec::new();
        for message in messages {
            let start = Instant::now();
            pending.push(app.update(message));
            assert!(
                start.elapsed() < Duration::from_millis(250),
                "native navigation waited for the helper"
            );
        }
        for task in pending {
            settle_settings(&mut app, task);
        }
        assert_eq!(app.page, Page::DesktopDock);
        let rect = native_reveal(&app, &mut native, "native-islands-style");
        assert!(rect.y + rect.height <= native.size.height);
        let _ = native.event(
            app.settings_view(),
            Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)),
            rect.center(),
        );
        let messages = native
            .event(
                app.settings_view(),
                Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)),
                rect.center(),
            )
            .0;
        assert!(!messages.is_empty());
        for message in messages {
            let start = Instant::now();
            let task = app.update(message);
            assert!(
                start.elapsed() < Duration::from_millis(250),
                "native style action waited for pre-write discovery"
            );
            // Another ordinary control must not enter the config lock while
            // this window's worker can be checking installation under it.
            let before = Config::load().clock.show_date;
            let start = Instant::now();
            let preference = app.update(Message::ClockDate(!before));
            assert!(start.elapsed() < Duration::from_millis(250));
            assert_eq!(
                Config::load().clock.show_date,
                before,
                "preference waits behind the active native transaction"
            );
            settle_settings(&mut app, Task::batch([task, preference]));
            assert_eq!(Config::load().clock.show_date, !before);
        }
        let key = crate::desktop::ConfigKey::new(
            dock_islands_core::NAMESPACE,
            1,
            app.desktop.selected.entry(),
        )
        .unwrap();
        assert!(
            store.read_user(&key).unwrap().is_some(),
            "rendered Islands choice must complete its Apply transaction"
        );
        // Installation disappears after the worker has accepted the displayed
        // choice, before its guarded commit. Cached UI capability is no authority.
        std::fs::remove_file(store.user_path(&key)).unwrap();
        app.desktop.refresh();
        let prior = store.latest_change().unwrap().map(|v| v.id);
        crate::test_env::native_worker(&root, true);
        std::fs::write(root.join("slow-islands-probe.py"),format!("import time,pathlib\ntime.sleep(0.6)\nc=pathlib.Path({:?})\nn=int(c.read_text())+1 if c.exists() else 1\nc.write_text(str(n))\np=pathlib.Path({:?})\nif n==3: p.write_text('{{\"format\":\"halcyon-islands-installation\",\"version\":1,\"installation\":\"absent\",\"enabled\":false}}')\nprint(p.read_text())\n",root.join("probe-count").to_str().unwrap(),root.join("islands-installation.json").to_str().unwrap())).unwrap();
        let rect = native_reveal(&app, &mut native, "native-islands-style");
        let _ = native.event(
            app.settings_view(),
            Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)),
            rect.center(),
        );
        let messages = native
            .event(
                app.settings_view(),
                Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)),
                rect.center(),
            )
            .0;
        assert!(!messages.is_empty());
        for message in messages {
            let start = Instant::now();
            let task = app.update(message);
            assert!(start.elapsed() < Duration::from_millis(250));
            settle_settings(&mut app, task);
        }
        assert!(
            std::fs::read_to_string(root.join("probe-count"))
                .unwrap()
                .parse::<u32>()
                .unwrap()
                >= 3
        );
        assert!(
            store.read_user(&key).unwrap().is_none(),
            "removal before commit must refuse the sidecar"
        );
        assert_eq!(store.latest_change().unwrap().map(|v| v.id), prior);
        assert!(matches!(
            app.desktop.notice,
            Some(desktop::Notice::Error(_))
        ));
        let start = Instant::now();
        let task = app.update(Message::RefreshInstallation);
        assert!(start.elapsed() < Duration::from_millis(250));
        // Collect the real background response, then dispatch it as the native
        // event loop does. Result handling itself must not launch another probe.
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_time()
            .build()
            .unwrap();
        let messages = runtime.block_on(async {
            let mut output = Vec::new();
            if let Some(mut stream) = cosmic::iced::runtime::task::into_stream(task) {
                while let Some(action) = stream.next().await {
                    if let cosmic::iced::runtime::Action::Output(cosmic::Action::App(message)) =
                        action
                    {
                        output.push(message);
                    }
                }
            }
            output
        });
        for message in messages {
            let start = Instant::now();
            let task = app.update(message);
            assert!(
                start.elapsed() < Duration::from_millis(250),
                "focus completion repeated discovery on the native thread"
            );
            settle_settings(&mut app, task);
        }
        // Closing waits for the asynchronous material completion rather than
        // dropping the final adjustment when the app exits.
        app.material_target = crate::materials::Target::Widgets;
        let finish = crate::materials::Finish::new(crate::materials::Kind::Frosted);
        app.material_draft = Some(finish);
        let start = Instant::now();
        let exit = app.on_app_exit();
        assert!(start.elapsed() < Duration::from_millis(250));
        assert!(
            exit.is_some(),
            "exit must wait for its pending native write"
        );
        send_settings(&mut app, exit.unwrap());
        assert_eq!(Config::load().materials.widgets, Some(finish));
        assert!(app.on_app_exit().is_none());
    }
}

impl Settings {
    fn apply_material(&mut self, finish: Option<crate::materials::Finish>) {
        let baseline = self.material_baseline.take();
        if self.native_worker_needed() {
            self.background_material(self.material_target, finish, baseline);
            return;
        }
        let result = crate::desktop::actions::history_store()
            .map_err(|e| e.to_string())
            .and_then(|store| {
                crate::desktop::materials::apply_from(
                    &store,
                    self.material_target,
                    finish,
                    baseline,
                )
            });
        match result {
            Ok(()) => {
                self.config = Config::load();
                self.saved();
            }
            Err(error) => self.failed(error),
        }
    }
    fn save_material_draft(&mut self) {
        if let Some(finish) = self.material_draft.take() {
            self.apply_material(Some(finish));
        }
    }
}
