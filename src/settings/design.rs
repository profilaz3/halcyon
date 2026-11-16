//! Explicit migration review and a recovery-only view for unreadable designs.
use super::*;
#[derive(Debug, Clone)]
pub enum Message {
    Review,
    Cancel,
    /// Apply design, before the one-time update: the update runs at once
    /// when there's nothing older to protect, or the question opens.
    Prepare,
    Migrate,
    Recover,
    ReviewSource,
    KeepDesign,
    Recheck,
    Export(bool),
    Exported(Result<Option<PathBuf>, String>),
    /// A damaged settings file is set aside, kept, and Halcyon starts again
    /// with its usual settings.
    StartFresh,
    /// Shows or hides why the settings file can't be read.
    Details(bool),
}
#[derive(Default)]
pub(super) struct State {
    pub managed: bool,
    pub review: bool,
    pub pending: bool,
    pub source_conflict: bool,
    pub source_review: bool,
    pub notice: Option<String>,
    /// The builder's design applies once the update is done: the question
    /// opened from Apply design.
    pub apply_after: bool,
    /// Why a damaged settings file can't be read is shown.
    pub details: bool,
}
impl State {
    /// Settings may be constructed without the command-line startup path.
    /// Initialize before reading Config so the first view uses the same store
    /// as later edits. Existing profiles are only inspected, never migrated.
    pub fn initialize() -> (Config, Self) {
        let result = crate::design::commands::initialize_fresh();
        let config = Config::load();
        let mut state = Self::load();
        state.notice = result.err();
        (config, state)
    }

    pub fn load() -> Self {
        Self {
            source_conflict: Config::path()
                .as_deref()
                .is_some_and(crate::design::storage::source_conflict),
            managed: Config::path()
                .as_deref()
                .is_some_and(crate::design::storage::managed),
            pending: crate::desktop::actions::history_store()
                .ok()
                .and_then(|s| s.pending_recovery().ok())
                .flatten()
                .is_some_and(|j| j.version == 4),
            ..Self::default()
        }
    }
}
impl Settings {
    pub(super) fn update_design(&mut self, edit: Message) -> Task<super::Message> {
        match edit {
            Message::Review => {
                self.design.review = true;
                self.design.apply_after = false;
            }
            // There are older widgets to protect (otherwise the native
            // queue already started the update): ask, once.
            Message::Prepare => {
                self.design.review = !self.design.managed;
                self.design.apply_after = !self.design.managed;
            }
            Message::Cancel => {
                self.design.review = false;
                self.design.apply_after = false;
                self.design.source_review = false;
            }
            Message::ReviewSource => self.design.source_review = self.design.source_conflict,
            Message::Recheck => {
                self.config = Config::load();
                self.design = State::load();
                if self.config.design_problem.is_none() {
                    return self.after_desktop_change();
                }
            }
            Message::Details(open) => self.design.details = open,
            Message::StartFresh => match start_fresh() {
                Ok(kept) => {
                    let notice = format!(
                        "Started fresh. The damaged file is kept as {}.",
                        kept.file_name()
                            .map(|name| name.to_string_lossy().into_owned())
                            .unwrap_or_default()
                    );
                    let task = self.update_design(Message::Recheck);
                    self.design.notice = Some(notice);
                    return task;
                }
                Err(error) => self.design.notice = Some(error),
            },
            Message::Export(legacy) => return export_task(legacy),
            Message::Exported(result) => {
                self.design.notice = match result {
                    Ok(Some(path)) => Some(format!("Saved {}", path.display())),
                    Ok(None) => None,
                    Err(error) => Some(error),
                }
            }
            // Always dispatched by the shared native queue, including fixtures.
            Message::Migrate | Message::Recover | Message::KeepDesign => {}
        }
        Task::none()
    }
}
/// Moves a damaged settings file aside, under a name that says so and never
/// over another copy, then starts from a fresh one as a new installation
/// does. A file the widget design manages, or one that reads again, stays.
fn start_fresh() -> Result<PathBuf, String> {
    let path = Config::path().ok_or("No configuration directory")?;
    let lock = crate::config::lock_config(&path)?;
    if crate::config::damaged_file(&path).is_none() {
        return Err("The settings file can be read again. Choose Check again.".into());
    }
    if crate::design::storage::managed(&path) {
        return Err("Put back a saved copy of the settings file, then choose Check again.".into());
    }
    let stamp = chrono::Local::now().format("%Y%m%d-%H%M%S");
    let kept = (0..100)
        .map(|n| {
            let suffix = if n == 0 {
                String::new()
            } else {
                format!("-{n}")
            };
            path.with_file_name(format!("config.damaged-{stamp}{suffix}.json"))
        })
        .find(|kept| std::fs::symlink_metadata(kept).is_err())
        .ok_or("Couldn’t find a name for the copy of the damaged file")?;
    std::fs::rename(&path, &kept).map_err(|e| e.to_string())?;
    drop(lock);
    // Without a file Halcyon already uses its usual settings; a fresh
    // design that can't be made now is made at the next start.
    if let Err(error) = crate::design::commands::initialize_fresh() {
        eprintln!("Starting fresh: {error}");
    }
    Ok(kept)
}
fn export_task(legacy: bool) -> Task<super::Message> {
    cosmic::task::future(async move {
        let result = async {
            let path = Config::path().ok_or("No configuration directory")?;
            let response = cosmic::dialog::file_chooser::save::Dialog::new()
                .title(
                    if legacy {
                        "Export private legacy settings"
                    } else {
                        "Export original widget design"
                    }
                    .to_string(),
                )
                .file_name(
                    if legacy {
                        "halcyon-legacy-config.json"
                    } else {
                        "halcyon-design-original.json"
                    }
                    .to_string(),
                )
                .save_file()
                .await;
            match response {
                Ok(response) => {
                    let Some(url) = response.url() else {
                        return Ok(None);
                    };
                    let destination = url.to_file_path().map_err(|_| "Choose a local file")?;
                    crate::design::commands::export(&path, &destination, legacy)?;
                    Ok(Some(destination))
                }
                Err(cosmic::dialog::file_chooser::Error::Cancelled) => Ok(None),
                Err(e) => Err(e.to_string()),
            }
        }
        .await;
        super::Message::Design(Message::Exported(result))
    })
}

#[cfg(test)]
mod tests;
