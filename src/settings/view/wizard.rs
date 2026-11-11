//! First-run setup with native, opaque theme-aware cards and controls.
//! Illustrations are local samples; the existing wizard owns apply and rollback.
use super::*;
mod widget_pages;
use crate::desktop::profiles::Preset;
use crate::settings::wizard::{Message as Step, Shape, Step as Screen, Wizard};
use cosmic::iced::widget::stack;
use cosmic::widget::image::Handle;

const INK: Color = Color::from_rgb(0.95, 0.95, 1.);
const SOFT: Color = Color::from_rgb(0.72, 0.72, 0.84);
const LAVENDER: Color = Color::from_rgb(0.69, 0.63, 1.);
const PRIMARY: Color = Color::from_rgb(0.60, 0.53, 0.98);
const CARD: Color = Color::from_rgba(0.06, 0.07, 0.16, 0.84);
const TILE: Color = Color::from_rgba(1., 1., 1., 0.05);
const EDGE: Color = Color::from_rgba(1., 1., 1., 0.12);
/// The step card's widest; the window's sides show the backdrop.
const CARD_WIDTH: f32 = 1100.;
/// One caption under every step's preview.
const CAPTION: &str = "Preview with your choices";
const DANGER: Color = Color::from_rgb(0.96, 0.58, 0.54);

fn go(message: Step) -> Message {
    Message::Wizard(message)
}

/// A wizard background, decoded once.
fn backdrop(welcome: bool) -> Option<Handle> {
    static WELCOME: std::sync::OnceLock<Option<Handle>> = std::sync::OnceLock::new();
    static BACKDROP: std::sync::OnceLock<Option<Handle>> = std::sync::OnceLock::new();
    let (cache, bytes): (_, &[u8]) = if welcome {
        (
            &WELCOME,
            include_bytes!("../../../assets/wizard/welcome.jpg"),
        )
    } else {
        (
            &BACKDROP,
            include_bytes!("../../../assets/wizard/backdrop.jpg"),
        )
    };
    cache
        .get_or_init(|| {
            let image =
                image::load_from_memory_with_format(bytes, image::ImageFormat::Jpeg).ok()?;
            let rgba = image.to_rgba8();
            Some(Handle::from_rgba(
                rgba.width(),
                rgba.height(),
                rgba.into_raw(),
            ))
        })
        .clone()
}

/// The pinned renderer can draw Cover outside a clipped stack. Crop the
/// already decoded pixels instead, retaining only the most recent crop. Never
/// decode again or accumulate a texture for every window size while resizing.
fn band(look: Option<&str>, aspect: f32) -> Option<Handle> {
    static LAST: std::sync::Mutex<Option<((Option<String>, u32), Handle)>> =
        std::sync::Mutex::new(None);
    if !aspect.is_finite() || aspect <= 0. {
        return None;
    }
    let key = (look.map(str::to_owned), (aspect * 50.).round() as u32);
    let mut last = LAST.lock().ok()?;
    if let Some((known, handle)) = last.as_ref()
        && known == &key
    {
        return Some(handle.clone());
    }
    let source = look
        .and_then(|id| super::profiles::look_image(id, true))
        .or_else(|| backdrop(false))?;
    let Handle::Rgba {
        width,
        height,
        pixels,
        ..
    } = &source
    else {
        return None;
    };
    let (cw, ch) = if *width as f32 / *height as f32 > aspect {
        (((*height as f32 * aspect) as u32).clamp(1, *width), *height)
    } else {
        (*width, ((*width as f32 / aspect) as u32).clamp(1, *height))
    };
    // Copy contiguous rows instead of invoking a per-pixel image conversion.
    let stride = *width as usize * 4;
    let row_len = cw as usize * 4;
    let x = ((width - cw) / 2) as usize * 4;
    let y = ((height - ch) / 2) as usize;
    let mut cropped = Vec::with_capacity(row_len * ch as usize);
    for row in y..y + ch as usize {
        let start = row * stride + x;
        cropped.extend_from_slice(&pixels[start..start + row_len]);
    }
    let handle = Handle::from_rgba(cw, ch, cropped);
    *last = Some((key, handle.clone()));
    Some(handle)
}

fn words<'a>(value: impl Into<String>, size: f32, color: Color) -> Element<'a, Message> {
    text::body(value.into())
        .size(size)
        .line_height(1.3)
        .class(super::text_style(if color == INK {
            FG
        } else if color == SOFT {
            MUTED
        } else if color == LAVENDER {
            cosmic::theme::active().cosmic().accent_text_color().into()
        } else if color == DANGER {
            super::desktop::ERROR
        } else {
            color
        }))
        .into()
}

fn heading<'a>(value: &'static str, size: f32) -> Element<'a, Message> {
    text::title1(value)
        .size(size)
        .line_height(1.15)
        .font(cosmic::font::bold())
        .class(cosmic::theme::Text::Default)
        .into()
}

