//! Dock and panel share an object selector, a collapsible desktop scene and
//! focused behavior, style and contents groups. Commands stay in DesktopEditor.
use super::pages::section;
use super::*;
use crate::desktop::applets::AppletCatalog;
use crate::desktop::panels::{
    Anchor, AutoHide, Background as PanelBackground, MAX_MARGIN, MAX_PADDING,
    MAX_SPACING, Output, PanelDraft, PanelKind, PanelSize, Region, Setting, Slot, Timing,
};
use crate::settings::desktop::Message as Edit;
use cosmic::iced::advanced::text as advanced_text;

mod applets;
pub(super) mod host;
mod islands;
mod panel;
mod penumbra;
pub(super) mod preview;
mod closeup;

pub(super) const WARN: Color = Color::from_rgb(0.96, 0.78, 0.45);
pub(super) const ERROR: Color = Color::from_rgb(0.96, 0.58, 0.54);

fn edit(message: Edit) -> Message {
    Message::DesktopEditor(message)
}

/// One line of text that ends with an ellipsis instead of wrapping.
pub(super) fn line<'a>(value: impl Into<String>, size: f32, color: Color) -> Element<'a, Message> {
    text::body(value.into())
        .size(size)
        .line_height(1.25)
        .wrapping(advanced_text::Wrapping::None)
        .ellipsize(advanced_text::Ellipsize::End(
            advanced_text::EllipsizeHeightLimit::Lines(1),
        ))
        .class(text_style(color))
        .into()
}

/// Label left, control right; the control wraps under at compact widths.
/// Said under controls whose change makes COSMIC rebuild the bar, which
/// restarts every item on it.
pub(super) const RESTARTS: &str = "This restarts the panel";

/// A [`field`] whose change restarts the panel, and says so.
fn restarting<'a>(title: &'static str, control: Element<'a, Message>) -> Element<'a, Message> {
    super::field(
        Some(title.to_owned()),
        None,
        detail_state(title, RESTARTS),
        row::with_children(vec![control])
            .align_y(Alignment::Center)
            .wrap()
            .into(),
    )
}

fn field<'a>(title: &'static str, control: Element<'a, Message>) -> Element<'a, Message> {
    super::field(
        Some(title.to_owned()),
        None,
        label(title, 14., FG),
        row::with_children(vec![control])
            .align_y(Alignment::Center)
            .wrap()
            .into(),
    )
}

pub(super) fn banner<'a>(
    message: impl Into<String>,
    color: Color,
    actions: Vec<Element<'a, Message>>,
) -> Element<'a, Message> {
    let mut parts = vec![
        container(label(message, 12., color))
            .width(Length::Fill)
            .into(),
    ];
    parts.extend(actions);
    container(
        row::with_children(parts)
            .spacing(10)
            .align_y(Alignment::Center),
    )
    .padding([10, 14])
    .width(Length::Fill)
    .class(cosmic::theme::Container::custom(move |_| {
        cosmic::iced::widget::container::Style {
            background: Some(Background::Color(Color { a: 0.10, ..color })),
            border: Border {
                color: Color { a: 0.35, ..color },
                width: 1.,
                radius: 12.into(),
            },
            ..Default::default()
        }
    }))
    .into()
}

/// Shared by every desktop editor: something outside this editor changed a
/// value the draft also edits. Either action settles it and keeps every
/// other edit: take the current value, or keep the edit and write it over
/// the outside change at Apply. Until then Apply reports the conflict.
pub(super) fn conflict_banner<'a>(
    names: String,
    use_current: Message,
    keep_mine: Message,
) -> Element<'a, Message> {
    banner(
        format!("Changed outside this editor: {names}."),
        WARN,
        vec![
            button::standard("Use current value")
                .on_press(use_current)
                .into(),
            button::standard("Keep my change").on_press(keep_mine).into(),
        ],
    )
}

