# Calendar setup

Halcyon reads events from HTTPS/iCal subscription links. You can add up to eight
calendars and choose which ones appear. Event editing stays in your calendar app.
Subscription links work without a Google API key or OAuth client configuration.

## Get a subscription link

Use a link that returns iCalendar data directly, usually ending in `.ics`.
An ordinary calendar webpage, invitation or sign-in link will not work. HTTPS
links are supported; Halcyon converts `webcal://` links to HTTPS automatically.

### Google Calendar

1. Open Google Calendar in a desktop browser and open **Settings**.
2. Under **Settings for my calendars**, select the calendar you want.
3. Open **Integrate calendar** and copy **Secret address in iCal format**.

You do not need to make the calendar public. If the secret address is missing
on a work or school account, ask your administrator whether it is allowed.
[Google's instructions](https://support.google.com/calendar/answer/37648?hl=en).

### Outlook

1. Open Outlook on the web and go to **Settings → Calendar → Shared calendars**.
2. Under **Publish a calendar**, choose the calendar and how much detail to share.
3. Select **Publish** and copy the **ICS** link.

Published details are accessible to anyone with the link. Use the ICS address,
not the HTML address.
[Microsoft's instructions](https://support.microsoft.com/en-us/outlook/share-your-calendar-in-outlook-com).

### iCloud

On iCloud.com, open Calendar and the information button beside your calendar.
Enable **Public Calendar**, then copy its link. This makes the calendar readable
by anyone with that link; private invitation-only sharing does not provide the
subscription Halcyon needs.
[Apple's instructions](https://support.apple.com/en-gb/guide/icloud/-mm6b1a9479/icloud).

Other providers work if they supply a compatible HTTPS iCal subscription.
CalDAV, local `.ics` files and feeds requiring a separate username/password or
browser sign-in are not supported.

## Add it to Halcyon

1. Open **Settings → Widgets → Calendar**.
2. Select **Add calendar link** under **Calendars & connection**.
3. Enter a name, paste the subscription link and select **Add calendar**.
4. Unlock your system keyring if prompted. Halcyon checks the feed before saving it.
5. Enable Calendar, or the Calendar section of **At a glance**, to show events
   on the desktop.

Repeat for additional calendars. The switches beside their names select which
feeds appear. Adding a subscription replaces an existing Google sign-in
connection on this computer; the two connection types cannot be combined.

## What the agenda shows

Calendar's **What to show** group chooses the days and events:

- **Today**, **Tomorrow** or **Next 7 days**. The week starts today and puts
  each day with events under its own heading, such as "Wednesday 18". Near a
  month's end Halcyon also fetches the next month.
- **All-day events** can be left out.
- **Hide finished events** removes today's events once they end, instead of
  folding them under **Earlier**.
- **Show event location** adds the place under each title. Links to calls are
  left out; they are offered as **Join**.
- **Hide event details** shows Busy instead of titles and places.

**How it looks** has **Calendar colors**, which marks each event in its
calendar's color (off, every event uses the widget accent), and **Highlight
meetings**.

## Updates and privacy

Events refresh periodically while the widget is active. Change the interval in
Calendar's advanced **Refresh** controls. For connection problems, open the
account options menu beside the connection header and choose **Check connection**.
Provider changes may take time to appear in the feed.

Treat private subscription links like passwords. Halcyon stores them in the
system keyring and fetches calendars directly from their providers. Cached events
remain in private local files for offline viewing. **Show Busy instead of titles**
hides details on the desktop; it does not erase the local cache.

Use **Remove** beside a feed to stop using it. **Disconnect** in account options
removes Halcyon's connection and cached events without changing the original
calendars. It does not revoke the provider's link. If a link is exposed, reset it
or unpublish it at the provider. [Privacy details](PRIVACY.md#widget-connections).

## If it does not connect

- **Invalid link:** copy the provider's iCal/ICS subscription address again.
  Links requiring a browser login cannot be fetched by Halcyon.
- **Keyring unavailable or locked:** start or unlock a Secret Service-compatible
  keyring such as GNOME Keyring or KWallet, then retry.
- **Cannot reach the provider:** check the connection and whether the provider
  still publishes the feed. Previous cached events remain available.
- **Unsupported or oversized calendar:** Halcyon limits feed size and recurrence
  processing. Custom time-zone definitions and some recurrence features are not
  supported; use a smaller compatible feed when available.

## Google sign-in

Google sign-in is a separate connection method. Current releases require your
own OAuth client configuration, and live sign-in validation is pending. The
[Google sign-in setup guide](google-calendar-setup.html) covers that configuration.
The subscription steps above do not require it.
