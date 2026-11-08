## Idle CPU with the island in the panel — 2026-11-08

Halcyon 0.6.1 used about 90% of a core while doing nothing, as long as the
Activity island applet was in the panel. The island’s observers only run while
an applet is connected, and one of them fed itself. `quiet::policy` watches
`~/.config/cosmic` for Do Not Disturb with notify, whose inotify mask includes
`IN_OPEN`, and it took any event on the `do_not_disturb` key, or on a folder
above it, as a change. Each “change” read the key again through the desktop
store, which opens each of those folders and then the key, and asked logind
whether the session is locked (four or more system-bus calls). That read was
the next change, about 1,500 times a second.

Now only a creation, write, rename or removal counts, as in `watch::files`. A
Do Not Disturb change reads only the key, with no logind call, and a burst of
changes is coalesced over 25 ms. `activity_island::quiet::tests` runs the
policy loop’s file half against a real inotify watch: with the old filter it
wakes 50 times in a row (the test’s cap); now it wakes 0 times, and once for a
real write.

Measured on the Ryzen 7 5700G with a copy of the user’s configuration (Clock
and Focus on screen) in temporary XDG folders and a test client on the island
socket standing in for the applet; 60 s after a 15 s warm-up, from
`/proc/<pid>/task`:

| Build | CPU | Voluntary context switches |
|---|---:|---:|
| 0.6.1 (`/usr/bin/halcyon`) | 82.7% | 37,180/s |
| main at b27fc19, release | 93.7% | 44,142/s |
| main with the fix, release | 0.02% | 8.2/s |

`strace -f -c` on 0.6.1 over about 35 s counted 171k `openat`, 140k `sendmsg`,
410k `recvmsg` and 213k `futex`. Over 150 s the fixed build used 0.07% (0.09%
counting the workers it starts). `DESKTOP_CARDS_CONFIG` can’t show this bug:
in that preview mode the island’s observers are off.

Idle wakeups left with an island connected (fixed build, from `strace`):

| What | How often | |
|---|---|---|
| Clock depth worker (`--depth-worker`) | once a minute | Redraws the depth layer for the new time. By design. |
| Two `watch::read_only_files` fallbacks | every 30 s each | Both include the wallpaper picture, and each read all of it (615 KB here) and kept a copy to compare. Fixed: files over 256 KiB are now compared by size, modification time and inode, never read; smaller settings files still compare contents. |
| Lock safety net, bus observers’ fallback reads, camera rescan | every 30 s | One logind query, one read each of UPower, NetworkManager and PackageKit, one `/sys/class/video4linux` scan. |
| Network observer | about every 6 s here | Follows NetworkManager signals; reads stay 200 ms apart. |

---

## Windows that show nothing — 2026-11-08

Every open layer-shell window holds its own swapchain buffers and iced
renderer, whatever it draws. Halcyon used to keep a window for every widget
and move hidden ones to x = −10000, so a desktop with two widgets on screen
still held 12 windows. Now a window is open only while it shows something:

- **Hidden widgets** (turned off, combined into Glance, `widgets_visible` off,
  or a profile or schedule that hides them) have no window. Showing one
  creates its window again, in its saved place; its data, position and timers
  live in the app and are untouched (a running focus round keeps running).
- **Music** has no window while nothing is playing; **Notes** and **Focus**
  have none while off. Before, each kept a 16×16 window off screen.
- **The fixed columns** open only the columns that hold a card. Devbox and
  Projects are drawn in the right-hand column and never had content in their
  own windows there.
- **The edge menu** has a window only while it is on screen, sliding, or
  holding a drag. It opens when the reveal starts (after the one-second hover
  on the edge), not when the edge is touched.
- **The arrange surface** covers the display only while arranging. Otherwise
  it is a strip one pixel tall along the top that measures the usable width.
  The edge strip, two pixels wide and already open, measures the usable height.
  Before, this was an invisible 1920×1080 window.
