//! Changes: every recorded change on one page, pinned at the bottom of the
//! sidebar. Undo is offered for the latest manual change only; every other
//! entry opens the setting it changed. Older setups to go back to, and work
//! left unfinished by an update, live here too.
use super::desktop::{WARN, banner};
use super::*;
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

fn local(time: i64) -> Option<chrono::DateTime<chrono::Local>> {
    chrono::DateTime::from_timestamp_millis(time).map(|d| d.with_timezone(&chrono::Local))
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
        _ => date.format("%A, %e %B").to_string(),
    }
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

    /// How many changes were made today, for the sidebar.
    pub(in crate::settings) fn changes_today(&self) -> usize {
        let today = chrono::Local::now().date_naive();
        self.change_timeline()
            .iter()
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
        let when = local(time)
            .map(|d| format!("{} {}", day_title(Some(d)), d.format("%H:%M")))
            .unwrap_or_default();
        Some(format!("Latest: {what} · {when}"))
    }

    /// The pinned sidebar entry: "Changes", today's count and a dot when
    /// something waits; hovering names the latest change.
    pub(super) fn changes_entry(&self, rail: bool) -> Element<'_, Message> {
        let active = self.page.sidebar() == Page::Changes;
        let today = self.changes_today();
        let unfinished = !self.recovery_sheet.rows.is_empty();
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
        let mark: Element<'_, Message> = if unfinished {
            dot(WARN, 7.)
        } else if today > 0 {
            dot(accent, 7.)
        } else {
            cosmic::iced::widget::space().width(0).into()
        };
        let count: Element<'_, Message> = if today > 0 && !rail {
            label(today.to_string(), 12., MUTED)
        } else {
            cosmic::iced::widget::space().width(0).into()
        };
        let content: Element<'_, Message> = if rail {
            column::with_children(vec![
                row::with_children(vec![icon, mark])
                    .spacing(2)
                    .align_y(Alignment::Start)
                    .into(),
                title.into(),
            ])
            .spacing(4)
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
            .padding(if rail { [8, 2] } else { [11, 12] })
            .width(Length::Fill)
            .on_press(Message::Page(Page::Changes));
        let tip = if unfinished {
            Some("Unfinished changes from the last update are waiting".to_owned())
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
            "What changed on your desktop, and why. Undo the latest change, or open any change where it was made.",
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
        if timeline.is_empty() && !self.support.loading {
            items.push(group(
                None,
                vec![detail_state(
                    match filter {
                        Filter::All => "No changes recorded yet",
                        Filter::You => "No changes made by you yet",
                        Filter::Automatic => "No automatic changes yet",
                    },
                    "Completed desktop changes will appear here. Note editing and private calendar content are never recorded.",
                )],
            ));
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
            .map(|d| d.format("%H:%M").to_string())
            .unwrap_or_default();
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
        let undo = entry.action == Action::Changed
            && entry.cause == Cause::You
            && self.support.local.recent.undo_id.as_deref() == Some(&entry.id);
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
            button::text("Open")
                .id(cosmic::widget::Id::new(format!("change-open-{}", entry.id)))
                .on_press(edit(Edit::OpenChange(entry.id.clone())))
                .into(),
        ];
        if undo {
            actions.push(
                button::standard("Undo…")
                    .id(cosmic::widget::Id::new("history-review-undo"))
                    .on_press(Message::UndoDesktopShown(Some(entry.id.clone())))
                    .into(),
            );
        }
        let mut rows = vec![group_row_state(
            entry.title(),
            meta,
            row::with_children(actions).spacing(8).wrap().into(),
        )];
        if open {
            if entry.details.is_empty() {
                rows.push(label(
                    "Individual values were not recorded for this change.",
                    12.,
                    MUTED,
                ));
            }
            for d in &entry.details {
                rows.push(label(
                    format!(
                        "{}: {} → {}{}",
                        d.setting,
                        d.before,
                        d.after,
                        match d.setting.as_str() {
                            "Bar spacing" | "Bar padding" | "Bar margin" | "Widget corners" => {
                                " px"
                            }
                            "Clock scale" | "Widget opacity" => "%",
                            _ => "",
                        }
                    ),
                    14.,
                    FG,
                ));
            }
            if !undo {
                rows.push(label(
                    "Undo is available only for your latest change.",
                    12.,
                    MUTED,
                ));
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
        let mut items = vec![self.search_target("changes-earlier", group(Some("Earlier setups"), earlier))];
        // Before the update, "how widgets are saved" lives under Updates.
        if self.design.managed {
            items.push(self.design_section(Some("Widgets")));
        }
        column::with_children(items).spacing(16).into()
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
