//! Native inspector with direct completed controls and local form validation.
use super::desktop::{ERROR, WARN, banner, conflict_banner, kept};
use super::system_appearance::fill;
use super::*;
use crate::desktop::DesktopStore;
use crate::desktop::panels::Setting;
use crate::desktop::windows::{Behavior, Gap, TileBehavior, WindowsDraft, is_transition};
use crate::settings::windows::Message as Edit;

fn edit(message: Edit) -> Message {
    Message::Windows(message)
}

/// The largest gap the sliders offer; larger stored values are shown as
/// they are and kept until edited.
const GAP_RANGE: u32 = 48;

/// Preview or fixture mode, when the store is not the live desktop.
pub(super) fn store_pill<'a>(store: Option<&DesktopStore>, accent: Color) -> Element<'a, Message> {
    match store {
        Some(store) if !store.writable() => pill("Preview · Read only", WARN),
        Some(store) => match store.fixture_root() {
            Some(root) => pill(
                format!(
                    "Fixture · {}",
                    root.file_name()
                        .map(|name| name.to_string_lossy().into_owned())
                        .unwrap_or_default()
                ),
                accent,
            ),
            None => text::body("").into(),
        },
        None => text::body("").into(),
    }
}

/// The recovery banner shared by the desktop subpages.
pub(super) fn recovery_banner<'a>(
    recovery: Option<&Result<crate::desktop::store::Journal, String>>,
    writable: bool,
    recover: Message,
    keep: Message,
) -> Option<Element<'a, Message>> {
    match recovery? {
        Ok(_) => Some(banner(
            "An earlier change was interrupted.",
            WARN,
            vec![
                button::text("Keep current")
                    .on_press_maybe(writable.then_some(keep))
                    .into(),
                button::standard("Restore previous")
                    .on_press_maybe(writable.then_some(recover))
                    .into(),
            ],
        )),
        Err(error) => Some(banner(
            error.clone(),
            ERROR,
            vec![
                button::text("Keep current")
                    .on_press_maybe(writable.then_some(keep))
                    .into(),
            ],
        )),
    }
}

/// A row: title and a short consequence on the left, the control right.
fn row_of<'a>(
    title: &'static str,
    note: Option<&'static str>,
    control: Element<'a, Message>,
) -> Element<'a, Message> {
    let mut left = vec![label(title, 13., FG)];
    if let Some(note) = note {
        left.push(label(note, 11., MUTED));
    }
    split(column::with_children(left).spacing(2).into(), control)
}

fn flag_row<'a>(
    draft: &WindowsDraft,
    behavior: Behavior,
    title: &'static str,
    note: Option<&'static str>,
) -> Element<'a, Message> {
    let control = match draft.flag(behavior) {
        Setting::Known(on) => toggler(on)
            .on_toggle(move |on| edit(Edit::Flag(behavior, on)))
            .into(),
        Setting::Unsupported(raw) => kept(&raw),
    };
    row_of(title, note, control)
}

impl Settings {
    fn windows_accent(&self) -> Color {
        self.settings_accent()
    }

