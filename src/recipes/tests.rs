use super::*;
#[test]
fn fb17_recipes_are_bounded_typed_and_have_no_executable_bindings() {
    for kind in Kind::ALL {
        let r = Recipe::starter(kind);
        r.validate().unwrap();
        let mut v = serde_json::to_value(&r).unwrap();
        v["command"] = "sh".into();
        assert!(Recipe::parse(&serde_json::to_vec(&v).unwrap()).is_err());
        let mut invalid = r.clone();
        invalid.parts.push(kind.primary());
        assert!(invalid.validate().is_err());
        invalid = r.clone();
        invalid.parts.clear();
        assert!(invalid.validate().is_err());
        invalid = r.clone();
        invalid.version = 99;
        assert!(invalid.validate().is_err());
        invalid = r.clone();
        invalid.caption = "x".repeat(121);
        assert!(invalid.validate().is_err());
        invalid = r.clone();
        invalid.parts.push(if kind == Kind::Clock {
            Part::Reading
        } else {
            Part::Face
        });
        assert!(invalid.validate().is_err());
    }
    assert!(Recipe::parse(&vec![b' '; MAX_BYTES + 1]).is_err());
}
#[test]
fn fb17_recipe_library_guards_revisions_and_keeps_applied_copies() {
    let temp = Temp::new();
    let path = temp.path().join("library.json");
    let first = library::save(&path, &Recipe::starter(Kind::Clock), None).unwrap();
    let applied = first.clone();
    let mut changed = first.clone();
    changed.name = "Changed".into();
    changed.seal();
    let second = library::save(&path, &changed, Some(&first)).unwrap();
    assert_ne!(second.revision, first.revision);
    assert!(library::save(&path, &changed, Some(&first)).is_err());
    assert!(library::delete(&path, &first).is_err());
    library::delete(&path, &second).unwrap();
    assert!(library::load(&path).unwrap().entries.is_empty());
    assert_eq!(applied, first);
    assert_ne!(applied.name, changed.name);
}
#[test]
fn fb17_library_rejects_damage_symlinks_and_capacity_without_replacement() {
    use std::os::unix::fs::symlink;
    let temp = Temp::new();
    let path = temp.path().join("library.json");
    std::fs::write(&path, b"broken").unwrap();
    assert!(library::save(&path, &Recipe::starter(Kind::Weather), None).is_err());
    assert_eq!(std::fs::read(&path).unwrap(), b"broken");
    std::fs::remove_file(&path).unwrap();
    let target = temp.path().join("target");
    std::fs::write(&target, b"{}").unwrap();
    symlink(&target, &path).unwrap();
    assert!(library::load(&path).is_err());
    std::fs::remove_file(&path).unwrap();
    for _ in 0..MAX_LIBRARY {
        library::save(&path, &Recipe::starter(Kind::Clock), None).unwrap();
    }
    assert!(library::save(&path, &Recipe::starter(Kind::Clock), None).is_err());
    assert_eq!(library::load(&path).unwrap().entries.len(), MAX_LIBRARY);
}
#[test]
fn fb17_canonical_recipe_roundtrip_keeps_identity_and_explicit_reset() {
    let raw = serde_json::json!({"clock":{"face":"analog"}});
    let mut d = crate::design::Document::from_legacy(&raw).unwrap();
    let id = crate::design::InstanceId::legacy(crate::drawer::Widget::Clock);
    let recipe = Recipe::starter(Kind::Clock);
    d.put_legacy(
        Kind::Clock.pointer(),
        Some(serde_json::to_value(&recipe).unwrap()),
    )
    .unwrap();
    assert_eq!(d.version, 2);
    let bytes = d.bytes().unwrap();
    let d2 = crate::design::Document::parse(&bytes).unwrap();
    assert_eq!(d2.instances[&id].kind, crate::drawer::Widget::Clock);
    let projected = d2.project(&raw).unwrap();
    assert_eq!(projected["recipes"]["clock"]["id"], recipe.id);
    let mut reset = d2;
    reset
        .put_legacy(Kind::Clock.pointer(), Some(serde_json::Value::Null))
        .unwrap();
    assert!(reset.project(&raw).unwrap()["recipes"]["clock"].is_null());
    assert!(
        reset.project(&raw).unwrap()["recipes"]
            .get("clock")
            .is_some()
    );
    reset.put_legacy(Kind::Clock.pointer(), None).unwrap();
    assert!(
        reset
            .project(&raw)
            .unwrap()
            .get("recipes")
            .is_none_or(|v| v.get("clock").is_none())
    );
}

