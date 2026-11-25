//! The tool cards, drawn the same way on the desktop and as the live
//! previews in Settings. Each takes the palette and card look from the
//! configuration, like every other widget, and an optional action mapper:
//! without one the card is drawn without controls that respond.
mod devices;
mod folder;
mod markets;
mod tiles;
pub use folder::{folder, folder_in};
pub use markets::{markets, sample_for as sample_markets_for};
#[cfg(test)]
pub use markets::sample as sample_markets;
pub(crate) mod messages;
pub(crate) mod recipe;
pub use tiles::tiles;
use super::{focus, music, notes};
use crate::config::{Config, Palette};
use crate::drawer::Widget;
use cosmic::Element;
use cosmic::iced::advanced::text as advanced_text;
use cosmic::iced::{Alignment, Background, Border, Color, Font, Length};
use cosmic::widget::image::Handle;
use cosmic::widget::{column, container, icon, row, text, text_editor};
pub(crate) use devices::{SOURCES as DEVICE_SOURCES, charging_icon};
pub use devices::{devices, sample as sample_devices};
pub use messages::{messages, sample as sample_messages};
#[cfg(test)]
pub(crate) use messages::{
    rows_fitting as messages_rows, status as messages_status, when as messages_when,
};
#[cfg(test)]
pub(crate) use devices::{fitted as devices_fitted, grid as devices_grid};
use std::time::Instant;

/// What a card's controls ask for.
#[derive(Clone, Debug)]
pub enum Action {
    Music(music::TransportRequest),
    /// Follow the next running player instead.
    MusicCycle,
    MusicAuto,
    MusicSeek(music::SeekRequest),
    NoteCopy,
    NoteEdit,
    NoteSave,
    NoteCancel,
    /// Bring back the draft Cancel just discarded.
    NoteUndo,
    /// The chance to undo discard `serial` has passed.
    NoteUndoExpired(u64),
    NoteAction(text_editor::Action),
    /// Tick or untick the checklist item on this line.
    NoteToggle(usize),
    FocusToggle,
    FocusReset,
    FocusSkip,
    FocusMinutes(u16),
    /// Open the phone's conversations, or KDE Connect to set it up.
    MessagesOpen,
    MarketsOpen,
    MarketSelect(String),
}

pub const NOTE_EDITOR: &str = "desktop-cards-note-editor";

const PADDING: u16 = crate::widget_style::PADDING;
const WARN: Color = Color::from_rgb(233. / 255., 192. / 255., 124. / 255.);

/// The card's logical size on the desktop.
pub fn size(widget: Widget, config: &Config) -> [f32; 2] {
    if widget == Widget::Music
        && config.music.visualizer.circle != crate::tools::visualizer::Circle::Off
    {
        return [300., 300.];
    }
    if let Some(recipe) = match widget {
        Widget::Music => config.recipes.music.as_ref(),
        Widget::Focus => config.recipes.focus.as_ref(),
        _ => None,
    } {
        return recipe.tool_size();
    }
    match widget {
        Widget::Music => [
            360.,
            if config.music.show_progress {
                140.
            } else {
                118.
            },
        ],
        Widget::Notes => config.notes.size.card(),
        // Room for the four newest conversations.
        Widget::Messages => [320., 260.],
        Widget::Folder => [344., 234.],
        Widget::Tiles => tiles::size(&config.tiles.tiles),
        Widget::Markets => [
            if !config.markets.show_chart && config.markets.symbols.len() == 1 { 280. } else { 360. },
            config.markets.height_limit().unwrap_or(420.),
        ],
        _ => [280., 160.],
    }
}

fn font() -> Font {
    crate::widget_style::font()
}
fn label<'a, M: 'a>(value: impl Into<String>, size: f32, color: Color) -> Element<'a, M> {
    text::body(value.into())
        .size(size.max(crate::widget_style::META))
        .line_height(1.25)
        .font(font())
        .class(cosmic::theme::Text::Color(crate::widget_style::semantic(
            color,
        )))
        .into()
}
/// One line, cut with an ellipsis.
fn line<'a, M: 'a>(value: impl Into<String>, size: f32, color: Color) -> Element<'a, M> {
    text::body(value.into())
        .size(size.max(crate::widget_style::META))
        .line_height(1.25)
        .font(font())
        .wrapping(advanced_text::Wrapping::None)
        .ellipsize(advanced_text::Ellipsize::End(
            advanced_text::EllipsizeHeightLimit::Lines(1),
        ))
        .class(cosmic::theme::Text::Color(crate::widget_style::semantic(
            color,
        )))
        .into()
}
fn light<'a, M: 'a>(value: impl Into<String>, size: f32, color: Color) -> Element<'a, M> {
    text::body(value.into())
        .size(size)
        .line_height(1.05)
        .font(Font {
            weight: cosmic::iced::font::Weight::Light,
            ..font()
        })
        .class(cosmic::theme::Text::Color(crate::widget_style::semantic(
            color,
        )))
        .into()
}
/// A card title, the same size and ink as the Weather and Projects titles.
fn card_title<'a, M: 'a>(value: impl Into<String>, p: Palette) -> Element<'a, M> {
    line(value, crate::widget_style::TITLE, p.foreground)
}
fn fill<'a, M: 'a>() -> Element<'a, M> {
    container(cosmic::iced::widget::space())
        .width(Length::Fill)
        .into()
}
fn mix(a: Color, b: Color, amount: f32) -> Color {
    crate::widget_style::tint(a, b, amount)
}

