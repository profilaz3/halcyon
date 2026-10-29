# Activity and reporting-choice collector

The live private collector accepts schema 1–4. Local verification needs no live
endpoint or connected Google service. Reporting views are described below.

## Wire protocol and compatibility

Schema 1 (`schema`, `installation`, `version`) and schema 2 (plus `country`)
remain accepted. Neither can turn a retained opted-out row back on.
Schema 3 has two exact shapes, with no extra keys:

```json
{"schema":3,"kind":"activity","installation":"<64 lowercase hex>","revision":0,"version":"0.2.1","country":null}
{"schema":3,"kind":"status","installation":"<64 lowercase hex>","revision":1,"enabled":false,"changed_at":1792756800}
```

Schema 4 extends activity with `first_started_at` (integer UTC seconds or `null`):

```json
{"schema":4,"kind":"activity","installation":"<64 lowercase hex>","revision":0,"version":"0.3.0","country":null,"first_started_at":1792756800}
```

Status continues using the same minimal schema 3 shape. Older activity remains
accepted. The new client records first desktop start locally before the five-minute
warmup, only for a new, enabled identity. Existing identities keep unknown (`null`)
instead of being relabelled as new installs. This is not a package-install date;
file timestamps and package-manager logs are not used. First-run opt-out creates
no startup metadata. Already-recorded startup time survives later opt-out/re-enable,
but is never included in a status request.

The first schema 3 shape is activity; the second is the minimal choice update (no country or
version). Revisions are safe nonnegative integers; status requires revision >0.
Action time is integer UTC epoch seconds in years 1970–9999. Skew is accepted
as informational data; receipt and retention always use server time. Status
acknowledgements echo the retained `revision` and `enabled`; the client clears
only a matching latest queue. Activity acknowledges a server UTC `day`.

A higher status revision replaces current status. Equal revisions are no-ops
for matching choices; conflicting choices fail. Lower revisions cannot replace
newer ones. Activity at an existing row must have its exact revision and enabled
status; legacy activity cannot change status. A newly seen activity token can
start an enabled row, including after retention has removed the old identity.
No permanent tombstone is retained beyond the disclosed window.

The worker has no healthy idle polling tick. It sleeps until a state-file
notification, a local explicit choice, or its next real deadline: startup warmup,
UTC day rollover, six-hour retry or pending-choice expiry. A status-only/disabled
worker with no pending choice has no timer. File notifications observe atomic
replacement across processes and rebind after state-directory replacement.
Local notifications coalesce and remain pending if network work is in progress.
During a network operation the existing 50 ms choice checks remain in force.

One-shot Linux realtime timers notice elapsed deadlines after resume and cancel
on wall-clock steps so the worker can recompute its UTC/retry state. They do not
wake the computer from suspend. The five-minute monotonic startup warmup remains.
Only unavailable file watching/timers or due work unable to acquire/save state
uses a one-minute error backoff; this is not routine reporting or a new network
retry policy. Corrupt state still fails closed. Report contents/frequency and the
seven-day/28-attempt status limit are unchanged.

The state file migrates old client state while preserving the random token.
New fields intentionally make older strict state readers reject it, preventing
a downgrade from silently enabling reporting. In particular, 0.2.5 cannot read
the added startup field; downgrading stops reporting rather than dropping choice state. Never delete/strip those fields
to make an older client report again. Broken state fails closed.

## Schema and migration

`Installations` retains its original A:D columns unchanged during `setup()`:

1. Installation hash (SHA-256 of the random token)
2. Last active (UTC), blank for status-only installations
3. Version, blank for status-only installations
4. First reported country, blank for status-only installations
5. Reporting status: `enabled` or `opted_out`
6. Opted out at (UTC): latest off action, preserved on re-enable until aged out
7. Status received at (UTC): collector receipt of the latest distinct choice
8. Preference revision (internal ordering/idempotency)
9. Choice at (UTC): latest action time, including re-enable
10. First activity received since tracking (server UTC)
11. First desktop start (client UTC; blank if unknown)
12. First version received since tracking
13. Reported active days in the last 90 UTC days
14. Internal sorted, distinct UTC day serials (at most 90)
15. Last activity receipt (server UTC)
16. Observed version changes: bounded JSON pairs of UTC day and version (internal)

J:O started empty for legacy rows; the new P column also starts empty. The next accepted activity initializes first
observation/version, not an inferred historical install time. The active-day count
starts with reports actually received after this collector upgrade; missed/offline
activity is not backfilled. Repeats count once per UTC day. Maintenance ages the
90-day day-set and count together. Retained duplicate rows merge day sets without
inflating counts. Client startup time is informational and never drives retention.
No operating system, hardware or widget-type information is added.

