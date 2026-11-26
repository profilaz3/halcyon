# Data tiles from local JSON files

## Context

FB35 asks for native tiles fed by data the person already has: a server
status, a build result, a download's progress, a personal goal. Halcyon has
a widget for each thing it knows about, but nothing for a number some other
program writes. The plan starts with local regular files and leaves HTTP(S)
for a separately reviewed step. It rules out executable recipes and a code
plugin system, and it keeps local paths out of shared designs.

Widgets run one instance per kind. Usage sharing reports a fixed list of
widget keys and Settings pages, and the collector refuses keys it doesn't
know.

## Decision

- A new tool widget, **Data tiles** (`Widget::Tiles`, key `tiles`), off
  until chosen. It shows up to six tiles. Several readings live in one
  widget, so no multi-instance machinery is needed.
- Settings key `tiles`: `{"enabled": bool, "tiles": [Tile]}`. Each `Tile`
  has the following fields; unknown fields are refused:
  - `label`: up to 40 characters.
  - `source`: `{"kind": "file", "path": <absolute path>}`. The tag leaves
    room for another kind of source later.
  - `field`: a dotted path into the JSON, up to 120 characters, with a
    number for a place in a list (`jobs.0.state`). Empty means the whole
    document.
  - `show`: one of `{"kind": "text"}`, `{"kind": "progress", "max": f64}`,
    `{"kind": "gauge", "min": f64, "max": f64}` or `{"kind": "graph"}`.
  - `unit`: up to 12 characters.
  - `every`: the refresh interval in seconds, one of 10, 30, 60, 300, 900
    or 3600.

  Invalid tiles are dropped on load.
- Reading:
  - It happens only while the widget shows and has tiles. Tiles that name
    the same file share one read, made at the shortest of their intervals.
  - The file is opened without blocking and must be a regular file once
    opened, so a FIFO or a device never hangs the widget. It must be at most
    256 KB, and is parsed as JSON. Nothing runs.
- States: each tile shows exactly one of these.
  - **Reading…** before the first answer.
  - **The value.** A string is cut to 80 characters, and true or false read
    "Yes" or "No". Progress, gauge and graph need a number; a numeric string
    counts.
  - **Stale**, when the file hasn't changed for three intervals or a read
    has failed since the last good one. It shows the last value with its
    age.
  - **A plain error**: the file is missing, isn't a regular file, is too
    big, isn't valid JSON, has no such field, or the value isn't a number.
- A graph keeps the last 48 readings in memory only.
- Settings > Data tiles lists the tiles and adds one with these steps:
  1. Choose a file.
  2. Halcyon reads it once and lists up to 40 fields that hold a value,
     with what each holds now; one click picks a field.
  3. Pick how to show it, its unit and range, and how often to read it.

  Four starting points fill the form: server status, build result,
  download progress and personal goal.
- Sharing:
  - `/tiles/tiles` is a local store field, never exported in a Look, so no
    path leaves the machine. An imported Look that shows Data tiles shows an
    empty widget until tiles are added here.
  - The widget and its page are left out of usage sharing, as Messages,
    Markets and Folder are, so reports keep their schema.

## Alternatives

- A command source whose output is parsed. It is the executable recipe FB35
  rules out.
- Multiple Data tile widgets. It needs instances of one kind at runtime,
  which no widget has yet; six tiles in one widget cover the examples.
- JSON Pointer (`/jobs/0/state`) for fields. It is exact but unfamiliar;
  the dotted path covers every example, and a key containing a dot can still
  be reached through its parent with a text tile.
- Watching the file with inotify instead of polling. It reacts at once, but
  it holds a watch per file all day, and editors that replace a file break
  the watch. A chosen interval is predictable.

## Consequences

- A file that grows past 256 KB, or a field a new version of the writer
  renames, shows an error on its tile instead of an old value.
- The 48-reading graph starts empty after each start.
- HTTP(S) sources are a later decision with their own record, covering
  credentials, redirects and rate limits.