fn fill_box<'a>(
    content: Element<'a, Message>,
    color: Color,
    border: Color,
    radius: f32,
) -> cosmic::widget::Container<'a, Message, cosmic::Theme> {
    container(content).class(cosmic::theme::Container::custom(move |theme| {
        let color = if color == CARD {
            theme.cosmic().background(theme.transparent).base.into()
        } else if color == TILE {
            theme.current_container().component.base.into()
        } else if color == PRIMARY || color == LAVENDER {
            theme.cosmic().accent.base.into()
        } else {
            color
        };
        let border = if border == EDGE {
            theme.current_container().component.divider.into()
        } else {
            border
        };
        cosmic::iced::widget::container::Style {
            background: Some(Background::Color(color)),
            border: Border {
                radius: radius.into(),
                width: if border.a > 0. { 1. } else { 0. },
                color: border,
            },
            ..Default::default()
        }
    }))
}

/// A plain drawn shape of `w` × `h`.
fn shape<'a>(w: f32, h: f32, radius: f32, color: Color) -> Element<'a, Message> {
    fill_box(
        cosmic::iced::widget::space().into(),
        color,
        Color::TRANSPARENT,
        radius,
    )
    .width(w)
    .height(h)
    .into()
}

fn button_style(
    _background: Color,
    _hover: Color,
    border: Color,
    _radius: f32,
) -> cosmic::theme::Button {
    super::profiles::tile_style(
        border == LAVENDER,
        cosmic::theme::active().cosmic().accent_color().into(),
    )
}

fn primary<'a>(
    label: &'static str,
    id: &'static str,
    message: Option<Message>,
) -> Element<'a, Message> {
    button::suggested(label)
        .id(cosmic::widget::Id::new(id))
        .on_press_maybe(message)
        .into()
}

fn secondary<'a>(label: &'static str, id: &'static str, message: Message) -> Element<'a, Message> {
    button::standard(label)
        .id(cosmic::widget::Id::new(id))
        .on_press(message)
        .into()
}

fn quiet<'a>(label: &'static str, id: &'static str, message: Message) -> Element<'a, Message> {
    button::text(label)
        .id(cosmic::widget::Id::new(id))
        .on_press(message)
        .into()
}

/// The selected look's mark: a check in a filled accent circle.
fn picked<'a>(selected: bool) -> Element<'a, Message> {
    if !selected {
        return cosmic::iced::widget::space().width(18.).height(18.).into();
    }
    container(
        icon::from_name("object-select-symbolic")
            .size(12)
            .icon()
            .class(cosmic::theme::Svg::custom(|_| cosmic::iced::widget::svg::Style {
                color: Some(Color::from_rgb(0.04, 0.05, 0.1)),
            })),
    )
    .width(18.)
    .height(18.)
    .center_x(18.)
    .center_y(18.)
    .class(cosmic::theme::Container::custom(|theme| {
        cosmic::iced::widget::container::Style {
            background: Some(Background::Color(theme.cosmic().accent.base.into())),
            border: Border {
                radius: 9.0.into(),
                ..Default::default()
            },
            ..Default::default()
        }
    }))
    .into()
}

/// A segmented choice whose options keep stable ids.
fn picker<'a>(
    options: Vec<(&'static str, bool, Option<Message>, String)>,
    accent: Color,
) -> Element<'a, Message> {
    let buttons: Vec<Element<'a, Message>> = options
        .into_iter()
        .map(|(title, selected, message, id)| {
            let ink = if selected {
                FG
            } else if message.is_none() {
                Color { a: 0.35, ..MUTED }
            } else {
                MUTED
            };
            button::custom(label(title, 12., ink))
                .id(cosmic::widget::Id::new(id))
                .class(nav_style(selected, accent))
                .selected(selected)
                .padding([6, 10])
                .on_press_maybe(message)
                .into()
        })
        .collect();
    container(row::with_children(buttons).spacing(2))
        .padding(3)
        .class(cosmic::theme::Container::custom(|theme| {
            cosmic::iced::widget::container::Style {
                background: Some(Background::Color(surface_color(theme, BASE))),
                border: Border {
                    radius: 12.0.into(),
                    ..Default::default()
                },
                ..Default::default()
            }
        }))
        .into()
}

fn corner(shape: Shape, size: f32) -> f32 {
    match shape {
        Shape::Pill => size / 2.,
        Shape::Rounded => (size / 3.).min(5.),
        Shape::Square => 0.,
    }
}

/// The panel drawn `width` wide: one bar, or three islands together.
fn panel_drawing<'a>(islands: bool, shape: Shape, width: f32, height: f32) -> Element<'a, Message> {
    let tone = Color::from_rgba(0.08, 0.08, 0.2, 0.88);
    let piece = |w: Length| -> Element<'a, Message> {
        fill_box(
            row::with_children(vec![shape_dot(height * 0.32), shape_dot(height * 0.32)])
                .spacing(height * 0.2)
                .into(),
            tone,
            EDGE,
            corner(shape, height),
        )
        .width(w)
        .height(height)
        .padding([height * 0.34, height * 0.5])
        .into()
    };
    if islands {
        container(
            row::with_children(vec![
                piece(Length::Fixed(width * 0.16)),
                piece(Length::Fixed(width * 0.2)),
                piece(Length::Fixed(width * 0.14)),
            ])
            .spacing(height * 0.35),
        )
        .width(width)
        .align_x(Alignment::Center)
        .into()
    } else {
        container(piece(Length::Fill)).width(width).into()
    }
}

