//! The edge menu's Icons and Grid styles, and the visible handle on its
//! strip. Icons is a slim column of icons; Grid sets them in two to four
//! columns with short names under them. An item's name and state show in a
//! small bubble beside the menu while the pointer or the keyboard is on it,
//! and a screen reader hears both on every item.
use super::edge::{TIP_ROOM, item_icon, item_title};
use super::*;
use crate::config::edge_menu::{Action, EdgeMenu, HandleColor, Item, Kind, Side, Style};
use cosmic::iced::mouse;
use drawer::DrawerItem;

/// Inside the panel's edge.
pub(super) const PAD: f32 = 10.;
/// Between neighbouring cells, across and down.
const GAP: f32 = 4.;
/// Where a section title was, in the Icons style.
const TITLE_GAP: f32 = 12.;
/// A section title over its items in the grid.
const HEADING: f32 = 22.;
/// A separator: a thin line with room above and below.
const RULE: f32 = 13.;
const FOOT_GAP: f32 = 6.;
/// A name under an icon in the grid: up to two short lines.
const LABEL: f32 = 28.;
/// A grid cell with a name under it is at least this wide.
const LABEL_WIDTH: f32 = 76.;

/// A cell's square: the icon with room for its disc and the focus ring.
fn cell(menu: &EdgeMenu) -> f32 {
    f32::from(menu.compact_icon()) + 24.
}
fn columns(menu: &EdgeMenu) -> usize {
    match menu.style {
        Style::Grid => menu.grid_columns(),
        Style::List | Style::Icons => 1,
    }
}
fn labelled(menu: &EdgeMenu) -> bool {
    menu.style == Style::Grid && menu.labels
}
fn cell_width(menu: &EdgeMenu) -> f32 {
    if labelled(menu) {
        cell(menu).max(LABEL_WIDTH)
    } else {
        cell(menu)
    }
}
fn cell_height(menu: &EdgeMenu) -> f32 {
    cell(menu) + if labelled(menu) { LABEL } else { 0. }
}
/// The panel's width in the Icons and Grid styles.
pub(super) fn width(menu: &EdgeMenu) -> f32 {
    let columns = columns(menu) as f32;
    columns * cell_width(menu) + (columns - 1.) * GAP + 2. * PAD
}

/// One line of the menu, top down.
#[derive(Debug, PartialEq)]
pub(super) enum Line {
    Gap(f32),
    Rule,
    /// A section title, shown in the grid.
    Heading(usize),
    /// A row of cells: one in the Icons style, up to the columns in the grid.
    Cells(Vec<usize>),
}

/// The Icons or Grid menu laid out: what each line holds and how tall it is.
pub(super) struct Compact {
    pub lines: Vec<Line>,
    cell: f32,
    cell_height: f32,
}
impl Compact {
    pub fn new(menu: &EdgeMenu, items: &[Item]) -> Self {
        let grid = menu.style == Style::Grid;
        let columns = columns(menu);
        let mut lines = Vec::new();
        let mut run = Vec::new();
        fn flush(lines: &mut Vec<Line>, run: &mut Vec<usize>) {
            if run.is_empty() {
                return;
            }
            if matches!(lines.last(), Some(Line::Cells(_))) {
                lines.push(Line::Gap(GAP));
            }
            lines.push(Line::Cells(std::mem::take(run)));
        }
        for (index, item) in items.iter().enumerate() {
            match &item.kind {
                Kind::Title { .. } => {
                    flush(&mut lines, &mut run);
                    if !menu.section_titles {
                        continue;
                    }
                    if grid {
                        if !lines.is_empty() {
                            lines.push(Line::Gap(6.));
                        }
                        lines.push(Line::Heading(index));
                    } else if !lines.is_empty() {
                        lines.push(Line::Gap(TITLE_GAP));
                    }
                }
                Kind::Separator => {
                    flush(&mut lines, &mut run);
                    lines.push(Line::Rule);
                }
                _ => {
                    run.push(index);
                    if run.len() == columns {
                        flush(&mut lines, &mut run);
                    }
                }
            }
        }
        flush(&mut lines, &mut run);
        Self {
            lines,
            cell: cell(menu),
            cell_height: cell_height(menu),
        }
    }
    fn line_height(&self, line: &Line) -> f32 {
        match line {
            Line::Gap(height) => *height,
            Line::Rule => RULE,
            Line::Heading(_) => HEADING,
            Line::Cells(_) => self.cell_height,
        }
    }
    fn body(&self) -> f32 {
        self.lines.iter().map(|line| self.line_height(line)).sum()
    }
    /// The panel's height: the lines, then Customize.
    pub fn height(&self) -> f32 {
        2. * PAD + self.body() + FOOT_GAP + self.cell
    }
    /// The middle of item `index`'s icon, from the top of the panel.
    pub fn centre(&self, index: usize) -> Option<f32> {
        let mut y = PAD;
        for line in &self.lines {
            if let Line::Cells(cells) = line
                && cells.contains(&index)
            {
                return Some(y + self.cell / 2.);
            }
            y += self.line_height(line);
        }
        None
    }
    /// The middle of Customize, from the top of a panel `height` tall.
    pub fn footer_centre(&self, height: f32) -> f32 {
        height - PAD - self.cell / 2.
    }
}

