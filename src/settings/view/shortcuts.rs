//! Native inspector with direct completed controls and local form validation.
use super::desktop::{ERROR, WARN, banner, conflict_banner, line};
use super::system_appearance::fill;
use super::windows::{recovery_banner, store_pill};
use super::*;
use crate::desktop::shortcuts::{Ask, Binding, KEYCODE_NOTE, Kind, Row};
use crate::settings::shortcuts::{Answer, Category, CommandForm, Filter, Message as Edit};
use cosmic::Apply;

fn edit(message: Edit) -> Message {
    Message::Shortcuts(message)
}

/// One key drawn as a small keycap.
pub(super) fn keycap<'a>(cap: impl Into<String>, muted: bool) -> Element<'a, Message> {
    fill(
        label(cap.into(), CAPTION, if muted { MUTED } else { FG }),
        SURFACE,
        LINE,
        1.,
        5.,
    )
    .padding([1, 6])
    .into()
}

/// The combination as a row of small keycaps.
fn keycaps<'a>(binding: &Binding, muted: bool) -> Element<'a, Message> {
    let caps: Vec<Element<'a, Message>> = binding
        .keycaps()
        .into_iter()
        .map(|cap| keycap(cap, muted))
        .collect();
    row::with_children(caps)
        .spacing(3)
        .align_y(Alignment::Center)
        .wrap()
        .into()
}

/// A labelled text field; Enter sends `submit`, like the form's main button.
fn field<'a>(
    title: &'static str,
    placeholder: &'static str,
    value: &str,
    message: impl Fn(String) -> Edit + 'a,
    submit: Option<Message>,
) -> Element<'a, Message> {
    column::with_children(vec![
        label(title, 12., FG),
        text_input(placeholder, value.to_string())
            .on_input(move |text| edit(message(text)))
            .on_submit_maybe(submit.map(|submit| move |_| submit.clone()))
            .width(Length::Fill)
            .into(),
    ])
    .spacing(5)
    .into()
}

/// The question and its answers, for a form or a row.
fn question<'a>(ask: &Ask) -> Option<Element<'a, Message>> {
    let answer = |title: String, answer: Answer, suggested: bool| -> Element<'a, Message> {
        if suggested {
            button::standard(title)
                .on_press(edit(Edit::Answer(answer)))
                .into()
        } else {
            button::text(title)
                .on_press(edit(Edit::Answer(answer)))
                .into()
        }
    };
    match ask {
        Ask::Replace { combo, current } => Some(banner(
            format!("{combo} already runs {current}. Replace it?"),
            WARN,
            vec![
                answer("Cancel".into(), Answer::Cancel, false),
                answer("Replace".into(), Answer::Replace, true),
            ],
        )),
        Ask::Resume { combo, default } => Some(
            column::with_children(vec![
                banner(
                    format!("Freeing {combo} brings back the built-in “{default}”."),
                    WARN,
                    Vec::new(),
                ),
                row::with_children(vec![
                    container(text::body("")).width(Length::Fill).into(),
                    answer("Cancel".into(), Answer::Cancel, false),
                    answer(format!("Keep {combo} off"), Answer::KeepOff, false),
                    answer("Turn it back on".into(), Answer::TurnBackOn, true),
                ])
                .spacing(8)
                .align_y(Alignment::Center)
                .wrap()
                .into(),
            ])
            .spacing(8)
            .into(),
        ),
        Ask::Refused(_) => None,
    }
}

fn matches(row: &Row, filter: Filter, query: &str) -> bool {
    let kind = match filter {
        Filter::All => true,
        Filter::Custom => !row.builtin(),
        Filter::Builtin => row.builtin(),
    };
    kind && (query.is_empty()
        || row.name.to_lowercase().contains(query)
        || row.binding.to_string().to_lowercase().contains(query)
        || match &row.kind {
            Kind::Command { command, .. } => command.to_lowercase().contains(query),
            Kind::Builtin { label, .. } | Kind::Override { label } => {
                label.to_lowercase().contains(query)
            }
            _ => false,
        })
}

