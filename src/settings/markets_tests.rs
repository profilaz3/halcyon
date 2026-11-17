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
        assert!(native.scrollables(card()).is_empty());
        native.capture(
            card(),
            &cosmic::Theme::dark(),
            &format!("markets-card-{}", dims[0]),
        );
    }
}

#[test]
fn markets_resize_keeps_every_stock_and_chart_visible_without_scrolling() {
    let name = "markets_resize_keeps_every_stock_and_chart_visible_without_scrolling";
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
    use crate::tools::cards::Action;
    use cosmic::iced::{Event, mouse};
    let mut config = Config::default();
    config.appearance.family = Some(crate::desktop_theme::Family::DarkGlass);
    config.appearance.opacity = 30;
    let mut state = crate::tools::cards::sample_markets();
    for symbol in ["AMZN", "TSLA"] {
        let mut quote = state.quotes[0].clone();
        quote.symbol = symbol.into();
        state.quotes.push(quote);
        config.markets.symbols.push(symbol.into());
    }
    for dims in [[280., 300.], [300., 360.], [360., 420.], [520., 600.]] {
        for offline in [false, true] {
            state.problem = offline.then_some(crate::markets::Problem::Network);
            let card = || crate::tools::cards::markets(&state, &config, Some(|a| a), Some(dims));
            let mut native = crate::native_test::Native::new(Size::new(dims[0], dims[1]));
            assert!(native.scrollables(card()).is_empty(), "{dims:?}");
            for symbol in &config.markets.symbols {
                assert!(
                    !native.text_all(card(), symbol).is_empty(),
                    "{symbol} at {dims:?}"
                );
            }
            for (text, bounds) in native.placed_text(card()) {
                assert!(
                    bounds.x >= 0.
                        && bounds.y >= 0.
                        && bounds.x + bounds.width <= dims[0] + 0.5
                        && bounds.y + bounds.height <= dims[1] + 0.5,
                    "{dims:?}: {text}: {bounds:?}"
                );
            }
            let text = native.all_text(card());
            let caption = text
                .iter()
                .find(|t| t.contains("30 trading sessions"))
                .unwrap();
            let dates = text
                .iter()
                .find(|t| t.starts_with("Oct 1") && t.contains(" to "))
                .unwrap();
            let top = native.text_bounds(card(), caption);
            let bottom = native.text_bounds(card(), dates);
            assert!(
                bottom.y - (top.y + top.height) >= 24.,
                "chart collapsed at {dims:?}"
            );
            let point = native.text_bounds(card(), "TSLA").center();
            let _ = native.event(
                card(),
                Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)),
                point,
            );
            let (messages, _) = native.event(
                card(),
                Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)),
                point,
            );
            assert!(
                messages
                    .iter()
                    .any(|m| matches!(m, Action::MarketSelect(s) if s == "TSLA"))
            );
            if !offline {
                native.capture(
                    card(),
                    &cosmic::Theme::dark(),
                    &format!("markets-five-stocks-{}", dims[0]),
                );
            }
        }
    }
}
