//! Workspaces subpage: a small diagram of the orientation and display mode,
//! then Navigation, Multiple displays and Overview.
use super::desktop::{ERROR, WARN, banner, conflict_banner, kept};
use super::system_appearance::fill;
use super::windows::{recovery_banner, store_pill};
use super::*;
use crate::desktop::panels::Setting;
use crate::desktop::workspaces::{
    TypingAction, WorkspaceConfig, WorkspaceLayout, WorkspaceMode,
};
use crate::settings::workspaces::Message as Edit;

fn edit(message: Edit) -> Message {
    Message::Workspaces(message)
}

fn choice<'a, T: PartialEq + Copy>(
    current: Option<T>,
    options: &[(&'static str, T)],
    message: impl Fn(T) -> Edit,
    accent: Color,
) -> Element<'a, Message> {
    segmented(
        options
            .iter()
            .map(|(title, value)| {
                (
                    title.to_string(),
                    current == Some(*value),
                    current.is_some().then(|| edit(message(*value))),
                )
            })
            .collect(),
        accent,
    )
}

impl Settings {
    pub(super) fn workspaces_page(&self) -> Element<'_, Message> {
        self.workspaces_section(false)
    }

    /// The Workspaces editor; embedded in Windows it starts with a section
    /// title instead of its own diagram.
    pub(super) fn workspaces_section(&self, embedded: bool) -> Element<'_, Message> {
        let accent = self.settings_accent();
        let editor = &self.workspaces;
        let mut items: Vec<Element<'_, Message>> = Vec::new();
        if let Some(error) = editor.unavailable() {
            items.push(split(
                heading("Workspaces", ""),
                store_pill(editor.store(), accent),
            ));
            items.push(banner(
                format!("Workspace settings are unavailable. {error}"),
                ERROR,
                Vec::new(),
            ));
            return column::with_children(items).spacing(14).into();
        }
        let Some(draft) = editor.draft() else {
            items.push(split(
                heading("Workspaces", ""),
                store_pill(editor.store(), accent),
            ));
            items.push(label("Reading workspace settings…", 12., MUTED));
            return column::with_children(items).spacing(14).into();
        };
        let config = draft.config();
        let known = config.known().copied();
        if !embedded {
            items.push(split(
                heading("Workspaces", ""),
                store_pill(editor.store(), accent),
            ));
            let height = (self.window.height * 0.15).clamp(76., 104.);
            items.push(
                container(workspace_diagram(known.unwrap_or_default(), accent, height))
                    .max_width(520)
                    .width(Length::Fill)
                    .into(),
            );
        }
        // Embedded under Windows, which already shows the same journal.
        let shown_above = embedded && self.windows.recovery().is_some();
        if !shown_above
            && let Some(banner) = recovery_banner(
                editor.recovery(),
                editor.writable(),
                edit(Edit::Recover),
                edit(Edit::KeepRecovery),
            )
        {
            items.push(banner);
        }
        if !draft.conflicts().is_empty() {
            items.push(conflict_banner(
                crate::settings::system_appearance::describe(draft.conflicts()),
                edit(Edit::UseCurrent),
                edit(Edit::KeepMine),
            ));
        }
        if let Setting::Unsupported(raw) = &config {
            items.push(banner(
                "These workspace settings include values this version doesn’t recognize. They are kept as they are and can’t be changed here.",
                WARN,
                vec![kept(raw)],
            ));
        }
        let wrap: Element<'_, Message> = match draft.config() {
            Setting::Known(config) => toggler(config.workspace_wraparound)
                .on_toggle(|on| edit(Edit::Wraparound(on)))
                .into(),
            Setting::Unsupported(_) => text::body("").into(),
        };
        let mode = known.map(|config| config.workspace_mode);
        items.push(group(
            Some("Workspaces"),
            vec![
                group_row(
                    "Direction",
                    "How workspaces are lined up",
                    choice(
                        known.map(|config| config.workspace_layout),
                        &[
                            ("Vertical", WorkspaceLayout::Vertical),
                            ("Horizontal", WorkspaceLayout::Horizontal),
                        ],
                        Edit::Layout,
                        accent,
                    ),
                ),
                group_row(
                    "Wrap around",
                    "Going past the last workspace returns to the first",
                    wrap,
                ),
                group_row(
                    "Switch workspaces",
                    match mode {
                        Some(WorkspaceMode::Global) => "All displays switch together",
                        _ => "Each display switches on its own",
                    },
                    choice(
                        mode,
                        &[
                            ("Per display", WorkspaceMode::OutputBound),
                            ("All together", WorkspaceMode::Global),
                        ],
                        Edit::Mode,
                        accent,
                    ),
                ),
            ],
        ));
        // The overview is its own thing, not a workspace setting.
        items.push(group(
            Some("Overview"),
            vec![group_row(
                "When you start typing",
                "In the workspace overview",
                choice(
                    known.map(|config| config.action_on_typing),
                    &[
                        ("Nothing", TypingAction::None),
                        ("Search apps", TypingAction::OpenApplications),
                        ("Run a command", TypingAction::OpenLauncher),
                    ],
                    Edit::Typing,
                    accent,
                ),
            )],
        ));
        column::with_children(items).spacing(16).into()
    }
}