Migration preflights **values and formulas** throughout E:P before any workbook
write. A blank status header authorizes only a genuinely empty column, never an
occupied body cell or a formula whose evaluated result looks empty/numeric.
Recognized headers must contain coherent status/revision/timestamp records;
formulas, orphan status cells and unrelated/incomplete non-default records are
refused without changing any cells, dimensions or triggers. Ingestion and
maintenance apply the same validation and require the migrated status headers.
A historical formula evaluating to 42 cannot become a preference revision.

A clean legacy extension can be initialized. Interrupted header writes and
empty/partly initialized default rows (`enabled`, revision 0, no choice times)
can be completed safely. Valid existing status rows—including off, positive
revisions and Date-valued timestamps—are preserved exactly during setup, never
replaced by defaults. Incomplete off/positive-revision records need owner
recovery from authoritative data; setup will not invent missing consent state.

Migration adds missing columns, initializes only the required default cells and
is idempotent. It does not prune, sort, remove original rows or change original
A:D values/formulas, spreadsheet timezone, sharing, Script Properties, formatting
or historical aggregates. Normal ingestion/maintenance then deduplicates and
prunes expired records. No automatic backup duplicates identifiable data.

`Countries` and a dedicated `Reporting` tab show current snapshots excluding
opted-out rows. The latter labels the retained 90-day opt-out count explicitly.
`Reporting!A8` holds the literal ownership marker
`halcyon:reporting:schema3:v1`; A1:B6 is the reserved current snapshot. A new or
entirely empty tab may be claimed. An unmarked nonempty tab is **never adopted
or overwritten**, even if it resembles a snapshot from the earlier local
implementation. Formulas returning an empty result still make a tab occupied.
Owned snapshots must retain their expected labels/count shapes and contain no
formulas in the reserved cells; custom edits there cause refusal. Other cells
on an owned tab are left untouched. The marker is written before migration so
an interruption with an empty/partial snapshot can be retried safely.

Setup, ingestion and maintenance check the destination and status columns before
any writes. A conflict leaves the workbook unchanged; ingestion returns
`ok:false`, and setup/maintenance throw a diagnostic error. This intentionally
pauses collector writes until the owner resolves the collision, rather than
sacrificing custom data or silently ignoring an opt-out. The local client choice
and its bounded retry behavior are unchanged.

`setup()` also updates only the three exact documented legacy `COUNTIFS`
current-count formulas wherever they occur in `Summary`, adding
`Installations!E2:E20001,"enabled"`. Custom formulas and historical values remain
untouched. The maintainer must inspect any customized current-population
formulas and add that same criterion; do not reinterpret historical aggregates.
`Summary!B9` remains the existing configuration status cell.

## Live deployment status

On 2026-10-26 the existing web-app deployment was upgraded to immutable version
5, with schema 4 at the unchanged endpoint. The private workbook was backed up,
the additive migration rehearsed on a private copy, and the setup writes applied
through the Sheets API because remote Apps Script execution returned 403.
The existing daily maintenance trigger was retained; its successful execution
was visible in the project activity. The customized current-count formulas were
updated to exclude opted-out installations. Existing records were preserved.

Live verification covered status-only opt-out, activity suppression, explicit
re-enable, detailed activity, duplicate activity/status, stale revisions and
schema-2 compatibility. Synthetic rows were removed and aggregates checked.
The accepted source avoids Apps Script's unsupported logical assignment syntax
and accepts Sheets' native date coercion for the owned snapshot date. Local
collector coverage is 38 passing tests. Restricted migration evidence remains
outside the repository; release evidence contains only sanitized results.

## Reporting dashboard deployment

On 2026-10-28 (local date; 2026-10-27 UTC), immutable collector version 6
replaced version 5 at the existing endpoint. The update adds only column P and
four generated views; the request schema remains 4. All 20 existing A:O records
were verified unchanged. Eight synthetic protocol cases and a post-format refresh
passed live; synthetic records were then removed and aggregates verified.

Remote Apps Script function execution still returns 403. The preflighted additive
header was applied through the Sheets API, followed by the immutable deployment
update. The existing trigger, configuration and manifest were preserved.
Local validation: 49 collector tests and two release-endpoint tests passed.
Native Sheet formats, charts, dates and percentages were read back through the
API; the inline aggregate presentation was visually checked at desktop and
mobile widths. A native Google Sheets browser render was not available.

