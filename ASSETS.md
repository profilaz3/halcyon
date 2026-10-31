# Artwork and assets

Where every picture in this repository comes from, and how it may be used.

| Files | What | Origin | License |
|---|---|---|---|
| `assets/wallpapers/halcyon-*.jpg`, `assets/previews/halcyon-*-thumb.png`, `assets/looks/*-wallpaper.jpg` | The seven Halcyon wallpapers (night, calm, evening, focus, workbench, paper, rose) | Generated for this project with an AI image model (OpenAI image generation through Codex) from written prompts. Two images were shown to the model as style references only: this project's own welcome art and a third-party app icon. The prompts asked for original landscapes with no content from the references, and none is reproduced | GPL-3.0-or-later, as the rest of the project |
| `assets/wallpapers/{lavender,amber,teal,ocean,rose,graphite}.jpg`, matching `assets/previews/*-thumb.png` | The six palette wallpapers | Rendered procedurally by `scripts/wallpapers.py` | GPL-3.0-or-later |
| `assets/wizard/welcome.jpg`, `assets/wizard/backdrop.jpg` | First-run backgrounds | Generated for this project with an AI image model (OpenAI image generation through Codex) from written prompts | GPL-3.0-or-later |
| `docs/brand/halcyon-icon.png` | App icon (kingfisher over calm water) | Generated for this project with an AI image model (OpenAI image generation through Codex). The COSMIC icon set (pop-os/cosmic-icons) was named as a style reference only | GPL-3.0-or-later |
| `docs/presentation/055-readme/*.png` | README illustrations: desktop appearance, widget design, panel layouts, wallpaper effects, audio visualization and scheduled appearances | Generated with the built-in OpenAI image generation tool on 2026-10-31. Project-native captures were used for widget/layout references; effects, panel comparison and schedules used text prompts. [Prompts and provenance](docs/presentation/055-readme/README.md) | GPL-3.0-or-later; third-party app trademarks remain with their owners |
| `assets/looks/*.jpg`, `docs/brand/looks/shipped/*.png`, `docs/brand/wizard/built/*.png` | Look previews and screenshots | Rendered by this project's own code (look render tool and Settings test renderer) over the wallpapers above | GPL-3.0-or-later |

GitHub presentation artwork and native UI compositions in `docs/presentation/`
are also GPL-3.0-or-later. The `docs/screenshots/desktop-*.png` and
`docs/screenshots/settings-*.png` images are unmodified screenshots taken and
provided by the maintainer on 2026-10-23. App icons and media artwork visible
in them retain their respective owners' rights. See the
[source and capture notes](docs/presentation/README.md).

No fonts are bundled: widget styles name system fonts and fall back when one isn't installed.

Earlier explorations (icon rounds, wallpaper drafts, mockups) and the reference images used for them are not part of the repository.
# Ridge prepared wallpaper

`assets/wallpapers/halcyon-ridge.png` and `assets/depth/halcyon-ridge.png`
were generated for Halcyon with OpenAI image generation on 2026-10-27.
The second image is a prepared foreground mask, not another wallpaper.
Original outputs, prompts and refinement history are retained in
`docs/future-builder/assets/ridge/`. No external photograph, logo or personal
content was used as a reference. Local depth qualification is tracked under FB14.
