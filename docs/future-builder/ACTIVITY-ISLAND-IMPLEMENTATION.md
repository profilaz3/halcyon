# Activity island — native implementation

## Control alignment follow-up — 2026-11-04

Centered the actual playback glyphs inside their unchanged 44px targets and the
collapse chevron inside its 28px target. Previously the native icon-button row
placed these 4px and 2px left of center respectively. Explicit 16px icon sizes
and equal insets fix the offset without increasing the capsule or controls.

66 focused tests passed. Rendered-pixel checks cover playing, paused and disabled
controls, light/dark themes, and 100/150/200% scale. Final-ELF private COSMIC
playback, chooser, dismissal, keyboard and mode checks passed; fixture cleanup
left no processes. Evidence: `.task-activity-alignment-20261104/delivery.json`.
Local only; previous qualification limits remain.

## Compact density follow-up — 2026-11-04

The active resting slot is now **220 px wide** (previously 328); quiet remains
112 px. Resting height follows the native COSMIC panel size: XS is 32 px, M is
56 px with zero outer padding. No saved or installed panel preference is changed.
The expanded view is **296 × 128 px on XS**, or **296 × 140 px on M**, with outer
panel padding added where configured. Previously the medium-panel view was
328 × 170 px. Playback hit targets remain 44 × 44 px.

Compact icon/title groups and the quiet label are centered. Native text layout
moves the identity continuously toward its expanded reading edge; no character
width estimates or halfway alignment snap. The shorter chooser scrolls to further
activities. Header cues remain visible through opening, and system percentages
are no longer duplicated below the timeline.

65 focused Rust regressions passed, including centered identity, continuous
movement, first-frame matching, five panel heights, padding, scale, scrolling and
control hit targets. Exact-ELF native XS and M runs passed playback, active-task
selection, keyboard/outside dismissal, quiet withdrawal and mode changes. Fixtures
were cleaned up. The comparison viewer caps captures at their native scale; the
previous viewer magnified a 420 px crop to 630 px.

Evidence: `.task-activity-density-20261104/delivery.json`. Local only. Earlier
qualification limits below remain open; this density change does not qualify a
released package or fix the stationary-hover edge case.


## Reactive native revision — 2026-11-04

Local implementation; acceptance is recorded in
`.task-activity-native-reactive-20261104/`. This section supersedes the earlier
first-version behavior below. The installed session and released packages are unchanged.

- Default selection: an urgent system incident, an approaching timed meeting, a
  running Focus session, playing media, then another live system activity. No idle
  timer, paused-player discovery or distant meeting is promoted.
- The quiet slot is a 112-pixel **Activities** capsule. An active slot is 328 pixels.
  Opening preserves the host allocation and outer edge. The medium-panel popup is
  328 × 170 logical pixels, including its panel-height inset.
- Media uses Previous, round Play/Pause and Next icons. Known duration enables a
  bounded timeline; live streams have no invented duration. A pause while using the
  controls keeps Resume available. Closing starts a ten-second grace period.
- One primary view and a count replace the permanent tabs. The chooser contains
  only eligible activities. Selection holds the exact Calendar occurrence or media
  player identity. Chooser rows are full-width native buttons. Busy privacy can
  still update immediately. Ended sources cannot
  put a replacement action beneath the pointer.
- The header chevron collapses the view. Clicking the title or using the chooser
  promotes a hover popup to a native grabbed popup; Escape and outside clicks can
  then dismiss it. Hover leaves keyboard focus with the current application. The
  replacement waits for native teardown; cancellation prevents a late reopen.
  Recreating the popup preserves its selected identity and motion. The companion
  now maps native keyboard focus to the exact applet popup instead of discarding
  that mapping; a late leave from a retired popup cannot clear its replacement.
- A microphone indicator remains in the title when another activity is primary.
  It describes physical input capture, not a call or screen recording.
- Panel → Content exposes System activities, Playing media, Upcoming meetings,
  Brief automatic reveals and a 5/10/15-minute reminder. All sources default on.
  Each preference is saved and undone independently. Standard Panel mode still
  hides the applet and disconnects its observers.

