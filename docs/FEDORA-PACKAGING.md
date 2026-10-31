# Fedora packaging and updates

Available for Fedora 43 and 44 COSMIC on x86-64. Fedora 44 support starts with 0.5.6. Download the RPM from
[GitHub Releases](https://github.com/profilaz3/halcyon/releases/latest). The signed
update source selects `rpm/fedora/$releasever/x86_64/` for the installed Fedora version.
Choose the `.fc43` or `.fc44` download to match it.
Atomic variants and other RPM distributions are not covered.

## One package

`cosmic-ext-halcyon` contains Halcyon, Islands and Penumbra. The user enables the
panel component in Settings; installation alone does not activate it. Existing
enabled/disabled choices and layouts survive updates. Activation or replacement
of an enabled panel takes effect at the next login, without ending the session.

The package enrolls in the signed update source once, using included files.
Later updates preserve disabled, deleted, renamed and administrator-edited
sources. Removal leaves public trust material available to other sources and
records the disabled decision so reinstall does not undo it.

Settings uses the same **Open system updates** action for DEB and RPM. It opens
COSMIC Store; the user selects Updates there. If Store is unavailable, Settings
offers a copyable native command for the verified package installation:

```sh
sudo dnf upgrade --refresh cosmic-ext-halcyon
```

DNF/PackageKit owns installation, dependencies and authorization. Key-import
prompts remain part of the system's trust flow. Halcyon does not enable unattended
upgrades, reset a disabled source or restart the desktop.

## Build and signing

Build both executables natively on Fedora with Rust 1.93.0, locked dependencies
and the pinned native patches. `packaging/build-rpm.py` accepts their checksummed
Fedora build record, exact native manifest, catalog and license notices. It uses
the shared DEB payload staging functions without converting a DEB. Product
version must match Cargo; native epoch/version/release remains separate.

Each Fedora startup policy binds its platform and native ELF. Fedora 43 supports
COSMIC 1.10 and 1.11; Fedora 44 supports COSMIC 1.11.
Debian qualification is not reused. RPM database changes still invalidate
admission; SQLite reader bookkeeping does not. Unrelated multilib packages and
retained older kernels do not hide otherwise unambiguous COSMIC measurements.

`scripts/rpm/prepare-repository.py` writes a new local repository directory,
signs a copied RPM, verifies it with a private keyring, builds SHA-256 metadata,
and signs/verifies `repomd.xml`. The candidate manifest hashes the final signed
bytes and records `published: false`. It does not upload or change the APT feed.
Both `gpgcheck` and `repo_gpgcheck` are enabled in the packaged DNF source.

Publication adds a signed `release.json` binding both RPMs, repository files,
corresponding source and acceptance evidence. `scripts/rpm/distribution.py`
verifies those bindings, the package signature and the repository signature
against the approved archive key. The approved distribution pins this immutable
repository archive. APT and DNF are deployed together, so daily APT renewal
preserves both reviewed RPM repositories. Version-2 reviews require both Fedora
targets; historical version-1 reviews retain their original Fedora 43 contract.

For key rotation, deliver overlapping old/new public keys through a package
signed by the old key before changing the repository signer. Enrollment replaces
only its unchanged owned key and source. Interrupted key updates resume;
administrator changes and removed files are preserved. Modified enrollment
needs explicit administrator action during rotation.

## Validation and release boundary

Local checks cover native RPM building, installed identity and EVR ordering,
signed DNF installation/reinstall/upgrade, activation staging and on/off changes,
preservation of a disabled source and user files, and enrollment interruption,
removal, rename and key-rotation fixtures. Shared DEB payload and enrollment
regressions pass. Update views were rendered at 820×600 and 1024×760.

The packaged Fedora panel passed nested COSMIC checks for Standard, Together and
Separate arrangements, Penumbra on/off and autohide at 150% scaling. This used
software rendering in an isolated Fedora container; it did not alter the host
desktop. Full Fedora login, enforcing SELinux, installer/PolicyKit prompts and
physical GPU/display acceptance remain release checks.

Release acceptance checks the final signed RPM and repository, fresh installation
and upgrade, corresponding source and nested native rendering. The RPM publication
gate is separate from the retained historical DEB approvals; each platform binds
its own executable and package bytes.
