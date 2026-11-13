//! The Edge menu page: the strip on the screen edge, how the menu opens,
//! and the list of items it shows. Every change is saved at once and the
//! desktop picks it up live; the preview is the desktop's own menu.
use super::reorder::Reorder;
use super::*;
use crate::automation::ProfileRef;
use crate::config::edge_menu::{
    Close, Displays, EdgeMenu, Frame, HandleColor, IconSize, Item, Kind, MAX_ITEMS, OpenOn, Side,
    Speed, Style,
};
use crate::desktop::apps::{AppCatalog, AppInfo};

#[derive(Debug, Clone)]
pub enum Edit {
    Enabled(bool),
    Side(Side),
    /// Top, middle or bottom; a slider sets any place between.
    Position(u8),
    Open(OpenOn),
    Style(Style),
    IconSize(IconSize),
    Columns(u8),
    Labels(bool),
    SectionTitles(bool),
    Frame(Frame),
    Handle(bool),
    HandleColor(HandleColor),
    Close(Close),
    Speed(Speed),
    Displays(Displays),
    /// A display, by its saved name, chosen or not for `Displays::Chosen`.
    Output(String, bool),
    /// A slider moving; saved when it is let go (`Commit`).
    Draft(Field, u32),
    Commit,
    Add(Item),
    Remove(usize),
    MoveUp(usize),
    MoveDown(usize),
    /// Typing a section title; saved with Enter or the next change.
    TitleText(usize, String),
    Reset,
    /// The Add picker open or closed.
    Picker(bool),
    AppSearch(String),
    ChooseFolder,
    FolderChosen(Result<Option<std::path::PathBuf>, String>),
    DragStart(usize),
    DragOver(usize),
    DragLeave(usize),
    DragDrop,
    DragCancel,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Field {
    Position,
    Length,
    Delay,
    Opacity,
    Roundness,
    HandleSize,
    HandleOpacity,
}
impl Field {
    pub fn key(self) -> &'static str {
        match self {
            Self::Position => "position",
            Self::Length => "length",
            Self::Delay => "delay_ms",
            Self::Opacity => "opacity",
            Self::Roundness => "roundness",
            Self::HandleSize => "handle_size",
            Self::HandleOpacity => "handle_opacity",
        }
    }
}

pub struct EdgeMenuPage {
    /// A slider's value while it moves.
    pub draft: Option<(Field, u32)>,
    /// A section title being typed: its item and text.
    pub title: Option<(usize, String)>,
    pub picker: bool,
    pub search: String,
    /// Installed apps, read when the picker first opens.
    pub apps: Option<Vec<AppInfo>>,
    pub looks: Option<Vec<(ProfileRef, String)>>,
    /// Dragging a row of the list, by its place.
    pub reorder: Reorder<usize>,
    pub folder_pending: bool,
    /// Whether each item can run here (see `crate::app::edge`).
    pub available: Vec<bool>,
}
impl Default for EdgeMenuPage {
    fn default() -> Self {
        Self {
            draft: None,
            title: None,
            picker: false,
            search: String::new(),
            apps: None,
            looks: None,
            reorder: Reorder::new(),
            folder_pending: false,
            available: Vec::new(),
        }
    }
}

/// Installed apps; a desktop fixture's own apps in place of the system's.
fn catalog() -> AppCatalog {
    let fixture = crate::desktop::DesktopStore::from_env()
        .ok()
        .and_then(|store| store.fixture_root().map(|root| root.join("applications")));
    match fixture {
        Some(dir) => AppCatalog::discover(&[dir]),
        None => AppCatalog::installed(),
    }
}

/// Every Look a menu item can switch to: built in, then saved.
fn looks() -> Vec<(ProfileRef, String)> {
    let mut looks: Vec<(ProfileRef, String)> = crate::desktop::profiles::presets()
        .into_iter()
        .chain(crate::desktop::profiles::adaptive::presets())
        .map(|preset| (ProfileRef::Preset(preset.id.into()), preset.profile.name))
        .collect();
    if let Ok(store) = crate::desktop::DesktopStore::from_env()
        && let Ok(entries) = crate::desktop::profiles::ProfileLibrary::new(&store).list()
    {
        looks.extend(entries.into_iter().filter_map(|entry| {
            Some((
                ProfileRef::Saved(entry.id.as_str().into()),
                entry.profile.ok()?.name,
            ))
        }));
    }
    looks
}

/// The folders offered before "Choose a folder": home and the usual ones.
pub(super) fn common_folders() -> Vec<(String, std::path::PathBuf)> {
    [
        ("Home", dirs::home_dir()),
        ("Documents", dirs::document_dir()),
        ("Downloads", dirs::download_dir()),
        ("Pictures", dirs::picture_dir()),
        ("Music", dirs::audio_dir()),
        ("Videos", dirs::video_dir()),
    ]
    .into_iter()
    .filter_map(|(name, path)| Some((name.to_string(), path?)))
    .collect()
}