/// The shared card look: palette background and opacity, accent outline
/// and the configured corners.
pub fn frame<'a, M: 'a>(content: Element<'a, M>, config: &Config, height: f32) -> Element<'a, M> {
    frame_with_light(content, config, height, None)
}

fn frame_with_light<'a, M: 'a>(
    content: Element<'a, M>,
    config: &Config,
    height: f32,
    light: Option<crate::materials::AlbumLight>,
) -> Element<'a, M> {
    let _style = crate::widget_style::scope(config);
    let p = config.palette();
    let appearance = config.appearance;
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
            .height(Length::Fill),
    )
    .padding(PADDING)
    .width(Length::Fill)
    .height(Length::Fixed(height));
    let card = match light {
        Some(light) => container(crate::materials::album_light(content.into(), light))
            .width(Length::Fill).height(height),
        None => content,
    };
    let content = card.class(cosmic::theme::Container::custom(move |_| {
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

fn tinted<'a, M: 'a>(
    content: Element<'a, M>,
    background: Color,
    icon_color: Color,
    padding: [u16; 2],
    radius: f32,
) -> Element<'a, M> {
    container(content)
        .padding(padding)
        .align_y(Alignment::Center)
        .class(cosmic::theme::Container::custom(move |_| {
            cosmic::iced::widget::container::Style {
                icon_color: Some(icon_color),
                background: Some(Background::Color(background)),
                border: Border {
                    radius: radius.into(),
                    ..Default::default()
                },
                ..Default::default()
            }
        }))
        .into()
}

fn press<'a, M: Clone + 'a>(
    content: Element<'a, M>,
    message: Option<M>,
    name: impl Into<String>,
) -> Element<'a, M> {
    match message {
        Some(message) => cosmic::widget::button::custom(content)
            .name(name.into())
            .padding(0)
            .class(cosmic::theme::Button::Text)
            .on_press(message)
            .into(),
        None => content,
    }
}

/// A round icon control; dimmed when it can't be used.
fn control<'a, M: Clone + 'a>(
    name: &'static str,
    size: u16,
    message: Option<M>,
    p: Palette,
    primary: bool,
) -> Element<'a, M> {
    let enabled = message.is_some();
    let color = if !enabled {
        Color { a: 0.35, ..p.muted }
    } else if primary {
        p.accent
    } else {
        p.muted
    };
    let background = if primary && enabled {
        Color {
            a: 0.16,
            ..p.accent
        }
    } else {
        Color::TRANSPARENT
    };
    let pad = if primary { 8 } else { 6 };
    let glyph = || -> Element<'a, M> { icon::from_name(name).size(size).icon().into() };
    let content = if enabled && !primary {
        // Muted at rest; the same glyph in the accent is drawn over it only
        // while the pointer is on the control.
        let ink = move |color: Color, glyph: Element<'a, M>| -> Element<'a, M> {
            container(glyph)
                .class(cosmic::theme::Container::custom(move |_| {
                    cosmic::iced::widget::container::Style {
                        icon_color: Some(color),
                        ..Default::default()
                    }
                }))
                .into()
        };
        cosmic::iced::widget::hover(ink(color, glyph()), ink(p.accent, glyph()))
    } else {
        glyph()
    };
    let title = match name {
        "media-playback-start-symbolic" => "Play or resume",
        "media-playback-pause-symbolic" => "Pause",
        "media-skip-forward-symbolic" => "Next track",
        "media-skip-backward-symbolic" => "Previous track",
        "edit-copy-symbolic" => "Copy note",
        "edit-symbolic" => "Edit note",
        _ => name,
    };
    press(tinted(content, background, color, [pad, pad], 40.), message, title)
}

/// A text control with an optional leading icon. A strong chip is the
/// card's main action: filled with the accent, drawn in the card colour.
pub(crate) fn chip<'a, M: Clone + 'a>(
    name: Option<&'static str>,
    title: impl Into<String>,
    message: Option<M>,
    p: Palette,
    strong: bool,
) -> Element<'a, M> {
    let filled = strong && message.is_some();
    let color = if message.is_none() {
        Color { a: 0.45, ..p.muted }
    } else if filled {
        Color {
            a: 1.,
            ..crate::widget_style::on_accent(p)
        }
    } else {
        p.muted
    };
    let mut parts: Vec<Element<'a, M>> = Vec::new();
    if let Some(name) = name {
        parts.push(icon::from_name(name).size(14).icon().into());
    }
    let title = title.into();
    parts.push(label(title.clone(), crate::widget_style::SECONDARY, color));
    press(
        tinted(
            row::with_children(parts)
                .spacing(6)
                .align_y(Alignment::Center)
                .into(),
            if filled {
                p.accent
            } else {
                Color {
                    a: if strong { 0.16 } else { 0.07 },
                    ..p.accent
                }
            },
            color,
            [5, 11],
            14.,
        ),
        message,
        title,
    )
}

fn track<'a, M: 'a>(progress: f32, color: Color, p: Palette) -> Element<'a, M> {
    let done = (progress.clamp(0., 1.) * 1000.).round() as u16;
    let mut cells = Vec::new();
    for (portion, color) in [
        (done, color),
        (1000 - done, mix(p.accent, p.background, 0.12)),
    ] {
        if portion > 0 {
            cells.push(
                container(text::body(""))
                    .height(Length::Fixed(4.))
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
                    .into(),
            );
        }
    }
    row::with_children(cells).height(Length::Fixed(4.)).into()
}

/// The player's artwork row with nothing in it: a placeholder square and a
/// muted title and reason. The card keeps its size, so nothing moves when a
/// player appears.
fn idle_player<'a, M: Clone + 'a>(
    title: &'static str,
    detail: &'static str,
    config: &Config,
    p: Palette,
) -> Element<'a, M> {
    let mut top: Vec<Element<'a, M>> = Vec::new();
    if config.music.show_artwork {
        top.push(
            container(tinted(
                icon::from_name("audio-x-generic-symbolic")
                    .size(20)
                    .icon()
                    .into(),
                Color {
                    a: 0.12,
                    ..p.accent
                },
                p.muted,
                [12, 12],
                9.,
            ))
            .width(44.)
            .height(44.)
            .into(),
        );
    }
    top.push(
        column::with_children(vec![line(title, 15., p.muted), line(detail, 11., p.muted)])
            .spacing(3)
            .width(Length::Fill)
            .into(),
    );
    // No controls or track until there is something to control: the row
    // sits in the middle of the card, which keeps its size.
    row::with_children(top)
        .spacing(10)
        .height(44.)
        .align_y(Alignment::Center)
        .into()
}

/// Everything the Music card shows.
pub struct MusicView<'a> {
    /// `None` until the first answer from the session bus.
    pub feed: Option<&'a music::Feed>,
    pub player: Option<&'a music::Player>,
    pub art: Option<&'a Handle>,
    pub now: Instant,
    pub manually_selected: bool,
    pub problem: bool,
    /// The track shown last, for an idle card to name and resume.
    pub last: Option<&'a music::LastTrack>,
}

pub fn music<'a, M: Clone + 'static>(
    view: MusicView<'a>,
    config: &Config,
    on: Option<fn(Action) -> M>,
    dimensions: Option<[f32; 2]>,
) -> Element<'a, M> {
    music_visualized(
        view,
        config,
        on,
        dimensions,
        [0.; crate::tools::visualizer::BINS],
    )
}

/// Decorations follow the actual cover and leave clear faces and contrast assistance alone.
fn cover_light(
    config: &Config,
    palette: Palette,
    cover: Option<&Handle>,
    artwork_id: cosmic::widget::Id,
    spread: f32,
    radius: f32,
) -> Option<crate::materials::AlbumLight> {
    (config.music.album_light
        && config.music_artwork()
        && !config.appearance.contrast_assist
        && palette.background.a > 0.
        && crate::widget_style::luminance(palette.background) < 0.27)
        .then(|| crate::materials::AlbumLight {
            cover: cover.cloned(),
            artwork_id,
            spread,
            radius,
            animate: config.material_motion,
        })
}

