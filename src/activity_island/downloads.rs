//! Browser downloads, seen as their temporary files grow in the download
//! folder: Firefox writes `name.part`, Chrome and Chromium `name.crdownload`,
//! and each renames it to the final name when done. No browser reports a
//! total this way, so a download has a size and a rate but no percentage.
//!
//! The folder is watched with inotify. While a download is active its size
//! is read at most once a second; with none active nothing runs at all.
use super::transfers::{FileState, Update};
use std::collections::{HashMap, HashSet, VecDeque};
use std::os::unix::fs::MetadataExt;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

/// Sizes are read this often while a download is active, never faster.
const TICK: Duration = Duration::from_secs(1);
/// A download that has not grown for this long shows as paused.
const PAUSED: Duration = Duration::from_secs(5);
/// A download that has not grown for this long is dropped.
const STALLED: Duration = Duration::from_secs(30);
/// The rate comes from this much recent history.
const RATE_WINDOW: Duration = Duration::from_secs(5);
/// Downloads followed at once, at most.
const FILES: usize = 16;

/// Which browser writes a temporary name, and the name it will have.
fn temporary(name: &str) -> Option<(Browser, &str)> {
    if name.starts_with('.') {
        return None;
    }
    let (browser, stem) = if let Some(stem) = name.strip_suffix(".part") {
        (Browser::Firefox, stem)
    } else if let Some(stem) = name.strip_suffix(".crdownload") {
        (Browser::Chromium, stem)
    } else {
        return None;
    };
    (!stem.is_empty()).then_some((browser, stem))
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Browser {
    Firefox,
    Chromium,
}
impl Browser {
    fn name(self) -> &'static str {
        match self {
            Browser::Firefox => "Firefox",
            Browser::Chromium => "Chrome",
        }
    }
    /// The apps whose island rules govern its downloads.
    fn owners(self) -> Vec<(String, String)> {
        let pairs: &[(&str, &str)] = match self {
            Browser::Firefox => &[("Firefox", "firefox"), ("Firefox", "org.mozilla.firefox")],
            Browser::Chromium => &[
                ("Google Chrome", "google-chrome"),
                ("Google Chrome", "com.google.Chrome"),
                ("Chromium", "chromium"),
                ("Chromium", "org.chromium.Chromium"),
            ],
        };
        pairs
            .iter()
            .map(|(a, e)| ((*a).to_owned(), (*e).to_owned()))
            .collect()
    }
}

/// One download being written.
#[derive(Debug)]
struct File {
    path: PathBuf,
    /// The name it will have, as shown.
    name: String,
    browser: Browser,
    /// Sizes over the last few seconds.
    samples: VecDeque<(Instant, u64)>,
    bytes: u64,
    grew_at: Instant,
    /// It has grown since it was found, so it is worth showing.
    shown: bool,
    /// What was last reported, to report only changes.
    reported: Option<(u64, bool)>,
}
impl File {
    fn state(&self, now: Instant) -> FileState {
        let name = self.name.clone();
        let paused = now.saturating_duration_since(self.grew_at) >= PAUSED;
        FileState {
            name,
            bytes: self.bytes,
            rate: (!paused).then(|| rate(&self.samples)).flatten(),
            paused,
            path: self.path.clone(),
        }
    }
}
/// The name a temporary file will have. Firefox writes `name.Ab3_xY9z.ext.part`
/// beside an empty `name.ext` it holds for the result, so a sibling with one
/// part of the stem left out, when there is one, is the real name.
fn final_name(path: &Path, exists: impl Fn(&Path) -> bool) -> String {
    let Some((_, stem)) = path
        .file_name()
        .and_then(|n| n.to_str())
        .and_then(temporary)
    else {
        return String::new();
    };
    let parts: Vec<&str> = stem.split('.').collect();
    for skip in 1..parts.len() {
        let name: Vec<&str> = parts
            .iter()
            .enumerate()
            .filter(|(i, _)| *i != skip)
            .map(|(_, p)| *p)
            .collect();
        let name = name.join(".");
        if exists(&path.with_file_name(&name)) {
            return name;
        }
    }
    stem.to_owned()
}
fn rate(samples: &VecDeque<(Instant, u64)>) -> Option<f64> {
    let (t0, b0) = *samples.front()?;
    let (t1, b1) = *samples.back()?;
    let span = t1.saturating_duration_since(t0).as_secs_f64();
    (span >= 1. && b1 > b0).then(|| (b1 - b0) as f64 / span)
}

