# Immersive native Settings implementation plan

The agreed direction combines clear native navigation with an interactive desktop
preview, focused object controls, and a dedicated Setups workflow. Implement it
in libcosmic using the existing GPU renderer. Preserve every current setting and
the existing apply, Undo, conflict handling and recovery behavior.

Design direction accepted 2026-10-25. **Implemented and validated locally on 2026-10-25.** The installed release is unchanged.
Validation and native captures are recorded below.

## Design references and boundaries

- [Interactive immersive concept](../.task-settings-design-concepts/immersive.html)
- [Dock and panel reference](../.task-settings-design-concepts/imagegen/01-dock-panel-dark.png)
- [Appearance reference](../.task-settings-design-concepts/imagegen/02-appearance-light.png)
- [Setups reference](../.task-settings-design-concepts/imagegen/03-setups-review-dark.png)
- [Direct interaction contract](adr/0003-settings-interactions-and-state.md)
- [Native GPU rendering decision](adr/0004-hardware-rendering.md)

The images guide composition, hierarchy and atmosphere. Use actual widget
rendering, native symbolic icons and theme-derived colors, spacing and radii.
Generated calendar details, app icons, shading and explanatory labels do not add
features or change saved configuration schemas. In particular, selecting a theme
must not silently switch native light/dark mode, and Setups must retain their
actual supported capture domains and portability rules.

This plan refines ADR 0003's navigation into six roots, keeping Accessibility
visible. Its direct-action model remains unchanged. Settings chrome is opaque
and theme-aware; decorative widget finishes do not imply native blur. The work
does not require a web runtime, a new daemon or a toolkit upgrade.

## Destination map

Keep existing page keys, command-line routes and search targets working through
aliases. A sidebar entry, search result and preview object must open the same
editor and state. The table covers all 28 current `Page` variants.

| Current page IDs | New home and retained functions |
| --- | --- |
| `Themes` | Appearance → Theme and colors; style row (Show all lists every style in place), color source, Whole desktop / Widgets only, theme library operations |
| `Appearance` | Appearance → Surface finishes; shared and individual surface overrides, readability, motion and legacy styling |
| `DesktopAppearance` | Appearance → Fonts and icons, detailed mode colors, window appearance and supported effects |
| `DesktopWallpaper` | Appearance → Wallpaper; pictures, gradients, slideshows, fit and display targeting |
| `DesktopDock`, `Desktop` alias | Desktop → Dock and panel; behavior, style, applets, Islands placement, advanced timing and spacing |
| `DesktopWindows`, `DesktopWorkspaces` alias | Desktop → Windows and workspaces; tiling, gaps, focus and window controls |
| `DesktopWindowRules` | Windows and workspaces → Floating apps; retain shortcut entry points |
| `DesktopIcons` | Desktop → Desktop icons |
| `DesktopShortcuts` | Desktop → Shortcuts; custom bindings and native settings links |
| `Widgets` | Widgets → My widgets; enable controls and all detail screens |
| `Glance`, `Clock` | Widgets → At a glance and Clock; content, style and placement |
| `Calendar` | Widgets → Calendar; Google setup, read-only subscriptions, calendar selection, privacy and reminders |
| `Weather`, `Devbox`, `Projects` | Widgets → Weather, System monitor and Projects; city, readings, folders, style and refresh controls |
| `Music`, `Notes`, `Focus` | Widgets → Music, Notes and Focus timer; distinguish live controls, content and settings; retain note Save/Revert |
| `Overview`, `WidgetLayout` alias | Widgets → Layout and displays; arrangement, sizing, snap/grid, automatic order and display defaults/overrides |
| `DesktopProfiles` | Setups; starter and saved configurations, included domains, library management and quick switches |
| `DesktopAutomation` | Setups → Schedules and display rules |
| `DesktopMotion` | Accessibility; reduced motion, readability links and accurate native capability limits |
| `General` | App settings; widget visibility, background operation, startup, interface, privacy, updates, installation, add-on diagnostics and onboarding |
| `PendingChanges` | App settings → Restore and recovery; unfinished work, conflicts, export and original wallpaper restoration |

Retain screens without their own page IDs: first-run wizard, getting-started
checklist, connection flows, library operations and local forms.
Keep connections accessible within their widget. First use opens Appearance;
later launches restore the last destination unless an explicit route is supplied.

## Interaction and preview rules

- Completed choices apply directly; sliders and layout gestures commit on
  completion. Partial text stays local until valid completion. Keep scoped Undo.
- Themes, wallpapers and setups use local candidates with explicit Apply and
  Cancel. Cancel writes nothing. Library capture uses Save theme or Save setup;
  forms and notes retain their own Save behavior. There is no global Save basket.
