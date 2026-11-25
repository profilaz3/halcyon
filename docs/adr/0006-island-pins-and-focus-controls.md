# Island pins and Focus controls

## Context

The Activity island can pin Music and Messages to the desktop, and its Focus
card has one button: start, pause or resume. The desktop Focus card also has
Skip, Stop and the 15, 25 and 50 minute lengths, and Focus, Calendar and
Devices have desktop widgets the island could pin the same way. The island
runs in COSMIC's panel process and reaches the desktop only through the
owner-only bridge: a `Snapshot` going out, `Command`s coming back, both
serde-tagged with `deny_unknown_fields`.

## Decision

Extend the bridge in the shape it already has:

- `Command::PinFocus`, `Command::PinCalendar` and `Command::PinDevices`, like
  `PinMusic` and `PinMessages`. The desktop runs the existing
  `pin_from_island` flow for each, gated by the island setting that shows
  that card (Calendar by `calendar`, Devices by `system`; Focus always shows).
- `Command::FocusSkip { key }`, `Command::FocusStop { key }` and
  `Command::FocusMinutes { key, minutes }`. They carry the session key like
  `Command::Focus`, so a click on a card the desktop has since replaced does
  nothing. They map onto the desktop card's own `FocusSkip`, `FocusReset` and
  `FocusMinutes(minutes)` actions; minutes are the card's 15, 25 or 50 only.
- `Snapshot::focus_pinned`, `calendar_pinned` and `devices_pinned`, like
  `music_pinned`: present only when true, so the button says "Show on
  desktop" for a widget already there.

## Alternatives

- A generic `Pin { widget }` command and a `pinned` list. Cleaner, but it
  would replace two commands and a field in the same release for no user
  benefit, and two shapes of the same thing would live side by side.
- Island buttons that open the desktop widget's Settings page instead of
  pinning. Already possible from Settings; it does not save the trip.

## Consequences

The island and the desktop are the same build. During an update a panel
still running the previous island refuses a snapshot carrying a new `true`
flag until the panel restarts, as with `messages_pinned` before; nothing is
written or lost. Pinning Calendar while the Daily overview shows the
calendar says so instead of adding a second calendar.
