use super::*;
use crate::moments::Phase;
use crate::settings::moments::Message as Edit;
const FAMILIES: [&str; 14] = [
    "Follow setup",
    "Soft",
    "Outline",
    "Paper",
    "Bare",
    "Glass",
    "Frosted",
    "Midnight",
    "Terminal",
    "Neon",
    "Editorial",
    "Retro",
    "Pastel",
    "Bauhaus",
];
fn edit(message: Edit) -> Message {
    Message::Moment(message)
}
impl Settings {
    pub(super) fn moment_clock_target_view(&self) -> Element<'_, Message> {
        let Some(active) = self.config.moments.active else {
            return cosmic::widget::space().height(0).into();
        };
        group(
            Some("Edit clock style"),
            vec![
                segmented(
                    vec![
                        (
                            active.title().into(),
                            !self.moment_base_clock,
                            Some(edit(Edit::BaseClock(false))),
                        ),
                        (
                            "All moments".into(),
                            self.moment_base_clock,
                            Some(edit(Edit::BaseClock(true))),
                        ),
                    ],
                    self.settings_accent(),
                ),
                label(
                    "Face, weight, italic and surface follow this choice. Size, fonts and time stay shared.",
                    12.,
                    MUTED,
                ),
                if self.moment_base_clock {
                    label(
                        "A moment's own clock choices can override the base style.",
                        12.,
                        MUTED,
                    )
                } else {
                    label(
                        "Use base widget style removes this moment’s override.",
                        12.,
                        MUTED,
                    )
                },
            ],
        )
    }
    pub(super) fn moments_page(&self) -> Element<'_, Message> {
        let phase = self.moment_preview;
        let look = self.config.moments.get(phase);
        let preview = self.moment_sample();
        let title = self
            .config
            .moments
            .active
            .map_or("Base style", Phase::title);
        let family_index = look
            .family
            .and_then(|f| {
                crate::desktop_theme::Family::ALL
                    .iter()
                    .position(|known| *known == f)
            })
            .map_or(0, |i| i + 1);
        let wide = self.stage_width() < 540.;
        let sample = cosmic::widget::responsive(move |bounds| {
            use chrono::TimeZone;
            let now = chrono::Local.from_utc_datetime(
                &chrono::NaiveDate::from_ymd_opt(2026, 7, 21)
                    .unwrap()
                    .and_hms_opt(7, 41, 0)
                    .unwrap(),
            );
            let clock_time = chrono::NaiveDate::from_ymd_opt(2026, 7, 21)
                .unwrap()
                .and_hms_opt(9, 41, 0)
                .unwrap();
            let width = if wide {
                bounds.width
            } else {
                (bounds.width - 12.) / 2.
            };
            let height = if wide { 164. } else { 132. };
            let clock =
                crate::app::settings_clock_preview_at(&preview, [width, height], clock_time)
                    .map(|_| Message::Preview);
            let weather = super::sample::thumbnail(
                crate::app::settings_weather_preview_at(&preview, [360., 180.], now)
                    .map(|_| Message::Preview),
                [360., 180.],
                [width, 150.],
            );
            if wide {
                column::with_children(vec![clock, weather])
                    .spacing(10)
                    .align_x(Alignment::Center)
                    .into()
            } else {
                row::with_children(vec![
                    container(clock).width(width).into(),
                    container(weather).width(width).into(),
                ])
                .spacing(12)
                .align_y(Alignment::Center)
                .into()
            }
        })
        .height(if wide { 324 } else { 150 });
        let choice = |key: &'static str,
                      name: &'static str,
                      current: serde_json::Value,
                      choices: &[(&'static str, serde_json::Value)]| {
            let mut items = vec![(
                "Inherit".into(),
                current.is_null(),
                Some(edit(Edit::Style(phase, key, serde_json::Value::Null))),
            )];
            items.extend(choices.iter().map(|(label, value)| {
                (
                    label.to_string(),
                    current == *value,
                    Some(edit(Edit::Style(phase, key, value.clone()))),
                )
            }));
            column::with_children(vec![
                label(name, 14., FG),
                segmented(items, self.settings_accent()),
            ])
            .spacing(6)
            .into()
        };
        let controls = group(
            Some("Widget style"),
            vec![
                split(
                    label("Look", 14., FG),
                    cosmic::widget::dropdown(&FAMILIES[..], Some(family_index), move |i| {
                        edit(Edit::Family(
                            phase,
                            i.checked_sub(1)
                                .and_then(|n| crate::desktop_theme::Family::ALL.get(n).copied()),
                        ))
                    })
                    .into(),
                ),
                choice(
                    "face",
                    "Clock face",
                    serde_json::json!(look.clock.face),
                    &[
                        ("Digital", serde_json::json!("digital")),
                        ("Analog", serde_json::json!("analog")),
                    ],
                ),
                choice(
                    "surface",
                    "Clock surface",
                    serde_json::json!(look.clock.surface),
                    &[
                        ("None", serde_json::json!("none")),
                        ("Panel", serde_json::json!("panel")),
                    ],
                ),
                choice(
                    "weight",
                    "Clock weight",
                    serde_json::json!(look.clock.weight),
                    &[
                        ("Light", serde_json::json!("light")),
                        ("Normal", serde_json::json!("normal")),
                        ("Bold", serde_json::json!("bold")),
                    ],
                ),
                choice(
                    "italic",
                    "Clock italic",
                    serde_json::json!(look.clock.italic),
                    &[
                        ("Off", serde_json::json!(false)),
                        ("On", serde_json::json!(true)),
                    ],
                ),
                label(
                    format!(
                        "Saved to {}. Inherit uses your base widget choice.",
                        phase.title()
                    ),
                    12.,
                    MUTED,
                ),
            ],
        );
        column::with_children(vec![
            heading("Moments", ""),
            label(format!("Active · {title}"), 14., FG),
            segmented(
                Phase::ALL
                    .into_iter()
                    .map(|p| (p.title().into(), phase == p, Some(edit(Edit::Preview(p)))))
                    .collect(),
                self.settings_accent(),
            ),
            row::with_children(vec![
                button::suggested(format!("Use {}", phase.title()))
                    .id(cosmic::widget::Id::new("moment-activate"))
                    .on_press(edit(Edit::Activate(Some(phase))))
                    .into(),
                button::text(format!("Schedule {}…", phase.title()))
                    .id(cosmic::widget::Id::new("moment-schedule"))
                    .on_press(Message::Automation(
                        crate::settings::automation::Message::Schedule(
                            crate::automation::ProfileRef::Moment(phase),
                        ),
                    ))
                    .into(),
                button::text("Use base style")
                    .id(cosmic::widget::Id::new("moment-base"))
                    .on_press(edit(Edit::Activate(None)))
                    .into(),
            ])
            .spacing(8)
            .wrap()
            .into(),
            self.editor_with_preview(sample.into(), controls, "Preview · sample content"),
            label(
                "Widget style only · wallpaper, layout and content stay.",
                12.,
                MUTED,
            ),
            self.automation_link_status("moments-schedules"),
        ])
        .spacing(16)
        .into()
    }
}
