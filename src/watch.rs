//! Watch the containing directories so atomic config/command replacements are seen.
//! Only changed contents reach the UI; the fallback also recovers missed events.
use cosmic::iced::{Subscription, futures::SinkExt, stream};
use notify::{EventKind, RecursiveMode, Watcher};
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
        let (sender, mut receiver) = tokio::sync::mpsc::channel(1);
        let watched = paths.clone();
        let mut watcher =
            notify::recommended_watcher(move |event: notify::Result<notify::Event>| {
                let relevant = match event {
                    Ok(event) => {
                        matches!(
                            event.kind,
                            EventKind::Create(_) | EventKind::Modify(_) | EventKind::Remove(_)
                        ) && event.paths.iter().any(|path| watched.contains(path))
                    }
                    Err(_) => true,
                };
                if relevant {
                    let _ = sender.try_send(());
                }
            })
            .ok();
        if let Some(watcher) = &mut watcher {
            let mut directories = Vec::new();
            for path in &paths {
                if let Some(parent) = path.parent() {
                    if !directories.contains(&parent) {
                        if create_parents {
                            let _ = std::fs::create_dir_all(parent);
                        }
                        let _ = watcher.watch(parent, RecursiveMode::NonRecursive);
                        directories.push(parent);
                    }
                }
            }
        }
        let mut previous = None;
        let mut fallback = tokio::time::interval(Duration::from_secs(30));
        fallback.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        loop {
            tokio::select! {
                signal = receiver.recv(), if watcher.is_some() => {
                    if signal.is_none() { watcher = None; }
                    // Coalesce the write/rename pair, and avoid reading a partial write.
                    tokio::time::sleep(Duration::from_millis(25)).await;
                    while receiver.try_recv().is_ok() {}
                }
                _ = fallback.tick() => {}
            }
            let current: Vec<_> = paths.iter().map(|path| std::fs::read(path).ok()).collect();
            if previous.as_ref() != Some(&current) {
                previous = Some(current);
                if output.send(()).await.is_err() {
                    break;
                }
            }
        }
    })
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
        let (send, mut receive) = tokio::sync::mpsc::channel(1);
        let watched = paths.iter().map(|(p, _)| p.clone()).collect::<Vec<_>>();
        let mut watcher =
            notify::recommended_watcher(move |event: notify::Result<notify::Event>| {
                if event.is_ok_and(|e| {
                    matches!(
                        e.kind,
                        EventKind::Create(_) | EventKind::Modify(_) | EventKind::Remove(_)
                    ) && e.paths.iter().any(|p| watched.contains(p))
                }) {
                    let _ = send.try_send(());
                }
            })
            .ok();
        if let Some(w) = &mut watcher {
            let mut parents = std::collections::BTreeSet::new();
            for (p, _) in &paths {
                if let Some(dir) = p.parent()
                    && parents.insert(dir)
                {
                    let _ = w.watch(dir, RecursiveMode::NonRecursive);
                }
            }
        }
        let mut previous = paths.iter().map(|(_, s)| *s).collect::<Vec<_>>();
        let mut fallback = tokio::time::interval(Duration::from_secs(30));
        fallback.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        loop {
            tokio::select! { _=fallback.tick()=>{}, _=receive.recv(),if watcher.is_some()=>{tokio::time::sleep(Duration::from_millis(25)).await;while receive.try_recv().is_ok(){}} }
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
}
