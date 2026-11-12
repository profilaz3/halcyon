use super::*;
use crate::drawer::Widget;
use cosmic::iced::widget::{pin, stack};
use cosmic::iced::{Length, Point};
use cosmic::widget::{button, column, container, responsive, row, text, toggler};

impl Settings {
    pub(super) fn save_widget_monitor(
        &mut self,
        widget: Widget,
        field: &str,
        value: serde_json::Value,
    ) {
        if super::navigation::widget_of(self.page).is_some()
            && self.object_tab == super::navigation::ObjectTab::Position
        {
            self.preview_widget_monitor(widget, field, value);
            return;
        }
        self.placement.leave();
        if self.placement.dirty() {
            let _ = self.update_placement(super::placement::Message::Save);
        }
        if self.placement.dirty() {
            return;
        }
        match Config::save_monitor_field(widget, field, value) {
            Ok(config) => {
                self.config = config;
                self.saved();
                self.placement.refresh(&self.config);
            }
            Err(error) => self.failed(error),
        }
    }
    pub(super) fn monitor_overview(&self) -> Element<'_, Message> {
        if self.displays.outputs().len() < 2 {
            return self.display_selector();
        }
        let outputs: Vec<_> = self
            .displays
            .outputs()
            .iter()
            .filter_map(|o| Some((o, self.displays.key(o)?)))
            .collect();
        let selected = self
            .placement
            .preview_snapshot()
            .display
            .as_ref()
            .and_then(|d| d.output.as_ref());
        let map = responsive(move |available| {
            let bounds: Vec<_> = outputs
                .iter()
                .map(|(o, _)| {
                    let (x, y) = o.info.logical_position.unwrap_or(o.info.location);
                    let (w, h) = o.info.logical_size.unwrap_or((1920, 1080));
                    (x as f32, y as f32, w.max(1) as f32, h.max(1) as f32)
                })
                .collect();
            let minx = bounds.iter().map(|b| b.0).fold(f32::INFINITY, f32::min);
            let miny = bounds.iter().map(|b| b.1).fold(f32::INFINITY, f32::min);
            let maxx = bounds.iter().map(|b| b.0 + b.2).fold(0., f32::max);
            let maxy = bounds.iter().map(|b| b.1 + b.3).fold(0., f32::max);
            let scale = ((available.width - 8.).max(1.) / (maxx - minx).max(1.))
                .min(94. / (maxy - miny).max(1.));
            let screens: Vec<Element<'_, Message>> = outputs
                .iter()
                .zip(bounds)
                .enumerate()
                .map(|(i, ((o, key), (x, y, w, h)))| {
                    let content = container(
                        row::with_children(vec![
                            text::title3((i + 1).to_string()).into(),
                            text::caption(o.display_name()).into(),
                        ])
                        .spacing(6)
                        .align_y(cosmic::iced::Alignment::Center),
                    )
                    .center(Length::Fill);
                    let tile = button::custom(content)
                        .class(monitor_style(
                            self.config.palette().accent,
                            selected == Some(key),
                        ))
                        .padding(8)
                        .width((w * scale - 6.).max(1.))
                        .height((h * scale - 6.).max(1.))
                        .on_press(Message::MonitorPreview(key.clone()))
                        .id(cosmic::widget::Id::new(format!("monitor-preview-{key}")));
                    pin(tile)
                        .position(Point::new((x - minx) * scale, (y - miny) * scale))
                        .into()
                })
                .collect();
            stack(screens).width(Length::Fill).height(100).into()
        });
        column::with_children(vec![
            row::with_children(vec![
                text::body("Displays").into(),
                container(text::body("")).width(Length::Fill).into(),
                button::text("Identify")
                    .on_press(Message::IdentifyDisplays)
                    .into(),
                button::text("Panel")
                    .on_press(Message::Page(Page::DesktopPanel))
                    .into(),
                button::text("Dock")
                    .on_press(Message::Page(Page::DesktopDock))
                    .into(),
            ])
            .spacing(8)
            .into(),
            map.into(),
            self.compact_default_display(),
        ])
        .spacing(6)
        .into()
    }
    fn compact_default_display(&self) -> Element<'_, Message> {
        let mut controls: Vec<Element<'_, Message>> = vec![text::caption("Default").into()];
        for (title, value) in self.displays.choices(self.config.widget_display.as_deref()) {
            let title = value
                .as_deref()
                .and_then(|v| self.displays.unique(v))
                .and_then(|o| o.name())
                .map(str::to_owned)
                .unwrap_or(title);
            let b = if self
                .displays
                .same_choice(value.as_deref(), self.config.widget_display.as_deref())
            {
                button::suggested(title)
            } else {
                button::standard(title)
            };
            let enabled = value
                .as_deref()
                .is_none_or(|v| self.displays.unique(v).is_some());
            let id = format!("widget-display-{}", value.as_deref().unwrap_or("auto"));
            controls.push(
                b.id(cosmic::widget::Id::new(id))
                    .on_press_maybe(enabled.then_some(Message::WidgetDisplay(value)))
                    .into(),
            );
        }
        row::with_children(controls).spacing(6).wrap().into()
    }
    pub(super) fn widget_display_controls(&self, widget: Widget) -> Element<'_, Message> {
        let choice = self
            .canvas_config()
            .widget_monitors
            .get(widget.key())
            .cloned()
            .unwrap_or_default();
        let mut buttons: Vec<Element<'_, Message>> = self
            .displays
            .choices(choice.display.as_deref())
            .into_iter()
            .map(|(title, value)| {
                let title = if value.is_none() {
                    "Default".into()
                } else {
                    value
                        .as_deref()
                        .and_then(|v| self.displays.unique(v))
                        .and_then(|o| o.name())
                        .map(str::to_owned)
                        .unwrap_or(title)
                };
                let button = if self
                    .displays
                    .same_choice(value.as_deref(), choice.display.as_deref())
                {
                    button::suggested(title)
                } else {
                    button::standard(title)
                };
                let enabled = value
                    .as_deref()
                    .is_none_or(|v| self.displays.unique(v).is_some());
                button
                    .on_press_maybe(
                        enabled.then_some(Message::WidgetMonitor(widget, value.clone())),
                    )
                    .id(cosmic::widget::Id::new(format!(
                        "widget-monitor-{}-{}",
                        widget.key(),
                        value.as_deref().unwrap_or("default")
                    )))
                    .into()
            })
            .collect();
        buttons.push(
            toggler(choice.hide_disconnected)
                .label(String::from("Hide if disconnected"))
                .on_toggle(move |v| Message::WidgetMonitorHide(widget, v))
                .into(),
        );
        row::with_children(buttons)
            .spacing(6)
            .align_y(cosmic::iced::Alignment::Center)
            .wrap()
            .into()
    }
    pub(super) fn identify_displays(&mut self) -> Task<Message> {
        use cosmic::iced::platform_specific::runtime::wayland::layer_surface::{
            IcedOutput, SctkLayerSurfaceSettings,
        };
        use cosmic::iced::platform_specific::shell::commands::layer_surface::*;
        let mut tasks = Vec::new();
        for (i, o) in self.displays.outputs().iter().enumerate() {
            let id = window::Id::unique();
            self.identified_displays
                .push((id, format!("{} · {}", i + 1, o.title())));
            tasks.push(get_layer_surface(SctkLayerSurfaceSettings {
                id,
                output: IcedOutput::Output(o.handle.clone()),
                layer: Layer::Overlay,
                keyboard_interactivity: KeyboardInteractivity::None,
                anchor: Anchor::TOP,
                size: Some((Some(420), Some(85))),
                input_zone: Some(Vec::new()),
                exclusive_zone: 0,
                namespace: "halcyon-identify".into(),
                ..Default::default()
            }));
        }
        let ids: Vec<_> = self.identified_displays.iter().map(|(id, _)| *id).collect();
        tasks.push(cosmic::task::future(async move {
            tokio::time::sleep(std::time::Duration::from_secs(3)).await;
            Message::ClearDisplayLabels(ids)
        }));
        Task::batch(tasks)
    }
}

fn monitor_style(accent: cosmic::iced::Color, selected: bool) -> cosmic::theme::Button {
    let style = move |focused: bool, hovered: bool| cosmic::widget::button::Style {
        background: Some(cosmic::iced::Background::Color(cosmic::iced::Color {
            a: if selected { 0.22 } else { 0.07 },
            ..accent
        })),
        border_color: if selected || focused || hovered {
            accent
        } else {
            cosmic::iced::Color { a: 0.3, ..accent }
        },
        border_width: if selected || focused { 2. } else { 1. },
        border_radius: 8.into(),
        ..Default::default()
    };
    cosmic::theme::Button::Custom {
        active: Box::new(move |focus, _| style(focus, false)),
        hovered: Box::new(move |focus, _| style(focus, true)),
        pressed: Box::new(move |focus, _| style(focus, true)),
        disabled: Box::new(move |_| style(false, false)),
    }
}
