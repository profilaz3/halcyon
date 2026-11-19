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

The optional magnifying dock applet builds System76's COSMIC App List at
revision `779f920c34d7a309acc12d9bba5bc0c53b0abc53` with Halcyon's icon
magnification patch. The applet is licensed under GPL-3.0-only. The build
recipe and modifications are in `companion/magnifying-dock/`. [Upstream source and license](https://github.com/pop-os/cosmic-applets/tree/779f920c34d7a309acc12d9bba5bc0c53b0abc53).

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

## Clock typefaces

Four typefaces are compiled into the binary for the clock (Settings ›
Clock › Font). Each is the variable font from the
[google/fonts](https://github.com/google/fonts) repository at revision
`cc39c4c2c304bc368c9b7c311ea2bcc6013299fe`, unmodified apart from the file
name, and is licensed under the SIL Open Font License, Version 1.1. None
declares a Reserved Font Name. Each folder keeps the font's own `OFL.txt`.

| Typeface | File | Copyright |
|---|---|---|
| Outfit | `assets/fonts/clock/outfit/Outfit-wght.ttf` | Copyright 2021 The Outfit Project Authors (https://github.com/Outfitio/Outfit-Fonts) |
| Antonio | `assets/fonts/clock/antonio/Antonio-wght.ttf` | Copyright 2013 The Antonio Project Authors (https://github.com/googlefonts/antonioFont) |
| Red Hat Mono | `assets/fonts/clock/red-hat-mono/RedHatMono-wght.ttf` | Copyright 2024 The Red Hat Project Authors (https://github.com/RedHatOfficial/RedHatFont) |
| Bodoni Moda | `assets/fonts/clock/bodoni-moda/BodoniModa-opsz-wght.ttf` | Copyright 2020 The Bodoni Moda Project Authors (https://github.com/indestructible-type/Bodoni) |

The license text, which applies to all four:

```text
-----------------------------------------------------------
SIL OPEN FONT LICENSE Version 1.1 - 26 February 2007
-----------------------------------------------------------

PREAMBLE
The goals of the Open Font License (OFL) are to stimulate worldwide
development of collaborative font projects, to support the font creation
efforts of academic and linguistic communities, and to provide a free and
open framework in which fonts may be shared and improved in partnership
with others.

The OFL allows the licensed fonts to be used, studied, modified and
redistributed freely as long as they are not sold by themselves. The
fonts, including any derivative works, can be bundled, embedded, 
redistributed and/or sold with any software provided that any reserved
names are not used by derivative works. The fonts and derivatives,
however, cannot be released under any other type of license. The
requirement for fonts to remain under this license does not apply
to any document created using the fonts or their derivatives.

DEFINITIONS
"Font Software" refers to the set of files released by the Copyright
Holder(s) under this license and clearly marked as such. This may
include source files, build scripts and documentation.

"Reserved Font Name" refers to any names specified as such after the
copyright statement(s).

"Original Version" refers to the collection of Font Software components as
distributed by the Copyright Holder(s).

"Modified Version" refers to any derivative made by adding to, deleting,
or substituting -- in part or in whole -- any of the components of the
Original Version, by changing formats or by porting the Font Software to a
new environment.

"Author" refers to any designer, engineer, programmer, technical
writer or other person who contributed to the Font Software.

PERMISSION & CONDITIONS
Permission is hereby granted, free of charge, to any person obtaining
a copy of the Font Software, to use, study, copy, merge, embed, modify,
redistribute, and sell modified and unmodified copies of the Font
Software, subject to the following conditions:

1) Neither the Font Software nor any of its individual components,
in Original or Modified Versions, may be sold by itself.

2) Original or Modified Versions of the Font Software may be bundled,
redistributed and/or sold with any software, provided that each copy
contains the above copyright notice and this license. These can be
included either as stand-alone text files, human-readable headers or
in the appropriate machine-readable metadata fields within text or
binary files as long as those fields can be easily viewed by the user.

3) No Modified Version of the Font Software may use the Reserved Font
Name(s) unless explicit written permission is granted by the corresponding
Copyright Holder. This restriction only applies to the primary font name as
presented to the users.

4) The name(s) of the Copyright Holder(s) or the Author(s) of the Font
Software shall not be used to promote, endorse or advertise any
Modified Version, except to acknowledge the contribution(s) of the
Copyright Holder(s) and the Author(s) or with their explicit written
permission.

5) The Font Software, modified or unmodified, in part or in whole,
must be distributed entirely under this license, and must not be
distributed under any other license. The requirement for fonts to
remain under this license does not apply to any document created
using the Font Software.

TERMINATION
This license becomes null and void if any of the above conditions are
not met.

DISCLAIMER
THE FONT SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND,
EXPRESS OR IMPLIED, INCLUDING BUT NOT LIMITED TO ANY WARRANTIES OF
MERCHANTABILITY, FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT
OF COPYRIGHT, PATENT, TRADEMARK, OR OTHER RIGHT. IN NO EVENT SHALL THE
COPYRIGHT HOLDER BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER LIABILITY,
INCLUDING ANY GENERAL, SPECIAL, INDIRECT, INCIDENTAL, OR CONSEQUENTIAL
DAMAGES, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING
FROM, OUT OF THE USE OR INABILITY TO USE THE FONT SOFTWARE OR FROM
OTHER DEALINGS IN THE FONT SOFTWARE.
```
