//! The Edge menu page: the strip's settings, the item list with a live
//! preview of the menu, and the Add picker.
use super::*;
use crate::config::edge_menu::{Action, Item, Kind, OpenOn, Side};
use crate::settings::edge_menu::{Edit, Field, common_folders, folder_item, listed};

fn edit(message: Edit) -> Message {
    Message::EdgeMenu(message)
}

/// What kind of item a row is, under its name.
fn kind_title(item: &Item) -> &'static str {
    match &item.kind {
        Kind::Widget { .. } => "Widget",
        Kind::Action { .. } => "Action",
        Kind::Look { .. } => "Look",
        Kind::App { .. } => "App",
        Kind::Folder { .. } => "Folder",
        Kind::Separator => "Separator",
        Kind::Title { .. } => "Section title",
        Kind::Unknown(_) => "From a newer Halcyon",
    }
}

fn item_icon<'a>(item: &Item, size: u16) -> Element<'a, Message> {
    let (name, symbolic) = crate::app::edge_item_icon(item);
    let icon = icon::from_name(name.as_str()).size(size);
    if symbolic {
        icon.symbolic(true).icon().into()
    } else {
        icon.icon().into()
    }
}

/// A small icon button a screen reader calls `name`.
fn icon_button<'a>(
    glyph: &'static str,
    name: String,
    id: String,
    message: Option<Message>,
    accent: Color,
) -> Element<'a, Message> {
    button::custom(icon::from_name(glyph).size(16).icon())
        .name(name)
        .id(cosmic::widget::Id::new(id))
        .class(nav_style(false, accent))
        .padding(6)
        .on_press_maybe(message)
        .into()
}

