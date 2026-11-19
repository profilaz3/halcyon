//! A personal stock and currency watchlist. Provider timestamps, currency and missing data
//! are retained; an unavailable quote is never presented as a zero price.
use cosmic::iced::{Subscription, futures::SinkExt};
use serde::{Deserialize, Serialize};
use std::time::Duration;

mod ecb;
mod public;

const REFRESH_INTERVAL: Duration = Duration::from_secs(15 * 60);

fn attributes() -> std::collections::HashMap<&'static str, &'static str> {
    std::collections::HashMap::from([
        ("application", "cosmic-desktop-cards"),
        ("purpose", "markets-twelve-data"),
    ])
}

/// Explicit setup may unlock the keyring. Polling never opens a prompt.
pub async fn save_key(key: Secret) -> Result<(), Problem> {
    if crate::tools::preview() {
        return Err(Problem::Setup);
    }
    let value = key.0.trim();
    if value.is_empty()
        || value.len() > 256
        || !value
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"_-".contains(&b))
    {
        return Err(Problem::Setup);
    }
    tokio::time::timeout(Duration::from_secs(90), async {
        use secret_service::{EncryptionType, SecretService};
        let service = SecretService::connect(EncryptionType::Dh)
            .await
            .map_err(|_| Problem::Keyring)?;
        let collection = match service.get_default_collection().await {
            Ok(c) => c,
            Err(_) => service
                .create_collection("Halcyon", "default")
                .await
                .map_err(|_| Problem::Keyring)?,
        };
        if collection.is_locked().await.map_err(|_| Problem::Keyring)? {
            collection.unlock().await.map_err(|_| Problem::Locked)?;
        }
        collection
            .create_item(
                "Halcyon · Markets",
                attributes(),
                value.as_bytes(),
                true,
                "text/plain",
            )
            .await
            .map_err(|_| Problem::Keyring)?;
        Ok(())
    })
    .await
    .map_err(|_| Problem::Locked)?
}

pub async fn remove_key() -> Result<(), Problem> {
    if crate::tools::preview() {
        return Err(Problem::Setup);
    }
    tokio::time::timeout(Duration::from_secs(90), async {
        use secret_service::{EncryptionType, SecretService};
        let service = SecretService::connect(EncryptionType::Dh)
            .await
            .map_err(|_| Problem::Keyring)?;
        let items = service
            .search_items(attributes())
            .await
            .map_err(|_| Problem::Keyring)?;
        for item in items.unlocked.iter().chain(&items.locked) {
            if item.is_locked().await.map_err(|_| Problem::Keyring)? {
                item.unlock().await.map_err(|_| Problem::Locked)?;
            }
            item.delete().await.map_err(|_| Problem::Keyring)?;
        }
        Ok(())
    })
    .await
    .map_err(|_| Problem::Locked)?
}

async fn read_key() -> Result<Secret, Problem> {
    if crate::tools::preview() {
        return Err(Problem::Setup);
    }
    tokio::time::timeout(Duration::from_secs(5), async {
        use secret_service::{EncryptionType, SecretService};
        let service = SecretService::connect(EncryptionType::Dh)
            .await
            .map_err(|_| Problem::Keyring)?;
        let items = service
            .search_items(attributes())
            .await
            .map_err(|_| Problem::Keyring)?;
        let Some(item) = items.unlocked.first() else {
            return Err(if items.locked.is_empty() {
                Problem::Setup
            } else {
                Problem::Locked
            });
        };
        let bytes = item.get_secret().await.map_err(|_| Problem::Locked)?;
        if bytes.is_empty() || bytes.len() > 256 {
            return Err(Problem::Setup);
        }
        String::from_utf8(bytes)
            .map(Secret)
            .map_err(|_| Problem::Setup)
    })
    .await
    .map_err(|_| Problem::Keyring)?
}

