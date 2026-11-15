# Calendar setup

Halcyon reads events from iCal subscription links, CalDAV accounts and local
`.ics` files. You can add up to eight calendars, mixing these kinds freely, and
choose which ones appear. Event editing stays in your calendar app. None of them
needs a Google API key or OAuth client configuration.

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

Other providers work if they supply a compatible iCal subscription. Feeds that
need a browser sign-in are not supported; for a feed that asks for a username
and password, see [Links that need a password](#links-that-need-a-password).

## Add it to Halcyon

1. Open **Settings → Widgets → Calendar**.
2. Select **Add calendar link** under **Calendars & connection**.
3. Enter a name, paste the subscription link and select **Add calendar**.
4. Unlock your system keyring if prompted. Halcyon checks the feed before saving it.
5. Enable Calendar, or the Calendar section of **At a glance**, to show events
   on the desktop.

Repeat for additional calendars. The switches beside their names select which
feeds appear. Adding a link, a CalDAV calendar or a file replaces an existing
Google sign-in connection on this computer; Google sign-in cannot be combined
with the other kinds.

### Links that need a password

Some servers protect a feed with a username and password instead of a secret
link. Turn on **This link needs a password** under the link and enter them.
Halcyon sends them with HTTP Basic authentication and keeps them in the system
keyring with the link. If the server answers that they are wrong, Settings says
"The server did not accept the username or password."

## CalDAV accounts

CalDAV lets Halcyon read the calendars of an account directly, including ones
that are not shared or published.

1. Open **Settings → Widgets → Calendar** and select **Add CalDAV account**.
2. Enter the server address, your username and your password. Use an app
   password if your provider offers one.
3. Select **Find calendars**. Halcyon looks up the account's calendars, with
   their names and colors.
4. Turn on the calendars you want and select **Add calendars**.

The address can be the server's name, such as `caldav.fastmail.com`, the
account's full CalDAV address, or the address of one calendar. Halcyon follows
the standard discovery steps (`/.well-known/caldav`, then your principal and its
calendar home) and reads each month with one CalDAV query. Calendars that only
hold tasks are left out.

| Provider | Server address | Notes |
| --- | --- | --- |
| iCloud | `caldav.icloud.com` | Create an app-specific password at account.apple.com. |
| Fastmail | `caldav.fastmail.com` | Use an app password with CalDAV access. |
| Nextcloud | `https://your.server/remote.php/dav` | An app password from Security settings works best. |
| mailbox.org | `dav.mailbox.org` | |
| Zoho | `calendar.zoho.com` | Use an app specific password if two factor sign in is on. |
| Yahoo | `caldav.calendar.yahoo.com` | Generate an app password in account security. |
| Radicale | `http://host:5232` | Plain http only on your network, see below. |
| Baikal | `https://your.server/dav.php` | |

Google Calendar's CalDAV needs an OAuth sign-in, so a Google password will not
work there. Use Google sign-in or a Google calendar link instead.

Remove a CalDAV calendar with **Remove** beside it. Adding it again needs the
password once more.

## Calendar files

**Add calendar file…** opens the file chooser for an `.ics` file on this
computer, such as an export or a file kept in sync by another app. Halcyon
reads the file again on every refresh and whenever it changes on disk. Files go
through the same size and recurrence limits as links. The file's path is kept
in Halcyon's private calendar settings; it is not a secret, needs no keyring
and is never part of usage reports.

## Plain http on your network

Links and CalDAV servers normally use HTTPS. Plain `http://` addresses are
accepted only for servers on your own network: `localhost`, names ending in
`.local`, `.lan`, `.home.arpa` or `.internal`, single word names such as `nas`,
and private addresses (`10.0.0.0/8`, `172.16.0.0/12`, `192.168.0.0/16`,
`100.64.0.0/10` for Tailscale, `fc00::/7`, `fe80::/10` and loopback). Halcyon
resolves the name and checks every address it gets, so a name that points at a
public server is refused. Settings shows "This server is on your network and the
connection is not encrypted." under such an address.

Redirects are followed a few times at most. A redirect from https to http is
refused, and the username and password are never sent to a different host.

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
- **Username or password not accepted:** check them, and use an app password
  where your provider requires one (iCloud, Fastmail, Yahoo, Zoho).
- **Not a CalDAV server:** enter the full CalDAV address from your provider's
  help pages, such as `https://your.server/remote.php/dav` for Nextcloud.
- **Plain http refused:** the server is not on your network. Use its https
  address.
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
