//! Completed settings actions share a guarded transaction and durable history.
//! Domain drafts are transient semantic builders; they never own a page queue.
use super::store::{
    DesktopError, DesktopPatch, DesktopStore, LatestChange, Owner, Source, WidgetPatch, widget,
};
use crate::config;
use serde_json::Value;
use std::cell::RefCell;

thread_local! { static EPOCH: std::cell::Cell<Option<u64>> = const { std::cell::Cell::new(None) }; }
thread_local! { static SCHEDULE: RefCell<Option<Option<String>>> = const { RefCell::new(None) }; }
thread_local! { static GUARD: RefCell<Option<Value>> = const { RefCell::new(None) }; }
thread_local! { static PROPERTY: std::cell::Cell<u64> = const { std::cell::Cell::new(0) }; }
thread_local! { static ACTION: RefCell<Option<String>> = const { RefCell::new(None) }; }

thread_local! { static RECEIPT: RefCell<Option<Vec<LatestChange>>> = const { RefCell::new(None) }; }

/// Keep the exact transactions completed by this dispatch, even if a subsequent
/// read fails. Reading the shared latest-history file could claim another
/// process's newer action as part of this arrangement session.
pub(crate) fn with_receipts<T>(run: impl FnOnce() -> T) -> (T, Vec<LatestChange>) {
    struct Restore(Option<Vec<LatestChange>>);
    impl Drop for Restore {
        fn drop(&mut self) {
            RECEIPT.with(|slot| *slot.borrow_mut() = self.0.take());
        }
    }
    let _restore = Restore(RECEIPT.with(|slot| slot.replace(Some(Vec::new()))));
    let result = run();
    let receipts = RECEIPT.with(|slot| slot.borrow_mut().take().unwrap_or_default());
    (result, receipts)
}

/// The scope lasts for a single synchronous UI dispatch. Background jobs use
/// explicit Source values and cannot inherit an idle window's action.
pub fn in_action<T>(label: &str, run: impl FnOnce() -> T) -> T {
    struct Restore(Option<String>);
    impl Drop for Restore {
        fn drop(&mut self) {
            ACTION.with(|slot| *slot.borrow_mut() = self.0.take());
        }
    }
    let _restore =
        Restore(ACTION.with(|slot| slot.replace(Some(label.chars().take(120).collect()))));
    run()
}
pub fn guarded<T>(baseline: Option<&Value>, run: impl FnOnce() -> T) -> T {
    struct Restore(Option<Value>);
    impl Drop for Restore {
        fn drop(&mut self) {
            GUARD.with(|slot| *slot.borrow_mut() = self.0.take());
        }
    }
    let _restore = Restore(GUARD.with(|slot| slot.replace(baseline.cloned())));
    run()
}
pub fn scheduled<T>(expected: Option<String>, run: impl FnOnce() -> T) -> T {
    struct Restore(Option<Option<String>>);
    impl Drop for Restore {
        fn drop(&mut self) {
            SCHEDULE.with(|slot| *slot.borrow_mut() = self.0.take());
        }
    }
    let _restore = Restore(SCHEDULE.with(|slot| slot.replace(Some(expected))));
    run()
}
pub fn scheduled_epoch<T>(epoch: u64, expected: Option<String>, run: impl FnOnce() -> T) -> T {
    struct Reset(Option<u64>);
    impl Drop for Reset {
        fn drop(&mut self) {
            EPOCH.with(|v| v.set(self.0));
        }
    }
    let _reset = Reset(EPOCH.with(|v| v.replace(Some(epoch))));
    scheduled(expected, run)
}
pub fn property<T>(mask: u64, run: impl FnOnce() -> T) -> T {
    struct Restore(u64);
    impl Drop for Restore {
        fn drop(&mut self) {
            PROPERTY.with(|v| v.set(self.0));
        }
    }
    let _restore = Restore(PROPERTY.with(|v| v.replace(mask)));
    run()
}
pub fn complete_property(mask: u64, label: &str) -> Result<(), String> {
    if mask == 0 {
        return Ok(());
    }
    let store = history_store().map_err(|e| e.to_string())?;
    let (raw, config) = super::wallpaper::automatic::read_config(&store)?;
    if config.appearance.overrides & mask == mask {
        return Ok(());
    }
    let mut widgets = WidgetPatch::new();
    widgets.set(
        "/appearance/overrides",
        widget::get(&raw, "/appearance/overrides"),
        Some((config.appearance.overrides | mask).into()),
    )?;
    store
        .apply_action(
            &DesktopPatch::new(),
            Some(&widgets),
            Owner::Preferences,
            label,
            Source::Manual,
        )
        .map_err(|e| e.to_string())?;
    Ok(())
}
pub fn current_label() -> Option<String> {
    ACTION.with(|slot| slot.borrow().clone())
}

