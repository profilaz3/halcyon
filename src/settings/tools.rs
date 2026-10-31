//! Settings for the desktop tools: Music's player choice and card options,
//! the note itself and the Notes card size, Focus durations and behavior.
//!
//! Each page shows the real card as its preview. Music's preview reads the
//! players this window sees and its controls work; Focus's preview is the
//! desktop's session, and its controls are sent to the desktop. The note is
//! edited here with an explicit Save, compare-and-swap against the desktop;
//! unsaved edits outlive the window as a kept draft (see `keep_draft`).
use crate::config::Config;
use crate::drawer::Widget;
use crate::tools::cards::Action;
use crate::tools::{focus, music, notes};
use cosmic::app::Task;
use cosmic::widget::image::Handle;
use cosmic::widget::text_editor;

#[derive(Clone, Debug)]
pub enum Message {
    Visualizer(crate::tools::visualizer::Preferences),
    VisualizerSave,
    /// A control on a preview card.
    Card(Action),
    Players(Result<Vec<music::Player>, String>),
    MediaControlDone(Result<Vec<music::Player>, String>),
    Art(String, Result<music::Art, String>),
    /// Look for players again (while the Music page is open).
    Refresh,
    /// Redraw a running countdown or progress bar.
    Redraw,
    /// The preferred player's bus key, or `None` to follow what plays.
    Player(Option<String>),
    /// One member of a tool's configuration section.
    Option(Widget, &'static str, serde_json::Value),
    Note(text_editor::Action),
    NoteSave,
    NoteSaved(String, Result<notes::Note, notes::SaveError>),
    NoteRevert,
    /// After a conflict: replace the newer note with this text.
    NoteKeepMine,
    /// After a conflict: load the newer note, dropping this text.
    NoteUseSaved,
    Done(Result<(), String>),
}

pub struct ToolsEditor {
    /// What this window sees on the session bus; `None` until asked.
    pub feed: Option<music::Feed>,
    pub visualizer_draft: Option<crate::tools::visualizer::Preferences>,
    recent: Vec<String>,
    pub pinned: Option<String>,
    pub music_problem: bool,
    art: Option<(String, Option<Handle>)>,
    art_loading: bool,
    querying: bool,
    pub note: Result<notes::Note, String>,
    pub content: text_editor::Content,
    /// The saved text this editor started from.
    pub base: String,
    pub problem: Option<String>,
    /// A newer note saved elsewhere while this one had unsaved edits.
    pub conflict: Option<notes::Note>,
    /// The editor opened with unsaved edits kept from last time.
    pub restored: bool,
    saving: Option<String>,
    pub close_after_save: bool,
    /// A draft file holds this editor's unsaved edits.
    kept: bool,
    pub focus: focus::Session,
    /// Notes is the last tools tab shown; the navigation returns to it.
    pub tab: Widget,
}

impl Default for ToolsEditor {
    fn default() -> Self {
        let note = notes::load();
        let text = note
            .as_ref()
            .map(|note| note.text.clone())
            .unwrap_or_default();
        let draft = notes::load_draft();
        let mut editor = Self {
            feed: None,
            visualizer_draft: None,
            recent: Vec::new(),
            pinned: None,
            music_problem: false,
            art: None,
            art_loading: false,
            querying: false,
            content: text_editor::Content::with_text(&text),
            base: text,
            note,
            problem: None,
            conflict: None,
            restored: false,
            saving: None,
            close_after_save: false,
            kept: draft.is_some(),
            focus: focus::Session::load(),
            tab: Widget::Music,
        };
        if let Some(draft) = draft {
            // Unsaved edits from a closed window come back as unsaved edits,
            // checked against whatever was saved since.
            editor.content = text_editor::Content::with_text(&draft.text);
            editor.base = draft.base;
            editor.restored = true;
            editor.reconcile();
            editor.settle();
        }
        editor
    }
}

/// The editor's text without the newline it reports after the last line.
fn editor_text(content: &text_editor::Content) -> String {
    let text = content.text();
    text.strip_suffix('\n').unwrap_or(&text).to_string()
}

impl ToolsEditor {
    /// Changes when the pages show something different (capture mode);
    /// a running countdown alone does not count.
    pub fn state_key(&self) -> String {
        let players = self
            .player_list()
            .iter()
            .map(|p| format!("{}{:?}{}", p.bus, p.status, p.title))
            .collect::<Vec<_>>()
            .join(",");
        format!(
            "{}:{}:{}:{}:{}:{}:{:?}:{:?}",
            self.feed.is_some(),
            players,
            self.art
                .as_ref()
                .is_some_and(|(_, handle)| handle.is_some()),
            self.dirty(),
            self.saving(),
            self.conflict.is_some(),
            self.focus.phase,
            std::mem::discriminant(&self.focus.status),
        )
    }
    pub fn dirty(&self) -> bool {
        editor_text(&self.content) != self.base
    }
    pub fn saving(&self) -> bool {
        self.saving.is_some()
    }
    pub fn note_chars(&self) -> usize {
        editor_text(&self.content).chars().count()
    }
    pub fn player_list(&self) -> &[music::Player] {
        match &self.feed {
            Some(music::Feed::Players(players)) => players,
            _ => &[],
        }
    }
    pub fn current(&self, config: &Config) -> Option<&music::Player> {
        music::choose(
            self.player_list(),
            self.pinned.as_deref(),
            config.music.player.as_deref(),
            &self.recent,
        )
    }
    pub fn art(&self, config: &Config) -> Option<&Handle> {
        let url = self.current(config)?.art_url.as_deref()?;
        self.art
            .as_ref()
            .filter(|(cached, _)| cached == url)
            .and_then(|(_, handle)| handle.as_ref())
    }
    pub fn playing(&self, config: &Config) -> bool {
        self.current(config)
            .is_some_and(|player| player.progress_delay(std::time::Instant::now()).is_some())
    }

