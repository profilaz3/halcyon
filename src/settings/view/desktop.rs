//! Dock and panel share an object selector, a collapsible desktop scene and
//! focused behavior, style and contents groups. Commands stay in DesktopEditor.
use super::pages::section;
use super::*;
use crate::desktop::applets::AppletCatalog;
use crate::desktop::panels::{
    Anchor, AutoHide, Background as PanelBackground, MAX_MARGIN, MAX_PADDING, MAX_RADIUS,
    MAX_SPACING, Output, PanelDraft, PanelKind, PanelSize, Region, Setting, Slot, Timing,
};
use crate::settings::desktop::Message as Edit;
use cosmic::iced::advanced::text as advanced_text;

mod applets;
pub(super) mod inspector;
mod islands;
pub(super) use inspector::paired;
mod panel;
pub(super) mod preview;

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

/// A labelled control with room for native choices at compact widths.
fn field<'a>(title: &'static str, control: Element<'a, Message>) -> Element<'a, Message> {
    column::with_children(vec![label(title, 12., MUTED), control])
        .spacing(6)
        .width(Length::Fill)
        .into()
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
/// value the draft also edits. One action adopts the current value and keeps
/// every other edit; Apply would otherwise report the conflict.
pub(super) fn conflict_banner<'a>(names: String, use_current: Message) -> Element<'a, Message> {
    banner(
        format!("Changed outside this editor: {names}."),
        WARN,
        vec![
            button::standard("Use current value")
                .on_press(use_current)
                .into(),
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
        split(label(title, 12., FG), label(value_label, 11., accent)),
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
    /// The scene and keyboard object selector use the same selected bar.
    /// This header scrolls with the inspector at compact window sizes.
    pub(super) fn desktop_dock_header(&self) -> Element<'_, Message> {
        let accent = self.settings_accent();
        let editor = &self.desktop;
        let title = split(
            heading("Dock & panel", ""),
            super::windows::store_pill(editor.store(), accent),
        );
        if editor.store().is_none() {
            return title;
        }
        let selector = segmented(
            PanelKind::ALL
                .into_iter()
                .map(|kind| {
                    let dirty = editor.draft(kind).is_some_and(PanelDraft::dirty);
                    (
                        if dirty {
                            format!(
                                "{} •",
                                (if kind == PanelKind::Panel {
                                    "Panel"
                                } else {
                                    "Dock"
                                })
                            )
                        } else {
                            (if kind == PanelKind::Panel {
                                "Panel"
                            } else {
                                "Dock"
                            })
                            .to_string()
                        },
                        editor.selected == kind,
                        Some(edit(Edit::Select(kind))),
                    )
                })
                .collect(),
            accent,
        );
        let system = button::text("System settings ↗")
            .on_press(Message::OpenDesktop(editor.selected.system_page()));
        let mut parts = vec![title];
        parts.push(
            row::with_children(vec![label("Editing", 12., MUTED), selector, system.into()])
                .spacing(12)
                .align_y(Alignment::Center)
                .wrap()
                .into(),
        );
        column::with_children(parts)
            .spacing(12)
            .width(Length::Fill)
            .into()
    }

    pub(super) fn dock_workspace(&self) -> Element<'_, Message> {
        let editor = &self.desktop;
        let mut parts = Vec::new();
        if let Some(draft) = editor.current().filter(|draft| draft.installed()) {
            let scene = if self.scene_open() {
                self.desktop_preview(self.dock_preview_height())
            } else {
                cosmic::widget::space().height(0).into()
            };
            let caption = if draft.dirty() {
                "Current adjustment · schematic bar appearance"
            } else {
                "Current desktop · schematic bar appearance"
            };
            parts.push(self.scene_frame(scene, caption));
        }
        if !self.scene_open() {
            // Collapsing the preview gives its width back to the controls.
            parts.push(self.desktop_dock_page());
            return column::with_children(parts).spacing(12).into();
        }
        self.inspector_layout(
            column::with_children(parts).spacing(12).into(),
            self.desktop_dock_page(),
            "dock",
        )
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
        match editor.recovery() {
            Some(Ok(_)) => items.push(banner(
                "An earlier change was interrupted.",
                WARN,
                vec![
                    button::text("Keep current")
                        .on_press_maybe(editor.writable().then_some(edit(Edit::KeepRecovery)))
                        .into(),
                    button::standard("Restore previous")
                        .on_press_maybe(editor.writable().then_some(edit(Edit::Recover)))
                        .into(),
                ],
            )),
            Some(Err(error)) => items.push(banner(
                error.clone(),
                ERROR,
                vec![
                    button::text("Keep current")
                        .on_press_maybe(editor.writable().then_some(edit(Edit::KeepRecovery)))
                        .into(),
                ],
            )),
            None => {}
        }
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
            items.push(conflict_banner(names.join(", "), edit(Edit::UseCurrent)));
        }
        // Activation is separate from each bar's saved visual style.
        if !matches!(
            self.desktop.islands.installation,
            crate::desktop::islands::Installation::Pending
        ) {
            items.push(self.addons_section());
        }
        items.push(self.layout_section(draft));
        // A missing/unhealthy host must not hide the saved Islands intent or
        // its recovery controls. Availability is reported next to placement.
        if self.desktop.islands.installation.installed()
            || draft.islands().known().is_some_and(|s| s.enabled)
        {
            if draft.kind() == PanelKind::Dock || draft.islands().known().is_some_and(|s| s.enabled)
            {
                items.push(self.islands_section(draft));
            }
        }
        if draft.kind() == PanelKind::Panel {
            items.push(self.panel_contents(draft));
        } else {
            items.push(self.applets_section(draft));
            items.push(self.installed_applets(draft));
        }
        items.push(self.advanced_section(draft));
        column::with_children(items).spacing(16).into()
    }

    /// Full-width scene, with controls still near at compact window sizes.
    fn dock_preview_height(&self) -> f32 {
        self.stage_height()
    }

    fn layout_section(&self, draft: &PanelDraft) -> Element<'_, Message> {
        let accent = self.settings_accent();
        let horizontal = draft
            .anchor()
            .known()
            .is_none_or(|anchor| anchor.horizontal());
        let anchors = Anchor::ALL
            .into_iter()
            .map(|anchor| (anchor.title().to_string(), anchor))
            .collect();
        let mut sizes: Vec<(String, PanelSize)> = PanelSize::CHOICES
            .into_iter()
            .map(|size| (size.title(), size))
            .collect();
        if let Setting::Known(custom @ PanelSize::Custom(_)) = draft.size() {
            sizes.push((custom.title(), custom));
        }
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
        let opacity = draft.opacity();
        let opacity_percent = opacity
            .known()
            .map(|value| (value * 100.).round().clamp(0., 100.) as u32);
        let radius = draft.radius().known().copied();
        let mut toggle_items: Vec<Element<'_, Message>> = vec![
            container(self.desktop_switch(
                "Floating",
                "Gap from the screen edge",
                draft.floating(),
                Edit::Floating,
            ))
            .width(Length::FillPortion(1))
            .into(),
        ];
        if draft.kind() == PanelKind::Dock && !draft.islands().known().is_some_and(|s| s.enabled) {
            toggle_items.push(
                container(self.desktop_switch(
                    if horizontal {
                        "Full width"
                    } else {
                        "Full height"
                    },
                    "",
                    draft.extend(),
                    Edit::Extend,
                ))
                .width(Length::FillPortion(1))
                .into(),
            );
        }
        let toggles = column::with_children(toggle_items).spacing(12);
        let mut style: Vec<Element<'_, Message>> = vec![label("Style", 16., FG)];
        if draft.kind() == PanelKind::Panel {
            style.push(self.panel_style(draft));
        }
        style.push(field(
            "Size",
            choice(&draft.size(), sizes, Edit::Size, accent),
        ));
        style.push(match opacity_percent {
            Some(percent) => container(value_slider(
                "Background opacity",
                format!("{percent}%"),
                0..=100,
                percent,
                1,
                accent,
                |value| Edit::Opacity(value as u8),
            ))
            .id("native-bar-opacity")
            .into(),
            None => field(
                "Background opacity",
                kept(match &opacity {
                    Setting::Unsupported(raw) => raw,
                    Setting::Known(_) => "",
                }),
            ),
        });
        style.push(toggles.into());
        style.push(field(
            "Corners",
            choice(
                &draft.radius(),
                vec![
                    ("Square".into(), 0),
                    ("Rounded".into(), 12),
                    ("Pill".into(), MAX_RADIUS),
                ],
                Edit::RadiusChoice,
                accent,
            ),
        ));
        if let Some(radius) = radius.filter(|radius| ![0, 12, MAX_RADIUS].contains(radius)) {
            style.push(label(
                format!("Custom radius {radius} px · adjust it in Advanced"),
                11.,
                MUTED,
            ));
        }
        style.push(field(
            "Background",
            choice(
                &draft.background(),
                {
                    let mut options = vec![
                        ("Theme".into(), PanelBackground::ThemeDefault),
                        ("Dark".into(), PanelBackground::Dark),
                        ("Light".into(), PanelBackground::Light),
                    ];
                    if let Setting::Known(color @ PanelBackground::Color(_)) = draft.background() {
                        options.push(("Custom color".into(), color));
                    }
                    options
                },
                Edit::Background,
                accent,
            ),
        ));
        paired(
            self.window.width >= 1120.,
            section(vec![
                label("Position & behavior", 16., FG),
                field(
                    "Show on display",
                    choice(&draft.output(), outputs, Edit::Output, accent),
                ),
                field(
                    "Screen edge",
                    choice(&draft.anchor(), anchors, Edit::Anchor, accent),
                ),
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
            ]),
            section(style),
        )
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
                label("Advanced", 16., FG),
                container(text::body("")).width(Length::Fill).into(),
                label("Timing, spacing and corners", 11., MUTED),
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
        .padding([4, 4])
        .width(Length::Fill)
        .on_press(edit(Edit::Advanced(!open)));
        if !open {
            return self.search_target("dock-advanced", section(vec![toggle.into()]));
        }
        let mut items: Vec<Element<'_, Message>> = vec![toggle.into()];
        let hides = draft.autohide().known().copied();
        match draft.behavior() {
            Setting::Known(behavior) => {
                items.push(label(
                    if hides == Some(AutoHide::Never) {
                        "HIDE TIMING · used when hiding is on"
                    } else {
                        "HIDE TIMING"
                    },
                    10.,
                    MUTED,
                ));
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
        items.push(label("SPACING", 10., MUTED));
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
                    "Between applets",
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
                number(draft.radius(), "Corner radius", MAX_RADIUS, Edit::Radius),
                if floating {
                    number(margin, "Edge gap", u32::from(MAX_MARGIN), |value| {
                        Edit::Margin(value as u16)
                    })
                } else {
                    container(
                        column::with_children(vec![
                            label("Edge gap", 12., FG),
                            label("Turn on Floating to set a gap.", 11., MUTED),
                        ])
                        .spacing(7),
                    )
                    .width(Length::FillPortion(1))
                    .into()
                },
            ])
            .spacing(20)
            .into(),
        );
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
