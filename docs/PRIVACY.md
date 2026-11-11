# Privacy

Halcyon **0.2.5** introduced a reporting checkbox. Halcyon 0.2.0 reports automatically;
older 0.1.0/0.1.1 releases do not report.

**Collector status:** Halcyon 0.6.1 moved reporting from a Google Apps Script
web app and private Google Sheet to a Cloudflare Worker with a private
Cloudflare D1 database. Halcyon 0.6.1 and later report to the new collector.
Halcyon 0.2.0 to 0.6.0 keep reporting to the Google Apps Script collector, which
stores them in the private Google Sheet as before, under the same 90-day
retention. The two are kept separate: older reports are not forwarded, and the
public statistics count only 0.6.1 and later. Records from before the move were
copied across once, keeping every reporting choice. Turning
reporting off works locally without server acknowledgement. Preference updates
retry for up to seven days, then expire without changing the saved choice;
expired updates are not sent later. Reports rejected before the 2026-10-26
upgrade are not reconstructed, so historical counts remain incomplete.

## Your reporting choice

**Share usage data** is checked by default in first-run setup and appears under
App settings → Privacy. Uncheck it to stop new daily activity and country
requests. The choice is saved independently of themes, portable setups and Undo,
and survives restarts and upgrades. Running setup again preserves it.

Turning reporting off **still sends one minimal preference update**: a random
installation token, whether reporting is enabled, the UTC action time and a
monotonically increasing preference revision (plus format/type fields). On first
setup, a random token is created solely for this disclosed status update if none
exists. No country lookup, app version, device identity or widget content is
included. Re-enabling sends the newer explicit choice, using the same token.

The latest choice can be queued locally for up to seven days, with at most 28
attempts spaced at least six hours apart. A newer choice replaces the queued
one; there is no history of queued choices. Clock rollback expires the queue
rather than extending it. Disabling takes effect locally without waiting for
delivery. Failed delivery can make the collector's status count incomplete.

Settings saves without waiting on the network or desktop transactions. A save
failure is shown next to the checkbox; the saved setting must be checked before
closing. Damaged reporting state stops reporting instead of silently enabling
it or registering another identity. A separate running desktop observes saved
changes; in-flight requests are cancelled where possible, with checks at most
50 milliseconds apart during network work. A request already transmitted cannot
be unsent. No further country or activity requests are started once the changed
preference is observed; stale responses cannot save a country or activity date.

## Daily activity while enabled

The purpose is to estimate how many installations remain in use. Schema 3
activity contains `schema`, `kind` (`activity`), `installation`, `revision`,
`version`, and `country` (a two-letter first country code, or `null`).
The 0.3.0 client uses schema 4, adding `first_started_at`: the UTC
second of the first desktop start, or `null` when unknown. It is recorded locally
only for a new enabled reporting identity, before network warmup. It is not an
exact package-install time. Existing installations are not assigned an invented
historical date, and filesystem timestamps/package logs are not inspected.
The field is absent from opt-out/status messages. First-run opt-out does not
create it; an already recorded date stays local and can be reported again after
explicit re-enable. The collector accepts this field; historical start dates
remain unknown where no client supplied one.

The next client uses schema 5 and adds three bounded fields to the same daily
request: `package_format` (`deb`, `rpm`, `source` or `unknown`), `update_channel`
(`apt`, `dnf`, `manual`, `disabled`, `source` or `unknown`) and
`installed_app_version` (the verified product version, or `null`). These come
from local package metadata. Repository URLs, package inventories and native
package release suffixes are not sent. The running app version remains separate:
an update can be installed while an older process is still open. These fields
are absent from preference/opt-out messages and create no extra requests.

Halcyon 0.6.1 and later use schema 6 and add one `features` object to the same
daily request: which headline features are switched on in your saved settings
at that moment. It has exactly these fields, each `null` when the setting could
not be read:

- `wallpaper_effect`: `off`, `rain`, `snow`, `fog` or `clouds`
- `activity_island`, `penumbra`, `islands`: whether the Activity island applet,
  the Penumbra bar gradient or Islands is on for a listed panel or dock
- `widgets`: which of the built-in widgets are shown (`calendar`, `projects`,
  `devbox`, `weather`, `clock`, `glance`, `music`, `notes`, `focus`)
- `visualizer`, `wallpaper_depth`, `automation`: whether the audio visualizer,
  wallpaper depth or setup automation is on
- `theme_style`: the built-in theme style name, or `default`

These are read from local settings files only, once per daily report. They are
switch positions. Weather locations, calendar or player names, custom theme or
font names, automation rules, display names, widget positions and widget
contents are never included. The feature snapshot is covered by the same
**Share usage data** choice and is absent from preference/opt-out messages.

