use super::*;
use cosmic::iced::mouse;
use std::collections::{BTreeMap, BTreeSet};

fn short(value: &str, count: usize) -> String {
    if value.chars().count() > count {
        format!("{}…", value.chars().take(count - 1).collect::<String>())
    } else {
        value.into()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use cosmic::Application;
    use cosmic::iced::advanced::widget::Tree;

    fn project(path: &str, title: &str, identity: &str) -> data::Project {
        serde_json::from_value(serde_json::json!({
            "path": path, "name": title, "title": title, "repository_id": identity,
            "group": "projects", "branch": "main", "changed": 0, "untracked": 0,
            "conflicts": 0, "ahead": 0, "behind": 0, "upstream": false,
            "error": false, "servers": [], "recent": 0.0
        }))
        .unwrap()
    }

    #[test]
    fn linked_worktrees_share_one_title_and_keep_the_most_relevant_open_target() {
        let main = project(
            "/projects/Voice-Agent",
            "Voice Agent",
            "/projects/Voice-Agent/.git",
        );
        let mut active = project(
            "/worktrees/a1b2c3/Voice-Agent",
            "Voice Agent",
            "/projects/Voice-Agent/.git",
        );
        active.servers.push(data::ProjectServer {
            port: 5173,
            web: true,
        });
        let mut conflict = project(
            "/worktrees/repair-abcdef",
            "Voice Agent",
            "/projects/Voice-Agent/.git",
        );
        conflict.conflicts = 1;
        let input = [main, active, conflict];
        let groups = project_groups(&input);
        assert_eq!(groups.len(), 1);
        assert_eq!(groups[0].title, "Voice Agent");
        assert_eq!(groups[0].selected.path, "/worktrees/repair-abcdef");
        assert_eq!(groups[0].web_ports(), vec![5173]);
        assert_eq!(groups[0].status(), "1 conflict");
    }

    #[test]
    fn active_then_changed_then_recent_checkouts_are_selected() {
        let mut main = project("/projects/app", "App", "same-repository");
        main.recent = 200.0;
        let mut changed = project("/worktrees/changed", "App", "same-repository");
        changed.changed = 1;
        let mut live = project("/worktrees/live", "App", "same-repository");
        live.servers.push(data::ProjectServer {
            port: 3000,
            web: true,
        });
        let input = [main, changed, live];
        assert_eq!(project_groups(&input)[0].selected.path, "/worktrees/live");
        assert_eq!(
            project_groups(&input[..2])[0].selected.path,
            "/worktrees/changed"
        );
        let mut input = input;
        input[1].changed = 0;
        assert_eq!(
            project_groups(&input[..2])[0].selected.path,
            "/projects/app"
        );
    }

    #[test]
    fn different_repositories_are_not_merged_just_because_their_titles_match() {
        let input = [
            project("/client-one/app", "App", "one"),
            project("/client-two/app", "App", "two"),
        ];
        assert_eq!(project_groups(&input).len(), 2);
    }

    #[test]
    fn old_same_title_snapshots_group_without_exposing_generated_names() {
        let input = [
            project("/projects/Voice-Agent", "Voice-Agent", ""),
            project("/worktrees/1234/Voice-Agent", "Voice-Agent-abcdef12", ""),
        ];
        let groups = project_groups(&input);
        assert_eq!(groups.len(), 1);
        assert_eq!(groups[0].title, "Voice Agent");
    }

    #[test]
    fn legacy_nested_worktree_title_uses_the_repository_name() {
        let mut worktree = project(
            "/projects/Voice-Agent/.claude/worktrees/history-callbacks-design-6f7515",
            "history-callbacks-design-6f7515",
            "",
        );
        worktree.title.clear();
        worktree.group = "worktrees".into();
        let input = [
            project("/projects/Voice-Agent", "Voice-Agent", ""),
            worktree,
        ];
        let groups = project_groups(&input);
        assert_eq!(groups.len(), 1);
        assert_eq!(groups[0].title, "Voice Agent");
    }

    #[test]
    fn unknown_or_path_titles_do_not_leak_paths_or_merge_unrelated_projects() {
        let mut first = project("/projects/one", "/tmp/private/project-abcdef12", "");
        first.name = "Unknown".into();
        let mut second = project("/projects/two", "", "");
        second.name = "".into();
        let input = [first, second];
        let groups = project_groups(&input);
        assert_eq!(groups.len(), 2);
        assert!(groups.iter().all(|group| group.title == "Untitled project"));
    }

    #[test]
    fn titles_preserve_product_capitalization_and_reject_generated_placeholders() {
        assert_eq!(title("iOS Studio"), Some("iOS Studio".into()));
        assert_eq!(title("unknown-deadbeef"), None);
        assert_eq!(title("Untitled project"), None);
    }

    #[test]
    fn branch_labels_drop_generated_suffixes_but_keep_meaningful_branch_names() {
        assert_eq!(
            branch_label("claude/history-callbacks-design-6f7515"),
            "history-callbacks-design"
        );
        assert_eq!(
            branch_label("feature/payment-retry"),
            "feature/payment-retry"
        );
        assert_eq!(branch_label("abc12345 · detached"), "Detached HEAD");
        assert_eq!(branch_label("/tmp/private/checkout"), "Branch unavailable");
    }

    #[test]
    fn only_absolute_local_checkout_paths_can_be_opened() {
        assert_eq!(
            checkout_target("/tmp/Voice Agent"),
            Some("/tmp/Voice Agent".into())
        );
        for path in [
            "https://example.com/project",
            "file:///tmp/project",
            "../project",
            "--help",
            "",
            "/tmp/bad\npath",
        ] {
            assert_eq!(checkout_target(path), None, "{path:?}");
        }
    }

    #[test]
    fn grouped_changes_errors_and_ports_are_not_lost_or_double_counted() {
        let mut first = project("/projects/app", "App", "same");
        first.changed = 2;
        first.servers = vec![data::ProjectServer {
            port: 3000,
            web: true,
        }];
        let mut second = project("/worktrees/app", "App", "same");
        second.untracked = 3;
        second.error = true;
        second.servers = vec![
            data::ProjectServer {
                port: 3000,
                web: true,
            },
            data::ProjectServer {
                port: 9229,
                web: false,
            },
        ];
        let input = [first.clone(), first, second];
        let groups = project_groups(&input);
        assert_eq!(groups[0].checkouts.len(), 2);
        assert_eq!(groups[0].status(), "2 edits · 3 new");
        assert_eq!(groups[0].unavailable(), 1);
        assert_eq!(groups[0].web_ports(), vec![3000]);
        assert_eq!(groups[0].other_ports(), 1);
    }

    #[test]
    fn project_height_fits_grouped_content_and_grows_for_port_rows() {
        let (mut cards, _) = Cards::init(Core::default(), Config::default());
        let placeholder = cards.projects_natural_height();
        let main = project("/projects/app", "Voice Agent", "same");
        let linked = project("/worktrees/app", "Voice Agent", "same");
        cards.projects = Some(ProjectsData {
            updated_at: String::new(),
            projects: vec![main, linked],
        });
        let one_group = cards.projects_natural_height();
        assert!(one_group > placeholder && one_group < 220.);
        cards.projects.as_mut().unwrap().projects.pop();
        assert_eq!(cards.projects_natural_height(), one_group);
        cards.projects.as_mut().unwrap().projects[0].servers = (3000..3003)
            .map(|port| data::ProjectServer { port, web: true })
            .collect();
        let one_port_row = cards.projects_natural_height();
        assert!(one_port_row > one_group);
        cards.projects.as_mut().unwrap().projects[0]
            .servers
            .push(data::ProjectServer {
                port: 3003,
                web: true,
            });
        assert!(cards.projects_natural_height() > one_port_row);
        cards
            .projects
            .as_mut()
            .unwrap()
            .projects
            .extend((0..6).map(|index| {
                project(
                    &format!("/other/{index}"),
                    "Another project",
                    &index.to_string(),
                )
            }));
        assert_eq!(cards.projects_natural_height(), 300.);
        cards.all_projects = true;
        assert_eq!(cards.projects_natural_height(), 300.);
    }

    #[test]
    fn project_height_reserves_space_for_wrapped_titles_and_partial_warnings() {
        let (mut cards, _) = Cards::init(Core::default(), Config::default());
        cards.projects = Some(ProjectsData {
            updated_at: String::new(),
            projects: vec![project("/projects/app", "App", "same")],
        });
        let compact = cards.projects_natural_height();
        let mut missing = project("/worktrees/missing", "App", "same");
        missing.error = true;
        cards.projects.as_mut().unwrap().projects.push(missing);
        assert!(cards.projects_natural_height() > compact);
        cards.projects.as_mut().unwrap().projects.pop();
        cards.projects.as_mut().unwrap().projects[0].title =
            "A deliberately long project title that needs another visible line".into();
        assert!(cards.projects_natural_height() > compact);
    }

    #[test]
    fn checkout_updates_and_expansion_preserve_widget_state() {
        let (mut cards, _) = Cards::init(Core::default(), Config::default());
        let mut tree = Tree::new(cards.projects_card().as_widget());
        cards.projects = Some(ProjectsData {
            updated_at: Local::now().to_rfc3339(),
            projects: (0..6)
                .map(|i| data::Project {
                    path: format!("/tmp/project-{i}"),
                    name: format!("Long checkout name {i}"),
                    title: format!("Project {i}"),
                    repository_id: format!("project-{i}"),
                    recent: 0.,
                    group: "worktrees".into(),
                    branch: "feature/a-long-branch-name-for-a-worktree".into(),
                    changed: i,
                    untracked: 1,
                    conflicts: usize::from(i == 1),
                    ahead: 2,
                    behind: 3,
                    upstream: true,
                    error: i == 2,
                    servers: if i == 0 {
                        (3000..3007)
                            .map(|port| data::ProjectServer { port, web: true })
                            .collect()
                    } else {
                        vec![]
                    },
                })
                .collect(),
        });
        for expanded in [false, true, false] {
            cards.all_projects = expanded;
            tree.diff(cards.projects_card().as_widget_mut());
        }
        cards.projects.as_mut().unwrap().projects.clear();
        tree.diff(cards.projects_card().as_widget_mut());
        cards.projects = None;
        cards.projects_error = true;
        tree.diff(cards.projects_card().as_widget_mut());
    }
}

fn badge<'a>(value: String, color: Color, p: Palette) -> Element<'a, Message> {
    container(label(value, 11., color))
        .padding([3, 6])
        .class(cosmic::theme::Container::custom(move |_| {
            cosmic::iced::widget::container::Style {
                background: Some(Background::Color(mix(color, p.background, 0.10))),
                border: Border {
                    radius: 6.into(),
                    ..Default::default()
                },
                ..Default::default()
            }
        }))
        .into()
}

/// Refresh as a quiet round icon, like the Notes card's copy and edit controls.
fn refresh_button<'a>(p: Palette) -> Element<'a, Message> {
    mouse_area(
        container(
            cosmic::widget::icon::from_name("view-refresh-symbolic")
                .size(14)
                .icon(),
        )
        // 22px tall, so the header keeps its fixed 44px.
        .padding(4)
        .id(cosmic::widget::Id::new("projects-refresh"))
        .class(cosmic::theme::Container::custom(move |_| {
            cosmic::iced::widget::container::Style {
                icon_color: Some(p.muted),
                border: Border {
                    radius: 40.into(),
                    ..Default::default()
                },
                ..Default::default()
            }
        })),
    )
    .on_press(Message::RefreshProjects)
    .interaction(mouse::Interaction::Pointer)
    .into()
}

/// A text link that starts on the card's text edge, unlike `action`'s
/// padded label.
fn text_action<'a>(
    value: impl Into<String>,
    message: Message,
    color: Color,
) -> Element<'a, Message> {
    mouse_area(container(label(value, 12., color)).padding([2, 0]))
        .on_press(message)
        .interaction(mouse::Interaction::Pointer)
        .into()
}

