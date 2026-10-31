//! Music and Focus presentation only. Player capabilities and timer ownership stay
//! in their existing engines; sample cards never receive an action mapper.
use super::*;
use crate::recipes::{Arrangement, Finish, Kind, Part, Recipe, TimerFace};
use cosmic::widget::{button, slider, svg};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum PreviewState {
    #[default]
    Active,
    Paused,
    NoArtwork,
    NoPlayer,
    Unavailable,
    NoSeeking,
    Ready,
    Break,
    Complete,
}
impl PreviewState {
    pub fn choices(kind: Kind) -> &'static [Self] {
        if kind == Kind::Music {
            &[
                Self::Active,
                Self::Paused,
                Self::NoArtwork,
                Self::NoPlayer,
                Self::Unavailable,
                Self::NoSeeking,
            ]
        } else {
            &[
                Self::Active,
                Self::Paused,
                Self::Ready,
                Self::Break,
                Self::Complete,
            ]
        }
    }
    pub fn title(self) -> &'static str {
        match self {
            Self::Active => "Playing / running",
            Self::Paused => "Paused",
            Self::NoArtwork => "No artwork",
            Self::NoPlayer => "No player",
            Self::Unavailable => "Unavailable",
            Self::NoSeeking => "Seeking unavailable",
            Self::Ready => "Ready",
            Self::Break => "Break",
            Self::Complete => "Complete",
        }
    }
}
fn palette(config: &Config, recipe: &Recipe) -> Palette {
    let rgb = |hex: u32| Color::from_rgb8((hex >> 16) as u8, (hex >> 8) as u8, hex as u8);
    let (accent, background, foreground, muted) = match recipe.finish {
        None => return config.palette(),
        Some(Finish::Tidal) => (0xa9dfc6, 0x163635, 0xf0f8f4, 0xa8c3bc),
        Some(Finish::Paper) => (0x765636, 0xeee6ce, 0x332c22, 0x706653),
        Some(Finish::Carbon) => (0xc6b5f8, 0x242428, 0xf4f1fa, 0xb2aebc),
    };
    Palette {
        accent: rgb(accent),
        background: rgb(background),
        foreground: rgb(foreground),
        muted: rgb(muted),
    }
}
fn surface<'a, M: 'a>(
    content: Element<'a, M>,
    config: &Config,
    recipe: &Recipe,
    p: Palette,
    size: [f32; 2],
) -> Element<'a, M> {
    if recipe.finish.is_none() {
        return frame(content, config, size[1]);
    }
    container(content)
        .padding(PADDING)
        .width(size[0])
        .height(size[1])
        .clip(true)
        .class(cosmic::theme::Container::custom(move |_| {
            cosmic::iced::widget::container::Style {
                background: Some(p.background.into()),
                text_color: Some(p.foreground),
                icon_color: Some(p.foreground),
                border: Border {
                    radius: 18.0.into(),
                    width: 1.,
                    color: Color { a: 0.2, ..p.accent },
                },
                ..Default::default()
            }
        }))
        .into()
}
fn button_style(p: Palette, primary: bool, preview: bool) -> cosmic::theme::Button {
    let style = move |focused: bool, hovered: bool, disabled: bool| {
        let disabled = disabled && !preview;
        cosmic::widget::button::Style {
            text_color: Some(if disabled {
                p.muted
            } else if primary {
                p.background
            } else {
                p.foreground
            }),
            icon_color: Some(if disabled {
                p.muted
            } else if primary {
                p.background
            } else {
                p.foreground
            }),
            background: Some(
                if primary && !disabled {
                    p.accent
                } else {
                    Color {
                        a: if hovered { 0.16 } else { 0.06 },
                        ..p.foreground
                    }
                }
                .into(),
            ),
            border_radius: 24.0.into(),
            outline_width: if focused { 2. } else { 0. },
            outline_color: p.accent,
            ..Default::default()
        }
    };
    cosmic::theme::Button::Custom {
        active: Box::new(move |focused, _| style(focused, false, false)),
        hovered: Box::new(move |focused, _| style(focused, true, false)),
        pressed: Box::new(move |focused, _| style(focused, true, false)),
        disabled: Box::new(move |_| style(false, false, true)),
    }
}
fn timeline_style(p: Palette) -> cosmic::style::iced::Slider {
    let style = move |theme: &cosmic::Theme| {
        use cosmic::iced::widget::slider::{Catalog, HandleShape, Status};
        let mut style = <cosmic::Theme as Catalog>::style(
            theme,
            &cosmic::style::iced::Slider::Standard,
            Status::Active,
        );
        style.rail.backgrounds = (
            p.accent.into(),
            Color {
                a: 0.18,
                ..p.accent
            }
            .into(),
        );
        style.handle.background = p.accent.into();
        style.handle.shape = HandleShape::Circle { radius: 6. };
        style.handle.border_color = p.background;
        style.handle.border_width = 2.;
        style
    };
    cosmic::style::iced::Slider::Custom {
        active: std::rc::Rc::new(style),
        hovered: std::rc::Rc::new(style),
        dragging: std::rc::Rc::new(style),
    }
}

