//! The Data tiles editor (ADR 0015): the form that adds a tile, the fields
//! its file holds, and the readings the page's preview shows.
use super::Saved;
use crate::config::Config;
use crate::drawer::Widget;
use crate::tiles::{self, Show, Source, Tile, Value};
use cosmic::app::Task;
use std::path::PathBuf;

#[derive(Clone, Debug)]
pub enum Message {
    /// Fill the form from a starting point in [`tiles::TEMPLATES`].
    Template(usize),
    Label(String),
    /// A file path or web address, as typed.
    Source(String),
    /// Read the typed source and list its fields.
    ReadSource,
    Choose,
    Chosen(Result<Option<PathBuf>, String>),
    /// A source read for the form: the fields it holds, or why not.
    Read(Source, Result<Vec<(String, Value)>, String>),
    Field(String),
    Show(u8),
    Unit(String),
    Min(String),
    Max(String),
    Every(u32),
    Add,
    Remove(usize),
    /// Read the saved tiles' files again for the preview.
    Refresh,
    Preview(tiles::State),
}

/// The form for a new tile, as typed.
#[derive(Clone, Debug)]
pub struct Draft {
    pub label: String,
    /// A file path or a web address.
    pub source: String,
    pub field: String,
    /// Text, progress bar, gauge or graph.
    pub show: u8,
    pub unit: String,
    pub min: String,
    pub max: String,
    pub every: u32,
}
impl Default for Draft {
    fn default() -> Self {
        Self {
            label: String::new(),
            source: String::new(),
            field: String::new(),
            show: 0,
            unit: String::new(),
            min: "0".into(),
            max: "100".into(),
            every: 60,
        }
    }
}
pub const SHOWS: [&str; 4] = ["Text", "Progress bar", "Gauge", "Graph"];

#[derive(Default)]
pub struct Editor {
    pub draft: Draft,
    /// The fields of the draft's source, once read.
    pub fields: Option<(Source, Result<Vec<(String, Value)>, String>)>,
    pub error: Option<String>,
    pub choosing: bool,
    pub preview: tiles::State,
    reading: bool,
}

/// What a source holds, for the form, in a sentence when it can't be used.
fn listed(source: Source) -> Task<crate::settings::Message> {
    cosmic::task::future(async move {
        let result = tiles::read(source.clone())
            .await
            .map(|document| tiles::fields(&document.value))
            .map_err(|problem| problem.text());
        crate::settings::Message::Tools(super::Message::Tile(Message::Read(source, result)))
    })
}

/// Why a typed source can't be used, in a sentence.
fn unusable(source: &Source) -> &'static str {
    match source {
        Source::File { .. } => "Choose a file, or type its full path.",
        Source::Http { url } if url.to_ascii_lowercase().starts_with("http://") => {
            "Plain http works only on your own network. Use https."
        }
        Source::Http { .. } => "Use a whole web address, like https://example.com/status.json.",
    }
}

