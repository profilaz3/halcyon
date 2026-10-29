//! What applying a profile's selected domains would change, and Apply.
//!
//! `plan` reads the current desktop and builds one checked patch through
//! the domain drafts (a fresh `PanelDraft` per panel, one `AppearanceDraft`
//! for every Appearance edit) plus one widget patch against the current raw
//! configuration. It writes nothing and never starts the add-on. `apply` commits the
//! plan as a single owned transaction of the store, which re-reads every
//! original under lock, so a plan that went stale conflicts instead of
//! overwriting newer changes.
//!
//! Wallpaper and Desktop icons go through fresh `WallpaperDraft` and
//! `DesktopIconsDraft` edits, so they share the pages' validators, member
//! preservation and write order. Each wallpaper picture or folder the plan
//! would newly show is resolved and checked now, and checked again by
//! `apply` before anything is written; one that isn't available blocks only
//! the Wallpaper part.
use super::desktop_look::{BackgroundLook, DesktopIconsLook, DisplaysName, file_label};
use super::format::{
    Domain, ModeName, PanelLook, Profile, SourceLook, WallpaperLook, WindowsBehaviorLook,
    WorkspacesLook, widget_members,
};
use crate::config;
use crate::desktop::appearance::catalog::{FontCatalog, IconCatalog};
use crate::desktop::appearance::{
    self, AppearanceDraft, Control, FontRole, SystemMode, ThemeVariant, Value as Edit, WindowEffect,
};
use crate::desktop::applets::AppletCatalog;
use crate::desktop::desktop_icons::{self, DesktopIconsDraft};
use crate::desktop::panels::{self, PanelDraft, PanelKind, Regions, Setting};
use crate::desktop::store::widget::{self, WidgetPatch};
use crate::desktop::store::{
    ConfigKey, DesktopError, DesktopPatch, DesktopStore, LatestChange, Owner, RawValue,
};
use crate::desktop::wallpaper::media::{self, SourceCheck};
use crate::desktop::wallpaper::{self, BgEntry, BgSource, Class, Displays, Target, WallpaperDraft};
use crate::desktop::windows::{self, Behavior, Choice, WindowsDraft};
use crate::desktop::workspaces::WorkspacesDraft;
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

/// Installed components that profile identifiers are resolved against.
/// An empty catalog means "could not check", not "nothing installed".
#[derive(Clone, Debug, Default)]
pub struct Installed {
    pub fonts: FontCatalog,
    pub icons: IconCatalog,
    pub applets: AppletCatalog,
    /// Where the bundled palette wallpapers are installed.
    pub bundled_dir: PathBuf,
    /// Trusted system `backgrounds` folders (COSMIC wallpapers).
    pub background_dirs: Vec<PathBuf>,
    /// Whether COSMIC Files, which draws desktop icons, is installed;
    /// `None` when not checked.
    pub files: Option<bool>,
}

impl Installed {
    pub fn new(fonts: FontCatalog, icons: IconCatalog, applets: AppletCatalog) -> Self {
        Self {
            fonts,
            icons,
            applets,
            ..Default::default()
        }
    }
    /// The store's fixture catalogs (`fonts`, `icons`, `applications`,
    /// `pictures`, `backgrounds` under its root) or the installed ones.
    /// Reads only.
    pub fn discover(store: &DesktopStore) -> Self {
        let root = store.fixture_root().map(std::path::Path::to_path_buf);
        let fonts = match root.as_ref().map(|root| root.join("fonts")) {
            Some(file) if file.is_file() => {
                FontCatalog::from_manifest(&std::fs::read_to_string(file).unwrap_or_default())
            }
            Some(_) => FontCatalog::default(),
            None => FontCatalog::installed(),
        };
        let icons = match &root {
            Some(root) => {
                IconCatalog::discover(&[root.join("icons")], &appearance::catalog::languages())
            }
            None => IconCatalog::installed(),
        };
        let applets = match root.as_ref().map(|root| root.join("applications")) {
            Some(dir) if dir.is_dir() => AppletCatalog::discover(&[dir]),
            Some(_) => AppletCatalog::default(),
            None => AppletCatalog::installed(),
        };
        let files = match &root {
            Some(_) => Some(true),
            None => Some(desktop_icons::files_installed(
                std::env::var_os("PATH").as_deref(),
            )),
        };
        Self {
            bundled_dir: wallpaper::bundled_dir(store),
            background_dirs: wallpaper::background_dirs(store),
            files,
            ..Self::new(fonts, icons, applets)
        }
    }
    fn fonts_known(&self) -> bool {
        !self.fonts.families(false).is_empty() || !self.fonts.families(true).is_empty()
    }
}

/// One selected domain: what would change, what to know, what prevents it.
#[derive(Clone, Debug, PartialEq)]
pub struct DomainPlan {
    pub domain: Domain,
    /// Short names of the settings that would change.
    pub changes: Vec<String>,
    /// Missing components and other things the preview must show.
    pub warnings: Vec<String>,
    /// Reasons this domain cannot be applied. Exclude it to apply the rest.
    pub blocked: Vec<String>,
    /// Good to know, not a problem (shown with the details).
    pub notes: Vec<String>,
}

impl DomainPlan {
    fn new(domain: Domain) -> Self {
        Self {
            domain,
            changes: Vec::new(),
            warnings: Vec::new(),
            blocked: Vec::new(),
            notes: Vec::new(),
        }
    }
}

/// Where a wallpaper picture or folder came from, for its recheck and the
/// reason shown when it isn't available.
#[derive(Clone, Debug, PartialEq)]
enum Origin {
    Bundled(&'static str),
    /// A trusted backgrounds folder and the relative name inside it.
    System(PathBuf, String),
    Personal,
}

/// A wallpaper picture or folder the plan would newly show. `apply` checks
/// it again, with the same resolved path and kind, before writing.
#[derive(Clone, Debug, PartialEq)]
pub struct Asset {
    pub path: PathBuf,
    origin: Origin,
    package_digest: Option<String>,
}

impl Asset {
    /// Whether it can still be shown: it exists (a folder with pictures) and
    /// a COSMIC wallpaper still resolves inside its trusted folder. Reads
    /// only; a file can still disappear afterwards.
    pub fn check(&self) -> Result<(), String> {
        if let Some(hash) = &self.package_digest {
            super::packages::verify_picture(&self.path, hash)?;
        }
        let name = file_label(&self.path);
        let missing = || match &self.origin {
            Origin::Bundled(palette) => format!(
                "The built-in {} wallpaper isn’t installed. Reinstall Halcyon to restore it.",
                palette_title(palette)
            ),
            Origin::System(_, relative) => {
                format!("The COSMIC wallpaper “{relative}” isn’t on this computer.")
            }
            Origin::Personal => format!("“{name}” was moved or deleted."),
        };
        match media::check_source(&BgSource::Path(self.path.clone())) {
            SourceCheck::Ready => {}
            SourceCheck::Missing => return Err(missing()),
            SourceCheck::Empty => return Err(format!("“{name}” has no pictures.")),
            SourceCheck::NotText => return Err(format!("The location of “{name}” can’t be used.")),
            SourceCheck::Unreadable(reason) => return Err(format!("“{name}”: {reason}.")),
        }
        if let Origin::System(dir, relative) = &self.origin
            && wallpaper::classify(&self.path, Path::new(""), std::slice::from_ref(dir))
                != Class::System(relative.clone())
        {
            return Err(format!(
                "“{relative}” is no longer one of this computer’s COSMIC wallpapers."
            ));
        }
        Ok(())
    }
}

/// How the islands look changes; what sits where doesn't. Per-monitor
/// arrangements are local and always stay. A record without its own
/// arrangement (an appearance-only style) keeps the current order and
/// positions, and, where islands are already on, whether they sit together
/// or apart.
fn keep_arrangement(
    mut islands: dock_islands_core::Settings,
    current: dock_islands_core::Settings,
) -> dock_islands_core::Settings {
    if islands.default_layout == dock_islands_core::Arrangement::default() {
        islands.default_layout = current.default_layout;
        if current.enabled && islands.enabled {
            islands.placement = current.placement;
        }
    }
    islands.outputs = current.outputs;
    islands
}

fn palette_title(name: &str) -> &'static str {
    crate::desktop::wallpaper::BUNDLED
        .iter()
        .find(|(known, _)| *known == name)
        .map_or("", |(_, title)| *title)
}

