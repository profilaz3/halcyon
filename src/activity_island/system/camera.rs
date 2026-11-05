//! Which cameras are open, and by what. Reads device links and process
//! names only; never opens a camera.
//!
//! Only cameras backed by hardware count. Loopback and other virtual video
//! devices are skipped: an app writing a virtual camera is not a lens being
//! on, and the app behind it reads the real camera, which is seen.
use super::*;
use std::path::{Path, PathBuf};

const SYSFS: &str = "/sys/class/video4linux";
/// While a camera is on, look again this often: closing a camera opened
/// read-only raises no file event.
const IN_USE_RECHECK: Duration = Duration::from_secs(5);
/// Otherwise only for cameras plugged in or out.
const IDLE_RECHECK: Duration = Duration::from_secs(30);

/// `/dev/videoN` for each hardware camera under `sys`.
fn cameras(sys: &Path) -> Vec<PathBuf> {
    let Ok(entries) = std::fs::read_dir(sys) else {
        return Vec::new();
    };
    let mut found: Vec<PathBuf> = entries
        .filter_map(Result::ok)
        .take(64)
        .filter_map(|entry| {
            let name = entry.file_name().into_string().ok()?;
            if !name.starts_with("video") {
                return None;
            }
            let device = std::fs::canonicalize(entry.path().join("device")).ok()?;
            if device.components().any(|c| c.as_os_str() == "virtual") {
                return None;
            }
            Some(Path::new("/dev").join(name))
        })
        .collect();
    found.sort();
    found
}
/// Names of the processes under `proc` holding one of `devices` open.
/// Only this user's processes can be read, which is what matters here.
fn holders(proc: &Path, devices: &[PathBuf]) -> Vec<String> {
    let mut names = Vec::new();
    let Ok(entries) = std::fs::read_dir(proc) else {
        return names;
    };
    for entry in entries.filter_map(Result::ok).take(8192) {
        if !entry
            .file_name()
            .to_str()
            .is_some_and(|n| n.bytes().all(|b| b.is_ascii_digit()))
        {
            continue;
        }
        let Ok(fds) = std::fs::read_dir(entry.path().join("fd")) else {
            continue;
        };
        let open = fds
            .filter_map(Result::ok)
            .take(4096)
            .any(|fd| std::fs::read_link(fd.path()).is_ok_and(|target| devices.contains(&target)));
        if !open {
            continue;
        }
        let name = std::fs::read_to_string(entry.path().join("comm"))
            .map(|n| clean(n.trim()))
            .unwrap_or_default();
        names.push(if name.is_empty() {
            "An app".into()
        } else {
            name
        });
    }
    names.sort();
    names.dedup();
    names.truncate(4);
    names
}
fn activity(names: &[String]) -> Option<Activity> {
    if names.is_empty() {
        return None;
    }
    let mut a = Activity::new(
        Source::Camera,
        "Camera in use",
        &names.join(", "),
        "Active".into(),
        None,
        false,
        None,
    );
    // A dot says it; the island does not jump open for it.
    a.announce = false;
    Some(a)
}
fn sample(devices: &[PathBuf]) -> Option<Activity> {
    if devices.is_empty() {
        return None;
    }
    activity(&holders(Path::new("/proc"), devices))
}
pub(super) async fn observe(tx: tokio::sync::mpsc::Sender<(Source, Option<Activity>)>) {
    use notify::Watcher;
    let (changed, mut events) = tokio::sync::mpsc::channel(1);
    // Opening or closing a watched camera node wakes the scan.
    let mut watcher = notify::recommended_watcher(move |_: notify::Result<notify::Event>| {
        let _ = changed.try_send(());
    })
    .ok();
    let mut watched: Vec<PathBuf> = Vec::new();
    let mut last: Option<Option<Activity>> = None;
    // Something happened to a camera node: look at who holds it.
    let mut look = true;
    loop {
        let devices = tokio::task::spawn_blocking(|| cameras(Path::new(SYSFS)))
            .await
            .unwrap_or_default();
        // Watched afresh each time: a node replugged under the same name is
        // a new file, and its old watch went with the old one.
        if let Some(w) = &mut watcher {
            for device in &watched {
                let _ = w.unwatch(device);
            }
            for device in &devices {
                let _ = w.watch(device, notify::RecursiveMode::NonRecursive);
            }
        }
        look |= devices != watched || last.as_ref().is_some_and(Option::is_some);
        watched = devices.clone();
        let state = if look {
            tokio::task::spawn_blocking(move || sample(&devices))
                .await
                .ok()
                .flatten()
        } else {
            last.clone().flatten()
        };
        look = false;
        let recheck = if state.is_some() {
            IN_USE_RECHECK
        } else {
            IDLE_RECHECK
        };
        if last.as_ref() != Some(&state) {
            if tx.send((Source::Camera, state.clone())).await.is_err() {
                return;
            }
            last = Some(state);
        }
        tokio::select! {
            _ = tx.closed() => return,
            Some(()) = events.recv() => {
                // An app opening a camera often opens it more than once.
                tokio::time::sleep(Duration::from_millis(250)).await;
                while events.try_recv().is_ok() {}
                look = true;
            }
            _ = tokio::time::sleep(recheck) => {}
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::symlink;
    struct Root(PathBuf);
    impl Drop for Root {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }
    fn root(name: &str) -> Root {
        let root = std::env::temp_dir().join(format!("camera-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        Root(root)
    }
    #[test]
    fn only_hardware_cameras_are_watched() {
        let r = root("sys");
        let devices = r.0.join("devices");
        for (node, parent) in [
            ("video0", "pci0000:00/usb1/1-4/1-4:1.0"),
            ("video10", "virtual/video4linux/video10"),
        ] {
            std::fs::create_dir_all(devices.join(parent)).unwrap();
            let class = r.0.join("class").join(node);
            std::fs::create_dir_all(&class).unwrap();
            symlink(devices.join(parent), class.join("device")).unwrap();
        }
        std::fs::create_dir_all(r.0.join("class/media0")).unwrap();
        assert_eq!(cameras(&r.0.join("class")), vec![PathBuf::from("/dev/video0")]);
        assert!(cameras(&r.0.join("missing")).is_empty());
    }
    #[test]
    fn holders_are_named_once_and_others_are_ignored() {
        let r = root("proc");
        for (pid, comm, target) in [
            ("101", "zoom\n", "/dev/video0"),
            ("102", "zoom\n", "/dev/video0"),
            ("103", "firefox\n", "/dev/video1"),
            ("104", "bash\n", "/dev/null"),
            ("self", "x\n", "/dev/video0"),
        ] {
            let dir = r.0.join(pid);
            std::fs::create_dir_all(dir.join("fd")).unwrap();
            std::fs::write(dir.join("comm"), comm).unwrap();
            symlink(target, dir.join("fd/3")).unwrap();
        }
        let devices = [PathBuf::from("/dev/video0"), PathBuf::from("/dev/video1")];
        assert_eq!(holders(&r.0, &devices), vec!["firefox", "zoom"]);
        let a = activity(&holders(&r.0, &devices[..1])).unwrap();
        assert_eq!((a.title.as_str(), a.detail.as_str()), ("Camera in use", "zoom"));
        assert!(!a.announce && a.expires_ms.is_none());
        assert!(activity(&[]).is_none());
        assert!(sample(&[]).is_none());
    }
}
