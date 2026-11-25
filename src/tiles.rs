//! Data tiles: values from local JSON files the person chooses, shown as
//! text, a progress bar, a gauge or a small graph (ADR 0015). Files are read
//! only while the widget shows; nothing is run.
use cosmic::iced::{Subscription, futures::SinkExt};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, VecDeque};
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

#[cfg(test)]
mod tests;

pub const MAX_TILES: usize = 6;
/// Larger files are refused, not cut: half a JSON document has no meaning.
pub const MAX_BYTES: u64 = 256 * 1024;
/// Readings a graph keeps, in memory only.
pub const HISTORY: usize = 48;
/// The refresh intervals offered, in seconds.
pub const EVERY: [u32; 6] = [10, 30, 60, 300, 900, 3600];
/// Fields Settings lists from a file, so a huge document stays a short list.
pub const MAX_FIELDS: usize = 40;
const MAX_LABEL: usize = 40;
const MAX_FIELD: usize = 120;
const MAX_UNIT: usize = 12;
const MAX_TEXT: usize = 80;

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Preferences {
    pub enabled: bool,
    #[serde(deserialize_with = "checked")]
    pub tiles: Vec<Tile>,
}

/// One reading on the card: where it comes from and how it shows.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Tile {
    pub label: String,
    pub source: Source,
    /// A dotted path into the document, a number picking a place in a list:
    /// `jobs.0.state`. Empty means the whole document.
    #[serde(default)]
    pub field: String,
    pub show: Show,
    #[serde(default)]
    pub unit: String,
    /// Seconds between reads, one of [`EVERY`].
    pub every: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum Source {
    File { path: PathBuf },
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum Show {
    Text,
    /// A bar filling from 0 to `max`.
    Progress {
        max: f64,
    },
    /// A dial from `min` to `max`.
    Gauge {
        min: f64,
        max: f64,
    },
    /// The last readings as a line.
    Graph,
}
impl Show {
    pub fn numeric(self) -> bool {
        self != Self::Text
    }
    pub fn label(self) -> &'static str {
        match self {
            Self::Text => "Text",
            Self::Progress { .. } => "Progress bar",
            Self::Gauge { .. } => "Gauge",
            Self::Graph => "Graph",
        }
    }
    fn valid(self) -> bool {
        match self {
            Self::Text | Self::Graph => true,
            Self::Progress { max } => max.is_finite() && max > 0.,
            Self::Gauge { min, max } => min.is_finite() && max.is_finite() && max > min,
        }
    }
}

fn short(value: &str, limit: usize) -> bool {
    value.chars().count() <= limit && !value.chars().any(char::is_control)
}

impl Tile {
    pub fn valid(&self) -> bool {
        let Source::File { path } = &self.source;
        short(&self.label, MAX_LABEL)
            && !self.label.trim().is_empty()
            && path.is_absolute()
            && path.as_os_str().len() <= 4096
            && short(&self.field, MAX_FIELD)
            && (self.field.is_empty() || self.field.split('.').all(|s| !s.trim().is_empty()))
            && short(&self.unit, MAX_UNIT)
            && EVERY.contains(&self.every)
            && self.show.valid()
    }
    pub fn path(&self) -> &Path {
        let Source::File { path } = &self.source;
        path
    }
    pub fn every(&self) -> Duration {
        Duration::from_secs(u64::from(self.every))
    }
}

/// Saved tiles that this version can't use are dropped, not fatal.
fn checked<'de, D: serde::Deserializer<'de>>(d: D) -> Result<Vec<Tile>, D::Error> {
    let values = Vec::<serde_json::Value>::deserialize(d)?;
    Ok(values
        .into_iter()
        .filter_map(|v| serde_json::from_value::<Tile>(v).ok())
        .filter(Tile::valid)
        .take(MAX_TILES)
        .collect())
}

/// The store's check for `/tiles/tiles`: every entry usable, six at most.
pub fn valid_tiles(value: &serde_json::Value) -> bool {
    value.as_array().is_some_and(|list| {
        list.len() <= MAX_TILES
            && list
                .iter()
                .all(|v| serde_json::from_value::<Tile>(v.clone()).is_ok_and(|tile| tile.valid()))
    })
}

