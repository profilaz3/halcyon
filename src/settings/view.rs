mod adaptive;
mod automation;
mod calendar;
mod canvas;
mod changes;
mod color_well;
mod colors;
mod design;
mod desktop;
pub(super) use desktop::host::Host;
mod desktop_icons;
pub(super) mod explain;
pub(super) mod home;
mod layered;
mod materials;
mod moments;
mod motion;
mod navigation;
mod pages;
mod preview;
mod privacy;
pub(super) mod profiles;
mod rain;
mod recipes;
mod recovery;
mod reveal;
mod sample;
mod scene;
mod setup;
pub(super) mod sharing;
mod shortcuts;
mod sidebar;
mod slider_completion;
mod support;
mod system_appearance;
mod themes;
mod tools;
mod updates;
mod wallpaper;
mod widget_page;
mod widget_text;
mod window_rules;
mod windows;
mod wizard;
mod workspaces;

use super::*;
use crate::config::Palette;
pub(super) use automation::time_id as automation_time_id;
use cosmic::Apply;
use cosmic::iced::{Alignment, Background, Border, Color};
use cosmic::widget::{
    button, column, container, icon, mouse_area, row, scrollable, slider, text, text_input, toggler,
};
pub(super) use pages::AppSettingsPage;
pub(super) use slider_completion::completed_slider;

const FG: Color = Color::from_rgb(0.94, 0.93, 0.97);
const MUTED: Color = Color::from_rgb(0.65, 0.64, 0.71);
const SURFACE: Color = Color::from_rgb(0.13, 0.13, 0.16);
const BASE: Color = Color::from_rgb(0.09, 0.09, 0.115);
const LINE: Color = Color::from_rgb(0.22, 0.22, 0.27);

pub(super) fn text_style(color: Color) -> cosmic::theme::Text {
    if color == FG {
        cosmic::theme::Text::Default
    } else if color == MUTED {
        cosmic::theme::Text::Custom(|theme| cosmic::iced::widget::text::Style {
            color: Some(Color {
                a: if theme.theme_type.is_high_contrast() {
                    1.
                } else {
                    0.75
                },
                ..theme.cosmic().on_bg_color().into()
            }),
            ..Default::default()
        })
    } else if color == cosmic::theme::active().cosmic().accent_text_color().into() {
        cosmic::theme::Text::Custom(|theme| cosmic::iced::widget::text::Style {
            color: Some(theme.cosmic().accent_text_color().into()),
            ..Default::default()
        })
    } else if color == desktop::WARN || color == Color::from_rgb8(245, 190, 107) {
        cosmic::theme::Text::Custom(|theme| cosmic::iced::widget::text::Style {
            color: Some(theme.cosmic().warning_text_color().into()),
            ..Default::default()
        })
    } else if color == desktop::ERROR || color == Color::from_rgb8(245, 149, 138) {
        cosmic::theme::Text::Custom(|theme| cosmic::iced::widget::text::Style {
            color: Some(theme.cosmic().destructive_text_color().into()),
            ..Default::default()
        })
    } else {
        cosmic::theme::Text::Color(color)
    }
}
fn surface_color(theme: &cosmic::Theme, color: Color) -> Color {
    if color == SURFACE {
        theme.current_container().component.base.into()
    } else if color == BASE {
        theme.cosmic().primary(theme.transparent).base.into()
    } else if color == LINE {
        theme.current_container().component.divider.into()
    } else if color == FG {
        theme.cosmic().on_bg_color().into()
    } else if color == cosmic::theme::active().cosmic().accent_text_color().into() {
        theme.cosmic().accent_text_color().into()
    } else if color == desktop::WARN {
        theme.cosmic().warning_text_color().into()
    } else if color == desktop::ERROR {
        theme.cosmic().destructive_text_color().into()
    } else {
        color
    }
}

