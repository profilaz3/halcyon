//! The desktop tools — Music, Notes and Focus — on surfaces of their own.
//!
//! Each tool is placed by its own position in every layout mode, like
//! Glance, and is recreated alone when its size changes, so showing or
//! resizing a tool never disturbs the other cards. Surfaces stay below
//! windows and never take the keyboard, except while the note is being
//! edited: Edit lifts the note above windows with the keyboard until Save
//! or Cancel. Hiding the note or arranging the desktop pauses an unsaved
//! draft without the keyboard; only Resume (or Edit) takes it again.
use super::*;
use crate::tools::cards::{self, Action};
use crate::tools::{focus, music, notes};
use cosmic::iced::platform_specific::shell::commands::layer_surface::{
    destroy_layer_surface, set_keyboard_interactivity, set_layer,
};
use cosmic::widget::image::Handle;
use cosmic::widget::text_editor;
use std::collections::BTreeMap;

/// Artwork thumbnails kept for recently shown tracks.
const ART_CACHE: usize = 6;

pub(super) struct NoteDraft {
    pub content: text_editor::Content,
    /// The saved text editing started from; a save checks it is unchanged.
    pub base: String,
    /// The user is typing: the note holds the keyboard. A paused draft waits
    /// on the card for Resume, Save or Cancel.
    pub typing: bool,
    /// A newer note was saved elsewhere since editing started. Only an
    /// explicit Save replaces it; nothing saves it implicitly.
    pub conflicted: bool,
    /// A save is under way (off the UI thread).
    pub saving: bool,
}

pub(super) struct Tools {
    /// `None` until the watcher first reports (or while Music is hidden).
    pub feed: Option<music::Feed>,
    /// Players by how recently they started playing.
    pub recent: Vec<String>,
    /// A player chosen on the card for this session.
    pub pinned: Option<String>,
    pub music_problem: bool,
    art: Vec<(String, Option<Handle>)>,
    art_loading: Option<String>,
    pub note: Result<notes::Note, String>,
    pub draft: Option<NoteDraft>,
    pub note_problem: Option<String>,
    checking_note: bool,
    pub focus: focus::Session,
    /// The size each tool surface was created with.
    geometry: BTreeMap<Widget, [u32; 2]>,
    /// Whether the Music card is on the desktop now.
    pub music_shown: bool,
    pub spectrum_key: Option<String>,
    pub audio_bins: [f32; crate::tools::visualizer::BINS],
}

impl Tools {
    pub(super) fn load() -> Self {
        Self {
            feed: None,
            recent: Vec::new(),
            pinned: None,
            music_problem: false,
            art: Vec::new(),
            art_loading: None,
            note: notes::load(),
            draft: None,
            note_problem: None,
            checking_note: false,
            focus: focus::Session::load(),
            geometry: BTreeMap::new(),
            music_shown: false,
            spectrum_key: None,
            audio_bins: [0.; crate::tools::visualizer::BINS],
        }
    }
    pub(super) fn sample() -> Self {
        Self {
            feed: None,
            recent: Vec::new(),
            pinned: None,
            music_problem: false,
            art: Vec::new(),
            art_loading: None,
            note: Ok(notes::Note::default()),
            draft: None,
            note_problem: None,
            checking_note: false,
            focus: focus::Session::default(),
            geometry: BTreeMap::new(),
            music_shown: false,
            spectrum_key: None,
            audio_bins: [0.; crate::tools::visualizer::BINS],
        }
    }
    fn players(&self) -> &[music::Player] {
        match &self.feed {
            Some(music::Feed::Players(players)) => players,
            _ => &[],
        }
    }
    fn art(&self, url: &str) -> Option<&Option<Handle>> {
        self.art
            .iter()
            .find(|(cached, _)| cached == url)
            .map(|(_, handle)| handle)
    }
    /// Changes whenever a tool shows something different; capture mode only.
    pub(super) fn capture_key(&self) -> String {
        format!(
            "{:?}:{:?}:{}:{:?}:{:?}",
            self.feed.as_ref().map(|feed| match feed {
                music::Feed::Players(players) => players
                    .iter()
                    .map(|p| format!("{}{:?}{}", p.bus, p.status, p.title))
                    .collect::<Vec<_>>()
                    .join(","),
                music::Feed::Unavailable(reason) => reason.clone(),
            }),
            self.art.len(),
            self.note.as_ref().map(|note| note.text.len()).unwrap_or(0),
            self.draft.is_some(),
            self.focus,
        )
    }
}

fn on_desktop(action: Action) -> Message {
    Message::Tool(action)
}

impl Cards {
    /// Whether a tool is on the desktop. Music shows while it has something
    /// to show (see `music_present`), and always while arranging.
    pub(super) fn tool_shown(&self, widget: Widget) -> bool {
        self.widget_available(widget)
            && self.config.widget_visible(widget)
            && (widget != Widget::Music || self.layout_editor.active || self.music_present())
    }

    /// Something the Music card can show: a player playing, or paused with
    /// a track loaded (to see it and resume). Stopped or empty players don't
    /// count.
    fn music_present(&self) -> bool {
        self.tools
            .players()
            .iter()
            .any(|player| match player.status {
                music::Playback::Playing => true,
                music::Playback::Paused => {
                    !player.title.trim().is_empty() || player.art_url.is_some()
                }
                music::Playback::Stopped => false,
            })
    }

    /// Brings the Music card onto the desktop or takes it off when that
    /// changed (playback, the grace running out, arranging).
    pub(super) fn sync_music_shown(&mut self) -> Task<Message> {
        let shown = self.tool_shown(Widget::Music);
        if shown == self.tools.music_shown {
            return Task::none();
        }
        self.tools.music_shown = shown;
        Task::batch(self.sync_tool_surfaces())
    }

    pub(super) fn current_player(&self) -> Option<&music::Player> {
        music::choose(
            self.tools.players(),
            self.tools.pinned.as_deref(),
            self.config.music.player.as_deref(),
            &self.tools.recent,
        )
    }