/// The read-only result of `plan`.
#[derive(Clone, Debug, PartialEq)]
pub struct ProfilePlan {
    appearance_only: bool,
    pub name: String,
    /// Selected domains in `Domain::ALL` order.
    pub domains: Vec<DomainPlan>,
    /// Reasons that concern the whole change (preview mode, overlaps).
    pub blocking: Vec<String>,
    patch: DesktopPatch,
    widget: WidgetPatch,
    /// Wallpaper pictures and folders checked again before Apply.
    assets: Vec<Asset>,
    /// The planned shared background, or the effective display background
    /// for an appearance-only theme.
    background: Option<BgEntry>,
}

impl ProfilePlan {
    pub(crate) fn guard_widgets(&mut self, guards: WidgetPatch) -> Result<(), String> {
        self.widget.merge(guards).map_err(|fields| {
            format!(
                "The schedule would change its own location inputs: {}",
                fields.join(", ")
            )
        })
    }
    pub(crate) fn from_display_rule(&mut self, display: bool) {
        self.patch.activity_display = display;
    }
    /// The setup wizard's explicit choice may enable or disable Islands.
    /// Profiles and automatic switches preserve the existing enabled state.
    pub(crate) fn choose_islands(
        &mut self,
        store: &DesktopStore,
        kind: PanelKind,
        enabled: bool,
    ) -> Result<(), DesktopError> {
        if !crate::desktop::islands::Installation::discover(store).installed() {
            return Err(DesktopError::Io("Islands is no longer installed".into()));
        }
        let key = crate::desktop::islands::key(kind);
        let original = if let Some(entry) = self.patch.get(&key) {
            entry.original.clone()
        } else if let Some((_, original)) = self.patch.guards().find(|(guard, _)| **guard == key) {
            original.clone()
        } else {
            store.read_user(&key)?
        };
        let raw = self
            .patch
            .get(&key)
            .and_then(|entry| entry.desired.as_ref())
            .or(original.as_ref());
        let mut settings = raw
            .map(RawValue::decode::<dock_islands_core::Settings>)
            .transpose()
            .map_err(|e| DesktopError::Io(e.to_string()))?
            .unwrap_or_default();
        settings.enabled = enabled;
        self.patch
            .set(key, original, Some(RawValue::encode(&settings)?));
        Ok(())
    }
    /// Whether nothing would be written.
    pub fn is_empty(&self) -> bool {
        self.patch.is_empty() && self.widget.is_empty()
    }
    /// Every reason Apply is refused, domain reasons first.
    pub fn blocking_reasons(&self) -> Vec<String> {
        self.domains
            .iter()
            .flat_map(|domain| {
                domain
                    .blocked
                    .iter()
                    .map(move |reason| format!("{}: {reason}", domain.domain.title()))
            })
            .chain(self.blocking.iter().cloned())
            .collect()
    }
    pub fn can_apply(&self) -> bool {
        !self.is_empty() && self.blocking_reasons().is_empty()
    }
    pub fn patch(&self) -> &DesktopPatch {
        &self.patch
    }
    pub fn widget_patch(&self) -> &WidgetPatch {
        &self.widget
    }
    /// The wallpaper pictures and folders the plan would newly show.
    pub fn assets(&self) -> &[Asset] {
        &self.assets
    }
    /// Checks the prepared wallpaper assets again (reads only): one reason
    /// per asset that can no longer be shown.
    pub fn recheck(&self) -> Vec<String> {
        self.assets
            .iter()
            .filter_map(|asset| asset.check().err())
            .map(|reason| format!("{}: {reason}", Domain::Wallpaper.title()))
            .collect()
    }
    /// The shared background as the plan leaves it, when the Wallpaper part
    /// is included and can be applied (the preview's backdrop).
    pub fn background(&self) -> Option<&BgEntry> {
        self.background.as_ref()
    }
    /// The Undo label this change is recorded with.
    pub fn label(&self) -> String {
        let label = format!("Profile “{}”", self.name);
        label.chars().take(120).collect()
    }
}

