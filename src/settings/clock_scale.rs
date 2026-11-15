//! The clock's size slider. Each step of a drag or arrow key is a draft the
//! preview shows at once; the gesture's end commits it as one change, saved
//! off the interface thread. Commits that arrive while a save is running
//! are coalesced into one more save of the latest size.
use super::{Config, Message, Settings};
use cosmic::app::Task;

#[derive(Debug, Default)]
pub(super) struct ScaleEdit {
    /// The size being dragged or stepped to, not saved yet.
    draft: Option<u16>,
    /// A save is running.
    saving: bool,
    /// A size committed while a save was running, saved after it.
    queued: Option<u16>,
    /// Saves started, for tests.
    #[cfg(test)]
    pub(super) saves: u32,
}

impl Settings {
    /// One step of a drag or arrow key: the preview follows, nothing is
    /// written.
    pub(super) fn clock_scale_step(&mut self, value: u16) {
        let value = value.clamp(50, 200);
        self.clock_scale.draft = Some(value);
        self.config.clock.scale = value;
    }

    /// The gesture ended: save its size once.
    pub(super) fn clock_scale_commit(&mut self) -> Task<Message> {
        let Some(value) = self.clock_scale.draft.take() else {
            return Task::none();
        };
        if self.clock_scale.saving {
            self.clock_scale.queued = Some(value);
            return Task::none();
        }
        self.clock_scale_save(value)
    }

    fn clock_scale_save(&mut self, value: u16) -> Task<Message> {
        self.clock_scale.saving = true;
        #[cfg(test)]
        {
            self.clock_scale.saves += 1;
        }
        let label = crate::desktop::store::widget::label("/clock/scale");
        cosmic::task::future(async move {
            let result = tokio::task::spawn_blocking(move || {
                crate::desktop::actions::in_action(&label, || {
                    Config::save_clock(serde_json::json!({ "scale": value }))
                })
            })
            .await
            .map_err(|error| error.to_string())
            .and_then(|result| result);
            Message::ClockScaleSaved(Box::new(result))
        })
    }

    /// A save finished: take the saved preferences, keep any size still
    /// being dragged, and save the latest committed size if one waited.
    pub(super) fn clock_scale_saved(&mut self, result: Result<Config, String>) -> Task<Message> {
        self.clock_scale.saving = false;
        match result {
            Ok(config) => {
                self.config = config;
                self.saved();
            }
            Err(error) => {
                // Nothing newer is saved over a failure; the slider returns
                // to the size that is saved.
                self.failed(error);
                self.clock_scale.queued = None;
                self.config.clock.scale = Config::load().clock.scale;
            }
        }
        if let Some(value) = self.clock_scale.draft.or(self.clock_scale.queued) {
            self.config.clock.scale = value;
        }
        self.refresh_latest_change();
        match self.clock_scale.queued.take() {
            Some(value) if !self.error => self.clock_scale_save(value),
            _ => Task::none(),
        }
    }
}