const UNSET_MESSAGE: &str = "Your Git projects show up here.";

fn without_generated_suffix(value: &str) -> &str {
    value
        .rsplit_once(['-', '_'])
        .filter(|(_, suffix)| suffix.len() >= 6 && suffix.chars().all(|c| c.is_ascii_hexdigit()))
        .map_or(value, |(name, _)| name)
}

fn title(value: &str) -> Option<String> {
    if value.chars().any(char::is_control) {
        return None;
    }
    let value = without_generated_suffix(value.trim()).trim();
    if value.is_empty()
        || value.contains(['/', '\\'])
        || matches!(
            value.to_lowercase().as_str(),
            "unknown" | "none" | "null" | "untitled" | "untitled project"
        )
        || (value.len() >= 6 && value.chars().all(|c| c.is_ascii_hexdigit()))
    {
        return None;
    }
    if !value.contains(['-', '_']) {
        return Some(value.split_whitespace().collect::<Vec<_>>().join(" "));
    }
    let value = value.replace(['-', '_'], " ");
    let words = value
        .split_whitespace()
        .map(|word| {
            let mut chars = word.chars();
            chars
                .next()
                .map(|first| first.to_uppercase().chain(chars).collect::<String>())
                .unwrap_or_default()
        })
        .collect::<Vec<_>>();
    (!words.is_empty()).then(|| words.join(" "))
}

