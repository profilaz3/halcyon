//! Local wallpaper candidates, explicit display scope and a cached scene.
//! The existing editor applies the wallpaper and linked colors together.
use super::desktop::{ERROR, WARN, banner, conflict_banner, kept, line};
use super::pages::section;
use super::system_appearance::fill;
use super::windows::{recovery_banner, store_pill};
use super::*;
use crate::desktop::panels::Setting;
use crate::desktop::wallpaper::{
    BgEntry, BgSource, Displays, ROTATIONS, SamplingMethod, ScalingMode, Target,
};
use crate::settings::desktop::Notice;
use crate::settings::wallpaper::{
    Custom, FitChoice, Message as Edit, Pick, Tab, Tile, display_name, hex,
};

fn edit(message: Edit) -> Message {
    Message::Wallpaper(message)
}

const TILE_W: f32 = 136.;
const TILE_H: f32 = 77.;

fn every_label(seconds: u64) -> String {
    match seconds {
        0 => "Paused".into(),
        seconds if seconds % 3600 == 0 => match seconds / 3600 {
            1 => "1 hour".into(),
            hours => format!("{hours} hours"),
        },
        seconds if seconds % 60 == 0 => format!("{} min", seconds / 60),
        seconds => format!("{seconds} s"),
    }
}

/// A tile's picture: thumbnail, rendered colour, or a quiet placeholder.
fn face<'a>(
    handle: Option<&cosmic::widget::image::Handle>,
    placeholder: Element<'a, Message>,
) -> Element<'a, Message> {
    match handle {
        Some(handle) => cosmic::widget::image(handle.clone())
            .width(TILE_W)
            .height(TILE_H)
            .content_fit(cosmic::iced::ContentFit::Cover)
            .border_radius(10.)
            .into(),
        None => fill(
            container(placeholder)
                .width(Length::Fill)
                .height(Length::Fill)
                .center_x(Length::Fill)
                .center_y(Length::Fill)
                .into(),
            BASE,
            LINE,
            1.,
            10.,
        )
        .width(TILE_W)
        .height(TILE_H)
        .into(),
    }
}

/// The selection ring (accent) and current-background dot around a face.
fn framed<'a>(
    face: Element<'a, Message>,
    selected: bool,
    current: bool,
    missing: bool,
    accent: Color,
) -> Element<'a, Message> {
    let ring = fill(
        face,
        Color::TRANSPARENT,
        if selected { accent } else { Color::TRANSPARENT },
        2.,
        12.,
    )
    .padding(2);
    let mut layers: Vec<Element<'a, Message>> = vec![ring.into()];
    if current {
        layers.push(
            container(fill(
                cosmic::iced::widget::space().width(10).height(10).into(),
                accent,
                Color { a: 0.9, ..FG },
                1.,
                5.,
            ))
            .width(Length::Fill)
            .align_right(Length::Fill)
            .padding([8, 8])
            .into(),
        );
    }
    if missing {
        layers.push(
            container(pill("Missing", WARN))
                .width(Length::Fill)
                .height(Length::Fill)
                .align_bottom(Length::Fill)
                .padding([6, 8])
                .into(),
        );
    }
    cosmic::iced::widget::Stack::with_children(layers).into()
}

impl Settings {
    fn wallpaper_accent(&self) -> Color {
        self.settings_accent()
    }

    /// Room for display controls beside a compact picture.
    fn wallpaper_wide(&self) -> bool {
        self.content_width() >= 584.
    }

