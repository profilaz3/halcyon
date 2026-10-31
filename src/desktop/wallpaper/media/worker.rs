//! Cancellable, bounded scene decoding. The short-lived child starts before
//! migrations/toolkit initialization and never writes preferences or cache files.
use super::{Pixels, Preview, SCENE};
use crate::desktop::wallpaper::{BgEntry, BgSource};
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::sync::watch;

const INPUT: usize = 16 * 1024;
static SLOTS: tokio::sync::Semaphore = tokio::sync::Semaphore::const_new(2);
const MAGIC: &[u8] = b"HCBACKDROP1\n";

/// Package validation/conversion shares the two image slots and exact-child
/// cancellation/deadline ownership. Its output is a bounded PNG or validation reply.
pub(crate) async fn package_image(
    input: Vec<u8>,
    mut cancel: watch::Receiver<bool>,
    started: impl FnOnce(u32) + Send + 'static,
) -> Option<Vec<u8>> {
    if input.len() > crate::desktop::profiles::packages::MAX_BYTES {
        return None;
    }
    let (owner, mut gone) = tokio::sync::oneshot::channel::<()>();
    let task = tokio::spawn(async move {
        if *cancel.borrow() || cancel.has_changed().is_err() {
            return None;
        }
        let _slot = tokio::select! {biased; _=&mut gone=>return None,_=cancel.changed()=>return None,slot=SLOTS.acquire_many(2)=>slot.ok()?};
        let mut command = tokio::process::Command::new(std::env::current_exe().ok()?);
        #[cfg(not(test))]
        command.arg("--package-image-worker");
        #[cfg(test)]
        command.args([
            "--exact",
            "desktop::profiles::packages::tests::worker_entry",
            "--ignored",
            "--nocapture",
        ]);
        let mut child = command
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::null())
            .kill_on_drop(true)
            .spawn()
            .ok()?;
        let pid = child.id()?;
        let mut stdin = child.stdin.take()?;
        let mut stdout = child.stdout.take()?;
        let run = async {
            stdin.write_all(&input).await.ok()?;
            drop(stdin);
            let mut prefix = Vec::new();
            loop {
                prefix.push(stdout.read_u8().await.ok()?);
                if prefix.ends_with(MAGIC) {
                    break;
                }
                if prefix.len() > 256 {
                    return None;
                }
            }
            started(pid);
            let size = stdout.read_u32_le().await.ok()? as usize;
            let max = crate::desktop::profiles::packages::MAX_IMAGE;
            if size > max {
                return None;
            }
            let mut bytes = Vec::new();
            stdout
                .take(max as u64 + 1)
                .read_to_end(&mut bytes)
                .await
                .ok()?;
            if bytes.len() != size {
                return None;
            }
            child.wait().await.ok()?.success().then_some(bytes)
        };
        let result = tokio::select! {biased; _=&mut gone=>None,_=cancel.changed()=>None,result=tokio::time::timeout(Duration::from_secs(4),run)=>result.ok().flatten()};
        if result.is_none() {
            let _ = child.kill().await;
            let _ = child.wait().await;
        }
        result
    });
    let result = task.await.ok().flatten();
    drop(owner);
    result
}

/// Trusted compiled-in specimens share admission with file decoding. The
/// blocking worker owns the permit even if its requesting future is dropped.
pub(crate) async fn prepare_static(work: impl FnOnce() + Send + 'static) {
    let Ok(slot) = SLOTS.acquire().await else {
        return;
    };
    let _ = tokio::task::spawn_blocking(move || {
        let _slot = slot;
        work();
    })
    .await;
}

#[derive(Clone, Copy, serde::Serialize, serde::Deserialize)]
enum Kind {
    Scene,
    Thumbnail,
    Wallpaper([u32; 2]),
}

pub(crate) async fn wallpaper(
    entry: BgEntry,
    size: [u32; 2],
    cancel: watch::Receiver<bool>,
) -> Option<Pixels> {
    if !wallpaper_size(size) {
        return None;
    }
    run_kind(
        entry,
        Kind::Wallpaper(size),
        cancel,
        Duration::from_secs(8),
        |_| {},
    )
    .await
}
fn wallpaper_size([w, h]: [u32; 2]) -> bool {
    w > 0 && h > 0 && w <= 8192 && h <= 8192 && u64::from(w) * u64::from(h) <= 3840 * 2160
}

pub(crate) async fn render(entry: BgEntry, cancel: watch::Receiver<bool>) -> Option<Pixels> {
    run(entry, cancel, Duration::from_secs(4), |_| {}).await
}

pub(crate) async fn thumbnail(
    path: std::path::PathBuf,
    cancel: watch::Receiver<bool>,
) -> Option<Pixels> {
    run_kind(
        BgEntry::new("all", BgSource::Path(path)),
        Kind::Thumbnail,
        cancel,
        Duration::from_secs(4),
        |_| {},
    )
    .await
}

