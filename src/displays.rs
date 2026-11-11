//! Output names are persistent connection choices; handles belong to this client only.
pub use cosmic::cctk::sctk::{
    output::OutputInfo, reexports::client::protocol::wl_output::WlOutput,
};
pub use cosmic::iced::event::wayland::OutputEvent;

pub fn valid_name(name: &str) -> bool {
    !name.trim().is_empty() && name.len() <= 128 && !name.chars().any(char::is_control)
}

/// A laptop's own panel, by its connector.
pub fn built_in(connector: &str) -> bool {
    ["eDP", "LVDS", "DSI"]
        .iter()
        .any(|prefix| connector.starts_with(prefix))
}

/// The one name Settings gives a display: “Built-in display” for a
/// laptop's own panel, else the model it reports, else its connector.
pub fn display_name(connector: &str, model: Option<&str>) -> String {
    let model = model
        .map(str::trim)
        .filter(|m| !m.is_empty() && *m != connector);
    match model {
        _ if built_in(connector) => "Built-in display".into(),
        Some(model) => model.into(),
        None => connector.into(),
    }
}

/// The name with the connector after it, “Dell U2723QE · DP-2”, or the
/// connector alone when that is all there is.
pub fn display_title(connector: &str, model: Option<&str>) -> String {
    let name = display_name(connector, model);
    if name == connector {
        name
    } else {
        format!("{name} · {connector}")
    }
}

#[derive(Clone, Debug)]
pub struct Output {
    pub handle: WlOutput,
    pub info: OutputInfo,
}
impl Output {
    pub fn name(&self) -> Option<&str> {
        self.info.name.as_deref().filter(|name| valid_name(name))
    }
    /// “Dell U2723QE · DP-2”: the display's name, then its connector.
    pub fn title(&self) -> String {
        let model = self.info.model.trim();
        match (self.name(), model.is_empty()) {
            (Some(name), _) => display_title(name, Some(model)),
            (None, false) => format!("{model} (unnamed display)"),
            (None, true) => "Unnamed display".into(),
        }
    }
    /// The display's name alone, where its connector would crowd the text.
    pub fn display_name(&self) -> String {
        match self.name() {
            Some(name) => display_name(name, Some(&self.info.model)),
            None => self.title(),
        }
    }
    /// OutputEvent's library equality deliberately ignores these properties.
    pub fn same_geometry(&self, other: &Self) -> bool {
        self.handle == other.handle
            && self.info.logical_size == other.info.logical_size
            && self.info.scale_factor == other.info.scale_factor
            && self.info.transform == other.info.transform
    }
}

