//! Bounded native type specimens. One page retains twelve choices, sharing the
//! current wallpaper handle; browsing never edits preferences or starts data.
use super::pages::section;
use super::*;
use crate::settings::FontTarget;

/// Common, readable families offered first when they are installed.
const SUGGESTED: [&str; 15] = [
    "Inter",
    "Fira Sans",
    "Open Sans",
    "Noto Sans",
    "Ubuntu",
    "Roboto",
    "Lato",
    "Source Sans 3",
    "Noto Serif",
    "Merriweather",
    "Lora",
    "Fira Mono",
    "JetBrains Mono",
    "Noto Sans Mono",
    "Hack",
];

fn in_font<'a>(name: &str, size: f32, color: Color) -> Element<'a, Message> {
    text::body(name.to_string())
        .size(size)
        .font(cosmic::iced::Font {
            family: cosmic::iced::font::Family::Name(crate::widget_style::intern(name)),
            ..cosmic::iced::Font::DEFAULT
        })
        .class(cosmic::theme::Text::Color(color))
        .into()
}

impl Settings {
    /// Installed families, suggested ones first, matching the search.
    pub(in crate::settings) fn widget_font_families(&self) -> Vec<String> {
        let Some(fonts) = &self.font_catalog else {
            return Vec::new();
        };
        let mut all: Vec<String> = fonts
            .families(false)
            .iter()
            .chain(fonts.families(true))
            .map(|family| family.name.clone())
            .collect();
        all.sort_by_key(|name| name.to_lowercase());
        all.dedup();
        let query = self.font_search.trim().to_lowercase();
        let mut ordered: Vec<String> = SUGGESTED
            .iter()
            .filter(|name| all.iter().any(|family| family == *name))
            .map(|name| name.to_string())
            .collect();
        ordered.extend(
            all.into_iter()
                .filter(|name| !SUGGESTED.contains(&name.as_str())),
        );
        ordered.retain(|name| query.is_empty() || name.to_lowercase().contains(&query));
        ordered
    }

