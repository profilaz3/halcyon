//! Shared preview presentation. A preview is always shown, without a
//! caption or collapse control: it is the page's picture, not a section.
use super::*;

impl Settings {
    pub(in crate::settings) fn scene_open(&self) -> bool {
        true
    }

    pub(in crate::settings) fn scene_frame<'a>(
        &'a self,
        content: Element<'a, Message>,
        _caption: &str,
    ) -> Element<'a, Message> {
        container(content)
            .id(cosmic::widget::Id::new("desktop-scene"))
            .width(Length::Fill)
            .into()
    }
}