pub fn commit(
    store: &DesktopStore,
    patch: &DesktopPatch,
    widgets: Option<&WidgetPatch>,
    owner: Owner,
    label: &str,
    source: Source,
) -> Result<LatestChange, DesktopError> {
    let mut widgets = widgets.cloned().unwrap_or_default();
    if source == Source::Manual
        && matches!(
            owner,
            Owner::DockPanel | Owner::Appearance | Owner::Preferences
        )
        && store.app_config().is_some()
    {
        let (raw, config) =
            super::wallpaper::automatic::read_config(store).map_err(DesktopError::Io)?;
        let mut overrides = config.appearance.overrides | PROPERTY.with(|v| v.get());
        for (key, entry) in patch.iter() {
            let bit = override_for_native(key);
            let desired = entry.desired.as_ref().map(|raw| raw.as_str().trim());
            // A bar background of Match style follows the look's style, so
            // choosing it is not a choice a style must keep.
            let follows_style = key.key() == "background" && desired == Some("ThemeDefault");
            if bit != 0
                && (follows_style
                    || (super::appearance::ThemeVariant::of(key).is_some()
                        && desired.is_none_or(|raw| raw == "None")))
            {
                overrides &= !bit;
            } else {
                overrides |= bit;
            }
        }
        for (pointer, entry) in widgets.changes() {
            if pointer == "/appearance/surface" && entry.desired.as_ref().is_none_or(Value::is_null)
            {
                overrides &= !override_for_widget(pointer);
            } else {
                overrides |= override_for_widget(pointer);
            }
        }
        if overrides != config.appearance.overrides {
            widgets
                .set(
                    "/appearance/overrides",
                    widget::get(&raw, "/appearance/overrides"),
                    Some(overrides.into()),
                )
                .map_err(DesktopError::Io)?;
        }
    }
    let mut patch = patch.clone();
    if source == Source::Scheduled {
        // Missing scope is a programming error, not permission to overwrite a
        // newer manual choice. Some(None) is the valid empty-history baseline.
        patch.manual_guard =
            Some(SCHEDULE.with(|slot| slot.borrow().clone()).ok_or_else(|| {
                DesktopError::InvalidValue {
                    key: None,
                    message: "Scheduled action lacks its manual-change guard".into(),
                }
            })?);
        patch.epoch_guard =
            Some(
                EPOCH
                    .with(|v| v.get())
                    .ok_or_else(|| DesktopError::InvalidValue {
                        key: None,
                        message: "Scheduled action lacks its Restore generation".into(),
                    })?,
            );
    }
    let label = if source == Source::Manual {
        action_label(&patch, owner, label)
    } else {
        label.into()
    };
    store.apply_action(&patch, Some(&widgets), owner, &label, source)
}