- Label distinct scopes explicitly: Color applies to, Surface, Preview display,
  Widget location and Wallpaper display. A display preview must not imply that
  global fonts or theme settings become display-specific.
- Selecting an object opens its existing controls. Provide equivalent keyboard
  navigation through an object list. Preview actions never accidentally operate
  Music, Notes or Focus; live controls are identified separately.
- Preserve the selected Islands intent when the host is unhealthy. Show the
  observed fallback and a relevant recovery action beside its controls. Healthy
  pages stay quiet; diagnostic detail is available through disclosure.
- Keep the preview collapsible and its selection stable. Reuse cached images and
  real widget rendering without creating extra provider polling or native hosts.
  Candidate previews must have no live desktop side effects. Native bar previews
  must distinguish schematic appearance from observed host status.
- At wide widths use the scene with grouped controls or a contextual inspector;
  at compact widths stack content and collapse the inspector into a subpage.
  Keep focus, scroll position and preview collapse state stable during refreshes.
  Use restrained transitions that honor reduced motion.

## Implementation checklist

### 1 Preserve the baseline and build the native structure

- [x] Record the current source/diff and existing validation references before
  edits. Preserve the uncommitted R01–R22 and T01–T06 fixes and all evidence.
  A checkout based only on HEAD would omit those fixes.
- [x] Separate sidebar roots from leaf routes in `src/settings.rs`; update titles,
  breadcrumbs, search reveal behavior and last-destination restoration together.
- [x] Establish reusable page headers, native setting sections, scoped status,
  local preview actions and responsive content layout in `src/settings/view/`.
  Keep every existing editor reachable while pages migrate.
- [x] Reuse the pinned toolkit and existing renderer. Preserve native instance
  ownership, serialized page forwarding and independent Settings lifetime.

Completion: every mapped route opens the correct editor; search reveals hidden
controls; keyboard focus follows navigation; existing CLI/controller routes and
aliases still work. No configuration migration or runtime write occurs on open.

### 2 Complete Dock and panel and Appearance

- [x] Refactor `view/desktop.rs`, `view/desktop/panel.rs` and desktop preview components
  into the agreed behavior, style and contents groups with shared object selection.
- [x] Consolidate `view/themes.rs`, `colors.rs`, `materials.rs`,
  `system_appearance.rs` and `wallpaper.rs` under Appearance without dropping
  detailed controls, library actions or overrides.
- [x] Reuse `view/home.rs`, desktop preview code and `src/app/settings_preview.rs`
  where appropriate for the immersive scene. Keep cached preview handles stable.
- [x] Connect local candidates and direct edits to the existing command paths;
  preserve native-job ordering, source generations, conflict handling and Undo.
- [x] Implement healthy, previewing, pending and failed states with accurate
  controls and recovery actions. Use the current installation refresh pipeline.

Completion: theme browse/cancel, wallpaper/color scope, direct bar edits, Islands
on/off and scoped Undo work through the redesigned screens. Verify stale work
cannot replace a newer action. The actual native screens are usable at 820×600
and larger sizes before extending the same components to other pages.

### 3 Migrate widgets and desktop arrangement

- [x] Convert all nine widget editors, retaining content, connections, refresh,
  style, live tool actions and per-widget display choices.
- [x] Extend Layout and displays with scene selection and the shared inspector;
  retain a keyboard-accessible object list and on-desktop Arrange entry point.
- [x] Preserve display identity rebasing, connector geometry, inherited defaults,
  disconnected assignments, hide/fallback policy and restoration. Keep automatic
  ordering restrictions for monitor-specific placement.
- [x] Migrate windows/workspaces, floating rules, desktop icons and shortcuts to
  the common structure; retain links to additional COSMIC settings.

Completion: list and scene entry points show the same state; content editing and
placement remain independent. Arrange completion, Escape, stale tasks, reset and
disconnect preserve the existing ownership and recovery guarantees.

### 4 Complete Setups and maintenance screens

- [x] Build current/candidate comparison and included-domain review using the
  existing setup planner. Preserve save/import/export/rename/duplicate/delete,
  starter setups, quick switches, schedules and display rules.
- [x] Split App settings into focused subpages for operation, interface, privacy,
  updates/installation and restore/recovery. Retain all onboarding entry points.
- [x] Make Accessibility and readability entry points consistent. Keep the wizard
  and getting-started checklist aligned with the new destinations and terminology.

Completion: setup Cancel writes nothing; Apply uses the existing guarded action;
saved setups remain portable. Notes, schedules and recovered forms retain input.
Hiding widgets and stopping background operation remain distinct actions.