    pub(super) fn windows_page(&self) -> Element<'_, Message> {
        let accent = self.windows_accent();
        let editor = &self.windows;
        let mut items: Vec<Element<'_, Message>> = Vec::new();
        if let Some(error) = editor.unavailable() {
            items.push(split(
                heading("Windows", "How windows open, line up and take focus."),
                store_pill(self.windows.store(), accent),
            ));
            items.push(banner(
                format!("Window settings are unavailable. {error}"),
                ERROR,
                Vec::new(),
            ));
            return column::with_children(items).spacing(14).into();
        }
        let Some(draft) = editor.draft() else {
            items.push(split(
                heading("Windows", "How windows open, line up and take focus."),
                store_pill(self.windows.store(), accent),
            ));
            items.push(label("Reading window settings…", 12., MUTED));
            return column::with_children(items).spacing(14).into();
        };
        items.push(split(
            heading("Windows", "How windows open, line up and take focus."),
            store_pill(self.windows.store(), accent),
        ));
        if let Some(banner) = recovery_banner(
            editor.recovery(),
            editor.writable(),
            edit(Edit::Recover),
            edit(Edit::KeepRecovery),
        ) {
            items.push(banner);
        }
        let conflicts = draft.conflicts();
        if !conflicts.is_empty() {
            items.push(conflict_banner(
                crate::settings::system_appearance::describe(&conflicts),
                edit(Edit::UseCurrent),
                edit(Edit::KeepMine),
            ));
        }
        let blocked = editor.blocked();
        if let Some(transition) = blocked.iter().find(|b| is_transition(b)) {
            let first = match editor.first_step() {
                Some(Behavior::Autotile) => "Apply tiling first",
                _ => "Apply scope first",
            };
            items.push(banner(
                transition.reason.clone(),
                WARN,
                vec![
                    button::standard(first)
                        .on_press_maybe(editor.can_apply_first().then_some(edit(Edit::ApplyFirst)))
                        .into(),
                ],
            ));
        }
        let other: Vec<_> = blocked
            .iter()
            .filter(|b| !is_transition(b))
            .map(|b| b.key.clone())
            .collect();
        if !other.is_empty() {
            items.push(banner(
                format!(
                    "Can’t apply: this version doesn’t recognize the current {}. It is kept as it is; discard these edits to continue.",
                    crate::settings::system_appearance::describe(&other)
                ),
                ERROR,
                vec![button::text("Discard").on_press(edit(Edit::Discard)).into()],
            ));
        }
        items.push(self.windows_header(draft));
        // Tiling and snapping are both about where windows go: one card.
        let mut arranging = self.windows_tiling(draft);
        arranging.extend(self.windows_snap(draft));
        items.push(group(Some("Arranging windows"), arranging));
        // Gaps only space out tiled windows, so they wait until tiling is on.
        if draft.flag(Behavior::Autotile).known() != Some(&false) {
            items.push(self.windows_gaps(draft));
        }
        items.push(windows_controls(draft));
        // Corners, outline color and shadows come from the appearance draft.
        if let Some(look) = self.system_appearance.draft() {
            items.push(self.disclosure(
                "appearance-window-details",
                "Corners, outline color and shadows",
                column::with_children(vec![
                    self.appearance_window(look),
                    self.search_target("appearance-effects", self.appearance_effects(look)),
                ])
                .spacing(16)
                .into(),
            ));
        }
        items.push(self.windows_focus(draft));
        column::with_children(items).spacing(16).into()
    }

    /// A small picture of tiling, gaps and the active outline; it follows
    /// the draft as you change it.
    fn windows_header(&self, draft: &WindowsDraft) -> Element<'_, Message> {
        let accent = self.windows_accent();
        let variant = self.windows.variant;
        let tiled = draft.flag(Behavior::Autotile).known() == Some(&true);
        let (outer, inner) = draft.gaps(variant).known().copied().unwrap_or((8, 8));
        let hint = draft.flag(Behavior::ActiveHint).known() != Some(&false);
        let outline = draft.outline_width(variant).known().copied().unwrap_or(1);
        let height = (self.window.height * 0.26).clamp(120., 190.);
        let buttons = (
            draft.flag(Behavior::ShowMinimize).known() != Some(&false),
            draft.flag(Behavior::ShowMaximize).known() != Some(&false),
        );
        container(self.preview_stage_padded(
            tiling_schematic(tiled, outer, inner, hint, outline, accent, height, buttons),
            0,
        ))
        .id(cosmic::widget::Id::new("desktop-scene"))
        .width(Length::Fill)
        .into()
    }

    fn windows_tiling(&self, draft: &WindowsDraft) -> Vec<Element<'_, Message>> {
        let accent = self.windows_accent();
        let mut items = Vec::new();
        let tiled = draft.flag(Behavior::Autotile).known() == Some(&true);
        items.push(match draft.flag(Behavior::Autotile) {
            Setting::Known(on) => row_of(
                "Tile windows automatically",
                Some("Arrange windows side by side for you"),
                toggler(on).on_toggle(|on| edit(Edit::Autotile(on))).into(),
            ),
            Setting::Unsupported(raw) => row_of("Tile windows automatically", None, kept(&raw)),
        });
        if tiled || !matches!(draft.scope(), Setting::Known(TileBehavior::Global)) {
            let scope = draft.scope();
            let mut choice = vec![segmented(
                [
                    ("All", TileBehavior::Global),
                    ("Each on its own", TileBehavior::PerWorkspace),
                ]
                .into_iter()
                .map(|(title, value)| {
                    (
                        title.to_string(),
                        scope.known() == Some(&value),
                        Some(edit(Edit::Scope(value))),
                    )
                })
                .collect(),
                accent,
            )];
            if let Setting::Unsupported(raw) = &scope {
                choice.push(kept(raw));
            }
            items.push(group_row(
                "Workspaces",
                match scope {
                    Setting::Known(TileBehavior::PerWorkspace) => {
                        "Each workspace remembers its own choice"
                    }
                    _ => "Every workspace tiles the same way",
                },
                row::with_children(choice)
                    .spacing(8)
                    .align_y(Alignment::Center)
                    .into(),
            ));
        }
        items
    }

    fn windows_gaps(&self, draft: &WindowsDraft) -> Element<'_, Message> {
        let editor = &self.windows;
        let variant = editor.variant;
        let mut items = Vec::new();
        match (draft.gaps(variant), draft.base_gaps(variant)) {
            (Setting::Known((outer, inner)), Setting::Known(_)) => {
                for (gap, title, value) in [
                    (Gap::Outer, "Screen edges", outer),
                    (Gap::Inner, "Between windows", inner),
                ] {
                    let shown = format!("{value} px");
                    items.push(
                        column::with_children(vec![
                            split(label(title, 13., FG), label(shown, 12., MUTED)),
                            slider(0..=GAP_RANGE, value.min(GAP_RANGE), move |value| {
                                edit(Edit::Gap(gap, value))
                            })
                            .step(1u32)
                            .on_release(edit(Edit::Apply))
                            .width(Length::Fill)
                            .apply(|control| completed_slider(control.into(), edit(Edit::Apply))),
                        ])
                        .spacing(7)
                        .into(),
                    );
                }
            }
            (_, Setting::Unsupported(raw)) | (Setting::Unsupported(raw), _) => {
                items.push(row_of("Gaps", None, kept(&raw)));
            }
        }
        group(Some("Gaps"), items)
    }

    fn windows_focus(&self, draft: &WindowsDraft) -> Element<'_, Message> {
        let editor = &self.windows;
        let mut items = Vec::new();
        items.push(flag_row(
            draft,
            Behavior::FocusFollowsCursor,
            "Select a window by pointing at it",
            Some("No click needed"),
        ));
        if draft.flag(Behavior::FocusFollowsCursor).known() == Some(&true) {
            let text = editor.delay_text().map(str::to_owned).unwrap_or_else(|| {
                match draft.focus_delay() {
                    Setting::Known(ms) => ms.to_string(),
                    Setting::Unsupported(_) => String::new(),
                }
            });
            let control: Element<'_, Message> = match draft.focus_delay() {
                Setting::Unsupported(raw) if editor.delay_text().is_none() => kept(&raw),
                _ => row::with_children(vec![
                    container(
                        text_input("250", text)
                            .on_input(|text| edit(Edit::DelayInput(text)))
                            .on_submit(|_| edit(Edit::SubmitNumber(true)))
                            .width(Length::Fill),
                    )
                    .width(84)
                    .into(),
                    label("ms", 12., MUTED),
                ])
                .spacing(8)
                .align_y(Alignment::Center)
                .into(),
            };
            items.push(row_of("Delay", None, control));
            if editor.delay_text().is_some() {
                items.push(label(
                    format!(
                        "Use a whole number from 0 to {} ms.",
                        crate::desktop::windows::MAX_FOCUS_DELAY
                    ),
                    11.,
                    ERROR,
                ));
            }
        }
        items.push(flag_row(
            draft,
            Behavior::CursorFollowsFocus,
            "Move the pointer to the window you switch to",
            Some("Handy when switching with the keyboard"),
        ));
        group(Some("Choosing a window"), items)
    }

    /// Dragging windows: whether they snap to edges, and how close.
    fn windows_snap(&self, draft: &WindowsDraft) -> Vec<Element<'_, Message>> {
        let editor = &self.windows;
        let mut items = Vec::new();
        let snap = draft.edge_snap();
        items.push(match &snap {
            Setting::Known(px) => row_of(
                "Snap to screen edges",
                Some("Dragged windows stick to the display edges"),
                toggler(*px > 0)
                    .on_toggle(|on| edit(Edit::EdgeSnap(on)))
                    .into(),
            ),
            Setting::Unsupported(raw) => row_of("Snap to screen edges", None, kept(raw)),
        });
        if let Setting::Known(px) = snap
            && (px > 0 || editor.snap_text().is_some())
        {
            let text = editor
                .snap_text()
                .map(str::to_owned)
                .unwrap_or_else(|| px.to_string());
            items.push(row_of(
                "Snap distance",
                Some("How close a window gets before it snaps"),
                row::with_children(vec![
                    container(
                        text_input("10", text)
                            .on_input(|text| edit(Edit::SnapInput(text)))
                            .on_submit(|_| edit(Edit::SubmitNumber(false)))
                            .width(Length::Fill),
                    )
                    .width(84)
                    .into(),
                    label("px", 12., MUTED),
                ])
                .spacing(8)
                .align_y(Alignment::Center)
                .into(),
            ));
            if editor.snap_text().is_some() {
                items.push(label(
                    format!(
                        "Use a whole number from 1 to {} px.",
                        crate::desktop::windows::MAX_EDGE_SNAP
                    ),
                    11.,
                    ERROR,
                ));
            }
        }
        items
    }
}

