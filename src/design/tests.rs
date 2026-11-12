use super::*;
use crate::drawer::Widget;
use serde_json::{Value, json};
fn fixture() -> Value {
    json!({
        "theme":"teal","clock":{"face":"analog","format_24h":false,"surface":null},
        "moments":{"active":"night","day":{"family":"paper","clock":{"weight":"bold"}}},
        "widget_font":"Noto Sans","clock_font":null,"enabled_widgets":["weather","clock"],
        "widget_order":["calendar","weather","projects","devbox"],"widgets_visible":false,
        "widget_positions":{"clock":[0.2,0.3]},"widget_sizes":{"clock":[420.0,250.0]},
        "widget_display":"DP-1","widget_monitors":{"clock":{"display":"DP-2","hide_disconnected":true}},
        "monitor_layouts":{"clock":{"DP-2":{"position":[0.7,0.2],"size":[480.,260.]}}},
        "notes":{"enabled":true,"size":"large"},"focus":{"enabled":false,"focus_minutes":45},
        "location":{"name":"PRIVATE CITY","latitude":1.,"longitude":2.},"project_roots":["/private/project"],
        "calendar_private":true,"refresh_intervals":{"weather":900},"future_preference":{"keep":37}
    })
}
#[test]
fn fb16_model_roundtrip_preserves_all_legacy_choices_and_disabled_instances() {
    let raw = fixture();
    let doc = Document::from_legacy(&raw).unwrap();
    assert_eq!(doc.instances.len(), Widget::ALL.len());
    for kind in Widget::ALL {
        assert_eq!(doc.instances[&InstanceId::legacy(kind)].kind, kind);
    }
    assert_eq!(
        doc.shared["/enabled_widgets"],
        json!(["legacy:weather", "legacy:clock"])
    );
    assert_eq!(
        doc.shared["/widget_order"],
        json!([
            "legacy:calendar",
            "legacy:weather",
            "legacy:projects",
            "legacy:devbox"
        ])
    );
    let clock = &doc.instances[&InstanceId::legacy(Widget::Clock)];
    assert_eq!(clock.values["options/surface"], Value::Null);
    assert_eq!(clock.values["layout/size"], json!([420., 250.]));
    assert_eq!(clock.values["layout/display"], "DP-2");
    assert!(
        !doc.instances[&InstanceId::legacy(Widget::Weather)]
            .values
            .contains_key("layout/size")
    );
    assert_eq!(doc.project(&raw).unwrap(), raw);
    assert_eq!(Document::parse(&doc.bytes().unwrap()).unwrap(), doc);
    let saved = String::from_utf8(doc.bytes().unwrap()).unwrap();
    for private in [
        "PRIVATE CITY",
        "/private/project",
        "future_preference",
        "focus_minutes",
        "calendar_private",
        "refresh_intervals",
    ] {
        assert!(!saved.contains(private), "{private}");
    }
}
#[test]
fn fb16_model_frozen_domain_detects_old_writer_but_allows_unrelated_preferences() {
    let mut raw = fixture();
    let doc = Document::from_legacy(&raw).unwrap();
    raw["location"]["name"] = json!("OTHER CITY");
    raw["future_preference"] = json!(41);
    raw["calendar_private"] = json!(false);
    assert!(doc.check_frozen(&raw).is_ok());
    assert_eq!(doc.project(&raw).unwrap()["location"], raw["location"]);
    raw["clock"]["face"] = json!("digital");
    assert!(doc.check_frozen(&raw).is_err());
    assert!(doc.project(&raw).is_err());
}
#[test]
fn fb16_model_instance_edits_do_not_modify_frozen_source_or_other_members() {
    let raw = fixture();
    let mut doc = Document::from_legacy(&raw).unwrap();
    let source = doc.migration.clone();
    doc.put_legacy("/clock/face", Some(json!("digital")))
        .unwrap();
    doc.put_legacy("/moments/day/clock/italic", Some(json!(true)))
        .unwrap();
    doc.put_legacy("/widget_sizes/clock", None).unwrap();
    let projected = doc.project(&raw).unwrap();
    assert_eq!(projected["clock"]["face"], "digital");
    assert!(projected.pointer("/widget_sizes/clock").is_none());
    assert_eq!(projected["moments"]["day"]["clock"]["italic"], true);
    assert_eq!(doc.migration, source);
    assert_eq!(raw["clock"]["face"], "analog");
    for key in [
        "location",
        "notes",
        "focus",
        "future_preference",
        "widget_monitors",
        "monitor_layouts",
    ] {
        assert_eq!(projected[key], raw[key]);
    }
    assert!(doc.put_legacy("/location", Some(Value::Null)).is_err());
}
#[test]
fn fb16_model_rejects_newer_damaged_unknown_and_mismatched_identity_records() {
    let doc = Document::from_legacy(&fixture()).unwrap();
    let original = serde_json::to_value(&doc).unwrap();
    for (pointer, value) in [
        ("/version", json!(super::VERSION + 1)),
        ("/version", json!(0)),
        ("/format", json!("other")),
        ("/migration/fingerprint", json!("0")),
        ("/instances/legacy:clock/kind", json!("weather")),
        ("/instances/legacy:clock/recipe", json!({"command":"sh"})),
    ] {
        let mut raw = original.clone();
        crate::desktop::store::widget::put(&mut raw, pointer, Some(value)).unwrap();
        assert!(
            Document::parse(&serde_json::to_vec(&raw).unwrap()).is_err(),
            "{pointer}"
        );
    }
    let mut invalid_face = original.clone();
    invalid_face["instances"]["legacy:clock"]["values"]["options/face"] = json!("html");
    assert!(Document::parse(&serde_json::to_vec(&invalid_face).unwrap()).is_err());
    let mut raw = original;
    raw["unrecognized"] = json!(true);
    assert!(Document::parse(&serde_json::to_vec(&raw).unwrap()).is_err());
    assert!(Document::parse(&vec![b' '; MAX_BYTES + 1]).is_err());
    assert!(Document::parse(b"{").is_err());
}
#[test]
fn fb16_model_unsupported_instance_set_cannot_silently_project_or_recreate() {
    let raw = fixture();
    let mut doc = Document::from_legacy(&raw).unwrap();
    let clock = doc
        .instances
        .remove(&InstanceId::legacy(Widget::Clock))
        .unwrap();
    assert!(doc.project(&raw).is_err());
    assert!(
        doc.put_legacy("/clock/face", Some(json!("digital")))
            .is_err()
    );
    doc.instances
        .insert(InstanceId::legacy(Widget::Clock), clock.clone());
    doc.instances.insert(InstanceId::new().unwrap(), clock);
    assert!(doc.validate().is_ok());
    assert!(doc.check_runtime().is_err());
    assert!(doc.project(&raw).is_err());
}
#[test]
fn a_design_saved_before_devices_existed_still_projects() {
    let raw = fixture();
    let mut doc = Document::from_legacy(&raw).unwrap();
    doc.instances.remove(&InstanceId::legacy(Widget::Devices));
    let bytes = serde_json::to_vec_pretty(&doc).unwrap();
    let upgraded = Document::parse(&bytes).unwrap();
    let devices = &upgraded.instances[&InstanceId::legacy(Widget::Devices)];
    assert_eq!(devices.kind, Widget::Devices);
    assert!(devices.values.is_empty());
    assert!(upgraded.check_runtime().is_ok());
    assert!(upgraded.project(&raw).is_ok());
    // An older kind that went missing is still damage, not an upgrade.
    let mut damaged = Document::from_legacy(&raw).unwrap();
    damaged.instances.remove(&InstanceId::legacy(Widget::Clock));
    let bytes = serde_json::to_vec_pretty(&damaged).unwrap();
    assert!(
        Document::parse(&bytes)
            .and_then(|d| d.check_runtime())
            .is_err()
    );
}
#[test]
fn fb16_model_ids_are_stable_bounded_and_not_names_or_indices() {
    for bad in [
        "Clock",
        "0",
        "legacy:unknown",
        "legacy:clock/../weather",
        "w-abc",
        "w-0000000000000000000000000000000Z",
    ] {
        assert!(InstanceId::parse(bad).is_err());
    }
    let mut ids = std::collections::BTreeSet::new();
    for _ in 0..128 {
        let id = InstanceId::new().unwrap();
        assert_eq!(InstanceId::parse(id.as_str()).unwrap(), id);
        assert!(id.legacy_kind().is_none());
        assert!(ids.insert(id));
    }
    assert_eq!(InstanceId::legacy(Widget::Clock).as_str(), "legacy:clock");
}
#[test]
fn fb16_model_field_mapping_is_reversible_unique_and_excludes_content() {
    let mut mapped = std::collections::BTreeSet::new();
    for pointer in legacy::pointers() {
        if let Some((kind, key)) = legacy::instance_slot(pointer) {
            assert!(mapped.insert((kind, key.clone())));
            assert_eq!(legacy::instance_pointer(kind, &key), Some(pointer));
        }
    }
    for pointer in [
        "/location",
        "/music/player",
        "/project_roots",
        "/focus/focus_minutes",
        "/calendar_private",
        "/onboarded",
        "/setup_dismissed",
    ] {
        assert!(!legacy::owned(pointer));
    }
    let raw = json!({});
    let doc = Document::from_legacy(&raw).unwrap();
    assert_eq!(doc.project(&raw).unwrap(), raw);
}

