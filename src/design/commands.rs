use super::*;
use crate::{
    config::{self, Config},
    desktop::actions,
};
use serde_json::Value;
use std::{
    io::{Read, Write},
    path::Path,
};

/// Existing journal recovery runs before either the service or Settings starts.
/// New, empty installations use the current design format. Existing profiles
/// keep their explicit migration boundary.
pub fn startup() -> Result<(), String> {
    let store = actions::history_store().map_err(|e| e.to_string())?;
    if let Some(journal) = store.pending_recovery().map_err(|e| e.to_string())? {
        if journal.version == 4 {
            store.recover().map_err(|e| e.to_string())?;
        }
    }
    initialize_fresh()?;
    let path = Config::path().ok_or("No configuration directory")?;
    if storage::managed(&path) {
        storage::check(&path)?;
    }
    Ok(())
}

/// Also used before constructing Settings directly. This quick read only
/// avoids unnecessary locking for normal profiles; the store repeats every
/// fresh-state check under its publication locks.
pub fn initialize_fresh() -> Result<bool, String> {
    let path = Config::path().ok_or("No configuration directory")?;
    match std::fs::symlink_metadata(&path) {
        Ok(_) => return Ok(false),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => return Err(error.to_string()),
    }
    actions::preference_store(&path)
        .map_err(|error| error.to_string())?
        .initialize_fresh_design()
        .map_err(|error| error.to_string())
}

/// Export original bytes even when their schema cannot be understood. The
/// caller's chosen destination must be new; it can never replace applied data.
pub fn export(path: &Path, destination: &Path, legacy: bool) -> Result<(), String> {
    let store = actions::preference_store(path).map_err(|e| e.to_string())?;
    if let Some(root) = store.fixture_root() {
        let root = std::fs::canonicalize(root).map_err(|e| e.to_string())?;
        let parent = std::fs::canonicalize(destination.parent().ok_or("No export directory")?)
            .map_err(|e| e.to_string())?;
        if !parent.starts_with(root) {
            return Err("The export must stay inside the preview directory".into());
        }
    }
    let _lock = config::lock_config(path)?;
    let bytes = if legacy {
        let text = config::read_config_text(path)?;
        let mut raw = config::parse_config_text(text.as_deref())?;
        if crate::config::check_config_value(&raw)?.recipes != crate::recipes::Applied::default() {
            return Err("Personal widget recipes cannot be exported as legacy settings. Use built-in widgets first.".into());
        }
        raw.as_object_mut()
            .ok_or("Invalid preferences")?
            .remove("widget_design_version");
        // `project` already requires precisely the representable reserved set.
        // This is a private downgrade file, not a public share card/profile.
        config::config_text(&raw)?.into_bytes()
    } else {
        storage::read_bytes(path)?.ok_or("No widget design to export")?
    };
    use std::os::unix::fs::OpenOptionsExt;
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(destination)
        .map_err(|e| e.to_string())?;
    file.write_all(&bytes)
        .and_then(|()| file.sync_all())
        .map_err(|e| e.to_string())
}

pub fn run(command: &str, mut args: impl Iterator<Item = String>) -> Result<(), String> {
    let path = Config::path().ok_or("No configuration directory")?;
    let destination = if matches!(command, "--export-design" | "--export-legacy-design") {
        Some(args.next().ok_or("Choose a new export file")?)
    } else {
        None
    };
    if args.next().is_some() {
        return Err("Unexpected extra arguments".into());
    }
    match command {
        "--read-widget-config" => {
            let text = config::read_config_text(&path)?.unwrap_or_else(|| "{}".into());
            println!("{text}");
        }
        "--edit-widget-config" => {
            let mut bytes = Vec::new();
            std::io::stdin()
                .take((config::MAX_CONFIG_BYTES * 2 + 1024) as u64 + 1)
                .read_to_end(&mut bytes)
                .map_err(|e| e.to_string())?;
            if bytes.len() > config::MAX_CONFIG_BYTES * 2 + 1024 {
                return Err("Controller request is too large".into());
            }
            #[derive(serde::Deserialize)]
            #[serde(deny_unknown_fields)]
            struct Request {
                before: Value,
                after: Value,
            }
            let request: Request = serde_json::from_slice(&bytes).map_err(|e| e.to_string())?;
            config::check_config_value(&request.before)?;
            config::check_config_value(&request.after)?;
            let patch = actions::preference_patch(&request.before, &request.after)?;
            let store = actions::preference_store(&path).map_err(|e| e.to_string())?;
            actions::commit(
                &store,
                &crate::desktop::DesktopPatch::new(),
                Some(&patch),
                crate::desktop::Owner::Preferences,
                "Controller settings",
                crate::desktop::store::Source::Manual,
            )
            .map_err(|e| e.to_string())?;
        }
        "--migrate-design" => {
            actions::preference_store(&path)
                .map_err(|e| e.to_string())?
                .migrate_design()
                .map_err(|e| e.to_string())?;
        }
        "--keep-widget-design" => {
            actions::preference_store(&path)
                .map_err(|e| e.to_string())?
                .repair_design_source()
                .map_err(|e| e.to_string())?;
        }
        "--recover-design" => {
            startup()?;
        }
        "--export-design" | "--export-legacy-design" => {
            let destination = destination.ok_or("Choose a new export file")?;
            export(
                &path,
                Path::new(&destination),
                command == "--export-legacy-design",
            )?;
        }
        _ => return Err("Unknown widget design command".into()),
    }
    Ok(())
}