/// Every download in the folder, keyed by the file's inode so a rename
/// carries it along.
#[derive(Debug, Default)]
pub(super) struct Folder {
    files: HashMap<u64, File>,
}
impl Folder {
    fn key(inode: u64) -> String {
        format!("download:{inode}")
    }
    fn update(inode: u64, file: &File, now: Instant) -> Update {
        Update {
            key: Self::key(inode),
            app: file.browser.name().into(),
            visible: Some(true),
            file: Some(file.state(now)),
            owners: file.browser.owners(),
            ..Default::default()
        }
    }
    /// Something happened to `path`, which now has this inode and size if it
    /// still exists. Returns what the island should hear at once: a finished
    /// download. Growth is reported by `tick`.
    pub(super) fn touched(
        &mut self,
        path: &Path,
        found: Option<(u64, u64)>,
        now: Instant,
    ) -> Option<Update> {
        let name = path.file_name()?.to_str()?;
        let Some((inode, bytes)) = found else {
            // Gone: renamed away, deleted or cancelled. `tick` settles it.
            return None;
        };
        match temporary(name) {
            Some((browser, _)) => {
                let room = self.files.len() < FILES;
                match self.files.get_mut(&inode) {
                    // Chrome renames its file once it knows the name.
                    Some(file) => {
                        file.path = path.to_owned();
                        file.name = final_name(path, Path::exists);
                    }
                    None if room => {
                        self.files.insert(
                            inode,
                            File {
                                path: path.to_owned(),
                                name: final_name(path, Path::exists),
                                browser,
                                samples: VecDeque::from([(now, bytes)]),
                                bytes,
                                grew_at: now,
                                shown: false,
                                reported: None,
                            },
                        );
                    }
                    None => {}
                }
                None
            }
            None => {
                // Renamed to its final name: done.
                let file = self.files.remove(&inode)?;
                if !file.shown {
                    return None;
                }
                let mut update = Self::update(inode, &file, now);
                update.finished = true;
                if let Some(state) = &mut update.file {
                    state.name = name.to_owned();
                    state.path = path.to_owned();
                    state.bytes = bytes;
                    state.rate = None;
                    state.paused = false;
                }
                Some(update)
            }
        }
    }
    /// Reads each download's size. `size` gives a path's inode and size, or
    /// `None` once it is gone. Returns the changes worth reporting.
    pub(super) fn tick(
        &mut self,
        size: impl Fn(&Path) -> Option<(u64, u64)>,
        now: Instant,
    ) -> Vec<Update> {
        let mut updates = Vec::new();
        let mut gone = Vec::new();
        for (&inode, file) in &mut self.files {
            let found = size(&file.path).filter(|(i, _)| *i == inode);
            let Some((_, bytes)) = found else {
                gone.push(inode);
                continue;
            };
            if bytes != file.bytes {
                if bytes > file.bytes {
                    if !file.shown || now.saturating_duration_since(file.grew_at) >= PAUSED {
                        // The rate counts from the first byte, not from the
                        // empty file the browser made first, nor a pause.
                        file.samples.clear();
                    }
                    file.grew_at = now;
                    file.shown |= bytes > 0;
                } else {
                    // Started over: the old samples say nothing now.
                    file.samples.clear();
                }
                file.bytes = bytes;
            }
            file.samples.push_back((now, bytes));
            while file.samples.len() > 2
                && file
                    .samples
                    .front()
                    .is_some_and(|(t, _)| now.saturating_duration_since(*t) > RATE_WINDOW)
            {
                file.samples.pop_front();
            }
            if now.saturating_duration_since(file.grew_at) >= STALLED {
                gone.push(inode);
            }
        }
        for inode in gone {
            if let Some(file) = self.files.remove(&inode)
                && file.shown
            {
                updates.push(Update {
                    key: Self::key(inode),
                    app: file.browser.name().into(),
                    visible: Some(false),
                    ..Default::default()
                });
            }
        }
        for (&inode, file) in &mut self.files {
            if !file.shown {
                continue;
            }
            let paused = now.saturating_duration_since(file.grew_at) >= PAUSED;
            if file.reported == Some((file.bytes, paused)) {
                continue;
            }
            file.reported = Some((file.bytes, paused));
            updates.push(Self::update(inode, file, now));
        }
        updates
    }
    /// When to read sizes next: a second on while anything is active.
    pub(super) fn next_tick(&self, now: Instant) -> Option<Instant> {
        (!self.files.is_empty()).then(|| now + TICK)
    }
}

