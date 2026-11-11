//! Named local profiles: `<store state>/profiles/<id>.json`.
//!
//! The file name is a generated plain ID; the human name lives only inside
//! the record, so equal names never share or overwrite a file. Listing and
//! loading create nothing. Every write checks write access, fixture
//! confinement and symlinks before creating anything, writes a private
//! temporary file and then links (create, never overwriting) or renames
//! (update an existing profile) it into place.
//!
//! Library records are local: they keep this computer's own wallpaper
//! pictures and display arrangement (`export_local` / `import_local`).
use super::format::{
    MAX_PROFILE_BYTES, Profile, ProfileError, export_local, import_local, valid_name,
};
use crate::desktop::store::{DesktopStore, PROFILES_DIR};
use std::fs;
use std::io::{self, Write};
use std::os::unix::fs::{DirBuilderExt, OpenOptionsExt};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

static NEXT_ID: AtomicU64 = AtomicU64::new(0);
const MAX_PROFILES: usize = 512;

/// A library storage ID: 1–64 of `a-z`, `0-9` and `-`. Never a path.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ProfileId(String);

impl ProfileId {
    pub fn parse(value: &str) -> Result<Self, ProfileError> {
        let valid = !value.is_empty()
            && value.len() <= 64
            && !value.starts_with('-')
            && value
                .bytes()
                .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-');
        if valid {
            Ok(Self(value.to_string()))
        } else {
            Err(ProfileError::Invalid("This look’s ID isn’t valid".into()))
        }
    }
    pub fn as_str(&self) -> &str {
        &self.0
    }
    fn generate() -> Self {
        Self(format!(
            "p{}-{}-{}",
            chrono::Local::now().format("%Y%m%d%H%M%S%3f"),
            std::process::id(),
            NEXT_ID.fetch_add(1, Ordering::Relaxed)
        ))
    }
    fn file_name(&self) -> String {
        format!("{}.json", self.0)
    }
}

/// One stored profile, or why it cannot be read.
#[derive(Clone, Debug, PartialEq)]
pub struct ProfileEntry {
    pub id: ProfileId,
    pub profile: Result<Profile, String>,
}

pub struct ProfileLibrary<'a> {
    store: &'a DesktopStore,
}

impl<'a> ProfileLibrary<'a> {
    pub fn new(store: &'a DesktopStore) -> Self {
        Self { store }
    }
    fn dir(&self) -> PathBuf {
        self.store.state_dir().join(PROFILES_DIR)
    }

    /// Stored profiles, by name. A missing library is empty. Reads only;
    /// symlinks are reported, never followed.
    pub fn list(&self) -> Result<Vec<ProfileEntry>, ProfileError> {
        let dir = self.dir();
        match fs::symlink_metadata(&dir) {
            Ok(meta) if meta.is_dir() => {}
            Ok(_) => {
                return Err(ProfileError::Storage(format!(
                    "{} is not a folder",
                    dir.display()
                )));
            }
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(Vec::new()),
            Err(error) => return Err(storage(&dir, error)),
        }
        let mut entries = Vec::new();
        for item in fs::read_dir(&dir).map_err(|error| storage(&dir, error))? {
            let item = item.map_err(|error| storage(&dir, error))?;
            let name = item.file_name();
            let Some(stem) = name.to_str().and_then(|name| name.strip_suffix(".json")) else {
                continue;
            };
            let Ok(id) = ProfileId::parse(stem) else {
                continue;
            };
            if entries.len() >= MAX_PROFILES {
                break;
            }
            let profile = read_profile(&item.path()).map_err(|error| error.to_string());
            entries.push(ProfileEntry { id, profile });
        }
        entries.sort_by(|a, b| {
            let key = |entry: &ProfileEntry| {
                entry
                    .profile
                    .as_ref()
                    .map(|profile| profile.name.to_lowercase())
                    .unwrap_or_default()
            };
            key(a).cmp(&key(b)).then_with(|| a.id.cmp(&b.id))
        });
        Ok(entries)
    }

    /// Reads one profile. Creates nothing.
    pub fn load(&self, id: &ProfileId) -> Result<Profile, ProfileError> {
        read_profile(&self.dir().join(id.file_name()))
    }

    /// Stores `profile` under a new ID. Never overwrites a file.
    pub fn create(&self, profile: &Profile) -> Result<ProfileId, ProfileError> {
        let bytes = export_local(profile)?;
        let dir = self.prepare()?;
        for _ in 0..16 {
            let id = ProfileId::generate();
            let path = self
                .store
                .state_subpath(PROFILES_DIR, Some(&id.file_name()))?;
            let temporary = write_temporary(&dir, &bytes)?;
            let linked = fs::hard_link(&temporary, &path);
            let _ = fs::remove_file(&temporary);
            match linked {
                Ok(()) => {
                    sync(&dir);
                    return Ok(id);
                }
                Err(error) if error.kind() == io::ErrorKind::AlreadyExists => continue,
                Err(error) => return Err(storage(&path, error)),
            }
        }
        Err(ProfileError::Storage(
            "Couldn’t choose an ID for this look".into(),
        ))
    }