    pub(super) fn tool_card(&self, widget: Widget) -> Element<'_, Message> {
        match widget {
            Widget::Music => {
                let player = self.current_player();
                let art = player
                    .filter(|_| self.config.music_artwork())
                    .and_then(|player| player.art_url.as_deref())
                    .and_then(|url| self.tools.art(url))
                    .and_then(Option::as_ref);
                cards::music_visualized(
                    cards::MusicView {
                        feed: self.tools.feed.as_ref(),
                        player,
                        art,
                        now: Instant::now(),
                        manually_selected: self.tools.pinned.is_some()
                            || self.config.music.player.is_some(),
                        problem: self.tools.music_problem,
                    },
                    &self.config,
                    Some(on_desktop),
                    Some(self.widget_size(widget)),
                    self.tools.audio_bins,
                )
            }
            Widget::Notes => cards::notes(
                cards::NotesView {
                    note: self.tools.note.as_ref().map_err(String::as_str),
                    editing: self.tools.draft.as_ref().map(|draft| &draft.content),
                    paused: self.tools.draft.as_ref().is_some_and(|draft| !draft.typing),
                    problem: self.tools.note_problem.as_deref(),
                },
                &self.config,
                (!self.tools.checking_note).then_some(on_desktop as fn(Action) -> Message),
                Some(self.widget_size(widget)),
            ),
            _ => cards::focus(
                &self.tools.focus,
                &self.config,
                focus::now_ms(),
                &focus::today(),
                Some(on_desktop),
                Some(self.widget_size(widget)),
            ),
        }
    }

    /// A new list of players: remember who started playing and fetch the
    /// followed track's artwork.
    pub(super) fn music_feed(&mut self, mut feed: music::Feed) -> Task<Message> {
        if let music::Feed::Players(players) = &mut feed {
            let before = self.tools.players().to_vec();
            music::reconcile_timing(&before, players);
            music::remember(&mut self.tools.recent, &before, players);
            if self
                .tools
                .pinned
                .as_ref()
                .is_some_and(|bus| !players.iter().any(|player| player.bus == *bus))
            {
                self.tools.pinned = None;
            }
        }
        self.tools.feed = Some(feed);
        self.music_art_task()
    }

    fn music_art_task(&mut self) -> Task<Message> {
        if !self.config.music_artwork() || self.tools.art_loading.is_some() {
            return Task::none();
        }
        let Some(url) = self
            .current_player()
            .and_then(|player| player.art_url.clone())
        else {
            return Task::none();
        };
        if self.tools.art(&url).is_some() {
            return Task::none();
        }
        self.tools.art_loading = Some(url.clone());
        cosmic::task::future(async move {
            let result = music::load_art(url.clone()).await;
            Message::MusicArt(url, result)
        })
    }

    pub(super) fn music_art(
        &mut self,
        url: String,
        result: Result<music::Art, String>,
    ) -> Task<Message> {
        if self.tools.art_loading.as_deref() == Some(url.as_str()) {
            self.tools.art_loading = None;
        }
        // A failed picture is remembered too, so it is not fetched again.
        let handle = result
            .map_err(|error| eprintln!("Music artwork: {error}"))
            .ok()
            .map(|art| Handle::from_rgba(art.width, art.height, art.rgba));
        self.tools.art.retain(|(cached, _)| *cached != url);
        self.tools.art.insert(0, (url, handle));
        self.tools.art.truncate(ART_CACHE);
        // The followed track may have changed while this one loaded.
        self.music_art_task()
    }

    fn music_command(&self, request: music::TransportRequest) -> Task<Message> {
        cosmic::task::future(async move {
            Message::MusicControlDone(music::perform(music::Control::Transport(request)).await)
        })
    }

    pub(super) fn reload_note(&mut self) {
        if self.tools.draft.is_none() && !self.tools.checking_note {
            let note = notes::load();
            if note != self.tools.note {
                self.tools.note_problem = None;
            }
            self.tools.note = note;
        }
    }

    /// Whether the note surface holds the keyboard above windows.
    fn note_lifted(&self) -> bool {
        self.tools.draft.as_ref().is_some_and(|draft| draft.typing)
            && self.config.widget_visible(Widget::Notes)
            && self.widget_available(Widget::Notes)
            && self.placed_position(Widget::Notes)[0] >= 0.
            && !(self.layout_editor.active && self.arranging_widget(Widget::Notes))
    }

    /// Hiding the note or arranging the desktop pauses its draft: the
    /// keyboard is released and isn't taken back when the note returns.
    pub(super) fn pause_note(&mut self) {
        if let Some(draft) = &mut self.tools.draft {
            draft.typing = false;
        }
    }

    /// Puts the note surface in the mode its state calls for; nothing to
    /// do without a draft, as it never leaves its resting mode then.
    pub(super) fn note_surface_mode(&self) -> Task<Message> {
        if self.tools.draft.is_none() {
            return Task::none();
        }
        self.note_editing(self.note_lifted())
    }

    fn note_editing(&self, editing: bool) -> Task<Message> {
        let id = self.widget_id(Widget::Notes);
        let mut tasks = vec![
            set_layer(id, if editing { Layer::Top } else { Layer::Bottom }),
            set_keyboard_interactivity(
                id,
                if editing {
                    KeyboardInteractivity::Exclusive
                } else {
                    KeyboardInteractivity::None
                },
            ),
        ];
        if editing {
            tasks.push(cosmic::iced::widget::operation::focus(
                cosmic::widget::Id::new(cards::NOTE_EDITOR),
            ));
        }
        Task::batch(tasks)
    }

    /// Saves the draft; a newer note made elsewhere is kept and explained,
    /// and the next Save replaces it.
    /// Saves the draft off the UI thread: a slow disk or another writer
    /// holding the notes lock never freezes the widgets or traps the
    /// keyboard. The result arrives as `NoteSaved`.
    pub(super) fn save_note(&mut self) -> Task<Message> {
        if self.tools.checking_note {
            return Task::none();
        }
        let Some(draft) = &mut self.tools.draft else {
            return Task::none();
        };
        if draft.saving {
            return Task::none();
        }
        let text = draft.content.text();
        // The editor reports a trailing newline for its last line.
        let text = text.strip_suffix('\n').unwrap_or(&text).to_string();
        let base = draft.base.clone();
        draft.saving = true;
        #[cfg(test)]
        {
            let result = notes::save(&base, &text);
            self.note_saved(text, result)
        }
        #[cfg(not(test))]
        cosmic::task::future(async move {
            let saved = text.clone();
            let result = tokio::task::spawn_blocking(move || notes::save(&base, &text))
                .await
                .unwrap_or_else(|_| {
                    Err(notes::SaveError::Failed(
                        "saving stopped unexpectedly".into(),
                    ))
                });
            Message::NoteSaved(saved, result)
        })
    }

    /// The draft as an implicit save would keep it (arranging, hiding): never
    /// over a newer note saved elsewhere.
    pub(super) fn save_note_quietly(&mut self) -> Task<Message> {
        if self
            .tools
            .draft
            .as_ref()
            .is_some_and(|draft| draft.conflicted)
        {
            return Task::none();
        }
        self.save_note()
    }

    pub(super) fn note_saved(
        &mut self,
        text: String,
        result: Result<notes::Note, notes::SaveError>,
    ) -> Task<Message> {
        let Some(draft) = &mut self.tools.draft else {
            // Cancelled meanwhile: whatever was saved is the note now.
            if let Ok(note) = result {
                self.tools.note = Ok(note);
            }
            return Task::none();
        };
        draft.saving = false;
        match result {
            Ok(note) => {
                let current = draft.content.text();
                let current = current.strip_suffix('\n').unwrap_or(&current);
                self.tools.note_problem = None;
                if current == text {
                    self.tools.note = Ok(note);
                    self.tools.draft = None;
                    self.note_editing(false)
                } else {
                    // Typed on while it saved: keep editing from the saved note.
                    draft.base = note.text.clone();
                    draft.conflicted = false;
                    self.tools.note = Ok(note);
                    Task::none()
                }
            }
            Err(notes::SaveError::Conflict(current)) => {
                draft.base = current.text.clone();
                draft.conflicted = true;
                self.tools.note_problem =
                    Some("Changed in Settings. Save again to replace it.".into());
                Task::none()
            }
            Err(notes::SaveError::Failed(error)) => {
                self.tools.note_problem = Some(format!("Not saved: {error}"));
                Task::none()
            }
        }
    }

    fn save_focus(&self) {
        if let Err(error) = self.tools.focus.save() {
            eprintln!("Focus timer: {error}");
        }
    }

    pub(super) fn update_tool(&mut self, action: Action) -> Task<Message> {
        let prefs = self.config.focus;
        let now = focus::now_ms();
        let today = focus::today();
        match action {
            Action::NoteEdit | Action::NoteSave | Action::NoteToggle(_)
                if self.tools.checking_note =>
            {
                return Task::none();
            }
            Action::Music(command) => return self.music_command(command),
            Action::MusicSeek(request) => {
                return cosmic::task::future(async move {
                    Message::MusicControlDone(music::perform(music::Control::Seek(request)).await)
                });
            }
            Action::MusicAuto => {
                self.tools.pinned = None;
                self.tools.music_problem = false;
                if self.config.music.player.is_some() {
                    match Config::save_tool(Widget::Music, serde_json::json!({"player": null})) {
                        Ok(config) => return self.apply_config(config),
                        Err(error) => eprintln!("Could not switch to automatic media: {error}"),
                    }
                }
                return self.music_art_task();
            }
            Action::NoteCopy => {
                if let Ok(note) = &self.tools.note {
                    let text = note.text.clone();
                    return cosmic::task::future(async move {
                        Message::NoteCopyDone(notes::copy_from_desktop(text).await)
                    });
                }
            }
            Action::MusicCycle => {
                let players = self.tools.players();
                if players.len() > 1 {
                    let current = self.current_player().map(|player| player.bus.clone());
                    let index = players
                        .iter()
                        .position(|player| Some(&player.bus) == current.as_ref())
                        .map_or(0, |index| (index + 1) % players.len());
                    self.tools.pinned = Some(players[index].bus.clone());
                    self.tools.music_problem = false;
                    return self.music_art_task();
                }
            }
            Action::NoteEdit => {
                if let Some(draft) = &mut self.tools.draft {
                    // Resume a paused draft where it was left.
                    draft.typing = true;
                } else {
                    let Ok(note) = &self.tools.note else {
                        return Task::none();
                    };
                    self.tools.draft = Some(NoteDraft {
                        content: text_editor::Content::with_text(&note.text),
                        base: note.text.clone(),
                        typing: true,
                        conflicted: false,
                        saving: false,
                    });
                    self.tools.note_problem = None;
                }
                if !self.note_lifted() {
                    self.pause_note();
                }
                return self.note_editing(self.note_lifted());
            }
            Action::NoteAction(action) => {
                if let Some(draft) = &mut self.tools.draft {
                    draft.content.perform(action);
                }
            }
            Action::NoteSave => return self.save_note(),
            Action::NoteCancel => {
                if self.tools.draft.take().is_some() {
                    self.tools.note_problem = None;
                    self.reload_note();
                    return self.note_editing(false);
                }
            }
            Action::NoteToggle(line) => {
                if self.tools.draft.is_none()
                    && let Ok(note) = &self.tools.note
                    && let Some(text) = notes::toggle(&note.text, line)
                {
                    let expected = note.text.clone();
                    self.tools.checking_note = true;
                    return cosmic::task::future(async move {
                        Message::NoteChecked(notes::save_async(expected, text).await)
                    });
                }
            }
            Action::FocusToggle => {
                self.tools.focus.toggle(now, &prefs, &today);
                self.save_focus();
            }
            Action::FocusReset => {
                self.tools.focus.reset(&prefs);
                self.save_focus();
            }
            Action::FocusSkip => {
                self.tools.focus.skip(&prefs, &today);
                self.save_focus();
            }
            Action::FocusMinutes(minutes) => {
                match Config::save_tool(
                    Widget::Focus,
                    serde_json::json!({ "focus_minutes": minutes }),
                ) {
                    Ok(config) => return self.apply_config(config),
                    Err(error) => eprintln!("Could not save the focus length: {error}"),
                }
            }
        }
        Task::none()
    }

    pub(super) fn note_checked(
        &mut self,
        result: Result<notes::Note, notes::SaveError>,
    ) -> Task<Message> {
        if !self.tools.checking_note {
            return Task::none();
        }
        self.tools.checking_note = false;
        match result {
            Ok(note) => {
                self.tools.note = Ok(note);
                self.tools.note_problem = None;
            }
            Err(notes::SaveError::Conflict(current)) => {
                self.tools.note = Ok(current);
                self.tools.note_problem = Some("Changed elsewhere. Check the item again.".into());
            }
            Err(notes::SaveError::Failed(error)) => {
                self.tools.note_problem = Some(format!("Not saved: {error}"))
            }
        }
        // A watch event may have arrived while the save was pending. Read the
        // latest bounded document so that event cannot leave an older view.
        self.reload_note();
        self.sync_widget_surfaces()
    }

    /// Ends a focus round or break whose time is up: saves it and, when
    /// asked for and not in a preview, sends a notification. Hiding the card
    /// doesn't stop a round someone started; it still ends on time.
    pub(super) fn tools_tick(&mut self) -> Task<Message> {
        let prefs = self.config.focus;
        // A round that ended while away is saved without a late notice.
        let Some(event) = self
            .finish_focus(focus::now_ms())
            .filter(|_| prefs.notify && !crate::tools::preview())
        else {
            return Task::none();
        };
        let (summary, body) = focus::announcement(event, &prefs);
        cosmic::task::future(
            async move { Message::ToolDone(crate::tools::notify(summary, body).await) },
        )
    }

    /// Advances a running round whose end has passed (starting the break
    /// when asked for) and saves it; returns what ended, if worth telling.
    fn finish_focus(&mut self, now: i64) -> Option<focus::Event> {
        if !self.tools.focus.running() {
            return None;
        }
        let before = self.tools.focus.clone();
        let event = self
            .tools
            .focus
            .tick(now, &self.config.focus, &focus::today());
        if self.tools.focus != before {
            self.save_focus();
        }
        event
    }

    /// The next moment a visible tool's display changes, or a hidden
    /// running round ends (once, with no redraws on the way).
    pub(super) fn tools_wake(&self) -> Option<Instant> {
        let now = Instant::now();
        let wall = focus::now_ms();
        let focus = if self.provider_visible(Widget::Focus) {
            self.tools.focus.next_change(wall)
        } else {
            self.tools.focus.ends_at()
        }
        .map(|at| now + Duration::from_millis((at - wall).max(1) as u64));
        let music = self
            .music_demand()
            .then_some(())
            .filter(|_| {
                self.config.music.show_progress
                    && self.config.music.visualizer.circle == crate::tools::visualizer::Circle::Off
            })
            .and_then(|_| self.current_player())
            .and_then(|player| player.progress_delay(now))
            .map(|delay| now + delay);
        focus.into_iter().chain(music).min()
    }

    /// A hidden tool keeps a tiny offscreen surface, like hidden cards.
    fn tool_surface_size(&self, widget: Widget) -> [u32; 2] {
        if self.tool_shown(widget) {
            let (_, mu) = self.shadow_px();
            self.widget_size(widget)
                .map(|value| value.ceil() as u32 + mu)
        } else {
            [16, 16]
        }
    }

    fn tool_input_zone(&self, widget: Widget) -> Vec<cosmic::iced::Rectangle> {
        if !self.tool_shown(widget) {
            return Vec::new();
        }
        let m = self.shadow_px().0.max(0) as f32;
        let size = self.widget_size(widget);
        vec![cosmic::iced::Rectangle {
            x: m,
            y: m,
            width: size[0].ceil(),
            height: size[1].ceil(),
        }]
    }

    fn tool_surface(&self, widget: Widget) -> Task<Message> {
        let size = self.tool_surface_size(widget);
        let position = if self.tool_shown(widget)
            && !(self.layout_editor.active && self.arranging_widget(widget))
        {
            self.placed_position(widget)
        } else {
            [-10000., 0.]
        };
        let editing = widget == Widget::Notes && self.note_lifted();
        get_layer_surface(SctkLayerSurfaceSettings {
            id: self.widget_id(widget),
            output: self.widget_layer_output(widget),
            layer: if editing { Layer::Top } else { Layer::Bottom },
            keyboard_interactivity: if editing {
                KeyboardInteractivity::Exclusive
            } else {
                KeyboardInteractivity::None
            },
            anchor: Anchor::TOP | Anchor::LEFT,
            margin: IcedMargin {
                top: position[1].round() as i32 - self.shadow_px().0,
                left: position[0].round() as i32 - self.shadow_px().0,
                ..Default::default()
            },
            size: Some((Some(size[0]), Some(size[1]))),
            exclusive_zone: 0,
            // Clicks land on the card itself; its transparent shadow margin
            // passes them to whatever is under it.
            input_zone: Some(self.tool_input_zone(widget)),
            namespace: format!("halcyon-{}", widget.key()),
            ..Default::default()
        })
    }

    #[cfg(test)]
    pub(super) fn tool_geometry_known(&self, widget: Widget) -> bool {
        self.tools.geometry.contains_key(&widget)
    }

    /// The next sync recreates every tool surface (above newer card surfaces).
    pub(super) fn forget_tool_surfaces(&mut self) {
        self.tools.geometry.clear();
    }

    /// Positions each tool, recreating only a surface whose size changed.
    pub(super) fn sync_tool_surfaces(&mut self) -> Vec<Task<Message>> {
        if !self.output_known || self.selected_output.is_none() {
            return Vec::new();
        }
        // Every sync places Music as it should be now.
        self.tools.music_shown = self.tool_shown(Widget::Music);
        // Placement can hide Notes even while its saved visibility is on.
        // Keep the draft paused until an explicit Resume after room returns.
        let release_note = !self.note_lifted() && self.tools.draft.is_some();
        if release_note {
            self.pause_note();
        }
        let mut tasks = Vec::new();
        for widget in Widget::TOOLS {
            let size = self.tool_surface_size(widget);
            let release = if widget == Widget::Notes && release_note {
                self.note_editing(false)
            } else {
                Task::none()
            };
            if self.tools.geometry.get(&widget) != Some(&size) {
                self.tools.geometry.insert(widget, size);
                tasks.push(
                    release
                        .chain(destroy_layer_surface(self.widget_id(widget)))
                        .chain(self.tool_surface(widget)),
                );
                continue;
            }
            let point = if self.tool_shown(widget)
                && !(self.layout_editor.active && self.arranging_widget(widget))
            {
                self.placed_position(widget)
            } else {
                [-10000., 0.]
            };
            let mi = self.shadow_px().0;
            tasks.push(release.chain(set_margin(
                self.widget_id(widget),
                point[1].round() as i32 - mi,
                0,
                0,
                point[0].round() as i32 - mi,
            )));
        }
        tasks
    }

    /// Commands from the controller (`desktop-cards focus …`), for keyboard
    /// shortcuts.
    pub(super) fn focus_command(&mut self, command: &str) -> Task<Message> {
        let running = self.tools.focus.running();
        let action = match command {
            "toggle" => Action::FocusToggle,
            "start" if !running => Action::FocusToggle,
            "pause" if running => Action::FocusToggle,
            "reset" => Action::FocusReset,
            "skip" => Action::FocusSkip,
            _ => return Task::none(),
        };
        self.update_tool(action)
    }
}

