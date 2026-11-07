//! Settings ownership, search and the shared pending/Undo controls.
#[cfg(test)]
pub(super) use super::preference_undo::PreferenceUndo;
use super::{AppSettingsPage, Message, Page, Settings};
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
}

impl SearchHit {
    pub(super) fn app_destination(self) -> AppSettingsPage {
        self.destination
    }
    /// Which object tab holds this control.
    pub(super) fn object_tab(self) -> ObjectTab {
        match self.anchor {
            "widget-design" | "dock-advanced" | "music-visualizers" | "music-spectrum" => {
                ObjectTab::Design
            }
            _ => ObjectTab::Content,
        }
    }
    pub fn anchor(self) -> &'static str {
        self.anchor
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

pub(super) const WIDGET_READABILITY: SearchHit = hit(
    Page::Appearance,
    "Widget readability",
    "contrast readability bright wallpaper readable text",
    true,
)
.at("contrast-assist")
.open(SearchOpen::Readability);

pub(super) const LOOKS: SearchHit = hit(
    Page::DesktopProfiles,
    "Looks",
    "setups profiles presets starter looks saved calm evening focus workbench paper rose clean desktop presentation welcome rain snow fog weather",
    false,
);
pub(super) const STARTUP: SearchHit = hit(
    Page::General,
    "Startup and background",
    "general halcyon app settings startup login background service setup again",
    false,
);
pub(super) const ACTIVITY_ISLAND: SearchHit = hit(
    Page::DesktopDock,
    "Activity island",
    "activity island notifications do not disturb dnd camera microphone privacy bluetooth meetings music media",
    false,
)
.at("panel-items")
.bar(crate::desktop::panels::PanelKind::Panel);
pub(super) const SEE_THROUGH: SearchHit = hit(
    Page::Appearance,
    "See-through widgets",
    "transparent transparency see-through see through opacity clear glass",
    false,
)
.at("material-scope")
.open(SearchOpen::Transparency);

pub(super) const PRIVACY: SearchHit = hit(
    Page::General,
    "Privacy and usage reporting",
    "privacy usage reporting anonymous telemetry",
    false,
)
.destination(AppSettingsPage::Privacy);

const SEARCH: &[SearchHit] = &[
    hit(
        Page::General,
        "Help and diagnostics",
        "help diagnostics support problem report status troubleshoot",
        false,
    )
    .destination(AppSettingsPage::Help),
    hit(
        Page::Changes,
        "Changes",
        "why did my desktop change history automatic schedule undo recent changes log",
        false,
    ),
    hit(
        Page::Themes,
        "Shared desktop themes",
        "theme soft outline paper bare colors widgets dock panel style save share",
        false,
    ),
    WIDGET_READABILITY,
    hit(
        Page::General,
        "Settings animations",
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
        "Reorder readings",
        "system monitor devbox reorder readings drag move order",
        false,
    )
    .at("devbox-reorder-anchor"),
    hit(
        Page::Weather,
        "City and forecast",
        "weather location city forecast belgrade",
        false,
    )
    .open(SearchOpen::City),
    hit(
        Page::Weather,
        "Dew point",
        "weather dew point dewpoint humidity moisture temperature daily overview",
        // The switch sits in the Weather page's folded "Dew point and how
        // often it updates" section; search opens it.
        true,
    )
    .at("weather-dew-point"),
    hit(
        Page::Weather,
        "Weather update interval",
        "weather refresh update interval frequency",
        true,
    ),
    hit(
        Page::Devbox,
        "CPU and temperature",
        "system monitor devbox cpu temperature readings history graph sparkline",
        false,
    )
    .at("devbox-cpu")
    .open(SearchOpen::Metric(crate::config::DevboxSection::Cpu)),
    hit(
        Page::Devbox,
        "Memory usage",
        "system monitor devbox ram memory cache swap readings history graph sparkline",
        false,
    )
    .at("devbox-memory")
    .open(SearchOpen::Metric(crate::config::DevboxSection::Memory)),
    hit(
        Page::Devbox,
        "Disk usage",
        "system monitor devbox disk storage readings history graph sparkline",
        false,
    )
    .at("devbox-disk")
    .open(SearchOpen::Metric(crate::config::DevboxSection::Disk)),
    hit(
        Page::Devbox,
        "Network activity",
        "system monitor devbox network speed readings history graph sparkline",
        false,
    )
    .at("devbox-network")
    .open(SearchOpen::Metric(crate::config::DevboxSection::Network)),
    hit(
        Page::Devbox,
        "GPU readings",
        "system monitor devbox gpu graphics readings history graph sparkline",
        false,
    )
    .at("devbox-gpu")
    .open(SearchOpen::Metric(crate::config::DevboxSection::Gpu)),
    hit(
        Page::Devbox,
        "System monitor update interval",
        "system monitor devbox refresh update interval",
        true,
    ),
    // Before the card finish: transparency is what someone searching it wants.
    SEE_THROUGH,
    hit(
        Page::Appearance,
        "Widget card finish",
        "appearance widget opacity corners border outline card style finish",
        false,
    ),
    hit(
        Page::Overview,
        "Arrange widgets",
        "layout drag drop move resize size width height placement position grid snap guides arrange",
        false,
    )
    .at("canvas-start"),
    hit(
        Page::Calendar,
        "Today, tomorrow and meetings",
        "calendar today tomorrow meetings join highlights",
        false,
    ),
    hit(
        Page::Calendar,
        "Calendar update interval",
        "calendar sync refresh update interval account",
        true,
    ),
    hit(
        Page::Clock,
        "Time and date",
        "clock time date hour format 12 24",
        false,
    ),
    hit(
        Page::Clock,
        "Face and typography",
        "clock size scale alignment center analog digital face typography weight italic surface design font depth landscape",
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
        "Overview design",
        "at a glance daily overview font background surface layout density opacity design",
        false,
    )
    .at("widget-design"),
    hit(
        Page::Projects,
        "Project folders and servers",
        "projects branches repositories worktrees folders servers",
        false,
    ),
    hit(
        Page::Music,
        "Music player",
        "music audio media player artwork progress",
        false,
    ),
    hit(
        Page::Music,
        "Audio visualizers",
        "music audio visualizer visualiser radial bars orbit dots pulse rings",
        false,
    )
    .at("music-visualizers"),
    hit(
        Page::Music,
        "Spectrum",
        "music audio spectrum analyzer bars sound visualizer",
        false,
    )
    .at("music-spectrum"),
    hit(
        Page::Widgets,
        "Widgets",
        "widgets show hide turn on off cards my widgets",
        false,
    )
    .at("desktop-widgets"),
    hit(
        Page::Notes,
        "Notes and checklists",
        "notes notebook text checklist",
        false,
    ),
    hit(
        Page::Focus,
        "Focus timer",
        "focus timer pomodoro breaks notifications",
        false,
    ),
    hit(
        Page::Themes,
        "Desktop theme",
        "system appearance desktop theme dark light automatic",
        false,
    ),
    hit(
        Page::DesktopAppearance,
        "Fonts and icon theme",
        "desktop font typeface typography icons icon theme",
        false,
    )
    .at("appearance-fonts"),
    hit(
        Page::DesktopAppearance,
        "Custom surface colors",
        "desktop surface background window color",
        true,
    )
    .at("appearance-colors"),
    hit(
        Page::DesktopWindows,
        "Window corners, outline and shadows",
        "desktop effects blur frosted glass window corners outline shadows roundness",
        true,
    )
    .at("appearance-effects")
    .open(SearchOpen::Effects),
    hit(
        Page::Clock,
        "Clock update interval",
        "clock refresh update interval frequency",
        true,
    ),
    hit(
        Page::Projects,
        "Projects update interval",
        "projects refresh update interval frequency",
        true,
    ),
    hit(
        Page::DesktopDock,
        "Dock timing and spacing",
        "dock timing padding spacing corners hide delay slide speed",
        true,
    )
    .at("dock-advanced")
    .bar(crate::desktop::panels::PanelKind::Dock),
    hit(
        Page::DesktopDock,
        "Panel timing and spacing",
        "panel timing padding spacing corners hide delay slide speed",
        true,
    )
    .at("dock-advanced")
    .bar(crate::desktop::panels::PanelKind::Panel),
    hit(
        Page::DesktopWallpaper,
        "Wallpaper effects",
        "rain snow fog cloud shadows living wallpaper droplets flakes opacity mist wind refraction animation",
        false,
    )
    .at("rain-controls"),
    hit(
        Page::DesktopWallpaper,
        "Wallpaper and colors",
        "wallpaper background pictures slideshow gradient widget color colors palette accent sync automatic follow desktop pop os",
        false,
    ),
    hit(
        Page::DesktopIcons,
        "Icons on the desktop",
        "desktop files icons trash home mounted drives size spacing",
        false,
    ),
    hit(
        Page::DesktopDock,
        "Dock",
        "dock applets position size autohide penumbra gradient",
        false,
    )
    .bar(crate::desktop::panels::PanelKind::Dock),
    hit(
        Page::DesktopDock,
        "Panel and Islands",
        "top bar panel islands applets position size autohide",
        false,
    )
    .bar(crate::desktop::panels::PanelKind::Panel),
    ACTIVITY_ISLAND,
    hit(
        Page::DesktopWindows,
        "Windows and tiling",
        "windows tiling gaps focus controls snapping",
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
    ),
    hit(
        Page::DesktopShortcuts,
        "Keyboard shortcuts",
        "shortcuts keyboard keys commands bindings hotkeys",
        false,
    ),
    hit(
        Page::DesktopMotion,
        "Animations in supported apps",
        "accessibility motion animations gtk reduced",
        false,
    ),
    LOOKS,
    hit(
        Page::Builder,
        "Make your own widget",
        "recipe custom clock weather calendar agenda cpu memory disk network gpu uptime system readings native parts compose widget builder",
        false,
    ),
    hit(
        Page::DesktopAutomation,
        "Time of day",
        "change with time of day day and night style widget style moments clock night style adaptive daily cycle day evening night moments automation schedule automatic time displays monitors rules preset sun solar dawn sunrise noon golden hour sunset city timezone",
        false,
    ),
    STARTUP,
    hit(
        Page::General,
        "Widget controls",
        "widget controls sidebar screen edge menu side panel slide delay animation",
        false,
    )
    .destination(AppSettingsPage::Interface),
    PRIVACY,
    hit(
        Page::General,
        "Updates and installation",
        "update version installation islands addon add-on diagnostics check",
        false,
    )
    .destination(AppSettingsPage::Updates),
    hit(
        Page::General,
        "What’s new",
        "release notes new features changes changelog version whats",
        false,
    )
    .destination(AppSettingsPage::WhatsNew),
    hit(
        Page::Changes,
        "Restore previous desktop",
        "recovery restore reset previous wallpaper hide widgets earlier setups",
        false,
    )
    .at("changes-earlier"),
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
    }
}