/// A bounded burst keeps first load responsive without exceeding the provider's
/// entry-level minute allowance. It survives subscription/configuration changes.
#[derive(Default)]
struct Budget(std::collections::VecDeque<std::time::Instant>);
impl Budget {
    fn reserve(&mut self, now: std::time::Instant) -> Option<Duration> {
        while self
            .0
            .front()
            .is_some_and(|at| now.saturating_duration_since(*at) >= Duration::from_secs(61))
        {
            self.0.pop_front();
        }
        if self.0.len() >= 7 {
            return Some(
                Duration::from_secs(61)
                    .saturating_sub(now.saturating_duration_since(*self.0.front().unwrap())),
            );
        }
        self.0.push_back(now);
        None
    }
}
async fn reserve_request() {
    static BUDGET: std::sync::OnceLock<tokio::sync::Mutex<Budget>> = std::sync::OnceLock::new();
    let mut budget = BUDGET
        .get_or_init(|| tokio::sync::Mutex::new(Budget::default()))
        .lock()
        .await;
    while let Some(delay) = budget.reserve(std::time::Instant::now()) {
        tokio::time::sleep(delay).await;
    }
    drop(budget);
}
async fn request(
    client: &reqwest::Client,
    endpoint: &str,
    symbol: &str,
    key: &Secret,
) -> Result<serde_json::Value, Problem> {
    reserve_request().await;
    let mut query = vec![("symbol", symbol)];
    let mut auth = reqwest::header::HeaderValue::from_str(&format!("apikey {}", key.0))
        .map_err(|_| Problem::Setup)?;
    auth.set_sensitive(true);
    if endpoint == "time_series" {
        query.extend([("interval", "1day"), ("outputsize", "30"), ("order", "asc")]);
    }
    let mut response = client
        .get(format!("https://api.twelvedata.com/{endpoint}"))
        .header(reqwest::header::AUTHORIZATION, auth)
        .query(&query)
        .send()
        .await
        .map_err(|_| Problem::Network)?;
    if response.status().as_u16() == 429 {
        return Err(Problem::RateLimit);
    }
    if response.status().as_u16() == 401 {
        return Err(Problem::Setup);
    }
    if !response.status().is_success() {
        return Err(Problem::Provider);
    }
    let mut bytes = Vec::new();
    while let Some(chunk) = response.chunk().await.map_err(|_| Problem::Network)? {
        if bytes.len() + chunk.len() > 256 * 1024 {
            return Err(Problem::InvalidData);
        }
        bytes.extend_from_slice(&chunk);
    }
    serde_json::from_slice(&bytes).map_err(|_| Problem::InvalidData)
}

struct Feed {
    preferences: Preferences,
    seed: State,
}
impl std::hash::Hash for Feed {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        std::hash::Hash::hash(&self.preferences.feed_key(), state);
    }
}

pub fn subscription(mut preferences: Preferences, seed: State) -> Subscription<State> {
    preferences.normalize();
    Subscription::run_with(Feed { preferences, seed }, |feed| {
        let preferences = feed.preferences.clone();
        let seed = if feed.seed.same_watchlist(&preferences) {
            feed.seed.clone()
        } else {
            State::default()
        };
        cosmic::iced::stream::channel::<State>(4, async move |mut output| {
            let mut state = seed;
            let client = reqwest::Client::builder()
                .timeout(Duration::from_secs(15))
                .redirect(reqwest::redirect::Policy::none())
                .build();
            loop {
                state.source = Some(preferences.clone());
                state.loading = true;
                if output.send(state.clone()).await.is_err() {
                    break;
                }
                if let Ok(client) = &client {
                    refresh(client, &preferences, &mut state, &mut output).await;
                } else {
                    state.problem = Some(Problem::Network);
                    state.received_at = Some(std::time::Instant::now());
                }
                state.loading = false;
                if output.send(state.clone()).await.is_err() {
                    break;
                }
                tokio::time::sleep(state.next_refresh(std::time::Instant::now())).await;
            }
        })
    })
}