impl EdgeMenuPage {
    pub fn open(&mut self, config: &Config) {
        self.refresh(config);
    }
    /// Which items can run: re-read when the list changes.
    pub fn refresh(&mut self, config: &Config) {
        self.available = match &config.edge_menu.items {
            Some(items) => crate::app::edge_availability(items, catalog),
            None => Vec::new(),
        };
    }
    pub fn load_picker(&mut self) {
        if self.apps.is_none() {
            self.apps = Some(catalog().sorted().into_iter().cloned().collect());
        }
        if self.looks.is_none() {
            self.looks = Some(looks());
        }
    }
    /// The apps matching the search, by name.
    pub fn matching_apps(&self) -> Vec<&AppInfo> {
        let query = self.search.trim().to_lowercase();
        self.apps
            .iter()
            .flatten()
            .filter(|app| {
                query.is_empty()
                    || app.name.to_lowercase().contains(&query)
                    || app.id.to_lowercase().contains(&query)
            })
            .collect()
    }
    pub fn value(&self, menu: &EdgeMenu, field: Field) -> u32 {
        match self.draft {
            Some((draft, value)) if draft == field => value,
            _ => match field {
                Field::Position => menu.position.into(),
                Field::Length => menu.length.into(),
                Field::Delay => menu.delay_ms,
                Field::Opacity => menu.opacity.into(),
                Field::Roundness => menu.roundness.into(),
                Field::HandleSize => menu.handle_size.into(),
                Field::HandleOpacity => menu.handle_opacity.into(),
            },
        }
    }
}

/// Whether `item` is already in `items`: widgets and actions are listed
/// once.
pub(super) fn listed(items: &[Item], item: &Item) -> bool {
    match &item.kind {
        Kind::Widget { .. } | Kind::Action { .. } | Kind::Look { .. } | Kind::App { .. } => {
            items.iter().any(|other| other.kind == item.kind)
        }
        Kind::Folder { path, .. } => items
            .iter()
            .any(|other| matches!(&other.kind, Kind::Folder { path: p, .. } if p == path)),
        _ => false,
    }
}

