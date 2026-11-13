//! The Look studio: a look made part by part, by picture. Every choice
//! changes only the draft and its preview. The desktop changes only through
//! Apply's parts review, and the library only through Save, Save as new,
//! Duplicate, Rename and Delete.
use super::icon_art::{self, Sample};
use super::*;
use crate::desktop::icon_packs::overlay::{self, Shape, Style};
use crate::desktop::profiles::built_in;
use crate::desktop::profiles::{AppearanceLook, Corners, ModeName, VariantLook};
use cosmic::cosmic_theme::Roundness;
use serde_json::{Value as Json, json};

#[cfg(test)]
mod tests;

/// One part of a look, edited as a whole from its own tile.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum Element {
    Wallpaper,
    Weather,
    Panel,
    Dock,
    Windows,
    Colors,
    Font,
    Icons,
    WidgetStyle,
    Layout,
    DesktopIcons,
}

/// Widget style members the Colors tile owns; Weather owns `/rain`, and
/// Widget style owns everything else.
const COLOR_MEMBERS: [&str; 9] = [
    "/theme",
    "/appearance/accent",
    "/appearance/color_source",
    "/appearance/sync_colors",
    "/appearance/wallpaper_colors",
    "/appearance/color_policy",
    "/appearance/contrast_assist",
    "/appearance/native_mode_palettes",
    "/appearance/wallpaper_light",
];
const WEATHER_MEMBERS: [&str; 1] = ["/rain"];

impl Element {
    pub const ALL: [Self; 11] = [
        Self::Wallpaper,
        Self::Weather,
        Self::Panel,
        Self::Dock,
        Self::Windows,
        Self::Colors,
        Self::Font,
        Self::Icons,
        Self::WidgetStyle,
        Self::Layout,
        Self::DesktopIcons,
    ];
    pub const fn title(self) -> &'static str {
        match self {
            Self::Wallpaper => "Wallpaper",
            Self::Weather => "Weather",
            Self::Panel => "Panel",
            Self::Dock => "Dock",
            Self::Windows => "Windows",
            Self::Colors => "Colors",
            Self::Font => "Font",
            Self::Icons => "Icons",
            Self::WidgetStyle => "Widget style",
            Self::Layout => "Widget layout",
            Self::DesktopIcons => "Desktop icons",
        }
    }
    /// A stable widget ID part.
    pub const fn key(self) -> &'static str {
        match self {
            Self::Wallpaper => "wallpaper",
            Self::Weather => "weather",
            Self::Panel => "panel",
            Self::Dock => "dock",
            Self::Windows => "windows",
            Self::Colors => "colors",
            Self::Font => "font",
            Self::Icons => "icons",
            Self::WidgetStyle => "style",
            Self::Layout => "layout",
            Self::DesktopIcons => "desktop-icons",
        }
    }
    /// The preview part a pointer on this tile rings.
    pub const fn domain(self) -> Domain {
        match self {
            Self::Wallpaper | Self::Weather => Domain::Wallpaper,
            Self::Panel => Domain::Panel,
            Self::Dock => Domain::Dock,
            Self::Windows | Self::Colors | Self::Font | Self::Icons => Domain::Windows,
            Self::WidgetStyle => Domain::WidgetStyle,
            Self::Layout => Domain::WidgetLayout,
            Self::DesktopIcons => Domain::DesktopIcons,
        }
    }
    /// The parts a thumbnail of this element needs planned.
    fn domains(self) -> BTreeSet<Domain> {
        match self {
            Self::Wallpaper => [Domain::Wallpaper].into(),
            Self::Weather | Self::WidgetStyle => [Domain::WidgetStyle].into(),
            Self::Panel => [Domain::Panel, Domain::Appearance].into(),
            Self::Dock => [Domain::Dock, Domain::Appearance].into(),
            Self::Windows => [Domain::Windows, Domain::Appearance].into(),
            Self::Colors => [Domain::Appearance, Domain::WidgetStyle].into(),
            Self::Font | Self::Icons => [Domain::Appearance].into(),
            Self::Layout => [Domain::WidgetLayout, Domain::WidgetStyle].into(),
            Self::DesktopIcons => [Domain::DesktopIcons].into(),
        }
    }
    /// Whether a look may leave this part as it is on the desktop.
    pub const fn leavable(self) -> bool {
        !matches!(self, Self::Colors | Self::Font | Self::Icons)
    }
    /// Sets `into`'s part to `from`'s, leaving everything else.
    pub fn copy(self, from: &Profile, into: &mut Profile) {
        match self {
            Self::Wallpaper => into.wallpaper = from.wallpaper.clone(),
            Self::Panel => into.panel = from.panel.clone(),
            Self::Dock => into.dock = from.dock.clone(),
            Self::Layout => into.widget_layout = from.widget_layout.clone(),
            Self::DesktopIcons => into.desktop_icons = from.desktop_icons.clone(),
            Self::Weather => copy_members(&WEATHER_MEMBERS, from, into),
            Self::WidgetStyle => {
                // Everything but the colors and the weather, which keep
                // `into`'s own.
                let mut kept = into.clone();
                kept.widget_style = from.widget_style.clone();
                copy_members(&COLOR_MEMBERS, into, &mut kept);
                copy_members(&WEATHER_MEMBERS, into, &mut kept);
                into.widget_style = kept.widget_style;
            }
            Self::Colors => {
                copy_members(&COLOR_MEMBERS, from, into);
                copy_appearance(self, from.appearance.as_ref(), into);
            }
            Self::Windows => {
                into.windows = from.windows.clone();
                copy_appearance(self, from.appearance.as_ref(), into);
            }
            Self::Font | Self::Icons => copy_appearance(self, from.appearance.as_ref(), into),
        }
        tidy(into);
    }
    /// Only this part of `profile`.
    pub fn extract(self, profile: &Profile) -> Profile {
        let mut part = blank();
        self.copy(profile, &mut part);
        part
    }
    /// Whether `profile` sets this part.
    pub fn has(self, profile: &Profile) -> bool {
        let part = self.extract(profile);
        match self {
            Self::Wallpaper => part
                .wallpaper
                .as_ref()
                .is_some_and(|look| look.background.is_some()),
            _ => part != blank(),
        }
    }
    /// Whether two looks set this part the same way.
    pub fn same(self, a: &Profile, b: &Profile) -> bool {
        self.extract(a) == self.extract(b)
    }
}

