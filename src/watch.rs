//! Watch the containing directories so atomic config/command replacements are seen.
//! Only changed contents reach the UI; the fallback also recovers missed events.
use cosmic::iced::{Subscription, futures::SinkExt, stream};
pub(crate) mod dir;
use dir::DirWatch;
use std::path::PathBuf;
use std::time::Duration;

pub fn files(paths: Vec<PathBuf>) -> Subscription<()> {
    Subscription::run_with(paths, |paths| changes(paths.clone()))
}

/// Observe settings owned by the desktop without creating namespaces or
/// files. If a namespace is not present yet, the fallback notices it once
/// created; the desktop remains the sole owner of its directories.
pub fn read_only_files(paths: Vec<PathBuf>) -> Subscription<()> {
    Subscription::run_with(paths, |paths| changes_with(paths.clone(), false))
}

fn changes(paths: Vec<PathBuf>) -> impl cosmic::iced::futures::Stream<Item = ()> {
    changes_with(paths, true)
}

fn changes_with(
    paths: Vec<PathBuf>,
    create_parents: bool,
) -> impl cosmic::iced::futures::Stream<Item = ()> {
    stream::channel(1, async move |mut output| {
        let mut watch = watch_parents(&paths, create_parents);
        let mut previous = None;
        let mut fallback = tokio::time::interval(Duration::from_secs(30));
        fallback.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        loop {
            tokio::select! {
                woke = changed(&mut watch, &paths), if watch.is_some() => {
                    if woke.is_err() { watch = None; }
                    // Coalesce the write/rename pair, and avoid reading a partial write.
                    settle(&mut watch, &paths).await;
                }
                _ = fallback.tick() => {}
            }
            let current: Vec<_> = paths.iter().map(|path| snapshot(path)).collect();
            if previous.as_ref() != Some(&current) {
                previous = Some(current);
                if output.send(()).await.is_err() {
                    break;
                }
            }
        }
    })
}

/// Watches the folders holding `paths`, creating them first when asked.
/// `None` when inotify is not available; the fallback still polls.
fn watch_parents(paths: &[PathBuf], create: bool) -> Option<DirWatch> {
    let mut watch = DirWatch::new().ok()?;
    rewatch(&mut watch, paths, create);
    Some(watch)
}
/// Watches each existing parent again: one that was removed and made
/// again, as some writers replace a whole namespace, needs a new watch.
fn rewatch(watch: &mut DirWatch, paths: &[PathBuf], create: bool) {
    let mut parents = std::collections::BTreeSet::new();
    for parent in paths.iter().filter_map(|path| path.parent()) {
        if parents.insert(parent) {
            if create {
                let _ = std::fs::create_dir_all(parent);
            }
            let _ = watch.watch(parent);
        }
    }
}
/// Waits for a change to one of `paths`.
async fn changed(watch: &mut Option<DirWatch>, paths: &[PathBuf]) -> std::io::Result<()> {
    match watch {
        Some(watch) => watch.changed(|path| paths.iter().any(|p| p == path)).await,
        None => std::future::pending().await,
    }
}
/// Lets a burst of changes finish, then watches the folders again.
async fn settle(watch: &mut Option<DirWatch>, paths: &[PathBuf]) {
    tokio::time::sleep(Duration::from_millis(25)).await;
    if let Some(watch) = watch {
        rewatch(watch, paths, false);
    }
}

/// Files up to this size are compared by contents; larger ones by metadata.
/// Settings and state files sit well below it; wallpaper pictures above it.
const CONTENTS_LIMIT: u64 = 256 * 1024;

/// What a file watch compares between wakes. Small files compare contents,
/// so rewriting a setting with the same value stays quiet. Larger files,
/// such as the wallpaper picture in the wallpaper and depth watches, compare
/// metadata, so the 30 s fallback never rereads or holds megabytes of pixels.
#[derive(Debug, PartialEq, Eq)]
enum Snapshot {
    Contents(Vec<u8>),
    Stamp(FileStamp),
}
fn snapshot(path: &std::path::Path) -> Option<Snapshot> {
    let stamp = file_stamp(path)?;
    if stamp.len > CONTENTS_LIMIT {
        return Some(Snapshot::Stamp(stamp));
    }
    std::fs::read(path).ok().map(Snapshot::Contents)
}