- **Other displays with widgets** are measured by two `halcyon-display-probe`
  strips each, a pixel thick, with an empty input region and no keyboard: one
  along the top that the compositor sizes to the usable width, one along the
  left sized to the usable height. Before, each such display had an invisible
  window its own size.

Two windows stay open for a real reason: the edge strip, which has to take the
pointer to reveal the edge menu and which measures the height, and the
one-pixel arrange strip, which tracks panels and docks that change the usable
area. Each other display with widgets adds its two strips.

**Why two strips per display.** A compositor chooses a layer surface's length
only along an axis where the surface is anchored to both edges, so a single
surface that reports both the usable width and height has to cover the whole
usable area. Each strip is anchored to both ends of one axis with an exclusive
zone of 0, which places it inside what panels and docks leave free: docks on
the left or right shorten the top strip, panels or docks on the top or bottom
shorten the left one. Widgets on that display wait until both are sized, as
they waited for the full-size window, and the measurement feeds placement the
same way. A strip configured again at the same length no longer rebuilds the
widget windows; before, every configure of the full-size window did.

**Arranging another display** makes it the display Halcyon is drawn on: its
arrange surface covers it and its edge strip opens there, and the display
being left (if it keeps widgets) gets the two strips instead. A probe itself
never needs to cover a display.

Not measured on hardware: the workstation used for these numbers has one
display connected. `src/app/surface_lifecycle_tests.rs` replays the windows a
second display opens and closes, including Arrange there and unplugging it.

Measured on the same Ryzen 7 5700G with the same user configuration (Clock and
Focus on screen, everything else off), read from `/proc/<pid>/fdinfo` 40 s
after start:

| | Before | After |
|---|---|---|
| Layer-shell windows open | 12 | 4 (arrange, edge, clock, focus) |
| `drm-total-vram` | 87,436 KiB | 43,112 KiB |
| `drm-resident-vram` | 87,436 KiB | 43,112 KiB |
| `drm-resident-gtt` | 11,800 KiB | 10,456 KiB |

Showing a hidden card now creates its window, and its first frame arrives
about 45 ms later (measured at startup), against about one frame for the old
margin change. Tools already worked this way.

---

## Video memory per window — 2026-11-08

Halcyon draws each desktop widget in its own layer-shell window, and each
window gets its own iced renderer. Each renderer's image atlas
created its full 2048×2048 RGBA texture (16 MiB) as soon as it existed, even for
windows that never draw an image. On integrated GPUs with a 512 MiB carve-out
this filled most of the dedicated video memory and pushed the rest into GTT.

The fix, in Halcyon's iced patch (`profilaz3/iced`, branch `lazy-image-atlas`,
offered upstream to `pop-os/iced`), starts each atlas as a 1×1 placeholder and
creates the full texture on its first image upload. Nothing else changes.

Measured on a Ryzen 7 5700G (Radeon Vega, Cezanne), both builds with the same user
configuration and 12 layer-shell windows open, read from
`/proc/<pid>/fdinfo` 40 s after start.

| amdgpu counter | Before | After |
|---|---|---|
| `drm-total-vram` | 382,340 KiB (373 MiB) | 87,436 KiB (85 MiB) |
| `drm-resident-vram` | 185,732 KiB | 87,436 KiB |
| `drm-resident-gtt` | 207,064 KiB | 10,440 KiB |

Windows that do draw images (wallpapers, album art) still allocate their atlas on
first use, so their cost is unchanged.

---

## Large Settings scenes — 2026-10-25

The visual comfort pass uses a separate 1280×720 bound for full-width wallpaper
scenes. Gallery thumbnails remain 320×180. Scene decoding still runs in the
existing background job and shares the four-entry stable-handle cache: at most
about 14 MiB of retained CPU pixel data, plus renderer-managed GPU resources.
Source-version checks and stale-result rejection are unchanged. There is no
image decoding in view construction and no new polling subscription.

The same PNG/JPEG, file-byte, image-side and allocation limits govern both paths.
Bundled scenes now read the installed image rather than enlarging a compiled
thumbnail; the isolated validation fixture supplies repository assets under its
own prefix. Small source images are not enlarged by the decoder. See the
[visual comfort review](../.task-settings-comfort-evidence/REVIEW.md) for validation.
This is a resolution/memory tradeoff, not a new frame-time or battery claim.