### Schema 7: use, problems, setup and system

The next client uses schema 7 and adds five fields to the same daily request.
They describe the time since the last report the collector accepted. Every key
comes from a fixed list in the app and the collector refuses any other key; each
count is a whole number from 1 to 100000, and keys with nothing to count are
left out.

- `pages`: how many times each Settings page was opened, by its fixed page key:
  `overview`, `layout`, `pending`, `appearance`, `widgets`, `glance`,
  `calendar`, `weather`, `devbox`, `clock`, `projects`, `music`, `notes`,
  `focus`, `general`, `desktop`, `desktop-dock`, `desktop-appearance`,
  `desktop-wallpaper`, `desktop-icons`, `desktop-profiles`, `themes`,
  `desktop-windows`, `desktop-workspaces`, `desktop-motion`,
  `desktop-window-rules`, `desktop-shortcuts`, `desktop-automation`,
  `moments`, `adaptive`, `builder` and `changes`. A page counts when Settings
  opens on it and each time you move to it. Nothing you type or search for is
  recorded.
- `runtime_hours`: how long the desktop process ran, counted in ten-minute
  steps and rounded to the nearest whole hour (0 to 10000). Time asleep or
  suspended is not counted.
- `problems`: how many times each of these happened:
  - `panic`: a Halcyon process hit an internal error (a Rust panic). Only the
    fact is counted; no message, location or log is kept or sent.
  - `render_timeout`: a wallpaper or picture took too long to prepare.
  - `update.<kind>`: **Check for updates** failed, and
    `islands.<kind>`: turning on Islands failed, where `<kind>` is one of
    `missing_packagekit`, `offline`, `timeout`, `authorization`, `untrusted`,
    `invalid_response`, `failed`, `installation_changed`, `activation`,
    `islands_mismatch` or `islands_refused`. A check you cancel is not counted.
- `setup`: how far first-run setup got: `opened`, `left` (skipped from the
  welcome screen), `kept_for_later`, `cancelled`, `finished`, `undone`,
  `reached.<step>` for `look`, `bars`, `widgets`, `arrange`, `weather`, `ready`
  and `done`, and `skipped.<step>` for `bars`, `widgets` and `arrange`. None of
  the choices made in setup are included.
- `system`: a few facts read at report time, each `null` when unknown:
  - `distro` and `distro_version`: `ID` and `VERSION_ID` from
    `/etc/os-release` (or `/usr/lib/os-release`), for example `pop` and
    `24.04`. Only lowercase letters, digits, `.`, `_` and `-` are allowed, up to
    32 and 16 characters; any other value is sent as `null`, not cleaned up.
  - `cosmic`: the installed version of the `cosmic-session` package, read from
    the package database (dpkg, RPM on Fedora, or pacman), as up to three
    numbers such as `1.0.3`. Halcyon never starts COSMIC programs to ask.
  - `gpu`: the makers of the graphics cards, each `amd`, `intel`, `nvidia` or
    `other`, from the PCI vendor number in `/sys/class/drm`. Model names and
    serial numbers are not read.
  - `displays` and `resolution`: how many displays the desktop shows (1 to 16)
    and the largest one's current resolution, such as `3840x2160`. Display
    names, makers, positions and scale are not included.

For example:

```json
"pages": {"appearance": 2, "weather": 1},
"runtime_hours": 5,
"problems": {"update.offline": 1},
"setup": {"opened": 1, "reached.look": 1, "skipped.bars": 1},
"system": {"distro": "pop", "distro_version": "24.04", "cosmic": "1.0.3",
           "gpu": ["amd"], "displays": 2, "resolution": "3840x2160"}
```

The counts are kept in `~/.local/state/halcyon/usage-counts.json` until they
are sent. Any Halcyon process adds to it, and only while **Share usage data** is
on: nothing is counted while it is off, and turning it off deletes the file.
Once the collector accepts a report, the counts it carried are subtracted, so
each one is sent once. A failed report keeps them for the next try. The
collector stores them on that day's row; a second report on the same UTC day
replaces it rather than adding to it. If an accepted reply is lost and the
retry lands on a later UTC day, those counts can be stored twice.

The installation token is randomly generated locally with 256 bits of entropy.
It is not a Google identity, machine ID, username, email or hardware identifier.
It stays the same across restarts and upgrades. Copying or deleting local state
can affect counts: these are installations, not accounts, individual people or
downloads. A token is pseudonymous, **not anonymous**.

Reports contain no calendar events, music, notes, project names or paths,
weather locations, file paths, host or user names, language or locale, typed
text, crash logs or screenshots. Hardware is limited to the graphics maker,
display count and largest resolution listed above. Use is limited to the page
counts and running hours above: no clicks, times of day or per-page durations.

