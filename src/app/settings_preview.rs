//! Settings examples reuse the actual desktop cards. These fixed readings are
//! illustrative: no samplers, services, filesystem reads or network requests.
use super::history::MetricHistories;
use super::*;
use crate::data::{GpuMetrics, NetworkMetrics};
use std::sync::OnceLock;

pub(in crate::app) struct MonitorSample {
    pub(in crate::app) metrics: Metrics,
    histories: MetricHistories,
}

pub(in crate::app) fn monitor_sample() -> &'static MonitorSample {
    static SAMPLE: OnceLock<MonitorSample> = OnceLock::new();
    SAMPLE.get_or_init(|| {
        let metrics = Metrics {
            cpu: 26.,
            cpu_sampled: true,
            cpu_temperature: Some(48.),
            memory_used: 8.2,
            memory_cache: 7.6,
            memory_free: 16.2,
            memory_total: 32.,
            memory_sampled: true,
            swap: Some((0.3, 4.)),
            disk: Some((166., 512.)),
            network: Some(NetworkMetrics {
                download: Some(2.4 * 1_048_576.),
                upload: Some(128. * 1024.),
                interfaces: 1,
            }),
            gpus: vec![GpuMetrics {
                id: "settings-sample-gpu".into(),
                name: String::new(),
                utilization: Some(18.),
                utilization_at: None,
                memory: Some((2_147_483_648, 8_589_934_592)),
                temperature: Some(46.),
                asleep: false,
            }],
            uptime: "3h 24m".into(),
            t3: true,
            dictation: "idle".into(),
        };
        let histories = MetricHistories::settings_sample(&metrics);
        MonitorSample { metrics, histories }
    })
}

/// Callers label this as sample data and ignore its desktop actions. Width is
/// supplied by the enclosing Settings preview; the card uses automatic height.
pub(crate) fn settings_devbox_preview(config: &Config) -> Element<'static, Message> {
    if let Some(recipe) = &config.recipes.devbox {
        return recipe::system::sample(config, recipe, false, [360., recipe.natural_height()]);
    }
    let sample = monitor_sample();
    devbox::view(
        config,
        &sample.metrics,
        &sample.histories,
        devbox::natural_height(config, sample.metrics.gpus.len()),
    )
}

/// Uses the selected city and card style, with illustrative conditions. Dates
/// follow today so an open Settings window never shows an expired sample week.
pub(crate) fn settings_weather_preview(config: &Config) -> Element<'static, Message> {
    settings_weather_preview_sized(config, crate::widget_presentation::WEATHER_SIZE)
}
pub(crate) fn settings_weather_preview_sized(
    config: &Config,
    size: [f32; 2],
) -> Element<'static, Message> {
    settings_weather_preview_at(config, size, Local::now())
}
pub(crate) fn settings_weather_preview_at(
    config: &Config,
    size: [f32; 2],
    now: chrono::DateTime<Local>,
) -> Element<'static, Message> {
    let weather = weather_sample(now.date_naive());
    weather::Presentation::new(
        config,
        weather::Snapshot {
            weather: Some(&weather),
            updated_at: None,
            failed: false,
        },
        now,
    )
    .view(crate::widget_presentation::Context::LocalPreview, size)
}
pub(super) fn weather_sample(today: NaiveDate) -> Weather {
    let weather = Weather {
        current: data::Current {
            temperature_2m: 21.,
            apparent_temperature: 20.,
            weather_code: 2,
            wind_speed_10m: 8.,
            relative_humidity_2m: 52.,
            dew_point_2m: Some(10.8),
            is_day: 1,
        },
        daily: data::Daily {
            time: (0..3)
                .map(|offset| (today + Days::days(offset)).format("%Y-%m-%d").to_string())
                .collect(),
            temperature_2m_max: vec![23., 22., 24.],
            temperature_2m_min: vec![14., 13., 15.],
            weather_code: vec![2, 3, 0],
            precipitation_probability_max: vec![Some(10.), Some(25.), Some(0.)],
        },
    };
    weather
}

/// Projects preview needs only immutable sample readings and the chosen style.
/// The caller maps every action to Preview, so links and refresh stay inactive.
pub(crate) fn settings_projects_preview(config: &Config) -> Element<'static, Message> {
    static SAMPLE: OnceLock<(ProjectsData, std::collections::BTreeSet<String>)> = OnceLock::new();
    let (data, expanded) = SAMPLE.get_or_init(|| {
        let projects=serde_json::from_value(serde_json::json!({"updated_at": Local::now().to_rfc3339(),"projects":[{
            "path":"/sample/atlas","name":"Atlas","title":"Atlas","repository_id":"sample-atlas",
            "group":"sample", "upstream":true, "error":false,
            "branch":"main","changed":2,"untracked":0,"conflicts":0,"ahead":0,"behind":0,
            "servers":[{"port":5173,"web":true}],"recent":0.0
        }]})).expect("static project preview is valid");
        (projects, Default::default())
    });
    let view = projects::Presentation {
        config,
        projects: Some(data),
        projects_fetching: false,
        projects_error: false,
        all_projects: false,
        expanded_projects: expanded,
    };
    view.view(view.natural_height(360.), false)
}

