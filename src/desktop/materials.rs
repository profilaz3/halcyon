//! One semantic action: material selection and its native bar records, no color writes.
use super::store::{
    ConfigKey, DesktopPatch, DesktopStore, KeyRule, Namespace, Owner, RawValue, Source,
    WidgetPatch, widget,
};
use crate::materials::{Finish, Target};
use dock_islands_core::material::{BarMaterial, NAMESPACE};
pub fn key(panel: &str) -> ConfigKey {
    ConfigKey::new(NAMESPACE, 1, panel).expect("material key")
}
fn validate(raw: &RawValue) -> Result<(), String> {
    raw.decode::<BarMaterial>()?.validate()
}
pub static RULES: [KeyRule; 2] = [
    KeyRule {
        namespace: Namespace::Exact(NAMESPACE),
        version: 1,
        key: "Dock",
        validate,
    },
    KeyRule {
        namespace: Namespace::Exact(NAMESPACE),
        version: 1,
        key: "Panel",
        validate,
    },
];
#[cfg(test)]
pub fn apply(store: &DesktopStore, target: Target, finish: Option<Finish>) -> Result<(), String> {
    apply_from(store, target, finish, None)
}
pub fn apply_from(
    store: &DesktopStore,
    target: Target,
    finish: Option<Finish>,
    expected: Option<crate::materials::Materials>,
) -> Result<(), String> {
    let installed = super::islands::Installation::discover(store).installed();
    if matches!(target, Target::Dock | Target::Panel) && !installed {
        return Err("The Islands panel isn’t installed".into());
    }
    if let Some(f) = finish {
        f.validate()?;
    }
    let (raw, config) = super::wallpaper::automatic::read_config(store)?;
    if expected.is_some_and(|base| {
        base.get(target) != config.materials.get(target)
            || base.effective(target) != config.materials.effective(target)
    }) {
        return Err(
            "This material changed elsewhere. Try the control again to use the current value."
                .into(),
        );
    }
    let mut next = config.materials;
    next.set(target, finish);
    let mut widgets = WidgetPatch::new();
    widgets.set(
        target.pointer(),
        widget::get(&raw, target.pointer()),
        Some(serde_json::to_value(finish).map_err(|e| e.to_string())?),
    )?;
    // Guard the inheritance decisions alongside the changed member.
    for other in Target::ALL.into_iter().filter(|other| *other != target) {
        let value = widget::get(&raw, other.pointer());
        widgets.set(other.pointer(), value.clone(), value)?;
    }
    let mut patch = DesktopPatch::new();
    for (bar, name) in [(Target::Dock, "Dock"), (Target::Panel, "Panel")] {
        if !installed {
            continue;
        }
        if target != Target::Shared && target != bar {
            continue;
        }
        // An explicit surface override survives a change to the shared material.
        if target == Target::Shared && next.get(bar).is_some() {
            continue;
        }
        let key = key(name);
        let base = store.read_user(&key).map_err(|e| e.to_string())?;
        if let Some(base) = &base {
            validate(base)?;
        } // preserve unknown future records
        let desired = next
            .effective(bar)
            .map(|finish| RawValue::encode(&BarMaterial::new(finish)))
            .transpose()
            .map_err(|e| e.to_string())?;
        patch.set(key, base, desired);
    }
    if patch.is_empty() && widgets.is_empty() {
        return Ok(());
    }
    super::actions::commit(
        store,
        &patch,
        Some(&widgets),
        Owner::Preferences,
        if finish.is_some() {
            "Glass material"
        } else {
            "Reset material"
        },
        Source::Manual,
    )
    .map_err(|e| e.to_string())?;
    Ok(())
}
/// A separate optional request leaves protocol-1 Capabilities unchanged for older clients.
pub fn native_support(store: &DesktopStore) -> bool {
    super::islands::Client::connect(store)
        .and_then(|mut c| c.request(&dock_islands_core::Request::MaterialSupport))
        .is_ok_and(|r| {
            matches!(
                r,
                dock_islands_core::Response::MaterialSupport { version: 1 }
            )
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::materials::Kind;
    #[test]
    fn shared_material_undo_preserves_later_colors_and_individual_overrides() {
        let root =
            std::env::temp_dir().join(format!("material-transaction-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        crate::desktop::islands::fixture_installed(&root);
        let path = root.join("widgets.json");
        std::fs::write(&path, r#"{"appearance":{"accent":[21,100,150],"family":"paper","opacity":89},"glance":{"style":"transparent"},"widget_font":"Open Sans","future":{"keep":true}}"#).unwrap();
        let store = DesktopStore::fixture(&root)
            .unwrap()
            .with_app_config(path.clone());
        let stale = crate::materials::Materials::default();
        apply(&store, Target::Shared, Some(Finish::new(Kind::Frosted))).unwrap();
        assert!(
            apply_from(
                &store,
                Target::Shared,
                Some(Finish::new(Kind::Clear)),
                Some(stale)
            )
            .is_err()
        );
        apply(&store, Target::Shared, None).unwrap();
        apply(&store, Target::Dock, Some(Finish::new(Kind::Smoked))).unwrap();
        apply(&store, Target::Shared, Some(Finish::new(Kind::Liquid))).unwrap();
        assert_eq!(
            store
                .read_user(&key("Dock"))
                .unwrap()
                .unwrap()
                .decode::<BarMaterial>()
                .unwrap()
                .finish
                .kind,
            Kind::Smoked
        );
        assert_eq!(
            store
                .read_user(&key("Panel"))
                .unwrap()
                .unwrap()
                .decode::<BarMaterial>()
                .unwrap()
                .finish
                .kind,
            Kind::Liquid
        );
        let mut raw: serde_json::Value =
            serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
        assert_eq!(raw["appearance"]["opacity"], 89);
        assert_eq!(raw["glance"]["style"], "transparent");
        raw["appearance"]["accent"] = serde_json::json!([180, 40, 120]);
        std::fs::write(&path, serde_json::to_vec(&raw).unwrap()).unwrap();
        let change = store.latest_change().unwrap().unwrap();
        store.undo_latest(&change.id).unwrap();
        let raw: serde_json::Value =
            serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
        assert_eq!(
            raw["appearance"]["accent"],
            serde_json::json!([180, 40, 120])
        );
        assert!(raw["materials"]["shared"].is_null());
        assert_eq!(raw["materials"]["dock"]["kind"], "smoked");
        assert!(store.read_user(&key("Panel")).unwrap().is_none());
        assert_eq!(raw["future"]["keep"], true);
        std::fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn unknown_bar_version_blocks_without_overwriting_preferences() {
        let root = std::env::temp_dir().join(format!("material-future-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        crate::desktop::islands::fixture_installed(&root);
        let path = root.join("widgets.json");
        std::fs::write(&path, "{}").unwrap();
        let store = DesktopStore::fixture(&root)
            .unwrap()
            .with_app_config(path.clone());
        let bar = store.user_path(&key("Dock"));
        std::fs::create_dir_all(bar.parent().unwrap()).unwrap();
        std::fs::write(
            &bar,
            "(version:99,finish:(kind:liquid,opacity:None,tint:35))",
        )
        .unwrap();
        assert!(apply(&store, Target::Shared, Some(Finish::new(Kind::Frosted))).is_err());
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "{}");
        assert!(std::fs::read_to_string(bar).unwrap().contains("version:99"));
        std::fs::remove_dir_all(root).unwrap();
    }
}
