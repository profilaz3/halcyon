//! Automation subpage: a status card with the master switch and pause
//! choices, the ordered rules (each row on/off, up/down, edit, delete; one
//! row opens into its editor), and the recent switches.
use super::desktop::{ERROR, WARN, banner, line};
use super::pages::section;
use super::windows::store_pill;
use super::*;
use crate::automation::solar::{self, Event, Source};
use crate::automation::status::{self as words, Tone};
use crate::automation::{Change, Days, Effect, Rule, Trigger};
use crate::settings::automation::{DraftEdit, Kind, Message as Edit, PauseFor, RuleDraft};
use chrono::Local;
use cosmic::Apply;
use cosmic::widget::Space;

fn edit(message: Edit) -> Message {
    Message::Automation(message)
}

/// The start of a part of the day, in everyday words.
fn moment(event: Event) -> &'static str {
    match event {
        Event::GoldenHour => "about an hour before sunset",
        Event::Dawn => "dawn",
        Event::Sunrise => "sunrise",
        Event::Noon => "midday",
        Event::Sunset => "sunset",
    }
}

fn rule_editor_title(draft: &RuleDraft) -> &'static str {
    match (draft.kind, draft.id.is_some()) {
        (Kind::Solar, false) => "New sun schedule",
        (Kind::Solar, true) => "Edit sun schedule",
        (Kind::Schedule, false) => "New schedule",
        (Kind::Schedule, true) => "Edit schedule",
        (Kind::Display, false) => "New display rule",
        (Kind::Display, true) => "Edit display rule",
    }
}

pub fn time_id() -> cosmic::widget::Id {
    cosmic::widget::Id::new("automation-time")
}

/// A labelled row in the rule editor.
fn field<'a>(title: &'static str, control: Element<'a, Message>) -> Element<'a, Message> {
    let control = super::focus::control(Some(title.to_owned()), None, control);
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
        .on_press_maybe(message)
        .apply(|control| {
            super::tip::tip(
                control,
                match name {
                    "go-up-symbolic" => "Move rule up in priority",
                    "go-down-symbolic" => "Move rule down in priority",
                    "edit-delete-symbolic" => "Remove rule",
                    _ => "Rule action",
                },
            )
        })
}

impl Settings {
    fn h24(&self) -> bool {
        self.config.clock.format_24h
    }