pub fn music_visualized<'a, M: Clone + 'static>(
    view: MusicView<'a>,
    config: &Config,
    on: Option<fn(Action) -> M>,
    dimensions: Option<[f32; 2]>,
    bins: [f32; crate::tools::visualizer::BINS],
) -> Element<'a, M> {
    if config.music.visualizer.circle != crate::tools::visualizer::Circle::Off {
        return self::recipe::circular(view, config, on, dimensions, bins);
    }
    if let Some(recipe) = &config.recipes.music {
        return self::recipe::music(view, config, recipe, on, dimensions);
    }
    let _style = crate::widget_style::scope(config);
    let p = config.palette();
    let [width, height] = dimensions.unwrap_or_else(|| size(Widget::Music, config));
    let sleeve = (height - f32::from(PADDING) * 2.).clamp(84., 164.);
    let act = |action: Action, allowed: bool| on.filter(|_| allowed).map(|on| on(action));
    let live: &[music::Player] = match view.feed {
        Some(music::Feed::Players(players)) => players,
        _ => &[],
    };
    let players = live.len();
    // Nothing loaded now, but something played earlier: name it and offer
    // to resume it, unless the bus itself is the problem.
    let last = view.last.filter(|_| match (view.feed, view.player) {
        (Some(music::Feed::Unavailable(_)), _) => false,
        (_, Some(player)) => !player.has_track(),
        _ => true,
    });
    let live_track = matches!((view.feed, view.player), (Some(music::Feed::Players(_)), Some(_)));
    let cover = view.art.filter(|_| last.is_some() || live_track);
    // Keep the wide layout stable while a player's new cover is loading.
    let wide = width >= 440. && height >= 130. && config.music.show_artwork && last.is_none()
        && (cover.is_some() || view.player.is_some_and(|p| p.art_url.is_some()));
    let content: Element<'a, M> = match (view.feed, view.player, last) {
        (_, _, Some(last)) => last_played(last, live, view.art, config, on, p),
        (None, _, _) => idle_player("Nothing playing", "Start a music app", config, p),
        (Some(music::Feed::Unavailable(_)), _, _) => idle_player(
            "Media controls unavailable",
            "The desktop session bus isn’t reachable.",
            config,
            p,
        ),
        (Some(music::Feed::Players(_)), None, _) => idle_player(
            "Nothing playing",
            "Play something in any music app or browser tab.",
            config,
            p,
        ),
        (Some(music::Feed::Players(_)), Some(player), None) => {
            let playing = player.status == music::Playback::Playing;
            let transport = |command, allowed| {
                act(
                    Action::Music(music::TransportRequest {
                        bus: player.bus.clone(),
                        command,
                    }),
                    allowed,
                )
            };
            let title = if player.has_track() {
                player.title.clone()
            } else {
                "Not playing".into()
            };
            let author = if player.artists.is_empty() {
                player.album.clone()
            } else {
                player.artists.join(", ")
            };
            let subtitle = if view.problem {
                "Control failed · try again".to_string()
            } else {
                [
                    player.site().map(str::to_string),
                    (!author.is_empty()).then_some(author),
                ]
                .into_iter()
                .flatten()
                .collect::<Vec<_>>()
                .join(" · ")
            };
            let detail = column::with_children(vec![
                press(
                    line(
                        title,
                        if wide { 21. } else { 15. },
                        if player.has_track() {
                            p.foreground
                        } else {
                            p.muted
                        },
                    ),
                    transport(music::Command::Raise, player.can_raise),
                    "Open music player",
                ),
                line(
                    if subtitle.is_empty() {
                        match player.status {
                            music::Playback::Playing => "Playing".into(),
                            music::Playback::Paused => "Paused".into(),
                            music::Playback::Stopped => "Stopped".into(),
                        }
                    } else {
                        subtitle
                    },
                    11.,
                    if view.problem { WARN } else { p.muted },
                ),
            ])
            .spacing(3)
            .width(Length::Fill);
            let mut top: Vec<Element<'a, M>> = Vec::new();
            let sleeve_view = |side: f32| -> Element<'a,M> {
                let image: Element<'a, M> = match cover {
                    Some(handle) => cosmic::widget::image(handle.clone())
                        .width(side).height(side)
                        .content_fit(cosmic::iced::ContentFit::Cover)
                        .border_radius([9.; 4]).into(),
                    None => container(tinted(
                        icon::from_name(if player.site().is_some() { "video-x-generic-symbolic" }
                            else { "multimedia-audio-player-symbolic" })
                        .size(20).icon().into(),
                        Color { a: 0.12, ..p.accent }, p.muted, [12, 12], 9.,
                    )).center_x(side).center_y(side).into(),
                };
                container(image)
                    .id(cosmic::widget::Id::new("music-artwork"))
                    .into()
            };
            if config.music.show_artwork && !wide {
                top.push(sleeve_view(44.));
            }
            top.push(detail.into());
            let source = row::with_children(vec![
                line(player.name().to_string(), 11., p.muted),
                if players > 1 {
                    icon::from_name("media-playlist-repeat-symbolic")
                        .size(12)
                        .icon()
                        .into()
                } else {
                    container(cosmic::iced::widget::space()).width(0.).into()
                },
            ])
            .spacing(5)
            .align_y(Alignment::Center);
            let source = press(
                container(source).padding([4, 0]).into(),
                act(Action::MusicCycle, players > 1),
                "Switch music player",
            );
            let controls = row::with_children(vec![
                container(control(
                    "media-skip-backward-symbolic",
                    16,
                    transport(music::Command::Previous, player.can_previous),
                    p,
                    false,
                ))
                .id(cosmic::widget::Id::new("music-previous"))
                .into(),
                container(control(
                    if playing {
                        "media-playback-pause-symbolic"
                    } else {
                        "media-playback-start-symbolic"
                    },
                    16,
                    transport(
                        if playing {
                            music::Command::Pause
                        } else {
                            music::Command::Play
                        },
                        if playing {
                            player.can_pause
                        } else {
                            player.can_play
                        },
                    ),
                    p,
                    true,
                ))
                .id(cosmic::widget::Id::new("music-play-pause"))
                .into(),
                container(control(
                    "media-skip-forward-symbolic",
                    16,
                    transport(music::Command::Next, player.can_next),
                    p,
                    false,
                ))
                .id(cosmic::widget::Id::new("music-next"))
                .into(),
                container(source)
                    .width(Length::Fill)
                    .align_x(Alignment::End)
                    .into(),
                if view.manually_selected {
                    container(chip(None, "Auto", act(Action::MusicAuto, true), p, true))
                        .id(cosmic::widget::Id::new("music-auto"))
                        .into()
                } else {
                    // Following whichever player is active is the normal
                    // case and needs no tag; only a pinned player offers
                    // the way back.
                    cosmic::iced::widget::space().width(0).into()
                },
            ])
            .spacing(5)
            .align_y(Alignment::Center);
            let mut parts = vec![
                row::with_children(top)
                    .spacing(10)
                    .height(44.)
                    .align_y(Alignment::Center)
                    .into(),
                controls.into(),
            ];
            if config.music.show_progress {
                let at = player.position_at(view.now);
                let length = player.length_us.filter(|length| *length > 0);
                let timing = at.zip(length);
                let fraction = timing.map_or(0., |(at, length)| (at as f64 / length as f64) as f32);
                let bar = match (timing, on, player.seek_request(0.)) {
                    (Some((_, length)), Some(on), Some(request)) => {
                        super::seek_bar::view(fraction, p.accent, move |fraction| {
                            let mut request = request.clone();
                            request.position_us = ((fraction as f64 * length as f64) as i64)
                                .min(length.saturating_sub(1));
                            on(Action::MusicSeek(request))
                        })
                    }
                    _ => container(track(
                        fraction,
                        if timing.is_some() { p.accent } else { p.muted },
                        p,
                    ))
                    .height(14.)
                    .align_y(Alignment::Center)
                    .into(),
                };
                let time = match (at, length) {
                    (Some(at), Some(length)) => {
                        format!("{} / {}", music::duration(at), music::duration(length))
                    }
                    (Some(at), None) => format!("{} / --:--", music::duration(at)),
                    (None, Some(length)) => format!("--:-- / {}", music::duration(length)),
                    (None, None) => "Time unavailable".into(),
                };
                parts.push(
                    row::with_children(vec![
                        container(bar)
                            .id(cosmic::widget::Id::new("music-progress"))
                            .width(Length::Fill)
                            .into(),
                        label(time, 11., p.muted),
                    ])
                    .spacing(10)
                    .align_y(Alignment::Center)
                    .into(),
                );
            }
            let detail = column::with_children(parts).spacing(6).width(Length::Fill);
            if wide {
                row::with_children(vec![
                    sleeve_view(sleeve),
                    detail.into(),
                ])
                .spacing(16)
                .align_y(Alignment::Center)
                .into()
            } else {
                detail.into()
            }
        }
    };
    // Idle or not, the card fills its slot, so arranging places what the
    // desktop shows and nothing moves when playback starts.
    frame_with_light(
        container(content)
            .height(Length::Fill)
            .align_y(Alignment::Center)
            .into(),
        config,
        height,
        cover_light(
            config,
            p,
            cover,
            cosmic::widget::Id::new("music-artwork"),
            if wide {
                sleeve * 1.12
            } else {
                82.
            },
            crate::widget_style::radius(config.appearance.corner_radius),
        ),
    )
}

