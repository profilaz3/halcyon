# Next work — desktop interactions and scenes

Accepted 2026-11-01. Baseline: 0.5.6. Target version is unassigned.
FB31 has a [local first native version](ACTIVITY-ISLAND-IMPLEMENTATION.md); further
interaction policies and release qualification remain. FB33 has a
[local first version](WEATHER-SCENES.md), validated on the real desktop and
unreleased. The other four features are **planned**.
[tracker.json](tracker.json) owns task status.

The user accepted research ideas 1–4 and 6–7. Idea 5, synchronizing Halcyon
colors into terminals and editors, is excluded from this work.

## Proposed sequence

| Order | Task | Deliverable |
| --- | --- | --- |
| 1 | FB31 — Activity island | A status-bar activity that expands on hover or relevant events |
| 2 | FB33 — Weather-connected scenes | Existing wallpaper effects follow reported weather |
| 3 | FB34 — Sun and Moon widget | A horizon, dial or sky view of daylight and lunar phase |
| 4 | FB32 — Wallpaper layer editor | User-supplied cutouts and masks, with deliberate depth ordering |
| 5 | FB35 — Custom data tiles | Native widgets bound to a local JSON file or chosen endpoint |
| 6 | FB36 — Desktop companion | An optional character responding to selected desktop events |

This is a working order, not a requirement that every feature ship together.
Finish, review and fix each meaningful increment before proceeding. Visual work
needs native screenshots and interaction review before it is marked complete.

## FB31 — Activity island

[Expanded design and interactive concept](ACTIVITY-ISLAND.md) define the proposed
states, activity priority, placement, settings and implementation slices.

**Experience:** an activity lives in the status bar and expands downward on hover,
click/keyboard activation or important events. One capsule morphs between states;
its title/status never repeat in a separate header and card. It has Music, Focus and Calendar
controls, with brief automatic reveals that never steal focus or move windows.

**First delivery:** prove the native COSMIC panel slot and anchored popup first.
Then add shared providers/actions, stable selection, explicit priority, pending
activity cues, quiet policies and privacy in every state.

**Boundary:** this supersedes the standalone desktop-widget concept. Prefer stock
and Islands-styled panel support, subject to the capability probe. Do not cover or
silently replace existing clock/system applets. Keep the panel's reserved height
constant, preserve input ownership and avoid duplicate provider polling.

**Acceptance:** playback and timer actions work exactly once; Calendar respects
Busy privacy; Escape and outside-click close correctly; the expanded surface
stays inside its display. Check source removal, concurrent activity, hotplug,
relaunch, keyboard use and compact/wide native views. Hidden content adds no
duplicate polling or permanent animation loop.

## FB33 — Weather-connected scenes

**Status:** first version implemented and validated locally on 2026-11-08
([design and decisions](WEATHER-SCENES.md),
[evidence](evidence/fb33-weather-scenes-20261108.json)). It settles the mapping,
a stale cutoff of two refresh intervals (at most 3 h) and 1.5 s fades.
Intensity limits are now chosen with Weather strength: Subtle (25/50/100%),
Natural (40/70/100%, the default) or Strong (70/85/100%) of the person's own
setting. The status line says how strong, and Adjust tunes any effect while
Match the weather is on. A per-effect allow list and a timed manual override
remain out; turning Match the weather off is the override.

**Experience:** rain, snow, fog and cloud shadows follow the Weather widget's
city. Settings offer Follow weather, allowed effects, intensity limits and a
temporary manual override with a clear return to automatic control.

**First delivery:** an opt-in condition-to-effect mapping using the existing
weather observation and wallpaper renderer. Settle transition timing, stale-data
cutoff and manual-override duration before implementation. Show what observation
is driving the current effect.

**Boundary:** weather controls the selected effect; sun schedules retain their
appearance responsibilities. Neither moves widgets. Turning Weather off or
losing fresh data must have an explicit, visible fallback. No additional weather
requests solely for the effect, and no video playback.

**Acceptance:** deterministic weather fixtures cover changing cities, stale and
missing observations, rapid updates, overrides and resume. Preserve user intensity
limits, battery/lock pauses, reduced motion, setup persistence and Undo. Review
transitions on the actual desktop and measure the combined rendering cost.

## FB34 — Sun and Moon widget

**Experience:** a horizon arc with the sun's position, sunset countdown and
golden-hour window; a moon-phase view at night. Designs include a compact strip,
circular dial and larger sky illustration.

**First delivery:** reuse the existing city and solar calculations; implement
and validate lunar phase separately. Follow the enabled Weather city or use an
explicit manual city. Begin with the strip and dial, then review the larger sky
view as its own visual increment.

