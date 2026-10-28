# Halcyon future builder

Design the foundations for expressive desktops, reusable widgets and visual editing before building the full editor. **0.5 is the next working target, not a release-date commitment.** The published application is version 0.4.1.

Direction accepted on 2026-10-26: shared styling, varied desktop compositions, visual editing, personal widgets and portable setups. FB05–FB19 are implemented and locally validated: shared styling, Clock choices, shared Clock/Weather presentation and measurement, keyed provider demand, and layout/input integration. The original feature implementations are complete in their documented bounded scope. See the [0.4.0 package verification](../releases/0.4.0-acceptance.md) for release evidence. FB21 adds locally implemented sun-based appearance schedules for 0.5, following the enabled Weather city or a separate chosen city. Adaptive Tidal, Paper and Moss looks and coordinated package status are implemented locally. [The delivery tracker](DELIVERY-050.md) records staged checks; 0.5 is not published.

## Start here

| File | Purpose |
| --- | --- |
| [Current delivery](DELIVERY-050.md) | 0.4.1 publication and staged local 0.5 work |
| [Adaptive looks](ADAPTIVE-LOOKS.md) | Tidal, Paper and Moss daily appearance cycles |
| [One Halcyon package](BUNDLED-ISLANDS.md) | Included Islands, activation and legacy migration |
| [Coordinated updates](COORDINATED-UPDATES.md) | Matching app/Islands offers and one next action |
| [Calendar and system recipes](PERSONAL-WIDGETS-050.md) | Personal agendas and selected system readings |
| [0.5 sun-based schedules](FB21-PLAN.md) | Local sun events, city binding, appearance-only changes and validation |
| [Settings structural pass](STRUCTURAL-PASS.md) | Responsive native editors, compact help, navigation and validation |
| [Architecture](ARCHITECTURE.md) | Component boundaries, data flow, ownership and migration |
| [Reviewed contracts](CONTRACTS.md) | Legacy setup behavior, later instance migration, override and reset rules |
| [Native capabilities](CAPABILITIES.md) | What the pinned renderer and panel probes actually support |
| [Resource budgets](BUDGETS.md) | Finite limits, measured baseline and first-slice acceptance targets |
| [Baseline evidence](evidence/baseline-20261026.json) | Raw measurements, exact artifacts, font cases and focused checks |
| [First implementation](FB05.md) | Shared styling, Clock choices, compatibility and scoped acceptance |
| [Shared components](FB06.md) | Clock/Weather measurement, actual scene renderers and preview isolation |
| [Layout and input](FB08.md) | Native hit regions, display epochs and guarded edit integration |
| [Provider demand](FB07.md) | Shared requests, cancellation, typed observations and accepted cache writes |
| [Local package acceptance](FB20.md) | Exact artifacts, isolated lifecycle checks and remaining shipping gates |
| [Discovery and remix](FB19.md) | Offline catalog, independent copies, credits and explicit revision review |
| [Portable packages](FB18.md) | Bounded pictures, credits, isolated workers and durable import |
| [Personal recipes](FB17.md) | Native Clock/Weather parts, immutable snapshots and guarded Apply |
| [Canonical widget design](FB16.md) | Explicit migration, stable IDs, old-writer recovery and downgrade export |
| [Moments](FB15.md) | Day/Night presentation layers and existing scheduled switching |
| [Prepared Ridge depth](FB14.md) | Prepared skyline masking, native fit/input/fallback and bounded rendering |
| [Sample images and Spine](FB13.md) | Public native-rendered PNG export and a guarded standard vertical panel |
| [Local Mix board](FB12.md) | Picture/style/layout composition, independent saved setups and provenance |
| [Canvas and inspector](FB11.md) | Local widget geometry/style drafts, recovery and one Apply/Undo |
| [Presets and typography](FB10.md) | Four portable looks, bounded font specimens and gallery images |
| [Complete foundation](FB09.md) | Cancellable image work, native performance, memory cycles and review |
| [Architecture overview](architecture.html) | Compact visual map of the six supporting systems |
| [Roadmap](ROADMAP.md) | Dependency order and gates for moving forward |
| [Tracker](tracker.json) | Canonical task IDs, status, dependencies and completion evidence |
| [Quality gates](QUALITY.md) | What must be demonstrated before a feature can ship |
| [Decisions](DECISIONS.md) | Settled constraints and unresolved technical choices |
| [Concept gallery](concepts/index.html) | All twelve imagined screenshots, with captions |
| [Concept map](CONCEPTS.md) | Every concept and Fable proposal mapped to supporting systems |
| [References](references/README.md) | Inspiration, generation prompts and historical proposals |

## Build the smallest complete foundation

Extend the existing Rust/libcosmic application, provider code, layout logic and guarded transaction store. These are module boundaries inside the application, not a set of new services or a replacement desktop shell.

The first implementation proof is one vertical slice: clock typography/faces and a weather component, expressed through shared style and layout rules, rendered in both the desktop and a side-effect-free preview. It must preserve existing preferences, support Undo and avoid duplicate provider work. A minimal native control surface is enough to exercise it.

Use Tidal and Paper to test contrasting styles and Spine to challenge layout assumptions. Only after that proof should we expand the preset collection and build richer editors. Ridge depth, Moments, custom recipes and sharing each have their own gates.

## Tracking rules

`tracker.json` owns status. The roadmap owns sequencing; architecture and decisions own contracts. Generated images guide art direction, not exact controls, capabilities, dates or typography. Historical JSON proposals are reference material, not competing backlogs.

Task states are `planned`, `design_review`, `in_progress`, `blocked`, `validated`, and `deferred`. Mark implementation validated only with evidence for its exit criteria. Keep unknown capabilities visible. Add decision changes to `DECISIONS.md` before changing contracts.

Foundation contract review, the current native baseline and initial resource budgets are recorded. All twelve original concept PNGs remain bundled with prompts and SHA256 hashes. FB05 adds the shared style resolver and native digital/analog Clock choices, with scoped Undo and versioned setup transport. FB06 shares Clock/Weather presentation, measured sizes and native scene rendering, with inactive local preview actions. FB07 adds keyed demand, cancellation, fenced delivery and shared observations. FB08 connects logical card frames, input regions and gesture cancellation to the existing placement/store owners. FB09 qualifies the complete local foundation, including native interaction timing, memory cycles, image cancellation and the full standalone desktop. FB10 adds four portable presets, bounded native font specimens and an asynchronous gallery cache. FB11 adds the native canvas and inspector, local geometry/style drafts, combined Apply/Undo and private recovery. FB12 adds local picture/style/layout composition, saved provenance and one existing Apply/Undo. FB13 adds public native-rendered image export and Spine. FB14 adds prepared Ridge depth with bounded rendering and plain fallbacks. FB15 adds Day/Night widget styles, scoped Clock edits and existing scheduled switching. FB16 adds explicit canonical widget migration and old-writer recovery. FB17 adds bounded personal Clock/Weather recipes. FB18 adds portable packages with credits and retained immutable pictures. FB19 adds offline discovery, independent remixes and retained source revisions. FB20 validates the local development package, source archive and extracted-artifact flows. Physical hardware and live release installation remain open shipping gates.
