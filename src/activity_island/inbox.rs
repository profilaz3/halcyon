//! The open island's notification list: notifications grouped by app, the
//! session's Recent list and per-app rules. Everything here stays in memory.
use super::*;
use notifications::{Note, Recent};
use preferences::AppRule;

/// Recent keeps this many notifications, newest first.
pub(super) const RECENT: usize = 20;
/// Recent rows shown before the section scrolls.
const RECENT_ROWS: usize = 4;
const RECENT_ROW: f32 = 30.;
const RECENT_GAP: f32 = 2.;
/// The Recent toggle under the list.
const RECENT_HEAD: f32 = 32.;
/// "N from muted apps" above the Recent rows.
const MUTED_LINE: f32 = 18.;
/// A notification's own buttons under its row in an open group.
const ACTION_ROW: f32 = 32.;
const MENU_ROW: f32 = 32.;
const MENU_GAP: f32 = 2.;
/// Back and the four rules.
const MENU_HEIGHT: f32 = 5. * MENU_ROW + 4. * MENU_GAP;
/// The most the list asks of the card below its title line: five rows,
/// Clear all, and an open Recent section.
pub(super) const LIST_MAX: f32 = LIST_ROWS as f32 * (ROW_HEIGHT + ROW_GAP) - ROW_GAP
    + LIST_FOOTER
    + ROW_GAP
    + RECENT_HEAD
    + MUTED_LINE
    + RECENT_GAP
    + RECENT_ROWS as f32 * (RECENT_ROW + RECENT_GAP);

/// The app a rules menu is open for.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct MenuTarget {
    pub app: String,
    pub entry: String,
}

/// One line of the list: a notification, or two or more from one app.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum Line {
    Note(u32),
    /// The app key and its notifications, newest first.
    Group(String, Vec<u32>),
}

/// What the list draws, top to bottom.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum Row {
    Note(u32),
    Group(String, Vec<u32>),
    /// A notification inside the open group.
    Member(u32),
}

pub(super) struct Layout {
    pub rows: Vec<Row>,
    /// Live notifications that did not fit, counted as "and N more".
    pub hidden: usize,
    /// The body's height below the title line.
    pub height: f32,
}

