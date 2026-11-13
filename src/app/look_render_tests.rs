//! Look tuning: renders whole desktops (wallpaper, panel, a window, the real
//! widgets and the dock) for setups listed in a JSON file, with colors taken
//! from each wallpaper the same way the app derives them. Opt-in:
//!
//! `HALCYON_LOOKS=<looks.json> DESKTOP_CARDS_LAYOUT_EVIDENCE=<out dir>
//!  cargo test --bin cosmic-desktop-cards look_renders -- --ignored`
//!
//! `looks.json` is a list of `{"name", "wallpaper", "config"}`; `config` is
//! merged over the defaults (an `appearance.accent` there wins over the
//! wallpaper's).
use super::*;
use crate::desktop::wallpaper::{automatic, media, palette};
use crate::desktop_theme::Family;
use crate::native_test::Native;
use cosmic::{
    Application,
    iced::{
        Point, Size,
        widget::{pin, stack},
    },
};

/// The rendered screen, `HALCYON_SCREEN=WxH` (default 1920x1080). A
/// smaller screen gives the widgets more of a small preview.
fn screen() -> [f32; 2] {
    std::env::var("HALCYON_SCREEN")
        .ok()
        .and_then(|value| {
            let (w, h) = value.split_once('x')?;
            Some([w.parse().ok()?, h.parse().ok()?])
        })
        .unwrap_or([1920., 1080.])
}

fn merge(base: &mut serde_json::Value, patch: &serde_json::Value) {
    match (base, patch) {
        (serde_json::Value::Object(base), serde_json::Value::Object(patch)) => {
            for (key, value) in patch {
                merge(
                    base.entry(key.clone()).or_insert(serde_json::Value::Null),
                    value,
                );
            }
        }
        (base, patch) => *base = patch.clone(),
    }
}

/// The wallpaper scaled to cover the screen, and its suggestion.
fn wallpaper(path: &std::path::Path) -> (cosmic::widget::image::Handle, palette::Suggestion) {
    let picture = image::open(path)
        .unwrap_or_else(|error| panic!("{}: {error}", path.display()))
        .to_rgba8();
    let (w, h) = (picture.width() as f32, picture.height() as f32);
    let scale = (screen()[0] / w).max(screen()[1] / h);
    let resized = image::imageops::resize(
        &picture,
        (w * scale).ceil() as u32,
        (h * scale).ceil() as u32,
        image::imageops::FilterType::Triangle,
    );
    let x = (resized.width() - screen()[0] as u32) / 2;
    let y = (resized.height() - screen()[1] as u32) / 2;
    let cropped = image::imageops::crop_imm(&resized, x, y, screen()[0] as u32, screen()[1] as u32)
        .to_image();
    let suggestion = match media::thumbnail(path) {
        media::Preview::Ready(pixels) => palette::from_pixels(&pixels),
        media::Preview::None(reason) => panic!("{}: {reason}", path.display()),
    };
    let handle = cosmic::widget::image::Handle::from_rgba(
        cropped.width(),
        cropped.height(),
        cropped.into_raw(),
    );
    (handle, suggestion)
}

