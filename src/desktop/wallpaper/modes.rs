//! Per-mode backgrounds (COSMIC 1.11 and later).
//!
//! Source (pinned): cosmic-bg 1.11 `config/src/lib.rs` (`Mode`,
//! `Context::seed_from`, `Entry::transition_duration`) and `src/main.rs`
//! (`Mode::current`, one context per mode). cosmic-bg keeps one set of the
//! `all` / `same-on-all` / `backgrounds` / `output.<name>` keys per theme
//! mode, under `com.system76.CosmicBackground.Dark` and `.Light`, and shows
//! the set of the mode `com.system76.CosmicTheme.Mode/v1/is_dark` names
//! (dark when unset). It seeds a mode from the shared namespace only while
//! that mode has no `all`, and ignores the shared namespace afterwards.
//!
//! Halcyon keeps describing a background with the shared key names. When
//! per-mode sets are in use, it reads the set of the current mode and every
//! write goes to the shared namespace and to both modes (`mirror`), so the
//! picture stays the same when the mode changes and older COSMIC keeps
//! working. Without per-mode sets nothing here changes a read or a write.
use super::{BgEntry, NAMESPACE, OUTPUT_PREFIX, SHARED, plain_duration};
use crate::desktop::appearance::{MODE, decode_exact};
use crate::desktop::store::{
    ChangeEntry, ConfigKey, DesktopError, DesktopPatch, DesktopStore, RawValue,
};
use serde::Deserialize;
use std::path::Path;

pub const DARK: &str = "com.system76.CosmicBackground.Dark";
pub const LIGHT: &str = "com.system76.CosmicBackground.Light";
/// The mode sets, dark first (cosmic-bg's `Mode::ALL`).
pub const MODES: [&str; 2] = [DARK, LIGHT];
/// Every namespace a background key can live in, shared first.
pub const NAMESPACES: [&str; 3] = [NAMESPACE, DARK, LIGHT];

/// Whether `namespace` holds background keys: shared or one of the modes.
pub fn is_background(namespace: &str) -> bool {
    NAMESPACES.contains(&namespace)
}

/// Whether `namespace` is one of the per-mode sets.
pub fn is_mode(namespace: &str) -> bool {
    MODES.contains(&namespace)
}

/// `com.system76.CosmicTheme.Mode/v1/is_dark`, which picks the mode set.
pub fn mode_key() -> ConfigKey {
    ConfigKey::new(MODE, 1, "is_dark").expect("mode key is valid")
}

/// Whether this desktop keeps per-mode backgrounds: a mode set exists in
/// the user configuration or in the installed defaults. cosmic-bg 1.11
/// ships defaults for both modes and creates the user directories at
/// startup; older versions never create either, so this tells them apart
/// without asking a running program for its version. A fixture only counts
/// defaults inside its own folder, so tests never depend on the machine.
pub fn per_mode(store: &DesktopStore) -> bool {
    let fixture = store.fixture_root();
    MODES.iter().any(|namespace| {
        let dir = Path::new(namespace).join("v1");
        store.user_root().join(&dir).is_dir()
            || store
                .system_roots()
                .iter()
                .filter(|root| fixture.is_none_or(|fixture| root.starts_with(fixture)))
                .any(|root| root.join(&dir).is_dir())
    })
}

/// Whether the dark set is shown (cosmic-bg's `Mode::current`: dark unless
/// `is_dark` reads `false`).
pub fn is_dark(store: &DesktopStore) -> bool {
    let key = mode_key();
    store
        .load(std::slice::from_ref(&key))
        .ok()
        .and_then(|snapshot| {
            snapshot
                .effective(&key)
                .and_then(|raw| raw.decode::<bool>().ok())
        })
        .unwrap_or(true)
}

/// The namespace whose `all` cosmic-bg shows now: the set of the current
/// mode, or the shared namespace when that set has no `all` yet (cosmic-bg
/// then seeds it from the shared one) or there are no sets.
pub fn current(store: &DesktopStore) -> &'static str {
    if !per_mode(store) {
        return NAMESPACE;
    }
    let mode = if is_dark(store) { DARK } else { LIGHT };
    let all = rehome(&super::shared_key(), mode);
    let seeded = store
        .load(std::slice::from_ref(&all))
        .is_ok_and(|snapshot| snapshot.effective(&all).is_some());
    if seeded { mode } else { NAMESPACE }
}

/// `key` in `namespace` (same version and name).
pub fn rehome(key: &ConfigKey, namespace: &str) -> ConfigKey {
    if key.namespace() == namespace {
        return key.clone();
    }
    ConfigKey::new(namespace, key.version(), key.key()).expect("background key is valid")
}

