//! Exact ownership and reversible addresses for the pre-instance editors.
use crate::{desktop::store::widget, drawer::Widget};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;

pub(super) fn pointers() -> impl Iterator<Item = &'static str> {
    widget::FIELDS
        .iter()
        .map(|f| f.pointer)
        .chain(widget::LOCAL_FIELDS.iter().map(|f| f.pointer).filter(|p| {
            p.starts_with("/widget_monitors/")
                || p.starts_with("/monitor_layouts/")
                || matches!(
                    *p,
                    "/widget_display"
                        | "/weather/show_dew_point"
                        | "/placement/columns"
                        | "/widgets_visible"
                        | "/appearance/match_wallpaper"
                        | "/appearance/match_cosmic_colors"
                )
        }))
        .chain(["/appearance/wallpaper_color_stamp"])
}
pub(crate) fn owned(pointer: &str) -> bool {
    pointers().any(|p| p == pointer)
}
pub(super) fn identity_list(pointer: &str) -> bool {
    matches!(pointer, "/enabled_widgets" | "/widget_order")
}
pub(super) fn domain(raw: &Value) -> BTreeMap<String, Value> {
    pointers()
        .filter_map(|p| widget::get(raw, p).map(|value| (p.into(), value)))
        .collect()
}
pub(super) fn fingerprint(domain: &BTreeMap<String, Value>) -> String {
    fn canonical(value: &Value) -> Value {
        match value {
            Value::Object(object) => {
                let sorted: BTreeMap<_, _> = object
                    .iter()
                    .map(|(k, v)| (k.clone(), canonical(v)))
                    .collect();
                Value::Object(sorted.into_iter().collect())
            }
            Value::Array(values) => Value::Array(values.iter().map(canonical).collect()),
            value => value.clone(),
        }
    }
    // serde_json's preserve_order feature is enabled by the desktop stack.
    // Formatting/key order is not an old writer changing a widget choice.
    let sorted: BTreeMap<_, _> = domain.iter().map(|(k, v)| (k, canonical(v))).collect();
    Sha256::digest(serde_json::to_vec(&sorted).expect("finite JSON"))
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}
pub(super) fn validate_value(pointer: &str, value: &Value) -> Result<(), String> {
    if !owned(pointer) || !widget::field(pointer).is_some_and(|f| f.accepts(value)) {
        return Err(format!(
            "Unsupported widget value: {}",
            widget::label(pointer)
        ));
    }
    if serde_json::to_vec(value).map_err(|e| e.to_string())?.len() > 16 * 1024 {
        return Err("Widget value is too large".into());
    }
    Ok(())
}
pub(super) fn validate_domain(values: &BTreeMap<String, Value>) -> Result<(), String> {
    for (p, v) in values {
        validate_value(p, v)?;
    }
    Ok(())
}

pub(super) fn instance_slot(pointer: &str) -> Option<(Widget, String)> {
    if pointer == "/clock_font" {
        return Some((Widget::Clock, "font".into()));
    }
    let (root, rest) = pointer.strip_prefix('/')?.split_once('/')?;
    match root {
        "widget_positions" => Some((Widget::parse(rest)?, "layout/position".into())),
        "widget_sizes" => Some((Widget::parse(rest)?, "layout/size".into())),
        "monitor_layouts" => Some((Widget::parse(rest)?, "layout/displays".into())),
        "widget_monitors" => {
            let (kind, key) = rest.split_once('/')?;
            Some((Widget::parse(kind)?, format!("layout/{key}")))
        }
        "recipes" => Some((Widget::parse(rest)?, "recipe".into())),
        "moments" => {
            let (phase, tail) = rest.split_once('/')?;
            let (kind, key) = tail.split_once('/')?;
            if !["day", "night"].contains(&phase) {
                return None;
            }
            Some((Widget::parse(kind)?, format!("moments/{phase}/{key}")))
        }
        kind => Some((Widget::parse(kind)?, format!("options/{rest}"))),
    }
}
pub(super) fn instance_pointer(kind: Widget, key: &str) -> Option<&'static str> {
    static KEYS: std::sync::OnceLock<BTreeMap<(Widget, String), &'static str>> =
        std::sync::OnceLock::new();
    KEYS.get_or_init(|| {
        pointers()
            .filter_map(|p| instance_slot(p).map(|slot| (slot, p)))
            .collect()
    })
    .get(&(kind, key.into()))
    .copied()
}
