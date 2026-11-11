//! Native COSMIC desktop settings.
//!
//! `store` is the shared per-key adapter: it reads effective values from the
//! user and system COSMIC config trees, and applies small, journaled,
//! conflict-checked patches to whitelisted keys only. Domain models such as
//! `panels` build those patches; they never write files themselves.
pub mod actions;
pub mod appearance;
pub mod applets;
pub mod apps;
// Consumed by the Wallpaper / Desktop icons pages (stage 5, Task 2) and
// profiles (Task 3).
#[allow(dead_code)]
pub mod desktop_icons;
pub mod islands;
pub mod materials;
pub mod motion;
pub mod panels;
// Consumed by the Profiles page (stage 3, Task 2) and later scheduling.
#[allow(dead_code, unused_imports)]
pub mod profiles;
pub mod recovery;
pub mod shortcuts;
pub mod store;
#[allow(dead_code)]
pub mod wallpaper;
pub mod window_rules;
pub mod windows;
pub mod workspaces;

// The adapter surface later stages (Appearance, Profiles) build on.
#[allow(unused_imports)]
pub use store::{
    AppliedChange, ConfigKey, DesktopError, DesktopPatch, DesktopStore, KeyRule, LatestChange,
    Owner, RawValue, Snapshot, UserMessage, WidgetChange, WidgetPatch,
};

/// Every key the adapter may write. Later domains append their rule tables.
pub fn rules() -> impl Iterator<Item = &'static KeyRule> {
    panels::RULES
        .iter()
        .chain(islands::RULES.iter())
        .chain(materials::RULES.iter())
        .chain(appearance::rules())
        .chain(windows::RULES.iter())
        .chain(workspaces::RULES.iter())
        .chain(window_rules::RULES.iter())
        .chain(shortcuts::RULES.iter())
        .chain(wallpaper::RULES.iter())
        .chain(desktop_icons::RULES.iter())
}

/// Where a key's write falls within one change (lowest first; ties keep
/// key order). Only the wallpaper keys rank; every other key is 0.
pub fn write_rank(key: &ConfigKey) -> u8 {
    wallpaper::write_rank(key)
}
