//! Native inspector with direct completed controls and local form validation.
use super::desktop::{ERROR, WARN, banner, conflict_banner, line};
use super::system_appearance::fill;
use super::windows::{recovery_banner, store_pill};
use super::*;
use crate::desktop::window_rules::{ListState, Match, RuleRow, WindowRulesDraft, classify};
use crate::settings::window_rules::{Message as Edit, RuleForm};
use cosmic::Apply;

const APP_ICON: &str = "application-x-executable-symbolic";

fn edit(message: Edit) -> Message {
    Message::WindowRules(message)
}

/// A labelled text field that keeps its label above narrow inputs. Enter
/// saves the rule, like the form's Add or Save button.
fn field<'a>(
    title: &'static str,
    placeholder: &'static str,
    value: &str,
    message: impl Fn(String) -> Edit + 'a,
) -> Element<'a, Message> {
    column::with_children(vec![
        label(title, 12., FG),
        text_input(placeholder, value.to_string())
            .on_input(move |text| edit(message(text)))
            .on_submit(|_| edit(Edit::Save))
            .width(Length::Fill)
            .into(),
    ])
    .spacing(5)
    .into()
}

/// “Any app”, the exact ID, “contains …” or “Pattern”, for one line.
fn app_text(pattern: &str) -> String {
    match classify(pattern) {
        Match::Any => "Any app".into(),
        Match::Exact(id) => id,
        Match::Contains(text) => format!("App ID contains “{text}”"),
        Match::Pattern(_) => "App ID pattern".into(),
    }
}
fn title_text(pattern: &str) -> String {
    match classify(pattern) {
        Match::Any => "Any title".into(),
        Match::Exact(text) => format!("Title is “{text}”"),
        Match::Contains(text) => format!("Title contains “{text}”"),
        Match::Pattern(_) => "Title pattern".into(),
    }
}
fn is_pattern(row: &RuleRow) -> bool {
    matches!(classify(&row.appid), Match::Pattern(_))
        || matches!(classify(&row.title), Match::Pattern(_))
        || row.appid.chars().count() > 40
        || row.title.chars().count() > 40
}