impl Editor {
    /// The tile the form describes, or what to fix first.
    fn tile(&self) -> Result<Tile, &'static str> {
        let d = &self.draft;
        let number = |s: &str| s.trim().parse::<f64>().ok().filter(|n| n.is_finite());
        let show = match d.show {
            0 => Show::Text,
            1 => Show::Progress {
                max: number(&d.max).ok_or("Use a number for the largest value.")?,
            },
            2 => Show::Gauge {
                min: number(&d.min).ok_or("Use a number for the smallest value.")?,
                max: number(&d.max).ok_or("Use a number for the largest value.")?,
            },
            _ => Show::Graph,
        };
        let tile = Tile {
            label: d.label.trim().to_owned(),
            source: Source::typed(&d.source),
            field: d.field.trim().to_owned(),
            show,
            unit: d.unit.trim().to_owned(),
            every: d.every,
        };
        if tile.label.is_empty() {
            return Err("Give the tile a name.");
        }
        if !tile.source.valid() {
            return Err(unusable(&tile.source));
        }
        if tile.source.web() && tile.every < tiles::WEB_EVERY {
            return Err("Web addresses are read every 30 s at most.");
        }
        if !tile.valid() {
            return Err(match show {
                Show::Progress { .. } => "The largest value must be above 0.",
                Show::Gauge { .. } => "The largest value must be above the smallest.",
                _ => "Use names joined by dots for the field, like build.result.",
            });
        }
        Ok(tile)
    }

    /// Reads the saved tiles' files for the preview, one read at a time.
    pub fn refresh(&mut self, config: &Config) -> Task<crate::settings::Message> {
        if self.reading || config.tiles.tiles.is_empty() {
            return Task::none();
        }
        self.reading = true;
        let (tiles, previous) = (config.tiles.tiles.clone(), self.preview.clone());
        cosmic::task::future(async move {
            crate::settings::Message::Tools(super::Message::Tile(Message::Preview(
                tiles::read_all(tiles, previous).await,
            )))
        })
    }

    pub fn update(
        &mut self,
        message: Message,
        config: &Config,
    ) -> (Task<crate::settings::Message>, Option<Saved>) {
        let save =
            |list: Vec<Tile>| Some(Saved::Tool(Widget::Tiles, "tiles", serde_json::json!(list)));
        let draft = &mut self.draft;
        match message {
            Message::Template(index) => {
                if let Some(t) = tiles::TEMPLATES.get(index) {
                    draft.label = t.label.into();
                    draft.field = t.field.into();
                    draft.unit = t.unit.into();
                    (draft.show, draft.min, draft.max) = match t.show {
                        Show::Text => (0, "0".into(), "100".into()),
                        Show::Progress { max } => (1, "0".into(), tiles::number(max)),
                        Show::Gauge { min, max } => (2, tiles::number(min), tiles::number(max)),
                        Show::Graph => (3, "0".into(), "100".into()),
                    };
                    self.error = None;
                }
            }
            Message::Label(value) => draft.label = value.chars().take(40).collect(),
            Message::Source(value) => {
                draft.source = value.chars().take(4096).collect();
                self.error = None;
            }
            Message::ReadSource => {
                let source = Source::typed(&draft.source);
                if source.valid() {
                    return (listed(source), None);
                }
                self.error = Some(unusable(&source).into());
            }
            Message::Choose => {
                if self.choosing {
                    return (Task::none(), None);
                }
                self.choosing = true;
                let task = cosmic::task::future(async {
                    use cosmic::dialog::file_chooser::{self, FileFilter, open::Dialog};
                    let result = match Dialog::new()
                        .title("Choose a JSON file for this tile")
                        .filter(FileFilter::new("JSON").glob("*.json"))
                        .open_file()
                        .await
                    {
                        Ok(response) => response
                            .url()
                            .to_file_path()
                            .map(Some)
                            .map_err(|_| "Choose a local file.".to_owned()),
                        Err(file_chooser::Error::Cancelled) => Ok(None),
                        Err(error) => Err(format!("Could not open the file chooser: {error}")),
                    };
                    crate::settings::Message::Tools(super::Message::Tile(Message::Chosen(result)))
                });
                return (task, None);
            }
            Message::Chosen(result) => {
                self.choosing = false;
                match result {
                    Ok(Some(path)) => {
                        draft.source = path.display().to_string();
                        return (listed(Source::File { path }), None);
                    }
                    Ok(None) => {}
                    Err(error) => self.error = Some(error),
                }
            }
            Message::Read(source, result) => {
                // Only the source the form still names.
                if Source::typed(&draft.source) == source {
                    self.error = None;
                    self.fields = Some((source, result));
                }
            }
            Message::Field(field) => {
                if draft.label.trim().is_empty()
                    && let Some(last) = field.rsplit('.').find(|s| s.parse::<usize>().is_err())
                {
                    let mut name: String = last.replace('_', " ");
                    if let Some(first) = name.get(..1) {
                        name = first.to_uppercase() + &name[1..];
                    }
                    draft.label = name.chars().take(40).collect();
                }
                draft.field = field.chars().take(120).collect();
                self.error = None;
            }
            Message::Show(show) => draft.show = show.min(3),
            Message::Unit(value) => draft.unit = value.chars().take(12).collect(),
            Message::Min(value) => draft.min = value.chars().take(20).collect(),
            Message::Max(value) => draft.max = value.chars().take(20).collect(),
            Message::Every(every) => {
                if tiles::EVERY.contains(&every) {
                    draft.every = every;
                }
            }
            Message::Add => {
                let list = &config.tiles.tiles;
                match self.tile() {
                    Err(error) => self.error = Some(error.into()),
                    Ok(_) if list.len() >= tiles::MAX_TILES => {
                        self.error =
                            Some("Data tiles shows six at most. Remove one to add another.".into());
                    }
                    Ok(tile) if list.contains(&tile) => {
                        self.error = Some("This tile is already there.".into());
                    }
                    Ok(tile) => {
                        self.error = None;
                        let mut list = list.clone();
                        list.push(tile);
                        // The next tile often reads the same file.
                        let draft = &mut self.draft;
                        draft.label.clear();
                        draft.field.clear();
                        draft.unit.clear();
                        return (Task::none(), save(list));
                    }
                }
            }
            Message::Remove(index) => {
                if index < config.tiles.tiles.len() {
                    let mut list = config.tiles.tiles.clone();
                    list.remove(index);
                    return (Task::none(), save(list));
                }
            }
            Message::Refresh => return (self.refresh(config), None),
            Message::Preview(state) => {
                self.reading = false;
                if state.tiles == config.tiles.tiles {
                    self.preview = state;
                }
            }
        }
        (Task::none(), None)
    }
}