/// "4:32" left on a timer, or "1h05".
pub(super) fn remaining(ms: i64) -> String {
    let seconds = (ms.max(0) + 999) / 1000;
    if seconds >= 3600 {
        format!("{}h{:02}", seconds / 3600, seconds % 3600 / 60)
    } else {
        format!("{}:{:02}", seconds / 60, seconds % 60)
    }
}

/// The strip on the screen edge: clear, or the visible handle rounded on
/// the side facing the screen.
pub(super) fn handle<'a>(config: &Config) -> Element<'a, Message> {
    let menu = &config.edge_menu;
    let strip = container(cosmic::iced::widget::space())
        .width(Length::Fill)
        .height(Length::Fill);
    if !menu.handle {
        return strip.into();
    }
    let p = config.surface_palette();
    let color = Color {
        a: f32::from(menu.handle_opacity.clamp(10, 100)) / 100.,
        ..match menu.handle_color {
            HandleColor::Accent => p.accent,
            HandleColor::Neutral => mix(p.foreground, p.background, 0.45),
        }
    };
    let r = f32::from(menu.handle_size) / 2.;
    let radius = match menu.side {
        Side::Right => [r, 0., 0., r],
        Side::Left => [0., r, r, 0.],
    };
    strip
        .class(cosmic::theme::Container::custom(move |_| {
            cosmic::iced::widget::container::Style {
                background: Some(Background::Color(color)),
                border: Border {
                    radius: radius.into(),
                    ..Default::default()
                },
                ..Default::default()
            }
        }))
        .into()
}