async fn refresh(
    client: &reqwest::Client,
    preferences: &Preferences,
    state: &mut State,
    output: &mut cosmic::iced::futures::channel::mpsc::Sender<State>,
) {
    let reuse = state.fresh_for(preferences);
    let (pairs, stocks): (Vec<_>, Vec<_>) = preferences
        .symbols
        .iter()
        .partition(|symbol| currency_pair(symbol).is_some());
    let selected = &preferences.selected;
    let needs_stock_chart = preferences.show_chart
        && preferences.provider == Provider::TwelveData
        && stocks.contains(&selected)
        && (!reuse
            || (!state.charts.contains_key(selected) && !state.unavailable.contains_key(selected)));
    if reuse && !needs_stock_chart {
        return;
    }

    if !reuse {
        state.problem = None;
        let reuse_ecb = state
            .ecb_checked_at
            .is_some_and(|at| at.elapsed() < REFRESH_INTERVAL);
        state
            .unavailable
            .retain(|symbol, _| reuse_ecb && currency_pair(symbol).is_some());
        if !pairs.is_empty() && !reuse_ecb {
            let history = ecb::History::fetch(client).await;
            for symbol in pairs {
                let result = history
                    .as_ref()
                    .map_err(Clone::clone)
                    .and_then(|h| h.quote(symbol));
                state.record(symbol, result);
            }
            state.ecb_checked_at = Some(std::time::Instant::now());
            let _ = output.send(state.clone()).await;
        }
    }

    if !stocks.is_empty() {
        match preferences.provider {
            Provider::Public => {
                let mut limited = false;
                for symbol in stocks {
                    let result = if limited {
                        Err(Problem::RateLimit)
                    } else {
                        public::fetch(client, symbol).await
                    };
                    limited = matches!(result, Err(Problem::RateLimit));
                    state.record(symbol, result);
                    let _ = output.send(state.clone()).await;
                }
            }
            Provider::TwelveData => match read_key().await {
                Ok(key) => {
                    let mut stop: Option<Problem> = None;
                    if !reuse {
                        // Twelve Data supplies charts separately. Refresh the next selected
                        // stock on demand and preserve the current trace if its request fails.
                        state.charts.retain(|symbol, _| {
                            currency_pair(symbol).is_some()
                                || (preferences.show_chart && symbol == selected)
                        });
                        for symbol in stocks {
                            let result = if let Some(problem) = &stop {
                                Err(problem.clone())
                            } else {
                                request(client, "quote", symbol, &key)
                                    .await
                                    .and_then(|v| parse_quote(&v, symbol))
                            };
                            match result {
                                Ok(quote) => state.store_quote(quote),
                                Err(problem) => {
                                    if matches!(problem, Problem::Setup | Problem::RateLimit) {
                                        stop = Some(problem.clone());
                                    }
                                    state.failed(symbol, problem);
                                }
                            }
                            let _ = output.send(state.clone()).await;
                        }
                    }
                    if needs_stock_chart && stop.is_none() {
                        match request(client, "time_series", selected, &key)
                            .await
                            .and_then(|v| parse_chart(&v, selected))
                        {
                            Ok(points) => {
                                state.charts.insert(selected.clone(), points);
                            }
                            Err(problem) => state.failed(selected, problem),
                        }
                    }
                }
                Err(problem) => {
                    for symbol in stocks {
                        state.failed(symbol, problem.clone());
                    }
                }
            },
        }
    }
    if !reuse {
        state.received_at = Some(std::time::Instant::now());
    }
}

pub fn currency_pair(symbol: &str) -> Option<(&str, &str)> {
    let (base, counter) = symbol.split_once('/')?;
    let code = |s: &str| s.len() == 3 && s.bytes().all(|b| b.is_ascii_uppercase());
    (code(base) && code(counter) && base != counter).then_some((base, counter))
}

