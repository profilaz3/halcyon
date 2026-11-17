# Markets widget

Markets shows up to five stocks, their quoted prices and daily percentage changes, and a daily price chart for the selected stock. It uses the same appearance, transparency, resizing and display placement settings as other widgets.

Resizing adapts the content without scrolling inside the widget. All watchlist rows remain visible, the chart fills the remaining space, and smaller sizes use shorter labels and fewer secondary details.

## Set up

1. Open Halcyon settings and search for **Markets** or **stocks**.
2. Enable the Markets widget.
3. Leave **Yahoo Finance** selected under **Data source**. Quotes and charts are requested directly from Yahoo Finance without an account or saved credential.
4. Enter up to five Yahoo symbols separated by commas, such as `AAPL, MSFT, NVDA`, and choose **Save watchlist**. Yahoo uses symbols such as `BRK-B` and `VOD.L`; Twelve Data's `SYMBOL:EXCHANGE` notation is not accepted by the public source.
5. Select a stock on the desktop widget to show its chart. **Edit** opens Markets settings again.

To use Twelve Data instead, select **Twelve Data**, choose **Get an API key**, and save your key in the password field. Your desktop keyring may ask to unlock. This is optional.

The settings preview always contains labeled example data. It does not contact the provider.

## Prices and updates

Quotes refresh every 15 minutes while the widget is visible. Selecting another stock reuses recent quotes. The public source caches all watchlist charts, so switching stocks does not make another request while that cache is fresh. Requests are paced to limit traffic. With Twelve Data, other applications using the same key share your provider allowance.

Charts show up to 30 daily price points in chronological order, with dates and a low/high scale. The latest daily point can change while a trading session is in progress. Prices are shown in the instrument's currency, without conversion. The selected quote displays its provider timestamp in UTC and market status when available.

The default Yahoo Finance feed is public but unofficial: access and formats may change, and no uptime guarantee is implied. Coverage and delays depend on the instrument. With the optional Twelve Data source, coverage and quotas depend on your [plan](https://twelvedata.com/pricing). The widget labels prices as potentially delayed. If a request fails, it keeps any cached prices with their timestamps and displays the problem; it does not substitute example prices. Cache is held only in memory and cleared when the widget is hidden.

## Credentials and privacy

Public-data updates never read the keyring or sends a Twelve Data key. When you choose Twelve Data, its API key is saved in the desktop's Secret Service keyring. Halcyon never saves it in its configuration or profile exports. Background updates do not prompt to unlock the keyring; use Markets settings to save the key again after unlocking if needed. **Remove API key** deletes Halcyon's saved Markets credential.

The watchlist and selected stock are saved on this computer and excluded from shared appearance profiles. Quotes are requested directly from the selected provider over HTTPS. Yahoo Finance receives requested symbols but no API key; Twelve Data receives its API key and requested symbols. Halcyon does not send the watchlist through its usage reporting.
