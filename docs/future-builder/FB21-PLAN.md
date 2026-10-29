# FB21 — Sun-based appearance schedules for 0.5

Status: **implemented locally**, 2026-10-27; release packaging remains pending. Tracks
[GitHub issue #14](https://github.com/profilaz3/halcyon/issues/14).
The released 0.4.1 supports fixed-time schedules and Day/Night widget Moments;
this extends those foundations. Implemented through the existing scheduler and guarded store; no 0.5 release has been published.

## Experience

Add sun events to **Setups → Schedules & display rules**: dawn, sunrise, solar
noon, evening golden hour and sunset. Each rule selects a saved appearance or an
existing Day/Night Moment. Show the next calculated switch time beside the rule.
Keep fixed-time schedules available, including a chosen night start time.

Use the **Weather city when Weather is enabled**, otherwise offer a manual city.
Show the location source and the next switch beside each rule. Hiding all desktop
widgets must not pause an otherwise enabled Weather binding or start a forecast.
Disabling Weather or clearing its city pauses rules that follow it, keeping the
current appearance until the user chooses a source.

## Location and occurrence contract

The 0.4.1 baseline has one `Config.location { name, latitude, longitude }`, no saved
time zone, and one Weather widget. The local 0.5 implementation adds an optional
IANA zone to that city and a separate manual schedule city. Daily overview can display that same Weather
section. Do not expose selection among multiple Weather instances in this slice.

- Follow the singleton Weather binding. With Daily overview enabled, use its
  `show_weather` choice; otherwise use the standalone Weather enabled choice.
  Ignore global widget hiding and display disconnection for this decision.
- Store a manual schedule city separately in local automation preferences.
  Give city-picker requests an explicit Weather or Schedule destination; a
  Schedule selection must never overwrite `Config.location`. Reuse bounded,
  cancellable search and its generation checks. Preserve onboarding's existing
  Weather-city behavior on Cancel.
- Add an optional IANA time-zone identifier to saved city metadata, with an
  absent default for existing locations. A solar rule needs a resolved city
  zone before it can be enabled. An old location without a zone requires city
  confirmation; never silently substitute the computer's zone or UTC. Check
  the city provider's actual response contract during implementation.
- Define each occurrence by **the date in the selected city's zone**, the event,
  rule revision and location revision. Calculate a UTC instant for execution.
  UTC storage alone does not determine which city's date is being scheduled.
  Format next-switch times in the computer's zone, identifying the city/zone
  when the dates or zones differ. Existing fixed-time rules keep their semantics.
- A location revision covers source, coordinates, time zone and relevant
  enabled state. Changing any of these invalidates persisted waiting events,
  queued calculations and queued writes. Revalidate that revision under the
  commit lock as well as at dispatch; a late job must never apply the old city.
- Once a city and zone are saved, calculation works offline. It must not depend
  on the forecast cache, forecast demand or a successful weather response.
  Coordinates, manual cities, zones and binding revisions remain local and
  are excluded from shared setups, preview exports and usage reporting.


## Implementation boundary

- Extend the existing scheduler, rule editor and guarded store. Reuse pause,
  manual-change protection, priority, Undo and recovery.
- Build an explicit appearance projection from the existing theme/style and
  visual-bar allowlists. A whole `WidgetStyle` profile domain is too broad: it
  includes content and Clock sizing. Allow only the reviewed wallpaper, color,
  typography, finish and bar-style keys. Keep positions, sizes, visibility,
  applets, widget content, display assignments and Islands enablement unchanged.
  Add coverage proving that non-appearance members cannot enter a solar plan.
- Existing Day/Night Moment targets retain their widget-only scope. Do not
  silently broaden the FB15 data model or require extra named Moments for the
  first sun-trigger implementation.
- Settle explicit dawn/golden-hour definitions and the solar calculation method
  during design review. Solar noon follows the sun at the chosen location.
  Resolve interaction with COSMIC automatic light/dark mode so two policies do
  not repeatedly overwrite each other.
- Recalculate on date, location or time-zone changes and resume. Reuse bounded
  wake scheduling, once-per-occurrence execution, latest missed-event handling
  and fencing of obsolete rules. No continuous polling or animated transitions.
- When a sun event does not occur, retain the current appearance, show that the
  event is unavailable and skip that occurrence. Fixed-time rules still work.
- Version new rule data explicitly. Existing schedules and saved setups must
  retain their behavior. Previewing an event never changes the desktop or runs
  the scheduler.

## Delivery and acceptance

1. Review the location binding, event definitions, appearance projection and
   migration contract against the existing scheduler and Moments implementation.
2. Implement local event calculations and scheduling; review and fix before UI
   integration. Check dates/seasons, hemispheres, polar missing events, DST,
   differing city/system time zones, clock changes, sleep and restart.
3. Add native rule controls, Weather source indication, manual-city fallback and
   next-switch status. Check standalone/overview Weather enabled/off, global
   hiding, disconnected displays, a missing zone and offline operation. Test
   independent Weather/manual cities and city changes both while waiting and
   between planning and commit. Multiple Weather instances remain future scope.
4. Verify manual overrides, pause/resume, coincident rules, Undo/recovery, old
   schedules, preview isolation and unchanged geometry/content. Review compact,
   wide, keyboard and hover/focus-help behavior; qualify the final 0.5 packages
   through the existing release checks.

Completion requires evidence for these cases. Planning does not mark FB21
implemented or validated.

## Local validation

Seventeen focused solar checks passed, including NOAA reference comparisons, city
weekdays and time zones, polar missing events, clock rollback, stale-city rejection
at commit, appearance-only projection and native compact/wide rule controls.
Native renders were inspected at 820 × 600 and 1280 × 800 in light and dark themes;
Save is visible without scrolling at the compact size. The complete automation
suite also covers fixed-time scheduling, manual precedence, pause and restart.

Dawn uses a −6° solar altitude; sunrise/sunset use −0.833°; evening golden hour
uses the descending +6° crossing. Solar noon is local solar transit. Positions use
the published NOAA/Meeus equations, bounded to 1900–2100. The UI identifies the
city zone and shows next-switch times in the computer’s zone. No event within the
bounded eight-day search is shown explicitly, with no invented replacement time.

Automatic changes retain the existing scheduled recovery journal and do not enter
manual Undo history. Imported picture identity and saved theme scope survive the
projection. Physical-device and final packaged acceptance are separate release
checks. Evidence is retained privately in `.task-050-delivery-20261027/solar/`.
