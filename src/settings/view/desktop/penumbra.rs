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
                controls.push(self.penumbra_setup(true));
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
            controls.push(self.penumbra_setup(false));
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

    /// Why blending isn't available yet, and what to do. As `row`, it is
    /// the feature's own row: its title, the reason under it, the action
    /// on the right.
    fn penumbra_setup(&self, row: bool) -> Element<'_, Message> {
        use crate::desktop::islands::{CompatibilityStatus, Installation, Provenance};
        use crate::settings::updates::{
            Message as Update,
            islands::{self, Message as Panel, Phase},
        };

        let installation = &self.desktop.islands.installation;
        let state = &self.updates.islands;
        let verified = self
            .updates
            .identity
            .as_ref()
            .is_some_and(|id| id.can_enable_islands());
        let enable = islands::can_enable(installation) && verified;
        let waiting = matches!(
            installation,
            Installation::Installed {
                enabled: true,
                status: Some(CompatibilityStatus::LogoutRequired),
                ..
            }
        );
        let checking = self.installation_busy
            || matches!(
                installation,
                Installation::Pending
                    | Installation::Installed {
                        status: Some(CompatibilityStatus::Checking),
                        ..
                    }
            );
        let description = if state.busy() {
            "Preparing the included panel for your next login…"
        } else if checking {
            "Checking panel support…"
        } else if waiting {
            "Ready after you sign out and back in. Then turn this on."
        } else if enable {
            "Needs Halcyon’s own panel, which starts the next time you sign in."
        } else if matches!(
            installation,
            Installation::Installed {
                provenance: Provenance::Manual,
                ..
            }
        ) {
            "Not available with a manually installed panel. Install the Halcyon package to use it."
        } else if matches!(installation, Installation::Absent) {
            "Needs the Halcyon panel add-on."
        } else if self.desktop.islands.compatible {
            "Your panel doesn’t support this yet. Update Halcyon, then sign in again."
        } else {
            "Halcyon’s panel isn’t running right now. Check it under Updates."
        };
        let (title, action) = if enable {
            (
                "Turn on at next sign-in",
                Message::Updates(Update::Islands(Panel::Enable)),
            )
        } else if waiting {
            (
                "Check again",
                Message::Updates(Update::Islands(Panel::Recheck)),
            )
        } else {
            (
                if matches!(installation, Installation::Absent) {
                    "Get the add-on"
                } else {
                    "Open Updates"
                },
                Message::AppSettingsPage(AppSettingsPage::Updates),
            )
        };
        let failed = match &state.phase {
            Phase::Failed(error) => Some(label(error.label(), 12., ERROR)),
            _ => None,
        };
        let mut actions = vec![
            button::standard(title)
                .id("penumbra-setup".into())
                .on_press_maybe((!state.busy() && !checking).then_some(action))
                .into(),
        ];
        // A missing add-on has one way forward; checking again changes nothing.
        if !waiting && !matches!(installation, Installation::Absent) {
            actions.push(
                button::text("Check again")
                    .id("penumbra-recheck".into())
                    .on_press_maybe(
                        (!state.busy() && !checking)
                            .then_some(Message::Updates(Update::Islands(Panel::Recheck))),
                    )
                    .into(),
            );
        }
        let actions: Element<'_, Message> = row::with_children(actions).spacing(8).wrap().into();
        if row {
            let mut parts = vec![split(detail("Blend into wallpaper", description), actions)];
            parts.extend(failed);
            return column::with_children(parts).spacing(8).into();
        }
        let mut rows = vec![label(description, 12., MUTED)];
        rows.extend(failed);
        rows.push(actions);
        column::with_children(rows).spacing(8).into()
    }
}