System sources are read-only and shared by the desktop process. UPower supplies
charging changes and battery warnings. PulseAudio-compatible PipeWire metadata
(`pactl`) supplies physical-input capture and changes to the default audio route.
NetworkManager supplies connection and VPN transitions. PackageKit supplies actual
software-operation phases and known progress. A completion/failure cue requires a
Finished signal for a transaction that this observer saw running; service loss is
not success. No installation or authorization operation is performed by the island.

Observers start only while there is a connected island and the source is enabled.
They use bounded queues, bounded reads and timeouts. Audio graph events coalesce.
Package observers set NoAutoStart. Reconnect discovery is silent, percentage
updates do not repeat a critical alert, and expired acknowledgements withdraw.

Automatic reveals last three seconds unless the user interacts. They are suppressed
by DND, inactive/locked login session, fullscreen, panel autohide, an existing popup,
unknown policy state and a thirty-second routine cooldown. Fullscreen currently
suppresses reveals across all outputs, conservatively. Suppressed events are not
replayed when quiet mode ends. Reveals never replace an existing urgent incident.

Limits: system adapters require the corresponding services; audio metadata needs
`pactl` and does not establish capture outside that graph. Network disconnections
are described neutrally; deliberate versus unexpected loss is not inferred.
PackageKit does not cover unrelated APT/DNF/Flatpak command-line operations. No
universal downloads, camera, screen-sharing, meeting attendance or join-link
detection is claimed. The upcoming-event projection withdraws from the chooser two
minutes after start; the owner reprojects the cached events once per minute while
a Calendar-enabled island is connected, so an ongoing meeting cannot hide the next
one until a provider refresh. Calendar remains available in Settings. Incident dismissal
beyond the current Calendar occurrence remains a later extension.

## Validation of this revision

- 64 focused Rust tests passed: 63 Activity/bridge/provider checks and the native
  Settings click/save/Undo check at 820×600 and 1280×900. Dark/light and scaled
  renderer captures cover controls, system states and panel geometry.
- The exact main ELF and newly rebuilt companion passed private COSMIC tests in
  both dark and light: Pause/Resume/Next, choosing Focus, hold/departure, outside
  click, actual Tab/Escape delivery, quiet width, Standard-mode hiding and Islands
  restoration with saved placement. All fixture children were reaped.
- The previous companion failed actual keyboard delivery. The updated bridge
  passed the same input check. Its patch applies with zero fuzz to the verified
  upstream archive and reproduces all 83 Rust files used in the build.
- Each three-second settled-idle sample recorded zero animation messages and zero
  measurable applet CPU ticks. This is a short software-rendered nested-compositor
  sample, not a hardware frame-time or battery qualification.
- Private UPower/PackageKit fixture tests cover discovery, transitions, owner loss
  and observed transaction completion. The fullscreen observer's initialization
  and cancellation test passed in the private compositor. Actual hardware event
  sequences and live fullscreen/lock/DND transitions remain unqualified here.

Evidence and hashes: `.task-activity-native-reactive-20261104/delivery.json`.
No host installation, logout or release was performed. The native focus fix needs
this new companion; an older installed companion does not gain it from an applet
binary update alone. Final-package qualification and release pin updates remain
part of release preparation.

One existing interaction limit remains: an unentered popup can close after five
seconds beneath a completely stationary pointer. Moving into the popup or holding
it explicitly avoids that path. Crowded-panel adaptation and physical-display
acceptance remain open. The earlier pinned-hover outside-click issue is fixed by
the new native grabbed-popup handoff and validated in both themes.

## Earlier native revisions

First native version implemented and validated locally for FB31. No release or
installed-session changes. The remaining feature slices stay open in the tracker.

## Rendering choice

The panel contains a normal COSMIC applet. It is visible only while its owning
Panel runs Islands mode. Standard mode and a stock-host fallback hide it, close any
popup and disconnect its activity feed. The applet placement remains saved, so
switching Islands back on restores it without adding another item.

