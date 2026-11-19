//! Desktop configuration writes, off the UI thread and in order.
//!
//! The edge menu, the tools and the island change settings from the desktop
//! process. Each write takes the shared configuration lock, which another
//! process (Settings, a Look, the controller) may hold for a while. Waiting
//! for it on the UI thread froze the desktop, so these writes run one at a
//! time on a writer thread. The UI keeps answering and applies the saved
//! configuration, or shows the failure, when the write returns.
use super::*;
use std::sync::mpsc;

type Job = Box<dyn FnOnce() + Send>;

/// One writer thread for the process, so writes land in the order asked.
fn writer() -> &'static std::sync::Mutex<mpsc::Sender<Job>> {
    static WRITER: std::sync::OnceLock<std::sync::Mutex<mpsc::Sender<Job>>> =
        std::sync::OnceLock::new();
    WRITER.get_or_init(|| {
        let (sender, jobs) = mpsc::channel::<Job>();
        std::thread::Builder::new()
            .name("halcyon-config-writer".into())
            .spawn(move || {
                for job in jobs {
                    job();
                }
            })
            .expect("spawn the configuration writer");
        std::sync::Mutex::new(sender)
    })
}

/// Runs `job` on the writer thread, after every write queued before it.
pub(super) fn in_order(job: impl FnOnce() + Send + 'static) {
    let sent = writer()
        .lock()
        .map_err(|_| ())
        .and_then(|sender| sender.send(Box::new(job)).map_err(|_| ()));
    if sent.is_err() {
        eprintln!("The settings writer stopped");
    }
}

/// What the desktop shows when a background write fails.
#[derive(Debug, Clone)]
pub struct Failure {
    pub title: std::borrow::Cow<'static, str>,
    pub body: std::borrow::Cow<'static, str>,
    pub retry: Option<Box<Message>>,
}

impl Failure {
    pub fn notice(
        title: impl Into<std::borrow::Cow<'static, str>>,
        body: impl Into<std::borrow::Cow<'static, str>>,
        retry: Option<Message>,
    ) -> Self {
        Self {
            title: title.into(),
            body: body.into(),
            retry: retry.map(Box::new),
        }
    }
}

/// The worker writes in submission order, but its individual tasks can deliver
/// their replies in a different order. Keep the latest confirmed configuration.
#[derive(Default)]
pub(super) struct Writes {
    submitted: u64,
    confirmed: u64,
    pub pending: usize,
}

/// A finished background write.
#[derive(Debug, Clone)]
pub struct Saved {
    sequence: u64,
    pub result: Result<Box<Config>, String>,
    pub failure: Failure,
    /// Runs on the UI thread before a failure is shown, to undo a change the
    /// desktop already showed.
    pub revert: Option<Revert>,
    /// It failed because the settings file is damaged, so trying again
    /// can't work until Settings repairs it.
    pub damaged: bool,
}

/// A change the desktop showed ahead of its write, put back on failure.
#[derive(Debug, Clone)]
pub enum Revert {
    WidgetOrder(Vec<String>),
}

impl Cards {
    /// Queues `write` behind earlier writes and answers with its result. The
    /// action label and the caller's lock policy (Arrange refuses contended
    /// locks) travel with the write.
    pub(super) fn save_config_later(
        &mut self,
        label: &'static str,
        failure: Failure,
        revert: Option<Revert>,
        write: impl FnOnce() -> Result<Config, String> + Send + 'static,
    ) -> Task<Message> {
        self.config_writes.submitted += 1;
        self.config_writes.pending += 1;
        let sequence = self.config_writes.submitted;
        #[cfg(test)]
        QUEUED.with(|queued| queued.set(queued.get() + 1));
        let nonblocking = crate::config::settings_lock_operation() & libc::LOCK_NB != 0;
        let (reply, answer) = tokio::sync::oneshot::channel();
        let job: Job = Box::new(move || {
            let run = || crate::desktop::actions::in_action(label, write);
            let result = if nonblocking {
                crate::config::with_nonblocking_locks(run)
            } else {
                run()
            };
            let damaged = result.is_err()
                && Config::path()
                    .as_deref()
                    .and_then(crate::config::damaged_file)
                    .is_some();
            let _ = reply.send((result, damaged));
        });
        // Queued now, not when the task first runs, so order is kept.
        let queued = writer()
            .lock()
            .map_err(|_| "The settings writer stopped".to_string())
            .and_then(|sender| {
                sender
                    .send(job)
                    .map_err(|_| "The settings writer stopped".to_string())
            });
        cosmic::task::future(async move {
            let (result, damaged) = match queued {
                Ok(()) => answer
                    .await
                    .unwrap_or_else(|_| (Err("The settings writer stopped".into()), false)),
                Err(error) => (Err(error), false),
            };
            Message::ConfigSaved(Box::new(Saved {
                sequence,
                result: result.map(Box::new),
                failure,
                revert,
                damaged,
            }))
        })
    }

