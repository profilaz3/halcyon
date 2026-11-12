//! Music, Notes and Focus timer are individual widget settings pages.
//! Each starts with its on/off switch and the real desktop card.
use super::desktop::{ERROR, WARN, banner, line};
use super::*;
use crate::config::{FocusPreferences, NotesSize};
use crate::settings::tools::Message as Edit;
use crate::tools::{cards, focus, music};
use cosmic::widget::text_editor;

fn edit(message: Edit) -> Message {
    Message::Tools(message)
}
fn on_preview(action: cards::Action) -> Message {
    Message::Tools(Edit::Card(action))
}
fn option(widget: Widget, key: &'static str, value: impl Into<serde_json::Value>) -> Message {
    Message::Tools(Edit::Option(widget, key, value.into()))
}

/// A one-of row whose options wrap under a narrow window.
fn choice_row<'a>(
    title: &'static str,
    options: Vec<(String, bool, Option<Message>)>,
    accent: Color,
) -> Element<'a, Message> {
    // Label left, choice right, like every other row.
    split(
        label(title, 14., FG),
        row::with_children(vec![segmented(options, accent)])
            .align_y(Alignment::Center)
            .wrap()
            .into(),
    )
}

impl Settings {
    fn tool_preview_size(&self, widget: Widget) -> [f32; 2] {
        let [width, height] = self
            .config
            .widget_sizes
            .get(widget.key())
            .copied()
            .map(|size| crate::layout::sized(widget, size, self.placement.screen()))
            .unwrap_or_else(|| cards::size(widget, &self.config));
        [
            width.min(if self.window.width >= 990. {
                320.
            } else {
                360.
            }),
            height.min(400.),
        ]
    }