/// The shared name of a background key from any of the three namespaces;
/// other keys are returned as they are.
pub fn logical(key: &ConfigKey) -> ConfigKey {
    if is_background(key.namespace()) {
        rehome(key, NAMESPACE)
    } else {
        key.clone()
    }
}

/// Only `transition_duration` of a stored entry, read without this
/// version's bounds.
#[derive(Deserialize)]
struct Fade {
    #[serde(default, with = "plain_duration")]
    transition_duration: Option<u64>,
}

fn fade_of(raw: &RawValue) -> Option<u64> {
    raw.decode::<Fade>()
        .ok()
        .and_then(|fade| fade.transition_duration)
}

fn is_entry_key(key: &ConfigKey) -> bool {
    key.key() == SHARED || key.key().starts_with(OUTPUT_PREFIX)
}

/// `desired` as written over `current` in another namespace: an entry
/// keeps the fade `current` has (none when it has none, so cosmic-bg uses
/// its default), everything else is copied as it is. A value this version
/// can't rewrite exactly is copied unchanged.
pub fn adapt(
    key: &ConfigKey,
    desired: Option<&RawValue>,
    current: Option<&RawValue>,
) -> Option<RawValue> {
    let desired = desired?;
    if !is_entry_key(key) {
        return Some(desired.clone());
    }
    let Ok(mut entry) = decode_exact::<BgEntry>(desired) else {
        return Some(desired.clone());
    };
    let fade = current.and_then(fade_of);
    if entry.transition_duration == fade {
        return Some(desired.clone());
    }
    entry.transition_duration = fade;
    Some(RawValue::encode(&entry).unwrap_or_else(|_| desired.clone()))
}

/// Whether a recorded change already names a per-mode namespace, so its
/// Undo restores each namespace exactly instead of copying between them.
pub(crate) fn names_modes(entries: &[ChangeEntry]) -> bool {
    entries.iter().any(|entry| is_mode(entry.key.namespace()))
}

/// `patch` with every background key it writes also written in the
/// namespaces it leaves out, so cosmic-bg shows the change in both modes
/// and older COSMIC reads it from the shared namespace. A key the patch
/// guards keeps its guarded value as the original, so outside changes are
/// still refused; any other original is read now (under the desktop lock).
///
/// Undo and Restore call this too, unless their record already names a
/// mode (`DesktopPatch::exact_backgrounds`): a record made before Halcyon
/// knew about the modes then restores its shared value to all three.
pub(crate) fn mirror(
    store: &DesktopStore,
    patch: &DesktopPatch,
) -> Result<DesktopPatch, DesktopError> {
    let mut mirrored = patch.clone();
    let shown = current(store);
    // Where a key name's value comes from: the shown mode, then the shared
    // namespace, then the other mode.
    let mut preference: Vec<&str> = vec![shown];
    preference.extend(NAMESPACES.iter().copied().filter(|ns| *ns != shown));
    let mut names: Vec<ConfigKey> = patch
        .iter()
        .map(|(key, _)| key)
        .filter(|key| is_background(key.namespace()) && key.version() == 1)
        .map(logical)
        .collect();
    names.sort();
    names.dedup();
    for name in names {
        let Some(source) = preference
            .iter()
            .find_map(|namespace| patch.get(&rehome(&name, namespace)))
        else {
            continue;
        };
        let desired = source.desired.clone();
        for namespace in NAMESPACES {
            let key = rehome(&name, namespace);
            if patch.get(&key).is_some() {
                continue;
            }
            let original = match patch.guards().find(|(guarded, _)| **guarded == key) {
                Some((_, original)) => original.clone(),
                None => store.read_user(&key)?,
            };
            let value = adapt(&key, desired.as_ref(), original.as_ref());
            mirrored.set(key, original, value);
        }
    }
    Ok(mirrored)
}

#[cfg(test)]
mod tests {
    use super::super::{BgSource, Target, WallpaperDraft, same_key, shared_key};
    use super::*;
    use crate::desktop::store::tests::Fixture;
    use crate::desktop::store::{AppliedChange, LatestChange, Owner, Source, rule_for};

