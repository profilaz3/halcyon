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

fn dots<'a>(current: Screen) -> Element<'a, Message> {
    row::with_children(
        Screen::ALL
            .iter()
            .map(|step| {
                if *step == current {
                    shape(22., 8., 4., PRIMARY)
                } else {
                    shape(8., 8., 4., Color::from_rgba(1., 1., 1., 0.28))
                }
            })
            .collect::<Vec<_>>(),
    )
    .spacing(8)
    .align_y(Alignment::Center)
    .into()
}

/// A choice drawn as a small picture with its name under it.
fn choice<'a>(
    picture: Element<'a, Message>,
    name: &'static str,
    id: String,
    selected: bool,
    message: Option<Message>,
) -> Element<'a, Message> {
    let enabled = message.is_some();
    button::custom(
        column::with_children(vec![
            container(picture)
                .width(Length::Fill)
                .height(18.)
                .align_x(Alignment::Center)
                .align_y(Alignment::Center)
                .into(),
            words(name, 12., if enabled { INK } else { SOFT }),
        ])
        .spacing(3)
        .align_x(Alignment::Center),
    )
    .id(cosmic::widget::Id::new(id))
    .class(button_style(
        if selected {
            Color::from_rgba(0.60, 0.53, 0.98, 0.16)
        } else {
            TILE
        },
        Color::from_rgba(1., 1., 1., 0.09),
        if selected { LAVENDER } else { EDGE },
        10.,
    ))
    .padding([5, 4])
    .width(Length::Fill)
    .on_press_maybe(message)
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
    fn wizard_inner_width(&self) -> f32 {
        (self.window.width - 48.).min(CARD_WIDTH) - 44. - 12.
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
                quiet("Cancel setup", "wizard-cancel", go(Step::Cancel)),
            ]
        } else {
            vec![
                primary(
                    "Start",
                    "wizard-start",
                    (!self.native_busy).then_some(go(Step::Start)),
                ),
                quiet("Skip", "wizard-leave", go(Step::Leave)),
            ]
        };
        let intro = column::with_children(vec![
            words("Step 1 of 7", 12., SOFT),
            heading("Welcome to Halcyon", 32.),
            words(
                "Choose a look, add widgets and arrange your desktop. Every step is optional.",
                15.,
                SOFT,
            ),
            cosmic::iced::widget::space().height(18.).into(),
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
        column::with_children(vec![
            container(super::adaptive::columns(
                container(artwork)
                    .id(cosmic::widget::Id::new("wizard-welcome-art"))
                    .width(Length::Fill)
                    .into(),
                intro.into(),
                [320., 280.],
                true,
            ))
            .width(Length::Fill)
            .height(Length::Fill)
            .padding([24, 40])
            .align_y(Alignment::Center)
            .into(),
            container(dots(Screen::Welcome))
                .width(Length::Fill)
                .padding([0, 0, 28, 0])
                .align_x(Alignment::Center)
                .into(),
        ])
        .into()
    }

    fn wizard_card<'a>(&'a self, wizard: &'a Wizard) -> Element<'a, Message> {
        let step = wizard.step;
        let placement_error = (step == Screen::Arrange)
            .then(|| wizard.widget_plan(&self.config).error)
            .flatten();
        let body = match step {
            Screen::Look => self.wizard_looks(wizard),
            Screen::Bars => self.wizard_bars(wizard),
            Screen::Widgets => self.wizard_widgets(wizard, false),
            Screen::Arrange => self.wizard_widgets(wizard, true),
            Screen::Weather => self.wizard_weather(),
            Screen::Ready => self.wizard_ready(wizard),
            Screen::Welcome => cosmic::iced::widget::space().into(),
        };
        let mut parts = vec![
            row::with_children(vec![
                container(words(
                    format!(
                        "Step {} of {} · {}",
                        step.index() + 1,
                        Screen::ALL.len(),
                        step.title()
                    ),
                    12.,
                    SOFT,
                ))
                .id(cosmic::widget::Id::new("wizard-progress"))
                .width(Length::Fill)
                .into(),
                dots(step),
            ])
            .align_y(Alignment::Center)
            .into(),
            heading(
                match step {
                    Screen::Look => "Starter setup",
                    Screen::Bars => "Dock and panel",
                    Screen::Widgets => "Your widgets",
                    Screen::Arrange => "Arrange your widgets",
                    Screen::Weather => "Weather",
                    Screen::Ready => "Your desktop is ready",
                    Screen::Welcome => "Welcome",
                },
                26.,
            ),
            words(
                match step {
                    Screen::Look => {
                        "Setups · Choices apply as you go. Cancel reverses setup’s desktop changes."
                    }
                    Screen::Bars => "Desktop · Choose the shape and behavior of your bars.",
                    Screen::Widgets => "Turn widgets on or off. Arrange them in the next step.",
                    Screen::Arrange => "Drag or use the arrows to reorder.",
                    Screen::Weather => {
                        "Widgets · Choose a forecast city. This preference stays if you cancel setup."
                    }
                    Screen::Ready => "Find these choices again in Setups, Desktop and Widgets.",
                    Screen::Welcome => "Make yourself at home.",
                },
                14.,
                SOFT,
            ),
            cosmic::iced::widget::space().height(6.).into(),
            scrollable(container(body).padding([0, 12, 12, 0]))
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
        let (left, right): (Vec<Element<'a, Message>>, Element<'a, Message>) = match step {
            Screen::Ready => (
                vec![
                    secondary("Back", "wizard-back", go(Step::Back)),
                    quiet("Cancel", "wizard-cancel", go(Step::Cancel)),
                ],
                primary(
                    if wizard.widgets.pending {
                        "Review widgets"
                    } else {
                        "Finish"
                    },
                    "wizard-finish",
                    (!self.native_busy).then_some(go(Step::Finish)),
                ),
            ),
            _ => (
                vec![
                    secondary("Back", "wizard-back", go(Step::Back)),
                    quiet(
                        if matches!(step, Screen::Widgets | Screen::Arrange) {
                            "Keep current widgets"
                        } else if step == Screen::Weather {
                            "Do this later"
                        } else {
                            "Skip"
                        },
                        "wizard-skip",
                        go(Step::Skip),
                    ),
                ],
                // Cancel on every step undoes what setup changed so far.
                row::with_children(vec![
                    quiet("Cancel", "wizard-cancel", go(Step::Cancel)),
                    button::suggested(match step {
                        Screen::Widgets => "Arrange widgets",
                        Screen::Arrange if placement_error.is_some() => "Choose widgets",
                        Screen::Arrange if wizard.widgets.active().is_empty() => "Continue",
                        Screen::Arrange => "Use these widgets",
                        _ => "Next",
                    })
                    .id(cosmic::widget::Id::new("wizard-next"))
                    .on_press_maybe((!self.native_busy).then_some(go(
                        if placement_error.is_some() {
                            Step::Show(Screen::Widgets)
                        } else {
                            Step::Next
                        },
                    )))
                    .into(),
                ])
                .spacing(8)
                .align_y(Alignment::Center)
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
            .align_y(Alignment::Center)
            .into(),
        );
        let card = fill_box(
            column::with_children(parts).spacing(4).into(),
            CARD,
            EDGE,
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
        let thumb_height = by_width
            .min((self.window.height - 390.).max(100.))
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
                        words(if selected { "✓" } else { "" }, 13., LAVENDER),
                    ])
                    .align_y(Alignment::Center)
                    .into(),
                ])
                .spacing(6),
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
        row::with_children(looks.iter().map(|preset| tile(preset)).collect::<Vec<_>>())
            .spacing(12)
            .wrap()
            .into()
    }

    fn wizard_bars<'a>(&'a self, wizard: &'a Wizard) -> Element<'a, Message> {
        let bars = wizard.bars;
        // Both columns fit even at the minimum 820px width. Stacking them
        // makes the dock choices fall below the footer in a 600px window.
        let column_width = (self.wizard_inner_width() - 18.) / 2.;
        let tile_width = (column_width - 8. * 2.) / 3.;
        let draw = tile_width - 24.;
        let line =
            |name: &'static str, choices: Vec<Element<'a, Message>>| -> Element<'a, Message> {
                column::with_children(vec![
                    words(name, 12., SOFT),
                    row::with_children(choices).spacing(8).into(),
                ])
                .spacing(4)
                .into()
            };
        let shapes = |current: Shape, part: &'static str, message: fn(Shape) -> Step| {
            Shape::ALL
                .into_iter()
                .map(|candidate| {
                    choice(
                        shape(draw * 0.8, 16., corner(candidate, 16.), LAVENDER),
                        candidate.title(),
                        format!("wizard-{part}-{}", candidate.title()).to_lowercase(),
                        current == candidate,
                        Some(go(message(candidate))),
                    )
                })
                .collect::<Vec<_>>()
        };
        let mut styles = vec![choice(
            panel_drawing(false, bars.panel_shape, draw, 12.),
            "Bar",
            "wizard-panel-bar".into(),
            !bars.islands,
            Some(go(Step::Islands(false))),
        )];
        styles.push(choice(
            panel_drawing(true, bars.panel_shape, draw, 12.),
            "Islands",
            "wizard-panel-islands".into(),
            bars.islands,
            (wizard.islands_available && self.desktop.islands.installation.installed())
                .then_some(go(Step::Islands(true))),
        ));
        let panel = vec![
            words("Panel", 15., INK),
            line("Style", styles),
            line("Shape", shapes(bars.panel_shape, "panel", Step::PanelShape)),
        ];
        let edge = || shape(46., 2., 1., Color::from_rgba(1., 1., 1., 0.35));
        let position = |floating: bool| -> Element<'a, Message> {
            column::with_children(vec![
                dock_drawing(bars.dock_shape, 5.),
                cosmic::iced::widget::space()
                    .height(if floating { 5. } else { 0. })
                    .into(),
                edge(),
            ])
            .align_x(Alignment::Center)
            .into()
        };
        let dock = vec![
            words("Dock", 15., INK),
            line("Shape", shapes(bars.dock_shape, "dock", Step::DockShape)),
            row::with_children(vec![
                line(
                    "Attachment",
                    vec![
                        choice(
                            position(true),
                            "Floating",
                            "wizard-dock-floating".into(),
                            bars.floating,
                            Some(go(Step::Floating(true))),
                        ),
                        choice(
                            position(false),
                            "Attached",
                            "wizard-dock-attached".into(),
                            !bars.floating,
                            Some(go(Step::Floating(false))),
                        ),
                    ],
                ),
                line(
                    "Visibility",
                    vec![
                        choice(
                            dock_drawing(bars.dock_shape, 5.),
                            "Shown",
                            "wizard-dock-shown".into(),
                            !bars.hides,
                            Some(go(Step::Hides(false))),
                        ),
                        choice(
                            column::with_children(vec![words("↓", 13., SOFT), edge()])
                                .align_x(Alignment::Center)
                                .into(),
                            "Hides",
                            "wizard-dock-hides".into(),
                            bars.hides,
                            Some(go(Step::Hides(true))),
                        ),
                    ],
                ),
            ])
            .spacing(12)
            .into(),
        ];
        let controls = vec![
            column::with_children(panel)
                .spacing(10)
                .width(column_width)
                .into(),
            column::with_children(dock)
                .spacing(10)
                .width(column_width)
                .into(),
        ];
        let controls = row::with_children(controls).spacing(18).into();
        let mut parts = vec![
            self.wizard_bars_preview(wizard),
            self.help(
                "wizard-bars-options", "Selected setup preview",
                "Islands needs to be enabled for this session. You can fine-tune size and spacing later in Desktop → Dock & panel, under More options.",
            ),
            controls,
        ];
        if bars.islands && !self.desktop.islands.installation.installed() {
            parts.push(words("Islands is selected; the preview shows the standard panel fallback. Check the add-on in App settings after setup.", 12., DANGER));
        }
        column::with_children(parts).spacing(10).into()
    }

    /// The chosen look's wallpaper, full width, with the panel on top and
    /// the dock below as they will look.
    fn wizard_bars_preview<'a>(&'a self, wizard: &'a Wizard) -> Element<'a, Message> {
        let mut bars = wizard.bars;
        bars.islands &= self.desktop.islands.installation.installed();
        let w = self.wizard_inner_width();
        // What the window height leaves above the two choice columns.
        let h = (self.window.height - 520.)
            .min(w * 0.24)
            .clamp(80., 190.)
            .floor();
        let background: Element<'a, Message> = match band(wizard.look, w / h) {
            Some(handle) => cosmic::widget::image(handle)
                .width(w)
                .height(h)
                .content_fit(cosmic::iced::ContentFit::Fill)
                .into(),
            None => shape(w, h, 0., Color::from_rgb(0.08, 0.08, 0.2)),
        };
        let panel_gap = if bars.islands { 6. } else { 0. };
        let overlay = column::with_children(vec![
            container(panel_drawing(
                bars.islands,
                bars.panel_shape,
                w - 2. * panel_gap - 16.,
                18.,
            ))
            .width(Length::Fill)
            .padding([panel_gap, 8. + panel_gap])
            .align_x(Alignment::Center)
            .into(),
            cosmic::iced::widget::space().height(Length::Fill).into(),
            container(if bars.hides {
                shape(60., 4., 2., Color::from_rgba(1., 1., 1., 0.5))
            } else {
                dock_drawing(bars.dock_shape, 9.)
            })
            .width(Length::Fill)
            .align_x(Alignment::Center)
            .padding([0., 0., if bars.floating { 8. } else { 0. }, 0.])
            .into(),
        ])
        .width(w)
        .height(h);
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
        .clip(true)
        .into()
    }

    fn wizard_weather(&self) -> Element<'_, Message> {
        let state = &self.city_search;
        let mut parts = Vec::new();
        if let Some(location) = &self.config.location {
            parts.push(words(format!("Current: {}", location.name), 14., INK));
        }
        let search = text_input("City name", &state.query)
            .id(cosmic::widget::Id::new("wizard-city"))
            .on_input(Message::CityInput)
            .on_submit(|_| Message::SearchCity)
            .width(Length::Fill);
        parts.push(search.into());
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
        column::with_children(parts).spacing(10).into()
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
            .unwrap_or_else(|| "Unchanged".into());
        let panel = format!(
            "{} {}, {} dock",
            bars.panel_shape.title(),
            if bars.islands { "islands" } else { "bar" },
            bars.dock_shape.title().to_lowercase()
        );
        let city = self
            .config
            .location
            .as_ref()
            .map_or_else(|| "Not set".to_string(), |location| location.name.clone());
        let summary = |name: &'static str, value: String, target: Screen| -> Element<'a, Message> {
            fill_box(
                row::with_children(vec![
                    container(words(name, 14., INK))
                        .width(Length::FillPortion(1))
                        .into(),
                    container(words(value, 14., SOFT))
                        .width(Length::FillPortion(2))
                        .into(),
                    button::custom(words("Change", 13., LAVENDER))
                        .class(button_style(
                            Color::TRANSPARENT,
                            Color::from_rgba(1., 1., 1., 0.06),
                            Color::TRANSPARENT,
                            8.,
                        ))
                        .padding([4, 8])
                        .on_press(go(Step::Show(target)))
                        .into(),
                ])
                .spacing(12)
                .align_y(Alignment::Center)
                .into(),
                TILE,
                EDGE,
                10.,
            )
            .padding([8, 14])
            .width(Length::Fill)
            .into()
        };
        let mut parts = vec![
            summary("Starter setup", look, Screen::Look),
            summary("Dock and panel", panel, Screen::Bars),
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
        if wizard.widgets.selected(Widget::Weather) && self.config.location.is_some() {
            parts.push(summary("Weather", city, Screen::Weather));
        }
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
                        "After setup: {}. These are optional; find them in My widgets.",
                        pending.join(" · ")
                    ),
                    12.,
                    SOFT,
                ))
                .id(cosmic::widget::Id::new("wizard-followups"))
                .into(),
            );
        }
        if !wizard.widgets.active().is_empty() {
            parts.push(
                fill_box(
                    row::with_children(vec![
                        container(words("Start at sign-in", 14., INK))
                            .width(Length::Fill)
                            .into(),
                        toggler(wizard.startup)
                            .on_toggle(|on| go(Step::Startup(on)))
                            .into(),
                    ])
                    .align_y(Alignment::Center)
                    .into(),
                    TILE,
                    EDGE,
                    10.,
                )
                .id(cosmic::widget::Id::new("wizard-startup"))
                .padding([10, 14])
                .width(Length::Fill)
                .into(),
            );
        }
        super::adaptive::columns(
            column::with_children(vec![
                words("Sample desktop", 12., SOFT),
                self.wizard_widget_preview(wizard, 210.),
                self.usage_control(),
            ])
            .spacing(12)
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
