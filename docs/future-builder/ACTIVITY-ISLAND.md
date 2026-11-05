# FB31 — Activity island in the status bar

## Reactive defaults — native revision, 2026-11-04

The requested default is **system activities, media while playing, and Calendar
when a meeting is approaching**. This replaces the original default Focus fallback,
always-available media and distant-event promotion described later in this document.
Focus remains available when explicitly started; it is not the idle screen.

The reactive revision is implemented locally. The
[native implementation notes](ACTIVITY-ISLAND-IMPLEMENTATION.md) describe current
behavior, validation and limits. The research contract below retains longer-term
ideas beyond this revision; it is not a list of released capabilities. The
[interactive behavior study](concepts/activity-island-reactive.html) uses simulated
events; its compact geometry and timing do not qualify native performance.

### What the research supports

Apple distinguishes minimal, compact and expanded presentations. Its design talk
emphasizes glanceable ongoing tasks, essential controls, content-specific layouts,
selective alerts, and removing completed activities after a short interval. The
useful model is a persistent home for an activity, with transitions that explain
what changed. Desktop hover/click behavior is our adaptation, not an iOS gesture
copied literally. Sources: [Live Activities guidance][reactive-apple-hig] and
[Design dynamic Live Activities][reactive-apple-talk].

### Proposed default behavior

| Source | Eligible for the island | Presentation and end condition |
| --- | --- | --- |
| Media | MPRIS reports Playing | Title and supported controls. Pausing while interacting preserves Resume. Once closed, allow a 10-second resume grace, then withdraw. An unrelated already-paused player does not become eligible. |
| Calendar | A timed event starts within 5 minutes | Countdown, then a brief starting cue. After 2 minutes past start, leave automatic primary selection; retain the event in active details until it ends. This does not imply attendance. All-day events and distant events stay out. |
| Power | Charging changes, low/critical battery | Charging acknowledgement for about 3 seconds, then restore the previous activity. Low-battery incidents remain accessible until resolved or dismissed; do not alert on each percentage change. Hide absent hardware and unknown estimates. |
| Audio devices | A usable output connects or routing changes | Brief device confirmation; then return to the previous activity. Show battery only when reported. A Bluetooth connection alone does not prove that audio has switched. |
| Microphone | A running capture stream is linked to a physical input | Persistent, labelled capture indicator; app identity and input mute in details where supported. Never infer a call from a microphone stream. Monitor graph metadata, not recorded audio. |
| Network/VPN | Connection attempt, unexpected loss or recovery | Brief named status; an unresolved failure remains in active details. Do not repeatedly announce normal connectivity or display a permanent duplicate Wi-Fi meter. |
| Updates | A supported updater exposes an actual operation | Current phase and truthful progress, then one completion/failure cue. Unknown progress is labelled, never fabricated. Open the owning updater for authorization, errors and restart decisions. |

Five minutes, two minutes, ten seconds and three seconds are starting values for
usability testing. Calendar lead time can use a short 5/10/15-minute selector.
The bridge currently carries the next timed event, not a meeting-attendance signal.
The initial behavior therefore uses timed events. Cancellation, declined-event
filtering and an explicitly supplied meeting link need provider-level verification.
The owner currently picks the earliest event that has not ended. An ongoing event
must not obscure the next approaching meeting: select the upcoming candidate before
reducing the feed to the bounded island projection, retaining the current event
separately if necessary. Cover overlapping and back-to-back events explicitly.
Show Join only for a validated link from the current event; otherwise Open Calendar.
Never open a call automatically. Preserve Busy privacy before publishing any view.

### Selection and interruption contract

1. **No eligible activity:** a quiet minimal handle. Opening it says there are no
   active tasks and offers the Activity settings entry. No default Focus launcher,
   paused-player placeholder or fabricated healthy-system status.
2. **One activity:** one compact title and one useful cue. Hover reveals only the
   relevant details/actions. No permanent Music/Focus/Calendar tab strip.
3. **Several activities:** one primary plus a small labelled count. Open the count
   to choose from named live activities. A microphone-in-use indicator remains
   visible even if a different source is primary. Never cycle automatically.
4. **During interaction:** keep the selected identity and controls fixed through
   hover, keyboard use and explicit hold. Incoming events update the pending cue.
   If that source ends, show its ended/unavailable state and disable stale actions;
   do not put a different operation beneath the pointer.
