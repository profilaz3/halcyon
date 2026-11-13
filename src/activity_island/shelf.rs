//! The island's file shelf: files, folders and links dropped on the island,
//! kept as references (paths and addresses, never copies) to drag out again.
//! The list lives in `$XDG_RUNTIME_DIR/halcyon/shelf.json`, so it outlives the
//! applet restarting into an update but not the session. Files are looked at
//! only when the shelf is shown or something lands on it: nothing polls.
use super::*;
use cosmic::iced::clipboard::dnd::DndAction;
use cosmic::iced::clipboard::mime::AsMimeTypes;
use serde::{Deserialize, Serialize};
use std::borrow::Cow;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

/// Items kept at most; the oldest go first.
pub(super) const MAX_ITEMS: usize = 24;
/// What a file manager or browser offers for files and links.
const URI_LIST: &str = "text/uri-list";
/// A stored list larger than this is not ours to read.
const STORE_MAX: u64 = 64 * 1024;
const ROW: f32 = ROW_HEIGHT;
const GAP: f32 = 4.;
/// Rows shown before the list scrolls.
const VISIBLE: usize = 4;
/// The selected item's actions and Clear all, under the rows.
const STRIP: f32 = 32.;
const STRIP_GAP: f32 = 8.;
/// The room the chooser layout gives below the title line.
const ROOM: f32 = HEIGHT - 50.;
/// Thumbnails are drawn at twice their 32 px size.
const THUMB: u32 = 64;
/// Pictures larger than this get the icon, not a thumbnail.
const THUMB_MAX_BYTES: u64 = 40 << 20;
/// The tray glyph, on the pill and in the open island.
pub(super) const ICON: &str = "mail-folder-inbox-symbolic";

/// What an item points at: stored as `{"path": …}` or `{"url": …}`.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum Target {
    Path(PathBuf),
    Url(String),
}
#[derive(Clone, Debug, PartialEq)]
pub(super) struct Item {
    pub target: Target,
    pub name: String,
    pub size: Option<u64>,
    pub folder: bool,
    pub modified: Option<SystemTime>,
}
impl Item {
    fn new(target: Target) -> Self {
        let name = match &target {
            Target::Path(path) => path
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_else(|| path.to_string_lossy().into_owned()),
            Target::Url(url) => link_name(url),
        };
        Self {
            target,
            name,
            size: None,
            folder: false,
            modified: None,
        }
    }
    /// Reads the file's size and kind. False once the file is gone.
    fn stat(&mut self) -> bool {
        let Target::Path(path) = &self.target else {
            return true;
        };
        match std::fs::metadata(path) {
            Ok(meta) => {
                self.folder = meta.is_dir();
                self.size = (!self.folder).then_some(meta.len());
                self.modified = meta.modified().ok();
                true
            }
            Err(_) => false,
        }
    }
    pub fn path(&self) -> Option<&Path> {
        match &self.target {
            Target::Path(path) => Some(path),
            Target::Url(_) => None,
        }
    }
    pub fn uri(&self) -> String {
        match &self.target {
            Target::Path(path) => downloads::file_uri(path),
            Target::Url(url) => url.clone(),
        }
    }
    fn extension(&self) -> String {
        self.path()
            .and_then(|p| p.extension())
            .map(|e| e.to_string_lossy().to_ascii_lowercase())
            .unwrap_or_default()
    }
    /// A picture the island can make a thumbnail of.
    fn picture(&self) -> bool {
        !self.folder
            && matches!(self.extension().as_str(), "png" | "jpg" | "jpeg")
            && self.size.is_some_and(|s| s <= THUMB_MAX_BYTES)
    }
    pub fn icon(&self) -> &'static str {
        if matches!(self.target, Target::Url(_)) {
            return "web-browser-symbolic";
        }
        if self.folder {
            return "folder-symbolic";
        }
        match self.extension().as_str() {
            "png" | "jpg" | "jpeg" | "gif" | "webp" | "svg" | "bmp" | "avif" | "heic" | "tif"
            | "tiff" => "image-x-generic-symbolic",
            "mp3" | "flac" | "ogg" | "oga" | "opus" | "wav" | "m4a" | "aac" => {
                "audio-x-generic-symbolic"
            }
            "mp4" | "mkv" | "webm" | "mov" | "avi" | "m4v" => "video-x-generic-symbolic",
            "zip" | "tar" | "gz" | "tgz" | "xz" | "zst" | "bz2" | "7z" | "rar" | "deb"
            | "rpm" | "flatpak" => "package-x-generic-symbolic",
            "xls" | "xlsx" | "ods" | "csv" => "x-office-spreadsheet-symbolic",
            "ppt" | "pptx" | "odp" => "x-office-presentation-symbolic",
            "pdf" | "doc" | "docx" | "odt" | "rtf" | "epub" => "x-office-document-symbolic",
            _ => "text-x-generic-symbolic",
        }
    }
    /// The quiet second line: a size, "Folder" or a link's site.
    pub fn detail(&self) -> String {
        match &self.target {
            Target::Url(url) => link_host(url).unwrap_or("Link").to_owned(),
            Target::Path(_) if self.folder => "Folder".into(),
            Target::Path(_) => self.size.map(downloads::bytes_text).unwrap_or_default(),
        }
    }
}
/// A link's site, as people say it.
fn link_host(url: &str) -> Option<&str> {
    let rest = url.split_once("://")?.1;
    let host = rest.split(['/', '?', '#']).next()?;
    let host = host.rsplit('@').next()?;
    let host = host.strip_prefix("www.").unwrap_or(host);
    (!host.is_empty()).then_some(host)
}
/// A link's name: its last path part, else its site.
fn link_name(url: &str) -> String {
    let rest = url.split_once("://").map_or(url, |(_, r)| r);
    let path = rest.split(['?', '#']).next().unwrap_or(rest);
    let last = path
        .trim_end_matches('/')
        .split_once('/')
        .and_then(|(_, p)| p.rsplit('/').next())
        .filter(|s| !s.is_empty())
        .map(percent_decode)
        .and_then(|b| String::from_utf8(b).ok());
    last.or_else(|| link_host(url).map(str::to_owned))
        .unwrap_or_else(|| url.to_owned())
}
fn percent_decode(s: &str) -> Vec<u8> {
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        let hex = |b: u8| (b as char).to_digit(16).map(|d| d as u8);
        if bytes[i] == b'%'
            && let (Some(hi), Some(lo)) = (
                bytes.get(i + 1).copied().and_then(hex),
                bytes.get(i + 2).copied().and_then(hex),
            )
        {
            out.push(hi << 4 | lo);
            i += 3;
        } else {
            out.push(bytes[i]);
            i += 1;
        }
    }
    out
}
/// The files and links in a `text/uri-list`: local `file:` URIs as paths,
/// web addresses as links. Comments, other schemes and other hosts are left
/// out, and so are names that are not text.
pub(super) fn parse_uri_list(data: &[u8]) -> Vec<Target> {
    use std::os::unix::ffi::OsStringExt;
    let Ok(text) = std::str::from_utf8(data) else {
        return Vec::new();
    };
    let mut targets = Vec::new();
    for line in text.lines() {
        let line = line.trim_matches(|c: char| c == '\r' || c == '\0' || c.is_whitespace());
        if line.is_empty() || line.starts_with('#') || line.chars().any(char::is_control) {
            continue;
        }
        let target = if let Some(rest) = line.strip_prefix("file://") {
            let (host, path) = match rest.find('/') {
                Some(at) => rest.split_at(at),
                None => continue,
            };
            if !host.is_empty() && host != "localhost" {
                continue;
            }
            let path = PathBuf::from(std::ffi::OsString::from_vec(percent_decode(path)));
            // Stored as JSON text, so the name must be text too.
            if path.to_str().is_none() || !path.is_absolute() {
                continue;
            }
            Target::Path(path)
        } else if (line.starts_with("https://") || line.starts_with("http://"))
            && line.len() <= 2048
        {
            Target::Url(line.to_owned())
        } else {
            continue;
        };
        if !targets.contains(&target) {
            targets.push(target);
        }
    }
    targets
}