fn project_title(project: &data::Project) -> String {
    title(&project.title)
        .or_else(|| {
            // Compatibility with older snapshots: Claude worktrees live inside
            // their primary repository, so their folder slug is not a title.
            (project.group == "worktrees")
                .then(|| {
                    std::path::Path::new(&project.path)
                        .ancestors()
                        .find_map(|ancestor| {
                            if ancestor.file_name()? == "worktrees"
                                && ancestor.parent()?.file_name()? == ".claude"
                            {
                                title(ancestor.parent()?.parent()?.file_name()?.to_str()?)
                            } else {
                                None
                            }
                        })
                })
                .flatten()
        })
        .or_else(|| title(&project.name))
        .unwrap_or_else(|| "Untitled project".into())
}

fn branch_label(branch: &str) -> String {
    if branch.ends_with(" · detached") || branch == "(detached)" {
        return "Detached HEAD".into();
    }
    let branch = branch.trim();
    if branch.is_empty()
        || branch == "Unknown"
        || branch.starts_with(['/', '~'])
        || branch.chars().any(char::is_control)
    {
        return "Branch unavailable".into();
    }
    let branch = branch
        .strip_prefix("claude/")
        .or_else(|| branch.strip_prefix("codex/"))
        .unwrap_or(branch);
    without_generated_suffix(branch).into()
}

