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
    assert_eq!(
        Config::default().markets.provider,
        crate::markets::Provider::Public
    );
    assert!(Widget::TOOLS.contains(&Widget::Markets));
    assert_eq!(
        crate::desktop::store::widget::tool_of("/markets/enabled"),
        Some("markets")
    );
    assert_eq!(navigation::widget_page(Widget::Markets), Page::Markets);
    for query in ["stocks", "currency", "exchange rates", "watchlist", "price charts"] {
        assert!(
            navigation::search(query)
                .iter()
                .any(|hit| hit.page == Page::Markets)
        );
    }
    let (mut app, task) = Settings::init(Core::default(), Page::Markets);
    settle_settings(&mut app, task);
    for size in [Size::new(640., 600.), Size::new(820., 600.), Size::new(1100., 800.)] {
        send_settings(&mut app, Message::Resized(size));
        let mut native = crate::native_test::Native::new(size);
        assert!(native.duplicate_ids(app.settings_view()).is_empty());
        let input = native.bounds(app.settings_view(), "market-symbol-input");
        assert!(input.y >= 0. && input.y + input.height <= size.height);
        let save = native.text_bounds(app.settings_view(), "Add");
        assert!(save.y + save.height <= size.height);

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
    let mut native = crate::native_test::Native::new(Size::new(1100., 800.));
    assert!(native.text_all(app.settings_view(), "Save API key").is_empty());
    send_settings(
        &mut app,
        Message::Tools(Edit::MarketProvider(crate::markets::Provider::TwelveData)),
    );
    assert_eq!(
        Config::load().markets.provider,
        crate::markets::Provider::TwelveData
    );
    assert_eq!(native.text_all(app.settings_view(), "Save API key").len(), 1);
    send_settings(
        &mut app,
        Message::Tools(Edit::MarketProvider(crate::markets::Provider::Public)),
    );
    assert_eq!(
        Config::load().markets.provider,
        crate::markets::Provider::Public
    );
    send_settings(&mut app, Message::Widget(Widget::Markets, true));
    assert!(Config::load().widget_enabled(Widget::Markets));
    send_settings(&mut app, Message::Tools(Edit::MarketSymbols("JPY/GBP".into())));
    send_settings(&mut app, Message::Tools(Edit::MarketAdd));
    assert_eq!(Config::load().markets.symbols.len(), 5);
    assert!(app.tools.watchlist_error.is_some());
    for (key, value) in [("show_change", false), ("show_chart", true), ("show_date", true), ("show_details", true)] {
        send_settings(&mut app, Message::Tools(Edit::Option(Widget::Markets, key, value.into())));
    }
    let display = Config::load().markets;
    assert!(!display.show_change && display.show_chart && display.show_date && display.show_details);
    for symbol in ["NVDA", "MSFT", "EUR/USD"] {
        send_settings(&mut app, Message::Tools(Edit::MarketRemove(symbol.into())));
    }
    send_settings(&mut app, Message::Tools(Edit::MarketSymbols("msft".into())));
    send_settings(&mut app, Message::Tools(Edit::MarketAdd));
    send_settings(&mut app, Message::Tools(Edit::MarketMove("MSFT".into(), -1)));
    send_settings(&mut app, Message::Tools(Edit::MarketMove("MSFT".into(), -1)));
    assert!(app.tools.market_symbols.is_empty());
    assert_eq!(Config::load().markets.symbols, ["MSFT", "EUR/GBP", "AAPL"]);
    assert_eq!(
        app.tools.market_preview.source.as_ref().unwrap(),
        &app.config.markets
    );
    assert!(
        app.tools
            .market_preview
            .charts
            .contains_key(&app.config.markets.selected)
    );
    assert_eq!(app.tools.market_preview.quotes.len(), 3);
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
    send_settings(&mut app, Message::Tools(Edit::MarketAdd));
    assert_eq!(Config::load().markets.symbols, ["MSFT", "EUR/GBP", "AAPL"]);
    for symbol in ["MSFT", "AAPL", "EUR/GBP"] {
        send_settings(&mut app, Message::Tools(Edit::MarketRemove(symbol.into())));
    }
    assert_eq!(Config::load().markets.symbols, ["EUR/GBP"]);
    assert_eq!(Config::load().markets.selected, "EUR/GBP");
    send_settings(&mut app, Message::Tools(Edit::MarketSymbols("IBM".into())));
    Config::save_tool(Widget::Markets, serde_json::json!({"symbols": ["AMD"], "selected": "AMD"})).unwrap();
    send_settings(&mut app, Message::Tick);
    assert_eq!(app.tools.market_symbols, "IBM");
    assert_eq!(app.tools.market_preview.source.as_ref().unwrap().selected, "AMD");
    send_settings(&mut app, Message::Page(Page::Clock));
    send_settings(&mut app, Message::Page(Page::Markets));
    assert_eq!(app.tools.market_symbols, "IBM");
    send_settings(&mut app, Message::Tools(Edit::MarketAdd));
    assert_eq!(Config::load().markets.symbols, ["AMD", "IBM"]);
    send_settings(&mut app, Message::UndoDesktop);
    assert_eq!(app.tools.market_preview.source.as_ref().unwrap().symbols, ["AMD"]);
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
    config.markets.show_chart = true;
    config.markets.show_date = true;
    config.markets.show_details = true;
    config.markets.symbols = vec!["IBM".into()];
    config.markets.selected = "IBM".into();
    config.appearance.family = Some(crate::desktop_theme::Family::DarkGlass);
    config.appearance.opacity = 30;
    let fixture = crate::markets::Preferences {
        symbols: vec!["AAPL".into()],
        selected: "AAPL".into(),
        ..Default::default()
    };
    let sample = crate::tools::cards::sample_markets_for(&fixture);
    for dims in [[360., 420.], [280., 300.]] {
        let mut native = crate::native_test::Native::new(Size::new(dims[0], dims[1]));
        let card = || crate::tools::cards::markets::<Message>(&sample, &config, None, Some(dims));
        let text = native.all_text(card());
        assert!(text.iter().any(|s| s.contains("190.12 USD")));
        assert!(text.iter().any(|s| s.contains("30 trading sessions")));
        assert!(text.iter().any(|s| s.ends_with("UTC")));
        assert!(!text.iter().any(|s| s.contains("Market closed") || s.contains("Status unavailable")));
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
    config.markets.show_chart = true;
    config.markets.show_date = true;
    config.markets.show_details = true;
    config.markets.symbols = ["AAPL", "MSFT", "NVDA", "AMZN", "TSLA"].map(String::from).into();
    config.markets.selected = "AAPL".into();
    let mut state = crate::tools::cards::sample_markets_for(&config.markets);
    let date_ranges = ["%b %-d", "%b %-d, %Y"].map(|format| {
        let first = state.charts[&config.markets.selected]
            .first()
            .unwrap()
            .date
            .format(format);
        let last = state.charts[&config.markets.selected]
            .last()
            .unwrap()
            .date
            .format(format);
        format!("{first} to {last}")
    });
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
            let dates = date_ranges
                .iter()
                .find(|range| text.contains(range))
                .unwrap_or_else(|| panic!("missing chart date range at {dims:?}: {text:?}"));
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

#[test]
fn markets_currency_resizes_and_cycles_the_mixed_watchlist() {
    use crate::tools::cards::{Action, markets, sample_markets_for};
    use cosmic::iced::{Event, mouse};
    let mut config = Config::default();
    config.markets.show_chart = true;
    config.markets.show_date = true;
    config.markets.symbols = ["EUR/USD", "EUR/GBP", "AAPL"].map(String::from).into();
    config.markets.selected = "EUR/USD".into();
    let state = sample_markets_for(&config.markets);
    for dims in [[280., 300.], [520., 400.]] {
        let view = || markets(&state, &config, Some(|a| a), Some(dims));
        let mut native = crate::native_test::Native::new(Size::new(dims[0], dims[1]));
        let text = native.all_text(view());
        assert!(text.iter().any(|s| s == "1 EUR = 1.0854 USD"));
        assert!(text.iter().any(|s| s.starts_with("Published ") && !s.contains("UTC")));
        assert!(text.iter().any(|s| s == "Last 30 days"));
        assert!(native.scrollables(view()).is_empty());
        for (label, rect) in native.placed_text(view()) {
            assert!(rect.x >= 0. && rect.y >= 0. && rect.x + rect.width <= dims[0] + 0.5
                && rect.y + rect.height <= dims[1] + 0.5, "{label}: {rect:?} at {dims:?}");
        }
        let target = if dims[0] < 320. {
            assert!(!text.iter().any(|s| s == "AAPL"));
            "Next"
        } else {
            assert!(text.iter().any(|s| s == "AAPL"));
            "EUR/GBP"
        };
        let point = native.text_bounds(view(), target).center();
        let _ = native.event(view(), Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)), point);
        let (messages, _) = native.event(view(), Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)), point);
        assert!(messages.iter().any(|m| matches!(m, Action::MarketSelect(s) if s == "EUR/GBP")));
    }
}

