//! What changed for people using Halcyon, newest release first. Only what
//! someone would notice or want to try; fixes and internals stay in the
//! changelog. A note that has a place to set it up links straight there.
use super::navigation::{self, SearchHit};

pub(super) struct Note {
    pub title: &'static str,
    pub body: &'static str,
    /// Where to turn it on or adjust it, opened like a search result.
    pub place: Option<SearchHit>,
    /// A web page the note names, opened in the browser.
    pub link: Option<&'static str>,
}

pub(super) struct Release {
    pub version: &'static str,
    pub notes: &'static [Note],
}

pub(super) const RELEASES: &[Release] = &[
    Release {
        version: "0.7.0",
        notes: &[
            Note {
                title: "Looks, part by part",
                body: "Take only the parts of a look you want, or make your own in the Look studio.",
                place: Some(navigation::LOOKS),
                link: None,
            },
            Note {
                title: "Rainy Lake and Deep Purple",
                body: "Two new ready-made themes, each with a matching look and wallpaper.",
                place: None,
                link: None,
            },
            Note {
                title: "Icon packs",
                body: "Get more icons, give app icons one shape and tint folders with your accent.",
                place: Some(navigation::GET_MORE_ICONS),
                link: None,
            },
            Note {
                title: "Edge menu",
                body: "Move it, show it as icons or a grid, or turn it off, and choose what it lists.",
                place: Some(navigation::EDGE_MENU),
                link: None,
            },
            Note {
                title: "Settings Home",
                body: "Settings opens on a picture of your desktop. Click a part to change it.",
                place: None,
                link: None,
            },
            Note {
                title: "Rain on glass",
                body: "Rain fogs the glass and runs down in natural drops, with the clock behind it.",
                place: Some(navigation::WEATHER_EFFECTS),
                link: None,
            },
            Note {
                title: "Activity island",
                body: "Grouped notifications, live progress, a quick timer and a shelf for files.",
                place: Some(navigation::ACTIVITY_ISLAND),
                link: None,
            },
            Note {
                title: "Devices and battery",
                body: "See the battery of headphones, mice and phones in the island and a widget.",
                place: Some(navigation::DEVICES),
                link: None,
            },
        ],
    },
    Release {
        version: "0.6.5",
        notes: &[
            Note {
                title: "Match the weather",
                body: "Rain, snow, fog and cloud shadows on your wallpaper can follow the Weather widget.",
                place: Some(navigation::MATCH_WEATHER),
                link: None,
            },
            Note {
                title: "Clock stage",
                body: "The Clock page shows a small copy of your screen. Drag the clock to move or resize it.",
                place: None,
                link: None,
            },
            Note {
                title: "The scene covers the clock",
                body: "Now on your own wallpapers too, and Scene lighting tints the digits with the sky.",
                place: Some(navigation::SCENE_COVERS_CLOCK),
                link: None,
            },
            Note {
                title: "Islands setup",
                body: "Pick Islands under Shape and Halcyon shows one clear next step to finish switching.",
                place: None,
                link: None,
            },
            Note {
                title: "Changes",
                body: "Your history by day, at the bottom of the sidebar. Undo any change on its own.",
                place: Some(navigation::CHANGES),
                link: None,
            },
            Note {
                title: "Time of day",
                body: "One page switches your whole look or just the widget style for Day, Evening and Night.",
                place: Some(navigation::TIME_OF_DAY),
                link: None,
            },
            Note {
                title: "Settings, tidied up",
                body: "Explanations behind a ?, search by the names you see, and every control works by keyboard.",
                place: None,
                link: None,
            },
            Note {
                title: "Lighter on your computer",
                body: "Much less CPU while idle and far less video memory.",
                place: None,
                link: None,
            },
        ],
    },
    Release {
        version: "0.6.1",
        notes: &[
            Note {
                title: "Usage reports go straight to Halcyon",
                body: "If you share usage data, reports now go directly to Halcyon's own collector.",
                place: Some(navigation::PRIVACY),
                link: None,
            },
            Note {
                title: "Which features are on",
                body: "The daily report also says which main features are switched on. Never what's in your widgets.",
                place: Some(navigation::PRIVACY),
                link: None,
            },
            Note {
                title: "Public statistics",
                body: "Totals from all reports are public at halcyon-reports.profilaz3.workers.dev.",
                place: None,
                link: Some("https://halcyon-reports.profilaz3.workers.dev"),
            },
        ],
    },
    Release {
        version: "0.6.0",
        notes: &[
            Note {
                title: "Activity pill (beta)",
                body: "Music, meetings, notifications and camera or mic use, in the top panel. Needs Islands.",
                place: Some(navigation::ACTIVITY_ISLAND),
                link: None,
            },
            Note {
                title: "Rain, snow and fog looks",
                body: "Three new looks, each with its own wallpaper, colors and weather effect.",
                place: Some(navigation::LOOKS),
                link: None,
            },
            Note {
                title: "Tidier built-in looks",
                body: "Time and weather together top left; system monitor and music on the right.",
                place: Some(navigation::LOOKS),
                link: None,
            },
            Note {
                title: "Much more see-through",
                body: "Widgets can now be up to 90% see-through.",
                place: Some(navigation::SEE_THROUGH),
                link: None,
            },
            Note {
                title: "Settings, rearranged",
                body: "Organized by what you change, and Back returns where you came from.",
                place: None,
                link: None,
            },
            Note {
                title: "Set up again, step by step",
                body: "Pick a look, bars, widgets and city, then review once before applying.",
                place: Some(navigation::STARTUP),
                link: None,
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

const SEEN_FILE: &str = "release-notes-seen";

/// The newest version whose notes were shown, kept beside Settings' other
/// state. None on a first run and in versions that never recorded it.
pub(super) fn seen() -> Option<String> {
    let store = crate::desktop::DesktopStore::from_env().ok()?;
    let text = std::fs::read_to_string(store.state_dir().join(SEEN_FILE)).ok()?;
    let text = text.trim();
    (!text.is_empty() && text.len() <= 32).then(|| text.to_owned())
}

/// Records the running version's notes as seen.
pub(super) fn record_seen() -> Result<(), String> {
    let store = crate::desktop::DesktopStore::from_env().map_err(|e| e.to_string())?;
    let path = store.state_file(SEEN_FILE).map_err(|e| e.to_string())?;
    std::fs::create_dir_all(store.state_dir()).map_err(|e| e.to_string())?;
    crate::config::write_config_text(&path, Some(env!("CARGO_PKG_VERSION")))
}

/// Every release newer than `seen` up to the running one, newest first.
/// With nothing recorded, only the running version's notes.
pub(super) fn since(seen: Option<&str>) -> Vec<&'static Release> {
    since_in(RELEASES, seen, env!("CARGO_PKG_VERSION"))
}

fn since_in(
    releases: &'static [Release],
    seen: Option<&str>,
    running: &str,
) -> Vec<&'static Release> {
    releases
        .iter()
        .filter(|release| match seen {
            Some(seen) => newer(release.version, seen) && !newer(release.version, running),
            None => release.version == running,
        })
        .collect()
}

/// Dotted version order; a part that isn't a number counts as 0.
fn newer(a: &str, b: &str) -> bool {
    let parts = |v: &str| -> Vec<u32> { v.split('.').map(|p| p.parse().unwrap_or(0)).collect() };
    parts(a) > parts(b)
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
    fn notes_since_the_last_seen_version_cover_every_release_in_between() {
        static THREE: &[Release] = &[
            Release {
                version: "0.7.0",
                notes: &[],
            },
            Release {
                version: "0.6.10",
                notes: &[],
            },
            Release {
                version: "0.6.2",
                notes: &[],
            },
        ];
        let versions = |seen, running| -> Vec<_> {
            since_in(THREE, seen, running)
                .iter()
                .map(|r| r.version)
                .collect()
        };
        assert_eq!(
            versions(Some("0.6.1"), "0.7.0"),
            ["0.7.0", "0.6.10", "0.6.2"]
        );
        assert_eq!(versions(Some("0.6.2"), "0.7.0"), ["0.7.0", "0.6.10"]);
        assert!(versions(Some("0.7.0"), "0.7.0").is_empty());
        // Notes for a version not yet running stay hidden.
        assert_eq!(versions(Some("0.6.1"), "0.6.10"), ["0.6.10", "0.6.2"]);
        // Nothing recorded: only what is running.
        assert_eq!(versions(None, "0.7.0"), ["0.7.0"]);
        assert_eq!(since(None).len(), 1);
        assert!(since(Some(env!("CARGO_PKG_VERSION"))).is_empty());
        for note in RELEASES.iter().flat_map(|r| r.notes) {
            assert!(note.link.is_none_or(|link| link.starts_with("https://")));
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