fn action<'a, M: Clone + 'static>(
    name: &'static str,
    title: &'static str,
    id: &'static str,
    message: Option<M>,
    p: Palette,
    primary: bool,
    preview: bool,
) -> Element<'a, M> {
    cosmic::widget::tooltip(
        button::icon(icon::from_name(name).size(18))
            .id(cosmic::widget::Id::new(id))
            .padding(9)
            .class(button_style(p, primary, preview))
            .on_press_maybe(message),
        text::body(title),
        cosmic::widget::tooltip::Position::Top,
    )
    .into()
}
fn centered<'a, M: 'a>(child: Element<'a, M>) -> Element<'a, M> {
    container(child)
        .width(Length::Fill)
        .align_x(Alignment::Center)
        .into()
}
fn spare<'a, M: 'a>(part: Part, recipe: &Recipe, p: Palette) -> Element<'a, M> {
    match part {
        Part::Caption => line(recipe.caption.clone(), 13., p.muted),
        Part::Divider => container(cosmic::widget::space())
            .height(1)
            .width(Length::Fill)
            .class(cosmic::theme::Container::custom(move |_| {
                cosmic::iced::widget::container::Style {
                    background: Some(Color { a: 0.2, ..p.muted }.into()),
                    ..Default::default()
                }
            }))
            .into(),
        _ => cosmic::widget::space().height(8).into(),
    }
}
/// Remove optional content before it crowds the main reading or controls. Missing
/// artwork never has a slot, including in wide layouts.
fn parts(recipe: &Recipe, height: f32, artwork: bool) -> Vec<Part> {
    let mut parts: Vec<_> = recipe
        .parts
        .iter()
        .copied()
        .filter(|p| *p != Part::Artwork || artwork)
        .collect();
    let required = |parts: &[Part]| {
        parts
            .iter()
            .map(|p| match p {
                Part::Track => 64.,
                Part::Artwork => 100.,
                Part::Progress => 30.,
                Part::Playback | Part::TimerControls => 38.,
                Part::Timer => 72.,
                Part::Caption | Part::SessionDots => 18.,
                Part::Divider => 1.,
                _ => 8.,
            })
            .sum::<f32>()
            + 8. * parts.len().saturating_sub(1) as f32
    };
    for optional in [
        Part::Space,
        Part::Divider,
        Part::Caption,
        Part::Artwork,
        Part::SessionDots,
        Part::Progress,
    ] {
        if required(&parts) > height {
            parts.retain(|p| *p != optional);
        }
    }
    parts
}

