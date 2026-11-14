// Native layer-shell setup adapted from longer54/pop-os-cosmic-widgets (GPL-3.0-or-later).
#[cfg(test)]
use crate::config::Location;
use crate::config::{Appearance, Config, Palette};
use crate::data::{
    self, CalendarData, CalendarEvent, MeetingState, Metrics, ProjectsData, Sampler, Weather,
    WeatherSnapshot,
};
use crate::drawer::{self, DragResult, ReorderDrag, Sidebar, Widget};
#[cfg(test)]
mod auditfix_tests;
mod autoswitch;
mod calendar_details;
mod clock;
mod depth;
#[cfg(test)]
mod desktop_ux_tests;
mod devbox;
#[cfg(test)]
mod display_tests;
mod edge;
mod edge_compact;
mod glance;
mod monitors;
mod recipe;
mod restart;
#[cfg(test)]
mod surface_lifecycle_tests;

pub(crate) fn settings_clock_preview(
    config: &Config,
    size: [f32; 2],
) -> cosmic::Element<'static, Message> {
    settings_clock_preview_at(config, size, chrono::Local::now().naive_local())
}

pub(crate) fn settings_clock_preview_at(
    config: &Config,
    size: [f32; 2],
    now: chrono::NaiveDateTime,
) -> cosmic::Element<'static, Message> {
    clock::view_context(
        config,
        now,
        size,
        crate::widget_presentation::Context::LocalPreview,
    )
}

pub(crate) fn settings_recipe_preview(
    config: &Config,
    recipe: &crate::recipes::Recipe,
    missing: bool,
    size: [f32; 2],
) -> cosmic::Element<'static, Message> {
    if recipe.tool() {
        return crate::tools::cards::recipe::sample(
            config,
            recipe,
            crate::tools::cards::recipe::PreviewState::Active,
            size,
        )
        .map(|_| Message::Preview);
    }
    let mut config = config.clone();
    config.recipes.set(recipe.kind, Some(recipe.clone()));
    config.location = (!missing).then(|| crate::config::Location {
        name: "Sample city".into(),
        latitude: 0.,
        longitude: 0.,
        timezone: None,
    });
    let now = chrono::Local::now();
    if recipe.kind == crate::recipes::Kind::Clock {
        clock::view_context(
            &config,
            chrono::NaiveDate::from_ymd_opt(2026, 7, 21)
                .unwrap()
                .and_hms_opt(9, 41, 0)
                .unwrap(),
            size,
            crate::widget_presentation::Context::LocalPreview,
        )
    } else if recipe.kind == crate::recipes::Kind::Calendar {
        recipe::calendar::sample(&config, recipe, missing, size)
    } else if recipe.kind == crate::recipes::Kind::System {
        recipe::system::sample(&config, recipe, missing, size)
    } else if missing {
        weather::Presentation::new(
            &config,
            weather::Snapshot {
                weather: None,
                updated_at: None,
                failed: false,
            },
            now,
        )
        .view(crate::widget_presentation::Context::LocalPreview, size)
    } else {
        settings_weather_preview_at(&config, size, now)
    }
}

pub(crate) fn settings_clock_face_rows(
    config: &Config,
    size: [f32; 2],
    now: chrono::NaiveDateTime,
) -> [f32; 2] {
    clock::Presentation::new(config)
        .frame(now, size)
        .face_rows()
}
mod activity;
mod history;
mod layout;
mod panel;
mod projects;
mod providers;
mod settings_preview;
pub(crate) use edge::{
    availability as edge_availability, item_icon as edge_item_icon, item_title as edge_item_title,
};
pub(crate) use settings_preview::{
    SettingsSamples, settings_devbox_preview, settings_projects_preview, settings_weather_preview,
    settings_weather_preview_at, settings_weather_preview_sized,
};
mod glass;
#[cfg(test)]
mod look_render_tests;
mod rain;
#[cfg(test)]
mod theme_desktop_tests;
#[cfg(test)]
mod theme_tests;
mod tools;
#[cfg(test)]
mod utility_tests;
mod visualizer;
mod wallpaper_colors;
mod weather;
use chrono::{Datelike, Duration as Days, Local, NaiveDate};
use cosmic::Application as _;
use cosmic::Element;
use cosmic::app::{Core, Task};
use cosmic::iced::platform_specific::runtime::wayland::layer_surface::{
    IcedMargin, IcedOutput, SctkLayerSurfaceSettings,
};
use cosmic::iced::platform_specific::shell::commands::layer_surface::{
    Anchor, KeyboardInteractivity, Layer, get_layer_surface, set_keyboard_interactivity, set_margin,
};
use cosmic::iced::{Alignment, Background, Border, Color, Font, Length, Subscription, window};
use cosmic::widget::{column, container, mouse_area, row, scrollable, text};
use std::time::{Duration, Instant, SystemTime};

#[derive(Debug, Clone)]
pub enum Message {
    Preview,
    Activity(crate::activity_island::bridge::Event),
    Tick,
    /// The wake for this deadline fired, this much earlier than the
    /// monotonic clock reached it (time lost in suspend).
    Woke(Instant, Duration),
    FilesChanged,
    /// The system's “enable animations” setting may have changed.
    ReadSystemMotion,
    /// Whether the system asks apps to animate.
    SystemMotion(bool),
    DesktopPaletteChanged,
    WallpaperColorsChanged,
    RainLifecycle(crate::rain::lifecycle::State),
    /// A quiet scene finished fading out; its surfaces can go.
    RainRetired(u64),
    /// Rain surfaces whose frames stopped coming: re-arm them.
    RainNudge(Vec<window::Id>),
    /// What hides the desktop changed.
    RainDesk(crate::rain::desk::Desk),
    /// cosmic-bg rewrote its state file.
    RainReceipt,
    RainReady(
        window::Id,
        u64,
        Option<crate::desktop::wallpaper::BgEntry>,
        Option<crate::desktop::wallpaper::media::Pixels>,
        Option<crate::watch::FileStamp>,
    ),
    DepthObserved(u64, Option<crate::desktop::wallpaper::BgEntry>),
    DepthReady(u64, Option<crate::depth::Rendered>),
    /// The clock drawn for behind the rain glass, or nothing.
    GlassReady(u64, Option<crate::desktop::wallpaper::media::Pixels>),
    /// The picture with this content key was analysed, or failed.
    DepthAnalysed(String, bool),
    WallpaperColorsReady(
        u64,
        Result<Option<crate::desktop::wallpaper::automatic::Prepared>, String>,
    ),
    WallpaperColorsApplied(Result<(), String>),
    Metrics(Result<(Sampler, Metrics), String>),
    Animate,
    Hover(bool, bool),
    ToggleWidget(Widget),
    FocusWidget(Widget),
    ShowControls,
    Arrange,
    ResetLayout,
    LayoutPress(Widget),
    LayoutResizePress(Widget),
    LayoutSelect(Widget),
    LayoutResize([i8; 2]),
    LayoutResizeMode,
    LayoutAutomatic,
    LayoutUndo,
    /// Arrange: put a widget on the desktop, or take it off.
    LayoutShow(Widget, bool),
    /// Arrange: open or close the Add widget list.
    LayoutAddMenu,
    /// Arrange: back in front after a widget's Settings.
    LayoutFront,
    LayoutFinish(bool),
    LayoutRetryCancel,
    LayoutResume,
    LayoutKeepSaved,
    ArrangeSurface(u64),
    ArrangeDisplay(String),
    LayoutDefaults,
    LayoutSnap(bool),
    LayoutCancelDrag,
    LayoutKey(cosmic::iced::keyboard::Key, bool),
    HideControls,
    SidebarPress(Widget),
    Pointer(window::Id, cosmic::iced::mouse::Event),
    SurfaceResized(window::Id, cosmic::iced::Size),
    ToggleProjects,
    ToggleProject(String),
    ToggleEarlier,
    RefreshProjects,
    Projects(Vec<String>, Result<ProjectsData, String>),
    Weather(crate::providers::weather::Completion),
    WeatherCacheSaved(Result<(), String>),
    Service(bool),
    Calendar(String, String, Result<CalendarData, String>),
    PreviewCalendar(CalendarData),
    RequestCapture,
    Week(i32),
    Day(NaiveDate),
    Today,
    RefreshCalendar,
    Open(String),
    Settings,
    WeatherSettings,
    ClockSettings,
    CalendarSettings,
    DevboxSettings,
    ProjectSettings,
    ControlFinished(Result<(), String>),
    CaptureScreens,
    Captured(usize, window::Screenshot),
    /// A Music, Notes or Focus card control.
    Tool(crate::tools::cards::Action),
    Audio(crate::tools::visualizer::Event),
    MusicFeed(crate::tools::music::Feed),
    /// Connected devices, from `crate::devices`.
    Devices(Vec<crate::devices::Device>),
    MusicArt(String, Result<crate::tools::music::Art, String>),
    ToolDone(Result<(), String>),
    MusicControlDone(Result<Vec<crate::tools::music::Player>, String>),
    NoteCopyDone(Result<(), String>),
    NoteChecked(Result<crate::tools::notes::Note, crate::tools::notes::SaveError>),
    /// A note save finished: the text it saved, and the result. (Tests
    /// save inline.)
    #[cfg_attr(test, allow(dead_code))]
    NoteSaved(
        String,
        Result<crate::tools::notes::Note, crate::tools::notes::SaveError>,
    ),
    /// `desktop-cards focus start|pause|toggle|reset|skip`.
    FocusCommand(String),
    /// Full compositor metadata, including startup and removal events.
    OutputChanged(crate::displays::OutputEvent, crate::displays::WlOutput),
    SurfaceMessage(u64, Box<Message>),
    SurfaceInput(window::Id, Box<Message>),
    /// Old surfaces are gone: measure the selected output for this epoch.
    RebuildSurfaces(u64),
    DisplayIdentities(u64, std::collections::BTreeMap<String, String>),
    /// Automatic profile switches finished.
    Switched(Vec<(crate::automation::Due, crate::automation::run::Ran)>),
    /// `halcyon --toggle drawer`: open the edge menu for the keyboard, or close it.
    DrawerToggle,
    /// A click on an edge strip: the menu opens at once, on its display.
    EdgeClick(window::Id),
    /// The pointer onto or off an edge strip.
    EdgeHover(window::Id, bool),
    /// The pointer over an item of the Icons or Grid menu, or off them:
    /// its name shows beside it.
    EdgeTip(Option<drawer::DrawerItem>),
    /// An item of the customized edge menu, by its place in the list.
    EdgeItem(usize),
    /// "Customize this menu": the edge menu's page in Settings.
    EdgeCustomize,
    /// Widgets › Show widgets, from the edge menu.
    SetWidgetsVisible(bool),
    /// Do Not Disturb, the island's and COSMIC's switch.
    SetDnd(bool),
    /// The wallpaper's weather effect on or off.
    SetWeatherEffect(bool),
    /// Opens an installed app by its desktop entry ID.
    LaunchApp(String),
    /// Switches to a Look from the edge menu.
    ApplyLook(crate::automation::ProfileRef, String),
    LookApplied(Result<(), String>),
    /// A key on the edge menu or the widget menu (others are dropped).
    KeyInput(
        window::Id,
        cosmic::iced::keyboard::Key,
        cosmic::iced::keyboard::Modifiers,
    ),
    /// A surface lost the keyboard.
    Unfocused(window::Id),
    /// The pointer left a surface.
    PointerLeft(window::Id),
    /// A surface closed; the compositor dismisses the widget menu this way.
    SurfaceClosed(window::Id),
    /// Right-click on a widget: its menu.
    WidgetMenu(Widget),
    WidgetMenuPick(layout::MenuPick),
    WidgetSettings(Widget),
    SetWidgetVisible(Widget, bool),
    SaveOrder(Vec<Widget>),
    NoticeRetry,
    NoticeDismiss,
    NoticeDetails,
    RecoveryDetails,
    /// A working update replaced the program (see `restart`).
    Updated(crate::upgrade::Program),
    /// The armed restart is due, this much earlier than the monotonic clock
    /// reached it.
    RestartDue(Duration),
}

/// A change made on the desktop that couldn't be saved, shown in the edge
/// menu (where Arrange explains its own failures) until dismissed.
pub struct Notice {
    pub title: &'static str,
    pub body: &'static str,
    pub error: String,
    pub retry: Option<Message>,
    pub details: bool,
}

/// Where the pointer last moved, so the widget menu opens under it. Kept
/// out of the update loop: motion over cards must not rebuild surfaces.
static POINTER: std::sync::Mutex<Option<(window::Id, cosmic::iced::Point)>> =
    std::sync::Mutex::new(None);
