//! Folder watches that wake only for changes. `notify`'s inotify backend
//! always asks for open events, so every read of any file in a watched
//! folder (COSMIC's apps read their settings all the time) woke its thread.
//! These watches ask the kernel only for entries being created, written,
//! renamed or removed, and for the folder itself going away.
use std::collections::HashMap;
use std::ffi::OsStr;
use std::io;
use std::os::fd::{AsRawFd, FromRawFd, OwnedFd};
use std::os::unix::ffi::OsStrExt;
use std::path::{Path, PathBuf};
use tokio::io::unix::AsyncFd;

/// Entry changes in a folder, and the folder itself going away. No
/// `IN_OPEN`, `IN_ACCESS` or `IN_CLOSE_NOWRITE`: reading is not a change.
/// `IN_CLOSE_WRITE` marks a finished write, so a long write wakes once
/// rather than once per `write` call as `IN_MODIFY` would.
const MASK: u32 = libc::IN_CREATE
    | libc::IN_DELETE
    | libc::IN_CLOSE_WRITE
    | libc::IN_MOVED_FROM
    | libc::IN_MOVED_TO
    | libc::IN_ATTRIB
    | libc::IN_DELETE_SELF
    | libc::IN_MOVE_SELF
    | libc::IN_ONLYDIR;

/// One inotify instance watching any number of folders, not recursively.
pub struct DirWatch {
    fd: AsyncFd<OwnedFd>,
    dirs: HashMap<i32, PathBuf>,
    /// Times the kernel had events for this watch: each is one wake of the
    /// task waiting on it.
    #[cfg(test)]
    pub wakes: usize,
}

impl DirWatch {
    pub fn new() -> io::Result<Self> {
        // SAFETY: no pointers; the result is checked.
        let fd = unsafe { libc::inotify_init1(libc::IN_NONBLOCK | libc::IN_CLOEXEC) };
        if fd < 0 {
            return Err(io::Error::last_os_error());
        }
        // SAFETY: `fd` was just opened and nothing else owns it.
        let fd = unsafe { OwnedFd::from_raw_fd(fd) };
        Ok(Self {
            fd: AsyncFd::new(fd)?,
            dirs: HashMap::new(),
            #[cfg(test)]
            wakes: 0,
        })
    }

    /// Watches `dir`'s entries. Watching a folder again is harmless.
    pub fn watch(&mut self, dir: &Path) -> io::Result<()> {
        let name = std::ffi::CString::new(dir.as_os_str().as_bytes())
            .map_err(|_| io::Error::from(io::ErrorKind::InvalidInput))?;
        // SAFETY: `name` is a valid C string for the duration of the call.
        let wd = unsafe { libc::inotify_add_watch(self.fd.as_raw_fd(), name.as_ptr(), MASK) };
        if wd < 0 {
            return Err(io::Error::last_os_error());
        }
        self.dirs.insert(wd, dir.to_path_buf());
        Ok(())
    }

    /// Waits until an entry for which `wanted` holds changes, or until a
    /// watched folder goes away or the kernel dropped events (both may hide
    /// a change, so they count). Cancel safe: events are read and judged
    /// within one poll, so dropping the future between wakes loses none.
    pub async fn changed(&mut self, wanted: impl Fn(&Path) -> bool) -> io::Result<()> {
        let mut buffer = [0u8; 4096];
        loop {
            let mut guard = self.fd.readable().await?;
            let read = guard.try_io(|fd| {
                // SAFETY: the buffer is valid for its length.
                let n =
                    unsafe { libc::read(fd.as_raw_fd(), buffer.as_mut_ptr().cast(), buffer.len()) };
                if n < 0 {
                    Err(io::Error::last_os_error())
                } else {
                    Ok(n as usize)
                }
            });
            let length = match read {
                Ok(result) => result?,
                Err(_would_block) => continue,
            };
            #[cfg(test)]
            {
                self.wakes += 1;
            }
            if self.judge(&buffer[..length], &wanted) {
                return Ok(());
            }
        }
    }