/// Honest display of a value this editor does not offer as a choice.
pub(super) fn kept<'a>(raw: &str) -> Element<'a, Message> {
    let raw = if raw.chars().count() > 24 {
        format!("{}…", raw.chars().take(23).collect::<String>())
    } else {
        raw.to_string()
    };
    pill(format!("Current: {raw} · kept"), WARN)
}

fn choice<'a, T: PartialEq + Clone>(
    setting: &Setting<T>,
    options: Vec<(String, T)>,
    message: impl Fn(T) -> Edit,
    accent: Color,
) -> Element<'a, Message> {
    let current = setting.known();
    let mut parts = vec![segmented(
        options
            .into_iter()
            .map(|(title, value)| {
                let selected = current == Some(&value);
                (title, selected, Some(edit(message(value))))
            })
            .collect(),
        accent,
    )];
    match setting {
        Setting::Unsupported(raw) => parts.push(kept(raw)),
        Setting::Known(_) => {}
    }
    row::with_children(parts)
        .spacing(8)
        .align_y(Alignment::Center)
        .wrap()
        .into()
}

fn value_slider<'a>(
    title: &'static str,
    value_label: String,
    range: std::ops::RangeInclusive<u32>,
    value: u32,
    step: u32,
    accent: Color,
    message: impl Fn(u32) -> Edit + 'a,
) -> Element<'a, Message> {
    column::with_children(vec![
        split(label(title, 14., FG), label(value_label, 13., accent)),
        slider(range, value, move |value| edit(message(value)))
            .on_release(edit(Edit::Apply))
            .step(step)
            .width(Length::Fill)
            .apply(|control| completed_slider(control.into(), edit(Edit::Apply))),
    ])
    .spacing(7)
    .into()
}