On the top edge, a popup overlaps the
applet's own reserved slot. Hover opens without a keyboard grab; an explicit held
or keyboard opening can request native popup focus. The popup begins at the panel
island's outer edge and grows downward. Its title and short cue move with it;
details and controls appear inside the same shape. The resting title is hidden
under the moving popup and restored at the matching closed frame.

The 328-pixel applet slot uses the panel's configured applet height. Its popup
includes the panel's outer padding and preserves the top corner radius. For a
medium panel without extra padding, the shape grows from 328 × 56 to 328 × 130
logical pixels. Neither neighboring applets nor the panel resize during opening.
Other edges use a small icon and an ordinary anchored popup. Crowded-panel adaptive
width is a later slice, not an inferred capability.

The resting content is transparent: the panel supplies its shape, color and finish.
The expanded reading surface and controls follow the panel's native dark, light or
custom-color palette. The expanded surface is opaque; it does not claim native blur.

The spring is critically damped and solved using elapsed time. Retargeting preserves
position and velocity; refresh rate does not determine duration. Animation requests
end when it settles. Reduced motion uses the final geometry immediately. Controls
are inactive while hidden and acquire input only near the expanded state.

The pointer must rest for 150 ms before the capsule opens; motion can postpone that
to at most 600 ms, so crossing it does nothing. Departure has a 300 ms grace period.
Clicking the title holds the view open. An explicit **Pin / Pinned** control exposes
that state; Escape, a click on another window or releasing the pin closes it. A popup
opened by pressing the capsule has a native grab, so any outside click closes it. A
hover-opened popup has no grab, so a click on bare wallpaper does not reach it, and
staying pinned is the behavior there. Choosing an activity preserves
transient hover behavior. Keyboard focus keeps the controls open while in use. The
popup owns its ordinary rectangular input region. The current COSMIC panel proxy
loses rectangle offsets when forwarding a custom multi-rectangle region, so a
rounded input mask would incorrectly exclude the lower controls. The visual
outline remains rounded. Transient focus handoffs inside the popup do not close it.

Research informing this choice:

