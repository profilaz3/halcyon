//! Exact, selective restoration. Plans are private capabilities, not imported patches.
use super::*;
use crate::desktop::store::{DesktopPatch, LatestChange, RawValue, WidgetPatch};

#[derive(Clone, Debug)]
pub struct RestorePlan {
    pub snapshot: BaselineInfo,
    pub domains: Vec<RecoveryDomain>,
    pub skipped: Vec<String>,
    pub missing_assets: Vec<String>,
    pub(crate) patch: DesktopPatch,
    pub(crate) widgets: WidgetPatch,
    identity: String,
}
impl RestorePlan {
    pub fn changes(&self) -> usize {
        self.patch.len() + self.widgets.changes().count()
    }
}

pub fn plan_restore(
    store: &DesktopStore,
    domains: &[RecoveryDomain],
) -> Result<RestorePlan, DesktopError> {
    let snapshot = load_baseline(store)?.ok_or_else(|| failed("no desktop snapshot yet"))?;
    let identity = digest(&serde_json::to_vec(&snapshot).map_err(failed)?);
    let selected = domains
        .iter()
        .copied()
        .filter(|d| snapshot.supported_domains.contains(d))
        .collect();
    let mut plan = RestorePlan {
        snapshot,
        domains: selected,
        skipped: vec![],
        missing_assets: vec![],
        patch: DesktopPatch::exact(),
        widgets: WidgetPatch::new(),
        identity,
    };
    let owned = owned_keys(store)?;
    let raw = store
        .app_config()
        .map(|p| {
            crate::config::read_config_text(p)
                .and_then(|text| crate::config::parse_config_text(text.as_deref()))
        })
        .transpose()
        .map_err(failed)?;
    for domain in &plan.snapshot.domains {
        if !domains.contains(&domain.domain) {
            continue;
        }
        if domain.motion.is_some() {
            plan.skipped.push("App animations: separate GSettings preference; exported only. Desktop Restore and Undo do not change it.".into());
        }
        for record in &domain.native {
            let name = format!(
                "{}/v{}/{}",
                record.namespace,
                record.version,
                record.key_name()?.to_string_lossy()
            );
            let Some(key) = record.supported_key() else {
                if record.bytes.is_some() {
                    plan.skipped.push(name);
                }
                continue;
            };
            if [
                dock_islands_core::NAMESPACE,
                dock_islands_core::material::NAMESPACE,
            ]
            .contains(&key.namespace())
                && !super::super::islands::Installation::discover(store).installed()
            {
                if record.bytes.is_some() {
                    plan.skipped.push(name);
                }
                continue;
            }
            let desired = match record.contents()?.map(String::from_utf8).transpose() {
                Ok(text) => text.map(RawValue::parse).transpose(),
                Err(_) => {
                    plan.skipped.push(name);
                    continue;
                }
            };
            let desired = match desired {
                Ok(desired)
                    if desired.as_ref().is_none_or(|v| {
                        (super::super::store::rule_for(&key).unwrap().validate)(v).is_ok()
                    }) =>
                {
                    desired
                }
                _ => {
                    plan.skipped.push(name);
                    continue;
                }
            };
            if let Some(value) = &desired {
                if super::super::wallpaper::modes::is_background(key.namespace()) {
                    if let Ok(entry) = value.decode::<super::super::wallpaper::BgEntry>() {
                        if let super::super::wallpaper::BgSource::Path(path) = entry.source {
                            if !path.exists() {
                                plan.missing_assets.push(path.display().to_string());
                                plan.skipped.push(name);
                                continue;
                            }
                        }
                    }
                }
            }
            let current = match read_native(store, &key) {
                Ok(current) => current,
                Err(_) => {
                    plan.skipped.push(name);
                    continue;
                }
            };
            if current.as_ref().is_some_and(|v| {
                (super::super::store::rule_for(&key).unwrap().validate)(v).is_err()
            }) {
                plan.skipped.push(name);
                continue;
            }
            // Captured absence is evidence of absence, not ownership of a later
            // outside override. Only keys actually created by Halcyon may go.
            if desired.is_none() && current.is_some() && !owned.contains(&key) {
                plan.skipped.push(name);
                continue;
            }
            plan.patch.set(key, current, desired);
        }
        for leaf in &domain.app {
            let Some(raw) = &raw else {
                continue;
            };
            let desired = leaf.present.then(|| leaf.value.clone());
            if desired
                .as_ref()
                .is_some_and(|v| widget::field(&leaf.pointer).is_none_or(|f| !f.accepts(v)))
            {
                plan.skipped.push(leaf.pointer.clone());
                continue;
            }
            plan.widgets
                .set(&leaf.pointer, widget::get(raw, &leaf.pointer), desired)
                .map_err(failed)?;
        }
    }
    // Wildcard/output and newly created bar keys have no individual absent
    // record. Only creation evidence plus a known original namespace inventory
    // authorizes their removal; no namespace is ever removed.
    for key in owned {
        if !domains.contains(
            &domain_of(key.namespace(), key.key()).unwrap_or(RecoveryDomain::AppAnimations),
        ) {
            continue;
        }
        let known = plan
            .snapshot
            .domains
            .iter()
            .filter(|d| domains.contains(&d.domain))
            .any(|d| {
                d.inventory
                    .iter()
                    .any(|i| i.namespace == key.namespace() && i.version == key.version())
                    && !d
                        .native
                        .iter()
                        .any(|r| r.supported_key().as_ref() == Some(&key))
            });
        if known {
            if [
                dock_islands_core::NAMESPACE,
                dock_islands_core::material::NAMESPACE,
            ]
            .contains(&key.namespace())
                && !super::super::islands::Installation::discover(store).installed()
            {
                continue;
            }
            if let Some(rule) = super::super::store::rule_for(&key) {
                if let Ok(current) = read_native(store, &key) {
                    if current
                        .as_ref()
                        .is_none_or(|value| (rule.validate)(value).is_ok())
                    {
                        plan.patch.set(key, current, None);
                    } else {
                        plan.skipped.push(key.to_string());
                    }
                }
            }
        }
    }
    // A snapshot taken before Halcyon knew about per-mode backgrounds has
    // only the shared ones; Apply copies those to both modes. A newer one
    // restores each namespace exactly.
    let is_mode = super::super::wallpaper::modes::is_mode;
    plan.patch.exact_backgrounds = plan.snapshot.domains.iter().any(|domain| {
        domain
            .native
            .iter()
            .any(|record| is_mode(&record.namespace))
            || domain.inventory.iter().any(|item| is_mode(&item.namespace))
    });
    Ok(plan)
}

