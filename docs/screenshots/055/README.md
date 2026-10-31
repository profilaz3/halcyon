# 0.5.5 documentation captures

Real native captures and renderer outputs, with synthetic media data. The files
are copied unchanged; no interface elements were painted in or generated.

- `desktop.png`: Clock, circular Music, audio spectrum and a stock COSMIC dock in
  an isolated nested desktop, captured from the current local executable on
  2026-10-31. Audio comes from a private test output. This is a documentation
  capture, not evidence of physical-device or package acceptance.
- `rain.png`, `snow.png`, `fog.png`, `cloud-shadows.png`: outputs from the actual
  optimized GPU effect renderer. They are still frames of animated effects.
- `music-bars.png`, `music-dots.png`, `music-rings.png`: native widget-tree renders
  with sample track information and spectrum values.
- `widget-builder.png`: native Settings widget-tree capture with sample media
  and the Paper finish, from 2026-10-30.

The README also uses the existing [Mix board capture](../040/README.md) to show
setup composition. Its scene contains illustrative widget content.

The desktop capture's executable SHA-256 is
`6fcbfc77f2eff95bc80daf7d536b9fd54d3ac6bdd9a7c2f641034ce2d52b0faf`.
All desktop fixture processes were reaped. No installed application, host settings
or user session was changed.

Wallpaper and icon provenance: [ASSETS.md](../../../ASSETS.md). Project-created
artwork and UI use GPL-3.0-or-later; third-party applets retain their original rights.
