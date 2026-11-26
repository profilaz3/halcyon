# A quick timer becomes a Focus session

## Context

The Activity island's quick timer (`activity_island::timer`) and the Focus
timer (`tools::focus::Session`) are separate models. The quick timer takes
any length but lives only in the island; Focus has a desktop widget, rounds
and breaks, but only its preset lengths. A timer someone started for "the
next 40 minutes" can't be put on the desktop or counted as focus.

## Decision

Keep both models, and let a quick timer turn into a focus round:

- `Session::adopt(total, remaining, running, now)` makes the session a focus
  round with the timer's time left, running or paused as the timer was,
  measured against the timer's full length.
- A bridge command `TimerToFocus` (the island timer card's Focus button) and
  `halcyon-ctl timer focus` (desktop command `{"action":"timer","do":"focus"}`)
  ask the desktop to do it. The desktop adopts the timer into the session,
  cancels the quick timer and pins the Focus widget as Pin to desktop does.
- A Focus session already running or paused is not replaced: the desktop
  says so and keeps both.

## Alternatives

- Merge the models into one timer. A large change to two working features,
  their saved state and every place that shows them.
- Replace the quick timer with Focus at a custom length. Focus rounds lead to
  breaks, which a kitchen timer shouldn't.

## Consequences

A finished adopted round counts as a focus round and leads to a break, like
any focus round. The quick timer's chime no longer plays for it; Focus's own
notification does.
