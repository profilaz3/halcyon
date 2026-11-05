# App UX implementation

Implementation authorized 2026-11-02. Implements [the reviewed proposal](APP-UX-REDESIGN.md).
The three-screen direction was accepted on 2026-11-03. Its wider rollout has been reviewed
locally; user acceptance of the remaining screens is pending.
Version remains 0.5.6. Local implementation only; installation and release are separate work.

| Stage | Status | Delivered |
| --- | --- | --- |
| 1. Predictable actions | Implemented and reviewed | Notes text Undo/Redo; hidden timer controls; explicit styling scope; save/error feedback; retained previews; local preview Undo; per-value conflict resolution |
| 2. Navigation | Implemented and reviewed | Desktop, Appearance, Setups and Settings; historical Back; restored search, tab, scroll and display; one widget editor with Content, Design and Position; existing routes retained |
| 3. Native patterns | Implemented and visually reviewed | Desktop workspace; compact navigation rail; scene and inspector layouts with one scroll owner; fixed preview actions; compact, wide, light/dark and enlarged-text checks |
| 4. First run and maintenance | Implemented and reviewed | Staged setup with bars, widget selection/order and city; one review/apply boundary; keep/forget/resume; completion screen; unified Updates card and Panel activation route |
| 5. Remaining screens | Implemented and validated | Browse/Saved/Mix/Schedules; retained builder designs; legacy draft migration; feature routes; integrated interaction regressions and visual review |

## Behavior

- A root opens its landing page. Back follows history; a page's parent action returns to its list.
- Widgets remain prominently named on Desktop. Each widget owns Content, Design and Position.
- Live controls save directly. Notes has Save/Revert. Multi-part changes use one named Apply.
- Colors stages the theme, mode, accent, scope and bar styling together. Wallpaper stages its
  picture, fit and effect together. Existing theme fields and supported surfaces remain available.
- Leaving keeps edited previews. The sidebar identifies unapplied work. Restart requires review
  before applying recovered changes. Untouched setup browsing creates no retained edit.
- Conflicts show the current and proposed values, with Keep current or Use my preview per value.
  Apply stays blocked until resolved; unrelated newer changes remain intact. On Appearance,
  which has no Apply button, settling the last conflict applies the waiting choice. The desktop
  editors (Dock & panel, System appearance, Wallpaper, Desktop icons, Windows, Workspaces,
  Floating apps, Shortcuts) offer Use current value or Keep my change for the named keys.
- Undo stays in the active note or preview, even when its local history is empty. Desktop history
  remains in Settings → Recovery.
- Onboarding keeps Dock & panel. Keep current preserves existing bars; Islands intent is staged
  independently from colors. City and widget placement belong to the final setup transaction.

## Integrated validation — 2026-11-02

The full suite and final focused recheck account for **1,511 passing tests and 22 existing ignored
tests**. The full run had 1,510 passes and one obsolete Setup Back assertion. After updating that
assertion to distinguish the in-page parent action from historical Back, both final focused tests
passed, including every built-in setup at compact and wide sizes and incomplete-color Discard.
No failing regression remains from these runs. This total combines the full run and its recheck;
it is not a second full-suite run.

The production release binary builds with Rust 1.93.0, locked dependencies and offline mode.
Native interaction coverage includes repeated rendered frames, pointer and keyboard actions,
search reveal, recovery, guarded writes, queued workers, local Undo and disabled Apply controls.

Visual review covered 33 compact routes and 11 onboarding states, plus representative wide,
light-theme and enlarged-text layouts. Follow-up captures cover compact Setup review, per-value
conflicts, custom colors, the theme picker, Updates and the builder's two-row compact action bar.

These are native Iced/libcosmic fixture renders. They do not establish live compositor/GPU,
physical-display, screen-reader or user acceptance. No host installation, service restart, logout,
package publication or version bump was performed.

## Review corrections

- Preserved preview baselines, exclusions, partial Mix choices, local history and widget designs
  across navigation and restart, including migration from the previous single-preview format.
- Fixed unchanged automatic-mode guards, effective color choices after conflict resolution and
  incomplete custom-color recovery/Discard.
- Preserved placement display choices through the same draft and Undo boundary.
- Corrected duplicate native widget IDs, changing footer child layouts and pinned scene coordinates.
- Kept compact Apply controls visible, including enlarged text, and bounded Setup scene height.
- Removed the test-only restriction from the schedule draft query used by the production sidebar.

## Evidence

Private local evidence: `.task-app-ux-implementation-20261102/`.