impl Applet {
    pub(super) fn rule(&self, note: &Note) -> Option<AppRule> {
        self.snapshot.preferences.rule_for(&note.app, &note.entry)
    }
    /// Something to show under Recent.
    pub(super) fn has_recent(&self) -> bool {
        !self.recent.is_empty() || self.muted > 0
    }
    /// The reveal showing now is for an app set to Always show.
    pub(super) fn showing_always(&self) -> bool {
        matches!(self.kind, Kind::Notification(id)
            if self.reveal_deadline.is_some()
                && self.note(id).is_some_and(|n| self.rule(n) == Some(AppRule::Always)))
    }
    /// Urgent notifications first, each on its own; then the rest newest
    /// first, two or more from one app as one group.
    pub(super) fn lines(&self) -> Vec<Line> {
        let mut lines: Vec<Line> = self
            .notes
            .iter()
            .filter(|n| n.urgent)
            .map(|n| Line::Note(n.id))
            .collect();
        let mut groups: Vec<(&str, Vec<u32>)> = Vec::new();
        for n in self.notes.iter().filter(|n| !n.urgent) {
            match groups.iter_mut().find(|(key, _)| *key == n.app_key()) {
                Some((_, ids)) => ids.push(n.id),
                None => groups.push((n.app_key(), vec![n.id])),
            }
        }
        lines.extend(groups.into_iter().map(|(key, ids)| match ids.as_slice() {
            [one] => Line::Note(*one),
            _ => Line::Group(key.to_owned(), ids),
        }));
        lines
    }
    /// The notifications a group stands for: every one from the app that
    /// is not urgent.
    pub(super) fn group_ids(&self, key: &str) -> Vec<u32> {
        self.notes
            .iter()
            .filter(|n| !n.urgent && n.app_key() == key)
            .map(|n| n.id)
            .collect()
    }
    /// What fits in `room` below the title line, and the height it takes.
    pub(super) fn list_layout(&self, room: f32) -> Layout {
        if self.menu.is_some() {
            return Layout {
                rows: Vec::new(),
                hidden: 0,
                height: MENU_HEIGHT,
            };
        }
        let live = !self.notes.is_empty();
        let mut fixed = if live { LIST_FOOTER } else { 0. };
        if self.has_recent() {
            fixed += RECENT_HEAD + if live { ROW_GAP } else { 0. };
            if self.recent_open {
                if self.muted > 0 {
                    fixed += MUTED_LINE + RECENT_GAP;
                }
                fixed += self.recent.len().min(RECENT_ROWS) as f32 * (RECENT_ROW + RECENT_GAP);
            }
        }
        let mut all = Vec::new();
        for line in self.lines() {
            match line {
                Line::Note(id) => all.push(Row::Note(id)),
                Line::Group(key, ids) => {
                    let open = self.open_group.as_deref() == Some(key.as_str());
                    all.push(Row::Group(key, ids.clone()));
                    if open {
                        all.extend(ids.into_iter().map(Row::Member));
                    }
                }
            }
        }
        let height = |row: &Row| match row {
            Row::Member(id) if self.note(*id).is_some_and(|n| !n.actions.is_empty()) => {
                ROW_HEIGHT + ACTION_ROW
            }
            _ => ROW_HEIGHT,
        };
        let budget = room - fixed;
        // Up to five lines, with an open group's own rows beyond them, as
        // far as the room goes; always one.
        let take = |start: usize| {
            let (mut used, mut lines, mut rows) = (0., 0, Vec::new());
            for row in &all[start..] {
                let lead = !matches!(row, Row::Member(_));
                let next = used + height(row) + if rows.is_empty() { 0. } else { ROW_GAP };
                if !rows.is_empty() && (next > budget + 0.5 || (lead && lines == LIST_ROWS)) {
                    break;
                }
                lines += usize::from(lead);
                used = next;
                rows.push(row.clone());
            }
            (rows, used)
        };
        let (mut rows, mut used) = take(0);
        // An open group that would run out of room moves to the top.
        if let Some(head) = all.iter().position(
            |row| matches!(row, Row::Group(key, _) if self.open_group.as_deref() == Some(key.as_str())),
        ) {
            let members = all[head + 1..]
                .iter()
                .take_while(|row| matches!(row, Row::Member(_)))
                .count();
            if head > 0 && rows.len() < head + 1 + members {
                (rows, used) = take(head);
            }
        }
        let shown: usize = rows
            .iter()
            .map(|row| match row {
                Row::Note(_) | Row::Member(_) => 1,
                Row::Group(key, ids) if self.open_group.as_deref() != Some(key.as_str()) => {
                    ids.len()
                }
                Row::Group(..) => 0,
            })
            .sum();
        Layout {
            rows,
            hidden: self.notes.len().saturating_sub(shown),
            height: used + fixed,
        }
    }
    /// Height the list needs beyond the card, as large as it may grow.
    pub(super) fn list_extra(&self) -> f32 {
        (self.list_layout(LIST_MAX).height - LIST_ROOM).max(0.)
    }
    /// Rows that fit the open popup.
    #[cfg(test)]
    pub(super) fn list_rows(&self) -> usize {
        self.list_layout(LIST_ROOM + self.popup_extra).rows.len()
    }
    /// The card's own extra height while listing.
    pub(super) fn list_card_extra(&self, extra: f32) -> f32 {
        (self.list_layout(LIST_ROOM + extra).height - LIST_ROOM).clamp(0., extra)
    }
    /// A popup's size is fixed when it opens: reopen it in hand when the
    /// list has grown past it.
    pub(super) fn fit_list(&mut self) -> Task<Message> {
        match self.popup {
            Some(id) if self.popup_extra + 0.5 < self.list_extra() => {
                self.held = true;
                self.promote_popup = Some(id);
                self.update(Message::Surface(cosmic::surface::action::destroy_popup(id)))
            }
            _ => Task::none(),
        }
    }
    /// Keeps a notification that left the list, unless its app is muted.
    pub(super) fn remember(&mut self, note: Note, alive: bool) {
        if self.rule(&note) == Some(AppRule::Mute) || !self.snapshot.preferences.notifications {
            return;
        }
        self.recent.retain(|r| r.note.id != note.id);
        let app_path = if note.entry.is_empty() {
            None
        } else {
            match self.recent.iter().find(|r| r.note.entry == note.entry) {
                Some(known) => known.app_path.clone(),
                None => notifications::entry_path(&note.entry),
            }
        };
        self.recent.insert(
            0,
            Recent {
                note,
                alive,
                app_path,
            },
        );
        self.recent.truncate(RECENT);
    }
    /// Notifications whose own timeout has passed move to Recent, still
    /// open on the server. Not while the island is open on them.
    pub(super) fn expire_notes(&mut self) {
        self.expiry_at = None;
        if self.popup.is_some() {
            return;
        }
        let now_ms = chrono::Utc::now().timestamp_millis();
        let gone: Vec<Note> = self
            .notes
            .iter()
            .filter(|n| n.timeout_ms > 0 && n.at_ms + i64::from(n.timeout_ms) <= now_ms)
            .cloned()
            .collect();
        for note in gone {
            let id = note.id;
            self.remember(note, true);
            self.forget_note(id);
        }
        self.arm_expiry();
    }
    /// One deadline for the soonest notification timeout.
    pub(super) fn arm_expiry(&mut self) {
        let now_ms = chrono::Utc::now().timestamp_millis();
        self.expiry_at = self
            .notes
            .iter()
            .filter(|n| n.timeout_ms > 0)
            .map(|n| n.at_ms + i64::from(n.timeout_ms))
            .min()
            .map(|end| Instant::now() + Duration::from_millis((end - now_ms).max(0) as u64));
    }
    /// Muted apps are neither listed nor kept; returns how many went.
    pub(super) fn drop_muted(&mut self) -> usize {
        let muted: Vec<u32> = self
            .notes
            .iter()
            .filter(|n| self.rule(n) == Some(AppRule::Mute))
            .map(|n| n.id)
            .collect();
        for id in &muted {
            self.forget_note(*id);
        }
        let prefs = &self.snapshot.preferences;
        self.recent
            .retain(|r| prefs.rule_for(&r.note.app, &r.note.entry) != Some(AppRule::Mute));
        if self
            .open_group
            .as_ref()
            .is_some_and(|key| self.group_ids(key).len() < 2)
        {
            self.open_group = None;
        }
        muted.len()
    }
    /// After notifications leave the open list, it leads with one that is
    /// still there.
    fn settle_list(&mut self) -> Task<Message> {
        if self.notes.is_empty() && !(self.list && self.has_recent()) {
            return self.update(Message::Close);
        }
        if matches!(self.kind, Kind::Notification(id) if self.note(id).is_none()) {
            if let Some(first) = self.notes.iter().find(|n| n.urgent).or(self.notes.first()) {
                self.kind = Kind::Notification(first.id);
            } else if !self.list {
                return self.update(Message::Close);
            }
        }
        Task::none()
    }
    /// The island's handling of groups, Recent and rules.
    pub(super) fn inbox_update(&mut self, message: Message, now: Instant) -> Task<Message> {
        match message {
            Message::GroupToggle(key) => {
                self.open_group = if self.open_group.as_ref() == Some(&key) {
                    None
                } else {
                    Some(key)
                };
                self.reveal_deadline = None;
                return self.fit_list();
            }
            Message::GroupDismiss(key) => {
                let ids = self.group_ids(&key);
                // Oldest first, so Recent keeps them newest first.
                for id in ids.iter().rev() {
                    if let Some(note) = self.note(*id).cloned() {
                        self.remember(note, false);
                    }
                    self.forget_note(*id);
                }
                if self.open_group.as_ref() == Some(&key) {
                    self.open_group = None;
                }
                self.batch.retain(|b| !ids.contains(b));
                let follow = self.settle_list();
                let dismiss = cosmic::task::future(async move {
                    Message::SettingsOpened(notifications::dismiss(ids).await)
                });
                return Task::batch([follow, dismiss]);
            }
            Message::RecentToggle => {
                self.recent_open = !self.recent_open;
                return self.fit_list();
            }
            Message::RecentClear => {
                self.recent.clear();
                self.muted = 0;
                self.recent_open = false;
                if self.notes.is_empty() {
                    return self.update(Message::Close);
                }
            }
            Message::RecentOpen(id) => {
                let Some(recent) = self.recent.iter_mut().find(|r| r.note.id == id) else {
                    return Task::none();
                };
                let alive = recent.alive && recent.note.default;
                let path = recent.app_path.clone();
                if !alive && path.is_none() {
                    return Task::none();
                }
                if alive && !recent.note.resident {
                    recent.alive = false;
                }
                let close = self.update(Message::Close);
                let open = cosmic::task::future(async move {
                    if alive && notifications::invoke(id, "default".into()).await.is_ok() {
                        return Message::SettingsOpened(Ok(()));
                    }
                    Message::SettingsOpened(match path {
                        Some(path) => notifications::open_app(path).await,
                        None => Err("Could not open the notification".into()),
                    })
                });
                return Task::batch([close, open]);
            }
            Message::ShowRecent => {
                if !self.notes_shown() || !self.has_recent() {
                    return Task::none();
                }
                self.recent_open = true;
                return self.update(Message::ShowNotes);
            }
            Message::NoteMenu(id) => {
                let Some(note) = self
                    .note(id)
                    .or_else(|| self.recent.iter().map(|r| &r.note).find(|n| n.id == id))
                else {
                    return Task::none();
                };
                if !preferences::valid_rule(&note.app, &note.entry) {
                    return Task::none();
                }
                self.menu = Some(MenuTarget {
                    app: note.app.clone(),
                    entry: note.entry.clone(),
                });
                self.reveal_deadline = None;
                self.revealed = false;
                self.held = true;
                return self.fit_list();
            }
            Message::MenuClose => {
                self.menu = None;
            }
            Message::NoteRule(rule) => {
                let Some(target) = self.menu.take() else {
                    return Task::none();
                };
                let mut next = self.snapshot.preferences.clone();
                next.set_rule(&target.app, &target.entry, rule);
                self.snapshot.preferences.app_rules = next.app_rules.clone();
                self.rules_wish = Some((next.app_rules, now + DND_SETTLE));
                if rule == Some(AppRule::Mute) {
                    self.muted += self.drop_muted();
                }
                let follow = self.settle_list();
                let save = self.update(Message::Control(Command::NoteRule {
                    app: target.app,
                    entry: target.entry,
                    rule,
                }));
                return Task::batch([follow, save]);
            }
            _ => {}
        }
        Task::none()
    }

