//! Fonts and icons: installed icon themes as cards of real icons, the
//! "Get more icons" browser, the app icon shape and accent folders.
use super::desktop::{ERROR, banner, line};
use super::system_appearance::fill;
use super::*;
use crate::desktop::icon_packs::install::Progress;
use crate::desktop::icon_packs::ocs::{Pack, Sort};
use crate::desktop::icon_packs::overlay::Shape;
use crate::settings::icon_packs::{
    Context, IconPacks, Install, Load, Message as Edit, Notice, Thumb, shown_theme,
};
use cosmic::iced::advanced::text as advanced_text;
use cosmic::widget::image::Handle;

fn edit(message: Edit) -> Message {
    Message::IconPacks(message)
}

/// The narrowest a card gets before the grid drops a column.
const CARD_MIN: f32 = 208.;
const GAP: f32 = 12.;
/// Sample icons on a theme card.
const SAMPLE: u16 = 32;

/// Columns and card width for a grid `available` wide.
fn grid(available: f32, min: f32) -> (usize, f32) {
    let columns = ((available + GAP) / (min + GAP)).floor().max(1.);
    let width = ((available - GAP * (columns - 1.)) / columns).floor();
    (columns as usize, width)
}

fn wrap_grid<'a>(cards: Vec<Element<'a, Message>>) -> Element<'a, Message> {
    row::with_children(cards)
        .spacing(GAP)
        .wrap()
        .vertical_spacing(GAP)
        .into()
}

/// A card's surface: the theme's component color with a hairline, or an
/// accent ring when it is the one in use.
fn card<'a>(content: Element<'a, Message>, width: f32, selected: bool) -> Element<'a, Message> {
    container(content)
        .width(width)
        .padding(14)
        .class(cosmic::theme::Container::custom(move |theme| {
            let component = &theme.current_container().component;
            cosmic::iced::widget::container::Style {
                background: Some(Background::Color(component.base.into())),
                border: Border {
                    color: if selected {
                        theme.cosmic().accent.base.into()
                    } else {
                        component.divider.into()
                    },
                    width: if selected { 2. } else { 1. },
                    radius: theme.cosmic().corner_radii.radius_m.into(),
                },
                ..Default::default()
            }
        }))
        .into()
}

/// A gray block where something is still loading or missing.
fn placeholder<'a>(width: impl Into<Length>, height: f32, radius: f32) -> Element<'a, Message> {
    container(
        cosmic::iced::widget::space()
            .width(Length::Fill)
            .height(height),
    )
    .width(width)
    .height(height)
    .class(cosmic::theme::Container::custom(move |theme| {
        // A faint wash of the text color reads as a skeleton in both shades.
        let on: Color = theme.current_container().component.on.into();
        cosmic::iced::widget::container::Style {
            background: Some(Background::Color(Color { a: 0.09, ..on })),
            border: Border {
                radius: radius.into(),
                ..Default::default()
            },
            ..Default::default()
        }
    }))
    .into()
}

/// A thin bar `fraction` full; `None`, while the size is unknown, shows a
/// short stub.
fn progress_bar<'a>(fraction: Option<f32>) -> Element<'a, Message> {
    let filled = (fraction.unwrap_or(0.12).clamp(0., 1.) * 1000.).round() as u16;
    let bar = |accent: bool| {
        cosmic::theme::Container::custom(move |theme| {
            let color: Color = if accent {
                theme.cosmic().accent.base.into()
            } else {
                Color {
                    a: 0.14,
                    ..theme.current_container().component.on.into()
                }
            };
            cosmic::iced::widget::container::Style {
                background: Some(Background::Color(color)),
                border: Border {
                    radius: 3.into(),
                    ..Default::default()
                },
                ..Default::default()
            }
        })
    };
    let mut parts: Vec<Element<'a, Message>> = Vec::new();
    if filled > 0 {
        parts.push(
            container(cosmic::iced::widget::space().width(Length::Fill).height(6))
                .width(Length::FillPortion(filled))
                .height(6)
                .class(bar(true))
                .into(),
        );
    }
    if filled < 1000 {
        parts.push(
            cosmic::iced::widget::space()
                .width(Length::FillPortion(1000 - filled))
                .height(6)
                .into(),
        );
    }
    container(row::with_children(parts).width(Length::Fill))
        .width(Length::Fill)
        .height(6)
        .class(bar(false))
        .into()
}

