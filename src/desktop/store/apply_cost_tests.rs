//! What one Apply costs on disk: fsync calls, the Undo record's size and
//! the time a realistic 29-key change takes on a fixture.
use super::tests::Fixture;
use super::*;

const DOCK: &str = "com.system76.CosmicPanel.Dock";
const PANEL: &str = "com.system76.CosmicPanel.Panel";

/// Fifteen Dock and fourteen Panel keys, as a style with bar geometry writes.
fn change(round: u32) -> DesktopPatch {
    let mut patch = DesktopPatch::new();
    let previous = |namespace: &str, key: &str| -> Option<RawValue> {
        round.checked_sub(1).map(|r| values(r, namespace, key))
    };
    for namespace in [DOCK, PANEL] {
        for &key in keys(namespace) {
            patch.set(
                ConfigKey::new(namespace, 1, key).unwrap(),
                previous(namespace, key),
                Some(values(round, namespace, key)),
            );
        }
    }
    patch
}
fn keys(namespace: &str) -> &'static [&'static str] {
    const ALL: [&str; 15] = [
        "opacity",
        "border_radius",
        "spacing",
        "padding",
        "margin",
        "anchor_gap",
        "expand_to_edges",
        "exclusive_zone",
        "keep_style_on_maximize",
        "size",
        "background",
        "autohide",
        "plugins_center",
        "plugins_wings",
        "output",
    ];
    if namespace == DOCK { &ALL } else { &ALL[..14] }
}
fn values(round: u32, namespace: &str, key: &str) -> RawValue {
    let flag = if (round + namespace.len() as u32) % 2 == 0 {
        "true"
    } else {
        "false"
    };
    let text = match key {
        "opacity" => format!("0.{}", 50 + round % 40),
        "border_radius" => (round % 20).to_string(),
        "spacing" => (round % 9).to_string(),
        "padding" => (round % 7).to_string(),
        "margin" => (round % 5).to_string(),
        "anchor_gap" | "expand_to_edges" | "exclusive_zone" | "keep_style_on_maximize" => {
            flag.to_string()
        }
        "size" => ["XS", "S", "M", "L", "XL"][round as usize % 5].to_string(),
        "background" => ["ThemeDefault", "Dark", "Light"][round as usize % 3].to_string(),
        "autohide" => "Never".into(),
        "plugins_center" => format!(
            "Some([\"com.system76.CosmicAppList\", \"com.example.Applet{}\"])",
            round % 4
        ),
        "plugins_wings" => format!(
            "Some(([\"com.system76.CosmicPanelWorkspacesButton\", \"com.example.Left{}\"], [\"com.system76.CosmicAppletTime\", \"com.example.Right{}\"]))",
            round % 3,
            round % 5
        ),
        "output" => "All".into(),
        _ => unreachable!(),
    };
    RawValue::parse(&text).unwrap()
}

/// Applies round `round` as the Dock and panel page does.
fn apply(store: &DesktopStore, round: u32) -> std::time::Duration {
    let started = std::time::Instant::now();
    store
        .apply_action(
            &change(round),
            None,
            Owner::DockPanel,
            "Dock and panel",
            Source::Manual,
        )
        .unwrap();
    started.elapsed()
}

/// One 29-key Apply syncs each new file once and each touched folder once:
/// the change journal (file, folder), 29 values, their two bar folders, the
/// created-key inventory (file, folder; only when keys are new), the Undo
/// record (file, folder) and the journal's removal. Syncing the folder
/// after every key, as before, cost 62 syncs for the same Apply.
#[test]
fn a_29_key_apply_syncs_each_file_and_each_folder_once() {
    let fixture = Fixture::new("apply-cost");
    let store = fixture.store();
    apply(&store, 0);
    let before = super::super::recovery::SYNCS.with(|count| count.get());
    apply(&store, 1);
    let syncs = super::super::recovery::SYNCS.with(|count| count.get()) - before;
    assert!(
        syncs <= 29 + 2 + 2 + 2 + 1,
        "{syncs} syncs for one 29-key Apply"
    );
}

/// The Undo record is written compactly; older, pretty-printed records
/// still load.
#[test]
fn the_undo_record_is_compact_and_pretty_records_still_read() {
    let fixture = Fixture::new("apply-compact-undo");
    let store = fixture.store();
    for round in 0..3 {
        apply(&store, round);
    }
    let path = store.undo_path();
    let text = fs::read_to_string(&path).unwrap();
    assert!(!text.contains("\n  "), "compact record");
    let value: serde_json::Value = serde_json::from_str(&text).unwrap();
    fs::write(&path, serde_json::to_vec_pretty(&value).unwrap()).unwrap();
    let latest = store.latest_change().unwrap().unwrap();
    assert!(!latest.change.entries.is_empty());
    apply(&store, 3);
    assert!(!fs::read_to_string(&path).unwrap().contains("\n  "));
}

/// Apply time with a long history, for the record. Run with
/// `--ignored --nocapture` to print the numbers.
#[test]
#[ignore = "measurement"]
fn measure_apply_time_with_a_long_history() {
    let fixture = Fixture::new("apply-measure");
    let store = fixture.store();
    let rounds = 60;
    for round in 0..rounds {
        apply(&store, round);
    }
    let size = fs::metadata(store.undo_path()).unwrap().len();
    let mut times = Vec::new();
    for round in rounds..rounds + 20 {
        let before = super::super::recovery::SYNCS.with(|count| count.get());
        let time = apply(&store, round);
        let syncs = super::super::recovery::SYNCS.with(|count| count.get()) - before;
        times.push((time, syncs));
    }
    times.sort();
    let median = times[times.len() / 2];
    println!(
        "undo record {size} bytes; 29-key apply median {:?}, min {:?}, max {:?}; {} syncs",
        median.0,
        times[0].0,
        times[times.len() - 1].0,
        median.1
    );
}