fn action_label(patch: &DesktopPatch, owner: Owner, fallback: &str) -> String {
    let input: Vec<_> = patch
        .keys()
        .filter(|key| !matches!(super::appearance::ThemeVariant::of(key), Some((_, false))))
        .collect();
    if input.len() == 1 && owner == Owner::DockPanel {
        let key = input[0];
        let bar = if key.namespace().ends_with(".Dock") {
            "Dock"
        } else {
            "Panel"
        };
        let name = match key.key() {
            "opacity" => Some("opacity"),
            "anchor" => Some("position"),
            "size" => Some("size"),
            "padding" => Some("padding"),
            "spacing" => Some("spacing"),
            "border_radius" => Some("corners"),
            // Other keys (Floating, Shape, …) take the control's name below.
            _ => None,
        };
        if let Some(name) = name {
            return format!("{bar} {name}");
        }
    }
    // One control can write several keys (Floating sets the edge gap too);
    // the control that was used names it.
    if owner == Owner::DockPanel
        && let Some(label) =
            current_label().filter(|label| !matches!(label.as_str(), "Settings" | "Dock and panel"))
    {
        return label;
    }
    if owner == Owner::Appearance {
        if let Some(label) = current_label().filter(|label| label != "Settings") {
            return label;
        }
    }
    fallback.into()
}

/// Stable property bits, deliberately separate from inferred external edits.
/// Equal RGB or geometry is still an explicit choice until that property resets.
pub fn override_for_widget(pointer: &str) -> u64 {
    match pointer {
        "/appearance/opacity" => 1,
        "/appearance/corner_radius" => 1 << 1,
        "/appearance/border_opacity" => 1 << 2,
        "/appearance/surface" => 1 << 3,
        "/appearance/contrast_assist" => 1 << 4,
        _ => 0,
    }
}
pub fn override_for_native(key: &super::ConfigKey) -> u64 {
    let bar = if key.namespace() == "com.system76.CosmicPanel.Dock" {
        Some(8)
    } else if key.namespace() == "com.system76.CosmicPanel.Panel" {
        Some(16)
    } else {
        None
    };
    if let Some(start) = bar {
        return match key.key() {
            "opacity" => 1 << start,
            "border_radius" => 1 << (start + 1),
            "padding" => 1 << (start + 2),
            "spacing" => 1 << (start + 3),
            "background" => 1 << (start + 4),
            _ => 0,
        };
    }
    if let Some((variant, true)) = super::appearance::ThemeVariant::of(key) {
        let start = if variant == super::appearance::ThemeVariant::Dark {
            24
        } else {
            32
        };
        return match key.key() {
            "accent" => 1 << start,
            "bg_color" => 1 << (start + 1),
            "primary_container_bg" => 1 << (start + 2),
            "secondary_container_bg" => 1 << (start + 3),
            "window_hint" => 1 << (start + 4),
            _ => 0,
        };
    }
    0
}

/// Named fields, not a whole config replacement. Validation remains in both
/// the existing helper and the transaction participant. Unknown members stay.
pub fn preference_patch(before: &Value, after: &Value) -> Result<WidgetPatch, String> {
    fn diff(
        path: &str,
        before: Option<&Value>,
        after: Option<&Value>,
        patch: &mut WidgetPatch,
    ) -> Result<(), String> {
        if before == after {
            return Ok(());
        }
        if widget::field(path).is_some() {
            return patch.set(path, before.cloned(), after.cloned());
        }
        let old = before.and_then(Value::as_object);
        let new = after.and_then(Value::as_object);
        if old.is_some() || new.is_some() {
            let keys: std::collections::BTreeSet<_> = old
                .into_iter()
                .flat_map(|v| v.keys())
                .chain(new.into_iter().flat_map(|v| v.keys()))
                .collect();
            for key in keys {
                diff(
                    &format!("{path}/{key}"),
                    old.and_then(|v| v.get(key)),
                    new.and_then(|v| v.get(key)),
                    patch,
                )?;
            }
            Ok(())
        } else {
            Err(format!("This action cannot change {path}"))
        }
    }
    let mut patch = WidgetPatch::new();
    diff("", Some(before), Some(after), &mut patch)?;
    Ok(patch)
}

pub fn preferences(
    path: &std::path::Path,
    label: &str,
    change: impl FnOnce(&mut Value) -> Result<(), String>,
) -> Result<Value, String> {
    preferences_guarded(path, label, None, change)
}