pub(super) fn sample_data(cards: &mut Cards) {
    let today = Local::now().date_naive();
    cards.weather = Some(Weather {
        current: data::Current {
            temperature_2m: 18.,
            apparent_temperature: 17.,
            weather_code: 2,
            wind_speed_10m: 8.,
            relative_humidity_2m: 52.,
            dew_point_2m: Some(9.),
            is_day: 1,
        },
        daily: data::Daily {
            time: (0..3)
                .map(|n| (today + Days::days(n)).to_string())
                .collect(),
            temperature_2m_max: vec![21., 20., 23.],
            temperature_2m_min: vec![12., 11., 13.],
            weather_code: vec![2, 3, 0],
            precipitation_probability_max: vec![Some(10.), Some(25.), None],
        },
    });
    // Later today, whatever the time of the render, as RFC 3339.
    let now = Local::now();
    let at = |hours: i64| (now + chrono::Duration::hours(hours)).to_rfc3339();
    let calendar = CalendarData {
        events: vec![
            CalendarEvent {
                title: "Standup".into(),
                start: at(1),
                end: at(2),
                all_day: false,
                url: String::new(),
                meeting_url: None,
                ..Default::default()
            },
            CalendarEvent {
                title: "Design review".into(),
                start: at(5),
                end: at(6),
                all_day: false,
                url: String::new(),
                meeting_url: None,
                ..Default::default()
            },
        ],
        month: Local::now().format("%Y-%m").to_string(),
        ..Default::default()
    };
    cards.calendar = Some(calendar.clone());
    cards.calendar_today = Some(calendar);
    cards.calendar_connection = crate::calendar::Connection {
        backend: crate::calendar::Backend::Google,
        account: "sample@example.com".into(),
        revision: "sample".into(),
        ..Default::default()
    };
    cards.metrics.cpu = 18.;
    cards.metrics.cpu_sampled = true;
    cards.metrics.cpu_temperature = Some(46.);
    cards.metrics.memory_used = 9.1;
    cards.metrics.memory_cache = 6.4;
    cards.metrics.memory_free = 15.5;
    cards.metrics.memory_total = 31.;
    cards.metrics.memory_sampled = true;
    cards.metrics.swap = Some((0., 20.));
    cards.metrics.disk = Some((128., 219.));
    cards.metrics.gpus = vec![Default::default()];
    let gpu = &mut cards.metrics.gpus[0];
    gpu.id = "sample".into();
    gpu.name = "Radeon 780M".into();
    gpu.utilization = Some(12.);
    gpu.memory = Some((1_200_000_000, 4_000_000_000));
    gpu.temperature = Some(41.);
    cards.metrics.network = Some(data::NetworkMetrics {
        download: Some(512_000.),
        upload: Some(48_000.),
        interfaces: 1,
    });
    cards.metrics.uptime = "2 days".into();
}

/// A COSMIC window in the look's native colors: title bar, sidebar, content
/// lines and an accent button.
fn window<'a>(
    background: Color,
    container_bg: Color,
    accent: Color,
    ink: Color,
) -> Element<'a, Message> {
    let fill = move |color: Color, radius: f32| {
        cosmic::theme::Container::custom(move |_| cosmic::iced::widget::container::Style {
            background: Some(Background::Color(color)),
            border: Border {
                radius: radius.into(),
                ..Default::default()
            },
            ..Default::default()
        })
    };
    let line = |width: f32, color: Color| -> Element<'a, Message> {
        container(cosmic::iced::widget::space())
            .width(width)
            .height(8.)
            .class(fill(color, 4.))
            .into()
    };
    let faint = Color { a: 0.35, ..ink };
    let title = container(
        row::with_children(vec![
            container(cosmic::iced::widget::space())
                .width(Length::Fill)
                .into(),
            text::body("Files")
                .size(13)
                .class(cosmic::theme::Text::Color(ink))
                .into(),
            container(cosmic::iced::widget::space())
                .width(Length::Fill)
                .into(),
            container(cosmic::iced::widget::space())
                .width(12.)
                .height(12.)
                .class(fill(accent, 6.))
                .into(),
        ])
        .align_y(Alignment::Center),
    )
    .padding([8, 12])
    .width(Length::Fill);
    let sidebar = container(
        column::with_children(vec![
            container(
                text::body("Home")
                    .size(12)
                    .class(cosmic::theme::Text::Color(ink)),
            )
            .padding([4, 8])
            .width(Length::Fill)
            .class(fill(Color { a: 0.25, ..accent }, 6.))
            .into(),
            line(90., faint),
            line(70., faint),
            line(80., faint),
        ])
        .spacing(10),
    )
    .padding(10)
    .width(150.)
    .height(Length::Fill)
    .class(fill(container_bg, 10.));
    let content = column::with_children(vec![
        line(260., faint),
        line(200., faint),
        line(230., faint),
        line(180., faint),
        container(
            text::body("Open")
                .size(12)
                .class(cosmic::theme::Text::Color(crate::desktop_theme::ink(
                    ink, accent,
                ))),
        )
        .padding([5, 14])
        .class(fill(accent, 14.))
        .into(),
    ])
    .spacing(14)
    .padding(16);
    container(column::with_children(vec![
        title.into(),
        row::with_children(vec![sidebar.into(), content.into()])
            .padding([0, 10, 10, 10])
            .height(Length::Fill)
            .into(),
    ]))
    .width(640.)
    .height(400.)
    .class(cosmic::theme::Container::custom(move |_| {
        cosmic::iced::widget::container::Style {
            background: Some(Background::Color(background)),
            border: Border {
                radius: 12.into(),
                color: Color { a: 0.6, ..accent },
                width: 1.,
            },
            shadow: cosmic::iced::Shadow {
                color: Color::from_rgba(0., 0., 0., 0.45),
                offset: cosmic::iced::Vector::new(0., 10.),
                blur_radius: 30.,
            },
            ..Default::default()
        }
    }))
    .into()
}

