# Distribution and system updates

The main app and optional Islands package are available from
[GitHub Releases](https://github.com/profilaz3/halcyon/releases/latest) and signed APT.
The signed channel is `https://profilaz3.github.io/halcyon/apt`, suite `noble`,
component `main`, architecture `amd64` (Intel and AMD x86-64 PCs).

For a new installation, open the
[latest release](https://github.com/profilaz3/halcyon/releases/latest), download
`cosmic-ext-halcyon_<version>_amd64.deb`, and open it with COSMIC Store to install.
Then open Halcyon from the launcher. [Terminal fallback](#install-a-downloaded-package).

**Release packages since 0.3.0 enable signed system updates on a clean install.** No bootstrap
download is needed. The main package contains the reviewed public key, scoped
source and restrictive pins. Its installation script enrolls once, offline,
inside the original install transaction. The system may fetch ordinary missing
dependencies from its configured repositories.

Repository enrollment makes future releases available. **Check for updates**
refreshes metadata and reports availability. **COSMIC Store → Updates** or APT
installs updates; automatic installation remains the administrator's OS policy.
Halcyon never collects a password or turns on unattended installation.

Existing enrollment, disabled/deleted sources and administrator modifications
are preserved. A clean upgrade from an unenrolled 0.2.5 manual package enrolls;
a previous enrollment decision is retained. Islands stays optional. Source and
per-user installs do not add system repositories. For older manual installations
or recovery, use the explicit terminal enrollment below. The app does not
re-enable a source that an administrator disabled or removed.

## Install a downloaded package

If COSMIC Store cannot install the downloaded file, open a terminal in its folder
and run the following, replacing `VERSION` with the version in its filename:

```sh
sudo apt install ./cosmic-ext-halcyon_VERSION_amd64.deb
```

APT installs the local package and resolves missing dependencies through the
system repositories. An internet connection is needed for those downloads.

## Enroll an older/manual installation

Download both bootstrap packages from the separate
[apt-bootstrap-0.2.5-1 release](https://github.com/profilaz3/halcyon/releases/tag/apt-bootstrap-0.2.5-1):

```sh
curl -fLO https://github.com/profilaz3/halcyon/releases/download/apt-bootstrap-0.2.5-1/halcyon-archive-keyring_0.2.5-1_all.deb
curl -fLO https://github.com/profilaz3/halcyon/releases/download/apt-bootstrap-0.2.5-1/halcyon-update-source_0.2.5-1_all.deb
printf '%s\n' \
  '4a0b973025c98b03d4ed901bc499f1af61c001dcdd1f1d7885b4009811ac2262  halcyon-archive-keyring_0.2.5-1_all.deb' \
  'fad29b1760e53d7bf1115829d71f2568f7c208f6176b6480aee3a0c85d2d00af  halcyon-update-source_0.2.5-1_all.deb' \
  | sha256sum -c -
sudo apt install ./halcyon-archive-keyring_0.2.5-1_all.deb ./halcyon-update-source_0.2.5-1_all.deb
sudo apt update
```

Public certification fingerprint: **A1EB 6576 D959 5ECA 5750 9CE1 4EB2 B42D F234 CB16**.
Signing subkey: `610B73343D6F40603553A295A4658CCECBC306AB`.
The independently published checksums above bind the bootstrap downloads to the
reviewed public key. To inspect the included key without importing it:

```sh
dpkg-deb --fsys-tarfile halcyon-archive-keyring_0.2.5-1_all.deb \
  | tar -xOf - ./usr/share/keyrings/halcyon-archive-keyring.gpg \
  | gpg --show-keys --with-subkey-fingerprint
```

The source uses `/usr/share/keyrings/halcyon-archive-keyring.gpg` through scoped
`Signed-By`. It never uses `apt-key` or global trust. Selection preferences allow
only `cosmic-ext-halcyon`, `cosmic-ext-halcyon-islands`, `halcyon-archive-keyring`
and `halcyon-update-source` from this origin. No COSMIC version holds are added.

## Install or update

Once the Halcyon source is enabled and its package lists are refreshed:

```sh
sudo apt install cosmic-ext-halcyon
```

Existing package installations update normally through APT or **COSMIC Store →
Updates**. The app's Updates row offers an explicit check and opens COSMIC Store
for app upgrades. The terminal enrollment below installs only the two bootstrap
packages through APT. Halcyon never collects passwords. Unattended installation depends
on the user's OS policy. Store search discovery is separate from system updates;
AppStream metadata and icons are provided, but a native Store search trial has not
been claimed by this deployment work.

Islands remains optional. Enrollment and installing/updating the main app do not
install it. To request it explicitly:

```sh
sudo apt install cosmic-ext-halcyon-islands
```

Installing the companion does not enable it or restart a panel. Already-installed
Islands receives normal package updates. Existing enabled users use the guarded
per-user refresh and next-login activation; disabled users stay disabled. There
is no forced logout or root package script that edits user home directories.

Manual `.deb` downloads remain supported. Source/per-user installs remain distinct
from `/usr` package installations; the app checks its actual running executable
before offering package update status. Enrolling does not replace a source build.

## Stop using this channel

For a package with default enrollment, edit
`/etc/apt/sources.list.d/halcyon.sources` as administrator and add `Enabled: no`.
Upgrades preserve that choice and also preserve a deleted source. Remove that
line to enable the source again. Disabling enrollment does not uninstall Halcyon.

For the legacy, separately installed bootstrap, disable the source first and
review all active APT sources before removing its keyring package. In particular,
the published `halcyon-archive-keyring` **0.2.5-1** package does not protect a key
still referenced by an administrator-renamed source during purge. Do not purge
that keyring while any active source uses its `Signed-By` path. Leave the keyring
installed when only disabling updates; a public key by itself enables no source.

Removing the legacy source package preserves its source as an ignored
`halcyon.sources.removed.*` recovery file. Other active source copies require an
administrator's explicit decision. This legacy behavior is distinct from the
reference-aware default-enrollment removal described below.

## Release identity, source and maintenance

APT distributes the **exact** reviewed app/companion bytes from GitHub releases.
The 0.4.0 pair uses `cosmic-1.10-1.11-startup-v3`; retained versions keep their
original approvals and scope. Published releases remain immutable.

The original release supplies corresponding source, licenses, provenance and
focused evidence. The separate bootstrap release supplies its own corresponding
source and GPL copyright/license. The archive landing page and retained
`release-assets.json` records link to immutable GitHub release assets and hashes,
so large source/evidence archives do not consume the Pages site budget.

[APT operations](APT-OPERATIONS.md) documents the approved input manifest,
protected signing environment, daily seven-day metadata renewal, expiry checks,
retention, rotation and exact deployment commands.
[Updates integration](UPDATES-HANDOFF.md) records the already-published app contract
and the limited Store discovery claim.


## Default enrollment ownership

The main package owns its reference files under
`/usr/share/cosmic-ext-halcyon/updates/`. The one-time receipt is
`/var/lib/halcyon/updates/default-source.json`; it contains only public file hashes
and installation state. Generated source, pins and key use the same paths and
scope as the existing bootstrap. Installed-state detection verifies the running
binary, dpkg-owned reference files, receipt and generated bytes before enabling
update checks. Legacy bootstrap ownership remains accepted independently.

No source file is installed over another package's conffile during unpack. Before
enrollment, the script checks for existing files, renamed sources, duplicate
configuration and dpkg enrollment records. Interrupted creation can resume only
when already-written files still match. A completed installation never silently
recreates a source, key or pin deleted or edited later.

Removing the main package disables an unchanged source it generated, retaining
an ignored recovery copy and its key. Reinstallation preserves the disabled state.
Purge removes unchanged generated pins/key and the receipt when no active source
needs them. Administrator changes and bootstrap-owned files are retained. Recovery
of a removed installation can use the explicit bootstrap above. If Python itself
has already been removed, the removal script leaves the public configuration
intact rather than leaving an active source without its key.

## Islands in 0.5 (in development)

The next release includes Islands in the Halcyon package. Enable it in
**Desktop → Dock & panel → Enable for next login**. There is no second download.
The current panel keeps running until the next login. Existing enabled or disabled
choices and layouts are preserved. App and Islands updates arrive together.

Existing two-package installations receive an empty upgrade bridge through APT;
new installs only need Halcyon. See the [implementation and migration contract](future-builder/BUNDLED-ISLANDS.md).

## Islands in published 0.4.x releases

Available since Halcyon 0.4.0.

Open **App settings → Updates → Get Islands…**. Halcyon downloads and verifies
one reviewed companion package, checks the proposed system changes, then asks
**Install Islands**. The system handles authorization. **Enable for next login**
is a separate per-user choice; the existing helper checks native compatibility,
ownership and previous failures before staging activation. Log out and back in.

This path works from a verified system package with the same version as the
running app. It does not enable or rewrite APT sources. An intentionally disabled
source stays disabled. The exact companion is pinned in the app, so the first
install does not require refreshing newly enrolled Halcyon package lists.
Future companion updates use the signed source when it is enabled.

The normal supported desktop already provides the companion's system dependencies.
If the system simulation proposes other package changes, Halcyon directs you to
the package manager and this guide to review them. It does not install over a
manual companion, repair a half-configured package, downgrade COSMIC, or choose
an arbitrary newer companion. Cancel is available before installation. After an
interruption, check system installation status before retrying; successful OS
installation is verified against dpkg before activation is offered.

For an absent companion on the signed repository path, an administrator can use:

```sh
sudo apt update
sudo apt install cosmic-ext-halcyon-islands
halcyon-islands enable
```

A connection is needed for uncached packages or metadata. Offline failures keep
Halcyon usable; reconnect and retry. Native Store install/PolicyKit prompts and a
complete fresh-machine journey remain release acceptance checks for the next build.