    /// The pages read again whatever changed on disk: the desktop's focus
    /// session, and the note unless it has unsaved edits here.
    pub fn refresh(&mut self) {
        self.focus = focus::Session::load();
        if self.saving() {
            return;
        }
        let note = notes::load();
        if note == self.note {
            return;
        }
        self.note = note;
        self.reconcile();
        self.settle();
    }

    /// Squares the editor with the saved note: a clean editor follows it,
    /// the same text on both sides is simply saved, and a different newer
    /// note is kept for the user to choose. Nothing typed here is replaced.
    fn reconcile(&mut self) {
        let Ok(saved) = &self.note else {
            return;
        };
        if saved.text == editor_text(&self.content) {
            self.base = saved.text.clone();
            self.conflict = None;
            self.problem = None;
        } else if !self.dirty() {
            self.content = text_editor::Content::with_text(&saved.text);
            self.base = saved.text.clone();
            self.problem = None;
            self.conflict = None;
        } else if saved.text != self.base {
            self.conflict = Some(saved.clone());
        } else {
            self.conflict = None;
        }
    }

    /// Keeps unsaved edits in the draft file when the window may go away
    /// (closing, leaving the page); called by Settings' `flush_edits`.
    pub fn keep_draft(&mut self) -> Result<(), String> {
        if !self.dirty() {
            return self.settle_draft();
        }
        notes::keep_draft(Some(&notes::Draft {
            text: editor_text(&self.content),
            base: self.base.clone(),
        }))?;
        self.kept = true;
        Ok(())
    }

    /// Forgets a kept draft once nothing is unsaved (saved, reverted or
    /// replaced by the user's choice).
    fn settle_draft(&mut self) -> Result<(), String> {
        if self.dirty() {
            return Ok(());
        }
        self.restored = false;
        if !self.kept {
            return Ok(());
        }
        notes::keep_draft(None)?;
        self.kept = false;
        Ok(())
    }
    fn settle(&mut self) {
        if let Err(error) = self.settle_draft() {
            eprintln!("Notes draft: {error}");
        }
    }

    /// Opening a tools page: Music looks for players.
    pub fn open(&mut self, tab: Widget) -> Task<crate::settings::Message> {
        self.tab = tab;
        self.refresh();
        if tab == Widget::Music {
            return self.query();
        }
        Task::none()
    }

    fn query(&mut self) -> Task<crate::settings::Message> {
        if self.querying {
            return Task::none();
        }
        self.querying = true;
        cosmic::task::future(async {
            let result = match music::connection().await {
                Ok(connection) => music::snapshot(&connection).await,
                Err(error) => Err(error),
            };
            crate::settings::Message::Tools(Message::Players(result))
        })
    }

    fn art_task(&mut self, config: &Config) -> Task<crate::settings::Message> {
        if !config.music_artwork() || self.art_loading {
            return Task::none();
        }
        let Some(url) = self
            .current(config)
            .and_then(|player| player.art_url.clone())
        else {
            return Task::none();
        };
        if self.art.as_ref().is_some_and(|(cached, _)| *cached == url) {
            return Task::none();
        }
        self.art_loading = true;
        cosmic::task::future(async move {
            let result = music::load_art(url.clone()).await;
            crate::settings::Message::Tools(Message::Art(url, result))
        })
    }