async fn run(
    entry: BgEntry,
    cancel: watch::Receiver<bool>,
    deadline: Duration,
    started: impl FnOnce(u32) + Send + 'static,
) -> Option<Pixels> {
    run_kind(entry, Kind::Scene, cancel, deadline, started).await
}

async fn run_kind(
    entry: BgEntry,
    kind: Kind,
    cancel: watch::Receiver<bool>,
    deadline: Duration,
    started: impl FnOnce(u32) + Send + 'static,
) -> Option<Pixels> {
    let input = serde_json::to_vec(&(&entry, kind)).ok()?;
    if input.len() > INPUT {
        return None;
    }
    run_work(
        Work {
            input,
            kind,
            shared: false,
            depth: false,
            stalled: entry.output == "fixture-stall",
        },
        cancel,
        deadline,
        started,
    )
    .await
}

struct Work {
    input: Vec<u8>,
    kind: Kind,
    shared: bool,
    depth: bool,
    stalled: bool,
}

/// Public scene rendering shares decode admission and exact child ownership.
pub(crate) async fn render_share(input: Vec<u8>, cancel: watch::Receiver<bool>) -> Option<Pixels> {
    if input.len() > crate::desktop::profiles::MAX_PROFILE_BYTES + 8192 {
        return None;
    }
    run_work(
        Work {
            input,
            kind: Kind::Scene,
            shared: true,
            depth: false,
            stalled: false,
        },
        cancel,
        Duration::from_secs(4),
        |_| {},
    )
    .await
}

pub(crate) async fn render_depth(input: Vec<u8>, cancel: watch::Receiver<bool>) -> Option<Pixels> {
    if input.len() > crate::depth::MAX_INPUT {
        return None;
    }
    run_work(
        Work {
            input,
            kind: Kind::Scene,
            shared: false,
            depth: true,
            stalled: false,
        },
        cancel,
        Duration::from_secs(4),
        |_| {},
    )
    .await
}

async fn run_work(
    work: Work,
    cancel: watch::Receiver<bool>,
    deadline: Duration,
    started: impl FnOnce(u32) + Send + 'static,
) -> Option<Pixels> {
    // The owner survives a dropped caller until the exact child is reaped.
    let (owner, gone) = tokio::sync::oneshot::channel::<()>();
    let task = tokio::spawn(run_owned(work, cancel, gone, deadline, started));
    let result = task.await.ok().flatten();
    drop(owner);
    result
}

async fn run_owned(
    work: Work,
    mut cancel: watch::Receiver<bool>,
    mut gone: tokio::sync::oneshot::Receiver<()>,
    deadline: Duration,
    started: impl FnOnce(u32),
) -> Option<Pixels> {
    if *cancel.borrow() || cancel.has_changed().is_err() {
        return None;
    }
    let _slot = tokio::select! {
        biased;
        _ = &mut gone => return None,
        _ = cancel.changed() => return None,
        slot = SLOTS.acquire() => slot.ok()?,
    };
    if *cancel.borrow() {
        return None;
    }
    let Work {
        input,
        kind,
        shared,
        depth,
        stalled: _stalled,
    } = work;
    let mut command = tokio::process::Command::new(std::env::current_exe().ok()?);
    #[cfg(not(test))]
    command.arg(if depth {
        "--depth-worker"
    } else if shared {
        "--share-worker"
    } else {
        "--backdrop-worker"
    });
    #[cfg(test)]
    command.args([
        "--exact",
        if depth {
            "depth::tests::worker_entry"
        } else if shared {
            "settings::sharing::tests::worker_entry"
        } else {
            "desktop::wallpaper::media::worker::tests::worker_entry"
        },
        "--ignored",
        "--nocapture",
    ]);
    #[cfg(test)]
    if _stalled {
        command.env("HALCYON_BACKDROP_WORKER_STALL", "1");
    }
    if shared {
        command.env("TZ", "UTC");
    }
    let mut child = command
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null())
        .kill_on_drop(true)
        .spawn()
        .ok()?;
    let pid = child.id()?;
    let mut stdin = child.stdin.take()?;
    let mut stdout = child.stdout.take()?;
    let result = {
        let work = async {
            stdin.write_all(&input).await.ok()?;
            drop(stdin);
            // libtest writes a short preamble. Production begins with MAGIC.
            let mut prefix = Vec::new();
            loop {
                prefix.push(stdout.read_u8().await.ok()?);
                if prefix.ends_with(MAGIC) {
                    break;
                }
                if prefix.len() > 256 {
                    return None;
                }
            }
            started(pid);
            let width = stdout.read_u32_le().await.ok()?;
            let height = stdout.read_u32_le().await.ok()?;
            let bounds = if depth {
                (crate::depth::MAX_RASTER[0], crate::depth::MAX_RASTER[1])
            } else {
                match kind {
                    Kind::Scene => SCENE,
                    Kind::Thumbnail => super::THUMB,
                    Kind::Wallpaper(size) => (size[0], size[1]),
                }
            };
            if width == 0
                || height == 0
                || width > bounds.0
                || height > bounds.1
                || !depth
                    && matches!(kind, Kind::Scene | Kind::Wallpaper(_))
                    && (width, height) != bounds
            {
                return None;
            }
            let length = (width * height * 4) as usize;
            let mut rgba = Vec::new();
            stdout
                .take(if depth {
                    (crate::depth::MAX_RASTER[0] * crate::depth::MAX_RASTER[1] * 4) as u64 + 1
                } else {
                    u64::from(bounds.0) * u64::from(bounds.1) * 4 + 1
                })
                .read_to_end(&mut rgba)
                .await
                .ok()?;
            if rgba.len() != length {
                return None;
            }
            child.wait().await.ok()?.success().then_some(Pixels {
                width,
                height,
                rgba,
            })
        };
        tokio::select! {
            biased;
            _ = &mut gone => None,
            _ = cancel.changed() => None,
            result = tokio::time::timeout(deadline, work) => result.ok().flatten(),
        }
    };
    // Retain the worker slot until cancellation, invalid output or timeout is
    // acknowledged by this exact child. A late process never overlaps its slot.
    if result.is_none() {
        let _ = child.kill().await;
        let _ = child.wait().await;
    }
    result
}