/// The download folder: `xdg-user-dir DOWNLOAD`, else `~/Downloads`. The
/// home folder itself, which the command answers when none is set, is not
/// watched.
pub(super) async fn download_dir() -> Option<PathBuf> {
    let home = dirs::home_dir()?;
    let asked = tokio::time::timeout(
        Duration::from_secs(2),
        tokio::process::Command::new("xdg-user-dir")
            .arg("DOWNLOAD")
            .kill_on_drop(true)
            .output(),
    )
    .await
    .ok()
    .and_then(Result::ok)
    .filter(|o| o.status.success())
    .and_then(|o| String::from_utf8(o.stdout).ok())
    .map(|s| PathBuf::from(s.trim_end_matches('\n')));
    asked
        .or_else(dirs::download_dir)
        .filter(|dir| dir.is_absolute() && *dir != home)
        .or_else(|| Some(home.join("Downloads")))
        .filter(|dir| dir.is_dir())
}
fn size(path: &Path) -> Option<(u64, u64)> {
    let meta = std::fs::symlink_metadata(path).ok()?;
    meta.is_file().then(|| (meta.ino(), meta.len()))
}

/// Downloads in the download folder, as transfer updates.
pub fn subscription() -> cosmic::iced::Subscription<Update> {
    if crate::tools::preview() {
        return cosmic::iced::Subscription::none();
    }
    cosmic::iced::Subscription::run_with("activity-downloads", |_| {
        cosmic::iced::stream::channel(16, async |out| {
            if let Some(dir) = download_dir().await {
                watch(&dir, out).await;
            }
            std::future::pending().await
        })
    })
}
/// Waits on inotify. Writes to a download already followed are not passed
/// on at all: the once-a-second read sees them.
pub(super) async fn watch(
    dir: &Path,
    mut out: cosmic::iced::futures::channel::mpsc::Sender<Update>,
) {
    use cosmic::iced::futures::SinkExt;
    use notify::{EventKind, RecursiveMode, Watcher, event::ModifyKind};
    let known: Arc<Mutex<HashSet<PathBuf>>> = Arc::default();
    let (send, mut paths) = tokio::sync::mpsc::channel::<PathBuf>(64);
    let seen = known.clone();
    let Ok(mut watcher) =
        notify::recommended_watcher(move |event: notify::Result<notify::Event>| {
            let Ok(event) = event else { return };
            // Renames, creations and removals are rare and all matter; a
            // write matters only for a temporary file not yet followed.
            let writes = matches!(
                event.kind,
                EventKind::Modify(ModifyKind::Data(_) | ModifyKind::Any | ModifyKind::Metadata(_))
                    | EventKind::Access(_)
            );
            for path in event.paths {
                let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
                    continue;
                };
                if name.starts_with('.') {
                    continue;
                }
                if writes
                    && (temporary(name).is_none() || seen.lock().is_ok_and(|k| k.contains(&path)))
                {
                    continue;
                }
                let _ = send.try_send(path);
            }
        })
    else {
        return;
    };
    if watcher.watch(dir, RecursiveMode::NonRecursive).is_err() {
        return;
    }
    let mut folder = Folder::default();
    let mut next: Option<Instant> = None;
    loop {
        let tick = async {
            match next {
                Some(at) => tokio::time::sleep_until(at.into()).await,
                None => std::future::pending().await,
            }
        };
        let mut finished = Vec::new();
        tokio::select! {
            path = paths.recv() => {
                let Some(path) = path else { return };
                let now = Instant::now();
                finished.extend(folder.touched(&path, size(&path), now));
                while let Ok(path) = paths.try_recv() {
                    finished.extend(folder.touched(&path, size(&path), now));
                }
            }
            () = tick => {
                let now = Instant::now();
                finished = folder.tick(size, now);
                next = None;
            }
        }
        let now = Instant::now();
        if next.is_none() {
            next = folder.next_tick(now);
        } else if folder.files.is_empty() {
            next = None;
        }
        if let Ok(mut k) = known.lock() {
            k.clear();
            k.extend(folder.files.values().map(|f| f.path.clone()));
        }
        for update in finished {
            if out.send(update).await.is_err() {
                return;
            }
        }
    }
}

