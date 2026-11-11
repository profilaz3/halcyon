//! First run has one path: the setup wizard. Until it is finished, Home
//! offers it in one card; a stopped widgets service says so with a way out.
use super::desktop::{WARN, banner};
use super::*;

impl Settings {
    /// Home offers setup until it is finished or put off with “Not now”.
    pub(in crate::settings) fn needs_setup(&self) -> bool {
        !self.config.onboarded && !self.config.setup_dismissed
    }

    /// The one way into first-run setup from Home.
    pub(super) fn setup_card(&self) -> Element<'_, Message> {
        frame(
            split(
                detail_state(
                    "Set up your desktop",
                    "Pick a look, your dock and panel, and widgets in four short steps. Nothing changes until you apply.",
                ),
                row::with_children(vec![
                    button::text("Not now")
                        .id(cosmic::widget::Id::new("setup-dismiss"))
                        .on_press(Message::SetupDismissed(true))
                        .into(),
                    button::suggested("Start")
                        .id(cosmic::widget::Id::new("setup-open"))
                        .on_press(Message::OpenWizard)
                        .into(),
                ])
                .spacing(6)
                .align_y(Alignment::Center)
                .into(),
            ),
            SURFACE,
            LINE,
            12,
        )
    }

    /// Whether the widgets service is known to be stopped.
    pub(in crate::settings) fn widgets_stopped(&self) -> bool {
        self.service_known && !self.service.running
    }

    /// Shown on Widgets and Looks while the widgets service is stopped.
    /// `offer_setup` adds Set up… where Home's setup card isn't already shown.
    pub(super) fn widgets_stopped_banner(&self, offer_setup: bool) -> Option<Element<'_, Message>> {
        if !self.widgets_stopped() {
            return None;
        }
        let mut actions = vec![
            button::standard("Start widgets")
                .id(cosmic::widget::Id::new("widgets-start"))
                .on_press_maybe((!self.busy).then_some(Message::Background(true)))
                .into(),
        ];
        if offer_setup {
            actions.push(
                button::text("Set up…")
                    .id(cosmic::widget::Id::new("widgets-setup"))
                    .on_press(Message::OpenWizard)
                    .into(),
            );
        }
        Some(banner(
            "Widgets aren’t running. Start them to see your desktop widgets.",
            WARN,
            actions,
        ))
    }
}