#[cfg(test)]
mod tests {
    fn finish_note_check(cards: &mut Cards, task: Task<Message>) {
        use cosmic::iced::futures::StreamExt;
        use cosmic::iced::runtime::{Action as RuntimeAction, task};
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap()
            .block_on(async {
                let mut stream = task::into_stream(task).expect("async note save");
                let mut completed = false;
                while let Some(action) = stream.next().await {
                    if let RuntimeAction::Output(cosmic::Action::App(Message::NoteChecked(
                        result,
                    ))) = action
                    {
                        let _ = cards.note_checked(result);
                        completed = true;
                    }
                }
                assert!(completed);
            });
    }

    #[test]
    fn checklist_save_is_async_and_keeps_concurrent_text() {
        if crate::displays::test_support::isolated(
            "app::tools::tests::checklist_save_is_async_and_keeps_concurrent_text",
        ) {
            return;
        }
        notes::save("", "[ ] task").unwrap();
        let (mut cards, _) = Cards::init(Core::default(), Config::default());
        let task = cards.update(Message::Tool(Action::NoteToggle(0)));
        assert!(cards.tools.checking_note);
        assert_eq!(notes::load().unwrap().text, "[ ] task");
        assert!(
            cosmic::iced::runtime::task::into_stream(cards.update_tool(Action::NoteToggle(0)))
                .is_none()
        );
        let _ = cards.update(Message::Tool(Action::NoteEdit));
        assert!(cards.tools.draft.is_none());
        notes::save("[ ] task", "[ ] edited elsewhere").unwrap();
        finish_note_check(&mut cards, task);
        assert!(!cards.tools.checking_note);
        assert_eq!(
            cards.tools.note.as_ref().unwrap().text,
            "[ ] edited elsewhere"
        );
        let task = cards.update(Message::Tool(Action::NoteToggle(0)));
        finish_note_check(&mut cards, task);
        assert_eq!(
            cards.tools.note.as_ref().unwrap().text,
            "[x] edited elsewhere"
        );
        assert_eq!(notes::load().unwrap().text, "[x] edited elsewhere");
    }