fn shape_dot<'a>(size: f32) -> Element<'a, Message> {
    shape(size, size, size / 2., Color::from_rgba(1., 1., 1., 0.75))
}

/// The dock: a short row of icons in its shape.
fn dock_drawing<'a>(shape: Shape, icon: f32) -> Element<'a, Message> {
    let icons =
        row::with_children((0..5).map(|_| shape_dot(icon)).collect::<Vec<_>>()).spacing(icon * 0.5);
    fill_box(
        icons.into(),
        Color::from_rgba(0.08, 0.08, 0.2, 0.88),
        EDGE,
        corner(shape, icon * 2.),
    )
    .padding([icon * 0.5, icon])
    .into()
}

impl Settings {
    /// The width inside the step card, beside its scroll bar.
    /// The tallest a step's preview gets; every step shares it.
    fn wizard_preview_height(&self) -> f32 {
        let inner = self.wizard_inner_width();
        let left = inner - (inner * 0.40).clamp(340., 420.) - 16.;
        let screen = self.wizard.as_ref().map_or(self.placement.preview_snapshot().screen, |w| {
            w.widget_plan(&self.config).snapshot.screen
        });
        (left * screen[1] / screen[0].max(1.))
            .min((self.window.height - 300.).clamp(150., 340.))
            .floor()
    }

    fn wizard_inner_width(&self) -> f32 {
        (self.window.width - 48.).min(CARD_WIDTH) - 44. - 18.
    }

