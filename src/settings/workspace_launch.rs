//! Launching is an explicit invocation. Applying or undoing a Look is entirely independent.
use super::{Message, Settings, profiles::Selection};
use crate::desktop::{
    DesktopStore,
    apps::AppCatalog,
    profiles::{Installed, ProfileId, ProfileLibrary},
};
use cosmic::app::Task;
use std::{
    sync::Arc,
    time::{Duration, Instant},
};

#[derive(Debug, Clone)]
pub enum Edit {
    Manage(ProfileId),
    Catalog(ProfileId, Arc<AppCatalog>),
    Search(String),
    Toggle(String),
    Earlier(usize),
    Save,
    Cancel,
    Start(ProfileId),
    Done(ProfileId, Result<String, String>),
}
#[derive(Default)]
pub struct State {
    pub edit: Option<ProfileId>,
    pub catalog: Option<Arc<AppCatalog>>,
    pub query: String,
    pub ordered: Vec<String>,
    pub selected: Vec<String>,
    baseline: Vec<String>,
    pub running: Option<ProfileId>,
    last_start: Option<Instant>,
    pub report: Option<(ProfileId, String)>,
}
fn catalog(store: &DesktopStore) -> AppCatalog {
    store
        .fixture_root()
        .map_or_else(AppCatalog::installed, |p| {
            AppCatalog::discover(&[p.join("applications")])
        })
}

impl Settings {
    pub(super) fn workspace_edit(&mut self, edit: Edit) -> Task<Message> {
        match edit {
            Edit::Manage(id) if self.profiles.selection() == &Selection::Saved(id.clone()) => {
                let (Some(profile), Some(store)) =
                    (self.profiles.profile(), self.profiles.store().cloned())
                else {
                    return Task::none();
                };
                self.workspace_launch.selected = profile.workspace_apps.clone();
                self.workspace_launch.baseline = profile.workspace_apps.clone();
                self.workspace_launch.query.clear();
                self.workspace_launch.edit = Some(id.clone());
                self.workspace_launch.catalog = None;
                return cosmic::task::future(async move {
                    let apps = tokio::task::spawn_blocking(move || catalog(&store))
                        .await
                        .unwrap_or_default();
                    Message::LaunchWorkspace(Edit::Catalog(id, Arc::new(apps)))
                });
            }
            Edit::Catalog(id, apps) if self.workspace_launch.edit.as_ref() == Some(&id) => {
                self.workspace_launch.ordered =
                    apps.sorted().into_iter().map(|a| a.id.clone()).collect();
                self.workspace_launch.catalog = Some(apps);
            }
            Edit::Search(text) => self.workspace_launch.query = text,
            Edit::Toggle(id) => {
                let state = &mut self.workspace_launch;
                if state.selected.contains(&id) {
                    state.selected.retain(|v| v != &id);
                } else if state.selected.len() < 12
                    && state.catalog.as_ref().is_some_and(|c| c.get(&id).is_some())
                {
                    state.selected.push(id);
                }
            }
            Edit::Earlier(i) if i > 0 && i < self.workspace_launch.selected.len() => {
                self.workspace_launch.selected.swap(i - 1, i)
            }
            Edit::Cancel => self.workspace_launch.edit = None,
            Edit::Save => {
                let (Some(id), Some(store)) =
                    (self.workspace_launch.edit.clone(), self.profiles.store())
                else {
                    return Task::none();
                };
                if self.profiles.selection() != &Selection::Saved(id.clone()) || !store.writable() {
                    return Task::none();
                }
                let lib = ProfileLibrary::new(store);
                let result = lib.load(&id).and_then(|mut profile| {
                    if profile.workspace_apps != self.workspace_launch.baseline {
                        return Err(crate::desktop::profiles::ProfileError::Invalid(
                            "Workspace apps changed elsewhere. Reopen the list to review them."
                                .into(),
                        ));
                    }
                    profile.workspace_apps = self.workspace_launch.selected.clone();
                    profile.version = profile.written_version();
                    lib.save(&id, &profile)
                });
                match result {
                    Ok(()) => {
                        self.workspace_launch.edit = None;
                        self.profiles.refresh();
                        self.workspace_launch.report = Some((id, "Workspace apps saved.".into()));
                    }
                    Err(e) => self.workspace_launch.report = Some((id, e.to_string())),
                }
            }
            Edit::Start(id) if self.profiles.selection() == &Selection::Saved(id.clone()) => {
                if self.workspace_launch.running.is_some()
                    || self
                        .workspace_launch
                        .last_start
                        .is_some_and(|t| t.elapsed() < Duration::from_millis(650))
                {
                    return Task::none();
                }
                let Some(store) = self.profiles.store().cloned() else {
                    return Task::none();
                };
                self.workspace_launch.last_start = Some(Instant::now());
                self.workspace_launch.running = Some(id.clone());
                self.workspace_launch.report = None;
                return cosmic::task::future(async move {
                    let result = start(store, id.clone()).await;
                    Message::LaunchWorkspace(Edit::Done(id, result))
                });
            }
            Edit::Done(id, result) if self.workspace_launch.running.as_ref() == Some(&id) => {
                self.workspace_launch.running = None;
                self.workspace_launch.report = Some((id, result.unwrap_or_else(|error| error)));
                return self.after_desktop_change();
            }
            _ => {}
        }
        Task::none()
    }
}

