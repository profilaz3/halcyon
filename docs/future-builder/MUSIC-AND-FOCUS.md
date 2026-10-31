# Music and Focus widget designs

Local implementation for the next release. No app version or publication change.

## Scope

Extend the existing Widget builder and guarded recipe store with two widget kinds.

- Music: Sleeve, Player and Quiet starting designs. Track title, optional artwork,
  playback controls and progress. Missing artwork removes its area completely.
- Focus: Orbit, Type and Edge starting designs. Time and phase, optional caption,
  controls and session count. Reuse the existing persistent timer.
- Both: compact/wide layouts, inherited desktop appearance or Tidal/Paper/Carbon
  finishes, library Save, independent Apply, built-in reset and scoped Undo.
- Sample states are local and non-interactive. Player selection and timer lengths
  remain on their existing settings pages.

## Boundaries

Recipes contain presentation choices only. They carry no media URLs, credentials,
player identities, track positions, timer deadlines or completed-session history.
Existing MPRIS ownership/capability checks, artwork limits and timer state remain
in their original engines. No new providers or background polling.

The existing recipes retain their serialized fields and revision hashes. Tool
recipes use recipe version 3, applied design version 4 and setup format 17. Older
setup exports keep their original minimum version. Imports reject downgrades.

## Acceptance

- [x] Music layouts: compact, wide, missing art/player, paused and unavailable seeking.
- [x] Focus layouts: ready, running, paused, break and complete.
- [x] Native keyboard controls and bounds; dark/light Settings at compact/wide sizes.
- [x] Save/Apply/Undo/export preserve player preferences and the active timer file.
- [x] Recipe, design, profile, tool engine and placement regressions.
- [x] Visual inspection of native captures and corrective polish.

Evidence: `.task-music-focus-implementation-20261030/` (private local fixtures).

## Validation result

Implemented locally. **228 distinct selected regressions passed** across the
recipe, design, profile, media/timer, placement and layout suites. Two existing
profile integration entries remain ignored; they are not counted as passes.
The final focused run passed all nine Music/Focus regressions.

Native widget-tree captures cover all six designs at compact and wide sizes,
all sample states, dark/light Settings at 820×600 and 1280×800, live capability
controls and minimum widget sizes. Representative captures were visually reviewed;
these are isolated tiny-skia renders, not installed-session screenshots.

Review corrections: remove missing-art slots; bound artwork through the existing
thumbnail decoder; preserve progress at automatic sizes; match seeking controls to
the chosen finish; retain keyboard transport and timer actions; show disabled live
Focus controls honestly; preserve the phase while paused; refresh setup preview
geometry before a new runtime snapshot; reset sample state when loading another
widget kind. Save/Apply/Undo leave the active timer file and player preferences
unchanged. Older recipe serialization and setup export versions remain compatible.

A release-mode local app was built with Rust 1.93.0, locked dependencies and no
network access. App version remains 0.5.0; this work is unreleased and uninstalled.
No live player/session or public release assets were changed.