### 5 Validate the integrated native interface

- [x] Check 820×600 and a larger window, light/dark themes, keyboard-only use,
  long labels, reduced motion and 100%/150% display scaling. No clipped primary
  controls; focus and scrolling remain stable across asynchronous refreshes.
- [x] Exercise actual native repeated launches and search/deep links; run the
  pending-edit, external-change, scoped Undo, recovery and lifecycle regressions.
  Retain the private systemd stop/restart fixture proving the independent scope
  survives and flushes a pending edit. Fresh graphical launch from the installed
  widget service remains a release-session acceptance item.
- [x] Run focused regressions for the changed routing/state paths, existing
  display/placement/Arrange suites, native queue/status tests and lifecycle tests.
  Add tests for meaningful new behavior, not copies of the layout implementation.
- [x] Compare preview navigation/scrolling and idle behavior with the baseline:
  no repeated image decoding, compatibility discovery or extra background polling.
  Use one Cargo slot, Rust 1.93.0, `--locked --offline` and the established target.
- [x] Build once after integration and capture native screenshots plus results
  for each completed slice. Update this checklist and the main tracker with
  evidence and any remaining acceptance limits.

Completion means the full destination map is implemented and the integrated
checks pass. Local UI completion does not establish physical hotplug/resume,
live calendar/collector delivery or final package qualification; track those
existing release obligations separately in [the review tracker](REVIEW-030.md).
Installation, publication and external review are separate from this plan.

## Local validation — 2026-10-25

- **370 Settings tests passed**, with two pre-existing opt-in rendering probes
  ignored. This includes native pointer/keyboard interaction, compact/wide and
  light/dark/high-contrast rendering, search reveal, all canonical destinations,
  candidate cancellation, custom-size removal in setup projections, pending
  edits, native job ordering, display placement, recovery and scoped Undo.
- **40 additional tests passed** across app display/Arrange, display identity,
  layout state and Settings process ownership. These source paths were unchanged
  by the final preview-ID correction.
- Release build passed using Rust 1.93.0, `--locked --offline`, one Cargo slot.
  Existing dead-code warnings remain. No toolkit or dependency upgrade.
- Actual release executable passed five isolated nested-COSMIC runs: retained
  baseline; matching 1100×800; 820×600; 1280×900; and 820×600 at compositor-verified
  150%, producing a 1230×900 pixel capture. Search, preview collapse/expand and
  repeated CLI forwarding worked; navigation changed no preferences; all windows
  exited cleanly. Fixture compositors/buses were stopped.
- At matching size, settled median idle CPU was approximately 4.5% for the
  baseline and 4.0% for the redesign; navigation plus screenshot capture used
  2.65 and 2.88 CPU seconds respectively. RSS was approximately 252 and 255 MiB.
  These are short software-compositor fixture measurements, not hardware frame
  timing, battery or long-session claims. Existing image-cache/discovery
  regressions passed and the redesign adds no polling subscription.

Evidence: [validation report](../.task-settings-redesign-evidence/VALIDATION.md),
[source/binary manifest](../.task-settings-redesign-evidence/validation-manifest.json),
[Settings results](../.task-settings-redesign-evidence/settings-validated.log),
[display/lifecycle results](../.task-settings-redesign-evidence/retained-safety.log),
[native GUI results](../.task-settings-redesign-evidence/native-gui-final-result.json).

Physical monitor hotplug/resume, live calendar/collector acceptance, a fresh
installed-service graphical stop/restart and final 0.3.0 package qualification
remain release work. This implementation did not install or publish a build.

## Screen-by-screen production polish — 2026-10-25

Reviewed every canonical screen, the five App settings subpages, all five wizard
steps, style browser and setup checklist at compact and wide sizes. Primary
controls now precede unnecessary preview space, long headings fit, empty widget
editors lead with setup actions, and compact Notes leads with its editor and
Save/Revert. Theme, finish, desktop and widget screens share quieter preview
framing and clearer section labels. Wide My widgets uses two columns.

Setups reveals a newly chosen candidate at the top while ordinary refreshes keep
scroll position. App settings has one subpage navigation layer. Accessibility
opens and reveals the actual readability control. Privacy copy accurately names
the reporting fields and final opt-out preference update. Existing direct
application, candidate confirmation, library, conflict, Undo and recovery actions
are retained.

- **374 Settings tests passed**, zero failures, two existing opt-in probes ignored.
  Four new regressions cover primary-action visibility, candidate reveal versus
  refresh scroll, privacy disclosure and the readability shortcut.
