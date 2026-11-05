//! Desktop icons editor state: one draft of cosmic-files' desktop struct,
//! written only by Apply as one change owned by this page. cosmic-files
//! redraws the desktop with an outside change only at its next desktop
//! refresh, so Apply says so instead of pretending to refresh.
use crate::desktop::DesktopStore;
use crate::desktop::desktop_icons::{DesktopIconsDraft, SCALE_STEP, Show};
use crate::desktop::store::{Journal, Owner};
use crate::settings::desktop::Notice;
use crate::settings::drafts::DraftEditor;
use std::path::{Path, PathBuf};

/// The deferred-refresh wording (spec §4).
pub const REFRESH_NOTE: &str = crate::desktop::desktop_icons::REFRESH_NOTE;
pub const APPLIED: &str = "Applied. The desktop shows it after its next refresh, for example when you sign out and back in.";
pub const UNAVAILABLE: &str = "Desktop icons are drawn by COSMIC Files, which isn’t installed";

#[derive(Clone, Debug, PartialEq)]
pub enum Message {
    Size(u16),
    Spacing(u16),
    Show(Show, bool),
    Apply,
    Discard,
    Undo,
    Refresh,
    UseCurrent,
    /// Keeps the edits over a value changed outside this editor.
    KeepMine,
    Recover,
    KeepRecovery,
}

impl Message {
    /// Messages that write desktop settings or the change journal.
    pub fn writes(&self) -> bool {
        matches!(
            self,
            Self::Apply | Self::Undo | Self::Recover | Self::KeepRecovery
        )
    }
}

fn on_path(path: Option<&std::ffi::OsStr>) -> bool {
    crate::desktop::desktop_icons::files_installed(path)
}

pub struct DesktopIconsEditor {
    pub(super) core: DraftEditor<DesktopIconsDraft>,
    available: bool,
}

impl Default for DesktopIconsEditor {
    fn default() -> Self {
        Self {
            core: DraftEditor::new(Owner::DesktopIcons, "Desktop icons"),
            available: false,
        }
    }
}

impl DesktopIconsEditor {
    pub fn open(&mut self) {
        self.core.open();
        self.available = Self::available_for(
            self.core.store().and_then(DesktopStore::fixture_root),
            std::env::var_os("PATH").as_deref(),
        );
    }
    #[cfg(test)]
    pub fn with_store(store: DesktopStore, path: Option<&std::ffi::OsStr>) -> Self {
        let available = Self::available_for(store.fixture_root(), path);
        Self {
            core: DraftEditor::with_store(Owner::DesktopIcons, "Desktop icons", store),
            available,
        }
    }
    /// A fixture always has the page; otherwise COSMIC Files must be
    /// installed.
    fn available_for(fixture: Option<&Path>, path: Option<&std::ffi::OsStr>) -> bool {
        fixture.is_some() || on_path(path)
    }
    /// Whether COSMIC Files draws the desktop here (checked on open).
    pub fn available(&self) -> bool {
        self.available
    }
    pub fn opened(&self) -> bool {
        self.core.opened()
    }
    pub fn store(&self) -> Option<&DesktopStore> {
        self.core.store()
    }
    pub fn unavailable(&self) -> Option<&str> {
        self.core.unavailable()
    }
    pub fn writable(&self) -> bool {
        self.core.writable()
    }
    pub fn draft(&self) -> Option<&DesktopIconsDraft> {
        self.core.draft()
    }
    pub fn notice(&self) -> Option<&Notice> {
        self.core.notice.as_ref()
    }
    pub fn dirty(&self) -> bool {
        self.core.dirty()
    }
    pub fn recovery(&self) -> Option<&Result<Journal, String>> {
        self.core.recovery()
    }
    pub fn can_undo(&self) -> bool {
        self.available && self.core.can_undo()
    }
    pub fn can_apply(&self) -> bool {
        self.available && self.core.can_apply()
    }
    pub fn watch_paths(&self) -> Vec<PathBuf> {
        self.core.watch_paths()
    }
    pub fn state_key(&self) -> String {
        format!(
            "{}:{}:{:?}",
            self.core.state_key(),
            self.available,
            self.draft().map(DesktopIconsDraft::edits)
        )
    }
    pub fn refresh(&mut self) {
        self.core.refresh();
    }