pub(crate) fn pixels(entry: &BgEntry) -> Option<Pixels> {
    match &entry.source {
        BgSource::Color(color) => Some(super::render_color(color, SCENE.0, SCENE.1)),
        BgSource::Path(path) => {
            let picture = if path.is_dir() {
                super::scan_folder(path).ok()?.first.into_iter().next()?
            } else {
                path.clone()
            };
            let Preview::Ready(pixels) = super::scene_preview(&picture) else {
                return None;
            };
            Some(super::fit_preview(
                &pixels,
                &entry.scaling_mode,
                SCENE.0,
                SCENE.1,
            ))
        }
    }
}

pub(crate) fn limits() {
    let parent = unsafe { libc::getppid() };
    if parent == 1
        || unsafe { libc::prctl(libc::PR_SET_PDEATHSIG, libc::SIGKILL) } != 0
        || unsafe { libc::getppid() } != parent
    {
        std::process::exit(2);
    }
    for (resource, value) in [
        (libc::RLIMIT_AS, 512 * 1024 * 1024),
        (libc::RLIMIT_CPU, 2),
        (libc::RLIMIT_CORE, 0),
    ] {
        let limit = libc::rlimit {
            rlim_cur: value,
            rlim_max: value,
        };
        if unsafe { libc::setrlimit(resource, &limit) } != 0 {
            std::process::exit(2);
        }
    }
}

pub(crate) fn ready() -> Option<()> {
    use std::io::Write;
    let mut out = std::io::stdout().lock();
    out.write_all(MAGIC).ok()?;
    out.flush().ok()
}

pub(crate) fn write_pixels(pixels: &Pixels) -> Option<()> {
    use std::io::Write;
    let mut out = std::io::stdout().lock();
    out.write_all(&pixels.width.to_le_bytes()).ok()?;
    out.write_all(&pixels.height.to_le_bytes()).ok()?;
    out.write_all(&pixels.rgba).ok()?;
    out.flush().ok()
}

pub(crate) fn main() -> ! {
    use std::io::Read;
    limits();
    let result = (|| {
        let mut input = Vec::new();
        std::io::stdin()
            .take(INPUT as u64 + 1)
            .read_to_end(&mut input)
            .ok()?;
        if input.len() > INPUT {
            return None;
        }
        let (entry, kind): (BgEntry, Kind) = serde_json::from_slice(&input).ok()?;
        if crate::desktop::wallpaper::entry_problem(&entry).is_some() {
            return None;
        }
        ready()?;
        #[cfg(test)]
        if std::env::var_os("HALCYON_BACKDROP_WORKER_STALL").is_some() {
            loop {
                std::thread::sleep(Duration::from_secs(1));
            }
        }
        let pixels = match kind {
            Kind::Wallpaper(size) => {
                if !wallpaper_size(size) {
                    return None;
                }
                match &entry.source {
                    BgSource::Color(color) => super::render_color(color, size[0], size[1]),
                    BgSource::Path(path) => {
                        // Runtime resolves a slideshow to its actual displayed
                        // image. Never silently choose the first file instead.
                        let Preview::Ready(source) = super::read_preview(path, (8192, 8192)) else {
                            return None;
                        };
                        super::fit_preview(&source, &entry.scaling_mode, size[0], size[1])
                    }
                }
            }
            Kind::Scene => pixels(&entry)?,
            Kind::Thumbnail => {
                let BgSource::Path(path) = &entry.source else {
                    return None;
                };
                let Preview::Ready(pixels) = super::thumbnail(path) else {
                    return None;
                };
                pixels
            }
        };
        write_pixels(&pixels)
    })();
    std::process::exit(if result.is_some() { 0 } else { 2 });
}

#[cfg(test)]
mod tests;