/// The idle card after something played: the track, its player, and a
/// button that resumes it while that player (or another of its app's) is
/// still there to take Play.
fn last_played<'a, M: Clone + 'static>(
    last: &'a music::LastTrack,
    live: &'a [music::Player],
    art: Option<&'a Handle>,
    config: &Config,
    on: Option<fn(Action) -> M>,
    p: Palette,
) -> Element<'a, M> {
    let resumer = last.resumer(live);
    let mut top: Vec<Element<'a, M>> = Vec::new();
    if config.music.show_artwork {
        let image: Element<'a, M> = match art {
            Some(handle) => cosmic::widget::image(handle.clone())
                .width(44.)
                .height(44.)
                .content_fit(cosmic::iced::ContentFit::Cover)
                .border_radius([9.; 4])
                .opacity(0.7)
                .into(),
            None => container(tinted(
                icon::from_name("multimedia-audio-player-symbolic")
                    .size(20)
                    .icon()
                    .into(),
                Color {
                    a: 0.12,
                    ..p.accent
                },
                p.muted,
                [12, 12],
                9.,
            ))
            .width(44.)
            .height(44.)
            .into(),
        };
        top.push(container(image).id(cosmic::widget::Id::new("music-artwork")).into());
    }
    let resume = on.zip(resumer).map(|(on, player)| {
        on(Action::Music(music::TransportRequest {
            bus: player.bus.clone(),
            command: music::Command::Play,
        }))
    });
    // Artist, then where it played: "Aura · Last played in Music".
    let source = if resumer.is_some() {
        format!("Last played in {}", last.player)
    } else {
        format!("{} isn’t open", last.player)
    };
    let subtitle = if last.author.is_empty() {
        source
    } else {
        format!("{} · {source}", last.author)
    };
    top.push(
        column::with_children(vec![
            line(last.title.clone(), 15., p.foreground),
            line(subtitle, 11., p.muted),
        ])
        .spacing(3)
        .width(Length::Fill)
        .into(),
    );
    // One row like the playing card, with Resume at its end; the card
    // centres the row in its height.
    top.push(
        container(control("media-playback-start-symbolic", 16, resume, p, true))
            .id(cosmic::widget::Id::new("music-resume"))
            .into(),
    );
    row::with_children(top)
        .spacing(10)
        .height(44.)
        .align_y(Alignment::Center)
        .into()
}

fn note_lines(value: &str, size: f32, width: f32) -> usize {
    use advanced_text::Paragraph as _;
    type Paragraph = <cosmic::Renderer as advanced_text::Renderer>::Paragraph;
    let text = Paragraph::with_text(advanced_text::Text {
        content: value,
        bounds: cosmic::iced::Size::new(width, f32::INFINITY),
        size: size.into(),
        line_height: advanced_text::LineHeight::Relative(1.25),
        font: font(),
        align_x: advanced_text::Alignment::Left,
        align_y: cosmic::iced::alignment::Vertical::Top,
        shaping: advanced_text::Shaping::Advanced,
        wrapping: advanced_text::Wrapping::WordOrGlyph,
        ellipsize: advanced_text::Ellipsize::None,
    });
    (text.min_bounds().height / (size * 1.25)).round().max(1.) as usize
}