/// Plans applying exactly the `selected` domains of `profile` to the
/// current desktop. Reads only; installation discovery never starts the add-on.
pub fn plan(
    store: &DesktopStore,
    installed: &Installed,
    profile: &Profile,
    selected: &BTreeSet<Domain>,
) -> ProfilePlan {
    let mut selected = selected.clone();
    if profile.appearance_only
        && super::themes::selects_style(profile)
        && profile
            .widget_style
            .as_ref()
            .and_then(|v| v.pointer("/appearance/sync_colors"))
            .and_then(serde_json::Value::as_bool)
            == Some(false)
    {
        selected.retain(|domain| matches!(domain, Domain::WidgetStyle | Domain::Wallpaper));
    }
    let selected = &selected;
    let mut result = ProfilePlan {
        appearance_only: profile.appearance_only,
        name: profile.name.clone(),
        domains: Vec::new(),
        blocking: Vec::new(),
        patch: DesktopPatch::new(),
        widget: WidgetPatch::new(),
        assets: Vec::new(),
        background: None,
    };
    if let Err(error) = profile.validate() {
        result.blocking.push(error.to_string());
        return result;
    }
    if selected.is_empty() {
        result
            .blocking
            .push("Choose at least one part of the profile.".into());
    }
    if !store.writable() {
        result
            .blocking
            .push(store.apply(&DesktopPatch::new()).err().map_or_else(
                || "Desktop changes are disabled".into(),
                |error| error.to_string(),
            ));
    }
    let mut config_value = None;
    let mut unavailable_bars = BTreeSet::new();
    for domain in Domain::ALL
        .into_iter()
        .filter(|domain| selected.contains(domain))
    {
        let mut part = DomainPlan::new(domain);
        if !profile.has(domain) {
            part.blocked
                .push(format!("This profile has no {} settings.", domain.title()));
            result.domains.push(part);
            continue;
        }
        match domain {
            Domain::Dock | Domain::Panel => {
                let (kind, look) = match domain {
                    Domain::Dock => (PanelKind::Dock, profile.dock.as_ref()),
                    _ => (PanelKind::Panel, profile.panel.as_ref()),
                };
                if profile.appearance_only
                    && let Ok(draft) = PanelDraft::load(store, kind)
                    && !draft.installed()
                {
                    // Absence is part of the reviewed native scope. If this
                    // bar appears before Apply, generation must be reviewed again.
                    let snapshot = draft.into_snapshot();
                    for key in snapshot.keys() {
                        result.patch.guard(key.clone(), snapshot.user(key).cloned());
                    }
                    unavailable_bars.insert(domain);
                    part.warnings.push(format!(
                        "{} is unavailable; its appearance is kept for another desktop",
                        kind.title()
                    ));
                    result.domains.push(part);
                    continue;
                }
                if let Some(patch) =
                    look.and_then(|look| plan_panel(store, installed, kind, look, &mut part))
                {
                    merge(&mut result, &mut part, patch);
                }
            }
            Domain::Appearance => {
                let relink = super::themes::resets_windows(profile);
                if let Some(patch) = profile
                    .appearance
                    .as_ref()
                    .map(|look| {
                        if profile.appearance_only {
                            // Read compatibility preserves old effect fields;
                            // color application must write neither On nor Off.
                            super::themes::visual_appearance(look)
                        } else {
                            look.clone()
                        }
                    })
                    .as_ref()
                    .and_then(|look| plan_appearance(store, installed, look, relink, &mut part))
                {
                    merge(&mut result, &mut part, patch);
                }
            }
            Domain::Windows => {
                if let Some(patch) = profile
                    .windows
                    .as_ref()
                    .and_then(|look| plan_windows(store, look, &mut part))
                {
                    merge(&mut result, &mut part, patch);
                }
            }
            Domain::Workspaces => {
                if let Some(patch) = profile
                    .workspaces
                    .as_ref()
                    .and_then(|look| plan_workspaces(store, look, &mut part))
                {
                    merge(&mut result, &mut part, patch);
                }
            }
            Domain::Wallpaper => {
                if let Some(prepared) = profile
                    .wallpaper
                    .as_ref()
                    .and_then(|look| plan_wallpaper(store, installed, look, &mut part))
                {
                    merge(&mut result, &mut part, prepared.patch);
                    result.assets = prepared.assets;
                    if let Some(hash) = profile.package.as_ref().and_then(|p| p.picture.as_ref())
                        && let Some(SourceLook::Image(path)) = profile
                            .wallpaper
                            .as_ref()
                            .and_then(|w| w.background.as_ref())
                            .and_then(|b| b.source.as_ref())
                    {
                        let asset = Asset {
                            path: path.clone(),
                            origin: Origin::Personal,
                            package_digest: Some(hash.clone()),
                        };
                        if let Err(reason) = asset.check() {
                            part.blocked.push(reason);
                        }
                        result.assets.push(asset);
                    }
                    if part.blocked.is_empty() {
                        result.background = if profile.appearance_only {
                            prepared.source.background
                        } else {
                            prepared.background
                        };
                    }
                }
            }
            Domain::DesktopIcons => {
                if let Some(patch) = profile
                    .desktop_icons
                    .as_ref()
                    .and_then(|look| plan_desktop_icons(store, installed, look, &mut part))
                {
                    merge(&mut result, &mut part, patch);
                }
            }
            Domain::WidgetStyle | Domain::WidgetLayout => {
                let group = domain.widget_group().expect("widget domain");
                let section = profile.widget_section(group).expect("checked above");
                match load_config(store, &mut config_value) {
                    Ok(current) => match plan_widgets(current, section, group, &mut part) {
                        Some(patch) => {
                            if let Err(overlaps) = result.widget.merge(patch) {
                                part.blocked.push(format!(
                                    "Overlaps another part of this change: {}",
                                    overlaps.join(", ")
                                ));
                            }
                        }
                        None => {}
                    },
                    Err(reason) => part.blocked.push(reason),
                }
            }
        }
        result.domains.push(part);
    }
    // A profile's explicit colors take priority over the wallpaper link.
    // A captured/requested automatic mode can deliberately retain the link;
    // a wallpaper-only profile never changes it.
    let style = selected
        .contains(&Domain::WidgetStyle)
        .then_some(profile.widget_style.as_ref())
        .flatten();
    let explicit_auto = style
        .and_then(|value| value.pointer("/appearance/wallpaper_colors"))
        .and_then(serde_json::Value::as_bool);
    let style_colors = style.is_some_and(|value| {
        ["/theme", "/appearance/accent", "/appearance/color_source"]
            .iter()
            .any(|pointer| value.pointer(pointer).is_some())
    });
    let theme_colors = selected.contains(&Domain::Appearance)
        && profile.appearance.as_ref().is_some_and(|look| {
            [&look.light, &look.dark]
                .iter()
                .filter_map(|variant| variant.as_ref())
                .any(|variant| {
                    variant.accent.is_some()
                        || variant.background.is_some()
                        || variant.primary.is_some()
                        || variant.secondary.is_some()
                })
        });
    if !profile.appearance_only
        && store.app_config().is_some()
        && explicit_auto.is_none()
        && (style_colors || theme_colors)
    {
        match load_config(store, &mut config_value) {
            Ok((current, effective)) => {
                if effective
                    .pointer("/appearance/wallpaper_colors")
                    .and_then(serde_json::Value::as_bool)
                    == Some(true)
                {
                    if let Err(reason) = result.widget.set(
                        "/appearance/color_policy",
                        widget::get(current, "/appearance/color_policy"),
                        Some(serde_json::json!("custom")),
                    ) {
                        result.blocking.push(reason);
                    }
                    let pointer = "/appearance/wallpaper_colors";
                    if let Err(reason) = result.widget.set(
                        pointer,
                        widget::get(current, pointer),
                        Some(serde_json::json!(false)),
                    ) {
                        result.blocking.push(reason);
                    }
                    if let Some(part) = result.domains.iter_mut().find(|part| {
                        part.domain
                            == if style_colors {
                                Domain::WidgetStyle
                            } else {
                                Domain::Appearance
                            }
                    }) {
                        part.changes.push("Use this profile’s fixed colors".into());
                    }
                }
            }
            Err(reason) => result.blocking.push(reason),
        }
    }
    if profile.appearance_only
        && selected.contains(&Domain::WidgetStyle)
        && super::themes::family(profile).is_some()
    {
        match WallpaperDraft::load(store) {
            Ok(draft) => {
                for key in draft.watch_keys() {
                    result
                        .patch
                        .guard(key.clone(), draft.snapshot().user(&key).cloned());
                }
            }
            Err(error) => result.blocking.push(error.to_string()),
        }
    }
    if selected.contains(&Domain::WidgetStyle)
        && let Some(Ok(current)) = &config_value
    {
        if let Err(reason) =
            native_palette_provenance(profile, selected, &unavailable_bars, current, &mut result)
        {
            result.blocking.push(reason);
        }
    }
    // The combined widget result must pass the same validation as every
    // configuration write.
    if let Some((current, _)) = config_value.as_ref().and_then(|value| value.as_ref().ok())
        && !result.widget.is_empty()
    {
        let mut value = current.clone();
        let outcome = result
            .widget
            .changes()
            .try_for_each(|(pointer, entry)| {
                widget::put(&mut value, pointer, entry.desired.clone())
            })
            .and_then(|()| config::check_config_value(&value).map(drop));
        if outcome.is_ok()
            && crate::desktop::islands::Installation::discover(store).installed()
            && result
                .widget
                .changes()
                .any(|(p, _)| p.starts_with("/materials/"))
        {
            let material_result = (|| -> Result<(), String> {
                let next = config::check_config_value(&value)?;
                for (target, name) in [
                    (crate::materials::Target::Dock, "Dock"),
                    (crate::materials::Target::Panel, "Panel"),
                ] {
                    let key = crate::desktop::materials::key(name);
                    let original = store.read_user(&key).map_err(|e| e.to_string())?;
                    if let Some(raw) = &original {
                        raw.decode::<dock_islands_core::material::BarMaterial>()?
                            .validate()?;
                    }
                    let desired = next
                        .materials
                        .effective(target)
                        .map(|finish| {
                            RawValue::encode(&dock_islands_core::material::BarMaterial {
                                version: 1,
                                finish,
                            })
                        })
                        .transpose()
                        .map_err(|e| e.to_string())?;
                    result.patch.set(key, original, desired);
                }
                Ok(())
            })();
            if let Err(reason) = material_result {
                result.blocking.push(reason);
            }
        }
        if let Err(reason) = outcome {
            result
                .blocking
                .push(format!("The widget settings would not be valid: {reason}"));
        }
    }
    result
}

