//! Projects existing provider state into the panel view; the applet owns no accounts.
use super::*;
use crate::activity_island::bridge::{self, Command, Event, Snapshot};
use crate::tools::{focus, music};

fn short(value: &str) -> String {
    value
        .chars()
        .filter(|c| !c.is_control())
        .take(180)
        .collect()
}
fn key(value: &str) -> String {
    use sha2::{Digest, Sha256};
    Sha256::digest(value.as_bytes())
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}
impl Cards {
    pub(super) fn publish_activity(&mut self) {
        let next = self.config.activity_island;
        if self.activity.preferences.takes_popups() && !next.takes_popups() {
            crate::activity_island::popups::restore_soon();
        }
        if !next.system || !self.activity.preferences.system {
            self.activity.system.clear();
        }
        self.activity.preferences = next;
        self.activity.publish(self.activity_snapshot());
    }
    fn activity_snapshot(&self) -> Snapshot {
        let focus = &self.tools.focus;
        let now = focus::now_ms();
        let playing = self
            .current_player()
            .filter(|_| self.config.activity_island.media);
        let music = playing.map(|p| bridge::Music {
            key: music_key(p),
            title: short(&p.title),
            artist: short(&p.artists.join(", ")),
            playing: p.status == music::Playback::Playing,
            play: p.can_control && p.can_play,
            pause: p.can_control && p.can_pause,
            next: p.can_control && p.can_next,
            previous: p.can_control && p.can_previous,
        });
        let calendar = self
            .calendar_today
            .as_ref()
            .filter(|data| {
                self.config.activity_island.calendar
                    && self.calendar_connection.connected()
                    && data.connection_revision == self.calendar_connection.revision
            })
            .and_then(|data| {
                data.events
                    .iter()
                    .filter_map(|event| {
                        if event.all_day {
                            return None;
                        }
                        let start = chrono::DateTime::parse_from_rfc3339(&event.start)
                            .ok()?
                            .timestamp_millis();
                        let end = chrono::DateTime::parse_from_rfc3339(&event.end)
                            .ok()?
                            .timestamp_millis();
                        (end > now).then(|| bridge::Calendar {
                            key: event.details_key(),
                            title: if self.config.calendar_private {
                                "Busy".into()
                            } else {
                                short(&event.title)
                            },
                            starts_at_ms: start,
                            ends_at_ms: end,
                            all_day: false,
                            join: (!self.config.calendar_private)
                                .then(|| event.join_url().map(str::to_owned))
                                .flatten(),
                        })
                    })
                    .min_by_key(|e| {
                        (
                            e.starts_at_ms.saturating_add(120_000) <= now,
                            e.starts_at_ms,
                        )
                    })
            });
        Snapshot {
            calendar_minute: if calendar.is_some() {
                now.div_euclid(60_000)
            } else {
                0
            },
            preferences: self.config.activity_island,
            music_source: playing.map(|p| key(&format!("{}:{:?}", p.bus, p.owner))),
            quiet: Some(self.activity.quiet),
            dnd: self.activity.dnd,
            timing: playing.and_then(|p| {
                Some(bridge::MusicTiming {
                    key: music_key(p),
                    position_ms: p.position_at(std::time::Instant::now())? / 1000,
                    length_ms: p.length_us.filter(|n| *n > 0)? / 1000,
                    rate: if p.rate.is_finite() {
                        p.rate.clamp(0., 16.)
                    } else {
                        1.
                    },
                })
            }),
            system: self
                .activity
                .system
                .iter()
                .filter(|a| self.config.activity_island.source(a.source))
                .cloned()
                .collect(),
            music,
            focus: bridge::Focus {
                key: focus_key(focus),
                label: focus.phase.title().into(),
                ends_at_ms: match focus.status {
                    focus::Status::Running { ends_at } => Some(ends_at),
                    _ => None,
                },
                remaining_ms: if matches!(focus.status, focus::Status::Running { .. }) {
                    focus.length
                } else {
                    focus.remaining(now, &self.config.focus)
                },
                running: matches!(focus.status, focus::Status::Running { .. }),
                done: matches!(focus.status, focus::Status::Done),
                ready: matches!(focus.status, focus::Status::Ready),
            },
            calendar,
            calendar_problem: self.calendar_today_error,
            music_problem: self.tools.music_problem
                || matches!(self.tools.feed, Some(music::Feed::Unavailable(_))),
            reduced_motion: !self.config.settings_animations,
        }
    }
    pub(super) fn activity_message(&mut self, event: Event) -> Task<Message> {
        match event {
            Event::RefreshCalendar => {}
            Event::Settled => {
                if self.activity.clients == 0 {
                    crate::activity_island::popups::restore_soon();
                }
            }
            Event::Quiet(quiet) => {
                self.activity.quiet = quiet.quiet;
                self.activity.dnd = quiet.dnd;
            }
            Event::System(state) => {
                self.activity.system = state;
            }
            Event::Clients(n) => {
                let before = self.activity.clients;
                self.activity.clients = n;
                if n == 0 {
                    self.activity.system.clear();
                    self.activity.quiet = true;
                    // The island that took COSMIC's popups is gone.
                    crate::activity_island::popups::restore_soon();
                }
                if before == 0 && n > 0 {
                    return Task::batch(self.refresh_visible_data());
                }
            }
            Event::Command(command) => {
                let music_command = match command {
                    Command::Focus { key } => {
                        if key == focus_key(&self.tools.focus) {
                            return self.update_tool(crate::tools::cards::Action::FocusToggle);
                        }
                        return Task::none();
                    }
                    Command::Play { key } => (key, music::Command::Play),
                    Command::Pause { key } => (key, music::Command::Pause),
                    Command::Next { key } => (key, music::Command::Next),
                    Command::Previous { key } => (key, music::Command::Previous),
                    Command::Raise { key } => (key, music::Command::Raise),
                };
                if let Some(player) = self
                    .current_player()
                    .filter(|p| music_key(p) == music_command.0)
                {
                    if transport_allowed(player, music_command.1) {
                        return self.update_tool(crate::tools::cards::Action::Music(
                            music::TransportRequest {
                                bus: player.bus.clone(),
                                command: music_command.1,
                            },
                        ));
                    }
                }
            }
        }
        Task::none()
    }
}
/// What the player says it can do. Raising asks nothing of playback, as on
/// the cards.
fn transport_allowed(player: &music::Player, command: music::Command) -> bool {
    match command {
        music::Command::Raise => player.can_raise,
        music::Command::Play => player.can_control && player.can_play,
        music::Command::Pause => player.can_control && player.can_pause,
        music::Command::Next => player.can_control && player.can_next,
        music::Command::Previous => player.can_control && player.can_previous,
    }
}
fn music_key(p: &music::Player) -> String {
    key(&format!(
        "{:?}:{:?}:{:?}:{}",
        p.bus, p.owner, p.track_id, p.title
    ))
}
fn focus_key(s: &focus::Session) -> String {
    key(&format!("{s:?}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn activity_island_raise_needs_only_the_players_own_permission() {
        let mut player = music::Player {
            bus: "org.mpris.MediaPlayer2.test".into(),
            key: String::new(),
            owner: None,
            seeked: false,
            identity: "Test".into(),
            status: music::Playback::Playing,
            title: "Song".into(),
            artists: Vec::new(),
            album: String::new(),
            art_url: None,
            media_url: None,
            track_id: None,
            can_seek: false,
            length_us: None,
            position_us: None,
            rate: 1.,
            sampled: std::time::Instant::now(),
            can_play: true,
            can_pause: true,
            can_next: true,
            can_previous: true,
            can_control: false,
            can_raise: true,
        };
        assert!(transport_allowed(&player, music::Command::Raise));
        assert!(!transport_allowed(&player, music::Command::Pause));
        player.can_control = true;
        player.can_raise = false;
        assert!(!transport_allowed(&player, music::Command::Raise));
        assert!(transport_allowed(&player, music::Command::Pause));
    }
    #[test]
    fn activity_island_privacy_revision_and_provider_demand() {
        if crate::displays::test_support::isolated(
            "app::activity::tests::activity_island_privacy_revision_and_provider_demand",
        ) {
            return;
        }
        let (mut cards, _) = Cards::init(
            Core::default(),
            Config {
                widgets_visible: false,
                ..Default::default()
            },
        );
        assert!(!cards.music_demand());
        assert!(!cards.calendar_demand());
        let _ = cards.activity_message(Event::Clients(1));
        assert!(cards.music_demand());
        assert!(cards.calendar_demand());
        cards.calendar_connection = crate::calendar::Connection {
            backend: crate::calendar::Backend::Google,
            account: "private@example.invalid".into(),
            revision: "r1".into(),
            calendar_id: "private".into(),
            ..Default::default()
        };
        let now = chrono::Utc::now();
        cards.calendar_today = Some(CalendarData {
            connection_revision: "r1".into(),
            events: vec![crate::data::CalendarEvent {
                title: "Private appointment".into(),
                start: now.to_rfc3339(),
                end: (now + chrono::Duration::minutes(30)).to_rfc3339(),
                url: "https://private.invalid/secret".into(),
                ..Default::default()
            }],
            ..Default::default()
        });
        cards.config.calendar_private = true;
        let projected = cards.activity_snapshot();
        assert_eq!(projected.calendar.as_ref().unwrap().title, "Busy");
        let wire = serde_json::to_string(&projected).unwrap();
        assert!(!wire.contains("Private appointment"));
        assert!(!wire.contains("private.invalid"));
        assert!(!wire.contains("example.invalid"));
        let mut next = cards.calendar_today.as_ref().unwrap().events[0].clone();
        next.title = "Next appointment".into();
        next.start = (now + chrono::Duration::minutes(3)).to_rfc3339();
        next.end = (now + chrono::Duration::minutes(20)).to_rfc3339();
        cards.calendar_today.as_mut().unwrap().events[0].start =
            (now - chrono::Duration::minutes(3)).to_rfc3339();
        cards.calendar_today.as_mut().unwrap().events.push(next);
        cards.config.calendar_private = false;
        assert_eq!(
            cards.activity_snapshot().calendar.unwrap().title,
            "Next appointment",
            "ongoing meeting cannot obscure the upcoming one"
        );
        cards.config.activity_island.calendar = false;
        assert!(!cards.calendar_demand());
        assert!(cards.activity_snapshot().calendar.is_none());
        cards.config.activity_island.calendar = true;
        cards.calendar_connection.revision = "r2".into();
        assert!(cards.activity_snapshot().calendar.is_none());
        let _ = cards.activity_message(Event::Clients(0));
        assert!(!cards.music_demand());
        assert!(!cards.calendar_demand());
    }
    #[test]
    fn activity_island_focus_command_is_fenced_and_uses_shared_session() {
        if crate::displays::test_support::isolated(
            "app::activity::tests::activity_island_focus_command_is_fenced_and_uses_shared_session",
        ) {
            return;
        }
        let (mut cards, _) = Cards::init(Core::default(), Config::default());
        let initial = cards.tools.focus.clone();
        let _ = cards.activity_message(Event::Command(Command::Focus {
            key: "stale".into(),
        }));
        assert_eq!(cards.tools.focus, initial);
        let key = focus_key(&initial);
        let _ = cards.activity_message(Event::Command(Command::Focus { key: key.clone() }));
        assert!(cards.tools.focus.running());
        assert_eq!(cards.tools.focus, focus::Session::load());
        let started = cards.tools.focus.clone();
        let _ = cards.activity_message(Event::Command(Command::Focus { key }));
        assert_eq!(cards.tools.focus, started);
        let a = cards.activity_snapshot();
        let b = cards.activity_snapshot();
        assert_eq!(a, b);
    }
}
