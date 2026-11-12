//! Restarting the desktop widgets into an updated program.
//!
//! A package upgrade replaces `/usr/bin/halcyon` under the running widgets
//! (see `crate::upgrade`). Once nothing on the desktop is in use, the process
//! exits with `RESTART_STATUS` and `halcyon.service` starts the new program.
//! What the widgets show comes back from disk: positions and settings from
//! the config, the focus session and the island's timer from their own
//! files, notes from the note file. What lives only here (a note being
//! typed, an arrangement, a drag, an open menu, a change that could not be
//! saved yet) is waited for, never dropped.
//!
//! Only the service starts the widgets again, so a copy started some other
//! way (from a terminal) keeps running what it runs.
use super::*;
use crate::upgrade::{Ledger, Plan, Program};

/// The session ledger of the widgets' restarts into updates. Each process
/// restarts into an update once, so the Activity pill keeps its own: one of
/// them restarting often must not keep the other on the old version.
const RESTARTS: &str = "desktop-widgets";
/// How long the desktop must be left alone before restarting into an
/// update. Longer than the pill's: every card goes away for a moment.
pub(super) const RESTART_QUIET: Duration = Duration::from_secs(10);

/// When the pointer last moved over any of the widgets' surfaces. Motion
/// over the cards sends no message (it must not rebuild surfaces), so it is
/// noted where the event arrives.
pub(super) static POINTER_MOVED: std::sync::Mutex<Option<Instant>> = std::sync::Mutex::new(None);

#[derive(Default)]
pub(super) struct Restart {
    /// A working update replaced the program: the new one, and the earliest
    /// moment to restart into it.
    pub updated: Option<(Program, Instant)>,
    /// When to restart. Armed only while nothing is in use.
    pub at: Option<Instant>,
}

/// Something the person did on the desktop: it puts a restart off for
/// another quiet moment.
pub(super) fn input(message: &Message) -> bool {
    matches!(
        message,
        Message::SurfaceMessage(..)
            | Message::SurfaceInput(..)
            | Message::Pointer(..)
            | Message::KeyInput(..)
            | Message::Hover(..)
            | Message::DrawerToggle
            | Message::FocusCommand(_)
            | Message::WidgetMenu(_)
            | Message::WidgetMenuPick(_)
            | Message::Tool(_)
    )
}

impl Cards {
    /// A working update replaced the program. `plan` is the ledger's;
    /// `in_service`: the service will start the widgets again.
    pub(super) fn updated(&mut self, new: Program, plan: Plan, in_service: bool, now: Instant) {
        if !in_service {
            eprintln!(
                "Halcyon was updated; these desktop widgets were not started by {}, so they keep running the old version until started again",
                crate::paths::SERVICE
            );
            return;
        }
        match plan {
            Plan::Restart { after } => self.restart.updated = Some((new, now + after)),
            Plan::Stay => eprintln!(
                "Halcyon was updated, but the desktop widgets restarted often lately; they keep the running version until the next sign-in"
            ),
        }
    }

    /// Nothing on the desktop is in use or waiting to be saved: no drag,
    /// arrangement, open edge menu or widget menu, no note being written,
    /// no change still being applied or waiting for Retry.
    pub(super) fn restart_idle(&self) -> bool {
        let sidebar = &self.sidebar;
        sidebar.drag.is_none()
            && sidebar.focus.is_none()
            && !sidebar.visible
            && !sidebar.hovered
            && !sidebar.edge_hovered
            && !sidebar.animating()
            && sidebar.next_transition().is_none()
            && !self.controls_open
            && !self.layout_editor.active
            && self.paused_layout.is_none()
            && self.pending_arrange.is_none()
            && self.arrange_display.is_none()
            && self.menu.is_none()
            && self.calendar_details.is_none()
            && self.notice.is_none()
            && self.tools.settled()
            && !self.automator.busy()
            && self.applying_colors == 0
    }

    /// Arms the restart into an update once the desktop has been left alone
    /// for `RESTART_QUIET`, and disarms it as soon as it is in use again.
    pub(super) fn arm_restart(&mut self, now: Instant) {
        let Some((_, earliest)) = &self.restart.updated else {
            return;
        };
        if self.restart_idle() {
            let earliest = *earliest;
            self.restart
                .at
                .get_or_insert_with(|| (now + RESTART_QUIET).max(earliest));
        } else {
            self.restart.at = None;
        }
    }

    /// Whether to restart now. `moved`: when the pointer last moved over
    /// the widgets.
    pub(super) fn restart_due(&self, now: Instant, moved: Option<Instant>) -> bool {
        self.restart.updated.is_some()
            && self.restart_idle()
            && self.restart.at.is_some_and(|at| now >= at)
            && moved.is_none_or(|moved| now >= moved + RESTART_QUIET)
    }

    /// The armed moment came, `early` before the monotonic clock reached it.
    pub(super) fn restart_wake(&mut self, early: Duration) {
        let now = Instant::now() + early;
        let moved = POINTER_MOVED.lock().ok().and_then(|moved| *moved);
        if self.restart_due(now, moved) {
            if let Some((new, _)) = self.restart.updated.take() {
                // Returns only when the restart cannot be recorded.
                crate::upgrade::restart(&Ledger::session(RESTARTS), &new, "the desktop widgets");
                self.restart.at = None;
            }
        } else if let (Some(at), Some(moved)) = (self.restart.at, moved) {
            // The pointer is still about: a quiet moment after it stops.
            self.restart.at = Some(at.max(moved + RESTART_QUIET));
        }
    }

