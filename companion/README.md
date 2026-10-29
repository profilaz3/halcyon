# Dock Islands companion

Dock Islands is an optional style on Desktop > Dock & panel. Standard remains the default. It uses the existing Start, Center and End applet sections, including repeated and unknown IDs. No applet or top bar is added automatically.

[Penumbra](PENUMBRA.md) is a separate optional gradient mod. A panel built with
`--with-penumbra` supports both mods in one host, with independent controls and
preferences. Standard layout can use Penumbra without enabling Islands.

Together has one exact gap and a movable compact group. Separate has individual positions and a minimum gap. Both layouts are saved. Choose an edge, use Arrange, drag the temporary handles, then Done and Apply. Tab selects handles, arrows move or reorder, Home/End place at the ends, and Escape cancels a gesture first. Settings also has move buttons. Cancel restores the draft from before Arrange. Discard and persistent Undo keep their existing meanings.

Apply native screen-edge or applet-list edits before arranging the live dock. Those edits can recreate native applets, so a companion restart or an external structural edit ends Arrange safely. Unapplied drafts retain their baselines. Dock Islands does not write preferences during pointer motion.

Islands use native theme colors and opacity. The current release uses decorative translucency, tint, edges and highlights; native backdrop blur and optical refraction are disabled. Transparent gaps pass clicks through, with separate reveal strips and arrangement handles. No launcher or companion code changes global theme preferences. COSMIC still reserves a rectangular edge strip according to its normal policy.

The current `rounded_blur` capability is false. See [the decorative material notes](../docs/decorative-materials-025.md) for the rendering contract, including ordinary Standard bar layouts. The older [material integration contract](MATERIALS.md) describes historical 0.2.0 behavior.

## Compatibility and build

The patch targets official cosmic-panel `ca20ce3185b3b4404f429d59d429c40b58a2ddef`. The current `cosmic-1.10-1.11-startup-v2` policy in `runtime_compatibility.py` admits bounded startup attempts on supported panel/session families. Packaging binds that policy, the native binary and the helper files into an immutable generation. The full historical exact-cohort catalog remains in source; current-policy packages carry its validated startup policy. See [the compatibility contract](../docs/COMPATIBILITY.md).

Panel and session must share a supported family. Native loader/Wayland checks, configuration consistency and child-owned IPC readiness must pass; unknown families or failed checks select stock. Eligibility is not physical-hardware or full OS certification. New native binaries require fresh final-package acceptance rather than historical evidence. Halcyon and the companion retain separate libcosmic pins and lockfiles. The shared `dock-islands-core` crate contains only schema, IPC types and logical geometry.

Build in a new directory, using Rust 1.93.0 and the native dependencies listed in upstream's README:

```sh
python3 companion/build.py --archive /path/to/verified-upstream.tar.gz --workdir /path/to/new-panel-source --target-dir /path/to/panel-target --offline
```

Omit `--offline` to fetch missing locked dependencies. Omitting `--archive` downloads the exact official archive and verifies its SHA256 before extracting or patching it. `--prepare-only` verifies and applies the patch without building.

## Review and installation

The release handoff contains both binaries, their hashes, frozen sources, native evidence and `companion-manifest.json`. Installation is separate from ordinary Desktop Cards installation. It never changes native applet lists, the sidecar, notes, accounts, widget layout, system binaries or running processes.

Stage into a fixture home first:

```sh
python3 companion/install.py --root /path/to/fixture-home --binary /path/to/cosmic-panel-islands --manifest /path/to/companion-manifest.json
```

After review, the same installer accepts `--user`. It installs a per-user `~/.local/bin/cosmic-panel` selector and a versioned bundle under `~/.local/share/cosmic-desktop-cards/companion`. The session PATH must put `~/.local/bin` before `/usr/bin`. Selection defaults to stock. `--enable-next-login` records the current boot and login identity. Restarts in this login stay on stock; a later login selects the companion. Run this option from the COSMIC desktop session so its login identity is available. The running session is never restarted by this installer.

Exactly one panel host owns applets. The selector checks for an existing host on the same Wayland display and holds a session lock. It verifies the binary hash and source qualification before starting the companion. New bundles then run the checksummed executable's five-second `--islands-probe`: its dynamic loader checks directly linked native libraries and a fresh named Wayland connection checks required globals. This path creates no surfaces and never consumes the session's inherited notification/Wayland descriptors. The running companion must answer its own IPC within 15 seconds; a silent startup is killed and handled as a failed host. Protocol-error flooding is also supervised. These checks cannot prove that every advertised protocol behaves correctly, hence the separate reviewed-source gate. A component crash records a failure marker and selects stock. In COSMIC's supervised session, it exits so cosmic-session can recreate the panel/notifications socket pair before restarting stock. Without a session notification socket, it starts stock after reaping the failed host. The tested notification descriptor is inherited intact; an already authenticated stream is never reused in a different host.

## Restore

The installer returns an `install-record.json` path and retains every replaced companion file. Restore refuses to overwrite a subsequent outside edit:

```sh
python3 companion/install.py --restore /path/to/install-record.json
```

The installer journals intended replacements before writing. An interrupted install can use the same restore command. Restore checks every managed file and backup, including modes, before changing anything. Restoration affects the next login and does not terminate a running host. Selecting Standard in Desktop Cards immediately returns the active companion to its ordinary native layout. Removing the per-user selector also leaves `/usr/bin/cosmic-panel` as the next-session default. Neither operation deletes the saved island layouts or native preferences.

After a crash, `~/.local/state/desktop-cards-panel/failed` keeps future logins on stock. Remove that marker only when deliberately retrying a reviewed component. The companion sidecar is `~/.config/cosmic/io.github.desktopcards.DockIslands/v1/{Dock,Panel}`; Desktop Cards is its only persistent writer. Unsupported versions are preserved and rendered with the ordinary dock after restart.
