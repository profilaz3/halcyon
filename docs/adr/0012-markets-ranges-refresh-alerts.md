# Markets chart ranges, refresh and price alerts

## Context

The Markets widget charts about a month of daily prices and refreshes every
15 minutes, with no way to see more or less, ask for prices now, or be told
when a price passes a level. Its sources keep different history: the ECB's
90-day file, Yahoo's three months and Twelve Data's requested page.

## Decision

- `markets.chart_range` (`week`, `month`, `quarter`; default `month`)
  chooses how much of the history the chart shows. Every source now keeps
  about three months: the ECB's whole 90 days, and Twelve Data asks for 92
  sessions instead of 30, in the same single request. A year isn't offered:
  the ECB has no file between 90 days and its complete history, far past the
  256 KB response bound. The card shows 1W, 1M and 3M; it is a style field,
  exported with Looks.
- A refresh on the card (and Refresh now in its right-click menu) asks
  every source again now. It restarts the feed with the last reading marked
  old; the existing request budget still bounds a burst of presses.
- `markets.alerts` maps a watchlist symbol to an optional price above and
  below. When a quote crosses one, its row is marked and one desktop
  notification is sent; the alert sends again only after the price has been
  back inside. Alerts are personal and local: never exported with a Look.
  A symbol leaving the watchlist drops its alert.

## Alternatives

- Intraday or yearly ranges. They need other requests per source, and the
  ECB offers neither.
- Island-only alerts through the bridge. A desktop notification already
  reaches the island when it handles notifications, and works without it.

## Consequences

Which side an alert last fired on is kept in memory, so after a restart a
price still past its level notifies once more.