    pub fn update(&mut self, message: Message) {
        if !self.available {
            return;
        }
        // Settings this version can't read are kept, never edited.
        let editable = self
            .draft()
            .is_some_and(|draft| draft.config().known().is_some());
        match message {
            Message::Size(_) | Message::Spacing(_) | Message::Show(..) if !editable => {}
            // The sliders' steps only; a stored value between steps stays
            // until the slider moves.
            Message::Size(percent) if percent % SCALE_STEP == 0 => {
                self.core.edit(|draft| draft.set_icon_size(percent))
            }
            Message::Spacing(percent) if percent % SCALE_STEP == 0 => {
                self.core.edit(|draft| draft.set_spacing(percent))
            }
            Message::Size(_) | Message::Spacing(_) => {}
            Message::Show(what, show) => self.core.edit(|draft| draft.set_show(what, show)),
            Message::Apply => {
                if self.can_apply() && self.core.apply() {
                    self.core.notice = Some(Notice::Done(APPLIED.into()));
                }
            }
            Message::Discard => self.core.discard(),
            Message::Undo => {
                if self.can_undo() {
                    self.core.undo();
                }
            }
            Message::Refresh => self.core.refresh(),
            Message::UseCurrent => self.core.use_current(&[]),
            Message::KeepMine => self.core.keep_edits(),
            Message::Recover => self.core.recover(true),
            Message::KeepRecovery => self.core.recover(false),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::desktop::desktop_icons::{NAMESPACE, key};
    use crate::desktop::store::tests::Fixture;

    #[test]
    fn icons_page_applies_one_owned_change_and_reports_deferred_refresh_state() {
        let fixture = Fixture::new("icons-page");
        fixture.user(
            NAMESPACE,
            "desktop",
            "(grid_spacing: 100, icon_size: 100, show_content: true, show_mounted_drives: false, show_trash: false)",
        );
        let mut editor = DesktopIconsEditor::with_store(fixture.store(), None);
        assert!(editor.available(), "a fixture has the page");
        editor.update(Message::Size(150));
        editor.update(Message::Spacing(75));
        editor.update(Message::Show(Show::Trash, true));
        editor.update(Message::Size(137)); // not a slider step: ignored
        assert!(editor.dirty() && editor.can_apply());
        let before = fixture.read(NAMESPACE, "desktop");
        editor.update(Message::Apply);
        let latest = editor.core.latest().cloned().expect("owned change");
        assert_eq!(latest.owner, Owner::DesktopIcons);
        let keys: Vec<_> = latest
            .change
            .entries()
            .iter()
            .map(|entry| entry.key.clone())
            .collect();
        assert_eq!(keys, vec![key()]);
        let written = fixture.read(NAMESPACE, "desktop").unwrap();
        for member in [
            "icon_size: 150",
            "grid_spacing: 75",
            "show_trash: true",
            "show_content: true",
        ] {
            assert!(written.contains(member), "{written}");
        }
        assert_eq!(editor.notice(), Some(&Notice::Done(APPLIED.into())));
        assert!(editor.can_undo());
        editor.update(Message::Undo);
        assert_eq!(fixture.read(NAMESPACE, "desktop"), before);
    }

    #[test]
    fn icons_page_unavailable_without_cosmic_files() {
        let fixture = Fixture::new("icons-unavailable");
        let bin = fixture.0.join("bin");
        std::fs::create_dir_all(&bin).unwrap();
        assert!(!on_path(Some(bin.as_os_str())));
        assert!(!on_path(None));
        // Not a fixture store: the page needs COSMIC Files on PATH.
        let store = || {
            DesktopStore::with_roots(
                fixture.0.join("config/cosmic"),
                vec![],
                fixture.0.join("state"),
                crate::desktop::store::Access::Live,
            )
        };
        let mut editor = DesktopIconsEditor::with_store(store(), Some(bin.as_os_str()));
        assert!(!editor.available());
        let before = fixture.tree();
        editor.update(Message::Size(200));
        editor.update(Message::Apply);
        assert!(!editor.dirty() && !editor.can_apply());
        assert_eq!(fixture.tree(), before);
        let applet = bin.join("cosmic-files-applet");
        std::fs::write(&applet, "#!/bin/sh\n").unwrap();
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&applet, std::fs::Permissions::from_mode(0o755)).unwrap();
        let editor = DesktopIconsEditor::with_store(store(), Some(bin.as_os_str()));
        assert!(editor.available());
    }

    /// Unsupported stored settings stay read-only: no slider or toggle
    /// edit reaches the draft.
    #[test]
    fn unsupported_icon_settings_ignore_edits() {
        let fixture = Fixture::new("icons-page-unsupported");
        fixture.user(
            NAMESPACE,
            "desktop",
            "(grid_spacing: 100, icon_size: 100, show_content: true, show_mounted_drives: false, show_trash: false, future: 1)",
        );
        let mut editor = DesktopIconsEditor::with_store(fixture.store(), None);
        assert!(matches!(
            editor.draft().unwrap().config(),
            crate::desktop::panels::Setting::Unsupported(_)
        ));
        for message in [
            Message::Size(150),
            Message::Spacing(150),
            Message::Show(Show::Trash, true),
        ] {
            editor.update(message.clone());
            assert!(!editor.dirty() && !editor.can_apply(), "{message:?}");
        }
    }
}
