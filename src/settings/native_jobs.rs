//! Blocking add-on checks and native writes run on the task executor. The UI
//! keeps a display snapshot; the worker owns the model and any live lease.
use super::*;
use crate::desktop::islands::Installation;
use std::sync::{Arc, Mutex};

pub struct Delivery(Arc<Mutex<Option<Done>>>);
impl Clone for Delivery {
    fn clone(&self) -> Self {
        Self(self.0.clone())
    }
}
impl std::fmt::Debug for Delivery {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Native completion")
    }
}
pub enum Done {
    Desktop(desktop::DesktopEditor, Option<cosmic::widget::Id>, bool),
    Profiles(profiles::ProfilesEditor, profiles::Outcome),
    Wizard(wizard::Wizard, wizard::Outcome),
    Undo(
        Result<Option<String>, String>,
        Option<crate::desktop::store::UndoReview>,
    ),
    Material(Result<(), String>),
    Recovery(legacy::Recovery, Result<(), String>),
    Canvas(u64, Result<Config, String>),
    Design(Result<String, String>),
    Recipes(recipes::Editor, Result<Option<Config>, String>),
    Failed(String),
}
fn task(run: impl FnOnce() -> Done + Send + 'static) -> Task<Message> {
    cosmic::task::future(async move {
        let done = tokio::task::spawn_blocking(run)
            .await
            .unwrap_or_else(|e| Done::Failed(e.to_string()));
        Message::NativeDone(Delivery(Arc::new(Mutex::new(Some(done)))))
    })
}
impl Settings {
    pub(super) fn native_worker_needed(&self) -> bool {
        crate::desktop::DesktopStore::from_env().is_ok_and(|s| Installation::needs_worker(&s))
    }
    fn begin_native(&mut self) {
        self.native_busy = true;
        self.status = "Applying desktop settings…".into();
        self.error = false;
    }
    pub(super) fn dispatch_native(&mut self, message: &Message) -> Option<Task<Message>> {
        if self.native_busy
            && matches!(
                message,
                Message::Profiles(profiles::Message::Package(_) | profiles::Message::RemixSelected)
            )
        {
            return Some(Task::none());
        }
        if let Message::Wizard(edit) = message
            && self.wizard.as_ref().is_none_or(|w| !w.accepts(edit))
        {
            return Some(Task::none());
        }
        // Pure view state does not wait for desktop IO or grow the edit queue.
        if matches!(
            message,
            Message::ToggleScene
                | Message::ToggleHelp(_)
                | Message::ToggleSection(_)
                | Message::ContentScrolled(_)
                | Message::Moment(moments::Message::Preview(_) | moments::Message::BaseClock(_))
        ) {
            return None;
        }

        // Privacy lives outside desktop transactions and must not wait behind a
        // theme/profile operation. Its own worker never holds a desktop lock.
        if matches!(
            message,
            Message::ShareUsage(_) | Message::UsageSaved(_) | Message::OpenPrivacy
        ) {
            return None;
        }
        if self.native_busy
            && self.profiles.mix.is_some()
            && matches!(
                message,
                Message::Profiles(
                    profiles::Message::MixStart
                        | profiles::Message::MixPart(_)
                        | profiles::Message::MixPick(..)
                        | profiles::Message::MixQuery(_)
                        | profiles::Message::MixPage(_)
                        | profiles::Message::MixClosePicker
                )
            )
        {
            return Some(Task::none());
        }
        if self.native_busy
            && self.canvas.is_some()
            && matches!(
                message,
                Message::Canvas(_)
                    | Message::Placement(_)
                    | Message::WidgetFont(..)
                    | Message::UndoDesktop
                    | Message::UndoPreferences
            )
        {
            // Apply owns this draft until completion. Pointer samples must not
            // grow a queue, and draft gestures must never replay as ordinary
            // immediate controls after successful Apply has closed the canvas.
            return Some(Task::none());
        }
        if self.native_busy && matches!(message, Message::Recipe(_)) {
            return Some(Task::none());
        }
        if self.native_busy {
            // Default to deferral at the shared admission boundary, before
            // dispatch, completion, preference saves or checkpoint processing.
            // Only these read-only window events may bypass the active writer.
            match message {
                Message::Profiles(profiles::Message::DiscoveryQuery(query)) => {
                    // A bounded, coalesced local search must not grow the writer queue.
                    if let Some(Message::Profiles(profiles::Message::DiscoveryQuery(last))) =
                        self.native_deferred.back_mut()
                    {
                        *last = query.chars().take(80).collect();
                    } else {
                        self.native_deferred.push_back(Message::Profiles(
                            profiles::Message::DiscoveryQuery(query.chars().take(80).collect()),
                        ));
                    }
                }
                Message::NativeDone(_) => return None,
                Message::Resized(size) => self.window = *size,
                Message::Window(control) if *control != WindowControl::Close => {
                    return Some(window_action(*control));
                }
                Message::Preview
                | Message::DesktopEditor(desktop::Message::Islands(desktop::islands::Edit::Sync)) =>
                    {}
                Message::Tick
                    if self
                        .native_deferred
                        .iter()
                        .any(|m| matches!(m, Message::Tick)) => {}
                _ => {
                    // Legacy keyboard/capture requests also bind at admission.
                    // Rendered controls already carry their immutable identity.
                    let request = match message {
                        Message::UndoDesktop | Message::UndoPreferences => {
                            Message::UndoDesktopShown(
                                self.last_desktop_change
                                    .as_ref()
                                    .map(|(_, _, id)| id.clone()),
                            )
                        }
                        _ => message.clone(),
                    };
                    // Only replace the immediately preceding sample of the
                    // same continuous control. Releases, Undo, bar selection
                    // and other actions remain ordered barriers.
                    if let Some(last) = self.native_deferred.back_mut()
                        && same_slider(last, &request)
                    {
                        *last = request;
                    } else {
                        self.native_deferred.push_back(request);
                    }
                }
            }
            return Some(Task::none());
        }
        if matches!(message, Message::Wizard(edit) if !matches!(edit.action(), wizard::Message::Next | wizard::Message::Look(_) | wizard::Message::Cancel | wizard::Message::Start))
        {
            return None;
        }
        if let Message::Design(
            edit @ (design::Message::Migrate
            | design::Message::Recover
            | design::Message::KeepDesign),
        ) = message
        {
            let migrate = matches!(edit, design::Message::Migrate);
            let repair = matches!(edit, design::Message::KeepDesign);
            if repair && (!self.design.source_review || !self.design.source_conflict) {
                return Some(Task::none());
            }
            if migrate && !self.design.review {
                return Some(Task::none());
            }
            self.begin_native();
            return Some(task(move || {
                Done::Design(
                    crate::desktop::actions::history_store()
                        .map_err(|e| e.to_string())
                        .and_then(|store| {
                            if migrate {
                                store
                                    .migrate_design()
                                    .map(|()| "Widgets migrated".into())
                                    .map_err(|e| e.to_string())
                            } else if repair {
                                store
                                    .repair_design_source()
                                    .map(|()| {
                                        format!(
                                            "Current design kept. Older changes saved in {}",
                                            store.state_dir().display()
                                        )
                                    })
                                    .map_err(|e| e.to_string())
                            } else {
                                store
                                    .recover()
                                    .map(|_| "Interrupted change recovered".into())
                                    .map_err(|e| e.to_string())
                            }
                        }),
                )
            }));
        }
        if let Message::Recipe(
            edit @ (recipes::Message::Save
            | recipes::Message::ConfirmDelete
            | recipes::Message::Apply
            | recipes::Message::UseBuiltin
            | recipes::Message::Reload),
        ) = message
        {
            let mut editor = self.recipe_editor.clone();
            let edit = edit.clone();
            let baseline = self
                .config
                .recipes
                .get(editor.draft.kind)
                .and_then(|r| serde_json::to_value(r).ok());
            self.begin_native();
            return Some(task(move || {
                let result = editor.work(edit, baseline);
                Done::Recipes(editor, result)
            }));
        }
        let package_work = matches!(
            message,
            Message::Profiles(
                profiles::Message::Package(
                    profiles::packages::Edit::Open | profiles::packages::Edit::Export
                ) | profiles::Message::Imported(..)
            )
        );
        if !self.native_worker_needed() && !package_work {
            return None;
        }
        // Switching tabs reads the in-memory drafts, not the companion. A
        // live Arrange lease still has to be cancelled by its owning worker.
        if matches!(message, Message::DesktopEditor(desktop::Message::Select(_)))
            && !self.desktop.arranging()
        {
            return None;
        }
        match message {
            Message::DesktopEditor(edit) if self.page == Page::DesktopDock => {
                let display = self.desktop.display_snapshot();
                let mut editor = std::mem::replace(&mut self.desktop, display);
                let edit = edit.clone();
                self.native_property |= direct::bar_property(editor.selected, &edit);
                let property = direct::property(self, message);
                if property != 0 {
                    self.native_property = 0;
                }
                let recovery = matches!(
                    edit,
                    desktop::Message::Recover | desktop::Message::KeepRecovery
                );
                let complete = direct::completion(message).is_some();
                let label = direct::label(message);
                self.begin_native();
                Some(task(move || {
                    use crate::desktop::actions;
                    let revision = crate::desktop::store::revision();
                    let prior = actions::history_store()
                        .ok()
                        .and_then(|s| s.latest_change().ok())
                        .flatten()
                        .map(|v| v.id);
                    let focus = actions::in_action(&label, || {
                        actions::property(property, || {
                            let focus = editor.update(edit);
                            if complete {
                                editor.update(desktop::Message::Apply);
                            }
                            focus
                        })
                    });
                    if property != 0
                        && editor.writable()
                        && !matches!(editor.notice, Some(desktop::Notice::Error(_)))
                    {
                        let current = actions::history_store()
                            .ok()
                            .and_then(|s| s.latest_change().ok())
                            .flatten()
                            .map(|v| v.id);
                        if current == prior {
                            if let Err(e) = actions::complete_property(property, &label) {
                                editor.notice = Some(desktop::Notice::Error(e));
                            }
                        }
                    }
                    // Recovery can restore widget preferences or clear a journal
                    // without advancing the normal transaction revision. Refresh
                    // the other views before any watcher notification arrives.
                    let changed = crate::desktop::store::revision() != revision
                        || (recovery && !matches!(editor.notice, Some(desktop::Notice::Error(_))));
                    Done::Desktop(editor, focus, changed)
                }))
            }
            Message::Profiles(edit)
                if edit.blocking()
                    && (matches!(
                        self.page,
                        Page::DesktopProfiles | Page::Themes | Page::AdaptiveLooks
                    ) || matches!(edit, profiles::Message::Imported(..))) =>
            {
                // Preserve the same page/scope admission as the synchronous path.
                if self.page == Page::Themes
                    && matches!(edit, profiles::Message::Apply)
                    && self.profiles.profile().is_some_and(|p| !p.appearance_only)
                {
                    return None;
                }
                self.flush_edits();
                if self.native_busy {
                    self.native_deferred.push_back(message.clone());
                    return Some(Task::none());
                }
                let mut editor = self.profiles.clone();
                let edit = edit.clone();
                self.begin_native();
                if !edit.writes() {
                    self.status = "Preparing preview…".into();
                } else if matches!(edit, profiles::Message::Package(_)) {
                    self.status = "Preparing package…".into();
                } else if matches!(edit, profiles::Message::Imported(..)) {
                    self.status = "Importing setup…".into();
                }
                Some(task(move || {
                    let result = editor.update(edit);
                    Done::Profiles(editor, result)
                }))
            }
            Message::RecoveryApply | Message::RecoveryApplyOne(_) => {
                let mut recovery = self.recovery_sheet.clone();
                let index = match message {
                    Message::RecoveryApplyOne(i) => Some(*i),
                    _ => None,
                };
                self.begin_native();
                Some(task(move || {
                    let result = match index {
                        Some(i) => recovery.apply_one(i),
                        None => recovery.apply(),
                    };
                    Done::Recovery(recovery, result)
                }))
            }
            Message::ResetThemeSurface(domain) => {
                let mut editor = self.profiles.clone();
                let domain = *domain;
                self.begin_native();
                Some(task(move || {
                    editor.open();
                    editor.update(profiles::Message::ResetSurface(domain));
                    let result = editor.update(profiles::Message::Apply);
                    Done::Profiles(editor, result)
                }))
            }
            Message::Wizard(edit) => {
                let mut wizard = self.wizard.as_ref()?.clone();
                let edit = edit.clone();
                let config = self.config.clone();
                self.begin_native();
                Some(task(move || {
                    let store = crate::desktop::DesktopStore::from_env().ok();
                    let result = wizard.update(edit, store.as_ref(), &config);
                    Done::Wizard(wizard, result)
                }))
            }
            _ => None,
        }
    }
    pub(super) fn background_canvas(&mut self, apply: canvas::Apply) -> Task<Message> {
        self.begin_native();
        task(move || Done::Canvas(apply.id, apply.run()))
    }
    pub(super) fn background_undo(
        &mut self,
        shown: Option<String>,
        review: Option<(crate::desktop::store::UndoReview, bool)>,
    ) -> Task<Message> {
        self.begin_native();
        task(move || {
            let mut conflict = None;
            let undone = if review.is_none() {
                shown.clone()
            } else {
                None
            };
            let result = crate::desktop::actions::history_store()
                .and_then(|store| {
                    if let Some((review, restore)) = review {
                        return if restore {
                            store.accept_undo_review(&review)
                        } else {
                            store.keep_undo_current(&review)
                        };
                    }
                    let id = shown.ok_or_else(|| {
                        crate::desktop::DesktopError::UndoUnavailable(
                            "No settings action to undo".into(),
                        )
                    })?;
                    let result = store.undo_latest(&id);
                    if matches!(
                        result,
                        Err(crate::desktop::DesktopError::Conflict(_)
                            | crate::desktop::DesktopError::FieldConflict { .. })
                    ) {
                        conflict = store.review_undo(&id).ok();
                    }
                    result
                })
                .map(|_| ())
                .map_err(|e| e.to_string());
            Done::Undo(result.map(|()| undone), conflict)
        })
    }
    pub(super) fn background_material(
        &mut self,
        target: crate::materials::Target,
        finish: Option<crate::materials::Finish>,
        expected: Option<crate::materials::Materials>,
    ) {
        self.begin_native();
        self.native_tasks.push(task(move || {
            Done::Material(
                crate::desktop::actions::history_store()
                    .map_err(|e| e.to_string())
                    .and_then(|s| {
                        crate::desktop::materials::apply_from(&s, target, finish, expected)
                    }),
            )
        }));
    }
    pub(super) fn finish_native(&mut self, delivery: Delivery) -> Task<Message> {
        let Some(done) = delivery.0.lock().unwrap().take() else {
            return Task::none();
        };
        self.native_busy = false;
        self.status.clear();
        let task = match done {
            Done::Desktop(mut editor, focus, changed) => {
                editor.islands.installation = self.desktop.islands.installation.clone();
                self.desktop = editor;
                // A picker, slider draft or read-only check did not change the
                // desktop. Replanning every previously opened page here makes
                // controls lag even though the worker itself is asynchronous.
                let refresh = if changed {
                    self.after_desktop_change()
                } else {
                    Task::none()
                };
                Task::batch([
                    refresh,
                    focus
                        .map(cosmic::widget::button::focus)
                        .unwrap_or_else(Task::none),
                ])
            }
            Done::Profiles(editor, outcome) => {
                let before = self.profiles.selection().clone();
                let origins = |editor: &profiles::ProfilesEditor| {
                    editor.mix.as_ref().map(|d| {
                        d.sources
                            .each_ref()
                            .map(|s| s.as_ref().map(|s| s.origin.clone()))
                    })
                };
                let mix_changed = origins(&self.profiles) != origins(&editor)
                    || self.profiles.package.is_some() != editor.package.is_some();
                self.profiles = editor;
                let reveal = if mix_changed && self.page == Page::DesktopProfiles {
                    self.page_scroll.insert(self.page.key().into(), 0.);
                    cosmic::iced::widget::scrollable::snap_to(
                        cosmic::widget::Id::new("settings-content"),
                        cosmic::iced::widget::scrollable::RelativeOffset {
                            x: None,
                            y: Some(0.),
                        },
                    )
                } else {
                    self.reveal_setup_selection(&before)
                };
                Task::batch([self.profile_outcome(outcome), reveal])
            }
            Done::Wizard(wizard, outcome) => {
                self.wizard = Some(wizard);
                self.wizard_outcome(outcome)
            }
            Done::Undo(result, review) => {
                self.undo_review = review;
                match result {
                    Ok(id) => {
                        self.error = false;
                        self.wizard_undo_completed(id);
                        #[cfg(test)]
                        {
                            self.preference_undo = None;
                        }
                    }
                    Err(e) => self.failed(e),
                }
                self.after_desktop_change()
            }
            Done::Material(result) => {
                match result {
                    Ok(()) => self.saved(),
                    Err(e) => self.failed(e),
                }
                self.after_desktop_change()
            }
            Done::Recovery(recovery, result) => {
                self.recovery_sheet = recovery;
                match result {
                    Ok(()) => self.error = false,
                    Err(e) => self.failed(e),
                }
                self.after_desktop_change()
            }
            Done::Canvas(id, result) => self.finish_canvas(id, result),
            Done::Recipes(mut editor, result) => {
                let changed = matches!(&result, Ok(Some(_)));
                match result {
                    Ok(config) => {
                        if let Some(config) = config {
                            self.config = config;
                        }
                        editor.error = false;
                    }
                    Err(e) => {
                        editor.notice = Some(e);
                        editor.error = true;
                    }
                }
                self.error = editor.error;
                self.status = editor.notice.clone().unwrap_or_default();
                self.recipe_editor = editor;
                if changed {
                    self.after_desktop_change()
                } else {
                    Task::none()
                }
            }
            Done::Design(result) => {
                self.config = Config::load();
                self.design = design::State::load();
                self.design.notice = Some(match result {
                    Ok(notice) => notice,
                    Err(error) => error,
                });
                if self.config.design_problem.is_none() {
                    self.after_desktop_change()
                } else {
                    Task::none()
                }
            }
            Done::Failed(e) => {
                self.failed(e);
                Task::none()
            }
        };
        self.last_desktop_change = navigation::latest_desktop_change();
        self.checkpoint_session_drafts();
        let mut tasks = vec![task];
        if self.page == Page::General
            && (self.support.recent_open || self.app_settings_page == super::AppSettingsPage::Help)
        {
            tasks.push(self.load_support());
        }
        while !self.native_busy {
            let Some(message) = self.native_deferred.pop_front() else {
                break;
            };
            tasks.push(self.update(message));
        }
        Task::batch(tasks)
    }
}

