# Devices in the Widget Builder

## Context

ADR 0017 brought Notes into the Widget Builder by letting the Notes card
render a design from its own parts. Devices is the other widget the audit
named first. Its card shows a title, an "N more →" link when not every
device fits (it opens the full reading), and a grid of battery rings sized
to the card. An empty card says where devices come from.

## Decision

- **A new kind:** `Kind::Devices` (`/recipes/devices`, `Applied::devices`,
  version 1), a tool kind, so finishes apply.
- **Its parts:**
  - `devices_header`, "Title and count": the title and "N more →".
  - `batteries`, "Batteries": the ring grid, or the empty message.
  - `caption`, `divider` and `space`.
  - Both of the first two are required, so a design can't hide devices
    without saying so. Designs are always stacked, as Notes designs are.
- **Starting points:** Rings (title above the rings), Shelf (Tidal finish,
  rings above the title) and Label (Carbon finish, a divider under the
  title, a caption below).
- **Rendering:** the Devices card renders the design from its own pieces,
  with the design's palette and surface. The grid is sized to the room the
  other parts leave, and "N more →" still counts what doesn't fit.
- **Builder previews:** three sample devices, or none connected.
- **Look format:** a Look carrying `recipes.devices` (a design or its
  explicit reset) is written as format version 30, and a Look below 30 that
  carries it is refused. Notes designs keep 29.

## Alternatives

- Choosing ring or list rows as parts. The card picks its tile size from
  the room it has; a second layout is a card change, not a design.
- Sharing format 29 with Notes. If Notes ships first, an app that reads 29
  would meet a Devices design it doesn't know.

## Consequences

An older Halcyon refuses a Look holding a Devices design by its version,
and a configuration holding one as an unknown recipe kind. Removing the
design in the builder restores the plain card.
