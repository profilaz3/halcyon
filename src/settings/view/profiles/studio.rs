//! The Look studio: the draft's live preview on top, one picture per part
//! below, and a picture grid for each part's choices.
use super::*;
use crate::desktop::appearance::ThemeVariant;
use crate::desktop::panels::PanelKind;
use crate::desktop::profiles::Profile;
use crate::settings::desktop::Notice;
use crate::settings::profiles::Preview as Planned;
use crate::settings::profiles::studio::{Built, Edit as Studio, Element as Part, Pick, Start};
use cosmic::widget::image::Handle;
use std::path::PathBuf;

fn studio(edit: Studio) -> Message {
    super::edit(Edit::Studio(edit))
}

/// A quiet picture for parts without a wallpaper behind them.
fn neutral<'a>(size: [f32; 2]) -> Element<'a, Message> {
    container(cosmic::widget::space())
        .width(size[0])
        .height(size[1])
        .class(cosmic::theme::Container::custom(|theme| {
            cosmic::iced::widget::container::Style {
                background: Some(Background::Gradient(
                    cosmic::iced::gradient::Linear::new(2.4)
                        .add_stop(0., theme.cosmic().primary(theme.transparent).base.into())
                        .add_stop(1., theme.cosmic().background(theme.transparent).base.into())
                        .into(),
                )),
                border: Border {
                    radius: 6.into(),
                    ..Default::default()
                },
                ..Default::default()
            }
        }))
        .into()
}

fn picture<'a>(handle: Option<Handle>, size: [f32; 2]) -> Element<'a, Message> {
    match handle {
        Some(handle) => cosmic::widget::image(handle)
            .width(size[0])
            .height(size[1])
            .content_fit(cosmic::iced::ContentFit::Cover)
            .border_radius(6.)
            .into(),
        None => neutral(size),
    }
}

fn shade<'a>(size: [f32; 2], alpha: f32) -> Element<'a, Message> {
    fill(
        cosmic::widget::space()
            .width(size[0])
            .height(size[1])
            .into(),
        Color::from_rgba(0., 0., 0., alpha),
        Color::TRANSPARENT,
        0.,
        6.,
    )
    .into()
}

/// Centers `content` over a picture of `size`.
fn centered<'a>(content: Element<'a, Message>, size: [f32; 2]) -> Element<'a, Message> {
    container(content)
        .width(size[0])
        .height(size[1])
        .align_x(Alignment::Center)
        .align_y(Alignment::Center)
        .into()
}

fn rain_of(profile: &Profile) -> Option<crate::rain::Preferences> {
    serde_json::from_value(profile.widget_style.as_ref()?.pointer("/rain")?.clone()).ok()
}

fn font_of(profile: &Profile) -> Option<&str> {
    profile
        .appearance
        .as_ref()?
        .interface_font
        .as_ref()
        .map(|font| font.family.as_str())
}

fn bundled_title(name: &str) -> Option<&'static str> {
    crate::desktop::wallpaper::BUNDLED
        .iter()
        .find(|(id, _)| *id == name)
        .map(|(_, title)| *title)
}

impl Settings {
    fn studio_tile_width(&self) -> f32 {
        let available = (self.content_width() - 2.).max(200.);
        let count = ((available + 12.) / (176. + 12.)).floor().clamp(2., 5.);
        ((available - (count - 1.) * 12.) / count).floor()
    }

    /// The draft's planned wallpaper, as the preview shows it.
    fn studio_backdrop(&self) -> Option<Handle> {
        let editor = &self.profiles;
        editor
            .backdrop_image()
            .cloned()
            .or_else(|| {
                editor
                    .profile()
                    .and_then(bundled_name)
                    .and_then(bundled_handle)
            })
            .or_else(|| editor.home_image().cloned())
    }

    /// The wallpaper a choice shows: its own picture, or the desktop's.
    fn choice_wallpaper(&self, pick: &Pick, profile: &Profile) -> Option<Handle> {
        let editor = &self.profiles;
        match pick {
            Pick::Desktop | Pick::Leave => editor.home_image().cloned(),
            Pick::Wallpaper(name) => bundled_handle(name),
            Pick::Look(Selection::Preset(id)) => bundled_name(profile)
                .and_then(bundled_handle)
                .or_else(|| look_image(id, true)),
            _ => bundled_name(profile).and_then(bundled_handle).or_else(|| {
                editor
                    .studio
                    .as_ref()
                    .filter(|draft| Part::Wallpaper.same(draft.desktop(), profile))
                    .and_then(|_| editor.home_image().cloned())
            }),
        }
    }

