//! The running Cards process maintains the wallpaper link, including while
//! Settings is closed. No second service or independent theme writer.
use super::*;
use crate::desktop::{DesktopStore, wallpaper::automatic};
use std::sync::atomic::Ordering;

impl Cards {
    pub(super) fn wallpaper_colors_changed(&mut self) -> Task<Message> {
        let id = self
            .wallpaper_color_generation
            .fetch_add(1, Ordering::Relaxed)
            .wrapping_add(1);
        if !self.config.appearance.wallpaper_colors && self.config.appearance.family.is_none() {
            return Task::none();
        }
        let generation = self.wallpaper_color_generation.clone();
        cosmic::task::future(async move {
            // Collapse cosmic-bg's entry/list/state write sequence before
            // decoding. Results carry a generation; older work cannot win.
            tokio::time::sleep(Duration::from_millis(200)).await;
            if generation.load(Ordering::Relaxed) != id {
                return Message::WallpaperColorsReady(id, Ok(None));
            }
            let result = tokio::task::spawn_blocking(|| {
                let store = DesktopStore::from_env().map_err(|error| error.to_string())?;
                automatic::prepare(&store)
            })
            .await
            .unwrap_or_else(|_| Err("Color analysis stopped unexpectedly".into()));
            Message::WallpaperColorsReady(id, result)
        })
    }

    pub(super) fn wallpaper_colors_ready(
        &mut self,
        id: u64,
        result: Result<Option<automatic::Prepared>, String>,
    ) -> Task<Message> {
        if id != self.wallpaper_color_generation.load(Ordering::Relaxed)
            || (!self.config.appearance.wallpaper_colors && self.config.appearance.family.is_none())
        {
            return Task::none();
        }
        let prepared = match result {
            Ok(Some(prepared)) => prepared,
            Ok(None) => return Task::none(),
            Err(error) => {
                eprintln!("Wallpaper colors: {error}");
                return Task::none();
            }
        };
        // Counted until applied: a restart into an update waits for it.
        self.applying_colors += 1;
        cosmic::task::future(async move {
            let result = tokio::task::spawn_blocking(move || {
                let store = DesktopStore::from_env().map_err(|error| error.to_string())?;
                automatic::commit(&store, prepared).map(|_| ())
            })
            .await
            .unwrap_or_else(|_| Err("Applying wallpaper colors stopped unexpectedly".into()));
            Message::WallpaperColorsApplied(result)
        })
    }
}