fn merge(result: &mut ProfilePlan, part: &mut DomainPlan, patch: DesktopPatch) {
    if let Err(DesktopError::Overlap(keys)) = result.patch.merge(patch) {
        part.blocked.push(format!(
            "Overlaps another part of this change: {}",
            keys.iter()
                .map(ToString::to_string)
                .collect::<Vec<_>>()
                .join(", ")
        ));
    }
}

/// The raw widget configuration and the same with Config's defaults.
type Loaded = (serde_json::Value, serde_json::Value);

fn load_config<'a>(
    store: &DesktopStore,
    cache: &'a mut Option<Result<Loaded, String>>,
) -> Result<&'a Loaded, String> {
    if cache.is_none() {
        let loaded = match store.app_config() {
            Some(path) => config::read_config_text(path)
                .and_then(|text| config::parse_config_text(text.as_deref()))
                .and_then(|raw| {
                    let typed = config::check_config_value(&raw)?;
                    let effective =
                        serde_json::to_value(&typed).map_err(|error| error.to_string())?;
                    Ok((raw, effective))
                })
                .map_err(|error| format!("Widget settings are unreadable: {error}")),
            None => Err("Widget settings aren’t available here.".into()),
        };
        *cache = Some(loaded);
    }
    cache
        .as_ref()
        .expect("filled above")
        .as_ref()
        .map_err(Clone::clone)
}

fn plan_panel(
    store: &DesktopStore,
    installed: &Installed,
    kind: PanelKind,
    look: &PanelLook,
    part: &mut DomainPlan,
) -> Option<DesktopPatch> {
    let mut draft = match PanelDraft::load(store, kind) {
        Ok(draft) => draft,
        Err(error) => {
            part.blocked.push(error.to_string());
            return None;
        }
    };
    if !draft.installed() {
        part.blocked.push(format!(
            "The {} isn’t set up on this computer.",
            kind.title()
        ));
        return None;
    }
    if !draft.listed() {
        part.warnings.push(format!(
            "The {} is turned off; these settings take effect when it’s turned on.",
            kind.title()
        ));
    }
    // Vertical Islands remains unqualified. A vertical setup never silently
    // turns off selected Islands intent; the user makes that choice explicitly.
    let vertical = look.anchor.is_some_and(|anchor| !anchor.horizontal());
    if vertical
        && draft
            .islands()
            .known()
            .is_none_or(|islands| islands.enabled)
    {
        part.blocked.push(format!(
            "{} uses a standard vertical bar. Turn off Islands for this bar, then check again.",
            kind.title()
        ));
        return None;
    }
    let installed_islands = crate::desktop::islands::Installation::discover(store).installed();
    if let Some(gap) = look.island_gap.filter(|_| installed_islands) {
        if let Some(mut islands) = draft.islands().known().cloned() {
            islands.gap = gap;
            draft.set_islands(islands);
        }
    }
    if installed_islands {
        if let Some(islands) = &look.islands {
            if let Some(current) = draft.islands().known().cloned() {
                // Applying a look does not activate an installed but disabled add-on.
                let mut desired = keep_arrangement(islands.clone(), current.clone());
                desired.enabled = current.enabled;
                draft.set_islands(desired);
            } else {
                part.blocked
                    .push("Unsupported island settings are kept".into());
            }
        }
    }
    let values = match look.values() {
        Ok(values) => values,
        Err(error) => {
            part.blocked.push(error);
            return None;
        }
    };
    for (name, value) in values {
        // A current value this version cannot decode is never replaced
        // without the user seeing it: the domain is blocked instead.
        if let Err(reason) = set_typed(&mut draft, name, &value) {
            part.blocked.push(format!(
                "{} {} {reason}",
                kind.title(),
                panels::key_label(name)
            ));
        }
    }
    if let Some(applets) = &look.applets {
        match draft.applets() {
            Setting::Known(_) => {
                let known = |id: &String| {
                    installed.applets.sorted().is_empty() || installed.applets.known(id)
                };
                if installed.applets.sorted().is_empty() {
                    part.warnings
                        .push("Installed applets could not be checked.".into());
                }
                let missing: Vec<&String> = applets
                    .start
                    .iter()
                    .chain(&applets.center)
                    .chain(&applets.end)
                    .filter(|id| !known(id))
                    .collect();
                if !missing.is_empty() {
                    part.warnings.push(format!(
                        "Not installed, left out of the {}: {}",
                        kind.title(),
                        missing
                            .iter()
                            .map(|id| id.as_str())
                            .collect::<Vec<_>>()
                            .join(", ")
                    ));
                }
                let keep = |list: &[String]| list.iter().filter(|id| known(id)).cloned().collect();
                draft.set_applets(&Regions {
                    start: keep(&applets.start),
                    center: keep(&applets.center),
                    end: keep(&applets.end),
                });
            }
            Setting::Unsupported(_) => part.blocked.push(format!(
                "{} applets have a value this version doesn’t recognize.",
                kind.title()
            )),
        }
    }
    let mut patch = draft.patch();
    if vertical {
        let snapshot = draft.clone().into_snapshot();
        let key = crate::desktop::islands::key(kind);
        patch.guard(key.clone(), snapshot.user(&key).cloned());
    }
    if look.background.is_some() {
        let key = ConfigKey::new(kind.namespace(), 1, "background").expect("panel key");
        let snapshot = draft.into_snapshot();
        patch.guard(key.clone(), snapshot.user(&key).cloned());
    }
    for key in patch.keys() {
        let label = format!("{} {}", kind.title(), panels::key_label(key.key()));
        if !part.changes.contains(&label) {
            part.changes.push(label);
        }
    }
    Some(patch)
}

/// Read-only catalogue projection using the planner's typed setters and
/// placement preservation. Curated setups have no applet edits. This helper
/// never discovers an installation, reads a file, or applies a transaction.
pub(crate) fn preview_panel_style(
    mut draft: PanelDraft,
    look: &super::format::PanelLook,
    installed_islands: bool,
) -> PanelDraft {
    if installed_islands && let Some(current) = draft.islands().known().cloned() {
        let mut desired = look
            .islands
            .clone()
            .map(|requested| keep_arrangement(requested, current.clone()))
            .unwrap_or_else(|| current.clone());
        // A recipe describes an arrangement; enablement is an explicit user choice.
        desired.enabled = current.enabled;
        if look.islands.is_none()
            && let Some(gap) = look.island_gap
        {
            desired.gap = gap;
        }
        draft.set_islands(desired);
    }
    if let Ok(values) = look.values() {
        for (name, value) in values {
            let _ = set_typed(&mut draft, name, &value);
        }
    }
    draft
}

/// Sets a panel key through its typed setter when the current value decodes.
fn set_typed(draft: &mut PanelDraft, name: &str, value: &RawValue) -> Result<(), String> {
    use crate::desktop::panels::{Anchor, AutoHide, AutoHideBehavior, Background, PanelSize};
    macro_rules! typed {
        ($ty:ty) => {{
            let new: $ty = value.decode()?;
            if draft.value::<$ty>(name).known().is_none() {
                return Err("has a value this version doesn’t recognize.".into());
            }
            draft.set_value(name, &new);
            Ok(())
        }};
    }
    match name {
        "anchor" => typed!(Anchor),
        "anchor_gap" | "expand_to_edges" | "exclusive_zone" | "keep_style_on_maximize" => {
            typed!(bool)
        }
        "margin" => typed!(u16),
        "size" => typed!(PanelSize),
        "size_wings" => typed!(Option<(Option<PanelSize>, Option<PanelSize>)>),
        "size_center" => typed!(Option<PanelSize>),
        "autohide" => typed!(AutoHide),
        "autohide_behavior" => typed!(AutoHideBehavior),
        "opacity" => typed!(f32),
        "border_radius" | "spacing" | "padding" => typed!(u32),
        "background" => typed!(Background),
        _ => Err("is not supported in profiles.".into()),
    }
}

