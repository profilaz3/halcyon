//! Shared wording for Restore after an interrupted change, used by Dock &
//! Panel, System appearance and Profiles: one short line for the footer and
//! full details for a disclosure. Both name what was kept because someone
//! changed it afterwards: COSMIC settings, widget settings and the saved
//! Undo entry.
#[cfg(test)]
use crate::desktop::Owner;
use crate::desktop::store::RecoveryReport;
use crate::settings::system_appearance::describe;

/// Names shown in the short line before the rest is counted.
const SHORT: usize = 3;

/// The page that can undo a latest change made by `owner`.
#[cfg(test)]
pub fn owner_page(owner: Owner) -> super::Page {
    match owner {
        Owner::Recovery => super::Page::General,
        Owner::Preferences => super::Page::Widgets,
        Owner::DockPanel => super::Page::DesktopDock,
        Owner::Appearance => super::Page::DesktopAppearance,
        Owner::Profile => super::Page::DesktopProfiles,
        Owner::Windows => super::Page::DesktopWindows,
        Owner::Workspaces => super::Page::DesktopWorkspaces,
        Owner::WindowRules => super::Page::DesktopWindowRules,
        Owner::Shortcuts => super::Page::DesktopShortcuts,
        Owner::Wallpaper => super::Page::DesktopWallpaper,
        Owner::DesktopIcons => super::Page::DesktopIcons,
    }
}

fn kept(report: &RecoveryReport) -> Vec<String> {
    let mut names: Vec<String> = Vec::new();
    if !report.kept_external.is_empty() {
        names.extend(
            describe(&report.kept_external)
                .split(", ")
                .map(String::from),
        );
    }
    names.extend(report.kept_fields.iter().cloned());
    if report.kept_undo {
        names.push("the saved Undo entry".into());
    }
    names
}

fn collapse(names: &[String]) -> String {
    if names.len() <= SHORT {
        return names.join(", ");
    }
    format!(
        "{} and {} more",
        names[..SHORT].join(", "),
        names.len() - SHORT
    )
}

/// One line: “Interrupted change undone; kept newer Dock size, Card palette
/// and 2 more”.
pub fn summary(report: &RecoveryReport) -> String {
    let kept = kept(report);
    if kept.is_empty() {
        "Interrupted change undone".into()
    } else {
        format!("Interrupted change undone; kept newer {}", collapse(&kept))
    }
}

/// Every group in full, for a details disclosure. Empty groups are left out.
pub fn details(report: &RecoveryReport) -> Vec<String> {
    let mut lines = Vec::new();
    if !report.restored.is_empty() {
        lines.push(format!("Restored: {}", describe(&report.restored)));
    }
    if !report.restored_fields.is_empty() {
        lines.push(format!(
            "Widget settings restored: {}",
            report.restored_fields.join(", ")
        ));
    }
    if !report.kept_external.is_empty() {
        lines.push(format!(
            "Changed since and kept: {}",
            describe(&report.kept_external)
        ));
    }
    if !report.kept_fields.is_empty() {
        lines.push(format!(
            "Widget settings changed since and kept: {}",
            report.kept_fields.join(", ")
        ));
    }
    if report.kept_undo {
        lines.push("The saved Undo entry was changed since and kept as it is.".into());
    }
    lines
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::desktop::ConfigKey;

    #[test]
    fn notices_name_kept_widget_settings_and_the_undo_entry() {
        let dock = |name| ConfigKey::new("com.system76.CosmicPanel.Dock", 1, name).unwrap();
        let report = RecoveryReport {
            restored: vec![dock("anchor")],
            unchanged: Vec::new(),
            kept_external: vec![dock("size")],
            restored_fields: vec!["Card opacity".into()],
            kept_fields: vec!["Card palette".into(), "Visible widgets".into()],
            kept_undo: true,
        };
        let line = summary(&report);
        assert!(line.contains("Dock size"), "{line}");
        assert!(line.contains("Card palette"), "{line}");
        // Long lists collapse; details keep every name.
        assert!(line.ends_with("and 1 more"), "{line}");
        let details = details(&report).join("\n");
        for name in [
            "Dock position",
            "Card opacity",
            "Dock size",
            "Visible widgets",
            "Undo entry",
        ] {
            assert!(details.contains(name), "{name}: {details}");
        }
        let quiet = RecoveryReport {
            restored: vec![dock("anchor")],
            unchanged: Vec::new(),
            kept_external: Vec::new(),
            restored_fields: Vec::new(),
            kept_fields: Vec::new(),
            kept_undo: false,
        };
        assert_eq!(summary(&quiet), "Interrupted change undone");
    }
}

