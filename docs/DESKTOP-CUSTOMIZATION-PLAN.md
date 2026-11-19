# Desktop customization tracker

Reviewed on 2026-11-19. All five features are implemented. The table links to captures of the native controls. The research and original acceptance criteria follow the implementation notes.

| Feature | Where to find it | What the improvement pass verified |
| --- | --- | --- |
| Reusable glass finishes | Any widget’s settings → Glass finish → Choose… | Link, rename, copy, remove and Undo preserve the intended assignments. Mixed finishes render in the same wallpaper pass. [Finish editor](screenshots/desktop-customization/glass-library.png) |
| Widget Peek | Edge menu editor → Actions → Peek widgets | Selected cards work over an open window. The same shortcut, Escape and an outside click dismiss Peek. [Peek over a text editor](screenshots/desktop-customization/widget-peek.png) |
| Folder | Widgets → Folder → Choose folder | Compact bounds show whole file rows. Wide bounds switch to tiles and image previews. [Compact](screenshots/desktop-customization/folder-compact.png) · [Wide](screenshots/desktop-customization/folder-wide.png) |
| Dock magnification | Dock → Dock magnification → Apply | Four native favorites grew, shrank and launched from their original hit targets. The applet follows reduced motion. [Pointer and dock recording](screenshots/desktop-customization/dock-motion.mp4) |
| Setups that launch a workspace | Looks → select a saved Look → Choose apps… → Start workspace | The app list survives export/import. Starting applies the Look and requests apps in order. Unavailable entries are reported individually. [App picker](screenshots/desktop-customization/workspace-apps.png) |

## Findings and decisions

