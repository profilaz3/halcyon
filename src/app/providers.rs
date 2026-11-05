//! Legacy singleton consumers projected into the keyed provider owner.
use super::*;
use crate::providers::weather::{Completion, Demand, Key, State};

impl Cards {
    pub(super) fn provider_visible(&self, kind: Widget) -> bool {
        if !self.config.data_visible(kind) {
            return false;
        }
        let owner = if self.config.glance.enabled
            && matches!(kind, Widget::Clock | Widget::Weather | Widget::Calendar)
        {
            Widget::Glance
        } else {
            kind
        };
        // Before the first output event, preserve startup's useful prefetch.
        // Afterwards, disconnected/hidden owners release demand immediately.
        if self.displays.generation == 0 {
            return true;
        }
        self.widget_output(owner).is_some()
    }

    /// Calendar also feeds the desktop rail's meeting indicator. Its saved
    /// visibility remains the demand boundary even when its own output is gone.
    pub(super) fn calendar_demand(&self) -> bool {
        self.config.data_visible(Widget::Calendar)
            || (self.activity.clients > 0 && self.config.activity_island.calendar)
    }
    /// Keep watching an available empty Music widget so a new player is found.
    pub(super) fn music_demand(&self) -> bool {
        (self.provider_visible(Widget::Music)
            || (self.activity.clients > 0 && self.config.activity_island.media))
            && !self.layout_editor.active
    }

    pub(super) fn reconcile_weather(&mut self, now: Instant) {
        let key = self.config.location.as_ref().and_then(Key::new);
        let demands = key
            .as_ref()
            .filter(|_| self.provider_visible(Widget::Weather))
            .map(|key| Demand {
                consumer: if self.config.glance.enabled { 1 } else { 0 },
                key: key.clone(),
                interval: self.config.refresh_intervals.duration(Widget::Weather),
            })
            .into_iter()
            .collect::<Vec<_>>();
        // Only two fixed consumers exist in this slice; the finite hub limits
        // cannot be reached by the singleton config projection.
        self.weather_hub
            .reconcile(&demands, now)
            .expect("singleton weather demand");
        if let Some(key) = key {
            let snapshot = self.weather_hub.snapshot(&key, Local::now());
            if snapshot.state != State::Disconnected
                && snapshot.data.is_none()
                && let (Some(weather), Some(at)) = (&self.weather, &self.weather_at)
            {
                self.weather_hub.seed(WeatherSnapshot {
                    location: key.location(),
                    updated_at: at.clone(),
                    weather: weather.clone(),
                });
            }
        }
    }

    pub(super) fn refresh_weather(&mut self, now: Instant) -> Vec<Task<Message>> {
        self.reconcile_weather(now);
        self.weather_hub
            .start_due(now)
            .into_iter()
            .map(|request| {
                cosmic::task::future(async move { Message::Weather(request.run().await) })
            })
            .collect()
    }

    pub(super) fn weather_finished(&mut self, completion: Completion) -> Task<Message> {
        let now = Instant::now();
        self.reconcile_weather(now);
        let accepted = self.weather_hub.complete(completion, now, Local::now());
        let mut tasks = Vec::new();
        if let Some(snapshot) = accepted
            && self.config.location.as_ref().and_then(Key::new).as_ref() == Some(&snapshot.key)
        {
            debug_assert!(snapshot.revision > 0 && !snapshot.refreshing);
            // This is the existing bounded presentation projection, not another
            // scheduler or persistent source of truth. Both live views share it.
            self.weather_error = snapshot.failed;
            self.weather = snapshot.data.as_ref().map(|d| d.weather.clone());
            self.weather_at = snapshot.data.as_ref().map(|d| d.updated_at.clone());
            if !snapshot.failed
                && let Some(data) = snapshot.data
                && let Some(write) = self.weather_cache.prepare(data)
            {
                tasks.push(cosmic::task::future(async move {
                    Message::WeatherCacheSaved(
                        tokio::task::spawn_blocking(move || write.save())
                            .await
                            .unwrap_or_else(|e| Err(e.to_string())),
                    )
                }));
            }
            tasks.push(self.sync_widget_surfaces());
        }
        // Cancellation acknowledgement also frees capacity for queued bindings.
        tasks.extend(self.refresh_weather(now));
        Task::batch(tasks)
    }

    #[cfg(test)]
    pub(super) fn weather_fetching(&self) -> bool {
        self.config
            .location
            .as_ref()
            .and_then(Key::new)
            .is_some_and(|key| self.weather_hub.snapshot(&key, Local::now()).refreshing)
    }
}

#[cfg(test)]
mod tests;