fn megabytes(bytes: u64) -> String {
    let mb = bytes as f64 / 1_048_576.;
    if mb < 10. {
        format!("{mb:.1} MB")
    } else {
        format!("{mb:.0} MB")
    }
}

fn progress_text(progress: &Progress) -> String {
    match progress.total {
        Some(total) => format!(
            "Downloading {} of {}",
            megabytes(progress.done),
            megabytes(total)
        ),
        None => format!("Downloading {}", megabytes(progress.done)),
    }
}

/// A short paragraph cut to `lines`.
fn summary<'a>(value: impl Into<String>, lines: usize) -> Element<'a, Message> {
    text::body(value.into())
        .size(CAPTION)
        .line_height(1.3)
        .ellipsize(advanced_text::Ellipsize::End(
            advanced_text::EllipsizeHeightLimit::Lines(lines),
        ))
        .class(text_style(MUTED))
        .into()
}

/// Body text that is not a row title (search indexes those).
fn body<'a>(value: impl Into<String>) -> Element<'a, Message> {
    text::body(value.into())
        .size(BODY)
        .line_height(1.25)
        .class(text_style(FG))
        .into()
}

fn sample_icon<'a>(path: Option<&std::path::PathBuf>) -> Element<'a, Message> {
    let inner: Element<'a, Message> = match path {
        Some(path) => icon::from_path(path.clone()).icon().size(SAMPLE).into(),
        None => placeholder(
            Length::Fixed(f32::from(SAMPLE) - 6.),
            f32::from(SAMPLE) - 6.,
            8.,
        ),
    };
    container(inner)
        .width(Length::Fill)
        .height(f32::from(SAMPLE))
        .align_x(Alignment::Center)
        .align_y(Alignment::Center)
        .into()
}

fn picture<'a>(handle: &Handle, size: f32) -> Element<'a, Message> {
    cosmic::widget::image(handle.clone())
        .width(size)
        .height(size)
        .into()
}