pub struct Cards {
    activity: crate::activity_island::bridge::Owner,
    core: Core,
    ids: [window::Id; 14],
    layout_editor: layout::LayoutEditor,
    surface_geometry: Option<layout::SurfaceGeometry>,
    displays: crate::displays::Registry,
    selected_output: Option<crate::displays::Output>,
    arrange_display: Option<String>,
    pending_arrange: Option<crate::displays::WlOutput>,
    widget_outputs: Vec<Option<crate::displays::Output>>,
    extra_probes: Vec<monitors::Probe>,
    display_positions: std::collections::BTreeMap<String, [f32; 2]>,
    display_sizes: std::collections::BTreeMap<String, [f32; 2]>,
    paused_layout: Option<layout::LayoutEditor>,
    /// Display changes seen; only the latest one rebuilds the surfaces.
    display_generation: u64,
    /// Where each card surface takes clicks (`Widget::BASE` order).
    surface_inputs: Option<[Vec<[u32; 4]>; 6]>,
    /// Inside `update` (which also runs nested for some messages).
    updating: bool,
    /// The card surfaces need rebuilding at the end of this update.
    rebuild_pending: bool,
    /// The edge menu's surface is open. It is closed while the menu is
    /// fully out of sight.
    controls_open: bool,
    /// The usable area of the widget display as the probes last reported
    /// it: the width from the arrange surface, the height from the edge
    /// strip (or both from the arrange surface while arranging).
    work_area: [Option<f32>; 2],
    screen_size: [f32; 2],
    output_known: bool,
    sidebar: Sidebar,
    desktop_height: f32,
    calendar_height: f32,
    projects: Option<ProjectsData>,
    projects_fetching: bool,
    projects_error: bool,
    next_projects: Instant,
    all_projects: bool,
    expanded_projects: std::collections::BTreeSet<String>,
    config: Config,
    /// The system's “enable animations” (Accessibility › Reduce motion off).
    /// While it is off, the loaded config is read with every motion
    /// switch off, so rain, the visualizer, the island and the drawer stay
    /// still, whatever Halcyon's own switches say.
    system_animations: bool,
    sampler: Sampler,
    metrics: Metrics,
    histories: history::MetricHistories,
    metrics_fetching: bool,
    next_metrics: Instant,
    service_fetching: bool,
    next_service: Instant,
    next_clock: Instant,
    clock_at: chrono::DateTime<Local>,
    next_calendar_view: Instant,
    weather: Option<Weather>,
    weather_error: bool,
    weather_at: Option<String>,
    weather_hub: crate::providers::weather::Hub,
    weather_cache: crate::providers::weather::Cache,
    capture_key: Option<String>,
    calendar: Option<CalendarData>,
    calendar_details: Option<String>,
    calendar_connection: crate::calendar::Connection,
    calendar_today: Option<CalendarData>,
    today_fetching: Option<String>,
    calendar_today_error: bool,
    calendar_preview: bool,
    next_today_calendar: Instant,
    calendar_error: bool,
    calendar_fetching: Option<String>,
    next_calendar: Instant,
    month: NaiveDate,
    selected_day: NaiveDate,
    calendar_home: bool,
    last_today: NaiveDate,
    show_earlier: bool,
    tools: tools::Tools,
    automator: crate::automation::runtime::Automator,
    /// Both clocks at the last wake, to see time lost in suspend.
    woke_at: (Instant, SystemTime),
    /// Part of the wake's identity, so a fired wake can be armed again for
    /// the same deadline.
    wake_generation: u64,
    depth: depth::State,
    rain: rain::State,
    glass: glass::State,
    wallpaper_color_generation: std::sync::Arc<std::sync::atomic::AtomicU64>,
    /// The edge menu's strip and the state of its customized list.
    edge: edge::EdgeState,
    notice: Option<Notice>,
    /// The open widget menu: its popup and the widget it's for.
    menu: Option<(window::Id, Widget)>,
    recovery_details: bool,
    /// Restarting into an update once the desktop is left alone.
    restart: restart::Restart,
    /// Wallpaper colors being written to the desktop.
    applying_colors: u32,
}
impl cosmic::Application for Cards {
    type Executor = cosmic::executor::Default;
    type Flags = Config;
    type Message = Message;
    const APP_ID: &'static str = "io.github.profilaz3.Halcyon.Cards";
    fn core(&self) -> &Core {
        &self.core
    }
    fn core_mut(&mut self) -> &mut Core {
        &mut self.core
    }
    fn init(mut core: Core, config: Config) -> (Self, Task<Message>) {
        // Styles name their fonts; learn which are installed before drawing.
        crate::desktop_theme::load_installed_fonts();
        core.window.show_headerbar = false;
        // Desktop surfaces draw their own shapes. Automatic compositor blur
        // covers the entire surface rectangle, including transparent clock
        // pixels and the space outside rounded card corners.
        core.set_auto_blur(Default::default());
        let ids = std::array::from_fn(|_| window::Id::unique());
        let mut sidebar = Sidebar::new();
        sidebar.dwell = config.edge_menu.dwell();
        sidebar.rate = config.edge_menu.speed.rate();
        let mut tasks = Vec::new();
        let today = Local::now().date_naive();
        let month = today.with_day(1).unwrap();
        let calendar = CalendarData::cached(&month.format("%Y-%m").to_string());
        let weather = config.location.as_ref().and_then(WeatherSnapshot::cached);
        let calendar_refresh = config.refresh_intervals.duration(Widget::Calendar);
        let mut app = Self {
            activity: Default::default(),
            system_animations: true,
            core,
            ids,
            layout_editor: layout::LayoutEditor::default(),
            surface_geometry: None,
            displays: Default::default(),
            selected_output: None,
            arrange_display: None,
            pending_arrange: None,
            widget_outputs: Vec::new(),
            extra_probes: Vec::new(),
            display_positions: Default::default(),
            display_sizes: Default::default(),
            paused_layout: None,
            display_generation: 0,
            surface_inputs: None,
            updating: false,
            rebuild_pending: false,
            controls_open: false,
            work_area: [None; 2],
            screen_size: [1920., 1080.],
            output_known: false,
            sidebar,
            desktop_height: 984.,
            calendar_height: 984.,
            projects: config.project_roots.is_empty().then(ProjectsData::default),
            projects_fetching: false,
            projects_error: false,
            next_projects: Instant::now(),
            all_projects: false,
            expanded_projects: Default::default(),
            config,
            sampler: Sampler::default(),
            metrics: Metrics::default(),
            histories: history::MetricHistories::default(),
            metrics_fetching: false,
            next_metrics: Instant::now(),
            service_fetching: false,
            next_service: Instant::now(),
            next_clock: Instant::now(),
            clock_at: Local::now(),
            next_calendar_view: Instant::now(),
            weather: weather.as_ref().map(|snapshot| snapshot.weather.clone()),
            weather_error: false,
            weather_at: weather.map(|snapshot| snapshot.updated_at),
            weather_hub: crate::providers::weather::Hub::new(
                crate::widget_presentation::Context::Live,
            ),
            weather_cache: Default::default(),
            capture_key: None,
            calendar_details: None,
            calendar_today: calendar.clone(),
            today_fetching: None,
            calendar_today_error: false,
            calendar_preview: false,
            next_today_calendar: Instant::now() + calendar_refresh,
            calendar,
            calendar_connection: crate::calendar::load_connection(),
            calendar_error: false,
            calendar_fetching: None,
            next_calendar: Instant::now(),
            month,
            selected_day: today,
            calendar_home: true,
            last_today: today,
            show_earlier: false,
            tools: tools::Tools::load(),
            automator: crate::automation::runtime::Automator::from_env(),
            woke_at: (Instant::now(), SystemTime::now()),
            wake_generation: 0,
            depth: Default::default(),
            rain: Default::default(),
            glass: Default::default(),
            wallpaper_color_generation: Default::default(),
            edge: Default::default(),
            notice: None,
            menu: None,
            recovery_details: false,
            restart: Default::default(),
            applying_colors: 0,
        };
        // Output discovery creates the first probe, including on late startup.
        tasks.extend(app.refresh_visible_data());
        tasks.push(app.wallpaper_colors_changed());
        tasks.push(app.publish_display_state());
        tasks.push(Self::read_system_motion());
        app.activity.timer = crate::activity_island::timer::path().and_then(|path| {
            crate::activity_island::timer::load_from(&path, chrono::Utc::now().timestamp_millis())
        });
        app.publish_activity();
        app.refresh_edge_available();
        tasks.push(cosmic::task::future(async {
            tokio::time::sleep(std::time::Duration::from_secs(20)).await;
            Message::Activity(crate::activity_island::bridge::Event::Settled)
        }));
        (app, Task::batch(tasks))
    }
    fn subscription(&self) -> Subscription<Message> {
        let mut subscriptions = vec![
            self.activity
                .subscription(self.config.activity_island.clone())
                .map(Message::Activity),
            self.restart_subscription(),
        ];
        if self.config.rain.wanted() {
            subscriptions.push(crate::rain::lifecycle::watch().map(Message::RainLifecycle));
            subscriptions.push(crate::rain::desk::watch().map(Message::RainDesk));
            if let Some(watch) = self.rain.watch() {
                subscriptions.push(watch.map(Message::RainNudge));
            }
            let paths = self.rain.paths();
            if !paths.is_empty() {
                subscriptions.push(
                    crate::watch::image_files(paths).map(|_| Message::WallpaperColorsChanged),
                );
            }
            if let Some(receipt) = self.rain.receipt_path() {
                subscriptions
                    .push(crate::watch::image_files(vec![receipt]).map(|_| Message::RainReceipt));
            }
        }
        if let Some(deadline) = self.wake_deadline() {
            subscriptions.push(
                crate::schedule::wake_at(deadline)
                    .with((deadline, self.wake_generation))
                    .map(|((deadline, _), early)| Message::Woke(deadline, early)),
            );
        }
        // Players are followed only while the Music card is shown.
        if self.music_demand() {
            subscriptions.push(crate::tools::music::watch().map(Message::MusicFeed));
        }
        if self.devices_demand() {
            subscriptions.push(crate::devices::subscription().map(Message::Devices));
        }
        if self.visualizer_demand() {
            subscriptions.push(crate::tools::visualizer::watch().map(Message::Audio));
        }
        let paths = Config::path()
            .into_iter()
            .chain(Config::path().map(|p| crate::design::storage::path(&p)))
            .chain(Self::command_path())
            .chain(crate::tools::notes::path())
            .chain(crate::calendar::account_path())
            .chain(self.automator.paths())
            .collect();
        subscriptions.push(crate::watch::files(paths).map(|_| Message::FilesChanged));
        // The same files Settings watches for Accessibility › Reduce motion.
        if let Some(dir) = dirs::config_dir() {
            subscriptions.push(
                crate::watch::read_only_files(vec![
                    dir.join("dconf/user"),
                    dir.join("dconf/cosmic"),
                ])
                .map(|_| Message::ReadSystemMotion),
            );
        }
        subscriptions.push(
            self.config
                .color_subscription()
                .map(|_| Message::DesktopPaletteChanged),
        );
        if (self.config.rain.wanted()
            || self.config.clock.depth
            || self.config.appearance.wallpaper_colors
            || self.config.appearance.family.is_some())
            && let Ok(store) = crate::desktop::DesktopStore::from_env()
            && store.writable()
        {
            subscriptions.push(
                crate::watch::read_only_files(crate::desktop::wallpaper::automatic::watch_paths(
                    &store,
                ))
                .map(|_| Message::WallpaperColorsChanged),
            );
        }
        if self.config.clock.depth
            && let Some(path) = self.depth.picture()
        {
            subscriptions.push(
                crate::watch::read_only_files(vec![path]).map(|_| Message::WallpaperColorsChanged),
            );
        }
        subscriptions.push(
            cosmic::iced::event::listen_with(|event, status, id| match event {
                cosmic::iced::Event::Mouse(event) => {
                    match event {
                        cosmic::iced::mouse::Event::CursorMoved { position } => {
                            if let Ok(mut pointer) = POINTER.lock() {
                                *pointer = Some((id, position));
                            }
                            if let Ok(mut moved) = restart::POINTER_MOVED.lock() {
                                *moved = Some(Instant::now());
                            }
                        }
                        cosmic::iced::mouse::Event::CursorLeft => {
                            return Some(Message::PointerLeft(id));
                        }
                        _ => {}
                    }
                    Some(Message::Pointer(id, event))
                }
                cosmic::iced::Event::Keyboard(cosmic::iced::keyboard::Event::KeyPressed {
                    key,
                    modifiers,
                    ..
                }) if status == cosmic::iced::event::Status::Ignored => {
                    Some(Message::KeyInput(id, key, modifiers))
                }
                cosmic::iced::Event::Window(window::Event::Unfocused) => {
                    Some(Message::Unfocused(id))
                }
                cosmic::iced::Event::Window(window::Event::Closed) => {
                    Some(Message::SurfaceClosed(id))
                }
                cosmic::iced::Event::Window(window::Event::Opened { size, .. })
                | cosmic::iced::Event::Window(window::Event::Resized(size)) => {
                    Some(Message::SurfaceResized(id, size))
                }
                cosmic::iced::Event::PlatformSpecific(
                    cosmic::iced::event::PlatformSpecific::Wayland(
                        cosmic::iced::event::wayland::Event::Output(event, output),
                    ),
                ) => Some(Message::OutputChanged(event, output)),
                _ => None,
            })
            .with((
                self.ids,
                self.sidebar.drag.is_some(),
                self.layout_editor.active,
                self.sidebar.focus.is_some(),
                self.menu.map(|(id, _)| id),
                self.note_typing_surface(),
            ))
            .filter_map(
                |((ids, dragging, arranging, drawer_keys, menu, note), message)| {
                    match &message {
                        Message::KeyInput(id, key, modifiers) => {
                            if arranging {
                                Some(Message::SurfaceInput(
                                    *id,
                                    Box::new(Message::LayoutKey(key.clone(), modifiers.shift())),
                                ))
                            } else {
                                ((drawer_keys && *id == ids[2]) || Some(*id) == menu)
                                    .then_some(message)
                            }
                        }
                        Message::Unfocused(id) => {
                            if arranging {
                                Some(Message::SurfaceInput(
                                    *id,
                                    Box::new(Message::LayoutCancelDrag),
                                ))
                            } else {
                                (Some(*id) == note).then_some(message)
                            }
                        }
                        Message::PointerLeft(id) => {
                            if Some(*id) == note {
                                Some(message)
                            } else {
                                // Treated as before: plain pointer motion.
                                let event = cosmic::iced::mouse::Event::CursorLeft;
                                ((arranging && *id == ids[5]) || *id == ids[2] || *id == ids[3])
                                    .then_some(Message::Pointer(*id, event))
                            }
                        }
                        Message::SurfaceClosed(id) => (Some(*id) == menu).then_some(message),
                        Message::Pointer(id, event) => {
                            // Motion over Calendar/Weather must not rebuild all five surfaces.
                            // Outside releases still cancel an active sidebar drag.
                            ((arranging
                                && (*id == ids[5]
                                    || matches!(
                                        event,
                                        cosmic::iced::mouse::Event::ButtonReleased(
                                            cosmic::iced::mouse::Button::Left
                                        )
                                    )))
                                || *id == ids[2]
                                || *id == ids[3]
                                || (dragging
                                    && matches!(
                                        event,
                                        cosmic::iced::mouse::Event::ButtonReleased(
                                            cosmic::iced::mouse::Button::Left
                                        )
                                    )))
                            .then_some(message)
                        }
                        Message::LayoutKey(..) | Message::LayoutCancelDrag => {
                            arranging.then_some(message)
                        }
                        _ => Some(message),
                    }
                },
            ),
        );
        if self.sidebar.animating() {
            subscriptions.push(
                cosmic::iced::time::every(Duration::from_millis(16)).map(|_| Message::Animate),
            );
        }
        if let Some(deadline) = self.sidebar.next_transition() {
            subscriptions.push(crate::schedule::wake_at(deadline).map(|_| Message::Animate));
        }
        Subscription::batch(subscriptions)
    }
    fn view(&self) -> Element<'_, Message> {
        text::body("").into()
    }
    fn view_window(&self, id: window::Id) -> Element<'_, Message> {
        if let Some(view) = self.rain_view(id) {
            return view;
        }
        if let Some((menu, widget)) = self.menu
            && menu == id
        {
            return self.widget_menu_view(widget);
        }
        if !self.ids.contains(&id) {
            return empty_surface();
        }
        let epoch = self.display_generation;
        self.surface_view(id)
            .map(move |message| Message::SurfaceMessage(epoch, Box::new(message)))
    }

    fn update(&mut self, msg: Message) -> Task<Message> {
        // Card surfaces are rebuilt at most once per update, for its final
        // geometry: two rebuilds at once race, and the second creation is
        // refused, leaving a surface with a stale size and input region.
        // Surface work runs beside the update's other work (an artwork or
        // weather download), never queued behind it.
        let activity_dirty = matches!(
            &msg,
            Message::Activity(_)
                | Message::MusicFeed(_)
                | Message::Devices(_)
                | Message::MusicControlDone(_)
                | Message::FocusCommand(_)
                | Message::MusicArt(..)
                | Message::Tool(_)
                | Message::Calendar(..)
                | Message::PreviewCalendar(_)
                | Message::FilesChanged
                | Message::SystemMotion(_)
                | Message::Woke(..)
        );
        let outer = !self.updating;
        let input = restart::input(&msg);
        self.updating = true;
        // Include watcher/timer/command dispatches: any synchronous writer
        // reached while Arrange owns input must refuse contended locks.
        let task = if self.layout_editor.active || self.paused_layout.is_some() {
            crate::config::with_nonblocking_locks(|| self.handle_update(msg))
        } else {
            self.handle_update(msg)
        };
        if !outer {
            return task;
        }
        self.updating = false;
        if input {
            self.restart.at = None;
        }
        self.arm_restart(Instant::now());
        let pending = std::mem::take(&mut self.rebuild_pending);
        let surfaces = if !self.output_known || self.selected_output.is_none() {
            Task::none()
        } else {
            let cards = if pending && self.cards_need_rebuild() {
                self.rebuild_cards()
            } else {
                Task::batch([self.sync_open_cards(), self.sync_music_shown()])
            };
            Task::batch([cards, self.sync_controls_surface()])
        };
        let depth = self.depth_sync();
        let spectrum = self.sync_spectrum();
        let rain = self.rain_sync();
        self.rain
            .watch_sync(self.display_generation, Instant::now());
        let glass = self.glass_sync();
        if activity_dirty {
            self.publish_activity();
        }
        Task::batch([task, surfaces, depth, spectrum, rain, glass])
    }
    fn style(&self) -> Option<cosmic::iced::theme::Style> {
        Some(cosmic::iced::theme::Style {
            background_color: Color::TRANSPARENT,
            text_color: self.config.palette().foreground,
            icon_color: self.config.palette().foreground,
        })
    }
}

