//! Eligibility and selection are independent of drawing and provider observation.
use super::*;
impl Applet {
    pub(super) fn capture_source(&mut self) {
        self.selected_source = match self.kind {
            Kind::Music => self.snapshot.music_source.clone(),
            Kind::Calendar => self.snapshot.calendar.as_ref().map(|c| c.key.clone()),
            _ => None,
        };
    }
    pub(super) fn selected_source_matches(&self) -> bool {
        if self.popup.is_none() || self.selected_source.is_none() {
            return true;
        }
        match self.kind {
            Kind::Music => self.selected_source == self.snapshot.music_source,
            Kind::Calendar => {
                self.selected_source.as_ref() == self.snapshot.calendar.as_ref().map(|c| &c.key)
            }
            _ => true,
        }
    }

    pub(super) fn observe_system_deadlines(&mut self, next: &Snapshot, now: Instant) {
        self.system_deadlines = next
            .system
            .iter()
            .filter_map(|a| {
                let end = a.expires_ms?;
                let at = self
                    .system_deadlines
                    .iter()
                    .find(|(source, stamp, _)| *source == a.source && *stamp == end)
                    .map(|(_, _, at)| *at)
                    .unwrap_or_else(|| {
                        now + Duration::from_millis(
                            end.saturating_sub(self.now_ms).clamp(0, 3000) as u64
                        )
                    });
                Some((a.source, end, at))
            })
            .collect();
    }
    pub(super) fn slot_width(&self) -> f32 {
        if self.kind == Kind::Idle && self.live() {
            // The shelf's tray and count sit beside the title.
            112. + self.shelf_trailing_width()
        } else {
            COMPACT_WIDTH
        }
    }
    pub(super) fn observe_playback(&mut self, next: &Snapshot, now: Instant) {
        let before = self.snapshot.music.as_ref();
        match next.music.as_ref() {
            Some(m) if !next.music_problem && m.playing => {
                self.paused_key = None;
                self.paused_until = None;
            }
            Some(m)
                if !next.music_problem
                    && before.is_some_and(|old| old.key == m.key && old.playing) =>
            {
                self.paused_key = Some(m.key.clone());
                self.paused_until = self
                    .popup
                    .is_none()
                    .then_some(now + Duration::from_secs(10));
            }
            Some(m) if !next.music_problem && self.paused_key.as_ref() == Some(&m.key) => {}
            _ => {
                self.paused_key = None;
                self.paused_until = None;
            }
        }
    }
    pub(super) fn arm_pause_grace(&mut self) {
        if self
            .snapshot
            .music
            .as_ref()
            .is_some_and(|m| !m.playing && self.paused_key.as_ref() == Some(&m.key))
        {
            self.paused_until = Some(Instant::now() + Duration::from_secs(10));
        }
    }
    fn media_eligible(&self) -> bool {
        self.snapshot.preferences.media
            && !self.snapshot.music_problem
            && self.snapshot.music.as_ref().is_some_and(|m| {
                m.playing
                    || (self.paused_key.as_ref() == Some(&m.key)
                        && (self.paused_until.is_some_and(|at| at > Instant::now())
                            || (self.popup.is_some() && self.kind == Kind::Music)))
            })
    }
    fn calendar_eligible(&self) -> bool {
        self.snapshot.preferences.calendar
            && !self.snapshot.calendar_problem
            && self.snapshot.calendar.as_ref().is_some_and(|c| {
                !c.all_day
                    && self.dismissed_calendar.as_ref() != Some(&c.key)
                    && c.starts_at_ms
                        <= self
                            .now_ms
                            .saturating_add(self.snapshot.preferences.lead_ms())
                    && c.starts_at_ms.saturating_add(120_000) > self.now_ms
                    && c.ends_at_ms > self.now_ms
            })
    }
    /// The shown system activity when it asks for attention.
    pub(super) fn urgent(&self) -> Option<system::Source> {
        match self.kind {
            Kind::System(source) if !self.chooser => self
                .system_activity(source)
                // Low battery is a warning before it becomes urgent.
                .filter(|a| {
                    a.urgent || (a.source == system::Source::Power && a.title.ends_with(" low"))
                })
                .map(|a| a.source),
            _ => None,
        }
    }
    pub(super) fn system_activity(&self, source: system::Source) -> Option<&system::Activity> {
        self.snapshot
            .system
            .iter()
            .filter(|a| self.snapshot.preferences.source(a.source))
            .find(|a| {
                a.source == source
                    && a.expires_ms.is_none_or(|end| {
                        self.system_deadlines
                            .iter()
                            .find(|(s, stamp, _)| *s == source && *stamp == end)
                            .map(|(_, _, at)| *at > Instant::now())
                            .unwrap_or(end > self.now_ms)
                    })
            })
    }
    pub(super) fn eligible_kinds(&self) -> Vec<Kind> {
        let mut kinds = Vec::new();
        // A finished timer asks first; a counting one sits after Focus.
        if self.timer_done() {
            kinds.push(Kind::Timer);
        }
        if self.calendar_eligible() {
            kinds.push(Kind::Calendar);
        }
        if self.notes_shown() {
            kinds.extend(self.notes.iter().map(|n| Kind::Notification(n.id)));
        }
        if self.snapshot.focus.running
            && self
                .snapshot
                .focus
                .ends_at_ms
                .is_some_and(|end| end > self.now_ms)
        {
            kinds.push(Kind::Focus);
        }
        if self.timer_live() {
            kinds.push(Kind::Timer);
        }
        if self.media_eligible() {
            kinds.push(Kind::Music);
        }
        if !self.ui.jobs.is_empty() {
            kinds.push(Kind::Transfer);
        }
        if self.devices_eligible() {
            kinds.push(Kind::Devices);
        }
        for source in [
            system::Source::Power,
            system::Source::Audio,
            system::Source::Bluetooth,
            system::Source::Network,
            system::Source::Updates,
            system::Source::Microphone,
            system::Source::Camera,
        ] {
            if self.system_activity(source).is_some() {
                kinds.push(Kind::System(source));
            }
        }
        kinds
    }
    pub(super) fn automatic_kind(&self) -> Kind {
        let kinds = self.eligible_kinds();
        if let Some(urgent) = self
            .snapshot
            .system
            .iter()
            .find(|a| a.urgent && kinds.contains(&Kind::System(a.source)))
        {
            return Kind::System(urgent.source);
        }
        if let Some(note) = self
            .notes
            .iter()
            .find(|n| n.urgent && kinds.contains(&Kind::Notification(n.id)))
        {
            return Kind::Notification(note.id);
        }
        if matches!(kinds.first(), Some(Kind::System(_)))
            && matches!(self.kind,Kind::System(source) if !source.privacy())
            && kinds.contains(&self.kind)
        {
            return self.kind;
        }
        kinds.first().copied().unwrap_or(Kind::Idle)
    }
    pub(super) fn music_progress(&self) -> Option<(i64, i64)> {
        if !self.selected_source_matches() {
            return None;
        }
        let m = self.snapshot.music.as_ref()?;
        let timing = self.snapshot.timing.as_ref()?;
        if m.key != timing.key
            || self.snapshot.music_problem
            || timing.length_ms <= 0
            || !timing.rate.is_finite()
        {
            return None;
        }
        let elapsed = if m.playing {
            (self.music_sampled.elapsed().as_millis() as f64 * timing.rate.clamp(0., 16.)) as i64
        } else {
            0
        };
        Some((
            timing
                .position_ms
                .saturating_add(elapsed)
                .clamp(0, timing.length_ms),
            timing.length_ms,
        ))
    }
    pub(super) fn system_content(&self, source: system::Source) -> Content {
        match self.system_activity(source) {
            Some(a) => Content {
                title: a.title.clone(),
                cue: a.cue.clone(),
                meta: a.detail.clone(),
                progress: a.progress.map(|p| p as f32 / 100.),
                bar_start: 0.,
            },
            None => Content {
                title: source.label().into(),
                meta: "This activity has ended".into(),
                ..Default::default()
            },
        }
    }
    pub(super) fn chooser_view(
        &self,
        palette: Palette,
        tint: Color,
        reveal: f32,
    ) -> Element<'_, Message> {
        let mut list = widget::column([]).spacing(2);
        for kind in self.eligible_kinds() {
            let label = match kind {
                Kind::Music => self
                    .snapshot
                    .music
                    .as_ref()
                    .map(|m| m.title.as_str())
                    .unwrap_or("Music"),
                Kind::Calendar => self
                    .snapshot
                    .calendar
                    .as_ref()
                    .map(|c| c.title.as_str())
                    .unwrap_or("Calendar"),
                Kind::System(source) => self
                    .system_activity(source)
                    .map(|a| a.title.as_str())
                    .unwrap_or(source.label()),
                Kind::Notification(id) => self
                    .note(id)
                    .map(|n| n.title.as_str())
                    .unwrap_or("Notification"),
                _ => kind.label(),
            };
            let id = format!("activity-choice-{}", kind.id());
            let row = widget::row([
                self.kind_icon(kind, 16),
                self.text(label, 13., alpha(palette.ink, reveal)),
            ])
            .spacing(10)
            .align_y(cosmic::iced::Alignment::Center);
            list = list.push(
                widget::button::custom(row)
                    .id(id.into())
                    .width(Length::Fill)
                    .height(32)
                    .padding([6, 10])
                    .class(capsule_button(palette, tint, reveal, Role::Ghost))
                    .on_press(Message::Select(kind)),
            );
        }
        if self.notes.len() > 1 && self.notes_shown() {
            list = list.push(
                widget::button::custom(
                    widget::row([
                        widget::icon::from_name("edit-clear-all-symbolic")
                            .symbolic(true)
                            .size(16)
                            .into(),
                        self.text("Clear all notifications", 13., alpha(palette.ink, reveal)),
                    ])
                    .spacing(10)
                    .align_y(cosmic::iced::Alignment::Center),
                )
                .id("activity-choice-clear".into())
                .width(Length::Fill)
                .height(32)
                .padding([6, 10])
                .class(capsule_button(palette, tint, reveal, Role::Ghost))
                .on_press(Message::NotesClear),
            );
        }
        if self.has_recent() && self.notes_shown() {
            list = list.push(
                widget::button::custom(
                    widget::row([
                        widget::icon::from_name("document-open-recent-symbolic")
                            .symbolic(true)
                            .size(16)
                            .into(),
                        widget::container(self.text("Recent", 13., alpha(palette.ink, reveal)))
                            .width(Length::Fill)
                            .into(),
                        self.text(
                            self.recent.len().to_string(),
                            12.,
                            alpha(palette.quiet, reveal),
                        ),
                    ])
                    .spacing(10)
                    .align_y(cosmic::iced::Alignment::Center),
                )
                .id("activity-choice-recent".into())
                .width(Length::Fill)
                .height(32)
                .padding([6, 10])
                .class(capsule_button(palette, tint, reveal, Role::Ghost))
                .on_press(Message::ShowRecent),
            );
        }
        if self.timer().is_none() {
            list = list.push(self.timer_choice(palette, tint, reveal));
        }
        let dnd = self.snapshot.dnd;
        list = list.push(
            widget::button::custom(
                widget::row([
                    widget::icon::from_name(super::MOON)
                        .symbolic(true)
                        .size(16)
                        .into(),
                    widget::container(self.text("Do Not Disturb", 13., alpha(palette.ink, reveal)))
                        .width(Length::Fill)
                        .into(),
                    self.text(
                        if dnd { "On" } else { "Off" },
                        12.,
                        alpha(palette.quiet, reveal),
                    ),
                ])
                .spacing(10)
                .align_y(cosmic::iced::Alignment::Center),
            )
            .id("activity-choice-dnd".into())
            .width(Length::Fill)
            .height(32)
            .padding([6, 10])
            .class(capsule_button(palette, tint, reveal, Role::Ghost))
            .on_press(Message::Dnd(!dnd)),
        );
        widget::scrollable(list)
            .id("activity-choices")
            .height(Length::Fill)
            .width(Length::Fill)
            .into()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn app() -> Applet {
        let (mut a, _) = Applet::init(Core::default(), ());
        a.active = true;
        a.snapshot.focus.key = "focus".into();
        a.commands = Some(tokio::sync::mpsc::channel(4).0);
        a
    }
    fn music(playing: bool) -> bridge::Music {
        bridge::Music {
            key: "track".into(),
            title: "Golden Hour".into(),
            artist: "Aura · Solstice".into(),
            playing,
            play: true,
            pause: true,
            next: true,
            previous: true,
        }
    }
    #[test]
    fn reactive_idle_excludes_paused_media_distant_and_all_day_events() {
        let mut a = app();
        assert_eq!(a.automatic_kind(), Kind::Idle);
        a.snapshot.music = Some(music(false));
        a.snapshot.calendar = Some(bridge::Calendar {
            key: "event".into(),
            title: "Later".into(),
            starts_at_ms: a.now_ms + 3_600_000,
            ends_at_ms: a.now_ms + 7_200_000,
            all_day: false,
            join: None,
            color: None,
        });
        assert_eq!(a.automatic_kind(), Kind::Idle);
        a.snapshot.calendar.as_mut().unwrap().starts_at_ms = a.now_ms + 120_000;
        assert_eq!(a.automatic_kind(), Kind::Calendar);
        a.snapshot.calendar.as_mut().unwrap().all_day = true;
        assert_eq!(a.automatic_kind(), Kind::Idle);
    }
    #[test]
    fn reactive_pause_grace_requires_observed_playback_and_resuming_cancels_it() {
        let mut a = app();
        a.snapshot.music = Some(music(true));
        let mut next = a.snapshot.clone();
        next.music = Some(music(false));
        a.observe_playback(&next, Instant::now());
        a.snapshot = next.clone();
        assert_eq!(a.automatic_kind(), Kind::Music);
        a.paused_until = Some(Instant::now() - Duration::from_secs(1));
        assert_eq!(a.automatic_kind(), Kind::Idle);
        next.music = Some(music(true));
        a.observe_playback(&next, Instant::now());
        assert!(a.paused_until.is_none() && a.paused_key.is_none());
        a.snapshot = next;
        a.popup = Some(window::Id::unique());
        let mut next = a.snapshot.clone();
        next.music = Some(music(false));
        a.kind = Kind::Music;
        a.observe_playback(&next, Instant::now());
        a.snapshot = next;
        assert!(a.paused_until.is_none());
        assert_eq!(a.automatic_kind(), Kind::Music);
        a.arm_pause_grace();
        assert!(a.paused_until.is_some());
    }
    #[test]
    fn reactive_meeting_releases_primary_after_start_and_dismissal_is_occurrence_scoped() {
        let mut a = app();
        a.snapshot.music = Some(music(true));
        a.snapshot.calendar = Some(bridge::Calendar {
            key: "event".into(),
            title: "Soon".into(),
            starts_at_ms: a.now_ms + 60_000,
            ends_at_ms: a.now_ms + 1_800_000,
            all_day: false,
            join: None,
            color: None,
        });
        assert_eq!(a.automatic_kind(), Kind::Calendar);
        a.dismissed_calendar = Some("event".into());
        assert_eq!(a.automatic_kind(), Kind::Music);
        a.snapshot.calendar.as_mut().unwrap().key = "next".into();
        assert_eq!(a.automatic_kind(), Kind::Calendar);
        a.snapshot.calendar.as_mut().unwrap().starts_at_ms = a.now_ms - 120_000;
        assert_eq!(a.automatic_kind(), Kind::Music);
    }
    #[test]
    fn reactive_system_urgency_and_expiry_do_not_promote_stale_incidents() {
        let mut a = app();
        a.snapshot.music = Some(music(true));
        a.snapshot.system = vec![system::Activity {
            source: system::Source::Power,
            title: "Low battery".into(),
            detail: String::new(),
            cue: "5%".into(),
            progress: None,
            urgent: true,
            announce: true,
            expires_ms: Some(a.now_ms + 1),
        }];
        assert_eq!(a.automatic_kind(), Kind::System(system::Source::Power));
        a.now_ms += 1;
        assert_eq!(a.automatic_kind(), Kind::Music);
    }
    #[test]
    fn reactive_music_timing_is_bounded_and_fenced_to_displayed_track() {
        let mut a = app();
        a.snapshot.music = Some(music(false));
        a.snapshot.timing = Some(bridge::MusicTiming {
            key: "track".into(),
            position_ms: 102_000,
            length_ms: 236_000,
            rate: 1.,
        });
        assert_eq!(a.music_progress(), Some((102_000, 236_000)));
        a.snapshot.timing.as_mut().unwrap().key = "old".into();
        assert_eq!(a.music_progress(), None);
        let t = a.snapshot.timing.as_mut().unwrap();
        t.key = "track".into();
        t.rate = f64::NAN;
        assert_eq!(a.music_progress(), None);
    }
    #[test]
    fn reactive_native_player_quiet_and_chooser_fit_and_capture() {
        let mut a = app();
        a.snapshot.music = Some(music(true));
        a.kind = Kind::Music;
        a.snapshot.timing = Some(bridge::MusicTiming {
            key: "track".into(),
            position_ms: 102_000,
            length_ms: 236_000,
            rate: 1.,
        });
        a.motion.retarget(1., false, Instant::now());
        a.popup = Some(window::Id::unique());
        let mut ui = crate::native_test::Native::new(cosmic::iced::Size::new(WIDTH, HEIGHT));
        for id in [
            "activity-previous",
            "activity-pause",
            "activity-next",
            "activity-collapse",
        ] {
            let b = ui.bounds(a.capsule(true), id);
            assert!(
                b.x >= 0. && b.y >= 0. && b.x + b.width <= WIDTH && b.y + b.height <= HEIGHT,
                "{id}: {b:?}"
            );
        }
        assert!(ui.duplicate_ids(a.capsule(true)).is_empty());
        let root = std::env::var_os("HALCYON_ACTIVITY_EVIDENCE").map(std::path::PathBuf::from);
        for (name, theme) in [
            ("dark", cosmic::Theme::dark()),
            ("light", cosmic::Theme::light()),
        ] {
            a.core.applet.background = if name == "light" {
                cosmic::applet::cosmic_panel_config::CosmicPanelBackground::Light
            } else {
                cosmic::applet::cosmic_panel_config::CosmicPanelBackground::Dark
            };
            for (state, kind) in [("player", Kind::Music), ("quiet", Kind::Idle)] {
                a.kind = kind;
                let bytes = ui.render_scaled(a.capsule(true), &theme, 2.);
                if let Some(root) = &root {
                    std::fs::create_dir_all(root).unwrap();
                    image::save_buffer(
                        root.join(format!("reactive-{state}-{name}.png")),
                        &bytes,
                        WIDTH as u32 * 2,
                        HEIGHT as u32 * 2,
                        image::ColorType::Rgba8,
                    )
                    .unwrap();
                }
            }
        }
        a.snapshot.music = None;
        a.snapshot.timing = None;
        a.popup = None;
        a.kind = Kind::Idle;
        assert_eq!(a.slot_width(), 112.);
    }
}