## Settings renderer — 2026-10-24

The 0.2.5 build explicitly selected libcosmic features without `wgpu`, leaving
the application's renderer CPU-only. On a dual-display laptop (AMD integrated
graphics and RTX 3050 Mobile; 100% external display and 150% internal display),
the installed Settings process had no graphics-driver mappings or GPU handles.
A user-driven 30-second scrolling/navigation sample used approximately 110% of
one CPU core. After interaction stopped, an eight-second sample showed no CPU
counter increase. These are process CPU samples, not frame timings.

Enabled the pinned toolkit's `wgpu` feature, with its normal adapter selection.
No new dependency versions, driver overrides or native backdrop effects were
introduced. The navigation/preview fixes below are included in both comparison
builds, so their results do not account for the renderer difference.

Native laptop comparison, same 1100×700 fixture window and scripted sequence:

| Measurement | CPU renderer | GPU renderer |
| --- | ---: | ---: |
| Four Dock/Panel switches, process CPU time | 0.93 s | 0.22 s |
| Eight scroll-and-capture operations, process CPU time | 2.16 s | 0.22 s |
| Four-second idle sample in capture mode | 0.50% of one core | 0.50% of one core |

The GPU process opened `renderD128`, with `amdgpu` DRM counters showing actual
graphics-engine work. It did not load the NVIDIA driver. Both processes exited
normally; their Panel captures were inspected for text, icons, preview and
scroll layout. Capture mode includes file commands, screenshot readback and a
one-second diagnostic tick: the numbers above are not input latency, FPS or a
battery-life estimate. The native window comparison was captured at 100% scale;
subjective mixed-scale interaction is a separate check.

An independent optimized, offscreen widget-tree probe on the development machine
also exercised real wheel events, drawing and screenshot readback (30 cycles):

| Page | CPU renderer | GPU renderer |
| --- | ---: | ---: |
| Colors & theme | 281.35 ms | 53.70 ms |
| App settings | 235.96 ms | 31.35 ms |
| Dock & panel | 281.20 ms | 45.81 ms |

Run this explicit diagnostic with `cargo test --release --locked --bin halcyon
settings::wizard_tests::rendering_probe -- --ignored --nocapture`. It requires a
working GPU adapter; it is deliberately outside normal headless CI. Existing
layout tests still explicitly use tiny-skia. Seven Welcome/navigation/queue tests,
four material tests, the shared-theme widget rendering test and the GPU probe
passed. Release build, formatting and whitespace checks passed.

See [the rendering decision](adr/0004-hardware-rendering.md). The temporary laptop
build uses isolated preferences; it does not replace the installed package.
GPU memory, long-running widget use and broader hardware coverage are not
established by these short Settings tests.

---

Desktop Cards performance audit — 2026-10-17

Environment: Pop!_OS 24.04 / COSMIC Wayland, Ryzen 5700G, 32 GB RAM,
1920x1080 at 100% scaling. Baseline: installed unoptimized development binary.
Revised: optimized release binary, the same preferences and data.

Observed bottlenecks and changes

- The whole native renderer was unoptimized. Install release builds from now on.
- Settings reloaded preferences and rebuilt its view every second while idle.
  Directory notifications now catch atomic saves and commands, coalesced for 25 ms.
  A 30-second background contents check recovers missed events and emits nothing
  when unchanged. Capture-mode timers remain opt-in development features.
- Appearance used full 1920x1080 wallpapers for all four preview images. The
  software renderer copies images to round their corners during drawing. Bundled
  960x540 and 320x180 previews, with stable handles, reduce this pixel processing
  and remove per-view path checks. Actual desktop wallpapers retain their resolution.
- Mouse movement over ordinary cards caused application updates across all five
  surfaces. Global pointer handling now filters for the sidebar/edge and active
  drag releases. Native widget clicks and hover handling remain with the toolkit.
