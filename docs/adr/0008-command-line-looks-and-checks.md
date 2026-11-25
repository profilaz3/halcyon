# Command-line Looks, running checks and desktop restore

## Context

`halcyon-ctl` hands commands to the running widgets by writing
`$XDG_RUNTIME_DIR/halcyon/command.json`. Unlike `halcyon --toggle`, it never
checks that the widgets run, so `halcyon-ctl focus toggle` exits 0 and does
nothing while they are stopped. Its `toggle` knows widget keys only, not
`peek` or `drawer`. It can apply one of six palettes but no Look, and
`restore` puts back the wallpaper only, while Settings' Stop using Halcyon
restores the whole desktop from before Halcyon.

## Decision

- `halcyon-ctl` checks the widgets' lock (`widgets.lock` beside
  `command.json`, held with `flock` by the running widgets) before writing a
  command, and fails with "Halcyon isn't running" and exit 1 when no
  process holds it, as `halcyon --toggle` does.
- `halcyon-ctl toggle` accepts every `halcyon --toggle` target: `peek`,
  `drawer` (and `menu`), `system-monitor`, `daily-overview` and widget keys.
- A desktop command `{"action": "look", "look": <ProfileRef>, "name": …}`
  switches Looks exactly as an edge menu Look item does.
- `halcyon --looks` prints each Look one could switch to, built in then
  saved, one name per line. `halcyon --look NAME` finds one by name (without
  regard to case) or id, then sends the `look` command; an unknown or
  ambiguous name is an error naming the choices. `halcyon-ctl looks` and
  `halcyon-ctl look NAME` run these.
- `halcyon --restore-desktop` runs the same desktop recovery as Stop using
  Halcyon. `halcyon-ctl restore --desktop` runs it before stopping the
  widgets; plain `restore` keeps its wallpaper-only meaning.

The list of Looks moves from Settings' edge menu code to
`automation::look_choices`, which both use.

## Alternatives

- Teach the Python script the Look library. It would duplicate the built-in
  Looks and the saved library's format.
- Make `halcyon-ctl` start the widgets when they are stopped. Starting is
  `halcyon-ctl start`; a shortcut that silently starts the desktop is a
  surprise.

## Consequences

Scripts that relied on `halcyon-ctl` exiting 0 while the widgets are
stopped now see exit 1 with a message. Looks are switched by the running
widgets, so `halcyon --look` also needs them running and says so.
