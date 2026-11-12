//! The item editor both bars share: regions of drag-and-drop rows with
//! arrow controls, an inline picker, and the panel's few extras.
use super::*;
use crate::desktop::applets::AppletGroup;
use crate::desktop::panels::Regions;
use crate::settings::desktop::{AppletControl, control_id};
use cosmic::Apply;

/// The picker and the installed list offer search from this many applets.
const SEARCH_FROM: usize = 8;

impl Settings {
    pub(in crate::settings) fn activity_preferences(&self) -> Element<'_, Message> {
        let p = &self.config.activity_island;
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
                ))
                .push(self.activity_app_rules());
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
                ("Device connected", "device_banners", p.device_banners),
                ("Low battery", "low_battery", p.low_battery),
                ("Software updates", "updates", p.updates),
            ] {
                controls = controls.push(toggle(name, key, on, true));
            }
        }
        controls = controls.push(toggle("Brief automatic reveals", "reveals", p.reveals, false));
        controls = controls.push(split(
            label("Island style", 14., FG),
            container(cosmic::widget::dropdown(
                &["Follow the theme", "Dark"],
                Some(p.style.index()),
                Message::ActivityStyle,
            ))
            .id(cosmic::widget::Id::new("activity-preference-style"))
            .into(),
        ));
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
    /// Per-app notification rules chosen in the island. A muted app can
    /// always be unmuted here.
    fn activity_app_rules(&self) -> Element<'_, Message> {
        use crate::activity_island::preferences::AppRule;
        const RULES: [AppRule; 3] = [AppRule::Mute, AppRule::Always, AppRule::ListOnly];
        let rules = &self.config.activity_island.app_rules;
        let mut list =
            cosmic::widget::column::with_children([label("App rules", 13., FG)]).spacing(8);
        if rules.is_empty() {
            list = list.push(label(
                "No app rules yet. Right-click a notification in the island to add one.",
                12.,
                MUTED,
            ));
        }
        for (index, rule) in rules.iter().enumerate() {
            let mut name =
                cosmic::widget::column::with_children([label(rule.app.clone(), 13., FG)]);
            if !rule.entry.is_empty() {
                name = name.push(label(rule.entry.clone(), 11., MUTED));
            }
            let controls = row::with_children(vec![
                container(cosmic::widget::dropdown(
                    &["Muted", "Always show", "List only"],
                    RULES.iter().position(|r| *r == rule.rule),
                    move |choice| Message::ActivityAppRule(index, RULES.get(choice).copied()),
                ))
                .id(cosmic::widget::Id::new(format!(
                    "activity-app-rule-{index}"
                )))
                .into(),
                button::text("Reset")
                    .id(cosmic::widget::Id::new(format!(
                        "activity-app-rule-reset-{index}"
                    )))
                    .on_press(Message::ActivityAppRule(index, None))
                    .into(),
            ])
            .spacing(8)
            .align_y(Alignment::Center);
            list = list.push(split(name.into(), controls.into()));
        }
        container(list)
            .id("activity-app-rules")
            .padding([0, 0, 0, 20])
            .into()
    }
    /// A top panel without the Activity island offers to add it, centred.
    pub(in crate::settings) fn add_activity_island(
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
                button::text("Add Activity pill")
                    .id("panel-add-activity".into())
                    .on_press_maybe((!self.native_busy && !editor.drag.active()).then_some(edit(
                        Edit::Add(Region::Center, crate::activity_island::APPLET_ID.into()),
                    )))
                    .into(),
            )
        })
    }

    /// The one item editor for both bars. Each region lists its items as
    /// rows (drag handle, name, arrows, Remove) under its own Add, and a
    /// picked row shows where else it can go. Only the panel's extras differ:
    /// the Activity island and its starter layout.
    pub(super) fn bar_items(&self, draft: &PanelDraft) -> Element<'_, Message> {
        let accent = self.settings_accent();
        let editor = &self.desktop;
        let panel = draft.kind() == PanelKind::Panel;
        let horizontal = draft
            .anchor()
            .known()
            .is_none_or(|anchor| anchor.horizontal());
        let header: Element<'_, Message> = text::heading(items::HEADING).into();
        let id = if panel { "panel-items" } else { "dock-items" };
        let regions = match draft.applets() {
            Setting::Known(regions) => regions,
            Setting::Unsupported(raw) => {
                return container(
                    column::with_children(vec![
                        header,
                        section(vec![
                            banner(
                                "This layout uses a format Halcyon can’t edit. It is kept as it is.",
                                WARN,
                                Vec::new(),
                            ),
                            kept(&raw),
                        ]),
                    ])
                    .spacing(8),
                )
                .id(cosmic::widget::Id::new(id))
                .width(Length::Fill)
                .into();
            }
        };
        let mut blocks: Vec<Element<'_, Message>> = Vec::new();
        if let Some(add) = self.add_activity_island(draft, &regions) {
            blocks.push(add);
        }
        // Drop targets never appear or disappear when a drag starts, so the
        // layout cannot shift under the pointer: a region's header inserts at
        // its start, a row takes that row's place, an empty region's zone
        // receives the item.
        let dragging = editor.drag.active();
        let idle = !dragging && !self.native_busy;
        // Islands list in the order the screen shows them.
        let screen = editor.screen_order(draft.kind());
        for region in screen.regions() {
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
                    screen.name(region),
                    13.,
                    if header_target { accent } else { FG },
                ),
                button::text(if picking { items::CLOSE } else { items::ADD })
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
                    .on_press_maybe(idle.then(|| edit(Edit::Picker((!picking).then_some(region)))))
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
            blocks.push(self.drop_target(header.into(), start));
            if picking {
                blocks.push(self.item_picker(draft, region));
            }
            let mut rows: Vec<Element<'_, Message>> = Vec::new();
            for (index, id) in list.iter().enumerate() {
                let slot = Slot { region, index };
                rows.push(self.item_row(id, slot, &regions, accent));
                if editor.focus == Some(slot) && !dragging {
                    rows.push(self.item_details(id, slot, screen));
                }
            }
            if list.is_empty() {
                let target = dragging && editor.drag.target == Some(start);
                let zone = container(label(
                    if dragging { items::DROP } else { items::EMPTY },
                    11.,
                    if target { accent } else { MUTED },
                ))
                .padding([8, 12])
                .width(Length::Fill)
                .class(cosmic::theme::Container::custom(move |theme| {
                    cosmic::iced::widget::container::Style {
                        background: Some(Background::Color(Color {
                            a: if target { 0.12 } else { 0.03 },
                            ..accent
                        })),
                        border: Border {
                            color: if target {
                                accent
                            } else {
                                surface_color(theme, LINE)
                            },
                            width: 1.,
                            radius: 10.into(),
                        },
                        ..Default::default()
                    }
                }));
                rows.push(self.drop_target(zone.into(), start));
            }
            blocks.push(column::with_children(rows).spacing(2).into());
        }
        if panel && editor.starter_open {
            blocks.extend(self.panel_starter(horizontal, idle));
        } else {
            let hint = label(
                items::hint(dragging, regions == Regions::default()),
                11.,
                MUTED,
            );
            blocks.push(if panel {
                split(
                    hint,
                    button::text("Starter layout…")
                        .on_press_maybe(idle.then_some(edit(Edit::Starter(true))))
                        .into(),
                )
            } else {
                hint
            });
        }
        // The island's own options live on its page.
        if panel
            && Region::ALL.iter().any(|r| {
                regions
                    .list(*r)
                    .iter()
                    .any(|id| id == crate::activity_island::APPLET_ID)
            })
        {
            blocks.push(separator());
            blocks.push(self.link_row_state(
                "Activity island",
                self.part_state(Page::ActivityIsland),
                Message::Page(Page::ActivityIsland),
                "panel-open-island".into(),
            ));
        }
        container(column::with_children(vec![header, section(blocks)]).spacing(8))
            .id(cosmic::widget::Id::new(id))
            .width(Length::Fill)
            .into()
    }

    fn drop_target<'a>(&self, content: Element<'a, Message>, slot: Slot) -> Element<'a, Message> {
        mouse_area(content)
            .on_release(edit(Edit::DropAt(slot)))
            .on_enter(edit(Edit::DragOver(slot)))
            .on_move(move |_| edit(Edit::DragOver(slot)))
            .on_exit(edit(Edit::DragLeave(slot)))
            .into()
    }

    /// The panel's starter layout, previewed before it replaces the items.
    fn panel_starter(&self, horizontal: bool, idle: bool) -> Vec<Element<'_, Message>> {
        let editor = &self.desktop;
        let starter = editor.catalog.starter(editor.has_battery);
        let mut blocks = vec![separator(), label("A quiet starting point", 14., FG)];
        for region in Region::ALL {
            let names = starter
                .list(region)
                .iter()
                .map(|id| editor.catalog.panel_label(id))
                .collect::<Vec<_>>();
            blocks.push(label(
                format!(
                    "{}: {}",
                    region.title(horizontal),
                    if names.is_empty() {
                        "Empty".into()
                    } else {
                        names.join(", ")
                    }
                ),
                12.,
                MUTED,
            ));
        }
        blocks.push(label(
            "Choose a layout to replace this panel’s items. Undo restores your previous layout.",
            11.,
            MUTED,
        ));
        blocks.push(
            row::with_children(vec![
                button::suggested("Apply starter layout")
                    .on_press_maybe(
                        (idle && starter != Regions::default()).then_some(edit(Edit::UseStarter)),
                    )
                    .into(),
                button::text("Cancel")
                    .on_press(edit(Edit::Starter(false)))
                    .into(),
            ])
            .spacing(8)
            .into(),
        );
        blocks
    }

    fn item_row(
        &self,
        id: &str,
        slot: Slot,
        regions: &Regions,
        accent: Color,
    ) -> Element<'_, Message> {
        let editor = &self.desktop;
        let known = editor.catalog.known(id);
        let dragging = editor.drag.source == Some(slot);
        let target = editor.drag.active() && editor.drag.target == Some(slot) && !dragging;
        let focused = editor.focus == Some(slot);
        let first = slot.region == Region::Start && slot.index == 0;
        let last = slot.region == Region::End && slot.index + 1 == regions.end.len();
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
                    items::DROP
                } else if dragging {
                    "Esc cancels"
                } else {
                    "Not installed"
                },
                10.,
                if target { accent } else { MUTED },
            ));
        }
        // Pressing the name shows the item's details and where else it can go.
        let name = button::custom(
            row::with_children(vec![
                icon::from_name(editor.catalog.icon(id))
                    .size(18)
                    .icon()
                    .into(),
                column::with_children(title)
                    .spacing(2)
                    .width(Length::Fill)
                    .into(),
            ])
            .spacing(8)
            .align_y(Alignment::Center),
        )
        .id(control_id(slot, AppletControl::Select))
        .class(cosmic::theme::Button::Text)
        .padding([2, 4])
        .width(Length::Fill)
        .on_press_maybe(idle.then(|| edit(Edit::SelectApplet((!focused).then_some(slot)))));
        let parts: Vec<Element<'_, Message>> = vec![
            handle.into(),
            cosmic::widget::tooltip(
                name,
                text::body(editor.catalog.label(id)),
                cosmic::widget::tooltip::Position::Bottom,
            )
            .into(),
            // Stable per-occurrence ids let focus follow a keyboard move.
            button::icon(icon::from_name("go-up-symbolic"))
                .padding(6)
                .id(control_id(slot, AppletControl::Up))
                .on_press_maybe((!first && idle).then_some(edit(Edit::Step(slot, -1))))
                .apply(|control| super::super::tip::tip(control, "Move up")),
            button::icon(icon::from_name("go-down-symbolic"))
                .padding(6)
                .id(control_id(slot, AppletControl::Down))
                .on_press_maybe((!last && idle).then_some(edit(Edit::Step(slot, 1))))
                .apply(|control| super::super::tip::tip(control, "Move down")),
            button::icon(icon::from_name("window-close-symbolic"))
                .padding(6)
                .id(control_id(slot, AppletControl::Remove))
                .on_press_maybe(idle.then_some(edit(Edit::Remove(slot))))
                .apply(|control| super::super::tip::tip(control, "Remove")),
        ];
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
        self.drop_target(content.into(), slot)
    }

    /// Under a picked row: what the item is, and a move to another region.
    fn item_details(
        &self,
        id: &str,
        slot: Slot,
        screen: crate::settings::desktop::islands::ScreenOrder,
    ) -> Element<'_, Message> {
        let editor = &self.desktop;
        let accent = self.settings_accent();
        let idle = !editor.drag.active() && !self.native_busy;
        let description = editor.catalog.get(id).map_or(
            "Not installed. It stays in this bar until you remove it.",
            |i| i.description(),
        );
        let mut lines = Vec::new();
        if !description.is_empty() {
            lines.push(label(description, 12., MUTED));
        }
        lines.push(
            row::with_children(vec![
                label("Move to", 12., MUTED),
                segmented(
                    screen
                        .regions()
                        .into_iter()
                        .map(|region| {
                            (
                                screen.name(region).into(),
                                slot.region == region,
                                (idle && slot.region != region)
                                    .then_some(edit(Edit::MoveTo(slot, region))),
                            )
                        })
                        .collect(),
                    accent,
                ),
                button::text("Done")
                    .on_press(edit(Edit::SelectApplet(None)))
                    .into(),
            ])
            .spacing(8)
            .align_y(Alignment::Center)
            .wrap()
            .into(),
        );
        container(column::with_children(lines).spacing(8))
            .padding([4, 12, 8, 40])
            .width(Length::Fill)
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
        for (kind, region, _) in self.desktop.placements(&info.id) {
            let here = kind == self.desktop.selected;
            title.push(pill(
                format!(
                    "{} · {}",
                    kind.title(),
                    self.desktop.screen_order(kind).name(region)
                ),
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

    /// Opens under a region's Add: every installed item, with where it is
    /// already used. Search and categories appear once the list is long.
    fn item_picker(&self, draft: &PanelDraft, region: Region) -> Element<'_, Message> {
        let editor = &self.desktop;
        let accent = self.settings_accent();
        let placed = draft.applets().known().cloned().unwrap_or_default();
        let installed = editor.catalog.sorted();
        let searchable = installed.len() >= SEARCH_FROM;
        let shown: Vec<_> = installed
            .iter()
            .copied()
            .filter(|info| {
                !searchable || (editor.applet_group.includes(info) && editor.matches(info))
            })
            .collect();
        let mut rows: Vec<Element<'_, Message>> = vec![label(
            format!(
                "Add to the {}",
                editor.screen_order(draft.kind()).name(region)
            ),
            12.,
            MUTED,
        )];
        if searchable {
            rows.push(self.applet_search());
            rows.push(segmented(
                AppletGroup::ALL
                    .into_iter()
                    .map(|g| {
                        (
                            g.title().into(),
                            editor.applet_group == g,
                            Some(edit(Edit::AppletGroup(g))),
                        )
                    })
                    .collect(),
                accent,
            ));
        }
        if installed.is_empty() {
            rows.push(label("No items are installed.", 12., MUTED));
        } else if shown.is_empty() {
            rows.push(label(
                if editor.applet_group == AppletGroup::Readings
                    && editor.applet_query.trim().is_empty()
                {
                    "No system readings item is installed. The System monitor widget shows them on the desktop."
                } else {
                    "Nothing matches. Try another search or category."
                },
                12.,
                MUTED,
            ));
        }
        for (index, info) in shown.into_iter().enumerate() {
            if index > 0 {
                rows.push(separator());
            }
            let here = placed.contains(&info.id);
            rows.push(
                self.applet_entry(
                    info,
                    button::text(if here { "Add another" } else { items::ADD })
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

/// The words both bars’ item editors use, kept in one place so they match.
mod items {
    pub(super) const HEADING: &str = "Items";
    pub(super) const ADD: &str = "Add";
    pub(super) const CLOSE: &str = "Close";
    pub(super) const EMPTY: &str = "Empty. Drag an item here or use Add";
    pub(super) const DROP: &str = "Drop here";

    /// The line at the foot of the editor.
    pub(super) fn hint(dragging: bool, empty: bool) -> &'static str {
        if dragging {
            "Drop on a row or region. Esc cancels."
        } else if empty {
            "Nothing here yet. Use Add to put items on this bar."
        } else {
            "Drag to reorder. Press a name to move it to another region."
        }
    }
}
