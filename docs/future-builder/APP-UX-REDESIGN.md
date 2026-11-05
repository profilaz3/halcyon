# Halcyon: whole-app UX proposal

Date: 2026-11-01, revised 2026-11-02. **Approved for local implementation on 2026-11-02.**
See the [implementation tracker](APP-UX-IMPLEMENTATION.md) for native validation and remaining
limits. This has not been tested with users or released. Baseline: 0.5.6 plus the local Activity island work. This covers
the whole application. The island is one optional feature, not its organizing model.

[Interactive proposal](concepts/app-ux-proposal.html) ·
[Critique and decisions](../../.task-app-ux-audit-20261101/claude-opus/report.md) ·
[Structure review](../../.task-app-ux-audit-20261101/flow.md) ·
[Interaction review](../../.task-app-ux-audit-20261101/interaction.md) ·
[Visual review](../../.task-app-ux-audit-20261101/visual.md)

## The proposal in one page

Halcyon has useful customization, but it makes people learn its internal organization. One
widget is edited in several places, similar screens save differently, and Back, Undo and
preview do not always mean what they appear to mean. More decoration will not fix that.

**Your desktop is the app.** The landing screen is a large picture of your real desktop.
Click anything on it (wallpaper, panel, dock, a widget) and its one editor opens. Toggle a
widget and it appears right there. Four things hold the rest together:

1. **Four stable roots** with one-line definitions.
2. **One screen, one contract.** Every screen is visibly live, a preview, a document, or a
   command, and it never mixes save rules without a marked boundary.
3. **One visible Undo target**, shown by name, that always matches Ctrl+Z.
4. **Previews wait for you.** Leaving never applies or discards them, and they stay listed
   as “Not applied” until you decide.

Keep the native toolkit, provider ownership, guarded stores, conflict-aware Undo, package
checks, display recovery and every existing capability. Do not add a global Save basket, a new
state framework or a new preview runtime.

## Four roots

| Root | One-line definition | Contents |
|---|---|---|
| **Desktop** | What you can point at on your screen | Picture of the desktop; Wallpaper and effects; Panel and Islands; Dock; Desktop icons; all nine widgets, each with Content, Design and Position; Windows & workspaces; Shortcuts |
| **Appearance** | What everything shares | Colors (theme, mode, accent as one preview); Surfaces (Everything, Widgets, Panel or Dock); Fonts & windows |
| **Setups** | Whole looks you try, save and schedule | Browse; Saved (save, import, export); Mix; Schedules |
| **Settings** | Halcyon itself | Accessibility (first); Startup; Interface; Updates; Privacy; Recovery; Help |

**Why not five roots.** The earlier draft split Desktop (placement) from Widgets (content and
design) and called them complementary. That split is the source of the overlapping editors:
people must learn it, the sidebar jumps when they click a widget on the canvas, and Position
becomes a trip to another page and back. Merging them removes the choice. It also lets cause
and effect share one screen: the widget switch and the widget's appearance on the picture sit
side by side. On phones and other desktops, widgets are added from the home surface, so the
pattern is familiar.

*Risk and reversal:* “Widgets” is a strong word and some people may look for it in the sidebar.
The Desktop landing shows a Widgets heading immediately, and search finds it. If observation
shows people still hunting, add a fifth sidebar entry that opens that same section. No editor
or route changes, so this decision is cheap to reverse.

**Why Wallpaper moves to Desktop.** It is the most visible thing on the desktop, COSMIC Settings
files it under Desktop, and clicking it on the picture should not teleport you to another root.
Appearance keeps a “Looking for wallpaper?” row that opens the same editor.

**Windows, Shortcuts and icons stay in Desktop.** They are about your desktop, not about
Halcyon. That keeps the definition of Settings clean.

**Setups and Appearance stay separate roots but share one library.** Appearance edits what is
current. Setups chooses and saves whole configurations. Native themes are already stored as
`appearance_only` profiles, so themes can share the same library and review screen, with an
**Appearance only** filter in Setups and a Browse link in Appearance. These records are not
necessarily colors-only: they can include widget surfaces and dock/panel styling, such as
opacity, radius, spacing and floating behavior (`src/desktop/profiles/themes.rs::record`).
Review must list the actual included changes before Apply. The prototype's Moss and Carbon
examples contain only colors; they demonstrate a subset, not a restriction on existing themes.
Storage, formats and supported theme fields stay intact.