    /// An app icon for a notification, the notification symbol otherwise.
    pub(super) fn note_icon<'a>(&self, note: Option<&Note>, size: u16) -> Element<'a, Message> {
        if let Some(icon) = note.map(|n| n.icon.as_str()).filter(|i| !i.is_empty()) {
            let handle = if icon.starts_with('/') {
                widget::icon::from_path(std::path::PathBuf::from(icon))
            } else {
                widget::icon::from_name(icon)
                    .size(size)
                    .fallback(Some(widget::icon::IconFallback::Names(vec![
                        "preferences-system-notifications-symbolic".into(),
                    ])))
                    .handle()
            };
            return widget::icon(handle).size(size).into();
        }
        widget::icon::from_name(Kind::Notification(0).icon())
            .symbolic(true)
            .size(size)
            .into()
    }
    fn round_icon<'a>(
        &self,
        palette: Palette,
        tint: Color,
        reveal: f32,
        (name, id, label): (&str, String, &str),
        press: Message,
    ) -> Element<'a, Message> {
        self.tip(
            palette,
            widget::button::icon(widget::icon::from_name(name).symbolic(true))
                .id(id.into())
                .icon_size(14)
                .padding(7)
                .width(28)
                .height(28)
                .class(capsule_button(palette, tint, reveal, Role::Ghost))
                .on_press(press),
            label,
            widget::tooltip::Position::Top,
        )
    }
    /// Two lines: a title with the age at its end, then a quieter detail.
    fn note_lines<'a>(
        &self,
        palette: Palette,
        reveal: f32,
        title: String,
        ink: Color,
        at_ms: i64,
        detail: String,
    ) -> Element<'a, Message> {
        widget::column([
            widget::row([
                widget::container(self.text(title, 13., alpha(ink, reveal)))
                    .width(Length::Fill)
                    .into(),
                self.text(
                    super::note_age(at_ms, self.now_ms, self.format_24h),
                    11.,
                    alpha(palette.quiet, reveal),
                ),
            ])
            .spacing(8)
            .align_y(cosmic::iced::Alignment::Center)
            .into(),
            self.text(detail, 12., alpha(palette.quiet, reveal)),
        ])
        .spacing(2)
        .into()
    }
    /// One notification: a row that opens it, More and its own dismiss.
    fn note_row<'a>(
        &self,
        palette: Palette,
        tint: Color,
        reveal: f32,
        n: &Note,
        member: bool,
    ) -> Element<'a, Message> {
        let center = cosmic::iced::Alignment::Center;
        let text = self.snapshot.preferences.notification_text;
        let detail = if member {
            if text && !n.body.is_empty() {
                n.body.clone()
            } else {
                n.app.clone()
            }
        } else if text && !n.body.is_empty() {
            format!("{} · {}", n.app, n.body)
        } else {
            n.app.clone()
        };
        let ink = if n.urgent {
            palette.warning()
        } else {
            palette.ink
        };
        let lines = self.note_lines(palette, reveal, n.title.clone(), ink, n.at_ms, detail);
        let content: Element<'a, Message> = if member {
            lines
        } else {
            widget::row([self.note_icon(Some(n), 20), lines])
                .spacing(10)
                .align_y(center)
                .into()
        };
        let open = widget::button::custom(content)
            .id(format!("activity-note-row-{}", n.id).into())
            .width(Length::Fill)
            .height(super::ROW_HEIGHT)
            .padding(if member {
                [4, 10, 4, 40]
            } else {
                [4, 10, 4, 10]
            })
            .class(capsule_button(palette, tint, reveal, Role::Ghost))
            // Like the popup it stands for: the notification itself, or
            // its card when the app gave it nothing to open.
            .on_press(if n.default {
                Message::NoteAction(n.id, "default".into())
            } else {
                Message::Select(Kind::Notification(n.id))
            });
        let mut row = widget::row([widget::mouse_area(open)
            .on_right_press(Message::NoteMenu(n.id))
            .into()])
        .spacing(2)
        .align_y(center);
        if !member {
            row = row.push(self.round_icon(
                palette,
                tint,
                reveal,
                (
                    "view-more-symbolic",
                    format!("activity-note-row-more-{}", n.id),
                    "More",
                ),
                Message::NoteMenu(n.id),
            ));
        }
        row = row.push(self.round_icon(
            palette,
            tint,
            reveal,
            (
                "window-close-symbolic",
                format!("activity-note-row-dismiss-{}", n.id),
                "Dismiss",
            ),
            Message::NoteDismiss(n.id),
        ));
        if !member || n.actions.is_empty() {
            return row.into();
        }
        // In an open group each notification keeps its own buttons.
        let mut buttons = widget::row([widget::Space::new().width(30).into()]).spacing(4);
        for (index, (key, label)) in n.actions.iter().take(2).enumerate() {
            buttons = buttons.push(
                widget::button::custom(
                    widget::container(self.text(label.clone(), 12., alpha(palette.ink, reveal)))
                        .height(Length::Fill)
                        .align_y(center),
                )
                .id(format!("activity-note-row-action-{}-{index}", n.id).into())
                .height(28)
                .padding([0, 12])
                .class(capsule_button(
                    palette,
                    tint,
                    reveal,
                    if index == 0 { Role::Quiet } else { Role::Ghost },
                ))
                .on_press(Message::NoteAction(n.id, key.clone())),
            );
        }
        widget::column([row.into(), buttons.into()])
            .spacing(ACTION_ROW - 28.)
            .into()
    }
    /// Two or more from one app: "Slack · 4 new" with the newest beneath.
    fn group_row<'a>(
        &self,
        palette: Palette,
        tint: Color,
        reveal: f32,
        key: &str,
        ids: &[u32],
    ) -> Element<'a, Message> {
        let center = cosmic::iced::Alignment::Center;
        let Some(newest) = ids.first().and_then(|id| self.note(*id)) else {
            return widget::Space::new().into();
        };
        let open = self.open_group.as_deref() == Some(key);
        let detail = if self.snapshot.preferences.notification_text && !newest.body.is_empty() {
            format!("{} · {}", newest.title, newest.body)
        } else {
            newest.title.clone()
        };
        let lines = self.note_lines(
            palette,
            reveal,
            format!("{} · {} new", newest.app, ids.len()),
            palette.ink,
            newest.at_ms,
            detail,
        );
        let toggle = widget::button::custom(
            widget::row([
                self.note_icon(Some(newest), 20),
                lines,
                widget::icon::from_name(if open {
                    "pan-up-symbolic"
                } else {
                    "pan-down-symbolic"
                })
                .symbolic(true)
                .size(12)
                .into(),
            ])
            .spacing(10)
            .align_y(center),
        )
        .id(format!("activity-note-group-{key}").into())
        .width(Length::Fill)
        .height(super::ROW_HEIGHT)
        .padding([4, 10])
        .class(capsule_button(
            palette,
            tint,
            reveal,
            if open { Role::Quiet } else { Role::Ghost },
        ))
        .on_press(Message::GroupToggle(key.to_owned()));
        widget::row([
            widget::mouse_area(toggle)
                .on_right_press(Message::NoteMenu(newest.id))
                .into(),
            self.round_icon(
                palette,
                tint,
                reveal,
                (
                    "view-more-symbolic",
                    format!("activity-note-group-more-{key}"),
                    "More",
                ),
                Message::NoteMenu(newest.id),
            ),
            self.round_icon(
                palette,
                tint,
                reveal,
                (
                    "window-close-symbolic",
                    format!("activity-note-group-dismiss-{key}"),
                    "Dismiss all",
                ),
                Message::GroupDismiss(key.to_owned()),
            ),
        ])
        .spacing(2)
        .align_y(center)
        .into()
    }
    /// The quiet section under the list: a toggle, then what left it.
    fn recent_view<'a>(&self, palette: Palette, tint: Color, reveal: f32) -> Element<'a, Message> {
        let center = cosmic::iced::Alignment::Center;
        let label = match self.recent.len() {
            0 => "Recent".to_owned(),
            count => format!("Recent · {count}"),
        };
        let toggle = widget::button::custom(
            widget::container(
                widget::row([
                    self.text(label, 12., alpha(palette.quiet, reveal)),
                    widget::icon::from_name(if self.recent_open {
                        "pan-up-symbolic"
                    } else {
                        "pan-down-symbolic"
                    })
                    .symbolic(true)
                    .size(12)
                    .into(),
                ])
                .spacing(6)
                .align_y(center),
            )
            .height(Length::Fill)
            .align_y(center),
        )
        .id("activity-recent-toggle".into())
        .height(RECENT_HEAD)
        .padding([0, 10])
        .class(capsule_button(palette, tint, reveal, Role::Ghost))
        .on_press(Message::RecentToggle);
        let mut head = widget::row([
            toggle.into(),
            widget::Space::new().width(Length::Fill).into(),
        ])
        .align_y(center);
        if self.recent_open {
            head = head.push(self.label_button(
                palette,
                "activity-recent-clear",
                "Clear",
                tint,
                reveal,
                Role::Ghost,
                Some(Message::RecentClear),
            ));
        }
        let mut section = widget::column([head.into()]).spacing(RECENT_GAP);
        if !self.recent_open {
            return section.into();
        }
        if self.muted > 0 {
            section = section.push(
                widget::container(self.text(
                    match self.muted {
                        1 => "1 from a muted app".to_owned(),
                        count => format!("{count} from muted apps"),
                    },
                    11.,
                    alpha(palette.quiet, reveal),
                ))
                .height(MUTED_LINE)
                .padding([0, 10])
                .align_y(center),
            );
        }
        let mut rows = widget::column([]).spacing(RECENT_GAP);
        for r in &self.recent {
            let n = &r.note;
            let content = widget::row([
                self.note_icon(Some(n), 16),
                widget::container(self.text(
                    n.title.clone(),
                    12.,
                    alpha(palette.ink, 0.85 * reveal),
                ))
                .width(Length::Fill)
                .into(),
                self.text(
                    super::note_age(n.at_ms, self.now_ms, self.format_24h),
                    11.,
                    alpha(palette.quiet, reveal),
                ),
            ])
            .spacing(10)
            .align_y(center);
            let opens = (r.alive && n.default) || r.app_path.is_some();
            let row: Element<'a, Message> = if opens {
                widget::button::custom(content)
                    .id(format!("activity-recent-row-{}", n.id).into())
                    .width(Length::Fill)
                    .height(RECENT_ROW)
                    .padding([0, 10])
                    .class(capsule_button(palette, tint, reveal, Role::Ghost))
                    .on_press(Message::RecentOpen(n.id))
                    .into()
            } else {
                widget::container(content)
                    .width(Length::Fill)
                    .height(RECENT_ROW)
                    .padding([0, 10])
                    .into()
            };
            rows = rows.push(widget::mouse_area(row).on_right_press(Message::NoteMenu(n.id)));
        }
        if self.recent.is_empty() {
            return section.into();
        }
        let shown = self.recent.len().min(RECENT_ROWS) as f32;
        section
            .push(
                widget::scrollable(rows)
                    .id("activity-recent")
                    .height(shown * (RECENT_ROW + RECENT_GAP) - RECENT_GAP)
                    .width(Length::Fill),
            )
            .into()
    }
    /// The rules for one app, in place of the list.
    fn menu_view<'a>(&self, palette: Palette, tint: Color, reveal: f32) -> Element<'a, Message> {
        let center = cosmic::iced::Alignment::Center;
        let current = self
            .menu
            .as_ref()
            .and_then(|t| self.snapshot.preferences.rule_for(&t.app, &t.entry));
        let item = |id: &'static str, icon: Option<&'static str>, label: &str, press: Message| {
            widget::button::custom(
                widget::row([
                    match icon {
                        Some(name) => widget::icon::from_name(name).symbolic(true).size(16).into(),
                        None => widget::Space::new().width(16).into(),
                    },
                    self.text(label, 13., alpha(palette.ink, reveal)),
                ])
                .spacing(10)
                .align_y(center),
            )
            .id(id.into())
            .width(Length::Fill)
            .height(MENU_ROW)
            .padding([6, 10])
            .class(capsule_button(palette, tint, reveal, Role::Ghost))
            .on_press(press)
        };
        let mut list = widget::column([item(
            "activity-rule-back",
            Some("go-previous-symbolic"),
            "Back",
            Message::MenuClose,
        )
        .into()])
        .spacing(MENU_GAP);
        for (id, label, rule) in [
            ("activity-rule-mute", "Mute this app", Some(AppRule::Mute)),
            ("activity-rule-always", "Always show", Some(AppRule::Always)),
            (
                "activity-rule-list",
                "List only, don’t pop up",
                Some(AppRule::ListOnly),
            ),
            ("activity-rule-reset", "Reset to default", None),
        ] {
            let chosen = current == rule;
            list = list.push(item(
                id,
                chosen.then_some("object-select-symbolic"),
                label,
                Message::NoteRule(rule),
            ));
        }
        list.width(Length::Fill).into()
    }
    /// The notifications, newest first with urgent ones on top: a row opens
    /// one, a group opens in place, and Clear all closes them together.
    /// Lines beyond the popup's room are counted rather than scrolled.
    /// Recent sits underneath.
    pub(super) fn list_view(
        &self,
        palette: Palette,
        tint: Color,
        reveal: f32,
    ) -> Element<'_, Message> {
        if self.menu.is_some() {
            return self.menu_view(palette, tint, reveal);
        }
        let center = cosmic::iced::Alignment::Center;
        let layout = self.list_layout(LIST_ROOM + self.popup_extra);
        let mut list = widget::column([]).spacing(ROW_GAP);
        for row in &layout.rows {
            list = list.push(match row {
                Row::Note(id) | Row::Member(id) => match self.note(*id) {
                    Some(n) => {
                        self.note_row(palette, tint, reveal, n, matches!(row, Row::Member(_)))
                    }
                    None => widget::Space::new().into(),
                },
                Row::Group(key, ids) => self.group_row(palette, tint, reveal, key, ids),
            });
        }
        if !self.notes.is_empty() {
            let more = layout.hidden;
            let footer = widget::row([
                widget::container(if more > 0 {
                    self.text(
                        format!("and {more} more"),
                        12.,
                        alpha(palette.quiet, reveal),
                    )
                } else {
                    widget::Space::new().into()
                })
                .width(Length::Fill)
                .padding([0, 10])
                .into(),
                self.label_button(
                    palette,
                    "activity-note-clear",
                    "Clear all",
                    tint,
                    reveal,
                    Role::Ghost,
                    Some(Message::NotesClear),
                ),
            ])
            .spacing(4)
            .align_y(center);
            list = list.push(widget::container(footer).padding(cosmic::iced::Padding {
                top: LIST_FOOTER - 32. - ROW_GAP,
                ..Default::default()
            }));
        }
        if self.has_recent() {
            list = list.push(self.recent_view(palette, tint, reveal));
        }
        list.width(Length::Fill).into()
    }
}
