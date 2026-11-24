# Calendar setup

Halcyon reads events from iCal subscription links, CalDAV accounts, local
`.ics` files and folders, and public holiday calendars. You can add up to eight
calendars, mixing these kinds freely, and choose which ones appear. Event editing stays in your calendar app. None of them
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

Halcyon recognizes Windows time zone names in published calendars. For example,
a weekly 9 AM meeting in `Eastern Standard Time` stays at 9 AM Eastern when
daylight saving time starts or ends.

### iCloud

On iCloud.com, open Calendar and the information button beside your calendar.
Enable **Public Calendar**, then copy its link. This makes the calendar readable
by anyone with that link; private invitation-only sharing does not provide the
subscription Halcyon needs.
[Apple's instructions](https://support.apple.com/en-gb/guide/icloud/-mm6b1a9479/icloud).

### Proton Calendar

1. In Proton Calendar, open **Settings → Calendars** and choose your calendar.
2. Under **Share with anyone**, select **Create link**, and choose limited or
   full view.
3. Copy the link.

Anyone with the link can read the calendar at the level you chose.
[Proton's instructions](https://proton.me/support/share-calendar-via-link).

### Fastmail

1. Open **Settings → Calendars** and select **Edit & share** beside the calendar.
2. Turn on the public link and copy its ICS address.

Fastmail accounts can also be added as a [CalDAV account](#caldav-accounts),
which needs no public link. [Fastmail's instructions](https://www.fastmail.help/hc/en-us/articles/360060590793).

### Nextcloud

1. In the Calendar app, open the pencil button beside the calendar.
2. Select the plus button beside **Share link**, then the three dots beside the
   new link, and **Copy subscription link**.

Use the subscription link, not the public page link. Unsharing and sharing again
makes a new address. A Nextcloud account can also be added as a
[CalDAV account](#caldav-accounts).

### Zoho Calendar

1. Open **Settings → Calendar → Calendars → My Calendars** and select the
   calendar.
2. Under **Private URL**, select **iCal** to copy the address.

[Zoho's instructions](https://www.zoho.com/calendar/help/ways-to-share-calendars.html).

### Yahoo Calendar

1. Open the calendar in full view, then choose **Actions → Share**.
2. Turn on the shareable link and copy the ICS link under importing into a
   calendar app, not the HTML link.

### Teamup

1. Open Teamup in a browser and choose the menu at the top right, then
   **Preferences → iCalendar feeds**.
2. Copy the feed address for one sub calendar, or the one for all of them at
   the bottom of the list.

A feed with several sub calendars arrives as one calendar with one color.
[Teamup's instructions](https://calendar.teamup.com/kb/how-to-use-icalendar-feeds/).

### Tuta

Tuta (Tutanota) shares calendars only with other Tuta users and does not publish
an iCal link, so it can't be added today.

### Other providers

Other providers work if they supply a compatible iCal subscription. Feeds that
need a browser sign-in are not supported; for a feed that asks for a username
and password, see [Links that need a password](#links-that-need-a-password).

## Add it to Halcyon

1. Open **Settings → Widgets → Calendar**.
2. Select **Add calendar…** under **Calendars**, then **Calendar link**.
3. Enter a name, paste the subscription link and select **Add calendar**.
4. Unlock your system keyring if prompted. Halcyon checks the feed before saving it.
5. Enable Calendar, or the Calendar section of **At a glance**, to show events
   on the desktop.

Repeat for additional calendars. The switches beside their names select which
feeds appear. Adding a link, a CalDAV calendar or a file replaces an existing
Google sign-in connection on this computer; Google sign-in cannot be combined
with the other kinds.

**Add calendar…** lists every kind of calendar Halcyon can show: Online
Accounts (when the system has them), CalDAV account, Calendar link, Holidays,
Calendar file and Calendar folder. Choosing one opens its form in place of the
list, and **Cancel** goes back to the list. When Google sign-in is set up for
this build, **Connect Google Calendar** stays beside it.

### Links that need a password

Some servers protect a feed with a username and password instead of a secret
link. Turn on **This link needs a password** under the link and enter them.
Halcyon sends them with HTTP Basic authentication and keeps them in the system
keyring with the link. If the server answers that they are wrong, Settings says
"The server did not accept the username or password."

## CalDAV accounts

CalDAV lets Halcyon read the calendars of an account directly, including ones
that are not shared or published.

1. Open **Settings → Widgets → Calendar** and select **Add calendar…**, then **CalDAV account**.
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

**Add calendar…** then **Calendar file** opens the file chooser for an `.ics` file on this
computer, such as an export or a file kept in sync by another app. Halcyon
reads the file again on every refresh and whenever it changes on disk. Files go
through the same size and recurrence limits as links. The file's path is kept
in Halcyon's private calendar settings; it is not a secret, needs no keyring
and is never part of usage reports.

## Calendar folders

**Add calendar…** then **Calendar folder** opens the folder chooser. It is meant for folders kept
in sync by tools such as vdirsyncer, and read by khal: one `.ics` file per
event, or whole calendars in a file.

- `.ics` files in the chosen folder make one calendar, named after the folder.
- Each subfolder holding `.ics` files is a calendar of its own. Its `displayname`
  and `color` files, as vdirsyncer writes them, set its name and color.
- Folders deeper than one level, hidden files and other files are not read.

Settings lists the calendars it found, with a switch for each. Halcyon reads a
folder again on every refresh and whenever one of its calendar files changes.
All files in one folder count toward the same size limit as one link (4 MB), so
a very large folder reports that it is too large instead of slowing the desktop.
Like a calendar file, the folder's path needs no keyring and is never part of
usage reports.

## Public holidays

**Add calendar…** then **Holidays** offers a list of countries. Halcyon adds that country's
holiday calendar from the public holiday calendars Thunderbird publishes at
`thunderbird.net`. It is fetched like any calendar link, needs no account, and
appears in a muted gray so it stays behind your own events. Remove it like any
other calendar.

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

## System accounts

Halcyon can also show calendars from accounts you already added to the system,
with no link and no sign-in in Halcyon. This is the way to get Google calendars
without a subscription link, including work and school accounts that don't allow
secret addresses. Nextcloud, WebDAV and other CalDAV accounts, calendars kept on
this computer, and birthdays from your contacts appear the same way.

Microsoft 365, Outlook and Exchange calendars need Evolution's Microsoft support,
which Pop!_OS does not install by default. Install it with
`sudo apt install evolution-ews`, sign out and back in, and keep the account's
**Calendar** switch on in Online Accounts. Without it those accounts give mail or
files but no calendars, and they don't appear in Halcyon's list.

1. Add the account in **Online Accounts** (`gnome-online-accounts-gtk` on
   Pop!_OS and COSMIC) and leave its **Calendar** switch on.
2. In **Settings → Widgets → Calendar**, select **Add calendar…**
   under **Calendars**, then **Online Accounts**. That row is listed when
   Evolution Data Server is available or the Online Accounts app is installed.
3. Turn on the calendars you want. The **Online Accounts…** button opens the
   accounts app when it is installed.

System calendars join the same list as calendar links: eight calendars in all,
each with its switch, **Remove** and **Check connection**. Evolution Data Server
keeps the accounts and syncs their events; Halcyon reads them from it on the
usual refresh interval and only reads. If Evolution Data Server isn't running,
Halcyon shows "System calendars are not available" and your link calendars keep
updating. A calendar renamed or recolored in its account shows the new name or
color after you switch it off and on again in the list.

## What the agenda shows

Calendar's **What to show** group chooses the days and events:

- **Days shown**: Today, Tomorrow, Today and tomorrow, or Next 7 days. The week starts today and puts
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
- **No .ics calendar files in a folder:** choose the folder that holds the
  `.ics` files, or the folder right above the calendars' own folders.
- **Unrecognized time zone:** Halcyon accepts IANA city names and Windows time
  zone IDs. A feed can also give a custom name an IANA location using
  `X-LIC-LOCATION` in its `VTIMEZONE`. If the feed defines its own rules
  without a recognized location, ask the publisher for a named zone or UTC.
- **Unsupported or oversized calendar:** Halcyon limits feed size and recurrence
  processing. Try a smaller feed if the calendar is large or uses an
  unsupported recurrence rule.

## Google sign-in

Google sign-in is a separate connection method. Current releases require your
own OAuth client configuration, and live sign-in validation is pending. The
[Google sign-in setup guide](google-calendar-setup.html) covers that configuration.
The subscription steps above do not require it.
