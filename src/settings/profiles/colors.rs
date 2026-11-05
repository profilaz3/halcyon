//! Colors uses the profile planner for every choice. Mode is a local option,
//! kept with the preview rather than added to the portable theme format.
use super::*;
use crate::desktop::appearance::{ColorRole, Control, SystemMode, ThemeVariant, Value};
use crate::desktop::profiles::{ColorChoice, VariantLook};
use crate::desktop::store::{ConfigKey, RawValue, WidgetPatch};
use serde::{Deserialize, Serialize};
use serde_json::json;

#[cfg(test)]
#[path = "colors_tests.rs"]
mod tests;

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Options {
    pub mode: Option<SystemMode>,
    pub style_bars: Option<bool>,
}

#[derive(Clone, Debug)]
pub enum Edit {
    Mode(SystemMode),
    StyleBars(bool),
    Palette(String),
    Outline(bool),
    Recommended,
    /// An obsolete live completion cannot apply this screen's preview.
    Noop,
}

impl ProfilesEditor {
    /// Editing colors without choosing a family starts from the actual look.
    /// Capturing is read-only and retains every supported theme field.
    pub(super) fn ensure_color_preview(&mut self) -> bool {
        if self
            .profile()
            .is_some_and(|profile| profile.appearance_only)
        {
            return true;
        }
        let Some(store) = self.store() else {
            return false;
        };
        match profiles::themes::capture(store, "Colors") {
            Ok(capture) => {
                self.select(Selection::Current);
                self.theme_override = Some(capture.profile);
                self.notes = capture.warnings;
                true
            }
            Err(error) => {
                self.notice = Some(Notice::Error(error.to_string()));
                false
            }
        }
    }

    pub fn color_mode(&self) -> Option<SystemMode> {
        self.preview
            .as_ref()
            .and_then(|preview| preview.appearance.as_ref())
            .and_then(|appearance| appearance.mode().known().copied())
            .or(self.color_options.mode)
    }

    pub fn color_style_bars(&self, config: &Config) -> bool {
        self.color_preferences(config).style_bars
    }

    /// The values the preview will actually apply, including Keep current.
    pub fn color_preferences<'a>(&'a self, config: &'a Config) -> &'a config::Appearance {
        &self
            .preview
            .as_ref()
            .and_then(|preview| preview.config.as_ref())
            .unwrap_or(config)
            .appearance
    }

    pub(super) fn plan_colors(
        &self,
        profile: &Profile,
        selected: &BTreeSet<Domain>,
    ) -> ProfilePlan {
        let store = self.store().expect("planning requires an open store");
        let mut selected = selected.clone();
        if profile.appearance_only {
            if let Some(style_bars) = self.color_options.style_bars {
                for domain in [Domain::Dock, Domain::Panel] {
                    if style_bars && profile.has(domain) && !self.excluded.contains(&domain) {
                        selected.insert(domain);
                    } else if !style_bars {
                        selected.remove(&domain);
                    }
                }
            }
            if profile
                .widget_style
                .as_ref()
                .and_then(|s| s.pointer("/appearance/sync_colors"))
                .and_then(serde_json::Value::as_bool)
                == Some(false)
            {
                selected.retain(|domain| matches!(domain, Domain::WidgetStyle | Domain::Wallpaper));
            }
        }
        let mut plan = profiles::plan(store, &self.installed, profile, &selected);
        if let Some(mode) = self.color_options.mode
            && !self.excluded.contains(&Domain::Appearance)
        {
            match AppearanceDraft::load(store) {
                Ok(mut draft) => {
                    draft.set(Control::Mode, Value::Mode(mode));
                    let mode_plan = draft.plan();
                    for blocked in &mode_plan.blocked {
                        plan.blocking.push(blocked.reason.clone());
                    }
                    if let Err(error) = plan.add_native_patch(mode_plan.patch, "Desktop mode") {
                        plan.blocking.push(error.to_string());
                    }
                    // Light and Dark explicitly choose both parts of Mode,
                    // even when auto_switch already matches. Automatic owns
                    // only auto_switch; the clock still chooses is_dark.
                    let desired = match mode {
                        SystemMode::Light => {
                            [("is_dark", Some(false)), ("auto_switch", Some(false))]
                        }
                        SystemMode::Dark => [("is_dark", Some(true)), ("auto_switch", Some(false))],
                        SystemMode::Automatic => [("is_dark", None), ("auto_switch", Some(true))],
                    };
                    for (name, desired) in desired {
                        let Some(desired) = desired else { continue };
                        let key = ConfigKey::new(crate::desktop::appearance::MODE, 1, name)
                            .expect("known theme mode key");
                        match (store.read_user(&key), RawValue::encode(&desired)) {
                            (Ok(original), Ok(desired)) => {
                                plan.explicit_native(key, original, Some(desired))
                            }
                            (Err(error), _) | (_, Err(error)) => {
                                plan.blocking.push(error.to_string())
                            }
                        }
                    }
                }
                Err(error) => plan.blocking.push(error.to_string()),
            }
        }
        if let Some(style_bars) = self.color_options.style_bars
            && !self.excluded.contains(&Domain::WidgetStyle)
        {
            match wallpaper::automatic::read_config(store) {
                Ok((raw, _)) => {
                    let mut patch = WidgetPatch::new();
                    let result = patch
                        .set(
                            "/appearance/style_bars",
                            widget::get(&raw, "/appearance/style_bars"),
                            Some(json!(style_bars)),
                        )
                        .and_then(|()| plan.add_widget_patch(patch));
                    if let Err(error) = result {
                        plan.blocking.push(error);
                    }
                }
                Err(error) => plan.blocking.push(error),
            }
        }
        plan
    }