## Maintainer deployment procedure

1. Keep publication on hold. Record the existing immutable Apps Script version,
   deployment ID/URL, Script Properties, trigger list, private sharing and Sheet
   timezone. Export a restricted backup of the private workbook for rollback;
   keep it outside the repository and expire it under the existing retention
   policy. Do not create public links or include tokens in logs.
2. First exercise `setup()`, reports and maintenance on a separate **private test
   Sheet**, including duplicate rows and old/new status dates. Use synthetic
   tokens only. Verify original A:D values and all unrelated workbook settings
   survive migration, repeated setup and a simulated interruption. Include an
   occupied custom `Reporting` tab and formulas/literals below blank E:I headers;
   verify refusal before **any** workbook write, plus a clean first revision-1
   opt-out and a mixed valid-status/partially initialized migration.
3. Upload `collector.gs` and the unchanged `appsscript.json` to the existing
   Apps Script project. Keep `SPREADSHEET_ID` pointing to the intended private
   workbook. Run `setup()` as owner before switching the live deployment.
   Before running setup, audit `Reporting` and E:I values **and formulas**. If a
   collision exists, preserve it in the restricted backup and have the owner
   relocate the unrelated content intact, verifying dependent formulas/links.
   Do not clear custom cells or paste the ownership marker to bypass preflight.
   Keep valid preference rows/revisions in place. Even a known old unmarked
   Reporting snapshot must be retained/relocated before a new empty destination
   can be claimed; setup does not guess ownership. If the former unsafe migration
   was ever run, a literal revision such as 42 may already have replaced a
   formula: reconcile it from authoritative records/backup, never lower real
   consent revisions by guesswork. Then rerun setup and inspect Summary formula
   coverage and Reporting/Countries snapshots. Migration preserves timezone;
   verify the intended UTC date conventions explicitly.
4. Create a new immutable Apps Script version and update the existing web-app
   deployment (owner execution, Anyone access). Keep the endpoint URL stable.
   Existing schema 1/2 clients remain supported throughout. Do **not** release
   the schema 3 client against a schema 1/2 collector: its choices would fail.
5. After approval for live verification, run the health gate:
   `python3 scripts/usage/release-check.py --check-live`. It now requires schema 4.
   A GET alone does not prove writes: use synthetic status-only, enabled activity,
   opt-out, replay, stale activity and explicit re-enable requests; check one
   hashed row, unchanged last-active on status, identical receipt after replay,
   and current counts. Remove only those synthetic rows afterward.
6. Verify the daily `maintenance` trigger, retained-row cap (20,000), retention
   and privacy of the workbook. Confirm status expiry uses receipt, not action
   time. Verify no opted-out row contributes to current Summary/Countries counts.
7. Only the parent/maintainer can approve publication after all other release
   gates. Installing this code or passing local tests does not lift the hold.

## Rollback

Before accepting schema 3 traffic, the old immutable collector version and the
restricted pre-migration backup can restore the former deployment. Prefer to
leave the additive columns; they do not require erasing A:D.

**After any opt-out has been accepted, do not roll back to the old collector or
restore an old workbook over current status:** either would discard suppression.
Keep this schema 3 collector in place while rolling back desktop code (the old
client fails closed on upgraded state), or temporarily disable the web-app
endpoint and maintenance trigger while preparing a forward fix. Retain the
current private Sheet E:I data and newer revisions. Merge any backup recovery
by installation hash and highest revision, favoring off on equal-revision
conflicts; verify in the private test Sheet before restoring service. Never
remove off state or rotate tokens to work around compatibility. During downtime
the latest client choice retries for only seven days, so count delivery may be
incomplete; local opt-out remains effective. No rollback here is executed live.

## Local verification

```sh
cargo +1.93.0 test --locked --offline --target-dir /tmp/halcyon-app-target usage:: -- --test-threads=1
cargo +1.93.0 test --locked --offline --target-dir /tmp/halcyon-app-target settings::privacy_tests:: -- --test-threads=1
node --test scripts/usage/collector*.test.cjs
python3 -m unittest discover -s scripts/usage -p 'test_*.py'
```

Rust tests use loopback HTTP only and isolated state. Node tests use fake Sheets,
locks, properties and trigger services. The fixture stores evaluated values and
formulas separately and can interrupt range writes after individual cells;
these model recovery boundaries, not a guarantee of live Sheets atomicity.
Capture/debug network guards are retained.
The public endpoint is not authenticated: fabricated reports and copied/deleted
local state can affect estimates. The token is pseudonymous, not an account or
person count; a shared embedded secret would not authenticate installations.
See [Privacy](../../docs/PRIVACY.md) for the user-facing disclosure and retention.


