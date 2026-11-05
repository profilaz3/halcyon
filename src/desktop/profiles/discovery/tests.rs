use super::*;
use crate::desktop::profiles::{
    self,
    mix::{self, Component, Source, SourceKind},
};
fn source(id: &str) -> Profile {
    credited(
        profiles::studio_presets()
            .into_iter()
            .find(|p| p.id == id)
            .unwrap(),
    )
    .profile
}
#[test]
fn fb19_catalog_identity_is_stable_unfitted_and_revision_tracks_content() {
    for id in CURATED {
        let p = source(id);
        let origin = p.package.as_ref().unwrap();
        assert_eq!(origin, source(id).package.as_ref().unwrap());
        assert_eq!(origin.credits.author, "Halcyon");
        // Weather setups carry a wallpaper effect, which needs version 19.
        let expected = if matches!(id, "rain" | "snow" | "fog") { 19 } else { 14 };
        assert_eq!(p.written_version(), expected, "{id}");
        let mut changed = p.clone();
        changed.package = None;
        changed.name.push_str(" revised");
        let newer = Package::curated(&changed, id, origin.credits.clone()).unwrap();
        assert_eq!(newer.origin.identity.id, origin.identity.id);
        assert_ne!(newer.origin.identity.revision, origin.identity.revision);
        let fitted = profiles::fitted(
            credited(
                profiles::studio_presets()
                    .into_iter()
                    .find(|p| p.id == id)
                    .unwrap(),
            ),
            &crate::config::Config::default(),
            std::path::Path::new("/absent"),
            false,
        );
        assert_eq!(fitted.profile.package.as_ref(), Some(origin));
    }
}
#[test]
fn fb19_mix_retains_distinct_source_credits_without_local_ids() {
    let ids = ["tidal", "paper-studio", "spine"];
    let sources = Component::ALL.map(|part| {
        Source::freeze(
            &source(ids[part.index()]),
            part,
            SourceKind::Library,
            Some(format!("local-{}", part.index())),
        )
        .unwrap()
    });
    let p = mix::compose("My remix", &sources).unwrap();
    let origin = p.package.as_ref().unwrap();
    assert_eq!(origin.ancestors.len(), 3);
    assert_eq!(p.written_version(), 15);
    let portable = profiles::export(&p).unwrap();
    let restored = profiles::import(&portable.bytes).unwrap();
    assert_eq!(restored.package, p.package);
    assert!(!String::from_utf8_lossy(&portable.bytes).contains("local-"));
    assert_eq!(mix::compose("My remix", &sources).unwrap(), p);
    let (package, _) = Package::build(
        &p,
        false,
        Credits {
            author: "New creator".into(),
            license: "CC0-1.0".into(),
            source: String::new(),
        },
    )
    .unwrap();
    assert_eq!(package.version, 2);
    assert_eq!(package.origin.ancestors.len(), 4);
    assert_eq!(
        package
            .origin
            .ancestors
            .iter()
            .filter(|a| a.credits.author == "Halcyon")
            .count(),
        3
    );
    assert_eq!(Package::parse(&package.encode().unwrap()).unwrap(), package);
}
#[test]
fn fb19_lineage_bounds_conflicts_and_legacy_admission_are_explicit() {
    let p = source("tidal");
    let origin = p.package.as_ref().unwrap();
    let mut conflict = origin.clone();
    conflict.credits.author = "Changed claimant".into();
    assert!(remix_origin(&Profile::new("mix").unwrap(), &[origin, &conflict], None).is_err());
    let mut many = origin.clone();
    many.ancestors = (0..16)
        .map(|n| Attribution {
            identity: Revision {
                id: format!("s-{n:032x}"),
                revision: format!("{n:064x}"),
            },
            credits: Credits::default(),
        })
        .collect();
    assert!(many.validate().is_ok());
    let mut full = p.clone();
    full.package = Some(many);
    full.version = 15;
    assert!(Package::build(&full, false, Credits::default()).is_err());
    let sources = Component::ALL
        .map(|part| Source::freeze(&p, part, SourceKind::Builtin, Some("tidal".into())).unwrap());
    let mut remix = mix::compose("mix", &sources).unwrap();
    assert_eq!(remix.package.as_ref().unwrap().ancestors.len(), 1);
    remix.version = 14;
    assert!(remix.validate().is_err());
    let (mut envelope, _) = Package::build(&p, false, Credits::default()).unwrap();
    envelope.version = 1;
    assert!(envelope.validate().is_err());
    assert!(profiles::import(&profiles::export(&p).unwrap().bytes).is_ok());
}
#[test]
fn fb19_picture_provenance_survives_mix_but_never_leaks_into_other_parts() {
    let mut p = source("tidal");
    p.package.as_mut().unwrap().picture = Some("a".repeat(64));
    for part in Component::ALL {
        let projected = mix::project(&p, part).unwrap();
        assert_eq!(
            projected.package.as_ref().unwrap().picture.is_some(),
            part == Component::Picture
        );
    }
    let sources = Component::ALL
        .map(|part| Source::freeze(&p, part, SourceKind::Builtin, Some("tidal".into())).unwrap());
    assert_eq!(
        mix::compose("mix", &sources)
            .unwrap()
            .package
            .unwrap()
            .picture,
        Some("a".repeat(64))
    );
}
#[test]
fn fb19_search_matches_words_without_loading_or_network_requests() {
    let p = credited(
        profiles::studio_presets()
            .into_iter()
            .find(|p| p.id == "paper-studio")
            .unwrap(),
    );
    assert!(matches(&p, "PAPER studio"));
    assert!(matches(&p, ""));
    assert!(!matches(&p, "not-present"));
}