### Navigation rules

- A **root** click always opens that root's stable landing.
- **Back** is real history and names its destination (`‹ Clock`). It restores the tab, display,
  scroll and search query.
- The **breadcrumb** is the parent, and is separate from Back.
- Flat tab changes do not add history.
- Search results show where you will land, reveal the exact control, and Back returns to the
  results with the query intact. Every feature gets a task name in the same change that adds it.
- Existing CLI routes, deep links, aliases and saved data stay valid.

### Where every current route goes

| Current route IDs | Proposed home |
|---|---|
| `Overview`, `WidgetLayout`, `Desktop`, `Widgets` | Desktop landing |
| `Glance`, `Clock`, `Weather`, `Calendar`, `Devbox`, `Projects`, `Music`, `Notes`, `Focus` | Desktop › that widget |
| `Builder` | Desktop › widget › Design › Your own design |
| `DesktopWallpaper` | Desktop › Wallpaper |
| `DesktopDock` | Desktop › Dock and Desktop › Panel |
| `DesktopIcons` | Desktop › Desktop icons |
| `DesktopWindows`, `DesktopWorkspaces`, `DesktopWindowRules` | Desktop › Windows & workspaces |
| `DesktopShortcuts` | Desktop › Shortcuts |
| `Themes` | Appearance › Colors, and Setups filtered to Appearance only |
| `Appearance` | Appearance › Surfaces |
| `DesktopAppearance` | Appearance › Fonts & windows |
| `DesktopProfiles` | Setups › Browse and Saved |
| `DesktopAutomation`, `Moments`, `AdaptiveLooks` | Setups › Schedules, plus per-widget Design for overrides |
| `General` | Settings |
| `DesktopMotion` | Settings › Accessibility |
| `PendingChanges` | Settings › Recovery › Not applied yet; verify exact behavior when implementing |

## One screen, one contract

Each screen shows which of these it is. The earlier draft had the same four contracts but let a
single editor mix them; the rule below is the fix.

| Contract | What you see | Leaving the screen | Undo |
|---|---|---|---|
| **Live** (default) | No Apply button. `Saving…` beside the control, then `Saved · Clock face`. Errors stay beside the control and the value does not pretend to change. A repeat click cannot queue a second save. | Value stays. | Named: `Undo Clock face`. Restores only its keys; refuses and says so if they changed again. |
| **Preview** | A `Preview · not applied` badge on the picture and an accent bar with `Discard` and one named Apply. Every control on the screen belongs to that one preview. | Kept and listed as “Not applied”. Never applied or discarded by navigation. | Undoes preview steps only. Apply then makes one desktop change. |
| **Document** | Notes only. `Unsaved` / `Saved`, `Revert`, `Save note`. | Text kept and listed as “Unsaved note”. | Text only. An empty text history stops there and never reaches the desktop. |
| **Command** | A marked `Live` group with verbs: Pause, Connect, Open system updates. Shows real state and `Pausing…`. | Keeps running. | None implied. |

**What is live and what is a preview?** A single setting is live. Anything that replaces several
things at once is a preview: wallpaper with its fit and effect, a theme with all its included appearance fields (not just mode and accent),
widget placement, a setup, and a widget's own built design. Native Clock and surface controls
already apply immediately, so they stay live with a clear Undo. Canvas style controls disappear
because that form moves to the one Clock editor. Showing a widget is always live and always in
the same header position.

**Undo precedence** is by screen: this note's text, then the preview on this screen, then the
latest desktop change. The bar shows the actual target. A no-op in a local scope never falls
through. Keyboard and button always agree. All desktop changes remain in Settings › Recovery with
per-entry Undo, marked “Changed again later” when blocked.

### How waiting previews look

The previous draft used a small footer button labelled with a draft count. “Draft” is jargon and
the state was easy to miss. Now:

- Leaving says so in the status bar: `Wallpaper preview kept. Not applied.` with `Resume`.
- The root gets a dot, and the sidebar lists “Not applied” items. The compact rail shows a count.
- A review opened and left untouched leaves nothing behind, so browsing setups creates no clutter.
- No blocking “save changes?” dialogs.