/// Two displays and their workspaces: a strip beside (vertical) or below
/// (horizontal) each display, or one strip shared by both. No titles or
/// window contents; the number of workspaces is only illustrative.
fn workspace_diagram<'a>(
    config: WorkspaceConfig,
    accent: Color,
    height: f32,
) -> Element<'a, Message> {
    let vertical = config.workspace_layout == WorkspaceLayout::Vertical;
    let thumb = |current: bool| -> Element<'a, Message> {
        let (w, h) = if vertical { (16., 10.) } else { (14., 9.) };
        fill(
            cosmic::iced::widget::space().width(w).height(h).into(),
            if current {
                Color { a: 0.35, ..accent }
            } else {
                SURFACE
            },
            if current { accent } else { LINE },
            1.,
            2.,
        )
        .into()
    };
    let strip = || -> Element<'a, Message> {
        let mut parts = vec![thumb(true), thumb(false), thumb(false)];
        if config.workspace_wraparound {
            parts.push(label("↻", 10., MUTED));
        }
        if vertical {
            column::with_children(parts)
                .spacing(3)
                .align_x(Alignment::Center)
                .into()
        } else {
            row::with_children(parts)
                .spacing(3)
                .align_y(Alignment::Center)
                .into()
        }
    };
    let display = |name: &'static str| -> Element<'a, Message> {
        fill(
            container(label(name, 10., MUTED))
                .center(Length::Fill)
                .into(),
            BASE,
            LINE,
            1.,
            4.,
        )
        .width(Length::Fill)
        .height(Length::Fill)
        .into()
    };
    let with_strip = |screen: Element<'a, Message>| -> Element<'a, Message> {
        if vertical {
            row::with_children(vec![strip(), screen])
                .spacing(5)
                .align_y(Alignment::Center)
                .width(Length::Fill)
                .height(Length::Fill)
                .into()
        } else {
            column::with_children(vec![screen, strip()])
                .spacing(5)
                .align_x(Alignment::Center)
                .width(Length::Fill)
                .height(Length::Fill)
                .into()
        }
    };
    let content: Element<'a, Message> = match config.workspace_mode {
        WorkspaceMode::OutputBound => {
            row::with_children(vec![with_strip(display("1")), with_strip(display("2"))])
                .spacing(10)
                .into()
        }
        WorkspaceMode::Global => with_strip(
            row::with_children(vec![display("1"), display("2")])
                .spacing(6)
                .width(Length::Fill)
                .height(Length::Fill)
                .into(),
        ),
    };
    fill(content, Color { a: 0.4, ..BASE }, LINE, 1., 8.)
        .padding(8)
        .width(Length::Fill)
        .height(height)
        .into()
}
