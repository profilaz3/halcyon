use super::*;
use crate::settings::support::{Health, Message as Edit};

fn action<'a>(title: &'static str, message: Edit) -> Element<'a, Message> {
    button::text(title)
        .on_press(Message::Support(message))
        .into()
}
impl Settings {
    pub(super) fn help_page(&self) -> Element<'_, Message> {
        if let Some(report) = &self.support.report {
            return column::with_children(vec![
                action("‹ Help and diagnostics", Edit::Preview(false)),
                label("Support report", 20., FG),
                label(
                    "Review what you share. Nothing is sent automatically.",
                    14.,
                    MUTED,
                ),
                pages::section(vec![label(report.clone(), 14., FG)]),
                row::with_children(vec![
                    button::suggested("Copy report")
                        .id(cosmic::widget::Id::new("help-copy-report"))
                        .on_press(Message::Support(Edit::Copy))
                        .into(),
                    button::standard("Save report…")
                        .on_press(Message::Support(Edit::Save))
                        .into(),
                    button::text("Open the issue page ↗")
                        .on_press(Message::Support(Edit::ReportProblem))
                        .into(),
                ])
                .spacing(8)
                .into(),
                label(self.support.notice.unwrap_or(""), 12., MUTED),
            ])
            .spacing(16)
            .into();
        }
        let snapshot = self.help_snapshot();
        let mut items = vec![group(
            None,
            vec![
                group_row(
                    "Report a problem",
                    "You'll see exactly what's included before anything is sent",
                    button::standard("Start")
                        .id(cosmic::widget::Id::new("help-preview-report"))
                        .on_press(Message::Support(Edit::Preview(true)))
                        .into(),
                ),
            ],
        )];
        let accent = self.settings_accent();
        let mut rows = Vec::new();
        for c in &snapshot.components {
            let color = match c.health {
                Health::Healthy => Color::from_rgb(0.45, 0.80, 0.55),
                Health::Attention => super::desktop::WARN,
                _ => MUTED,
            };
            let mut parts = vec![dot(color, 8.), detail_state(c.name, c.state)];
            if c.health == Health::Healthy {
                rows.push(
                    container(
                        row::with_children(parts)
                            .spacing(12)
                            .align_y(Alignment::Center),
                    )
                    .padding([4, 0])
                    .into(),
                );
            } else {
                parts.push(icon::from_name("go-next-symbolic").size(16).icon().into());
                rows.push(
                    button::custom(
                        row::with_children(parts)
                            .spacing(12)
                            .align_y(Alignment::Center),
                    )
                    .class(nav_style(false, accent))
                    .padding([4, 0])
                    .width(Length::Fill)
                    .on_press(Message::Support(Edit::Open(c.action)))
                    .into(),
                );
            }
        }
        items.push(group(Some("How things are running"), rows));
        items.push(group(None, vec![group_row(
            "Check again",
            "Look at everything once more",
            button::standard(if self.support.loading {
                "Checking…"
            } else {
                "Check"
            })
            .on_press_maybe((!self.support.loading).then_some(Message::Support(Edit::Refresh)))
            .into(),
        )]));
        column::with_children(items).spacing(16).into()
    }
}