pub(super) fn music<'a, M: Clone + 'static>(
    view: MusicView<'_>,
    config: &Config,
    recipe: &Recipe,
    on: Option<fn(Action) -> M>,
    dimensions: Option<[f32; 2]>,
) -> Element<'a, M> {
    let _scope = crate::widget_style::scope(config);
    let p = palette(config, recipe);
    let size = crate::widget_presentation::viewport(
        dimensions.unwrap_or_else(|| recipe.tool_size()),
        recipe.tool_size(),
    );
    let height = (size[1] - f32::from(PADDING) * 2.).max(1.);
    let width = (size[0] - f32::from(PADDING) * 2.).max(1.);
    let Some(player) = view
        .player
        .filter(|_| matches!(view.feed, Some(music::Feed::Players(_))))
    else {
        let (title, detail) = match view.feed {
            None => ("Music", "Looking for players…"),
            Some(music::Feed::Unavailable(_)) => {
                ("Music unavailable", "Reconnect to your desktop session.")
            }
            _ => ("Nothing playing", "Start music in your player or browser."),
        };
        let content = column::with_children(vec![
            label("MUSIC", 11., p.accent),
            label(title, 24., p.foreground),
            label(detail, 13., p.muted),
        ])
        .spacing(10);
        return surface(
            container(content)
                .height(Length::Fill)
                .align_y(Alignment::Center)
                .into(),
            config,
            recipe,
            p,
            size,
        );
    };
    let split = recipe.arrangement(size[0]) == Arrangement::Split && width >= 280.;
    let has_art = recipe.parts.contains(&Part::Artwork) && view.art.is_some();
    // Split artwork consumes width rather than the content column's height.
    let mut visible = parts(recipe, height, has_art && !split);
    if has_art && split && height >= 90. {
        visible.push(Part::Artwork);
    }
    let art_size = if split {
        (height.min(width * if size[0] >= 420. { 0.36 } else { 0.20 })).min(190.)
    } else {
        (height - 158.).min(width).max(48.)
    };
    let playing = player.status == music::Playback::Playing;
    let transport = |command, allowed| {
        on.filter(|_| allowed).map(|on| {
            on(Action::Music(music::TransportRequest {
                bus: player.bus.clone(),
                command,
            }))
        })
    };
    let mut pieces: Vec<Element<'a, M>> = Vec::new();
    let artwork = || {
        container(
            cosmic::widget::image(view.art.unwrap().clone())
                .width(art_size)
                .height(art_size)
                .content_fit(cosmic::iced::ContentFit::Contain)
                .border_radius([12.; 4]),
        )
        .id(cosmic::widget::Id::new("music-artwork"))
        .width(art_size)
        .height(art_size)
        .clip(true)
        .into()
    };
    for part in &visible {
        let child = match part {
            Part::Artwork if split => continue,
            Part::Artwork => centered(artwork()),
            Part::Track => {
                let state = if view.problem {
                    "Control failed · try again"
                } else if playing {
                    "NOW PLAYING"
                } else if player.status == music::Playback::Paused {
                    "PAUSED"
                } else {
                    "STOPPED"
                };
                let author = if player.artists.is_empty() {
                    player.album.clone()
                } else {
                    player.artists.join(" · ")
                };
                column::with_children(vec![
                    row::with_children(vec![
                        container(line(state, 11., if view.problem { WARN } else { p.accent })).width(Length::Fill).into(),
                        if matches!(view.feed, Some(music::Feed::Players(players)) if players.len() > 1) {
                            button::custom(line(player.name().to_string(), 11., p.muted)).width(80.).id(cosmic::widget::Id::new("music-source")).class(button_style(p, false, on.is_none())).padding([0,4]).on_press_maybe(on.map(|on| on(Action::MusicCycle))).into()
                        } else { cosmic::widget::space().width(0).into() },
                        if view.manually_selected {
                            button::text("Auto").id(cosmic::widget::Id::new("music-auto")).class(button_style(p, false, on.is_none())).padding([0,4]).on_press_maybe(on.map(|on| on(Action::MusicAuto))).into()
                        } else { cosmic::widget::space().width(0).into() },
                    ]).spacing(4).align_y(Alignment::Center).into(),
                    button::custom(line(
                        if player.has_track() {
                            player.title.clone()
                        } else {
                            "No track selected".into()
                        },
                        20.,
                        p.foreground,
                    )).id(cosmic::widget::Id::new("music-open-player")).class(cosmic::theme::Button::Link)
                        .padding(0).height(25.).width(Length::Fill)
                        .on_press_maybe(transport(music::Command::Raise, player.can_raise)).into(),
                    line(
                        if author.is_empty() {
                            player.name().to_string()
                        } else {
                            author
                        },
                        12.,
                        p.muted,
                    ),
                ])
                .spacing(3)
                .into()
            }
            Part::Playback => centered(
                row::with_children(vec![
                    action(
                        "media-skip-backward-symbolic",
                        "Previous track",
                        "music-previous",
                        transport(
                            music::Command::Previous,
                            player.can_control && player.can_previous,
                        ),
                        p,
                        false,
                        on.is_none(),
                    ),
                    action(
                        if playing {
                            "media-playback-pause-symbolic"
                        } else {
                            "media-playback-start-symbolic"
                        },
                        if playing { "Pause" } else { "Play" },
                        "music-play-pause",
                        transport(
                            if playing {
                                music::Command::Pause
                            } else {
                                music::Command::Play
                            },
                            player.can_control
                                && if playing {
                                    player.can_pause
                                } else {
                                    player.can_play
                                },
                        ),
                        p,
                        true,
                        on.is_none(),
                    ),
                    action(
                        "media-skip-forward-symbolic",
                        "Next track",
                        "music-next",
                        transport(music::Command::Next, player.can_control && player.can_next),
                        p,
                        false,
                        on.is_none(),
                    ),
                ])
                .spacing(16)
                .align_y(Alignment::Center)
                .into(),
            ),
            Part::Progress => {
                let timing = player
                    .position_at(view.now)
                    .zip(player.length_us.filter(|n| *n > 0));
                let fraction = timing.map_or(0., |(at, length)| at as f32 / length as f32);
                let bar: Element<'a, M> = match (timing, on, player.seek_request(0.)) {
                    (Some((_, length)), Some(on), Some(request)) if player.can_control => {
                        slider(0.0..=1.0, fraction, move |value| {
                            let mut request = request.clone();
                            request.position_us = ((value as f64 * length as f64) as i64)
                                .clamp(0, length.saturating_sub(1));
                            on(Action::MusicSeek(request))
                        })
                        .step(0.01)
                        .class(timeline_style(p))
                        .into()
                    }
                    _ => track(fraction, p.accent, p),
                };
                let (at, length) = timing.map_or(
                    ("--:--".into(), "Time unavailable".into()),
                    |(at, length)| (music::duration(at), music::duration(length)),
                );
                column::with_children(vec![
                    container(bar)
                        .id(cosmic::widget::Id::new("music-progress"))
                        .height(12)
                        .align_y(Alignment::Center)
                        .into(),
                    row::with_children(vec![
                        label(at, 10., p.muted),
                        fill(),
                        label(length, 10., p.muted),
                    ])
                    .into(),
                ])
                .spacing(4)
                .into()
            }
            _ => spare(*part, recipe, p),
        };
        pieces.push(child);
    }
    let body: Element<'a, M> = column::with_children(pieces)
        .spacing(8)
        .width(Length::Fill)
        .into();
    let body = if split && visible.contains(&Part::Artwork) {
        row::with_children(vec![artwork(), body])
            .spacing(18)
            .align_y(Alignment::Center)
            .into()
    } else {
        body
    };
    surface(
        container(body)
            .height(Length::Fill)
            .align_y(Alignment::Center)
            .into(),
        config,
        recipe,
        p,
        size,
    )
}

