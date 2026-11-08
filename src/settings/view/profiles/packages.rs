use super::*;
use crate::settings::profiles::packages::{Draft, Edit as PackageEdit};
fn message(edit: PackageEdit) -> Message {
    super::edit(Edit::Package(edit))
}
impl Settings {
    pub(super) fn package_page<'a>(&'a self, draft: &'a Draft) -> Element<'a, Message> {
        let available = !self.native_busy && !self.profiles.busy();
        let mut items = vec![
            button::text("Back to look")
                .on_press_maybe(available.then_some(message(PackageEdit::Close)))
                .into(),
            column::with_children(vec![
                label("Export look package", 24., FG),
                label(&draft.profile.name, 14., MUTED),
            ])
            .spacing(4)
            .into(),
            label("Recipients review the look before applying.", 14., MUTED),
        ];
        if draft.has_picture() {
            items.push(
                container(
                    cosmic::widget::checkbox(draft.picture)
                        .label("Include this wallpaper picture")
                        .on_toggle(|on| message(PackageEdit::Picture(on))),
                )
                .id(cosmic::widget::Id::new("package-picture"))
                .into(),
            );
            items.push(label(
                "Include only pictures you may share. Image metadata is removed.",
                12.,
                MUTED,
            ));
        } else {
            items.push(label("Built-in wallpapers and colors travel with the look. Personal slideshows are left out.",12.,MUTED));
        }
        items.push(section(vec![
            label("Credits", 16., FG),
            text_input("Author", &draft.credits.author)
                .id(cosmic::widget::Id::new("package-author"))
                .on_input(|s| message(PackageEdit::Author(s)))
                .into(),
            text_input("License or permission", &draft.credits.license)
                .id(cosmic::widget::Id::new("package-license"))
                .on_input(|s| message(PackageEdit::License(s)))
                .into(),
            text_input("Source link (optional, HTTPS)", &draft.credits.source)
                .id(cosmic::widget::Id::new("package-source"))
                .on_input(|s| message(PackageEdit::Source(s)))
                .into(),
        ]));
        items.push(label(
            "Accounts, notes, locations and physical display assignments stay on this computer.",
            12.,
            MUTED,
        ));
        self.inspector_layout(
            column::with_children(vec![
                self.profile_preview(self.stage_height()),
                label(&draft.profile.name, 18., FG),
                label(
                    "Recipients review the included changes before applying.",
                    13.,
                    MUTED,
                ),
            ])
            .spacing(12)
            .into(),
            column::with_children(items).spacing(16).into(),
            "package",
        )
    }
}

impl Settings {
    pub(super) fn package_actions(&self) -> Element<'_, Message> {
        let available = !self.native_busy && !self.profiles.busy();
        container(
            row::with_children(vec![
                button::suggested("Save package…")
                    .id(cosmic::widget::Id::new("package-export"))
                    .on_press_maybe(available.then_some(message(PackageEdit::Export)))
                    .into(),
                button::text("Cancel")
                    .on_press_maybe(available.then_some(message(PackageEdit::Close)))
                    .into(),
            ])
            .spacing(8),
        )
        .padding([8, 20])
        .width(Length::Fill)
        .into()
    }
}
