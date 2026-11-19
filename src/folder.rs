//! One non-recursive directory watcher shared by every view of the Folder widget.
use cosmic::iced::{Subscription, futures::SinkExt};
use cosmic::widget::image::Handle;
use notify::{RecommendedWatcher, RecursiveMode, Watcher};
use serde::{Deserialize, Serialize};
use std::{
    cmp::Reverse,
    collections::{BinaryHeap, HashMap},
    path::{Path, PathBuf},
    time::{Duration, SystemTime},
};

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Preferences {
    pub enabled: bool,
    pub path: Option<PathBuf>,
    pub show_hidden: bool,
}

#[derive(Clone, Debug)]
pub struct Entry {
    pub path: PathBuf,
    pub directory: bool,
    pub thumbnail: Option<Handle>,
    modified: SystemTime,
    bytes: u64,
}
#[derive(Clone, Debug, Default)]
pub struct Listing {
    pub source: PathBuf,
    pub entries: Vec<Entry>,
    pub count: usize,
    pub show_hidden: bool,
    pub problem: Option<String>,
}
pub fn sample() -> Listing {
    let source = PathBuf::from("/Sample/Studio");
    let entries = [
        "Sketches",
        "Field notes.pdf",
        "Cover study.png",
        "Project outline.md",
        "References",
        "Meeting notes.txt",
    ]
    .into_iter()
    .enumerate()
    .map(|(i, s)| Entry {
        path: source.join(s),
        directory: i == 0 || i == 4,
        thumbnail: None,
        modified: SystemTime::UNIX_EPOCH,
        bytes: 0,
    })
    .collect();
    Listing {
        source,
        entries,
        count: 6,
        show_hidden: false,
        problem: None,
    }
}
pub fn layout(size: [f32; 2]) -> (usize, usize) {
    let columns = if size[0] >= 420. {
        ((size[0] - 28.) / 124.).floor().clamp(2., 5.) as usize
    } else {
        1
    };
    let row_height = if columns == 1 { 36. } else { 86. };
    (
        columns,
        ((size[1] - 64.) / row_height).floor().clamp(0., 6.) as usize,
    )
}
fn scan(
    path: &Path,
    hidden: bool,
    limit: usize,
    cancelled: &std::sync::atomic::AtomicBool,
) -> Listing {
    let mut listing = Listing {
        source: path.to_owned(),
        show_hidden: hidden,
        ..Default::default()
    };
    let dir = match std::fs::read_dir(path) {
        Ok(dir) => dir,
        Err(e) => {
            listing.problem = Some(
                match e.kind() {
                    std::io::ErrorKind::PermissionDenied => {
                        "Permission denied. Choose another folder or allow access."
                    }
                    std::io::ErrorKind::NotFound => "This folder has moved or was removed.",
                    _ => "This folder is unavailable.",
                }
                .into(),
            );
            return listing;
        }
    };
    let mut recent = BinaryHeap::new();
    for item in dir.flatten() {
        if cancelled.load(std::sync::atomic::Ordering::Relaxed) {
            break;
        }
        if !hidden && item.file_name().as_encoded_bytes().starts_with(b".") {
            continue;
        }
        let Ok(meta) = std::fs::symlink_metadata(item.path()) else {
            continue;
        };
        if !meta.is_file() && !meta.is_dir() {
            continue;
        }
        listing.count += 1;
        recent.push(Reverse((
            meta.modified().unwrap_or(SystemTime::UNIX_EPOCH),
            item.path(),
            meta.is_dir(),
            meta.len(),
        )));
        if recent.len() > limit {
            recent.pop();
        }
    }
    listing.entries = recent
        .into_sorted_vec()
        .into_iter()
        .map(|Reverse((modified, path, directory, bytes))| Entry {
            path,
            directory,
            bytes,
            modified,
            thumbnail: None,
        })
        .collect();
    listing
}
type Cache = HashMap<(PathBuf, SystemTime, u64), Handle>;
fn thumbnails(listing: &mut Listing, cache: &mut Cache, cancelled: &std::sync::atomic::AtomicBool) {
    let mut visible = Cache::new();
    for e in &mut listing.entries {
        if cancelled.load(std::sync::atomic::Ordering::Relaxed) {
            break;
        }
        if e.directory || e.bytes > 8 * 1024 * 1024 {
            continue;
        }
        let ext = e
            .path
            .extension()
            .and_then(|s| s.to_str())
            .unwrap_or("")
            .to_ascii_lowercase();
        if !matches!(ext.as_str(), "png" | "jpg" | "jpeg" | "webp") {
            continue;
        }
        let key = (e.path.clone(), e.modified, e.bytes);
        let image = cache.remove(&key).or_else(|| {
            let mut reader = image::ImageReader::open(&e.path)
                .ok()?
                .with_guessed_format()
                .ok()?;
            let mut limits = image::Limits::default();
            limits.max_image_width = Some(4096);
            limits.max_image_height = Some(4096);
            limits.max_alloc = Some(32 * 1024 * 1024);
            reader.limits(limits);
            let rgba = reader.decode().ok()?.thumbnail(144, 80).to_rgba8();
            Some(Handle::from_rgba(
                rgba.width(),
                rgba.height(),
                rgba.into_raw(),
            ))
        });
        if let Some(h) = image {
            e.thumbnail = Some(h.clone());
            visible.insert(key, h);
        }
    }
    *cache = visible;
}

#[derive(Default)]
struct Lease(std::sync::Arc<std::sync::atomic::AtomicBool>);
impl Drop for Lease {
    fn drop(&mut self) {
        self.0.store(true, std::sync::atomic::Ordering::Relaxed);
    }
}