    fn judge(&mut self, mut events: &[u8], wanted: &impl Fn(&Path) -> bool) -> bool {
        const HEADER: usize = std::mem::size_of::<libc::inotify_event>();
        let mut relevant = false;
        while events.len() >= HEADER {
            // SAFETY: at least one header's bytes remain; read unaligned.
            let event: libc::inotify_event =
                unsafe { std::ptr::read_unaligned(events.as_ptr().cast()) };
            let end = (HEADER + event.len as usize).min(events.len());
            let name = &events[HEADER..end];
            let name = &name[..name.iter().position(|b| *b == 0).unwrap_or(name.len())];
            events = &events[end..];
            if event.mask & libc::IN_Q_OVERFLOW != 0 {
                relevant = true;
                continue;
            }
            if event.mask & libc::IN_IGNORED != 0 {
                // The folder went away or was unmounted; its watch is gone.
                self.dirs.remove(&event.wd);
                relevant = true;
                continue;
            }
            if event.mask & (libc::IN_DELETE_SELF | libc::IN_MOVE_SELF) != 0 {
                relevant = true;
                continue;
            }
            if let Some(dir) = self.dirs.get(&event.wd)
                && !name.is_empty()
                && wanted(&dir.join(OsStr::from_bytes(name)))
            {
                relevant = true;
            }
        }
        relevant
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    fn folder(name: &str) -> PathBuf {
        let root = std::env::temp_dir().join(format!(
            "halcyon-dir-watch-{name}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&root).unwrap();
        root
    }
    async fn wakes_for(watch: &mut DirWatch, key: &Path) -> bool {
        let key = key.to_path_buf();
        tokio::time::timeout(
            Duration::from_millis(200),
            watch.changed(move |path| path == key),
        )
        .await
        .is_ok()
    }

    /// Reading a file in a watched folder never wakes the watch: the kernel
    /// is not asked for opens. `notify` woke once per open.
    #[tokio::test]
    async fn reads_do_not_wake_and_writes_renames_and_removals_do() {
        let dir = folder("reads");
        let key = dir.join("key");
        std::fs::write(&key, "a").unwrap();
        let mut watch = DirWatch::new().unwrap();
        watch.watch(&dir).unwrap();
        for _ in 0..100 {
            std::fs::read(&key).unwrap();
            std::fs::read_dir(&dir).unwrap().for_each(drop);
        }
        assert!(!wakes_for(&mut watch, &key).await);
        assert_eq!(watch.wakes, 0, "100 reads woke the watch");

        std::fs::write(&key, "b").unwrap();
        assert!(wakes_for(&mut watch, &key).await, "write");
        std::fs::write(dir.join(".tmp"), "c").unwrap();
        std::fs::rename(dir.join(".tmp"), &key).unwrap();
        assert!(wakes_for(&mut watch, &key).await, "replace");
        std::fs::remove_file(&key).unwrap();
        assert!(wakes_for(&mut watch, &key).await, "remove");
        // Other entries wake the task but are not the key.
        std::fs::write(dir.join("other"), "d").unwrap();
        assert!(!wakes_for(&mut watch, &key).await);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[tokio::test]
    async fn a_removed_folder_counts_as_a_change() {
        let dir = folder("removed");
        let mut watch = DirWatch::new().unwrap();
        watch.watch(&dir).unwrap();
        std::fs::remove_dir(&dir).unwrap();
        assert!(wakes_for(&mut watch, &dir.join("key")).await);
    }

    /// For the record: how often each watch's thread or task wakes while
    /// another program reads 1,000 files in the watched folder. Run with
    /// `--ignored --nocapture`.
    #[tokio::test]
    #[ignore = "measurement"]
    async fn measure_wakes_for_reads() {
        use notify::Watcher;
        let dir = folder("measure");
        for i in 0..10 {
            std::fs::write(dir.join(format!("key{i}")), "x").unwrap();
        }
        let calls = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let counted = calls.clone();
        let mut old = notify::recommended_watcher(move |_: notify::Result<notify::Event>| {
            counted.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        })
        .unwrap();
        old.watch(&dir, notify::RecursiveMode::NonRecursive)
            .unwrap();
        let mut new = DirWatch::new().unwrap();
        new.watch(&dir).unwrap();
        for i in 0..1000 {
            std::fs::read(dir.join(format!("key{}", i % 10))).unwrap();
        }
        tokio::time::sleep(Duration::from_millis(300)).await;
        let _ = tokio::time::timeout(Duration::from_millis(100), new.changed(|_| true)).await;
        println!(
            "1000 reads: notify callbacks {}, DirWatch wakes {}",
            calls.load(std::sync::atomic::Ordering::Relaxed),
            new.wakes
        );
        drop(old);
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
