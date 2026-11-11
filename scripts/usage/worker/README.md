# Halcyon reports (Cloudflare Worker + D1)

The activity collector and the public statistics page, live at
<https://halcyon-reports.profilaz3.workers.dev/>.

| Path | What it does |
| --- | --- |
| `POST /v1/report` | Accepts schema 1 to 7 reports (schema 6 adds the feature snapshot, schema 7 the use, problem, setup and system details). Same validation, ordering rules and acknowledgements as the former Apps Script collector. Always HTTP 200; `{"ok":false}` on rejection. |
| `GET /v1/report` | Health: `{"service":"halcyon-activity","schema":7}`. |
| `GET /api/summary.json` | Public aggregates, rebuilt hourly, cached for 5 minutes. |
| `GET /` | The dashboard (`public/`), plain HTML, CSS and JS with a strict CSP. |

Halcyon 0.6.1 and later report here. Halcyon 0.2.0 to 0.6.0 still report to
the Google Sheet collector in [`..`](../README.md); those reports are not
forwarded, and the dashboard counts 0.6.1 and later only (`DIRECT_SINCE` in
`src/stats.js`). Changing that value rebuilds every frozen daily total from
retained activity on the next hourly run.

## Data

`migrations/0001_initial.sql` defines five tables; `0002_features.sql` adds the schema 6 feature snapshot to `activity` and `0003_details.sql` the schema 7 details:

- `installations`: one row per SHA-256 hashed token: reporting choice, first-use facts, latest activity.
- `activity`: at most one row per installation and UTC day (version, package format, update channel, feature snapshot and schema 7 details as canonical JSON).
- `daily`: anonymous totals per completed day, install method, metric and version, plus per-feature totals. Kept indefinitely.
- `releases`, `meta`: GitHub release dates, the rollup watermark and the published summary.

Retention matches [PRIVACY.md](../../../docs/PRIVACY.md): activity older than 90
days is deleted, opted-out rows expire 90 days after the status receipt,
enabled rows 90 days after their last activity or choice, and old choice
timestamps are cleared. Turning reporting off deletes that installation's
activity immediately. The hourly cron applies retention, freezes completed days
into `daily`, refreshes release dates once a day and rebuilds the summary.

Publishing rules (`src/stats.js`): no identifier leaves the database. Every
other count is published exactly, however small: countries, versions, install
methods, settings and weekly cohorts.

Cost: each hourly run reads every retained activity row once. At 1,000 daily
active installations that is about 2 million rows a day, within the free
plan's 5 million. Reports cost 2 to 4 queries each.

## Develop and deploy

```sh
node --test test/*.test.js            # uses node:sqlite, no dependencies
npx wrangler@4.158.0 d1 migrations apply halcyon-reports --remote
npx wrangler@4.158.0 deploy
```

Wrangler reads `CLOUDFLARE_API_KEY` and `CLOUDFLARE_EMAIL` (or an API token)
from the environment. Never log request bodies; Workers observability stays off.

## Importing the former Google Sheet

Export the `Installations` tab as CSV, then:

```sh
node tools/import-sheet.mjs Installations.csv --sql /tmp/import.sql           # dry run, counts only
node tools/import-sheet.mjs Installations.csv --sql /tmp/import.sql --apply
```

The import merges with live data: the higher preference revision wins, an
opt-out wins an equal revision, activity days are combined, and every write is
guarded by the row state that was read, so a report arriving mid-import is
never overwritten. It can be repeated safely. It then clears the rollup
watermark so the next hourly run rebuilds the daily totals for the whole
retained window. Delete the CSV and the generated SQL afterwards.
