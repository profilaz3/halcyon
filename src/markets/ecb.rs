//! Daily reference rates, quoted by the ECB as units of each currency per euro.
use super::{Point, Problem, Quote};
use chrono::NaiveDate;
use std::collections::BTreeMap;

const HISTORY: &str = "https://www.ecb.europa.eu/stats/eurofxref/eurofxref-hist-90d.xml";
const NS: &str = "http://www.ecb.int/vocabulary/2002-08-01/eurofxref";

pub(super) struct History(BTreeMap<NaiveDate, BTreeMap<String, f64>>);

impl History {
    pub(super) async fn fetch(client: &reqwest::Client) -> Result<Self, Problem> {
        if crate::tools::preview() {
            return Err(Problem::Provider);
        }
        let mut response = client
            .get(HISTORY)
            .send()
            .await
            .map_err(|_| Problem::Network)?;
        if response.status().as_u16() == 429 {
            return Err(Problem::RateLimit);
        }
        if !response.status().is_success() {
            return Err(Problem::Provider);
        }
        let mut body = Vec::new();
        while let Some(chunk) = response.chunk().await.map_err(|_| Problem::Network)? {
            if body.len() + chunk.len() > 512 * 1024 {
                return Err(Problem::InvalidData);
            }
            body.extend_from_slice(&chunk);
        }
        Self::parse(std::str::from_utf8(&body).map_err(|_| Problem::InvalidData)?)
    }

    fn parse(xml: &str) -> Result<Self, Problem> {
        let document = roxmltree::Document::parse(xml).map_err(|_| Problem::InvalidData)?;
        let mut days = BTreeMap::new();
        for day in document
            .descendants()
            .filter(|n| n.has_tag_name((NS, "Cube")) && n.has_attribute("time"))
        {
            let date = day
                .attribute("time")
                .and_then(|s| s.parse::<NaiveDate>().ok())
                .ok_or(Problem::InvalidData)?;
            let mut rates = BTreeMap::from([("EUR".into(), 1.0)]);
            for rate in day.children().filter(|n| n.has_tag_name((NS, "Cube"))) {
                let code = rate.attribute("currency").ok_or(Problem::InvalidData)?;
                let value = rate
                    .attribute("rate")
                    .and_then(|v| v.parse::<f64>().ok())
                    .filter(|v| v.is_finite() && *v > 0.)
                    .ok_or(Problem::InvalidData)?;
                if rates.insert(code.to_owned(), value).is_some() {
                    return Err(Problem::InvalidData);
                }
            }
            if rates.len() < 2 || days.insert(date, rates).is_some() {
                return Err(Problem::InvalidData);
            }
        }
        if days.is_empty() {
            return Err(Problem::InvalidData);
        }
        Ok(Self(days))
    }

