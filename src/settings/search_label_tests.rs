// Search results are named the way their screens are. Each result opens its
// page in the real widget tree, which must draw the result's label.
#[test]
fn search_results_use_the_words_on_their_screens() {
    let name = "search_results_use_the_words_on_their_screens";
    if desktop_test_child(
        name,
        DesktopEnv {
            app_config: true,
            fixture: true,
            ..Default::default()
        },
    ) {
        return;
    }
    let root = isolated_root();
    let mut app = open_isolated_editor(&root);
    let size = Size::new(1280., 800.);
    send_settings(&mut app, Message::Resized(size));
    // Daily overview shows its Look section only while it is on.
    send_settings(&mut app, Message::Widget(Widget::Glance, true));
    let mut missing = Vec::new();
    for hit in navigation::all_search_hits() {
        // A label the screen draws only in a state the fixture lacks.
        if navigation::UNCHECKED_LABELS
            .iter()
            .any(|(label, _)| *label == hit.label)
        {
            continue;
        }
        send_settings(&mut app, Message::SearchResult(*hit));
        let mut native = crate::native_test::Native::new(size);
        if native.text_all(app.settings_view(), hit.label).is_empty() {
            missing.push(format!("{:?} “{}”", hit.page, hit.label));
        }
    }
    assert!(missing.is_empty(), "not on screen: {missing:#?}");
}

// Where a result sits reads as a path, never naming a step twice.
#[test]
fn search_result_paths_never_repeat_a_step() {
    for hit in navigation::all_search_hits() {
        let crumb = hit.crumb();
        let steps: Vec<&str> = crumb.split(" › ").collect();
        for pair in steps.windows(2) {
            assert_ne!(pair[0], pair[1], "{} · {crumb}", hit.label);
        }
        assert!(!steps.contains(&hit.label), "{} · {crumb}", hit.label);
    }
    let crumb = |query: &str, label: &str| {
        navigation::search(query)
            .into_iter()
            .find(|hit| hit.label == label)
            .unwrap_or_else(|| panic!("{query} finds no {label}"))
            .crumb()
    };
    assert_eq!(crumb("dark", "Mode"), "Appearance");
    assert_eq!(crumb("dock", "Dock"), "Dock and panel");
    assert_eq!(
        crumb("dock spacing", "Spacing"),
        "Dock and panel › Dock › Spacing and timing"
    );
    assert_eq!(
        crumb("window corners", "Window corners"),
        "Windows › Corners, outline color and shadows"
    );
}