    fn save_note(&mut self, expected: String) -> Task<crate::settings::Message> {
        if self.saving() {
            return Task::none();
        }
        let text = editor_text(&self.content);
        self.saving = Some(text.clone());
        self.problem = None;
        cosmic::task::future(async move {
            let result = notes::save_async(expected, text.clone()).await;
            crate::settings::Message::Tools(Message::NoteSaved(text, result))
        })
    }
    fn note_saved(
        &mut self,
        text: String,
        result: Result<notes::Note, notes::SaveError>,
    ) -> Option<Saved> {
        if self.saving.as_ref() != Some(&text) {
            return None;
        }
        self.saving = None;
        let outcome = match result {
            Ok(note) => {
                // Edits made during this save stay in the editor. Only its
                // baseline advances to the text that actually reached disk.
                self.base = note.text.clone();
                self.note = Ok(note);
                self.problem = None;
                self.conflict = None;
                self.settle();
                self.refresh();
                Ok(())
            }
            Err(notes::SaveError::Conflict(current)) => {
                self.conflict = Some(current);
                self.problem = None;
                Ok(())
            }
            Err(notes::SaveError::Failed(error)) => {
                self.problem = Some(error.clone());
                Err(error)
            }
        };
        Some(Saved::Note(outcome))
    }

    /// Applies `message`. Returns follow-up work and, for preference
    /// changes, the configuration to save.
    pub fn update(
        &mut self,
        message: Message,
        config: &Config,
    ) -> (Task<crate::settings::Message>, Option<Saved>) {
        match message {
            Message::NoteSaved(text, result) => {
                return (Task::none(), self.note_saved(text, result));
            }
            Message::NoteSave
            | Message::NoteKeepMine
            | Message::NoteRevert
            | Message::NoteUseSaved
                if self.saving() =>
            {
                return (Task::none(), None);
            }
            Message::MediaControlDone(result) => {
                self.querying = false;
                self.music_problem = result.is_err();
                return match result {
                    Ok(players) => self.update(Message::Players(Ok(players)), config),
                    Err(error) => {
                        eprintln!("Music: {error}");
                        (Task::none(), None)
                    }
                };
            }
            Message::Players(mut result) => {
                self.querying = false;
                if let Ok(players) = &mut result {
                    let before = self.player_list().to_vec();
                    music::reconcile_timing(&before, players);
                    music::remember(&mut self.recent, &before, players);
                    if self
                        .pinned
                        .as_ref()
                        .is_some_and(|bus| !players.iter().any(|player| player.bus == *bus))
                    {
                        self.pinned = None;
                    }
                }
                self.feed = Some(match result {
                    Ok(players) => music::Feed::Players(players),
                    Err(error) => music::Feed::Unavailable(error),
                });
                return (self.art_task(config), None);
            }
            Message::Art(url, result) => {
                self.art_loading = false;
                let handle = result
                    .ok()
                    .map(|art| Handle::from_rgba(art.width, art.height, art.rgba));
                self.art = Some((url, handle));
                return (self.art_task(config), None);
            }
            Message::Refresh => return (self.query(), None),
            Message::Redraw => {}
            Message::Player(key) => {
                self.pinned = None;
                self.music_problem = false;
                return (
                    Task::none(),
                    Some(Saved::Tool(
                        Widget::Music,
                        "player",
                        key.map(|key| music::app_key(&key).to_string()).into(),
                    )),
                );
            }
            Message::Visualizer(p) => {
                self.visualizer_draft = Some(p);
            }
            Message::VisualizerSave => {
                if let Some(p) = self.visualizer_draft.take() {
                    return (
                        Task::none(),
                        Some(Saved::Tool(
                            Widget::Music,
                            "visualizer",
                            serde_json::to_value(p).unwrap(),
                        )),
                    );
                }
            }
            Message::Option(widget, key, value) => {
                return (Task::none(), Some(Saved::Tool(widget, key, value)));
            }
            Message::Note(action) => {
                if !matches!(self.note, Err(_)) {
                    self.content.perform(action);
                }
            }
            Message::NoteSave => {
                if self.dirty() && self.note.is_ok() {
                    let expected = self.base.clone();
                    return (self.save_note(expected), None);
                }
            }
            Message::NoteRevert => {
                // Back to what is saved now: after a conflict that is the
                // newer note, not the older one these edits started from.
                if let Ok(saved) = &self.note {
                    self.base = saved.text.clone();
                }
                self.content = text_editor::Content::with_text(&self.base);
                self.problem = None;
                self.conflict = None;
            }
            Message::NoteKeepMine => {
                if let Some(current) = self.conflict.take() {
                    return (self.save_note(current.text), None);
                }
            }
            Message::NoteUseSaved => {
                if let Some(current) = self.conflict.take() {
                    self.content = text_editor::Content::with_text(&current.text);
                    self.base = current.text.clone();
                    self.note = Ok(current);
                    self.problem = None;
                }
            }
            Message::Card(Action::FocusMinutes(minutes)) => {
                return (
                    Task::none(),
                    Some(Saved::Tool(Widget::Focus, "focus_minutes", minutes.into())),
                );
            }
            Message::Card(Action::MusicAuto) => return self.update(Message::Player(None), config),
            Message::Card(action) => return (self.card(action, config), None),
            Message::Done(result) => {
                if let Err(error) = result {
                    self.problem = Some(error);
                }
            }
        }
        self.settle();
        (Task::none(), None)
    }

