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
/// Messages in the list: their size and the height of each line.
const MESSAGE_SIZE: f32 = 12.;
const MESSAGE_LINE: f32 = 16.;
/// A row's title line, tall enough for the buttons that take the age's
/// place under the pointer.
const TITLE_LINE: f32 = 22.;
/// Above and below a row's lines.
const ROW_PAD: f32 = 2.;
/// Between a title line and its message.
const LINE_GAP: f32 = 2.;
/// A row's small round buttons.
const CONTROL: f32 = 22.;
const CONTROL_GAP: f32 = 2.;
/// The most a title line gives its buttons: expand, More and Dismiss.
const TRAIL_ROOM: f32 = 3. * CONTROL + 2. * CONTROL_GAP;
/// Kept at the end of a group member's first line for its age, or its
/// expand and Dismiss buttons.
const MEMBER_TRAIL: f32 = 2. * CONTROL + CONTROL_GAP + 6.;
const TRAIL_GAP: f32 = 8.;
/// A group member's row with one line of message.
const MEMBER_ROW: f32 = 36.;
/// A row with a title line and two lines of message.
const TALL_ROW: f32 = 2. * ROW_PAD + TITLE_LINE + LINE_GAP + 2. * MESSAGE_LINE;
/// Lines an opened message shows at most.
const OPEN_LINES: usize = 12;
/// Where a row's text starts: its padding, the app icon and a gap.
const ROW_INDENT: f32 = 10. + 20. + 10.;
/// A row's button: icon, widget id, tooltip and what it does.
type Control = (&'static str, String, &'static str, Message);
/// The most the list asks of the card below its title line: five rows of
/// two lines, Clear all on a bar, and an open Recent section.
pub(super) const LIST_MAX: f32 = LIST_ROWS as f32 * (TALL_ROW + ROW_GAP) - ROW_GAP
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

/// A row of the list the pointer can be on.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RowId {
    Note(u32),
    Group(String),
}

/// How a message fits a row: its first line, then the rest on the line
/// below, cut short there unless the notification is opened in place.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Fit {
    pub first: String,
    pub rest: String,
    /// The rest needs more than its one line.
    pub cut: bool,
    /// Lines the rest takes opened in place, at most `OPEN_LINES`.
    pub open_lines: usize,
}
impl Fit {
    /// Lines the message takes in a row that is not opened.
    fn lines(&self) -> usize {
        usize::from(!self.first.is_empty()) + usize::from(!self.rest.is_empty())
    }
}

/// What a row says: a title line, none for a group member that would only
/// repeat its app, then its message.
struct Said {
    title: Option<String>,
    message: Fit,
}