/// These window commands do not read or write preferences. Shared with the
/// normal handler so busy-window admission has no save/checkpoint side effects.
pub(super) fn window_action(control: WindowControl) -> Task<Message> {
    use cosmic::app::Action;
    Task::done(cosmic::Action::Cosmic(match control {
        WindowControl::Drag => Action::Drag,
        WindowControl::Menu => Action::ShowWindowMenu,
        WindowControl::Maximize => Action::Maximize,
        WindowControl::Minimize => Action::Minimize,
        WindowControl::Close => Action::Close,
    }))
}

/// These messages only update an unfinished slider draft. Discrete choices
/// and completion messages deliberately do not participate.
fn same_slider(a: &Message, b: &Message) -> bool {
    use desktop::Message as Edit;
    matches!(
        (a, b),
        (
            Message::DesktopEditor(Edit::Opacity(_)),
            Message::DesktopEditor(Edit::Opacity(_))
        ) | (
            Message::DesktopEditor(Edit::Radius(_)),
            Message::DesktopEditor(Edit::Radius(_))
        ) | (
            Message::DesktopEditor(Edit::Padding(_)),
            Message::DesktopEditor(Edit::Padding(_))
        ) | (
            Message::DesktopEditor(Edit::Spacing(_)),
            Message::DesktopEditor(Edit::Spacing(_))
        )
    )
}

