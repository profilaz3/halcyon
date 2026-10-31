//! The tool cards, drawn the same way on the desktop and as the live
//! previews in Settings. Each takes the palette and card look from the
//! configuration, like every other widget, and an optional action mapper:
//! without one the card is drawn without controls that respond.
pub(crate) mod recipe;
use super::{focus, music, notes};
use crate::config::{Config, Palette};
use crate::drawer::Widget;
use cosmic::Element;
use cosmic::iced::advanced::text as advanced_text;
use cosmic::iced::{Alignment, Background, Border, Color, Font, Length, mouse};
use cosmic::widget::image::Handle;
use cosmic::widget::{column, container, icon, mouse_area, row, scrollable, text, text_editor};
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
    NoteAction(text_editor::Action),
    /// Tick or untick the checklist item on this line.
    NoteToggle(usize),
    FocusToggle,
    FocusReset,
    FocusSkip,
    FocusMinutes(u16),
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
    .height(Length::Fixed(height))
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

fn press<'a, M: Clone + 'a>(content: Element<'a, M>, message: Option<M>) -> Element<'a, M> {
    match message {
        Some(message) => mouse_area(content)
            .on_press(message)
            .interaction(mouse::Interaction::Pointer)
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
        p.foreground
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
    press(
        tinted(
            icon::from_name(name).size(size).icon().into(),
            background,
            color,
            [pad, pad],
            40.,
        ),
        message,
    )
}

/// A text control with an optional leading icon.
fn chip<'a, M: Clone + 'a>(
    name: Option<&'static str>,
    title: impl Into<String>,
    message: Option<M>,
    p: Palette,
    strong: bool,
) -> Element<'a, M> {
    let color = if message.is_none() {
        Color { a: 0.45, ..p.muted }
    } else if strong {
        p.accent
    } else {
        p.muted
    };
    let mut parts: Vec<Element<'a, M>> = Vec::new();
    if let Some(name) = name {
        parts.push(icon::from_name(name).size(14).icon().into());
    }
    parts.push(label(title, crate::widget_style::SECONDARY, color));
    press(
        tinted(
            row::with_children(parts)
                .spacing(6)
                .align_y(Alignment::Center)
                .into(),
            Color {
                a: if strong { 0.16 } else { 0.07 },
                ..p.accent
            },
            color,
            [5, 11],
            14.,
        ),
        message,
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

/// A quiet centered message for empty and unavailable states.
fn empty<'a, M: Clone + 'a>(
    name: &'static str,
    title: &'static str,
    detail: impl Into<String>,
    p: Palette,
) -> Element<'a, M> {
    row::with_children(vec![
        tinted(
            icon::from_name(name).size(22).icon().into(),
            Color {
                a: 0.10,
                ..p.accent
            },
            p.muted,
            [13, 13],
            14.,
        ),
        column::with_children(vec![
            label(title, crate::widget_style::BODY, p.foreground),
            label(detail, 11., p.muted),
        ])
        .spacing(3)
        .width(Length::Fill)
        .into(),
    ])
    .spacing(14)
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
    let [_, height] = dimensions.unwrap_or_else(|| size(Widget::Music, config));
    let act = |action: Action, allowed: bool| on.filter(|_| allowed).map(|on| on(action));
    let players = match view.feed {
        Some(music::Feed::Players(players)) => players.len(),
        _ => 0,
    };
    let content: Element<'a, M> = match (view.feed, view.player) {
        (None, _) => empty(
            "multimedia-audio-player-symbolic",
            "Music",
            "Looking for players…",
            p,
        ),
        (Some(music::Feed::Unavailable(_)), _) => empty(
            "multimedia-audio-player-symbolic",
            "Media controls unavailable",
            "The desktop session bus isn’t reachable.",
            p,
        ),
        (Some(music::Feed::Players(_)), None) => empty(
            "multimedia-audio-player-symbolic",
            "Nothing playing",
            "Start YouTube, Spotify or another browser tab.",
            p,
        ),
        (Some(music::Feed::Players(_)), Some(player)) => {
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
                        15.,
                        if player.has_track() {
                            p.foreground
                        } else {
                            p.muted
                        },
                    ),
                    transport(music::Command::Raise, player.can_raise),
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
            if config.music.show_artwork {
                top.push(match view.art {
                    Some(handle) => cosmic::widget::image(handle.clone())
                        .width(44.)
                        .height(44.)
                        .content_fit(cosmic::iced::ContentFit::Cover)
                        .border_radius([9.; 4])
                        .into(),
                    None => container(tinted(
                        icon::from_name(if player.site().is_some() {
                            "video-x-generic-symbolic"
                        } else {
                            "multimedia-audio-player-symbolic"
                        })
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
                });
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
                    container(label("Auto", 10., p.muted))
                        .padding([4, 2])
                        .into()
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
                        label(time, 10., p.muted),
                    ])
                    .spacing(10)
                    .align_y(Alignment::Center)
                    .into(),
                );
            }
            column::with_children(parts).spacing(6).into()
        }
    };
    frame(
        container(content)
            .height(Length::Fill)
            .align_y(Alignment::Center)
            .into(),
        config,
        height,
    )
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
}

