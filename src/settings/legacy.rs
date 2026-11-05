//! Version-one page drafts are optional proposals. Read originals in place;
//! only an explicit recovery action commits, and it composes before writing.
use super::{session_drafts::Kind, *};
use crate::desktop::{
    actions::Composition,
    appearance::AppearanceDraft,
    store::{DesktopPatch, DesktopStore, Owner, Source},
};
use serde::Deserialize;

const KINDS: [Kind; 11] = [
    Kind::Dock,
    Kind::Appearance,
    Kind::Windows,
    Kind::Workspaces,
    Kind::Wallpaper,
    Kind::DesktopIcons,
    Kind::WindowRules,
    Kind::Shortcuts,
    Kind::Layout,
    Kind::Automation,
    Kind::Preset,
];
#[derive(serde::Serialize, Deserialize)]
struct Record {
    format: String,
    version: u32,
    kind: String,
    draft: String,
}
#[derive(Clone)]
pub(super) struct Proposal {
    pub kind: Kind,
    pub selected: bool,
    pub use_proposed: bool,
    pub problem: Option<String>,
    pub summary: String,
    pub text: String,
    original: Vec<u8>,
}
#[derive(Clone, Default)]
pub(super) struct Recovery {
    pub rows: Vec<Proposal>,
}
impl Recovery {
    pub fn load() -> Self {
        DesktopStore::from_env()
            .ok()
            .filter(DesktopStore::writable)
            .map_or_else(Self::default, |store| Self::read(&store))
    }
    fn read(store: &DesktopStore) -> Self {
        let mut rows = vec![];
        for kind in KINDS {
            let path = store
                .state_dir()
                .join(format!("settings-draft-{}.json", kind.key()));
            let meta = match std::fs::symlink_metadata(&path) {
                Ok(meta) => meta,
                Err(_) => continue,
            };
            let result = if !meta.is_file()
                || meta.file_type().is_symlink()
                || meta.len() > 4 * 1024 * 1024
            {
                Err("Unsupported recovery file; original retained".into())
            } else {
                std::fs::read(&path)
                    .map_err(|e| e.to_string())
                    .and_then(|bytes| {
                        let record: Record =
                            serde_json::from_slice(&bytes).map_err(|e| e.to_string())?;
                        if record.format != "desktop-cards-settings-draft"
                            || record.version != 1
                            || record.kind != kind.key()
                        {
                            return Err("Unrecognized recovery version; original retained".into());
                        }
                        Ok((bytes, record.draft))
                    })
            };
            let (original, text, problem) = match result {
                Ok((bytes, text)) => (bytes, text, None),
                Err(error) => (vec![], String::new(), Some(error)),
            };
            let mut row = Proposal {
                kind,
                selected: false,
                use_proposed: false,
                summary: String::new(),
                text,
                original,
                problem,
            };
            let form = store
                .state_file(&format!("settings-recovery-form-{}.json", kind.key()))
                .ok()
                .filter(|path| {
                    std::fs::metadata(path).is_ok_and(|m| m.is_file() && m.len() < 4 * 1024 * 1024)
                });
            if let Some(bytes) = form.and_then(|path| std::fs::read(path).ok()) {
                if bytes.len() < 4 * 1024 * 1024 {
                    if let Ok((original, text)) =
                        serde_json::from_slice::<(Vec<u8>, String)>(&bytes)
                    {
                        if original == row.original {
                            row.text = text;
                        }
                    }
                }
            }
            row.summary = row.form_summary(store);
            if row.problem.is_none() {
                let mut composition = match Composition::new(store) {
                    Ok(c) => c,
                    Err(e) => {
                        row.problem = Some(e.to_string());
                        rows.push(row);
                        continue;
                    }
                };
                match row.compose(store, &mut composition) {
                    Ok(()) => {
                        row.selected = true;
                        row.summary = proposal_summary(&composition);
                    }
                    Err(error) => {
                        row.problem = Some(error);
                        // Show the proposed values beside an outside-write
                        // conflict without granting permission to replace it.
                        if row.kind != Kind::Preset && row.fields().is_empty() {
                            if let Ok(mut preview) = Composition::new(store) {
                                row.use_proposed = true;
                                if row.compose(store, &mut preview).is_ok() {
                                    row.summary = proposal_summary(&preview);
                                }
                                row.use_proposed = false;
                            }
                        }
                    }
                }
            }
            rows.push(row);
        }
        Self { rows }
    }
    pub fn discard(&mut self, index: usize) -> Result<(), String> {
        let Some(row) = self.rows.get(index) else {
            return Ok(());
        };
        let store = DesktopStore::from_env().map_err(|e| e.to_string())?;
        row.archive(&store)?;
        self.rows.remove(index);
        Ok(())
    }
    pub fn apply(&mut self) -> Result<(), String> {
        let store = DesktopStore::from_env().map_err(|e| e.to_string())?;
        self.apply_at(&store)
    }
    pub fn apply_one(&mut self, index: usize) -> Result<(), String> {
        let selected: Vec<_> = self.rows.iter().map(|r| (r.kind, r.selected)).collect();
        for (i, row) in self.rows.iter_mut().enumerate() {
            row.selected = i == index;
        }
        let result = self.apply();
        for row in &mut self.rows {
            row.selected = selected
                .iter()
                .find(|(kind, _)| *kind == row.kind)
                .is_some_and(|(_, on)| *on);
        }
        result
    }
    fn apply_at(&mut self, store: &DesktopStore) -> Result<(), String> {
        let mut composition = Composition::new(store).map_err(|e| e.to_string())?;
        if !self.rows.iter().any(|r| r.selected) {
            return Err("Select the choices you want to recover".into());
        }
        let mut explicit = 0;
        for row in self.rows.iter_mut().filter(|r| r.selected) {
            if let Err(error) = row.compose(&store, &mut composition) {
                row.problem = Some(error.clone());
                return Err(format!("{}: {error}", row.kind.title()));
            }
            if matches!(row.kind, Kind::Dock | Kind::Appearance) {
                for key in composition.native.keys() {
                    explicit |= crate::desktop::actions::override_for_native(key);
                }
            }
        }
        let (native, mut widgets) = composition.finish()?;
        if explicit != 0 {
            let (raw, config) = crate::desktop::wallpaper::automatic::read_config(store)?;
            widgets.set(
                "/appearance/overrides",
                crate::desktop::store::widget::get(&raw, "/appearance/overrides"),
                Some((config.appearance.overrides | explicit).into()),
            )?;
        }
        if !native.is_empty() || !widgets.is_empty() {
            crate::desktop::actions::commit(
                &store,
                &native,
                Some(&widgets),
                Owner::Profile,
                "Recovered settings",
                Source::Manual,
            )
            .map_err(|e| e.to_string())?;
        }
        // A crash before archival simply leaves an idempotent proposal. Its
        // desired values already match and cannot replay stale page handlers.
        for row in self.rows.iter().filter(|r| r.selected) {
            row.archive(&store)?;
        }
        self.rows.retain(|r| !r.selected);
        Ok(())
    }
}
fn proposal_summary(c: &Composition) -> String {
    let mut parts = Vec::new();
    for (key, entry) in c.native.iter() {
        if key.namespace() == dock_islands_core::NAMESPACE {
            if let Some(settings) = entry
                .desired
                .as_ref()
                .and_then(|raw| raw.decode::<dock_islands_core::Settings>().ok())
            {
                let old = entry
                    .original
                    .as_ref()
                    .and_then(|raw| raw.decode::<dock_islands_core::Settings>().ok());
                let area = key.key();
                if old
                    .as_ref()
                    .is_none_or(|old| old.enabled != settings.enabled)
                {
                    parts.push(format!(
                        "{area} style → {}",
                        if settings.enabled {
                            "Islands"
                        } else {
                            "Single bar"
                        }
                    ));
                }
                if old
                    .as_ref()
                    .is_none_or(|old| old.placement != settings.placement)
                {
                    parts.push(format!(
                        "{area} groups → {}",
                        match settings.placement {
                            dock_islands_core::Placement::Together => "Together",
                            dock_islands_core::Placement::Separate => "Separate",
                        }
                    ));
                }
                if old.as_ref().is_none_or(|old| old.gap != settings.gap) {
                    parts.push(format!("{area} group gap → {} px", settings.gap));
                }
                if old
                    .as_ref()
                    .is_some_and(|old| old.default_layout != settings.default_layout)
                {
                    parts.push(format!("{area} group positions → Saved arrangement"));
                }
                if old
                    .as_ref()
                    .is_none_or(|old| old.outputs != settings.outputs)
                    && !settings.outputs.is_empty()
                {
                    parts.push(format!(
                        "{area} display positions → {} saved displays",
                        settings.outputs.len()
                    ));
                }
                continue;
            }
        }
        let area = if key.namespace().ends_with(".Dock") {
            "Dock".to_owned()
        } else if key.namespace().ends_with(".Panel") {
            "Panel".to_owned()
        } else if let Some((variant, _)) = crate::desktop::appearance::ThemeVariant::of(key) {
            variant.title().to_owned()
        } else {
            String::new()
        };
        let name = match key.key() {
            "bg_color" => "background",
            "primary_container_bg" => "primary surface",
            "secondary_container_bg" => "secondary surface",
            "window_hint" => "window outline",
            "all" => "shared wallpaper",
            key => key,
        };
        let value = entry.desired.as_ref().map_or("System default".into(), |v| {
            if key.namespace() == crate::desktop::wallpaper::NAMESPACE {
                if let Ok(entry) = ron::from_str::<crate::desktop::wallpaper::BgEntry>(v.as_str()) {
                    use crate::desktop::wallpaper::{BgColor, BgSource};
                    return match entry.source {
                        BgSource::Color(BgColor::Single(rgb)) => {
                            format!("Solid color {}", wallpaper::hex(rgb))
                        }
                        BgSource::Color(BgColor::Gradient(g)) => {
                            format!("{}-color gradient", g.colors.len())
                        }
                        BgSource::Path(path) => path
                            .file_name()
                            .map_or("Picture or slideshow".into(), |name| {
                                name.to_string_lossy().into_owned()
                            }),
                    };
                }
            }
            match v.as_str().trim() {
                "true" => "On".into(),
                "false" => "Off".into(),
                _ => short_value(v.as_str()),
            }
        });
        parts.push(format!(
            "{}{} → {value}",
            if area.is_empty() {
                String::new()
            } else {
                format!("{area} ")
            },
            name.replace('_', " ")
        ));
    }
    for variant in &c.relink_outline {
        parts.push(format!(
            "{} window outline → Follow accent",
            variant.title()
        ));
    }
    for (pointer, entry) in c.widgets.changes().filter(|(p, _)| {
        !matches!(
            *p,
            "/appearance/overrides" | "/appearance/wallpaper_color_stamp"
        )
    }) {
        parts.push(format!(
            "{} → {}",
            crate::desktop::store::widget::label(pointer),
            entry
                .desired
                .as_ref()
                .map_or("Theme default".into(), |v| recovered_widget_value(
                    pointer, v
                ))
        ));
    }
    if parts.is_empty() {
        "Already matches the current desktop".into()
    } else {
        parts.join(" · ")
    }
}
fn recovered_widget_value(pointer: &str, value: &serde_json::Value) -> String {
    if value.is_null() {
        return "Theme default".into();
    }
    if pointer == "/appearance/accent" {
        if let Ok([r, g, b]) = serde_json::from_value::<[u8; 3]>(value.clone()) {
            return format!("#{r:02X}{g:02X}{b:02X}");
        }
    }
    if let Some(on) = value.as_bool() {
        return if pointer == "/appearance/sync_colors" {
            if on { "Whole desktop" } else { "Widgets only" }
        } else if on {
            "On"
        } else {
            "Off"
        }
        .into();
    }
    if pointer == "/appearance/color_policy" {
        return match value.as_str() {
            Some("wallpaper") => "From wallpaper".into(),
            Some("theme") => "Theme color".into(),
            Some("custom") => "Custom color".into(),
            _ => short_value(&value.to_string()),
        };
    }
    short_value(&value.to_string())
}