- Hidden widgets launched project scans, calendar/weather requests and system
  samples. These jobs now pause while disabled and resume when due after enabling.
- Settings service checks run only on General, every 30 seconds or after actions.
  Project scans and external network/service commands already run off the UI thread.

Measurements

Warm Appearance page, 12-second samples:
  Before: 1.75% CPU of one core, 80.95 MiB RSS.
  After: no measurable CPU at the /proc counter resolution, 35.51 MiB RSS.
  A separate 15-second baseline sample was 1.53% CPU and 81.66 MiB RSS.

Six-page settings navigation (Widgets, Calendar, Appearance, Weather, Calendar,
Appearance), each followed by a two-second settling period:
  CPU time: 7.96 s before, 0.39 s after (95.1% lower in this check).
  File-command consumption: 51–659 ms before, 31 ms after.
  The latter measures receipt of a page/focus command, not click-to-paint latency.

Desktop widget process, 15-second samples, Clock/Calendar/Weather/Devbox enabled:
  Before: 1.20% of one core, 58.69 MiB RSS.
  After: 0.13% of one core, 36.05 MiB RSS (native capture mode enabled).

Project scan: 70 ms wall time, five checkouts, no errors. No scan redesign needed.
Settings Calendar page: no measurable CPU in the revised 12-second idle sample.

Method: cumulative user+system CPU counters and RSS from /proc/PID/stat.
These are short local samples, not a frame-rate benchmark or performance guarantee.
The desktop remained in normal use; initial compilation overlapped some baseline
work. External weather/calendar response times were not benchmarked. The new file
watcher also makes preference changes propagate immediately instead of waiting
for the next three-second widget tick.

Validation

30 Rust and 8 Python checks passed. New tests exercise actual filesystem atomic
replacement, unchanged-write suppression, hidden-widget job suspension and resume,
and prevention of duplicate in-flight refreshes. Existing calendar, sidebar drag,
layout reconciliation, settings, controller and integration tests still pass.
All six native settings pages were captured. Today-only and Today+Tomorrow changes
propagated to both windows; the agenda stayed centered as its height changed.
Selected calendar dates now use a subtle neutral palette tint and readable light
text, with a slightly stronger outline for the current day.

Raw audit artifacts: /tmp/desktop-cards-performance/


Compact cards, refresh scheduling and clock reliability — 2026-10-17

Clock disappearance was a stacking problem, not a process crash. Calendar's
full-height COSMIC backdrop covered the earlier Clock surface, including the
visually empty area above the calendar card. Creating the decorative Clock last
in the same bottom layer fixes this while retaining click-through input and
keeping ordinary application windows above it. Verified on the actual compositor
across three fresh preview starts and the installed service, with Calendar enabled.

Weather now uses a city/time header and no separate update footer. Devbox removes
its title row and places uptime in its status footer. Their combined height falls
by 52 pixels. Calendar's footer shows the account, time and refresh icon. Full
native captures with live weather and calendar data show no content clipping.

Each widget has an independently configurable interval in its Settings page:
Devbox 3 s, Clock 60 s, Projects 30 s, Calendar 5 min, Weather 15 min by default.
Clock ticks align to wall-clock boundaries; meeting highlights update locally
without requesting Google data every minute. Hidden widgets pause their jobs.

The desktop now sleeps until the next actual deadline instead of running every
job from a shared three-second tick. Settings stay in memory between file-change
notifications, and invalid reloads preserve the last valid configuration. Refresh
settings patch only their own fields, preserving unrelated preferences.

System sampling runs off the UI thread. The CPU sensor path and HTTP client are
reused. T3 service checks run separately, at most every 15 seconds, with a timeout.
In-flight jobs cannot duplicate themselves; obsolete project and calendar results
are discarded. Weather retains a location-checked atomic disk cache across starts.
The installed service encountered an Open-Meteo 503 during validation: the saved
forecast stayed visible with an amber timestamp indicator, rather than going blank.

Validation: cargo fmt --check, 38 Rust tests, and 8 Python integration checks pass.
All eight native Settings pages were captured. The release binary was installed
atomically; its running executable hash matches the tested build. User preferences
were preserved and the service reported zero automatic restarts after installation.