    pub(super) fn config_saved(&mut self, saved: Saved) -> Task<Message> {
        self.config_writes.pending -= 1;
        let superseded = saved.sequence < self.config_writes.confirmed;
        match saved.result {
            Ok(_) if superseded => Task::none(),
            Ok(config) => {
                self.config_writes.confirmed = saved.sequence;
                self.apply_config(*config)
            }
            Err(error) => {
                match saved.revert {
                    Some(Revert::WidgetOrder(order)) if !superseded => {
                        self.config.widget_order = order;
                    }
                    _ => {}
                }
                eprintln!("{}: {error}", saved.failure.title);
                if saved.damaged {
                    // Try again can't help; the parser's words stay under
                    // Details.
                    self.raise_notice(
                        "Halcyon’s settings file is damaged",
                        format!("Your desktop still works. {}", saved.failure.body),
                        error,
                        None,
                    );
                    if let Some(notice) = &mut self.notice {
                        notice.repair = true;
                    }
                } else {
                    self.raise_notice(
                        saved.failure.title,
                        saved.failure.body,
                        error,
                        saved.failure.retry.map(|message| *message),
                    );
                }
                Task::none()
            }
        }
    }
}

/// Waits for every queued write, in a test.
#[cfg(test)]
pub(crate) fn flush() {
    let (done, wait) = mpsc::channel();
    in_order(move || {
        let _ = done.send(());
    });
    wait.recv().unwrap();
}

/// Runs a desktop task to the end in a test, feeding its messages back.
#[cfg(test)]
pub(crate) fn finish(cards: &mut Cards, task: Task<Message>) {
    use cosmic::iced::futures::StreamExt;
    use cosmic::iced::runtime::Action as RuntimeAction;
    let mut pending = vec![task];
    while let Some(task) = pending.pop() {
        let Some(mut stream) = cosmic::iced::runtime::task::into_stream(task) else {
            continue;
        };
        cosmic::iced::futures::executor::block_on(async {
            while let Some(action) = stream.next().await {
                if let RuntimeAction::Output(cosmic::Action::App(message)) = action {
                    pending.push(cards.update(message));
                }
            }
        });
    }
}

