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
//! - `presets`: the built-in looks, as plain records, including the
//!   handcrafted ones shipped as `resources/looks/<id>.json`.
//! - `built_in`: turning a handcrafted look into such a file.
//!
//! Undo and recovery are the store's: `latest_change`, `undo_latest`,
//! `pending_recovery`, `recover` and `discard_recovery`. Nothing here runs
//! commands, palette or wallpaper helpers, or writes outside the store's
//! transaction and the library.
pub mod adaptive;
pub mod built_in;
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

pub use capture::{Capture, capture, icon_style, resolve_icon_theme, resolve_icon_theme_in};
pub use desktop_look::{DISPLAY_ARRANGEMENT, DISPLAY_BACKGROUNDS, OWN_WALLPAPER};
pub use format::{
    AppearanceLook, Applets, BackgroundLook, ColorChoice, Corners, DesktopIconsLook, DisplaysName,
    Domain, Exported, FitName, FrostedLook, Glass, GradientLook, IconShape, LEGACY_VERSION,
    LOCAL_ONLY, MAX_DESCRIPTION, MAX_PROFILE_BYTES, ModeName, OrderName, PanelLook, Profile,
    ProfileError, Rgb, SlideshowLook, SourceLook, SpacingChoice, TilingScope, TypingActionName,
    VERSION, VERSION_2, VERSION_20, VERSION_21, VariantLook, WallpaperLook, WindowLook,
    WindowsBehaviorLook, WorkspaceLayoutName, WorkspaceModeName, WorkspacesLook, export,
    export_local, import, import_local, valid_description,
};
pub use library::{ProfileEntry, ProfileId, ProfileLibrary};
pub use plan::{
    Asset, DomainPlan, Installed, ProfilePlan, apply, apply_as, plan, planned_wallpaper_source,
};
#[cfg(test)]
pub(crate) use presets::islands_composition;
pub use presets::{
    CLEAN, Preset, atmosphere, built_in_file, built_in_looks, colors_line, fitted,
    islands_available, islands_description, missing, occasion, parse_built_in, preset, presets,
    studio_presets,
};

#[cfg(test)]
mod desktop_tests;
#[cfg(test)]
mod tests;

pub(crate) use plan::PreviewBaseline;
pub use plan::PreviewConflict;
pub(crate) use plan::preview_panel_style;