/// A phone or tablet paired through KDE Connect or GSConnect and in reach.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Device {
    pub name: String,
    via: Via,
}
#[derive(Clone, Debug, PartialEq, Eq)]
enum Via {
    /// KDE Connect's device id.
    KdeConnect(String),
    /// GSConnect's device object.
    GsConnect(String),
}
const KDECONNECT: &str = "org.kde.kdeconnect";
const GSCONNECT: &str = "org.gnome.Shell.Extensions.GSConnect";
const GSCONNECT_PATH: &str = "/org/gnome/Shell/Extensions/GSConnect";
/// Phones in reach, from whichever of KDE Connect and GSConnect is running.
/// Neither is started for the asking; without them the list is empty.
async fn devices() -> Vec<Device> {
    let ask = async {
        let Ok(bus) = zbus::Connection::session().await else {
            return Vec::new();
        };
        let mut found = Vec::new();
        if running(&bus, KDECONNECT).await {
            found.extend(kdeconnect_devices(&bus).await.unwrap_or_default());
        }
        if running(&bus, GSCONNECT).await {
            found.extend(gsconnect_devices(&bus).await.unwrap_or_default());
        }
        found.sort_by(|a, b| a.name.cmp(&b.name));
        found.truncate(2);
        found
    };
    tokio::time::timeout(Duration::from_secs(3), ask)
        .await
        .unwrap_or_default()
}
async fn running(bus: &zbus::Connection, name: &str) -> bool {
    bus.call_method(
        Some("org.freedesktop.DBus"),
        "/org/freedesktop/DBus",
        Some("org.freedesktop.DBus"),
        "NameHasOwner",
        &(name,),
    )
    .await
    .ok()
    .and_then(|r| r.body().deserialize::<bool>().ok())
    .unwrap_or(false)
}
async fn kdeconnect_devices(bus: &zbus::Connection) -> zbus::Result<Vec<Device>> {
    let reply = bus
        .call_method(
            Some(KDECONNECT),
            "/modules/kdeconnect",
            Some("org.kde.kdeconnect.daemon"),
            "deviceNames",
            // Reachable and paired only.
            &(true, true),
        )
        .await?;
    let names: HashMap<String, String> = reply.body().deserialize()?;
    Ok(names
        .into_iter()
        .filter(|(id, name)| valid_id(id) && !name.trim().is_empty())
        .map(|(id, name)| Device {
            name: name.trim().to_owned(),
            via: Via::KdeConnect(id),
        })
        .collect())
}
async fn gsconnect_devices(bus: &zbus::Connection) -> zbus::Result<Vec<Device>> {
    use zbus::zvariant::{OwnedObjectPath, OwnedValue};
    let reply = bus
        .call_method(
            Some(GSCONNECT),
            GSCONNECT_PATH,
            Some("org.freedesktop.DBus.ObjectManager"),
            "GetManagedObjects",
            &(),
        )
        .await?;
    let objects: HashMap<OwnedObjectPath, HashMap<String, HashMap<String, OwnedValue>>> =
        reply.body().deserialize()?;
    let mut found = Vec::new();
    for (path, interfaces) in objects {
        let Some(device) = interfaces.get("org.gnome.Shell.Extensions.GSConnect.Device") else {
            continue;
        };
        let flag = |key: &str| {
            device
                .get(key)
                .and_then(|v| bool::try_from(v).ok())
                .unwrap_or(false)
        };
        let name = device
            .get("Name")
            .and_then(|v| <&str>::try_from(v).ok())
            .map(|s| s.trim().to_owned())
            .unwrap_or_default();
        if flag("Connected") && flag("Paired") && !name.is_empty() {
            found.push(Device {
                name,
                via: Via::GsConnect(path.to_string()),
            });
        }
    }
    Ok(found)
}
/// KDE Connect ids become part of an object path.
fn valid_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 64
        && id.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
}
/// Hands a file or link to the phone's share plugin.
async fn send(device: Device, target: Target, uri: String) -> Result<(), String> {
    let ask = async {
        let bus = zbus::Connection::session().await?;
        match &device.via {
            Via::KdeConnect(id) => {
                bus.call_method(
                    Some(KDECONNECT),
                    format!("/modules/kdeconnect/devices/{id}/share").as_str(),
                    Some("org.kde.kdeconnect.device.share"),
                    "shareUrl",
                    &(uri.as_str(),),
                )
                .await?;
            }
            Via::GsConnect(path) => {
                use zbus::zvariant::Value;
                let (action, parameter) = match target {
                    Target::Path(_) => ("shareFile", Value::from((uri.clone(), false))),
                    Target::Url(_) => ("shareUri", Value::from(uri.clone())),
                };
                bus.call_method(
                    Some(GSCONNECT),
                    path.as_str(),
                    Some("org.gtk.Actions"),
                    "Activate",
                    &(
                        action,
                        vec![parameter],
                        HashMap::<String, Value<'_>>::new(),
                    ),
                )
                .await?;
            }
        }
        zbus::Result::Ok(())
    };
    match tokio::time::timeout(Duration::from_secs(10), ask).await {
        Ok(Ok(())) => Ok(()),
        _ => Err(format!("Could not send it to {}", device.name)),
    }
}