/// Offline examples for the calendar and overview; no workers,
/// account lookup, note loading, file reads or compositor tasks are started.
pub(crate) struct SettingsSamples {
    cards: Cards,
}
impl SettingsSamples {
    pub fn new(config: &Config) -> Self {
        let core = Core::default();
        let ids = std::array::from_fn(|_| window::Id::unique());
        let sidebar = Sidebar::new();
        let today = Local::now().date_naive();
        let month = today.with_day(1).unwrap_or(today);
        let calendar = Some(Self::calendar());
        let weather = None::<WeatherSnapshot>;
        let calendar_refresh = Duration::from_secs(3600);
        let source = config;
        let config = config.clone();
        let cards = Cards {
            calendar_details: None,
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
            screen_size: [1920., 1080.],
            output_known: false,
            sidebar,
            desktop_height: 360.,
            calendar_height: 360.,
            projects: None,
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
                crate::widget_presentation::Context::LocalPreview,
            ),
            weather_cache: Default::default(),
            capture_key: None,
            calendar_today: calendar.clone(),
            today_fetching: None,
            calendar_today_error: false,
            calendar_preview: true,
            next_today_calendar: Instant::now() + calendar_refresh,
            calendar,
            calendar_connection: crate::calendar::Connection {
                backend: crate::calendar::Backend::Google,
                account: "sample@example.com".into(),
                ..Default::default()
            },
            calendar_error: false,
            calendar_fetching: None,
            next_calendar: Instant::now(),
            month,
            selected_day: today,
            calendar_home: true,
            last_today: today,
            show_earlier: false,
            tools: tools::Tools::sample(),
            automator: crate::automation::runtime::Automator::with_store(None),
            woke_at: (Instant::now(), SystemTime::now()),
            wake_generation: 0,
            depth: Default::default(),
            rain: Default::default(),
            wallpaper_color_generation: Default::default(),
        };
        let mut samples = Self { cards };
        samples.update(source);
        samples
    }
    pub(in crate::app) fn calendar() -> CalendarData {
        let now = Local::now();
        CalendarData {
            month: now.format("%Y-%m").to_string(),
            updated_at: now.to_rfc3339(),
            events: [
                ("Design review", 9),
                ("Project planning", 80),
                ("Team check-in", 1500),
            ]
            .into_iter()
            .map(|(title, minutes)| CalendarEvent {
                title: title.into(),
                start: (now + Days::minutes(minutes)).to_rfc3339(),
                end: (now + Days::minutes(minutes + 30)).to_rfc3339(),
                all_day: false,
                url: String::new(),
                meeting_url: Some("https://meet.google.com/".into()),
                ..Default::default()
            })
            .collect(),
            ..Default::default()
        }
    }
    pub fn update(&mut self, config: &Config) {
        self.cards.config = config.clone();
        self.cards.config.widget_sizes.remove("calendar");
        self.cards.config.widget_sizes.remove("projects");
        self.cards.config.widget_positions.clear();
        // The Calendar editor previews the agenda part of Daily overview.
        // Its style and Tomorrow preference are those of the actual widget.
        self.cards.config.glance.show_clock = false;
        self.cards.config.glance.show_weather = false;
        self.cards.config.glance.show_calendar = true;
        let today = Local::now().date_naive();
        if today != self.cards.last_today {
            self.cards.calendar = Some(Self::calendar());
            self.cards.calendar_today = self.cards.calendar.clone();
            self.cards.month = today.with_day(1).unwrap_or(today);
            self.cards.last_today = today;
            self.cards.selected_day = today;
        }
        self.cards
            .config
            .widget_sizes
            .insert("glance".into(), [320., 480.]);
        let height = self.cards.glance_natural_height().min(520.);
        self.cards
            .config
            .widget_sizes
            .insert("glance".into(), [320., height]);
    }
    pub fn calendar_view(&self) -> Element<'_, Message> {
        if self.cards.config.glance.enabled {
            self.cards.glance()
        } else {
            self.cards.calendar_card()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn offline_samples_reach_calendar_glance_and_projects_renderers() {
        let mut config = Config::default();
        config.glance.enabled = true;
        config.glance.show_tomorrow = true;
        let mut samples = SettingsSamples::new(&config);
        let today = Local::now().date_naive();
        assert_eq!(
            samples.cards.calendar.as_ref().unwrap().month,
            today.format("%Y-%m").to_string()
        );
        assert!(
            samples
                .cards
                .glance_days(today)
                .iter()
                .map(|&day| samples.cards.glance_events(day).len())
                .sum::<usize>()
                >= 2
        );
        assert_eq!(
            samples.cards.config.glance.show_tomorrow,
            config.glance.show_tomorrow
        );
        let mut native = crate::native_test::Native::new(cosmic::iced::Size::new(360., 520.));
        for combined in [true, false] {
            config.glance.enabled = combined;
            samples.update(&config);
            let pixels = native.render(samples.calendar_view(), &cosmic::Theme::dark());
            assert!(pixels.chunks_exact(4).any(|pixel| pixel[3] > 0));
        }
        // Projects now supplies its own immutable sample to the shared
        // renderer, independently of the calendar's desktop-runtime fixture.
        assert!(samples.cards.projects.is_none());
        let pixels = native.render(settings_projects_preview(&config), &cosmic::Theme::dark());
        assert!(pixels.chunks_exact(4).any(|pixel| pixel[3] > 0));
    }
}