    fn raw(text: &str) -> RawValue {
        RawValue::parse(text).unwrap()
    }
    fn entry(picture: &str, fade: Option<u64>) -> String {
        let fade = fade.map_or(String::new(), |fade| {
            format!(", transition_duration: {fade}")
        });
        format!(
            r#"(output: "all", source: Path("{picture}"), filter_by_theme: false, rotation_frequency: 300{fade}, filter_method: Lanczos, scaling_mode: Zoom, sampling_method: Alphanumeric)"#
        )
    }
    fn stored(fixture: &Fixture, namespace: &str) -> BgEntry {
        RawValue::parse(fixture.read(namespace, SHARED).expect("written"))
            .unwrap()
            .decode()
            .unwrap()
    }
    fn picture(entry: &BgEntry) -> String {
        match &entry.source {
            BgSource::Path(path) => path.display().to_string(),
            other => format!("{other:?}"),
        }
    }
    fn shown(fixture: &Fixture) -> String {
        let draft = WallpaperDraft::load(&fixture.store()).unwrap();
        picture(draft.effective(&Target::Shared).known().unwrap())
    }
    /// A COSMIC 1.11 desktop: cosmic-bg seeded both modes, the light one
    /// with a fade of its own; the shared namespace still has the old one.
    fn per_mode_fixture(name: &str) -> Fixture {
        let fixture = Fixture::new(name);
        fixture.user(NAMESPACE, SHARED, &entry("/old/shared.jpg", None));
        fixture.user(DARK, SHARED, &entry("/old/dark.jpg", None));
        fixture.user(LIGHT, SHARED, &entry("/old/light.jpg", Some(750)));
        fixture
    }
    fn plan(store: &DesktopStore, picture: &str) -> DesktopPatch {
        let mut draft = WallpaperDraft::load(store).unwrap();
        draft.set_source(&Target::Shared, BgSource::Path(picture.into()));
        let plan = draft.plan();
        assert!(plan.blocked.is_empty(), "{:?}", plan.blocked);
        plan.patch
    }
    fn pick(store: &DesktopStore, picture: &str) -> AppliedChange {
        store.apply(&plan(store, picture)).unwrap()
    }
    fn pick_action(store: &DesktopStore, picture: &str) -> LatestChange {
        let patch = plan(store, picture);
        store
            .apply_action(&patch, None, Owner::Wallpaper, "Wallpaper", Source::Manual)
            .unwrap()
    }

    #[test]
    fn transition_duration_survives_a_round_trip() {
        let stored = raw(&entry("/a.jpg", Some(1200)));
        super::super::check_entry(&stored).unwrap();
        let decoded: BgEntry = decode_exact(&stored).unwrap();
        assert_eq!(decoded.transition_duration, Some(1200));
        let encoded = RawValue::encode(&decoded).unwrap();
        assert!(encoded.as_str().contains("transition_duration: 1200"));
        // Absent stays absent, so older cosmic-bg never sees the member.
        let plain: BgEntry = decode_exact(&raw(&entry("/a.jpg", None))).unwrap();
        assert_eq!(plain.transition_duration, None);
        assert!(
            !RawValue::encode(&plain)
                .unwrap()
                .as_str()
                .contains("transition")
        );
    }

    #[test]
    fn adapt_keeps_the_fade_of_the_entry_it_replaces() {
        let key = shared_key();
        let desired = raw(&entry("/a.jpg", None));
        let faded = raw(&entry("/b.jpg", Some(750)));
        let adapted = adapt(&key, Some(&desired), Some(&faded)).unwrap();
        let entry: BgEntry = decode_exact(&adapted).unwrap();
        assert_eq!(entry.transition_duration, Some(750));
        assert_eq!(picture(&entry), "/a.jpg");
        // Without a fade to keep, none is written.
        let plain = adapt(&key, Some(&faded), None).unwrap();
        assert!(!plain.as_str().contains("transition_duration"));
        // Other keys are copied as they are.
        let same = raw("true");
        assert_eq!(
            adapt(&same_key(), Some(&same), Some(&raw("false"))),
            Some(same)
        );
    }

    #[test]
    fn a_pick_is_written_to_the_shared_namespace_and_both_modes() {
        let fixture = per_mode_fixture("modes-write");
        let store = fixture.store();
        assert!(per_mode(&store));
        pick(&store, "/new/pick.jpg");
        for namespace in NAMESPACES {
            assert_eq!(
                picture(&stored(&fixture, namespace)),
                "/new/pick.jpg",
                "{namespace}"
            );
        }
        // Each namespace keeps its own fade; none is added where there was none.
        assert_eq!(stored(&fixture, LIGHT).transition_duration, Some(750));
        assert_eq!(stored(&fixture, DARK).transition_duration, None);
        assert_eq!(stored(&fixture, NAMESPACE).transition_duration, None);
        assert!(
            !fixture
                .read(NAMESPACE, SHARED)
                .unwrap()
                .contains("transition")
        );
    }

