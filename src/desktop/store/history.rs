//! Conditional semantic reversal. Derived colors may advance a manual action's
//! expected result; they never give permission to overwrite an external writer.
use super::*;
use crate::desktop::appearance::{self, AppearanceDraft, ThemeVariant};
use crate::desktop::wallpaper::{WallpaperDraft, automatic};
use serde_json::Value;

/// A local, guarded preview of a partial reversal. Conflicting current values
/// are retained; accepting this review cannot overwrite them or a newer edit.
#[derive(Clone, Debug)]
pub struct UndoReview {
    pub label: String,
    pub conflicts: Vec<String>,
    id: String,
    patch: DesktopPatch,
    widgets: Option<WidgetPatch>,
    /// An older change put back: it leaves the history on its own and
    /// every newer change stays.
    retire: Option<LatestChange>,
}
impl UndoReview {
    /// The change this review would undo.
    pub fn id(&self) -> &str {
        &self.id
    }
    /// An older change being put back, not the latest one undone.
    pub fn puts_back(&self) -> bool {
        self.retire.is_some()
    }
}
impl DesktopStore {
    pub fn review_undo(&self, id: &str) -> Result<UndoReview, DesktopError> {
        let latest = self
            .latest_change()?
            .filter(|v| v.id == id)
            .ok_or_else(|| {
                DesktopError::UndoUnavailable("The last settings action changed".into())
            })?;
        let mut conflicts = Vec::new();
        let (patch, widgets) = self.reversal_review(&latest, Some(&mut conflicts))?;
        Ok(UndoReview {
            label: latest.label,
            id: latest.id,
            conflicts,
            patch,
            widgets,
            retire: None,
        })
    }
    /// The kept change with `id`, at any place in the history.
    fn kept_change(&self, id: &str) -> Result<LatestChange, DesktopError> {
        let file = self.history_file()?.and_then(|history| {
            history
                .history
                .iter()
                .chain(std::iter::once(&history))
                .find(|file| file.id == id && file.source == Source::Manual)
                .cloned()
        });
        let mut file = file.ok_or_else(|| {
            DesktopError::UndoUnavailable("This change is no longer kept, so it can’t be undone".into())
        })?;
        file.history.clear();
        file.recent.clear();
        LatestChange::from_file(file)
    }
    /// Puts back what one change replaced, at any place in the history. Refuses
    /// without writing when a setting it changed was changed again since.
    pub fn put_back(&self, id: &str) -> Result<AppliedChange, DesktopError> {
        let change = self.kept_change(id)?;
        self.undo_change(&change)
    }
    /// What putting back one change can still do: settings changed since stay
    /// as they are now and are listed.
    pub fn review_put_back(&self, id: &str) -> Result<UndoReview, DesktopError> {
        let change = self.kept_change(id)?;
        let mut conflicts = Vec::new();
        let (patch, widgets) = self.reversal_review(&change, Some(&mut conflicts))?;
        Ok(UndoReview {
            label: change.label.clone(),
            id: change.id.clone(),
            conflicts,
            patch,
            widgets,
            retire: Some(change),
        })
    }
    fn review_record<'a>(review: &'a UndoReview) -> Record<'a> {
        match &review.retire {
            Some(change) => Record::Retire(change),
            None => Record::Clear(&review.id),
        }
    }
    pub fn accept_undo_review(&self, review: &UndoReview) -> Result<AppliedChange, DesktopError> {
        self.transact(
            &review.patch,
            review.widgets.as_ref(),
            Operation::Undo,
            Self::review_record(review),
        )
        .map(|v| v.change)
    }
    pub fn keep_undo_current(&self, review: &UndoReview) -> Result<AppliedChange, DesktopError> {
        let mut patch = DesktopPatch::new();
        for (key, entry) in review.patch.iter() {
            patch.guard(key.clone(), entry.original.clone());
        }
        for (key, value) in review.patch.guards() {
            patch.guard(key.clone(), value.clone());
        }
        let mut widgets = WidgetPatch::new();
        if let Some(part) = &review.widgets {
            for (pointer, entry) in part.iter() {
                widgets
                    .set(pointer, entry.original.clone(), entry.original.clone())
                    .map_err(DesktopError::Io)?;
            }
        }
        self.transact(
            &patch,
            Some(&widgets),
            Operation::Undo,
            Self::review_record(review),
        )
        .map(|v| v.change)
    }
    pub(super) fn reversal(
        &self,
        latest: &LatestChange,
    ) -> Result<(DesktopPatch, Option<WidgetPatch>), DesktopError> {
        self.reversal_review(latest, None)
    }
    fn reversal_review(
        &self,
        latest: &LatestChange,
        mut conflicts: Option<&mut Vec<String>>,
    ) -> Result<(DesktopPatch, Option<WidgetPatch>), DesktopError> {
        if latest.owner == Owner::Recovery {
            let mut patch = DesktopPatch::exact();
            patch.exact_backgrounds =
                crate::desktop::wallpaper::modes::names_modes(latest.change.entries());
            for entry in latest.change.entries() {
                let current = self.read_user(&entry.key)?;
                if !exact_equal(&current, &entry.after) {
                    if let Some(conflicts) = conflicts.as_deref_mut() {
                        conflicts.push(activity::native_conflict(
                            &entry.key.namespace,
                            &entry.key.key,
                            current.as_ref().map(RawValue::as_str),
                            entry.before.as_ref().map(RawValue::as_str),
                        ));
                        patch.guard(entry.key.clone(), current);
                        continue;
                    }
                    return Err(DesktopError::Conflict(vec![entry.key.clone()]));
                }
                patch.set(entry.key.clone(), current, entry.before.clone());
            }
            let mut widgets = latest
                .widget
                .as_ref()
                .map(WidgetChange::reverse)
                .transpose()
                .map_err(DesktopError::Io)?;
            if let Some(widgets) = &mut widgets {
                let (raw, _) = automatic::read_config(self).map_err(DesktopError::Io)?;
                let entries = widgets
                    .iter()
                    .map(|(p, e)| (p, e.clone()))
                    .collect::<Vec<_>>();
                for (pointer, entry) in entries {
                    let current = widget::get(&raw, pointer);
                    if current != entry.original {
                        let Some(conflicts) = conflicts.as_deref_mut() else {
                            return Err(DesktopError::FieldConflict {
                                keys: vec![],
                                fields: vec![widget::label(pointer)],
                            });
                        };
                        conflicts.push(activity::widget_conflict(
                            pointer,
                            current.as_ref(),
                            entry.original.as_ref(),
                        ));
                        widgets
                            .set(pointer, current.clone(), current)
                            .map_err(DesktopError::Io)?;
                        widgets.forget_exact();
                    }
                }
            }
            return Ok((patch, widgets));
        }
        let history = self.history_file()?;
        // An owned Cancel can reverse an older action. Its derivation chain
        // must come from that receipt, never from the latest unrelated action.
        let file = history.as_ref().and_then(|history| {
            history
                .history
                .iter()
                .chain(std::iter::once(history))
                .find(|file| file.id == latest.id)
        });
        let mut patch = DesktopPatch::new();
        patch.exact_backgrounds =
            crate::desktop::wallpaper::modes::names_modes(latest.change.entries());
        for entry in latest.change.entries() {
            let mut expected = entry.after.clone();
            if let Some(derived) = file
                .into_iter()
                .flat_map(|file| &file.derived)
                .find(|derived| {
                    derived.namespace == entry.key.namespace
                        && derived.version == entry.key.version
                        && derived.key == entry.key.key
                })
            {
                if derived.before.as_deref() == expected.as_ref().map(RawValue::as_str) {
                    expected = derived.after.clone().map(RawValue::from_disk);
                }
            }
            let current = self.read_user(&entry.key)?;
            let desired = if current == expected {
                Some(entry.before.clone())
            } else {
                reverse_native(
                    &entry.key,
                    entry.before.as_ref(),
                    expected.as_ref(),
                    current.as_ref(),
                )
            };
            match desired {
                Some(desired) => {
                    if current == expected {
                        patch.reversed_native.insert(entry.key.clone());
                    }
                    patch.set(entry.key.clone(), current, desired);
                }
                None => {
                    let Some(conflicts) = conflicts.as_deref_mut() else {
                        return Err(DesktopError::Conflict(vec![entry.key.clone()]));
                    };
                    conflicts.push(activity::native_conflict(
                        &entry.key.namespace,
                        &entry.key.key,
                        current.as_ref().map(RawValue::as_str),
                        entry.before.as_ref().map(RawValue::as_str),
                    ));
                    patch.guard(entry.key.clone(), current);
                }
            }
        }
        let mut widgets = latest
            .widget
            .as_ref()
            .map(WidgetChange::reverse)
            .transpose()
            .map_err(DesktopError::UndoUnavailable)?;
        if let (Some(widget), Some(file), Some(change)) = (&mut widgets, file, &latest.widget) {
            for field in &change.fields {
                if let Some(derived) = file.derived_widget.iter().find(|derived| {
                    derived.pointer == field.pointer
                        && derived.before == field.after
                        && derived.after != field.after
                }) {
                    widget
                        .set(&field.pointer, derived.after.clone(), field.before.clone())
                        .map_err(DesktopError::UndoUnavailable)?;
                    // Exact-file restoration is no longer appropriate.
                    widget.forget_exact();
                }
            }
        }
        // A repeated native customization need not change the override mask.
        // Its builder inputs still require semantic restoration; the presence
        // of widget writes is not an indication of native appearance ownership.
        let native_appearance = latest
            .change
            .entries()
            .iter()
            .any(|entry| matches!(ThemeVariant::of(&entry.key), Some((_, true))));
        let derived_native = native_appearance
            && file.is_some_and(|file| {
                file.derived.iter().any(|entry| {
                    entry.before != entry.after
                        && ThemeVariant::ALL.into_iter().any(|variant| {
                            entry.namespace == variant.builder_namespace()
                                || entry.namespace == variant.theme_namespace()
                        })
                })
            });
        // Restoring appearance intent uses the current wallpaper. A later
        // external wallpaper is never restored as a side effect of a theme Undo.
        if widgets.is_some() || (native_appearance && self.app_config().is_some()) {
            let widgets = widgets.get_or_insert_with(WidgetPatch::new);
            let (raw, current) = automatic::read_config(self).map_err(DesktopError::Io)?;
            let mut candidate = raw.clone();
            let fields: Vec<_> = widgets
                .iter()
                .map(|(pointer, entry)| (pointer.to_owned(), entry.clone()))
                .collect();
            for (pointer, entry) in fields {
                let current = widget::get(&raw, &pointer);
                if current != entry.original {
                    // The serialized mask contains independent property intents.
                    // An outside customization of another property is unrelated.
                    if pointer == "/appearance/overrides" {
                        if let Some(desired) =
                            reverse_overrides(&entry.desired, &entry.original, &current)
                        {
                            widgets
                                .set(&pointer, current, desired.clone())
                                .map_err(DesktopError::Io)?;
                            widgets.forget_exact();
                            widget::put(&mut candidate, &pointer, desired)
                                .map_err(DesktopError::Io)?;
                            continue;
                        }
                    }
                    let Some(conflicts) = conflicts.as_deref_mut() else {
                        return Err(DesktopError::FieldConflict {
                            keys: vec![],
                            fields: vec![widget::label(&pointer)],
                        });
                    };
                    conflicts.push(activity::widget_conflict(
                        &pointer,
                        current.as_ref(),
                        entry.desired.as_ref(),
                    ));
                    widgets
                        .set(&pointer, current.clone(), current)
                        .map_err(DesktopError::Io)?;
                    widgets.forget_exact();
                } else {
                    patch.reversed_widget.insert(pointer.clone());
                    widget::put(&mut candidate, &pointer, entry.desired)
                        .map_err(DesktopError::Io)?;
                }
            }
            let config = config::check_config_change(&raw, &candidate).map_err(DesktopError::Io)?;
            let appearance_action = native_appearance
                || widgets.changes().any(|(pointer, _)| {
                    matches!(
                        pointer,
                        "/appearance/family"
                            | "/appearance/overrides"
                            | "/appearance/accent"
                            | "/appearance/wallpaper_colors"
                            | "/appearance/color_policy"
                            | "/appearance/sync_colors"
                    )
                });
            // A wallpaper action owns its own image reversal. Its original
            // derivation is already part of that same transaction.
            let owns_wallpaper = patch
                .keys()
                .any(|key| crate::desktop::wallpaper::modes::is_background(key.namespace()));
            // Without following or an advanced native derivation, the exact
            // reversal remains valid, including absent keys and custom values
            // that the current builder does not generate.
            if appearance_action
                && (derived_native || config.appearance.wallpaper_colors)
                && !owns_wallpaper
                && conflicts.as_ref().is_none_or(|c| c.is_empty())
            {
                let linked = if config.appearance.wallpaper_colors {
                    let wallpaper = WallpaperDraft::load(self)?;
                    let source = automatic::source(self, &wallpaper);
                    let suggestion = source.analyse().map_err(DesktopError::Io)?;
                    let rgb = automatic::recommended_for(&suggestion, config.appearance.family)
                        .ok_or_else(|| {
                            DesktopError::Io("Cannot resolve wallpaper colors".into())
                        })?;
                    let light = automatic::light(&suggestion);
                    for (pointer, value, unchanged) in [
                        (
                            "/appearance/accent",
                            serde_json::json!(rgb),
                            config.appearance.accent == Some(rgb),
                        ),
                        (
                            "/appearance/wallpaper_light",
                            serde_json::json!(light),
                            config.appearance.wallpaper_light == light,
                        ),
                        (
                            "/appearance/wallpaper_color_stamp",
                            serde_json::json!(source.stamp),
                            config.appearance.wallpaper_color_stamp == source.stamp,
                        ),
                    ] {
                        // Preserve absent defaults as well as explicit values.
                        // A native-only reversal must not manufacture widget
                        // writes just to participate in semantic restoration.
                        let desired = if unchanged {
                            widget::get(&candidate, pointer)
                        } else {
                            Some(value)
                        };
                        widgets
                            .set(pointer, widget::get(&raw, pointer), desired)
                            .map_err(DesktopError::Io)?;
                    }
                    wallpaper.guard(&mut patch);
                    config.appearance.sync_colors.then_some(rgb)
                } else {
                    None
                };
                if native_appearance || linked.is_some() {
                    self.restore_native_appearance(
                        &mut patch,
                        latest,
                        current.appearance,
                        config.appearance,
                        linked,
                    )?;
                }
                // Even a native-only Undo depends on today's applied intent.
                // Keep those reads under the config lock without writing or
                // claiming ownership of otherwise unchanged widget metadata.
                for pointer in [
                    "/appearance/family",
                    "/appearance/overrides",
                    "/appearance/color_policy",
                    "/appearance/wallpaper_colors",
                    "/appearance/sync_colors",
                ] {
                    if !widgets.iter().any(|(field, _)| field == pointer) {
                        let original = widget::get(&raw, pointer);
                        widgets
                            .set(pointer, original.clone(), original)
                            .map_err(DesktopError::Io)?;
                    }
                }
                widgets.forget_exact();
            }
        }
        Ok((patch, widgets))
    }

    fn restore_native_appearance(
        &self,
        patch: &mut DesktopPatch,
        latest: &LatestChange,
        current: config::Appearance,
        desired: config::Appearance,
        linked: Option<[u8; 3]>,
    ) -> Result<(), DesktopError> {
        use crate::desktop::profiles::themes;
        use appearance::{ColorRole, Control, Value as Edit, VariantValues};
        let observed = self.load(&AppearanceDraft::keys())?;
        let mut restored = observed.clone();
        for (key, entry) in patch.iter() {
            if let Some(mut value) = restored.entry(key).cloned() {
                value.user = entry.desired.clone();
                restored.insert(key.clone(), value);
            }
        }
        let mut target = AppearanceDraft::from_snapshot(restored);
        if let Some(rgb) = linked {
            // Most restored roles can be inferred from the restored color intent.
            themes::linked_colors(&mut target, desired, rgb);
            themes::linked_accents(&mut target, desired, rgb);
            // An override-only action does not restore the global accent. Its
            // native before-value belongs to the wallpaper at that earlier time.
            // Recover only the roles this action restores, using its recorded
            // intent; other external/custom roles stay outside the operation.
            if let Some(change) = &latest.widget {
                let before = config::parse_config_text(change.before.as_deref())
                    .and_then(|raw| config::config_from_value(&raw))
                    .map_err(DesktopError::Io)?
                    .appearance;
                if before.family == desired.family {
                    let mut followed = AppearanceDraft::from_snapshot(target.snapshot().clone());
                    themes::linked_colors(&mut followed, before, rgb);
                    themes::linked_accents(&mut followed, before, rgb);
                    for (control, value) in followed.edits() {
                        let Control::Color(variant, role) = *control else {
                            continue;
                        };
                        let key = appearance::theme::builder_key(
                            variant,
                            match role {
                                ColorRole::Accent => "accent",
                                ColorRole::Background => "bg_color",
                                ColorRole::Primary => "primary_container_bg",
                                ColorRole::Secondary => "secondary_container_bg",
                                ColorRole::Outline => "window_hint",
                            },
                        );
                        let bit = crate::desktop::actions::override_for_native(&key);
                        if desired.overrides & bit == 0
                            && (patch.keys().any(|owned| *owned == key)
                                || (current.overrides & bit != 0))
                        {
                            target.set(*control, value.clone());
                        }
                    }
                }
            }
        }
        // Build from today's native state with the restored semantic inputs.
        // Explicit native actions still need this when coordination is off;
        // only automatic linked-role updates depend on Whole desktop scope.
        // Replaying old generated containers here would put an earlier
        // wallpaper back even after the override bit has been cleared.
        let mut semantic = AppearanceDraft::from_snapshot(observed);
        let mut rebuilt = Vec::new();
        for variant in ThemeVariant::ALL {
            let builder = target
                .builder(variant)
                .map_err(|e| DesktopError::UndoUnavailable(e.reason))?;
            semantic.set(
                Control::Variant(variant),
                Edit::Variant(Box::new(VariantValues::of(&builder))),
            );
            if semantic.edited(Control::Variant(variant))
                || patch
                    .keys()
                    .any(|key| ThemeVariant::of(key) == Some((variant, true)))
            {
                rebuilt.push(variant);
            }
            let outline = appearance::theme::builder_key(variant, "window_hint");
            if patch.keys().any(|key| *key == outline) && target.outline_follows(variant) {
                semantic.set(
                    Control::Color(variant, ColorRole::Outline),
                    Edit::Color(None),
                );
            }
        }
        let plan = semantic.plan();
        if let Some(blocked) = plan.blocked.first() {
            return Err(DesktopError::UndoUnavailable(blocked.reason.clone()));
        }
        let obsolete: Vec<_> = patch
            .keys()
            .filter(|key| {
                ThemeVariant::of(key)
                    .is_some_and(|(variant, builder)| !builder && rebuilt.contains(&variant))
            })
            .cloned()
            .collect();
        for key in obsolete {
            let original = patch.entries.remove(&key).expect("owned").original;
            patch.guard(key, original);
        }
        patch.extend(plan.patch);
        Ok(())
    }
}