    pub(super) fn quote(&self, symbol: &str) -> Result<(Quote, Vec<Point>), Problem> {
        let (base, counter) = super::currency_pair(symbol).ok_or(Problem::Provider)?;
        let (&published, _) = self.0.last_key_value().ok_or(Problem::InvalidData)?;
        let points: Vec<_> = self
            .0
            .iter()
            .filter_map(|(&date, rates)| {
                let close = rates.get(counter)? / rates.get(base)?;
                (close.is_finite() && close > 0.).then_some(Point { date, close })
            })
            .collect();
        let latest = points
            .last()
            .filter(|p| p.date == published)
            .ok_or(Problem::Provider)?;
        let price = latest.close;
        let previous = points.iter().rev().nth(1);
        let quote = Quote {
            symbol: symbol.into(),
            name: symbol.into(),
            currency: counter.into(),
            price,
            change: previous.map(|p| price - p.close),
            percent: previous.map(|p| (price / p.close - 1.) * 100.),
            timestamp: published
                .and_hms_opt(0, 0, 0)
                .unwrap()
                .and_utc()
                .timestamp(),
            market_open: None,
        };
        let chart = points
            .into_iter()
            .filter(|p| (published - p.date).num_days() < 30)
            .collect();
        Ok((quote, chart))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn publication_dates_and_cross_rates_use_matching_observations() {
        let xml = format!(
            r#"<Cube xmlns="{NS}">
          <Cube time="2026-11-13"><Cube currency="USD" rate="1.2"/><Cube currency="GBP" rate="0.75"/></Cube>
          <Cube time="2026-10-14"><Cube currency="USD" rate="1"/><Cube currency="GBP" rate="0.7"/></Cube>
          <Cube time="2026-11-12"><Cube currency="USD" rate="1.1"/><Cube currency="GBP" rate="0.8"/></Cube>
        </Cube>"#
        );
        let history = History::parse(&xml).unwrap();
        let (quote, points) = history.quote("GBP/USD").unwrap();
        assert!((quote.price - 1.6).abs() < 1e-12);
        assert!((quote.percent.unwrap() - (1.6 / 1.375 - 1.) * 100.).abs() < 1e-10);
        assert_eq!(points.len(), 2);
        assert_eq!(points[0].date.to_string(), "2026-11-12");
        assert_eq!(
            chrono::DateTime::from_timestamp(quote.timestamp, 0)
                .unwrap()
                .date_naive()
                .to_string(),
            "2026-11-13"
        );
        assert_eq!(history.quote("EUR/USD").unwrap().0.price, 1.2);
        assert_eq!(history.quote("USD/EUR").unwrap().0.price, 1. / 1.2);
        assert_eq!(history.quote("EUR/ZZZ").unwrap_err(), Problem::Provider);
        for bad in [
            xml.replace("1.2", "NaN"),
            xml.replace("2026-10-14", "2026-11-12"),
            xml.replace(NS, "wrong"),
        ] {
            assert!(History::parse(&bad).is_err());
        }
    }

    #[test]
    fn selecting_another_pair_keeps_rates_and_cached_history_after_a_stock_failure() {
        use crate::markets::{Preferences, State};
        let mut preferences = Preferences {
            symbols: ["EUR/USD", "EUR/GBP", "AAPL"].map(String::from).into(),
            ..Default::default()
        };
        let mut state = State {
            source: Some(preferences.clone()),
            ..State::default()
        };
        let xml = format!(
            r#"<Cube xmlns="{NS}"><Cube time="2026-11-13"><Cube currency="USD" rate="1.2"/><Cube currency="GBP" rate="0.75"/></Cube></Cube>"#
        );
        let history = History::parse(&xml).unwrap();
        for symbol in ["EUR/USD", "EUR/GBP"] {
            state.record(symbol, history.quote(symbol));
        }
        state.failed("AAPL", Problem::RateLimit);
        state.received_at = Some(std::time::Instant::now());
        preferences.selected = "EUR/GBP".into();
        assert!(state.fresh_for(&preferences));
        assert_eq!(state.charts[&preferences.selected][0].close, 0.75);
        state.record("EUR/GBP", Err(Problem::Network));
        assert_eq!(state.charts["EUR/GBP"][0].close, 0.75);
        assert_eq!(state.quotes.len(), 2);
        assert_eq!(state.unavailable["EUR/GBP"], Problem::Network);
    }

    #[test]
    #[ignore = "manual smoke test against the ECB public feed"]
    fn markets_live_ecb_rates_and_native_layouts_without_credentials() {
        use crate::{
            config::Config,
            markets::{Preferences, Provider, State},
            native_test::Native,
            tools::cards,
        };
        use cosmic::iced::Size;
        let mut config = Config::default();
        config.appearance.family = Some(crate::desktop_theme::Family::DarkGlass);
        config.markets = Preferences {
            enabled: true,
            provider: Provider::TwelveData,
            symbols: ["EUR/USD", "EUR/GBP", "GBP/USD"].map(String::from).into(),
            selected: "EUR/USD".into(),
            ..Default::default()
        };
        let mut state = State {
            source: Some(config.markets.clone()),
            ..State::default()
        };
        let (mut output, _rx) = cosmic::iced::futures::channel::mpsc::channel(8);
        tokio::runtime::Runtime::new().unwrap().block_on(async {
            let client = reqwest::Client::builder()
                .timeout(std::time::Duration::from_secs(15))
                .redirect(reqwest::redirect::Policy::none())
                .build()
                .unwrap();
            crate::markets::refresh(&client, &config.markets, &mut state, &mut output).await;
        });
        assert_eq!(state.problem, None);
        assert_eq!(state.quotes.len(), 3);
        let eur_usd = &state.quotes[0];
        assert!((eur_usd.price / state.quotes[1].price - state.quotes[2].price).abs() < 1e-10);
        assert!((2..=30).contains(&state.charts["EUR/USD"].len()));
        println!(
            "Published {}: EUR/USD {}, EUR/GBP {}",
            state.charts["EUR/USD"].last().unwrap().date,
            eur_usd.price,
            state.quotes[1].price
        );
        let mut one = config.clone();
        one.markets.symbols.truncate(1);
        one.markets.show_change = false;
        for (style, width, name) in [
            (&config, 360., "ecb-watchlist"),
            (&config, 520., "ecb-watchlist-wide"),
            (&one, 280., "ecb-price-only"),
        ] {
            let size = [width, cards::size(crate::drawer::Widget::Markets, style)[1]];
            let mut native = Native::new(Size::new(size[0], size[1]));
            native.capture(
                cards::markets(&state, style, Some(|a| a), Some(size)),
                &cosmic::Theme::dark(),
                name,
            );
        }
        config.markets.show_chart = true;
        config.markets.show_date = true;
        config.markets.show_details = true;
        for (size, name) in [([280., 300.], "ecb-compact"), ([520., 400.], "ecb-wide")] {
            let mut native = Native::new(Size::new(size[0], size[1]));
            let view = || cards::markets(&state, &config, Some(|a| a), Some(size));
            assert!(native.scrollables(view()).is_empty());
            native.capture(view(), &cosmic::Theme::dark(), name);
        }
    }
}