    pub(super) fn wizard_view<'a>(&'a self, wizard: &'a Wizard) -> Element<'a, Message> {
        let view: Element<'a, Message> = container(if wizard.step == Screen::Welcome {
            self.wizard_welcome(wizard)
        } else {
            self.wizard_card(wizard)
        })
        .class(cosmic::theme::Container::custom(|theme| {
            cosmic::iced::widget::container::Style {
                background: Some(Background::Color(
                    theme.cosmic().background(theme.transparent).base.into(),
                )),
                ..Default::default()
            }
        }))
        .width(Length::Fill)
        .height(Length::Fill)
        .into();
        view.map(move |message| match message {
            Message::Wizard(action) => go(wizard.rendered(action)),
            other => other,
        })
    }

    fn wizard_welcome<'a>(&'a self, wizard: &'a Wizard) -> Element<'a, Message> {
        // Setup resumed with changes made: continue, or undo them.
        let actions = if wizard.has_changes() {
            vec![
                primary(
                    "Continue",
                    "wizard-start",
                    (!self.native_busy).then_some(go(Step::Start)),
                ),
                quiet("Forget choices", "wizard-cancel", go(Step::Cancel)),
            ]
        } else {
            vec![
                primary(
                    "Get started",
                    "wizard-start",
                    (!self.native_busy).then_some(go(Step::Start)),
                ),
                quiet("Skip for now", "wizard-leave", go(Step::Leave)),
            ]
        };
        let intro = column::with_children(vec![
            heading("Welcome to Halcyon", 32.),
            words(
                "Set up your desktop in four short steps. Nothing changes until you press Apply at the end.",
                15.,
                SOFT,
            ),
            cosmic::iced::widget::space().height(14.).into(),
            column::with_children(
                [
                    "Pick a look",
                    "Dock and panel",
                    "Your widgets",
                    "Review",
                ]
                .into_iter()
                .enumerate()
                .map(|(index, line)| {
                    row::with_children(vec![
                        container(words((index + 1).to_string(), 12., INK))
                            .width(24.)
                            .height(24.)
                            .center_x(24.)
                            .center_y(24.)
                            .class(cosmic::theme::Container::custom(|theme| {
                                cosmic::iced::widget::container::Style {
                                    background: Some(Background::Color(
                                        theme.current_container().component.base.into(),
                                    )),
                                    border: Border {
                                        radius: 12.0.into(),
                                        ..Default::default()
                                    },
                                    ..Default::default()
                                }
                            }))
                            .into(),
                        words(line, 14., INK),
                    ])
                    .spacing(12)
                    .align_y(Alignment::Center)
                    .into()
                })
                .collect::<Vec<_>>(),
            )
            .spacing(10)
            .into(),
            cosmic::iced::widget::space().height(22.).into(),
            row::with_children(actions)
                .spacing(16)
                .align_y(Alignment::Center)
                .into(),
        ])
        .spacing(6);
        // Keep the original welcome artwork separate from the setup previews.
        // Contain preserves the complete bird and sunset at every window size;
        // the decoded handle is shared across redraws and visits.
        let artwork: Element<'a, Message> = match backdrop(true) {
            Some(handle) => cosmic::widget::image(handle)
                .width(Length::Fill)
                .height(Length::Shrink)
                .content_fit(cosmic::iced::ContentFit::Contain)
                .border_radius(12.)
                .into(),
            None => cosmic::iced::widget::space().into(),
        };
        container(super::adaptive::columns(
            container(artwork)
                .id(cosmic::widget::Id::new("wizard-welcome-art"))
                .width(Length::Fill)
                .into(),
            container(intro).padding([24, 0, 0, 0]).into(),
            [320., 280.],
            true,
        ))
        .width(Length::Fill)
        .height(Length::Fill)
        .padding([24, 40])
        .align_y(Alignment::Center)
        .into()
    }

    fn wizard_card<'a>(&'a self, wizard: &'a Wizard) -> Element<'a, Message> {
        let step = wizard.step;
        let placement_error = matches!(step, Screen::Widgets | Screen::Arrange | Screen::Ready)
            .then(|| wizard.widget_plan(&self.config).error)
            .flatten();
        let body = match step {
            Screen::Look => self.wizard_looks(wizard),
            Screen::Bars => self.wizard_bars(wizard),
            Screen::Widgets | Screen::Weather => self.wizard_widgets(wizard, false),
            Screen::Arrange => self.wizard_widgets(wizard, true),
            Screen::Ready => self.wizard_ready(wizard),
            Screen::Done => self.wizard_done(wizard),
            Screen::Welcome => cosmic::iced::widget::space().into(),
        };
        let mut parts = vec![
            row::with_children(vec![
                container(words(
                    if step == Screen::Done {
                        "Setup finished".to_string()
                    } else {
                        format!("Step {} of 4", step.index().min(4))
                    },
                    12.,
                    SOFT,
                ))
                .id(cosmic::widget::Id::new("wizard-progress"))
                .width(Length::Fill)
                .into(),
            ])
            .align_y(Alignment::Center)
            .into(),
            heading(
                match step {
                    Screen::Look => "Pick a look",
                    Screen::Bars => "Dock and panel",
                    Screen::Widgets | Screen::Arrange | Screen::Weather => "Your widgets",
                    Screen::Ready => "Review",
                    Screen::Done => "Your desktop is ready",
                    Screen::Welcome => "Welcome",
                },
                26.,
            ),
            words(
                match step {
                    Screen::Look => {
                        "A look brings a wallpaper and a set of widgets. You can change the widgets next."
                    }
                    Screen::Bars => "Keep your current dock and panel, or pick a new style.",
                    Screen::Widgets | Screen::Arrange | Screen::Weather => {
                        "Choose what appears on your desktop and in what order."
                    }
                    Screen::Ready => {
                        "Nothing changes until you press Apply, and one Undo puts it all back."
                    }
                    Screen::Done => "Applied as one change. Undo setup puts it all back.",
                    Screen::Welcome => "Make yourself at home.",
                },
                14.,
                SOFT,
            ),
            cosmic::iced::widget::space().height(6.).into(),
            scrollable(container(body).padding([0, 18, 12, 0]))
                .id(cosmic::widget::Id::new("wizard-body"))
                .height(Length::Fill)
                .into(),
        ];
        // The reason a step cannot apply stays beside its action, outside the
        // scrollable preview. Give the person a way to correct it, keeping the draft.
        if let Some(error) = wizard.error.as_ref().or(placement_error.as_ref()) {
            parts.push(
                container(words(error.clone(), 13., DANGER))
                    .id(cosmic::widget::Id::new("wizard-step-error"))
                    .width(Length::Fill)
                    .into(),
            );
        }
        if wizard.leaving {
            parts.push(
                container(
                    row::with_children(vec![
                        container(words("Your desktop stays as it is either way.", 13., INK))
                            .width(Length::Fill)
                            .into(),
                        quiet("Stay", "wizard-stay", go(Step::AskLeave(false))),
                        secondary("Forget choices", "wizard-cancel", go(Step::Cancel)),
                        primary(
                            "Keep choices for later",
                            "wizard-keep",
                            Some(go(Step::KeepForLater)),
                        ),
                    ])
                    .spacing(8)
                    .align_y(Alignment::Center)
                    .wrap(),
                )
                .width(Length::Fill)
                .into(),
            );
        }
        let (left, right): (Vec<Element<'a, Message>>, Element<'a, Message>) = match step {
            Screen::Done => {
                let mut left = Vec::new();
                if wizard.has_changes() {
                    left.push(secondary("Undo setup", "wizard-undo", go(Step::UndoSetup)));
                }
                (
                    left,
                    primary(
                        "Done",
                        "wizard-finish",
                        (!self.native_busy).then_some(go(Step::Finish)),
                    ),
                )
            }
            Screen::Ready => (
                vec![
                    secondary("Back", "wizard-back", go(Step::Back)),
                    quiet("Set up later", "wizard-leave-ask", go(Step::AskLeave(true))),
                ],
                primary(
                    "Apply and finish",
                    "wizard-apply",
                    (!self.native_busy && placement_error.is_none()).then_some(go(Step::ApplyAll)),
                ),
            ),
            _ => (
                vec![
                    secondary("Back", "wizard-back", go(Step::Back)),
                    quiet("Set up later", "wizard-leave-ask", go(Step::AskLeave(true))),
                ],
                button::suggested(if placement_error.is_some() {
                    "Choose widgets"
                } else {
                    "Next"
                })
                .id(cosmic::widget::Id::new("wizard-next"))
                .on_press_maybe(
                    (!self.native_busy).then_some(go(if placement_error.is_some() {
                        Step::Show(Screen::Widgets)
                    } else {
                        Step::Next
                    })),
                )
                .into(),
            ),
        };
        parts.push(cosmic::iced::widget::space().height(6.).into());
        parts.push(
            row::with_children(vec![
                container(
                    row::with_children(left)
                        .spacing(8)
                        .align_y(Alignment::Center),
                )
                .width(Length::FillPortion(2))
                .into(),
                container(right).align_x(Alignment::End).into(),
            ])
            // In line with the content, which leaves room for its scrollbar.
            .padding([0, 18, 0, 0])
            .align_y(Alignment::Center)
            .into(),
        );
        let card = fill_box(
            column::with_children(parts).spacing(4).into(),
            CARD,
            Color::TRANSPARENT,
            18.,
        )
        .padding([16, 22])
        .max_width(CARD_WIDTH)
        .height(Length::Fill);
        container(card)
            .width(Length::Fill)
            .height(Length::Fill)
            .padding([16, 24])
            .align_x(Alignment::Center)
            .into()
    }

    fn wizard_current_thumbnail<'a>(&'a self, height: f32) -> Element<'a, Message> {
        match self.profiles.home_image() {
            Some(handle) => cosmic::widget::image(handle.clone())
                .width(Length::Fill)
                .height(height)
                .content_fit(cosmic::iced::ContentFit::Cover)
                .border_radius(8.)
                .into(),
            None => fill_box(
                container(words("Your wallpaper", 12., SOFT))
                    .width(Length::Fill)
                    .height(height)
                    .center_x(Length::Fill)
                    .center_y(height)
                    .into(),
                TILE,
                EDGE,
                8.,
            )
            .width(Length::Fill)
            .into(),
        }
    }

    fn wizard_looks<'a>(&'a self, wizard: &'a Wizard) -> Element<'a, Message> {
        let looks: Vec<&Preset> = self
            .profiles
            .presets()
            .iter()
            .filter(|preset| {
                preset.id != crate::desktop::profiles::CLEAN && !preset.profile.appearance_only
            })
            .collect();
        let columns = ((self.wizard_inner_width() + 12.) / 232.).floor().max(1.) as usize;
        let tile_width = (self.wizard_inner_width() - (columns - 1) as f32 * 12.) / columns as f32;
        let by_width = (tile_width - 14.) * 9. / 16.;
        // Thumbnails keep the screen's shape, so they fill the tile width.
        let thumb_height = by_width
            .min((self.window.height - 430.).max(100.))
            .clamp(96., 180.);
        let tile = |preset: &'a Preset| -> Element<'a, Message> {
            let selected = wizard.look == Some(preset.id);
            button::custom(
                column::with_children(vec![
                    self.preset_thumbnail(preset, thumb_height),
                    row::with_children(vec![
                        container(words(preset.profile.name.clone(), 13., INK))
                            .width(Length::Fill)
                            .into(),
                        picked(selected),
                    ])
                    .align_y(Alignment::Center)
                    .into(),
                    container(words(preset.description, 11., SOFT))
                        .height(30.)
                        .clip(true)
                        .into(),
                ])
                .spacing(4),
            )
            .id(cosmic::widget::Id::new(format!(
                "wizard-look-{}",
                preset.id
            )))
            .class(button_style(
                if selected {
                    Color::from_rgba(0.60, 0.53, 0.98, 0.16)
                } else {
                    TILE
                },
                Color::from_rgba(1., 1., 1., 0.09),
                if selected { LAVENDER } else { EDGE },
                12.,
            ))
            .padding(7)
            .width(tile_width)
            .on_press(go(Step::Look(preset.id)))
            .into()
        };
        let keep = button::custom(
            column::with_children(vec![
                self.wizard_current_thumbnail(thumb_height),
                row::with_children(vec![
                    container(words("My current desktop", 13., INK))
                        .width(Length::Fill)
                        .into(),
                    picked(wizard.look.is_none()),
                ])
                .align_y(Alignment::Center)
                .into(),
                container(words(
                    "Your wallpaper and widgets, as they are now.",
                    11.,
                    SOFT,
                ))
                .height(30.)
                .clip(true)
                .into(),
            ])
            .spacing(4),
        )
        .id(cosmic::widget::Id::new("wizard-look-current"))
        .class(button_style(
            if wizard.look.is_none() {
                Color::from_rgba(0.60, 0.53, 0.98, 0.16)
            } else {
                TILE
            },
            Color::from_rgba(1., 1., 1., 0.09),
            if wizard.look.is_none() {
                LAVENDER
            } else {
                EDGE
            },
            12.,
        ))
        .padding(7)
        .width(tile_width)
        .on_press(go(Step::KeepCurrent))
        .into();
        let mut tiles = vec![keep];
        tiles.extend(looks.iter().map(|preset| tile(preset)));
        row::with_children(tiles).spacing(12).wrap().into()
    }

    /// Keep the bars, or change them with the same rows as Settings, beside
    /// a preview of the chosen look with the panel and dock drawn on it.
    fn wizard_bars<'a>(&'a self, wizard: &'a Wizard) -> Element<'a, Message> {
        let bars = wizard.bars;
        let keep = wizard.keep_bars && !wizard.has_bar_choices();
        let accent = self.settings_accent();
        let mut controls = vec![picker(
            vec![
                ("Keep as they are", keep, Some(go(Step::KeepBars(true))), "wizard-bars-keep".into()),
                ("Change them", !keep, Some(go(Step::KeepBars(false))), "wizard-bars-custom".into()),
            ],
            accent,
        )];
        let dock_summary = format!(
            "{}, {}, {}",
            bars.dock_shape.title(),
            if bars.floating { "floating" } else { "on the screen edge" },
            if bars.hides { "hides until needed" } else { "always shown" }
        );
        if keep {
            controls.push(group(
                Some("What you have now"),
                vec![
                    detail(
                        "Panel",
                        format!(
                            "{} {}",
                            bars.panel_shape.title(),
                            if bars.islands { "islands" } else { "bar" }
                        ),
                    ),
                    detail("Dock", dock_summary),
                ],
            ));
            controls.push(words(
                "Nothing here changes. You can still adjust both later in Settings.",
                12.,
                SOFT,
            ));
        } else {
            let shapes = |current: Shape, part: &'static str, message: fn(Shape) -> Step| {
                picker(
                    Shape::ALL
                        .into_iter()
                        .map(|candidate| {
                            (
                                candidate.title(),
                                current == candidate,
                                Some(go(message(candidate))),
                                format!("wizard-{part}-{}", candidate.title().to_lowercase()),
                            )
                        })
                        .collect(),
                    accent,
                )
            };
            let islands_ready =
                wizard.islands_available && self.desktop.islands.installation.installed();
            controls.push(group(
                Some("Panel"),
                vec![
                    group_row(
                        "Style",
                        "Islands splits it into floating pieces",
                        picker(
                            vec![
                                ("Bar", !bars.islands, Some(go(Step::Islands(false))), "wizard-panel-bar".into()),
                                (
                                    "Islands",
                                    bars.islands,
                                    islands_ready.then_some(go(Step::Islands(true))),
                                    "wizard-panel-islands".into(),
                                ),
                            ],
                            accent,
                        ),
                    ),
                    group_row("Corners", "", shapes(bars.panel_shape, "panel", Step::PanelShape)),
                ],
            ));
            controls.push(group(
                Some("Dock"),
                vec![
                    group_row("Corners", "", shapes(bars.dock_shape, "dock", Step::DockShape)),
                    group_row(
                        "Position",
                        "",
                        picker(
                            vec![
                                ("Floating", bars.floating, Some(go(Step::Floating(true))), "wizard-dock-floating".into()),
                                ("On the edge", !bars.floating, Some(go(Step::Floating(false))), "wizard-dock-attached".into()),
                            ],
                            accent,
                        ),
                    ),
                    group_row(
                        "Visibility",
                        "",
                        picker(
                            vec![
                                ("Always shown", !bars.hides, Some(go(Step::Hides(false))), "wizard-dock-shown".into()),
                                ("Hides", bars.hides, Some(go(Step::Hides(true))), "wizard-dock-hides".into()),
                            ],
                            accent,
                        ),
                    ),
                ],
            ));
            if bars.islands {
                controls.push(words(
                    if self.desktop.islands.installation.installed() {
                        "Islands start at next login. No need to log out now."
                    } else {
                        "Islands shows as a normal panel in this preview. Check it in Settings after setup."
                    },
                    12.,
                    SOFT,
                ));
            }
        }
        super::adaptive::columns(
            column::with_children(vec![
                self.wizard_bars_preview(wizard),
                words(if keep { "Your dock and panel now" } else { CAPTION }, 12., SOFT),
            ])
            .spacing(8)
            .align_x(Alignment::Center)
            .into(),
            column::with_children(controls).spacing(12).into(),
            [300., 340.],
            true,
        )
    }

    /// The chosen look's wallpaper with the panel on top and the dock below
    /// as they will look, each named so the picture reads at a glance.
    fn wizard_bars_preview<'a>(&'a self, wizard: &'a Wizard) -> Element<'a, Message> {
        let mut bars = wizard.bars;
        bars.islands &= self.desktop.islands.installation.installed();
        let look = wizard.look;
        let most = self.wizard_preview_height();
        let scene = cosmic::widget::responsive(move |bounds| {
            let w = bounds.width.floor().max(120.);
            let h = most;
            let background: Element<'a, Message> = match band(look, w / h) {
                Some(handle) => cosmic::widget::image(handle)
                    .width(w)
                    .height(h)
                    .content_fit(cosmic::iced::ContentFit::Fill)
                    .into(),
                None => shape(w, h, 0., Color::from_rgb(0.08, 0.08, 0.2)),
            };
            let panel_gap = if bars.islands { 6. } else { 0. };
            let tag = |name: &'static str| -> Element<'a, Message> {
                fill_box(
                    words(name, 11., INK),
                    Color::from_rgba(0., 0., 0., 0.45),
                    Color::TRANSPARENT,
                    6.,
                )
                .padding([2, 8])
                .into()
            };
            let overlay = column::with_children(vec![
                container(panel_drawing(
                    bars.islands,
                    bars.panel_shape,
                    w - 2. * panel_gap - 16.,
                    16.,
                ))
                .width(Length::Fill)
                .padding([panel_gap, 8. + panel_gap])
                .align_x(Alignment::Center)
                .into(),
                container(tag("Panel")).padding([6, 16]).into(),
                cosmic::iced::widget::space().height(Length::Fill).into(),
                container(tag("Dock"))
                    .width(Length::Fill)
                    .padding([0, 0, 6, 0])
                    .align_x(Alignment::Center)
                    .into(),
                container(if bars.hides {
                    shape(60., 4., 2., Color::from_rgba(1., 1., 1., 0.5))
                } else {
                    dock_drawing(bars.dock_shape, 8.)
                })
                .width(Length::Fill)
                .align_x(Alignment::Center)
                .padding([0., 0., if bars.floating { 8. } else { 0. }, 0.])
                .into(),
            ])
            .width(w)
            .height(h);
            container(
                fill_box(
                    stack(vec![background, overlay.into()])
                        .width(w)
                        .height(h)
                        .clip(true)
                        .into(),
                    Color::TRANSPARENT,
                    EDGE,
                    12.,
                )
                .clip(true),
            )
            .width(Length::Fill)
            .height(h)
            .into()
        });
        container(scene).width(Length::Fill).height(most).into()
    }

    /// What the city search found, under the Weather city row.
    fn wizard_city_results(&self) -> Option<Element<'_, Message>> {
        let state = &self.city_search;
        let mut parts = Vec::new();
        if state.searching || state.deadline.is_some() {
            parts.push(words("Searching cities…", 13., SOFT));
        } else if let Some(error) = &state.error {
            parts.push(words(error.clone(), 13., DANGER));
        } else if !self.cities.is_empty() {
            for (index, city) in self.cities.iter().enumerate() {
                let selected = self.config.location.as_ref().is_some_and(|location| {
                    location.latitude == city.latitude && location.longitude == city.longitude
                });
                parts.push(
                    button::custom(
                        row::with_children(vec![
                            column::with_children(vec![
                                words(city.name.clone(), 14., INK),
                                words(city.region(), 12., SOFT),
                            ])
                            .width(Length::Fill)
                            .into(),
                            words(if selected { "✓" } else { "Choose" }, 13., LAVENDER),
                        ])
                        .align_y(Alignment::Center),
                    )
                    .id(cosmic::widget::Id::new(format!("wizard-city-{index}")))
                    .class(button_style(
                        TILE,
                        Color::from_rgba(1., 1., 1., 0.09),
                        if selected { LAVENDER } else { EDGE },
                        10.,
                    ))
                    .padding([8, 12])
                    .width(Length::Fill)
                    .on_press(Message::ChooseCity(index))
                    .into(),
                );
            }
        } else if state.searched {
            parts.push(words("No cities found. Try a nearby town.", 13., SOFT));
        }
        (!parts.is_empty()).then(|| column::with_children(parts).spacing(8).into())
    }

    /// Choose and Order are two views of one step. Neither adds a step.
    fn wizard_widget_tabs<'a>(&self, step: Screen) -> Element<'a, Message> {
        segmented(
            vec![
                (
                    "Choose".into(),
                    step != Screen::Arrange,
                    Some(go(Step::Show(Screen::Widgets))),
                ),
                (
                    "Order".into(),
                    step == Screen::Arrange,
                    Some(go(Step::Show(Screen::Arrange))),
                ),
            ],
            self.settings_accent(),
        )
    }

    /// The ending names what happened, offers one Undo and what can come next.
    fn wizard_done<'a>(&'a self, wizard: &'a Wizard) -> Element<'a, Message> {
        let mut parts: Vec<Element<'a, Message>> = Vec::new();
        let applied = wizard.applied.as_ref();
        let lines: Vec<Element<'a, Message>> = if applied.is_some_and(|applied| applied.changed) {
            applied
                .map(|a| a.lines.as_slice())
                .unwrap_or_default()
                .iter()
                .map(|line| {
                    row::with_children(vec![
                        icon::from_name("object-select-symbolic").size(16).into(),
                        words(line.clone(), 14., INK),
                    ])
                    .spacing(10)
                    .align_y(Alignment::Center)
                    .into()
                })
                .collect()
        } else {
            vec![words("Your desktop stays as it was.", 14., INK)]
        };
        parts.push(group(Some("What changed"), lines));
        if applied.is_some_and(|applied| applied.next_login) {
            parts.push(words(
                "Islands start at next login. No need to log out now.",
                13.,
                SOFT,
            ));
        }
        let pending: Vec<Element<'a, Message>> = crate::settings::wizard::follow_up::Task::ALL
            .into_iter()
            .filter(|task| task.needed(&self.config, self.calendar.connection.connected()))
            .map(|task| {
                group_row(
                    task.title(),
                    match task.key() {
                        "weather" => "So the forecast shows your city",
                        "calendar" => "So your events show on the desktop",
                        _ => "So the Projects widget lists your work",
                    },
                    button::standard(task.action())
                        .on_press_maybe(
                            (!self.native_busy).then_some(Message::FinishSetupTo(task.page())),
                        )
                        .into(),
                )
            })
            .collect();
        if !pending.is_empty() {
            parts.push(group(Some("Later, if you like"), pending));
        }
        super::adaptive::columns(
            column::with_children(vec![
                self.wizard_widget_preview(wizard, self.wizard_preview_height()),
                words("Your desktop now", 12., SOFT),
            ])
            .spacing(8)
            .align_x(Alignment::Center)
            .into(),
            container(column::with_children(parts).spacing(8))
                .id(cosmic::widget::Id::new("wizard-done"))
                .width(Length::Fill)
                .into(),
            [300., 340.],
            true,
        )
    }

    fn wizard_ready<'a>(&'a self, wizard: &'a Wizard) -> Element<'a, Message> {
        let bars = wizard.bars;
        let look = wizard
            .look
            .and_then(|id| {
                self.profiles
                    .presets()
                    .iter()
                    .find(|preset| preset.id == id)
                    .map(|preset| preset.profile.name.clone())
            })
            .unwrap_or_else(|| "My current desktop".into());
        let panel = format!(
            "{} {}, {} dock",
            bars.panel_shape.title(),
            if bars.islands { "islands" } else { "bar" },
            bars.dock_shape.title().to_lowercase()
        );
        let city = wizard
            .city
            .as_ref()
            .or(self.config.location.as_ref())
            .map_or_else(|| "Not set".to_string(), |location| location.name.clone());
        let summary = |name: &'static str, value: String, target: Screen| -> Element<'a, Message> {
            group_row(
                name,
                value,
                button::standard("Change")
                    .on_press(go(Step::Show(target)))
                    .into(),
            )
        };
        let mut choices = vec![
            summary("Look", look, Screen::Look),
            summary(
                "Dock and panel",
                if wizard.keep_bars && !wizard.has_bar_choices() {
                    "Keep as they are".into()
                } else {
                    panel
                },
                Screen::Bars,
            ),
            summary(
                "Widgets",
                if wizard.widgets.active().is_empty() {
                    "None".into()
                } else {
                    wizard
                        .widgets
                        .active()
                        .iter()
                        .map(|w| w.title())
                        .collect::<Vec<_>>()
                        .join(", ")
                },
                Screen::Widgets,
            ),
        ];
        if wizard.widgets.selected(Widget::Weather) {
            choices.push(summary(
                "Weather city",
                if wizard.city.is_none() && self.config.location.is_none() {
                    "Not chosen. Preview shows sample weather.".into()
                } else {
                    city
                },
                Screen::Widgets,
            ));
        }
        let mut parts = vec![group(Some("Your choices"), choices)];
        if wizard.bars.islands && wizard.has_bar_choices() {
            parts.push(words(
                "Islands start at next login. No need to log out now.",
                12.,
                SOFT,
            ));
        }
        let mut running = Vec::new();
        if !wizard.widgets.active().is_empty() {
            running.push(group_row(
                "Start at sign-in",
                "Widgets come back each time you sign in",
                container(toggler(wizard.startup).on_toggle(|on| go(Step::Startup(on))))
                    .id(cosmic::widget::Id::new("wizard-startup"))
                    .into(),
            ));
        }
        running.push(self.usage_control());
        parts.push(group(Some("Sign-in and privacy"), running));
        let projected = wizard.widgets.config(&self.config);
        let pending = crate::settings::wizard::follow_up::Task::ALL
            .into_iter()
            .filter(|task| task.needed(&projected, self.calendar.connection.connected()))
            .map(|task| task.title())
            .collect::<Vec<_>>();
        if !pending.is_empty() {
            parts.push(
                container(words(
                    format!(
                        "Later, if you like: {}. You can find these under Widgets.",
                        pending.join(" · ")
                    ),
                    12.,
                    SOFT,
                ))
                .id(cosmic::widget::Id::new("wizard-followups"))
                .into(),
            );
        }
        super::adaptive::columns(
            column::with_children(vec![
                self.wizard_widget_preview(wizard, self.wizard_preview_height()),
                words(CAPTION, 12., SOFT),
            ])
            .spacing(8)
            .align_x(Alignment::Center)
            .into(),
            column::with_children(parts).spacing(8).into(),
            [300., 340.],
            true,
        )
    }
}