/// Files for the clipboard or a drag: their URIs, as a file manager pastes
/// them, and their paths as text.
#[derive(Clone, Debug)]
pub(super) struct Files {
    uris: Vec<String>,
    text: String,
    /// GNOME's copied-files list, which file managers paste as a copy.
    clipboard: bool,
}
impl Files {
    fn of(item: &Item, clipboard: bool) -> Self {
        Self {
            uris: vec![item.uri()],
            text: match &item.target {
                Target::Path(path) => path.to_string_lossy().into_owned(),
                Target::Url(url) => url.clone(),
            },
            clipboard: clipboard && item.path().is_some(),
        }
    }
}
impl AsMimeTypes for Files {
    fn available(&self) -> Cow<'static, [String]> {
        let mut mimes = vec![URI_LIST.to_owned()];
        if self.clipboard {
            mimes.push("x-special/gnome-copied-files".to_owned());
        }
        mimes.extend(["text/plain;charset=utf-8".to_owned(), "text/plain".to_owned()]);
        Cow::Owned(mimes)
    }
    fn as_bytes(&self, mime: &str) -> Option<Cow<'static, [u8]>> {
        let list = || self.uris.iter().map(|u| format!("{u}\r\n")).collect::<String>();
        let bytes = match mime {
            URI_LIST => list(),
            "x-special/gnome-copied-files" if self.clipboard => {
                format!("copy\n{}", self.uris.join("\n"))
            }
            "text/plain;charset=utf-8" | "text/plain" => self.text.clone(),
            _ => return None,
        };
        Some(Cow::Owned(bytes.into_bytes()))
    }
}

/// A thumbnail decoded off the update loop: its file, the version it shows
/// and its pixels.
#[derive(Clone)]
pub struct Thumb {
    path: PathBuf,
    modified: Option<SystemTime>,
    rgba: Vec<u8>,
}
impl std::fmt::Debug for Thumb {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Thumb").field("path", &self.path).finish()
    }
}
/// A square, centre-cropped thumbnail of a PNG or JPEG.
fn thumbnail(path: &Path) -> Option<Vec<u8>> {
    let mut reader = image::ImageReader::open(path).ok()?.with_guessed_format().ok()?;
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(12_000);
    limits.max_image_height = Some(12_000);
    limits.max_alloc = Some(256 << 20);
    reader.limits(limits);
    let picture = reader.decode().ok()?;
    Some(
        picture
            .resize_to_fill(THUMB, THUMB, image::imageops::FilterType::Triangle)
            .into_rgba8()
            .into_raw(),
    )
}

#[derive(Default)]
pub(super) struct Shelf {
    /// Newest first.
    pub items: Vec<Item>,
    /// The open island shows the shelf.
    pub open: bool,
    /// Something is being dragged over the island.
    pub over: bool,
    /// The item the actions under the rows are for.
    pub selected: usize,
    pub devices: Vec<Device>,
    thumbs: HashMap<PathBuf, (Option<SystemTime>, widget::image::Handle)>,
    /// Where the list is kept; nowhere in tests unless one asks.
    store: Option<PathBuf>,
}
#[derive(Serialize, Deserialize, Default)]
struct Stored {
    #[serde(default)]
    items: Vec<Target>,
}
impl Shelf {
    /// The session's shelf, as the last run of the island left it.
    pub fn load() -> Self {
        #[cfg(not(test))]
        let store = std::env::var_os("XDG_RUNTIME_DIR")
            .map(PathBuf::from)
            .filter(|dir| dir.is_absolute())
            .map(|dir| dir.join("halcyon").join("shelf.json"));
        #[cfg(test)]
        let store = None;
        Self::at(store)
    }
    pub fn at(store: Option<PathBuf>) -> Self {
        let mut shelf = Self {
            store,
            ..Default::default()
        };
        let stored = shelf
            .store
            .as_deref()
            .and_then(read_stored)
            .unwrap_or_default();
        shelf.items = stored
            .items
            .into_iter()
            .filter(|t| !matches!(t, Target::Path(p) if !p.is_absolute()))
            .take(MAX_ITEMS)
            .map(Item::new)
            .filter_map(|mut item| item.stat().then_some(item))
            .collect();
        shelf
    }
    /// Keeps the list: what each item points at, nothing more.
    pub fn save(&self) -> std::io::Result<()> {
        use std::io::Write;
        use std::os::unix::fs::{DirBuilderExt, OpenOptionsExt};
        let Some(store) = &self.store else {
            return Ok(());
        };
        let dir = store.parent().ok_or(std::io::ErrorKind::NotFound)?;
        std::fs::DirBuilder::new()
            .recursive(true)
            .mode(0o700)
            .create(dir)?;
        let stored = Stored {
            items: self.items.iter().map(|i| i.target.clone()).collect(),
        };
        let json = serde_json::to_vec(&stored).map_err(std::io::Error::other)?;
        let next = store.with_extension("json.next");
        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .mode(0o600)
            .open(&next)?;
        file.write_all(&json)?;
        drop(file);
        std::fs::rename(&next, store)
    }
    fn persist(&self) {
        if let Err(error) = self.save() {
            eprintln!("Activity island shelf: could not keep the list: {error}");
        }
    }
    /// Puts dropped files and links first, in the order they came. One
    /// already on the shelf moves up; files that are gone are skipped.
    /// Returns how many were added or moved.
    pub fn add(&mut self, targets: Vec<Target>) -> usize {
        let mut fresh = Vec::new();
        for target in targets {
            if fresh.iter().any(|i: &Item| i.target == target) {
                continue;
            }
            let mut item = Item::new(target);
            if item.stat() {
                fresh.push(item);
            }
        }
        let added = fresh.len();
        if added == 0 {
            return 0;
        }
        self.items.retain(|i| !fresh.iter().any(|f| f.target == i.target));
        fresh.append(&mut self.items);
        fresh.truncate(MAX_ITEMS);
        self.items = fresh;
        self.selected = 0;
        self.forget_thumbs();
        added
    }
    /// Drops items whose file is gone and refreshes sizes. True if any left.
    pub fn prune(&mut self) -> bool {
        let before = self.items.len();
        self.items.retain_mut(Item::stat);
        self.selected = self.selected.min(self.items.len().saturating_sub(1));
        self.forget_thumbs();
        self.items.len() != before
    }
    pub fn remove(&mut self, index: usize) -> bool {
        if index >= self.items.len() {
            return false;
        }
        self.items.remove(index);
        if self.selected > index || self.selected >= self.items.len() {
            self.selected = self.selected.saturating_sub(1);
        }
        self.forget_thumbs();
        true
    }
    pub fn clear(&mut self) {
        self.items.clear();
        self.selected = 0;
        self.thumbs.clear();
    }
    /// Thumbnails only for the pictures still on the shelf.
    fn forget_thumbs(&mut self) {
        let items = &self.items;
        self.thumbs
            .retain(|path, _| items.iter().any(|i| i.path() == Some(path.as_path())));
    }
    /// Pictures without a current thumbnail.
    fn wanted_thumbs(&self) -> Vec<(PathBuf, Option<SystemTime>)> {
        self.items
            .iter()
            .filter(|i| i.picture())
            .filter_map(|i| Some((i.path()?.to_owned(), i.modified)))
            .filter(|(path, modified)| {
                self.thumbs
                    .get(path)
                    .is_none_or(|(seen, _)| seen != modified)
            })
            .collect()
    }
    fn thumb(&self, item: &Item) -> Option<widget::image::Handle> {
        let (seen, handle) = self.thumbs.get(item.path()?)?;
        (*seen == item.modified).then(|| handle.clone())
    }
    /// Decodes the thumbnails showing the shelf would ask for, in place.
    #[cfg(test)]
    pub fn decode_thumbs(&mut self) -> usize {
        let wanted = self.wanted_thumbs();
        let count = wanted.len();
        for (path, modified) in wanted {
            let rgba = thumbnail(&path).expect("a thumbnail");
            self.thumbs.insert(
                path,
                (modified, widget::image::Handle::from_rgba(THUMB, THUMB, rgba)),
            );
        }
        count
    }
    #[cfg(test)]
    pub fn put_device(&mut self, name: &str) {
        self.devices.push(Device {
            name: name.into(),
            via: Via::KdeConnect("test".into()),
        });
    }
}
fn read_stored(path: &Path) -> Option<Stored> {
    let meta = std::fs::metadata(path).ok()?;
    if !meta.is_file() || meta.len() > STORE_MAX {
        return None;
    }
    serde_json::from_slice(&std::fs::read(path).ok()?).ok()
}