impl Applet {
    pub(super) fn new_system_reveal(&self, snapshot: &Snapshot) -> Option<Kind> {
        snapshot
            .system
            .iter()
            .find(|a| {
                a.announce
                    && (a.urgent || a.expires_ms.is_some())
                    && !self.snapshot.system.iter().any(|old| {
                        old.source == a.source
                            && old.title == a.title
                            && old.urgent == a.urgent
                            && old.expires_ms == a.expires_ms
                    })
            })
            .map(|a| Kind::System(a.source))
    }
    pub(super) fn consider_reveal(&mut self, system: Option<Kind>, now: Instant) -> Task<Message> {
        let meeting = self
            .snapshot
            .calendar
            .as_ref()
            .filter(|_| self.calendar_eligible())
            .map(|c| c.key.clone());
        let changed = meeting.is_some() && meeting != self.seen_meeting;
        self.seen_meeting = meeting;
        if !self.observed {
            self.observed = true;
            return Task::none();
        }
        let candidate = if changed {
            Some(Kind::Calendar)
        } else {
            system
        };
        let Some(kind) = candidate else {
            return Task::none();
        };
        if !self.eligible_kinds().contains(&kind) {
            return Task::none();
        }
        // Mark occurrences above even when quiet/busy: never replay a backlog.
        if self.popup.is_some()
            || self.hovered
            || !self.snapshot.preferences.reveals
            || !self.host_reveals
            || self.snapshot.quiet != Some(false)
            || self.snapshot.dnd
            || !self.live()
        {
            return Task::none();
        }
        // A finished timer is as pressing as an urgent activity.
        let urgent = kind == Kind::Timer
            || (kind == Kind::Devices && self.devices_pressing())
            || matches!(kind,Kind::System(source) if self.system_activity(source).is_some_and(|a|a.urgent));
        if !urgent
            && self
                .snapshot
                .system
                .iter()
                .any(|a| a.urgent && self.system_activity(a.source).is_some())
        {
            return Task::none();
        }
        if !urgent
            && self
                .last_reveal
                .is_some_and(|at| now.saturating_duration_since(at) < Duration::from_secs(30))
        {
            return Task::none();
        }
        self.kind = kind;
        self.last_reveal = Some(now);
        self.reveal_deadline = Some(now + Duration::from_secs(3));
        self.revealed = true;
        self.open_popup()
    }
}

