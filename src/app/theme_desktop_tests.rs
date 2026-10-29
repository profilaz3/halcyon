//! Desktop compositions for judging shared colors on real wallpapers: the
//! production widget renderers over the actual picture, with the native
//! roles a theme writes drawn as simple bars and an app window (its active
//! outline included). Captures only; nothing is applied.
use super::*;
use crate::desktop::appearance::ThemeVariant;
use crate::desktop::profiles::themes;
use crate::desktop::wallpaper::{automatic, media, palette};
use crate::desktop_theme::{Family, tone};
use crate::native_test::Native;
use cosmic::{
    Application,
    iced::{
        Point, Size,
        widget::{pin, stack},
    },
};

const SIZE: [f32; 2] = [1600., 1000.];

fn sample_cards() -> Cards {
    let today = Local::now().date_naive();
    let mut cfg = Config::default();
    cfg.location = Some(Location {
        name: "Sample city".into(),
        latitude: 0.,
        longitude: 0.,
        timezone: None,
    });
    cfg.weather.show_dew_point = true;
    let (mut cards, _) = Cards::init(Core::default(), cfg);
    cards.weather = Some(Weather {
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
                .map(|n| (today + Days::days(n)).to_string())
                .collect(),
            temperature_2m_max: vec![23., 22., 24.],
            temperature_2m_min: vec![14., 13., 15.],
            weather_code: vec![2, 3, 0],
            precipitation_probability_max: vec![Some(10.), Some(25.), None],
        },
    });
    let calendar = CalendarData {
        events: vec![CalendarEvent {
            title: "Design review".into(),
            start: today.to_string(),
            end: (today + Days::days(1)).to_string(),
            all_day: true,
            url: "https://example.com/event".into(),
            meeting_url: Some("https://example.com/join".into()),
            ..Default::default()
        }],
        ..Default::default()
    };
    cards.calendar = Some(calendar.clone());
    cards.calendar_today = Some(calendar);
    for (widget, size) in [
        (Widget::Clock, [380., 130.]),
        (Widget::Weather, [360., 220.]),
        (Widget::Calendar, [360., 400.]),
        (Widget::Devbox, [360., 360.]),
        (Widget::Projects, [330., 210.]),
    ] {
        cards.config.widget_sizes.insert(widget.key().into(), size);
    }
    cards
}

fn view(cards: &Cards, widget: Widget) -> Element<'_, Message> {
    match widget {
        Widget::Clock => cards.clock(),
        Widget::Weather => cards.weather_card(),
        Widget::Calendar => cards.calendar_card(),
        Widget::Devbox => cards.devbox(),
        _ => cards.projects_card(),
    }
}

fn block<'a>(
    size: [f32; 2],
    color: Color,
    border: Color,
    width: f32,
    radius: f32,
) -> Element<'a, Message> {
    container(cosmic::iced::widget::space())
        .width(size[0])
        .height(size[1])
        .class(cosmic::theme::Container::custom(move |_| {
            cosmic::iced::widget::container::Style {
                background: Some(Background::Color(color)),
                border: Border {
                    color: border,
                    width,
                    radius: radius.into(),
                },
                ..Default::default()
            }
        }))
        .into()
}

fn place<'a>(element: Element<'a, Message>, at: [f32; 2]) -> Element<'a, Message> {
    pin(element).position(Point::new(at[0], at[1])).into()
}

/// The native roles the selected look writes, per variant.
struct NativeLook {
    accent: [u8; 3],
    background: [u8; 3],
    container: [u8; 3],
}
fn native(family: Family, accent: Option<[u8; 3]>, dark: bool) -> NativeLook {
    let look = themes::appearance(family, accent);
    let variant = look
        .variant(if dark {
            ThemeVariant::Dark
        } else {
            ThemeVariant::Light
        })
        .cloned()
        .unwrap_or_default();
    let rgb = |choice: Option<crate::desktop::profiles::ColorChoice>, fallback: [u8; 3]| {
        choice
            .and_then(|c| c.color(crate::desktop::appearance::ColorRole::Accent).ok())
            .flatten()
            .map(|c| [c.red, c.green, c.blue].map(|v| (v * 255.).round() as u8))
            .unwrap_or(fallback)
    };
    let (background, container) = if dark {
        ([27, 27, 27], [39, 39, 39])
    } else {
        ([228, 228, 228], [243, 243, 243])
    };
    NativeLook {
        accent: rgb(variant.accent, [99, 208, 223]),
        background: rgb(variant.background, background),
        container: rgb(variant.primary, container),
    }
}