pub fn apply_restore(
    store: &DesktopStore,
    plan: &RestorePlan,
) -> Result<LatestChange, DesktopError> {
    store.restore_desktop(plan)
}

pub(crate) fn verify_plan(store: &DesktopStore, plan: &RestorePlan) -> Result<(), DesktopError> {
    let snapshot = load_baseline(store)?.ok_or_else(|| failed("snapshot disappeared"))?;
    if digest(&serde_json::to_vec(&snapshot).map_err(failed)?) != plan.identity {
        return Err(failed("snapshot changed; review Restore again"));
    }
    for (key, value) in plan
        .patch
        .iter()
        .map(|(k, e)| (k, &e.desired))
        .chain(plan.patch.guards())
    {
        if super::super::wallpaper::modes::is_background(key.namespace()) {
            if let Some(raw) = value {
                if let Ok(entry) = raw.decode::<super::super::wallpaper::BgEntry>() {
                    if let super::super::wallpaper::BgSource::Path(path) = entry.source {
                        if !path.exists() {
                            return Err(failed(
                                "saved wallpaper disappeared; review Restore again",
                            ));
                        }
                    }
                }
            }
        }
    }
    Ok(())
}

pub fn export_baseline(store: &DesktopStore) -> Result<String, DesktopError> {
    // A damaged checksum/version is never restore authority. Salvage only
    // scoped records into a clearly unverified evidence envelope, never copy
    // arbitrary header fields or a raw possibly-content-bearing manifest.
    match load_baseline(store) {
        Ok(Some(snapshot)) => return serde_json::to_string_pretty(&snapshot).map_err(failed),
        Ok(None) => return Err(failed("no desktop snapshot yet")),
        Err(_) => {}
    }
    let (bytes, _) = read_path(
        &store
            .state_dir()
            .join(DIRECTORY)
            .join("original/manifest.json"),
        MAX_MANIFEST_BYTES,
    )?
    .ok_or_else(|| failed("missing manifest"))?;
    let value: serde_json::Value = serde_json::from_slice(&bytes).map_err(failed)?;
    let mut native = Vec::new();
    let mut app = Vec::new();
    let mut total = 0;
    for domain in value["domains"]
        .as_array()
        .ok_or_else(|| failed("unreadable domain inventory"))?
    {
        for raw in domain["native"].as_array().into_iter().flatten() {
            let Ok(mut record) = serde_json::from_value::<NativeRecord>(raw.clone()) else {
                continue;
            };
            let Ok(name) = record.key_name() else {
                continue;
            };
            if ConfigKey::new(&record.namespace, record.version, "checked").is_err()
                || domain_of(&record.namespace, name.to_str().unwrap_or("")).is_none()
                || (!whole_namespace(&record.namespace) && record.supported_key().is_none())
            {
                continue;
            }
            let Ok(contents) = record.contents() else {
                continue;
            };
            let size = contents.as_ref().map_or(0, Vec::len);
            if size > MAX_FILE_BYTES || total + size > MAX_TOTAL_BYTES || native.len() >= MAX_FILES
            {
                continue;
            }
            total += size;
            record.sha256 = contents.as_ref().map(|b| digest(b));
            record.mode = contents.as_ref().map(|_| 0o600);
            native.push(record);
        }
        for raw in domain["app"].as_array().into_iter().flatten() {
            let Ok(leaf) = serde_json::from_value::<AppLeaf>(raw.clone()) else {
                continue;
            };
            if !app_pointers().any(|p| p == leaf.pointer)
                || (!leaf.present && !leaf.value.is_null())
            {
                continue;
            }
            if leaf.present && widget::field(&leaf.pointer).is_none_or(|f| !f.accepts(&leaf.value))
            {
                continue;
            }
            let size = serde_json::to_vec(&leaf).map_err(failed)?.len();
            if total + size > MAX_TOTAL_BYTES {
                continue;
            }
            total += size;
            app.push(leaf);
        }
    }
    serde_json::to_string_pretty(&serde_json::json!({"format":"halcyon-desktop-evidence", "version":1, "verified":false, "native":native, "app":app})).map_err(failed)
}