#[derive(Debug, Clone)]
pub enum Msg {
    /// Open the island on the shelf.
    Show,
    /// From the shelf back to the card.
    Back,
    /// Files are being dragged over the island.
    DragEnter,
    DragLeave,
    /// A `text/uri-list` was dropped.
    Dropped(Vec<u8>),
    Select(usize),
    Open(usize),
    Reveal(usize),
    Copy(usize),
    /// Send an item to a device.
    Send(usize, usize),
    Remove(usize),
    Clear,
    Devices(Vec<Device>),
    Thumbs(Vec<Thumb>),
}

impl Applet {
    /// The shelf is switched on.
    pub(super) fn shelf_enabled(&self) -> bool {
        self.snapshot.preferences.shelf && self.active
    }
    /// The open island shows the shelf, or a drop onto it.
    pub(super) fn shelf_shown(&self) -> bool {
        self.shelf.open && self.shelf_enabled()
    }
    pub(super) fn shelf_accent(&self, palette: Palette) -> Color {
        palette.readable(accent::SHELF)
    }
    /// Height the shelf needs beyond the card: rows up to `VISIBLE`, then
    /// the actions. A drop needs no more than the card.
    pub(super) fn shelf_extra(&self) -> f32 {
        let count = self.shelf.items.len().min(VISIBLE);
        if count == 0 || self.shelf.over {
            return 0.;
        }
        let rows = count as f32 * ROW + (count - 1) as f32 * GAP;
        (rows + STRIP_GAP + STRIP - ROOM).max(0.)
    }
    /// The open title line names the shelf.
    pub(super) fn shelf_heading(&self) -> String {
        // A Standard bar keeps the title line at its resting width, too
        // narrow for the word; the tray glyph names the card there and the
        // title carries only the count.
        let attached = self.frame().attached;
        match self.shelf.items.len() {
            _ if self.shelf.over && attached => String::new(),
            _ if self.shelf.over => "Shelf".into(),
            0 if attached => String::new(),
            0 => "Shelf".into(),
            count if attached => count.to_string(),
            count => format!("Shelf · {count}"),
        }
    }
    /// Accepts files and links dragged onto the pill or the open island.
    pub(super) fn shelf_target<'a>(
        &self,
        child: Element<'a, Message>,
    ) -> Element<'a, Message> {
        if !self.shelf_enabled() {
            return child;
        }
        widget::dnd_destination(child, vec![Cow::Borrowed(URI_LIST)])
            // Never a move: the file stays where it is.
            .action(DndAction::Copy)
            .preferred_action(DndAction::Copy)
            .on_enter(|_, _, _| Message::Shelf(Msg::DragEnter))
            .on_leave(|| Message::Shelf(Msg::DragLeave))
            .on_finish(|_, data, _, _, _| Message::Shelf(Msg::Dropped(data)))
            .into()
    }
    /// Opens the island on the shelf, in hand and tall enough for the rows.
    fn shelf_grow(&mut self, now: Instant) -> Task<Message> {
        self.shelf.open = true;
        self.reveal_deadline = None;
        self.revealed = false;
        self.hover_deadline = None;
        self.leave_deadline = None;
        self.hover_block = None;
        self.chooser = false;
        self.list = false;
        self.menu = None;
        self.ui.view = live::View::Card;
        self.problem = None;
        self.now_ms = chrono::Utc::now().timestamp_millis();
        let Some(id) = self.popup else {
            self.held = true;
            self.paused_until = None;
            return self.open_popup_sized();
        };
        if !self.held || self.popup_extra + 0.5 < self.shelf_extra() {
            self.held = true;
            self.promote_popup = Some(id);
            return self.update(Message::Surface(cosmic::surface::action::destroy_popup(id)));
        }
        if self.motion.target() == 0. {
            self.motion.retarget(1., self.animates(), now);
        }
        Task::none()
    }
    /// What only showing the shelf asks for: thumbnails of new pictures and
    /// the phones in reach.
    fn shelf_look(&mut self) -> Task<Message> {
        let wanted = self.shelf.wanted_thumbs();
        let thumbs = if wanted.is_empty() {
            Task::none()
        } else {
            cosmic::task::future(async move {
                let thumbs = tokio::task::spawn_blocking(move || {
                    wanted
                        .into_iter()
                        .filter_map(|(path, modified)| {
                            thumbnail(&path).map(|rgba| Thumb {
                                path,
                                modified,
                                rgba,
                            })
                        })
                        .collect()
                })
                .await
                .unwrap_or_default();
                Message::Shelf(Msg::Thumbs(thumbs))
            })
        };
        let phones =
            cosmic::task::future(async { Message::Shelf(Msg::Devices(devices().await)) });
        Task::batch([thumbs, phones])
    }
    /// The open island has closed: the shelf is put away with it.
    pub(super) fn shelf_closed(&mut self) {
        self.shelf.open = false;
        self.shelf.over = false;
    }
    pub(super) fn shelf_update(&mut self, msg: Msg, now: Instant) -> Task<Message> {
        if !self.shelf_enabled() {
            self.shelf_closed();
            return Task::none();
        }
        let item = |s: &Self, index: usize| s.shelf.items.get(index).cloned();
        match msg {
            Msg::Show => {
                self.shelf.over = false;
                if self.shelf.prune() {
                    self.shelf.persist();
                }
                let grow = self.shelf_grow(now);
                Task::batch([grow, self.shelf_look()])
            }
            Msg::Back => {
                self.shelf.open = false;
                Task::none()
            }
            Msg::DragEnter => {
                self.shelf.over = true;
                self.shelf.open = true;
                self.hover_deadline = None;
                self.leave_deadline = None;
                self.reveal_deadline = None;
                self.revealed = false;
                self.chooser = false;
                self.list = false;
                self.menu = None;
                self.ui.view = live::View::Card;
                match self.popup {
                    // Opened without a grab: a grab would take the drag.
                    None => {
                        self.paused_until = None;
                        self.open_popup_sized()
                    }
                    Some(_) => {
                        if self.motion.target() == 0. {
                            self.motion.retarget(1., self.animates(), now);
                        }
                        Task::none()
                    }
                }
            }
            Msg::DragLeave => {
                self.shelf.over = false;
                if self.popup.is_some() && !self.held {
                    // Crossing from the pill to the open island leaves one
                    // and enters the other; only a real departure closes.
                    self.leave_deadline = Some(now + LEAVE_GRACE);
                } else if self.shelf.items.is_empty() && self.held {
                    self.shelf.open = false;
                }
                Task::none()
            }
            Msg::Dropped(data) => {
                self.shelf.over = false;
                let added = self.shelf.add(parse_uri_list(&data));
                if added == 0 {
                    if self.shelf.items.is_empty() && !self.held {
                        self.shelf.open = false;
                        return self.update(Message::Close);
                    }
                    return Task::none();
                }
                self.shelf.persist();
                let grow = self.shelf_grow(now);
                Task::batch([grow, self.shelf_look()])
            }
            Msg::Select(index) => {
                if index < self.shelf.items.len() {
                    self.shelf.selected = index;
                }
                Task::none()
            }
            Msg::Open(index) => {
                let Some(item) = item(self, index) else {
                    return Task::none();
                };
                let target = match item.target {
                    Target::Path(path) => path,
                    Target::Url(url) => PathBuf::from(url),
                };
                let close = self.update(Message::Close);
                let open = cosmic::task::future(async move {
                    Message::SettingsOpened(downloads::open_path(target).await)
                });
                Task::batch([close, open])
            }
            Msg::Reveal(index) => {
                let Some(path) = item(self, index).and_then(|i| i.path().map(Path::to_owned))
                else {
                    return Task::none();
                };
                let close = self.update(Message::Close);
                let show = cosmic::task::future(async move {
                    Message::SettingsOpened(downloads::show_in_folder(path).await)
                });
                Task::batch([close, show])
            }
            Msg::Copy(index) => match item(self, index) {
                Some(item) => {
                    cosmic::iced::clipboard::write_data(Files::of(&item, true))
                }
                None => Task::none(),
            },
            Msg::Send(index, device) => {
                let (Some(item), Some(device)) =
                    (item(self, index), self.shelf.devices.get(device).cloned())
                else {
                    return Task::none();
                };
                let uri = item.uri();
                cosmic::task::future(async move {
                    Message::SettingsOpened(send(device, item.target, uri).await)
                })
            }
            Msg::Remove(index) => {
                if self.shelf.remove(index) {
                    self.shelf.persist();
                }
                Task::none()
            }
            Msg::Clear => {
                self.shelf.clear();
                self.shelf.persist();
                Task::none()
            }
            Msg::Devices(devices) => {
                self.shelf.devices = devices;
                Task::none()
            }
            Msg::Thumbs(thumbs) => {
                for t in thumbs {
                    if self.shelf.items.iter().any(|i| i.path() == Some(t.path.as_path())) {
                        self.shelf.thumbs.insert(
                            t.path,
                            (
                                t.modified,
                                widget::image::Handle::from_rgba(THUMB, THUMB, t.rgba),
                            ),
                        );
                    }
                }
                Task::none()
            }
        }
    }
    /// The Idle card's way to the shelf, with how many it holds.
    pub(super) fn shelf_button<'a>(
        &self,
        palette: Palette,
        tint: Color,
        reveal: f32,
    ) -> Element<'a, Message> {
        let count = self.shelf.items.len();
        let mut content = widget::row([tinted(ICON, 16, alpha(palette.ink, reveal))])
        .spacing(6)
        .align_y(cosmic::iced::Alignment::Center);
        if count > 0 {
            content = content.push(self.text(count.to_string(), 12., alpha(palette.ink, reveal)));
        }
        self.tip(
            palette,
            widget::button::custom(
                widget::container(content)
                    .height(Length::Fill)
                    .align_y(cosmic::iced::Alignment::Center),
            )
            .id("activity-shelf".into())
            .height(32)
            .padding(if count > 0 { [0, 12] } else { [0, 14] })
            .class(capsule_button(palette, tint, reveal, Role::Quiet))
            .on_press(Message::Shelf(Msg::Show)),
            "Shelf",
            widget::tooltip::Position::Top,
        )
    }
    /// Activity settings as a glyph, where the shelf or the devices button
    /// leaves no room for its label.
    pub(super) fn settings_glyph_button<'a>(
        &self,
        palette: Palette,
        tint: Color,
        reveal: f32,
    ) -> Element<'a, Message> {
        self.tip(
            palette,
            widget::button::icon(
                widget::icon::from_name("preferences-system-symbolic").symbolic(true),
            )
            .icon_size(16)
            .padding([8, 14])
            .width(44)
            .height(32)
            .id("activity-preferences".into())
            .class(capsule_button(palette, tint, reveal, Role::Quiet))
            .on_press(Message::Settings("panel")),
            "Activity settings",
            widget::tooltip::Position::Top,
        )
    }
    /// Resting with nothing else to show, the pill shows the shelf's tray
    /// and how many it holds.
    pub(super) fn shelf_trailing<'a>(&self, palette: Palette) -> Option<Element<'a, Message>> {
        let count = self.shelf.items.len();
        (self.shelf_enabled() && count > 0).then(|| {
            let ink = palette.quiet;
            widget::container(
                widget::row([
                    tinted(ICON, 14, ink),
                    live::tabular_text(&count.to_string(), 11., ink),
                ])
                .spacing(3)
                .align_y(cosmic::iced::Alignment::Center),
            )
            .id(widget::Id::new("activity-shelf-count"))
            .into()
        })
    }
    /// Width the pill's tray and count take beside the title.
    pub(super) fn shelf_trailing_width(&self) -> f32 {
        match self.shelf.items.len() {
            _ if !self.shelf_enabled() => 0.,
            0 => 0.,
            1..10 => 14. + 3. + 7. + 8.,
            _ => 14. + 3. + 14. + 8.,
        }
    }
    /// Below the title while open: the drop zone, the empty shelf or the
    /// rows with the selected item's actions.
    pub(super) fn shelf_view<'a>(&'a self, palette: Palette, reveal: f32) -> Element<'a, Message> {
        let accent = self.shelf_accent(palette);
        if self.shelf.over {
            return self.shelf_drop_zone(palette, accent, reveal);
        }
        if self.shelf.items.is_empty() {
            return widget::container(
                widget::column([
                    tinted(ICON, 20, alpha(palette.quiet, reveal)),
                    widget::text("Drop files to keep them here")
                        .size(12.)
                        .width(Length::Fill)
                        .align_x(cosmic::iced::Alignment::Center)
                        .class(cosmic::theme::Text::Color(alpha(palette.quiet, reveal)))
                        .into(),
                ])
                .width(Length::Fill)
                .spacing(6)
                .align_x(cosmic::iced::Alignment::Center),
            )
            .id(widget::Id::new("activity-shelf-empty"))
            .width(Length::Fill)
            .height(ROOM - 8.)
            .padding([0, 16])
            .align_x(cosmic::iced::Alignment::Center)
            .align_y(cosmic::iced::Alignment::Center)
            .into();
        }
        let mut rows = widget::column([]).spacing(GAP);
        for (index, item) in self.shelf.items.iter().enumerate() {
            rows = rows.push(self.shelf_row(palette, accent, reveal, index, item));
        }
        let shown = self.shelf.items.len().min(VISIBLE);
        let height = shown as f32 * ROW + shown.saturating_sub(1) as f32 * GAP;
        let list: Element<'a, Message> = if self.shelf.items.len() > VISIBLE {
            // The scrollbar keeps clear of the remove buttons.
            widget::scrollable(widget::container(rows).padding([0, 10, 0, 0]))
                .id(widget::Id::new("activity-shelf-list"))
                .height(height)
                .into()
        } else {
            rows.into()
        };
        widget::column([list, self.shelf_strip(palette, accent, reveal)])
            .spacing(STRIP_GAP)
            .width(Length::Fill)
            .into()
    }
    fn shelf_drop_zone<'a>(
        &self,
        palette: Palette,
        accent: Color,
        reveal: f32,
    ) -> Element<'a, Message> {
        let fill = alpha(accent, 0.14 * reveal);
        let edge = alpha(accent, 0.7 * reveal);
        widget::container(
            widget::row([
                tinted(ICON, 20, alpha(accent, reveal)),
                widget::text("Drop to keep on the shelf")
                    .size(13.)
                    .font(cosmic::font::semibold())
                    .class(cosmic::theme::Text::Color(alpha(palette.ink, reveal)))
                    .into(),
            ])
            .spacing(10)
            .align_y(cosmic::iced::Alignment::Center),
        )
        .id(widget::Id::new("activity-shelf-zone"))
        .width(Length::Fill)
        .height(Length::Fill)
        .max_height(ROOM - 8. + self.popup_extra)
        .align_x(cosmic::iced::Alignment::Center)
        .align_y(cosmic::iced::Alignment::Center)
        .class(cosmic::theme::Container::custom(move |_| {
            cosmic::widget::container::Style {
                background: Some(fill.into()),
                border: cosmic::iced::Border {
                    radius: 18.0.into(),
                    width: 1.5,
                    color: edge,
                },
                ..Default::default()
            }
        }))
        .into()
    }
    /// The picture's thumbnail, else its kind's icon, on a small tile.
    fn shelf_tile<'a>(
        &self,
        palette: Palette,
        reveal: f32,
        item: &Item,
    ) -> Element<'a, Message> {
        let face: Element<'a, Message> = match self.shelf.thumb(item) {
            Some(handle) => widget::image(handle)
                .width(32)
                .height(32)
                .border_radius([10.; 4])
                .into(),
            None => widget::container(tinted(
                item.icon(),
                16,
                alpha(palette.ink, reveal),
            ))
            .width(32)
            .height(32)
            .align_x(cosmic::iced::Alignment::Center)
            .align_y(cosmic::iced::Alignment::Center)
            .class(cosmic::theme::Container::custom(move |_| {
                cosmic::widget::container::Style {
                    background: Some(alpha(palette.ink, 0.08 * reveal).into()),
                    border: cosmic::iced::Border {
                        radius: 10.0.into(),
                        ..Default::default()
                    },
                    ..Default::default()
                }
            }))
            .into(),
        };
        face
    }
    /// One item: a row to pick it and drag it out, and its own remove.
    fn shelf_row<'a>(
        &'a self,
        palette: Palette,
        accent: Color,
        reveal: f32,
        index: usize,
        item: &'a Item,
    ) -> Element<'a, Message> {
        let selected = index == self.shelf.selected;
        let lines = widget::column([
            self.text(item.name.clone(), 13., alpha(palette.ink, reveal)),
            self.text(item.detail(), 11., alpha(palette.quiet, reveal)),
        ])
        .spacing(1);
        let content = widget::row([
            self.shelf_tile(palette, reveal, item),
            widget::container(lines).width(Length::Fill).into(),
        ])
        .spacing(10)
        .align_y(cosmic::iced::Alignment::Center);
        let pick = widget::button::custom(content)
            .id(widget::Id::new(format!("activity-shelf-row-{index}")))
            .width(Length::Fill)
            .height(ROW)
            .padding([6, 10, 6, 6])
            .class(capsule_button(
                palette,
                accent,
                reveal,
                if selected { Role::Quiet } else { Role::Ghost },
            ))
            .on_press(Message::Shelf(Msg::Select(index)));
        let files = Files::of(item, false);
        let name = item.name.clone();
        let icon = item.icon();
        let drag = widget::dnd_source::<Message, Files>(pick)
            .action(DndAction::Copy)
            .drag_content(move || files.clone())
            .drag_icon(move |offset| {
                (
                    drag_face(palette, icon, name.clone()),
                    cosmic::iced::advanced::widget::tree::State::None,
                    offset,
                )
            })
            .on_start(Some(Message::Shelf(Msg::Select(index))));
        widget::row([
            drag.into(),
            self.tip(
                palette,
                widget::button::icon(widget::icon::from_name("window-close-symbolic").symbolic(true))
                    .id(widget::Id::new(format!("activity-shelf-remove-{index}")))
                    .icon_size(14)
                    .padding(7)
                    .width(28)
                    .height(28)
                    .class(capsule_button(palette, accent, reveal, Role::Ghost))
                    .on_press(Message::Shelf(Msg::Remove(index))),
                "Remove from shelf",
                widget::tooltip::Position::Top,
            ),
        ])
        .spacing(2)
        .align_y(cosmic::iced::Alignment::Center)
        .into()
    }
    /// The selected item's actions, then Clear all.
    fn shelf_strip<'a>(&self, palette: Palette, accent: Color, reveal: f32) -> Element<'a, Message> {
        let index = self.shelf.selected;
        let Some(item) = self.shelf.items.get(index) else {
            return widget::Space::new().into();
        };
        let control = |id: String, icon: &'static str, label: String, press: Option<Message>| {
            self.tip(
                palette,
                widget::button::icon(widget::icon::from_name(icon).symbolic(true))
                    .id(widget::Id::new(id))
                    .icon_size(16)
                    .padding(8)
                    .width(32)
                    .height(32)
                    .class(capsule_button(palette, accent, reveal, Role::Quiet))
                    .on_press_maybe(press),
                &label,
                widget::tooltip::Position::Top,
            )
        };
        let mut row = widget::row([
            control(
                "activity-shelf-open".into(),
                "document-open-symbolic",
                "Open".into(),
                Some(Message::Shelf(Msg::Open(index))),
            ),
        ])
        .spacing(6)
        .align_y(cosmic::iced::Alignment::Center);
        if item.path().is_some() {
            row = row.push(control(
                "activity-shelf-folder".into(),
                "folder-open-symbolic",
                "Show in folder".into(),
                Some(Message::Shelf(Msg::Reveal(index))),
            ));
        }
        row = row.push(control(
            "activity-shelf-copy".into(),
            "edit-copy-symbolic",
            if item.path().is_some() { "Copy file" } else { "Copy link" }.into(),
            Some(Message::Shelf(Msg::Copy(index))),
        ));
        for (at, device) in self.shelf.devices.iter().enumerate() {
            row = row.push(control(
                format!("activity-shelf-send-{at}"),
                "smartphone-symbolic",
                format!("Send to {}", device.name),
                Some(Message::Shelf(Msg::Send(index, at))),
            ));
        }
        let controls = 2 + usize::from(item.path().is_some()) + self.shelf.devices.len();
        let inner = if self.frame().attached { 220. } else { 268. };
        row = row.push(widget::Space::new().width(Length::Fill));
        // Crowded by phones, Clear all keeps to a glyph.
        row = row.push(if controls as f32 * 38. + 92. > inner {
            control(
                "activity-shelf-clear".into(),
                "edit-clear-all-symbolic",
                "Clear all".into(),
                Some(Message::Shelf(Msg::Clear)),
            )
        } else {
            self.label_button(
                palette,
                "activity-shelf-clear",
                "Clear all",
                accent,
                reveal,
                Role::Quiet,
                Some(Message::Shelf(Msg::Clear)),
            )
        });
        row.width(Length::Fill).height(STRIP).into()
    }
}
/// A symbolic glyph in a given colour: buttons do not tint drawn icons.
pub(super) fn tinted<'a>(name: &'static str, size: u16, color: Color) -> Element<'a, Message> {
    widget::icon::from_name(name)
        .symbolic(true)
        .size(size)
        .icon()
        .class(cosmic::theme::Svg::custom(move |_| {
            cosmic::iced::widget::svg::Style { color: Some(color) }
        }))
        .into()
}
/// What follows the pointer while an item is dragged out.
fn drag_face(palette: Palette, icon: &'static str, name: String) -> Element<'static, ()> {
    widget::container(
        widget::row([
            widget::icon::from_name(icon)
                .symbolic(true)
                .size(16)
                .icon()
                .class(cosmic::theme::Svg::custom(move |_| {
                    cosmic::iced::widget::svg::Style {
                        color: Some(palette.ink),
                    }
                }))
                .into(),
            widget::text(name)
                .size(13.)
                .wrapping(cosmic::iced::advanced::text::Wrapping::None)
                .class(cosmic::theme::Text::Color(palette.ink))
                .into(),
        ])
        .spacing(8)
        .align_y(cosmic::iced::Alignment::Center),
    )
    .padding([6, 12])
    .class(cosmic::theme::Container::custom(move |_| {
        cosmic::widget::container::Style {
            background: Some(palette.raised.into()),
            border: cosmic::iced::Border {
                radius: 16.0.into(),
                ..Default::default()
            },
            ..Default::default()
        }
    }))
    .into()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "halcyon-shelf-{name}-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }
    fn file(dir: &Path, name: &str, bytes: usize) -> PathBuf {
        let path = dir.join(name);
        std::fs::write(&path, vec![b'x'; bytes]).unwrap();
        path
    }

    #[test]
    fn shelf_uri_lists_keep_local_files_and_web_links_only() {
        let list = b"# from a file manager\r\nfile:///home/me/My%20Report.pdf\r\n\
            file://localhost/tmp/a.png\r\nfile://elsewhere/tmp/b.png\r\n\
            https://example.com/docs/guide?x=1\r\nmailto:me@example.com\r\n\
            file:///home/me/My%20Report.pdf\r\n\r\nrelative/path\nfile:///caf%C3%A9.txt";
        assert_eq!(
            parse_uri_list(list),
            vec![
                Target::Path("/home/me/My Report.pdf".into()),
                Target::Path("/tmp/a.png".into()),
                Target::Url("https://example.com/docs/guide?x=1".into()),
                Target::Path("/café.txt".into()),
            ]
        );
        // Names that are not text, and binary rubbish, are left out.
        assert!(parse_uri_list(b"file:///tmp/%FF%FE").is_empty());
        assert!(parse_uri_list(&[0xff, 0xfe, 0x00]).is_empty());
        assert_eq!(link_name("https://www.example.com/a/report%20one.pdf"), "report one.pdf");
        assert_eq!(link_name("https://www.example.com/"), "example.com");
        assert_eq!(link_host("https://user@www.example.com:8/x"), Some("example.com:8"));
    }

    #[test]
    fn shelf_keeps_newest_first_bounded_and_without_duplicates() {
        let dir = temp("bounds");
        let mut shelf = Shelf::at(None);
        let paths: Vec<PathBuf> = (0..30)
            .map(|i| file(&dir, &format!("f{i}.txt"), i))
            .collect();
        assert_eq!(shelf.add(vec![Target::Path(paths[0].clone())]), 1);
        assert_eq!(
            shelf.add(paths[1..].iter().cloned().map(Target::Path).collect()),
            29
        );
        assert_eq!(shelf.items.len(), MAX_ITEMS);
        // A drop keeps its own order at the top; the oldest fell off.
        assert_eq!(shelf.items[0].path(), Some(paths[1].as_path()));
        assert!(shelf.items.iter().all(|i| i.path() != Some(paths[0].as_path())));
        // The same file again moves up instead of doubling.
        shelf.add(vec![Target::Path(paths[10].clone())]);
        assert_eq!(shelf.items.len(), MAX_ITEMS);
        assert_eq!(shelf.items[0].path(), Some(paths[10].as_path()));
        assert_eq!(shelf.items[0].size, Some(10));
        // A file that is not there is never added.
        assert_eq!(shelf.add(vec![Target::Path(dir.join("missing"))]), 0);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn shelf_list_round_trips_as_references_only() {
        let dir = temp("store");
        let store = dir.join("runtime").join("halcyon").join("shelf.json");
        let a = file(&dir, "a.pdf", 300);
        let folder = dir.join("Projects");
        std::fs::create_dir_all(&folder).unwrap();
        let mut shelf = Shelf::at(Some(store.clone()));
        assert!(shelf.items.is_empty());
        shelf.add(vec![
            Target::Path(a.clone()),
            Target::Path(folder.clone()),
            Target::Url("https://example.com/x".into()),
        ]);
        shelf.save().unwrap();
        let written = std::fs::read_to_string(&store).unwrap();
        // Paths and links, never what the files hold.
        assert!(!written.contains("xxx"));
        assert!(written.contains("\"path\"") && written.contains("\"url\""));
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(
            std::fs::metadata(&store).unwrap().permissions().mode() & 0o777,
            0o600
        );
        let again = Shelf::at(Some(store.clone()));
        assert_eq!(
            again.items.iter().map(|i| i.target.clone()).collect::<Vec<_>>(),
            shelf.items.iter().map(|i| i.target.clone()).collect::<Vec<_>>()
        );
        assert!(again.items[1].folder && again.items[1].detail() == "Folder");
        assert_eq!(again.items[0].detail(), "300 B");
        assert_eq!(again.items[2].detail(), "example.com");
        // A vanished file is gone on the next load.
        std::fs::remove_file(&a).unwrap();
        assert_eq!(Shelf::at(Some(store.clone())).items.len(), 2);
        // A damaged or oversized store reads as empty.
        std::fs::write(&store, b"{not json").unwrap();
        assert!(Shelf::at(Some(store.clone())).items.is_empty());
        std::fs::write(&store, vec![b' '; STORE_MAX as usize + 1]).unwrap();
        assert!(Shelf::at(Some(store)).items.is_empty());
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn shelf_prunes_vanished_files_and_keeps_the_selection_in_range() {
        let dir = temp("prune");
        let a = file(&dir, "a.txt", 1);
        let b = file(&dir, "b.txt", 2);
        let mut shelf = Shelf::at(None);
        shelf.add(vec![Target::Path(a.clone()), Target::Path(b.clone())]);
        shelf.selected = 1;
        std::fs::remove_file(&b).unwrap();
        assert!(shelf.prune());
        assert_eq!(shelf.items.len(), 1);
        assert_eq!(shelf.selected, 0);
        assert!(!shelf.prune());
        assert!(shelf.remove(0));
        assert!(!shelf.remove(0));
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn shelf_offers_files_as_a_file_manager_pastes_them() {
        let item = Item::new(Target::Path("/tmp/My File.png".into()));
        let copied = Files::of(&item, true);
        assert!(copied.available().contains(&"x-special/gnome-copied-files".to_owned()));
        assert_eq!(
            copied.as_bytes(URI_LIST).unwrap().as_ref(),
            b"file:///tmp/My%20File.png\r\n"
        );
        assert_eq!(
            copied.as_bytes("x-special/gnome-copied-files").unwrap().as_ref(),
            b"copy\nfile:///tmp/My%20File.png"
        );
        assert_eq!(copied.as_bytes("text/plain").unwrap().as_ref(), b"/tmp/My File.png");
        let dragged = Files::of(&item, false);
        assert!(!dragged.available().contains(&"x-special/gnome-copied-files".to_owned()));
        let link = Files::of(&Item::new(Target::Url("https://example.com".into())), true);
        assert_eq!(link.as_bytes("text/plain").unwrap().as_ref(), b"https://example.com");
        assert!(link.as_bytes("x-special/gnome-copied-files").is_none());
        assert_eq!(item.icon(), "image-x-generic-symbolic");
        assert!(valid_id("a1_b2") && !valid_id("../x") && !valid_id(""));
    }
}
