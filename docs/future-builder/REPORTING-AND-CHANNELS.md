# Reporting and native update channels

Planned on 2026-10-30, after published 0.5.0. Release number is not assigned.
Tracked as FB25 and FB26 in [tracker.json](tracker.json).
RPM responds to [issue #17](https://github.com/profilaz3/halcyon/issues/17).
Implemented locally on 2026-10-30. The collector and private workbook are updated.
The app changes and RPM repository remain unpublished at the user’s request.
See [Fedora packaging](../FEDORA-PACKAGING.md) for validation and release limits.

## Outcome

Install one Halcyon package, then receive app, Islands and Penumbra updates
through the normal system updater. Preserve settings, reporting choice and
Islands activation. The reporting workbook should explain which versions and
channels are actually being used, with readable installation histories.

First RPM target: traditional Fedora COSMIC on x86-64. The Debian target remains
the currently supported Pop!_OS/COSMIC package environment. RPM does not imply
support for every RPM distribution, another desktop, or an atomic OS variant.
Flatpak remains a separate feasibility task.

## Baseline before this work

- Published 0.5.0 already bundles the app and native panel host in one DEB.
  Existing split installations receive an empty migration package.
- Clean installs enroll in signed APT once. Later disabled, removed or modified
  sources are preserved. Package installation is owned by the OS.
- Settings checks PackageKit and opens COSMIC Store. It does not install updates
  unattended. Installed identity, version comparison and native compatibility
  still depend on dpkg/APT.
- The live reporting reader has Overview, Activity, Versions and History.
  Schema 4 already supplies app version and optional first desktop start.
  The collector retains observed activity and version history for 90 days.
  Package type, repository channel and installed app version are not reported.

Sources: [distribution](../DISTRIBUTION.md), [coordinated updates](COORDINATED-UPDATES.md),
[reporting protocol and views](../../scripts/usage/README.md),
[privacy](../PRIVACY.md), [0.5.0 verification](../releases/0.5.0-acceptance.md).

## FB25 — one upgrade experience

### User flow

1. Download one DEB or RPM for the supported system. Open it in the system
   installer, which handles dependencies and authorization. Include the update
   source and public key in this package; no separate Islands or bootstrap
   download on a clean supported installation. Make the source enrollment clear.
2. Future releases appear in the system updater. Halcyon provides one clear
   **Open system updates** action, ideally targeting the updater's Updates view.
   Verify the destination on each supported desktop; a button must not claim to
   install an update when it only opens a store.
3. After installation, distinguish the installed version from running processes.
   Offer **Reopen Halcyon** only with a safe handoff that preserves pending edits.
   Otherwise explain the required reopen without discarding work.
4. An enabled, replaced panel host takes effect at the next login. Show that
   notice only when measured host state needs it. Disabled Islands stays disabled;
   an ordinary app update does not produce a blanket logout request.

Use the same short states on both channels: checking, up to date, update
available, installed/reopen, and panel ready for next login. Offline, unavailable
package service, disabled source and repair-needed states must have distinct
actions. A failed check must never become “up to date.” Preserve a usable app
while offline. Keep technical details behind disclosure.

The OS retains its authorization, scheduling and reboot policies. Halcyon must
not force logout, replace a running panel, or enable unattended upgrades.
When a GUI updater is unavailable, provide one copyable native command for the
verified installation; do not silently enable a disabled source.

### Implementation boundary

- Extract only the shared read-only installation facts needed by Settings,
  compatibility and reporting. Keep explicit Debian and RPM implementations;
  no new updater daemon or general plugin framework.
- Keep product version, native package version and architecture separate.
  Debian ordering and RPM epoch/version/release ordering must use the respective
  native rules. An RPM release suffix is not an application-version mismatch.
- Verify ownership of the running executable, included host, helper and manifest
  with the appropriate package backend. Unknown evidence stays unknown. Preserve
  generation fences and revalidation across package transactions.
- Build a native RPM from the reviewed source, including licenses, AppStream,
  desktop entries and user-service integration. Resolve Fedora dependencies and
  qualify the native host against the Fedora COSMIC build. Do not convert the
  DEB or treat its Ubuntu ELF qualification as Fedora evidence.
- Publish a signed DNF repository with package and metadata verification and
  package-name restrictions. Retain existing APT signing and renewal. Bind each
  release manifest to the final signed package bytes and matching source commit.
  Each channel needs its own supported-system and native-host evidence.
- Enroll once using local packaged material. Preserve disabled, deleted,
  renamed and administrator-edited repository configuration through upgrades,
  reinstall and interrupted enrollment. RPM config-file preservation alone does
  not demonstrate all these cases. Never fetch scripts or bypass signature
  checks from a package scriptlet.
- Verify the initial trust/key prompt in the actual Fedora installer and DNF
  backend. Do not promise a prompt-free install or import arbitrary keys merely
  to remove a prompt. Test key rotation and repository failure recovery.
- Keep the existing DEB bridge migration. Both channels carry app and panel
  together, retain per-user activation/layout and leave user home directories
  untouched by package scripts.

Official references checked during planning: [DNF repository configuration](https://dnf.readthedocs.io/en/latest/conf_ref.html),
[RPM config-file behavior](https://rpm.org/docs/4.20.x/manual/spec.html),
[RPM signing](https://rpm.org/docs/6.1.x/man/rpmsign.1), and
[COSMIC Store's PackageKit backend](https://github.com/pop-os/cosmic-store/blob/master/src/backend/packagekit.rs).
These describe mechanisms; local and native acceptance must establish Halcyon's
actual behavior. A store build may not include every backend.

## FB26 — readable reporting of versions and upgrades

Keep the four reader tabs. Extend their existing data instead of adding more
top-level dashboards or feature-use tracking.

- **Overview:** retain active/returning counts; add a compact channel breakdown
  and coverage for the new fields. Show the latest receipt and distinguish
  missing reports from zero activity. No fabricated live values in mockups.
- **Activity:** retain 7/30/90-day reported activity. Allow channel filtering
  where daily channel observations exist; old days remain unclassified. Never
  apply today's channel retrospectively to all of an installation's history.
- **Versions:** add DEB/RPM/manual/unknown filters and counts of observed changes
  between app versions. Keep current adoption separate from changes observed
  during the selected period. Display an explicit eligible population and period.
- **History:** show a compact timeline of first observed report, version changes,
  last report and channel changes. Distinguish running from installed app version
  when both are known. Keep first desktop start separate from first seen.

Schema 5 adds only three bounded activity fields:

| Field | Meaning |
| --- | --- |
| `package_format` | `deb`, `rpm`, `source` or `unknown` |
| `update_channel` | Verified official `apt` or `dnf`; otherwise `manual`, `disabled`, `source` or `unknown` |
| `installed_app_version` | Product version from verified installed metadata, or `null`; never a guessed conversion of an RPM release string |

Retain `version` as the version of the process sending the report. A report of
the new running version establishes observed adoption. A newer installed version
alone does not establish that the user has run it or that the panel has restarted.
Define valid field combinations and classify third-party/ambiguous ownership as
unknown rather than calling it the official channel.

These fields are disclosed in Privacy, Settings and the protocol documentation.
Old clients do not send them; the new client has not been released.
Keep the existing reporting choice and pseudonymous identity across upgrades.
Do not add widget usage, clicks, content, hardware IDs, repository URLs, logs,
package-manager history or an install-time network report.

Use the existing daily activity request and retry schedule. Read installation
facts within a bounded background operation; measurement failure must yield
unknown fields rather than blocking activity or the UI. Do not refresh package
repositories or perform repeated integrity scans merely to send reporting.
Opt-out/status stays the minimal schema 3 message and includes none of these
fields. Preserve local immediate opt-out and cancellation fences.

### Statistical limits and rollout

- Counts are reporting installations, not people, downloads or exact installs.
  No report cannot establish an uninstall, failed upgrade or loss of interest.
- Version changes are observed on reporting dates. Same-day intermediate versions
  and offline changes can be missing. Do not label the timeline an exact package
  transaction log or infer a precise upgrade-success rate.
- Use unknown categories for schema 1–4 clients and insufficient history. New
  history starts with actual observations; no backfill from current values.
- Keep the existing 90-day retention and current enabled-population rules.
  Each generated view must disclose its denominator and incomplete coverage.
- A “behind current release” metric needs a verified release catalog for that
  channel and supported target. Do not compare every RPM/DEB client to the latest
  GitHub tag. Defer that metric until eligibility is known.
- Migrate the collector additively on a private copy first, then deploy support
  for schemas 1–5 before shipping schema 5 clients. Keep the endpoint stable,
  preserve all consent revisions and ownership checks, and prove repeated setup
  and interrupted writes. Retain minimal status handling throughout rollback.
- Patch exact owned reader ranges. Preserve current history, formulas and private
  sharing. Verify native formulas, charts, filters and empty/unknown states.

## Delivery order and acceptance

1. **Installation facts:** implement the small package-backend boundary, keep
   Debian behavior intact, and prove RPM version/ownership rules with fixtures.
2. **RPM and channel:** build and inspect the Fedora package, then rehearse
   signed-repository enrollment and upgrade in isolation. Review and fix before
   moving on; no host desktop/package changes as a substitute for a test target.
3. **Reporting:** finalize the bounded schema, collector migration and readable
   views; then integrate the daily client observation. Deploy collector support
   before releasing clients. Show field coverage during mixed-version rollout.
4. **Native UX:** validate the shared update states and Store handoff at 820×600
   and wide sizes on both supported systems. Inspect actual renders after changes.
5. **Release:** qualify the exact signed artifacts, sources and metadata. Retain
   private validation evidence; publish only support claims demonstrated there.

Acceptance includes fresh install; an older supported release to candidate;
candidate to a newer package; legacy split DEB migration; Islands on/off;
saved settings and reporting-off preservation; app running during upgrade;
disabled/deleted/renamed sources; interrupted enrollment; package-manager busy;
offline and signature failures; key rotation; removal/reinstall; and a supported
OS/COSMIC upgrade. On Fedora, exercise actual DNF/PackageKit and native COSMIC,
not just an RPM build. Include SELinux in its normal enforcing configuration.

Reporting checks include schema 1–5 compatibility, field validation, duplicates,
unknown classifications, channel changes, observed version changes, installed
versus running versions, opt-out/re-enable, retention and interrupted migration.
Use synthetic identities for live verification and remove only those fixtures.
Review each meaningful change and correct findings before advancing.