impl Cards {
    /// An item's name, and its state when it has one: "On", "Off", the
    /// time left on the timer, "Unavailable".
    pub(super) fn item_label(&self, index: usize, item: &Item) -> (String, Option<String>) {
        let title = item_title(item);
        if !self.edge_available(index) {
            return (title, Some("Unavailable".into()));
        }
        if let Some(on) = self.item_state(item) {
            return (title, Some(if on { "On" } else { "Off" }.into()));
        }
        if item.as_action() == Some(Action::Timer)
            && let Some(timer) = &self.activity.timer
        {
            let left = remaining(timer.remaining(chrono::Utc::now().timestamp_millis()));
            let state = if timer.paused() {
                format!("Paused, {left} left")
            } else {
                format!("{left} left")
            };
            return (title, Some(state));
        }
        (title, self.item_caption(item))
    }
    /// The Icons or Grid menu, with room beside it for the name of the item
    /// the pointer or keyboard is on.
    pub(super) fn compact_rail(&self, items: &[Item]) -> Element<'_, Message> {
        let menu = &self.config.edge_menu;
        let p = self.config.surface_palette();
        let layout = Compact::new(menu, items);
        let focus = self.drawer_focus();
        let mut body: Vec<Element<'_, Message>> = Vec::new();
        for line in &layout.lines {
            body.push(match line {
                Line::Gap(height) => space(*height),
                Line::Rule => container(
                    container(cosmic::iced::widget::space())
                        .width(Length::Fill)
                        .height(1.)
                        .class(cosmic::theme::Container::custom(move |_| {
                            cosmic::iced::widget::container::Style {
                                background: Some(Background::Color(mix(
                                    p.foreground,
                                    p.background,
                                    0.18,
                                ))),
                                ..Default::default()
                            }
                        })),
                )
                .padding([6, 6])
                .height(RULE)
                .into(),
                Line::Heading(index) => container(
                    text::body(item_title(&items[*index]))
                        .size(11.)
                        .font(cosmic::font::semibold())
                        .wrapping(cosmic::iced::advanced::text::Wrapping::Word)
                        .class(cosmic::theme::Text::Color(mix(p.foreground, p.muted, 0.5))),
                )
                .padding([0, 4])
                .height(HEADING)
                .width(Length::Fill)
                .clip(true)
                .align_y(Alignment::Center)
                .into(),
                Line::Cells(cells) => row::with_children(
                    cells
                        .iter()
                        .map(|index| {
                            self.compact_cell(
                                *index,
                                &items[*index],
                                focus == Some(DrawerItem::Entry(*index)),
                            )
                        })
                        .collect::<Vec<_>>(),
                )
                .spacing(GAP)
                .into(),
            });
        }
        let customize = mouse_area(
            cosmic::widget::button::custom(
                container(
                    cosmic::widget::icon::from_name("document-edit-symbolic")
                        .size(16)
                        .symbolic(true)
                        .icon()
                        .class(cosmic::theme::Svg::custom(move |_| {
                            cosmic::iced::widget::svg::Style {
                                color: Some(p.muted),
                            }
                        })),
                )
                .center(Length::Fill),
            )
            .name("Customize this menu")
            .id(cosmic::widget::Id::new("edge-customize"))
            .padding(0)
            .width(Length::Fixed(layout.cell))
            .height(Length::Fixed(layout.cell))
            .class(cell_style(p, focus == Some(DrawerItem::Customize), false))
            .on_press(Message::EdgeCustomize),
        )
        .on_enter(Message::EdgeTip(Some(DrawerItem::Customize)))
        .on_exit(Message::EdgeTip(None))
        .interaction(mouse::Interaction::Pointer);
        let panel = self.rail_panel(
            column::with_children(vec![
                scrollable(column::with_children(body).width(Length::Fill))
                    .id(cosmic::widget::Id::new("edge-menu-items"))
                    .height(Length::Fill)
                    .into(),
                space(FOOT_GAP),
                container(customize).center_x(Length::Fill).into(),
            ])
            .width(Length::Fill)
            .into(),
            PAD,
        );
        let panel: Element<'_, Message> = container(panel)
            .width(Length::Fixed(self.rail_width() as f32))
            .height(Length::Fill)
            .into();
        if self.edge.preview {
            // Settings' preview: the menu alone.
            return panel;
        }
        let tip = self.tip_bubble(items, &layout);
        let parts = match menu.side {
            Side::Right => vec![tip, panel],
            Side::Left => vec![panel, tip],
        };
        row::with_children(parts).height(Length::Fill).into()
    }

    /// The name of the item under the pointer, or the keyboard's, beside
    /// it in the clear room next to the menu.
    fn tip_bubble(&self, items: &[Item], layout: &Compact) -> Element<'_, Message> {
        let p = self.config.surface_palette();
        let side = self.config.edge_menu.side;
        let target = self
            .edge
            .tip
            .or_else(|| self.sidebar.focus.and_then(|_| self.drawer_focus()));
        let height = self.controls_height() as f32;
        let found = match target {
            Some(DrawerItem::Entry(index)) => items.get(index).and_then(|item| {
                let (title, state) = self.item_label(index, item);
                Some((layout.centre(index)?, title, state))
            }),
            Some(DrawerItem::Customize) => Some((
                layout.footer_centre(height),
                "Customize this menu".to_string(),
                None,
            )),
            _ => None,
        };
        let room = container(cosmic::iced::widget::space())
            .width(Length::Fixed(TIP_ROOM as f32))
            .height(Length::Fill);
        let Some((centre, title, state)) = found else {
            return room.into();
        };
        let mut words = vec![
            text::body(title)
                .size(12.)
                .font(cosmic::font::semibold())
                .wrapping(cosmic::iced::advanced::text::Wrapping::None)
                .class(cosmic::theme::Text::Color(p.foreground))
                .into(),
        ];
        if let Some(state) = state {
            words.push(
                text::body(state)
                    .size(11.)
                    .wrapping(cosmic::iced::advanced::text::Wrapping::None)
                    .class(cosmic::theme::Text::Color(p.muted))
                    .into(),
            );
        }
        const BUBBLE: f32 = 30.;
        let bubble = container(
            row::with_children(words)
                .spacing(6)
                .align_y(Alignment::Center),
        )
        .padding([0, 10])
        .height(BUBBLE)
        .align_y(Alignment::Center)
        .max_width(TIP_ROOM as f32 - 12.)
        .clip(true)
        .class(cosmic::theme::Container::custom(move |_| {
            cosmic::iced::widget::container::Style {
                background: Some(Background::Color(Color {
                    a: 0.98,
                    ..mix(p.foreground, p.background, 0.1)
                })),
                border: Border {
                    color: mix(p.foreground, p.background, 0.24),
                    width: 1.,
                    radius: 9.into(),
                },
                ..Default::default()
            }
        }));
        let top = (centre - BUBBLE / 2.).clamp(0., (height - BUBBLE).max(0.));
        container(column::with_children(vec![space(top), bubble.into()]))
            .padding(match side {
                Side::Right => [0, 6, 0, 0],
                Side::Left => [0, 0, 0, 6],
            })
            .width(Length::Fixed(TIP_ROOM as f32))
            .height(Length::Fill)
            .align_x(match side {
                Side::Right => Alignment::End,
                Side::Left => Alignment::Start,
            })
            .into()
    }

    /// One icon of the Icons or Grid menu: its icon in a disc, accent
    /// filled while a switch is on, the timer's time left as a badge, and
    /// in the grid a short name under it.
    fn compact_cell(&self, index: usize, item: &Item, focused: bool) -> Element<'_, Message> {
        let menu = &self.config.edge_menu;
        let p = self.config.surface_palette();
        let available = self.edge_available(index);
        let state = available.then(|| self.item_state(item)).flatten();
        let running = available
            && match item.as_action() {
                Some(Action::Timer) => self.activity.timer.as_ref().is_some_and(|t| t.running()),
                Some(Action::Focus) => self.tools.focus.running(),
                _ => false,
            };
        let lit = state == Some(true) || running;
        let size = menu.compact_icon();
        let disc = f32::from(size) + 14.;
        let ink = if lit {
            crate::widget_style::on_accent(p)
        } else if available {
            p.foreground
        } else {
            Color { a: 0.45, ..p.muted }
        };
        let (icon_name, symbolic) = item_icon(item);
        let icon = cosmic::widget::icon::from_name(icon_name.as_str()).size(size);
        let icon: Element<'_, Message> = if symbolic {
            icon.symbolic(true)
                .icon()
                .class(cosmic::theme::Svg::custom(move |_| {
                    cosmic::iced::widget::svg::Style { color: Some(ink) }
                }))
                .into()
        } else {
            icon.icon().into()
        };
        let switch = state.is_some();
        let accent = p.accent;
        let disc_fill = if lit {
            Some(accent)
        } else if symbolic && !switch {
            Some(mix(p.foreground, p.background, 0.08))
        } else {
            None
        };
        let disc_border = if switch && !lit {
            mix(p.foreground, p.background, 0.32)
        } else {
            Color::TRANSPARENT
        };
        let disc: Element<'_, Message> = container(icon)
            .width(disc)
            .height(disc)
            .center(disc)
            .class(cosmic::theme::Container::custom(move |_| {
                cosmic::iced::widget::container::Style {
                    background: disc_fill.map(Background::Color),
                    border: Border {
                        color: disc_border,
                        width: if disc_border == Color::TRANSPARENT {
                            0.
                        } else {
                            1.5
                        },
                        radius: (disc / 2.).into(),
                    },
                    ..Default::default()
                }
            }))
            .into();
        let square = cell(menu);
        let mut face: Element<'_, Message> = container(disc).center(square).into();
        if item.as_action() == Some(Action::Timer)
            && available
            && let Some(timer) = &self.activity.timer
        {
            let left = remaining(timer.remaining(chrono::Utc::now().timestamp_millis()));
            let paused = timer.paused();
            let badge = container(
                text::body(left)
                    .size(9.)
                    .font(cosmic::font::semibold())
                    .wrapping(cosmic::iced::advanced::text::Wrapping::None)
                    .class(cosmic::theme::Text::Color(if paused {
                        p.foreground
                    } else {
                        crate::widget_style::on_accent(p)
                    })),
            )
            .padding([1, 4])
            .class(cosmic::theme::Container::custom(move |_| {
                cosmic::iced::widget::container::Style {
                    background: Some(Background::Color(if paused {
                        mix(p.foreground, p.background, 0.2)
                    } else {
                        accent
                    })),
                    border: Border {
                        color: p.background,
                        width: 1.5,
                        radius: 7.into(),
                    },
                    ..Default::default()
                }
            }));
            face = cosmic::iced::widget::stack![
                face,
                container(badge)
                    .width(square)
                    .height(square)
                    .align_x(Alignment::Center)
                    .align_y(Alignment::End)
                    .padding([0, 0, 1, 0])
            ]
            .into();
        }
        let (title, caption) = self.item_label(index, item);
        let mut content: Vec<Element<'_, Message>> = vec![face];
        if labelled(menu) {
            content.push(
                container(
                    text::body(title.clone())
                        .size(10.5)
                        .line_height(1.2)
                        .font(crate::widget_style::font())
                        .wrapping(cosmic::iced::advanced::text::Wrapping::Word)
                        .align_x(cosmic::iced::alignment::Horizontal::Center)
                        .class(cosmic::theme::Text::Color(if available {
                            mix(p.foreground, p.muted, 0.25)
                        } else {
                            p.muted
                        })),
                )
                .width(Length::Fill)
                .height(LABEL)
                .padding([1, 2, 0, 2])
                .center_x(Length::Fill)
                .align_y(Alignment::Start)
                .clip(true)
                .into(),
            );
        }
        let spoken = match &caption {
            Some(state) => format!("{title}, {}", state.to_lowercase()),
            None => title,
        };
        let mut button = cosmic::widget::button::custom(
            column::with_children(content)
                .width(Length::Fill)
                .align_x(Alignment::Center),
        )
        .name(spoken)
        .id(cosmic::widget::Id::new(format!("edge-item-{index}")))
        .padding(0)
        .width(Length::Fixed(cell_width(menu)))
        .height(Length::Fixed(cell_height(menu)))
        .class(cell_style(p, focused, true));
        if available {
            button = button.on_press(Message::EdgeItem(index));
        }
        mouse_area(button)
            .on_enter(Message::EdgeTip(Some(DrawerItem::Entry(index))))
            .on_exit(Message::EdgeTip(None))
            .interaction(if available {
                mouse::Interaction::Pointer
            } else {
                mouse::Interaction::NotAllowed
            })
            .into()
    }
}