- [Apple Motion](https://developer.apple.com/design/human-interface-guidelines/motion):
  purposeful, interruptible feedback and reduced-motion behavior.
- [Understanding hitches](https://developer.apple.com/documentation/xcode/understanding-hitches-in-your-app):
  individual stalled frames matter, beyond average frame rate.
- [COSMIC applets](https://pop-os.github.io/libcosmic-book/panel-applets.html):
  native applet discovery, sizing and popup integration.
- [libcosmic's applet example](https://github.com/pop-os/libcosmic/blob/master/examples/applet/src/window.rs):
  popup lifecycle and native controls. Implementation uses the repository's pinned
  `dc11d6a`, not an unpinned upstream API.
- [Wayland protocol model](https://wayland.freedesktop.org/docs/book/Protocol.html):
  surface lifetime, compositor placement and input ownership.

## Ownership and data

The existing Halcyon desktop owns MPRIS, Focus persistence and Calendar connections.
It publishes a bounded projection on a private session socket. The applet has no
provider requests, account credentials, artwork fetcher or Focus persistence writer.

Connections verify the peer UID. The endpoint and lock are private, frames are
limited to 16 KiB, there are at most eight clients, and commands are bounded and
rate-limited. Commands contain the displayed activity's identity; the owner rejects
stale targets and checks transport capabilities. Reconnection creates a fresh
command queue. Calendar Busy mode is applied before data reaches the applet.

Unchanged snapshots do not wake clients. A settled Music capsule has no progress
timer. Focus uses its absolute deadline and a one-second text tick while visible;
Calendar uses minute/deadline updates. The hidden Music widget does not gain an
animation or artwork workload merely because the applet is present.

## Entry point

The application package supplies `io.github.profilaz3.Halcyon.Activity.desktop`.
Debian and RPM use the same staged application payload. Source installation also
installs the applet entry with the chosen prefix.

In **Dock & panel → Applets**, a top panel offers **Add Activity island**. The
existing guarded applet editor applies the change and keeps ordinary movement and
removal controls. Nothing is added to a user's panel without their action. The
applet can also be found in COSMIC's applet picker. Adding it to a Standard panel
keeps it dormant until Panel Islands is enabled. Dock Islands alone does not enable
Activity Island. Removing the applet remains an independent opt-out.

## First-version boundaries

- Music: title, artist, supported Previous/Play/Pause/Next controls.
- Focus: current shared session and Start/Pause/Resume.
- Calendar: next timed event, time/countdown, Busy privacy and Open Calendar.
- Hover and explicit access; stable named tabs for Music, Focus and Calendar.
  Switching does not replace the current view with a separate navigation screen.
- No automatic expanded alerts yet. They need the planned full-screen, DND,
  autohide and occurrence-dismissal contract. Compact priority can still change.
- No artwork block, progress scrubber or new account configuration.

## Local validation

The original native acceptance below is retained as the first-version baseline.
The subsequent visual revision has its own exact ELF, native results and captures
in `.task-activity-island-20261101/polish/`.

The visual revision shortens the expanded body from 128 to 100 logical pixels,
keeps three small activity tabs visible beside the current controls, and removes
the held-state perimeter outline. Labels appear on hover. The title reserves room
for the hold indicator, including when long text is truncated. Motion and provider
ownership are unchanged.


The accepted release-mode ELF has SHA256
`5c55019ea0fdc80d9756d07e832c559fe1f13d5fce921c1e5d99807fcf2fc727`.
The task evidence is in `.task-activity-island-20261101/` at the repository root.

- 52 focused tests passed: 14 Activity island/provider/IPC/native-control tests,
  13 existing Music tests, 14 panel-editor tests, seven Debian packaging tests and
  four source-installation tests.
- The exact ELF passed private nested COSMIC checks for hover, pointer transfer,
  departure, live Pause, hold/release, Escape, outside-click, shared Focus and
  reconnection after the desktop process restarted. All owned children were reaped.
- Native controls were checked at 1×, 1.5× and 2×. A recording and native panel
  screenshots are in `accepted-native/`; scaled control captures are in
  `native-controls-final/`.
- The settled capsule emitted no animation messages during a three-second idle
  sample. Process CPU did not advance at `/proc` tick resolution. This is a bounded
  idle observation, not a battery or frame-latency measurement.

The nested compositor used software rendering; the applet opened the AMD render
device. These checks and headless scaled captures do not qualify physical
multi-monitor hotplug, hardware presentation latency, a crowded panel or a complete
Fedora package installation. Companion-specific placement must be tested separately
before claiming identical behavior with every Islands layout. Automatic expanded
activity alerts remain a later slice.

### Visual revision acceptance

The revised ELF has SHA256 `d43202349171899262a90c67ff8914b7e21847700e0124125841100c4973f03a`.
All 15 focused Activity island tests passed, including a new native tab-switching
and immediate playback regression. Renewed native checks passed for hover, hold,
Escape, outside-click, live playback, shared Focus and owner restart. No animation
messages were emitted during the settled three-second sample.

Reviewed native control captures at 1×, 1.5× and 2×, including long-title truncation.
The final recording and exact source hashes are in the `polish/` evidence directory.
The earlier package/provider suites above were not repeated for this view-only
revision. No installed-session changes or release.

### Interaction and lifecycle hardening — 2026-11-03

The expanded capsule was 328 × 144 logical pixels in this revision. Stable, named Music,
Focus and Calendar choices fill the bottom row; activity actions have their own
space. Explicit Pin / Pinned replaces the small hold indicator. Changing activity
keeps the popup transient; its selection remains stable until it closes. Long
names ellipsize, and labels retain contrast in both native light and dark themes.

Calendar priority and content advance at the five-minute, start and end boundaries.
Expired events disappear even without another provider snapshot. Unavailable Music
and expired-but-not-yet-completed Focus disable their stale controls. Completed
Focus sessions name the next action. Offline views expose Settings and Start
Halcyon; repeated connection retries preserve pending startup and failure feedback.

Native COSMIC testing found that CursorLeft can arrive with the previous Available
cursor position. The custom surface now suppresses that stale position until
pointer reentry, across update, redraw and pointer interaction. This clears stuck
tooltips and prevents an outside release from activating an inside control.

Validation: **23 focused tests passed**, including IPC/privacy, animation,
interaction, stale input and rendered label contrast. Native controls were reviewed
at 1×, 1.5× and 2×; empty/error/completed/private states were reviewed in light and
dark themes. The final ELF passed private COSMIC checks for hover transfer,
pin/unpin, activity switching without implicit pinning, live Pause, shared Focus,
Escape, outside-click, owner restart and tooltip dismissal. All fixture children
were reaped. The three-second settled sample emitted zero animation messages and
showed no CPU advance at proc tick resolution.

Exact ELF: `9c71a73af2a773bc5b9b2aac055ff111d76b29b69e707a76865f70d06614b00f`.
Evidence, source hashes, captures and recording:
`.task-activity-production-20261103/delivery.json` and `native-final/`.

This was a local build, not an installation or release. The existing first-version
boundaries above still apply, including automatic alerts, crowded panels,
companion-specific acceptance, physical displays and final packaged-artifact checks.

### Visual and input review — 2026-11-03

Current geometry: compact 220 × 32, expanded 328 × 118, in the same 328-pixel slot.

- **Layout.** Details and actions share one band, removing the blank row. Title,
  pin, actions and activities share left and right edges. The activity choices are one
  quiet segmented control. Pin is a labeled chip at a fixed width, so toggling it never
  moves the title. Keyboard order now follows reading order.
- **Words.** One title and one cue travel with the capsule, so the first popup frame
  matches the panel capsule. *Playing* is gone because only the exception needs a word;
  Focus shows its phase and end time; Calendar shows *In 3 min* or *In 7 h* instead of
  hundreds of minutes. Event times follow the COSMIC clock's 12/24-hour setting.
- **State.** A running Focus session or started event shows elapsed progress beneath
  its details, on the existing clock tick and without a wire change. A choice with
  live activity keeps its accent while another is selected. Offline shows a neutral
  power icon, no selected or selectable activity, and names a start failure in the title.
- **Hierarchy.** One primary action per activity. Tooltips stay dark in every theme
  and wait 500 ms so they do not flash over neighbors.
- **Input.** A flick while the popup is mapping used to leave it open indefinitely,
  because the panel proxy sends that popup no pointer event. It now closes after 5 s.
  The same bound means a popup that has never seen the pointer closes after 5 s even if
  the pointer rests on it, until the next motion. Escape stays dismissed until the
  pointer leaves. A press on a closing capsule reopens and keeps it. Opening Settings
  dismisses the popup, and Settings finishing no longer erases a start failure.
- **Other edges.** These open complete, without growing from a slot they do not have.

No wire, provider or privacy changes; `bridge.rs`, `motion.rs` and `src/app/activity.rs`
are unchanged. The stale-cursor fence in the surface's input path is unchanged too.

Validation: **34 focused tests passed**, including first-frame pixel equality, shared
edges, no overlaps, clock formats and each input case above. The exact ELF passed the
private nested COSMIC probe in dark and light. Against the previous ELF, on the same
fixture: crossing no longer opens, a flick no longer sticks and motion after Escape no
longer reopens. Recorded motion was monotonic in both directions, about 200–267 ms
opening and 233 ms closing at 60 fps capture. Three settled idle seconds emitted zero
animation messages.

Exact ELF: `5545bda6b5264eb90f9e7a776fb89ac0383102802e5fc8fb68719fe2c4239dc1`.
Evidence: `.task-activity-opus-20261103/delivery.json`.

This is a local build, not an installation or release. Software rendering does not
qualify presentation latency. Crowded panels, companion-specific placement, physical
displays, packaged artifacts and automatic alerts remain unqualified or planned.


### Parent acceptance of the Opus review

Retained the visual revision after inspecting source, native captures and baseline
race evidence. Added one fix: Escape suppression also applies to pointer reentry
while the closing animation is active. An explicit press still reopens and pins.
The new regression covers the interval missed by the reduced-motion Escape test.

**35 focused tests passed.** Final ELF `2a27bb53b7f193c2c38e10d2bed6385ccf51a907b2a1fc3095aec5b8417a6eb9`
passed the private dark-theme COSMIC probe, including live playback, shared Focus,
Settings launch/dismissal, owner restart, explicit pin, window-click dismissal,
Escape persistence and tooltip cleanup. No fixture processes remained. Light-theme
native captures above belong to the Opus ELF; the parent change only gates Escape
input, and final control captures cover both themes.

The stationary-hover five-second timeout and hover-pinned bare-wallpaper dismissal
are still open interaction issues, not production-complete behavior. Existing
placement/device/package qualifications and automatic-alert scope remain open.

Evidence: `.task-activity-opus-20261103/parent-review/delivery.json`.
No installed-session changes or release.


### Panel Islands integration — 2026-11-03

Activity Island now follows the owning Panel's observed Islands mode. Standard mode
and a stock-host fallback hide it and close its popup; re-enabling Islands restores
the saved item. Removal remains an independent opt-out. Dock Islands alone does not
activate it. Settings explains why an item placed on a Standard panel is hidden.

The resting applet is unpainted, so its surface belongs to the panel. Expanded
controls, tooltips and text use the native panel palette; light and custom-colored
panels get readable activity accents. No new blur, artwork or provider workload.

The read-only observer verifies the host UID/PID and output, bounds IPC to 300 ms
and 64 KiB, reacts to configuration/socket events, and uses a 30-second fallback.
Inactive mode has no activity-provider connection or animation subscription.

Final validation: **56 focused checks passed**, plus exact-ELF private COSMIC
acceptance in dark and light with two Standard/Islands round trips, preserved
placement/process, grouped Islands, live controls, service restart and zero settled
animation messages. Stock COSMIC stayed hidden despite a saved Islands-enabled
preference. All fixture processes were reaped. No installation or release.

Evidence: `.task-activity-islands-integration-20261103/delivery.json` and its native
capture directories. Existing interaction and hardware/package gaps remain open.


### Outer-frame alignment and title ownership — 2026-11-03

The earlier Islands integration capture still showed the panel behind the popup:
the popup began at the inner applet's top edge. It now uses the panel's recommended
applet height, outer padding and top corners. The background extends downward from
that outer frame; its top edge does not create a second lip. The fixed slot keeps
neighboring items still. A geometry change closes an obsolete popup anchor.

The parent also stops painting its title while the popup moves or is open. The
popup carries the only title; the parent returns at the identical closed frame.
This removes the ghost title visible through the growing surface.

Validation: **43 focused tests passed**, including five sizes with three padding
values. Final-ELF native dark/light interaction runs and XS/M/padded-XL captures
passed. A 300-frame native recording retained the title throughout and was reviewed
for duplicated content. No installation or release; existing interaction and
hardware/package qualification gaps remain open.

Evidence: `.task-activity-seam-20261103/delivery.json`, `review.md`, and
`motion-final-m/opening.mp4`.


### Standard bar: attached card — 2026-11-04

Activity Island no longer hides on Standard panels or stock COSMIC. The observer
still asks the companion whether the owning Panel runs Islands (with the same
startup grace, so Islands users never see the other shape first). Any other top
Panel gets the attached card:

- The title line keeps its resting place and width in the bar; the collapse
  control joins it there. Nothing is painted over the bar beside the slot, so
  neighbouring applets stay visible.
- The card widens from the slot to the full width and drops from the bar's lower
  edge. It uses the bar's colour and opacity (never below 0.7, for reading), with
  concave 12px shoulders where it meets the bar and 20px lower corners.
- Details, the elapsed line and actions sit in the card; the chooser too.
- Pointer and outside-click handling cover the slot and the card, not the
  transparent corners of the popup.
- Settings no longer says the island needs Panel Islands.

Known limits: COSMIC turns the bar opaque while a window is maximized, but the
card keeps the configured opacity; while the card is open the popup still
captures input over the bar a little beyond the slot, as in Islands mode.

### Notifications and per-source switches — 2026-11-04

The island now shows desktop notifications, on Islands and Standard bars alike.

- Source: the applet is marked `X-NotificationsApplet=true`, so COSMIC's panel
  hands it the notification daemon's private applet connection (fd number in
  `COSMIC_NOTIFICATIONS`, a p2p D-Bus link). The daemon sends every lasting
  notification there (`com.system76.NotificationsApplet.Notify`), also while its
  own popups are off; actions go back through `InvokeAction`, so apps get their
  activation token as from a popup. Dismiss and Clear all use
  `org.freedesktop.Notifications.CloseNotification`; `NotificationClosed` with
  any reason but expiry removes a notification. Text never leaves the applet:
  it is not sent to the Halcyon desktop, stored or logged.
- Shape: the newest notification leads with its app icon and title. Every other
  activity (more notifications, music, Focus, meetings, system) shows as an icon
  beside the title, up to three; beyond that the last place is a "+N" count that
  opens the list. Each icon selects its activity directly.
- Card: the message on up to two lines, Dismiss, up to two of the app's own
  buttons and Clear all, as width allows. Clicking the open title runs the
  notification's default action. Urgent notifications use the warning colour
  and lead ahead of everything but an urgent system activity.
- Popups: "Use the island instead of popups" sets COSMIC's
  `max_notifications` to 0 while the island is live and connected, after
  saving the previous limit in `<state>/notification-popups`. The applet gives
  it back when the preference turns off or Halcyon goes away; the desktop gives
  it back when no island is connected (immediately on disconnect, and 20s after
  startup). A limit changed by hand meanwhile is left alone. Written with
  COSMIC's config API, not the desktop store: a lease, not a design change.
- Reveals: with popups replaced, a new notification opens the island for 5s,
  following Do Not Disturb, fullscreen and "Brief automatic reveals". With
  COSMIC popups still on, the island only lists them, so nothing shows twice.
- Settings ("Show in the island"): Playing media, Upcoming meetings,
  Notifications (with "Use the island instead of popups", default off, and
  "Show message text"), System activities (with Battery, Sound output,
  Microphone in use, Network, Software updates) and Brief automatic reveals.

Known limits: the panel only passes the connection to applets started after
the `.desktop` change, so the island must be reinstalled and the panel
restarted once. Notifications that arrived before the island started are not
shown (COSMIC's own applet keeps its history).

### Tier 1: notification rules, Do Not Disturb, privacy, Bluetooth and pointer — 2026-11-05

Modelled on the behaviour reviewed in zickrian/isle, minus screen sharing.

- Notification rules: low urgency notifications are listed but never reveal.
  Reveals that cannot show now (island open, pointer on it, fullscreen,
  locked, or within the 2s gap after a reveal) wait in a queue of ids instead
  of being dropped, and are shown once the way is clear (800ms after a popup
  closes, or after the pointer leaves). More than three waiting become one
  "N new notifications" reveal naming their apps, with Show all and Clear
  all. Critical notifications go first, never fold into a summary, and stay
  until someone hovers, opens or dismisses them; a second critical one queues
  the first rather than hiding it.
- Do Not Disturb: the open island's idle card and the chooser have a switch
  that writes COSMIC's own `com.system76.CosmicNotifications`
  `do_not_disturb` (v1), and the closed island shows a moon beside the title.
  While it is on only critical notifications reveal. The desktop's quiet
  watcher now always runs while an island listens, so the moon and switch
  follow changes made elsewhere; a read error counts as off.
- Privacy dots: a green dot beside the title while a hardware camera is open,
  orange while the microphone is in use, with the app in the tooltip. A dot
  is not repeated for the card already showing that source. The camera
  source watches `/dev/videoN` for hardware devices only (loopback and other
  `virtual` devices are skipped), and names the holders from `/proc/*/fd`
  and `comm`. It never opens a camera. New switch: "Camera in use".
- Bluetooth: connects and disconnects from BlueZ (`org.bluez` ObjectManager,
  `Device1`, `Battery1`) reveal for 4s with the device name and battery when
  known; a battery reported a moment later fills in the same reveal. What is
  already connected at startup is not announced. Read only, with NoAutoStart.
  New switch: "Bluetooth devices", opening COSMIC's Bluetooth page.
- Pointer: middle-click plays or pauses the current player, right-click
  dismisses an automatic reveal and otherwise raises the player, and a
  touchpad swipe (40px of pixel scrolling in either direction) dismisses a
  reveal. Mouse wheels scroll in lines and are ignored. Resting to open and
  closing after leaving are unchanged.
- Protocol: `Snapshot.dnd` and `Command::Raise` were added, so the socket is
  now `session-v3.sock` (`preview-{key}-v3.sock`; v4 since app rules).
  Island and desktop must be installed together.

Known limits: there is no idle detection, so "until someone is there" means
until the pointer reaches the island or it is dismissed. Swipe accepts both
directions because COSMIC's natural scrolling setting is not known to the
applet. Closing a camera opened read-only raises no file event, so the dot
can take up to 5s to go. Processes of other users are not visible, so their
camera use is not shown.

### Notification list (2026-11-12)

Several notifications now read as one list instead of one card at a time.
Before, every notification was kept (newest first, up to 20) but each was its
own activity: the card showed one, and the rest were small icons or a "+N"
that opened the activity chooser. Nothing was dropped; the shape hid them.

- Closed: the newest notification leads (an urgent one first, as before) and
  the others become one "+N" count beside the title, ahead of any other
  activity icons. The count and its tooltip open the list. Behind another
  activity, only the newest notification shows as an icon, and with more than
  one waiting it opens the list too.
- Open: with a notification in front and more than one present, the open
  island shows the list. The title line reads "N notifications". Each row has
  the app icon, the title (warning colour when urgent), how long ago it came
  ("now", "N min", then the clock time), and the app name with the body when
  notification text is on. Clicking a row runs its default action, or selects
  it when it has none. Each row has its own Dismiss; Clear all sits below.
- Size: up to five rows, then "and N more" beside Clear all. The popup is
  sized once when it opens, at most 198px taller than the card; opening the
  list from a smaller popup recreates it at the list's height, as promotion
  already does. Rows that would not fit are counted, never clipped.
- Summary: "N new notifications" Show all and its title now open the same
  list rather than the chooser.
- Unchanged: Do Not Disturb, the urgent and low rules, reveals and LIMIT; a
  reveal still shows one notification as its card. Focus moves through the
  rows, each row's Dismiss and then Clear all, in reading order.
- Each notification now records when it arrived (`Note.at_ms`), in memory
  only, for the row's age.

### Groups, Recent and app rules (2026-11-12)

- Groups: two or more notifications from one app (by desktop entry, else app
  name) are one row, "Slack · 4 new", with the newest title and message
  beneath. Activating it opens the group in place, newest first, each with its
  own Dismiss and actions; one group is open at a time and its Dismiss clears
  the group. Urgent notifications always stand alone above the groups. The
  pill and the "+N" count still count notifications; closed, the pill title
  reads "Slack · 4". An open group that would not fit moves to the top.
- Recent: what was dismissed, timed out (its own `expire_timeout`, one wake
  deadline, no polling) or closed by the app. Session only, in memory, last
  20, one entry per id. Under the list as "Recent · N", folded by default,
  with Clear. A row runs the notification while the server still holds it,
  else opens the app from its desktop entry, else is plain text. With nothing
  live, the idle island shows a Recent button and the chooser a Recent row.
- App rules: right-click a row or group, or its More button, for Mute this
  app, Always show, List only and Reset to default, shown in place of the
  list (Escape goes back). Stored in `activity_island.app_rules` as
  `{app, entry, rule}` (at most 32), sent with `Command::NoteRule`, which
  carries only the app and its entry. Muted apps are never listed or kept;
  Recent counts their arrivals. Always show reveals even when the island only
  lists, still behind Do Not Disturb unless urgent. Settings lists the rules
  under Activity island with a rule picker and Reset per app.
- Protocol: `Command::NoteRule` was added, so the socket is now
  `session-v4.sock` (`preview-{key}-v4.sock`).

Known limits: Always show reveals the island even when COSMIC's own popups
are on, so both can show. List only cannot stop COSMIC's popup when the island
does not replace popups.