impl Cards {
    fn surface_view(&self, id: window::Id) -> Element<'_, Message> {
        if id == self.ids[12] {
            return self.spectrum_view();
        }
        if self.extra_probes.iter().any(|p| p.owns(id)) {
            return empty_surface();
        }
        if id == self.ids[5] {
            return self.layout_view();
        }
        if id == self.ids[2] {
            return self.control_rail();
        }
        if id == self.ids[3]
            || self.edge.part.is_some_and(|part| part.id == id)
            || self.edge.others.iter().any(|strip| strip.id == id)
        {
            return if self.is_edge_strip(id) {
                self.edge_handle(id)
            } else {
                empty_surface()
            };
        }
        if Widget::ALL
            .into_iter()
            .any(|w| self.widget_id(w) == id && self.arrange_hides_surface(w))
        {
            return empty_surface();
        }
        if id == self.ids[8] {
            return self.individual_widget(Widget::Glance);
        }
        for tool in Widget::TOOLS {
            if id == self.widget_id(tool) {
                return self.individual_widget(tool);
            }
        }
        if self.custom_layout() {
            let widget = if id == self.ids[0] {
                Widget::Clock
            } else if id == self.ids[1] {
                Widget::Weather
            } else if id == self.ids[4] {
                Widget::Calendar
            } else if id == self.ids[6] {
                Widget::Devbox
            } else {
                Widget::Projects
            };
            return self.individual_widget(widget);
        }
        if id == self.ids[0] {
            self.individual_widget(Widget::Clock)
        } else if id == self.ids[4] {
            self.calendar_widget()
        } else if id == self.ids[1] {
            self.desktop_widgets()
        } else {
            empty_surface()
        }
    }

    /// The edge menu's surface.
    fn controls_surface(&mut self, id: window::Id, margin: i32) -> Task<Message> {
        let width = self.controls_width();
        let height = self.controls_height();
        self.edge.surface = Some([width, height]);
        self.sidebar.width = width;
        // Beside the Icons and Grid menus, the room for names is clear:
        // the pointer passes through it to whatever is below.
        let input_zone = (width != self.rail_width()).then(|| {
            vec![cosmic::iced::Rectangle {
                x: self.rail_x(),
                y: 0.,
                width: self.rail_width() as f32,
                height: height as f32,
            }]
        });
        get_layer_surface(SctkLayerSurfaceSettings {
            id,
            output: self.menu_layer_output(),
            layer: Layer::Overlay,
            // Opened from the keyboard, the menu takes the keys until it closes.
            keyboard_interactivity: if self.sidebar.focus.is_some() {
                KeyboardInteractivity::Exclusive
            } else {
                KeyboardInteractivity::None
            },
            anchor: self.controls_anchor().0,
            margin: self.controls_margin(margin),
            size: Some((Some(width), Some(height))),
            input_zone,
            exclusive_zone: 0,
            namespace: "halcyon-controls".into(),
            ..Default::default()
        })
    }
    /// Opens the edge menu's surface while the menu is on screen, sliding or
    /// holding a drag, and closes it once it is fully out of sight.
    /// Hovering the edge strip doesn't open it: the reveal waits a second
    /// anyway, and brushing past the edge shouldn't create a window.
    fn sync_controls_surface(&mut self) -> Task<Message> {
        if !self.output_known || self.selected_output.is_none() {
            return Task::none();
        }
        let needed = self.sidebar.needs_surface();
        if needed == self.controls_open {
            // A notice in the Icons or Grid menu needs the List's room.
            if needed && self.edge.surface != Some([self.controls_width(), self.controls_height()])
            {
                let id = self.ids[2];
                return cosmic::iced::platform_specific::shell::commands::layer_surface::destroy_layer_surface(id)
                    .chain(self.controls_surface(id, self.sidebar.margin()));
            }
            return Task::none();
        }
        self.controls_open = needed;
        if needed {
            // Apps installed or removed, folders made or gone since.
            self.refresh_edge_available();
        }
        let destroy =
            cosmic::iced::platform_specific::shell::commands::layer_surface::destroy_layer_surface(
                self.ids[2],
            );
        if needed {
            destroy.chain(self.controls_surface(self.ids[2], self.sidebar.margin()))
        } else {
            destroy
        }
    }
    fn layer_output(&self) -> IcedOutput {
        IcedOutput::Output(
            self.selected_output
                .as_ref()
                .expect("selected output before surface creation")
                .handle
                .clone(),
        )
    }
    /// Each transition waits for a fresh probe. Old callbacks cannot create a new set.
    fn reconcile_output(&mut self) -> Task<Message> {
        self.refresh_display_layouts();
        self.reconcile_weather(Instant::now());
        if let Some(expected) = &self.pending_arrange {
            // A connector or EDID alias can change on the same live output.
            // A replacement output must never inherit a pending input capture.
            let output = self
                .displays
                .outputs()
                .iter()
                .find(|o| &o.handle == expected);
            self.arrange_display = output.and_then(|o| self.displays.key(o));
            if output.is_none() {
                self.pending_arrange = None;
            }
        }
        let selected = self
            .displays
            .selected(
                self.arrange_display
                    .as_deref()
                    .or(self.config.widget_display.as_deref()),
            )
            .cloned();
        if self
            .pending_arrange
            .as_ref()
            .is_some_and(|expected| selected.as_ref().is_none_or(|o| &o.handle != expected))
        {
            self.pending_arrange = None;
            self.arrange_display = None;
            return self.reconcile_output();
        }
        let widget_outputs: Vec<_> = Widget::ALL
            .into_iter()
            .map(|w| self.widget_output(w).cloned())
            .collect();
        let same_widgets = widget_outputs.len() == self.widget_outputs.len()
            && widget_outputs
                .iter()
                .zip(&self.widget_outputs)
                .all(|(a, b)| match (a, b) {
                    (Some(a), Some(b)) => a.same_geometry(b),
                    (None, None) => true,
                    _ => false,
                });
        let same_geometry = same_widgets
            && match (&selected, &self.selected_output) {
                (Some(a), Some(b)) => a.same_geometry(b),
                (None, None) => true,
                _ => false,
            };
        if same_geometry {
            self.rebase_arrange_identity();
        }
        if self.pause_invalid_arrange_resume(selected.as_ref()) {
            // The recovery request was cancelled, including its display override.
            // Reconcile normal controls without ever arranging on the fallback.
            return self.reconcile_output();
        }
        if same_geometry {
            self.selected_output = selected;
            // A newly connected, unassigned screen still belongs in Settings,
            // and gets the edge menu's strip when the menu is on every display.
            return if self.output_known {
                Task::batch([self.sync_widget_surfaces(), self.sync_edge_strip()])
            } else {
                Task::none()
            };
        }
        self.widget_outputs = widget_outputs;
        let old_probes = std::mem::take(&mut self.extra_probes);
        for output in self.widget_outputs.iter().flatten() {
            if selected.as_ref().is_some_and(|o| o.handle == output.handle)
                || self
                    .extra_probes
                    .iter()
                    .any(|p| p.output.handle == output.handle)
            {
                continue;
            }
            self.extra_probes.push(monitors::Probe::new(output.clone()));
        }
        self.cancel_layout_drag();
        if self.layout_editor.active {
            self.layout_editor.pause_for_output_change();
            self.paused_layout = Some(std::mem::take(&mut self.layout_editor));
        }
        self.pause_note();
        // Sidebar reordering only previews in config until release. Retiring its
        // surface cancels that preview before replacement geometry is computed.
        if let Some(drag) = self.sidebar.drag.take() {
            self.config.widget_order = drag.original_order.iter().map(|w| w.key().into()).collect();
        }
        self.sidebar.hide();
        if self
            .paused_layout
            .as_ref()
            .is_some_and(|editor| editor.exit_failed)
        {
            self.sidebar.show(Instant::now());
        }
        self.display_generation += 1;
        self.selected_output = selected;
        self.output_known = false;
        self.surface_geometry = None;
        self.surface_inputs = None;
        self.rebuild_pending = false;
        self.controls_open = false;
        self.work_area = [None; 2];
        self.forget_tool_surfaces();
        self.edge.kind = None;
        let old_strip = self
            .edge
            .part
            .take()
            .map(|part| part.id)
            .into_iter()
            .chain(self.forget_other_strips());
        let old_ids =
            std::mem::replace(&mut self.ids, std::array::from_fn(|_| window::Id::unique()));
        let old_probes = old_probes.into_iter().flat_map(|p| p.ids);
        let destroy = Task::batch(old_ids.into_iter().chain(old_probes).chain(old_strip).map(
            cosmic::iced::platform_specific::shell::commands::layer_surface::destroy_layer_surface,
        ));
        destroy
            .chain(self.publish_display_state())
            .chain(Task::done(cosmic::Action::App(Message::RebuildSurfaces(
                self.display_generation,
            ))))
    }
    fn publish_display_state(&self) -> Task<Message> {
        let mut snapshot = crate::layout_state::read(&self.config);
        snapshot.display = Some(crate::layout_state::DisplayState {
            output: self
                .selected_output
                .as_ref()
                .and_then(|output| self.displays.key(output)),
            ready: self.output_known,
            epoch: self.display_generation,
        });
        let generation = crate::layout_state::next_generation();
        cosmic::iced::Task::future(async move {
            crate::layout_state::publish(&snapshot, generation);
        })
        .discard()
    }
    fn handle_update(&mut self, msg: Message) -> Task<Message> {
        match msg {
            Message::Activity(event) => self.activity_message(event),
            Message::RainLifecycle(state) => {
                self.rain.lifecycle = state;
                Task::none()
            }
            Message::RainRetired(generation) => self.rain.retired(generation),
            Message::RainNudge(ids) => self.rain.nudge(ids),
            Message::RainDesk(desk) => {
                self.rain.desk = desk;
                Task::none()
            }
            Message::RainReceipt => self.rain_receipt(),
            Message::RainReady(id, generation, entry, pixels, stamp) => {
                self.rain.ready(id, generation, entry, pixels, stamp);
                Task::none()
            }
            Message::Preview => Task::none(),
            Message::SurfaceMessage(epoch, message) => {
                if epoch != self.display_generation || !self.output_known {
                    return Task::none();
                }
                self.handle_update(*message)
            }
            Message::SurfaceInput(id, message) => {
                if !self.ids.contains(&id) || !self.output_known {
                    return Task::none();
                }
                self.handle_update(*message)
            }
            Message::Woke(deadline, early) => {
                self.resumed(early, Instant::now(), SystemTime::now());
                let task = self.update(Message::Tick);
                // Handling a wake normally moves its deadline. One that came
                // early and is still next (a deadline computed again from the
                // same moment) is armed again: a finished wake with the same
                // identity never fires a second time.
                if self.wake_deadline() == Some(deadline) && deadline > Instant::now() {
                    self.wake_generation += 1;
                }
                task
            }
            Message::Tick => {
                let mut tasks = vec![];
                let today = Local::now().date_naive();
                if today != self.last_today {
                    self.show_earlier = false;
                    if self.calendar_home || self.selected_day == self.last_today {
                        tasks.push(self.calendar_overview());
                    }
                    self.last_today = today;
                    self.next_today_calendar = Instant::now();
                }
                tasks.extend(self.refresh_visible_data());
                tasks.push(self.tools_tick());
                tasks.push(self.automation_tick());
                tasks.push(self.capture_if_changed());
                Task::batch(tasks)
            }
            Message::FilesChanged => {
                let mut tasks = vec![];
                self.reload_note();
                tasks.push(self.reload_calendar_connection());
                match Config::try_load() {
                    Ok(config) => tasks.push(self.apply_config(config)),
                    Err(error) => eprintln!("Keeping last valid widget settings: {error}"),
                }
                if let Some(message) = Self::read_command() {
                    tasks.push(self.update(message));
                }
                tasks.push(self.automation_reload());
                tasks.extend(self.refresh_visible_data());
                tasks.push(self.capture_if_changed());
                Task::batch(tasks)
            }
            Message::ReadSystemMotion => Self::read_system_motion(),
            Message::SystemMotion(on) => {
                if on == self.system_animations {
                    return Task::none();
                }
                self.system_animations = on;
                // Stopping needs nothing new from disk; motion comes back
                // with Halcyon's own switches as saved.
                let config = if on {
                    match Config::try_load() {
                        Ok(config) => config,
                        Err(error) => {
                            eprintln!("Keeping last valid widget settings: {error}");
                            return Task::none();
                        }
                    }
                } else {
                    self.config.clone()
                };
                self.apply_config(config)
            }
            Message::DesktopPaletteChanged => {
                if let Err(error) = self.config.refresh_desktop_palette() {
                    eprintln!("Keeping last valid desktop colors: {error}");
                }
                self.capture_if_changed()
            }
            Message::WallpaperColorsChanged => {
                self.depth.invalidate();
                let rain = self.rain.invalidate();
                Task::batch([rain, self.wallpaper_colors_changed()])
            }
            Message::GlassReady(id, pixels) => self.glass_ready(id, pixels),
            Message::DepthObserved(id, entry) => {
                self.depth.observe(id, entry);
                Task::none()
            }
            Message::DepthReady(id, rendered) => match self.depth.complete(id, rendered) {
                Some(key) => self.depth_analyse(key),
                None => Task::none(),
            },
            Message::DepthAnalysed(key, ok) => {
                if self.depth.analysed(&key, ok) {
                    self.depth_sync()
                } else {
                    Task::none()
                }
            }
            Message::WallpaperColorsReady(id, result) => self.wallpaper_colors_ready(id, result),
            Message::WallpaperColorsApplied(result) => {
                self.applying_colors = self.applying_colors.saturating_sub(1);
                if let Err(error) = result {
                    eprintln!("Wallpaper colors: {error}");
                }
                self.update(Message::FilesChanged)
            }
            Message::Metrics(result) => {
                let height_before = self.devbox_natural_height();
                self.metrics_fetching = false;
                match result {
                    Ok((sampler, mut metrics)) => {
                        // Service status can finish while the sample is in flight.
                        metrics.t3 = self.metrics.t3;
                        self.histories.record(
                            &metrics,
                            &self.config.devbox_sampling(),
                            self.config.widget_visible(Widget::Devbox),
                        );
                        self.sampler = sampler;
                        self.metrics = metrics;
                    }
                    Err(error) => {
                        if self.config.recipes.devbox.is_some() {
                            self.metrics = Metrics {
                                t3: self.metrics.t3,
                                ..Default::default()
                            };
                        }
                        self.histories.unavailable(
                            &self.config.devbox_sampling(),
                            self.config.widget_visible(Widget::Devbox),
                        );
                        eprintln!("System sample: {error}");
                    }
                }
                if self.devbox_natural_height() != height_before {
                    self.sync_widget_surfaces()
                } else {
                    Task::none()
                }
            }
            Message::Animate => {
                self.sidebar.tick(Instant::now(), self.config.sidebar_slide);
                self.slide_controls()
            }
            Message::Hover(edge, entered) => {
                self.sidebar.hover(edge, entered, Instant::now());
                Task::none()
            }
            Message::SurfaceResized(id, size) => {
                if self.rain.owns(id) {
                    return Task::none();
                }
                // Another display's strips: widgets there wait for both its
                // usable width and height, and move when a panel changes it.
                if let Some(probe) = self.extra_probes.iter_mut().find(|p| p.owns(id)) {
                    if probe.measured(id, size) {
                        self.surface_geometry = None;
                        return self.sync_widget_surfaces();
                    }
                    return Task::none();
                }
                // A previously queued Create can finish after migration's
                // Destroy. Its fresh ID cannot impersonate the new epoch;
                // close that late surface as well as ignoring its geometry.
                if self.edge.part.is_some_and(|part| part.id == id)
                    || self.edge.others.iter().any(|strip| strip.id == id)
                {
                    return Task::none();
                }
                if !self.ids.contains(&id) && id != window::Id::NONE {
                    return cosmic::iced::platform_specific::shell::commands::layer_surface::destroy_layer_surface(id);
                }
                // libcosmic's layer Size action emits a synthetic square resize
                // using width twice. The probes are compositor-sized, so only
                // their dimensions are authoritative for the desktop budget.
                // Outside Arrange the arrange surface is a one-pixel strip
                // along the top that measures the usable width, and the edge
                // strip measures the usable height; a full-size window just
                // to measure the screen would hold full-screen buffers.
                if (id == self.ids[5] || id == self.ids[3])
                    && self.selected_output.is_some()
                    && size.width.is_finite()
                    && size.height.is_finite()
                    && size.width > 0.
                    && size.height > 0.
                {
                    if id == self.ids[5] {
                        self.work_area[0] = Some(size.width);
                    }
                    // The strip along the top is a pixel tall; the full-size
                    // Arrange overlay reports the height as well.
                    if id == self.ids[3] || size.height > layout::PROBE_THICKNESS as f32 {
                        self.work_area[1] = Some(size.height);
                    }
                    let [Some(width), Some(height)] = self.work_area else {
                        return Task::none();
                    };
                    let screen = [width, height];
                    self.desktop_height = (height - 96.).max(1.);
                    self.calendar_height = self.desktop_height;
                    if self.screen_size != screen || !self.output_known {
                        self.output_known = true;
                        self.screen_size = screen;
                        let widgets = self.sync_widget_surfaces();
                        let arrange = if self
                            .paused_layout
                            .as_ref()
                            .is_some_and(|editor| editor.pending_resume.is_some())
                        {
                            self.complete_arrange_resume()
                        } else if self.pending_arrange.is_some() {
                            self.complete_pending_arrange()
                        } else {
                            Task::none()
                        };
                        return Task::batch([widgets, arrange, self.sync_edge_strip()]);
                    }
                }
                Task::none()
            }
            Message::SidebarPress(widget) => {
                self.sidebar.drag = Some(ReorderDrag::new(
                    widget,
                    self.config.ordered_right_cards(),
                    self.sidebar.pointer,
                ));
                self.sidebar.show(Instant::now());
                Task::none()
            }
            Message::Pointer(id, event) => {
                if !self.ids.contains(&id) || !self.output_known {
                    return Task::none();
                }
                if self.layout_editor.active
                    && (id == self.ids[5]
                        || matches!(
                            event,
                            cosmic::iced::mouse::Event::ButtonReleased(
                                cosmic::iced::mouse::Button::Left
                            )
                        ))
                {
                    self.layout_pointer(event)
                } else {
                    self.sidebar_pointer(id, event)
                }
            }
            Message::Arrange => self.arrange(),
            Message::ArrangeDisplay(display) => self.arrange_on_display(display),
            Message::ResetLayout => self.reset_layout(),
            Message::LayoutPress(widget) => {
                self.layout_editor.adding = false;
                self.layout_press(widget);
                Task::none()
            }
            Message::LayoutResizePress(widget) => {
                self.layout_resize_press(widget);
                Task::none()
            }
            Message::LayoutSelect(widget) => {
                self.layout_editor.adding = false;
                self.layout_editor.selected = Some(widget);
                Task::none()
            }
            Message::LayoutResize(direction) => {
                self.layout_resize(direction, false);
                self.commit_arrange();
                Task::none()
            }
            Message::LayoutResizeMode => {
                self.layout_resize_mode();
                Task::none()
            }
            Message::LayoutAutomatic => {
                self.layout_automatic();
                self.commit_arrange();
                Task::none()
            }
            Message::LayoutUndo => {
                self.layout_undo();
                self.commit_arrange_shown()
            }
            Message::LayoutShow(widget, visible) => self.layout_show(widget, visible),
            Message::LayoutAddMenu => {
                self.layout_editor.adding =
                    !self.layout_editor.adding && !self.addable_widgets().is_empty();
                Task::none()
            }
            Message::LayoutFront => self.layout_front(),
            Message::LayoutFinish(save) => self.finish_arrange(save),
            Message::LayoutRetryCancel => self.retry_arrange_cancel(),
            Message::LayoutResume => self.resume_arrange(),
            Message::LayoutKeepSaved => self.keep_arrange_saved(),
            Message::ArrangeSurface(epoch) => {
                if epoch != self.display_generation
                    || !self.output_known
                    || self.selected_output.is_none()
                {
                    return Task::none();
                }
                Task::batch([
                    self.layout_probe(self.ids[5], self.layout_editor.active),
                    self.sync_widget_surfaces(),
                    self.note_surface_mode(),
                ])
            }
            Message::LayoutDefaults => {
                self.layout_defaults();
                self.commit_arrange();
                Task::none()
            }
            Message::LayoutSnap(value) => {
                self.layout_editor.grid.snap = value;
                self.commit_arrange();
                Task::none()
            }
            Message::LayoutKey(key, shift) => self.layout_key(key, shift),
            Message::LayoutCancelDrag => {
                self.cancel_layout_drag();
                Task::none()
            }
            Message::ToggleWidget(widget) => {
                self.set_widget_visible(widget, !self.config.data_visible(widget))
            }
            Message::SetWidgetVisible(widget, visible) => self.set_widget_visible(widget, visible),
            Message::SaveOrder(order) => self.save_widget_order(order),
            Message::DrawerToggle => self.drawer_toggle(),
            Message::EdgeClick(id) => {
                if !self.sidebar.visible {
                    if !self.controls_open {
                        self.edge.menu_output = self
                            .edge
                            .others
                            .iter()
                            .find(|strip| strip.id == id)
                            .map(|strip| strip.output.clone());
                    }
                    self.sidebar.show(Instant::now());
                }
                Task::none()
            }
            Message::EdgeHover(id, entered) => self.edge_strip_hover(id, entered),
            Message::EdgeTip(tip) => {
                self.edge.tip = tip;
                Task::none()
            }
            Message::EdgeItem(index) => self.edge_item(index),
            Message::EdgeCustomize => self.customize_edge_menu(),
            Message::SetWidgetsVisible(visible) => self.set_widgets_visible(visible),
            Message::SetDnd(on) => self.set_dnd(on),
            Message::SetWeatherEffect(on) => self.set_weather_effect(on),
            Message::LaunchApp(app) => self.launch("gtk-launch", vec![app]),
            Message::ApplyLook(look, name) => self.apply_look(look, name),
            Message::LookApplied(result) => {
                if let Err(error) = result {
                    eprintln!("Look not applied: {error}");
                    self.raise_notice(
                        "Couldn’t switch the Look",
                        "Nothing was changed.",
                        error,
                        None,
                    );
                }
                Task::none()
            }
            Message::KeyInput(id, key, modifiers) => {
                if self.menu.is_some_and(|(menu, _)| menu == id) {
                    if matches!(
                        key,
                        cosmic::iced::keyboard::Key::Named(
                            cosmic::iced::keyboard::key::Named::Escape
                        )
                    ) {
                        return self.close_widget_menu();
                    }
                    return Task::none();
                }
                match drawer::DrawerKey::from_key(&key, modifiers) {
                    Some(key) if id == self.ids[2] => self.drawer_key(key),
                    _ => Task::none(),
                }
            }
            Message::Unfocused(id) | Message::PointerLeft(id)
                if Some(id) != self.note_typing_surface() =>
            {
                Task::none()
            }
            Message::Unfocused(_) => self.note_focus_lost(),
            Message::PointerLeft(_) => self.note_pointer_left(),
            Message::SurfaceClosed(id) => {
                if self.menu.is_some_and(|(menu, _)| menu == id) {
                    self.menu = None;
                }
                Task::none()
            }
            Message::WidgetMenu(widget) => self.open_widget_menu(widget),
            Message::WidgetMenuPick(pick) => self.widget_menu_pick(pick),
            Message::WidgetSettings(widget) if self.layout_editor.active => {
                self.layout_settings(widget)
            }
            Message::WidgetSettings(widget) => self.launch(
                crate::paths::CONTROLLER,
                vec!["settings".into(), "--page".into(), widget.key().into()],
            ),
            Message::NoticeRetry => match self.notice.take().and_then(|notice| notice.retry) {
                Some(message) => self.update(message),
                None => Task::none(),
            },
            Message::NoticeDismiss => {
                self.notice = None;
                Task::none()
            }
            Message::NoticeDetails => {
                if let Some(notice) = &mut self.notice {
                    notice.details = !notice.details;
                }
                Task::none()
            }
            Message::RecoveryDetails => {
                self.recovery_details = !self.recovery_details;
                Task::none()
            }
            Message::Updated(new) => {
                let plan = Self::restart_ledger_plan(&new);
                let in_service = crate::settings_lifecycle::in_widget_service_now();
                self.updated(new, plan, in_service, Instant::now());
                Task::none()
            }
            Message::RestartDue(early) => {
                self.restart_wake(early);
                Task::none()
            }
            Message::ShowControls => {
                self.sidebar.show(Instant::now());
                Task::none()
            }
            Message::HideControls => {
                if let Some(drag) = self.sidebar.drag.take() {
                    self.config.widget_order =
                        drag.original_order.iter().map(|w| w.key().into()).collect();
                }
                self.sidebar.hide();
                Task::none()
            }
            Message::FocusWidget(widget) => {
                if !self.config.data_visible(widget) {
                    self.set_widget_visible(widget, true)
                } else {
                    Task::none()
                }
            }
            Message::ToggleProjects => {
                self.all_projects = !self.all_projects;
                self.sync_widget_surfaces()
            }
            Message::ToggleProject(id) => {
                if self
                    .projects
                    .as_ref()
                    .is_some_and(|data| projects::has_group(&data.projects, &id))
                {
                    if !self.expanded_projects.remove(&id) {
                        self.expanded_projects.insert(id);
                    }
                }
                self.sync_widget_surfaces()
            }
            Message::ToggleEarlier => {
                self.show_earlier = !self.show_earlier;
                self.sync_widget_surfaces()
            }
            Message::RefreshProjects => {
                self.next_projects = Instant::now();
                Task::none()
            }
            Message::Projects(roots, result) => {
                self.projects_fetching = false;
                if roots != self.config.project_roots {
                    self.next_projects = Instant::now();
                    return Task::none();
                }
                self.projects_error = result.is_err();
                if let Ok(projects) = result {
                    self.expanded_projects
                        .retain(|id| projects::has_group(&projects.projects, id));
                    self.projects = Some(projects);
                }
                self.next_projects =
                    Instant::now() + self.config.refresh_intervals.duration(Widget::Projects);
                self.sync_widget_surfaces()
            }
            Message::Weather(completion) => self.weather_finished(completion),
            Message::WeatherCacheSaved(result) => {
                if let Err(error) = result {
                    eprintln!("Weather cache: {error}");
                }
                Task::none()
            }
            Message::Calendar(revision, month, result) => {
                if revision != self.calendar_connection.revision
                    || self.calendar_preview
                    || !self.calendar_month_pending(&month)
                {
                    return Task::none();
                }
                if self.calendar_fetching.as_deref() == Some(&month) {
                    self.calendar_fetching = None;
                }
                if self.today_fetching.as_deref() == Some(&month) {
                    self.today_fetching = None;
                }
                let retry = Instant::now()
                    + if result.is_ok() {
                        self.config.refresh_intervals.duration(Widget::Calendar)
                    } else {
                        Duration::from_secs(120)
                    };
                // One request owns each month until its cache write AND this
                // completion finish. Both views consume that same result.
                if month == Local::now().format("%Y-%m").to_string() {
                    self.calendar_today_error = result.is_err();
                    if let Ok(calendar) = &result {
                        self.calendar_today = Some(calendar.clone());
                    }
                    self.next_today_calendar = retry;
                }
                if month == self.month.format("%Y-%m").to_string() {
                    self.calendar_error = result.is_err();
                    self.next_calendar = retry;
                    if let Ok(calendar) = result {
                        self.calendar = Some(calendar);
                    }
                }
                self.sync_widget_surfaces()
            }
            Message::PreviewCalendar(calendar) => {
                if std::env::var_os("DESKTOP_CARDS_CAPTURE_DIR").is_none()
                    || std::env::var_os("DESKTOP_CARDS_CONFIG").is_none()
                {
                    return Task::none();
                }
                self.calendar_preview = true;
                self.calendar_today = Some(calendar.clone());
                self.calendar = Some(calendar);
                self.calendar_home = true;
                self.selected_day = Local::now().date_naive();
                self.month = self.selected_day.with_day(1).unwrap();
                self.calendar_fetching = None;
                self.today_fetching = None;
                self.calendar_error = false;
                self.calendar_today_error = false;
                self.next_calendar = Instant::now() + Duration::from_secs(3600);
                self.next_today_calendar = self.next_calendar;
                self.sync_widget_surfaces()
            }
            Message::Week(delta) => {
                let anchor = self
                    .calendar_days(Local::now().date_naive())
                    .first()
                    .copied()
                    .unwrap_or(self.selected_day);
                if let Some(day) = anchor.checked_add_signed(Days::weeks(delta.into())) {
                    return self.select_day(day);
                }
                Task::none()
            }
            Message::Day(day) => self.select_day(day),
            Message::Today => self.calendar_overview(),
            Message::RefreshCalendar => {
                self.next_today_calendar = Instant::now();
                self.next_calendar = Instant::now();
                Task::none()
            }
            Message::Service(active) => {
                self.service_fetching = false;
                self.metrics.t3 = active;
                Task::none()
            }
            Message::Settings => self.launch(crate::paths::CONTROLLER, vec!["settings".into()]),
            Message::ClockSettings => self.launch(
                crate::paths::CONTROLLER,
                vec!["settings".into(), "--page".into(), "clock".into()],
            ),
            Message::WeatherSettings => {
                self.launch(crate::paths::CONTROLLER, vec!["weather-picker".into()])
            }
            Message::DevboxSettings => self.launch(
                crate::paths::CONTROLLER,
                vec!["settings".into(), "--page".into(), "devbox".into()],
            ),
            Message::CalendarSettings => self.launch(
                crate::paths::CONTROLLER,
                vec!["settings".into(), "--page".into(), "calendar".into()],
            ),
            Message::ProjectSettings => self.launch(
                crate::paths::CONTROLLER,
                vec!["settings".into(), "--page".into(), "projects".into()],
            ),
            Message::Open(url) => {
                if url.starts_with("halcyon-event:") {
                    if !self.config.calendar_private {
                        let exists = self
                            .calendar
                            .iter()
                            .chain(self.calendar_today.iter())
                            .flat_map(|c| &c.events)
                            .any(|e| e.details_key() == url);
                        if exists {
                            self.calendar_details = if self.calendar_details.as_ref() == Some(&url)
                            {
                                None
                            } else {
                                Some(url)
                            };
                        }
                    }
                    self.sync_widget_surfaces()
                } else {
                    self.launch("xdg-open", vec![url])
                }
            }
            Message::ControlFinished(result) => {
                if let Err(e) = result {
                    eprintln!("Halcyon: {e}");
                }
                Task::none()
            }
            Message::RequestCapture => cosmic::task::future(async {
                tokio::time::sleep(Duration::from_millis(250)).await;
                Message::CaptureScreens
            }),
            Message::CaptureScreens => {
                Task::batch(self.ids.into_iter().enumerate().map(|(i, id)| {
                    window::screenshot(id)
                        .map(move |s| cosmic::Action::App(Message::Captured(i, s)))
                }))
            }
            Message::Tool(action) => {
                let task = self.update_tool(action);
                Task::batch([task, self.capture_if_changed()])
            }
            Message::Audio(event) => {
                self.tools.audio_bins = if self.visualizer_demand() {
                    match event {
                        crate::tools::visualizer::Event::Frame(bins) => bins,
                        _ => [0.; crate::tools::visualizer::BINS],
                    }
                } else {
                    [0.; crate::tools::visualizer::BINS]
                };
                Task::none()
            }
            Message::MusicFeed(feed) => {
                let task = self.music_feed(feed);
                Task::batch([task, self.capture_if_changed()])
            }
            Message::Devices(devices) => {
                self.tools.devices = devices;
                Task::none()
            }
            Message::MusicArt(url, result) => {
                let task = self.music_art(url, result);
                Task::batch([task, self.capture_if_changed()])
            }
            Message::FocusCommand(command) => self.focus_command(&command),
            Message::DisplayIdentities(generation, identities) => {
                if self.displays.accept_identities(generation, identities) {
                    self.reconcile_output()
                } else {
                    Task::none()
                }
            }
            Message::OutputChanged(event, output) => {
                self.displays.update(event, output);
                let (count, largest) = self.displays.summary();
                crate::usage::system::displays(count, largest);
                let migration = self.reconcile_output();
                Task::batch([migration, self.automation_displays(), self.identity_scan()])
            }
            Message::RebuildSurfaces(epoch) => {
                if epoch != self.display_generation
                    || self.selected_output.is_none()
                    || self.output_known
                {
                    return Task::none();
                }
                Task::batch([
                    self.layout_probe(self.ids[5], false),
                    self.edge_surface(self.ids[3]),
                    self.extra_probe_tasks(),
                ])
            }
            Message::Switched(results) => self.automation_done(results),
            Message::MusicControlDone(result) => match result {
                Ok(players) => {
                    self.tools.music_problem = false;
                    self.music_feed(crate::tools::music::Feed::Players(players))
                }
                Err(error) => {
                    eprintln!("Music: {error}");
                    self.tools.music_problem = true;
                    Task::none()
                }
            },
            Message::ToolDone(result) => {
                if let Err(error) = result {
                    eprintln!("Halcyon: {error}");
                }
                Task::none()
            }
            Message::NoteSaved(text, result) => self.note_saved(text, result),
            Message::NoteChecked(result) => self.note_checked(result),
            Message::NoteCopyDone(result) => {
                self.tools.note_problem = result.err();
                Task::none()
            }
            Message::Captured(i, shot) => {
                if let Some(path) = std::env::var_os("DESKTOP_CARDS_CAPTURE_DIR") {
                    let path = std::path::PathBuf::from(path);
                    let stem = format!("{}-{i}", self.config.theme);
                    let _ = std::fs::create_dir_all(&path);
                    let _ = std::fs::write(path.join(format!("{stem}.rgba")), &shot.rgba);
                    let _ = std::fs::write(
                        path.join(format!("{stem}.json")),
                        format!("[{}, {}]", shot.size.width, shot.size.height),
                    );
                }
                Task::none()
            }
        }
    }
}
fn empty_surface<'a>() -> Element<'a, Message> {
    container(cosmic::iced::widget::space())
        .width(Length::Fill)
        .height(Length::Fill)
        .into()
}
fn label<'a>(value: impl Into<String>, size: f32, color: Color) -> Element<'a, Message> {
    text::body(value.into())
        .size(size.max(crate::widget_style::META))
        .line_height(1.25)
        .font(crate::widget_style::font())
        .class(cosmic::theme::Text::Color(crate::widget_style::semantic(
            color,
        )))
        .into()
}
fn timestamp_text(value: Option<&str>, failed: bool, now: chrono::DateTime<Local>) -> String {
    let date = value
        .and_then(|value| chrono::DateTime::parse_from_rfc3339(value).ok())
        .map(|date| date.with_timezone(&Local));
    let time = date
        .as_ref()
        .map(|date| {
            if date.date_naive() == now.date_naive() {
                date.format("%H:%M").to_string()
            } else if date.date_naive() == now.date_naive() - Days::days(1) {
                format!("Yesterday · {}", date.format("%H:%M"))
            } else if date.year() == now.year() {
                date.format("%b %-d · %H:%M").to_string()
            } else {
                date.format("%b %-d, %Y · %H:%M").to_string()
            }
        })
        .unwrap_or_else(|| "—".into());
    if failed {
        format!("Saved · {time}")
    } else {
        time
    }
}
fn timestamp<'a>(
    value: Option<&str>,
    failed: bool,
    _source: &str,
    p: Palette,
) -> Element<'a, Message> {
    label(
        timestamp_text(value, failed, Local::now()),
        9.,
        if failed {
            Color::from_rgb8(233, 192, 124)
        } else {
            p.muted
        },
    )
}

