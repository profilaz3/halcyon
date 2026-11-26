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
    pub(super) fn folder_page(&self) -> Element<'_, Message> {
        let sample = crate::folder::sample();
        let mut cfg = self.config.clone();
        cfg.folder.path = Some(sample.source.clone());
        cfg.folder.show_hidden = false;
        let preview = cards::folder(
            &sample,
            &cfg,
            self.tool_preview_size(Widget::Folder),
            |_| None,
            None,
        );
        let chosen = self.config.folder.path.as_deref().map_or_else(
            || "Choose the folder to show".into(),
            |p| p.display().to_string(),
        );
        self.widget_template(
            Widget::Folder,
            "Folder",
            Some(preview),
            vec![
                self.sample_note("folder-sample", None),
                group(
                    None,
                    vec![
                        group_row(
                            "Local folder",
                            &chosen,
                            button::standard("Choose folder…")
                                .on_press(Message::ChooseWidgetFolder)
                                .into(),
                        ),
                        group_row(
                            "Show hidden files",
                            "",
                            toggler(self.config.folder.show_hidden)
                                .on_toggle(|v| option(Widget::Folder, "show_hidden", v))
                                .into(),
                        ),
                    ],
                ),
            ],
        )
    }
    /// A price above and below for each watchlist item, saved with Enter.
    fn market_alerts(&self) -> Element<'_, Message> {
        let editor = &self.tools;
        let markets = &self.config.markets;
        let mut rows = vec![label(
            "Get a notification when a price goes above or below a level. Leave a box empty for no alert.",
            13.,
            FG,
        )];
        for symbol in &markets.symbols {
            let saved = markets.alerts.get(symbol).copied().unwrap_or_default();
            let shown = |above: bool, value: Option<f64>| {
                editor
                    .market_alert_drafts
                    .get(&(symbol.clone(), above))
                    .cloned()
                    .unwrap_or_else(|| value.map(|v| v.to_string()).unwrap_or_default())
            };
            let field = |above: bool, value: Option<f64>| {
                let symbol = symbol.clone();
                let submit = symbol.clone();
                text_input(if above { "Above" } else { "Below" }, shown(above, value))
                    .id(format!("market-alert-{}-{symbol}", if above { "above" } else { "below" })
                        .into())
                    .on_input(move |text| edit(Edit::MarketAlert(symbol.clone(), above, text)))
                    .on_submit(move |_| edit(Edit::MarketAlertSave(submit.clone())))
                    .width(Length::Fixed(110.))
            };
            rows.push(
                row::with_children(vec![
                    container(line(symbol, 14., FG)).width(Length::Fill).into(),
                    field(true, saved.above).into(),
                    field(false, saved.below).into(),
                ])
                .spacing(6)
                .align_y(Alignment::Center)
                .into(),
            );
            if let Some((_, error)) = editor.alert_error.as_ref().filter(|(s, _)| s == symbol) {
                rows.push(label(error, 13., WARN));
            }
        }
        group(Some("Price alerts"), rows)
    }

    pub(super) fn markets_page(&self) -> Element<'_, Message> {
        let editor = &self.tools;
        let preview = cards::markets(
            &editor.market_preview,
            &self.config,
            None,
            Some(self.tool_preview_size(Widget::Markets)),
        );
        use crate::markets::Provider;
        let provider = self.config.markets.provider;
        let sources = [
            ("Yahoo Finance", Provider::Public),
            ("Twelve Data", Provider::TwelveData),
        ];
        let source_controls = segmented(
            sources
                .into_iter()
                .map(|(name, source)| {
                    (
                        name.into(),
                        provider == source,
                        Some(edit(Edit::MarketProvider(source))),
                    )
                })
                .collect(),
            self.settings_accent(),
        );
        let source_note = match provider {
            Provider::Public => {
                "Yahoo supplies stock prices without signup. Currency pairs always use daily ECB reference rates."
            }
            Provider::TwelveData => "Twelve Data supplies stock prices. Currency pairs use the ECB and need no API key.",
        };
        let markets = &self.config.markets;
        let add = row::with_children(vec![
            text_input("Stock or pair, e.g. EUR/USD", &editor.market_symbols)
                .id("market-symbol-input".into())
                .on_input(|s| edit(Edit::MarketSymbols(s)))
                .on_submit(|_| edit(Edit::MarketAdd))
                .width(Length::Fill)
                .into(),
            button::text("Add")
                .on_press_maybe(
                    (!editor.market_symbols.trim().is_empty()).then(|| edit(Edit::MarketAdd)),
                )
                .into(),
        ])
        .spacing(6)
        .align_y(Alignment::Center);
        let mut watchlist = vec![add.into()];
        if let Some(error) = &editor.watchlist_error {
            watchlist.push(label(error, 13., WARN));
        }
        for (index, symbol) in markets.symbols.iter().enumerate() {
            let mut item = row::with_children(vec![
                container(line(symbol, 14., FG)).width(Length::Fill).into(),
            ])
            .spacing(4)
            .align_y(Alignment::Center);
            for (icon_name, hint, enabled, message) in [
                (
                    "go-up-symbolic",
                    "Move up",
                    index > 0,
                    Edit::MarketMove(symbol.clone(), -1),
                ),
                (
                    "go-down-symbolic",
                    "Move down",
                    index + 1 < markets.symbols.len(),
                    Edit::MarketMove(symbol.clone(), 1),
                ),
                (
                    "list-remove-symbolic",
                    "Remove",
                    markets.symbols.len() > 1,
                    Edit::MarketRemove(symbol.clone()),
                ),
            ] {
                let name = format!("{hint} {symbol}");
                let control = button::icon(icon::from_name(icon_name).size(16))
                    .extra_small()
                    .name(name.clone())
                    .id(format!("market-{icon_name}-{symbol}").into())
                    .on_press_maybe(enabled.then(|| edit(message)));
                item = item.push(super::tip::tip(control, &name));
            }
            watchlist.push(item.into());
        }
        let mut controls = vec![
            self.search_target("markets-watchlist", group(Some("Watchlist"), watchlist)),
            self.search_target("markets-display", group(
                Some("Show on widget"),
                vec![
                    self.search_target("markets-change", switch(
                        "Daily change",
                        "Percentage change since the previous quote",
                        markets.show_change,
                        |value| option(Widget::Markets, "show_change", value),
                    )),
                    self.search_target("markets-chart", switch(
                        "Price chart",
                        "Price history for the selected item",
                        markets.show_chart,
                        |value| option(Widget::Markets, "show_chart", value),
                    )),
                    self.search_target("markets-range", group_row(
                        "Chart range",
                        "How much price history the chart shows",
                        segmented(
                            crate::markets::Range::ALL
                                .into_iter()
                                .map(|range| {
                                    (
                                        range.label().into(),
                                        markets.chart_range == range,
                                        Some(option(
                                            Widget::Markets,
                                            "chart_range",
                                            serde_json::json!(range),
                                        )),
                                    )
                                })
                                .collect(),
                            self.settings_accent(),
                        ),
                    )),
                    self.search_target("markets-date", switch(
                        "Publication date",
                        "Date of the rate or time of the stock quote",
                        markets.show_date,
                        |value| option(Widget::Markets, "show_date", value),
                    )),
                    self.search_target("markets-details", switch(
                        "Source and chart details",
                        "Provider, chart dates, and low and high prices when space permits",
                        markets.show_details,
                        |value| option(Widget::Markets, "show_details", value),
                    )),
                ],
            )),
            self.search_target("markets-alerts", self.market_alerts()),
            group(
                Some("Data source"),
                vec![source_controls, label(source_note, 13., FG)],
            ),
        ];
        if provider == Provider::TwelveData {
            controls.push(group(
                None,
                vec![
                    button::text("Get an API key")
                        .on_press(Message::OpenNoteLink(
                            "https://twelvedata.com/account/api-keys",
                        ))
                        .into(),
                    label(
                        "Save your Twelve Data API key in your desktop keyring.",
                        13.,
                        FG,
                    ),
                    text_input("API key", &editor.market_key.0)
                        .password()
                        .on_input(|s| edit(Edit::MarketKey(crate::markets::Secret(s))))
                        .width(Length::Fill)
                        .into(),
                    row::with_children(vec![
                        button::text(if editor.market_busy {
                            "Working…"
                        } else {
                            "Save API key"
                        })
                        .on_press_maybe((!editor.market_busy).then(|| edit(Edit::MarketSaveKey)))
                        .into(),
                        button::text("Remove API key")
                            .on_press_maybe(
                                (!editor.market_busy).then(|| edit(Edit::MarketRemoveKey)),
                            )
                            .into(),
                    ])
                    .spacing(8)
                    .into(),
                ],
            ));
        }
        controls.push(label(
            "Checks every 15 minutes while visible. The ECB publishes rates on working days.",
            12.,
            MUTED,
        ));
        if let Some(status) = &editor.market_status {
            controls.push(label(status, 13., FG));
        }
        let preview = column::with_children(vec![
            container(label("Example prices", 13., FG))
                .id("markets-sample").width(Length::Fill).into(),
            self.preview_stage(container(preview).max_width(360.).into()),
        ]).spacing(6);
        let editor = adaptive::columns(
            column::with_children(controls).spacing(12).into(),
            preview.into(),
            [320., 300.],
            false,
        );
        self.widget_template(Widget::Markets, "Markets", None, vec![editor])
    }

    fn tool_preview_size(&self, widget: Widget) -> [f32; 2] {
        let [width, height] = self
            .config
            .widget_sizes
            .get(widget.key())
            .copied()
            .map(|size| {
                crate::layout::fitted_size(
                    widget,
                    size,
                    self.placement.screen(),
                    self.config.markets.height_limit(),
                )
            })
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
        let sample = cards::recipe::paused_sample();
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
        let mut own = Vec::new();
        if example {
            own.push(self.sample_note(
                "music-sample-note",
                Some("play something in a music app to see it here."),
            ));
        }
        own.extend([
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
        ]);
        own.push(self.search_target("music-album-light", group(
            Some("Album lighting"),
            vec![switch(
                "Light the card with the album cover",
                "Adds a soft reflection to dark widget backgrounds when artwork is visible",
                self.config.music.album_light,
                |value| option(Widget::Music, "album_light", value),
            )],
        )));
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
                        let at = at.with_timezone(&chrono::Local);
                        format!(
                            "Saved {}, {}",
                            at.format("%b %-d"),
                            crate::config::time_of_day(&at, self.config.clock.format_24h)
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
            note.push(label(crate::tools::notes::not_saved(problem), 12., ERROR));
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
                page: 0,
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
                        "You resized it by hand. To pick a size here, use Fit content in Move or resize",
                    )
                } else {
                    detail("Card size", "Grows with your note, up to this size")
                },
                sizes,
            ),
            self.search_target("notes-pages", switch(
                "Pages",
                "Each heading line starting with # begins a page the card shows on its own",
                self.config.notes.pages,
                |value| option(Widget::Notes, "pages", value),
            ))],
        );
        // The card as it looks on the desktop leads, like every widget page;
        // the editor for its text follows.
        let mut sections = Vec::new();
        if example {
            sections.push(self.sample_note(
                "notes-sample-note",
                Some("write your note below and it shows here."),
            ));
        }
        sections.extend([document, items]);
        self.widget_template(Widget::Notes, "Notes", Some(preview), sections)
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
    /// example devices so the look can be judged, and says they are samples.
    pub(super) fn devices_page(&self) -> Element<'_, Message> {
        let preview = cards::devices(
            &cards::sample_devices(),
            &self.config,
            None,
            Some(self.tool_preview_size(Widget::Devices)),
        );
        let note = self.sample_note("devices-sample-note", Some(DEVICES_SAMPLE));
        self.widget_template(Widget::Devices, "Devices", Some(preview), vec![note])
    }

    /// Messages: the card with example conversations, whether it shows
    /// their text, and how to connect a phone through KDE Connect.
    pub(super) fn messages_page(&self) -> Element<'_, Message> {
        let preview = cards::messages(
            &cards::sample_messages(),
            &self.config,
            None,
            Some(self.tool_preview_size(Widget::Messages)),
        );
        let note = self.sample_note("messages-sample-note", Some(MESSAGES_SAMPLE));
        let installed = crate::app::desktop_entry("org.kde.kdeconnect.app").is_some();
        let connect: Element<'_, Message> = if installed {
            button::text("Open KDE Connect")
                .on_press(edit(Edit::OpenKdeConnect))
                .into()
        } else {
            cosmic::iced::widget::space().into()
        };
        let items = vec![
            note,
            group(
                Some("Messages"),
                vec![
                    switch(
                        "Show in activity island",
                        "Read conversations in the island, even when the desktop widget is off",
                        self.config.activity_island.messages,
                        |value| Message::ActivityPreference("messages", value),
                    ),
                    switch(
                        "Show message text",
                        "Off hides message text in both the widget and the activity island",
                        self.config.messages.show_text,
                        |value| option(Widget::Messages, "show_text", value),
                    ),
                ],
            ),
            group(
                Some("Your phone"),
                vec![
                    group_row_state(
                        "KDE Connect",
                        if installed {
                            "Pair your phone in KDE Connect. Then, in KDE Connect on the phone, turn on SMS and allow it to read messages."
                        } else {
                            "Install KDE Connect on this computer (the kdeconnect package) and on your phone, then pair them."
                        },
                        connect,
                    ),
                    group_row_state(
                        "Names",
                        "Names come from the contacts KDE Connect copies from your phone. Without them, numbers show.",
                        cosmic::iced::widget::space().into(),
                    ),
                    group_row_state(
                        "Privacy",
                        "Messages stay on this computer. Halcyon reads them from KDE Connect only while the widget or activity island needs them, and never saves them.",
                        cosmic::iced::widget::space().into(),
                    ),
                ],
            ),
        ];
        self.widget_template(Widget::Messages, "Messages", Some(preview), items)
    }
}

/// Under the Messages preview: what it shows and where real messages come
/// from.
const MESSAGES_SAMPLE: &str = "your phone’s newest conversations show here, \
     through KDE Connect.";

/// Under the Devices preview: what it shows and where real readings come from.
const DEVICES_SAMPLE: &str = "your Bluetooth and wireless \
     devices show here, and phones paired with KDE Connect or GSConnect.";

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