/// A read dependency participates in the store's commit-time conflict check,
/// but is not owned or restored by this action's Undo.
pub(crate) fn preferences_guarded(
    path: &std::path::Path,
    label: &str,
    guard: Option<(&str, Option<Value>)>,
    change: impl FnOnce(&mut Value) -> Result<(), String>,
) -> Result<Value, String> {
    let before = config::parse_config_text(config::read_config_text(path)?.as_deref())?;
    let mut after = before.clone();
    change(&mut after)?;
    config::check_config_value(&after)?;
    let mut patch = preference_patch(&before, &after)?;
    if patch.is_empty() {
        complete_property(PROPERTY.with(|v| v.get()), label)?;
        return config::parse_config_text(config::read_config_text(path)?.as_deref());
    }
    GUARD.with(|slot| {
        if let Some(baseline)=slot.borrow().as_ref() {
            for (pointer,entry) in patch.changes() {
                if widget::get(baseline,pointer) != entry.original { return Err(format!("{} changed elsewhere during this gesture. Try the control again to use the current value.",widget::label(pointer))); }
            }
        }
        Ok(())
    })?;
    if let Some((pointer, expected)) = guard {
        patch.set(pointer, expected.clone(), expected)?;
    }
    let store = preference_store(path).map_err(|error| error.to_string())?;
    let receipt = commit(
        &store,
        &DesktopPatch::new(),
        Some(&patch),
        Owner::Preferences,
        label,
        Source::Manual,
    )
    .map_err(|error| error.to_string())?;
    RECEIPT.with(|slot| {
        if let Some(receipts) = slot.borrow_mut().as_mut() {
            receipts.push(receipt);
        }
    });
    #[cfg(test)]
    if let Some(after) =
        crate::test_env::AFTER_PREFERENCE_COMMIT.with(|slot| slot.borrow_mut().take())
    {
        after();
    }
    config::parse_config_text(config::read_config_text(path)?.as_deref())
}