impl Settings {
    pub(super) fn edge_menu_update(&mut self, edit: Edit) -> Task<Message> {
        // A typed title is kept before anything else changes the list.
        if !matches!(edit, Edit::TitleText(..)) {
            self.commit_edge_title();
        }
        match edit {
            Edit::Enabled(on) => self.save_edge(json!({ "enabled": on })),
            Edit::Side(side) => self.save_edge(json!({ "side": side })),
            Edit::Position(position) => {
                self.edge_menu.draft = None;
                self.save_edge(json!({ "position": position.min(100) }));
            }
            Edit::Open(open) => self.save_edge(json!({ "open": open })),
            Edit::Style(style) => self.save_edge(json!({ "style": style })),
            Edit::IconSize(size) => self.save_edge(json!({ "icon_size": size })),
            Edit::Columns(columns) => self.save_edge(json!({ "columns": columns.clamp(
                crate::config::edge_menu::MIN_COLUMNS,
                crate::config::edge_menu::MAX_COLUMNS,
            ) })),
            Edit::Labels(on) => self.save_edge(json!({ "labels": on })),
            Edit::SectionTitles(on) => self.save_edge(json!({ "section_titles": on })),
            Edit::Frame(frame) => self.save_edge(json!({ "frame": frame })),
            Edit::Handle(on) => self.save_edge(json!({ "handle": on })),
            Edit::HandleColor(color) => self.save_edge(json!({ "handle_color": color })),
            Edit::Close(close) => self.save_edge(json!({ "close": close })),
            Edit::Speed(speed) => self.save_edge(json!({ "speed": speed })),
            Edit::Displays(displays) => self.save_edge(json!({ "displays": displays })),
            Edit::Output(key, on) => {
                let mut outputs = self.config.edge_menu.outputs.clone();
                outputs.retain(|output| *output != key);
                if on {
                    outputs.push(key);
                }
                self.save_edge(json!({ "outputs": outputs }));
            }
            Edit::Draft(field, value) => self.edge_menu.draft = Some((field, value)),
            Edit::Commit => {
                if let Some((field, value)) = self.edge_menu.draft.take() {
                    self.save_edge(json!({ field.key(): value }));
                }
            }
            Edit::Add(item) => {
                let mut items = self.config.edge_items();
                if items.len() >= MAX_ITEMS {
                    self.failed(format!("The edge menu holds at most {MAX_ITEMS} items"));
                } else if !listed(&items, &item) {
                    items.push(item);
                    self.save_edge_items(items);
                }
            }
            Edit::Remove(index) => {
                let mut items = self.config.edge_items();
                if index < items.len() {
                    items.remove(index);
                    self.save_edge_items(items);
                }
            }
            Edit::MoveUp(index) | Edit::MoveDown(index) => {
                let up = matches!(edit, Edit::MoveUp(_));
                let mut items = self.config.edge_items();
                let to = if up {
                    index.checked_sub(1)
                } else {
                    (index + 1 < items.len()).then_some(index + 1)
                };
                if let Some(to) = to {
                    items.swap(index, to);
                    self.save_edge_items(items);
                    // The keyboard stays with the item it moved.
                    let button = if up { "up" } else { "down" };
                    return cosmic::iced::widget::operation::focus(cosmic::widget::Id::new(
                        format!("edge-row-{to}-{button}"),
                    ));
                }
            }
            Edit::TitleText(index, text) => {
                self.edge_menu.title = Some((index, text.chars().take(40).collect()));
            }
            Edit::Reset => {
                self.edge_menu.reorder.cancel();
                self.save_edge(json!({ "items": null }));
            }
            Edit::Picker(open) => {
                self.edge_menu.picker = open;
                if open {
                    self.edge_menu.load_picker();
                }
            }
            Edit::AppSearch(text) => self.edge_menu.search = text,
            Edit::ChooseFolder => {
                if self.edge_menu.folder_pending {
                    return Task::none();
                }
                self.edge_menu.folder_pending = true;
                return cosmic::task::future(async {
                    use cosmic::dialog::file_chooser::{self, open::Dialog};
                    let result = match Dialog::new()
                        .title("Choose a folder for the edge menu")
                        .open_folder()
                        .await
                    {
                        Ok(response) => response
                            .url()
                            .to_file_path()
                            .map(Some)
                            .map_err(|_| "Choose a local folder".into()),
                        Err(file_chooser::Error::Cancelled) => Ok(None),
                        Err(error) => Err(format!("Could not open the folder chooser: {error}")),
                    };
                    Message::EdgeMenu(Edit::FolderChosen(result))
                });
            }
            Edit::FolderChosen(result) => {
                self.edge_menu.folder_pending = false;
                match result {
                    Ok(Some(path)) => return self.edge_menu_update(Edit::Add(folder_item(&path))),
                    Ok(None) => {}
                    Err(error) => self.failed(error),
                }
            }
            Edit::DragStart(index) => self.edge_menu.reorder.start(index),
            Edit::DragOver(index) => self.edge_menu.reorder.over(index),
            Edit::DragLeave(index) => self.edge_menu.reorder.leave(index),
            Edit::DragCancel => self.edge_menu.reorder.cancel(),
            Edit::DragDrop => {
                let items = self.config.edge_items();
                let mut order: Vec<usize> = (0..items.len()).collect();
                if self.edge_menu.reorder.finish(&mut order) {
                    let items = order.into_iter().map(|at| items[at].clone()).collect();
                    self.save_edge_items(items);
                }
            }
        }
        Task::none()
    }
    fn commit_edge_title(&mut self) {
        let Some((index, text)) = self.edge_menu.title.take() else {
            return;
        };
        let mut items = self.config.edge_items();
        if let Some(Kind::Title { text: old }) = items.get_mut(index).map(|item| &mut item.kind)
            && *old != text
        {
            *old = text;
            self.save_edge_items(items);
        }
    }
    fn save_edge_items(&mut self, items: Vec<Item>) {
        match serde_json::to_value(&items) {
            Ok(items) => self.save_edge(json!({ "items": items })),
            Err(error) => self.failed(error.to_string()),
        }
    }
    fn save_edge(&mut self, changes: serde_json::Value) {
        match Config::save_edge_menu(changes) {
            Ok(config) => {
                self.config = config;
                self.edge_menu.refresh(&self.config);
                self.saved();
            }
            Err(error) => self.failed(error),
        }
    }
}

/// A folder item named after the folder.
pub(super) fn folder_item(path: &std::path::Path) -> Item {
    Item::new(Kind::Folder {
        path: path.to_string_lossy().into_owned(),
        name: path
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_else(|| path.to_string_lossy().into_owned()),
    })
}

/// The config members an edit writes, for Undo.
pub(super) fn scope(page: &EdgeMenuPage, edit: &Edit) -> Option<&'static str> {
    Some(match edit {
        Edit::Enabled(_) => "enabled",
        Edit::Side(_) => "side",
        Edit::Position(_) => "position",
        Edit::Open(_) => "open",
        Edit::Style(_) => "style",
        Edit::IconSize(_) => "icon_size",
        Edit::Columns(_) => "columns",
        Edit::Labels(_) => "labels",
        Edit::SectionTitles(_) => "section_titles",
        Edit::Frame(_) => "frame",
        Edit::Handle(_) => "handle",
        Edit::HandleColor(_) => "handle_color",
        Edit::Close(_) => "close",
        Edit::Speed(_) => "speed",
        Edit::Displays(_) => "displays",
        Edit::Output(..) => "outputs",
        Edit::Commit => page.draft?.0.key(),
        Edit::Add(_)
        | Edit::Remove(_)
        | Edit::MoveUp(_)
        | Edit::MoveDown(_)
        | Edit::Reset
        | Edit::FolderChosen(Ok(Some(_))) => "items",
        Edit::DragDrop if page.reorder.source.is_some() => "items",
        _ => return None,
    })
}