/// Search stays entirely local. Whole words and prefixes come before aliases
/// and spelling corrections; a typo never displaces a literal result.
pub fn search(query: &str) -> Vec<SearchHit> {
    use std::cmp::Reverse;

    let mut query = search_words(query);
    if query.is_empty() {
        return Vec::new();
    }
    // Repeating a word must not inflate its relevance.
    let mut seen = std::collections::HashSet::new();
    query.retain(|word| seen.insert(word.clone()));
    let phrase = query.join(" ");
    let mut matches = Vec::new();
    for (index, entry) in SEARCH.iter().copied().enumerate() {
        let label = search_words(entry.label);
        let page = search_words(entry.page.title());
        let terms = search_words(entry.terms);
        let parent = search_words(entry.page.nav().title());
        let fields = [(&label, 12), (&page, 8), (&terms, 6), (&parent, 2)];
        let mut corrections = 0;
        let mut aliases = 0;
        let mut prefixes = 0;
        let mut relevance = 0;
        let mut complete = true;
        for word in &query {
            let best = fields
                .iter()
                .flat_map(|(tokens, weight)| {
                    tokens.iter().filter_map(move |token| {
                        word_match(word, token)
                            .map(|(typo, alias, prefix)| (typo, alias, prefix, Reverse(*weight)))
                    })
                })
                .min();
            if let Some((typo, alias, prefix, Reverse(weight))) = best {
                corrections += usize::from(typo);
                aliases += usize::from(alias);
                prefixes += usize::from(prefix);
                relevance += weight;
            } else {
                complete = false;
                break;
            }
        }
        if !complete {
            continue;
        }
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
        matches.push((
            (
                corrections,
                aliases,
                prefixes,
                Reverse(relevance),
                entry.advanced,
                index,
            ),
            entry,
        ));
    }
    matches.sort_unstable_by_key(|(rank, _)| *rank);
    matches.into_iter().take(10).map(|(_, hit)| hit).collect()
}

