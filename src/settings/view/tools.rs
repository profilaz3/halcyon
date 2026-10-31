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
    column::with_children(vec![label(title, 14., FG), segmented(options, accent)])
        .spacing(6)
        .width(Length::Fill)
        .into()
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
        let preview = cards::music(
            cards::MusicView {
                feed: tools.feed.as_ref(),
                player,
                art: tools.art(&self.config),
                now: std::time::Instant::now(),
                manually_selected: tools.pinned.is_some() || self.config.music.player.is_some(),
                problem: tools.music_problem,
            },
            &self.config,
            Some(on_preview),
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
            Some(music::Feed::Players(players)) if players.is_empty() => Some(
                "Play a video or song in your browser to see it here. Your browser chooses which tab to share with desktop controls.",
            ),
            Some(_) => None,
        };
        let mut follow = vec![
            split(
                label("Follow", 16., FG),
                button::text("Refresh players")
                    .on_press(edit(Edit::Refresh))
                    .into(),
            ),
            column::with_children(rows).spacing(2).into(),
        ];
        if let Some(hint) = hint {
            follow.push(label(hint, 12., MUTED));
        }
        let mut items = vec![
            group(Some("Player"), follow),
            label(
                "Live controls play, pause or seek in your selected desktop player.",
                12.,
                MUTED,
            ),
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
                    Some("Music card"),
                    vec![
                        switch("Album art", "", self.config.music.show_artwork, |value| {
                            option(Widget::Music, "show_artwork", value)
                        }),
                        switch(
                            "Progress",
                            "Click the bar to seek when the player supports it.",
                            self.config.music.show_progress,
                            |value| option(Widget::Music, "show_progress", value),
                        ),
                        button::text("Design a Music widget…")
                            .on_press(Message::OpenRecipe(crate::recipes::Kind::Music))
                            .into(),
                    ],
                )
            },
        ];
        items.insert(0, self.visualizer_controls());
        items.push(self.widget_style_and_layout(Widget::Music));
        column::with_children(vec![
            self.widget_heading(Widget::Music, "Music"),
            self.editor_with_preview(
                preview,
                column::with_children(items).spacing(16).into(),
                "Live controls",
            ),
        ])
        .spacing(16)
        .into()
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
            .placeholder("Write anything. Start a line with [ ] for a checklist.")
            .height(Length::Fixed((self.inspector_height() - 88.).max(180.)))
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
            format!("Unsaved · {} characters", tools.note_chars())
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
                            at.with_timezone(&chrono::Local).format("%-d %b, %H:%M")
                        )
                    })
                    .unwrap_or_else(|_| "Saved".into()),
                None => "Not written yet".into(),
            }
        };
        let mut note = vec![
            split(label("Note", 16., FG), label("Ctrl+S saves", 11., MUTED)),
            container(editor)
                .id(cosmic::widget::Id::new("note-editor"))
                .width(Length::Fill)
                .into(),
            row::with_children(vec![
                container(label(state, 12., if readable { MUTED } else { WARN }))
                    .width(Length::Fill)
                    .into(),
                button::text("Revert")
                    .id(cosmic::widget::Id::new("note-revert"))
                    .on_press_maybe((dirty && !tools.saving()).then(|| edit(Edit::NoteRevert)))
                    .into(),
                button::suggested("Save note")
                    .id(cosmic::widget::Id::new("note-save"))
                    .on_press_maybe(
                        (dirty && readable && !tools.saving()).then(|| edit(Edit::NoteSave)),
                    )
                    .into(),
            ])
            .spacing(10)
            .align_y(Alignment::Center)
            .into(),
        ];
        if tools.restored && dirty {
            note.push(banner(
                "Your unsaved edits from last time are back. Save puts them on the desktop.",
                accent,
                Vec::new(),
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
        let preview = cards::notes(
            cards::NotesView {
                note: tools.note.as_ref().map_err(String::as_str),
                editing: None,
                paused: false,
                problem: None,
            },
            &self.config,
            (!dirty && !tools.saving()).then_some(on_preview as fn(cards::Action) -> Message),
            Some(self.tool_preview_size(Widget::Notes)),
        );
        let document = group(None, note);
        let mut items = vec![
            group(
                Some("Note size"),
                vec![
                    choice_row(
                        "Automatic size",
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
                    ),
                    if self.config.widget_sizes.contains_key(Widget::Notes.key()) {
                        split(
                            detail(
                                "Custom size is active",
                                "Return to Automatic size in Layout to use these presets.",
                            ),
                            button::text("Edit size in Layout")
                                .on_press(Message::Page(Page::Overview))
                                .into(),
                        )
                    } else {
                        label(
                            "The card grows with its content within this size.",
                            11.,
                            MUTED,
                        )
                    },
                ],
            ),
            label(
                "Notes stay on this computer. Setups and exports never include them.",
                12.,
                MUTED,
            ),
        ];
        items.push(self.widget_style_and_layout(Widget::Notes));
        column::with_children(vec![
            self.widget_heading(Widget::Notes, "Notes"),
            self.inspector_layout(
                document,
                column::with_children(items).spacing(16).into(),
                "notes",
            ),
            self.disclosure(
                "notes-checklist",
                "Saved checklist · live controls",
                self.preview_stage(preview),
            ),
        ])
        .spacing(16)
        .into()
    }

    pub(super) fn focus_page(&self) -> Element<'_, Message> {
        let accent = self.settings_accent();
        let prefs = &self.config.focus;
        let visible = self.config.widget_visible(Widget::Focus);
        let preview = cards::focus(
            &self.tools.focus,
            &self.config,
            focus::now_ms(),
            &focus::today(),
            visible.then_some(on_preview as fn(cards::Action) -> Message),
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
        let mut items = vec![
            button::text("Design a Focus widget…")
                .on_press(Message::OpenRecipe(crate::recipes::Kind::Focus))
                .into(),
            label(
                if visible {
                    "Live controls run your desktop timer."
                } else {
                    "Show Focus timer to use its controls. Hiding it does not stop a running session."
                },
                12.,
                MUTED,
            ),
            group(
                Some("Session lengths"),
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
                Some("Timer behavior"),
                vec![
                    switch(
                        "Start breaks automatically",
                        "",
                        prefs.auto_start_breaks,
                        |value| option(Widget::Focus, "auto_start_breaks", value),
                    ),
                    switch("Notify when time is up", "", prefs.notify, |value| {
                        option(Widget::Focus, "notify", value)
                    }),
                ],
            ),
        ];
        items.push(self.widget_style_and_layout(Widget::Focus));
        column::with_children(vec![
            self.widget_heading(Widget::Focus, "Focus timer"),
            self.editor_with_preview(
                preview,
                column::with_children(items).spacing(16).into(),
                "Live controls",
            ),
        ])
        .spacing(16)
        .into()
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
        let mut rows=vec![self.help("visualizer-help","Audio visualizers","Reacts to all sound on the default audio output while your selected player is playing. Never reads the microphone, saves audio or uploads it. Stops when Music is hidden or Reduce motion is enabled. Circular size and display use Music’s Layout controls.")];
        rows.push(segmented(
            [
                (Circle::Off, "Card"),
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
        ));
        let next = p.clone();
        rows.push(switch("Spectrum", "", p.spectrum, move |enabled| {
            let mut p = next.clone();
            p.spectrum = enabled;
            option(
                Widget::Music,
                "visualizer",
                serde_json::to_value(p).unwrap(),
            )
        }));
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
                    label("Spectrum placement · sample audio", 12., MUTED),
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
                "",
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
            rows.push(self.help("spectrum-dock-help","Dock overlap","The dock stays in front and remains clickable. Opaque dock surfaces cover bars underneath. Turn this off to respect the space reserved by panels and docks."));
        }
        if !self.config.material_motion {
            rows.push(label(
                "Reduce motion is on. Audio visualization is paused.",
                12.,
                MUTED,
            ));
        }
        if !std::path::Path::new("/usr/bin/parec").exists() {
            rows.push(label(
                "Audio capture needs pulseaudio-utils. It is included as a package dependency.",
                12.,
                WARN,
            ));
        }
        group(Some("Visualizers"), rows)
    }
}