async fn start(store: DesktopStore, id: ProfileId) -> Result<String, String> {
    let (apps, installed, mut warnings) = tokio::task::spawn_blocking(move || {
        use crate::automation::{
            ProfileRef,
            run::{Ran, run_chosen},
        };
        let profile = ProfileLibrary::new(&store)
            .load(&id)
            .map_err(|e| e.to_string())?;
        if profile.workspace_apps.is_empty() {
            return Err("Choose workspace apps first.".to_string());
        }
        let installed = Installed::discover(&store);
        let held = crate::automation::read_hold(&store);
        let warnings = match run_chosen(
            &store,
            &installed,
            &ProfileRef::Saved(id.as_str().to_owned()),
            &profile.name,
            &held,
            crate::automation::run::Invocation::Workspace,
        ) {
            Ran::Applied { skipped, .. } => skipped,
            Ran::AlreadySet => vec![],
            Ran::Waiting(_) => {
                return Err(
                    "Finish the pending appearance changes and start the workspace again.".into(),
                );
            }
            Ran::Failed(reason) => return Err(reason),
            Ran::KeptManual(_) => {
                return Err("The desktop changed. Review the Look before starting.".into());
            }
        };
        Ok((profile.workspace_apps, catalog(&store), warnings))
    })
    .await
    .map_err(|_| "The workspace could not be read.".to_string())??;
    let mut count = 0;
    for id in apps {
        let Some(app) = installed.get(&id) else {
            warnings.push(format!("{id} is unavailable"));
            continue;
        };
        let command = if let Some(gtk) = crate::app::program_path("gtk-launch") {
            let mut cmd = tokio::process::Command::new(gtk);
            cmd.arg(&id);
            Some(cmd)
        } else if let (Some(gio), Some(file)) = (
            crate::app::program_path("gio"),
            crate::app::desktop_entry(&id),
        ) {
            let mut cmd = tokio::process::Command::new(gio);
            cmd.arg("launch").arg(file);
            Some(cmd)
        } else {
            None
        };
        let Some(mut command) = command else {
            warnings.push(format!("Couldn’t open {}", app.name));
            continue;
        };
        let status =
            tokio::time::timeout(Duration::from_secs(5), command.kill_on_drop(true).status()).await;
        if matches!(status,Ok(Ok(s)) if s.success()) {
            count += 1;
        } else {
            warnings.push(format!("Couldn’t open {}", app.name));
        }
    }
    let noun = if count == 1 { "app" } else { "apps" };
    let mut result = format!("Workspace started · {count} {noun} requested.");
    if !warnings.is_empty() {
        result.push(' ');
        result.push_str(&warnings.join(" · "));
    }
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::desktop::{
        profiles::{self, Profile},
        store::tests::Fixture,
    };
    #[test]
    fn workspace_records_are_passive_and_explicit_start_handles_missing_apps() {
        use std::os::unix::fs::PermissionsExt;
        let child = "HALCYON_WORKSPACE_TEST_CHILD";
        if std::env::var_os(child).is_none() {
            let status = std::process::Command::new(std::env::current_exe().unwrap())
                .args([
                    "workspace_records_are_passive_and_explicit_start_handles_missing_apps",
                    "--test-threads=1",
                    "--nocapture",
                ])
                .env(child, "1")
                .status()
                .unwrap();
            assert!(status.success());
            return;
        }
        let fixture = Fixture::new("workspace-launch");
        let widget_config = fixture.0.join("widgets.json");
        std::fs::write(&widget_config, "{}").unwrap();
        let store = fixture.store().with_app_config(widget_config);
        let appdir = fixture.0.join("applications");
        let bin = fixture.0.join("bin");
        let system_gio = crate::app::program_path("gio");
        std::fs::create_dir_all(&appdir).unwrap();
        std::fs::create_dir_all(&bin).unwrap();
        let log = fixture.0.join("requested");
        for (id, exec) in [
            ("org.example.Editor", "writer"),
            (
                "org.example.Flatpak",
                "/usr/bin/flatpak run org.example.Flatpak",
            ),
        ] {
            std::fs::write(
                appdir.join(format!("{id}.desktop")),
                format!("[Desktop Entry]\nType=Application\nName={id}\nExec={exec}\n"),
            )
            .unwrap();
        }
        let launcher = bin.join("gtk-launch");
        std::fs::write(
            &launcher,
            "#!/bin/sh\nprintf '%s\\n' \"$1\" >> \"$HALCYON_WORKSPACE_LAUNCH_LOG\"\n",
        )
        .unwrap();
        std::fs::set_permissions(&launcher, std::fs::Permissions::from_mode(0o755)).unwrap();
        unsafe {
            std::env::set_var("PATH", &bin);
            std::env::set_var("HALCYON_WORKSPACE_LAUNCH_LOG", &log);
        }
        let mut profile = Profile::new("Writing").unwrap();
        profile.widget_style = Some(serde_json::json!({"appearance":{"opacity":65}}));
        profile.workspace_apps = [
            "org.example.Editor",
            "org.example.Gone",
            "org.example.Flatpak",
        ]
        .map(String::from)
        .into();
        let exported = profiles::export(&profile).unwrap();
        let imported = profiles::import(&exported.bytes).unwrap();
        assert_eq!(imported.workspace_apps, profile.workspace_apps);
        let lib = ProfileLibrary::new(&store);
        let id = lib.create(&imported).unwrap();
        assert!(!log.exists());
        let mut bad = profile.clone();
        bad.workspace_apps.push("../program".into());
        assert!(bad.validate().is_err());
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        let result = rt.block_on(start(store.clone(), id.clone())).unwrap();
        assert!(store.manual_at().is_some());
        assert!(
            result.contains("2 apps") && result.contains("org.example.Gone is unavailable"),
            "{result}"
        );
        assert_eq!(
            std::fs::read_to_string(&log).unwrap(),
            "org.example.Editor\norg.example.Flatpak\n"
        );
        if let Some(gio) =
            system_gio.filter(|_| std::env::var_os("HALCYON_CUSTOMIZATION_EVIDENCE").is_some())
        {
            let helper = bin.join("record-activation");
            std::fs::rename(&launcher, &helper).unwrap();
            std::os::unix::fs::symlink(gio, bin.join("gio")).unwrap();
            std::fs::remove_file(&log).unwrap();
            unsafe {
                std::env::set_var("XDG_DATA_HOME", &fixture.0);
            }
            for app in ["org.example.Editor", "org.example.Flatpak"] {
                std::fs::write(
                    appdir.join(format!("{app}.desktop")),
                    format!(
                        "[Desktop Entry]\nType=Application\nName={app}\nExec={} {app}\n",
                        helper.display()
                    ),
                )
                .unwrap();
            }
            let result = rt.block_on(start(store, id)).unwrap();
            assert!(result.contains("2 apps"), "{result}");
            let deadline = Instant::now() + Duration::from_secs(3);
            loop {
                let opened = std::fs::read_to_string(&log).unwrap_or_default();
                if opened.lines().count() == 2 {
                    let mut names: Vec<_> = opened.lines().collect();
                    names.sort();
                    assert_eq!(names, ["org.example.Editor", "org.example.Flatpak"]);
                    break;
                }
                assert!(Instant::now() < deadline, "GIO activations: {opened}");
                std::thread::sleep(Duration::from_millis(10));
            }
        }
    }
}
