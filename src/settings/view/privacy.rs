use super::*;
impl Settings {
    pub(in crate::settings) fn usage_control(&self) -> Element<'_, Message> {
        // What is shared is consent text: always on screen, never behind "?".
        let mut controls = vec![split(
            detail_state(
                "Share usage data",
                "The app version, your country, which days you use it and which main features are on. Never what's in your widgets.",
            ),
            container(
                toggler(self.usage_enabled)
                    .on_toggle_maybe((!self.usage_busy).then_some(Message::ShareUsage)),
            )
            .id(cosmic::widget::Id::new("share-usage-data"))
            .into(),
        )];
        if self.usage_busy {
            controls.push(label("Saving…", 12., MUTED));
        }
        if let Some(error) = &self.usage_error {
            controls.push(label(
                format!("Your choice wasn't saved: {error}. Please try again."),
                12.,
                Color::from_rgb(1., 0.55, 0.5),
            ));
        }
        column::with_children(controls).spacing(8).into()
    }
    pub(in crate::settings) fn privacy_section(&self) -> Element<'_, Message> {
        column::with_children(vec![
            group(
                Some("Sharing"),
                vec![
                    self.usage_control(),
                    group_row(
                        "Privacy policy",
                        "",
                        button::text("Read it ↗")
                            .on_press(Message::OpenPrivacy)
                            .into(),
                    ),
                ],
            ),
            group(
                Some("Always stays on your computer"),
                vec![detail_state(
                    "Your calendar, notes, accounts and projects",
                    "Never sent anywhere, even with usage data on",
                )],
            ),
        ])
        .spacing(16)
        .into()
    }
}
