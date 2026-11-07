use super::*;

impl Settings {
    pub(super) fn search_target<'a>(
        &'a self,
        id: &str,
        content: Element<'a, Message>,
    ) -> Element<'a, Message> {
        let selected = self.search_selected.is_some_and(|hit| hit.anchor() == id);
        let accent = self.settings_accent();
        container(content)
            .id(cosmic::widget::Id::new(id.to_owned()))
            .width(Length::Fill)
            .class(cosmic::theme::Container::custom(move |_| {
                cosmic::iced::widget::container::Style {
                    border: Border {
                        color: if selected { accent } else { Color::TRANSPARENT },
                        width: if selected { 1. } else { 0. },
                        radius: 12.into(),
                    },
                    ..Default::default()
                }
            }))
            .into()
    }
    pub(super) fn page_settings_advanced<'a>(
        &'a self,
        page: Page,
        children: Vec<Element<'a, Message>>,
    ) -> Element<'a, Message> {
        let open = self.advanced_open(page);
        // Say what is inside, so nobody has to open it to find out.
        let title = match page {
            Page::Calendar => "Sync and sign-in setup",
            Page::Weather => "Dew point and how often it updates",
            Page::Clock | Page::Projects | Page::Devbox => "How often it updates",
            Page::Appearance => "Card opacity and outline",
            _ => "More options",
        };
        let mut rows = vec![
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
            .class(nav_style(false, self.settings_accent()))
            .width(Length::Fill)
            // Same inset as card rows, like every fold-open section.
            .padding([4, 10])
            .on_press(Message::ToggleAdvanced(page))
            .into(),
        ];
        let mut body = vec![separator()];
        body.extend(children);
        rows.push(super::reveal::reveal(
            container(column::with_children(body).spacing(10))
                .padding([10, 0, 0, 0])
                .into(),
            open,
            self.config.settings_animations && self.system_animations,
        ));
        self.search_target(
            "settings-advanced",
            pages::section(vec![column::with_children(rows).into()]),
        )
    }

    pub(super) fn navigation_header(&self) -> Element<'_, Message> {
        let mut items = Vec::new();
        let searching = !self.search_query.trim().is_empty() || self.search_open;
        let mut top: Vec<Element<'_, Message>> = Vec::new();
        // Sidebar pages have no back link. A subpage, or a view opened inside
        // a page, has one link to where it sits, so it is always predictable.
        if let Some((title, message)) = self.up_link().filter(|_| !searching) {
            top.push(
                button::text(title)
                    .id(cosmic::widget::Id::new("settings-back"))
                    .leading_icon(icon::from_name("go-previous-symbolic").size(16))
                    .tooltip("Back (Alt+←)")
                    .on_press(message)
                    .into(),
            );
        }
        if !top.is_empty() {
            items.push(
                row::with_children(top)
                    .spacing(6)
                    .align_y(Alignment::Center)
                    .into(),
            );
        }
        if !searching && !self.page.destinations().is_empty() {
            items.push(self.destination_switcher());
        }
        if let Some(problem) = &self.session_drafts.error {
            items.push(label(
                format!("Draft recovery: {problem}"),
                11.,
                Color::from_rgb8(245, 149, 138),
            ));
        }
        container(column::with_children(items).spacing(4))
            .padding([8, 20, 0, 20])
            .width(Length::Fill)
            .into()
    }

    /// Browse, Saved, Mix and Schedules: flat tabs of one root.
    fn setups_tabs(&self) -> Element<'_, Message> {
        use crate::settings::profiles::Message as Edit;
        let accent = self.settings_accent();
        let editor = &self.profiles;
        let on_list = self.page == Page::DesktopProfiles;
        let mix = on_list && editor.mix.is_some();
        let kept = self.kept_previews();
        let tab = |title: &'static str,
                   id: &'static str,
                   selected: bool,
                   waiting: bool,
                   message: Message|
         -> Element<'_, Message> {
            let mut parts = vec![label(title, 14., if selected { FG } else { MUTED })];
            if waiting {
                parts.push(dot(accent, 6.));
            }
            button::custom(
                row::with_children(parts)
                    .spacing(6)
                    .align_y(Alignment::Center),
            )
            .id(cosmic::widget::Id::new(id))
            .class(nav_style(selected, accent))
            .selected(selected)
            .padding([7, 10])
            .on_press(message)
            .into()
        };
        let waiting = |target: &dyn Fn(&crate::settings::navigation::KeptTarget) -> bool| {
            kept.iter().any(|item| target(&item.target))
        };
        row::with_children(vec![
            tab(
                "Browse",
                "setups-explore",
                on_list && !mix && !editor.saved_collection,
                waiting(&|t| matches!(t, crate::settings::navigation::KeptTarget::Setup(_))),
                Message::SetupsTab(false),
            ),
            tab(
                "Saved",
                "setups-saved",
                on_list && !mix && editor.saved_collection,
                false,
                Message::SetupsTab(true),
            ),
            tab(
                "Mix",
                "mix-start",
                mix,
                false,
                Message::Profiles(Edit::MixStart),
            ),
            tab(
                "Schedules",
                "destination-desktop-automation",
                !on_list,
                waiting(&|t| {
                    matches!(
                        t,
                        crate::settings::navigation::KeptTarget::Adaptive
                            | crate::settings::navigation::KeptTarget::Page(
                                Page::DesktopAutomation
                            )
                    )
                }),
                Message::FlatPage(Page::DesktopAutomation),
            ),
        ])
        .spacing(4)
        .wrap()
        .into()
    }

    fn destination_switcher(&self) -> Element<'_, Message> {
        if self.page.sidebar() == Page::DesktopProfiles {
            return self.setups_tabs();
        }
        let current = if self.page.destinations().contains(&self.page) {
            self.page
        } else {
            self.page.nav()
        };
        row::with_children(
            self.page
                .destinations()
                .iter()
                .map(|&page| {
                    let selected = current == page;
                    let mut parts = vec![label(
                        page.nav_title(),
                        14.,
                        if selected { FG } else { MUTED },
                    )];
                    if self.kept_previews().iter().any(|item| {
                        matches!(
                            (&item.target, page),
                            (
                                crate::settings::navigation::KeptTarget::Colors,
                                Page::Themes
                            ) | (
                                crate::settings::navigation::KeptTarget::Setup(_),
                                Page::DesktopProfiles
                            ) | (
                                crate::settings::navigation::KeptTarget::Adaptive,
                                Page::DesktopAutomation
                            )
                        ) || item.target == crate::settings::navigation::KeptTarget::Page(page)
                    }) {
                        parts.push(dot(self.settings_accent(), 6.));
                    }
                    button::custom(
                        row::with_children(parts)
                            .spacing(6)
                            .align_y(Alignment::Center),
                    )
                    .id(cosmic::widget::Id::new(format!(
                        "destination-{}",
                        page.key()
                    )))
                    .class(nav_style(selected, self.settings_accent()))
                    .selected(selected)
                    .padding([7, 10])
                    .on_press(Message::FlatPage(page))
                    .into()
                })
                .collect::<Vec<_>>(),
        )
        .spacing(4)
        .wrap()
        .into()
    }

    pub(super) fn search_results_page(&self) -> Element<'_, Message> {
        let matches = super::super::navigation::search(&self.search_query);
        let mut rows = vec![heading("Find a setting", "")];
        if self.rail() {
            let mut search =
                cosmic::widget::text_input::search_input("Search settings", &self.search_query)
                    .id(super::super::navigation::search_id())
                    .on_input(Message::SearchSettings)
                    .on_submit(|_| Message::SearchFirst)
                    .width(Length::Fill);
            if !self.search_query.is_empty() {
                search = search.on_clear(Message::SearchSettings(String::new()));
            }
            rows.push(search.into());
        }
        if matches.is_empty() && !self.search_query.trim().is_empty() {
            rows.push(label(
                "No settings found. Try city, GPU, font, tiling or startup.",
                14.,
                MUTED,
            ));
        }
        for hit in matches {
            rows.push(
                button::custom(split(
                    row::with_children(vec![
                        dot(area_tint(hit.page.area(), self.settings_accent()), 8.),
                        detail_state(
                            hit.label,
                            format!(
                                "{} › {}{}",
                                hit.page.area().label(),
                                if hit.page == Page::General {
                                    hit.app_destination().tab_title()
                                } else {
                                    hit.page.nav_title()
                                },
                                if hit.advanced { " › Advanced" } else { "" }
                            ),
                        ),
                    ])
                    .spacing(12)
                    .align_y(Alignment::Center)
                    .into(),
                    icon::from_name("go-next-symbolic").size(16).icon().into(),
                ))
                .class(nav_style(false, self.settings_accent()))
                .padding(14)
                .width(Length::Fill)
                .on_press(Message::SearchResult(hit))
                .into(),
            );
        }
        column::with_children(rows).spacing(10).into()
    }

    pub(super) fn pending_changes_page(&self) -> Element<'_, Message> {
        let mut rows = vec![heading(
            "Unfinished changes",
            if self.recovery_sheet.rows.is_empty() {
                "Changes left half-done by an update show up here."
            } else {
                "From the previous version. Nothing changes until you choose to use one."
            },
        )];
        for (index, proposal) in self.recovery_sheet.rows.iter().enumerate() {
            let mut parts = vec![split(
                detail_state(proposal.kind.title(), &proposal.summary),
                toggler(proposal.selected)
                    .on_toggle(move |on| Message::RecoverySelect(index, on))
                    .into(),
            )];
            if let Some(problem) = &proposal.problem {
                parts.push(label(problem, 12., desktop::ERROR));
            }
            for (field, (name, value)) in proposal.fields().into_iter().enumerate() {
                parts.push(split(
                    label(name, 13., FG),
                    cosmic::widget::text_input("#RRGGBB", value)
                        .on_input(move |text| Message::RecoveryText(index, field, text))
                        .width(180)
                        .into(),
                ));
            }
            let mut actions = vec![
                button::text("Keep current")
                    .on_press(Message::RecoveryDiscard(index))
                    .into(),
                button::text("Use this choice")
                    .on_press(Message::RecoveryApplyOne(index))
                    .into(),
            ];
            if proposal.problem.is_some() {
                if proposal.fields().is_empty() {
                    actions.push(
                        button::text("Open form")
                            .on_press(Message::RecoveryOpen(index))
                            .into(),
                    );
                }
                actions.push(
                    button::text("Use recovered values")
                        .on_press(Message::RecoveryUseProposed(index))
                        .into(),
                );
            }
            parts.push(row::with_children(actions).spacing(8).wrap().into());
            rows.push(group(None, parts));
        }
        if self.recovery_sheet.rows.is_empty() {
            rows.push(group(
                None,
                vec![detail_state(
                    "Nothing to recover",
                    "Nothing was left half-done when Halcyon updated.",
                )],
            ));
        } else {
            rows.push(
                button::suggested("Use selected choices")
                    .on_press(Message::RecoveryApply)
                    .into(),
            );
        }
        column::with_children(rows).spacing(12).into()
    }

    pub(super) fn simple_status(&self) -> Element<'_, Message> {
        if let Some(review) = &self.undo_review {
            let mut lines = vec![label(
                &format!(
                    "Undo {} · some values changed elsewhere",
                    review.label.to_lowercase()
                ),
                13.,
                desktop::ERROR,
            )];
            lines.extend(
                review
                    .conflicts
                    .iter()
                    .take(3)
                    .map(|conflict| label(conflict, 11., Color::from_rgb8(170, 175, 184))),
            );
            lines.push(
                row::with_children(vec![
                    button::text("Keep all current values")
                        .id("undo-keep-current".into())
                        .on_press(Message::ResolveUndoShown(review.clone(), false))
                        .into(),
                    button::suggested("Undo other changes")
                        .id("undo-reviewed".into())
                        .on_press(Message::ResolveUndoShown(review.clone(), true))
                        .into(),
                ])
                .spacing(8)
                .into(),
            );
            return container(column::with_children(lines).spacing(6))
                .padding([8, 22])
                .width(Length::Fill)
                .into();
        }
        if self.native_busy {
            // The rendered target stays visible while a write finishes; it is
            // admitted in order and refuses if that write makes it stale.
            let mut items = vec![
                container(label(&self.status, 12., MUTED))
                    .id("settings-saving")
                    .width(Length::Fill)
                    .into(),
            ];
            items.extend(self.page_undo_button());
            return container(
                row::with_children(items)
                    .spacing(8)
                    .align_y(Alignment::Center),
            )
            .padding([8, 22])
            .width(Length::Fill)
            .into();
        }
        let error = self.page_error();
        let mut items: Vec<Element<'_, Message>> = Vec::new();
        let mut empty = true;
        if let Some(error) = error {
            empty = false;
            items.push(
                container(label(error, 12., desktop::ERROR))
                    .width(Length::Fill)
                    .into(),
            );
        } else if let Some((note, target)) = &self.kept_note {
            empty = false;
            items.push(
                container(label(note, 12., MUTED))
                    .width(Length::Fill)
                    .into(),
            );
            items.push(
                button::text("Resume")
                    .id(cosmic::widget::Id::new("settings-resume"))
                    .on_press(Message::ResumeKept(target.clone()))
                    .into(),
            );
        } else {
            let fresh = self
                .last_desktop_change
                .as_ref()
                .filter(|(_, _, id)| self.undo_baseline.as_ref() != Some(id))
                .map(|(_, title, _)| format!("Saved · {title}"));
            if let Some(status) = fresh {
                empty = false;
                // The latest save opens the full list of changes.
                items.push(
                    container(
                        // Reads as the quiet status line it replaces.
                        button::custom(label(status, 12., MUTED))
                            .id(cosmic::widget::Id::new("settings-saved-changes"))
                            .class(cosmic::theme::Button::Text)
                            .padding([2, 4])
                            .on_press(Message::OpenRoot(Page::Changes)),
                    )
                    .width(Length::Fill)
                    .into(),
                );
            } else {
                items.push(container(label("", 12., MUTED)).width(Length::Fill).into());
            }
        }
        if let Some(undo) = self.page_undo_button() {
            empty = false;
            items.push(undo);
        }
        // Nothing to say: no empty bar at the bottom of the page.
        if empty {
            return cosmic::iced::widget::space().height(0).into();
        }
        container(
            row::with_children(items)
                .spacing(8)
                .align_y(Alignment::Center),
        )
        .padding([8, 22])
        .width(Length::Fill)
        .into()
    }

    /// The same target Ctrl+Z uses; local scopes draw their own Undo.
    pub(in crate::settings) fn page_undo_button(&self) -> Option<Element<'_, Message>> {
        let crate::settings::navigation::UndoTarget::Desktop(id, title) = self.undo_target() else {
            return None;
        };
        let title = title
            .strip_prefix("Profile “")
            .and_then(|name| name.strip_suffix('”'))
            .map(|name| format!("look “{name}”"))
            .unwrap_or(title);
        Some(
            button::text(format!("Undo {title}"))
                .id(cosmic::widget::Id::new("settings-undo"))
                .leading_icon(icon::from_name("edit-undo-symbolic").size(14))
                .on_press(Message::UndoDesktopShown(Some(id)))
                .into(),
        )
    }

    /// The accent bar of a preview screen: one Undo, Discard and one named Apply.
    pub(in crate::settings) fn preview_bar<'a>(
        &'a self,
        status: Element<'a, Message>,
        can_undo: bool,
        extra: Vec<Element<'a, Message>>,
        discard: Message,
        discard_id: &'static str,
        apply: Element<'a, Message>,
    ) -> Element<'a, Message> {
        // libcosmic matches named siblings by name but keeps their old index, so a
        // conditional named button would shift its neighbours onto the wrong state.
        // Each sits in an unnamed slot, which is matched by position instead.
        let slot =
            |action: Element<'a, Message>| -> Element<'a, Message> { container(action).into() };
        let undo = slot(
            button::text("Undo")
                .id(cosmic::widget::Id::new("preview-undo"))
                .leading_icon(icon::from_name("edit-undo-symbolic").size(14))
                .on_press_maybe((can_undo && !self.native_busy).then_some(Message::UndoPreviewStep))
                .into(),
        );
        // Keep the optional action group in its own stable slot. Saving a
        // Mix removes its Save button; Discard and Apply must not inherit the
        // state of the old named neighbour when that happens.
        let extras = slot(row::with_children(extra).spacing(8).wrap().into());
        let discard = slot(
            // The builder is an edit mode, like arranging: it cancels.
            button::text(if discard_id == "recipe-discard" {
                "Cancel"
            } else {
                "Discard"
            })
                .id(cosmic::widget::Id::new(discard_id))
                .on_press_maybe((!self.native_busy).then_some(discard))
                .into(),
        );
        let apply = slot(apply);
        let status: Element<'a, Message> = container(status).width(Length::Fill).into();
        // At enlarged text sizes the compact rail leaves less room than the
        // normal minimum window. Keep Apply's full label and hit target intact.
        let actions: Element<'a, Message> = if self.content_width() < 640. {
            column::with_children(vec![
                row::with_children(vec![status, extras])
                    .spacing(8)
                    .align_y(Alignment::Center)
                    .into(),
                row::with_children(vec![
                    undo,
                    cosmic::widget::space().width(Length::Fill).into(),
                    discard,
                    apply,
                ])
                .spacing(8)
                .align_y(Alignment::Center)
                .into(),
            ])
            .spacing(6)
            .into()
        } else {
            row::with_children(vec![status, undo, extras, discard, apply])
                .spacing(8)
                .align_y(Alignment::Center)
                .into()
        };
        container(actions)
            .padding([10, 18])
            .width(Length::Fill)
            .into()
    }

    pub(super) fn page_error(&self) -> Option<&str> {
        use crate::settings::desktop::Notice;
        let notices = match self.page {
            Page::DesktopDock => [self.desktop.notice.as_ref(), None, None],
            Page::DesktopAppearance => [self.system_appearance.notice.as_ref(), None, None],
            Page::DesktopWindows => [self.windows.notice(), self.workspaces.notice(), None],
            Page::DesktopWorkspaces => [self.workspaces.notice(), None, None],
            Page::DesktopIcons => [self.desktop_icons.notice(), None, None],
            Page::DesktopWindowRules => [self.window_rules.notice(), None, None],
            Page::DesktopShortcuts => [self.shortcuts.notice(), None, None],
            Page::DesktopProfiles => [self.profiles.notice.as_ref(), None, None],
            Page::DesktopAutomation => [self.automation.notice.as_ref(), None, None],
            Page::AdaptiveLooks => [
                self.automation.notice.as_ref(),
                self.profiles.notice.as_ref(),
                None,
            ],
            Page::Themes => [
                self.profiles.notice.as_ref(),
                self.system_appearance.notice.as_ref(),
                self.wallpaper.notice(),
            ],
            Page::DesktopWallpaper | Page::Appearance => [self.wallpaper.notice(), None, None],
            _ => [None, None, None],
        };
        notices
            .into_iter()
            .flatten()
            .find_map(|notice| match notice {
                Notice::Error(error) => Some(error.as_str()),
                Notice::Done(_) => None,
            })
            .or_else(|| self.error.then_some(self.status.as_str()))
    }
}
