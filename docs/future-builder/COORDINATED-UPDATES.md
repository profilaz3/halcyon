# Halcyon updates

The local 0.5 implementation uses [one Halcyon package](BUNDLED-ISLANDS.md).
Islands is included and optional to activate. **App settings → Updates** shows
one update status. COSMIC Store or APT owns installation and authentication.

A main-package update carries both executables. The empty upgrade bridge for older
Islands installations does not create a separate app update offer. Fresh installs
never need the bridge. Disabled update sources remain disabled.

Restart-app and next-login states stay distinct. Halcyon never logs out or restarts
the desktop automatically. Activation still requires verified package ownership,
a matching manifest and the existing guarded native checks.

The earlier two-package offer matching and in-app download flow are removed from
0.5. Published 0.4.x releases retain their original behavior and immutable evidence.
Validation for this change is tracked in [Bundled Islands](BUNDLED-ISLANDS.md).
