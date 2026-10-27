use super::*;
use crate::automation::solar::Source;
use crate::desktop::profiles::adaptive::{Look, Phase};

impl Settings {
    pub(super) fn adaptive_looks_page(&self) -> Element<'_, Message> {
        let look = self.adaptive_look;
        let phase = self.adaptive_phase;
        let accent = self.settings_accent();
        let compact = self.content_width() < 800.;
        let preview = column::with_children(vec![
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
            self.profile_preview(if compact { 170. } else { 350. }),
            label("Local preview · sample content", 11., MUTED),
        ])
        .spacing(8)
        .into();
        let has_rules=self.automation.settings().is_ok_and(|s| s.rules.iter().any(|r| matches!(&r.profile,crate::automation::ProfileRef::Preset(id) if id.starts_with("adaptive-"))));
        let available = self.automation.solar.city(self.adaptive_source).is_ok()
            && self.automation.writable()
            && !self.native_busy
            && !has_rules;
        let mut controls=vec![
            split(label("Daily cycle",16.,FG),self.help("adaptive-cycle","", "Adds three ordinary rules: Day at sunrise, Evening at golden hour, Night at sunset. Each switch applies once and respects manual edits and pauses. Missing sun events are skipped. Edit times, priority or disable individual rules in Schedules & display rules.")),
            label("Sunrise → Day · Golden hour → Evening · Sunset → Night",12.,MUTED),
            segmented([(Source::Weather,"Weather city"),(Source::Manual,"Schedule city")].into_iter().map(|(s,title)|(title.into(),self.adaptive_source==s,Some(Message::AdaptiveSource(s)))).collect(),accent),
            self.schedule_city_controls(self.adaptive_source),
        ];
        if let Some(notice) = &self.automation.notice {
            if let crate::settings::desktop::Notice::Done(text) = notice
                && text.starts_with("Daily cycle")
            {
                controls.push(label(text, 12., MUTED));
            }
        }
        controls.push(if has_rules {
            button::standard("Edit daily cycle…")
                .id(cosmic::widget::Id::new("adaptive-rules"))
                .on_press(Message::Page(Page::DesktopAutomation))
                .into()
        } else {
            button::suggested("Enable daily cycle")
                .id(cosmic::widget::Id::new("adaptive-enable"))
                .on_press_maybe(available.then_some(Message::Automation(
                    crate::settings::automation::Message::AdaptiveRoutine(
                        look,
                        self.adaptive_source,
                    ),
                )))
                .into()
        });
        if self
            .config
            .moments
            .selected()
            .and_then(|m| m.family)
            .is_some()
        {
            controls.push(split(
                label("Your active Moment overrides widget style.", 12., MUTED),
                button::text("Moments…")
                    .on_press(Message::Page(Page::Moments))
                    .into(),
            ));
        }
        controls.push(self.help("adaptive-scope", "What changes", "The wallpaper, colors and surface style change together. Widget positions, sizes, content, applets and Islands enablement stay as they are. COSMIC still controls light/dark mode. Existing Moment and per-widget overrides remain active."));
        column::with_children(vec![
            split(
                heading("Adaptive looks", ""),
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
            super::adaptive::columns(
                preview,
                group(
                    None,
                    vec![column::with_children(controls).spacing(12).into()],
                ),
                [270., 280.],
                true,
            ),
        ])
        .spacing(16)
        .into()
    }
}