fn blank() -> Profile {
    Profile::new("Part").expect("a valid name")
}

fn member(profile: &Profile, pointer: &str) -> Option<Json> {
    profile.widget_style.as_ref()?.pointer(pointer).cloned()
}

fn copy_members(pointers: &[&str], from: &Profile, into: &mut Profile) {
    for pointer in pointers {
        let value = member(from, pointer);
        if value.is_none() && into.widget_style.is_none() {
            continue;
        }
        let style = into.widget_style.get_or_insert_with(|| json!({}));
        let _ = widget::put(style, pointer, value);
    }
}

fn copy_appearance(element: Element, from: Option<&AppearanceLook>, into: &mut Profile) {
    let empty = AppearanceLook::default();
    let from = from.unwrap_or(&empty);
    let target = into.appearance.get_or_insert_with(Default::default);
    let variants = |target: &mut AppearanceLook, copy: &dyn Fn(&mut VariantLook, &VariantLook)| {
        for (into, from) in [
            (&mut target.light, &from.light),
            (&mut target.dark, &from.dark),
        ] {
            let from = from.clone().unwrap_or_default();
            copy(into.get_or_insert_with(Default::default), &from);
        }
    };
    match element {
        Element::Font => {
            target.interface_font = from.interface_font.clone();
            target.monospace_font = from.monospace_font.clone();
        }
        Element::Icons => {
            target.icon_theme = from.icon_theme.clone();
            target.icon_shape = from.icon_shape;
            target.folder_tint = from.folder_tint;
        }
        Element::Colors => {
            target.mode = from.mode;
            variants(target, &|into, from| {
                into.accent = from.accent.clone();
                into.background = from.background.clone();
                into.primary = from.primary.clone();
                into.secondary = from.secondary.clone();
                into.outline = from.outline.clone();
                into.blur = from.blur;
                into.glass = from.glass.clone();
                into.frosted = from.frosted.clone();
            });
        }
        Element::Windows => {
            target.window = from.window.clone();
            target.interface_density = from.interface_density;
            target.header_size = from.header_size;
            variants(target, &|into, from| {
                into.corners = from.corners.clone();
                into.gaps = from.gaps;
                into.outline_width = from.outline_width;
                into.spacing = from.spacing.clone();
            });
        }
        _ => {}
    }
}

/// Drops empty sections, so a part that is unset reads as unset.
fn tidy(profile: &mut Profile) {
    if let Some(appearance) = &mut profile.appearance {
        for variant in [&mut appearance.light, &mut appearance.dark] {
            if variant.as_ref() == Some(&VariantLook::default()) {
                *variant = None;
            }
        }
        if *appearance == AppearanceLook::default() {
            profile.appearance = None;
        }
    }
    if let Some(style) = &mut profile.widget_style {
        if let Some(object) = style.get_mut("appearance").and_then(Json::as_object_mut)
            && object.is_empty()
        {
            style
                .as_object_mut()
                .map(|style| style.remove("appearance"));
        }
        if style.as_object().is_some_and(serde_json::Map::is_empty) {
            profile.widget_style = None;
        }
    }
}

/// Weather effects the Weather picker offers, as widget style `rain` values.
pub const WEATHER: [(&str, &str); 5] = [
    ("off", "No effect"),
    ("rain", "Rain"),
    ("snow", "Snow"),
    ("fog", "Fog"),
    ("clouds", "Clouds"),
];

fn weather(id: &str) -> Option<Json> {
    Some(match id {
        "off" => json!({"enabled": false}),
        "clouds" => json!({"enabled": true, "effect": "clouds",
            "clouds": {"coverage": 35, "softness": 75, "speed": 15, "wind": 15, "strength": 20}}),
        id => profiles::atmosphere(id)?,
    })
}

/// Accent colors the Colors picker offers.
pub const ACCENTS: [(&str, [u8; 3]); 8] = [
    ("Teal", [0x6a, 0xc2, 0xc6]),
    ("Blue", [0x7a, 0xa7, 0xff]),
    ("Violet", [0xb5, 0x9c, 0xff]),
    ("Pink", [0xf2, 0x9a, 0xc0]),
    ("Red", [0xf2, 0x8b, 0x82]),
    ("Orange", [0xf6, 0xb2, 0x6b]),
    ("Yellow", [0xe8, 0xd0, 0x6a]),
    ("Green", [0x8f, 0xd1, 0x8f]),
];

/// Window corner and gap presets the Windows picker offers.
pub const CORNERS: [(&str, u8); 3] = [("Round", 0), ("Slightly round", 1), ("Square", 2)];
pub const GAPS: [(&str, (u32, u32)); 3] = [
    ("No gaps", (0, 0)),
    ("Small gaps", (4, 8)),
    ("Wide gaps", (10, 14)),
];

fn roundness(index: u8) -> Roundness {
    match index {
        0 => Roundness::Round,
        1 => Roundness::SlightlyRound,
        _ => Roundness::Square,
    }
}