    /// One part of a look as a small picture.
    fn studio_thumb(
        &self,
        part: Part,
        profile: &Profile,
        planned: Option<&Planned>,
        icons: &[PathBuf],
        wallpaper: Option<Handle>,
        size: [f32; 2],
    ) -> Element<'_, Message> {
        let editor = &self.profiles;
        let planned = planned.or(editor.preview());
        let config = planned
            .and_then(|preview| preview.config.clone())
            .unwrap_or_else(|| self.config.clone());
        let layout = scene_geometry(self.placement.preview_snapshot().clone(), &config);
        let scale = size[0] / layout.screen[0].max(1.);
        let mut layers: Vec<Element<'_, Message>> = Vec::new();
        let base = self.studio_backdrop();
        match part {
            Part::Wallpaper => layers.push(picture(wallpaper, size)),
            Part::Weather => {
                layers.push(picture(base, size));
                layers.push(shade(size, 0.28));
                let rain = rain_of(profile).unwrap_or_else(|| config.rain.clone());
                if rain.enabled {
                    layers.extend(looks::weather_layers(&rain, size));
                } else {
                    layers.push(centered(label("No effect", 12., Color::WHITE), size));
                }
            }
            Part::Panel | Part::Dock => {
                let kind = if part == Part::Panel {
                    PanelKind::Panel
                } else {
                    PanelKind::Dock
                };
                layers.push(picture(base, size));
                let bars: Vec<&crate::desktop::panels::PanelDraft> = planned
                    .map(|preview| {
                        preview
                            .bars
                            .iter()
                            .filter(|bar| bar.kind() == kind && bar.listed())
                            .collect()
                    })
                    .unwrap_or_default();
                if bars.is_empty() {
                    layers.push(shade(size, 0.3));
                    layers.push(centered(
                        label(
                            format!("No {}", kind.title().to_lowercase()),
                            12.,
                            Color::WHITE,
                        ),
                        size,
                    ));
                } else {
                    // Larger than to scale, so the bar reads in a thumbnail.
                    layers.extend(super::super::desktop::preview::bars_on_output(
                        self.desktop.islands.installation.installed(),
                        bars,
                        &editor.installed.applets,
                        None,
                        size,
                        scale * 3.,
                        config.palette().accent,
                        &|_| None,
                        self.placement.display.as_deref().unwrap_or(""),
                    ));
                }
            }
            Part::Windows => {
                layers.push(picture(base, size));
                if let Some((pair, _)) =
                    planned.and_then(|preview| looks::window_pair(preview, [0.; 4], size, scale))
                {
                    layers.push(pair);
                }
            }
            Part::Colors => {
                layers.push(neutral(size));
                if let Some(appearance) = planned.and_then(|preview| preview.appearance.as_ref()) {
                    let active = appearance.active_variant();
                    let window = [size[0] * 0.4, size[1] * 0.6];
                    let windows = [ThemeVariant::Light, ThemeVariant::Dark]
                        .into_iter()
                        .map(|variant| {
                            let colors = appearance.preview(variant);
                            let ring = if variant == active {
                                color(colors.accent)
                            } else {
                                Color::TRANSPARENT
                            };
                            fill(
                                schematic_window_with(colors, window, 1),
                                Color::TRANSPARENT,
                                ring,
                                2.,
                                8.,
                            )
                            .padding(3)
                            .into()
                        })
                        .collect::<Vec<_>>();
                    let accent = color(appearance.preview(active).accent);
                    layers.push(centered(
                        row::with_children(windows).spacing(6).into(),
                        [size[0], size[1] * 1.1],
                    ));
                    layers.push(
                        pin(fill(
                            cosmic::widget::space().width(18).height(18).into(),
                            accent,
                            Color::WHITE,
                            2.,
                            9.,
                        ))
                        .position(Point::new(8., 8.))
                        .into(),
                    );
                }
            }
            Part::Font => {
                layers.push(neutral(size));
                let family = font_of(profile)
                    .or_else(|| editor.studio.as_ref().and_then(|d| font_of(d.desktop())))
                    .unwrap_or("Open Sans")
                    .to_string();
                let font = cosmic::iced::Font {
                    family: cosmic::iced::font::Family::Name(crate::widget_style::intern(&family)),
                    ..Default::default()
                };
                layers.push(centered(
                    column::with_children(vec![
                        text::body("Aa")
                            .size((size[1] * 0.4).clamp(24., 64.))
                            .line_height(cosmic::iced::widget::text::LineHeight::Relative(1.15))
                            .font(font)
                            .into(),
                        text::body("The quick brown fox")
                            .size(12.)
                            .line_height(cosmic::iced::widget::text::LineHeight::Relative(1.3))
                            .font(font)
                            .into(),
                    ])
                    .spacing(2)
                    .align_x(Alignment::Center)
                    .into(),
                    size,
                ));
            }
            Part::Icons => {
                layers.push(neutral(size));
                let edge = ((size[0] - 40.) / 5.).clamp(16., 40.);
                let row_items: Vec<Element<'_, Message>> = icons
                    .iter()
                    .map(|path| {
                        icon::from_path(path.clone())
                            .icon()
                            .size(edge as u16)
                            .into()
                    })
                    .collect();
                layers.push(centered(
                    if row_items.is_empty() {
                        label("No preview for this theme", 12., MUTED)
                    } else {
                        row::with_children(row_items).spacing(6).into()
                    },
                    size,
                ));
            }
            Part::WidgetStyle => {
                layers.push(picture(base, size));
                let card_size = [size[0] * 0.62, size[1] * 0.62];
                layers.push(centered(
                    card(Widget::Clock, card_size, &config, scale * 2.4),
                    size,
                ));
            }
            Part::Layout => {
                layers.push(picture(base, size));
                layers.push(shade(size, 0.35));
                let widgets = layout
                    .widgets
                    .iter()
                    .filter(|geometry| {
                        Widget::parse(&geometry.key).is_some_and(|w| config.widget_visible(w))
                    })
                    .collect::<Vec<_>>();
                if widgets.is_empty() {
                    layers.push(centered(label("No widgets", 12., Color::WHITE), size));
                }
                let accent = config.palette().accent;
                for geometry in widgets {
                    let area = [
                        (geometry.size[0] * scale).max(4.),
                        (geometry.size[1] * scale).max(4.),
                    ];
                    layers.push(
                        pin(fill(
                            cosmic::widget::space()
                                .width(area[0])
                                .height(area[1])
                                .into(),
                            Color { a: 0.55, ..accent },
                            Color::WHITE,
                            1.,
                            3.,
                        ))
                        .position(Point::new(
                            geometry.position[0] * scale,
                            geometry.position[1] * scale,
                        ))
                        .into(),
                    );
                }
            }
            Part::DesktopIcons => {
                layers.push(picture(base, size));
                let icons = planned
                    .and_then(|preview| preview.desktop_icons.as_ref())
                    .and_then(|draft| draft.config().known().copied());
                let shown = icons
                    .map(|icons| looks::icon_layers(&icons, [0.; 4], &[], size, scale * 1.4).0)
                    .unwrap_or_default();
                if shown.is_empty() {
                    layers.push(shade(size, 0.3));
                    layers.push(centered(label("No desktop icons", 12., Color::WHITE), size));
                } else {
                    layers.extend(shown);
                }
            }
        }
        container(stack(layers).width(size[0]).height(size[1]).clip(true))
            .width(size[0])
            .height(size[1])
            .into()
    }

    /// What a part of the draft is, in a few words.
    fn studio_summary(&self, part: Part, profile: &Profile) -> String {
        if !part.has(profile) {
            return "As it is now".into();
        }
        let note = |domain| {
            let note = looks::part_note(profile, domain);
            if note.is_empty() {
                "Set by this look".to_string()
            } else {
                let mut chars = note.chars();
                chars
                    .next()
                    .map(|first| first.to_uppercase().chain(chars).collect())
                    .unwrap_or_default()
            }
        };
        match part {
            Part::Wallpaper => bundled_name(profile)
                .and_then(bundled_title)
                .unwrap_or("Your picture")
                .into(),
            Part::Weather => match rain_of(profile) {
                Some(rain) if rain.enabled => crate::settings::profiles::studio::WEATHER
                    .iter()
                    .find(|(id, _)| {
                        serde_json::to_value(rain.effect)
                            .ok()
                            .is_some_and(|v| v.as_str() == Some(id))
                    })
                    .map(|(_, title)| (*title).to_string())
                    .unwrap_or_else(|| "On".into()),
                _ => "No effect".into(),
            },
            Part::Colors => {
                let style = profile.widget_style.as_ref();
                let follows = style
                    .and_then(|s| s.pointer("/appearance/wallpaper_colors"))
                    .and_then(serde_json::Value::as_bool)
                    == Some(true);
                let mode = match profile.appearance.as_ref().and_then(|a| a.mode) {
                    Some(crate::desktop::profiles::ModeName::Dark) => "Dark",
                    Some(crate::desktop::profiles::ModeName::Light) => "Light",
                    Some(crate::desktop::profiles::ModeName::Automatic) => "Light and dark",
                    None => "Your mode",
                };
                if follows {
                    format!("{mode}, wallpaper colors")
                } else {
                    format!("{mode}, own accent")
                }
            }
            Part::Font => font_of(profile).unwrap_or("Your font").into(),
            Part::Icons => {
                let id = profile
                    .appearance
                    .as_ref()
                    .and_then(|a| a.icon_theme.clone())
                    .unwrap_or_default();
                self.profiles
                    .installed
                    .icons
                    .themes
                    .iter()
                    .find(|theme| theme.id == id)
                    .map(|theme| theme.name.clone())
                    .unwrap_or(id)
            }
            Part::Windows => {
                let mut items = Vec::new();
                let variant = profile
                    .appearance
                    .as_ref()
                    .and_then(|a| a.dark.as_ref().or(a.light.as_ref()));
                if let Some(crate::desktop::profiles::Corners::Preset(round)) =
                    variant.and_then(|v| v.corners.as_ref())
                {
                    items.push(match round {
                        cosmic::cosmic_theme::Roundness::Round => "Round corners",
                        cosmic::cosmic_theme::Roundness::SlightlyRound => "Soft corners",
                        _ => "Square corners",
                    });
                }
                match variant.and_then(|v| v.gaps) {
                    Some((0, 0)) => items.push("no gaps"),
                    Some((_, inner)) if inner <= 8 => items.push("small gaps"),
                    Some(_) => items.push("wide gaps"),
                    None => {}
                }
                if items.is_empty() {
                    "Set by this look".into()
                } else {
                    let text = items.join(", ");
                    let mut chars = text.chars();
                    chars
                        .next()
                        .map(|first| first.to_uppercase().chain(chars).collect())
                        .unwrap_or_default()
                }
            }
            Part::Panel => note(Domain::Panel),
            Part::Dock => note(Domain::Dock),
            Part::WidgetStyle => note(Domain::WidgetStyle),
            Part::Layout => note(Domain::WidgetLayout),
            Part::DesktopIcons => note(Domain::DesktopIcons),
        }
    }

    /// A picture tile with a title and a line below it.
    fn studio_tile<'a>(
        &'a self,
        thumb: Element<'a, Message>,
        title: String,
        line: String,
        selected: bool,
        id: String,
        on_press: Option<Message>,
        width: f32,
    ) -> Element<'a, Message> {
        let mut head = vec![container(label(title, 13., FG)).width(Length::Fill).into()];
        if selected {
            head.push(
                icon::from_name("object-select-symbolic")
                    .size(16)
                    .icon()
                    .class(cosmic::theme::Svg::Custom(std::rc::Rc::new(|theme| {
                        cosmic::iced::widget::svg::Style {
                            color: Some(theme.cosmic().accent_text_color().into()),
                        }
                    })))
                    .into(),
            );
        }
        button::custom(
            column::with_children(vec![
                thumb,
                row::with_children(head)
                    .spacing(4)
                    .align_y(Alignment::Center)
                    .into(),
                line_text(line),
            ])
            .spacing(6),
        )
        .class(tile_style(selected, self.settings_accent()))
        .padding(8)
        .width(width)
        .id(cosmic::widget::Id::new(id))
        .on_press_maybe(on_press.filter(|_| !self.native_busy))
        .into()
    }

    pub(super) fn studio_page(&self) -> Element<'_, Message> {
        let editor = &self.profiles;
        let Some(draft) = &editor.studio else {
            return cosmic::widget::space().into();
        };
        let accent = self.settings_accent();
        let mut items: Vec<Element<'_, Message>> = Vec::new();
        let writable = editor.writable() && !editor.busy() && !self.native_busy;

        // Name and description, edited where they show.
        let name = text_input("Name this look", &draft.profile.name)
            .id(cosmic::widget::Id::new("studio-name"))
            .size(20)
            .on_input(|text| studio(Studio::Name(text)));
        let description = text_input(
            "Describe it in a line (optional)",
            draft.profile.description.as_deref().unwrap_or(""),
        )
        .id(cosmic::widget::Id::new("studio-description"))
        .on_input(|text| studio(Studio::Description(text)));
        let state = if draft.saved().is_none() {
            "Not saved yet"
        } else if draft.dirty() {
            "Unsaved changes"
        } else {
            "Saved"
        };
        let save_label = if draft.saved().is_some() {
            "Save"
        } else {
            "Save to my looks"
        };
        let can_save = writable && (draft.dirty() || draft.saved().is_none());
        let actions = row::with_children(vec![
            label(state, 12., MUTED),
            cosmic::widget::space().width(Length::Fill).into(),
            button::icon(icon::from_name("edit-undo-symbolic").size(16))
                .id(cosmic::widget::Id::new("studio-undo"))
                .on_press_maybe(draft.can_undo().then(|| studio(Studio::Undo)))
                .apply(|control| super::super::tip::tip(control, "Undo")),
            button::icon(icon::from_name("edit-redo-symbolic").size(16))
                .id(cosmic::widget::Id::new("studio-redo"))
                .on_press_maybe(draft.can_redo().then(|| studio(Studio::Redo)))
                .apply(|control| super::super::tip::tip(control, "Redo")),
            if draft.dirty() || draft.saved().is_none() {
                button::suggested(save_label)
            } else {
                button::standard(save_label)
            }
            .id(cosmic::widget::Id::new("studio-save"))
            .on_press_maybe(can_save.then(|| studio(Studio::Save)))
            .into(),
            button::icon(icon::from_name("view-more-symbolic").size(18))
                .id(cosmic::widget::Id::new("studio-more"))
                .selected(draft.menu)
                .on_press(studio(Studio::Menu(!draft.menu)))
                .apply(|control| super::super::tip::tip(control, "More")),
        ])
        .spacing(6)
        .align_y(Alignment::Center);
        items.push(
            column::with_children(vec![name.into(), description.into(), actions.into()])
                .spacing(8)
                .into(),
        );
        if draft.menu {
            items.push(self.studio_menu(writable));
        }
        if draft.confirm_delete {
            let rules = match editor.studio_rules_using() {
                0 => String::new(),
                1 => " An automation rule uses it and will stop working.".into(),
                count => format!(" {count} automation rules use it and will stop working."),
            };
            items.push(banner(
                format!("Delete “{}” from your looks?{rules}", draft.profile.name),
                WARN,
                vec![
                    button::text("Cancel")
                        .id(cosmic::widget::Id::new("studio-delete-cancel"))
                        .on_press(studio(Studio::Delete(false)))
                        .into(),
                    button::destructive("Delete")
                        .id(cosmic::widget::Id::new("studio-delete-confirm"))
                        .on_press_maybe(writable.then(|| studio(Studio::Delete(true))))
                        .into(),
                ],
            ));
        }
        if let Some(built) = &draft.built {
            items.push(self.studio_built(built));
        }
        match &editor.notice {
            Some(Notice::Error(_)) | Some(Notice::Warning(_)) => {
                let notice = editor.notice.as_ref().unwrap();
                items.push(banner(notice.text().to_string(), WARN, Vec::new()));
            }
            Some(notice) => items.push(label(notice.text().to_string(), 12., MUTED)),
            None => {}
        }

        // The draft, large, with the part under the pointer ringed.
        let height = self
            .stage_height()
            .min((self.window.height - 420.).max(180.))
            .min(self.content_width() * 0.5);
        items.push(match editor.preview() {
            Some(preview) => self.profile_scene(preview.clone(), height, false, false),
            None => label("Reading your desktop…", 12., MUTED),
        });

        if draft.reviewing {
            items.push(self.studio_review(accent));
        } else if let Some(part) = draft.picking {
            items.push(self.studio_picker(part));
        } else {
            items.push(self.studio_tiles());
        }
        container(column::with_children(items).spacing(16))
            .id(cosmic::widget::Id::new("studio"))
            .into()
    }

    fn studio_menu(&self, writable: bool) -> Element<'_, Message> {
        let editor = &self.profiles;
        let draft = editor.studio.as_ref().unwrap();
        let saved = draft.saved();
        let protected = saved
            .as_ref()
            .is_some_and(|id| id.as_str() == "before-themes");
        let mut library = vec![
            button::standard("Save as new look")
                .id(cosmic::widget::Id::new("studio-save-as"))
                .on_press_maybe(writable.then(|| studio(Studio::SaveAsNew)))
                .into(),
        ];
        if saved.is_some() {
            library.push(
                button::standard("Duplicate")
                    .id(cosmic::widget::Id::new("studio-duplicate"))
                    .on_press_maybe(writable.then(|| studio(Studio::Duplicate)))
                    .into(),
            );
            library.push(
                button::standard("Rename")
                    .id(cosmic::widget::Id::new("studio-rename"))
                    .on_press_maybe(
                        (writable && !protected && draft.renamed()).then(|| studio(Studio::Rename)),
                    )
                    .into(),
            );
        }
        library.push(
            button::standard("Discard changes")
                .id(cosmic::widget::Id::new("studio-discard"))
                .on_press_maybe(draft.dirty().then(|| studio(Studio::Discard)))
                .into(),
        );
        if saved.is_some() && !protected {
            library.push(
                button::destructive("Delete…")
                    .id(cosmic::widget::Id::new("studio-delete"))
                    .on_press_maybe(writable.then(|| studio(Studio::Delete(false))))
                    .into(),
            );
        }
        let built = vec![
            button::standard("Make built-in")
                .id(cosmic::widget::Id::new("studio-built-in"))
                .on_press_maybe(writable.then(|| studio(Studio::MakeBuiltIn)))
                .into(),
            button::standard("Make built-in as a file…")
                .id(cosmic::widget::Id::new("studio-built-in-as"))
                .on_press_maybe(writable.then(|| studio(Studio::MakeBuiltInAs)))
                .into(),
        ];
        section(vec![
            label("Your looks", 12., MUTED),
            row::with_children(library).spacing(6).wrap().into(),
            label("For Halcyon itself", 12., MUTED),
            row::with_children(built).spacing(6).wrap().into(),
            label(
                "A built-in look ships with Halcyon. It keeps only Halcyon wallpapers and \
                 leaves out personal pictures and files.",
                12.,
                MUTED,
            ),
        ])
    }

    fn studio_built<'a>(&'a self, built: &'a Built) -> Element<'a, Message> {
        match built {
            Built::Written {
                path,
                removed,
                fallbacks,
            } => {
                let mut lines = vec![
                    label("Built-in look written", 14., FG),
                    container(text::monotext(path.display().to_string()).size(12))
                        .id(cosmic::widget::Id::new("studio-built-path"))
                        .into(),
                ];
                if !removed.is_empty() {
                    lines.push(label(
                        format!("Left out: {}.", removed.join(", ")),
                        12.,
                        MUTED,
                    ));
                }
                for fallback in fallbacks {
                    lines.push(label(fallback.clone(), 12., MUTED));
                }
                lines.push(label(
                    "To ship it, copy the file into resources/looks and build Halcyon.",
                    12.,
                    MUTED,
                ));
                section(vec![
                    column::with_children(lines).spacing(6).into(),
                    row::with_children(vec![
                        button::standard("Copy path")
                            .id(cosmic::widget::Id::new("studio-copy-path"))
                            .leading_icon(icon::from_name("edit-copy-symbolic").size(14))
                            .on_press(studio(Studio::CopyPath))
                            .into(),
                    ])
                    .into(),
                ])
            }
            Built::Refused { message, wallpaper } => {
                let mut actions = Vec::new();
                if *wallpaper {
                    actions.push(
                        button::standard("Choose a Halcyon wallpaper")
                            .id(cosmic::widget::Id::new("studio-built-in-wallpaper"))
                            .on_press(studio(Studio::Open(Some(Part::Wallpaper))))
                            .into(),
                    );
                }
                banner(message.clone(), WARN, actions)
            }
        }
    }

    fn studio_tiles(&self) -> Element<'_, Message> {
        let editor = &self.profiles;
        let draft = editor.studio.as_ref().unwrap();
        let width = self.studio_tile_width();
        let size = [width - 16., ((width - 16.) * 0.5625).round()];
        let backdrop = self.studio_backdrop();
        let tiles = Part::ALL
            .into_iter()
            .map(|part| {
                let thumb = self.studio_thumb(
                    part,
                    &draft.profile,
                    None,
                    editor.studio_icons(),
                    backdrop.clone(),
                    size,
                );
                let tile = self.studio_tile(
                    thumb,
                    part.title().into(),
                    self.studio_summary(part, &draft.profile),
                    false,
                    format!("studio-tile-{}", part.key()),
                    Some(studio(Studio::Open(Some(part)))),
                    width,
                );
                cosmic::widget::mouse_area(tile)
                    .on_enter(edit(Edit::Highlight(Some(part.domain()))))
                    .on_exit(edit(Edit::Highlight(None)))
                    .into()
            })
            .collect::<Vec<Element<'_, Message>>>();
        column::with_children(vec![
            label(
                "Choose each part by picture. Nothing on your desktop changes until you apply.",
                12.,
                MUTED,
            ),
            row::with_children(tiles).spacing(12).wrap().into(),
        ])
        .spacing(10)
        .into()
    }

    fn studio_picker(&self, part: Part) -> Element<'_, Message> {
        let editor = &self.profiles;
        let draft = editor.studio.as_ref().unwrap();
        let width = self.studio_tile_width();
        let size = [width - 16., ((width - 16.) * 0.5625).round()];
        let mut items: Vec<Element<'_, Message>> = vec![split(
            column::with_children(vec![
                text::heading(format!("Choose {}", part.title().to_lowercase())).into(),
                label(self.studio_summary(part, &draft.profile), 12., MUTED),
            ])
            .spacing(2)
            .into(),
            button::suggested("Done")
                .id(cosmic::widget::Id::new("studio-picker-done"))
                .on_press(studio(Studio::Open(None)))
                .into(),
        )];
        if matches!(part, Part::Font | Part::Icons) {
            items.push(
                text_input(
                    if part == Part::Font {
                        "Find a font…"
                    } else {
                        "Find an icon theme…"
                    },
                    &draft.query,
                )
                .id(cosmic::widget::Id::new("studio-search"))
                .on_input(|query| studio(Studio::Query(query)))
                .into(),
            );
        }
        let backdrop = self.studio_backdrop();
        let choices = editor
            .studio_choices()
            .iter()
            .enumerate()
            .map(|(index, choice)| {
                let wallpaper = if part == Part::Wallpaper {
                    self.choice_wallpaper(&choice.pick, &choice.profile)
                } else {
                    backdrop.clone()
                };
                let icons = if part == Part::Icons {
                    choice.icons.as_slice()
                } else {
                    editor.studio_icons()
                };
                let thumb = self.studio_thumb(
                    part,
                    &choice.profile,
                    choice.preview.as_ref(),
                    icons,
                    wallpaper,
                    size,
                );
                self.studio_tile(
                    thumb,
                    choice.title.clone(),
                    choice.source.into(),
                    choice.selected,
                    format!("studio-choice-{}-{index}", part.key()),
                    Some(studio(Studio::Pick(part, choice.pick.clone()))),
                    width,
                )
            })
            .collect::<Vec<_>>();
        if choices.is_empty() {
            items.push(label("Nothing matches this search.", 13., MUTED));
        } else {
            items.push(row::with_children(choices).spacing(12).wrap().into());
        }
        column::with_children(items).spacing(12).into()
    }

    fn studio_review(&self, accent: Color) -> Element<'_, Message> {
        let editor = &self.profiles;
        let mut items: Vec<Element<'_, Message>> = vec![split(
            column::with_children(vec![
                text::heading("Review and apply").into(),
                label(
                    "Choose which parts go to your desktop. Undo stays available after.",
                    12.,
                    MUTED,
                ),
            ])
            .spacing(2)
            .into(),
            button::standard("Back to editing")
                .id(cosmic::widget::Id::new("studio-review-back"))
                .on_press(studio(Studio::Review(false)))
                .into(),
        )];
        if let Some(profile) = editor.profile() {
            items.push(self.parts(profile, accent));
        }
        items.extend(self.plan_states());
        column::with_children(items).spacing(16).into()
    }

    /// The footer while the studio is open.
    pub(super) fn studio_actions(&self) -> Element<'_, Message> {
        let editor = &self.profiles;
        let Some(draft) = &editor.studio else {
            return cosmic::widget::space().into();
        };
        let ready = editor.can_apply()
            && !self.native_busy
            && !editor.theme_loading()
            && editor.theme_problem().is_none();
        let problem = self.page_error().unwrap_or_else(|| {
            editor.theme_problem().unwrap_or(if self.native_busy {
                "Saving…"
            } else if editor.theme_loading() {
                "Finding wallpaper colors…"
            } else if !editor.can_apply() {
                "Matches your desktop"
            } else {
                "Your desktop hasn’t changed yet"
            })
        });
        let apply: Element<'_, Message> = if draft.reviewing {
            button::suggested(self.apply_label())
                .id("profiles-confirm".into())
                .on_press_maybe(ready.then_some(edit(Edit::Apply)))
                .into()
        } else {
            button::suggested("Review and apply…")
                .id(cosmic::widget::Id::new("studio-review"))
                .on_press_maybe(ready.then(|| studio(Studio::Review(true))))
                .into()
        };
        let status = if self.page_error().is_some() {
            self.error_line(problem)
        } else {
            label(problem, 12., MUTED).into()
        };
        self.preview_bar(
            status,
            false,
            Vec::new(),
            studio(Studio::Close),
            "studio-close",
            apply,
        )
    }

    /// "New look" choices on Looks.
    pub(super) fn studio_starts(&self) -> Vec<Element<'_, Message>> {
        let editor = &self.profiles;
        let ready = editor.writable() && !editor.busy() && !self.native_busy;
        vec![
            button::standard("New look from my desktop")
                .id(cosmic::widget::Id::new("studio-new-desktop"))
                .on_press_maybe(ready.then(|| studio(Studio::Start(Start::Desktop))))
                .into(),
            button::standard("New plain look")
                .id(cosmic::widget::Id::new("studio-new-plain"))
                .on_press_maybe(ready.then(|| studio(Studio::Start(Start::Plain))))
                .into(),
        ]
    }

    /// "Edit" for a saved look, "Edit a copy" for a built-in one.
    pub(super) fn studio_edit_button(&self, available: bool) -> Option<Element<'_, Message>> {
        let editor = &self.profiles;
        if editor.mix.is_some() || self.page != Page::DesktopProfiles {
            return None;
        }
        let profile = editor.profile()?;
        if profile.appearance_only {
            return None;
        }
        let (title, start) = match editor.selection() {
            Selection::Saved(id) if id.as_str() != "before-themes" => {
                ("Edit in studio…", Start::Saved(id.clone()))
            }
            _ => ("Edit a copy in studio…", Start::Selected),
        };
        Some(
            button::text(title)
                .id(cosmic::widget::Id::new("studio-edit"))
                .leading_icon(icon::from_name("document-edit-symbolic").size(14))
                .on_press_maybe(available.then(|| studio(Studio::Start(start))))
                .into(),
        )
    }
}

fn line_text<'a>(text: String) -> Element<'a, Message> {
    text::caption(text)
        .class(cosmic::theme::Text::Color(MUTED))
        .wrapping(cosmic::iced::widget::text::Wrapping::None)
        .into()
}
