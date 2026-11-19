# Desktop customization tracker

Opened 2026-11-19. The five items below are requested features. Research and first-slice plans are recorded here. Implementation, native prototypes and performance measurements are pending for all five.

| Feature | Next decision or experiment | Prototype | Implementation | Verification |
| --- | --- | --- | --- | --- |
| Reusable glass finishes | Render two linked cards and one independent card on the same output | Pending | Pending | Pending |
| Widget Peek with a custom button | Open and dismiss a two-widget overlay from an editable button | Pending | Pending | Pending |
| Folder widget | Compare a small folder and a 10,000-entry folder at compact and wide sizes | Pending | Pending | Pending |
| Dock magnification | Establish who draws each icon and whether growing icons can draw beyond the applet bounds | Pending | Pending | Pending |
| Setups that launch a workspace | Launch an installed app once from an explicitly invoked setup | Pending | Pending | Pending |

Start with finishes. Peek and Folder can then use real saved finishes in their visual tests. Run the dock feasibility experiment early because it crosses the panel/applet boundary. For each row, record the prototype commit or PR, a native capture, the before/after measurements and any revised decision here before marking it verified.

## Reusable glass finishes

**Research.** Kustom's reusable components package both an appearance and the controls its author exposes. Halcyon already has a glass editor, theme-linked tints and personal widget recipes. The missing piece is naming a finish and referring to the same finish from several widgets. [Kustom components](https://docs.kustom.rocks/docs/general_information/groups_explained/) · [Glass editor](../src/settings/view/widget_glass.rs) · [Recipe library](../src/recipes/library.rs)

**Plan.** Add **Save finish…** to the editor and a **Finish** picker to each widget. Existing widgets start on **Use shared finish**. Use stable IDs so renaming *Evening Frost* keeps its links. Editing it should update all linked cards. **Save as copy** lets Music diverge. Show the affected widget count before deletion and offer a shared-finish fallback with Undo. Keep a theme tint as a reference so it follows the destination theme when a Look is imported.

**First experiment and acceptance.** Put Weather and Messages in Evening Frost and Music in Polished on one display. The current renderer shares pane parameters. Prototype a bounded per-pane finish record in the existing pass and reuse the scene's cached wallpaper mipmaps. Compare one finish with three mixed finishes at the same pane count. Check for extra wallpaper uploads, render targets and idle redraws. Capture light and dark wallpaper, resize each card and change the theme accent. Verify that text stays sharp, a custom tint stays fixed, and export/import and Undo preserve the links. [Pane synchronization](../src/app/rain.rs) · [Renderer](../src/rain/gpu.rs)

## Widget Peek with a custom button

**Research.** Fences can bring selected desktop content above open windows on demand. Halcyon's edge menu already supports customizable items and invokes desktop actions. This gives Peek a discoverable home even for people who never configure a shortcut. [Fences Peek](https://www.stardock.com/products/fences/) · [Edge actions](../src/app/edge.rs) · [Custom item storage](../src/config/edge_menu.rs)

**Plan.** Add a **Peek widgets** button to the edge-menu editor. Let people choose its icon, label, position in the menu and the widgets it reveals. A click opens the selected cards over the current app on the invoking display. A second click, Escape or an outside click dismisses them. Keep the button reachable while Peek is open. A configurable keyboard shortcut invokes the same action. Start with click-to-toggle so the feature also works with a mouse or keyboard that cannot report a held global shortcut.

**First experiment and acceptance.** Show Music and Calendar over a maximized editor. Preserve their familiar order and finishes, with a short opacity transition and an immediate reduced-motion path. Reuse current provider data and widget actions. Create overlay surfaces only while Peek is open. Test keyboard entry, playback interaction, focus restoration, two monitors, lock and display removal. After 25 open/close cycles, check that overlay surfaces disappear, provider subscriptions return to baseline and click targets on the underlying app work again. [Provider ownership](../src/app/providers.rs) · [Surface lifecycle tests](../src/app/surface_lifecycle_tests.rs)

## Folder widget