fn desktop<'a>(
    cards: &'a Cards,
    wallpaper: &cosmic::widget::image::Handle,
    family: Family,
    dark_mode: bool,
) -> Element<'a, Message> {
    let accent = cards.config.appearance.accent;
    let bars = native(family, accent, !family.light());
    let window = native(family, accent, dark_mode);
    let (opacity, radius, _, _) = family.bar();
    let bar = Color {
        a: opacity,
        ..tone::color(bars.background)
    };
    let bar_ink = crate::desktop_theme::ink(Color::WHITE, tone::color(bars.background));
    let mut layers = vec![
        cosmic::widget::image(wallpaper.clone())
            .width(SIZE[0])
            .height(SIZE[1])
            .content_fit(cosmic::iced::ContentFit::Cover)
            .into(),
    ];
    for (widget, at) in [
        (Widget::Clock, [74., 92.]),
        (Widget::Calendar, [72., 300.]),
        (Widget::Projects, [611., 600.]),
        (Widget::Weather, [1144., 92.]),
        (Widget::Devbox, [1144., 340.]),
    ] {
        let size = cards.config.widget_sizes[widget.key()];
        layers.push(place(
            container(view(cards, widget))
                .width(size[0])
                .height(size[1])
                .into(),
            at,
        ));
    }
    // An active COSMIC window: its background, a container, an accent
    // button and the active outline, which follows the accent.
    let win_bg = tone::color(window.background);
    let win_on = crate::desktop_theme::ink(Color::WHITE, win_bg);
    let faint = Color { a: 0.35, ..win_on };
    layers.push(place(
        block([540., 300.], win_bg, tone::color(window.accent), 3., 12.),
        [540., 150.],
    ));
    layers.push(place(
        block(
            [150., 236.],
            tone::color(window.container),
            Color::TRANSPARENT,
            0.,
            8.,
        ),
        [556., 202.],
    ));
    layers.push(place(
        block(
            [120., 26.],
            tone::color(window.accent),
            Color::TRANSPARENT,
            0.,
            13.,
        ),
        [571., 218.],
    ));
    for (y, w) in [(222., 300.), (252., 240.), (282., 280.)] {
        layers.push(place(
            block([w, 8.], faint, Color::TRANSPARENT, 0., 4.),
            [724., y],
        ));
    }
    layers.push(place(
        text::body("Files")
            .size(13)
            .class(cosmic::theme::Text::Color(win_on))
            .into(),
        [560., 163.],
    ));
    // Panel and dock paint the owned variant's background.
    layers.push(place(
        block([1584., 40.], bar, Color::TRANSPARENT, 0., radius as f32),
        [8., 6.],
    ));
    layers.push(place(
        text::body("Workspaces    Launcher")
            .size(14)
            .class(cosmic::theme::Text::Color(bar_ink))
            .into(),
        [28., 16.],
    ));
    layers.push(place(
        text::body("Oct 20  12:51")
            .size(14)
            .class(cosmic::theme::Text::Color(bar_ink))
            .into(),
        [750., 16.],
    ));
    layers.push(place(
        block([640., 76.], bar, Color::TRANSPARENT, 0., radius as f32),
        [480., 912.],
    ));
    for n in 0..7 {
        layers.push(place(
            block(
                [52., 52.],
                Color {
                    a: 0.85,
                    ..tone::color(bars.accent)
                },
                Color::TRANSPARENT,
                0.,
                14.,
            ),
            [500. + n as f32 * 88., 924.],
        ));
    }
    container(stack(layers).width(SIZE[0]).height(SIZE[1]).clip(true)).into()
}

