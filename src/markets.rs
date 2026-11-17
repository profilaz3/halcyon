//! A personal stock watchlist. Provider timestamps, currency and missing data
//! are retained; an unavailable quote is never presented as a zero price.
use cosmic::iced::{Subscription, futures::SinkExt};
use serde::{Deserialize, Serialize};
use std::time::Duration;

mod public;

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
        std::hash::Hash::hash(&self.preferences, state);
    }
}

pub fn subscription(mut preferences: Preferences, seed: State) -> Subscription<State> {
    preferences.normalize();
    Subscription::run_with(Feed { preferences, seed }, |feed| {
        let preferences = feed.preferences.clone();
        let seed = if feed.seed.fresh_for(&preferences) {
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
                let reuse_quotes = state.fresh_for(&preferences);
                state.source = Some(preferences.clone());
                if preferences.provider == Provider::Public {
                    state.loading = true;
                    let _ = output.send(state.clone()).await;
                    if let Ok(client) = &client {
                        public::refresh(client, &preferences, &mut state, reuse_quotes).await;
                    } else {
                        state.problem = Some(Problem::Network);
                    }
                    state.loading = false;
                    if output.send(state.clone()).await.is_err() {
                        break;
                    }
                    tokio::time::sleep(Duration::from_secs(15 * 60)).await;
                    continue;
                }
                let key = read_key().await;
                if let (Ok(client), Ok(key)) = (&client, &key) {
                    state.loading = true;
                    state.problem = None;
                    state.unavailable.clear();
                    let _ = output.send(state.clone()).await;
                    if !reuse_quotes {
                        for symbol in &preferences.symbols {
                            let result = match request(client, "quote", symbol, key).await {
                                Ok(value) => parse_quote(&value, symbol),
                                Err(p) => Err(p),
                            };
                            match result {
                                Ok(quote) => {
                                    state.quotes.retain(|q| &q.symbol != symbol);
                                    state.quotes.push(quote);
                                }
                                Err(problem) => {
                                    state.unavailable.push(symbol.clone());
                                    let stop =
                                        matches!(problem, Problem::RateLimit | Problem::Setup);
                                    state.problem = Some(problem);
                                    if stop {
                                        break;
                                    }
                                }
                            }
                            let _ = output.send(state.clone()).await;
                        }
                        state.received_at =
                            state.unavailable.is_empty().then(std::time::Instant::now);
                    }
                    if !preferences.selected.is_empty()
                        && !matches!(state.problem, Some(Problem::RateLimit | Problem::Setup))
                    {
                        let chart = match request(client, "time_series", &preferences.selected, key)
                            .await
                        {
                            Ok(v) => parse_chart(&v, &preferences.selected),
                            Err(p) => Err(p),
                        };
                        match chart {
                            Ok(points) => {
                                state.chart = points;
                                state.chart_symbol = preferences.selected.clone();
                            }
                            Err(problem) => {
                                state.problem = Some(problem);
                            }
                        }
                    }
                    state.loading = false;
                } else {
                    state.loading = false;
                    state.problem = Some(key.err().unwrap_or(Problem::Network));
                }
                if output.send(state.clone()).await.is_err() {
                    break;
                }
                tokio::time::sleep(Duration::from_secs(15 * 60)).await;
            }
        })
    })
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
    /// Changes after explicit keyring setup, restarting the desktop feed.
    pub credential_revision: u64,
}

impl Default for Preferences {
    fn default() -> Self {
        Self {
            provider: Provider::Public,
            enabled: false,
            symbols: vec!["AAPL".into(), "MSFT".into(), "NVDA".into()],
            selected: "AAPL".into(),
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
        if value.len() > 32
            || !value
                .bytes()
                .all(|c| c.is_ascii_alphanumeric() || b".-:".contains(&c))
            || !value
                .bytes()
                .next()
                .is_some_and(|c| c.is_ascii_alphanumeric())
        {
            return Err("Use stock symbols such as AAPL, MSFT or BRK.B.");
        }
        if !result.contains(&value) {
            result.push(value);
        }
        if result.len() > MAX_SYMBOLS {
            return Err("Choose up to five stocks.");
        }
    }
    if result.is_empty() {
        return Err("Add at least one stock symbol.");
    }
    Ok(result)
}

impl Preferences {
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
    pub source: Option<Preferences>,
    pub received_at: Option<std::time::Instant>,
    pub quotes: Vec<Quote>,
    pub public_charts: std::collections::BTreeMap<String, Vec<Point>>,
    pub chart_symbol: String,
    pub chart: Vec<Point>,
    pub problem: Option<Problem>,
    pub unavailable: Vec<String>,
    pub loading: bool,
}

impl State {
    fn fresh_for(&self, preferences: &Preferences) -> bool {
        self.source.as_ref().is_some_and(|p| {
            p.provider == preferences.provider
                && p.symbols == preferences.symbols
                && p.credential_revision == preferences.credential_revision
        }) && self
            .received_at
            .is_some_and(|at| at.elapsed() < Duration::from_secs(15 * 60))
            && self.unavailable.is_empty()
            && preferences
                .symbols
                .iter()
                .all(|s| self.quotes.iter().any(|q| &q.symbol == s))
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
    fn switching_charts_reuses_quotes_but_a_different_key_or_watchlist_does_not() {
        let mut preferences = Preferences::default();
        let mut state = crate::tools::cards::sample_markets();
        state.source = Some(preferences.clone());
        state.received_at = Some(std::time::Instant::now());
        preferences.provider = Provider::TwelveData;
        assert!(!state.fresh_for(&preferences));
        preferences.provider = Provider::Public;
        preferences.selected = "MSFT".into();
        assert!(state.fresh_for(&preferences));
        preferences.credential_revision = 1;
        assert!(!state.fresh_for(&preferences));
        preferences.credential_revision = 0;
        preferences.symbols = vec!["OTHER".into()];
        assert!(!state.fresh_for(&preferences));
        state.unavailable.push("AAPL".into());
        assert!(!state.fresh_for(&Preferences::default()));
    }

    #[test]
    fn symbols_are_bounded_deduplicated_and_do_not_accept_urls() {
        assert_eq!(
            symbols(" aapl,MSFT AAPL BRK.B ").unwrap(),
            ["AAPL", "MSFT", "BRK.B"]
        );
        for bad in [
            "",
            "A,B,C,D,E,F",
            "https://example.com",
            "AAPL?apikey=secret",
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