5. **Outside interaction:** an unresolved urgent incident is primary, followed by
   an imminent meeting, an explicitly started timer, playing media, then another
   ongoing system task. Keep the current eligible item on equal priority. Microphone
   use is a persistent secondary indicator, not a reason to steal the main title.
   If nothing else is active, it can be primary. Device/charging acknowledgements
   can briefly reveal and then return; they cannot displace an urgent incident.
6. **Dismiss:** Escape closes the surface and suppresses the current reveal, not
   the underlying job. An explicit Dismiss hides that incident/meeting occurrence
   from automatic promotion, without cancelling it. Identities include provider
   generation and occurrence; a future meeting or new critical condition is new.
7. **Quiet policies:** no unsolicited expansion during DND, screen lock, fullscreen
   on the island's output, or hidden-panel state. Explicit opening still works when
   the desktop is unlocked. If these signals are unavailable, suppress automatic
   reveals. Never replay a backlog after quiet mode, resume or reconnect.

Coalesce charging/device reconnect bursts, use monotonic deadlines for UI intervals,
and refresh calendar wall-clock deadlines after sleep or clock changes. Initial
provider discovery is a snapshot, not a queue of new alerts. Debounce transient
network loss; distinguish deliberate disconnects. Keep a maximum of one pending
reveal, expire obsolete events and impose a routine-reveal cooldown (initially 30
seconds). No sounds or repeated track-change expansion by default. A genuine new
critical incident may bypass the cooldown, but never quiet mode or interaction hold.

### Motion, placement and accessibility

Visual refinement, 2026-11-03: the user rejected rectangular Pause/Next text
buttons and a dot-like idle state. The revised concept uses familiar Previous,
Play/Pause and Next icons, a circular central transport control, and a compact
timeline only when duration is known. Every icon has a keyboard-accessible control
and an explicit accessible name. Collapse is a small header chevron; Escape and
outside click remain available. There is no Close action beside the transport.
Quiet mode uses a slim horizontal capsule labelled Activities, with no dot or
idle pulsing. These are prototype refinements; native integration remains pending.

Keep the corrected native outer-edge handoff: one title and one shape growing
downward, with an interruptible spring. Hover intent remains 150 ms and departure
grace 300 ms as the starting baseline. Content should crossfade only when its
identity changes; number updates do not restart the whole opening animation.
Compact media shows actual state rather than a decorative continuously animated
equalizer. No audio sampling merely to animate an icon. Reduced motion removes
size animation; text and controls remain fully available.

Idle/minimal width is new native work: the current applet reserves 328 logical
pixels. A smaller rest state must be measured against real host layout. Hover must
never reflow neighboring applets, overlap their controls or change reserved panel
height. Grouped Islands may need to retain a larger allocated slot. At crowded
widths use a compact summary and a bounded inward popup, not clipped actions.

Click or Enter opens and holds; Escape/outside click closes. Hover does not steal
keyboard focus. Resolve the known hover-to-pin popup-grab gap and stationary-hover
timeout before claiming this contract is complete. Tab order, screen-reader names,
focus restoration and pointer transfer need native acceptance. Activity Island
needs an explicitly added applet on a top Panel. In Islands mode the island grows
in place; on a Standard bar, including stock COSMIC, the title stays in the bar and
the details hang beneath it as an attached card (see "Standard bar" in the
implementation notes). This change does not add or replace panel applets automatically.

### Linux feasibility and honest limits

| Integration | Available mechanism | Boundary to verify |
| --- | --- | --- |
| Media | [MPRIS PlaybackStatus, metadata and capabilities][reactive-mpris] | Preserve the user's selected playing player. A browser exposing MPRIS can participate; an arbitrary sound is not a media session. |
| Power | [UPower device state, warnings and estimates][reactive-upower] | Missing/unknown values and hardware that reports levels rather than precise percentages. |
| Audio and capture | [PipeWire graph][reactive-pipewire] and [application metadata][reactive-pipewire-keys] | Separate microphone input from loopback capture; stale nodes and app labels are not proof of live recording. No blanket camera or screen-sharing guarantee. |
| Bluetooth | [BlueZ connection state][reactive-bluez] and [Battery1][reactive-bluez-battery] | Optional battery support; connected device versus selected audio route. |
| Network | [NetworkManager active-connection state][reactive-network] | Only managed connections; distinguish intentional disconnects, VPN types and startup/resume reconciliation. |
| Package updates | [PackageKit transaction phase/progress][reactive-packagekit] | Covers PackageKit operations. APT/DNF commands and independent Flatpak frontends do not automatically share that transaction. Observe; do not become a second installer. |
| Screen sharing | [Portal session API][reactive-portal] | The client API creates and controls the caller's sessions; it is not a general list of every app's recording. Needs compositor/backend or app integration. |
| Downloads, copies, builds | Explicit application integrations | No universal source established by this research. Defer rather than infer progress from file growth or scrape notifications. |

