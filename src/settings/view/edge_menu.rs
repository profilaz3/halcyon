//! The Edge menu page: the strip's settings, the item list with a live
//! preview of the menu, and the Add picker.
use super::*;
use crate::config::edge_menu::{
    Action, Close, Displays, Frame, HandleColor, IconSize, Item, Kind, MAX_COLUMNS, MAX_HANDLE,
    MAX_ROUNDNESS, MIN_COLUMNS, MIN_HANDLE, MIN_HANDLE_OPACITY, MIN_OPACITY, OpenOn, Side, Speed,
    Style,
};
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

/// A tiny drawing of the menu in `style`, for the style picker: rows of
/// icon and name, a column of icons, or a grid.
fn style_sketch<'a>(style: Style) -> Element<'a, Message> {
    fn ink(theme: &cosmic::Theme, a: f32) -> Color {
        Color {
            a,
            ..surface_color(theme, FG)
        }
    }
    fn block<'a>(width: f32, height: f32, lit: bool, round: f32) -> Element<'a, Message> {
        container(cosmic::iced::widget::space())
            .width(width)
            .height(height)
            .class(cosmic::theme::Container::custom(move |theme| {
                cosmic::iced::widget::container::Style {
                    background: Some(Background::Color(if lit {
                        theme.cosmic().accent.base.into()
                    } else {
                        ink(theme, 0.38)
                    })),
                    border: Border {
                        radius: round.into(),
                        ..Default::default()
                    },
                    ..Default::default()
                }
            }))
            .into()
    }
    let dot = |lit: bool| block(9., 9., lit, 4.5);
    let lines: Vec<Element<'a, Message>> = match style {
        Style::List => (0..5)
            .map(|at| {
                row::with_children(vec![
                    dot(at == 1),
                    block(if at % 2 == 0 { 30. } else { 22. }, 4., false, 2.),
                ])
                .spacing(6)
                .align_y(Alignment::Center)
                .into()
            })
            .collect(),
        Style::Icons => (0..5).map(|at| dot(at == 1)).collect(),
        Style::Grid => (0..3)
            .map(|at| {
                row::with_children(
                    (0..2)
                        .map(|column| {
                            column::with_children(vec![
                                dot(at == 0 && column == 1),
                                block(12., 3., false, 1.5),
                            ])
                            .spacing(3)
                            .align_x(Alignment::Center)
                            .into()
                        })
                        .collect::<Vec<_>>(),
                )
                .spacing(7)
                .into()
            })
            .collect(),
    };
    let spacing = match style {
        Style::Grid => 6,
        _ => 7,
    };
    // The panel against the screen's right edge.
    let panel = container(
        column::with_children(lines)
            .spacing(spacing)
            .align_x(Alignment::Center),
    )
    .padding([8, 8])
    .class(cosmic::theme::Container::custom(|theme| {
        cosmic::iced::widget::container::Style {
            background: Some(Background::Color(surface_color(theme, SURFACE))),
            border: Border {
                color: surface_color(theme, LINE),
                width: 1.,
                radius: [8., 0., 0., 8.].into(),
            },
            ..Default::default()
        }
    }));
    container(panel)
        .width(Length::Fill)
        .height(84.)
        .align_x(Alignment::End)
        .align_y(Alignment::Center)
        .class(cosmic::theme::Container::custom(|theme| {
            cosmic::iced::widget::container::Style {
                background: Some(Background::Color(surface_color(theme, BASE))),
                border: Border {
                    radius: 8.into(),
                    ..Default::default()
                },
                ..Default::default()
            }
        }))
        .into()
}

