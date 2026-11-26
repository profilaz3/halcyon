# Note pages and inline Markdown

## Context

The desktop note is one text in one owner-only file, saved compare-and-swap
against edits made elsewhere, with Settings drafts kept beside it. The card
already reads `# headings`, `- bullets` and `[ ] checklists`. The audit asks
for more than one note and richer formatting.

## Decision

- **Pages, not files.** `notes.pages` (off by default) splits the note at
  its top-level `# Heading` lines: each heading starts a page named by it,
  and text before the first heading is a page called Notes. The card shows
  one page with ‹ › to move between them; the editor still edits the whole
  note. The page shown is kept in memory, starting at the first.
- **Inline Markdown on the card**: `**bold**`, `*italic*` or `_italic_`,
  `` `code` ``, and numbered lists (`1. ` or `1) `). Markers without a
  closing pair stay as typed. The editor stays plain text.

## Alternatives

- A file per note. Every save, conflict, draft and Settings path assumes one
  note; separate files would multiply them for what headings already name.
- Always splitting at headings. A note that uses headings as section titles
  would change how it reads; the switch keeps that a choice.

## Consequences

The file format and saving are unchanged, so nothing is migrated and a note
written by an older Halcyon reads the same. A page only exists while its
heading does: renaming the heading renames the page.