#[cfg(test)]
mod image_tests {
    use super::*;
    #[test]
    fn resize_crop_is_bounded_and_reuses_decoded_pixels() {
        let first = band(None, 4.).unwrap();
        assert_eq!(first.id(), band(None, 4.).unwrap().id());
        let Handle::Rgba { pixels: source, .. } = backdrop(false).unwrap() else {
            panic!("source pixels");
        };
        let Handle::Rgba { pixels: crop, .. } = &first else {
            panic!("crop pixels");
        };
        assert_eq!(
            &crop[..],
            &source[300 * 1600 * 4..700 * 1600 * 4],
            "centered 1600x400 strip"
        );

        for aspect in 5..85 {
            let cropped = band(None, aspect as f32).unwrap();
            let Handle::Rgba {
                width,
                height,
                ref pixels,
                ..
            } = cropped
            else {
                panic!("decoded crop");
            };
            assert_eq!(width, 1600);
            assert!(height <= 320);
            assert_eq!(pixels.len(), width as usize * height as usize * 4);
        }
        // One entry, not one per historical size: revisiting recreates a crop.
        assert_ne!(first.id(), band(None, 4.).unwrap().id());
        assert!(band(None, f32::NAN).is_none());
        assert!(band(None, 0.).is_none());
    }
}
