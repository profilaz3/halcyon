//! Changes: every recorded change on one page, pinned at the bottom of the
//! sidebar. Your changes offer Undo while their record is kept (older ones
//! undo only themselves); every entry opens the setting it changed. Older
//! setups to go back to, and work left unfinished by an update, live here too.
use super::desktop::{WARN, banner};
use super::*;
use chrono::Datelike;
use crate::desktop::store::activity::{Action, Cause, Summary};
use crate::settings::support::{Destination, Filter, Message as Edit};

fn edit(message: Edit) -> Message {
    Message::Support(message)
}

/// One line of the timeline: a recorded change, or an automatic switch
/// that has no recorded change of its own.
pub(in crate::settings) enum Row<'a> {
    Change(&'a Summary),
    Automation(&'a crate::automation::Outcome),
}

impl Row<'_> {
    fn automatic(&self) -> bool {
        match self {
            Self::Change(entry) => entry.cause != Cause::You,
            Self::Automation(_) => true,
        }
    }
}

/// A recorded time in local time; `None` when it wasn't recorded.
fn local(time: i64) -> Option<chrono::DateTime<chrono::Local>> {
    (time > 0)
        .then(|| chrono::DateTime::from_timestamp_millis(time))
        .flatten()
        .map(|d| d.with_timezone(&chrono::Local))
}

/// The time of day in the clock's own format.
fn clock(date: chrono::DateTime<chrono::Local>, format_24h: bool) -> String {
    date.format(if format_24h { "%H:%M" } else { "%-I:%M %p" })
        .to_string()
}

/// "Today", "Yesterday" or the weekday and date.
fn day_title(date: Option<chrono::DateTime<chrono::Local>>) -> String {
    let Some(date) = date else {
        return "Earlier".into();
    };
    let today = chrono::Local::now().date_naive();
    match (today - date.date_naive()).num_days() {
        0 => "Today".into(),
        1 => "Yesterday".into(),
        _ if date.year() == today.year() => date.format("%A %-d %B").to_string(),
        _ => date.format("%A %-d %B %Y").to_string(),
    }
}

/// The rail's count badge: the theme's accent with black or white on it,
/// whichever reads better, so the number passes 4.5:1 in any theme.
fn badge_colors(theme: &cosmic::Theme) -> [Color; 2] {
    let fill: Color = theme.cosmic().accent_color().into();
    let ink = [Color::BLACK, Color::WHITE]
        .into_iter()
        .max_by(|a, b| {
            crate::widget_style::ratio(*a, fill).total_cmp(&crate::widget_style::ratio(*b, fill))
        })
        .unwrap_or(Color::WHITE);
    [fill, ink]
}

