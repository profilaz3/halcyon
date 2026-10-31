use super::*;
use crate::drawer::Widget;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;

/// A saved identity is unrelated to a display name, list position or recipe.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
#[serde(transparent)]
pub struct InstanceId(std::borrow::Cow<'static, str>);
impl InstanceId {
    pub fn legacy(kind: Widget) -> Self {
        Self(std::borrow::Cow::Borrowed(match kind {
            Widget::Clock => "legacy:clock",
            Widget::Weather => "legacy:weather",
            Widget::Calendar => "legacy:calendar",
            Widget::Projects => "legacy:projects",
            Widget::Devbox => "legacy:devbox",
            Widget::Glance => "legacy:glance",
            Widget::Music => "legacy:music",
            Widget::Notes => "legacy:notes",
            Widget::Focus => "legacy:focus",
        }))
    }
    pub fn new() -> Result<Self, String> {
        let mut bytes = [0u8; 16];
        getrandom::fill(&mut bytes).map_err(|e| e.to_string())?;
        Ok(Self(
            format!(
                "w-{}",
                bytes.iter().map(|v| format!("{v:02x}")).collect::<String>()
            )
            .into(),
        ))
    }
    pub fn parse(value: &str) -> Result<Self, String> {
        let reserved = value
            .strip_prefix("legacy:")
            .is_some_and(|kind| Widget::parse(kind).is_some());
        let opaque = value.strip_prefix("w-").is_some_and(|s| {
            s.len() == 32
                && s.bytes()
                    .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
        });
        if !reserved && !opaque {
            return Err("Unsupported widget identity".into());
        }
        Ok(Self(value.to_owned().into()))
    }
    pub fn as_str(&self) -> &str {
        &self.0
    }
    pub fn legacy_kind(&self) -> Option<Widget> {
        self.0.strip_prefix("legacy:").and_then(Widget::parse)
    }
}
impl<'de> Deserialize<'de> for InstanceId {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let value = String::deserialize(d)?;
        Self::parse(&value).map_err(serde::de::Error::custom)
    }
}