pub const MAX_SYMBOLS: usize = 5;

#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum Provider {
    #[default]
    Public,
    TwelveData,
}
impl Provider {
    pub fn label(self) -> &'static str {
        match self {
            Self::Public => "Yahoo Finance",
            Self::TwelveData => "Twelve Data",
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(default)]
pub struct Preferences {
    pub provider: Provider,
    pub enabled: bool,
    pub symbols: Vec<String>,
    pub selected: String,
    pub show_change: bool,
    pub show_chart: bool,
    pub show_date: bool,
    pub show_details: bool,
    /// Changes after explicit keyring setup, restarting the desktop feed.
    pub credential_revision: u64,
}

impl Default for Preferences {
    fn default() -> Self {
        Self {
            provider: Provider::Public,
            enabled: false,
            symbols: ["EUR/USD", "EUR/GBP", "AAPL", "MSFT", "NVDA"]
                .map(String::from)
                .into(),
            selected: "EUR/USD".into(),
            show_change: true,
            show_chart: false,
            show_date: false,
            show_details: false,
            credential_revision: 0,
        }
    }
}

pub fn symbols(input: &str) -> Result<Vec<String>, &'static str> {
    let mut result = Vec::new();
    for value in input
        .split(|c: char| c == ',' || c.is_whitespace())
        .filter(|v| !v.is_empty())
    {
        let value = value.to_ascii_uppercase();
        if currency_pair(&value).is_none()
            && (value.len() > 32
                || !value
                    .bytes()
                    .all(|c| c.is_ascii_alphanumeric() || b".-:".contains(&c))
                || !value
                    .bytes()
                    .next()
                    .is_some_and(|c| c.is_ascii_alphanumeric()))
        {
            return Err("Use a stock symbol such as AAPL or a currency pair such as EUR/USD.");
        }
        if !result.contains(&value) {
            result.push(value);
        }
        if result.len() > MAX_SYMBOLS {
            return Err("Choose up to five stocks or currency pairs.");
        }
    }
    if result.is_empty() {
        return Err("Add at least one stock or currency pair.");
    }
    Ok(result)
}

impl Preferences {
    fn quote_key(&self) -> (Provider, Vec<&str>, u64) {
        let mut symbols: Vec<_> = self.symbols.iter().map(String::as_str).collect();
        symbols.sort_unstable();
        (self.provider, symbols, self.credential_revision)
    }

    /// Only Twelve Data needs a separate stock-chart request.
    pub(crate) fn feed_key(&self) -> impl std::hash::Hash + Eq + '_ {
        (
            self.enabled,
            self.quote_key(),
            (self.provider == Provider::TwelveData
                && self.show_chart
                && currency_pair(&self.selected).is_none())
            .then_some(&self.selected),
        )
    }

    /// Includes frame padding, the heading, each quote row and optional metadata.
    pub fn quote_height(&self) -> f32 {
        90. + 24. * self.symbols.len().saturating_sub(1) as f32
            + 20. * f32::from(self.show_date)
            + 20. * f32::from(self.show_details)
    }

    pub fn height_limit(&self) -> Option<f32> {
        (!self.show_chart).then(|| self.quote_height())
    }

    pub fn normalize(&mut self) {
        self.symbols = symbols(&self.symbols.join(",")).unwrap_or_default();
        if !self.symbols.contains(&self.selected) {
            self.selected = self.symbols.first().cloned().unwrap_or_default();
        }
    }
}