fn wallpapers() -> Vec<(&'static str, std::path::PathBuf)> {
    let bundled = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("assets/wallpapers");
    let mut list = vec![
        ("amber", bundled.join("amber.jpg")),
        ("teal", bundled.join("teal.jpg")),
        ("graphite", bundled.join("graphite.jpg")),
        ("lavender", bundled.join("lavender.jpg")),
        ("rose", bundled.join("rose.jpg")),
        ("ocean", bundled.join("ocean.jpg")),
    ];
    for (name, file) in [
        (
            "phytoplankton",
            "phytoplankton_bloom_nasa_oli2_20240121.jpg",
        ),
        ("orion", "orion_nebula_nasa_heic0601a.jpg"),
        ("webb", "webb-inspired-wallpaper-system76.jpg"),
        ("pattern-light", "COSMIC-Pattern-Light.png"),
        ("earth", "otherworldly_earth_nasa_ISS064-E-29444.jpg"),
        ("tarantula", "tarantula_nebula_nasa_PIA23646.jpg"),
        ("logo-white", "COSMIC-logo-White.png"),
    ] {
        let path = std::path::Path::new("/usr/share/backgrounds/cosmic").join(file);
        if path.is_file() {
            list.push((name, path));
        }
    }
    list
}

/// Set `DESKTOP_CARDS_THEME_DESKTOPS=1` with `DESKTOP_CARDS_LAYOUT_EVIDENCE`
/// to write the compositions; otherwise this checks one wallpaper quickly.
#[test]
fn shared_colors_on_real_wallpapers_stay_lively_and_readable() {
    let all = std::env::var_os("DESKTOP_CARDS_THEME_DESKTOPS").is_some();
    let mut cards = sample_cards();
    let only = std::env::var("DESKTOP_CARDS_THEME_ONLY").ok();
    for (name, path) in wallpapers()
        .into_iter()
        .filter(|(name, _)| only.as_deref().is_none_or(|only| only == *name))
        .take(if all { usize::MAX } else { 1 })
    {
        let media::Preview::Ready(pixels) = media::thumbnail(&path) else {
            panic!("{name} decodes");
        };
        let suggestion = palette::from_pixels(&pixels);
        let light = automatic::light(&suggestion);
        if all {
            let describe = |rgb: [u8; 3]| {
                let l = tone::lch(rgb);
                format!("{rgb:?} L{:.2} C{:.3}", l.l, l.c)
            };
            println!("{name}: light {light}");
            for swatch in &suggestion.swatches {
                println!(
                    "  swatch {} share {:.2}",
                    describe(swatch.rgb),
                    swatch.share
                );
            }
            for accent in &suggestion.accents {
                println!("  accent {}", describe(*accent));
            }
        }
        let handle = cosmic::widget::image::Handle::from_rgba(
            pixels.width,
            pixels.height,
            pixels.rgba.clone(),
        );
        for family in Family::ALL {
            let accent = automatic::recommended_for(&suggestion, Some(family));
            let a = &mut cards.config.appearance;
            a.family = Some(family);
            (a.opacity, a.corner_radius, a.border_opacity) = family.geometry();
            a.accent = accent;
            a.wallpaper_light = light;
            a.wallpaper_colors = true;
            let roles = cards.config.theme_roles().unwrap();
            if !roles.transparent {
                for ink in [roles.card.foreground, roles.card.muted, roles.card.accent] {
                    assert!(
                        tone::contrast(
                            ink,
                            Color {
                                a: 1.,
                                ..roles.card.background
                            }
                        ) >= 4.5,
                        "{name} {family:?}"
                    );
                }
            }
            if all && std::env::var_os("DESKTOP_CARDS_THEME_PRINT").is_none() {
                for dark in [true, false] {
                    let mut native = Native::new(Size::new(SIZE[0], SIZE[1]));
                    native.capture(
                        desktop(&cards, &handle, family, dark),
                        &if dark {
                            cosmic::Theme::dark()
                        } else {
                            cosmic::Theme::light()
                        },
                        &format!(
                            "desktop-{name}-{}-{}",
                            family.id(),
                            if dark { "dark" } else { "light" }
                        ),
                    );
                }
            }
        }
    }
}