impl Settings {
    pub(super) fn edge_menu_page(&self) -> Element<'_, Message> {
        let accent = self.settings_accent();
        let menu = &self.config.edge_menu;
        let page = &self.edge_menu;
        let can_show = self.service_known && self.service.running && !self.busy && !preview();
        let top = split(
            heading(
                "Edge menu",
                "A menu at the edge of the screen with widget switches, shortcuts and actions.",
            ),
            button::standard("Show menu")
                .on_press_maybe(can_show.then_some(Message::ShowControls))
                .into(),
        );
        let position = page.value(menu, Field::Position);
        let length = page.value(menu, Field::Length);
        let delay = page.value(menu, Field::Delay);
        let mut strip_rows = vec![
            switch(
                "Edge menu",
                "Off, nothing waits at the screen edge. The menu still opens with halcyon --toggle drawer, and from Widgets in Settings.",
                menu.enabled,
                |on| edit(Edit::Enabled(on)),
            ),
            group_row(
                "Side",
                "The edge the strip and menu sit on",
                segmented(
                    vec![
                        (
                            "Left".into(),
                            menu.side == Side::Left,
                            Some(edit(Edit::Side(Side::Left))),
                        ),
                        (
                            "Right".into(),
                            menu.side == Side::Right,
                            Some(edit(Edit::Side(Side::Right))),
                        ),
                    ],
                    accent,
                ),
            ),
            group_row(
                "Position",
                "Where along the edge the strip is centred; the menu opens beside it",
                column::with_children(vec![
                    segmented(
                        [("Top", 0u8), ("Middle", 50), ("Bottom", 100)]
                            .into_iter()
                            .map(|(title, at)| {
                                (
                                    title.to_string(),
                                    position == u32::from(at),
                                    Some(edit(Edit::Position(at))),
                                )
                            })
                            .collect(),
                        accent,
                    ),
                    completed_slider(
                        slider(0..=100u32, position, |v| {
                            edit(Edit::Draft(Field::Position, v))
                        })
                        .on_release(edit(Edit::Commit))
                        .name("Position along the edge")
                        .width(Length::Fixed(220.))
                        .into(),
                        edit(Edit::Commit),
                    ),
                ])
                .spacing(8)
                .align_x(Alignment::End)
                .into(),
            ),
            group_row_state(
                "Length",
                format!("Covers {length}% of the edge"),
                completed_slider(
                    slider(10..=100u32, length, |v| edit(Edit::Draft(Field::Length, v)))
                        .on_release(edit(Edit::Commit))
                        .name("Length of the strip")
                        .width(Length::Fixed(220.))
                        .into(),
                    edit(Edit::Commit),
                ),
            ),
            group_row(
                "Opens",
                "Hovering keeps the menu a gesture away; clicking keeps it out of the way",
                segmented(
                    vec![
                        (
                            "On hover".into(),
                            menu.open == OpenOn::Hover,
                            Some(edit(Edit::Open(OpenOn::Hover))),
                        ),
                        (
                            "On click".into(),
                            menu.open == OpenOn::Click,
                            Some(edit(Edit::Open(OpenOn::Click))),
                        ),
                    ],
                    accent,
                ),
            ),
        ];
        if menu.open == OpenOn::Hover {
            strip_rows.push(group_row_state(
                "Hover delay",
                format!(
                    "Opens after the pointer rests {:.1} s on the strip",
                    delay as f32 / 1000.
                ),
                completed_slider(
                    slider(0..=3000u32, delay, |v| edit(Edit::Draft(Field::Delay, v)))
                        .step(100u32)
                        .on_release(edit(Edit::Commit))
                        .name("Hover delay")
                        .width(Length::Fixed(220.))
                        .into(),
                    edit(Edit::Commit),
                ),
            ));
        }
        let strip = container(group(Some("Edge strip"), strip_rows))
            .id(cosmic::widget::Id::new("edge-menu-strip"))
            .into();
        let mut parts = vec![top, strip, self.edge_items_section()];
        if page.picker {
            parts.push(self.edge_picker());
        }
        column::with_children(parts).spacing(20).into()
    }

    /// The list on the left, the menu as the desktop draws it on the right.
    fn edge_items_section(&self) -> Element<'_, Message> {
        let accent = self.settings_accent();
        let items = self.config.edge_items();
        let customized = self.config.edge_menu.customized();
        let mut rows: Vec<Element<'_, Message>> = Vec::new();
        if items.is_empty() {
            rows.push(
                container(
                    column::with_children(vec![
                        icon::from_name("view-list-symbolic").size(32).icon().into(),
                        label("The menu is empty", BODY, FG),
                        label(
                            "Add apps, folders, switches and actions, or reset it to the widget switches.",
                            CAPTION,
                            MUTED,
                        ),
                    ])
                    .spacing(8)
                    .align_x(Alignment::Center),
                )
                .padding(24)
                .width(Length::Fill)
                .center_x(Length::Fill)
                .into(),
            );
        }
        let count = items.len();
        for (index, item) in items.iter().enumerate() {
            rows.push(self.edge_list_row(index, item, count));
        }
        let actions = row::with_children(vec![
            button::suggested("Add")
                .leading_icon(icon::from_name("list-add-symbolic").size(14))
                .on_press(edit(Edit::Picker(!self.edge_menu.picker)))
                .id(cosmic::widget::Id::new("edge-add"))
                .into(),
            button::standard("Reset to default")
                .on_press_maybe(customized.then_some(edit(Edit::Reset)))
                .id(cosmic::widget::Id::new("edge-reset"))
                .into(),
        ])
        .spacing(8);
        let note = label(
            if customized {
                "Drag the handle or use the arrows to reorder. Alt+Up and Alt+Down move items in the open menu too."
            } else {
                "The built-in menu: every widget switch, Arrange and Settings. Any change here starts your own list from it."
            },
            CAPTION,
            MUTED,
        );
        let list =
            column::with_children(vec![group(Some("Menu items"), rows), note, actions.into()])
                .spacing(12)
                .width(Length::Fill);
        let (menu, [width, height]) = self.samples.edge_menu_view();
        let preview = column::with_children(vec![
            label("Preview", CAPTION, MUTED),
            container(menu.map(|_| Message::Preview))
                .width(Length::Fixed(width))
                .height(Length::Fixed(height.min(640.)))
                .into(),
        ])
        .spacing(8)
        .align_x(match self.config.edge_menu.side {
            Side::Left => Alignment::Start,
            Side::Right => Alignment::End,
        });
        let body: Element<'_, Message> = if self.content_width() >= 640. {
            let mut columns = vec![list.into(), preview.into()];
            if self.config.edge_menu.side == Side::Left {
                columns.reverse();
            }
            row::with_children(columns)
                .spacing(24)
                .align_y(Alignment::Start)
                .into()
        } else {
            column::with_children(vec![preview.into(), list.into()])
                .spacing(16)
                .into()
        };
        let _ = accent;
        container(body)
            .id(cosmic::widget::Id::new("edge-menu-items"))
            .into()
    }

    fn edge_list_row(&self, index: usize, item: &Item, count: usize) -> Element<'_, Message> {
        let accent = self.settings_accent();
        let page = &self.edge_menu;
        let customized = self.config.edge_menu.customized();
        let available = !customized
            || (!matches!(item.kind, Kind::Unknown(_))
                && page.available.get(index) != Some(&false));
        let title = crate::app::edge_item_title(item);
        let target = page.reorder.source.is_some() && page.reorder.target == Some(index);
        let handle = mouse_area(
            container(icon::from_name("list-drag-handle-symbolic").size(16).icon()).padding([8, 4]),
        )
        .on_press(edit(Edit::DragStart(index)))
        .interaction(cosmic::iced::mouse::Interaction::Grab);
        let name: Element<'_, Message> = match (&item.kind, &page.title) {
            (Kind::Title { .. }, Some((at, text))) if *at == index => {
                text_input("Section title", text.clone())
                    .on_input(move |text| edit(Edit::TitleText(index, text)))
                    .on_submit(|_| edit(Edit::Commit))
                    .id(cosmic::widget::Id::new(format!("edge-title-{index}")))
                    .width(Length::Fill)
                    .into()
            }
            (Kind::Title { text }, _) => text_input("Section title", text.clone())
                .on_input(move |text| edit(Edit::TitleText(index, text)))
                .id(cosmic::widget::Id::new(format!("edge-title-{index}")))
                .width(Length::Fill)
                .into(),
            _ => column::with_children(vec![
                // An item's name is the user's data, not a setting to search for.
                text::body(title.clone())
                    .size(BODY)
                    .line_height(1.25)
                    .class(text_style(if available { FG } else { MUTED }))
                    .into(),
                label(
                    if available {
                        kind_title(item).to_string()
                    } else {
                        format!("{} · Unavailable", kind_title(item))
                    },
                    CAPTION,
                    if available {
                        MUTED
                    } else {
                        super::desktop::WARN
                    },
                ),
            ])
            .spacing(2)
            .width(Length::Fill)
            .into(),
        };
        let content = row::with_children(vec![
            handle.into(),
            container(item_icon(item, 20))
                .width(24.)
                .center_x(24.)
                .into(),
            name,
            icon_button(
                "go-up-symbolic",
                format!("Move {title} up"),
                format!("edge-row-{index}-up"),
                (index > 0).then_some(edit(Edit::MoveUp(index))),
                accent,
            ),
            icon_button(
                "go-down-symbolic",
                format!("Move {title} down"),
                format!("edge-row-{index}-down"),
                (index + 1 < count).then_some(edit(Edit::MoveDown(index))),
                accent,
            ),
            icon_button(
                "list-remove-symbolic",
                format!("Remove {title}"),
                format!("edge-row-{index}-remove"),
                Some(edit(Edit::Remove(index))),
                accent,
            ),
        ])
        .spacing(8)
        .align_y(Alignment::Center);
        mouse_area(
            container(content)
                .padding([4, 4])
                .width(Length::Fill)
                .class(cosmic::theme::Container::custom(move |theme| {
                    cosmic::iced::widget::container::Style {
                        border: Border {
                            color: theme.cosmic().accent.base.into(),
                            width: if target { 2. } else { 0. },
                            radius: theme.cosmic().corner_radii.radius_s.into(),
                        },
                        ..Default::default()
                    }
                })),
        )
        .on_enter(edit(Edit::DragOver(index)))
        .on_move(move |_| edit(Edit::DragOver(index)))
        .on_exit(edit(Edit::DragLeave(index)))
        .into()
    }

    /// Everything that can be added, as tiles grouped by kind.
    fn edge_picker(&self) -> Element<'_, Message> {
        let accent = self.settings_accent();
        let page = &self.edge_menu;
        let items = self.config.edge_items();
        let tile = |item: Item, caption: Option<String>| -> Element<'_, Message> {
            let title = crate::app::edge_item_title(&item);
            let added = listed(&items, &item);
            let mut lines = vec![
                item_icon(&item, 24),
                text::body(title.clone())
                    .size(12)
                    .align_x(cosmic::iced::alignment::Horizontal::Center)
                    .class(text_style(if added { MUTED } else { FG }))
                    .into(),
            ];
            let caption = if added {
                Some("Added".to_string())
            } else {
                caption
            };
            if let Some(caption) = caption {
                lines.push(label(caption, 11., MUTED));
            }
            button::custom(
                column::with_children(lines)
                    .spacing(4)
                    .align_x(Alignment::Center)
                    .width(Length::Fill),
            )
            .name(if added {
                format!("{title}, already in the menu")
            } else {
                format!("Add {title}")
            })
            .class(nav_style(false, accent))
            .padding([10, 6])
            .width(Length::Fixed(116.))
            .height(Length::Fixed(84.))
            .on_press_maybe((!added).then(|| edit(Edit::Add(item))))
            .into()
        };
        fn section<'a>(
            title: &'static str,
            tiles: Vec<Element<'a, Message>>,
        ) -> Element<'a, Message> {
            column::with_children(vec![
                label(title, CAPTION, MUTED),
                row::with_children(tiles).spacing(8).wrap().into(),
            ])
            .spacing(8)
            .into()
        }
        let widgets = Widget::ALL
            .into_iter()
            .map(|widget| tile(Item::widget(widget), None))
            .collect();
        let actions = Action::ALL
            .into_iter()
            .map(|action| tile(Item::action(action), None))
            .collect();
        let looks = page
            .looks
            .iter()
            .flatten()
            .map(|(look, name)| {
                tile(
                    Item::new(Kind::Look {
                        look: look.clone(),
                        name: name.clone(),
                    }),
                    None,
                )
            })
            .collect();
        let matching = page.matching_apps();
        let shown = 18;
        let mut apps: Vec<Element<'_, Message>> = matching
            .iter()
            .take(shown)
            .map(|app| {
                tile(
                    Item::new(Kind::App {
                        app: app.id.clone(),
                        name: app.name.clone(),
                        icon: app.icon.clone(),
                    }),
                    None,
                )
            })
            .collect();
        if apps.is_empty() {
            apps.push(label("No apps match", CAPTION, MUTED));
        }
        let mut folders: Vec<Element<'_, Message>> = common_folders()
            .into_iter()
            .map(|(name, path)| {
                let mut item = folder_item(&path);
                if let Kind::Folder { name: own, .. } = &mut item.kind {
                    *own = name;
                }
                tile(item, None)
            })
            .collect();
        folders.push(
            button::custom(
                column::with_children(vec![
                    icon::from_name("folder-open-symbolic")
                        .size(24)
                        .icon()
                        .into(),
                    label("Choose…", 12., FG),
                ])
                .spacing(4)
                .align_x(Alignment::Center)
                .width(Length::Fill),
            )
            .name("Choose a folder")
            .class(nav_style(false, accent))
            .padding([10, 6])
            .width(Length::Fixed(116.))
            .height(Length::Fixed(84.))
            .on_press_maybe((!page.folder_pending).then_some(edit(Edit::ChooseFolder)))
            .into(),
        );
        let layout = vec![
            tile(Item::new(Kind::Separator), Some("A thin line".into())),
            tile(Item::title("Section"), Some("A heading".into())),
        ];
        let more = matching.len().saturating_sub(shown);
        let app_search = text_input::search_input("Search apps", page.search.clone())
            .on_input(|text| edit(Edit::AppSearch(text)))
            .id(cosmic::widget::Id::new("edge-app-search"))
            .width(Length::Fixed(260.));
        let mut app_header = vec![label("Apps", CAPTION, MUTED), app_search.into()];
        if more > 0 {
            app_header.push(label(
                format!("{more} more, search to find them"),
                CAPTION,
                MUTED,
            ));
        }
        let apps = column::with_children(vec![
            row::with_children(app_header)
                .spacing(12)
                .align_y(Alignment::Center)
                .into(),
            row::with_children(apps).spacing(8).wrap().into(),
        ])
        .spacing(8)
        .into();
        container(
            column::with_children(vec![
                split(
                    label("Add to the menu", BODY, FG),
                    button::standard("Done")
                        .on_press(edit(Edit::Picker(false)))
                        .id(cosmic::widget::Id::new("edge-picker-done"))
                        .into(),
                ),
                section("Widgets", widgets),
                section("Actions", actions),
                apps,
                section("Folders", folders),
                section("Looks", looks),
                section("Layout", layout),
            ])
            .spacing(16),
        )
        .id(cosmic::widget::Id::new("edge-picker"))
        .padding(16)
        .width(Length::Fill)
        .class(cosmic::theme::Container::custom(|theme| {
            cosmic::iced::widget::container::Style {
                background: Some(Background::Color(surface_color(theme, SURFACE))),
                border: Border {
                    radius: theme.cosmic().corner_radii.radius_m.into(),
                    ..Default::default()
                },
                ..Default::default()
            }
        }))
        .into()
    }
}
