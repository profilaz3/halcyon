use super::*;
use crate::moments::{Moments, Phase};
#[derive(Clone, Debug)]
pub enum Message {
    Preview(Phase),
    Activate(Option<Phase>),
    Family(Phase, Option<crate::desktop_theme::Family>),
    Style(Phase, &'static str, serde_json::Value),
    BaseClock(bool),
    Clock(Option<Phase>, &'static str, serde_json::Value),
}
/// The four Clock appearance choices shared by base settings and Moments.
/// A rendered Moment action captures its phase before it enters the queue.
pub(super) fn clock_style_edit(edit: &super::Message) -> Option<(&'static str, serde_json::Value)> {
    Some(match edit {
        super::Message::ClockFace(value) => ("face", json!(value)),
        super::Message::ClockWeight(value) => ("weight", json!(value)),
        super::Message::ClockItalic(value) => ("italic", json!(value)),
        super::Message::ClockSurface(value) => ("surface", json!(value)),
        _ => return None,
    })
}
impl Settings {
    pub(super) fn moment_sample(&self) -> Config {
        let mut config = self.config.clone();
        config.moments.active = Some(self.moment_preview);
        config.location = Some(crate::config::Location {
            name: "Sample city".into(),
            latitude: 0.,
            longitude: 0.,
            timezone: None,
        });
        config
    }
    pub(super) fn clock_moment_target(&self) -> Option<Phase> {
        (!self.moment_base_clock)
            .then_some(self.config.moments.active)
            .flatten()
    }
    pub(super) fn clock_moment_edit(&self, edit: super::Message) -> super::Message {
        let Some((key, value)) = clock_style_edit(&edit) else {
            return edit;
        };
        super::Message::Moment(Message::Clock(self.clock_moment_target(), key, value))
    }
    pub(super) fn update_moment(&mut self, edit: Message) {
        let result = match edit {
            Message::Preview(phase) => {
                self.moment_preview = phase;
                return;
            }
            Message::BaseClock(base) => {
                self.moment_base_clock = base;
                return;
            }
            Message::Activate(phase) => Config::save_moment("/moments/active", json!(phase)),
            Message::Family(phase, family) => {
                Config::save_moment(&format!("/moments/{}/family", phase.key()), json!(family))
            }
            Message::Style(phase, key, value) => {
                if !Moments::clock_member(key) {
                    return self.failed("Unsupported moment choice".into());
                }
                Config::save_moment(&Moments::pointer(Some(phase), key), value)
            }
            Message::Clock(phase, key, value) => Config::save_moment_clock(phase, key, value),
        };
        match result {
            Ok(config) => {
                self.config = config;
                self.status = "Moment saved · Undo is available".into();
                self.error = false;
            }
            Err(error) => self.failed(error),
        }
    }
}
pub(super) fn scope(edit: &Message) -> Option<String> {
    match edit {
        Message::Activate(_) => Some("/moments/active".into()),
        Message::Family(phase, _) => Some(format!("/moments/{}/family", phase.key())),
        Message::Style(phase, key, _) => Some(Moments::pointer(Some(*phase), key)),
        Message::Clock(phase, key, _) => Some(Moments::pointer(*phase, key)),
        _ => None,
    }
}

/// The existing isolated capture admission calls this bounded message parser.
pub(super) fn capture(value: &serde_json::Value) -> Option<Message> {
    let phase = value
        .get("phase")
        .and_then(|v| serde_json::from_value(v.clone()).ok());
    match value.get("action")?.as_str()? {
        "preview" => Some(Message::Preview(phase?)),
        "activate" => Some(Message::Activate(phase)),
        "face" => Some(Message::Style(phase?, "face", value.get("value")?.clone())),
        _ => None,
    }
}

#[cfg(test)]
mod tests;