/// What a notification's new version is to the one it replaces, when it
/// carries a progress value.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Step {
    /// No progress either side: an ordinary arrival.
    Plain,
    /// Progress begins: it may be revealed once, as any arrival.
    Start,
    /// The value moved: updated in place, never revealed.
    Update,
    /// It reached the end or dropped its value: revealed once more.
    Finish,
}
/// `before` is the replaced notification's progress, `None` when it is new.
pub(super) fn progress_step(before: Option<Option<u8>>, after: Option<u8>) -> Step {
    match (before, after) {
        (Some(Some(old)), Some(new)) if old < 100 && new >= 100 => Step::Finish,
        (Some(Some(old)), None) if old < 100 => Step::Finish,
        (Some(Some(_)), Some(_)) => Step::Update,
        (_, Some(new)) if new < 100 => Step::Start,
        _ => Step::Plain,
    }
}

#[cfg(test)]
mod integration_tests {
    use super::*;
    fn app() -> Applet {
        let (mut a, _) = Applet::init(Core::default(), ());
        a.active = true;
        a.commands = Some(tokio::sync::mpsc::channel(2).0);
        a.snapshot.focus.key = "ready".into();
        a.snapshot.quiet = Some(false);
        a.host_reveals = true;
        a
    }
    fn meeting(a: &mut Applet, key: &str) {
        a.snapshot.calendar = Some(bridge::Calendar {
            key: key.into(),
            title: "Review".into(),
            starts_at_ms: a.now_ms + 180_000,
            ends_at_ms: a.now_ms + 900_000,
            all_day: false,
            join: None,
            color: None,
        });
    }
    #[test]
    fn reactive_reveals_respect_discovery_quiet_autohide_and_no_backlog() {
        for gate in 0..4 {
            let mut a = app();
            meeting(&mut a, "first");
            let _ = a.consider_reveal(None, Instant::now());
            assert!(a.popup.is_none(), "discovery is silent");
            meeting(&mut a, "second");
            match gate {
                0 => a.snapshot.quiet = Some(true),
                1 => a.snapshot.quiet = None,
                2 => a.host_reveals = false,
                _ => a.snapshot.preferences.reveals = false,
            }
            let _ = a.consider_reveal(None, Instant::now());
            assert!(a.popup.is_none());
            a.snapshot.quiet = Some(false);
            a.host_reveals = true;
            a.snapshot.preferences.reveals = true;
            let _ = a.consider_reveal(None, Instant::now());
            assert!(a.popup.is_none(), "quiet occurrences are not replayed");
            meeting(&mut a, "third");
            let _ = a.consider_reveal(None, Instant::now());
            assert!(a.popup.is_some());
            assert!(a.reveal_deadline.is_some());
            let _ = a.destroy_popup();
            meeting(&mut a, "fourth");
            let _ = a.consider_reveal(None, Instant::now());
            assert!(a.popup.is_none(), "routine cooldown");
        }
    }
    #[test]
    fn reactive_source_identity_and_privacy_remain_stable_during_interaction() {
        let mut a = app();
        meeting(&mut a, "first");
        a.kind = Kind::Calendar;
        a.capture_source();
        a.popup = Some(window::Id::unique());
        a.snapshot.calendar.as_mut().unwrap().title = "Busy".into();
        assert_eq!(
            a.content().title,
            "Busy",
            "privacy updates must not be frozen"
        );
        meeting(&mut a, "second");
        assert_eq!(a.content().meta, "This activity has ended");
        let _ = a.update(Message::Dismiss);
        assert!(
            a.dismissed_calendar.is_none(),
            "cannot dismiss replacement event"
        );
        a.popup = None;
        a.kind = Kind::Music;
        a.snapshot.music_source = Some("player-a".into());
        a.capture_source();
        a.popup = Some(window::Id::unique());
        assert!(a.selected_source_matches());
        a.snapshot.music_source = Some("player-b".into());
        assert!(!a.selected_source_matches());
    }
    #[test]
    fn reactive_standard_panel_disable_is_not_masked_by_reveal_close() {
        let mut a = app();
        a.popup = Some(window::Id::unique());
        a.motion.retarget(1., false, Instant::now());
        a.reveal_deadline = Some(Instant::now() + Duration::from_secs(3));
        let _ = a.update(Message::PanelObserved(host::Observation {
            enabled: false,
            reveals_allowed: false,
            frame: host::Frame::default(),
        }));
        assert!(!a.active);
        assert!(a.popup.is_none());
        assert!(a.commands.is_none());
    }
    #[test]
    fn reactive_urgent_incidents_do_not_reannounce_every_percentage() {
        let mut a = app();
        let activity = system::Activity {
            source: system::Source::Power,
            title: "Battery critically low".into(),
            detail: "Connect power".into(),
            cue: "5%".into(),
            progress: None,
            urgent: true,
            announce: true,
            expires_ms: None,
        };
        a.snapshot.system = vec![activity.clone()];
        let mut next = a.snapshot.clone();
        next.system[0].cue = "4%".into();
        assert!(a.new_system_reveal(&next).is_none());
        next.system[0].title = "New incident".into();
        assert_eq!(
            a.new_system_reveal(&next),
            Some(Kind::System(system::Source::Power))
        );
        next.system[0].announce = false;
        assert!(a.new_system_reveal(&next).is_none());
    }
    #[test]
    fn reactive_transient_expiry_survives_a_backwards_wall_clock() {
        let mut a = app();
        let mut next = a.snapshot.clone();
        next.system = vec![system::Activity {
            source: system::Source::Audio,
            title: "Audio output changed".into(),
            detail: String::new(),
            cue: String::new(),
            progress: None,
            urgent: false,
            announce: true,
            expires_ms: Some(a.now_ms + 3000),
        }];
        a.observe_system_deadlines(&next, Instant::now());
        a.snapshot = next.clone();
        assert!(a.system_activity(system::Source::Audio).is_some());
        a.system_deadlines[0].2 = Instant::now() - Duration::from_secs(1);
        a.now_ms -= 3_600_000;
        a.observe_system_deadlines(&next, Instant::now());
        assert!(a.system_activity(system::Source::Audio).is_none());
    }
    #[test]
    fn reactive_preferences_are_default_on_bounded_and_scoped() {
        let p: preferences::Preferences = serde_json::from_str("{}").unwrap();
        assert!(p.system && p.media && p.calendar && p.reveals);
        assert_eq!(p.lead_ms(), 300_000);
        assert_eq!(
            preferences::Preferences {
                lead_minutes: 255,
                ..p
            }
            .lead_ms(),
            300_000
        );
        let mut a = app();
        meeting(&mut a, "first");
        a.snapshot.calendar.as_mut().unwrap().starts_at_ms = a.now_ms + 600_000;
        assert_eq!(a.automatic_kind(), Kind::Idle);
        a.snapshot.preferences.lead_minutes = 15;
        assert_eq!(a.automatic_kind(), Kind::Calendar);
        a.snapshot.preferences.calendar = false;
        assert_eq!(a.automatic_kind(), Kind::Idle);
    }
}