/// Secret text must not appear in messages' Debug output or exported settings.
#[derive(Clone, Default)]
pub struct Secret(pub String);
impl std::fmt::Debug for Secret {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Secret([redacted])")
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Quote {
    pub symbol: String,
    pub name: String,
    pub currency: String,
    pub price: f64,
    pub change: Option<f64>,
    pub percent: Option<f64>,
    pub timestamp: i64,
    pub market_open: Option<bool>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Point {
    pub date: chrono::NaiveDate,
    pub close: f64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Problem {
    Setup,
    Locked,
    Keyring,
    Network,
    RateLimit,
    Provider,
    InvalidData,
}
impl Problem {
    pub fn short(&self, cached: bool) -> &'static str {
        match self {
            Self::Setup => "Connect API key",
            Self::Locked => "Keyring locked",
            Self::Keyring => "Keyring unavailable",
            Self::Network if cached => "Offline · cached",
            Self::Network => "Offline",
            Self::RateLimit => "Data limit reached",
            Self::Provider | Self::InvalidData => "Data unavailable",
        }
    }

    pub fn text(&self) -> &'static str {
        match self {
            Self::Setup => "Connect Twelve Data in Markets settings.",
            Self::Locked => "Unlock your desktop keyring in Markets settings.",
            Self::Keyring => "The desktop keyring is unavailable. Try connecting again.",
            Self::Network => "Couldn’t reach the data provider. Prices will retry automatically.",
            Self::RateLimit => {
                "The data limit was reached. Updates are paused until the next refresh."
            }
            Self::Provider => "This symbol or data is unavailable from this data source.",
            Self::InvalidData => "The provider returned incomplete price data.",
        }
    }
}

#[derive(Clone, Debug, Default)]
pub struct State {
    pub ecb_checked_at: Option<std::time::Instant>,
    pub source: Option<Preferences>,
    pub received_at: Option<std::time::Instant>,
    pub quotes: Vec<Quote>,
    pub charts: std::collections::BTreeMap<String, Vec<Point>>,
    pub problem: Option<Problem>,
    pub unavailable: std::collections::BTreeMap<String, Problem>,
    pub loading: bool,
}

impl State {
    pub(crate) fn same_watchlist(&self, preferences: &Preferences) -> bool {
        self.source
            .as_ref()
            .is_some_and(|p| p.quote_key() == preferences.quote_key())
    }

    fn next_refresh(&self, now: std::time::Instant) -> Duration {
        self.received_at.map_or(REFRESH_INTERVAL, |at| {
            REFRESH_INTERVAL.saturating_sub(now.saturating_duration_since(at))
        })
    }

    fn fresh_for(&self, preferences: &Preferences) -> bool {
        self.same_watchlist(preferences)
            && self
                .received_at
                .is_some_and(|at| at.elapsed() < REFRESH_INTERVAL)
    }

    fn store_quote(&mut self, quote: Quote) {
        self.quotes.retain(|q| q.symbol != quote.symbol);
        self.quotes.push(quote);
    }

    fn record(&mut self, symbol: &str, result: Result<(Quote, Vec<Point>), Problem>) {
        match result {
            Ok((quote, points)) => {
                self.store_quote(quote);
                self.charts.insert(symbol.into(), points);
            }
            Err(problem) => self.failed(symbol, problem),
        }
    }

