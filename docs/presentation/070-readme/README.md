# 0.7.0 README illustrations

The built-in OpenAI image generation tool produced this set on 2026-11-14.
The maintainer approved it for the 0.7.0 README with one correction: removal of
the desktop appearance image's header and tagline. That correction uses a
built-in image edit, preserving the three appearances and their labels.
All nine individual images are 1672 x 941 PNGs, approximately 16:9, copied
without further editing or resampling from the tool outputs.

## Files and placement

The first seven files replace the 055 set one for one, with the same filenames.
The final two add sections for clock depth with rain and complete desktop Looks.

| File | Placement | What it shows |
| --- | --- | --- |
| [hero-graphic.png](hero-graphic.png) | Hero | Halcyon identity and abstract wing, grouped Islands, regular Music, System and Notes |
| [desktop-configurations.png](desktop-configurations.png) | 1. Desktop appearance | Rainy Lake, light Paper and Deep Purple arrangements with coordinated surfaces and accents |
| [widgets-elements-dark.png](widgets-elements-dark.png) | 2. Widget design and placement | Clock, Weather, Notes, System, Focus and regular Music on charcoal |
| [panel-elements-dark.png](panel-elements-dark.png) | 3. Dock and panel customization | Islands and Penumbra, compact and expanded Activity island concepts with notifications and file shelf, icon-only edge menu with handle |
| [wallpaper-effects.png](wallpaper-effects.png) | 4. Wallpaper animation | Rain on foreground glass, snow, layered fog and a cloud-shadow landscape study |
| [audio-elements-dark.png](audio-elements-dark.png) | 5. Audio visualization | Radial bars, orbit dots, pulse rings and discrete spectrum bars behind a dock |
| [scheduled-appearances.png](scheduled-appearances.png) | 6. Saved setups and automation | Dawn, midday and evening appearance examples |
| [depth-clock-rain.png](depth-clock-rain.png) | 7. Clock depth and rain on glass | Bare clock behind a wooded ridge, rain on glass in front of the clock and landscape |
| [looks.png](looks.png) | 8. Looks | Rainy Lake and Deep Purple desktop arrangements, with an editorial Look Studio icon shape and tint strip |
| [contact-sheet.png](contact-sheet.png) | Maintainer review | All nine images in a labeled 3 by 3 grid |

## References and provenance

Read alongside the top-level README, the previous
[illustration notes](../055-readme/README.md) and prompts, the native
[055 capture notes](../../screenshots/055/README.md), earlier 030 and 040
capture notes, [release presentation notes](../RELEASES.md), and the main
commit history since `3c7d42eb4`. Source reviewed at
`658010ca3130e581f428842927f88028937d4dcd`.

The previous hero and component illustrations anchor the visual continuity.
The native widget-builder and three circular Music captures anchor component
geometry. Bundled `halcyon-fog.jpg` and `halcyon-night.jpg` anchor the landscape
and palette. The main source and Rainy Lake and Deep Purple preset fixtures
were consulted for feature scope and colors. The older screenshots do not
establish the current Settings layout.

[prompts.json](prompts.json) records each exact submitted prompt, the actual
image inputs and their SHA-256 hashes, output paths, dimensions, sizes and
SHA-256 hashes. It also records the contact sheet assembly command and hash.
Each image used one built-in generation call. The desktop appearance image
then used one built-in edit call to remove the header and tagline. Its original
input is retained at [references/desktop-configurations-with-header.png](references/desktop-configurations-with-header.png),
with the original prompt and hashes in that image's `generation_history`.
No CLI or API fallback was used.
The contact sheet uses ImageMagick only to assemble labeled thumbnails; it
does not modify the individual illustrations.

## Honesty notes and review limits

- These are generated illustrations, not native screenshots, captured shader
  output, pixel-accurate current interfaces or release-validation evidence.
  UI details, icon shapes, typography and widget proportions are approximations.
- Only repository images were supplied. Track titles, artist names, weather,
  file names, notifications, notes and system values are fictional sample data.
  No real calendar events, accounts, personal pictures or external album art
  were supplied. The artwork in Music is generated or based on bundled art.
- The graphic hero's sculptural wing is promotional artwork, not a wallpaper
  or additional application screen. The appearance triptych has no editorial
  header or tagline; its three appearance labels are retained.
- Rainy Lake and Deep Purple depict the implemented feature combinations,
  not exact exports of the built-in presets or guaranteed default placements.
  Paper is an illustrative appearance example, not a new preset claim.
  The compact At a glance composition is approximate.
- The Activity island study separates compact and expanded states for
  explanation. Its popup and file shelf are illustrative summaries, not a
  captured current layout. The edge menu shows the icon-only tray and visible
  handle. It does not demonstrate interaction or autohide behavior.
- The Look Studio strip and the three-dot time-of-day line are editorial
  comparisons, not settings controls, buttons or extra product screens.
- The depth image visibly places the ridge in front of the lower clock digits
  and the rain over both. A still illustration cannot demonstrate natural
  variation over time, animation quality or renderer correctness. Cloud shadows
  are subtle in the fourth weather panel; the image does not establish motion.
- No current Settings Home, live-preview or Arrange screen is fabricated here.
  The available native Settings captures predate the redesign. A current native
  fixture capture should supply that evidence before those screens are shown.
  No new native captures or application builds were made for this illustration set.
- The running desktop and user configuration were not changed. No new native
  capture is implied by adopting these illustrations in the README.

The contact sheet shows the nine selected illustrations. Clock depth with rain
and whole-desktop Looks have their own sections, keeping the existing feature
compositions focused.