impl Settings {
    /// The Icons section of Fonts and icons.
    pub(super) fn icon_section(&self) -> Element<'_, Message> {
        let packs = &self.icon_packs;
        let context = self.icon_context();
        let mut items: Vec<Element<'_, Message>> = Vec::new();
        items.push(
            column::with_children(vec![
                text::heading("Icons").into(),
                label(
                    "Pick a theme, shape app icons and color folders. Changes apply right away.",
                    CAPTION,
                    MUTED,
                ),
            ])
            .spacing(4)
            .into(),
        );
        if let Some(notice) = &packs.notice {
            let (message, color) = match notice {
                Notice::Done(message) => (message.clone(), HEALTHY),
                Notice::Error(message) => (message.clone(), ERROR),
            };
            items.push(banner(
                message,
                color,
                vec![button::text("Dismiss").on_press(edit(Edit::Dismiss)).into()],
            ));
        }
        if packs.paths.is_none() {
            items.push(label(
                "Icon themes can’t be listed: your home folder isn’t known.",
                CAPTION,
                ERROR,
            ));
            return column::with_children(items).spacing(14).into();
        }
        items.push(self.icon_theme_cards(packs, &context));
        if packs.browser.is_some() {
            items.push(self.icon_browser(packs, &context));
        }
        items.push(self.icon_shape(packs, &context));
        items.push(self.icon_folders(packs, &context));
        column::with_children(items).spacing(18).into()
    }

    fn icon_theme_cards<'a>(
        &'a self,
        packs: &'a IconPacks,
        context: &Context,
    ) -> Element<'a, Message> {
        let editor = &self.system_appearance;
        let writable = editor.writable();
        let shown = shown_theme(&packs.style, context.icon_theme.as_deref());
        let (_, width) = grid(self.content_width(), CARD_MIN);
        let mut cards: Vec<Element<'_, Message>> = Vec::new();
        let mut sparse: Vec<(String, String)> = Vec::new();
        for theme in &editor.icons.themes {
            if theme.id == "hicolor" || crate::desktop::icon_packs::overlay::is_overlay(&theme.id) {
                continue;
            }
            let samples = packs.scan.samples.get(&theme.id);
            let found = samples.map_or(0, |samples| samples.iter().flatten().count());
            // Cursor, branding and partial themes have few of the samples:
            // they are listed by name, not drawn half empty.
            if samples.is_some() && found < 3 && shown.as_deref() != Some(theme.id.as_str()) {
                sparse.push((theme.id.clone(), theme.name.clone()));
                continue;
            }
            let selected = shown.as_deref() == Some(theme.id.as_str());
            cards.push(self.theme_card(packs, theme, samples, selected, writable, width));
        }
        // The last card opens the browser.
        let browsing = packs.browser.is_some();
        let more = button::custom(
            column::with_children(vec![
                icon::from_name(if browsing {
                    "go-down-symbolic"
                } else {
                    "list-add-symbolic"
                })
                .size(24)
                .icon()
                .into(),
                label("Get more icons", BODY, FG),
                summary(
                    if browsing {
                        "The catalog is open below"
                    } else {
                        "Browse free icon themes and install them in a click"
                    },
                    2,
                ),
            ])
            .spacing(8)
            .align_x(Alignment::Center)
            .width(Length::Fill),
        )
        .id(cosmic::widget::Id::new("icons-get-more"))
        .name("Get more icons")
        .class(nav_style(browsing, self.settings_accent()))
        .padding([28, 16])
        .width(width)
        .height(Length::Shrink)
        .on_press(edit(Edit::Browse(!browsing)));
        cards.push(
            fill(more.into(), Color::TRANSPARENT, LINE, 1., 12.)
                .width(width)
                .into(),
        );
        let mut rows = vec![wrap_grid(cards)];
        if !sparse.is_empty() {
            let chips: Vec<Element<'_, Message>> = sparse
                .into_iter()
                .map(|(id, name)| {
                    button::custom(label(name.clone(), CAPTION, MUTED))
                        .class(chip_style(false, self.settings_accent()))
                        .name(format!("Use {name} icons"))
                        .padding([5, 10])
                        .on_press_maybe(writable.then(|| edit(Edit::Use(id))))
                        .into()
                })
                .collect();
            let mut line_items = vec![label("Other themes", CAPTION, MUTED)];
            line_items.extend(chips);
            rows.push(
                row::with_children(line_items)
                    .spacing(6)
                    .align_y(Alignment::Center)
                    .wrap()
                    .into(),
            );
        }
        container(column::with_children(rows).spacing(10))
            .id(cosmic::widget::Id::new("icon-theme-cards"))
            .into()
    }

    fn theme_card<'a>(
        &'a self,
        packs: &'a IconPacks,
        theme: &'a crate::desktop::appearance::catalog::IconTheme,
        samples: Option<&'a Vec<Option<std::path::PathBuf>>>,
        selected: bool,
        writable: bool,
        width: f32,
    ) -> Element<'a, Message> {
        let accent = self.settings_accent();
        let grid_rows: Vec<Element<'a, Message>> = (0..2)
            .map(|half| {
                let icons = (0..4)
                    .map(|i| {
                        let index = half * 4 + i;
                        match samples {
                            Some(samples) => {
                                sample_icon(samples.get(index).and_then(Option::as_ref))
                            }
                            // Still scanning.
                            None => container(placeholder(
                                Length::Fixed(f32::from(SAMPLE) - 6.),
                                f32::from(SAMPLE) - 6.,
                                8.,
                            ))
                            .width(Length::Fill)
                            .align_x(Alignment::Center)
                            .into(),
                        }
                    })
                    .collect::<Vec<_>>();
                cosmic::widget::row::with_children(icons)
                    .spacing(6)
                    .width(Length::Fill)
                    .into()
            })
            .collect();
        let tray = fill(
            column::with_children(grid_rows).spacing(10).into(),
            BASE,
            Color::TRANSPARENT,
            0.,
            10.,
        )
        .padding([12, 8])
        .width(Length::Fill);
        let installed = packs.manifest.get(&theme.id);
        let origin = match installed {
            Some(entry) => format!("Installed by Halcyon from {}", entry.pack),
            None if theme.path.starts_with(dirs::home_dir().unwrap_or_default()) => {
                "In your icons folder".to_string()
            }
            None => "Comes with the system".to_string(),
        };
        let face = column::with_children(vec![
            tray.into(),
            column::with_children(vec![
                line(theme.name.clone(), BODY, FG),
                line(origin, CAPTION, MUTED),
            ])
            .spacing(2)
            .into(),
        ])
        .spacing(10);
        let pick = button::custom(face)
            .id(cosmic::widget::Id::new(format!("icon-theme-{}", theme.id)))
            .name(if selected {
                format!("{} icons, in use", theme.name)
            } else {
                format!("Use {} icons", theme.name)
            })
            .class(nav_style(false, accent))
            .selected(selected)
            .padding(0)
            .width(Length::Fill)
            .on_press_maybe((writable && !selected).then(|| edit(Edit::Use(theme.id.clone()))));
        let confirming = installed.is_some() && packs.confirm.as_deref() == Some(theme.id.as_str());
        let mut footer: Vec<Element<'a, Message>> = Vec::new();
        // While a remove is being confirmed its two buttons get the room.
        if selected {
            footer.push(pill("In use", accent));
        } else if !confirming {
            footer.push(label("Choose to use", CAPTION, MUTED));
        }
        footer.push(cosmic::widget::space().width(Length::Fill).into());
        if installed.is_some() {
            if confirming {
                footer.push(
                    button::text("Cancel")
                        .on_press(edit(Edit::ConfirmRemove(None)))
                        .into(),
                );
                footer.push(
                    button::destructive("Remove")
                        .on_press(edit(Edit::Remove(theme.id.clone())))
                        .into(),
                );
            } else {
                footer.push(
                    button::text("Remove…")
                        .on_press_maybe(
                            (!selected).then(|| edit(Edit::ConfirmRemove(Some(theme.id.clone())))),
                        )
                        .into(),
                );
            }
        }
        card(
            column::with_children(vec![
                pick.into(),
                row::with_children(footer)
                    .spacing(6)
                    .align_y(Alignment::Center)
                    .height(32)
                    .into(),
            ])
            .spacing(8)
            .into(),
            width,
            selected,
        )
    }

    fn icon_browser<'a>(&'a self, packs: &'a IconPacks, context: &Context) -> Element<'a, Message> {
        let Some(browser) = &packs.browser else {
            return column::with_children(Vec::new()).into();
        };
        let accent = self.settings_accent();
        let available = self.content_width() - 32.;
        let mut items: Vec<Element<'_, Message>> = Vec::new();
        items.push(split(
            column::with_children(vec![
                label("Get more icons", BODY, FG),
                label(
                    "Free themes from the Pling catalog. They install to your icons folder, and Halcyon only ever removes what it installed.",
                    CAPTION,
                    MUTED,
                ),
            ])
            .spacing(3)
            .into(),
            button::text("Close")
                .on_press(edit(Edit::Browse(false)))
                .into(),
        ));
        // Popular: the curated row.
        let curated_cards: Vec<Element<'_, Message>> = match &browser.curated_state {
            Load::Ready if !browser.curated.is_empty() => {
                let (columns, width) = grid(available, 168.);
                let width = if browser.curated.len() <= columns {
                    width.min(220.)
                } else {
                    width
                };
                browser
                    .curated
                    .iter()
                    .map(|pack| self.curated_card(packs, pack, context, width))
                    .collect()
            }
            Load::Loading => {
                let (_, width) = grid(available, 168.);
                (0..6.min(grid(available, 168.).0.max(3)))
                    .map(|_| {
                        card(
                            column::with_children(vec![
                                placeholder(Length::Fill, width * 0.5, 8.),
                                placeholder(Length::Fixed(width * 0.5), 12., 4.),
                            ])
                            .spacing(10)
                            .into(),
                            width,
                            false,
                        )
                    })
                    .collect()
            }
            _ => Vec::new(),
        };
        if !curated_cards.is_empty() {
            items.push(label("Popular", CAPTION, MUTED));
            items.push(wrap_grid(curated_cards));
        }
        // Search and sort.
        let search = text_input::search_input("Search icon themes", &browser.query)
            .id(cosmic::widget::Id::new("icon-packs-search"))
            .on_input(|text| edit(Edit::Query(text)))
            .on_submit(|_| edit(Edit::Search))
            .width(Length::Fill);
        let sort = segmented(
            Sort::ALL
                .into_iter()
                .map(|sort| {
                    (
                        sort.title().to_string(),
                        browser.sort == sort,
                        Some(edit(Edit::Sort(sort))),
                    )
                })
                .collect(),
            accent,
        );
        items.push(
            row::with_children(vec![
                container(search).width(Length::Fill).max_width(420).into(),
                sort,
            ])
            .spacing(12)
            .align_y(Alignment::Center)
            .wrap()
            .vertical_spacing(8)
            .into(),
        );
        let (columns, width) = grid(available, CARD_MIN);
        match &browser.state {
            Load::Loading => {
                let skeletons = (0..columns.max(2) * 2)
                    .map(|_| self.pack_skeleton(width))
                    .collect();
                items.push(wrap_grid(skeletons));
                items.push(label("Loading the catalog…", CAPTION, MUTED));
            }
            Load::Failed(error) => {
                items.push(
                    fill(
                        column::with_children(vec![
                            icon::from_name("network-offline-symbolic")
                                .size(32)
                                .icon()
                                .into(),
                            body("The catalog didn’t load"),
                            label(error.clone(), CAPTION, MUTED),
                            button::standard("Retry")
                                .id(cosmic::widget::Id::new("icon-packs-retry"))
                                .on_press(edit(Edit::Retry))
                                .into(),
                        ])
                        .spacing(8)
                        .align_x(Alignment::Center)
                        .width(Length::Fill)
                        .into(),
                        BASE,
                        Color::TRANSPARENT,
                        0.,
                        12.,
                    )
                    .padding(28)
                    .width(Length::Fill)
                    .into(),
                );
            }
            Load::Ready if browser.packs.is_empty() => {
                let message = if browser.search.is_empty() {
                    "The catalog has no icon themes right now.".to_string()
                } else {
                    format!("No icon themes match “{}”.", browser.search)
                };
                items.push(
                    fill(
                        column::with_children(vec![
                            icon::from_name("system-search-symbolic")
                                .size(32)
                                .icon()
                                .into(),
                            body(message),
                            label("Try a shorter word, or another spelling.", CAPTION, MUTED),
                        ])
                        .spacing(8)
                        .align_x(Alignment::Center)
                        .width(Length::Fill)
                        .into(),
                        BASE,
                        Color::TRANSPARENT,
                        0.,
                        12.,
                    )
                    .padding(28)
                    .width(Length::Fill)
                    .into(),
                );
            }
            Load::Ready => {
                let cards = browser
                    .packs
                    .iter()
                    .map(|pack| self.pack_card(packs, pack, context, width))
                    .collect();
                items.push(wrap_grid(cards));
                let shown = browser.packs.len();
                let mut footer = vec![label(
                    format!("Showing {shown} of {}", browser.total.max(shown as u32)),
                    CAPTION,
                    MUTED,
                )];
                footer.push(cosmic::widget::space().width(Length::Fill).into());
                if browser.has_more() {
                    footer.push(
                        button::standard(if browser.more {
                            "Loading…"
                        } else {
                            "Load more"
                        })
                        .id(cosmic::widget::Id::new("icon-packs-more"))
                        .on_press_maybe((!browser.more).then(|| edit(Edit::More)))
                        .into(),
                    );
                }
                items.push(row::with_children(footer).align_y(Alignment::Center).into());
            }
        }
        container(
            fill(
                column::with_children(items).spacing(14).into(),
                SURFACE,
                LINE,
                1.,
                14.,
            )
            .padding(16)
            .width(Length::Fill),
        )
        .id(cosmic::widget::Id::new("icon-packs-browser"))
        .into()
    }

    fn pack_skeleton<'a>(&self, width: f32) -> Element<'a, Message> {
        card(
            column::with_children(vec![
                placeholder(Length::Fill, (width - 28.) * 0.56, 10.),
                placeholder(Length::Fixed((width - 28.) * 0.6), 14., 4.),
                placeholder(Length::Fixed((width - 28.) * 0.4), 10., 4.),
                placeholder(Length::Fill, 10., 4.),
                placeholder(Length::Fixed(88.), 30., 15.),
            ])
            .spacing(10)
            .into(),
            width,
            false,
        )
    }

    fn thumbnail<'a>(&self, packs: &IconPacks, pack: &Pack, height: f32) -> Element<'a, Message> {
        match pack.preview.as_ref().and_then(|url| packs.thumbs.get(url)) {
            Some(Thumb::Ready(path)) => cosmic::widget::image(Handle::from_path(path))
                .width(Length::Fill)
                .height(height)
                .content_fit(cosmic::iced::ContentFit::Cover)
                .border_radius([10.; 4])
                .into(),
            Some(Thumb::Loading) => placeholder(Length::Fill, height, 10.),
            _ => fill(
                container(icon::from_name("image-missing-symbolic").size(28).icon())
                    .width(Length::Fill)
                    .height(height)
                    .align_x(Alignment::Center)
                    .align_y(Alignment::Center)
                    .into(),
                BASE,
                Color::TRANSPARENT,
                0.,
                10.,
            )
            .width(Length::Fill)
            .into(),
        }
    }

    /// The install control: Install, progress, then Apply or In use.
    fn install_control<'a>(
        &self,
        packs: &IconPacks,
        pack: &Pack,
        context: &Context,
        compact: bool,
    ) -> Element<'a, Message> {
        let writable = self.system_appearance.writable();
        let name = pack.name.clone();
        let themes = packs.pack_themes(pack.id);
        let shown = shown_theme(&packs.style, context.icon_theme.as_deref());
        // A pack can be in the Popular row and the grid at once; ids must differ.
        let id = cosmic::widget::Id::new(if compact {
            format!("icon-pack-popular-{}", pack.id)
        } else {
            format!("icon-pack-install-{}", pack.id)
        });
        let state = packs.installs.get(&pack.id);
        let working = |status: String, fraction: Option<f32>| -> Element<'a, Message> {
            column::with_children(vec![line(status, CAPTION, MUTED), progress_bar(fraction)])
                .spacing(6)
                .width(Length::Fill)
                .into()
        };
        match state {
            Some(Install::Preparing) => working("Preparing…".into(), None),
            Some(Install::Downloading(progress)) => working(
                progress_text(progress),
                progress
                    .total
                    .filter(|total| *total > 0)
                    .map(|total| progress.done as f32 / total as f32),
            ),
            Some(Install::Unpacking) => working("Installing…".into(), Some(1.)),
            _ if !themes.is_empty() => {
                if shown.as_ref().is_some_and(|shown| themes.contains(shown)) {
                    row::with_children(vec![
                        icon::from_name("object-select-symbolic")
                            .size(16)
                            .icon()
                            .into(),
                        label("In use", CAPTION, FG),
                    ])
                    .spacing(6)
                    .align_y(Alignment::Center)
                    .height(32)
                    .into()
                } else {
                    let mut parts: Vec<Element<'a, Message>> = Vec::new();
                    if !compact {
                        parts.push(label("Installed", CAPTION, MUTED));
                        parts.push(cosmic::widget::space().width(Length::Fill).into());
                    }
                    parts.push(
                        button::suggested("Apply")
                            .id(id)
                            .on_press_maybe(writable.then(|| edit(Edit::Apply(pack.id))))
                            .into(),
                    );
                    row::with_children(parts)
                        .spacing(8)
                        .align_y(Alignment::Center)
                        .into()
                }
            }
            failed => {
                let mut parts: Vec<Element<'a, Message>> = Vec::new();
                if let Some(Install::Failed(error)) = failed {
                    parts.push(summary(format!("Couldn’t install: {error}"), 2));
                }
                let usable = !pack.files.is_empty();
                parts.push(
                    button::standard(if matches!(failed, Some(Install::Failed(_))) {
                        "Try again"
                    } else {
                        "Install"
                    })
                    .id(id)
                    .on_press_maybe(usable.then(|| edit(Edit::Install(pack.id))))
                    .into(),
                );
                if !usable {
                    parts.push(label("No download Halcyon can open", CAPTION, MUTED));
                }
                let _ = name;
                column::with_children(parts).spacing(6).into()
            }
        }
    }

    fn pack_card<'a>(
        &'a self,
        packs: &'a IconPacks,
        pack: &'a Pack,
        context: &Context,
        width: f32,
    ) -> Element<'a, Message> {
        let inner = width - 28.;
        let mut facts = vec![pack.downloads_title()];
        if pack.score > 0 {
            facts.push(pack.rating_title());
        }
        let installed = !packs.pack_themes(pack.id).is_empty();
        card(
            column::with_children(vec![
                self.thumbnail(packs, pack, (inner * 0.56).round()),
                column::with_children(vec![
                    line(pack.name.clone(), BODY, FG),
                    line(format!("by {}", pack.author), CAPTION, MUTED),
                ])
                .spacing(2)
                .into(),
                container(summary(pack.summary.clone(), 2))
                    .height(34)
                    .into(),
                line(facts.join(" · "), CAPTION, MUTED),
                self.install_control(packs, pack, context, false),
            ])
            .spacing(10)
            .into(),
            width,
            installed,
        )
    }

    fn curated_card<'a>(
        &'a self,
        packs: &'a IconPacks,
        pack: &'a Pack,
        context: &Context,
        width: f32,
    ) -> Element<'a, Message> {
        let inner = width - 28.;
        card(
            column::with_children(vec![
                self.thumbnail(packs, pack, (inner * 0.5).round()),
                line(pack.name.clone(), BODY, FG),
                self.install_control(packs, pack, context, true),
            ])
            .spacing(8)
            .into(),
            width,
            !packs.pack_themes(pack.id).is_empty(),
        )
    }

    fn icon_shape<'a>(&'a self, packs: &'a IconPacks, context: &Context) -> Element<'a, Message> {
        let accent = self.settings_accent();
        let style = packs.effective(context);
        let writable = self.system_appearance.writable();
        let previews = &packs.previews;
        let row_of = |shape: Shape| -> Option<&Vec<Handle>> {
            previews
                .shapes
                .iter()
                .find(|(each, _)| *each == shape)
                .map(|(_, row)| row)
        };
        // One tile per shape, drawn with a real app icon.
        let tiles: Vec<Element<'_, Message>> = Shape::ALL
            .into_iter()
            .map(|shape| {
                let selected = style.shape == shape;
                let sample: Element<'_, Message> = match row_of(shape).and_then(|row| row.first()) {
                    Some(handle) => picture(handle, 44.),
                    None => placeholder(Length::Fixed(44.), 44., 12.),
                };
                button::custom(
                    column::with_children(vec![
                        sample,
                        label(shape.title(), CAPTION, if selected { FG } else { MUTED }),
                    ])
                    .spacing(8)
                    .align_x(Alignment::Center)
                    .width(Length::Fill),
                )
                .id(cosmic::widget::Id::new(format!(
                    "icon-shape-{}",
                    shape.key()
                )))
                .name(format!("{} app icons", shape.title()))
                .class(nav_style(selected, accent))
                .selected(selected)
                .padding([12, 8])
                .width(104)
                .on_press_maybe(writable.then(|| edit(Edit::Shape(shape))))
                .into()
            })
            .collect();
        // The dock-like strip of the chosen shape.
        let strip: Element<'_, Message> = match row_of(style.shape) {
            Some(row) if !row.is_empty() => row::with_children(
                row.iter()
                    .map(|handle| picture(handle, 52.))
                    .collect::<Vec<_>>(),
            )
            .spacing(14)
            .align_y(Alignment::Center)
            .into(),
            _ => row::with_children(
                (0..crate::settings::icon_packs::PREVIEW_COUNT)
                    .map(|_| placeholder(Length::Fixed(52.), 52., 14.))
                    .collect::<Vec<_>>(),
            )
            .spacing(14)
            .into(),
        };
        let strip = fill(
            container(strip)
                .width(Length::Fill)
                .align_x(Alignment::Center)
                .into(),
            BASE,
            Color::TRANSPARENT,
            0.,
            18.,
        )
        .padding([16, 18])
        .width(Length::Fill);
        let state = match packs.build {
            Some(build) if build.total > 0 => format!(
                "Adjusting icons… {} of {}",
                build.done.min(build.total),
                build.total
            ),
            Some(_) => "Adjusting icons…".to_string(),
            None if style.shape == Shape::Original => {
                "App icons look as their theme draws them.".to_string()
            }
            None => format!(
                "App icons are {} on {}.",
                match style.shape {
                    Shape::Circle => "circles",
                    Shape::Squircle => "squircles",
                    _ => "rounded squares",
                },
                self.theme_name(&packs.base(context)),
            ),
        };
        let mut rows = vec![
            detail_state("App icon shape", state),
            row::with_children(tiles)
                .spacing(8)
                .wrap()
                .vertical_spacing(8)
                .into(),
        ];
        if let Some(build) = packs.build {
            rows.push(progress_bar(
                (build.total > 0).then(|| build.done as f32 / build.total as f32),
            ));
        }
        rows.push(strip.into());
        container(
            fill(
                column::with_children(rows).spacing(12).into(),
                SURFACE,
                LINE,
                1.,
                14.,
            )
            .padding(16),
        )
        .id(cosmic::widget::Id::new("icon-shape"))
        .into()
    }

    fn icon_folders<'a>(&'a self, packs: &'a IconPacks, context: &Context) -> Element<'a, Message> {
        let style = packs.effective(context);
        let writable = self.system_appearance.writable();
        let preview: Element<'_, Message> = match &packs.previews.folder {
            Some((plain, tinted)) => row::with_children(vec![
                container(picture(plain, 40.)).padding(4).into(),
                icon::from_name("go-next-symbolic").size(14).icon().into(),
                fill(picture(tinted, 40.), Color::TRANSPARENT, LINE, 0., 10.)
                    .padding(4)
                    .into(),
            ])
            .spacing(6)
            .align_y(Alignment::Center)
            .into(),
            None => placeholder(Length::Fixed(40.), 40., 10.),
        };
        self.folder_row(style.tint, writable, preview)
    }

    fn folder_row<'a>(
        &self,
        on: bool,
        writable: bool,
        preview: Element<'a, Message>,
    ) -> Element<'a, Message> {
        let title = "Folders use the accent color";
        let toggle = toggler(on).on_toggle_maybe(writable.then_some(|on| edit(Edit::Tint(on))));
        let row_items = row::with_children(vec![
            preview,
            container(detail_state(
                title,
                if on {
                    "Folders in Files and on the desktop follow your accent."
                } else {
                    "Folders keep their theme’s color."
                },
            ))
            .width(Length::Fill)
            .into(),
            focus::control(
                Some(title.to_string()),
                Some(on),
                container(toggle)
                    .id(cosmic::widget::Id::new("icon-folders-toggle"))
                    .into(),
            ),
        ])
        .spacing(14)
        .align_y(Alignment::Center);
        container(
            fill(row_items.into(), SURFACE, LINE, 1., 14.)
                .padding([12, 16])
                .width(Length::Fill),
        )
        .id(cosmic::widget::Id::new("icon-folders"))
        .into()
    }

    fn theme_name(&self, id: &str) -> String {
        self.system_appearance
            .icons
            .get(id)
            .map_or_else(|| id.to_string(), |theme| theme.name.clone())
    }
}