/// With `relink`, an outline the look makes follow the accent also loses a
/// color only COSMIC's derived theme holds (the Windows Reset).
fn plan_appearance(
    store: &DesktopStore,
    installed: &Installed,
    look: &super::format::AppearanceLook,
    relink: bool,
    part: &mut DomainPlan,
) -> Option<DesktopPatch> {
    let mut draft = match AppearanceDraft::load(store) {
        Ok(draft) => draft,
        Err(error) => {
            part.blocked.push(error.to_string());
            return None;
        }
    };
    if let Some(mode) = look.mode {
        draft.set(
            Control::Mode,
            Edit::Mode(match mode {
                ModeName::Light => SystemMode::Light,
                ModeName::Dark => SystemMode::Dark,
                ModeName::Automatic => SystemMode::Automatic,
            }),
        );
    }
    if let Some(density) = look.interface_density {
        draft.set(Control::InterfaceDensity, Edit::Density(density));
    }
    if let Some(density) = look.header_size {
        draft.set(Control::HeaderSize, Edit::Density(density));
    }
    for (role, font) in [
        (FontRole::Interface, &look.interface_font),
        (FontRole::Monospace, &look.monospace_font),
    ] {
        let Some(font) = font else {
            continue;
        };
        if !installed.fonts_known() {
            part.warnings.push(format!(
                "Installed fonts could not be checked ({}).",
                font.family
            ));
            draft.set(Control::Font(role), Edit::Font(font.clone()));
        } else if installed
            .fonts
            .family(role.monospace(), &font.family)
            .is_none()
        {
            part.warnings.push(format!(
                "{} “{}” isn’t installed; your current font is kept.",
                role.title(),
                font.family
            ));
        } else {
            draft.set(Control::Font(role), Edit::Font(font.clone()));
        }
    }
    if let Some(name) = &look.icon_theme {
        if installed.icons.themes.is_empty() {
            part.warnings.push(format!(
                "Installed icon themes could not be checked ({name})."
            ));
            draft.set(Control::IconTheme, Edit::Name(name.clone()));
        } else if installed.icons.get(name).is_none() {
            part.warnings.push(format!(
                "Icon theme “{name}” isn’t installed; your current icons are kept."
            ));
        } else {
            draft.set(Control::IconTheme, Edit::Name(name.clone()));
        }
    }
    if let Some(window) = &look.window {
        for (effect, on) in [
            (WindowEffect::ClipFloating, window.clip_floating),
            (WindowEffect::ClipTiled, window.clip_tiled),
            (WindowEffect::ShadowTiled, window.shadow_tiled),
        ] {
            if let Some(on) = on {
                draft.set(Control::Window(effect), Edit::Flag(on));
            }
        }
    }
    for variant in [ThemeVariant::Light, ThemeVariant::Dark] {
        if let Some(variant_look) = look.variant(variant) {
            match variant_look.values(variant) {
                Ok(values) => draft.set(Control::Variant(variant), Edit::Variant(Box::new(values))),
                Err(reason) => part.blocked.push(reason),
            }
            if relink && variant_look.outline == Some(super::format::ColorChoice::Auto) {
                draft.set(
                    Control::Color(variant, appearance::ColorRole::Outline),
                    Edit::Color(None),
                );
            }
        }
    }
    let mut plan = draft.plan();
    // A reviewed generation can accept already-equal roles. They still need
    // guards before its provenance is committed; a no-op draft has no edits.
    for variant in ThemeVariant::ALL {
        if let Some(look) = look.variant(variant) {
            for (name, color) in [
                ("bg_color", &look.background),
                ("primary_container_bg", &look.primary),
            ] {
                if color.is_some() {
                    let key = appearance::theme::builder_key(variant, name);
                    plan.patch
                        .guard(key.clone(), draft.snapshot().user(&key).cloned());
                }
            }
        }
    }
    for blocked in &plan.blocked {
        part.blocked.push(format!(
            "{} has a value this version doesn’t recognize ({}).",
            capitalize(&appearance::key_label(&blocked.key)),
            blocked.reason
        ));
    }
    for key in plan.patch.keys() {
        let label = capitalize(&appearance::key_label(key));
        if !part.changes.contains(&label) {
            part.changes.push(label);
        }
    }
    Some(plan.patch)
}

/// Window behavior through a fresh `WindowsDraft`: the same typed setters,
/// unsupported-value blocking and tiling transition rule as the page.
fn plan_windows(
    store: &DesktopStore,
    look: &WindowsBehaviorLook,
    part: &mut DomainPlan,
) -> Option<DesktopPatch> {
    let mut draft = match WindowsDraft::load(store) {
        Ok(draft) => draft,
        Err(error) => {
            part.blocked.push(error.to_string());
            return None;
        }
    };
    let values = match look.values() {
        Ok(values) => values,
        Err(error) => {
            part.blocked.push(error);
            return None;
        }
    };
    for (behavior, raw) in values {
        let choice = match behavior {
            Behavior::Scope => raw.decode().map(Choice::Scope),
            Behavior::FocusDelay => raw.decode().map(Choice::Millis),
            Behavior::EdgeSnap => raw.decode().map(Choice::Pixels),
            _ => raw.decode().map(Choice::Flag),
        };
        match choice {
            Ok(choice) => draft.set(behavior, choice),
            Err(reason) => part.blocked.push(reason),
        }
    }
    let plan = draft.plan();
    for blocked in &plan.blocked {
        if windows::is_transition(blocked) {
            // The Windows page offers the same first step.
            let first = match draft.first_step() {
                Some((Behavior::Autotile, _)) => match draft.flag(Behavior::Autotile) {
                    Setting::Known(false) => "Turn automatic tiling off",
                    _ => "Turn automatic tiling on",
                },
                _ => "Change the tiling scope",
            };
            part.blocked.push(format!(
                "Automatic tiling and its scope would both change. {first} on the Windows page first, then apply this profile."
            ));
        } else {
            part.blocked.push(format!(
                "{} {}.",
                capitalize(&windows::key_label(&blocked.key).unwrap_or_default()),
                blocked.reason
            ));
        }
    }
    for key in plan.patch.keys() {
        if let Some(label) = windows::key_label(key) {
            let label = capitalize(&label);
            if !part.changes.contains(&label) {
                part.changes.push(label);
            }
        }
    }
    Some(plan.patch)
}

/// The workspace struct through a fresh `WorkspacesDraft`: only the present
/// members change; a struct this version doesn't recognize blocks.
fn plan_workspaces(
    store: &DesktopStore,
    look: &WorkspacesLook,
    part: &mut DomainPlan,
) -> Option<DesktopPatch> {
    let mut draft = match WorkspacesDraft::load(store) {
        Ok(draft) => draft,
        Err(error) => {
            part.blocked.push(error.to_string());
            return None;
        }
    };
    if let Some(mode) = look.workspace_mode {
        draft.set_mode(mode.into());
    }
    if let Some(layout) = look.workspace_layout {
        draft.set_layout(layout.into());
    }
    if let Some(action) = look.action_on_typing {
        draft.set_typing(action.into());
    }
    if let Some(wrap) = look.workspace_wraparound {
        draft.set_wraparound(wrap);
    }
    let plan = draft.plan();
    for blocked in &plan.blocked {
        part.blocked.push(format!(
            "Workspace settings have values this version doesn’t recognize ({}).",
            blocked.reason
        ));
    }
    let edits = draft.edits();
    for (edited, label) in [
        (edits.workspace_mode.is_some(), "Workspaces on each display"),
        (edits.workspace_layout.is_some(), "Workspace orientation"),
        (edits.action_on_typing.is_some(), "Typing in the overview"),
        (edits.workspace_wraparound.is_some(), "Wrapping"),
    ] {
        if edited && plan.blocked.is_empty() {
            part.changes.push(label.into());
        }
    }
    Some(plan.patch)
}

