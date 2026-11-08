//! What changed for people using Halcyon, newest release first. Only what
//! someone would notice or want to try; fixes and internals stay in the
//! changelog. A note that has a place to set it up links straight there.
use super::navigation::{self, SearchHit};

pub(super) struct Note {
    pub title: &'static str,
    pub body: &'static str,
    /// Where to turn it on or adjust it, opened like a search result.
    pub place: Option<SearchHit>,
}

pub(super) struct Release {
    pub version: &'static str,
    pub notes: &'static [Note],
}

pub(super) const RELEASES: &[Release] = &[
    Release {
        version: "0.6.1",
        notes: &[
            Note {
                title: "Usage reports go straight to Halcyon",
                body: "If you share usage data, reports now go directly to Halcyon's own collector.",
                place: Some(navigation::PRIVACY),
            },
            Note {
                title: "Which features are on",
                body: "The daily report also says which main features are switched on. Never what's in your widgets.",
                place: Some(navigation::PRIVACY),
            },
            Note {
                title: "Public statistics",
                body: "Totals from all reports are public at halcyon-reports.profilaz3.workers.dev.",
                place: None,
            },
        ],
    },
    Release {
        version: "0.6.0",
        notes: &[
            Note {
                title: "Activity island (beta)",
                body: "Music, meetings, notifications and camera or mic use, in the top panel. Needs Islands.",
                place: Some(navigation::ACTIVITY_ISLAND),
            },
            Note {
                title: "Rain, snow and fog looks",
                body: "Three new looks, each with its own wallpaper, colors and weather effect.",
                place: Some(navigation::LOOKS),
            },
            Note {
                title: "Tidier built-in looks",
                body: "Time and weather together top left; system monitor and music on the right.",
                place: Some(navigation::LOOKS),
            },
            Note {
                title: "Much more see-through",
                body: "Widgets can now be up to 90% see-through.",
                place: Some(navigation::SEE_THROUGH),
            },
            Note {
                title: "Settings, rearranged",
                body: "Organized by what you change, and Back returns where you came from.",
                place: None,
            },
            Note {
                title: "Set up again, step by step",
                body: "Pick a look, bars, widgets and city, then review once before applying.",
                place: Some(navigation::STARTUP),
            },
        ],
    },
];

/// The notes for the version that is running, if it has any.
pub(super) fn current() -> Option<&'static Release> {
    RELEASES
        .iter()
        .find(|release| release.version == env!("CARGO_PKG_VERSION"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_running_version_has_notes_and_they_read_plainly() {
        // A version bump needs its notes before it builds a release.
        let release = current().expect("notes for this version");
        assert_eq!(RELEASES[0].version, release.version, "newest first");
        assert!(!release.notes.is_empty() && release.notes.len() <= 8);
        let mut versions: Vec<_> = RELEASES.iter().map(|r| r.version).collect();
        versions.dedup();
        assert_eq!(versions.len(), RELEASES.len());
        for note in RELEASES.iter().flat_map(|r| r.notes) {
            // One short line each: what it is, not how it works.
            assert!(!note.title.is_empty() && note.body.len() <= 100, "{}", note.title);
            for text in [note.title, note.body] {
                assert!(!text.contains(['\u{2013}', '\u{2014}']), "{text}");
                assert!(!text.contains("  "), "{text}");
            }
        }
    }

    #[test]
    fn every_link_is_also_found_by_search() {
        for note in RELEASES.iter().flat_map(|r| r.notes) {
            if let Some(place) = note.place {
                assert!(
                    navigation::search(place.label).contains(&place),
                    "{} links to a place search cannot find",
                    note.title
                );
            }
        }
    }
}
