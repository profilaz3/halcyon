//! Explicit migration review and a recovery-only view for unreadable designs.
use super::*;
#[derive(Debug, Clone)]
pub enum Message {
    Review,
    Cancel,
    Migrate,
    Recover,
    ReviewSource,
    KeepDesign,
    Recheck,
    Export(bool),
    Exported(Result<Option<PathBuf>, String>),
}
#[derive(Default)]
pub(super) struct State {
    pub managed: bool,
    pub review: bool,
    pub pending: bool,
    pub source_conflict: bool,
    pub source_review: bool,
    pub notice: Option<String>,
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
            Message::Review => self.design.review = true,
            Message::Cancel => {
                self.design.review = false;
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
