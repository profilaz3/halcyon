//! Shared preview presentation. Collapse is local UI state, never a desktop edit.
use super::*;

impl Settings {
    pub(super) fn preview_header<'a>(&self, caption: &str, live: bool) -> Element<'a, Message> {
        let (key, title, explanation) = if live {
            (
                "preview-live",
                "Live controls",
                "These controls operate your actual desktop widget. Playback and timer actions take effect immediately.",
            )
        } else if self.page == Page::DesktopWallpaper {
            (
                "preview-wallpaper",
                "Wallpaper preview",
                "This shows your current or selected wallpaper. Choosing a picture previews it locally; Apply wallpaper changes your desktop.",
            )
        } else if matches!(
            self.page,
            Page::Themes | Page::DesktopProfiles | Page::AdaptiveLooks | Page::DesktopDock
        ) {
            (
                "preview-desktop",
                "Desktop preview",
                "The wallpaper and appearance reflect the current selection. Widgets use illustrative content. Highlighted objects open their settings; they do not run widget actions. Review the included changes before applying a theme or setup.",
            )
        } else {
            (
                "preview-sample",
                "Sample content",
                "Illustrative content shows how the design fits. Buttons inside a sample do not perform actions or connect accounts. Your connection and location are shown separately in settings.",
            )
        };
        row::with_children(vec![
            label(caption, 12., MUTED),
            help::explanation(key, title, explanation, self.help_open == Some(key)),
        ])
        .spacing(4)
        .align_y(Alignment::Center)
        .into()
    }
    pub(in crate::settings) fn scene_open(&self) -> bool {
        !self.collapsed_scenes.contains(&self.page)
    }

    pub(in crate::settings) fn scene_frame<'a>(
        &'a self,
        content: Element<'a, Message>,
        caption: &str,
    ) -> Element<'a, Message> {
        let open = self.scene_open();
        let live = caption.starts_with("Live controls");
        let header = split(
            container(self.preview_header(
                if caption == "Sample data" {
                    "Sample content · current style"
                } else if caption == "Live controls" {
                    "Live controls · changes your desktop widget"
                } else {
                    caption
                },
                live,
            ))
            .width(Length::Fill)
            .into(),
            button::text(match (live, open) {
                (true, true) => "Hide controls",
                (true, false) => "Show controls",
                (false, true) => "Hide preview",
                (false, false) => "Show preview",
            })
            .id(cosmic::widget::Id::new("scene-toggle"))
            .trailing_icon(
                icon::from_name(if open {
                    "go-up-symbolic"
                } else {
                    "go-down-symbolic"
                })
                .size(16),
            )
            .on_press(Message::ToggleScene)
            .into(),
        );
        let mut children = vec![header];
        if open {
            children.push(content);
        }
        container(column::with_children(children).spacing(10))
            .id(cosmic::widget::Id::new("desktop-scene"))
            .width(Length::Fill)
            .into()
    }
}