    pub(super) fn restart_subscription(&self) -> Subscription<Message> {
        let mut all = vec![crate::upgrade::watch().map(Message::Updated)];
        if let Some(at) = self.restart.at {
            all.push(crate::schedule::wake_at(at).map(Message::RestartDue));
        }
        Subscription::batch(all)
    }

    pub(super) fn restart_ledger_plan(new: &Program) -> Plan {
        Ledger::session(RESTARTS).plan(new)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use cosmic::widget::text_editor;

    fn cards() -> Cards {
        let (cards, _) = Cards::init(Core::default(), Config::default());
        cards
    }

    /// An update waiting to be restarted into, as `updated` leaves it.
    /// Never through `Message::Updated`: that reads this session's ledger.
    fn waiting(cards: &mut Cards, earliest: Instant) {
        let new = Program::running().expect("the test binary is a program");
        cards.restart.updated = Some((new, earliest));
    }

    #[test]
    fn update_restart_waits_for_quiet_widgets() {
        let now = Instant::now();
        let mut cards = cards();
        assert!(cards.restart_idle(), "a fresh desktop is idle");

        // No update: nothing is armed.
        cards.arm_restart(now);
        assert_eq!(cards.restart.at, None);
        assert!(!cards.restart_due(now + RESTART_QUIET * 10, None));

        // An update arms a quiet moment, which later arms do not push back.
        waiting(&mut cards, now);
        cards.arm_restart(now);
        assert_eq!(cards.restart.at, Some(now + RESTART_QUIET));
        cards.arm_restart(now + Duration::from_secs(3));
        assert_eq!(cards.restart.at, Some(now + RESTART_QUIET));
        assert!(!cards.restart_due(now + Duration::from_secs(3), None));
        assert!(cards.restart_due(now + RESTART_QUIET, None));

        // The pointer moving over the cards keeps it off for a quiet moment.
        let moved = now + Duration::from_secs(8);
        assert!(!cards.restart_due(now + RESTART_QUIET, Some(moved)));
        assert!(cards.restart_due(moved + RESTART_QUIET, Some(moved)));
    }

    #[test]
    fn update_restart_keeps_the_ledger_back_off() {
        let now = Instant::now();
        let mut cards = cards();
        let earliest = now + Duration::from_secs(120);
        waiting(&mut cards, earliest);
        cards.arm_restart(now);
        assert_eq!(cards.restart.at, Some(earliest));
        assert!(!cards.restart_due(now + RESTART_QUIET, None));
        assert!(cards.restart_due(earliest, None));
    }

    #[test]
    fn update_restart_waits_for_whatever_is_in_use() {
        let now = Instant::now();
        let busy: [(&str, fn(&mut Cards)); 8] = [
            ("the drawer", |cards| cards.sidebar.visible = true),
            ("the drawer opened from the keyboard", |cards| {
                cards.sidebar.focus = Some(0)
            }),
            ("the widget controls", |cards| cards.controls_open = true),
            ("the layout editor", |cards| {
                cards.layout_editor.active = true
            }),
            ("a widget menu", |cards| {
                cards.menu = Some((window::Id::unique(), Widget::Clock))
            }),
            ("a notice waiting for Retry", |cards| {
                cards.notice = Some(Notice {
                    title: "Could not save",
                    body: "",
                    error: String::new(),
                    retry: None,
                    details: false,
                })
            }),
            ("a note being written", |cards| {
                cards.tools.draft = Some(tools::NoteDraft {
                    content: text_editor::Content::with_text("unsaved"),
                    base: String::new(),
                    typing: true,
                    conflicted: false,
                    saving: false,
                    on_demand: false,
                })
            }),
            ("wallpaper colours being applied", |cards| {
                cards.applying_colors = 1
            }),
        ];
        for (what, set) in busy {
            let mut cards = cards();
            waiting(&mut cards, now);
            cards.arm_restart(now);
            assert!(cards.restart.at.is_some(), "{what}: armed while idle");

            set(&mut cards);
            assert!(!cards.restart_idle(), "{what} is in use");
            assert!(
                !cards.restart_due(now + RESTART_QUIET * 10, None),
                "{what}: never restarted under it"
            );
            cards.arm_restart(now + Duration::from_secs(1));
            assert_eq!(cards.restart.at, None, "{what}: disarmed");
            assert!(cards.restart.updated.is_some(), "{what}: still waiting");
        }
    }

    #[test]
    fn only_the_service_restarts_the_widgets() {
        let now = Instant::now();
        let new = || Program::running().expect("the test binary is a program");
        let after = Duration::from_secs(30);

        let mut cards = cards();
        cards.updated(new(), Plan::Restart { after }, false, now);
        assert!(
            cards.restart.updated.is_none(),
            "nobody would start them again"
        );

        cards.updated(new(), Plan::Stay, true, now);
        assert!(cards.restart.updated.is_none(), "the ledger says stay");

        cards.updated(new(), Plan::Restart { after }, true, now);
        assert_eq!(
            cards.restart.updated.as_ref().map(|(_, at)| *at),
            Some(now + after)
        );
        cards.arm_restart(now);
        assert_eq!(cards.restart.at, Some(now + after));
    }

    #[test]
    fn using_the_desktop_puts_the_restart_off() {
        assert!(input(&Message::DrawerToggle));
        assert!(input(&Message::WidgetMenu(Widget::Clock)));
        assert!(!input(&Message::RestartDue(Duration::ZERO)));
        assert!(!input(&Message::WallpaperColorsApplied(Ok(()))));
    }
}
