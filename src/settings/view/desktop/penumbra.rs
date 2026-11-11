//! Penumbra is a separate optional mod; Islands owns only applet layout.
use super::*;

impl Settings {
    pub(super) fn penumbra_controls(&self, draft: &PanelDraft) -> Element<'_, Message> {
        let supported = self.desktop.penumbra_supported;
        let setting = draft.penumbra();
        let enabled = setting
            .known()
            .is_some_and(|v| v.is_some_and(|v| v.enabled));
        let effect = setting.known().copied().flatten().filter(|v| v.enabled);
        let mut controls = Vec::new();
        match setting {
            // Not available and off: one row that says why, with the way
            // forward on the right, instead of a switch that does nothing.
            Setting::Known(_) if !supported && !enabled => {
                controls.push(self.penumbra_setup(draft, true));
            }
            Setting::Known(_) => controls.push(split(
                detail("Blend into wallpaper", "The background fades into the picture behind it"),
                container(toggler(enabled).on_toggle_maybe(
                    (supported || enabled).then_some(|value| edit(Edit::Penumbra(value))),
                ))
                .id(cosmic::widget::Id::new("penumbra-gradient"))
                .into(),
            )),
            Setting::Unsupported(raw) => controls.push(field("Blend into wallpaper", kept(&raw))),
        }
        if !supported && enabled {
            controls.push(self.penumbra_setup(draft, false));
        }
        if let Some(effect) = effect {
            controls.push(
                container(value_slider(
                    "Fade length",
                    format!("{:.1}× bar thickness", effect.height as f32 / 32.),
                    16..=512,
                    effect.height,
                    8,
                    self.settings_accent(),
                    Edit::PenumbraHeight,
                ))
                .id(cosmic::widget::Id::new("penumbra-fade"))
                .into(),
            );
            let opacity = (effect.maximized_opacity * 100.).round() as u32;
            controls.push(
                container(value_slider(
                    "Solid with a maximized window",
                    format!("{opacity}%"),
                    0..=100,
                    opacity,
                    1,
                    self.settings_accent(),
                    |value| Edit::PenumbraMaximizedOpacity(value as u8),
                ))
                .id(cosmic::widget::Id::new("penumbra-maximized-opacity"))
                .into(),
            );
            if draft.keep_style().known() == Some(&true) {
                controls.push(label(
                    "Turn off Keep style when maximized, under Spacing and timing, to change this.",
                    11.,
                    MUTED,
                ));
            }
        }
        container(column::with_children(controls).spacing(10))
            .id(cosmic::widget::Id::new("penumbra-controls"))
            .into()
    }

    /// Why blending isn't available yet. The Islands panel provides it; when
    /// Islands already asks for that panel, its step under Shape is the only
    /// one, so this just points there. As `titled`, it is the feature's row.
    fn penumbra_setup(&self, draft: &PanelDraft, titled: bool) -> Element<'_, Message> {
        let islands = draft.islands().known().is_some_and(|s| s.enabled);
        const TITLE: &str = "Blend into wallpaper";
        const ABOUT: &str = "The background fades into the picture behind it";
        let note = |text: &'static str| -> Element<'_, Message> {
            if titled {
                detail_state(TITLE, text)
            } else {
                label(text, 12., MUTED)
            }
        };
        if self.panel_host() == super::host::Host::Running {
            return note(
                "The Islands panel needs an update for this. Update Halcyon, then sign out and back in.",
            );
        }
        if islands {
            return note("Comes with the Islands panel. See Shape above.");
        }
        let Some(step) = self.host_step("This uses") else {
            return if titled {
                detail(TITLE, ABOUT)
            } else {
                label(ABOUT, 12., MUTED)
            };
        };
        if titled {
            column::with_children(vec![detail(TITLE, ABOUT), step])
                .spacing(8)
                .into()
        } else {
            step
        }
    }
}