impl Settings {
    /// Recorded changes and automatic switches, newest first.
    pub(in crate::settings) fn change_timeline(&self) -> Vec<(i64, Row<'_>)> {
        let recent = &self.support.local.recent.entries;
        let mut timeline: Vec<(i64, Row<'_>)> = recent
            .iter()
            .map(|entry| {
                (
                    chrono::DateTime::parse_from_rfc3339(&entry.at)
                        .map(|d| d.timestamp_millis())
                        .unwrap_or(0),
                    Row::Change(entry),
                )
            })
            .collect();
        timeline.extend(
            self.support
                .local
                .automation
                .recent
                .iter()
                .filter(|outcome| {
                    outcome
                        .transaction
                        .as_ref()
                        .is_none_or(|id| !recent.iter().any(|entry| &entry.id == id))
                })
                .map(|outcome| (outcome.at.saturating_mul(1000), Row::Automation(outcome))),
        );
        timeline.sort_by_key(|(time, _)| std::cmp::Reverse(*time));
        timeline
    }

    /// How many changes were made today, for the sidebar. Automatic
    /// checks that left the desktop as it was don't count.
    pub(in crate::settings) fn changes_today(&self) -> usize {
        let today = chrono::Local::now().date_naive();
        self.change_timeline()
            .iter()
            .filter(|(_, row)| match row {
                Row::Change(_) => true,
                Row::Automation(outcome) => {
                    matches!(outcome.effect, crate::automation::Effect::Applied { .. })
                }
            })
            .filter(|(time, _)| local(*time).is_some_and(|d| d.date_naive() == today))
            .count()
    }

    /// The newest change in a few words, for the sidebar's tooltip.
    pub(in crate::settings) fn latest_change_line(&self) -> Option<String> {
        let (time, row) = self.change_timeline().into_iter().next()?;
        let what = match row {
            Row::Change(entry) => entry.title(),
            Row::Automation(outcome) => {
                crate::automation::status::outcome_line(outcome, self.config.clock.format_24h).what
            }
        };
        Some(match local(time) {
            Some(d) => format!(
                "Latest: {what} · {} {}",
                day_title(Some(d)),
                clock(d, self.config.clock.format_24h)
            ),
            None => format!("Latest: {what}"),
        })
    }

    /// The pinned sidebar entry: "Changes", today's count and a dot when
    /// something waits; hovering names the latest change.
    pub(super) fn changes_entry(&self, rail: bool) -> Element<'_, Message> {
        let active = self.page.sidebar() == Page::Changes;
        let today = self.changes_today();
        let unfinished = !self.recovery_sheet.rows.is_empty();
        // The dot means something waits for a choice; the count is news.
        let waiting = unfinished || !self.kept_previews().is_empty();
        let accent = self.settings_accent();
        let title = text::body("Changes")
            .size(if rail { 12 } else { 15 })
            .class(if active {
                cosmic::theme::Text::Custom(|theme| cosmic::iced::widget::text::Style {
                    color: Some(theme.cosmic().accent_text_color().into()),
                    ..Default::default()
                })
            } else {
                cosmic::theme::Text::Default
            });
        let icon: Element<'_, Message> = icon::from_name(Page::Changes.nav_icon())
            .size(18)
            .icon()
            .into();
        // In words beside the name; the narrow rail keeps a dot, and its
        // tooltip says what waits.
        let waiting_count = self.recovery_sheet.rows.len() + self.kept_previews().len();
        let mark: Element<'_, Message> = match (waiting, rail) {
            (true, true) => dot(WARN, 7.),
            (true, false) => label(format!("{waiting_count} waiting"), CAPTION, WARN),
            _ => cosmic::iced::widget::space().width(0).into(),
        };
        let count: Element<'_, Message> = if today > 0 {
            label(today.to_string(), 12., MUTED)
        } else {
            cosmic::iced::widget::space().width(0).into()
        };
        let content: Element<'_, Message> = if rail {
            // Today's count is a small badge beside the icon, so the one-line
            // label never reads as "Changes 2".
            let badge: Element<'_, Message> = if today > 0 {
                container(text::body(today.to_string()).size(CAPTION).line_height(1.0))
                    .padding([2, 5])
                    .class(cosmic::theme::Container::custom(move |theme| {
                        let [fill, ink] = badge_colors(theme);
                        cosmic::iced::widget::container::Style {
                            background: Some(Background::Color(fill)),
                            text_color: Some(ink),
                            border: Border {
                                radius: 8.0.into(),
                                ..Default::default()
                            },
                            ..Default::default()
                        }
                    }))
                    .into()
            } else {
                cosmic::iced::widget::space().width(0).into()
            };
            column::with_children(vec![
                row::with_children(vec![icon, badge, mark])
                    .spacing(2)
                    .align_y(Alignment::Start)
                    .into(),
                title.into(),
            ])
            .spacing(2)
            .width(Length::Fill)
            .align_x(Alignment::Center)
            .into()
        } else {
            row::with_children(vec![icon, container(title).width(Length::Fill).into(), count, mark])
                .spacing(12)
                .align_y(Alignment::Center)
                .into()
        };
        let entry = button::custom(content)
            .id(cosmic::widget::Id::new("nav-changes"))
            .class(nav_style(active, accent))
            .selected(active)
            .padding(if rail { [5, 2] } else { [11, 12] })
            .width(Length::Fill)
            .on_press(Message::OpenRoot(Page::Changes));
        let tip = if unfinished {
            Some("Unfinished changes from the last update are waiting".to_owned())
        } else if waiting {
            Some("A previewed change is waiting to be applied or discarded".to_owned())
        } else {
            self.latest_change_line()
        };
        match tip {
            Some(tip) => cosmic::widget::tooltip(
                entry,
                container(text::body(tip).size(12)).max_width(280),
                cosmic::widget::tooltip::Position::Top,
            )
            .padding(8)
            .gap(4)
            .into(),
            None => entry.into(),
        }
    }

    pub(super) fn changes_page(&self) -> Element<'_, Message> {
        let mut items = vec![heading(
            "Changes",
            "What changed on your desktop and widgets, and why. Undo any change of yours, or open it where it was made.",
        )];
        let unfinished = self.recovery_sheet.rows.len();
        if unfinished > 0 {
            items.push(banner(
                format!(
                    "{unfinished} unfinished {} from the previous version. Nothing changes until you choose.",
                    if unfinished == 1 { "change" } else { "changes" }
                ),
                WARN,
                vec![
                    button::standard("Review")
                        .id(cosmic::widget::Id::new("changes-unfinished"))
                        .on_press(Message::Page(Page::PendingChanges))
                        .into(),
                ],
            ));
        }
        if !self.kept_previews().is_empty() {
            items.push(self.not_applied_section());
        }
        let local = &self.support.local;
        if local.paused || local.skipping {
            items.push(group(
                None,
                vec![group_row_state(
                    if local.paused {
                        "Automatic switching is paused"
                    } else {
                        "The next scheduled switch will be skipped"
                    },
                    "Your current desktop stays as it is.",
                    button::text("Review rules")
                        .on_press(edit(Edit::Open(Destination::Automation)))
                        .into(),
                )],
            ));
        }
        if local.automation.pending.is_some() {
            items.push(group(
                Some("Automatic switching is waiting"),
                vec![group_row_state(
                    "Pending edits",
                    "Finish or discard pending edits before a look can switch.",
                    button::text("Review rules")
                        .on_press(edit(Edit::Open(Destination::Automation)))
                        .into(),
                )],
            ));
        }
        items.push(self.change_history());
        items.push(self.earlier_setups());
        column::with_children(items).spacing(20).into()
    }

    fn change_history(&self) -> Element<'_, Message> {
        let filter = self.support.filter;
        let accent = self.settings_accent();
        let mut items: Vec<Element<'_, Message>> = vec![
            row::with_children(vec![
                container(text::heading("History"))
                    .width(Length::Fill)
                    .into(),
                segmented(
                    [Filter::All, Filter::You, Filter::Automatic]
                        .into_iter()
                        .map(|choice| {
                            (
                                choice.title().to_owned(),
                                choice == filter,
                                Some(edit(Edit::Filter(choice))),
                            )
                        })
                        .collect(),
                    accent,
                ),
            ])
            .spacing(12)
            .align_y(Alignment::Center)
            .into(),
        ];
        if self.support.loading && local_is_empty(self) {
            items.push(label("Reading local history…", 14., MUTED));
        } else if self.support.local.problem {
            items.push(label(
                "Local history couldn’t be read. Your current desktop has not changed.",
                14.,
                MUTED,
            ));
        }
        let timeline: Vec<_> = self
            .change_timeline()
            .into_iter()
            .filter(|(_, row)| match filter {
                Filter::All => true,
                Filter::You => !row.automatic(),
                Filter::Automatic => row.automatic(),
            })
            .collect();
        if timeline.is_empty() && !self.support.loading && !self.support.local.problem {
            let (title, description) = match filter {
                Filter::All => (
                    "No changes recorded yet",
                    "Completed desktop changes will appear here. Note editing and private calendar content are never recorded.",
                ),
                Filter::You => (
                    "No changes made by you yet",
                    "Changes you make in Halcyon will appear here. Note editing and private calendar content are never recorded.",
                ),
                Filter::Automatic => (
                    "No automatic changes yet",
                    "Looks switched by time of day, a display or the wallpaper will appear here.",
                ),
            };
            items.push(group(None, vec![detail_state(title, description)]));
        }
        let mut day = String::new();
        let mut rows: Vec<Element<'_, Message>> = Vec::new();
        for (time, item) in timeline {
            let date = local(time);
            let title = day_title(date);
            if title != day {
                if !rows.is_empty() {
                    items.push(day_group(std::mem::take(&mut day), std::mem::take(&mut rows)));
                }
                day = title;
            }
            rows.push(self.change_row(date, item));
        }
        if !rows.is_empty() {
            items.push(day_group(day, rows));
        }
        items.push(label(
            if self.support.local.recent.earlier_partial {
                "Earlier history is partial. These are the records still available."
            } else {
                "Up to 50 recent changes are kept on this computer. Changes made outside Halcyon are not tracked."
            },
            12.,
            MUTED,
        ));
        column::with_children(items).spacing(12).into()
    }

    fn change_row(
        &self,
        date: Option<chrono::DateTime<chrono::Local>>,
        item: Row<'_>,
    ) -> Element<'_, Message> {
        let time = date
            .map(|d| clock(d, self.config.clock.format_24h))
            .unwrap_or_else(|| "Time not recorded".into());
        let entry = match item {
            Row::Automation(outcome) => {
                let line =
                    crate::automation::status::outcome_line(outcome, self.config.clock.format_24h);
                let mut rows = vec![group_row_state(
                    line.what,
                    format!("{time} · {}", line.cause),
                    button::text("Open")
                        .on_press(edit(Edit::Open(Destination::Automation)))
                        .into(),
                )];
                if let Some(why) = line.why {
                    rows.push(label(why, 12., MUTED));
                }
                return column::with_children(rows).spacing(6).into();
            }
            Row::Change(entry) => entry,
        };
        let open = self.support.expanded.as_deref() == Some(&entry.id);
        // Restores made here are undone from Before Halcyon, not twice.
        let elsewhere = crate::settings::support::change_page(entry) != Page::Changes;
        let undo = elsewhere
            && entry.action == Action::Changed
            && entry.cause == Cause::You
            && self.support.local.recent.undo_id.as_deref() == Some(&entry.id);
        // Older changes of yours can be undone on their own while their
        // record is kept.
        let yours = elsewhere && entry.action == Action::Changed && entry.cause == Cause::You;
        let put_back = yours
            && !undo
            && self.support.local.recent.restorable.iter().any(|id| *id == entry.id);
        let meta = format!(
            "{time} · {} · {}",
            entry.cause.title(),
            entry.areas.join(", ")
        );
        let mut actions = vec![
            button::text(if open { "Hide details" } else { "Details" })
                .id(cosmic::widget::Id::new(format!("change-details-{}", entry.id)))
                .on_press(edit(Edit::Details(
                    (!open).then(|| entry.id.clone()),
                )))
                .into(),
        ];
        if elsewhere {
            actions.push(
                button::text("Open")
                    .id(cosmic::widget::Id::new(format!("change-open-{}", entry.id)))
                    .on_press(edit(Edit::OpenChange(entry.id.clone())))
                    .into(),
            );
        }
        if undo {
            actions.push(
                button::standard("Undo")
                    .id(cosmic::widget::Id::new("history-review-undo"))
                    .on_press(Message::UndoDesktopShown(Some(entry.id.clone())))
                    .into(),
            );
        }
        if put_back {
            actions.push(
                button::standard("Undo")
                    .id(cosmic::widget::Id::new(format!("change-undo-{}", entry.id)))
                    .on_press(Message::PutBackChange(entry.id.clone()))
                    .into(),
            );
        }
        let mut rows = vec![group_row_state(
            entry.title(),
            meta,
            row::with_children(actions).spacing(8).wrap().into(),
        )];
        if put_back {
            rows.push(label(
                "Undoes only this change. Later changes stay.",
                12.,
                MUTED,
            ));
        }
        if open {
            if entry.details.is_empty() {
                rows.push(label(
                    "Individual values were not recorded for this change.",
                    12.,
                    MUTED,
                ));
            }
            for d in &entry.details {
                rows.push(label(d.line(), 14., FG));
            }
            if yours && !undo && !put_back {
                rows.push(label("Already undone, or too old to undo.", 12., MUTED));
            }
        }
        column::with_children(rows).spacing(6).into()
    }

    fn earlier_setups(&self) -> Element<'_, Message> {
        let mut earlier = self.desktop_recovery_rows();
        earlier.push(group_row(
            "Wallpaper from before Halcyon",
            "The picture you had when Halcyon was installed",
            button::standard("Restore…")
                .on_press_maybe((!self.busy && !preview()).then_some(Message::Restore(true)))
                .into(),
        ));
        self.search_target("changes-earlier", group(Some("Before Halcyon"), earlier))
    }
}