    use super::*;
    use cosmic::Application;

    fn isolated(name: &str, body: impl FnOnce()) {
        let key = format!("DESKTOP_CARDS_TOOLS_TEST_{name}");
        if std::env::var_os(&key).is_none() {
            let dir = std::env::temp_dir()
                .join(format!("desktop-cards-tools-{name}-{}", std::process::id()));
            let _ = std::fs::remove_dir_all(&dir);
            std::fs::create_dir_all(&dir).unwrap();
            std::fs::write(
                dir.join("config.json"),
                r#"{"theme":"teal","future_option":7}"#,
            )
            .unwrap();
            let status = std::process::Command::new(std::env::current_exe().unwrap())
                .args([
                    "--exact",
                    &format!("app::tools::tests::{name}"),
                    "--nocapture",
                ])
                .env(&key, "1")
                .env("DESKTOP_CARDS_CONFIG", dir.join("config.json"))
                .status()
                .unwrap();
            let _ = std::fs::remove_dir_all(&dir);
            assert!(status.success());
            return;
        }
        body();
    }

    #[test]
    fn tools_start_off_and_never_turn_the_layout_custom() {
        let config = Config::default();
        for tool in Widget::TOOLS {
            assert!(!config.widget_visible(tool));
        }
        assert!(
            !config
                .enabled_widgets
                .iter()
                .any(|key| { Widget::parse(key).is_some_and(Widget::is_tool) })
        );
        let mut config = Config::default();
        config.music.enabled = true;
        config.widget_positions.insert("music".into(), [0.2, 0.9]);
        config.widget_positions.insert("focus".into(), [0.5, 0.1]);
        assert!(!config.has_custom_positions());
        let (cards, _) = Cards::init(Core::default(), config);
        assert!(!cards.custom_layout());
        assert!(cards.config.widget_visible(Widget::Music));
        assert!(!cards.config.widget_visible(Widget::Notes));
        // An existing configuration without tool sections reads as off.
        let old: Config = serde_json::from_str(
            r#"{"enabled_widgets":["clock","weather","calendar","devbox"],"widget_positions":{"clock":[0.1,0.1]}}"#,
        )
        .unwrap();
        assert!(Widget::TOOLS.iter().all(|tool| !old.widget_visible(*tool)));
        assert!(old.has_custom_positions());
    }