/// A compound action has one native appearance builder, so disjoint recovered
/// color/font/gap edits never concatenate independently generated COSMIC themes.
pub struct Composition {
    pub native: DesktopPatch,
    pub widgets: WidgetPatch,
    snapshot: super::Snapshot,
    pub relink_outline: std::collections::BTreeSet<super::appearance::ThemeVariant>,
}
impl Composition {
    pub fn new(store: &DesktopStore) -> Result<Self, DesktopError> {
        Ok(Self {
            native: DesktopPatch::new(),
            widgets: WidgetPatch::new(),
            relink_outline: Default::default(),
            snapshot: store.load(&super::appearance::AppearanceDraft::keys())?,
        })
    }
    pub fn appearance(
        &mut self,
        store: &DesktopStore,
        mut proposal: super::appearance::AppearanceDraft,
        use_proposed: bool,
    ) -> Result<(), String> {
        if !use_proposed {
            proposal.refresh(
                store
                    .load(&super::appearance::AppearanceDraft::keys())
                    .map_err(|e| e.to_string())?,
            );
            if !proposal.conflicts().is_empty() {
                return Err(format!(
                    "Changed elsewhere: {}",
                    proposal
                        .conflicts()
                        .iter()
                        .map(ToString::to_string)
                        .collect::<Vec<_>>()
                        .join(", ")
                ));
            }
        }
        let plan = proposal.plan();
        if let Some(blocked) = plan.blocked.first() {
            return Err(blocked.reason.clone());
        }
        self.patch(store, plan.patch, use_proposed)
    }
    pub fn patch(
        &mut self,
        store: &DesktopStore,
        patch: DesktopPatch,
        use_proposed: bool,
    ) -> Result<(), String> {
        let mut candidate = DesktopPatch::new();
        for (key, entry) in patch.iter() {
            // COSMIC's generated theme is composed once from all selected
            // builder inputs. Independent proposals never merge generated files.
            if let Some((variant, false)) = super::appearance::ThemeVariant::of(key) {
                // Explicit relinking can change only a derived outline while
                // its builder already says None. Keep that semantic operation.
                if key.key() == "window_hint"
                    && entry
                        .desired
                        .as_ref()
                        .is_none_or(|v| v.as_str().trim() == "None")
                {
                    let current = store.read_user(key).map_err(|e| e.to_string())?;
                    if !use_proposed && current != entry.original && current != entry.desired {
                        return Err(
                            "The window outline changed elsewhere. Review the recovered choice."
                                .into(),
                        );
                    }
                    let builder = super::appearance::theme::builder_key(variant, "window_hint");
                    if self.native.iter().any(|(k, e)| {
                        *k == builder
                            && e.desired
                                .as_ref()
                                .is_some_and(|v| v.as_str().trim() != "None")
                    }) {
                        return Err("Recovered choices disagree about the window outline".into());
                    }
                    self.relink_outline.insert(variant);
                    candidate.guard(key.clone(), current);
                }
                continue;
            }
            if let Some((variant, true)) = super::appearance::ThemeVariant::of(key) {
                if key.key() == "window_hint"
                    && self.relink_outline.contains(&variant)
                    && entry
                        .desired
                        .as_ref()
                        .is_some_and(|v| v.as_str().trim() != "None")
                {
                    return Err("Recovered choices disagree about the window outline".into());
                }
            }
            let current = store.read_user(key).map_err(|e| e.to_string())?;
            if !use_proposed && current != entry.original && current != entry.desired {
                return Err(format!(
                    "{} changed elsewhere. Choose current or recovered values.",
                    key
                ));
            }
            candidate.set(key.clone(), current, entry.desired.clone());
        }
        for (key, original) in patch.guards() {
            if super::appearance::AppearanceDraft::keys().contains(key) {
                continue;
            }
            let current = store.read_user(key).map_err(|e| e.to_string())?;
            if !use_proposed && current != *original {
                return Err(format!("{} changed while this work was saved", key));
            }
            candidate.guard(key.clone(), current);
        }
        self.native
            .merge(candidate)
            .map_err(|e| format!("Conflicting recovered choices: {e}. Select one choice."))
    }
    pub fn widgets(
        &mut self,
        store: &DesktopStore,
        patch: WidgetPatch,
        use_proposed: bool,
    ) -> Result<(), String> {
        let raw = super::wallpaper::automatic::read_config(store)?.0;
        let mut candidate = WidgetPatch::new();
        for (pointer, entry) in patch.iter() {
            let current = widget::get(&raw, pointer);
            if !use_proposed && current != entry.original && current != entry.desired {
                return Err(format!(
                    "{} changed elsewhere. Choose current or recovered values.",
                    widget::label(pointer)
                ));
            }
            candidate.set(pointer, current, entry.desired.clone())?;
        }
        self.widgets.merge(candidate).map_err(|fields| {
            format!(
                "Two recovered choices disagree about {}. Select one choice.",
                fields.join(", ")
            )
        })
    }
    pub fn finish(self) -> Result<(DesktopPatch, WidgetPatch), String> {
        use super::appearance::{
            AppearanceDraft, Control, ThemeVariant, Value as Edit, VariantValues,
        };
        let mut next = self.snapshot.clone();
        for (key, entry) in self.native.iter() {
            if let Some(mut value) = next.entry(key).cloned() {
                value.user = entry.desired.clone();
                next.insert(key.clone(), value);
            }
        }
        let desired = AppearanceDraft::from_snapshot(next);
        let mut draft = AppearanceDraft::from_snapshot(self.snapshot);
        for variant in ThemeVariant::ALL {
            if self
                .native
                .keys()
                .any(|key| ThemeVariant::of(key) == Some((variant, true)))
            {
                let builder = desired.base_builder(variant).map_err(|e| e.reason)?;
                draft.set(
                    Control::Variant(variant),
                    Edit::Variant(Box::new(VariantValues::of(&builder))),
                );
            }
        }
        for variant in self.relink_outline {
            draft.set(
                Control::Color(variant, super::appearance::ColorRole::Outline),
                Edit::Color(None),
            );
        }
        let generated = draft.plan();
        if let Some(blocked) = generated.blocked.first() {
            return Err(blocked.reason.clone());
        }
        let mut native = self.native;
        // Regeneration carries guarded reads of untouched builder inputs and
        // conditional native-role preservation from the existing theme engine.
        native.extend(generated.patch);
        Ok((native, self.widgets))
    }
}

