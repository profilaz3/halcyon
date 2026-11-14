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

    /// Whether anything follows the wallpaper files: rain, the depth clock
    /// or colors taken from the wallpaper or a style.
    pub(super) fn wallpaper_watch_wanted(&self) -> bool {
        self.config.rain.wanted()
            || self.config.clock.depth
            || self.config.appearance.wallpaper_colors
            || self.config.appearance.family.is_some()
    }

    /// Reads which wallpaper files to watch off the UI thread; the answer
    /// for the latest request replaces [`Cards::wallpaper_watch`].
    pub(super) fn refresh_wallpaper_watch(&mut self) -> Task<Message> {
        self.wallpaper_watch.0 = self.wallpaper_watch.0.wrapping_add(1);
        let generation = self.wallpaper_watch.0;
        if !self.wallpaper_watch_wanted() {
            self.wallpaper_watch.1.clear();
            return Task::none();
        }
        cosmic::task::future(async move {
            let paths = tokio::task::spawn_blocking(|| {
                DesktopStore::from_env()
                    .ok()
                    .filter(DesktopStore::writable)
                    .map(|store| automatic::watch_paths(&store))
                    .unwrap_or_default()
            })
            .await
            .unwrap_or_default();
            Message::WallpaperWatch(generation, paths)
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

#[cfg(test)]
mod tests {
    use super::*;

    /// Building subscriptions runs after every message: it uses the watch
    /// list found off the UI thread and opens no desktop store itself.
    #[test]
    fn subscriptions_use_the_stored_wallpaper_watch_without_reading_the_desktop() {
        if crate::displays::test_support::isolated(
            "app::wallpaper_colors::tests::subscriptions_use_the_stored_wallpaper_watch_without_reading_the_desktop",
        ) {
            return;
        }
        let mut config = Config::default();
        config.appearance.wallpaper_colors = true;
        config.clock.depth = true;
        let (mut cards, _) = Cards::init(Core::default(), config);
        assert!(cards.wallpaper_watch_wanted());
        let opened = || crate::desktop::store::FROM_ENV.with(|count| count.get());
        let before = opened();
        for _ in 0..3 {
            let _ = cosmic::Application::subscription(&cards);
        }
        assert_eq!(opened(), before, "subscription() opened the desktop store");

        // Only the latest read's answer is kept.
        let _ = cards.refresh_wallpaper_watch();
        let stale = cards.wallpaper_watch.0.wrapping_sub(1);
        let _ = cosmic::Application::update(
            &mut cards,
            Message::WallpaperWatch(stale, vec!["/stale".into()]),
        );
        assert!(cards.wallpaper_watch.1.is_empty());
        let current = cards.wallpaper_watch.0;
        let _ = cosmic::Application::update(
            &mut cards,
            Message::WallpaperWatch(current, vec!["/wallpaper".into()]),
        );
        assert_eq!(
            cards.wallpaper_watch.1,
            vec![std::path::PathBuf::from("/wallpaper")]
        );
        let _ = cosmic::Application::subscription(&cards);
        assert_eq!(opened(), before);

        // Turning everything off forgets the list.
        cards.config.appearance.wallpaper_colors = false;
        cards.config.clock.depth = false;
        cards.config.appearance.family = None;
        cards.config.rain = Default::default();
        let _ = cards.refresh_wallpaper_watch();
        assert!(cards.wallpaper_watch.1.is_empty());
    }
}