### Conflicts are three-way

Retained previews keep a baseline. If something newer changes the same value:

| Your preview | Current value since baseline | Result |
|---|---|---|
| Unchanged | Changed | Preview refreshes silently |
| Changed | Same as your value | No conflict |
| Changed | Changed differently | **Choice:** thumbnail/value of `Keep current` beside `Use my preview`, per value |

Apply is disabled until each conflict is chosen, and writes only the values you changed. Turning
off a setup part clears that part's conflict. History keeps immutable action IDs and never rewinds
newer unrelated data.

## Design scope

Generic “Widget style” links can land on All surfaces, Dock or Panel. Scope is now always visible
and short: a `Changes: Clock only` line on each design screen.

- **Widget › Design:** that widget's own controls (live) plus Surface: `Follow desktop`, `None`,
  `Panel`, the existing native choices. No per-widget material is claimed; that would be new work.
- **Desktop finish** is a separate row, marked “Shared by all widgets”. It opens Appearance ›
  Surfaces with the Widgets scope already chosen, and Back returns to the widget.
- **Appearance › Surfaces:** `Applies to` Everything, Widgets, Panel or Dock. The affected objects
  are outlined on the picture as you switch. An individual target shows whether it follows
  Everything, with `Use Everything`.
- **Your own design:** the builder, reached from a widget (the six native recipe kinds). Parts,
  Compact/Wide layout, sample state, a labelled Design name, saved designs and `Save a copy`.
  `Apply to Music` is primary. Saving a copy changes nothing on the desktop.
- **Required but not demonstrated:** an effective-source label such as `From Paper setup`,
  `Custom` or `Night only`, with one `Use default`, for values with several origins.

Display language stays exact: **Viewing** changes only the picture, **Widget location** moves a
widget on Apply, and **Changes** on wallpaper names its display. Global fonts and colors never
look per-display. Disconnected assignments and fallback choices are preserved.

## Three layouts, one scroll owner

| Layout | Used for | Wide (1280) | Native minimum (820 × 600) | Phone-width review |
|---|---|---|---|---|
| **Form** | Settings, windows, shortcuts, fonts | Readable max width | Same | Single column |
| **Object editor** | Widgets, panel, dock | Object at useful size on your wallpaper, controls beside it | Same, narrower | Picture, then controls |
| **Composition** | Desktop, wallpaper, colors, surfaces, setup review, design | Large picture and a bounded inspector | Picture stays pinned while the inspector scrolls with the page | One ordered scroll |

The page is the only scroll owner. There are no nested scrollers and no gallery below two
independent panes. At 820 px the sidebar becomes an icon-and-label rail, which gives the picture
about 80 px more width. Phone width is only a reflow so the proposal can be reviewed there. It is
not a request to turn the desktop app into a phone app.

**Striking without decoration.** The drama comes from the real picture at a large size, with real
panel, dock and widgets, and a soft ambient wash from the current wallpaper behind the page header.
Chrome stays opaque, theme-aware and native COSMIC. The wash is optional: a gradient container that
can be dropped if it costs too much natively.

**Control vocabulary.** A filled button does this screen's task. A row with a chevron opens
something. A `?` explains. A switch or segmented control changes a value. Status is never styled
as a button, and sample pictures have no hover or focus affordance. Every sample says `Sample
content · not clickable`, and real commands sit in a marked `Live` group outside the picture.

**Plain text and help.** Essential facts stay visible: scope, save timing, errors, next-login
changes. Everything secondary moves behind `?`, which opens on hover, keyboard focus or click,
stays while you read it, and closes with Escape, matching the existing native help behavior.

## Onboarding

Welcome artwork, then four steps with a large preview that follows every choice. **Dock & panel
stays.**

1. **Start with:** keep the current desktop, or begin from a setup.
2. **Dock & panel:** `Keep current` or `Customize` with the existing choices: panel Bar or Islands
   and shape, dock shape, attachment, hide behind windows. `Islands start at next login. No need to
   log out now.` appears beside the choice.
3. **Your widgets:** on/off, order, target display. The city field appears here only when Weather
   is on and no city is set. Arrange and Weather stop being separate steps.
