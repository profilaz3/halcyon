# Markets widget

Follow up to five stocks and currency pairs in one watchlist. New watchlists include EUR/USD, EUR/GBP, AAPL, MSFT and NVDA. Existing watchlists keep their saved symbols and selection.

## Set up

1. Open Halcyon settings, search for **Markets** or **exchange rates**, and enable the widget.
2. Under **Watchlist**, enter a stock or pair and choose **Add**. You can also paste several symbols separated by commas. Use the arrows to change their order and the remove button to take an item off the list. At least one item stays on the widget.
3. Under **Show on widget**, choose what you want to see. The default shows each symbol, its price and its daily change. Turn off **Daily change** for prices alone. **Price chart**, **Publication date**, and **Source and chart details** are off by default.
4. Select a row to choose the item for the optional chart or publication date. At small sizes **Next** cycles through items when the list doesn't fit. Choose **Edit** to return to settings. The preview uses example prices.

With the chart off, the card fits its list. If you previously resized a chart, turning it back on restores the saved chart space. A failed update still shows a short status beside cached quotes.

## Currency rates

Currency pairs use public European Central Bank reference rates. Both currencies must be covered by the ECB. Halcyon calculates cross rates from values published on the same date. The percentage change is relative to the previous published rate for that pair.

The chart covers the most recent 30 calendar days ending on the publication date. Only published observations appear. With **Publication date** enabled, the card shows the last published date, including on weekends and holidays. The ECB provides rates for information purposes. [ECB rates, publication schedule and history](https://www.ecb.europa.eu/stats/policy_and_exchange_rates/euro_reference_exchange_rates/html/index.en.html)

## Stocks

Yahoo Finance supplies public stock quotes and charts without signup. Its unofficial feed can change, and coverage and delays depend on the instrument. Enter Yahoo symbols such as `BRK-B` or `VOD.L`. Twelve Data also supports `SYMBOL:EXCHANGE` notation.

To use Twelve Data for stocks, select it under **Data source**, choose **Get an API key** and save your key. The desktop keyring may ask to unlock. Stock charts show up to 30 trading sessions in the instrument’s currency. **Publication date** shows the provider timestamp in UTC. **Source and chart details** includes market status when supplied. [Twelve Data coverage and plans](https://twelvedata.com/pricing)

## Updates and privacy

Halcyon checks for updates every 15 minutes while the widget is visible. One ECB history download serves all currency pairs without sending your watchlist. Stock requests send the relevant stock symbols directly to the selected provider. Switching rows reuses recently fetched data. On a failed refresh, cached quotes keep their original dates. Hiding the widget clears the in-memory cache.

Only Twelve Data stock requests use your API key. The key stays in the desktop Secret Service keyring. Background updates do not prompt to unlock it. **Remove API key** deletes the saved credential. The watchlist and selection stay on this computer and are excluded from shared appearance profiles and usage reporting.
