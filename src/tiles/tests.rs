use super::*;
use serde_json::json;

fn tile(path: &str, field: &str, show: Show, every: u32) -> Tile {
    Tile {
        label: "Build".into(),
        source: Source::File { path: path.into() },
        field: field.into(),
        show,
        unit: String::new(),
        every,
    }
}

#[test]
fn saved_tiles_are_checked_and_bad_ones_dropped() {
    let good = json!({"label": "Build", "source": {"kind": "file", "path": "/tmp/b.json"},
        "field": "build.result", "show": {"kind": "text"}, "every": 60});
    let prefs: Preferences = serde_json::from_value(json!({"enabled": true, "tiles": [
        good,
        {"label": "Relative", "source": {"kind": "file", "path": "b.json"},
            "show": {"kind": "text"}, "every": 60},
        {"label": "Odd interval", "source": {"kind": "file", "path": "/b.json"},
            "show": {"kind": "text"}, "every": 7},
        {"label": "Empty range", "source": {"kind": "file", "path": "/b.json"},
            "show": {"kind": "gauge", "min": 5, "max": 5}, "every": 60},
        {"label": "Command", "source": {"kind": "command", "run": "sh"},
            "show": {"kind": "text"}, "every": 60},
        {"label": "Extra", "source": {"kind": "file", "path": "/b.json"},
            "show": {"kind": "text"}, "every": 60, "script": "x"},
    ]}))
    .unwrap();
    assert_eq!(prefs.tiles.len(), 1);
    assert_eq!(prefs.tiles[0].field, "build.result");
    assert!(valid_tiles(&json!([good])));
    assert!(!valid_tiles(
        &json!([good, {"label": "", "source": {"kind": "file", "path": "/a"},
        "show": {"kind": "text"}, "every": 60}])
    ));
    assert!(!valid_tiles(&json!(vec![good.clone(); MAX_TILES + 1])));
    let many: Preferences =
        serde_json::from_value(json!({"tiles": vec![good; MAX_TILES + 2]})).unwrap();
    assert_eq!(many.tiles.len(), MAX_TILES);
    assert!(
        !tile("/a", "jobs..state", Show::Text, 60).valid(),
        "an empty step in a path"
    );
}

#[test]
fn fields_are_found_by_dotted_path_and_shaped_for_how_they_show() {
    let doc = json!({
        "build": {"result": "passed", "ok": true},
        "jobs": [{"state": "queued"}, {"state": "done"}],
        "percent": "42.5",
        "cpu": 31,
        "notes": "x".repeat(200),
        "empty": null,
    });
    let text = |field| evaluate(&doc, field, Show::Text);
    let bar = |field| evaluate(&doc, field, Show::Progress { max: 100. });
    assert_eq!(text("build.result"), Ok(Value::Text("passed".into())));
    assert_eq!(text("build.ok").unwrap().text(), "Yes");
    assert_eq!(text("jobs.1.state"), Ok(Value::Text("done".into())));
    assert_eq!(text("jobs.2.state"), Err(Problem::NoField));
    assert_eq!(text("jobs.x"), Err(Problem::NoField));
    assert_eq!(text("build.result.more"), Err(Problem::NoField));
    assert_eq!(text("jobs"), Err(Problem::NotValue));
    assert_eq!(text("empty"), Err(Problem::NotValue));
    assert_eq!(text("notes").unwrap().text().chars().count(), 80);
    assert_eq!(
        bar("percent"),
        Ok(Value::Number(42.5)),
        "a number written as text"
    );
    assert_eq!(bar("cpu"), Ok(Value::Number(31.)));
    assert_eq!(bar("build.result"), Err(Problem::NotNumber));
    assert_eq!(bar("build.ok"), Err(Problem::NotNumber));
    assert_eq!(evaluate(&json!(7), "", Show::Graph), Ok(Value::Number(7.)));
    for (n, shown) in [
        (42., "42"),
        (42.5, "42.5"),
        (3.14159, "3.14"),
        (123.45, "123.5"),
        (12345.6, "12346"),
        (-0.001, "0"),
    ] {
        assert_eq!(number(n), shown);
    }
}

