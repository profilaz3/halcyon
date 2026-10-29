//! Desktop profiles: portable, versioned records of a desktop look.
//!
//! - `format`: the record, its validation and transport: `import` /
//!   `export` for shared profiles, `import_local` / `export_local` for the
//!   library.
//! - `desktop_look`: the version 3 `wallpaper` and `desktop_icons` sections.
//! - `capture`: the applied desktop as a record (reads only).
//! - `plan`: what applying selected domains would change (reads only), and
//!   `apply`, one recoverable owned transaction through `DesktopStore`.
//! - `library`: named local profiles under the store's state directory.
//! - `presets`: the four built-in starting looks, as plain records.
//!
//! Undo and recovery are the store's: `latest_change`, `undo_latest`,
//! `pending_recovery`, `recover` and `discard_recovery`. Nothing here runs
//! commands, palette or wallpaper helpers, or writes outside the store's
//! transaction and the library.
pub mod adaptive;
mod capture;
mod desktop_look;
pub mod discovery;
mod format;
mod library;
pub mod mix;
pub mod packages;
mod plan;
mod presets;
pub mod themes;

pub use capture::{Capture, capture};
pub use desktop_look::{DISPLAY_ARRANGEMENT, DISPLAY_BACKGROUNDS, OWN_WALLPAPER};
pub use format::{
    AppearanceLook, Applets, BackgroundLook, ColorChoice, Corners, DesktopIconsLook, DisplaysName,
    Domain, Exported, FitName, FrostedLook, Glass, GradientLook, LEGACY_VERSION, LOCAL_ONLY,
    MAX_PROFILE_BYTES, ModeName, OrderName, PanelLook, Profile, ProfileError, Rgb, SlideshowLook,
    SourceLook, SpacingChoice, TilingScope, TypingActionName, VERSION, VERSION_2, VariantLook,
    WallpaperLook, WindowLook, WindowsBehaviorLook, WorkspaceLayoutName, WorkspaceModeName,
    WorkspacesLook, export, export_local, import, import_local,
};
pub use library::{ProfileEntry, ProfileId, ProfileLibrary};
pub use plan::{
    Asset, DomainPlan, Installed, ProfilePlan, apply, apply_as, plan, planned_wallpaper_source,
};
#[cfg(test)]
pub(crate) use presets::islands_composition;
pub use presets::{
    CLEAN, Preset, fitted, islands_available, islands_description, missing, occasion, preset,
    presets, studio_presets,
};

#[cfg(test)]
mod desktop_tests;
#[cfg(test)]
mod tests;

pub(crate) use plan::preview_panel_style;