/// What a field holds.
#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    Number(f64),
    Text(String),
    Flag(bool),
}
impl Value {
    /// A number, or text that reads as one.
    pub fn number(&self) -> Option<f64> {
        match self {
            Self::Number(n) => Some(*n),
            Self::Text(s) => s.trim().parse::<f64>().ok().filter(|n| n.is_finite()),
            Self::Flag(_) => None,
        }
    }
    pub fn text(&self) -> String {
        match self {
            Self::Number(n) => number(*n),
            Self::Text(s) => s.clone(),
            Self::Flag(true) => "Yes".into(),
            Self::Flag(false) => "No".into(),
        }
    }
}

/// A number as people write it: no trailing zeros, at most two decimals.
pub fn number(n: f64) -> String {
    let places = if n.fract() == 0. || n.abs() >= 1000. {
        0
    } else if n.abs() >= 100. {
        1
    } else {
        2
    };
    let text = format!("{n:.places$}");
    let text = if text.contains('.') {
        text.trim_end_matches('0').trim_end_matches('.').to_owned()
    } else {
        text
    };
    if text == "-0" { "0".into() } else { text }
}

#[derive(Debug, Clone, PartialEq)]
pub enum Problem {
    Missing,
    NotFile,
    TooBig,
    Unreadable,
    /// Not JSON; the line the parser stopped on.
    NotJson(usize),
    NoField,
    /// A list, a group of fields or null where one value belongs.
    NotValue,
    NotNumber,
}
impl Problem {
    pub fn text(&self) -> String {
        match self {
            Self::Missing => "File not found".into(),
            Self::NotFile => "Not a regular file".into(),
            Self::TooBig => "File is over 256 KB".into(),
            Self::Unreadable => "Can’t read the file".into(),
            Self::NotJson(line) => format!("Not valid JSON (line {line})"),
            Self::NoField => "No such field".into(),
            Self::NotValue => "The field holds a list or group".into(),
            Self::NotNumber => "Not a number".into(),
        }
    }
}

/// A file's document and when it last changed.
pub struct Document {
    pub value: serde_json::Value,
    pub changed: Option<SystemTime>,
}

/// Reads a regular file of at most [`MAX_BYTES`] as JSON. It opens without
/// blocking, so a FIFO or a device can't hang the reader, and checks what
/// it opened, not the name, so a swap between check and read changes
/// nothing.
pub fn read_file(path: &Path) -> Result<Document, Problem> {
    use std::io::Read;
    use std::os::unix::fs::OpenOptionsExt;
    let file = std::fs::OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NONBLOCK | libc::O_NOCTTY)
        .open(path)
        .map_err(|e| match e.kind() {
            std::io::ErrorKind::NotFound => Problem::Missing,
            _ => Problem::Unreadable,
        })?;
    let meta = file.metadata().map_err(|_| Problem::Unreadable)?;
    if !meta.is_file() {
        return Err(Problem::NotFile);
    }
    if meta.len() > MAX_BYTES {
        return Err(Problem::TooBig);
    }
    let mut bytes = Vec::with_capacity(meta.len() as usize);
    file.take(MAX_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| Problem::Unreadable)?;
    if bytes.len() as u64 > MAX_BYTES {
        return Err(Problem::TooBig);
    }
    let value = serde_json::from_slice(&bytes).map_err(|e| Problem::NotJson(e.line()))?;
    Ok(Document {
        value,
        changed: meta.modified().ok(),
    })
}

pub async fn read(path: PathBuf) -> Result<Document, Problem> {
    tokio::task::spawn_blocking(move || read_file(&path))
        .await
        .unwrap_or(Err(Problem::Unreadable))
}

/// The value at `field`, as `show` needs it.
pub fn evaluate(document: &serde_json::Value, field: &str, show: Show) -> Result<Value, Problem> {
    let mut at = document;
    if !field.is_empty() {
        for segment in field.split('.') {
            let segment = segment.trim();
            at = match at {
                serde_json::Value::Object(map) => map.get(segment),
                serde_json::Value::Array(list) => {
                    segment.parse::<usize>().ok().and_then(|i| list.get(i))
                }
                _ => None,
            }
            .ok_or(Problem::NoField)?;
        }
    }
    let value = scalar(at).ok_or(Problem::NotValue)?;
    if show.numeric() {
        return value.number().map(Value::Number).ok_or(Problem::NotNumber);
    }
    Ok(value)
}