Read-only service discovery on this workstation found UPower, NetworkManager and
BlueZ running, with PackageKit activatable. This establishes service presence only,
not successful subscriptions, physical-device behavior or permission coverage.
The pinned COSMIC toplevel protocol exposes fullscreen and output association;
Halcyon's Activity path does not yet consume those signals or DND. Establish the
quiet-policy adapter before enabling automatic reveals. Do not take over the
desktop notification bus name or duplicate notification sounds. Existing COSMIC
notifications remain intact; coordinate supported event presentation instead of
promising universal deduplication.

Update 2026-11-04: notifications now come through COSMIC's supported
notification-applet connection, not the bus name. Replacing COSMIC's popups is
an opt-in switch that only lowers its popup limit while the island can show
them, and always gives the previous limit back.

### Implementation order and acceptance

1. Selection model: eligibility, expiry, stable selection, occurrence dismissal,
   deterministic priority and quiet gates. Reuse the existing bridge/provider owner;
   version additions because its current snapshots reject unknown fields.
2. Media + Calendar: correct the current paused/distant-event fallback. Review the
   quiet, playing, paused, imminent, starting, expired and simultaneous states.
3. Power + audio device events, then microphone and network: read-only event
   subscriptions, meaningful transitions, burst coalescing and availability states.
4. PackageKit operation observation, followed by explicitly supported integrations.
   No assumption that Debian/RPM packaging alone supplies global update progress.
5. Native interaction and motion: dark/light, compact/crowded/grouped panel, scales,
   keyboard, stationary hover, hold/outside click, source loss, privacy and quiet
   mode. Measure idle/active/hidden CPU and frame pacing on the final binary.

Keep source observation demand-driven and animation asleep when settled/hidden.
Reconcile fresh snapshots after reconnect; never turn source failure into success.
Test service ownership changes, obsolete action targets, suspend/resume and missing
hardware. Use the existing bounded IPC and finite source-specific records; no plugin
engine or general rules editor is needed for these defaults. Initial settings are
System activities, Media and Upcoming events, plus event lead time and automatic
reveals. Advanced source opt-outs can stay behind one disclosure.

[reactive-apple-hig]: https://developer.apple.com/design/human-interface-guidelines/live-activities/
[reactive-apple-talk]: https://developer.apple.com/videos/play/wwdc2023/10194/
[reactive-mpris]: https://specifications.freedesktop.org/mpris/latest/Player_Interface.html
[reactive-upower]: https://upower.freedesktop.org/docs/Device.html
[reactive-pipewire]: https://docs.pipewire.org/page_objects_design.html
[reactive-pipewire-keys]: https://docs.pipewire.org/group__pw__keys.html
[reactive-bluez]: https://bluez.readthedocs.io/en/latest/device-api/
[reactive-bluez-battery]: https://bluez.readthedocs.io/en/latest/battery-api/
[reactive-network]: https://networkmanager.pages.freedesktop.org/NetworkManager/NetworkManager/gdbus-org.freedesktop.NetworkManager.Connection.Active.html
[reactive-packagekit]: https://www.freedesktop.org/software/PackageKit/doc/dbus-org.freedesktop.packagekit1.Transaction.html
[reactive-portal]: https://flatpak.github.io/xdg-desktop-portal/docs/doc-org.freedesktop.portal.ScreenCast.html

## Earlier design exploration

The following material records the original direction. Where defaults or host
support differ, the reactive proposal above and the native implementation notes
take precedence respectively.

Revised design, 2026-11-01. The [first native implementation](ACTIVITY-ISLAND-IMPLEMENTATION.md) is local; broader delivery gates below remain planned.
[Interactive concept](concepts/activity-island.html) uses sample data and simulated
actions. It is not a screenshot of the application or proof of native support.

## Direction

The user corrected the first concept: the activity belongs **in the status bar**,
expanding from that position on hover or relevant activity. It must not look like
a separate desktop widget floating underneath the bar.

This supersedes the first concept's desktop placement, click-only controls and
no-automatic-expansion rules. The [first mockup](concepts/activity-island-v1-widget.html)
is retained only as a superseded reference.

## Reference behavior

