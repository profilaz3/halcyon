//! Reading a saved settings file this version can't use all of.
//!
//! A value from a newer (or older, or hand edited) version, such as an
//! unknown choice or a number out of range, must not cost the whole file:
//! everything else is kept, the value falls back to its default or the
//! nearest number this version accepts, and Settings says what happened.
//!
//! The repair is driven by the same deserializer and validation every
//! write passes, so it covers every field and enum without each one
//! needing its own fallback.

use super::{Config, check_config_value};
use serde::Deserialize;
use serde_json::{Map, Value};

/// One saved value this version couldn't use as written.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Repair {
    /// Where it is in the file, as a JSON pointer ("/edge_menu/style").
    pub pointer: String,
    /// Moved to the nearest accepted number; otherwise the default is used.
    pub clamped: bool,
}

/// Whether this version reads and accepts `value` exactly.
fn accepts(value: &Value) -> bool {
    Config::deserialize(value).is_ok_and(|config| config.validate().is_ok())
}

/// `value` with what this version can't use set aside: unknown choices
/// fall back to their defaults, numbers out of range move to the nearest
/// accepted value, and everything else is kept. Fails only when the file
/// is not a settings object at all.
pub fn repair_config_value(value: &Value) -> Result<(Value, Vec<Repair>), String> {
    if accepts(value) {
        return Ok((value.clone(), Vec::new()));
    }
    let strict = check_config_value(value).err().unwrap_or_default();
    let Value::Object(saved) = value else {
        return Err(strict);
    };
    let mut working = Value::Object(Map::new());
    if !accepts(&working) {
        return Err(strict);
    }
    let mut repairs = Vec::new();
    merge(&mut working, "", saved, &mut repairs);
    Ok((working, repairs))
}

/// The settings in `value`, read the forgiving way.
pub fn config_from_value(value: &Value) -> Result<Config, String> {
    let (repaired, _) = repair_config_value(value)?;
    check_config_value(&repaired)
}

/// The validation a write passes when the file already held values this
/// version can't use: `after` may keep those exactly as they were in
/// `before`, but anything the write itself changed must be valid.
pub fn check_config_change(before: &Value, after: &Value) -> Result<Config, String> {
    let strict = match check_config_value(after) {
        Ok(config) => return Ok(config),
        Err(error) => error,
    };
    let (repaired, repairs) = repair_config_value(after).map_err(|_| strict.clone())?;
    if repairs
        .iter()
        .any(|repair| before.pointer(&repair.pointer) != after.pointer(&repair.pointer))
    {
        return Err(strict);
    }
    check_config_value(&repaired)
}

/// A short sentence for Settings naming what was set aside, or `None`.
pub fn repair_note(repairs: &[Repair]) -> Option<String> {
    if repairs.is_empty() {
        return None;
    }
    let mut names: Vec<String> = repairs
        .iter()
        .map(|repair| {
            let name = repair
                .pointer
                .trim_start_matches('/')
                .split('/')
                .filter(|part| part.parse::<usize>().is_err())
                .map(|part| part.replace("~1", "/").replace("~0", "~").replace('_', " "))
                .collect::<Vec<_>>()
                .join(" ");
            if repair.clamped {
                format!("{name} (adjusted to fit)")
            } else {
                name
            }
        })
        .collect();
    names.dedup();
    const SHOWN: usize = 4;
    let more = names.len().saturating_sub(SHOWN);
    names.truncate(SHOWN);
    let mut list = names.join(", ");
    if more > 0 {
        list.push_str(&format!(" and {more} more"));
    }
    Some(format!(
        "Some saved settings weren’t recognized, so their defaults are used: {list}. \
         Everything else was kept."
    ))
}

fn escape(key: &str) -> String {
    key.replace('~', "~0").replace('/', "~1")
}

fn slot<'a>(working: &'a mut Value, pointer: &str) -> &'a mut Map<String, Value> {
    working
        .pointer_mut(pointer)
        .and_then(Value::as_object_mut)
        .expect("the parent object was placed before its members")
}