fn short_value(value: &str) -> String {
    let value = value.split_whitespace().collect::<Vec<_>>().join(" ");
    let compact = value
        .strip_prefix("Some(")
        .and_then(|v| v.strip_suffix(')'))
        .unwrap_or(&value)
        .trim_matches('"');
    if compact.chars().count() > 64 {
        format!("{}…", compact.chars().take(64).collect::<String>())
    } else {
        compact.to_owned()
    }
}
impl Proposal {
    fn form_summary(&self, store: &DesktopStore) -> String {
        let fields = self.fields();
        if !fields.is_empty() {
            return fields
                .iter()
                .map(|(name, value)| format!("{name} → {value}"))
                .collect::<Vec<_>>()
                .join(" · ");
        }
        if self.kind == Kind::Preset {
            if let Ok(saved) = self.decode::<profiles::SavedSelection>() {
                let profile = if let Some(text) = saved.theme {
                    crate::desktop::profiles::import_local(text.as_bytes()).ok()
                } else if saved.preset {
                    crate::desktop::profiles::preset(&saved.id).map(|p| p.profile)
                } else {
                    crate::desktop::profiles::ProfileId::parse(&saved.id)
                        .ok()
                        .and_then(|id| {
                            crate::desktop::profiles::ProfileLibrary::new(store)
                                .load(&id)
                                .ok()
                        })
                };
                if let Some(profile) = profile {
                    return format!(
                        "{} · Includes {}",
                        profile.name,
                        profile
                            .domains()
                            .iter()
                            .filter(|d| !saved.excluded.iter().any(|key| {
                                crate::desktop::profiles::Domain::parse(key) == Some(**d)
                            }))
                            .map(|d| d.title())
                            .collect::<Vec<_>>()
                            .join(", ")
                    );
                }
            }
        }
        "Unfinished choices retained for review".into()
    }
    pub fn review(&mut self) {
        if let Ok(store) = DesktopStore::from_env() {
            self.summary = self.form_summary(&store);
            let result = Composition::new(&store)
                .map_err(|e| e.to_string())
                .and_then(|mut c| {
                    self.compose(&store, &mut c)?;
                    Ok(proposal_summary(&c))
                });
            match result {
                Ok(summary) => {
                    self.summary = summary;
                    self.problem = None;
                    self.selected = true;
                }
                Err(error) => {
                    self.problem = Some(error);
                    self.selected = false;
                }
            }
        }
    }