/// Everything the Notes card shows.
pub struct NotesView<'a> {
    pub note: Result<&'a notes::Note, &'a str>,
    /// Editing on the desktop: the text being written.
    pub editing: Option<&'a text_editor::Content>,
    /// The draft is paused (the note was hidden or the desktop arranged):
    /// it holds no keyboard until Resume.
    pub paused: bool,
    /// A save problem or conflict to explain.
    pub problem: Option<&'a str>,
    /// Cancel just discarded a draft: offer Undo.
    pub discarded: bool,
}

pub fn notes<'a, M: Clone + 'a>(
    view: NotesView<'a>,
    config: &Config,
    on: Option<fn(Action) -> M>,
    dimensions: Option<[f32; 2]>,
) -> Element<'a, M> {
    let _style = crate::widget_style::scope(config);
    let p = config.palette();
    let [width, height] = dimensions.unwrap_or_else(|| size(Widget::Notes, config));
    let act = |action: Action| on.map(|on| on(action));
    if let Some(content) = view.editing {
        let editor = text_editor(content)
            .id(cosmic::widget::Id::new(NOTE_EDITOR))
            .placeholder("Write anything. Start a line with [ ] for a checklist.")
            .height(Length::Fill)
            .padding(0)
            .size(13.)
            .font(font())
            .class(cosmic::theme::iced::TextEditor::Custom(Box::new(
                move |_, _| text_editor::Style {
                    background: Background::Color(Color::TRANSPARENT),
                    border: Border::default(),
                    placeholder: p.muted,
                    value: p.foreground,
                    selection: Color {
                        a: 0.35,
                        ..p.accent
                    },
                },
            )));
        let editor = match on {
            Some(on) => editor
                .on_action(move |action| on(Action::NoteAction(action)))
                .key_binding(move |press| {
                    use cosmic::iced::keyboard::{Key, key::Named};
                    let save = matches!(press.key, Key::Named(Named::Escape))
                        || (press.modifiers.control()
                            && (matches!(press.key, Key::Named(Named::Enter))
                                || matches!(&press.key, Key::Character(c) if c.as_str() == "s")));
                    if save {
                        Some(text_editor::Binding::Custom(on(Action::NoteSave)))
                    } else {
                        text_editor::Binding::from_key_press(press)
                    }
                }),
            None => editor,
        };
        let header = row::with_children(if view.paused {
            vec![
                card_title("Paused", p),
                fill(),
                chip(None, "Cancel", act(Action::NoteCancel), p, false),
                chip(
                    Some("edit-symbolic"),
                    "Resume",
                    act(Action::NoteEdit),
                    p,
                    true,
                ),
            ]
        } else {
            vec![
                card_title("Editing", p),
                fill(),
                chip(None, "Cancel", act(Action::NoteCancel), p, false),
                chip(None, "Save", act(Action::NoteSave), p, true),
            ]
        })
        .spacing(6)
        .align_y(Alignment::Center);
        let footer = label(
            view.problem.unwrap_or(if view.paused {
                "Unsaved · Resume to keep writing"
            } else {
                "Esc saves and closes · Ctrl+S saves"
            }),
            crate::widget_style::SECONDARY,
            if view.problem.is_some() {
                WARN
            } else {
                p.muted
            },
        );
        return frame(
            column::with_children(vec![
                header.into(),
                container(editor).height(Length::Fill).into(),
                footer,
            ])
            .spacing(10)
            .height(Length::Fill)
            .into(),
            config,
            height,
        );
    }
    let readable = view.note.is_ok();
    let (done, total) = view
        .note
        .as_ref()
        .ok()
        .into_iter()
        .flat_map(|note| notes::lines(&note.text))
        .filter_map(|line| {
            if let notes::Line::Task { done, .. } = line {
                Some(done)
            } else {
                None
            }
        })
        .fold((0, 0), |(done, total), checked| {
            (done + usize::from(checked), total + 1)
        });
    let header = row::with_children(vec![
        card_title("Notes", p),
        if total == 0 {
            container(cosmic::iced::widget::space()).width(0.).into()
        } else {
            label(
                format!("{done} of {total} done"),
                crate::widget_style::SECONDARY,
                p.muted,
            )
        },
        fill(),
        container(control(
            "edit-copy-symbolic",
            14,
            view.note
                .as_ref()
                .ok()
                .filter(|note| !note.text.is_empty())
                .and_then(|_| act(Action::NoteCopy)),
            p,
            false,
        ))
        .id(cosmic::widget::Id::new("notes-copy"))
        .into(),
        control(
            "edit-symbolic",
            14,
            readable.then(|| act(Action::NoteEdit)).flatten(),
            p,
            false,
        ),
    ])
    .spacing(6)
    .align_y(Alignment::Center);
    let mut body: Vec<Element<'a, M>> = Vec::new();
    let mut footer = None;
    let mut overflow = false;
    match view.note {
        Err(reason) => body.push(label(reason.to_string(), 12., WARN)),
        Ok(note) if note.text.trim().is_empty() => {
            body.push(label("Nothing here yet.", 13., p.muted));
            body.push(chip(
                Some("edit-symbolic"),
                "Write a note",
                act(Action::NoteEdit),
                p,
                true,
            ));
        }
        Ok(note) => {
            let lines = notes::lines(&note.text);
            let mut space = (height - 104.).max(20.);
            for entry in lines {
                let (value, size, inset) = match entry {
                    notes::Line::Heading(title) => (title, 15., 0.),
                    notes::Line::Text(value) => (value, 13., 0.),
                    notes::Line::Bullet(value) => (value, 13., 18.),
                    notes::Line::Task { text, .. } => (text, 13., 26.),
                    notes::Line::Blank => ("", 13., 0.),
                };
                let available = (space / (size * 1.25)).floor() as usize;
                if available == 0 {
                    overflow = true;
                    break;
                }
                let color = if matches!(entry, notes::Line::Task { done: true, .. }) {
                    p.muted
                } else {
                    p.foreground
                };
                let text_width = (width - 2. * f32::from(PADDING) - inset).max(1.);
                let needed = note_lines(value, size, text_width);
                let count = available.min(needed).max(1);
                let paragraph: Element<'a, M> = text::body(value.to_owned())
                    .size(size)
                    .font(font())
                    .line_height(1.25)
                    .wrapping(advanced_text::Wrapping::WordOrGlyph)
                    .ellipsize(advanced_text::Ellipsize::End(
                        advanced_text::EllipsizeHeightLimit::Lines(count),
                    ))
                    .class(cosmic::theme::Text::Color(crate::widget_style::semantic(
                        color,
                    )))
                    .width(Length::Fill)
                    .into();
                body.push(match entry {
                    notes::Line::Blank => container(cosmic::iced::widget::space()).height(4).into(),
                    notes::Line::Heading(_) | notes::Line::Text(_) => paragraph,
                    notes::Line::Bullet(_) => {
                        row::with_children(vec![label("•", 13., p.accent), paragraph])
                            .spacing(8)
                            .into()
                    }
                    notes::Line::Task { line, done, text } => press(
                        row::with_children(vec![
                            container(checkbox(done, p)).padding([2, 0, 0, 0]).into(),
                            paragraph,
                        ])
                        .spacing(9)
                        .into(),
                        act(Action::NoteToggle(line)),
                        format!("{} {text}", if done { "Uncheck" } else { "Check" }),
                    ),
                });
                space -= if matches!(entry, notes::Line::Blank) {
                    10.
                } else {
                    count as f32 * size * 1.25 + 6.
                };
                if count < needed {
                    overflow = true;
                    break;
                }
            }
            footer = note
                .updated_at
                .as_deref()
                .and_then(|at| chrono::DateTime::parse_from_rfc3339(at).ok())
                .map(|at| {
                    let at = at.with_timezone(&chrono::Local);
                    if at.date_naive() == chrono::Local::now().date_naive() {
                        format!(
                            "Saved {}",
                            crate::config::time_of_day(&at, config.clock.format_24h)
                        )
                    } else {
                        format!("Saved {}", at.format("%b %-d"))
                    }
                });
        }
    }
    let mut parts = vec![
        header.into(),
        column::with_children(body)
            .spacing(6)
            .height(Length::Fill)
            .width(Length::Fill)
            .into(),
    ];
    if let Some(problem) = view.problem {
        parts.push(label(
            problem.to_string(),
            crate::widget_style::SECONDARY,
            WARN,
        ));
    } else if view.discarded {
        parts.push(
            row::with_children(vec![
                label("Changes discarded", crate::widget_style::SECONDARY, p.muted),
                fill(),
                chip(
                    Some("edit-undo-symbolic"),
                    "Undo",
                    act(Action::NoteUndo),
                    p,
                    false,
                ),
            ])
            .spacing(6)
            .align_y(Alignment::Center)
            .into(),
        );
    } else if overflow {
        parts.push(press(
            line("Open full note →", 12., p.accent),
            act(Action::NoteEdit),
            "Open full note",
        ));
    } else if let Some(footer) = footer {
        parts.push(label(footer, crate::widget_style::SECONDARY, p.muted));
    }
    frame(
        column::with_children(parts)
            .spacing(8)
            .height(Length::Fill)
            .into(),
        config,
        height,
    )
}