#[test]
fn files_are_read_only_when_regular_small_and_json() {
    let dir = crate::test_env::TempPath::new("tiles-read");
    let file = dir.root().join("status.json");
    std::fs::write(&file, r#"{"status": "up"}"#).unwrap();
    let document = read_file(&file).unwrap();
    assert_eq!(document.value, json!({"status": "up"}));
    assert!(document.changed.is_some());
    assert_eq!(
        read_file(&dir.root().join("gone.json")).err(),
        Some(Problem::Missing)
    );
    assert_eq!(read_file(dir.root()).err(), Some(Problem::NotFile));
    let fifo = dir.root().join("pipe");
    let name = std::ffi::CString::new(fifo.as_os_str().as_encoded_bytes()).unwrap();
    // SAFETY: a valid C string naming a path in the test's own folder.
    assert_eq!(unsafe { libc::mkfifo(name.as_ptr(), 0o600) }, 0);
    // With no writer, a blocking open would never return.
    assert_eq!(read_file(&fifo).err(), Some(Problem::NotFile));
    std::fs::write(&file, "{\n  \"status\": \n}").unwrap();
    assert_eq!(read_file(&file).err(), Some(Problem::NotJson(3)));
    let big = format!("[{}0]", "0,".repeat(MAX_BYTES as usize / 2));
    std::fs::write(&file, big).unwrap();
    assert_eq!(read_file(&file).err(), Some(Problem::TooBig));
}

#[test]
fn settings_lists_the_fields_that_hold_one_value() {
    let doc = json!({
        "server": {"name": "nas", "load": 0.4, "disks": [{"free": 10}, {"free": 20},
            {"free": 30}, {"free": 40}]},
        "a.b": 1,
        "ok": false,
    });
    let found: Vec<String> = fields(&doc).into_iter().map(|(path, _)| path).collect();
    assert_eq!(
        found,
        [
            "server.name",
            "server.load",
            "server.disks.0.free",
            "server.disks.1.free",
            "server.disks.2.free",
            "ok",
        ],
        "in the file's order; a key with a dot can't be a path; a list shows three places"
    );
    let wide: serde_json::Map<_, _> = (0..100).map(|i| (format!("k{i:03}"), json!(i))).collect();
    assert_eq!(fields(&serde_json::Value::Object(wide)).len(), MAX_FIELDS);
}

#[test]
fn a_reading_says_when_its_value_may_be_old() {
    let file = tile("/a.json", "n", Show::Graph, 60);
    let now = SystemTime::now();
    let minutes = |m: u64| now - Duration::from_secs(m * 60);
    let doc = |n: i64, changed| {
        Ok(Document {
            value: json!({"n": n}),
            changed: Some(changed),
        })
    };
    let mut reading = Reading::default();
    reading.apply(&file, &doc(4, minutes(1)), minutes(1));
    assert_eq!(reading.value, Some(Value::Number(4.)));
    assert_eq!(reading.stale(&file, now), None);
    // Unchanged for three intervals.
    reading.apply(&file, &doc(4, minutes(5)), now);
    assert_eq!(reading.stale(&file, now), Some(Duration::from_secs(300)));
    // A failed read keeps the last value and says how old it is.
    reading.apply(&file, &doc(5, now), minutes(2));
    reading.apply(&file, &Err(Problem::Missing), now);
    assert_eq!(reading.value, Some(Value::Number(5.)));
    assert_eq!(reading.problem, Some(Problem::Missing));
    assert_eq!(reading.stale(&file, now), Some(Duration::from_secs(120)));
    for n in 0..100 {
        reading.apply(&file, &doc(n, now), now);
    }
    assert_eq!(reading.history.len(), HISTORY);
    assert_eq!(reading.history.back(), Some(&99.));
    let mut text = Reading::default();
    let words = tile("/a.json", "n", Show::Text, 60);
    text.apply(&words, &doc(1, now), now);
    assert!(text.history.is_empty(), "only a graph keeps history");
}

#[test]
fn tiles_reading_one_file_share_a_read_and_keep_readings_across_edits() {
    let a = tile("/a.json", "x", Show::Text, 300);
    let b = tile("/a.json", "y", Show::Graph, 30);
    let c = tile("/c.json", "z", Show::Text, 3600);
    assert_eq!(
        sources(&[a.clone(), b.clone(), c.clone()]),
        BTreeMap::from([
            (PathBuf::from("/a.json"), Duration::from_secs(30)),
            (PathBuf::from("/c.json"), Duration::from_secs(3600)),
        ])
    );
    let mut state = State::carried(&[a.clone(), b.clone()], &State::default());
    let now = SystemTime::now();
    state.apply(
        Path::new("/a.json"),
        &Ok(Document {
            value: json!({"x": "on", "y": 3}),
            changed: Some(now),
        }),
        now,
    );
    assert_eq!(state.readings[0].value, Some(Value::Text("on".into())));
    assert_eq!(state.readings[1].history, [3.]);
    // Adding a tile keeps the graph of the one that stayed.
    let next = State::carried(&[c, b], &state);
    assert_eq!(next.readings[0], Reading::default());
    assert_eq!(next.readings[1].history, [3.]);
}