#[cfg(test)]
mod visual_tests {
    use super::*;
    #[test]
    fn reactive_system_and_meeting_views_keep_native_targets_and_readable_status() {
        let (mut a, _) = Applet::init(Core::default(), ());
        a.commands = Some(tokio::sync::mpsc::channel(2).0);
        a.snapshot.focus.key = "focus".into();
        a.active = true;
        a.popup = Some(window::Id::unique());
        a.motion.retarget(1., false, Instant::now());
        let mut ui = crate::native_test::Native::new(cosmic::iced::Size::new(WIDTH, HEIGHT));
        let root = std::env::var_os("HALCYON_ACTIVITY_EVIDENCE").map(std::path::PathBuf::from);
        for (name, source, title, detail, cue, progress) in [
            (
                "power",
                system::Source::Power,
                "Battery low",
                "Connect a power source",
                "9%",
                None,
            ),
            (
                "microphone",
                system::Source::Microphone,
                "Microphone in use",
                "Recorder",
                "Active",
                None,
            ),
            (
                "network",
                system::Source::Network,
                "VPN connected",
                "Work network",
                "",
                None,
            ),
            (
                "updates",
                system::Source::Updates,
                "Updating software",
                "Open the updater for details",
                "42%",
                Some(42),
            ),
        ] {
            a.kind = Kind::System(source);
            a.snapshot.system = vec![system::Activity {
                source,
                title: title.into(),
                detail: detail.into(),
                cue: cue.into(),
                progress,
                urgent: false,
                announce: false,
                expires_ms: None,
            }];
            assert!(ui.duplicate_ids(a.capsule(true)).is_empty());
            let action = ui.bounds(a.capsule(true), "activity-system-settings");
            assert!(action.y + action.height <= HEIGHT);
            if !cue.is_empty() {
                assert!(ui.text_bounds(a.capsule(true), cue).width > 0.);
            }
            for (theme_name, theme) in [
                ("dark", cosmic::Theme::dark()),
                ("light", cosmic::Theme::light()),
            ] {
                a.core.applet.background = if theme_name == "dark" {
                    cosmic::applet::cosmic_panel_config::CosmicPanelBackground::Dark
                } else {
                    cosmic::applet::cosmic_panel_config::CosmicPanelBackground::Light
                };
                if let Some(root) = &root {
                    std::fs::create_dir_all(root).unwrap();
                    let pixels = ui.render_scaled(a.capsule(true), &theme, 2.);
                    image::save_buffer(
                        root.join(format!("reactive-{name}-{theme_name}.png")),
                        &pixels,
                        WIDTH as u32 * 2,
                        HEIGHT as u32 * 2,
                        image::ColorType::Rgba8,
                    )
                    .unwrap();
                }
            }
        }
    }
}

