# Design decisions

## 2026-11-01 — one morphing activity surface

The user rejected duplicated content between the bar label and expanded card.
The current concept uses one visual title/icon/status that moves into the expanded
layout. Remove the attached-card stem, repeated headers and permanent tabs; place
other activities behind one switcher. Keep neighbors in place and retarget motion
smoothly. Native applet/popup composition must prove this visual contract before
implementation advances. This supersedes the attached-card mockup.

## 2026-11-01 — Activity island belongs in the status bar

The user rejected the floating desktop-card concept. FB31 now lives in the native
status bar and expands from that anchor on hover or relevant activity. Keep the
panel height fixed, controls stable and automatic reveals brief and non-focusing.
[The revised design](ACTIVITY-ISLAND.md) supersedes widget-only placement and
no-auto-expansion assumptions. Prove the native panel slot, popup and input behavior
first; stock/Islands support is a capability question, not a demonstrated guarantee.

## 2026-11-01 — accepted next-work scope

The user accepted research ideas 1–4 and 6–7: Activity island, Wallpaper layer
editor, Weather-connected scenes, Sun and Moon widget, Custom data tiles and
Desktop companion. Idea 5, terminal/editor theme syncing, is excluded.
[NEXT-WORK.md](NEXT-WORK.md) records the proposed implementation order and
boundaries. These are planned tasks, with no release number assigned; each needs
review and validation, including native visual review for visual changes.

## Settled direction

| ID | Decision | Reason |
| --- | --- | --- |
| D01 | Design supporting contracts first, prove a small complete path, then expand the UI | Avoid both a cosmetic-only redesign and an oversized framework |
| D02 | Extend Rust/libcosmic and current native adapters/store | Preserve existing rendering, integration, recovery and shipped behavior |
| D03 | Distinguish design, local data bindings, applied state and observations | Shared looks must not carry accounts/content or confuse saved intent with runtime health |
| D04 | One style model with explicit inheritance and overrides | Very different desktops should share components rather than independent preset code paths |
| D05 | Preserve ordinary direct settings and scoped setup drafts | The new editor must not reintroduce a global unsaved basket |
| D06 | Local preview/share output cannot run live actions | Safe browsing and public previews must not operate the desktop or leak personal content |
| D07 | Keep Islands optional and capability-specific | Native panel orientation does not imply companion support or current health |
| D08 | 0.5 is a working target; no version bump in this planning task | Choose release scope from validated useful increments |
| D16 | For 0.5 sun-based schedules, follow the Weather widget's city when enabled; offer a manual city when it is off and calculate event times locally | Reuse the configured location, avoid duplicate setup and keep saved-location scheduling independent of weather refreshes; [FB21 plan](FB21-PLAN.md) |

D01 and D08 reflect the user's 2026-10-26 request. D02–D07 preserve existing contracts or the accepted concept direction. D16 records the user's 2026-10-27 location-source decision for the 0.5 plan.

## Reviewed foundation decisions

| ID | Decision | Evidence and remaining implementation gate |
| --- | --- | --- |
| D09 | Keep Config authoritative for the first slice. Later migrate the widget domain once to a versioned store-owned record; freeze old widget fields and detect old writers, without a writable mirror | [Contracts and downgrade limits](CONTRACTS.md), [examples](examples/contracts.json); migration, receipts and recovery must pass FB16 |
| D10 | Resolve recipe → setup → moment shared → instance → moment-instance → accessibility → capability fallback; reset names the next actual source | [Editing cases](CONTRACTS.md); resolver tests in FB05, actual Moments/schedule behavior in FB15 |
| D11 | Start with installed families and discrete styles; verified stable-width faces or measured digit cells. Do not expose arbitrary axes/feature controls unsupported by pinned Iced | [36 native paragraph cases and capability results](CAPABILITIES.md); full Type wall acceptance in FB10 |
| D12 | Freeze revision-1 finite limits and current baseline; distinguish draw/readback proxy from real presentation latency | [Budgets](BUDGETS.md), [measured evidence](evidence/baseline-20261026.json); actual presentation, maximum-load and memory-cycle gates remain FB09/FB20 |

## Open decisions

| ID | Question and proposed direction | Closure task |
| --- | --- | --- |
| D13 | Ridge fit/scale alignment and layer input: prepared asset pairs, known transforms, plain-clock fallback | FB14 |
| D14 | Portable asset transport and lifecycle: stage immutable assets, commit references with recoverable ownership, preserve Undo references | FB18 |
| D15 | FB15 fixed-time scheduling is implemented. FB21 now specifies singleton Weather/manual location separation, city-local occurrence dates, optional-zone migration and commit-time location fencing. Event definitions, calculation method and COSMIC automatic-mode interaction still need implementation review | FB21 |