/// One choice in a picker.
#[derive(Clone, Debug, PartialEq)]
pub enum Pick {
    /// The part as it is on the desktop now.
    Desktop,
    /// The part as another look has it.
    Look(Selection),
    /// The look leaves this part as it is.
    Leave,
    Wallpaper(&'static str),
    Weather(&'static str),
    Accent([u8; 3]),
    /// Colors that follow the wallpaper.
    WallpaperColors,
    Mode(ModeName),
    Font(String),
    IconTheme(String),
    /// The app icon shape, keeping the icon theme and folders.
    IconShape(Shape),
    /// Folders in the accent color (or the theme's own), keeping the rest.
    FolderTint(bool),
    Corners(u8),
    Gaps(u32, u32),
}

/// A picker choice: what it is, the draft with it, and how it reads.
#[derive(Clone, Debug)]
pub struct Choice {
    pub pick: Pick,
    pub title: String,
    /// Where it comes from: "Your desktop", "Built-in look", ….
    pub source: &'static str,
    /// The draft with this choice.
    pub profile: Profile,
    pub selected: bool,
    /// The planned result, for parts drawn from the plan.
    pub preview: Option<Preview>,
    /// Icons from the choice's icon theme.
    pub icons: Vec<Sample>,
}

/// What Make built-in did.
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum Built {
    Written {
        path: PathBuf,
        removed: Vec<String>,
        fallbacks: Vec<String>,
    },
    Refused {
        message: String,
        /// The refusal is about the wallpaper: offer the wallpaper picker.
        wallpaper: bool,
    },
}

/// How a studio session starts.
#[derive(Clone, Debug, PartialEq)]
pub enum Start {
    /// From the desktop as it is now.
    Desktop,
    /// From the look on screen, as a new look.
    Selected,
    /// From a plain default look.
    Plain,
    /// Editing a saved look.
    Saved(ProfileId),
}

#[derive(Clone, Debug)]
pub enum Edit {
    Start(Start),
    /// Opens a part's picker, or closes it with `None`.
    Open(Option<Element>),
    Pick(Element, Pick),
    Query(String),
    Name(String),
    Description(String),
    Undo,
    Redo,
    Save,
    SaveAsNew,
    Duplicate,
    Rename,
    /// `false` asks (or withdraws the question); `true` deletes.
    Delete(bool),
    /// Back to the last saved state.
    Discard,
    /// Shows the parts review with Apply, or goes back to editing.
    Review(bool),
    Menu(bool),
    MakeBuiltIn,
    /// Make built-in, to a file the person chooses.
    MakeBuiltInAs,
    CopyPath,
    Close,
}

impl Edit {
    pub fn writes(&self) -> bool {
        matches!(
            self,
            Self::Save
                | Self::SaveAsNew
                | Self::Duplicate
                | Self::Rename
                | Self::Delete(true)
                | Self::MakeBuiltIn
                | Self::MakeBuiltInAs
        )
    }
}

const MAX_UNDO: usize = 64;

#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Draft {
    pub profile: Profile,
    /// The saved look this draft edits.
    saved: Option<String>,
    /// The draft as last saved or opened: Discard returns to it.
    base: Profile,
    /// The desktop as it was when the studio opened.
    desktop: Profile,
    pub picking: Option<Element>,
    pub query: String,
    undo: Vec<Profile>,
    redo: Vec<Profile>,
    pub reviewing: bool,
    pub confirm_delete: bool,
    pub menu: bool,
    pub built: Option<Built>,
}

impl Draft {
    pub(super) fn validate(&self) -> Result<(), String> {
        for profile in [&self.profile, &self.base, &self.desktop]
            .into_iter()
            .chain(&self.undo)
            .chain(&self.redo)
        {
            profile.validate().map_err(|e| e.to_string())?;
        }
        if let Some(id) = &self.saved {
            ProfileId::parse(id).map_err(|e| e.to_string())?;
        }
        Ok(())
    }
    pub fn saved(&self) -> Option<ProfileId> {
        self.saved
            .as_deref()
            .and_then(|id| ProfileId::parse(id).ok())
    }
    /// Changed since it was opened or last saved.
    pub fn dirty(&self) -> bool {
        self.profile != self.base
    }
    /// The name above differs from the saved look's name.
    pub fn renamed(&self) -> bool {
        let name = self.profile.name.trim();
        !name.is_empty() && name != self.base.name
    }
    pub fn can_undo(&self) -> bool {
        !self.undo.is_empty()
    }
    pub fn can_redo(&self) -> bool {
        !self.redo.is_empty()
    }
    pub fn desktop(&self) -> &Profile {
        &self.desktop
    }
}

/// Where Make built-in writes by default: `$XDG_DATA_HOME/halcyon/
/// built-in-looks`, or inside a fixture's own folder.
pub fn built_in_folder(store: &DesktopStore) -> Option<PathBuf> {
    match store.fixture_root() {
        Some(root) => Some(root.join(".local/share/halcyon").join(built_in::FOLDER)),
        None => built_in::default_folder(),
    }
}

impl ProfilesEditor {
    pub fn studio_choices(&self) -> &[Choice] {
        &self.studio_choices
    }
    /// Icons from the draft's icon theme, for its tile.
    pub fn studio_icons(&self) -> &[Sample] {
        &self.studio_icons
    }