### Approximate country

Before the first activity report, Halcyon requests an approximate country from
[Country.is](https://country.is/) over HTTPS. The service receives the connection's
public IP, but no installation token or widget content. Halcyon discards the
returned IP and caches only the first successful country code. No fresh lookup
is made when reporting is off, including first-run opt-out. A previously cached
country remains local and can be used if reporting is re-enabled.

The code is not travel history or a live location. Later VPN use or moving does
not update it; VPNs, proxies and geolocation errors can make it inaccurate.
No GPS, precise location, weather city or account location is collected. A
failed lookup uses an unknown country and may retry with a later activity
report. Country.is operates independently under its own infrastructure practices.

### Timing and recipients

Activity waits until the desktop process has run for five minutes. Successful
reports occur at most once per local UTC day, with collector dates assigned by
the server. Failed attempts wait at least six hours, including after restart.
There is no activity backfill or offline daily-report queue. Opening Settings
alone can deliver a preference update but never daily activity or country lookups.
Debug builds, tests and isolated captures/previews cannot automatically report.
Installing a package alone does not send a report. The worker sleeps between
scheduled work and reporting-preference file changes; there is no routine
30-second reporting poll. A local checkbox change signals the worker immediately.
Failed file watching/timer setup or a busy state file can cause a one-minute
recovery check, without bypassing daily limits or six-hour network retry spacing.

Reports go over HTTPS to the maintainer's collector at
`halcyon-reports.profilaz3.workers.dev`, a Cloudflare Worker. It hashes the
token with SHA-256 before storing it in a **private Cloudflare D1 database**.
The collector does not store IP addresses and does not log request bodies;
Worker request logging is turned off. Cloudflare receives the connection's IP
and processes infrastructure metadata under its own
[privacy policy](https://www.cloudflare.com/privacypolicy/). Reports from
Halcyon 0.2.0 to 0.6.0 go to the Google Apps Script collector and its private
Google Sheet instead; Google receives the connection's IP there under its own
[privacy policy](https://policies.google.com/privacy).

### Public statistics

The totals are published at
[halcyon-reports.profilaz3.workers.dev](https://halcyon-reports.profilaz3.workers.dev/),
together with the same numbers as JSON. Only aggregates are published: daily
active and new installation counts, version shares, how Halcyon was installed,
update channel shares, how many days installations were active, first-country
counts, weekly return rates, the share of installations that turned
reporting off and, from 0.6.1, the share of installations with each headline
feature, widget, theme style and wallpaper effect switched on. From schema 7
it also shows, over the last 28 days, how many installations opened each
Settings page and how often, how many hit each problem, how far setup got,
running hours in ranges with the median, and the distributions, COSMIC
versions, graphics makers, display counts and largest resolutions from each
installation's latest report. No installation token, hash or per-installation
record is published. Counts are published exactly, without a minimum group
size, so a group of one installation is visible as 1.

## Status, counts and retention after the collector upgrade

The statistics use the existing daily reports; the move to Cloudflare added no
fields or client requests. The schema 6 feature snapshot and the schema 7
fields travel inside the same daily request and add no requests.

Each installation has one current record. The row keeps
its latest activity date, version, first country, current reporting status,
latest opt-out action time, latest choice time, collector receipt time and
preference revision. The 0.3.0 collector also keeps first and latest activity receipt times,
first observed version, optional client first-start time, and a bounded set of
reported UTC activity days with its count for the last 90 days. The reporting
upgrade also keeps observed version changes for up to 90 days, with at most the
last observed version per UTC day. It uses versions already in activity reports,
starts when the upgraded collector receives them, and does not reconstruct old
upgrade dates. Existing rows
start with unknown history; the first new receipt is explicitly labelled as
observed since tracking began, not an install date. Missed/offline days are not
backfilled, and repeated reports count once per UTC day. A status update never
advances activity or supplies startup time, country or version. Re-enabling preserves the most recent opt-out time until that
identifiable choice information ages out. Client action time is informational:
clock skew cannot change receipt dates, retention or preference ordering.

Schema 5 keeps at most one package/channel observation per UTC day for 90 days,
alongside the running and installed product versions reported that day. Earlier
clients and days without these fields remain unknown. A channel change is an
observation, not a package-manager transaction log. Schema 6 keeps the feature
snapshot on the same daily row, so at most one per UTC day for 90 days, and it
is deleted with the rest of the daily activity when reporting is turned off.
Schema 7 keeps its five fields the same way: on that day's row, in a fixed
order, replaced by a later report the same day, kept for 90 days and deleted
when reporting is turned off.

Turning reporting off deletes that installation's stored daily activity at once.
Its record keeps only what is needed to honor the choice (status, revision,
choice times and the earlier first-use facts) until it expires.

Duplicate delivery does not refresh status receipt or retention. Older status
revisions and mismatched activity revisions cannot replace the current choice.
Legacy schema 1/2 activity cannot re-enable a retained opted-out record.

Current active counts (today / seven / 30 days) and first-country counts exclude
opted-out installations. Status-only installations are never counted as active.
The separately labelled opt-out metric means **currently opted out, with status
received in the last 90 days**, not lifetime opt-outs or unique people. Each
completed UTC day is summarized once into anonymous daily totals, which are kept
after the identifiable records expire and are not rewritten later.

Opted-out identifiable rows expire within 90 days of collector receipt, even
if old clients replay activity. Enabled rows expire after 90 days without
activity or a new choice; old identifiable choice timestamps are removed after
90 days while ongoing activity may keep that installation row current. Daily
maintenance, run every hour, applies retention. Anonymous aggregate totals are
retained. After an identifiable record expires, the collector no longer has its
revision/status tombstone; ordering guarantees apply to retained records.
The local off preference continues to prevent reporting. Cloudflare's D1
point-in-time recovery keeps database history for up to 30 days, and
infrastructure logs follow Cloudflare's own retention practices. The Google
Sheet that still receives reports from Halcyon 0.2.0 to 0.6.0 applies the same
retention in its daily maintenance; its backups follow Google's own practices.

## Local state

`~/.local/state/halcyon/usage.json` (or the XDG state directory) contains the
random token, preference, revision, latest opt-out/choice times, cached country,
optional first desktop start and bounded retry state. `usage-counts.json` next
to it holds the schema 7 counts not yet sent. Both files are private to the
local user and are excluded from exported looks and setups. Older 0.2.0 clients reject this extended state
rather than ignoring the off preference. Uninstalling preserves it normally;
stopping the desktop and Settings processes stops future transmissions.

The [client](../src/usage.rs), [collector and public statistics](../scripts/usage/worker/) and [Google Sheet collector](../scripts/usage/collector.gs) are
open source. Questions can go to the [issue tracker](https://github.com/profilaz3/halcyon/issues).
Do not post installation tokens, credentials or private widget contents there.

## Widget connections

- **Weather:** searches and selected coordinates go to Open-Meteo.
- **Google Calendar:** connecting an account allows Halcyon to read the calendars
  you select. Sign-in tokens stay in the system keyring.
- **Calendar subscriptions (0.3.0):** HTTPS/iCal links are fetched
  directly from their provider. Private links stay in the system keyring, not in
  preferences, exported setups or logs. The provider receives normal HTTPS
  requests and the connection IP. Redirects do not send the private link as a
  Referer header. Halcyon does not send feeds through its own server.
- **Cached agenda:** event titles, times, descriptions, locations and links are
  cached in private local files for offline viewing. Disconnecting removes this
  cache. “Hide event details” displays Busy on the desktop and hides event/Join
  links; it does not erase the local cache. Calendar contents never go to the
  activity collector.
- **Music:** controls use local MPRIS over D-Bus. Artwork may be fetched from the
  public HTTPS URL supplied by the player.
- **Projects, notes and settings:** remain local; activity reports do not include
  their contents.

## Scene model

“Let the scene cover the clock” on your own pictures needs a scene model. It
downloads only when you choose Download: one 99 MB file from Hugging Face, fetched
once over HTTPS, checked against a pinned fingerprint and kept in
`~/.local/share/halcyon/models`. Hugging Face receives a normal download request
and the connection IP. Your pictures are analyzed on this computer; nothing is
uploaded, and the results stay in `~/.cache/halcyon/depth`. Remove, on the Scene
model row, deletes the model and those results.

## System update checks

App settings reads installation and package-source information locally. It does
not poll for updates. **Check for updates**, available only for a verified system
installation, asks PackageKit to refresh the system's configured package sources
and then checks for a newer Halcyon package. Those sources receive the usual
package-manager requests and connection IP; Halcyon sends no reporting token or
widget data. The check can be cancelled. Authentication and installation belong
to the operating system; Halcyon never asks for a password or installs updates.
**Open system updates** opens COSMIC Store, where you choose Updates. If the
Store is unavailable, a verified enrolled installation can copy its APT or DNF
command. Halcyon does not run that command or enable a disabled source. Release and
installation-information links open GitHub only when selected. System update
enrollment is unavailable until the source and bootstrap assets are verified.

## Audio visualization

Spectrum and circular Music visualizers are optional and off by default. When
enabled, Halcyon reads the default output monitor locally while the selected media
player is playing. This includes other sound on that output. It does not select
the microphone, save audio, or transmit samples. Capture stops when Music is hidden,
playback stops, Arrange is open, or Reduce motion is enabled.
