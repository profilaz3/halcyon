# Product polish for 0.5

Authorized 2026-10-28 from the four reviews of the installed 0.4.1 application,
checked against the local 0.5 implementation. Source baseline: `ce719511`.
Review evidence: `.task-041-product-review-20261028/`. Implementation, checks and
native captures: `.task-product-polish-20261028/`.

## Decisions

- Keep Dock & panel as a dedicated onboarding step. Show the setup's selected
  appearance, accept it with Next, and keep common choices visible. Put detailed
  sizing and spacing under More options. Do not shorten onboarding by removing it.
- Keep the six navigation roots and all six setup inclusion switches.
- Preserve local draft, immediate-change, Apply, Cancel, exact Undo and recovery
  boundaries. Explain them consistently; do not introduce a global Save basket.
- Extend existing native layout and preview helpers. No replacement UI framework.
- Test in isolated profiles and private desktop sessions. Do not log out, restart
  the user's desktop, or install over the live system.
- One Cargo invocation at a time. Review each meaningful slice and inspect native
  compact/wide renders after visual changes, fixing findings before closing it.

## Implementation tracker

| Work | Review IDs | Acceptance | State |
|---|---|---|---|
| Saved widget selection and global visibility | FLOW01 | Hide/show preserves individual choices; hidden-state controls work after restart | Complete locally |
| Fresh Builder initialization and inline legacy upgrade | FLOW04, F02 | Fresh Apply has no Recovery detour; legacy/corrupt/newer records retain safeguards | Complete locally |
| Calendar link guidance | F01 | Provider guide beside form; distinct validation and keyring errors retained | Complete locally |
| Explicit widget groups and relevant follow-up tasks | FLOW03, F03 | Section hierarchy and counts clear; selected unconfigured tasks dismiss/reopen correctly | Complete locally |
| Compact onboarding with dedicated Dock & panel | FLOW06, VP02 | Common choices and footer fit; progress matches steps; Cancel remains scoped | Complete locally |
| One primary layout editor | FLOW02 | Explicit draft Apply/Cancel; real-desktop alternative; selection/display/recovery preserved | Complete locally |
| Setup inclusion, library and export presentation | FLOW05, VP03, IMP01 | Six switches visible; one action strip; image/export/library verbs distinct | Complete locally |
| Useful empty states | VP06 | Save current desktop / Import and Choose folder are primary actions | Complete locally |
| Appearance-to-schedule journey | FLOW07 | Direct entry, clear affected scope and next switch; existing pause semantics retained | Complete locally |
| Shared scene/control/form page patterns | VP01, IMP04 | Useful control width, stable child identity/focus/drafts, compact/wide and large text | Complete locally |
| Fonts and icons specimen | VP04 | Meaningful type/icon examples and wide selectors; optional window sample | Complete locally |
| Builder and task-specific status/actions | VP05, IMP01 | One action strip; unrelated prior edits are not presented as current-page changes | Complete locally |
| Consistent preview contracts | IMP02 | Sample/current/candidate/live states clear; samples inert, live tools usable | Complete locally |
| Useful privacy-safe history | IMP03 | Safe specific summaries; old-schema fallback; exact eligible Undo only | Complete locally |
| Schedule form and collapsed Dock width | VP07, IMP04 | Already fixed locally; retain regression coverage | Existing fix retained |

## Separate feature proposals

The review explicitly placed these later proposals outside its completion plan.
They remain separate feature work, rather than completion claims for this pass.
FB16 currently admits only reserved singleton identities: adding a second clock
needs instance-aware runtime windows, placement, profile mapping and editing,
not a duplicate library recipe. Retained-content previews need explicit opt-in and
separate local/public rendering boundaries. The current sample defaults stay.

| Proposal | Review ID | Required contract | State |
|---|---|---|---|
| Independent Clock copies | F04 | Stable identity, independent style/display, add/remove, Undo, import and old-setup mapping; shared provider demand | Future feature, scoped |
| Private preview of retained content | F05 | Explicit opt-in, cached reads only, no polling/actions; sample-only public/export paths and Busy privacy | Future feature, scoped |

## Validation record

Implemented and validated locally, unpublished.

- New focused regressions: **31 passed**, also passing in the broad r10 run, including fresh initialization,
  shared XDG roots, old/corrupt/newer record refusal, saved versus effective widget
  visibility, reminders and exact Undo, inline migration, all Calendar guides,
  Layout recovery and late input after Cancel, safe history and schedule routing.
- Native tree captures: compact 820×600 and wide 1280×800; onboarding also
  1024×760. Shared layout checks include larger default text and focus/draft
  preservation through resize. Evidence: `integration/native-r8/`,
  `integration/native-r10/` and corrected interactions in `integration/native-final/`.
- Optimized release build passed with Rust 1.93.0, `--locked --offline`.
- Full regression run: **1,396 passed, three test failures, 18 existing ignored
  worker/platform cases**. The three failures came from old UI test assumptions:
  centering can be a no-op; compact content begins at x=184; Save & export must
  be revealed before clicking. The centering helper also needed to scroll the
  controls in the Canvas inspector. After correcting the journeys, **all three
  passed**, giving **1,399 distinct passing Rust tests**. Functional placement,
  focus, export, Cancel and Undo assertions remain. Logs: `integration/full-r10.log`
  and `integration/corrections-final.log`.
- Private COSMIC release-binary walkthroughs passed at **820×600 and 1280×800**,
  with **44 actual window captures**. Configuration remained unchanged after
  local drafts, Cancel and late input. All fixture processes were reaped.
  Evidence: `native/{compact-final,wide-final}/result.json`. The Adaptive looks
  capture uses its actual `adaptive` route; an earlier harness spelling was fixed.
- Formatting and whitespace checks passed. Source hashes are retained in
  `integration/source-final.json`. About **70 GiB** remains available on the SSD.
- Independent integration reviews covered state, flow/actions and native visuals.
  Their fixes are listed below; none remain open in this pass.

### Review fixes included

- Fresh initialization accepts its own locks when XDG roots share a directory;
  real prior state still prevents automatic migration.
- Dismissing optional setup tasks uses the guarded preference allowlist; exact
  Undo and external refresh restore the current reminder state.
- Read-only Layout landing rejects late movement/save events after Apply/Cancel.
  Widget Move & resize shortcuts enter Canvas on the correct display.
- Retained pre-Canvas drafts have explicit guarded Save/Cancel recovery controls.
- Preview help distinguishes sample content, actual images, navigable desktop
  scenes and live tools.
- Dock attachment and visibility are separate choice pairs. Calendar privacy
  help shares the action row. Builder actions occupy one footer row.
- Surface finishes uses the wider controls column for its choices. Schedule
  forms use one heading and compact labelled rows, keeping Save in view.

Physical-display transitions, real provider sign-in and assistive-technology
acceptance must not be inferred from fixture screenshots. The user's installed
application and desktop session have not been replaced or restarted.
