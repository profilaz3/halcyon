use super::*;
use crate::settings::support::{Destination, Health, Message as Edit};

fn action<'a>(title: &'static str, message: Edit) -> Element<'a, Message> {
    button::text(title)
        .on_press(Message::Support(message))
        .into()
}
impl Settings {
    pub(super) fn help_page(&self) -> Element<'_, Message> {
        if let Some(report) = &self.support.report {
            return column::with_children(vec![
                action("‹ Help & diagnostics", Edit::Preview(false)),
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
            vec![group_row(
                "Need a hand?",
                "Check what needs attention, or prepare a report to share.",
                button::suggested("Preview report")
                    .id(cosmic::widget::Id::new("help-preview-report"))
                    .on_press(Message::Support(Edit::Preview(true)))
                    .into(),
            )],
        )];
        let mut groups = Vec::new();
        for (heading, problem) in [("Needs attention", true), ("Local status", false)] {
            let rows = snapshot
                .components
                .iter()
                .filter(|c| (c.health == Health::Attention) == problem)
                .map(|c| {
                    let title = match c.action {
                        Destination::Updates => "Open Updates",
                        Destination::Background => "Widget controls",
                        Destination::Islands => "Review Islands",
                        Destination::Calendar => "Open Calendar",
                        Destination::Displays => "Layout & displays",
                        Destination::Automation => "Review rules",
                        Destination::Recovery => "Open Recovery",
                    };
                    group_row(c.name, c.state, action(title, Edit::Open(c.action)))
                })
                .collect::<Vec<_>>();
            if !rows.is_empty() {
                groups.push(group(Some(heading), rows));
            }
        }
        if groups.len() == 2 {
            let right = groups.pop().unwrap();
            let left = groups.pop().unwrap();
            items.push(self.setting_columns(left, right));
        } else {
            items.extend(groups);
        }
        items.push(
            row::with_children(vec![
                button::standard(if self.support.loading {
                    "Refreshing…"
                } else {
                    "Refresh status"
                })
                .on_press_maybe((!self.support.loading).then_some(Message::Support(Edit::Refresh)))
                .into(),
                action("Report a problem ↗", Edit::ReportProblem),
            ])
            .spacing(8)
            .into(),
        );
        items.push(label("These are local observations. Calendar and update servers are contacted only from their own controls.",12.,MUTED));
        items.push(group(
            Some("Understand a desktop change"),
            vec![group_row(
                "Recent changes",
                "See what changed and why, including automatic switches.",
                action("View changes", Edit::Recent(true)),
            )],
        ));
        column::with_children(items).spacing(20).into()
    }
    pub(super) fn recent_changes_page(&self) -> Element<'_, Message> {
        use crate::desktop::store::activity::{Action, Cause};
        let mut items = vec![
            action("‹ Recovery", Edit::Recent(false)),
            heading("Recent changes", "What changed on your desktop, and why."),
        ];
        if self.support.loading {
            items.push(label("Reading local history…", 14., MUTED));
        } else if self.support.local.problem {
            items.push(label(
                "Local history couldn’t be read. Your current desktop has not changed.",
                14.,
                MUTED,
            ));
        }
        let state = &self.support.local.automation;
        if self.support.local.paused || self.support.local.skipping {
            items.push(group(
                None,
                vec![group_row(
                    if self.support.local.paused {
                        "Automatic switching is paused"
                    } else {
                        "The next scheduled switch will be skipped"
                    },
                    "Your current desktop stays as it is.",
                    action("Review rules", Edit::Open(Destination::Automation)),
                )],
            ));
        }
        if state.pending.is_some() {
            items.push(group(
                Some("Automatic switching is waiting"),
                vec![
                    label(
                        "Finish or discard pending edits before a setup can switch.",
                        14.,
                        MUTED,
                    ),
                    action("Review rules", Edit::Open(Destination::Automation)),
                ],
            ));
        }
        if self.support.local.recent.entries.is_empty()
            && state.recent.is_empty()
            && !self.support.loading
        {
            items.push(group(None,vec![label("No changes recorded yet",18.,FG),label("Completed desktop changes will appear here. Note editing and private calendar content are never recorded.",14.,MUTED)]));
        }
        enum Row<'a> {
            Change(&'a crate::desktop::store::activity::Summary),
            Automation(&'a crate::automation::Outcome),
        }
        let mut timeline = self
            .support
            .local
            .recent
            .entries
            .iter()
            .map(|e| {
                (
                    chrono::DateTime::parse_from_rfc3339(&e.at)
                        .map(|d| d.timestamp_millis())
                        .unwrap_or(0),
                    Row::Change(e),
                )
            })
            .collect::<Vec<_>>();
        timeline.extend(
            state
                .recent
                .iter()
                .filter(|o| {
                    o.transaction.as_ref().is_none_or(|id| {
                        !self
                            .support
                            .local
                            .recent
                            .entries
                            .iter()
                            .any(|e| &e.id == id)
                    })
                })
                .map(|o| (o.at.saturating_mul(1000), Row::Automation(o))),
        );
        timeline.sort_by_key(|(time, _)| std::cmp::Reverse(*time));
        let mut day = String::new();
        for (time, item) in timeline {
            let date = chrono::DateTime::from_timestamp_millis(time)
                .map(|d| d.with_timezone(&chrono::Local));
            let new_day = date
                .map(|d| d.format("%A, %e %B").to_string())
                .unwrap_or_else(|| "Earlier".into());
            if new_day != day {
                items.push(label(new_day.clone(), 14., MUTED));
                day = new_day;
            }
            let entry = match item {
                Row::Automation(o) => {
                    let line =
                        crate::automation::status::outcome_line(o, self.config.clock.format_24h);
                    let time = date
                        .map(|d| d.format("%H:%M").to_string())
                        .unwrap_or_default();
                    let mut rows = vec![group_row(
                        line.what,
                        format!("{time} · {}", line.cause),
                        action("Review rules", Edit::Open(Destination::Automation)),
                    )];
                    if let Some(why) = line.why {
                        rows.push(label(why, 12., MUTED));
                    }
                    if o.transaction.is_none()
                        && matches!(o.effect, crate::automation::Effect::Applied { .. })
                    {
                        rows.push(label(
                            "Earlier automation observation; no linked transaction was recorded.",
                            12.,
                            MUTED,
                        ));
                    }
                    items.push(group(None, rows));
                    continue;
                }
                Row::Change(entry) => entry,
            };
            let open = self.support.expanded.as_deref() == Some(&entry.id);
            let eligible = entry.action == Action::Changed
                && entry.cause == Cause::You
                && self.support.local.recent.undo_id.as_deref() == Some(&entry.id);
            let meta = format!(
                "{} · {} · {}",
                date.map(|d| d.format("%H:%M").to_string())
                    .unwrap_or_default(),
                entry.cause.title(),
                entry.areas.join(", ")
            );
            let mut actions = vec![action(
                if open { "Hide details" } else { "Details" },
                Edit::Details(if open { None } else { Some(entry.id.clone()) }),
            )];
            if eligible {
                actions.push(
                    button::standard("Review Undo")
                        .id(cosmic::widget::Id::new("history-review-undo"))
                        .on_press(Message::UndoDesktopShown(Some(entry.id.clone())))
                        .into(),
                );
            }
            let mut rows = vec![group_row(
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
                                "Bar spacing" | "Bar padding" | "Bar margin" | "Widget corners" =>
                                    " px",
                                "Clock scale" | "Widget opacity" => "%",
                                _ => "",
                            }
                        ),
                        14.,
                        FG,
                    ));
                }
                if !eligible {
                    rows.push(label("This entry is a record of what happened. Undo is available only for the latest eligible manual change.",12.,MUTED));
                }
                let destination = match entry.cause {
                    Cause::Schedule | Cause::DisplayRule => Message::Page(Page::DesktopAutomation),
                    Cause::WallpaperColors => Message::Page(Page::Themes),
                    Cause::You => match crate::desktop::Owner::parse(&entry.owner) {
                        Some(crate::desktop::Owner::DockPanel) => Message::Page(Page::DesktopDock),
                        Some(crate::desktop::Owner::Profile) => {
                            Message::Page(Page::DesktopProfiles)
                        }
                        Some(crate::desktop::Owner::Wallpaper) => {
                            Message::Page(Page::DesktopWallpaper)
                        }
                        Some(crate::desktop::Owner::Appearance) => Message::Page(Page::Themes),
                        _ => Message::AppSettingsPage(AppSettingsPage::Restore),
                    },
                };
                rows.push(button::text("Open settings").on_press(destination).into());
            }
            items.push(group(None, rows));
        }
        items.push(label(if self.support.local.recent.earlier_partial {"Earlier history is partial. These are the records still available."} else {"Up to 50 recent changes are kept on this computer. Changes made outside Halcyon are not tracked."},12.,MUTED));
        items.push(action("Refresh history", Edit::Refresh));
        column::with_children(items).spacing(16).into()
    }
}
