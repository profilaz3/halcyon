//! Finding a picture's foreground: a bounded child runs the scene model on
//! the picture, and the main process keeps the result in the cache.
//!
//! The child gets the picture and model paths, and the model's expected size
//! and hash. It has its own limits (memory, CPU time, lower priority) because
//! the model is far larger than the clock worker's 512 MB allows. It answers
//! MAGIC, a length-prefixed JSON [`Reply`], then on success the mask, edges
//! and picture PNGs, each length-prefixed. It never writes a file.
use super::{cache, infer, model, scene};
use crate::desktop::wallpaper::media::worker;
use serde::{Deserialize, Serialize};
use std::io::Read;
use std::path::{Path, PathBuf};
use std::time::Duration;
use tokio::sync::watch;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Job {
    pub picture: PathBuf,
    pub model: PathBuf,
    pub size: u64,
    pub sha256: String,
    pub threads: usize,
}
const MAX_JOB: u64 = 16 * 1024;
/// Threads for the model: enough to finish in well under a second.
pub const THREADS: usize = 4;
/// The child's memory and CPU-time limits, and the parent's deadline.
pub const MEMORY: u64 = 3 << 30;
pub const CPU_SECONDS: u64 = 40;
pub const DEADLINE: Duration = Duration::from_secs(60);
/// The three PNGs together, at most.
const MAX_REPLY: u64 = 96 << 20;

