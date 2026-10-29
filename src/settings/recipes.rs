use super::*;
use crate::recipes::{self as model, Arrangement, Kind, Part, Recipe};
#[derive(Clone, Debug)]
pub enum Message {
    New(Kind),
    Name(String),
    Caption(String),
    Add(Part),
    Remove(usize),
    Move(usize, bool),
    Arrangement(bool, Arrangement),
    Wide(bool),
    Missing(bool),
    Load(String),
    Save,
    Duplicate,
    Delete,
    ConfirmDelete,
    Apply,
    UseBuiltin,
    Reload,
}
#[derive(Clone, Debug)]
pub struct Editor {
    pub draft: Recipe,
    pub library: model::library::Library,
    pub library_labels: Vec<String>,
    pub library_ids: Vec<String>,
    pub expected: Option<Recipe>,
    pub wide: bool,
    pub missing: bool,
    pub delete_review: bool,
    pub notice: Option<String>,
    pub error: bool,
}
impl Default for Editor {
    fn default() -> Self {
        Self {
            draft: Recipe::starter(Kind::Clock),
            library: Default::default(),
            library_labels: Vec::new(),
            library_ids: Vec::new(),
            expected: None,
            wide: false,
            missing: false,
            delete_review: false,
            notice: None,
            error: false,
        }
    }
}
impl Editor {
    pub fn loaded() -> Self {
        let mut editor = Self::default();
        editor.load_library();
        editor
    }
    pub fn index_library(&mut self) {
        self.library_ids = self.library.entries.keys().cloned().collect();
        self.library_labels = self
            .library
            .entries
            .values()
            .map(|r| format!("{} · {}", r.name, r.kind.title()))
            .collect();
    }
    pub fn load_library(&mut self) {
        match model::library::path().and_then(|p| model::library::load(&p)) {
            Ok(l) => {
                self.library = l;
                self.index_library();
                self.error = false;
            }
            Err(e) => {
                self.notice = Some(e);
                self.error = true;
            }
        }
    }
    pub fn update(&mut self, edit: Message) {
        self.delete_review = matches!(edit, Message::Delete);
        match edit {
            Message::New(kind) => {
                self.draft = Recipe::starter(kind);
                self.expected = None;
                self.notice = None;
            }
            Message::Name(name) => self.draft.name = name.chars().take(80).collect(),
            Message::Caption(caption) => self.draft.caption = caption.chars().take(120).collect(),
            Message::Add(part) => {
                if self.draft.parts.len() < 8
                    && self.draft.kind.parts().contains(&part)
                    && !self.draft.parts.contains(&part)
                {
                    self.draft.parts.push(part)
                }
            }
            Message::Remove(i) => {
                if self.draft.can_remove(i) {
                    self.draft.parts.remove(i);
                }
            }
            Message::Move(i, up) => {
                let to = if up {
                    i.checked_sub(1)
                } else {
                    i.checked_add(1)
                };
                if let Some(to) = to.filter(|to| *to < self.draft.parts.len())
                    && i < self.draft.parts.len()
                {
                    self.draft.parts.swap(i, to);
                }
            }
            Message::Arrangement(wide, value) => {
                if wide {
                    self.draft.wide = value
                } else {
                    self.draft.compact = value
                }
            }
            Message::Wide(wide) => self.wide = wide,
            Message::Missing(missing) => self.missing = missing,
            Message::Load(id) => {
                if let Some(r) = self.library.entries.get(&id) {
                    self.draft = r.clone();
                    self.expected = Some(r.clone());
                    self.notice = None;
                }
            }
            Message::Duplicate => match model::new_id() {
                Ok(id) => {
                    self.draft.id = id;
                    self.draft.seal();
                    self.expected = None;
                    self.notice = Some("New copy · save it to keep it in your library".into());
                }
                Err(e) => {
                    self.notice = Some(e);
                    self.error = true;
                }
            },
            _ => {}
        }
        self.draft.seal();
    }
    /// All blocking library/store work is admitted through the existing native queue.
    pub fn work(
        &mut self,
        edit: Message,
        baseline: Option<serde_json::Value>,
    ) -> Result<Option<Config>, String> {
        match edit {
            Message::Save => {
                let path = model::library::path()?;
                self.draft = model::library::save(&path, &self.draft, self.expected.as_ref())?;
                self.expected = Some(self.draft.clone());
                self.library = model::library::load(&path)?;
                self.index_library();
                self.notice = Some("Widget saved · the desktop is unchanged".into());
            }
            Message::ConfirmDelete => {
                if !self.delete_review {
                    return Err("Review the saved widget before deleting it".into());
                }
                let expected = self
                    .expected
                    .as_ref()
                    .ok_or("Choose a saved widget first")?;
                let path = model::library::path()?;
                model::library::delete(&path, expected)?;
                self.library = model::library::load(&path)?;
                self.index_library();
                self.expected = None;
                self.delete_review = false;
                self.draft.id = model::new_id()?;
                self.draft.seal();
                self.notice =
                    Some("Library copy deleted · applied and saved setups are kept".into());
            }
            Message::Reload => {
                self.load_library();
                if self.error {
                    return Err(self.notice.clone().unwrap_or_default());
                }
            }
            Message::Apply | Message::UseBuiltin => {
                let path = Config::path().ok_or("No settings directory")?;
                if !crate::design::storage::managed(&path) {
                    return Err("Migrate widgets in App settings → Recovery before applying a personal widget".into());
                }
                let value = if matches!(edit, Message::Apply) {
                    self.draft.validate()?;
                    serde_json::to_value(&self.draft).map_err(|e| e.to_string())?
                } else {
                    serde_json::Value::Null
                };
                let store =
                    crate::desktop::actions::preference_store(&path).map_err(|e| e.to_string())?;
                let raw = crate::config::parse_config_text(
                    crate::config::read_config_text(&path)?.as_deref(),
                )?;
                let original = crate::desktop::store::widget::get(&raw, self.draft.kind.pointer());
                if original.clone().filter(|v| !v.is_null()) != baseline {
                    return Err(
                        "The applied widget changed. Check the current widget before applying."
                            .into(),
                    );
                }
                let mut patch = crate::desktop::store::WidgetPatch::new();
                patch.set(self.draft.kind.pointer(), original, Some(value))?;
                crate::desktop::actions::commit(
                    &store,
                    &crate::desktop::DesktopPatch::new(),
                    Some(&patch),
                    crate::desktop::Owner::Preferences,
                    "Widget recipe",
                    crate::desktop::store::Source::Manual,
                )
                .map_err(|e| e.to_string())?;
                self.notice = Some("Widget applied · Undo is available".into());
                return Config::try_load().map(Some);
            }
            _ => {}
        }
        Ok(None)
    }
}
impl Settings {
    pub(super) fn update_recipe(&mut self, edit: Message) {
        self.recipe_editor.update(edit);
    }
}
#[cfg(test)]
mod tests;

/// The existing isolated Settings capture protocol admits only these typed edits.
pub(super) fn capture(value: &serde_json::Value) -> Option<Message> {
    let action = value.get("action")?.as_str()?;
    Some(match action {
        "new" => Message::New(serde_json::from_value(value.get("kind")?.clone()).ok()?),
        "name" => Message::Name(value.get("value")?.as_str()?.into()),
        "caption" => Message::Caption(value.get("value")?.as_str()?.into()),
        "add" => Message::Add(serde_json::from_value(value.get("part")?.clone()).ok()?),
        "wide" => Message::Wide(value.get("value")?.as_bool()?),
        "missing" => Message::Missing(value.get("value")?.as_bool()?),
        "save" => Message::Save,
        "apply" => Message::Apply,
        "builtin" => Message::UseBuiltin,
        "delete" => Message::Delete,
        "confirm_delete" => Message::ConfirmDelete,
        _ => return None,
    })
}