fn scalar(value: &serde_json::Value) -> Option<Value> {
    match value {
        serde_json::Value::Number(n) => n.as_f64().map(Value::Number),
        serde_json::Value::String(s) => Some(Value::Text(if s.chars().count() > MAX_TEXT {
            s.chars().take(MAX_TEXT - 1).chain(['…']).collect()
        } else {
            s.clone()
        })),
        serde_json::Value::Bool(b) => Some(Value::Flag(*b)),
        _ => None,
    }
}

/// The fields of a document that hold one value, in reading order, with
/// what each holds: what Settings offers to pick from. Lists show their
/// first three places.
pub fn fields(document: &serde_json::Value) -> Vec<(String, Value)> {
    fn walk(at: &serde_json::Value, path: String, depth: u8, out: &mut Vec<(String, Value)>) {
        if out.len() >= MAX_FIELDS {
            return;
        }
        if let Some(value) = scalar(at) {
            out.push((path, value));
            return;
        }
        if depth == 6 {
            return;
        }
        let join = |key: &str| {
            if path.is_empty() {
                key.to_owned()
            } else {
                format!("{path}.{key}")
            }
        };
        match at {
            serde_json::Value::Object(map) => {
                for (key, value) in map {
                    // A key with a dot or an edge space can't be written as a path.
                    if !key.contains('.') && key.trim() == key && !key.is_empty() {
                        walk(value, join(key), depth + 1, out);
                    }
                }
            }
            serde_json::Value::Array(list) => {
                for (i, value) in list.iter().take(3).enumerate() {
                    walk(value, join(&i.to_string()), depth + 1, out);
                }
            }
            _ => {}
        }
    }
    let mut out = Vec::new();
    walk(document, String::new(), 0, &mut out);
    out
}

/// One tile's latest reading.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Reading {
    /// The last good value, kept through a failed read.
    pub value: Option<Value>,
    /// When the last good value was read.
    pub read_at: Option<SystemTime>,
    /// When the file last changed, as of the last read.
    pub changed: Option<SystemTime>,
    /// What went wrong with the latest read, if anything.
    pub problem: Option<Problem>,
    pub history: VecDeque<f64>,
}
impl Reading {
    /// How old the shown value is, when it may be out of date: a read has
    /// failed since, or the file hasn't changed for three intervals.
    pub fn stale(&self, tile: &Tile, now: SystemTime) -> Option<Duration> {
        let age = |at: Option<SystemTime>| at.and_then(|t| now.duration_since(t).ok());
        if self.value.is_some() && self.problem.is_some() {
            return age(self.read_at);
        }
        age(self.changed).filter(|age| *age >= tile.every() * 3)
    }
    fn apply(&mut self, tile: &Tile, document: &Result<Document, Problem>, now: SystemTime) {
        let result = document.as_ref().map_err(Clone::clone).and_then(|d| {
            self.changed = d.changed;
            evaluate(&d.value, &tile.field, tile.show)
        });
        match result {
            Ok(value) => {
                if let Some(n) = value.number().filter(|_| tile.show == Show::Graph) {
                    if self.history.len() == HISTORY {
                        self.history.pop_front();
                    }
                    self.history.push_back(n);
                }
                self.value = Some(value);
                self.read_at = Some(now);
                self.problem = None;
            }
            Err(problem) => self.problem = Some(problem),
        }
    }
}

/// What the widget shows: a reading for each tile it was read for.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct State {
    pub tiles: Vec<Tile>,
    pub readings: Vec<Reading>,
}
impl State {
    /// A state for `tiles`, keeping the readings of tiles `previous` had.
    pub fn carried(tiles: &[Tile], previous: &State) -> Self {
        Self {
            tiles: tiles.to_vec(),
            readings: tiles
                .iter()
                .map(|tile| {
                    previous
                        .tiles
                        .iter()
                        .position(|t| t == tile)
                        .and_then(|i| previous.readings.get(i).cloned())
                        .unwrap_or_default()
                })
                .collect(),
        }
    }
    pub fn reading(&self, index: usize) -> Option<&Reading> {
        self.readings.get(index)
    }
    fn apply(&mut self, path: &Path, document: &Result<Document, Problem>, now: SystemTime) {
        for (tile, reading) in self.tiles.iter().zip(self.readings.iter_mut()) {
            if tile.path() == path {
                reading.apply(tile, document, now);
            }
        }
    }
}

/// Each file once, at the shortest interval of the tiles reading it.
pub fn sources(tiles: &[Tile]) -> BTreeMap<PathBuf, Duration> {
    let mut out = BTreeMap::<PathBuf, Duration>::new();
    for tile in tiles {
        out.entry(tile.path().to_owned())
            .and_modify(|every| *every = (*every).min(tile.every()))
            .or_insert(tile.every());
    }
    out
}

