//! Sample icons drawn the way a look shows them: app icons in its shape,
//! folders in its accent, through the same code that builds Halcyon's
//! overlay. Pictures are drawn off the UI thread and kept by what they
//! show; until one is ready the plain icon file stands in.
use crate::desktop::icon_packs::overlay::{self, Shape, Style};
use cosmic::widget::image::Handle;
use std::path::PathBuf;

/// What overlay change a sample icon takes.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Kind {
    /// Tinted with the folder tint.
    Folder,
    /// Left as it is (a document).
    File,
    /// Masked to the app icon shape.
    App,
}

/// One sample icon file from a theme.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Sample {
    pub path: PathBuf,
    pub kind: Kind,
}

/// The sample icons a look's Icons picture shows, in order.
const SAMPLES: [(&str, Kind); 5] = [
    ("folder", Kind::Folder),
    ("text-x-generic", Kind::File),
    ("utilities-terminal", Kind::App),
    ("web-browser", Kind::App),
    ("preferences-system", Kind::App),
];

/// A few everyday icons from an icon theme, for its sample. Never an
/// overlay: the caller names its base.
pub fn samples(theme: &str) -> Vec<Sample> {
    if overlay::is_overlay(theme) {
        return Vec::new();
    }
    SAMPLES
        .into_iter()
        .filter_map(|(name, kind)| {
            let path = freedesktop_icons::lookup(name)
                .with_theme(theme)
                .with_size(32)
                .with_cache()
                .find()?;
            Some(Sample { path, kind })
        })
        .collect()
}

/// A sample as shown: drawn in its shape or tint, or the plain file.
#[derive(Clone, Debug)]
pub enum Picture {
    Drawn(Handle),
    File(PathBuf),
}

impl Picture {
    /// The picture as an icon of `size` pixels.
    pub fn icon<'a, M: 'a>(&self, size: u16) -> cosmic::Element<'a, M> {
        match self {
            Self::Drawn(handle) => cosmic::widget::image(handle.clone())
                .width(cosmic::iced::Length::Fixed(size as f32))
                .height(cosmic::iced::Length::Fixed(size as f32))
                .into(),
            Self::File(path) => cosmic::widget::icon::from_path(path.clone())
                .icon()
                .size(size)
                .into(),
        }
    }
}

/// A drawn sample: the file and exactly what was done to it.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Key {
    pub path: PathBuf,
    pub shape: Shape,
    pub tint: Option<[u8; 3]>,
    pub dark: bool,
}

/// Pixel size the samples are drawn at (twice their largest shown size).
const SIZE: u32 = 80;

impl Key {
    /// The picture `sample` takes under `style`, or `None` when the file is
    /// shown as it is.
    pub fn new(sample: &Sample, style: &Style, accent: [u8; 3], dark: bool) -> Option<Self> {
        let path = sample.path.clone();
        match sample.kind {
            Kind::App if style.shape != Shape::Original => Some(Self {
                path,
                shape: style.shape,
                tint: None,
                dark,
            }),
            Kind::Folder if style.tint => Some(Self {
                path,
                shape: Shape::Original,
                tint: Some(accent),
                dark: false,
            }),
            _ => None,
        }
    }
    /// Draws the picture. Blocking.
    pub fn draw(&self) -> Option<Handle> {
        let picture = match self.tint {
            Some(accent) => overlay::preview_folder(&self.path, Some(accent), SIZE)?,
            None => overlay::preview(&self.path, self.shape, self.dark, SIZE)?,
        };
        let (width, height) = picture.dimensions();
        Some(Handle::from_rgba(width, height, picture.into_raw()))
    }
}

/// Pictures to draw, as one background job.
#[derive(Clone, Debug)]
pub struct Job {
    pub keys: Vec<Key>,
}

impl Job {
    /// Draws every picture. Blocking.
    pub fn run(self) -> Vec<(Key, Option<Handle>)> {
        self.keys
            .into_iter()
            .map(|key| {
                let handle = key.draw();
                (key, handle)
            })
            .collect()
    }
}

/// The 8-bit accent a preview's folders take.
pub fn accent_of(appearance: Option<&crate::desktop::appearance::AppearanceDraft>) -> [u8; 3] {
    use crate::desktop::appearance::ColorRole;
    use crate::desktop::panels::Setting;
    let Some(draft) = appearance else {
        return [0x63, 0xd0, 0xdf];
    };
    let variant = draft.active_variant();
    let color = match draft.color(variant, ColorRole::Accent) {
        Setting::Known(Some(color)) => color,
        _ => draft.preview(variant).accent,
    };
    [color.red, color.green, color.blue].map(|c| (c.clamp(0., 1.) * 255.).round() as u8)
}

/// Words for a style's shape and tint ("circle", "accent folders").
pub fn words(style: &Style) -> Vec<String> {
    let mut items = Vec::new();
    if style.shape != Shape::Original {
        items.push(style.shape.title().to_lowercase());
    }
    if style.tint {
        items.push("accent folders".into());
    }
    items
}