    #[test]
    fn toggling_a_tool_writes_only_its_section() {
        isolated("toggling_a_tool_writes_only_its_section", || {
            let (mut cards, _) = Cards::init(Core::default(), Config::load());
            let _output = cards.test_output_ready();
            let _ = cards.update(Message::ToggleWidget(Widget::Focus));
            let raw: serde_json::Value =
                serde_json::from_slice(&std::fs::read(Config::path().unwrap()).unwrap()).unwrap();
            assert_eq!(raw["focus"]["enabled"], true);
            assert_eq!(raw["future_option"], 7);
            assert!(raw.get("enabled_widgets").is_none());
            assert!(cards.config.widget_visible(Widget::Focus));
            let _ = cards.update(Message::ToggleWidget(Widget::Focus));
            assert!(!Config::load().widget_visible(Widget::Focus));
        });
    }

    #[test]
    fn focus_rounds_persist_as_runtime_state_not_configuration() {
        isolated(
            "focus_rounds_persist_as_runtime_state_not_configuration",
            || {
                let mut config = Config::load();
                config.focus.enabled = true;
                let (mut cards, _) = Cards::init(Core::default(), config);
                let _output = cards.test_output_ready();
                let before = std::fs::read(Config::path().unwrap()).unwrap();
                let _ = cards.update(Message::Tool(Action::FocusToggle));
                assert!(cards.tools.focus.running());
                assert!(cards.tools_wake().is_some());
                // The session lives beside the configuration, which is unchanged.
                assert_eq!(std::fs::read(Config::path().unwrap()).unwrap(), before);
                let saved = focus::Session::load();
                assert!(saved.running());
                // A restarted desktop resumes the same round.
                let (restarted, _) = Cards::init(Core::default(), cards.config.clone());
                assert_eq!(restarted.tools.focus, saved);
                let _ = cards.update(Message::Tool(Action::FocusToggle));
                assert!(matches!(
                    cards.tools.focus.status,
                    focus::Status::Paused { .. }
                ));
                assert!(cards.tools_wake().is_none());
                // Quick lengths are a preference.
                let _ = cards.update(Message::Tool(Action::FocusReset));
                let _ = cards.update(Message::Tool(Action::FocusMinutes(50)));
                assert_eq!(Config::load().focus.focus_minutes, 50);
                assert_eq!(
                    cards.tools.focus.remaining(0, &cards.config.focus),
                    50 * 60_000
                );
            },
        );
    }

    #[test]
    fn note_edits_save_explicitly_and_conflicts_keep_both_texts() {
        isolated(
            "note_edits_save_explicitly_and_conflicts_keep_both_texts",
            || {
                let mut config = Config::load();
                config.notes.enabled = true;
                let (mut cards, _) = Cards::init(Core::default(), config);
                let _output = cards.test_output_ready();
                assert_eq!(cards.tools.note, Ok(notes::Note::default()));
                let _ = cards.update(Message::Tool(Action::NoteEdit));
                assert!(cards.tools.draft.is_some());
                for c in "[ ] oolong".chars() {
                    let _ = cards.update(Message::Tool(Action::NoteAction(
                        text_editor::Action::Edit(text_editor::Edit::Insert(c)),
                    )));
                }
                // Nothing is written until Save.
                assert_eq!(notes::load().unwrap().text, "");
                let _ = cards.update(Message::Tool(Action::NoteSave));
                assert!(cards.tools.draft.is_none());
                assert_eq!(notes::load().unwrap().text, "[ ] oolong");
                // Ticking saves at once and changes only the box.
                let task = cards.update(Message::Tool(Action::NoteToggle(0)));
                finish_note_check(&mut cards, task);
                assert_eq!(notes::load().unwrap().text, "[x] oolong");
                // Settings saves while the desktop edits: the first Save explains,
                // keeps the draft, and only a second Save replaces.
                let _ = cards.update(Message::Tool(Action::NoteEdit));
                let _ = cards.update(Message::Tool(Action::NoteAction(
                    text_editor::Action::Edit(text_editor::Edit::Insert('!')),
                )));
                notes::save("[x] oolong", "from settings").unwrap();
                let _ = cards.update(Message::Tool(Action::NoteSave));
                assert!(cards.tools.draft.is_some());
                assert!(cards.tools.note_problem.is_some());
                assert_eq!(notes::load().unwrap().text, "from settings");
                let _ = cards.update(Message::Tool(Action::NoteSave));
                assert!(cards.tools.draft.is_none());
                assert_eq!(notes::load().unwrap().text, "![x] oolong");
                // Cancel drops the draft.
                let _ = cards.update(Message::Tool(Action::NoteEdit));
                let _ = cards.update(Message::Tool(Action::NoteAction(
                    text_editor::Action::Edit(text_editor::Edit::Insert('?')),
                )));
                let _ = cards.update(Message::Tool(Action::NoteCancel));
                assert_eq!(notes::load().unwrap().text, "![x] oolong");
                // Arranging the desktop saves an open note and keeps no keyboard.
                let _ = cards.update(Message::Tool(Action::NoteEdit));
                assert!(cards.note_lifted());
                let _ = cards.update(Message::Tool(Action::NoteAction(
                    text_editor::Action::Edit(text_editor::Edit::Insert('#')),
                )));
                let _ = cards.update(Message::Arrange);
                assert!(cards.layout_editor.active && cards.tools.draft.is_none());
                assert!(!cards.note_lifted());
                assert_eq!(notes::load().unwrap().text, "#![x] oolong");
                let _ = cards.update(Message::LayoutFinish(false));
                // Hiding Notes while editing releases the keyboard; the draft waits.
                let _ = cards.update(Message::Tool(Action::NoteEdit));
                let mut hidden = cards.config.clone();
                hidden.notes.enabled = false;
                let _ = cards.apply_config(hidden);
                assert!(cards.tools.draft.is_some() && !cards.note_lifted());
                // The note is never part of the configuration.
                let raw = std::fs::read_to_string(Config::path().unwrap()).unwrap();
                assert!(!raw.contains("oolong"));
            },
        );
    }