/// A planned Wallpaper part.
struct Prepared {
    patch: DesktopPatch,
    assets: Vec<Asset>,
    background: Option<BgEntry>,
    source: wallpaper::automatic::Source,
}

/// The wallpaper through a fresh `WallpaperDraft`: the shared background,
/// then (when the profile has one) the display arrangement, which replaces
/// the target's: exactly the profile's displays get their own background.
/// Members the profile leaves out keep the target's values.
fn plan_wallpaper(
    store: &DesktopStore,
    installed: &Installed,
    look: &WallpaperLook,
    part: &mut DomainPlan,
) -> Option<Prepared> {
    let mut draft = match WallpaperDraft::load(store) {
        Ok(draft) => draft,
        Err(error) => {
            part.blocked.push(error.to_string());
            return None;
        }
    };
    // Use shared deliberately retains output files. A saved arrangement
    // may name an offline output that is no longer listed, so load its
    // retained baseline too, before any edits. Keep the draft's metadata
    // (including unreadable entries) and normal unsupported-value checks.
    let mut snapshot = draft.snapshot().clone();
    for name in look.per_display.iter().flat_map(|map| map.keys()) {
        let key = match wallpaper::output_key(name) {
            Ok(key) => key,
            Err(error) => {
                part.blocked.push(error.to_string());
                return None;
            }
        };
        if snapshot.entry(&key).is_none() {
            match store.load(std::slice::from_ref(&key)) {
                Ok(one) => {
                    snapshot.insert(key.clone(), one.entry(&key).cloned().unwrap_or_default())
                }
                Err(error) => {
                    part.blocked.push(format!(
                        "The background file of {name} can’t be read: {error}"
                    ));
                    return None;
                }
            }
        }
    }
    draft.refresh(snapshot);
    let mut origins = Vec::new();
    // First, as displays without their own start from it.
    if let Some(background) = &look.background {
        edit_background(
            &mut draft,
            &Target::Shared,
            background,
            installed,
            &mut origins,
            part,
        );
    }
    match look.displays {
        Some(DisplaysName::Same) => draft.set_displays(Displays::Same),
        Some(DisplaysName::Separate) => {
            draft.set_displays(Displays::Separate);
            let none = BTreeMap::new();
            let map = look.per_display.as_ref().unwrap_or(&none);
            let connected = wallpaper::discover_outputs(store);
            for view in draft.display_views() {
                if view.listed && !map.contains_key(&view.name) {
                    draft.use_shared(&view.name);
                }
            }
            for (name, background) in map {
                draft.assign_own(name);
                let target = Target::Output(name.clone());
                edit_background(
                    &mut draft,
                    &target,
                    background,
                    installed,
                    &mut origins,
                    part,
                );
                if draft.entry(&target).is_none() {
                    part.blocked.push(format!(
                        "The background file of {name} can’t be read, so it can’t be replaced."
                    ));
                }
                if !connected.contains(name) {
                    part.warnings.push(format!(
                        "{name} isn’t connected; its background is kept for when it is."
                    ));
                }
            }
        }
        None => {
            let own = draft
                .display_views()
                .into_iter()
                .any(|view| view.listed && view.own.is_some());
            if own && draft.displays() == Setting::Known(Displays::Separate) {
                part.warnings
                    .push("Displays with their own background keep it.".into());
            }
        }
    }
    let plan = draft.plan();
    for blocked in &plan.blocked {
        part.blocked.push(format!(
            "The {} {}.",
            wallpaper::key_label(&blocked.key).unwrap_or_default(),
            blocked.reason
        ));
    }
    // Pictures and folders the change would newly show must be available
    // (the Wallpaper page's rule); an unchanged one never blocks.
    let mut assets = Vec::new();
    for path in draft.edited_paths() {
        let origin = origins
            .iter()
            .find(|(known, _)| *known == path)
            .map_or(Origin::Personal, |(_, origin)| origin.clone());
        let asset = Asset {
            path,
            origin,
            package_digest: None,
        };
        if let Err(reason) = asset.check() {
            part.blocked.push(reason);
        }
        assets.push(asset);
    }
    // What each written entry shows, where the profile names its source.
    let mut named: Vec<(Option<ConfigKey>, &BackgroundLook)> = Vec::new();
    named.extend(
        look.background
            .iter()
            .map(|look| (Some(wallpaper::shared_key()), look)),
    );
    for (name, look) in look.per_display.iter().flatten() {
        named.push((wallpaper::output_key(name).ok(), look));
    }
    for key in plan.patch.keys() {
        if let Some(label) = wallpaper::key_label(key) {
            let source = named
                .iter()
                .find(|(known, _)| known.as_ref() == Some(key))
                .and_then(|(_, look)| look.source.as_ref());
            let label = match source {
                Some(source) => format!("{}: {}", capitalize(&label), describe(source)),
                None => capitalize(&label),
            };
            if !part.changes.contains(&label) {
                part.changes.push(label);
            }
        }
    }
    Some(Prepared {
        patch: plan.patch,
        assets,
        background: draft.effective(&Target::Shared).known().cloned(),
        source: wallpaper::automatic::source(store, &draft),
    })
}

/// Reuse exactly the wallpaper planner without rebuilding native themes for
/// color analysis or its source-version check.
pub fn planned_wallpaper_source(
    store: &DesktopStore,
    installed: &Installed,
    look: Option<&WallpaperLook>,
) -> Option<wallpaper::automatic::Source> {
    if let Some(look) = look {
        let mut part = DomainPlan::new(Domain::Wallpaper);
        let prepared = plan_wallpaper(store, installed, look, &mut part)?;
        part.blocked.is_empty().then_some(prepared.source)
    } else {
        WallpaperDraft::load(store)
            .ok()
            .map(|draft| wallpaper::automatic::source(store, &draft))
    }
}

/// A source as the change list names it.
fn describe(source: &SourceLook) -> String {
    match source {
        SourceLook::Bundled(name) => palette_title(name).to_string(),
        SourceLook::System(relative) => file_label(Path::new(relative)),
        SourceLook::Color(rgb) => match rgb {
            super::format::Rgb::Hex(text) => format!("color {text}"),
            super::format::Rgb::Exact(_) => "a color".into(),
        },
        SourceLook::Gradient(_) => "a gradient".into(),
        SourceLook::Image(path) | SourceLook::Folder(path) => file_label(path),
    }
}