type Measured = <cosmic::Renderer as cosmic::iced::advanced::text::Renderer>::Paragraph;
/// A message's paragraph, set as a row draws it.
fn paragraph(text: &str, width: f32, wrap: bool) -> Measured {
    use cosmic::iced::advanced::text::{self as t, Paragraph as _};
    Measured::with_text(t::Text {
        content: text,
        bounds: cosmic::iced::Size::new(width, f32::INFINITY),
        size: MESSAGE_SIZE.into(),
        line_height: t::LineHeight::Absolute(MESSAGE_LINE.into()),
        font: cosmic::font::default(),
        align_x: t::Alignment::Left,
        align_y: cosmic::iced::alignment::Vertical::Top,
        shaping: t::Shaping::Advanced,
        wrapping: if wrap {
            t::Wrapping::WordOrGlyph
        } else {
            t::Wrapping::None
        },
        ellipsize: t::Ellipsize::None,
    })
}
fn line_width(text: &str) -> f32 {
    use cosmic::iced::advanced::text::Paragraph as _;
    paragraph(text, f32::INFINITY, false).min_bounds().width
}
fn wrapped_lines(text: &str, width: f32) -> usize {
    use cosmic::iced::advanced::text::Paragraph as _;
    let height = paragraph(text, width.max(1.), true).min_bounds().height;
    ((height / MESSAGE_LINE).round() as usize).max(1)
}
/// Breaks a message after the last word that fits `first` wide; the rest
/// goes on the line below, `width` wide. Runs of spaces and line breaks
/// read as one space.
pub(super) fn fit_message(text: &str, first: f32, width: f32) -> Fit {
    let text = text.split_whitespace().collect::<Vec<_>>().join(" ");
    let fits = |end: usize| line_width(&text[..end]) <= first;
    if text.is_empty() || fits(text.len()) {
        return Fit {
            first: text,
            ..Fit::default()
        };
    }
    // The most whole words that fit, else as many characters, at least one.
    let spaces: Vec<usize> = text.match_indices(' ').map(|(at, _)| at).collect();
    let words = spaces.partition_point(|&end| fits(end));
    let cut = if words > 0 {
        spaces[words - 1]
    } else {
        let ends: Vec<usize> = text.char_indices().map(|(at, _)| at).skip(1).collect();
        let chars = ends.partition_point(|&end| fits(end));
        ends.get(chars.saturating_sub(1))
            .copied()
            .unwrap_or(text.len())
    };
    let rest = text[cut..].trim_start().to_owned();
    Fit {
        first: text[..cut].trim_end().to_owned(),
        cut: !rest.is_empty() && line_width(&rest) > width,
        open_lines: if rest.is_empty() {
            0
        } else {
            wrapped_lines(&rest, width).min(OPEN_LINES)
        },
        rest,
    }
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
    /// Live notifications not shown: older ones after the rows, and newer
    /// ones before them once the list has moved on.
    pub hidden: usize,
    /// Of those, the older ones "Show N more" brings in.
    pub older: usize,
    /// Where the list starts to show them.
    pub next: usize,
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
                older: 0,
                next: 0,
                height: MENU_HEIGHT,
            };
        }
        let live = !self.notes.is_empty();
        let mut fixed = if live && !self.clear_on_title() {
            LIST_FOOTER
        } else {
            0.
        };
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
            Row::Note(id) | Row::Member(id) => self.note(*id).map_or(ROW_HEIGHT, |n| {
                self.note_height(n, matches!(row, Row::Member(_)))
            }),
            Row::Group(_, ids) => ids
                .first()
                .and_then(|id| self.note(*id))
                .map_or(ROW_HEIGHT, |n| {
                    self.titled_height(&self.group_message(n), false)
                }),
        };
        // Up to five lines from `start`, with an open group's own rows
        // beyond them, as far as the room goes; always one. Gives back
        // where they end and the height they take.
        let take = |start: usize, budget: f32| {
            let (mut used, mut lines, mut end) = (0., 0, start);
            for row in &all[start..] {
                let lead = !matches!(row, Row::Member(_));
                let next = used + height(row) + if end == start { 0. } else { ROW_GAP };
                if end > start && (next > budget + 0.5 || (lead && lines == LIST_ROWS)) {
                    break;
                }
                lines += usize::from(lead);
                used = next;
                end += 1;
            }
            (start, end, used)
        };
        // An open group, or a notification opened in place, that would run
        // out of room moves to the top.
        let head = all.iter().position(|row| match row {
            Row::Group(key, _) => self.open_group.as_deref() == Some(key.as_str()),
            Row::Note(id) => self.expanded == Some(*id),
            Row::Member(_) => false,
        });
        // "Show N more" moved the list on to older ones.
        let from = self.list_from.min(all.len().saturating_sub(1));
        let pick = |budget: f32| {
            let (start, end, used) = take(from, budget);
            if let Some(head) = head {
                let members = all[head + 1..]
                    .iter()
                    .take_while(|row| matches!(row, Row::Member(_)))
                    .count();
                if head > start && end < head + 1 + members {
                    return take(head, budget);
                }
            }
            (start, end, used)
        };
        let count = |rows: &[Row]| -> usize {
            rows.iter()
                .map(|row| match row {
                    Row::Note(_) | Row::Member(_) => 1,
                    Row::Group(key, ids) if self.open_group.as_deref() != Some(key.as_str()) => {
                        ids.len()
                    }
                    Row::Group(..) => 0,
                })
                .sum()
        };
        let hidden =
            |start: usize, end: usize| self.notes.len().saturating_sub(count(&all[start..end]));
        let (mut start, mut end, mut used) = pick(room - fixed);
        // On an island, those that are not shown get a line of their own;
        // on a bar, they sit beside Clear all.
        if hidden(start, end) > 0 && self.clear_on_title() {
            fixed += MORE_LINE;
            (start, end, used) = pick(room - fixed);
        }
        let hidden = hidden(start, end);
        Layout {
            hidden,
            older: count(&all[end..]).min(hidden),
            next: end,
            rows: all[start..end].to_vec(),
            height: used + fixed,
        }
    }
    /// Clear all is on the title line, beside Collapse. A Standard bar's
    /// title line keeps the panel's width, too narrow for it beside the
    /// title, so there it closes the list instead.
    pub(super) fn clear_on_title(&self) -> bool {
        !self.frame().attached
    }
    /// The list's width inside the open card.
    fn list_inner(&self) -> f32 {
        LIST_WIDTH - if self.frame().attached { 24. } else { 28. }
    }
    /// A row's message column: the list less the row's padding and icon.
    fn message_width(&self) -> f32 {
        self.list_inner() - ROW_INDENT - 10.
    }
    /// A message as it fits a row, measured once for each text and width.
    fn fit(&self, text: &str, first: f32) -> Fit {
        let width = self.message_width();
        let key = (text.to_owned(), first.round() as u32, width.round() as u32);
        if let Some(fit) = self.fits.borrow().get(&key) {
            return fit.clone();
        }
        let fit = fit_message(text, first, width);
        let mut fits = self.fits.borrow_mut();
        if fits.len() >= 256 {
            fits.clear();
        }
        fits.insert(key, fit.clone());
        fit
    }
    /// What a notification's row says. The app's icon already names the
    /// app, so the message no longer repeats it.
    fn said(&self, n: &Note, member: bool) -> Said {
        let body = if self.snapshot.preferences.notification_text {
            n.body.trim()
        } else {
            ""
        };
        let title = n.title.trim();
        let own = !title.is_empty() && !title.eq_ignore_ascii_case(n.app.trim());
        let width = self.message_width();
        if member && !own {
            let message = if body.is_empty() { n.app.trim() } else { body };
            return Said {
                title: None,
                message: self.fit(message, width - MEMBER_TRAIL - TRAIL_GAP),
            };
        }
        let message = match (body.is_empty(), own && !member) {
            (false, _) => body,
            (true, true) => n.app.trim(),
            (true, false) => "",
        };
        Said {
            title: Some(if title.is_empty() {
                n.app.clone()
            } else {
                title.to_owned()
            }),
            message: self.fit(message, width),
        }
    }
    /// A group's message: the newest notification's, until the group
    /// opens and its members say it.
    fn group_message(&self, newest: &Note) -> Fit {
        if self.open_group.as_deref() == Some(newest.app_key()) {
            return Fit::default();
        }
        let body = if self.snapshot.preferences.notification_text {
            newest.body.trim()
        } else {
            ""
        };
        let title = newest.title.trim();
        let own = !title.is_empty() && !title.eq_ignore_ascii_case(newest.app.trim());
        let message = match (own, body.is_empty()) {
            (true, false) => format!("{title} · {body}"),
            (true, true) => title.to_owned(),
            (false, _) => body.to_owned(),
        };
        self.fit(&message, self.message_width())
    }
    /// A row with a title line, never shorter than one with a line of
    /// message.
    fn titled_height(&self, fit: &Fit, open: bool) -> f32 {
        let lines = if open {
            usize::from(!fit.first.is_empty()) + fit.open_lines
        } else {
            fit.lines()
        };
        let message = if lines == 0 {
            0.
        } else {
            LINE_GAP + lines as f32 * MESSAGE_LINE
        };
        (2. * ROW_PAD + TITLE_LINE + message).max(ROW_HEIGHT)
    }
    /// The app's own buttons under a row: in an open group, or opened in
    /// place.
    fn actions_height(&self, n: &Note, member: bool) -> f32 {
        if (member || self.expanded == Some(n.id)) && !n.actions.is_empty() {
            ACTION_ROW
        } else {
            0.
        }
    }
    /// A notification's row and the app's buttons under it.
    fn note_height(&self, n: &Note, member: bool) -> f32 {
        let said = self.said(n, member);
        let open = self.expanded == Some(n.id);
        let row = match said.title {
            Some(_) => self.titled_height(&said.message, open),
            None => {
                let rest = match (said.message.rest.is_empty(), open) {
                    (true, _) => 0,
                    (false, true) => said.message.open_lines,
                    (false, false) => 1,
                };
                (2. * ROW_PAD + TITLE_LINE + rest as f32 * MESSAGE_LINE).max(MEMBER_ROW)
            }
        };
        row + self.actions_height(n, member)
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
        // Their jobs go too, downloads and copies alike.
        self.ui.jobs.forget(|j| {
            j.owners
                .iter()
                .find_map(|(app, entry)| prefs.rule_for(app, entry))
                == Some(AppRule::Mute)
        });
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
            Message::NotesPage(from) => {
                self.list_from = from;
                self.expanded = None;
                self.reveal_deadline = None;
                self.held = true;
                return self.fit_list();
            }
            Message::RowHover(row, true) => self.hover_row = Some(row),
            Message::RowHover(row, false) => {
                if self.hover_row.as_ref() == Some(&row) {
                    self.hover_row = None;
                }
            }
            Message::NoteExpand(id) => {
                self.expanded = (self.expanded != Some(id)).then_some(id);
                self.reveal_deadline = None;
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
    /// A row's small round button on its title line.
    fn control<'a>(
        &self,
        palette: Palette,
        tint: Color,
        reveal: f32,
        (name, id, label, press): Control,
    ) -> Element<'a, Message> {
        self.tip(
            palette,
            widget::button::icon(widget::icon::from_name(name).symbolic(true))
                .id(id.into())
                .icon_size(12)
                .padding(5)
                .width(CONTROL)
                .height(CONTROL)
                .class(capsule_button(palette, tint, reveal, Role::Ghost))
                .on_press(press),
            label,
            widget::tooltip::Position::Top,
        )
    }
    /// A row shows its buttons while the pointer is on it, and every row
    /// does after Tab. The rest of the time the message keeps the width.
    fn controls_shown(&self, row: &RowId) -> bool {
        self.keyed || self.hover_row.as_ref() == Some(row)
    }
    /// Where a title line ends: the age, or room for the buttons laid over
    /// the row.
    fn trailing<'a>(
        &self,
        palette: Palette,
        reveal: f32,
        at_ms: i64,
        controls: Option<usize>,
    ) -> Element<'a, Message> {
        match controls {
            Some(count) => widget::Space::new()
                .width(count as f32 * CONTROL + count.saturating_sub(1) as f32 * CONTROL_GAP)
                .into(),
            None => self.text(
                super::note_age(at_ms, self.now_ms, self.format_24h),
                11.,
                alpha(palette.quiet, reveal),
            ),
        }
    }
    /// A row with its buttons laid over the end of its title line. Laid
    /// over rather than inside, the row comes before its buttons for Tab,
    /// as they read.
    fn with_controls<'a>(
        &self,
        palette: Palette,
        tint: Color,
        reveal: f32,
        row: Element<'a, Message>,
        controls: Option<Vec<Control>>,
        end: f32,
    ) -> Element<'a, Message> {
        let Some(controls) = controls else {
            return row;
        };
        let mut buttons = widget::row([]).spacing(CONTROL_GAP);
        for control in controls {
            buttons = buttons.push(self.control(palette, tint, reveal, control));
        }
        let over = widget::container(
            widget::row([
                widget::Space::new().width(Length::Fill).into(),
                buttons.into(),
            ])
            .align_y(cosmic::iced::Alignment::Center),
        )
        .width(Length::Fill)
        .height(ROW_PAD + TITLE_LINE)
        .padding([ROW_PAD, end, 0., 0.]);
        cosmic::iced::widget::stack![row, over].into()
    }
    /// One line of a message, set as it was measured.
    fn message_text<'a>(&self, value: String, color: Color) -> Element<'a, Message> {
        use cosmic::iced::advanced::text as t;
        widget::text(value)
            .size(MESSAGE_SIZE)
            .line_height(t::LineHeight::Absolute(MESSAGE_LINE.into()))
            .shaping(t::Shaping::Advanced)
            .wrapping(t::Wrapping::None)
            .ellipsize(t::Ellipsize::End(t::EllipsizeHeightLimit::Lines(1)))
            .class(cosmic::theme::Text::Color(color))
            .into()
    }
    /// What follows the first line: one more line cut short, or opened, all
    /// of it.
    fn message_rest<'a>(
        &self,
        fit: &Fit,
        open: bool,
        color: Color,
    ) -> Option<Element<'a, Message>> {
        use cosmic::iced::advanced::text as t;
        if fit.rest.is_empty() {
            return None;
        }
        if !open {
            return Some(self.message_text(fit.rest.clone(), color));
        }
        Some(
            widget::container(
                widget::text(fit.rest.clone())
                    .size(MESSAGE_SIZE)
                    .line_height(t::LineHeight::Absolute(MESSAGE_LINE.into()))
                    .shaping(t::Shaping::Advanced)
                    .wrapping(t::Wrapping::WordOrGlyph)
                    .class(cosmic::theme::Text::Color(color)),
            )
            .width(Length::Fill)
            .height(fit.open_lines as f32 * MESSAGE_LINE)
            .clip(true)
            .into(),
        )
    }
    /// A title line: the title, a count for a group, then the age or the
    /// row's buttons.
    fn title_line<'a>(
        &self,
        title: String,
        ink: Color,
        reveal: f32,
        count: Option<(usize, Palette)>,
        trailing: Element<'a, Message>,
    ) -> Element<'a, Message> {
        let center = cosmic::iced::Alignment::Center;
        let title = self.text(title, 13., alpha(ink, reveal));
        let mut line = widget::row([]).spacing(6).align_y(center);
        line = match count {
            Some((count, palette)) => line
                .push(
                    widget::container(title)
                        .width(Length::Shrink)
                        .max_width(self.message_width() - TRAIL_ROOM - 54.),
                )
                .push(
                    widget::container(self.text(
                        count.to_string(),
                        11.,
                        alpha(palette.ink, reveal),
                    ))
                    .padding([0, 6])
                    .class(cosmic::theme::Container::custom(move |_| {
                        cosmic::widget::container::Style {
                            background: Some(alpha(palette.ink, 0.16 * reveal).into()),
                            border: cosmic::iced::Border {
                                radius: 8.0.into(),
                                ..Default::default()
                            },
                            ..Default::default()
                        }
                    })),
                )
                .push(widget::Space::new().width(Length::Fill)),
            None => line.push(widget::container(title).width(Length::Fill)),
        };
        widget::container(line.push(trailing))
            .height(TITLE_LINE)
            .align_y(center)
            .into()
    }
    /// One notification: a row that opens it, with its age, and More and
    /// Dismiss under the pointer. Opened in place it shows all its message
    /// and the app's own buttons.
    fn note_row<'a>(
        &self,
        palette: Palette,
        tint: Color,
        reveal: f32,
        n: &Note,
        member: bool,
    ) -> Element<'a, Message> {
        let center = cosmic::iced::Alignment::Center;
        let row_id = RowId::Note(n.id);
        let said = self.said(n, member);
        let open = self.expanded == Some(n.id);
        let fit = &said.message;
        let mut controls: Vec<Control> = Vec::new();
        if open || fit.cut || (!member && !n.actions.is_empty()) {
            controls.push((
                if open {
                    "pan-up-symbolic"
                } else {
                    "pan-down-symbolic"
                },
                format!("activity-note-row-expand-{}", n.id),
                if open { "Show less" } else { "Show all" },
                Message::NoteExpand(n.id),
            ));
        }
        if !member {
            controls.push((
                "view-more-symbolic",
                format!("activity-note-row-more-{}", n.id),
                "More",
                Message::NoteMenu(n.id),
            ));
        }
        controls.push((
            "window-close-symbolic",
            format!("activity-note-row-dismiss-{}", n.id),
            "Dismiss",
            Message::NoteDismiss(n.id),
        ));
        let controls = self.controls_shown(&row_id).then_some(controls);
        let trailing = self.trailing(palette, reveal, n.at_ms, controls.as_ref().map(Vec::len));
        let ink = if n.urgent {
            palette.warning()
        } else {
            palette.ink
        };
        let mut body = widget::column([]).spacing(0);
        match said.title {
            Some(title) => {
                body = body.push(self.title_line(title, ink, reveal, None, trailing));
                let quiet = alpha(palette.quiet, reveal);
                let mut message = widget::column([]);
                if !fit.first.is_empty() {
                    message = message.push(self.message_text(fit.first.clone(), quiet));
                }
                if let Some(rest) = self.message_rest(fit, open, quiet) {
                    message = message.push(rest);
                }
                if !fit.first.is_empty() {
                    body = body.push(widget::container(message).padding([LINE_GAP, 0., 0., 0.]));
                }
            }
            // A group member that would only repeat its app: the message
            // leads, its first line beside the age.
            None => {
                let ink = alpha(ink, reveal);
                // The first line sits low in the title line's height, so
                // the next follows at the message's own spacing.
                body = body.push(
                    widget::container(
                        widget::row([
                            widget::container(self.message_text(fit.first.clone(), ink))
                                .width(Length::Fill)
                                .height(TITLE_LINE)
                                .align_y(cosmic::iced::Alignment::End)
                                .into(),
                            widget::container(trailing)
                                .width(MEMBER_TRAIL)
                                .height(TITLE_LINE)
                                .align_x(cosmic::iced::Alignment::End)
                                .align_y(cosmic::iced::Alignment::End)
                                .padding([0., 0., 1., 0.])
                                .into(),
                        ])
                        .spacing(TRAIL_GAP)
                        .align_y(center),
                    )
                    .height(TITLE_LINE)
                    .align_y(center),
                );
                if let Some(rest) = self.message_rest(fit, open, ink) {
                    body = body.push(rest);
                }
            }
        }
        let content: Element<'a, Message> = if member {
            body.into()
        } else {
            widget::row([
                widget::container(self.note_icon(Some(n), 20))
                    .padding([(TITLE_LINE - 20.) / 2., 0.])
                    .into(),
                body.into(),
            ])
            .spacing(10)
            .into()
        };
        let height = self.note_height(n, member) - self.actions_height(n, member);
        let open_row = widget::button::custom(
            widget::container(content)
                .height(Length::Fill)
                .align_y(center),
        )
        .id(format!("activity-note-row-{}", n.id).into())
        .width(Length::Fill)
        .height(height)
        .padding(if member {
            [ROW_PAD, 10., ROW_PAD, ROW_INDENT]
        } else {
            [ROW_PAD, 10., ROW_PAD, 10.]
        })
        .class(capsule_button(
            palette,
            tint,
            reveal,
            if controls.is_some() && !self.keyed {
                Role::Lit
            } else {
                Role::Ghost
            },
        ))
        // Like the popup it stands for: the notification itself, or
        // its card when the app gave it nothing to open.
        .on_press(if n.default {
            Message::NoteAction(n.id, "default".into())
        } else {
            Message::Select(Kind::Notification(n.id))
        });
        let open_row = self.with_controls(palette, tint, reveal, open_row.into(), controls, 10.);
        let mut whole = widget::column([open_row]).spacing(ACTION_ROW - 28.);
        if self.actions_height(n, member) > 0. {
            // In an open group, or opened in place, the app's own buttons.
            let mut buttons =
                widget::row([widget::Space::new().width(ROW_INDENT - 4.).into()]).spacing(4);
            for (index, (key, label)) in n.actions.iter().take(2).enumerate() {
                buttons = buttons.push(
                    widget::button::custom(
                        widget::container(self.text(
                            label.clone(),
                            12.,
                            alpha(palette.ink, reveal),
                        ))
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
            whole = whole.push(buttons);
        }
        self.hovered(whole.into(), row_id, Message::NoteMenu(n.id))
    }
    /// Two or more from one app: the app and a count, then the newest
    /// message beneath. It opens in place.
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
        let row_id = RowId::Group(key.to_owned());
        let open = self.open_group.as_deref() == Some(key);
        let fit = self.group_message(newest);
        let controls = self.controls_shown(&row_id).then(|| {
            vec![
                (
                    "view-more-symbolic",
                    format!("activity-note-group-more-{key}"),
                    "More",
                    Message::NoteMenu(newest.id),
                ),
                (
                    "window-close-symbolic",
                    format!("activity-note-group-dismiss-{key}"),
                    "Dismiss all",
                    Message::GroupDismiss(key.to_owned()),
                ),
            ]
        });
        let trailing = self.trailing(
            palette,
            reveal,
            newest.at_ms,
            controls.as_ref().map(Vec::len),
        );
        let caret: Element<'a, Message> = widget::icon::from_name(if open {
            "pan-up-symbolic"
        } else {
            "pan-down-symbolic"
        })
        .symbolic(true)
        .size(12)
        .into();
        let quiet = alpha(palette.quiet, reveal);
        let mut message = widget::column([]);
        if !fit.first.is_empty() {
            message = message.push(self.message_text(fit.first.clone(), quiet));
        }
        if let Some(rest) = self.message_rest(&fit, false, quiet) {
            message = message.push(rest);
        }
        let mut body = widget::column([self.title_line(
            newest.app.clone(),
            palette.ink,
            reveal,
            Some((ids.len(), palette)),
            widget::row([trailing, caret])
                .spacing(6)
                .align_y(center)
                .into(),
        )]);
        if !fit.first.is_empty() {
            body = body.push(widget::container(message).padding([LINE_GAP, 0., 0., 0.]));
        }
        let toggle = widget::button::custom(
            widget::container(
                widget::row([
                    widget::container(self.note_icon(Some(newest), 20))
                        .padding([(TITLE_LINE - 20.) / 2., 0.])
                        .into(),
                    body.into(),
                ])
                .spacing(10),
            )
            .height(Length::Fill)
            .align_y(center),
        )
        .id(format!("activity-note-group-{key}").into())
        .width(Length::Fill)
        .height(self.titled_height(&fit, false))
        .padding([ROW_PAD, 10.])
        .class(capsule_button(
            palette,
            tint,
            reveal,
            match (open, controls.is_some() && !self.keyed) {
                (true, _) => Role::Quiet,
                (false, true) => Role::Lit,
                (false, false) => Role::Ghost,
            },
        ))
        .on_press(Message::GroupToggle(key.to_owned()));
        // The buttons stop short of the caret.
        let toggle = self.with_controls(
            palette,
            tint,
            reveal,
            toggle.into(),
            controls,
            10. + 12. + 6.,
        );
        self.hovered(toggle, row_id, Message::NoteMenu(newest.id))
    }
    /// A row that says when the pointer is on it, and offers its app's
    /// rules on right-click. Motion counts as well as entering: after a
    /// dismissal the next row slides under a pointer that never entered it.
    fn hovered<'a>(
        &self,
        row: Element<'a, Message>,
        id: RowId,
        menu: Message,
    ) -> Element<'a, Message> {
        let moved = id.clone();
        widget::mouse_area(row)
            .on_right_press(menu)
            .on_enter(Message::RowHover(id.clone(), true))
            .on_move(move |_| Message::RowHover(moved.clone(), true))
            .on_exit(Message::RowHover(id, false))
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
                Role::Quiet,
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
            ("activity-rule-reset", "Default", None),
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
    /// one and a group opens in place. Clear all closes them together, on
    /// the title line or, on a bar, under the rows. Lines beyond the
    /// popup's room are a page away: "Show N more" swaps in the older ones
    /// and "Back to newest" returns, so each page keeps its Tab order.
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
        // Those not shown are a press away: older ones replace the rows,
        // and the newest come back the same way.
        let more = (layout.hidden > 0).then(|| -> Element<'_, Message> {
            let page = |id: &'static str, label: String, from: usize| -> Element<'_, Message> {
                widget::button::custom(
                    widget::container(self.text(label, 12., alpha(palette.ink, 0.8 * reveal)))
                        .height(Length::Fill)
                        .align_y(center),
                )
                .id(id.into())
                .height(Length::Fixed(MORE_LINE - ROW_GAP))
                .padding([0, 10])
                .class(capsule_button(palette, tint, reveal, Role::Ghost))
                .on_press(Message::NotesPage(from))
                .into()
            };
            let mut pages = widget::row([]).spacing(2).align_y(center);
            if layout.older > 0 {
                pages = pages.push(page(
                    "activity-note-older",
                    format!("Show {} more", layout.older),
                    layout.next,
                ));
            }
            if layout.hidden > layout.older {
                pages = pages.push(page("activity-note-newest", "Back to newest".into(), 0));
            }
            pages.into()
        });
        if self.clear_on_title() || self.notes.is_empty() {
            if let Some(more) = more {
                list = list.push(
                    widget::container(more)
                        .width(Length::Fill)
                        .height(MORE_LINE - ROW_GAP)
                        .align_y(cosmic::iced::Alignment::End),
                );
            }
        } else {
            let footer = widget::row([
                widget::container(more.unwrap_or_else(|| widget::Space::new().into()))
                    .width(Length::Fill)
                    .into(),
                self.label_button(
                    palette,
                    "activity-note-clear",
                    "Clear all",
                    tint,
                    reveal,
                    Role::Quiet,
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