impl Settings {
    pub(super) fn edge_menu_page(&self) -> Element<'_, Message> {
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
        let groups = column::with_children(vec![
            self.edge_appearance(),
            self.edge_handle_group(),
            self.edge_behavior(),
            self.edge_items_section(),
        ])
        .spacing(20)
        .width(Length::Fill);
        let body: Element<'_, Message> = if self.content_width() >= 640. {
            let mut columns = vec![groups.into(), self.edge_preview()];
            if menu.side == Side::Left {
                columns.reverse();
            }
            row::with_children(columns)
                .spacing(24)
                .align_y(Alignment::Start)
                .into()
        } else {
            column::with_children(vec![self.edge_preview(), groups.into()])
                .spacing(16)
                .into()
        };
        let mut parts = vec![top, body];
        if page.picker {
            parts.push(self.edge_picker());
        }
        column::with_children(parts).spacing(20).into()
    }

    /// The menu as the desktop draws it, beside its handle on a sketch of
    /// the screen's edge.
    fn edge_preview(&self) -> Element<'_, Message> {
        let menu = &self.config.edge_menu;
        let (rail, [width, height]) = self.samples.edge_menu_view();
        let height = height.min(640.);
        let rail: Element<'_, Message> = container(rail.map(|_| Message::Preview))
            .width(Length::Fixed(width))
            .height(Length::Fixed(height))
            .into();
        let thickness = menu.strip_thickness() as f32;
        let [above, below] = menu.strip_margins(height);
        let handle: Element<'_, Message> = if menu.enabled && menu.handle {
            column::with_children(vec![
                cosmic::iced::widget::space().height(above as f32).into(),
                container(self.samples.edge_handle_view().map(|_| Message::Preview))
                    .width(Length::Fixed(thickness))
                    .height(Length::Fixed((height - (above + below) as f32).max(1.)))
                    .into(),
                cosmic::iced::widget::space().height(below as f32).into(),
            ])
            .into()
        } else {
            cosmic::iced::widget::space()
                .width(thickness)
                .height(height)
                .into()
        };
        let mut screen = vec![rail, cosmic::iced::widget::space().width(6.).into(), handle];
        if menu.side == Side::Left {
            screen.reverse();
        }
        let room = width + 6. + thickness;
        column::with_children(vec![
            label("Preview", CAPTION, MUTED),
            container(row::with_children(screen).align_y(Alignment::Start))
                .width(Length::Fixed(room.max(212.) + 24.))
                .padding([12, 0])
                .align_x(match menu.side {
                    Side::Left => Alignment::Start,
                    Side::Right => Alignment::End,
                })
                .class(cosmic::theme::Container::custom(|theme| {
                    cosmic::iced::widget::container::Style {
                        background: Some(Background::Color(surface_color(theme, BASE))),
                        border: Border {
                            radius: theme.cosmic().corner_radii.radius_m.into(),
                            ..Default::default()
                        },
                        ..Default::default()
                    }
                }))
                .into(),
        ])
        .spacing(8)
        .align_x(match menu.side {
            Side::Left => Alignment::Start,
            Side::Right => Alignment::End,
        })
        .into()
    }

    /// The style picker: a drawing of each style to choose from.
    fn edge_style_picker(&self) -> Element<'_, Message> {
        let accent = self.settings_accent();
        let current = self.config.edge_menu.style;
        let tiles: Vec<Element<'_, Message>> = Style::ALL
            .into_iter()
            .map(|style| {
                let selected = style == current;
                let description = match style {
                    Style::List => "Icon and name",
                    Style::Icons => "Icons only",
                    Style::Grid => "Icons in columns",
                };
                button::custom(
                    column::with_children(vec![
                        style_sketch(style),
                        column::with_children(vec![
                            label(style.title(), BODY, FG),
                            label(description, CAPTION, MUTED),
                        ])
                        .spacing(1)
                        .into(),
                    ])
                    .spacing(8),
                )
                .name(format!("{} style, {description}", style.title()))
                .id(cosmic::widget::Id::new(format!(
                    "edge-style-{}",
                    style.title().to_lowercase()
                )))
                .class(nav_style(selected, accent))
                .selected(selected)
                .padding(8)
                .width(Length::Fixed(148.))
                .on_press(edit(Edit::Style(style)))
                .into()
            })
            .collect();
        row::with_children(tiles).spacing(8).wrap().into()
    }

    fn edge_slider(
        &self,
        title: &'static str,
        state: String,
        field: Field,
        range: std::ops::RangeInclusive<u32>,
        step: u32,
    ) -> Element<'_, Message> {
        let value = self.edge_menu.value(&self.config.edge_menu, field);
        group_row_state(
            title,
            state,
            completed_slider(
                slider(range, value, move |v| edit(Edit::Draft(field, v)))
                    .step(step)
                    .on_release(edit(Edit::Commit))
                    .name(title)
                    .width(Length::Fixed(220.))
                    .into(),
                edit(Edit::Commit),
            ),
        )
    }

    fn edge_appearance(&self) -> Element<'_, Message> {
        let accent = self.settings_accent();
        let menu = &self.config.edge_menu;
        let page = &self.edge_menu;
        let choice = |options: Vec<(String, bool, Edit)>| {
            segmented(
                options
                    .into_iter()
                    .map(|(title, on, message)| (title, on, Some(edit(message))))
                    .collect(),
                accent,
            )
        };
        let mut rows = vec![
            container(
                column::with_children(vec![
                    detail("Style", "How the menu draws its items"),
                    self.edge_style_picker(),
                ])
                .spacing(10),
            )
            .padding([12, 16])
            .width(Length::Fill)
            .into(),
            group_row(
                "Icon size",
                "Bigger icons are easier to hit; the menu grows to fit",
                choice(
                    IconSize::ALL
                        .into_iter()
                        .map(|size| {
                            (
                                size.title().to_string(),
                                menu.icon_size == size,
                                Edit::IconSize(size),
                            )
                        })
                        .collect(),
                ),
            ),
        ];
        if menu.style == Style::Grid {
            rows.push(group_row(
                "Columns",
                "Icons side by side in the grid",
                choice(
                    (MIN_COLUMNS..=MAX_COLUMNS)
                        .map(|columns| {
                            (
                                columns.to_string(),
                                menu.grid_columns() == usize::from(columns),
                                Edit::Columns(columns),
                            )
                        })
                        .collect(),
                ),
            ));
            rows.push(switch(
                "Labels under icons",
                "A short name under each icon in the grid",
                menu.labels,
                |on| edit(Edit::Labels(on)),
            ));
        }
        rows.push(switch(
            "Section titles",
            "Headings over groups of items. Off, groups are set apart by a little space.",
            menu.section_titles,
            |on| edit(Edit::SectionTitles(on)),
        ));
        let opacity = page.value(menu, Field::Opacity);
        rows.push(self.edge_slider(
            "Background opacity",
            format!("{opacity}%"),
            Field::Opacity,
            u32::from(MIN_OPACITY)..=100,
            5,
        ));
        let roundness = page.value(menu, Field::Roundness);
        rows.push(self.edge_slider(
            "Corner roundness",
            if roundness == 0 {
                "Square corners".to_string()
            } else {
                format!("{roundness} px")
            },
            Field::Roundness,
            0..=u32::from(MAX_ROUNDNESS),
            2,
        ));
        rows.push(group_row(
            "Panel",
            "Match your widgets' colors, material and border, or keep a plain panel",
            choice(vec![
                (
                    "Plain".into(),
                    menu.frame == Frame::Plain,
                    Edit::Frame(Frame::Plain),
                ),
                (
                    "Widget style".into(),
                    menu.frame == Frame::Widget,
                    Edit::Frame(Frame::Widget),
                ),
            ]),
        ));
        container(group(Some("Appearance"), rows))
            .id(cosmic::widget::Id::new("edge-menu-appearance"))
            .into()
    }

    fn edge_handle_group(&self) -> Element<'_, Message> {
        let accent = self.settings_accent();
        let menu = &self.config.edge_menu;
        let page = &self.edge_menu;
        let position = page.value(menu, Field::Position);
        let length = page.value(menu, Field::Length);
        let mut rows = vec![
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
            switch(
                "Visible handle",
                "A slim bar on the edge shows where the menu is. Off, the strip is invisible.",
                menu.handle,
                |on| edit(Edit::Handle(on)),
            ),
        ];
        if menu.handle {
            let size = page.value(menu, Field::HandleSize);
            rows.push(self.edge_slider(
                "Handle thickness",
                format!("{size} px"),
                Field::HandleSize,
                u32::from(MIN_HANDLE)..=u32::from(MAX_HANDLE),
                1,
            ));
            let opacity = page.value(menu, Field::HandleOpacity);
            rows.push(self.edge_slider(
                "Handle opacity",
                format!("{opacity}%"),
                Field::HandleOpacity,
                u32::from(MIN_HANDLE_OPACITY)..=100,
                5,
            ));
            rows.push(group_row(
                "Handle color",
                "Your accent color, or a quiet gray",
                segmented(
                    vec![
                        (
                            "Accent".into(),
                            menu.handle_color == HandleColor::Accent,
                            Some(edit(Edit::HandleColor(HandleColor::Accent))),
                        ),
                        (
                            "Neutral".into(),
                            menu.handle_color == HandleColor::Neutral,
                            Some(edit(Edit::HandleColor(HandleColor::Neutral))),
                        ),
                    ],
                    accent,
                ),
            ));
        }
        container(group(Some("Handle"), rows))
            .id(cosmic::widget::Id::new("edge-menu-strip"))
            .into()
    }

    fn edge_behavior(&self) -> Element<'_, Message> {
        let accent = self.settings_accent();
        let menu = &self.config.edge_menu;
        let delay = self.edge_menu.value(menu, Field::Delay);
        let mut rows = vec![group_row(
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
        )];
        if menu.open == OpenOn::Hover {
            rows.push(group_row_state(
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
        rows.push(group_row(
            "Close after use",
            match menu.close {
                Close::Never => "The menu stays open until the pointer leaves it",
                Close::Actions => {
                    "Apps, folders, Looks and actions close it; switches keep it open"
                }
                Close::Always => "Any item closes it, switches too",
            },
            segmented(
                [
                    ("Never", Close::Never),
                    ("After actions", Close::Actions),
                    ("Always", Close::Always),
                ]
                .into_iter()
                .map(|(title, close)| {
                    (
                        title.to_string(),
                        menu.close == close,
                        Some(edit(Edit::Close(close))),
                    )
                })
                .collect(),
                accent,
            ),
        ));
        rows.push(switch(
            "Animation",
            "Slide the menu in and out",
            self.config.sidebar_slide,
            Message::SidebarSlide,
        ));
        if self.config.sidebar_slide {
            rows.push(group_row(
                "Animation speed",
                "How quickly the menu slides",
                segmented(
                    Speed::ALL
                        .into_iter()
                        .map(|speed| {
                            (
                                speed.title().to_string(),
                                menu.speed == speed,
                                Some(edit(Edit::Speed(speed))),
                            )
                        })
                        .collect(),
                    accent,
                ),
            ));
        }
        rows.push(group_row(
            "Displays",
            match menu.displays {
                Displays::Main => "Only the display your widgets are on",
                Displays::All => "Every connected display has its own strip",
                Displays::Chosen => "The displays you pick below",
            },
            segmented(
                [
                    ("Main display", Displays::Main),
                    ("All", Displays::All),
                    ("Chosen", Displays::Chosen),
                ]
                .into_iter()
                .map(|(title, displays)| {
                    (
                        title.to_string(),
                        menu.displays == displays,
                        Some(edit(Edit::Displays(displays))),
                    )
                })
                .collect(),
                accent,
            ),
        ));
        if menu.displays == Displays::Chosen {
            rows.push(self.edge_display_choices());
        }
        container(group(Some("Behavior"), rows))
            .id(cosmic::widget::Id::new("edge-menu-behavior"))
            .into()
    }

    /// Each connected display, and any chosen one that isn't connected now,
    /// as a chip to pick.
    fn edge_display_choices(&self) -> Element<'_, Message> {
        let accent = self.settings_accent();
        let menu = &self.config.edge_menu;
        let mut chips: Vec<(String, String)> = self
            .displays
            .outputs()
            .iter()
            .filter_map(|output| Some((output.title(), self.displays.key(output)?)))
            .collect();
        chips.sort_by(|a, b| a.1.cmp(&b.1));
        for saved in &menu.outputs {
            if !chips.iter().any(|(_, key)| key == saved) {
                chips.push((format!("{saved} (not connected)"), saved.clone()));
            }
        }
        let controls: Vec<Element<'_, Message>> = if chips.is_empty() {
            vec![label("No displays found yet", CAPTION, MUTED)]
        } else {
            chips
                .into_iter()
                .map(|(title, key)| {
                    let on = menu.outputs.contains(&key);
                    button::custom(label(title.clone(), 12., if on { FG } else { MUTED }))
                        .name(format!(
                            "{title}, {}",
                            if on { "chosen" } else { "not chosen" }
                        ))
                        .id(cosmic::widget::Id::new(format!("edge-display-{key}")))
                        .class(nav_style(on, accent))
                        .selected(on)
                        .padding([6, 12])
                        .on_press(edit(Edit::Output(key, !on)))
                        .into()
                })
                .collect()
        };
        container(row::with_children(controls).spacing(6).wrap())
            .padding([8, 16, 12, 16])
            .width(Length::Fill)
            .into()
    }

    /// The list of items, with its Add and Reset buttons.
    fn edge_items_section(&self) -> Element<'_, Message> {
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
        container(
            column::with_children(vec![group(Some("Menu items"), rows), note, actions.into()])
                .spacing(12)
                .width(Length::Fill),
        )
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
