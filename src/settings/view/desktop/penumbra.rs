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
            Setting::Known(_) => controls.push(split(
                detail("Penumbra", "Fade the bar background into your wallpaper"),
                container(toggler(enabled).on_toggle_maybe(
                    (supported || enabled).then_some(|value| edit(Edit::Penumbra(value))),
                ))
                .id(cosmic::widget::Id::new("penumbra-gradient"))
                .into(),
            )),
            Setting::Unsupported(raw) => controls.push(field("Penumbra", kept(&raw))),
        }
        controls.push(label(
            "Uses Background opacity. Floating and Penumbra turn each other off.",
            11.,
            MUTED,
        ));
        if !supported {
            controls.push(label("Requires a panel component with Penumbra support. Update the component and log out and back in.", 11., MUTED));
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
                    "Maximized opacity",
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
                    "Turn off Keep style on maximize to use Maximized opacity.",
                    11.,
                    MUTED,
                ));
            }
        }
        container(column::with_children(controls).spacing(10))
            .id(cosmic::widget::Id::new("penumbra-controls"))
            .into()
    }
}