/// Alternate widget configs can be edited while native COSMIC remains read-only.
/// Their history is confined beside that explicit config, never in live state.
pub fn preference_store(path: &std::path::Path) -> Result<DesktopStore, DesktopError> {
    let store = DesktopStore::from_env()?;
    if store.writable() {
        return Ok(store.with_app_config(path.to_path_buf()));
    }
    let root = path
        .parent()
        .filter(|p| p.is_absolute())
        .ok_or_else(|| DesktopError::Io("An absolute configuration path is required".into()))?;
    Ok(DesktopStore::with_roots(
        root.join(".desktop-cards-history/native"),
        Vec::new(),
        root.join(".desktop-cards-history"),
        super::store::Access::Fixture(root.to_path_buf()),
    )
    .with_app_config(path.to_path_buf()))
}
pub fn history_store() -> Result<DesktopStore, DesktopError> {
    let path = config::Config::path()
        .ok_or_else(|| DesktopError::Io("Configuration directory unavailable".into()))?;
    preference_store(&path)
}

/// Revert only fields touched since an arrangement session started. Every field
/// still has to match its last completed gesture; unrelated edits are retained.
pub fn cancel_arrangement(actions: &[LatestChange]) -> Result<(), String> {
    let store = history_store().map_err(|e| e.to_string())?;
    let mut fields = std::collections::BTreeMap::new();
    for action in actions {
        for field in action.widget.iter().flat_map(|w| &w.fields) {
            fields
                .entry(field.pointer.clone())
                .and_modify(|old: &mut super::store::widget::WidgetFieldChange| {
                    old.after = field.after.clone()
                })
                .or_insert_with(|| field.clone());
        }
    }
    let mut patch = WidgetPatch::new();
    for (pointer, field) in fields {
        patch.set(&pointer, field.after, field.before)?;
    }
    if !patch.is_empty() {
        store
            .apply_action(
                &DesktopPatch::new(),
                Some(&patch),
                Owner::Preferences,
                "Cancel arrangement",
                Source::Manual,
            )
            .map_err(|e| e.to_string())?;
    }
    Ok(())
}

#[cfg(test)]
mod architecture_tests {
    use super::*;
    use crate::desktop::store::tests::Fixture;
    #[test]
    fn scheduled_commit_requires_both_guards_and_accepts_empty_history() {
        let fixture = Fixture::new("scheduled-action-guards");
        let config = fixture.0.join("config.json");
        std::fs::write(&config, b"{}").unwrap();
        let store = fixture.store().with_app_config(config);
        let mut widgets = WidgetPatch::new();
        widgets
            .set("/clock/show_date", None, Some(Value::Bool(false)))
            .unwrap();
        let write = || {
            commit(
                &store,
                &DesktopPatch::new(),
                Some(&widgets),
                Owner::Preferences,
                "Scheduled fixture",
                Source::Scheduled,
            )
        };
        let before = fixture.tree();
        assert!(matches!(write(), Err(DesktopError::InvalidValue { .. })));
        assert_eq!(fixture.tree(), before);
        assert!(matches!(
            scheduled(None, write),
            Err(DesktopError::InvalidValue { .. })
        ));
        assert_eq!(fixture.tree(), before);
        EPOCH.with(|slot| slot.set(Some(0)));
        let epoch_only = write();
        EPOCH.with(|slot| slot.set(None));
        assert!(matches!(epoch_only, Err(DesktopError::InvalidValue { .. })));
        assert_eq!(fixture.tree(), before);
        assert!(store.manual_at().is_none());
        scheduled_epoch(store.restore_epoch().unwrap(), None, write).unwrap();
        assert_eq!(
            serde_json::from_slice::<Value>(&std::fs::read(store.app_config().unwrap()).unwrap())
                .unwrap()["clock"]["show_date"],
            false
        );
        // Both scopes must be cleared once the admitted operation finishes.
        assert!(matches!(write(), Err(DesktopError::InvalidValue { .. })));
    }
}