fn checkout_target(path: &str) -> Option<String> {
    (std::path::Path::new(path).is_absolute() && !path.chars().any(char::is_control))
        .then(|| path.into())
}

fn open_checkout<'a>(content: Element<'a, Message>, path: &str) -> Element<'a, Message> {
    let area = mouse_area(content);
    match checkout_target(path) {
        Some(path) => area
            .on_press(Message::Open(path))
            .interaction(mouse::Interaction::Pointer)
            .into(),
        None => area.into(),
    }
}

fn relevance(a: &data::Project, b: &data::Project) -> std::cmp::Ordering {
    let state = |project: &data::Project| {
        (
            project.conflicts > 0,
            !project.error,
            project.servers.iter().any(|s| s.web),
            project.changed + project.untracked > 0,
        )
    };
    let recent = |project: &data::Project| {
        if project.recent.is_finite() {
            project.recent
        } else {
            0.
        }
    };
    state(b)
        .cmp(&state(a))
        .then_with(|| recent(b).total_cmp(&recent(a)))
        .then_with(|| a.path.cmp(&b.path))
}

struct ProjectGroup<'a> {
    id: String,
    title: String,
    selected: &'a data::Project,
    checkouts: Vec<&'a data::Project>,
}

impl ProjectGroup<'_> {
    fn conflicts(&self) -> usize {
        self.checkouts.iter().map(|project| project.conflicts).sum()
    }

    fn unavailable(&self) -> usize {
        self.checkouts
            .iter()
            .filter(|project| project.error)
            .count()
    }

    fn changes(&self) -> usize {
        self.checkouts
            .iter()
            .map(|project| project.changed + project.untracked)
            .sum()
    }

    fn status(&self) -> String {
        let conflicts = self.conflicts();
        if conflicts > 0 {
            return format!(
                "{conflicts} {}",
                if conflicts == 1 {
                    "conflict"
                } else {
                    "conflicts"
                }
            );
        }
        if self.unavailable() == self.checkouts.len() {
            return "Unavailable".into();
        }
        let changed: usize = self.checkouts.iter().map(|project| project.changed).sum();
        let untracked: usize = self.checkouts.iter().map(|project| project.untracked).sum();
        let mut parts = Vec::new();
        if changed > 0 {
            parts.push(format!(
                "{changed} {}",
                if changed == 1 { "edit" } else { "edits" }
            ));
        }
        if untracked > 0 {
            parts.push(format!("{untracked} new"));
        }
        if parts.is_empty() {
            "Clean".into()
        } else {
            parts.join(" · ")
        }
    }

    fn web_ports(&self) -> Vec<u16> {
        self.checkouts
            .iter()
            .flat_map(|project| &project.servers)
            .filter(|server| server.web)
            .map(|server| server.port)
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect()
    }

    fn other_ports(&self) -> usize {
        self.checkouts
            .iter()
            .flat_map(|project| &project.servers)
            .filter(|server| !server.web)
            .map(|server| server.port)
            .collect::<BTreeSet<_>>()
            .len()
    }

    fn branch_count(&self) -> usize {
        self.checkouts
            .iter()
            .map(|project| project.branch.as_str())
            .collect::<BTreeSet<_>>()
            .len()
    }
}