    /// Family recoloring keeps that family's custom surfaces. Before the first
    /// family, use the same linked native accents as Wallpaper's color editor.
    pub(super) fn recolor_preview(&self, profile: &mut Profile, accent: Option<[u8; 3]>) {
        if profiles::themes::family(profile).is_some() {
            profiles::themes::recolor(profile, accent);
            return;
        }
        let Some(store) = self.store() else { return };
        let Ok((_, config)) = wallpaper::automatic::read_config(store) else {
            return;
        };
        let rgb = accent.unwrap_or_else(|| {
            crate::desktop_theme::bytes(config::Palette::named(&config.theme).accent)
        });
        let style = profile.widget_style.get_or_insert(json!({}));
        let _ = widget::put(style, "/appearance/accent", Some(json!(accent)));
        let _ = widget::put(style, "/appearance/color_source", Some(json!("custom")));
        if let Ok(mut draft) = AppearanceDraft::load(store) {
            profiles::themes::linked_accents(&mut draft, config.appearance, rgb);
            let appearance = profile.appearance.get_or_insert_with(Default::default);
            for variant in ThemeVariant::ALL {
                let target = match variant {
                    ThemeVariant::Light => &mut appearance.light,
                    ThemeVariant::Dark => &mut appearance.dark,
                }
                .get_or_insert_with(VariantLook::default);
                if let Some(color) = draft.color(variant, ColorRole::Accent).known() {
                    target.accent = Some(color.map_or(ColorChoice::Auto, |c| {
                        ColorChoice::Exact([c.red, c.green, c.blue, c.alpha])
                    }));
                }
            }
        }
    }

    pub(super) fn color_accent(&mut self, rgb: [u8; 3]) {
        if !self.ensure_color_preview() {
            return;
        }
        let mut profile = self.profile().unwrap().clone();
        let style = profile.widget_style.get_or_insert(json!({}));
        let _ = widget::put(style, "/appearance/wallpaper_colors", Some(json!(false)));
        let _ = widget::put(style, "/appearance/color_policy", Some(json!("custom")));
        self.recolor_preview(&mut profile, Some(rgb));
        self.theme_override = Some(profile);
        self.theme_error = None;
        self.replan();
    }

    pub(super) fn color_source(&mut self, automatic: bool) {
        if !self.ensure_color_preview() {
            return;
        }
        let mut profile = self.profile().unwrap().clone();
        let style = profile.widget_style.get_or_insert(json!({}));
        let _ = widget::put(
            style,
            "/appearance/wallpaper_colors",
            Some(json!(automatic)),
        );
        let _ = widget::put(
            style,
            "/appearance/color_policy",
            Some(json!(if automatic { "wallpaper" } else { "theme" })),
        );
        if !automatic {
            self.recolor_preview(&mut profile, None);
        }
        self.custom = None;
        self.theme_override = Some(profile);
        self.queue_theme_colors();
        self.replan();
    }

    pub(super) fn color_scope(&mut self, sync: bool) {
        if !self.ensure_color_preview() {
            return;
        }
        let mut profile = self.profile().unwrap().clone();
        let style = profile.widget_style.get_or_insert(json!({}));
        let _ = widget::put(style, "/appearance/sync_colors", Some(json!(sync)));
        self.theme_override = Some(profile);
        self.replan();
    }

    pub(super) fn edit_colors(&mut self, edit: Edit) -> Outcome {
        if matches!(edit, Edit::Noop) {
            return Outcome::None;
        }
        if !self.ensure_color_preview() {
            return Outcome::None;
        }
        match edit {
            Edit::Mode(mode) => self.color_options.mode = Some(mode),
            Edit::StyleBars(on) => self.color_options.style_bars = Some(on),
            Edit::Palette(key) => {
                if !config::Palette::CHOICES
                    .iter()
                    .any(|(known, _)| *known == key)
                {
                    return Outcome::None;
                }
                self.color_accent(crate::desktop_theme::bytes(
                    config::Palette::named(&key).accent,
                ));
                if let Some(profile) = &mut self.theme_override {
                    let style = profile.widget_style.get_or_insert(json!({}));
                    let _ = widget::put(style, "/theme", Some(json!(key)));
                }
            }
            Edit::Outline(linked) => {
                let current = self
                    .store()
                    .and_then(|s| profiles::themes::capture(s, "Current").ok());
                let mut profile = self.profile().unwrap().clone();
                let appearance = profile.appearance.get_or_insert_with(Default::default);
                for (variant, target) in [
                    (ThemeVariant::Light, &mut appearance.light),
                    (ThemeVariant::Dark, &mut appearance.dark),
                ] {
                    target.get_or_insert_with(VariantLook::default).outline = if linked {
                        Some(ColorChoice::Auto)
                    } else {
                        current
                            .as_ref()
                            .and_then(|c| c.profile.appearance.as_ref())
                            .and_then(|a| a.variant(variant))
                            .and_then(|v| v.outline.clone())
                    };
                }
                self.theme_override = Some(profile);
            }
            Edit::Recommended => {
                if let Some(rgb) = self.suggestion().and_then(|suggestion| {
                    wallpaper::automatic::recommended_for(
                        suggestion,
                        self.profile().and_then(profiles::themes::family),
                    )
                }) {
                    self.color_accent(rgb);
                }
            }
            Edit::Noop => {}
        }
        self.replan();
        Outcome::None
    }
}
