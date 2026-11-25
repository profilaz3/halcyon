# Notes in the Widget Builder

## Context

The Widget Builder makes personal designs for six widgets: Clock, Weather,
Calendar, System monitor, Music and Focus. A design is a `Recipe`: a kind,
an ordered list of native parts the kind allows, an optional finish
(Tidal, Paper or Carbon) for tool widgets, and a caption. The applied
designs live in `recipes` (`Applied`, one optional recipe per kind) and
are exported with a Look under `/recipes/<kind>`. The audit asked for the
builder to reach the other widgets, starting with Notes and Devices.

The Notes card edits in place, ticks checklist items, copies, offers Undo
after Cancel, and says when a note was saved or runs past the card. A
design must not lose any of that.

## Decision

- **A new kind:** `Kind::Notes` (`/recipes/notes`, `Applied::notes`, version
  1), a tool kind, so finishes apply.
- **Its parts:**
  - `note_header`, "Title and buttons": the title, the checklist count, and
    Copy and Edit.
  - `note`, "Note": the note itself with its footer line (saved time, "Open
    full note →", Undo, or a save problem).
  - `caption`, `divider` and `space`.
  - Both of the first two are required, so a design can't hide the note or
    the way to edit it. It can reorder them and add the rest.
- **One arrangement:** Notes designs are always stacked. Validation refuses
  `split` for them, and the builder doesn't offer the Layout row.
- **Starting points:** Sheet (title above the note), Sticky (Paper finish,
  note above its buttons) and Index card (Carbon finish, a divider under
  the title, a caption below).
- **Rendering:** the Notes card itself renders the design. Its parts are
  the card's own pieces, with the design's palette and surface, so editing,
  checklists and Undo behave the same. Editing keeps its header and footer
  in the design's look. The card's size stays the Notes size setting.
- **Builder previews:** a sample note with a checklist, and the empty note.
- **Look format:** a Look that carries `recipes.notes` (a design or its
  explicit reset) is written as format version 29, and a Look below 29 that
  carries it is refused. Workspace apps keep 28.

## Alternatives

- A separate design renderer for Notes, like Music and Focus have. It would
  duplicate the editor, checklist and overflow handling that the card
  already has.
- Making the buttons optional. A Sticky without them couldn't be edited
  unless the note overflowed.
- Devices in the same change. Its card has its own list and grid layouts;
  it gets its own record and change.

## Consequences

An older Halcyon refuses a Look holding a Notes design by its version, with
its usual "newer format" message, and refuses a configuration holding one,
as it would any unknown recipe kind. Looks without one keep their older
version. Removing the design in the builder restores the plain card.