    /// The selected display's draft background, as COSMIC will draw it, with
    /// the real clock renderer on top when the clock is a separate card.
    fn wallpaper_preview(&self, width: f32) -> Element<'_, Message> {
        let editor = &self.wallpaper;
        let height = (width * 9. / 16.).round();
        let shown = editor.shown();
        let picture: Element<'_, Message> = match &shown.handle {
            Some(handle) => cosmic::widget::image(handle.clone())
                .width(Length::Fill)
                .height(height)
                .content_fit(cosmic::iced::ContentFit::Cover)
                .border_radius(12.)
                .into(),
            None => fill(
                container(label(shown.note.unwrap_or("No preview"), 12., MUTED))
                    .center_x(Length::Fill)
                    .center_y(Length::Fill)
                    .into(),
                BASE,
                LINE,
                1.,
                12.,
            )
            .width(Length::Fill)
            .height(height)
            .into(),
        };
        let picture = if let Some(rain) = self.rain_preview(height) {
            cosmic::iced::widget::Stack::with_children(vec![picture, rain]).into()
        } else {
            picture
        };
        let mut layers = vec![
            cosmic::iced::widget::space()
                .width(Length::Fill)
                .height(height)
                .into(),
            picture,
        ];
        // Only cards that are on the desktop; never panels or a dock.
        if width >= 400.
            && shown.handle.is_some()
            && !self.config.glance.enabled
            && self.config.data_visible(Widget::Clock)
        {
            let mut config = self.config.clone();
            config.desktop_palette = Some(editor.preview_palette(&self.config));
            config.appearance.color_source = crate::config::ColorSource::Desktop;
            layers.push(
                container(
                    crate::app::settings_clock_preview(&config, [width * 0.46, height * 0.36])
                        .map(|_| Message::Preview),
                )
                .padding([14, 16])
                .into(),
            );
        }
        let mut notes = Vec::new();
        if shown.animated {
            // cosmic-bg draws one frame of an animated picture.
            notes.push(pill("Animated · shown still", FG));
        }
        if shown.folder
            && let Some(Setting::Known(entry)) = editor.effective()
        {
            notes.push(pill(
                format!(
                    "Slideshow · every {}",
                    every_label(entry.rotation_frequency)
                ),
                FG,
            ));
        }
        if !notes.is_empty() {
            layers.push(
                container(row::with_children(notes).spacing(6))
                    .width(Length::Fill)
                    .height(height)
                    .align_bottom(height)
                    .padding([8, 10])
                    .into(),
            );
        }
        container(
            cosmic::iced::widget::Stack::with_children(layers)
                .width(Length::Fill)
                .clip(true),
        )
        .id(cosmic::widget::Id::new("wallpaper-preview"))
        .width(if self.wallpaper_wide() {
            Length::Fill
        } else {
            Length::Fixed(width)
        })
        .max_width(width)
        .into()
    }

    /// Same background on all displays, and which display is being edited.
    fn wallpaper_displays(&self) -> Option<Element<'_, Message>> {
        let accent = self.wallpaper_accent();
        let editor = &self.wallpaper;
        let draft = editor.draft()?;
        let displays = draft.displays();
        let same = displays != Setting::Known(Displays::Separate);
        let mut same_toggle = toggler(same);
        if matches!(displays, Setting::Known(_)) {
            same_toggle = same_toggle.on_toggle(|on| edit(Edit::Same(on)));
        }
        let mut controls = Vec::new();
        if let Setting::Unsupported(raw) = &displays {
            controls.push(kept(raw));
        }
        controls.push(same_toggle.into());
        let mut rows = vec![split(
            label("Same on all displays", 14., FG),
            row::with_children(controls)
                .spacing(10)
                .align_y(Alignment::Center)
                .into(),
        )];
        if !same {
            let mut chips = vec![(
                "Shared".to_string(),
                *editor.target() == Target::Shared,
                Some(edit(Edit::Display(Target::Shared))),
            )];
            for view in editor.displays() {
                if !crate::desktop::wallpaper::valid_output_name(&view.name) {
                    continue;
                }
                let mut text = display_name(&view.name);
                text.push_str(if view.listed { " · own" } else { " · shared" });
                if !view.connected {
                    text.push_str(" · not connected");
                }
                let target = Target::Output(view.name.clone());
                chips.push((
                    text,
                    *editor.target() == target,
                    Some(edit(Edit::Display(target))),
                ));
            }
            rows.push(segmented(chips, accent));
        }
        if let Target::Output(name) = editor.target()
            && editor.target_blocked().is_none()
            && editor
                .displays()
                .iter()
                .any(|view| view.name == *name && view.listed)
        {
            rows.push(split(
                label(
                    format!("{} has its own background.", display_name(name)),
                    12.,
                    MUTED,
                ),
                button::text("Use the shared background")
                    .on_press(edit(Edit::UseShared))
                    .into(),
            ));
        }
        Some(group(Some("Wallpaper display"), rows))
    }

    /// Theme mode and fonts are a separate System appearance draft, so they
    /// are linked rather than mixed into this page's Apply.
    fn wallpaper_related(&self) -> Element<'_, Message> {
        let link = |id: &'static str,
                    title: &'static str,
                    description: &'static str,
                    trailing: Element<'static, Message>,
                    message: Message| {
            button::custom(
                row::with_children(vec![detail(title, description), trailing])
                    .spacing(12)
                    .align_y(Alignment::Center),
            )
            .id(cosmic::widget::Id::new(id))
            .class(nav_style(false, self.wallpaper_accent()))
            .padding([4, 0])
            .width(Length::Fill)
            .on_press(message)
            .into()
        };
        link(
            "wallpaper-cosmic-settings",
            "More in COSMIC Settings",
            "Options this app doesn’t cover",
            label("↗", 16., MUTED),
            Message::OpenDesktop("wallpaper"),
        )
    }

    fn wallpaper_choose(&self) -> Element<'_, Message> {
        container(button::standard("Choose picture…").on_press(edit(Edit::AddPicture)))
            .id(cosmic::widget::Id::new("wallpaper-choose-picture"))
            .into()
    }

    /// Page title with the store note (fixture or preview mode only).
    pub(super) fn wallpaper_header(&self) -> Element<'_, Message> {
        row::with_children(vec![
            heading(
                "Wallpaper",
                "Choose locally, then Apply wallpaper to confirm.",
            ),
            store_pill(self.wallpaper.store(), self.wallpaper_accent()),
        ])
        .spacing(8)
        .align_y(Alignment::Center)
        .wrap()
        .into()
    }

    fn wallpaper_tile<'a>(&'a self, tile: &'a Tile, enabled: bool) -> Element<'a, Message> {
        let accent = self.wallpaper_accent();
        let editor = &self.wallpaper;
        let source = tile.source(editor.bundled_dir());
        let selected = editor
            .entry()
            .and_then(|entry| entry.known().map(|entry| entry.source == source))
            .unwrap_or(false);
        let current = editor.current() == Some(&source);
        let placeholder: Element<'a, Message> = match &tile.pick {
            Pick::Folder(path) => {
                let count = match editor.scan(path) {
                    Some(Ok(scan)) if scan.truncated => format!("{}+ pictures", scan.images),
                    Some(Ok(scan)) if scan.images == 1 => "1 picture".into(),
                    Some(Ok(scan)) => format!("{} pictures", scan.images),
                    Some(Err(_)) => "Can’t be read".into(),
                    None if tile.missing => String::new(),
                    None => "Counting…".into(),
                };
                column::with_children(vec![
                    icon::from_name("folder-pictures-symbolic")
                        .size(22)
                        .icon()
                        .into(),
                    label(count, 11., MUTED),
                ])
                .spacing(4)
                .align_x(Alignment::Center)
                .into()
            }
            _ => label(
                editor
                    .thumb_note(tile)
                    .filter(|_| !tile.missing)
                    .unwrap_or(if tile.missing { "" } else { "…" }),
                11.,
                MUTED,
            ),
        };
        let picture = framed(
            face(editor.thumb(tile), placeholder),
            selected,
            current,
            tile.missing,
            accent,
        );
        let mut parts = vec![picture];
        if !tile.label.is_empty() {
            parts.push(
                container(line(
                    if tile.external {
                        format!("{} · COSMIC Settings", tile.label)
                    } else {
                        tile.label.clone()
                    },
                    11.,
                    if selected { FG } else { MUTED },
                ))
                .width(TILE_W + 4.)
                .padding([0, 4])
                .into(),
            );
        }
        button::custom(column::with_children(parts).spacing(3))
            .class(nav_style(false, accent))
            .padding(0)
            .on_press_maybe(enabled.then(|| edit(Edit::Select(tile.pick.clone()))))
            .into()
    }

    fn add_tile<'a>(
        &self,
        title: &'static str,
        symbol: &'static str,
        message: Edit,
    ) -> Element<'a, Message> {
        let accent = self.wallpaper_accent();
        button::custom(
            column::with_children(vec![
                fill(
                    container(icon::from_name(symbol).size(22).icon())
                        .center_x(Length::Fill)
                        .center_y(Length::Fill)
                        .into(),
                    Color { a: 0.04, ..FG },
                    LINE,
                    1.,
                    10.,
                )
                // Same outer size as a framed wallpaper, so the gallery wraps
                // as an even grid.
                .width(TILE_W + 4.)
                .height(TILE_H + 4.)
                .padding(2)
                .into(),
                container(label(title, 11., FG))
                    .width(TILE_W + 4.)
                    .padding([0, 4])
                    .into(),
            ])
            .spacing(3),
        )
        .class(nav_style(false, accent))
        .padding(0)
        .on_press(edit(message))
        .into()
    }

    pub(super) fn wallpaper_page(&self) -> Element<'_, Message> {
        let editor = &self.wallpaper;
        let mut items: Vec<Element<'_, Message>> = vec![self.wallpaper_header()];
        if let Some(error) = editor.unavailable() {
            items.push(banner(
                format!("Wallpaper settings are unavailable. {error}"),
                ERROR,
                Vec::new(),
            ));
            return column::with_children(items).spacing(14).into();
        }
        let Some(draft) = editor.draft() else {
            items.push(label("Reading wallpaper settings…", 12., MUTED));
            return column::with_children(items).spacing(14).into();
        };
        if let Some(banner) = recovery_banner(
            editor.recovery(),
            editor.writable(),
            edit(Edit::Recover),
            edit(Edit::KeepRecovery),
        ) {
            items.push(banner);
        }
        if !draft.conflicts().is_empty() {
            items.push(conflict_banner(
                crate::settings::system_appearance::describe(draft.conflicts()),
                edit(Edit::UseCurrent),
            ));
        }
        let blocked_target = editor.target_blocked();
        if let Some(raw) = &blocked_target {
            let mut actions = vec![kept(raw)];
            if let Target::Output(name) = editor.target()
                && editor
                    .displays()
                    .iter()
                    .any(|view| view.name == *name && view.listed)
            {
                actions.push(
                    button::text("Use the shared background")
                        .on_press(edit(Edit::UseShared))
                        .into(),
                );
            }
            items.push(banner(
                "This background has settings this version can’t change. It is kept.",
                WARN,
                actions,
            ));
        }
        let blocked = editor.blocked();
        if !blocked.is_empty() {
            items.push(banner(
                format!(
                    "Can’t apply: {} {}",
                    crate::settings::system_appearance::describe(
                        &blocked.iter().map(|b| b.key.clone()).collect::<Vec<_>>()
                    ),
                    blocked[0].reason
                ),
                WARN,
                Vec::new(),
            ));
        }
        if let Some(problem) = editor.source_problem() {
            let mut actions = Vec::new();
            let bundled_missing = matches!(
                editor.entry().and_then(|entry| entry.known().map(|entry| entry.source.clone())),
                Some(BgSource::Path(path)) if path.starts_with(editor.bundled_dir())
            );
            if bundled_missing {
                actions.push(
                    button::standard("Reinstall bundled wallpapers")
                        .on_press_maybe(editor.writable().then(|| edit(Edit::Reinstall)))
                        .into(),
                );
            }
            items.push(banner(problem.to_string(), WARN, actions));
        }
        // An unchanged missing background warns; Apply isn't held back.
        if editor.shown().warning {
            let mut actions = Vec::new();
            // The Halcyon tab has its own Reinstall banner.
            if editor.shown().bundled && editor.tab != Tab::Bundled {
                actions.push(
                    button::standard("Reinstall bundled wallpapers")
                        .on_press_maybe(editor.writable().then(|| edit(Edit::Reinstall)))
                        .into(),
                );
            }
            items.push(banner(
                "The current background picture is missing.",
                WARN,
                actions,
            ));
        }
        items.extend(self.colors_conflicts());

        let enabled = blocked_target.is_none();
        let scene = if self.scene_open() {
            self.wallpaper_preview(self.stage_width().max(240.) - 12.)
        } else {
            cosmic::widget::space().height(0).into()
        };
        let target = match editor.target() {
            Target::Shared => "Shared wallpaper".to_string(),
            Target::Output(name) => display_name(name),
        };
        let caption = format!(
            "{} · {target}",
            if editor.background_dirty() {
                "Previewing wallpaper · local until Apply wallpaper"
            } else {
                "Current wallpaper"
            }
        );
        let preview = self.scene_frame(scene, &caption);
        let mut side = vec![self.wallpaper_choose()];
        if let Some(displays) = self.wallpaper_displays() {
            side.push(displays);
        }
        side.push(self.rain_controls());
        side.push(self.wallpaper_related());
        items.push(self.inspector_layout(
            preview,
            column::with_children(side).spacing(14).into(),
            "wallpaper",
        ));
        items.push(self.wallpaper_gallery(enabled, false));
        column::with_children(items).spacing(16).into()
    }

    /// Wallpapers heading, sources, thumbnails and the options for what the
    /// selected display shows.
    fn wallpaper_gallery(&self, enabled: bool, choose: bool) -> Element<'_, Message> {
        let accent = self.wallpaper_accent();
        let editor = &self.wallpaper;
        let mut items: Vec<Element<'_, Message>> = Vec::new();
        let title: Element<'_, Message> = text::heading("Wallpapers").into();
        items.push(if choose {
            split(title, self.wallpaper_choose())
        } else {
            title
        });
        let tab = editor.tab;
        items.push(segmented(
            [
                ("Halcyon", Tab::Bundled),
                ("COSMIC", Tab::Cosmic),
                ("Yours", Tab::Yours),
                ("Solid & gradient", Tab::Colors),
            ]
            .into_iter()
            .map(|(title, value)| {
                (
                    title.to_string(),
                    tab == value,
                    Some(edit(Edit::Tab(value))),
                )
            })
            .collect(),
            accent,
        ));
        let (page, pages) = (editor.page(), editor.pages());
        if pages > 1 {
            items.push(split(
                label(format!("{} of {pages}", page + 1), 12., MUTED),
                row::with_children(vec![
                    button::text("Previous")
                        .on_press_maybe((page > 0).then(|| edit(Edit::Page(page - 1))))
                        .into(),
                    button::text("Next")
                        .on_press_maybe((page + 1 < pages).then(|| edit(Edit::Page(page + 1))))
                        .into(),
                ])
                .spacing(8)
                .into(),
            ));
        }
        let mut tiles: Vec<Element<'_, Message>> = Vec::new();
        if tab == Tab::Yours {
            tiles.push(self.add_tile("Add folder", "folder-new-symbolic", Edit::AddFolder));
        }
        for tile in editor.page_tiles() {
            tiles.push(self.wallpaper_tile(tile, enabled));
        }
        items.push(
            row::with_children(tiles)
                .spacing(6)
                .wrap()
                .vertical_spacing(10)
                .into(),
        );
        match tab {
            Tab::Bundled if editor.tiles().iter().any(|tile| tile.missing) => {
                items.push(banner(
                    "Some bundled wallpapers aren’t installed.",
                    WARN,
                    vec![
                        button::standard("Reinstall bundled wallpapers")
                            .on_press_maybe(editor.writable().then(|| edit(Edit::Reinstall)))
                            .into(),
                    ],
                ));
            }
            Tab::Cosmic if editor.tiles().is_empty() => {
                items.push(label("No COSMIC backgrounds are installed.", 12., MUTED));
            }
            Tab::Yours => {
                if let Some(error) = editor.collection_error() {
                    items.push(banner(error.to_string(), WARN, Vec::new()));
                }
                if editor.session() {
                    items.push(label(
                        crate::settings::wallpaper::ADDED_IN_PREVIEW,
                        11.,
                        MUTED,
                    ));
                }
                let selected = editor
                    .entry()
                    .and_then(|entry| entry.known().map(|entry| entry.source.clone()));
                if let Some(BgSource::Path(path)) = selected
                    && editor.in_collection(&path)
                {
                    items.push(
                        container(
                            button::text("Remove from collection")
                                .on_press(edit(Edit::Remove(path.clone()))),
                        )
                        .into(),
                    );
                }
            }
            Tab::Colors => items.push(self.wallpaper_custom(enabled)),
            _ => {}
        }

        // Options for what the selected display shows.
        if let Some(Setting::Known(entry)) = editor.entry() {
            let folder = editor.shown().folder;
            match &entry.source {
                BgSource::Path(_) if folder => {
                    items.push(self.wallpaper_slideshow(&entry, enabled))
                }
                BgSource::Path(_) => items.push(self.wallpaper_fit(&entry, enabled)),
                BgSource::Color(_) => {}
            }
        }
        column::with_children(items).spacing(12).into()
    }

    /// Colors from the selected wallpaper's hues, toned to be usable as an
    /// accent, or why there are none yet.
    pub(super) fn wallpaper_swatches(&self) -> Element<'_, Message> {
        let editor = &self.wallpaper;
        let accent = self.wallpaper_accent();
        if let Some(suggestion) = editor
            .suggestion()
            .filter(|value| !value.accents.is_empty())
        {
            row::with_children(suggestion.accents.iter().enumerate().map(|(index, rgb)| {
                let [r, g, b] = *rgb;
                let selected =
                    editor.accent_color().or(self.config.appearance.accent) == Some(*rgb);
                let chip = fill(
                    cosmic::iced::widget::space().width(24).height(24).into(),
                    Color::from_rgb8(r, g, b),
                    if selected { FG } else { LINE },
                    if selected { 2. } else { 1. },
                    12.,
                );
                container(
                    button::custom(chip)
                        .class(nav_style(selected, accent))
                        .padding(4)
                        .on_press(edit(Edit::Accent(*rgb))),
                )
                .id(cosmic::widget::Id::new(format!("wallpaper-swatch-{index}")))
                .into()
            }))
            .spacing(2)
            .wrap()
            .into()
        } else {
            container(label(
                match editor.shown().note {
                    _ if editor
                        .suggestion()
                        .is_some_and(|value| !value.swatches.is_empty()) =>
                    {
                        "No strong colors here"
                    }
                    Some("Loading…") => "Finding colors…",
                    Some(crate::desktop::wallpaper::media::NO_PREVIEW) => {
                        "Available for PNG and JPEG"
                    }
                    Some(crate::desktop::wallpaper::media::MISSING) => {
                        "Choose an available picture"
                    }
                    _ => "Choose a background",
                },
                12.,
                MUTED,
            ))
            .max_width(180)
            .into()
        }
    }

    fn wallpaper_fit<'a>(&'a self, entry: &BgEntry, enabled: bool) -> Element<'a, Message> {
        let accent = self.wallpaper_accent();
        let editor = &self.wallpaper;
        let choice = match entry.scaling_mode {
            ScalingMode::Zoom => FitChoice::Fill,
            ScalingMode::Fit(_) => FitChoice::Fit,
            ScalingMode::Stretch => FitChoice::Stretch,
        };
        let mut rows = vec![split(
            label("Fit", 15., FG),
            segmented(
                [
                    ("Fill", FitChoice::Fill),
                    ("Fit", FitChoice::Fit),
                    ("Stretch", FitChoice::Stretch),
                ]
                .into_iter()
                .map(|(title, value)| {
                    (
                        title.to_string(),
                        choice == value,
                        enabled.then(|| edit(Edit::Fit(value))),
                    )
                })
                .collect(),
                accent,
            ),
        )];
        if let ScalingMode::Fit(color) = entry.scaling_mode {
            let background = self.config.palette().background;
            let palette = hex([background.r, background.g, background.b]);
            let text = editor
                .fit_text()
                .map(String::from)
                .unwrap_or_else(|| hex(color));
            let target = crate::settings::color_well::Target::Fit;
            let controls: Vec<Element<'a, Message>> = vec![
                self.color_button(
                    "wallpaper-fit-color",
                    target,
                    crate::settings::wallpaper::parse_rgb(&text).or(Some(color)),
                    enabled,
                ),
                button::text("Black")
                    .on_press_maybe(enabled.then(|| edit(Edit::FitColor("#000000".into()))))
                    .into(),
                button::text("Palette background")
                    .on_press_maybe(enabled.then(|| edit(Edit::FitColor(palette))))
                    .into(),
            ];
            rows.push(split(
                label("Letterbox color", 13., MUTED),
                row::with_children(controls)
                    .spacing(8)
                    .align_y(Alignment::Center)
                    .wrap()
                    .into(),
            ));
            if let Some(panel) = self.color_panel(target, &[]) {
                rows.push(panel);
            }
        }
        section(rows)
    }

    fn wallpaper_slideshow<'a>(&'a self, entry: &BgEntry, enabled: bool) -> Element<'a, Message> {
        let accent = self.wallpaper_accent();
        let editor = &self.wallpaper;
        let every = entry.rotation_frequency;
        let mut rows = vec![label("Slideshow", 15., FG)];
        if let BgSource::Path(path) = &entry.source
            && let Some(scan) = editor.scan(path)
        {
            match scan {
                Ok(scan) => {
                    let count = if scan.truncated {
                        format!(
                            "{}+ pictures · only part of this large folder was counted",
                            scan.images
                        )
                    } else {
                        format!(
                            "{} picture{}",
                            scan.images,
                            if scan.images == 1 { "" } else { "s" }
                        )
                    };
                    rows.push(label(count, 12., MUTED));
                    if scan.others > 0 {
                        rows.push(label(
                            "Some files aren’t pictures; the slideshow skips a turn on them.",
                            12.,
                            WARN,
                        ));
                    }
                }
                Err(error) => rows.push(label(error.clone(), 12., WARN)),
            }
        }
        let mut every_controls = vec![segmented(
            ROTATIONS
                .iter()
                .map(|seconds| {
                    (
                        every_label(*seconds),
                        every == *seconds,
                        enabled.then(|| edit(Edit::Every(*seconds))),
                    )
                })
                .collect(),
            accent,
        )];
        if !ROTATIONS.contains(&every) {
            every_controls.insert(
                0,
                pill(
                    if every == 0 {
                        "Paused · kept".to_string()
                    } else {
                        format!("Every {} · kept", every_label(every))
                    },
                    WARN,
                ),
            );
        }
        rows.push(split(
            label("Change every", 13., FG),
            row::with_children(every_controls)
                .spacing(8)
                .align_y(Alignment::Center)
                .wrap()
                .into(),
        ));
        rows.push(split(
            label("Order", 13., FG),
            segmented(
                [
                    ("In order", SamplingMethod::Alphanumeric),
                    ("Shuffle", SamplingMethod::Random),
                ]
                .into_iter()
                .map(|(title, value)| {
                    (
                        title.to_string(),
                        entry.sampling_method == value,
                        enabled.then(|| edit(Edit::Order(value))),
                    )
                })
                .collect(),
                accent,
            ),
        ));
        section(rows)
    }

    fn wallpaper_custom(&self, enabled: bool) -> Element<'_, Message> {
        let accent = self.wallpaper_accent();
        let editor = &self.wallpaper;
        let color = match editor.entry() {
            Some(Setting::Known(BgEntry {
                source: BgSource::Color(color),
                ..
            })) => Some(color),
            _ => None,
        };
        // As typed, else as displayed: the draft's own values.
        let custom = editor
            .custom()
            .cloned()
            .or_else(|| color.as_ref().map(Custom::of));
        let count = custom.as_ref().map_or(1, |custom| custom.values.len());
        let mut rows = vec![split(
            label("Custom", 15., FG),
            segmented(
                [("1 color", 1), ("2 colors", 2), ("3 colors", 3)]
                    .into_iter()
                    .map(|(title, value)| {
                        (
                            title.to_string(),
                            count == value,
                            enabled.then(|| edit(Edit::Stops(value))),
                        )
                    })
                    .collect(),
                accent,
            ),
        )];
        if count > 3 {
            rows.push(
                row::with_children(vec![
                    pill(format!("{count} colors · kept"), WARN),
                    label("Choose 1–3 colors to edit them here.", 12., MUTED),
                ])
                .spacing(10)
                .align_y(Alignment::Center)
                .into(),
            );
        } else {
            let mut fields: Vec<Element<'_, Message>> = Vec::new();
            for index in 0..count.max(1) {
                let text = custom
                    .as_ref()
                    .map(|custom| custom.text(index))
                    .unwrap_or_default();
                let shown = custom
                    .as_ref()
                    .and_then(|custom| custom.shown(index))
                    .unwrap_or([0.2, 0.4, 0.6]);
                let _ = text;
                fields.push(self.color_button(
                    format!("wallpaper-stop-{index}"),
                    crate::settings::color_well::Target::Stop(index),
                    Some(shown),
                    enabled,
                ));
            }
            if custom
                .as_ref()
                .is_some_and(|custom| custom.invalid().is_some())
            {
                fields.push(label("Use #RRGGBB", 11., WARN));
            }
            rows.push(
                row::with_children(fields)
                    .spacing(8)
                    .align_y(Alignment::Center)
                    .wrap()
                    .into(),
            );
            rows.extend((0..count.max(1)).filter_map(|index| {
                self.color_panel(crate::settings::color_well::Target::Stop(index), &[])
            }));
        }
        if count > 1 {
            let radius = custom.as_ref().map_or(180., |custom| custom.radius);
            let cardinal = [0., 90., 180., 270.].contains(&radius);
            let mut controls = Vec::new();
            if !cardinal {
                controls.push(pill("Custom angle · kept", WARN));
            }
            controls.push(segmented(
                [("↑", 0u16), ("→", 90), ("↓", 180), ("←", 270)]
                    .into_iter()
                    .map(|(title, degrees)| {
                        (
                            title.to_string(),
                            radius == f32::from(degrees),
                            enabled.then(|| edit(Edit::Direction(degrees))),
                        )
                    })
                    .collect(),
                accent,
            ));
            rows.push(split(
                label("Direction", 13., FG),
                row::with_children(controls)
                    .spacing(8)
                    .align_y(Alignment::Center)
                    .into(),
            ));
        }
        section(rows)
    }

    pub(super) fn wallpaper_actions(&self) -> Element<'_, Message> {
        let editor = &self.wallpaper;
        if !editor.background_dirty() {
            return self.simple_status();
        }
        let error = match editor.notice() {
            Some(Notice::Error(error)) => error.as_str(),
            _ => "Previewing wallpaper · desktop unchanged until Apply",
        };
        container(
            row::with_children(vec![
                container(label(
                    error,
                    12.,
                    if matches!(editor.notice(), Some(Notice::Error(_))) {
                        ERROR
                    } else {
                        MUTED
                    },
                ))
                .width(Length::Fill)
                .into(),
                button::text("Cancel")
                    .id(cosmic::widget::Id::new("wallpaper-discard"))
                    .on_press(edit(Edit::Discard))
                    .into(),
                button::suggested("Apply wallpaper")
                    .id(cosmic::widget::Id::new("wallpaper-apply"))
                    .on_press_maybe(editor.can_apply().then_some(edit(Edit::Apply)))
                    .into(),
            ])
            .spacing(10)
            .align_y(Alignment::Center),
        )
        .padding([8, 22])
        .width(Length::Fill)
        .into()
    }
}