    pub(super) fn studio_edit(&mut self, edit: Edit) -> Outcome {
        if edit.writes() && (self.busy() || !self.writable()) {
            if !self.writable() {
                self.notice = Some(Notice::Error("Saving is off in preview mode".into()));
            }
            return Outcome::None;
        }
        if let Edit::Start(start) = edit {
            self.start_studio(start);
            return Outcome::None;
        }
        if self.studio.is_none() {
            return Outcome::None;
        }
        // A finished save or export note gives way to the next action.
        if matches!(self.notice, Some(Notice::Done(_))) {
            self.notice = None;
        }
        match edit {
            Edit::Start(_) => {}
            Edit::Open(element) => {
                let draft = self.studio.as_mut().unwrap();
                draft.picking = element;
                draft.query.clear();
                draft.menu = false;
                draft.reviewing = false;
                self.studio_cache();
            }
            Edit::Query(query) => {
                self.studio.as_mut().unwrap().query = query.chars().take(80).collect();
                self.studio_cache();
            }
            Edit::Pick(element, pick) => self.studio_pick(element, pick),
            Edit::Name(name) => {
                let name: String = name.chars().filter(|c| !c.is_control()).take(80).collect();
                let mut profile = self.studio.as_ref().unwrap().profile.clone();
                profile.name = name;
                self.studio_text(profile);
            }
            Edit::Description(text) => {
                let text: String = text
                    .chars()
                    .filter(|c| !c.is_control())
                    .take(profiles::MAX_DESCRIPTION)
                    .collect();
                let mut profile = self.studio.as_ref().unwrap().profile.clone();
                profile.description = (!text.trim().is_empty()).then_some(text);
                self.studio_text(profile);
            }
            Edit::Undo | Edit::Redo => {
                let draft = self.studio.as_mut().unwrap();
                let (from, to) = if matches!(edit, Edit::Undo) {
                    (&mut draft.undo, &mut draft.redo)
                } else {
                    (&mut draft.redo, &mut draft.undo)
                };
                if let Some(profile) = from.pop() {
                    to.push(std::mem::replace(&mut draft.profile, profile));
                    self.studio_show();
                }
            }
            Edit::Discard => {
                let draft = self.studio.as_mut().unwrap();
                if draft.dirty() {
                    let base = draft.base.clone();
                    draft.undo.push(std::mem::replace(&mut draft.profile, base));
                    draft.redo.clear();
                    self.notice = Some(Notice::Done("Back to the last saved version".into()));
                    self.studio_show();
                }
            }
            Edit::Review(on) => {
                let draft = self.studio.as_mut().unwrap();
                draft.reviewing = on;
                draft.picking = None;
                draft.menu = false;
            }
            Edit::Menu(open) => {
                let draft = self.studio.as_mut().unwrap();
                draft.menu = open;
                draft.confirm_delete = false;
            }
            Edit::Save => self.studio_save(),
            Edit::SaveAsNew => self.studio_save_as(),
            Edit::Duplicate => self.studio_duplicate(),
            Edit::Rename => self.studio_rename(),
            Edit::Delete(false) => {
                let draft = self.studio.as_mut().unwrap();
                draft.confirm_delete = !draft.confirm_delete && draft.saved.is_some();
                draft.menu = false;
            }
            Edit::Delete(true) => self.studio_delete(),
            Edit::MakeBuiltIn => self.make_built_in(),
            Edit::MakeBuiltInAs => return self.make_built_in_as(),
            Edit::CopyPath => {
                if let Some(Built::Written { path, .. }) = &self.studio.as_ref().unwrap().built {
                    return Outcome::Copy(path.display().to_string());
                }
            }
            Edit::Close => {
                self.forget_review();
                self.select(Selection::Current);
            }
        }
        Outcome::None
    }

    fn start_studio(&mut self, start: Start) {
        let Some(store) = self.store() else {
            return;
        };
        let captured = match profiles::capture(store, "Your desktop") {
            Ok(captured) => captured,
            Err(error) => {
                self.notice = Some(Notice::failed(&error));
                return;
            }
        };
        let mut notes = Vec::new();
        let (mut profile, saved) = match start {
            Start::Desktop => {
                notes = captured.warnings.clone();
                let mut profile = captured.profile.clone();
                profile.name = "My look".into();
                (profile, None)
            }
            Start::Selected => match self.profile().cloned() {
                Some(mut profile) => {
                    // A curated look opens as a copy: the original stays.
                    if let Selection::Preset(id) = &self.selection
                        && profile.description.is_none()
                        && let Some(preset) = self.preset_for(id)
                        && !preset.description.is_empty()
                    {
                        profile.description = Some(preset.description.into());
                    }
                    let copy = format!("{} copy", profile.name);
                    if copy.chars().count() <= 80 {
                        profile.name = copy;
                    }
                    profile.package = None;
                    profile.appearance_only = false;
                    (profile, None)
                }
                None => {
                    notes = captured.warnings.clone();
                    let mut profile = captured.profile.clone();
                    profile.name = "My look".into();
                    (profile, None)
                }
            },
            Start::Plain => {
                let mut profile = self
                    .preset_for(profiles::CLEAN)
                    .map(|preset| preset.profile.clone())
                    .unwrap_or_else(blank);
                profile.name = "New look".into();
                profile.description = None;
                (profile, None)
            }
            Start::Saved(id) => match ProfileLibrary::new(store).load(&id) {
                Ok(profile) => (profile, Some(id.as_str().to_string())),
                Err(error) => {
                    self.notice = Some(Notice::failed(&error));
                    return;
                }
            },
        };
        profile.version = profiles::VERSION;
        profile.composition = None;
        if let Some(description) = &profile.description
            && !profiles::valid_description(description)
        {
            profile.description = None;
        }
        self.select(Selection::Current);
        // The studio keeps its own undo; earlier review steps don't apply.
        self.steps.clear();
        self.touched = true;
        self.notes = notes;
        self.studio = Some(Draft {
            base: profile.clone(),
            profile,
            saved,
            desktop: captured.profile,
            picking: None,
            query: String::new(),
            undo: Vec::new(),
            redo: Vec::new(),
            reviewing: false,
            confirm_delete: false,
            menu: false,
            built: None,
        });
        self.studio_show();
    }

    /// Shows the draft: plans it and refreshes the pickers.
    pub(super) fn studio_show(&mut self) {
        let Some(draft) = &self.studio else {
            return;
        };
        self.theme_override = Some(draft.profile.clone());
        self.excluded.clear();
        self.theme_pending = None;
        self.theme_source = None;
        self.theme_error = None;
        self.replan();
        self.queue_theme_colors();
        self.want_backdrop();
        self.studio_cache();
    }