    /// A preview card control. Music talks to the player; Focus asks the
    /// desktop, which owns the session; a checklist tick saves the note.
    fn card(&mut self, action: Action, config: &Config) -> Task<crate::settings::Message> {
        match action {
            Action::Music(request) => {
                self.querying = true;
                cosmic::task::future(async move {
                    crate::settings::Message::Tools(Message::MediaControlDone(
                        music::perform(music::Control::Transport(request)).await,
                    ))
                })
            }
            Action::MusicSeek(request) => {
                self.querying = true;
                cosmic::task::future(async move {
                    crate::settings::Message::Tools(Message::MediaControlDone(
                        music::perform(music::Control::Seek(request)).await,
                    ))
                })
            }
            Action::MusicAuto => Task::none(), // handled as a saved preference above
            Action::NoteCopy => self
                .note
                .as_ref()
                .map(|note| cosmic::iced::clipboard::write(note.text.clone()))
                .unwrap_or_else(|_| Task::none()),

            Action::MusicCycle => {
                let players = self.player_list();
                if players.len() > 1 {
                    let current = self.current(config).map(|player| player.bus.clone());
                    let index = players
                        .iter()
                        .position(|player| Some(&player.bus) == current.as_ref())
                        .map_or(0, |index| (index + 1) % players.len());
                    self.pinned = Some(players[index].bus.clone());
                    self.music_problem = false;
                    return self.art_task(config);
                }
                Task::none()
            }
            Action::NoteToggle(line) => {
                if !self.dirty()
                    && !self.saving()
                    && let Ok(note) = &self.note
                    && let Some(text) = notes::toggle(&note.text, line)
                {
                    let expected = note.text.clone();
                    self.content = text_editor::Content::with_text(&text);
                    return self.save_note(expected);
                }
                Task::none()
            }
            Action::FocusToggle => focus_command("toggle"),
            Action::FocusReset => focus_command("reset"),
            Action::FocusSkip => focus_command("skip"),
            Action::FocusMinutes(_)
            | Action::NoteEdit
            | Action::NoteSave
            | Action::NoteCancel
            | Action::NoteAction(_) => Task::none(),
        }
    }
}

/// What the page asks Settings to save.
pub enum Saved {
    Tool(Widget, &'static str, serde_json::Value),
    Note(Result<(), String>),
}

/// Sends a focus command to the desktop through its command file (beside
/// an alternate configuration in previews).
fn focus_command(command: &'static str) -> Task<crate::settings::Message> {
    let path = super::desktop_command_path();
    cosmic::task::future(async move {
        let result = tokio::task::spawn_blocking(move || {
            let path = path.ok_or_else(|| "Desktop session unavailable".to_string())?;
            let parent = path
                .parent()
                .ok_or_else(|| "Desktop command directory unavailable".to_string())?;
            std::fs::create_dir_all(parent).map_err(|error| error.to_string())?;
            let temp = path.with_extension("tmp");
            std::fs::write(
                &temp,
                serde_json::json!({"action": "focus", "do": command}).to_string(),
            )
            .map_err(|error| error.to_string())?;
            std::fs::rename(temp, path).map_err(|error| error.to_string())
        })
        .await
        .unwrap_or_else(|error| Err(error.to_string()));
        crate::settings::Message::Tools(Message::Done(result))
    })
}

#[cfg(test)]
mod timing_tests {
    use super::*;

    #[test]
    fn control_refresh_keeps_progress_in_settings_preview() {
        use std::sync::atomic::Ordering;
        let bus = music::fixture::Bus::new();
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap()
            .block_on(async {
                let state = music::fixture::State::new();
                state.playing.store(true, Ordering::SeqCst);
                let _service = music::fixture::serve(
                    &bus.address,
                    "org.mpris.MediaPlayer2.firefox",
                    state.clone(),
                )
                .await;
                let client = zbus::connection::Builder::address(bus.address.as_str())
                    .unwrap()
                    .build()
                    .await
                    .unwrap();
                let config = Config::default();
                let mut editor = ToolsEditor::default();
                let _ = editor.update(
                    Message::Players(Ok(music::snapshot(&client).await.unwrap())),
                    &config,
                );
                state.broken_timing.store(true, Ordering::SeqCst);
                let _ = editor.update(
                    Message::MediaControlDone(Ok(music::snapshot(&client).await.unwrap())),
                    &config,
                );
                let player = editor.current(&config).unwrap();
                assert_eq!(player.length_us, Some(300_000_000));
                assert!(player.position_us.unwrap() >= 60_000_000);
                assert_eq!(player.seek_request(0.5).unwrap().position_us, 150_000_000);
            });
    }
}