pub(crate) fn save_pre_restore(store: &DesktopStore) -> Result<(), DesktopError> {
    let snapshot = capture(store, BaselineKind::CurrentDesktop)?;
    validate(&snapshot)?;
    let bytes = serde_json::to_vec(&snapshot).map_err(failed)?;
    let dir = Directory::open(&store.state_dir().join(DIRECTORY).join("pre-restore"), true)
        .map_err(failed)?
        .unwrap();
    let name = format!("{}.json", getrandom::u64().map_err(failed)?);
    let pending = format!("pending-{name}");
    dir.write_new(&pending, &bytes).map_err(failed)?;
    dir.rename(&pending, &name, false).map_err(failed)
}

pub(crate) fn read_native(
    store: &DesktopStore,
    key: &ConfigKey,
) -> Result<Option<RawValue>, DesktopError> {
    read_path(&store.user_path(key), MAX_FILE_BYTES)?
        .map(|(bytes, _)| {
            String::from_utf8(bytes)
                .map(RawValue::from_disk)
                .map_err(failed)
        })
        .transpose()
}

pub(crate) fn write_scoped(path: &Path, bytes: Option<&[u8]>) -> io::Result<()> {
    write_confined(path, bytes, false)
}
pub(crate) fn write_native(path: &Path, bytes: Option<&[u8]>) -> io::Result<()> {
    write_confined(path, bytes, true)
}
fn write_confined(path: &Path, bytes: Option<&[u8]>, preserve_mode: bool) -> io::Result<()> {
    let parent = path
        .parent()
        .ok_or_else(|| io::Error::other("missing parent"))?;
    let Some(dir) = Directory::open(parent, bytes.is_some())? else {
        return Ok(());
    };
    let name = path
        .file_name()
        .and_then(OsStr::to_str)
        .ok_or_else(|| io::Error::other("invalid filename"))?;
    if let Some(bytes) = bytes {
        let pending = format!(".restore-{}", getrandom::u64().map_err(io::Error::other)?);
        let mode = if preserve_mode {
            dir.read(OsStr::new(name), MAX_FILE_BYTES)?
                .map(|(_, mode)| mode)
                .unwrap_or(0o600)
        } else {
            0o600
        };
        dir.write_new_mode(&pending, bytes, mode)?;
        dir.rename(&pending, name, true)
    } else {
        let name = component(name.as_bytes())?;
        let rc = unsafe { libc::unlinkat(dir.0.as_raw_fd(), name.as_ptr(), 0) };
        if rc != 0 && io::Error::last_os_error().kind() != io::ErrorKind::NotFound {
            return Err(io::Error::last_os_error());
        }
        dir.0.sync_all()
    }
}