    #[test]
    fn a_mode_without_its_own_entry_still_gets_the_pick() {
        // cosmic-bg created only the directories so far.
        let fixture = Fixture::new("modes-empty");
        fixture.user(NAMESPACE, SHARED, &entry("/old/shared.jpg", None));
        for namespace in MODES {
            std::fs::create_dir_all(fixture.0.join("config/cosmic").join(namespace).join("v1"))
                .unwrap();
        }
        let store = fixture.store();
        assert!(per_mode(&store));
        // Until cosmic-bg seeds the mode, it shows the shared entry.
        assert_eq!(current(&store), NAMESPACE);
        assert_eq!(shown(&fixture), "/old/shared.jpg");
        pick(&store, "/new/pick.jpg");
        for namespace in NAMESPACES {
            assert_eq!(
                picture(&stored(&fixture, namespace)),
                "/new/pick.jpg",
                "{namespace}"
            );
        }
    }

    #[test]
    fn the_shown_mode_is_read_and_follows_is_dark() {
        let fixture = per_mode_fixture("modes-read");
        // Dark unless COSMIC says otherwise.
        assert_eq!(current(&fixture.store()), DARK);
        assert_eq!(shown(&fixture), "/old/dark.jpg");
        fixture.user(MODE, "is_dark", "false");
        assert_eq!(current(&fixture.store()), LIGHT);
        assert_eq!(shown(&fixture), "/old/light.jpg");
        fixture.user(MODE, "is_dark", "true");
        assert_eq!(shown(&fixture), "/old/dark.jpg");
        // The page reloads when the mode or any set changes.
        let draft = WallpaperDraft::load(&fixture.store()).unwrap();
        let watched = draft.watch_keys();
        assert!(watched.contains(&mode_key()));
        for namespace in NAMESPACES {
            assert!(
                watched.contains(&rehome(&shared_key(), namespace)),
                "{namespace}"
            );
        }
    }

    #[test]
    fn without_mode_sets_everything_stays_shared() {
        let fixture = Fixture::new("modes-none");
        fixture.user(NAMESPACE, SHARED, &entry("/old/shared.jpg", None));
        fixture.user(MODE, "is_dark", "false");
        let store = fixture.store();
        assert!(!per_mode(&store));
        assert_eq!(current(&store), NAMESPACE);
        assert_eq!(shown(&fixture), "/old/shared.jpg");
        let draft = WallpaperDraft::load(&store).unwrap();
        assert!(!draft.watch_keys().contains(&mode_key()));
        let change = pick(&store, "/new/pick.jpg");
        assert_eq!(change.entries().len(), 1);
        assert_eq!(picture(&stored(&fixture, NAMESPACE)), "/new/pick.jpg");
        for namespace in MODES {
            assert!(!fixture.0.join("config/cosmic").join(namespace).exists());
        }
    }

    #[test]
    fn undo_restores_every_namespace_exactly() {
        let fixture = per_mode_fixture("modes-undo");
        let store = fixture.store();
        let before: Vec<_> = NAMESPACES
            .iter()
            .map(|namespace| fixture.read(namespace, SHARED))
            .collect();
        let latest = pick_action(&store, "/new/pick.jpg");
        assert_eq!(latest.change.entries().len(), 3);
        assert!(names_modes(latest.change.entries()));
        store.undo_latest(&latest.id).unwrap();
        let after: Vec<_> = NAMESPACES
            .iter()
            .map(|namespace| fixture.read(namespace, SHARED))
            .collect();
        assert_eq!(after, before);
    }

    #[test]
    fn an_older_shared_only_record_undoes_into_both_modes() {
        // Applied before COSMIC kept per-mode backgrounds.
        let fixture = Fixture::new("modes-legacy");
        fixture.user(NAMESPACE, SHARED, &entry("/old/shared.jpg", None));
        let store = fixture.store();
        let latest = pick_action(&store, "/new/pick.jpg");
        assert!(!names_modes(latest.change.entries()));
        // COSMIC 1.11 then seeded both modes from the shared entry.
        fixture.user(DARK, SHARED, &entry("/new/pick.jpg", None));
        fixture.user(LIGHT, SHARED, &entry("/new/pick.jpg", Some(750)));
        store.undo_latest(&latest.id).unwrap();
        for namespace in NAMESPACES {
            assert_eq!(
                picture(&stored(&fixture, namespace)),
                "/old/shared.jpg",
                "{namespace}"
            );
        }
        assert_eq!(stored(&fixture, LIGHT).transition_duration, Some(750));
    }

    #[test]
    fn mode_keys_are_whitelisted_like_the_shared_ones() {
        for namespace in NAMESPACES {
            for name in [SHARED, "same-on-all", "backgrounds", "output.DP-1"] {
                let key = ConfigKey::new(namespace, 1, name).unwrap();
                assert!(rule_for(&key).is_some(), "{namespace} {name}");
            }
        }
        assert!(
            rule_for(&ConfigKey::new("com.system76.CosmicBackground.Sepia", 1, SHARED).unwrap())
                .is_none()
        );
    }
}