impl Settings {
    pub(super) fn shortcuts_page(&self) -> Element<'_, Message> {
        let accent = self.settings_accent();
        let editor = &self.shortcuts;
        let mut items: Vec<Element<'_, Message>> = Vec::new();
        if let Some(error) = editor.unavailable() {
            items.push(split(
                heading("Keyboard shortcuts", ""),
                store_pill(editor.store(), accent),
            ));
            items.push(banner(
                format!("Shortcuts are unavailable. {error}"),
                ERROR,
                Vec::new(),
            ));
            return column::with_children(items).spacing(14).into();
        }
        let Some(draft) = editor.draft() else {
            items.push(split(
                heading("Keyboard shortcuts", ""),
                store_pill(editor.store(), accent),
            ));
            items.push(label("Reading shortcuts…", 12., MUTED));
            return column::with_children(items).spacing(14).into();
        };
        let rows = draft.rows();
        items.push(
            column::with_children(vec![
                // The main action sits top right, beside the title.
                split(
                    heading("Keyboard shortcuts", ""),
                    row::with_children(vec![
                        store_pill(editor.store(), accent),
                        button::standard("Add shortcut")
                            .leading_icon(icon::from_name("list-add-symbolic").size(14))
                            .on_press_maybe(
                                (!editor.busy() && draft.custom_problem().is_none())
                                    .then_some(edit(Edit::Add)),
                            )
                            .into(),
                    ])
                    .spacing(10)
                    .align_y(Alignment::Center)
                    .into(),
                ),
                label("Keys for windows, workspaces and your favorite apps.", 13., MUTED),
            ])
            .spacing(4)
            .into(),
        );
        if let Some(banner) = recovery_banner(
            editor.recovery(),
            editor.writable(),
            edit(Edit::Recover),
            edit(Edit::KeepRecovery),
        ) {
            items.push(banner);
        }
        if !draft.conflicts().is_empty() {
            items.push(conflict_banner(
                crate::settings::system_appearance::describe(draft.conflicts()),
                edit(Edit::UseCurrent),
                edit(Edit::KeepMine),
            ));
        }
        if let Some(problem) = draft.custom_problem() {
            items.push(banner(problem, ERROR, Vec::new()));
        }
        if let Err(error) = draft.defaults() {
            items.push(banner(
                format!("COSMIC’s default shortcuts can’t be read ({error}), so COSMIC uses none of them."),
                WARN,
                Vec::new(),
            ));
        }
        if draft.base_paused().is_err() {
            items.push(banner(
                "Paused commands can’t be read. They are kept as they are; pausing and resuming are off.",
                WARN,
                Vec::new(),
            ));
        }
        if !draft.duplicates().is_empty() {
            items.push(banner(
                "Some combinations are listed more than once; COSMIC uses the last copy. Remove the extra copy to edit shortcuts.",
                WARN,
                Vec::new(),
            ));
        }
        if let Some(pending) = &editor.pending
            && let Some(question) = question(&pending.ask)
        {
            items.push(question);
        }
        items.push(
            row::with_children(vec![
                cosmic::widget::search_input("Search all shortcuts", editor.search.clone())
                    .on_input(|text| edit(Edit::Search(text)))
                    .width(Length::Fill)
                    .into(),
                segmented(
                    [
                        ("Your shortcuts", Filter::Custom),
                        ("Built-in", Filter::Builtin),
                    ]
                    .into_iter()
                    .map(|(title, filter)| {
                        (
                            title.to_string(),
                            editor.filter == filter,
                            Some(edit(Edit::Filter(filter))),
                        )
                    })
                    .collect(),
                    accent,
                ),
            ])
            .spacing(10)
            .align_y(Alignment::Center)
            .wrap()
            .into(),
        );
        if let Some(form) = &editor.form {
            items.push(self.inspector_layout(
                self.shortcut_list(&rows),
                command_form(form, editor.recording(), editor.recording_error.as_deref()),
                "shortcuts",
            ));
        } else {
            items.push(self.shortcut_list(&rows));
        }
        column::with_children(items).spacing(16).into()
    }

    fn shortcut_list<'a>(&'a self, rows: &[Row]) -> Element<'a, Message> {
        let editor = &self.shortcuts;
        let query = editor.search.trim().to_lowercase();
        // Search is global within shortcuts, even when the custom tab is
        // selected; built-ins should remain easy to find from the default view.
        // With none of your own yet, the built-in ones show under the empty
        // note, so the page never opens on nothing.
        let filter = if !query.is_empty()
            || (editor.filter == Filter::Custom && rows.iter().all(|row| row.builtin()))
        {
            Filter::All
        } else {
            editor.filter
        };
        let mut items: Vec<Element<'a, Message>> = Vec::new();
        let matching: Vec<&Row> = rows
            .iter()
            .filter(|row| matches(row, filter, &query))
            .collect();
        if !query.is_empty() {
            items.push(label("Results across all shortcuts", CAPTION, MUTED));
        }
        let custom: Vec<&Row> = matching
            .iter()
            .copied()
            .filter(|row| !row.builtin())
            .collect();
        if !custom.is_empty() {
            let mut custom_items = Vec::new();
            for row in custom {
                custom_items.push(self.shortcut_row(row));
            }
            items.push(group(Some("Your shortcuts"), custom_items));
        }
        for category in Category::ALL {
            let grouped: Vec<&Row> = matching
                .iter()
                .copied()
                .filter(|row| row.builtin() && Category::for_row(row) == category)
                .collect();
            if grouped.is_empty() {
                continue;
            }
            let open = !query.is_empty() || editor.expanded_categories.contains(&category);
            let changed = grouped.iter().filter(|row| row.custom).count();
            let mut group_items = vec![split(
                button::text(category.title())
                    .leading_icon(
                        icon::from_name(if open {
                            "go-down-symbolic"
                        } else {
                            "go-next-symbolic"
                        })
                        .size(14),
                    )
                    .on_press_maybe(query.is_empty().then_some(edit(Edit::Category(category))))
                    .into(),
                label(
                    if changed > 0 {
                        format!("{} · {changed} changed", grouped.len())
                    } else {
                        grouped.len().to_string()
                    },
                    11.,
                    MUTED,
                ),
            )];
            if open {
                for row in grouped {
                    group_items.push(self.shortcut_row(row));
                }
            }
            items.push(group(None, group_items));
        }
        if matching.is_empty() {
            // One empty state: what is missing and what to do about it.
            items.push(group(None, vec![detail_state(
                if query.is_empty() && editor.filter == Filter::Custom {
                    "No shortcuts of your own yet".to_string()
                } else if query.is_empty() {
                    "COSMIC’s shortcuts couldn’t be read".to_string()
                } else {
                    format!("No shortcuts match “{}”", editor.search.trim())
                },
                if query.is_empty() && editor.filter == Filter::Custom {
                    "Use Add shortcut to open an app or run a command with keys you choose."
                } else if query.is_empty() {
                    "They still work; they just can’t be listed here."
                } else {
                    "Try an action name, command or key combination."
                },
            )]));
        }
        column::with_children(items).spacing(10).into()
    }

    fn shortcut_row<'a>(&'a self, row: &Row) -> Element<'a, Message> {
        let editor = &self.shortcuts;
        let free = !editor.busy();
        let ident = row.ident();
        let target = row.target();
        let expanded = editor.expanded == Some(ident);
        let mut details: Vec<String> = Vec::new();
        let mut controls: Vec<Element<'a, Message>> = Vec::new();
        let mut on = true;
        let mut long: Option<String> = None;
        match &row.kind {
            Kind::Command {
                command,
                running,
                displaced,
            } => {
                on = *running;
                details.push(if *running {
                    "Custom command".into()
                } else {
                    "Custom command · paused".into()
                });
                if let Some(other) = displaced {
                    details.push(format!("now used by {other}"));
                }
                if command.chars().count() > 32 {
                    long = Some(command.clone());
                } else {
                    details.push(command.clone());
                }
                let running = *running;
                controls.push(
                    toggler(running)
                        .on_toggle_maybe(free.then_some(move |on| edit(Edit::Running(ident, on))))
                        .into(),
                );
                if !row.keycode() {
                    controls.push(
                        button::text("Edit")
                            .on_press_maybe(free.then_some(edit(Edit::Edit(target))))
                            .into(),
                    );
                }
                controls.push(
                    button::icon(icon::from_name("list-remove-symbolic"))
                        .extra_small()
                        .on_press_maybe(free.then_some(edit(Edit::Remove(target))))
                        .apply(|control| super::tip::tip(control, "Remove shortcut")),
                );
            }
            Kind::Builtin {
                label: _,
                on: state,
            } => {
                on = *state;
                details.push(if row.custom {
                    "Built-in · turned off".into()
                } else {
                    "Built-in".into()
                });
                if !row.unused {
                    let state = *state;
                    controls.push(
                        toggler(state)
                            .on_toggle_maybe(
                                free.then_some(move |on| edit(Edit::Default(ident, on))),
                            )
                            .into(),
                    );
                }
            }
            Kind::Override { label } => {
                details.push(format!("Custom · {label}"));
                controls.push(self.restore_or_remove(row, free));
            }
            Kind::Off => {
                on = false;
                details.push("Custom · off, nothing runs".into());
                if row.custom {
                    controls.push(self.restore_or_remove(row, free));
                }
            }
            Kind::Unknown { raw } => {
                on = false;
                details.push("Not recognized · off in COSMIC, kept as is".into());
                long = Some(raw.clone());
                if row.custom {
                    controls.push(self.restore_or_remove(row, free));
                }
            }
        }
        if let Some(default) = row.default.as_ref().filter(|_| row.custom) {
            if !matches!(row.kind, Kind::Builtin { .. }) {
                details.push(format!("replaces “{default}”"));
            }
        }
        if row.keycode() {
            details.push(KEYCODE_NOTE.into());
        }
        if row.unused {
            details.push("repeated · COSMIC uses the other copy".into());
        }
        if row.duplicate {
            details.push("listed twice".into());
            if let Some(index) = row.index {
                controls.push(
                    button::text("Remove this copy")
                        .on_press_maybe(free.then_some(edit(Edit::RemoveCopy(index))))
                        .into(),
                );
            }
        }
        if long.is_some() {
            controls.insert(
                0,
                button::icon(icon::from_name(if expanded {
                    "go-up-symbolic"
                } else {
                    "go-down-symbolic"
                }))
                .extra_small()
                .on_press(edit(Edit::Expand((!expanded).then_some(ident))))
                .apply(|control| {
                    super::tip::tip(
                        control,
                        if expanded {
                            "Hide shortcut details"
                        } else {
                            "Show shortcut details"
                        },
                    )
                }),
            );
        }
        let summary = row::with_children(vec![
            container(keycaps(&row.binding, !on)).width(156).into(),
            column::with_children(vec![
                line(row.name.clone(), 13., if on { FG } else { MUTED }),
                line(details.join(" · "), CAPTION, MUTED),
            ])
            .spacing(1)
            .width(Length::Fill)
            .into(),
            row::with_children(controls)
                .spacing(6)
                .align_y(Alignment::Center)
                .into(),
        ])
        .spacing(10)
        .align_y(Alignment::Center);
        match long.filter(|_| expanded) {
            None => summary.into(),
            Some(text) => column::with_children(vec![
                summary.into(),
                fill(label(text, 12., FG), BASE, LINE, 1., 10.)
                    .padding(10)
                    .width(Length::Fill)
                    .into(),
            ])
            .spacing(8)
            .into(),
        }
    }

    fn restore_or_remove<'a>(&'a self, row: &Row, free: bool) -> Element<'a, Message> {
        let ident = row.ident();
        if row.default.is_some() {
            button::text("Restore default")
                .on_press_maybe(free.then_some(edit(Edit::Restore(ident))))
                .into()
        } else {
            button::icon(icon::from_name("list-remove-symbolic"))
                .extra_small()
                .on_press_maybe(free.then_some(edit(Edit::Remove(row.target()))))
                .apply(|control| super::tip::tip(control, "Remove shortcut"))
        }
    }
}