const OWNED: &str = "created-native-keys.json";
fn owned_keys(store: &DesktopStore) -> Result<BTreeSet<ConfigKey>, DesktopError> {
    read_path(
        &store.state_dir().join(DIRECTORY).join(OWNED),
        MAX_TOTAL_BYTES,
    )?
    .map(|(bytes, _)| serde_json::from_slice(&bytes).map_err(failed))
    .transpose()
    .map(Option::unwrap_or_default)
}
pub(crate) fn record_created(store: &DesktopStore, key: &ConfigKey) -> Result<(), DesktopError> {
    let mut owned = owned_keys(store)?;
    owned.insert(key.clone());
    if owned.len() > MAX_FILES {
        return Err(failed("created-key inventory is full"));
    }
    write_scoped(
        &store.state_dir().join(DIRECTORY).join(OWNED),
        Some(&serde_json::to_vec(&owned).map_err(failed)?),
    )
    .map_err(failed)
}

/// Private export uses the same descriptor traversal as recovery storage.
/// Refuse linked/special destinations; never inherit profile-export modes.
pub fn write_export(path: &Path, bytes: &[u8], confine: Option<&Path>) -> Result<(), DesktopError> {
    if !path.is_absolute() || bytes.len() > MAX_MANIFEST_BYTES {
        return Err(failed("choose an absolute local recovery export path"));
    }
    if let Some(root) = confine {
        if !path.starts_with(root) {
            return Err(failed("export must remain inside the fixture root"));
        }
        Directory::open(root, false)
            .map_err(failed)?
            .ok_or_else(|| failed("missing fixture root"))?;
    }
    let parent = path
        .parent()
        .ok_or_else(|| failed("missing export folder"))?;
    let name = path
        .file_name()
        .and_then(OsStr::to_str)
        .ok_or_else(|| failed("invalid export name"))?;
    let dir = Directory::open(parent, false)
        .map_err(failed)?
        .ok_or_else(|| failed("missing export folder"))?;
    // Bounded no-follow inspection also rejects linked and special leaves.
    dir.read(OsStr::new(name), MAX_MANIFEST_BYTES)
        .map_err(failed)?;
    let pending = format!(".recovery-export-{}", getrandom::u64().map_err(failed)?);
    dir.write_new(&pending, bytes).map_err(failed)?;
    dir.rename(&pending, name, true).map_err(failed)
}