#[derive(Clone, Default, Debug)]
pub struct Registry {
    outputs: Vec<Output>,
    identities: std::collections::BTreeMap<String, (WlOutput, String)>,
    pub generation: u64,
}
impl Registry {
    pub fn update(&mut self, event: OutputEvent, handle: WlOutput) {
        self.generation += 1;
        match event {
            OutputEvent::Removed => self.outputs.retain(|o| o.handle != handle),
            OutputEvent::Created(None) => {} // Wait for this handle's InfoUpdate.
            OutputEvent::Created(Some(info)) | OutputEvent::InfoUpdate(info) => {
                if let Some(output) = self.outputs.iter_mut().find(|o| o.handle == handle) {
                    output.info = info;
                } else {
                    self.outputs.push(Output { handle, info });
                }
            }
        }
        // EDID observations belong to the unique live output that was scanned,
        // not to a connector name that a later monitor may reuse.
        self.identities.retain(|name, (handle, _)| {
            let mut outputs = self.outputs.iter().filter(|o| o.name() == Some(name));
            outputs.next().is_some_and(|o| &o.handle == handle) && outputs.next().is_none()
        });
    }
    pub fn unique(&self, name: &str) -> Option<&Output> {
        let mut found = self
            .outputs
            .iter()
            .filter(|o| o.name() == Some(name) || self.identity(o) == Some(name));
        let output = found.next()?;
        found.next().is_none().then_some(output)
    }
    /// Compare explicit choices by output, while keeping Automatic distinct.
    pub fn same_choice(&self, a: Option<&str>, b: Option<&str>) -> bool {
        a == b
            || match (
                a.and_then(|s| self.unique(s)),
                b.and_then(|s| self.unique(s)),
            ) {
                (Some(a), Some(b)) => a.handle == b.handle,
                _ => false,
            }
    }
    /// A newly identified output still inherits geometry saved under its connector.
    /// New edits use the canonical identity; old geometry remains available to Undo.
    pub fn effective_config(&self, config: &crate::config::Config) -> crate::config::Config {
        let mut contexts = self.contexts(config);
        for (widget, display) in &mut contexts {
            if let Some(layouts) = config.monitor_layouts.get(widget)
                && !layouts.contains_key(display)
                && let Some(name) = self.unique(display).and_then(Output::name)
                && layouts.contains_key(name)
            {
                *display = name.into();
            }
        }
        config.on_displays(&contexts)
    }
    pub fn outputs(&self) -> &[Output] {
        &self.outputs
    }
    /// What Settings calls the display on `connector`, with its model when
    /// it is connected: “On {display}”.
    pub fn display_name(&self, connector: &str) -> String {
        let model = self
            .outputs
            .iter()
            .find(|o| o.name() == Some(connector))
            .map(|o| o.info.model.as_str());
        display_name(connector, model)
    }
    pub fn accept_identities(
        &mut self,
        generation: u64,
        identities: std::collections::BTreeMap<String, String>,
    ) -> bool {
        if generation != self.generation {
            return false;
        }
        self.identities = identities
            .into_iter()
            .filter_map(|(name, id)| {
                let mut outputs = self
                    .outputs
                    .iter()
                    .filter(|o| o.name() == Some(name.as_str()));
                let output = outputs.next()?;
                outputs
                    .next()
                    .is_none()
                    .then(|| (name, (output.handle.clone(), id)))
            })
            .collect();
        true
    }
    fn identity(&self, output: &Output) -> Option<&str> {
        let (handle, id) = self.identities.get(output.name()?)?;
        (handle == &output.handle).then_some(id.as_str())
    }
    pub fn key(&self, output: &Output) -> Option<String> {
        let name = output.name()?;
        self.unique(name)?;
        self.identity(output)
            .filter(|id| self.unique(id).is_some())
            .map(str::to_owned)
            .or_else(|| Some(name.into()))
    }
    pub fn widget<'a>(
        &'a self,
        config: &crate::config::Config,
        widget: crate::drawer::Widget,
    ) -> Option<&'a Output> {
        let preferred = config.monitor_preference(widget);
        if preferred.is_some()
            && preferred.and_then(|p| self.unique(p)).is_none()
            && config
                .widget_monitors
                .get(widget.key())
                .is_some_and(|m| m.hide_disconnected)
        {
            return None;
        }
        self.selected(preferred)
    }
    pub fn contexts(&self, config: &crate::config::Config) -> crate::config::monitors::Contexts {
        if !config.multi_monitor() && self.outputs.len() < 2 {
            return Default::default();
        }
        self.placements(config)
    }
    pub fn placements(&self, config: &crate::config::Config) -> crate::config::monitors::Contexts {
        crate::drawer::Widget::ALL
            .into_iter()
            .filter_map(|w| Some((w.key().into(), self.key(self.widget(config, w)?)?)))
            .collect()
    }
    pub fn selected(&self, preferred: Option<&str>) -> Option<&Output> {
        preferred.and_then(|name| self.unique(name)).or_else(|| {
            self.outputs
                .iter()
                .filter(|o| o.name().is_some_and(|n| self.unique(n).is_some()))
                .min_by_key(|o| {
                    let name = o.name().unwrap();
                    (!crate::automation::rules::builtin(name), name)
                })
                // Without unique names only session-local placement is possible.
                .or_else(|| self.outputs.first())
        })
    }
    pub fn choices(&self, preferred: Option<&str>) -> Vec<(String, Option<String>)> {
        let mut named: Vec<_> = self
            .outputs
            .iter()
            .filter(|o| o.name().is_some_and(|name| self.unique(name).is_some()))
            .map(|o| (o.title(), self.key(o)))
            .collect();
        named.sort_by(|a, b| a.1.cmp(&b.1));
        let mut choices = vec![("Automatic".into(), None)];
        choices.extend(named);
        if let Some(name) = preferred.filter(|name| self.unique(name).is_none()) {
            let reason = if self.outputs.iter().any(|o| o.name() == Some(name)) {
                "ambiguous name"
            } else {
                "not connected"
            };
            let title = if name.starts_with("edid:") {
                "Saved monitor"
            } else {
                name
            };
            choices.push((format!("{title} ({reason})"), Some(name.into())));
        }
        choices
    }
    pub fn status(&self, preferred: Option<&str>) -> String {
        let Some(actual) = self.selected(preferred) else {
            return "Waiting for a display. Your selection and layout are kept.".into();
        };
        if self
            .outputs
            .iter()
            .any(|o| o.name().is_none_or(|n| self.unique(n).is_none()))
        {
            return "Some displays have no unique connection name and cannot be pinned. Automatic placement is temporary on those displays.".into();
        }
        if preferred.is_some_and(|name| self.unique(name).is_none()) {
            format!(
                "Preferred display disconnected · temporarily using {}",
                actual.title()
            )
        } else {
            format!("Default: {}", actual.title())
        }
    }
}