/// Adds the members of `saved` (the object at `pointer`) to `working` one
/// at a time, keeping each one this version accepts.
fn merge(
    working: &mut Value,
    pointer: &str,
    saved: &Map<String, Value>,
    repairs: &mut Vec<Repair>,
) {
    for (key, item) in saved {
        let at = format!("{pointer}/{}", escape(key));
        slot(working, pointer).insert(key.clone(), item.clone());
        if accepts(working) {
            continue;
        }
        slot(working, pointer).remove(key);
        match item {
            Value::Object(members) => {
                slot(working, pointer).insert(key.clone(), Value::Object(Map::new()));
                if accepts(working) {
                    merge(working, &at, members, repairs);
                    continue;
                }
                slot(working, pointer).remove(key);
            }
            Value::Array(items) => {
                slot(working, pointer).insert(key.clone(), Value::Array(Vec::new()));
                if accepts(working) {
                    fn list<'a>(
                        working: &'a mut Value,
                        pointer: &str,
                        key: &str,
                    ) -> &'a mut Vec<Value> {
                        slot(working, pointer)
                            .get_mut(key)
                            .and_then(Value::as_array_mut)
                            .expect("the list was placed above")
                    }
                    for (index, element) in items.iter().enumerate() {
                        list(working, pointer, key).push(element.clone());
                        if !accepts(working) {
                            list(working, pointer, key).pop();
                            repairs.push(Repair {
                                pointer: format!("{at}/{index}"),
                                clamped: false,
                            });
                        }
                    }
                    continue;
                }
                slot(working, pointer).remove(key);
            }
            Value::Number(number) => {
                if let Some(nearest) = nearest(working, pointer, key, number) {
                    slot(working, pointer).insert(key.clone(), nearest);
                    repairs.push(Repair {
                        pointer: at,
                        clamped: true,
                    });
                    continue;
                }
            }
            _ => {}
        }
        repairs.push(Repair {
            pointer: at,
            clamped: false,
        });
    }
}

/// The accepted number closest to `saved`, searching from one this
/// version accepts there (the default, else 0 or 1) toward `saved`.
fn nearest(
    working: &mut Value,
    pointer: &str,
    key: &str,
    saved: &serde_json::Number,
) -> Option<Value> {
    let default = Config::deserialize(&*working)
        .ok()
        .and_then(|config| serde_json::to_value(config).ok())
        .and_then(|value| {
            value
                .pointer(&format!("{pointer}/{}", escape(key)))
                .cloned()
        })
        .filter(Value::is_number);
    let mut try_value = |value: Value| {
        slot(working, pointer).insert(key.to_string(), value);
        let ok = accepts(working);
        slot(working, pointer).remove(key);
        ok
    };
    let start = default
        .into_iter()
        .chain([Value::from(0), Value::from(1)])
        .find(|value| try_value(value.clone()))?;
    if let (Some(target), Some(mut good)) = (whole(saved), start.as_number().and_then(whole)) {
        // The accepted range is searched between a value it holds and the
        // saved one; the result is always one that was accepted.
        let mut bad = target;
        while (bad - good).abs() > 1 {
            let middle = good + (bad - good) / 2;
            if try_value(whole_value(middle)?) {
                good = middle;
            } else {
                bad = middle;
            }
        }
        return whole_value(good);
    }
    let target = saved.as_f64().filter(|value| value.is_finite())?;
    let mut good = start.as_f64()?;
    let mut bad = target;
    for _ in 0..64 {
        let middle = good + (bad - good) / 2.;
        if middle == good || middle == bad {
            break;
        }
        if try_value(Value::from(middle)) {
            good = middle;
        } else {
            bad = middle;
        }
    }
    Some(Value::from(good))
}

fn whole(number: &serde_json::Number) -> Option<i128> {
    number
        .as_i64()
        .map(i128::from)
        .or_else(|| number.as_u64().map(i128::from))
}