    #[test]
    fn a_hidden_focus_round_still_ends_on_time_without_redraws() {
        isolated(
            "a_hidden_focus_round_still_ends_on_time_without_redraws",
            || {
                let mut config = Config::load();
                config.focus.enabled = true;
                config.focus.auto_start_breaks = true;
                let (mut cards, _) = Cards::init(Core::default(), config);
                let _output = cards.test_output_ready();
                let _ = cards.update(Message::Tool(Action::FocusToggle));
                let focus::Status::Running { ends_at } = cards.tools.focus.status else {
                    panic!("the round should run");
                };
                // Visible: the countdown redraws every second.
                let wake = cards.tools_wake().unwrap();
                assert!(wake <= Instant::now() + Duration::from_millis(1001));
                // Presentation (or the switch) hides Focus: the round keeps
                // running, and the only wake is its end.
                let mut hidden = cards.config.clone();
                hidden.focus.enabled = false;
                let _ = cards.apply_config(hidden);
                assert!(cards.tools.focus.running());
                let wake = cards.tools_wake().unwrap();
                let left = wake.saturating_duration_since(Instant::now());
                let expected = Duration::from_millis((ends_at - focus::now_ms()).max(0) as u64);
                assert!(left > Duration::from_secs(60));
                assert!(left.abs_diff(expected) < Duration::from_secs(1));
                // Its end, while hidden: done, counted, announced, the break
                // started from that moment, and all of it saved.
                let event = cards.finish_focus(ends_at);
                assert_eq!(
                    event,
                    Some(focus::Event::Finished {
                        phase: focus::Phase::Focus,
                        next: focus::Phase::Break,
                    })
                );
                assert_eq!(cards.tools.focus.phase, focus::Phase::Break);
                assert_eq!(cards.tools.focus.rounds_today(&focus::today()), 1);
                assert_eq!(focus::Session::load(), cards.tools.focus);
                let break_ends = cards.tools.focus.ends_at().unwrap();
                assert_eq!(
                    break_ends,
                    ends_at + i64::from(cards.config.focus.break_minutes) * 60_000
                );
                // The break ends hidden too; without auto-start the next round waits.
                let event = cards.finish_focus(break_ends);
                assert!(matches!(
                    event,
                    Some(focus::Event::Finished {
                        phase: focus::Phase::Break,
                        ..
                    })
                ));
                assert_eq!(cards.tools.focus.status, focus::Status::Done);
                assert!(cards.tools_wake().is_none());
                // A shortcut starts a round while the card is off; it ends on its own.
                let _ = cards.update(Message::Tool(Action::FocusReset));
                let _ = cards.focus_command("start");
                assert!(cards.tools.focus.running());
                assert!(cards.tools_wake().is_some());
                let ends_at = cards.tools.focus.ends_at().unwrap();
                assert!(cards.finish_focus(ends_at + 1).is_some());
                assert_eq!(cards.tools.focus.rounds_today(&focus::today()), 2);
                // A tick before the end changes nothing and writes nothing.
                let _ = cards.update(Message::Tool(Action::FocusReset));
                let _ = cards.focus_command("start");
                let before = std::fs::read(focus::Session::path().unwrap()).unwrap();
                assert_eq!(cards.finish_focus(focus::now_ms()), None);
                assert_eq!(
                    std::fs::read(focus::Session::path().unwrap()).unwrap(),
                    before
                );
            },
        );
    }

    #[test]
    fn a_note_hidden_while_editing_returns_paused_and_resumes_only_on_request() {
        isolated(
            "a_note_hidden_while_editing_returns_paused_and_resumes_only_on_request",
            || {
                let mut config = Config::load();
                config.notes.enabled = true;
                let (mut cards, _) = Cards::init(Core::default(), config);
                let _output = cards.test_output_ready();
                let type_char = |cards: &mut Cards, c| {
                    let _ = cards.update(Message::Tool(Action::NoteAction(
                        text_editor::Action::Edit(text_editor::Edit::Insert(c)),
                    )));
                };
                let _ = cards.update(Message::Tool(Action::NoteEdit));
                assert!(cards.note_lifted());
                type_char(&mut cards, 'a');
                // Hidden (Presentation, the switch, `toggle notes`): no keyboard.
                let shown = cards.config.clone();
                let mut hidden = shown.clone();
                hidden.notes.enabled = false;
                let _ = cards.apply_config(hidden);
                assert!(!cards.note_lifted());
                // Shown again: the draft is there, paused, still without the keyboard.
                let _ = cards.apply_config(shown.clone());
                let draft = cards.tools.draft.as_ref().unwrap();
                assert!(!draft.typing && draft.content.text().starts_with('a'));
                assert!(!cards.note_lifted());
                assert_eq!(notes::load().unwrap().text, "");
                // A surface recreated for any reason stays below windows.
                let _ = cards.sync_tool_surfaces();
                assert!(!cards.note_lifted());
                // Resume is the explicit way back, keeping what was written.
                let _ = cards.update(Message::Tool(Action::NoteEdit));
                assert!(cards.note_lifted());
                type_char(&mut cards, 'b');
                assert!(
                    cards
                        .tools
                        .draft
                        .as_ref()
                        .unwrap()
                        .content
                        .text()
                        .starts_with("ab")
                );
                // Arranging with a draft that can't be saved yet (Settings saved
                // meanwhile) pauses it too; finishing doesn't take the keyboard.
                notes::save("", "from settings").unwrap();
                let _ = cards.update(Message::Arrange);
                assert!(cards.tools.draft.is_some() && !cards.note_lifted());
                let _ = cards.update(Message::LayoutFinish(false));
                assert!(!cards.layout_editor.active && !cards.note_lifted());
                assert_eq!(notes::load().unwrap().text, "from settings");
                assert!(cards.tools.draft.as_ref().unwrap().conflicted);
                // Arranging again never replaces the newer note implicitly.
                let _ = cards.update(Message::Arrange);
                let _ = cards.update(Message::LayoutFinish(false));
                assert_eq!(notes::load().unwrap().text, "from settings");
                // Resume, then Save replaces deliberately, as before.
                let _ = cards.update(Message::Tool(Action::NoteEdit));
                assert!(cards.note_lifted());
                let _ = cards.update(Message::Tool(Action::NoteSave));
                assert!(cards.tools.draft.is_none() && !cards.note_lifted());
                assert!(notes::load().unwrap().text.starts_with("ab"));
                // Cancel on a paused draft drops it without the keyboard.
                let _ = cards.update(Message::Tool(Action::NoteEdit));
                type_char(&mut cards, 'c');
                let mut hidden = cards.config.clone();
                hidden.notes.enabled = false;
                let _ = cards.apply_config(hidden);
                let _ = cards.apply_config(shown);
                let _ = cards.update(Message::Tool(Action::NoteCancel));
                assert!(cards.tools.draft.is_none() && !cards.note_lifted());
                assert!(notes::load().unwrap().text.starts_with("ab"));
            },
        );
    }