/// Sets the members `look` has on `target`, resolving its source.
fn edit_background(
    draft: &mut WallpaperDraft,
    target: &Target,
    look: &BackgroundLook,
    installed: &Installed,
    origins: &mut Vec<(PathBuf, Origin)>,
    part: &mut DomainPlan,
) {
    let current = draft.entry(target).and_then(|entry| entry.known().cloned());
    let whose = match target {
        Target::Shared => String::new(),
        Target::Output(name) => format!("{name}: "),
    };
    if let Some(source) = &look.source {
        match resolve(
            source,
            installed,
            current.as_ref().map(|entry| &entry.source),
        ) {
            Ok((source, origin)) => {
                if let BgSource::Path(path) = &source {
                    origins.push((path.clone(), origin));
                }
                draft.set_source(target, source);
            }
            Err(reason) => part.blocked.push(format!("{whose}{reason}")),
        }
    }
    match look.scaling(current.as_ref().map(|entry| &entry.scaling_mode)) {
        Ok(Some(scaling)) => draft.set_scaling(target, scaling),
        Ok(None) => {}
        Err(reason) => part.blocked.push(format!("{whose}{reason}")),
    }
    if let Some(every) = look.rotation() {
        draft.set_rotation(target, every);
    }
    if let Some(order) = look.sampling() {
        draft.set_sampling(target, order);
    }
}

/// The native source of a profile source. A bundled wallpaper is the
/// installed one (a desktop already showing the same wallpaper from an
/// install before the rename keeps it); a COSMIC wallpaper must resolve
/// canonically inside a trusted folder.
fn resolve(
    source: &SourceLook,
    installed: &Installed,
    current: Option<&BgSource>,
) -> Result<(BgSource, Origin), String> {
    Ok(match source {
        SourceLook::Bundled(name) => {
            let palette = crate::desktop::wallpaper::bundled_name(name)
                .ok_or_else(|| format!("“{name}” is not a built-in wallpaper."))?;
            let installed_path = wallpaper::bundled_path(&installed.bundled_dir, palette);
            let path = match current {
                Some(BgSource::Path(path))
                    if !installed_path.is_file()
                        && path.is_file()
                        && wallpaper::classify(path, &installed.bundled_dir, &[])
                            == Class::Bundled(palette) =>
                {
                    path.clone()
                }
                _ => installed_path,
            };
            (BgSource::Path(path), Origin::Bundled(palette))
        }
        SourceLook::System(relative) => installed
            .background_dirs
            .iter()
            .find_map(|dir| {
                let path = dir.join(relative);
                (wallpaper::classify(&path, Path::new(""), std::slice::from_ref(dir))
                    == Class::System(relative.clone()))
                .then(|| (path, dir.clone()))
            })
            .map(|(path, dir)| (BgSource::Path(path), Origin::System(dir, relative.clone())))
            .ok_or_else(|| format!("The COSMIC wallpaper “{relative}” isn’t on this computer."))?,
        SourceLook::Color(_) | SourceLook::Gradient(_) => (
            BgSource::Color(source.color()?.ok_or("not a color")?),
            Origin::Personal,
        ),
        SourceLook::Image(path) | SourceLook::Folder(path) => {
            (BgSource::Path(path.clone()), Origin::Personal)
        }
    })
}

/// Desktop icons through a fresh `DesktopIconsDraft`: any valid native
/// values (also between the page's slider steps); a struct this version
/// doesn't recognize blocks.
fn plan_desktop_icons(
    store: &DesktopStore,
    installed: &Installed,
    look: &DesktopIconsLook,
    part: &mut DomainPlan,
) -> Option<DesktopPatch> {
    let mut draft = match DesktopIconsDraft::load(store) {
        Ok(draft) => draft,
        Err(error) => {
            part.blocked.push(error.to_string());
            return None;
        }
    };
    draft.set_members(look.edits());
    let plan = draft.plan();
    if !plan.blocked.is_empty() {
        part.blocked.push(
            "Desktop icon settings have values this version doesn’t recognize; they are kept."
                .into(),
        );
    }
    let edits = draft.edits();
    for (edited, label) in [
        (edits.icon_size.is_some(), "Icon size"),
        (edits.grid_spacing.is_some(), "Icon spacing"),
        (edits.show_content.is_some(), "Desktop folder"),
        (edits.show_mounted_drives.is_some(), "Mounted drives"),
        (edits.show_trash.is_some(), "Trash"),
    ] {
        if edited && plan.blocked.is_empty() {
            part.changes.push(label.into());
        }
    }
    if !part.changes.is_empty() {
        if installed.files == Some(false) {
            part.warnings.push(
                "COSMIC Files, which draws desktop icons, isn’t installed; these settings apply once it is."
                    .into(),
            );
        }
        part.notes.push(desktop_icons::REFRESH_NOTE.into());
    }
    Some(plan.patch)
}

fn capitalize(text: &str) -> String {
    let mut chars = text.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().chain(chars).collect(),
        None => String::new(),
    }
}

/// A profile marker is a generation request, not proof. Only accepted native
/// roles in the same transaction can establish it. Partial applications retain
/// existing proof only when its family, tint and explicit ownership are unchanged.
fn native_palette_provenance(
    profile: &Profile,
    selected: &BTreeSet<Domain>,
    unavailable_bars: &BTreeSet<Domain>,
    (current, effective): &Loaded,
    result: &mut ProfilePlan,
) -> Result<(), String> {
    const MARKER: &str = "/appearance/native_mode_palettes";
    let original = widget::get(current, MARKER);
    let requested = result
        .widget
        .iter()
        .find(|(p, _)| *p == MARKER)
        .map(|(_, e)| e.desired.clone());
    let mut next = current.clone();
    for (pointer, entry) in result.widget.changes() {
        widget::put(&mut next, pointer, entry.desired.clone())?;
    }
    let before = config::check_config_value(effective)?.appearance;
    let after = config::check_config_value(&next)?.appearance;
    let same_owner = before.family == after.family
        && before.accent == after.accent
        && before.overrides == after.overrides;
    let touched = |key: &ConfigKey| {
        result.patch.get(key).is_some() || result.patch.guards().any(|(guard, _)| guard == key)
    };
    let native_selected = [Domain::Appearance, Domain::Dock, Domain::Panel]
        .into_iter()
        .all(|domain| {
            selected.contains(&domain)
                && result
                    .domains
                    .iter()
                    .any(|part| part.domain == domain && part.blocked.is_empty())
        });
    let complete_roles = ThemeVariant::ALL.into_iter().all(|variant| {
        profile
            .appearance
            .as_ref()
            .and_then(|look| look.variant(variant))
            .is_some_and(|look| {
                [
                    ("bg_color", &look.background),
                    ("primary_container_bg", &look.primary),
                ]
                .into_iter()
                .all(|(name, color)| {
                    let key = appearance::theme::builder_key(variant, name);
                    touched(&key)
                        && color.as_ref().is_some_and(|color| {
                            !matches!(color, super::format::ColorChoice::Auto)
                                || after.overrides
                                    & crate::desktop::actions::override_for_native(&key)
                                    != 0
                        })
                })
            })
    });
    let complete_bars = [
        (Domain::Dock, PanelKind::Dock, &profile.dock),
        (Domain::Panel, PanelKind::Panel, &profile.panel),
    ]
    .into_iter()
    .all(|(domain, kind, look)| {
        look.as_ref().is_some_and(|look| look.background.is_some())
            && (unavailable_bars.contains(&domain)
                || touched(&ConfigKey::new(kind.namespace(), 1, "background").expect("panel key")))
    });
    let generated = requested
        .as_ref()
        .and_then(|v| v.as_ref())
        .and_then(|v| v.as_u64())
        == Some(1)
        && after.family.is_some()
        && native_selected
        && complete_roles
        && complete_bars;
    let explicitly_unproven = requested
        .as_ref()
        .is_some_and(|value| value.as_ref().and_then(|v| v.as_u64()) != Some(1));
    let proven =
        generated || (before.native_mode_palettes == 1 && same_owner && !explicitly_unproven);
    let desired = if proven {
        Some(serde_json::json!(1))
    } else if original.is_none() {
        None
    } else {
        Some(serde_json::json!(0))
    };
    // Guard identity even when a repair only carries the generation request.
    for pointer in [
        "/appearance/family",
        "/appearance/accent",
        "/appearance/overrides",
    ] {
        if !result.widget.iter().any(|(p, _)| p == pointer) {
            let value = widget::get(current, pointer);
            result.widget.set(pointer, value.clone(), value)?;
        }
    }
    result
        .widget
        .set(MARKER, original.clone(), desired.clone())?;
    if let Some(part) = result
        .domains
        .iter_mut()
        .find(|part| part.domain == Domain::WidgetStyle)
    {
        let label = widget::label(MARKER);
        part.changes.retain(|change| change != &label);
        if original != desired {
            part.changes.push(label);
        }
    }
    Ok(())
}

