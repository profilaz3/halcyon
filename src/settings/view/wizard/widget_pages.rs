use super::*;
use crate::drawer::Widget;
use cosmic::iced::widget::pin;
use cosmic::widget::{dropdown, responsive};

fn description(widget: Widget) -> &'static str {
    match widget {
        Widget::Clock => "Time & date",
        Widget::Weather => "Your local forecast",
        Widget::Calendar => "Connect later",
        Widget::Devbox => "CPU, memory & more",
        Widget::Notes => "A space for ideas",
        Widget::Focus => "Time to concentrate",
        Widget::Music => "Your music player",
        Widget::Projects => "Choose folders later",
        Widget::Glance => "Clock, weather & calendar",
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
        let mut controls = Vec::new();
        if arrange {
            controls.push(words(
                format!("Your desktop order · {} widgets", plan.visible.len()),
                13.,
                INK,
            ));
            let mut list = Vec::new();
            for (index, widget) in plan.visible.iter().copied().enumerate() {
                let key = widget.key();
                let selected = wizard.widgets.focused.as_deref() == Some(key);
                let mut up = button::icon(icon::from_name("go-up-symbolic").size(16))
                    .id(cosmic::widget::Id::new(format!("wizard-up-{key}")))
                    .on_press_maybe((index > 0).then(|| go(Step::MoveWidget(key.into(), -1))));
                up = up.tooltip("Move up");
                let down = button::icon(icon::from_name("go-down-symbolic").size(16))
                    .id(cosmic::widget::Id::new(format!("wizard-down-{key}")))
                    .tooltip("Move down")
                    .on_press_maybe(
                        (index + 1 < plan.visible.len())
                            .then(|| go(Step::MoveWidget(key.into(), 1))),
                    );
                let grip = mouse_area(
                    container(icon::from_name("view-list-symbolic").size(18))
                        .id(cosmic::widget::Id::new(format!("wizard-grip-{key}")))
                        .padding([8, 2]),
                )
                .on_press(go(Step::DragWidget(key.into())))
                .interaction(cosmic::iced::mouse::Interaction::Grab);
                let select = button::custom(
                    column::with_children(vec![
                        words(format!("{}  {}", index + 1, widget.title()), 13., INK),
                        words(description(widget), 11., SOFT),
                    ])
                    .spacing(3),
                )
                .id(cosmic::widget::Id::new(format!("wizard-order-{key}")))
                .class(cosmic::theme::Button::Text) // native focus and keyboard activation
                .padding([6, 4])
                .width(Length::Fill)
                .on_press(go(Step::SelectWidget(key.into())));
                let content =
                    row::with_children(vec![grip.into(), select.into(), up.into(), down.into()])
                        .spacing(4)
                        .align_y(Alignment::Center);
                let row = fill_box(
                    content.into(),
                    TILE,
                    if selected || wizard.widgets.over.as_deref() == Some(key) {
                        LAVENDER
                    } else {
                        Color::TRANSPARENT
                    },
                    10.,
                )
                .padding([4, 6])
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
            controls.push(
                scrollable(column::with_children(list).spacing(8))
                    .id(cosmic::widget::Id::new("wizard-widget-list"))
                    .height(available.min(288.))
                    .into(),
            );
            if !plan.visible.is_empty() {
                controls.push(
                    row::with_children(vec![
                        button::text("Use setup order")
                            .on_press(go(Step::SetupWidgetOrder))
                            .into(),
                        button::text("Undo move")
                            .id(cosmic::widget::Id::new("wizard-widget-undo"))
                            .on_press_maybe(
                                wizard
                                    .widgets
                                    .can_undo()
                                    .then_some(go(Step::UndoWidgetMove)),
                            )
                            .into(),
                    ])
                    .spacing(6)
                    .into(),
                );
                controls.push(words(
                    "Top to bottom, then the next column.\nSelect a row · Alt + ↑ / ↓ to move it",
                    11.,
                    SOFT,
                ));
            }
        } else {
            // One compact list keeps all eight choices visible at the normal
            // window size. Section membership is shown before readiness.
            let pick = |widget: Widget| -> Element<'a, Message> {
                let on = wizard.widgets.selected(widget);
                let grouped = wizard.widgets.grouped
                    && matches!(widget, Widget::Clock | Widget::Weather | Widget::Calendar);
                button::custom(
                    row::with_children(vec![
                        icon::from_name(widget.icon()).size(16).into(),
                        container(words(widget.title(), 13., INK))
                            .width(Length::Fill)
                            .into(),
                        words(
                            if on { "✓" } else { "○" },
                            14.,
                            if on { LAVENDER } else { SOFT },
                        ),
                    ])
                    .spacing(8)
                    .align_y(Alignment::Center),
                )
                .id(cosmic::widget::Id::new(format!(
                    "wizard-widget-{}",
                    widget.key()
                )))
                .class(button_style(
                    TILE,
                    TILE,
                    if on { LAVENDER } else { EDGE },
                    8.,
                ))
                .padding([4, 8, 4, if grouped { 20 } else { 8 }])
                .width(Length::Fill)
                .on_press(go(Step::Widget(widget)))
                .into()
            };
            let mut choices = vec![
                row::with_children(vec![
                    container(words("Daily overview", 13., INK))
                        .width(Length::Fill)
                        .into(),
                    container(
                        toggler(wizard.widgets.grouped).on_toggle(|on| go(Step::GroupWidgets(on))),
                    )
                    .id(cosmic::widget::Id::new("wizard-widget-group"))
                    .into(),
                ])
                .spacing(8)
                .align_y(Alignment::Center)
                .into(),
                words(
                    if wizard.widgets.grouped {
                        "Clock, weather & calendar share one widget"
                    } else {
                        "Clock, weather & calendar are separate"
                    },
                    11.,
                    SOFT,
                ),
            ];
            choices.extend(
                [Widget::Clock, Widget::Weather, Widget::Calendar]
                    .into_iter()
                    .map(pick),
            );
            choices.push(words("More widgets", 11., SOFT));
            choices.extend(
                [
                    Widget::Devbox,
                    Widget::Notes,
                    Widget::Focus,
                    Widget::Music,
                    Widget::Projects,
                ]
                .into_iter()
                .map(pick),
            );
            controls.push(column::with_children(choices).spacing(4).into());
        }
        if !arrange && wizard.widgets.customized && wizard.look.is_some() {
            controls.push(
                button::text("Use setup widgets")
                    .on_press(go(Step::UseSetupWidgets))
                    .into(),
            );
        }
        let mut preview = vec![
            words("Desktop preview · Sample content", 12., SOFT),
            self.wizard_widget_preview(wizard, if compact { 180. } else { 270. }),
            words(
                format!(
                    "{} selected · {} desktop widgets",
                    wizard.widgets.selected.len(),
                    wizard.widgets.active().len()
                ),
                12.,
                SOFT,
            ),
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
        if let Some(error) = plan.error {
            // Arrange shows this next to its fixed footer action.
            if !arrange {
                preview.push(words(error, 13., DANGER));
            }
        } else if !plan.visible.is_empty() {
            preview.push(words(
                format!("{} widgets on this display", plan.visible.len()),
                12.,
                SOFT,
            ));
        }
        super::super::adaptive::columns(
            container(column::with_children(preview).spacing(10))
                .width(Length::Fill)
                .into(),
            container(column::with_children(controls).spacing(10))
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
                            fill_box(words((index + 1).to_string(), 9., INK), TILE, EDGE, 4.)
                                .padding([1, 3]),
                        )
                        .position(cosmic::iced::Point::new(
                            g.position[0] * scale,
                            (g.position[1] * scale - 12.).max(0.),
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
                .align_y(Alignment::Center)
                .into()
        });
        container(scene).height(height).width(Length::Fill).into()
    }
}