    pub(super) fn music_page(&self) -> Element<'_, Message> {
        let accent = self.settings_accent();
        let tools = &self.tools;
        let player = tools.current(&self.config);
        // Nothing playing: an example song, paused, so the options below
        // have something to show.
        static SAMPLE: std::sync::OnceLock<(crate::tools::music::Feed, crate::tools::music::Player)> =
            std::sync::OnceLock::new();
        let sample = SAMPLE.get_or_init(|| {
            let mut player = cards::recipe::sample_player(std::time::Instant::now());
            player.status = crate::tools::music::Playback::Paused;
            (crate::tools::music::Feed::Players(vec![player.clone()]), player)
        });
        let example = player.is_none()
            && !matches!(tools.feed, Some(crate::tools::music::Feed::Unavailable(_)));
        let preview = cards::music(
            cards::MusicView {
                feed: if example { Some(&sample.0) } else { tools.feed.as_ref() },
                player: if example { Some(&sample.1) } else { player },
                art: if example {
                    Some(cards::recipe::sample_art())
                } else {
                    tools.art(&self.config)
                },
                now: std::time::Instant::now(),
                manually_selected: tools.pinned.is_some() || self.config.music.player.is_some(),
                problem: tools.music_problem,
                last: None,
            },
            &self.config,
            // The example's buttons do nothing.
            (!example).then_some(on_preview as fn(cards::Action) -> Message),
            Some(self.tool_preview_size(Widget::Music)),
        );
        // Follow names an app, so it survives the app's restarts.
        let preferred = self.config.music.player.as_deref().map(music::app_key);
        let mut rows: Vec<Element<'_, Message>> = vec![self.player_row(
            "Automatic".into(),
            "Current or most recent player".into(),
            preferred.is_none(),
            Some(edit(Edit::Player(None))),
            accent,
        )];
        let mut seen = std::collections::BTreeSet::new();
        for player in tools.player_list().iter().filter(|p| seen.insert(p.app())) {
            let player = music::choose(tools.player_list(), None, Some(player.app()), &[]).unwrap();
            let state = match player.status {
                music::Playback::Playing => "Playing",
                music::Playback::Paused => "Paused",
                music::Playback::Stopped => "Not playing",
            };
            let detail = if player.has_track() {
                format!("{state} · {}", player.title)
            } else {
                state.to_string()
            };
            rows.push(separator());
            rows.push(self.player_row(
                player.name().to_string(),
                detail,
                preferred == Some(player.app()),
                Some(edit(Edit::Player(Some(player.app().to_string())))),
                accent,
            ));
        }
        if let Some(key) = preferred.filter(|key| {
            !tools
                .player_list()
                .iter()
                .any(|player| player.app() == *key)
        }) {
            rows.push(separator());
            rows.push(self.player_row(
                music::fallback_name(key),
                "Not running · used when it starts".into(),
                true,
                None,
                accent,
            ));
        }
        let hint = match &tools.feed {
            None => Some("Looking for players…"),
            Some(music::Feed::Unavailable(_)) => {
                Some("Media controls need the desktop session bus, which isn’t reachable.")
            }
            // The preview already says what to do when nothing is playing.
            Some(music::Feed::Players(players)) if players.is_empty() => None,
            Some(_) => None,
        };
        // The player list follows apps starting and quitting on its own.
        // With nothing else to pick, the one choice is a plain statement.
        let mut follow = if rows.len() == 1 {
            // Nothing to pick yet: a quiet note, not a row to click.
            vec![label(
                "Shows whichever app is playing. Open a music app to choose it here.",
                13.,
                MUTED,
            )]
        } else {
            vec![
                detail("Show music from", "Choose a player, or follow whichever is playing"),
                column::with_children(rows).spacing(2).into(),
            ]
        };
        if let Some(hint) = hint {
            follow.push(label(hint, 12., MUTED));
        }
        let own = vec![
            group(Some("Player"), follow),
            self.search_target("music-visualizers", self.visualizer_controls()),
            if let Some(recipe) = &self.config.recipes.music {
                group(
                    Some("Personal widget"),
                    vec![split(
                        label(&recipe.name, 14., FG),
                        button::text("Edit widget…")
                            .on_press(Message::OpenRecipe(crate::recipes::Kind::Music))
                            .into(),
                    )],
                )
            } else {
                group(
                    Some("Card"),
                    vec![
                        switch("Album art", "Cover of the song or video", self.config.music.show_artwork, |value| {
                            option(Widget::Music, "show_artwork", value)
                        }),
                        switch(
                            "Progress bar",
                            "Click it to jump to another point",
                            self.config.music.show_progress,
                            |value| option(Widget::Music, "show_progress", value),
                        ),
                    ],
                )
            },
        ];
        self.widget_template(Widget::Music, "Music", Some(preview), own)
    }

    fn player_row(
        &self,
        title: String,
        detail: String,
        selected: bool,
        message: Option<Message>,
        accent: Color,
    ) -> Element<'_, Message> {
        button::custom(
            row::with_children(vec![
                column::with_children(vec![
                    label(title, 14., if selected { FG } else { MUTED }),
                    line(detail, 11., MUTED),
                ])
                .spacing(2)
                .width(Length::Fill)
                .into(),
                label(if selected { "✓" } else { "" }, 14., accent),
            ])
            .spacing(10)
            .align_y(Alignment::Center),
        )
        .class(nav_style(selected, accent))
        .selected(selected)
        .padding([8, 12])
        .width(Length::Fill)
        .on_press_maybe(message)
        .into()
    }

    pub(super) fn notes_page(&self) -> Element<'_, Message> {
        let accent = self.settings_accent();
        let tools = &self.tools;
        let readable = tools.note.is_ok();
        let editor = text_editor(&tools.content)
            .id(cosmic::widget::Id::new("note-editor-text"))
            .placeholder("Write anything. Start a line with [ ] for a checklist.")
            .height(Length::Fixed(((self.window.height - 560.) * 0.6 + 160.).clamp(160., 360.)))
            .padding(12)
            .size(15.)
            .class(cosmic::style::iced::TextEditor::Custom(Box::new(
                |theme, status| {
                    let focused = matches!(status, text_editor::Status::Focused { .. });
                    let foreground: Color = theme.cosmic().on_bg_color().into();
                    text_editor::Style {
                        background: Background::Color(
                            theme.cosmic().background(theme.transparent).base.into(),
                        ),
                        border: Border {
                            color: if focused {
                                theme.cosmic().accent.base.into()
                            } else {
                                theme.current_container().component.divider.into()
                            },
                            width: if focused || theme.theme_type.is_high_contrast() {
                                2.
                            } else {
                                1.
                            },
                            radius: 10.into(),
                        },
                        placeholder: Color {
                            a: if theme.theme_type.is_high_contrast() {
                                1.
                            } else {
                                0.7
                            },
                            ..foreground
                        },
                        value: foreground,
                        selection: theme.cosmic().accent.base.into(),
                    }
                },
            )))
            .on_action(|action| edit(Edit::Note(action)))
            .key_binding(|press| {
                use cosmic::iced::keyboard::Key;
                if press.modifiers.control()
                    && matches!(&press.key, Key::Character(c) if c.as_str() == "s")
                {
                    Some(text_editor::Binding::Custom(edit(Edit::NoteSave)))
                } else if press.modifiers.control()
                    && matches!(&press.key, Key::Character(c) if c.eq_ignore_ascii_case("z"))
                {
                    Some(text_editor::Binding::Custom(edit(
                        if press.modifiers.shift() {
                            Edit::NoteRedo
                        } else {
                            Edit::NoteUndo
                        },
                    )))
                } else {
                    text_editor::Binding::from_key_press(press)
                }
            });
        let dirty = tools.dirty();
        let state = if tools.saving() {
            "Saving…".to_string()
        } else if !readable {
            "This note can’t be read; it is left as it is.".to_string()
        } else if dirty {
            "Saves as you type".to_string()
        } else {
            match tools
                .note
                .as_ref()
                .ok()
                .and_then(|note| note.updated_at.as_deref())
            {
                Some(at) => chrono::DateTime::parse_from_rfc3339(at)
                    .map(|at| {
                        format!(
                            "Saved {}",
                            at.with_timezone(&chrono::Local).format("%b %-d, %H:%M")
                        )
                    })
                    .unwrap_or_else(|_| "Saved".into()),
                None => String::new(),
            }
        };
        let mut note = vec![
            container(editor)
                .id(cosmic::widget::Id::new("note-editor"))
                .width(Length::Fill)
                .into(),
        ];
        // Undo for the note lives in the app's own Undo, like every other edit.
        if !state.is_empty() {
            note.push(label(state, 12., if readable { MUTED } else { WARN }));
        }
        if tools.restored && dirty {
            note.push(banner(
                "Your unsaved edits from last time are back.",
                accent,
                vec![
                    button::standard("Save them")
                        .id(cosmic::widget::Id::new("note-save"))
                        .on_press_maybe((!tools.saving()).then(|| edit(Edit::NoteSave)))
                        .into(),
                    button::text("Discard")
                        .id(cosmic::widget::Id::new("note-revert"))
                        .on_press_maybe((!tools.saving()).then(|| edit(Edit::NoteRevert)))
                        .into(),
                ],
            ));
        }
        if tools.conflict.is_some() {
            note.push(banner(
                "The note changed on the desktop while you were editing here.",
                WARN,
                vec![
                    button::standard("Use the desktop’s")
                        .on_press_maybe((!tools.saving()).then(|| edit(Edit::NoteUseSaved)))
                        .into(),
                    button::standard("Keep mine")
                        .on_press_maybe((!tools.saving()).then(|| edit(Edit::NoteKeepMine)))
                        .into(),
                ],
            ));
        }
        if let Some(problem) = &tools.problem {
            note.push(label(format!("Not saved: {problem}"), 12., ERROR));
        }
        // An empty note previews as a short example, so the card and the
        // checklist tip make sense before anything is written.
        static SAMPLE: std::sync::OnceLock<crate::tools::notes::Note> = std::sync::OnceLock::new();
        let example = tools.note.as_ref().is_ok_and(|note| note.text.trim().is_empty());
        let shown = if example {
            Ok(SAMPLE.get_or_init(|| crate::tools::notes::Note {
                text: "Weekend\n[x] Water the plants\n[ ] Call Sam\n[ ] Pick up the books".into(),
                updated_at: None,
            }))
        } else {
            tools.note.as_ref().map_err(String::as_str)
        };
        let preview = cards::notes(
            cards::NotesView {
                note: shown,
                editing: None,
                paused: false,
                problem: None,
                discarded: false,
            },
            &self.config,
            (!dirty && !tools.saving() && !example)
                .then_some(on_preview as fn(cards::Action) -> Message),
            Some(self.tool_preview_size(Widget::Notes)),
        );
        let document = group(Some("Your note"), note);
        let sizes = segmented(
                    NotesSize::ALL
                        .into_iter()
                        .map(|size| {
                            (
                                size.title().to_string(),
                                self.config.notes.size == size,
                                (!self.config.widget_sizes.contains_key(Widget::Notes.key()))
                                    .then(|| option(Widget::Notes, "size", size.key())),
                            )
                        })
                        .collect(),
                    accent,
                );
        let resized = self.config.widget_sizes.contains_key(Widget::Notes.key());
        let items = group(
            None,
            vec![split(
                if resized {
                    detail_state(
                        "Card size",
                        "You resized it by hand. Use Move or resize to change it",
                    )
                } else {
                    detail("Card size", "Grows with your note, up to this size")
                },
                sizes,
            )],
        );
        // The card as it looks on the desktop leads, like every widget page;
        // the editor for its text follows.
        self.widget_template(Widget::Notes, "Notes", Some(preview), vec![document, items])
    }

    pub(super) fn focus_page(&self) -> Element<'_, Message> {
        let accent = self.settings_accent();
        let prefs = &self.config.focus;
        let preview = cards::focus(
            &self.tools.focus,
            &self.config,
            focus::now_ms(),
            &focus::today(),
            Some(on_preview as fn(cards::Action) -> Message),
            Some(self.tool_preview_size(Widget::Focus)),
        );
        let minutes = |key: &'static str, choices: &[u16], current: u16| {
            let mut choices = choices.to_vec();
            if !choices.contains(&current) {
                choices.push(current);
                choices.sort_unstable();
            }
            choices
                .into_iter()
                .map(|value| {
                    (
                        format!("{value} min"),
                        value == current,
                        Some(option(Widget::Focus, key, value)),
                    )
                })
                .collect::<Vec<_>>()
        };
        let mut every = vec![0u8, 2, 3, 4, 6];
        if !every.contains(&prefs.long_break_every) {
            every.push(prefs.long_break_every);
            every.sort_unstable();
        }
        let items = vec![
            group(
                Some("How long"),
                vec![
                    choice_row(
                        "Focus",
                        minutes(
                            "focus_minutes",
                            &FocusPreferences::FOCUS_CHOICES,
                            prefs.focus_minutes,
                        ),
                        accent,
                    ),
                    choice_row(
                        "Break",
                        minutes(
                            "break_minutes",
                            &FocusPreferences::BREAK_CHOICES,
                            prefs.break_minutes,
                        ),
                        accent,
                    ),
                    choice_row(
                        "Long break",
                        minutes(
                            "long_break_minutes",
                            &FocusPreferences::LONG_BREAK_CHOICES,
                            prefs.long_break_minutes,
                        ),
                        accent,
                    ),
                    choice_row(
                        "Long break after",
                        every
                            .into_iter()
                            .map(|count| {
                                (
                                    if count == 0 {
                                        "Never".to_string()
                                    } else {
                                        format!("{count} rounds")
                                    },
                                    prefs.long_break_every == count,
                                    Some(option(Widget::Focus, "long_break_every", count)),
                                )
                            })
                            .collect(),
                        accent,
                    ),
                ],
            ),
            group(
                Some("When time is up"),
                vec![
                    switch(
                        "Start the break automatically",
                        "",
                        prefs.auto_start_breaks,
                        |value| option(Widget::Focus, "auto_start_breaks", value),
                    ),
                    switch("Send a notification", "", prefs.notify, |value| {
                        option(Widget::Focus, "notify", value)
                    }),
                ],
            ),
        ];
        self.widget_template(Widget::Focus, "Focus timer", Some(preview), items)
    }

    /// Devices has nothing to set besides on and off: the card shows three
    /// example devices so the look can be judged.
    pub(super) fn devices_page(&self) -> Element<'_, Message> {
        let preview = cards::devices(
            &cards::sample_devices(),
            &self.config,
            Some(self.tool_preview_size(Widget::Devices)),
        );
        self.widget_template(Widget::Devices, "Devices", Some(preview), Vec::new())
    }
}