fn label<'a>(value: impl Into<String>, size: f32, color: Color) -> Element<'a, Message> {
    text::body(value.into())
        .size(size)
        .line_height(1.25)
        .class(text_style(color))
        .into()
}
fn frame<'a>(
    content: Element<'a, Message>,
    color: Color,
    border: Color,
    padding: u16,
) -> Element<'a, Message> {
    container(column::with_children(vec![content]).width(Length::Fill))
        .width(Length::Fill)
        .padding(padding)
        .class(cosmic::theme::Container::custom(move |theme| {
            cosmic::iced::widget::container::Style {
                background: Some(Background::Color(surface_color(theme, color))),
                border: Border {
                    color: surface_color(theme, border),
                    width: if border == LINE && !theme.theme_type.is_high_contrast() {
                        0.
                    } else {
                        1.
                    },
                    radius: theme.cosmic().corner_radii.radius_s.into(),
                },
                ..Default::default()
            }
        }))
        .into()
}
/// A setting's title with its explanation behind a "?" (see [`explain`]).
/// Only for text that explains; live state goes through [`detail_state`].
fn detail<'a>(title: impl Into<String>, description: impl Into<String>) -> Element<'a, Message> {
    let title = title.into();
    let heading = label(title.clone(), 14., FG);
    explain::titled(heading, &title, description.into(), 12., 3)
}
/// A title with a line under it that is always shown: current state,
/// values, warnings and the next step.
fn detail_state<'a>(
    title: impl Into<String>,
    state: impl Into<String>,
) -> Element<'a, Message> {
    let state = state.into();
    let mut lines = vec![label(title, 14., FG)];
    if !state.is_empty() {
        lines.push(label(state, 12., MUTED));
    }
    column::with_children(lines)
        .spacing(3)
        .width(Length::Fill)
        .into()
}
fn split<'a>(left: Element<'a, Message>, right: Element<'a, Message>) -> Element<'a, Message> {
    adaptive::field(left, right)
}
fn switch<'a>(
    title: &'static str,
    description: &'static str,
    value: bool,
    message: impl Fn(bool) -> Message + 'a,
) -> Element<'a, Message> {
    split(
        detail(title, description),
        toggler(value).on_toggle(message).into(),
    )
}
fn heading<'a>(title: &'static str, description: &'static str) -> Element<'a, Message> {
    let big = text::title3(title).size(28).into();
    explain::titled(big, title, description.to_owned(), 13., 4)
}
fn separator() -> Element<'static, Message> {
    container(text::body(""))
        .height(1)
        .width(Length::Fill)
        .class(cosmic::theme::Container::custom(|theme| {
            cosmic::iced::widget::container::Style {
                background: Some(Background::Color(surface_color(theme, LINE))),
                ..Default::default()
            }
        }))
        .into()
}
/// A titled native COSMIC list: rows separated by thin dividers, the
/// heading outside the box. Used by the grouped settings pages.
pub(super) fn group<'a>(
    title: Option<&'a str>,
    rows: Vec<Element<'a, Message>>,
) -> Element<'a, Message> {
    let mut section = cosmic::widget::settings::section();
    if let Some(title) = title {
        section = section.title(title);
    }
    section.extend(rows).into()
}
/// One labelled row of a [`group`]: title (and optional muted description)
/// left, control right.
pub(super) fn group_row<'a>(
    title: impl Into<String>,
    description: impl Into<String>,
    control: Element<'a, Message>,
) -> Element<'a, Message> {
    // A wrapping row keeps segmented controls sized to their labels.
    split(
        detail(title, description),
        row::with_children(vec![control])
            .align_y(Alignment::Center)
            .wrap()
            .into(),
    )
}
/// A [`group_row`] whose line under the title is live state, always shown.
pub(super) fn group_row_state<'a>(
    title: impl Into<String>,
    state: impl Into<String>,
    control: Element<'a, Message>,
) -> Element<'a, Message> {
    split(
        detail_state(title, state),
        row::with_children(vec![control])
            .align_y(Alignment::Center)
            .wrap()
            .into(),
    )
}
fn pill<'a>(value: impl Into<String>, color: Color) -> Element<'a, Message> {
    container(label(value, 11., color))
        .padding([4, 9])
        .class(cosmic::theme::Container::custom(move |theme| {
            cosmic::iced::widget::container::Style {
                background: Some(Background::Color(Color {
                    a: 0.12,
                    ..surface_color(theme, color)
                })),
                border: Border {
                    radius: 20.into(),
                    ..Default::default()
                },
                ..Default::default()
            }
        }))
        .into()
}
/// One-of-several choice with COSMIC's selection and focus states.
pub(super) fn segmented<'a>(
    options: Vec<(String, bool, Option<Message>)>,
    accent: Color,
) -> Element<'a, Message> {
    let buttons: Vec<Element<'a, Message>> = options
        .into_iter()
        .map(|(title, selected, message)| {
            let id = match &message {
                Some(Message::DesktopEditor(crate::settings::desktop::Message::RadiusChoice(
                    radius,
                ))) => cosmic::widget::Id::new(format!("native-bar-radius-{radius}")),
                Some(Message::DesktopEditor(
                    crate::settings::desktop::Message::Islands(
                        crate::settings::desktop::islands::Edit::Enabled(true),
                    )
                    | crate::settings::desktop::Message::PanelStyle(
                        crate::settings::desktop::PanelStyle::Islands,
                    ),
                )) => cosmic::widget::Id::new("native-islands-style"),
                _ => cosmic::widget::Id::unique(),
            };
            // An option that can't be picked right now is dimmed.
            let ink = if selected {
                FG
            } else if message.is_none() {
                Color { a: 0.35, ..MUTED }
            } else {
                MUTED
            };
            button::custom(label(title, 12., ink))
                .id(id)
                .class(nav_style(selected, accent))
                .selected(selected)
                .padding([6, 12])
                .on_press_maybe(message)
                .into()
        })
        .collect();
    container(row::with_children(buttons).spacing(2).wrap())
        .padding(3)
        .class(cosmic::theme::Container::custom(|theme| {
            cosmic::iced::widget::container::Style {
                background: Some(Background::Color(surface_color(theme, BASE))),
                border: Border {
                    radius: 12.into(),
                    ..Default::default()
                },
                ..Default::default()
            }
        }))
        .into()
}
/// Standard COSMIC selection, hover and keyboard focus colors.
fn chip_style(on: bool, accent: Color) -> cosmic::theme::Button {
    nav_style(on, accent)
}
pub(super) fn nav_style(selected: bool, _accent: Color) -> cosmic::theme::Button {
    let style = move |theme: &cosmic::Theme, focused: bool, hovered: bool| {
        let component = &theme.current_container().component;
        cosmic::widget::button::Style {
            background: Some(Background::Color(if hovered {
                component.hover.into()
            } else if selected {
                let base: Color = component.base.into();
                let accent: Color = theme.cosmic().accent.base.into();
                let strength = if theme.theme_type.is_high_contrast() {
                    0.35
                } else {
                    0.16
                };
                Color::from_rgb(
                    base.r + (accent.r - base.r) * strength,
                    base.g + (accent.g - base.g) * strength,
                    base.b + (accent.b - base.b) * strength,
                )
            } else {
                Color::TRANSPARENT
            })),
            border_radius: theme.cosmic().corner_radii.radius_s.into(),
            outline_width: if focused { 2. } else { 0. },
            outline_color: theme.cosmic().accent.base.into(),
            ..Default::default()
        }
    };
    cosmic::theme::Button::Custom {
        active: Box::new(move |focused, theme| style(theme, focused, false)),
        hovered: Box::new(move |focused, theme| style(theme, focused, true)),
        pressed: Box::new(move |focused, theme| style(theme, focused, true)),
        disabled: Box::new(move |theme| style(theme, false, false)),
    }
}

