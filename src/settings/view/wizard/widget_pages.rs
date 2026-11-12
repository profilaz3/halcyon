use super::*;
use crate::drawer::Widget;
use cosmic::iced::widget::pin;
use cosmic::widget::{dropdown, responsive};

/// The Weather city row, its title in line with the widget titles above.
fn split_row<'a>(
    title: &'static str,
    description: &'static str,
    control: Element<'a, Message>,
) -> Element<'a, Message> {
    row::with_children(vec![
        icon::from_name("mark-location-symbolic").size(16).into(),
        container(detail(title, description)).width(Length::Fill).into(),
        control,
    ])
    .spacing(12)
    .align_y(Alignment::Center)
    .into()
}

fn description(widget: Widget) -> &'static str {
    match widget {
        Widget::Clock => "Time and date",
        Widget::Weather => "Your local forecast",
        Widget::Calendar => "Today's events",
        Widget::Devbox => "CPU, memory and more",
        Widget::Notes => "A space for ideas",
        Widget::Focus => "Time to concentrate",
        Widget::Music => "Your music player",
        Widget::Projects => "Your recent folders",
        Widget::Glance => "Clock, weather and calendar",
        Widget::Devices => "Battery of connected devices",
    }
}
impl Settings {
    pub(super) fn wizard_widgets<'a>(
        &'a self,
        wizard: &'a Wizard,
        arrange: bool,
    ) -> Element<'a, Message> {
        let plan = wizard.widget_plan(&self.config);
        let compact = self.wizard_inner_width() < 900.;
        let available = (self.window.height - 330.).clamp(200., 400.);
        let mut controls = vec![self.wizard_widget_tabs(wizard.step)];
        if arrange {
            let mut list = Vec::new();
            for (index, widget) in plan.visible.iter().copied().enumerate() {
                let key = widget.key();
                let selected = wizard.widgets.focused.as_deref() == Some(key);
                // An arrow that can't move the row is dimmed, not hidden.
                let arrow = |name: &'static str, id: String, tip: &'static str, message: Option<Message>| -> Element<'a, Message> {
                    match message {
                        Some(message) => button::icon(icon::from_name(name).size(16))
                            .id(cosmic::widget::Id::new(id))
                            .tooltip(tip)
                            .on_press(message)
                            .into(),
                        None => container(
                            icon::from_name(name).size(16).icon().class(cosmic::theme::Svg::custom(|_| {
                                cosmic::iced::widget::svg::Style {
                                    color: Some(Color::from_rgb(0.36, 0.36, 0.38)),
                                }
                            })),
                        )
                        .id(cosmic::widget::Id::new(id))
                        .padding(8)
                        .into(),
                    }
                };
                let up = arrow(
                    "go-up-symbolic",
                    format!("wizard-up-{key}"),
                    "Move up",
                    (index > 0).then(|| go(Step::MoveWidget(key.into(), -1))),
                );
                let down = arrow(
                    "go-down-symbolic",
                    format!("wizard-down-{key}"),
                    "Move down",
                    (index + 1 < plan.visible.len()).then(|| go(Step::MoveWidget(key.into(), 1))),
                );
                let grip = mouse_area(
                    container(icon::from_name("grip-lines-symbolic").size(16).icon().class(
                        cosmic::theme::Svg::custom(|_| cosmic::iced::widget::svg::Style {
                            color: Some(Color::from_rgb(0.5, 0.5, 0.54)),
                        }),
                    ))
                        .id(cosmic::widget::Id::new(format!("wizard-grip-{key}")))
                        .padding([8, 2]),
                )
                .on_press(go(Step::DragWidget(key.into())))
                .interaction(cosmic::iced::mouse::Interaction::Grab);
                let select = button::custom(
                    row::with_children(vec![
                        fill_box(
                            container(words(
                                (index + 1).to_string(),
                                11.,
                                Color::from_rgb(0.04, 0.05, 0.1),
                            ))
                            .center_x(20.)
                            .center_y(18.)
                            .into(),
                            PRIMARY,
                            Color::TRANSPARENT,
                            9.,
                        )
                        .width(20.)
                        .height(18.)
                        .into(),
                        icon::from_name(widget.icon()).size(16).into(),
                        detail(widget.title(), description(widget)),
                    ])
                    .spacing(10)
                    .align_y(Alignment::Center),
                )
                .id(cosmic::widget::Id::new(format!("wizard-order-{key}")))
                .class(nav_style(false, self.settings_accent())) // native focus and keyboard activation
                .padding([2, 4])
                .width(Length::Fill)
                .on_press(go(Step::SelectWidget(key.into())));
                let content =
                    row::with_children(vec![grip.into(), select.into(), up, down])
                        .spacing(4)
                        .align_y(Alignment::Center);
                let lit = selected || wizard.widgets.over.as_deref() == Some(key);
                let row = container(content)
                    .class(cosmic::theme::Container::custom(move |theme| {
                        cosmic::iced::widget::container::Style {
                            background: lit.then(|| {
                                let accent: Color = theme.cosmic().accent.base.into();
                                Background::Color(Color { a: 0.14, ..accent })
                            }),
                            border: Border {
                                radius: 8.0.into(),
                                ..Default::default()
                            },
                            ..Default::default()
                        }
                    }))
                    .padding([0, 2])
                    .width(Length::Fill);
                list.push(
                    mouse_area(row)
                        .on_enter(go(Step::OverWidget(key.into())))
                        .on_exit(go(Step::LeaveWidget(key.into())))
                        .into(),
                );
            }
            if list.is_empty() {
                list.push(words(
                    if plan.elsewhere.is_empty() {
                        "No widgets selected. You can add them later in Settings."
                    } else {
                        "Your selected widgets are on other displays."
                    },
                    14.,
                    SOFT,
                ));
            }
            let _ = available;
            controls.push(
                container(group(None, list))
                    .id(cosmic::widget::Id::new("wizard-widget-list"))
                    .into(),
            );
            if !plan.visible.is_empty() {
                controls.push(
                    row::with_children(vec![
                        container(words(
                            "Drag or use the arrows. Numbers match the preview.",
                            12.,
                            SOFT,
                        ))
                            .width(Length::Fill)
                            .into(),
                        button::text("Reset order")
                            .on_press(go(Step::SetupWidgetOrder))
                            .into(),
                    ])
                    .align_y(Alignment::Center)
                    .into(),
                );
            }
        } else {
            // The same rows as Settings: icon, name and a switch.
            let pick = |widget: Widget| -> Element<'a, Message> {
                let on = wizard.widgets.selected(widget);
                row::with_children(vec![
                    icon::from_name(widget.icon()).size(16).into(),
                    container(detail(widget.title(), description(widget)))
                        .width(Length::Fill)
                        .into(),
                    container(toggler(on).on_toggle(move |_| go(Step::Widget(widget))))
                        .id(cosmic::widget::Id::new(format!("wizard-widget-{}", widget.key())))
                        .into(),
                ])
                .spacing(12)
                .align_y(Alignment::Center)
                .into()
            };
            let mut main = vec![pick(Widget::Clock), pick(Widget::Weather)];
            // The city sits under Weather, only while it matters.
            if wizard.widgets.selected(Widget::Weather) {
                if self.config.location.is_none() && wizard.city.is_none() {
                    main.push(split_row(
                        "Weather city",
                        "Used for your forecast",
                        container(
                            text_input("City name", &self.city_search.query)
                                .id(cosmic::widget::Id::new("wizard-city"))
                                .on_input(Message::CityInput)
                                .on_submit(|_| Message::SearchCity),
                        )
                        .width(190.)
                        .into(),
                    ));
                    if let Some(results) = self.wizard_city_results() {
                        main.push(results);
                    }
                } else if let Some(city) = &wizard.city {
                    main.push(detail("Weather city", city.name.clone()));
                }
            }
            main.push(pick(Widget::Calendar));
            main.push(
                row::with_children(vec![
                    icon::from_name(Widget::Glance.icon()).size(16).into(),
                    container(detail(
                        "Combine into a Daily overview card",
                        "Clock, weather and calendar together in one card",
                    ))
                    .width(Length::Fill)
                    .into(),
                    container(
                        toggler(wizard.widgets.grouped)
                            .on_toggle(|on| go(Step::GroupWidgets(on))),
                    )
                    .id(cosmic::widget::Id::new("wizard-widget-group"))
                    .into(),
                ])
                .spacing(12)
                .align_y(Alignment::Center)
                .into(),
            );
            controls.push(group(Some("Everyday"), main));
            controls.push(group(
                Some("More widgets"),
                [
                    Widget::Devbox,
                    Widget::Notes,
                    Widget::Focus,
                    Widget::Music,
                    Widget::Projects,
                ]
                .into_iter()
                .map(pick)
                .collect(),
            ));
        }
        if !arrange && wizard.widgets.customized && wizard.look.is_some() {
            controls.push(
                button::text("Use setup widgets")
                    .on_press(go(Step::UseSetupWidgets))
                    .into(),
            );
        }
        let mut preview = vec![
            self.wizard_widget_preview(wizard, if compact { 180. } else { self.wizard_preview_height() }),
            container(words(CAPTION, 12., SOFT))
                .width(Length::Fill)
                .center_x(Length::Fill)
                .into(),
        ];
        if self.displays.outputs().len() > 1 || !plan.elsewhere.is_empty() {
            preview.push(words("Preview display", 12., INK));
            let choices = self.displays.choices(wizard.widgets.display.as_deref());
            let labels = choices.iter().map(|c| c.0.clone()).collect::<Vec<_>>();
            let selected = choices.iter().position(|c| c.1 == wizard.widgets.display);
            preview.push(
                dropdown(labels, selected, move |i| {
                    go(Step::WidgetDisplay(choices[i].1.clone()))
                })
                .into(),
            );
            if !plan.elsewhere.is_empty() {
                preview.push(words(
                    format!(
                        "Elsewhere: {}",
                        plan.elsewhere
                            .iter()
                            .map(|w| w.title())
                            .collect::<Vec<_>>()
                            .join(", ")
                    ),
                    12.,
                    SOFT,
                ));
                preview.push(
                    button::text("Move selected widgets here")
                        .on_press(go(Step::MoveWidgetsHere))
                        .into(),
                );
            }
        }
        super::super::adaptive::columns(
            container(column::with_children(preview).spacing(10))
                .width(Length::Fill)
                .into(),
            container(column::with_children(controls).spacing(12))
                .width(Length::Fill)
                .into(),
            [300., 340.],
            true,
        )
    }
    pub(super) fn wizard_widget_preview<'a>(
        &'a self,
        wizard: &Wizard,
        height: f32,
    ) -> Element<'a, Message> {
        let plan = wizard.widget_plan(&self.config);
        let wallpaper = wizard
            .look
            .and_then(|id| super::super::profiles::look_image(id, true))
            .or_else(|| self.profiles.home_image().cloned())
            .or_else(|| backdrop(false));
        let selected = wizard.widgets.focused.clone();
        let numbered = wizard.step == Screen::Arrange;
        let bars = wizard.bars;
        let installed = self.desktop.islands.installation.installed();
        let scene = responsive(move |bounds| {
            let scale =
                (bounds.width / plan.snapshot.screen[0]).min(height / plan.snapshot.screen[1]);
            let screen = [
                plan.snapshot.screen[0] * scale,
                plan.snapshot.screen[1] * scale,
            ];
            let bg = match &wallpaper {
                Some(h) => cosmic::widget::image(h.clone())
                    .width(screen[0])
                    .height(screen[1])
                    .content_fit(cosmic::iced::ContentFit::Cover)
                    .into(),
                None => shape(screen[0], screen[1], 0., TILE),
            };
            let mut layers = vec![
                cosmic::widget::space()
                    .width(screen[0])
                    .height(screen[1])
                    .into(),
                bg,
            ];
            for (index, w) in plan.visible.iter().enumerate() {
                let Some(g) = plan.snapshot.widgets.iter().find(|g| g.key == w.key()) else {
                    continue;
                };
                let size = [g.size[0] * scale, g.size[1] * scale];
                let card = super::super::profiles::card(*w, size, &plan.config, scale);
                let card = fill_box(
                    card,
                    Color::TRANSPARENT,
                    if selected.as_deref() == Some(w.key()) {
                        LAVENDER
                    } else {
                        Color::TRANSPARENT
                    },
                    4.,
                )
                .width(size[0])
                .height(size[1]);
                layers.push(
                    pin(card)
                        .position(cosmic::iced::Point::new(
                            g.position[0] * scale,
                            g.position[1] * scale,
                        ))
                        .into(),
                );
                if numbered {
                    layers.push(
                        pin(
                            fill_box(
                                words((index + 1).to_string(), 11., Color::from_rgb(0.04, 0.05, 0.1)),
                                PRIMARY,
                                Color::TRANSPARENT,
                                9.,
                            )
                            .padding([1, 6]),
                        )
                        .position(cosmic::iced::Point::new(
                            g.position[0] * scale,
                            (g.position[1] * scale - 16.).max(0.),
                        ))
                        .into(),
                    );
                }
            }
            layers.push(
                pin(panel_drawing(
                    bars.islands && installed,
                    bars.panel_shape,
                    screen[0] - 16.,
                    10.,
                ))
                .position(cosmic::iced::Point::new(8., 4.))
                .into(),
            );
            if !bars.hides {
                layers.push(
                    pin(dock_drawing(bars.dock_shape, 5.))
                        .position(cosmic::iced::Point::new(
                            screen[0] / 2. - 25.,
                            screen[1] - 19.,
                        ))
                        .into(),
                );
            }
            container(stack(layers).width(screen[0]).height(screen[1]).clip(true))
                .width(Length::Fill)
                .height(height)
                .align_x(Alignment::Center)
                .align_y(Alignment::Start)
                .into()
        });
        container(scene).height(height).width(Length::Fill).into()
    }
}
