use super::*;
use crate::moments::Phase;
use crate::settings::moments::Message as Edit;
const FAMILIES: [&str; 14] = [
    "Same as Appearance",
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
            Some("Clock changes apply to"),
            vec![group_row(
                format!("{} is showing now", active.title()),
                "Type, weight, italic and background. Size and font are always shared.",
                segmented(
                    vec![
                        (
                            format!("{} only", active.title()),
                            !self.moment_base_clock,
                            Some(edit(Edit::BaseClock(false))),
                        ),
                        (
                            "Every time of day".into(),
                            self.moment_base_clock,
                            Some(edit(Edit::BaseClock(true))),
                        ),
                    ],
                    self.settings_accent(),
                ),
            )],
        )
    }
    /// On the Time of day page in Widget style only: a preview, which style
    /// shows now, then the style each one uses.
    pub(super) fn moment_style_section(&self) -> Element<'_, Message> {
        let phase = self.moment_preview;
        let look = self.config.moments.get(phase);
        let preview = self.moment_sample();
        let accent = self.settings_accent();
        let family_index = look
            .family
            .and_then(|f| {
                crate::desktop_theme::Family::ALL
                    .iter()
                    .position(|known| *known == f)
            })
            .map_or(0, |i| i + 1);
        let narrow = self.content_width() < 620.;
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
            let width = if narrow {
                bounds.width.min(360.)
            } else {
                ((bounds.width - 24.) / 2.).min(360.)
            };
            let clock =
                crate::app::settings_clock_preview_at(&preview, [width, 132.], clock_time)
                    .map(|_| Message::Preview);
            let weather = super::sample::thumbnail(
                crate::app::settings_weather_preview_at(&preview, [360., 180.], now)
                    .map(|_| Message::Preview),
                [360., 180.],
                [width, 150.],
            );
            let cards = vec![
                container(clock).width(width).into(),
                container(weather).width(width).into(),
            ];
            container(if narrow {
                Element::from(column::with_children(cards).spacing(10).align_x(Alignment::Center))
            } else {
                row::with_children(cards)
                    .spacing(24)
                    .align_y(Alignment::Center)
                    .into()
            })
            .width(Length::Fill)
            .align_x(Alignment::Center)
            .into()
        })
        .height(if narrow { 300 } else { 160 });
        let choice = |name: &'static str,
                      key: &'static str,
                      current: serde_json::Value,
                      choices: &[(&'static str, serde_json::Value)]| {
            let mut items = vec![(
                "Same as Appearance".into(),
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
            group_row(name, "", segmented(items, accent))
        };
        let active = self.config.moments.active;
        let mut showing = vec![(
            "Off".to_string(),
            active.is_none(),
            Some(edit(Edit::Activate(None))),
        )];
        showing.extend(Phase::ALL.into_iter().map(|p| {
            (
                format!("{} style", p.title()),
                active == Some(p),
                Some(edit(Edit::Activate(Some(p)))),
            )
        }));
        column::with_children(vec![
            container(self.preview_stage(
                column::with_children(vec![
                    container(label(
                        format!("Previewing the {} style", phase.title().to_lowercase()),
                        12.,
                        Color::WHITE,
                    ))
                    .padding([3, 10])
                    .class(cosmic::theme::Container::custom(|_| {
                        cosmic::iced::widget::container::Style {
                            background: Some(Background::Color(Color::from_rgba(0., 0., 0., 0.35))),
                            border: Border {
                                radius: 10.into(),
                                ..Default::default()
                            },
                            ..Default::default()
                        }
                    }))
                    .into(),
                    sample.into(),
                ])
                .spacing(8)
                .align_x(Alignment::Center)
                .into(),
            ))
            .id(cosmic::widget::Id::new("moment-preview"))
            .into(),
            group(
                None,
                vec![group_row(
                    "Showing now",
                    "Until the next part of the day. Off shows your Appearance settings as they are.",
                    container(segmented(showing, accent))
                        .id(cosmic::widget::Id::new("moment-activate"))
                        .into(),
                )],
            ),
            column::with_children(vec![
            row::with_children(vec![
                text::heading("Style").width(Length::Fill).into(),
                segmented(
                    Phase::ALL
                        .into_iter()
                        .map(|p| {
                            (
                                p.title().to_string(),
                                phase == p,
                                Some(edit(Edit::Preview(p))),
                            )
                        })
                        .collect(),
                    accent,
                ),
            ])
            .align_y(Alignment::Center)
            .into(),
            group(
                None,
                vec![
                    group_row(
                        "Widget look",
                        "",
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
                        "Clock type",
                        "face",
                        serde_json::json!(look.clock.face),
                        &[
                            ("Digital", serde_json::json!("digital")),
                            ("Analog", serde_json::json!("analog")),
                        ],
                    ),
                    choice(
                        "Clock background",
                        "surface",
                        serde_json::json!(look.clock.surface),
                        &[
                            ("None", serde_json::json!("none")),
                            ("Card", serde_json::json!("panel")),
                        ],
                    ),
                    choice(
                        "Clock weight",
                        "weight",
                        serde_json::json!(look.clock.weight),
                        &[
                            ("Light", serde_json::json!("light")),
                            ("Regular", serde_json::json!("normal")),
                            ("Bold", serde_json::json!("bold")),
                        ],
                    ),
                    choice(
                        "Clock italic",
                        "italic",
                        serde_json::json!(look.clock.italic),
                        &[
                            ("Off", serde_json::json!(false)),
                            ("On", serde_json::json!(true)),
                        ],
                    ),
                ],
            ),
            ])
            .spacing(8)
            .into(),
        ])
        .spacing(16)
        .into()
    }
}