#[test]
fn fb16_model_nested_key_order_is_not_an_old_writer_conflict() {
    let first:Value=serde_json::from_str(r#"{"monitor_layouts":{"clock":{"DP-1":{"size":[400.0,240.0],"position":[0.4,0.3]}}},"clock":{"face":"digital"}}"#).unwrap();
    let reordered:Value=serde_json::from_str(r#"{"clock":{"face":"digital"},"monitor_layouts":{"clock":{"DP-1":{"position":[0.4,0.3],"size":[400.0,240.0]}}}}"#).unwrap();
    assert_eq!(first, reordered);
    let document = Document::from_legacy(&first).unwrap();
    assert!(document.check_frozen(&reordered).is_ok());
    assert_eq!(document.project(&reordered).unwrap(), reordered);
    assert_eq!(
        Document::from_legacy(&reordered)
            .unwrap()
            .migration
            .fingerprint,
        document.migration.fingerprint
    );
}

#[test]
fn architecture_local_design_ownership_stays_separate_from_export_scope() {
    use crate::desktop::store::widget;
    use std::collections::BTreeSet;
    let mut local_design: BTreeSet<String> = [
        "/widget_display",
        "/weather/show_dew_point",
        "/placement/columns",
        "/widgets_visible",
        "/appearance/match_wallpaper",
        "/appearance/match_cosmic_colors",
    ]
    .into_iter()
    .map(str::to_string)
    .collect();
    for widget in Widget::ALL {
        for tail in ["display", "hide_disconnected"] {
            local_design.insert(format!("/widget_monitors/{}/{tail}", widget.key()));
        }
        local_design.insert(format!("/monitor_layouts/{}", widget.key()));
    }
    let actual: BTreeSet<_> = widget::LOCAL_FIELDS
        .iter()
        .filter(|f| legacy::owned(f.pointer))
        .map(|f| f.pointer.to_string())
        .collect();
    assert_eq!(actual, local_design);
    for pointer in local_design {
        assert!(
            !widget::FIELDS.iter().any(|f| f.pointer == pointer),
            "local identity leaked to portable scope: {pointer}"
        );
    }
    for field in &widget::FIELDS {
        assert!(
            legacy::owned(field.pointer),
            "portable field lost design ownership: {}",
            field.pointer
        );
    }
    assert!(legacy::owned("/appearance/wallpaper_color_stamp"));
    for private in [
        "/location",
        "/project_roots",
        "/calendar_private",
        "/music/player",
    ] {
        assert!(widget::field(private).is_some());
        assert!(
            !legacy::owned(private),
            "private preference froze into design: {private}"
        );
    }
}