    #[test]
    fn music_is_on_the_desktop_while_it_has_something_to_show() {
        let mut config = Config::default();
        config.music.enabled = true;
        let (mut cards, _) = Cards::init(Core::default(), config);
        let _output = cards.test_output_ready();
        let player = |status| music::Player {
            bus: format!("{}spotify", music::PREFIX),
            key: "spotify".into(),
            owner: None,
            seeked: false,
            identity: "Spotify".into(),
            status,
            title: "Track".into(),
            artists: vec![],
            album: String::new(),
            art_url: None,
            media_url: None,
            track_id: None,
            can_seek: false,
            length_us: None,
            position_us: None,
            rate: 1.,
            sampled: Instant::now(),
            can_play: true,
            can_pause: true,
            can_next: true,
            can_previous: true,
            can_control: true,
            can_raise: false,
        };
        // No player: no card, and no surface on the desktop.
        assert!(!cards.tool_shown(Widget::Music));
        let feed = |status| Message::MusicFeed(music::Feed::Players(vec![player(status)]));
        // Stopped with nothing loaded: still nothing to show.
        let _ = cards.update(feed(music::Playback::Stopped));
        assert!(!cards.tool_shown(Widget::Music));
        assert_eq!(cards.tool_surface_size(Widget::Music), [16, 16]);
        // Playing: the card is there.
        let _ = cards.update(feed(music::Playback::Playing));
        assert!(cards.tool_shown(Widget::Music) && cards.tools.music_shown);
        // Paused with a track loaded: it stays, to see it and resume.
        let _ = cards.update(feed(music::Playback::Paused));
        assert!(cards.tool_shown(Widget::Music));
        // Paused with nothing loaded: gone.
        let mut empty = player(music::Playback::Paused);
        empty.title.clear();
        let _ = cards.update(Message::MusicFeed(music::Feed::Players(vec![empty])));
        assert!(!cards.tool_shown(Widget::Music) && !cards.tools.music_shown);
        // The player quits: gone.
        let _ = cards.update(Message::MusicFeed(music::Feed::Players(vec![])));
        assert!(!cards.tool_shown(Widget::Music));
        // Arranging always shows it, so it can be placed.
        cards.layout_editor.active = true;
        assert!(cards.tool_shown(Widget::Music));
    }

    #[test]
    fn music_follows_the_playing_player_and_cycles_on_request() {
        let mut config = Config::default();
        config.music.enabled = true;
        let (mut cards, _) = Cards::init(Core::default(), config);
        let _output = cards.test_output_ready();
        assert!(cards.current_player().is_none());
        let player = |key: &str, status| music::Player {
            bus: format!("{}{key}", music::PREFIX),
            key: key.into(),
            owner: None,
            seeked: false,
            identity: key.into(),
            status,
            title: "Track".into(),
            artists: vec![],
            album: String::new(),
            art_url: None,
            media_url: None,
            track_id: None,
            can_seek: false,
            length_us: Some(200_000_000),
            position_us: Some(0),
            rate: 1.,
            sampled: Instant::now(),
            can_play: true,
            can_pause: true,
            can_next: true,
            can_previous: true,
            can_control: true,
            can_raise: false,
        };
        let _ = cards.update(Message::MusicFeed(music::Feed::Players(vec![
            player("a", music::Playback::Paused),
            player("b", music::Playback::Playing),
        ])));
        assert_eq!(cards.current_player().unwrap().key, "b");
        assert!(cards.tools_wake().is_some());
        let _ = cards.update(Message::Tool(Action::MusicCycle));
        assert_eq!(cards.current_player().unwrap().key, "a");
        let _ = cards.update(Message::Tool(Action::MusicAuto));
        assert!(cards.tools.pinned.is_none());
        assert_eq!(cards.current_player().unwrap().key, "b");
        let _ = cards.update(Message::Tool(Action::MusicCycle));
        // A picked player that closes falls back to what plays.
        let _ = cards.update(Message::MusicFeed(music::Feed::Players(vec![player(
            "b",
            music::Playback::Playing,
        )])));
        assert_eq!(cards.current_player().unwrap().key, "b");
        assert!(cards.tools.pinned.is_none());
        let _ = cards.update(Message::MusicFeed(music::Feed::Unavailable(
            "no bus".into(),
        )));
        assert!(cards.current_player().is_none());
    }