fn checkbox<'a, M: 'a>(done: bool, p: Palette) -> Element<'a, M> {
    let mark: Element<'a, M> = if done {
        icon::from_name("object-select-symbolic")
            .size(10)
            .icon()
            .into()
    } else {
        cosmic::iced::widget::space().into()
    };
    container(mark)
        .width(14)
        .height(14)
        .align_x(Alignment::Center)
        .align_y(Alignment::Center)
        .class(cosmic::theme::Container::custom(move |_| {
            cosmic::iced::widget::container::Style {
                icon_color: Some(crate::widget_style::on_accent(p)),
                background: done.then_some(Background::Color(p.accent)),
                border: Border {
                    color: if done { p.accent } else { p.muted },
                    width: 1.5,
                    radius: 4.into(),
                },
                ..Default::default()
            }
        }))
        .into()
}

/// The Focus card: the countdown, its progress and the next step.
pub fn focus<'a, M: Clone + 'static>(
    session: &focus::Session,
    config: &Config,
    now_ms: i64,
    today: &str,
    on: Option<fn(Action) -> M>,
    dimensions: Option<[f32; 2]>,
) -> Element<'a, M> {
    let _style = crate::widget_style::scope(config);
    let p = config.palette();
    if let Some(recipe) = &config.recipes.focus {
        return self::recipe::focus(
            session, config, recipe, now_ms, today, on, dimensions, false,
        );
    }
    let prefs = &config.focus;
    let [_, height] = dimensions.unwrap_or_else(|| size(Widget::Focus, config));
    let act = |action: Action| on.map(|on| on(action));
    let rounds = session.rounds_today(today);
    let every = u32::from(prefs.long_break_every);
    let phase_color = if session.phase == focus::Phase::Focus {
        p.accent
    } else {
        mix(p.accent, p.muted, 0.55)
    };
    let mut heading = session.phase.title().to_string();
    if matches!(session.status, focus::Status::Paused { .. }) {
        heading.push_str(" · Paused");
    }
    let mut header: Vec<Element<'a, M>> =
        vec![container(card_title(heading, p)).width(Length::Fill).into()];
    if every > 0 {
        header.push(label(
            round_text(session, rounds, every),
            crate::widget_style::SECONDARY,
            p.muted,
        ));
    } else if rounds > 0 {
        header.push(label(
            format!("{rounds} today"),
            crate::widget_style::SECONDARY,
            p.muted,
        ));
    }
    let remaining = session.remaining(now_ms, prefs);
    let next = session.next_phase(prefs, today);
    let (time, detail): (Element<'a, M>, Option<String>) = match session.status {
        focus::Status::Done => (
            light(
                if session.phase == focus::Phase::Focus {
                    "Round done"
                } else {
                    "Break over"
                },
                30.,
                p.foreground,
            ),
            Some(format!(
                "Next: {} min {}",
                next.minutes(prefs),
                next.title().to_lowercase()
            )),
        ),
        focus::Status::Paused { .. } => (light(focus::clock(remaining), 44., p.muted), None),
        _ => (light(focus::clock(remaining), 44., p.foreground), None),
    };
    let mut middle = vec![time];
    if let Some(detail) = detail {
        middle.push(label(detail, crate::widget_style::SECONDARY, p.muted));
    }
    let primary = match session.status {
        focus::Status::Ready if session.phase == focus::Phase::Focus => {
            ("media-playback-start-symbolic", "Start".to_string())
        }
        focus::Status::Ready => ("media-playback-start-symbolic", "Start break".to_string()),
        focus::Status::Running { .. } => ("media-playback-pause-symbolic", "Pause".to_string()),
        focus::Status::Paused { .. } => ("media-playback-start-symbolic", "Resume".to_string()),
        focus::Status::Done if next == focus::Phase::Focus => {
            ("media-playback-start-symbolic", "Start focus".to_string())
        }
        focus::Status::Done => ("media-playback-start-symbolic", "Start break".to_string()),
    };
    let mut controls: Vec<Element<'a, M>> = vec![chip(
        Some(primary.0),
        primary.1,
        act(Action::FocusToggle),
        p,
        true,
    )];
    let idle = matches!(session.status, focus::Status::Ready);
    if idle && session.phase == focus::Phase::Focus {
        controls.push(fill());
        let mut choices = vec![15, 25, 50];
        if !choices.contains(&prefs.focus_minutes) {
            choices.push(prefs.focus_minutes);
            choices.sort_unstable();
        }
        for minutes in choices {
            let selected = minutes == prefs.focus_minutes;
            controls.push(press(
                tinted(
                    label(
                        minutes.to_string(),
                        11.,
                        if selected { p.accent } else { p.muted },
                    ),
                    Color {
                        a: if selected { 0.16 } else { 0. },
                        ..p.accent
                    },
                    p.muted,
                    [4, 8],
                    10.,
                ),
                (!selected)
                    .then(|| act(Action::FocusMinutes(minutes)))
                    .flatten(),
                format!("{minutes} minutes"),
            ));
        }
    } else {
        controls.push(fill());
        // Skip and Stop change the session, so both say so in words. At the
        // 280px minimum the row still fits the primary chip with its icon.
        controls.push(chip(None, "Skip", act(Action::FocusSkip), p, false));
        controls.push(chip(
            None,
            "Stop",
            (!(idle && session.phase == focus::Phase::Focus))
                .then(|| act(Action::FocusReset))
                .flatten(),
            p,
            false,
        ));
    }
    frame(
        column::with_children(vec![
            row::with_children(header)
                .spacing(8)
                .align_y(Alignment::Center)
                .into(),
            container(column::with_children(middle).spacing(2))
                .height(Length::Fill)
                .align_y(Alignment::Center)
                .into(),
            track(session.progress(now_ms, prefs), phase_color, p),
            row::with_children(controls)
                .spacing(4)
                .align_y(Alignment::Center)
                .into(),
        ])
        // Tighter gaps keep the countdown clear of the title-sized header.
        .spacing(8)
        .height(Length::Fill)
        .into(),
        config,
        height,
    )
}