impl Settings {
    /// Panel and Dock each have their own page; the header names the bar
    /// it shows. This header scrolls with the inspector at compact sizes.
    pub(super) fn desktop_dock_header(&self) -> Element<'_, Message> {
        let accent = self.settings_accent();
        let editor = &self.desktop;
        let (title, about) = match editor.selected {
            PanelKind::Dock => ("Dock", "The bar with your apps, along an edge of the screen."),
            PanelKind::Panel => ("Panel", "The bar with the clock and system menus."),
        };
        split(
            heading(title, about),
            super::windows::store_pill(editor.store(), accent),
        )
    }

    pub(super) fn dock_workspace(&self) -> Element<'_, Message> {
        let editor = &self.desktop;
        let mut parts = Vec::new();
        if editor.current().is_some_and(|draft| draft.installed()) {
            parts.push(self.scene_frame(self.bar_closeup(), ""));
        }
        parts.push(self.desktop_dock_page());
        column::with_children(parts).spacing(20).into()
    }
    /// Scrolling part of the Dock & Panel page: states, then the controls.
    pub(super) fn desktop_dock_page(&self) -> Element<'_, Message> {
        let editor = &self.desktop;
        let mut items: Vec<Element<'_, Message>> = Vec::new();
        if let Some(error) = editor.unavailable() {
            items.push(banner(
                format!("Desktop settings are unavailable. {error}"),
                ERROR,
                Vec::new(),
            ));
            return column::with_children(items).spacing(16).into();
        }
        if !editor.opened() {
            items.push(label("Reading desktop settings…", 12., MUTED));
            return column::with_children(items).spacing(16).into();
        }
        items.extend(super::windows::recovery_banner(
            editor.recovery(),
            editor.writable(),
            edit(Edit::Recover),
            edit(Edit::KeepRecovery),
        ));
        let Some(draft) = editor.current() else {
            items.push(banner(
                format!("Could not read the {} settings.", editor.selected.title()),
                ERROR,
                Vec::new(),
            ));
            return column::with_children(items).spacing(16).into();
        };
        if !draft.installed() {
            items.push(banner(
                format!(
                    "No {} settings were found. Is cosmic-panel installed?",
                    editor.selected.title()
                ),
                WARN,
                Vec::new(),
            ));
            return column::with_children(items).spacing(16).into();
        }
        if !draft.listed() {
            items.push(banner(
                format!(
                    "The {} is turned off. Changes apply when it is on again.",
                    editor.selected.title()
                ),
                WARN,
                vec![
                    button::text("Open COSMIC Settings ↗")
                        .on_press(Message::OpenDesktop(editor.selected.system_page()))
                        .into(),
                ],
            ));
        }
        if !draft.conflicts().is_empty() {
            let names: Vec<_> = draft
                .conflicts()
                .iter()
                .map(|name| crate::desktop::panels::key_label(name))
                .collect();
            let mut names = names;
            names.dedup();
            items.push(conflict_banner(
                names.join(", "),
                edit(Edit::UseCurrent),
                edit(Edit::KeepMine),
            ));
        }
        // One page, no tabs: its shape, how it looks, where it sits, what it
        // holds. A missing or unhealthy host must not hide a saved Islands
        // shape or the way back to the bar.
        items.push(self.islands_section(draft));
        items.push(self.bar_style(draft));
        items.push(self.bar_position(draft));
        // Both bars edit their items with the same editor.
        items.push(self.bar_items(draft));
        items.push(self.advanced_section(draft));
        items.push(group(None, vec![self.cosmic_link(editor.selected.system_page())]));
        column::with_children(items).spacing(16).into()
    }

    fn bar_position(&self, draft: &PanelDraft) -> Element<'_, Message> {
        let accent = self.settings_accent();
        let anchors = Anchor::ALL
            .into_iter()
            .map(|anchor| (anchor.title().to_string(), anchor))
            .collect();
        let mut outputs: Vec<(String, Output)> = vec![("All displays".into(), Output::All)];
        outputs.extend(
            self.desktop
                .outputs
                .iter()
                .map(|name| (name.clone(), Output::Name(name.clone()))),
        );
        match draft.output() {
            Setting::Known(Output::Name(name)) if !self.desktop.outputs.contains(&name) => {
                outputs.push((format!("{name} (not connected)"), Output::Name(name)));
            }
            Setting::Known(Output::Active) => {
                outputs.push(("Active display".into(), Output::Active));
            }
            _ => {}
        }
        let mut rows = Vec::new();
        // With one display and the default choice there is nothing to pick.
        if self.desktop.outputs.len() > 1 || draft.output().known() != Some(&Output::All) {
            rows.push(restarting(
                "Show on display",
                choice(&draft.output(), outputs, Edit::Output, accent),
            ));
        }
        rows.extend([
            restarting(
                "Screen edge",
                choice(&draft.anchor(), anchors, Edit::Anchor, accent),
            ),
            self.search_target(
                "dock-visibility",
                field(
                    "Visibility",
                    choice(
                        &draft.autohide(),
                        AutoHide::ALL
                            .into_iter()
                            .map(|mode| (mode.title().to_string(), mode))
                            .collect(),
                        Edit::AutoHide,
                        accent,
                    ),
                ),
            ),
        ]);
        group(Some("Position"), rows)
    }

    fn bar_style(&self, draft: &PanelDraft) -> Element<'_, Message> {
        let accent = self.settings_accent();
        let mut sizes: Vec<(String, PanelSize)> = PanelSize::CHOICES
            .into_iter()
            .map(|size| (size.title(), size))
            .collect();
        if let Setting::Known(custom @ PanelSize::Custom(_)) = draft.size() {
            sizes.push((custom.title(), custom));
        }
        let opacity = draft.opacity();
        let opacity_percent = opacity
            .known()
            .map(|value| (value * 100.).round().clamp(0., 100.) as u32);
        let toggle_items: Vec<Element<'_, Message>> = vec![
            container(self.desktop_switch(
                "Floating",
                "Gap from the screen edge",
                draft.floating(),
                Edit::Floating,
            ))
            .width(Length::FillPortion(1))
            .into(),
        ];
        let mut style: Vec<Element<'_, Message>> = Vec::new();
        style.push(restarting(
            "Size",
            choice(&draft.size(), sizes, Edit::Size, accent),
        ));
        style.push(match opacity_percent {
            // Shown as how see-through it is, the opposite of the stored opacity.
            Some(percent) => container(value_slider(
                "See-through",
                format!("{}%", 100 - percent),
                0..=100,
                100 - percent,
                1,
                accent,
                |value| Edit::Opacity(100 - value.min(100) as u8),
            ))
            .id("native-bar-opacity")
            .into(),
            None => field(
                "See-through",
                kept(match &opacity {
                    Setting::Unsupported(raw) => raw,
                    Setting::Known(_) => "",
                }),
            ),
        });
        style.extend(toggle_items);
        // “Match style” is the bar following the look’s style; any other
        // background is the bar’s own. Only while bars follow styles at all
        // (the former switch) is the theme’s background “Match style”.
        let following = self.config.appearance.style_bars;
        let background = draft.background();
        let mut options = vec![
            ("Match style".to_string(), PanelBackground::ThemeDefault),
            ("Dark".into(), PanelBackground::Dark),
            ("Light".into(), PanelBackground::Light),
        ];
        if let Setting::Known(color @ PanelBackground::Color(_)) = &background {
            options.push(("Custom color".into(), color.clone()));
        }
        let mut control = vec![segmented(
            options
                .into_iter()
                .map(|(title, value)| {
                    let selected = background.known() == Some(&value)
                        && (following || value != PanelBackground::ThemeDefault);
                    (title, selected, Some(edit(Edit::Background(value))))
                })
                .collect(),
            accent,
        )];
        if let Setting::Unsupported(raw) = &background {
            control.push(kept(raw));
        }
        style.push(self.search_target(
            "dock-background",
            split(
                if following {
                    detail_state("Background", RESTARTS)
                } else {
                    column::with_children(vec![
                        detail_state(
                            "Background",
                            "Choosing a style leaves the dock and panel as they are",
                        ),
                        label(RESTARTS, CAPTION, MUTED),
                    ])
                    .spacing(3)
                    .width(Length::Fill)
                    .into()
                },
                row::with_children(control)
                    .spacing(8)
                    .align_y(Alignment::Center)
                    .wrap()
                    .into(),
            ),
        ));
        style.push(self.penumbra_controls(draft));
        group(Some("Style"), style)
    }

    fn desktop_switch(
        &self,
        title: &'static str,
        description: &'static str,
        setting: Setting<bool>,
        message: fn(bool) -> Edit,
    ) -> Element<'_, Message> {
        match setting {
            Setting::Known(value) => split(
                detail(title, description),
                toggler(value)
                    .on_toggle(move |value| edit(message(value)))
                    .into(),
            ),
            Setting::Unsupported(raw) => split(detail(title, description), kept(&raw)),
        }
    }

    fn advanced_section(&self, draft: &PanelDraft) -> Element<'_, Message> {
        let accent = self.settings_accent();
        let open = self.desktop.advanced;
        let toggle = button::custom(
            row::with_children(vec![
                label(crate::settings::navigation::BAR_ADVANCED, 14., FG),
                container(text::body("")).width(Length::Fill).into(),
                icon::from_name(if open {
                    "go-up-symbolic"
                } else {
                    "go-down-symbolic"
                })
                .size(16)
                .icon()
                .into(),
            ])
            .spacing(10)
            .align_y(Alignment::Center),
        )
        .class(nav_style(false, accent))
        .padding([4, 10])
        .width(Length::Fill)
        .on_press(edit(Edit::Advanced(!open)));
        if !open {
            return self.search_target("dock-advanced", section(vec![toggle.into()]));
        }
        let mut items: Vec<Element<'_, Message>> = vec![toggle.into()];
        let hides = draft.autohide().known().copied();
        match draft.behavior() {
            Setting::Known(behavior) => {
                let mut heading = vec![label("Hide timing", 13., FG)];
                if hides == Some(AutoHide::Never) {
                    heading.push(label("Used when hiding is on", 12., MUTED));
                }
                items.push(
                    row::with_children(heading)
                        .spacing(8)
                        .align_y(Alignment::Center)
                        .into(),
                );
                for pair in Timing::ALL.chunks(2) {
                    let sliders: Vec<Element<'_, Message>> = pair
                        .iter()
                        .map(|&timing| {
                            let value = timing.get(&behavior);
                            container(value_slider(
                                timing.title(),
                                format!("{value} {}", timing.unit()),
                                timing.range(),
                                value.clamp(*timing.range().start(), *timing.range().end()),
                                timing.step(),
                                accent,
                                move |value| Edit::Timing(timing, value),
                            ))
                            .width(Length::FillPortion(1))
                            .into()
                        })
                        .collect();
                    items.push(row::with_children(sliders).spacing(20).into());
                }
            }
            Setting::Unsupported(raw) => {
                items.push(field("Hide timing", kept(&raw)));
            }
        }
        items.push(separator());
        items.push(label("Spacing", 13., FG));
        // Padding is drawn inside the bar; only the gap between items rebuilds it.
        items.push(label(
            format!("Between items: {}", RESTARTS.to_lowercase()),
            CAPTION,
            MUTED,
        ));
        let number = |setting: Setting<u32>,
                      title: &'static str,
                      max: u32,
                      message: fn(u32) -> Edit|
         -> Element<'_, Message> {
            match setting {
                Setting::Known(value) => container(value_slider(
                    title,
                    if value > max {
                        format!("{value} px · above range")
                    } else {
                        format!("{value} px")
                    },
                    0..=max,
                    value.min(max),
                    1,
                    accent,
                    message,
                ))
                .width(Length::FillPortion(1))
                .into(),
                Setting::Unsupported(raw) => container(field(title, kept(&raw)))
                    .width(Length::FillPortion(1))
                    .into(),
            }
        };
        let floating = draft.floating().known().copied().unwrap_or(false);
        let margin = match draft.margin() {
            Setting::Known(value) => Setting::Known(u32::from(value)),
            Setting::Unsupported(raw) => Setting::Unsupported(raw),
        };
        items.push(
            row::with_children(vec![
                number(
                    draft.spacing(),
                    "Between items",
                    MAX_SPACING,
                    Edit::Spacing,
                ),
                number(draft.padding(), "Inner padding", MAX_PADDING, Edit::Padding),
            ])
            .spacing(20)
            .into(),
        );
        items.push(
            row::with_children(vec![
                if floating {
                    number(margin, "Edge gap", u32::from(MAX_MARGIN), |value| {
                        Edit::Margin(value as u16)
                    })
                } else {
                    container(
                        column::with_children(vec![
                            label("Edge gap", 12., FG),
                            label("Turn on Floating to set a gap.", CAPTION, MUTED),
                        ])
                        .spacing(7),
                    )
                    .width(Length::FillPortion(1))
                    .into()
                },
                container(text::body("")).width(Length::FillPortion(1)).into(),
            ])
            .spacing(20)
            .into(),
        );
        items.push(separator());
        // This bar's own corners, in the steps Appearance's Corners uses. A
        // size set some other way is kept and shown as Custom.
        let radius = draft.radius();
        let mut steps: Vec<(String, u32)> = crate::settings::corners::Step::ALL
            .into_iter()
            .map(|step| (step.title().to_string(), step.bar()))
            .collect();
        if let Setting::Known(value) = radius
            && !steps.iter().any(|(_, step)| *step == value)
        {
            steps.push((format!("Custom · {value} px"), value));
        }
        items.push(self.search_target(
            "dock-corners",
            field(
                if draft.kind() == PanelKind::Panel {
                    "Panel corners"
                } else {
                    "Dock corners"
                },
                choice(&radius, steps, Edit::RadiusChoice, accent),
            ),
        ));
        items.push(separator());
        items.push(self.desktop_switch(
            "Keep style when maximized",
            "Otherwise the bar turns opaque and square",
            draft.keep_style(),
            Edit::KeepStyle,
        ));
        self.search_target("dock-advanced", section(items))
    }
}