fn plan_widgets(
    (current, effective): &Loaded,
    section: &serde_json::Value,
    group: widget::WidgetGroup,
    part: &mut DomainPlan,
) -> Option<WidgetPatch> {
    let mut members = match widget_members(section, group) {
        Ok(members) => members,
        Err(reason) => {
            part.blocked.push(reason);
            return None;
        }
    };
    // Old profiles chose a visible palette before Follow desktop existed.
    // Keep that intent when applying one to a desktop-following setup.
    // A newly captured profile includes the source and therefore wins.
    if members.iter().any(|(pointer, _)| *pointer == "/theme")
        && !members
            .iter()
            .any(|(pointer, _)| *pointer == "/appearance/color_source")
    {
        members.push((
            "/appearance/color_source",
            Some(serde_json::json!("custom")),
        ));
    }
    // A palette-only preset must replace a previously picked wallpaper
    // color. Captured profiles with their own accent retain it explicitly.
    if members.iter().any(|(pointer, _)| *pointer == "/theme")
        && !members
            .iter()
            .any(|(pointer, _)| *pointer == "/appearance/accent")
    {
        members.push(("/appearance/accent", None));
    }
    // A portable layout applies to saved display variants too. Assignments
    // stay local; the transaction records each variant map for exact Undo.
    let mut display_edits = std::collections::BTreeMap::new();
    for (pointer, desired) in &members {
        let (key, field) = if let Some(key) = pointer.strip_prefix("/widget_positions/") {
            (key, "position")
        } else if let Some(key) = pointer.strip_prefix("/widget_sizes/") {
            (key, "size")
        } else {
            continue;
        };
        let path = format!("/monitor_layouts/{key}");
        let Some(field_spec) = widget::field(&path) else {
            continue;
        };
        let Some(original) = widget::get(current, field_spec.pointer) else {
            continue;
        };
        let edited = display_edits.entry(field_spec.pointer).or_insert(original);
        if let Some(layouts) = edited.as_object_mut() {
            for layout in layouts.values_mut() {
                if let Some(layout) = layout.as_object_mut() {
                    layout.insert(
                        field.into(),
                        desired.clone().unwrap_or(serde_json::Value::Null),
                    );
                }
            }
        }
    }
    members.extend(display_edits.into_iter().map(|(p, v)| (p, Some(v))));
    let mut patch = WidgetPatch::new();
    for (pointer, desired) in members {
        let original = widget::get(current, pointer);
        // An absent member already showing the desired default stays absent
        // (and is guarded), so capture → apply on the same desktop is a no-op.
        let desired = match (&original, &desired) {
            (None, Some(value)) if widget::get(effective, pointer).as_ref() == Some(value) => None,
            _ => desired,
        };
        if original != desired {
            part.changes.push(widget::label(pointer));
        }
        if let Err(reason) = patch.set(pointer, original, desired) {
            part.blocked.push(reason);
        }
    }
    Some(patch)
}

/// Applies a plan that `can_apply` as one owned, recoverable transaction:
/// the COSMIC keys and widget members together, recorded for Undo as
/// `Owner::Profile`. Refuses a blocked or empty plan, and one whose
/// wallpaper pictures or folders are no longer available (`recheck`),
/// without writing anything.
pub fn apply(store: &DesktopStore, plan: &ProfilePlan) -> Result<LatestChange, DesktopError> {
    apply_with_source(
        store,
        plan,
        &plan.label(),
        crate::desktop::store::Source::Manual,
    )
}

/// `apply`, recorded for Undo with its own `label` (automatic switches say
/// so).
pub fn apply_as(
    store: &DesktopStore,
    plan: &ProfilePlan,
    label: &str,
) -> Result<LatestChange, DesktopError> {
    apply_with_source(store, plan, label, crate::desktop::store::Source::Scheduled)
}
fn apply_with_source(
    store: &DesktopStore,
    plan: &ProfilePlan,
    label: &str,
    source: crate::desktop::store::Source,
) -> Result<LatestChange, DesktopError> {
    if let Some(reason) = plan
        .blocking_reasons()
        .into_iter()
        .chain(plan.recheck())
        .next()
    {
        return Err(DesktopError::InvalidValue {
            key: None,
            message: reason,
        });
    }
    let islands: Vec<_> = plan
        .patch
        .iter()
        .filter(|(k, _)| k.namespace() == dock_islands_core::NAMESPACE)
        .map(|(key, entry)| {
            (
                key.key().to_string(),
                entry
                    .desired
                    .as_ref()
                    .and_then(|v| v.decode::<dock_islands_core::Settings>().ok())
                    .unwrap_or_default(),
            )
        })
        .collect();
    let mut component = if !islands.is_empty() {
        crate::desktop::islands::Client::connect(store).ok()
    } else {
        None
    };
    if plan.appearance_only && !plan.is_empty() {
        super::ProfileLibrary::new(store)
            .retain_before_themes()
            .map_err(|error| DesktopError::Io(error.to_string()))?;
    }
    let change = crate::desktop::actions::commit(
        store,
        &plan.patch,
        Some(&plan.widget),
        Owner::Profile,
        label,
        source,
    )?;
    if let Some(c) = &mut component {
        for (panel, expected) in islands {
            if let Err(error) = c.request(&dock_islands_core::Request::Activate { panel, expected })
            {
                return Err(DesktopError::Io(format!(
                    "Preset saved. {error}. Undo is available."
                )));
            }
        }
    }
    Ok(change)
}

#[cfg(test)]
mod arrangement_tests {
    use dock_islands_core::{Placement, Section, Settings};

    #[test]
    fn a_look_changes_how_islands_show_but_not_what_sits_where() {
        let mut current = Settings::default();
        current.default_layout.together.order = [Section::End, Section::Start, Section::Center];
        current.default_layout.together.position = 1234;
        current
            .outputs
            .insert("DP-1".into(), current.default_layout.clone());
        let look = Settings {
            enabled: true,
            placement: Placement::Together,
            gap: 10,
            ..Default::default()
        };
        let applied = super::keep_arrangement(look.clone(), current.clone());
        assert!(applied.enabled);
        // Islands not on yet: the look's Together.
        assert_eq!(applied.placement, Placement::Together);
        assert_eq!(applied.gap, 10);
        assert_eq!(applied.default_layout, current.default_layout);
        assert_eq!(applied.outputs, current.outputs);
        // A record with its own arrangement (a saved setup) restores it;
        // monitor arrangements stay local either way.
        // Islands already on and apart: they stay apart.
        let mut apart = current.clone();
        apart.enabled = true;
        apart.placement = Placement::Separate;
        assert_eq!(
            super::keep_arrangement(look, apart).placement,
            Placement::Separate
        );
        let mut saved = Settings::default();
        saved.default_layout.separate.positions = [0, 100, 200];
        let applied = super::keep_arrangement(saved.clone(), current.clone());
        assert_eq!(applied.default_layout, saved.default_layout);
        assert_eq!(applied.outputs, current.outputs);
    }
}