    fn failed(&mut self, symbol: &str, problem: Problem) {
        self.unavailable.insert(symbol.into(), problem);
    }
}

fn provider_error(value: &serde_json::Value) -> Result<(), Problem> {
    if value.get("status").and_then(|v| v.as_str()) != Some("error") {
        return Ok(());
    }
    Err(match value.get("code").and_then(|v| v.as_u64()) {
        Some(401) => Problem::Setup,
        Some(429) => Problem::RateLimit,
        _ => Problem::Provider,
    })
}
fn number(value: &serde_json::Value, key: &str) -> Option<f64> {
    let v = value.get(key)?;
    v.as_f64()
        .or_else(|| v.as_str()?.parse().ok())
        .filter(|v| v.is_finite())
}
fn string(value: &serde_json::Value, key: &str, limit: usize) -> Option<String> {
    let s = value.get(key)?.as_str()?;
    if s.is_empty() || s.chars().count() > limit || s.chars().any(char::is_control) {
        return None;
    }
    Some(s.into())
}

pub fn parse_quote(value: &serde_json::Value, requested: &str) -> Result<Quote, Problem> {
    provider_error(value)?;
    let symbol = string(value, "symbol", 32).ok_or(Problem::InvalidData)?;
    if symbol != requested.split(':').next().unwrap_or(requested) {
        return Err(Problem::InvalidData);
    }
    let price = number(value, "close")
        .filter(|v| *v > 0.)
        .ok_or(Problem::InvalidData)?;
    let timestamp = value
        .get("timestamp")
        .and_then(|v| v.as_i64())
        .filter(|v| *v > 0)
        .ok_or(Problem::InvalidData)?;
    Ok(Quote {
        symbol: requested.into(),
        name: string(value, "name", 120).unwrap_or(symbol),
        currency: string(value, "currency", 12).ok_or(Problem::InvalidData)?,
        price,
        change: number(value, "change"),
        percent: number(value, "percent_change"),
        timestamp,
        market_open: value.get("is_market_open").and_then(|v| v.as_bool()),
    })
}

pub fn parse_chart(value: &serde_json::Value, requested: &str) -> Result<Vec<Point>, Problem> {
    provider_error(value)?;
    if value.pointer("/meta/symbol").and_then(|v| v.as_str()) != requested.split(':').next() {
        return Err(Problem::InvalidData);
    }
    let rows = value
        .get("values")
        .and_then(|v| v.as_array())
        .filter(|v| v.len() <= 64)
        .ok_or(Problem::InvalidData)?;
    let mut points = Vec::new();
    for row in rows {
        let date = row
            .get("datetime")
            .and_then(|v| v.as_str())
            .and_then(|v| chrono::NaiveDate::parse_from_str(v, "%Y-%m-%d").ok())
            .ok_or(Problem::InvalidData)?;
        let close = number(row, "close")
            .filter(|v| *v > 0.)
            .ok_or(Problem::InvalidData)?;
        points.push(Point { date, close });
    }
    points.sort_by_key(|p| p.date);
    if points.len() < 2 || points.windows(2).any(|p| p[0].date == p[1].date) {
        return Err(Problem::InvalidData);
    }
    Ok(points)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn request_budget_allows_a_small_burst_then_waits_for_the_window() {
        let mut budget = Budget::default();
        let now = std::time::Instant::now();
        for _ in 0..7 {
            assert_eq!(budget.reserve(now), None);
        }
        assert_eq!(budget.reserve(now), Some(Duration::from_secs(61)));
        assert_eq!(
            budget.reserve(now + Duration::from_secs(30)),
            Some(Duration::from_secs(31))
        );
        assert_eq!(budget.reserve(now + Duration::from_secs(61)), None);
        assert_eq!(budget.0.len(), 1);
    }

    #[test]
    fn public_stock_selection_keeps_an_inflight_feed() {
        let mut a = Preferences {
            selected: "AAPL".into(),
            show_chart: true,
            ..Default::default()
        };
        let mut b = a.clone();
        b.selected = "MSFT".into();
        assert!(a.feed_key() == b.feed_key());
        a.provider = Provider::TwelveData;
        b.provider = Provider::TwelveData;
        assert!(a.feed_key() != b.feed_key());
        a.show_chart = false;
        b.show_chart = false;
        assert!(a.feed_key() == b.feed_key());
    }

    #[test]
    fn switching_charts_reuses_quotes_but_a_different_key_or_watchlist_does_not() {
        let mut preferences = Preferences::default();
        let mut state = crate::tools::cards::sample_markets();
        state.source = Some(preferences.clone());
        state.received_at = Some(std::time::Instant::now());
        preferences.provider = Provider::TwelveData;
        assert!(!state.fresh_for(&preferences));
        preferences.provider = Provider::Public;
        preferences.symbols.reverse();
        let original = Preferences::default();
        assert!(preferences.feed_key() == original.feed_key());
        preferences.selected = "MSFT".into();
        assert!(state.fresh_for(&preferences));
        preferences.credential_revision = 1;
        assert!(!state.fresh_for(&preferences));
        preferences.credential_revision = 0;
        preferences.symbols = vec!["OTHER".into()];
        assert!(!state.fresh_for(&preferences));
        state.unavailable.insert("AAPL".into(), Problem::Network);
        assert!(
            state.fresh_for(&Preferences::default()),
            "changing selection should not retry a failed provider early"
        );
    }

    #[test]
    fn selecting_a_row_keeps_the_quote_deadline_and_failed_rows_keep_their_cache() {
        let mut state = crate::tools::cards::sample_markets();
        let mut prefs = state.source.clone().unwrap();
        let now = std::time::Instant::now();
        state.received_at = Some(now - REFRESH_INTERVAL + Duration::from_secs(60));
        prefs.selected = prefs.symbols[1].clone();
        assert!(state.fresh_for(&prefs));
        assert_eq!(state.next_refresh(now), Duration::from_secs(60));
        assert_eq!(state.next_refresh(now + Duration::from_secs(60)), Duration::ZERO);
        state.received_at = Some(now - REFRESH_INTERVAL);
        state.unavailable.insert(prefs.selected.clone(), Problem::Network);
        assert!(!state.fresh_for(&prefs));
        assert!(state.same_watchlist(&prefs));
    }

    #[test]
    fn symbols_are_bounded_deduplicated_and_do_not_accept_urls() {
        assert_eq!(
            symbols(" aapl,MSFT AAPL BRK.B ").unwrap(),
            ["AAPL", "MSFT", "BRK.B"]
        );
        assert_eq!(
            symbols("eur/usd, gbp/jpy, EUR/USD AAPL").unwrap(),
            ["EUR/USD", "GBP/JPY", "AAPL"]
        );
        for bad in [
            "",
            "A,B,C,D,E,F",
            "https://example.com",
            "AAPL?apikey=secret",
            "EUR/EUR",
            "USD/12A",
            "EUR//USD",
            "USD/GBP/JPY",
        ] {
            assert!(symbols(bad).is_err(), "{bad}");
        }
        assert!(!format!("{:?}", Secret("private-key".into())).contains("private-key"));
    }

    #[test]
    fn quotes_preserve_missing_changes_and_reject_mismatched_or_invalid_prices() {
        let q = json!({"symbol":"AAPL","name":"Apple","currency":"USD","close":"190.12","timestamp":1700000000});
        let parsed = parse_quote(&q, "AAPL").unwrap();
        assert_eq!(parsed.percent, None);
        assert_eq!(parsed.market_open, None);
        assert!(parse_quote(&q, "MSFT").is_err());
        for bad in ["NaN", "inf", "0", "-1", "unknown"] {
            let mut q = q.clone();
            q["close"] = json!(bad);
            assert!(parse_quote(&q, "AAPL").is_err());
        }
        assert_eq!(
            parse_quote(&json!({"status":"error","code":429}), "AAPL"),
            Err(Problem::RateLimit)
        );
    }

    #[test]
    fn charts_are_chronological_and_never_bridge_invalid_or_duplicate_samples() {
        let q = json!({"meta":{"symbol":"AAPL"},"values":[{"datetime":"2026-11-16","close":"101"},{"datetime":"2026-11-13","close":"100"}]});
        let p = parse_chart(&q, "AAPL").unwrap();
        assert!(p[0].date < p[1].date);
        assert!(parse_chart(&q, "MSFT").is_err());
        let mut duplicate = q.clone();
        duplicate["values"][1] = duplicate["values"][0].clone();
        assert!(parse_chart(&duplicate, "AAPL").is_err());
        let mut missing = q;
        missing["values"][0]["close"] = json!("bad");
        assert!(parse_chart(&missing, "AAPL").is_err());
    }
}