    /// First application retains the original appearance under a stable ID.
    /// Atomic create keeps an existing capture, including across failed applies.
    pub fn retain_before_themes(&self) -> Result<(), ProfileError> {
        let id = ProfileId::parse("before-themes")?;
        let path = self
            .store
            .state_subpath(PROFILES_DIR, Some(&id.file_name()))?;
        if path.exists() {
            self.load(&id)?;
            return Ok(());
        }
        let captured = super::themes::capture(self.store, "Before Themes")?;
        let bytes = export_local(&captured.profile)?;
        let dir = self.prepare()?;
        let temporary = write_temporary(&dir, &bytes)?;
        let result = fs::hard_link(&temporary, &path);
        let _ = fs::remove_file(temporary);
        match result {
            Ok(()) => {
                sync(&dir);
                Ok(())
            }
            Err(e) if e.kind() == io::ErrorKind::AlreadyExists => self.load(&id).map(|_| ()),
            Err(e) => Err(storage(&path, e)),
        }
    }

    /// Renames a stored profile; only its record's name changes.
    pub fn rename(&self, id: &ProfileId, name: &str) -> Result<Profile, ProfileError> {
        if !valid_name(name) {
            return Err(ProfileError::Invalid(
                "Look names can be 1 to 80 characters, on one line.".into(),
            ));
        }
        let mut profile = self.load(id)?;
        profile.name = name.to_string();
        self.replace(id, &profile)?;
        Ok(profile)
    }

    /// Stores a copy of a profile under a new ID and `name`.
    pub fn duplicate(&self, id: &ProfileId, name: &str) -> Result<ProfileId, ProfileError> {
        let mut profile = self.load(id)?;
        if !valid_name(name) {
            return Err(ProfileError::Invalid(
                "Look names can be 1 to 80 characters, on one line.".into(),
            ));
        }
        profile.name = name.to_string();
        self.create(&profile)
    }

    /// Deletes exactly the stored profile `id`.
    pub fn delete(&self, id: &ProfileId) -> Result<(), ProfileError> {
        if id.as_str() == "before-themes" {
            return Err(ProfileError::Invalid(
                "Your original appearance stays available in Before Themes.".into(),
            ));
        }
        let path = self
            .store
            .state_subpath(PROFILES_DIR, Some(&id.file_name()))?;
        regular_file(&path)?;
        fs::remove_file(&path).map_err(|error| storage(&path, error))?;
        sync(&self.dir());
        Ok(())
    }

    fn replace(&self, id: &ProfileId, profile: &Profile) -> Result<(), ProfileError> {
        let bytes = export_local(profile)?;
        let path = self
            .store
            .state_subpath(PROFILES_DIR, Some(&id.file_name()))?;
        regular_file(&path)?;
        let dir = self.dir();
        let temporary = write_temporary(&dir, &bytes)?;
        if let Err(error) = fs::rename(&temporary, &path) {
            let _ = fs::remove_file(&temporary);
            return Err(storage(&path, error));
        }
        sync(&dir);
        Ok(())
    }

    /// The library folder, created (0700) after every check passed.
    fn prepare(&self) -> Result<PathBuf, ProfileError> {
        let dir = self.store.state_subpath(PROFILES_DIR, None)?;
        // Checks the folder's own leaf name too, before anything exists.
        self.store.state_subpath(PROFILES_DIR, Some("probe.json"))?;
        if !dir.is_dir() {
            fs::create_dir_all(self.store.state_dir())
                .map_err(|error| storage(self.store.state_dir(), error))?;
            match fs::DirBuilder::new().mode(0o700).create(&dir) {
                Ok(()) => {}
                Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {}
                Err(error) => return Err(storage(&dir, error)),
            }
            // Re-check: nothing may have been swapped in meanwhile.
            self.store.state_subpath(PROFILES_DIR, None)?;
        }
        Ok(dir)
    }
}

fn storage(path: &Path, error: io::Error) -> ProfileError {
    ProfileError::Storage(format!("{}: {error}", path.display()))
}

fn regular_file(path: &Path) -> Result<(), ProfileError> {
    match fs::symlink_metadata(path) {
        Ok(meta) if meta.is_file() => Ok(()),
        Ok(_) => Err(ProfileError::Storage(format!(
            "{} isn’t a look file",
            path.display()
        ))),
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            Err(ProfileError::NotFound("This look no longer exists.".into()))
        }
        Err(error) => Err(storage(path, error)),
    }
}

fn read_profile(path: &Path) -> Result<Profile, ProfileError> {
    regular_file(path)?;
    let meta = fs::symlink_metadata(path).map_err(|error| storage(path, error))?;
    if meta.len() > MAX_PROFILE_BYTES as u64 {
        return Err(ProfileError::Invalid("This look is too large.".into()));
    }
    let bytes = fs::read(path).map_err(|error| storage(path, error))?;
    import_local(&bytes)
}

fn write_temporary(dir: &Path, bytes: &[u8]) -> Result<PathBuf, ProfileError> {
    let temporary = dir.join(format!(
        ".tmp-{}-{}",
        std::process::id(),
        NEXT_ID.fetch_add(1, Ordering::Relaxed)
    ));
    let result = (|| {
        let mut file = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(&temporary)?;
        file.write_all(bytes)?;
        file.sync_all()
    })();
    match result {
        Ok(()) => Ok(temporary),
        Err(error) => {
            let _ = fs::remove_file(&temporary);
            Err(storage(&temporary, error))
        }
    }
}

fn sync(dir: &Path) {
    if let Ok(file) = fs::File::open(dir) {
        let _ = file.sync_all();
    }
}