- `logs/parent-full-r6.log`: complete integrated run.
- `logs/parent-focused-r7.log`: final Setup navigation and incomplete-color confirmation.
- `logs/parent-app-build-r7.log`: production build.
- `final-native/` and `final-native-r7/`: final native captures.
- `source-final-r7.json`, `preserved-sources-final.json`, `artifact.json`: source and binary hashes.
- `final-report.md`: detailed acceptance and limits.

Earlier Activity island and packaging work remains intact. The evidence directory retains the
starting diff, review checkpoints and earlier test results.


## Three-screen refinement — 2026-11-03

Accepted examples used as the baseline for the remaining editors:

- **Desktop:** give the scene most of the width. Keep Widgets, Wallpaper, Panel and Dock
  beside it; place secondary desktop links below. Keep widget arrangement clearly named.
- **Clock / Design:** put a readable clock beside Face and Surface, then use two columns
  for Size & alignment and Typography. Keep detailed explanations behind question marks.
  Existing moment, shared-finish, custom-design and depth controls remain available.
- **Surfaces:** compare four large finish samples directly on the same wallpaper. Keep
  scope above the choices and Adjust/Reset below. Remove the separate miniature preview.
- **Navigation:** name the historical Back destination; remove the repeated current-page
  breadcrumb. Preserve the action's name and capitalization in Undo.

The main application builds with the same locked, offline Rust 1.93.0 configuration.
Sixteen focused checks cover compact and wide layouts, native clicks, search reveal,
all route/tab widget IDs, Back, guarded finish writes, unknown preferences and Undo.
Detailed results and native images: `.task-ux-three-screens-20261103/`.

Screenshots use the real native widgets with isolated wallpaper/applet/provider fixtures.
Compact and wide page captures reserve 48 logical pixels for the titlebar; the titlebar
itself is omitted. Enlarged-text captures exercise the smaller logical viewport and may
scroll. These checks do not establish live compositor/GPU or physical-display acceptance.
No installation, release, service or desktop-session change is part of this refinement.

## Remaining editors — 2026-11-03

- Widget Design tabs pair a readable sample with their primary controls, then use the full width
  for the remaining groups. Content tabs keep live controls and editing actions distinct.
- Weather puts location beside its sample. System monitor puts reading controls first, with two
  columns for browsing and one ordered list for rearranging. Focus pairs its timer with session
  lengths and keeps behavior below.
- Colors puts the desktop beside color choices, with scope and mode below. The custom picker
  stacks in compact windows. Fonts, windows and Dock use the same grouped proportions.
- Desktop icons puts visibility beside size and spacing; its schematic is under Layout preview.
  Windows puts tiling beside its schematic, with gaps, focus and window controls below.
- The builder uses the shared column layout and retains its fixed actions. Notes, Setups, Mix,
  placement, maintenance and onboarding retain their existing editing and review structures.

Final validation: **571 Settings tests passed, zero failed, eight existing tests ignored**.
The separate native tour passed across 33 routes in compact dark/light, wide and enlarged-text
viewports. Visual inspection covered every compact route and representative other sizes;
the tour also checks repeated frames and duplicate IDs. The production binary builds with
Rust 1.93.0, locked dependencies and offline mode.

Source changes in this pass are limited to Settings views and four existing test files.
Native images use isolated data; live compositor/GPU, physical-display and screen-reader
acceptance remain separate. Evidence: `.task-ux-rollout-20261103/`, including the task-only
diff, final test logs, source hashes, native captures and screen-family review decisions.

## Panel and Dock applet controls — 2026-11-03

Both editors now show a named Remove button beside each applet. Dock rows give names more
room and put status below the name. Moving an applet retains its selection and keyboard
controls after saving. Controls wait for an active save; idle hovering starts no worker.
External changes to the current applet list cancel stale drags and selections, while
unrelated changes preserve them.

Validation: **575 Settings tests and 18 panel/applet tests passed; eight existing tests
ignored**. New coverage exercises both bars, native background workers, add/search,
movement, keyboard focus, drag/cancel, duplicates, missing applets, empty groups, Undo,
repeat clicks during a save and external layout changes. Native views were reviewed at
compact, wide and enlarged-UI logical sizes. The production binary builds. The installed
application and desktop session were not changed. Evidence: `.task-applet-editor-20261103/`.

## Conflict choices and style list — 2026-11-04

- The separate style browser screen is removed. Appearance shows one row of styles; Show all
  lists every style in place. `--settings styles` opens Appearance with every style shown.
- Appearance re-arms its instant apply when a conflict is resolved, so a choice blocked by an
  outside change applies once the last conflict is settled instead of waiting with no Apply button.
- Desktop editors gained Keep my change: the draft keeps its edits and takes the current values
  of the conflicting keys as its baseline. Apply then writes the edits over them; a value changed
  again after that is a new conflict. Wallpaper colors use the same banner with Keep my color's
  behavior, replacing the separate button.