    fn click_music_control(cards: &Cards, id: &str) -> Vec<Message> {
        use cosmic::iced::{Event, Size, mouse};
        let mut native = crate::native_test::Native::new(Size::new(360., 140.));
        let bounds = native.bounds(cards.tool_card(Widget::Music), id);
        let (messages, _) = native.event(
            cards.tool_card(Widget::Music),
            Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)),
            bounds.center(),
        );
        messages
    }

    fn click_music_play(cards: &Cards) -> Message {
        click_music_control(cards, "music-play-pause")
            .into_iter()
            .find(|message| matches!(message, Message::Tool(Action::Music(_))))
            .expect("The visible play control must produce a request")
    }

    async fn finish_music_control(cards: &mut Cards, message: Message) {
        use cosmic::iced::futures::StreamExt;
        use cosmic::iced::runtime::{Action as RuntimeAction, task};
        let mut stream = task::into_stream(cards.update(message)).unwrap();
        let mut completed = false;
        while let Some(action) = stream.next().await {
            if let RuntimeAction::Output(cosmic::Action::App(Message::MusicControlDone(result))) =
                action
            {
                let _ = cards.music_feed(music::Feed::Players(result.unwrap()));
                completed = true;
            }
        }
        assert!(completed, "The transport request must finish");
    }

    #[test]
    fn music_control_refresh_keeps_progress_on_desktop() {
        music::fixture::isolated(
            "app::tools::tests::music_control_refresh_keeps_progress_on_desktop",
            || {
                use std::sync::atomic::Ordering;
                tokio::runtime::Builder::new_current_thread()
                    .enable_all()
                    .build()
                    .unwrap()
                    .block_on(async {
                        let address = std::env::var("DBUS_SESSION_BUS_ADDRESS").unwrap();
                        let state = music::fixture::State::new();
                        state.playing.store(true, Ordering::SeqCst);
                        let _service = music::fixture::serve(
                            &address,
                            "org.mpris.MediaPlayer2.firefox",
                            state.clone(),
                        )
                        .await;
                        let client = music::connection().await.unwrap();
                        let mut config = Config::default();
                        config.music.enabled = true;
                        let (mut cards, _) = Cards::init(Core::default(), config.clone());
                        let _output = cards.test_output_ready();
                        let players = music::snapshot(&client).await.unwrap();
                        let _ = cards.music_feed(music::Feed::Players(players.clone()));
                        state.broken_timing.store(true, Ordering::SeqCst);
                        let players = music::snapshot(&client).await.unwrap();
                        let _ = cards.update(Message::MusicControlDone(Ok(players.clone())));
                        {
                            let player = cards.current_player().unwrap();
                            assert_eq!(player.length_us, Some(300_000_000));
                            assert!(player.position_us.unwrap() >= 60_000_000);
                            assert_eq!(player.seek_request(0.5).unwrap().position_us, 150_000_000);
                        }
                        let messages = click_music_control(&cards, "music-progress");
                        assert!(
                            messages
                                .iter()
                                .any(|m| matches!(m, Message::Tool(Action::MusicSeek(_)))),
                            "Progress must stay interactive after a broken control refresh"
                        );
                    });
            },
        );
    }

    #[test]
    fn music_click_controls_the_visible_player_after_auto_follow_changes() {
        music::fixture::isolated(
            "app::tools::tests::music_click_controls_the_visible_player_after_auto_follow_changes",
            || {
                use std::sync::atomic::Ordering;
                tokio::runtime::Builder::new_current_thread()
                    .enable_all()
                    .build()
                    .unwrap()
                    .block_on(async {
                        let address = std::env::var("DBUS_SESSION_BUS_ADDRESS").unwrap();
                        let first = music::fixture::State::new();
                        let other = music::fixture::State::new();
                        let _a = music::fixture::serve(
                            &address,
                            "org.mpris.MediaPlayer2.a",
                            first.clone(),
                        )
                        .await;
                        let _b = music::fixture::serve(
                            &address,
                            "org.mpris.MediaPlayer2.b",
                            other.clone(),
                        )
                        .await;
                        let client = music::connection().await.unwrap();
                        let mut config = Config::default();
                        config.music.enabled = true;
                        let (mut cards, _) = Cards::init(Core::default(), config);
                        let _output = cards.test_output_ready();
                        let _ = cards.music_feed(music::Feed::Players(
                            music::snapshot(&client).await.unwrap(),
                        ));
                        assert_eq!(cards.current_player().unwrap().key, "a");
                        let clicked = click_music_play(&cards);
                        other.playing.store(true, Ordering::SeqCst);
                        let _ = cards.music_feed(music::Feed::Players(
                            music::snapshot(&client).await.unwrap(),
                        ));
                        assert_eq!(cards.current_player().unwrap().key, "b");
                        finish_music_control(&mut cards, clicked).await;
                        assert!(
                            first.playing.load(Ordering::SeqCst),
                            "Play must target the player that was drawn"
                        );
                        assert!(
                            other.playing.load(Ordering::SeqCst),
                            "Auto-follow must not retarget an earlier click"
                        );
                    });
            },
        );
    }

    #[test]
    fn music_play_click_does_not_pause_a_player_that_already_started() {
        music::fixture::isolated(
            "app::tools::tests::music_play_click_does_not_pause_a_player_that_already_started",
            || {
                use std::sync::atomic::Ordering;
                tokio::runtime::Builder::new_current_thread()
                    .enable_all()
                    .build()
                    .unwrap()
                    .block_on(async {
                        let address = std::env::var("DBUS_SESSION_BUS_ADDRESS").unwrap();
                        let state = music::fixture::State::new();
                        let _service = music::fixture::serve(
                            &address,
                            "org.mpris.MediaPlayer2.a",
                            state.clone(),
                        )
                        .await;
                        let client = music::connection().await.unwrap();
                        let mut config = Config::default();
                        config.music.enabled = true;
                        let (mut cards, _) = Cards::init(Core::default(), config);
                        let _output = cards.test_output_ready();
                        let _ = cards.music_feed(music::Feed::Players(
                            music::snapshot(&client).await.unwrap(),
                        ));
                        let clicked = click_music_play(&cards);
                        state.playing.store(true, Ordering::SeqCst);
                        finish_music_control(&mut cards, clicked).await;
                        assert!(
                            state.playing.load(Ordering::SeqCst),
                            "A displayed Play action must not toggle playback off"
                        );
                    });
            },
        );
    }

    #[test]
    fn music_transport_controls_send_supported_actions_and_disable_unsupported_skips() {
        music::fixture::isolated(
            "app::tools::tests::music_transport_controls_send_supported_actions_and_disable_unsupported_skips",
            || {
                use std::sync::atomic::Ordering;
                tokio::runtime::Builder::new_current_thread()
                    .enable_all()
                    .build()
                    .unwrap()
                    .block_on(async {
                        let address = std::env::var("DBUS_SESSION_BUS_ADDRESS").unwrap();
                        let state = music::fixture::State::new();
                        let _service = music::fixture::serve(
                            &address,
                            "org.mpris.MediaPlayer2.a",
                            state.clone(),
                        )
                        .await;
                        let client = music::connection().await.unwrap();
                        let mut config = Config::default();
                        config.music.enabled = true;
                        let (mut cards, _) = Cards::init(Core::default(), config);
                        let _output = cards.test_output_ready();
                        let _ = cards.music_feed(music::Feed::Players(
                            music::snapshot(&client).await.unwrap(),
                        ));
                        for (id, playing, track) in [
                            ("music-play-pause", true, 1),
                            ("music-play-pause", false, 1),
                            ("music-next", false, 2),
                            ("music-previous", false, 1),
                        ] {
                            let mut messages = click_music_control(&cards, id);
                            assert_eq!(messages.len(), 1);
                            finish_music_control(&mut cards, messages.remove(0)).await;
                            assert_eq!(state.playing.load(Ordering::SeqCst), playing);
                            assert_eq!(state.track.load(Ordering::SeqCst), track);
                        }
                        let mut native =
                            crate::native_test::Native::new(cosmic::iced::Size::new(360., 140.));
                        native.capture(
                            cards.tool_card(Widget::Music),
                            &cosmic::Theme::dark(),
                            "music-skips-enabled",
                        );
                        state.can_skip.store(false, Ordering::SeqCst);
                        let _ = cards.music_feed(music::Feed::Players(
                            music::snapshot(&client).await.unwrap(),
                        ));
                        native.capture(
                            cards.tool_card(Widget::Music),
                            &cosmic::Theme::dark(),
                            "music-skips-disabled",
                        );
                        for id in ["music-previous", "music-next"] {
                            assert!(
                                click_music_control(&cards, id).is_empty(),
                                "Unsupported skip cannot emit a command"
                            );
                        }
                    });
            },
        );
    }
}

#[cfg(test)]
mod auditfix_tests;
