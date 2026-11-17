# Markets widget

Markets shows up to five stocks, their quoted prices and daily percentage changes, and a daily price chart for the selected stock. It uses the same appearance, transparency, resizing and display placement settings as other widgets.

Resizing adapts the content without scrolling inside the widget. All watchlist rows remain visible, the chart fills the remaining space, and smaller sizes use shorter labels and fewer secondary details.

## Set up

1. Open Halcyon settings and search for **Markets** or **stocks**.
2. Enable the Markets widget.
3. Enter stock symbols separated by commas, such as `AAPL, MSFT, NVDA`, and choose **Save watchlist**. For an exchange-specific symbol, use the provider's `SYMBOL:EXCHANGE` notation.
4. Choose **Get an API key** to open [Twelve Data](https://twelvedata.com/account/api-keys), then paste your own key into the password field and choose **Save API key**. Your desktop keyring may ask to unlock.
5. Select a stock on the desktop widget to show its chart. **Edit** opens Markets settings again.

The settings preview always contains labeled example data. It does not contact the provider.

## Prices and updates

Quotes refresh every 15 minutes while the widget is visible. Selecting another stock reuses recent quotes and requests its chart. Requests are paced to reduce quota use; other applications using the same key still share your provider allowance.

Charts show up to 30 daily price points in chronological order, with dates and a low/high scale. The latest daily point can change while a trading session is in progress. Prices are shown in the instrument's currency, without conversion. The selected quote displays its provider timestamp in UTC and market status when available.

Exchange coverage, delays and quotas depend on your [Twelve Data plan](https://twelvedata.com/pricing). The widget labels prices as potentially delayed. If a request fails, it keeps any cached prices with their timestamps and displays the problem; it does not substitute example prices. Cache is held only in memory and cleared when the widget is hidden.

## Credentials and privacy

The API key is saved in the desktop's Secret Service keyring. Halcyon never saves it in its configuration or profile exports. Background updates do not prompt to unlock the keyring; use Markets settings to save the key again after unlocking if needed. **Remove API key** deletes Halcyon's saved Markets credential.

Watchlist symbols and the selected stock remain local and are excluded from shared appearance profiles. Quotes are requested directly from Twelve Data over HTTPS; that provider receives your API key and requested symbols. Halcyon does not send the watchlist through its usage reporting.
