# Edge menu media actions

## Context

A customized edge menu saves each Halcyon action by its key
(`Kind::Action { action }`, `Action::key`). It offers widgets, Peek, Looks,
apps, folders and seven actions, but nothing for the music that is playing;
the Music widget's controls only reach a player while that card is shown,
because the player watcher runs only then.

## Decision

Add two action keys, `play_pause` and `next_track`. Running one asks the
session bus for the players once, picks the player the Music widget would
follow (the one picked on the card, else the preferred app, else the most
recent) with `music::choose`, and sends it Play, Pause or Next through
`music::perform`, as the card does. A player that can't take the command,
or no player at all, gets a short notice instead of nothing.

Neither shows an on or off switch: the state they change is the player's.

## Alternatives

- Keep the player watcher running while the menu has a media item. It costs
  a session bus subscription all day for a button pressed now and then.
- One `media` action that opens the Music card. A detour for a single press.

## Consequences

A menu saved with these keys and opened by an older Halcyon lists them as
unavailable, as it already does for any action key it doesn't know; nothing
is dropped from the saved list.