#[cfg(test)]
mod review030_tests {
    use super::*;
    use crate::displays::test_support::isolated;
    use cosmic::Application;
    use cosmic::iced::futures::StreamExt;

    #[test]
    fn fb12_mix_preparation_uses_worker_and_busy_input_stays_bounded() {
        if isolated(
            "settings::native_jobs::review030_tests::fb12_mix_preparation_uses_worker_and_busy_input_stays_bounded",
        ) {
            return;
        }
        use crate::desktop::profiles::mix::Component;
        let root =
            std::path::PathBuf::from(std::env::var_os("DESKTOP_CARDS_DESKTOP_ROOT").unwrap());
        crate::desktop::islands::fixture_installed(&root);
        crate::test_env::native_worker(&root, true);
        std::fs::write(root.join("status-islands-probe.py"), "import pathlib\nprint((pathlib.Path(__file__).parent/'islands-installation.json').read_text())\n").unwrap();
        Config::save_changes(serde_json::json!({"theme":"teal"})).unwrap();
        let original = std::fs::read(Config::path().unwrap()).unwrap();
        let (mut app, _) = Settings::init(Core::default(), Page::DesktopProfiles);
        for edit in [
            profiles::Message::MixStart,
            profiles::Message::MixPick(
                Component::Style,
                profiles::Selection::Preset("paper-studio"),
            ),
        ] {
            let task = app.update(Message::Profiles(edit));
            assert!(app.native_busy);
            assert_eq!(app.status, "Preparing preview…");
            if app.profiles.mix.is_some() {
                for _ in 0..100 {
                    let _ = app.update(Message::Profiles(profiles::Message::MixQuery(
                        "busy".into(),
                    )));
                }
                assert!(app.native_deferred.is_empty());
            }
            let messages = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .unwrap()
                .block_on(async {
                    let mut messages = Vec::new();
                    if let Some(mut stream) = cosmic::iced::runtime::task::into_stream(task) {
                        while let Some(action) = stream.next().await {
                            if let cosmic::iced::runtime::Action::Output(cosmic::Action::App(
                                message,
                            )) = action
                            {
                                messages.push(message);
                            }
                        }
                    }
                    messages
                });
            assert_eq!(messages.len(), 1);
            for message in messages {
                let _ = app.update(message);
            }
            assert!(!app.native_busy);
        }
        assert_eq!(
            app.profiles
                .mix
                .as_ref()
                .unwrap()
                .source_name(Component::Style),
            Some("Paper")
        );
        assert_eq!(std::fs::read(Config::path().unwrap()).unwrap(), original);
    }