impl Settings {
    fn visualizer_controls(&self) -> Element<'_, Message> {
        use crate::tools::visualizer::Circle;
        let p = self
            .tools
            .visualizer_draft
            .as_ref()
            .unwrap_or(&self.config.music.visualizer)
            .clone();
        let set = |p| {
            option(
                Widget::Music,
                "visualizer",
                serde_json::to_value(p).unwrap(),
            )
        };
        let mut rows: Vec<Element<'_, Message>> = Vec::new();
        rows.push(group_row(
            "Around the cover",
            "Moves with the sound. Nothing is recorded.",
            segmented(
            [
                (Circle::Off, "None"),
                (Circle::Bars, "Radial bars"),
                (Circle::Dots, "Orbit dots"),
                (Circle::Rings, "Pulse rings"),
            ]
            .into_iter()
            .map(|(c, title)| {
                let mut next = p.clone();
                next.circle = c;
                (title.into(), p.circle == c, Some(set(next)))
            })
            .collect(),
            self.settings_accent(),
            ),
        ));
        let next = p.clone();
        rows.push(self.search_target(
            "music-spectrum",
            switch("Bars along the bottom", "On the desktop, under your windows", p.spectrum, move |enabled| {
                let mut p = next.clone();
                p.spectrum = enabled;
                option(
                    Widget::Music,
                    "visualizer",
                    serde_json::to_value(p).unwrap(),
                )
            }),
        ));
        if p.spectrum {
            let sample: Element<'_, Message> =
                cosmic::widget::canvas(crate::tools::visualizer::drawing::Visual {
                    bins: std::array::from_fn(|i| 0.15 + (i as f32 * 0.37).sin().abs() * 0.7),
                    circle: Circle::Off,
                    color: Color {
                        a: f32::from(p.opacity) / 100.,
                        ..self.settings_accent()
                    },
                    region: Some(p.clone()),
                    backdrop: None,
                })
                .width(Length::Fill)
                .height(150)
                .into();
            rows.push(
                column::with_children(vec![
                    label("Where the bars appear", 12., MUTED),
                    self.preview_stage_padded(sample, 0),
                ])
                .spacing(6)
                .into(),
            );
            for (key, title, value, range) in [
                ("opacity", "Opacity", p.opacity, 0..=100),
                ("width", "Width", p.width, 10..=100),
                ("height", "Height", p.height, 4..=100),
                ("x", "Horizontal position", p.x, 0..=100),
                ("y", "Vertical position", p.y, 0..=100),
            ] {
                let next = p.clone();
                rows.push(
                    column::with_children(vec![
                        split(
                            label(title, 13., FG),
                            label(format!("{value}%"), 12., MUTED),
                        ),
                        container(
                            slider(range, value, move |v| {
                                let mut p = next.clone();
                                match key {
                                    "opacity" => p.opacity = v,
                                    "width" => p.width = v,
                                    "height" => p.height = v,
                                    "x" => p.x = v,
                                    _ => p.y = v,
                                };
                                edit(Edit::Visualizer(p))
                            })
                            .on_release(edit(Edit::VisualizerSave))
                            .width(Length::Fill)
                            .apply(|v| completed_slider(v.into(), edit(Edit::VisualizerSave))),
                        )
                        .id(cosmic::widget::Id::new(format!("spectrum-{key}")))
                        .into(),
                    ])
                    .spacing(6)
                    .into(),
                );
            }
            let mut full = p.clone();
            full.width = 100;
            full.x = 50;
            let mut bottom = p.clone();
            bottom.y = 100;
            rows.push(
                row::with_children(vec![
                    button::text("Full width").on_press(set(full)).into(),
                    button::text("Bottom edge").on_press(set(bottom)).into(),
                ])
                .spacing(8)
                .into(),
            );
            let next = p.clone();
            rows.push(switch(
                "Allow behind dock",
                "The dock stays in front and clickable",
                p.behind_dock,
                move |enabled| {
                    let mut p = next.clone();
                    p.behind_dock = enabled;
                    option(
                        Widget::Music,
                        "visualizer",
                        serde_json::to_value(p).unwrap(),
                    )
                },
            ));
        }
        if self.system_reduce_motion {
            rows.push(label("Paused while Reduce motion is on", 12., MUTED));
        } else if !self.config.material_motion {
            rows.push(label("Paused because Liquid motion is off", 12., MUTED));
        }
        if !std::path::Path::new("/usr/bin/parec").exists() {
            rows.push(label(
                "Audio capture needs pulseaudio-utils. It is included as a package dependency.",
                12.,
                WARN,
            ));
        }
        group(Some("Animation"), rows)
    }
}