fn timer_ring<'a, M: 'a>(
    content: Element<'a, M>,
    diameter: f32,
    progress: f32,
    p: Palette,
) -> Element<'a, M> {
    let hex = |c: Color| {
        format!(
            "#{:02x}{:02x}{:02x}",
            (c.r * 255.) as u8,
            (c.g * 255.) as u8,
            (c.b * 255.) as u8
        )
    };
    let circumference = std::f32::consts::TAU * 90.;
    let bytes = format!(r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 200 200"><circle cx="100" cy="100" r="90" fill="none" stroke="{}" stroke-opacity="0.16" stroke-width="5"/><circle cx="100" cy="100" r="90" fill="none" stroke="{}" stroke-width="5" stroke-linecap="round" stroke-dasharray="{} {}" transform="rotate(-90 100 100)"/></svg>"#, hex(p.accent), hex(p.accent), circumference * progress.clamp(0., 1.), circumference).into_bytes();
    cosmic::iced::widget::stack(vec![
        svg(svg::Handle::from_memory(bytes))
            .width(diameter)
            .height(diameter)
            .into(),
        container(content)
            .width(diameter)
            .height(diameter)
            .align_x(Alignment::Center)
            .align_y(Alignment::Center)
            .into(),
    ])
    .into()
}
pub(super) fn focus<'a, M: Clone + 'static>(
    session: &focus::Session,
    config: &Config,
    recipe: &Recipe,
    now_ms: i64,
    today: &str,
    on: Option<fn(Action) -> M>,
    dimensions: Option<[f32; 2]>,
    preview: bool,
) -> Element<'a, M> {
    let _scope = crate::widget_style::scope(config);
    let p = palette(config, recipe);
    let size = crate::widget_presentation::viewport(
        dimensions.unwrap_or_else(|| recipe.tool_size()),
        recipe.tool_size(),
    );
    let height = (size[1] - f32::from(PADDING) * 2.).max(1.);
    let width = (size[0] - f32::from(PADDING) * 2.).max(1.);
    let split = recipe.arrangement(size[0]) == Arrangement::Split && size[0] >= 280.;
    let visible = parts(recipe, if split { height + 80. } else { height }, false);
    let next = session.next_phase(&config.focus, today);
    let done = matches!(session.status, focus::Status::Done);
    let phase = if done {
        if session.phase == focus::Phase::Focus {
            "ROUND COMPLETE"
        } else {
            "BREAK COMPLETE"
        }
    } else if matches!(session.status, focus::Status::Paused { .. }) {
        "PAUSED"
    } else {
        session.phase.title()
    };
    let time = if done {
        "✓".into()
    } else {
        focus::clock(session.remaining(now_ms, &config.focus))
    };
    let detail = if done {
        format!(
            "Next · {} min {}",
            next.minutes(&config.focus),
            next.title().to_lowercase()
        )
    } else {
        phase.to_uppercase()
    };
    let remaining_height: f32 = visible
        .iter()
        .filter(|p| **p != Part::Timer)
        .map(|p| match p {
            Part::TimerControls => 46.,
            Part::Caption | Part::SessionDots => 26.,
            _ => 16.,
        })
        .sum();
    let diameter = (if split {
        height.min(width * 0.48)
    } else {
        (height - remaining_height).min(width)
    })
    .min(210.);
    let ring = recipe.timer_face == Some(TimerFace::Ring) && diameter >= 120.;
    let timer_width = if split { width * 0.5 - 8. } else { width };
    let heading = if matches!(session.status, focus::Status::Paused { .. }) {
        format!("{} · PAUSED", session.phase.title().to_uppercase())
    } else {
        phase.to_uppercase()
    };
    let timer = column::with_children(vec![
        centered(line(heading, 11., p.accent)),
        centered(light(
            time,
            if ring {
                diameter * 0.24
            } else if !split && height < 190. {
                36.
            } else {
                48.
            },
            p.foreground,
        )),
        if done {
            centered(line(detail, 12., p.muted))
        } else {
            cosmic::widget::space().height(0).into()
        },
    ])
    .spacing(5)
    .into();
    let timer = if ring {
        timer_ring(timer, diameter, session.progress(now_ms, &config.focus), p)
    } else {
        timer
    };
    let timer: Element<'a, M> = container(timer)
        .id(cosmic::widget::Id::new("focus-recipe-time"))
        .width(timer_width)
        .align_x(Alignment::Center)
        .into();
    let primary = match session.status {
        focus::Status::Running { .. } => "Pause",
        focus::Status::Paused { .. } => "Resume",
        focus::Status::Ready if session.phase == focus::Phase::Focus => "Start",
        focus::Status::Done if next == focus::Phase::Focus => "Start focus",
        _ => "Start break",
    };
    let mut timer = Some(timer);
    let mut pieces = Vec::new();
    for part in &visible {
        let child = match part {
            Part::Timer if split => continue,
            Part::Timer => timer.take().unwrap(),
            Part::TimerControls => {
                let main = button::text(primary)
                    .id(cosmic::widget::Id::new("focus-toggle"))
                    .class(button_style(p, true, preview))
                    .padding([8, 16])
                    .on_press_maybe(on.map(|on| on(Action::FocusToggle)))
                    .into();
                let others = row::with_children(vec![
                    action(
                        "media-skip-forward-symbolic",
                        "Skip phase",
                        "focus-skip",
                        on.map(|on| on(Action::FocusSkip)),
                        p,
                        false,
                        preview,
                    ),
                    action(
                        "view-refresh-symbolic",
                        "Reset timer",
                        "focus-reset",
                        on.map(|on| on(Action::FocusReset)),
                        p,
                        false,
                        preview,
                    ),
                ])
                .spacing(6)
                .into();
                if split && width < 440. {
                    centered(
                        column::with_children(vec![centered(main), centered(others)])
                            .spacing(8)
                            .into(),
                    )
                } else {
                    centered(
                        row::with_children(vec![main, others])
                            .spacing(6)
                            .align_y(Alignment::Center)
                            .into(),
                    )
                }
            }
            Part::SessionDots => {
                let rounds = session.rounds_today(today);
                let every = u32::from(config.focus.long_break_every);
                let full = rounds > 0
                    && rounds % every.max(1) == 0
                    && (session.phase == focus::Phase::LongBreak
                        || done && session.phase == focus::Phase::Focus);
                let filled = if full { every } else { rounds % every.max(1) };
                if (1..=6).contains(&every) {
                    centered(
                        row::with_children(vec![
                            row::with_children(
                                (0..every).map(|i| dot(i < filled, p)).collect::<Vec<_>>(),
                            )
                            .spacing(5)
                            .into(),
                            label(format!("{rounds} today"), 12., p.muted),
                        ])
                        .spacing(10)
                        .align_y(Alignment::Center)
                        .into(),
                    )
                } else {
                    centered(label(
                        if every == 0 {
                            format!("{rounds} rounds today")
                        } else {
                            format!("{filled} / {every} · {rounds} today")
                        },
                        12.,
                        p.muted,
                    ))
                }
            }
            _ => centered(spare(*part, recipe, p)),
        };
        pieces.push(child);
    }
    let body = column::with_children(pieces)
        .spacing(8)
        .width(Length::Fill)
        .into();
    let body = if split {
        row::with_children(vec![timer.unwrap(), body])
            .spacing(16)
            .align_y(Alignment::Center)
            .into()
    } else {
        body
    };
    surface(
        container(body)
            .height(Length::Fill)
            .align_y(Alignment::Center)
            .into(),
        config,
        recipe,
        p,
        size,
    )
}