pub fn notes<'a, M: Clone + 'a>(
    view: NotesView<'a>,
    config: &Config,
    on: Option<fn(Action) -> M>,
    dimensions: Option<[f32; 2]>,
) -> Element<'a, M> {
    let _style = crate::widget_style::scope(config);
    let p = config.palette();
    let [_, height] = dimensions.unwrap_or_else(|| size(Widget::Notes, config));
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
                label("PAUSED", 10., p.accent),
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
                label("EDITING", 10., p.accent),
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
                "Esc or Ctrl+S saves"
            }),
            10.,
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
        label("NOTES", 10., p.accent),
        if total == 0 {
            container(cosmic::iced::widget::space()).width(0.).into()
        } else {
            label(format!("{done}/{total}"), 10., p.muted)
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
            for line in notes::lines(&note.text) {
                body.push(match line {
                    notes::Line::Blank => container(cosmic::iced::widget::space()).height(4).into(),
                    notes::Line::Heading(title) => label(title.to_string(), 15., p.foreground),
                    notes::Line::Text(value) => label(value.to_string(), 13., p.foreground),
                    notes::Line::Bullet(value) => row::with_children(vec![
                        label("•", 13., p.accent),
                        container(label(value.to_string(), 13., p.foreground))
                            .width(Length::Fill)
                            .into(),
                    ])
                    .spacing(8)
                    .into(),
                    notes::Line::Task { line, done, text } => press(
                        row::with_children(vec![
                            container(checkbox(done, p)).padding([2, 0, 0, 0]).into(),
                            container(label(
                                text.to_string(),
                                13.,
                                if done { p.muted } else { p.foreground },
                            ))
                            .width(Length::Fill)
                            .into(),
                        ])
                        .spacing(9)
                        .into(),
                        act(Action::NoteToggle(line)),
                    ),
                });
            }
            footer = note
                .updated_at
                .as_deref()
                .and_then(|at| chrono::DateTime::parse_from_rfc3339(at).ok())
                .map(|at| {
                    let at = at.with_timezone(&chrono::Local);
                    if at.date_naive() == chrono::Local::now().date_naive() {
                        format!("Saved {}", at.format("%H:%M"))
                    } else {
                        format!("Saved {}", at.format("%-d %b"))
                    }
                });
        }
    }
    let mut parts = vec![
        header.into(),
        scrollable(
            container(column::with_children(body).spacing(6).width(Length::Fill))
                .padding([0, 8, 0, 0])
                .width(Length::Fill),
        )
        .height(Length::Fill)
        .width(Length::Fill)
        .scroller_width(3.)
        .scrollbar_width(3.)
        .scrollbar_padding(2.)
        .into(),
    ];
    if let Some(problem) = view.problem {
        parts.push(label(problem.to_string(), 10., WARN));
    } else if let Some(footer) = footer {
        parts.push(label(footer, 9., p.muted));
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
    let mut heading = session.phase.title().to_uppercase();
    if matches!(session.status, focus::Status::Paused { .. }) {
        heading.push_str(" · PAUSED");
    }
    let mut header: Vec<Element<'a, M>> = vec![
        container(line(heading, 10., phase_color))
            .width(Length::Fill)
            .into(),
    ];
    if rounds > 0 {
        header.push(label(format!("{rounds} today"), 10., p.muted));
    }
    if every > 0 {
        let full = rounds > 0
            && rounds % every == 0
            && (session.phase == focus::Phase::LongBreak
                || matches!(session.status, focus::Status::Done)
                    && session.phase == focus::Phase::Focus);
        let filled = if full { every } else { rounds % every };
        header.push(if every > 6 {
            // A long cycle remains readable on the smallest card.
            label(format!("{filled}/{every}"), 10., phase_color)
        } else {
            row::with_children(
                (0..every)
                    .map(|index| dot(index < filled, p))
                    .collect::<Vec<_>>(),
            )
            .spacing(4)
            .into()
        });
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
        middle.push(label(detail, 11., p.muted));
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
            ));
        }
    } else {
        controls.push(fill());
        controls.push(control(
            "media-skip-forward-symbolic",
            14,
            act(Action::FocusSkip),
            p,
            false,
        ));
        controls.push(control(
            "media-playback-stop-symbolic",
            14,
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
        .spacing(10)
        .height(Length::Fill)
        .into(),
        config,
        height,
    )
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
    use cosmic::iced::{Event, Point, Size};
    use std::collections::HashMap;
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
                },
                &config,
                enabled.then_some(std::convert::identity::<Action> as fn(Action) -> Action),
                None,
            )
        };
        let copy = native.bounds(view(true), "notes-copy").center();
        let click = Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left));
        let (messages, _) = native.event(view(true), click.clone(), copy);
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
                    let (messages, _) = native.event(
                        view(true, true),
                        Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)),
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