/// "Round N of M": the round in progress, or the one just finished during
/// its break, within the cycle that ends in a long break.
fn round_text(session: &focus::Session, rounds: u32, every: u32) -> String {
    let full = rounds > 0
        && rounds % every == 0
        && (session.phase == focus::Phase::LongBreak
            || matches!(session.status, focus::Status::Done)
                && session.phase == focus::Phase::Focus);
    let finished = if full { every } else { rounds % every };
    let current =
        if session.phase == focus::Phase::Focus && !matches!(session.status, focus::Status::Done) {
            finished + 1
        } else {
            finished.max(1)
        };
    format!("Round {} of {every}", current.min(every))
}

fn dot<'a, M: 'a>(on: bool, p: Palette) -> Element<'a, M> {
    container(cosmic::iced::widget::space())
        .width(6)
        .height(6)
        .class(cosmic::theme::Container::custom(move |_| {
            cosmic::iced::widget::container::Style {
                background: Some(Background::Color(if on {
                    p.accent
                } else {
                    mix(p.accent, p.background, 0.22)
                })),
                border: Border {
                    radius: 3.into(),
                    ..Default::default()
                },
                ..Default::default()
            }
        }))
        .into()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::native_test::Native;
    use cosmic::iced::{Event, Point, Size, mouse};
    use std::collections::HashMap;
    #[test]
    fn album_light_is_visible_beyond_the_cover_on_both_renderers() {
        use crate::native_test::Native;
        use cosmic::iced::{Event, Point, Size, window};

        let now = Instant::now();
        let player = recipe::sample_player(now);
        let feed = music::Feed::Players(vec![player.clone()]);
        let art = Handle::from_rgba(256, 256, [53, 40, 82, 255].repeat(256 * 256));
        for backend in ["tiny-skia", "wgpu"] {
            for (size, progress, template) in [
                ([520., 164.], true, None),
                ([360., 140.], true, None),
                ([360., 140.], false, None),
                ([520., 220.], true, Some(1)),
                ([320., 400.], true, Some(0)),
            ] {
                let mut config = Config::default();
                config.appearance.family = Some(crate::desktop_theme::Family::DarkGlass);
                config.material_motion = false;
                config.music.show_progress = progress;
                config.recipes.music = template.map(|i| {
                    crate::recipes::Recipe::template(crate::recipes::Kind::Music, i)
                });
                let mut ui = Native::with_backend(Size::new(size[0], size[1]), backend);
                let mut capture = |cfg: &Config| {
                    let card = || music::<Action>(
                        MusicView {
                            feed: Some(&feed),
                            player: Some(&player),
                            art: Some(&art),
                            now,
                            manually_selected: false,
                            problem: false,
                            last: None,
                        },
                        cfg,
                        None,
                        Some(size),
                    );
                    let bounds = ui.bounds(card(), "music-artwork");
                    let _ = ui.event(
                        card(),
                        Event::Window(window::Event::RedrawRequested(now)),
                        Point::ORIGIN,
                    );
                    let pixels = ui.render(card(), &cosmic::Theme::dark());
                    if cfg.music.album_light && size == [520., 164.] {
                        ui.capture(
                            card(),
                            &cosmic::Theme::dark(),
                            &format!("album-lit-music-{backend}"),
                        );
                    }
                    (bounds, pixels)
                };
                config.music.album_light = true;
                let (cover, lit) = capture(&config);
                config.music.album_light = false;
                let (_, plain) = capture(&config);
                let delta = |x: f32, y: f32| {
                    let i = (size[0] as usize * y as usize + x as usize) * 4;
                    i16::from(lit[i + 2]) - i16::from(plain[i + 2])
                };
                let left = cover.x - 4.;
                let right = cover.x + cover.width + 3.;
                let cy = cover.center_y();
                let context = format!("{backend} {size:?} progress={progress} template={template:?}");
                for y in [cy - 12., cy + 11.] {
                    let (l, r) = (delta(left, y), delta(right, y));
                    assert!(l > 12 && r > 12 && (l - r).abs() <= 8,
                        "{context}: expected a balanced glow beside the cover, got {l} and {r}");
                }
                assert!((delta(left, cy - 12.) - delta(left, cy + 11.)).abs() <= 8,
                    "{context}: the vertical glow should follow the artwork row");
                if size == [520., 164.] {
                    assert!(delta(230., 26.) <= 40, "{context}: keep track details readable");
                    assert!(delta(510., 78.).abs() <= 1, "{context}: keep the far edge neutral");
                }
            }
        }
    }

    #[test]
    fn copying_a_note_does_not_enter_editing_or_toggle_its_checklist() {
        let config = Config::default();
        let note = notes::Note {
            text: "# Today\n[x] Review the latest build\n[ ] Prepare the next demo\n[ ] Write a short handoff".into(),
            updated_at: None,
        };
        let [width, height] = size(Widget::Notes, &config);
        let mut native = Native::new(Size::new(width, height));
        let view = |enabled: bool| {
            notes(
                NotesView {
                    note: Ok(&note),
                    editing: None,
                    paused: false,
                    problem: None,
                    discarded: false,
                },
                &config,
                enabled.then_some(std::convert::identity::<Action> as fn(Action) -> Action),
                None,
            )
        };
        let copy = native.bounds(view(true), "notes-copy").center();
        let click = Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left));
        let _ = native.event(view(true), click.clone(), copy);
        let (messages, _) = native.event(view(true), Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)), copy);
        assert_eq!(messages.len(), 1);
        assert!(matches!(messages[0], Action::NoteCopy));
        let (messages, _) = native.event(view(false), click, copy);
        assert!(messages.is_empty());
        native.capture(
            view(true),
            &cosmic::Theme::dark(),
            "notes-checklist-example",
        );
    }

    #[test]
    fn music_controls_fit_and_seek_the_visible_browser_track() {
        let mut player = music::parse_player(
            "org.mpris.MediaPlayer2.firefox.instance_1_123",
            &HashMap::new(),
            &HashMap::new(),
            Instant::now(),
        );
        player.identity = "Zen".into();
        player.title = "Designing a quieter workspace — a very long video title".into();
        player.artists = vec!["Studio session".into()];
        player.media_url = Some("https://www.youtube.com/watch?v=fixture".into());
        player.track_id = Some("/browser/video_one".into());
        player.length_us = Some(300_000_000);
        player.position_us = Some(60_000_000);
        player.status = music::Playback::Paused;
        player.can_seek = true;
        player.can_play = true;
        player.can_pause = true;
        let feed = music::Feed::Players(vec![player.clone()]);
        let mut config = Config::default();
        for width in [300., 360., 480.] {
            for progress in [true, false] {
                for artwork in [true, false] {
                    config.music.show_progress = progress;
                    config.music.show_artwork = artwork;
                    let height = size(Widget::Music, &config)[1];
                    let mut native = Native::new(Size::new(width, height));
                    let view = |manual: bool, enabled: bool| {
                        music(
                            MusicView {
                                feed: Some(&feed),
                                player: Some(&player),
                                art: None,
                                now: Instant::now(),
                                manually_selected: manual,
                                problem: false,
                                last: None,
                            },
                            &config,
                            enabled.then_some(
                                std::convert::identity::<Action> as fn(Action) -> Action,
                            ),
                            Some([width, height]),
                        )
                    };
                    native.capture(
                        view(true, true),
                        &cosmic::Theme::dark(),
                        &format!("music-{width}-{progress}-{artwork}"),
                    );
                    let auto = native.bounds(view(true, true), "music-auto");
                    assert!(
                        auto.x >= 0.
                            && auto.x + auto.width <= width
                            && auto.y + auto.height <= height
                    );
                    let _ = native.event(view(true, true), Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)), auto.center());
                    let (messages, _) = native.event(
                        view(true, true),
                        Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)),
                        auto.center(),
                    );
                    assert!(messages.iter().any(|m| matches!(m, Action::MusicAuto)));
                    if progress {
                        let bar = native.bounds(view(false, true), "music-progress");
                        assert!(bar.width > 60. && bar.y + bar.height <= height - 14.);
                        let point = Point::new(bar.x + bar.width * 0.75, bar.center_y());
                        let (messages, _) = native.event(
                            view(false, true),
                            Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)),
                            point,
                        );
                        let request = messages
                            .iter()
                            .find_map(|m| {
                                if let Action::MusicSeek(r) = m {
                                    Some(r)
                                } else {
                                    None
                                }
                            })
                            .unwrap();
                        assert_eq!(request.bus, player.bus);
                        assert_eq!(request.track_id, "/browser/video_one");
                        assert!((request.position_us - 225_000_000).abs() < 100);
                        let (messages, _) = native.event(
                            view(false, false),
                            Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)),
                            point,
                        );
                        assert!(messages.is_empty(), "Read-only previews must not seek");
                    }
                }
            }
        }
        // Keep a representative screenshot without a manual pin.
        config.music.show_progress = true;
        config.music.show_artwork = true;
        Native::new(Size::new(360., 140.)).capture(
            music(
                MusicView {
                    feed: Some(&feed),
                    player: Some(&player),
                    art: None,
                    now: Instant::now(),
                    manually_selected: false,
                    problem: false,
                    last: None,
                },
                &config,
                Some(std::convert::identity::<Action>),
                None,
            ),
            &cosmic::Theme::dark(),
            "music-browser-example",
        );
    }

    #[test]
    fn music_keeps_an_honest_unseekable_timeline_when_browser_timing_is_partial() {
        let mut player = music::parse_player(
            "org.mpris.MediaPlayer2.firefox.instance_1_123",
            &HashMap::new(),
            &HashMap::new(),
            Instant::now(),
        );
        player.identity = "Mozilla zen".into();
        player.title = "A browser video with delayed timing".into();
        player.track_id = Some("/org/mpris/MediaPlayer2/firefox".into());
        player.media_url = Some("https://www.youtube.com/watch?v=fixture".into());
        player.status = music::Playback::Playing;
        player.can_play = true;
        player.can_pause = true;
        player.can_seek = true;
        let config = Config::default();
        for width in [300., 360.] {
            for (name, at, length) in [
                ("elapsed", Some(60_000_000), None),
                ("duration", None, Some(224_000_000)),
                ("unknown", None, None),
            ] {
                player.position_us = at;
                player.length_us = length;
                let feed = music::Feed::Players(vec![player.clone()]);
                let view = || {
                    music(
                        MusicView {
                            feed: Some(&feed),
                            player: Some(&player),
                            art: None,
                            now: player.sampled,
                            manually_selected: false,
                            problem: false,
                            last: None,
                        },
                        &config,
                        Some(std::convert::identity::<Action>),
                        Some([width, 140.]),
                    )
                };
                let mut native = Native::new(Size::new(width, 140.));
                native.capture(
                    view(),
                    &cosmic::Theme::dark(),
                    &format!("music-partial-{name}-{width}"),
                );
                let rail = native.bounds(view(), "music-progress");
                assert!(rail.width > 60. && rail.y + rail.height <= 126.);
                let (messages, _) = native.event(
                    view(),
                    Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)),
                    rail.center(),
                );
                assert!(
                    messages.is_empty(),
                    "Incomplete timing must never seek or invent progress"
                );
            }
        }
    }
}