**Boundary:** this is a display widget, not a new scheduler. Do not imply that
moon phase also supplies moonrise, moonset or astronomical position. Any later
addition of those calculations needs its own accuracy checks.

**Acceptance:** known-date references, city changes, time zones, DST, polar day
and polar night, missing location and suspend/resume. Keep text readable at small
sizes; static scenes need no continuous redraw. Share the renderer between
desktop, builder and setup previews.

## FB32 — Wallpaper layer editor

**Experience:** place a clock behind a skyline, add a transparent illustration,
or arrange a foreground silhouette. An ordered layer list controls visibility,
opacity, placement and masking.

**First delivery:** generalize the prepared Ridge effect to imported transparent
cutouts and manually edited masks. Define supported layer roles and which widgets
can be masked before building the editor. Start with static composition; optional
subtle movement follows only after alignment and resource checks pass.

**Boundary:** automatic subject segmentation is not included. Layers belong to
the desktop and must not obscure application windows or swallow their input.
Use the existing bounded media workers, immutable assets and setup transport.

**Acceptance:** preview/runtime agreement for Fill/Fit, mixed scaling, portrait
outputs and display changes; deterministic ordering; Apply/Cancel/Undo and
recovery; bounded decoding and masks; missing-asset fallbacks; portable setup
round trips. Review edges and text legibility on real native captures.

## FB35 — Custom data tiles

**Experience:** choose a local JSON file or endpoint, map fields, then select
text, a progress bar, gauge or small graph. Initial templates demonstrate server
status, build results, download progress and a personal goal.

**First delivery:** a small typed field-mapping interface, explicit refresh
interval, unit/label controls and honest loading, stale and error states. Start
with local regular files, then add HTTP(S) sources as a separately reviewed step.

**Boundary:** no executable recipes or general code plugin system. Endpoint
credentials stay in the existing secret store. Saved and shared designs must not
export credentials, personal data, local paths or automatically activate someone
else's endpoint. Imported designs require local source binding.

**Acceptance:** bounded file/network reads, response sizes, parsing, history and
refresh frequency; malformed data, timeouts and cancellation; shared demand and
zero work when unused. Validate redirect/authentication handling, safe preview
samples, migration and export/import. Measure several simultaneously active tiles
before selecting a supported instance limit.

## FB36 — Desktop companion

**Experience:** an optional illustrated or pixel-art character sleeps during
Focus, reacts to music or shelters from rain. Users choose its location, size,
activity level and which reactions are enabled.

**First delivery:** one properly licensed character with a small set of states
and explicit event precedence. Use existing Music, Focus and weather observations.
Provide a quiet/static mode and predictable click-through behaviour.

**Boundary:** bounded sprite animation, not a simulation or background agent.
No video, microphone listening or cloud dependency. Weather reactions are enabled
only when a usable weather observation exists.

**Acceptance:** animation sleeps when hidden, locked, suspended or paused by
power policy; reduced motion works; frames and memory are bounded. Review event
transitions, input pass-through, display changes, position persistence and native
appearance over different wallpapers.

## Delivery checks

For each feature: review its state and ownership contract; deliver the smallest
usable slice; run focused regressions; inspect native visual states and keyboard
interaction; fix findings; then record evidence. Reuse the guarded store, provider
demand and rendering infrastructure. New abstractions need a concrete second use.

Keep preview content separate from live actions and private data. Preserve saved
setups, scoped Undo, recovery, display assignments and existing installation/update
behaviour. Measure idle and active CPU/GPU/memory on the delivered implementation;
do not turn a design target into an unmeasured performance claim. Exact DEB/RPM
release acceptance remains FB20 and is separate from feature completion.

## Research references

These projects informed the ideas; their behaviour, performance and compositor
support are not claims about Halcyon.

- [Caelestia](https://github.com/caelestia-dots/shell): expandable and morphing shell surfaces.
- [Noctalia desktop widgets](https://docs.noctalia.dev/noctalia/desktop/widgets/): positioned imagery and desktop composition.
- [Weathr](https://github.com/Veirt/weathr): scenes driven by reported weather.
- [Caelum](https://github.com/Juliowilly/caelum): location-aware sky displays.
- [Eww](https://elkowar.github.io/eww/): configurable widgets and data presentation.
- [Vitreus showcase](https://www.reddit.com/r/unixporn/comments/1x7vsm0/pleamarwmhyprland_vitreus_updates_dashboard_and/): interactive characters and weather-responsive pixel art.
