# README maintenance

The root README is a stable introduction and installation entry point. Release
notes describe what changed in a release.

## Keep stable

- Keep the title, overview, section order and feature groups across releases.
- Use `/releases/latest` for downloads and `/releases` for release notes. Package
  filenames in the README use `<version>` rather than a hard-coded release.
- Keep the feature gallery stable. Replace an image when the feature changes
  materially or a presentation change is requested. An archived path containing
  a version is not a reason to replace it. Label generated artwork as illustration
  and link native captures separately; illustrations are not feature-validation
  evidence or a promise of exact visual output.
- Describe capabilities directly. Use feature names as headings; avoid slogans,
  sales language and second-person claims about customization.
- Lead with "Complete Desktop Customization". Keep numbered feature sections,
  each with a relevant supporting image, instead of an unstructured component
  inventory. Installation and developer links follow the feature sections.
- Match each image's framing to its feature. Show widgets as a collection of
  individual widgets, bars as close-ups, and visualizers as isolated components.
  Do not put a full desktop or decorative landscape behind every feature.
  Whole-desktop compositions belong with the hero, appearance and saved setups; wallpaper
  effect examples may show the affected wallpaper itself. Use dark charcoal
  presentation backgrounds for component illustrations; avoid white or pale sheets.
  Individual widgets may retain their own light theme finishes.
- Keep setup instructions in focused guides: Calendar in `docs/CALENDAR.md`,
  installation/updates in `docs/DISTRIBUTION.md`, and privacy in `docs/PRIVACY.md`.
- Keep optional Islands and its next-login activation clear. Keep the Privacy
  guide linked; reporting changes belong in that guide and the existing consent
  screens, not in README or release-note feature promotion.

## During a release

1. Write concise product changes in `docs/releases/<version>.md` and render the
   release notes with the existing template. Put versioned download links there.
2. Check README links, platform requirements, installation steps and feature
   claims against the release. A routine version change needs no README edit.
3. If behavior changes, update only the affected sentence and its detailed guide.
   Add a feature line only for a lasting new capability; put smaller additions
   and bug fixes in release notes.
4. Keep release checks and development progress in their existing documents.
   Do not turn the README into a changelog or a test report.

The README describes released behavior. Planned work belongs in
[`docs/PLANNED_WORK.md`](../docs/PLANNED_WORK.md).