#[test]
fn markets_defaults_to_watchlist_and_fits_the_saved_chart_viewport() {
    use crate::drawer::Widget;
    use crate::tools::cards::{markets, sample_markets_for};
    let mut config = Config::default();
    config.markets = serde_json::from_value(serde_json::json!({
        "symbols": ["EUR/USD", "EUR/GBP", "GBP/USD"], "selected": "EUR/USD"
    }))
    .unwrap();
    let state = sample_markets_for(&config.markets);
    let requested = [520., 400.];
    let fitted = |config: &Config| {
        crate::layout::fitted_size(
            Widget::Markets,
            requested,
            [1920., 1080.],
            config.markets.height_limit(),
        )
    };
    for count in [3, 1] {
        config.markets.symbols.truncate(count);
        let height = config.markets.quote_height();
        assert_eq!(fitted(&config), [520., height]);
        let size = crate::tools::cards::size(Widget::Markets, &config);
        assert_eq!(size[1], height);
        let mut native = crate::native_test::Native::new(Size::new(size[0], height));
        let view = || markets(&state, &config, Some(|a| a), Some(size));
        let text = native.all_text(view());
        assert!(text.iter().any(|s| s == "EUR/USD"));
        assert!(text.iter().any(|s| s == "1.0854 USD"));
        assert!(text.iter().any(|s| s.ends_with('%')));
        assert!(!text.iter().any(|s| s.starts_with("Published")
            || s.starts_with("Low ")
            || s.contains("reference rate")
            || s == "Last 30 days"));
        assert!(native.scrollables(view()).is_empty());
    }
    config.markets.show_change = false;
    let size = crate::tools::cards::size(Widget::Markets, &config);
    let mut native = crate::native_test::Native::new(Size::new(size[0], size[1]));
    let view = || markets(&state, &config, Some(|a| a), Some(size));
    assert!(!native.all_text(view()).iter().any(|s| s.contains('%')));
    let mut offline = state.clone();
    offline.problem = Some(crate::markets::Problem::Network);
    let cached = || markets(&offline, &config, Some(|a| a), Some(size));
    assert!(native.all_text(cached()).iter().any(|s| s == "Offline · cached"));

    config.markets.show_date = true;
    config.markets.show_details = true;
    let mut native =
        crate::native_test::Native::new(Size::new(360., config.markets.quote_height()));
    let view = || markets(&state, &config, Some(|a| a), None);
    let text = native.all_text(view());
    assert!(text.iter().any(|s| s.starts_with("Published")));
    assert!(text.iter().any(|s| s.contains("reference rate")));
    assert!(!text.iter().any(|s| s == "Last 30 days"));
    config.markets.show_chart = true;
    assert_eq!(fitted(&config), requested);
    assert_eq!(crate::layout::fitted_size(Widget::Markets, [280., 90.], [1920., 1080.], config.markets.height_limit()), [280., 300.]);

}

#[test]
fn markets_quote_precision_survives_resize() {
    use crate::tools::cards::{markets, sample_markets_for};
    let mut config = Config::default();
    config.markets.symbols = vec![
        "AAPL".into(),
        "MSFT".into(),
        "NVDA".into(),
        "EUR/USD".into(),
        "IBM".into(),
    ];
    config.markets.selected = "AAPL".into();
    config.markets.show_chart = true;
    let mut state = sample_markets_for(&config.markets);
    state
        .quotes
        .iter_mut()
        .find(|q| q.symbol == "AAPL")
        .unwrap()
        .price = 0.0042;
    for (size, expected) in [([520., 400.], "0.0042 USD"), ([300., 300.], "0.0042 USD")] {
        let mut n = crate::native_test::Native::new(Size::new(size[0], size[1]));
        let labels = n.all_text(markets(&state, &config, Some(|a| a), Some(size)));
        assert!(labels.iter().any(|s| s == expected), "{labels:?}");
    }
}