fn reverse_overrides(
    before: &Option<Value>,
    after: &Option<Value>,
    current: &Option<Value>,
) -> Option<Option<Value>> {
    let bits = |value: &Option<Value>| match value {
        None => Some(0),
        Some(value) => value.as_u64(),
    };
    let (b, a, c) = (bits(before)?, bits(after)?, bits(current)?);
    let changed = b ^ a;
    if (c ^ a) & changed != 0 {
        return None;
    }
    let restored = (c & !changed) | (b & changed);
    Some(if restored == 0 && before.is_none() {
        None
    } else {
        Some(restored.into())
    })
}


/// Merge only members changed by the action. An external change to one of
/// those members is a conflict; unrelated members of a native container stay.
fn reverse_value(before: &Value, after: &Value, current: &mut Value) -> Result<(), ()> {
    if before == after {
        return Ok(());
    }
    if let (Some(before), Some(after), Some(current)) = (
        before.as_object(),
        after.as_object(),
        current.as_object_mut(),
    ) {
        if before.keys().ne(after.keys()) {
            return Err(());
        }
        for (key, value) in before {
            reverse_value(value, &after[key], current.get_mut(key).ok_or(())?)?;
        }
        Ok(())
    } else if current == after {
        *current = before.clone();
        Ok(())
    } else {
        Err(())
    }
}
fn reverse_typed<T: serde::Serialize + serde::de::DeserializeOwned>(
    before: &RawValue,
    after: &RawValue,
    current: &RawValue,
) -> Option<RawValue> {
    let json = |raw: &RawValue| serde_json::to_value(appearance::decode_exact::<T>(raw).ok()?).ok();
    let mut value = json(current)?;
    reverse_value(&json(before)?, &json(after)?, &mut value).ok()?;
    RawValue::encode(&serde_json::from_value::<T>(value).ok()?).ok()
}
/// Apply only a known derivation's changed members to an older expected
/// container. A different value in a changed member breaks the chain.
pub(super) fn advance_native(
    entry: &JournalEntry,
    expected: &Option<String>,
) -> Option<Option<String>> {
    let key = ConfigKey::new(&entry.namespace, entry.version, &entry.key).ok()?;
    let before = entry.before.clone().map(RawValue::from_disk);
    let after = entry.after.clone().map(RawValue::from_disk);
    let expected = expected.clone().map(RawValue::from_disk);
    reverse_native(&key, after.as_ref(), before.as_ref(), expected.as_ref())
        .map(|value| value.map(|raw| raw.as_str().to_owned()))
}
fn reverse_native(
    key: &ConfigKey,
    before: Option<&RawValue>,
    after: Option<&RawValue>,
    current: Option<&RawValue>,
) -> Option<Option<RawValue>> {
    let (before, after, current) = (before?, after?, current?);
    let (_, false) = ThemeVariant::of(key)? else {
        return None;
    };
    use cosmic::cosmic_theme::{Component, Container};
    reverse_typed::<Container>(before, after, current)
        .or_else(|| reverse_typed::<Component>(before, after, current))
        .map(Some)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::desktop::store::tests::Fixture;
    use serde_json::json;

    fn setup(name: &str) -> (Fixture, DesktopStore) {
        let fixture = Fixture::new(name);
        let path = fixture.0.join("config.json");
        fs::write(&path, r#"{"appearance":{"opacity":90},"future":42}"#).unwrap();
        let store = fixture.store().with_app_config(path);
        (fixture, store)
    }
    fn widget_action(
        store: &DesktopStore,
        pointer: &str,
        value: Value,
        source: Source,
    ) -> LatestChange {
        let raw = automatic::read_config(store).unwrap().0;
        let mut widget = WidgetPatch::new();
        widget
            .set(pointer, widget::get(&raw, pointer), Some(value))
            .unwrap();
        store
            .apply_action(
                &DesktopPatch::new(),
                Some(&widget),
                Owner::Preferences,
                "Card opacity",
                source,
            )
            .unwrap()
    }
    #[test]
    fn manual_history_survives_derived_updates_restart_and_unrelated_external_fields() {
        let (fixture, store) = setup("manual-history-derived");
        let first = widget_action(&store, "/appearance/opacity", json!(80), Source::Manual);
        let second = widget_action(
            &store,
            "/appearance/corner_radius",
            json!(16),
            Source::Manual,
        );
        widget_action(
            &store,
            "/appearance/accent",
            json!([12, 120, 180]),
            Source::Derived,
        );
        assert_eq!(store.latest_change().unwrap().unwrap().id, second.id);
        let mut raw = automatic::read_config(&store).unwrap().0;
        raw["weather"] = json!({"show_dew_point":true});
        fs::write(store.app_config().unwrap(), raw.to_string()).unwrap();
        let restarted = fixture
            .store()
            .with_app_config(store.app_config().unwrap().to_path_buf());
        restarted.undo_latest(&second.id).unwrap();
        let raw = automatic::read_config(&restarted).unwrap().0;
        assert_eq!(raw["weather"]["show_dew_point"], true);
        assert_eq!(raw["appearance"]["accent"], json!([12, 120, 180]));
        assert_eq!(restarted.latest_change().unwrap().unwrap().id, first.id);
        restarted.undo_latest(&first.id).unwrap();
        assert_eq!(
            automatic::read_config(&restarted)
                .unwrap()
                .1
                .appearance
                .opacity,
            90
        );
    }
    #[test]
    fn same_field_external_write_refuses_undo_and_keeps_history() {
        let (_fixture, store) = setup("manual-history-conflict");
        let action = widget_action(&store, "/appearance/opacity", json!(80), Source::Manual);
        let mut raw = automatic::read_config(&store).unwrap().0;
        raw["appearance"]["opacity"] = json!(75);
        fs::write(store.app_config().unwrap(), raw.to_string()).unwrap();
        assert!(matches!(
            store.undo_latest(&action.id),
            Err(DesktopError::FieldConflict { .. })
        ));
        assert_eq!(store.latest_change().unwrap().unwrap().id, action.id);
        assert_eq!(
            automatic::read_config(&store).unwrap().1.appearance.opacity,
            75
        );
    }
    #[test]
    fn scheduled_and_derived_actions_never_advance_manual_precedence() {
        let (_fixture, store) = setup("manual-provenance");
        let action = widget_action(&store, "/appearance/opacity", json!(80), Source::Manual);
        let stamp = store.manual_at();
        widget_action(
            &store,
            "/appearance/accent",
            json!([20, 80, 170]),
            Source::Derived,
        );
        widget_action(
            &store,
            "/appearance/corner_radius",
            json!(18),
            Source::Scheduled,
        );
        assert_eq!(store.manual_at(), stamp);
        assert_eq!(store.latest_change().unwrap().unwrap().id, action.id);
    }
    #[test]
    fn conflict_review_restores_only_unchanged_fields_and_is_guarded() {
        let (_fixture, store) = setup("review-undo-scope");
        let raw = automatic::read_config(&store).unwrap().0;
        let mut patch = WidgetPatch::new();
        for (pointer, value) in [
            ("/appearance/opacity", json!(80)),
            ("/appearance/corner_radius", json!(16)),
        ] {
            patch
                .set(pointer, widget::get(&raw, pointer), Some(value))
                .unwrap();
        }
        let action = store
            .apply_action(
                &DesktopPatch::new(),
                Some(&patch),
                Owner::Preferences,
                "Widget style",
                Source::Manual,
            )
            .unwrap();
        let mut changed = automatic::read_config(&store).unwrap().0;
        changed["appearance"]["opacity"] = json!(75);
        fs::write(store.app_config().unwrap(), changed.to_string()).unwrap();
        let review = store.review_undo(&action.id).unwrap();
        assert_eq!(review.conflicts.len(), 1);
        assert!(review.conflicts[0].contains("75"));
        changed["appearance"]["opacity"] = json!(74);
        fs::write(store.app_config().unwrap(), changed.to_string()).unwrap();
        assert!(
            store.accept_undo_review(&review).is_err(),
            "a review pins even retained values"
        );
        let review = store.review_undo(&action.id).unwrap();
        store.accept_undo_review(&review).unwrap();
        let raw = automatic::read_config(&store).unwrap().0;
        assert_eq!(raw["appearance"]["opacity"], 74);
        assert!(raw.pointer("/appearance/corner_radius").is_none());
        assert!(store.latest_change().unwrap().is_none());
        assert!(
            store.manual_at().is_some(),
            "Undo remains a manual choice with no older history"
        );
    }
    #[test]
    fn keep_current_resolves_a_conflict_without_changing_preferences() {
        let (_fixture, store) = setup("keep-current-undo");
        let first = widget_action(&store, "/appearance/opacity", json!(80), Source::Manual);
        let second = widget_action(
            &store,
            "/appearance/corner_radius",
            json!(16),
            Source::Manual,
        );
        let review = store.review_undo(&second.id).unwrap();
        let bytes = fs::read(store.app_config().unwrap()).unwrap();
        store.keep_undo_current(&review).unwrap();
        assert_eq!(fs::read(store.app_config().unwrap()).unwrap(), bytes);
        assert_eq!(store.latest_change().unwrap().unwrap().id, first.id);
    }
    #[test]
    fn a_newer_manual_choice_invalidates_an_inflight_scheduled_transaction() {
        let (_fixture, store) = setup("schedule-manual-guard");
        let raw = automatic::read_config(&store).unwrap().0;
        let mut native = DesktopPatch::new();
        native.manual_guard = Some(store.manual_at());
        let mut patch = WidgetPatch::new();
        patch
            .set(
                "/appearance/corner_radius",
                widget::get(&raw, "/appearance/corner_radius"),
                Some(json!(12)),
            )
            .unwrap();
        let manual = widget_action(&store, "/appearance/opacity", json!(80), Source::Manual);
        assert!(
            store
                .apply_action(
                    &native,
                    Some(&patch),
                    Owner::Profile,
                    "Scheduled look",
                    Source::Scheduled
                )
                .is_err()
        );
        assert_eq!(store.latest_change().unwrap().unwrap().id, manual.id);
        assert!(
            automatic::read_config(&store)
                .unwrap()
                .0
                .pointer("/appearance/corner_radius")
                .is_none()
        );
    }
    #[test]
    fn history_is_bounded_without_losing_the_latest_action() {
        let (_fixture, store) = setup("bounded-history");
        for i in 0..40 {
            widget_action(
                &store,
                "/appearance/opacity",
                json!(70 + i % 30),
                Source::Manual,
            );
        }
        let file = store.history_file().unwrap().unwrap();
        assert_eq!(file.history.len() + 1, undo::HISTORY_LIMIT);
        assert_eq!(store.latest_change().unwrap().unwrap().id, file.id);
    }
    #[test]
    fn theme_undo_after_recolor_uses_current_wallpaper_and_keeps_outside_edits() {
        use crate::desktop::profiles::{self, Installed, themes};
        use crate::desktop::wallpaper::{BgColor, BgEntry, BgSource, NAMESPACE};
        use crate::desktop_theme::Family;
        let (fixture, store) = setup("semantic-theme-undo");
        let set_wallpaper = |rgb| {
            fixture.user(
                NAMESPACE,
                "all",
                &ron::to_string(&BgEntry::new("all", BgSource::Color(BgColor::Single(rgb))))
                    .unwrap(),
            )
        };
        set_wallpaper([0.2, 0.6, 0.7]);
        let installed = Installed::discover(&store);
        let apply_theme = |family| {
            let mut profile = themes::record(family);
            themes::LookColor::Wallpaper.set(&mut profile);
            profiles::apply(
                &store,
                &profiles::plan(
                    &store,
                    &installed,
                    &profile,
                    &profile.domains().into_iter().collect(),
                ),
            )
            .unwrap()
        };
        apply_theme(Family::Soft);
        automatic::commit(&store, automatic::prepare(&store).unwrap().unwrap()).unwrap();
        let manual = apply_theme(Family::Outline);
        set_wallpaper([0.8, 0.3, 0.2]);
        automatic::commit(&store, automatic::prepare(&store).unwrap().unwrap()).unwrap();
        fixture.user("com.system76.CosmicPanel.Dock", "anchor", "Left");
        let image = fixture.read(NAMESPACE, "all");
        store.undo_latest(&manual.id).unwrap();
        let config = automatic::read_config(&store).unwrap().1;
        assert_eq!(config.appearance.family, Some(Family::Soft));
        assert_eq!(fixture.read(NAMESPACE, "all"), image);
        assert_eq!(
            fixture
                .read("com.system76.CosmicPanel.Dock", "anchor")
                .as_deref(),
            Some("Left")
        );
        let source = automatic::source(&store, &WallpaperDraft::load(&store).unwrap());
        assert_eq!(
            config.appearance.accent,
            automatic::recommended_for(&source.analyse().unwrap(), Some(Family::Soft))
        );
    }
    #[test]
    fn undo_preserves_unrelated_external_property_intent() {
        let (_fixture, store) = setup("history-property-intent");
        let action = widget_action(&store, "/appearance/overrides", json!(1), Source::Manual);
        let mut raw = automatic::read_config(&store).unwrap().0;
        raw["appearance"]["overrides"] = json!(3);
        raw["appearance"]["corner_radius"] = json!(18);
        fs::write(store.app_config().unwrap(), raw.to_string()).unwrap();
        store.undo_latest(&action.id).unwrap();
        let actual = automatic::read_config(&store).unwrap().0;
        assert_eq!(actual["appearance"]["overrides"], 2);
        assert_eq!(actual["appearance"]["corner_radius"], 18);
        let action = widget_action(&store, "/appearance/overrides", json!(3), Source::Manual);
        let mut raw = actual;
        raw["appearance"]["overrides"] = json!(2);
        fs::write(store.app_config().unwrap(), raw.to_string()).unwrap();
        assert!(
            matches!(
                store.undo_latest(&action.id),
                Err(DesktopError::FieldConflict { .. })
            ),
            "a changed owned bit is still a conflict"
        );
    }
    #[test]
    fn independent_r1_theme_undo_after_intervening_manual_action_and_recolor() {
        use crate::desktop::profiles::{self, Installed, themes};
        use crate::desktop::wallpaper::{BgColor, BgEntry, BgSource, NAMESPACE};
        use crate::desktop_theme::Family;
        let (fixture, store) = setup("review-r1-history-intervening");
        let set_wallpaper = |rgb| {
            fixture.user(
                NAMESPACE,
                "all",
                &ron::to_string(&BgEntry::new("all", BgSource::Color(BgColor::Single(rgb))))
                    .unwrap(),
            )
        };
        set_wallpaper([0.2, 0.6, 0.7]);
        let installed = Installed::discover(&store);
        let apply_theme = |family| {
            let mut profile = themes::record(family);
            themes::LookColor::Wallpaper.set(&mut profile);
            profiles::apply(
                &store,
                &profiles::plan(
                    &store,
                    &installed,
                    &profile,
                    &profile.domains().into_iter().collect(),
                ),
            )
            .unwrap()
        };
        apply_theme(Family::Soft);
        automatic::commit(&store, automatic::prepare(&store).unwrap().unwrap()).unwrap();
        let theme = apply_theme(Family::Outline);
        let later = widget_action(&store, "/clock/show_date", json!(false), Source::Manual);
        set_wallpaper([0.8, 0.3, 0.2]);
        automatic::commit(&store, automatic::prepare(&store).unwrap().unwrap()).unwrap();
        store.undo_latest(&later.id).unwrap();
        assert_eq!(store.latest_change().unwrap().unwrap().id, theme.id);
        let review = store.review_undo(&theme.id).unwrap();
        assert!(
            review.conflicts.is_empty(),
            "Only derived colors followed this action, not external edits to its fields: {:?}",
            review.conflicts
        );
        store.undo_latest(&theme.id).unwrap();
        assert_eq!(
            automatic::read_config(&store).unwrap().1.appearance.family,
            Some(Family::Soft)
        );
    }

    #[test]
    fn independent_r1_undo_native_override_relinks_current_wallpaper() {
        use crate::desktop::appearance::{ColorRole, Control, Value as Edit};
        use crate::desktop::profiles::{self, Installed, themes};
        use crate::desktop::wallpaper::{BgColor, BgEntry, BgSource, NAMESPACE};
        use crate::desktop_theme::Family;
        let (fixture, store) = setup("review-r1-undo-native-override");
        let set_wallpaper = |rgb| {
            fixture.user(
                NAMESPACE,
                "all",
                &ron::to_string(&BgEntry::new("all", BgSource::Color(BgColor::Single(rgb))))
                    .unwrap(),
            )
        };
        set_wallpaper([0.2, 0.6, 0.7]);
        let mut profile = themes::record(Family::Soft);
        themes::LookColor::Wallpaper.set(&mut profile);
        profiles::apply(
            &store,
            &profiles::plan(
                &store,
                &Installed::discover(&store),
                &profile,
                &profile.domains().into_iter().collect(),
            ),
        )
        .unwrap();
        automatic::commit(&store, automatic::prepare(&store).unwrap().unwrap()).unwrap();
        let mut draft = AppearanceDraft::load(&store).unwrap();
        draft.set(
            Control::Color(ThemeVariant::Dark, ColorRole::Background),
            Edit::Color(appearance::parse_hex("#604020", false)),
        );
        let action = store
            .apply_owned(&draft.patch(), None, Owner::Appearance, "Custom background")
            .unwrap();
        assert!(
            automatic::read_config(&store)
                .unwrap()
                .1
                .appearance
                .overrides
                & (1 << 25)
                != 0
        );
        set_wallpaper([0.8, 0.3, 0.2]);
        automatic::commit(&store, automatic::prepare(&store).unwrap().unwrap()).unwrap();
        store.undo_latest(&action.id).unwrap();
        let current = automatic::read_config(&store).unwrap().1;
        let draft = AppearanceDraft::load(&store).unwrap();
        assert_eq!(current.appearance.overrides & (1 << 25), 0);
        assert!(
            themes::linked_surface(
                &draft,
                current.appearance,
                ThemeVariant::Dark,
                ColorRole::Background
            ),
            "Undo must restore following the CURRENT wallpaper; actual={:?}, current_accent={:?}, automatic_retry={:?}",
            draft.color(ThemeVariant::Dark, ColorRole::Background),
            current.appearance.accent,
            automatic::prepare(&store).unwrap().is_some()
        );
    }
    #[test]
    fn repair_r1_native_color_undo_restores_every_supported_role_after_recolor() {
        use crate::desktop::profiles::{self, Installed, themes};
        use crate::desktop::wallpaper::{BgColor, BgEntry, BgSource, NAMESPACE};
        use crate::desktop_theme::Family;
        use appearance::{ColorRole, Control, Value as Edit};
        for family in [Family::Soft, Family::Paper] {
            for role in ColorRole::ALL {
                let (fixture, store) = setup(&format!("repair-role-{family:?}-{role:?}"));
                let wallpaper = |rgb| {
                    fixture.user(
                        NAMESPACE,
                        "all",
                        &ron::to_string(&BgEntry::new(
                            "all",
                            BgSource::Color(BgColor::Single(rgb)),
                        ))
                        .unwrap(),
                    )
                };
                wallpaper([0.2, 0.6, 0.7]);
                let mut profile = themes::record(family);
                themes::LookColor::Wallpaper.set(&mut profile);
                profiles::apply(
                    &store,
                    &profiles::plan(
                        &store,
                        &Installed::discover(&store),
                        &profile,
                        &profile.domains().into_iter().collect(),
                    ),
                )
                .unwrap();
                if let Some(work) = automatic::prepare(&store).unwrap() {
                    automatic::commit(&store, work).unwrap();
                }
                let variant = if family.light() {
                    ThemeVariant::Light
                } else {
                    ThemeVariant::Dark
                };
                let mut draft = AppearanceDraft::load(&store).unwrap();
                draft.set(
                    Control::Color(variant, role),
                    Edit::Color(appearance::parse_hex("#604020", false)),
                );
                let action = store
                    .apply_owned(
                        &draft.patch(),
                        None,
                        Owner::Appearance,
                        "Custom native color",
                    )
                    .unwrap();
                wallpaper([0.8, 0.3, 0.2]);
                automatic::commit(&store, automatic::prepare(&store).unwrap().unwrap()).unwrap();
                let reopened = fixture
                    .store()
                    .with_app_config(fixture.0.join("config.json"));
                reopened.undo_latest(&action.id).unwrap();
                for next in [None, Some([0.2, 0.7, 0.4])] {
                    if let Some(rgb) = next {
                        wallpaper(rgb);
                        automatic::commit(
                            &reopened,
                            automatic::prepare(&reopened).unwrap().unwrap(),
                        )
                        .unwrap();
                    }
                    let appearance = automatic::read_config(&reopened).unwrap().1.appearance;
                    let draft = AppearanceDraft::load(&reopened).unwrap();
                    match role {
                        ColorRole::Background | ColorRole::Primary => assert!(
                            themes::linked_surface(&draft, appearance, variant, role),
                            "{family:?} {role:?} must resume following"
                        ),
                        ColorRole::Accent => {
                            assert!(themes::linked_accent(&draft, appearance, variant))
                        }
                        ColorRole::Secondary | ColorRole::Outline => {
                            assert_eq!(draft.color(variant, role).known().copied(), Some(None))
                        }
                    }
                    let built = draft.builder(variant).unwrap().build();
                    for (key, expected) in [
                        ("background", built.background(false).base),
                        ("primary", built.primary(false).base),
                        ("secondary", built.secondary(false).base),
                    ] {
                        let key = appearance::theme::theme_key(variant, key);
                        let actual = reopened
                            .read_user(&key)
                            .unwrap()
                            .unwrap()
                            .decode::<cosmic::cosmic_theme::Container>()
                            .unwrap();
                        assert_eq!(
                            appearance::hex(actual.base, true),
                            appearance::hex(expected, true),
                            "{family:?} {role:?}: generated {key} matches current builder"
                        );
                    }
                    assert!(
                        automatic::prepare(&reopened).unwrap().is_none(),
                        "acknowledgment describes the restored current colors"
                    );
                }
            }
        }
    }

    #[test]
    fn repair_r1_older_actions_keep_derivation_across_multiple_reloads_and_undos() {
        use crate::desktop::profiles::{self, Installed, themes};
        use crate::desktop::wallpaper::{BgColor, BgEntry, BgSource, NAMESPACE};
        use crate::desktop_theme::Family;
        let (fixture, store) = setup("repair-history-stack");
        let wallpaper = |rgb| {
            fixture.user(
                NAMESPACE,
                "all",
                &ron::to_string(&BgEntry::new("all", BgSource::Color(BgColor::Single(rgb))))
                    .unwrap(),
            )
        };
        wallpaper([0.2, 0.6, 0.7]);
        let theme = |family| {
            let mut profile = themes::record(family);
            themes::LookColor::Wallpaper.set(&mut profile);
            profiles::apply(
                &store,
                &profiles::plan(
                    &store,
                    &Installed::discover(&store),
                    &profile,
                    &profile.domains().into_iter().collect(),
                ),
            )
            .unwrap()
        };
        let soft = theme(Family::Soft);
        if let Some(work) = automatic::prepare(&store).unwrap() {
            automatic::commit(&store, work).unwrap();
        }
        let outline = theme(Family::Outline);
        let mut actions = Vec::new();
        for (value, rgb) in [
            (false, [0.8, 0.3, 0.2]),
            (true, [0.3, 0.7, 0.4]),
            (false, [0.6, 0.3, 0.7]),
        ] {
            actions.push(widget_action(
                &store,
                "/clock/show_date",
                json!(value),
                Source::Manual,
            ));
            wallpaper(rgb);
            automatic::commit(&store, automatic::prepare(&store).unwrap().unwrap()).unwrap();
        }
        for action in actions.into_iter().rev().chain([outline, soft]) {
            let reopened = fixture
                .store()
                .with_app_config(fixture.0.join("config.json"));
            assert_eq!(reopened.latest_change().unwrap().unwrap().id, action.id);

            assert!(
                reopened
                    .review_undo(&action.id)
                    .unwrap()
                    .conflicts
                    .is_empty(),
                "{} is app-derived, not an external conflict: {:?}",
                action.label,
                reopened.review_undo(&action.id).unwrap().conflicts
            );
            reopened.undo_latest(&action.id).unwrap();
        }
        assert_eq!(
            automatic::read_config(&store).unwrap().1.appearance.family,
            None
        );
    }

    #[test]
    fn repair_r1_derived_attribution_does_not_hide_a_broken_external_chain() {
        let (fixture, store) = setup("repair-history-external");
        let first = widget_action(
            &store,
            "/appearance/accent",
            json!([10, 20, 30]),
            Source::Manual,
        );
        let clock = widget_action(&store, "/clock/show_date", json!(false), Source::Manual);
        let path = fixture.0.join("config.json");
        let mut raw = automatic::read_config(&store).unwrap().0;
        widget::put(&mut raw, "/appearance/accent", Some(json!([90, 80, 70]))).unwrap();
        fs::write(&path, serde_json::to_string(&raw).unwrap()).unwrap();
        widget_action(
            &store,
            "/appearance/accent",
            json!([40, 50, 60]),
            Source::Derived,
        );
        let reopened = fixture.store().with_app_config(path);
        reopened.undo_latest(&clock.id).unwrap();
        assert!(
            !reopened
                .review_undo(&first.id)
                .unwrap()
                .conflicts
                .is_empty()
        );
        assert!(reopened.undo_latest(&first.id).is_err());
        assert_eq!(
            automatic::read_config(&reopened)
                .unwrap()
                .1
                .appearance
                .accent,
            Some([40, 50, 60])
        );
    }

    #[test]
    fn repair_r1_partial_or_kept_undo_cannot_attribute_outside_values_to_older_actions() {
        for keep in [false, true] {
            let (fixture, store) = setup("repair-history-retained");
            let key = ConfigKey::new("com.system76.CosmicPanel.Dock", 1, "opacity").unwrap();
            let action = |opacity, corner, native: f32, source| {
                let raw = automatic::read_config(&store).unwrap().0;
                let mut widget = WidgetPatch::new();
                for (pointer, value) in [
                    ("/appearance/opacity", json!(opacity)),
                    ("/appearance/corner_radius", json!(corner)),
                ] {
                    widget
                        .set(pointer, widget::get(&raw, pointer), Some(value))
                        .unwrap();
                }
                let mut patch = DesktopPatch::new();
                patch.set(
                    key.clone(),
                    store.read_user(&key).unwrap(),
                    Some(RawValue::encode(&native).unwrap()),
                );
                store
                    .apply_action(&patch, Some(&widget), Owner::Preferences, "Style", source)
                    .unwrap()
            };
            let first = action(80, 12, 0.8, Source::Manual);
            let second = action(85, 16, 0.6, Source::Manual);
            let mut raw = automatic::read_config(&store).unwrap().0;
            raw["appearance"]["opacity"] = json!(75);
            fs::write(store.app_config().unwrap(), raw.to_string()).unwrap();
            fixture.user(key.namespace(), key.key(), "0.7");
            let review = store.review_undo(&second.id).unwrap();
            assert_eq!(review.conflicts.len(), 2);
            if keep {
                store.keep_undo_current(&review).unwrap();
            } else {
                store.accept_undo_review(&review).unwrap();
            }
            assert_eq!(
                automatic::read_config(&store).unwrap().1.appearance.opacity,
                75
            );
            assert_eq!(
                store
                    .read_user(&key)
                    .unwrap()
                    .unwrap()
                    .decode::<f32>()
                    .unwrap(),
                0.7
            );
            // Later derived work must not turn a retained outside value into
            // evidence that the older action still owns either property.
            action(77, if keep { 16 } else { 12 }, 0.65, Source::Derived);
            let reopened = fixture
                .store()
                .with_app_config(store.app_config().unwrap().to_path_buf());
            assert!(reopened.review_undo(&first.id).unwrap().conflicts.len() >= 2);
            assert!(reopened.undo_latest(&first.id).is_err());
        }
    }
    fn assert_current_generated_colors(store: &DesktopStore, variant: ThemeVariant) {
        // Compare the complete serialized colors: COSMIC stores 8-bit channels,
        // while its builder computes unquantized floating-point values.
        let built = AppearanceDraft::load(store)
            .unwrap()
            .builder(variant)
            .unwrap()
            .build();
        for (name, expected) in [
            ("background", built.background(false)),
            ("primary", built.primary(false)),
            ("secondary", built.secondary(false)),
            ("transparent_background", built.background(true)),
            ("transparent_primary", built.primary(true)),
            ("transparent_secondary", built.secondary(true)),
        ] {
            let key = appearance::theme::theme_key(variant, name);
            if let Some(actual) = store.read_user(&key).unwrap() {
                assert_eq!(
                    serde_json::to_value(
                        actual.decode::<cosmic::cosmic_theme::Container>().unwrap()
                    )
                    .unwrap(),
                    serde_json::to_value(expected).unwrap(),
                    "all generated container fields match today's builder: {key}"
                );
            }
        }
        for (name, expected) in [
            ("button", &built.button),
            ("list_button", &built.list_button),
            ("accent", &built.accent),
        ] {
            let key = appearance::theme::theme_key(variant, name);
            if let Some(actual) = store.read_user(&key).unwrap() {
                assert_eq!(
                    serde_json::to_value(
                        actual.decode::<cosmic::cosmic_theme::Component>().unwrap()
                    )
                    .unwrap(),
                    serde_json::to_value(expected).unwrap(),
                    "all generated component fields match today's builder: {key}"
                );
            }
        }
    }

    #[test]
    fn repair_r2_consecutive_native_roles_keep_current_derivation_across_reloads() {
        use crate::desktop::profiles::themes;
        use crate::desktop_theme::Family;
        use appearance::{ColorRole, Control, Value as Edit};
        for family in [Family::Soft, Family::Paper] {
            for role in ColorRole::ALL {
                let (f, store) = setup(&format!("r2-repeated-{family:?}-{role:?}"));
                r2_wallpaper(&f, [0.2, 0.6, 0.7]);
                r2_theme(&store, family);
                r2_recolor(&store);
                let variant = if family.light() {
                    ThemeVariant::Light
                } else {
                    ThemeVariant::Dark
                };
                let mut actions = Vec::new();
                for hex in ["#604020", "#206040", "#402060"] {
                    let mut draft = AppearanceDraft::load(&store).unwrap();
                    draft.set(
                        Control::Color(variant, role),
                        Edit::Color(appearance::parse_hex(hex, false)),
                    );
                    let action = store
                        .apply_owned(
                            &draft.patch(),
                            None,
                            Owner::Appearance,
                            "Custom native color",
                        )
                        .unwrap();
                    if !actions.is_empty() {
                        assert!(
                            action.widget.is_none(),
                            "repeated explicit choices need no metadata writes"
                        );
                    }
                    actions.push(action);
                }
                for (index, (action, (expected, wallpaper))) in actions
                    .iter()
                    .rev()
                    .zip([
                        (Some("#206040"), [0.8, 0.3, 0.2]),
                        (Some("#604020"), [0.3, 0.7, 0.4]),
                        (None, [0.6, 0.3, 0.7]),
                    ])
                    .enumerate()
                {
                    r2_wallpaper(&f, wallpaper);
                    r2_recolor(&store);
                    let reopened = f.store().with_app_config(f.0.join("config.json"));
                    let config_before = fs::read(reopened.app_config().unwrap()).unwrap();
                    let review = reopened.review_undo(&action.id).unwrap();
                    assert!(
                        review.conflicts.is_empty(),
                        "{family:?} {role:?}, reversal {index}: {:?}",
                        review.conflicts
                    );
                    reopened.accept_undo_review(&review).unwrap();
                    let reopened = f.store().with_app_config(f.0.join("config.json"));
                    let a = automatic::read_config(&reopened).unwrap().1.appearance;
                    let draft = AppearanceDraft::load(&reopened).unwrap();
                    if let Some(hex) = expected {
                        assert_eq!(
                            draft.color(variant, role).known().copied(),
                            Some(appearance::parse_hex(hex, false))
                        );
                        assert_eq!(
                            fs::read(reopened.app_config().unwrap()).unwrap(),
                            config_before,
                            "native-only restoration must not manufacture metadata writes"
                        );
                    } else {
                        assert_eq!(a.overrides, 0, "last explicit choice returns to following");
                        match role {
                            ColorRole::Background | ColorRole::Primary => {
                                assert!(themes::linked_surface(&draft, a, variant, role))
                            }
                            ColorRole::Accent => assert!(themes::linked_accent(&draft, a, variant)),
                            ColorRole::Secondary | ColorRole::Outline => {
                                assert_eq!(draft.color(variant, role).known().copied(), Some(None))
                            }
                        }
                    }
                    assert_current_generated_colors(&reopened, variant);
                    assert!(
                        automatic::prepare(&reopened).unwrap().is_none(),
                        "current derivation remains acknowledged"
                    );
                }
            }
        }
    }

    #[test]
    fn repair_r2_native_only_undo_guards_policy_but_preserves_unrelated_widget_edits() {
        use crate::desktop_theme::Family;
        for (pointer, value) in [
            ("/appearance/family", json!("paper")),
            ("/appearance/overrides", json!((1u64 << 25) | (1 << 26))),
            ("/appearance/color_policy", json!("theme")),
            ("/appearance/wallpaper_colors", json!(false)),
            ("/appearance/sync_colors", json!(false)),
            ("/clock/show_date", json!(false)),
        ] {
            let (f, store) = setup("r2-native-only-policy-guard");
            r2_wallpaper(&f, [0.2, 0.6, 0.7]);
            r2_theme(&store, Family::Soft);
            r2_recolor(&store);
            r2_custom(&store, "#604020");
            let second = r2_custom(&store, "#206040");
            r2_wallpaper(&f, [0.8, 0.3, 0.2]);
            r2_recolor(&store);
            assert!(second.widget.is_none());
            let review = store.review_undo(&second.id).unwrap();
            assert!(review.conflicts.is_empty());
            let (mut raw, _) = automatic::read_config(&store).unwrap();
            widget::put(&mut raw, pointer, Some(value.clone())).unwrap();
            fs::write(store.app_config().unwrap(), raw.to_string()).unwrap();
            let before = super::super::tests::tree(&f.0);
            if pointer.starts_with("/appearance/") {
                assert!(
                    store.accept_undo_review(&review).is_err(),
                    "stale applied intent: {pointer}"
                );
                assert_eq!(super::super::tests::tree(&f.0), before);
                assert_eq!(store.latest_change().unwrap().unwrap().id, second.id);
            } else {
                store.accept_undo_review(&review).unwrap();
                assert_eq!(
                    widget::get(&automatic::read_config(&store).unwrap().0, pointer),
                    Some(value)
                );
                assert_current_generated_colors(&store, ThemeVariant::Dark);
            }
        }
    }

    #[test]
    fn repair_r2_native_only_undo_uses_current_native_inputs_when_coordination_is_off() {
        use crate::desktop_theme::Family;
        for pointer in ["/appearance/wallpaper_colors", "/appearance/sync_colors"] {
            let (f, store) = setup("r2-native-only-coordination-off");
            r2_wallpaper(&f, [0.2, 0.6, 0.7]);
            r2_theme(&store, Family::Soft);
            r2_recolor(&store);
            r2_custom(&store, "#604020");
            let second = r2_custom(&store, "#206040");
            r2_wallpaper(&f, [0.8, 0.3, 0.2]);
            r2_recolor(&store);
            let (mut raw, _) = automatic::read_config(&store).unwrap();
            widget::put(&mut raw, pointer, Some(json!(false))).unwrap();
            if pointer == "/appearance/wallpaper_colors" {
                widget::put(&mut raw, "/appearance/color_policy", Some(json!("theme"))).unwrap();
            }
            fs::write(store.app_config().unwrap(), raw.to_string()).unwrap();
            let config_before = fs::read(store.app_config().unwrap()).unwrap();
            let reopened = f.store().with_app_config(f.0.join("config.json"));
            reopened.undo_latest(&second.id).unwrap();
            assert_eq!(
                fs::read(store.app_config().unwrap()).unwrap(),
                config_before
            );
            assert_current_generated_colors(&reopened, ThemeVariant::Dark);
        }
    }

    fn r2_wallpaper(f: &Fixture, rgb: [f32; 3]) {
        use crate::desktop::wallpaper::{BgColor, BgEntry, BgSource, NAMESPACE};
        f.user(
            NAMESPACE,
            "all",
            &ron::to_string(&BgEntry::new("all", BgSource::Color(BgColor::Single(rgb)))).unwrap(),
        );
    }
    fn r2_theme(store: &DesktopStore, family: crate::desktop_theme::Family) -> LatestChange {
        use crate::desktop::profiles::{self, Installed, themes};
        let mut profile = themes::record(family);
        themes::LookColor::Wallpaper.set(&mut profile);
        profiles::apply(
            store,
            &profiles::plan(
                store,
                &Installed::discover(store),
                &profile,
                &profile.domains().into_iter().collect(),
            ),
        )
        .unwrap()
    }
    fn r2_custom(store: &DesktopStore, hex: &str) -> LatestChange {
        use appearance::{ColorRole, Control, Value as Edit};
        let mut d = AppearanceDraft::load(store).unwrap();
        d.set(
            Control::Color(ThemeVariant::Dark, ColorRole::Background),
            Edit::Color(appearance::parse_hex(hex, false)),
        );
        store
            .apply_owned(&d.patch(), None, Owner::Appearance, "Custom background")
            .unwrap()
    }
    fn r2_recolor(store: &DesktopStore) {
        if let Some(work) = automatic::prepare(store).unwrap() {
            automatic::commit(store, work).unwrap();
        }
    }
    #[test]
    fn independent_r2_stacked_native_customizations_restore_prior_intent_and_current_wallpaper() {
        use crate::desktop::profiles::themes;
        use crate::desktop_theme::Family;
        use appearance::ColorRole;
        let (f, store) = setup("r2-stack-custom");
        r2_wallpaper(&f, [0.2, 0.6, 0.7]);
        r2_theme(&store, Family::Soft);
        r2_recolor(&store);
        let first = r2_custom(&store, "#604020");
        let second = r2_custom(&store, "#206040");
        r2_wallpaper(&f, [0.8, 0.3, 0.2]);
        r2_recolor(&store);
        let reopened = f.store().with_app_config(f.0.join("config.json"));
        let history_before_undo = reopened.history_file().unwrap();
        eprintln!(
            "first widget fields={:?}; second widget fields={:?}",
            first.widget.as_ref().map(|w| &w.fields),
            second.widget.as_ref().map(|w| &w.fields)
        );
        assert!(
            second.widget.is_none(),
            "repeated native choice has no metadata change"
        );
        let config_before = fs::read(reopened.app_config().unwrap()).unwrap();
        reopened.undo_latest(&second.id).unwrap();
        assert_eq!(
            fs::read(reopened.app_config().unwrap()).unwrap(),
            config_before
        );
        let reopened = f.store().with_app_config(f.0.join("config.json"));
        assert_eq!(
            AppearanceDraft::load(&reopened)
                .unwrap()
                .color(ThemeVariant::Dark, ColorRole::Background)
                .known()
                .copied(),
            Some(appearance::parse_hex("#604020", false))
        );
        let before = automatic::read_config(&reopened).unwrap().1.appearance;
        assert!(
            before.overrides & (1 << 25) != 0,
            "first customization remains explicit"
        );
        let review = reopened.review_undo(&first.id).unwrap();
        let key = appearance::theme::theme_key(ThemeVariant::Dark, "background");
        let actual = reopened
            .read_user(&key)
            .unwrap()
            .unwrap()
            .decode::<cosmic::cosmic_theme::Container>()
            .unwrap();
        let built = AppearanceDraft::load(&reopened)
            .unwrap()
            .builder(ThemeVariant::Dark)
            .unwrap()
            .build();
        assert_eq!(
            serde_json::to_value(&actual).unwrap(),
            serde_json::to_value(built.background(false)).unwrap(),
            "all generated background fields use the current builder"
        );
        let evidence = json!({"first_id":first.id,"second_id":second.id,
            "history_before_second_undo":history_before_undo,"history_after_second_undo":reopened.history_file().unwrap(),
            "actual_background_after_second_undo":actual,"current_builder_background":built.background(false),
            "first_undo_conflicts":review.conflicts,"automatic_work_pending":automatic::prepare(&reopened).unwrap().is_some()});
        if let Ok(dir) = std::env::var("DESKTOP_CARDS_LAYOUT_EVIDENCE") {
            let dir = std::path::PathBuf::from(dir);
            fs::create_dir_all(&dir).unwrap();
            fs::write(
                dir.join("stacked-history-evidence.json"),
                serde_json::to_string_pretty(&evidence).unwrap(),
            )
            .unwrap();
        }
        eprintln!(
            "first Undo conflicts after verified second Undo: {:?}",
            review.conflicts
        );
        reopened.undo_latest(&first.id).unwrap();
        for next in [None, Some([0.3, 0.7, 0.2])] {
            if let Some(rgb) = next {
                r2_wallpaper(&f, rgb);
                r2_recolor(&reopened);
            }
            let a = automatic::read_config(&reopened).unwrap().1.appearance;
            let d = AppearanceDraft::load(&reopened).unwrap();
            assert_eq!(a.overrides & (1 << 25), 0);
            assert!(themes::linked_surface(
                &d,
                a,
                ThemeVariant::Dark,
                ColorRole::Background
            ));
        }
    }
    #[test]
    fn independent_r2_native_external_conflicts_survive_recolor_and_partial_or_kept_undo() {
        use crate::desktop_theme::Family;
        use appearance::{ColorRole, Control, Value as Edit};
        for keep in [false, true] {
            let (f, store) = setup("r2-native-conflict");
            r2_wallpaper(&f, [0.2, 0.6, 0.7]);
            let first = r2_theme(&store, Family::Soft);
            r2_recolor(&store);
            let second = r2_custom(&store, "#604020");
            let clock = widget_action(&store, "/clock/show_date", json!(false), Source::Manual);
            let mut outside = AppearanceDraft::load(&store).unwrap();
            outside.set(
                Control::Color(ThemeVariant::Dark, ColorRole::Background),
                Edit::Color(appearance::parse_hex("#123456", false)),
            );
            for (key, entry) in outside.patch().iter() {
                if let Some(raw) = &entry.desired {
                    let p = store.user_path(key);
                    fs::create_dir_all(p.parent().unwrap()).unwrap();
                    fs::write(p, raw.as_str()).unwrap();
                }
            }
            r2_wallpaper(&f, [0.8, 0.3, 0.2]);
            r2_recolor(&store);
            store.undo_latest(&clock.id).unwrap();
            let unchanged = f.tree();
            assert!(store.undo_latest(&second.id).is_err());
            assert_eq!(f.tree(), unchanged);
            let review = store.review_undo(&second.id).unwrap();
            assert!(!review.conflicts.is_empty());
            if keep {
                store.keep_undo_current(&review).unwrap();
            } else {
                store.accept_undo_review(&review).unwrap();
            }
            r2_wallpaper(&f, [0.3, 0.7, 0.4]);
            r2_recolor(&store);
            let reopened = f.store().with_app_config(f.0.join("config.json"));
            assert!(
                !reopened
                    .review_undo(&first.id)
                    .unwrap()
                    .conflicts
                    .is_empty(),
                "outside native color must not be attributed to older theme after keep={keep}"
            );
            assert!(reopened.undo_latest(&first.id).is_err());
            assert_eq!(
                AppearanceDraft::load(&reopened)
                    .unwrap()
                    .color(ThemeVariant::Dark, ColorRole::Background)
                    .known()
                    .copied(),
                Some(appearance::parse_hex("#123456", false))
            );
        }
    }
    #[test]
    fn independent_r2_semantic_undo_review_rejects_later_wallpaper_or_same_role_writer() {
        use crate::desktop_theme::Family;
        for wallpaper_changed in [false, true] {
            let (f, store) = setup("r2-undo-review-race");
            r2_wallpaper(&f, [0.2, 0.6, 0.7]);
            r2_theme(&store, Family::Soft);
            r2_recolor(&store);
            let action = r2_custom(&store, "#604020");
            r2_wallpaper(&f, [0.8, 0.3, 0.2]);
            r2_recolor(&store);
            let review = store.review_undo(&action.id).unwrap();
            assert!(review.conflicts.is_empty());
            if wallpaper_changed {
                r2_wallpaper(&f, [0.3, 0.7, 0.4]);
            } else {
                let key = appearance::theme::builder_key(ThemeVariant::Dark, "bg_color");
                fs::write(store.user_path(&key), "Some(\"#123456\")").unwrap();
            }
            let unchanged = f.tree();
            assert!(store.accept_undo_review(&review).is_err());
            assert_eq!(f.tree(), unchanged, "stale review writes nothing");
            assert_eq!(store.latest_change().unwrap().unwrap().id, action.id);
        }
    }
    #[test]
    fn independent_r2_shared_native_container_conflict_is_scoped_and_guarded() {
        use crate::desktop_theme::Family;
        use appearance::{ColorRole, Control, Value as Edit};
        let (f, store) = setup("r2-shared-native");
        r2_wallpaper(&f, [0.2, 0.6, 0.7]);
        r2_theme(&store, Family::Soft);
        r2_recolor(&store);
        let action = r2_custom(&store, "#604020");
        let key = appearance::theme::theme_key(ThemeVariant::Dark, "transparent_primary");
        assert!(
            action.change.entries().iter().any(|entry| entry.key == key),
            "background action changes this generated container"
        );
        let mut outside = AppearanceDraft::load(&store).unwrap();
        outside.set(
            Control::Color(ThemeVariant::Dark, ColorRole::Primary),
            Edit::Color(appearance::parse_hex("#4E3B59", false)),
        );
        assert!(
            outside.patch().keys().any(|k| *k == key),
            "outside primary edit changes the same generated container"
        );
        for (key, entry) in outside.patch().iter() {
            if let Some(raw) = &entry.desired {
                let p = store.user_path(key);
                fs::create_dir_all(p.parent().unwrap()).unwrap();
                fs::write(p, raw.as_str()).unwrap();
            }
        }
        r2_wallpaper(&f, [0.8, 0.3, 0.2]);
        r2_recolor(&store);
        let retained = store.read_user(&key).unwrap();
        let unchanged = f.tree();
        assert!(store.undo_latest(&action.id).is_err());
        assert_eq!(
            f.tree(),
            unchanged,
            "unreviewed Undo writes nothing on shared native conflict"
        );
        let review = store.review_undo(&action.id).unwrap();
        // Named by its page, never by its config key.
        assert!(
            review
                .conflicts
                .iter()
                .any(|v| v.starts_with("Appearance · a setting")),
            "{:?}",
            review.conflicts
        );
        assert!(
            !review
                .conflicts
                .iter()
                .any(|v| v.contains("transparent_primary"))
        );
        store.accept_undo_review(&review).unwrap();
        assert_eq!(
            store.read_user(&key).unwrap(),
            retained,
            "partial reversal retains the conflicting generated value"
        );
        assert_eq!(
            AppearanceDraft::load(&store)
                .unwrap()
                .color(ThemeVariant::Dark, ColorRole::Primary)
                .known()
                .copied(),
            Some(appearance::parse_hex("#4E3B59", false)),
            "explicit outside primary survives"
        );
    }
}