/// Notifications: kept here, shown while Halcyon is live and the preference
/// is on, and revealed on arrival only when they replace COSMIC's popups.
impl Applet {
    pub(super) fn notes_shown(&self) -> bool {
        self.live() && self.snapshot.preferences.notifications
    }
    pub(super) fn note(&self, id: u32) -> Option<&notifications::Note> {
        self.notes.iter().find(|n| n.id == id)
    }
    /// An app icon for a notification, the kind's symbol otherwise.
    pub(super) fn kind_icon<'a>(&self, kind: Kind, size: u16) -> Element<'a, Message> {
        if let Kind::Notification(id) = kind {
            return self.note_icon(self.note(id), size);
        }
        widget::icon::from_name(kind.icon())
            .symbolic(true)
            .size(size)
            .into()
    }
    pub(super) fn forget_note(&mut self, id: u32) {
        self.notes.retain(|n| n.id != id);
        self.pending.retain(|p| *p != id);
        if self.kind == Kind::Notification(id) && self.popup.is_none() {
            self.kind = self.automatic_kind();
        }
    }
    pub(super) fn note_event(&mut self, event: notifications::Event, now: Instant) -> Task<Message> {
        match event {
            notifications::Event::Ready(ready) => {
                self.notes_ready = ready;
                self.sync_popups()
            }
            notifications::Event::Closed(id) | notifications::Event::Expired(id) => {
                let alive = matches!(event, notifications::Event::Expired(_));
                let shown = self.kind == Kind::Notification(id) && self.popup.is_some();
                if let Some(note) = self.note(id).cloned() {
                    self.remember(note, alive);
                } else if !alive {
                    // Gone from the server: Recent can no longer open it.
                    for r in self.recent.iter_mut().filter(|r| r.note.id == id) {
                        r.alive = false;
                    }
                }
                self.forget_note(id);
                self.arm_expiry();
                if shown {
                    match self
                        .eligible_kinds()
                        .into_iter()
                        .find(|k| matches!(k, Kind::Notification(_)))
                    {
                        Some(next) => self.kind = next,
                        None => return self.update(Message::Close),
                    }
                }
                Task::none()
            }
            notifications::Event::Notify(mut note) => {
                if self.live() && !self.snapshot.preferences.notifications {
                    return Task::none();
                }
                note.at_ms = chrono::Utc::now().timestamp_millis();
                let id = note.id;
                // A replaced notification is live again, not recent.
                self.recent.retain(|r| r.note.id != id);
                if self.rule(&note) == Some(preferences::AppRule::Mute) {
                    self.forget_note(id);
                    self.muted += 1;
                    return Task::none();
                }
                let before = self.notes.iter().position(|n| n.id == id);
                match progress_step(before.map(|i| self.notes[i].progress), note.progress) {
                    Step::Plain | Step::Start => {}
                    step @ (Step::Update | Step::Finish) => {
                        // In place: same row, same group, no new reveal.
                        let index = before.expect("a step follows a known note");
                        self.notes[index] = note;
                        self.arm_expiry();
                        // The live activity's Done card stands for the end.
                        let done_shown = self
                            .ui
                            .jobs
                            .for_note(id)
                            .is_some_and(|j| j.done_at.is_some());
                        if step == Step::Finish && !done_shown {
                            return self.reveal_note(id, now);
                        }
                        return Task::none();
                    }
                }
                self.notes.retain(|n| n.id != id);
                self.notes.insert(0, note);
                // The oldest give way to Recent, still open on the server.
                while self.notes.len() > notifications::LIMIT {
                    let old = self.notes.pop().expect("more than the limit");
                    self.remember(old, true);
                }
                self.arm_expiry();
                let notes = &self.notes;
                self.pending.retain(|p| notes.iter().any(|n| n.id == *p));
                if self.popup.is_none() {
                    self.kind = self.automatic_kind();
                }
                self.reveal_note(id, now)
            }
        }
    }
    /// Reveals stand in for COSMIC's popups: off with them, or by choice.
    /// An app set to Always show is revealed either way.
    fn note_reveals(&self) -> bool {
        self.notes_shown() && self.host_reveals
    }
    /// Whether this notification may be revealed on arrival, before Do Not
    /// Disturb has its say.
    pub(super) fn reveals(&self, note: &notifications::Note) -> bool {
        if !self.note_reveals() {
            return false;
        }
        match self.rule(note) {
            Some(preferences::AppRule::Always) => true,
            Some(_) => false,
            None => {
                !note.low && self.popups == Some(true) && self.snapshot.preferences.reveals
            }
        }
    }
    /// The critical notification an automatic reveal is showing.
    pub(super) fn showing_critical(&self) -> Option<u32> {
        match self.kind {
            Kind::Notification(id)
                if self.popup.is_some()
                    && self.reveal_deadline.is_some()
                    && !self.chooser
                    && self.batch.len() <= SUMMARY_AFTER
                    && self.note(id).is_some_and(|n| n.urgent) =>
            {
                Some(id)
            }
            _ => None,
        }
    }
    /// The showing reveal stands for this many notifications at once.
    pub(super) fn summary(&self) -> Option<usize> {
        (self.batch.len() > SUMMARY_AFTER
            && !self.chooser
            && matches!(self.kind, Kind::Notification(_)))
        .then_some(self.batch.len())
    }
    pub(super) fn queue_note(&mut self, id: u32) {
        self.pending.retain(|p| *p != id);
        self.pending.push(id);
        if self.pending.len() > notifications::LIMIT {
            self.pending.remove(0);
        }
    }
    /// Try the waiting notifications at `at`, or sooner if already asked.
    pub(super) fn arm_pending(&mut self, at: Instant) {
        if !self.pending.is_empty() {
            self.pending_at = Some(self.pending_at.map_or(at, |old| old.min(at)));
        }
    }
    /// Opened by hand, the island shows what was waiting.
    pub(super) fn seen_pending(&mut self) {
        self.pending.clear();
        self.pending_at = None;
    }
    /// Drop what may no longer show; critical notifications go first.
    fn tidy_pending(&mut self) {
        let dnd = self.snapshot.dnd;
        let keep: Vec<u32> = self
            .pending
            .iter()
            .copied()
            .filter(|id| {
                self.note(*id)
                    .is_some_and(|n| self.reveals(n) && (n.urgent || !dnd))
            })
            .collect();
        self.pending = keep;
        let urgent = |id: &u32| self.notes.iter().any(|n| n.id == *id && n.urgent);
        let (mut first, rest): (Vec<u32>, Vec<u32>) =
            self.pending.iter().copied().partition(urgent);
        first.extend(rest);
        self.pending = first;
    }
    fn blocked(&self) -> bool {
        self.popup.is_some() || self.hovered || self.held || self.snapshot.quiet != Some(false)
    }
    fn throttled(&self, now: Instant) -> Option<Instant> {
        self.last_reveal
            .map(|at| at + NOTE_GAP)
            .filter(|end| *end > now)
    }
    /// A brief look at a new notification, like the popup it replaces.
    /// Low urgency never shows; Do Not Disturb lets only critical ones by.
    /// Anything held back by fullscreen, the lock screen, an open island or
    /// the pointer waits in `pending`.
    fn reveal_note(&mut self, id: u32, now: Instant) -> Task<Message> {
        let Some(note) = self.note(id) else {
            return Task::none();
        };
        let urgent = note.urgent;
        if !self.reveals(note) || (self.snapshot.dnd && !urgent) {
            return Task::none();
        }
        let kind = Kind::Notification(id);
        if self.popup.is_some()
            && self.reveal_deadline.is_some()
            && !self.chooser
            && (urgent || self.showing_critical().is_none())
        {
            // A reveal already showing moves on to the newest notification;
            // a burst reads as one summary. A critical one stands alone, and
            // one it replaces waits to be seen.
            if urgent && let Some(old) = self.showing_critical().filter(|old| *old != id) {
                self.queue_note(old);
            }
            self.kind = kind;
            if urgent {
                self.batch = vec![id];
            } else {
                self.batch.retain(|b| *b != id);
                self.batch.push(id);
            }
            self.reveal_deadline = Some(now + NOTE_REVEAL);
            return Task::none();
        }
        if self.blocked() {
            self.queue_note(id);
            return Task::none();
        }
        if !urgent && let Some(end) = self.throttled(now) {
            self.queue_note(id);
            self.arm_pending(end);
            return Task::none();
        }
        self.show_note(id, vec![id], now)
    }
    fn show_note(&mut self, id: u32, batch: Vec<u32>, now: Instant) -> Task<Message> {
        self.kind = Kind::Notification(id);
        self.batch = batch;
        self.last_reveal = Some(now);
        self.reveal_deadline = Some(now + NOTE_REVEAL);
        self.revealed = true;
        if self.popup.is_none() {
            self.open_popup()
        } else {
            Task::none()
        }
    }
    /// Shows what waited: critical ones one by one, then more than
    /// `SUMMARY_AFTER` as one summary, otherwise each in turn.
    pub(super) fn flush_pending(&mut self, now: Instant) -> Task<Message> {
        self.tidy_pending();
        if self.pending.is_empty() || !self.note_reveals() {
            self.pending.clear();
            return Task::none();
        }
        if self.blocked() {
            // Leaving, closing or a quiet change asks again.
            return Task::none();
        }
        if let Some(end) = self.throttled(now) {
            self.arm_pending(end);
            return Task::none();
        }
        self.next_pending_unchecked(now)
    }
    /// The next waiting notification in place of the reveal that just ended.
    pub(super) fn next_pending(&mut self, now: Instant) -> Option<Task<Message>> {
        self.tidy_pending();
        if self.pending.is_empty()
            || !self.note_reveals()
            || self.snapshot.quiet != Some(false)
            || self.summary().is_some()
        {
            return None;
        }
        Some(self.next_pending_unchecked(now))
    }
    fn next_pending_unchecked(&mut self, now: Instant) -> Task<Message> {
        let first = self.pending[0];
        let critical = self.note(first).is_some_and(|n| n.urgent);
        if !critical && self.pending.len() > SUMMARY_AFTER {
            let newest = self.pending[self.pending.len() - 1];
            let batch = std::mem::take(&mut self.pending);
            return self.show_note(newest, batch, now);
        }
        self.pending.remove(0);
        self.show_note(first, vec![first], now)
    }
    /// COSMIC's popups are off exactly while the island can show
    /// notifications in their place.
    pub(super) fn sync_popups(&mut self) -> Task<Message> {
        let take = self.live() && self.notes_ready && self.snapshot.preferences.takes_popups();
        if self.popups == Some(take) {
            return Task::none();
        }
        self.popups = Some(take);
        cosmic::task::future(async move { Message::PopupsSet(super::popups::set(take).await) })
    }
}
pub(super) const NOTE_REVEAL: Duration = Duration::from_secs(5);
/// Between two notification reveals, so a stream of them doesn't flicker.
pub(super) const NOTE_GAP: Duration = Duration::from_secs(2);
/// More than this many notifications at once read as one summary.
pub(super) const SUMMARY_AFTER: usize = 3;
/// After the island closes, before what waited shows.
pub(super) const PENDING_PAUSE: Duration = Duration::from_millis(800);