## Readable activity and version history

The 2026-10-28 reporting upgrade uses existing schema 1–4 requests; no client
update, new payload fields, feature telemetry or additional requests are needed.

- **Dashboard:** active today / 7 / 30 days, first seen and returning this week,
  repeat use, average reported days, observed return in weeks 2 and 5, opt-outs
  and history coverage. Return rates show “Not enough history” until their full
  observation window has elapsed. Denominators include only currently enabled,
  retained installations with a known first observation within 90 days.
- **Versions:** latest observed version among active installations, seven/30-day
  counts and 30-day share. The 20 largest categories are shown; the rest are
  combined. This is current adoption, not historical version market share.
- **Daily activity:** up to 90 UTC days of reported activity, first seen and
  returning installations. Blank days precede known tracking. Counts are rebuilt
  from currently enabled retained records, so opt-out and expiry can reduce
  historical counts. This is not an immutable lifetime activity log.
- **Installation history:** readable installation references, reporting choice,
  latest version, first seen, last activity, 7/30/90-day counts, optional client
  first-start date, first observed version and observed version changes. Missing
  history stays unknown; first seen is never labelled as a package install date.

Column P stores only actual accepted version observations from this upgrade
onward. The last observed version wins within a UTC day. Unchanged daily reports
do not add entries; version observations age out after 90 days, with at most 90
entries per retained installation. The first observed version/date already stored
in J:L remain separate; they do not imply a complete upgrade timeline. Suppressed
activity and preference updates never add version observations.

The four generated tabs use the ownership marker `halcyon:analytics:v1` in A1,
a dated explanation in row 2 and fixed headers in row 4. Their first 3/4/4/11
columns respectively are collector-owned; add notes outside those columns.
Unmarked nonempty destinations, edited headers and formulas in generated ranges
are refused before any source write. Refreshes preserve formatting and unrelated
columns, compare native date coercion correctly, and write only changed rows in
batches of at most 256. Existing Summary, Countries and Reporting stay available.

The private workbook presents these views with formatted dates, percentages,
frozen headers, and linked charts. No reporting preference or raw source record
is changed by dashboard formatting.

## Four-tab reading layout

The live workbook now opens with **Overview, Activity, Versions, History**.
The presentation is an additive reading layer; collector version 6, its endpoint,
wire schema, consent state and source records are unchanged.

- **Overview** shows active installations, repeat reporting, daily/version charts,
  last activity receipt, and expandable countries and coverage definitions.
- **Activity** has a 7/30/90-day selector. It omits untracked dates rather than
  inventing zeroes. Return metrics remain available in a collapsed detail group.
- **Versions** has a 7/30-day selector, counts, shares and a native chart.
- **History** has a five-column filtered table and a record selector. Expand its
  detail row group for first-seen/start dates and observed versions. Older records
  without day history show `Unknown`, not a misleading zero. An expired selection
  or one outside the current filter is identified explicitly.

Collector-owned `Dashboard`, `Daily activity`, `Installation history`, `Summary`,
`Countries`, `Reporting` and `Installations` are hidden, not renamed or deleted.
`Versions!A:D` remains collector-owned and hidden; its reader controls/formulas
live in E:L. Other reader tabs have `halcyon:presentation:v1` in hidden Z1.
The data source names must stay unchanged. Use **View → Hidden sheets** for
maintenance. Hiding is a navigation choice, not an access-control boundary.

`presentation.cjs` builds the one-time native Sheets batch from freshly inspected
metadata and cells. It refuses existing reader tabs, occupied IDs and nonempty
Versions presentation cells. Its JSON input is `{metadata, inspected}` (Sheets
metadata and a bounded CellData read of `Versions!E1:Z1000`); its output contains
separate `stage` and `publish` requests. It performs no network calls. Apply
`stage`, validate formulas and controls, then apply `publish`. For later edits,
read and patch exact owned cells rather than rerunning the installer. The History
source grid is extended to the existing 20,000-record cap without changing values.

Native formulas read current collector results, so reports and daily maintenance
refresh the visible views without an Apps Script redeployment. Column ownership
is regression-tested for setup, activity, opt-out and maintenance. The live
rollout checked period/status controls, unknown history, stale selection and
selection/filter consistency. All 21 original records were unchanged; sharing
remained private. Verification used live formula/format/chart API readback;
no native browser screenshot was available.