fn forecast_temperature(value: Option<&f64>) -> String {
    forecast_temperature_in(crate::config::WeatherPreferences::default(), value)
}
/// A reading in the chosen units, rounded, without a "-0".
fn forecast_temperature_in(units: crate::config::WeatherPreferences, value: Option<&f64>) -> String {
    value
        .filter(|value| value.is_finite())
        .map(|value| {
            let rounded = units.degrees(*value).round();
            format!("{:.0}°", if rounded == 0. { 0. } else { rounded })
        })
        .unwrap_or_else(|| "—".into())
}

/// Cached readings remain useful, but must disclose both age and refresh
/// failures. The refresh interval is the maximum age of a current reading.
fn weather_status(
    config: &Config,
    updated_at: Option<&str>,
    failed: bool,
    now: chrono::DateTime<Local>,
) -> Option<String> {
    let updated = updated_at.and_then(|value| chrono::DateTime::parse_from_rfc3339(value).ok());
    let age = updated.map(|updated| now.signed_duration_since(updated));
    let stale = age.is_some_and(|age| {
        age.num_seconds() < 0
            || age.num_seconds() as u64 >= config.refresh_intervals.seconds(Widget::Weather)
    });
    if !failed && !stale && (updated_at.is_none() || updated.is_some()) {
        return None;
    }
    let age = match age
        .map(|age| age.num_seconds())
        .filter(|seconds| *seconds >= 0)
    {
        Some(seconds) if seconds < 60 => "just now".into(),
        Some(seconds) if seconds < 3600 => format!("{}m ago", seconds / 60),
        Some(seconds) if seconds < 86400 => format!("{}h ago", seconds / 3600),
        Some(seconds) => format!("{}d ago", seconds / 86400),
        None => "time unknown".into(),
    };
    Some(format!(
        "Saved · {age}{}",
        if failed { "\nRefresh failed" } else { "" },
    ))
}