## Deferred by design

- Automatic Focus/meeting rearrangement: needs opt-in, active-layer editing, ownership and reliable restoration.
- Timed “try on desktop”: current Undo alone does not provide crash-safe timed revert.
- Arbitrary executable recipes, third-party code plugins and a new desktop shell.
- Automatic photo segmentation, native compositor blur and lock-screen replacement.
- Hosted community accounts, social features and automatic remix updates before local portable sharing works.

These ideas are retained in the reference proposals and concept map. Deferral is sequencing, not an assertion that the idea is impossible.

## Change record

2026-10-26: consolidated twelve visual concepts and the Fable review. Replaced UI-first staging with contract review, baseline measurement and a narrow integration proof. Original proposals are archived for traceability; [tracker.json](tracker.json) is the current status source.

2026-10-26, foundation review: recorded D09–D12. Source inspection showed old Config readers ignore unknown fields; downgrade protection must account for that. Native paragraph and stock vertical panel probes support the initial scope. Measured current rendering and idle behavior and set finite limits; future features and full performance qualification remain unimplemented.

2026-10-26, FB05: implemented typed style resolution and the first Clock choices through current preferences and transactions. New Clock fields require profile version 9; legacy captures keep their previous version until those choices are used. The analog face keeps a native text time label and introduces no redraw timer. Tidal/Paper remain fixtures until the preset milestone. Full measurement/provider integration and later migration are unchanged gates.

2026-10-26, FB09: the complete local foundation passes its reviewed resource and
behavior gates. Native input-to-visible timing is explicitly a nested-compositor
measurement; physical scanout/device qualification remains FB20. Corrected the
maximum-workload wording: there are nine fixed kinds, but Glance replaces three,
so the maximum standalone view contains eight. No visibility or persistence
semantics were changed to make the test possible. The user requested sequential
implementation with review and fixes before moving to the next stage.

2026-10-27, FB10: new setup IDs preserve the original six records and wizard
order. Font selection uses twelve native specimens per page. Compiled gallery
pixels have an immutable 16 MiB cache prepared outside views, sharing image-worker
admission. Native preset reviews, explicit fallbacks and guarded Apply/Undo are
locally qualified; no hosted assets or font downloads were added.

## 2026-10-27 — local canvas transaction

The first canvas edits existing widget geometry, Clock presentation and shared
fonts. Config stays authoritative; one guarded transaction combines the fields.
Content/account editors retain their own actions. Private draft recovery stores
geometry and style together. Per-instance/Moment editing waits for its model.
Native controls describe current Clock settings and inherited defaults without
inventing setup provenance that the legacy Config does not retain.

## 2026-10-27 — Mix board composition contract

Picture supplies the shared wallpaper, without physical display assignments.
Style supplies appearance, fonts, widget presentation and bar color/opacity/corners.
Layout supplies widget visibility/order/geometry and bar placement, spacing,
size, applets and Islands arrangement. Clock time/date preferences, readability,
reduced motion, provider content and window/workspace behavior remain local.
Three explicit, frozen source choices produce one ordinary ProfilePlan. Saving
creates an independent library record; it does not apply the candidate.

Profile format 10 adds optional bounded composition provenance: three source
names, kinds, IDs and fingerprints of their portable component projections.
Local library IDs are stripped on export. Metadata is informational, never an
instruction to load or execute another record. Formats 1–9 remain readable and
records without provenance retain their existing minimum write version. This
extends existing profile transport; Config remains authoritative until FB16.

## 2026-10-27 — local sharing and Spine

Share images use the native renderer with a sanitized profile, a public reference
canvas and fixed sample content. They never read the applied Config, runtime
geometry, accounts, notes, media artwork or provider bindings. Personal and
unresolved system wallpaper images are omitted; known bundled backgrounds and
colors/gradients are eligible. Existing profile-file export stays separate.

Rendering runs in a bounded child, sharing the two media-worker slots and owned
cancellation/reaping policy. Output is a 1280×720 PNG. Saved images are local files
chosen through the existing file dialog; no upload or hosting is introduced.

Spine uses a standard vertical panel. An enabled Islands layout is not silently
disabled or represented as qualified vertical Islands. Its standard-panel
requirement must be visible and checked by the shared plan before Apply.


FB13 review retained the separate full-record sample export. Its child uses the
native software headless backend for deterministic bounded image generation;
interactive Settings keeps its GPU renderer. Static full-size public backgrounds
are compiled into this child path, avoiding mutable file resolution. Spine's
standard-panel constraint is checked by the shared planner, including a guard
against concurrent Islands enablement. The final visual pass added rail clearance
and bounded sample-line counts. See FB13 evidence for artifact-specific checks.