fn project_groups(projects: &[data::Project]) -> Vec<ProjectGroup<'_>> {
    let mut grouped: BTreeMap<String, Vec<&data::Project>> = BTreeMap::new();
    for (index, project) in projects.iter().enumerate() {
        let title = project_title(project);
        let key = if !project.repository_id.is_empty() {
            format!("repository:{}", project.repository_id)
        } else if title != "Untitled project" {
            format!("title:{}", title.to_lowercase())
        } else {
            format!("unknown:{}:{index}", project.path)
        };
        let checkouts = grouped.entry(key).or_default();
        if !checkouts
            .iter()
            .any(|existing| existing.path == project.path)
        {
            checkouts.push(project);
        }
    }
    let mut groups: Vec<_> = grouped
        .into_iter()
        .map(|(id, mut checkouts)| {
            checkouts.sort_by(|a, b| relevance(a, b));
            let selected = checkouts[0];
            ProjectGroup {
                id,
                title: project_title(selected),
                selected,
                checkouts,
            }
        })
        .collect();
    groups.sort_by(|a, b| relevance(a.selected, b.selected).then_with(|| a.title.cmp(&b.title)));
    groups
}

pub(super) fn has_group(projects: &[data::Project], id: &str) -> bool {
    project_groups(projects)
        .iter()
        .any(|group| group.id == id && group.checkouts.len() > 1)
}

fn checkout_row(project: &data::Project, p: Palette) -> Element<'static, Message> {
    let status = if project.error {
        "Unavailable".into()
    } else if project.conflicts > 0 {
        format!("{} conflicts", project.conflicts)
    } else if project.changed + project.untracked > 0 {
        format!("{} changes", project.changed + project.untracked)
    } else {
        "Clean".into()
    };
    let sync = if project.upstream && project.ahead + project.behind > 0 {
        match (project.ahead, project.behind) {
            (0, behind) => format!("↓{behind}"),
            (ahead, 0) => format!("↑{ahead}"),
            (ahead, behind) => format!("↑{ahead} ↓{behind}"),
        }
    } else {
        String::new()
    };
    open_checkout(
        container(
            column::with_children(vec![
                spread(
                    label(
                        format!("↳ {}", short(&branch_label(&project.branch), 32)),
                        11.,
                        p.foreground,
                    ),
                    label("↗", 11., p.muted),
                ),
                spread(
                    label(
                        status,
                        11.,
                        if project.error || project.conflicts > 0 {
                            Color::from_rgb8(245, 145, 130)
                        } else {
                            p.muted
                        },
                    ),
                    label(sync, 11., p.muted),
                ),
            ])
            .spacing(2)
            .width(Length::Fill),
        )
        .padding([4, 6])
        .id(cosmic::widget::Id::new(format!(
            "project-checkout:{}",
            project.path
        )))
        .width(Length::Fill)
        .into(),
        &project.path,
    )
}

fn estimated_text_lines(value: &str, size: f32, available: f32) -> f32 {
    // Conservative Fira Sans advances, with word wrapping and a glyph fallback
    // for long tokens. This is only used to size the layout editor's surface.
    let available = available.max(1.);
    let mut lines = 1.;
    let mut used = 0.;
    for word in value.split_whitespace() {
        let width: f32 = word
            .chars()
            .map(|character| {
                size * match character {
                    'W' | 'M' | 'w' | 'm' => 0.95,
                    'i' | 'l' | 'I' | '.' | ',' | ':' | '!' | '|' => 0.35,
                    c if !c.is_ascii() => 1.,
                    _ => 0.62,
                }
            })
            .sum();
        let space = if used > 0. { size * 0.35 } else { 0. };
        if used > 0. && used + space + width > available {
            lines += 1.;
            used = 0.;
        } else {
            used += space;
        }
        let wrapped = (width / available).ceil().max(1.);
        lines += wrapped - 1.;
        used += if wrapped > 1. {
            width - (wrapped - 1.) * available
        } else {
            width
        };
    }
    lines
}