/// One read of every file: what Settings shows while its page is open.
pub async fn read_all(tiles: Vec<Tile>, previous: State) -> State {
    let mut state = State::carried(&tiles, &previous);
    for path in sources(&tiles).into_keys() {
        let document = read(path.clone()).await;
        state.apply(&path, &document, SystemTime::now());
    }
    state
}

struct Feed {
    tiles: Vec<Tile>,
    seed: State,
}
impl std::hash::Hash for Feed {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        serde_json::to_string(&self.tiles)
            .unwrap_or_default()
            .hash(state);
    }
}

/// Reads each file when due and publishes the state after each round.
pub fn subscription(tiles: Vec<Tile>, seed: State) -> Subscription<State> {
    Subscription::run_with(Feed { tiles, seed }, |feed| {
        let mut state = State::carried(&feed.tiles, &feed.seed);
        let sources = sources(&feed.tiles);
        cosmic::iced::stream::channel::<State>(4, async move |mut output| {
            let start = tokio::time::Instant::now();
            let mut due: BTreeMap<PathBuf, tokio::time::Instant> =
                sources.keys().map(|path| (path.clone(), start)).collect();
            loop {
                let now = tokio::time::Instant::now();
                for (path, every) in &sources {
                    if due[path] <= now {
                        let document = read(path.clone()).await;
                        state.apply(path, &document, SystemTime::now());
                        due.insert(path.clone(), now + *every);
                    }
                }
                if output.send(state.clone()).await.is_err() {
                    break;
                }
                let Some(next) = due.values().min().copied() else {
                    break;
                };
                tokio::time::sleep_until(next).await;
            }
        })
    })
}

/// A starting point for a new tile: what it shows and the field it
/// usually reads.
pub struct Template {
    pub name: &'static str,
    pub label: &'static str,
    pub field: &'static str,
    pub show: Show,
    pub unit: &'static str,
}
pub const TEMPLATES: [Template; 4] = [
    Template {
        name: "Server status",
        label: "Server",
        field: "status",
        show: Show::Text,
        unit: "",
    },
    Template {
        name: "Build result",
        label: "Last build",
        field: "build.result",
        show: Show::Text,
        unit: "",
    },
    Template {
        name: "Download progress",
        label: "Download",
        field: "percent",
        show: Show::Progress { max: 100. },
        unit: "%",
    },
    Template {
        name: "Personal goal",
        label: "Steps today",
        field: "steps",
        show: Show::Gauge {
            min: 0.,
            max: 10000.,
        },
        unit: "steps",
    },
];

/// What the Settings preview shows: one tile of each kind.
pub fn sample() -> (Vec<Tile>, State) {
    let file = |name: &str| Source::File {
        path: PathBuf::from(format!("/Sample/{name}.json")),
    };
    let tiles = vec![
        Tile {
            label: "Server".into(),
            source: file("server"),
            field: "status".into(),
            show: Show::Text,
            unit: String::new(),
            every: 60,
        },
        Tile {
            label: "Download".into(),
            source: file("download"),
            field: "percent".into(),
            show: Show::Progress { max: 100. },
            unit: "%".into(),
            every: 10,
        },
        Tile {
            label: "Steps today".into(),
            source: file("goal"),
            field: "steps".into(),
            show: Show::Gauge {
                min: 0.,
                max: 10000.,
            },
            unit: "steps".into(),
            every: 300,
        },
        Tile {
            label: "Queue".into(),
            source: file("server"),
            field: "queue".into(),
            show: Show::Graph,
            unit: "jobs".into(),
            every: 60,
        },
    ];
    let now = SystemTime::now();
    let reading = |value: Value, history: &[f64]| Reading {
        value: Some(value),
        read_at: Some(now),
        changed: Some(now),
        problem: None,
        history: history.iter().copied().collect(),
    };
    let readings = vec![
        reading(Value::Text("All systems normal".into()), &[]),
        reading(Value::Number(64.), &[]),
        reading(Value::Number(7240.), &[]),
        reading(
            Value::Number(9.),
            &[4., 6., 5., 8., 12., 9., 7., 10., 14., 11., 8., 9.],
        ),
    ];
    let state = State {
        tiles: tiles.clone(),
        readings,
    };
    (tiles, state)
}