- Final release build and formatting/diff checks passed. Existing warnings remain.
- Final executable passed all 25 destinations, five App settings subpages, setup
  review and readability search in isolated native COSMIC at 820×600, 1280×900,
  and compositor-verified 150% (1230×900 physical pixels). **96 captures** retained.
- Repeated launch forwarding, search and preview toggles passed; preference bytes
  were unchanged and all fixture processes stopped. The 54 pre-existing tracked
  diff sections outside this polish remain identical to the turn-start baseline.

Evidence: [screen review](../.task-settings-polish-evidence/REVIEW.md),
[Settings results](../.task-settings-polish-evidence/settings-validated.log),
[native results](../.task-settings-polish-evidence/native-gui-final-result.json),
[source and executable manifest](../.task-settings-polish-evidence/validation-manifest.json).
The release-session obligations above remain separate; this pass did not install
or publish the build.


## Visual comfort refinement — 2026-10-25

The user found the first polished design too cramped and visually weak. The
follow-up changes composition rather than only spacing: persistent sidebar search,
roomier accent-selected navigation, larger headings, a single navigation layer,
unboxed preview scenes, and a scene beside the color inspector in wide windows.
Compact windows bound the scene height. The theme chooser stays above it; optional
scene shortcuts remain keyboard-accessible. Notes uses a softer native-themed
editor, finish choices avoid double framing, and setup tiles have clearer labels.

Large wallpaper scenes now use bounded 1280×720 decoding in the existing background
worker and four-handle cache; gallery thumbnails remain 320×180. Source freshness,
decode limits and existing commit/recovery behavior are preserved. See
[performance details](PERFORMANCE.md#large-settings-scenes--2026-10-25).

Final validation: **376 Settings tests and 11 media tests passed**, with two existing
opt-in Settings probes ignored. The release executable passed all 25 destinations,
five maintenance subpages, setup review and readability search in native COSMIC at
820×600, 1280×900 and compositor-verified 150%. All three runs preserved preferences,
forwarded repeated launches and exited cleanly. 96 captures are retained.

Evidence: [review](../.task-settings-comfort-evidence/REVIEW.md),
[native results](../.task-settings-comfort-evidence/native-gui-final-result.json),
[manifest](../.task-settings-comfort-evidence/validation-manifest.json).
The installed application is unchanged; existing release obligations remain.


## Purpose-led Setups — 2026-10-25

Setups now start with six distinct compositions: Everyday, Deep work, Writing
desk, Studio, Slow morning and After hours. Their existing IDs remain stable for
schedules, but choosing one now intentionally replaces the visible widget
composition and portable positions. Users can exclude Widget layout to keep their
current arrangement. Personal content, configured project folders, calendar
accounts, applets and physical display assignments remain outside the recipes.

The Explore collection uses two large cards per row, each rendered from the
fitted recipe over its bundled wallpaper. Your setups contains capture/import,
rich library entries and automatic-switching controls; schedules also retain the
persistent destination tab. Selecting a card opens a dedicated review with one
large scene, This setup / Your desktop comparison, included domains and persistent
Apply. Back cancels the local selection. Scene content is illustrative and
non-interactive, so clicking a widget cannot unexpectedly discard a setup.

The personal library retains duplicate, rename, export, delete, scoped Apply,
Undo and recovery. Existing compatibility adaptation remains in the planner.
Implementation and validation evidence: `.task-setups-rework-evidence/REVIEW.md`.

## Islands compositions in Setups — 2026-10-25

Each curated setup now carries an explicit portable panel arrangement:

| Setup | Islands composition |
| --- | --- |
| Everyday | Three groups balanced across the panel |
| Deep work | Compact center group, clock section first |
| Writing desk | Three inset groups and softer square corners |
| Studio | Workspace and clock groups toward the left, status group right |
| Slow morning | Looser central group with generous gaps |
| After hours | Compact group tucked toward the right |

These arrange the user's existing applet groups without adding, deleting, or
moving applets between groups. Applying a setup retains the current Islands
enablement and custom per-display positions. Excluding Panel retains its entire
configuration. The composition applies to the default layout, with existing
per-display overrides taking precedence. No physical output IDs enter a recipe.

Explore cards draw the portable composition through the shared Islands renderer.
The selected review uses the planner's candidate and the actual preview display,
including its retained override. Installed-but-disabled and absent installations
show the standard panel and explanatory copy. A disabled installed add-on can
store the arrangement for later explicit enablement. Setup scenes are read-only;
the Dock editor retains its interactive Islands selection controls.

Validation and native screenshots: `.task-setups-islands-evidence/REVIEW.md`.