/// Fixed local snapshots. No player lookup, artwork request, or timer mutation.
pub fn sample(
    config: &Config,
    recipe: &Recipe,
    state: PreviewState,
    size: [f32; 2],
) -> Element<'static, Action> {
    if recipe.kind == Kind::Music {
        let now = Instant::now();
        let player = music::Player {
            bus: "sample".into(),
            key: "sample".into(),
            owner: None,
            seeked: false,
            identity: "Sample player".into(),
            status: if state == PreviewState::Paused {
                music::Playback::Paused
            } else {
                music::Playback::Playing
            },
            title: "Golden Hour".into(),
            artists: vec!["Aura".into()],
            album: "Solstice".into(),
            art_url: None,
            media_url: None,
            track_id: None,
            can_seek: state != PreviewState::NoSeeking,
            length_us: Some(236_000_000),
            position_us: Some(102_000_000),
            rate: 1.,
            sampled: now,
            can_play: true,
            can_pause: true,
            can_next: true,
            can_previous: true,
            can_control: true,
            can_raise: false,
        };
        let feed = if state == PreviewState::Unavailable {
            music::Feed::Unavailable("Sample".into())
        } else {
            music::Feed::Players(vec![player.clone()])
        };
        static ART: std::sync::OnceLock<Handle> = std::sync::OnceLock::new();
        let art = ART.get_or_init(|| {
            let image = music::thumbnail(include_bytes!(
                "../../../assets/looks/evening-wallpaper.jpg"
            ))
            .expect("bundled sample image");
            Handle::from_rgba(image.width, image.height, image.rgba)
        });
        music(
            MusicView {
                feed: Some(&feed),
                player: (state != PreviewState::NoPlayer).then_some(&player),
                art: (state != PreviewState::NoArtwork).then_some(art),
                now,
                manually_selected: false,
                problem: false,
            },
            config,
            recipe,
            None,
            Some(size),
        )
    } else {
        let mut session = focus::Session {
            day: "2026-07-21".into(),
            rounds: 2,
            ..Default::default()
        };
        session.status = match state {
            PreviewState::Paused => focus::Status::Paused {
                remaining: 18 * 60_000 + 42_000,
            },
            PreviewState::Ready => focus::Status::Ready,
            PreviewState::Complete => focus::Status::Done,
            _ => focus::Status::Running {
                ends_at: 18 * 60_000 + 42_000,
            },
        };
        if state == PreviewState::Break {
            session.phase = focus::Phase::Break;
            session.length = 5 * 60_000;
            session.status = focus::Status::Running {
                ends_at: 3 * 60_000 + 12_000,
            };
        }
        focus(
            &session,
            config,
            recipe,
            0,
            "2026-07-21",
            None,
            Some(size),
            true,
        )
    }
}
#[cfg(test)]
mod tests;

