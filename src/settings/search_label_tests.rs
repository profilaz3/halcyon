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
        "Dock and panel › Dock › Corners, spacing and timing"
    );
    assert_eq!(
        crumb("window corners", "Window corners"),
        "Windows › Corners, outline color and shadows"
    );
}

/// Every screen a search result can open: page, Halcyon tab, bar, object tab
/// and whether the page's fold is open.
fn every_screen() -> Vec<(navigation::SearchHit, navigation::ObjectTab)> {
    use crate::desktop::panels::PanelKind;
    let mut screens = Vec::new();
    for page in Page::ALL {
        let destinations: &[AppSettingsPage] = if page == Page::General {
            &[
                AppSettingsPage::Home,
                AppSettingsPage::Operation,
                AppSettingsPage::Interface,
                AppSettingsPage::Privacy,
                AppSettingsPage::Updates,
                AppSettingsPage::WhatsNew,
                AppSettingsPage::Help,
            ]
        } else {
            &[AppSettingsPage::Operation]
        };
        let bars: &[Option<PanelKind>] = if page == Page::DesktopDock {
            &[Some(PanelKind::Dock), Some(PanelKind::Panel)]
        } else {
            &[None]
        };
        for &destination in destinations {
            for &bar in bars {
                for advanced in [false, true] {
                    for tab in navigation::ObjectTab::ALL {
                        screens.push((navigation::probe(page, destination, bar, advanced), tab));
                    }
                }
            }
        }
    }
    screens
}

// Search knows every row title and heading the pages draw: each one, on
// every page, tab, bar and fold a result can open, is a result for that
// screen. A title that names another page is found as that page.
#[test]
fn search_finds_every_title_the_pages_draw() {
    let name = "search_finds_every_title_the_pages_draw";
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
    send_settings(&mut app, Message::Widget(Widget::Glance, true));
    let index = navigation::all_search_hits();
    let pages: Vec<&str> = index
        .iter()
        .filter(|hit| hit.names_page_result())
        .map(|hit| hit.label)
        .collect();
    let mut missing = std::collections::BTreeSet::new();
    for (screen, tab) in every_screen() {
        send_settings(&mut app, Message::SearchResult(screen));
        send_settings(&mut app, Message::ObjectTab(tab));
        view::drawn::take();
        let mut native = crate::native_test::Native::new(size);
        // A view can build a row and then leave it out; only what is shown counts.
        let shown = native.all_text(app.settings_view());
        for title in view::drawn::take() {
            if !shown.contains(&title) {
                continue;
            }
            let found = pages.contains(&title.as_str())
                || navigation::NOT_INDEXED
                    .iter()
                    .any(|(skipped, _)| *skipped == title)
                || index
                    .iter()
                    .any(|hit| hit.label == title && hit.opens(&screen));
            if !found {
                missing.insert(index_line(&screen, &title));
            }
        }
    }
    // A title drawn both with and without Advanced needs only the plain entry.
    let missing: Vec<_> = missing
        .iter()
        .filter(|line| {
            !line.contains(", true)") || !missing.contains(&line.replacen(", true)", ", false)", 1))
        })
        .cloned()
        .collect();
    assert!(
        missing.is_empty(),
        "These titles are on screen but not in search. Add each line below to the \
         index in src/settings/navigation.rs, or add the title to NOT_INDEXED \
         there with the reason it stays out:\n{}",
        missing.join("\n")
    );
}

/// A search index entry for `title` on `screen`, ready to paste.
fn index_line(screen: &navigation::SearchHit, title: &str) -> String {
    let mut line = format!(
        "hit(Page::{:?}, {title:?}, \"\", {})",
        screen.page, screen.advanced
    );
    if screen.page == Page::General {
        line += &format!(
            ".destination(AppSettingsPage::{:?})",
            screen.app_destination()
        );
    }
    if let Some(bar) = screen.bar {
        line += &format!(".bar(PanelKind::{bar:?})");
    }
    line + ","
}