struct Temp(std::path::PathBuf);
impl Temp {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!("halcyon-fb17-{}", new_id().unwrap()));
        std::fs::create_dir(&path).unwrap();
        Self(path)
    }
    fn path(&self) -> &std::path::Path {
        &self.0
    }
}
impl Drop for Temp {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}
#[test]
fn fb17_recipe_revision_binds_all_content_and_profiles_require_version13() {
    use crate::desktop::profiles::{Domain, Profile};
    let recipe = Recipe::starter(Kind::Clock);
    let mut changed = recipe.clone();
    changed.name = "Other".into();
    assert!(changed.validate().is_err());
    changed.seal();
    assert_ne!(recipe.revision, changed.revision);
    changed.validate().unwrap();
    let mut profile = Profile::new("Recipe").unwrap();
    profile.widget_style = Some(serde_json::json!({"recipes":{"clock":recipe}}));
    assert_eq!(profile.written_version(), 13);
    profile.version = 12;
    assert!(profile.validate().is_err());
    profile.version = 13;
    profile.validate().unwrap();
    assert!(
        profile
            .widget_section(Domain::WidgetStyle.widget_group().unwrap())
            .is_some()
    );
}
#[test]
fn fb17_personal_recipe_public_profile_keeps_only_native_parts() {
    use crate::desktop::profiles::{Profile, export, import};
    let mut profile = Profile::new("Public recipe").unwrap();
    let recipe = Recipe::starter(Kind::Weather);
    profile.widget_style = Some(serde_json::json!({"recipes":{"weather":recipe}}));
    let output = export(&profile).unwrap();
    let round = import(&output.bytes).unwrap();
    assert_eq!(round.widget_style, profile.widget_style);
    let scene = crate::settings::sharing::Scene::new(&profile).unwrap();
    let config = scene.config().unwrap();
    assert!(config.location.is_none());
    assert_eq!(config.recipes.weather, Some(recipe));
}

#[test]
fn calendar_recipe_roundtrip_versions_reset_and_public_projection() {
    let raw = serde_json::json!({"calendar_private":true,"location":{"name":"Private","latitude":1.,"longitude":2.}});
    let recipe = Recipe::starter(Kind::Calendar);
    assert_eq!(recipe.version, 2);
    let mut document = crate::design::Document::from_legacy(&raw).unwrap();
    document
        .put_legacy(
            Kind::Calendar.pointer(),
            Some(serde_json::to_value(&recipe).unwrap()),
        )
        .unwrap();
    assert_eq!(document.version, 3);
    let restored = crate::design::Document::parse(&document.bytes().unwrap()).unwrap();
    assert_eq!(
        restored.project(&raw).unwrap()["recipes"]["calendar"]["id"],
        recipe.id
    );
    let mut older = document.clone();
    older.version = 2;
    assert!(older.validate().is_err());
    document
        .put_legacy(Kind::Calendar.pointer(), Some(serde_json::Value::Null))
        .unwrap();
    assert!(document.project(&raw).unwrap()["recipes"]["calendar"].is_null());
    use crate::desktop::profiles::{Profile, export, import};
    let mut p = Profile::new("Agenda").unwrap();
    p.widget_style = Some(serde_json::json!({"recipes":{"calendar":recipe}}));
    p.version = p.written_version();
    assert_eq!(p.version, 16);
    let bytes = export(&p).unwrap().bytes;
    assert!(!String::from_utf8_lossy(&bytes).contains("Private"));
    let shared = import(&bytes).unwrap();
    let scene = crate::settings::sharing::Scene::new(&shared).unwrap();
    assert_eq!(scene.config().unwrap().recipes.calendar, Some(recipe));
    p.version = 15;
    assert!(p.validate().is_err());
}

#[test]
fn system_recipe_metric_selection_sampling_and_portable_versions() {
    use crate::config::Config;
    let mut recipe = Recipe::starter(Kind::System);
    recipe.parts = vec![Part::Caption, Part::Memory, Part::Settings];
    recipe.seal();
    recipe.validate().unwrap();
    assert_eq!(recipe.primary(), Part::Memory);
    assert!(!recipe.can_remove(1));
    let mut config = Config::default();
    config.devbox.cpu = true;
    config.devbox.services = true;
    config.devbox.memory = false;
    let original = config.devbox.clone();
    config.recipes.devbox = Some(recipe.clone());
    let sampling = config.devbox_sampling();
    assert!(sampling.memory && sampling.needs_sample());
    assert!(
        !sampling.cpu
            && !sampling.gpu
            && !sampling.network
            && !sampling.services
            && !sampling.disk
            && !sampling.uptime
    );
    assert_eq!(config.devbox, original);
    let raw = serde_json::json!({"recipes":{"devbox":recipe},"devbox":{"memory":false}});
    let document = crate::design::Document::from_legacy(&raw).unwrap();
    assert_eq!(document.version, 3);
    assert_eq!(
        document.project(&raw).unwrap()["recipes"]["devbox"],
        raw["recipes"]["devbox"]
    );
    let mut old = document.clone();
    old.version = 2;
    assert!(old.validate().is_err());
    let mut profile = crate::desktop::profiles::Profile::new("Readings").unwrap();
    profile.widget_style = Some(serde_json::json!({"recipes":{"devbox":recipe}}));
    profile.version = profile.written_version();
    assert_eq!(profile.version, 16);
    let bytes = crate::desktop::profiles::export(&profile).unwrap().bytes;
    let imported = crate::desktop::profiles::import(&bytes).unwrap();
    assert_eq!(
        crate::settings::sharing::Scene::new(&imported)
            .unwrap()
            .config()
            .unwrap()
            .recipes
            .devbox,
        config.recipes.devbox
    );
    profile.version = 15;
    assert!(profile.validate().is_err());
    config.recipes.devbox = None;
    assert_eq!(config.devbox_sampling().as_ref(), &original);
}
