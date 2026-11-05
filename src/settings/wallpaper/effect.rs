//! Effect edits join the wallpaper transaction; browsing never changes the desktop.
use super::*;

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub(super) struct EffectDraft {
    baseline: Option<serde_json::Value>,
    initial: crate::rain::Preferences,
    value: crate::rain::Preferences,
}
impl EffectDraft {
    pub(super) fn validate(&self) -> Result<(), String> {
        self.initial.validate()?;
        self.value.validate()?;
        let initial: crate::rain::Preferences = match &self.baseline {
            Some(raw) => serde_json::from_value(raw.clone()).map_err(|e| e.to_string())?,
            None => Default::default(),
        };
        if initial != self.initial {
            return Err("The saved effect baseline is invalid".into());
        }
        Ok(())
    }
}
/// A long enough local history for trying pictures, fits and effects.
const MAX_STEPS: usize = 32;

impl WallpaperEditor {
    /// The preview as text, or `None` when nothing is staged.
    pub fn snapshot(&self) -> Option<String> {
        self.session_draft()
            .and_then(|saved| ron::to_string(&saved).ok())
    }
    /// Keeps `before` only if the edit actually changed the preview.
    pub fn record_step(&mut self, before: Option<String>) {
        if before != self.snapshot() {
            self.preview_open = true;
            self.steps.push(before);
            if self.steps.len() > MAX_STEPS {
                self.steps.remove(0);
            }
        }
        if !self.dirty() {
            self.steps.clear();
        }
    }
    /// A slider gesture is one step, however many values it passed through.
    pub fn begin_gesture(&mut self) {
        if self.gesture.is_none() {
            self.gesture = Some(self.snapshot());
        }
    }
    pub fn finish_gesture(&mut self) {
        if let Some(before) = self.gesture.take() {
            self.record_step(before);
        }
    }
    pub fn can_undo_step(&self) -> bool {
        !self.steps.is_empty()
    }
    pub fn preview_open(&self) -> bool {
        self.preview_open || self.dirty()
    }
    /// Undoes one preview edit. It never reaches desktop history; an empty
    /// history leaves the preview exactly as it is.
    pub fn undo_step(&mut self) -> Result<bool, String> {
        let Some(before) = self.steps.pop() else {
            return Ok(false);
        };
        let steps = std::mem::take(&mut self.steps);
        match before {
            None => {
                let _ = self.update(Message::Discard);
            }
            Some(text) => {
                let saved = ron::from_str(&text).map_err(|error| error.to_string())?;
                self.restore_session(saved)?;
                self.core.notice = None;
            }
        }
        self.steps = steps;
        self.preview_open = true;
        Ok(true)
    }
    pub fn effect(&self) -> Option<&crate::rain::Preferences> {
        self.effect.as_ref().map(|d| &d.value)
    }
    pub fn effect_conflict(&self) -> bool {
        self.effect_conflict
    }
    pub fn resolve_effect(&mut self, keep_preview: bool) {
        if !keep_preview {
            self.effect = None;
            self.effect_conflict = false;
            return;
        }
        let result = self
            .store()
            .ok_or("Wallpaper settings are unavailable".into())
            .and_then(automatic::read_config);
        match result {
            Ok((raw, config)) => {
                if let Some(draft) = &mut self.effect {
                    draft.baseline = widget::get(&raw, "/rain");
                    draft.initial = config.rain;
                }
                self.effect_conflict = false;
                self.refresh_effect();
            }
            Err(error) => self.set_notice(Notice::Error(error)),
        }
    }
    pub(super) fn refresh_effect(&mut self) {
        if self.effect.is_none() {
            self.effect_conflict = false;
            return;
        }
        let result = self
            .store()
            .ok_or("Wallpaper settings are unavailable".into())
            .and_then(automatic::read_config);
        match result {
            Ok((raw, config)) => {
                let draft = self.effect.as_mut().expect("checked");
                self.effect_conflict = config.rain != draft.initial && config.rain != draft.value;
                if config.rain == draft.value {
                    self.effect = None;
                } else if !self.effect_conflict {
                    draft.baseline = widget::get(&raw, "/rain");
                }
            }
            Err(error) => {
                self.effect_conflict = true;
                self.set_notice(Notice::Error(error));
            }
        }
    }
    pub fn stage_effect(&mut self, value: crate::rain::Preferences) -> Result<(), String> {
        value.validate()?;
        if self.effect.is_none() {
            self.open();
            let store = self.store().ok_or("Wallpaper settings are unavailable")?;
            let (raw, config) = automatic::read_config(store)?;
            self.effect = Some(EffectDraft {
                baseline: widget::get(&raw, "/rain"),
                initial: config.rain,
                value: value.clone(),
            });
        }
        let draft = self.effect.as_mut().expect("created above");
        draft.value = value;
        if draft.initial == draft.value {
            self.effect = None;
        }
        self.core.notice = None;
        self.refresh_effect();
        Ok(())
    }
    fn effect_patch(&self) -> Result<Option<WidgetPatch>, String> {
        let Some(draft) = &self.effect else {
            return Ok(None);
        };
        draft.value.validate()?;
        let mut patch = WidgetPatch::new();
        patch.set(
            "/rain",
            draft.baseline.clone(),
            Some(serde_json::to_value(&draft.value).map_err(|e| e.to_string())?),
        )?;
        Ok(Some(patch))
    }
    pub(super) fn merge_effect(&self, patch: &mut Option<WidgetPatch>) -> Result<(), String> {
        if let Some(effect) = self.effect_patch()? {
            match patch {
                Some(patch) => patch
                    .merge(effect)
                    .map_err(|keys| format!("Overlapping effect changes: {}", keys.join(", ")))?,
                None => *patch = Some(effect),
            }
        }
        Ok(())
    }
}