/// `file://` URI for a path, for the file manager's ShowItems.
pub(super) fn file_uri(path: &Path) -> String {
    use std::os::unix::ffi::OsStrExt;
    let mut uri = String::from("file://");
    for &b in path.as_os_str().as_bytes() {
        if b.is_ascii_alphanumeric() || b"/-._~".contains(&b) {
            uri.push(b as char);
        } else {
            uri.push_str(&format!("%{b:02X}"));
        }
    }
    uri
}
/// Shows the file selected in the file manager, else opens its folder. Only
/// a refusal falls back: a slow file manager still gets the request, and a
/// second window from the fallback would be one too many.
pub async fn show_in_folder(path: PathBuf) -> Result<(), String> {
    let asked = async {
        let session = zbus::Connection::session().await?;
        session
            .call_method(
                Some("org.freedesktop.FileManager1"),
                "/org/freedesktop/FileManager1",
                Some("org.freedesktop.FileManager1"),
                "ShowItems",
                &(vec![file_uri(&path)], ""),
            )
            .await
            .map(|_| ())
    };
    match tokio::time::timeout(Duration::from_secs(10), asked).await {
        Ok(Err(_)) => {}
        // Shown, or still starting with the request in hand.
        Ok(Ok(())) | Err(_) => return Ok(()),
    }
    let folder = path.parent().ok_or("Could not open the folder")?;
    open_path(folder.to_owned()).await
}
/// Opens a file or folder with its default app.
pub async fn open_path(path: PathBuf) -> Result<(), String> {
    tokio::process::Command::new("xdg-open")
        .arg(&path)
        .spawn()
        .map(|_| ())
        .map_err(|_| "Could not open it".into())
}

