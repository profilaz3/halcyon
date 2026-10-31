# Privacy

Halcyon **0.2.5** introduced a reporting checkbox. Halcyon 0.2.0 reports automatically;
older 0.1.0/0.1.1 releases do not report.

**Collector status:** upgraded on 2026-10-26. Live checks verified activity and
preference updates from current and older clients, including opt-out, replay and
explicit re-enable. Current counts exclude retained opted-out installations.
Turning reporting off works locally without server acknowledgement. Preference
updates retry for up to seven days, then expire without changing the saved
choice; expired updates are not sent later. Reports rejected before the upgrade
are not reconstructed, so historical counts remain incomplete.

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

The installation token is randomly generated locally with 256 bits of entropy.
It is not a Google identity, machine ID, username, email or hardware identifier.
It stays the same across restarts and upgrades. Copying or deleting local state
can affect counts: these are installations, not accounts, individual people or
downloads. A token is pseudonymous, **not anonymous**.

Reports contain no calendar events, music, notes, project names or paths,
weather locations, hardware details, crash logs or screenshots. There is no
tracking of clicks or feature use.

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

Reports go over HTTPS to the maintainer's Google Apps Script web app. It hashes
the token with SHA-256 before storing it in a **private Google Sheet**. The
public endpoint accepts reports but exposes no stored-record read API. The
collector does not store IP addresses or log bodies. Google receives the
connection's IP and may process infrastructure metadata under its own
[privacy policy](https://policies.google.com/privacy).

## Status, counts and retention after the collector upgrade

The collector is live. The private dashboard uses the existing daily reports;
the reporting upgrade does not collect feature use or add client requests.

Each installation has one current row. The row keeps
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
observation, not a package-manager transaction log.

Duplicate delivery does not refresh status receipt or retention. Older status
revisions and mismatched activity revisions cannot replace the current choice.
Legacy schema 1/2 activity cannot re-enable a retained opted-out record.

Current active counts (today / seven / 30 days) and first-country counts exclude
opted-out installations. Status-only installations are never counted as active.
The separately labelled opt-out metric means **currently opted out, with status
received in the last 90 days**, not lifetime opt-outs or unique people. Snapshot
dates are displayed; already saved historical aggregates are not rewritten.

Opted-out identifiable rows expire within 90 days of collector receipt, even
if old clients replay activity. Enabled rows expire after 90 days without
activity or a new choice; old identifiable choice timestamps are removed after
90 days while ongoing activity may keep that installation row current. Daily
maintenance and ingestion apply retention. Anonymous aggregate totals may be
retained. After an identifiable record expires, the collector no longer has its
revision/status tombstone; ordering guarantees apply to retained records.
The local off preference continues to prevent reporting. Google file history,
backups and infrastructure logs follow Google's own retention practices.

## Local state

`~/.local/state/halcyon/usage.json` (or the XDG state directory) contains the
random token, preference, revision, latest opt-out/choice times, cached country,
optional first desktop start and bounded retry state. The file is private to the local user and is excluded
from exported looks and setups. Older 0.2.0 clients reject this extended state
rather than ignoring the off preference. Uninstalling preserves it normally;
stopping the desktop and Settings processes stops future transmissions.

The [client](../src/usage.rs) and [collector](../scripts/usage/collector.gs) are
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