/// Circular presentation reuses the same captured MPRIS transport requests.
/// Its decoration never accepts input; title and controls stay keyboard usable.
pub(super) fn circular<'a, M: Clone + 'static>(
    view: MusicView<'_>,
    config: &Config,
    on: Option<fn(Action) -> M>,
    dimensions: Option<[f32; 2]>,
    bins: [f32; crate::tools::visualizer::BINS],
) -> Element<'a, M> {
    let p = config
        .recipes
        .music
        .as_ref()
        .map_or_else(|| config.palette(), |recipe| palette(config, recipe));
    let size = dimensions.unwrap_or([300., 300.]);
    let diameter = size[0].min(size[1]).max(1.);
    let canvas: Element<'a, M> =
        cosmic::widget::canvas(crate::tools::visualizer::drawing::Visual {
            bins,
            circle: config.music.visualizer.circle,
            color: p.accent,
            region: None,
            backdrop: Some(Color {
                a: 0.72,
                ..p.background
            }),
        })
        .width(diameter)
        .height(diameter)
        .into();
    let mut content = Vec::new();
    if let Some(player) = view.player {
        let playing = player.status == music::Playback::Playing;
        let transport = |command, allowed| {
            on.filter(|_| allowed).map(|f| {
                f(Action::Music(music::TransportRequest {
                    bus: player.bus.clone(),
                    command,
                }))
            })
        };
        if diameter >= 200. {
            content.push(centered(line(
                if view.problem {
                    "Control failed"
                } else if playing {
                    "NOW PLAYING"
                } else {
                    "PAUSED"
                },
                10.,
                p.accent,
            )));
        }
        content.push(centered(
            button::custom(line(
                player.title.clone(),
                (diameter * 0.065).clamp(12., 22.),
                p.foreground,
            ))
            .class(cosmic::theme::Button::Link)
            .padding(0)
            .on_press_maybe(transport(music::Command::Raise, player.can_raise))
            .into(),
        ));
        if diameter >= 200. {
            content.push(centered(line(player.artists.join(" · "), 11., p.muted)));
        }
        let mut controls = vec![
            action(
                "media-skip-backward-symbolic",
                "Previous",
                "music-previous",
                transport(
                    music::Command::Previous,
                    player.can_control && player.can_previous,
                ),
                p,
                false,
                on.is_none(),
            ),
            action(
                if playing {
                    "media-playback-pause-symbolic"
                } else {
                    "media-playback-start-symbolic"
                },
                if playing { "Pause" } else { "Play" },
                "music-play-pause",
                transport(
                    if playing {
                        music::Command::Pause
                    } else {
                        music::Command::Play
                    },
                    player.can_control
                        && if playing {
                            player.can_pause
                        } else {
                            player.can_play
                        },
                ),
                p,
                true,
                on.is_none(),
            ),
            action(
                "media-skip-forward-symbolic",
                "Next",
                "music-next",
                transport(music::Command::Next, player.can_control && player.can_next),
                p,
                false,
                on.is_none(),
            ),
        ];
        if diameter < 200. {
            controls.remove(2);
            controls.remove(0);
        }
        content.push(centered(
            row::with_children(controls)
                .spacing(4)
                .align_y(Alignment::Center)
                .into(),
        ));
    } else {
        content.push(centered(line("Music", 20., p.foreground)));
        content.push(centered(line(
            if matches!(view.feed, Some(music::Feed::Unavailable(_))) {
                "Player unavailable"
            } else {
                "Nothing playing"
            },
            12.,
            p.muted,
        )));
    }
    let inner = container(column::with_children(content).spacing(6))
        .width(diameter * 0.64)
        .clip(true);
    let middle: Element<'a, M> = container(inner)
        .width(diameter)
        .height(diameter)
        .align_x(Alignment::Center)
        .align_y(Alignment::Center)
        .into();
    container(cosmic::iced::widget::stack(vec![canvas, middle]))
        .width(size[0])
        .height(size[1])
        .align_x(Alignment::Center)
        .align_y(Alignment::Center)
        .clip(true)
        .into()
}