#[cfg(test)]
pub mod test_support;

#[cfg(test)]
mod tests {
    use super::*;
    use test_support::{Wire, info};
    #[test]
    fn every_screen_names_a_display_the_same_way() {
        assert_eq!(display_name("eDP-1", Some("0x0A2B")), "Built-in display");
        assert_eq!(display_title("eDP-1", None), "Built-in display · eDP-1");
        assert_eq!(display_name("DP-2", Some(" Dell U2723QE ")), "Dell U2723QE");
        assert_eq!(
            display_title("DP-2", Some("Dell U2723QE")),
            "Dell U2723QE · DP-2"
        );
        assert_eq!(display_title("DP-2", Some("")), "DP-2");
        assert_eq!(display_title("DP-2", Some("DP-2")), "DP-2");
    }
    #[test]
    fn physical_aliases_end_with_the_unique_live_output() {
        let wire = Wire::new();
        let a = wire.output(1);
        let b = wire.output(2);
        let mut registry = Registry::default();
        registry.update(
            OutputEvent::Created(Some(info(Some("DP-1"), (1920, 1080)))),
            a.clone(),
        );
        registry.accept_identities(
            registry.generation,
            [
                ("DP-1".into(), "edid:old".into()),
                ("absent".into(), "edid:absent".into()),
            ]
            .into(),
        );
        // Geometry changes do not invalidate identity on the same live object.
        registry.update(
            OutputEvent::InfoUpdate(info(Some("DP-1"), (1280, 720))),
            a.clone(),
        );
        assert_eq!(registry.unique("edid:old").unwrap().handle, a);
        registry.update(OutputEvent::Removed, a.clone());
        registry.update(
            OutputEvent::Created(Some(info(Some("DP-1"), (1920, 1080)))),
            b.clone(),
        );
        assert!(registry.unique("edid:old").is_none());
        assert_eq!(
            registry.key(registry.unique("DP-1").unwrap()).as_deref(),
            Some("DP-1")
        );
        registry.update(
            OutputEvent::Created(Some(info(Some("absent"), (1920, 1080)))),
            a.clone(),
        );
        assert!(registry.unique("edid:absent").is_none());
        registry.accept_identities(
            registry.generation,
            [("DP-1".into(), "edid:new".into())].into(),
        );
        registry.update(
            OutputEvent::InfoUpdate(info(Some("DP-2"), (1920, 1080))),
            b.clone(),
        );
        registry.update(
            OutputEvent::InfoUpdate(info(Some("DP-1"), (1920, 1080))),
            a.clone(),
        );
        assert!(
            registry.unique("edid:new").is_none(),
            "renaming cannot transfer an alias to another output"
        );
        registry.accept_identities(
            registry.generation,
            [("DP-1".into(), "edid:a".into())].into(),
        );
        registry.update(
            OutputEvent::InfoUpdate(info(Some("DP-1"), (1920, 1080))),
            b.clone(),
        );
        registry.accept_identities(
            registry.generation,
            [("DP-1".into(), "edid:ambiguous".into())].into(),
        );
        registry.update(OutputEvent::Removed, a);
        assert!(registry.unique("edid:a").is_none() && registry.unique("edid:ambiguous").is_none());
        registry.accept_identities(
            registry.generation,
            [("DP-1".into(), "edid:b".into())].into(),
        );
        assert_eq!(registry.unique("edid:b").unwrap().handle, b);
        assert!(!registry.accept_identities(
            registry.generation - 1,
            [("DP-1".into(), "edid:old".into())].into()
        ));
        assert!(registry.unique("edid:old").is_none());
    }

