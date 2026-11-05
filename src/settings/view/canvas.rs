use super::*;
use crate::settings::canvas::Message as Edit;

fn action(edit: Edit) -> Message {
    Message::Canvas(edit)
}

impl Settings {
    pub(super) fn canvas_page(&self) -> Element<'_, Message> {
        column::with_children(vec![
            heading(
                "Arrange widgets",
                "Drag a widget to move it. Click one to see its size and place.",
            ),
            self.canvas_body(),
        ])
        .spacing(12)
        .into()
    }
    /// Every widget on the desktop as a full-width row, so one can be
    /// picked without the mouse.
    fn canvas_objects(&self) -> Vec<Element<'_, Message>> {
        let mut objects: Vec<Element<'_, Message>> = Widget::ALL
            .into_iter()
            .filter(|w| self.config.widget_visible(*w))
            .map(|w| {
                button::custom(
                    row::with_children(vec![
                        icon::from_name(w.icon()).size(16).icon().into(),
                        text::body(w.title()).into(),
                    ])
                    .spacing(10)
                    .align_y(Alignment::Center),
                )
                .class(nav_style(
                    self.placement.selected() == Some(w),
                    self.settings_accent(),
                ))
                .padding([6, 8])
                .width(Length::Fill)
                .id(cosmic::widget::Id::new(format!(
                    "canvas-object-{}",
                    w.key()
                )))
                .on_press(Message::Placement(
                    crate::settings::placement::Message::Select(w.key().into()),
                ))
                .into()
            })
            .collect();
        if objects.is_empty() {
            objects.push(label("Turn on a widget to place it here.", 13., MUTED));
        }
        objects
    }
    /// The placement picture with its inspector, shared by Arrange and Position.
    pub(super) fn canvas_body(&self) -> Element<'_, Message> {
        self.inspector_layout(
            // On its own page the picture leaves room for the title, so
            // the whole arranging view fits without scrolling.
            column::with_children(vec![self.placement_editor(if self.page == Page::Overview {
                (self.stage_height() - 90.).max(200.)
            } else {
                self.stage_height()
            })])
                .spacing(10)
                .width(Length::Fill)
                .into(),
            self.canvas_inspector(),
            "canvas",
        )
    }
    fn canvas_inspector(&self) -> Element<'_, Message> {
        let mut parts = vec![group(Some("Widgets"), self.canvas_objects())];
        parts.push(group(
            Some("Size and place"),
            vec![if self.placement.selected().is_some() {
                self.placement_inspector()
            } else {
                label("Pick a widget to change its size and place", 13., MUTED)
            }],
        ));
        column::with_children(parts).spacing(14).into()
    }
    pub(super) fn canvas_actions(&self) -> Element<'_, Message> {
        let mut parts = Vec::new();
        if self.error {
            parts.push(label(&self.status, 12., Color::from_rgb8(245, 149, 138)));
        }
        parts.push(
            row::with_children(vec![
                container(label(
                    if self.native_busy {
                        "Saving…"
                    } else if self.canvas_dirty() {
                        "Press Done to keep the new layout"
                    } else {
                        ""
                    },
                    12.,
                    MUTED,
                ))
                .width(Length::Fill)
                .into(),
                button::text("Undo")
                    .id(cosmic::widget::Id::new("canvas-undo"))
                    .leading_icon(icon::from_name("edit-undo-symbolic").size(14))
                    .on_press_maybe(
                        (self.canvas_can_undo() && !self.native_busy).then_some(action(Edit::Undo)),
                    )
                    .into(),
                button::text("Cancel")
                    .id(cosmic::widget::Id::new("canvas-cancel"))
                    .on_press_maybe((!self.native_busy).then_some(action(Edit::Cancel)))
                    .into(),
                button::suggested("Done")
                    .id(cosmic::widget::Id::new("canvas-apply"))
                    .on_press_maybe(self.canvas_done())
                    .into(),
            ])
            .spacing(8)
            .align_y(Alignment::Center)
            .into(),
        );
        container(column::with_children(parts).spacing(6))
            .padding([10, 18])
            .width(Length::Fill)
            .into()
    }
}
