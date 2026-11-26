# Command-line styles

## Context

`halcyon-ctl theme` applies one of six palettes. The styles Settings ›
Appearance offers (Soft through Blueprint, and the ready-made themes) have
no command, and `halcyon --look` (ADR 0008) switches Looks, which styles
aren't: Settings fits a style to the desktop before applying it.

## Decision

- `halcyon --styles` lists every style by name, built in then ready-made.
- `halcyon --style NAME [--own-bars]` finds one by name (without regard to
  case) or id and applies it to the desktop directly, as Settings does:
  - a built-in style keeps your own colors, overrides and Window tone
    (`themes::preserve_custom`), and the first style takes the color
    choice made in setup (`themes::first_colors`, now shared with
    Settings);
  - the dock and panel keep their shape unless `--own-bars` asks for the
    style's own, as "Use this style's dock and panel" does;
  - it plans and applies through the same commit step as a Look switch,
    so parts with unapplied changes in Settings wait instead of being
    overwritten, and a conflict is planned again once.
- `halcyon-ctl styles`, `halcyon-ctl style NAME [--with-dock-and-panel]`,
  and `halcyon-ctl theme NAME`, which now also takes a style name.

## Alternatives

- Make styles Looks so `--look` covers them. Looks apply as saved; styles
  are fitted to the desktop, and that difference is the point of a style.
- Send the style to the running widgets. Nothing about applying it needs
  them, so the command works while they are stopped.

## Consequences

Settings' color analysis of the wallpaper isn't available to the command.
A first style on a desktop with no accent chosen follows the wallpaper's
colors, which the desktop works out once the style is applied.