    #[test]
    fn monitor_pinning_registry_orders_live_handles_and_waits_for_metadata() {
        let wire = Wire::new();
        let a = wire.output(1);
        let b = wire.output(2);
        let c = wire.output(3);
        for order in [
            [
                (a.clone(), "DP-2"),
                (b.clone(), "DP-1"),
                (c.clone(), "eDP-1"),
            ],
            [
                (c.clone(), "eDP-1"),
                (b.clone(), "DP-1"),
                (a.clone(), "DP-2"),
            ],
        ] {
            let mut registry = Registry::default();
            registry.update(OutputEvent::Created(None), a.clone());
            assert!(registry.selected(None).is_none());
            for (handle, name) in order {
                registry.update(
                    OutputEvent::InfoUpdate(info(Some(name), (1920, 1080))),
                    handle,
                );
            }
            assert_eq!(registry.selected(None).unwrap().handle, c);
            assert_eq!(registry.selected(Some("DP-2")).unwrap().handle, a);
            registry.update(OutputEvent::Removed, c.clone());
            assert_eq!(registry.selected(Some("missing")).unwrap().handle, b);
        }
    }
}

/// EDID is read on a blocking worker, never while painting Settings. Require a
/// nonzero serial so identical generic EDIDs do not pretend to identify hardware.
pub fn physical_identities() -> std::collections::BTreeMap<String, String> {
    use std::io::Read;
    let mut result = std::collections::BTreeMap::new();
    let mut seen = std::collections::BTreeSet::new();
    for entry in std::fs::read_dir("/sys/class/drm")
        .into_iter()
        .flatten()
        .flatten()
        .take(64)
    {
        let name = entry.file_name().to_string_lossy().to_string();
        let Some((_, connector)) = name.split_once('-') else {
            continue;
        };
        let mut bytes = [0u8; 128];
        let Ok(mut file) = std::fs::File::open(entry.path().join("edid")) else {
            continue;
        };
        if file.read_exact(&mut bytes).is_ok()
            && let Some(id) = edid_identity(&bytes)
        {
            if seen.insert(connector.to_string()) {
                result.insert(connector.into(), id);
            } else {
                result.remove(connector);
            }
        }
    }
    result
}
fn edid_identity(bytes: &[u8; 128]) -> Option<String> {
    use sha2::{Digest, Sha256};
    if bytes[..8] != [0, 255, 255, 255, 255, 255, 255, 0]
        || bytes.iter().fold(0u8, |sum, b| sum.wrapping_add(*b)) != 0
    {
        return None;
    }
    let numeric = &bytes[12..16];
    let text = [54, 72, 90, 108].into_iter().find_map(|i| {
        (bytes[i..i + 5] == [0, 0, 0, 255, 0]).then(|| {
            bytes[i + 5..i + 18]
                .iter()
                .copied()
                .filter(|b| !b.is_ascii_whitespace() && *b != 0)
                .collect::<Vec<_>>()
        })
    });
    if (numeric == [0, 0, 0, 0] || numeric == [255, 255, 255, 255])
        && text
            .as_ref()
            .is_none_or(|t| t.is_empty() || t.iter().all(|b| *b == b'0'))
    {
        return None;
    }
    let mut hash = Sha256::new();
    hash.update(&bytes[8..16]);
    if let Some(text) = text {
        hash.update(text);
    }
    Some(format!(
        "edid:{}",
        hash.finalize()
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect::<String>()
    ))
}