fn windows_controls<'a>(draft: &WindowsDraft) -> Element<'a, Message> {
    group(
        Some("Title bar and outline"),
        vec![
            flag_row(
                draft,
                Behavior::ActiveHint,
                "Outline the active window",
                Some("A thin line in your accent color"),
            ),
            flag_row(draft, Behavior::ShowMinimize, "Minimize button", None),
            flag_row(draft, Behavior::ShowMaximize, "Maximize button", None),
        ],
    )
}

/// A screen with windows: tiled side by side within the gaps, or two
/// floating windows. Gaps are drawn at half scale and capped, so large
/// values stay legible; the focused window wears the outline when shown.
#[allow(clippy::too_many_arguments)]
fn tiling_schematic<'a>(
    tiled: bool,
    outer: u32,
    inner: u32,
    hint: bool,
    outline: u32,
    accent: Color,
    height: f32,
    buttons: (bool, bool),
) -> Element<'a, Message> {
    let scale = |px: u32| (px as f32 * 0.5).clamp(0., 12.);
    let window = move |active: bool| -> Element<'a, Message> {
        let border = if active && hint {
            (accent, (outline as f32).clamp(1., 3.))
        } else {
            (Color { a: 0.25, ..FG }, 1.)
        };
        // Title bar buttons appear and disappear with their switches.
        let dot = |shown: bool| -> Element<'a, Message> {
            if shown {
                fill(
                    cosmic::iced::widget::space().width(7).height(7).into(),
                    Color { a: 0.45, ..FG },
                    Color::TRANSPARENT,
                    0.,
                    4.,
                )
                .into()
            } else {
                cosmic::iced::widget::space().width(0).into()
            }
        };
        let title = row::with_children(vec![
            fill(
                cosmic::iced::widget::space().width(Length::Fill).height(5).into(),
                Color { a: 0.18, ..FG },
                Color::TRANSPARENT,
                0.,
                2.,
            )
            .width(Length::FillPortion(2))
            .into(),
            cosmic::iced::widget::space().width(Length::FillPortion(3)).into(),
            dot(buttons.0),
            dot(buttons.1),
            dot(true),
        ])
        .spacing(5)
        .align_y(Alignment::Center);
        fill(
            column::with_children(vec![title.into()]).padding([5, 7]).into(),
            Color { a: 0.92, ..SURFACE },
            border.0,
            border.1,
            6.,
        )
        .width(Length::Fill)
        .height(Length::Fill)
        .into()
    };
    // Windows sit straight on the same desktop backdrop as every other
    // preview, filling the width like a real screen.
    cosmic::widget::responsive(move |size| {
        let width = size.width;
        let content: Element<'a, Message> = if tiled {
            let gap = scale(inner);
            container(
                row::with_children(vec![
                    container(window(true))
                        .width(Length::FillPortion(3))
                        .height(Length::Fill)
                        .into(),
                    column::with_children(vec![window(false), window(false)])
                        .spacing(gap)
                        .width(Length::FillPortion(2))
                        .height(Length::Fill)
                        .into(),
                ])
                .spacing(gap)
                .height(Length::Fill),
            )
            .padding(scale(outer).max(2.))
            .into()
        } else {
            cosmic::iced::widget::stack(vec![
                container(window(false))
                    .padding(cosmic::iced::Padding {
                        top: height * 0.10,
                        left: width * 0.16,
                        right: width * 0.42,
                        bottom: height * 0.30,
                    })
                    .into(),
                container(window(true))
                    .padding(cosmic::iced::Padding {
                        top: height * 0.30,
                        left: width * 0.40,
                        right: width * 0.14,
                        bottom: height * 0.08,
                    })
                    .into(),
            ])
            .width(Length::Fill)
            .height(Length::Fill)
            .into()
        };
        container(content)
            .width(Length::Fill)
            .height(height)
            .into()
    })
    .height(height)
    .into()
}