fn whole_value(value: i128) -> Option<Value> {
    i64::try_from(value)
        .map(Value::from)
        .ok()
        .or_else(|| u64::try_from(value).ok().map(Value::from))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn repaired(value: Value) -> (Config, Vec<Repair>) {
        let (value, repairs) = repair_config_value(&value).unwrap();
        (check_config_value(&value).unwrap(), repairs)
    }

    #[test]
    fn an_unknown_choice_falls_back_and_keeps_the_rest_of_the_file() {
        let (config, repairs) = repaired(json!({
            "theme": "mint",
            "meeting_notice_minutes": 25,
            "edge_menu": {"style": "carousel", "side": "left"},
        }));
        assert_eq!(config.theme, "mint");
        assert_eq!(config.meeting_notice_minutes, 25);
        assert_eq!(
            config.edge_menu.style,
            super::super::edge_menu::EdgeMenu::default().style
        );
        assert_eq!(config.edge_menu.side, super::super::edge_menu::Side::Left);
        assert_eq!(
            repairs,
            [Repair {
                pointer: "/edge_menu/style".into(),
                clamped: false
            }]
        );
    }

    #[test]
    fn out_of_range_numbers_move_to_the_nearest_accepted_value() {
        let (config, repairs) = repaired(json!({
            "theme": "mint",
            "edge_menu": {"delay_ms": 5000, "side": "left"},
            "meeting_notice_minutes": 600,
            "clock": {"scale": 10},
            "appearance": {"corner_radius": 99},
        }));
        assert_eq!(config.theme, "mint");
        assert_eq!(config.edge_menu.side, super::super::edge_menu::Side::Left);
        assert_eq!(config.meeting_notice_minutes, 60);
        assert_eq!(config.clock.scale, 50);
        assert_eq!(config.appearance.corner_radius, 32);
        let edge = Config::default().edge_menu;
        let limit = config.edge_menu.delay_ms;
        assert!(limit > edge.delay_ms && limit < 5000, "{limit}");
        assert!(repairs.iter().all(|repair| repair.clamped), "{repairs:?}");
        assert_eq!(repairs.len(), 4, "{repairs:?}");
    }

    #[test]
    fn current_old_and_garbage_files() {
        // A file this version wrote reads exactly.
        let current = serde_json::to_value(Config::default()).unwrap();
        assert!(repair_config_value(&current).unwrap().1.is_empty());
        // An old file without newer sections reads with their defaults.
        let (config, repairs) = repaired(json!({"theme": "lavender", "widgets_visible": false}));
        assert!(!config.widgets_visible);
        assert!(repairs.is_empty());
        // Wrong types and unknown values deep in the file only lose themselves.
        let (config, repairs) = repaired(json!({
            "theme": 7,
            "widgets_visible": false,
            "enabled_widgets": ["clock", 3, "weather"],
            "widget_positions": {"clock": [2.0, "x"], "weather": [0.5, 0.5]},
            "devbox": {"order": "sideways"},
            "future_section": {"anything": true},
        }));
        assert!(!config.widgets_visible);
        assert_eq!(config.theme, Config::default().theme);
        assert_eq!(config.enabled_widgets, ["clock", "weather"]);
        assert_eq!(config.widget_positions.get("weather"), Some(&[0.5, 0.5]));
        assert!(!config.widget_positions.contains_key("clock"));
        let pointers: Vec<_> = repairs.iter().map(|r| r.pointer.as_str()).collect();
        assert_eq!(
            pointers,
            [
                "/theme",
                "/enabled_widgets/1",
                "/widget_positions/clock",
                "/devbox/order",
            ]
        );
        // Not a settings object at all: that is still an error.
        assert!(repair_config_value(&json!([1, 2])).is_err());
    }

    #[test]
    fn a_write_may_keep_an_old_problem_but_not_add_one() {
        let before = json!({"edge_menu": {"style": "carousel"}, "theme": "mint"});
        let mut after = before.clone();
        after["theme"] = json!("lavender");
        assert_eq!(
            check_config_change(&before, &after).unwrap().theme,
            "lavender"
        );
        let mut broken = before.clone();
        broken["meeting_notice_minutes"] = json!(0);
        assert!(check_config_change(&before, &broken).is_err());
    }

    #[test]
    fn the_note_names_what_was_set_aside() {
        let note = repair_note(&[
            Repair {
                pointer: "/edge_menu/style".into(),
                clamped: false,
            },
            Repair {
                pointer: "/edge_menu/delay_ms".into(),
                clamped: true,
            },
        ])
        .unwrap();
        assert!(
            note.contains("edge menu style, edge menu delay ms (adjusted to fit)"),
            "{note}"
        );
        assert!(!note.contains('—') && !note.contains('–'));
        assert_eq!(repair_note(&[]), None);
    }
}