/// Search context uses the same system accent as the rest of settings.
pub(super) fn area_tint(_area: super::navigation::Area, accent: Color) -> Color {
    accent
}
/// A small filled dot.
pub(super) fn dot<'a>(color: Color, size: f32) -> Element<'a, Message> {
    container(cosmic::iced::widget::space().width(size).height(size))
        .class(cosmic::theme::Container::custom(move |_| {
            cosmic::iced::widget::container::Style {
                background: Some(Background::Color(color)),
                border: Border {
                    radius: (size / 2.).into(),
                    ..Default::default()
                },
                ..Default::default()
            }
        }))
        .into()
}

impl Settings {
    /// A control's title. Controls explain themselves through their label
    /// and, where needed, a one-line description; there are no help bubbles.
    fn help(
        &self,
        _key: &'static str,
        title: &'static str,
        _explanation: &'static str,
    ) -> Element<'_, Message> {
        label(title, 14., FG)
    }
    /// The one way to fold extra content away: a full-width row with a
    /// chevron; the content opens inside the same card.
    fn disclosure<'a>(
        &'a self,
        key: &'static str,
        title: &'static str,
        content: Element<'a, Message>,
    ) -> Element<'a, Message> {
        let open = self.open_sections.contains(key);
        // One child, so the closed section adds no gap under its title.
        pages::section(vec![column::with_children(vec![
            button::custom(split(
                label(title, 14., FG),
                icon::from_name(if open {
                    "go-up-symbolic"
                } else {
                    "go-down-symbolic"
                })
                .size(16)
                .icon()
                .into(),
            ))
            .id(cosmic::widget::Id::new(key))
            .class(nav_style(false, self.settings_accent()))
            .width(Length::Fill)
            // Same inset as the rows inside cards, so labels line up.
            .padding([4, 10])
            .on_press(Message::ToggleSection(key))
            .into(),
            reveal::reveal(
                container(content).padding([10, 0, 0, 0]).into(),
                open,
                self.config.settings_animations && self.system_animations,
            ),
        ])
        .into()])
    }
    /// At the supported minimum the sidebar becomes an icon and label rail.
    pub(super) fn rail(&self) -> bool {
        self.window.width < 960.
    }
    pub(in crate::settings) fn sidebar_width(&self) -> f32 {
        if self.rail() {
            84.
        } else {
            (self.window.width * 0.2).clamp(176., 196.)
        }
    }
    fn content_width(&self) -> f32 {
        (self.window.width - self.sidebar_width() - 40.).max(240.)
    }
    fn inspector_height(&self) -> f32 {
        (self.window.height - 200.).max(260.)
    }
    fn stage_width(&self) -> f32 {
        let width = self.content_width();
        let scene = self.canvas_visible()
            || matches!(
                self.page,
                Page::Overview | Page::DesktopProfiles | Page::DesktopWallpaper
            );
        if scene && width >= 584. {
            width - (width * 0.40).clamp(248., 420.) - 16.
        } else if matches!(self.page, Page::Themes | Page::DesktopDock) && width >= 576. {
            (width - 16.) / 2.
        } else if !scene && width >= 600. {
            (width * 0.34).clamp(224., 340.)
        } else {
            width
        }
    }
    fn stage_height(&self) -> f32 {
        if self.canvas_visible()
            || matches!(
                self.page,
                Page::Overview | Page::Themes | Page::DesktopProfiles | Page::DesktopWallpaper
            )
        {
            (self.stage_width() * 0.625).clamp(180., self.inspector_height() - 48.)
        } else {
            (self.stage_width() * 0.625).clamp(130., 210.)
        }
    }
    fn inspector_layout<'a>(
        &'a self,
        stage: Element<'a, Message>,
        controls: Element<'a, Message>,
        id: &'static str,
    ) -> Element<'a, Message> {
        let pattern = match id {
            "desktop" => adaptive::Pattern::Workspace,
            "widget" | "calendar" | "dock" => adaptive::Pattern::Balanced,
            "setup" | "mix" | "package" | "canvas" | "wallpaper" => adaptive::Pattern::Scene,
            "content" | "notes" | "projects" => adaptive::Pattern::Form,
            _ => adaptive::Pattern::Controls,
        };
        // The page is the only scroll owner; a composition pins its picture.
        adaptive::page(
            container(stage)
                .id(cosmic::widget::Id::new(format!("{id}-stage")))
                .width(Length::Fill)
                .into(),
            container(controls)
                .width(Length::Fill)
                .id(cosmic::widget::Id::new(format!("{id}-inspector")))
                .into(),
            match pattern {
                adaptive::Pattern::Balanced => [280., 280.],
                adaptive::Pattern::Scene => [320., 248.],
                adaptive::Pattern::Workspace => [360., 200.],
                adaptive::Pattern::Form => [360., 224.],
                _ => [
                    224.,
                    if matches!(id, "theme" | "fonts") && self.color_well.is_some() {
                        // 300 px picker, 28 px card padding, 48 px enclosing
                        // section padding and the 8 px scroll gutter.
                        384.
                    } else {
                        360.
                    },
                ],
            },
            pattern,
        )
    }
    pub(super) fn settings_accent(&self) -> Color {
        cosmic::theme::active().cosmic().accent_text_color().into()
    }
    pub(super) fn settings_view(&self) -> Element<'_, Message> {
        // Setup and recovery explain every step in full.
        explain::begin(explain::Frame {
            show: self.config.show_explanations
                || self.wizard.is_some()
                || self.config.design_problem.is_some(),
            open: self
                .explained
                .and_then(|(page, key)| (page == self.page).then_some(key)),
        });
        if self.config.design_problem.is_some() {
            return self.design_recovery_view();
        }
        if let Some(wizard) = &self.wizard {
            return self.wizard_view(wizard);
        }
        let accent = self.settings_accent();
        let sidebar = self.sidebar(accent);
        let searching = !self.search_query.trim().is_empty() || self.search_open;
        let page = match self.page {
            Page::Overview | Page::WidgetLayout | Page::Widgets | Page::Desktop => {
                self.overview_page()
            }
            Page::PendingChanges => self.pending_changes_page(),
            Page::Changes => self.changes_page(),
            Page::Appearance => self.appearance_page(),
            Page::Glance => self.glance_page(),
            Page::Calendar => self.calendar_page(),
            Page::Weather => self.weather_page(),
            Page::Devbox => self.devbox_page(),
            Page::Clock => self.clock_page(),
            Page::Projects => self.projects_page(),
            Page::Music => self.music_page(),
            Page::Notes => self.notes_page(),
            Page::Focus => self.focus_page(),
            Page::General => self.general_page(),
            Page::DesktopDock => {
                column::with_children(vec![self.desktop_dock_header(), self.dock_workspace()])
                    .spacing(20)
                    .into()
            }
            // On a short viewport the sample window scrolls with the controls
            // so the basic controls stay visible.
            Page::DesktopAppearance if !self.appearance_pinned() => column::with_children(vec![
                self.system_appearance_header(),
                self.system_appearance_page(),
            ])
            .spacing(20)
            .into(),
            Page::DesktopAppearance => self.system_appearance_page(),
            // Like System appearance: the header scrolls with the content
            // on a short viewport.
            // Wallpaper & colors is one page: the title and large preview
            // scroll with the gallery and color controls.
            Page::DesktopWallpaper => self.wallpaper_page(),
            Page::DesktopIcons => self.desktop_icons_page(),
            Page::DesktopProfiles => self.profiles_page(),
            Page::Themes => self.themes_page(),
            // Windows and Workspaces share one page: windows first.
            Page::DesktopWindows => column::with_children(vec![
                self.windows_page(),
                container(self.workspaces_section(true))
                    .id(cosmic::widget::Id::new("windows-workspaces"))
                    .into(),
                group(
                    None,
                    vec![
                        self.subpage_row(
                            "Apps that don’t tile",
                            "They open as normal windows when tiling is on",
                            Page::DesktopWindowRules,
                            "windows-open-rules",
                        ),
                        self.subpage_row(
                            "Keyboard shortcuts",
                            "Keys for windows, workspaces and apps",
                            Page::DesktopShortcuts,
                            "windows-open-shortcuts",
                        ),
                    ],
                ),
            ])
            .spacing(16)
            .into(),
            Page::DesktopWorkspaces => self.workspaces_page(),
            Page::DesktopMotion => self.motion_page(),
            Page::DesktopWindowRules => self.window_rules_page(),
            Page::DesktopShortcuts => self.shortcuts_page(),
            Page::DesktopAutomation => self.automation_page(),
            Page::Moments => self.automation_page(),
            Page::AdaptiveLooks => self.automation_page(),
            Page::Builder => self.recipe_page(),
        };
        let pinned = !searching && self.page == Page::DesktopAppearance && self.appearance_pinned();
        // Unpinned, System appearance keeps a fixed strip above its scroll
        // area instead of the top padding: a hex field scrolled across the
        // top edge would otherwise paint into the window header bar.
        // Profiles has a name field in its scrolling content too.
        // Every other page scrolls below a fixed strip, so titles start at
        // the same height everywhere and a scrolled text field's escaped text
        // stays covered.
        let strip = !searching && !pinned;
        let mut page_parts = Vec::new();
        if let Some(hit) = self.search_selected {
            page_parts.push(pill(
                format!(
                    "Found: {}{}",
                    hit.label,
                    if hit.advanced {
                        " · Advanced is open below"
                    } else {
                        ""
                    }
                ),
                accent,
            ));
        }
        if !searching
            && matches!(
                self.page,
                Page::Themes | Page::DesktopProfiles | Page::AdaptiveLooks
            )
            && !self.profiles.conflicts().is_empty()
        {
            page_parts.push(self.preview_conflicts());
        }
        // libcosmic reuses widget state by name and position, and a named widget
        // replaced by an unnamed one panics. A new name for each distinct screen
        // state rebuilds its tree instead of mismatching it.
        page_parts.push(
            container(
                container(page)
                    .id(cosmic::widget::Id::new(self.screen_key()))
                    .width(Length::Fill),
            )
            .id(cosmic::widget::Id::new("page-start"))
            .width(Length::Fill)
            .into(),
        );
        let page = if searching {
            self.search_results_page()
        } else {
            column::with_children(page_parts).spacing(12).into()
        };
        let scrolling = scrollable(container(page).width(Length::Fill).padding(if pinned {
            [16, 22, 24, 22]
        } else if strip {
            [0, 20, 20, 20]
        } else {
            [12, 20, 20, 20]
        }))
        .id(cosmic::widget::Id::new("settings-content"))
        .on_scroll(|viewport| Message::ContentScrolled(viewport.absolute_offset().y))
        .width(Length::Fill)
        .height(Length::Fill);
        let scrolling = container(scrolling)
            .id(cosmic::widget::Id::new("settings-viewport"))
            .width(Length::Fill)
            .height(Length::Fill);
        // Dock & Panel keeps its miniature and selector pinned above the
        // scrolling controls, so the preview stays in view while editing.
        // Fixed parts are drawn over the scroll area (see `layered`).
        let content: Element<'_, Message> = if pinned {
            layered::layered(
                vec![
                    layered::opaque(column::with_children(vec![
                        container(match self.page {
                            Page::DesktopDock => self.desktop_dock_header(),
                            _ => self.system_appearance_header(),
                        })
                        .width(Length::Fill)
                        .padding([14, 22, 12, 22])
                        .into(),
                        separator(),
                    ])),
                    scrolling.into(),
                ],
                1,
            )
        } else if strip {
            layered::layered(
                vec![
                    // Taller than a text field, so its escaped text is covered.
                    layered::opaque(cosmic::iced::widget::space().height(36)),
                    scrolling.into(),
                ],
                1,
            )
        } else {
            scrolling.into()
        };
        let status: Element<'_, Message> = if searching || self.undo_review.is_some() {
            self.simple_status()
        } else if self.canvas_visible() {
            self.canvas_actions()
        } else if self.page == Page::Builder {
            self.recipe_actions()
        } else if self.page == Page::DesktopWallpaper {
            self.wallpaper_actions()
        } else if matches!(self.page, Page::DesktopProfiles | Page::Themes) {
            self.profiles_actions()
        } else {
            self.simple_status()
        };
        let status = container(status)
            .id(cosmic::widget::Id::new(self.status_key()))
            .width(Length::Fill)
            .into();
        row::with_children(vec![
            sidebar,
            layered::layered(
                vec![
                    layered::opaque(column::with_children(vec![self.navigation_header()])),
                    layered::layered(
                        vec![
                            content,
                            layered::opaque(column::with_children(vec![separator(), status])),
                        ],
                        0,
                    ),
                ],
                1,
            ),
        ])
        .height(Length::Fill)
        .apply(container)
        // Opaque like the title bar (Settings::view), the fixed header and
        // the footer, so they read as one surface.
        .class(cosmic::theme::Container::custom(|theme: &cosmic::Theme| {
            let base: Color = theme.cosmic().background(theme.transparent).base.into();
            cosmic::iced::widget::container::Style {
                background: Some(Background::Color(Color { a: 1., ..base })),
                ..Default::default()
            }
        }))
        .into()
    }

    /// Names the current screen's shape, so a change of shape starts a new tree.
    fn screen_key(&self) -> String {
        format!(
            "screen-{}-{:?}-{:?}-{}-{}-{}-{}-{}-{}-{}-{}-{}-{}-{:?}",
            self.page.key(),
            self.object_tab,
            self.app_settings_page,
            self.arranging,
            self.profiles.mix.is_some(),
            self.profiles.package.is_some(),
            self.profiles.review_open(),
            self.setup_requested,
            self.setup_followups
                .pending(&self.config, self.calendar.connection.connected())
                .len(),
            self.profiles.name.is_some(),
            self.profiles.conflicts().len(),
            self.profiles.mix.as_ref().is_some_and(|mix| mix.choosing),
            self.profiles.saved_collection,
            self.desktop.selected,
        )
    }
    fn status_key(&self) -> String {
        format!(
            "status-{}-{}-{}-{}-{}",
            self.page.key(),
            self.canvas_visible(),
            self.native_busy,
            self.undo_review.is_some(),
            self.kept_note.is_some(),
        )
    }

    /// Four stable roots. Everything else stays within its owning root.
    /// Entries come from the shared nav model so selection, search and Back
    /// keep one source of truth.
    fn sidebar(&self, _accent: Color) -> Element<'_, Message> {
        let rail = self.rail();
        let kept = self.kept_previews();
        let entry = |id: cosmic::widget::segmented_button::Entity| -> Element<'_, Message> {
            // The pinned Changes entry has no place in the model.
            let active =
                self.nav_model.is_active(id) && self.page.sidebar() != Page::PINNED;
            let page = self.nav_model.data::<Page>(id).copied();
            let waiting = page.is_some_and(|page| kept.iter().any(|item| item.root == page));
            let title = text::body(self.nav_model.text(id).unwrap_or_default().to_owned())
                .size(if rail { 12 } else { 15 })
                .class(if active {
                    cosmic::theme::Text::Custom(|theme| cosmic::iced::widget::text::Style {
                        color: Some(theme.cosmic().accent_text_color().into()),
                        ..Default::default()
                    })
                } else {
                    cosmic::theme::Text::Default
                });
            let mark: Element<'_, Message> = if waiting {
                dot(self.settings_accent(), 7.)
            } else {
                cosmic::iced::widget::space().width(0).into()
            };
            let icon: Element<'_, Message> = match self.nav_model.icon(id) {
                Some(icon) => icon.clone().into(),
                None => cosmic::iced::widget::space().width(0).into(),
            };
            let content: Element<'_, Message> = if rail {
                column::with_children(vec![
                    row::with_children(vec![icon, mark])
                        .spacing(2)
                        .align_y(Alignment::Start)
                        .into(),
                    title.into(),
                ])
                .spacing(2)
                .width(Length::Fill)
                .align_x(Alignment::Center)
                .into()
            } else {
                row::with_children(vec![
                    icon,
                    container(title).width(Length::Fill).into(),
                    mark,
                ])
                .spacing(12)
                .align_y(Alignment::Center)
                .into()
            };
            button::custom(content)
                .id(cosmic::widget::Id::new(format!(
                    "nav-{}",
                    page.map(Page::key).unwrap_or_default()
                )))
                .class(nav_style(active, self.settings_accent()))
                .selected(active)
                // Eight roots and the pinned Changes fit the rail at 820×552.
                .padding(if rail { [5, 2] } else { [11, 12] })
                .width(Length::Fill)
                .on_press(Message::NavSelected(id))
                .into()
        };
        let mut groups: Vec<Element<'_, Message>> = if rail {
            vec![
                button::custom(
                    column::with_children(vec![
                        icon::from_name("system-search-symbolic")
                            .size(18)
                            .icon()
                            .into(),
                        text::body("Search").size(12).into(),
                    ])
                    .spacing(2)
                    .width(Length::Fill)
                    .align_x(Alignment::Center),
                )
                .id(cosmic::widget::Id::new("rail-search"))
                .class(nav_style(self.search_open, self.settings_accent()))
                .selected(self.search_open)
                .padding([5, 2])
                .width(Length::Fill)
                .on_press(Message::FocusSearch)
                .into(),
            ]
        } else {
            let mut search = cosmic::widget::text_input::search_input("Search", &self.search_query)
                .id(super::navigation::search_id())
                .on_input(Message::SearchSettings)
                .on_submit(|_| Message::SearchFirst)
                .width(Length::Fill);
            if !self.search_query.is_empty() {
                search = search.on_clear(Message::SearchSettings(String::new()));
            }
            vec![
                // The app's own root is called Halcyon; this is its settings.
                container(text::title3("Settings").size(22))
                    .padding([14, 12, 6, 12])
                    .into(),
                container(search).padding([0, 2, 12, 2]).into(),
            ]
        };
        for (_, pages) in Page::SIDEBAR {
            let items: Vec<Element<'_, Message>> = pages
                .iter()
                .filter_map(|page| {
                    self.nav_model
                        .iter()
                        .find(|&id| self.nav_model.data::<Page>(id) == Some(page))
                        .map(&entry)
                })
                .collect();
            groups.push(
                column::with_children(items)
                    .spacing(if rail { 2 } else { 6 })
                    .into(),
            );
        }
        if !kept.is_empty() {
            groups.push(if rail {
                button::custom(
                    column::with_children(vec![
                        pill(kept.len().to_string(), self.settings_accent()),
                        text::body("Not applied").size(10).into(),
                    ])
                    .spacing(2)
                    .width(Length::Fill)
                    .align_x(Alignment::Center),
                )
                .id(cosmic::widget::Id::new("rail-not-applied"))
                .class(nav_style(false, self.settings_accent()))
                .padding([6, 0])
                .width(Length::Fill)
                .on_press(Message::Page(Page::Changes))
                .into()
            } else {
                let mut rows = vec![
                    container(label("NOT APPLIED", 11., MUTED))
                        .padding([0, 14, 2, 14])
                        .into(),
                ];
                rows.extend(kept.iter().map(|item| {
                    button::custom(
                        row::with_children(vec![
                            container(label(item.title.clone(), 13., FG))
                                .width(Length::Fill)
                                .into(),
                            if item.conflict {
                                icon::from_name("dialog-warning-symbolic")
                                    .size(14)
                                    .icon()
                                    .into()
                            } else {
                                icon::from_name("go-next-symbolic").size(14).icon().into()
                            },
                        ])
                        .spacing(6)
                        .align_y(Alignment::Center),
                    )
                    .id(cosmic::widget::Id::new(format!("kept-{}", item.title)))
                    .class(nav_style(false, self.settings_accent()))
                    .padding([6, 12])
                    .width(Length::Fill)
                    .on_press(Message::ResumeKept(item.target.clone()))
                    .into()
                }));
                column::with_children(rows).spacing(2).into()
            });
        }
        let pinned = container(self.changes_entry(rail))
            .padding(if rail { [4, 6, 6, 6] } else { [6, 8, 12, 8] })
            .width(Length::Fill);
        let content = container(column::with_children(vec![
            scrollable(
                container(column::with_children(groups).spacing(14))
                    .padding(if rail { [10, 6, 10, 6] } else { [12, 8, 12, 8] })
                    .width(Length::Fill),
            )
            .height(Length::Fill)
            .into(),
            pinned.into(),
        ]))
        .width(self.sidebar_width())
        .height(Length::Fill)
        .class(cosmic::theme::Container::Primary)
        .into();
        let mut entries: Vec<_> = self
            .nav_model
            .iter()
            .filter_map(|entity| {
                self.nav_model.data::<Page>(entity).map(|page| {
                    (
                        cosmic::widget::Id::new(format!("nav-{}", page.key())),
                        Message::NavSelected(entity),
                    )
                })
            })
            .collect();
        // Arrows continue from the last group into the pinned Changes.
        entries.push((
            cosmic::widget::Id::new("nav-changes"),
            Message::OpenRoot(Page::PINNED),
        ));
        sidebar::navigation(content, entries)
    }

    /// The System appearance header stays pinned only while it leaves room
    /// for the first controls below it.
    pub(super) fn appearance_pinned(&self) -> bool {
        false // Visual previews scroll with their controls at every window size.
    }

    /// Without glass, a widget card is a plain fill: how solid it is and
    /// how strong its outline. Corners and fonts have their own homes.
    fn legacy_widget_style(&self) -> Element<'_, Message> {
        let appearance = &self.appearance_draft;
        pages::section(vec![
            self.style_slider(
                "Opacity",
                format!("{}%", appearance.opacity),
                crate::config::MIN_CARD_OPACITY as u16..=100,
                appearance.opacity as u16,
                AppearanceControl::Opacity,
            ),
            self.style_slider(
                "Outline",
                format!("{}%", appearance.border_opacity),
                0..=50,
                appearance.border_opacity as u16,
                AppearanceControl::Border,
            ),
            separator(),
            split(
                detail("Easier to read", "Adds a little more background behind small text on busy wallpapers"),
                container(toggler(appearance.contrast_assist).on_toggle(Message::ContrastAssist))
                    .id(cosmic::widget::Id::new("contrast-assist"))
                    .into(),
            ),
        ])
    }
    fn style_slider(
        &self,
        title: &'static str,
        value_label: String,
        range: std::ops::RangeInclusive<u16>,
        value: u16,
        control: AppearanceControl,
    ) -> Element<'_, Message> {
        column::with_children(vec![
            split(
                label(title, 12., FG),
                label(value_label, 11., self.settings_accent()),
            ),
            slider(range, value, move |value| {
                Message::AppearanceValue(control.clone(), value)
            })
            .step(1u16)
            .on_release(Message::SaveAppearance)
            .width(Length::Fill)
            .apply(|control| completed_slider(control.into(), Message::SaveAppearance)),
        ])
        .spacing(7)
        .into()
    }
}
