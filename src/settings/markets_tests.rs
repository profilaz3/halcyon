// Included in settings::tests; no external service or real keyring is touched.
#[test]
fn markets_widget_settings_preserve_watchlist_and_render_native_charts() {
    let name = "markets_widget_settings_preserve_watchlist_and_render_native_charts";
    if desktop_test_child(
        name,
        DesktopEnv {
            app_config: true,
            fixture: true,
            icons: Icons::Shown,
            ..Default::default()
        },
    ) {
        return;
    }
    let _root = isolated_root();
    use crate::drawer::Widget;
    use crate::settings::tools::Message as Edit;
    assert!(!Config::default().widget_enabled(Widget::Markets));
    assert!(Widget::TOOLS.contains(&Widget::Markets));
    assert_eq!(
        crate::desktop::store::widget::tool_of("/markets/enabled"),
        Some("markets")
    );
    assert_eq!(navigation::widget_page(Widget::Markets), Page::Markets);
    for query in ["stocks", "watchlist", "price charts"] {
        assert!(
            navigation::search(query)
                .iter()
                .any(|hit| hit.page == Page::Markets)
        );
    }
    let (mut app, task) = Settings::init(Core::default(), Page::Markets);
    settle_settings(&mut app, task);
    for size in [Size::new(640., 600.), Size::new(1100., 800.)] {
        send_settings(&mut app, Message::Resized(size));
        let mut native = crate::native_test::Native::new(size);
        assert!(native.duplicate_ids(app.settings_view()).is_empty());
        native.capture(
            app.settings_view(),
            &cosmic::Theme::dark(),
            &format!("markets-settings-{}", size.width),
        );
        assert!(
            native
                .all_text(app.settings_view())
                .iter()
                .any(|t| t.contains("Example prices"))
        );
    }
    send_settings(&mut app, Message::Widget(Widget::Markets, true));
    assert!(Config::load().widget_enabled(Widget::Markets));
    send_settings(
        &mut app,
        Message::Tools(Edit::MarketSymbols("msft, aapl, MSFT".into())),
    );
    send_settings(&mut app, Message::Tools(Edit::MarketSaveSymbols));
    assert_eq!(Config::load().markets.symbols, ["MSFT", "AAPL"]);
    for pointer in [
        "/markets/symbols",
        "/markets/selected",
        "/markets/credential_revision",
    ] {
        assert!(
            !crate::desktop::store::widget::FIELDS
                .iter()
                .any(|f| f.pointer == pointer)
        );
        assert!(
            crate::desktop::store::widget::LOCAL_FIELDS
                .iter()
                .any(|f| f.pointer == pointer)
        );
    }
    send_settings(
        &mut app,
        Message::Tools(Edit::MarketSymbols("https://invalid.example".into())),
    );
    send_settings(&mut app, Message::Tools(Edit::MarketSaveSymbols));
    assert_eq!(Config::load().markets.symbols, ["MSFT", "AAPL"]);
    let key = "synthetic-test-key-not-a-credential";
    send_settings(
        &mut app,
        Message::Tools(Edit::MarketKey(crate::markets::Secret(key.into()))),
    );
    assert!(
        !serde_json::to_string(&Config::load())
            .unwrap()
            .contains(key)
    );
    let runtime = tokio::runtime::Runtime::new().unwrap();
    assert_eq!(
        runtime.block_on(crate::markets::save_key(crate::markets::Secret(key.into()))),
        Err(crate::markets::Problem::Setup)
    );
    assert_eq!(
        runtime.block_on(crate::markets::remove_key()),
        Err(crate::markets::Problem::Setup)
    );
    let mut config = Config::default();
    config.markets.symbols = vec!["IBM".into()];
    config.markets.selected = "IBM".into();
    config.appearance.family = Some(crate::desktop_theme::Family::DarkGlass);
    config.appearance.opacity = 30;
    let sample = crate::tools::cards::sample_markets();
    for dims in [[360., 420.], [280., 300.]] {
        let mut native = crate::native_test::Native::new(Size::new(dims[0], dims[1]));
        let card = || crate::tools::cards::markets::<Message>(&sample, &config, None, Some(dims));
        let text = native.all_text(card());
        assert!(text.iter().any(|s| s.contains("190.12 USD")));
        assert!(text.iter().any(|s| s.contains("30 trading sessions")));
        native.capture(
            card(),
            &cosmic::Theme::dark(),
            &format!("markets-card-{}", dims[0]),
        );
    }
}
