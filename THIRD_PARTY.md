# Third-party software

Halcyon's own code is licensed GPL-3.0-or-later (see `LICENSE`). One source
file adapted from another project, `src/rain/glass.wgsl`, is GPL-3.0-only (see
below), so the crate declares `GPL-3.0-or-later AND GPL-3.0-only`. It is built from the
Rust crates pinned in `Cargo.lock` (534 packages for Linux x86_64, checked
2026-10-22 from the locked dependency set). Every license among them is
compatible with distributing the combined program under the GPL version 3:

| License (as declared) | Packages |
|---|---|
| MIT and/or Apache-2.0 (in their various spellings and combinations, incl. Zlib, ISC, BSD, 0BSD, Unlicense, CC0 alternatives) | the large majority |
| Unicode-3.0 | 18 (ICU and Unicode data crates) |
| GPL-3.0-only | `cosmic-client-toolkit`, `cosmic-protocols` (System76); `cosmic-panel-config`, `xdg-shell-wrapper-config` (System76 COSMIC Panel workspace, which declares GPL-3.0-only) |
| MPL-2.0 | `option-ext` |
| CDLA-Permissive-2.0 | `webpki-roots` (Mozilla's CA list) |
| Apache-2.0 OR GPL-2.0-only | `self_cell` (used under Apache-2.0) |
| MIT OR LGPL-3.0-or-later | `configparser` (used under MIT) |
| No license field in the manifest | `libcosmic`, `cosmic-config`, `cosmic-config-derive`, `cosmic-theme`, `cosmic-settings-daemon`, `iced_accessibility`, `dnd`, `mime`, `apply` (System76 git dependencies; their repositories carry MPL-2.0 or MIT license files. The iced crates come from `profilaz3/iced`, a fork of System76's `pop-os/iced` with one fix, under the same MIT license) |

Because some dependencies and the rain glass shader are GPL-3.0-only,
binaries built from this repository are distributed under the GPL version 3
only. Halcyon's own files keep their GPL-3.0-or-later grant, so they can be
reused under a later GPL version apart from the GPL-3.0-only parts.

Release packages include the full license text of every dependency
(generated from `Cargo.lock` at release time) next to the binary.

Artwork is described in `ASSETS.md`.

The clock's scene model runs with [tract](https://github.com/sonos/tract)
(`tract-onnx` 0.23.10, MIT OR Apache-2.0) and its dependencies, all under
licenses in the table above. The model itself, Depth Anything V2 Small
(`onnx-community/depth-anything-v2-small`, revision
`4472b7362082ad9968fee890ca0f1e5aca36b93d`, Apache-2.0), is not part of any
package: Settings downloads it only when asked, checks its size and SHA-256,
and keeps it under `~/.local/share/halcyon/models/`.

The optional Penumbra panel mod in `companion/patches/0002-penumbra-panel.patch`
adapts the renderer and configuration schema from
[motixo/penumbra-panel](https://github.com/motixo/penumbra-panel), revision
`3ef32c8ba54c2e296fc18840a419bc5c11fc58bb`, under GPL-3.0-only. It is based on
System76's COSMIC Panel. The native patch retains its SPDX license notice;
the full GPL version 3 text is in `LICENSE`. Halcyon's Penumbra controls retain
the original preference format for interoperability.

The rain-on-glass wallpaper effect's water lens outline and lighting
(`lens_shape` and `lens_light` in `src/rain/glass.wgsl`) are adapted from the
rain-glass shader in [motixo/cosmic-forge-shaders](https://github.com/motixo/cosmic-forge-shaders)
(`shaders/rain-glass.frag`, revision `e3dba3b36aa03afb14c0c0641bef8cd61bd3f996`),
under GPL-3.0-only. They were ported from GLSL to WGSL and are evaluated per
drop in Halcyon's own water field. Halcyon has modified both: the outline's
lobes scale with a roughness and a time-varying oscillation, with a teardrop
taper for sliding drops, and the lighting shows a flipped, slightly magnified
view with a soft highlight, a light caustic and a thin dark rim. The file
lists these changes in its header. The file retains its SPDX license notice
and the Cosmic Forge copyright; the full GPL version 3 text is in `LICENSE`.
Nothing from BigWings' Heartfelt Shadertoy (CC BY-NC-SA), which Cosmic Forge
names as a visual reference, is used: Halcyon's drop placement, motion,
condensation and composition are its own.
