//! Automation subpage: a status card with the master switch and pause
//! choices, the ordered rules (each row on/off, up/down, edit, delete; one
//! row opens into its editor), and the recent switches.
use super::desktop::{ERROR, WARN, banner, line};
use super::pages::{group_label, section};
use super::windows::store_pill;
use super::*;
use crate::automation::solar::{self, Event, Source};
use crate::automation::status::{self as words, Tone};
use crate::automation::{Change, Days, Effect, Rule, Trigger};
use crate::settings::automation::{DraftEdit, Kind, Message as Edit, PauseFor, RuleDraft};
use chrono::Local;
use cosmic::widget::Space;

fn edit(message: Edit) -> Message {
    Message::Automation(message)
}

pub fn time_id() -> cosmic::widget::Id {
    cosmic::widget::Id::new("automation-time")
}

/// A labelled row in the rule editor.
fn field<'a>(title: &'static str, control: Element<'a, Message>) -> Element<'a, Message> {
    column::with_children(vec![label(title, 12., MUTED), control])
        .spacing(6)
        .width(Length::Fill)
        .into()
}

fn chip<'a>(title: String, on: bool, accent: Color, message: Message) -> Element<'a, Message> {
    button::custom(label(title, 12., if on { FG } else { MUTED }))
        .class(chip_style(on, accent))
        .selected(on)
        .padding([5, 11])
        .on_press(message)
        .into()
}

fn small_icon<'a>(name: &'static str, message: Option<Message>) -> Element<'a, Message> {
    button::icon(icon::from_name(name))
        .extra_small()
        .tooltip(match name {
            "go-up-symbolic" => "Move rule up in priority",
            "go-down-symbolic" => "Move rule down in priority",
            "edit-delete-symbolic" => "Remove rule",
            _ => "Rule action",
        })
        .on_press_maybe(message)
        .into()
}

impl Settings {
    fn h24(&self) -> bool {
        self.config.clock.format_24h
    }