/// Immutable inputs shared by desktop and Settings; no jobs or runtime handles.
pub(super) struct Presentation<'a> {
    pub config: &'a Config,
    pub projects: Option<&'a ProjectsData>,
    pub projects_fetching: bool,
    pub projects_error: bool,
    pub all_projects: bool,
    pub expanded_projects: &'a BTreeSet<String>,
}
impl Presentation<'_> {
    pub(super) fn natural_height(&self, width: f32) -> f32 {
        const CHROME: f32 = 116.; // Same header, footer, gaps and frame as projects_card.
        let groups = self
            .projects
            .as_ref()
            .map(|data| project_groups(&data.projects))
            .unwrap_or_default();
        if self.projects.is_some() && groups.is_empty() {
            // Title, message, gaps, the Choose folders chip and the frame,
            // as drawn by `unset_view`.
            let content = estimated_text_lines(UNSET_MESSAGE, 13., width - 42.) * 16.25;
            return (36. + 23. + 8. + content + 8. + 25.).ceil();
        }
        if groups.is_empty() {
            let message = if self.projects_error {
                "Status unavailable · retrying"
            } else {
                "Reading your projects…"
            };
            let content = estimated_text_lines(message, 13., width - 42.) * 16.25;
            // The compact fallback card uses 10px gaps in place of 8px gaps.
            return (CHROME + content + if content < 44. { 4. } else { 0. })
                .ceil()
                .min(300.);
        }
        let count = if self.all_projects {
            groups.len()
        } else {
            groups.len().min(3)
        };
        let mut height = CHROME + count.saturating_sub(1) as f32 * 6.;
        for group in groups.iter().take(count) {
            let ports = group.web_ports();
            let title_width = width - 72. - if ports.is_empty() { 10. } else { 36. };
            let title_lines = estimated_text_lines(&group.title, 15., title_width);
            let project = group.selected;
            let upstream_width = if project.upstream && project.ahead + project.behind > 0 {
                12. + (project.ahead.to_string().len() + project.behind.to_string().len()) as f32
                    * 6.
            } else {
                0.
            };
            let branch_lines = estimated_text_lines(
                &format!("↳ {}", short(&branch_label(&project.branch), 30)),
                11.,
                width - 72. - upstream_width,
            );
            height += 87. + (title_lines - 1.) * 21. + (branch_lines - 1.) * 13.75;
            if self.expanded_projects.contains(&group.id) {
                // Expanded rows replace the selected branch's summary line.
                height -= 5. + branch_lines * 13.75;
                height += group
                    .checkouts
                    .iter()
                    .map(|project| {
                        37.5 + 7.
                            + (estimated_text_lines(
                                &format!("↳ {}", short(&branch_label(&project.branch), 32)),
                                11.,
                                width - 98.,
                            ) - 1.)
                                * 13.75
                    })
                    .sum::<f32>();
            }
            height += ports.len().div_ceil(3) as f32 * 26.;
            if group.other_ports() > 0 {
                height += 20.75;
            }
            if group.unavailable() > 0 && group.unavailable() < group.checkouts.len() {
                height += 20.75;
            }
        }
        height.ceil().min(300.)
    }

    /// No folders chosen yet: a title, one line of guidance and the way to
    /// choose them. Nothing to refresh, and no room kept for a summary.
    fn unset_view(&self, limit: f32, explicit: bool) -> Element<'static, Message> {
        let p = self.config.palette();
        card_frame(
            column::with_children(vec![
                container(label("Projects", crate::widget_style::TITLE, p.foreground))
                    .height(Length::Fixed(23.))
                    .align_y(Alignment::Center)
                    .into(),
                label(UNSET_MESSAGE, 13., p.muted),
                crate::tools::cards::chip(
                    None,
                    "Choose folders",
                    Some(Message::ProjectSettings),
                    p,
                    true,
                ),
            ])
            .spacing(8)
            .width(Length::Fill)
            .into(),
            p,
            self.config.appearance,
            if explicit {
                Length::Fixed(limit)
            } else {
                Length::Shrink
            },
        )
    }

    pub(super) fn view(&self, limit: f32, explicit: bool) -> Element<'static, Message> {
        let _style = crate::widget_style::scope(self.config);
        if self
            .projects
            .as_ref()
            .is_some_and(|data| project_groups(&data.projects).is_empty())
        {
            return self.unset_view(limit, explicit);
        }
        let p = self.config.palette();
        let mut header = vec![spread(
            label("Projects", crate::widget_style::TITLE, p.foreground),
            if self.projects_fetching {
                label("Syncing…", crate::widget_style::SECONDARY, p.muted)
            } else {
                refresh_button(p)
            },
        )];
        let mut entries = Vec::new();
        let mut footer = label("Local project status", 11., p.muted);
        if let Some(data) = &self.projects {
            let groups = project_groups(&data.projects);
            let servers = groups
                .iter()
                .flat_map(ProjectGroup::web_ports)
                .collect::<BTreeSet<_>>()
                .len();
            let summary = label(
                format!(
                    "{} {}",
                    groups.len(),
                    if groups.len() == 1 {
                        "project"
                    } else {
                        "projects"
                    }
                ),
                crate::widget_style::SECONDARY,
                p.muted,
            );
            header.push(if servers > 0 {
                spread(
                    summary,
                    label(
                        format!(
                            "● {servers} dev {}",
                            if servers == 1 { "server" } else { "servers" }
                        ),
                        crate::widget_style::SECONDARY,
                        p.accent,
                    ),
                )
            } else {
                summary
            });
            let count = if self.all_projects { groups.len() } else { 3 };
            for group in groups.iter().take(count) {
                let project = group.selected;
                let web = group.web_ports();
                let live = !web.is_empty();
                let warning = group.unavailable() > 0 || group.conflicts() > 0;
                let color = if warning {
                    Color::from_rgb8(245, 145, 130)
                } else if group.changes() > 0 {
                    Color::from_rgb8(233, 192, 124)
                } else {
                    p.muted
                };
                let name = text::body(group.title.clone())
                    .size(15)
                    .font(Font {
                        weight: cosmic::iced::font::Weight::Semibold,
                        ..crate::widget_style::font()
                    })
                    .class(cosmic::theme::Text::Color(p.foreground))
                    .width(Length::Fill)
                    .into();
                let expanded = self.expanded_projects.contains(&group.id);
                let mut summary = vec![
                    spread(
                        name,
                        label(
                            if live { "● Live" } else { "↗" },
                            11.,
                            if live { p.accent } else { p.muted },
                        ),
                    ),
                    spread(
                        label(
                            format!("↳ {}", short(&branch_label(&project.branch), 30)),
                            11.,
                            p.muted,
                        ),
                        label(
                            if project.upstream && project.ahead + project.behind > 0 {
                                match (project.ahead, project.behind) {
                                    (0, behind) => format!("↓{behind}"),
                                    (ahead, 0) => format!("↑{ahead}"),
                                    (ahead, behind) => format!("↑{ahead} ↓{behind}"),
                                }
                            } else {
                                String::new()
                            },
                            11.,
                            p.muted,
                        ),
                    ),
                ];
                if expanded {
                    summary.pop();
                }
                let mut lines = vec![open_checkout(
                    column::with_children(summary)
                        .spacing(5)
                        .width(Length::Fill)
                        .into(),
                    &project.path,
                )];
                let branches = if group.checkouts.len() > 1 {
                    let count = group.checkouts.len();
                    let noun = if group.branch_count() == count {
                        "branches"
                    } else {
                        "checkouts"
                    };
                    mouse_area(
                        container(label(
                            format!("{count} {noun} {}", if expanded { "↑" } else { "↓" }),
                            11.,
                            p.accent,
                        ))
                        .padding([2, 4])
                        .id(cosmic::widget::Id::new(format!(
                            "project-branches:{}",
                            group.id
                        ))),
                    )
                    .on_press(Message::ToggleProject(group.id.clone()))
                    .interaction(mouse::Interaction::Pointer)
                    .into()
                } else {
                    label("", 11., p.muted)
                };
                // The disclosure is a sibling of the folder link: expanding a
                // repository must never open a checkout as a side effect.
                lines.push(spread(badge(group.status(), color, p), branches));
                if expanded {
                    lines.extend(
                        group
                            .checkouts
                            .iter()
                            .map(|project| checkout_row(project, p)),
                    );
                }
                for ports in web.chunks(3) {
                    lines.push(
                        row::with_children(ports.iter().map(|port| {
                            action(
                                format!(":{port} ↗"),
                                Message::Open(format!("http://127.0.0.1:{port}")),
                                p.accent,
                            )
                        }))
                        .spacing(6)
                        .into(),
                    );
                }
                let other = group.other_ports();
                if other > 0 {
                    lines.push(label(
                        format!(
                            "{other} other listening {}",
                            if other == 1 { "port" } else { "ports" }
                        ),
                        11.,
                        p.muted,
                    ));
                }
                if group.unavailable() > 0 && group.unavailable() < group.checkouts.len() {
                    lines.push(label(
                        format!(
                            "{} {} unavailable",
                            group.unavailable(),
                            if group.unavailable() == 1 {
                                "branch"
                            } else {
                                "branches"
                            }
                        ),
                        11.,
                        color,
                    ));
                }
                entries.push(
                    container(column::with_children(lines).spacing(7).width(Length::Fill))
                        .padding([10, 11])
                        .width(Length::Fill)
                        .class(cosmic::theme::Container::custom(move |_| {
                            cosmic::iced::widget::container::Style {
                                background: Some(Background::Color(mix(
                                    if warning { color } else { p.accent },
                                    p.background,
                                    if warning { 0.08 } else { 0.055 },
                                ))),
                                border: Border {
                                    color: Color {
                                        a: if warning { 0.35 } else { 0.12 },
                                        ..if warning { color } else { p.accent }
                                    },
                                    width: 1.,
                                    radius: 12.into(),
                                },
                                ..Default::default()
                            }
                        }))
                        .into(),
                );
            }
            footer = spread(
                if groups.len() > 3 {
                    text_action(
                        if self.all_projects {
                            "Show fewer ↑".into()
                        } else {
                            format!("All {} projects ↓", groups.len())
                        },
                        Message::ToggleProjects,
                        p.accent,
                    )
                } else {
                    label("", crate::widget_style::META, p.muted)
                },
                label(
                    timestamp_text(
                        Some(&data.updated_at),
                        self.projects_error,
                        Local::now(),
                        self.config.clock.format_24h,
                    ),
                    11.,
                    if self.projects_error {
                        Color::from_rgb8(233, 192, 124)
                    } else {
                        p.muted
                    },
                ),
            );
        } else {
            entries.push(label(
                if self.projects_error {
                    "Status unavailable · retrying"
                } else {
                    "Reading your projects…"
                },
                13.,
                p.muted,
            ));
        }
        let header = container(column::with_children(header).spacing(5)).height(Length::Fixed(44.));
        let entries = column::with_children(entries)
            .spacing(6)
            .width(Length::Fill);
        let footer = container(footer).height(Length::Fixed(20.));
        // The shared column uses the same bounded logical frame as placement
        // and input. Its contents scroll inside that frame instead of shrinking
        // the native tree behind the independently published geometry.
        const CHROME: f32 = 116.; // header + footer + two gaps + frame padding
        if limit < CHROME + 44. {
            return card(
                vec![header.into(), entries.into(), footer.into()],
                p,
                self.config.appearance,
                Widget::Projects,
                limit,
            );
        }
        let list = container(
            scrollable(container(entries).padding([0, 6, 0, 0]).width(Length::Fill))
                .id(cosmic::widget::Id::new(Widget::Projects.key()))
                .width(Length::Fill)
                .height(if explicit {
                    Length::Fill
                } else {
                    Length::Shrink
                })
                .scroller_width(2.)
                .scrollbar_width(2.)
                .scrollbar_padding(2.),
        )
        .max_height(limit - CHROME)
        .height(if explicit {
            Length::Fill
        } else {
            Length::Shrink
        })
        .width(Length::Fill);
        card_frame(
            column::with_children(vec![header.into(), list.into(), footer.into()])
                .spacing(8)
                .width(Length::Fill)
                .height(if explicit {
                    Length::Fill
                } else {
                    Length::Shrink
                })
                .into(),
            p,
            self.config.appearance,
            if explicit {
                Length::Fixed(limit)
            } else {
                Length::Shrink
            },
        )
    }
}

impl Cards {
    fn projects_presentation(&self) -> Presentation<'_> {
        Presentation {
            config: &self.config,
            projects: self.projects.as_ref(),
            projects_fetching: self.projects_fetching,
            projects_error: self.projects_error,
            all_projects: self.all_projects,
            expanded_projects: &self.expanded_projects,
        }
    }
    pub(super) fn projects_natural_height(&self) -> f32 {
        let width = self
            .explicit_size(Widget::Projects)
            .map(|size| size[0])
            .unwrap_or_else(|| {
                360_f32.min((self.widget_screen(Widget::Projects)[0] - 48.).max(1.))
            });
        self.projects_presentation().natural_height(width)
    }
    pub(super) fn projects_card(&self) -> Element<'_, Message> {
        self.projects_presentation().view(
            self.card_height(Widget::Projects),
            !self.custom_layout() || self.explicit_size(Widget::Projects).is_some(),
        )
    }
}