pub fn pending_problem() -> Option<String> {
    let store = actions::history_store().ok()?;
    match store.pending_recovery() {
        Ok(Some(journal)) if journal.version == 4 => {
            Some("A widget design change was interrupted. Recover it before continuing.".into())
        }
        Err(error) if Config::path().as_deref().is_some_and(storage::managed) => {
            Some(error.to_string())
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn private_live(name: &str) -> bool {
        if crate::displays::test_support::isolated(name) {
            return true;
        }
        // The isolated process has private HOME/XDG roots and no reachable
        // buses. Exercise the normal path resolver, never the host profile.
        unsafe {
            std::env::remove_var("DESKTOP_CARDS_CONFIG");
            std::env::remove_var("DESKTOP_CARDS_DESKTOP_ROOT");
        }
        crate::test_env::assert_isolated();
        false
    }

    #[test]
    fn polish050_fresh_start_supports_shared_xdg_roots() {
        if private_live("design::commands::tests::polish050_fresh_start_supports_shared_xdg_roots")
        {
            return;
        }
        let root = dirs::data_dir().unwrap();
        for shared_config in [false, true] {
            let common = root.join(if shared_config {
                "all-roots"
            } else {
                "data-state"
            });
            let config = if shared_config {
                common.clone()
            } else {
                root.join("separate-config")
            };
            unsafe {
                std::env::set_var("XDG_CONFIG_HOME", &config);
                std::env::set_var("XDG_DATA_HOME", &common);
                std::env::set_var("XDG_STATE_HOME", &common);
                std::env::set_var("XDG_CACHE_HOME", &common);
            }
            startup().unwrap();
            assert!(
                storage::managed(&Config::path().unwrap()),
                "fresh overlapping XDG roots"
            );
            assert!(
                !initialize_fresh().unwrap(),
                "existing initialized profile remains unchanged"
            );
        }
    }

    #[test]
    fn polish050_startup_initializes_a_new_private_user_once() {
        if private_live(
            "design::commands::tests::polish050_startup_initializes_a_new_private_user_once",
        ) {
            return;
        }
        startup().unwrap();
        let path = Config::path().unwrap();
        assert!(storage::managed(&path));
        let original = std::fs::read(&path).unwrap();
        let design = storage::read_bytes(&path).unwrap().unwrap();
        assert!(!Config::load().onboarded);
        startup().unwrap();
        assert_eq!(std::fs::read(path).unwrap(), original);
        assert_eq!(
            storage::read_bytes(&Config::path().unwrap())
                .unwrap()
                .unwrap(),
            design
        );
    }

    #[test]
    fn polish050_startup_preserves_prior_state_without_config() {
        if private_live(
            "design::commands::tests::polish050_startup_preserves_prior_state_without_config",
        ) {
            return;
        }
        let state = crate::paths::state_dir().unwrap().join("usage.json");
        std::fs::create_dir_all(state.parent().unwrap()).unwrap();
        std::fs::write(&state, b"existing state").unwrap();
        startup().unwrap();
        assert!(!Config::path().unwrap().exists());
        assert_eq!(std::fs::read(state).unwrap(), b"existing state");
        assert!(!storage::managed(&Config::path().unwrap()));
    }

    #[test]
    fn polish050_startup_preserves_a_pre_rename_profile_for_explicit_review() {
        if private_live(
            "design::commands::tests::polish050_startup_preserves_a_pre_rename_profile_for_explicit_review",
        ) {
            return;
        }
        let old = dirs::config_dir()
            .unwrap()
            .join(crate::paths::LEGACY)
            .join("config.json");
        std::fs::create_dir_all(old.parent().unwrap()).unwrap();
        let bytes = br#"{"theme":"amber","clock":{"format_24h":false}}"#;
        std::fs::write(&old, bytes).unwrap();
        // Even before the rename copier runs, old data is not a new install.
        assert!(!initialize_fresh().unwrap());
        assert!(!Config::path().unwrap().exists());
        assert!(!crate::paths::config_dir().unwrap().exists());
        // The application normally copies old folders before design startup.
        crate::paths::migrate();
        startup().unwrap();
        let path = Config::path().unwrap();
        assert_eq!(std::fs::read(&old).unwrap(), bytes);
        assert_eq!(std::fs::read(&path).unwrap(), bytes);
        assert!(!storage::managed(&path));
        assert_eq!(Config::load().theme, "amber");
    }
}
