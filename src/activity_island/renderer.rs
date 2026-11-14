//! How the island draws. A capsule and a small popup need no GPU, and the
//! panel often gives its applets none: wgpu then falls back to llvmpipe,
//! which runs two threads per core and maps all of Mesa into a process
//! that sits in the panel all day. tiny-skia draws the same widgets on the
//! CPU, in place, with no threads of its own.
use std::ffi::OsStr;
use std::sync::atomic::{AtomicBool, Ordering};

const RENDERER: &str = "tiny-skia";
/// iced reads its renderer from here when the first surface is made.
const VARIABLE: &str = "ICED_BACKEND";
/// Whether the island set `VARIABLE` itself, so the apps it opens get
/// their own default rather than the island's.
static CHOSEN: AtomicBool = AtomicBool::new(false);

/// The renderer to ask for, unless the person already chose one.
fn wanted(current: Option<&OsStr>) -> Option<&'static str> {
    current.is_none_or(OsStr::is_empty).then_some(RENDERER)
}

/// Picks tiny-skia for this process. Call first thing in `main`, before
/// any thread starts.
pub fn choose() {
    if let Some(renderer) = wanted(std::env::var_os(VARIABLE).as_deref()) {
        // SAFETY: `main` calls this before the process starts any thread,
        // so nothing reads the environment concurrently.
        unsafe { std::env::set_var(VARIABLE, renderer) };
        CHOSEN.store(true, Ordering::Relaxed);
    }
}

/// A program the island starts for the person: an app, a link, Settings.
/// It never inherits the island's own renderer choice.
pub(crate) fn command(program: impl AsRef<OsStr>) -> tokio::process::Command {
    command_with(program, CHOSEN.load(Ordering::Relaxed))
}
fn command_with(program: impl AsRef<OsStr>, chosen: bool) -> tokio::process::Command {
    let mut command = tokio::process::Command::new(program);
    if chosen {
        command.env_remove(VARIABLE);
    }
    command
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_island_asks_for_tiny_skia_unless_a_renderer_was_chosen() {
        assert_eq!(wanted(None), Some("tiny-skia"));
        assert_eq!(wanted(Some(OsStr::new(""))), Some("tiny-skia"));
        assert_eq!(wanted(Some(OsStr::new("wgpu"))), None);
    }

    #[test]
    fn apps_the_island_opens_do_not_inherit_its_renderer() {
        let removed = |command: &tokio::process::Command| {
            command
                .as_std()
                .get_envs()
                .any(|(key, value)| key == VARIABLE && value.is_none())
        };
        assert!(removed(&command_with("xdg-open", true)));
        // A renderer the person set is theirs to pass on.
        assert!(!removed(&command_with("xdg-open", false)));
    }
}