    pub(super) fn automation_page(&self) -> Element<'_, Message> {
        let editor = &self.automation;
        let accent = self.settings_accent();
        let title = if let Some(draft) = &editor.draft {
            // The active form owns the page title. Repeating a title inside
            // the card pushes its primary action below compact viewports.
            heading(rule_editor_title(draft), "")
        } else {
            heading(
                "Time of day",
                "Your look, or just your widgets' style, switches on its own through the day.",
            )
        };
        let mut items = vec![split(title, store_pill(editor.store(), accent))];
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
        if editor.draft.is_some() {
            items.push(self.automation_rules(&settings));
            return column::with_children(items).spacing(16).into();
        }
        // One switch, then what changes, where you are and the three parts
        // of the day; choosing what a part of the day uses turns it on.
        let widgets = self.time_widget_mode(&settings);
        let source = self.day_source(&settings);
        if self.automation.solar.city(source).is_err() {
            // Sun times need a city, so finding it is the one first step.
            // A switch with nothing to switch would only repeat that. What
            // works without a city stays: the widget styles (set and shown
            // by hand) and, below, set times and screens.
            if !settings.rules.is_empty() {
                items.push(self.automation_status(&settings));
            }
            items.push(
                container(group(None, vec![self.schedule_city_controls(source)]))
                    .id(cosmic::widget::Id::new("time-of-day-first-step"))
                    .into(),
            );
            items.push(self.time_mode(&settings, widgets));
        } else {
            items.push(self.automation_status(&settings));
            items.push(self.time_mode(&settings, widgets));
            items.push(self.automation_day(&settings, widgets));
        }
        if widgets {
            items.push(self.moment_style_section());
        }
        if let (false, Some(active)) = (widgets, self.config.moments.active) {
            // A widget style left on from before would hide the look's own.
            items.push(group(
                None,
                vec![group_row_state(
                    format!("Widgets keep the {} style", active.title().to_lowercase()),
                    "Looks switch, but widgets stay in this style",
                    button::standard("Use each look’s style")
                        .id(cosmic::widget::Id::new("time-of-day-style-reset"))
                        .on_press(Message::Moment(crate::settings::moments::Message::Activate(None)))
                        .into(),
                )],
            ));
        }
        items.push(self.disclosure(
            "automation-more",
            "Set times and screens",
            self.automation_rules(&settings),
        ));
        if !editor.state.recent.is_empty() {
            items.push(self.disclosure(
                "automation-recent",
                "Recent switches",
                self.automation_recent(),
            ));
        }
        column::with_children(items).spacing(16).into()
    }

    /// Widget style only when the daily slots say so; the chosen mode otherwise.
    pub(in crate::settings) fn time_widget_mode(&self, settings: &crate::automation::Automation) -> bool {
        self.automation.widget_style(settings)
    }

    /// What changes through the day: the whole look or only the widgets.
    fn time_mode(&self, settings: &crate::automation::Automation, widgets: bool) -> Element<'_, Message> {
        let writable = self.automation.writable() && !self.native_busy;
        let _ = settings;
        group(
            None,
            vec![group_row(
                "What changes",
                "Whole look switches wallpaper, colors and widgets. Widget style only switches between the day and night widget styles; wallpaper and colors stay.",
                container(segmented(
                    vec![
                        (
                            "Whole look".into(),
                            !widgets,
                            (writable && widgets).then(|| edit(Edit::WidgetStyle(false))),
                        ),
                        (
                            "Widget style only".into(),
                            widgets,
                            (writable && !widgets).then(|| edit(Edit::WidgetStyle(true))),
                        ),
                    ],
                    self.settings_accent(),
                ))
                .id(cosmic::widget::Id::new("time-of-day-mode"))
                .into(),
            )],
        )
    }

    /// The city the daily switches use: the one a sun rule already names,
    /// else Weather's when it has one, else the schedule's own.
    fn day_source(&self, settings: &crate::automation::Automation) -> Source {
        let context = &self.automation.solar;
        settings
            .rules
            .iter()
            .find_map(|rule| match rule.when {
                Trigger::Solar { source, .. } => Some(source),
                _ => None,
            })
            .unwrap_or(if context.city(Source::Weather).is_ok() {
                Source::Weather
            } else {
                Source::Manual
            })
    }

    /// Day, evening and night as three sky tiles, each with its look
    /// picker, then where you are (the times depend on it).
    fn automation_day(
        &self,
        settings: &crate::automation::Automation,
        widgets: bool,
    ) -> Element<'_, Message> {
        use crate::desktop::profiles::adaptive::Phase;
        let editor = &self.automation;
        let context = &editor.solar;
        let source = self.day_source(settings);
        let now = chrono::Utc::now();
        // The page shows this only once there is a city (see automation_page).
        let mut tiles: Vec<Element<'_, Message>> = Vec::new();
        for phase in Phase::ALL {
            let event = phase.event();
            let rule = settings.rules.iter().find(|rule| {
                matches!(rule.when, Trigger::Solar { event: e, days, .. }
                    if e == event && days == Days::EVERY)
            });
            // Each mode offers its own choices: looks, or widget styles.
            let (choices, labels) = if widgets {
                (&editor.style_choices, &editor.style_labels)
            } else {
                (&editor.look_choices, &editor.look_labels)
            };
            let selected = match rule {
                Some(rule) => choices
                    .iter()
                    .position(|(profile, _)| *profile == rule.profile)
                    .map(|index| index + 1),
                None => Some(0),
            };
            let starts = match solar::next(context, source, event, Days::EVERY, now) {
                Some(at) => {
                    let at = at.with_timezone(&Local);
                    format!(
                        "From {}, {}",
                        moment(event),
                        if self.h24() {
                            at.format("%H:%M").to_string()
                        } else {
                            at.format("%-I:%M %p").to_string()
                        }
                    )
                }
                None => format!("From {}", moment(event)),
            };
            let targets: Vec<_> = choices.iter().map(|(profile, _)| profile.clone()).collect();
            let picker = cosmic::widget::dropdown(labels, selected, move |index| {
                edit(Edit::Moment(
                    event,
                    index.checked_sub(1).map(|index| targets[index].clone()),
                ))
            })
            // Says what the slot holds when it isn't one of this mode's choices.
            .placeholder(match rule.map(|rule| &rule.profile) {
                Some(crate::automation::ProfileRef::Moment(_)) => "Uses a widget style",
                Some(_) if widgets => "Uses a whole look",
                _ => "Look unavailable",
            })
            .width(Length::Fill);
            let (top, bottom, ink) = match phase {
                // Deep enough that white text and the picker stay readable.
                Phase::Day => ((0.16, 0.45, 0.78), (0.38, 0.66, 0.90), Color::WHITE),
                Phase::Evening => ((0.45, 0.33, 0.62), (0.96, 0.62, 0.42), Color::WHITE),
                Phase::Night => ((0.05, 0.07, 0.16), (0.16, 0.18, 0.36), Color::WHITE),
            };
            tiles.push(
                container(
                    column::with_children(vec![
                        label(phase.title(), 17., ink),
                        label(starts, 12., Color { a: 0.8, ..ink }),
                        cosmic::iced::widget::space().height(Length::Fill).into(),
                        // A theme-coloured plate keeps the picker's text
                        // readable on every sky, in light and dark.
                        container(super::focus::control(
                            Some(phase.title().to_owned()),
                            None,
                            picker.into(),
                        ))
                            .class(cosmic::theme::Container::custom(|theme| {
                                let base: Color = theme.cosmic().background(theme.transparent).base.into();
                                cosmic::iced::widget::container::Style {
                                    background: Some(Background::Color(Color {
                                        a: 0.82,
                                        ..base
                                    })),
                                    border: Border {
                                        radius: 8.into(),
                                        ..Default::default()
                                    },
                                    ..Default::default()
                                }
                            }))
                            .into(),
                    ])
                    .spacing(2),
                )
                .padding([14, 14])
                .width(Length::Fill)
                .height(132)
                .class(cosmic::theme::Container::custom(move |_| {
                    cosmic::iced::widget::container::Style {
                        background: Some(Background::Gradient(
                            cosmic::iced::gradient::Linear::new(std::f32::consts::PI)
                                .add_stop(0., Color::from_rgb(top.0, top.1, top.2))
                                .add_stop(1., Color::from_rgb(bottom.0, bottom.1, bottom.2))
                                .into(),
                        )),
                        border: Border {
                            radius: 12.into(),
                            ..Default::default()
                        },
                        ..Default::default()
                    }
                }))
                .into(),
            );
        }
        use crate::desktop::profiles::adaptive::Look;
        let daily = |event| {
            settings.rules.iter().find(|rule| {
                matches!(rule.when, Trigger::Solar { event: e, days, .. }
                    if e == event && days == Days::EVERY)
            })
        };
        let shades = Look::ALL.into_iter().find(|look| {
            Phase::ALL.into_iter().all(|phase| {
                daily(phase.event()).is_some_and(|rule| {
                    rule.profile == crate::automation::ProfileRef::Preset(look.id(phase).into())
                })
            })
        });
        let ready = editor.writable() && !self.native_busy;
        let accent = self.settings_accent();
        // Where you are sets the times, so it comes before them.
        let mut items = vec![group(None, vec![self.schedule_city_controls(source)])];
        items.push(row::with_children(tiles).spacing(12).into());
        if widgets {
            return column::with_children(items).spacing(16).into();
        }
        let shifting: Element<'_, Message> = container(segmented(
            Look::ALL
                .into_iter()
                .map(|look| {
                    (
                        look.title().into(),
                        shades == Some(look),
                        (ready && shades != Some(look))
                            .then(|| edit(Edit::AdaptiveRoutine(look, source))),
                    )
                })
                .collect(),
            accent,
        ))
        .id(cosmic::widget::Id::new("automation-shades"))
        .into();
        items.push(group(
            None,
            vec![group_row(
                "Or one look that shifts through the day",
                "Its own lighter and darker shades fill all three",
                shifting,
            )],
        ));
        column::with_children(items)
        .spacing(16)
        .into()
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
        let tone = match status.tone {
            Tone::Attention => WARN,
            _ => MUTED,
        };
        // The master switch: on while anything is set to switch.
        if settings.rules.is_empty() {
            // Nothing to switch yet: no dead switch, only what to do first.
            let source = if editor.solar.city(Source::Weather).is_ok() {
                Source::Weather
            } else {
                Source::Manual
            };
            let next = if editor.solar.city(source).is_ok() {
                "Choose what a part of the day uses below to turn this on"
            } else {
                "Add where you are to begin"
            };
            return group(None, vec![detail_state("Off", next)]);
        }
        let text = if settings.enabled {
            let mut lines = vec![label(status.headline.trim_end_matches('.').to_string(), 14., FG)];
            if let Some(more) = status.detail {
                lines.push(label(more, 12., tone));
            }
            column::with_children(lines).spacing(2).into()
        } else {
            detail_state("Off", "Your choices below are kept for later")
        };
        let mut switch = toggler(settings.enabled);
        if editor.writable() {
            switch = switch.on_toggle(|on| edit(Edit::Enable(on)));
        }
        let head = split(
            text,
            container(switch)
                .id(cosmic::widget::Id::new("automation-enable"))
                .into(),
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
                        "The last automatic switch can be undone in Looks.",
                        12.,
                        MUTED,
                    ))
                    .width(Length::Fill)
                    .into(),
                    button::text("Undo in Looks")
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
                let mut choices = vec![label("Pause for", 12., MUTED)];
                let mut options = Vec::new();
                if next {
                    options.push(("Skip next", PauseFor::Next));
                }
                options.extend([
                    ("1 hour", PauseFor::Hour),
                    ("The rest of today", PauseFor::Today),
                    ("Until I turn it back on", PauseFor::Resume),
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
                rows.push(
                    row::with_children(choices)
                        .spacing(6)
                        .align_y(Alignment::Center)
                        .wrap()
                        .into(),
                );
            }
        }
        group(None, rows)
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
                    "No other rules yet."
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
                    button::standard("At sunrise or sunset")
                        .id(cosmic::widget::Id::new("automation-add-solar"))
                        .on_press_maybe(adding.then_some(edit(Edit::New(Kind::Solar))))
                        .into(),
                    button::standard("At a set time")
                        .leading_icon(icon::from_name("list-add-symbolic").size(14))
                        .on_press_maybe(adding.then_some(edit(Edit::New(Kind::Schedule))))
                        .into(),
                    button::standard("When a display connects")
                        .leading_icon(icon::from_name("list-add-symbolic").size(14))
                        .on_press_maybe(adding.then_some(edit(Edit::New(Kind::Display))))
                        .into(),
                ])
                .spacing(8)
                .align_y(Alignment::Center)
                .wrap()
                .into(),
            );
        }
        let list = column::with_children(vec![
            label(
                "Add a fixed time or a display rule. Rules higher in the list win.",
                12.,
                MUTED,
            ),
            section(rows),
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
        label(text, CAPTION, MUTED)
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
            Ok(city) if source == Source::Weather => {
                detail_state("Where you are", format!("{}, same as Weather", city.name))
            }
            Ok(city) => detail_state("Where you are", city.name.clone()),
            Err(reason) if reason.starts_with("Choose a city") => detail_state(
                "Where you are",
                "Sunrise and sunset there decide when your look switches",
            ),
            Err(reason) => detail_state("Where you are", reason),
        };
        if source == Source::Weather {
            controls.push(split(
                summary,
                button::text("Change in Weather")
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
                    text_input("Search for your city", &state.query)
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
        let mut items = Vec::new();
        let targets: Vec<_> = editor
            .profiles
            .iter()
            .map(|(profile, _)| profile.clone())
            .collect();
        let selected = targets
            .iter()
            .position(|profile| Some(profile) == draft.profile.as_ref());
        items.push(field(
            "What changes",
            cosmic::widget::dropdown(&editor.profile_labels, selected, move |index| {
                edit(Edit::Draft(DraftEdit::Profile(targets[index].clone())))
            })
            .placeholder("Choose a look…")
            .width(Length::Fill)
            .into(),
        ));
        if draft.profile.is_some() && selected.is_none() {
            items.push(label("The chosen look is unavailable", 12., WARN));
        }
        if let Some(profile) = &draft.profile {
            items.push(label(editor.profile_scope(profile, draft.kind), 12., MUTED));
        }
        if draft.id.is_none() {
            items.push(split(
                label("When", 12., MUTED),
                segmented(
                    [
                        (Kind::Schedule, "Fixed time"),
                        (Kind::Solar, "Sun event"),
                        (Kind::Display, "Display connection"),
                    ]
                    .into_iter()
                    .map(|(kind, title)| {
                        (
                            title.into(),
                            draft.kind == kind,
                            Some(edit(Edit::Draft(DraftEdit::Kind(kind)))),
                        )
                    })
                    .collect(),
                    accent,
                ),
            ));
        }
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
                        Some(_) => label("Local time", CAPTION, MUTED),
                        None if draft.time.trim().is_empty() => label("e.g. 19:00", CAPTION, MUTED),
                        None => label("Use a time like 19:00 or 7 pm", CAPTION, WARN),
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
                items.push(
                    column::with_children(vec![
                        split(
                            label("On", 12., MUTED),
                            row::with_children(days).spacing(4).wrap().into(),
                        ),
                        presets,
                    ])
                    .spacing(8)
                    .into(),
                );
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
        if draft.kind == Kind::Schedule {
            if let (Some(time), Some(profile)) = (draft.parsed_time(), draft.profile.clone()) {
                let now = chrono::Utc::now();
                let preview = crate::automation::Automation {
                    enabled: true,
                    since: now.timestamp(),
                    rules: vec![Rule {
                        id: "preview".into(),
                        enabled: true,
                        since: now.timestamp(),
                        profile,
                        name: String::new(),
                        when: Trigger::Schedule {
                            time,
                            days: draft.days,
                        },
                    }],
                    ..Default::default()
                };
                if let Some((at, _)) = crate::automation::decide::next_instant(
                    &preview,
                    &crate::automation::State::default(),
                    &Local,
                    now,
                    &editor.solar,
                ) {
                    items.push(label(
                        format!(
                            "Next occurrence {} · this computer’s time",
                            words::when(
                                at.with_timezone(&Local).naive_local(),
                                now.with_timezone(&Local).naive_local(),
                                self.h24()
                            )
                        ),
                        12.,
                        MUTED,
                    ));
                }
            }
        }
        let city_problem = (draft.kind == Kind::Solar)
            .then(|| editor.solar.city(draft.source).err())
            .flatten();
        let missing = draft.missing().or(city_problem.as_deref());
        let enables_automation = editor
            .settings()
            .is_ok_and(|settings| draft.enables_automation(settings));
        let save_label = if enables_automation {
            "Save and enable"
        } else {
            "Save rule"
        };
        items.push(
            row::with_children(vec![
                container(match missing {
                    Some(missing) => label(missing, CAPTION, MUTED),
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
            let mut lines = vec![line(what, 13., FG), line(cause, CAPTION, MUTED)];
            if let Some(why) = why {
                lines.push(label(why, CAPTION, if tone == ERROR { ERROR } else { WARN }));
            }
            rows.push(
                row::with_children(vec![
                    container(label(symbol, 13., tone)).width(14).into(),
                    container(column::with_children(lines).spacing(2))
                        .width(Length::Fill)
                        .into(),
                    label(words::stamp(at, now_local, self.h24()), CAPTION, MUTED),
                ])
                .spacing(10)
                .align_y(Alignment::Center)
                .into(),
            );
        }
        section(rows)
    }
}
