//! Frozen local composition. Metadata describes origins; it never loads them.
use super::{PanelLook, Profile, ProfileId, export};
use crate::desktop::store::widget::{self, WidgetGroup};
use serde::{Deserialize, Serialize};
use serde_json::json;
use sha2::{Digest, Sha256};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Component {
    Picture,
    Style,
    Layout,
}
impl Component {
    pub const ALL: [Self; 3] = [Self::Picture, Self::Style, Self::Layout];
    pub fn index(self) -> usize {
        match self {
            Self::Picture => 0,
            Self::Style => 1,
            Self::Layout => 2,
        }
    }
    pub fn title(self) -> &'static str {
        match self {
            Self::Picture => "Picture",
            Self::Style => "Style",
            Self::Layout => "Layout",
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SourceKind {
    Builtin,
    Library,
    Desktop,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Origin {
    pub kind: SourceKind,
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    /// Digest of the portable component, not an asset hash or trust signature.
    pub fingerprint: String,
}
impl Origin {
    fn validate(&self) -> Result<(), String> {
        if !super::format::valid_name(&self.name)
            || self.fingerprint.len() != 64
            || !self
                .fingerprint
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
            || self
                .id
                .as_ref()
                .is_some_and(|id| ProfileId::parse(id).is_err())
            || (self.kind == SourceKind::Desktop && self.id.is_some())
        {
            return Err("Invalid composition source".into());
        }
        Ok(())
    }
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Provenance {
    pub picture: Origin,
    pub style: Origin,
    pub layout: Origin,
}
impl Provenance {
    pub fn validate(&self) -> Result<(), String> {
        for origin in [&self.picture, &self.style, &self.layout] {
            origin.validate()?;
        }
        Ok(())
    }
    pub fn has_local_ids(&self) -> bool {
        [&self.picture, &self.style, &self.layout]
            .iter()
            .any(|o| o.kind == SourceKind::Library && o.id.is_some())
    }
    pub fn strip_local_ids(&mut self) -> bool {
        let mut stripped = false;
        for origin in [&mut self.picture, &mut self.style, &mut self.layout] {
            if origin.kind == SourceKind::Library {
                stripped |= origin.id.take().is_some();
            }
        }
        stripped
    }
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Source {
    pub origin: Origin,
    pub profile: Profile,
}
impl Source {
    pub fn freeze(
        profile: &Profile,
        component: Component,
        kind: SourceKind,
        id: Option<String>,
    ) -> Result<Self, String> {
        profile.validate().map_err(|e| e.to_string())?;
        let projected = project(profile, component)?;
        let portable = export(&projected).map_err(|e| e.to_string())?;
        let origin = Origin {
            kind,
            name: profile.name.clone(),
            id,
            fingerprint: Sha256::digest(&portable.bytes)
                .iter()
                .map(|b| format!("{b:02x}"))
                .collect(),
        };
        origin.validate()?;
        Ok(Self {
            origin,
            profile: projected,
        })
    }
}

/// A source contributes only its component. Omitted fields keep the desktop's
/// current value through the existing planner, not a guessed default.
pub fn project(source: &Profile, component: Component) -> Result<Profile, String> {
    let mut p = Profile::new(&source.name).ok_or("Invalid source name")?;
    p.package = source.package.clone();
    if component != Component::Picture {
        if let Some(origin) = &mut p.package {
            origin.picture = None;
        }
    }
    match component {
        Component::Picture => {
            p.wallpaper = source
                .wallpaper
                .as_ref()
                .and_then(|look| look.background.clone())
                .map(|background| super::WallpaperLook {
                    background: Some(background),
                    ..Default::default()
                });
        }
        Component::Style => {
            p.appearance = source.appearance.clone();
            p.dock = source.dock.as_ref().map(style_bar);
            p.panel = source.panel.as_ref().map(style_bar);
            if let Some(style) = &source.widget_style {
                let mut selected = json!({});
                for field in widget::FIELDS
                    .iter()
                    .filter(|f| f.group == WidgetGroup::Style && style_field(f.pointer))
                {
                    if let Some(value) = style.pointer(field.pointer) {
                        widget::put(&mut selected, field.pointer, Some(value.clone()))?;
                    }
                }
                p.widget_style = Some(selected);
            }
        }
        Component::Layout => {
            p.widget_layout = source.widget_layout.clone();
            p.dock = source.dock.as_ref().map(layout_bar);
            p.panel = source.panel.as_ref().map(layout_bar);
        }
    }
    p.version = p.written_version();
    p.validate().map_err(|e| e.to_string())?;
    Ok(p)
}
fn style_field(pointer: &str) -> bool {
    !pointer.starts_with("/devbox/")
        && !matches!(
            pointer,
            "/clock/format_24h"
                | "/clock/show_date"
                | "/appearance/contrast_assist"
                | "/material_motion"
                | "/calendar_show_today"
                | "/calendar_show_tomorrow"
                | "/glance/show_tomorrow"
        )
}
fn style_bar(source: &PanelLook) -> PanelLook {
    PanelLook {
        opacity: source.opacity,
        border_radius: source.border_radius,
        background: source.background.clone(),
        keep_style: source.keep_style,
        ..Default::default()
    }
}
fn layout_bar(source: &PanelLook) -> PanelLook {
    let mut bar = source.clone();
    bar.opacity = None;
    bar.border_radius = None;
    bar.background = None;
    bar.keep_style = None;
    if let Some(islands) = &mut bar.islands {
        islands.outputs.clear();
    }
    bar
}
fn merge_bar(style: Option<PanelLook>, layout: Option<PanelLook>) -> Option<PanelLook> {
    match (style, layout) {
        (None, layout) => layout,
        (Some(style), layout) => {
            let mut bar = layout.unwrap_or_default();
            bar.opacity = style.opacity;
            bar.border_radius = style.border_radius;
            bar.background = style.background;
            bar.keep_style = style.keep_style;
            Some(bar)
        }
    }
}
pub fn compose(name: &str, sources: &[Source; 3]) -> Result<Profile, String> {
    let mut p = Profile::new(name).ok_or("Use a name with 1–80 characters")?;
    // Re-project defensively: callers cannot smuggle another domain in a source.
    let picture = project(&sources[0].profile, Component::Picture)?;
    let style = project(&sources[1].profile, Component::Style)?;
    let layout = project(&sources[2].profile, Component::Layout)?;
    p.wallpaper = picture.wallpaper;
    p.appearance = style.appearance;
    p.widget_style = style.widget_style;
    p.widget_layout = layout.widget_layout;
    p.dock = merge_bar(style.dock, layout.dock);
    p.panel = merge_bar(style.panel, layout.panel);
    p.composition = Some(Provenance {
        picture: sources[0].origin.clone(),
        style: sources[1].origin.clone(),
        layout: sources[2].origin.clone(),
    });
    let origins = sources
        .iter()
        .filter_map(|s| s.profile.package.as_ref())
        .collect::<Vec<_>>();
    p.package = super::discovery::remix_origin(
        &p,
        &origins,
        sources[0]
            .profile
            .package
            .as_ref()
            .and_then(|p| p.picture.clone()),
    )?;
    p.version = p.written_version();
    p.validate().map_err(|e| e.to_string())?;
    Ok(p)
}

#[cfg(test)]
mod tests;
