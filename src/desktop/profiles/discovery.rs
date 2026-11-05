//! Offline curated sources. Metadata is attribution, never a trust decision.
use super::{
    Preset, Profile,
    packages::{Attribution, Credits, Origin, Package, Revision, digest},
};
pub const CURATED: [&str; 8] = [
    "tidal",
    "paper-studio",
    "carbon",
    "ridge",
    "spine",
    "rain",
    "snow",
    "fog",
];
pub fn credited(mut preset: Preset) -> Preset {
    if CURATED.contains(&preset.id) {
        let source = Package::curated(
            &preset.profile,
            preset.id,
            Credits {
                author: "Halcyon".into(),
                license: "GPL-3.0-or-later".into(),
                source: "https://github.com/profilaz3/halcyon/blob/main/ASSETS.md".into(),
            },
        )
        .expect("curated source is valid");
        preset.profile.package = Some(source.origin);
        preset.profile.version = preset.profile.written_version();
    }
    preset
}
pub fn matches(preset: &Preset, query: &str) -> bool {
    let haystack = format!(
        "{} {} {} {}",
        preset.profile.name,
        preset.id.replace('-', " "),
        preset.description,
        super::occasion(preset.id)
    )
    .to_lowercase();
    query
        .split_whitespace()
        .all(|word| haystack.contains(&word.to_lowercase()))
}
pub fn remix_origin(
    profile: &Profile,
    origins: &[&Origin],
    picture: Option<String>,
) -> Result<Option<Origin>, String> {
    let mut ancestors: Vec<Attribution> = Vec::new();
    for origin in origins {
        for source in origin.lineage() {
            if let Some(existing) = ancestors.iter().find(|s| s.identity == source.identity) {
                if existing.credits != source.credits {
                    return Err("Conflicting credits for the same source revision".into());
                }
            } else {
                ancestors.push(source);
            }
        }
    }
    if ancestors.is_empty() {
        return Ok(None);
    }
    if ancestors.len() > 16 {
        return Err("This remix has too many source revisions".into());
    }
    let bytes =
        serde_json::to_vec(&(profile.portable().0, &ancestors)).map_err(|e| e.to_string())?;
    let revision = digest(&bytes);
    let origin = Origin {
        identity: Revision {
            id: format!("s-{}", &revision[..32]),
            revision,
        },
        credits: Credits::default(),
        parent: None,
        picture,
        ancestors,
    };
    origin.validate()?;
    Ok(Some(origin))
}
#[cfg(test)]
mod tests;