pub fn subscription(path: PathBuf, hidden: bool, size: [f32; 2]) -> Subscription<Listing> {
    let (cols, rows) = layout(size);
    Subscription::run_with((path, hidden, cols, rows), |(path, hidden, cols, rows)| {
        let path = path.clone();
        let hidden = *hidden;
        let wide = *cols > 1;
        let limit = cols * rows;
        cosmic::iced::stream::channel(1, async move |mut output| {
            let lease = Lease::default();
            let (tx, mut rx) = tokio::sync::mpsc::channel(1);
            let filter = path.clone();
            let watcher: Result<RecommendedWatcher, _> =
                notify::recommended_watcher(move |event: notify::Result<notify::Event>| {
                    let changed = event.map_or(true, |e| {
                        !matches!(e.kind, notify::EventKind::Access(_))
                            && (e.need_rescan()
                                || e.paths
                                    .iter()
                                    .any(|p| p == &filter || p.parent() == Some(filter.as_path())))
                    });
                    if changed {
                        let _ = tx.try_send(());
                    }
                });
            let mut watcher = watcher.ok();
            let parent_watched = path.parent().is_some_and(|p| {
                watcher
                    .as_mut()
                    .is_some_and(|w| w.watch(p, RecursiveMode::NonRecursive).is_ok())
            });
            let mut watched = false;
            let mut cache = Cache::new();
            loop {
                if !watched {
                    watched = watcher
                        .as_mut()
                        .is_some_and(|w| w.watch(&path, RecursiveMode::NonRecursive).is_ok());
                }
                let target = path.clone();
                let cancelled = lease.0.clone();
                let Ok(mut listing) =
                    tokio::task::spawn_blocking(move || scan(&target, hidden, limit, &cancelled))
                        .await
                else {
                    break;
                };
                if listing.problem.is_some() {
                    watched = false;
                }
                for entry in &mut listing.entries {
                    entry.thumbnail = cache
                        .get(&(entry.path.clone(), entry.modified, entry.bytes))
                        .cloned();
                }
                if output.send(listing.clone()).await.is_err() {
                    break;
                }
                if wide && !listing.entries.is_empty() {
                    let cancelled = lease.0.clone();
                    let Ok((updated, next)) = tokio::task::spawn_blocking(move || {
                        thumbnails(&mut listing, &mut cache, &cancelled);
                        (listing, cache)
                    })
                    .await
                    else {
                        break;
                    };
                    cache = next;
                    if updated.entries.iter().any(|e| e.thumbnail.is_some())
                        && output.send(updated).await.is_err()
                    {
                        break;
                    }
                }
                if watched || parent_watched {
                    if rx.recv().await.is_none() {
                        break;
                    }
                    tokio::time::sleep(Duration::from_millis(160)).await;
                    while rx.try_recv().is_ok() {}
                    // A directory may have been replaced at the same name.
                    if let Some(w) = &mut watcher {
                        let _ = w.unwatch(&path);
                    }
                    watched = false;
                } else {
                    tokio::time::sleep(Duration::from_secs(30)).await;
                }
            }
        })
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn folder_keeps_recent_visible_files_and_releases_removed_sources() {
        let root = std::env::temp_dir().join(format!("halcyon-folder-test-{}", std::process::id()));
        std::fs::create_dir_all(&root).unwrap();
        let large = std::env::var_os("HALCYON_CUSTOMIZATION_EVIDENCE").is_some();
        let n = if large { 10_000 } else { 60 };
        for i in 0..n {
            std::fs::write(root.join(format!("Document {i:05}.txt")), []).unwrap();
        }
        std::fs::write(root.join(".private"), []).unwrap();
        let now = std::time::Instant::now();
        let listing = scan(&root, false, 6, &false.into());
        eprintln!(
            "Folder scan: {} entries, {} ms, {} retained",
            listing.count,
            now.elapsed().as_millis(),
            listing.entries.len()
        );
        assert_eq!((listing.count, listing.entries.len()), (n, 6));
        assert!(
            listing
                .entries
                .windows(2)
                .all(|w| w[0].modified >= w[1].modified)
        );
        assert_eq!(scan(&root, true, 6, &false.into()).count, n + 1);
        std::fs::rename(root.join("Document 00000.txt"), root.join("Renamed.txt")).unwrap();
        assert_eq!(scan(&root, false, 6, &false.into()).count, n);
        std::fs::remove_dir_all(&root).unwrap();
        assert!(scan(&root, false, 6, &false.into()).problem.is_some());
        assert!(
            crate::desktop::store::widget::FIELDS
                .iter()
                .all(|v| v.pointer != "/folder/path")
        );
    }
    #[test]
    fn folder_native_compact_and_wide_fit_whole_entries() {
        if std::env::var_os("HALCYON_CUSTOMIZATION_EVIDENCE").is_none() {
            return;
        }
        let mut listing = sample();
        let mut config = crate::config::Config::default();
        config.folder.path = Some(listing.source.clone());
        let rgba = image::load_from_memory(include_bytes!("../assets/looks/evening-wallpaper.jpg"))
            .unwrap()
            .thumbnail(144, 80)
            .into_rgba8();
        listing.entries[2].thumbnail = Some(Handle::from_rgba(
            rgba.width(),
            rgba.height(),
            rgba.into_raw(),
        ));
        for (size, name) in [
            ([310., 214.], "folder-compact"),
            ([520., 265.], "folder-wide"),
        ] {
            let mut ui = crate::native_test::Native::new(cosmic::iced::Size::new(size[0], size[1]));
            let face = || crate::tools::cards::folder(&listing, &config, size, |_| Some(()), None);
            assert!(ui.scrollables(face()).is_empty());
            ui.capture(face(), &cosmic::Theme::dark(), name);
        }
    }
}