Steady desktop samples using /proc/PID/stat, Clock/Calendar/Weather/Devbox enabled:
  Previous release, 58.50 s: 0.120% of one CPU core, 36.74 MiB RSS.
  Revised release, 60.46 s: 0.149% of one CPU core, 36.70 MiB RSS.
Resource use remains essentially unchanged in these short samples. These results
do not establish a CPU reduction; the changes reduce repeated work and separate
background jobs from UI interaction while retaining a similar memory footprint.

Raw audit artifacts: /tmp/desktop-cards-compact-audit/


Settings and appearance audit — 2026-10-17 (evening)

Three parallel reviews covered preferences/controller persistence, all widget
settings pages, and runtime reliability. Appearance now includes a clickable
layout preview, coordinated native controls, Soft/Solid/Minimal presets, opacity,
corner radius, outline strength, and independent wallpaper matching. Clock gains
12/24-hour format and an optional date. Widget pages gain visibility controls,
wrapping refresh choices, a per-widget default reset, and clearer paused states.

Concrete fixes:
- Rust and Python share config.json.lock around read/patch/atomic-replace, so
  concurrent settings edits preserve each other's changes. Nested fields and
  unknown future keys remain intact. Invalid files are not silently replaced.
- Failed theme writes roll back wallpaper, wallpaper collection and preferences
  when the affected files remain writable. Original wallpaper backups are kept.
- Sliders save keyboard/wheel edits as well as mouse releases. A 350 ms one-shot
  debounce coalesces changes; page navigation and both native close paths flush
  drafts. Obsolete debounce generations cannot save over a newer edit.
- Hidden sidebar hover/dismissal delays use one-shot timers; 60 fps animation
  runs only during movement. Project-folder changes clear old rows immediately.
- General controls wait for service status; duplicate city-search submits and
  folder chooser requests are ignored. Small-window navigation remains reachable.

Validation: cargo fmt --check; 45 Rust tests and 14 Python tests pass. Native
settings pages captured at 1020x720 and 820x600; lower controls remain reachable
by scrolling. Appearance verified in all three palettes, neutral-background
mode, and with no widgets selected. Native desktop captures verified opacity,
0/32 px corners, outlines, 12-hour time and hidden date across all card surfaces.

The optimized release was installed atomically and Settings reopened on Appearance.
The running executable hash matches the build; existing preferences were preserved
byte for byte. Desktop and Settings services reported zero automatic restarts.
Restore point: ~/.local/share/cosmic-desktop-cards/backups/20261017-202335-settings-appearance
Raw audit artifacts: /tmp/desktop-cards-appearance-audit/

Observed 40-second sample after installation (all five desktop widgets enabled):
  Desktop: 0.150% of one CPU core, 36.71 MiB RSS.
  Open Appearance settings: 0.125% of one CPU core, 37.10 MiB RSS.
These short observations overlapped normal desktop activity and screenshot
capture. They are not a performance guarantee or a controlled speedup benchmark.
A final appearance-only spacing adjustment followed this sample.


Settings usability and freely positioned widgets — 2026-10-17 (21:20)

Appearance now uses compact palette swatches and a concise style preview. All
primary Appearance, Widgets and Devbox controls fit the 820x600 minimum window.
Nine settings pages were captured at both 820x600 and 1020x720. Calendar overview
switches and the desktop Today/Tomorrow agenda use vertical rows. Desktop cards
have no hover tooltips. Weather search debounces input, labels region/country,
and invalidates obsolete responses; the live geocoder returned Zagreb, Croatia.

Arrange on desktop provides a fullscreen drag editor with Save, Cancel, Reset,
keyboard nudging and normalized persisted positions. Settings also offers drag
reordering of the default right column. Native pointer/keyboard checks verified
movement, Save, Escape cancellation, an unchanged save round trip, an unrelated
settings edit without position loss, and restoration of the default layout.
Independent review caught captured Enter handling and cancellation edge cases.