fn space<'a>(height: f32) -> Element<'a, Message> {
    cosmic::iced::widget::space().height(height).into()
}

/// A cell's look: clear at rest, a light accent wash under the pointer,
/// an accent ring while the keyboard is on it.
fn cell_style(p: crate::config::Palette, focused: bool, wash: bool) -> cosmic::theme::Button {
    let style = move |hovered: bool, pressed: bool| {
        let tint = if pressed {
            0.2
        } else if hovered && wash {
            0.12
        } else if hovered {
            0.08
        } else {
            0.
        };
        cosmic::widget::button::Style {
            background: (tint > 0.).then(|| {
                Background::Color(Color {
                    a: tint,
                    ..p.accent
                })
            }),
            border_radius: 12.0.into(),
            border_width: if focused { 2. } else { 0. },
            border_color: p.accent,
            outline_width: if focused { 3. } else { 0. },
            outline_color: Color {
                a: 0.25,
                ..p.accent
            },
            icon_color: Some(p.foreground),
            text_color: Some(p.foreground),
            ..Default::default()
        }
    };
    cosmic::theme::Button::Custom {
        active: Box::new(move |_, _| style(false, false)),
        hovered: Box::new(move |_, _| style(true, false)),
        pressed: Box::new(move |_, _| style(true, true)),
        disabled: Box::new(move |_| style(false, false)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::edge_menu::{IconSize, default_items};

    fn menu(style: Style) -> EdgeMenu {
        EdgeMenu {
            style,
            ..Default::default()
        }
    }

    #[test]
    fn the_width_follows_the_style_and_icons_is_narrow() {
        let icons = width(&menu(Style::Icons));
        assert!(icons < 80., "{icons}");
        let mut grid = menu(Style::Grid);
        let two = width(&grid);
        grid.columns = 3;
        let three = width(&grid);
        grid.columns = 4;
        let four = width(&grid);
        assert!(icons < two && two < three && three < four);
        assert!(four < drawer::SIDEBAR_WIDTH as f32 * 1.6);
        let mut large = menu(Style::Icons);
        large.icon_size = IconSize::Large;
        assert!(width(&large) > icons);
    }

    #[test]
    fn titles_become_gaps_or_headings_and_cells_fill_the_columns() {
        let items = default_items(&[Widget::Weather, Widget::Devbox, Widget::Projects]);
        let icons = Compact::new(&menu(Style::Icons), &items);
        assert!(!icons.lines.iter().any(|l| matches!(l, Line::Heading(_))));
        assert_eq!(icons.lines.first(), Some(&Line::Cells(vec![1])));
        assert!(icons.lines.contains(&Line::Gap(TITLE_GAP)));
        assert!(icons.lines.contains(&Line::Rule));
        let mut grid = menu(Style::Grid);
        grid.columns = 3;
        let laid = Compact::new(&grid, &items);
        assert_eq!(laid.lines[0], Line::Heading(0));
        assert_eq!(laid.lines[1], Line::Cells(vec![1, 2, 3]));
        grid.section_titles = false;
        let bare = Compact::new(&grid, &items);
        assert!(!bare.lines.iter().any(|l| matches!(l, Line::Heading(_))));
        // Every item that does something has a cell, once.
        let cells: Vec<usize> = laid
            .lines
            .iter()
            .flat_map(|l| match l {
                Line::Cells(c) => c.clone(),
                _ => Vec::new(),
            })
            .collect();
        let focusable: Vec<usize> = (0..items.len()).filter(|i| items[*i].focusable()).collect();
        assert_eq!(cells, focusable);
        // The height covers every line and grows with the icons.
        assert!(laid.height() > laid.body());
        let mut large = grid.clone();
        large.icon_size = IconSize::Large;
        assert!(Compact::new(&large, &items).height() > Compact::new(&grid, &items).height());
        assert!(laid.centre(1).unwrap() < laid.centre(5).unwrap());
        assert_eq!(laid.centre(1), laid.centre(3), "one row");
        assert_eq!(laid.centre(4), None, "a title has no cell");
    }

    #[test]
    fn time_left_reads_short() {
        assert_eq!(remaining(272_000), "4:32");
        assert_eq!(remaining(300_000), "5:00");
        assert_eq!(remaining(1), "0:01");
        assert_eq!(remaining(3_900_000), "1h05");
        assert_eq!(remaining(-5), "0:00");
    }
}
