use super::*;
impl Cards {
    pub(super) fn event_details_open(&self, event: &CalendarEvent) -> bool {
        !self.config.calendar_private
            && self.calendar_details.as_deref() == Some(event.details_key().as_str())
    }
    pub(super) fn event_details_height(&self, event: &CalendarEvent) -> f32 {
        if self.event_details_open(event) {
            184.
        } else {
            0.
        }
    }
    pub(super) fn event_details_view(
        &self,
        event: &CalendarEvent,
        p: Palette,
    ) -> Element<'_, Message> {
        let mut lines = vec![];
        if !event.calendar_name.is_empty() {
            lines.push(label(event.calendar_name.clone(), 11., p.muted));
        }
        let time = if event.all_day {
            let start = NaiveDate::parse_from_str(&event.start, "%Y-%m-%d").ok();
            let last = NaiveDate::parse_from_str(&event.end, "%Y-%m-%d")
                .ok()
                .and_then(|day| day.pred_opt());
            match (start, last) {
                (Some(start), Some(last)) if last > start => format!(
                    "{} – {} · All day",
                    start.format("%a, %b %-d"),
                    last.format("%a, %b %-d")
                ),
                (Some(start), _) => format!("{} · All day", start.format("%a, %b %-d")),
                _ => "All day".into(),
            }
        } else {
            let local = |s: &str| {
                chrono::DateTime::parse_from_rfc3339(s)
                    .map(|t| {
                        t.with_timezone(&Local)
                            .format("%a, %b %-d, %H:%M")
                            .to_string()
                    })
                    .unwrap_or_default()
            };
            format!("{} – {}", local(&event.start), local(&event.end))
        };
        lines.push(label(time, 12., p.foreground));
        if !event.location.is_empty() {
            lines.push(label(event.location.clone(), 12., p.foreground));
        }
        if !event.description.is_empty() {
            lines.push(label(event.description.clone(), 12., p.foreground));
        }
        let mut actions = vec![action(
            "Close",
            Message::Open(event.details_key()),
            p.accent,
        )];
        if !event.url.is_empty() {
            actions.push(action(
                "Open event ↗",
                Message::Open(event.url.clone()),
                p.accent,
            ));
        }
        if let Some(url) = event.join_url() {
            actions.push(action("Join ↗", Message::Open(url.into()), p.accent));
        }
        container(
            column::with_children(vec![
                cosmic::widget::scrollable(column::with_children(lines).spacing(5))
                    .height(Length::Fill)
                    .into(),
                row::with_children(actions).spacing(12).into(),
            ])
            .spacing(8),
        )
        .padding([8, 8])
        .height(184.)
        .width(Length::Fill)
        .into()
    }
}
