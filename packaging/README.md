# Packaging

How Halcyon is laid out on disk, for the Debian package and for a per-user
install from source. Both use the same layout under a prefix: `/usr` for the
package, `~/.local` for a user install. The program finds its files relative
to its own executable (`<prefix>/bin/halcyon`), so the same build works in
both places.

## Supported processors

The `amd64` packages run on **64-bit Intel and AMD PCs** (x86-64). The name
does not mean AMD-only. ARM devices require a separate build. The packaged
release targets Pop!_OS 24.04 with COSMIC.

## Identity

| What | Value |
|---|---|
| Display name | Halcyon |
| Summary | Desktop widgets and themes for COSMIC |
| Debian package | `cosmic-ext-halcyon` |
| App ID (Settings window, launcher, metainfo, icon) | `io.github.profilaz3.Halcyon` |
| App ID (desktop widgets process) | `io.github.profilaz3.Halcyon.Cards` |
| Binary | `halcyon` (`halcyon` runs the widgets, `halcyon --settings [page]` opens Settings) |
| Controller | `halcyon-ctl` (start, stop, settings, focus commands, used by shortcuts) |
| User service | `halcyon.service` |
| Homepage | https://github.com/profilaz3/halcyon |
| License | GPL-3.0-or-later (metadata: CC0-1.0) |
| Maintainer | profilaz3 <261673162+profilaz3@users.noreply.github.com> |

## Layout under a prefix

| Path | Contents |
|---|---|
| `bin/halcyon` | The program |
| `bin/halcyon-ctl` | Controller (Python 3) |
| `lib/cosmic-ext-halcyon/` | Helpers run by the program: `calendar-sync.py`, `project-status.py` |
| `share/cosmic-ext-halcyon/` | `google-calendar-setup.html`; optional `google-calendar-client.json` (publisher OAuth client, added at release build time, never committed) |
| `share/backgrounds/halcyon/` | Thirteen JPEG wallpapers and the prepared `halcyon-ridge.png`; identical assets for Debian and per-user installs |
| `share/applications/io.github.profilaz3.Halcyon.desktop` | Launcher (opens Settings) |
| `share/metainfo/io.github.profilaz3.Halcyon.metainfo.xml` | AppStream metadata |
| `share/icons/hicolor/<size>x<size>/apps/io.github.profilaz3.Halcyon.png` | Icon at 48, 64, 128, 256 and 512 px |
| `share/doc/cosmic-ext-halcyon/` | `copyright`, `changelog.gz`, `THIRD_PARTY.md`, `ASSETS.md`, dependency license texts |

The user service lives at `/usr/lib/systemd/user/halcyon.service` (package)
or `~/.config/systemd/user/halcyon.service` (user install). It is never
enabled by the package; Halcyon's first-run setup enables it for the person
who runs it.

## Per-user data (created by the program, kept on uninstall)

`~/.config/halcyon/`, `~/.local/share/halcyon/`, `~/.local/state/halcyon/`,
`~/.cache/halcyon/`, runtime files in `$XDG_RUNTIME_DIR/halcyon/`. Earlier
versions used `cosmic-desktop-cards` for these; Halcyon copies them over once,
on first start, and leaves the old folders in place.

## Islands add-on

`cosmic-ext-halcyon-islands` is a separate, optional package (see
`packaging/islands/`). Its reviewed COSMIC source/family policy and native startup checks determine
compatibility; a matching version string alone does not establish compatibility.
Activation is per person for the next login. See [the add-on guide](islands/README.md).


## Default system updates

Release main packages since 0.3.0 include reviewed update templates under
`/usr/share/cosmic-ext-halcyon/updates/`. On a clean install, the package enables
the signed Halcyon source once, offline. The script preserves existing bootstrap
ownership and disabled, deleted, renamed or administrator-edited sources. It does
not enable unattended upgrades or install/activate Islands.

The public installation receipt is `/var/lib/halcyon/updates/default-source.json`.
Package removal disables only its unchanged generated source; purge removes only
unchanged generated trust/pin files when safe. [Ownership, opt-out and recovery](../docs/DISTRIBUTION.md#default-enrollment-ownership).

## Build mode

Release package `Version`, release manifest version and `Cargo.toml` must match
exactly. Debian revisions and local suffixes belong to `--development` packages;
the release app currently verifies its installed version against its Rust version.
From 0.5, the Islands payload shares the main package version and ownership.
The final gate checks the included manifest, both executable hashes and activation
helper bytes. The old download pin remains solely for historical split-package
verification; the app no longer downloads a companion.

Prefer `python3 scripts/install.py` for per-user source work; it adds no APT source.
The Debian builder requires an explicit mode:

```sh
python3 packaging/build-deb.py --development --binary target/release/halcyon --version 0.4.0~local1 --out dist
# Release assembly only, after reviewing the binary and public enrollment inputs:
python3 packaging/build-deb.py --release-updates --binary target/release/halcyon \
  --islands-binary PATH/cosmic-panel-islands --islands-manifest PATH/companion-manifest.json \
  --islands-licenses PATH/THIRD_PARTY_LICENSES.txt --version VERSION --out dist
python3 packaging/islands/build-transition.py --version VERSION --out dist
```

`--development` produces `cosmic-ext-halcyon-dev`, with no update templates or
maintainer enrollment/removal hooks. It conflicts with and replaces the main
package's files, provides no release package identity, and receives no Halcyon
release updates. Installing it over a release is an explicit package-manager
replacement; an installed Islands package requires the real main package.
APT consequently proposes removing Islands when replacing its required main
package with the development package; review that transaction explicitly.
Returning to releases likewise replaces the development package explicitly.
Per-user builds can coexist, so launch the intended executable explicitly.

CI uses the development mode. Release assembly uses `--release-updates`, which
retains the existing default-enrollment and administrator-preservation behavior.
Do not reuse a published version for changed bytes.

Historical split releases use `islands/download.json` to bind the separate
companion download. Bundled releases no longer use this download path; both
executables ship in the main package and update through signed APT.

## Bundled releases from 0.5

The main release package owns both executable payloads. Build the shared panel
with `companion/build.py --with-penumbra` so both optional effects are included.
The native input must
match its reviewed manifest; package assembly refreshes helper hashes and records
`package_owner: cosmic-ext-halcyon`. The Islands library path remains stable.
The development package remains isolated and does not claim bundled activation.

Publish the main download plus the empty upgrade bridge in the APT archive. The
release manifest uses `packaging: bundled-islands`, `islands_package` names the
main `.deb`, and the bridge has kind `transition-deb`. The final gate binds both
executables to that main package, checks the bridge has no payload or scripts,
and still requires fresh acceptance of the final artifacts. Source archives and
build provenance must include both payload recipes and native dependency notices.
No old native evidence automatically qualifies a new package.
