# Halcyon installation and updates: investigation and plan

Reviewed 2026-10-26 against `main` at `306b5a6ef1168bdd654fac440e15eaf861d040b9`, the public 0.3.0 release, its signed APT index, and retained packages matching those public hashes. The initial audit made no product or host changes. The user subsequently requested implementation; the local implementation and validation are recorded below under [Implementation](#implementation-2026-10-26). Nothing has been released or installed on the host. The future-builder work and existing local wizard changes are separate.

**Recommendation:** keep one visible download, the main Halcyon `.deb`. Open it with COSMIC Store, install through the OS dialog, then open Halcyon from Applications. Retain the main package's existing offline signed-source enrollment. Offer Islands inside Halcyon as an optional reviewed release installation, followed by explicit per-user activation for the next login. Keep the packages separate. This removes the second manual download and terminal activation from the normal optional-component journey without making Islands mandatory.

The base route already needs only one package download. The immediate problems are stale guidance, terminal-first instructions, an external Islands guide instead of an install action, and unclear ownership/status boundaries. A new general-purpose installer is unnecessary for this design.

## Evidence and confidence

- **Confirmed here:** the [release API](https://api.github.com/repos/profilaz3/halcyon/releases/latest) reports public, non-prerelease `v0.3.0`, published `2026-10-26T03:39:24Z`. There are ten attached assets plus GitHub's two source snapshots. The current raw public README matches the checkout; a search-engine rendering of the repository was stale and was not used as current status.
- **Confirmed here:** the public [APT InRelease](https://profilaz3.github.io/halcyon/apt/dists/noble/InRelease) verifies with the reviewed archive key, signing subkey `610B73343D6F40603553A295A4658CCECBC306AB`. Its SHA-256 binds the downloaded Packages index. Metadata expires `2026-11-02T10:06:24Z`. A private APT refresh verified and fetched 3,622 bytes of metadata successfully.
- **Confirmed here:** retained main and Islands `.deb` files match both public GitHub digests and the signed index. Main: 22,609,552 bytes, SHA-256 `a161840432580b8c48dd1ab2e45d0f54d6b75137f98aef50c60ca25a33820a32`. Islands: 6,017,798 bytes, SHA-256 `24cb50aefc64c08ebf1f41cb3d196769b1538e945269cc3e521b6152bead7c29`. Only control metadata was extracted; no large release downloads or builds were needed.
- **Confirmed here:** the main package's embedded postinst/postrm helper matches current `packaging/updates/defaults.py`. All 13 focused default-enrollment tests passed. Nine APT 2.8.3 resolver simulations and one private signed refresh establish the dependency behavior below. The simulations use synthetic already-satisfied base dependencies, not a complete fresh Pop!_OS image.
- **Source-traced:** application onboarding, PackageKit calls, source-install behavior, companion staging and runtime admission. The relevant committed paths did not differ between `v0.3.0` and HEAD. Uncommitted wizard fixes were excluded from the release claims.
- **Prior acceptance, not repeated here:** [0.3.0 acceptance](releases/0.3.0-acceptance.md) records installation of both final packages with dpkg into a populated offline COSMIC 1.11 guest, startup, toggles, crash fallback and restoration. That proves neither fresh online dependency acquisition nor a browser-to-Store installation. It does not qualify physical GPUs or a complete COSMIC 1.10 session.
- **Still untested:** actual fresh-machine MIME dispatch, Store dependency acquisition, exact authentication dialog count, Store discovery, and the proposed in-app Islands installation. These are release gates, not advertised guarantees.

Reproduction scripts, logs, exact public snapshots and extracted control files are in [the isolated evidence directory](../.task-install-experience-20261026/). The main entry points are `package-verification.json`, `public/signature-verification.txt`, `resolver-results.json`, `defaults-tests.log`, `bootstrap-purge-probe-logged.json`, and `primary/urls.json`.

## What a user encounters today

The README's prominent **Download 0.3.0** points directly at the correct main package for Pop!_OS 24.04/COSMIC on Intel or AMD x86-64. The release page repeats that direct link. Users who instead open the asset list must distinguish the main package from Islands, checksums, manifests, evidence and a 280 MB corresponding-source archive. ARM is not a supported binary download. No account, Rust toolchain or separate bootstrap is required for the normal main-package route.

The README then requires `sudo apt install ./cosmic-ext-halcyon_0.3.0_amd64.deb`. It assumes the terminal is already in the download directory. APT can acquire missing dependencies from configured, indexed distro repositories; the app package does not contain those libraries. An offline installation succeeds only if dependencies are installed or available locally.

The packaged launcher runs `halcyon --settings`. Setup has Welcome, Look, Dock & panel, Widgets, Arrange, optional Weather, and Ready screens, with skipping available. Look/bar changes use the existing guarded desktop operations. Finishing starts selected widgets using the user service, with sign-in startup initially selected; no administrator password is needed. Package installation itself does not start widgets or change a user's desktop. Weather and calendar setup are optional; usage-sharing choice belongs in onboarding, not package installation. See `src/settings/wizard.rs`, committed `src/settings.rs::open_wizard`/`Outcome::Done`, and `packaging/systemd/halcyon.service`.

The optional Islands route asks for another `.deb`, another `sudo apt install`, `halcyon-islands enable`, and logout/login. The app's absent-component action is **Get Islands ↗**, opening a guide (`src/settings/view/recovery.rs`, `src/settings.rs::OpenIslandsGuide`). It does not install a package. Dock styling and companion installation/activation are distinct operations.

Counts below exclude optional accounts, incidental browser clicks and downloads made automatically by APT. A transaction is an OS package installation; dependency package counts depend on the machine. Password counts cannot be invariant because sudo caching, PolicyKit, account privileges and administrator policy differ.

| Current route | Downloads the person must select | Package transactions / Halcyon packages | Terminal commands | Privilege and login |
|---|---:|---|---:|---|
| README main install | 1 | 1 / main, plus missing distro dependencies | 1, plus directory navigation if needed | 1 privileged install; normally one password prompt |
| README main + Islands | 2 | 2 / main and Islands, plus dependencies | 3: two installs and enable | 2 privileged installs; normally 1–2 sudo password prompts; one logout/login |
| Both local packages supplied to one APT command | 2 | 1 / both | 2: combined install and enable | 1 privileged install; one logout/login |
| Older manual installation: documented bootstrap enrollment only | 2 | 1 / keyring and source | 5 logical commands: two curl calls, checksum pipeline, install, refresh | 2 privileged operations; usually one cached sudo authorization |
| Existing app's eligible GUI enrollment | 0 manual; app downloads 2 bootstrap files | 1 / keyring and source | 0; fallback offers install + refresh | OS authorization for local files; actual dialog count unmeasured |
| Source route on a prepared developer machine | Checkout/dependencies, not release assets | 0 application package transactions | Documented build, user install, start: 3 | No root for build/user install; toolchain/OS dependency setup is additional |

Installing main first, then adding Islands from APT avoids the second manual download **today**, but requires a metadata refresh after enrollment before `apt install cosmic-ext-halcyon-islands` can reliably find it. Main installation, refresh, add-on installation and enable are four shell commands; two are install transactions. This is an advanced fallback, not the proposed default UI.

The linked [distribution guide](DISTRIBUTION.md) is publicly stale: it says 0.3.0 is upcoming/local/unpublished and describes APT as serving exactly 0.2.5. `packaging/README.md` also says upcoming and incorrectly describes Islands as restricted to one exact panel version. These statements contradict current release/package evidence. The bootstrap's own `0.2.5-1` version remains valid published metadata; its age is not evidence that the 0.3.0 app channel is unpublished.

Developer guidance also has obsolete binary/service names and an unsupported `scripts/install.py --restart` option. Its broad opening rename warning is insufficient for copyable installation commands.

## What the packages actually do

The main 0.3.0 control declares `Depends: libc6 (>= 2.39), libgcc-s1 (>= 12), libxkbcommon0 (>= 1.0.0), python3, git, systemd` and `Recommends: cosmic-session`. It has no dependency or recommendation on Islands or the bootstrap packages. Islands declares native libraries, Python, main Halcyon, `cosmic-panel`, `cosmic-comp` and `cosmic-session`; its dependency on main has no version bound. Installing Islands cannot be used to obtain the main package on an unconfigured machine unless both local files are supplied or the repository is already indexed.

`packaging/build-deb.py::update_defaults` embeds three reviewed templates and a helper in main. Its postinst, offline and within the original package transaction, writes key and restrictive pins before publishing the source. It records `/var/lib/halcyon/updates/default-source.json`. It never runs nested APT/dpkg, refreshes metadata, enables unattended upgrades, activates Islands or edits home directories.

| Installation condition | Current enrollment behavior |
|---|---|
| Fresh main `.deb`, no previous artifacts | Generates source/key/pins and a complete receipt; no extra package or authorization |
| Upgrade from manual 0.2.5 with no enrollment history | Same one-time enrollment |
| Existing bootstrap packages or residual dpkg records | Preserves their ownership; no second source |
| Existing edited, disabled, renamed or removed source evidence | Preserves the decision; does not silently recreate it |
| Interrupted enrollment, matching pending files | Resumes missing writes; source published last |
| Pending receipt plus external edits/renaming | Preserves or reports conflict rather than overwriting |
| Islands package alone | Does not enroll; main is a dependency |
| `scripts/install.py` user install | Does not enroll, write `/usr`, or acquire system dependencies |
| Locally built `.deb` via current `packaging/build-deb.py` | Always embeds default enrollment, regardless of whether the binary is a reviewed release |

The last row is a real design gap. An arbitrary local package can receive release-channel behavior simply by using the generic builder. Separately, update identity only recognizes the exact build-output executable as `Source`; a copy installed in `~/.local/bin` normally becomes `Unknown`, though it remains barred from enrollment. A local source launcher and user service can also shadow the release's launcher/service after a system install. Treat migration as an explicit ownership change, not just another download. See `scripts/install.py`, `build.rs`, and `src/settings/updates/identity.rs::measure`.

Update status verifies the running executable, dpkg ownership/checksums, source contract and receipt. A renamed/custom source may work in APT while Halcyon conservatively reports that it cannot verify enrollment. Do not turn that status into an automatic repair. `Check for updates` explicitly runs PackageKit `RefreshCache` and `GetUpdates`; it does not install updates. The upgrade button opens COSMIC Store and tells the user to choose Updates. Successful bootstrap installation only rechecks enrollment; it does not itself perform the subsequent metadata refresh.

The main package's tested lifecycle fixes preserve active renamed-source trust during purge, avoid recreating administrator-disabled sources after interrupted enrollment, publish atomically and preserve removal decisions on reinstall. Removing its unchanged generated source retains an ignored recovery copy. Purge removes unchanged key/pins only when active sources do not need them; the recovery copy can still prevent reenrollment after purge/reinstall. If Python is unavailable, removal keeps configuration rather than breaking trust. These invariants must survive any changes.

**Legacy bootstrap exception, reproduced:** the public `halcyon-archive-keyring_0.2.5-1_all.deb` owns its key directly and has no maintainer-script reference guard. Installing that scriptless package in a private dpkg root, adding a renamed active source referencing its key, then purging it leaves the source but deletes the key. The separate source package's postrm moves only its canonical source on remove; it does not make keyring purge reference-aware. The main helper's fixes do not fix this separately owned package. Broad instructions to purge both bootstrap packages therefore need qualification and a new reviewed lifecycle fix. No immutable public artifact should be replaced.

## Why a dependency alone does not solve installation

The [APT source manual](https://manpages.debian.org/bookworm/apt/sources.list.5.en.html) describes metadata acquisition as a separate update operation. [Debian's maintainer-script contract](https://www.debian.org/doc/debian-policy/ch-maintainerscripts.html) puts postinst after unpacking and dependency processing. A new source written during postinst cannot supply a dependency that prevented the transaction from starting.

Isolated APT 2.8.3 results, with a synthetic satisfied COSMIC base:

| Resolver input | Result |
|---|---|
| Real local main; no Halcyon source/index | Plans main only |
| Real local Islands alone; main absent; no index | Fails: main unavailable |
| Both real local packages; no index | Plans both |
| Tiny test package depending on main + Islands, plus local main; no index | Fails: Islands unavailable |
| Same relationship with Islands only recommended | Succeeds without Islands |
| Source file exists, index still absent | Hard dependency still fails |
| After private signed refresh | Plans both from the verified archive/local file |
| Empty base-dependency state; only Halcyon archive indexed | Main fails with unavailable distro dependencies |

These are solver results, not installed-system or GUI acceptance. The fixtures deliberately contain no distro repository indexes, so they demonstrate failure when required packages are unavailable, not whether a particular fresh Pop!_OS image already supplies every dependency.

COSMIC Store has native local-file support: its current desktop entry registers Debian MIME types and its PackageKit backend calls `GetDetailsLocal`/`InstallFiles`, using trusted `InstallPackages` for repository installs. This is confirmed by [upstream source](https://github.com/pop-os/cosmic-store/blob/master/src/backend/packagekit.rs) and the [maintainer's response on local installs](https://github.com/pop-os/cosmic-store/issues/624); exact source snapshots are retained. PackageKit resolves available dependencies, but cannot obtain packages from an unknown/unindexed source. It also cannot guarantee a fixed password count across distro policies. [Upstream PolicyKit rules](https://github.com/PackageKit/PackageKit/blob/main/data/policy/org.freedesktop.packagekit.policy.in) distinguish local untrusted installation, signed repository installation and metadata refresh. Implement against supported Pop!_OS's API: current PackageKit upstream also has a newer local-file descriptor API; do not blindly substitute it for Noble's pathname API.

A downloaded `.deb` is not automatically authenticated as an archive package merely because it contains a public key. [APT authentication](https://manpages.debian.org/bookworm/apt/apt-secure.8.en.html) verifies signed repository metadata and package hashes, not a standalone `.deb` signature. Preserve the release download's HTTPS/checksum provenance and native local-package authorization. Repository Islands installation must retain signed-index verification, expiry checks and trusted-only installation. Do not silence the native local-file trust dialog or add `trusted=yes`/unauthenticated fallbacks.

| Option | Fresh-machine consequence | Decision |
|---|---|---|
| Add main `Depends: Islands` | Fails without an index; makes add-on mandatory | Reject |
| Add main `Recommends: Islands` | May be skipped initially, installed later according to policy; inconsistent optionality | Reject |
| Download a small repository/bootstrap `.deb` first | Still needs refresh and a second install operation to obtain main | Keep only for advanced administration/recovery |
| ZIP containing two `.deb`s | One archive download but extraction and package selection remain; Store does not treat ZIP as a transaction | Optional offline distribution, not default |
| Put companion payload directly into main | Can reach one install authorization, but everyone receives it; needs file-ownership migration, updated manifests/admission and coupled native releases | Defer; unnecessary for the chosen route |
| Installer package invoking APT/dpkg in postinst | Nested transactions/locking and unsafe recovery semantics | Reject |
| Separate setup application doing sequential OS transactions | Can orchestrate steps, but introduces another executable/package and still cannot promise one authorization | More work than using Halcyon's existing UI |
| Main `.deb`, optional in-app repository install | One manual download; base works independently; optional OS transaction after signed refresh | Recommended |

## Recommended user-facing journey

1. **Download Halcyon for Pop!_OS 24.04 — Intel/AMD 64-bit.** One primary button links to the versioned main `.deb`. State beside it: system updates are registered during installation; Islands is optional. Put source, integrity files and other architectures under clearly named secondary links.
2. **Open with COSMIC Store → Install.** Let the OS explain required dependencies and request administrator authorization. The main package enrolls a clean machine during that same transaction. Existing administrator decisions remain effective. Keep an expandable fallback with `sudo apt install ~/Downloads/cosmic-ext-halcyon_<version>_amd64.deb`; no separate Rust, GnuPG or bootstrap install.
3. **Open Halcyon from Applications.** Do not depend on Store exposing an Open button. Offer the normal setup, with a quick path to a working standard-panel desktop; keep detailed widget arrangement and accounts skippable. Show measured update status on Ready/App settings. Do not present an extra enrollment prompt when enrollment is already complete.
4. **Optional: Add Islands.** A clear card explains the extra system package and next-login activation. Selecting Install Islands performs an explicit signed-source refresh, resolves a trusted candidate, previews the OS package changes and uses PackageKit to install it. After success and a fresh integrity/compatibility inspection, offer **Enable for me** with the next-login explanation. No terminal and no second file hunt. Incompatibility, decline or offline state leaves the base app usable. Previously disabled or source-managed companions are never enabled by installation alone.
5. **Future updates:** use COSMIC Store → Updates or the OS's chosen package manager. Halcyon's **Check for updates** checks; **Open system updates** hands off installation. “Receive updates through your system” means repository enrollment. Automatic installation remains an OS policy, not a Halcyon promise or default change. [System76 documents Store's update page](https://support.system76.com/support/pop-basics); [Ubuntu's unattended-upgrade documentation](https://ubuntu.com/server/docs/how-to/software/automatic-updates/) treats allowed origins and automatic installation as separate configuration.

Target counts: base = **one manual download, one install transaction, normally one OS authentication, zero terminal commands**. With Islands = still **one manual download**, one additional automatically downloaded application package and install transaction, normally one additional OS authentication, explicit per-user activation and one logout/login. A stricter policy may also authenticate refresh; cached authorization may reduce prompts. No claim of a universal one-password full installation.

If a source is disabled or customized, describe it and provide an administrator-guided path; do not enable it as a side effect of checking or installing Islands. If no compatible candidate is available, show “Islands is unavailable for this desktop” and retain Standard. Do not offer COSMIC downgrades or version holds. Installing an eligible package does not prove runtime readiness: retain native interface/applet checks, generation integrity, the failure latch and stock fallback.

## Staged implementation, in dependency order

**Stage 1 — correct the public route and record the GUI baseline.** Update README, `docs/DISTRIBUTION.md`, `packaging/README.md`, the Islands guide and current release-page instructions together. Remove unpublished/0.2.5-only channel claims; place old bootstrap enrollment under advanced recovery. Explain source registration, check and installation separately. Add the download-directory-safe fallback and describe local-file upgrade limitations; [Store issue 494](https://github.com/pop-os/cosmic-store/issues/494) is an upstream report, not a confirmed Halcyon reproduction. Run the fresh-image browser → Files → Store → launcher test before making the no-terminal route the primary promise. Retain APT as a supported fallback if that gate fails. No new installer or package migration is needed for this stage.

**Stage 2 — settle package ownership before extending automation.**

- In `packaging/build-deb.py`, make release-channel embedding an explicit build mode used only by reviewed release assembly/promotion. Ordinary developer builds must not silently emit the release package identity and enrollment. Prefer `scripts/install.py` for local work. If local `.deb` builds remain supported, use a distinct identity with explicit file conflicts against the release package, no release-package `Provides`, no automatic enrollment, and no accidental release upgrade candidate. A version suffix alone is insufficient.
- Make production shared-library dependency detection fail closed; the main builder currently falls back to only libc6/libxkbcommon when detection fails. Keep runtime dependencies resolvable from supported distro sources. Do not add Islands to main's Depends/Recommends. Assess optional `wl-clipboard` for the Notes copy feature and document toolchain requirements separately.
- Keep the existing main default-source owner and immutable templates while fixing the legacy bootstrap lifecycle. Design a versioned migration so one owner manages generated trust and retains keys/pins referenced by active administrator sources. Do not transfer conffile ownership casually or duplicate sources. A migration must account for old keyring unpack/removal, active renamed sources, bootstrap-to-main adoption and rollback before shipping. Publish a new bootstrap version and update pinned download metadata only after those tests; never mutate `0.2.5-1`.
- Define the tested minimum main/helper contract for Islands and encode an appropriate version dependency if needed. Keep COSMIC compatibility in the existing admission/catalog mechanism. Do not replace it with exact distro version Depends, downgrade requests or holds.
- Gate any key/template rotation on explicit owned-file migration; the current completed receipt intentionally prevents a generic configure rerun from overwriting later administrator choices.

Acceptance gate: fixture-root main/legacy/combined-owner lifecycle matrix passes, arbitrary local package builds cannot join release updates accidentally, and final package bytes remain tied to the release/native approval pipeline.

**Stage 3 — add the optional-component transaction in the existing UI.** Extend the update/PackageKit layer rather than creating a privileged daemon. Replace the absent **Get Islands** guide-only action with an install action plus secondary help. Use the same optional action from onboarding only after coordinating with the separate wizard owner.

Required transaction: inspect actual running package ownership → verify source state → explicit refresh → resolve allowlisted name/architecture/version/origin → simulate changes → reject unexpected removals, downgrades or broad system changes → trusted-only install → verify dpkg state and installed bundle → expose explicit per-user enable. Obtain fresh identity before and after, because a parallel OS update can change it. The UI must track the OS transaction after window closure/restart and re-inspect before retry, rather than start a duplicate install. Cancellation before handoff stops preparation; after handoff, display the OS-owned operation honestly. Never collect a password.

Use the existing guarded user helper for activation; no root script may write home directories or restart panels. Preserve installation, disabled, incompatible, staged-for-login, ready and failed states separately. Keep the source-build route from invoking release-package installation into a different owner. If PackageKit/Store is missing, offer explicit terminal instructions without silently installing another package manager.

Acceptance gate: base and Islands paths work with decline, cancel, timeout, offline/retry and concurrent package changes; no unauthenticated package fallback; existing disabled/source-owned installations remain untouched. The intended extra prompt and logout are measured on the supported image.

**Stage 4 — finish status, developer instructions and migration.** Add a readable first-launch summary: “System updates ready”, “Updates disabled by administrator”, “Setup incomplete” or “Local build — update from source”, based on evidence. Expose last successful check without calling absence of results “up to date” after errors. A preserved custom source needs accurate uncertainty, not a repair button that overwrites it.

Rewrite the source quick start around `cargo build --release --locked`, `python3 scripts/install.py`, and the installed Halcyon launcher. Explain compiler/system prerequisites and `CARGO_TARGET_DIR`; users should not install the application build toolchain for a release download. The installer already preserves current running/enabled service behavior during upgrade, so remove the unsupported `--restart` advice. Add a user-install receipt/manifest for accurate Source identity and guarded uninstall/migration. Moving from source to release must explicitly handle the user launcher, PATH and user unit that can shadow `/usr`; remove only proven owned files and preserve preferences/backups. Rebuild/reinstall remains the source update mechanism.

Acceptance gate: full route documentation matches the final tested artifact; upgrade and reinstall behavior is reproducible. Publish only through the existing separately authorized release process. This investigation authorizes none of that implementation or publishing.

## Acceptance cases for the implementation

| Case | Required outcome and evidence | Current evidence |
|---|---|---|
| Fresh supported Pop!_OS/COSMIC, online | One selected `.deb`; Store resolves missing distro dependencies; measured OS dialogs; main starts from launcher; no widgets/panel started by package script; complete source receipt; signed refresh works | Package/control/helper and isolated refresh confirmed; fresh GUI trial pending |
| Main-only setup | Standard dock/panel; setup can finish without Islands, account or developer toolchain; selected widgets start according to sign-in choice | Source-traced; prior native acceptance is narrower |
| Fresh compatible Islands choice | One automatic repository package fetch; explicit install authorization and separate per-user enable; verified generation, one normal logout/login; native readiness/fallback retained | Existing staging/activation source and prior 1.11 acceptance; new install UI pending |
| Upgrade current 0.3.0 or manual 0.2.5 | Preserve settings and disabled choices; enroll only a manual system package with no prior decision; no repeated bootstrap downloads for enrolled installs; warn to reopen a replaced running binary | Lifecycle tests/source tracing; Store upgrade trial pending |
| Dependencies unavailable, wrong architecture or unsupported OS | Explain missing dependency/platform; do not force dpkg or install arbitrary repos; no “installed/ready” success before configuration completes | Isolated missing-dependency solver failure confirmed |
| Offline/retry | Main works offline only with available dependencies; offline enrollment remains valid but check is unsuccessful; Islands can be deferred; retry reuses verified state without bypassing signature expiry | Default enrollment tests/source tracing; new UI pending |
| Interrupted main install/enrollment | Resume matching pending writes only, publish source last; modified/renamed files block repair; distinguish incomplete dpkg transaction from usable app | 13 focused tests passed |
| Interrupted optional PackageKit install | Reconnect/recheck OS transaction and dpkg status; no duplicate install, lost cancellation or false success; retain existing panel | Proposed acceptance; not run |
| Disabled/deleted/renamed/duplicate sources | Never add a shadow source or silently reenable; APT may work with custom sources even when app cannot verify; explicit administrator recovery only | Main fixtures pass; status source-traced |
| Incompatible, absent, damaged or source-managed Islands | Base remains usable; no downgrade/hold; no activation of unowned selectors, modified generations or a failure-latched companion | Existing contracts/source and prior release fallback; no new native trial |
| Upgrade enabled/disabled Islands | Disabled remains disabled; owned enabled installations stage installed approved bytes; report next login only after staging; legacy 0.2.0 migration may need another login | Source-traced; documented existing behavior |
| Source build and user install | No APT writes or release replacement; accurate Source identity; preserve compiler/developer ownership and running-service choice | Installer source traced; copied-binary identity gap identified |
| Local development `.deb` | Separate owner/channel; cannot join release updates merely by building or collide silently with release | Current builder gap; Stage 2 gate |
| Remove, purge, reinstall main | User data retained; unchanged generated source disabled; active renamed source keeps its trust; reinstall preserves disabled/recovery state; absent Python is safe | Main lifecycle fixtures pass |
| Remove/purge legacy bootstrap | Every still-active source retains required key/pins; no unrelated file edits; migration and rollback preserve source ownership | **Current keyring purge fails renamed-reference probe; repair required** |
| Remove Islands / main dependency removal | Explain package-manager dependency removals; disable/restore owned user selector before uninstall where requested; retained generation records permit guarded restoration; no root home cleanup | Existing removal design source-traced; UI guidance needed |
| Reinstallation after purge and after source-to-release migration | Honor retained admin decisions; explicit reenrollment only; no shadow user service/launcher; preserved settings | Main fixture coverage partial; combined migration trial pending |

The remaining release qualification should use one disposable supported image with real distro indexes, PackageKit, PolicyKit and COSMIC Store, recording versions, dialogs, download counts and native session outcomes. Existing offline guest results and these small probes should be reused as evidence, not presented as that missing end-to-end test.


## Implementation (2026-10-26)

The user authorized the easiest implementation after this investigation. The local
change keeps the single visible main download and uses the existing verified
release-file installer for optional Islands. This deliberately replaces the
proposed repository-candidate resolver with a smaller first-install path: a
reviewed immutable companion pin, then PackageKit `InstallFiles`. Main and Islands
remain separate packages. Future updates continue through the existing signed APT
source. Default enrollment code and the published 0.3.0 assets are unchanged.

### User journey implemented for the next app build

1. Download the main app, open it with COSMIC Store, authorize installation if
   required, and open Halcyon. The existing main package enrolls a clean system
   offline; the OS resolves any ordinary missing dependencies.
2. Finish the existing onboarding. Widgets and themes work without Islands.
3. Optionally open **App settings → Updates → Get Islands…**. The app downloads
   the reviewed companion, verifies its exact size/hash, and simulates the
   proposed install. **Install Islands** hands the verified file to the OS.
4. Choose **Switch at next sign-in**. The existing per-user helper checks ownership,
   failed state, native qualification and activation conditions under its lock.
   Log out and back in. Disabled packages intentionally report compatibility as
   unknown; the button requests the authoritative check instead of inventing an
   admission result. Explicit incompatibility or recovery warnings block it.
5. Use **Check for updates** for discovery and **COSMIC Store → Updates** for
   installation. OS policy still controls any automatic installation.

Base journey: one manual download, one package transaction, normally one OS
authorization prompt, zero terminal steps. Optional Islands: one additional
background payload download and package transaction, normally one further OS
prompt, a separate unprivileged activation click, and next-login activation.
Actual prompts depend on OS authorization policy. No extra repository refresh or
bootstrap download is needed for the pinned companion's first install.

### Boundaries and migration

- `packaging/islands/download.json` binds the companion to a reviewed main-version
  context and immutable release asset. Release assembly must update this pin when
  necessary. A stale/malformed pin fails closed. This is application-pinned local
  file trust, not a claim that a standalone `.deb` is APT-signed.
- PackageKit simulation accepts exactly the pinned companion installation, plus
  its local unsigned-file annotation. Other dependency changes, removals,
  upgrades, downgrades or ambiguous results lead to the OS package-manager guide.
  The supported COSMIC desktop already contains its native dependencies. The
  simulation is repeated before installation; it is not an atomic reservation
  of system package state. A fresh-machine GUI trial remains required.
- The running release executable and dpkg ownership must agree. Existing manual
  companions and incomplete package installs are not overwritten. Offline,
  authorization, trust, timeout and busy outcomes remain explicit; unknown OS
  outcomes retain the verified files and require a recheck. Closing Settings does
  not cancel an installation already handed to the OS.
- Source/unknown installs cannot reach the new installer. `scripts/install.py`
  remains the per-user build route. The Debian builder now requires either
  `--release-updates` or `--development`. Development uses the distinct
  `cosmic-ext-halcyon-dev` identity with no enrollment hooks/templates and no
  release `Provides`. CI uses it. APT explicitly replaces release/dev packages;
  replacing the main release with development also proposes removal of Islands,
  whose dependency requires the real release package.
- Existing disabled, renamed or modified sources remain untouched. The current
  main default-enrollment receipt and reference-aware cleanup are unchanged and
  regression-tested. The published legacy keyring 0.2.5-1 purge gap is **not
  repaired by this change**. Distribution instructions now explain keeping its key
  while any active source references it, instead of suggesting blanket purge.
- README leads with graphical main installation and keeps an APT fallback. It
  distinguishes published 0.3.0 instructions from the new in-app action. Stale
  unpublished-release claims, exact-version Islands claims and source installer
  names/flags are corrected.

### Verification and remaining release gate

- Integrated Rust 1.93.0 release test build passed, using the single shared target
  through the future-builder thread. `settings::updates::`: **32 passed, 0 failed,
  1 existing network-download test ignored**. New coverage includes private-bus
  simulation/trust/authorization/timeout scope, pin validation, incomplete dpkg
  states, source identity, cancellation, and native button actions.
- Debian packaging: **5 passed** using tiny stub packages; default source lifecycle:
  **13 passed**, including interrupted enrollment and disabled/renamed-source
  preservation. Four private APT simulations confirm release/dev replacement and
  Islands dependency behavior; none executed package installation or hooks.
- The local companion matches the public 6,017,798-byte asset and SHA-256 pin.
  The activation wrapper matches the exact approved package bytes.
- Native Settings captures at **820×600** and **1024×760** were inspected. Install
  confirmation and activation controls are visible and usable; the small window
  scrolls normally. Fixture app version and disabled Store button are test data.
- Still untested here: real privileged PackageKit/PolicyKit installation and a
  complete fresh supported Pop!_OS/COSMIC journey, OS dependency-repair dialogs,
  and end-to-end logout/relogin. The acceptance matrix above remains the release
  checklist. No live package/source/service/session/desktop changes, commits, PRs
  or publishing occurred in this implementation.

Evidence: `.task-install-implementation-20261026/README.md`, including copied final
build/test logs, source hashes and native captures. The future-builder implementation
owns its separate files and the coordinated build; this work did not edit
`src/settings.rs`, `src/settings/view/pages.rs`, wizard repairs or its tracker.

PackageKit behavior was checked against its [v1.2.8 APT backend](https://github.com/PackageKit/PackageKit/blob/v1.2.8/backends/apt/apt-job.cpp)
(`init`, `checkTrusted`, `checkChangedPackages`, `installPackages`) and
[InstallFiles dispatch](https://github.com/PackageKit/PackageKit/blob/v1.2.8/backends/apt/pk-backend-apt.cpp).


## Coordinated updates — architecture contract, 2026-10-27

Future UI work should present one observation of Halcyon and optional Islands,
using the existing identity checks, package manager and guarded activation helper.
The current fixes enforce supported package pairs; they do not implement a new
combined updater.

- Distinguish installed package versions, available candidates, the running main
  executable, the selected Islands generation and the generation actually running.
  An installed package is not proof that its enabled payload is staged or active.
- Keep absent Islands optional and preserve an explicitly disabled installation.
  Compare installed pairs against the release pin, including Debian revisions;
  do not assume the two version strings must always be equal.
- Prefer COSMIC Store/APT for the package operation. Recheck installed identities
  afterward, then offer guarded staging for an enabled companion and accurate
  restart/login status. Report a mismatched pair or partial failure beside the
  affected component. Do not report both components current from the main result.
- Retain the separate lifetime policies: PackageKit owns an installation after
  handoff; Halcyon owns and reaps its activation helper. Reconcile uncertain
  outcomes before offering another action.
- Add an in-app PackageKit upgrade transaction only if the Store handoff still
  causes measured usability problems. Reuse the existing transport and scoped
  simulation; no second installer, update service or ownership journal.

Acceptance must cover absent/disabled Islands, matching and unsupported pairs,
partial installs, an old running main executable, installed-but-unstaged Islands,
activation interruption and final active-generation confirmation. This is a
future contract, not installation or release qualification.