    #[test]
    fn review030_native_recovery_refreshes_preferences_flags_and_undo_before_watchers() {
        if isolated(
            "settings::native_jobs::review030_tests::review030_native_recovery_refreshes_preferences_flags_and_undo_before_watchers",
        ) {
            return;
        }
        use crate::desktop::{
            DesktopPatch, Owner,
            store::{Fault, widget::WidgetPatch},
        };
        let root =
            std::path::PathBuf::from(std::env::var_os("DESKTOP_CARDS_DESKTOP_ROOT").unwrap());
        crate::desktop::islands::fixture_installed(&root);
        // Force the actual native-worker boundary without invoking a real helper.
        crate::test_env::native_worker(&root, true);
        std::fs::write(root.join("status-islands-probe.py"), "import pathlib\nprint((pathlib.Path(__file__).parent/'islands-installation.json').read_text())\n").unwrap();
        for restore in [true, false] {
            Config::save_changes(serde_json::json!({"theme":"teal"})).unwrap();
            let store = crate::desktop::DesktopStore::from_env().unwrap();
            let prior = store.latest_change().unwrap().map(|c| c.id);
            let mut patch = WidgetPatch::new();
            patch
                .set("/theme", Some("teal".into()), Some("ocean".into()))
                .unwrap();
            store.inject(Some(Fault::InterruptBeforeRecord));
            assert!(
                store
                    .apply_owned(
                        &DesktopPatch::new(),
                        Some(&patch),
                        Owner::Profile,
                        "Interrupted theme"
                    )
                    .is_err()
            );
            store.inject(None);
            assert_eq!(Config::load().theme, "ocean");
            let (mut app, _) = Settings::init(Core::default(), Page::DesktopDock);
            assert_eq!(app.config.theme, "ocean");
            assert!(app.desktop.recovery().is_some());
            let revision = crate::desktop::store::revision();
            let edit = if restore {
                desktop::Message::Recover
            } else {
                desktop::Message::KeepRecovery
            };
            let task = app.update(Message::DesktopEditor(edit));
            assert!(app.native_busy, "test must use the real worker path");
            let messages = tokio::runtime::Builder::new_current_thread()
                .enable_time()
                .build()
                .unwrap()
                .block_on(async {
                    let mut messages = Vec::new();
                    if let Some(mut stream) = cosmic::iced::runtime::task::into_stream(task) {
                        while let Some(action) = stream.next().await {
                            if let cosmic::iced::runtime::Action::Output(cosmic::Action::App(
                                message,
                            )) = action
                            {
                                messages.push(message);
                            }
                        }
                    }
                    messages
                });
            assert_eq!(
                crate::desktop::store::revision(),
                revision,
                "recovery does not advance ordinary-write revision"
            );
            assert_eq!(messages.len(), 1);
            for message in messages {
                assert!(matches!(message, Message::NativeDone(_)));
                // Deliver completion alone: no Tick, Preview, or filesystem event.
                let _ = app.update(message);
            }
            assert!(!app.native_busy);
            assert!(app.desktop.recovery().is_none());
            assert!(store.pending_recovery().unwrap().is_none());
            assert_eq!(app.config.theme, if restore { "teal" } else { "ocean" });
            assert_eq!(app.config.theme, Config::load().theme);
            assert_eq!(
                app.last_desktop_change
                    .as_ref()
                    .map(|(_, _, id)| id.clone()),
                prior
            );
        }
    }
}