    /// A new draft state from a choice: validated, then shown.
    fn studio_commit(&mut self, profile: Profile) {
        if let Err(error) = profile.validate() {
            self.notice = Some(Notice::Error(error.to_string()));
            return;
        }
        let Some(draft) = &mut self.studio else {
            return;
        };
        if draft.profile == profile {
            return;
        }
        draft
            .undo
            .push(std::mem::replace(&mut draft.profile, profile));
        if draft.undo.len() > MAX_UNDO {
            draft.undo.remove(0);
        }
        draft.redo.clear();
        draft.built = None;
        self.notice = None;
        self.touched = true;
        self.studio_show();
    }

    /// Name and description: no new plan, no undo step per key.
    fn studio_text(&mut self, profile: Profile) {
        let Some(draft) = &mut self.studio else {
            return;
        };
        draft.profile = profile;
        draft.built = None;
        self.theme_override = Some(draft.profile.clone());
        self.touched = true;
    }

    fn studio_pick(&mut self, element: Element, pick: Pick) {
        let Some(profile) = self.picked(element, &pick) else {
            self.notice = Some(Notice::Error("This choice is no longer available".into()));
            return;
        };
        self.studio_commit(profile);
    }

    /// The draft with `pick` for `element`.
    pub(super) fn picked(&self, element: Element, pick: &Pick) -> Option<Profile> {
        let draft = self.studio.as_ref()?;
        let mut profile = draft.profile.clone();
        match pick {
            Pick::Desktop => element.copy(&draft.desktop, &mut profile),
            Pick::Look(selection) => {
                let source = self.look(selection)?;
                if !element.has(source) {
                    return None;
                }
                element.copy(source, &mut profile);
            }
            Pick::Leave => element.copy(&blank(), &mut profile),
            Pick::Wallpaper(name) => {
                crate::desktop::wallpaper::bundled_name(name)?;
                let look: profiles::WallpaperLook = serde_json::from_value(json!({
                    "background": {"source": {"bundled": name}, "fit": "fill"}
                }))
                .ok()?;
                profile.wallpaper = Some(look);
            }
            Pick::Weather(id) => {
                let style = profile.widget_style.get_or_insert_with(|| json!({}));
                widget::put(style, "/rain", Some(weather(id)?)).ok()?;
            }
            Pick::Accent(rgb) => {
                let style = profile.widget_style.get_or_insert_with(|| json!({}));
                let _ = widget::put(style, "/appearance/wallpaper_colors", Some(json!(false)));
                let _ = widget::put(style, "/appearance/color_policy", Some(json!("custom")));
                self.recolor_preview(&mut profile, Some(*rgb));
            }
            Pick::WallpaperColors => {
                let style = profile.widget_style.get_or_insert_with(|| json!({}));
                let _ = widget::put(style, "/appearance/wallpaper_colors", Some(json!(true)));
                let _ = widget::put(style, "/appearance/color_policy", Some(json!("wallpaper")));
            }
            Pick::Mode(mode) => {
                profile.appearance.get_or_insert_with(Default::default).mode = Some(*mode);
            }
            Pick::Font(name) => {
                let mut font = cosmic::config::CosmicTk::default().interface_font;
                font.family = name.clone();
                profile
                    .appearance
                    .get_or_insert_with(Default::default)
                    .interface_font = Some(font);
            }
            Pick::IconTheme(id) => {
                profile
                    .appearance
                    .get_or_insert_with(Default::default)
                    .icon_theme = Some(id.clone());
            }
            Pick::IconShape(shape) => {
                profile
                    .appearance
                    .get_or_insert_with(Default::default)
                    .icon_shape = Some(*shape);
            }
            Pick::FolderTint(tint) => {
                profile
                    .appearance
                    .get_or_insert_with(Default::default)
                    .folder_tint = Some(*tint);
            }
            Pick::Corners(index) => {
                let appearance = profile.appearance.get_or_insert_with(Default::default);
                for variant in [&mut appearance.light, &mut appearance.dark] {
                    variant.get_or_insert_with(Default::default).corners =
                        Some(Corners::Preset(roundness(*index)));
                }
            }
            Pick::Gaps(outer, inner) => {
                let appearance = profile.appearance.get_or_insert_with(Default::default);
                for variant in [&mut appearance.light, &mut appearance.dark] {
                    variant.get_or_insert_with(Default::default).gaps = Some((*outer, *inner));
                }
            }
        }
        tidy(&mut profile);
        Some(profile)
    }

    /// A built-in or saved look by its selection.
    fn look(&self, selection: &Selection) -> Option<&Profile> {
        match selection {
            Selection::Current => self.studio.as_ref().map(|draft| &draft.desktop),
            Selection::Preset(id) => self.preset_for(id).map(|preset| &preset.profile),
            Selection::Saved(id) => self
                .library()
                .ok()?
                .iter()
                .find(|entry| entry.id == *id)?
                .profile
                .as_ref()
                .ok(),
        }
    }