- [Apple's Live Activities guidance](https://developer.apple.com/design/human-interface-guidelines/live-activities/)
  distinguishes compact, minimal and expanded presentations. On iPhone,
  touch-and-hold expands the island; essential updates may present an expanded alert.
- [Apple's Mac behavior](https://support.apple.com/en-ca/120684) puts Live Activities
  in the menu bar, with a click opening details. This is different from a hover notch.
- [Boring Notch](https://github.com/TheBoredTeam/boring.notch) explicitly expands
  on hover. This is the relevant desktop pointer interaction reference.

For Halcyon, combine a native bar position with hover controls, click/keyboard
access and brief, selective activity reveals. No physical display notch is required.

## Revision 3 — one visual object

The user rejected the second concept because the bar and open card repeated the
same title and status. The [attached-card version](concepts/activity-island-v2-attached-card.html)
is now superseded as well.

The revised island is one capsule that changes shape in place. Its title, icon and
status each have one visual representation. The same title moves into the open
layout; no compact header remains above a second copy. The single HTML title node
persists across all three states. This is a visual contract, not a requirement
that COSMIC implement it using one operating-system surface.

- Expand from the same top-center anchor. Keep the top edge steady and interpolate
  the capsule's width, height and radius. No stem, stacked card, duplicated title
  or permanent activity-tab row.
- Show one title, one useful secondary line and the minimum supported controls.
  Calendar uses a shallower surface than Music. No large blank artwork placeholder.
- A single Other activities button reveals only the other activities. Incoming
  events mark that switcher while the user operates current controls.
- Hover opens; clicking the title holds/releases; outside-click and Escape close.
  No permanent pin/close toolbar. Keyboard access remains explicit.
- Reserve neighbors' geometry: expansion can use free center space, never cover
  existing panel items. In a crowded bar, constrain width and use a taller layout.
  The HTML uses representative safe margins, not real panel geometry detection.

Motion proposal: a 340 ms ease-out for the outline and shared content movement;
controls fade in after approximately 100 ms. Retarget an in-progress transition
from its current geometry, including reversal on departure. No bounce, autonomous
pulsing, idle physics simulation or repeated panel-wide redraw. Reduced motion
changes state without size animation. Native smoothness/resource use is unmeasured.

## Three states

| State | Presentation | Behavior |
| --- | --- | --- |
| In the bar | Activity icon, short title and live status | Fits the bar's normal height; no second floating card |
| Hover controls | The capsule itself grows downward from its bar position | Music transport, Focus controls or Calendar details; remains open while used |
| Activity reveal | The same capsule briefly grows enough for one update | Briefly appears for an important event, then returns to the bar without taking focus |

Starting timings for evaluation: 150 ms hover intent, 300 ms pointer-leave grace,
and a 2.8 s activity reveal. These are prototype values, not validated product defaults.
Entering a reveal opens its controls. Moving between the bar and expanded surface
must not collapse it. Clicking the title holds/releases it; Escape or outside-click dismisses.
Keyboard activation opens controls without relying on hover. Pointer-only opening
does not capture keyboard focus; button clicks do not silently pin a hover surface.
Reduced motion removes size animation while preserving the same states.

The panel's reserved height stays constant. The expanded surface overlays the
workspace; it does not move windows, push other applets aside or reflow the desktop.
Continuous and floating panel styles retain the same interaction. Expansion has
no separate header/body seam. It must not resize the panel slot on hover and push
neighboring applets aside.

## Activities and interruption rules

Start with Music, Focus and Calendar, sharing existing providers and actions.
Standalone widgets may coexist; enabling the island does not hide or reconfigure them.

- Music: title, artist, supported playback controls and progress. No empty artwork
  frame when artwork is unavailable. Keep selected-player semantics.
- Focus: remaining time, session/break and pause/resume. Completion presents a
  truthful completed state and a deliberate Start break action; never restart a timer
  merely by opening the island. End/restart confirmation follows the existing timer.
- Calendar: event time and countdown. Busy privacy hides event titles and calendar
  identity in every state, including incoming reveals. Open Calendar uses the existing
  editor; it does not mutate an event or start a call.
- Idle: optional clock or an empty compact anchor. The native prototype must settle
  discoverability and avoid silently replacing the existing panel clock.

Proposed priority is imminent Calendar, Focus, Music, then idle. Opening/selecting
an activity holds its controls stable: another event shows a small pending cue,
not a different set of buttons beneath the pointer. Keep the highest-priority pending
cue, coalesce repeats and expire obsolete events. Dismissal applies to that occurrence.
Track changes are an optional reveal; playback progress is never an alert trigger.
Define repeat limits and priority tie-breaking in the state slice before shipping.

Do not reveal automatically during full-screen use or Do Not Disturb. Autohide
panels must not be pulled onscreen for routine updates. If the user disables activity
reveals, compact status and deliberate access still work. Source loss remains visible
without replacing the activity currently being operated.

## Native integration is the first gate

Investigate a **COSMIC panel applet with an anchored popup**, using the existing
provider/service boundary. Prove placement and input before building full activity
editors. A desktop-widget overlay covering the clock is not an acceptable substitute.

The existing Islands feature styles native panel groups; it is not evidence that
an arbitrary widget can merge into the panel. Prefer support in both stock and
Islands-styled panels, but verify this rather than declaring independence from the
companion. A shared shape or animation may require additional host support. In particular,
prove the visual handoff between compact applet and popup: no duplicate pixels,
no blank frame and no second title. The popup may need to overlap its own applet
slot while preserving other applets, without requesting a larger layout slot.
Document the proven capability and any unavailable state before production work.

The initial native probe must cover:

- Real panel slot, configured output and anchor geometry, including crowded bars.
- Popup placement, stable reserved space and pointer crossing between both surfaces.
- Input transparency outside the surface, no focus theft, keyboard open/dismiss,
  click pinning and proper cleanup after dismissal or host exit.
- Scaling, display changes, autohide/full-screen and stock/companion behavior.

Default visual placement is top-center. A supported neighboring slot near system
indicators may be offered after the probe. Never silently remove, reorder or obscure
existing clock/system applets. No free desktop dragging or above-dock placement in
this first design. Other panel orientations require an explicitly supported inward
expansion and are not established by the top-bar mockup.

## Settings

Desktop → Dock & panel → Activity island:

- Enable and supported panel/display placement.
- Activities and simple priority order; Calendar reminder threshold.
- Hover opening, activity reveals, optional track-change reveals and keyboard access.
- Idle appearance and inherited panel colors, with a sample-data preview.

Use the existing Calendar privacy preference and accessibility/reduced-motion
settings. Do not create competing copies. The prototype's privacy switch only
demonstrates that rendering. Avoid a new general-purpose rules editor.

## Delivery slices

1. **Panel capability probe:** a compact sample and anchored popup, with hover,
   keyboard, focus and display checks. Review real native captures before proceeding.
2. **Activity state:** availability, priority, selection hold, pending events,
   dismissal, quiet policies and bounded reveal timers, with deterministic tests.
3. **Music:** shared MPRIS data/actions, no-artwork and unavailable states. Review
   native compact, hover and reveal captures and input behavior.
4. **Focus and Calendar:** shared timer/event data, completion, simultaneous activity
   handling and privacy throughout. Review and fix each before advancing.
5. **Settings and persistence:** native sample preview, search, scoped Apply/Undo,
   saved setups, restart and migration. Keep credentials/content out of shared looks.
6. **Qualification:** compact/wide/scaled/light/dark captures, keyboard and pointer,
   hotplug/resume, host failure and idle/active/hidden resource measurements. Exact
   package acceptance remains FB20.

Use existing provider demand and guarded stores. No duplicate polling, constant
hidden animation or new provider framework. The integration probe may change the
implementation mechanism, but should preserve the accepted placement and behavior.

## Prototype scope

Revision 3 demonstrates continuous/floating bars, a persistent single title node,
hover/held/reveal states, a secondary activity chooser, Music/Focus/Calendar samples
and Calendar privacy. It opens held for inspection; Replay motion demonstrates
collapse and expansion. Tracks and timers are simulated. The prototype does not
qualify native panel APIs, input ownership, accessibility, hotplug, resource use
or packaging. Prior revision checks do not qualify this revision.

Revision 3 concept review: inspected Calendar (normal/private), Music, compact,
expanded and brief-reveal states at desktop/phone widths and dark/light themes.
Browser checks passed at 728px and 360px for persistent single-title identity,
intermediate CSS transition geometry, bar-neighbor separation across three
activities/two bar styles/three states, privacy, chooser focus, hover/leave and
reveal timers, incoming events, keyboard dismissal and horizontal bounds. Timers
were controlled for these checks; native animation pacing and resource use remain
unmeasured.

## Alternative activity navigation — exploration

The user questioned the permanent icon tabs in the first native revision.
[Four interactive directions](concepts/activity-island-directions.html) explore
automatic priority, a live secondary counter, a hidden text picker and a ribbon
with a named next activity. These are sample-data design options, not an accepted
implementation change or proof of native behavior. No direction has been selected.