**Research.** Fences Folder Portals make a real directory accessible from the desktop. The freedesktop thumbnail standard explains how to detect stale previews. Halcyon already has a nonrecursive directory watcher that avoids waking on ordinary reads. [Folder Portals](https://www.stardock.com/products/fences/) · [Thumbnail validity](https://specifications.freedesktop.org/thumbnail/latest/modifications.html) · [Directory watcher](../src/watch/dir.rs)

**Plan.** Choose a local folder. Compact shows recent files as rows, wider sizes add thumbnail columns, and **Open folder** opens the file manager. Fit whole items to the available height with no internal scrolling. Keep labels and tile positions stable as a thumbnail replaces its placeholder. Include empty, unavailable and permission-denied states. A portable Look carries the design and asks the recipient to choose their own folder.

**First experiment and acceptance.** Enumerate off the UI thread, keep only the newest entries needed for the current view, and show file-type icons immediately. Read valid cached thumbnails for visible entries first. Start with at most two concurrent thumbnail reads and a 16 MiB decoded-image cache shared across folder views. Debounce bursts, cancel stale jobs when the folder changes, and rescan after watch overflow or resuming visibility. Test a 10,000-entry directory, rapid renames and a deleted folder. Record time to first usable rows, peak cache bytes and work after hiding. Compare compact and wide captures using long names and missing thumbnails.

## Dock magnification

**Research.** Nexus independently controls magnified size, the span of affected neighbors and transition smoothness. COSMIC exposes its app list as an applet. Halcyon's Islands companion customizes the panel host. Establish the icon-rendering boundary on the supported build before promising per-icon animation on stock COSMIC or Islands. [Nexus user guide](https://themes.winstep.net/help/Nx/nexus.htm) · [COSMIC applets](https://github.com/pop-os/cosmic-applets) · [Halcyon applet catalog](../src/desktop/applets.rs) · [Islands companion](../companion/README.md)

**Motion prototype.** Start at 1.18× peak size. Spread the lift across roughly two neighbors on either side, measuring distance to each icon's resting center. Use a smooth falloff that reaches zero with zero slope at the edge. Anchor growth toward the desktop so icons stay attached to the dock. As the pointer moves from one icon to the next, the first should shrink while the next grows. On fast reversals, retarget from the currently drawn scale. Try a 120–180 ms settle time and judge it from a native recording at normal speed.

**Resource and interaction test.** First draw three real neighboring icons with their running indicators. Check applet clipping, available padding and hit regions at the first and last icon. Keep a stable mapping from pointer position to launcher and avoid resizing the panel or reloading icons on animation frames. Use frame callbacks while scales are changing, stop at rest, and give reduced motion a static hover highlight. Test slow sweeps, fast reversals, exit/re-entry, dragging, auto-hide, vertical docks and fractional scaling. Record frame intervals and misdirected clicks. If the host sees only a single applet surface, document the applet-side integration needed and its packaging cost before proceeding.

## Setups that launch a workspace

**Research.** PowerToys Workspaces pairs a saved arrangement with a launch action. Halcyon already saves Looks, discovers installed desktop entries and launches apps from edge actions. The first useful slice is opening a chosen set of apps with a Look. Treat exact window placement as a separate compositor experiment. [PowerToys Workspaces](https://learn.microsoft.com/en-us/windows/powertoys/workspaces) · [Installed apps](../src/desktop/apps.rs) · [Setup planning](../src/desktop/profiles/plan.rs)

**Plan.** Give a setup an optional list of installed app IDs and a distinct **Start workspace** action. For example, *Writing* applies its Look and opens the editor and reference manager. Show app icons in launch order and report an unavailable app inline. Previewing, importing or automatically applying a Look should leave its apps closed. Track appearance Undo separately so undoing a Look cannot close a document someone has started editing.

**First experiment and acceptance.** Resolve desktop entries on invocation and use normal desktop launch semantics. Test a native app, a Flatpak, an already-running app and a missing entry. Coalesce rapid double-clicks into one launch operation. Bound concurrent launches and release operation state on completion. Measure Halcyon's own overhead separately from time and memory consumed by the launched apps. Prototype activation and placement on a real COSMIC session before adding either to the UI. [Desktop launch semantics](https://docs.gtk.org/gio/method.AppInfo.launch.html)

## Evidence to collect before shipping each feature

Use the repository's [resource budgets](future-builder/BUDGETS.md) and [performance measurement guide](PERFORMANCE.md). The values above are prototype targets. Record hardware, renderer, resolution, scale, enabled widgets and sample duration alongside results.

- Compare the same release build with the feature off, active and settled. For a 60-second static sample, target zero feature-scheduled frames and measure added wakeups and CPU separately from regular widget refreshes.
- For motion, record actual frame intervals during interaction. Use the existing 60 Hz target of p95 at or below 20 ms. Check a higher-refresh display when available. Keep GPU timing and end-to-end presentation measurements separate.
- Record cold first use and retained memory after repeated use. Capture compact and wide layouts, a bright and a dark background, and reduced motion where applicable. For the dock, include video of entry, traversal, reversal and exit; still frames cannot establish smoothness.