/// Where a look's saved position puts `widget`, as the desktop places it.
fn placed(config: &Config, widget: Widget, size: [f32; 2]) -> Option<[f32; 2]> {
    let position = config.widget_positions.get(widget.key())?;
    Some(crate::layout_state::decode_position(widget, *position, size, screen()))
}

fn rgb(c: [u8; 3]) -> Color {
    Color::from_rgb8(c[0], c[1], c[2])
}

#[test]
#[ignore = "look tuning renders; set HALCYON_LOOKS and DESKTOP_CARDS_LAYOUT_EVIDENCE"]
fn look_renders() {
    let Some(list) = std::env::var_os("HALCYON_LOOKS") else {
        return;
    };
    let looks: Vec<serde_json::Value> =
        serde_json::from_slice(&std::fs::read(&list).unwrap()).unwrap();
    for look in looks {
        let name = look["name"].as_str().unwrap().to_string();
        let path = std::path::PathBuf::from(look["wallpaper"].as_str().unwrap());
        let (picture, suggestion) = wallpaper(&path);
        let light = automatic::light(&suggestion);
        let recommended = automatic::recommended(&suggestion);
        let mut value = serde_json::to_value(Config::default()).unwrap();
        merge(
            &mut value,
            &serde_json::json!({
                "enabled_widgets": [],
                "location": {"name": "Belgrade", "latitude": 44.8, "longitude": 20.5},
                "appearance": {"wallpaper_light": light, "accent": recommended,
                    "color_source": "custom", "wallpaper_colors": true},
            }),
        );
        merge(&mut value, &look["config"]);
        let mut config: Config = serde_json::from_value(value).unwrap();
        if let Some(family) = config.appearance.family {
            let (o, r, b) = family.geometry();
            if look["config"].pointer("/appearance/opacity").is_none() {
                config.appearance.opacity = o;
            }
            if look["config"]
                .pointer("/appearance/corner_radius")
                .is_none()
            {
                config.appearance.corner_radius = r;
            }
            if look["config"]
                .pointer("/appearance/border_opacity")
                .is_none()
            {
                config.appearance.border_opacity = b;
            }
        }
        let family = config.appearance.family.unwrap_or(Family::Soft);
        let seed = config.appearance.accent.unwrap_or_else(|| family.accent());
        let (mut cards, _) = Cards::init(Core::default(), config.clone());
        cards.screen_size = screen();
        sample_data(&mut cards);
        // Sample notes, never the notes saved on this machine.
        cards.tools.note = Ok(crate::tools::notes::Note {
            text: "- [x] Book train tickets\n- [ ] Call the dentist\n- [ ] Pick up the cake"
                .into(),
            updated_at: Some(Local::now().to_rfc3339()),
        });
        cards.tools.draft = None;
        cards.sidebar.visible = true;
        cards.sidebar.progress = 1.;

        let mut layers: Vec<Element<'_, Message>> = vec![
            cosmic::widget::image(picture)
                .width(screen()[0])
                .height(screen()[1])
                .into(),
        ];
        // COSMIC's window colors as a style sets them for the variant it owns.
        let native_dark = !family.light();
        let window_bg = rgb(family.surface(seed));
        let container_bg = rgb(family.container(seed));
        let accent = rgb(crate::desktop_theme::native_accent(seed, native_dark));
        let ink = if native_dark {
            Color::from_rgb(0.93, 0.93, 0.95)
        } else {
            Color::from_rgb(0.12, 0.12, 0.14)
        };
        layers.push(
            pin(window(window_bg, container_bg, accent, ink))
                .position(Point::new(screen()[0] * 0.3, screen()[1] * 0.34))
                .into(),
        );
        // The widgets where the default layout puts them.
        let mut right_y = 48.;
        for widget in config.ordered_right_cards() {
            if !cards.config.widget_visible(widget) {
                continue;
            }
            let height = cards.widget_size(widget)[1];
            let at = placed(&config, widget, [360., height])
                .unwrap_or([screen()[0] - 42. - 360., right_y]);
            let view = match widget {
                Widget::Weather => cards.weather_card(),
                Widget::Calendar => cards.calendar_card(),
                Widget::Devbox => cards.devbox(),
                Widget::Projects => cards.projects_card(),
                _ => continue,
            };
            layers.push(
                pin(container(view).width(360.).height(height))
                    .position(Point::new(at[0], at[1]))
                    .into(),
            );
            right_y += height + 16.;
        }
        // The left column stacks from the top, so nothing overlaps.
        let mut left_y = 72.;
        for widget in [Widget::Glance, Widget::Focus, Widget::Notes, Widget::Music] {
            if !cards.config.widget_visible(widget) {
                continue;
            }
            let size = cards.widget_size(widget);
            let at = match placed(&config, widget, size) {
                Some(at) => at,
                None => {
                    left_y += size[1] + 16.;
                    [48., left_y - size[1] - 16.]
                }
            };
            let view = if widget == Widget::Glance {
                cards.glance()
            } else {
                cards.tool_card(widget)
            };
            layers.push(
                pin(container(view).width(size[0]).height(size[1]))
                    .position(Point::new(at[0], at[1]))
                    .into(),
            );
        }
        // The widget controls, open at the right edge, when the look asks.
        if look["rail"].as_bool().unwrap_or(false) {
            layers.push(
                pin(container(cards.control_rail())
                    .width(drawer::SIDEBAR_WIDTH as f32)
                    .height(drawer::SIDEBAR_HEIGHT as f32))
                .position(Point::new(
                    screen()[0] - drawer::SIDEBAR_WIDTH as f32,
                    (screen()[1] - drawer::SIDEBAR_HEIGHT as f32) / 2.,
                ))
                .into(),
            );
        }
        // Panel and dock: a dark tint of the wallpaper's color (like the
        // windows), as Islands (together or separate) or a bar, in the
        // look's shape. `bars` in looks.json: {"panel": "islands"|"bar",
        // "placement": "together"|"separate", "shape": "pill"|"rounded"|"square",
        // "dock_hides": bool}.
        let bars = &look["bars"];
        let islands = bars["panel"].as_str().unwrap_or("islands") == "islands";
        let separate = bars["placement"].as_str() == Some("separate");
        let shape = bars["shape"].as_str().unwrap_or("pill");
        let bar = family.spec().bar;
        let tone = {
            let c = family.surface(seed);
            Color::from_rgba8(c[0], c[1], c[2], bar.opacity.max(0.9))
        };
        let bar_ink = if family.light() {
            Color::from_rgb(0.12, 0.12, 0.14)
        } else {
            Color::from_rgb(0.92, 0.92, 0.95)
        };
        let panel_height = 34.;
        let radius = match shape {
            "pill" => panel_height / 2.,
            "rounded" => 10.,
            _ => 0.,
        };
        let piece =
            move |content: Element<'static, Message>, r: f32| -> Element<'static, Message> {
                container(content)
                    .padding([0., 14.])
                    .height(panel_height)
                    .align_y(Alignment::Center)
                    .class(cosmic::theme::Container::custom(move |_| {
                        cosmic::iced::widget::container::Style {
                            background: Some(Background::Color(tone)),
                            border: Border {
                                radius: r.into(),
                                ..Default::default()
                            },
                            ..Default::default()
                        }
                    }))
                    .into()
            };
        let dot = |color: Color, size: f32, radius: f32| -> Element<'static, Message> {
            container(cosmic::iced::widget::space())
                .width(size)
                .height(size)
                .class(cosmic::theme::Container::custom(move |_| {
                    cosmic::iced::widget::container::Style {
                        background: Some(Background::Color(color)),
                        border: Border {
                            radius: radius.into(),
                            ..Default::default()
                        },
                        ..Default::default()
                    }
                }))
                .into()
        };
        let workspaces = move || -> Element<'static, Message> {
            row::with_children(vec![
                dot(accent, 8., 4.),
                dot(Color { a: 0.5, ..bar_ink }, 8., 4.),
                dot(Color { a: 0.5, ..bar_ink }, 8., 4.),
            ])
            .spacing(6)
            .align_y(Alignment::Center)
            .into()
        };
        // The same moment the clock widget shows.
        let now = Local::now().format("%a %b %-d  %H:%M").to_string();
        let clock = move || -> Element<'static, Message> {
            text::body(now.clone())
                .size(13)
                .class(cosmic::theme::Text::Color(bar_ink))
                .into()
        };
        let tray = move || -> Element<'static, Message> {
            row::with_children(vec![
                dot(Color { a: 0.8, ..bar_ink }, 10., 3.),
                dot(Color { a: 0.8, ..bar_ink }, 10., 3.),
                dot(Color { a: 0.8, ..bar_ink }, 10., 3.),
            ])
            .spacing(8)
            .align_y(Alignment::Center)
            .into()
        };
        let gap = 8.;
        let panel: Element<'_, Message> = if !islands {
            piece(
                row::with_children(vec![
                    container(workspaces()).width(Length::Fill).into(),
                    clock(),
                    container(tray())
                        .width(Length::Fill)
                        .align_x(Alignment::End)
                        .into(),
                ])
                .align_y(Alignment::Center)
                .into(),
                if shape == "square" { 0. } else { radius },
            )
        } else if separate {
            container(
                row::with_children(vec![
                    container(piece(workspaces(), radius))
                        .width(Length::Fill)
                        .into(),
                    piece(clock(), radius),
                    container(piece(tray(), radius))
                        .width(Length::Fill)
                        .align_x(Alignment::End)
                        .into(),
                ])
                .align_y(Alignment::Center),
            )
            .width(screen()[0] - 2. * gap)
            .into()
        } else {
            container(
                row::with_children(vec![
                    piece(workspaces(), radius),
                    piece(clock(), radius),
                    piece(tray(), radius),
                ])
                .spacing(6)
                .align_y(Alignment::Center),
            )
            .width(screen()[0] - 2. * gap)
            .align_x(Alignment::Center)
            .into()
        };
        let panel = if islands {
            panel
        } else {
            container(panel).width(screen()[0] - 2. * gap).into()
        };
        layers.push(pin(panel).position(Point::new(gap, gap)).into());
        let icon_radius = match shape {
            "pill" => 12.,
            "rounded" => 8.,
            _ => 2.,
        };
        let icons: Vec<Element<'_, Message>> = [1.0, 0.85, 0.7, 0.85, 0.6, 0.75]
            .into_iter()
            .enumerate()
            .map(|(index, alpha)| {
                let color = if index == 0 {
                    accent
                } else {
                    Color {
                        a: alpha,
                        ..bar_ink
                    }
                };
                dot(color, 40., icon_radius)
            })
            .collect();
        let dock_radius = match shape {
            "pill" => 28.,
            "rounded" => 14.,
            _ => 0.,
        };
        let dock = container(row::with_children(icons).spacing(10))
            .padding(8)
            .class(cosmic::theme::Container::custom(move |_| {
                cosmic::iced::widget::container::Style {
                    background: Some(Background::Color(tone)),
                    border: Border {
                        radius: dock_radius.into(),
                        ..Default::default()
                    },
                    ..Default::default()
                }
            }));
        if !bars["dock_hides"].as_bool().unwrap_or(false) {
            layers.push(
                container(dock)
                    .width(screen()[0])
                    .height(screen()[1])
                    .align_x(Alignment::Center)
                    .align_y(Alignment::End)
                    .padding([0., 0., gap, 0.])
                    .into(),
            );
        }
        let theme = if native_dark {
            cosmic::Theme::dark()
        } else {
            cosmic::Theme::light()
        };
        let mut native = Native::new(Size::new(screen()[0], screen()[1]));
        native.capture(
            stack(layers).width(screen()[0]).height(screen()[1]).into(),
            &theme,
            &format!("look-{name}"),
        );
        println!(
            "{name}: accent #{:02X}{:02X}{:02X}, wallpaper {}",
            seed[0],
            seed[1],
            seed[2],
            if light { "light" } else { "dark" }
        );
    }
}