/// Image watches compare metadata, never reread multi-megabyte pixels. The
/// caller's decode stamp closes the gap between loading and installing a watch.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct FileStamp {
    len: u64,
    modified: (i64, i64),
    inode: u64,
    device: u64,
}
pub fn file_stamp(path: &std::path::Path) -> Option<FileStamp> {
    use std::os::unix::fs::MetadataExt;
    let m = std::fs::metadata(path).ok()?;
    Some(FileStamp {
        len: m.len(),
        modified: (m.mtime(), m.mtime_nsec()),
        inode: m.ino(),
        device: m.dev(),
    })
}
pub fn image_files(paths: Vec<(PathBuf, Option<FileStamp>)>) -> Subscription<()> {
    Subscription::run_with(paths, |paths| image_changes(paths.clone()))
}
fn image_changes(
    paths: Vec<(PathBuf, Option<FileStamp>)>,
) -> impl cosmic::iced::futures::Stream<Item = ()> {
    stream::channel(1, async move |mut output| {
        let watched = paths.iter().map(|(p, _)| p.clone()).collect::<Vec<_>>();
        let mut watch = watch_parents(&watched, false);
        let mut previous = paths.iter().map(|(_, s)| *s).collect::<Vec<_>>();
        let mut fallback = tokio::time::interval(Duration::from_secs(30));
        fallback.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        loop {
            tokio::select! {
                _ = fallback.tick() => {}
                woke = changed(&mut watch, &watched), if watch.is_some() => {
                    if woke.is_err() { watch = None; }
                    settle(&mut watch, &watched).await;
                }
            }
            let current = paths.iter().map(|(p, _)| file_stamp(p)).collect::<Vec<_>>();
            if current != previous {
                previous = current;
                if output.send(()).await.is_err() {
                    break;
                }
            }
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use cosmic::iced::futures::{StreamExt, pin_mut};

    #[tokio::test]
    async fn rain_image_watch_ignores_initial_decode_and_catches_replace_and_decode_race() {
        let root = std::env::temp_dir().join(format!("halcyon-rain-watch-{}", std::process::id()));
        std::fs::create_dir_all(&root).unwrap();
        let path = root.join("picture.png");
        std::fs::write(&path, "original").unwrap();
        let before = file_stamp(&path);
        let events = image_changes(vec![(path.clone(), before)]);
        pin_mut!(events);
        assert!(
            tokio::time::timeout(Duration::from_millis(100), events.next())
                .await
                .is_err(),
            "initial watch must not invalidate its own decoded surface"
        );
        std::fs::write(root.join("replacement"), "new picture").unwrap();
        std::fs::rename(root.join("replacement"), &path).unwrap();
        assert_eq!(
            tokio::time::timeout(Duration::from_secs(2), events.next())
                .await
                .unwrap(),
            Some(())
        );
        let raced = image_changes(vec![(path.clone(), before)]);
        pin_mut!(raced);
        assert_eq!(
            tokio::time::timeout(Duration::from_secs(2), raced.next())
                .await
                .unwrap(),
            Some(()),
            "change between decode and watch cannot be missed"
        );
        std::fs::remove_dir_all(root).unwrap();
    }

    #[tokio::test]
    async fn read_only_watch_does_not_create_absent_desktop_namespaces() {
        let root = std::env::temp_dir().join(format!(
            "desktop-cards-read-only-watch-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&root);
        let events = changes_with(vec![root.join("namespace/v2/accent")], false);
        pin_mut!(events);
        assert_eq!(
            tokio::time::timeout(Duration::from_secs(2), events.next())
                .await
                .unwrap(),
            Some(())
        );
        assert!(!root.exists());
    }

    #[tokio::test]
    async fn atomic_replacement_wakes_ui_but_reads_and_identical_writes_do_not() {
        let directory =
            std::env::temp_dir().join(format!("desktop-cards-watch-test-{}", std::process::id()));
        std::fs::create_dir_all(&directory).unwrap();
        let path = directory.join("config.json");
        std::fs::write(&path, "old").unwrap();
        let events = changes(vec![path.clone()]);
        pin_mut!(events);
        let deadline = Duration::from_secs(2);
        assert_eq!(
            tokio::time::timeout(deadline, events.next()).await.unwrap(),
            Some(())
        );
        let replacement = directory.join("replacement.tmp");
        std::fs::write(&replacement, "new").unwrap();
        std::fs::rename(replacement, &path).unwrap();
        assert_eq!(
            tokio::time::timeout(deadline, events.next()).await.unwrap(),
            Some(())
        );
        std::fs::read(&path).unwrap();
        std::fs::write(&path, "new").unwrap();
        assert!(
            tokio::time::timeout(Duration::from_millis(150), events.next())
                .await
                .is_err()
        );
        std::fs::remove_file(&path).unwrap();
        assert_eq!(
            tokio::time::timeout(deadline, events.next()).await.unwrap(),
            Some(())
        );
        std::fs::remove_dir_all(directory).unwrap();
    }

    /// The wallpaper and depth watches include the picture itself; reading
    /// it whole on every 30 s fallback cost 615 KB a pass on a plain 1080p
    /// wallpaper, and far more on a 4K one.
    #[tokio::test]
    async fn pictures_are_compared_by_metadata_and_replacements_still_wake() {
        let directory = std::env::temp_dir().join(format!(
            "desktop-cards-picture-watch-{}",
            std::process::id()
        ));
        std::fs::create_dir_all(&directory).unwrap();
        let picture = directory.join("wallpaper.jpg");
        let setting = directory.join("setting");
        std::fs::write(&picture, vec![1u8; CONTENTS_LIMIT as usize + 1]).unwrap();
        std::fs::write(&setting, "same").unwrap();
        assert!(matches!(snapshot(&picture), Some(Snapshot::Stamp(_))));
        assert!(matches!(snapshot(&setting), Some(Snapshot::Contents(_))));
        assert_eq!(snapshot(&directory.join("absent")), None);

        let events = changes_with(vec![picture.clone(), setting.clone()], false);
        pin_mut!(events);
        let deadline = Duration::from_secs(2);
        assert_eq!(
            tokio::time::timeout(deadline, events.next()).await.unwrap(),
            Some(())
        );
        // Reading the picture changes nothing a watch compares.
        std::fs::read(&picture).unwrap();
        assert!(
            tokio::time::timeout(Duration::from_millis(150), events.next())
                .await
                .is_err()
        );
        // Replacing it, as a wallpaper change does, wakes the watch.
        let replacement = directory.join("replacement.tmp");
        std::fs::write(&replacement, vec![2u8; CONTENTS_LIMIT as usize + 1]).unwrap();
        std::fs::rename(replacement, &picture).unwrap();
        assert_eq!(
            tokio::time::timeout(deadline, events.next()).await.unwrap(),
            Some(())
        );
        std::fs::remove_dir_all(directory).unwrap();
    }
}