Native validation found the pinned libcosmic layer Size action synthesizes a
square Configure by using width for both dimensions. Initial size zero correctly
means compositor stretching, but resizing an existing surface to zero can also
create an invalid viewport. The app now takes output measurements only from its
fullscreen probe, avoids Size actions entirely, and recreates card surfaces only
when actual size/layout mode changes. Position-only changes use margins. The
Clock is recreated last to retain its proper desktop stacking. Arrangement hides
old stretched surfaces without changing their required opposing anchors.

Devbox has independent CPU, memory, disk, network, GPU, uptime and service switches.
Network uses real elapsed-time deltas and physical active interfaces. AMD GPU
utilization, VRAM and temperature were verified on this machine. NVIDIA parsing
and unavailable-device behavior were tested; no NVIDIA device was available for
a hardware check. The all-sections-off state was captured and settings restored.
Projects group linked worktrees by repository and show human titles, concise Git
state and server links without generated paths. The local five checkouts now
render as one Voice Agent project.

Validation: cargo fmt --check, 81 Rust tests and 20 Python tests pass. Native
captures show full Weather/Devbox/Projects cards and a compact vertical Calendar
in default and custom positions. The Pop!_OS page opens supported COSMIC system
settings for appearance, wallpaper, dock, panel, workspaces, tiling, keys and
screens. This is a local installation; public account onboarding and distribution
packaging remain future work.

Raw audit artifacts: /tmp/desktop-cards-production-audit/
Restore point: ~/.local/share/cosmic-desktop-cards/backups/20261017-production-205358

The release, controller, project scanner and source were installed. The running
binary SHA256 matches the verified build; desktop and Settings services are active
with zero automatic restarts. Preferences were preserved byte for byte. Settings
was reopened on Appearance. Desktop RSS after startup was 58.5 MiB; this is a
single observation, not a benchmark against the earlier release.


Grid editor, Devbox consistency and coordinated themes — 2026-10-17

Devbox uses six embedded 16-pixel outline icons with consistent stroke weight.
Every metric has the same header/indicator spacing; Network and Uptime use neutral
rules instead of invented utilization bars. All enabled sections with one GPU
fit a 281-pixel card.

Settings Layout uses the desktop's actual geometry in an aspect-correct miniature.
Both editors share Spacious/Balanced/Fine snapping and keyboard step sizes. The
right/bottom inset boundaries compete with regular grid points, so an incomplete
final cell no longer leaves a large gap. Layout saves merge only edited positions
and grid fields; Reset explicitly covers visible cards and preserves hidden ones.
A native refresh previously cleared the pointer before a press; preserving it fixes
interrupted drags. Save errors remain visible through idle pointer/release events.

Six palettes have all twelve 4K/1080p wallpapers bundled offline. The installer
validates PNG data and preserves identical installed copies; theme selection repairs
missing or damaged wallpaper copies from the installed source bundle. The two
Appearance switches independently match wallpapers and COSMIC desktop accents.
COSMIC matching is opt-in, updates both modes using the pinned official builder,
preserves unrelated customization, keeps a first-apply backup and rolls back failures.

Native checks at 820x600 verified Appearance controls fit, uniform Devbox metrics,
miniature drag/save propagation to the desktop, and right/bottom edge placement.
A saved Weather move reached [726,540] on the exact shared grid without moving the
other cards. The final edge drag reached the inset boundary and cancellation left
the saved user layout unchanged. Later user changes to preview themes and layout
are preserved for installation.

All six COSMIC palette changes were applied to isolated copies of the host's actual
theme files. Mode, spacing, backgrounds and unrelated settings were preserved; a
repeated apply was stable, the first backup stayed unchanged, preview writes were
refused, and the real host configuration remained untouched during those checks.

Validation: 117 Rust tests, 37 Python checks, formatting and optimized release build.
New tests cover edge geometry, reset/concurrent saves, failed-save feedback, default
and sparse COSMIC files, exact rollback, opt-in isolation, and offline wallpaper repair.
Audit artifacts: /tmp/desktop-cards-grid-audit/
Restore point: ~/.local/share/cosmic-desktop-cards/backups/20261017-213006-grid-themes

