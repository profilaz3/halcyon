use super::*;
use crate::automation::solar::Source;
use crate::desktop::profiles::adaptive::{Look, Phase};

impl Settings {
    /// One look that follows the sun: pick it, see it at each time of day,
    /// then let it change on its own.
    pub(super) fn adaptive_looks_page(&self) -> Element<'_, Message> {
        let look = self.adaptive_look;
        let phase = self.adaptive_phase;
        let accent = self.settings_accent();
        let has_rules=self.automation.settings().is_ok_and(|s| s.rules.iter().any(|r| matches!(&r.profile,crate::automation::ProfileRef::Preset(id) if id.starts_with("adaptive-"))));
        // Without a Weather city, the city is chosen right here.
        let weather_city = self.automation.solar.city(Source::Weather).is_ok();
        let source = if weather_city {
            self.adaptive_source
        } else {
            Source::Manual
        };
        let city = self.automation.solar.city(source).is_ok();
        let mut items = vec![
            heading(
                "Adaptive looks",
                "Wallpaper and colors that follow the time of day. Your widgets stay where they are.",
            ),
            container(self.profile_preview(self.stage_height().clamp(260., 320.)))
                .id(cosmic::widget::Id::new("adaptive-preview"))
                .into(),
            group(
                Some("Look"),
                vec![
                    group_row(
                        "Style",
                        "",
                        segmented(
                            Look::ALL
                                .into_iter()
                                .map(|l| {
                                    (
                                        l.title().into(),
                                        look == l,
                                        Some(Message::AdaptivePreview(l, phase)),
                                    )
                                })
                                .collect(),
                            accent,
                        ),
                    ),
                    group_row(
                        "Show it at",
                        "Only the preview changes",
                        segmented(
                            Phase::ALL
                                .into_iter()
                                .map(|p| {
                                    (
                                        p.title().into(),
                                        p == phase,
                                        Some(Message::AdaptivePreview(look, p)),
                                    )
                                })
                                .collect(),
                            accent,
                        ),
                    ),
                ],
            ),
        ];
        let cycle_detail = "Day after sunrise, evening at golden hour, night after sunset";
        let can_toggle = (has_rules || city) && self.automation.writable() && !self.native_busy;
        let mut cycle = vec![group_row(
            "Change through the day",
            if has_rules || city {
                cycle_detail
            } else {
                "Needs your city, just below"
            },
            container(toggler(has_rules).on_toggle_maybe(can_toggle.then_some(
                move |on: bool| {
                    Message::Automation(if on {
                        crate::settings::automation::Message::AdaptiveRoutine(look, source)
                    } else {
                        crate::settings::automation::Message::AdaptiveOff
                    })
                },
            )))
            .id(cosmic::widget::Id::new("adaptive-enable"))
            .into(),
        )];
        if has_rules {
            cycle.push(self.link_row(
                "Adjust the times",
                "",
                Message::Page(Page::DesktopAutomation),
                "adaptive-rules".into(),
            ));
        }
        if let Some(crate::settings::desktop::Notice::Done(text)) = &self.automation.notice
            && text.starts_with("Daily cycle")
        {
            cycle.push(label(text, 12., MUTED));
        }
        if !has_rules && weather_city {
            cycle.push(group_row(
                "Sun times from",
                "",
                segmented(
                    [(Source::Weather, "Weather city"), (Source::Manual, "Another city")]
                        .into_iter()
                        .map(|(s, title)| {
                            (
                                title.into(),
                                self.adaptive_source == s,
                                Some(Message::AdaptiveSource(s)),
                            )
                        })
                        .collect(),
                    accent,
                ),
            ));
        }
        if !has_rules {
            cycle.push(self.schedule_city_controls(source));
        }
        cycle.push(group_row(
            "Use this look at a set time",
            format!("{} {}", look.title(), phase.title().to_lowercase()),
            button::standard("Schedule…")
                .id(cosmic::widget::Id::new("adaptive-schedule"))
                .on_press(Message::Automation(
                    crate::settings::automation::Message::Schedule(
                        crate::automation::ProfileRef::Preset(look.id(phase).into()),
                    ),
                ))
                .into(),
        ));
        if self
            .config
            .moments
            .selected()
            .and_then(|m| m.family)
            .is_some()
        {
            cycle.push(group_row(
                "Your Moment keeps its own widget style",
                "",
                button::text("Moments")
                    .on_press(Message::Page(Page::Moments))
                    .into(),
            ));
        }
        items.push(group(Some("Through the day"), cycle));
        items.push(self.automation_link_status("adaptive-automation"));
        column::with_children(items).spacing(16).into()
    }
}