/// Why a picture could not be analysed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Failure {
    /// Not a picture this can read (only PNG and JPEG).
    Picture,
    /// The model file is missing or is not the expected file.
    Model,
    /// The model ran but gave nothing usable.
    Depth,
    /// The child stopped, ran out of time or was cancelled.
    Stopped,
    /// The result could not be kept.
    Disk,
}
impl Failure {
    pub fn sentence(self) -> &'static str {
        match self {
            Self::Picture => "This picture’s format can’t be analysed; use a PNG or JPEG",
            Self::Model => "The scene model needs downloading again",
            Self::Depth | Self::Stopped => "Couldn’t find the foreground in this picture",
            Self::Disk => "Couldn’t save the result on this computer",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub enum Reply {
    Done { key: String, stats: scene::Stats },
    Failed(Failure),
}

/// The scene child's own limits: room for the model, CPU for one run, no
/// core file, lower priority than the desktop, and gone with its parent.
fn limits() {
    let parent = unsafe { libc::getppid() };
    if parent == 1
        || unsafe { libc::prctl(libc::PR_SET_PDEATHSIG, libc::SIGKILL) } != 0
        || unsafe { libc::getppid() } != parent
    {
        std::process::exit(2);
    }
    for (resource, value) in [
        (libc::RLIMIT_AS, MEMORY),
        (libc::RLIMIT_CPU, CPU_SECONDS),
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
    unsafe {
        libc::setpriority(libc::PRIO_PROCESS, 0, 10);
    }
}

fn decode(bytes: &[u8]) -> Option<image::RgbImage> {
    use crate::desktop::wallpaper::media::{MAX_ALLOC, MAX_SIDE};
    let mut reader = image::ImageReader::new(std::io::Cursor::new(bytes))
        .with_guessed_format()
        .ok()?;
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(MAX_SIDE);
    limits.max_image_height = Some(MAX_SIDE);
    limits.max_alloc = Some(MAX_ALLOC);
    reader.limits(limits);
    Some(reader.decode().ok()?.to_rgb8())
}

fn encode(image: &image::DynamicImage) -> Option<Vec<u8>> {
    let mut out = Vec::new();
    image
        .write_to(&mut std::io::Cursor::new(&mut out), image::ImageFormat::Png)
        .ok()?;
    Some(out)
}

/// Everything the child does after reading its job; also what tests run.
pub fn run(job: &Job) -> (Reply, Vec<Vec<u8>>) {
    let failed = |f| (Reply::Failed(f), Vec::new());
    let Some(bytes) = super::read_picture(&job.picture) else {
        return failed(Failure::Picture);
    };
    let key = cache::key(&bytes);
    let Some(picture) = decode(&bytes) else {
        return failed(Failure::Picture);
    };
    drop(bytes);
    let source = model::Source {
        url: String::new(),
        size: job.size,
        sha256: job.sha256.clone(),
    };
    let Ok(weights) = model::read_verified(&job.model, &source) else {
        return failed(Failure::Model);
    };
    let depth = infer::run(&weights, &picture, job.threads.clamp(1, 8));
    drop(weights);
    let Ok(depth) = depth else {
        return failed(Failure::Depth);
    };
    let Some(found) = scene::derive(&depth, &picture) else {
        return failed(Failure::Depth);
    };
    let files = [
        encode(&image::DynamicImage::ImageLuma8(found.mask)),
        encode(&image::DynamicImage::ImageLuma8(found.edges)),
        encode(&image::DynamicImage::ImageRgb8(found.picture)),
    ];
    let Some(files) = files.into_iter().collect::<Option<Vec<_>>>() else {
        return failed(Failure::Depth);
    };
    (
        Reply::Done {
            key,
            stats: found.stats,
        },
        files,
    )
}

pub(crate) fn main() -> ! {
    use std::io::Write;
    limits();
    let result = (|| -> Option<()> {
        let mut input = Vec::new();
        std::io::stdin()
            .take(MAX_JOB + 1)
            .read_to_end(&mut input)
            .ok()?;
        if input.len() as u64 > MAX_JOB {
            return None;
        }
        let job: Job = serde_json::from_slice(&input).ok()?;
        worker::ready()?;
        #[cfg(test)]
        if std::env::var_os("HALCYON_BACKDROP_WORKER_STALL").is_some() {
            loop {
                std::thread::sleep(Duration::from_secs(1));
            }
        }
        let (reply, files) = run(&job);
        let mut out = std::io::stdout().lock();
        let reply = serde_json::to_vec(&reply).ok()?;
        out.write_all(&(reply.len() as u32).to_le_bytes()).ok()?;
        out.write_all(&reply).ok()?;
        for file in files {
            out.write_all(&(file.len() as u32).to_le_bytes()).ok()?;
            out.write_all(&file).ok()?;
        }
        out.flush().ok()
    })();
    std::process::exit(if result.is_some() { 0 } else { 2 });
}

/// Reads the child's answer into what the cache keeps.
pub(crate) fn parse(bytes: &[u8]) -> Result<(String, cache::Files), Failure> {
    let mut at = 0usize;
    let mut next = || -> Option<&[u8]> {
        let length = u32::from_le_bytes(bytes.get(at..at + 4)?.try_into().ok()?) as usize;
        let part = bytes.get(at + 4..at.checked_add(4 + length)?)?;
        at += 4 + length;
        Some(part)
    };
    let reply: Reply = next()
        .and_then(|b| serde_json::from_slice(b).ok())
        .ok_or(Failure::Stopped)?;
    let (key, stats) = match reply {
        Reply::Done { key, stats } => (key, stats),
        Reply::Failed(failure) => return Err(failure),
    };
    let mut files = [(); 3].map(|_| next().map(<[u8]>::to_vec));
    let [Some(mask), Some(edges), Some(picture)] =
        [files[0].take(), files[1].take(), files[2].take()]
    else {
        return Err(Failure::Stopped);
    };
    Ok((
        key,
        cache::Files {
            stats,
            mask,
            edges,
            picture,
        },
    ))
}

/// Analyses `picture` with the model at `model` and keeps the result under
/// `root`. Returns the picture's key.
pub async fn analyse(
    picture: &Path,
    model: &Path,
    source: &model::Source,
    root: PathBuf,
    cancel: watch::Receiver<bool>,
) -> Result<String, Failure> {
    let job = Job {
        picture: picture.into(),
        model: model.into(),
        size: source.size,
        sha256: source.sha256.clone(),
        threads: THREADS,
    };
    let reply = run_job(&job, cancel, false, |_| {})
        .await
        .ok_or(Failure::Stopped)?;
    let (key, files) = parse(&reply)?;
    let stored = key.clone();
    tokio::task::spawn_blocking(move || {
        cache::store(&root, &stored, &files)?;
        cache::load(&root, &stored)
            .map(|_| ())
            .ok_or_else(|| std::io::Error::other("unreadable entry"))
    })
    .await
    .map_err(|_| Failure::Disk)?
    .map_err(|_| Failure::Disk)?;
    Ok(key)
}

/// [`analyse`] for the picture whose content is `key`, unless it is already
/// kept. When another process is analysing it, waits for that instead.
pub async fn analyse_once(
    key: &str,
    picture: &Path,
    model: &Path,
    source: &model::Source,
    root: PathBuf,
    mut cancel: watch::Receiver<bool>,
) -> Result<String, Failure> {
    loop {
        if cache::exists(&root, key) {
            return Ok(key.into());
        }
        if let Some(claim) = cache::claim(&root, key) {
            let result = analyse(picture, model, source, root, cancel).await;
            drop(claim);
            return result;
        }
        if *cancel.borrow() {
            return Err(Failure::Stopped);
        }
        tokio::select! {
            _ = cancel.changed() => return Err(Failure::Stopped),
            () = tokio::time::sleep(Duration::from_millis(500)) => {}
        }
    }
}

pub(crate) async fn run_job(
    job: &Job,
    cancel: watch::Receiver<bool>,
    stall: bool,
    started: impl FnOnce(u32) + Send + 'static,
) -> Option<Vec<u8>> {
    worker::run_child(
        worker::Child {
            flag: "--depth-scene-worker",
            test: "depth::tests::scene_worker_entry",
            slots: 2,
            deadline: DEADLINE,
            max: MAX_REPLY,
            stall,
        },
        serde_json::to_vec(job).ok()?,
        cancel,
        started,
    )
    .await
}