4. **Review:** each choice with `Change`, which returns straight to Review. Truthful exceptions:
   `Weather will say “Needs a city”`. Then **Apply and finish**.

**Completion** is an actual screen: `Your desktop is ready`, what was applied as one change, one
`Undo setup`, what starts at next login, and optional next tasks such as `Connect calendar`.

**Leave** is named by outcome: `Keep choices for later` or `Forget choices`, and `Your desktop stays
as it is` either way. Kept choices resume from Settings › Recovery. The city is part of the draft,
which fixes today's city that survives Cancel. Accounts stay separate explicit actions after setup;
Cancel never promises to revoke anything already granted. Final placement uses the chosen bars and
current display geometry; real desktop changes during setup keep existing conflict handling.

This requires native staging changes. Relabeling today's buttons would not create this contract.
Interim repair if staging is delayed: label writing steps `Apply and continue`, and name exits by
what they keep.

## Issues addressed

| ID | Priority | Current problem (source evidence) | Outcome in this proposal |
|---|---|---|---|
| UX01 | First | Notes passes Ctrl+Z to an editor with no Undo binding; an ignored key can invoke desktop Undo. [Global handler](../../src/settings.rs:1288), [Notes binding](../../src/settings/view/tools.rs:261). Specific to Notes, not custom TextInput. | Notes owns its Undo even when empty. Never falls through. |
| UX02 | First | Wallpaper has Apply/Cancel beside immediate effects; Themes mixes a candidate with direct mode. [Caption](../../src/settings/view/wallpaper.rs:553), [effects](../../src/settings/view/rain.rs:1). | One screen, one contract. Picture, fit and effect are one preview; so are theme, mode and accent. |
| UX03 | First | Leaving Themes/Setups cancels; leaving other editors retains or flushes. [Navigation](../../src/settings.rs:2922). | Retained and listed. If retention cannot ship with a screen, ask before leaving. Never silently discard. |
| UX04 | First | “Widget style” opens a retained shared target. [Link](../../src/settings/view/pages.rs:91). | Widget-owned Surface plus a labelled shared row that forces the Widgets scope. |
| UX05 | First | Focus actions disabled when hidden, though the timer runs. [Gating](../../src/settings/view/tools.rs:422). | Marked Live group works regardless of visibility. List says `Running · hidden`. |
| UX06 | Next | Sidebar restores a hidden leaf; Back uses structural parent; search context is lost. | Stable landing, real history, named Back, retained query. |
| UX07 | Next | Widget, canvas, builder, Moment and setup editors overlap. | One editor per object with Content, Design and Position. |
| UX08 | Next | Native writes set busy text that ordinary footers omit; actions queue invisibly. | `Saving…` at the control and in the bar; repeat clicks coalesced. |
| UX09 | Next | Footer and keyboard Undo can target different scopes. | One named target that always matches Ctrl+Z. |
| UX10 | Next | Onboarding applies some choices now and some later; a city survives Cancel. | Staged, reviewed, applied once, with a clear ending. |
| UX11 | Next | Nested scroll; samples, live controls and hotspots look alike. | Three layouts, one scroll owner, marked samples, separate Live group. |
| UX12 | Next | Unified install shows Halcyon and Islands as two cards. | One update card. Islands listed as included; it turns on in Panel. An exception appears only when real. |
| UX13 | Next | Spectrum and visualizers are not in search. | Named entries that reveal the exact control. Dock & panel stays findable by its old name. |

Priorities are implementation order, not measured frequency. Source traces establish behavior; how
confusing each one is still needs observation. Specific defects need native reproduction when fixed.

## Preserved capabilities

- **Widgets:** Daily overview, Clock, Calendar, Weather, System monitor, Projects, Music, Notes and
  Focus, with calendar privacy, import and OAuth, project roots, content editing, audio visualizers,
  Spectrum and timer persistence.
- **Desktop:** wallpaper sources (Halcyon, COSMIC, Yours, solid and gradient), fit, display scope,
  effects; all dock and panel controls; applets and Islands arrangement; arrange-on-desktop overlay;
  snap; windows, workspaces, floating-app rules, desktop icons, shortcuts.
- **Setups:** all nine parts (appearance, wallpaper, dock, panel, windows, workspaces, desktop icons,
  widget style, widget layout), remix, import, export, portable-package review.