    pub(super) fn automation_page(&self) -> Element<'_, Message> {
        let editor = &self.automation;
        let accent = self.settings_accent();
        let mut items = vec![split(
            column::with_children(vec![
                heading("Schedules & display rules", ""),
                label(
                    "Switch by the sun, a fixed time, or a display connection.",
                    12.,
                    MUTED,
                ),
            ])
            .spacing(4)
            .into(),
            store_pill(editor.store(), accent),
        )];
        if let Some(problem) = editor.unavailable() {
            items.push(banner(
                format!("Desktop settings unavailable: {problem}"),
                ERROR,
                Vec::new(),
            ));
            return column::with_children(items).spacing(16).into();
        }
        let settings = match editor.settings() {
            Ok(settings) => settings.clone(),
            Err(problem) if !problem.is_empty() => {
                items.push(banner(
                    format!("{problem} Automation is off until this is fixed."),
                    ERROR,
                    vec![
                        button::standard("Start over")
                            .on_press_maybe(editor.writable().then_some(edit(Edit::StartOver)))
                            .into(),
                    ],
                ));
                return column::with_children(items).spacing(16).into();
            }
            Err(_) => return label("Reading automation…", 12., MUTED),
        };
        if editor.draft.is_none() {
            items.push(self.automation_status(&settings));
        }
        items.push(self.automation_rules(&settings));
        if editor.draft.is_none() && !editor.state.recent.is_empty() {
            items.push(self.disclosure(
                "automation-recent",
                "Recent switches",
                self.automation_recent(),
            ));
        }
        column::with_children(items).spacing(16).into()
    }

    fn automation_status(&self, settings: &crate::automation::Automation) -> Element<'_, Message> {
        let editor = &self.automation;
        let accent = self.settings_accent();
        let now = chrono::Utc::now();
        let status = words::status_with_solar(
            settings,
            &editor.state,
            &Local,
            now,
            editor.manual_at,
            self.h24(),
            &editor.solar,
        );
        let (symbol, tone) = match status.tone {
            Tone::Quiet => ("media-playback-stop-symbolic", MUTED),
            Tone::Active => ("emblem-synchronizing-symbolic", accent),
            Tone::Paused => ("media-playback-pause-symbolic", WARN),
            Tone::Attention => ("dialog-warning-symbolic", WARN),
        };
        let mut text = vec![label(status.headline, 15., FG)];
        if let Some(detail) = status.detail {
            text.push(label(detail, 12., MUTED));
        }
        let mut toggle = toggler(settings.enabled);
        if editor.writable() {
            toggle = toggle.on_toggle(|on| edit(Edit::Enable(on)));
        }
        let head = split(
            row::with_children(vec![
                container(icon::from_name(symbol).size(20).icon())
                    .class(cosmic::theme::Container::custom(move |_| {
                        cosmic::iced::widget::container::Style {
                            icon_color: Some(tone),
                            ..Default::default()
                        }
                    }))
                    .into(),
                column::with_children(text).spacing(3).into(),
            ])
            .spacing(12)
            .align_y(Alignment::Center)
            .into(),
            toggle.into(),
        );
        let mut rows = vec![head];
        if editor.undoable
            && editor
                .state
                .last()
                .is_some_and(|last| matches!(last.effect, Effect::Applied { .. }))
        {
            rows.push(
                row::with_children(vec![
                    container(label(
                        "The last automatic setup switch is available to undo in Setups.",
                        12.,
                        MUTED,
                    ))
                    .width(Length::Fill)
                    .into(),
                    button::text("Undo in Setups")
                        .leading_icon(icon::from_name("edit-undo-symbolic").size(14))
                        .on_press(Message::Page(Page::DesktopProfiles))
                        .into(),
                ])
                .spacing(12)
                .align_y(Alignment::Center)
                .into(),
            );
        }
        let now_unix = now.timestamp();
        if settings.running() && editor.writable() {
            if settings.skipping_at(now_unix).is_some() {
                rows.push(separator());
                rows.push(
                    row::with_children(vec![
                        container(label(
                            "Only that switch is skipped. Everything else keeps working.",
                            12.,
                            MUTED,
                        ))
                        .width(Length::Fill)
                        .into(),
                        button::standard("Don’t skip")
                            .leading_icon(icon::from_name("edit-undo-symbolic").size(14))
                            .on_press(edit(Edit::Resume))
                            .into(),
                    ])
                    .spacing(12)
                    .align_y(Alignment::Center)
                    .into(),
                );
            } else if settings.paused_at(now_unix).is_some() {
                rows.push(separator());
                rows.push(
                    row::with_children(vec![
                        container(label(
                            "Rules don’t act while paused, and nothing is caught up afterwards.",
                            12.,
                            MUTED,
                        ))
                        .width(Length::Fill)
                        .into(),
                        button::standard("Resume")
                            .leading_icon(icon::from_name("media-playback-start-symbolic").size(14))
                            .on_press(edit(Edit::Resume))
                            .into(),
                    ])
                    .spacing(12)
                    .align_y(Alignment::Center)
                    .into(),
                );
            } else {
                let next = crate::automation::decide::next_instant(
                    settings,
                    &editor.state,
                    &Local,
                    now,
                    &editor.solar,
                )
                .is_some();
                let mut choices = vec![label("Pause", 12., MUTED)];
                let mut options = Vec::new();
                if next {
                    options.push(("Skip next", PauseFor::Next));
                }
                options.extend([
                    ("1 hour", PauseFor::Hour),
                    ("Until tomorrow", PauseFor::Today),
                    ("Until I resume", PauseFor::Resume),
                ]);
                for (title, choice) in options {
                    choices.push(
                        button::custom(label(title, 12., FG))
                            .class(chip_style(false, accent))
                            .padding([5, 11])
                            .on_press(edit(Edit::Pause(choice)))
                            .into(),
                    );
                }
                rows.push(separator());
                rows.push(
                    row::with_children(choices)
                        .spacing(6)
                        .align_y(Alignment::Center)
                        .wrap()
                        .into(),
                );
            }
        }
        section(rows)
    }

    fn automation_rules(&self, settings: &crate::automation::Automation) -> Element<'_, Message> {
        let editor = &self.automation;
        let writable = editor.writable();
        let mut rows: Vec<Element<'_, Message>> = Vec::new();
        let count = settings.rules.len();
        for (index, rule) in settings.rules.iter().enumerate() {
            if !rows.is_empty() {
                rows.push(separator());
            }
            rows.push(self.automation_rule_row(rule, index, count, writable));
            if editor.confirm_delete.as_deref() == Some(rule.id.as_str()) {
                rows.push(banner(
                    format!("Remove the rule for {}?", rule.name),
                    WARN,
                    vec![
                        button::text("Keep")
                            .on_press(edit(Edit::Delete(rule.id.clone(), false)))
                            .into(),
                        button::destructive("Remove")
                            .on_press(edit(Edit::Delete(rule.id.clone(), true)))
                            .into(),
                    ],
                ));
            }
        }
        if rows.is_empty() {
            rows.push(label(
                if editor.draft.is_some() {
                    "No saved rules yet."
                } else {
                    "Add a sun event, fixed time or display rule."
                },
                12.,
                MUTED,
            ));
        }
        let adding =
            editor.draft.is_none() && writable && count < crate::automation::rules::MAX_RULES;
        if editor.draft.is_none() {
            rows.push(
                row::with_children(vec![
                    button::standard("Sun event")
                        .id(cosmic::widget::Id::new("automation-add-solar"))
                        .on_press_maybe(adding.then_some(edit(Edit::New(Kind::Solar))))
                        .into(),
                    button::standard("Fixed time")
                        .leading_icon(icon::from_name("list-add-symbolic").size(14))
                        .on_press_maybe(adding.then_some(edit(Edit::New(Kind::Schedule))))
                        .into(),
                    button::standard("Add display rule")
                        .leading_icon(icon::from_name("list-add-symbolic").size(14))
                        .on_press_maybe(adding.then_some(edit(Edit::New(Kind::Display))))
                        .into(),
                    container(label(
                        "Higher rules win when two happen at once",
                        11.,
                        MUTED,
                    ))
                    .width(Length::Fill)
                    .align_x(cosmic::iced::alignment::Horizontal::Right)
                    .into(),
                ])
                .spacing(8)
                .align_y(Alignment::Center)
                .wrap()
                .into(),
            );
        }
        let list = column::with_children(vec![
            group_label("Rules · highest priority first"),
            section(rows),
            label(
                "Your own changes stay until the next switch. Missed times catch up once, after sleep or a restart.",
                11.,
                MUTED,
            ),
        ])
        .spacing(8)
        .into();
        if let Some(draft) = &editor.draft {
            // The form gets the useful width; the rule list is supporting context.
            // At compact widths it follows the form instead of squeezing it.
            adaptive::columns(self.automation_rule_editor(draft), list, [520., 300.], true)
        } else {
            list
        }
    }

    fn automation_rule_row(
        &self,
        rule: &Rule,
        index: usize,
        count: usize,
        writable: bool,
    ) -> Element<'_, Message> {
        let editor = &self.automation;
        let free = writable && editor.draft.is_none();
        let symbol = match rule.when {
            Trigger::Solar { .. } => "weather-clear-symbolic",
            Trigger::Schedule { .. } => "alarm-symbolic",
            Trigger::Display(_) => "video-display-symbolic",
        };
        let name = editor.profile_name(&rule.profile, &rule.name);
        let target: Element<'_, Message> = match editor.rule_problem(rule) {
            Some(problem) => line(format!("→ {name} · {problem}"), 12., WARN),
            None => line(format!("→ {name}"), 12., MUTED),
        };
        let tone = if rule.enabled { FG } else { MUTED };
        if !free {
            return column::with_children(vec![
                label(rule.when.summary(self.h24()), 14., tone),
                target,
                self.solar_rule_next(rule),
            ])
            .spacing(6)
            .into();
        }

        let mut toggle = toggler(rule.enabled);
        if free {
            let id = rule.id.clone();
            toggle = toggle.on_toggle(move |on| edit(Edit::Rule(id.clone(), on)));
        }
        row::with_children(vec![
            icon::from_name(symbol).size(18).icon().into(),
            container(
                column::with_children(vec![
                    line(rule.when.summary(self.h24()), 14., tone),
                    target,
                    self.solar_rule_next(rule),
                ])
                .spacing(2),
            )
            .width(Length::Fill)
            .into(),
            small_icon(
                "go-up-symbolic",
                (free && index > 0).then(|| edit(Edit::Move(rule.id.clone(), -1))),
            ),
            small_icon(
                "go-down-symbolic",
                (free && index + 1 < count).then(|| edit(Edit::Move(rule.id.clone(), 1))),
            ),
            button::text("Edit")
                .on_press_maybe(free.then(|| edit(Edit::Edit(rule.id.clone()))))
                .into(),
            small_icon(
                "edit-delete-symbolic",
                free.then(|| edit(Edit::Delete(rule.id.clone(), false))),
            ),
            toggle.into(),
        ])
        .spacing(8)
        .align_y(Alignment::Center)
        .into()
    }

    fn solar_rule_next(&self, rule: &Rule) -> Element<'_, Message> {
        let Trigger::Solar {
            event,
            days,
            source,
        } = rule.when
        else {
            return Space::new().height(0).into();
        };
        self.solar_next(event, days, source)
    }

    fn solar_next(&self, event: Event, days: Days, source: Source) -> Element<'_, Message> {
        let context = &self.automation.solar;
        let Ok(city) = context.city(source) else {
            return Space::new().height(0).into();
        };
        let now = chrono::Utc::now();
        let text = match solar::next(context, source, event, days, now) {
            Some(at) => format!(
                "{} · {} · {} ({})",
                event.title(),
                words::when(
                    at.with_timezone(&Local).naive_local(),
                    now.with_timezone(&Local).naive_local(),
                    self.h24()
                ),
                city.name,
                city.timezone.as_deref().unwrap_or("")
            ),
            None => format!(
                "{} · no {} in the next 8 days",
                city.name,
                event.title().to_lowercase()
            ),
        };
        label(text, 11., MUTED)
    }

    fn solar_rule_controls(&self, draft: &RuleDraft) -> Element<'_, Message> {
        let accent = self.settings_accent();
        let mut controls = vec![
            split(self.help("solar-event", "Sun event", draft.event.help()),
                segmented(Event::ALL.into_iter().map(|event| (event.title().into(), event == draft.event,
                    Some(edit(Edit::Draft(DraftEdit::Event(event)))))).collect(), accent)),
            split(self.help("solar-city", "City", "Weather follows your enabled Weather widget or Daily overview section. A schedule city is separate and does not change Weather. Times are calculated locally; the next switch is shown in this computer's time zone."),
                segmented([(Source::Weather, "Weather city"), (Source::Manual, "Schedule city")].into_iter()
                    .map(|(source, title)| (title.into(), source == draft.source, Some(edit(Edit::Draft(DraftEdit::Source(source)))))).collect(), accent)),
        ];
        controls.push(self.schedule_city_controls(draft.source));
        controls.push(self.solar_next(draft.event, draft.days, draft.source));
        column::with_children(controls).spacing(10).into()
    }

    pub(super) fn schedule_city_controls(&self, source: Source) -> Element<'_, Message> {
        let mut controls = Vec::new();
        let context = &self.automation.solar;
        let summary = match context.city(source) {
            Ok(city) => label(
                format!("{} · {}", city.name, city.timezone.as_deref().unwrap_or("")),
                12.,
                FG,
            ),
            Err(reason) => label(reason, 12., WARN),
        };
        if source == Source::Weather {
            controls.push(split(
                summary,
                button::text("Open Weather")
                    .on_press(Message::Page(Page::Weather))
                    .into(),
            ));
        } else if context.city(Source::Manual).is_ok() && !self.schedule_city_editing {
            controls.push(split(
                summary,
                button::text("Change…")
                    .on_press(Message::EditScheduleCity(true))
                    .into(),
            ));
        } else {
            controls.push(summary);
            let state = &self.schedule_city_search;
            controls.push(
                row::with_children(vec![
                    text_input("Find a schedule city", &state.query)
                        .id(cosmic::widget::Id::new("solar-city-search"))
                        .on_input(Message::ScheduleCityInput)
                        .on_submit(|_| Message::SearchScheduleCity)
                        .width(Length::Fill)
                        .into(),
                    button::standard("Search")
                        .on_press_maybe(
                            (state.query.trim().chars().count() >= 2 && !state.searching)
                                .then_some(Message::SearchScheduleCity),
                        )
                        .into(),
                ])
                .spacing(8)
                .into(),
            );
            if state.searching || state.deadline.is_some() {
                controls.push(label("Searching cities…", 12., MUTED));
            } else if let Some(error) = &state.error {
                controls.push(label(error, 12., WARN));
            } else if state.searched && self.schedule_cities.is_empty() {
                controls.push(label("No cities found. Try a nearby city.", 12., MUTED));
            }
            for (index, city) in self.schedule_cities.iter().enumerate() {
                let zone = city.timezone.as_deref().unwrap_or("Time zone unavailable");
                controls.push(
                    button::standard(format!("{} · {} · {zone}", city.name, city.region()))
                        .on_press_maybe(
                            solar::validate_city(&city.location())
                                .is_ok()
                                .then_some(Message::ChooseScheduleCity(index)),
                        )
                        .into(),
                );
            }
        }
        column::with_children(controls).spacing(8).into()
    }

    fn automation_rule_editor(&self, draft: &RuleDraft) -> Element<'_, Message> {
        let editor = &self.automation;
        let accent = self.settings_accent();
        let title = match (draft.kind, draft.id.is_some()) {
            (Kind::Solar, false) => "New sun schedule",
            (Kind::Solar, true) => "Edit sun schedule",
            (Kind::Schedule, false) => "New schedule",
            (Kind::Schedule, true) => "Edit schedule",
            (Kind::Display, false) => "New display rule",
            (Kind::Display, true) => "Edit display rule",
        };
        let mut items = vec![label(title, 14., FG)];
        match draft.kind {
            Kind::Schedule | Kind::Solar => {
                if draft.kind == Kind::Solar {
                    items.push(self.solar_rule_controls(draft));
                } else {
                    let hint: Element<'_, Message> = match draft.parsed_time() {
                        // Shows how a loosely typed time was read.
                        Some(time) if time.format(self.h24()) != draft.time.trim() => label(
                            format!("{} · local time", time.format(self.h24())),
                            11.,
                            MUTED,
                        ),
                        Some(_) => label("Local time", 11., MUTED),
                        None if draft.time.trim().is_empty() => label("e.g. 19:00", 11., MUTED),
                        None => label("Use a time like 19:00 or 7 pm", 11., WARN),
                    };
                    items.push(field(
                        "At",
                        row::with_children(vec![
                            text_input("19:00", draft.time.clone())
                                .id(time_id())
                                .on_input(|text| edit(Edit::Draft(DraftEdit::Time(text))))
                                .on_submit(|_| edit(Edit::Save))
                                .width(96)
                                .into(),
                            hint,
                        ])
                        .spacing(10)
                        .align_y(Alignment::Center)
                        .into(),
                    ));
                }
                let days: Vec<Element<'_, Message>> = (0..7)
                    .map(|index| {
                        chip(
                            Days::short(index).into(),
                            draft.days.has(index),
                            accent,
                            edit(Edit::Draft(DraftEdit::Day(index))),
                        )
                    })
                    .collect();
                let presets = segmented(
                    [
                        ("Every day", Days::EVERY),
                        ("Weekdays", Days::WEEKDAYS),
                        ("Weekends", Days::WEEKEND),
                    ]
                    .into_iter()
                    .map(|(title, preset)| {
                        (
                            title.to_string(),
                            draft.days == preset,
                            Some(edit(Edit::Draft(DraftEdit::Days(preset)))),
                        )
                    })
                    .collect(),
                    accent,
                );
                items.push(field(
                    "On",
                    column::with_children(vec![
                        row::with_children(days).spacing(4).wrap().into(),
                        presets,
                    ])
                    .spacing(8)
                    .into(),
                ));
            }
            Kind::Display => {
                items.push(field(
                    "When",
                    segmented(
                        [
                            ("Connects", Change::Connect),
                            ("Disconnects", Change::Disconnect),
                        ]
                        .into_iter()
                        .map(|(title, change)| {
                            (
                                title.to_string(),
                                draft.change == change,
                                Some(edit(Edit::Draft(DraftEdit::Change(change)))),
                            )
                        })
                        .collect(),
                        accent,
                    ),
                ));
                let group = match draft.change {
                    Change::Connect => "First external display",
                    Change::Disconnect => "Last external display",
                };
                let mut choices = vec![chip(
                    group.into(),
                    draft.display.is_none(),
                    accent,
                    edit(Edit::Draft(DraftEdit::Display(None))),
                )];
                for (name, title, connected) in editor.display_choices() {
                    let on = draft.display.as_deref() == Some(name.as_str());
                    let title = if connected {
                        title
                    } else {
                        format!("{title} · not connected")
                    };
                    choices.push(chip(
                        title,
                        on,
                        accent,
                        edit(Edit::Draft(DraftEdit::Display(Some(name)))),
                    ));
                }
                items.push(field(
                    "Display",
                    row::with_children(choices).spacing(6).wrap().into(),
                ));
            }
        }
        let targets: Vec<_> = editor
            .profiles
            .iter()
            .map(|(profile, _)| profile.clone())
            .collect();
        let selected = targets
            .iter()
            .position(|profile| Some(profile) == draft.profile.as_ref());
        items.push(field(
            if draft.kind == Kind::Solar {
                "Appearance"
            } else {
                "Setup"
            },
            cosmic::widget::dropdown(&editor.profile_labels, selected, move |index| {
                edit(Edit::Draft(DraftEdit::Profile(targets[index].clone())))
            })
            .placeholder(if draft.kind == Kind::Solar {
                "Choose appearance…"
            } else {
                "Choose setup…"
            })
            .width(Length::Fill)
            .into(),
        ));
        if draft.profile.is_some() && selected.is_none() {
            items.push(label("The chosen setup is unavailable", 12., WARN));
        }
        let city_problem = (draft.kind == Kind::Solar)
            .then(|| editor.solar.city(draft.source).err())
            .flatten();
        let missing = draft.missing().or(city_problem.as_deref());
        let enables_automation = editor
            .settings()
            .is_ok_and(|settings| draft.enables_automation(settings));
        let save_label = if enables_automation {
            "Save & enable"
        } else {
            "Save rule"
        };
        items.push(
            row::with_children(vec![
                container(match missing {
                    Some(missing) => label(missing, 11., MUTED),
                    None => label(
                        match draft.kind {
                            Kind::Schedule => {
                                "Applies at that time, once a day on the chosen days."
                            }
                            Kind::Solar => {
                                "Changes appearance only; layout and content stay as they are."
                            }
                            Kind::Display => "Applies once each time this happens.",
                        },
                        11.,
                        MUTED,
                    ),
                })
                .width(Length::Fill)
                .into(),
                button::text("Cancel").on_press(edit(Edit::Cancel)).into(),
                button::suggested(save_label)
                    .id(cosmic::widget::Id::new("automation-save"))
                    .on_press_maybe(
                        (missing.is_none() && editor.writable()).then_some(edit(Edit::Save)),
                    )
                    .into(),
            ])
            .spacing(10)
            .align_y(Alignment::Center)
            .into(),
        );
        container(column::with_children(items).spacing(12))
            .padding([12, 14])
            .width(Length::Fill)
            .class(cosmic::theme::Container::custom(move |theme| {
                cosmic::iced::widget::container::Style {
                    background: Some(Background::Color(surface_color(theme, BASE))),
                    border: Border {
                        color: Color { a: 0.5, ..accent },
                        width: 1.,
                        radius: 12.into(),
                    },
                    ..Default::default()
                }
            }))
            .into()
    }

    fn automation_recent(&self) -> Element<'_, Message> {
        let editor = &self.automation;
        let accent = self.settings_accent();
        let now = chrono::Utc::now();
        let now_local = crate::automation::clock::local(&Local, now);
        let mut rows = Vec::new();
        for outcome in editor.state.recent.iter().take(4) {
            if !rows.is_empty() {
                rows.push(separator());
            }
            let words::Line { what, cause, why } = words::outcome_line(outcome, self.h24());
            let (symbol, tone) = match &outcome.effect {
                Effect::Applied { skipped } if skipped.is_empty() => ("✓", accent),
                Effect::Applied { .. } | Effect::Waiting { .. } => ("!", WARN),
                Effect::AlreadySet | Effect::KeptManual { .. } | Effect::Replaced { .. } => {
                    ("•", MUTED)
                }
                Effect::Failed { .. } => ("!", ERROR),
            };
            let at = crate::automation::clock::local(
                &Local,
                crate::automation::clock::from_unix(outcome.at),
            );
            let mut lines = vec![line(what, 13., FG), line(cause, 11., MUTED)];
            if let Some(why) = why {
                lines.push(label(why, 11., if tone == ERROR { ERROR } else { WARN }));
            }
            rows.push(
                row::with_children(vec![
                    container(label(symbol, 13., tone)).width(14).into(),
                    container(column::with_children(lines).spacing(2))
                        .width(Length::Fill)
                        .into(),
                    label(words::stamp(at, now_local, self.h24()), 11., MUTED),
                ])
                .spacing(10)
                .align_y(Alignment::Center)
                .into(),
            );
        }
        section(rows)
    }
}