/// A compiled base or a frozen personal composition of its approved native parts.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged, deny_unknown_fields)]
pub enum RecipeId {
    Builtin {
        builtin: Widget,
        #[serde(default, skip_serializing_if = "std::ops::Not::not")]
        reset: bool,
    },
    Personal {
        personal: crate::recipes::Recipe,
    },
}
impl RecipeId {
    pub fn builtin(kind: Widget) -> Self {
        Self::Builtin {
            builtin: kind,
            reset: false,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Instance {
    pub kind: Widget,
    pub recipe: RecipeId,
    /// Bounded, allowlisted local leaves. Absence and explicit null differ.
    pub values: BTreeMap<String, Value>,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Migration {
    pub source: String,
    pub fingerprint: String,
    /// Immutable provenance, not a writable mirror of the applied design.
    pub frozen: BTreeMap<String, Value>,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Document {
    pub format: String,
    pub version: u32,
    pub migration: Migration,
    pub shared: BTreeMap<String, Value>,
    pub instances: BTreeMap<InstanceId, Instance>,
}
impl Document {
    pub fn parse(bytes: &[u8]) -> Result<Self, String> {
        if bytes.len() > MAX_BYTES {
            return Err("Widget design is too large".into());
        }
        // Inspect the envelope before serde defaults or component projection.
        let header: Value =
            serde_json::from_slice(bytes).map_err(|_| "Widget design is damaged")?;
        if header.get("format").and_then(Value::as_str) != Some(FORMAT) {
            return Err("Unrecognized widget design format".into());
        }
        match header.get("version").and_then(Value::as_u64) {
            Some(v) if (1..=u64::from(VERSION)).contains(&v) => {}
            Some(v) if v > u64::from(VERSION) => {
                return Err(
                    "This widget design needs a newer Halcyon. Its original data was kept.".into(),
                );
            }
            _ => return Err("Unsupported widget design version".into()),
        }
        let document: Self =
            serde_json::from_value(header).map_err(|e| format!("Widget design is damaged: {e}"))?;
        document.validate()?;
        Ok(document)
    }
    pub fn bytes(&self) -> Result<Vec<u8>, String> {
        self.validate()?;
        let bytes = serde_json::to_vec_pretty(self).map_err(|e| e.to_string())?;
        if bytes.len() > MAX_BYTES {
            return Err("Widget design is too large".into());
        }
        Ok(bytes)
    }
    pub fn validate(&self) -> Result<(), String> {
        if self.format != FORMAT || !(1..=VERSION).contains(&self.version) {
            return Err("Unsupported widget design version".into());
        }
        if self.instances.len() > MAX_INSTANCES {
            return Err("Too many widget instances".into());
        }
        if self.migration.source != "config-v0"
            || self.migration.fingerprint != legacy::fingerprint(&self.migration.frozen)
        {
            return Err("Widget migration provenance is damaged".into());
        }
        legacy::validate_domain(&self.migration.frozen)?;
        for (id, instance) in &self.instances {
            if id.legacy_kind().is_some_and(|kind| kind != instance.kind)
                || match &instance.recipe {
                    RecipeId::Builtin { builtin, reset } => {
                        *builtin != instance.kind
                            || (*reset
                                && self.version
                                    < if matches!(builtin, Widget::Music | Widget::Focus) {
                                        4
                                    } else if matches!(builtin, Widget::Calendar | Widget::Devbox) {
                                        3
                                    } else {
                                        2
                                    })
                    }
                    RecipeId::Personal { personal } => {
                        personal.kind.widget() != instance.kind
                            || self.version < personal.kind.version() + 1
                            || personal.validate().is_err()
                    }
                }
            {
                return Err("Widget identity and recipe do not match".into());
            }
            for (key, value) in &instance.values {
                if key == "recipe" {
                    return Err("Recipe cannot be duplicated in instance values".into());
                }
                let pointer = legacy::instance_pointer(instance.kind, key)
                    .ok_or("Unsupported widget property")?;
                legacy::validate_value(&pointer, value)?;
            }
        }
        if self.shared.contains_key("/rain") && self.version < 5 {
            return Err("Wallpaper rain requires design version 5".into());
        }
        for (pointer, value) in &self.shared {
            if legacy::instance_slot(pointer).is_some() {
                return Err("An instance property cannot be shared".into());
            }
            let value = self.decode_list(pointer, value)?;
            legacy::validate_value(pointer, &value)?;
        }
        Ok(())
    }
    pub fn from_legacy(raw: &Value) -> Result<Self, String> {
        if !raw.is_object() {
            return Err("Settings must be an object".into());
        }
        let frozen = legacy::domain(raw);
        legacy::validate_domain(&frozen)?;
        let mut document = Self {
            format: FORMAT.into(),
            version: 1,
            migration: Migration {
                source: "config-v0".into(),
                fingerprint: legacy::fingerprint(&frozen),
                frozen,
            },
            shared: BTreeMap::new(),
            instances: Widget::ALL
                .into_iter()
                .map(|kind| {
                    (
                        InstanceId::legacy(kind),
                        Instance {
                            kind,
                            recipe: RecipeId::builtin(kind),
                            values: BTreeMap::new(),
                        },
                    )
                })
                .collect(),
        };
        for (pointer, value) in document.migration.frozen.clone() {
            document.put_legacy(&pointer, Some(value))?;
        }
        document.validate()?;
        Ok(document)
    }
    pub fn check_frozen(&self, raw: &Value) -> Result<(), String> {
        if legacy::fingerprint(&legacy::domain(raw)) != self.migration.fingerprint {
            return Err("An older Halcyon or another program changed the saved widget settings. Review the conflict before continuing.".into());
        }
        Ok(())
    }
    /// Explicit conflict recovery restores only our frozen source leaves.
    /// The current applied document and unrelated local preferences survive.
    pub fn restore_frozen_source(&self, raw: &Value) -> Result<Value, String> {
        self.validate()?;
        self.check_runtime()?;
        crate::config::check_config_value(raw)?;
        let mut restored = raw.clone();
        for pointer in legacy::pointers() {
            crate::desktop::store::widget::put(
                &mut restored,
                pointer,
                self.migration.frozen.get(pointer).cloned(),
            )?;
        }
        restored["widget_design_version"] = 1.into();
        self.project(&restored)?;
        Ok(restored)
    }
    fn encode_list(&self, pointer: &str, value: Value) -> Result<Value, String> {
        if !legacy::identity_list(pointer) {
            return Ok(value);
        }
        let list = value.as_array().ok_or("Invalid widget list")?;
        let ids = list
            .iter()
            .map(|v| {
                v.as_str()
                    .and_then(Widget::parse)
                    .map(InstanceId::legacy)
                    .map(|id| Value::String(id.0.into_owned()))
                    .ok_or("Invalid widget kind")
            })
            .collect::<Result<Vec<_>, _>>()?;
        Ok(Value::Array(ids))
    }
    fn decode_list(&self, pointer: &str, value: &Value) -> Result<Value, String> {
        if !legacy::identity_list(pointer) {
            return Ok(value.clone());
        }
        let list = value.as_array().ok_or("Invalid widget identity list")?;
        let mut values = Vec::new();
        for id in list {
            let id = InstanceId::parse(id.as_str().ok_or("Invalid widget identity")?)?;
            let instance = self
                .instances
                .get(&id)
                .ok_or("Widget list names a missing instance")?;
            values.push(Value::String(instance.kind.key().into()));
        }
        Ok(Value::Array(values))
    }
    /// Adapter for existing semantic editors/receipts. It never changes a
    /// different instance of the same kind and never recreates a missing one.
    pub fn put_legacy(&mut self, pointer: &str, value: Option<Value>) -> Result<(), String> {
        if !legacy::owned(pointer) {
            return Err("This setting is outside the widget design".into());
        }
        if let Some(value) = &value {
            legacy::validate_value(pointer, value)?;
        }
        if let Some((kind, key)) = legacy::instance_slot(pointer) {
            let instance = self
                .instances
                .get_mut(&InstanceId::legacy(kind))
                .ok_or("The original widget is missing; restore it in the change plan first")?;
            if key == "recipe" {
                let reset = value.as_ref().is_some_and(Value::is_null);
                if value.is_some() {
                    self.version =
                        self.version
                            .max(if matches!(kind, Widget::Music | Widget::Focus) {
                                4
                            } else if matches!(kind, Widget::Calendar | Widget::Devbox) {
                                3
                            } else {
                                2
                            });
                }
                instance.recipe = match value.filter(|v| !v.is_null()) {
                    Some(value) => {
                        self.version =
                            self.version
                                .max(if matches!(kind, Widget::Music | Widget::Focus) {
                                    4
                                } else if matches!(kind, Widget::Calendar | Widget::Devbox) {
                                    3
                                } else {
                                    2
                                });
                        RecipeId::Personal {
                            personal: serde_json::from_value(value).map_err(|e| e.to_string())?,
                        }
                    }
                    None => RecipeId::Builtin {
                        builtin: kind,
                        reset,
                    },
                };
                return Ok(());
            }
            if let Some(value) = value {
                instance.values.insert(key, value);
            } else {
                instance.values.remove(&key);
            }
        } else {
            let value = value.map(|v| self.encode_list(pointer, v)).transpose()?;
            if let Some(value) = value {
                if pointer == "/rain" {
                    self.version = self.version.max(5);
                }
                self.shared.insert(pointer.into(), value);
            } else {
                self.shared.remove(pointer);
            }
        }
        Ok(())
    }
    /// The current singleton runtime may not silently discard a valid but
    /// unsupported instance. Its adapter admits precisely its nine identities.
    pub fn check_runtime(&self) -> Result<(), String> {
        if self.instances.len() != Widget::ALL.len()
            || Widget::ALL.into_iter().any(|kind| {
                self.instances
                    .get(&InstanceId::legacy(kind))
                    .is_none_or(|v| v.kind != kind)
            })
        {
            return Err("This design uses widget instances this Halcyon runtime cannot display. Its data was kept.".into());
        }
        Ok(())
    }
    pub fn project(&self, raw: &Value) -> Result<Value, String> {
        self.validate()?;
        self.check_frozen(raw)?;
        self.check_runtime()?;
        let mut projected = raw.clone();
        for pointer in legacy::pointers() {
            crate::desktop::store::widget::put(&mut projected, pointer, None)?;
        }
        for (pointer, value) in &self.shared {
            crate::desktop::store::widget::put(
                &mut projected,
                pointer,
                Some(self.decode_list(pointer, value)?),
            )?;
        }
        for instance in self.instances.values() {
            if let RecipeId::Builtin {
                builtin,
                reset: true,
            } = &instance.recipe
            {
                let pointer = crate::recipes::Kind::from_widget(*builtin)
                    .ok_or("Unsupported recipe reset")?
                    .pointer();
                crate::desktop::store::widget::put(&mut projected, pointer, Some(Value::Null))?;
            }
            if let RecipeId::Personal { personal } = &instance.recipe {
                crate::desktop::store::widget::put(
                    &mut projected,
                    personal.kind.pointer(),
                    Some(serde_json::to_value(personal).map_err(|e| e.to_string())?),
                )?;
            }
            for (key, value) in &instance.values {
                if key == "recipe" {
                    return Err("Recipe cannot be duplicated in instance values".into());
                }
                let pointer = legacy::instance_pointer(instance.kind, key)
                    .ok_or("Unsupported widget property")?;
                crate::desktop::store::widget::put(&mut projected, &pointer, Some(value.clone()))?;
            }
        }
        crate::config::check_config_value(&projected)?;
        Ok(projected)
    }
}