fn search_words(text: &str) -> Vec<String> {
    text.split(|character: char| !character.is_alphanumeric())
        .filter(|word| !word.is_empty())
        .map(str::to_lowercase)
        .collect()
}

/// (spelling correction, synonym, prefix). Short technical names such as CPU
/// and RAM are never fuzzy matched, and every query word must find a match.
fn word_match(word: &str, token: &str) -> Option<(bool, bool, bool)> {
    if token == word {
        return Some((false, false, false));
    }
    if token.starts_with(word) {
        return Some((false, false, true));
    }
    let aliases: &[&str] = match word {
        "transparent" | "transparency" => &["opacity"],
        "colour" => &["color"],
        "colours" => &["colors"],
        "autostart" | "boot" => &["startup"],
        "hotkey" | "hotkeys" => &["shortcuts"],
        "wallpapers" => &["wallpaper"],
        _ => &[],
    };
    for alias in aliases {
        if token.starts_with(alias) {
            return Some((false, true, token != *alias));
        }
    }
    one_typo(word, token).then_some((true, false, false))
}

/// One insertion, deletion, replacement or adjacent transposition. Restrict
/// corrections to words of at least four characters to avoid noisy matches
/// for acronyms, numbers and partially typed short queries.
fn one_typo(left: &str, right: &str) -> bool {
    if !left.is_ascii() || !right.is_ascii() {
        return false;
    }
    let (left, right) = (left.as_bytes(), right.as_bytes());
    if left.len().min(right.len()) < 4 || left.len().abs_diff(right.len()) > 1 {
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
            Self::Themes => "Choose a theme to preview it here",
            Self::DesktopWallpaper => "Choose a wallpaper to preview it here",
            Self::DesktopProfiles => "Choose a setup to see what it includes",
            Self::Notes => "",
            _ => "",
        }
    }
}