fn local_is_empty(app: &Settings) -> bool {
    app.support.local.recent.entries.is_empty() && app.support.local.automation.recent.is_empty()
}

/// One day of the history, titled "Today", "Yesterday" or its date.
fn day_group<'a>(day: String, rows: Vec<Element<'a, Message>>) -> Element<'a, Message> {
    cosmic::widget::settings::section()
        .title(day)
        .extend(rows)
        .into()
}

#[cfg(test)]
mod badge_tests {
    use super::*;
    use crate::widget_style::ratio;

    /// The count reads on its badge, and every status dot stands out from
    /// the page, in the light and dark themes and their high-contrast forms.
    #[test]
    fn badge_and_status_dots_stand_out_in_every_theme() {
        for theme in [
            cosmic::Theme::light(),
            cosmic::Theme::dark(),
            cosmic::Theme::light_hc(),
            cosmic::Theme::dark_hc(),
        ] {
            let [fill, ink] = badge_colors(&theme);
            assert!(ratio(ink, fill) >= 4.5, "{fill:?} {ink:?}");
            let pages: [Color; 2] = [
                theme.cosmic().bg_color().into(),
                theme.cosmic().primary(theme.transparent).base.into(),
            ];
            for color in [WARN, HEALTHY, MUTED, super::super::desktop::ERROR] {
                let dot = dot_color(&theme, color);
                for page in pages {
                    let shown = Color::from_rgb(
                        dot.r * dot.a + page.r * (1. - dot.a),
                        dot.g * dot.a + page.g * (1. - dot.a),
                        dot.b * dot.a + page.b * (1. - dot.a),
                    );
                    assert!(
                        ratio(shown, page) >= 3.,
                        "{color:?} -> {shown:?} on {page:?}: {}",
                        ratio(shown, page)
                    );
                }
            }
        }
    }
}
