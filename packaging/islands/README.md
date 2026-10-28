# Islands — included in Halcyon

The package contains a panel payload, a versioned local compatibility catalog,
read-only diagnostics and a user-session bootstrap. Installation never enables
Islands or restarts a bar. No package script writes a user home.

**0.3.0 supports attempted startup on COSMIC 1.10 and 1.11.** GPU and kernel
versions do not need to match the test VM. Package integrity, native interfaces,
configured applets and startup readiness are checked, with stock-panel fallback
on failure. The recorded complete-session test used COSMIC 1.11 with software
rendering; physical GPUs and a complete 1.10 session are not validated.

From 0.5, install the Halcyon package and open **Settings → Desktop → Dock & panel**.
Choose **Enable for next login**, then log out and back in when convenient.
No additional package download is needed. **Turn off for next login** restores
standard startup without changing your saved bar styles or layouts.

For diagnostics or terminal management, run as your desktop user:

```sh
halcyon-islands status --json
halcyon-islands diagnostics
halcyon-islands enable
halcyon-islands disable
```

Enable is explicit and waits for a later login. Unsupported COSMIC versions or failed startup use the
standard panel while retaining your layout. Installed-but-disabled, incompatible
and damaged installations remain manageable. Diagnostics omit private paths,
configuration content and credentials.

The package-owned XDG autostart stages local installed updates only for verified,
previously enabled owned installations. It runs even with Halcyon widgets stopped.
Activation is promised on the **next login after successful staging**; the first
upgrade from 0.2.0 may therefore need a second login. There is no network fetch or
forced live swap. The selector resolves one immutable verified generation, and
Disable unwinds the original restore chain across updates.

A failure marker at `$XDG_STATE_HOME/desktop-cards-panel/failed` (normally
`~/.local/state/desktop-cards-panel/failed`) keeps the standard
panel selected. Updates and Disable do not clear it. Diagnose the native failure
before an intentional retry; automatic staging is not consent to retry a crash.
External edits and unsupported XDG layouts block automatic migration rather than
being overwritten. Keep the installer records if manually managing a source
installation. After package removal, retained generations include `install.py`
for guarded newest-first restoration of those records.

See [the compatibility contract](https://github.com/profilaz3/halcyon/blob/v0.3.0/docs/COMPATIBILITY.md) for signed archive
trust, native qualification, exact artifact identity, generation recovery and
reproducible regression commands. Fresh installations of the main Halcyon package
enable the signed APT source by default; activating Islands does not change that
choice. Existing disabled or modified sources stay unchanged. See
[system updates](https://github.com/profilaz3/halcyon/blob/v0.3.0/docs/DISTRIBUTION.md)
for enrollment and disabling the source. The Halcyon package is available from GitHub Releases. Upgrades from older split
installations use an empty transitional package; it can be removed after upgrading.