#[derive(Default)]
pub(super) struct DesktopRecovery {
    pub info: Option<crate::desktop::recovery::BaselineInfo>,
    pub problem: Option<String>,
    pub plan: Option<crate::desktop::recovery::RestorePlan>,
    pub busy: bool,
}
impl DesktopRecovery {
    pub fn refresh(&mut self) {
        match crate::desktop::DesktopStore::from_env()
            .and_then(|s| crate::desktop::recovery::load_baseline(&s))
        {
            Ok(info) => {
                self.info = info;
                self.problem = None;
            }
            Err(error) => {
                self.info = None;
                self.problem = Some(error.to_string());
            }
        }
    }
    pub fn summary(&self) -> String {
        self.info.as_ref().map_or_else(
            || "Saved before the first desktop change".into(),
            |info| {
                let kind = match info.kind {
                    crate::desktop::recovery::BaselineKind::OriginalDesktop => "Original desktop",
                    crate::desktop::recovery::BaselineKind::CurrentDesktop => "Desktop snapshot",
                };
                format!(
                    "{kind} · {} · {} areas",
                    info.captured_at
                        .split('T')
                        .next()
                        .unwrap_or(&info.captured_at),
                    info.domains.len()
                )
            },
        )
    }
}

pub(super) fn export_task() -> cosmic::app::Task<super::Message> {
    cosmic::task::future(async {
        let result = async {
            let store = crate::desktop::DesktopStore::from_env().map_err(|e| e.to_string())?;
            let bytes =
                crate::desktop::recovery::export_baseline(&store).map_err(|e| e.to_string())?;
            let response = cosmic::dialog::file_chooser::save::Dialog::new()
                .title("Export desktop recovery".to_string())
                .file_name("halcyon-desktop-recovery.json".to_string())
                .save_file()
                .await;
            match response {
                Ok(response) => {
                    let Some(url) = response.url() else {
                        return Ok(None);
                    };
                    let path = url
                        .to_file_path()
                        .map_err(|_| "Choose a local file".to_string())?;
                    write_export(&path, bytes.as_bytes(), store.fixture_root())?;
                    Ok(Some(path))
                }
                Err(cosmic::dialog::file_chooser::Error::Cancelled) => Ok(None),
                Err(e) => Err(e.to_string()),
            }
        }
        .await;
        super::Message::RecoveryExported(result)
    })
}

fn write_export(
    path: &std::path::Path,
    bytes: &[u8],
    confine: Option<&std::path::Path>,
) -> Result<(), String> {
    crate::desktop::recovery::write_export(path, bytes, confine).map_err(|e| e.to_string())
}

#[cfg(test)]
mod export_tests {
    use super::*;
    use std::os::unix::fs::{PermissionsExt, symlink};
    #[test]
    fn task2r1_export_is_private_on_create_and_replace_under_normal_umask() {
        let f = crate::desktop::store::tests::Fixture::new("private-recovery-export");
        struct Umask(libc::mode_t);
        impl Drop for Umask {
            fn drop(&mut self) {
                unsafe {
                    libc::umask(self.0);
                }
            }
        }
        let _mask = Umask(unsafe { libc::umask(0o022) });
        let path = f.0.join("recovery.json");
        for contents in [b"first".as_slice(), b"replacement".as_slice()] {
            write_export(&path, contents, Some(&f.0)).unwrap();
            assert_eq!(std::fs::read(&path).unwrap(), contents);
            assert_eq!(
                std::fs::metadata(&path).unwrap().permissions().mode() & 0o777,
                0o600
            );
            std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600)).unwrap();
        }
    }
    #[test]
    fn task2r1_export_refuses_links_and_fixture_escape_without_mutation() {
        let f = crate::desktop::store::tests::Fixture::new("confined-recovery-export");
        let outside = f.0.join("outside");
        std::fs::create_dir(&outside).unwrap();
        let target = outside.join("keep.json");
        std::fs::write(&target, b"keep").unwrap();
        let link = f.0.join("linked");
        symlink(&outside, &link).unwrap();
        assert!(write_export(&link.join("new.json"), b"private", None).is_err());
        let leaf = f.0.join("leaf.json");
        symlink(&target, &leaf).unwrap();
        assert!(write_export(&leaf, b"private", None).is_err());
        let confined = f.0.join("confined");
        std::fs::create_dir(&confined).unwrap();
        assert!(write_export(&target, b"private", Some(&confined)).is_err());
        assert_eq!(std::fs::read(target).unwrap(), b"keep");
        assert!(!outside.join("new.json").exists());
    }
}
