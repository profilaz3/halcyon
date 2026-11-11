use super::*;
use crate::desktop::profiles::mix::{self, Component, Source, SourceKind};

pub const PAGE_SIZE: usize = 12;
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Draft {
    desktop: Profile,
    pub sources: [Option<Source>; 3],
    pub picking: Component,
    pub choosing: bool,
    pub query: String,
    pub page: usize,
}
impl Draft {
    pub(super) fn validate(&self) -> Result<(), String> {
        self.desktop.validate().map_err(|e| e.to_string())?;
        for source in self.sources.iter().flatten() {
            source.profile.validate().map_err(|e| e.to_string())?;
        }
        Ok(())
    }
    pub fn source_name(&self, part: Component) -> Option<&str> {
        self.sources[part.index()]
            .as_ref()
            .map(|s| s.origin.name.as_str())
    }
}
impl ProfilesEditor {
    pub(super) fn start_remix(&mut self) {
        let Some(profile) = self.profile().cloned() else {
            return;
        };
        let (kind, id) = match &self.selection {
            Selection::Preset(id) => (SourceKind::Builtin, Some((*id).into())),
            Selection::Saved(id) => (SourceKind::Library, Some(id.as_str().into())),
            Selection::Current => (SourceKind::Desktop, None),
        };
        let frozen = Component::ALL.map(|part| Source::freeze(&profile, part, kind, id.clone()));
        let [Ok(picture), Ok(style), Ok(layout)] = frozen else {
            self.notice = Some(Notice::Error(
                "This look could not be prepared for remixing".into(),
            ));
            return;
        };
        let sources = [picture, style, layout];
        let composed = match mix::compose(
            &format!(
                "{} remix",
                profile.name.chars().take(74).collect::<String>()
            ),
            &sources,
        ) {
            Ok(profile) => profile,
            Err(error) => {
                self.notice = Some(Notice::Error(error));
                return;
            }
        };
        self.start_mix();
        let Some(draft) = &mut self.mix else {
            return;
        };
        draft.sources = sources.map(Some);
        draft.choosing = false;
        self.theme_override = Some(composed);
        self.replan();
        self.queue_theme_colors();
        self.want_backdrop();
    }
    pub(super) fn start_mix(&mut self) {
        let Some(store) = self.store() else {
            return;
        };
        let captured = match profiles::capture(store, "Your desktop") {
            Ok(captured) => captured,
            Err(error) => {
                self.notice = Some(Notice::failed(&error));
                return;
            }
        };
        self.select(Selection::Current);
        self.notes = captured.warnings;
        self.mix = Some(Draft {
            desktop: captured.profile,
            sources: [None, None, None],
            picking: Component::Picture,
            choosing: true,
            query: String::new(),
            page: 0,
        });
    }
    pub fn mix_choices(&self) -> Vec<(Selection, &str)> {
        let Some(draft) = &self.mix else {
            return Vec::new();
        };
        let mut sources = vec![(Selection::Current, &draft.desktop)];
        sources.extend(
            self.presets
                .iter()
                .map(|p| (Selection::Preset(p.id), &p.profile)),
        );
        if let Ok(entries) = self.library() {
            sources.extend(entries.iter().filter_map(|entry| {
                entry
                    .profile
                    .as_ref()
                    .ok()
                    .map(|p| (Selection::Saved(entry.id.clone()), p))
            }));
        }
        let query = draft.query.to_lowercase();
        sources
            .into_iter()
            .filter(|(_, p)| {
                let available = match draft.picking {
                    Component::Picture => {
                        p.wallpaper.as_ref().is_some_and(|w| w.background.is_some())
                    }
                    Component::Style => p.appearance.is_some() || p.widget_style.is_some(),
                    Component::Layout => {
                        p.widget_layout.is_some() || p.dock.is_some() || p.panel.is_some()
                    }
                };
                available && p.name.to_lowercase().contains(&query)
            })
            .map(|(selection, p)| (selection, p.name.as_str()))
            .collect()
    }
    pub(super) fn pick_mix(&mut self, part: Component, selection: Selection) {
        let Some(draft) = &self.mix else {
            return;
        };
        let (profile, kind, id) = match &selection {
            Selection::Current => (Some(&draft.desktop), SourceKind::Desktop, None),
            Selection::Preset(id) => (
                self.presets
                    .iter()
                    .find(|p| p.id == *id)
                    .map(|p| &p.profile),
                SourceKind::Builtin,
                Some((*id).to_string()),
            ),
            Selection::Saved(id) => (
                self.library()
                    .ok()
                    .and_then(|entries| entries.iter().find(|e| e.id == *id))
                    .and_then(|e| e.profile.as_ref().ok()),
                SourceKind::Library,
                Some(id.as_str().into()),
            ),
        };
        let result = profile
            .ok_or("This source is unavailable".to_string())
            .and_then(|p| Source::freeze(p, part, kind, id));
        let source = match result {
            Ok(source) if !source.profile.domains().is_empty() => source,
            Ok(_) => {
                self.notice = Some(Notice::Error(
                    "This source has no choices for this part".into(),
                ));
                return;
            }
            Err(error) => {
                self.notice = Some(Notice::Error(error));
                return;
            }
        };
        let draft = self.mix.as_mut().unwrap();
        draft.sources[part.index()] = Some(source);
        draft.query.clear();
        draft.page = 0;
        if let Some(next) = Component::ALL
            .into_iter()
            .find(|part| draft.sources[part.index()].is_none())
        {
            draft.picking = next;
            draft.choosing = true;
        } else {
            draft.choosing = false;
        }
        self.name = None;
        self.notice = None;
        self.excluded.clear();
        self.theme_override = None;
        self.theme_pending = None;
        self.theme_source = None;
        self.theme_error = None;
        if let [Some(picture), Some(style), Some(layout)] = &draft.sources {
            match mix::compose("My mix", &[picture.clone(), style.clone(), layout.clone()]) {
                Ok(profile) => self.theme_override = Some(profile),
                Err(error) => self.notice = Some(Notice::Error(error)),
            }
        }
        self.replan();
        self.queue_theme_colors();
        self.want_backdrop();
    }
}