fn command_form<'a>(
    form: &'a CommandForm,
    recording: bool,
    recording_error: Option<&str>,
) -> Element<'a, Message> {
    let editing = form.editing.is_some();
    // Enter in a field does what the Add or Save button does, when it can.
    let save = (!recording && form.ask.is_none()).then_some(edit(Edit::Save));
    let mut items = Vec::new();
    // The question goes first, so it is in view without scrolling.
    if let Some(ask) = &form.ask
        && let Some(question) = question(ask)
    {
        items.push(question);
    }
    items.push(field(
        "Name",
        "Open notes",
        &form.name,
        Edit::Name,
        save.clone(),
    ));
    let mut combination_input = text_input("Super+Shift+K", form.combo.clone()).width(Length::Fill);
    if !recording {
        combination_input = combination_input
            .on_input(|text| edit(Edit::Combo(text)))
            .on_submit_maybe(save.clone().map(|save| move |_| save.clone()));
    }
    items.push(
        column::with_children(vec![
            label("Combination", 12., FG),
            row::with_children(vec![
                combination_input.into(),
                if recording {
                    button::standard("Cancel recording")
                        .on_press(edit(Edit::CancelRecording))
                        .into()
                } else {
                    button::standard("Record keys")
                        .leading_icon(icon::from_name("input-keyboard-symbolic").size(14))
                        .on_press_maybe(form.ask.is_none().then_some(edit(Edit::Record)))
                        .into()
                },
            ])
            .spacing(8)
            .align_y(Alignment::Center)
            .into(),
        ])
        .spacing(5)
        .into(),
    );
    if recording {
        items.push(label("Press a combination · Esc cancels", 13., FG));
        items.push(label(
            "If the desktop uses those keys, cancel and type the combination instead.",
            11.,
            MUTED,
        ));
        if let Some(error) = recording_error {
            items.push(label(error.to_owned(), CAPTION, ERROR));
        }
    }
    if !form.combo.trim().is_empty() {
        items.push(match form.binding() {
            Ok(binding) => keycaps(&binding, false),
            Err(error) => label(error, CAPTION, ERROR),
        });
    }
    items.push(field(
        "Command",
        "notes --new",
        &form.command,
        Edit::Command,
        save.clone(),
    ));
    items.push(label(
        "Runs when you press the combination after saving.",
        11.,
        MUTED,
    ));
    if let Some(error) = &form.error {
        items.push(label(error.clone(), CAPTION, ERROR));
    }
    items.push(
        row::with_children(vec![
            container(text::body("")).width(Length::Fill).into(),
            button::text("Cancel").on_press(edit(Edit::Cancel)).into(),
            button::suggested(if editing { "Save" } else { "Add" })
                .on_press_maybe(save)
                .into(),
        ])
        .spacing(10)
        .align_y(Alignment::Center)
        .into(),
    );
    group(
        Some(if editing {
            "Edit command"
        } else {
            "Add command"
        }),
        items,
    )
}