- **Schedules:** existing solar, time and display triggers, presets for Daily cycle and Day & night
  widgets, city binding, display rules, overrides, pause and run history. Rule form: **When → Change
  what → To which saved choice**.
- **Settings:** accessibility, update channels, support export, restore with review, conflict recovery.
- **Updates:** APT/DNF/system-manager authority stays. No fabricated progress, no `up to date` on
  failure, no forced logout, no hidden mismatch.
- **Activity island** stays an optional panel applet with its own design documents.

## Implementation sequence

The approved sequence requires native checks and visual review for each stage. Current
results are recorded in the implementation tracker rather than in this design specification.

| Stage | Work | Required check |
|---|---|---|
| 1. Predictable actions | UX01 to UX05, UX08 and UX09 on current routes | Focused native reproductions; exact Apply, Discard and Undo tests; busy and error states |
| 2. One navigation model | Four roots, history, search; object editor with canvas as Position; aliases | Clock → Position → Clock and search → result → Back; every old route resolves |
| 3. Three native patterns | Updates as Form, Clock and Music as Object, Wallpaper as Composition, Desktop landing | Real 820×600 and 1280 captures, enlarged text, light/dark, keyboard, empty/busy/error. Fix before migrating more |
| 4. First run and maintenance | Staged setup with bars, one update card, activation and recovery | New user, skip, leave, resume; system-update handoff; next-login states |
| 5. Remaining families | Setups, Mix, schedules, every widget and provider, shared appearance, shell | Capability and route coverage audit, saved-data compatibility, cross-screen tasks |

Native work identified by the proposal: retained setup and theme candidates, bounded note text
history, removing the Focus visibility gate, busy states in ordinary footers, wizard staging,
hosting the canvas inside the object editor at 820 px, and clear labels for inherited values.
The sample's stage is hand-drawn HTML; native should reuse the existing scene and preview views
rather than build a parallel renderer.

## Proposed observation tasks

Not yet run with people. Ask whether someone can say **what changes, when, and how to go back** before
reading help.

- Change a Clock design without changing other widgets; move it to display B; return to the same editor.
- Preview a new picture and fog, navigate away, return, then discard both.
- Edit a sentence and undo it without altering desktop history.
- Hide a running Focus widget, pause the timer, and keep the widget hidden.
- Apply only the wallpaper from a setup; say what stays.
- Create a sunset appearance rule without learning “Moments” versus “Adaptive looks”.
- Finish or leave onboarding with bars, widgets and a city; predict the outcome.
- Find Spectrum through search; return to the same results after editing.
- **New:** enable a widget without being told where. Do people look on Desktop, or hunt for a Widgets root?
- **New:** change wallpaper starting from Appearance. Does the “Looking for wallpaper?” row catch them?
- Update through the system channel and understand any next-login panel change.
- Recover an interrupted preview and resolve a conflicting later edit without data loss.

## Evidence and limits

Three independent read-only reviews covered journeys, interaction state and visual hierarchy, with a
fourth challenging the first proposal. This revision was then challenged and rebuilt against native
source; see the [critique and decisions](../../.task-app-ux-audit-20261101/claude-opus/report.md). Older
captures corroborate structure only; current source takes precedence. Fixture versions, missing assets
and provider data are not findings about any installed machine.

The prototype uses sample data: images, times, events, tracks, versions, cities and update states. It has
no backend, and it does not touch a desktop, a package manager or an account. Actions such as Connect,
Import, Export and Open system updates only name the command. Its checks validate the prototype's own
behavior and layout, in a browser. **They are not evidence that any native bug is fixed, that the design
works for novices, or that native rendering, text scaling or assistive technology will behave the same.**

## Design references

- [COSMIC toolkit concepts](https://pop-os.github.io/libcosmic-book/cosmic-concepts.html): native header,
  navigation and context patterns.
- [GNOME navigation guidance](https://developer.gnome.org/hig/guidelines/navigation.html): shallow,
  conventional navigation. Not a proposal to move to GTK or Adwaita.
- [W3C hover and focus content](https://www.w3.org/WAI/WCAG22/Understanding/content-on-hover-or-focus.html):
  revealed help is reachable, dismissible and persistent. Guidance here, not a certification.
