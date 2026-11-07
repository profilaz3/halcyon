//! Accessibility: the supported app-motion preference and its limits.
//! Halcyon edge-menu animation belongs in App settings.
use super::desktop::WARN;
use super::*;
use crate::settings::motion::Message as Edit;

impl Settings {
    pub(super) fn motion_page(&self) -> Element<'_, Message> {
        let editor = &self.motion;
        let unavailable = matches!(editor.current(), Some(Err(_)));
        let apps: Element<'_, Message> = match (editor.current(), editor.shown()) {
            (Some(Err(_)), _) => label("Not available", 13., MUTED),
            (_, Some(on)) => toggler(!on)
                .on_toggle_maybe(
                    editor
                        .can_change()
                        .then_some(|reduce: bool| Message::Motion(Edit::Animations(!reduce))),
                )
                .into(),
            _ => label("Reading…", 13., MUTED),
        };
        // Why it can't be used stays in view; what it does sits behind "?".
        let mut motion = vec![if unavailable {
            group_row_state("Reduce motion", "Your system doesn't offer this yet", apps)
        } else {
            group_row(
                "Reduce motion",
                "Less animation in Halcyon, most apps and web browsers",
                apps,
            )
        }];
        if editor.read_only().is_some() {
            motion.push(label("Changes are off in this preview", 12., WARN));
        }
        motion.push(self.link_row(
            "Dock and panel slide speed",
            "How fast they slide in when hidden",
            Message::Page(Page::DesktopDock),
            "accessibility-dock".into(),
        ));
        column::with_children(vec![
            heading("Accessibility", "Calmer motion and easier reading."),
            group(Some("Motion"), motion),
            group(
                Some("Reading"),
                vec![
                    self.link_row(
                        "Easier-to-read widgets",
                        "Stronger text contrast on widget cards",
                        Message::SearchResult(crate::settings::navigation::WIDGET_READABILITY),
                        "accessibility-readability".into(),
                    ),
                    self.link_row(
                        "Fonts and icons",
                        "Text and icons across your desktop",
                        Message::Page(Page::DesktopAppearance),
                        "accessibility-fonts".into(),
                    ),
                ],
            ),
            group(
                None,
                vec![
                    button::custom(
                        row::with_children(vec![
                            detail(
                                "More in COSMIC Settings",
                                "Screen reader, zoom, high contrast and more",
                            ),
                            label("↗", 16., MUTED),
                        ])
                        .spacing(12)
                        .align_y(Alignment::Center),
                    )
                    .class(nav_style(false, self.settings_accent()))
                    .padding([4, 0])
                    .width(Length::Fill)
                    .on_press(Message::OpenDesktop("accessibility"))
                    .into(),
                ],
            ),
        ])
        .spacing(16)
        .into()
    }
}