#[cfg(test)]
mod multi_monitor_tests {
    use super::*;
    use crate::{
        config::{Config, monitors::WidgetMonitor},
        drawer::Widget,
    };
    use test_support::{Wire, info};
    #[test]
    fn multi_monitor_physical_identity_port_change_duplicates_and_hide() {
        let wire = Wire::new();
        let a = wire.output(1);
        let b = wire.output(2);
        let mut r = Registry::default();
        r.update(
            OutputEvent::Created(Some(info(Some("eDP-1"), (1920, 1080)))),
            a.clone(),
        );
        r.update(
            OutputEvent::Created(Some(info(Some("DP-2"), (2560, 1440)))),
            b.clone(),
        );
        let id = "edid:monitor-serial";
        assert!(r.accept_identities(r.generation, [("DP-2".into(), id.into())].into()));
        let mut c = Config::default();
        c.widget_monitors.insert(
            "weather".into(),
            WidgetMonitor {
                display: Some(id.into()),
                hide_disconnected: false,
            },
        );
        assert_eq!(r.widget(&c, Widget::Weather).unwrap().handle, b);
        r.update(OutputEvent::Removed, b.clone());
        assert_eq!(r.widget(&c, Widget::Weather).unwrap().handle, a);
        c.widget_monitors
            .get_mut("weather")
            .unwrap()
            .hide_disconnected = true;
        assert!(r.widget(&c, Widget::Weather).is_none());
        r.update(
            OutputEvent::Created(Some(info(Some("HDMI-A-1"), (2560, 1440)))),
            b.clone(),
        );
        assert!(r.accept_identities(r.generation, [("HDMI-A-1".into(), id.into())].into()));
        assert_eq!(r.widget(&c, Widget::Weather).unwrap().handle, b);
        assert!(!r.accept_identities(r.generation - 1, Default::default()));
        r.accept_identities(
            r.generation,
            [("HDMI-A-1".into(), id.into()), ("eDP-1".into(), id.into())].into(),
        );
        assert!(r.unique(id).is_none());
        assert_eq!(r.key(r.unique("eDP-1").unwrap()).as_deref(), Some("eDP-1"));
        assert!(r.widget(&c, Widget::Weather).is_none());
    }
    #[test]
    fn multi_monitor_edid_requires_serial_and_checksum() {
        let mut edid = [0; 128];
        edid[..8].copy_from_slice(&[0, 255, 255, 255, 255, 255, 255, 0]);
        let checksum = |bytes: &mut [u8; 128]| {
            bytes[127] = 0;
            bytes[127] = 0u8.wrapping_sub(bytes.iter().fold(0u8, |a, b| a.wrapping_add(*b)));
        };
        checksum(&mut edid);
        assert!(edid_identity(&edid).is_none());
        edid[12] = 1;
        checksum(&mut edid);
        let first = edid_identity(&edid).unwrap();
        edid[40] = 2;
        checksum(&mut edid);
        assert_eq!(edid_identity(&edid).unwrap(), first);
        edid[12] = 2;
        checksum(&mut edid);
        assert_ne!(edid_identity(&edid).unwrap(), first);
        edid[127] ^= 1;
        assert!(edid_identity(&edid).is_none());
    }
}

#[cfg(test)]
mod review030_tests {
    use super::*;
    use test_support::{Wire, info};
    #[test]
    fn review030_connector_choices_match_exactly_one_identity_and_keep_automatic_distinct() {
        let wire = Wire::new();
        let mut registry = Registry::default();
        for (id, name) in [(1, "DP-1"), (2, "DP-2")] {
            registry.update(
                OutputEvent::Created(Some(info(Some(name), (1920, 1080)))),
                wire.output(id),
            );
        }
        registry.accept_identities(
            registry.generation,
            [
                ("DP-1".into(), "edid:one".into()),
                ("DP-2".into(), "edid:two".into()),
            ]
            .into(),
        );
        for preferred in [None, Some("DP-2"), Some("edid:two"), Some("missing")] {
            let choices = registry.choices(preferred);
            let selected: Vec<_> = choices
                .iter()
                .filter(|(_, value)| registry.same_choice(value.as_deref(), preferred))
                .collect();
            assert_eq!(selected.len(), 1);
            assert_eq!(
                selected[0].1.as_deref(),
                match preferred {
                    Some("DP-2") => Some("edid:two"),
                    other => other,
                }
            );
        }
        assert!(!registry.same_choice(None, Some("DP-1")));
    }
}