/// Human sizes in decimal units, as file managers show them.
pub fn bytes_text(bytes: u64) -> String {
    let b = bytes as f64;
    match bytes {
        0..1_000 => format!("{bytes} B"),
        1_000..1_000_000 => format!("{:.0} kB", b / 1e3),
        1_000_000..10_000_000 => format!("{:.1} MB", b / 1e6),
        10_000_000..1_000_000_000 => format!("{:.0} MB", b / 1e6),
        _ => format!("{:.1} GB", b / 1e9),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn downloads_know_browser_temporary_names() {
        assert_eq!(
            temporary("report.pdf.part"),
            Some((Browser::Firefox, "report.pdf"))
        );
        assert_eq!(
            temporary("Unconfirmed 81234.crdownload"),
            Some((Browser::Chromium, "Unconfirmed 81234"))
        );
        for plain in ["report.pdf", ".part", ".x.crdownload", "part", "a.partial"] {
            assert_eq!(temporary(plain), None, "{plain}");
        }
    }
    #[test]
    fn downloads_name_firefox_files_by_their_placeholder() {
        let dir = Path::new("/d");
        let held =
            |names: &'static [&'static str]| move |p: &Path| names.iter().any(|n| p == dir.join(n));
        // Firefox 160: a random part before the extension, and the final
        // name held by an empty file.
        assert_eq!(
            final_name(
                &dir.join("sample-video.sn4_XdLJ.bin.part"),
                held(&["sample-video.bin"])
            ),
            "sample-video.bin"
        );
        assert_eq!(
            final_name(&dir.join("notes.Qw3_zZ1a.part"), held(&["notes"])),
            "notes"
        );
        // Older Firefox and Chrome: the stem is the name.
        assert_eq!(
            final_name(&dir.join("report.2026.pdf.part"), held(&[])),
            "report.2026.pdf"
        );
        assert_eq!(
            final_name(&dir.join("photo.jpg.crdownload"), held(&[])),
            "photo.jpg"
        );
    }
    #[test]
    fn downloads_size_and_uri_read_plainly() {
        assert_eq!(bytes_text(512), "512 B");
        assert_eq!(bytes_text(48_200), "48 kB");
        assert_eq!(bytes_text(3_450_000), "3.5 MB");
        assert_eq!(bytes_text(84_000_000), "84 MB");
        assert_eq!(bytes_text(2_100_000_000), "2.1 GB");
        assert_eq!(
            file_uri(Path::new("/home/a/Downloads/My file #1.zip")),
            "file:///home/a/Downloads/My%20file%20%231.zip"
        );
    }
    /// A Firefox download from start to rename, as the folder sees it.
    #[test]
    fn downloads_follow_growth_rate_pause_and_rename() {
        let t = Instant::now();
        let s = |secs: u64| t + Duration::from_secs(secs);
        let dir = Path::new("/d");
        let part = dir.join("big.iso.part");
        let mut folder = Folder::default();
        // Created empty: followed, not shown.
        assert!(folder.touched(&part, Some((7, 0)), s(0)).is_none());
        assert_eq!(folder.next_tick(s(0)), Some(s(0) + TICK));
        let sizes = std::cell::Cell::new(0u64);
        let at = |path: &Path| (path == part).then(|| (7, sizes.get()));
        assert!(folder.tick(at, s(1)).is_empty(), "nothing written yet");
        sizes.set(1_000_000);
        let first = folder.tick(at, s(2));
        assert_eq!(first.len(), 1);
        let state = first[0].file.clone().unwrap();
        assert_eq!(
            (state.name.as_str(), state.bytes, state.paused, state.rate),
            ("big.iso", 1_000_000, false, None)
        );
        assert_eq!(first[0].progress, None, "no percentage without a total");
        sizes.set(3_000_000);
        let second = folder.tick(at, s(3));
        let rate = second[0].file.as_ref().unwrap().rate.unwrap();
        assert!((rate - 2_000_000.).abs() < 1., "{rate}");
        // Unchanged and not yet paused: nothing new to say.
        assert!(folder.tick(at, s(4)).is_empty());
        // Paused after a while without growth.
        let paused = folder.tick(at, s(8));
        assert!(paused[0].file.as_ref().unwrap().paused);
        assert_eq!(paused[0].file.as_ref().unwrap().rate, None);
        // Growing again, then renamed to its final name: done.
        sizes.set(4_000_000);
        assert!(!folder.tick(at, s(9))[0].file.as_ref().unwrap().paused);
        let done = folder
            .touched(&dir.join("big.iso"), Some((7, 4_000_000)), s(10))
            .unwrap();
        assert!(done.finished);
        let state = done.file.unwrap();
        assert_eq!(
            (state.name.as_str(), state.path.as_path()),
            ("big.iso", dir.join("big.iso").as_path())
        );
        assert_eq!(folder.next_tick(s(10)), None, "nothing active: no timer");
    }
    #[test]
    fn downloads_follow_chrome_renames_and_drop_stalled_or_deleted_files() {
        let t = Instant::now();
        let s = |secs: u64| t + Duration::from_secs(secs);
        let dir = Path::new("/d");
        let unconfirmed = dir.join("Unconfirmed 1.crdownload");
        let named = dir.join("photo.jpg.crdownload");
        let mut folder = Folder::default();
        folder.touched(&unconfirmed, Some((3, 10)), s(0));
        // Chrome renames its temporary file once it knows the name.
        folder.touched(&named, Some((3, 10)), s(0));
        let grown = folder.tick(|p| (p == named).then_some((3, 5_000)), s(1));
        assert_eq!(grown[0].file.as_ref().unwrap().name, "photo.jpg");
        assert_eq!(grown[0].app, "Chrome");
        // Stops growing: paused, then dropped after the stall time.
        let stalled = |p: &Path| (p == named).then_some((3, 5_000));
        assert!(!folder.tick(stalled, s(10)).is_empty());
        let gone = folder.tick(stalled, s(1) + STALLED);
        assert_eq!(gone[0].visible, Some(false));
        assert!(folder.next_tick(s(40)).is_none());
        // Deleted while downloading: dropped at the next read.
        let part = dir.join("x.part");
        folder.touched(&part, Some((9, 0)), s(50));
        folder.tick(|_| Some((9, 100)), s(51));
        let cancelled = folder.tick(|_| None, s(52));
        assert_eq!(cancelled[0].visible, Some(false));
        // A file that never grew is dropped without a word.
        folder.touched(&part, Some((11, 0)), s(60));
        assert!(folder.tick(|_| None, s(61)).is_empty());
        // A final name that was never followed is not a download.
        assert!(
            folder
                .touched(&dir.join("notes.txt"), Some((12, 5)), s(62))
                .is_none()
        );
    }

    /// A real browser download, watched live: `cargo test --bin halcyon --
    /// --ignored downloads_watch_live --nocapture`, then download something
    /// within a minute. `HALCYON_WATCH_DIR` watches another folder and
    /// `HALCYON_WATCH_SECS` sets how long.
    #[test]
    #[ignore = "watches the real download folder for a minute"]
    fn downloads_watch_live() {
        let runtime = tokio::runtime::Runtime::new().unwrap();
        let secs: u64 = std::env::var("HALCYON_WATCH_SECS")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(60);
        runtime.block_on(async {
            let dir = match std::env::var_os("HALCYON_WATCH_DIR") {
                Some(dir) => PathBuf::from(dir),
                None => download_dir().await.expect("a download folder"),
            };
            println!("watching {}", dir.display());
            let (out, mut updates) = cosmic::iced::futures::channel::mpsc::channel(16);
            let watching = watch(&dir, out);
            let reading = async {
                use cosmic::iced::futures::StreamExt;
                let start = Instant::now();
                while let Some(u) = updates.next().await {
                    let f = u.file.clone();
                    println!(
                        "{:>5.1}s {} finished={} visible={:?} {:?}",
                        start.elapsed().as_secs_f64(),
                        u.key,
                        u.finished,
                        u.visible,
                        f.map(|f| (
                            f.name,
                            bytes_text(f.bytes),
                            f.rate.map(|r| bytes_text(r as u64)),
                            f.paused
                        ))
                    );
                }
            };
            let _ = tokio::time::timeout(Duration::from_secs(secs), async {
                tokio::join!(watching, reading)
            })
            .await;
        });
    }
}
