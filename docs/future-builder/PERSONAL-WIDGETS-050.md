# Calendar and system recipes

Implemented locally for 0.5 in **Widgets → Widget builder**. Calendar and System
monitor join Clock and Weather. Choose parts and order, compact/wide arrangement,
and preview sample or missing data. Save keeps a library copy; Apply updates the
existing widget. Use built-in and Undo preserve the previous configuration.
Canvas and setup scenes use the same recipe presentation with illustrative data.

Calendar offers date, agenda, caption, divider, space and Calendar settings. It
uses the existing calendar cache, selected dates and connection lifecycle. Busy
mode removes titles and event actions before rendering. Retained and unavailable
events remain labelled. Preview events cannot open live appointments. A short
widget keeps an agenda entry and any connection warning; larger agendas show a
remaining count.

System monitor offers CPU, memory, disk, network, GPU and uptime plus the shared
presentation parts. Keep at least one reading; the first stays visible in a small
widget. Missing readings show a dash and a status. The existing sampler requests
only the recipe's selected metrics and pauses when hidden. No extra polling loop,
hardware process or provider is created. Built-in section choices remain saved.
The System monitor page links directly to editing the active recipe.

The existing limits remain: eight parts, 16 KiB per recipe and 32 saved recipes.
Library entries are immutable revisions; an applied snapshot survives library
changes and deletion. No commands, arbitrary URLs, paths or provider bindings are
accepted. Calendar/System recipes require recipe version 2, applied design version
3 and portable profile version 16. Clock/Weather recipes keep their old versions;
older apps reject unsupported files without rewriting them.

Focused tests cover round trips, migration and downgrade rejection, guarded
Apply/Undo, preserved source preferences, privacy, missing observations, hidden
provider demand, part bounds and native rendering. Compact and wide light/dark
views are inspected before final integration. Final packaged 0.5 acceptance is
separate from this local implementation.
