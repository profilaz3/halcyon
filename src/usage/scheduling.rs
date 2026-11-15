//! Event-driven reporting: state-file changes or a real wall-clock deadline.
use super::{MAX_ATTEMPTS, PAUSED, STATUS_LIFETIME, State};
use chrono::Utc;
use std::io;
use std::os::fd::{AsRawFd, FromRawFd, OwnedFd};
use std::path::{Path, PathBuf};
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};
use std::time::Duration;
use tokio::sync::Notify;

// Only failed watches, unavailable timer support or a due operation unable to
// make progress use this backoff. Healthy idle workers have no periodic tick.
pub(super) const ERROR_BACKOFF: i64 = 60;

pub(super) fn deadline(state: &State, activity: bool, warmup: Duration, now: i64) -> Option<i64> {
    if let Some(pending) = &state.pending {
        let expires = pending.created_at.saturating_add(STATUS_LIFETIME);
        return Some(
            if now < pending.created_at || pending.attempts >= MAX_ATTEMPTS {
                now // let claim_status expire invalid/backwards-clock queues
            } else {
                pending.retry_after.max(now).min(expires)
            },
        );
    }
    if !activity || !state.enabled || PAUSED.load(Ordering::SeqCst) {
        return None;
    }
    let day = chrono::DateTime::from_timestamp(now, 0)?
        .format("%Y-%m-%d")
        .to_string();
    let next_day = if state.last_success.as_deref() == Some(day.as_str()) {
        now.div_euclid(86400)
            .saturating_add(1)
            .saturating_mul(86400)
    } else {
        now
    };
    let warmup_seconds = warmup
        .as_secs()
        .saturating_add(u64::from(warmup.subsec_nanos() > 0));
    Some(
        next_day
            .max(state.retry_after)
            .max(now.saturating_add(warmup_seconds.min(i64::MAX as u64) as i64)),
    )
}

pub(super) struct StateWatch {
    path: PathBuf,
    directory: Option<PathBuf>,
    watcher: Option<tokio::task::JoinHandle<()>>,
    failed: Arc<AtomicBool>,
    ready: Arc<Notify>,
}
impl StateWatch {
    pub(super) fn new(path: PathBuf, ready: Arc<Notify>) -> Self {
        Self {
            path,
            directory: None,
            watcher: None,
            failed: Arc::new(AtomicBool::new(false)),
            ready,
        }
    }
    /// Watch an existing ancestor if the state directory has not been created
    /// yet. On its creation/removal, rebind before reading the latest state.
    pub(super) fn refresh(&mut self) -> bool {
        let directory = self
            .path
            .ancestors()
            .skip(1)
            .find(|p| p.is_dir())
            .map(Path::to_path_buf);
        if self.watcher.is_some()
            && self.directory == directory
            && !self.failed.load(Ordering::Relaxed)
        {
            return true;
        }
        if let Some(task) = self.watcher.take() {
            task.abort();
        }
        self.directory = directory.clone();
        self.failed.store(false, Ordering::Relaxed);
        let path = self.path.clone();
        let ready = self.ready.clone();
        let failed = self.failed.clone();
        let Some(directory) = directory else {
            return false;
        };
        let Ok(mut watcher) = crate::watch::dir::DirWatch::new() else {
            return false;
        };
        if watcher.watch(&directory).is_err() {
            return false;
        }
        self.watcher = Some(tokio::spawn(async move {
            // Rebind after each relevant event, including invalidated watches.
            // Notify coalesces bursts until the worker reads the latest state.
            let _ = watcher.changed(|changed| path.starts_with(changed)).await;
            failed.store(true, Ordering::Relaxed);
            ready.notify_one();
        }));
        true
    }
}

impl Drop for StateWatch {
    fn drop(&mut self) {
        if let Some(task) = self.watcher.take() {
            task.abort();
        }
    }
}

/// Linux realtime timers become readable after suspend, and CANCEL_ON_SET
/// wakes us on a wall-clock step so the UTC day/retry deadline can be recomputed.
/// This never sets the system clock or wakes a suspended computer.
async fn wall_deadline(at: i64) -> io::Result<()> {
    let raw = unsafe {
        libc::timerfd_create(libc::CLOCK_REALTIME, libc::TFD_NONBLOCK | libc::TFD_CLOEXEC)
    };
    if raw < 0 {
        return Err(io::Error::last_os_error());
    }
    let fd = unsafe { OwnedFd::from_raw_fd(raw) };
    let spec = libc::itimerspec {
        it_interval: libc::timespec {
            tv_sec: 0,
            tv_nsec: 0,
        },
        // Zero would disarm the timer; a past positive deadline fires at once.
        it_value: libc::timespec {
            tv_sec: at.max(1) as libc::time_t,
            tv_nsec: 0,
        },
    };
    let result = unsafe {
        libc::timerfd_settime(
            fd.as_raw_fd(),
            libc::TFD_TIMER_ABSTIME | libc::TFD_TIMER_CANCEL_ON_SET,
            &spec,
            std::ptr::null_mut(),
        )
    };
    if result < 0 {
        return Err(io::Error::last_os_error());
    }
    let timer = tokio::io::unix::AsyncFd::new(fd)?;
    loop {
        let mut readable = timer.readable().await?;
        match readable.try_io(|timer| {
            let mut expirations = 0u64;
            let result = unsafe {
                libc::read(
                    timer.get_ref().as_raw_fd(),
                    (&mut expirations as *mut u64).cast(),
                    std::mem::size_of::<u64>(),
                )
            };
            if result >= 0 {
                return Ok(());
            }
            let error = io::Error::last_os_error();
            if error.raw_os_error() == Some(libc::ECANCELED) {
                Ok(())
            } else {
                Err(error)
            }
        }) {
            Ok(result) => return result,
            Err(_) => continue,
        }
    }
}

pub(super) async fn wait(ready: &Notify, at: Option<i64>) {
    tokio::select! {
        _ = ready.notified() => {},
        result = async {
            match at { Some(at) => wall_deadline(at).await, None => std::future::pending().await }
        } => {
            if result.is_err() {
                // Keep local choices responsive even on a broken/unsupported timer.
                tokio::select! {
                    _ = ready.notified() => {},
                    _ = tokio::time::sleep(Duration::from_secs(ERROR_BACKOFF as u64)) => {},
                }
            }
        },
    }
}

pub(super) fn next(path: &Path, activity: bool, warmup: Duration, watch_ok: bool) -> Option<i64> {
    let now = Utc::now().timestamp();
    let due = super::load(path)
        .ok()
        .and_then(|state| deadline(&state, activity, warmup, now));
    // If cycle couldn't acquire its lock/save state, avoid a past-deadline spin.
    let due = due.map(|at| {
        if at <= now {
            now.saturating_add(ERROR_BACKOFF)
        } else {
            at
        }
    });
    if watch_ok {
        due
    } else {
        Some(
            due.unwrap_or(i64::MAX)
                .min(now.saturating_add(ERROR_BACKOFF)),
        )
    }
}

#[cfg(test)]
#[path = "scheduling_tests.rs"]
mod tests;
