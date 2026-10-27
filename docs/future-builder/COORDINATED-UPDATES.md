# Coordinated app and Islands updates

Implemented locally for 0.5. **App settings → Updates** shows the installed app
and companion together, with one status and next action. The OS package manager
continues to own installation and authentication.

When Islands is installed, an offered update must leave both packages on the same
version. A companion-only catch-up is allowed when it matches the installed app.
A partial or mismatched offer is refused. Without Islands, app updates continue
normally and do not install the optional companion.

Restart-app and next-login states stay distinct. Next login is shown only after
validated activation evidence; Halcyon never logs out or restarts the desktop on
its own. Package installation, disabled/unknown compatibility and helper failures
retain their existing guarded flows.

39 focused tests pass, including private PackageKit bus fixtures, installed/absent
companion offers, version mismatches, preparation and activation. Compact and wide
native views were inspected. No host packages or update sources were changed.