pub fn latest_desktop_change() -> Option<(crate::desktop::Owner, String, String)> {
    crate::desktop::actions::history_store()
        .ok()?
        .latest_change()
        .ok()?
        .map(|change| (change.owner, change.label, change.id))
}

impl Settings {
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
                self.last_desktop_change = latest_desktop_change();
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
                #[cfg(test)]
                {
                    self.preference_undo = None;
                }
            }
            Err(error) => self.failed(error.to_string()),
        }
        self.last_desktop_change = latest_desktop_change();
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
                self.undo_review = None;
                self.error = false;
                self.status.clear();
            }
            Err(error) => self.failed(error.to_string()),
        }
        self.last_desktop_change = latest_desktop_change();
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
        assert!(search("nonexistent-thing").is_empty());
    }

    #[test]
    fn search_ranks_specific_controls_and_common_page_options_first() {
        assert_eq!(search("memory usage")[0].label, "Memory usage");
        assert_eq!(search("calendar")[0].label, "Today, tomorrow and meetings");
        assert_eq!(search("weather")[0].label, "City and forecast");
        assert_eq!(search("clock")[0].label, "Time and date");
        assert_eq!(search("time")[0].label, "Time and date");
        assert_eq!(
            search("calendar update")[0].label,
            "Calendar update interval"
        );
        let colors = search("colors");
        assert_eq!(colors[0].page, Page::DesktopWallpaper);
        assert!(
            colors
                .iter()
                .any(|hit| hit.label == "Custom surface colors")
        );
        assert_eq!(colors, search("colors"));
    }

    #[test]
    fn search_accepts_common_typos_without_fuzzy_acronyms_or_unrelated_words() {
        for query in ["calender", "calndar", "calendarr", "clalendar", "claendar"] {
            assert_eq!(
                search(query)[0].label,
                "Today, tomorrow and meetings",
                "{query}"
            );
        }
        assert_eq!(search("netwrok")[0].label, "Network activity");
        assert_eq!(
            search("calender updat")[0].label,
            "Calendar update interval"
        );
        for query in [
            "cpx",
            "gpx",
            "zxyw",
            "caaaalendar",
            "calendar banana",
            "---",
            "🪴",
        ] {
            assert!(search(query).is_empty(), "{query}");
        }
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
        assert_eq!(search("autostart")[0].label, "Startup and background");
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
        assert_eq!(search("GPU-readings")[0].label, "GPU readings");
        assert_eq!(search("  MEMORY / cache  ")[0].label, "Memory usage");
        assert_eq!(
            search("calendar update"),
            search("calendar calendar update")
        );
        // Builder now supports both Calendar and GPU parts; use an actually
        // unsupported second term to verify that search retains every word.
        assert!(search("calendar bluetooth").is_empty());
        assert_eq!(search("calendar agenda")[0].page, Page::Builder);
        assert_eq!(search("gpu recipe")[0].page, Page::Builder);
        assert!(search("calendar mystery option").is_empty());
        assert!(search("desktop").len() <= 10);
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
        let hits = search("history");
        assert_eq!(hits.len(), 6);
        assert_eq!(
            hits.iter()
                .filter(|hit| hit.page == Page::Devbox && !hit.advanced)
                .count(),
            5
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
    setup_requested: bool,
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
            setup_requested: self.setup_requested,
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
            Page::Overview if self.setup_requested => Some(Page::Overview),
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
                Some("Setups".into())
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
        self.setup_requested = route.setup_requested;
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
        if self.page == Page::Notes && self.tools.dirty() {
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
        if self.tools.dirty() {
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
                    self.profiles.profile().map_or("Setup", |p| p.name.as_str()),
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
                "Schedule",
                Page::DesktopProfiles,
                false,
                KeptTarget::Page(Page::DesktopAutomation),
            );
        }
        if self.window_rules.dirty() {
            push(
                "Floating apps",
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
                "Desktop icons",
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
