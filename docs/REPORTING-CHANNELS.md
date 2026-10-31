# Reporting schema 5

The collector accepts schemas 1–5. The next, unreleased client adds package
format, update channel and installed product version to its existing daily
activity request. Preference messages keep their schema-3 minimal payload.
See [Privacy](PRIVACY.md) for consent, timing and retention.

The original raw columns A:P are unchanged. Q stores at most 90 daily package
observations as one bounded JSON value. Each entry binds its day, package,
channel, installed version, running version and receipt time. A partially
written newer receipt cannot inherit an older channel. Missing observations
remain unknown. Schema-4 clients continue to report normally.

Four hidden generated tables join the existing analytics: Installation channels,
Channel versions, Channel activity and Version changes. Their ownership marker
and collision checks match the existing collector. Current channel is never
projected onto earlier days. An observed version change is not evidence of a
successful package update. Counts exclude opted-out and expired records.

The reader retains Overview, Activity, Versions and History. Package/channel
filters apply to adoption and daily observations. Return rates explicitly use
their full eligible population. Installation details distinguish first report,
first desktop start, last activity, installed version and running version.

`scripts/usage/presentation-v2.cjs` updates only owned reader ranges and their
known charts. It preserves raw tables, existing names and sharing. Validate the
native formulas and dropdowns after applying it. Exported Excel previews do not
implement every Google Sheets formula; use the native calculated values for a
render-only snapshot, never replace live formulas with that snapshot.

Deployment order: back up the private workbook and script; run the source-column
and analytics collision preflight; append Q's header; update the existing Apps
Script deployment; verify schema-5 health; inspect the generated views. Do not
fabricate activity to test the live dashboard. Apps Script execution-API access
is independent of source/deployment access. If `setup()` is forbidden, the
preflighted additive Sheets migration can initialize the new column/views while
preserving existing maintenance triggers and all raw records.