impl Settings {
    pub(super) fn window_rules_page(&self) -> Element<'_, Message> {
        let accent = self.settings_accent();
        let editor = &self.window_rules;
        let mut items: Vec<Element<'_, Message>> = Vec::new();
        if let Some(error) = editor.unavailable() {
            items.push(split(
                heading("Apps that don’t tile", ""),
                store_pill(editor.store(), accent),
            ));
            items.push(banner(
                format!("Floating-app rules are unavailable. {error}"),
                ERROR,
                Vec::new(),
            ));
            return column::with_children(items).spacing(14).into();
        }
        let Some(draft) = editor.draft() else {
            items.push(split(
                heading("Apps that don’t tile", ""),
                store_pill(editor.store(), accent),
            ));
            items.push(label("Reading floating-app rules…", 12., MUTED));
            return column::with_children(items).spacing(14).into();
        };
        let rows = draft.rows();
        items.push(
            column::with_children(vec![
                // The main action sits top right, beside the title.
                split(
                    heading("Apps that don’t tile", ""),
                    row::with_children(vec![
                        store_pill(editor.store(), accent),
                        button::standard("Add app")
                            .leading_icon(icon::from_name("list-add-symbolic").size(14))
                            .on_press_maybe(
                                (editor.form.is_none() && draft.editable())
                                    .then_some(edit(Edit::Add)),
                            )
                            .into(),
                    ])
                    .spacing(10)
                    .align_y(Alignment::Center)
                    .into(),
                ),
                label(
                    "When tiling is on, these apps still open as normal windows you can move and resize.",
                    13.,
                    MUTED,
                ),
            ])
            .spacing(4)
            .into(),
        );
        // The rules only matter with tiling on; say so, with the way there.
        if self
            .windows
            .draft()
            .is_some_and(|windows| {
                windows
                    .flag(crate::desktop::windows::Behavior::Autotile)
                    .known()
                    == Some(&false)
            })
        {
            items.push(group(
                None,
                vec![group_row_state(
                    "Tiling is off",
                    "These rules start working when windows tile",
                    button::standard("Turn on tiling")
                        .id(cosmic::widget::Id::new("window-rules-tiling"))
                        .on_press(Message::Windows(
                            crate::settings::windows::Message::Autotile(true),
                        ))
                        .into(),
                )],
            ));
        }
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
        match draft.custom_state() {
            ListState::Exact => {}
            ListState::Unknown => items.push(banner(
                "Your floating-app rules include settings this version doesn’t know. COSMIC applies the rules shown; they are kept as they are and can’t be changed here.",
                WARN,
                Vec::new(),
            )),
            ListState::Unreadable(error) => items.push(banner(
                format!("Your floating-app rules can’t be read ({error}). COSMIC ignores them; they are kept as they are and can’t be changed here."),
                ERROR,
                Vec::new(),
            )),
        }
        match draft.defaults_state() {
            ListState::Exact => {}
            ListState::Unknown => items.push(banner(
                "COSMIC’s default floating-app rules include settings this version doesn’t know. The rules are shown as COSMIC applies them and are never changed here.",
                WARN,
                Vec::new(),
            )),
            ListState::Unreadable(_) => items.push(banner(
                "COSMIC’s default floating-app rules can’t be read, so COSMIC uses none of them. They are never changed here.",
                WARN,
                Vec::new(),
            )),
        }
        let blocked = editor_blocked(editor);
        if let Some(reason) = blocked {
            items.push(banner(
                format!("Can’t apply: these rules {reason}. Discard or remove some rules."),
                ERROR,
                vec![button::text("Discard").on_press(edit(Edit::Discard)).into()],
            ));
        }
        if !rows.is_empty() || !editor.search.is_empty() {
            items.push(
                text_input("Search", editor.search.clone())
                    .on_input(|text| edit(Edit::Search(text)))
                    .width(Length::Fill)
                    .into(),
            );
        }
        if let Some(form) = &editor.form {
            items.push(self.inspector_layout(
                self.rule_list(&rows, accent),
                self.rule_form(form, accent),
                "floating-rules",
            ));
        } else {
            items.push(self.rule_list(&rows, accent));
        }
        column::with_children(items).spacing(16).into()
    }

    fn rule_form<'a>(&'a self, form: &'a RuleForm, accent: Color) -> Element<'a, Message> {
        let editing = form.editing.is_some();
        let mut items = Vec::new();
        if form.advanced {
            items.push(field(
                "App ID pattern",
                "^org\\.example\\.App$",
                &form.app_pattern,
                Edit::AppPattern,
            ));
            items.push(field(
                "Title pattern",
                "Empty matches any title",
                &form.title_pattern,
                Edit::TitlePattern,
            ));
            items.push(label(
                "Rust regular expressions, matched anywhere in the text; both must match. Use ^…$ to match a whole ID.",
                11.,
                MUTED,
            ));
        } else {
            items.push(
                button::text(if form.picker {
                    "Hide installed apps"
                } else {
                    "Choose an installed app…"
                })
                .leading_icon(icon::from_name("view-app-grid-symbolic").size(14))
                .on_press(edit(Edit::Picker(!form.picker)))
                .into(),
            );
            if form.picker {
                items.push(self.app_picker(form, accent));
            }
            items.push(field(
                "App ID",
                "org.example.App",
                &form.app_id,
                Edit::AppId,
            ));
            items.push(field(
                "Title contains (optional)",
                "Any title",
                &form.title,
                Edit::Title,
            ));
        }
        items.push(split(
            column::with_children(vec![
                label("Use patterns", 13., FG),
                label("For several apps or titles at once", 11., MUTED),
            ])
            .spacing(2)
            .into(),
            toggler(form.advanced)
                .on_toggle(|on| edit(Edit::Advanced(on)))
                .into(),
        ));
        if let Some(error) = &form.error {
            items.push(label(error.clone(), 11., ERROR));
        }
        items.push(
            row::with_children(vec![
                container(text::body("")).width(Length::Fill).into(),
                button::text("Cancel").on_press(edit(Edit::Cancel)).into(),
                button::suggested(if editing { "Save" } else { "Add" })
                    .on_press(edit(Edit::Save))
                    .into(),
            ])
            .spacing(10)
            .align_y(Alignment::Center)
            .into(),
        );
        group(
            Some(if editing {
                "Edit floating-app rule"
            } else {
                "Add floating app"
            }),
            items,
        )
    }

    fn app_picker<'a>(&'a self, form: &'a RuleForm, accent: Color) -> Element<'a, Message> {
        let query = form.picker_search.to_lowercase();
        let mut rows: Vec<Element<'a, Message>> = Vec::new();
        if let Some(apps) = self.window_rules.apps() {
            for app in apps.sorted().into_iter().filter(|app| {
                query.is_empty()
                    || app.name.to_lowercase().contains(&query)
                    || app.app_id.to_lowercase().contains(&query)
            }) {
                let symbol = if app.icon.is_empty() || app.icon.contains('/') {
                    APP_ICON.to_string()
                } else {
                    app.icon.clone()
                };
                rows.push(
                    button::custom(
                        row::with_children(vec![
                            icon::from_name(symbol).size(20).icon().into(),
                            column::with_children(vec![
                                line(app.name.clone(), 13., FG),
                                line(app.app_id.clone(), 11., MUTED),
                            ])
                            .spacing(1)
                            .width(Length::Fill)
                            .into(),
                        ])
                        .spacing(10)
                        .align_y(Alignment::Center),
                    )
                    .class(nav_style(false, accent))
                    .padding([4, 6])
                    .width(Length::Fill)
                    .on_press(edit(Edit::Pick(app.id.clone())))
                    .into(),
                );
            }
        }
        if rows.is_empty() {
            rows.push(label("No installed apps match.", 11., MUTED));
        }
        fill(
            column::with_children(vec![
                text_input("Search installed apps", form.picker_search.clone())
                    .on_input(|text| edit(Edit::PickerSearch(text)))
                    .width(Length::Fill)
                    .into(),
                scrollable(column::with_children(rows).spacing(2))
                    .height(Length::Fixed(168.))
                    .into(),
            ])
            .spacing(8)
            .into(),
            BASE,
            LINE,
            1.,
            12.,
        )
        .padding(10)
        .width(Length::Fill)
        .into()
    }

    fn rule_list<'a>(&'a self, rows: &[RuleRow], accent: Color) -> Element<'a, Message> {
        let editor = &self.window_rules;
        let query = editor.search.to_lowercase();
        let editable = editor.draft().is_some_and(WindowRulesDraft::editable);
        let apps = editor.apps();
        let mut items: Vec<Element<'a, Message>> = Vec::new();
        for row in rows {
            let app = match classify(&row.appid) {
                Match::Exact(id) => apps.and_then(|apps| apps.for_app_id(&id)),
                _ => None,
            };
            let name = app
                .map(|app| app.name.clone())
                .unwrap_or_else(|| app_text(&row.appid));
            if !query.is_empty()
                && ![&name, &row.appid, &row.title]
                    .iter()
                    .any(|text| text.to_lowercase().contains(&query))
            {
                continue;
            }
            items.push(self.rule_row(row, name, app.map(|app| app.icon.clone()), editable, accent));
        }
        if items.is_empty() {
            items.push(if rows.is_empty() {
                detail_state("No apps yet", "Use Add app to pick one.")
            } else {
                label(format!("No apps match “{}”.", editor.search), 12., MUTED)
            });
        }
        group(None, items)
    }

    fn rule_row<'a>(
        &'a self,
        row: &RuleRow,
        name: String,
        app_icon: Option<String>,
        editable: bool,
        accent: Color,
    ) -> Element<'a, Message> {
        let editor = &self.window_rules;
        let pair = (row.appid.clone(), row.title.clone());
        let expanded = editor.expanded.as_ref() == Some(&pair);
        let symbol = app_icon
            .filter(|icon| !icon.is_empty() && !icon.contains('/'))
            .unwrap_or_else(|| APP_ICON.to_string());
        let (state, tone) = match (row.default, row.overridden) {
            (true, true) => ("Default · changed", WARN),
            (true, false) => ("Default", MUTED),
            (false, _) => ("Custom", accent),
        };
        let mut condition = title_text(&row.title);
        if row.copies > 1 {
            condition.push_str(&format!(" · listed {}×", row.copies));
        }
        let mut controls: Vec<Element<'a, Message>> = vec![pill(state, tone)];
        if is_pattern(row) {
            controls.push(
                button::icon(icon::from_name(if expanded {
                    "go-up-symbolic"
                } else {
                    "go-down-symbolic"
                }))
                .extra_small()
                .on_press(edit(Edit::Expand((!expanded).then(|| pair.clone()))))
                .apply(|control| {
                    super::tip::tip(
                        control,
                        if expanded {
                            "Hide matching windows"
                        } else {
                            "Show matching windows"
                        },
                    )
                }),
            );
        }
        let (appid, title) = pair.clone();
        controls.push(
            toggler(row.enabled)
                .on_toggle_maybe(
                    editable
                        .then(|| move |on| edit(Edit::Toggle(appid.clone(), title.clone(), on))),
                )
                .into(),
        );
        if row.default && row.overridden {
            controls.push(
                button::text("Restore")
                    .on_press_maybe(
                        editable.then(|| edit(Edit::Remove(pair.0.clone(), pair.1.clone()))),
                    )
                    .into(),
            );
        } else if !row.default {
            controls.push(
                button::text("Edit")
                    .on_press_maybe(
                        (editable && editor.form.is_none())
                            .then(|| edit(Edit::Edit(pair.0.clone(), pair.1.clone()))),
                    )
                    .into(),
            );
            controls.push(
                button::icon(icon::from_name("list-remove-symbolic"))
                    .extra_small()
                    .on_press_maybe(
                        editable.then(|| edit(Edit::Remove(pair.0.clone(), pair.1.clone()))),
                    )
                    .apply(|control| super::tip::tip(control, "Remove window rule")),
            );
        }
        let summary = row::with_children(vec![
            icon::from_name(symbol).size(22).icon().into(),
            column::with_children(vec![
                line(name, 13., if row.enabled { FG } else { MUTED }),
                line(condition, 11., MUTED),
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
        if !expanded {
            return summary.into();
        }
        let detail = |title: &'static str, value: &str| -> Element<'a, Message> {
            column::with_children(vec![
                label(title, 10., MUTED),
                label(value.to_string(), 12., FG),
            ])
            .spacing(2)
            .into()
        };
        column::with_children(vec![
            summary.into(),
            fill(
                column::with_children(vec![
                    detail("App ID pattern", &row.appid),
                    detail(
                        "Title pattern",
                        if row.title.is_empty() {
                            "(empty: any title)"
                        } else {
                            &row.title
                        },
                    ),
                ])
                .spacing(6)
                .into(),
                BASE,
                LINE,
                1.,
                10.,
            )
            .padding(10)
            .width(Length::Fill)
            .into(),
        ])
        .spacing(8)
        .into()
    }
}

/// The draft's blocked reason, when it has one.
fn editor_blocked(editor: &crate::settings::window_rules::WindowRulesEditor) -> Option<String> {
    let draft = editor.draft()?;
    draft
        .dirty()
        .then(|| draft.plan().blocked.first().map(|b| b.reason.clone()))
        .flatten()
}
