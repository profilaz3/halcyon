//! Settings ownership, search and the shared pending/Undo controls.
#[cfg(test)]
pub(super) use super::preference_undo::PreferenceUndo;
use super::{AppSettingsPage, Message, Page, Settings};
use cosmic::app::Task;

/// What a page changes, used by navigation and search-result context.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Area {
    Appearance,
    Widgets,
    Desktop,
    Setups,
    Accessibility,
    App,
}

impl Area {
    pub fn label(self) -> &'static str {
        match self {
            Self::Appearance => "Appearance",
            Self::Setups => "Setups",
            Self::Accessibility => "Accessibility",
            Self::Widgets => "Widgets",
            Self::Desktop => "Desktop",
            Self::App => "App settings",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(super) enum SearchOpen {
    #[default]
    None,
    City,
    Effects,
    Recent,
    Readability,
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
    pub(super) open: SearchOpen,
}

impl SearchHit {
    pub(super) fn app_destination(self) -> AppSettingsPage {
        self.destination
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
}

impl Settings {
    pub(super) fn prepare_search_control(&mut self, hit: SearchHit) {
        if hit.open == SearchOpen::Readability {
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

const SEARCH: &[SearchHit] = &[
    hit(
        Page::General,
        "Help & diagnostics",
        "help diagnostics support problem report status troubleshoot",
        false,
    )
    .destination(AppSettingsPage::Help),
    hit(
        Page::General,
        "Recent changes",
        "why did my desktop change history automatic schedule undo recent changes",
        false,
    )
    .destination(AppSettingsPage::Restore)
    .open(SearchOpen::Recent),
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
        Page::Devbox,
        "Reorder readings",
        "system monitor devbox reorder readings drag move order",
        false,
    )
    .at("devbox-reorder-anchor"),
    hit(
        Page::Weather,
        "City & forecast",
        "weather location city forecast belgrade",
        false,
    )
    .open(SearchOpen::City),
    hit(
        Page::Weather,
        "Dew point",
        "weather dew point dewpoint humidity moisture temperature daily overview",
        false,
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
        "CPU & temperature",
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
    ),
    hit(
        Page::Calendar,
        "Today, tomorrow & meetings",
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
        "Time & date",
        "clock time date hour format 12 24 size scale alignment center position resize analog digital face typography weight italic surface",
        false,
    ),
    hit(
        Page::Glance,
        "At a glance",
        "at a glance daily overview combined separate cards clock weather calendar font background",
        false,
    ),
    hit(
        Page::Projects,
        "Project folders & servers",
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
        Page::Notes,
        "Notes & checklists",
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
        Page::DesktopAppearance,
        "Desktop theme",
        "system appearance desktop theme dark light automatic",
        false,
    ),
    hit(
        Page::DesktopAppearance,
        "Fonts & icon theme",
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
        Page::DesktopAppearance,
        "Desktop effects",
        "desktop effects blur frosted glass",
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
        "Dock timing & spacing",
        "dock panel timing padding spacing corners hide delay slide speed",
        true,
    )
    .at("dock-advanced"),
    hit(
        Page::DesktopWallpaper,
        "Wallpaper & colors",
        "wallpaper background pictures slideshow gradient widget color colors palette accent sync automatic follow desktop pop os",
        false,
    ),
    hit(
        Page::DesktopIcons,
        "Desktop files & icons",
        "desktop files icons trash home mounted drives size spacing",
        false,
    ),
    hit(
        Page::DesktopDock,
        "Dock & panel",
        "dock top bar panel applets position size autohide",
        false,
    ),
    hit(
        Page::DesktopWindows,
        "Windows & tiling",
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
        "Floating-app rules",
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
    hit(
        Page::DesktopProfiles,
        "Starter looks & saved setups",
        "setups profiles presets starter looks saved calm evening focus workbench paper rose clean desktop presentation welcome",
        false,
    ),
    hit(
        Page::Builder,
        "Personal widget builder",
        "recipe custom clock weather calendar agenda cpu memory disk network gpu uptime system readings native parts compose widget builder",
        false,
    ),
    hit(
        Page::AdaptiveLooks,
        "Adaptive looks",
        "adaptive tidal paper moss day evening night solar daily cycle appearance",
        false,
    ),
    hit(
        Page::Moments,
        "Day & Night moments",
        "moments day night widget clock style scheduled",
        false,
    ),
    hit(
        Page::DesktopAutomation,
        "Schedules & display rules",
        "automation schedule automatic time displays monitors rules preset sun solar dawn sunrise noon golden hour sunset city timezone",
        false,
    ),
    hit(
        Page::General,
        "Startup & background",
        "general app settings startup login widgets background service",
        false,
    ),
    hit(
        Page::General,
        "Edge menu",
        "sidebar edge menu side delay animation",
        false,
    )
    .destination(AppSettingsPage::Interface),
    hit(
        Page::General,
        "Privacy & usage reporting",
        "privacy usage reporting anonymous telemetry",
        false,
    )
    .destination(AppSettingsPage::Privacy),
    hit(
        Page::General,
        "Updates & installation",
        "update version installation islands addon add-on diagnostics check",
        false,
    )
    .destination(AppSettingsPage::Updates),
    hit(
        Page::General,
        "Restore previous desktop",
        "recovery restore reset previous wallpaper hide widgets",
        false,
    )
    .destination(AppSettingsPage::Restore),
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
        match self {
            Self::DesktopAppearance | Self::Appearance | Self::DesktopWallpaper => {
                Some(Self::Themes)
            }
            Self::DesktopAutomation | Self::Moments => Some(Self::DesktopProfiles),
            Self::PendingChanges => Some(Self::General),
            Self::DesktopWindowRules | Self::DesktopWorkspaces => Some(Self::DesktopWindows),
            Self::DesktopWindows | Self::DesktopIcons | Self::DesktopShortcuts => {
                Some(Self::DesktopDock)
            }
            page if page.sidebar() == Self::Widgets && page != Self::Widgets => Some(Self::Widgets),
            _ => None,
        }
    }
    /// The sidebar group a page belongs to, which also says what it changes.
    pub(super) fn area(self) -> Area {
        // The menu group a page sits in.
        match self.sidebar() {
            Self::Themes => Area::Appearance,
            Self::DesktopProfiles => Area::Setups,
            Self::DesktopMotion => Area::Accessibility,
            Self::General => Area::App,
            Self::Widgets => Area::Widgets,
            _ => Area::Desktop,
        }
    }
    pub(super) fn save_hint(self) -> &'static str {
        match self {
            Self::Overview => "Drag to move · drag a corner to resize",
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
                self.profiles.session_draft().is_some(),
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
                SearchOpen::Recent => assert!(app.support.recent_open),
                SearchOpen::Readability => assert!(app.material_adjust),
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
            assert!(entries.contains(&page.sidebar()), "{page:?}");
            let destination = match page {
                Page::WidgetLayout => Page::Overview,
                Page::Desktop => Page::DesktopDock,
                Page::DesktopWorkspaces => Page::DesktopWindows,
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
        assert_eq!(Page::parse("desktop"), Page::DesktopDock);
        assert_eq!(Page::Calendar.parent(), Some(Page::Widgets));
        assert_eq!(Page::DesktopDock.parent(), None);
        assert_eq!(Page::DesktopWorkspaces.sidebar(), Page::DesktopDock);
        // Every menu entry is its own destination; only detailed editors go Back.
        for page in entries {
            assert_eq!(page.sidebar(), page, "{page:?}");
            assert_eq!(page.parent(), None, "{page:?}");
        }
        assert_eq!(Page::DesktopAppearance.parent(), Some(Page::Themes));
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
        assert_eq!(search("calendar")[0].label, "Today, tomorrow & meetings");
        assert_eq!(search("weather")[0].label, "City & forecast");
        assert_eq!(search("clock")[0].label, "Time & date");
        assert_eq!(search("time")[0].label, "Time & date");
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
                "Today, tomorrow & meetings",
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
            assert_eq!(hit.anchor(), "page-start");
        }
        for query in [
            "colours",
            "accent",
            "palette",
            "wallpaper colors",
            "widget colors",
        ] {
            let hit = search(query)[0];
            assert_eq!(hit.page, Page::DesktopWallpaper, "{query}");
            assert!(!hit.advanced);
        }
        assert_eq!(search("autostart")[0].label, "Startup & background");
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
        assert!(search("calendar gpu").is_empty());
        assert!(search("calendar mystery option").is_empty());
        assert!(search("desktop").len() <= 10);
    }

    #[test]
    fn layout_queries_reach_the_home_editor() {
        for query in ["layout", "arrange widgets", "resize widgets", "grid snap"] {
            let hit = search(query)[0];
            assert_eq!(hit.page, Page::Overview, "{query}");
            assert_eq!(hit.anchor(), "page-start");
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
                .any(|hit| hit.page == Page::General && hit.label == "Recent changes")
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

/// Each root remembers its last leaf for this window; reopening still uses the
/// existing durable last-destination key, without changing app configuration.
pub(super) fn root_destinations(page: Page) -> [Page; 6] {
    let mut pages = [
        Page::Themes,
        Page::DesktopDock,
        Page::Widgets,
        Page::DesktopProfiles,
        Page::DesktopMotion,
        Page::General,
    ];
    pages[page.root_index()] = page.canonical();
    pages
}

pub(super) fn app_destination() -> AppSettingsPage {
    crate::desktop::DesktopStore::from_env()
        .ok()
        .and_then(|store| {
            std::fs::read_to_string(store.state_dir().join("settings-app-destination")).ok()
        })
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
