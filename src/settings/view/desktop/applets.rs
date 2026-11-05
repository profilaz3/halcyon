//! Applet regions: drag-and-drop rows, arrow controls and the picker.
use super::*;
use crate::settings::desktop::{AppletControl, control_id};

/// The picker and the installed list offer search from this many applets.
const SEARCH_FROM: usize = 8;

impl Settings {
    pub(super) fn activity_preferences(&self) -> Element<'_, Message> {
        let p = self.config.activity_island;
        let toggle = |name: &'static str, key: &'static str, on: bool, nested: bool| {
            let row = split(
                label(name, if nested { 13. } else { 14. }, FG),
                container(
                    toggler(on)
                        .width(Length::Shrink)
                        .on_toggle(move |on| Message::ActivityPreference(key, on)),
                )
                .id(cosmic::widget::Id::new(format!(
                    "activity-preference-{key}"
                )))
                .into(),
            );
            if nested {
                container(row).padding([0, 0, 0, 20]).into()
            } else {
                row
            }
        };
        // Still in beta: said once, beside its heading.
        let heading = row::with_children(vec![
            label("Show in the island", 16., FG),
            pill("Beta", MUTED),
        ])
        .spacing(8)
        .align_y(Alignment::Center);
        let mut controls = cosmic::widget::column::with_children([heading.into()]).spacing(12);
        controls = controls
            .push(toggle("Playing media", "media", p.media, false))
            .push(toggle("Upcoming meetings", "calendar", p.calendar, false))
            .push(toggle("Notifications", "notifications", p.notifications, false));
        if p.notifications {
            controls = controls
                .push(toggle(
                    "Use the island instead of popups",
                    "replace_popups",
                    p.replace_popups,
                    true,
                ))
                .push(toggle(
                    "Show message text",
                    "notification_text",
                    p.notification_text,
                    true,
                ));
        }
        controls = controls.push(toggle("System activities", "system", p.system, false));
        if p.system {
            for (name, key, on) in [
                ("Battery", "battery", p.battery),
                ("Sound output", "sound", p.sound),
                ("Microphone in use", "microphone", p.microphone),
                ("Camera in use", "camera", p.camera),
                ("Network", "network", p.network),
                ("Bluetooth devices", "bluetooth", p.bluetooth),
                ("Software updates", "updates", p.updates),
            ] {
                controls = controls.push(toggle(name, key, on, true));
            }
        }
        controls = controls.push(toggle("Brief automatic reveals", "reveals", p.reveals, false));
        controls = controls.push(split(
            label("Meeting reminder", 14., FG),
            cosmic::widget::dropdown(
                &["5 minutes", "10 minutes", "15 minutes"],
                Some(match p.lead_minutes {
                    10 => 1,
                    15 => 2,
                    _ => 0,
                }),
                Message::ActivityLead,
            )
            .into(),
        ));
        container(controls).id("activity-preferences").into()
    }
    /// A top panel without the Activity island offers to add it, centred.
    pub(super) fn add_activity_island(
        &self,
        draft: &PanelDraft,
        regions: &crate::desktop::panels::Regions,
    ) -> Option<Element<'_, Message>> {
        let editor = &self.desktop;
        (draft.kind() == PanelKind::Panel
            && draft.anchor().known() == Some(&crate::desktop::panels::Anchor::Top)
            && editor.catalog.known(crate::activity_island::APPLET_ID)
            && !Region::ALL.iter().any(|r| {
                regions
                    .list(*r)
                    .iter()
                    .any(|id| id == crate::activity_island::APPLET_ID)
            }))
        .then(|| {
            split(
                label("Beta · System, music and meetings", 13., MUTED),
                button::text("Add Activity island")
                    .id("panel-add-activity".into())
                    .on_press_maybe((!self.native_busy && !editor.drag.active()).then_some(edit(
                        Edit::Add(Region::Center, crate::activity_island::APPLET_ID.into()),
                    )))
                    .into(),
            )
        })
    }
        pub(super) fn applets_section(&self, draft: &PanelDraft) -> Element<'_, Message> {
        let accent = self.settings_accent();
        let editor = &self.desktop;
        let horizontal = draft
            .anchor()
            .known()
            .is_none_or(|anchor| anchor.horizontal());
        let regions = match draft.applets() {
            Setting::Known(regions) => regions,
            Setting::Unsupported(raw) => {
                return section(vec![
                    label("Items", 14., FG),
                    banner(
                        "This layout uses a format Halcyon can’t edit. It is kept as it is.",
                        WARN,
                        Vec::new(),
                    ),
                    kept(&raw),
                ]);
            }
        };
        // The heading sits above the card, like every other section.
        let header: Element<'_, Message> = split(
            text::heading("Items").into(),
            label(
                if editor.drag.active() {
                    "Drop on a row or region · Esc cancels"
                } else {
                    ""
                },
                11.,
                MUTED,
            ),
        );
        let mut blocks: Vec<Element<'_, Message>> = Vec::new();
        if let Some(add) = self.add_activity_island(draft, &regions) {
            blocks.push(add);
        }
        // Drop targets never appear or disappear when a drag starts, so the
        // layout cannot shift under the pointer: a region's header inserts at
        // its start, a row takes that row's place, an empty region's zone
        // receives the applet.
        let dragging = editor.drag.active();
        for region in Region::ALL {
            let list = regions.list(region);
            let picking = editor.picker == Some(region);
            let start = Slot { region, index: 0 };
            let island_selected = editor.islands.installation.installed()
                && draft.islands().known().is_some_and(|s| s.enabled)
                && editor.islands.selected
                    == Some(match region {
                        Region::Start => dock_islands_core::Section::Start,
                        Region::Center => dock_islands_core::Section::Center,
                        Region::End => dock_islands_core::Section::End,
                    });
            let header_target = island_selected
                || (dragging && editor.drag.target == Some(start) && !list.is_empty());
            let header = container(split(
                label(
                    match (region, horizontal) {
                        (Region::Center, true) => "Middle",
                        _ => region.title(horizontal),
                    },
                    13.,
                    if header_target { accent } else { FG },
                ),
                button::text(if picking { "Close" } else { "Add" })
                    .id(cosmic::widget::Id::new(format!(
                        "applets-add-{}",
                        region.key()
                    )))
                    .leading_icon(
                        icon::from_name(if picking {
                            "window-close-symbolic"
                        } else {
                            "list-add-symbolic"
                        })
                        .size(14),
                    )
                    .on_press_maybe(
                        (!dragging && !self.native_busy)
                            .then(|| edit(Edit::Picker((!picking).then_some(region)))),
                    )
                    .into(),
            ))
            .padding([0, 6])
            .class(cosmic::theme::Container::custom(move |_| {
                cosmic::iced::widget::container::Style {
                    border: Border {
                        color: if header_target {
                            accent
                        } else {
                            Color::TRANSPARENT
                        },
                        width: 1.,
                        radius: 10.into(),
                    },
                    ..Default::default()
                }
            }));
            blocks.push(
                mouse_area(header)
                    .on_release(edit(Edit::DropAt(start)))
                    .on_enter(edit(Edit::DragOver(start)))
                    .on_move(move |_| edit(Edit::DragOver(start)))
                    .on_exit(edit(Edit::DragLeave(start)))
                    .into(),
            );
            if picking {
                blocks.push(self.applet_picker(draft, region));
            }
            let mut rows: Vec<Element<'_, Message>> = Vec::new();
            for (index, id) in list.iter().enumerate() {
                rows.push(self.applet_row(id, Slot { region, index }, list.len(), accent));
            }
            if list.is_empty() {
                let target = dragging && editor.drag.target == Some(start);
                let zone = container(label(
                    if dragging {
                        "Drop here"
                    } else {
                        "Empty. Drag an item here or use Add"
                    },
                    11.,
                    if target { accent } else { MUTED },
                ))
                .padding([8, 12])
                .width(Length::Fill)
                .class(cosmic::theme::Container::custom(move |_| {
                    cosmic::iced::widget::container::Style {
                        background: Some(Background::Color(Color {
                            a: if target { 0.12 } else { 0.03 },
                            ..accent
                        })),
                        border: Border {
                            color: if target { accent } else { LINE },
                            width: 1.,
                            radius: 10.into(),
                        },
                        ..Default::default()
                    }
                }));
                rows.push(
                    mouse_area(zone)
                        .on_release(edit(Edit::DropAt(start)))
                        .on_enter(edit(Edit::DragOver(start)))
                        .on_move(move |_| edit(Edit::DragOver(start)))
                        .on_exit(edit(Edit::DragLeave(start)))
                        .into(),
                );
            }
            blocks.push(column::with_children(rows).spacing(2).into());
        }
        column::with_children(vec![header, section(blocks)])
            .spacing(8)
            .into()
    }

    fn applet_row(&self, id: &str, slot: Slot, len: usize, accent: Color) -> Element<'_, Message> {
        let editor = &self.desktop;
        let known = editor.catalog.known(id);
        let dragging = editor.drag.source == Some(slot);
        let target = editor.drag.active() && editor.drag.target == Some(slot) && !dragging;
        let focused = editor.focus == Some(slot);
        let first = slot.region == Region::Start && slot.index == 0;
        let last = slot.region == Region::End && slot.index + 1 == len;
        let idle = !editor.drag.active() && !self.native_busy;
        let handle = mouse_area(
            container(icon::from_name("grip-lines-symbolic").size(16).icon())
                .padding(6)
                .id(control_id(slot, AppletControl::Drag)),
        )
        .on_release(edit(Edit::DropAt(slot)))
        .interaction(if dragging {
            cosmic::iced::mouse::Interaction::Grabbing
        } else {
            cosmic::iced::mouse::Interaction::Grab
        });
        let handle = if idle {
            handle.on_press(edit(Edit::DragStart(slot)))
        } else {
            handle
        };
        let mut parts: Vec<Element<'_, Message>> = vec![
            handle.into(),
            icon::from_name(editor.catalog.icon(id))
                .size(18)
                .icon()
                .into(),
        ];
        // The label and its optional status share one child, so the named
        // controls below keep fixed positions in this row. libcosmic matches
        // their state by id but keeps it at the old index; status appearing in
        // front of them during a drag would otherwise break the layout.
        let mut title = vec![
            container(line(
                editor.catalog.label(id),
                14.,
                if known { FG } else { MUTED },
            ))
            .width(Length::Fill)
            .into(),
        ];
        if target || dragging || !known {
            title.push(label(
                if target {
                    "Drop here"
                } else if dragging {
                    "Esc cancels"
                } else {
                    "Not installed"
                },
                10.,
                if target { accent } else { MUTED },
            ));
        }
        parts.push(
            cosmic::widget::tooltip(
                column::with_children(title).spacing(2).width(Length::Fill),
                text::body(editor.catalog.label(id)),
                cosmic::widget::tooltip::Position::Bottom,
            )
            .into(),
        );
        // Stable per-occurrence ids let focus follow a keyboard move.
        parts.push(
            button::icon(icon::from_name("go-up-symbolic"))
                .padding(6)
                .tooltip("Move up")
                .id(control_id(slot, AppletControl::Up))
                .on_press_maybe((!first && idle).then_some(edit(Edit::Step(slot, -1))))
                .into(),
        );
        parts.push(
            button::icon(icon::from_name("go-down-symbolic"))
                .padding(6)
                .tooltip("Move down")
                .id(control_id(slot, AppletControl::Down))
                .on_press_maybe((!last && idle).then_some(edit(Edit::Step(slot, 1))))
                .into(),
        );
        parts.push(
            button::icon(icon::from_name("window-close-symbolic"))
                .padding(6)
                .tooltip("Remove")
                .id(control_id(slot, AppletControl::Remove))
                .on_press_maybe(idle.then_some(edit(Edit::Remove(slot))))
                .into(),
        );
        let content = container(
            row::with_children(parts)
                .spacing(4)
                .align_y(Alignment::Center),
        )
        .padding([2, 6])
        .height(42)
        .align_y(Alignment::Center)
        .width(Length::Fill)
        .class(cosmic::theme::Container::custom(move |_| {
            cosmic::iced::widget::container::Style {
                background: Some(Background::Color(Color {
                    a: if target {
                        0.12
                    } else if dragging || focused {
                        0.07
                    } else {
                        0.
                    },
                    ..accent
                })),
                border: Border {
                    color: if target {
                        accent
                    } else if focused {
                        Color { a: 0.4, ..accent }
                    } else {
                        Color::TRANSPARENT
                    },
                    width: 1.,
                    radius: 12.into(),
                },
                ..Default::default()
            }
        }));
        mouse_area(content)
            .on_release(edit(Edit::DropAt(slot)))
            .on_enter(edit(Edit::DragOver(slot)))
            .on_move(move |_| edit(Edit::DragOver(slot)))
            .on_exit(edit(Edit::DragLeave(slot)))
            .into()
    }

    fn applet_search(&self) -> Element<'_, Message> {
        cosmic::widget::search_input("Search", &self.desktop.applet_query)
            .on_input(|text| edit(Edit::Search(text)))
            .on_clear(edit(Edit::Search(String::new())))
            .width(Length::Fill)
            .into()
    }

    /// One installed applet: icon, name, description and where it is used.
    fn applet_entry<'a>(
        &'a self,
        info: &'a crate::desktop::applets::AppletInfo,
        action: Element<'a, Message>,
    ) -> Element<'a, Message> {
        let accent = self.settings_accent();
        let mut title: Vec<Element<'_, Message>> = vec![
            container(line(info.name.clone(), 14., FG))
                .width(Length::Shrink)
                .into(),
        ];
        for (kind, region, horizontal) in self.desktop.placements(&info.id) {
            let here = kind == self.desktop.selected;
            title.push(pill(
                format!("{} · {}", kind.title(), region.title(horizontal)),
                if here { accent } else { MUTED },
            ));
        }
        let mut text_parts = vec![
            row::with_children(title)
                .spacing(6)
                .align_y(Alignment::Center)
                .wrap()
                .into(),
        ];
        if !info.comment.is_empty() {
            text_parts.push(line(info.comment.clone(), 11., MUTED));
        }
        row::with_children(vec![
            icon::from_name(self.desktop.catalog.icon(&info.id))
                .size(20)
                .icon()
                .into(),
            column::with_children(text_parts)
                .spacing(2)
                .width(Length::Fill)
                .into(),
            action,
        ])
        .spacing(12)
        .align_y(Alignment::Center)
        .into()
    }

    fn applet_picker(&self, draft: &PanelDraft, region: Region) -> Element<'_, Message> {
        let horizontal = draft
            .anchor()
            .known()
            .is_none_or(|anchor| anchor.horizontal());
        let placed = draft.applets().known().cloned().unwrap_or_default();
        let unplaced: Vec<_> = self
            .desktop
            .catalog
            .sorted()
            .into_iter()
            .filter(|info| !placed.contains(&info.id))
            .collect();
        let searchable = unplaced.len() >= SEARCH_FROM;
        let available: Vec<_> = unplaced
            .iter()
            .copied()
            .filter(|info| !searchable || self.desktop.matches(info))
            .collect();
        let mut rows: Vec<Element<'_, Message>> = vec![label(
            format!("Add to the {}", region.title(horizontal)),
            12.,
            MUTED,
        )];
        if searchable {
            rows.push(self.applet_search());
        }
        if unplaced.is_empty() {
            rows.push(label(
                "Everything installed is already here.",
                12.,
                MUTED,
            ));
        } else if available.is_empty() {
            rows.push(label("Nothing matches.", 12., MUTED));
        }
        for (index, info) in available.into_iter().enumerate() {
            if index > 0 {
                rows.push(separator());
            }
            rows.push(
                self.applet_entry(
                    info,
                    button::text("Add")
                        .id(cosmic::widget::Id::new(format!(
                            "applet-picker-add-{}",
                            info.id
                        )))
                        .on_press_maybe(
                            (!self.native_busy).then_some(edit(Edit::Add(region, info.id.clone()))),
                        )
                        .into(),
                ),
            );
        }
        frame(
            column::with_children(rows)
                .spacing(8)
                .width(Length::Fill)
                .into(),
            BASE,
            LINE,
            12,
        )
    }
}