    /// At a glance's own fonts; the defaults are the widget fonts.
    pub(super) fn glance_text_section(&self) -> Element<'_, Message> {
        let g = &self.config.glance;
        section(vec![
            label("Text", 14., FG),
            self.widget_font_row(
                FontTarget::GlanceText,
                "Font",
                g.font.as_deref(),
                "Same as widgets",
            ),
            separator(),
            self.widget_font_row(
                FontTarget::GlanceClock,
                "Clock font",
                g.clock_font.as_deref(),
                "Same as text",
            ),
        ])
    }

    pub(super) fn widget_font_row(
        &self,
        target: FontTarget,
        title: &'static str,
        current: Option<&str>,
        default: &'static str,
    ) -> Element<'_, Message> {
        let accent = self.settings_accent();
        let open = self.font_menu == Some(target);
        let key = match target {
            FontTarget::Text => "text",
            FontTarget::Clock => "clock",
            FontTarget::GlanceText => "glance-text",
            FontTarget::GlanceClock => "glance-clock",
        };
        let shown: Element<'_, Message> = match current {
            Some(name) => in_font(name, 14., FG),
            None => label(default, 14., FG),
        };
        let mut rows = vec![split(
            label(title, 14., FG),
            // The same chooser as the Fonts page: fixed width, value left,
            // chevron at the right edge.
            container(button::custom(
                row::with_children(vec![
                    container(shown).width(Length::Fill).into(),
                    icon::from_name(if open {
                        "go-up-symbolic"
                    } else {
                        "go-down-symbolic"
                    })
                    .size(14)
                    .icon()
                    .into(),
                ])
                .spacing(10)
                .align_y(Alignment::Center),
            )
            .id(cosmic::widget::Id::new(format!("font-{key}")))
            .class(chip_style(open, accent))
            .padding([6, 12])
            .width(Length::Fill)
            .on_press(Message::FontMenu(Some(target))))
            .width(Length::Fixed(260.))
            .into(),
        )];
        if !open {
            return column::with_children(rows).into();
        }
        let choice = |value: Option<String>, face: Element<'static, Message>, selected: bool| {
            button::custom(container(face).width(Length::Fill))
                .class(chip_style(selected, accent))
                .padding([6, 10])
                .width(Length::Fill)
                .on_press(Message::WidgetFont(target, value))
                .into()
        };
        rows.push(choice(None, label(default, 14., MUTED), current.is_none()));
        let families = self.widget_font_families();
        let total = families.len();
        let page = self.font_page.min(total.saturating_sub(1) / FONT_PAGE_SIZE);
        let start = page * FONT_PAGE_SIZE;
        let shown = families
            .into_iter()
            .skip(start)
            .take(FONT_PAGE_SIZE)
            .collect::<Vec<_>>();
        rows.push(
            container(
                text_input("Search fonts", &self.font_search)
                    .on_input(Message::FontSearch)
                    .width(Length::Fill),
            )
            .id(cosmic::widget::Id::new(format!("font-search-{key}")))
            .into(),
        );
        rows.push(label(
            if self.canvas.is_some() && self.page == Page::Overview {
                "Sample text · previewing until Apply"
            } else {
                "Sample text · choosing a font applies it immediately"
            },
            11.,
            MUTED,
        ));
        let selected_name = current.map(str::to_owned);
        let grid = cosmic::widget::responsive(move |bounds| {
            let columns = if bounds.width >= 660. {
                3.
            } else if bounds.width >= 360. {
                2.
            } else {
                1.
            };
            let width = ((bounds.width - (columns - 1.) * 8.) / columns).max(1.);
            let tiles = shown
                .iter()
                .map(|name| {
                    let specimen = self.font_specimen(target, name, [width - 16., 96.]);
                    button::custom(
                        column::with_children(vec![specimen, label(name.clone(), 12., FG)])
                            .spacing(6),
                    )
                    .id(cosmic::widget::Id::new(format!("font-choice-{key}-{name}")))
                    .class(super::profiles::tile_style(
                        selected_name.as_deref() == Some(name.as_str()),
                        accent,
                    ))
                    .padding(7)
                    .width(width)
                    .on_press(Message::WidgetFont(target, Some(name.clone())))
                    .into()
                })
                .collect::<Vec<_>>();
            let height = ((shown.len() as f32 / columns).ceil() * 142.).clamp(1., 300.);
            scrollable(row::with_children(tiles).spacing(8).wrap())
                .id(cosmic::widget::Id::new(format!("font-scroll-{key}")))
                .height(height)
                .into()
        })
        .height(Length::Shrink);
        rows.push(
            container(grid)
                .id(cosmic::widget::Id::new(format!("font-list-{key}")))
                .into(),
        );
        if total > FONT_PAGE_SIZE {
            rows.push(
                row::with_children(vec![
                    button::text("Previous")
                        .id(cosmic::widget::Id::new(format!("font-previous-{key}")))
                        .on_press_maybe((page > 0).then(|| Message::FontPage(page - 1)))
                        .into(),
                    label(
                        format!(
                            "{}–{} of {total}",
                            start + 1,
                            (start + FONT_PAGE_SIZE).min(total)
                        ),
                        12.,
                        MUTED,
                    ),
                    button::text("Next")
                        .id(cosmic::widget::Id::new(format!("font-next-{key}")))
                        .on_press_maybe(
                            (start + FONT_PAGE_SIZE < total).then_some(Message::FontPage(page + 1)),
                        )
                        .into(),
                ])
                .align_y(Alignment::Center)
                .spacing(12)
                .into(),
            );
        } else if total == 0 {
            rows.push(label(
                if self.font_loading {
                    "Loading installed fonts…"
                } else if self.font_search.trim().is_empty() {
                    "No installed fonts were found."
                } else {
                    "No fonts match."
                },
                12.,
                MUTED,
            ));
        }
        column::with_children(rows).spacing(8).into()
    }

    fn font_specimen(
        &self,
        target: FontTarget,
        name: &str,
        size: [f32; 2],
    ) -> Element<'static, Message> {
        use crate::widget_style::resolver::{ClockFace, ClockSurface};
        let mut config = self.canvas_config().clone();
        let clock = matches!(target, FontTarget::Clock | FontTarget::GlanceClock);
        let ink = config.wallpaper_palette().foreground;
        let content = if clock {
            config.clock_font = Some(name.into());
            config.clock.face = Some(ClockFace::Digital);
            config.clock.surface = Some(ClockSurface::None);
            config.clock.scale = 100;
            config.clock.show_date = false;
            config.clock.alignment = crate::config::ClockAlignment::Center;
            config.clock.center_vertically = true;
            crate::app::settings_clock_preview(&config, size).map(|_| Message::Preview)
        } else {
            container(
                text::body("Make room\nfor your ideas")
                    .size(24)
                    .font(cosmic::iced::Font {
                        family: cosmic::iced::font::Family::Name(crate::widget_style::intern(name)),
                        ..cosmic::iced::Font::DEFAULT
                    })
                    .class(cosmic::theme::Text::Color(ink)),
            )
            .center_x(size[0])
            .center_y(size[1])
            .into()
        };
        let background: Element<'static, Message> = match self.profiles.home_image() {
            Some(handle) => cosmic::widget::image(handle.clone())
                .width(size[0])
                .height(size[1])
                .content_fit(cosmic::iced::ContentFit::Cover)
                .into(),
            None => {
                let color = config.palette().background;
                container(cosmic::iced::widget::space())
                    .width(size[0])
                    .height(size[1])
                    .class(cosmic::theme::Container::custom(move |_| {
                        cosmic::iced::widget::container::Style {
                            background: Some(color.into()),
                            ..Default::default()
                        }
                    }))
                    .into()
            }
        };
        cosmic::iced::widget::stack(vec![background, content])
            .width(size[0])
            .height(size[1])
            .into()
    }
}