    fn decode<T: serde::de::DeserializeOwned>(&self) -> Result<T, String> {
        ron::from_str(&self.text)
            .map_err(|e| format!("Cannot read this proposal: {e}. Original retained"))
    }
    fn compose(&self, store: &DesktopStore, c: &mut Composition) -> Result<(), String> {
        let use_proposed = self.use_proposed;
        let plan = |plan: crate::desktop::appearance::Plan| -> Result<DesktopPatch, String> {
            if plan.blocked.is_empty() {
                Ok(plan.patch)
            } else {
                Err(plan
                    .blocked
                    .iter()
                    .map(|p| p.reason.clone())
                    .collect::<Vec<_>>()
                    .join(" · "))
            }
        };
        match self.kind {
            Kind::Dock => {
                for draft in self.decode::<Vec<crate::desktop::panels::PanelDraft>>()? {
                    c.patch(store, draft.patch(), use_proposed)?;
                }
            }
            Kind::Appearance => {
                let mut saved: system_appearance::SavedAppearance = self.decode()?;
                for ((variant, role), text) in &saved.hex {
                    let color = crate::desktop::appearance::parse_hex(text, role.alpha())
                        .ok_or("Finish the color below, or keep the current value")?;
                    saved.draft.set(
                        crate::desktop::appearance::Control::Color(*variant, *role),
                        crate::desktop::appearance::Value::Color(Some(color)),
                    );
                }
                c.appearance(store, saved.draft, use_proposed)?;
            }
            Kind::Windows => {
                let saved: windows::SavedWindows = self.decode()?;
                if saved.typed.delay.is_some() || saved.typed.snap.is_some() {
                    return Err(
                        "Unfinished number text is retained. Open the form to finish it.".into(),
                    );
                }
                c.patch(store, plan(saved.draft.behavior_plan())?, use_proposed)?;
                let mut gaps = AppearanceDraft::from_snapshot(saved.draft.snapshot().clone());
                for (variant, gap, value) in saved.draft.gap_edits() {
                    gaps.set(
                        match gap {
                            crate::desktop::windows::Gap::Outer => {
                                crate::desktop::appearance::Control::OuterGap(variant)
                            }
                            crate::desktop::windows::Gap::Inner => {
                                crate::desktop::appearance::Control::InnerGap(variant)
                            }
                        },
                        crate::desktop::appearance::Value::Number(value),
                    );
                }
                c.appearance(store, gaps, use_proposed)?;
            }
            Kind::Workspaces => c.patch(
                store,
                plan(
                    self.decode::<crate::desktop::workspaces::WorkspacesDraft>()?
                        .plan(),
                )?,
                use_proposed,
            )?,
            Kind::DesktopIcons => c.patch(
                store,
                plan(
                    self.decode::<crate::desktop::desktop_icons::DesktopIconsDraft>()?
                        .plan(),
                )?,
                use_proposed,
            )?,
            Kind::Wallpaper => {
                let mut saved: wallpaper::SavedWallpaper = self.decode()?;
                if !saved.fit_text.is_empty() || !saved.custom.is_empty() {
                    return Err(
                        "An unfinished wallpaper form is retained. Open it to continue.".into(),
                    );
                }
                if let Some(text) = &saved.accent_text {
                    let rgb = wallpaper::parse_rgb(text)
                        .ok_or("Finish the color below, or keep the current value")?
                        .map(|v| (v * 255.).round() as u8);
                    let colors = saved
                        .colors
                        .as_mut()
                        .ok_or("This color form has no original settings. Original retained")?;
                    colors.accent = Some(rgb);
                    colors.automatic = false;
                    colors.policy = Some(crate::config::ColorPolicy::Custom);
                    if colors.sync {
                        let draft = saved.accent.get_or_insert(
                            AppearanceDraft::load(store).map_err(|e| e.to_string())?,
                        );
                        crate::desktop::profiles::themes::linked_colors(
                            draft,
                            colors.baseline_appearance(),
                            rgb,
                        );
                        crate::desktop::profiles::themes::linked_accents(
                            draft,
                            colors.baseline_appearance(),
                            rgb,
                        );
                    }
                }
                saved.draft.check_sources()?;
                c.patch(store, plan(saved.draft.plan())?, use_proposed)?;
                if let Some(appearance) = saved.accent {
                    c.appearance(store, appearance, use_proposed)?;
                }
                if let Some(colors) = saved.colors {
                    colors.validate()?;
                    c.widgets(store, colors.patch()?, use_proposed)?;
                }
            }
            Kind::Shortcuts => {
                let saved: shortcuts::SavedShortcuts = self.decode()?;
                if saved.form.is_some() {
                    return Err(
                        "An unfinished shortcut is retained. Open the form to finish it.".into(),
                    );
                }
                c.patch(store, plan(saved.draft.plan())?, use_proposed)?;
            }
            Kind::WindowRules => {
                let saved: window_rules::SavedRules = self.decode()?;
                if saved.form.is_some() {
                    return Err(
                        "An unfinished rule is retained. Open the form to finish it.".into(),
                    );
                }
                c.patch(store, plan(saved.draft.plan())?, use_proposed)?;
            }
            Kind::Previews | Kind::Recipe => {
                return Err("Review this preview on its own screen.".into());
            }
            Kind::Preset => {
                if !use_proposed {
                    return Err(
                        "Review the included categories before using this saved theme or setup."
                            .into(),
                    );
                }
                let saved: profiles::SavedSelection = self.decode()?;
                let profile = if let Some(text) = saved.theme {
                    crate::desktop::profiles::import_local(text.as_bytes())
                        .map_err(|e| e.to_string())?
                } else if saved.preset {
                    crate::desktop::profiles::preset(&saved.id)
                        .ok_or("This preset is unavailable")?
                        .profile
                } else {
                    crate::desktop::profiles::ProfileLibrary::new(store)
                        .load(
                            &crate::desktop::profiles::ProfileId::parse(&saved.id)
                                .map_err(|e| e.to_string())?,
                        )
                        .map_err(|e| e.to_string())?
                };
                let selected = profile
                    .domains()
                    .into_iter()
                    .filter(|d| {
                        !saved
                            .excluded
                            .iter()
                            .any(|k| crate::desktop::profiles::Domain::parse(k) == Some(*d))
                    })
                    .collect();
                let planned = crate::desktop::profiles::plan(
                    store,
                    &crate::desktop::profiles::Installed::discover(store),
                    &profile,
                    &selected,
                );
                if let Some(error) = planned
                    .blocking_reasons()
                    .into_iter()
                    .chain(planned.recheck())
                    .next()
                {
                    return Err(error);
                }
                c.patch(store, planned.patch().clone(), use_proposed)?;
                c.widgets(store, planned.widget_patch().clone(), use_proposed)?;
            }
            Kind::Layout | Kind::Canvas | Kind::Automation => {
                return Err(
                    "An unfinished arrangement or schedule is retained. Open it to continue."
                        .into(),
                );
            }
        }
        Ok(())
    }
    pub fn fields(&self) -> Vec<(String, String)> {
        match self.kind {
            Kind::Appearance => self
                .decode::<system_appearance::SavedAppearance>()
                .map(|saved| {
                    saved
                        .hex
                        .iter()
                        .map(|((variant, role), text)| {
                            (
                                format!("{} {}", variant.title(), role.title()),
                                text.clone(),
                            )
                        })
                        .collect()
                })
                .unwrap_or_default(),
            Kind::Wallpaper => self
                .decode::<wallpaper::SavedWallpaper>()
                .ok()
                .and_then(|saved| saved.accent_text)
                .map(|text| vec![("Shared color".into(), text)])
                .unwrap_or_default(),
            _ => Vec::new(),
        }
    }
    pub fn set_text(&mut self, index: usize, text: String) -> Result<(), String> {
        let text: String = text.chars().filter(|c| !c.is_control()).take(36).collect();
        self.text = match self.kind {
            Kind::Appearance => {
                let mut saved: system_appearance::SavedAppearance = self.decode()?;
                if let Some((_, value)) = saved.hex.iter_mut().nth(index) {
                    *value = text;
                }
                ron::to_string(&saved)
            }
            Kind::Wallpaper => {
                let mut saved: wallpaper::SavedWallpaper = self.decode()?;
                saved.accent_text = Some(text);
                ron::to_string(&saved)
            }
            _ => return Ok(()),
        }
        .map_err(|e| e.to_string())?;
        if let Ok(store) = DesktopStore::from_env() {
            let path = store
                .state_file(&format!("settings-recovery-form-{}.json", self.kind.key()))
                .map_err(|e| e.to_string())?;
            let bytes = serde_json::to_vec(&(self.original.clone(), self.text.clone()))
                .map_err(|e| e.to_string())?;
            let temp = path.with_extension("tmp");
            std::fs::create_dir_all(path.parent().unwrap()).map_err(|e| e.to_string())?;
            std::fs::write(&temp, bytes).map_err(|e| e.to_string())?;
            std::fs::rename(temp, path).map_err(|e| e.to_string())?;
        }
        self.review();
        Ok(())
    }
    fn archive(&self, store: &DesktopStore) -> Result<(), String> {
        let original = store
            .state_file(&format!("settings-draft-{}.json", self.kind.key()))
            .map_err(|e| e.to_string())?;
        use std::hash::{Hash, Hasher};
        let bytes = std::fs::read(&original).map_err(|e| e.to_string())?;
        let mut hash = std::collections::hash_map::DefaultHasher::new();
        bytes.hash(&mut hash);
        let archived = store
            .state_file(&format!(
                "settings-recovered-{}-{:016x}.json",
                self.kind.key(),
                hash.finish()
            ))
            .map_err(|e| e.to_string())?;
        if !self.original.is_empty()
            && std::fs::read(&original).map_err(|e| e.to_string())? != self.original
        {
            return Err("The recovery record changed elsewhere; it was kept".into());
        }
        // The original is retained for manual recovery, including malformed data.
        if archived.exists() {
            if std::fs::read(&archived).map_err(|e| e.to_string())? == bytes {
                return std::fs::remove_file(original).map_err(|e| e.to_string());
            }
            return Err("An archived original differs; both records were kept".into());
        }
        std::fs::rename(original, archived).map_err(|e| e.to_string())
    }
}
impl Settings {
    pub(super) fn open_recovered_form(&mut self, index: usize) -> Task<Message> {
        let Some(row) = self.recovery_sheet.rows.get(index) else {
            return Task::none();
        };
        let (kind, text) = (row.kind, row.text.clone());
        macro_rules! restore {
            ($editor:ident,$ty:ty,$page:expr) => {{
                self.$editor.open();
                match ron::from_str::<$ty>(&text)
                    .map_err(|e| e.to_string())
                    .and_then(|saved| self.$editor.restore_session(saved))
                {
                    Ok(()) => {
                        self.checkpoint_session_drafts();
                        return self.handle(Message::Page($page));
                    }
                    Err(e) => self.failed(e),
                }
            }};
        }
        match kind {
            Kind::Appearance => restore!(
                system_appearance,
                system_appearance::SavedAppearance,
                Page::DesktopAppearance
            ),
            Kind::Windows => restore!(windows, windows::SavedWindows, Page::DesktopWindows),
            Kind::Wallpaper => {
                restore!(wallpaper, wallpaper::SavedWallpaper, Page::DesktopWallpaper)
            }
            Kind::WindowRules => restore!(
                window_rules,
                window_rules::SavedRules,
                Page::DesktopWindowRules
            ),
            Kind::Shortcuts => {
                restore!(shortcuts, shortcuts::SavedShortcuts, Page::DesktopShortcuts)
            }
            Kind::Automation => {
                restore!(automation, automation::RuleDraft, Page::DesktopAutomation)
            }
            Kind::Layout => match ron::from_str(&text)
                .map_err(|e| e.to_string())
                .and_then(|saved| self.placement.restore_session(saved))
            {
                Ok(()) => return self.handle(Message::Page(Page::Overview)),
                Err(e) => self.failed(e),
            },
            _ => {}
        }
        Task::none()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::desktop::{
        appearance::{ColorRole, Control, ThemeVariant, Value, parse_hex},
        panels::{Anchor, PanelDraft, PanelKind},
        store::tests::Fixture,
    };
    fn save<T: serde::Serialize>(store: &DesktopStore, kind: Kind, value: &T) {
        let record = Record {
            format: "desktop-cards-settings-draft".into(),
            version: 1,
            kind: kind.key().into(),
            draft: ron::to_string(value).unwrap(),
        };
        let path = store
            .state_file(&format!("settings-draft-{}.json", kind.key()))
            .unwrap();
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, serde_json::to_vec(&record).unwrap()).unwrap();
    }
    fn setup(name: &str) -> (Fixture, DesktopStore) {
        let f = Fixture::new(name);
        let path = f.0.join("config.json");
        std::fs::write(&path, "{}").unwrap();
        let store = f.store().with_app_config(path);
        (f, store)
    }
    #[test]
    fn disjoint_legacy_inputs_compose_once_and_opening_never_writes() {
        let (f, store) = setup("recovery-compose");
        let mut dock = PanelDraft::load(&store, PanelKind::Dock).unwrap();
        dock.set_value("anchor", &Anchor::Left);
        save(&store, Kind::Dock, &vec![dock]);
        let mut appearance = AppearanceDraft::load(&store).unwrap();
        appearance.set(
            Control::Color(ThemeVariant::Dark, ColorRole::Accent),
            Value::Color(parse_hex("#AC6742", false)),
        );
        save(
            &store,
            Kind::Appearance,
            &system_appearance::SavedAppearance {
                draft: appearance,
                variant: ThemeVariant::Dark,
                hex: Default::default(),
            },
        );
        let before = f.tree();
        let mut recovery = Recovery::read(&store);
        assert_eq!(f.tree(), before, "upgrade reads only");
        assert_eq!(recovery.rows.len(), 2);
        recovery.apply_at(&store).unwrap();
        assert!(recovery.rows.is_empty());
        let latest = store.latest_change().unwrap().unwrap();
        assert_eq!(latest.label, "Recovered settings");
        assert!(
            latest
                .change
                .entries()
                .iter()
                .any(|entry| entry.key.key() == "anchor")
        );
        assert!(
            latest
                .change
                .entries()
                .iter()
                .any(|entry| entry.key.key() == "accent")
        );
        assert!(
            Recovery::read(&store).rows.is_empty(),
            "migration is idempotent"
        );
        assert!(
            std::fs::read_dir(store.state_dir())
                .unwrap()
                .flatten()
                .filter(|e| e
                    .file_name()
                    .to_string_lossy()
                    .starts_with("settings-recovered-"))
                .count()
                == 2
        );
    }
    #[test]
    fn incomplete_color_stays_in_one_sheet_and_conflicting_sources_write_nothing() {
        let (f, store) = setup("recovery-invalid");
        let saved = system_appearance::SavedAppearance {
            draft: AppearanceDraft::load(&store).unwrap(),
            variant: ThemeVariant::Dark,
            hex: [((ThemeVariant::Dark, ColorRole::Accent), "#12".into())]
                .into_iter()
                .collect(),
        };
        save(&store, Kind::Appearance, &saved);
        let mut recovery = Recovery::read(&store);
        assert!(!recovery.rows[0].selected);
        assert_eq!(recovery.rows[0].fields()[0].1, "#12");
        recovery.rows[0].selected = true;
        let before = f.tree();
        assert!(recovery.apply_at(&store).is_err());
        assert_eq!(f.tree(), before);
        let mut saved = saved;
        saved
            .hex
            .insert((ThemeVariant::Dark, ColorRole::Accent), "#123456".into());
        recovery.rows[0].text = ron::to_string(&saved).unwrap();
        recovery.apply_at(&store).unwrap();
        assert!(recovery.rows.is_empty());
    }
    #[test]
    fn overlapping_semantic_inputs_are_explicit_choices() {
        let (_f, store) = setup("recovery-conflict");
        let mut c = Composition::new(&store).unwrap();
        for (index, hex) in ["#123456", "#654321"].into_iter().enumerate() {
            let mut a = AppearanceDraft::load(&store).unwrap();
            a.set(
                Control::Color(ThemeVariant::Dark, ColorRole::Accent),
                Value::Color(parse_hex(hex, false)),
            );
            let result = c.appearance(&store, a, false);
            assert_eq!(result.is_ok(), index == 0);
        }
        assert!(store.latest_change().unwrap().is_none());
    }
    #[test]
    fn malformed_original_is_retained_and_never_selected() {
        let (_f, store) = setup("recovery-malformed");
        let path = store.state_file("settings-draft-appearance.json").unwrap();
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, "old or damaged form").unwrap();
        let recovery = Recovery::read(&store);
        assert_eq!(recovery.rows.len(), 1);
        assert!(!recovery.rows[0].selected);
        assert_eq!(
            std::fs::read_to_string(path).unwrap(),
            "old or damaged form"
        );
    }
    #[test]
    fn four_legacy_records_show_choices_and_never_lock_runtime() {
        let (f, store) = setup("recovery-four");
        let mut dock = PanelDraft::load(&store, PanelKind::Dock).unwrap();
        dock.set_value("anchor", &Anchor::Left);
        save(&store, Kind::Dock, &vec![dock]);
        save(
            &store,
            Kind::Appearance,
            &system_appearance::SavedAppearance {
                draft: AppearanceDraft::load(&store).unwrap(),
                variant: ThemeVariant::Dark,
                hex: [((ThemeVariant::Dark, ColorRole::Accent), "#12".into())]
                    .into_iter()
                    .collect(),
            },
        );
        let mut wallpaper = wallpaper::WallpaperEditor::with_store(
            f.store().with_app_config(f.0.join("config.json")),
        );
        wallpaper.update(wallpaper::Message::Select(wallpaper::Pick::Color(
            crate::desktop::wallpaper::BgColor::Single([0.2, 0.4, 0.6]),
        )));
        save(&store, Kind::Wallpaper, &wallpaper.session_draft().unwrap());
        save(
            &store,
            Kind::Preset,
            &profiles::SavedSelection {
                preset: true,
                id: "glass".into(),
                excluded: vec!["wallpaper".into()],
                theme: None,
            },
        );
        let before = f.tree();
        let recovery = Recovery::read(&store);
        assert_eq!(f.tree(), before);
        assert_eq!(recovery.rows.len(), 4);
        assert_eq!(recovery.rows.iter().filter(|r| r.selected).count(), 2);
        assert!(recovery.rows.iter().any(|r| r.summary.contains("Left")));
        assert!(recovery.rows.iter().any(|r| r.summary.contains("Includes")));
        assert!(!crate::desktop::wallpaper::automatic::paused(&store));
        if let Some(dir) = std::env::var_os("DESKTOP_CARDS_RECOVERY_EVIDENCE") {
            let dir = PathBuf::from(dir);
            assert!(dir.is_absolute());
            std::fs::create_dir_all(&dir).unwrap();
            for item in std::fs::read_dir(store.state_dir()).unwrap().flatten() {
                if item
                    .file_name()
                    .to_string_lossy()
                    .starts_with("settings-draft-")
                {
                    std::fs::copy(item.path(), dir.join(item.file_name())).unwrap();
                }
            }
        }
        let key = crate::desktop::wallpaper::shared_key();
        let path = store.user_path(&key);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(
            path,
            ron::to_string(&crate::desktop::wallpaper::BgEntry::new(
                "all",
                crate::desktop::wallpaper::BgSource::Color(
                    crate::desktop::wallpaper::BgColor::Single([0.8, 0.2, 0.1]),
                ),
            ))
            .unwrap(),
        )
        .unwrap();
        let before = f.tree();
        let reviewed = Recovery::read(&store);
        let row = reviewed
            .rows
            .iter()
            .find(|r| r.kind == Kind::Wallpaper)
            .unwrap();
        assert!(row.problem.is_some() && !row.selected && !row.use_proposed);
        assert!(row.summary.contains("Solid color"), "{}", row.summary);
        assert_eq!(f.tree(), before);
    }
    #[test]
    fn recovery_relinks_derived_only_outline_while_composing_other_changes() {
        let (_f, store) = setup("recovery-derived-outline");
        let key = crate::desktop::appearance::theme::theme_key(ThemeVariant::Dark, "window_hint");
        let path = store.user_path(&key);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, "Some((red:0.3,green:0.8,blue:0.6))").unwrap();
        let mut outline = AppearanceDraft::load(&store).unwrap();
        outline.set(
            Control::Color(ThemeVariant::Dark, ColorRole::Outline),
            Value::Color(None),
        );
        let mut accent = AppearanceDraft::load(&store).unwrap();
        accent.set(
            Control::Color(ThemeVariant::Dark, ColorRole::Accent),
            Value::Color(parse_hex("#BA6348", false)),
        );
        let mut composition = Composition::new(&store).unwrap();
        composition.appearance(&store, outline, false).unwrap();
        composition.appearance(&store, accent, false).unwrap();
        let (native, widgets) = composition.finish().unwrap();
        store
            .apply_action(
                &native,
                Some(&widgets),
                Owner::Profile,
                "Recovered settings",
                Source::Manual,
            )
            .unwrap();
        let actual = AppearanceDraft::load(&store).unwrap();
        assert!(actual.outline_follows(ThemeVariant::Dark));
        assert!(actual.derived_outline(ThemeVariant::Dark).is_none());
        store
            .undo_latest(&store.latest_change().unwrap().unwrap().id)
            .unwrap();
        assert!(
            AppearanceDraft::load(&store)
                .unwrap()
                .derived_outline(ThemeVariant::Dark)
                .is_some()
        );
    }
    #[test]
    fn independent_r1_legacy_conflict_refuses_all_then_resolves_one_locally() {
        let (f, store) = setup("review-r1-legacy-resolution");
        let mut appearance = AppearanceDraft::load(&store).unwrap();
        appearance.set(
            Control::Color(ThemeVariant::Dark, ColorRole::Accent),
            Value::Color(parse_hex("#AA4422", false)),
        );
        save(
            &store,
            Kind::Appearance,
            &system_appearance::SavedAppearance {
                draft: appearance,
                variant: ThemeVariant::Dark,
                hex: Default::default(),
            },
        );
        let mut wallpaper = wallpaper::WallpaperEditor::with_store(
            f.store().with_app_config(f.0.join("config.json")),
        );
        wallpaper.update(wallpaper::Message::AccentText("#2244AA".into()));
        wallpaper.update(wallpaper::Message::AccentSubmit);
        save(&store, Kind::Wallpaper, &wallpaper.session_draft().unwrap());
        let originals: Vec<_> = [Kind::Appearance, Kind::Wallpaper]
            .into_iter()
            .map(|k| {
                std::fs::read(
                    store
                        .state_dir()
                        .join(format!("settings-draft-{}.json", k.key())),
                )
                .unwrap()
            })
            .collect();
        let untouched = f.tree();
        let mut recovery = Recovery::read(&store);
        assert_eq!(
            f.tree(),
            untouched,
            "loading must not change records or preferences"
        );
        assert_eq!(recovery.rows.iter().filter(|r| r.selected).count(), 2);
        assert!(
            recovery.apply_at(&store).is_err(),
            "competing native accent choices must not be replayed sequentially"
        );
        assert_eq!(
            f.tree(),
            untouched,
            "failed composition must have zero writes"
        );
        for row in &mut recovery.rows {
            row.selected = row.kind == Kind::Wallpaper;
        }
        recovery.apply_at(&store).unwrap();
        assert_eq!(recovery.rows.len(), 1);
        let applied = AppearanceDraft::load(&store).unwrap();
        assert!(
            applied
                .color(ThemeVariant::Dark, ColorRole::Accent)
                .known()
                .unwrap()
                .is_some()
        );
        let after_first = f.tree();
        recovery.rows[0].selected = true;
        assert!(
            recovery.apply_at(&store).is_err(),
            "the competing original must now require explicit permission to replace the field"
        );
        assert_eq!(f.tree(), after_first);
        recovery.rows[0].use_proposed = true;
        recovery.apply_at(&store).unwrap();
        assert_eq!(
            AppearanceDraft::load(&store)
                .unwrap()
                .color(ThemeVariant::Dark, ColorRole::Accent)
                .known()
                .copied(),
            Some(parse_hex("#AA4422", false))
        );
        let archives: Vec<_> = std::fs::read_dir(store.state_dir())
            .unwrap()
            .flatten()
            .filter(|e| {
                e.file_name()
                    .to_string_lossy()
                    .starts_with("settings-recovered-")
            })
            .map(|e| std::fs::read(e.path()).unwrap())
            .collect();
        for original in originals {
            assert!(
                archives.contains(&original),
                "each original record survives explicit resolution byte-for-byte"
            );
        }
        assert!(Recovery::read(&store).rows.is_empty());
    }
    #[test]
    fn repair_r1_recovery_summary_names_properties_and_colors() {
        let (_fixture, store) = setup("repair-recovery-words");
        let mut composition = Composition::new(&store).unwrap();
        let settings = dock_islands_core::Settings {
            placement: dock_islands_core::Placement::Separate,
            ..Default::default()
        };
        composition.native.set(
            crate::desktop::islands::key(crate::desktop::panels::PanelKind::Panel),
            None,
            Some(crate::desktop::store::RawValue::encode(&settings).unwrap()),
        );
        composition
            .widgets
            .set(
                "/appearance/accent",
                None,
                Some(serde_json::json!([51, 102, 153])),
            )
            .unwrap();
        composition
            .widgets
            .set("/appearance/sync_colors", None, Some(false.into()))
            .unwrap();
        composition
            .widgets
            .set(
                "/appearance/wallpaper_color_stamp",
                None,
                Some(serde_json::json!(12345678901234567890u64)),
            )
            .unwrap();
        let summary = proposal_summary(&composition);
        assert!(
            summary.contains("Panel style → Single bar")
                && summary.contains("Panel groups → Separate")
        );
        assert!(summary.contains("Chosen color → #336699") && summary.contains("Widgets only"));
        assert!(
            !summary.contains("version:")
                && !summary.contains("[51")
                && !summary.contains("default_layout")
                && !summary.contains("12345678901234567890")
                && !summary.contains("Wallpaper color source")
        );
    }
}