fn weather_forecast_days(
    daily: &data::Daily,
    today: NaiveDate,
) -> impl Iterator<Item = (usize, NaiveDate)> + '_ {
    daily
        .time
        .iter()
        .enumerate()
        .filter_map(move |(index, day)| {
            let day = NaiveDate::parse_from_str(day, "%Y-%m-%d").ok()?;
            (day >= today).then_some((index, day))
        })
}

fn action<'a>(value: impl Into<String>, message: Message, color: Color) -> Element<'a, Message> {
    mouse_area(
        container(label(value, 12., color))
            .padding([2, 8])
            .align_x(Alignment::Center),
    )
    .on_press(message)
    .interaction(cosmic::iced::mouse::Interaction::Pointer)
    .into()
}
fn spread<'a>(left: Element<'a, Message>, right: Element<'a, Message>) -> Element<'a, Message> {
    row::with_children(vec![container(left).width(Length::Fill).into(), right])
        .align_y(Alignment::Center)
        .spacing(8)
        .into()
}
fn card<'a>(
    children: Vec<Element<'a, Message>>,
    p: Palette,
    appearance: Appearance,
    widget: Widget,
    height: f32,
) -> Element<'a, Message> {
    card_sized(children, p, appearance, widget, height, Length::Fill)
}
fn card_sized<'a>(
    children: Vec<Element<'a, Message>>,
    p: Palette,
    appearance: Appearance,
    widget: Widget,
    height: f32,
    width: Length,
) -> Element<'a, Message> {
    let spacing = match widget {
        Widget::Weather => 6,
        Widget::Devbox => 8,
        _ => 10,
    };
    card_frame_width(
        scrollable(
            container(
                column::with_children(children)
                    .spacing(spacing)
                    .width(Length::Fill),
            )
            .width(Length::Fill)
            .padding([0, 6, 0, 0]),
        )
        .id(cosmic::widget::Id::new(widget.key()))
        .height(Length::Fill)
        .width(Length::Fill)
        .scroller_width(3.)
        .scrollbar_width(3.)
        .scrollbar_padding(2.)
        .into(),
        p,
        appearance,
        Length::Fixed(height),
        width,
    )
}
fn card_frame<'a>(
    content: Element<'a, Message>,
    p: Palette,
    appearance: Appearance,
    height: Length,
) -> Element<'a, Message> {
    card_frame_width(content, p, appearance, height, Length::Fill)
}
fn card_frame_width<'a>(
    content: Element<'a, Message>,
    p: Palette,
    appearance: Appearance,
    height: Length,
    width: Length,
) -> Element<'a, Message> {
    // The pinned keyed column diffs card roots directly. Keep their root types
    // identical so switching between an agenda and a scroller reconciles state
    // through Tree::diff instead of reusing a different widget's state.
    let edge = crate::widget_style::edge(p);
    let radius = crate::widget_style::radius(appearance.corner_radius);
    let background = crate::widget_style::background(p.background);
    let material_border = crate::widget_style::border(Color {
        a: f32::from(appearance.border_opacity.min(50)) / 100.,
        ..p.accent
    });
    let content = container(
        column::with_children(vec![content])
            .width(Length::Fill)
            .height(if height == Length::Shrink {
                Length::Shrink
            } else {
                Length::Fill
            }),
    )
    .height(height)
    .padding(crate::widget_style::PADDING)
    .width(width)
    .class(cosmic::theme::Container::custom(move |_| {
        cosmic::iced::widget::container::Style {
            text_color: Some(p.foreground),
            icon_color: Some(p.foreground),
            background: Some(background),
            border: Border {
                color: if p.background.a == 0. {
                    Color::TRANSPARENT
                } else {
                    material_border
                },
                width: edge.0,
                radius: radius.into(),
            },
            shadow: edge.1,
            ..Default::default()
        }
    }))
    .into();
    crate::widget_style::surface(content, p)
}
fn mix(a: Color, b: Color, amount: f32) -> Color {
    crate::widget_style::tint(a, b, amount)
}
fn weather_dew_point_degrees(config: &Config, current: &data::Current) -> Option<String> {
    let value = current
        .dew_point_2m
        .filter(|value| config.weather.show_dew_point && value.is_finite())?;
    let rounded = config.weather.degrees(value).round();
    // Avoid a misleading "-0" when the reading is just below freezing.
    let rounded = if rounded == 0. { 0. } else { rounded };
    Some(format!("{rounded:.0}°"))
}
fn weather_dew_point(config: &Config, current: &data::Current) -> Option<String> {
    weather_dew_point_degrees(config, current).map(|degrees| format!("Dew point {degrees}"))
}

/// Compatibility wrapper for existing focused fixtures.
#[cfg(test)]
fn weather_view(
    config: &Config,
    weather: Option<&Weather>,
    updated_at: Option<&str>,
    failed: bool,
    height: f32,
) -> Element<'static, Message> {
    weather::Presentation::new(
        config,
        weather::Snapshot {
            weather,
            updated_at,
            failed,
        },
        Local::now(),
    )
    .view(
        crate::widget_presentation::Context::Live,
        [crate::widget_presentation::WEATHER_SIZE[0], height],
    )
}