    /// The looks a picker offers parts from: built-in looks and styles,
    /// then saved looks.
    fn studio_sources(&self) -> Vec<(Selection, &Profile, &'static str)> {
        let mut sources: Vec<(Selection, &Profile, &'static str)> = self
            .presets
            .iter()
            .filter(|preset| !preset.id.starts_with("adaptive-"))
            .map(|preset| {
                (
                    Selection::Preset(preset.id),
                    &preset.profile,
                    if preset.profile.appearance_only {
                        "Built-in style"
                    } else {
                        "Built-in look"
                    },
                )
            })
            .collect();
        if let Ok(entries) = self.library() {
            sources.extend(entries.iter().filter_map(|entry| {
                let profile = entry.profile.as_ref().ok()?;
                Some((Selection::Saved(entry.id.clone()), profile, "Saved look"))
            }));
        }
        sources
    }

    /// The open picker's choices, each with what it needs drawn.
    pub(super) fn studio_cache(&mut self) {
        self.studio_choices.clear();
        self.studio_icons = self
            .studio
            .as_ref()
            .and_then(|draft| self.icon_style_of(&draft.profile).base)
            .map(|theme| icon_art::samples(&theme))
            .unwrap_or_default();
        if let Some(draft) = &self.studio {
            let style = self.icon_style_of(&draft.profile);
            let keys = self.icon_keys(&self.studio_icons.clone(), &style);
            self.want_icon_art(keys);
        }
        let Some(draft) = &self.studio else {
            return;
        };
        let Some(element) = draft.picking else {
            return;
        };
        let query = draft.query.trim().to_lowercase();
        let mut picks: Vec<(Pick, String, &'static str)> =
            vec![(Pick::Desktop, "From my desktop".into(), "Your desktop")];
        if element.leavable() {
            picks.push((Pick::Leave, "Leave as it is".into(), "Not part of the look"));
        }
        match element {
            Element::Wallpaper => picks.extend(crate::desktop::wallpaper::BUNDLED.iter().map(
                |(name, title)| (Pick::Wallpaper(name), (*title).into(), "Halcyon wallpaper"),
            )),
            Element::Weather => picks.extend(
                WEATHER
                    .iter()
                    .map(|(id, title)| (Pick::Weather(id), (*title).into(), "Effect")),
            ),
            Element::Colors => {
                picks.push((Pick::WallpaperColors, "From the wallpaper".into(), "Colors"));
                picks.extend(
                    ACCENTS
                        .iter()
                        .map(|(title, rgb)| (Pick::Accent(*rgb), (*title).into(), "Accent")),
                );
                picks.push((Pick::Mode(ModeName::Light), "Light".into(), "Mode"));
                picks.push((Pick::Mode(ModeName::Dark), "Dark".into(), "Mode"));
                picks.push((
                    Pick::Mode(ModeName::Automatic),
                    "Light and dark".into(),
                    "Mode",
                ));
            }
            Element::Windows => {
                picks.extend(
                    CORNERS
                        .iter()
                        .map(|(title, index)| (Pick::Corners(*index), (*title).into(), "Corners")),
                );
                picks.extend(
                    GAPS.iter()
                        .map(|(title, (o, i))| (Pick::Gaps(*o, *i), (*title).into(), "Gaps")),
                );
            }
            Element::Font => {
                // Fonts every install has first, then the rest by name.
                let families = self.installed.fonts.families(false);
                let mut names: Vec<&str> = built_in::DEFAULT_FONTS
                    .iter()
                    .copied()
                    .filter(|name| families.iter().any(|f| f.name == *name) || families.is_empty())
                    .collect();
                names.extend(
                    families
                        .iter()
                        .map(|family| family.name.as_str())
                        .filter(|name| !built_in::DEFAULT_FONTS.contains(name)),
                );
                picks.extend(
                    names
                        .into_iter()
                        .map(|name| (Pick::Font(name.into()), name.into(), "Font")),
                );
            }
            Element::Icons => {
                let themes = &self.installed.icons.themes;
                let mut list: Vec<(String, String)> = built_in::DEFAULT_ICON_THEMES
                    .iter()
                    .filter(|id| themes.is_empty() || themes.iter().any(|t| t.id == **id))
                    .map(|id| (id.to_string(), id.to_string()))
                    .collect();
                // Halcyon's shaped overlay is never a choice: a look names
                // its base theme and the shape and tint below.
                list.extend(
                    themes
                        .iter()
                        .filter(|t| !built_in::DEFAULT_ICON_THEMES.contains(&t.id.as_str()))
                        .filter(|t| !overlay::is_overlay(&t.id))
                        .map(|t| (t.id.clone(), t.name.clone())),
                );
                picks.extend(
                    list.into_iter()
                        .map(|(id, name)| (Pick::IconTheme(id), name, "Icon theme")),
                );
                picks.extend(Shape::ALL.into_iter().map(|shape| {
                    (
                        Pick::IconShape(shape),
                        shape.title().into(),
                        "App icon shape",
                    )
                }));
                picks.push((Pick::FolderTint(true), "Accent folders".into(), "Folders"));
                picks.push((Pick::FolderTint(false), "Theme folders".into(), "Folders"));
            }
            _ => {}
        }
        picks.extend(
            self.studio_sources()
                .into_iter()
                .filter(|(_, profile, _)| element.has(profile))
                .map(|(selection, profile, source)| {
                    (Pick::Look(selection), profile.name.clone(), source)
                }),
        );
        let mut choices: Vec<Choice> = Vec::new();
        for (pick, title, source) in picks {
            if !query.is_empty()
                && !matches!(pick, Pick::Desktop | Pick::Leave)
                && !title.to_lowercase().contains(&query)
            {
                continue;
            }
            let Some(profile) = self.picked(element, &pick) else {
                continue;
            };
            // One tile per distinct result; the first name wins. Shapes
            // and folder tints always show, so each row stays whole.
            if !matches!(
                pick,
                Pick::Desktop | Pick::Leave | Pick::IconShape(_) | Pick::FolderTint(_)
            ) && choices
                .iter()
                .any(|choice| element.same(&choice.profile, &profile))
            {
                continue;
            }
            let selected = match &pick {
                // The draft's own shape or tint, as the tile shows it.
                Pick::IconShape(shape) => self.icon_style_of(&draft.profile).shape == *shape,
                Pick::FolderTint(tint) => self.icon_style_of(&draft.profile).tint == *tint,
                _ => element.same(&profile, &draft.profile),
            };
            choices.push(Choice {
                selected,
                pick,
                title,
                source,
                profile,
                preview: None,
                icons: Vec::new(),
            });
        }
        if let Some(store) = self.store() {
            let domains = element.domains();
            for choice in &mut choices {
                match element {
                    Element::Wallpaper | Element::Weather | Element::Font => {}
                    Element::Icons => {
                        choice.icons = self
                            .icon_style_of(&choice.profile)
                            .base
                            .map(|theme| icon_art::samples(&theme))
                            .unwrap_or_default();
                    }
                    _ => {
                        let selected: BTreeSet<Domain> = choice
                            .profile
                            .domains()
                            .into_iter()
                            .filter(|domain| domains.contains(domain))
                            .collect();
                        let plan =
                            profiles::plan(store, &self.installed, &choice.profile, &selected);
                        choice.preview = Some(preview(store, Some(&plan)));
                    }
                }
            }
        }
        let keys: Vec<_> = choices
            .iter()
            .flat_map(|choice| self.icon_keys(&choice.icons, &self.icon_style_of(&choice.profile)))
            .collect();
        self.want_icon_art(keys);
        self.studio_choices = choices;
    }

    /// The icons `profile` shows: its base theme, shape and tint, with what
    /// it leaves as is taken from the desktop.
    pub fn icon_style_of(&self, profile: &Profile) -> Style {
        let desktop = self
            .studio
            .as_ref()
            .and_then(|draft| draft.desktop.appearance.as_ref());
        let look = profile.appearance.as_ref();
        let base = look
            .and_then(|a| a.icon_theme.clone())
            .or_else(|| desktop.and_then(|a| a.icon_theme.clone()))
            .or_else(|| {
                self.preview
                    .as_ref()
                    .and_then(|p| p.icons_now.as_ref())
                    .and_then(|style| style.base.clone())
            });
        // What the desktop shows now, for what neither sets.
        let shown = self.preview.as_ref().and_then(|p| p.icons_now.clone());
        Style {
            base,
            shape: look
                .and_then(|a| a.icon_shape)
                .or_else(|| desktop.and_then(|a| a.icon_shape))
                .or(shown.as_ref().map(|style| style.shape))
                .unwrap_or_default(),
            tint: look
                .and_then(|a| a.folder_tint)
                .or_else(|| desktop.and_then(|a| a.folder_tint))
                .or(shown.as_ref().map(|style| style.tint))
                .unwrap_or_default(),
        }
    }

    /// The accent and shade sample icons are drawn with: the preview's.
    fn icon_shade(&self) -> ([u8; 3], bool) {
        let appearance = self.preview.as_ref().and_then(|p| p.appearance.as_ref());
        let dark = appearance.is_none_or(|draft| {
            draft.active_variant() == crate::desktop::appearance::ThemeVariant::Dark
        });
        (icon_art::accent_of(appearance), dark)
    }

    fn icon_keys(&self, samples: &[Sample], style: &Style) -> Vec<icon_art::Key> {
        let (accent, dark) = self.icon_shade();
        samples
            .iter()
            .filter_map(|sample| icon_art::Key::new(sample, style, accent, dark))
            .collect()
    }

    /// The pictures for `samples` under `style`: drawn ones once ready, the
    /// plain file until then.
    pub fn icon_pictures(&self, samples: &[Sample], style: &Style) -> Vec<icon_art::Picture> {
        let (accent, dark) = self.icon_shade();
        samples
            .iter()
            .map(|sample| {
                icon_art::Key::new(sample, style, accent, dark)
                    .and_then(|key| self.icon_art(&key).cloned())
                    .map(icon_art::Picture::Drawn)
                    .unwrap_or_else(|| icon_art::Picture::File(sample.path.clone()))
            })
            .collect()
    }

    fn studio_save(&mut self) {
        let Some(draft) = self.studio.clone() else {
            return;
        };
        let Some(id) = draft.saved() else {
            return self.studio_save_as();
        };
        let Some(store) = self.store() else {
            return;
        };
        if let Err(error) = studio_ready(&draft.profile) {
            self.notice = Some(Notice::Error(error));
            return;
        }
        match ProfileLibrary::new(store).save(&id, &draft.profile) {
            Ok(()) => {
                self.reload_library();
                let draft = self.studio.as_mut().unwrap();
                draft.base = draft.profile.clone();
                self.notice = Some(Notice::Done(format!("Saved “{}”", draft.profile.name)));
            }
            Err(error) => self.notice = Some(Notice::failed(&error)),
        }
    }

    fn studio_save_as(&mut self) {
        let Some(draft) = self.studio.clone() else {
            return;
        };
        let Some(store) = self.store() else {
            return;
        };
        if let Err(error) = studio_ready(&draft.profile) {
            self.notice = Some(Notice::Error(error));
            return;
        }
        match ProfileLibrary::new(store).create(&draft.profile) {
            Ok(id) => {
                self.reload_library();
                let draft = self.studio.as_mut().unwrap();
                draft.saved = Some(id.as_str().into());
                draft.base = draft.profile.clone();
                self.notice = Some(Notice::Done(format!(
                    "Saved “{}” to your looks",
                    draft.profile.name
                )));
            }
            Err(error) => self.notice = Some(Notice::failed(&error)),
        }
    }

    /// A copy of the saved look under a new name, opened in the studio.
    /// Unsaved edits stay with the copy.
    fn studio_duplicate(&mut self) {
        let Some(draft) = self.studio.clone() else {
            return;
        };
        let Some(store) = self.store() else {
            return;
        };
        let mut copy = draft.profile.clone();
        let name = format!("{} copy", copy.name.trim());
        if name.chars().count() <= 80 {
            copy.name = name;
        }
        if let Err(error) = studio_ready(&copy) {
            self.notice = Some(Notice::Error(error));
            return;
        }
        match ProfileLibrary::new(store).create(&copy) {
            Ok(id) => {
                self.reload_library();
                let draft = self.studio.as_mut().unwrap();
                draft.menu = false;
                draft.saved = Some(id.as_str().into());
                draft
                    .undo
                    .push(std::mem::replace(&mut draft.profile, copy.clone()));
                draft.redo.clear();
                draft.base = copy;
                self.theme_override = Some(draft.profile.clone());
                self.notice = Some(Notice::Done(format!(
                    "Saved “{}”. You’re editing the copy.",
                    draft.profile.name
                )));
            }
            Err(error) => self.notice = Some(Notice::failed(&error)),
        }
    }

    /// Renames the saved look to the name above; other edits stay unsaved.
    fn studio_rename(&mut self) {
        let Some(draft) = self.studio.clone() else {
            return;
        };
        let (Some(id), Some(store)) = (draft.saved(), self.store()) else {
            return;
        };
        let name = draft.profile.name.trim().to_string();
        match ProfileLibrary::new(store).rename(&id, &name) {
            Ok(_) => {
                self.reload_library();
                let draft = self.studio.as_mut().unwrap();
                draft.menu = false;
                draft.profile.name = name.clone();
                draft.base.name = name.clone();
                self.theme_override = Some(draft.profile.clone());
                self.notice = Some(Notice::Done(format!("Renamed to “{name}”")));
            }
            Err(error) => self.notice = Some(Notice::failed(&error)),
        }
    }

    /// How many automation rules apply the look being edited.
    pub(in crate::settings) fn studio_rules_using(&self) -> usize {
        let (Some(store), Some(id)) = (self.store(), self.studio.as_ref().and_then(Draft::saved))
        else {
            return 0;
        };
        let target = crate::automation::ProfileRef::Saved(id.as_str().into());
        crate::automation::load(store).map_or(0, |settings| {
            settings
                .rules
                .iter()
                .filter(|rule| rule.profile == target)
                .count()
        })
    }

    fn studio_delete(&mut self) {
        let Some(draft) = self.studio.clone() else {
            return;
        };
        if !draft.confirm_delete {
            return;
        }
        self.studio.as_mut().unwrap().confirm_delete = false;
        let (Some(id), Some(store)) = (draft.saved(), self.store()) else {
            return;
        };
        match ProfileLibrary::new(store).delete(&id) {
            Ok(()) => {
                self.reload_library();
                self.forget_review();
                self.select(Selection::Current);
                self.notice = Some(Notice::Done(format!("Deleted “{}”", draft.base.name)));
            }
            Err(error) => self.notice = Some(Notice::failed(&error)),
        }
    }

    /// The draft as a built-in look, or why it can't be one.
    fn built_in_bytes(&mut self) -> Option<built_in::BuiltIn> {
        let mut profile = self.studio.as_ref()?.profile.clone();
        // A base icon theme, never the overlay only this computer has.
        if let Some(store) = self.store() {
            profiles::resolve_icon_theme_in(&mut profile, store);
        }
        let draft = self.studio.as_mut()?;
        draft.menu = false;
        let id = built_in::id_for(&draft.profile.name);
        match built_in::prepare(&profile, &id) {
            Ok(built) => Some(built),
            Err(refusal) => {
                draft.built = Some(Built::Refused {
                    message: refusal.message(),
                    wallpaper: matches!(
                        refusal,
                        built_in::Refusal::PersonalWallpaper | built_in::Refusal::NoWallpaper
                    ),
                });
                None
            }
        }
    }

    fn make_built_in(&mut self) {
        let Some(built) = self.built_in_bytes() else {
            return;
        };
        let Some(store) = self.store() else {
            return;
        };
        let Some(folder) = built_in_folder(store) else {
            self.notice = Some(Notice::Error(
                "There’s no folder for built-in looks. Choose a file instead.".into(),
            ));
            return;
        };
        let path = folder.join(format!("{}.json", built.id));
        let written = fs::create_dir_all(&folder)
            .map_err(|error| format!("Could not create {}: {error}", folder.display()))
            .and_then(|()| write_export(&path, &built.bytes, store.fixture_root()));
        match written {
            Ok(()) => self.built_written(path, &built.removed, built.fallbacks),
            Err(error) => self.notice = Some(Notice::Error(error)),
        }
    }

    fn built_written(&mut self, path: PathBuf, removed: &[&str], fallbacks: Vec<String>) {
        if let Some(draft) = &mut self.studio {
            draft.built = Some(Built::Written {
                path,
                removed: removed.iter().map(|item| item.to_string()).collect(),
                fallbacks,
            });
        }
    }

    fn make_built_in_as(&mut self) -> Outcome {
        if self.pending.is_some() {
            return Outcome::None;
        }
        let Some(built) = self.built_in_bytes() else {
            return Outcome::None;
        };
        let Some(store) = self.store() else {
            return Outcome::None;
        };
        let confine = store.fixture_root().map(Path::to_path_buf);
        let id = self.request(Pending::BuiltIn(
            built.removed.clone(),
            built.fallbacks.clone(),
        ));
        Outcome::Dialog(Request::Export {
            id,
            file_name: format!("{}.json", built.id),
            bytes: built.bytes,
            confine,
        })
    }

    /// A chooser for Make built-in finished.
    pub(super) fn built_in_exported(
        &mut self,
        removed: Vec<&'static str>,
        fallbacks: Vec<String>,
        result: Result<Option<PathBuf>, String>,
    ) {
        match result {
            Ok(Some(path)) => self.built_written(path, &removed, fallbacks),
            Ok(None) => {}
            Err(error) => self.notice = Some(Notice::Error(error)),
        }
    }
}

/// Whether the draft can be stored: a valid name and record with a part.
fn studio_ready(profile: &Profile) -> Result<(), String> {
    if Profile::new(profile.name.trim()).is_none() {
        return Err("Give the look a name: 1 to 80 characters, on one line.".into());
    }
    if profile.domains().is_empty() {
        return Err("Choose at least one part for this look.".into());
    }
    profile.validate().map_err(|error| error.to_string())
}
