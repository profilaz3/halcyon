//! Floating apps editor state: one draft of the custom tiling-exception
//! list, a search, and at most one add/edit form. The form edits only
//! itself; Save puts the rule into the draft and Apply writes the draft.
use crate::desktop::apps::AppCatalog;
use crate::desktop::store::{DesktopStore, Journal, Owner};
use crate::desktop::window_rules::{
    CustomRule, Match, RuleRow, WindowRulesDraft, classify, exact_app, title_contains, validate,
};
use crate::settings::Page;
use crate::settings::desktop::Notice;
use crate::settings::drafts::DraftEditor;
use std::path::PathBuf;

/// The open add/edit form. `editing` is the pair being changed.
#[derive(Clone, Debug, Default, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct RuleForm {
    pub editing: Option<(String, String)>,
    pub app_id: String,
    pub title: String,
    /// Raw patterns instead of an exact app ID and a title literal.
    pub advanced: bool,
    pub app_pattern: String,
    pub title_pattern: String,
    #[serde(skip)]
    pub picker: bool,
    #[serde(skip)]
    pub picker_search: String,
    #[serde(skip)]
    pub error: Option<String>,
}

impl RuleForm {
    /// Opens a custom rule: exact literals as text, anything else as the
    /// patterns it is, never rewritten.
    fn open(row: &RuleRow) -> Self {
        let app = classify(&row.appid);
        let title = classify(&row.title);
        // Only when saving the literal reproduces the stored text exactly.
        let literal = match (&app, &title) {
            (Match::Exact(app), _) if exact_app(app) != row.appid => None,
            (Match::Exact(app), Match::Any) => Some((app.clone(), String::new())),
            (Match::Exact(app), Match::Contains(title)) if row.title == title_contains(title) => {
                Some((app.clone(), title.clone()))
            }
            _ => None,
        };
        let editing = Some((row.appid.clone(), row.title.clone()));
        match literal {
            Some((app_id, title)) if row.title != ".*" => Self {
                editing,
                app_id,
                title,
                app_pattern: row.appid.clone(),
                title_pattern: row.title.clone(),
                ..Self::default()
            },
            _ => Self {
                editing,
                advanced: true,
                app_pattern: row.appid.clone(),
                title_pattern: row.title.clone(),
                ..Self::default()
            },
        }
    }
    /// The rule the form describes, or a short reason it can't be saved.
    pub fn rule(&self, enabled: bool) -> Result<CustomRule, String> {
        let (appid, title) = if self.advanced {
            validate(&self.app_pattern).map_err(|e| format!("App ID: {e}"))?;
            validate(&self.title_pattern).map_err(|e| format!("Title: {e}"))?;
            (self.app_pattern.clone(), self.title_pattern.clone())
        } else {
            let id = self.app_id.trim();
            if id.is_empty() {
                return Err("Type an app ID or choose an app.".into());
            }
            (exact_app(id), title_contains(&self.title))
        };
        Ok(CustomRule {
            appid,
            title,
            enabled,
        })
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum Message {
    Search(String),
    Add,
    Edit(String, String),
    AppId(String),
    Title(String),
    Advanced(bool),
    AppPattern(String),
    TitlePattern(String),
    Picker(bool),
    PickerSearch(String),
    Pick(String),
    Save,
    Cancel,
    Toggle(String, String, bool),
    Remove(String, String),
    Expand(Option<(String, String)>),
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

#[derive(serde::Serialize, serde::Deserialize)]
pub(super) struct SavedRules {
    pub(super) draft: WindowRulesDraft,
    pub(super) form: Option<RuleForm>,
}

pub struct WindowRulesEditor {
    pub(super) core: DraftEditor<WindowRulesDraft>,
    pub search: String,
    pub form: Option<RuleForm>,
    pub expanded: Option<(String, String)>,
    apps: Option<AppCatalog>,
}

impl Default for WindowRulesEditor {
    fn default() -> Self {
        Self {
            core: DraftEditor::new(Owner::WindowRules, Page::DesktopWindowRules.title()),
            search: String::new(),
            form: None,
            expanded: None,
            apps: None,
        }
    }
}

impl WindowRulesEditor {
    #[cfg(test)]
    pub fn with_store(store: DesktopStore) -> Self {
        Self {
            core: DraftEditor::with_store(
                Owner::WindowRules,
                Page::DesktopWindowRules.title(),
                store,
            ),
            ..Self::default()
        }
    }
    pub fn open(&mut self) {
        self.core.open();
        if self.apps.is_none() {
            // A fixture replaces discovery, like the applet catalog.
            let fixture = self
                .core
                .store()
                .and_then(DesktopStore::fixture_root)
                .map(|root| root.join("applications"));
            self.apps = Some(match fixture {
                Some(dir) => AppCatalog::discover(&[dir]),
                None => AppCatalog::installed(),
            });
        }
    }
    pub(super) fn session_draft(&self) -> Option<SavedRules> {
        (self.dirty() || self.form.is_some())
            .then(|| {
                self.draft().cloned().map(|draft| SavedRules {
                    draft,
                    form: self.form.clone(),
                })
            })
            .flatten()
    }
    pub(super) fn restore_session(&mut self, saved: SavedRules) -> Result<(), String> {
        if saved.form.as_ref().is_some_and(|form| {
            [
                &form.app_id,
                &form.title,
                &form.app_pattern,
                &form.title_pattern,
            ]
            .iter()
            .any(|text| text.len() > 64 * 1024)
        }) {
            return Err("The saved floating-app form is too large".into());
        }
        self.core.keep_session_form_baseline(saved.form.is_some());
        self.core.restore_session(saved.draft)?;
        self.form = saved.form;
        Ok(())
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
    pub fn draft(&self) -> Option<&WindowRulesDraft> {
        self.core.draft()
    }
    pub fn apps(&self) -> Option<&AppCatalog> {
        self.apps.as_ref()
    }
    pub fn notice(&self) -> Option<&Notice> {
        self.core.notice.as_ref()
    }
    pub fn dirty(&self) -> bool {
        self.core.dirty()
    }
    /// A form is open with text that isn't in the draft yet.
    pub fn form_open(&self) -> bool {
        self.form.is_some()
    }
    pub fn recovery(&self) -> Option<&Result<Journal, String>> {
        self.core.recovery()
    }
    #[cfg_attr(not(test), allow(dead_code))]
    pub fn latest(&self) -> Option<&crate::desktop::LatestChange> {
        self.core.latest()
    }
    #[cfg(test)]
    pub fn elsewhere(&self) -> Option<&(Owner, String)> {
        self.core.elsewhere()
    }
    pub fn can_undo(&self) -> bool {
        self.core.can_undo() && !self.form_open()
    }
    /// An open form must be saved or cancelled first, so Apply never
    /// writes a half-typed rule or skips one.
    pub fn can_apply(&self) -> bool {
        self.core.can_apply() && !self.form_open()
    }
    pub fn watch_paths(&self) -> Vec<PathBuf> {
        self.core.watch_paths()
    }
    pub fn state_key(&self) -> String {
        format!(
            "{}:{:?}:{:?}:{}:{:?}",
            self.core.state_key(),
            self.draft().map(|draft| draft.custom().ok()),
            self.form,
            self.search,
            self.expanded,
        )
    }
    pub fn refresh(&mut self) {
        self.core.refresh();
    }
    fn form_mut(&mut self, change: impl FnOnce(&mut RuleForm)) {
        if let Some(form) = &mut self.form {
            change(form);
            form.error = None;
        }
    }

    pub fn update(&mut self, message: Message) {
        match message {
            Message::Search(text) => self.search = text,
            Message::Add => self.form = Some(RuleForm::default()),
            Message::Edit(appid, title) => {
                let row = self.draft().and_then(|draft| {
                    draft
                        .rows()
                        .into_iter()
                        .find(|row| row.appid == appid && row.title == title && !row.default)
                });
                if let Some(row) = row {
                    self.form = Some(RuleForm::open(&row));
                }
            }
            Message::AppId(text) => self.form_mut(|form| form.app_id = text),
            Message::Title(text) => self.form_mut(|form| form.title = text),
            Message::Advanced(on) => self.form_mut(|form| {
                // Switching to patterns starts from the literal's exact pattern.
                if on && !form.advanced && !form.app_id.trim().is_empty() {
                    form.app_pattern = exact_app(form.app_id.trim());
                    form.title_pattern = title_contains(&form.title);
                }
                form.advanced = on;
            }),
            Message::AppPattern(text) => self.form_mut(|form| form.app_pattern = text),
            Message::TitlePattern(text) => self.form_mut(|form| form.title_pattern = text),
            Message::Picker(open) => self.form_mut(|form| form.picker = open),
            Message::PickerSearch(text) => self.form_mut(|form| form.picker_search = text),
            Message::Pick(id) => {
                let app_id = self
                    .apps
                    .as_ref()
                    .and_then(|apps| apps.get(&id))
                    .map(|app| app.app_id.clone());
                if let Some(app_id) = app_id {
                    self.form_mut(|form| {
                        form.app_id = app_id;
                        form.advanced = false;
                        form.picker = false;
                    });
                }
            }
            Message::Save => self.save(),
            Message::Cancel => self.form = None,
            Message::Toggle(appid, title, on) => {
                let result = self
                    .core
                    .draft()
                    .cloned()
                    .map(|mut draft| draft.set_enabled(&appid, &title, on).map(|()| draft));
                self.finish(result);
            }
            Message::Remove(appid, title) => {
                let result = self
                    .core
                    .draft()
                    .cloned()
                    .map(|mut draft| draft.remove(&appid, &title).map(|()| draft));
                self.finish(result);
            }
            Message::Expand(pair) => self.expanded = pair,
            Message::Apply => {
                if self.can_apply() {
                    self.core.apply();
                }
            }
            Message::Discard => {
                self.form = None;
                self.core.discard();
            }
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
        if self.form.is_none() {
            self.core.keep_session_form_baseline(false);
        }
    }
    /// Replaces the draft with an edited copy, or explains a refusal.
    fn finish(&mut self, result: Option<Result<WindowRulesDraft, String>>) {
        match result {
            Some(Ok(next)) => self.core.edit(|draft| *draft = next),
            Some(Err(reason)) => self.core.notice = Some(Notice::Error(reason)),
            None => {}
        }
    }
    fn save(&mut self) {
        if self.core.recovery().is_some() {
            self.core.notice=Some(Notice::Error("Resolve the interrupted transaction before saving this rule. The form is retained.".into()));
            return;
        }
        let Some(form) = self.form.clone() else {
            return;
        };
        let editing = form.editing.clone();
        let enabled = editing
            .as_ref()
            .and_then(|(appid, title)| {
                self.draft()?
                    .rows()
                    .into_iter()
                    .find(|row| &row.appid == appid && &row.title == title)
                    .map(|row| row.enabled)
            })
            .unwrap_or(true);
        let result = form.rule(enabled).and_then(|rule| {
            let mut draft = self
                .draft()
                .cloned()
                .ok_or_else(|| "Floating-app rules aren’t available.".to_string())?;
            draft
                .save(
                    editing.as_ref().map(|(a, t)| (a.as_str(), t.as_str())),
                    rule,
                )
                .map(|()| draft)
        });
        match result {
            Ok(next) => {
                self.core.edit(|draft| *draft = next);
                self.form = None;
            }
            Err(error) => {
                if let Some(form) = &mut self.form {
                    form.error = Some(error);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::desktop::store::tests::Fixture;
    use crate::desktop::window_rules::NAMESPACE;

    /// The form survives until saved or cancelled, holds Apply back while
    /// open, keeps patterns as patterns, and a system-default change found
    /// at Apply writes nothing and keeps the earlier Undo.
    #[test]
    fn floating_form_waits_for_save_and_apply_rechecks_defaults() {
        let fixture = Fixture::new("rules-editor");
        fixture.system(
            NAMESPACE,
            "tiling_exception_defaults",
            r#"[(appid: "Gjs", titles: ["Settings"])]"#,
        );
        let mut editor = WindowRulesEditor::with_store(fixture.store());
        editor.update(Message::Add);
        editor.update(Message::AppId("org.example.Notes".into()));
        editor.update(Message::Title("Quick note".into()));
        assert!(
            !editor.dirty() && !editor.can_apply(),
            "typing isn't an edit"
        );
        editor.update(Message::Save);
        assert!(editor.form.is_none() && editor.dirty() && editor.can_apply());
        editor.update(Message::Apply);
        assert!(editor.latest().is_some(), "{:?}", editor.notice());
        let earlier = editor.latest().cloned();
        assert!(
            fixture
                .read(NAMESPACE, "tiling_exception_custom")
                .unwrap()
                .contains("^org\\\\.example\\\\.Notes$")
        );

        // Editing a pattern rule opens it as the pattern it is.
        fixture.user(
            NAMESPACE,
            "tiling_exception_custom",
            r#"[(appid: "^org\\.example\\.Notes$", title: "Quick note", enabled: true), (appid: "a|b", title: "", enabled: true)]"#,
        );
        editor.update(Message::Refresh);
        editor.update(Message::Edit("a|b".into(), String::new()));
        let form = editor.form.clone().unwrap();
        assert!(form.advanced && form.app_pattern == "a|b");
        editor.update(Message::AppPattern("(".into()));
        editor.update(Message::Save);
        assert!(
            editor.form.as_ref().unwrap().error.is_some(),
            "invalid stays in the form"
        );
        editor.update(Message::AppPattern("a|c".into()));
        editor.update(Message::Save);
        assert!(editor.dirty());
        // A form opened afterwards holds Apply back.
        editor.update(Message::Add);
        assert!(!editor.can_apply());
        editor.update(Message::Cancel);
        assert!(editor.can_apply());

        // The system defaults change underneath: nothing is written.
        fixture.system(
            NAMESPACE,
            "tiling_exception_defaults",
            r#"[(appid: "Gjs", titles: ["Settings", "Preferences"])]"#,
        );
        let files = fixture.tree();
        editor.update(Message::Apply);
        assert_eq!(fixture.tree(), files, "nothing written");
        assert_eq!(editor.latest().cloned(), earlier, "earlier Undo kept");
        assert!(
            matches!(editor.notice(), Some(Notice::Error(text)) if text.contains("Default floating-app rules changed outside this editor. Nothing was applied.")),
            "{:?}",
            editor.notice()
        );
        assert!(!editor.dirty());
        assert_eq!(editor.draft().unwrap().rows().len(), 4);
    }
}