#[cfg(test)]
fn clock_labels(now: chrono::NaiveTime, format_24h: bool) -> (String, Option<String>) {
    let reading = crate::widget_style::resolver::ClockReading::at(now, format_24h);
    (reading.time, reading.period)
}
fn week_start(day: NaiveDate) -> NaiveDate {
    day - Days::days(day.weekday().num_days_from_monday().into())
}
fn week_label(day: NaiveDate) -> String {
    let start = week_start(day);
    let end = start + Days::days(6);
    if start.year() != end.year() {
        format!(
            "{} – {}",
            start.format("%b %-d, %Y"),
            end.format("%b %-d, %Y")
        )
    } else if start.month() != end.month() {
        format!("{} – {}", start.format("%b %-d"), end.format("%b %-d, %Y"))
    } else {
        format!("{}–{}", start.format("%b %-d"), end.format("%-d, %Y"))
    }
}
fn bar<'a>(segments: &[(f32, Color)], p: Palette) -> Element<'a, Message> {
    let mut used = 0u16;
    let mut cells = vec![];
    for (fraction, color) in segments {
        let portion = ((fraction.clamp(0., 1.) * 1000.).round() as u16).min(1000 - used);
        if portion == 0 {
            continue;
        }
        used += portion;
        cells.push(bar_piece(*color, portion));
    }
    if used < 1000 {
        cells.push(bar_piece(mix(p.accent, p.background, 0.09), 1000 - used));
    }
    row::with_children(cells).height(Length::Fixed(5.)).into()
}
fn bar_piece<'a>(color: Color, portion: u16) -> Element<'a, Message> {
    container(text::body(""))
        .height(Length::Fixed(5.))
        .width(Length::FillPortion(portion))
        .class(cosmic::theme::Container::custom(move |_| {
            cosmic::iced::widget::container::Style {
                background: Some(Background::Color(color)),
                border: Border {
                    radius: 2.into(),
                    ..Default::default()
                },
                ..Default::default()
            }
        }))
        .into()
}
impl Cards {
    fn select_day(&mut self, day: NaiveDate) -> Task<Message> {
        let scroll = self.show_day(day);
        Task::batch([scroll, self.sync_widget_surfaces()])
    }
    /// Shows `day` alone; the caller syncs the surfaces.
    fn show_day(&mut self, day: NaiveDate) -> Task<Message> {
        self.calendar_home = false;
        let month = day.with_day(1).unwrap();
        if self.month != month {
            self.month = month;
            self.calendar = CalendarData::cached(&month.format("%Y-%m").to_string());
            self.next_calendar = Instant::now();
            self.calendar_error = false;
        }
        self.selected_day = day;
        // Each day starts at its first event when navigating, even if the old list was scrolled.
        let scroll = Task::batch(["calendar"].map(|id| {
            cosmic::iced::widget::scrollable::snap_to(
                cosmic::widget::Id::new(id),
                cosmic::iced::widget::scrollable::RelativeOffset {
                    x: None,
                    y: Some(0.),
                },
            )
        }));
        scroll
    }
    fn calendar_overview(&mut self) -> Task<Message> {
        // One surface sync for the overview's final height, not one for the
        // single day and another for the overview.
        let scroll = self.show_day(Local::now().date_naive());
        self.calendar_home = true;
        Task::batch([scroll, self.sync_widget_surfaces()])
    }
    fn calendar_days(&self, today: NaiveDate) -> Vec<NaiveDate> {
        if !self.calendar_home {
            return vec![self.selected_day];
        }
        let mut days = Vec::new();
        if self.config.calendar_show_today {
            days.push(today);
        }
        if self.config.calendar_show_tomorrow {
            days.push(today + Days::days(1));
        }
        days
    }
    fn launch(&self, command: &str, args: Vec<String>) -> Task<Message> {
        let command = if command == crate::paths::CONTROLLER {
            crate::paths::controller()
        } else if command == "gtk-launch" {
            // Opens a desktop entry as the launcher would, by its ID.
            std::path::PathBuf::from("gtk-launch")
        } else {
            std::path::PathBuf::from("/usr/bin/xdg-open")
        };
        cosmic::task::future(async move {
            let result = tokio::task::spawn_blocking(move || {
                // Outside the service's control group, so restarting the
                // widgets (after an upgrade) leaves what they opened running.
                let in_service = crate::settings_lifecycle::in_widget_service_now();
                let status =
                    crate::settings_lifecycle::independent_command(&command, &args, in_service)
                        .status()
                        .map_err(|e| e.to_string())?;
                if status.success() {
                    Ok(())
                } else {
                    Err(format!("Command exited with {status}"))
                }
            })
            .await
            .unwrap_or_else(|e| Err(e.to_string()));
            Message::ControlFinished(result)
        })
    }
    fn reload_calendar_connection(&mut self) -> Task<Message> {
        if self.calendar_preview {
            return Task::none();
        }
        let connection = crate::calendar::load_connection();
        if connection == self.calendar_connection {
            return Task::none();
        }
        self.calendar_details = None;
        self.calendar_connection = connection;
        self.calendar = None;
        self.calendar_today = None;
        self.calendar_error = false;
        self.calendar_today_error = false;
        self.calendar_fetching = None;
        self.today_fetching = None;
        self.show_earlier = false;
        self.next_calendar = Instant::now();
        self.next_today_calendar = Instant::now();
        self.sync_widget_surfaces()
    }
    fn calendar_month_pending(&self, month: &str) -> bool {
        self.calendar_fetching.as_deref() == Some(month)
            || self.today_fetching.as_deref() == Some(month)
    }
    fn calendar_can_start(&self) -> bool {
        self.calendar_fetching.is_none()
            && !self.calendar_month_pending(&self.month.format("%Y-%m").to_string())
    }
    fn today_calendar_can_start(&self, month: &str) -> bool {
        self.today_fetching.is_none() && !self.calendar_month_pending(month)
    }
    fn calendar_task(&mut self) -> Task<Message> {
        if !self.calendar_connection.connected() || !self.calendar_can_start() {
            return Task::none();
        }
        let month = self.month.format("%Y-%m").to_string();
        self.calendar_fetching = Some(month.clone());
        self.calendar_month_task(month)
    }
    fn calendar_month_task(&self, month: String) -> Task<Message> {
        let connection = self.calendar_connection.clone();
        cosmic::task::future(async move {
            let revision = connection.revision.clone();
            let result = crate::calendar::fetch(connection, month.clone()).await;
            Message::Calendar(revision, month, result)
        })
    }
    fn clock(&self) -> Element<'_, Message> {
        // The rain surface draws the clock behind its glass; this surface
        // keeps its place, empty.
        if self.clock_behind_glass() {
            let size = self.widget_size(Widget::Clock);
            return cosmic::widget::Space::new()
                .width(size[0])
                .height(size[1])
                .into();
        }
        if let Some(view) = self.depth_view() {
            return view;
        }
        clock::view(
            &self.config,
            self.clock_at.naive_local(),
            self.widget_size(Widget::Clock),
        )
    }
    fn weather_card(&self) -> Element<'_, Message> {
        weather::Presentation::new(
            &self.config,
            weather::Snapshot {
                weather: self.weather.as_ref(),
                updated_at: self.weather_at.as_deref(),
                failed: self.weather_error,
            },
            Local::now(),
        )
        .view(
            crate::widget_presentation::Context::Live,
            [
                self.widget_size(Widget::Weather)[0],
                self.card_height(Widget::Weather),
            ],
        )
    }
    #[cfg(test)]
    fn agenda_events(&self, day: NaiveDate) -> Vec<&CalendarEvent> {
        self.calendar_agenda(day).events
    }
    fn calendar_agenda(&self, day: NaiveDate) -> data::CalendarAgenda<'_> {
        self.calendar
            .as_ref()
            .map(|calendar| calendar.agenda_at(day, Local::now(), self.show_earlier))
            .unwrap_or_default()
    }
    fn earlier_control(&self, count: usize, p: Palette) -> Element<'_, Message> {
        container(action(
            if self.show_earlier {
                "Hide earlier ↑".into()
            } else {
                format!("Earlier · {count} ↓")
            },
            Message::ToggleEarlier,
            p.muted,
        ))
        .id(cosmic::widget::Id::new("calendar-earlier"))
        .height(22.)
        .align_y(Alignment::Center)
        .into()
    }
    /// The same toggle, sized to sit beside a day heading.
    fn earlier_inline(&self, count: usize, p: Palette) -> Element<'_, Message> {
        container(
            mouse_area(label(
                if self.show_earlier {
                    "· Hide earlier ↑".into()
                } else {
                    format!("· Earlier {count} ↓")
                },
                10.,
                p.muted,
            ))
            .on_press(Message::ToggleEarlier)
            .interaction(cosmic::iced::mouse::Interaction::Pointer),
        )
        .id(cosmic::widget::Id::new("calendar-earlier"))
        .into()
    }
    fn calendar_event_height(&self, event: &CalendarEvent, width: f32) -> f32 {
        let visible = if self.config.calendar_private {
            event.private_copy()
        } else {
            event.clone()
        };
        let title = glance::agenda_text_height(&visible, 15., (width - 115.).max(20.));
        let status = matches!(
            event.meeting_state_at(Local::now(), self.config.meeting_notice_minutes),
            MeetingState::Soon(_) | MeetingState::Live
        );
        (title + if status { 15. } else { 0. }).max(30.) + 12. + self.event_details_height(event)
    }
    fn calendar_natural_height(&self) -> f32 {
        let now = Local::now();
        let days = self.calendar_days(now.date_naive());
        let width = self
            .explicit_size(Widget::Calendar)
            .map(|size| size[0])
            .unwrap_or_else(|| (self.widget_screen(Widget::Calendar)[0] - 48.).min(360.));
        let mut rows = Vec::new();
        if days.is_empty() {
            rows.push(18.);
        }
        for day in &days {
            if days.len() > 1 {
                rows.push(26.);
            }
            let agenda = self.calendar_agenda(*day);
            let events = agenda.events;
            if self.calendar.is_none() {
                rows.push(36.);
            } else if events.is_empty() {
                rows.push(38.);
            } else {
                for event in events {
                    rows.push(self.calendar_event_height(event, width));
                }
            }
            if agenda.earlier > 0 && days.len() == 1 {
                rows.push(22.);
            }
        }
        (176. + rows.iter().sum::<f32>() + rows.len().saturating_sub(1) as f32 * 3.).ceil()
    }
    fn calendar_card(&self) -> Element<'_, Message> {
        if let Some(recipe) = &self.config.recipes.calendar {
            return self.recipe_calendar(recipe);
        }
        let _style = crate::widget_style::scope(&self.config);
        let p = self.config.palette();
        let now = Local::now();
        let today = now.date_naive();
        let days = self.calendar_days(today);
        let display_day = days.first().copied().unwrap_or(self.selected_day);
        let heading = if self.calendar_home && days.len() != 1 {
            "Calendar".into()
        } else if self.calendar_home && days.first() == Some(&(today + Days::days(1))) {
            "Tomorrow".into()
        } else if self.selected_day == today {
            "Today".into()
        } else {
            self.selected_day.format("%A").to_string()
        };
        let mut rows = vec![spread(
            label(heading, 23., p.foreground),
            label(display_day.format("%a, %b %-d").to_string(), 11., p.muted),
        )];
        rows.push(spread(
            label(week_label(display_day), 11., p.muted),
            row::with_children(vec![
                action(
                    if self.config.calendar_show_today && !self.config.calendar_show_tomorrow {
                        "Today"
                    } else {
                        "Overview"
                    },
                    Message::Today,
                    p.accent,
                ),
                action("↑", Message::Week(-1), p.accent),
                action("↓", Message::Week(1), p.accent),
            ])
            .spacing(6)
            .into(),
        ));
        let start = week_start(display_day);
        rows.push(
            row::with_children((0..7).map(|offset| {
                let day = start + Days::days(offset);
                let selected = if self.calendar_home {
                    days.contains(&day)
                } else {
                    day == self.selected_day
                };
                let color = if selected {
                    p.foreground
                } else if day == today {
                    p.accent
                } else {
                    p.muted
                };
                let cell = container(
                    column::with_children(vec![
                        label(day.format("%a").to_string(), 9., color),
                        label(day.day().to_string(), 13., color),
                    ])
                    .spacing(2)
                    .align_x(Alignment::Center),
                )
                .width(Length::Fill)
                .height(Length::Fixed(36.))
                .align_x(Alignment::Center)
                .align_y(Alignment::Center)
                .class(cosmic::theme::Container::custom(move |_| {
                    cosmic::iced::widget::container::Style {
                        background: selected.then_some(Background::Color(mix(
                            p.muted,
                            p.background,
                            0.16,
                        ))),
                        border: Border {
                            color: if day == today {
                                mix(p.muted, p.background, 0.48)
                            } else if selected {
                                mix(p.muted, p.background, 0.24)
                            } else {
                                Color::TRANSPARENT
                            },
                            width: 1.,
                            radius: 10.into(),
                        },
                        ..Default::default()
                    }
                }));
                mouse_area(cell)
                    .on_press(Message::Day(day))
                    .interaction(cosmic::iced::mouse::Interaction::Pointer)
                    .into()
            }))
            .spacing(3)
            .into(),
        );
        let header = container(column::with_children(rows).spacing(8).width(Length::Fill))
            .height(Length::Fixed(100.));
        let mut agenda = Vec::new();
        if days.is_empty() {
            agenda.push(label("Choose a date above", 14., p.muted));
        }
        for day in &days {
            if days.len() > 1 {
                agenda.push(
                    container(spread(
                        {
                            let name = label(
                                if *day == today { "Today" } else { "Tomorrow" },
                                11.,
                                p.accent,
                            );
                            let earlier = self.calendar_agenda(*day).earlier;
                            if earlier > 0 {
                                row::with_children(vec![name, self.earlier_inline(earlier, p)])
                                    .spacing(6)
                                    .align_y(Alignment::Center)
                                    .into()
                            } else {
                                name
                            }
                        },
                        label(day.format("%a, %b %-d").to_string(), 10., p.muted),
                    ))
                    .padding([8, 2, 4, 2])
                    .into(),
                );
            }
            let day_agenda = self.calendar_agenda(*day);
            let events = day_agenda.events;
            if self.calendar.is_none() {
                agenda.push(if !self.calendar_connection.connected() {
                    action(
                        "Connect your calendar →",
                        Message::CalendarSettings,
                        p.accent,
                    )
                } else if self.calendar_error {
                    action(
                        "Check calendar connection →",
                        Message::CalendarSettings,
                        p.accent,
                    )
                } else {
                    label("Loading your events…", 14., p.muted)
                });
            } else if events.is_empty() {
                agenda.push(
                    container(label(
                        if day_agenda.earlier > 0 {
                            "All done for today"
                        } else {
                            "No events"
                        },
                        14.,
                        p.muted,
                    ))
                    .padding([10, 6])
                    .into(),
                );
            } else {
                for event in events {
                    let state = event.meeting_state_at(now, self.config.meeting_notice_minutes);
                    let highlighted = matches!(state, MeetingState::Soon(_) | MeetingState::Live);
                    let color = if matches!(state, MeetingState::Soon(_)) {
                        Color::from_rgb8(245, 190, 107)
                    } else {
                        p.accent
                    };
                    let status = match state {
                        MeetingState::Soon(minutes) => format!("In {minutes} min"),
                        MeetingState::Live => "Live now".into(),
                        _ => String::new(),
                    };
                    let end = if event.all_day {
                        String::new()
                    } else {
                        chrono::DateTime::parse_from_rfc3339(&event.end)
                            .map(|d| d.with_timezone(&Local).format("%H:%M").to_string())
                            .unwrap_or_default()
                    };
                    let timing = container(
                        column::with_children(vec![
                            label(event.time_label(), 12., color),
                            label(end, 10., p.muted),
                        ])
                        .spacing(2),
                    )
                    .width(Length::Fixed(43.));
                    let mut detail: Vec<Element<'_, Message>> = vec![
                        cosmic::iced::widget::rich_text(glance::agenda_spans_private(
                            event,
                            if event.finished() {
                                p.muted
                            } else {
                                p.foreground
                            },
                            color,
                            self.config.calendar_private,
                        ))
                        .size(15.)
                        .line_height(1.25)
                        .font(crate::widget_style::font())
                        .wrapping(cosmic::iced::advanced::text::Wrapping::WordOrGlyph)
                        .width(Length::Fill)
                        .on_link_click(Message::Open)
                        .into(),
                    ];
                    if !status.is_empty() {
                        detail.push(label(status, 9., color));
                    }
                    let marker_color = event.source_color().unwrap_or(color);
                    let marker = container(text::body(""))
                        .width(Length::Fixed(2.))
                        .height(Length::Fixed(26.))
                        .class(cosmic::theme::Container::custom(move |_| {
                            cosmic::iced::widget::container::Style {
                                background: Some(Background::Color(Color {
                                    a: if highlighted { 0.9 } else { 0.30 },
                                    ..marker_color
                                })),
                                border: Border {
                                    radius: 1.into(),
                                    ..Default::default()
                                },
                                ..Default::default()
                            }
                        }));
                    let entry = vec![
                        timing.into(),
                        marker.into(),
                        container(column::with_children(detail).spacing(3))
                            .width(Length::Fill)
                            .into(),
                    ];
                    agenda.push(
                        container(
                            row::with_children(entry)
                                .spacing(8)
                                .align_y(Alignment::Center),
                        )
                        .padding([6, 6])
                        .width(Length::Fill)
                        .class(cosmic::theme::Container::custom(move |_| {
                            cosmic::iced::widget::container::Style {
                                background: Some(Background::Color(mix(
                                    color,
                                    p.background,
                                    if highlighted { 0.10 } else { 0.025 },
                                ))),
                                border: Border {
                                    color: Color {
                                        a: if highlighted { 0.65 } else { 0.0 },
                                        ..color
                                    },
                                    width: 1.,
                                    radius: 10.into(),
                                },
                                ..Default::default()
                            }
                        }))
                        .into(),
                    );
                    if self.event_details_open(event) {
                        agenda.push(self.event_details_view(event, p));
                    }
                }
            }
            // With day headings, the toggle sits beside the day name.
            if day_agenda.earlier > 0 && days.len() == 1 {
                agenda.push(self.earlier_control(day_agenda.earlier, p));
            }
        }
        let account = self
            .calendar
            .as_ref()
            .map(|calendar| calendar.account.as_str())
            .filter(|account| !account.is_empty());
        let calendar_url = self.calendar_connection.web_url();
        let footer = container(spread(
            mouse_area(
                text::body(if self.calendar_error {
                    "Check connection →".into()
                } else {
                    if self.config.calendar_private || calendar_url.is_empty() {
                        "Calendar".into()
                    } else {
                        format!("{} ↗", account.unwrap_or("Google Calendar"))
                    }
                })
                .size(9.)
                .font(crate::widget_style::font())
                .class(cosmic::theme::Text::Color(p.muted))
                .width(Length::Fill)
                .wrapping(cosmic::iced::advanced::text::Wrapping::None)
                .ellipsize(cosmic::iced::advanced::text::Ellipsize::End(
                    cosmic::iced::advanced::text::EllipsizeHeightLimit::Lines(1),
                )),
            )
            .on_press(
                if self.calendar_error || calendar_url.is_empty() || self.config.calendar_private {
                    Message::CalendarSettings
                } else {
                    Message::Open(calendar_url.into())
                },
            )
            .interaction(cosmic::iced::mouse::Interaction::Pointer)
            .into(),
            row::with_children(vec![
                timestamp(
                    self.calendar
                        .as_ref()
                        .map(|calendar| calendar.updated_at.as_str()),
                    self.calendar_error,
                    "Calendar",
                    p,
                ),
                action(
                    if self.calendar_month_pending(&self.month.format("%Y-%m").to_string()) {
                        "◌"
                    } else {
                        "↻"
                    },
                    Message::RefreshCalendar,
                    p.muted,
                ),
            ])
            .spacing(4)
            .align_y(Alignment::Center)
            .into(),
        ))
        .height(Length::Fixed(20.));
        let agenda = column::with_children(agenda).spacing(3).width(Length::Fill);
        let explicit = self.explicit_size(Widget::Calendar).is_some();
        let limit = self.card_height(Widget::Calendar);
        // 100px header + 20px footer + two 10px gaps + 36px frame padding.
        // Shrink to the actual rendered rows, including wrapped titles. Only
        // the agenda is capped, so busy days keep the date and sync controls.
        const CHROME_HEIGHT: f32 = 176.;
        if limit < CHROME_HEIGHT + 44. {
            // On very short displays there isn't room for fixed controls and
            // an event; keep everything reachable in one compact scroller.
            return card(
                vec![header.into(), agenda.into(), footer.into()],
                p,
                self.config.appearance,
                Widget::Calendar,
                limit,
            );
        }
        let agenda = container(
            scrollable(container(agenda).padding([0, 6, 0, 0]).width(Length::Fill))
                .id(cosmic::widget::Id::new(Widget::Calendar.key()))
                .height(if explicit {
                    Length::Fill
                } else {
                    Length::Shrink
                })
                .width(Length::Fill)
                .scroller_width(2.)
                .scrollbar_width(2.)
                .scrollbar_padding(2.),
        )
        .max_height(limit - CHROME_HEIGHT)
        .height(if explicit {
            Length::Fill
        } else {
            Length::Shrink
        })
        .width(Length::Fill);
        card_frame(
            column::with_children(vec![header.into(), agenda.into(), footer.into()])
                .spacing(10)
                .height(if explicit {
                    Length::Fill
                } else {
                    Length::Shrink
                })
                .width(Length::Fill)
                .into(),
            p,
            self.config.appearance,
            if explicit {
                Length::Fixed(limit)
            } else {
                Length::Shrink
            },
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn desktop_surfaces_do_not_blur_transparent_clock_or_rounded_corners() {
        use cosmic::Application;
        let (cards, _) = Cards::init(Core::default(), Config::default());
        assert!(cards.core.auto_blur().is_empty());
    }

    #[test]
    fn output_probe_owns_desktop_budget_despite_synthetic_widget_resizes() {
        use cosmic::Application;
        let (mut cards, _) = Cards::init(Core::default(), Config::default());
        let _output = cards.test_output_ready();
        let _ = cards.update(Message::SurfaceResized(
            cards.ids[5],
            cosmic::iced::Size::new(1920., 1080.),
        ));
        for id in [cards.ids[1], cards.ids[4], cards.ids[6], cards.ids[7]] {
            let _ = cards.update(Message::SurfaceResized(
                id,
                cosmic::iced::Size::new(360., 360.),
            ));
        }
        assert_eq!(cards.desktop_height, 984.);
        assert_eq!(cards.calendar_height, 984.);
        let _ = cards.update(Message::SurfaceResized(
            cards.ids[5],
            cosmic::iced::Size::new(1280., 720.),
        ));
        assert_eq!(cards.screen_size, [1280., 720.]);
        assert_eq!(cards.desktop_height, 624.);
        assert_eq!(cards.calendar_height, 624.);
    }
    #[test]
    fn clock_formats_midnight_noon_and_evening_without_leading_spaces() {
        let midnight = chrono::NaiveTime::from_hms_opt(0, 5, 0).unwrap();
        let noon = chrono::NaiveTime::from_hms_opt(12, 5, 0).unwrap();
        let evening = chrono::NaiveTime::from_hms_opt(21, 7, 0).unwrap();
        assert_eq!(
            clock_labels(midnight, false),
            ("12:05".into(), Some("AM".into()))
        );
        assert_eq!(
            clock_labels(noon, false),
            ("12:05".into(), Some("PM".into()))
        );
        assert_eq!(
            clock_labels(evening, false),
            ("9:07".into(), Some("PM".into()))
        );
        assert_eq!(clock_labels(midnight, true), ("00:05".into(), None));
        assert_eq!(clock_labels(evening, true), ("21:07".into(), None));
    }
    #[test]
    fn appearance_and_clock_preferences_redraw_without_scheduling_data_refreshes() {
        use cosmic::Application;
        use cosmic::iced::advanced::widget::Tree;
        let (mut cards, _) = Cards::init(
            Core::default(),
            Config {
                enabled_widgets: vec!["clock".into()],
                ..Config::default()
            },
        );
        cards.next_clock = Instant::now() + Duration::from_secs(60);
        let deadline = cards.next_wake();
        let mut clock = Tree::new(cards.clock().as_widget());
        let mut devbox = Tree::new(cards.devbox().as_widget());
        let mut calendar = Tree::new(cards.calendar_card().as_widget());
        for (radius, border, format_24h, show_date) in [
            (0, 0, false, false),
            (32, 50, true, true),
            (16, 20, false, true),
        ] {
            let mut config = cards.config.clone();
            config.appearance.corner_radius = radius;
            config.appearance.border_opacity = border;
            config.clock.format_24h = format_24h;
            config.clock.show_date = show_date;
            let _ = cards.apply_config(config);
            clock.diff(cards.clock().as_widget_mut());
            devbox.diff(cards.devbox().as_widget_mut());
            calendar.diff(cards.calendar_card().as_widget_mut());
            assert_eq!(cards.next_wake(), deadline);
            assert!(cards.refresh_visible_data().is_empty());
        }
    }
    #[test]
    fn independent_deadlines_pause_hidden_widgets_and_do_not_duplicate_samples() {
        use cosmic::Application;
        let (mut cards, _) = Cards::init(
            Core::default(),
            Config {
                enabled_widgets: vec![],
                ..Config::default()
            },
        );
        assert!(cards.next_wake().is_none());
        let mut config = cards.config.clone();
        config.enabled_widgets = vec!["devbox".into()];
        config.devbox.services = true;
        config.refresh_intervals.devbox = 10;
        let _ = cards.apply_config(config);
        let start = Instant::now();
        assert_eq!(cards.refresh_visible_data().len(), 2);
        assert!(cards.metrics_fetching && cards.service_fetching);
        assert!(cards.next_wake().is_none());
        assert!(cards.refresh_visible_data().is_empty());
        assert!(cards.next_metrics >= start + Duration::from_secs(10));
        let _ = cards.update(Message::Service(true));
        let _ = cards.update(Message::Metrics(Ok((
            Sampler::default(),
            Metrics::default(),
        ))));
        assert!(cards.metrics.t3);
        assert!(cards.refresh_visible_data().is_empty());
        let mut config = cards.config.clone();
        config.refresh_intervals.devbox = 1;
        let _ = cards.apply_config(config);
        assert_eq!(cards.refresh_visible_data().len(), 2);
        cards.config.enabled_widgets.clear();
        assert!(cards.next_wake().is_none());
        assert!(cards.refresh_visible_data().is_empty());
    }
    /// After a suspend the wake comes by the wall clock while the monotonic
    /// deadlines are still ahead. It must be handled (the clock moves on,
    /// automation looks again, displays included) and a wake must stay
    /// armed: iced keeps a finished wake under its identity and never runs
    /// it again, so an unhandled early wake used to stop every timer.
    #[test]
    fn a_wake_after_suspend_is_handled_and_the_next_wake_stays_armed() {
        use crate::automation::{self, Change, DisplayWhen, ProfileRef, Rule, Trigger};
        use crate::desktop::store::tests::Fixture;
        use cosmic::Application;
        let fixture = Fixture::new("app-resume");
        std::fs::write(fixture.0.join("outputs"), "eDP-1\n").unwrap();
        automation::update(&fixture.store(), |settings| {
            settings.enabled = true;
            settings.rules.push(Rule {
                id: "r1".into(),
                enabled: true,
                since: 0,
                profile: ProfileRef::Preset("presentation".into()),
                name: "Presentation".into(),
                when: Trigger::Display(DisplayWhen {
                    change: Change::Connect,
                    display: None,
                    label: None,
                }),
            });
            Ok(())
        })
        .unwrap();
        let (mut cards, _) = Cards::init(
            Core::default(),
            Config {
                enabled_widgets: vec!["clock".into()],
                ..Config::default()
            },
        );
        cards.automator = automation::runtime::Automator::with_store(Some(fixture.store()));
        let _ = cards.update(Message::Tick);
        // Before the suspend: the clock and automation's next look are ahead.
        let start = Instant::now();
        cards.next_clock = start + Duration::from_secs(40);
        let looked = cards.automation_wake().unwrap();
        assert!(looked > start);
        let armed = cards.wake_deadline().unwrap();
        // Nine hours asleep: the wall clock moved on, the monotonic one
        // didn't. A dock was plugged in meanwhile.
        cards.woke_at = (start, SystemTime::now() - Duration::from_secs(9 * 3600));
        std::fs::write(fixture.0.join("outputs"), "DP-2\neDP-1\n").unwrap();
        let woke = Instant::now();
        let early = armed.saturating_duration_since(woke);
        assert!(!early.is_zero());
        let _ = cards.update(Message::Woke(armed, early));
        // Handled: the clock went on to its next minute, and automation
        // read the displays again (the new set now settles from here).
        assert!(cards.next_clock > woke);
        let next = cards.automation_wake().unwrap();
        assert!(next >= woke + crate::automation::runtime::SETTLE);
        assert_ne!(cards.wake_deadline(), Some(armed));
        // An early wake with nothing due that leaves the same deadline next
        // (the settle time is counted from a fixed moment) is armed again.
        cards.next_clock = Instant::now() + Duration::from_secs(50);
        let armed = cards.wake_deadline().unwrap();
        assert_eq!(Some(armed), cards.automation_wake());
        let generation = cards.wake_generation;
        let early = armed.saturating_duration_since(Instant::now());
        let _ = cards.update(Message::Woke(armed, early));
        assert_eq!(cards.wake_deadline(), Some(armed));
        assert_eq!(cards.wake_generation, generation + 1);
        // A wake that arrives on time doesn't change the identity.
        cards.next_clock = Instant::now();
        let _ = cards.update(Message::Woke(cards.next_clock, Duration::ZERO));
        assert_eq!(cards.wake_generation, generation + 1);
    }
    #[test]
    fn system_reduce_motion_stops_rain_visualizer_island_and_drawer() {
        use cosmic::Application;
        let moving = Config {
            material_motion: true,
            settings_animations: true,
            sidebar_slide: true,
            ..Config::default()
        };
        let (mut cards, _) = Cards::init(Core::default(), moving.clone());
        assert!(cards.config.material_motion && cards.config.sidebar_slide);
        // Accessibility › Reduce motion on: “enable animations” is off.
        let _ = cards.update(Message::SystemMotion(false));
        let still = |cards: &Cards| {
            !cards.config.material_motion
                && !cards.config.settings_animations
                && !cards.config.sidebar_slide
        };
        assert!(still(&cards));
        // A reload while it is off keeps everything still.
        let _ = cards.apply_config(moving.clone());
        assert!(still(&cards));
        // Halcyon's own switches still decide once the system allows motion.
        cards.system_animations = true;
        let _ = cards.apply_config(moving);
        assert!(cards.config.material_motion && cards.config.sidebar_slide);
    }
    #[test]
    fn timer_does_not_reload_preferences_or_revert_the_in_memory_selection() {
        use cosmic::Application;
        let config = Config {
            enabled_widgets: vec![],
            theme: "teal".into(),
            ..Config::default()
        };
        let (mut cards, _) = Cards::init(Core::default(), config);
        let _ = cards.update(Message::Tick);
        assert!(cards.config.enabled_widgets.is_empty());
        assert_eq!(cards.config.theme, "teal");
    }
    #[test]
    fn old_project_results_do_not_replace_a_new_folder_selection() {
        use cosmic::Application;
        let (mut cards, _) = Cards::init(
            Core::default(),
            Config {
                enabled_widgets: vec![],
                ..Config::default()
            },
        );
        cards.projects = Some(ProjectsData::default());
        cards.projects_error = true;
        cards.all_projects = true;
        let mut config = cards.config.clone();
        config.project_roots = vec!["/new/project".into()];
        let _ = cards.apply_config(config);
        assert!(cards.projects.is_none());
        assert!(!cards.projects_error && !cards.all_projects);
        let _ = cards.update(Message::Projects(
            vec!["/old/project".into()],
            Ok(ProjectsData::default()),
        ));
        assert!(cards.projects.is_none());
        assert!(cards.next_projects <= Instant::now());
    }
    #[test]
    fn calendar_lanes_share_month_owner_cache_and_both_views() {
        if crate::displays::test_support::isolated(
            "app::tests::calendar_lanes_share_month_owner_cache_and_both_views",
        ) {
            return;
        }
        let connection = crate::calendar::Connection {
            backend: crate::calendar::Backend::Google,
            account: "fixture@example.invalid".into(),
            revision: "fixture-revision".into(),
            calendar_id: "fixture".into(),
            ..Default::default()
        };
        std::fs::write(
            crate::calendar::account_path().unwrap(),
            serde_json::to_vec(&connection).unwrap(),
        )
        .unwrap();
        let today = Local::now().date_naive();
        let month = today.format("%Y-%m").to_string();
        let other_day = today + Days::days(60);
        let other_month = other_day.format("%Y-%m").to_string();
        let finish = |cards: &mut Cards, month: &str, label: &str| {
            let data = crate::calendar::fixture_completion(
                &connection,
                month,
                CalendarData {
                    account: connection.account.clone(),
                    month: month.into(),
                    updated_at: label.into(),
                    ..Default::default()
                },
            )
            .unwrap();
            let _ = cards.update(Message::Calendar(
                connection.revision.clone(),
                month.into(),
                Ok(data),
            ));
        };
        for background_first in [false, true] {
            let (mut cards, _) = Cards::init(
                Core::default(),
                Config {
                    enabled_widgets: vec![],
                    ..Default::default()
                },
            );
            cards.config.enabled_widgets = vec!["calendar".into()];
            let _ = cards.show_day(other_day);
            cards.next_today_calendar = Instant::now();
            let _ = cards.refresh_visible_data(); // Two distinct months admitted; no provider IO is executed.
            assert_eq!(
                cards.calendar_fetching.as_deref(),
                Some(other_month.as_str())
            );
            assert_eq!(cards.today_fetching.as_deref(), Some(month.as_str()));
            if !background_first {
                finish(&mut cards, &other_month, "other month");
            }
            let _ = cards.show_day(today);
            let _ = cards.calendar_task();
            assert!(
                !cards.calendar_can_start(),
                "returning to Today must join its existing request"
            );
            assert_ne!(cards.calendar_fetching.as_deref(), Some(month.as_str()));
            assert!(cards.today_fetching.is_some());
            finish(&mut cards, &month, "first current snapshot");
            if background_first {
                finish(&mut cards, &other_month, "other month");
            }
            for data in [&cards.calendar, &cards.calendar_today] {
                assert_eq!(data.as_ref().unwrap().updated_at, "first current snapshot");
            }
            assert_eq!(
                crate::calendar::store_cached(&connection, &month)
                    .unwrap()
                    .updated_at,
                "first current snapshot"
            );
            assert!(cards.calendar_can_start());
            let _ = cards.calendar_task();
            assert!(!cards.today_calendar_can_start(&month));
            let _ = cards.show_day(other_day);
            cards.next_today_calendar = Instant::now();
            let _ = cards.refresh_visible_data();
            assert!(
                cards.today_fetching.is_none(),
                "background refresh joins the selected lane's current-month request"
            );
            let _ = cards.show_day(today);
            finish(&mut cards, &month, "new current snapshot");
            assert_eq!(
                cards.calendar.as_ref().unwrap().updated_at,
                "new current snapshot"
            );
            assert_eq!(
                cards.calendar_today.as_ref().unwrap().updated_at,
                "new current snapshot"
            );
            assert_eq!(
                crate::calendar::store_cached(&connection, &month)
                    .unwrap()
                    .updated_at,
                "new current snapshot"
            );
            // Error completion is also shared, keeps cached data and schedules
            // a bounded retry instead of leaving either consumer due in a loop.
            let _ = cards.calendar_task();
            let _ = cards.update(Message::Calendar(
                connection.revision.clone(),
                month.clone(),
                Err("offline fixture".into()),
            ));
            assert!(cards.calendar_error && cards.calendar_today_error);
            assert!(cards.next_calendar > Instant::now() + Duration::from_secs(119));
            assert_eq!(cards.next_calendar, cards.next_today_calendar);
            assert_eq!(
                cards.calendar_today.as_ref().unwrap().updated_at,
                "new current snapshot"
            );
        }
    }
    #[test]
    fn disabled_widgets_do_not_start_jobs_and_reenable_resumes_due_work() {
        use cosmic::Application;
        let config = Config {
            enabled_widgets: Vec::new(),
            location: Some(Location {
                name: "Test".into(),
                latitude: 0.,
                longitude: 0.,
                timezone: None,
            }),
            ..Config::default()
        };
        let (mut cards, _) = Cards::init(Core::default(), config);
        cards.calendar_connection = crate::calendar::Connection {
            backend: crate::calendar::Backend::Google,
            account: "sample@example.com".into(),
            revision: "test".into(),
            ..Default::default()
        };
        cards.config.project_roots = vec!["/fixture/projects".into()];
        assert!(
            !cards.weather_fetching()
                && cards.calendar_fetching.is_none()
                && !cards.projects_fetching
        );
        assert!(cards.refresh_visible_data().is_empty());
        cards.config.enabled_widgets = vec!["weather".into(), "calendar".into(), "projects".into()];
        // Refresh may also reconcile calendar geometry; the fetching flags below
        // track the three data jobs independently of those local UI tasks.
        assert!(!cards.refresh_visible_data().is_empty());
        assert!(
            cards.weather_fetching()
                && cards.calendar_fetching.is_some()
                && cards.projects_fetching
        );
        // A second refresh must not queue duplicate jobs while these are in flight.
        assert!(cards.refresh_visible_data().is_empty());
    }
    #[test]
    fn fresh_calendar_and_projects_wait_for_setup_and_old_account_results_are_ignored() {
        use cosmic::Application;
        let (mut cards, _) = Cards::init(
            Core::default(),
            Config {
                enabled_widgets: vec!["calendar".into(), "projects".into()],
                ..Default::default()
            },
        );
        // `init` reads the machine's own calendar account; a test machine with
        // Calendar connected has already started a sync. Start from a fresh,
        // unconnected state whatever this machine has.
        cards.calendar_connection = crate::calendar::Connection::default();
        cards.calendar = None;
        cards.calendar_today = None;
        cards.calendar_fetching = None;
        cards.today_fetching = None;
        cards.projects_fetching = false;
        let _ = cards.refresh_visible_data();
        assert!(cards.calendar_fetching.is_none() && !cards.projects_fetching);
        // A visible calendar still updates its date/highlights locally. Only
        // remote sync and project scanning must wait for setup.
        assert_eq!(cards.next_wake(), Some(cards.next_calendar_view));
        assert!(cards.next_calendar_view > Instant::now());
        assert!(cards.refresh_visible_data().is_empty());
        let data = CalendarData {
            account: "former@example.com".into(),
            ..Default::default()
        };
        let _ = cards.update(Message::Calendar(
            "former-revision".into(),
            cards.selected_day.format("%Y-%m").to_string(),
            Ok(data.clone()),
        ));
        let _ = cards.update(Message::Calendar(
            "former-revision".into(),
            "2000-01".into(),
            Ok(data),
        ));
        assert!(cards.calendar.is_none() && cards.calendar_today.is_none());
    }
    fn date(year: i32, month: u32, day: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(year, month, day).unwrap()
    }
    #[test]
    fn calendar_and_scrollable_cards_reconcile_when_toggled_or_reordered() {
        use cosmic::Application;
        use cosmic::iced::advanced::widget::Tree;

        let config = Config {
            enabled_widgets: vec!["weather".into(), "devbox".into(), "calendar".into()],
            ..Config::default()
        };
        let (mut cards, _) = Cards::init(Core::default(), config);
        let mut tree = Tree::new(cards.desktop_widgets().as_widget());
        let mut calendar_tree = Tree::new(cards.calendar_widget().as_widget());
        // Appending a scrollable card used to pass Calendar's stateless root
        // directly to the scroller, panicking on a state downcast.
        for selection in [
            vec!["weather", "devbox", "calendar", "projects"],
            vec!["calendar", "projects"],
            vec!["clock"],
            vec!["clock", "calendar"],
            vec!["weather", "devbox", "calendar", "projects"],
            vec![],
            vec!["weather", "devbox", "calendar"],
        ] {
            cards.config.enabled_widgets = selection.into_iter().map(String::from).collect();
            tree.diff(cards.desktop_widgets().as_widget_mut());
            calendar_tree.diff(cards.calendar_widget().as_widget_mut());
        }
        cards.config.enabled_widgets.push("projects".into());
        for order in [
            ["calendar", "weather", "projects", "devbox"],
            ["devbox", "projects", "weather", "calendar"],
            ["weather", "devbox", "calendar", "projects"],
        ] {
            cards.config.widget_order = order.map(String::from).into();
            // This also switches between the content-sized agenda and the
            // whole-card scroll fallback for very short displays.
            for height in [384., 624., 984.] {
                cards.desktop_height = height;
                cards.calendar_height = height;
                tree.diff(cards.desktop_widgets().as_widget_mut());
                calendar_tree.diff(cards.calendar_widget().as_widget_mut());
            }
        }
    }
    #[test]
    fn calendar_space_is_independent_of_clock_and_the_right_column() {
        use cosmic::Application;
        let (mut cards, _) = Cards::init(Core::default(), Config::default());
        cards.calendar_height = 624.;
        let with_clock = cards.card_height(Widget::Calendar);
        cards
            .config
            .enabled_widgets
            .retain(|name| name == "calendar" || name == "clock");
        assert_eq!(cards.card_height(Widget::Calendar), with_clock);
        cards.config.enabled_widgets.retain(|name| name != "clock");
        assert_eq!(cards.card_height(Widget::Calendar), with_clock);
        assert!(cards.card_height(Widget::Calendar) <= cards.calendar_height);
        cards.calendar_height = 384.;
        assert_eq!(cards.card_height(Widget::Calendar), 384.);
        assert!(
            !cards
                .config
                .ordered_right_cards()
                .contains(&Widget::Calendar)
        );
        assert!(!cards.config.ordered_right_cards().contains(&Widget::Clock));
    }
    #[test]
    fn calendar_grows_past_the_old_cap_and_only_stops_at_the_output_budget() {
        use cosmic::Application;
        let (mut cards, _) = Cards::init(Core::default(), Config::default());
        let today = Local::now().date_naive();
        cards.calendar = Some(CalendarData {
            events: (0..24)
                .map(|index| CalendarEvent {
                    title: format!("Event {index}"),
                    start: today.to_string(),
                    end: (today + Days::days(1)).to_string(),
                    all_day: true,
                    url: String::new(),
                    meeting_url: None,
                    ..Default::default()
                })
                .collect(),
            ..CalendarData::default()
        });
        assert_eq!(cards.agenda_events(today).len(), 24);
        assert!(cards.calendar_natural_height() > 540.);
        assert_eq!(cards.card_height(Widget::Calendar), cards.calendar_height);
        cards
            .config
            .widget_positions
            .insert("calendar".into(), [0., 0.]);
        assert_eq!(cards.widget_size(Widget::Calendar)[1], 984.);
        cards.screen_size = [1920., 1440.];
        assert!(cards.widget_size(Widget::Calendar)[1] > 984.);
    }
    #[test]
    fn calendar_overview_preferences_and_date_browsing_are_independent() {
        use cosmic::Application;
        use cosmic::iced::advanced::widget::Tree;
        let (mut cards, _) = Cards::init(Core::default(), Config::default());
        let last_day = date(2026, 12, 31);
        let mut tree = Tree::new(cards.calendar_widget().as_widget());
        for (today, tomorrow, expected) in [
            (true, false, vec![last_day]),
            (true, true, vec![last_day, date(2027, 1, 1)]),
            (false, true, vec![date(2027, 1, 1)]),
            (false, false, vec![]),
        ] {
            cards.config.calendar_show_today = today;
            cards.config.calendar_show_tomorrow = tomorrow;
            cards.calendar_home = true;
            assert_eq!(cards.calendar_days(last_day), expected);
            tree.diff(cards.calendar_widget().as_widget_mut());
            let _ = cards.update(Message::Day(date(2026, 10, 20)));
            assert_eq!(cards.calendar_days(last_day), vec![date(2026, 10, 20)]);
            let _ = cards.update(Message::Today);
            assert!(cards.calendar_home);
            let now = Local::now().date_naive();
            assert_eq!(cards.selected_day, now);
            assert_eq!(
                cards.calendar_days(now).len(),
                usize::from(today) + usize::from(tomorrow)
            );
            tree.diff(cards.calendar_widget().as_widget_mut());
        }
    }
    #[test]
    fn week_navigation_from_tomorrow_overview_keeps_its_weekday() {
        use cosmic::Application;
        let (mut cards, _) = Cards::init(
            Core::default(),
            Config {
                calendar_show_today: false,
                calendar_show_tomorrow: true,
                ..Config::default()
            },
        );
        let tomorrow = Local::now().date_naive() + Days::days(1);
        let _ = cards.update(Message::Week(1));
        assert_eq!(cards.selected_day, tomorrow + Days::weeks(1));
        assert!(!cards.calendar_home);
        let _ = cards.update(Message::Today);
        assert_eq!(
            cards.calendar_days(Local::now().date_naive()),
            vec![tomorrow]
        );
    }
    #[test]
    fn monday_to_sunday_week_crosses_year_boundary() {
        let new_year = date(2027, 1, 1);
        assert_eq!(week_start(new_year), date(2026, 12, 28));
        assert_eq!(week_start(date(2027, 1, 3)), date(2026, 12, 28));
        assert_eq!(week_start(date(2027, 1, 4)), date(2027, 1, 4));
        assert_eq!(week_label(new_year), "Dec 28, 2026 – Jan 3, 2027");
    }
    #[test]
    fn week_navigation_preserves_the_selected_weekday() {
        use cosmic::Application;
        let sunday = date(2026, 5, 31);
        let (mut cards, _) = Cards::init(Core::default(), Config::default());
        let _ = cards.update(Message::Day(sunday));
        let _ = cards.update(Message::Week(1));
        assert_eq!(cards.selected_day, date(2026, 6, 7));
        assert_eq!(cards.selected_day.weekday(), sunday.weekday());
        assert_eq!(cards.month, date(2026, 6, 1));
        let _ = cards.update(Message::Week(-1));
        assert_eq!(cards.selected_day, sunday);
        assert_eq!(cards.month, date(2026, 5, 1));
        assert_eq!(week_label(sunday), "May 25–31, 2026");
        assert_eq!(week_label(date(2026, 9, 30)), "Sep 28 – Oct 4, 2026");
    }
    #[test]
    fn navigation_ignores_an_old_fetch_and_today_restores_the_current_week() {
        use cosmic::Application;
        let (mut cards, _) = Cards::init(Core::default(), Config::default());
        let _ = cards.update(Message::Day(date(2026, 12, 31)));
        let _ = cards.update(Message::Week(1));
        cards.calendar = None;
        let _ = cards.update(Message::Calendar(
            cards.calendar_connection.revision.clone(),
            "2026-12".into(),
            Ok(CalendarData::default()),
        ));
        assert_eq!(cards.selected_day, date(2027, 1, 7));
        assert_eq!(cards.month, date(2027, 1, 1));
        assert!(cards.calendar.is_none());
        let _ = cards.update(Message::Today);
        let today = Local::now().date_naive();
        assert_eq!(cards.selected_day, today);
        assert_eq!(cards.month, today.with_day(1).unwrap());
    }
    #[test]
    fn sidebar_visibility_does_not_change_the_desktop_selection() {
        use cosmic::Application;
        let config = Config {
            enabled_widgets: vec!["projects".into(), "clock".into()],
            ..Config::default()
        };
        let (mut cards, _) = Cards::init(Core::default(), config);
        let _ = cards.update(Message::ShowControls);
        assert!(cards.sidebar.visible);
        let _ = cards.update(Message::HideControls);
        assert!(!cards.sidebar.visible);
        assert_eq!(cards.config.enabled_widgets, ["projects", "clock"]);
        assert_eq!(cards.desktop_height, 984.);
        cards.config.enabled_widgets.clear();
        let _ = cards.update(Message::ShowControls);
        assert!(cards.config.enabled_widgets.is_empty());
    }
    #[test]
    fn meeting_alert_keeps_today_while_browsing_another_month() {
        use cosmic::Application;
        let (mut cards, _) = Cards::init(Core::default(), Config::default());
        let now = Local::now();
        let event = CalendarEvent {
            title: "Test meeting".into(),
            start: (now + Days::minutes(5)).to_rfc3339(),
            end: (now + Days::minutes(35)).to_rfc3339(),
            all_day: false,
            url: String::new(),
            meeting_url: Some("https://meet.google.com/example".into()),
            ..Default::default()
        };
        cards.today_fetching = Some(now.format("%Y-%m").to_string());
        let _ = cards.update(Message::Calendar(
            cards.calendar_connection.revision.clone(),
            now.format("%Y-%m").to_string(),
            Ok(CalendarData {
                events: vec![event],
                month: now.format("%Y-%m").to_string(),
                ..CalendarData::default()
            }),
        ));
        let _ = cards.update(Message::Day(now.date_naive() + Days::days(90)));
        assert!(cards.meeting_alert());
        cards
            .config
            .enabled_widgets
            .retain(|widget| widget != "calendar");
        assert!(!cards.meeting_alert());
    }
}