#[cfg(test)]
thread_local! {
    /// Writes queued from this thread, so a test knows a task carries one.
    static QUEUED: std::cell::Cell<u64> = const { std::cell::Cell::new(0) };
}
/// Runs one desktop update in a test, letting the settings write it queued
/// (if any) land. A task that queued none is dropped untouched, so nothing
/// it would launch is started.
#[cfg(test)]
pub(crate) fn update_saved(cards: &mut Cards, message: Message) {
    let before = QUEUED.with(std::cell::Cell::get);
    let task = cards.update(message);
    if QUEUED.with(std::cell::Cell::get) != before {
        settle(cards, task);
    }
}
/// Lets queued writes finish and feeds back what a desktop task has ready,
/// in a test. Unlike `finish` it never waits on the rest of the task (a
/// focus or surface request that only a running desktop would answer).
#[cfg(test)]
pub(crate) fn settle(cards: &mut Cards, task: Task<Message>) {
    use cosmic::iced::futures::{FutureExt, StreamExt};
    use cosmic::iced::runtime::Action as RuntimeAction;
    flush();
    let Some(mut stream) = cosmic::iced::runtime::task::into_stream(task) else {
        return;
    };
    while let Some(Some(action)) = stream.next().now_or_never() {
        if let RuntimeAction::Output(cosmic::Action::App(message)) = action {
            let next = cards.update(message);
            settle(cards, next);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::displays::test_support::isolated;

    fn cards() -> Cards {
        let mut config = Config::load();
        config.enabled_widgets = vec!["clock".into(), "weather".into(), "calendar".into()];
        let (cards, _) = Cards::init(Core::default(), config);
        cards
    }

    /// Holds the shared configuration lock from another thread.
    struct Held(
        Option<mpsc::Sender<()>>,
        Option<std::thread::JoinHandle<()>>,
    );
    impl Held {
        fn new() -> Self {
            let path = Config::path().unwrap();
            let (locked, ready) = mpsc::channel();
            let (release, wait) = mpsc::channel::<()>();
            let thread = std::thread::spawn(move || {
                let lock = crate::config::lock_config(&path).unwrap();
                locked.send(()).unwrap();
                // Released by drop, or after a second so a blocking UI
                // dispatch fails the timing check instead of hanging.
                let _ = wait.recv_timeout(Duration::from_secs(1));
                drop(lock);
            });
            ready.recv().unwrap();
            Self(Some(release), Some(thread))
        }
    }
    impl Drop for Held {
        fn drop(&mut self) {
            self.0.take();
            if let Some(thread) = self.1.take() {
                thread.join().unwrap();
            }
        }
    }

    #[test]
    fn d1_edge_menu_writes_never_wait_on_the_ui_thread() {
        if isolated("app::config_writes::tests::d1_edge_menu_writes_never_wait_on_the_ui_thread") {
            return;
        }
        let mut cards = cards();
        let held = Held::new();
        let messages = [
            Message::SetWidgetVisible(Widget::Weather, false),
            Message::SetWidgetsVisible(false),
            Message::SetWeatherEffect(true),
            Message::SaveOrder(vec![Widget::Projects, Widget::Devbox, Widget::Weather]),
            Message::Tool(crate::tools::cards::Action::FocusMinutes(40)),
        ];
        let mut tasks = Vec::new();
        for message in messages {
            let start = Instant::now();
            tasks.push(cards.update(message));
            assert!(
                start.elapsed() < Duration::from_millis(100),
                "the UI thread waited {:?} for another settings writer",
                start.elapsed()
            );
        }
        // The order shows at once and nothing was saved yet.
        assert_eq!(
            cards.config.ordered_right_cards(),
            vec![Widget::Projects, Widget::Devbox, Widget::Weather]
        );
        assert!(cards.config.widget_visible(Widget::Weather));
        drop(held);
        for task in tasks {
            finish(&mut cards, task);
        }
        let saved = Config::load();
        assert!(!saved.widget_visible(Widget::Weather));
        assert!(!saved.widgets_visible);
        assert!(saved.rain.enabled);
        assert_eq!(saved.focus.focus_minutes, 40);
        assert!(!cards.config.widget_visible(Widget::Weather));
        assert!(!cards.config.widgets_visible);
        assert!(cards.notice.is_none());
    }

    #[test]
    fn d1_writes_land_in_the_order_asked() {
        if isolated("app::config_writes::tests::d1_writes_land_in_the_order_asked") {
            return;
        }
        let mut cards = cards();
        let held = Held::new();
        let tasks: Vec<_> = [true, false, true, false]
            .into_iter()
            .map(|on| cards.update(Message::SetWidgetVisible(Widget::Calendar, on)))
            .collect();
        drop(held);
        for task in tasks {
            finish(&mut cards, task);
        }
        assert!(!Config::load().widget_visible(Widget::Calendar));
        assert!(!cards.config.widget_visible(Widget::Calendar));
    }

    #[test]
    fn d1_a_failed_reorder_puts_the_saved_order_back() {
        if isolated("app::config_writes::tests::d1_a_failed_reorder_puts_the_saved_order_back") {
            return;
        }
        let mut cards = cards();
        let before = cards.config.ordered_right_cards();
        let path = Config::path().unwrap();
        std::fs::write(&path, "{broken").unwrap();
        let mut order = before.clone();
        order.reverse();
        let task = cards.update(Message::SaveOrder(order.clone()));
        assert_eq!(cards.config.ordered_right_cards(), order);
        finish(&mut cards, task);
        assert_eq!(cards.config.ordered_right_cards(), before);
        // A damaged file says so, and that the order went back.
        let notice = cards.notice.as_ref().expect("the failure is shown");
        assert_eq!(notice.title, "Halcyon’s settings file is damaged");
        assert_eq!(
            notice.body,
            "Your desktop still works. The widgets are back in their saved order."
        );
        assert!(notice.repair && notice.retry.is_none());
    }

    #[test]
    fn fail08_a_mute_that_isnt_saved_says_so_with_try_again() {
        if isolated(
            "app::config_writes::tests::fail08_a_mute_that_isnt_saved_says_so_with_try_again",
        ) {
            return;
        }
        use crate::activity_island::bridge::{Command, Event};
        use crate::activity_island::preferences::AppRule;
        let mut cards = cards();
        let path = Config::path().unwrap();
        let saved = std::fs::read(&path).unwrap();
        // A file that can't be opened right now (here a link, which
        // settings never follow) isn't damaged, so Try again is offered.
        let elsewhere = path.with_file_name("elsewhere.json");
        std::fs::write(&elsewhere, &saved).unwrap();
        std::fs::remove_file(&path).unwrap();
        std::os::unix::fs::symlink(&elsewhere, &path).unwrap();
        let mute = Command::NoteRule {
            app: "Firefox".into(),
            entry: "firefox".into(),
            rule: Some(AppRule::Mute),
        };
        let task = cards.update(Message::Activity(Event::Command(mute)));
        finish(&mut cards, task);
        // The island stops waiting for the rule; the edge menu says why the
        // notifications keep coming.
        let notice = cards.notice.as_ref().expect("the failure is shown");
        assert_eq!(notice.title, "Couldn’t mute Firefox");
        assert_eq!(notice.body, "Notifications from Firefox will still show.");
        assert!(notice.retry.is_some() && !notice.brief && !notice.repair);
        assert!(cards.sidebar.visible);
        // Try again saves it once the settings can be written.
        std::fs::remove_file(&path).unwrap();
        std::fs::write(&path, &saved).unwrap();
        let task = cards.update(Message::NoticeRetry);
        finish(&mut cards, task);
        assert!(cards.notice.is_none());
        assert_eq!(
            Config::load()
                .activity_island
                .rule_for("Firefox", "firefox"),
            Some(AppRule::Mute)
        );
    }
}