The verified release, controller, full source and all12 wallpapers were installed.
Desktop and Settings are active with zero automatic restarts; the running binary
SHA256 matches the tested build. The user's saved preview layout and appearance
were merged into the installed preferences. A subsequent live Ocean palette
selection applied ocean-4k.png successfully. Settings was reopened on Appearance.


Settings and welcome responsiveness — 2026-10-24 (local 0.3.0 work)

The welcome wizard's dock/panel preview kept an unbounded crop cache keyed by
window aspect ratio. With no chosen look it decoded the same 1600x1000 JPEG for
each new ratio on the UI thread. It now reuses decoded wallpaper pixels and
keeps only the most recent crop. The pinned renderer leaks Cover images beyond
the preview even with clipping enabled, so a correctly sized crop is retained.
Resizing no longer decodes JPEGs or accumulates textures for historical sizes.
Welcome and later-step backdrops are decoded independently, and stock-bar look
tiles check bundled asset presence without decoding the unused Islands screenshot.
The first display of a required JPEG still performs a one-time decode; this is
not an asynchronous asset-loading implementation.

While a native settings worker was busy, every intermediate dock/panel slider
value was queued. Consecutive samples of the same unfinished slider now keep
only the newest value. Releases, bar switches, discrete choices and Undo remain
ordered barriers; Undo retains the identity shown when it was requested.

The isolated `settings::wizard_tests::responsiveness_probe` uses the actual view
and software renderer, in an unoptimized test build. For 80 distinct resize view
builds the measured time fell from 33,472.970 ms to 7.761 ms after the final
crop correction. First-display decoding remains; ordinary warm page-view timings
were largely unchanged. The rejected Cover implementation and its screenshots
are retained separately and are not the final result.
These diagnose redundant CPU work, not GPU frame rates, click-to-paint latency,
or confirmation of the cause on a reporter's hardware. No installed app was
replaced or user desktop preferences changed.

Run the explicit diagnostic separately from normal tests:

    cargo test --locked --offline --bin halcyon settings::wizard_tests::responsiveness_probe -- --ignored --nocapture

The reporting worker also has tests for short daily sessions: activity after
five minutes, duplicate suppression after restart, and next-day delivery with
the same identity. Failed requests keep the six-hour retry deadline on disk.
Offline days are not backfilled, and collector deployment/authorization is a
separate requirement from correct client scheduling.

Dock/Panel tabs now switch the loaded draft immediately when no Arrange lease
is active, rather than entering the native worker and running compatibility
queries. Live leases and already queued writes remain serialized. Measured
geometry from the previously selected bar is discarded; explicit Check/Arrange
obtains fresh geometry. A worker status read uses one fresh installation
observation and one bounded read-only IPC connection for capabilities and
geometry. Write/activation validation remains separate. Draft-only worker
completions no longer replan every previously opened settings page.

The resolved dependency feature tree enables `iced_tiny_skia`, not `iced_wgpu`:
this app currently uses software rendering. Scroll diagnostics must therefore
measure optimized drawing as well as message handlers. The diagnostic's
`software_scroll_*` samples include widget-tree wheel handling, CPU rendering
and screenshot readback, so they are not compositor presentation/frame timings.
No GPU backend change is part of this patch.

An optimized diagnostic at 1280x800 measured ten wheel-event/draw/readback cycles
in 103.301 ms for Themes, 87.457 ms for General, and 100.199 ms for Dock & panel.
These are about 9–10 ms per cycle on this machine, not evidence of the reporter's
hardware behaving identically. The optimized measurement preceded the final
retention fix for a freshly observed Islands installation; drawing code was
unchanged by that fix. Final debug regressions cover that correction as well.

Validation: 62 distinct focused regressions and the explicit diagnostic passed;
formatting and whitespace checks passed. Welcome steps were captured at 820x600
and 1280x800 and the corrected preview clipping was inspected. Current source is
local development work, not an installation, published build or collector update.