**Glass.** The reference image shows undisturbed wallpaper in the center and a reflection of the surroundings at the curved edge. The GNOME implementation samples wallpaper aligned with the window and includes scenery beyond the rim. Halcyon’s optional **Crystal** finish uses its existing wallpaper texture and a narrow refracting shoulder. The default has zero blur, 30% surface opacity and a subtle tint inherited from the theme. Its edge is static when the pointer moves. The editor previews the selected widget and the draft optics together. [Reference renderer](https://raw.githubusercontent.com/xnoviiq/MacOS-26-Gnome/main/extensions/liquidglass-widgets%40local/lib/glass.js) · [Dark wallpaper](screenshots/desktop-customization/crystal-dark.png) · [Bright wallpaper](screenshots/desktop-customization/crystal-light.png) · [Sunset](screenshots/desktop-customization/crystal-sunset.png)

**Peek.** A full-width tray hid too much of the current application. The tray now fits the selected cards and pages through larger selections. A live test opened Peek above a focused GTK editor, started the Focus timer, dismissed Peek with Escape and typed into the editor again. The same session completed 25 open/close cycles. Hidden cards can supply their data while Peek is open. Notes edits use the overlay’s keyboard focus.

**Folder.** A 10,000-entry local directory took 21 ms for one warm scan on the development machine at a six-entry capacity. The worker retains the newest entries that fit and releases its watcher when the widget and Peek are hidden. Wide tiles reuse bounded thumbnails and reserve the same space while decoding. Missing folders offer a fresh choice. Folder paths stay local when sharing a Look.

**Dock.** The icon under the pointer grows to 1.45× its resting size. Nearby icons grow less. Running indicators and click targets stay in place, and keyboard focus uses the desktop outline. In isolated 1280 × 800 COSMIC sessions with bottom and left docks, the Files icon measured 34 pixels wide at rest and about 50 on hover. The neighboring editor grew from 30 to 33 pixels. Files returned to 34 pixels on exit and held at 46 with reduced motion. All four launchers opened from their original targets on both docks. With software rendering, a separate traversal yielded 319 Wayland frame-callback intervals with an 8 ms median and a 9 ms p95. The settled applet used 0.00 CPU seconds over 60 seconds at process-counter resolution with animations enabled. Physical display presentation at 120 or 144 Hz still needs measurement.

**Workspace.** App selection and starting the workspace are separate actions. Saving, importing and scheduling a Look are passive with respect to its app list. An explicit start uses desktop entry IDs, checks for pending Look edits and continues past missing apps. The review found that explicit Look changes had been routed through the scheduled-action transaction. Manual workspace starts and edge-menu selections now record their own Undo entries. Undo restores the Look and leaves running applications alone. GIO launched two controlled desktop entries in the integration test. A Flatpak-style entry was covered by the launcher fixture.

## Review verdict

The visual and interaction review meets the 9/10 target for the glass editor, Peek, Folder and workspace picker on the tested desktop. The strongest evidence is the rendered light/dark comparison, focus returning to the editor after Peek, whole rows at both Folder sizes, and explicit launches surviving a missing app. Dock interaction meets the target in nested COSMIC. Its physical high-refresh pacing is still ungraded. Physical multi-monitor hot unplug and a real installed Flatpak’s window activation also remain outside the measurements above.

## Design review

Give each feature one obvious entry point and a reversible first action. A saved finish keeps its identity when renamed. Peek opens on the user's display and dismisses predictably. Folder picks up the system file associations and preserves the user's place across changes. Dock icons keep stable click targets through a reversal. Starting a workspace is a separate, explicit gesture.

Review each slice for visual fit, interaction and accessibility, failure recovery, and resource use. Record observed behavior, native evidence and outstanding measurements separately. A high score requires evidence in all four areas.

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

**Motion.** Scale the artwork around its center. Measure the pointer’s distance along the dock axis so the hovered icon reaches 1.45× and neighboring icons grow less as the distance increases. The falloff reaches zero with zero slope at its edge. When the pointer reverses, retarget from the currently drawn size so each icon can shrink smoothly as the next one grows.

**Resource and interaction test.** First draw three real neighboring icons with their running indicators. Check applet clipping, available padding and hit regions at the first and last icon. Keep a stable mapping from pointer position to launcher and avoid resizing the panel or reloading icons on animation frames. Use frame callbacks while scales are changing, stop at rest, and keep icons at their native size with reduced motion. Test slow sweeps, fast reversals, exit/re-entry, dragging, auto-hide, vertical docks and fractional scaling. Record frame intervals and misdirected clicks. If the host sees only a single applet surface, document the applet-side integration needed and its packaging cost before proceeding.

## Setups that launch a workspace

**Research.** PowerToys Workspaces pairs a saved arrangement with a launch action. Halcyon already saves Looks, discovers installed desktop entries and launches apps from edge actions. The first useful slice is opening a chosen set of apps with a Look. Treat exact window placement as a separate compositor experiment. [PowerToys Workspaces](https://learn.microsoft.com/en-us/windows/powertoys/workspaces) · [Installed apps](../src/desktop/apps.rs) · [Setup planning](../src/desktop/profiles/plan.rs)

**Plan.** Give a setup an optional list of installed app IDs and a distinct **Start workspace** action. For example, *Writing* applies its Look and opens the editor and reference manager. Show app icons in launch order and report an unavailable app inline. Previewing, importing or automatically applying a Look should leave its apps closed. Track appearance Undo separately so undoing a Look cannot close a document someone has started editing.

**First experiment and acceptance.** Resolve desktop entries on invocation and use normal desktop launch semantics. Test a native app, a Flatpak, an already-running app and a missing entry. Coalesce rapid double-clicks into one launch operation. Bound concurrent launches and release operation state on completion. Measure Halcyon's own overhead separately from time and memory consumed by the launched apps. Prototype activation and placement on a real COSMIC session before adding either to the UI. [Desktop launch semantics](https://docs.gtk.org/gio/method.AppInfo.launch.html)

## Evidence to collect before shipping each feature

Use the repository's [resource budgets](future-builder/BUDGETS.md) and [performance measurement guide](PERFORMANCE.md). The values above are prototype targets. Record hardware, renderer, resolution, scale, enabled widgets and sample duration alongside results.

- Compare the same release build with the feature off, active and settled. For a 60-second static sample, target zero feature-scheduled frames and measure added wakeups and CPU separately from regular widget refreshes.
- For motion, record actual frame intervals during interaction. Use the existing 60 Hz target of p95 at or below 20 ms. Check a higher-refresh display when available. Keep GPU timing and end-to-end presentation measurements separate.
- Record cold first use and retained memory after repeated use. Capture compact and wide layouts, a bright and a dark background, and reduced motion where applicable. For the dock, include video of entry, traversal, reversal and exit; still frames cannot establish smoothness.
