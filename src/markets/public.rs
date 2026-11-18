//! Best-effort, keyless public quotes. Requests go directly to Yahoo Finance.
use super::*;

pub(super) async fn fetch(
    client: &reqwest::Client,
    symbol: &str,
) -> Result<(Quote, Vec<Point>), Problem> {
    if crate::tools::preview()
        || symbol.contains([':', '/'])
        || symbols(symbol).as_deref() != Ok(&[symbol.to_string()])
    {
        return Err(Problem::Provider);
    }
    reserve_request().await;
    let mut response = client
        .get(format!(
            "https://query1.finance.yahoo.com/v8/finance/chart/{symbol}"
        ))
        .header(
            reqwest::header::USER_AGENT,
            "Halcyon/0.7.6 (desktop markets widget)",
        )
        .query(&[("range", "3mo"), ("interval", "1d")])
        .send()
        .await
        .map_err(|_| Problem::Network)?;
    if response.status().as_u16() == 429 {
        return Err(Problem::RateLimit);
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
    parse(
        &serde_json::from_slice(&bytes).map_err(|_| Problem::InvalidData)?,
        symbol,
    )
}

fn parse(value: &serde_json::Value, symbol: &str) -> Result<(Quote, Vec<Point>), Problem> {
    if value.pointer("/chart/error").is_some_and(|e| !e.is_null()) {
        return Err(Problem::Provider);
    }
    let results = value
        .pointer("/chart/result")
        .and_then(|v| v.as_array())
        .filter(|r| r.len() == 1)
        .ok_or(Problem::InvalidData)?;
    let data = &results[0];
    let meta = data.get("meta").ok_or(Problem::InvalidData)?;
    if string(meta, "symbol", 32).as_deref() != Some(symbol) {
        return Err(Problem::InvalidData);
    }
    let zone = string(meta, "exchangeTimezoneName", 64)
        .and_then(|s| s.parse::<chrono_tz::Tz>().ok())
        .ok_or(Problem::InvalidData)?;
    let timestamp = meta
        .get("regularMarketTime")
        .and_then(|v| v.as_i64())
        .filter(|n| *n > 0)
        .ok_or(Problem::InvalidData)?;
    let quoted_at = chrono::DateTime::from_timestamp(timestamp, 0)
        .ok_or(Problem::InvalidData)?
        .with_timezone(&zone)
        .date_naive();
    let price = number(meta, "regularMarketPrice")
        .filter(|n| *n > 0.)
        .ok_or(Problem::InvalidData)?;
    let times = data
        .get("timestamp")
        .and_then(|v| v.as_array())
        .ok_or(Problem::InvalidData)?;
    let closes = data
        .pointer("/indicators/quote/0/close")
        .and_then(|v| v.as_array())
        .filter(|v| v.len() == times.len() && v.len() <= 200)
        .ok_or(Problem::InvalidData)?;
    let mut points = Vec::new();
    for (time, close) in times.iter().zip(closes) {
        // Missing sessions stay missing; never manufacture a zero price.
        if close.is_null() {
            continue;
        }
        let close = close
            .as_f64()
            .filter(|n| n.is_finite() && *n > 0.)
            .ok_or(Problem::InvalidData)?;
        let date = time
            .as_i64()
            .and_then(|t| chrono::DateTime::from_timestamp(t, 0))
            .ok_or(Problem::InvalidData)?
            .with_timezone(&zone)
            .date_naive();
        points.push(Point { date, close });
    }
    points.sort_by_key(|p| p.date);
    if points.len() < 2
        || points.windows(2).any(|p| p[0].date == p[1].date)
        || points.last().unwrap().date > quoted_at
    {
        return Err(Problem::InvalidData);
    }
    let previous = points
        .iter()
        .rev()
        .find(|p| p.date < quoted_at)
        .map(|p| p.close);
    let change = previous.map(|p| price - p).filter(|n| n.is_finite());
    let percent = previous
        .and_then(|p| change.map(|c| c / p * 100.))
        .filter(|n| n.is_finite());
    if points.len() > 30 {
        points.drain(..points.len() - 30);
    }
    Ok((
        Quote {
            symbol: symbol.into(),
            name: string(meta, "longName", 120)
                .or_else(|| string(meta, "shortName", 120))
                .unwrap_or_else(|| symbol.into()),
            currency: string(meta, "currency", 12).ok_or(Problem::InvalidData)?,
            price,
            timestamp,
            change,
            percent,
            market_open: None,
        },
        points,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    #[test]
    #[ignore = "manual smoke test against the public Yahoo Finance service"]
    fn markets_public_live_quotes_and_charts_need_no_credentials() {
        assert!(!crate::tools::preview());
        let runtime = tokio::runtime::Runtime::new().unwrap();
        runtime.block_on(async {
            let client = reqwest::Client::builder()
                .timeout(Duration::from_secs(15))
                .redirect(reqwest::redirect::Policy::none())
                .build()
                .unwrap();
            for symbol in ["AAPL", "MSFT", "NVDA"] {
                let (quote, chart) = fetch(&client, symbol).await.unwrap();
                assert_eq!(quote.symbol, symbol);
                assert!(quote.price > 0. && !quote.currency.is_empty());
                assert!((2..=30).contains(&chart.len()));
            }
        });
    }
    fn sample() -> serde_json::Value {
        json!({"chart":{"error":null,"result":[{"meta":{"symbol":"AAPL","currency":"USD","exchangeTimezoneName":"America/New_York","regularMarketTime":1700254800_i64,"regularMarketPrice":105.0},"timestamp":[1700082000_i64,1700168400_i64,1700254800_i64],"indicators":{"quote":[{"close":[99.0,100.0,104.0]}]}}]}})
    }
    #[test]
    fn markets_public_quotes_use_previous_session_not_range_start_and_keep_currency() {
        let (q, chart) = parse(&sample(), "AAPL").unwrap();
        assert_eq!((q.price, q.percent, q.change), (105., Some(5.), Some(5.)));
        assert_eq!(q.currency, "USD");
        assert_eq!(chart.len(), 3);
        assert_eq!(q.market_open, None);
    }
    #[test]
    fn markets_public_errors_missing_prices_and_wrong_symbols_are_not_quotes() {
        assert!(parse(&sample(), "MSFT").is_err());
        assert_eq!(
            parse(&json!({"chart":{"error":{"code":"Not Found"}}}), "AAPL"),
            Err(Problem::Provider)
        );
        let mut bad = sample();
        bad["chart"]["result"][0]["meta"]["regularMarketPrice"] = json!(0);
        assert!(parse(&bad, "AAPL").is_err());
        let mut gap = sample();
        gap["chart"]["result"][0]["indicators"]["quote"][0]["close"][0] = serde_json::Value::Null;
        assert_eq!(parse(&gap, "AAPL").unwrap().1.len(), 2);
    }
}
