# Adding a handcrafted look

Halcyon's built-in looks come from two places: a few looks written in code
(`src/desktop/profiles/presets.rs`) and handcrafted looks shipped as data
files in `resources/looks/`. Adding a look of the second kind needs no code.
You make it in the Look studio, export it, drop the file in, and build.

## 1. Make the look in the Look studio

1. Open Settings, then Looks.
2. Choose ⋯ and then **New look from my desktop**, or **New plain look**.
   To start from a look that already ships, select it and choose
   **Edit a copy in studio…**.
3. Pick each part by its picture: wallpaper, weather, panel, dock, windows,
   colors, font, icons, widget style, widget layout and desktop icons. Every
   picker offers your desktop's version, every look's version and the part's
   own simple choices. The preview on top shows the draft; nothing on your
   desktop changes until you choose **Review and apply…** and then Apply.
4. Name the look and give it a one-line description at the top. The
   description is what people read under the look's picture in the gallery.

### Icons: theme, shape and folders

The Icons picker offers three kinds of choice, each drawn as a row of
sample icons:

- **Icon theme**: the base theme, for example Cosmic or Pop.
- **App icon shape**: Original, Circle, Squircle or Rounded square. App
  icons are redrawn in that shape.
- **Folders**: Accent folders (folders take the look's accent color) or
  Theme folders (the theme's own).

Each choice changes only that setting and keeps the other two. A look that
sets no shape or folder choice leaves the desktop's as they are, and so do
looks saved before format version 22.

Halcyon draws shaped icons into a theme of its own (`Halcyon-Shaped`). That
name is never saved: capture, saved looks, exports and Make built-in record
the base theme plus the shape and folder choice, so the look works on any
computer. Applying the look builds the shaped theme there, and Undo puts the
previous theme, shape and folders back. A look saved by an older version
that names `Halcyon-Shaped` applies its base theme.

In the parts review, the shape and folders belong to **Appearance**, with
the icon theme. Leaving Appearance out keeps the desktop's theme, shape and
folders, and the preview shows them.

## 2. Make it built-in

Open the studio's ⋯ menu and choose one of:

- **Make built-in** writes `$XDG_DATA_HOME/halcyon/built-in-looks/<id>.json`
  (usually `~/.local/share/halcyon/built-in-looks/<id>.json`). The studio
  shows the path with a **Copy path** button.
- **Make built-in as a file…** asks where to write it.

The ID is made from the look's name: lowercase letters, digits and single
hyphens, for example "Evening harbor" becomes `evening-harbor`.

A built-in look must look the same on every computer and say nothing about
the one it was made on, so Make built-in:

- refuses a personal wallpaper (your own picture or folder, or a COSMIC
  system picture). It offers **Choose a Halcyon wallpaper**, which opens the
  wallpaper picker on Halcyon's bundled wallpapers. Plain colors and
  gradients are fine.
- leaves out anything local: the display arrangement, per-display
  backgrounds, display-specific island positions, where the look was mixed
  from and who shared it. The studio lists what it left out.
- keeps fonts and icon themes that a default Pop!_OS or COSMIC install
  doesn't have, and says that they fall back to the current ones where they
  are missing. Prefer Open Sans, Fira Sans, Fira Mono or Noto Sans Mono, and
  the Cosmic or Pop icon themes.
- keeps the app icon shape and folder choice. On Cosmic or Pop they need
  nothing installed; Halcyon draws them on the computer that applies the
  look. A look with either is written as format version 22.

Nothing else is written, and your desktop is not touched.

## 3. Ship it

1. Copy the file into `resources/looks/`. The file name is the look's stable
   ID, so keep it: renaming the file later makes it a different look for
   anyone who saved or scheduled it.
2. Build Halcyon. `build.rs` embeds every `resources/looks/<id>.json`, and
   the look appears in the Looks gallery after the code looks.
3. Run the tests. `built_in_looks_plan_cleanly_on_a_default_install` and the
   tests in `src/desktop/profiles/built_in.rs` check every shipped file: it
   parses, it plans on a default install without blocking problems, it uses
   a bundled wallpaper and it carries no personal data.

The build refuses a file whose name isn't a valid ID. The app refuses, and
the tests fail on, a file that:

- doesn't parse as a look,
- takes an ID a code look owns (`halcyon`, `calm`, `evening`, `focus`,
  `paper`, `rose`, `tidal`, `paper-studio`, `carbon`, `rain`, `snow`, `fog`,
  `presentation`, `ridge`, `spine`, `adaptive`) or starts with `adaptive-`,
- carries personal data.

`resources/looks/harbor.json` is a complete example.

## Editing a shipped look

Select it on Looks and choose **Edit a copy in studio…**. Change what you
like, keep the name (so the ID stays the same), choose **Make built-in** and
copy the new file over the old one in `resources/looks/`.
