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

### Container stage

`scripts/rpm/container_acceptance.py` repeats the package checks in disposable
`registry.fedoraproject.org/fedora:43` and `:44` containers. It needs git, cargo,
network access and podman or docker usable without sudo:

```sh
python3 scripts/rpm/container_acceptance.py run --previous 0.6.1 --out ../fb25-acceptance
python3 scripts/rpm/container_acceptance.py run --previous 0.6.1 --out ../fb25-acceptance --dry-run
```

It tests the committed tree only. It vendors the dependencies with
`packaging/source_closure.py`, builds the application in each container with
Rust 1.93.0, locked and offline with an empty Cargo home, and reuses the previous
release's Fedora panel only while the panel source and startup policies are
unchanged. `build-rpm.py` packages the result with the official public key
embedded; a throwaway key generated in the container signs the package and the
local repository. Each target then runs, against that repository:

- fresh install, reinstall, removal and reinstall, checking EVR, `rpm -V`, both
  executable hashes and the enrollment receipt;
- upgrade from the previous released RPM, with the enrolled source untouched and
  with it disabled by the administrator, keeping the source and user files;
- rejection of a tampered package, tampered repository metadata and an
  unavailable repository.

Results are in `acceptance.json`, with `published: false`, and in per-stage logs.
Never upload or publish anything from the output folder. Delete it afterwards.
With rootful docker, pass `--engine "sudo docker"`; the script hands the output
files back to the calling user.

Last run, 2026-11-08, commit `2bcaed9ef` (main after the iced fork, weather
scenes and this fix), upgrading from the published 0.6.1 RPMs with rootful
docker 29.1.3: Fedora 43 and Fedora 44 each passed all 62 checks (build 9,
fresh install 28, upgrade 12, upgrade with the source disabled 13).

### Checks only a real Fedora COSMIC install can answer

Containers have no session, SELinux policy, PolicyKit agent or GPU. Before a
release, run these on Fedora 43 and Fedora 44 COSMIC, installed on hardware,
using the final signed RPMs. Record the Fedora version, COSMIC version, GPU,
driver and monitors with the results.

1. **Exact artifacts.** Checksums match `SHA256SUMS`, and `rpm -K` passes after
   importing only the official key. `/usr/share/doc/cosmic-ext-halcyon/fedora-build.json`
   matches the release provenance for that Fedora version.
2. **SELinux enforcing.** `getenforce` prints `Enforcing` during every step. After
   the steps, `sudo ausearch -m AVC,USER_AVC -ts boot` lists nothing for
   `halcyon`, `cosmic-panel-islands`, `rpm-enrollment.py` or `refresh.py`, and
   `sudo restorecon -nvR /usr/bin/halcyon /usr/lib/cosmic-ext-halcyon* /etc/yum.repos.d /etc/pki/rpm-gpg /var/lib/halcyon`
   prints nothing.
3. **Installer and PolicyKit prompts.**
   - Opening the downloaded RPM installs it after one administrator prompt; note
     any untrusted-package warning.
   - The first update from the Halcyon source asks to import the key with
     fingerprint `A1EB 6576 D959 5ECA 5750 9CE1 4EB2 B42D F234 CB16`. Declining
     it installs nothing.
   - **Open system updates** opens COSMIC Store, and Updates offers the new
     build. Without Store, the copied `sudo dnf upgrade --refresh cosmic-ext-halcyon`
     works.
   - Removing and reinstalling the package leaves the source off.
4. **Full login.**
   - After first run, `systemctl --user is-enabled halcyon.service` and
     `is-active` both succeed, and widgets come back after logging out and in.
   - Turning Islands and Penumbra on and off in Settings takes effect at the next
     login and keeps panel layouts.
   - Upgrading with Islands enabled keeps it enabled after the next login.
   - `journalctl --user -b` shows no Halcyon or panel errors.
5. **GPU and display.**
   - Widgets render with the hardware driver, not llvmpipe.
   - Mixed scaling and two monitors, including unplugging and replugging one.
   - Suspend, resume and lock keep widgets and the Islands panel in place.
   - Optional: note `drm-total-vram` for the Halcyon process from
     `/proc/<pid>/fdinfo` with 12 widgets.
